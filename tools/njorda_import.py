#!/usr/bin/env python3
"""Import real data from Njorda dev databases into the local Calce database.

Requires:
  - Cloud SQL Proxy running (invoke njorda-proxy)
  - NJORDA_DB_PASSWORD env var (dataapp database)
  - NJORDA_API_DB_PASSWORD env var (njorda business database)
  - Local calce DB running (invoke db) with migrations applied
"""

import argparse
import json
import os
import subprocess
import sys
import time
from datetime import date

import psycopg2
from psycopg2.extras import execute_values

# --- Njorda instrument type → calce instrument type ---

INSTRUMENT_TYPE_MAP = {
    "STOCK": "stock",
    "BOND": "bond",
    "ETF": "etf",
    "MFUND": "mutual_fund",
    "CERT": "certificate",
    "OPTION": "option",
    "WARRANT": "warrant",
    "SP": "structured_product",
    "FUTURE": "future",
    "INDEX": "other",
    "ETN": "other",
    "CDS": "other",
    "CURRENCY": "other",
    "NOTE": "bond",
    "SERVICE": "other",
    "NONE": "other",
}


def get_password_from_sops(secrets_path):
    """Decrypt a SOPS-encrypted YAML file and extract DATABASE_PASSWORD."""
    try:
        result = subprocess.run(
            ["sops", "-d", "--extract", '["DATABASE_PASSWORD"]', secrets_path],
            capture_output=True,
            text=True,
            check=True,
        )
        return result.stdout.strip()
    except subprocess.CalledProcessError as e:
        print(f"Failed to decrypt {secrets_path}: {e.stderr}", file=sys.stderr)
        sys.exit(1)
    except FileNotFoundError:
        print("sops not found. Install with: brew install sops", file=sys.stderr)
        sys.exit(1)


def connect_njorda_business(port, password):
    """Connect to the njorda business database."""
    url = f"postgresql://njorda:{password}@localhost:{port}/njorda"
    return psycopg2.connect(url)


def connect_njorda_dataapp(port, password):
    """Connect to the njorda market data database."""
    url = f"postgresql://dataapp:{password}@localhost:{port}/dataapp"
    return psycopg2.connect(url)


def connect_calce(db_url):
    """Connect to the local calce database."""
    return psycopg2.connect(db_url)


# --- Read phase: njorda business DB ---


def fetch_accounts_with_trades(cur):
    """Fetch investment/pension accounts that have at least one trade."""
    cur.execute("""
        SELECT DISTINCT a.id, a.owner_id, a.currency, a.name, a.type
        FROM account a
        JOIN account_trade t ON t.account_id = a.id
        WHERE a.type IN ('investment', 'pension')
          AND t.ticker IS NOT NULL
          AND t.quantity IS NOT NULL
          AND t.quantity != 0
    """)
    return cur.fetchall()


def fetch_trades_for_accounts(cur, account_ids):
    """Fetch all valid trades for the given account IDs."""
    cur.execute(
        """
        SELECT t.account_id, t.ticker, t.quantity, t.acquisition_price,
               t.provider_instrument_price, t.provider_instrument_currency,
               t.timestamp
        FROM account_trade t
        WHERE t.account_id = ANY(%s)
          AND t.ticker IS NOT NULL
          AND t.quantity IS NOT NULL
          AND t.quantity != 0
    """,
        (account_ids,),
    )
    return cur.fetchall()


def fetch_users(cur, user_ids):
    """Fetch users by IDs."""
    cur.execute(
        """
        SELECT id, organization_id, email, first_name, last_name
        FROM "user"
        WHERE id = ANY(%s)
    """,
        (user_ids,),
    )
    return cur.fetchall()


def fetch_organizations(cur, org_ids):
    """Fetch organizations by IDs."""
    cur.execute(
        """
        SELECT id, name FROM organization WHERE id = ANY(%s)
    """,
        (org_ids,),
    )
    return cur.fetchall()


# --- Read phase: njorda dataapp DB ---


def fetch_instruments(cur, tickers):
    """Fetch instrument metadata for the given tickers."""
    cur.execute(
        """
        SELECT ticker, currency, name, isin, type, sectors
        FROM instrument
        WHERE ticker = ANY(%s)
    """,
        (tickers,),
    )
    return cur.fetchall()


def fetch_prices(cur, tickers, from_date, to_date):
    """Fetch historical prices, deduped by source priority."""
    cur.execute(
        """
        SELECT DISTINCT ON (hp.ticker, hp.price_date)
            hp.ticker, hp.price_date, hp.close
        FROM historical_price hp
        JOIN instrument_source isrc
          ON hp.ticker = isrc.ticker AND hp.source = isrc.source
        WHERE hp.ticker = ANY(%s)
          AND hp.price_date >= %s AND hp.price_date <= %s
          AND hp.close IS NOT NULL
        ORDER BY hp.ticker, hp.price_date, isrc.priority ASC
    """,
        (tickers, from_date, to_date),
    )
    return cur.fetchall()


def fetch_fx_rates(cur, fx_tickers, from_date, to_date):
    """Fetch FX rates from historical_price (FX pairs stored as XXX/YYY tickers)."""
    cur.execute(
        """
        SELECT DISTINCT ON (hp.ticker, hp.price_date)
            hp.ticker, hp.price_date, hp.close
        FROM historical_price hp
        WHERE hp.ticker = ANY(%s)
          AND hp.price_date >= %s AND hp.price_date <= %s
          AND hp.close IS NOT NULL
        ORDER BY hp.ticker, hp.price_date
    """,
        (fx_tickers, from_date, to_date),
    )
    return cur.fetchall()


def find_fx_tickers(cur, currencies):
    """Find which FX pair tickers exist in the dataapp for the needed currencies."""
    pairs = []
    for a in currencies:
        for b in currencies:
            if a != b:
                pairs.append(f"{a}/{b}")
    if not pairs:
        return []
    cur.execute(
        """
        SELECT ticker FROM instrument WHERE ticker = ANY(%s)
    """,
        (pairs,),
    )
    return [row[0] for row in cur.fetchall()]


# --- Write phase ---


def timed_insert(cur, label, sql, rows, page_size=5000):
    if not rows:
        print(f"  {label}: 0 rows (skipped)")
        return
    t = time.time()
    execute_values(cur, sql, rows, page_size=page_size)
    elapsed = time.time() - t
    print(f"  {label}: {len(rows):,} rows in {elapsed:.1f}s")


def main():
    parser = argparse.ArgumentParser(description="Import Njorda dev data into Calce")
    parser.add_argument("--from-date", default="2023-01-01")
    parser.add_argument("--to-date", default=str(date.today()))
    parser.add_argument(
        "--db-url",
        default="postgres://calce:calce@localhost:5433/calce",
    )
    parser.add_argument("--njorda-port", type=int, default=22020)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    from_date = date.fromisoformat(args.from_date)
    to_date = date.fromisoformat(args.to_date)

    # --- Resolve passwords ---
    dataapp_pw = os.environ.get("NJORDA_DB_PASSWORD")
    api_pw = os.environ.get("NJORDA_API_DB_PASSWORD")

    njorda_repo = os.environ.get("NJORDA_REPO", os.path.expanduser("~/repos/njorda"))
    sops_path = os.path.join(njorda_repo, "services/api/secrets/dev.yml")

    if not dataapp_pw:
        print("Decrypting dataapp password from SOPS...")
        dataapp_sops = os.path.join(njorda_repo, "services/dataapp/secrets/dev.yml")
        dataapp_pw = get_password_from_sops(dataapp_sops)

    if not api_pw:
        print("Decrypting njorda API password from SOPS...")
        api_pw = get_password_from_sops(sops_path)

    # --- Connect ---
    print(f"Connecting to njorda databases (port {args.njorda_port})...")
    biz_conn = connect_njorda_business(args.njorda_port, api_pw)
    data_conn = connect_njorda_dataapp(args.njorda_port, dataapp_pw)
    biz_cur = biz_conn.cursor()
    data_cur = data_conn.cursor()

    t0 = time.time()

    # --- Read: business data ---
    print("\nReading njorda business data...")

    accounts_raw = fetch_accounts_with_trades(biz_cur)
    print(f"  Accounts with trades: {len(accounts_raw)}")

    account_ids = [a[0] for a in accounts_raw]
    user_ids = list({a[1] for a in accounts_raw})
    # account_id -> (owner_id, currency, name)
    account_info = {a[0]: (a[1], a[2], a[3] or f"Account {a[0]}") for a in accounts_raw}

    trades_raw = fetch_trades_for_accounts(biz_cur, account_ids)
    print(f"  Trades: {len(trades_raw)}")

    # Collect unique tickers from trades
    trade_tickers = list({t[1] for t in trades_raw if t[1]})
    print(f"  Unique tickers in trades: {len(trade_tickers)}")

    users_raw = fetch_users(biz_cur, user_ids)
    print(f"  Users: {len(users_raw)}")

    org_ids = list({u[1] for u in users_raw if u[1] is not None})
    orgs_raw = fetch_organizations(biz_cur, org_ids) if org_ids else []
    print(f"  Organizations: {len(orgs_raw)}")

    biz_cur.close()
    biz_conn.close()

    # --- Read: market data ---
    print(f"\nReading njorda market data ({from_date} to {to_date})...")

    instruments_raw = fetch_instruments(data_cur, trade_tickers)
    print(f"  Instruments found: {len(instruments_raw)}")

    # Only fetch prices for instruments that actually exist in dataapp
    known_tickers = [i[0] for i in instruments_raw]

    # Collect all currencies needed for FX rates
    instrument_currencies = {i[1] for i in instruments_raw if i[1]}
    account_currencies = {a[2] for a in accounts_raw}
    all_currencies = list(instrument_currencies | account_currencies)
    print(f"  Currencies in play: {sorted(all_currencies)}")

    fx_tickers = find_fx_tickers(data_cur, all_currencies)
    print(f"  FX pairs available: {len(fx_tickers)}")

    print("  Fetching prices...")
    prices_raw = fetch_prices(data_cur, known_tickers, from_date, to_date)
    print(f"  Prices: {len(prices_raw):,}")

    print("  Fetching FX rates...")
    fx_raw = fetch_fx_rates(data_cur, fx_tickers, from_date, to_date)
    print(f"  FX rates: {len(fx_raw):,}")

    data_cur.close()
    data_conn.close()

    read_time = time.time() - t0
    print(f"\nRead phase done in {read_time:.1f}s")

    # --- Prepare data for calce ---

    # Organizations: (external_id, name)
    org_rows = [(str(org_id), name) for org_id, name in orgs_raw]
    # user org_id -> njorda org_id (string)
    org_id_map = {org_id: str(org_id) for org_id, _ in orgs_raw}

    # Users: (external_id, email, name, org_external_id)
    user_rows = []
    user_org_lookup = {}  # njorda_user_id -> njorda_org_external_id
    for uid, org_id, email, first_name, last_name in users_raw:
        ext_id = str(uid)
        name = f"{first_name or ''} {last_name or ''}".strip() or f"User {uid}"
        org_ext_id = org_id_map.get(org_id) if org_id else None
        user_rows.append((ext_id, email, name, org_ext_id))
        user_org_lookup[uid] = org_ext_id

    # Instruments: (ticker, isin, name, instrument_type, currency, allocations_json)
    # Deduplicate ISINs — calce has a unique constraint on isin
    seen_isins = set()
    instrument_rows = []
    for ticker, currency, name, isin, inst_type, sectors in instruments_raw:
        calce_type = INSTRUMENT_TYPE_MAP.get(inst_type, "other")
        allocations = {}
        if sectors and isinstance(sectors, dict) and len(sectors) > 0:
            allocations = {"sector": sectors}
        # Set ISIN to None if it's a duplicate
        unique_isin = isin
        if isin:
            if isin in seen_isins:
                unique_isin = None
            else:
                seen_isins.add(isin)
        instrument_rows.append(
            (
                ticker,
                unique_isin,
                name,
                calce_type,
                currency or "USD",
                json.dumps(allocations),
            )
        )

    # Prices: (ticker, price_date, price) - will resolve instrument_id after insert
    price_rows = [(ticker, pd, float(close)) for ticker, pd, close in prices_raw]

    # FX rates: parse "XXX/YYY" tickers into (from, to, date, rate)
    fx_rows = []
    for ticker, pd, close in fx_raw:
        parts = ticker.split("/")
        if len(parts) == 2 and len(parts[0]) == 3 and len(parts[1]) == 3:
            fx_rows.append((parts[0], parts[1], pd, float(close)))

    # Trades: need to map account_id and ticker to calce IDs later
    # For now, keep as (njorda_account_id, ticker, quantity, price, currency, trade_date)
    known_ticker_set = set(known_tickers)
    trade_rows_prep = []
    skipped_trades = 0
    for acct_id, ticker, qty, acq_price, prov_price, prov_currency, timestamp in trades_raw:
        if ticker not in known_ticker_set:
            skipped_trades += 1
            continue
        price = float(acq_price) if acq_price else (float(prov_price) if prov_price else None)
        if price is None or price < 0:
            skipped_trades += 1
            continue
        currency = prov_currency or account_info[acct_id][1]
        if not currency or len(currency) != 3:
            currency = "USD"
        trade_date = timestamp.date() if timestamp else None
        if trade_date is None:
            skipped_trades += 1
            continue
        trade_rows_prep.append((acct_id, ticker, float(qty), price, currency, trade_date))

    if skipped_trades:
        print(f"  Skipped {skipped_trades} trades (missing price/date/instrument)")

    if args.dry_run:
        print("\n--- DRY RUN ---")
        print(f"  Organizations: {len(org_rows)}")
        print(f"  Users: {len(user_rows)}")
        print(f"  Accounts: {len(account_info)}")
        print(f"  Instruments: {len(instrument_rows)}")
        print(f"  Prices: {len(price_rows):,}")
        print(f"  FX rates: {len(fx_rows):,}")
        print(f"  Trades: {len(trade_rows_prep):,}")
        return

    # --- Write phase ---
    print(f"\nConnecting to calce DB ({args.db_url})...")
    calce_conn = connect_calce(args.db_url)
    calce_cur = calce_conn.cursor()

    print("Wiping existing data...")
    calce_cur.execute(
        "TRUNCATE trades, accounts, user_credentials, refresh_tokens, users,"
        " organizations, prices, fx_rates, instruments RESTART IDENTITY CASCADE"
    )
    calce_conn.commit()

    print("Inserting data...")

    # Organizations
    timed_insert(
        calce_cur,
        "organizations",
        "INSERT INTO organizations (external_id, name) VALUES %s",
        org_rows,
    )
    calce_conn.commit()

    # Build org external_id -> calce id map
    calce_cur.execute("SELECT external_id, id FROM organizations")
    org_ext_to_calce = {row[0]: row[1] for row in calce_cur.fetchall()}

    # Instruments
    timed_insert(
        calce_cur,
        "instruments",
        "INSERT INTO instruments (ticker, isin, name, instrument_type, currency, allocations) VALUES %s",
        instrument_rows,
    )
    calce_conn.commit()

    # Build ticker -> calce instrument_id map
    calce_cur.execute("SELECT ticker, id FROM instruments")
    ticker_to_id = {row[0]: row[1] for row in calce_cur.fetchall()}

    # Prices (resolve ticker -> instrument_id)
    price_insert_rows = [(ticker_to_id[ticker], pd, p) for ticker, pd, p in price_rows if ticker in ticker_to_id]
    timed_insert(
        calce_cur,
        "prices",
        "INSERT INTO prices (instrument_id, price_date, price) VALUES %s",
        price_insert_rows,
        page_size=10000,
    )

    # FX rates
    timed_insert(
        calce_cur,
        "fx_rates",
        "INSERT INTO fx_rates (from_currency, to_currency, rate_date, rate) VALUES %s",
        fx_rows,
        page_size=10000,
    )

    # Users (with organization_id)
    user_insert_rows = [
        (ext_id, email, name, org_ext_to_calce.get(org_ext_id)) for ext_id, email, name, org_ext_id in user_rows
    ]
    timed_insert(
        calce_cur,
        "users",
        "INSERT INTO users (external_id, email, name, organization_id) VALUES %s",
        user_insert_rows,
    )
    calce_conn.commit()

    # Build user external_id -> calce id map
    calce_cur.execute("SELECT external_id, id FROM users")
    user_ext_to_calce = {row[0]: row[1] for row in calce_cur.fetchall()}

    # Dev admin user (admin@njorda.se / protectme)
    from dev_admin import ensure_admin_user

    _, ph = ensure_admin_user(calce_cur, calce_conn)

    # All imported users get password "protectme" for dev/testing
    password_hash = ph.hash("protectme")
    credential_rows = [(uid, password_hash) for uid in user_ext_to_calce.values()]
    timed_insert(
        calce_cur,
        "user_credentials",
        "INSERT INTO user_credentials (user_id, password_hash) VALUES %s",
        credential_rows,
    )
    calce_conn.commit()

    # Accounts
    # njorda_account_id -> (njorda_user_id, currency, label)
    account_insert_rows = []
    njorda_acct_id_order = []
    # Track (calce_user_id, label) to deduplicate
    seen_user_labels = set()
    for njorda_acct_id, (njorda_user_id, currency, label) in account_info.items():
        user_ext_id = str(njorda_user_id)
        calce_user_id = user_ext_to_calce.get(user_ext_id)
        if calce_user_id is None:
            continue
        base_label = (label or f"Account {njorda_acct_id}")[:190]
        lbl = base_label
        # Ensure unique (user_id, label) by appending account ID if needed
        if (calce_user_id, lbl) in seen_user_labels:
            lbl = f"{base_label} #{njorda_acct_id}"[:200]
        seen_user_labels.add((calce_user_id, lbl))
        account_insert_rows.append((calce_user_id, currency or "USD", lbl))
        njorda_acct_id_order.append(njorda_acct_id)

    timed_insert(
        calce_cur,
        "accounts",
        "INSERT INTO accounts (user_id, currency, label) VALUES %s",
        account_insert_rows,
    )
    calce_conn.commit()

    # Build njorda_account_id -> calce_account_id using insertion order
    njorda_to_calce_acct = {}
    # Get calce accounts ordered by id (matches insertion order)
    calce_cur.execute("SELECT id FROM accounts ORDER BY id")
    calce_acct_ids = [row[0] for row in calce_cur.fetchall()]
    for i, njorda_acct_id in enumerate(njorda_acct_id_order):
        if i < len(calce_acct_ids):
            njorda_to_calce_acct[njorda_acct_id] = calce_acct_ids[i]

    # Trades (resolve all FK references)
    trade_insert_rows = []
    for njorda_acct_id, ticker, qty, price, currency, trade_date in trade_rows_prep:
        calce_acct_id = njorda_to_calce_acct.get(njorda_acct_id)
        calce_instr_id = ticker_to_id.get(ticker)
        njorda_user_id = account_info[njorda_acct_id][0]
        calce_user_id = user_ext_to_calce.get(str(njorda_user_id))
        if calce_acct_id and calce_instr_id and calce_user_id:
            trade_insert_rows.append(
                (
                    calce_user_id,
                    calce_acct_id,
                    calce_instr_id,
                    qty,
                    price,
                    currency[:3],
                    trade_date,
                )
            )

    timed_insert(
        calce_cur,
        "trades",
        "INSERT INTO trades (user_id, account_id, instrument_id, quantity, price, currency, trade_date) VALUES %s",
        trade_insert_rows,
        page_size=10000,
    )

    calce_conn.commit()
    calce_cur.close()
    calce_conn.close()

    total = time.time() - t0
    print(f"\nDone in {total:.1f}s")
    print(f"  {len(org_rows)} organizations")
    print(f"  {len(instrument_rows):,} instruments")
    print(f"  {len(price_insert_rows):,} prices")
    print(f"  {len(fx_rows):,} fx rates")
    print(f"  {len(user_insert_rows)} users (all passwords: protectme)")
    print(f"  {len(account_insert_rows)} accounts")
    print(f"  {len(trade_insert_rows):,} trades")


if __name__ == "__main__":
    main()
