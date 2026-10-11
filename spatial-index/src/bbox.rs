//! Axis-aligned bounding box for spatial indexing.
//!
//! Matches QGIS `QgsRectangle` which is the bounding-box type used by
//! `QgsSpatialIndex` for extent queries. All operations are in 2D (x, y)
//! space; Z and M coordinates are not part of the spatial index.

/// An axis-aligned 2D bounding box.
///
/// Matches QGIS `QgsRectangle`. The default value is an "empty" box with
/// min > max, which is the identity element for [`BoundingBox::union`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    /// Minimum X (west edge).
    pub min_x: f64,
    /// Minimum Y (south edge).
    pub min_y: f64,
    /// Maximum X (east edge).
    pub max_x: f64,
    /// Maximum Y (north edge).
    pub max_y: f64,
}

impl BoundingBox {
    /// Creates a new bounding box from its edges.
    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    /// Creates a bounding box from a single point (degenerate box).
    pub fn from_point(x: f64, y: f64) -> Self {
        Self {
            min_x: x,
            min_y: y,
            max_x: x,
            max_y: y,
        }
    }

    /// Creates a bounding box from two opposite corners.
    pub fn from_corners(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self {
            min_x: x1.min(x2),
            min_y: y1.min(y2),
            max_x: x1.max(x2),
            max_y: y1.max(y2),
        }
    }

    /// Returns the X coordinate of the box center.
    pub fn center_x(&self) -> f64 {
        (self.min_x + self.max_x) / 2.0
    }

    /// Returns the Y coordinate of the box center.
    pub fn center_y(&self) -> f64 {
        (self.min_y + self.max_y) / 2.0
    }

    /// Returns the width of the box (max_x - min_x).
    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }

    /// Returns the height of the box (max_y - min_y).
    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }

    /// Returns true if this box intersects (touches or overlaps) another.
    ///
    /// Edge contact counts as intersection, matching QGIS
    /// `QgsRectangle::intersects`.
    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.min_x <= other.max_x
            && self.max_x >= other.min_x
            && self.min_y <= other.max_y
            && self.max_y >= other.min_y
    }

    /// Returns true if this box fully contains another.
    pub fn contains(&self, other: &BoundingBox) -> bool {
        self.min_x <= other.min_x
            && self.max_x >= other.max_x
            && self.min_y <= other.min_y
            && self.max_y >= other.max_y
    }

    /// Returns true if the point lies inside or on the boundary of the box.
    pub fn contains_point(&self, x: f64, y: f64) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    /// Returns the area of the box (width * height).
    pub fn area(&self) -> f64 {
        self.width() * self.height()
    }

    /// Returns the area of the smallest box containing both `self` and `other`.
    pub fn expanded_area(&self, other: &BoundingBox) -> f64 {
        let w = self.max_x.max(other.max_x) - self.min_x.min(other.min_x);
        let h = self.max_y.max(other.max_y) - self.min_y.min(other.min_y);
        w * h
    }

    /// Returns the smallest box containing both `self` and `other`.
    pub fn union(&self, other: &BoundingBox) -> BoundingBox {
        BoundingBox {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    /// Returns the Euclidean distance from the box to a point.
    ///
    /// Returns `0.0` if the point is inside the box. This is the minimum
    /// distance from the point to any point in the box, used for nearest
    /// neighbor branch-and-bound.
    pub fn min_distance_to_point(&self, x: f64, y: f64) -> f64 {
        let dx = if x < self.min_x {
            self.min_x - x
        } else if x > self.max_x {
            x - self.max_x
        } else {
            0.0
        };
        let dy = if y < self.min_y {
            self.min_y - y
        } else if y > self.max_y {
            y - self.max_y
        } else {
            0.0
        };
        (dx * dx + dy * dy).sqrt()
    }

    /// Returns true if this is an empty (degenerate) box with min > max.
    pub fn is_empty(&self) -> bool {
        self.min_x > self.max_x || self.min_y > self.max_y
    }
}

impl Default for BoundingBox {
    /// Returns an empty box (min = +inf, max = -inf) that is the identity
    /// for [`BoundingBox::union`].
    fn default() -> Self {
        Self {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bbox_new() {
        let b = BoundingBox::new(0.0, 0.0, 10.0, 20.0);
        assert_eq!(b.min_x, 0.0);
        assert_eq!(b.min_y, 0.0);
        assert_eq!(b.max_x, 10.0);
        assert_eq!(b.max_y, 20.0);
        assert_eq!(b.width(), 10.0);
        assert_eq!(b.height(), 20.0);
        assert_eq!(b.area(), 200.0);
    }

    #[test]
    fn bbox_from_point() {
        let b = BoundingBox::from_point(3.0, 4.0);
        assert_eq!(b.min_x, 3.0);
        assert_eq!(b.max_x, 3.0);
        assert_eq!(b.min_y, 4.0);
        assert_eq!(b.max_y, 4.0);
        assert_eq!(b.area(), 0.0);
    }

    #[test]
    fn bbox_from_corners() {
        let b = BoundingBox::from_corners(10.0, 20.0, 0.0, 0.0);
        assert_eq!(b.min_x, 0.0);
        assert_eq!(b.min_y, 0.0);
        assert_eq!(b.max_x, 10.0);
        assert_eq!(b.max_y, 20.0);
    }

    #[test]
    fn bbox_center() {
        let b = BoundingBox::new(0.0, 0.0, 10.0, 20.0);
        assert_eq!(b.center_x(), 5.0);
        assert_eq!(b.center_y(), 10.0);
    }

    #[test]
    fn bbox_intersects_overlap() {
        let a = BoundingBox::new(0.0, 0.0, 10.0, 10.0);
        let b = BoundingBox::new(5.0, 5.0, 15.0, 15.0);
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn bbox_intersects_edge_contact() {
        let a = BoundingBox::new(0.0, 0.0, 10.0, 10.0);
        let b = BoundingBox::new(10.0, 0.0, 20.0, 10.0);
        assert!(a.intersects(&b));
    }

    #[test]
    fn bbox_intersects_no_overlap() {
        let a = BoundingBox::new(0.0, 0.0, 10.0, 10.0);
        let b = BoundingBox::new(11.0, 0.0, 20.0, 10.0);
        assert!(!a.intersects(&b));
        assert!(!b.intersects(&a));
    }

    #[test]
    fn bbox_contains() {
        let outer = BoundingBox::new(0.0, 0.0, 10.0, 10.0);
        let inner = BoundingBox::new(2.0, 2.0, 8.0, 8.0);
        let partial = BoundingBox::new(5.0, 5.0, 15.0, 15.0);
        assert!(outer.contains(&inner));
        assert!(!inner.contains(&outer));
        assert!(!outer.contains(&partial));
    }

    #[test]
    fn bbox_contains_point() {
        let b = BoundingBox::new(0.0, 0.0, 10.0, 10.0);
        assert!(b.contains_point(5.0, 5.0));
        assert!(b.contains_point(0.0, 0.0));
        assert!(b.contains_point(10.0, 10.0));
        assert!(!b.contains_point(11.0, 5.0));
        assert!(!b.contains_point(5.0, -1.0));
    }

    #[test]
    fn bbox_union() {
        let a = BoundingBox::new(0.0, 0.0, 5.0, 5.0);
        let b = BoundingBox::new(3.0, 3.0, 10.0, 10.0);
        let u = a.union(&b);
        assert_eq!(u.min_x, 0.0);
        assert_eq!(u.min_y, 0.0);
        assert_eq!(u.max_x, 10.0);
        assert_eq!(u.max_y, 10.0);
    }

    #[test]
    fn bbox_expanded_area() {
        let a = BoundingBox::new(0.0, 0.0, 5.0, 5.0);
        let b = BoundingBox::new(3.0, 3.0, 10.0, 10.0);
        // Union is 10x10 = 100; a is 25, b is 49.
        assert_eq!(a.expanded_area(&b), 100.0);
    }

    #[test]
    fn bbox_min_distance_to_point() {
        let b = BoundingBox::new(0.0, 0.0, 10.0, 10.0);
        // Point inside.
        assert_eq!(b.min_distance_to_point(5.0, 5.0), 0.0);
        // Point to the right.
        assert_eq!(b.min_distance_to_point(15.0, 5.0), 5.0);
        // Point diagonally out.
        assert!((b.min_distance_to_point(15.0, 15.0) - 50.0_f64.sqrt()).abs() < 1e-12);
        // Point below.
        assert_eq!(b.min_distance_to_point(5.0, -3.0), 3.0);
    }

    #[test]
    fn bbox_default_is_empty() {
        let b = BoundingBox::default();
        assert!(b.is_empty());
        assert!(b.min_x > b.max_x);
        assert!(b.min_y > b.max_y);
    }

    #[test]
    fn bbox_default_union_identity() {
        let b = BoundingBox::new(1.0, 2.0, 3.0, 4.0);
        let empty = BoundingBox::default();
        assert_eq!(b.union(&empty), b);
        assert_eq!(empty.union(&b), b);
    }

    #[test]
    fn bbox_empty_check() {
        assert!(!BoundingBox::new(0.0, 0.0, 1.0, 1.0).is_empty());
        assert!(BoundingBox::default().is_empty());
        assert!(BoundingBox::new(5.0, 0.0, 3.0, 1.0).is_empty());
    }
}
