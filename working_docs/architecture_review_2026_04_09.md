# Calce Architecture Review — 2026-04-09

## Overview

Calce is a Rust workspace for a portfolio/financial calculation engine. Six core crates with a clean dependency graph:

- **calce-core** — pure, sync calculation logic (money, FX, market value, aggregation, volatility). No async, no DB. Solid unit tests.
- **calce-datastructs** — concurrent caches and pub/sub primitives.
- **calce-data** — Postgres storage, auth, loaders. Async-heavy.
- **calce-cdc** — Postgres logical-replication CDC.
- **calce-api** — Axum HTTP server.
- **calce-python** — PyO3 bindings.

The crate-level boundaries are intentional and good. The friction is **inside** the crates, not between them.

## Top Architectural Friction Points

### 1. Auth fragmented across two crates (shallow + coupled)

Validation lives in `calce-data/src/auth/` (6 files, ~658 lines) but extraction/error handling is in `calce-api/src/auth.rs` (268 lines). Adding OAuth would touch both crates; testing auth requires setting up data-crate types from the api crate.

**Type:** shallow module + cross-crate coupling.

### 2. Market-data loading pipeline is bouncy

To follow data from DB to calculation you read 5 files in sequence: `loader.rs` → `market_data_builder.rs` → `concurrent_market_data.rs` (~600 lines) → `calc/aggregation.rs` → `calc/market_value.rs`. Each step is too small to own the concept.

**Type:** bouncing / unclear boundary.

### 3. AppState is a god bag

`calce-api/src/state.rs` holds 9 `Option` fields. Routes receive the full bag and have to know which fields are safe to use. Only `pool` has a `.require_*()` helper. New services mean editing every consumer.

**Type:** shallow abstraction.

### 4. Permissions logic leaks across modules

Defined in `calce-data/src/permissions.rs` (59 lines) but enforced inside `user_data_store.rs:39+`, then re-checked at route level in `routes/calc.rs:145`. There's no single place to read to know what a request is actually allowed to do. A comment at `permissions.rs:50` flags org-scoped permissions as future work — that change would be painful today.

**Type:** cross-cutting concern that leaked.

### 5. Route handlers are boilerplate-heavy and lack a shared context

9 route files, ~1,477 lines. Every endpoint repeats: extract params → parse → load from state → call core → wrap response. Warning mapping is inline (`routes/calc.rs:100–116`). No shared extractor or response-builder, so every new endpoint is ~50 lines of copypasta.

**Type:** shallow modules + missing composition layer.

### 6. CalculationContext is a trivial tuple

`calce-core/src/context.rs` is 13 lines: just `(base_currency, as_of_date)`. It's threaded through ~20 call sites but adds no value. Adding holiday calendars, tax jurisdiction, etc., requires changing every signature.

**Type:** shallow module that should either be deepened or removed.

### 7. UserDataStore has weak encapsulation

Created mutably during loader setup, then wrapped in `Arc` at runtime. Permission helper at `user_data_store.rs:39` is private and gets duplicated in `permissions.rs`. No builder, so test setup means manual `.add_trade()`/`.add_user()` chains.

**Type:** weak encapsulation + testability.

### 8. CDC split across two crates with no type-level contract

`calce-cdc` does Postgres protocol parsing; `calce-data/src/cdc.rs` does application-level handling. They are not coupled at the type level — easy to add a price-feed type and forget to update CDC. Tests require a live Postgres; integration between the two CDC layers is untested.

**Type:** untested seam.

### 9. calce-core lacks composition tests

Unit tests for aggregation, market_value, volatility are good. But there's only ~100 lines in `crates/calce-core/tests/integration_test.rs`. No test for "market value + volatility + allocation together" or "what if prices are partially missing across a portfolio?" The route at `routes/calc.rs:135–149` chains all of these untested.

**Type:** testability gap at the orchestration level.

## Testing Summary

- **Well tested:** calce-core domain types and calc functions; permissions unit tests.
- **Light:** api routes (1 minimal integration file, no per-handler tests, no auth-flow tests); data loaders.
- **Untested:** AppState wiring, route composition, calculation orchestration, CDC cross-crate seam.

## Notable Observations

- Calculation tags (`#CALC_MV` etc.) are used inconsistently — half-finished discipline.
- `MarketDataService` trait is a strength: enables `TestMarketData` swap-in for tests in calce-core.
- `calce-db` is Python (Alembic) — schema changes can silently break Rust loaders. Type safety is lost at the schema boundary.
- Async is correctly localized to data/api crates; calce-core stays sync.

## Candidate Deepening Opportunities

The friction points above cluster into 6 deepening candidates. Each would replace a fuzzy boundary with a deeper module that's easier to test at its edge:

| # | Cluster | Dependency category | Why it's a candidate |
|---|---|---|---|
| **1** | **Auth as a single deep module** spanning `calce-data/src/auth/*` and `calce-api/src/auth.rs` | Cross-boundary (DB + HTTP) — ports & adapters | Today: 6 + 1 files, two crates, no clear seam. Boundary test would replace ~half a dozen unit tests. |
| **2** | **Market-data loading pipeline** as one deep loader (`loader` + `market_data_builder` + `concurrent_market_data`) | In-process dependency on calce-core | Today: 5-file bounce. A `MarketDataLoader` with one entry point would let you test "DB rows → ready-to-calculate snapshot" at the boundary. |
| **3** | **AppState → typed service registry** | Composition root | Today: 9 Optional fields, no type guidance. Replacing with per-route capability traits would let routes declare what they need and make the wiring testable. |
| **4** | **Permissions as a deep authorization module** | Cross-cutting / pure | Today: split across `permissions.rs`, `user_data_store.rs`, and route handlers. A single `Authorizer` with `can_*` methods would centralize the rules and unblock org-scoped permissions. |
| **5** | **Route composition layer** (shared extractor + response wrapper + warning mapping) | In-process | Today: ~1,477 lines of repeated boilerplate. A `CalcEndpoint`-style helper would shrink each route to the unique bits and make warning handling consistent. |
| **6** | **CDC type-level contract** between `calce-cdc` and `calce-data/src/cdc.rs` | Cross-crate seam | Today: silent coupling — adding a feed type can desync the two halves. A typed change-event enum owned by one crate would make this a compile error. |

## Recommended Priority

By leverage:

1. **#1 (auth)** and **#4 (permissions)** — highest-impact because they touch security and are painful to evolve.
2. **#2 (market-data loader)** — highest-leverage testability win; would unlock real boundary tests for the data path.
3. **#3 (AppState)** — easiest, smallest blast radius.
4. **#5 (route composition)** — mostly ergonomics.
5. **#6 (CDC contract)** — most subtle, hardest to pitch, but prevents a class of silent bugs.
