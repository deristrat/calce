# Rust Implementation Guidelines

Coding conventions and patterns specific to this codebase. Add to this as patterns emerge.

## Workspace Architecture

Four crates with clear responsibilities:

```
calce-core          ← no heavy deps (chrono, decimal, thiserror)
    ↑         ↑
calce-data    calce-python
(sqlx, etc)   (pyo3)
    ↑
calce-api
(axum, tokio)
```

| Crate | Responsibility | Heavy deps |
|-------|---------------|------------|
| `calce-core` | Domain types, service traits, calc logic, reports | None |
| `calce-data` | Real DB implementations of service traits | sqlx, tokio |
| `calce-api` | HTTP server, wires data + core together | axum, tokio |
| `calce-python` | Python bindings, wraps core | pyo3 |

**Key rules:**
- `calce-core` never depends on DB or async runtimes. It stays pure and fast to compile.
- Service traits (`MarketDataService`, `UserDataService`) are defined in `calce-core`. `calce-data` implements them with real databases.
- `calce-python` depends only on `calce-core`, keeping the cdylib small and free of DB deps.
- `calce-api` is the only crate that depends on both `calce-core` and `calce-data`.

## Market Data Loading

`MarketDataBuilder` (calce-data) accumulates prices, FX rates, and instrument metadata, then `ConcurrentMarketData::from_builder()` materialises it into a lock-free concurrent store. This is used both at startup (Postgres bulk-load) and in tests.

Calc functions take `&dyn MarketDataService`, satisfied by `ConcurrentMarketData` at runtime and by `TestMarketData` (calce-core) in unit tests.

## Domain Types Are Data Carriers

Domain types (`domain/`) hold data and nothing more. They provide:

- **Construction and validation** — `new`, `try_new`
- **Accessors** — `value()`, `as_str()`
- **Same-type arithmetic** — `Quantity + Quantity`, `Money + Money`
- **Algebraic properties of the type itself** — `FxRate::invert()`, `FxRate::identity()`
- **Intrinsic conversions** — `Money::convert(rate)` (currency conversion is fundamental to what money *is*)

They do **not** contain:

- Business logic that combines multiple domain types for a purpose (e.g. aggregating trades into positions)
- Service dependencies or I/O
- Application-specific computations

All business logic lives in `calc/`. If you're writing a function that operates *on* domain types for a business purpose, it belongs in `calc/`, not on the type itself.

```rust
// Good — Money::convert is intrinsic to Money (like unit conversion)
let sek = money_usd.convert(&usd_to_sek)?;

// Good — aggregate_positions is business logic, lives in calc/
let positions = aggregation::aggregate_positions(&trades, as_of_date);

// Bad — pricing logic on a domain type
impl Position {
    fn market_value(&self, price: Price) -> Money { ... }  // belongs in calc/
}
```

## No Dead Code

Don't add trait impls, error variants, or functions speculatively. Add them when they're needed. Unused code is confusing — readers wonder "where is this used?" and find nothing.

If a refactor makes something unused (e.g. removing `TradeSide` made `Quantity::Neg` dead), remove it in the same change.

## Module Structure

Use the simplest module structure that works:
- Single file → `src/foo.rs` (not `src/foo/mod.rs`)
- Multiple files → `src/foo/mod.rs` + `src/foo/bar.rs`

Don't create a directory for a single file.

## HashMap Key Types

**Use domain types as HashMap keys, not raw strings.**

Bad — allocates a String on every lookup:
```rust
// Key is String, every lookup does .as_str().to_string()
prices: HashMap<(String, NaiveDate), Price>

self.prices.get(&(instrument.as_str().to_string(), date))
```

Good — Currency is Copy, zero-allocation lookup:
```rust
fx_rates: HashMap<(Currency, Currency, NaiveDate), FxRate>

self.fx_rates.get(&(from, to, date))  // no allocation
```

Acceptable — InstrumentId as key requires clone on lookup, but gives type safety:
```rust
prices: HashMap<(InstrumentId, NaiveDate), Price>

self.prices.get(&(instrument.clone(), date))  // clones, but type-safe
```

**Rationale:** `Currency` is `Copy` (backed by `[u8; 3]`), so using it directly as a key avoids the `String` round-trip entirely. For `InstrumentId` (backed by `String`), the clone cost is the same as `to_string()`, but you get compile-time assurance that you're using the right type.

**Future optimization:** If profiling shows `InstrumentId` cloning is hot, consider `Arc<str>` backing or a two-level HashMap (`HashMap<InstrumentId, HashMap<NaiveDate, Price>>`).

## Copy Types

Types that are small and frequently passed around should be `Copy`:
- `Currency` — `[u8; 3]`, always Copy
- `Quantity`, `Price`, `FxRate`, `Money` — all backed by `f64` which is Copy
- `NaiveDate` — Copy by default

This avoids unnecessary cloning and makes HashMap key usage cheap.

Note: `f64`-backed types derive `PartialEq` but **not** `Eq` or `Hash` (IEEE 754 NaN breaks reflexivity). If you need a HashMap key with an f64 field, wrap it or use an ordered container.

## Derive What You Can

Prefer `#[derive(...)]` over manual trait impls when the derived behavior is correct:

```rust
// Good — derive does exactly what the manual impl would
#[derive(Default)]
pub struct MarketDataBuilder {
    prices: HashMap<...>,
    fx_rates: HashMap<...>,
}

// Good — thiserror derive for error types, even in domain modules
#[derive(Debug, Clone, thiserror::Error)]
#[error("FX rate expects {expected}, but money is in {actual}")]
pub struct CurrencyMismatch { ... }
```

Manual impls are only warranted when the derived behavior is wrong (e.g. `Currency`'s custom `Debug` that shows the string instead of raw bytes).

## Newtype Constructors

Domain newtypes use `::new()` constructors, not `From` impls. This keeps construction explicit and avoids accidental conversions:

```rust
// Good — explicit
let qty = Quantity::new(100.0);
let price = Price::new(50.0);

// Avoid — implicit conversion could hide bugs
impl From<f64> for Quantity { ... }
let qty: Quantity = 100.0.into();  // too easy to mix up types
```

**For types with validation**, use the dual constructor pattern:

- `try_new(input) -> Result<Self, Error>` — fallible, for runtime/user data
- `new(input) -> Self` — panicking convenience, calls `try_new` internally. For known-valid constants and tests only.

```rust
// Currency validates A-Z only
let usd = Currency::new("USD");           // tests, known constants
let parsed = Currency::try_new(input)?;   // runtime data from files, APIs
```

Simple newtypes without validation (e.g. `Quantity`, `Price`) only need `new`.

**String ID newtypes** use the `string_id!` macro (defined in `domain/mod.rs`) to avoid boilerplate. It generates `new(impl Into<String>)`, `as_str()`, `Display`, and the standard derives (`Clone, Debug, PartialEq, Eq, Hash`):

```rust
string_id!(AccountId);
string_id!(InstrumentId);
```

Use this for any newtype that wraps a `String` with no validation. Types with validation (e.g. `Currency`) still get manual impls.

## Standard Trait Impls for Domain Types

Implement standard Rust traits where they make sense:

- **`FromStr`** — for any type constructible from a string. Delegates to `try_new`:
  ```rust
  impl FromStr for Currency {
      type Err = InvalidCurrencyCode;
      fn from_str(s: &str) -> Result<Self, Self::Err> { Self::try_new(s) }
  }
  // Enables: let usd: Currency = "USD".parse()?;
  ```

- **`AsRef<str>`** — for types that can be viewed as a string without allocation:
  ```rust
  impl AsRef<str> for Currency { fn as_ref(&self) -> &str { self.as_str() } }
  ```

- **`Display`** — for user-facing output. `Debug` — for developer diagnostics.

Don't implement traits speculatively — add them when needed.

## No Unnecessary `unsafe`

Don't use `unsafe` to skip trivial checks. The cost of safe validation on small data (e.g. `from_utf8` on 3 bytes) is negligible, and `unsafe` makes code harder to audit and maintain. Only reach for `unsafe` when profiling proves a safe alternative is a bottleneck.

## No Panics in Production Code

Production code must never panic. Enforce with crate-level deny lints:

```rust
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
```

Use `Result`/`Option` propagation (`?`) for all fallible operations, including cache deserialization, parsing, and external data.

**Exceptions where panicking is acceptable:**

- **`main()` / startup** — if configuration required to operate the system is missing or invalid (env vars, DB connection, port binding), panicking with a clear message is fine. There's nothing useful the server can do without these.
- **Validated constructors** — `Currency::new("USD")` panics on invalid input, but is only used with known-valid constants. Document with `# Panics`. The fallible `try_new()` exists for runtime data.
- **Test code** — `.unwrap()` / `.expect()` in `#[cfg(test)]` modules are fine.

For anything that processes runtime data (user input, files, cache, external APIs), always return errors — never panic.

## Error Handling Patterns

**Domain types define their own small errors.** The top-level `CalceError` aggregates them via `#[from]`:

```rust
// domain/money.rs — small, focused error using thiserror
#[derive(Debug, Clone, thiserror::Error)]
#[error("FX rate expects {expected}, but money is in {actual}")]
pub struct CurrencyMismatch { pub expected: Currency, pub actual: Currency }

// error.rs — aggregates domain errors
enum CalceError {
    CurrencyMismatch(#[from] CurrencyMismatch),
    ...
}
```

This avoids circular dependencies (domain types don't import `CalceError`) and keeps each module self-contained.

**Only add error variants that are used.** Don't add speculative variants — they create noise and make the error enum harder to match on exhaustively.

## Test Patterns

**Calculation function tests** — construct positions and a `MarketDataBuilder` → `ConcurrentMarketData`. No auth, no user data, just the calculation:
```rust
let mut b = MarketDataBuilder::new();
b.add_price(&aapl, date, Price::new(150.0));
let md = ConcurrentMarketData::from_builder(b);
let ctx = CalculationContext::new(usd, date);
let outcome = value_positions(&positions, &ctx, &md).unwrap();
assert_eq!(outcome.value.total.amount, 15000.0);
```

**Integration tests** — full pipeline including auth, trade loading, and aggregation:
```rust
let security_ctx = SecurityContext::new(alice.clone(), Role::User);
let trades = user_data.get_trades(&security_ctx, &alice).unwrap();
let positions = aggregate_positions(&trades, date).unwrap();
let outcome = value_positions(&positions, &ctx, &market_data).unwrap();
```

Calculation tests are fast and focused. Integration tests verify the wiring (auth, aggregation, data flow).
