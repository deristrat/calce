# Calce

Financial calculation engine for portfolio tracking.

## Workspace Structure

```
Cargo.toml                  — workspace root
crates/
├── calce-core/             — core Rust library (no DB/async deps)
├── calce-datastructs/      — concurrent data structures (caches, pubsub)
├── calce-data/             — Postgres-backed storage + DataService
├── calce-api/              — HTTP server, wires data + core
└── calce-python/           — PyO3 bindings (depends on core + data)
services/
├── calce-ai/               — AI chat interface (Anthropic Claude + calce bindings)
├── calce-console/          — admin console frontend (Vite/TypeScript)
└── calce-db/               — database schema management (Alembic/SQLAlchemy)
docs/                       — reference, design and architecture documentation
tools/                      — developer and testing tools (scripts, benchmarks)
working_docs/               — ephemeral working notes, design exploration, task tracking
```


## Documentation

Permanent docs — keep accurate but concise:

- `docs/architecture.md` — overall architecture, design principles, layer boundaries
- `docs/rust-guidelines.md` — Rust conventions and architecture rationale
- `docs/calculations/methodology.md` — calculation formulas and assumptions (`#CALC_*` tags)
- `docs/auth.md` — authentication (identity verification, tokens, login)
- `docs/permissions.md` — authorization (roles, access rules, enforcement)

### Working Notes (`working_docs/`)

Ephemeral tracking: implementation status, planned features, known issues, task progress. Not permanent docs — expected to go stale and get cleaned up. Use this for longer-running tasks and design exploration.

### Calculation Tags (`#CALC_*`)

Each calculation has a tag (e.g. `#CALC_MV`) that appears in both
`docs/calculations/methodology.md` and the implementing function's doc comment.
Trace with: `grep -r CALC_MV`.

When adding a new calculation you **must**:
1. Add a section in `docs/calculations/methodology.md` with a new `#CALC_*` tag
2. Add the same tag to the implementing function's doc comment

## Comments

Only comment when the comment adds value that the code doesn't already convey.

- Non-obvious domain conventions (sign conventions, currency directionality)
- `# Errors` and `# Panics` sections on public functions (required by `clippy::pedantic`)
- Why something exists when the reason isn't obvious

Don't restate the function/field/type name as a sentence — if the doc comment says what the name already says, delete it.

## Development

[Invoke](https://www.pyinvoke.org/) is the single task runner for everything — builds, tests, linting, database management, servers. Run `invoke --list` to see all available tasks.

### Key tasks

| Task | What it does |
|------|-------------|
| `invoke setup` | Install dev dependencies (`uv sync`) |
| `invoke check` | Lint & format-check all code (clippy + ruff) — no tests |
| `invoke test` | Run all tests (Rust + Python) |
| `invoke pre-commit` | Full pre-push gate: `check` + `test` |
| `invoke dev` | Start DB + API (hot-reload) + console |
| `invoke ai` | Interactive AI analyst chat (requires DB + `ANTHROPIC_API_KEY`) |

### Local dev credentials

Local admin account (created by both `seed-db` and `njorda-import`): `admin@njorda.se` / `protectme`

### Adding new services

When adding a new service or crate with Python code, wire it into the top-level invoke tasks:
- Add its directories to `PYTHON_DIRS` in `tasks.py` so `check` covers it
- If it has its own tests, add them to `test` (or `test_python`)
