use super::currency::Currency;
use super::instrument::InstrumentId;
use super::quantity::Quantity;

/// An aggregated net position in a single instrument.
/// Pure domain type — no market values or pricing attached.
#[derive(Clone, Debug)]
pub struct Position {
    /// The instrument held.
    pub instrument_id: InstrumentId,
    /// Net quantity (positive = long, negative = short).
    pub quantity: Quantity,
    /// Currency of the position.
    pub currency: Currency,
}
