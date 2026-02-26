# Architecture

Financial calculation engine for portfolio tracking, valuation, and analytics.

## Design Principles

1. **Pure calculations, impure boundaries** — calculation functions are pure (data in, result out). Side effects (data loading, auth) live at the edges.
2. **Dual API** — every calculation is available in two modes: _stateful_ (engine loads data, then calculates) and _stateless_ (caller provides data directly).
3. **Plain data types** — domain types carry data, not behavior. Business logic lives in `calc/`.
4. **Trait-based data access** — services are traits, swappable for testing, caching, or different backends.

## Layers

```
┌─────────────────────────────────────────────────────┐
│  API Layer                                          │
│  Stateful endpoints    Stateless endpoints          │
│  (user/account based)  (caller provides data)       │
├─────────────────────────────────────────────────────┤
│  CalcEngine (orchestration)                         │
│  Wires services → pure calc functions               │
│  Holds context, security, service references        │
├──────────────────────┬──────────────────────────────┤
│  Services            │  Calculations (calc/)        │
│  Data loading traits │  Pure functions              │
│  market data         │  aggregation                 │
│  user/account data   │  market_value                │
│                      │  pnl                         │
│                      │  cost_basis                  │
│                      │  risk                        │
├──────────────────────┴──────────────────────────────┤
│  Domain Types                                       │
│  Trade, Position, Money, FxRate, Quantity, Price ... │
└─────────────────────────────────────────────────────┘
```

## Domain Types (`domain/`)

Plain data carriers. No business logic beyond intrinsic operations (e.g. `Money::convert`, `FxRate::invert`).

Core types:
- **Money** — amount + currency, the fundamental financial value
- **Trade** — a single execution (instrument, quantity, price, date)
- **Position** — aggregated holding for one instrument (quantity, no pricing)
- **Quantity** — signed decimal (positive = long, negative = short)
- **Price** — decimal wrapper for type safety
- **FxRate** — directed exchange rate (from → to)
- **Currency** — ISO 4217 code

Identity types:
- **UserId**, **AccountId**, **InstrumentId**, **TradeId** _(planned)_

### Account

An account groups trades under a user. It has its own currency and a label (e.g. "Avanza ISK", "Interactive Brokers USD"). A trade belongs to exactly one account.

Account currency is used for account-level reporting. When aggregating across accounts for a user, values are converted to the base currency from `CalculationContext`.

## Services (`services/`)

Trait-based data access. Each trait has an in-memory implementation for testing and a production implementation backed by a database or external API.

### MarketDataService

```rust
trait MarketDataService {
    fn get_price(&self, instrument: &InstrumentId, date: NaiveDate) -> CalceResult<Price>;
    fn get_fx_rate(&self, from: Currency, to: Currency, date: NaiveDate) -> CalceResult<FxRate>;
}
```

Provides market prices and FX rates. Read-only. May be split into `PriceService` + `FxRateService` later if data sources diverge.

### UserDataService

```rust
trait UserDataService {
    fn get_accounts(&self, ctx: &SecurityContext, user_id: &UserId) -> CalceResult<Vec<Account>>;
    fn get_trades_for_user(&self, ctx: &SecurityContext, user_id: &UserId) -> CalceResult<Vec<Trade>>;
    fn get_trades_for_account(&self, ctx: &SecurityContext, account_id: &AccountId) -> CalceResult<Vec<Trade>>;
}
```

Loads accounts and trades. Authorization is checked here at the service boundary — if the caller can access the user, they can access all their accounts. No account-level permissions.

## Calculations (`calc/`)

Pure functions. No service dependencies — they receive data as arguments and return results. This is what makes the stateless API possible.

Each calculation module follows the same shape:

```rust
// calc/market_value.rs
pub fn value_positions(
    positions: &[Position],
    ctx: &CalculationContext,
    market_data: &dyn MarketDataService,
) -> CalceResult<MarketValueResult>
```

Note: `MarketDataService` is a read-only lookup trait, so passing it doesn't violate purity in any meaningful sense — the function has no side effects.

### Planned calculations

| Module | Input | Output | Description |
|--------|-------|--------|-------------|
| `aggregation` | trades, as_of_date | positions | Sum trades into net positions per instrument |
| `market_value` | positions, prices, fx | valued positions + total | Current market value in base currency |
| `pnl` | trades, current prices, fx | realized + unrealized P&L | Profit/loss broken down by component |
| `cost_basis` | trades | cost basis per position | Average cost, supports FIFO/average methods |
| `risk` | positions, prices, historical data | risk metrics | Exposure, concentration, currency risk |

Each builds on the ones above. For example, P&L needs cost basis and market value. The dependency is through data, not through function calls — the engine orchestrates which calculations to run and feeds results forward.

## CalcEngine (Orchestration)

```rust
pub struct CalcEngine<'a> {
    pub ctx: &'a CalculationContext,
    pub security_ctx: &'a SecurityContext,
    pub market_data: &'a dyn MarketDataService,
    pub user_data: &'a dyn UserDataService,
}
```

The engine is the **stateful** entry point. It:
1. Loads data via services (with authorization)
2. Calls pure calculation functions
3. Returns results

It does not contain business logic itself — it wires services to calculations.

## API Layer

Two modes for every calculation. Internally they share the same pure function.

### Stateful — load and calculate

The caller identifies _what_ to calculate (which user, which account). The engine loads the required data and performs the calculation.

```
market_value_for_user(user_id)       → MarketValueResult
market_value_for_account(account_id) → MarketValueResult
pnl_for_user(user_id)               → PnlResult
pnl_for_account(account_id)         → PnlResult
```

Use cases: production application serving a logged-in user, scheduled batch jobs.

### Stateless — calculate from provided data

The caller provides all input data. No data loading, no authorization.

```
calculate_market_value(trades, prices, fx_rates, context) → MarketValueResult
calculate_pnl(trades, prices, fx_rates, context)          → PnlResult
```

Use cases: simulations, what-if analysis, external integrations, testing, sold as a service.

### Where the API lives

The library exposes both modes as public Rust functions. This keeps the engine embeddable — any Rust application can use it as a dependency.

HTTP/gRPC is a separate concern. A thin service crate wraps the library API with transport, serialization, and authentication. The architecture doc does not prescribe the transport layer.

## Contexts

### CalculationContext

```rust
pub struct CalculationContext {
    pub base_currency: Currency,
    pub as_of_date: NaiveDate,
}
```

Pure parameters for a calculation. No service references, no state. Passed into every calculation function.

### SecurityContext

```rust
pub struct SecurityContext {
    pub user_id: UserId,
    pub role: Role,  // Admin | User
}
```

Used by the stateful path only. The stateless path has no concept of authorization — if you have the data, you can calculate.

## Partial Results

Calculations return partial results rather than failing on the first missing data point. A portfolio with 50 positions where 1 price is missing returns 49 valued positions plus a warning about the missing one.

```rust
struct Outcome<T> {
    value: T,
    warnings: Vec<Warning>,
}
```

Every calculation function returns `Outcome<T>` (or `CalceResult<Outcome<T>>` for errors that genuinely prevent any calculation). Warnings carry enough context for the caller to understand what was skipped and why.

## Open Design Questions

### Composed calculations

If a caller wants market value + P&L + risk in one call, they could make 3 separate calls. But this duplicates data loading and aggregation.

Options:
- **Portfolio report** — a higher-level function that runs multiple calculations on the same loaded data and returns a composite result
- **Keep it simple** — let the caller compose, optimize later

Starting simple is fine, but the architecture should not prevent the composed path.

### Time-series calculations

Current engine is point-in-time (`as_of_date`). Many use cases need time-series:
- Daily P&L over a month
- Historical NAV chart
- Drawdown analysis

These can be built as loops over point-in-time calculations initially, with optimization (incremental computation, caching) added later. The pure-function design supports this well — call the same function for each date.
