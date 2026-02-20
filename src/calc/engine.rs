use crate::auth::SecurityContext;
use crate::context::CalculationContext;
use crate::domain::user::UserId;
use crate::error::CalceResult;
use crate::services::market_data::MarketDataService;
use crate::services::user_data::UserDataService;

use super::aggregation;
use super::market_value::{self, MarketValueResult};

/// Orchestration layer that wires services to calculation functions.
///
/// Holds references to the calculation context, security context, and data
/// services. Each method handles data loading (with authorization), then
/// delegates to the corresponding calculation function.
pub struct CalcEngine<'a> {
    /// The calculation parameters (base currency, as-of date).
    pub ctx: &'a CalculationContext,
    /// The authenticated caller's security context.
    pub security_ctx: &'a SecurityContext,
    /// Market data provider (prices, FX rates).
    pub market_data: &'a dyn MarketDataService,
    /// User data provider (trades).
    pub user_data: &'a dyn UserDataService,
}

impl<'a> CalcEngine<'a> {
    /// Create a new calculation engine.
    #[must_use]
    pub fn new(
        ctx: &'a CalculationContext,
        security_ctx: &'a SecurityContext,
        market_data: &'a dyn MarketDataService,
        user_data: &'a dyn UserDataService,
    ) -> Self {
        CalcEngine {
            ctx,
            security_ctx,
            market_data,
            user_data,
        }
    }

    /// Calculate market value for a user's portfolio.
    ///
    /// Fetches trades (with authorization), aggregates into positions,
    /// then delegates to `value_positions` for pricing and conversion.
    ///
    /// # Errors
    ///
    /// Returns `Unauthorized` if the security context lacks access.
    /// Returns `NoTradesFound` if the user has no trades.
    /// Propagates price/FX lookup errors from the market data service.
    pub fn market_value_for_user(
        &self,
        user_id: &UserId,
    ) -> CalceResult<MarketValueResult> {
        let trades = self.user_data.get_trades(self.security_ctx, user_id)?;
        let positions = aggregation::aggregate_positions(&trades, self.ctx.as_of_date);
        market_value::value_positions(&positions, self.ctx, self.market_data)
    }
}
