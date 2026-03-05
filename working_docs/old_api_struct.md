# Old Njorda API Structure

Reference for porting from the Flask-based njorda API to the Rust calce-api.

## Overview

Flask app using `flask-api-framework`, Marshmallow schemas, SQLAlchemy ORM.
~280 routes across 8 blueprints serving different audiences.

## URL Structure

```
/meta                    — Health checks, version info (2 routes)
/v2                      — Core API: portfolios, calculations, market data (54 routes)
/b2b/v1                  — Advisor platform: org/client management (105 routes)
/app/v1                  — Mobile/web app: auth, holdings, settlements (68 routes)
/admin/v1                — Admin portal: org settings, audit logs (26 routes)
/connect/v1              — Bank connections, OAuth callbacks (18 routes)
/events/v1               — WebSocket/SSE event streaming (1 route)
/mih/v1                  — MIH integration (7 routes)
```

Endpoints are RESTful with hierarchical resource paths:
```
/data/portfolios
/data/portfolios/<id>
/data/accounts/<account_id>/transactions
/calculate/market-value/user
/market-data/instruments/<ticker>/info
```

## Authentication

Three methods:

1. **Bearer token** — `Authorization: Bearer <token>`, stored in DB with optional expiry.
   Organization tokens never expire. User tokens auto-extend when < 10 min remaining.

2. **Session cookies** — Flask-Login sessions. Multiple cookie names per blueprint
   (`njorda_b2b_session`, `njorda_admin_session`, etc.).

3. **OAuth** — Google, Apple, Bank ID for mobile/web app users.

Auth enforced via decorators on view classes:
```python
@auth.login_required_organization      # Bearer token + org account
@auth.login_required_app_user          # Session user + terms accepted
@auth.login_required_admin             # Admin role
@auth.login_required_org_advisor       # Advisor role
```

Role-based access: `has_access_to_user()`, `has_access_to_organization()`.
Advisors see specific clients or all clients (if `can_access_all_clients`).

## Error Handling

Unified format via `ApiError`:
```json
{
  "status_code": 400,
  "message": "Error description",
  "errors": { "field": ["validation detail"] }
}
```

Status codes: 400 (validation), 401 (missing auth), 403 (forbidden), 409 (conflict).
Global handler catches all `ApiError` and formats as JSON. Unhandled exceptions logged via Sentry.

## Endpoint Categories

### Data (`/data/*`)
CRUD for all domain objects: portfolios, accounts, holdings, transactions, trades, connections, data providers.

```
GET    /v2/data/accounts?user_id=uuid&account_type=brokerage
POST   /v2/data/accounts
GET    /v2/data/accounts/{id}
PUT    /v2/data/accounts/{id}
DELETE /v2/data/accounts/{id}
```

### Calculations (`/calculate/*`)
Three aggregation levels per calculation: user, accounts, positions.

```
GET  /v2/calculate/market-value/user?user_id=uuid&currency=SEK
GET  /v2/calculate/market-value/accounts?account_ids=uuid1,uuid2
POST /v2/calculate/market-value/positions  { "positions": [...] }
```

Calculations available: market-value, profit-and-loss, portfolio-scenario,
max-sharpe, efficient-frontier, portfolio-analytics.

### Market Data (`/market-data/*`)
```
GET /v2/market-data/instruments/search?query=apple
GET /v2/market-data/instruments/{ticker}/info
GET /v2/market-data/instruments/{ticker}/latest-price
```

### Analytics (`/analytics/*`)
Custom report builder with grouping, columns, export.

### Auth (varies by blueprint)
Login, register, OAuth flows, token management, password recovery.

## Request/Response Patterns

Three input sources, all validated by Marshmallow schemas:

1. **Path params** — Custom Marshmallow fields that load + validate the resource from DB
2. **Query params** — `ArgsSchema` with validators (`OneOf`, etc.)
3. **Request body** — JSON deserialized into schema, accessed via `self.loaded_body`

Responses are always JSON. Marshmallow handles serialization with field renaming
(`id = ma.String(attribute="uuid")`), computed fields (`ma.Method`), and nesting.

## View System

Base classes from flask-api-framework:
- `af.Read` — GET single resource
- `af.List` — GET collection with filtering
- `af.Create` — POST
- `af.Update` — PATCH/PUT
- `af.Delete` — DELETE

Combined via mixins: `class AccountEndpoint(af.List, af.Create)`.

`CalcViewMixin` provides shared calculation context building.

## What to Port vs What to Simplify

**Keep:**
- RESTful URL structure with clear resource hierarchy
- Unified error format with meaningful status codes
- Separate aggregation levels for calculations (user/accounts/positions)
- Auth decorators / middleware pattern

**Simplify:**
- No need for multiple session types (header-based auth is sufficient for now)
- No Marshmallow — Axum extractors + serde handle validation
- No ORM — calce-core service traits replace SQLAlchemy
- Fewer blueprints — start with one router, split later if needed
- No Celery — calculations are fast in Rust, no async task queue needed yet

**Watch out for:**
- The old API has ~280 routes. Port incrementally, starting with calculations.
- Custom Marshmallow fields that load resources from DB during deserialization — in Rust,
  do this explicitly in the handler instead.
- Multiple API versions serving different audiences — defer multi-audience support until needed.
