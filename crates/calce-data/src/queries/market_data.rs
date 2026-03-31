use chrono::NaiveDate;
use sqlx::PgPool;

use calce_core::domain::currency::Currency;
use calce_core::domain::fx_rate::FxRate;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::price::Price;

use serde_json::Value as JsonValue;

use crate::error::{DataError, DataResult};

fn parse_currency(column: &str, value: String) -> DataResult<Currency> {
    Currency::try_new(&value).map_err(|_| DataError::InvalidDbData {
        column: column.into(),
        value,
        reason: "not a valid 3-letter uppercase currency code".into(),
    })
}

pub struct MarketDataRepo {
    pool: PgPool,
}

impl MarketDataRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_all_prices(&self) -> DataResult<Vec<(InstrumentId, NaiveDate, Price)>> {
        let rows = sqlx::query_as::<_, (String, NaiveDate, f64)>(
            "SELECT i.ticker, p.price_date, p.price FROM prices p \
             JOIN instruments i ON p.instrument_id = i.id \
             ORDER BY i.ticker, p.price_date",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, d, p)| (InstrumentId::new(id), d, Price::new(p)))
            .collect())
    }

    pub async fn get_all_fx_rates(&self) -> DataResult<Vec<(NaiveDate, FxRate)>> {
        let rows = sqlx::query_as::<_, (String, String, NaiveDate, f64)>(
            "SELECT from_currency, to_currency, rate_date, rate FROM fx_rates ORDER BY rate_date",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|(from_str, to_str, date, rate)| {
                let from = parse_currency("from_currency", from_str)?;
                let to = parse_currency("to_currency", to_str)?;
                Ok((date, FxRate::new(from, to, rate)))
            })
            .collect()
    }

    pub async fn list_instruments(
        &self,
    ) -> DataResult<Vec<(i64, String, String, Option<String>, String, JsonValue)>> {
        let rows = sqlx::query_as::<_, (i64, String, String, Option<String>, String, JsonValue)>(
            "SELECT id, ticker, currency, name, instrument_type, allocations \
             FROM instruments ORDER BY ticker",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Batch upsert prices using a single `UNNEST` query.
    pub async fn batch_upsert_prices(
        &self,
        tickers: &[&str],
        date: NaiveDate,
        prices: &[f64],
    ) -> DataResult<u64> {
        let result = sqlx::query(
            "INSERT INTO prices (instrument_id, price_date, price) \
             SELECT i.id, $1, u.price \
             FROM UNNEST($2::text[], $3::float8[]) AS u(ticker, price) \
             JOIN instruments i ON i.ticker = u.ticker \
             ON CONFLICT ON CONSTRAINT uq_prices_instrument_date \
             DO UPDATE SET price = EXCLUDED.price",
        )
        .bind(date)
        .bind(tickers)
        .bind(prices)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Batch upsert FX rates using a single `UNNEST` query.
    pub async fn batch_upsert_fx_rates(
        &self,
        from_currencies: &[&str],
        to_currencies: &[&str],
        date: NaiveDate,
        rates: &[f64],
    ) -> DataResult<u64> {
        let result = sqlx::query(
            "INSERT INTO fx_rates (from_currency, to_currency, rate_date, rate) \
             SELECT u.from_ccy, u.to_ccy, $1, u.rate \
             FROM UNNEST($2::char(3)[], $3::char(3)[], $4::float8[]) AS u(from_ccy, to_ccy, rate) \
             ON CONFLICT (from_currency, to_currency, rate_date) \
             DO UPDATE SET rate = EXCLUDED.rate",
        )
        .bind(date)
        .bind(from_currencies)
        .bind(to_currencies)
        .bind(rates)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}
