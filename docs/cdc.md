# Change Data Capture (CDC)

Real-time propagation of database changes to in-memory caches.

## Purpose

Calce loads market data (prices, FX rates) and user data (trades, instruments) from Postgres into lock-free in-memory caches at startup. CDC keeps these caches fresh by streaming changes from Postgres as they happen, without requiring a restart.

## Architecture

```
Postgres (logical replication, pgoutput protocol)
    |
calce-cdc (CdcListener)
    |  typed events via mpsc channel
    v
calce-data (DataService applies events to caches)
    |
ConcurrentMarketData / UserDataStore
    |  existing PubSub notifications
    v
consumers: calce-api, calce-python, ...
```

**CDC lives in the data layer**, not the API layer. `DataService` in calce-data owns both the initial bulk load and the ongoing CDC stream. Any process that constructs a `DataService` gets live updates — the API server, Python bindings, future services.

## Crate: `calce-cdc`

A library crate that connects to Postgres logical replication and emits typed domain events. It has no knowledge of caches or stores — it only parses WAL changes and sends `CdcEvent` values on a channel. The consumer (DataService) decides what to do with them.

**Key dependency:** `postgres-protocol` (published crate, by the rust-postgres author). Provides wire protocol message encoding/decoding. The crate connects over raw TCP with `replication=database` in the startup message and parses pgoutput binary messages itself. We evaluated Supabase ETL and MaterializeInc's `postgres-replication` fork — both require git dependencies. Our approach uses only published crates.

## Monitored tables

| Table | Events | Cache effect |
|-------|--------|-------------|
| `prices` | INSERT, UPDATE | `set_current_price()` |
| `fx_rates` | INSERT, UPDATE | `set_current_fx_rate()` |
| `trades` | INSERT, DELETE | `insert_trade()` / `remove_trade()` |
| `instruments` | INSERT, UPDATE | Reload instrument metadata |

## Replication slot

**Persistent** — the slot (`calce_cdc_slot`) survives server restarts and Postgres retains unconfirmed WAL segments. On reconnect, streaming resumes from the last confirmed LSN with no data loss.

Trade-off: retained WAL consumes disk. Monitor with:

```sql
SELECT slot_name, pg_wal_lsn_diff(pg_current_wal_lsn(), restart_lsn) AS retained_bytes
FROM pg_replication_slots WHERE slot_name = 'calce_cdc_slot';
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `CALCE_CDC_ENABLED` | `true` | Enable/disable CDC. Set to `false` to use startup-only loading. |
| `DATABASE_URL` | (required) | Postgres connection string. Same as the main application. |

Slot name (`calce_cdc_slot`) and publication name (`calce_cdc_pub`) are fixed conventions, not configurable.

## Database prerequisites

1. `wal_level = logical` in `postgresql.conf` (requires Postgres restart)
2. Publication created by Alembic migration: `CREATE PUBLICATION calce_cdc_pub FOR TABLE prices, fx_rates, trades, instruments`
3. Replication slot created automatically by `CdcListener` on first connect
