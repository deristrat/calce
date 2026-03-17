# Calce

Financial calculation engine for portfolio tracking.

## Workspace Structure

```
Cargo.toml                  — workspace root
crates/
├── calce-core/             — core Rust library (no DB/async deps)
├── calce-data/             — Postgres-backed storage + DataService
├── calce-integrations/     — external data source integrations (njorda, etc.)
├── calce-api/              — HTTP server, wires data + core
└── calce-python/           — PyO3 bindings (depends on core only)
docs/                       — reference, design and architecture documentation
tools/                      — developer and testing tools, e.g. benchmarking
working_docs/               — ephemeral working notes, design exploration, task tracking
```


## Documentation

| Path | Purpose | When to update |
|------|---------|----------------|
| `docs/architecture.md` | Overall architecture, design principles, layer boundaries | Major structural changes |
| `docs/rust-guidelines.md` | Rust conventions and architecture rationale | When conventions change |
| `docs/calculations/methodology.md` | Calculation formulas and assumptions (`#CALC_*` tags) | Adding/changing a calculation |
| `docs/auth.md` | Authentication and authorization design | Auth changes |

These are **permanent documentation** — keep them accurate but concise.

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

We use Invoke for task automation.

**`invoke check`** — formatting, clippy, and tests. Run regularly during development.
**`invoke test`** — full test suite (Rust + Python). Always run before any commit.

### Python bindings

```sh
maturin develop -m crates/calce-python/Cargo.toml
pytest crates/calce-python/tests/
```
