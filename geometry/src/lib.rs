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
//! Scope (bead qgis-s4i.3, predicates and overlay via geo):
//! * Spatial predicates: contains, intersects, within, crosses, touches,
//!   overlaps, disjoint, equals
//! * Overlay operations: union, intersection, difference, symmetric_difference
//! * Buffer and simplify operations
//!
//! Matches the QGIS `QgsGeometry` / `QgsPoint` / `QgsLineString` /
//! `QgsPolygon` / `QgsMulti*` class hierarchy. Full GEOS parity (robust
//! predicates, overlay operations) is deferred to later hardening beads.

#![forbid(unsafe_code)]

mod buffer;
mod collection;
mod geo_bridge;
mod linestring;
mod multi;
mod overlay;
mod point;
mod polygon;
mod validity;
mod predicates;
mod simplify;

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

// Validity
pub use validity::ValidityError;

// Spatial predicates
pub use predicates::{contains, crosses, disjoint, equals, intersects, overlaps, touches, within};

// Overlay operations
pub use overlay::{difference, intersection, symmetric_difference, union};

// Buffer and simplify
pub use buffer::buffer;
pub use simplify::{simplify, simplify_vw};
