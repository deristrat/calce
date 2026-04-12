use chrono::NaiveDate;
use pyo3::prelude::*;

use calce_core::calc::aggregation;
use calce_core::calc::market_value;
use calce_core::calc::volatility;
use calce_core::context::CalculationContext;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::user::UserId;
use calce_core::reports::portfolio;

use crate::domain::Currency;
use crate::errors::{NoTradesFoundError, calce_err_to_py};
use crate::results::{MarketValueResult, PortfolioReport, VolatilityResult};
use crate::services::{MarketData, UserData};

#[pyclass]
pub struct CalcEngine {
    ctx: CalculationContext,
    user_id: UserId,
    market_data: Py<MarketData>,
    user_data: Py<UserData>,
}

impl CalcEngine {
    pub(crate) fn create(
        py: Python<'_>,
        base_currency: &Currency,
        as_of_date: NaiveDate,
        user_id: &str,
        market_data: Py<MarketData>,
        user_data: Py<UserData>,
    ) -> Self {
        market_data.borrow_mut(py).ensure_ready();
        CalcEngine {
            ctx: CalculationContext::new(base_currency.inner, as_of_date),
            user_id: UserId::new(user_id),
            market_data,
            user_data,
        }
    }
}

#[pymethods]
impl CalcEngine {
    #[new]
    #[pyo3(signature = (base_currency, as_of_date, user_id, market_data, user_data))]
    fn new(
        py: Python<'_>,
        base_currency: &Currency,
        as_of_date: NaiveDate,
        user_id: &str,
        market_data: Py<MarketData>,
        user_data: Py<UserData>,
    ) -> Self {
        Self::create(
            py,
            base_currency,
            as_of_date,
            user_id,
            market_data,
            user_data,
        )
    }

    fn market_value(&self, py: Python<'_>) -> PyResult<MarketValueResult> {
        let md = self.market_data.borrow(py);
        let ud = self.user_data.borrow(py);
        let trades = ud.inner.trades_for(&self.user_id).ok_or_else(|| {
            NoTradesFoundError::new_err(format!("No trades found for user {}", self.user_id))
        })?;
        let positions = aggregation::aggregate_positions(trades, self.ctx.as_of_date)
            .map_err(calce_err_to_py)?;
        market_value::value_positions(&positions, &self.ctx, md.as_service())
            .map(|outcome| MarketValueResult {
                warnings: outcome.warnings,
                inner: outcome.value,
            })
            .map_err(calce_err_to_py)
    }

    fn portfolio_report(&self, py: Python<'_>) -> PyResult<PortfolioReport> {
        let md = self.market_data.borrow(py);
        let ud = self.user_data.borrow(py);
        let trades = ud.inner.trades_for(&self.user_id).ok_or_else(|| {
            NoTradesFoundError::new_err(format!("No trades found for user {}", self.user_id))
        })?;
        portfolio::portfolio_report(trades, &self.ctx, md.as_service())
            .map(|outcome| PortfolioReport {
                warnings: outcome.warnings,
                inner: outcome.value,
            })
            .map_err(calce_err_to_py)
    }

    /// Compute historical realized volatility for an instrument.
    ///
    /// Args:
    ///     `instrument_id`: Instrument ticker/identifier.
    ///     `lookback_days`: Calendar days of history (default 1095 = 3 years).
    #[pyo3(signature = (instrument_id, lookback_days = 1095))]
    fn volatility(
        &self,
        py: Python<'_>,
        instrument_id: &str,
        lookback_days: u32,
    ) -> PyResult<VolatilityResult> {
        let md = self.market_data.borrow(py);
        let instrument = InstrumentId::new(instrument_id);
        volatility::calculate_volatility(
            &instrument,
            self.ctx.as_of_date,
            lookback_days,
            md.as_service(),
        )
        .map(|r| VolatilityResult { inner: r })
        .map_err(calce_err_to_py)
    }
}
