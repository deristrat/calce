# Calce Console

Internal admin console for the Calce platform.

## Design System Rules

**All UI must use the design system. No exceptions.**

- Only use `ds-*` CSS classes from `src/design/components.css`
- Only use React components from `src/components/`
- Never write inline styles or one-off CSS classes
- Never import external CSS frameworks
- All colors, spacing, typography come from CSS custom properties in `src/design/tokens.css`
- Keep the UI information-dense: tight spacing, small text, compact controls
- Tables should show 30+ rows without scrolling
- All lists (dropdowns, tables, etc.) must have a sensible sort order — alphabetical by name is the default
- Numeric table columns must be right-aligned with `meta: { numeric: true }` on the column def — DataTable applies `ds-table__cell--numeric` (right-align + tabular-nums) automatically

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

## Development

```bash
cd services/calce-console
npm install
npm run dev
```
