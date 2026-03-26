# Rust Finance Developer Guide

> For developers and LLMs working on our portfolio tracking platform.
> Companion to: [rust-finance-research.md](rust-finance-research.md) | [decisions.md](decisions.md)

---

## Crate Stack

### Core (always use)

| Crate | Purpose |
|-------|---------|
| `tokio` | Async runtime |
| `axum` | Web framework |
| `sqlx` | Async PostgreSQL (compile-time checked) |
| `serde` / `serde_json` | Serialization |
| `uuid` | Entity IDs |
| `chrono` | Dates and timestamps |
| `thiserror` | Error types in library/domain code |
| `anyhow` | Error propagation in app/handler code |
| `tracing` | Structured logging |

### Numerical (Monte Carlo, risk, analytics)

| Crate | Purpose |
|-------|---------|
| `faer` | Cholesky decomposition, matrix multiply, SVD — **8x faster than OpenBLAS, pure Rust** |
| `ndarray` | Array data container for time series and return matrices |
| `statrs` | Statistical distributions (Normal, t, etc.) for VaR |
| `rand` / `rand_distr` | RNG for Monte Carlo |
| `rayon` | Parallel iteration (Monte Carlo paths, batch processing) |
| `rust_decimal` | Exact arithmetic for ledger accounting, fee splits |

### Supporting

| Crate | Purpose |
|-------|---------|
| `reqwest` | HTTP client for market data APIs |
| `governor` | Rate limiting (API + external calls) |
| `polars` | DataFrame ops (heavy — use feature flags) |
| `csv` / `calamine` | Import CSV / Excel data |
| `bdays` / `RustQuant_time` | Business day arithmetic, holiday calendars (26 countries) |
| `proptest` | Property-based testing |
| `criterion` | Benchmarking |

---

## Numeric Types — The Rules

### When to use what

| Type | Use for | Example |
|------|---------|---------|
| `f64` | Portfolio analytics: market valuations, risk metrics, FX conversions, Monte Carlo | `Quantity`, `Price`, `Money`, `FxRate` in `domain/` |
| `rust_decimal::Decimal` | Ledger accounting: balances that must sum exactly, fee splits, pro-rata allocations | `Balance`, `LedgerEntry` in `accounting/` |
| `i64` (cents) | High-volume transaction processing where Decimal overhead matters | Settlement systems (not in this crate) |

**Why f64 for analytics?** Portfolio tracking does multiplication-heavy work (qty × price × fx_rate) where the inputs are already approximate (market prices change by the second). f64 gives native operators, feeds directly into statistical crates (`faer`, `statrs`, `ndarray`), and has zero overhead. Decimal adds friction for accuracy that doesn't matter here.

**Why Decimal for accounting?** Ledger balancing requires exact arithmetic — debits and credits must net to zero, not to 1e-15. Use `rust_decimal::Decimal` in the `accounting` module for this.

### Currency safety

Currency mismatches are caught at runtime via `Money::checked_add` and `Money::convert`, which return `Result<_, CurrencyMismatch>`:

```rust
let usd_money = Money::new(100.0, Currency::new("USD"));
let sek_money = Money::new(200.0, Currency::new("SEK"));
// Returns Err(CurrencyMismatch { .. })
let _ = usd_money.checked_add(sek_money);
```

---

## Error Handling

### Library / domain code → `thiserror`

```rust
// GOOD — callers can match on specific errors
#[derive(Debug, thiserror::Error)]
pub enum PortfolioError {
    #[error("position not found: {0}")]
    PositionNotFound(String),

    #[error("insufficient balance: need {needed}, have {available}")]
    InsufficientBalance { needed: Decimal, available: Decimal },

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
```

### App / handler code → `anyhow`

```rust
// GOOD — just propagate and log, don't enumerate every error
async fn handle_request(req: Request) -> anyhow::Result<Response> {
    let portfolio = load_portfolio(&req.user_id).await?;
    let risk = calculate_risk(&portfolio).await?;
    Ok(Response::json(&risk))
}
```

### Never do this

```rust
// WRONG — panics in production on missing data
let position = portfolio.positions.get(&name).unwrap();

// WRONG — panic with empty message (found in RustQuant)
panic!("");

// WRONG — assert in library code
assert!(!values.is_empty()); // will crash, not return an error

// RIGHT
let position = portfolio.positions.get(&name)
    .ok_or(PortfolioError::PositionNotFound(name.clone()))?;
```

---

## Async / Concurrency

### The async runtime is for I/O. CPU work goes on blocking threads.

```rust
// WRONG — Monte Carlo blocks the async runtime, starving HTTP handlers
async fn handle_risk_request(portfolio: Portfolio) -> Result<RiskResult> {
    let result = run_monte_carlo(&portfolio); // blocks for seconds
    Ok(result)
}

// RIGHT — offload to blocking thread pool
async fn handle_risk_request(portfolio: Portfolio) -> Result<RiskResult> {
    let result = tokio::task::spawn_blocking(move || {
        run_monte_carlo(&portfolio) // rayon parallelism inside here
    }).await??;
    Ok(result)
}
```

### Never hold locks across await points

```rust
// WRONG — deadlock risk, blocks other tasks
async fn update_cache(cache: Arc<Mutex<Cache>>) {
    let mut c = cache.lock().unwrap();
    let data = fetch_market_data().await; // lock held during I/O!
    c.insert(data);
}

// RIGHT — fetch first, then lock briefly
async fn update_cache(cache: Arc<Mutex<Cache>>) {
    let data = fetch_market_data().await;
    let mut c = cache.lock().unwrap();
    c.insert(data);
}
```

### Always wrap external calls in timeouts

```rust
// WRONG — hung API call blocks indefinitely
let response = reqwest::get("https://api.marketdata.com/prices").await?;

// RIGHT — explicit timeout
let response = tokio::time::timeout(
    Duration::from_secs(5),
    reqwest::get("https://api.marketdata.com/prices")
).await
    .map_err(|_| AppError::MarketDataTimeout)??;
```

### Prefer channels over shared state for pipelines

```rust
// GOOD — message-passing pipeline
let (tx, mut rx) = tokio::sync::mpsc::channel(100);

// Producer
tokio::spawn(async move {
    for order in orders {
        tx.send(order).await.ok();
    }
});

// Consumer
while let Some(order) = rx.recv().await {
    process_order(order).await;
}
```

---

## Type System Patterns

### Newtype wrappers — prevent mixing incompatible values

```rust
pub struct AccountId(pub Uuid);
pub struct PortfolioId(pub Uuid);
pub struct Weight(pub f64);

// COMPILE ERROR: can't pass AccountId where PortfolioId expected
fn load_portfolio(id: PortfolioId) -> Result<Portfolio> { ... }
```

### State machines via phantom types — invalid states are unrepresentable

```rust
pub struct Pending;
pub struct Approved;
pub struct Settled;

pub struct Trade<State> {
    id: TradeId,
    amount: Money<USD>,
    _state: PhantomData<State>,
}

impl Trade<Pending> {
    pub fn approve(self, approver: &str) -> Trade<Approved> {
        Trade { id: self.id, amount: self.amount, _state: PhantomData }
    }
    // No settle() method — can't skip Approved state
}

impl Trade<Approved> {
    pub fn settle(self) -> Trade<Settled> {
        Trade { id: self.id, amount: self.amount, _state: PhantomData }
    }
}

// COMPILE ERROR: can't call settle on a Pending trade
// let settled = pending_trade.settle();
```

---

## Linear Algebra — faer for Monte Carlo

Use `faer` for Cholesky decomposition of correlation matrices (correlated Monte Carlo paths), matrix multiplication, and any large-matrix operations. It is 8x faster than OpenBLAS at Cholesky for 1024×1024, pure Rust, no Fortran dependency.

Use `nalgebra` only for small utility matrix work (< 100×100) outside the hot path.

```
Monte Carlo stack:
faer (Cholesky) + ndarray (data layout) + statrs (distributions) + rand (RNG) + rayon (parallelism)
```

---

## Logging

Use `tracing` with structured fields. Never `println!()`.

```rust
use tracing::{info, warn, error};

// GOOD — structured, queryable in production
info!(
    user_id = %principal.user_id,
    portfolio_id = %portfolio.id,
    num_positions = portfolio.positions.len(),
    "running Monte Carlo simulation"
);

// GOOD — error with context
error!(
    user_id = %principal.user_id,
    error = %e,
    "market data fetch failed"
);

// WRONG
println!("Processing user {}", user_id);
```

---

## Crate-Level Settings

Every crate in the workspace **must** have these in `lib.rs`:

```rust
#![forbid(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(clippy::pedantic)]
```

Every pure computation function **must** have `#[must_use]`:

```rust
#[must_use]
pub fn calculate_sharpe_ratio(returns: &[f64], risk_free_rate: f64) -> f64 {
    // ...
}
```

---

## Testing

### Property-based tests for financial calculations

Edge cases in P&L and risk calcs are hard to enumerate. Let `proptest` find them.

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn portfolio_weights_sum_to_one(
        values in prop::collection::vec(1.0f64..1000.0, 1..20)
    ) {
        let total: f64 = values.iter().sum();
        let weights: Vec<f64> = values.iter().map(|v| v / total).collect();
        let weight_sum: f64 = weights.iter().sum();
        assert!((weight_sum - 1.0).abs() < 1e-10);
    }
}
```

### Benchmark hot paths with criterion

```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_cholesky(c: &mut Criterion) {
    let matrix = generate_correlation_matrix(50);
    c.bench_function("cholesky_50x50", |b| {
        b.iter(|| faer_cholesky(&matrix))
    });
}
```

---

## Quick Reference — Do / Don't

| Do | Don't |
|----|-------|
| `Result<T, E>` + `?` operator | `.unwrap()` / `.expect()` in library code |
| `f64` for analytics, `Decimal` for accounting | Mixing numeric types without clear rationale |
| `tracing::info!(field = %val, "msg")` | `println!()` |
| `tokio::task::spawn_blocking` for CPU work | Heavy computation on the async runtime |
| `tokio::time::timeout` on external calls | Unbounded async calls to APIs/DBs |
| Lock → work → drop → await | Hold `Mutex` across `.await` |
| `thiserror` for domain errors | `String` as error type |
| `#[must_use]` on pure functions | Silent discard of computed results |
| `proptest` for financial invariants | Only hand-written test cases |
| `&[T]` references in hot paths | `.clone()` in tight loops |
| Feature flags for heavy deps (`polars`) | Unconditional heavy dependencies |
| `faer` for large matrix ops | `nalgebra` native for >100×100 matrices |
