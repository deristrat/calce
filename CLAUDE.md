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

### Calculation Reference (`docs/calculations/methodology.md`)

Documentation of calculation methodology, assumptions, and formulas.

Each calculation has a tag (e.g. `#CALC_MV`) that appears in both the
methodology doc and the implementing function's doc comment. To trace from
spec to code or vice versa: `grep -r CALC_MV`.

When adding a new calculation you **must**:
1. Add a section in `docs/calculations/methodology.md` with a new `#CALC_*` tag
2. Add the same tag to the implementing function's doc comment

When making significant changes in calculations check that documentation is up to date.

## Comments

Only comment when the comment adds value that the code doesn't already convey.

**Do comment:**
- Non-obvious domain conventions (sign conventions, currency directionality)
- `# Errors` and `# Panics` sections on public functions (required by `clippy::pedantic`)
- Why something exists when the reason isn't obvious (e.g. "Sort for deterministic output")

**Do not comment:**
- `/// Create a new X` — the function is called `new`
- `/// Returns the Y` — the function is called `y()` or `get_y()`
- `/// The Z field` — the field is named `z`
- Module declarations (`pub mod foo`)
- Struct/enum definitions when the name is descriptive
- Enum variants when the variant name + error message are clear

**Rule of thumb:** if the doc comment is just the function/field/type name rephrased as a sentence, delete it.

## Development
```sh
cargo build
cargo test
cargo clippy --workspace -- -D warnings
```

We use Invoke for commands. Everyting a devleoper needs to do on a regular basis should be available via this.

**`invoke check`** runs formatting, clippy, :
- Run it regularly during development
- Ensure it passes before considering any feature complete

**`invoke test`** runs complete tests:
- **Always** run it before any commit

### Python bindings

```sh
maturin develop -m crates/calce-python/Cargo.toml
pytest crates/calce-python/tests/
```
