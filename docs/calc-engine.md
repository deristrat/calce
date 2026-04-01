# Calculation Engine

How the pure calculation layer in calce-core is structured. For overall system architecture, see [architecture.md](architecture.md). For calculation formulas and assumptions, see [calculations/methodology.md](calculations/methodology.md).

## Dual API

Every calculation is available in two modes:

**Stateful** — caller identifies _what_ to calculate (which user). `DataService` in calce-data loads data and packs it into in-memory services, then the API handler calls pure calc functions. Used by the HTTP API.

**Caller-provided** — caller constructs all input data (trades, market data) and passes it directly. No database access, no auth. Used for simulations, what-if analysis, and testing.

The PyO3 bindings support both modes: `CalcEngine` with manual data construction (caller-provided), or `DataService` which connects to Postgres and bulk-loads data at startup (stateful).

Both modes call the same pure `calc/` functions underneath.

## Calculation Composition

Calculations compose in layers:

1. **Primitive** — single-purpose pure function: `value_positions(positions, ctx, market_data)`
2. **Composite** — calls primitives at multiple points: `value_change_summary` calls `aggregate_positions` + `value_positions` for each comparison date, then diffs
3. **Report** (`reports/`) — bundles composites into a consumer-facing result, sharing intermediate values to avoid redundant computation

Data loading is separate from calculations: `DataService` in calce-data handles async I/O, then the API handler or caller invokes the pure calc layer.

Each level is independently testable. The pure-function design means caching/memoization can be added later by wrapping the same functions.

## Partial Results

Calculations return partial results rather than failing on the first missing data point. A portfolio with 50 positions where 1 price is missing returns 49 valued positions plus a warning.

```rust
pub struct Outcome<T> {
    pub value: T,
    pub warnings: Vec<Warning>,
}
```

Functions return `CalceResult<Outcome<T>>` — the `Result` catches structural errors (e.g. currency mismatch, aggregation conflicts) while `Outcome` collects data-quality warnings (missing prices, missing FX rates) that allow partial computation.

Currently implemented for `value_positions`, `value_change_summary`, and `portfolio_report`.
