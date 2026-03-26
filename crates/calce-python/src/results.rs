use chrono::NaiveDate;
use pyo3::prelude::*;

use crate::domain::{Currency, Money};

// ── Warning ─────────────────────────────────────────────────────────

#[pyclass(frozen)]
pub struct Warning {
    pub inner: calce_core::outcome::Warning,
}

#[pymethods]
impl Warning {
    #[getter]
    fn code(&self) -> &str {
        match self.inner.code {
            calce_core::outcome::WarningCode::MissingPrice => "MISSING_PRICE",
            calce_core::outcome::WarningCode::MissingFxRate => "MISSING_FX_RATE",
        }
    }

    #[getter]
    fn message(&self) -> &str {
        &self.inner.message
    }

    fn __repr__(&self) -> String {
        format!(
            "Warning(code=\"{}\", message=\"{}\")",
            self.code(),
            self.inner.message
        )
    }
}

#[pyclass(frozen)]
pub struct ValuedPosition {
    pub inner: calce_core::calc::market_value::ValuedPosition,
}

#[pymethods]
impl ValuedPosition {
    #[getter]
    fn instrument_id(&self) -> &str {
        self.inner.instrument_id.as_str()
    }

    #[getter]
    fn quantity(&self) -> f64 {
        self.inner.quantity.value()
    }

    #[getter]
    fn currency(&self) -> Currency {
        Currency {
            inner: self.inner.currency,
        }
    }

    #[getter]
    fn price(&self) -> f64 {
        self.inner.price.value()
    }

    #[getter]
    fn market_value(&self) -> Money {
        Money {
            inner: self.inner.market_value,
        }
    }

    #[getter]
    fn market_value_base(&self) -> Money {
        Money {
            inner: self.inner.market_value_base,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "ValuedPosition(instrument_id=\"{}\", quantity={}, market_value_base={})",
            self.inner.instrument_id,
            self.inner.quantity.value(),
            self.inner.market_value_base,
        )
    }
}

#[pyclass(frozen)]
pub struct MarketValueResult {
    pub inner: calce_core::calc::market_value::MarketValueResult,
    pub warnings: Vec<calce_core::outcome::Warning>,
}

#[pymethods]
impl MarketValueResult {
    #[getter]
    fn positions(&self) -> Vec<ValuedPosition> {
        self.inner
            .positions
            .iter()
            .map(|p| ValuedPosition { inner: p.clone() })
            .collect()
    }

    #[getter]
    fn total(&self) -> Money {
        Money {
            inner: self.inner.total,
        }
    }

    #[getter]
    fn warnings(&self) -> Vec<Warning> {
        self.warnings
            .iter()
            .map(|w| Warning { inner: w.clone() })
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "MarketValueResult(total={}, positions={}, warnings={})",
            self.inner.total,
            self.inner.positions.len(),
            self.warnings.len(),
        )
    }
}

#[pyclass(frozen)]
pub struct ValueChange {
    pub inner: calce_core::calc::value_change::ValueChange,
}

#[pymethods]
impl ValueChange {
    #[getter]
    fn current(&self) -> Money {
        Money {
            inner: self.inner.current,
        }
    }

    #[getter]
    fn previous(&self) -> Money {
        Money {
            inner: self.inner.previous,
        }
    }

    #[getter]
    fn change(&self) -> Money {
        Money {
            inner: self.inner.change,
        }
    }

    #[getter]
    fn change_pct(&self) -> Option<f64> {
        self.inner.change_pct
    }

    fn __repr__(&self) -> String {
        let pct = self
            .inner
            .change_pct
            .map_or("None".to_string(), |p| format!("{p:.4}"));
        format!(
            "ValueChange(change={}, change_pct={})",
            self.inner.change, pct,
        )
    }
}

#[pyclass(frozen)]
pub struct ValueChangeSummary {
    pub inner: calce_core::calc::value_change::ValueChangeSummary,
}

#[pymethods]
impl ValueChangeSummary {
    #[getter]
    fn market_value(&self) -> Money {
        Money {
            inner: self.inner.market_value,
        }
    }

    #[getter]
    fn daily(&self) -> ValueChange {
        ValueChange {
            inner: calce_core::calc::value_change::ValueChange {
                current: self.inner.daily.current,
                previous: self.inner.daily.previous,
                change: self.inner.daily.change,
                change_pct: self.inner.daily.change_pct,
            },
        }
    }

    #[getter]
    fn weekly(&self) -> ValueChange {
        ValueChange {
            inner: calce_core::calc::value_change::ValueChange {
                current: self.inner.weekly.current,
                previous: self.inner.weekly.previous,
                change: self.inner.weekly.change,
                change_pct: self.inner.weekly.change_pct,
            },
        }
    }

    #[getter]
    fn yearly(&self) -> ValueChange {
        ValueChange {
            inner: calce_core::calc::value_change::ValueChange {
                current: self.inner.yearly.current,
                previous: self.inner.yearly.previous,
                change: self.inner.yearly.change,
                change_pct: self.inner.yearly.change_pct,
            },
        }
    }

    #[getter]
    fn ytd(&self) -> ValueChange {
        ValueChange {
            inner: calce_core::calc::value_change::ValueChange {
                current: self.inner.ytd.current,
                previous: self.inner.ytd.previous,
                change: self.inner.ytd.change,
                change_pct: self.inner.ytd.change_pct,
            },
        }
    }
}

#[pyclass(frozen)]
pub struct TypeAllocationEntry {
    pub inner: calce_core::calc::allocation::TypeAllocationEntry,
}

#[pymethods]
impl TypeAllocationEntry {
    #[getter]
    fn instrument_type(&self) -> &str {
        self.inner.instrument_type.as_str()
    }

    #[getter]
    fn market_value(&self) -> Money {
        Money {
            inner: self.inner.market_value,
        }
    }

    #[getter]
    fn weight(&self) -> f64 {
        self.inner.weight
    }

    fn __repr__(&self) -> String {
        format!(
            "TypeAllocationEntry(type=\"{}\", value={}, weight={:.4})",
            self.inner.instrument_type, self.inner.market_value, self.inner.weight,
        )
    }
}

#[pyclass(frozen)]
pub struct TypeAllocation {
    pub inner: calce_core::calc::allocation::TypeAllocation,
}

#[pymethods]
impl TypeAllocation {
    #[getter]
    fn entries(&self) -> Vec<TypeAllocationEntry> {
        self.inner
            .entries
            .iter()
            .map(|e| TypeAllocationEntry { inner: e.clone() })
            .collect()
    }

    #[getter]
    fn total(&self) -> Money {
        Money {
            inner: self.inner.total,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "TypeAllocation(entries={}, total={})",
            self.inner.entries.len(),
            self.inner.total,
        )
    }
}

#[pyclass(frozen)]
pub struct AllocationEntry {
    pub inner: calce_core::calc::allocation::AllocationEntry,
}

#[pymethods]
impl AllocationEntry {
    #[getter]
    fn key(&self) -> &str {
        &self.inner.key
    }

    #[getter]
    fn market_value(&self) -> Money {
        Money {
            inner: self.inner.market_value,
        }
    }

    #[getter]
    fn weight(&self) -> f64 {
        self.inner.weight
    }

    fn __repr__(&self) -> String {
        format!(
            "AllocationEntry(key=\"{}\", value={}, weight={:.4})",
            self.inner.key, self.inner.market_value, self.inner.weight,
        )
    }
}

#[pyclass(frozen)]
pub struct AllocationResult {
    pub inner: calce_core::calc::allocation::AllocationResult,
}

#[pymethods]
impl AllocationResult {
    #[getter]
    fn dimension(&self) -> &str {
        &self.inner.dimension
    }

    #[getter]
    fn entries(&self) -> Vec<AllocationEntry> {
        self.inner
            .entries
            .iter()
            .map(|e| AllocationEntry { inner: e.clone() })
            .collect()
    }

    #[getter]
    fn total(&self) -> Money {
        Money {
            inner: self.inner.total,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "AllocationResult(dimension=\"{}\", entries={}, total={})",
            self.inner.dimension,
            self.inner.entries.len(),
            self.inner.total,
        )
    }
}

#[pyclass(frozen)]
pub struct PortfolioReport {
    pub inner: calce_core::reports::portfolio::PortfolioReport,
    pub warnings: Vec<calce_core::outcome::Warning>,
}

#[pymethods]
impl PortfolioReport {
    #[getter]
    fn market_value(&self) -> MarketValueResult {
        MarketValueResult {
            inner: calce_core::calc::market_value::MarketValueResult {
                positions: self.inner.market_value.positions.clone(),
                total: self.inner.market_value.total,
            },
            warnings: Vec::new(),
        }
    }

    #[getter]
    fn value_changes(&self) -> ValueChangeSummary {
        ValueChangeSummary {
            inner: calce_core::calc::value_change::ValueChangeSummary {
                market_value: self.inner.value_changes.market_value,
                daily: calce_core::calc::value_change::ValueChange {
                    current: self.inner.value_changes.daily.current,
                    previous: self.inner.value_changes.daily.previous,
                    change: self.inner.value_changes.daily.change,
                    change_pct: self.inner.value_changes.daily.change_pct,
                },
                weekly: calce_core::calc::value_change::ValueChange {
                    current: self.inner.value_changes.weekly.current,
                    previous: self.inner.value_changes.weekly.previous,
                    change: self.inner.value_changes.weekly.change,
                    change_pct: self.inner.value_changes.weekly.change_pct,
                },
                yearly: calce_core::calc::value_change::ValueChange {
                    current: self.inner.value_changes.yearly.current,
                    previous: self.inner.value_changes.yearly.previous,
                    change: self.inner.value_changes.yearly.change,
                    change_pct: self.inner.value_changes.yearly.change_pct,
                },
                ytd: calce_core::calc::value_change::ValueChange {
                    current: self.inner.value_changes.ytd.current,
                    previous: self.inner.value_changes.ytd.previous,
                    change: self.inner.value_changes.ytd.change,
                    change_pct: self.inner.value_changes.ytd.change_pct,
                },
            },
        }
    }

    #[getter]
    fn type_allocation(&self) -> TypeAllocation {
        TypeAllocation {
            inner: self.inner.type_allocation.clone(),
        }
    }

    #[getter]
    fn sector_allocation(&self) -> AllocationResult {
        AllocationResult {
            inner: self.inner.sector_allocation.clone(),
        }
    }

    #[getter]
    fn warnings(&self) -> Vec<Warning> {
        self.warnings
            .iter()
            .map(|w| Warning { inner: w.clone() })
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "PortfolioReport(market_value={}, positions={}, warnings={})",
            self.inner.market_value.total,
            self.inner.market_value.positions.len(),
            self.warnings.len(),
        )
    }
}

#[pyclass(frozen)]
pub struct VolatilityResult {
    pub inner: calce_core::calc::volatility::VolatilityResult,
}

#[pymethods]
impl VolatilityResult {
    #[getter]
    fn annualized_volatility(&self) -> f64 {
        self.inner.annualized_volatility
    }

    #[getter]
    fn daily_volatility(&self) -> f64 {
        self.inner.daily_volatility
    }

    #[getter]
    fn num_observations(&self) -> usize {
        self.inner.num_observations
    }

    #[getter]
    fn start_date(&self) -> NaiveDate {
        self.inner.start_date
    }

    #[getter]
    fn end_date(&self) -> NaiveDate {
        self.inner.end_date
    }

    fn __repr__(&self) -> String {
        format!(
            "VolatilityResult(annualized={:.4}, daily={:.6}, obs={}, {}..{})",
            self.inner.annualized_volatility,
            self.inner.daily_volatility,
            self.inner.num_observations,
            self.inner.start_date,
            self.inner.end_date,
        )
    }
}
