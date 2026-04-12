use std::sync::Arc;

use chrono::NaiveDate;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use calce_core::domain::fx_rate::FxRate;
use calce_core::domain::instrument::{InstrumentId, InstrumentType};
use calce_core::domain::price::Price;
use calce_core::services::market_data::MarketDataService;
use calce_data::MarketDataBuilder;
use calce_data::concurrent_market_data::ConcurrentMarketData;
use calce_data::user_data_store::UserDataStore;

use crate::domain::Currency;

enum MarketDataInner {
    Builder(Box<MarketDataBuilder>),
    Ready(Arc<ConcurrentMarketData>),
}

#[pyclass]
pub struct MarketData {
    inner: MarketDataInner,
}

impl MarketData {
    /// Create from a pre-loaded concurrent cache (Postgres path).
    pub fn from_concurrent(md: Arc<ConcurrentMarketData>) -> Self {
        Self {
            inner: MarketDataInner::Ready(md),
        }
    }

    /// Borrow the concurrent cache as a trait object.
    ///
    /// # Panics
    ///
    /// Panics if the builder has not been materialised via [`ensure_ready`].
    #[allow(clippy::panic)] // documented panic — programming error if called before ensure_ready
    pub fn as_service(&self) -> &dyn MarketDataService {
        match &self.inner {
            MarketDataInner::Builder(_) => {
                panic!("MarketData not materialised — call ensure_ready() first")
            }
            MarketDataInner::Ready(svc) => svc.as_ref(),
        }
    }

    /// Materialise the builder into a concurrent cache. No-op if already ready.
    pub fn ensure_ready(&mut self) {
        if let MarketDataInner::Builder(builder) = &mut self.inner {
            let builder = std::mem::replace(builder, Box::new(MarketDataBuilder::new()));
            self.inner =
                MarketDataInner::Ready(Arc::new(ConcurrentMarketData::from_builder(*builder)));
        }
    }
}

fn require_builder(inner: &mut MarketDataInner) -> PyResult<&mut MarketDataBuilder> {
    match inner {
        MarketDataInner::Builder(b) => Ok(b),
        MarketDataInner::Ready(_) => Err(PyRuntimeError::new_err(
            "cannot add data after MarketData is materialised",
        )),
    }
}

#[pymethods]
impl MarketData {
    #[new]
    fn new() -> Self {
        MarketData {
            inner: MarketDataInner::Builder(Box::new(MarketDataBuilder::new())),
        }
    }

    fn add_price(&mut self, instrument_id: &str, date: NaiveDate, price: f64) -> PyResult<()> {
        let svc = require_builder(&mut self.inner)?;
        svc.add_price(&InstrumentId::new(instrument_id), date, Price::new(price));
        Ok(())
    }

    fn add_fx_rate(
        &mut self,
        from_currency: &Currency,
        to_currency: &Currency,
        rate: f64,
        date: NaiveDate,
    ) -> PyResult<()> {
        let svc = require_builder(&mut self.inner)?;
        svc.add_fx_rate(
            FxRate::new(from_currency.inner, to_currency.inner, rate),
            date,
        );
        Ok(())
    }

    fn add_instrument_type(&mut self, instrument_id: &str, instrument_type: &str) -> PyResult<()> {
        let svc = require_builder(&mut self.inner)?;
        svc.add_instrument_type(
            &InstrumentId::new(instrument_id),
            InstrumentType::from_str_lossy(instrument_type),
        );
        Ok(())
    }

    fn add_allocation(
        &mut self,
        instrument_id: &str,
        dimension: &str,
        key: &str,
        weight: f64,
    ) -> PyResult<()> {
        let svc = require_builder(&mut self.inner)?;
        svc.add_allocation(&InstrumentId::new(instrument_id), dimension, key, weight);
        Ok(())
    }
}

#[pyclass]
pub struct UserData {
    pub inner: UserDataStore,
}

#[pymethods]
impl UserData {
    #[new]
    fn new() -> Self {
        UserData {
            inner: UserDataStore::new(),
        }
    }

    fn add_trade(&mut self, trade: &crate::domain::Trade) {
        self.inner.add_trade(trade.inner.clone());
    }
}
