use serde::Serialize;

#[derive(Serialize)]
pub struct DataStats {
    pub user_count: i64,
    pub organization_count: i64,
    pub instrument_count: i64,
    pub trade_count: i64,
    pub price_count: i64,
    pub fx_rate_count: i64,
}
