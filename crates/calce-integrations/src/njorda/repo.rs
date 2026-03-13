use chrono::NaiveDate;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use super::NjordaError;
use super::types::{CachedFxRate, CachedInstrument, CachedPrice};

pub struct NjordaRepo {
    pool: PgPool,
}

impl NjordaRepo {
    /// # Errors
    ///
    /// Returns `NjordaError::Database` if the connection fails.
    pub async fn connect(password: &str) -> Result<Self, NjordaError> {
        let url = format!("postgres://dataapp:{password}@localhost:22020/dataapp");
        let pool = PgPoolOptions::new()
            .max_connections(3)
            .connect(&url)
            .await
            .map_err(NjordaError::Database)?;
        Ok(Self { pool })
    }

    /// Fetch all tickers that have at least one historical price in the date range.
    ///
    /// # Errors
    ///
    /// Returns `NjordaError::Database` on query failure.
    pub async fn fetch_active_tickers(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<String>, NjordaError> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT ticker FROM historical_price \
             WHERE price_date >= $1 AND price_date <= $2 AND close IS NOT NULL",
        )
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(NjordaError::Database)?;
        Ok(rows)
    }

    /// Fetch prices for a set of tickers, deduped by source priority.
    ///
    /// # Errors
    ///
    /// Returns `NjordaError::Database` on query failure.
    pub async fn fetch_prices(
        &self,
        tickers: &[String],
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<CachedPrice>, NjordaError> {
        if tickers.is_empty() {
            return Ok(vec![]);
        }

        let rows = sqlx::query_as::<_, (String, NaiveDate, f64)>(
            "SELECT DISTINCT ON (hp.ticker, hp.price_date) \
                 hp.ticker, hp.price_date, hp.close::float8 \
             FROM historical_price hp \
             JOIN instrument_source isrc \
               ON hp.ticker = isrc.ticker AND hp.source = isrc.source \
             WHERE hp.ticker = ANY($1) \
               AND hp.price_date >= $2 AND hp.price_date <= $3 \
               AND hp.close IS NOT NULL \
             ORDER BY hp.ticker, hp.price_date, isrc.priority ASC",
        )
        .bind(tickers)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(NjordaError::Database)?;

        Ok(rows
            .into_iter()
            .map(|(ticker, date, close)| CachedPrice {
                ticker,
                date,
                close,
            })
            .collect())
    }

    /// Fetch instrument metadata for a set of tickers.
    ///
    /// # Errors
    ///
    /// Returns `NjordaError::Database` on query failure.
    pub async fn fetch_instruments(
        &self,
        tickers: &[String],
    ) -> Result<Vec<CachedInstrument>, NjordaError> {
        if tickers.is_empty() {
            return Ok(vec![]);
        }

        let rows = sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(
            "SELECT ticker, currency, name, isin, type \
             FROM instrument \
             WHERE ticker = ANY($1)",
        )
        .bind(tickers)
        .fetch_all(&self.pool)
        .await
        .map_err(NjordaError::Database)?;

        Ok(rows
            .into_iter()
            .map(
                |(ticker, currency, name, isin, instrument_type)| CachedInstrument {
                    ticker,
                    currency,
                    name,
                    isin,
                    instrument_type,
                },
            )
            .collect())
    }

    /// Fetch FX rates for a set of currency pair tickers (e.g. `SEK/EUR`).
    ///
    /// Uses the same `historical_price` + `instrument_source` tables as regular
    /// prices — FX pairs are just instruments whose ticker looks like `BASE/TARGET`.
    ///
    /// # Errors
    ///
    /// Returns `NjordaError::Database` on query failure.
    pub async fn fetch_fx_rates(
        &self,
        fx_tickers: &[String],
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<CachedFxRate>, NjordaError> {
        if fx_tickers.is_empty() {
            return Ok(vec![]);
        }

        // FX tickers don't have instrument_source entries, so query
        // historical_price directly without the source-priority JOIN.
        let rows = sqlx::query_as::<_, (String, NaiveDate, f64)>(
            "SELECT DISTINCT ON (hp.ticker, hp.price_date) \
                 hp.ticker, hp.price_date, hp.close::float8 \
             FROM historical_price hp \
             WHERE hp.ticker = ANY($1) \
               AND hp.price_date >= $2 AND hp.price_date <= $3 \
               AND hp.close IS NOT NULL \
             ORDER BY hp.ticker, hp.price_date",
        )
        .bind(fx_tickers)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(NjordaError::Database)?;

        let mut rates = Vec::with_capacity(rows.len());
        for (ticker, date, close) in rows {
            let (from_ccy, to_ccy) = parse_fx_ticker(&ticker)?;
            rates.push(CachedFxRate {
                from: from_ccy,
                to: to_ccy,
                date,
                rate: close,
            });
        }
        Ok(rates)
    }
}

/// Parse a legacy FX ticker like `SEK/EUR` into `("SEK", "EUR")`.
///
/// # Errors
///
/// Returns `NjordaError::InvalidFxTicker` if the format is not `XXX/YYY`.
pub fn parse_fx_ticker(ticker: &str) -> Result<(String, String), NjordaError> {
    let parts: Vec<&str> = ticker.split('/').collect();
    if parts.len() != 2 || parts[0].len() != 3 || parts[1].len() != 3 {
        return Err(NjordaError::InvalidFxTicker(ticker.to_string()));
    }
    Ok((parts[0].to_uppercase(), parts[1].to_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_fx_ticker() {
        let (from, to) = parse_fx_ticker("SEK/EUR").unwrap();
        assert_eq!(from, "SEK");
        assert_eq!(to, "EUR");
    }

    #[test]
    fn parse_fx_ticker_rejects_invalid() {
        assert!(parse_fx_ticker("SEKEUR").is_err());
        assert!(parse_fx_ticker("SE/EU").is_err());
        assert!(parse_fx_ticker("SEK/EUR/USD").is_err());
    }
}
