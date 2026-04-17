import os
import signal
import subprocess
import sys
import time as _time
from pathlib import Path

from invoke import task

# Load .env from project root if present
_dotenv = Path(__file__).parent / ".env"
if _dotenv.exists():
    for line in _dotenv.read_text().splitlines():
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            key, _, value = line.partition("=")
            os.environ.setdefault(key.strip(), value.strip())

PYTHON_CRATE = "crates/calce-python"
MATURIN_MANIFEST = f"{PYTHON_CRATE}/Cargo.toml"
API_PORT = 35701
CONSOLE_PORT = 38100
CALCE_DB = "services/calce-db"
CALCE_AI = "services/calce-ai"
CALCE_CONSOLE = "services/calce-console"

# All Python directories for linting/formatting
PYTHON_DIRS = "services/ crates/calce-python/tests/ tools/"


# ── Server ──────────────────────────────────────────────────────────────
#
#   invoke api          — local Postgres (requires: invoke db, invoke seed-db)
#   invoke ai           — AI chat interface (requires: invoke db, ANTHROPIC_API_KEY)


def _run_api(c, release=False, watch=False):
    env = {"RUST_LOG": "info"}
    flag = " --release" if release else ""
    cargo_cmd = f"run -p calce-api{flag}"
    if watch:
        c.run("bacon --job run-long", pty=True, env=env)
    else:
        c.run(f"cargo {cargo_cmd}", pty=True, env=env)


@task
def api(c, release=False, watch=False):
    """Start API server against local Postgres. Use -r for release, -w for auto-reload."""
    _run_api(c, release=release, watch=watch)


@task
def ai(c):
    """Start AI financial analyst chat (requires DB + ANTHROPIC_API_KEY)."""
    c.run(
        "uv run python -m calce_ai",
        pty=True,
        env={"PYTHONPATH": CALCE_AI},
    )


@task
def console(c):
    """Start the admin console frontend (Vite dev server)."""
    c.run(f"cd {CALCE_CONSOLE} && npm run dev", pty=True)


@task
def console_build(c):
    """Build the admin console for production."""
    c.run(f"cd {CALCE_CONSOLE} && npm run build", pty=True)


@task
def dev(c):
    """Start full dev environment: DB, API (hot-reload), and console."""
    # 1. Ensure DB is running + migrated
    print("Starting database...")
    c.run("docker compose up -d postgres", hide="both")
    c.run(f"cd {CALCE_DB} && uv run alembic upgrade head", hide="both")

    # 2. Start API with hot-reload in background (bacon watches for changes
    #    and restarts the server automatically via the run-long job).
    print("Starting API with hot-reload...")
    env = {"RUST_LOG": "info"}
    api_proc = subprocess.Popen(
        ["bacon", "--headless", "--job", "run-long"],
        env={**os.environ, **env},
    )

    # 3. Start console frontend in background
    print("Starting console frontend...")
    console_proc = subprocess.Popen(
        ["npm", "run", "dev"],
        cwd=CALCE_CONSOLE,
        env={**os.environ},
    )

    def _cleanup():
        for proc in [api_proc, console_proc]:
            proc.terminate()
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
        # Clean up any orphans still bound to our port.
        subprocess.run(
            f"lsof -ti:{API_PORT} | xargs kill -9 2>/dev/null",
            shell=True,
        )

    # 4. Wait for API to be ready, then open console
    print(f"Waiting for API on port {API_PORT}...")
    try:
        import socket
        for _ in range(60):
            try:
                with socket.create_connection(("localhost", API_PORT), timeout=1):
                    break
            except OSError:
                _time.sleep(1)
        else:
            print("Warning: API did not respond within 60s, opening browser anyway")
        print(f"Opening console at http://localhost:{CONSOLE_PORT}")
        c.run(f"open http://localhost:{CONSOLE_PORT}", hide="both")
    except KeyboardInterrupt:
        _cleanup()
        sys.exit(0)

    # 5. Keep running until Ctrl-C
    try:
        api_proc.wait()
    except KeyboardInterrupt:
        print("\nShutting down...")
        _cleanup()


# ── Database ────────────────────────────────────────────────────────────


@task
def db(c):
    """Start the local Postgres database (docker compose)."""
    c.run("docker compose up -d postgres", pty=True)


# ── Dockerized stack ────────────────────────────────────────────────────


@task
def docker_up(c, build=False):
    """Bring up the full containerized stack (postgres + api + ai + console). Use -b to rebuild images."""
    flag = " --build" if build else ""
    c.run(f"docker compose --profile full up -d{flag}", pty=True)
    print()
    print("Dockerized stack is up:")
    print("  Console:   http://localhost:48100")
    print("  API:       http://localhost:45701")
    print("  AI:        http://localhost:45801")
    print("  Postgres:  postgresql://calce:calce@localhost:5433/calce")
    print()
    print("Tail logs:  docker compose --profile full logs -f")
    print("Tear down:  invoke docker-down")


@task
def docker_down(c):
    """Stop the containerized stack (postgres volume is preserved)."""
    c.run("docker compose --profile full down", pty=True)


@task
def db_stop(c):
    """Stop the local Postgres database."""
    c.run("docker compose down", pty=True)


def _alembic(c, args):
    c.run(f"cd {CALCE_DB} && uv run alembic {args}", pty=True)


@task
def db_migrate(c):
    """Run Alembic migrations (upgrade to head)."""
    _alembic(c, "upgrade head")


@task
def db_revision(c, message="auto"):
    """Create a new Alembic migration (autogenerate from models)."""
    _alembic(c, f'revision --autogenerate -m "{message}"')


@task
def db_downgrade(c, revision="-1"):
    """Roll back Alembic migration (default: one step)."""
    _alembic(c, f"downgrade {revision}")


@task
def db_reset(c):
    """Reset database: drop all tables, re-run migrations, then seed."""
    answer = input("This will wipe the entire database. Continue? [y/N] ")
    if answer.lower() != "y":
        print("Aborted.")
        return
    for i in range(3, 0, -1):
        print(f"  Wiping in {i}...")
        _time.sleep(1)
    c.run(
        'docker compose exec -T postgres psql -U calce -d calce -c '
        '"DROP SCHEMA public CASCADE; CREATE SCHEMA public;"',
        pty=True,
    )
    _alembic(c, "upgrade head")
    seed_db(c)


@task
def seed_db(c, instruments=1000, users=100, trades_per_user=100, history_years=5):
    """Seed the database with realistic test data."""
    c.run(
        f"uv run tools/seed_db.py"
        f" --instruments {instruments}"
        f" --users {users}"
        f" --trades-per-user {trades_per_user}"
        f" --history-years {history_years}",
        pty=True,
    )


# ── CDC ─────────────────────────────────────────────────────────────────


@task
def cdc_status(c):
    """Show CDC replication slot status and WAL retention."""
    c.run(
        'docker compose exec -T postgres psql -U calce -d calce -c '
        '"SELECT slot_name, active, restart_lsn, '
        "pg_size_pretty(pg_wal_lsn_diff(pg_current_wal_lsn(), restart_lsn)) AS retained "
        'FROM pg_replication_slots;"',
        pty=True,
    )


@task
def cdc_drop_slot(c):
    """Drop the CDC replication slot (frees retained WAL)."""
    c.run(
        'docker compose exec -T postgres psql -U calce -d calce -c '
        "\"SELECT pg_drop_replication_slot('calce_cdc_slot');\"",
        pty=True,
    )


# ── Njorda ──────────────────────────────────────────────────────────────


@task
def njorda_import(c, from_date="2023-01-01", to_date="", dry_run=False):
    """Import real data from njorda dev DB into local calce DB.

    Requires: Cloud SQL Proxy running (invoke njorda-proxy).
    Passwords are auto-decrypted from SOPS (or set NJORDA_DB_PASSWORD / NJORDA_API_DB_PASSWORD).
    """
    to_flag = f" --to-date {to_date}" if to_date else ""
    dry = " --dry-run" if dry_run else ""
    c.run(
        f"uv run tools/njorda_import.py"
        f" --from-date {from_date}{to_flag}{dry}",
        pty=True,
    )


@task
def njorda_proxy(c):
    """Start Cloud SQL Proxy for njorda dev DB (port 22020)."""
    c.run(
        "cloud-sql-proxy --address 0.0.0.0 --port 22020 "
        "deristrat-njorda-dev:europe-west1:narvik",
        pty=True,
    )



# ── Build ───────────────────────────────────────────────────────────────


@task
def setup(c):
    """Install all dev dependencies (creates .venv via uv)."""
    c.run("uv sync")


@task
def build(c):
    """Build Rust crates and Python extension."""
    c.run("cargo build")
    build_python(c)


@task
def build_python(c):
    """Build Python extension into the venv."""
    # Unset CONDA_PREFIX to avoid maturin conflict with VIRTUAL_ENV
    c.run(f"unset CONDA_PREFIX && uv run maturin develop -m {MATURIN_MANIFEST} --uv")


# ── Lint & Check ────────────────────────────────────────────────────────


@task
def fix(c):
    """Auto-fix formatting and lint errors (Rust + Python)."""
    print("── Rust ──")
    c.run("cargo fmt")
    c.run("cargo clippy --workspace --fix --allow-dirty --allow-staged -- -D warnings")
    print("── Python ──")
    c.run(f"uv run ruff check --fix {PYTHON_DIRS}")
    c.run(f"uv run ruff format {PYTHON_DIRS}")


@task
def check(c):
    """Lint and format-check all code (Rust + Python). No tests."""
    print("── Rust ──")
    c.run("cargo fmt --check")
    c.run("cargo clippy --workspace -- -D warnings")
    print("── Python ──")
    c.run(f"uv run ruff check {PYTHON_DIRS}")
    c.run(f"uv run ruff format --check {PYTHON_DIRS}")


# ── Test ────────────────────────────────────────────────────────────────


@task
def test_rust(c):
    """Run Rust tests."""
    c.run("cargo test")


@task
def test_python(c):
    """Build Python extension and run pytest."""
    build_python(c)
    c.run(f"uv run pytest {PYTHON_CRATE}/tests/ -v")


@task
def test(c):
    """Run all tests (Rust + Python)."""
    test_rust(c)
    test_python(c)


@task
def pre_commit(c):
    """Full pre-push gate: lint + test everything. Run before pushing."""
    check(c)
    test(c)


# ── Utilities ───────────────────────────────────────────────────────────


@task
def smoke_test(c):
    """Run API smoke tests against the running server."""
    c.run("bash tools/smoke_test.sh", pty=True)


@task
def coverage(c, html=False):
    """Run tests with line coverage report. Use --html for a browsable HTML report."""
    fmt = "--html --open" if html else ""
    c.run(
        f"cargo llvm-cov --workspace --ignore-filename-regex calce-python {fmt}",
        pty=True,
    )


@task
def rel(c):
    """Start DB + release-mode API for benchmarking. Ctrl-C to stop."""
    # 1. Ensure DB is running + migrated
    print("Starting database...")
    c.run("docker compose up -d postgres", hide="both")
    c.run(f"cd {CALCE_DB} && uv run alembic upgrade head", hide="both")

    # 2. Build and start API in release mode
    print("Building API in release mode (this may take a while)...")
    c.run("cargo build --release -p calce-api", pty=True)

    print(f"Starting API on port {API_PORT} (release mode)...")
    env = {**os.environ, "RUST_LOG": "info"}
    api_proc = subprocess.Popen(
        ["./target/release/calce-api"],
        env=env,
    )

    def _cleanup():
        api_proc.terminate()
        try:
            api_proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            api_proc.kill()
        subprocess.run(
            f"lsof -ti:{API_PORT} | xargs kill -9 2>/dev/null",
            shell=True,
        )

    # 3. Wait for API to be ready
    print(f"Waiting for API on port {API_PORT}...")
    import urllib.request

    try:
        for _ in range(120):
            try:
                urllib.request.urlopen(f"http://localhost:{API_PORT}/", timeout=1)
                print(f"API ready on port {API_PORT} (release mode). Run 'invoke bench' in another terminal.")
                break
            except Exception:
                _time.sleep(1)
        else:
            print("Warning: API did not respond within 120s")
    except KeyboardInterrupt:
        _cleanup()
        sys.exit(0)

    # 4. Keep running until Ctrl-C
    try:
        api_proc.wait()
    except KeyboardInterrupt:
        print("\nShutting down...")
        _cleanup()


@task
def bench(c, duration="10s", threads=4, connections=50):
    """Load test the API (requires running server). Uses wrk."""
    c.run(
        f"PORT={API_PORT} DURATION={duration} THREADS={threads} CONNECTIONS={connections} "
        "bash tools/bench.sh",
        pty=True,
    )
