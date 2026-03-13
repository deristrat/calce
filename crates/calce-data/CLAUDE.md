# calce-data

Async data access layer. Provides `DataBackend` trait and implementations for loading
trades and market data from different sources.

## Module Layout

| Module | Purpose |
|--------|---------|
| `auth.rs` | `SecurityContext`, `Role` — user identity and role types |
| `permissions.rs` | `can_access_user_data()` — centralized access-control checks |
| `backend/mod.rs` | `DataBackend` trait — read-only interface used by `DataLoader` |
| `backend/postgres.rs` | Postgres backend — orchestrates repo queries into `DataBackend` |
| `backend/in_memory.rs` | In-memory backend for tests |
| `repo/market_data.rs` | Postgres query layer: prices, FX rates (reads + writes) |
| `repo/user_data.rs` | Postgres query layer: users, accounts, trades (reads + writes) |
| `loader.rs` | `DataLoader` — wraps a `DataBackend`, adds auth checks and input assembly |
| `config.rs` | Database connection configuration |

### Layer overview

```
DataLoader  →  DataBackend (trait)  →  PostgresBackend  →  repo/ (SQL queries)
                                    →  InMemoryBackend
```

`repo/` is Postgres-specific and includes write methods (inserts) not exposed
through `DataBackend`. It is only used by `PostgresBackend`.

## Database

Local Postgres via Docker (port 5433). Schema managed by sqlx migrations.

```sh
invoke db       # start
invoke db-stop  # stop
```
