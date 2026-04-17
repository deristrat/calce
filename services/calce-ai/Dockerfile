# syntax=docker/dockerfile:1.7

FROM python:3.12-slim-bookworm AS wheel-builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential curl ca-certificates pkg-config python3-dev \
    && rm -rf /var/lib/apt/lists/*
# Install Rust
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.94.1
RUN pip install --no-cache-dir "maturin>=1.0,<2.0"

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN maturin build --release -m crates/calce-python/Cargo.toml -o /wheels \
    --interpreter python3.12

FROM python:3.12-slim-bookworm AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=wheel-builder /wheels/*.whl /tmp/wheels/
RUN pip install --no-cache-dir /tmp/wheels/*.whl && rm -rf /tmp/wheels

COPY services/calce-ai/pyproject.toml ./pyproject.toml
COPY services/calce-ai/calce_ai ./calce_ai
RUN pip install --no-cache-dir .

ENV PORT=35801
EXPOSE 35801
CMD ["sh", "-c", "uvicorn calce_ai.app:app --host 0.0.0.0 --port ${PORT}"]
