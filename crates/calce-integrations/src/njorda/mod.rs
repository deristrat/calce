pub mod cache;
pub mod repo;
pub mod types;

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, NaiveDate};

use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_data::InMemoryMarketDataService;

use types::{CacheMetadata, CachedMarketData};

#[derive(Debug, thiserror::Error)]
pub enum NjordaError {
    #[error("Legacy DB error: {0}")]
    Database(sqlx::Error),

    #[error("Cache I/O error: {0}")]
    CacheIo(std::io::Error),

    #[error("Cache format error: {0}")]
    CacheFormat(String),

    #[error("Invalid FX ticker: {0}")]
    InvalidFxTicker(String),

    #[error("Invalid currency code: {0}")]
    InvalidCurrency(String),
}

pub struct NjordaLoader {
    repo: repo::NjordaRepo,
}

impl NjordaLoader {
    /// # Errors
    ///
    /// Returns `NjordaError::Database` if the connection fails.
    pub async fn connect(password: &str) -> Result<Self, NjordaError> {
        let repo = repo::NjordaRepo::connect(password).await?;
        Ok(Self { repo })
    }

    /// Fetch all market data in the date range and return it as a `CachedMarketData`.
    ///
    /// # Errors
    ///
    /// Returns `NjordaError::Database` on query failure.
    /// Returns `NjordaError::InvalidFxTicker` on malformed FX tickers.
    pub async fn fetch(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<CachedMarketData, NjordaError> {
        tracing::info!("Fetching active tickers from {from} to {to}");
        let all_tickers = self.repo.fetch_active_tickers(from, to).await?;
        tracing::info!("Found {} active tickers", all_tickers.len());

        // Separate FX tickers from instrument tickers
        let mut fx_tickers = Vec::new();
        let mut instrument_tickers = Vec::new();
        for ticker in &all_tickers {
            if ticker.contains('/') {
                fx_tickers.push(ticker.clone());
            } else {
                instrument_tickers.push(ticker.clone());
            }
        }

        tracing::info!(
            "Fetching prices for {} instruments and {} FX pairs",
            instrument_tickers.len(),
            fx_tickers.len()
        );

        let (prices, fx_rates, instruments) = tokio::try_join!(
            self.repo.fetch_prices(&instrument_tickers, from, to),
            self.repo.fetch_fx_rates(&fx_tickers, from, to),
            self.repo.fetch_instruments(&instrument_tickers),
        )?;

        tracing::info!(
            "Fetched {} prices, {} FX rates, {} instruments",
            prices.len(),
            fx_rates.len(),
            instruments.len()
        );

        Ok(CachedMarketData {
            metadata: CacheMetadata {
                fetched_at: chrono::Utc::now(),
                date_from: from,
                date_to: to,
                price_count: prices.len(),
                fx_rate_count: fx_rates.len(),
                instrument_count: instruments.len(),
            },
            prices,
            fx_rates,
            instruments,
        })
    }
}

/// Build an `InMemoryMarketDataService` from cached data, with forward-fill
/// for weekends/holidays.
///
/// # Errors
///
/// Returns `NjordaError::InvalidCurrency` if a cached currency code is invalid.
#[allow(clippy::cast_sign_loss)]
pub fn build_service(cached: &CachedMarketData) -> Result<InMemoryMarketDataService, NjordaError> {
    let from = cached.metadata.date_from;
    let to = cached.metadata.date_to;
    let base_day = from.num_days_from_ce();
    let num_days = (to.num_days_from_ce() - base_day + 1) as usize;

    // Group raw prices by ticker
    let mut prices_by_ticker: BTreeMap<&str, BTreeMap<NaiveDate, f64>> = BTreeMap::new();
    for p in &cached.prices {
        prices_by_ticker
            .entry(&p.ticker)
            .or_default()
            .insert(p.date, p.close);
    }

    // Scatter raw prices into dense arrays, then forward-fill
    let mut dense_prices: HashMap<InstrumentId, Vec<f64>> =
        HashMap::with_capacity(prices_by_ticker.len());
    for (ticker, date_map) in &prices_by_ticker {
        let mut arr = vec![f64::NAN; num_days];
        for (&date, &close) in date_map {
            let idx = (date.num_days_from_ce() - base_day) as usize;
            if idx < num_days {
                arr[idx] = close;
            }
        }
        // Forward-fill: carry last known price through gaps
        let mut last = f64::NAN;
        for v in &mut arr {
            if v.is_nan() {
                if !last.is_nan() {
                    *v = last;
                }
            } else {
                last = *v;
            }
        }
        dense_prices.insert(InstrumentId::new(*ticker), arr);
    }

    // Group FX rates by (from, to) pair
    let mut fx_by_pair: BTreeMap<(&str, &str), BTreeMap<NaiveDate, f64>> = BTreeMap::new();
    for r in &cached.fx_rates {
        fx_by_pair
            .entry((&r.from, &r.to))
            .or_default()
            .insert(r.date, r.rate);
    }

    // Collect all unique currency pairs to detect inverses
    let mut currency_pairs_seen: HashSet<(String, String)> = HashSet::new();
    for r in &cached.fx_rates {
        currency_pairs_seen.insert((r.from.clone(), r.to.clone()));
    }

    // Scatter raw FX rates into dense arrays, then forward-fill (direct + inverse)
    let mut dense_fx: HashMap<(Currency, Currency), Vec<f64>> = HashMap::new();
    for ((from_str, to_str), date_map) in &fx_by_pair {
        let from_ccy = Currency::try_new(from_str)
            .map_err(|_| NjordaError::InvalidCurrency((*from_str).to_string()))?;
        let to_ccy = Currency::try_new(to_str)
            .map_err(|_| NjordaError::InvalidCurrency((*to_str).to_string()))?;

        let need_inverse = {
            let inverse_key = (to_str.to_string(), from_str.to_string());
            !currency_pairs_seen.contains(&inverse_key)
        };

        // Direct rate array
        let mut arr = vec![f64::NAN; num_days];
        for (&date, &rate) in date_map {
            let idx = (date.num_days_from_ce() - base_day) as usize;
            if idx < num_days {
                arr[idx] = rate;
            }
        }
        let mut last = f64::NAN;
        for v in &mut arr {
            if v.is_nan() {
                if !last.is_nan() {
                    *v = last;
                }
            } else {
                last = *v;
            }
        }

        if need_inverse {
            let inverse: Vec<f64> = arr
                .iter()
                .map(|&r| {
                    if r == 0.0 || r.is_nan() {
                        f64::NAN
                    } else {
                        1.0 / r
                    }
                })
                .collect();
            dense_fx.insert((to_ccy, from_ccy), inverse);
        }

        dense_fx.insert((from_ccy, to_ccy), arr);
    }

    Ok(InMemoryMarketDataService::from_dense(
        base_day,
        num_days,
        dense_prices,
        dense_fx,
    ))
}

/// Print a human-readable summary of cached market data.
pub fn print_summary(cached: &CachedMarketData) {
    let m = &cached.metadata;
    println!("Njorda market data cache summary:");
    println!("  Date range: {} to {}", m.date_from, m.date_to);
    println!("  Fetched at: {}", m.fetched_at);
    println!("  Instruments: {}", m.instrument_count);
    println!("  Price points: {}", m.price_count);
    println!("  FX rate points: {}", m.fx_rate_count);
}

#[cfg(test)]
mod build_service_tests {
    use super::*;
    use calce_core::domain::instrument::InstrumentId;
    use calce_core::services::market_data::MarketDataService;
    use types::{CachedFxRate, CachedPrice};

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    fn empty_cached(from: NaiveDate, to: NaiveDate) -> CachedMarketData {
        CachedMarketData {
            metadata: CacheMetadata {
                fetched_at: chrono::Utc::now(),
                date_from: from,
                date_to: to,
                price_count: 0,
                fx_rate_count: 0,
                instrument_count: 0,
            },
            prices: vec![],
            fx_rates: vec![],
            instruments: vec![],
        }
    }

    #[test]
    fn empty_data_builds_successfully() {
        let cached = empty_cached(date(2025, 1, 1), date(2025, 1, 10));
        let svc = build_service(&cached).unwrap();
        assert_eq!(svc.instrument_count(), 0);
    }

    #[test]
    fn prices_are_queryable() {
        let mut cached = empty_cached(date(2025, 1, 6), date(2025, 1, 10));
        cached.prices.push(CachedPrice {
            ticker: "AAPL".into(),
            date: date(2025, 1, 6),
            close: 150.0,
        });
        cached.prices.push(CachedPrice {
            ticker: "AAPL".into(),
            date: date(2025, 1, 8),
            close: 155.0,
        });

        let svc = build_service(&cached).unwrap();
        let p = svc
            .get_price(&InstrumentId::new("AAPL"), date(2025, 1, 6))
            .unwrap();
        assert_eq!(p.value(), 150.0);
        let p = svc
            .get_price(&InstrumentId::new("AAPL"), date(2025, 1, 8))
            .unwrap();
        assert_eq!(p.value(), 155.0);
    }

    #[test]
    fn forward_fill_covers_gaps() {
        // Mon=6, Tue=7, Wed=8, Thu=9, Fri=10
        let mut cached = empty_cached(date(2025, 1, 6), date(2025, 1, 10));
        cached.prices.push(CachedPrice {
            ticker: "AAPL".into(),
            date: date(2025, 1, 6), // Monday
            close: 150.0,
        });
        // Gap on Tue and Wed — should forward-fill from Monday's price

        let svc = build_service(&cached).unwrap();
        let p = svc
            .get_price(&InstrumentId::new("AAPL"), date(2025, 1, 7))
            .unwrap();
        assert_eq!(p.value(), 150.0);
        let p = svc
            .get_price(&InstrumentId::new("AAPL"), date(2025, 1, 8))
            .unwrap();
        assert_eq!(p.value(), 150.0);
    }

    #[test]
    fn no_backfill_before_first_price() {
        // Data starts on Wed, so Mon and Tue should have no price
        let mut cached = empty_cached(date(2025, 1, 6), date(2025, 1, 10));
        cached.prices.push(CachedPrice {
            ticker: "AAPL".into(),
            date: date(2025, 1, 8), // Wednesday
            close: 155.0,
        });

        let svc = build_service(&cached).unwrap();
        // Monday before any price — should be NaN → PriceNotFound
        assert!(
            svc.get_price(&InstrumentId::new("AAPL"), date(2025, 1, 6))
                .is_err()
        );
        // Wednesday has the actual price
        let p = svc
            .get_price(&InstrumentId::new("AAPL"), date(2025, 1, 8))
            .unwrap();
        assert_eq!(p.value(), 155.0);
    }

    #[test]
    fn fx_rates_direct_and_forward_fill() {
        let mut cached = empty_cached(date(2025, 1, 6), date(2025, 1, 10));
        cached.fx_rates.push(CachedFxRate {
            from: "USD".into(),
            to: "SEK".into(),
            date: date(2025, 1, 6),
            rate: 10.5,
        });

        let svc = build_service(&cached).unwrap();
        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");

        // Direct lookup
        let rate = svc.get_fx_rate(usd, sek, date(2025, 1, 6)).unwrap();
        assert!((rate.rate - 10.5).abs() < f64::EPSILON);

        // Forward-filled
        let rate = svc.get_fx_rate(usd, sek, date(2025, 1, 8)).unwrap();
        assert!((rate.rate - 10.5).abs() < f64::EPSILON);
    }

    #[test]
    fn fx_inverse_generated_when_missing() {
        // Only USD→SEK provided, not SEK→USD
        let mut cached = empty_cached(date(2025, 1, 6), date(2025, 1, 6));
        cached.fx_rates.push(CachedFxRate {
            from: "USD".into(),
            to: "SEK".into(),
            date: date(2025, 1, 6),
            rate: 10.0,
        });

        let svc = build_service(&cached).unwrap();
        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");

        // Inverse should be auto-generated
        let rate = svc.get_fx_rate(sek, usd, date(2025, 1, 6)).unwrap();
        assert!((rate.rate - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn fx_inverse_not_generated_when_both_directions_exist() {
        let mut cached = empty_cached(date(2025, 1, 6), date(2025, 1, 6));
        cached.fx_rates.push(CachedFxRate {
            from: "USD".into(),
            to: "SEK".into(),
            date: date(2025, 1, 6),
            rate: 10.0,
        });
        cached.fx_rates.push(CachedFxRate {
            from: "SEK".into(),
            to: "USD".into(),
            date: date(2025, 1, 6),
            rate: 0.095, // Slightly different from 1/10
        });

        let svc = build_service(&cached).unwrap();
        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");

        // Should use the explicit rate, not the computed inverse
        let rate = svc.get_fx_rate(sek, usd, date(2025, 1, 6)).unwrap();
        assert!((rate.rate - 0.095).abs() < f64::EPSILON);
    }

    #[test]
    fn invalid_currency_in_fx_returns_error() {
        let mut cached = empty_cached(date(2025, 1, 6), date(2025, 1, 6));
        cached.fx_rates.push(CachedFxRate {
            from: "bad".into(), // lowercase — invalid
            to: "SEK".into(),
            date: date(2025, 1, 6),
            rate: 1.0,
        });

        let result = build_service(&cached);
        assert!(matches!(result, Err(NjordaError::InvalidCurrency(_))));
    }

    #[test]
    fn multiple_instruments() {
        let mut cached = empty_cached(date(2025, 1, 6), date(2025, 1, 7));
        cached.prices.push(CachedPrice {
            ticker: "AAPL".into(),
            date: date(2025, 1, 6),
            close: 150.0,
        });
        cached.prices.push(CachedPrice {
            ticker: "MSFT".into(),
            date: date(2025, 1, 6),
            close: 400.0,
        });

        let svc = build_service(&cached).unwrap();
        assert_eq!(svc.instrument_count(), 2);

        let p = svc
            .get_price(&InstrumentId::new("AAPL"), date(2025, 1, 6))
            .unwrap();
        assert_eq!(p.value(), 150.0);
        let p = svc
            .get_price(&InstrumentId::new("MSFT"), date(2025, 1, 6))
            .unwrap();
        assert_eq!(p.value(), 400.0);
    }
}
