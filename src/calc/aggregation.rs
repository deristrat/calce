use std::collections::HashMap;

use chrono::NaiveDate;

use crate::domain::instrument::InstrumentId;
use crate::domain::position::Position;
use crate::domain::quantity::Quantity;
use crate::domain::trade::Trade;

/// Aggregate a list of trades into net positions per instrument.
///
/// Only includes trades on or before `as_of_date`. Positions with
/// zero net quantity (fully closed) are excluded.
#[must_use]
pub fn aggregate_positions(trades: &[Trade], as_of_date: NaiveDate) -> Vec<Position> {
    let mut net: HashMap<InstrumentId, (Quantity, crate::domain::currency::Currency)> =
        HashMap::new();

    for trade in trades {
        if trade.date <= as_of_date {
            net.entry(trade.instrument_id.clone())
                .and_modify(|(qty, _)| *qty = *qty + trade.quantity)
                .or_insert((trade.quantity, trade.currency));
        }
    }

    let mut positions: Vec<Position> = net
        .into_iter()
        .filter(|(_, (qty, _))| !qty.value().is_zero())
        .map(|(id, (qty, ccy))| Position {
            instrument_id: id,
            quantity: qty,
            currency: ccy,
        })
        .collect();

    // Sort for deterministic output
    positions.sort_by(|a, b| a.instrument_id.as_str().cmp(b.instrument_id.as_str()));
    positions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::currency::Currency;
    use crate::domain::price::Price;
    use crate::domain::user::UserId;
    use rust_decimal_macros::dec;

    #[test]
    fn aggregates_buys_and_sells() {
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).expect("valid test date");
        let usd = Currency::new("USD");
        let aapl = InstrumentId::new("AAPL");
        let alice = UserId::new("alice");

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
                instrument_id: aapl,
                quantity: Quantity::new(dec!(-30)),
                price: Price::new(dec!(150)),
                currency: usd,
                date,
            },
        ];

        let positions = aggregate_positions(&trades, date);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].quantity.value(), dec!(70));
    }

    #[test]
    fn fully_closed_position_excluded() {
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).expect("valid test date");
        let usd = Currency::new("USD");
        let aapl = InstrumentId::new("AAPL");
        let alice = UserId::new("alice");

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
                instrument_id: aapl,
                quantity: Quantity::new(dec!(-100)),
                price: Price::new(dec!(150)),
                currency: usd,
                date,
            },
        ];

        let positions = aggregate_positions(&trades, date);
        assert!(positions.is_empty());
    }

    #[test]
    fn filters_by_as_of_date() {
        let early = NaiveDate::from_ymd_opt(2025, 1, 10).expect("valid test date");
        let late = NaiveDate::from_ymd_opt(2025, 1, 20).expect("valid test date");
        let usd = Currency::new("USD");
        let aapl = InstrumentId::new("AAPL");
        let alice = UserId::new("alice");

        let trades = vec![
            Trade {
                user_id: alice.clone(),
                instrument_id: aapl.clone(),
                quantity: Quantity::new(dec!(50)),
                price: Price::new(dec!(140)),
                currency: usd,
                date: early,
            },
            Trade {
                user_id: alice,
                instrument_id: aapl,
                quantity: Quantity::new(dec!(30)),
                price: Price::new(dec!(150)),
                currency: usd,
                date: late,
            },
        ];

        let positions = aggregate_positions(&trades, early);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].quantity.value(), dec!(50));
    }
}
