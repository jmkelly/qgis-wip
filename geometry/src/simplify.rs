//! Simplify operation using the Ramer–Douglas–Peucker algorithm.
//!
//! Delegates to the `geo` crate's `Simplify` trait. The algorithm removes
//! vertices that deviate less than the tolerance from the line connecting
//! their neighbors, preserving the overall shape.
//!
//! JTS/GEOS gaps:
//! * `geo` uses the standard Ramer–Douglas–Peucker algorithm. QGIS/GEOS
//!   uses the same algorithm but may have slightly different handling of
//!   degenerate cases (e.g. closed rings, very small tolerances).
//! * `geo` does not preserve topology between polygons (i.e. shared
//!   boundaries may diverge after independent simplification). QGIS offers
//!   `topologyPreserveSimplify` for this; we do not.
//! * `geo`'s `Simplify` is only implemented for `LineString`,
//!   `MultiLineString`, `Polygon`, and `MultiPolygon`. Simplifying a
//!   `Point` or `MultiPoint` returns the original geometry unchanged.

use crate::collection::Geometry;
use crate::geo_bridge;
use geo::{Simplify, SimplifyVw};

/// Returns a simplified copy of the geometry using the
/// Ramer–Douglas–Peucker algorithm.
///
/// `tolerance` is the maximum allowed deviation from the original geometry.
/// Larger values remove more vertices. A tolerance of 0.0 returns a copy
/// of the original geometry.
///
/// Matches QGIS `QgsGeometry::simplify(tolerance)`.
pub fn simplify(geometry: &Geometry, tolerance: f64) -> Geometry {
    if tolerance <= 0.0 {
        return geometry.clone();
    }
    let geo_geom = geo_bridge::geometry_to_geo(geometry);
    let simplified = simplify_geo(&geo_geom, tolerance);
    geo_bridge::geometry_from_geo(&simplified)
}

/// Returns a simplified copy of the geometry using the
/// Visvalingam–Whyatt algorithm.
///
/// This algorithm removes vertices based on the effective area of the
/// triangle formed with its neighbors, which often produces more
/// aesthetically pleasing results than Ramer–Douglas–Peucker.
///
/// `tolerance` is the minimum effective area threshold. Vertices whose
/// triangle area is below this threshold are removed.
///
/// Matches QGIS `QgsGeometry::simplify` with the VW variant.
pub fn simplify_vw(geometry: &Geometry, tolerance: f64) -> Geometry {
    if tolerance <= 0.0 {
        return geometry.clone();
    }
    let geo_geom = geo_bridge::geometry_to_geo(geometry);
    let simplified = simplify_vw_geo(&geo_geom, tolerance);
    geo_bridge::geometry_from_geo(&simplified)
}

/// Simplify a `geo::Geometry` using Ramer–Douglas–Peucker.
///
/// `Simplify` is implemented for `LineString`, `MultiLineString`,
/// `Polygon`, and `MultiPolygon`. Other types are returned unchanged.
fn simplify_geo(g: &geo::Geometry, tolerance: f64) -> geo::Geometry {
    match g {
        geo::Geometry::LineString(ls) => geo::Geometry::LineString(ls.simplify(&tolerance)),
        geo::Geometry::MultiLineString(mls) => {
            geo::Geometry::MultiLineString(mls.simplify(&tolerance))
        }
        geo::Geometry::Polygon(p) => geo::Geometry::Polygon(p.simplify(&tolerance)),
        geo::Geometry::MultiPolygon(mp) => geo::Geometry::MultiPolygon(mp.simplify(&tolerance)),
        other => other.clone(),
    }
}

/// Simplify a `geo::Geometry` using Visvalingam–Whyatt.
///
/// `SimplifyVw` is implemented for `LineString`, `MultiLineString`,
/// `Polygon`, and `MultiPolygon`. Other types are returned unchanged.
fn simplify_vw_geo(g: &geo::Geometry, tolerance: f64) -> geo::Geometry {
    match g {
        geo::Geometry::LineString(ls) => geo::Geometry::LineString(ls.simplify_vw(&tolerance)),
        geo::Geometry::MultiLineString(mls) => {
            geo::Geometry::MultiLineString(mls.simplify_vw(&tolerance))
        }
        geo::Geometry::Polygon(p) => geo::Geometry::Polygon(p.simplify_vw(&tolerance)),
        geo::Geometry::MultiPolygon(mp) => geo::Geometry::MultiPolygon(mp.simplify_vw(&tolerance)),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn simplify_zero_tolerance_returns_copy() {
        let poly = Geometry::polygon(Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap());
        let result = simplify(&poly, 0.0);
        assert_eq!(poly, result);
    }

    #[test]
    fn simplify_negative_tolerance_returns_copy() {
        let poly = Geometry::polygon(Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap());
        let result = simplify(&poly, -1.0);
        assert_eq!(poly, result);
    }

    #[test]
    fn simplify_removes_collinear_points() {
        // A square with extra collinear points on each edge
        let ring = vec![
            Point::new(0.0, 0.0),
            Point::new(0.5, 0.0), // collinear
            Point::new(1.0, 0.0),
            Point::new(1.0, 0.5), // collinear
            Point::new(1.0, 1.0),
            Point::new(0.5, 1.0), // collinear
            Point::new(0.0, 1.0),
            Point::new(0.0, 0.5), // collinear
            Point::new(0.0, 0.0),
        ];
        let poly = Geometry::polygon(Polygon::from_rings(vec![ring]).unwrap());
        let result = simplify(&poly, 0.01);
        match &result {
            Geometry::Polygon(p) => {
                // Should have removed the collinear points
                assert!(p.exterior().len() < 9, "expected fewer than 9 vertices, got {}", p.exterior().len());
            }
            _ => panic!("expected polygon"),
        }
    }

    #[test]
    fn simplify_preserves_area() {
        let ring = vec![
            Point::new(0.0, 0.0),
            Point::new(0.5, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 0.5),
            Point::new(1.0, 1.0),
            Point::new(0.5, 1.0),
            Point::new(0.0, 1.0),
            Point::new(0.0, 0.5),
            Point::new(0.0, 0.0),
        ];
        let poly = Geometry::polygon(Polygon::from_rings(vec![ring]).unwrap());
        let result = simplify(&poly, 0.01);
        match &result {
            Geometry::Polygon(p) => {
                assert!((p.area_abs() - 1.0).abs() < 1e-10);
            }
            _ => panic!("expected polygon"),
        }
    }

    #[test]
    fn simplify_linestring() {
        let ls = Geometry::linestring(LineString::from_2d_coords(&[
            (0.0, 0.0),
            (0.5, 0.0),
            (1.0, 0.0),
            (1.0, 1.0),
        ]));
        let result = simplify(&ls, 0.01);
        match &result {
            Geometry::LineString(ls) => {
                assert!(ls.len() < 4, "expected fewer than 4 points, got {}", ls.len());
            }
            _ => panic!("expected linestring"),
        }
    }

    #[test]
    fn simplify_point() {
        let pt = Geometry::point(Point::new(1.0, 2.0));
        let result = simplify(&pt, 1.0);
        assert!(matches!(result, Geometry::Point(_)));
    }

    #[test]
    fn simplify_multipolygon() {
        let mp = Geometry::multi_polygon(MultiPolygon::new(vec![
            Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap(),
            Polygon::from_rings(vec![sq(2.0, 2.0, 3.0, 3.0)]).unwrap(),
        ]));
        let result = simplify(&mp, 0.01);
        match &result {
            Geometry::MultiPolygon(mp) => {
                assert_eq!(mp.len(), 2);
            }
            _ => panic!("expected multipolygon"),
        }
    }

    #[test]
    fn simplify_vw_zero_tolerance_returns_copy() {
        let poly = Geometry::polygon(Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap());
        let result = simplify_vw(&poly, 0.0);
        assert_eq!(poly, result);
    }

    #[test]
    fn simplify_vw_removes_low_area_vertices() {
        // A square with a small bump
        let ring = vec![
            Point::new(0.0, 0.0),
            Point::new(0.5, 0.01), // small bump
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
            Point::new(0.0, 0.0),
        ];
        let poly = Geometry::polygon(Polygon::from_rings(vec![ring]).unwrap());
        let result = simplify_vw(&poly, 0.1);
        match &result {
            Geometry::Polygon(p) => {
                assert!(p.exterior().len() < 6, "expected fewer than 6 vertices, got {}", p.exterior().len());
            }
            _ => panic!("expected polygon"),
        }
    }

    #[test]
    fn simplify_vw_preserves_shape() {
        let ring = vec![
            Point::new(0.0, 0.0),
            Point::new(0.5, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 0.5),
            Point::new(1.0, 1.0),
            Point::new(0.5, 1.0),
            Point::new(0.0, 1.0),
            Point::new(0.0, 0.5),
            Point::new(0.0, 0.0),
        ];
        let poly = Geometry::polygon(Polygon::from_rings(vec![ring]).unwrap());
        let result = simplify_vw(&poly, 0.001);
        match &result {
            Geometry::Polygon(p) => {
                assert!((p.area_abs() - 1.0).abs() < 1e-10);
            }
            _ => panic!("expected polygon"),
        }
    }

    #[test]
    fn simplify_empty_polygon() {
        let poly = Geometry::polygon(Polygon::new(vec![]));
        let result = simplify(&poly, 1.0);
        assert!(result.is_empty());
    }

    #[test]
    fn simplify_multilinestring() {
        let mls = Geometry::multi_linestring(MultiLineString::new(vec![
            LineString::from_2d_coords(&[(0.0, 0.0), (0.5, 0.0), (1.0, 0.0)]),
            LineString::from_2d_coords(&[(0.0, 1.0), (0.5, 1.0), (1.0, 1.0)]),
        ]));
        let result = simplify(&mls, 0.01);
        match &result {
            Geometry::MultiLineString(mls) => {
                assert_eq!(mls.len(), 2);
                for ls in mls.lines() {
                    assert!(ls.len() < 3, "expected fewer than 3 points, got {}", ls.len());
                }
            }
            _ => panic!("expected multilinestring"),
        }
    }

    #[test]
    fn simplify_multipoint() {
        let mp = Geometry::multi_point(MultiPoint::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
        ]));
        let result = simplify(&mp, 1.0);
        match &result {
            Geometry::MultiPoint(mp) => {
                assert_eq!(mp.len(), 2);
            }
            _ => panic!("expected multipoint"),
        }
    }
}
