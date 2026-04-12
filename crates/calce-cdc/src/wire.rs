//! Low-level Postgres wire protocol client with replication support.
//!
//! Connects over raw TCP with `replication=database` in the startup message,
//! bypassing `tokio-postgres` (which doesn't expose replication mode). Uses
//! [`postgres_protocol`] for message encoding/decoding.

use bytes::{Buf, BufMut, BytesMut};
use postgres_protocol::authentication;
use postgres_protocol::message::{backend, frontend};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::error::CdcError;

/// Parsed database connection parameters.
pub(crate) struct ConnParams {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub database: String,
}

impl ConnParams {
    /// Parse a `postgres://user:pass@host:port/db` URL.
    ///
    /// # Errors
    ///
    /// Returns `CdcError::Config` if the URL format is invalid.
    pub(crate) fn from_url(url: &str) -> Result<Self, CdcError> {
        let stripped = url
            .strip_prefix("postgres://")
            .or_else(|| url.strip_prefix("postgresql://"))
            .ok_or_else(|| CdcError::Config("URL must start with postgres://".into()))?;

        let (userinfo, rest) = stripped
            .split_once('@')
            .ok_or_else(|| CdcError::Config("missing @ in URL".into()))?;

        let (user, password) = match userinfo.split_once(':') {
            Some((u, p)) => (u.to_string(), Some(p.to_string())),
            None => (userinfo.to_string(), None),
        };

        let (hostport, path) = rest
            .split_once('/')
            .ok_or_else(|| CdcError::Config("missing database in URL".into()))?;

        // Strip query parameters from database name
        let database = path.split('?').next().unwrap_or(path).to_string();

        let (host, port) = match hostport.split_once(':') {
            Some((h, p)) => {
                let port = p
                    .parse::<u16>()
                    .map_err(|_| CdcError::Config("invalid port".into()))?;
                (h.to_string(), port)
            }
            None => (hostport.to_string(), 5432),
        };

        Ok(Self {
            host,
            port,
            user,
            password,
            database,
        })
    }
}

/// A Postgres connection in logical replication mode.
///
/// Speaks the wire protocol over raw TCP with `replication=database` set in the
/// startup message. Supports simple queries and `CopyBoth` streaming for logical
/// replication.
pub(crate) struct PgStream {
    stream: TcpStream,
    read_buf: BytesMut,
}

impl PgStream {
    /// Connect, send startup message, and authenticate.
    ///
    /// # Errors
    ///
    /// Returns errors for TCP connection failures, auth failures, or protocol errors.
    pub(crate) async fn connect(params: &ConnParams) -> Result<Self, CdcError> {
        let addr = format!("{}:{}", params.host, params.port);
        let stream = TcpStream::connect(&addr).await?;
        let mut pg = Self {
            stream,
            read_buf: BytesMut::with_capacity(8192),
        };
        pg.startup(params).await?;
        Ok(pg)
    }

    /// Send startup message with `replication=database` and handle auth.
    async fn startup(&mut self, params: &ConnParams) -> Result<(), CdcError> {
        let mut buf = BytesMut::new();
        frontend::startup_message(
            [
                ("user", params.user.as_str()),
                ("database", params.database.as_str()),
                ("replication", "database"),
            ]
            .into_iter(),
            &mut buf,
        )
        .map_err(|e| CdcError::Protocol(format!("startup encode: {e}")))?;
        self.write_bytes(&buf).await?;

        loop {
            let msg = self.read_message().await?;
            match msg {
                backend::Message::AuthenticationOk => break,
                backend::Message::AuthenticationCleartextPassword => {
                    self.send_password(params.password.as_deref(), |pw| pw.as_bytes().to_vec())
                        .await?;
                }
                backend::Message::AuthenticationMd5Password(body) => {
                    let user = params.user.clone();
                    self.send_password(params.password.as_deref(), |pw| {
                        let hash =
                            authentication::md5_hash(user.as_bytes(), pw.as_bytes(), body.salt());
                        hash.into_bytes()
                    })
                    .await?;
                }
                backend::Message::AuthenticationSasl(body) => {
                    self.handle_sasl(params, &body).await?;
                }
                backend::Message::ErrorResponse(body) => {
                    return Err(extract_error(&body));
                }
                _ => {} // ParameterStatus, BackendKeyData — ignore during startup
            }
        }

        // Drain until ReadyForQuery
        loop {
            let msg = self.read_message().await?;
            match msg {
                backend::Message::ReadyForQuery(_) => return Ok(()),
                backend::Message::ErrorResponse(body) => return Err(extract_error(&body)),
                _ => {}
            }
        }
    }

    async fn send_password(
        &mut self,
        password: Option<&str>,
        hash_fn: impl FnOnce(&str) -> Vec<u8>,
    ) -> Result<(), CdcError> {
        let pw = password.ok_or_else(|| CdcError::Protocol("password required".into()))?;
        let hashed = hash_fn(pw);
        let mut buf = BytesMut::new();
        frontend::password_message(&hashed, &mut buf)
            .map_err(|e| CdcError::Protocol(e.to_string()))?;
        self.write_bytes(&buf).await
    }

    async fn handle_sasl(
        &mut self,
        params: &ConnParams,
        _body: &backend::AuthenticationSaslBody,
    ) -> Result<(), CdcError> {
        let pw = params
            .password
            .as_deref()
            .ok_or_else(|| CdcError::Protocol("password required for SCRAM".into()))?;

        let mut scram = authentication::sasl::ScramSha256::new(
            pw.as_bytes(),
            authentication::sasl::ChannelBinding::unsupported(),
        );

        // Step 1: SASLInitialResponse
        let mut buf = BytesMut::new();
        frontend::sasl_initial_response("SCRAM-SHA-256", scram.message(), &mut buf)
            .map_err(|e| CdcError::Protocol(e.to_string()))?;
        self.write_bytes(&buf).await?;

        // Step 2: SASLContinue
        let continue_data = match self.read_message().await? {
            backend::Message::AuthenticationSaslContinue(body) => body.data().to_vec(),
            backend::Message::ErrorResponse(body) => return Err(extract_error(&body)),
            _ => return Err(CdcError::Protocol("expected SASLContinue".into())),
        };
        scram
            .update(&continue_data)
            .map_err(|e| CdcError::Protocol(format!("SCRAM update: {e}")))?;

        // Step 3: SASLResponse
        buf.clear();
        frontend::sasl_response(scram.message(), &mut buf)
            .map_err(|e| CdcError::Protocol(e.to_string()))?;
        self.write_bytes(&buf).await?;

        // Step 4: SASLFinal
        match self.read_message().await? {
            backend::Message::AuthenticationSaslFinal(body) => {
                scram
                    .finish(body.data())
                    .map_err(|e| CdcError::Protocol(format!("SCRAM finish: {e}")))?;
            }
            backend::Message::ErrorResponse(body) => return Err(extract_error(&body)),
            _ => return Err(CdcError::Protocol("expected SASLFinal".into())),
        }

        Ok(())
    }

    // -- Simple query -----------------------------------------------------------

    /// Execute a simple query and collect result rows.
    ///
    /// Each row is a `Vec<Option<String>>` (one per column, `None` for NULL).
    ///
    /// # Errors
    ///
    /// Returns errors for query failures or protocol errors.
    pub(crate) async fn simple_query(&mut self, sql: &str) -> Result<Vec<Vec<Option<String>>>, CdcError> {
        let mut buf = BytesMut::new();
        frontend::query(sql, &mut buf).map_err(|e| CdcError::Protocol(e.to_string()))?;
        self.write_bytes(&buf).await?;

        let mut rows = Vec::new();
        loop {
            let msg = self.read_message().await?;
            match msg {
                backend::Message::DataRow(body) => {
                    rows.push(parse_data_row(&body));
                }
                backend::Message::ReadyForQuery(_) => return Ok(rows),
                backend::Message::ErrorResponse(body) => return Err(extract_error(&body)),
                _ => {}
            }
        }
    }

    // -- Replication streaming --------------------------------------------------

    /// Start logical replication. After this, use `read_copy_data` and
    /// `send_status_update` to stream changes.
    ///
    /// # Errors
    ///
    /// Returns errors if the slot or publication doesn't exist, or protocol errors.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    pub(crate) async fn start_replication(
        &mut self,
        slot: &str,
        publication: &str,
        lsn: u64,
    ) -> Result<(), CdcError> {
        let lsn_str = format!("{:X}/{:X}", lsn >> 32, lsn & 0xFFFF_FFFF);
        let sql = format!(
            "START_REPLICATION SLOT {slot} LOGICAL {lsn_str} \
             (proto_version '1', publication_names '{publication}')"
        );
        let mut buf = BytesMut::new();
        frontend::query(&sql, &mut buf).map_err(|e| CdcError::Protocol(e.to_string()))?;
        self.write_bytes(&buf).await?;

        // CopyBothResponse (tag 'W') isn't in postgres-protocol's Message enum,
        // so we parse the response tag manually.
        loop {
            self.fill_at_least(5).await?;
            let tag = self.read_buf[0];
            let len = i32::from_be_bytes([
                self.read_buf[1],
                self.read_buf[2],
                self.read_buf[3],
                self.read_buf[4],
            ]) as usize;
            self.fill_at_least(1 + len).await?;

            match tag {
                b'W' => {
                    // CopyBothResponse — now in streaming mode
                    self.read_buf.advance(1 + len);
                    return Ok(());
                }
                b'E' => {
                    // ErrorResponse — let postgres-protocol parse it
                    if let Ok(Some(backend::Message::ErrorResponse(body))) =
                        backend::Message::parse(&mut self.read_buf)
                    {
                        return Err(extract_error(&body));
                    }
                    return Err(CdcError::Protocol("start replication failed".into()));
                }
                _ => {
                    // Skip other messages (RowDescription, etc.)
                    self.read_buf.advance(1 + len);
                }
            }
        }
    }

    /// Read the next `CopyData` message from the replication stream.
    ///
    /// # Errors
    ///
    /// Returns `ConnectionLost` on `CopyDone`, or protocol errors.
    pub(crate) async fn read_copy_data(&mut self) -> Result<bytes::Bytes, CdcError> {
        loop {
            let msg = self.read_message().await?;
            match msg {
                backend::Message::CopyData(body) => {
                    return Ok(bytes::Bytes::copy_from_slice(body.data()));
                }
                backend::Message::CopyDone => return Err(CdcError::ConnectionLost),
                backend::Message::ErrorResponse(body) => return Err(extract_error(&body)),
                _ => {}
            }
        }
    }

    /// Send a standby status update to confirm the processed LSN.
    ///
    /// # Errors
    ///
    /// Returns IO errors.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub(crate) async fn send_status_update(&mut self, lsn: u64) -> Result<(), CdcError> {
        let payload = crate::protocol::build_status_update(lsn);
        // Build CopyData message manually: tag 'd', 4-byte length, payload
        let msg_len = (4 + payload.len()) as i32;
        let mut buf = BytesMut::with_capacity(1 + 4 + payload.len());
        buf.put_u8(b'd');
        buf.put_i32(msg_len);
        buf.put_slice(&payload);
        self.write_bytes(&buf).await
    }

    // -- Low-level IO -----------------------------------------------------------

    async fn read_message(&mut self) -> Result<backend::Message, CdcError> {
        loop {
            match backend::Message::parse(&mut self.read_buf) {
                Ok(Some(msg)) => return Ok(msg),
                Ok(None) => {
                    let n = self.stream.read_buf(&mut self.read_buf).await?;
                    if n == 0 {
                        return Err(CdcError::ConnectionLost);
                    }
                }
                Err(e) => return Err(CdcError::Protocol(format!("parse: {e}"))),
            }
        }
    }

    /// Ensure the read buffer has at least `n` bytes.
    async fn fill_at_least(&mut self, n: usize) -> Result<(), CdcError> {
        while self.read_buf.len() < n {
            let read = self.stream.read_buf(&mut self.read_buf).await?;
            if read == 0 {
                return Err(CdcError::ConnectionLost);
            }
        }
        Ok(())
    }

    async fn write_bytes(&mut self, buf: &[u8]) -> Result<(), CdcError> {
        self.stream.write_all(buf).await?;
        self.stream.flush().await?;
        Ok(())
    }
}

// -- Helpers ------------------------------------------------------------------

/// Parse column values from a `DataRowBody`.
fn parse_data_row(body: &backend::DataRowBody) -> Vec<Option<String>> {
    use fallible_iterator::FallibleIterator;

    let buf = body.buffer();
    let mut row = Vec::new();
    let mut ranges = body.ranges();
    while let Ok(Some(range)) = ranges.next() {
        match range {
            Some(r) => {
                let s = String::from_utf8_lossy(&buf[r]).into_owned();
                row.push(Some(s));
            }
            None => row.push(None),
        }
    }
    row
}

/// Extract the primary error message from a Postgres `ErrorResponse`.
fn extract_error(body: &backend::ErrorResponseBody) -> CdcError {
    use fallible_iterator::FallibleIterator;
    let mut message = String::from("postgres error");
    let mut fields = body.fields();
    while let Ok(Some(field)) = fields.next() {
        if field.type_() == b'M' {
            message = String::from_utf8_lossy(field.value_bytes()).into_owned();
            break;
        }
    }
    CdcError::Protocol(message)
}
