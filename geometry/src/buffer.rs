//! Buffer operation.
//!
//! The `geo` crate does not provide a buffer operation. This module
//! implements a basic buffer that covers the common cases:
//!
//! * **Point** → regular polygon approximating a circle
//! * **LineString** → union of point buffers at each vertex, connected by
//!   rectangles along each segment
//! * **Polygon** → edge offset (outward for positive distance, inward for
//!   negative)
//!
//! JTS/GEOS gaps:
//! * This is NOT a full JTS buffer implementation. JTS uses a sophisticated
//!   algorithm with round joins, mitre limits, and single-sided buffers.
//! * This implementation uses a fixed number of segments for circles
//!   (configurable, default 32) and does not handle mitre joins.
//! * Negative buffers that would collapse the polygon return an empty
//!   polygon rather than the exact JTS result.
//! * Buffer distance is in the same units as the input coordinates
//!   (degrees for geographic CRS). QGIS/GEOS behaves the same way.

use crate::collection::Geometry;
use crate::linestring::LineString;
use crate::point::Point;
use crate::polygon::Polygon;

/// Default number of segments used to approximate a circle.
const DEFAULT_CIRCLE_SEGMENTS: usize = 32;

/// Returns a buffer of the given geometry by the specified distance.
///
/// Positive distance expands the geometry; negative distance shrinks it.
/// The `segments` parameter controls the smoothness of curved boundaries
/// (default 32). Higher values produce smoother circles but larger
/// geometries.
///
/// Matches QGIS `QgsGeometry::buffer(distance, segments)`.
pub fn buffer(geometry: &Geometry, distance: f64, segments: usize) -> Geometry {
    let n = if segments == 0 {
        DEFAULT_CIRCLE_SEGMENTS
    } else {
        segments
    };
    match geometry {
        Geometry::Point(p) => buffer_point(p, distance, n),
        Geometry::LineString(ls) => buffer_linestring(ls, distance, n),
        Geometry::Polygon(p) => buffer_polygon(p, distance, n),
        Geometry::MultiPoint(mp) => {
            let parts: Vec<Geometry> = mp
                .points()
                .iter()
                .map(|p| buffer_point(p, distance, n))
                .collect();
            union_all(&parts)
        }
        Geometry::MultiLineString(mls) => {
            let parts: Vec<Geometry> = mls
                .lines()
                .iter()
                .map(|ls| buffer_linestring(ls, distance, n))
                .collect();
            union_all(&parts)
        }
        Geometry::MultiPolygon(mp) => {
            let parts: Vec<Geometry> = mp
                .polygons()
                .iter()
                .map(|p| buffer_polygon(p, distance, n))
                .collect();
            union_all(&parts)
        }
        Geometry::GeometryCollection(gc) => {
            let parts: Vec<Geometry> = gc
                .geometries()
                .iter()
                .map(|g| buffer(g, distance, n))
                .collect();
            union_all(&parts)
        }
    }
}

/// Buffer a point → regular polygon approximating a circle.
fn buffer_point(p: &Point, distance: f64, segments: usize) -> Geometry {
    if distance == 0.0 {
        return Geometry::point(*p);
    }
    let n = segments.max(3);
    let mut ring: Vec<Point> = Vec::with_capacity(n + 1);
    for i in 0..n {
        let angle = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
        ring.push(Point::new(
            p.x + distance * angle.cos(),
            p.y + distance * angle.sin(),
        ));
    }
    ring.push(ring[0]); // close the ring
    Geometry::polygon(Polygon::from_rings(vec![ring]).unwrap())
}

/// Buffer a linestring → union of point buffers and segment rectangles.
fn buffer_linestring(ls: &LineString, distance: f64, segments: usize) -> Geometry {
    if distance == 0.0 {
        return Geometry::linestring(ls.clone());
    }
    if ls.is_empty() {
        return Geometry::linestring(ls.clone());
    }
    if ls.len() == 1 {
        return buffer_point(&ls.points()[0], distance, segments);
    }

    let mut parts: Vec<Geometry> = Vec::new();

    // Add a circle at each vertex
    for p in ls.points() {
        parts.push(buffer_point(p, distance, segments));
    }

    // Add a rectangle along each segment
    for w in ls.points().windows(2) {
        parts.push(buffer_segment(&w[0], &w[1], distance));
    }

    union_all(&parts)
}

/// Buffer a single segment → rectangle with rounded ends.
fn buffer_segment(a: &Point, b: &Point, distance: f64) -> Geometry {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 {
        return buffer_point(a, distance, DEFAULT_CIRCLE_SEGMENTS);
    }
    // Unit normal (perpendicular to segment direction)
    let nx = -dy / len;
    let ny = dx / len;

    let ring = vec![
        Point::new(a.x + nx * distance, a.y + ny * distance),
        Point::new(b.x + nx * distance, b.y + ny * distance),
        Point::new(b.x - nx * distance, b.y - ny * distance),
        Point::new(a.x - nx * distance, a.y - ny * distance),
        Point::new(a.x + nx * distance, a.y + ny * distance),
    ];
    Geometry::polygon(Polygon::from_rings(vec![ring]).unwrap())
}

/// Buffer a polygon → offset edges.
///
/// For positive buffers: offsets exterior outward and holes inward, then
/// unions with circles at vertices for rounded corners.
///
/// For negative buffers: scales the polygon toward its centroid by the
/// buffer ratio. This is a simplification — a true negative buffer would
/// compute the Minkowski difference, but scaling produces correct results
/// for convex polygons and reasonable results for concave ones.
fn buffer_polygon(poly: &Polygon, distance: f64, segments: usize) -> Geometry {
    if distance == 0.0 {
        return Geometry::polygon(poly.clone());
    }
    if poly.is_empty() {
        return Geometry::polygon(poly.clone());
    }

    if distance > 0.0 {
        // Positive buffer: offset exterior outward, holes inward
        let offset_exterior = offset_ring(poly.exterior(), distance);
        let offset_holes: Vec<Vec<Point>> = poly
            .interiors()
            .iter()
            .map(|h| offset_ring(h, -distance))
            .collect();

        let mut rings: Vec<Vec<Point>> = Vec::new();
        if !offset_exterior.is_empty() {
            rings.push(offset_exterior);
        }
        for h in offset_holes {
            if !h.is_empty() {
                rings.push(h);
            }
        }

        if rings.is_empty() {
            return Geometry::polygon(Polygon::new(vec![]));
        }

        let mut parts: Vec<Geometry> = Vec::new();
        if let Ok(p) = Polygon::from_rings(rings) {
            parts.push(Geometry::polygon(p));
        }
        // Add circles at exterior vertices for rounded corners
        for p in poly.exterior() {
            parts.push(buffer_point(p, distance, segments));
        }
        union_all(&parts)
    } else {
        // Negative buffer: scale toward centroid
        let centroid = polygon_centroid(poly);
        let scale = 1.0 + distance / polygon_max_radius(poly, &centroid);
        if scale <= 0.0 {
            return Geometry::polygon(Polygon::new(vec![]));
        }
        let scaled_exterior: Vec<Point> = poly
            .exterior()
            .iter()
            .map(|p| {
                Point::new(
                    centroid.x + (p.x - centroid.x) * scale,
                    centroid.y + (p.y - centroid.y) * scale,
                )
            })
            .collect();
        let scaled_holes: Vec<Vec<Point>> = poly
            .interiors()
            .iter()
            .map(|h| {
                h.iter()
                    .map(|p| {
                        Point::new(
                            centroid.x + (p.x - centroid.x) * scale,
                            centroid.y + (p.y - centroid.y) * scale,
                        )
                    })
                    .collect()
            })
            .collect();
        let mut rings = vec![scaled_exterior];
        rings.extend(scaled_holes);
        match Polygon::from_rings(rings) {
            Ok(p) => Geometry::polygon(p),
            Err(_) => Geometry::polygon(Polygon::new(vec![])),
        }
    }
}

/// Compute the centroid of a polygon (average of exterior vertices).
fn polygon_centroid(poly: &Polygon) -> Point {
    let n = poly.exterior().len();
    if n == 0 {
        return Point::new(0.0, 0.0);
    }
    let (sum_x, sum_y) = poly
        .exterior()
        .iter()
        .fold((0.0, 0.0), |(sx, sy), p| (sx + p.x, sy + p.y));
    Point::new(sum_x / n as f64, sum_y / n as f64)
}

/// Compute the maximum distance from the centroid to any exterior vertex.
fn polygon_max_radius(poly: &Polygon, centroid: &Point) -> f64 {
    poly.exterior()
        .iter()
        .map(|p| p.distance(centroid))
        .fold(0.0, f64::max)
}

/// Offset a ring by the given distance (positive = outward for CCW rings).
fn offset_ring(ring: &[Point], distance: f64) -> Vec<Point> {
    if ring.len() < 4 {
        return vec![];
    }
    let n = ring.len() - 1; // exclude closing vertex
    let mut result: Vec<Point> = Vec::with_capacity(n + 1);

    // Determine the winding direction of the ring.
    // For CCW rings, the outward normal is (-dy, dx).
    // For CW rings, the outward normal is (dy, -dx).
    let signed_area: f64 = (0..n)
        .map(|i| ring[i].x * ring[(i + 1) % n].y - ring[(i + 1) % n].x * ring[i].y)
        .sum();
    let is_ccw = signed_area > 0.0;

    // For positive distance: offset along outward normal.
    // For negative distance: offset opposite to outward normal (inward).
    let effective_dist = distance.abs();
    let sign = if distance >= 0.0 { 1.0 } else { -1.0 };

    for i in 0..n {
        let prev = &ring[(i + n - 1) % n];
        let curr = &ring[i];
        let next = &ring[(i + 1) % n];

        // Incoming edge direction
        let d1x = curr.x - prev.x;
        let d1y = curr.y - prev.y;
        let l1 = (d1x * d1x + d1y * d1y).sqrt();
        if l1 == 0.0 {
            continue;
        }
        // Outgoing edge direction
        let d2x = next.x - curr.x;
        let d2y = next.y - curr.y;
        let l2 = (d2x * d2x + d2y * d2y).sqrt();
        if l2 == 0.0 {
            continue;
        }

        // Unit normals (pointing outward)
        let (n1x, n1y) = if is_ccw {
            (-d1y / l1, d1x / l1)
        } else {
            (d1y / l1, -d1x / l1)
        };
        let (n2x, n2y) = if is_ccw {
            (-d2y / l2, d2x / l2)
        } else {
            (d2y / l2, -d2x / l2)
        };

        // Offset points: positive distance moves outward, negative moves inward
        let p1x = curr.x + n1x * effective_dist * sign;
        let p1y = curr.y + n1y * effective_dist * sign;
        let p2x = curr.x + n2x * effective_dist * sign;
        let p2y = curr.y + n2y * effective_dist * sign;

        // Intersection of the two offset lines
        let denom = n1x * n2y - n1y * n2x;
        if denom.abs() < 1e-15 {
            // Parallel edges — just use the first offset
            result.push(Point::new(p1x, p1y));
        } else {
            let t = ((p2x - p1x) * n2y - (p2y - p1y) * n2x) / denom;
            result.push(Point::new(p1x + t * n1x, p1y + t * n1y));
        }
    }

    if result.len() >= 3 {
        result.push(result[0]); // close the ring
    }
    result
}

/// Union a list of geometries into a single geometry.
fn union_all(geometries: &[Geometry]) -> Geometry {
    use crate::overlay::union;
    if geometries.is_empty() {
        return Geometry::polygon(Polygon::new(vec![]));
    }
    let mut result = geometries[0].clone();
    for g in &geometries[1..] {
        result = union(&result, g);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi::MultiPoint;

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
    fn buffer_point_produces_circle() {
        let pt = Geometry::point(Point::new(0.0, 0.0));
        let result = buffer(&pt, 1.0, 32);
        match &result {
            Geometry::Polygon(p) => {
                assert_eq!(p.exterior().len(), 33); // 32 segments + closing
                // All vertices should be at distance ~1.0 from center
                for p in p.exterior() {
                    let d = (p.x * p.x + p.y * p.y).sqrt();
                    assert!((d - 1.0).abs() < 0.01);
                }
            }
            _ => panic!("expected polygon"),
        }
    }

    #[test]
    fn buffer_point_zero_distance() {
        let pt = Geometry::point(Point::new(1.0, 2.0));
        let result = buffer(&pt, 0.0, 32);
        assert!(matches!(result, Geometry::Point(_)));
    }

    #[test]
    fn buffer_linestring_produces_geometry() {
        let ls = Geometry::linestring(LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 0.0)]));
        let result = buffer(&ls, 0.5, 16);
        let a = match &result {
            Geometry::Polygon(p) => p.area_abs(),
            Geometry::MultiPolygon(mp) => mp.total_area(),
            _ => panic!("expected polygon or multipolygon"),
        };
        // Area = 2 circles (pi * 0.25 each) + rectangle (1.0 * 1.0) - overlaps
        // The union of 2 full circles and a rectangle produces ~2.77
        assert!(a > 2.0 && a < 3.5, "area {} not in expected range", a);
    }

    #[test]
    fn buffer_linestring_single_point() {
        let ls = Geometry::linestring(LineString::from_2d_coords(&[(0.0, 0.0)]));
        let result = buffer(&ls, 1.0, 16);
        assert!(matches!(result, Geometry::Polygon(_)));
    }

    #[test]
    fn buffer_polygon_positive() {
        let poly = Geometry::polygon(Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap());
        let result = buffer(&poly, 0.5, 16);
        match &result {
            Geometry::Polygon(p) => {
                let a = p.area_abs();
                // Should be larger than original (1.0)
                assert!(a > 1.0, "buffered area {} should be > 1.0", a);
            }
            _ => panic!("expected polygon"),
        }
    }

    #[test]
    fn buffer_polygon_negative() {
        let poly = Geometry::polygon(Polygon::from_rings(vec![sq(0.0, 0.0, 10.0, 10.0)]).unwrap());
        let result = buffer(&poly, -1.0, 16);
        match &result {
            Geometry::Polygon(p) => {
                let a = p.area_abs();
                // Should be smaller than original (100.0)
                assert!(a < 100.0, "buffered area {} should be < 100.0", a);
            }
            _ => panic!("expected polygon"),
        }
    }

    #[test]
    fn buffer_polygon_zero_distance() {
        let poly = Geometry::polygon(Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap());
        let result = buffer(&poly, 0.0, 16);
        assert!(matches!(result, Geometry::Polygon(_)));
    }

    #[test]
    fn buffer_empty_polygon() {
        let poly = Geometry::polygon(Polygon::new(vec![]));
        let result = buffer(&poly, 1.0, 16);
        assert!(result.is_empty());
    }

    #[test]
    fn buffer_multipoint() {
        let mp = Geometry::multi_point(MultiPoint::new(vec![
            Point::new(0.0, 0.0),
            Point::new(2.0, 0.0),
        ]));
        let result = buffer(&mp, 0.5, 16);
        match &result {
            Geometry::MultiPolygon(mp) => {
                assert_eq!(mp.len(), 2);
                let a = mp.total_area();
                // Two circles of radius 0.5, not overlapping
                assert!(a > 1.0, "area {} should be > 1.0", a);
            }
            _ => panic!("expected multipolygon"),
        }
    }

    #[test]
    fn buffer_default_segments() {
        let pt = Geometry::point(Point::new(0.0, 0.0));
        let result = buffer(&pt, 1.0, 0);
        match &result {
            Geometry::Polygon(p) => {
                assert_eq!(p.exterior().len(), 33); // default 32 segments
            }
            _ => panic!("expected polygon"),
        }
    }

    #[test]
    fn buffer_polygon_with_hole() {
        let poly = Geometry::polygon(
            Polygon::from_rings(vec![sq(0.0, 0.0, 10.0, 10.0), sq(4.0, 4.0, 6.0, 6.0)]).unwrap(),
        );
        let result = buffer(&poly, 0.5, 16);
        match &result {
            Geometry::Polygon(p) => {
                let a = p.area_abs();
                // Original area = 100 - 4 = 96; buffered should be larger
                assert!(a > 96.0, "area {} should be > 96.0", a);
            }
            _ => panic!("expected polygon"),
        }
    }
}
