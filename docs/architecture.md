# Architecture

Financial calculation engine for portfolio tracking, valuation, and analytics.

## Design Principles

1. **Pure calculations, impure boundaries** — calculation functions are pure (data in, result out). Side effects (data loading, auth) live at the edges.
2. **Dual API** — every calculation is available in two modes: _stateful_ (engine loads data, then calculates) and _stateless_ (caller provides data directly).
3. **Plain data types** — domain types carry data, not behavior. Business logic lives in `calc/`.
4. **Trait-based data access** — services are traits, swappable for testing, caching, or different backends.
5. **Sync core, async boundaries** — calce-core is 100% sync. Async data loading lives in calce-data. This keeps the core fast to compile, easy to test, and embeddable (PyO3, WASM).

## Crate Structure

```
calce-core    (sync, pure — domain types, calc functions, service traits, no auth)
    ↑
calce-cdc     (async — Postgres logical replication, emits typed CdcEvent)
    ↑
calce-data    (async — data access, authorization, input assembly, CDC wiring)
    ↑
calce-api     (async — axum HTTP handlers, extracts identity, routes to data+calc)

calce-python  (PyO3 bindings, depends on calce-core + calce-data)
    ↑
calce-ai      (Python — Anthropic Claude chat with tools backed by calce-python)
```

## The Sync/Async Bridge

The central architectural pattern. calce-core defines sync service traits (`MarketDataService`) with in-memory implementations. calce-data bridges the gap:

```
API handler → DataService.load_calc_inputs(security_ctx, spec)
  1. Authorize access to all subjects      (sync, calce-data auth)
  2. Load trades from backend              (async)
  3. Batch-load prices + FX for positions  (async, avoids N+1)
  4. Build MarketDataBuilder → ConcurrentMarketData
  5. Return CalcInputs { trades, market_data }

API handler → aggregate_positions + value_positions  (sync, calce-core)
```

Data is loaded async in bulk, packed into in-memory structs, then handed to pure sync functions. calce-core never sees a database or auth types.

## Calculation Engine

Dual API modes, calculation composition, and partial results are documented in [calc-engine.md](calc-engine.md). Calculation formulas and assumptions are in [calculations/methodology.md](calculations/methodology.md).

## Database Schema Management

Schema management (Alembic, invoke commands, models) is documented in [data-modeling.md](data-modeling.md#schema-management).
