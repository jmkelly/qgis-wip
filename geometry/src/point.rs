//! Point type with Z/M support.
//!
//! Matches QGIS `QgsPoint` which carries x, y, z, m with `is3D()` and
//! `isMeasure()` flags. In Rust we model the flags as `Option<f64>` fields.

/// Dimensionality of a geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dimension {
    /// Two-dimensional (x, y only).
    TwoD,
    /// Three-dimensional (x, y, z).
    ThreeD,
    /// Measure (x, y, m).
    Measure,
    /// Three-dimensional with measure (x, y, z, m).
    ThreeDMeasure,
}

impl Dimension {
    /// Returns true if this dimension includes a Z coordinate.
    pub fn has_z(self) -> bool {
        matches!(self, Dimension::ThreeD | Dimension::ThreeDMeasure)
    }

    /// Returns true if this dimension includes a measure.
    pub fn has_m(self) -> bool {
        matches!(self, Dimension::Measure | Dimension::ThreeDMeasure)
    }
}

/// A point with optional Z and M coordinates.
///
/// * [`Point::new`] creates a 2D point (x, y).
/// * [`Point::new_z`] creates a 3D point (x, y, z).
/// * [`Point::new_m`] creates a measured point (x, y, m).
/// * [`Point::new_zm`] creates a 3D measured point (x, y, z, m).
///
/// Matches QGIS `QgsPoint` which stores z/m as optional doubles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// X coordinate (easting / longitude).
    pub x: f64,
    /// Y coordinate (northing / latitude).
    pub y: f64,
    /// Z coordinate (elevation), if present.
    pub z: Option<f64>,
    /// M coordinate (measure), if present.
    pub m: Option<f64>,
}

impl Point {
    /// Creates a new 2D point at the given coordinates.
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            z: None,
            m: None,
        }
    }

    /// Creates a new 3D point with a Z coordinate.
    pub fn new_z(x: f64, y: f64, z: f64) -> Self {
        Self {
            x,
            y,
            z: Some(z),
            m: None,
        }
    }

    /// Creates a new measured point with an M coordinate.
    pub fn new_m(x: f64, y: f64, m: f64) -> Self {
        Self {
            x,
            y,
            z: None,
            m: Some(m),
        }
    }

    /// Creates a new 3D measured point with both Z and M coordinates.
    pub fn new_zm(x: f64, y: f64, z: f64, m: f64) -> Self {
        Self {
            x,
            y,
            z: Some(z),
            m: Some(m),
        }
    }

    /// Returns the X coordinate.
    pub fn x(&self) -> f64 {
        self.x
    }

    /// Returns the Y coordinate.
    pub fn y(&self) -> f64 {
        self.y
    }

    /// Returns the Z coordinate, if present.
    pub fn z(&self) -> Option<f64> {
        self.z
    }

    /// Returns the M coordinate, if present.
    pub fn m(&self) -> Option<f64> {
        self.m
    }

    /// Returns true if this point has a Z coordinate.
    pub fn has_z(&self) -> bool {
        self.z.is_some()
    }

    /// Returns true if this point has an M coordinate.
    pub fn has_m(&self) -> bool {
        self.m.is_some()
    }

    /// Returns the dimensionality of this point.
    pub fn dimension(&self) -> Dimension {
        match (self.z, self.m) {
            (None, None) => Dimension::TwoD,
            (Some(_), None) => Dimension::ThreeD,
            (None, Some(_)) => Dimension::Measure,
            (Some(_), Some(_)) => Dimension::ThreeDMeasure,
        }
    }

    /// Returns true if the point is empty (x or y is NaN).
    ///
    /// Matches QGIS `QgsPoint::isEmpty()`.
    pub fn is_empty(&self) -> bool {
        self.x.is_nan() || self.y.is_nan()
    }

    /// Returns the 2D Euclidean distance to another point.
    ///
    /// Z and M coordinates are ignored. Matches QGIS `QgsPoint::distance`
    /// which computes planar distance.
    pub fn distance(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_new_is_2d() {
        let p = Point::new(1.0, 2.0);
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
        assert_eq!(p.z, None);
        assert_eq!(p.m, None);
        assert!(!p.has_z());
        assert!(!p.has_m());
        assert_eq!(p.dimension(), Dimension::TwoD);
    }

    #[test]
    fn point_new_z() {
        let p = Point::new_z(1.0, 2.0, 3.0);
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
        assert_eq!(p.z, Some(3.0));
        assert_eq!(p.m, None);
        assert!(p.has_z());
        assert!(!p.has_m());
        assert_eq!(p.dimension(), Dimension::ThreeD);
    }

    #[test]
    fn point_new_m() {
        let p = Point::new_m(1.0, 2.0, 42.0);
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
        assert_eq!(p.z, None);
        assert_eq!(p.m, Some(42.0));
        assert!(!p.has_z());
        assert!(p.has_m());
        assert_eq!(p.dimension(), Dimension::Measure);
    }

    #[test]
    fn point_new_zm() {
        let p = Point::new_zm(1.0, 2.0, 3.0, 42.0);
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
        assert_eq!(p.z, Some(3.0));
        assert_eq!(p.m, Some(42.0));
        assert!(p.has_z());
        assert!(p.has_m());
        assert_eq!(p.dimension(), Dimension::ThreeDMeasure);
    }

    #[test]
    fn point_accessors() {
        let p = Point::new_zm(10.0, 20.0, 30.0, 40.0);
        assert_eq!(p.x(), 10.0);
        assert_eq!(p.y(), 20.0);
        assert_eq!(p.z(), Some(30.0));
        assert_eq!(p.m(), Some(40.0));
    }

    #[test]
    fn point_is_empty() {
        assert!(!Point::new(0.0, 0.0).is_empty());
        assert!(Point::new(f64::NAN, 0.0).is_empty());
        assert!(Point::new(0.0, f64::NAN).is_empty());
        assert!(Point::new(f64::NAN, f64::NAN).is_empty());
    }

    #[test]
    fn point_distance() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(3.0, 4.0);
        assert_eq!(a.distance(&b), 5.0);
        assert_eq!(b.distance(&a), 5.0);
    }

    #[test]
    fn point_distance_ignores_z_m() {
        let a = Point::new_zm(0.0, 0.0, 100.0, 999.0);
        let b = Point::new_zm(3.0, 4.0, 200.0, 888.0);
        assert_eq!(a.distance(&b), 5.0);
    }

    #[test]
    fn point_clone_equal() {
        let p1 = Point::new_zm(1.0, 2.0, 3.0, 4.0);
        let p2 = p1;
        assert_eq!(p1, p2);
    }

    #[test]
    fn dimension_helpers() {
        assert!(!Dimension::TwoD.has_z());
        assert!(!Dimension::TwoD.has_m());
        assert!(Dimension::ThreeD.has_z());
        assert!(!Dimension::ThreeD.has_m());
        assert!(!Dimension::Measure.has_z());
        assert!(Dimension::Measure.has_m());
        assert!(Dimension::ThreeDMeasure.has_z());
        assert!(Dimension::ThreeDMeasure.has_m());
    }
}
