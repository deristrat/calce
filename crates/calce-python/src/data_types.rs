use chrono::NaiveDate;
use pyo3::prelude::*;

use calce_data::market_data_store::InstrumentSummary;
use calce_data::user_data_store::UserSummary;

#[pyclass(frozen, name = "UserInfo")]
pub struct PyUserInfo {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub email: Option<String>,
    #[pyo3(get)]
    pub name: Option<String>,
    #[pyo3(get)]
    pub organization_id: Option<String>,
    #[pyo3(get)]
    pub organization_name: Option<String>,
    #[pyo3(get)]
    pub trade_count: i64,
    #[pyo3(get)]
    pub account_count: i64,
}

impl From<&UserSummary> for PyUserInfo {
    fn from(u: &UserSummary) -> Self {
        Self {
            id: u.id.clone(),
            email: u.email.clone(),
            name: u.name.clone(),
            organization_id: u.organization_id.clone(),
            organization_name: u.organization_name.clone(),
            trade_count: u.trade_count,
            account_count: u.account_count,
        }
    }
}

#[pymethods]
impl PyUserInfo {
    fn __repr__(&self) -> String {
        let email = self
            .email
            .as_deref()
            .map_or(String::new(), |e| format!(", email={e:?}"));
        format!(
            "UserInfo(id={:?}{}, trades={})",
            self.id, email, self.trade_count
        )
    }
}

#[pyclass(frozen, name = "InstrumentInfo")]
pub struct PyInstrumentInfo {
    #[pyo3(get)]
    pub id: i64,
    #[pyo3(get)]
    pub ticker: String,
    #[pyo3(get)]
    pub currency: String,
    #[pyo3(get)]
    pub name: Option<String>,
    #[pyo3(get)]
    pub instrument_type: String,
}

impl From<&InstrumentSummary> for PyInstrumentInfo {
    fn from(i: &InstrumentSummary) -> Self {
        Self {
            id: i.id,
            ticker: i.ticker.clone(),
            currency: i.currency.clone(),
            name: i.name.clone(),
            instrument_type: i.instrument_type.clone(),
        }
    }
}

#[pymethods]
impl PyInstrumentInfo {
    fn __repr__(&self) -> String {
        let name = self
            .name
            .as_deref()
            .map_or(String::new(), |n| format!(", name={n:?}"));
        format!(
            "InstrumentInfo(id={}, ticker={:?}{}, type={:?}, currency={:?})",
            self.id, self.ticker, name, self.instrument_type, self.currency
        )
    }
}

#[pyclass(frozen, name = "PricePoint")]
pub struct PyPricePoint {
    #[pyo3(get)]
    pub date: NaiveDate,
    #[pyo3(get)]
    pub price: f64,
}

#[pymethods]
impl PyPricePoint {
    fn __repr__(&self) -> String {
        format!("PricePoint({}, {:.2})", self.date, self.price)
    }
}

#[allow(clippy::struct_field_names)] // Python API — *_count is clear for consumers
#[pyclass(frozen, name = "DataStats")]
pub struct PyDataStats {
    #[pyo3(get)]
    pub user_count: i64,
    #[pyo3(get)]
    pub organization_count: i64,
    #[pyo3(get)]
    pub instrument_count: i64,
    #[pyo3(get)]
    pub trade_count: i64,
    #[pyo3(get)]
    pub price_count: i64,
    #[pyo3(get)]
    pub fx_rate_count: i64,
}

#[pymethods]
impl PyDataStats {
    fn __repr__(&self) -> String {
        format!(
            "DataStats(users={}, instruments={}, trades={}, prices={}, fx_rates={})",
            self.user_count,
            self.instrument_count,
            self.trade_count,
            self.price_count,
            self.fx_rate_count,
        )
    }
}
