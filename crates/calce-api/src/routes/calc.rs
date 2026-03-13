use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use calce_core::calc::aggregation;
use calce_core::calc::market_value::{self, MarketValueResult};
use calce_core::calc::volatility::{self, VolatilityResult};
use calce_core::context::CalculationContext;
use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::user::UserId;
use calce_core::reports::portfolio::PortfolioReport;
use calce_data::loader::{CalcInputSpec, DataStats, DateRange, InstrumentSummary, UserSummary};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::auth::Auth;
use crate::error::ApiError;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/users/{user_id}/market-value", get(market_value))
        .route("/v1/users/{user_id}/portfolio", get(portfolio_report))
        .route(
            "/v1/instruments/{instrument_id}/volatility",
            get(volatility),
        )
        .route("/v1/data/stats", get(data_stats))
        .route("/v1/data/users", get(data_users))
        .route("/v1/data/instruments", get(data_instruments))
        .route(
            "/v1/data/instruments/{instrument_id}/prices",
            get(instrument_prices),
        )
}

#[derive(Deserialize)]
struct CalcParams {
    as_of_date: NaiveDate,
    base_currency: String,
}

#[derive(Deserialize)]
struct VolatilityParams {
    as_of_date: NaiveDate,
    #[serde(default = "default_lookback")]
    lookback_days: u32,
}

fn default_lookback() -> u32 {
    1095 // 3 years
}

fn parse_currency(s: &str) -> Result<Currency, ApiError> {
    Currency::try_new(s).map_err(|_| ApiError::BadRequest(format!("Invalid currency code: {s}")))
}

async fn market_value(
    State(state): State<AppState>,
    Auth(security_ctx): Auth,
    Path(user_id): Path<String>,
    Query(params): Query<CalcParams>,
) -> Result<Json<MarketValueResult>, ApiError> {
    let base_currency = parse_currency(&params.base_currency)?;
    let ctx = CalculationContext::new(base_currency, params.as_of_date);
    let user_id = UserId::new(user_id);

    let spec = CalcInputSpec {
        subjects: vec![user_id],
        base_currency,
        date_range: DateRange {
            from: params.as_of_date,
            to: params.as_of_date,
        },
    };
    let inputs = state.loader.load_calc_inputs(&security_ctx, &spec).await?;

    let positions = aggregation::aggregate_positions(&inputs.trades, ctx.as_of_date)?;
    let outcome = market_value::value_positions(&positions, &ctx, &*inputs.market_data)?;
    // TODO: surface outcome.warnings in response headers or a wrapper
    Ok(Json(outcome.value))
}

async fn portfolio_report(
    State(state): State<AppState>,
    Auth(security_ctx): Auth,
    Path(user_id): Path<String>,
    Query(params): Query<CalcParams>,
) -> Result<Json<PortfolioReport>, ApiError> {
    let base_currency = parse_currency(&params.base_currency)?;
    let ctx = CalculationContext::new(base_currency, params.as_of_date);
    let user_id = UserId::new(user_id);

    // Portfolio report needs price history going back ~400 days for value changes
    let spec = CalcInputSpec {
        subjects: vec![user_id],
        base_currency,
        date_range: DateRange {
            from: params.as_of_date - chrono::Days::new(400),
            to: params.as_of_date,
        },
    };
    let inputs = state.loader.load_calc_inputs(&security_ctx, &spec).await?;

    let outcome = calce_core::reports::portfolio::portfolio_report(
        &inputs.trades,
        &ctx,
        &*inputs.market_data,
    )?;
    // TODO: surface outcome.warnings in response headers or a wrapper
    Ok(Json(outcome.value))
}

async fn volatility(
    State(state): State<AppState>,
    Auth(_security_ctx): Auth,
    Path(instrument_id): Path<String>,
    Query(params): Query<VolatilityParams>,
) -> Result<Json<VolatilityResult>, ApiError> {
    let instrument = InstrumentId::new(instrument_id);
    let from = params.as_of_date - chrono::Days::new(u64::from(params.lookback_days));

    let date_range = DateRange {
        from,
        to: params.as_of_date,
    };
    let result = state
        .loader
        .with_market_data(std::slice::from_ref(&instrument), &date_range, |md| {
            volatility::calculate_volatility(
                &instrument,
                params.as_of_date,
                params.lookback_days,
                md,
            )
        })
        .await?;
    Ok(Json(result))
}

// ── Data exploration (no auth required — developer tool) ──────────────

async fn data_stats(
    Auth(_ctx): Auth,
    State(state): State<AppState>,
) -> Result<Json<DataStats>, ApiError> {
    let stats = state.loader.data_stats().await?;
    Ok(Json(stats))
}

async fn data_users(
    Auth(ctx): Auth,
    State(state): State<AppState>,
) -> Result<Json<Vec<UserSummary>>, ApiError> {
    let users = state.loader.list_users(&ctx).await?;
    Ok(Json(users))
}

async fn data_instruments(
    Auth(_ctx): Auth,
    State(state): State<AppState>,
) -> Result<Json<Vec<InstrumentSummary>>, ApiError> {
    let instruments = state.loader.list_instruments().await?;
    Ok(Json(instruments))
}

#[derive(Deserialize)]
struct PriceHistoryParams {
    from: NaiveDate,
    to: NaiveDate,
}

#[derive(Serialize)]
struct PricePoint {
    date: NaiveDate,
    price: f64,
}

async fn instrument_prices(
    Auth(_ctx): Auth,
    State(state): State<AppState>,
    Path(instrument_id): Path<String>,
    Query(params): Query<PriceHistoryParams>,
) -> Result<Json<Vec<PricePoint>>, ApiError> {
    let instrument = InstrumentId::new(instrument_id);
    let history = state
        .loader
        .price_history(&instrument, params.from, params.to)
        .await?;
    let points: Vec<PricePoint> = history
        .into_iter()
        .map(|(date, price)| PricePoint { date, price })
        .collect();
    Ok(Json(points))
}

pub async fn explorer() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../../../../tools/api-explorer.html"))
}
