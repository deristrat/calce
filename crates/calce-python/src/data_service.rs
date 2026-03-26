use chrono::NaiveDate;
use pyo3::prelude::*;

use calce_core::domain::currency::Currency;
use calce_core::domain::instrument::InstrumentId;
use calce_core::services::market_data::MarketDataService;
use calce_data::auth::SecurityContext;
use calce_data::market_data_store::InstrumentSummary;
use calce_data::user_data_store::UserSummary;

use crate::data_types::{PyDataStats, PyInstrumentInfo, PyPricePoint, PyUserInfo};
use crate::engine::CalcEngine;
use crate::errors::{DataLoadError, calce_err_to_py};
use crate::services::{MarketData, UserData};

#[pyclass]
pub struct DataService {
    instruments: Vec<InstrumentSummary>,
    users: Vec<UserSummary>,
    market_data: Py<MarketData>,
    user_data: Py<UserData>,
    instrument_count: i64,
    price_count: i64,
    fx_rate_count: i64,
    user_count: i64,
    trade_count: i64,
    organization_count: i64,
}

#[pymethods]
impl DataService {
    #[new]
    fn new(py: Python<'_>, database_url: &str) -> PyResult<Self> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| DataLoadError::new_err(format!("Failed to create async runtime: {e}")))?;

        let (market_store, user_store) = rt
            .block_on(async {
                let pool = calce_data::config::create_pool(Some(database_url)).await?;
                calce_data::loader::load_from_postgres(&pool).await
            })
            .map_err(|e| DataLoadError::new_err(format!("Failed to load data: {e}")))?;

        let users = user_store.list_users(&SecurityContext::system());
        let user_count = user_store.user_count();
        let trade_count = user_store.trade_count();
        let organization_count = user_store.organization_count();

        let instrument_count = market_store.instrument_count();
        let price_count = market_store.price_count();
        let fx_rate_count = market_store.fx_rate_count();

        let (md_inner, instruments) = market_store.into_parts();

        let market_data = Py::new(py, MarketData { inner: md_inner })?;
        let user_data = Py::new(py, UserData { inner: user_store })?;

        Ok(Self {
            instruments,
            users,
            market_data,
            user_data,
            instrument_count,
            price_count,
            fx_rate_count,
            user_count,
            trade_count,
            organization_count,
        })
    }

    fn list_users(&self) -> Vec<PyUserInfo> {
        self.users.iter().map(PyUserInfo::from).collect()
    }

    fn search_instruments(&self, query: &str) -> Vec<PyInstrumentInfo> {
        let q = query.to_lowercase();
        self.instruments
            .iter()
            .filter(|i| {
                i.id.to_lowercase().contains(&q)
                    || i.name
                        .as_deref()
                        .is_some_and(|n| n.to_lowercase().contains(&q))
                    || i.instrument_type.to_lowercase().contains(&q)
            })
            .map(PyInstrumentInfo::from)
            .collect()
    }

    fn get_price_history(
        &self,
        py: Python<'_>,
        instrument_id: &str,
        from_date: NaiveDate,
        to_date: NaiveDate,
    ) -> PyResult<Vec<PyPricePoint>> {
        let md = self.market_data.borrow(py);
        let iid = InstrumentId::new(instrument_id);
        let history = md
            .inner
            .get_price_history(&iid, from_date, to_date)
            .map_err(calce_err_to_py)?;
        Ok(history
            .into_iter()
            .map(|(date, price)| PyPricePoint {
                date,
                price: price.value(),
            })
            .collect())
    }

    fn data_stats(&self) -> PyDataStats {
        PyDataStats {
            user_count: self.user_count,
            organization_count: self.organization_count,
            instrument_count: self.instrument_count,
            trade_count: self.trade_count,
            price_count: self.price_count,
            fx_rate_count: self.fx_rate_count,
        }
    }

    #[pyo3(signature = (user_id, base_currency, as_of_date))]
    fn engine(
        &self,
        py: Python<'_>,
        user_id: &str,
        base_currency: &str,
        as_of_date: NaiveDate,
    ) -> PyResult<CalcEngine> {
        let currency = Currency::try_new(base_currency)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let currency_py = crate::domain::Currency { inner: currency };
        CalcEngine::create(
            py,
            &currency_py,
            as_of_date,
            user_id,
            self.market_data.clone_ref(py),
            self.user_data.clone_ref(py),
        )
    }
}
