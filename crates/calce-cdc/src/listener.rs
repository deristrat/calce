//! CDC listener: streams WAL changes from Postgres and emits typed events.

use std::collections::HashMap;
use std::time::Duration;

use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use chrono::NaiveDate;
use tokio::sync::mpsc;

use crate::error::CdcError;
use crate::protocol::{self, Lsn, PgOutputMessage, RelationInfo, TupleValue};
use crate::wire::{ConnParams, PgStream};
use crate::{CdcConfig, CdcEvent};

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
                Ok(()) => return,
                Err(CdcError::ChannelClosed) => return,
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

        let instrument_map = load_instrument_map(&mut stream).await?;
        tracing::info!(
            "CDC streaming: slot={}, pub={}, instruments={}",
            self.config.slot_name,
            self.config.publication_name,
            instrument_map.len(),
        );

        stream
            .start_replication(&self.config.slot_name, &self.config.publication_name, lsn)
            .await?;

        self.stream_loop(&mut stream, &instrument_map).await
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
                    "CREATE PUBLICATION {} FOR TABLE prices, fx_rates, trades, instruments",
                    self.config.publication_name,
                ))
                .await?;
            tracing::info!("Created publication '{}'", self.config.publication_name);
        }
        Ok(())
    }

    // -- Streaming loop ---------------------------------------------------------

    async fn stream_loop(
        &self,
        stream: &mut PgStream,
        instrument_map: &HashMap<i64, String>,
    ) -> Result<(), CdcError> {
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
                        self.handle_pgoutput(pgmsg, &mut schema_cache, instrument_map)
                            .await?;
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
        instrument_map: &HashMap<i64, String>,
    ) -> Result<(), CdcError> {
        match msg {
            PgOutputMessage::Relation(info) => {
                tracing::debug!("CDC relation: {} (oid={})", info.name, info.id);
                schema_cache.insert(info.id, info);
            }
            PgOutputMessage::Insert { relation_id, tuple } => {
                self.emit_event(relation_id, &tuple, schema_cache, instrument_map)
                    .await?;
            }
            PgOutputMessage::Update {
                relation_id,
                new_tuple,
            } => {
                self.emit_event(relation_id, &new_tuple, schema_cache, instrument_map)
                    .await?;
            }
            PgOutputMessage::Begin | PgOutputMessage::Commit | PgOutputMessage::Delete => {}
        }
        Ok(())
    }

    async fn emit_event(
        &self,
        relation_id: u32,
        tuple: &[TupleValue],
        schema_cache: &HashMap<u32, RelationInfo>,
        instrument_map: &HashMap<i64, String>,
    ) -> Result<(), CdcError> {
        if let Some(event) = map_to_event(relation_id, tuple, schema_cache, instrument_map) {
            tracing::debug!(?event, "CDC event");
            if self.event_tx.send(event).await.is_err() {
                return Err(CdcError::ChannelClosed);
            }
        }
        Ok(())
    }
}

// =============================================================================
// Event mapping: WAL row → CdcEvent
// =============================================================================

fn map_to_event(
    relation_id: u32,
    tuple: &[TupleValue],
    schema_cache: &HashMap<u32, RelationInfo>,
    instrument_map: &HashMap<i64, String>,
) -> Option<CdcEvent> {
    let relation = schema_cache.get(&relation_id)?;

    match relation.name.as_str() {
        "prices" => {
            let db_id: i64 = col_text(relation, tuple, "instrument_id")?.parse().ok()?;
            let ticker = instrument_map.get(&db_id)?;
            let date = col_date(relation, tuple, "price_date")?;
            let price = col_f64(relation, tuple, "price")?;
            Some(CdcEvent::PriceChanged {
                instrument_id: InstrumentId::new(ticker),
                date,
                price,
            })
        }
        "fx_rates" => {
            let from = col_text(relation, tuple, "from_currency")?;
            let to = col_text(relation, tuple, "to_currency")?;
            let date = col_date(relation, tuple, "rate_date")?;
            let rate = col_f64(relation, tuple, "rate")?;
            Some(CdcEvent::FxRateChanged {
                from_currency: Currency::new(from.trim()),
                to_currency: Currency::new(to.trim()),
                date,
                rate,
            })
        }
        _ => None,
    }
}

// -- Column value helpers -----------------------------------------------------

fn col_index(relation: &RelationInfo, name: &str) -> Option<usize> {
    relation.columns.iter().position(|c| c.name == name)
}

fn col_text<'a>(relation: &RelationInfo, tuple: &'a [TupleValue], col: &str) -> Option<&'a str> {
    let idx = col_index(relation, col)?;
    match tuple.get(idx)? {
        TupleValue::Text(s) => Some(s.as_str()),
        _ => None,
    }
}

fn col_f64(relation: &RelationInfo, tuple: &[TupleValue], col: &str) -> Option<f64> {
    col_text(relation, tuple, col)?.parse().ok()
}

fn col_date(relation: &RelationInfo, tuple: &[TupleValue], col: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(col_text(relation, tuple, col)?, "%Y-%m-%d").ok()
}

// -- Utility ------------------------------------------------------------------

/// Load instrument DB `id` → `ticker` mapping.
async fn load_instrument_map(stream: &mut PgStream) -> Result<HashMap<i64, String>, CdcError> {
    let rows = stream
        .simple_query("SELECT id, ticker FROM instruments")
        .await?;
    let mut map = HashMap::new();
    for row in &rows {
        if let (Some(Some(id_str)), Some(Some(ticker))) = (row.first(), row.get(1))
            && let Ok(id) = id_str.parse::<i64>()
        {
            map.insert(id, ticker.clone());
        }
    }
    Ok(map)
}

/// Parse a PostgreSQL LSN string like `"0/1A2B3C4D"` into a `u64`.
fn parse_lsn(s: &str) -> Lsn {
    let (hi, lo) = s.split_once('/').unwrap_or(("0", s));
    let hi = u64::from_str_radix(hi, 16).unwrap_or(0);
    let lo = u64::from_str_radix(lo, 16).unwrap_or(0);
    (hi << 32) | lo
}
