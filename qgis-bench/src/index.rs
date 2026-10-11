//! Spatial-index (R-tree) hot paths.
//!
//! QGIS uses `QgsSpatialIndex` (an R-tree) for fast feature lookup by
//! extent. These stubs stand in for the index build (insert) and query hot
//! paths that the render and provider layers drive.

use crate::{Extent, Feature, Polygon};

/// Stub R-tree: a feature store keyed by id. Eventually backed by a real
/// R-tree replacing `QgsSpatialIndex`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RTree {
    features: Vec<Feature>,
}

impl RTree {
    /// Creates an empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a feature into the index.
    pub fn insert(&mut self, feature: Feature) {
        self.features.push(feature);
    }

    /// Returns the number of features in the index.
    pub fn len(&self) -> usize {
        self.features.len()
    }

    /// Returns true if the index contains no features.
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

/// Stub for the R-tree build hot path: insert every feature into a fresh index.
pub fn rtree_insert(features: &[Feature]) -> RTree {
    let mut tree = RTree::new();
    for feature in features {
        tree.insert(Feature::new(
            feature.id,
            Polygon::new(feature.geometry.exterior.clone()),
        ));
    }
    tree
}

/// Stub for the R-tree query hot path: return ids whose bbox intersects `extent`.
pub fn rtree_query(tree: &RTree, extent: &Extent) -> Vec<u64> {
    tree.features
        .iter()
        .filter(|f| {
            f.geometry.exterior.iter().any(|p| {
                p.x >= extent.xmin && p.x <= extent.xmax && p.y >= extent.ymin && p.y <= extent.ymax
            })
        })
        .map(|f| f.id)
        .collect()
}
