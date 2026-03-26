use pyo3::prelude::*;

mod data_service;
mod data_types;
mod domain;
mod engine;
mod errors;
mod results;
mod services;

#[pymodule]
fn calce(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Domain types
    m.add_class::<domain::Currency>()?;
    m.add_class::<domain::Money>()?;
    m.add_class::<domain::Trade>()?;

    // Services
    m.add_class::<services::MarketData>()?;
    m.add_class::<services::UserData>()?;

    // Engine
    m.add_class::<engine::CalcEngine>()?;

    // Data service
    m.add_class::<data_service::DataService>()?;
    m.add_class::<data_types::PyUserInfo>()?;
    m.add_class::<data_types::PyInstrumentInfo>()?;
    m.add_class::<data_types::PyPricePoint>()?;
    m.add_class::<data_types::PyDataStats>()?;

    // Result types
    m.add_class::<results::Warning>()?;
    m.add_class::<results::ValuedPosition>()?;
    m.add_class::<results::MarketValueResult>()?;
    m.add_class::<results::ValueChange>()?;
    m.add_class::<results::ValueChangeSummary>()?;
    m.add_class::<results::TypeAllocationEntry>()?;
    m.add_class::<results::TypeAllocation>()?;
    m.add_class::<results::AllocationEntry>()?;
    m.add_class::<results::AllocationResult>()?;
    m.add_class::<results::PortfolioReport>()?;
    m.add_class::<results::VolatilityResult>()?;

    // Exceptions
    errors::register(m)?;

    Ok(())
}
