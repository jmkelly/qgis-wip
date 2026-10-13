//! PyO3 bindings for the `qgis.processing` module.
//!
//! Placeholder classes only; the real processing plugin path is tracked
//! separately. Kept here so the `qgis.processing` import path continues to
//! resolve while SIP is retired.

use pyo3::prelude::*;

/// Declares a placeholder class with a no-argument constructor.
macro_rules! py_processing_placeholder {
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

py_processing_placeholder!(QgsProcessingRegistry);
py_processing_placeholder!(QgsProcessingAlgorithm);
py_processing_placeholder!(QgsProcessingParameterField);
py_processing_placeholder!(QgsProcessingParameterNumber);
py_processing_placeholder!(QgsProcessingParameterString);
py_processing_placeholder!(QgsProcessingParameterBoolean);
py_processing_placeholder!(QgsProcessingParameterExtent);
py_processing_placeholder!(QgsProcessingParameterCrs);
py_processing_placeholder!(QgsProcessingOutput);
py_processing_placeholder!(QgsProcessingContext);
py_processing_placeholder!(QgsProcessingFeedback);

/// `qgis.processing` bindings.
#[pymodule]
pub fn _processing(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<QgsProcessingRegistry>()?;
    m.add_class::<QgsProcessingAlgorithm>()?;
    m.add_class::<QgsProcessingParameterField>()?;
    m.add_class::<QgsProcessingParameterNumber>()?;
    m.add_class::<QgsProcessingParameterString>()?;
    m.add_class::<QgsProcessingParameterBoolean>()?;
    m.add_class::<QgsProcessingParameterExtent>()?;
    m.add_class::<QgsProcessingParameterCrs>()?;
    m.add_class::<QgsProcessingOutput>()?;
    m.add_class::<QgsProcessingContext>()?;
    m.add_class::<QgsProcessingFeedback>()?;
    Ok(())
}