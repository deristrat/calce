use calce_core::domain::account::AccountId;
use calce_core::domain::currency::Currency;
use calce_core::domain::fx_rate::FxRate;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::price::Price;
use calce_core::domain::quantity::Quantity;
use calce_core::domain::trade::Trade;
use calce_core::domain::user::UserId;
use calce_core::services::market_data::InMemoryMarketDataService;
use calce_core::services::user_data::InMemoryUserDataService;
use chrono::{Datelike, NaiveDate};

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid seed date")
}

pub(crate) fn seed_market_data() -> InMemoryMarketDataService {
    let mut svc = InMemoryMarketDataService::new();

    let usd = Currency::new("USD");
    let eur = Currency::new("EUR");
    let sek = Currency::new("SEK");
    let aapl = InstrumentId::new("AAPL");
    let vow3 = InstrumentId::new("VOW3");

    let today = date(2025, 3, 14); // Friday
    let year_ago = date(2024, 3, 1); // start a bit before year-ago comparison dates

    // Daily price history (weekdays only) from year_ago to today.
    // Uses a deterministic sine-wave wobble over a linear trend.
    add_daily_prices(&mut svc, &aapl, year_ago, today, 160.0, 200.0, 0.02);
    add_daily_prices(&mut svc, &vow3, year_ago, today, 100.0, 120.0, 0.03);

    // Constant FX rates for every weekday in the period
    add_daily_fx_rates(&mut svc, year_ago, today, usd, sek, 10.5);
    add_daily_fx_rates(&mut svc, year_ago, today, eur, sek, 11.2);

    svc
}

/// Generate daily prices (Mon-Fri) along a linear trend with sine-wave noise.
///
/// Produces a deterministic, reproducible price series useful for seed data
/// and sanity-checking calculations.
fn add_daily_prices(
    svc: &mut InMemoryMarketDataService,
    instrument: &InstrumentId,
    from: NaiveDate,
    to: NaiveDate,
    start_price: f64,
    end_price: f64,
    noise_amplitude: f64,
) {
    let total_days = (to - from).num_days().max(1) as f64;
    let slope = (end_price - start_price) / total_days;
    let mut d = from;
    while d <= to {
        // Skip weekends
        if d.weekday().number_from_monday() <= 5 {
            let days_elapsed = (d - from).num_days() as f64;
            let trend = start_price + slope * days_elapsed;
            let noise = (days_elapsed * 0.3).sin() * trend * noise_amplitude;
            svc.add_price(instrument, d, Price::new(trend + noise));
        }
        d = d + chrono::Days::new(1);
    }
}

fn add_daily_fx_rates(
    svc: &mut InMemoryMarketDataService,
    from: NaiveDate,
    to: NaiveDate,
    base: Currency,
    quote: Currency,
    rate: f64,
) {
    let mut d = from;
    while d <= to {
        if d.weekday().number_from_monday() <= 5 {
            svc.add_fx_rate(FxRate::new(base, quote, rate), d);
        }
        d = d + chrono::Days::new(1);
    }
}

pub(crate) fn seed_user_data() -> InMemoryUserDataService {
    let mut svc = InMemoryUserDataService::new();

    let alice = UserId::new("alice");
    let acct_usd = AccountId::new("alice-usd");
    let acct_eur = AccountId::new("alice-eur");
    let usd = Currency::new("USD");
    let eur = Currency::new("EUR");

    svc.add_trade(Trade {
        user_id: alice.clone(),
        account_id: acct_usd,
        instrument_id: InstrumentId::new("AAPL"),
        quantity: Quantity::new(80.0),
        price: Price::new(150.0),
        currency: usd,
        date: date(2024, 1, 10),
    });

    svc.add_trade(Trade {
        user_id: alice,
        account_id: acct_eur,
        instrument_id: InstrumentId::new("VOW3"),
        quantity: Quantity::new(50.0),
        price: Price::new(95.0),
        currency: eur,
        date: date(2024, 2, 15),
    });

    svc
}

#[cfg(test)]
mod tests {
    use super::*;
    use calce_core::calc::volatility::calculate_volatility;

    #[test]
    fn volatility_sanity_check_with_seed_data() {
        let md = seed_market_data();
        let today = date(2025, 3, 14);
        let aapl = InstrumentId::new("AAPL");
        let vow3 = InstrumentId::new("VOW3");

        let aapl_vol = calculate_volatility(&aapl, today, 365, &md).unwrap();
        let vow3_vol = calculate_volatility(&vow3, today, 365, &md).unwrap();

        // Seed data uses 2% noise for AAPL, 3% for VOW3 — VOW3 should be higher
        println!("AAPL: {:.1}% annualized ({} obs, {} to {})",
            aapl_vol.annualized_volatility * 100.0,
            aapl_vol.num_observations,
            aapl_vol.start_date, aapl_vol.end_date);
        println!("VOW3: {:.1}% annualized ({} obs, {} to {})",
            vow3_vol.annualized_volatility * 100.0,
            vow3_vol.num_observations,
            vow3_vol.start_date, vow3_vol.end_date);

        // Sanity: annualized vol should be in a plausible range (1%-100%)
        assert!(aapl_vol.annualized_volatility > 0.01);
        assert!(aapl_vol.annualized_volatility < 1.0);
        assert!(vow3_vol.annualized_volatility > 0.01);
        assert!(vow3_vol.annualized_volatility < 1.0);

        // Higher noise → higher vol
        assert!(vow3_vol.annualized_volatility > aapl_vol.annualized_volatility);
    }
}
