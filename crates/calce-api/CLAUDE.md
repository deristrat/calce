# calce-api

Axum HTTP server that wires `calce-core` calculations to REST endpoints.

## Module Structure

```
src/
├── main.rs       — router setup, server startup, integration tests
├── routes.rs     — handler functions (one per endpoint)
├── auth.rs       — X-User-Id / X-Role header extraction → SecurityContext
├── error.rs      — CalceError → HTTP status code mapping
├── state.rs      — AppState (shared service references)
└── seed.rs       — in-memory test data + seed sanity tests
```

## URL Convention

Two resource scopes:

All calculation and data endpoints require authentication (see `docs/auth.md`).

**User-scoped** — auth + user access check, operates on a user's portfolio:
```
GET /v1/users/{user_id}/market-value?as_of_date=...&base_currency=...
GET /v1/users/{user_id}/portfolio?as_of_date=...&base_currency=...
```

**Instrument-scoped** — auth only (no user access check), operates on market data:
```
GET /v1/instruments/{instrument_id}/volatility?as_of_date=...&lookback_days=...
```

When adding a new endpoint, decide which scope it belongs to:
- If it needs user trades/positions → user-scoped, auth + access check, use CalcEngine
- If it only needs market data → instrument-scoped, auth only, call calc function directly

## Authentication

Header-based, extracted in `auth.rs`:
- `X-User-Id` (required for user-scoped routes) — maps to `UserId`
- `X-Role` (optional, defaults to `"user"`) — `"admin"` grants cross-user access

Missing `X-User-Id` on any authenticated route → 401 Unauthorized.

## Error Handling

All `CalceError` variants must be mapped in `error.rs`. Current mapping:

| CalceError variant  | HTTP status | Notes |
|---------------------|-------------|-------|
| Unauthorized        | 403         | User lacks access to target user's data |
| NoTradesFound       | 404         | |
| CurrencyMismatch    | 400         | Client sent conflicting currencies |
| InsufficientData    | 422         | Not enough price history for calculation |
| PriceNotFound       | 422         | Missing market data — not a server bug |
| FxRateNotFound      | 422         | Missing market data — not a server bug |

**Important:** 500 should only occur for genuine server bugs (panics, DB connection
failures), never for missing data or bad input. If a `CalceError` maps to 500,
that's a sign the mapping needs fixing. PriceNotFound and FxRateNotFound are
currently 500 but should be 422 — this is a known issue.

Response format:
```json
{"error": "ERROR_CODE", "message": "Human-readable description"}
```

## Testing

Integration tests live in `main.rs` `#[cfg(test)]` and use `tower::ServiceExt::oneshot`
to send requests directly to the router (no real server needed).

Pattern:
```rust
let app = build_router(test_state());
let response = app.oneshot(request).await.unwrap();
```

`test_state()` uses `seed::seed_market_data()` / `seed::seed_user_data()`.
Seed data uses weekday dates only (prices and FX rates skip weekends).
The canonical test date is `2025-03-14` (Friday).
