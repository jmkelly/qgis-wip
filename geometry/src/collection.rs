//! GeometryCollection and the Geometry enum.
//!
//! Matches QGIS `QgsGeometryCollection` and `QgsGeometry`. The [`Geometry`]
//! enum is the top-level type that can represent any geometry kind, similar
//! to how QGIS `QgsGeometry` can hold any geometry type.

use crate::linestring::LineString;
use crate::multi::{MultiLineString, MultiPoint, MultiPolygon};
use crate::point::{Dimension, Point};
use crate::polygon::Polygon;

/// A collection of heterogeneous geometries.
///
/// Matches QGIS `QgsGeometryCollection`. Can contain any mix of geometry
/// types (points, lines, polygons, other collections).
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryCollection {
    geometries: Vec<Geometry>,
}

impl GeometryCollection {
    /// Creates a new geometry collection from a vector of geometries.
    pub fn new(geometries: Vec<Geometry>) -> Self {
        Self { geometries }
    }

    /// Returns the geometries in this collection.
    pub fn geometries(&self) -> &[Geometry] {
        &self.geometries
    }

    /// Returns the number of geometries.
    pub fn len(&self) -> usize {
        self.geometries.len()
    }

    /// Returns true if this collection has no geometries.
    pub fn is_empty(&self) -> bool {
        self.geometries.is_empty()
    }

    /// Returns true if any geometry in this collection has a Z coordinate.
    pub fn has_z(&self) -> bool {
        self.geometries.iter().any(|g| g.has_z())
    }

    /// Returns true if any geometry in this collection has an M coordinate.
    pub fn has_m(&self) -> bool {
        self.geometries.iter().any(|g| g.has_m())
    }

    /// Returns the dimensionality of this collection.
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

/// The top-level geometry enum — can represent any geometry type.
///
/// Matches QGIS `QgsGeometry` which can hold any geometry type. Each
/// variant wraps the corresponding geometry struct.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    /// A single point.
    Point(Point),
    /// A line string.
    LineString(LineString),
    /// A polygon.
    Polygon(Polygon),
    /// A collection of points.
    MultiPoint(MultiPoint),
    /// A collection of line strings.
    MultiLineString(MultiLineString),
    /// A collection of polygons.
    MultiPolygon(MultiPolygon),
    /// A collection of heterogeneous geometries.
    GeometryCollection(GeometryCollection),
}

impl Geometry {
    /// Creates a geometry from a point.
    pub fn point(p: Point) -> Self {
        Geometry::Point(p)
    }

    /// Creates a geometry from a line string.
    pub fn linestring(ls: LineString) -> Self {
        Geometry::LineString(ls)
    }

    /// Creates a geometry from a polygon.
    pub fn polygon(p: Polygon) -> Self {
        Geometry::Polygon(p)
    }

    /// Creates a geometry from a multi-point.
    pub fn multi_point(mp: MultiPoint) -> Self {
        Geometry::MultiPoint(mp)
    }

    /// Creates a geometry from a multi-line-string.
    pub fn multi_linestring(mls: MultiLineString) -> Self {
        Geometry::MultiLineString(mls)
    }

    /// Creates a geometry from a multi-polygon.
    pub fn multi_polygon(mp: MultiPolygon) -> Self {
        Geometry::MultiPolygon(mp)
    }

    /// Creates a geometry from a geometry collection.
    pub fn collection(gc: GeometryCollection) -> Self {
        Geometry::GeometryCollection(gc)
    }

    /// Returns true if this geometry has a Z coordinate.
    pub fn has_z(&self) -> bool {
        match self {
            Geometry::Point(p) => p.has_z(),
            Geometry::LineString(ls) => ls.has_z(),
            Geometry::Polygon(p) => p.has_z(),
            Geometry::MultiPoint(mp) => mp.has_z(),
            Geometry::MultiLineString(mls) => mls.has_z(),
            Geometry::MultiPolygon(mp) => mp.has_z(),
            Geometry::GeometryCollection(gc) => gc.has_z(),
        }
    }

    /// Returns true if this geometry has an M coordinate.
    pub fn has_m(&self) -> bool {
        match self {
            Geometry::Point(p) => p.has_m(),
            Geometry::LineString(ls) => ls.has_m(),
            Geometry::Polygon(p) => p.has_m(),
            Geometry::MultiPoint(mp) => mp.has_m(),
            Geometry::MultiLineString(mls) => mls.has_m(),
            Geometry::MultiPolygon(mp) => mp.has_m(),
            Geometry::GeometryCollection(gc) => gc.has_m(),
        }
    }

    /// Returns the dimensionality of this geometry.
    pub fn dimension(&self) -> Dimension {
        match self {
            Geometry::Point(p) => p.dimension(),
            Geometry::LineString(ls) => ls.dimension(),
            Geometry::Polygon(p) => p.dimension(),
            Geometry::MultiPoint(mp) => mp.dimension(),
            Geometry::MultiLineString(mls) => mls.dimension(),
            Geometry::MultiPolygon(mp) => mp.dimension(),
            Geometry::GeometryCollection(gc) => gc.dimension(),
        }
    }

    /// Returns true if this geometry is empty.
    pub fn is_empty(&self) -> bool {
        match self {
            Geometry::Point(p) => p.is_empty(),
            Geometry::LineString(ls) => ls.is_empty(),
            Geometry::Polygon(p) => p.is_empty(),
            Geometry::MultiPoint(mp) => mp.is_empty(),
            Geometry::MultiLineString(mls) => mls.is_empty(),
            Geometry::MultiPolygon(mp) => mp.is_empty(),
            Geometry::GeometryCollection(gc) => gc.is_empty(),
        }
    }

    /// Returns the geometry type name as a string.
    pub fn geometry_type(&self) -> &'static str {
        match self {
            Geometry::Point(_) => "Point",
            Geometry::LineString(_) => "LineString",
            Geometry::Polygon(_) => "Polygon",
            Geometry::MultiPoint(_) => "MultiPoint",
            Geometry::MultiLineString(_) => "MultiLineString",
            Geometry::MultiPolygon(_) => "MultiPolygon",
            Geometry::GeometryCollection(_) => "GeometryCollection",
        }
    }

    /// Returns a reference to the inner point, if this is a Point.
    pub fn as_point(&self) -> Option<&Point> {
        match self {
            Geometry::Point(p) => Some(p),
            _ => None,
        }
    }

    /// Returns a reference to the inner line string, if this is a LineString.
    pub fn as_linestring(&self) -> Option<&LineString> {
        match self {
            Geometry::LineString(ls) => Some(ls),
            _ => None,
        }
    }

    /// Returns a reference to the inner polygon, if this is a Polygon.
    pub fn as_polygon(&self) -> Option<&Polygon> {
        match self {
            Geometry::Polygon(p) => Some(p),
            _ => None,
        }
    }

    /// Returns a reference to the inner multi-point, if this is a MultiPoint.
    pub fn as_multi_point(&self) -> Option<&MultiPoint> {
        match self {
            Geometry::MultiPoint(mp) => Some(mp),
            _ => None,
        }
    }

    /// Returns a reference to the inner multi-line-string, if this is a MultiLineString.
    pub fn as_multi_linestring(&self) -> Option<&MultiLineString> {
        match self {
            Geometry::MultiLineString(mls) => Some(mls),
            _ => None,
        }
    }

    /// Returns a reference to the inner multi-polygon, if this is a MultiPolygon.
    pub fn as_multi_polygon(&self) -> Option<&MultiPolygon> {
        match self {
            Geometry::MultiPolygon(mp) => Some(mp),
            _ => None,
        }
    }

    /// Returns a reference to the inner collection, if this is a GeometryCollection.
    pub fn as_collection(&self) -> Option<&GeometryCollection> {
        match self {
            Geometry::GeometryCollection(gc) => Some(gc),
            _ => None,
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
    fn geometry_collection_new() {
        let gc = GeometryCollection::new(vec![
            Geometry::point(Point::new(0.0, 0.0)),
            Geometry::linestring(LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)])),
        ]);
        assert_eq!(gc.len(), 2);
        assert!(!gc.is_empty());
    }

    #[test]
    fn geometry_collection_empty() {
        let gc = GeometryCollection::new(vec![]);
        assert!(gc.is_empty());
        assert_eq!(gc.len(), 0);
    }

    #[test]
    fn geometry_collection_z_m_detection() {
        let gc_2d = GeometryCollection::new(vec![Geometry::point(Point::new(0.0, 0.0))]);
        assert!(!gc_2d.has_z());
        assert!(!gc_2d.has_m());
        assert_eq!(gc_2d.dimension(), Dimension::TwoD);

        let gc_z = GeometryCollection::new(vec![Geometry::point(Point::new_z(0.0, 0.0, 5.0))]);
        assert!(gc_z.has_z());
        assert!(!gc_z.has_m());
        assert_eq!(gc_z.dimension(), Dimension::ThreeD);

        let gc_m = GeometryCollection::new(vec![Geometry::point(Point::new_m(0.0, 0.0, 1.0))]);
        assert!(!gc_m.has_z());
        assert!(gc_m.has_m());
        assert_eq!(gc_m.dimension(), Dimension::Measure);

        let gc_zm = GeometryCollection::new(vec![Geometry::point(Point::new_zm(0.0, 0.0, 5.0, 1.0))]);
        assert!(gc_zm.has_z());
        assert!(gc_zm.has_m());
        assert_eq!(gc_zm.dimension(), Dimension::ThreeDMeasure);
    }

    #[test]
    fn geometry_enum_constructors() {
        let g_point = Geometry::point(Point::new(1.0, 2.0));
        assert_eq!(g_point.geometry_type(), "Point");
        assert!(g_point.as_point().is_some());

        let g_ls = Geometry::linestring(LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)]));
        assert_eq!(g_ls.geometry_type(), "LineString");
        assert!(g_ls.as_linestring().is_some());

        let g_poly = Geometry::polygon(Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap());
        assert_eq!(g_poly.geometry_type(), "Polygon");
        assert!(g_poly.as_polygon().is_some());

        let g_mp = Geometry::multi_point(MultiPoint::new(vec![Point::new(0.0, 0.0)]));
        assert_eq!(g_mp.geometry_type(), "MultiPoint");
        assert!(g_mp.as_multi_point().is_some());

        let g_mls = Geometry::multi_linestring(MultiLineString::new(vec![LineString::from_2d_coords(&[(
            0.0, 0.0,
        ), (1.0, 1.0)])]));
        assert_eq!(g_mls.geometry_type(), "MultiLineString");
        assert!(g_mls.as_multi_linestring().is_some());

        let g_mpoly = Geometry::multi_polygon(MultiPolygon::new(vec![
            Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap(),
        ]));
        assert_eq!(g_mpoly.geometry_type(), "MultiPolygon");
        assert!(g_mpoly.as_multi_polygon().is_some());

        let g_gc = Geometry::collection(GeometryCollection::new(vec![Geometry::point(Point::new(
            0.0, 0.0,
        ))]));
        assert_eq!(g_gc.geometry_type(), "GeometryCollection");
        assert!(g_gc.as_collection().is_some());
    }

    #[test]
    fn geometry_enum_z_m_propagation() {
        let g_z = Geometry::point(Point::new_z(0.0, 0.0, 5.0));
        assert!(g_z.has_z());
        assert!(!g_z.has_m());
        assert_eq!(g_z.dimension(), Dimension::ThreeD);

        let g_m = Geometry::linestring(LineString::new(vec![
            Point::new_m(0.0, 0.0, 1.0),
            Point::new_m(1.0, 1.0, 2.0),
        ]));
        assert!(!g_m.has_z());
        assert!(g_m.has_m());
        assert_eq!(g_m.dimension(), Dimension::Measure);

        let g_zm = Geometry::polygon(Polygon::new(vec![
            Point::new_zm(0.0, 0.0, 5.0, 1.0),
            Point::new_zm(1.0, 0.0, 5.0, 1.0),
            Point::new_zm(1.0, 1.0, 5.0, 1.0),
            Point::new_zm(0.0, 1.0, 5.0, 1.0),
            Point::new_zm(0.0, 0.0, 5.0, 1.0),
        ]));
        assert!(g_zm.has_z());
        assert!(g_zm.has_m());
        assert_eq!(g_zm.dimension(), Dimension::ThreeDMeasure);
    }

    #[test]
    fn geometry_enum_is_empty() {
        assert!(!Geometry::point(Point::new(0.0, 0.0)).is_empty());
        assert!(Geometry::point(Point::new(f64::NAN, 0.0)).is_empty());
        assert!(Geometry::linestring(LineString::new(vec![])).is_empty());
        assert!(Geometry::polygon(Polygon::new(vec![])).is_empty());
        assert!(Geometry::multi_point(MultiPoint::new(vec![])).is_empty());
        assert!(Geometry::collection(GeometryCollection::new(vec![])).is_empty());
    }

    #[test]
    fn geometry_enum_as_methods_return_none_for_wrong_type() {
        let g = Geometry::point(Point::new(0.0, 0.0));
        assert!(g.as_linestring().is_none());
        assert!(g.as_polygon().is_none());
        assert!(g.as_multi_point().is_none());
        assert!(g.as_multi_linestring().is_none());
        assert!(g.as_multi_polygon().is_none());
        assert!(g.as_collection().is_none());
    }

    #[test]
    fn geometry_collection_nested() {
        let inner = GeometryCollection::new(vec![Geometry::point(Point::new(0.0, 0.0))]);
        let outer = GeometryCollection::new(vec![Geometry::collection(inner)]);
        assert_eq!(outer.len(), 1);
        assert_eq!(outer.geometries()[0].geometry_type(), "GeometryCollection");
    }
}
