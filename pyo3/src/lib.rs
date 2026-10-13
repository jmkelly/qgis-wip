//! PyO3 Python bindings for the QGIS Rust port.
//!
//! Serves identical `qgis.core` / `qgis.gui` / `qgis._3d` import paths from new
//! PyO3 bindings, replacing SIP/PyQt6. See ADR-0003 for the compatibility
//! decision.
//!
//! Each submodule owns the logic for one Python package and exposes it through
//! plain Rust state structs, so the behaviour can be unit tested without an
//! embedded interpreter.

pub mod gui;
pub mod processing;
pub mod three_d;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// Adds the binding version string to a submodule.
fn add_version(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))
}

/// `qgis.core` bindings.
#[pymodule]
pub fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    add_version(m)
}

/// Registers a submodule under the `qgis` package namespace.
///
/// The submodule is also inserted into `sys.modules` under its dotted name, and
/// the parent is marked as a package, so `import qgis.gui` resolves the same way
/// it does against the SIP-built extension.
fn add_qgis_submodule<'py>(
    parent: &Bound<'py, PyModule>,
    name: &str,
    init: impl FnOnce(&Bound<'py, PyModule>) -> PyResult<()>,
) -> PyResult<()> {
    let sub = PyModule::new(parent.py(), name)?;
    init(&sub)?;
    parent.add_submodule(&sub)?;
    sys_modules(parent.py())?.set_item(name, &sub)?;
    Ok(())
}

/// The interpreter's `sys.modules` mapping.
fn sys_modules(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    Ok(PyModule::import(py, "sys")?
        .getattr("modules")?
        .cast_into()?)
}

/// The `qgis` package root; preserves the historical import paths.
#[pymodule]
pub fn qgis(m: &Bound<'_, PyModule>) -> PyResult<()> {
    add_version(m)?;
    // An empty `__path__` marks this as a package without adding a filesystem
    // location to search.
    m.setattr("__path__", PyList::empty(m.py()))?;
    add_qgis_submodule(m, "qgis.core", _core)?;
    add_qgis_submodule(m, "qgis.gui", gui::gui)?;
    add_qgis_submodule(m, "qgis._3d", three_d::three_d)?;
    add_qgis_submodule(m, "qgis.processing", processing::_processing)?;
    sys_modules(m.py())?.set_item("qgis", m)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_bindings_are_constructible() {
        assert_eq!(gui::MapCanvas::default().layer_count(), 0);
        assert!(gui::MessageBar::default().items().is_empty());
        assert!(gui::ProjectionSelectionWidget::default().crs_valid());
        assert!(!gui::LayerTreeMapCanvasBridge::default().has_canvas());
    }

    #[test]
    fn three_d_bindings_are_constructible() {
        assert_eq!(three_d::Map3DMapSettings::new().crs(), "EPSG:4326");
        assert_eq!(three_d::Map3DScene::new().update_count(), 0);
        assert_eq!(three_d::PlaneAlignment::Vertical.as_i64(), 1);
        assert_eq!(three_d::OriginType::Relative.as_i64(), 1);
        assert!(three_d::Renderer3D::new("QgsVectorLayer3DRenderer").is_valid());
    }
}