# Calce

Financial calculation engine for portfolio tracking.

## Architecture

```
CalcEngine (orchestration) — wires services to pure functions
├── accounting/ — exact-precision ledger arithmetic (Decimal)
├── calc/      — pure business logic, no side effects
├── reports/   — composed views bundling multiple calc primitives
├── services/  — trait-based data access, in-memory test impls
└── domain/    — data types only, no business logic
```

Domain types are data carriers. Business logic belongs in `calc/`.
Intrinsic operations (e.g. `Money::convert`, `FxRate::invert`) are fine on domain types.

## Numeric Types

| Type | Use for | Module |
|------|---------|--------|
| `f64` | Market valuations, risk metrics, FX conversions, portfolio analytics | `domain/`, `calc/` |
| `rust_decimal::Decimal` | Ledger balancing, fee splits, any arithmetic that must be exact | `accounting/` |

Domain types (`Quantity`, `Price`, `Money`, `FxRate`) use `f64`. They derive `PartialEq` but **not** `Eq` (f64 is not `Eq`).

The `accounting` module uses `Decimal` for exact ledger arithmetic where debits and credits must balance to zero.

## Comments

Only comment when the comment adds value that the code doesn't already convey.

**Do comment:**
- Non-obvious domain conventions (sign conventions, currency directionality)
- `# Errors` and `# Panics` sections on public functions (required by `clippy::pedantic`)
- Why something exists when the reason isn't obvious (e.g. "Sort for deterministic output")

**Do not comment:**
- `/// Create a new X` — the function is called `new`
- `/// Returns the Y` — the function is called `y()` or `get_y()`
- `/// The Z field` — the field is named `z`
- Module declarations (`pub mod foo`)
- Struct/enum definitions when the name is descriptive
- Enum variants when the variant name + error message are clear

**Rule of thumb:** if the doc comment is just the function/field/type name rephrased as a sentence, delete it.

## Calculation Reference (`docs/calculations/`)

Documentation of calculation methodology, assumptions, and formulas.

Each calculation has a tag (e.g. `#CALC_MV`) that appears in both the
methodology doc and the implementing function's doc comment. To trace from
spec to code or vice versa: `grep -r CALC_MV`.

| Tag                | Calculation            | Source                    |
|--------------------|------------------------|---------------------------|
| `#CALC_POS_AGG`    | Position aggregation   | `calc/aggregation.rs`     |
| `#CALC_MV`         | Market value           | `calc/market_value.rs`    |
| `#CALC_VCHG`       | Value change           | `calc/value_change.rs`    |
| `#CALC_LEDGER_BAL` | Ledger balance         | `accounting/balance.rs`   |
| `#CALC_REPORT`     | Portfolio report       | `reports/portfolio.rs`    |

When adding a new calculation you **must**:
1. Add a section in `docs/calculations/methodology.md` with a new `#CALC_*` tag
2. Add the same tag to the implementing function's doc comment
3. Update this table

When making significant changes in calculations check that documentation is up to date.

## Development

```sh
cargo build
cargo test
cargo clippy -- -D warnings
```
