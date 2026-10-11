//! Spatial predicates: contains, intersects, within, crosses, touches,
//! overlaps, disjoint, equals.
//!
//! Delegates to the `geo` crate's DE-9IM relational engine (`Relate` trait).
//! All predicates are computed in 2D planar space (Z/M dropped), matching
//! QGIS behavior where GEOS/JTS predicates ignore Z.
//!
//! JTS/GEOS gaps:
//! * `geo` uses a different tolerance model than GEOS for boundary
//!   classification. Degenerate cases (e.g. point-on-vertex) may produce
//!   different results between geo and JTS.
//! * `geo` does not implement the full JTS `relate` with all edge cases
//!   for GeometryCollections containing mixed dimensions.

use crate::collection::Geometry;
use crate::geo_bridge;
use geo::{Contains, Intersects, Relate, Within};

/// Returns true if geometry `a` contains geometry `b`.
///
/// `a` contains `b` if no points of `b` lie in the exterior of `a` and at
/// least one point of the interior of `b` lies in the interior of `a`.
/// Matches QGIS `QgsGeometry::contains`.
pub fn contains(a: &Geometry, b: &Geometry) -> bool {
    let geo_a = geo_bridge::geometry_to_geo(a);
    let geo_b = geo_bridge::geometry_to_geo(b);
    geo_a.contains(&geo_b)
}

/// Returns true if geometry `a` is within geometry `b`.
///
/// `a` is within `b` if `b` contains `a`. Matches QGIS
/// `QgsGeometry::within`.
pub fn within(a: &Geometry, b: &Geometry) -> bool {
    let geo_a = geo_bridge::geometry_to_geo(a);
    let geo_b = geo_bridge::geometry_to_geo(b);
    geo_a.is_within(&geo_b)
}

/// Returns true if geometry `a` intersects geometry `b`.
///
/// Geometries intersect if they have at least one point in common.
/// Matches QGIS `QgsGeometry::intersects`.
pub fn intersects(a: &Geometry, b: &Geometry) -> bool {
    let geo_a = geo_bridge::geometry_to_geo(a);
    let geo_b = geo_bridge::geometry_to_geo(b);
    geo_a.intersects(&geo_b)
}

/// Returns true if geometry `a` is disjoint from geometry `b`.
///
/// Geometries are disjoint if they have no points in common.
/// Matches QGIS `QgsGeometry::disjoint`.
pub fn disjoint(a: &Geometry, b: &Geometry) -> bool {
    !intersects(a, b)
}

/// Returns true if geometry `a` crosses geometry `b`.
///
/// Crosses applies to:
/// * Line/Line: they cross at a point (not along a line)
/// * Line/Polygon: the line crosses the polygon boundary
/// * Line/Point: never
/// * Polygon/Polygon: never (use overlaps)
///
/// Matches QGIS `QgsGeometry::crosses`.
pub fn crosses(a: &Geometry, b: &Geometry) -> bool {
    let geo_a = geo_bridge::geometry_to_geo(a);
    let geo_b = geo_bridge::geometry_to_geo(b);
    let matrix = geo_a.relate(&geo_b);
    matrix.is_crosses()
}

/// Returns true if geometry `a` touches geometry `b`.
///
/// Geometries touch if they have at least one boundary point in common but
/// their interiors do not intersect. Matches QGIS `QgsGeometry::touches`.
pub fn touches(a: &Geometry, b: &Geometry) -> bool {
    let geo_a = geo_bridge::geometry_to_geo(a);
    let geo_b = geo_bridge::geometry_to_geo(b);
    let matrix = geo_a.relate(&geo_b);
    matrix.is_touches()
}

/// Returns true if geometry `a` overlaps geometry `b`.
///
/// Overlaps applies to geometries of the same dimension that intersect but
/// neither contains the other. Matches QGIS `QgsGeometry::overlaps`.
pub fn overlaps(a: &Geometry, b: &Geometry) -> bool {
    let geo_a = geo_bridge::geometry_to_geo(a);
    let geo_b = geo_bridge::geometry_to_geo(b);
    let matrix = geo_a.relate(&geo_b);
    matrix.is_overlaps()
}

/// Returns true if geometry `a` equals geometry `b` topologically.
///
/// Topological equality: the geometries have the same interior, boundary,
/// and exterior. Matches QGIS `QgsGeometry::equals`.
pub fn equals(a: &Geometry, b: &Geometry) -> bool {
    let geo_a = geo_bridge::geometry_to_geo(a);
    let geo_b = geo_bridge::geometry_to_geo(b);
    let matrix = geo_a.relate(&geo_b);
    matrix.is_equal_topo()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::GeometryCollection;
    use crate::linestring::LineString;
    use crate::multi::{MultiLineString, MultiPoint, MultiPolygon};
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

    fn big_square() -> Geometry {
        Geometry::polygon(Polygon::from_rings(vec![sq(-1.0, -1.0, 2.0, 2.0)]).unwrap())
    }

    // --- contains ---

    #[test]
    fn contains_polygon_contains_polygon() {
        assert!(contains(&big_square(), &unit_square()));
    }

    #[test]
    fn contains_polygon_not_contains_disjoint() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(5.0, 5.0, 6.0, 6.0)]).unwrap());
        assert!(!contains(&unit_square(), &other));
    }

    #[test]
    fn contains_polygon_contains_point() {
        let pt = Geometry::point(Point::new(0.5, 0.5));
        assert!(contains(&unit_square(), &pt));
    }

    #[test]
    fn contains_polygon_not_contains_point_outside() {
        let pt = Geometry::point(Point::new(5.0, 5.0));
        assert!(!contains(&unit_square(), &pt));
    }

    #[test]
    fn contains_not_reflexive() {
        // A contains B does not imply B contains A
        assert!(contains(&big_square(), &unit_square()));
        assert!(!contains(&unit_square(), &big_square()));
    }

    // --- within ---

    #[test]
    fn within_polygon_within_polygon() {
        assert!(within(&unit_square(), &big_square()));
    }

    #[test]
    fn within_is_reverse_of_contains() {
        assert!(within(&unit_square(), &big_square()));
        assert!(contains(&big_square(), &unit_square()));
    }

    #[test]
    fn within_not_reflexive() {
        assert!(within(&unit_square(), &big_square()));
        assert!(!within(&big_square(), &unit_square()));
    }

    // --- intersects ---

    #[test]
    fn intersects_overlapping_polygons() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        assert!(intersects(&unit_square(), &other));
    }

    #[test]
    fn intersects_disjoint_polygons() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(5.0, 5.0, 6.0, 6.0)]).unwrap());
        assert!(!intersects(&unit_square(), &other));
    }

    #[test]
    fn intersects_adjacent_polygons_share_boundary() {
        // Adjacent polygons share boundary points, so they DO intersect
        let right = Geometry::polygon(Polygon::from_rings(vec![sq(1.0, 0.0, 2.0, 1.0)]).unwrap());
        assert!(intersects(&unit_square(), &right));
    }

    #[test]
    fn intersects_polygon_contains_point() {
        let pt = Geometry::point(Point::new(0.5, 0.5));
        assert!(intersects(&unit_square(), &pt));
    }

    // --- disjoint ---

    #[test]
    fn disjoint_disjoint_polygons() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(5.0, 5.0, 6.0, 6.0)]).unwrap());
        assert!(disjoint(&unit_square(), &other));
    }

    #[test]
    fn disjoint_overlapping_polygons() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        assert!(!disjoint(&unit_square(), &other));
    }

    // --- crosses ---

    #[test]
    fn crosses_crossing_lines() {
        let line_a = Geometry::linestring(LineString::from_2d_coords(&[(-1.0, 0.5), (2.0, 0.5)]));
        let line_b = Geometry::linestring(LineString::from_2d_coords(&[(0.5, -1.0), (0.5, 2.0)]));
        assert!(crosses(&line_a, &line_b));
    }

    #[test]
    fn crosses_parallel_lines_do_not_cross() {
        let line_a = Geometry::linestring(LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 0.0)]));
        let line_b = Geometry::linestring(LineString::from_2d_coords(&[(0.0, 1.0), (1.0, 1.0)]));
        assert!(!crosses(&line_a, &line_b));
    }

    #[test]
    fn crosses_line_through_polygon() {
        let line = Geometry::linestring(LineString::from_2d_coords(&[(-1.0, 0.5), (2.0, 0.5)]));
        assert!(crosses(&line, &unit_square()));
    }

    #[test]
    fn crosses_polygon_polygon_never_crosses() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        assert!(!crosses(&unit_square(), &other));
    }

    // --- touches ---

    #[test]
    fn touches_adjacent_polygons() {
        let right = Geometry::polygon(Polygon::from_rings(vec![sq(1.0, 0.0, 2.0, 1.0)]).unwrap());
        assert!(touches(&unit_square(), &right));
    }

    #[test]
    fn touches_overlapping_polygons_do_not_touch() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        assert!(!touches(&unit_square(), &other));
    }

    #[test]
    fn touches_point_on_boundary() {
        let pt = Geometry::point(Point::new(1.0, 0.5));
        assert!(touches(&unit_square(), &pt));
    }

    #[test]
    fn touches_shared_vertex_polygons() {
        let diag = Geometry::polygon(Polygon::from_rings(vec![sq(1.0, 1.0, 2.0, 2.0)]).unwrap());
        assert!(touches(&unit_square(), &diag));
    }

    // --- overlaps ---

    #[test]
    fn overlaps_overlapping_polygons() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        assert!(overlaps(&unit_square(), &other));
    }

    #[test]
    fn overlaps_contained_polygons_do_not_overlap() {
        assert!(!overlaps(&big_square(), &unit_square()));
    }

    #[test]
    fn overlaps_collinear_lines() {
        let line_a = Geometry::linestring(LineString::from_2d_coords(&[(0.0, 0.0), (2.0, 0.0)]));
        let line_b = Geometry::linestring(LineString::from_2d_coords(&[(1.0, 0.0), (3.0, 0.0)]));
        assert!(overlaps(&line_a, &line_b));
    }

    // --- equals ---

    #[test]
    fn equals_identical_polygons() {
        assert!(equals(&unit_square(), &unit_square()));
    }

    #[test]
    fn equals_different_polygons() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.0, 0.0, 2.0, 2.0)]).unwrap());
        assert!(!equals(&unit_square(), &other));
    }

    #[test]
    fn equals_reflexive() {
        assert!(equals(&unit_square(), &unit_square()));
    }

    #[test]
    fn equals_identical_points() {
        let a = Geometry::point(Point::new(1.0, 2.0));
        let b = Geometry::point(Point::new(1.0, 2.0));
        assert!(equals(&a, &b));
    }

    #[test]
    fn equals_different_points() {
        let a = Geometry::point(Point::new(1.0, 2.0));
        let b = Geometry::point(Point::new(3.0, 4.0));
        assert!(!equals(&a, &b));
    }

    // --- cross-type predicates ---

    #[test]
    fn intersects_polygon_and_linestring_crossing() {
        let line = Geometry::linestring(LineString::from_2d_coords(&[(-1.0, 0.5), (2.0, 0.5)]));
        assert!(intersects(&unit_square(), &line));
    }

    #[test]
    fn contains_polygon_contains_linestring() {
        let line = Geometry::linestring(LineString::from_2d_coords(&[(0.25, 0.25), (0.75, 0.75)]));
        assert!(contains(&unit_square(), &line));
    }

    #[test]
    fn contains_multipoint() {
        let mp = Geometry::multi_point(MultiPoint::new(vec![
            Point::new(0.25, 0.25),
            Point::new(0.75, 0.75),
        ]));
        assert!(contains(&unit_square(), &mp));
    }

    #[test]
    fn intersects_multipolygon() {
        let mp = Geometry::multi_polygon(MultiPolygon::new(vec![
            Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap(),
        ]));
        assert!(intersects(&unit_square(), &mp));
    }

    #[test]
    fn equals_geometry_collections() {
        let gc1 = Geometry::collection(GeometryCollection::new(vec![
            Geometry::point(Point::new(0.0, 0.0)),
        ]));
        let gc2 = Geometry::collection(GeometryCollection::new(vec![
            Geometry::point(Point::new(0.0, 0.0)),
        ]));
        assert!(equals(&gc1, &gc2));
    }

    #[test]
    fn disjoint_empty_polygon() {
        let empty = Geometry::polygon(Polygon::new(vec![]));
        assert!(disjoint(&empty, &unit_square()));
    }

    #[test]
    fn intersects_empty_polygon() {
        let empty = Geometry::polygon(Polygon::new(vec![]));
        assert!(!intersects(&empty, &unit_square()));
    }

    // --- symmetric predicates ---

    #[test]
    fn intersects_is_symmetric() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        assert_eq!(intersects(&unit_square(), &other), intersects(&other, &unit_square()));
    }

    #[test]
    fn disjoint_is_symmetric() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(5.0, 5.0, 6.0, 6.0)]).unwrap());
        assert_eq!(disjoint(&unit_square(), &other), disjoint(&other, &unit_square()));
    }

    #[test]
    fn equals_is_symmetric() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        assert_eq!(equals(&unit_square(), &other), equals(&other, &unit_square()));
    }

    #[test]
    fn touches_is_symmetric() {
        let right = Geometry::polygon(Polygon::from_rings(vec![sq(1.0, 0.0, 2.0, 1.0)]).unwrap());
        assert_eq!(touches(&unit_square(), &right), touches(&right, &unit_square()));
    }

    #[test]
    fn overlaps_is_symmetric() {
        let other = Geometry::polygon(Polygon::from_rings(vec![sq(0.5, 0.5, 2.0, 2.0)]).unwrap());
        assert_eq!(overlaps(&unit_square(), &other), overlaps(&other, &unit_square()));
    }

    // --- multi-geometry types ---

    #[test]
    fn contains_multilinestring() {
        let mls = Geometry::multi_linestring(MultiLineString::new(vec![
            LineString::from_2d_coords(&[(0.1, 0.1), (0.5, 0.5)]),
            LineString::from_2d_coords(&[(0.2, 0.8), (0.9, 0.2)]),
        ]));
        assert!(contains(&unit_square(), &mls));
    }

    #[test]
    fn crosses_multilinestring_with_polygon() {
        let mls = Geometry::multi_linestring(MultiLineString::new(vec![
            LineString::from_2d_coords(&[(-1.0, 0.5), (2.0, 0.5)]),
        ]));
        assert!(crosses(&mls, &unit_square()));
    }

    #[test]
    fn overlaps_multilinestring_and_polygon_is_false() {
        // Overlaps requires same dimension; line (1D) vs polygon (2D) never overlaps
        let mls = Geometry::multi_linestring(MultiLineString::new(vec![
            LineString::from_2d_coords(&[(0.5, 0.0), (0.5, 1.0)]),
        ]));
        assert!(!overlaps(&mls, &unit_square()));
    }

    #[test]
    fn contains_multipolygon_within() {
        let mp = Geometry::multi_polygon(MultiPolygon::new(vec![
            Polygon::from_rings(vec![sq(0.1, 0.1, 0.4, 0.4)]).unwrap(),
            Polygon::from_rings(vec![sq(0.6, 0.6, 0.9, 0.9)]).unwrap(),
        ]));
        assert!(contains(&unit_square(), &mp));
    }

    #[test]
    fn within_multilinestring() {
        let mls = Geometry::multi_linestring(MultiLineString::new(vec![
            LineString::from_2d_coords(&[(0.1, 0.1), (0.5, 0.5)]),
        ]));
        assert!(within(&mls, &unit_square()));
    }

    #[test]
    fn equals_multilinestring() {
        let mls1 = Geometry::multi_linestring(MultiLineString::new(vec![
            LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)]),
        ]));
        let mls2 = Geometry::multi_linestring(MultiLineString::new(vec![
            LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)]),
        ]));
        assert!(equals(&mls1, &mls2));
    }

    #[test]
    fn equals_multipoint() {
        let mp1 = Geometry::multi_point(MultiPoint::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
        ]));
        let mp2 = Geometry::multi_point(MultiPoint::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
        ]));
        assert!(equals(&mp1, &mp2));
    }

    #[test]
    fn touches_multilinestring_and_polygon() {
        // MultiLineString with a line on the polygon boundary
        let mls = Geometry::multi_linestring(MultiLineString::new(vec![
            LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 0.0)]),
        ]));
        assert!(touches(&mls, &unit_square()));
    }

    #[test]
    fn disjoint_multilinestring_and_polygon() {
        let mls = Geometry::multi_linestring(MultiLineString::new(vec![
            LineString::from_2d_coords(&[(5.0, 5.0), (6.0, 6.0)]),
        ]));
        assert!(disjoint(&mls, &unit_square()));
    }
}
