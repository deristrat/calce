use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};

use calce_core::domain::currency::Currency;
use calce_core::domain::fx_rate::FxRate;
use calce_core::domain::instrument::{InstrumentId, InstrumentType};
use calce_core::domain::price::Price;
use calce_core::error::{CalceError, CalceResult};
use calce_core::services::market_data::MarketDataService;

fn day_ord(date: NaiveDate) -> i32 {
    date.num_days_from_ce()
}

/// Accumulation buffer used during construction (pre-freeze).
#[derive(Clone, Default)]
struct PendingData {
    prices: HashMap<InstrumentId, Vec<(i32, f64)>>,
    fx_rates: HashMap<(Currency, Currency), Vec<(i32, f64)>>,
}

/// In-memory market data backed by dense date-indexed arrays.
///
/// Dates map to array indices via `index = date.num_days_from_ce() - base_day`.
/// Missing prices are `f64::NAN`. Lookups are O(1).
///
/// Must be frozen (via `freeze()` or a bulk constructor) before querying.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct InMemoryMarketDataService {
    base_day: i32,
    num_days: usize,
    prices: HashMap<InstrumentId, Vec<f64>>,
    fx_rates: HashMap<(Currency, Currency), Vec<f64>>,
    #[serde(default)]
    instrument_types: HashMap<InstrumentId, InstrumentType>,
    #[serde(default)]
    allocations: HashMap<InstrumentId, HashMap<String, Vec<(String, f64)>>>,
    total_prices: usize,
    total_fx_rates: usize,
    #[serde(skip)]
    pending: Option<PendingData>,
}

impl Default for InMemoryMarketDataService {
    fn default() -> Self {
        Self {
            base_day: 0,
            num_days: 0,
            prices: HashMap::new(),
            fx_rates: HashMap::new(),
            instrument_types: HashMap::new(),
            allocations: HashMap::new(),
            total_prices: 0,
            total_fx_rates: 0,
            pending: Some(PendingData::default()),
        }
    }
}

impl InMemoryMarketDataService {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_price(&mut self, instrument: &InstrumentId, date: NaiveDate, price: Price) {
        debug_assert!(self.pending.is_some(), "cannot add_price after freeze");
        if let Some(pending) = &mut self.pending {
            pending
                .prices
                .entry(instrument.clone())
                .or_default()
                .push((day_ord(date), price.value()));
        }
    }

    /// Add an allocation weight for an instrument. Works before and after freeze.
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

    /// Set the instrument type. Works before and after freeze.
    pub fn add_instrument_type(
        &mut self,
        instrument: &InstrumentId,
        instrument_type: InstrumentType,
    ) {
        self.instrument_types
            .insert(instrument.clone(), instrument_type);
    }

    pub fn add_fx_rate(&mut self, rate: FxRate, date: NaiveDate) {
        debug_assert!(self.pending.is_some(), "cannot add_fx_rate after freeze");
        if let Some(pending) = &mut self.pending {
            pending
                .fx_rates
                .entry((rate.from, rate.to))
                .or_default()
                .push((day_ord(date), rate.rate));
        }
    }

    /// Sort pending data into dense date-indexed arrays and mark as ready for queries.
    #[allow(clippy::cast_sign_loss)]
    pub fn freeze(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };

        // Find global date range
        let mut min_day = i32::MAX;
        let mut max_day = i32::MIN;
        for points in pending.prices.values() {
            for &(d, _) in points {
                min_day = min_day.min(d);
                max_day = max_day.max(d);
            }
        }
        for points in pending.fx_rates.values() {
            for &(d, _) in points {
                min_day = min_day.min(d);
                max_day = max_day.max(d);
            }
        }

        if min_day > max_day {
            return;
        }

        self.base_day = min_day;
        self.num_days = (max_day - min_day + 1) as usize;

        for (id, points) in pending.prices {
            let mut arr = vec![f64::NAN; self.num_days];
            for (d, v) in points {
                arr[(d - self.base_day) as usize] = v;
            }
            self.total_prices += arr.iter().filter(|x| !x.is_nan()).count();
            self.prices.insert(id, arr);
        }

        for (pair, points) in pending.fx_rates {
            let mut arr = vec![f64::NAN; self.num_days];
            for (d, v) in points {
                arr[(d - self.base_day) as usize] = v;
            }
            self.total_fx_rates += arr.iter().filter(|x| !x.is_nan()).count();
            self.fx_rates.insert(pair, arr);
        }
    }

    /// Build from pre-grouped data. Scatters entries into dense arrays.
    /// The result is already frozen.
    #[must_use]
    #[allow(clippy::cast_sign_loss)]
    pub fn from_bulk(
        prices: HashMap<InstrumentId, Vec<(NaiveDate, f64)>>,
        fx_rates: HashMap<(Currency, Currency), Vec<(NaiveDate, f64)>>,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Self {
        let base_day = day_ord(from);
        let num_days = (day_ord(to) - base_day + 1) as usize;
        let mut total_prices = 0;
        let mut total_fx_rates = 0;

        let prices = prices
            .into_iter()
            .map(|(id, entries)| {
                let mut arr = vec![f64::NAN; num_days];
                for (d, v) in entries {
                    let idx = (day_ord(d) - base_day) as usize;
                    if idx < num_days {
                        arr[idx] = v;
                    }
                }
                total_prices += arr.iter().filter(|x| !x.is_nan()).count();
                (id, arr)
            })
            .collect();

        let fx_rates = fx_rates
            .into_iter()
            .map(|(pair, entries)| {
                let mut arr = vec![f64::NAN; num_days];
                for (d, v) in entries {
                    let idx = (day_ord(d) - base_day) as usize;
                    if idx < num_days {
                        arr[idx] = v;
                    }
                }
                total_fx_rates += arr.iter().filter(|x| !x.is_nan()).count();
                (pair, arr)
            })
            .collect();

        Self {
            base_day,
            num_days,
            prices,
            fx_rates,
            instrument_types: HashMap::new(),
            allocations: HashMap::new(),
            total_prices,
            total_fx_rates,
            pending: None,
        }
    }

    /// Build directly from pre-built dense arrays (used by njorda and deserialization).
    #[must_use]
    pub fn from_dense(
        base_day: i32,
        num_days: usize,
        prices: HashMap<InstrumentId, Vec<f64>>,
        fx_rates: HashMap<(Currency, Currency), Vec<f64>>,
    ) -> Self {
        let total_prices = prices
            .values()
            .flat_map(|v| v.iter())
            .filter(|x| !x.is_nan())
            .count();
        let total_fx_rates = fx_rates
            .values()
            .flat_map(|v| v.iter())
            .filter(|x| !x.is_nan())
            .count();

        Self {
            base_day,
            num_days,
            prices,
            fx_rates,
            instrument_types: HashMap::new(),
            allocations: HashMap::new(),
            total_prices,
            total_fx_rates,
            pending: None,
        }
    }

    #[must_use]
    pub fn price_count(&self) -> usize {
        self.total_prices
    }

    #[must_use]
    pub fn fx_rate_count(&self) -> usize {
        self.total_fx_rates
    }

    #[must_use]
    pub fn instrument_count(&self) -> usize {
        self.prices.len()
    }

    #[must_use]
    pub fn instrument_ids(&self) -> Vec<InstrumentId> {
        let mut ids: Vec<_> = self.prices.keys().cloned().collect();
        ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        ids
    }

    /// Approximate heap memory used by the dense arrays, in bytes.
    #[must_use]
    pub fn approx_heap_bytes(&self) -> usize {
        let price_bytes: usize = self.prices.values().map(|v| v.len() * 8).sum();
        let fx_bytes: usize = self.fx_rates.values().map(|v| v.len() * 8).sum();
        price_bytes + fx_bytes
    }

    #[allow(clippy::cast_sign_loss)]
    fn date_index(&self, date: NaiveDate) -> Option<usize> {
        let idx = day_ord(date) - self.base_day;
        if idx >= 0 && (idx as usize) < self.num_days {
            Some(idx as usize)
        } else {
            None
        }
    }
}

impl MarketDataService for InMemoryMarketDataService {
    fn get_price(&self, instrument: &InstrumentId, date: NaiveDate) -> CalceResult<Price> {
        debug_assert!(self.pending.is_none(), "must call freeze() before querying");
        let idx = self
            .date_index(date)
            .ok_or_else(|| CalceError::PriceNotFound {
                instrument: instrument.clone(),
                date,
            })?;
        let arr = self
            .prices
            .get(instrument)
            .ok_or_else(|| CalceError::PriceNotFound {
                instrument: instrument.clone(),
                date,
            })?;
        let v = arr[idx];
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
        debug_assert!(self.pending.is_none(), "must call freeze() before querying");
        let arr = self
            .prices
            .get(instrument)
            .ok_or_else(|| CalceError::PriceNotFound {
                instrument: instrument.clone(),
                date: from,
            })?;

        let start = (day_ord(from) - self.base_day).max(0) as usize;
        let end = ((day_ord(to) - self.base_day + 1).max(0) as usize).min(self.num_days);

        let result: Vec<(NaiveDate, Price)> = (start..end)
            .filter(|&i| !arr[i].is_nan())
            .map(|i| {
                let d = NaiveDate::from_num_days_from_ce_opt(self.base_day + i as i32)
                    .unwrap_or(NaiveDate::MIN);
                (d, Price::new(arr[i]))
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
        debug_assert!(self.pending.is_none(), "must call freeze() before querying");
        if from == to {
            return Ok(FxRate::identity(from));
        }
        let idx = self
            .date_index(date)
            .ok_or(CalceError::FxRateNotFound { from, to, date })?;
        let arr = self
            .fx_rates
            .get(&(from, to))
            .ok_or(CalceError::FxRateNotFound { from, to, date })?;
        let v = arr[idx];
        if v.is_nan() {
            return Err(CalceError::FxRateNotFound { from, to, date });
        }
        Ok(FxRate::new(from, to, v))
    }

    fn get_instrument_type(&self, instrument: &InstrumentId) -> InstrumentType {
        self.instrument_types
            .get(instrument)
            .copied()
            .unwrap_or_default()
    }

    fn get_allocations(&self, instrument: &InstrumentId, dimension: &str) -> Vec<(String, f64)> {
        self.allocations
            .get(instrument)
            .and_then(|dims| dims.get(dimension))
            .cloned()
            .unwrap_or_default()
    }
}
