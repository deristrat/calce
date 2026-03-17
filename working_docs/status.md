# Implementation Status

## Implemented Calculations

| Module | Input | Output | Description |
|--------|-------|--------|-------------|
| `aggregation` | trades, as_of_date | positions | Sum trades into net positions per instrument |
| `market_value` | positions, prices, fx | valued positions + total | Current market value in base currency |
| `value_change` | trades, prices, fx, context | daily/weekly/yearly/YTD changes | Value change across standard periods |
| `volatility` | instrument, price history | annualized + daily vol | Historical realized volatility from log returns |

## Planned Calculations

| Module | Input | Output | Description |
|--------|-------|--------|-------------|
| `pnl` | trades, current prices, fx | realized + unrealized P&L | Profit/loss broken down by component |
| `cost_basis` | trades | cost basis per position | Average cost, supports FIFO/average methods |
| `risk` | positions, prices, historical data | risk metrics | Exposure, concentration, currency risk |

## Njorda Backend (planned)

Read-only access to njorda's existing Postgres databases (instruments, users, trades). Two separate connection pools for main DB and dataapp DB. Feature-gated behind `njorda` cargo feature.

## Open Design Questions

### Caching intermediate results

When composite calculations call the same primitive multiple times (e.g. `value_change_summary` calls `value_positions` at 5 dates), there may be overlap with other composite calculations that need the same snapshots. A calculation cache or result graph could avoid redundant work. The pure-function design makes this straightforward to add later — wrap the same functions with memoization.
