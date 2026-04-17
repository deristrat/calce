//! CDC listener: streams WAL changes from Postgres and emits events.

use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::error::CdcError;
use crate::protocol::{self, Lsn, PgOutputMessage, RelationInfo, TupleValue};
use crate::wire::{ConnParams, PgStream};
use crate::{CdcConfig, CdcEvent, CdcOperation, REPLICATED_TABLES};

/// Streams database changes and sends typed [`CdcEvent`]s on a channel.
pub struct CdcListener {
    config: CdcConfig,
    event_tx: mpsc::Sender<CdcEvent>,
}

impl CdcListener {
    /// Create a listener and its event receiver.
    ///
    /// `buffer_size` controls the bounded channel capacity. If the consumer is
    /// slow, the listener back-pressures (blocking WAL reads).
    #[must_use]
    pub fn new(config: CdcConfig, buffer_size: usize) -> (Self, mpsc::Receiver<CdcEvent>) {
        let (tx, rx) = mpsc::channel(buffer_size);
        (
            Self {
                config,
                event_tx: tx,
            },
            rx,
        )
    }

    /// Run the listener forever, reconnecting on failure.
    ///
    /// Returns only if the event channel is closed (all receivers dropped).
    pub async fn run(self) {
        let mut backoff = Duration::from_secs(1);
        loop {
            if self.event_tx.is_closed() {
                tracing::info!("CDC channel closed, stopping");
                return;
            }
            match self.run_once().await {
                Ok(()) | Err(CdcError::ChannelClosed) => return,
                Err(e) => {
                    tracing::warn!("CDC error: {e}, reconnecting in {backoff:?}");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(60));
                }
            }
        }
    }

    async fn run_once(&self) -> Result<(), CdcError> {
        let params = ConnParams::from_url(&self.config.database_url)?;
        let mut stream = PgStream::connect(&params).await?;
        tracing::info!("CDC connected to {}:{}", params.host, params.port);

        let lsn = self.ensure_slot(&mut stream).await?;
        self.ensure_publication(&mut stream).await?;

        tracing::info!(
            "CDC streaming: slot={}, pub={}",
            self.config.slot_name,
            self.config.publication_name,
        );

        stream
            .start_replication(&self.config.slot_name, &self.config.publication_name, lsn)
            .await?;

        self.stream_loop(&mut stream).await
    }

    // -- Setup ------------------------------------------------------------------

    async fn ensure_slot(&self, stream: &mut PgStream) -> Result<Lsn, CdcError> {
        let rows = stream
            .simple_query(&format!(
                "SELECT restart_lsn FROM pg_replication_slots \
                 WHERE slot_name = '{}'",
                self.config.slot_name,
            ))
            .await?;

        if let Some(row) = rows.first() {
            let lsn_str = row.first().and_then(Option::as_deref).unwrap_or("0/0");
            let lsn = parse_lsn(lsn_str);
            tracing::info!("Reusing slot '{}' at {lsn_str}", self.config.slot_name);
            Ok(lsn)
        } else {
            let rows = stream
                .simple_query(&format!(
                    "CREATE_REPLICATION_SLOT {} LOGICAL pgoutput",
                    self.config.slot_name,
                ))
                .await?;
            // CREATE_REPLICATION_SLOT returns (slot_name, consistent_point, ...)
            let lsn_str = rows
                .first()
                .and_then(|r| r.get(1))
                .and_then(Option::as_deref)
                .unwrap_or("0/0");
            let lsn = parse_lsn(lsn_str);
            tracing::info!("Created slot '{}' at {lsn_str}", self.config.slot_name);
            Ok(lsn)
        }
    }

    async fn ensure_publication(&self, stream: &mut PgStream) -> Result<(), CdcError> {
        let rows = stream
            .simple_query(&format!(
                "SELECT 1 FROM pg_publication WHERE pubname = '{}'",
                self.config.publication_name,
            ))
            .await?;

        if rows.is_empty() {
            stream
                .simple_query(&format!(
                    "CREATE PUBLICATION {} FOR TABLE {}",
                    self.config.publication_name,
                    REPLICATED_TABLES.join(", "),
                ))
                .await?;
            tracing::info!("Created publication '{}'", self.config.publication_name);
        } else {
            let table_rows = stream
                .simple_query(&format!(
                    "SELECT tablename FROM pg_publication_tables WHERE pubname = '{}'",
                    self.config.publication_name,
                ))
                .await?;
            let existing: Vec<&str> = table_rows
                .iter()
                .filter_map(|r| r.first().and_then(Option::as_deref))
                .collect();
            let missing: Vec<&str> = REPLICATED_TABLES
                .iter()
                .filter(|t| !existing.contains(*t))
                .copied()
                .collect();
            if !missing.is_empty() {
                let tables = missing.join(", ");
                stream
                    .simple_query(&format!(
                        "ALTER PUBLICATION {} ADD TABLE {}",
                        self.config.publication_name, tables,
                    ))
                    .await?;
                tracing::info!("Added tables to publication: {tables}");
            }
        }
        Ok(())
    }

    // -- Streaming loop ---------------------------------------------------------

    async fn stream_loop(&self, stream: &mut PgStream) -> Result<(), CdcError> {
        let mut schema_cache: HashMap<u32, RelationInfo> = HashMap::new();
        let mut last_lsn: Lsn = 0;
        let mut last_status = tokio::time::Instant::now();
        let status_interval = Duration::from_secs(10);

        loop {
            let data = stream.read_copy_data().await?;
            let msg = protocol::ReplicationMessage::parse(data)?;

            match msg {
                protocol::ReplicationMessage::XLogData { wal_end, data, .. } => {
                    last_lsn = wal_end;
                    if let Some(pgmsg) = PgOutputMessage::parse(&data)? {
                        self.handle_pgoutput(pgmsg, &mut schema_cache).await?;
                    }
                }
                protocol::ReplicationMessage::KeepAlive {
                    wal_end,
                    reply_requested,
                } => {
                    last_lsn = last_lsn.max(wal_end);
                    if reply_requested {
                        stream.send_status_update(last_lsn).await?;
                        last_status = tokio::time::Instant::now();
                    }
                }
            }

            // Periodic LSN confirmation so Postgres can release WAL segments
            if last_status.elapsed() >= status_interval && last_lsn > 0 {
                stream.send_status_update(last_lsn).await?;
                last_status = tokio::time::Instant::now();
            }
        }
    }

    async fn handle_pgoutput(
        &self,
        msg: PgOutputMessage,
        schema_cache: &mut HashMap<u32, RelationInfo>,
    ) -> Result<(), CdcError> {
        let (relation_id, tuple, operation) = match msg {
            PgOutputMessage::Relation(info) => {
                tracing::debug!("CDC relation: {} (oid={})", info.name, info.id);
                schema_cache.insert(info.id, info);
                return Ok(());
            }
            PgOutputMessage::Insert { relation_id, tuple } => {
                (relation_id, tuple, CdcOperation::Insert)
            }
            PgOutputMessage::Update {
                relation_id,
                new_tuple,
            } => (relation_id, new_tuple, CdcOperation::Update),
            PgOutputMessage::Delete {
                relation_id,
                key_tuple,
            } => (relation_id, key_tuple, CdcOperation::Delete),
            PgOutputMessage::Begin | PgOutputMessage::Commit => return Ok(()),
        };

        let Some(relation) = schema_cache.get(&relation_id) else {
            // Relation message not yet seen — skip.
            return Ok(());
        };

        let event = CdcEvent {
            table: relation.name.clone(),
            operation,
            columns: build_column_map(relation, &tuple),
        };
        tracing::debug!(?event, "CDC event");
        if self.event_tx.send(event).await.is_err() {
            return Err(CdcError::ChannelClosed);
        }
        Ok(())
    }
}

fn build_column_map(
    relation: &RelationInfo,
    tuple: &[TupleValue],
) -> HashMap<String, Option<String>> {
    relation
        .columns
        .iter()
        .zip(tuple.iter())
        .map(|(col, val)| {
            let v = match val {
                TupleValue::Text(s) => Some(s.clone()),
                _ => None,
            };
            (col.name.clone(), v)
        })
        .collect()
}

/// Parse a `PostgreSQL` LSN string like `"0/1A2B3C4D"` into a `u64`.
fn parse_lsn(s: &str) -> Lsn {
    let (hi, lo) = s.split_once('/').unwrap_or(("0", s));
    let hi = u64::from_str_radix(hi, 16).unwrap_or(0);
    let lo = u64::from_str_radix(lo, 16).unwrap_or(0);
    (hi << 32) | lo
}
