//! End-to-end CDC test: inserts a row into Postgres, verifies the CDC listener
//! picks it up and emits the correct event on the channel.
//!
//! Requires a running Postgres with `wal_level=logical` on the standard local
//! dev port (5433). Skips automatically if the database is unreachable.

use std::error::Error;
use std::time::Duration;

use calce_cdc::{CdcConfig, CdcEvent, CdcListener, CdcOperation};
use tokio::time::timeout;

type TestResult = Result<(), Box<dyn Error>>;

fn test_db_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or("postgresql://calce:calce@localhost:5433/calce".into())
}

fn test_config() -> CdcConfig {
    CdcConfig {
        database_url: test_db_url(),
        slot_name: "calce_cdc_test_slot".into(),
        publication_name: "calce_cdc_pub".into(),
    }
}

async fn db_available() -> bool {
    tokio_postgres::connect(&test_db_url(), tokio_postgres::NoTls)
        .await
        .is_ok()
}

async fn connect(db_url: &str) -> Result<tokio_postgres::Client, Box<dyn Error>> {
    let (client, conn) = tokio_postgres::connect(db_url, tokio_postgres::NoTls).await?;
    tokio::spawn(conn);
    Ok(client)
}

async fn drop_test_slot(db_url: &str) {
    let Ok(client) = connect(db_url).await else {
        return;
    };
    let _ = client
        .execute(
            "SELECT pg_drop_replication_slot(slot_name) FROM pg_replication_slots WHERE slot_name = 'calce_cdc_test_slot'",
            &[],
        )
        .await;
}

async fn pick_instrument(db_url: &str) -> Result<i64, Box<dyn Error>> {
    let client = connect(db_url).await?;
    let row = client
        .query_one("SELECT id FROM instruments LIMIT 1", &[])
        .await?;
    Ok(row.get(0))
}

async fn upsert_test_price(db_url: &str, inst_id: i64, price: f64) -> Result<(), Box<dyn Error>> {
    let client = connect(db_url).await?;
    client
        .execute(
            "INSERT INTO prices (instrument_id, price_date, price) \
             VALUES ($1, '2099-12-31', $2) \
             ON CONFLICT (instrument_id, price_date) DO UPDATE SET price = EXCLUDED.price",
            &[&inst_id, &price],
        )
        .await?;
    Ok(())
}

async fn cleanup_test_price(db_url: &str) {
    let Ok(client) = connect(db_url).await else {
        return;
    };
    let _ = client
        .execute("DELETE FROM prices WHERE price_date = '2099-12-31'", &[])
        .await;
}

/// Drain events until a `prices` row event arrives, or timeout.
async fn wait_for_prices_event(
    rx: &mut tokio::sync::mpsc::Receiver<CdcEvent>,
    deadline: Duration,
) -> Result<CdcEvent, Box<dyn Error>> {
    let end = tokio::time::Instant::now() + deadline;
    loop {
        let remaining = end.saturating_duration_since(tokio::time::Instant::now());
        match timeout(remaining, rx.recv()).await {
            Ok(Some(event)) if event.table == "prices" => return Ok(event),
            Ok(Some(_)) => {}
            Ok(None) => return Err("CDC channel closed unexpectedly".into()),
            Err(_elapsed) => return Err("timed out waiting for prices event".into()),
        }
    }
}

fn col_str<'a>(event: &'a CdcEvent, key: &str) -> Option<&'a str> {
    event.columns.get(key).and_then(|v| v.as_deref())
}

fn assert_price_row(event: &CdcEvent, expected_id: i64, expected_price: f64) -> TestResult {
    assert_eq!(event.table, "prices");

    let got_id: i64 = col_str(event, "instrument_id")
        .and_then(|s| s.parse().ok())
        .ok_or("missing or unparseable instrument_id")?;
    assert_eq!(got_id, expected_id);

    let price: f64 = col_str(event, "price")
        .and_then(|s| s.parse().ok())
        .ok_or("missing or unparseable price")?;
    assert!(
        (price - expected_price).abs() < 0.001,
        "expected price ~{expected_price}, got {price}"
    );

    let date = col_str(event, "price_date").ok_or("missing price_date")?;
    assert_eq!(date, "2099-12-31");
    Ok(())
}

/// Full round-trip: insert emits an `Insert` event with correct columns,
/// then updating the same row emits an `Update` event (a different pgoutput
/// framing that carries the new tuple after an optional old-tuple prefix).
#[tokio::test]
async fn cdc_emits_insert_then_update() -> TestResult {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("calce_cdc=debug")
        .with_test_writer()
        .try_init();

    let config = test_config();
    let db_url = config.database_url.clone();

    if !db_available().await {
        eprintln!("Skipping CDC E2E test: database not available");
        return Ok(());
    }

    drop_test_slot(&db_url).await;
    cleanup_test_price(&db_url).await;

    let (listener, mut rx) = CdcListener::new(config, 256);
    let listener_handle = tokio::spawn(async move { listener.run().await });

    // Give the listener time to connect and start streaming.
    tokio::time::sleep(Duration::from_secs(2)).await;

    let expected_id = pick_instrument(&db_url).await?;

    // --- INSERT ---
    upsert_test_price(&db_url, expected_id, 11111.11).await?;
    eprintln!("Inserted test price for instrument_id={expected_id}");

    let insert_event = wait_for_prices_event(&mut rx, Duration::from_secs(10)).await?;
    assert_eq!(insert_event.operation, CdcOperation::Insert);
    assert_price_row(&insert_event, expected_id, 11111.11)?;

    // --- UPDATE --- exercises the pgoutput 'U' code path with optional
    // 'K'/'O' old-tuple prefix handling.
    upsert_test_price(&db_url, expected_id, 22222.22).await?;
    eprintln!("Updated test price to 22222.22");

    let update_event = wait_for_prices_event(&mut rx, Duration::from_secs(10)).await?;
    assert_eq!(update_event.operation, CdcOperation::Update);
    assert_price_row(&update_event, expected_id, 22222.22)?;

    eprintln!("CDC E2E PASS: insert + update round-trip");

    listener_handle.abort();
    cleanup_test_price(&db_url).await;
    drop_test_slot(&db_url).await;
    Ok(())
}
