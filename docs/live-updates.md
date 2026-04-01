# Live-Update UI Pattern

The console receives real-time entity change notifications from the backend and automatically refreshes affected queries. **All pages displaying mutable data must use this pattern.**

## Architecture (one sentence)

Postgres CDC detects row changes → backend PubSub coalesces and fans out → SSE pushes `{"table","id"}` events to the browser → `useEntityEvents` invalidates matching TanStack Query caches → React re-renders with fresh data.

See `docs/cdc.md` for the backend/database side.

## How to wire up a page

### 1. Use table-name query keys

TanStack Query keys must start with the **plural table name** so the invalidation logic can match them:

```ts
// List query — key starts with plural table name
useQuery({ queryKey: ['instruments', { page, search }], queryFn: ... })

// Detail query — key starts with singular form + id
useQuery({ queryKey: ['instrument', id], queryFn: ... })
```

The invalidation hook matches on prefix: when an `instruments` CDC event arrives it invalidates all `['instruments', ...]` queries and also `['instrument', <id>]`.

### 2. Call `useEntityEvents` in the page component

```ts
import { useEntityEvents } from '../hooks/useEntityEvents'

export default function InstrumentsPage() {
  useEntityEvents(['instruments'])
  // ... queries, table, etc.
}
```

Pass the table names this page cares about. Pass `[]` to react to all tables.

That's it — no polling, no manual refetch, no WebSocket plumbing.

## Query key naming convention

| Query type | Key shape | Example |
|------------|-----------|---------|
| List / paginated | `[tablePlural, filterParams]` | `['users', { page: 1, search: '' }]` |
| Single entity | `[tableSingular, id]` | `['user', 'abc-123']` |
| Derived view | `[sourceTable, viewName, filterParams]` | `['trades', 'positions', { userId }]` |
| Cross-table aggregate | Use `alsoInvalidate` option | `useEntityEvents([], { alsoInvalidate: ['stats'] })` |

The `useEntityEvents` hook strips the trailing `s` to derive the singular form, so stick to this convention.

### Derived data (positions, accounts)

Positions are computed from trades — they have no separate DB table. Key them under the `trades` prefix so a single `trades` invalidation catches both:

```ts
// Trade list for a user
queryKey: ['trades', { userId }]

// Positions (aggregated from trades) for a user
queryKey: ['trades', 'positions', { userId }]

// Both get invalidated when a trades CDC event arrives
useEntityEvents(['trades'])
```

### Use DB table names, not hyphenated aliases

Query key prefixes must match the Postgres table name exactly so the hook can match CDC events. Use `fx_rates` (underscore, matching the table) not `fx-rates` (hyphen).

### Composite-key entities (e.g. FX rates)

When an entity is identified by a composite key (like a currency pair) rather than a single ID, use the **plural** table name for both list and detail queries:

```ts
// List page
queryKey: ['fx_rates', { page, search }]

// Detail page — still plural, since the singular invalidation
// ['fx_rate', event.id] won't match a composite key anyway.
queryKey: ['fx_rates', from, to]
```

Both get invalidated when a `fx_rates` CDC event arrives via prefix matching on `['fx_rates']`.

## Mutation + CDC dual invalidation

When a page mutates data (e.g. editing a user), invalidate manually in `onSuccess` **and** let CDC handle external changes:

```ts
const mutation = useMutation({
  mutationFn: (data) => api.updateUser(id, data),
  onSuccess: () => {
    queryClient.invalidateQueries({ queryKey: ['user', id] })
    queryClient.invalidateQueries({ queryKey: ['users'] })
  },
})

// CDC covers changes made by other users/systems
useEntityEvents(['users'])
```

The manual invalidation gives instant feedback; CDC catches everything else.

## Constraints and gotchas

- **Admin-only**: SSE events are only sent to admin users (checked in `useEntityEvents`). Non-admin pages that need live data will require a separate mechanism.
- **Monitored tables**: Only tables configured in the CDC listener emit events. If you add a new table, add it to **both** the `CREATE PUBLICATION` statement and the `required` array in `crates/calce-cdc/src/listener.rs`. The `CREATE PUBLICATION` is used for fresh databases; the `required` array ensures existing publications are updated via `ALTER PUBLICATION`. Currently monitored: `users`, `trades`, `instruments`, `organizations`, `accounts`, `prices`, `fx_rates`, `api_keys`.
- **Signal-only**: Events carry the table name and row ID but no payload. The frontend always refetches — it never patches local state from the event.
- **Coalescing**: The backend deduplicates rapid events for the same key within a 100ms window, so bursts of writes produce a single refetch.
- **Query key types**: Use string IDs from `useParams()` in query keys, not numeric conversions. CDC events send string IDs — a mismatch (`5` vs `"5"`) prevents invalidation.

## Reference implementation

- `services/calce-console/src/hooks/useEventSource.ts` — SSE connection with auto-reconnect
- `services/calce-console/src/hooks/useEntityEvents.ts` — TanStack Query invalidation bridge
- `services/calce-console/src/pages/UsersPage.tsx` — list page with live updates
- `services/calce-console/src/pages/UserDetailPage.tsx` — detail page with edit + live updates
