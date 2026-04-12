use chrono::NaiveDate;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use calce_core::domain::account::AccountId;
use calce_core::domain::instrument::InstrumentId;
use calce_core::domain::price::Price;
use calce_core::domain::quantity::Quantity;
use calce_core::domain::user::UserId;

/// ISO 4217 currency code (e.g. "USD", "SEK").
#[pyclass(frozen, hash, eq)]
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Currency {
    pub inner: calce_core::domain::currency::Currency,
}

#[pymethods]
impl Currency {
    #[new]
    fn new(code: &str) -> PyResult<Self> {
        calce_core::domain::currency::Currency::try_new(code)
            .map(|inner| Currency { inner })
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    #[getter]
    fn code(&self) -> &str {
        self.inner.as_str()
    }

    fn __repr__(&self) -> String {
        format!("Currency(\"{}\")", self.inner.as_str())
    }

    fn __str__(&self) -> &str {
        self.inner.as_str()
    }
}

/// An amount of money in a specific currency.
#[pyclass(frozen)]
#[derive(Clone)]
pub struct Money {
    pub inner: calce_core::domain::money::Money,
}

#[pymethods]
impl Money {
    #[new]
    fn new(amount: f64, currency: &Currency) -> Self {
        Money {
            inner: calce_core::domain::money::Money::new(amount, currency.inner),
        }
    }

    #[getter]
    fn amount(&self) -> f64 {
        self.inner.amount
    }

    #[getter]
    fn currency(&self) -> Currency {
        Currency {
            inner: self.inner.currency,
        }
    }

    fn __repr__(&self) -> String {
        format!("Money({}, \"{}\")", self.inner.amount, self.inner.currency)
    }

    fn __str__(&self) -> String {
        format!("{} {}", self.inner.amount, self.inner.currency)
    }
}

/// A single trade execution.
#[pyclass(frozen)]
#[derive(Clone)]
pub struct Trade {
    pub inner: calce_core::domain::trade::Trade,
}

#[pymethods]
impl Trade {
    #[new]
    #[pyo3(signature = (user_id, account_id, instrument_id, quantity, price, currency, date))]
    fn new(
        user_id: &str,
        account_id: i64,
        instrument_id: &str,
        quantity: f64,
        price: f64,
        currency: &Currency,
        date: NaiveDate,
    ) -> Self {
        Trade {
            inner: calce_core::domain::trade::Trade {
                id: None,
                user_id: UserId::new(user_id),
                account_id: AccountId::new(account_id),
                instrument_id: InstrumentId::new(instrument_id),
                quantity: Quantity::new(quantity),
                price: Price::new(price),
                currency: currency.inner,
                date,
            },
        }
    }

    #[getter]
    fn id(&self) -> Option<i64> {
        self.inner.id.map(calce_core::domain::trade::TradeId::value)
    }

    #[getter]
    fn user_id(&self) -> &str {
        self.inner.user_id.as_str()
    }

    #[getter]
    fn account_id(&self) -> i64 {
        self.inner.account_id.value()
    }

    #[getter]
    fn instrument_id(&self) -> &str {
        self.inner.instrument_id.as_str()
    }

    #[getter]
    fn quantity(&self) -> f64 {
        self.inner.quantity.value()
    }

    #[getter]
    fn price(&self) -> f64 {
        self.inner.price.value()
    }

    #[getter]
    fn currency(&self) -> Currency {
        Currency {
            inner: self.inner.currency,
        }
    }

    #[getter]
    fn date(&self) -> NaiveDate {
        self.inner.date
    }

    fn __repr__(&self) -> String {
        let id_part = self
            .inner
            .id
            .map_or(String::new(), |id| format!("id={id}, "));
        format!(
            "Trade({id_part}user_id=\"{}\", instrument_id=\"{}\", quantity={}, price={}, currency=\"{}\", date={})",
            self.inner.user_id,
            self.inner.instrument_id,
            self.inner.quantity.value(),
            self.inner.price.value(),
            self.inner.currency,
            self.inner.date,
        )
    }
}
