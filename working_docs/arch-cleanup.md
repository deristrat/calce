# Architecture Cleanup

Tracked items from the architecture review (2026-03-06).

## Completed

### 1. Update docs: InMemoryMarketDataService is not test-only
- [x] Updated rust-guidelines.md to reflect that InMemory* types serve as the sync bridge in production

### 2. Remove CalcEngine
- [x] Removed `engine.rs` from calce-core
- [x] Rewrote integration tests to call calc functions directly
- [x] Updated calce-python bindings to call calc functions directly
- [x] Updated CLAUDE.md, architecture.md, rust-guidelines.md references

### 3. Auth: SecurityContext into DataService + permissions module
- [x] Created `calce-core/src/permissions.rs` with `can_access_user_data` function + tests
- [x] `SecurityContext::can_access` now delegates to permissions module
- [x] `DataService.load_trades` and `load_user_portfolio` require `SecurityContext` and enforce access checks
- [x] Removed duplicate `check_access` from API routes — DataService handles it
- [x] Updated architecture doc

### 4. Implement Outcome<T> partial results
- [x] Added `Outcome<T>` type in `calce-core/src/outcome.rs` with `Warning` and `WarningCode`
- [x] Converted `value_positions` to return `Outcome<MarketValueResult>` — skips positions with missing data, emits warnings
- [x] `value_change_summary` and `portfolio_report` collect and merge warnings from sub-computations
- [x] Added tests: `missing_price_produces_warning_not_error`, `partial_success_with_mixed_availability`
- [x] API and Python layers unwrap `.value` (TODO: surface warnings in response)

### 5. Add Money::checked_sub
- [x] Added `checked_sub` to Money with tests
- [x] `value_change` now uses `checked_sub` instead of negation + `checked_add`

### 6. Fix prev_year / ytd_start fallbacks
- [x] `prev_year` returns `CalceResult` instead of falling back to `date`
- [x] `ytd_start` returns error instead of falling back to `as_of_date`

### 7. TODO: seed data for non-test use
- [x] Added TODO comment in `main.rs` about making seed data available for local dev

### 8. Include underlying error type in DataError
- [x] Changed `CalceError::DataError(String)` to `DataError { message, source }` preserving the original error

### 9. Aggregation: reject mixed currencies for same instrument
- [x] Added `CurrencyConflict` error variant
- [x] `aggregate_positions` now returns `CalceResult` and errors on mixed currencies
- [x] Added test `mixed_currencies_for_same_instrument_rejected`
- [x] Added API error mapping for `CurrencyConflict` → 422

## Remaining TODOs

- Surface `Outcome.warnings` in API responses (response wrapper or header)
- Surface warnings in Python bindings
- DataService backend enum: consider a trait-based approach when adding njorda backend
