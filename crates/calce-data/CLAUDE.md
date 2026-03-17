# calce-data

Postgres-backed storage and the `DataService` that the API layer consumes.

## Module Layout

| Module | Purpose |
|--------|---------|
| `service.rs` | `DataService` — bulk-loads Postgres into in-memory services at startup; serves calc inputs, user/instrument listings, price history with auth checks |
| `queries/market_data.rs` | `MarketDataRepo` — SQL for prices, FX rates, instruments (reads + upserts) |
| `queries/user_data.rs` | `UserDataRepo` — SQL for users, accounts, trades (reads + CRUD) |
| `auth.rs` | `SecurityContext`, `Role` — caller identity |
| `permissions.rs` | `can_access_user_data()` — access-control rules |
| `error.rs` | `DataError` enum — auth, not-found, DB, constraint violations |
| `config.rs` | `create_pool()`, `run_migrations()` |

### How it fits together

```
DataService::from_postgres(pool)
    ├── queries/  (async SQL, used only at startup to bulk-load)
    ├── InMemoryMarketDataService  (from calce-core, holds all prices/FX)
    └── InMemoryUserDataService    (from calce-core, holds all trades)
```

After startup, `DataService` methods are synchronous — they read from the
in-memory services and enforce auth via `SecurityContext`.

`queries/` also has write methods (inserts/upserts) used by the API's CRUD
endpoints and by data import paths.

## Database

Local Postgres via Docker (port 5433). Schema managed by sqlx migrations.

```sh
invoke db       # start
invoke db-stop  # stop
```
