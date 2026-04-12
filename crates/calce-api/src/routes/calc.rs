use std::collections::HashMap;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use calce_core::calc::aggregation;
use calce_core::calc::market_value::{self, MarketValueResult};
use calce_core::calc::volatility::{self, VolatilityResult};
use calce_core::context::CalculationContext;
use calce_core::domain::account::AccountId;
use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::trade::Trade;
use calce_core::domain::user::UserId;
use calce_core::outcome::{Outcome, Warning, WarningCode};
use calce_core::reports::portfolio::PortfolioReport;
use calce_core::services::market_data::MarketDataService;
use calce_data::market_data_store::{FxRateSummary, InstrumentSummary};
use calce_data::queries::user_data::{AccountSummary, UserDataRepo};
use calce_data::types::DataStats;
use calce_data::user_data_store::UserSummary;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

const DEFAULT_PAGE_SIZE: usize = 50;

use crate::auth::{self, Auth};
use crate::error::ApiError;
use crate::state::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/version", get(version))
        .route("/v1/users/{user_id}/market-value", get(market_value))
        .route("/v1/users/{user_id}/portfolio", get(portfolio_report))
        .route(
            "/v1/instruments/{instrument_id}/volatility",
            get(volatility),
        )
        .route("/v1/data/stats", get(data_stats))
        .route("/v1/data/users", get(data_users))
        .route("/v1/data/users/{user_id}", get(data_user))
        .route("/v1/data/users/{user_id}/accounts", get(user_accounts))
        .route("/v1/data/users/{user_id}/positions", get(user_positions))
        .route(
            "/v1/data/users/{user_id}/positions/{instrument_id}/trades",
            get(position_trades),
        )
        .route(
            "/v1/data/users/{user_id}/accounts/{account_id}/positions",
            get(account_positions),
        )
        .route(
            "/v1/data/users/{user_id}/accounts/{account_id}/trades",
            get(account_trades),
        )
        .route("/v1/data/users/{user_id}/trades", get(user_trades))
        .route("/v1/data/fx-rates", get(data_fx_rates))
        .route(
            "/v1/data/fx-rates/{from}/{to}/history",
            get(fx_rate_history),
        )
        .route("/v1/data/instruments", get(data_instruments))
        .route("/v1/data/instruments/{id}", get(data_instrument))
        .route(
            "/v1/data/instruments/{instrument_id}/prices",
            get(instrument_prices),
        )
}

async fn version() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "version": env!("CARGO_PKG_VERSION") }))
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

#[allow(clippy::similar_names)] // stats/state are distinct concepts
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
                    || u.name.as_deref().unwrap_or("").to_lowercase().contains(&q)
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

async fn data_user(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<UserSummary>, ApiError> {
    let user_id = UserId::new(user_id);
    state
        .user_data
        .get_user(&ctx, &user_id)?
        .map(Json)
        .ok_or_else(|| ApiError::Data(calce_data::error::DataError::NotFound("user".into())))
}

async fn user_accounts(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<AccountSummary>>, ApiError> {
    auth::require_admin(&ctx)?;
    let pool = state.require_pool()?;
    let repo = UserDataRepo::new(pool.clone());
    let accounts = repo.get_user_accounts(&user_id).await?;
    Ok(Json(accounts))
}

#[derive(Serialize)]
struct PositionResponse {
    instrument_id: String,
    instrument_name: Option<String>,
    quantity: f64,
    currency: String,
    trade_count: i64,
}

async fn user_positions(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<PositionResponse>>, ApiError> {
    let user_id = UserId::new(user_id);
    let positions = state.user_data.positions_for_user(&ctx, &user_id)?;

    let instruments = state.market_data.list_instruments();
    let name_lookup: HashMap<&str, Option<&str>> = instruments
        .iter()
        .map(|i| (i.ticker.as_str(), i.name.as_deref()))
        .collect();

    let enriched = positions
        .into_iter()
        .map(|p| {
            let instrument_name = name_lookup
                .get(p.instrument_id.as_str())
                .copied()
                .flatten()
                .map(str::to_owned);
            PositionResponse {
                instrument_id: p.instrument_id.to_string(),
                instrument_name,
                quantity: p.quantity,
                currency: p.currency.to_string(),
                trade_count: p.trade_count,
            }
        })
        .collect();

    Ok(Json(enriched))
}

/// Best-effort lookup: returns empty map if DB is unavailable (in-memory mode).
async fn load_account_names(state: &AppState, user_id: &str) -> HashMap<i64, String> {
    let Some(pool) = state.pool.as_ref() else {
        return HashMap::new();
    };
    let repo = UserDataRepo::new(pool.clone());
    match repo.get_account_names(user_id).await {
        Ok(pairs) => pairs.into_iter().collect(),
        Err(_) => HashMap::new(),
    }
}

#[derive(Serialize)]
struct TradeSummary {
    account_id: i64,
    account_name: Option<String>,
    instrument_id: String,
    quantity: f64,
    price: f64,
    total_value: f64,
    currency: String,
    date: NaiveDate,
}

impl TradeSummary {
    fn from_trade(t: &Trade, account_names: &HashMap<i64, String>) -> Self {
        let quantity = t.quantity.value();
        let price = t.price.value();
        Self {
            account_id: t.account_id.value(),
            account_name: account_names.get(&t.account_id.value()).cloned(),
            instrument_id: t.instrument_id.as_str().to_owned(),
            quantity,
            price,
            total_value: quantity * price,
            currency: t.currency.as_str().to_owned(),
            date: t.date,
        }
    }
}

#[derive(Deserialize)]
struct PositionTradesPath {
    user_id: String,
    instrument_id: String,
}

async fn position_trades(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Path(path): Path<PositionTradesPath>,
) -> Result<Json<Vec<TradeSummary>>, ApiError> {
    let user_id = UserId::new(&path.user_id);
    let trades = state.user_data.load_trades(&ctx, &[user_id])?;
    let account_names = load_account_names(&state, &path.user_id).await;
    let mut filtered: Vec<TradeSummary> = trades
        .iter()
        .filter(|t| t.instrument_id.as_str() == path.instrument_id)
        .map(|t| TradeSummary::from_trade(t, &account_names))
        .collect();
    filtered.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(Json(filtered))
}

#[derive(Deserialize)]
struct AccountTradesPath {
    user_id: String,
    account_id: i64,
}

async fn account_trades(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Path(path): Path<AccountTradesPath>,
) -> Result<Json<Vec<TradeSummary>>, ApiError> {
    let user_id = UserId::new(&path.user_id);
    let trades = state.user_data.load_trades(&ctx, &[user_id])?;
    let account_names = load_account_names(&state, &path.user_id).await;
    let mut filtered: Vec<TradeSummary> = trades
        .iter()
        .filter(|t| t.account_id.value() == path.account_id)
        .map(|t| TradeSummary::from_trade(t, &account_names))
        .collect();
    filtered.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(Json(filtered))
}

async fn account_positions(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Path(path): Path<AccountTradesPath>,
) -> Result<Json<Vec<PositionResponse>>, ApiError> {
    let user_id = UserId::new(&path.user_id);
    let positions =
        state
            .user_data
            .positions_for_account(&ctx, &user_id, AccountId::new(path.account_id))?;

    let instruments = state.market_data.list_instruments();
    let name_lookup: HashMap<&str, Option<&str>> = instruments
        .iter()
        .map(|i| (i.ticker.as_str(), i.name.as_deref()))
        .collect();

    let enriched = positions
        .into_iter()
        .map(|p| {
            let instrument_name = name_lookup
                .get(p.instrument_id.as_str())
                .copied()
                .flatten()
                .map(str::to_owned);
            PositionResponse {
                instrument_id: p.instrument_id.to_string(),
                instrument_name,
                quantity: p.quantity,
                currency: p.currency.to_string(),
                trade_count: p.trade_count,
            }
        })
        .collect();

    Ok(Json(enriched))
}

async fn user_trades(
    Auth(ctx): Auth,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<TradeSummary>>, ApiError> {
    let names = load_account_names(&state, &user_id).await;
    let user_id = UserId::new(user_id);
    let trades = state.user_data.load_trades(&ctx, &[user_id])?;
    let mut result: Vec<TradeSummary> = trades
        .iter()
        .map(|t| TradeSummary::from_trade(t, &names))
        .collect();
    result.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(Json(result))
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
                i.ticker.to_lowercase().contains(&q)
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
struct FxRateParams {
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_page_size")]
    limit: usize,
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    from_currency: Option<String>,
    #[serde(default)]
    to_currency: Option<String>,
}

async fn data_fx_rates(
    Auth(_ctx): Auth,
    State(state): State<AppState>,
    Query(params): Query<FxRateParams>,
) -> Result<Json<PaginatedResponse<FxRateSummary>>, ApiError> {
    let all = state.market_data.list_fx_rates();
    let filtered: Vec<_> = all
        .into_iter()
        .filter(|r| {
            if let Some(ref q) = params.search {
                let q = q.to_lowercase();
                if !r.pair.to_lowercase().contains(&q)
                    && !r.from_currency.to_lowercase().contains(&q)
                    && !r.to_currency.to_lowercase().contains(&q)
                {
                    return false;
                }
            }
            if let Some(ref f) = params.from_currency {
                let f = f.to_uppercase();
                if r.from_currency != f {
                    return false;
                }
            }
            if let Some(ref t) = params.to_currency {
                let t = t.to_uppercase();
                if r.to_currency != t {
                    return false;
                }
            }
            true
        })
        .collect();
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
struct FxRateHistoryPath {
    from: String,
    to: String,
}

async fn fx_rate_history(
    Auth(_ctx): Auth,
    State(state): State<AppState>,
    Path(path): Path<FxRateHistoryPath>,
    Query(params): Query<PriceHistoryParams>,
) -> Result<Json<Vec<PricePoint>>, ApiError> {
    let from = parse_currency(&path.from)?;
    let to = parse_currency(&path.to)?;
    let md = state.market_data.market_data();
    let history = md.get_fx_rate_history_range(from, to, params.from, params.to);
    let points: Vec<PricePoint> = history
        .into_iter()
        .map(|(date, rate)| PricePoint { date, price: rate })
        .collect();
    Ok(Json(points))
}

async fn data_instrument(
    Auth(_ctx): Auth,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<InstrumentSummary>, ApiError> {
    state
        .market_data
        .get_instrument(id)
        .map(Json)
        .ok_or_else(|| ApiError::Data(calce_data::error::DataError::NotFound("instrument".into())))
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
