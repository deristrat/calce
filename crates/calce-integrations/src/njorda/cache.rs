use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Datelike;

use calce_core::services::market_data::InMemoryMarketDataService;

use super::NjordaError;
use super::types::{CacheMetadata, CachedFxRate, CachedInstrument, CachedMarketData, CachedPrice};

const DEFAULT_CACHE_DIR: &str = ".cache/njorda";
const CACHE_FILENAME: &str = "njorda_market_data.rkyv.lz4";
const SERVICE_CACHE_FILENAME: &str = "market_data_service.bin.lz4";
const SERVICE_CACHE_VERSION: u32 = 2;
const DEFAULT_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

// ── Internal rkyv types (primitive date representations) ────────────────

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
struct RkyvMarketData {
    fetched_at_millis: i64,
    date_from_days: i32,
    date_to_days: i32,
    price_count: u64,
    fx_rate_count: u64,
    instrument_count: u64,
    prices: Vec<RkyvPrice>,
    fx_rates: Vec<RkyvFxRate>,
    instruments: Vec<RkyvInstrument>,
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
struct RkyvPrice {
    ticker: String,
    date_days: i32,
    close: f64,
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
struct RkyvFxRate {
    from: String,
    to: String,
    date_days: i32,
    rate: f64,
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
struct RkyvInstrument {
    ticker: String,
    currency: Option<String>,
    name: Option<String>,
    isin: Option<String>,
    instrument_type: Option<String>,
}

// ── Conversion helpers ──────────────────────────────────────────────────

fn to_rkyv(data: &CachedMarketData) -> RkyvMarketData {
    RkyvMarketData {
        fetched_at_millis: data.metadata.fetched_at.timestamp_millis(),
        date_from_days: data.metadata.date_from.num_days_from_ce(),
        date_to_days: data.metadata.date_to.num_days_from_ce(),
        price_count: data.metadata.price_count as u64,
        fx_rate_count: data.metadata.fx_rate_count as u64,
        instrument_count: data.metadata.instrument_count as u64,
        prices: data
            .prices
            .iter()
            .map(|p| RkyvPrice {
                ticker: p.ticker.clone(),
                date_days: p.date.num_days_from_ce(),
                close: p.close,
            })
            .collect(),
        fx_rates: data
            .fx_rates
            .iter()
            .map(|r| RkyvFxRate {
                from: r.from.clone(),
                to: r.to.clone(),
                date_days: r.date.num_days_from_ce(),
                rate: r.rate,
            })
            .collect(),
        instruments: data
            .instruments
            .iter()
            .map(|i| RkyvInstrument {
                ticker: i.ticker.clone(),
                currency: i.currency.clone(),
                name: i.name.clone(),
                isin: i.isin.clone(),
                instrument_type: i.instrument_type.clone(),
            })
            .collect(),
    }
}

fn from_rkyv(r: &ArchivedRkyvMarketData) -> CachedMarketData {
    let date_from_ce = |days: &rkyv::rend::i32_le| {
        chrono::NaiveDate::from_num_days_from_ce_opt(days.to_native())
            .expect("invalid date in cache")
    };

    CachedMarketData {
        metadata: CacheMetadata {
            fetched_at: chrono::DateTime::from_timestamp_millis(r.fetched_at_millis.to_native())
                .expect("invalid timestamp in cache"),
            date_from: date_from_ce(&r.date_from_days),
            date_to: date_from_ce(&r.date_to_days),
            price_count: r.price_count.to_native() as usize,
            fx_rate_count: r.fx_rate_count.to_native() as usize,
            instrument_count: r.instrument_count.to_native() as usize,
        },
        prices: r
            .prices
            .iter()
            .map(|p| CachedPrice {
                ticker: p.ticker.as_str().to_owned(),
                date: date_from_ce(&p.date_days),
                close: p.close.to_native(),
            })
            .collect(),
        fx_rates: r
            .fx_rates
            .iter()
            .map(|fx| CachedFxRate {
                from: fx.from.as_str().to_owned(),
                to: fx.to.as_str().to_owned(),
                date: date_from_ce(&fx.date_days),
                rate: fx.rate.to_native(),
            })
            .collect(),
        instruments: r
            .instruments
            .iter()
            .map(|i| CachedInstrument {
                ticker: i.ticker.as_str().to_owned(),
                currency: i.currency.as_ref().map(|s| s.as_str().to_owned()),
                name: i.name.as_ref().map(|s| s.as_str().to_owned()),
                isin: i.isin.as_ref().map(|s| s.as_str().to_owned()),
                instrument_type: i.instrument_type.as_ref().map(|s| s.as_str().to_owned()),
            })
            .collect(),
    }
}

// ── Public API ──────────────────────────────────────────────────────────

/// # Errors
///
/// Returns `NjordaError::CacheIo` on filesystem errors.
/// Returns `NjordaError::CacheFormat` on deserialization or decompression errors.
pub fn load_from_file(path: &Path) -> Result<CachedMarketData, NjordaError> {
    let compressed = std::fs::read(path).map_err(NjordaError::CacheIo)?;
    let bytes = lz4_flex::decompress_size_prepended(&compressed)
        .map_err(|e| NjordaError::CacheFormat(e.to_string()))?;
    let archived = rkyv::access::<ArchivedRkyvMarketData, rkyv::rancor::Error>(&bytes)
        .map_err(|e| NjordaError::CacheFormat(e.to_string()))?;
    Ok(from_rkyv(archived))
}

/// # Errors
///
/// Returns `NjordaError::CacheIo` on filesystem errors.
/// Returns `NjordaError::CacheFormat` on serialization errors.
pub fn save_to_file(path: &Path, data: &CachedMarketData) -> Result<(), NjordaError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(NjordaError::CacheIo)?;
    }
    let rkyv_data = to_rkyv(data);
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&rkyv_data)
        .map_err(|e| NjordaError::CacheFormat(e.to_string()))?;
    let compressed = lz4_flex::compress_prepend_size(&bytes);
    std::fs::write(path, compressed).map_err(NjordaError::CacheIo)?;
    Ok(())
}

/// # Errors
///
/// Returns `NjordaError::CacheIo` on filesystem errors.
/// Returns `NjordaError::CacheFormat` on serialization/compression errors.
pub fn save_service(path: &Path, service: &InMemoryMarketDataService) -> Result<(), NjordaError> {
    let bytes = bincode::serialize(service).map_err(|e| NjordaError::CacheFormat(e.to_string()))?;
    let mut data = Vec::with_capacity(4 + bytes.len());
    data.extend_from_slice(&SERVICE_CACHE_VERSION.to_le_bytes());
    data.extend_from_slice(&bytes);
    let compressed = lz4_flex::compress_prepend_size(&data);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(NjordaError::CacheIo)?;
    }
    std::fs::write(path, compressed).map_err(NjordaError::CacheIo)?;
    Ok(())
}

/// # Errors
///
/// Returns `NjordaError::CacheIo` on filesystem errors.
/// Returns `NjordaError::CacheFormat` on deserialization/decompression/version mismatch.
pub fn load_service(path: &Path) -> Result<InMemoryMarketDataService, NjordaError> {
    let compressed = std::fs::read(path).map_err(NjordaError::CacheIo)?;
    let data = lz4_flex::decompress_size_prepended(&compressed)
        .map_err(|e| NjordaError::CacheFormat(e.to_string()))?;
    if data.len() < 4 {
        return Err(NjordaError::CacheFormat("service cache too small".into()));
    }
    let version = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if version != SERVICE_CACHE_VERSION {
        return Err(NjordaError::CacheFormat(format!(
            "service cache version {version}, expected {SERVICE_CACHE_VERSION}"
        )));
    }
    bincode::deserialize(&data[4..]).map_err(|e| NjordaError::CacheFormat(e.to_string()))
}

/// Check if the service cache exists and is at least as new as the raw cache.
#[must_use]
pub fn service_is_fresh(service_path: &Path, raw_cache_path: &Path) -> bool {
    let Ok(service_meta) = std::fs::metadata(service_path) else {
        return false;
    };
    let Ok(cache_meta) = std::fs::metadata(raw_cache_path) else {
        return false;
    };
    let Ok(service_mtime) = service_meta.modified() else {
        return false;
    };
    let Ok(cache_mtime) = cache_meta.modified() else {
        return false;
    };
    service_mtime >= cache_mtime
}

#[must_use]
pub fn service_cache_path() -> PathBuf {
    PathBuf::from(DEFAULT_CACHE_DIR).join(SERVICE_CACHE_FILENAME)
}

#[must_use]
pub fn cache_path() -> PathBuf {
    std::env::var("CALCE_NJORDA_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_CACHE_DIR).join(CACHE_FILENAME))
}

#[must_use]
pub fn is_stale(path: &Path) -> bool {
    is_stale_with_max_age(path, DEFAULT_MAX_AGE)
}

#[must_use]
pub fn is_stale_with_max_age(path: &Path, max_age: Duration) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return true;
    };
    let Ok(modified) = metadata.modified() else {
        return true;
    };
    modified.elapsed().unwrap_or(Duration::MAX) > max_age
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_data() -> CachedMarketData {
        CachedMarketData {
            metadata: CacheMetadata {
                fetched_at: chrono::DateTime::from_timestamp_millis(1_700_000_000_000).unwrap(),
                date_from: chrono::NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                date_to: chrono::NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
                price_count: 2,
                fx_rate_count: 1,
                instrument_count: 2,
            },
            prices: vec![
                CachedPrice {
                    ticker: "AAPL".into(),
                    date: chrono::NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(),
                    close: 195.50,
                },
                CachedPrice {
                    ticker: "MSFT".into(),
                    date: chrono::NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
                    close: 420.99,
                },
            ],
            fx_rates: vec![CachedFxRate {
                from: "USD".into(),
                to: "SEK".into(),
                date: chrono::NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(),
                rate: 10.85,
            }],
            instruments: vec![
                CachedInstrument {
                    ticker: "AAPL".into(),
                    currency: Some("USD".into()),
                    name: Some("Apple Inc".into()),
                    isin: Some("US0378331005".into()),
                    instrument_type: Some("equity".into()),
                },
                CachedInstrument {
                    ticker: "MSFT".into(),
                    currency: Some("USD".into()),
                    name: None,
                    isin: None,
                    instrument_type: None,
                },
            ],
        }
    }

    #[test]
    fn roundtrip_empty() {
        let data = CachedMarketData {
            metadata: CacheMetadata {
                fetched_at: chrono::Utc::now(),
                date_from: chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                date_to: chrono::NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
                price_count: 0,
                fx_rate_count: 0,
                instrument_count: 0,
            },
            prices: vec![],
            fx_rates: vec![],
            instruments: vec![],
        };

        let dir = std::env::temp_dir().join("calce_test_cache_empty");
        let path = dir.join("test.rkyv.lz4");
        save_to_file(&path, &data).unwrap();
        let loaded = load_from_file(&path).unwrap();
        assert_eq!(loaded.metadata.price_count, 0);
        assert_eq!(loaded.metadata.date_from, data.metadata.date_from);
        assert!(loaded.prices.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn roundtrip_with_data() {
        let data = sample_data();
        let dir = std::env::temp_dir().join("calce_test_cache_data");
        let path = dir.join("test.rkyv.lz4");
        save_to_file(&path, &data).unwrap();
        let loaded = load_from_file(&path).unwrap();

        // Metadata
        assert_eq!(loaded.metadata.fetched_at, data.metadata.fetched_at);
        assert_eq!(loaded.metadata.date_from, data.metadata.date_from);
        assert_eq!(loaded.metadata.date_to, data.metadata.date_to);
        assert_eq!(loaded.metadata.price_count, 2);
        assert_eq!(loaded.metadata.fx_rate_count, 1);
        assert_eq!(loaded.metadata.instrument_count, 2);

        // Prices
        assert_eq!(loaded.prices.len(), 2);
        assert_eq!(loaded.prices[0].ticker, "AAPL");
        assert_eq!(loaded.prices[0].date, data.prices[0].date);
        assert!((loaded.prices[0].close - 195.50).abs() < f64::EPSILON);
        assert_eq!(loaded.prices[1].ticker, "MSFT");

        // FX rates
        assert_eq!(loaded.fx_rates.len(), 1);
        assert_eq!(loaded.fx_rates[0].from, "USD");
        assert_eq!(loaded.fx_rates[0].to, "SEK");
        assert!((loaded.fx_rates[0].rate - 10.85).abs() < f64::EPSILON);

        // Instruments — Some and None optional fields
        assert_eq!(loaded.instruments.len(), 2);
        assert_eq!(loaded.instruments[0].currency.as_deref(), Some("USD"));
        assert_eq!(loaded.instruments[0].name.as_deref(), Some("Apple Inc"));
        assert_eq!(loaded.instruments[0].isin.as_deref(), Some("US0378331005"));
        assert_eq!(loaded.instruments[1].name, None);
        assert_eq!(loaded.instruments[1].isin, None);
        assert_eq!(loaded.instruments[1].instrument_type, None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_corrupted_file_returns_error() {
        let dir = std::env::temp_dir().join("calce_test_cache_corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.rkyv.lz4");
        std::fs::write(&path, b"not valid data").unwrap();
        assert!(load_from_file(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_returns_io_error() {
        let result = load_from_file(Path::new("/nonexistent/path/cache.rkyv.lz4"));
        assert!(matches!(result, Err(NjordaError::CacheIo(_))));
    }

    #[test]
    fn stale_check_on_missing_file() {
        assert!(is_stale(Path::new("/nonexistent/path")));
    }
}
