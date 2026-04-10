# Permissions

Given an authenticated user, what are they allowed to do and what data can
they access? [Authentication](auth.md) produces a `SecurityContext` — this
document covers what happens after that.

## SecurityContext

Every authenticated request produces a `SecurityContext` (defined in
`calce-data::auth`) containing:

- **user_id** — the authenticated user
- **role** — the user's role
- **org_id** — present for all user roles except admin

## Permission layers

### API access

API routes check that the caller has the required permission based on their
role (see RBAC below).

NOTE: API key authentication should also map to a role.

### Multi-tenancy (organization isolation)

The security context is passed through to any part of the system that reads
data.

**SQL queries**
Every table has an `org_id` column. Row-level security (RLS) policies validate
it against a session variable. All queries set `org_id` before executing.

Exception: Njorda admin operations (e.g. managing organizations) use a
superuser that bypasses RLS.

**Cache reads**
Any org-specific cached data must also store the `org_id` and filter on it,
requiring a `SecurityContext` to read from the cache.

### Data access within an organization

#### Roles

Each user has one role (we may expand later, but starting simple). Roles are
hardcoded to support strict compile-time checking.

    admin   — Njorda admin, global rights
    CIO
    Advisor
    Audit

#### Permissions

Permissions govern data access, calculations, and operations. They are also
hardcoded since they map directly to code features.

    read:<table>    e.g. read:trades
    write:<table>   e.g. write:accounts
    app:<appname>   e.g. app:portfolio_builder
    op:<opname>     e.g. op:send_client_proposal

#### Role-based access control (RBAC)

Each user is assigned a role; each role is assigned a set of permissions.

The RBAC module exposes:

    has_permission(org, role, permission)

In practice, using the security context:

    has_permission(security_ctx, permission)

RBAC is checked frequently so it must be fast and never hit the database —
it should be fully cached. Init sequence:

1. Establish CDC connection.
2. Bulk-load all role/permission mappings. Queue any CDC updates that arrive
   during the load and apply them afterward (safer than skipping duplicates).
3. After init, CDC updates are applied continuously so the cache stays fresh.

#### Owned data

Some data within an org is accessible to anyone with the right permission —
e.g. if you can edit CMAs, you can edit any CMA in the org.

Other data is tied to a specific user. For example, client accounts are
"owned" by that client. An advisor has `read:client_data`, but only for
*their* clients. The check becomes:

    has_permission(org, role, permission, target_obj)

**Permissions vs settings for scope.** This could be modelled as an org-level
setting (the old B2B app treated "access all clients vs only your clients" as
a setting). However, encoding scope directly into permissions is probably
cleaner:

    read:clients:all
    read:clients:owned

This ownership logic likely lives outside the core RBAC module to keep RBAC
as a simple role-to-permission lookup.

**Ownership model.** We need strict definitions:

- Data with no `owner_id` is org-global — you can either access it or not.
- Data with an `owner_id` is owned (e.g. an account):
  - `data:account:own` — access your own data
  - `data:account:all` — access all accounts in the org
  - `data:account:indirect` — access via indirect ownership (e.g. advisor
    accessing their client's data)

We need a function to determine if a user can access another user's data:

    can_access_data(user, target_user)

For the initial advisor case this is a simple check: each user has an
`advisor_id` attribute linking them to their advisor.

More complex cases are foreseeable (e.g. a hierarchy where you implicitly
have access to data at lower levels), but we should avoid building that
until it's needed.

**Query-time filtering.** Checking permission for a single object is easy.
The harder case is querying — e.g. "list all portfolios I have access to."
Two approaches:

1. **Complex query** — join through the ownership chain (e.g. find all users
   where I am the advisor, then their portfolios). Risk: query errors can
   leak data, and this logic spreads outside the permission model.
2. **Bulk retrieve + filter** — fetch a broader set, then filter via RBAC
   object-by-object. Safer but potentially slower.

These can be combined: query to narrow the set, then apply an access filter.
Pushing this into RLS would be ideal.

**Cache access.** The same watertight access checking must apply when reading
from a cache — not just from SQL.

## Roadmap

### Access groups for owned data

Instead of stamping an `owner_id` / `advisor_id` directly on every data row,
introduce an indirection: each client gets an **access group**, and all their
data references that group. Users (client, advisor, CIO) are added as members
of the group with a specific relation.

Benefits: reassigning an advisor becomes a single-row update in the membership
table instead of touching every trade/account/position. RLS policies stay
uniform across tables (`access_group_id` checked against a session variable of
the user's accessible groups). The model extends naturally to teams or shared
access without schema changes.
