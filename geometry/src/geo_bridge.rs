//! Conversion bridge between qgis-geometry types and `geo` crate types.
//!
//! The `geo` crate is the computational geometry backend (ADR-0001 exception:
//! pure-Rust, MIT/Apache-2.0, GPL-3.0-compatible). This module provides
//! bidirectional conversion so that predicates, overlay, and simplify
//! operations can delegate to geo's algorithms.
//!
//! Z and M coordinates are dropped during conversion to `geo` types (which
//! are 2D-only). This matches QGIS behavior where spatial predicates and
//! overlay operations are computed in 2D planar space.

use crate::collection::{Geometry, GeometryCollection};
use crate::linestring::LineString;
use crate::multi::{MultiLineString, MultiPoint, MultiPolygon};
use crate::point::Point;
use crate::polygon::Polygon;

// ---------------------------------------------------------------------------
// qgis-geometry → geo
// ---------------------------------------------------------------------------

/// Convert a [`Point`] to a `geo::Point` (2D only, Z/M dropped).
pub(crate) fn point_to_geo(p: &Point) -> geo::Point {
    geo::Point::new(p.x, p.y)
}

/// Convert a [`LineString`] to a `geo::LineString` (2D only).
pub(crate) fn linestring_to_geo(ls: &LineString) -> geo::LineString {
    geo::LineString::new(
        ls.points()
            .iter()
            .map(|p| geo::Coord { x: p.x, y: p.y })
            .collect(),
    )
}

/// Convert a [`Polygon`] to a `geo::Polygon` (2D only).
pub(crate) fn polygon_to_geo(poly: &Polygon) -> geo::Polygon {
    let exterior = geo::LineString::new(
        poly.exterior()
            .iter()
            .map(|p| geo::Coord { x: p.x, y: p.y })
            .collect(),
    );
    let interiors: Vec<geo::LineString> = poly
        .interiors()
        .iter()
        .map(|ring| {
            geo::LineString::new(
                ring.iter()
                    .map(|p| geo::Coord { x: p.x, y: p.y })
                    .collect(),
            )
        })
        .collect();
    geo::Polygon::new(exterior, interiors)
}

/// Convert a [`MultiPoint`] to a `geo::MultiPoint` (2D only).
pub(crate) fn multipoint_to_geo(mp: &MultiPoint) -> geo::MultiPoint {
    geo::MultiPoint::new(mp.points().iter().map(point_to_geo).collect())
}

/// Convert a [`MultiLineString`] to a `geo::MultiLineString` (2D only).
pub(crate) fn multilinestring_to_geo(mls: &MultiLineString) -> geo::MultiLineString {
    geo::MultiLineString::new(mls.lines().iter().map(linestring_to_geo).collect())
}

/// Convert a [`MultiPolygon`] to a `geo::MultiPolygon` (2D only).
pub(crate) fn multipolygon_to_geo(mp: &MultiPolygon) -> geo::MultiPolygon {
    geo::MultiPolygon::new(mp.polygons().iter().map(polygon_to_geo).collect())
}

/// Convert a [`GeometryCollection`] to a `geo::GeometryCollection` (2D only).
pub(crate) fn geometry_collection_to_geo(gc: &GeometryCollection) -> geo::GeometryCollection {
    geo::GeometryCollection::new_from(
        gc.geometries()
            .iter()
            .map(geometry_to_geo)
            .collect::<Vec<_>>(),
    )
}

/// Convert a [`Geometry`] enum to a `geo::Geometry` (2D only).
pub(crate) fn geometry_to_geo(g: &Geometry) -> geo::Geometry {
    match g {
        Geometry::Point(p) => geo::Geometry::Point(point_to_geo(p)),
        Geometry::LineString(ls) => geo::Geometry::LineString(linestring_to_geo(ls)),
        Geometry::Polygon(p) => geo::Geometry::Polygon(polygon_to_geo(p)),
        Geometry::MultiPoint(mp) => geo::Geometry::MultiPoint(multipoint_to_geo(mp)),
        Geometry::MultiLineString(mls) => geo::Geometry::MultiLineString(multilinestring_to_geo(mls)),
        Geometry::MultiPolygon(mp) => geo::Geometry::MultiPolygon(multipolygon_to_geo(mp)),
        Geometry::GeometryCollection(gc) => {
            geo::Geometry::GeometryCollection(geometry_collection_to_geo(gc))
        }
    }
}

// ---------------------------------------------------------------------------
// geo → qgis-geometry
// ---------------------------------------------------------------------------

/// Convert a `geo::Point` to a [`Point`] (2D only, no Z/M).
pub(crate) fn point_from_geo(p: &geo::Point) -> Point {
    Point::new(p.x(), p.y())
}

/// Convert a `geo::LineString` to a [`LineString`] (2D only).
pub(crate) fn linestring_from_geo(ls: &geo::LineString) -> LineString {
    LineString::new(
        ls.coords()
            .map(|c| Point::new(c.x, c.y))
            .collect(),
    )
}

/// Convert a `geo::Polygon` to a [`Polygon`] (2D only).
pub(crate) fn polygon_from_geo(poly: &geo::Polygon) -> Polygon {
    let exterior: Vec<Point> = poly
        .exterior()
        .coords()
        .map(|c| Point::new(c.x, c.y))
        .collect();
    let interiors: Vec<Vec<Point>> = poly
        .interiors()
        .iter()
        .map(|ring| ring.coords().map(|c| Point::new(c.x, c.y)).collect())
        .collect();
    // geo polygons always have valid closed rings; from_rings should succeed.
    Polygon::from_rings(
        std::iter::once(exterior)
            .chain(interiors)
            .collect(),
    )
    .expect("geo polygon rings are valid")
}

/// Convert a `geo::MultiPoint` to a [`MultiPoint`] (2D only).
pub(crate) fn multipoint_from_geo(mp: &geo::MultiPoint) -> MultiPoint {
    MultiPoint::new(mp.0.iter().map(point_from_geo).collect())
}

/// Convert a `geo::MultiLineString` to a [`MultiLineString`] (2D only).
pub(crate) fn multilinestring_from_geo(mls: &geo::MultiLineString) -> MultiLineString {
    MultiLineString::new(mls.0.iter().map(linestring_from_geo).collect())
}

/// Convert a `geo::MultiPolygon` to a [`MultiPolygon`] (2D only).
pub(crate) fn multipolygon_from_geo(mp: &geo::MultiPolygon) -> MultiPolygon {
    MultiPolygon::new(mp.iter().map(polygon_from_geo).collect())
}

/// Convert a `geo::GeometryCollection` to a [`GeometryCollection`] (2D only).
pub(crate) fn geometry_collection_from_geo(gc: &geo::GeometryCollection) -> GeometryCollection {
    GeometryCollection::new(
        gc.iter()
            .map(geometry_from_geo)
            .collect(),
    )
}

/// Convert a `geo::Geometry` to a [`Geometry`] enum (2D only).
pub(crate) fn geometry_from_geo(g: &geo::Geometry) -> Geometry {
    match g {
        geo::Geometry::Point(p) => Geometry::point(point_from_geo(p)),
        geo::Geometry::LineString(ls) => Geometry::linestring(linestring_from_geo(ls)),
        geo::Geometry::Polygon(p) => Geometry::polygon(polygon_from_geo(p)),
        geo::Geometry::MultiPoint(mp) => Geometry::multi_point(multipoint_from_geo(mp)),
        geo::Geometry::MultiLineString(mls) => {
            Geometry::multi_linestring(multilinestring_from_geo(mls))
        }
        geo::Geometry::MultiPolygon(mp) => Geometry::multi_polygon(multipolygon_from_geo(mp)),
        geo::Geometry::GeometryCollection(gc) => {
            Geometry::collection(geometry_collection_from_geo(gc))
        }
        geo::Geometry::Line(l) => Geometry::linestring(linestring_from_geo(
            &geo::LineString::from(vec![l.start, l.end]),
        )),
        geo::Geometry::Rect(r) => {
            let poly = geo::Polygon::from(*r);
            Geometry::polygon(polygon_from_geo(&poly))
        }
        geo::Geometry::Triangle(t) => {
            let poly = geo::Polygon::from(*t);
            Geometry::polygon(polygon_from_geo(&poly))
        }
    }
}

/// Convert a `geo::MultiPolygon` to a [`Geometry`] enum.
///
/// Returns a [`Geometry::MultiPolygon`] if multiple polygons, or a
/// [`Geometry::Polygon`] if exactly one. Returns
/// [`Geometry::GeometryCollection`] with zero polygons if empty.
pub(crate) fn multipolygon_to_geometry(mp: &geo::MultiPolygon) -> Geometry {
    match mp.0.len() {
        0 => Geometry::collection(GeometryCollection::new(vec![])),
        1 => Geometry::polygon(polygon_from_geo(&mp.0[0])),
        _ => Geometry::multi_polygon(multipolygon_from_geo(mp)),
    }
}
