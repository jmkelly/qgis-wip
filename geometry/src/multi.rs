//! Multi-geometry types: MultiPoint, MultiLineString, MultiPolygon.
//!
//! Match QGIS `QgsMultiPoint`, `QgsMultiLineString`, `QgsMultiPolygon`.
//! Each is a collection of its single-geometry counterpart with Z/M support.

use crate::linestring::LineString;
use crate::point::{Dimension, Point};
use crate::polygon::Polygon;

/// A collection of points.
///
/// Matches QGIS `QgsMultiPoint`. Supports 2D, Z, M, and ZM variants.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiPoint {
    points: Vec<Point>,
}

impl MultiPoint {
    /// Creates a new multi-point from a vector of points.
    pub fn new(points: Vec<Point>) -> Self {
        Self { points }
    }

    /// Returns the points in this multi-point.
    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// Returns the number of points.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Returns true if this multi-point has no points.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Returns true if any point has a Z coordinate.
    pub fn has_z(&self) -> bool {
        self.points.iter().any(|p| p.has_z())
    }

    /// Returns true if any point has an M coordinate.
    pub fn has_m(&self) -> bool {
        self.points.iter().any(|p| p.has_m())
    }

    /// Returns the dimensionality of this multi-point.
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
}

/// A collection of line strings.
///
/// Matches QGIS `QgsMultiLineString`. Supports 2D, Z, M, and ZM variants.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiLineString {
    lines: Vec<LineString>,
}

impl MultiLineString {
    /// Creates a new multi-line-string from a vector of line strings.
    pub fn new(lines: Vec<LineString>) -> Self {
        Self { lines }
    }

    /// Returns the line strings in this multi-line-string.
    pub fn lines(&self) -> &[LineString] {
        &self.lines
    }

    /// Returns the number of line strings.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Returns true if this multi-line-string has no line strings.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Returns the total length of all line strings.
    pub fn total_length(&self) -> f64 {
        self.lines.iter().map(|l| l.length()).sum()
    }

    /// Returns the total number of points across all line strings.
    pub fn total_points(&self) -> usize {
        self.lines.iter().map(|l| l.len()).sum()
    }

    /// Returns true if any point in any line string has a Z coordinate.
    pub fn has_z(&self) -> bool {
        self.lines.iter().any(|l| l.has_z())
    }

    /// Returns true if any point in any line string has an M coordinate.
    pub fn has_m(&self) -> bool {
        self.lines.iter().any(|l| l.has_m())
    }

    /// Returns the dimensionality of this multi-line-string.
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
}

/// A collection of polygons.
///
/// Matches QGIS `QgsMultiPolygon`. Supports 2D, Z, M, and ZM variants.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiPolygon {
    polygons: Vec<Polygon>,
}

impl MultiPolygon {
    /// Creates a new multi-polygon from a vector of polygons.
    pub fn new(polygons: Vec<Polygon>) -> Self {
        Self { polygons }
    }

    /// Returns the polygons in this multi-polygon.
    pub fn polygons(&self) -> &[Polygon] {
        &self.polygons
    }

    /// Returns the number of polygons.
    pub fn len(&self) -> usize {
        self.polygons.len()
    }

    /// Returns true if this multi-polygon has no polygons.
    pub fn is_empty(&self) -> bool {
        self.polygons.is_empty()
    }

    /// Returns the total area of all polygons (absolute values).
    pub fn total_area(&self) -> f64 {
        self.polygons.iter().map(|p| p.area_abs()).sum()
    }

    /// Returns true if any vertex in any polygon has a Z coordinate.
    pub fn has_z(&self) -> bool {
        self.polygons.iter().any(|p| p.has_z())
    }

    /// Returns true if any vertex in any polygon has an M coordinate.
    pub fn has_m(&self) -> bool {
        self.polygons.iter().any(|p| p.has_m())
    }

    /// Returns the dimensionality of this multi-polygon.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
        vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
            Point::new(x0, y0),
        ]
    }

    #[test]
    fn multipoint_new() {
        let mp = MultiPoint::new(vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)]);
        assert_eq!(mp.len(), 2);
        assert!(!mp.is_empty());
        assert_eq!(mp.points().len(), 2);
    }

    #[test]
    fn multipoint_empty() {
        let mp = MultiPoint::new(vec![]);
        assert!(mp.is_empty());
        assert_eq!(mp.len(), 0);
    }

    #[test]
    fn multipoint_z_m_detection() {
        let mp_2d = MultiPoint::new(vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)]);
        assert!(!mp_2d.has_z());
        assert!(!mp_2d.has_m());
        assert_eq!(mp_2d.dimension(), Dimension::TwoD);

        let mp_z = MultiPoint::new(vec![Point::new_z(0.0, 0.0, 5.0), Point::new_z(1.0, 1.0, 10.0)]);
        assert!(mp_z.has_z());
        assert!(!mp_z.has_m());
        assert_eq!(mp_z.dimension(), Dimension::ThreeD);

        let mp_m = MultiPoint::new(vec![Point::new_m(0.0, 0.0, 1.0), Point::new_m(1.0, 1.0, 2.0)]);
        assert!(!mp_m.has_z());
        assert!(mp_m.has_m());
        assert_eq!(mp_m.dimension(), Dimension::Measure);

        let mp_zm = MultiPoint::new(vec![
            Point::new_zm(0.0, 0.0, 5.0, 1.0),
            Point::new_zm(1.0, 1.0, 10.0, 2.0),
        ]);
        assert!(mp_zm.has_z());
        assert!(mp_zm.has_m());
        assert_eq!(mp_zm.dimension(), Dimension::ThreeDMeasure);
    }

    #[test]
    fn multilinestring_new() {
        let mls = MultiLineString::new(vec![
            LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)]),
            LineString::from_2d_coords(&[(2.0, 2.0), (3.0, 3.0)]),
        ]);
        assert_eq!(mls.len(), 2);
        assert!(!mls.is_empty());
        assert_eq!(mls.total_points(), 4);
    }

    #[test]
    fn multilinestring_empty() {
        let mls = MultiLineString::new(vec![]);
        assert!(mls.is_empty());
        assert_eq!(mls.len(), 0);
        assert_eq!(mls.total_length(), 0.0);
    }

    #[test]
    fn multilinestring_total_length() {
        let mls = MultiLineString::new(vec![
            LineString::from_2d_coords(&[(0.0, 0.0), (3.0, 4.0)]),
            LineString::from_2d_coords(&[(0.0, 0.0), (6.0, 8.0)]),
        ]);
        assert_eq!(mls.total_length(), 15.0);
    }

    #[test]
    fn multilinestring_z_m_detection() {
        let mls_2d = MultiLineString::new(vec![LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)])]);
        assert!(!mls_2d.has_z());
        assert!(!mls_2d.has_m());
        assert_eq!(mls_2d.dimension(), Dimension::TwoD);

        let mls_z = MultiLineString::new(vec![LineString::new(vec![
            Point::new_z(0.0, 0.0, 5.0),
            Point::new_z(1.0, 1.0, 10.0),
        ])]);
        assert!(mls_z.has_z());
        assert!(!mls_z.has_m());
        assert_eq!(mls_z.dimension(), Dimension::ThreeD);
    }

    #[test]
    fn multipolygon_new() {
        let mp = MultiPolygon::new(vec![
            Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap(),
            Polygon::from_rings(vec![sq(2.0, 2.0, 3.0, 3.0)]).unwrap(),
        ]);
        assert_eq!(mp.len(), 2);
        assert!(!mp.is_empty());
    }

    #[test]
    fn multipolygon_empty() {
        let mp = MultiPolygon::new(vec![]);
        assert!(mp.is_empty());
        assert_eq!(mp.len(), 0);
        assert_eq!(mp.total_area(), 0.0);
    }

    #[test]
    fn multipolygon_total_area() {
        let mp = MultiPolygon::new(vec![
            Polygon::from_rings(vec![sq(0.0, 0.0, 2.0, 2.0)]).unwrap(),
            Polygon::from_rings(vec![sq(0.0, 0.0, 3.0, 3.0)]).unwrap(),
        ]);
        assert_eq!(mp.total_area(), 13.0);
    }

    #[test]
    fn multipolygon_z_m_detection() {
        let mp_2d = MultiPolygon::new(vec![Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap()]);
        assert!(!mp_2d.has_z());
        assert!(!mp_2d.has_m());
        assert_eq!(mp_2d.dimension(), Dimension::TwoD);

        let z_ring = vec![
            Point::new_z(0.0, 0.0, 5.0),
            Point::new_z(1.0, 0.0, 5.0),
            Point::new_z(1.0, 1.0, 5.0),
            Point::new_z(0.0, 1.0, 5.0),
            Point::new_z(0.0, 0.0, 5.0),
        ];
        let mp_z = MultiPolygon::new(vec![Polygon::from_rings(vec![z_ring]).unwrap()]);
        assert!(mp_z.has_z());
        assert!(!mp_z.has_m());
        assert_eq!(mp_z.dimension(), Dimension::ThreeD);
    }
}
