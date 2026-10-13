//! WCS (Web Coverage Service) request handlers.
//!
//! Implements WCS 2.0.1 GetCapabilities, DescribeCoverage, and GetCoverage.
//! Coverages are raster layers published from a QGIS project; GetCoverage
//! renders the coverage extent through the shared map renderer.
//!
//! Reference: OGC 09-025r2 WCS 2.0.1, `src/server/services/qgscs*.cpp`

pub mod get_coverage;
pub mod get_capabilities;

pub use get_capabilities::{capabilities_document, handle_get_capabilities};
pub use get_coverage::{
    GetCoverageParams, SUPPORTED_COVERAGE_FORMATS, handle_describe_coverage,
    handle_get_coverage, render_coverage,
};