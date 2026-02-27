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
│  Services            │  Reports (reports/)           │
│  Data loading traits │  Composed views for consumers │
│  market data         │  portfolio (MV + changes)     │
│  user/account data   │                               │
│                      │  Calculations (calc/)         │
│                      │  Pure functions               │
│                      │  aggregation                  │
│                      │  market_value                 │
│                      │  value_change                 │
│                      │  pnl                          │
│                      │  cost_basis                   │
│                      │  risk                         │
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

### Composition

Calculations compose by calling each other. A higher-level calculation receives the same inputs (trades, market data, context) and calls lower-level calculations internally. For example, `value_change_summary` calls `aggregate_positions` and `value_positions` at multiple dates, then computes the diff.

The pattern has four levels:

1. **Primitive** — single-purpose, takes exactly what it needs: `value_positions(positions, ctx, market_data)`
2. **Composite** — calls primitives at multiple points or combines results: `value_change_summary(trades, ctx, market_data)` calls `aggregate_positions` + `value_positions` for each comparison date
3. **Report** (`reports/`) — bundles multiple calc primitives/composites into a single consumer-facing result, sharing intermediate values to avoid redundant computation: `portfolio_report(trades, ctx, market_data)` computes market value once and passes it to value change
4. **Engine** — orchestrates data loading then delegates to pure functions at any level

This keeps each function testable in isolation. A future optimization is to cache intermediate results (e.g. a calculation graph that reuses already-computed market values), but the function-call structure doesn't need to change for that — caching wraps the same functions.

## CalcEngine (`engine.rs` — Orchestration)

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
2. Delegates to pure `calc/` functions or `reports/` composites
3. Returns results

It does not contain business logic itself — it wires services to calculations.

## API Layer

Two modes for every calculation. Internally they share the same pure function.

### Stateful — load and calculate

The caller identifies _what_ to calculate (which user, which account). The engine loads the required data and performs the calculation.

```
market_value_for_user(user_id)        → MarketValueResult
market_value_for_account(account_id)  → MarketValueResult
portfolio_report_for_user(user_id)    → PortfolioReport
pnl_for_user(user_id)                → PnlResult
pnl_for_account(account_id)          → PnlResult
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

## Reports (`reports/`)

Composed views that bundle multiple `calc/` results into a single consumer-facing struct. Reports share intermediate values (e.g. the current-date `MarketValueResult`) to avoid redundant computation. Each report has a pure function (stateless) and a corresponding engine method (stateful).

Current reports:
- **`portfolio_report`** — market value + value changes. Aggregates trades once, values once, passes the snapshot to value change.

## Open Design Questions

### Caching intermediate results

When composite calculations call the same primitive multiple times (e.g. `value_change_summary` calls `value_positions` at 5 dates), there may be overlap with other composite calculations that need the same snapshots. A calculation cache or result graph could avoid redundant work. The pure-function design makes this straightforward to add later — wrap the same functions with memoization.
