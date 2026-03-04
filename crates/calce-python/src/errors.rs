use pyo3::exceptions::PyException;
use pyo3::prelude::*;

pyo3::create_exception!(calce, CalceError, PyException);
pyo3::create_exception!(calce, UnauthorizedError, CalceError);
pyo3::create_exception!(calce, PriceNotFoundError, CalceError);
pyo3::create_exception!(calce, FxRateNotFoundError, CalceError);
pyo3::create_exception!(calce, NoTradesFoundError, CalceError);
pyo3::create_exception!(calce, CurrencyMismatchError, CalceError);
pyo3::create_exception!(calce, InsufficientDataError, CalceError);

pub fn calce_err_to_py(err: calce_core::error::CalceError) -> PyErr {
    use calce_core::error::CalceError as E;
    match err {
        E::Unauthorized { .. } => UnauthorizedError::new_err(err.to_string()),
        E::PriceNotFound { .. } => PriceNotFoundError::new_err(err.to_string()),
        E::FxRateNotFound { .. } => FxRateNotFoundError::new_err(err.to_string()),
        E::NoTradesFound(_) => NoTradesFoundError::new_err(err.to_string()),
        E::CurrencyMismatch(_) => CurrencyMismatchError::new_err(err.to_string()),
        E::InsufficientData { .. } => InsufficientDataError::new_err(err.to_string()),
    }
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add("CalceError", parent.py().get_type::<CalceError>())?;
    parent.add("UnauthorizedError", parent.py().get_type::<UnauthorizedError>())?;
    parent.add("PriceNotFoundError", parent.py().get_type::<PriceNotFoundError>())?;
    parent.add("FxRateNotFoundError", parent.py().get_type::<FxRateNotFoundError>())?;
    parent.add("NoTradesFoundError", parent.py().get_type::<NoTradesFoundError>())?;
    parent.add("CurrencyMismatchError", parent.py().get_type::<CurrencyMismatchError>())?;
    parent.add("InsufficientDataError", parent.py().get_type::<InsufficientDataError>())?;
    Ok(())
}
