# Backlog

Deferred improvements and future work, roughly prioritized.

## High Priority

## Medium Priority

### Trade ID for audit trails

`Trade` currently has no unique identifier. Needed for:
- Deduplication (same trade arriving twice from different sources)
- Reconciliation (matching trades to broker confirmations)
- Audit logs (which trades contributed to a position)

## Low Priority

### InstrumentId with cheap cloning

`InstrumentId(String)` allocates on clone. For hot paths (HashMap lookups), consider `InstrumentId(Arc<str>)` for refcount-based cloning. Profile before optimizing.

### Price currency

`Price` is just a `Decimal` with no currency. In reality, a price has a currency (the instrument's listing currency). Currently the position's currency serves this role. Consider `Price { amount: Decimal, currency: Currency }` if cross-listed instruments become relevant.
