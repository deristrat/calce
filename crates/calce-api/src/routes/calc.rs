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
use calce_core::outcome::{Outcome, Warning, WarningCode};
use calce_core::reports::portfolio::PortfolioReport;
use calce_core::services::market_data::MarketDataService;
use calce_data::market_data_store::InstrumentSummary;
use calce_data::types::DataStats;
use calce_data::user_data_store::UserSummary;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

const DEFAULT_PAGE_SIZE: usize = 50;

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

// ── Response wrapper for calculations that produce warnings ─────────

#[derive(Serialize)]
struct ApiWarning {
    code: &'static str,
    message: String,
}

impl From<&Warning> for ApiWarning {
    fn from(w: &Warning) -> Self {
        let code = match w.code {
            WarningCode::MissingPrice => "MISSING_PRICE",
            WarningCode::MissingFxRate => "MISSING_FX_RATE",
        };
        ApiWarning {
            code,
            message: w.message.clone(),
        }
    }
}

#[derive(Serialize)]
struct CalcResponse<T: Serialize> {
    data: T,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<ApiWarning>,
}

impl<T: Serialize> CalcResponse<T> {
    fn from_outcome(outcome: Outcome<T>) -> Self {
        let warnings = outcome.warnings.iter().map(ApiWarning::from).collect();
        CalcResponse {
            data: outcome.value,
            warnings,
        }
    }
}

async fn market_value(
    State(state): State<AppState>,
    Auth(security_ctx): Auth,
    Path(user_id): Path<String>,
    Query(params): Query<CalcParams>,
) -> Result<Json<CalcResponse<MarketValueResult>>, ApiError> {
    let base_currency = parse_currency(&params.base_currency)?;
    let ctx = CalculationContext::new(base_currency, params.as_of_date);
    let user_id = UserId::new(user_id);

    let trades = state.user_data.load_trades(&security_ctx, &[user_id])?;
    let market_data = state.market_data.market_data();

    let positions = aggregation::aggregate_positions(&trades, ctx.as_of_date)?;
    let outcome = market_value::value_positions(&positions, &ctx, &*market_data)?;
    Ok(Json(CalcResponse::from_outcome(outcome)))
}

async fn portfolio_report(
    State(state): State<AppState>,
    Auth(security_ctx): Auth,
    Path(user_id): Path<String>,
    Query(params): Query<CalcParams>,
) -> Result<Json<CalcResponse<PortfolioReport>>, ApiError> {
    let base_currency = parse_currency(&params.base_currency)?;
    let ctx = CalculationContext::new(base_currency, params.as_of_date);
    let user_id = UserId::new(user_id);

    let trades = state.user_data.load_trades(&security_ctx, &[user_id])?;
    let market_data = state.market_data.market_data();

    let outcome = calce_core::reports::portfolio::portfolio_report(&trades, &ctx, &*market_data)?;
    Ok(Json(CalcResponse::from_outcome(outcome)))
}

async fn volatility(
    State(state): State<AppState>,
    Auth(_security_ctx): Auth,
    Path(instrument_id): Path<String>,
    Query(params): Query<VolatilityParams>,
) -> Result<Json<VolatilityResult>, ApiError> {
    let instrument = InstrumentId::new(instrument_id);
    let md = state.market_data.market_data();
    let result = volatility::calculate_volatility(
        &instrument,
        params.as_of_date,
        params.lookback_days,
        &*md,
    )?;
    Ok(Json(result))
}

// ── Data exploration ──────────────────────────────────────────────────

async fn data_stats(
    Auth(_ctx): Auth,
    State(state): State<AppState>,
) -> Result<Json<DataStats>, ApiError> {
    let stats = DataStats {
        user_count: state.user_data.user_count(),
        organization_count: state.user_data.organization_count(),
        instrument_count: state.market_data.instrument_count(),
        trade_count: state.user_data.trade_count(),
        price_count: state.market_data.price_count(),
        fx_rate_count: state.market_data.fx_rate_count(),
    };
    Ok(Json(stats))
}

#[derive(Deserialize)]
struct PaginationParams {
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_page_size")]
    limit: usize,
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    organization_id: Option<String>,
}

fn default_page_size() -> usize {
    DEFAULT_PAGE_SIZE
}

#[derive(Serialize)]
struct PaginatedResponse<T: Serialize> {
    items: Vec<T>,
    total: usize,
    offset: usize,
    limit: usize,
}

async fn data_users(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<UserSummary>>, ApiError> {
    let all = state.user_data.list_users(&ctx);
    let after_org: Vec<_> = if let Some(ref org) = params.organization_id {
        all.into_iter()
            .filter(|u| u.organization_id.as_deref() == Some(org.as_str()))
            .collect()
    } else {
        all
    };
    let filtered: Vec<_> = if let Some(ref q) = params.search {
        let q = q.to_lowercase();
        after_org
            .into_iter()
            .filter(|u| {
                u.id.to_lowercase().contains(&q)
                    || u.email.as_deref().unwrap_or("").to_lowercase().contains(&q)
            })
            .collect()
    } else {
        after_org
    };
    let total = filtered.len();
    let items = filtered
        .into_iter()
        .skip(params.offset)
        .take(params.limit)
        .collect();
    Ok(Json(PaginatedResponse {
        items,
        total,
        offset: params.offset,
        limit: params.limit,
    }))
}

async fn data_instruments(
    Auth(_ctx): Auth,
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<InstrumentSummary>>, ApiError> {
    let all = state.market_data.list_instruments();
    let filtered: Vec<_> = if let Some(ref q) = params.search {
        let q = q.to_lowercase();
        all.into_iter()
            .filter(|i| {
                i.id.to_lowercase().contains(&q)
                    || i.name.as_deref().unwrap_or("").to_lowercase().contains(&q)
                    || i.instrument_type.to_lowercase().contains(&q)
                    || i.currency.to_lowercase().contains(&q)
            })
            .collect()
    } else {
        all
    };
    let total = filtered.len();
    let items = filtered
        .into_iter()
        .skip(params.offset)
        .take(params.limit)
        .collect();
    Ok(Json(PaginatedResponse {
        items,
        total,
        offset: params.offset,
        limit: params.limit,
    }))
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
    let md = state.market_data.market_data();
    let history = md.get_price_history(&instrument, params.from, params.to)?;
    let points: Vec<PricePoint> = history
        .into_iter()
        .map(|(date, price)| PricePoint {
            date,
            price: price.value(),
        })
        .collect();
    Ok(Json(points))
}

pub async fn explorer() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../../../../tools/api-explorer.html"))
}
