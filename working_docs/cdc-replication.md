# Postgres CDC for Live Cache Updates

## Overview

Currently calce loads all market data from Postgres into in-memory caches at startup (`loader::load_from_postgres`). After that, caches are only updated via the simulator API. When external processes write new prices or FX rates to Postgres, the API server doesn't know until restart.

**Goal:** A new crate (`calce-cdc`) that uses Postgres logical replication to stream changes from the database and apply them to the existing `ConcurrentMarketData` caches in real-time.

## Decisions

| Question | Decision |
|----------|----------|
| Date range extension | Only update current value via `set_current_price()` / `set_current_fx_rate()`. No array extension. |
| Scope | All tables: prices, FX rates, trades, instruments |
| Feature gating | On by default, controlled via `CALCE_CDC_ENABLED` env var |
| Replication slot | Persistent (survives restarts, retains WAL) |
| Library | `postgres-protocol` (published crate) + raw TCP. Evaluated Supabase ETL and MaterializeInc fork — both require git deps. Our approach uses `postgres-protocol` for wire format encoding/decoding and handles the replication startup (including `replication=database` in startup message) directly. |
| Architecture | CDC wired at calce-data level via `cdc::start_cdc()`, not calce-api |

## Progress

- [x] Resolve open questions
- [x] Library evaluation (Supabase ETL vs postgres-replication vs roll-our-own)
- [x] Architecture decision (data-layer, not API-layer)
- [x] Create `calce-cdc` crate scaffold
- [x] Wire protocol client (`wire.rs`): raw TCP, startup with `replication=database`, auth (cleartext/MD5/SCRAM), simple queries, CopyBoth streaming
- [x] pgoutput parser (`protocol.rs`): Relation, Insert, Update, Delete, Begin, Commit
- [x] Replication framing: XLogData / KeepAlive parsing, StandbyStatusUpdate
- [x] Typed event mapping: WAL rows → CdcEvent (prices, FX rates)
- [x] CdcListener (`listener.rs`): main loop with reconnect, LSN tracking, schema cache, instrument ID→ticker mapping
- [x] `calce-data/cdc.rs`: `start_cdc()` wires CDC events to `ConcurrentMarketData`
- [x] Wire into calce-api/main.rs
- [x] docker-compose: `wal_level=logical`
- [x] Invoke tasks: `cdc-status`, `cdc-drop-slot`
- [x] All 144 tests pass, clippy clean
- [ ] Wire CDC into calce-python (deferred — Python currently bulk-loads on init)
- [ ] Alembic migration for CREATE PUBLICATION (currently auto-created by listener)
- [ ] Integration test with live Postgres
- [ ] Create `DataService` in calce-data to own initialization + CDC lifecycle
- [ ] Wire CDC into calce-api via DataService
- [ ] Wire CDC into calce-python via DataService
- [ ] Database setup: Alembic migration for publication, wal_level docs
- [ ] Add invoke tasks for slot/publication management
- [ ] Integration testing with local Postgres
- [ ] Update docs/cdc.md as implementation evolves

## Architecture

### Why CDC lives in calce-data, not calce-api

CDC updates the in-memory caches (`ConcurrentMarketData`, `UserDataStore`). These are owned by the data layer, not the API layer. If CDC were wired in calce-api, other consumers would miss updates:

- **calce-python**: Python bindings hold long-lived `DataService` with their own `MarketData` / `UserData`. Without data-layer CDC, Python gets stale data.
- **Future services**: Any new service (workers, WebSocket servers) using calce-data would need to re-implement CDC wiring.
- **Separation of concerns**: The API layer shouldn't own data freshness — it just serves requests.

### Data flow

```
Postgres WAL stream
    | (pgoutput binary protocol via postgres-replication)
    v
calce-cdc crate (CdcListener)
    | (typed CdcEvent channel)
    v
calce-data DataService (background task)
    | (calls set_current_price(), set_current_fx_rate(), insert_trade(), etc.)
    v
ConcurrentMarketData / UserDataStore
    | (existing PubSub notifications)
    v
All consumers: calce-api, calce-python, future services
```

### DataService (new, in calce-data)

Owns the full lifecycle: load data → start CDC → provide Arc'd stores to consumers.

```rust
pub struct DataService {
    market_data: Arc<MarketDataStore>,
    user_data: Arc<UserDataStore>,
    cdc_handle: Option<JoinHandle<()>>,  // background CDC task
}

impl DataService {
    pub async fn new(database_url: &str) -> Result<Self>;
    pub fn market_data(&self) -> Arc<MarketDataStore>;
    pub fn user_data(&self) -> Arc<UserDataStore>;
}
```

calce-api and calce-python both construct a `DataService` and pull stores from it. CDC runs automatically unless `CALCE_CDC_ENABLED=false`.

## New crate: `calce-cdc`

### Dependencies

- `postgres-replication` (git dep, MaterializeInc fork of rust-postgres) — pgoutput binary parser + `LogicalReplicationStream`
- `tokio-postgres` — replication-mode connections
- `tokio` — async runtime
- `tracing` — logging
- `rust_decimal` — NUMERIC type parsing (avoids reimplementing Postgres arbitrary-precision decimals)

### Why `postgres-replication` and not Supabase ETL

Supabase ETL is a full CDC pipeline framework (~8,000+ lines) with its own config system, pipeline abstraction, destination trait, worker pools, and memory monitoring. It depends on `postgres-replication` internally for the actual pgoutput parsing.

We use `postgres-replication` directly because:
- It gives us the complete pgoutput parser (~1,041 lines, battle-tested by Materialize in production)
- Integrates with `tokio-postgres` we already know
- Provides `LogicalReplicationStream` as `Stream<Item = Result<ReplicationMessage<LogicalReplicationMessage>>>` — exactly what we need
- No framework overhead, no unused abstractions

Estimated code on top: ~1,050–1,800 lines for our replication client, event mapping, value parsing, and apply loop.

### Module structure

```
crates/calce-cdc/
├── Cargo.toml
└── src/
    ├── lib.rs              — public API: CdcListener, CdcEvent, CdcConfig
    ├── connection.rs       — replication connection, slot/publication management, keep-alive
    ├── events.rs           — CdcEvent enum, WAL row → domain event mapping
    ├── values.rs           — text-format PG value parsing (dates, floats, strings, decimals)
    └── listener.rs         — main loop: stream → parse → emit, reconnect logic, LSN tracking
```

### Key types

```rust
pub struct CdcConfig {
    pub database_url: String,
    pub slot_name: String,           // "calce_cdc_slot"
    pub publication_name: String,    // "calce_cdc_pub"
}

pub enum CdcEvent {
    PriceChanged {
        instrument_id: InstrumentId,
        date: NaiveDate,
        price: f64,
    },
    FxRateChanged {
        from_currency: Currency,
        to_currency: Currency,
        date: NaiveDate,
        rate: f64,
    },
    TradeInserted { trade: Trade },
    TradeDeleted { trade_id: TradeId },
    InstrumentChanged { instrument_id: i64 },
}

pub struct CdcListener { /* ... */ }

impl CdcListener {
    pub fn new(config: CdcConfig) -> (Self, mpsc::Receiver<CdcEvent>);
    pub async fn run(self) -> Result<(), CdcError>;
}
```

### pgoutput protocol (handled by postgres-replication)

| Tag | Message | Our handling |
|-----|---------|-------------|
| `R` | Relation | Cache table OID → (name, columns) in schema cache |
| `B` | Begin | Note transaction boundary |
| `C` | Commit | Confirm LSN to Postgres |
| `I` | Insert | Map row to CdcEvent, send on channel |
| `U` | Update | Map new row to CdcEvent, send on channel |
| `D` | Delete | Emit TradeDeleted (trades only, prices/FX ignore deletes) |

### Key implementation concerns

- **Keep-alive**: Postgres has `wal_sender_timeout` (default 60s). Must send `standby_status_update` periodically. `postgres-replication` handles the message format; we drive the timing.
- **TOAST columns**: Unchanged TOASTed columns arrive as `UnchangedToast` markers. Not relevant for our tables (prices/FX have no large columns), but handle gracefully.
- **Value parsing**: pgoutput sends values as text. We only need: int4/8, float8, text/varchar, date, char(3). Use `rust_decimal` if NUMERIC columns appear. Much simpler than Supabase's 2,400-line general-purpose parser.
- **Schema changes mid-stream**: New Relation messages can arrive after DDL. Update schema cache atomically.

## Changes to existing crates

### calce-data

- New `DataService` struct that wraps `MarketDataStore` + `UserDataStore` + CDC lifecycle
- `UserDataStore`: add `insert_trade()` and `remove_trade()` methods
- `MarketDataStore`: may need method to reload instrument metadata

### calce-api

- Simplify `main.rs`: construct `DataService`, pull stores from it, build `AppState`
- Remove manual loader calls (moved into DataService)

### calce-python

- Update `DataService` Python wrapper to use the Rust `DataService`
- Python scripts automatically get live updates via the shared Arc'd stores

### calce-db

- Alembic migration: `CREATE PUBLICATION calce_cdc_pub FOR TABLE prices, fx_rates, trades, instruments`
- Document `wal_level = logical` requirement

## Error handling & resilience

- **Persistent slot**: Survives server restarts. Postgres retains WAL segments until we confirm them. Requires monitoring WAL disk usage (add an invoke task or health check).
- **Connection loss**: Reconnect with exponential backoff, resume from last confirmed LSN.
- **LSN tracking**: Confirm after applying events. Start in-memory (acceptable — persistent slot means we re-process at most one batch on crash).
- **Graceful shutdown**: Send final status update, wait for server ack, then drop connection.
- **Out-of-range dates**: For prices/FX, use `set_current_price()` / `set_current_fx_rate()` which only update the latest value — no array bounds to worry about.

## Database setup

### Prerequisites

`wal_level = logical` in `postgresql.conf` (one-time, requires Postgres restart).

For local dev, add to docker-compose or document in setup instructions.

### Slot & publication

Slot is created programmatically by `CdcListener` on first connect:
```sql
SELECT pg_create_logical_replication_slot('calce_cdc_slot', 'pgoutput');
```

Publication is created by Alembic migration:
```sql
CREATE PUBLICATION calce_cdc_pub FOR TABLE prices, fx_rates, trades, instruments;
```

### Monitoring

Persistent slots retain WAL. Add periodic check:
```sql
SELECT slot_name, pg_wal_lsn_diff(pg_current_wal_lsn(), restart_lsn) AS retained_bytes
FROM pg_replication_slots WHERE slot_name = 'calce_cdc_slot';
```
