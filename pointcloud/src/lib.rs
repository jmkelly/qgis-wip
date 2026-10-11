//! Point cloud types for the QGIS Rust port.
//!
//! Replaces PDAL point cloud functionality with pure-Rust implementations.
//! See ADR-0001 for the no-FFI policy decision.
//!
//! Scope (bead qgis-s4i.6, E1 Core geometry rebuild):
//! * [`PointCloudLayer`] — a point cloud layer
//! * [`PointCloudAttribute`] — LAS/LAZ point attributes
//! * [`PointCloudBlock`] — a block of point data
//! * [`OctreeNode`] — octree structure for spatial indexing
//! * [`LasPoint`] — a single LAS point
//!
//! Matches the QGIS `QgsPointCloudLayer` / `QgsPointCloudAttribute` /
//! `QgsPointCloudIndex` class hierarchy. Full PDAL/LASzip parity is
//! deferred to later hardening beads.

#![forbid(unsafe_code)]

pub mod attribute;
pub mod block;
pub mod las;
pub mod layer;
pub mod octree;

pub use attribute::{PointCloudAttribute, PointCloudAttributeType};
pub use block::PointCloudBlock;
pub use las::LasPoint;
pub use layer::PointCloudLayer;
pub use octree::{OctreeNode, OctreePosition};
