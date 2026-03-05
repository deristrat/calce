# Architecture

Financial calculation engine for portfolio tracking, valuation, and analytics.

## Design Principles

1. **Pure calculations, impure boundaries** — calculation functions are pure (data in, result out). Side effects (data loading, auth) live at the edges.
2. **Dual API** — every calculation is available in two modes: _stateful_ (engine loads data, then calculates) and _stateless_ (caller provides data directly).
3. **Plain data types** — domain types carry data, not behavior. Business logic lives in `calc/`.
4. **Trait-based data access** — services are traits, swappable for testing, caching, or different backends.
5. **Sync core, async boundaries** — calce-core is 100% sync with no DB or async dependencies. Async data loading lives in calce-data; calce-core stays fast to compile and easy to test.

## Crate Structure

```
calce-core    (sync, pure — domain types, calc functions, service traits)
    ↑
calce-data    (async — sqlx repos, AsyncCalcEngine, Postgres access)
    ↑
calce-api     (async — axum HTTP handlers, thin layer over AsyncCalcEngine)

calce-python  (PyO3 bindings, depends on calce-core only)
```

## Layers

```
┌──────────────────────────────────────────────────────┐
│  calce-api — HTTP Layer                              │
│  Thin axum handlers, auth extraction, param parsing  │
├──────────────────────────────────────────────────────┤
│  calce-data — Data Layer                             │
│  AsyncCalcEngine (orchestration)                     │
│  Repos: MarketDataRepo, UserDataRepo (sqlx/Postgres) │
│  Loads data async → delegates to sync calce-core     │
├──────────────────────────────────────────────────────┤
│  calce-core — Calculation Layer                      │
│                                                      │
│  CalcEngine        │  Reports (reports/)              │
│  Sync orchestrator │  Composed views for consumers    │
│  Wires services    │  portfolio (MV + changes)        │
│  to calc functions │                                  │
│                    │  Calculations (calc/)             │
│  Services          │  Pure functions                   │
│  Data access traits│  aggregation, market_value        │
│  InMemory impls    │  value_change, volatility         │
├────────────────────┴─────────────────────────────────┤
│  Domain Types                                        │
│  Trade, Position, Money, FxRate, Quantity, Price ...  │
└──────────────────────────────────────────────────────┘
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

Trait-based data access in calce-core. Each trait has an in-memory implementation used for testing and seed data. Production data loading is handled by calce-data's async repos (see Data Layer below).

### MarketDataService

```rust
trait MarketDataService {
    fn get_price(&self, instrument: &InstrumentId, date: NaiveDate) -> CalceResult<Price>;
    fn get_price_history(&self, instrument: &InstrumentId, from: NaiveDate, to: NaiveDate) -> CalceResult<Vec<(NaiveDate, Price)>>;
    fn get_fx_rate(&self, from: Currency, to: Currency, date: NaiveDate) -> CalceResult<FxRate>;
}
```

Provides market prices and FX rates. Read-only. `InMemoryMarketDataService` is the default implementation, also used as the bridge between calce-data's async loading and calce-core's sync calculations.

### UserDataService

```rust
trait UserDataService {
    fn get_trades(&self, ctx: &SecurityContext, user_id: &UserId) -> CalceResult<Vec<Trade>>;
}
```

Loads trades for a user. Authorization is checked at the service boundary.

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

### Implemented calculations

| Module | Input | Output | Description |
|--------|-------|--------|-------------|
| `aggregation` | trades, as_of_date | positions | Sum trades into net positions per instrument |
| `market_value` | positions, prices, fx | valued positions + total | Current market value in base currency |
| `value_change` | trades, prices, fx, context | daily/weekly/yearly/YTD changes | Value change across standard periods |
| `volatility` | instrument, price history | annualized + daily vol | Historical realized volatility from log returns |

### Planned calculations

| Module | Input | Output | Description |
|--------|-------|--------|-------------|
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

## CalcEngine (`calce-core/engine.rs` — Sync Orchestration)

```rust
pub struct CalcEngine<'a> {
    pub ctx: &'a CalculationContext,
    pub security_ctx: &'a SecurityContext,
    pub market_data: &'a dyn MarketDataService,
    pub user_data: &'a dyn UserDataService,
}
```

The sync engine is calce-core's **stateful** entry point. It:
1. Loads data via service traits (with authorization)
2. Delegates to pure `calc/` functions or `reports/` composites
3. Returns results

It does not contain business logic itself — it wires services to calculations. Used directly in tests and by calce-python bindings.

## Data Layer (`calce-data/`)

Async data access layer connecting calce-core's sync calculations to real databases.

### AsyncCalcEngine

The async counterpart of CalcEngine. Supports two backends:

- **Postgres** — production mode. Loads data from sqlx repos, builds `InMemoryMarketDataService`/`InMemoryUserDataService` with the loaded data, then passes to calce-core's sync calc functions.
- **InMemory** — test mode. Wraps existing `InMemory*` services directly and delegates to CalcEngine.

```
AsyncCalcEngine.market_value_for_user(security_ctx, user_id, ctx)
  1. Auth check
  2. Load trades from UserDataRepo (async)
  3. Aggregate positions (sync, calce-core)
  4. Batch-load prices + FX rates for those positions (async)
  5. Build InMemoryMarketDataService with loaded data
  6. Call value_positions (sync, calce-core)
```

This pattern keeps calce-core sync and free of database dependencies while allowing efficient batch loading (no N+1 queries).

### Repos

- **MarketDataRepo** — prices and FX rates. Supports single lookups, history ranges, and batch loading for multiple instruments/currency pairs.
- **UserDataRepo** — users, accounts, and trades. Write operations for local DB; read-only planned for njorda backend.

### Database

Local Postgres (Dockerized, port 5433) with schema managed by sqlx migrations. Tables: `users`, `instruments`, `accounts`, `trades`, `prices`, `fx_rates`.

Start with `invoke db`, stop with `invoke db-stop`.

### Njorda Backend (planned)

Read-only access to njorda's existing Postgres databases (instruments, users, trades). Two separate connection pools for main DB and dataapp DB. Feature-gated behind `njorda` cargo feature.

## API Layer

Two modes for every calculation. Internally they share the same pure function.

### Stateful — load and calculate

The caller identifies _what_ to calculate (which user, which account). The engine loads the required data and performs the calculation.

```
GET /v1/users/{user_id}/market-value?as_of_date=...&base_currency=...
GET /v1/users/{user_id}/portfolio?as_of_date=...&base_currency=...
GET /v1/instruments/{instrument_id}/volatility?as_of_date=...&lookback_days=...
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

HTTP/gRPC is a separate concern. calce-api wraps the library API with transport (axum), serialization, and authentication.

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

### Caching intermediate results

When composite calculations call the same primitive multiple times (e.g. `value_change_summary` calls `value_positions` at 5 dates), there may be overlap with other composite calculations that need the same snapshots. A calculation cache or result graph could avoid redundant work. The pure-function design makes this straightforward to add later — wrap the same functions with memoization.
