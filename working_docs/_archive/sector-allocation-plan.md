# Weighted Allocation Plan

Generalize portfolio allocation to support weighted, multi-dimensional
breakdowns (sector, geography, asset class, etc.) with look-through for funds.

## Problem

The current `type_allocation` (`#CALC_ALLOC`) assigns each instrument a single
`InstrumentType` and groups by that. This works because instrument type is
inherently singular — AAPL is a stock, period.

But sector (and geography, asset class, etc.) are different:
- **Stocks** have a single sector: AAPL = 100% Information Technology
- **Mutual funds / ETFs** span many sectors: SPY = ~30% Info Tech, ~13%
  Health Care, ~12% Financials, etc.
- The same applies to geography, asset class, and other dimensions

A single-label-per-instrument model would force us to label SPY as
"Uncategorized" or "Diversified", hiding the true portfolio allocation. We need
**weighted classification**: each instrument carries a `{key: weight}` map per
dimension.

## Naming

Use **"allocation"** consistently at all levels:

| Level | Term | Example |
|-------|------|---------|
| Per-instrument data | **allocation weights** | AAPL sector = {"Information Technology": 1.0} |
| Per-instrument storage | **allocations** (JSONB column) | `{"sector": {"Information Technology": 1.0}}` |
| Trait method | `get_allocations(instrument, dimension)` | Returns `Vec<(String, f64)>` |
| Portfolio-level result | **allocation** | Sector allocation: 45% Info Tech, 20% Health Care... |
| Calc tag (existing type) | `#CALC_ALLOC_INSTYPE` | Renamed from `#CALC_ALLOC` |
| Calc tag (new weighted) | `#CALC_ALLOC_WEIGHTED` | Generic weighted allocation |
| Calc tag (sector) | `#CALC_ALLOC_SECTOR` | Sector allocation (uses weighted) |

Sources: Bloomberg PORT ("sector allocation", "allocation"), Morningstar
("sector weightings", "asset allocation"), MSCI ("sector allocation").

### Tag rename: `#CALC_ALLOC` → `#CALC_ALLOC_INSTYPE`

The existing type allocation tag is renamed to make room for the family of
allocation calculations:
- `#CALC_ALLOC_INSTYPE` — allocation by instrument type (single-label, enum)
- `#CALC_ALLOC_WEIGHTED` — generic weighted allocation engine
- `#CALC_ALLOC_SECTOR` — sector allocation (calls weighted engine)
- Future: `#CALC_ALLOC_GEO`, `#CALC_ALLOC_ASSET_CLASS`, etc.

## Design: unified weighted model

### Every dimension uses `{key: weight}` maps

Instead of special-casing each dimension, every weighted allocation dimension
uses the same data shape:

```
instrument allocations:
  AAPL:
    sector:      {"Information Technology": 1.0}
    geography:   {"United States": 1.0}
    asset_class: {"Equity": 1.0}
  SPY:
    sector:      {"Information Technology": 0.30, "Health Care": 0.13, "Financials": 0.12, ...}
    geography:   {"United States": 0.99, "Other": 0.01}
    asset_class: {"Equity": 1.0}
  balanced_fund:
    sector:      {"Information Technology": 0.20, "Health Care": 0.10, ...}
    asset_class: {"Equity": 0.65, "Fixed Income": 0.35}
```

For stocks, the map has one entry at weight 1.0. For funds, it has many entries
summing to ~1.0 (may not sum to exactly 1.0 due to rounding, cash drag, etc.).

**Instrument type stays as a separate concept.** The `InstrumentType` enum +
single-label model is correct for type — a fund is always type `Etf` or
`MutualFund`, regardless of what it holds. Type classification describes what
the instrument *is*, not what it *contains*. The weighted model is for
dimensions where an instrument can span multiple categories.

### The calculation is generic

One function handles all weighted dimensions:

```rust
pub fn weighted_allocation(
    positions: &[ValuedPosition],
    total: Money,
    dimension: &str,
    get_weights: impl Fn(&InstrumentId) -> Vec<(String, f64)>,
) -> AllocationResult
```

For each position, it calls `get_weights` to get the `{key: weight}` map, then
distributes that position's `market_value_base` across the keys proportionally.
Results are grouped, summed, and sorted by descending portfolio weight.

Calling it for sector:
```rust
let sector_alloc = weighted_allocation(&positions, total, "sector", |id| {
    market_data.get_allocations(id, "sector")
});
```

### Result types

```rust
pub struct AllocationEntry {
    pub key: String,           // e.g. "Information Technology"
    pub market_value: Money,   // value attributed to this key
    pub weight: f64,           // fraction of total portfolio
}

pub struct AllocationResult {
    pub dimension: String,     // e.g. "sector"
    pub entries: Vec<AllocationEntry>,
    pub total: Money,
}
```

## Data storage: JSONB vs separate table

### Option A: JSONB column on instruments

```sql
ALTER TABLE instruments ADD COLUMN allocations JSONB DEFAULT '{}';

-- Example row:
-- allocations = {
--   "sector": {"Information Technology": 1.0},
--   "geography": {"United States": 1.0}
-- }
```

**Pros:**
- Simple schema — one column, one read per instrument
- Flexible — add new dimensions without migrations
- Natural fit for bulk-load pattern (read all at startup, serve from memory)
- Easy to serialize/deserialize in Rust via serde
- Atomic updates per instrument (no partial-write inconsistency)

**Cons:**
- Can't efficiently query "all instruments with >20% Technology" (needs GIN
  index + jsonpath)
- No DB-level constraint that weights sum to ~1.0
- No referential integrity on dimension/key names (typos go unnoticed)
- Harder to build reports purely in SQL

### Option B: Separate table

```sql
CREATE TABLE instrument_allocations (
    instrument_id BIGINT REFERENCES instruments(id),
    dimension     VARCHAR(30) NOT NULL,  -- 'sector', 'geography'
    key           VARCHAR(80) NOT NULL,  -- 'Information Technology', 'United States'
    weight        DOUBLE PRECISION NOT NULL,
    PRIMARY KEY (instrument_id, dimension, key)
);
```

**Pros:**
- Fully queryable ("all tech-heavy instruments", aggregation in SQL)
- DB-level constraints possible (CHECK weight BETWEEN 0 AND 1)
- Normalized — plays well with SQL tooling and reporting
- Easy to update individual entries without rewriting the whole blob

**Cons:**
- Many rows per instrument (11 GICS sectors × N instruments)
- Requires joins when loading instrument data
- Adding a new dimension means inserting more rows, not just updating a JSON key
- Slightly more complex bulk load

### Recommendation: JSONB

For our architecture (bulk-load at startup → serve from memory), the JSONB
column is the better fit:

1. We never query allocations in SQL during normal operation — everything is in
   the `InMemoryMarketDataService` after startup
2. The flexibility to add dimensions without migrations is valuable since we
   have 7+ dimensions ahead
3. Serde makes JSONB ↔ `HashMap<String, HashMap<String, f64>>` trivial
4. The old njorda system also used a JSON-style structure for this data

We can always add a GIN index later if SQL-side querying becomes important.

## Plan (per /new-calculation skill)

### Phase 0: Rename `#CALC_ALLOC` → `#CALC_ALLOC_INSTYPE`
- Update tag in `docs/calculations/methodology.md`
- Update tag in `calc/allocation.rs` doc comment
- Grep for any other references

### Phase 1: Methodology docs
- Write `#CALC_ALLOC_WEIGHTED` section in `docs/calculations/methodology.md`
  documenting the generic weighted allocation engine
- Write `#CALC_ALLOC_SECTOR` section as the first consumer
- Document the weighted model, the look-through semantics for funds, and edge
  cases (missing allocations, weights not summing to 1.0)

### Phase 2: Tests
- Unit tests for `weighted_allocation()` in `calc/allocation.rs`:
  - Single-category instrument (weight map with one entry at 1.0)
  - Multi-category fund (weights distributed proportionally)
  - Mixed portfolio: stock + fund overlapping in same category, values combine
  - Missing allocations → "Uncategorized" bucket
  - Weights that don't sum to 1.0 (partial allocation, cash drag)
  - Zero total → zero weights
- Integration test: portfolio with AAPL (stock, single sector) + SPY (ETF,
  multi-sector), verify sector allocation reflects look-through
- Python test for `sector_allocation` on `PortfolioReport`

### Phase 3: Core implementation

#### 3a. Allocation data on `MarketDataService`
- Add default method to trait:
  ```rust
  fn get_allocations(&self, instrument: &InstrumentId, dimension: &str) -> Vec<(String, f64)>
  ```
  Default returns empty vec (instrument has no allocation data for this
  dimension → will map to "Uncategorized").
- `TestMarketData`: add storage + `add_allocation(instrument, dimension, key,
  weight)` method, implement override
- `InMemoryMarketDataService`: same pattern, stored as
  `HashMap<InstrumentId, HashMap<String, Vec<(String, f64)>>>`, populated from
  JSONB at load time

#### 3b. Generic weighted allocation — `calc/allocation.rs`
- Add `weighted_allocation()` function
- Add `AllocationEntry` and `AllocationResult` types
- Existing `type_allocation()` stays as-is (just tag renamed)

#### 3c. `PortfolioReport`
- Add `pub sector_allocation: AllocationResult` field
- Compute by calling `weighted_allocation()` with dimension "sector"
- Future dimensions (geography, asset_class) added the same way

### Phase 4: Data layer

#### 4a. DB migration
- Add `allocations JSONB DEFAULT '{}'` to `instruments` table
- New Alembic migration in `services/calce-db/`

#### 4b. Queries
- `list_instruments()` → SELECT `allocations` too
- `insert_instrument()` → accept optional `allocations` JSONB param
- Parse JSONB to `HashMap<String, HashMap<String, f64>>` in Rust

#### 4c. Loader
- Parse allocations JSON per instrument
- Call `md.add_allocation(instrument, dimension, key, weight)` for each entry
- Done before `freeze()`

#### 4d. `InstrumentSummary`
- Add `pub allocations: HashMap<String, Vec<(String, f64)>>`

### Phase 5: Seed data & sanity check
- API seed: AAPL sector={"Information Technology": 1.0},
  VOW3 sector={"Consumer Discretionary": 1.0},
  SPY sector={"Information Technology": 0.30, "Health Care": 0.13,
  "Financials": 0.12, "Consumer Discretionary": 0.10, "Industrials": 0.09,
  "Health Care": 0.08, "Other": 0.18}
- DB seed tool: assign GICS sectors to stocks (single entry), generate
  plausible multi-sector breakdowns for instruments typed as ETF/MutualFund

### Phase 6: Python bindings & API
- `results.rs`: `AllocationEntry` and `AllocationResult` pyclass wrappers
- `PortfolioReport`: `sector_allocation` getter
- `services.rs`: `add_allocation(instrument_id, dimension, key, weight)` on
  `MarketData`
- Register new classes in `lib.rs`

### Phase 7: Final verification
- `invoke check` + `invoke test`

## What does NOT change
- `InstrumentType` enum and `type_allocation` — separate concept (what an
  instrument *is*), only the tag gets renamed
- `ValuedPosition`, `Position`, `Trade` — allocations are instrument metadata
- `CalculationContext` — no new parameters
- `value_change.rs`, `aggregation.rs`, `volatility.rs` — untouched

## Future dimensions (no code needed now, just same pattern)
Once sector works, adding more dimensions is just data:
- **Geography**: {"United States": 0.60, "Europe": 0.25, "Asia": 0.15}
- **Asset class**: {"Equity": 0.65, "Fixed Income": 0.35}
- **Currency allocation**: {"USD": 0.70, "EUR": 0.20, "GBP": 0.10}
- Each becomes one more `AllocationResult` field on `PortfolioReport` and one
  more call to `weighted_allocation()` with a different dimension string

## GICS top-level sectors (reference)
For seed data and testing. 11 sectors per the GICS standard (MSCI/S&P):
Communication Services, Consumer Discretionary, Consumer Staples, Energy,
Financials, Health Care, Industrials, Information Technology, Materials,
Real Estate, Utilities
