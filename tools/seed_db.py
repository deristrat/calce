#!/usr/bin/env python3
"""Seed the Calce database with realistic test data."""

import argparse
import json
import math
import random
import string
import time
from datetime import date, timedelta

import psycopg2
from psycopg2.extras import execute_values

# --- Config ---

CURRENCIES = ["USD", "EUR", "GBP", "SEK", "JPY"]
CURRENCY_WEIGHTS = [0.40, 0.25, 0.15, 0.10, 0.10]

INSTRUMENT_TYPES = [
    "stock", "bond", "etf", "mutual_fund", "certificate",
    "option", "warrant", "structured_product", "future", "other",
]
INSTRUMENT_TYPE_WEIGHTS = [0.40, 0.15, 0.20, 0.10, 0.03, 0.03, 0.02, 0.03, 0.02, 0.02]

# GICS top-level sectors (MSCI/S&P standard)
GICS_SECTORS = [
    "Communication Services", "Consumer Discretionary", "Consumer Staples",
    "Energy", "Financials", "Health Care", "Industrials",
    "Information Technology", "Materials", "Real Estate", "Utilities",
]
GICS_SECTOR_WEIGHTS = [0.09, 0.10, 0.06, 0.04, 0.12, 0.13, 0.09, 0.30, 0.02, 0.02, 0.03]

# Types that get multi-sector breakdowns (funds)
_FUND_TYPES = {"etf", "mutual_fund"}

# FX pairs with approximate base rates.
# We generate all cross-pairs so the API can convert between any two currencies.
_CCY_VS_USD = {"USD": 1.0, "EUR": 1.07, "GBP": 1.24, "SEK": 0.095, "JPY": 0.0067}
FX_PAIRS: list[tuple[str, str, float]] = []
for _a in CURRENCIES:
    for _b in CURRENCIES:
        if _a != _b:
            FX_PAIRS.append((_a, _b, _CCY_VS_USD[_a] / _CCY_VS_USD[_b]))

# --- Helpers ---


def weekdays(start: date, end: date) -> list[date]:
    """Generate all weekdays (Mon-Fri) between start and end inclusive."""
    days = []
    d = start
    while d <= end:
        if d.weekday() < 5:
            days.append(d)
        d += timedelta(days=1)
    return days


def generate_tickers(n: int, rng: random.Random) -> list[str]:
    """Generate n unique ticker symbols (3-5 uppercase letters)."""
    tickers: set[str] = set()
    while len(tickers) < n:
        length = rng.randint(3, 5)
        ticker = "".join(rng.choices(string.ascii_uppercase, k=length))
        tickers.add(ticker)
    return sorted(tickers)


def geometric_brownian_motion(
    start_price: float,
    n_steps: int,
    drift: float,
    vol: float,
    rng: random.Random,
) -> list[float]:
    """Simulate a price path using geometric Brownian motion.

    dt is 1/252 (one trading day).
    """
    dt = 1.0 / 252.0
    prices = [start_price]
    for _ in range(n_steps - 1):
        z = rng.gauss(0, 1)
        ret = (drift - 0.5 * vol * vol) * dt + vol * math.sqrt(dt) * z
        prices.append(prices[-1] * math.exp(ret))
    return prices


# --- Data generators ---


def _gen_sector_allocations(
    instrument_type: str, rng: random.Random,
) -> dict[str, dict[str, float]]:
    """Generate sector allocation weights for an instrument.

    Stocks get a single sector. Funds get a multi-sector breakdown.
    """
    if instrument_type in _FUND_TYPES:
        # Pick 4-8 sectors with random weights, normalized to ~1.0
        n_sectors = rng.randint(4, 8)
        sectors = rng.sample(GICS_SECTORS, k=n_sectors)
        raw = [rng.random() for _ in sectors]
        total = sum(raw)
        weights = {s: round(w / total, 4) for s, w in zip(sectors, raw)}
        return {"sector": weights}
    # Single-sector instruments (stocks, bonds, etc.)
    sector = rng.choices(GICS_SECTORS, weights=GICS_SECTOR_WEIGHTS, k=1)[0]
    return {"sector": {sector: 1.0}}


def gen_instruments(n: int, rng: random.Random) -> list[tuple[str, str, str, str]]:
    """Return list of (ticker, currency, instrument_type, allocations_json)."""
    tickers = generate_tickers(n, rng)
    result = []
    for t in tickers:
        ccy = rng.choices(CURRENCIES, weights=CURRENCY_WEIGHTS, k=1)[0]
        itype = rng.choices(INSTRUMENT_TYPES, weights=INSTRUMENT_TYPE_WEIGHTS, k=1)[0]
        alloc = _gen_sector_allocations(itype, rng)
        result.append((t, ccy, itype, json.dumps(alloc)))
    return result


def gen_prices(
    instruments: list[tuple[str, str, str, str]],
    trading_days: list[date],
    rng: random.Random,
) -> list[tuple[str, date, float]]:
    """Generate price history rows: (instrument_id, date, price)."""
    rows: list[tuple[str, date, float]] = []
    n_days = len(trading_days)
    for ticker, _, _, _ in instruments:
        start_price = rng.uniform(10.0, 500.0)
        drift = rng.uniform(-0.05, 0.15)
        vol = rng.uniform(0.15, 0.50)
        path = geometric_brownian_motion(start_price, n_days, drift, vol, rng)
        for d, p in zip(trading_days, path):
            rows.append((ticker, d, round(p, 4)))
    return rows


def gen_fx_rates(
    trading_days: list[date],
    rng: random.Random,
) -> list[tuple[str, str, date, float]]:
    """Generate FX rate history: (from_ccy, to_ccy, date, rate).

    All cross-pairs are included so the API can convert between any two currencies.
    """
    rows: list[tuple[str, str, date, float]] = []
    for from_ccy, to_ccy, base_rate in FX_PAIRS:
        rate = base_rate
        for d in trading_days:
            rate *= math.exp(rng.gauss(0, 0.003))
            rate = max(rate, 0.001)  # floor
            rows.append((from_ccy, to_ccy, d, round(rate, 6)))
    return rows


ORG_NAMES = [
    "Acme Wealth", "Nordic Capital Partners", "Coastal Investments",
    "Summit Advisory", "Horizon Asset Management",
]


def gen_organizations(n_orgs: int) -> list[tuple[str, str]]:
    """Return organizations: (external_id, name)."""
    orgs = []
    for i in range(1, n_orgs + 1):
        oid = f"org_{i:03d}"
        name = ORG_NAMES[(i - 1) % len(ORG_NAMES)]
        if i > len(ORG_NAMES):
            name = f"{name} {i}"
        orgs.append((oid, name))
    return orgs


def gen_users_and_accounts(
    n_users: int,
    orgs: list[tuple[str, str]],
    rng: random.Random,
) -> tuple[list[tuple[str, str]], list[tuple[str, str, str]], dict[str, str]]:
    """Return (users, accounts, user_org_map).

    users: (external_id, email)
    accounts: (user_external_id, currency, label)
    user_org_map: user_external_id -> org_external_id (for users that have an org)
    """
    users = []
    accounts = []
    user_org_map: dict[str, str] = {}
    org_ids = [oid for oid, _ in orgs]
    for i in range(1, n_users + 1):
        uid = f"user_{i:03d}"
        email = f"{uid}@example.com"
        users.append((uid, email))
        # 80% of users belong to an organization
        if rng.random() < 0.8:
            user_org_map[uid] = rng.choice(org_ids)
        # 1-3 accounts per user in different currencies
        n_accts = rng.randint(1, 3)
        acct_currencies = rng.sample(CURRENCIES, k=n_accts)
        for j, ccy in enumerate(acct_currencies, 1):
            label = f"{uid} {ccy} account"
            accounts.append((uid, ccy, label))
    return users, accounts, user_org_map


def gen_trades(
    account_map: dict[int, tuple[str, str]],  # db_id -> (user_external_id, currency)
    instruments: list[tuple[str, str, str, str]],  # (ticker, ccy, instrument_type, alloc_json)
    price_lookup: dict[tuple[str, date], float],
    trading_days: list[date],
    avg_trades_per_user: int,
    rng: random.Random,
) -> list[tuple[str, int, str, float, float, str, date]]:
    """Generate trades: (user_external_id, account_id, ticker, qty, price, ccy, date)."""
    # Group accounts by user
    user_accounts: dict[str, list[tuple[int, str]]] = {}
    for acct_id, (user_ext_id, ccy) in account_map.items():
        user_accounts.setdefault(user_ext_id, []).append((acct_id, ccy))

    # Build instrument lookup by currency
    instr_ccy: dict[str, str] = {}
    for ticker, ccy, _, _ in instruments:
        instr_ccy[ticker] = ccy

    all_tickers = [t for t, _, _, _ in instruments]
    rows: list[tuple[str, int, str, float, float, str, date]] = []

    for user_id, accts in user_accounts.items():
        # Pick 5-20 instruments for this user
        n_instruments = min(rng.randint(5, 20), len(all_tickers))
        user_instruments = rng.sample(all_tickers, k=n_instruments)

        # Power-law weighting: some instruments get many trades
        weights = [1.0 / (i + 1) ** 0.8 for i in range(n_instruments)]
        total_w = sum(weights)
        weights = [w / total_w for w in weights]

        # Number of trades for this user (some variance)
        n_trades = max(1, int(rng.gauss(avg_trades_per_user, avg_trades_per_user * 0.3)))

        # Track net position per instrument to avoid large negative positions
        net_qty: dict[str, float] = {}

        for _ in range(n_trades):
            ticker = rng.choices(user_instruments, weights=weights, k=1)[0]
            ccy = instr_ccy[ticker]

            # Pick an account (prefer matching currency, fall back to any)
            matching = [(a, c) for a, c in accts if c == ccy]
            acct_id, acct_ccy = rng.choice(matching) if matching else rng.choice(accts)

            # Trade date: biased toward recent (exponential)
            idx = int(len(trading_days) * (1 - rng.expovariate(3.0) % 1.0))
            idx = max(0, min(idx, len(trading_days) - 1))
            trade_date = trading_days[idx]

            # Get price on trade date (or nearest)
            base_price = price_lookup.get((ticker, trade_date))
            if base_price is None:
                continue
            # Small slippage ±0.5%
            price = base_price * (1 + rng.uniform(-0.005, 0.005))

            # Decide buy or sell
            current_pos = net_qty.get(ticker, 0.0)
            if current_pos <= 0 or rng.random() < 0.7:
                # Buy
                qty = round(rng.uniform(1, 200), 2)
            else:
                # Sell (up to current position)
                max_sell = min(current_pos, 200)
                qty = -round(rng.uniform(1, max(1, max_sell)), 2)

            net_qty[ticker] = current_pos + qty
            rows.append((user_id, acct_id, ticker, qty, round(price, 4), ccy, trade_date))

    return rows


# --- Main ---


def main():
    parser = argparse.ArgumentParser(description="Seed the Calce database with test data")
    parser.add_argument("--instruments", type=int, default=1000)
    parser.add_argument("--organizations", type=int, default=5)
    parser.add_argument("--users", type=int, default=100)
    parser.add_argument("--trades-per-user", type=int, default=100)
    parser.add_argument("--history-years", type=int, default=5)
    parser.add_argument(
        "--db-url",
        default="postgres://calce:calce@localhost:5433/calce",
    )
    parser.add_argument("--seed", type=int, default=42)
    args = parser.parse_args()

    rng = random.Random(args.seed)

    end_date = date.today()
    start_date = end_date - timedelta(days=args.history_years * 365)
    trading_days = weekdays(start_date, end_date)

    t0 = time.time()

    # --- Generate data ---
    print(f"Generating {args.instruments} instruments...")
    instruments = gen_instruments(args.instruments, rng)

    print(f"Generating prices ({len(trading_days)} trading days × {args.instruments} instruments)...")
    price_rows = gen_prices(instruments, trading_days, rng)
    # Build lookup for trade generation
    price_lookup = {(r[0], r[1]): r[2] for r in price_rows}

    print(f"Generating FX rates ({len(FX_PAIRS)} pairs × {len(trading_days)} days)...")
    fx_rows = gen_fx_rates(trading_days, rng)

    print(f"Generating {args.organizations} organizations...")
    organizations = gen_organizations(args.organizations)

    print(f"Generating {args.users} users with accounts...")
    users, accounts, user_org_map = gen_users_and_accounts(args.users, organizations, rng)

    gen_time = time.time() - t0
    print(f"Data generated in {gen_time:.1f}s")

    # --- Insert into DB ---
    print(f"\nConnecting to {args.db_url}...")
    conn = psycopg2.connect(args.db_url)
    cur = conn.cursor()

    print("Truncating tables...")
    cur.execute("TRUNCATE trades, accounts, users, organizations, prices, fx_rates, instruments RESTART IDENTITY CASCADE")

    def timed_insert(label, sql, rows, page_size=5000):
        t = time.time()
        execute_values(cur, sql, rows, page_size=page_size)
        elapsed = time.time() - t
        print(f"  {label}: {len(rows):,} rows in {elapsed:.1f}s")

    timed_insert(
        "instruments",
        "INSERT INTO instruments (ticker, currency, instrument_type, allocations) VALUES %s",
        instruments,
    )

    # Build ticker -> instrument_id map (need integer FKs for prices/trades)
    conn.commit()
    cur.execute("SELECT ticker, id FROM instruments")
    ticker_to_id: dict[str, int] = {row[0]: row[1] for row in cur.fetchall()}

    timed_insert(
        "prices",
        "INSERT INTO prices (instrument_id, price_date, price) VALUES %s",
        [(ticker_to_id[ticker], d, p) for ticker, d, p in price_rows],
        page_size=10000,
    )
    timed_insert(
        "fx_rates",
        "INSERT INTO fx_rates (from_currency, to_currency, rate_date, rate) VALUES %s",
        fx_rows,
    )
    timed_insert(
        "organizations",
        "INSERT INTO organizations (external_id, name) VALUES %s",
        organizations,
    )

    # Build org external_id -> internal id map
    conn.commit()
    cur.execute("SELECT external_id, id FROM organizations")
    org_to_id: dict[str, int] = {row[0]: row[1] for row in cur.fetchall()}

    timed_insert(
        "users",
        "INSERT INTO users (external_id, email, organization_id) VALUES %s",
        [(uid, email, org_to_id.get(user_org_map.get(uid, ""), 0) or None)
         for uid, email in users],
    )

    # Build user external_id -> internal id map
    conn.commit()
    cur.execute("SELECT external_id, id FROM users")
    user_to_id: dict[str, int] = {row[0]: row[1] for row in cur.fetchall()}

    timed_insert(
        "accounts",
        "INSERT INTO accounts (user_id, currency, label) VALUES %s",
        [(user_to_id[user_ext_id], ccy, label) for user_ext_id, ccy, label in accounts],
    )

    # Build account_id map: db_id -> (user_external_id, currency)
    conn.commit()
    cur.execute("SELECT a.id, u.external_id, a.currency FROM accounts a JOIN users u ON a.user_id = u.id")
    account_map: dict[int, tuple[str, str]] = {row[0]: (row[1], row[2]) for row in cur.fetchall()}

    print(f"Generating trades (~{args.trades_per_user} per user)...")
    trade_rows = gen_trades(
        account_map, instruments, price_lookup, trading_days, args.trades_per_user, rng
    )

    timed_insert(
        "trades",
        "INSERT INTO trades (user_id, account_id, instrument_id, quantity, price, currency, trade_date) VALUES %s",
        [(user_to_id[user_ext_id], acct_id, ticker_to_id[ticker], qty, price, ccy, trade_date)
         for user_ext_id, acct_id, ticker, qty, price, ccy, trade_date in trade_rows],
        page_size=10000,
    )

    conn.commit()
    cur.close()
    conn.close()

    total = time.time() - t0
    print(f"\nDone in {total:.1f}s total")
    print(f"  {len(organizations):,} organizations")
    print(f"  {len(instruments):,} instruments")
    print(f"  {len(price_rows):,} prices")
    print(f"  {len(fx_rows):,} fx rates")
    print(f"  {len(users):,} users ({len(user_org_map)} with org)")
    print(f"  {len(accounts):,} accounts")
    print(f"  {len(trade_rows):,} trades")

    # Print some sample user IDs for smoke testing
    print(f"\nSample users: {users[0][0]}, {users[len(users)//2][0]}, {users[-1][0]}")
    sample_ticker = instruments[0][0]
    print(f"Sample instrument: {sample_ticker}")


if __name__ == "__main__":
    main()
