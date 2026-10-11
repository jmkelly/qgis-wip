//! Mesh types for the QGIS Rust port.
//!
//! Replaces MDAL mesh functionality with pure-Rust implementations.
//! See ADR-0001 for the no-FFI policy decision.
//!
//! Scope (bead qgis-s4i.6, E1 Core geometry rebuild):
//! * [`MeshLayer`] — a mesh layer with frames and datasets
//! * [`MeshFrame`] — a mesh frame at a time step
//! * [`MeshDataset`] — a dataset within a frame
//! * [`MeshFace`], [`MeshEdge`], [`MeshVertex`] — mesh elements
//!
//! Matches the QGIS `QgsMeshLayer` / `QgsMeshDataset` /
//! `QgsMeshFrame` class hierarchy. Full MDAL parity (format
//! reading, interpolation) is deferred to later hardening beads.

#![forbid(unsafe_code)]

pub mod dataset;
pub mod edge;
pub mod face;
pub mod frame;
pub mod layer;
pub mod vertex;

pub use dataset::MeshDataset;
pub use edge::MeshEdge;
pub use face::MeshFace;
pub use frame::MeshFrame;
pub use layer::MeshLayer;
pub use vertex::MeshVertex;
