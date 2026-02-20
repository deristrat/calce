use chrono::NaiveDate;
use rust_decimal_macros::dec;

use calce::auth::{Role, SecurityContext};
use calce::calc::engine::CalcEngine;
use calce::calc::market_value::value_positions;
use calce::context::CalculationContext;
use calce::domain::currency::Currency;
use calce::domain::fx_rate::FxRate;
use calce::domain::instrument::InstrumentId;
use calce::calc::aggregation::aggregate_positions;
use calce::domain::price::Price;
use calce::domain::quantity::Quantity;
use calce::domain::trade::Trade;
use calce::domain::user::UserId;
use calce::error::CalceError;
use calce::services::market_data::InMemoryMarketDataService;
use calce::services::user_data::InMemoryUserDataService;

fn setup_multi_currency_scenario() -> (
    InMemoryMarketDataService,
    InMemoryUserDataService,
    UserId,
    NaiveDate,
) {
    let alice = UserId::new("alice");
    let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
    let usd = Currency::new("USD");
    let eur = Currency::new("EUR");
    let sek = Currency::new("SEK");
    let aapl = InstrumentId::new("AAPL");
    let vow3 = InstrumentId::new("VOW3");

    let mut market_data = InMemoryMarketDataService::new();
    market_data.add_price(&aapl, date, Price::new(dec!(150)));
    market_data.add_price(&vow3, date, Price::new(dec!(120)));
    market_data.add_fx_rate(FxRate::new(usd, sek, dec!(10.5)), date);
    market_data.add_fx_rate(FxRate::new(eur, sek, dec!(11.4)), date);

    let mut user_data = InMemoryUserDataService::new();

    // Alice buys 100 AAPL, sells 20 → net 80
    user_data.add_trade(Trade {
        user_id: alice.clone(),
        instrument_id: aapl.clone(),
        quantity: Quantity::new(dec!(100)),
        price: Price::new(dec!(145)),
        currency: usd,
        date,
    });
    user_data.add_trade(Trade {
        user_id: alice.clone(),
        instrument_id: aapl,
        quantity: Quantity::new(dec!(-20)),
        price: Price::new(dec!(155)),
        currency: usd,
        date,
    });

    // Alice buys 50 VOW3
    user_data.add_trade(Trade {
        user_id: alice.clone(),
        instrument_id: vow3,
        quantity: Quantity::new(dec!(50)),
        price: Price::new(dec!(115)),
        currency: eur,
        date,
    });

    (market_data, user_data, alice, date)
}

// ---------------------------------------------------------------------------
// End-to-end tests via CalcEngine
// ---------------------------------------------------------------------------

#[test]
fn engine_multi_currency_portfolio() {
    let (market_data, user_data, alice, date) = setup_multi_currency_scenario();
    let sek = Currency::new("SEK");

    let ctx = CalculationContext::new(sek, date);
    let security_ctx = SecurityContext::new(alice.clone(), Role::User);
    let engine = CalcEngine::new(&ctx, &security_ctx, &market_data, &user_data);

    let result = engine
        .market_value_for_user(&alice)
        .expect("calculation should succeed");

    // AAPL: 80 * 150 = 12,000 USD → 12,000 * 10.5 = 126,000 SEK
    // VOW3: 50 * 120 = 6,000 EUR → 6,000 * 11.4 = 68,400 SEK
    // Total: 126,000 + 68,400 = 194,400 SEK
    assert_eq!(result.positions.len(), 2);
    assert_eq!(result.total.amount, dec!(194400.0));
    assert_eq!(result.total.currency, sek);

    let aapl_pos = &result.positions[0];
    assert_eq!(aapl_pos.instrument_id.as_str(), "AAPL");
    assert_eq!(aapl_pos.quantity.value(), dec!(80));
    assert_eq!(aapl_pos.market_value.amount, dec!(12000));
    assert_eq!(aapl_pos.market_value_base.amount, dec!(126000.0));

    let vow3_pos = &result.positions[1];
    assert_eq!(vow3_pos.instrument_id.as_str(), "VOW3");
    assert_eq!(vow3_pos.quantity.value(), dec!(50));
    assert_eq!(vow3_pos.market_value.amount, dec!(6000));
    assert_eq!(vow3_pos.market_value_base.amount, dec!(68400.0));
}

#[test]
fn engine_unauthorized_access_rejected() {
    let (market_data, user_data, alice, date) = setup_multi_currency_scenario();
    let sek = Currency::new("SEK");
    let bob = UserId::new("bob");

    let ctx = CalculationContext::new(sek, date);
    let security_ctx = SecurityContext::new(bob.clone(), Role::User);
    let engine = CalcEngine::new(&ctx, &security_ctx, &market_data, &user_data);

    let result = engine.market_value_for_user(&alice);

    match result.unwrap_err() {
        CalceError::Unauthorized { requester, target } => {
            assert_eq!(requester.as_str(), "bob");
            assert_eq!(target.as_str(), "alice");
        }
        other => panic!("Expected Unauthorized, got: {other:?}"),
    }
}

#[test]
fn engine_admin_can_access_any_user() {
    let (market_data, user_data, alice, date) = setup_multi_currency_scenario();
    let sek = Currency::new("SEK");

    let ctx = CalculationContext::new(sek, date);
    let security_ctx = SecurityContext::system();
    let engine = CalcEngine::new(&ctx, &security_ctx, &market_data, &user_data);

    let result = engine.market_value_for_user(&alice);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().positions.len(), 2);
}

#[test]
fn engine_retroactive_calculation() {
    let alice = UserId::new("alice");
    let usd = Currency::new("USD");
    let aapl = InstrumentId::new("AAPL");

    let early = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap();
    let late = NaiveDate::from_ymd_opt(2025, 1, 20).unwrap();

    let mut market_data = InMemoryMarketDataService::new();
    market_data.add_price(&aapl, early, Price::new(dec!(140)));

    let mut user_data = InMemoryUserDataService::new();
    user_data.add_trade(Trade {
        user_id: alice.clone(),
        instrument_id: aapl.clone(),
        quantity: Quantity::new(dec!(50)),
        price: Price::new(dec!(135)),
        currency: usd,
        date: early,
    });
    user_data.add_trade(Trade {
        user_id: alice.clone(),
        instrument_id: aapl,
        quantity: Quantity::new(dec!(30)),
        price: Price::new(dec!(145)),
        currency: usd,
        date: late,
    });

    let ctx = CalculationContext::new(usd, early);
    let security_ctx = SecurityContext::new(alice.clone(), Role::User);
    let engine = CalcEngine::new(&ctx, &security_ctx, &market_data, &user_data);

    let result = engine
        .market_value_for_user(&alice)
        .expect("calculation should succeed");

    assert_eq!(result.positions.len(), 1);
    assert_eq!(result.positions[0].quantity.value(), dec!(50));
    // 50 * 140 = 7,000 USD (same currency, no FX)
    assert_eq!(result.total.amount, dec!(7000));
}

// ---------------------------------------------------------------------------
// Calculation function tests — no auth, no user data, just positions + market data
// ---------------------------------------------------------------------------

#[test]
fn value_positions_multi_currency() {
    let usd = Currency::new("USD");
    let eur = Currency::new("EUR");
    let sek = Currency::new("SEK");
    let aapl = InstrumentId::new("AAPL");
    let vow3 = InstrumentId::new("VOW3");
    let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();

    let mut market_data = InMemoryMarketDataService::new();
    market_data.add_price(&aapl, date, Price::new(dec!(150)));
    market_data.add_price(&vow3, date, Price::new(dec!(120)));
    market_data.add_fx_rate(FxRate::new(usd, sek, dec!(10.5)), date);
    market_data.add_fx_rate(FxRate::new(eur, sek, dec!(11.4)), date);

    let positions = vec![
        calce::domain::position::Position {
            instrument_id: aapl,
            quantity: Quantity::new(dec!(80)),
            currency: usd,
        },
        calce::domain::position::Position {
            instrument_id: vow3,
            quantity: Quantity::new(dec!(50)),
            currency: eur,
        },
    ];
    let ctx = CalculationContext::new(sek, date);

    let result = value_positions(&positions, &ctx, &market_data).unwrap();

    assert_eq!(result.total.amount, dec!(194400.0));
    assert_eq!(result.total.currency, sek);
}

#[test]
fn aggregate_then_value() {
    let usd = Currency::new("USD");
    let aapl = InstrumentId::new("AAPL");
    let alice = UserId::new("alice");
    let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();

    let trades = vec![
        Trade {
            user_id: alice.clone(),
            instrument_id: aapl.clone(),
            quantity: Quantity::new(dec!(100)),
            price: Price::new(dec!(145)),
            currency: usd,
            date,
        },
        Trade {
            user_id: alice,
            instrument_id: aapl.clone(),
            quantity: Quantity::new(dec!(-40)),
            price: Price::new(dec!(155)),
            currency: usd,
            date,
        },
    ];

    // Step 1: aggregate trades into positions
    let positions = aggregate_positions(&trades, date);
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].quantity.value(), dec!(60));

    // Step 2: value positions
    let mut market_data = InMemoryMarketDataService::new();
    market_data.add_price(&aapl, date, Price::new(dec!(150)));
    let ctx = CalculationContext::new(usd, date);

    let result = value_positions(&positions, &ctx, &market_data).unwrap();
    assert_eq!(result.total.amount, dec!(9000)); // 60 * 150
}
