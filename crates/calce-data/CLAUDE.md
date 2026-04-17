# calce-data

Postgres-backed storage and the data stores that the API layer consumes.

## Module Layout

- `market_data_store.rs` — `MarketDataStore`: holds in-memory market data (prices, FX rates, instruments)
- `user_data_store.rs` — `UserDataStore`: holds in-memory user data (trades, users), enforces auth
- `loader.rs` — `load_from_postgres()`: bulk-loads Postgres into both stores at startup
- `types.rs` — `DataStats`: shared response type
- `queries/market_data.rs` — `MarketDataRepo`: SQL for prices, FX rates, instruments (reads + upserts)
- `queries/user_data.rs` — `UserDataRepo`: SQL for users, accounts, trades (reads + CRUD)
- `queries/auth.rs` — `AuthRepo`: SQL for credentials, refresh tokens
- `auth/mod.rs` — `SecurityContext`, `Role`, `AuthConfig`
- `auth/authz.rs` — access-control rules and `require_*` guards
- `auth/jwt.rs` — EdDSA JWT encode/decode
- `auth/password.rs` — Argon2id hash/verify
- `auth/tokens.rs` — secure token generation, HMAC-SHA256 hashing
- `auth/middleware.rs` — unified token validation (JWT + API key fallback)
- `error.rs` — `DataError` enum: auth, not-found, DB, constraint violations
- `config.rs` — `create_pool()`

### How it fits together

```
loader::load_from_postgres(pool)
    ├── queries/  (async SQL)
    ├── MarketDataStore  (wraps ConcurrentMarketData)
    └── UserDataStore    (trades + users + auth)
```

At startup, `load_from_postgres` bulk-loads all data via `queries/` into the two
stores. After that, read methods are synchronous with auth via `SecurityContext`.

`queries/` is also used at runtime for writes (inserts/upserts) by CRUD
endpoints and data import paths.

## Database

Local Postgres via Docker (port 5433). Schema managed by Alembic in `services/calce-db/`.

```sh
invoke db          # start postgres
invoke db-migrate  # run migrations
invoke db-stop     # stop postgres
```

## Queries

All queries use `sqlx::query!` / `query_as!` / `query_scalar!` — checked against
the live database at `cargo build` time. A committed `.sqlx/` cache at the
workspace root lets builds succeed without a running DB: sqlx uses the live DB
when `DATABASE_URL` is set, otherwise falls back to the cache.

After editing a query or running a migration, regenerate the cache and commit
the diff:

```sh
invoke sqlx-prepare   # cargo sqlx prepare --workspace
```

If a build fails with "no cached data for this query", the cache is stale —
run `invoke sqlx-prepare` and commit the result. `invoke sqlx-check` runs
`cargo sqlx prepare --check --workspace` to detect query/schema drift without
regenerating the cache.
