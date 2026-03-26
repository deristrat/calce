# Implementation Status

## Implemented Calculations

| Module | Tag | Description |
|--------|-----|-------------|
| `aggregation` | `#CALC_AGG` | Sum trades into net positions per instrument |
| `market_value` | `#CALC_MV` | Current market value in base currency |
| `value_change` | `#CALC_VC` | Value change across standard periods |
| `volatility` | `#CALC_VOL` | Historical realized volatility from log returns |
| `type_allocation` | `#CALC_ALLOC_INSTYPE` | Allocation by instrument type |
| `weighted_allocation` | `#CALC_ALLOC_WEIGHTED` | Generic weighted allocation engine |
| `sector_allocation` | `#CALC_ALLOC_SECTOR` | Sector allocation via weighted engine |

## In Progress

### Njorda data import
Script and DB migration done. Remaining: Rust Organization struct, full end-to-end test.
See: `njorda_import_plan.md`

## Planned Calculations

| Module | Description |
|--------|-------------|
| `pnl` | Realized + unrealized P&L (FIFO/average cost) |
| `cost_basis` | Average cost per position |
| `risk` | Exposure, concentration, currency risk |

## Open Design Questions

### Caching intermediate results

When composite calculations call the same primitive multiple times, there may be overlap. A calculation cache or result graph could avoid redundant work. The pure-function design makes this straightforward to add later.
