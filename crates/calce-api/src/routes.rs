use axum::Json;
use axum::extract::{Path, Query, State};
use calce_core::calc::market_value::MarketValueResult;
use calce_core::calc::volatility::VolatilityResult;
use calce_core::context::CalculationContext;
use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::user::UserId;
use calce_core::engine::CalcEngine;
use calce_core::reports::portfolio::PortfolioReport;
use chrono::NaiveDate;
use serde::Deserialize;

use crate::auth::Auth;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CalcParams {
    pub as_of_date: NaiveDate,
    pub base_currency: String,
}

#[derive(Deserialize)]
pub struct VolatilityParams {
    pub as_of_date: NaiveDate,
    pub base_currency: String,
    #[serde(default = "default_lookback")]
    pub lookback_days: u32,
}

fn default_lookback() -> u32 {
    1095 // 3 years
}

fn parse_currency(s: &str) -> Result<Currency, ApiError> {
    Currency::try_new(s).map_err(|_| ApiError::BadRequest(format!("Invalid currency code: {s}")))
}

pub async fn market_value(
    State(state): State<AppState>,
    Auth(security_ctx): Auth,
    Path(user_id): Path<String>,
    Query(params): Query<CalcParams>,
) -> Result<Json<MarketValueResult>, ApiError> {
    let base_currency = parse_currency(&params.base_currency)?;
    let ctx = CalculationContext::new(base_currency, params.as_of_date);
    let user_id = UserId::new(user_id);

    let engine = CalcEngine::new(
        &ctx,
        &security_ctx,
        state.market_data.as_ref(),
        state.user_data.as_ref(),
    );

    let result = engine.market_value_for_user(&user_id)?;
    Ok(Json(result))
}

pub async fn portfolio_report(
    State(state): State<AppState>,
    Auth(security_ctx): Auth,
    Path(user_id): Path<String>,
    Query(params): Query<CalcParams>,
) -> Result<Json<PortfolioReport>, ApiError> {
    let base_currency = parse_currency(&params.base_currency)?;
    let ctx = CalculationContext::new(base_currency, params.as_of_date);
    let user_id = UserId::new(user_id);

    let engine = CalcEngine::new(
        &ctx,
        &security_ctx,
        state.market_data.as_ref(),
        state.user_data.as_ref(),
    );

    let result = engine.portfolio_report_for_user(&user_id)?;
    Ok(Json(result))
}

pub async fn volatility(
    State(state): State<AppState>,
    Auth(security_ctx): Auth,
    Path(instrument_id): Path<String>,
    Query(params): Query<VolatilityParams>,
) -> Result<Json<VolatilityResult>, ApiError> {
    let base_currency = parse_currency(&params.base_currency)?;
    let ctx = CalculationContext::new(base_currency, params.as_of_date);
    let instrument = InstrumentId::new(instrument_id);

    // Volatility is instrument-scoped — engine is only used for consistency
    let engine = CalcEngine::new(
        &ctx,
        &security_ctx,
        state.market_data.as_ref(),
        state.user_data.as_ref(),
    );

    let result = engine.volatility(&instrument, params.lookback_days)?;
    Ok(Json(result))
}
