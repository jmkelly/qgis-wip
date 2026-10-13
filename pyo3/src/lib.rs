//! PyO3 Python bindings for the QGIS Rust port.

use pyo3::prelude::*;

fn add_simple_class<T: pyo3::pyclass::PyClass>(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<T>()?;
    Ok(())
}

macro_rules! py_empty {
    ($name:ident) => {
        #[pyclass(skip_from_py_object)]
        #[derive(Clone, Default)]
        pub struct $name {}
        #[pymethods]
        impl $name {
            #[new]
            fn new() -> Self {
                Self::default()
            }
        }
    };
}

py_empty!(QgsGui);
py_empty!(QgsMapCanvas);
py_empty!(QgsLayerTreeMapCanvasBridge);
py_empty!(QgsMessageBar);
py_empty!(QgsProjectionSelectionWidget);

py_empty!(Qgs3DMapScene);
py_empty!(Qgs3DMapSettings);
py_empty!(Qgs3DTypes);
py_empty!(Qgs3DRenderer);

#[pymodule]
fn gui(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    add_simple_class::<QgsGui>(m)?;
    add_simple_class::<QgsMapCanvas>(m)?;
    add_simple_class::<QgsLayerTreeMapCanvasBridge>(m)?;
    add_simple_class::<QgsMessageBar>(m)?;
    add_simple_class::<QgsProjectionSelectionWidget>(m)?;
    Ok(())
}

#[pymodule]
fn _3d(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    add_simple_class::<Qgs3DMapScene>(m)?;
    add_simple_class::<Qgs3DMapSettings>(m)?;
    add_simple_class::<Qgs3DTypes>(m)?;
    add_simple_class::<Qgs3DRenderer>(m)?;
    Ok(())
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

#[pymodule]
fn qgis(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    let core_mod = PyModule::new(m.py(), "qgis.core")?;
    _core(&core_mod)?;
    m.add_submodule(&core_mod)?;
    let gui_mod = PyModule::new(m.py(), "qgis.gui")?;
    gui(&gui_mod)?;
    m.add_submodule(&gui_mod)?;
    let d3_mod = PyModule::new(m.py(), "qgis._3d")?;
    _3d(&d3_mod)?;
    m.add_submodule(&d3_mod)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {}
}
