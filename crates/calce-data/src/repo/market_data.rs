use std::collections::HashMap;

use chrono::NaiveDate;
use sqlx::PgPool;

use calce_core::domain::currency::Currency;
use calce_core::domain::fx_rate::FxRate;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::price::Price;

use crate::error::DataResult;

pub struct MarketDataRepo {
    pub(crate) pool: PgPool,
}

impl MarketDataRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_price(
        &self,
        instrument: &InstrumentId,
        date: NaiveDate,
    ) -> DataResult<Option<Price>> {
        let row = sqlx::query_scalar::<_, f64>(
            "SELECT price FROM prices WHERE instrument_id = $1 AND price_date = $2",
        )
        .bind(instrument.as_str())
        .bind(date)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Price::new))
    }

    pub async fn get_price_history(
        &self,
        instrument: &InstrumentId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> DataResult<Vec<(NaiveDate, Price)>> {
        let rows = sqlx::query_as::<_, (NaiveDate, f64)>(
            "SELECT price_date, price FROM prices \
             WHERE instrument_id = $1 AND price_date >= $2 AND price_date <= $3 \
             ORDER BY price_date",
        )
        .bind(instrument.as_str())
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(d, p)| (d, Price::new(p)))
            .collect())
    }

    pub async fn get_fx_rate(
        &self,
        from: Currency,
        to: Currency,
        date: NaiveDate,
    ) -> DataResult<Option<FxRate>> {
        if from == to {
            return Ok(Some(FxRate::identity(from)));
        }
        let row = sqlx::query_scalar::<_, f64>(
            "SELECT rate FROM fx_rates \
             WHERE from_currency = $1 AND to_currency = $2 AND rate_date = $3",
        )
        .bind(from.as_str())
        .bind(to.as_str())
        .bind(date)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| FxRate::new(from, to, r)))
    }

    pub async fn get_prices_batch(
        &self,
        instruments: &[InstrumentId],
        date: NaiveDate,
    ) -> DataResult<HashMap<InstrumentId, Price>> {
        if instruments.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<&str> = instruments.iter().map(InstrumentId::as_str).collect();
        let rows = sqlx::query_as::<_, (String, f64)>(
            "SELECT instrument_id, price FROM prices \
             WHERE instrument_id = ANY($1) AND price_date = $2",
        )
        .bind(&ids)
        .bind(date)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, p)| (InstrumentId::new(id), Price::new(p)))
            .collect())
    }

    pub async fn get_fx_rates_batch(
        &self,
        pairs: &[(Currency, Currency)],
        date: NaiveDate,
    ) -> DataResult<HashMap<(Currency, Currency), FxRate>> {
        if pairs.is_empty() {
            return Ok(HashMap::new());
        }

        let mut result = HashMap::new();

        // Add identity rates
        for &(from, to) in pairs {
            if from == to {
                result.insert((from, to), FxRate::identity(from));
            }
        }

        let non_identity: Vec<_> = pairs.iter().filter(|(f, t)| f != t).collect();
        if non_identity.is_empty() {
            return Ok(result);
        }

        let froms: Vec<&str> = non_identity.iter().map(|(f, _)| f.as_str()).collect();
        let tos: Vec<&str> = non_identity.iter().map(|(_, t)| t.as_str()).collect();

        // Query all needed pairs at once using unnest to pair from/to arrays
        let rows = sqlx::query_as::<_, (String, String, f64)>(
            "SELECT from_currency, to_currency, rate FROM fx_rates \
             WHERE (from_currency, to_currency) IN (SELECT * FROM unnest($1::text[], $2::text[])) \
             AND rate_date = $3",
        )
        .bind(&froms)
        .bind(&tos)
        .bind(date)
        .fetch_all(&self.pool)
        .await?;

        for (from_str, to_str, rate) in rows {
            let from = Currency::new(&from_str);
            let to = Currency::new(&to_str);
            result.insert((from, to), FxRate::new(from, to, rate));
        }

        Ok(result)
    }

    pub async fn insert_price(
        &self,
        instrument: &InstrumentId,
        date: NaiveDate,
        price: Price,
    ) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO prices (instrument_id, price_date, price) VALUES ($1, $2, $3) \
             ON CONFLICT (instrument_id, price_date) DO UPDATE SET price = EXCLUDED.price",
        )
        .bind(instrument.as_str())
        .bind(date)
        .bind(price.value())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_fx_rate(
        &self,
        rate: &FxRate,
        date: NaiveDate,
    ) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO fx_rates (from_currency, to_currency, rate_date, rate) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (from_currency, to_currency, rate_date) DO UPDATE SET rate = EXCLUDED.rate",
        )
        .bind(rate.from.as_str())
        .bind(rate.to.as_str())
        .bind(date)
        .bind(rate.rate)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_instrument(
        &self,
        id: &InstrumentId,
        currency: Currency,
    ) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO instruments (id, currency) VALUES ($1, $2) \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(id.as_str())
        .bind(currency.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
