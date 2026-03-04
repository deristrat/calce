use chrono::NaiveDate;

use crate::domain::instrument::InstrumentId;
use crate::domain::price::Price;
use crate::error::{CalceError, CalceResult};
use crate::services::market_data::MarketDataService;

const TRADING_DAYS_PER_YEAR: f64 = 252.0;
const MIN_HISTORY_DAYS: i64 = 60;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VolatilityResult {
    pub annualized_volatility: f64,
    pub daily_volatility: f64,
    pub num_observations: usize,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
}

/// `#CALC_VOL`
///
/// Historical realized volatility: annualized standard deviation of
/// logarithmic daily returns.
///
/// # Errors
///
/// Returns `InsufficientData` when:
/// - fewer than 2 valid (positive) prices exist in the lookback window
/// - the earliest valid price is less than 60 days before `as_of_date`
/// - fewer than 80% of price records in the period have positive values
/// - the computed standard deviation is NaN or infinite
pub fn calculate_volatility(
    instrument: &InstrumentId,
    as_of_date: NaiveDate,
    lookback_days: u32,
    market_data: &dyn MarketDataService,
) -> CalceResult<VolatilityResult> {
    let from = as_of_date - chrono::Days::new(u64::from(lookback_days));
    let all_prices = market_data.get_price_history(instrument, from, as_of_date)?;

    let valid: Vec<(NaiveDate, Price)> = all_prices
        .iter()
        .filter(|(_, p)| p.value() > 0.0)
        .copied()
        .collect();

    validate(instrument, as_of_date, &all_prices, &valid)?;

    let log_returns: Vec<f64> = valid
        .windows(2)
        .map(|w| (w[1].1.value() / w[0].1.value()).ln())
        .collect();

    let daily_vol = sample_std_dev(&log_returns);

    if !daily_vol.is_finite() {
        return Err(CalceError::InsufficientData {
            instrument: instrument.clone(),
            reason: "standard deviation is not finite".into(),
        });
    }

    let annualized_vol = daily_vol * TRADING_DAYS_PER_YEAR.sqrt();

    // Safe: validate() ensures valid has at least 2 entries.
    let start_date = valid[0].0;
    let end_date = valid[valid.len() - 1].0;

    Ok(VolatilityResult {
        annualized_volatility: annualized_vol,
        daily_volatility: daily_vol,
        num_observations: log_returns.len(),
        start_date,
        end_date,
    })
}

fn validate(
    instrument: &InstrumentId,
    as_of_date: NaiveDate,
    all_prices: &[(NaiveDate, Price)],
    valid: &[(NaiveDate, Price)],
) -> CalceResult<()> {
    let err = |reason: &str| CalceError::InsufficientData {
        instrument: instrument.clone(),
        reason: reason.into(),
    };

    if valid.len() < 2 {
        return Err(err("fewer than 2 valid prices"));
    }

    let first_date = valid[0].0;
    if (as_of_date - first_date).num_days() < MIN_HISTORY_DAYS {
        return Err(err("need at least 60 days of history"));
    }

    // Completeness: of all records from first valid price onward,
    // at least 80% must be positive.
    let records_in_period = all_prices.iter().filter(|(d, _)| *d >= first_date).count();
    if records_in_period > 0 && valid.len() * 100 / records_in_period < 80 {
        return Err(err("less than 80% price coverage"));
    }

    Ok(())
}

#[allow(clippy::cast_precision_loss)] // price counts will never exceed 2^52
fn sample_std_dev(values: &[f64]) -> f64 {
    let n = values.len();
    if n < 2 {
        return f64::NAN;
    }
    let nf = n as f64;
    let mean = values.iter().sum::<f64>() / nf;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (nf - 1.0);
    variance.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::instrument::InstrumentId;
    use crate::domain::price::Price;
    use crate::error::CalceError;
    use crate::services::market_data::InMemoryMarketDataService;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    /// Generate consecutive daily prices starting from `start` for `count` days.
    fn add_prices(
        md: &mut InMemoryMarketDataService,
        instrument: &InstrumentId,
        start: NaiveDate,
        values: &[f64],
    ) {
        for (i, &v) in values.iter().enumerate() {
            let d = start + chrono::Days::new(i as u64);
            md.add_price(instrument, d, Price::new(v));
        }
    }

    #[test]
    fn constant_price_gives_zero_volatility() {
        let inst = InstrumentId::new("CONST");
        let as_of = date(2025, 6, 1);
        let start = date(2025, 1, 1); // 151 days before as_of
        let mut md = InMemoryMarketDataService::new();

        // 152 days of the same price
        let prices: Vec<f64> = vec![100.0; 152];
        add_prices(&mut md, &inst, start, &prices);

        let result = calculate_volatility(&inst, as_of, 365, &md).unwrap();

        assert_eq!(result.annualized_volatility, 0.0);
        assert_eq!(result.daily_volatility, 0.0);
        assert_eq!(result.num_observations, 151); // 152 prices → 151 returns
        assert_eq!(result.start_date, start);
        assert_eq!(result.end_date, as_of);
    }

    #[test]
    fn known_volatility_from_alternating_returns() {
        // Prices alternate between 100 and 101 → log returns of ±ln(1.01)
        let inst = InstrumentId::new("ALT");
        let as_of = date(2025, 6, 1);
        let start = date(2025, 1, 1);
        let mut md = InMemoryMarketDataService::new();

        let mut prices = Vec::new();
        for i in 0..152 {
            prices.push(if i % 2 == 0 { 100.0 } else { 101.0 });
        }
        add_prices(&mut md, &inst, start, &prices);

        let result = calculate_volatility(&inst, as_of, 365, &md).unwrap();

        // 151 returns, alternating ln(1.01) and ln(100/101)
        // With odd count (151): mean is slightly nonzero, but daily vol ≈ ln(1.01)
        let r = (1.01_f64).ln();
        assert!((result.daily_volatility - r).abs() < 0.001);
        assert!((result.annualized_volatility - r * 252_f64.sqrt()).abs() < 0.02);
        assert_eq!(result.num_observations, 151);
    }

    #[test]
    fn fewer_than_two_prices_fails() {
        let inst = InstrumentId::new("LONELY");
        let mut md = InMemoryMarketDataService::new();
        md.add_price(&inst, date(2025, 1, 1), Price::new(100.0));

        let result = calculate_volatility(&inst, date(2025, 6, 1), 365, &md);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CalceError::InsufficientData { .. }
        ));
    }

    #[test]
    fn history_too_short_fails() {
        // All prices within the last 30 days — fails the 60-day depth check
        let inst = InstrumentId::new("NEW");
        let as_of = date(2025, 6, 1);
        let start = date(2025, 5, 10); // only 22 days before as_of
        let mut md = InMemoryMarketDataService::new();

        let prices: Vec<f64> = (0..23).map(|i| 100.0 + f64::from(i)).collect();
        add_prices(&mut md, &inst, start, &prices);

        let result = calculate_volatility(&inst, as_of, 365, &md);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CalceError::InsufficientData { .. }
        ));
    }

    #[test]
    fn low_completeness_fails() {
        // Insert 100 price records but make most of them zero
        let inst = InstrumentId::new("SPARSE");
        let as_of = date(2025, 6, 1);
        let start = date(2025, 1, 1);
        let mut md = InMemoryMarketDataService::new();

        // 152 records: only first 20 are valid, rest are zero → 13% completeness
        let mut prices = vec![100.0; 20];
        prices.extend(vec![0.0; 132]);
        add_prices(&mut md, &inst, start, &prices);

        let result = calculate_volatility(&inst, as_of, 365, &md);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CalceError::InsufficientData { .. }
        ));
    }

    #[test]
    fn zero_prices_are_filtered_from_returns() {
        // Mix valid prices with some zero entries — zeros should be ignored,
        // not treated as real price drops
        let inst = InstrumentId::new("GAPPY");
        let as_of = date(2025, 6, 1);
        let start = date(2025, 1, 1);
        let mut md = InMemoryMarketDataService::new();

        // 152 entries: mostly 100.0 with a few zeros sprinkled in (still >80% valid)
        let mut prices = vec![100.0; 152];
        // Set ~10% to zero (15 entries) → 90% completeness, should pass
        for i in (10..152).step_by(10) {
            prices[i] = 0.0;
        }
        add_prices(&mut md, &inst, start, &prices);

        let result = calculate_volatility(&inst, as_of, 365, &md).unwrap();

        // All valid prices are 100.0, so returns should all be zero
        assert_eq!(result.annualized_volatility, 0.0);
        assert_eq!(result.daily_volatility, 0.0);
    }

    #[test]
    fn no_prices_in_range_fails() {
        let inst = InstrumentId::new("EMPTY");
        let md = InMemoryMarketDataService::new();

        let result = calculate_volatility(&inst, date(2025, 6, 1), 365, &md);
        assert!(result.is_err());
    }
}
