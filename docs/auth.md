# Authentication & Authorization

## Principle

Every route requires authentication. The only exceptions are health checks
and similar infrastructure endpoints. Even instrument-scoped calculations
(e.g. volatility) that aren't tied to a specific user still require the
caller to be an authenticated user of the system.

## Two Levels of Authorization

1. **Authenticated** — the caller is a valid user. Required for all
   calculation and data endpoints, including instrument-scoped ones.

2. **User-scoped** — the caller is accessing a specific user's data
   (portfolios, trades, market value). Requires authentication *plus*
   an access check: can this user see that user's data?

Instrument-scoped endpoints only need level 1. User-scoped endpoints
need both.

## User Model

Every authenticated request produces a `SecurityContext` containing:

- **user_id** — the authenticated user
- **role** — currently `User` or `Admin`

The `SecurityContext` is passed through `CalcEngine` and into service calls
so that data access checks happen close to the data, not just at the HTTP edge.

### Access Rules (User-Scoped)

A user can access their own data. An advisor or admin can access other users'
data. The check is `SecurityContext::can_access(target_user_id)`:

- `Role::User` — can only access data where `target == self`
- `Role::Admin` — can access any user's data

This will evolve to support an advisor model where a user may be granted
access to specific clients (similar to the old njorda system), but the
core pattern stays the same: authenticate → build `SecurityContext` →
pass it through to data layer → data layer enforces access.

## Current Implementation

Header-based (placeholder for real auth):

- `X-User-Id` — required on user-scoped routes, maps to `UserId`
- `X-Role` — optional, defaults to `User`

Extracted in `calce-api/src/auth.rs` as an Axum `FromRequestParts` extractor.
Missing `X-User-Id` on a user-scoped route returns 401.

## What's Coming

- Real authentication (tokens, OAuth, or similar) replacing header stubs
- Advisor role: a user with access to a set of client user IDs
- Per-client access grants rather than blanket admin access
- The `SecurityContext` will grow to carry these grants, but the flow
  (authenticate → context → pass to data layer) stays the same
