# Calce Architectural Review

Updated: 2026-04-10

This review consolidates the earlier architecture review in [working_docs/architecture_review_2026_04_09.md](/Users/daniel/repos/calce/working_docs/architecture_review_2026_04_09.md) with a fresh pass focused on making the codebase easier to evolve with LLM agents.

## Executive Summary

Calce has a strong top-level shape:

- `calce-core` is a genuinely good foundation. It has small, readable APIs and deep pure logic behind them.
- The crate graph is still mostly sensible: pure core, async data layer, HTTP API, Python bindings, frontend.
- The main problems are not at the workspace boundary. They are inside the orchestration layers.

The codebase is therefore in a good but unstable middle state:

- The architectural intent is strong.
- Several modules already embody the intended style.
- The implementation has drifted enough that the intended architecture is no longer the easiest mental model to use.

For human contributors this creates friction. For LLM agents it is worse: stale docs, broad public APIs, mixed-responsibility modules, and duplicated logic make it harder to localize changes safely.

## What Is Working Well

### 1. `calce-core` is a strong foundation

This is the healthiest part of the repo.

- The `MarketDataService` interface is thin and understandable in [crates/calce-core/src/services/market_data.rs](/Users/daniel/repos/calce/crates/calce-core/src/services/market_data.rs).
- Pure calculation functions are layered well.
- Composite logic like `portfolio_report` in [crates/calce-core/src/reports/portfolio.rs](/Users/daniel/repos/calce/crates/calce-core/src/reports/portfolio.rs) is easy to follow.
- The crate has good boundaries: no DB, no async runtime, minimal external concerns.

This is close to the target shape for LLM-friendly modules: thin API, deep internal logic, low ambient complexity.

### 2. The overall workspace decomposition is directionally correct

The intended layered architecture in [docs/architecture.md](/Users/daniel/repos/calce/docs/architecture.md) is still the right idea:

- pure core
- data loading and authorization at the edges
- API layer as composition and transport
- Python and AI services as consumers

The problem is not that the architecture is wrong. The problem is that parts of the implementation no longer honor it consistently.

### 3. There are already some good local conventions

- `CLAUDE.md` files explain local expectations.
- Domain logic in `calce-core` is reasonably discoverable.
- The console has some reusable hooks and UI primitives.
- The codebase clearly values testability and explicitness, even where the structure has drifted.

## Current Architectural Friction

### 1. The architecture docs are no longer fully trustworthy

This is the highest-leverage problem for agent development.

Examples:

- [docs/architecture.md:34](/Users/daniel/repos/calce/docs/architecture.md#L34) describes `DataService.load_calc_inputs(security_ctx, spec)`.
- That façade does not exist in `calce-data`; the public surface in [crates/calce-data/src/lib.rs](/Users/daniel/repos/calce/crates/calce-data/src/lib.rs) exposes raw modules instead.
- [docs/rust-guidelines.md:11](/Users/daniel/repos/calce/docs/rust-guidelines.md#L11) says `calce-python` depends only on `calce-core`.
- In reality, [crates/calce-python/Cargo.toml](/Users/daniel/repos/calce/crates/calce-python/Cargo.toml) depends on `calce-data` and `sqlx`.

When the docs describe a cleaner architecture than the code implements, an LLM will form the wrong mental model and make bad local decisions.

### 2. There is no real application-service layer

A lot of workflow logic lives directly in route handlers, Python bindings, or service startup code.

Examples:

- [crates/calce-api/src/routes/calc.rs](/Users/daniel/repos/calce/crates/calce-api/src/routes/calc.rs) parses inputs, loads trades, fetches market data, aggregates, runs calculations, maps warnings, and shapes responses.
- [crates/calce-python/src/data_service.rs](/Users/daniel/repos/calce/crates/calce-python/src/data_service.rs) contains loader orchestration, caching, summary gathering, and Python-facing object construction.
- [services/calce-ai/calce_ai/app.py](/Users/daniel/repos/calce/services/calce-ai/calce_ai/app.py) mixes startup, auth endpoints, chat orchestration, tool execution loop, and static file serving.

This means feature work often cuts across several modules instead of staying local to one use-case boundary.

### 3. `calce-data` exposes internals instead of a thin stable API

The public surface of `calce-data` is broad:

- [crates/calce-data/src/lib.rs](/Users/daniel/repos/calce/crates/calce-data/src/lib.rs) publishes almost every submodule.
- Downstream code reaches into `queries`, `auth`, `concurrent_market_data`, and store internals directly.

That makes the crate powerful but not legible. A thin external API is more useful for LLM agents than a wide menu of implementation pieces.

### 4. Some modules have become “god modules”

The clearest example is [crates/calce-data/src/concurrent_market_data.rs](/Users/daniel/repos/calce/crates/calce-data/src/concurrent_market_data.rs):

- builder materialization
- dense history layout
- stats
- query helpers
- history APIs
- mutation APIs
- PubSub notification wiring
- simulator helpers
- `MarketDataService` implementation

Each part is defensible. All of them in one module make the boundary harder to understand.

The same pattern exists elsewhere:

- [crates/calce-api/src/main.rs](/Users/daniel/repos/calce/crates/calce-api/src/main.rs) acts as startup, composition root, wiring module, and integration test host.
- [services/calce-console/src/api/client.ts](/Users/daniel/repos/calce/services/calce-console/src/api/client.ts) is transport, auth refresh, error handling, and all feature endpoints in one file.

### 5. Business logic is duplicated across layers

There are at least two different notions of position aggregation:

- canonical calculation logic in [crates/calce-core/src/calc/aggregation.rs](/Users/daniel/repos/calce/crates/calce-core/src/calc/aggregation.rs)
- data-layer summary aggregation in [crates/calce-data/src/user_data_store.rs](/Users/daniel/repos/calce/crates/calce-data/src/user_data_store.rs)

These are not equivalent:

- the core version respects `as_of_date`
- the core version validates currency conflicts
- the data-layer version is a simpler summary helper

That is exactly the kind of ambiguity that makes automated changes risky. There should be one canonical place for domain rules.

### 6. Permissions and auth remain cross-cutting instead of deep

This was already called out in the previous review and still holds.

- Auth logic is split between `calce-data/src/auth/*` and [crates/calce-api/src/auth.rs](/Users/daniel/repos/calce/crates/calce-api/src/auth.rs).
- Permission checks live partly in [crates/calce-data/src/permissions.rs](/Users/daniel/repos/calce/crates/calce-data/src/permissions.rs), partly in stores, and partly in route-level checks.

This is hard to extend and hard to audit.

### 7. `AppState` is still a broad service bag

[crates/calce-api/src/state.rs](/Users/daniel/repos/calce/crates/calce-api/src/state.rs) exposes many services to every route:

- market data
- user data
- DB pool
- auth config
- rate limiter
- simulators
- pubsub channels

This encourages handlers to reach through the whole system instead of depending on narrow capabilities.

### 8. API route organization is file-convenient, not concept-oriented

[crates/calce-api/src/routes/calc.rs](/Users/daniel/repos/calce/crates/calce-api/src/routes/calc.rs) is not just “calc routes”. It contains:

- calculation endpoints
- data exploration endpoints
- pagination DTOs
- warning response shaping
- several distinct resource families

This makes the file easy to append to and hard to understand as a bounded module.

### 9. The frontend has the same structural drift

The console is usable, but not yet organized for high-velocity feature work.

Examples:

- [services/calce-console/src/App.tsx](/Users/daniel/repos/calce/services/calce-console/src/App.tsx) is a flat route registry over page files.
- Page files like [services/calce-console/src/pages/UserDetailPage.tsx](/Users/daniel/repos/calce/services/calce-console/src/pages/UserDetailPage.tsx) mix fetching, mutations, cache invalidation, table definitions, formatting, and layout.
- Similar paging/filter logic exists in both [services/calce-console/src/hooks/usePaginatedSearch.ts](/Users/daniel/repos/calce/services/calce-console/src/hooks/usePaginatedSearch.ts) and [services/calce-console/src/pages/FxRatesPage.tsx](/Users/daniel/repos/calce/services/calce-console/src/pages/FxRatesPage.tsx).

Feature work is therefore page-centric rather than module-centric.

## Why This Matters For LLM Development

The target style described in the prompt is:

- thin, well-defined APIs
- deep internal functionality
- modules understandable from the boundary
- many tasks localizable to one module
- clear building blocks with easy-to-understand communication paths

Today the repo supports that well in `calce-core`, but less well in orchestration-heavy code.

The main blockers for LLM agent effectiveness are:

- stale architectural descriptions
- modules that expose internals instead of capabilities
- duplicated logic
- broad dependency bags
- feature code spread across transport, store, and calculation layers

An LLM can work well in a large codebase if the boundaries are trustworthy. Right now some of the most important boundaries are conceptual rather than enforced.

## Consolidated Top Improvements

### 1. Add a real application layer

This is the highest-value structural change.

Introduce narrow use-case services such as:

- `PortfolioService`
- `InstrumentAnalyticsService`
- `UserAdminService`
- `MarketDataExplorerService`
- `AuthService` as a single cross-boundary module

These should own orchestration such as:

- authorization checks
- loading trades or market data
- choosing the canonical calc flow
- shaping domain results for transport consumers

Routes, Python bindings, and AI tools should depend on these services rather than reassembling workflows themselves.

### 2. Make the docs match reality again

Pick one:

- implement the `DataService` / `load_calc_inputs` style abstraction the docs describe
- or rewrite the docs to describe the code as it is today

Do not leave the repo in the current split state where the architecture docs describe a cleaner system than the code actually exposes.

For LLM development this is not cosmetic. It changes whether the model starts from a correct mental map.

### 3. Shrink public APIs and hide internals

Especially in `calce-data`.

Prefer:

- a few stable exported service types and trait-facing adapters
- private `queries`, cache internals, and wiring helpers

Avoid:

- exposing almost every module as a public entry point

The question for each crate should be: what are the 3-6 things a caller is supposed to know?

### 4. Split mixed-responsibility modules by bounded context

Concrete candidates:

- split `routes/calc.rs` into `portfolio.rs`, `market_data.rs`, `instruments.rs`, `fx_rates.rs`, or similar
- move API warning/response shaping into shared transport helpers
- split `concurrent_market_data.rs` into storage, mutation, stats, and notification concerns behind a narrower outward type
- split frontend pages into feature folders with local API/query/column/view modules

The goal is not more files for their own sake. The goal is that a contributor can make a change inside one feature module without understanding unrelated concerns.

### 5. Eliminate duplicated domain logic

There should be one canonical implementation for core rules such as aggregation.

If a higher layer needs a specialized summary form, wrap the canonical logic or explicitly name the alternative as a projection. Do not keep parallel copies that look similar but behave differently.

### 6. Replace broad service bags with narrower dependencies

`AppState` should evolve away from being a universal bag.

Better options:

- feature-specific service structs injected into route modules
- capability traits
- smaller composition units with explicit dependency ownership

This reduces accidental coupling and makes modules safer to edit in isolation.

### 7. Deepen auth and permissions into one coherent module

The previous review identified this correctly. It remains a top priority.

The target should be:

- one place to read the rules
- one place to read credential/token flow
- one place to extend auth models

That module will still have HTTP and DB adapters, but the policy and flow should not be scattered.

### 8. Reorganize the frontend by feature, not just by page

A likely target shape:

- `src/features/users/...`
- `src/features/fx-rates/...`
- `src/features/instruments/...`
- `src/features/organizations/...`

Each feature should own:

- local API calls
- query key conventions
- columns/view models
- page components
- local hooks

The shared `api/client.ts` should become transport-only plus perhaps a few small domain clients, not the single home for every endpoint.

## Recommended Priority

By leverage:

1. Align docs and code.
2. Introduce an application-service layer.
3. Consolidate auth and permissions.
4. Remove duplicated domain logic.
5. Shrink public APIs and split god modules in `calce-data` and `calce-api`.
6. Reorganize the frontend by feature.
7. Improve typed seams around composition root and CDC.

## Suggested Near-Term Refactor Sequence

### Phase 1: Make the architecture explicit

- Update docs to match reality.
- Decide the intended public API for `calce-data`.
- Name the application services that should exist.

### Phase 2: Create the first narrow facades

- Extract `PortfolioService` from current API route orchestration.
- Extract `InstrumentAnalyticsService` for volatility and price-history style endpoints.
- Make Python and AI code consume those same use-case services where possible.

### Phase 3: Reduce cross-layer duplication

- Remove duplicate position aggregation logic or clearly separate it as projection-only code.
- Centralize warning mapping and response shaping.
- Narrow `AppState`.

### Phase 4: Featureize the frontend

- Start with one feature, likely `users`, and move it into a local module structure.
- Use that as the template for the rest of the console.

## Bottom Line

Calce does not need a wholesale rewrite. The architecture is already directionally good.

What it needs is enforcement of the intended style:

- fewer public internals
- more explicit use-case boundaries
- fewer mixed-responsibility modules
- one canonical home for domain rules
- documentation that reflects the actual system

If those changes are made, the repo will become substantially better for both human contributors and LLM agents: easier to navigate, easier to reason about, and much easier to change safely inside one module at a time.
