//! Mesh dataset: a named group of values on a mesh.
//!
//! Matches QGIS `QgsMeshDataset` which represents a named dataset
//! (e.g., water depth, velocity) on a mesh, with values defined
//! on vertices, faces, or edges.

/// The type of dataset: where its values are defined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatasetType {
    /// Values defined on vertices.
    Vertex,
    /// Values defined on faces.
    Face,
    /// Values defined on edges.
    Edge,
}

impl DatasetType {
    /// Returns true if this dataset is defined on vertices.
    pub fn is_vertex(&self) -> bool {
        matches!(self, DatasetType::Vertex)
    }

    /// Returns true if this dataset is defined on faces.
    pub fn is_face(&self) -> bool {
        matches!(self, DatasetType::Face)
    }

    /// Returns true if this dataset is defined on edges.
    pub fn is_edge(&self) -> bool {
        matches!(self, DatasetType::Edge)
    }
}

/// A named mesh dataset with values on vertices, faces, or edges.
///
/// Matches QGIS `QgsMeshDataset`.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshDataset {
    /// Dataset name.
    pub name: String,
    /// Metadata group index (for grouping related datasets).
    pub group_index: usize,
    /// Where the dataset values are defined.
    pub dataset_type: DatasetType,
    /// Values (one per element of the dataset type).
    pub values: Vec<f64>,
    /// True if the dataset is active (selected for rendering).
    pub is_active: bool,
    /// Minimum value in the dataset.
    pub min_value: Option<f64>,
    /// Maximum value in the dataset.
    pub max_value: Option<f64>,
}

impl MeshDataset {
    /// Creates a new mesh dataset.
    pub fn new(name: String, group_index: usize, dataset_type: DatasetType) -> Self {
        Self {
            name,
            group_index,
            dataset_type,
            values: Vec::new(),
            is_active: true,
            min_value: None,
            max_value: None,
        }
    }

    /// Creates a new mesh dataset with values.
    pub fn new_with_values(
        name: String,
        group_index: usize,
        dataset_type: DatasetType,
        values: Vec<f64>,
    ) -> Self {
        let mut ds = Self {
            name,
            group_index,
            dataset_type,
            values,
            is_active: true,
            min_value: None,
            max_value: None,
        };
        ds.compute_stats();
        ds
    }

    /// Returns the dataset name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the group index.
    pub fn group_index(&self) -> usize {
        self.group_index
    }

    /// Returns the dataset type.
    pub fn dataset_type(&self) -> DatasetType {
        self.dataset_type
    }

    /// Returns the values.
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// Returns the number of values.
    pub fn value_count(&self) -> usize {
        self.values.len()
    }

    /// Returns true if the dataset is active.
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Sets whether the dataset is active.
    pub fn set_active(&mut self, active: bool) {
        self.is_active = active;
    }

    /// Returns the minimum value.
    pub fn min_value(&self) -> Option<f64> {
        self.min_value
    }

    /// Returns the maximum value.
    pub fn max_value(&self) -> Option<f64> {
        self.max_value
    }

    /// Computes min/max statistics from the current values.
    pub fn compute_stats(&mut self) {
        if self.values.is_empty() {
            self.min_value = None;
            self.max_value = None;
            return;
        }
        let mut min = self.values[0];
        let mut max = self.values[0];
        for &v in &self.values {
            if v < min {
                min = v;
            }
            if v > max {
                max = v;
            }
        }
        self.min_value = Some(min);
        self.max_value = Some(max);
    }

    /// Returns the mean value.
    pub fn mean_value(&self) -> Option<f64> {
        if self.values.is_empty() {
            None
        } else {
            Some(self.values.iter().sum::<f64>() / self.values.len() as f64)
        }
    }

    /// Returns true if the dataset has no values.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dataset_type_checks() {
        assert!(DatasetType::Vertex.is_vertex());
        assert!(!DatasetType::Vertex.is_face());
        assert!(!DatasetType::Vertex.is_edge());

        assert!(!DatasetType::Face.is_vertex());
        assert!(DatasetType::Face.is_face());
        assert!(!DatasetType::Face.is_edge());

        assert!(!DatasetType::Edge.is_vertex());
        assert!(!DatasetType::Edge.is_face());
        assert!(DatasetType::Edge.is_edge());
    }

    #[test]
    fn mesh_dataset_new() {
        let ds = MeshDataset::new("depth".to_string(), 0, DatasetType::Vertex);
        assert_eq!(ds.name(), "depth");
        assert_eq!(ds.group_index(), 0);
        assert_eq!(ds.dataset_type(), DatasetType::Vertex);
        assert!(ds.is_active());
        assert!(ds.is_empty());
        assert_eq!(ds.value_count(), 0);
    }

    #[test]
    fn mesh_dataset_with_values() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ds = MeshDataset::new_with_values(
            "velocity".to_string(),
            1,
            DatasetType::Face,
            values,
        );
        assert_eq!(ds.name(), "velocity");
        assert_eq!(ds.value_count(), 5);
        assert_eq!(ds.min_value(), Some(1.0));
        assert_eq!(ds.max_value(), Some(5.0));
        assert!(!ds.is_empty());
    }

    #[test]
    fn mesh_dataset_active() {
        let mut ds = MeshDataset::new("test".to_string(), 0, DatasetType::Vertex);
        assert!(ds.is_active());
        ds.set_active(false);
        assert!(!ds.is_active());
    }

    #[test]
    fn mesh_dataset_mean() {
        let ds = MeshDataset::new_with_values(
            "test".to_string(),
            0,
            DatasetType::Vertex,
            vec![1.0, 2.0, 3.0],
        );
        assert_eq!(ds.mean_value(), Some(2.0));
    }

    #[test]
    fn mesh_dataset_empty_stats() {
        let ds = MeshDataset::new("empty".to_string(), 0, DatasetType::Vertex);
        assert_eq!(ds.min_value(), None);
        assert_eq!(ds.max_value(), None);
        assert_eq!(ds.mean_value(), None);
    }

    #[test]
    fn mesh_dataset_negative_values() {
        let ds = MeshDataset::new_with_values(
            "test".to_string(),
            0,
            DatasetType::Vertex,
            vec![-5.0, 0.0, 5.0],
        );
        assert_eq!(ds.min_value(), Some(-5.0));
        assert_eq!(ds.max_value(), Some(5.0));
    }
}
