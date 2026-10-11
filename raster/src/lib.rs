//! Raster types for the QGIS Rust port.
//!
//! Replaces GDAL raster functionality with pure-Rust implementations.
//! See ADR-0001 for the no-FFI policy decision.
//!
//! Scope (bead qgis-s4i.6, E1 Core geometry rebuild):
//! * [`RasterBlock`] — a block of pixel data with a given data type
//! * [`RasterDataType`] — the data type of raster pixels
//! * [`RasterLayer`] — raster layer metadata
//! * [`BandStatistics`] — per-band statistics
//!
//! Matches the QGIS `QgsRasterBlock` / `QgsRasterLayer` /
//! `QgsRasterBandStats` class hierarchy. Full GDAL parity (format
//! reading, reprojection, resampling) is deferred to later hardening
//! beads.

#![forbid(unsafe_code)]

pub mod block;
pub mod layer;
pub mod statistics;

pub use block::{RasterBlock, RasterDataType};
pub use layer::RasterLayer;
pub use statistics::BandStatistics;
