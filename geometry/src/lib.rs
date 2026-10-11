//! Geometry primitives for the QGIS Rust port.
//!
//! Replaces GEOS functionality with pure-Rust implementations.
//! See ADR-0001 for the no-FFI policy decision.
//!
//! Scope (bead qgis-s4i.1, E1 Core geometry rebuild):
//! * [`Point`] with Z/M support (2D, 3D, M, ZM)
//! * [`LineString`] with Z/M support
//! * [`Polygon`] with Z/M support
//! * [`MultiPoint`], [`MultiLineString`], [`MultiPolygon`]
//! * [`GeometryCollection`] and the [`Geometry`] enum
//!
//! Matches the QGIS `QgsGeometry` / `QgsPoint` / `QgsLineString` /
//! `QgsPolygon` / `QgsMulti*` class hierarchy. Full GEOS parity (robust
//! predicates, overlay operations) is deferred to later hardening beads.

#![forbid(unsafe_code)]

mod collection;
mod linestring;
mod multi;
mod point;
mod polygon;

// Point types
pub use point::{Dimension, Point};

// LineString
pub use linestring::LineString;

// Polygon
pub use polygon::{Polygon, PolygonError};

// Multi-geometry types
pub use multi::{MultiLineString, MultiPoint, MultiPolygon};

// GeometryCollection and Geometry enum
pub use collection::{Geometry, GeometryCollection};
