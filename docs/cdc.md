# Change Data Capture (CDC)

Real-time propagation of database changes to in-memory caches.

## Purpose

Calce loads market data (prices, FX rates) and user data (trades, instruments) from Postgres into lock-free in-memory caches at startup. CDC keeps these caches fresh by streaming changes from Postgres as they happen, without requiring a restart.

## Architecture

```
Postgres (logical replication, pgoutput protocol)
    |
calce-cdc (CdcListener)
    |  CdcEvent { table, operation, columns } via mpsc channel
    v
calce-data::cdc::start_cdc (background task)
    |  domain decoding per table
    v
ConcurrentMarketData / UserDataStore
    |  existing PubSub notifications
    v
consumers: calce-api, calce-python, ...
```

**CDC lives in the data layer**, not the API layer. `calce-data::cdc::start_cdc` spawns the listener and event-consumer tasks. Any process that calls it gets live updates — the API server, future services.

## Crate: `calce-cdc`

A domain-agnostic library crate that connects to Postgres logical replication and emits row-change events. It has no knowledge of Calce's domain types, caches, or stores — it only parses WAL changes and sends `CdcEvent` values on a channel. Each event carries the `table` name, the DML `operation`, and the row's `columns` as text. Consumers decode the domain meaning.

**Key dependency:** `postgres-protocol` (published crate, by the rust-postgres author). Provides wire protocol message encoding/decoding. The crate connects over raw TCP with `replication=database` in the startup message and parses pgoutput binary messages itself. We evaluated Supabase ETL and MaterializeInc's `postgres-replication` fork — both require git dependencies. Our approach uses only published crates.

## Replicated tables

The authoritative list is `calce_cdc::REPLICATED_TABLES`. The listener creates the publication on startup if missing and amends it if a table is missing from an existing publication.

Currently: `prices`, `fx_rates`, `trades`, `instruments`, `users`, `organizations`, `accounts`, `api_keys`.

The consumer in `calce-data` applies decoded changes as follows:

- `prices` (INSERT, UPDATE): calls `set_current_price()` after resolving `instrument_id` → ticker from an in-memory map. Deletes are ignored.
- `fx_rates` (INSERT, UPDATE): calls `set_current_fx_rate()`. Deletes are ignored.
- `instruments` (INSERT, UPDATE): updates the consumer's `id → ticker` map, then forwards an entity event.
- `users` (any): calls `update_user_info()` on `UserDataStore`, then forwards an entity event.
- all other tables: forwarded as an entity event only (SSE → frontend).

## Replication slot

**Persistent** — the slot (`calce_cdc_slot`) survives server restarts and Postgres retains unconfirmed WAL segments. On reconnect, streaming resumes from the last confirmed LSN with no data loss.

Trade-off: retained WAL consumes disk. Monitor with:

```sql
SELECT slot_name, pg_wal_lsn_diff(pg_current_wal_lsn(), restart_lsn) AS retained_bytes
FROM pg_replication_slots WHERE slot_name = 'calce_cdc_slot';
```

## Configuration

- `CALCE_CDC_ENABLED` (default `true`): set to `false` or `0` to disable CDC entirely.
- `DATABASE_URL` (required): Postgres connection string. Same as the main application.

Slot name (`calce_cdc_slot`) and publication name (`calce_cdc_pub`) are fixed conventions, not configurable.

## Database prerequisites

1. `wal_level = logical` in `postgresql.conf` (requires Postgres restart).
2. Publication is created and kept current automatically by the listener on startup.
3. Replication slot is created automatically on first connect.
