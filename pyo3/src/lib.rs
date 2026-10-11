//! PyO3 Python bindings for the QGIS Rust port.
//!
//! Serves identical `qgis.core` / `qgis.gui` import paths from new PyO3 bindings,
//! replacing SIP/PyQt6. See ADR-0003 for the compatibility decision.

use pyo3::prelude::*;

/// A minimal Python module for the QGIS core bindings.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {
        // PyO3 requires a Python interpreter to exercise the bindings;
        // this placeholder keeps `cargo test` meaningful until then.
    }
}
