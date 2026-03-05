from invoke import task

VENV = ".venv"
PYTHON_CRATE = "crates/calce-python"
MATURIN_MANIFEST = f"{PYTHON_CRATE}/Cargo.toml"


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
    """Build Rust crates (core + api, excludes calce-python cdylib)."""
    c.run("cargo build")


@task
def build_python(c):
    """Build Python extension into the venv (requires maturin)."""
    c.run(f"{VENV}/bin/maturin develop -m {MATURIN_MANIFEST} --uv")


@task
def test_python(c):
    """Run Python tests."""
    c.run(f"{VENV}/bin/pytest {PYTHON_CRATE}/tests/ -v")


@task
def test_rust(c):
    """Run Rust tests and clippy."""
    c.run("cargo test")
    c.run("cargo clippy -- -D warnings")


@task
def test(c):
    """Run all tests (Rust + Python)."""
    test_rust(c)
    build_python(c)
    test_python(c)


@task
def db(c):
    """Start the local Postgres database (docker compose)."""
    c.run("docker compose up -d postgres", pty=True)


@task
def db_stop(c):
    """Stop the local Postgres database."""
    c.run("docker compose down", pty=True)


@task
def run_api(c, release=False):
    """Run the API server (seeded with example data). Use -r for release build."""
    flag = " --release" if release else ""
    c.run(f"cargo run -p calce-api{flag}")


@task
def dev(c):
    """Run the API server with auto-reload on file changes (requires cargo-watch)."""
    c.run("cargo watch -x 'run -p calce-api'", pty=True)


@task
def bench(c, duration="10s", threads=4, connections=50):
    """Load test the API (requires running server). Uses wrk."""
    c.run(
        f"DURATION={duration} THREADS={threads} CONNECTIONS={connections} "
        "bash tools/bench.sh",
        pty=True,
    )
