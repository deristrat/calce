# Calce Console

Internal admin console for the Calce platform.

## Design System Rules

**All UI must use the design system. No exceptions.**

- Only use `ds-*` CSS classes from `src/design/components.css`
- Only use React components from `src/components/`
- Never write inline styles or one-off CSS classes in pages/layouts
- Never import external CSS frameworks
- All colors, spacing, typography come from CSS custom properties in `src/design/tokens.css`
- Keep the UI information-dense: tight spacing, small text, compact controls
- Tables should show 30+ rows without scrolling
- All lists (dropdowns, tables, etc.) must have a sensible sort order — alphabetical by name is the default
- Numeric table columns must be right-aligned with `meta: { numeric: true }` on the column def — DataTable applies `ds-table__cell--numeric` (right-align + tabular-nums) automatically

### Before writing UI

Always check `src/components/` for an existing component first. If none fits, **create a new component there** (with matching `ds-*` CSS) — do not write page-local markup or ad-hoc classes.

### Forbidden in pages/layouts

Never use these raw HTML elements directly in page or layout files — always use the React wrapper:

- `<input>` → `Input`
- `<select>` → `Select`
- `<button>` → `Button`
- `<table>` (for read-only data) → `DataTable`
- Toggle switch patterns → `Toggle`

`<form>`, `<label>`, `<code>` are OK when paired with the correct `ds-*` class (`ds-form-group`, `ds-label`, `ds-code-block`). Editable config tables may use raw `<table className="ds-table">` since `DataTable` is read-only.

### Never invent class names

If a `ds-*` class you want to use doesn't exist, **add it to `components.css` first**. Don't silently use an undefined class (it just won't apply). Don't invent non-`ds-` class names.

### Never use inline `style={{…}}` in production pages

Use utility classes instead:
- Margins: `ds-mt-{xs,sm,md,lg,xl,2xl}`, `ds-mb-{xs,sm,md,lg,xl,2xl}`
- Flex: `ds-flex`, `ds-flex--between`, `ds-flex--center`, `ds-flex--gap-2`
- Grid: `ds-grid ds-grid--cols-{2,3,4,5}`
- Cursor: `ds-cursor-pointer`
- Scroll: `ds-scroll-y`

The `Design*Page.tsx` showcase pages are exempt (they demo raw tokens).

### Adding new components

1. Add CSS to `src/design/components.css` using `ds-` prefix
2. Create typed React wrapper in `src/components/`
3. Add examples to the Design Showcase page (`/design`)
4. Only then use in pages

### Theme support

Light and dark themes in `tokens.css` via `[data-theme="light"]` and `[data-theme="dark"]`.
Components must work in both — use CSS custom properties, never hardcode colors.

## Stack

- React 19 + TypeScript
- Vite
- React Router v7 (import from `react-router`, not `react-router-dom`)
- TanStack Query v5 (data fetching)
- TanStack Table v8 (tables)
- lightweight-charts (TradingView, price charts)
- Pure CSS with custom properties (no Tailwind/CSS-in-JS)

## API

Backend at `http://localhost:35701`, proxied through Vite so use relative paths (`/v1/...`, `/auth/...`).
API client: `src/api/client.ts`.

## Live Updates

All pages displaying mutable data must use `useEntityEvents` from `src/hooks/useEntityEvents.ts` to receive real-time updates via CDC → SSE. Query keys must start with the plural table name (e.g. `['users', ...]`) for automatic invalidation to work. See `docs/live-updates.md` for the full pattern, query key conventions, and gotchas.

## Development

```bash
cd services/calce-console
npm install
npm run dev
```
