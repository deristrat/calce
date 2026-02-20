# Backlog

Deferred improvements and future work, roughly prioritized.

## High Priority

### Checked Money arithmetic

`Money::Add` panics on currency mismatch. `Money::Sum` panics on empty iterator. In a batch processing scenario, one bad data point crashes the entire run.

Options:
- Remove `Add`/`Sum` trait impls, replace with explicit `money.checked_add(other) -> CalceResult<Money>`
- Or keep operator overloads but make them return `Result` (non-standard but possible via a wrapper)
- `Sum` should handle empty iterators gracefully (return `Money::zero` with a provided currency)

This ties into the broader missing data strategy below.

### Missing data handling strategy

When a calculation encounters missing data (no price for an instrument, no FX rate for a pair), should it:

1. **Fail fast** — return error immediately (current behavior)
2. **Skip and report** — exclude the position, include it in a warnings/skipped list
3. **Partial results** — return what we can calculate + a list of failures

Option 3 is likely the right production answer. A `CalculationResult<T>` wrapper:
```rust
struct CalculationResult<T> {
    result: T,
    warnings: Vec<CalceWarning>,
    skipped: Vec<SkippedPosition>,
}
```

This is a significant design decision that affects every calculation function signature.

## Medium Priority

### Trade ID for audit trails

`Trade` currently has no unique identifier. Needed for:
- Deduplication (same trade arriving twice from different sources)
- Reconciliation (matching trades to broker confirmations)
- Audit logs (which trades contributed to a position)

### Split MarketDataService

Prices and FX rates often come from different sources (Bloomberg vs ECB). Consider splitting into `PriceService` and `FxRateService` for independent composition and caching.

### Per-module error types

Single `CalceError` enum will grow large. Consider per-module errors that compose via `#[from]`:
```rust
// calc/market_value.rs
enum MarketValueError { PriceNotFound(...), FxRateNotFound(...) }

// error.rs
enum CalceError {
    MarketValue(#[from] MarketValueError),
    UserData(#[from] UserDataError),
    ...
}
```

## Low Priority

### InstrumentId with cheap cloning

`InstrumentId(String)` allocates on clone. For hot paths (HashMap lookups), consider `InstrumentId(Arc<str>)` for refcount-based cloning. Profile before optimizing.

### Price currency

`Price` is just a `Decimal` with no currency. In reality, a price has a currency (the instrument's listing currency). Currently the position's currency serves this role. Consider `Price { amount: Decimal, currency: Currency }` if cross-listed instruments become relevant.
