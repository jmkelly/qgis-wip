//! Mesh vertex: a 3D point in a mesh.
//!
//! Matches QGIS `QgsMeshVertex` which represents a single vertex
//! in a mesh with x, y, z coordinates.

use qgis_geometry::Point;

/// A single vertex in a mesh.
///
/// Matches QGIS `QgsMeshVertex`. Stores x, y, z coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshVertex {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
    /// Z coordinate (elevation / value).
    pub z: f64,
}

impl MeshVertex {
    /// Creates a new mesh vertex.
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Returns the x coordinate.
    pub fn x(&self) -> f64 {
        self.x
    }

    /// Returns the y coordinate.
    pub fn y(&self) -> f64 {
        self.y
    }

    /// Returns the z coordinate.
    pub fn z(&self) -> f64 {
        self.z
    }

    /// Returns the 2D distance to another vertex (ignoring z).
    pub fn distance_2d(&self, other: &MeshVertex) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    /// Returns the 3D distance to another vertex.
    pub fn distance_3d(&self, other: &MeshVertex) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// Converts to a 2D `Point` (x, y only).
    pub fn to_point_2d(&self) -> Point {
        Point::new(self.x, self.y)
    }

    /// Converts to a 3D `Point` (x, y, z).
    pub fn to_point_3d(&self) -> Point {
        Point::new_z(self.x, self.y, self.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_vertex_new() {
        let v = MeshVertex::new(1.0, 2.0, 3.0);
        assert_eq!(v.x(), 1.0);
        assert_eq!(v.y(), 2.0);
        assert_eq!(v.z(), 3.0);
    }

    #[test]
    fn mesh_vertex_distance_2d() {
        let a = MeshVertex::new(0.0, 0.0, 100.0);
        let b = MeshVertex::new(3.0, 4.0, 200.0);
        assert_eq!(a.distance_2d(&b), 5.0);
    }

    #[test]
    fn mesh_vertex_distance_3d() {
        let a = MeshVertex::new(0.0, 0.0, 0.0);
        let b = MeshVertex::new(1.0, 1.0, 1.0);
        let expected = (3.0_f64).sqrt();
        assert!((a.distance_3d(&b) - expected).abs() < 1e-10);
    }

    #[test]
    fn mesh_vertex_to_point() {
        let v = MeshVertex::new(1.0, 2.0, 3.0);
        let p2d = v.to_point_2d();
        assert_eq!(p2d.x(), 1.0);
        assert_eq!(p2d.y(), 2.0);
        assert!(!p2d.has_z());

        let p3d = v.to_point_3d();
        assert_eq!(p3d.x(), 1.0);
        assert_eq!(p3d.y(), 2.0);
        assert_eq!(p3d.z(), Some(3.0));
    }
}
