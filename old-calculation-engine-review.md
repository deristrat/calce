# Old Calculation Engine Review

Review of the Python calculation engine in `njorda/services/api/src/njorda/libs/calculations/`.

---

## Architecture Overview

```
calculations/
├── core/           — Domain types (Price, Position, Account, Trade), exceptions
├── context/        — CalculationContext (DI container), InstrumentParameters (rule engine)
├── batch_calcs/    — Grouping, dispatch, table-oriented output, time series iteration
├── calc_funcs/     — Individual calculation implementations (~50 named functions)
├── pnl/            — P&L engine (FIFO/avg cost, TWR, realized/unrealized)
├── pensions/       — Swedish pension schemes (AKAP-KR, KAP-KL)
├── scenario/       — Deterministic stress testing
├── simulation/     — Monte Carlo (single + correlated multi-asset GBM)
├── math/           — Linear algebra (nearest positive-definite matrix)
├── summaries/      — Report assembly at position/account/client level
└── util/           — Lookup tables (sectors, categories, currencies, instrument types)
```

**Pattern:** Calculations are plain functions `(context, group, **kwargs) -> result` registered in a static dict. No decorator-based registration. Dispatch is by string name lookup. The `CalculationContext` carries the date, base currency, market data interface, settings, and instrument parameter rules. It is immutable-by-convention (mutations produce copies via `dataclasses.replace`).

---

## Calculations Inventory

### Core Valuation
- **Market value** — per-position, per-account (positions + cash), portfolio-level
- **Historical market value** — time series over date range, both from positions and from trades
- **Cash value** — sum of account cash balances, FX-converted
- **Loan value** — sum of account loan balances, FX-converted
- **Instrument price** — latest or historical, with close > mid(bid,ask) > last priority

### FX
- **Spot conversion** — single price to base currency (`amount / rate`, rate = foreign-per-base)
- **Historical batch conversion** — bulk date-keyed FX rates for price series

### Performance / P&L
- **TWR (Time-Weighted Return)** — two implementations:
  - `_invapp_performance.py`: chained daily MV ratios with AdjustedMarketValue (same-day trades at acquire cost)
  - `pnl_calculations.py`: Modified Dietz single-flow adjustment, chain-linked daily
- **Realized P&L** — FIFO and Average Cost methods
- **Unrealized P&L** — mark-to-market vs acquisition cost per lot
- **Fee P&L, Dividend P&L, Interest P&L** — tracked separately
- **Daily change (TWR and Total P&L)**
- **Net change** — absolute and relative (MV vs acquire cost)

### Risk
- **VaR (Value at Risk)** — Monte Carlo only (no parametric or historical VaR)
  - Single-instrument and multi-asset (correlated)
  - Separate quantiles for standard VaR and PRIIPs VaR
- **Volatility** — annualized std of log returns, auto-detects daily (252) vs monthly (12) trading
- **Correlation** — Pearson on weekly (Friday) log returns
- **Sharpe ratio** — historical mean-variance with annualization
- **PRIIPs MRM Category 2** — Cornish-Fisher analytical VaR
- **PRIIPs MRM Category 3** — Bootstrap simulation VaR (10k iterations)
- **VEV (Volatility Equivalent Variance)** — from VaR via Cornish-Fisher inverse

### Portfolio Optimization
- **Max Sharpe** — Markowitz mean-variance via scipy SLSQP and Nelder-Mead
- **Mean-variance optimization** — iso-volatility (maximize return at constant risk)
- **Efficient frontier** — 200-point sweep of target returns
- **Proposal position sizing** — allocation % to quantities given a target portfolio value

### Scenario / Stress Testing
- **Deterministic factor-based shifts** — `new_value = value * (1 + shift * sensitivity)`
- **Risk factor types**: sector, geography, instrument type, currency, product list
- **Preloaded scenarios**: GFC 2008, COVID-19 2020, Dot-com 2000

### Allocation
- **Market-value-weighted look-through** across 7+ dimensions:
  - Sector, Geography, Currency, Instrument Type, Product List, Asset Class, Top Holdings
  - Plus Morningstar Category Group and Category Name
- Normalization/capping when allocations exceed 100%

### Expected Return
- **Per-instrument**: priority chain (position override > pre-calc parameter > calculated > post-calc parameter)
- **Portfolio-level**: compound growth model `future_value = sum(q * p * (1+r)^t)`

### Pensions (Swedish-specific)
- **AKAP-KR** — defined contribution, two-tier premiums (6%/31.5%), birth-year extra premiums
- **KAP-KL** — hybrid DC + defined benefit ("manspension"), birth-year-dependent rates
- Present-value annuity math with salary growth, mortality adjustment, return tax

### Summaries / Reports
- Three aggregation levels: position, account, client
- Compose: summary + positions + allocations + risk scenarios + P&L + risk metrics
- Calculation cache with org/client/account/dates key

---

## Domain Objects Needed

### Value Types
| Type | Fields | Notes |
|------|--------|-------|
| `Price` | amount, currency, date, is_fallback | Arithmetic ops, date-snapping for holidays |
| `MonetaryAmount` | amount, currency | Undated value |
| `FxRate` | base, quote, rate | Convention: `amount / rate` converts quote->base |

### Entity Types
| Type | Fields | Notes |
|------|--------|-------|
| `Position` | ticker, quantity, position_type, overrides, metadata | External positions carry their own price |
| `Trade` | ticker, quantity, acquire_price, date | Links back to source record |
| `Account` | id, type, currency, cash_balance, loan_balance | Container for positions |
| `PortfolioPositions` | account, positions[] | The input unit for batch calcs |
| `Transaction` | type, amount, currency, fx_rate, date, instrument, quantity, fee | 21 transaction types, ~8 actively processed |

### Instrument Metadata
| Type | Fields | Notes |
|------|--------|-------|
| `InstrumentInfo` | ticker, type, subtype, currency, markets[], sectors[], top_holdings[], category | 16 primary types, many subtypes |
| `InstrumentParameters` | rules[] | Rule-based parameter resolution (5 match levels) |

### Calculation Parameters
| Parameter | Per-instrument | Notes |
|-----------|---------------|-------|
| `expected_return` | Yes | 4-level priority chain |
| `volatility` | Yes | 4-level priority chain, auto-detect monthly/daily |
| `correlation_proxy` | Yes | 3-level proxy resolution + synthetic fallback |

### Classification Enums
- **InstrumentType**: 16 types (STOCK, MFUND, ETF, BOND, CERT, OPTION, WARRANT, etc.)
- **InstrumentSubtype**: ~30 subtypes nested under types
- **Sector**: 16 values (Basic Materials through Utilities + Other/Unmapped/No Info)
- **AllocationType**: 9 dimensions (sector, geography, currency, instrument_type, product_list, asset_class, category_group, category_name, top_holdings)
- **CategoryGroup**: 9 Morningstar groups
- **CategoryName**: ~80 Morningstar categories
- **Currency**: 56 codes
- **TransactionType**: 21 types

---

## Complications and Potential Dead Ends

### 1. FX Rate Convention Is Division-Based and Implicit

The FX conversion uses `amount / rate` where rate means "units of foreign per 1 unit of base". This is the opposite of the more common `amount * rate` convention. The old code never explicitly documents this — it's embedded in the math. Getting this wrong inverts every cross-currency calculation.

**For calce:** Define the FX convention explicitly in the type system. An `FxRate` should know its directionality (base/quote pair) and expose a `convert(amount, from, to)` method that makes it impossible to apply the rate backwards.

### 2. Two Parallel Performance Engines

There are two completely separate TWR implementations:
- `_invapp_performance.py` — class-based, uses "adjusted market value" (same-day trades at acquire cost), chained daily MV ratios
- `pnl_calculations.py` — function-based, Modified Dietz single-flow adjustment

They can produce different results for the same portfolio. The InvApp engine also has its own `RealizedPNL` (FIFO stack) and `UnrealizedPNL`, separate from the P&L engine's AVG_COST/FIFO system.

**For calce:** Pick one TWR methodology and one P&L methodology. Don't let two competing engines accumulate.

### 3. The "FIFO" Implementation Is Actually LIFO

In `pnl_calculations.py`, `fifo_decrease_position` calls `parts.pop(-1)` which pops the *most recently added* part — that's LIFO, not FIFO. Since `fifo_new_or_increase_position` appends to the end, popping from the end takes the newest lot first.

**For calce:** If we need FIFO, implement it correctly (pop from the front / index 0). Document the cost basis method clearly. Consider supporting both FIFO and LIFO explicitly if needed for tax lot selection.

### 4. Cash Is Constant in Historical Calculations

`historical_market_value_portfolio` computes cash value once (today's balance) and adds it to every historical date. There is no concept of historical cash movements. This means historical portfolio values don't reflect actual cash balances on those dates.

**For calce:** If we need accurate historical portfolio values, we need to reconstruct cash balances from transaction history (deposits, withdrawals, dividends received, trade settlements). This is significantly more complex but necessary for correct TWR.

### 5. External Positions Are a Special Case Everywhere

External positions (user-entered assets like real estate, crypto, savings accounts) have no market data. The old code handles them with `if position.position_type == "external"` branches scattered across market value, correlation, VaR, allocation, and proposal code. They use:
- Static `price_per_unit` from metadata (no historical prices)
- Default currency of SEK if not specified
- Synthetic uncorrelated random walk for correlation (`rng.normal(0, 0.01, N)` with hash-based seed)
- `price = 1.0` and `quantity = value_in_base_currency` for VaR

**For calce:** Design external positions as a first-class concept from the start rather than bolting them on with special cases. Consider a `PriceSource` trait that has a `MarketPriceSource` and a `StaticPriceSource` implementation, so the valuation code doesn't need to branch.

### 6. The Instrument Parameter Rule Engine Is Complex

The `InstrumentParameters` system resolves calculation parameters (expected return, volatility, correlation proxy) through a 5-level priority matching system:
1. Position overrides (highest)
2. Ticker-specific rules
3. Asset list filter rules
4. Instrument type/subtype rules
5. Default rules (lowest)

Each parameter also has a before/after calculation priority that creates a second axis: override > before_calc > calculated_value > after_calc > error.

This creates a 5x4 resolution matrix that's hard to reason about and debug.

**For calce:** Simplify to a clear priority chain. Consider making the resolution explicit (return the full chain with sources for debugging) rather than just returning the winning value.

### 7. Correlation Matrix Repair (Nearest Positive-Definite)

When correlation matrices are not positive-definite (common with proxy tickers, missing data, or mixed time periods), Cholesky decomposition fails. The old code falls back to computing the nearest PD matrix via SVD + eigenvalue shifting (Higham's algorithm). The repaired matrix is an approximation that can distort correlations.

**For calce:** We will need this. Port the `nearest_positive_definite` algorithm or use a Rust linear algebra crate. Consider logging/warning when the fallback is triggered so users know the correlation data is approximate.

### 8. Monthly vs Daily Volatility Auto-Detection

The volatility calculation auto-detects whether an instrument trades monthly or daily by counting prices per month. Monthly instruments annualize with sqrt(12), daily with sqrt(252). Getting the classification wrong (e.g., an illiquid stock with sparse prices) can distort volatility by ~4.5x.

**For calce:** Consider making trading frequency explicit in instrument metadata rather than auto-detecting. Or at minimum, validate the auto-detection more robustly.

### 9. PRIIPs Non-Determinism and Date Bug

Category 3 MRM uses `np.random.choice` without a seed (non-reproducible results) and `date.today()` instead of `context.today` (ignores the calculation context date). This means the same inputs can produce different risk levels on different runs, and backtesting doesn't work.

**For calce:** Always use `context.today` and always seed random generators.

### 10. Market Data Gaps Are Handled Inconsistently

Different calculations handle missing prices differently:
- `market_value_positions`: silently drops positions with errors
- `instrument_price`: returns Price(0) if `ignore_no_price_instruments` is true, raises otherwise
- `historical_market_value_positions`: raises or skips depending on settings
- Price history: forward-fills then back-fills NaNs
- FX rates: raises `MissingFxRateError` on gaps (no interpolation)

**For calce:** Define a consistent missing-data policy. Consider: (a) what constitutes "missing" vs "no trading day", (b) when to forward-fill vs error, (c) how to report data quality issues without silently producing incorrect values.

### 11. Transaction Type Explosion

The P&L engine declares 21 transaction types but only actively processes 8 (TRADE, TRANSFER, POS_ADJUST, FEE, FEE_REFUND, DIVIDEND, INTEREST, OTHER). The remaining 13 are silently skipped with a log warning. This means TAX, DEPOSIT, WITHDRAWAL, FOREX, and others have no P&L impact.

**For calce:** Decide upfront which transaction types matter. Deposits/withdrawals affect TWR (they're external flows). Tax affects realized returns. Forex transactions affect FX P&L. Silently ignoring them can produce incorrect results.

### 12. Proposal/Optimization Is Tightly Coupled to the Calculation Engine

The portfolio optimization code (`max_sharpe.py`, 1167 lines) calls into VaR, volatility, correlation, expected return, and FX conversion. It uses scipy's SLSQP and Nelder-Mead optimizers, multiple random starting points, epsilon-perturbation for numerical stability, and a complex `_is_close_enough` convergence check.

**For calce:** This is a large, complex module. Consider whether optimization belongs in the core calculation engine or should be a separate higher-level crate that depends on calce for the primitives.

### 13. Sign Convention for Loans Is Implicit

`loans_value_portfolio` sums `account.loan_balance` without negation. Whether the result represents a liability (negative) or the absolute balance depends entirely on whether `loan_balance` is stored as negative in the database.

**For calce:** Make sign conventions explicit in the type system. Consider separate `Asset` and `Liability` types, or at minimum document and enforce the convention.

### 14. Pension Calculations Are Sweden-Specific and Rule-Heavy

The pension module hard-codes Swedish income base amounts (72,600 SEK), birth-year-dependent premium tables (1946-1985), mortality adjustment tables, and the 0.38% return tax (avkastningsskatt). The KAP-KL "manspension" defined-benefit component has its own salary-band logic.

**For calce:** These are highly localized calculations. Consider whether they belong in the core engine or in a separate Sweden-specific module. The constants will need regular updates as regulations change.

### 15. The Batch/Table Output Model Mixes Concerns

The `calc_grouped_table` function groups positions, dispatches calculations, normalizes heterogeneous return types (Price, float, str, dict) into `CellResult`, catches exceptions per-cell, and yields progress updates. This mixes orchestration, error handling, serialization, and presentation.

**For calce:** Keep the calculation functions pure and push grouping/error-handling/serialization to a separate layer.

---

## Scale Assessment

### What Needs to Be Built (roughly by priority)

**Phase 1 — Core primitives:**
- Domain types (Price, Money, FxRate, Position, Trade, Account, Transaction)
- FX conversion (spot + historical)
- Market value calculation (position, portfolio)
- Price resolution (close > mid > last, historical, fallback)
- Instrument metadata and parameter resolution

**Phase 2 — P&L and Performance:**
- Transaction processing (8+ active types)
- Cost basis tracking (AVG_COST and/or FIFO)
- Realized and unrealized P&L
- TWR calculation (Modified Dietz or chained daily ratios)
- Historical market value time series

**Phase 3 — Risk:**
- Volatility (from price history, with annualization)
- Correlation matrix (from weekly returns, with proxy resolution)
- Monte Carlo simulation (single + correlated multi-asset GBM)
- VaR (from Monte Carlo quantiles)
- Nearest positive-definite matrix repair

**Phase 4 — Analytics:**
- Sharpe ratio
- Allocation breakdown (7+ dimensions with look-through)
- Expected return (per-instrument and portfolio-level)
- Scenario/stress testing

**Phase 5 — Specialized:**
- Portfolio optimization (Markowitz mean-variance)
- Efficient frontier
- PRIIPs risk measures (Category 2 Cornish-Fisher, Category 3 bootstrap)
- Pension calculations (if needed)
- Summary/report assembly

### Rough Line Count of Core Logic (excluding tests, imports, boilerplate)
- Market value + price + cash + loans + FX: ~400 lines
- Performance / P&L (both engines): ~1200 lines
- Risk (VaR, volatility, correlation, Monte Carlo, linear algebra): ~900 lines
- Optimization (max Sharpe, efficient frontier): ~1200 lines
- Allocation: ~500 lines
- Pensions: ~600 lines
- Scenario: ~250 lines
- PRIIPs: ~150 lines
- Core/context/batch framework: ~600 lines
- Utilities/lookups: ~800 lines
- **Total: ~6,600 lines of calculation logic**

This is a substantial engine. The Rust rewrite should be more concise (no ORM boilerplate, stronger types eliminate defensive checks) but the domain complexity remains.
