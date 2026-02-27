use crate::auth::SecurityContext;
use crate::calc::aggregation;
use crate::calc::market_value::{self, MarketValueResult};
use crate::context::CalculationContext;
use crate::domain::user::UserId;
use crate::error::CalceResult;
use crate::reports::portfolio::{self, PortfolioReport};
use crate::services::market_data::MarketDataService;
use crate::services::user_data::UserDataService;

/// Orchestration layer wiring services to pure calculation functions.
pub struct CalcEngine<'a> {
    pub ctx: &'a CalculationContext,
    pub security_ctx: &'a SecurityContext,
    pub market_data: &'a dyn MarketDataService,
    pub user_data: &'a dyn UserDataService,
}

impl<'a> CalcEngine<'a> {
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

    /// # Errors
    ///
    /// Returns `Unauthorized` if the security context lacks access.
    /// Returns `NoTradesFound` if the user has no trades.
    /// Propagates price/FX lookup errors from market data.
    pub fn market_value_for_user(
        &self,
        user_id: &UserId,
    ) -> CalceResult<MarketValueResult> {
        let trades = self.user_data.get_trades(self.security_ctx, user_id)?;
        let positions = aggregation::aggregate_positions(&trades, self.ctx.as_of_date);
        market_value::value_positions(&positions, self.ctx, self.market_data)
    }

    /// # Errors
    ///
    /// Returns `Unauthorized` if the security context lacks access.
    /// Returns `NoTradesFound` if the user has no trades.
    /// Propagates price/FX lookup errors from market data.
    pub fn portfolio_report_for_user(
        &self,
        user_id: &UserId,
    ) -> CalceResult<PortfolioReport> {
        let trades = self.user_data.get_trades(self.security_ctx, user_id)?;
        portfolio::portfolio_report(&trades, self.ctx, self.market_data)
    }
}
