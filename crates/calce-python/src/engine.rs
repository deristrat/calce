use chrono::NaiveDate;
use pyo3::prelude::*;

use calce_core::auth::{Role, SecurityContext};
use calce_core::context::CalculationContext;
use calce_core::domain::user::UserId;

use crate::domain::Currency;
use crate::errors::calce_err_to_py;
use calce_core::domain::instrument::InstrumentId;

use crate::results::{MarketValueResult, PortfolioReport, VolatilityResult};
use crate::services::{MarketData, UserData};

#[pyclass]
pub struct CalcEngine {
    ctx: CalculationContext,
    security_ctx: SecurityContext,
    user_id: UserId,
    market_data: Py<MarketData>,
    user_data: Py<UserData>,
}

#[pymethods]
impl CalcEngine {
    #[new]
    #[pyo3(signature = (base_currency, as_of_date, user_id, market_data, user_data, role = "user"))]
    fn new(
        base_currency: &Currency,
        as_of_date: NaiveDate,
        user_id: &str,
        market_data: Py<MarketData>,
        user_data: Py<UserData>,
        role: &str,
    ) -> PyResult<Self> {
        let role = match role {
            "admin" => Role::Admin,
            "user" => Role::User,
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "role must be \"user\" or \"admin\", got \"{other}\""
                )))
            }
        };
        let uid = UserId::new(user_id);
        Ok(CalcEngine {
            ctx: CalculationContext::new(base_currency.inner, as_of_date),
            security_ctx: SecurityContext::new(uid.clone(), role),
            user_id: uid,
            market_data,
            user_data,
        })
    }

    fn market_value(&self, py: Python<'_>) -> PyResult<MarketValueResult> {
        let md = self.market_data.borrow(py);
        let ud = self.user_data.borrow(py);
        let engine = calce_core::engine::CalcEngine::new(
            &self.ctx,
            &self.security_ctx,
            &md.inner,
            &ud.inner,
        );
        engine
            .market_value_for_user(&self.user_id)
            .map(|r| MarketValueResult { inner: r })
            .map_err(calce_err_to_py)
    }

    fn portfolio_report(&self, py: Python<'_>) -> PyResult<PortfolioReport> {
        let md = self.market_data.borrow(py);
        let ud = self.user_data.borrow(py);
        let engine = calce_core::engine::CalcEngine::new(
            &self.ctx,
            &self.security_ctx,
            &md.inner,
            &ud.inner,
        );
        engine
            .portfolio_report_for_user(&self.user_id)
            .map(|r| PortfolioReport { inner: r })
            .map_err(calce_err_to_py)
    }

    /// Compute historical realized volatility for an instrument.
    ///
    /// Args:
    ///     instrument_id: Instrument ticker/identifier.
    ///     lookback_days: Calendar days of history (default 1095 = 3 years).
    #[pyo3(signature = (instrument_id, lookback_days = 1095))]
    fn volatility(
        &self,
        py: Python<'_>,
        instrument_id: &str,
        lookback_days: u32,
    ) -> PyResult<VolatilityResult> {
        let md = self.market_data.borrow(py);
        let ud = self.user_data.borrow(py);
        let engine = calce_core::engine::CalcEngine::new(
            &self.ctx,
            &self.security_ctx,
            &md.inner,
            &ud.inner,
        );
        let instrument = InstrumentId::new(instrument_id);
        engine
            .volatility(&instrument, lookback_days)
            .map(|r| VolatilityResult { inner: r })
            .map_err(calce_err_to_py)
    }
}
