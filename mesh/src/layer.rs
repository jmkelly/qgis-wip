//! Mesh layer: a collection of mesh frames.
//!
//! Matches QGIS `QgsMeshLayer` which represents a mesh data source
//! with multiple frames (time steps) and datasets.

use crate::dataset::DatasetType;
use crate::MeshFrame;

/// Mesh layer metadata and frame container.
///
/// Matches QGIS `QgsMeshLayer`. Contains the mesh source information
/// and a collection of frames (time steps).
#[derive(Debug, Clone, PartialEq)]
pub struct MeshLayer {
    /// Layer name.
    pub name: String,
    /// The CRS as a WKT string (simplified — full WKT2 parsing in E2).
    pub crs_wkt: Option<String>,
    /// Number of vertices per frame (0 if unknown).
    pub vertex_count: usize,
    /// Number of faces per frame (0 if unknown).
    pub face_count: usize,
    /// Number of edges per frame (0 if unknown).
    pub edge_count: usize,
    /// Frames (time steps) in the layer.
    pub frames: Vec<MeshFrame>,
    /// Dataset group names.
    pub dataset_groups: Vec<String>,
    /// Native data type for the layer.
    pub data_type: DatasetType,
}

impl MeshLayer {
    /// Creates a new empty mesh layer.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            crs_wkt: None,
            vertex_count: 0,
            face_count: 0,
            edge_count: 0,
            frames: Vec::new(),
            dataset_groups: Vec::new(),
            data_type: DatasetType::Vertex,
        }
    }

    /// Sets the CRS WKT string.
    pub fn set_crs(&mut self, wkt: impl Into<String>) {
        self.crs_wkt = Some(wkt.into());
    }

    /// Returns the CRS WKT string, if set.
    pub fn crs(&self) -> Option<&str> {
        self.crs_wkt.as_deref()
    }

    /// Returns true if the layer has a CRS set.
    pub fn has_crs(&self) -> bool {
        self.crs_wkt.is_some()
    }

    /// Returns the layer name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the number of frames.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Returns true if the layer has no frames.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Returns a reference to the frames.
    pub fn frames(&self) -> &[MeshFrame] {
        &self.frames
    }

    /// Adds a frame to the layer.
    pub fn add_frame(&mut self, frame: MeshFrame) {
        self.frames.push(frame);
    }

    /// Returns the number of dataset groups.
    pub fn dataset_group_count(&self) -> usize {
        self.dataset_groups.len()
    }

    /// Adds a dataset group name.
    pub fn add_dataset_group(&mut self, name: impl Into<String>) {
        self.dataset_groups.push(name.into());
    }

    /// Returns the dataset groups.
    pub fn dataset_groups(&self) -> &[String] {
        &self.dataset_groups
    }

    /// Returns the native data type.
    pub fn data_type(&self) -> DatasetType {
        self.data_type
    }

    /// Sets the native data type.
    pub fn set_data_type(&mut self, data_type: DatasetType) {
        self.data_type = data_type;
    }

    /// Returns the total number of vertices across all frames.
    pub fn total_vertex_count(&self) -> usize {
        self.frames.iter().map(|f| f.vertex_count()).sum()
    }

    /// Returns the total number of faces across all frames.
    pub fn total_face_count(&self) -> usize {
        self.frames.iter().map(|f| f.face_count()).sum()
    }

    /// Returns the total number of edges across all frames.
    pub fn total_edge_count(&self) -> usize {
        self.frames.iter().map(|f| f.edge_count()).sum()
    }

    /// Returns the extent across all frames as (min_x, min_y, max_x, max_y).
    pub fn extent(&self) -> Option<(f64, f64, f64, f64)> {
        let mut result: Option<(f64, f64, f64, f64)> = None;
        for frame in &self.frames {
            if let Some((fmin_x, fmin_y, fmax_x, fmax_y)) = frame.extent() {
                result = Some(match result {
                    None => (fmin_x, fmin_y, fmax_x, fmax_y),
                    Some((min_x, min_y, max_x, max_y)) => (
                        min_x.min(fmin_x),
                        min_y.min(fmin_y),
                        max_x.max(fmax_x),
                        max_y.max(fmax_y),
                    ),
                });
            }
        }
        result
    }

    /// Returns the time values of all frames.
    pub fn time_values(&self) -> Vec<f64> {
        self.frames.iter().map(|f| f.time()).collect()
    }

    /// Returns the frame with the maximum time value.
    pub fn max_time_frame(&self) -> Option<&MeshFrame> {
        self.frames.iter().max_by(|a, b| {
            a.time()
                .partial_cmp(&b.time())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Returns the frame with the minimum time value.
    pub fn min_time_frame(&self) -> Option<&MeshFrame> {
        self.frames.iter().min_by(|a, b| {
            a.time()
                .partial_cmp(&b.time())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MeshFace, MeshVertex};

    #[test]
    fn mesh_layer_new() {
        let layer = MeshLayer::new("test_mesh");
        assert_eq!(layer.name(), "test_mesh");
        assert!(!layer.has_crs());
        assert!(layer.is_empty());
        assert_eq!(layer.frame_count(), 0);
    }

    #[test]
    fn mesh_layer_crs() {
        let mut layer = MeshLayer::new("test");
        assert!(!layer.has_crs());
        layer.set_crs("EPSG:4326");
        assert!(layer.has_crs());
        assert_eq!(layer.crs(), Some("EPSG:4326"));
    }

    #[test]
    fn mesh_layer_add_frame() {
        let mut layer = MeshLayer::new("test");
        let frame = MeshFrame::new(0.0);
        layer.add_frame(frame);
        assert_eq!(layer.frame_count(), 1);
        assert!(!layer.is_empty());
    }

    #[test]
    fn mesh_layer_total_counts() {
        let mut layer = MeshLayer::new("test");
        let vertices = vec![
            MeshVertex::new(0.0, 0.0, 0.0),
            MeshVertex::new(1.0, 0.0, 0.0),
            MeshVertex::new(0.5, 1.0, 0.0),
        ];
        let faces = vec![MeshFace::triangle(0, 1, 2)];
        let frame = MeshFrame::with_geometry(0.0, vertices, vec![], faces);
        layer.add_frame(frame);
        assert_eq!(layer.total_vertex_count(), 3);
        assert_eq!(layer.total_face_count(), 1);
        assert_eq!(layer.total_edge_count(), 0);
    }

    #[test]
    fn mesh_layer_extent() {
        let mut layer = MeshLayer::new("test");
        let vertices = vec![
            MeshVertex::new(0.0, 0.0, 0.0),
            MeshVertex::new(10.0, 5.0, 3.0),
        ];
        let frame = MeshFrame::with_geometry(0.0, vertices, vec![], vec![]);
        layer.add_frame(frame);
        let (min_x, min_y, max_x, max_y) = layer.extent().unwrap();
        assert_eq!(min_x, 0.0);
        assert_eq!(min_y, 0.0);
        assert_eq!(max_x, 10.0);
        assert_eq!(max_y, 5.0);
    }

    #[test]
    fn mesh_layer_time_values() {
        let mut layer = MeshLayer::new("test");
        layer.add_frame(MeshFrame::new(0.0));
        layer.add_frame(MeshFrame::new(1.5));
        layer.add_frame(MeshFrame::new(3.0));
        assert_eq!(layer.time_values(), vec![0.0, 1.5, 3.0]);
    }

    #[test]
    fn mesh_layer_min_max_time() {
        let mut layer = MeshLayer::new("test");
        layer.add_frame(MeshFrame::new(2.0));
        layer.add_frame(MeshFrame::new(0.0));
        layer.add_frame(MeshFrame::new(5.0));
        assert_eq!(layer.min_time_frame().unwrap().time(), 0.0);
        assert_eq!(layer.max_time_frame().unwrap().time(), 5.0);
    }

    #[test]
    fn mesh_layer_empty_time() {
        let layer = MeshLayer::new("test");
        assert_eq!(layer.min_time_frame(), None);
        assert_eq!(layer.max_time_frame(), None);
        assert!(layer.time_values().is_empty());
    }

    #[test]
    fn mesh_layer_dataset_groups() {
        let mut layer = MeshLayer::new("test");
        layer.add_dataset_group("Bed Elevation");
        layer.add_dataset_group("Water Depth");
        assert_eq!(layer.dataset_group_count(), 2);
        assert_eq!(layer.dataset_groups()[0], "Bed Elevation");
    }

    #[test]
    fn mesh_layer_data_type() {
        let mut layer = MeshLayer::new("test");
        assert!(layer.data_type().is_vertex());
        layer.set_data_type(DatasetType::Face);
        assert!(layer.data_type().is_face());
    }

    #[test]
    fn mesh_layer_frames_accessor() {
        let mut layer = MeshLayer::new("test");
        layer.add_frame(MeshFrame::new(0.0));
        layer.add_frame(MeshFrame::new(1.0));
        assert_eq!(layer.frames().len(), 2);
    }
}
