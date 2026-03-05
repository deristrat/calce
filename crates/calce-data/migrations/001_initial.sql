CREATE TABLE users (
    id         VARCHAR(64) PRIMARY KEY,
    email      VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE instruments (
    id              VARCHAR(30) PRIMARY KEY,
    isin            VARCHAR(12) UNIQUE,
    name            VARCHAR(200),
    instrument_type VARCHAR(30) NOT NULL DEFAULT 'other',
    currency        CHAR(3) NOT NULL
);

CREATE TABLE accounts (
    id       VARCHAR(64) PRIMARY KEY,
    user_id  VARCHAR(64) NOT NULL REFERENCES users(id),
    currency CHAR(3) NOT NULL,
    label    VARCHAR(200) NOT NULL
);
CREATE INDEX idx_accounts_user ON accounts(user_id);

CREATE TABLE trades (
    id            BIGSERIAL PRIMARY KEY,
    user_id       VARCHAR(64) NOT NULL REFERENCES users(id),
    account_id    VARCHAR(64) NOT NULL REFERENCES accounts(id),
    instrument_id VARCHAR(30) NOT NULL REFERENCES instruments(id),
    quantity      DOUBLE PRECISION NOT NULL,
    price         DOUBLE PRECISION NOT NULL CHECK (price >= 0),
    currency      CHAR(3) NOT NULL,
    trade_date    DATE NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_trades_user ON trades(user_id);
CREATE INDEX idx_trades_instrument_date ON trades(instrument_id, trade_date);

CREATE TABLE prices (
    instrument_id VARCHAR(30) NOT NULL REFERENCES instruments(id),
    price_date    DATE NOT NULL,
    price         DOUBLE PRECISION NOT NULL CHECK (price >= 0),
    PRIMARY KEY (instrument_id, price_date)
);

CREATE TABLE fx_rates (
    from_currency CHAR(3) NOT NULL,
    to_currency   CHAR(3) NOT NULL,
    rate_date     DATE NOT NULL,
    rate          DOUBLE PRECISION NOT NULL CHECK (rate > 0),
    PRIMARY KEY (from_currency, to_currency, rate_date)
);
CREATE INDEX idx_fx_rates_date ON fx_rates(rate_date);
