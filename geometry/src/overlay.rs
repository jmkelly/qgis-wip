//! Overlay operations: union, intersection, difference, symmetric_difference.
//!
//! Delegates to the `geo` crate's `BooleanOps` trait. All operations are
//! computed in 2D planar space (Z/M dropped), matching QGIS behavior where
//! GEOS/JTS overlay operations ignore Z.
//!
//! JTS/GEOS gaps:
//! * `geo`'s boolean operations may produce slightly different vertex
//!   ordering than JTS for complex polygons with holes.
//! * `geo` does not support overlay on GeometryCollections containing
//!   mixed dimensions (e.g. Point + Polygon in one collection). QGIS/GEOS
//!   handles these via `GeometryCollection` decomposition.
//! * `geo` returns `MultiPolygon` for all boolean ops; QGIS may return
//!   `GeometryCollection` for mixed-type results.
//! * `geo`'s `BooleanOps` is only implemented for `Polygon` and
//!   `MultiPolygon`. Overlay involving points or linestrings returns an
//!   empty geometry (logged as a gap).

use crate::collection::Geometry;
use crate::geo_bridge;
use geo::BooleanOps;

/// Returns the union of two geometries.
///
/// The union is the set of all points that are in either geometry.
/// Matches QGIS `QgsGeometry::combine` (union).
///
/// **Gap**: `geo`'s `BooleanOps` only supports `Polygon` and `MultiPolygon`.
/// Union involving points or linestrings returns an empty geometry.
pub fn union(a: &Geometry, b: &Geometry) -> Geometry {
    let geo_a = geo_bridge::geometry_to_geo(a);
    let geo_b = geo_bridge::geometry_to_geo(b);
    let result = boolean_op(&geo_a, &geo_b, |x, y| x.union(y));
    geo_bridge::multipolygon_to_geometry(&result)
}

/// Returns the intersection of two geometries.
///
/// The intersection is the set of all points that are in both geometries.
/// Matches QGIS `QgsGeometry::intersection`.
///
/// **Gap**: `geo`'s `BooleanOps` only supports `Polygon` and `MultiPolygon`.
/// Intersection involving points or linestrings returns an empty geometry.
pub fn intersection(a: &Geometry, b: &Geometry) -> Geometry {
    let geo_a = geo_bridge::geometry_to_geo(a);
    let geo_b = geo_bridge::geometry_to_geo(b);
    let result = boolean_op(&geo_a, &geo_b, |x, y| x.intersection(y));
    geo_bridge::multipolygon_to_geometry(&result)
}

/// Returns the difference of two geometries (a - b).
///
/// The difference is the set of all points in `a` that are not in `b`.
/// Matches QGIS `QgsGeometry::difference`.
///
/// **Gap**: `geo`'s `BooleanOps` only supports `Polygon` and `MultiPolygon`.
/// Difference involving points or linestrings returns an empty geometry.
pub fn difference(a: &Geometry, b: &Geometry) -> Geometry {
    let geo_a = geo_bridge::geometry_to_geo(a);
    let geo_b = geo_bridge::geometry_to_geo(b);
    let result = boolean_op(&geo_a, &geo_b, |x, y| x.difference(y));
    geo_bridge::multipolygon_to_geometry(&result)
}

/// Returns the symmetric difference of two geometries.
///
/// The symmetric difference is the set of all points that are in exactly
/// one of the geometries (union minus intersection).
/// Matches QGIS `QgsGeometry::symDifference`.
///
/// **Gap**: `geo`'s `BooleanOps` only supports `Polygon` and `MultiPolygon`.
/// Symmetric difference involving points or linestrings returns an empty
/// geometry.
pub fn symmetric_difference(a: &Geometry, b: &Geometry) -> Geometry {
    let geo_a = geo_bridge::geometry_to_geo(a);
    let geo_b = geo_bridge::geometry_to_geo(b);
    let result = boolean_op(&geo_a, &geo_b, |x, y| x.xor(y));
    geo_bridge::multipolygon_to_geometry(&result)
}

/// Perform a boolean operation on two geo geometries.
///
/// `BooleanOps` is only implemented for `Polygon` and `MultiPolygon` in geo.
/// For other geometry types, returns an empty `MultiPolygon`.
fn boolean_op<F>(a: &geo::Geometry, b: &geo::Geometry, op: F) -> geo::MultiPolygon
where
    F: Fn(&geo::Polygon, &geo::Polygon) -> geo::MultiPolygon,
{
    match (a, b) {
        (geo::Geometry::Polygon(pa), geo::Geometry::Polygon(pb)) => op(pa, pb),
        (geo::Geometry::Polygon(pa), geo::Geometry::MultiPolygon(_pb)) => {
            let pa_as_mp = geo::MultiPolygon::new(vec![pa.clone()]);
            boolean_op(&geo::Geometry::MultiPolygon(pa_as_mp), b, op)
        }
        (geo::Geometry::MultiPolygon(_pa), geo::Geometry::Polygon(pb)) => {
            let pb_as_mp = geo::MultiPolygon::new(vec![pb.clone()]);
            boolean_op(a, &geo::Geometry::MultiPolygon(pb_as_mp), op)
        }
        (geo::Geometry::MultiPolygon(pa), geo::Geometry::MultiPolygon(pb)) => {
            // Apply the operation polygon-by-polygon and collect results
            let mut result = geo::MultiPolygon::new(vec![]);
            for pa_poly in pa.iter() {
                for pb_poly in pb.iter() {
                    let partial = op(pa_poly, pb_poly);
                    result.0.extend(partial.0);
                }
            }
            result
        }
        _ => {
            // BooleanOps not implemented for these types — return empty
            geo::MultiPolygon::new(vec![])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linestring::LineString;
    use crate::point::Point;
    use crate::polygon::Polygon;

    fn sq(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
        vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
            Point::new(x0, y0),
        ]
    }

    fn unit_square() -> Geometry {
        Geometry::polygon(Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap())
    }

    fn area(g: &Geometry) -> f64 {
        match g {
            Geometry::Polygon(p) => p.area_abs(),
            Geometry::MultiPolygon(mp) => mp.total_area(),
            _ => 0.0,
        }
    }

    // --- union ---

    #[test]
    fn union_disjoint_polygons() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(2.0, 0.0, 3.0, 1.0)]).unwrap());
        let result = union(&unit_square(), &other);
        assert!((area(&result) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn union_overlapping_polygons() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        let result = union(&unit_square(), &other);
        // 1.0 + 2.25 - 0.25 = 3.0
        assert!((area(&result) - 3.0).abs() < 1e-10);
    }

    #[test]
    fn union_contained_polygon() {
        let inner = Geometry::polygon(Polygon::from_rings(vec![sq(0.25, 0.25, 0.75, 0.75)]).unwrap());
        let result = union(&unit_square(), &inner);
        assert!((area(&result) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn union_identical_polygons() {
        let result = union(&unit_square(), &unit_square());
        assert!((area(&result) - 1.0).abs() < 1e-10);
    }

    // --- intersection ---

    #[test]
    fn intersection_overlapping_polygons() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        let result = intersection(&unit_square(), &other);
        assert!((area(&result) - 0.25).abs() < 1e-10);
    }

    #[test]
    fn intersection_disjoint_polygons() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(5.0, 5.0, 6.0, 6.0)]).unwrap());
        let result = intersection(&unit_square(), &other);
        assert!(result.is_empty() || area(&result).abs() < 1e-10);
    }

    #[test]
    fn intersection_contained_polygon() {
        let inner = Geometry::polygon(Polygon::from_rings(vec![sq(0.25, 0.25, 0.75, 0.75)]).unwrap());
        let result = intersection(&unit_square(), &inner);
        assert!((area(&result) - 0.25).abs() < 1e-10);
    }

    // --- difference ---

    #[test]
    fn difference_overlapping_polygons() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        let result = difference(&unit_square(), &other);
        assert!((area(&result) - 0.75).abs() < 1e-10);
    }

    #[test]
    fn difference_disjoint_polygons() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(5.0, 5.0, 6.0, 6.0)]).unwrap());
        let result = difference(&unit_square(), &other);
        assert!((area(&result) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn difference_identical_polygons() {
        let result = difference(&unit_square(), &unit_square());
        assert!(result.is_empty() || area(&result).abs() < 1e-10);
    }

    // --- symmetric difference ---

    #[test]
    fn symmetric_difference_overlapping_polygons() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        let result = symmetric_difference(&unit_square(), &other);
        // union(3.0) - intersection(0.25) = 2.75
        assert!((area(&result) - 2.75).abs() < 1e-10);
    }

    #[test]
    fn symmetric_difference_disjoint_polygons() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(2.0, 0.0, 3.0, 1.0)]).unwrap());
        let result = symmetric_difference(&unit_square(), &other);
        assert!((area(&result) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn symmetric_difference_identical_polygons() {
        let result = symmetric_difference(&unit_square(), &unit_square());
        assert!(result.is_empty() || area(&result).abs() < 1e-10);
    }

    // --- cross-type overlay (gap: returns empty) ---

    #[test]
    fn intersection_polygon_and_linestring_returns_empty() {
        let line = Geometry::linestring(LineString::from_2d_coords(&[(0.5, 0.0), (0.5, 1.0)]));
        let result = intersection(&unit_square(), &line);
        // BooleanOps not implemented for LineString — returns empty
        assert!(result.is_empty() || area(&result).abs() < 1e-10);
    }

    #[test]
    fn union_polygon_and_point_returns_empty() {
        let pt = Geometry::point(Point::new(0.5, 0.5));
        let result = union(&unit_square(), &pt);
        // BooleanOps not implemented for Point — returns empty
        assert!(result.is_empty() || area(&result).abs() < 1e-10);
    }

    // --- algebraic properties ---

    #[test]
    fn union_is_commutative() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        let r1 = union(&unit_square(), &other);
        let r2 = union(&other, &unit_square());
        assert!((area(&r1) - area(&r2)).abs() < 1e-10);
    }

    #[test]
    fn intersection_is_commutative() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        let r1 = intersection(&unit_square(), &other);
        let r2 = intersection(&other, &unit_square());
        assert!((area(&r1) - area(&r2)).abs() < 1e-10);
    }

    #[test]
    fn symmetric_difference_is_commutative() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        let r1 = symmetric_difference(&unit_square(), &other);
        let r2 = symmetric_difference(&other, &unit_square());
        assert!((area(&r1) - area(&r2)).abs() < 1e-10);
    }

    #[test]
    fn difference_is_not_commutative() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        let r1 = difference(&unit_square(), &other);
        let r2 = difference(&other, &unit_square());
        // a-b = 1.0 - 0.25 = 0.75, b-a = 2.25 - 0.25 = 2.0
        assert!((area(&r1) - 0.75).abs() < 1e-10);
        assert!((area(&r2) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn union_intersection_identity() {
        // area(A ∪ B) = area(A) + area(B) - area(A ∩ B)
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        let u = area(&union(&unit_square(), &other));
        let i = area(&intersection(&unit_square(), &other));
        assert!((u - (1.0 + 2.25 - i)).abs() < 1e-10);
    }

    // --- multipolygon overlay ---

    #[test]
    fn union_multipolygon_and_polygon() {
        use crate::multi::MultiPolygon;
        let mp = Geometry::multi_polygon(MultiPolygon::new(vec![
            Polygon::from_rings(vec![sq(2.0, 0.0, 3.0, 1.0)]).unwrap(),
        ]));
        let result = union(&unit_square(), &mp);
        assert!((area(&result) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn intersection_multipolygon_and_polygon() {
        use crate::multi::MultiPolygon;
        let mp = Geometry::multi_polygon(MultiPolygon::new(vec![
            Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap(),
        ]));
        let result = intersection(&unit_square(), &mp);
        assert!((area(&result) - 0.25).abs() < 1e-10);
    }

    #[test]
    fn union_two_multipolygons() {
        use crate::multi::MultiPolygon;
        let mp1 = Geometry::multi_polygon(MultiPolygon::new(vec![
            Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap(),
        ]));
        let mp2 = Geometry::multi_polygon(MultiPolygon::new(vec![
            Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap(),
        ]));
        let result = union(&mp1, &mp2);
        assert!((area(&result) - 3.0).abs() < 1e-10);
    }
}
