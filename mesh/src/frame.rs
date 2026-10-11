//! Mesh frame: a collection of datasets at a time step.
//!
//! Matches QGIS `QgsMeshFrame` which represents the mesh data at
//! a specific time step, containing vertices, edges, faces, and
//! associated datasets.

use crate::{MeshDataset, MeshEdge, MeshFace, MeshVertex};

/// A mesh frame at a specific time step.
///
/// Matches QGIS `QgsMeshFrame`. Contains the mesh geometry (vertices,
/// edges, faces) and a collection of datasets.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshFrame {
    /// Time value for this frame.
    pub time: f64,
    /// Vertices in the mesh.
    pub vertices: Vec<MeshVertex>,
    /// Edges in the mesh.
    pub edges: Vec<MeshEdge>,
    /// Faces in the mesh.
    pub faces: Vec<MeshFace>,
    /// Datasets associated with this frame.
    pub datasets: Vec<MeshDataset>,
}

impl MeshFrame {
    /// Creates a new empty mesh frame at the given time.
    pub fn new(time: f64) -> Self {
        Self {
            time,
            vertices: Vec::new(),
            edges: Vec::new(),
            faces: Vec::new(),
            datasets: Vec::new(),
        }
    }

    /// Creates a new mesh frame with vertices and faces.
    pub fn with_geometry(
        time: f64,
        vertices: Vec<MeshVertex>,
        edges: Vec<MeshEdge>,
        faces: Vec<MeshFace>,
    ) -> Self {
        Self {
            time,
            vertices,
            edges,
            faces,
            datasets: Vec::new(),
        }
    }

    /// Returns the time value for this frame.
    pub fn time(&self) -> f64 {
        self.time
    }

    /// Returns the number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Returns the number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Returns the number of faces.
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Returns the number of datasets.
    pub fn dataset_count(&self) -> usize {
        self.datasets.len()
    }

    /// Returns true if the frame has no geometry.
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty() && self.faces.is_empty()
    }

    /// Returns a reference to the vertices.
    pub fn vertices(&self) -> &[MeshVertex] {
        &self.vertices
    }

    /// Returns a reference to the edges.
    pub fn edges(&self) -> &[MeshEdge] {
        &self.edges
    }

    /// Returns a reference to the faces.
    pub fn faces(&self) -> &[MeshFace] {
        &self.faces
    }

    /// Returns a reference to the datasets.
    pub fn datasets(&self) -> &[MeshDataset] {
        &self.datasets
    }

    /// Adds a dataset to this frame.
    pub fn add_dataset(&mut self, dataset: MeshDataset) {
        self.datasets.push(dataset);
    }

    /// Returns the bounding box of the mesh frame as (min_x, min_y, max_x, max_y).
    pub fn extent(&self) -> Option<(f64, f64, f64, f64)> {
        if self.vertices.is_empty() {
            return None;
        }
        let mut min_x = self.vertices[0].x;
        let mut max_x = self.vertices[0].x;
        let mut min_y = self.vertices[0].y;
        let mut max_y = self.vertices[0].y;
        for v in &self.vertices {
            if v.x < min_x {
                min_x = v.x;
            }
            if v.x > max_x {
                max_x = v.x;
            }
            if v.y < min_y {
                min_y = v.y;
            }
            if v.y > max_y {
                max_y = v.y;
            }
        }
        Some((min_x, min_y, max_x, max_y))
    }

    /// Returns the z-range of the mesh frame as (min_z, max_z).
    pub fn z_range(&self) -> Option<(f64, f64)> {
        if self.vertices.is_empty() {
            return None;
        }
        let mut min_z = self.vertices[0].z;
        let mut max_z = self.vertices[0].z;
        for v in &self.vertices {
            if v.z < min_z {
                min_z = v.z;
            }
            if v.z > max_z {
                max_z = v.z;
            }
        }
        Some((min_z, max_z))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_frame_new() {
        let frame = MeshFrame::new(0.0);
        assert_eq!(frame.time(), 0.0);
        assert_eq!(frame.vertex_count(), 0);
        assert_eq!(frame.edge_count(), 0);
        assert_eq!(frame.face_count(), 0);
        assert_eq!(frame.dataset_count(), 0);
        assert!(frame.is_empty());
    }

    #[test]
    fn mesh_frame_with_geometry() {
        let vertices = vec![
            MeshVertex::new(0.0, 0.0, 0.0),
            MeshVertex::new(1.0, 0.0, 0.0),
            MeshVertex::new(0.5, 1.0, 0.0),
        ];
        let faces = vec![MeshFace::triangle(0, 1, 2)];
        let frame = MeshFrame::with_geometry(1.0, vertices, vec![], faces);
        assert_eq!(frame.time(), 1.0);
        assert_eq!(frame.vertex_count(), 3);
        assert_eq!(frame.face_count(), 1);
    }

    #[test]
    fn mesh_frame_extent() {
        let vertices = vec![
            MeshVertex::new(0.0, 0.0, 0.0),
            MeshVertex::new(10.0, 0.0, 5.0),
            MeshVertex::new(5.0, 8.0, 3.0),
        ];
        let frame = MeshFrame::with_geometry(0.0, vertices, vec![], vec![]);
        let (min_x, min_y, max_x, max_y) = frame.extent().unwrap();
        assert_eq!(min_x, 0.0);
        assert_eq!(min_y, 0.0);
        assert_eq!(max_x, 10.0);
        assert_eq!(max_y, 8.0);
    }

    #[test]
    fn mesh_frame_z_range() {
        let vertices = vec![
            MeshVertex::new(0.0, 0.0, -5.0),
            MeshVertex::new(1.0, 0.0, 10.0),
            MeshVertex::new(0.5, 1.0, 3.0),
        ];
        let frame = MeshFrame::with_geometry(0.0, vertices, vec![], vec![]);
        let (min_z, max_z) = frame.z_range().unwrap();
        assert_eq!(min_z, -5.0);
        assert_eq!(max_z, 10.0);
    }

    #[test]
    fn mesh_frame_empty_extent() {
        let frame = MeshFrame::new(0.0);
        assert_eq!(frame.extent(), None);
        assert_eq!(frame.z_range(), None);
    }

    #[test]
    fn mesh_frame_add_dataset() {
        let mut frame = MeshFrame::new(0.0);
        let ds = MeshDataset::new("test".to_string(), 0, crate::dataset::DatasetType::Vertex);
        frame.add_dataset(ds);
        assert_eq!(frame.dataset_count(), 1);
    }

    #[test]
    fn mesh_frame_accessors() {
        let vertices = vec![MeshVertex::new(1.0, 2.0, 3.0)];
        let edges = vec![MeshEdge::new(0, 1)];
        let faces = vec![MeshFace::triangle(0, 1, 2)];
        let frame = MeshFrame::with_geometry(
            5.0,
            vertices.clone(),
            edges.clone(),
            faces.clone(),
        );
        assert_eq!(frame.vertices(), vertices.as_slice());
        assert_eq!(frame.edges(), edges.as_slice());
        assert_eq!(frame.faces(), faces.as_slice());
    }
}
