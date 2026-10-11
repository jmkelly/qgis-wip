//! LineString type with Z/M support.
//!
//! Matches QGIS `QgsLineString` which is a sequence of `QgsPoint` vertices.

use crate::point::{Dimension, Point};

/// A line string (polyline) — an ordered sequence of points.
///
/// Matches QGIS `QgsLineString`. Supports 2D, Z, M, and ZM variants
/// through the [`Point`] type's optional z/m fields.
#[derive(Debug, Clone, PartialEq)]
pub struct LineString {
    points: Vec<Point>,
}

impl LineString {
    /// Creates a new line string from a vector of points.
    pub fn new(points: Vec<Point>) -> Self {
        Self { points }
    }

    /// Creates a new 2D line string from (x, y) coordinate pairs.
    pub fn from_2d_coords(coords: &[(f64, f64)]) -> Self {
        Self {
            points: coords.iter().map(|(x, y)| Point::new(*x, *y)).collect(),
        }
    }

    /// Returns the points in this line string.
    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// Returns the number of points.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Returns true if this line string has no points.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Returns the number of segments (edges) in this line string.
    pub fn num_segments(&self) -> usize {
        if self.points.len() < 2 {
            0
        } else {
            self.points.len() - 1
        }
    }

    /// Returns the total length of the line string (sum of segment distances).
    ///
    /// Computed in 2D (x, y) space; Z and M are ignored. Matches QGIS
    /// `QgsLineString::length()`.
    pub fn length(&self) -> f64 {
        self.points
            .windows(2)
            .map(|w| w[0].distance(&w[1]))
            .sum()
    }

    /// Returns true if any point in this line string has a Z coordinate.
    pub fn has_z(&self) -> bool {
        self.points.iter().any(|p| p.has_z())
    }

    /// Returns true if any point in this line string has an M coordinate.
    pub fn has_m(&self) -> bool {
        self.points.iter().any(|p| p.has_m())
    }

    /// Returns the dimensionality of this line string.
    pub fn dimension(&self) -> Dimension {
        let has_z = self.has_z();
        let has_m = self.has_m();
        match (has_z, has_m) {
            (false, false) => Dimension::TwoD,
            (true, false) => Dimension::ThreeD,
            (false, true) => Dimension::Measure,
            (true, true) => Dimension::ThreeDMeasure,
        }
    }

    /// Returns true if the line string is closed (first point equals last).
    pub fn is_closed(&self) -> bool {
        if self.points.len() < 2 {
            return false;
        }
        self.points.first() == self.points.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linestring_new() {
        let ls = LineString::new(vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)]);
        assert_eq!(ls.len(), 2);
        assert!(!ls.is_empty());
        assert_eq!(ls.num_segments(), 1);
    }

    #[test]
    fn linestring_from_2d_coords() {
        let ls = LineString::from_2d_coords(&[(0.0, 0.0), (3.0, 4.0), (6.0, 8.0)]);
        assert_eq!(ls.len(), 3);
        assert_eq!(ls.points()[0], Point::new(0.0, 0.0));
        assert_eq!(ls.points()[1], Point::new(3.0, 4.0));
        assert_eq!(ls.points()[2], Point::new(6.0, 8.0));
    }

    #[test]
    fn linestring_empty() {
        let ls = LineString::new(vec![]);
        assert!(ls.is_empty());
        assert_eq!(ls.len(), 0);
        assert_eq!(ls.num_segments(), 0);
        assert_eq!(ls.length(), 0.0);
    }

    #[test]
    fn linestring_single_point() {
        let ls = LineString::new(vec![Point::new(1.0, 2.0)]);
        assert_eq!(ls.len(), 1);
        assert_eq!(ls.num_segments(), 0);
        assert_eq!(ls.length(), 0.0);
    }

    #[test]
    fn linestring_length_2d() {
        let ls = LineString::from_2d_coords(&[(0.0, 0.0), (3.0, 4.0)]);
        assert_eq!(ls.length(), 5.0);
    }

    #[test]
    fn linestring_length_multiple_segments() {
        let ls = LineString::from_2d_coords(&[(0.0, 0.0), (3.0, 4.0), (6.0, 8.0)]);
        assert_eq!(ls.length(), 10.0);
    }

    #[test]
    fn linestring_length_ignores_z() {
        let ls = LineString::new(vec![
            Point::new_z(0.0, 0.0, 100.0),
            Point::new_z(3.0, 4.0, 200.0),
        ]);
        assert_eq!(ls.length(), 5.0);
    }

    #[test]
    fn linestring_z_m_detection() {
        let ls_2d = LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)]);
        assert!(!ls_2d.has_z());
        assert!(!ls_2d.has_m());
        assert_eq!(ls_2d.dimension(), Dimension::TwoD);

        let ls_z = LineString::new(vec![Point::new_z(0.0, 0.0, 1.0), Point::new_z(1.0, 1.0, 2.0)]);
        assert!(ls_z.has_z());
        assert!(!ls_z.has_m());
        assert_eq!(ls_z.dimension(), Dimension::ThreeD);

        let ls_m = LineString::new(vec![Point::new_m(0.0, 0.0, 10.0), Point::new_m(1.0, 1.0, 20.0)]);
        assert!(!ls_m.has_z());
        assert!(ls_m.has_m());
        assert_eq!(ls_m.dimension(), Dimension::Measure);

        let ls_zm = LineString::new(vec![
            Point::new_zm(0.0, 0.0, 1.0, 10.0),
            Point::new_zm(1.0, 1.0, 2.0, 20.0),
        ]);
        assert!(ls_zm.has_z());
        assert!(ls_zm.has_m());
        assert_eq!(ls_zm.dimension(), Dimension::ThreeDMeasure);
    }

    #[test]
    fn linestring_is_closed() {
        let open = LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]);
        assert!(!open.is_closed());

        let closed = LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)]);
        assert!(closed.is_closed());

        let single = LineString::new(vec![Point::new(0.0, 0.0)]);
        assert!(!single.is_closed());

        let empty = LineString::new(vec![]);
        assert!(!empty.is_closed());
    }

    #[test]
    fn linestring_points_accessor() {
        let ls = LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)]);
        let pts = ls.points();
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0], Point::new(0.0, 0.0));
        assert_eq!(pts[1], Point::new(1.0, 1.0));
    }
}
