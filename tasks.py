import os
import subprocess
import sys
import time as _time

from invoke import task

VENV = ".venv"
PYTHON_CRATE = "crates/calce-python"
MATURIN_MANIFEST = f"{PYTHON_CRATE}/Cargo.toml"
API_PORT = 35701
CALCE_DB = "services/calce-db"
CALCE_AI = "services/calce-ai"


# ── Server ──────────────────────────────────────────────────────────────
#
# Three backends:
#   invoke api          — local Postgres (requires: invoke db, invoke seed-db)
#   invoke ai           — AI chat interface (requires: invoke db, ANTHROPIC_API_KEY)
#   invoke api-njorda   — njorda market-data cache (requires: invoke njorda-fetch)


def _run_api(c, backend="postgres", features="", release=False, watch=False):
    env = {"RUST_LOG": "info"}
    if backend != "postgres":
        env["CALCE_BACKEND"] = backend
    flag = " --release" if release else ""
    cargo_cmd = f"run -p calce-api{flag}{features}"
    if watch:
        c.run(f"cargo watch -x '{cargo_cmd}'", pty=True, env=env)
    else:
        c.run(f"cargo {cargo_cmd}", pty=True, env=env)


@task
def api(c, release=False, watch=False):
    """Start API server against local Postgres. Use -r for release, -w for auto-reload."""
    _run_api(c, release=release, watch=watch)


@task
def api_njorda(c, release=False, watch=False):
    """Start API server against njorda cache (market data only, no users). Use -w for auto-reload."""
    _run_api(c, backend="njorda-cache", features=" --features njorda",
             release=release, watch=watch)


@task
def ai(c):
    """Start AI financial analyst chat (requires DB + ANTHROPIC_API_KEY)."""
    c.run(
        f"{VENV}/bin/python -m calce_ai",
        pty=True,
        env={"PYTHONPATH": CALCE_AI},
    )


@task
def explorer(c):
    """Open the dev console in the browser (API server must be running)."""
    c.run(f"open http://localhost:{API_PORT}")


@task
def dev(c):
    """Start full dev environment: DB, API (hot-reload), and open explorer."""
    # 1. Ensure DB is running + migrated
    print("Starting database...")
    c.run("docker compose up -d postgres", hide="both")
    c.run(f"cd {CALCE_DB} && uv run alembic upgrade head", hide="both")

    # 2. Start API with hot-reload in background
    print("Starting API with hot-reload...")
    env = {"RUST_LOG": "info"}
    api_proc = subprocess.Popen(
        ["cargo", "watch", "-x", "run -p calce-api"],
        env={**os.environ, **env},
    )

    def _kill_api():
        api_proc.terminate()
        try:
            api_proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            api_proc.kill()
        # cargo watch spawns children in separate process groups;
        # clean up any orphans still bound to our port.
        subprocess.run(
            f"lsof -ti:{API_PORT} | xargs kill -9 2>/dev/null",
            shell=True,
        )

    # 3. Wait for API to be ready, then open explorer
    print(f"Waiting for API on port {API_PORT}...")
    try:
        import urllib.request
        for _ in range(60):
            try:
                urllib.request.urlopen(f"http://localhost:{API_PORT}/", timeout=1)
                break
            except Exception:
                _time.sleep(1)
        else:
            print("Warning: API did not respond within 60s, opening browser anyway")
        print(f"Opening explorer at http://localhost:{API_PORT}")
        c.run(f"open http://localhost:{API_PORT}", hide="both")
    except KeyboardInterrupt:
        _kill_api()
        sys.exit(0)

    # 4. Keep running until Ctrl-C
    try:
        api_proc.wait()
    except KeyboardInterrupt:
        print("\nShutting down...")
        _kill_api()


# ── Database ────────────────────────────────────────────────────────────


@task
def db(c):
    """Start the local Postgres database (docker compose)."""
    c.run("docker compose up -d postgres", pty=True)


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
        f"uv run --with psycopg2-binary tools/seed_db.py"
        f" --instruments {instruments}"
        f" --users {users}"
        f" --trades-per-user {trades_per_user}"
        f" --history-years {history_years}",
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
        f"uv run --with psycopg2-binary tools/njorda_import.py"
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


@task
def njorda_fetch(c, from_date="2023-01-01", to_date="2026-03-06", fresh=False):
    """Fetch market data from njorda dev DB into local cache.

    Requires: Cloud SQL Proxy running (invoke njorda-proxy) and NJORDA_DB_PASSWORD env var.
    """
    fresh_flag = " --fresh" if fresh else ""
    c.run(
        f"cargo run -p calce-integrations --features njorda --bin njorda-fetch -- "
        f"--from {from_date} --to {to_date}{fresh_flag}",
        pty=True,
    )


# ── Build ───────────────────────────────────────────────────────────────


@task
def setup(c):
    """Create venv and install Python dev dependencies."""
    c.run(f"uv venv {VENV}")
    c.run(f"uv pip install --python {VENV}/bin/python maturin pytest")


@task
def build(c):
    """Build Rust crates (core + api) and Python extension."""
    c.run("cargo build")
    c.run(f"{VENV}/bin/maturin develop -m {MATURIN_MANIFEST} --uv")


@task
def build_rust(c):
    """Build Rust crates only (excludes calce-python cdylib)."""
    c.run("cargo build")


@task
def build_python(c):
    """Build Python extension into the venv (requires maturin)."""
    c.run(f"{VENV}/bin/maturin develop -m {MATURIN_MANIFEST} --uv")


# ── Test & Check ────────────────────────────────────────────────────────


@task
def check(c):
    """Run clippy and format check (no tests)."""
    c.run("cargo clippy --workspace -- -D warnings")
    c.run("cargo fmt --check")


@task
def test_rust(c):
    """Run Rust tests and clippy."""
    c.run("cargo test")
    c.run("cargo clippy --workspace -- -D warnings")


@task
def test_python(c):
    """Run Python tests."""
    c.run(f"{VENV}/bin/pytest {PYTHON_CRATE}/tests/ -v")


@task
def test(c):
    """Run all tests (Rust + Python)."""
    test_rust(c)
    build_python(c)
    test_python(c)


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
def bench(c, duration="10s", threads=4, connections=50):
    """Load test the API (requires running server). Uses wrk."""
    c.run(
        f"DURATION={duration} THREADS={threads} CONNECTIONS={connections} "
        "bash tools/bench.sh",
        pty=True,
    )
