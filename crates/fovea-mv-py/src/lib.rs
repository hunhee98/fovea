//! PyO3 bindings for fovea-mv.
//!
//! Status: scaffolding. Real bindings land in MVP step 4.

use pyo3::prelude::*;

/// Returns the version of the underlying core.
#[pyfunction]
fn core_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pymodule]
fn _fovea_mv(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(core_version, m)?)?;
    Ok(())
}
