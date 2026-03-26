# Njorda → Calce Data Import

Import real data from the Njorda dev databases into the local Calce database for realistic testing.

## Data Sources

Two Njorda databases (via Cloud SQL Proxy on port 22020):

| Database | User | Password source | Contains |
|----------|------|-----------------|----------|
| `dataapp` | `dataapp` | `NJORDA_DB_PASSWORD` env var | Instruments, historical prices, FX rates |
| `njorda` | `njorda` | SOPS: `sops -d services/api/secrets/dev.yml` in njorda repo | Users, orgs, accounts, trades |

Target: `calce` on `localhost:5433`.

## Schema Changes

Add an `organizations` table and `organization_id` FK on `users` — small migration, keeps the org structure intact from njorda rather than losing it and re-importing later.

New table:
```sql
CREATE TABLE organizations (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    external_id VARCHAR(64) UNIQUE NOT NULL,  -- njorda org ID
    name        VARCHAR(200),
    created_at  TIMESTAMP NOT NULL DEFAULT now(),
    updated_at  TIMESTAMP NOT NULL DEFAULT now()
);
-- trigger: set_updated_at on UPDATE
```

New column on `users`:
```sql
ALTER TABLE users ADD COLUMN organization_id BIGINT REFERENCES organizations(id);
CREATE INDEX idx_users_organization ON users(organization_id);
```

## Schema Mapping

### Market data (dataapp → calce)

| dataapp | calce | Notes |
|---------|-------|-------|
| `instrument.ticker` | `instruments.ticker` | Direct |
| `instrument.isin` | `instruments.isin` | Direct |
| `instrument.name` | `instruments.name` | Direct |
| `instrument.type` | `instruments.instrument_type` | Map enum values |
| `instrument.currency` | `instruments.currency` | Direct |
| `instrument.sectors` | `instruments.allocations` | JSONB — wrap as `{"sector": sectors}` |
| `historical_price` (close) | `prices` | Deduplicate by source priority |
| `historical_price` (FX pairs) | `fx_rates` | Tickers with "/" → split into from/to currencies |

### Business data (njorda → calce)

| njorda | calce | Notes |
|--------|-------|-------|
| `organization.id` | `organizations.external_id` | String of njorda org ID |
| `organization.name` | `organizations.name` | Direct |
| `user.id` | `users.external_id` | String of njorda user ID |
| `user.email` | `users.email` | **Anonymized**: `user_{id}@test.calce.dev` |
| `user.first_name + last_name` | `users.name` | **Anonymized**: `User {id}` |
| `user.organization_id` | `users.organization_id` | Via org ID mapping |
| `account.currency` | `accounts.currency` | Direct |
| `account.name` | `accounts.label` | Direct, fallback to `"Account {id}"` |
| `account.owner_id` | `accounts.user_id` | Via user ID mapping |
| `account_trade.ticker` | `trades.instrument_id` | Via instrument ticker→id mapping |
| `account_trade.quantity` | `trades.quantity` | Direct |
| `account_trade.acquisition_price` | `trades.price` | Fallback to `provider_instrument_price` |
| `account_trade.provider_instrument_currency` | `trades.currency` | Fallback to account currency |
| `account_trade.timestamp` | `trades.trade_date` | Extract date from datetime |

## Filtering Strategy

Start from trades, work outward:

1. **Organizations**: All orgs (no filter)
2. **Users**: Only users with at least one investment/pension account that has trades
3. **Accounts**: Only `investment` and `pension` type accounts
4. **Trades**: All trades on selected accounts, excluding NULL ticker/quantity/price
5. **Instruments**: Only tickers appearing in selected trades
6. **Prices**: Only for selected instruments, configurable date range (default: 2023-01-01 → today)
7. **FX rates**: Only currency pairs needed (instrument currencies ↔ account currencies)

## Implementation

**File**: `tools/njorda_import.py` (Python, like `seed_db.py`)
**Invoke task**: `invoke njorda-import`

### Prerequisites

- Cloud SQL Proxy running (`invoke njorda-proxy`)
- `NJORDA_DB_PASSWORD` env var set (for dataapp)
- `NJORDA_API_DB_PASSWORD` env var set (for njorda business DB — from SOPS)
- Local calce DB running with migrations applied

### Script Flow

```
1. Connect to njorda business DB (port 22020, user=njorda, db=njorda)
2. Connect to njorda market data DB (port 22020, user=dataapp, db=dataapp)
3. Connect to calce DB (port 5433)

--- Read phase ---
4. Query investment/pension accounts that have trades → get account IDs, user IDs, org IDs
5. Collect unique tickers from trades on those accounts
6. Fetch organizations for those users
7. Fetch instruments from dataapp for those tickers
8. Fetch historical prices for those instruments
9. Identify needed FX pairs, fetch FX rates

--- Write phase ---
10. Wipe calce tables (trades → accounts → users → organizations → prices → fx_rates → instruments)
11. Insert organizations
12. Insert instruments (with sector→allocations mapping)
13. Insert prices
14. Insert FX rates
15. Insert users (anonymized, with org_id)
16. Insert accounts (track njorda→calce ID mapping)
17. Insert trades (resolve instrument_id and account_id)
18. Print summary
```

### CLI Options

```
--from-date       Price/FX history start (default: 2023-01-01)
--to-date         Price/FX history end (default: today)
--db-url          Calce DB URL (default: postgres://calce:calce@localhost:5433/calce)
--njorda-port     Cloud SQL Proxy port (default: 22020)
--dry-run         Print counts of what would be imported without writing
```

### Approach

Full wipe-and-reload each run. This is a dev tool — idempotent fresh import is simplest.

## Progress

- [x] Plan reviewed and approved
- [x] Alembic migration (organizations table + users.organization_id)
- [x] Update SQLAlchemy models
- [x] Import script (`tools/njorda_import.py`)
- [x] Invoke task wired up
- [x] Update Rust data layer (Organization struct, queries, API routes, seed tool)
- [x] Tested with real data (14 orgs, 1113 users, 25844 trades, 1M+ prices)
