use chrono::NaiveDate;

use super::currency::Currency;
use super::instrument::InstrumentId;
use super::price::Price;
use super::quantity::Quantity;
use super::user::UserId;

/// A single trade execution.
///
/// Quantity is signed: positive = buy, negative = sell.
#[derive(Clone, Debug)]
pub struct Trade {
    /// The user who executed this trade.
    pub user_id: UserId,
    /// The instrument traded.
    pub instrument_id: InstrumentId,
    /// Signed quantity: positive = buy, negative = sell.
    pub quantity: Quantity,
    /// Execution price per unit.
    pub price: Price,
    /// Currency of the trade.
    pub currency: Currency,
    /// Trade date.
    pub date: NaiveDate,
}
