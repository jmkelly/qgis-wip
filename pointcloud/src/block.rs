//! Point cloud block: a chunk of point data.
//!
//! Matches QGIS `QgsPointCloudBlock` which stores a block of points
//! with their attribute values.

use crate::LasPoint;

/// A block of point cloud data.
///
/// Matches QGIS `QgsPointCloudBlock`. Stores a collection of points
/// with their raw attribute values.
#[derive(Debug, Clone, PartialEq)]
pub struct PointCloudBlock {
    /// Points in this block.
    pub points: Vec<LasPoint>,
    /// Custom attribute values (name -> values).
    pub attributes: Vec<(String, Vec<f64>)>,
}

impl PointCloudBlock {
    /// Creates a new empty point cloud block.
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            attributes: Vec::new(),
        }
    }

    /// Creates a new point cloud block from a vector of points.
    pub fn from_points(points: Vec<LasPoint>) -> Self {
        Self {
            points,
            attributes: Vec::new(),
        }
    }

    /// Returns the number of points in the block.
    pub fn count(&self) -> usize {
        self.points.len()
    }

    /// Returns true if the block has no points.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Returns a reference to the points.
    pub fn points(&self) -> &[LasPoint] {
        &self.points
    }

    /// Adds a point to the block.
    pub fn add_point(&mut self, point: LasPoint) {
        self.points.push(point);
    }

    /// Returns the bounding box as (min_x, min_y, min_z, max_x, max_y, max_z)
    /// using raw integer coordinates.
    pub fn raw_bounds(&self) -> Option<(i32, i32, i32, i32, i32, i32)> {
        if self.points.is_empty() {
            return None;
        }
        let mut min_x = self.points[0].x;
        let mut max_x = self.points[0].x;
        let mut min_y = self.points[0].y;
        let mut max_y = self.points[0].y;
        let mut min_z = self.points[0].z;
        let mut max_z = self.points[0].z;
        for p in &self.points {
            if p.x < min_x {
                min_x = p.x;
            }
            if p.x > max_x {
                max_x = p.x;
            }
            if p.y < min_y {
                min_y = p.y;
            }
            if p.y > max_y {
                max_y = p.y;
            }
            if p.z < min_z {
                min_z = p.z;
            }
            if p.z > max_z {
                max_z = p.z;
            }
        }
        Some((min_x, min_y, min_z, max_x, max_y, max_z))
    }

    /// Returns the bounding box as (min_x, min_y, min_z, max_x, max_y, max_z)
    /// using scaled coordinates.
    pub fn scaled_bounds(
        &self,
        scale_x: f64,
        offset_x: f64,
        scale_y: f64,
        offset_y: f64,
        scale_z: f64,
        offset_z: f64,
    ) -> Option<(f64, f64, f64, f64, f64, f64)> {
        let (min_x, min_y, min_z, max_x, max_y, max_z) = self.raw_bounds()?;
        Some((
            min_x as f64 * scale_x + offset_x,
            min_y as f64 * scale_y + offset_y,
            min_z as f64 * scale_z + offset_z,
            max_x as f64 * scale_x + offset_x,
            max_y as f64 * scale_y + offset_y,
            max_z as f64 * scale_z + offset_z,
        ))
    }

    /// Returns the number of points with the given classification.
    pub fn count_by_classification(&self, classification: u8) -> usize {
        self.points
            .iter()
            .filter(|p| p.classification == classification)
            .count()
    }

    /// Returns the number of ground points (classification 2).
    pub fn ground_count(&self) -> usize {
        self.count_by_classification(2)
    }

    /// Returns the number of vegetation points (classifications 3, 4, 5).
    pub fn vegetation_count(&self) -> usize {
        self.points
            .iter()
            .filter(|p| matches!(p.classification, 3 | 4 | 5))
            .count()
    }

    /// Returns the number of building points (classification 6).
    pub fn building_count(&self) -> usize {
        self.count_by_classification(6)
    }
}

impl Default for PointCloudBlock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_cloud_block_new() {
        let block = PointCloudBlock::new();
        assert!(block.is_empty());
        assert_eq!(block.count(), 0);
    }

    #[test]
    fn point_cloud_block_from_points() {
        let points = vec![LasPoint::new(1, 2, 3), LasPoint::new(4, 5, 6)];
        let block = PointCloudBlock::from_points(points);
        assert_eq!(block.count(), 2);
        assert!(!block.is_empty());
    }

    #[test]
    fn point_cloud_block_add_point() {
        let mut block = PointCloudBlock::new();
        block.add_point(LasPoint::new(1, 2, 3));
        assert_eq!(block.count(), 1);
        block.add_point(LasPoint::new(4, 5, 6));
        assert_eq!(block.count(), 2);
    }

    #[test]
    fn point_cloud_block_raw_bounds() {
        let points = vec![
            LasPoint::new(10, 20, 30),
            LasPoint::new(-5, 15, 25),
            LasPoint::new(8, -3, 35),
        ];
        let block = PointCloudBlock::from_points(points);
        let bounds = block.raw_bounds().unwrap();
        assert_eq!(bounds, (-5, -3, 25, 10, 20, 35));
    }

    #[test]
    fn point_cloud_block_scaled_bounds() {
        let points = vec![LasPoint::new(100, 200, 300), LasPoint::new(200, 400, 600)];
        let block = PointCloudBlock::from_points(points);
        let bounds = block.scaled_bounds(0.01, 0.0, 0.01, 0.0, 0.01, 0.0).unwrap();
        assert_eq!(bounds, (1.0, 2.0, 3.0, 2.0, 4.0, 6.0));
    }

    #[test]
    fn point_cloud_block_empty_bounds() {
        let block = PointCloudBlock::new();
        assert_eq!(block.raw_bounds(), None);
    }

    #[test]
    fn point_cloud_block_classification_counts() {
        let mut p1 = LasPoint::new(0, 0, 0);
        p1.classification = 2; // ground
        let mut p2 = LasPoint::new(1, 1, 1);
        p2.classification = 4; // vegetation
        let mut p3 = LasPoint::new(2, 2, 2);
        p3.classification = 6; // building
        let mut p4 = LasPoint::new(3, 3, 3);
        p4.classification = 2; // ground
        let block = PointCloudBlock::from_points(vec![p1, p2, p3, p4]);
        assert_eq!(block.ground_count(), 2);
        assert_eq!(block.vegetation_count(), 1);
        assert_eq!(block.building_count(), 1);
        assert_eq!(block.count_by_classification(2), 2);
        assert_eq!(block.count_by_classification(4), 1);
        assert_eq!(block.count_by_classification(6), 1);
    }

    #[test]
    fn point_cloud_block_default() {
        let block = PointCloudBlock::default();
        assert!(block.is_empty());
    }

    #[test]
    fn point_cloud_block_points_accessor() {
        let points = vec![LasPoint::new(1, 2, 3)];
        let block = PointCloudBlock::from_points(points.clone());
        assert_eq!(block.points(), points.as_slice());
    }
}
