# PostgreSQL Row-Level Security (RLS) Plan

## Status: Proposal / Investigation

## Motivation

Calce enforces data access control at two application layers:

1. **Route handlers** — call `require_access()`, `require_admin()`, `require_org_admin()`
2. **In-memory store** — `UserDataStore::check_user_access()` before returning data

This works but has structural risks:

- **Every new query/route must remember to check access.** A single missed check = data leak.
- **Org-scoped isolation is not enforced at the DB level.** An API key for org A could theoretically read org B's data if a route handler forgets to verify org membership.
- **Gaps already exist** — e.g. `user_accounts` in `calc.rs` calls `require_admin()` but doesn't verify the target user belongs to the caller's org.

RLS pushes the security boundary down to PostgreSQL itself — the database refuses to return rows the caller shouldn't see, regardless of application bugs.

---

## Current Access Model

```
SecurityContext { user_id, role, org_id }

Role::User                    → can only access own data (user_id == target)
Role::Admin + org_id == None  → unrestricted, can access any user's data
Role::Admin + org_id == Some  → org-scoped, should only access users in their org
```

**Tables with user-scoped data:** `accounts`, `trades`
**Tables with org-scoped data:** `users` (via `organization_id`), `api_keys` (via `organization_id`)
**Shared/public tables:** `instruments`, `prices`, `fx_rates`

---

## Proposed RLS Design

### Session Variables

Each DB connection sets session variables before executing queries:

```sql
SET app.user_id = '<external_id>';       -- authenticated user
SET app.role = 'admin' | 'user';         -- role
SET app.org_id = '<org_external_id>';    -- NULL for human users, set for API keys
```

Helper functions:

```sql
CREATE FUNCTION app_user_id() RETURNS TEXT STABLE AS $$
  SELECT current_setting('app.user_id', true)
$$ LANGUAGE SQL;

CREATE FUNCTION app_role() RETURNS TEXT STABLE AS $$
  SELECT current_setting('app.role', true)
$$ LANGUAGE SQL;

CREATE FUNCTION app_org_id() RETURNS TEXT STABLE AS $$
  SELECT current_setting('app.org_id', true)
$$ LANGUAGE SQL;
```

### Policies

#### `users` table

```sql
ALTER TABLE users ENABLE ROW LEVEL SECURITY;

-- Unrestricted admins see all users
CREATE POLICY users_unrestricted_admin ON users
  USING (app_role() = 'admin' AND app_org_id() IS NULL);

-- Org-scoped admins see users in their org
CREATE POLICY users_org_admin ON users
  USING (app_role() = 'admin' AND organization_id = (
    SELECT id FROM organizations WHERE external_id = app_org_id()
  ));

-- Regular users see only themselves
CREATE POLICY users_self ON users
  USING (external_id = app_user_id());
```

#### `accounts` table

```sql
ALTER TABLE accounts ENABLE ROW LEVEL SECURITY;

CREATE POLICY accounts_unrestricted_admin ON accounts
  USING (app_role() = 'admin' AND app_org_id() IS NULL);

CREATE POLICY accounts_org_scoped ON accounts
  USING (app_role() = 'admin' AND user_id IN (
    SELECT u.id FROM users u
    JOIN organizations o ON u.organization_id = o.id
    WHERE o.external_id = app_org_id()
  ));

CREATE POLICY accounts_owner ON accounts
  USING (user_id = (SELECT id FROM users WHERE external_id = app_user_id()));
```

#### `trades` table

Same pattern as `accounts` — filter by `user_id` ownership / org membership.

#### Shared tables (`instruments`, `prices`, `fx_rates`)

No RLS — these are public/read-only reference data.

### Database Role Setup

```sql
-- Application role (used by connection pool)
CREATE ROLE calce_app LOGIN;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES TO calce_app;

-- RLS applies to calce_app but NOT to the migration/superuser role
-- This means migrations and admin scripts bypass RLS automatically
```

Key: RLS policies apply to `calce_app`. The migration user (superuser or table owner) bypasses RLS by default — no special handling needed for schema changes.

### sqlx Integration

The critical piece: setting session variables on each connection checkout from the pool.

**Option A: Per-query SET (simple, explicit)**
```rust
async fn with_context(pool: &PgPool, ctx: &SecurityContext) -> sqlx::Result<PgConnection> {
    let mut conn = pool.acquire().await?;
    sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(ctx.user_id.as_str())
        .execute(&mut *conn)
        .await?;
    sqlx::query("SELECT set_config('app.role', $1, true)")
        .bind(if ctx.is_admin() { "admin" } else { "user" })
        .execute(&mut *conn)
        .await?;
    sqlx::query("SELECT set_config('app.org_id', $1, true)")
        .bind(ctx.org_id.as_deref())
        .execute(&mut *conn)
        .await?;
    Ok(conn)
}
```

Using `set_config(..., true)` makes the setting transaction-local, so it auto-resets when the connection returns to the pool. This is safe for connection pooling.

**Option B: sqlx `after_connect` hook + Axum middleware**
Set variables once per request in middleware. More elegant but tighter coupling.

**Recommendation: Option A.** Explicit, auditable, no hidden state.

---

## Implications

### Benefits

- **Defense in depth** — DB-level guarantee that queries cannot return unauthorized rows, even if application code has bugs
- **Closes existing gaps** — org-scoped isolation enforced automatically, no per-route manual checks needed
- **Simplifies future development** — new queries/routes automatically inherit access control
- **Audit-friendly** — policies are declarative SQL, reviewable independently of application code

### Risks & Costs

| Risk | Mitigation |
|------|-----------|
| **RLS policy bugs are silent** — wrong policy = missing data, not errors | Comprehensive integration tests that verify both access and denial for each role |
| **Performance** — subqueries in policies add overhead | Use indexed lookups (user_id, organization_id are already FK-indexed). Benchmark before/after. |
| **Connection pool complexity** — must set session vars correctly | Use `set_config(..., true)` for transaction-local scope; test that pool reuse doesn't leak context |
| **`SecurityContext::system()` bypass** — internal operations need unrestricted access | System operations use a separate connection pool with a superuser/table-owner role that bypasses RLS |
| **In-memory store divergence** — `UserDataStore` caches data; RLS only guards DB reads | RLS protects the DB boundary; `UserDataStore` access checks remain as a fast-path guard. Both layers stay. |
| **Migration complexity** — enabling RLS on existing tables with data | Enable RLS per-table incrementally; each table is independently testable |

### What Changes in Application Code

1. **Query functions** (`UserDataRepo`) gain a `SecurityContext` parameter → used to set session vars before queries
2. **Route handlers** can drop explicit `require_access()` calls for data-fetching routes (RLS handles it) — but keep them as belt-and-suspenders for now
3. **`DataService`/`UserDataStore`** keep their access checks — RLS is defense-in-depth, not a replacement
4. **Connection pool** configuration adds a `calce_app` role with restricted privileges

### What Does NOT Change

- Auth middleware (JWT/API key validation)
- SecurityContext construction
- In-memory store access checks
- Rate limiting, lockout, input validation
- Shared data tables (instruments, prices, fx_rates)

---

## Implementation Plan

### Phase 1: Foundation (DB role + session variable helpers)

- [ ] Create `calce_app` database role in Alembic migration
- [ ] Grant appropriate permissions to `calce_app`
- [ ] Create `app_user_id()`, `app_role()`, `app_org_id()` SQL functions
- [ ] Add Rust helper `with_context(pool, ctx) -> PgConnection` in calce-data
- [ ] Integration test: verify `set_config` is transaction-local and doesn't leak across pool connections

### Phase 2: RLS on `accounts` (pilot table)

- [ ] Alembic migration: `ALTER TABLE accounts ENABLE ROW LEVEL SECURITY`
- [ ] Add policies: owner, org-scoped admin, unrestricted admin
- [ ] Integration tests:
  - Regular user can only see their own accounts
  - Org-scoped admin sees only accounts for users in their org
  - Unrestricted admin sees all
  - User A cannot see user B's accounts even with a direct query
- [ ] Benchmark: compare query latency with and without RLS
- [ ] Wire `UserDataRepo::get_user_accounts()` to use `with_context()`

### Phase 3: RLS on `trades`

- [ ] Same pattern as accounts
- [ ] Integration tests for trade-level access control
- [ ] Wire trade queries through `with_context()`

### Phase 4: RLS on `users` table

- [ ] Policies for self-access, org-scoped admin, unrestricted admin
- [ ] Handle edge case: user creation (INSERT policy needed, or use superuser for writes)
- [ ] Integration tests

### Phase 5: System operations & admin bypass

- [ ] Configure separate connection pool (or use superuser) for:
  - CDC sync operations
  - Data seeding / import
  - Background jobs
- [ ] Verify these bypass RLS correctly
- [ ] Document which operations use which pool

### Phase 6: Cleanup & hardening

- [ ] Audit all remaining query paths — ensure all user-data queries go through `with_context()`
- [ ] Consider whether to remove application-level `require_access()` calls (recommendation: keep them)
- [ ] Add CI check: any new table with `user_id` column must have RLS enabled
- [ ] Update `docs/auth.md` with RLS documentation

---

## Open Questions

1. **INSERT policies** — Should RLS also control who can insert rows? Currently writes go through controlled code paths, but RLS INSERT policies would prevent a bug from creating trades under the wrong user.

2. **`UserDataStore` cache invalidation** — If RLS restricts what a query returns, but the in-memory store was populated by a system context, could a downgraded context still see cached data? (Answer: yes, because the in-memory store has its own access checks. But worth verifying.)

3. **Performance budget** — What's the acceptable latency overhead? The subquery in RLS policies (`SELECT id FROM users WHERE external_id = ...`) adds a lookup per row-check, though PostgreSQL optimizes this well with indexes.

4. **Test database** — Integration tests need a `calce_app` role and RLS-enabled schema. How does this interact with the existing test setup?
