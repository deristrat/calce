# syntax=docker/dockerfile:1.7

FROM python:3.12-slim-bookworm
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates build-essential libpq-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY services/calce-db/pyproject.toml ./pyproject.toml
COPY services/calce-db/alembic.ini ./alembic.ini
COPY services/calce-db/alembic ./alembic
COPY services/calce-db/calce_db ./calce_db
RUN pip install --no-cache-dir .

CMD ["alembic", "upgrade", "head"]
