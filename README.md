# Calce

High-performance financial calculation library in Rust. Built on exact decimal arithmetic (`rust_decimal`) with a type-safe domain model.

## Architecture

```
┌─────────────────────────────────────────────────┐
│  CalcEngine (orchestration)                     │
│  Wires services → pure functions                │
├────────────────────┬────────────────────────────┤
│  Pure Calculations │  Services (traits)         │
│  Data in, data out │  MarketDataService         │
│  No side effects   │  UserDataService           │
├────────────────────┴────────────────────────────┤
│  Domain Types                                   │
│  Currency, Money, Position, Trade, FxRate, ...   │
└─────────────────────────────────────────────────┘
```

### Layers

1. **Domain** (`domain/`) — Pure data types. No service dependencies, no business logic. Types like `Currency`, `Money`, `Position`, `Trade`. Domain types carry data and provide construction, validation, accessors, and same-type arithmetic — nothing more.

2. **Calculations** (`calc/`) — All business logic lives here. Functions that take domain types and read-only data services, return results. Includes trade aggregation (`aggregate_positions`), market value computation (`value_positions`), and orchestration (`CalcEngine`). No authorization in calculation functions, no side effects. The `MarketDataService` is a read-only lookup — functionally equivalent to a HashMap but without the redundant copying.

3. **Services** (`services/`) — Traits for data access (`MarketDataService`, `UserDataService`). In-memory implementations for testing. Real implementations would talk to databases, APIs, etc.

4. **Engine** (`calc/engine.rs`) — Thin orchestration layer. Handles authorization and user data loading, aggregates trades into positions, then delegates to calculation functions. Should contain minimal logic.

## Design Principles

### Domain types are pure data

Domain types are data carriers. They provide construction, validation, accessors, and same-type arithmetic (`Money + Money`, `Quantity + Quantity`). They never contain business logic, depend on services, or combine multiple domain types for a business purpose. A `Position` is just `(instrument, quantity, currency)` — it doesn't know how to price itself or how to aggregate trades.

### Separate authorization from calculation

Calculation functions operate on **positions** and **read-only market data**. They never touch user data or authorization. The engine handles that boundary:

```rust
// Engine: handles auth + data loading
let trades = user_data.get_trades(security_ctx, user_id)?;
let positions = aggregate_positions(&trades, ctx.as_of_date);

// Calculation: positions + market data lookups, nothing else
value_positions(&positions, ctx, market_data)
```

This means calculation functions are testable without any auth setup — just construct positions and an `InMemoryMarketDataService`.

`MarketDataService` is a read-only lookup trait (prices, FX rates). Passing it directly avoids redundant copying into intermediate HashMaps while keeping the same testability — the in-memory implementation is trivial to set up.

```rust
fn value_positions(
    positions: &[Position],
    ctx: &CalculationContext,
    market_data: &dyn MarketDataService,
) -> CalceResult<MarketValueResult>
```

### CalculationContext is pure data

`CalculationContext` holds parameters for a calculation run (`base_currency`, `as_of_date`). It does **not** hold service references — those belong to the engine. This keeps the context serializable, cloneable, and free of lifetime concerns.

### Newtype pattern for type safety

Every domain value has its own type: `Quantity(Decimal)`, `Price(Decimal)`, `Money { amount, currency }`, `FxRate { from, to, rate }`. The compiler prevents mixing up a price and a quantity, or applying an FX rate in the wrong direction.

### FX rates carry directionality

An `FxRate` knows its source and target currencies — like a physical unit. `FxRate { from: USD, to: SEK, rate: 10.5 }` means "1 USD = 10.5 SEK". When converting `Money(USD)`, the library validates the rate's `from` currency matches. This catches misapplied rates at runtime rather than silently producing wrong numbers.

### Authorization at the service boundary

`SecurityContext` is checked in `UserDataService::get_trades`, not in calculation logic. Calculations don't know about users or permissions — they operate on positions. The engine handles the wiring.

## Error Handling

All fallible operations return `CalceResult<T>` (alias for `Result<T, CalceError>`). Error variants carry contextual data for diagnostics:

```rust
CalceError::PriceNotFound { instrument: InstrumentId, date: NaiveDate }
CalceError::FxRateNotFound { from: Currency, to: Currency, date: NaiveDate }
CalceError::Unauthorized { requester: UserId, target: UserId }
```

See [BACKLOG.md](BACKLOG.md) for planned improvements to error handling (checked Money arithmetic, missing data strategies).

## Module Map

| Module | Purpose |
|---|---|
| `domain::currency` | `Currency([u8; 3])` — Copy, stack-allocated ISO 4217 |
| `domain::quantity` | `Quantity(Decimal)` — Add, Neg, Sum |
| `domain::price` | `Price(Decimal)` — execution or market price |
| `domain::money` | `Money { amount, currency }` — from_position, convert |
| `domain::fx_rate` | `FxRate { from, to, rate }` — directed exchange rate |
| `domain::trade` | `Trade` — single execution with signed qty + price |
| `domain::position` | `Position` — aggregated holding |
| `calc::aggregation` | `aggregate_positions()` — trades → positions |
| `auth` | `SecurityContext`, `Role` |
| `context` | `CalculationContext { base_currency, as_of_date }` |
| `services::market_data` | `MarketDataService` trait + in-memory impl |
| `services::user_data` | `UserDataService` trait + in-memory impl |
| `calc::market_value` | `value_positions()` — pure market value calculation |
| `calc::engine` | `CalcEngine` — orchestrates services + pure calcs |
| `error` | `CalceError` enum + `CalceResult<T>` alias |
