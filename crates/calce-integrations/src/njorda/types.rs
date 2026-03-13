use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedMarketData {
    pub metadata: CacheMetadata,
    pub prices: Vec<CachedPrice>,
    pub fx_rates: Vec<CachedFxRate>,
    pub instruments: Vec<CachedInstrument>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
    pub price_count: usize,
    pub fx_rate_count: usize,
    pub instrument_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedPrice {
    pub ticker: String,
    pub date: NaiveDate,
    pub close: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedFxRate {
    pub from: String,
    pub to: String,
    pub date: NaiveDate,
    pub rate: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedInstrument {
    pub ticker: String,
    pub currency: Option<String>,
    pub name: Option<String>,
    pub isin: Option<String>,
    pub instrument_type: Option<String>,
}
