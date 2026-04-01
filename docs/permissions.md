# Permissions

Given an authenticated user, what are they allowed to do and what data can
they access? [Authentication](auth.md) produces a `SecurityContext` — this
document covers what happens after that.

## SecurityContext

Every authenticated request produces a `SecurityContext` (defined in
`calce-data::auth`) containing:

- **user_id** — the authenticated user
- **role** — currently `User` or `Admin`
- **org_id** — present when authenticated via API key (org-scoped)

## Three Levels of Authorization

1. **Authenticated** — the caller is a valid user. Required for all
   calculation and data endpoints, including instrument-scoped ones
   (e.g. volatility).

2. **Admin-only** — the caller must be an admin. Used for user
   management (create, list, delete), organization endpoints, and API key
   management.

3. **User-scoped** — the caller is accessing a specific user's data
   (portfolios, trades, market value). Requires authentication *plus*
   an access check: can this user see that user's data?

## Access Rules

The check is `SecurityContext::can_access(target_user_id)`:

- `Role::User` — can only access data where `target == self`
- `Role::Admin` — can access any user's data

### Org-Scoped API Keys

API keys carry an `org_id` on their `SecurityContext`. The permissions layer
denies cross-org user-data access by default; route handlers must explicitly
verify org membership for user-scoped routes.

## Enforcement

Route handlers enforce access checks via helper functions in
`calce-api/src/auth.rs`:

- `require_admin(ctx)` — returns 403 unless `ctx.role == Admin`
- `require_access(ctx, target_user_id)` — returns 403 unless
  `ctx.can_access(target)` (delegates to `calce-data::permissions`)

calce-core has no auth or permissions types — it is a pure calculation engine.

## Module Layout

```
calce-data/src/auth/mod.rs    — SecurityContext, Role
calce-data/src/permissions.rs  — access-control rules
calce-api/src/auth.rs          — require_admin, require_access helpers
```

## Database Tables

- `users.role` — `"user"` or `"admin"` (default: `"user"`)

## What's Coming

- Advisor role: per-client access grants (a user may be granted access to
  specific clients, but the core pattern stays the same: authenticate →
  build SecurityContext → pass to data layer → data layer enforces access)
