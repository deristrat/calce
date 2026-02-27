# Calculation Reference

Specification of calculation methodology used in Calce.

Each calculation is tagged (e.g. `#CALC_MV`). The same tag appears in the
implementing source code, enabling cross-referencing between specification and
implementation via simple text search.

---

## 1. Assumptions

- Markets are liquid and positions can be valued at the last observed price.
- FX rates are point-in-time spot rates; no bid/ask spread is modelled.
- Portfolio value is additive across positions (no netting or margin offsets).
- No intraday granularity; all calculations operate on daily snapshots.

## 2. Conventions

**Base currency** — All top level results for a user are expressed in a single base
currency (e.g. SEK). Cross-currency positions are converted to base currency
using the applicable FX rate. The base currency is a parameter to every
calculation that produces monetary totals.

**Signed quantities** — Positive = long, negative = short. A buy trade adds
positive quantity; a sell adds negative. Net quantity determines the current
direction of a position.

**FX rate directionality** — Rates always carry explicit `from` and `to`
currencies. `FxRate(USD, SEK, 10.5)` means 1 USD = 10.5 SEK. Conversion
validates that the rate direction matches the source currency.

## 3. Market Data

**Instrument prices** — Daily close prices. For a given valuation date T, the
price used is typically T-1 close (last available end-of-day price).

**FX rates** — Daily spot rates including the current date. Rates are directed:
an `FxRate(from, to, rate)` means 1 unit of `from` = `rate` units of `to`.

**Temporal scope** — All market data lookups are keyed by date. If data is
missing for a requested date the calculation fails explicitly — no interpolation
or fill-forward. This means valuations on non-business days (weekends, holidays)
will fail unless market data is explicitly provided for those dates.

## 4. Calculations

### 4.1 Position Aggregation `#CALC_POS_AGG`

Derives current holdings from trade history.

Given a set of trades and a valuation date T:

    net_quantity(instrument) = sum of trade.quantity
                               for all trades where trade.date <= T

Positions with net quantity of zero (fully closed) are excluded from the result.

---

### 4.2 Market Value `#CALC_MV`

Values each position at current market prices, converting to base currency
where needed.

For each position:

    market_value        = quantity * price(instrument, T)
    market_value_base   = market_value * fx_rate(position_ccy, base_ccy, T)

When position currency equals base currency, no FX conversion is applied.

Portfolio total:

    total = sum of market_value_base across all positions

---

### 4.3 Value Change `#CALC_VCHG`

Measures the change in portfolio market value between two points in time.

Given portfolio value V(T) and a prior value V(T-n):

    change     = V(T) - V(T-n)
    change_pct = change / V(T-n)

Percentage change is undefined when V(T-n) = 0.

**Standard periods:**

| Period  | Comparison date              |
|---------|------------------------------|
| Daily   | T - 1 day                    |
| Weekly  | T - 7 days                   |
| Yearly  | T - 1 year (leap-year safe)  |
| YTD     | Dec 31 of previous year      |

Leap year handling: when T is Feb 29 and the prior year has no Feb 29, Feb 28
is used.

---

### 4.4 Portfolio Report `#CALC_REPORT`

Composed view that bundles market value and value changes into a single result,
avoiding redundant computation.

Internally:

1. Aggregate trades into positions (`#CALC_POS_AGG`)
2. Value positions at current date (`#CALC_MV`) → `MarketValueResult`
3. Pass the pre-computed current snapshot into value change summary (`#CALC_VCHG`)

The current-date market value is computed once and shared between the MV result
and the value change calculation.

Result:

    PortfolioReport {
        market_value:   MarketValueResult,    // positions + total
        value_changes:  ValueChangeSummary,   // daily/weekly/yearly/YTD
    }

---

## 5. Accounting

### 5.1 Ledger Balance `#CALC_LEDGER_BAL`

Sums ledger entries to produce an exact balance. Uses fixed-point decimal
arithmetic to guarantee that debits and credits balance to zero without
floating-point rounding errors.

    balance = sum of entry.amount for all entries

All entries must share the same currency; mixed-currency summation is rejected.
