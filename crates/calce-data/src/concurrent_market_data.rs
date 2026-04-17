use std::collections::HashMap;

use calce_core::domain::currency::Currency;
use calce_core::domain::fx_rate::FxRate;
use calce_core::domain::instrument::{InstrumentId, InstrumentType};
use calce_core::domain::price::Price;
use calce_core::error::{CalceError, CalceResult};
use calce_core::services::market_data::MarketDataService;
use calce_datastructs::cache::{CacheError, TimeSeriesCache};
use calce_datastructs::pubsub::UpdateEvent;
use chrono::{Datelike, NaiveDate};
use dashmap::DashMap;
use tokio::sync::mpsc;

fn day_ord(date: NaiveDate) -> i32 {
    date.num_days_from_ce()
}

/// Accumulates market data for bulk-loading into [`ConcurrentMarketData`].
///
/// Call [`add_price`], [`add_fx_rate`], [`add_instrument_type`], and
/// [`add_allocation`] to build up data, then pass the builder to
/// [`ConcurrentMarketData::from_builder`] to materialise the concurrent store.
///
/// [`add_price`]: MarketDataBuilder::add_price
/// [`add_fx_rate`]: MarketDataBuilder::add_fx_rate
/// [`add_instrument_type`]: MarketDataBuilder::add_instrument_type
/// [`add_allocation`]: MarketDataBuilder::add_allocation
#[derive(Default)]
pub struct MarketDataBuilder {
    prices: HashMap<InstrumentId, Vec<(i32, f64)>>,
    fx_rates: HashMap<(Currency, Currency), Vec<(i32, f64)>>,
    instrument_types: HashMap<InstrumentId, InstrumentType>,
    allocations: HashMap<InstrumentId, HashMap<String, Vec<(String, f64)>>>,
}

impl MarketDataBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_price(&mut self, instrument: &InstrumentId, date: NaiveDate, price: Price) {
        self.prices
            .entry(instrument.clone())
            .or_default()
            .push((date.num_days_from_ce(), price.value()));
    }

    pub fn add_fx_rate(&mut self, rate: FxRate, date: NaiveDate) {
        self.fx_rates
            .entry((rate.from, rate.to))
            .or_default()
            .push((date.num_days_from_ce(), rate.rate));
    }

    pub fn add_instrument_type(
        &mut self,
        instrument: &InstrumentId,
        instrument_type: InstrumentType,
    ) {
        self.instrument_types
            .insert(instrument.clone(), instrument_type);
    }

    pub fn add_allocation(
        &mut self,
        instrument: &InstrumentId,
        dimension: &str,
        key: &str,
        weight: f64,
    ) {
        self.allocations
            .entry(instrument.clone())
            .or_default()
            .entry(dimension.to_owned())
            .or_default()
            .push((key.to_owned(), weight));
    }
}

/// Concurrent market data store backed by lock-free time-series caches.
///
/// Wraps two [`TimeSeriesCache`] instances (prices keyed by instrument,
/// FX rates keyed by currency pair) plus `DashMap`s for instrument metadata.
/// Supports concurrent reads and writes — no server restart needed to update
/// market data.
pub struct ConcurrentMarketData {
    base_day: i32,
    num_days: usize,
    prices: TimeSeriesCache<InstrumentId>,
    fx_rates: TimeSeriesCache<(Currency, Currency)>,
    instrument_types: DashMap<InstrumentId, InstrumentType>,
    allocations: DashMap<InstrumentId, HashMap<String, Vec<(String, f64)>>>,
}

impl ConcurrentMarketData {
    /// Materialise a [`MarketDataBuilder`] into a concurrent cache.
    ///
    /// Scatters accumulated data into dense date-indexed arrays, then
    /// bulk-loads everything into lock-free caches.
    #[must_use]
    #[allow(clippy::cast_sign_loss)]
    pub fn from_builder(builder: MarketDataBuilder) -> Self {
        let MarketDataBuilder {
            prices: pending_prices,
            fx_rates: pending_fx,
            instrument_types: types_map,
            allocations: alloc_map,
        } = builder;

        // Find global date range across all data points.
        let mut min_day = i32::MAX;
        let mut max_day = i32::MIN;
        for points in pending_prices.values() {
            for &(d, _) in points {
                min_day = min_day.min(d);
                max_day = max_day.max(d);
            }
        }
        for points in pending_fx.values() {
            for &(d, _) in points {
                min_day = min_day.min(d);
                max_day = max_day.max(d);
            }
        }

        if min_day > max_day {
            return Self::empty();
        }

        let base_day = min_day;
        let num_days = (max_day - min_day + 1) as usize;

        // Scatter into dense arrays and bulk-load into lock-free caches.
        let prices = TimeSeriesCache::new();
        prices.bulk_insert(pending_prices.into_iter().map(|(id, points)| {
            let mut arr = vec![f64::NAN; num_days];
            for (d, v) in points {
                arr[(d - base_day) as usize] = v;
            }
            let current = last_non_nan(&arr);
            (id, current, arr)
        }));

        let fx_rates = TimeSeriesCache::new();
        fx_rates.bulk_insert(pending_fx.into_iter().map(|(pair, points)| {
            let mut arr = vec![f64::NAN; num_days];
            for (d, v) in points {
                arr[(d - base_day) as usize] = v;
            }
            let current = last_non_nan(&arr);
            (pair, current, arr)
        }));

        let instrument_types: DashMap<_, _> = types_map.into_iter().collect();
        let allocations: DashMap<_, _> = alloc_map.into_iter().collect();

        Self {
            base_day,
            num_days,
            prices,
            fx_rates,
            instrument_types,
            allocations,
        }
    }

    fn empty() -> Self {
        Self {
            base_day: 0,
            num_days: 0,
            prices: TimeSeriesCache::new(),
            fx_rates: TimeSeriesCache::new(),
            instrument_types: DashMap::new(),
            allocations: DashMap::new(),
        }
    }

    // -- Query helpers (non-trait) -------------------------------------------

    #[must_use]
    pub fn price_count(&self) -> usize {
        let mut total = 0;
        for key in self.instrument_ids() {
            if let Some(hist) = self.prices.get_history(&key) {
                total += hist.iter().filter(|v| !v.is_nan()).count();
            }
        }
        total
    }

    #[must_use]
    pub fn fx_rate_count(&self) -> usize {
        let mut total = 0;
        for pair in self.fx_rate_keys() {
            if let Some(hist) = self.fx_rates.get_history(&pair) {
                total += hist.iter().filter(|v| !v.is_nan()).count();
            }
        }
        total
    }

    #[must_use]
    pub fn instrument_count(&self) -> usize {
        self.prices.len()
    }

    #[must_use]
    pub fn instrument_ids(&self) -> Vec<InstrumentId> {
        // DashMap doesn't expose keys directly in a sorted way, so we
        // iterate snapshots via a batch read of all keys.
        let mut ids = Vec::with_capacity(self.prices.len());
        self.prices.for_each_key(|k| ids.push(k.clone()));
        ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        ids
    }

    /// Return summary info for each FX rate pair.
    #[must_use]
    pub fn fx_rate_pairs(&self) -> Vec<(Currency, Currency, usize, Option<f64>)> {
        let mut pairs: Vec<_> = self
            .fx_rate_keys()
            .into_iter()
            .map(|(from, to)| {
                let hist = self.fx_rates.get_history(&(from, to));
                let (count, latest) = hist.as_deref().map_or((0, None), |h| {
                    let count = h.iter().filter(|x| !x.is_nan()).count();
                    let latest = h.iter().rposition(|x| !x.is_nan()).map(|i| h[i]);
                    (count, latest)
                });
                (from, to, count, latest)
            })
            .collect();
        pairs.sort_by(|a, b| {
            a.0.as_str()
                .cmp(b.0.as_str())
                .then_with(|| a.1.as_str().cmp(b.1.as_str()))
        });
        pairs
    }

    /// Return FX rate history for a currency pair within a date range.
    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    pub fn get_fx_rate_history_range(
        &self,
        from_ccy: Currency,
        to_ccy: Currency,
        date_from: NaiveDate,
        date_to: NaiveDate,
    ) -> Vec<(NaiveDate, f64)> {
        let Some(hist) = self.fx_rates.get_history(&(from_ccy, to_ccy)) else {
            return Vec::new();
        };
        let start = (day_ord(date_from) - self.base_day).max(0) as usize;
        let end = ((day_ord(date_to) - self.base_day + 1).max(0) as usize).min(self.num_days);

        (start..end)
            .filter(|&i| i < hist.len() && !hist[i].is_nan())
            .map(|i| {
                let d = NaiveDate::from_num_days_from_ce_opt(self.base_day + i as i32)
                    .unwrap_or(NaiveDate::MIN);
                (d, hist[i])
            })
            .collect()
    }

    /// Approximate heap memory used by the dense arrays, in bytes.
    #[must_use]
    pub fn approx_heap_bytes(&self) -> usize {
        let mut bytes = 0;
        self.prices.for_each_key(|k| {
            if let Some(h) = self.prices.get_history(k) {
                bytes += h.len() * 8;
            }
        });
        self.fx_rates.for_each_key(|k| {
            if let Some(h) = self.fx_rates.get_history(k) {
                bytes += h.len() * 8;
            }
        });
        bytes
    }

    // -- Notification wiring -------------------------------------------------

    /// Wire a price notification sender (for `PubSub` integration).
    /// Must be called before any writes if notifications are desired.
    pub fn enable_price_notifications(&self, tx: mpsc::Sender<UpdateEvent<InstrumentId>>) {
        let _ = self.prices.set_notifier(tx);
    }

    /// Wire an FX rate notification sender (for `PubSub` integration).
    /// Must be called before any writes if notifications are desired.
    pub fn enable_fx_notifications(&self, tx: mpsc::Sender<UpdateEvent<(Currency, Currency)>>) {
        let _ = self.fx_rates.set_notifier(tx);
    }

    /// All FX pair keys in the cache.
    #[must_use]
    pub fn fx_pair_keys(&self) -> Vec<(Currency, Currency)> {
        self.fx_rate_keys()
    }

    /// Current (latest) price for an instrument.
    #[must_use]
    pub fn current_price(&self, instrument: &InstrumentId) -> Option<f64> {
        self.prices.get_current(instrument)
    }

    /// Update the current (latest) price for an instrument.
    pub fn set_current_price(
        &self,
        instrument: &InstrumentId,
        price: f64,
    ) -> Result<(), CacheError> {
        self.prices.update_current(instrument, price)
    }

    /// Current (latest) FX rate for a currency pair.
    #[must_use]
    pub fn current_fx_rate(&self, from: Currency, to: Currency) -> Option<f64> {
        self.fx_rates.get_current(&(from, to))
    }

    /// Update the current (latest) FX rate for a currency pair.
    pub fn set_current_fx_rate(
        &self,
        from: Currency,
        to: Currency,
        rate: f64,
    ) -> Result<(), CacheError> {
        self.fx_rates.update_current(&(from, to), rate)
    }

    // -- Mutation methods ----------------------------------------------------

    /// Update a single price point at a specific date.
    ///
    /// # Errors
    ///
    /// Returns `CacheError::IndexOutOfBounds` if the date is outside the
    /// covered range, or `CacheError::KeyNotFound` if the instrument is unknown.
    pub fn update_price(
        &self,
        instrument: &InstrumentId,
        date: NaiveDate,
        price: f64,
    ) -> Result<(), CacheError> {
        let idx = self.date_index(date).ok_or(CacheError::IndexOutOfBounds)?;
        self.prices.update_history(instrument, idx, price)
    }

    /// Update a single FX rate at a specific date.
    ///
    /// # Errors
    ///
    /// Returns `CacheError::IndexOutOfBounds` if the date is outside the
    /// covered range, or `CacheError::KeyNotFound` if the pair is unknown.
    pub fn update_fx_rate(
        &self,
        from: Currency,
        to: Currency,
        date: NaiveDate,
        rate: f64,
    ) -> Result<(), CacheError> {
        let idx = self.date_index(date).ok_or(CacheError::IndexOutOfBounds)?;
        self.fx_rates.update_history(&(from, to), idx, rate)
    }

    // -- Private helpers -----------------------------------------------------

    #[allow(clippy::cast_sign_loss)]
    fn date_index(&self, date: NaiveDate) -> Option<usize> {
        let idx = day_ord(date) - self.base_day;
        if idx >= 0 && (idx as usize) < self.num_days {
            Some(idx as usize)
        } else {
            None
        }
    }

    fn fx_rate_keys(&self) -> Vec<(Currency, Currency)> {
        let mut keys = Vec::with_capacity(self.fx_rates.len());
        self.fx_rates.for_each_key(|k| keys.push(*k));
        keys
    }
}

impl MarketDataService for ConcurrentMarketData {
    fn get_price(&self, instrument: &InstrumentId, date: NaiveDate) -> CalceResult<Price> {
        let idx = self
            .date_index(date)
            .ok_or_else(|| CalceError::PriceNotFound {
                instrument: instrument.clone(),
                date,
            })?;
        let hist =
            self.prices
                .get_history(instrument)
                .ok_or_else(|| CalceError::PriceNotFound {
                    instrument: instrument.clone(),
                    date,
                })?;
        let v = hist.get(idx).copied().unwrap_or(f64::NAN);
        if v.is_nan() {
            return Err(CalceError::PriceNotFound {
                instrument: instrument.clone(),
                date,
            });
        }
        Ok(Price::new(v))
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    fn get_price_history(
        &self,
        instrument: &InstrumentId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> CalceResult<Vec<(NaiveDate, Price)>> {
        let hist =
            self.prices
                .get_history(instrument)
                .ok_or_else(|| CalceError::PriceNotFound {
                    instrument: instrument.clone(),
                    date: from,
                })?;

        let start = (day_ord(from) - self.base_day).max(0) as usize;
        let end = ((day_ord(to) - self.base_day + 1).max(0) as usize).min(self.num_days);

        let result: Vec<(NaiveDate, Price)> = (start..end)
            .filter(|&i| i < hist.len() && !hist[i].is_nan())
            .map(|i| {
                let d = NaiveDate::from_num_days_from_ce_opt(self.base_day + i as i32)
                    .unwrap_or(NaiveDate::MIN);
                (d, Price::new(hist[i]))
            })
            .collect();

        if result.is_empty() {
            return Err(CalceError::PriceNotFound {
                instrument: instrument.clone(),
                date: from,
            });
        }
        Ok(result)
    }

    fn get_fx_rate(&self, from: Currency, to: Currency, date: NaiveDate) -> CalceResult<FxRate> {
        if from == to {
            return Ok(FxRate::identity(from));
        }
        let idx = self
            .date_index(date)
            .ok_or(CalceError::FxRateNotFound { from, to, date })?;
        let hist = self
            .fx_rates
            .get_history(&(from, to))
            .ok_or(CalceError::FxRateNotFound { from, to, date })?;
        let v = hist.get(idx).copied().unwrap_or(f64::NAN);
        if v.is_nan() {
            return Err(CalceError::FxRateNotFound { from, to, date });
        }
        Ok(FxRate::new(from, to, v))
    }

    fn get_instrument_type(&self, instrument: &InstrumentId) -> InstrumentType {
        self.instrument_types
            .get(instrument)
            .map(|r| *r)
            .unwrap_or_default()
    }

    fn get_allocations(&self, instrument: &InstrumentId, dimension: &str) -> Vec<(String, f64)> {
        self.allocations
            .get(instrument)
            .and_then(|dims| dims.get(dimension).cloned())
            .unwrap_or_default()
    }
}

/// Return the last non-NaN value in a slice, or `f64::NAN` if none found.
fn last_non_nan(data: &[f64]) -> f64 {
    data.iter()
        .rev()
        .find(|v| !v.is_nan())
        .copied()
        .unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use calce_core::domain::currency::Currency;
    use calce_core::domain::fx_rate::FxRate;
    use calce_core::domain::instrument::InstrumentId;
    use calce_core::domain::price::Price;
    use chrono::NaiveDate;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid test date")
    }

    fn build_test_builder() -> MarketDataBuilder {
        let aapl = InstrumentId::new("AAPL");
        let msft = InstrumentId::new("MSFT");
        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");

        let mut b = MarketDataBuilder::new();
        b.add_price(&aapl, date(2025, 1, 10), Price::new(150.0));
        b.add_price(&aapl, date(2025, 1, 13), Price::new(152.0));
        b.add_price(&msft, date(2025, 1, 10), Price::new(400.0));
        b.add_fx_rate(FxRate::new(usd, sek, 10.5), date(2025, 1, 10));
        b.add_fx_rate(FxRate::new(usd, sek, 10.6), date(2025, 1, 13));
        b.add_instrument_type(&aapl, InstrumentType::Stock);
        b.add_allocation(&aapl, "sector", "Technology", 1.0);
        b
    }

    #[test]
    fn from_builder_preserves_prices() {
        let md = build_test_builder();
        let concurrent = ConcurrentMarketData::from_builder(md);

        let aapl = InstrumentId::new("AAPL");
        let price = concurrent.get_price(&aapl, date(2025, 1, 10));
        assert!(price.is_ok());
        assert_eq!(price.ok().map(|p| p.value()), Some(150.0));

        let price2 = concurrent.get_price(&aapl, date(2025, 1, 13));
        assert_eq!(price2.ok().map(|p| p.value()), Some(152.0));
    }

    #[test]
    fn from_builder_preserves_fx_rates() {
        let md = build_test_builder();
        let concurrent = ConcurrentMarketData::from_builder(md);

        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");
        let rate = concurrent.get_fx_rate(usd, sek, date(2025, 1, 10));
        assert!(rate.is_ok());
        assert_eq!(rate.ok().map(|r| r.rate), Some(10.5));
    }

    #[test]
    fn identity_fx_rate() {
        let md = build_test_builder();
        let concurrent = ConcurrentMarketData::from_builder(md);

        let usd = Currency::new("USD");
        let rate = concurrent.get_fx_rate(usd, usd, date(2025, 1, 10));
        assert!(rate.is_ok());
        assert_eq!(rate.ok().map(|r| r.rate), Some(1.0));
    }

    #[test]
    fn price_not_found_for_missing_date() {
        let md = build_test_builder();
        let concurrent = ConcurrentMarketData::from_builder(md);

        let aapl = InstrumentId::new("AAPL");
        // Jan 11 is a Saturday with no data
        assert!(concurrent.get_price(&aapl, date(2025, 1, 11)).is_err());
    }

    #[test]
    fn price_history_returns_range() {
        let md = build_test_builder();
        let concurrent = ConcurrentMarketData::from_builder(md);

        let aapl = InstrumentId::new("AAPL");
        let history = concurrent.get_price_history(&aapl, date(2025, 1, 10), date(2025, 1, 13));
        assert!(history.is_ok());
        let history = history.ok();
        assert!(history.is_some());
        assert_eq!(history.as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn instrument_type_and_allocations() {
        let md = build_test_builder();
        let concurrent = ConcurrentMarketData::from_builder(md);

        let aapl = InstrumentId::new("AAPL");
        assert_eq!(concurrent.get_instrument_type(&aapl), InstrumentType::Stock);
        let alloc = concurrent.get_allocations(&aapl, "sector");
        assert_eq!(alloc.len(), 1);
        assert_eq!(alloc[0].0, "Technology");
    }

    #[test]
    fn concurrent_update_price() {
        let md = build_test_builder();
        let concurrent = ConcurrentMarketData::from_builder(md);

        let aapl = InstrumentId::new("AAPL");
        // Update the price at an existing date
        concurrent
            .update_price(&aapl, date(2025, 1, 10), 155.0)
            .expect("update should succeed");
        let price = concurrent.get_price(&aapl, date(2025, 1, 10));
        assert_eq!(price.ok().map(|p| p.value()), Some(155.0));

        // Other dates unaffected
        let price2 = concurrent.get_price(&aapl, date(2025, 1, 13));
        assert_eq!(price2.ok().map(|p| p.value()), Some(152.0));
    }

    #[test]
    fn fx_rate_history_range() {
        let md = build_test_builder();
        let concurrent = ConcurrentMarketData::from_builder(md);

        let usd = Currency::new("USD");
        let sek = Currency::new("SEK");
        let history =
            concurrent.get_fx_rate_history_range(usd, sek, date(2025, 1, 10), date(2025, 1, 13));
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn stats_methods() {
        let md = build_test_builder();
        let concurrent = ConcurrentMarketData::from_builder(md);

        assert_eq!(concurrent.instrument_count(), 2);
        assert_eq!(concurrent.price_count(), 3);
        assert_eq!(concurrent.fx_rate_count(), 2);
        assert!(concurrent.approx_heap_bytes() > 0);
        assert_eq!(concurrent.instrument_ids().len(), 2);
        assert_eq!(concurrent.fx_rate_pairs().len(), 1);
    }
}
