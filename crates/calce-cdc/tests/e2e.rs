//! End-to-end CDC test: inserts a row into Postgres, verifies the CDC listener
//! picks it up and emits the correct event on the channel.
//!
//! Requires a running Postgres with `wal_level=logical` on the standard local
//! dev port (5433). Skips automatically if the database is unreachable.

use std::time::Duration;

use calce_cdc::{CdcConfig, CdcListener};
use tokio::time::timeout;

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

async fn drop_test_slot(db_url: &str) {
    let Ok((client, conn)) = tokio_postgres::connect(db_url, tokio_postgres::NoTls).await else {
        return;
    };
    tokio::spawn(conn);
    let _ = client
        .execute(
            "SELECT pg_drop_replication_slot(slot_name) FROM pg_replication_slots WHERE slot_name = 'calce_cdc_test_slot'",
            &[],
        )
        .await;
}

async fn insert_test_price(db_url: &str) -> i64 {
    let (client, conn) = tokio_postgres::connect(db_url, tokio_postgres::NoTls)
        .await
        .expect("connect for insert");
    tokio::spawn(conn);

    let row = client
        .query_one("SELECT id FROM instruments LIMIT 1", &[])
        .await
        .expect("need at least one instrument");
    let inst_id: i64 = row.get(0);

    client
        .execute(
            "INSERT INTO prices (instrument_id, price_date, price) \
             VALUES ($1, '2099-12-31', 99999.99) \
             ON CONFLICT (instrument_id, price_date) DO UPDATE SET price = 99999.99",
            &[&inst_id],
        )
        .await
        .expect("insert test price");

    inst_id
}

async fn cleanup_test_price(db_url: &str) {
    let Ok((client, conn)) = tokio_postgres::connect(db_url, tokio_postgres::NoTls).await else {
        return;
    };
    tokio::spawn(conn);
    let _ = client
        .execute("DELETE FROM prices WHERE price_date = '2099-12-31'", &[])
        .await;
}

#[tokio::test]
async fn cdc_receives_price_insert() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("calce_cdc=debug")
        .with_test_writer()
        .try_init();

    let config = test_config();
    let db_url = config.database_url.clone();

    if !db_available().await {
        eprintln!("Skipping CDC E2E test: database not available");
        return;
    }

    drop_test_slot(&db_url).await;

    let (listener, mut rx) = CdcListener::new(config, 256);
    let listener_handle = tokio::spawn(async move { listener.run().await });

    // Give the listener time to connect and start streaming
    tokio::time::sleep(Duration::from_secs(2)).await;

    let expected_id = insert_test_price(&db_url).await;
    eprintln!("Inserted test price for instrument_id: {expected_id}");

    // Drain events until we see a matching prices row, or time out.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let result = timeout(remaining, rx.recv()).await;
        match result {
            Ok(Some(event)) if event.table == "prices" => {
                let got_id: i64 = event
                    .columns
                    .get("instrument_id")
                    .and_then(|v| v.as_deref())
                    .and_then(|s| s.parse().ok())
                    .expect("prices row must carry instrument_id");
                let price: f64 = event
                    .columns
                    .get("price")
                    .and_then(|v| v.as_deref())
                    .and_then(|s| s.parse().ok())
                    .expect("prices row must carry price");
                assert_eq!(got_id, expected_id);
                assert!((price - 99999.99).abs() < 0.001);
                eprintln!("CDC E2E PASS: prices row for id={got_id} price={price}");
                break;
            }
            Ok(Some(_other)) => continue,
            Ok(None) => panic!("CDC channel closed unexpectedly"),
            Err(_) => panic!("Timed out waiting for CDC event"),
        }
    }

    listener_handle.abort();
    cleanup_test_price(&db_url).await;
    drop_test_slot(&db_url).await;
}
