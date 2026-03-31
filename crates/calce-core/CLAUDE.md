# calce-core

Core Rust library — no DB or async dependencies. Fast to compile, easy to test.

## Module Layout

- `domain/` — Data types only, no business logic
- `calc/` — Pure calculation functions, no side effects
- `reports/` — Composed views bundling multiple calc primitives
- `services/` — `MarketDataService` trait + `TestMarketData` fake for tests
- `context.rs` — `CalculationContext` (pure parameters: `base_currency`, `as_of_date`)
- `outcome.rs` — `Outcome<T>` — partial results with warnings

## Key Design Decisions

- Calc functions take `&dyn MarketDataService`, never a concrete implementation. `ConcurrentMarketData` (calce-data) is the runtime impl.
- `TestMarketData` (HashMap-based, no freeze step) is available for unit tests via `services::test_market_data`. It is always compiled (not `#[cfg(test)]`) so integration tests in `tests/` can use it too.
- Domain types use `f64` and derive `PartialEq` but not `Eq`.

## Lint Config

Defined in `lib.rs`:
- `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`
- `#![warn(clippy::pedantic)]`
