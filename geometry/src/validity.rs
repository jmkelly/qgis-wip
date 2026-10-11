//! Validity checking and make-valid operations.
//!
//! Replaces GEOS `GEOSisValid` and `GEOSMakeValid` with pure-Rust
//! implementations. See ADR-0001 for the no-FFI policy decision.
//!
//! Scope (bead qgis-s4i.4, E1 Core geometry rebuild):
//! * [`ValidityError`] — reasons a geometry may be invalid
//! * [`is_valid`] for every geometry type (point, linestring, polygon, multi*)
//! * [`make_valid`] repairs: ring orientation, duplicate consecutive points,
//!   self-intersection splitting for linestrings, polygon collapse
//!
//! The algorithms follow the OGC Simple Features specification and the
//! behaviour of GEOS `GEOSisValid` / `GEOSMakeValid` as used by QGIS.

use crate::collection::{Geometry, GeometryCollection};
use crate::linestring::LineString;
use crate::multi::{MultiLineString, MultiPoint, MultiPolygon};
use crate::point::Point;
use crate::polygon::Polygon;

/// Tolerance for collinearity and boundary detection.
const EPSILON: f64 = 1e-12;

/// Reasons a geometry may be invalid, matching GEOS validity codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidityError {
    /// The geometry has no points or is otherwise empty.
    EmptyGeometry,
    /// A ring has fewer than 4 points (cannot form a closed triangle).
    TooFewPoints,
    /// A ring or linestring repeats a vertex consecutively.
    DuplicateConsecutivePoints,
    /// A linestring or ring self-intersects.
    SelfIntersection,
    /// A polygon ring self-intersects.
    RingSelfIntersection,
    /// A hole lies outside the exterior ring.
    HoleOutsideExterior,
    /// Two holes overlap each other.
    HolesOverlap,
    /// A hole touches the exterior ring (only point-touch is allowed by OGC
    /// but GEOS rejects even that for make_valid).
    HoleTouchesExterior,
    /// Components of a multi-geometry overlap.
    OverlappingComponents,
    /// A ring is degenerate (zero area).
    DegenerateRing,
}

impl std::fmt::Display for ValidityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyGeometry => write!(f, "geometry is empty"),
            Self::TooFewPoints => write!(f, "ring has fewer than 4 points"),
            Self::DuplicateConsecutivePoints => write!(f, "duplicate consecutive points"),
            Self::SelfIntersection => write!(f, "geometry self-intersects"),
            Self::RingSelfIntersection => write!(f, "ring self-intersects"),
            Self::HoleOutsideExterior => write!(f, "hole lies outside exterior ring"),
            Self::HolesOverlap => write!(f, "holes overlap each other"),
            Self::HoleTouchesExterior => write!(f, "hole touches exterior ring"),
            Self::OverlappingComponents => write!(f, "components overlap"),
            Self::DegenerateRing => write!(f, "ring is degenerate"),
        }
    }
}

impl std::error::Error for ValidityError {}

// ---------------------------------------------------------------------------
// Core geometric primitives
// ---------------------------------------------------------------------------

/// Returns the cross product of vectors (b − a) and (c − a).
///
/// Positive when c is to the left of a→b, negative when to the right,
/// zero when collinear.
fn cross(a: &Point, b: &Point, c: &Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Returns true if point `c` lies on segment a→b (assuming collinear).
fn on_segment(a: &Point, b: &Point, c: &Point) -> bool {
    let (min_x, max_x) = if a.x < b.x { (a.x, b.x) } else { (b.x, a.x) };
    let (min_y, max_y) = if a.y < b.y { (a.y, b.y) } else { (b.y, a.y) };
    c.x >= min_x - EPSILON
        && c.x <= max_x + EPSILON
        && c.y >= min_y - EPSILON
        && c.y <= max_y + EPSILON
}

/// Returns true if segments a1→a2 and b1→b2 intersect (including at
/// endpoints and collinear overlap).
pub(crate) fn segments_intersect(a1: &Point, a2: &Point, b1: &Point, b2: &Point) -> bool {
    let d1 = cross(b1, b2, a1);
    let d2 = cross(b1, b2, a2);
    let d3 = cross(a1, a2, b1);
    let d4 = cross(a1, a2, b2);

    // Proper intersection — segments cross each other.
    if ((d1 > EPSILON && d2 < -EPSILON) || (d1 < -EPSILON && d2 > EPSILON))
        && ((d3 > EPSILON && d4 < -EPSILON) || (d3 < -EPSILON && d4 > EPSILON))
    {
        return true;
    }

    // Collinear cases.
    if d1.abs() <= EPSILON && on_segment(b1, b2, a1) {
        return true;
    }
    if d2.abs() <= EPSILON && on_segment(b1, b2, a2) {
        return true;
    }
    if d3.abs() <= EPSILON && on_segment(a1, a2, b1) {
        return true;
    }
    if d4.abs() <= EPSILON && on_segment(a1, a2, b2) {
        return true;
    }

    false
}

/// Returns the intersection point of segments a1→a2 and b1→b2, or `None`
/// if they do not intersect.
fn segment_intersection_point(a1: &Point, a2: &Point, b1: &Point, b2: &Point) -> Option<Point> {
    let dx1 = a2.x - a1.x;
    let dy1 = a2.y - a1.y;
    let dx2 = b2.x - b1.x;
    let dy2 = b2.y - b1.y;

    let denom = dx1 * dy2 - dy1 * dx2;
    if denom.abs() < EPSILON {
        return None; // Parallel or collinear.
    }

    let t = ((b1.x - a1.x) * dy2 - (b1.y - a1.y) * dx2) / denom;
    let u = ((b1.x - a1.x) * dy1 - (b1.y - a1.y) * dx1) / denom;

    if t < -EPSILON
        || t > 1.0 + EPSILON
        || u < -EPSILON
        || u > 1.0 + EPSILON
    {
        return None;
    }

    let t = t.clamp(0.0, 1.0);
    Some(Point::new(a1.x + t * dx1, a1.y + t * dy1))
}

/// Signed area of a ring via the shoelace formula (positive for CCW).
fn signed_ring_area(ring: &[Point]) -> f64 {
    let mut sum = 0.0;
    for pair in ring.windows(2) {
        sum += pair[0].x * pair[1].y - pair[1].x * pair[0].y;
    }
    sum * 0.5
}

/// Orientation of a ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub(crate) enum RingOrientation {
    /// Counter-clockwise (positive signed area).
    CCW,
    /// Clockwise (negative signed area).
    CW,
    /// Degenerate (zero area).
    Degenerate,
}

fn ring_orientation(ring: &[Point]) -> RingOrientation {
    let area = signed_ring_area(ring);
    if area > EPSILON {
        RingOrientation::CCW
    } else if area < -EPSILON {
        RingOrientation::CW
    } else {
        RingOrientation::Degenerate
    }
}

/// Removes consecutive duplicate points from a ring or linestring.
/// The closing point of a ring is preserved if it matches the first point.
fn remove_consecutive_duplicates(points: &[Point]) -> Vec<Point> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut result = Vec::with_capacity(points.len());
    result.push(points[0]);
    for p in &points[1..] {
        if *p != result[result.len() - 1] {
            result.push(*p);
        }
    }
    // For rings, ensure closure is preserved.
    if result.len() >= 2 && result[0] == result[result.len() - 1] {
        return result;
    }
    // If original was closed and dedup broke closure, re-close.
    if result.len() >= 3 && points[0] == points[points.len() - 1] {
        result.push(result[0]);
    }
    result
}

/// Returns true if a closed ring self-intersects (ignoring the shared
/// endpoint of the first and last segments).
fn ring_self_intersects(ring: &[Point]) -> bool {
    let n = ring.len();
    if n < 5 {
        // A closed triangle (4 points with closure) cannot self-intersect.
        return false;
    }
    // For a ring, segments are (0,1), (1,2), ..., (n-2, n-1).
    // The last segment shares its start with the first segment's start,
    // so we skip the pair (last, first).
    let segment_count = n - 1;
    for i in 0..segment_count {
        for j in (i + 2)..segment_count {
            if i == 0 && j == segment_count - 1 {
                continue; // Adjacent via closure.
            }
            if segments_intersect(&ring[i], &ring[i + 1], &ring[j], &ring[j + 1]) {
                return true;
            }
        }
    }
    false
}

/// Returns true if a linestring self-intersects.
fn linestring_self_intersects(points: &[Point]) -> bool {
    let n = points.len();
    if n < 4 {
        return false;
    }
    for i in 0..n - 1 {
        for j in (i + 2)..n - 1 {
            if segments_intersect(&points[i], &points[i + 1], &points[j], &points[j + 1]) {
                return true;
            }
        }
    }
    false
}

/// Returns true if point `p` lies on the boundary of `ring`.
fn point_on_ring_boundary(p: &Point, ring: &[Point]) -> bool {
    for pair in ring.windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        let cr = cross(a, b, p);
        if cr.abs() > EPSILON {
            continue;
        }
        if on_segment(a, b, p) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Validity checks
// ---------------------------------------------------------------------------

/// Returns true if a point is valid.
///
/// A point is valid unless its coordinates are NaN.
pub(crate) fn is_valid_point(p: &Point) -> bool {
    !p.is_empty()
}

/// Returns `Some(ValidityError)` if the linestring is invalid, `None` if valid.
pub(crate) fn linestring_validity_error(ls: &LineString) -> Option<ValidityError> {
    let points = ls.points();
    if points.is_empty() {
        return Some(ValidityError::EmptyGeometry);
    }
    if points.len() < 2 {
        return Some(ValidityError::TooFewPoints);
    }
    // Check for duplicate consecutive points.
    for pair in points.windows(2) {
        if pair[0] == pair[1] {
            return Some(ValidityError::DuplicateConsecutivePoints);
        }
    }
    // Check for self-intersections.
    if linestring_self_intersects(points) {
        return Some(ValidityError::SelfIntersection);
    }
    None
}

/// Returns `Some(ValidityError)` if the polygon is invalid, `None` if valid.
pub(crate) fn polygon_validity_error(poly: &Polygon) -> Option<ValidityError> {
    let exterior = poly.exterior();
    if exterior.is_empty() {
        return Some(ValidityError::EmptyGeometry);
    }
    if exterior.len() < 4 {
        return Some(ValidityError::TooFewPoints);
    }
    // Check exterior ring for duplicate consecutive points.
    for pair in exterior.windows(2) {
        if pair[0] == pair[1] {
            return Some(ValidityError::DuplicateConsecutivePoints);
        }
    }
    // Check exterior ring for self-intersections.
    if ring_self_intersects(exterior) {
        return Some(ValidityError::RingSelfIntersection);
    }
    // Check interior rings.
    for hole in poly.interiors() {
        if hole.len() < 4 {
            return Some(ValidityError::TooFewPoints);
        }
        for pair in hole.windows(2) {
            if pair[0] == pair[1] {
                return Some(ValidityError::DuplicateConsecutivePoints);
            }
        }
        if ring_self_intersects(hole) {
            return Some(ValidityError::RingSelfIntersection);
        }
    }
    // Check holes are inside exterior and don't overlap each other.
    let hole_count = poly.interiors().len();
    for i in 0..hole_count {
        let hole = &poly.interiors()[i];
        // Hole must be inside exterior.
        if !ring_inside_ring(hole, exterior) {
            return Some(ValidityError::HoleOutsideExterior);
        }
        // Hole must not touch exterior boundary.
        if ring_touches_ring(hole, exterior) {
            return Some(ValidityError::HoleTouchesExterior);
        }
        // Hole must not overlap other holes.
        for j in (i + 1)..hole_count {
            let other = &poly.interiors()[j];
            if rings_overlap(hole, other) {
                return Some(ValidityError::HolesOverlap);
            }
        }
    }
    None
}

/// Returns true if ring `inner` is entirely inside ring `outer` (not touching
/// the boundary).
fn ring_inside_ring(inner: &[Point], outer: &[Point]) -> bool {
    // All vertices of inner must be strictly inside outer.
    for p in inner {
        if !point_strictly_inside_ring(p, outer) {
            return false;
        }
    }
    // No edge of inner may cross any edge of outer.
    for inner_pair in inner.windows(2) {
        for outer_pair in outer.windows(2) {
            if segments_intersect(&inner_pair[0], &inner_pair[1], &outer_pair[0], &outer_pair[1]) {
                return false;
            }
        }
    }
    true
}

/// Returns true if ring `a` touches ring `b` (shares a boundary point).
fn ring_touches_ring(a: &[Point], b: &[Point]) -> bool {
    for a_pair in a.windows(2) {
        for b_pair in b.windows(2) {
            if segments_intersect(&a_pair[0], &a_pair[1], &b_pair[0], &b_pair[1]) {
                return true;
            }
        }
    }
    false
}

/// Returns true if two rings overlap (share area).
fn rings_overlap(a: &[Point], b: &[Point]) -> bool {
    // Quick check: if any vertex of a is inside b or vice versa, they overlap.
    for p in a {
        if point_strictly_inside_ring(p, b) {
            return true;
        }
    }
    for p in b {
        if point_strictly_inside_ring(p, a) {
            return true;
        }
    }
    // Check edge crossings.
    for a_pair in a.windows(2) {
        for b_pair in b.windows(2) {
            if segments_intersect(&a_pair[0], &a_pair[1], &b_pair[0], &b_pair[1]) {
                return true;
            }
        }
    }
    false
}

/// Returns true if point `p` is strictly inside `ring` (not on boundary).
fn point_strictly_inside_ring(p: &Point, ring: &[Point]) -> bool {
    if point_on_ring_boundary(p, ring) {
        return false;
    }
    // Ray casting.
    let mut crossings = 0usize;
    for pair in ring.windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        if (a.y > p.y) == (b.y > p.y) {
            continue;
        }
        let x_intersect = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
        if x_intersect > p.x {
            crossings += 1;
        }
    }
    crossings % 2 == 1
}

/// Returns `Some(ValidityError)` if the multi-point is invalid.
pub(crate) fn multi_point_validity_error(mp: &MultiPoint) -> Option<ValidityError> {
    if mp.is_empty() {
        return Some(ValidityError::EmptyGeometry);
    }
    None
}

/// Returns `Some(ValidityError)` if the multi-linestring is invalid.
pub(crate) fn multi_linestring_validity_error(mls: &MultiLineString) -> Option<ValidityError> {
    if mls.is_empty() {
        return Some(ValidityError::EmptyGeometry);
    }
    for line in mls.lines() {
        if let Some(err) = linestring_validity_error(line) {
            return Some(err);
        }
    }
    // Check for intersections between different linestrings.
    let lines = mls.lines();
    for i in 0..lines.len() {
        for j in (i + 1)..lines.len() {
            if linestrings_intersect(&lines[i], &lines[j]) {
                return Some(ValidityError::OverlappingComponents);
            }
        }
    }
    None
}

/// Returns true if two linestrings intersect.
fn linestrings_intersect(a: &LineString, b: &LineString) -> bool {
    for a_pair in a.points().windows(2) {
        for b_pair in b.points().windows(2) {
            if segments_intersect(&a_pair[0], &a_pair[1], &b_pair[0], &b_pair[1]) {
                return true;
            }
        }
    }
    false
}

/// Returns `Some(ValidityError)` if the multi-polygon is invalid.
pub(crate) fn multi_polygon_validity_error(mp: &MultiPolygon) -> Option<ValidityError> {
    if mp.is_empty() {
        return Some(ValidityError::EmptyGeometry);
    }
    for poly in mp.polygons() {
        if let Some(err) = polygon_validity_error(poly) {
            return Some(err);
        }
    }
    // Check for overlaps between polygons.
    let polys = mp.polygons();
    for i in 0..polys.len() {
        for j in (i + 1)..polys.len() {
            if polygons_overlap(&polys[i], &polys[j]) {
                return Some(ValidityError::OverlappingComponents);
            }
        }
    }
    None
}

/// Returns true if two polygons overlap.
fn polygons_overlap(a: &Polygon, b: &Polygon) -> bool {
    // Check if any vertex of a is inside b or vice versa.
    for p in a.exterior() {
        if point_strictly_inside_ring(p, b.exterior()) {
            return true;
        }
    }
    for p in b.exterior() {
        if point_strictly_inside_ring(p, a.exterior()) {
            return true;
        }
    }
    // Check edge crossings between exterior rings.
    for a_pair in a.exterior().windows(2) {
        for b_pair in b.exterior().windows(2) {
            if segments_intersect(&a_pair[0], &a_pair[1], &b_pair[0], &b_pair[1]) {
                return true;
            }
        }
    }
    false
}

/// Returns `Some(ValidityError)` if the geometry collection is invalid.
pub(crate) fn geometry_collection_validity_error(
    gc: &GeometryCollection,
) -> Option<ValidityError> {
    if gc.is_empty() {
        return Some(ValidityError::EmptyGeometry);
    }
    for g in gc.geometries() {
        if let Some(err) = geometry_validity_error(g) {
            return Some(err);
        }
    }
    None
}

/// Returns `Some(ValidityError)` if the geometry is invalid, `None` if valid.
pub(crate) fn geometry_validity_error(g: &Geometry) -> Option<ValidityError> {
    match g {
        Geometry::Point(p) => {
            if is_valid_point(p) {
                None
            } else {
                Some(ValidityError::EmptyGeometry)
            }
        }
        Geometry::LineString(ls) => linestring_validity_error(ls),
        Geometry::Polygon(p) => polygon_validity_error(p),
        Geometry::MultiPoint(mp) => multi_point_validity_error(mp),
        Geometry::MultiLineString(mls) => multi_linestring_validity_error(mls),
        Geometry::MultiPolygon(mp) => multi_polygon_validity_error(mp),
        Geometry::GeometryCollection(gc) => geometry_collection_validity_error(gc),
    }
}

// ---------------------------------------------------------------------------
// Make-valid operations
// ---------------------------------------------------------------------------

/// Makes a linestring valid by removing duplicate consecutive points and
/// splitting at self-intersections.
fn make_valid_linestring(ls: &LineString) -> Geometry {
    let points = remove_consecutive_duplicates(ls.points());
    if points.len() < 2 {
        return Geometry::linestring(LineString::new(points));
    }
    if !linestring_self_intersects(&points) {
        return Geometry::linestring(LineString::new(points));
    }
    // Split at self-intersections.
    let split = split_linestring_at_intersections(&points);
    // Recursively validate each part.
    match split {
        Geometry::LineString(ls) => {
            if linestring_validity_error(&ls).is_none() {
                Geometry::LineString(ls)
            } else {
                Geometry::linestring(LineString::new(vec![]))
            }
        }
        Geometry::MultiLineString(mls) => {
            let valid_lines: Vec<LineString> = mls
                .lines()
                .iter()
                .filter(|l| linestring_validity_error(l).is_none())
                .cloned()
                .collect();
            if valid_lines.is_empty() {
                Geometry::linestring(LineString::new(vec![]))
            } else if valid_lines.len() == 1 {
                Geometry::linestring(valid_lines.into_iter().next().unwrap())
            } else {
                Geometry::multi_linestring(MultiLineString::new(valid_lines))
            }
        }
        other => other,
    }
}

/// Splits a self-intersecting linestring at intersection points, producing
/// a MultiLineString.
fn split_linestring_at_intersections(points: &[Point]) -> Geometry {
    let n = points.len();
    if n < 4 {
        return Geometry::linestring(LineString::new(points.to_vec()));
    }

    // Collect all intersection points.
    let mut intersections: Vec<(usize, usize, Point)> = Vec::new();
    for i in 0..n - 1 {
        for j in (i + 2)..n - 1 {
            if let Some(pt) = segment_intersection_point(
                &points[i],
                &points[i + 1],
                &points[j],
                &points[j + 1],
            ) {
                intersections.push((i, j, pt));
            }
        }
    }

    if intersections.is_empty() {
        return Geometry::linestring(LineString::new(points.to_vec()));
    }

    // Sort intersections by position along the linestring.
    intersections.sort_by_key(|a| a.0);

    // Split the linestring at intersection points.
    // We create a new set of line strings by cutting at each intersection.
    let mut result_lines: Vec<LineString> = Vec::new();
    let mut current_start = 0usize;

    for (i, j, pt) in &intersections {
        if *i >= current_start {
            let mut line_points: Vec<Point> = points[current_start..=*i].to_vec();
            line_points.push(*pt);
            result_lines.push(LineString::new(line_points));
        }
        current_start = *j + 1;
    }

    // Add the remaining tail.
    if current_start < n - 1 {
        let line_points: Vec<Point> = points[current_start..].to_vec();
        result_lines.push(LineString::new(line_points));
    }

    if result_lines.is_empty() {
        Geometry::linestring(LineString::new(points.to_vec()))
    } else if result_lines.len() == 1 {
        Geometry::linestring(result_lines.pop().unwrap())
    } else {
        Geometry::multi_linestring(MultiLineString::new(result_lines))
    }
}

/// Makes a polygon valid by fixing ring orientation, removing duplicate
/// consecutive points, and collapsing degenerate cases.
fn make_valid_polygon(poly: &Polygon) -> Geometry {
    let mut exterior = remove_consecutive_duplicates(poly.exterior());
    let mut interiors: Vec<Vec<Point>> = poly
        .interiors()
        .iter()
        .map(|h| remove_consecutive_duplicates(h))
        .collect();

    // Fix ring orientation: exterior CCW, holes CW.
    if ring_orientation(&exterior) == RingOrientation::CW {
        exterior.reverse();
    }
    for hole in &mut interiors {
        if ring_orientation(hole) == RingOrientation::CCW {
            hole.reverse();
        }
    }

    // Remove degenerate holes (fewer than 4 points after dedup).
    interiors.retain(|h| h.len() >= 4);

    // Try to build a valid polygon.
    if let Ok(candidate) = Polygon::from_rings(
        std::iter::once(exterior.clone())
            .chain(interiors.iter().cloned())
            .collect(),
    ) {
        if polygon_validity_error(&candidate).is_none() {
            return Geometry::polygon(candidate);
        }
    }

    // If the polygon has self-intersections, try to collapse to a
    // geometry collection of valid pieces.
    if ring_self_intersects(&exterior) {
        return collapse_polygon_to_valid(&exterior, &interiors);
    }

    // If holes are invalid, try removing them.
    if !interiors.is_empty() {
        if let Ok(candidate) = Polygon::from_rings(vec![exterior.clone()]) {
            if polygon_validity_error(&candidate).is_none() {
                return Geometry::polygon(candidate);
            }
        }
    }

    // Last resort: return an empty geometry.
    Geometry::polygon(Polygon::new(vec![]))
}

/// Attempts to collapse a self-intersecting polygon into valid pieces.
fn collapse_polygon_to_valid(exterior: &[Point], interiors: &[Vec<Point>]) -> Geometry {
    // Collect all intersection points in the exterior ring.
    let n = exterior.len();
    let mut intersections: Vec<(usize, usize, Point)> = Vec::new();
    for i in 0..n - 1 {
        for j in (i + 2)..n - 1 {
            if i == 0 && j == n - 2 {
                continue;
            }
            if let Some(pt) = segment_intersection_point(
                &exterior[i],
                &exterior[i + 1],
                &exterior[j],
                &exterior[j + 1],
            ) {
                intersections.push((i, j, pt));
            }
        }
    }

    if intersections.is_empty() {
        // No self-intersections detected — return the polygon as-is.
        let rings: Vec<Vec<Point>> = std::iter::once(exterior.to_vec())
            .chain(interiors.iter().cloned())
            .collect();
        if let Ok(poly) = Polygon::from_rings(rings) {
            return Geometry::polygon(poly);
        }
        return Geometry::polygon(Polygon::new(vec![]));
    }

    // Split the exterior ring at intersection points to create sub-rings.
    let sub_rings = split_ring_at_intersections(exterior, interiors);

    // Try to form valid polygons from the sub-rings.
    let mut valid_polys: Vec<Polygon> = Vec::new();
    for ring in &sub_rings {
        if ring.len() >= 4 && !ring_self_intersects(ring) {
            if let Ok(poly) = Polygon::from_rings(vec![ring.clone()]) {
                if polygon_validity_error(&poly).is_none() {
                    valid_polys.push(poly);
                }
            }
        }
    }

    if valid_polys.is_empty() {
        Geometry::polygon(Polygon::new(vec![]))
    } else if valid_polys.len() == 1 {
        Geometry::polygon(valid_polys.pop().unwrap())
    } else {
        Geometry::multi_polygon(MultiPolygon::new(valid_polys))
    }
}

/// Splits a ring at intersection points, returning sub-rings.
fn split_ring_at_intersections(
    ring: &[Point],
    _interiors: &[Vec<Point>],
) -> Vec<Vec<Point>> {
    let n = ring.len();
    let mut intersections: Vec<(usize, Point)> = Vec::new();
    for i in 0..n - 1 {
        for j in (i + 2)..n - 1 {
            if i == 0 && j == n - 2 {
                continue;
            }
            if let Some(pt) = segment_intersection_point(
                &ring[i],
                &ring[i + 1],
                &ring[j],
                &ring[j + 1],
            ) {
                intersections.push((i, pt));
            }
        }
    }

    if intersections.is_empty() {
        return vec![ring.to_vec()];
    }

    // Create sub-rings by cutting at intersection points.
    let mut sub_rings: Vec<Vec<Point>> = Vec::new();
    let mut used = vec![false; intersections.len()];

    for (idx, (seg_idx, pt)) in intersections.iter().enumerate() {
        if used[idx] {
            continue;
        }
        // Find the next intersection that forms a closed loop.
        let mut ring_points: Vec<Point> = Vec::new();
        ring_points.push(*pt);
        let mut current = *seg_idx;
        let mut found = false;
        for (next_idx, (next_seg, next_pt)) in intersections.iter().enumerate() {
            if next_idx == idx || used[next_idx] {
                continue;
            }
            if *next_seg > current {
                // Add points from current to next_seg.
                ring_points.extend(ring.iter().take(*next_seg + 1).skip(current + 1).copied());
                ring_points.push(*next_pt);
                used[next_idx] = true;
                current = *next_seg;
                found = true;
                break;
            }
        }
        if !found {
            // Close the ring back to the start.
            ring_points.extend(ring.iter().skip(current + 1).copied());
            // Close back to the first point.
            if ring_points[0] != ring_points[ring_points.len() - 1] {
                ring_points.push(ring_points[0]);
            }
        }
        if ring_points.len() >= 4 {
            sub_rings.push(ring_points);
        }
    }

    if sub_rings.is_empty() {
        vec![ring.to_vec()]
    } else {
        sub_rings
    }
}

/// Makes a multi-point valid.
fn make_valid_multi_point(mp: &MultiPoint) -> Geometry {
    Geometry::multi_point(mp.clone())
}

/// Makes a multi-linestring valid by validating each component.
fn make_valid_multi_linestring(mls: &MultiLineString) -> Geometry {
    let mut valid_lines: Vec<LineString> = Vec::new();
    for l in mls.lines() {
        match make_valid_linestring(l) {
            Geometry::LineString(ls) => valid_lines.push(ls),
            Geometry::MultiLineString(mls) => {
                valid_lines.extend(mls.lines().iter().cloned());
            }
            _ => {}
        }
    }

    if valid_lines.is_empty() {
        Geometry::multi_linestring(MultiLineString::new(vec![]))
    } else if valid_lines.len() == 1 {
        Geometry::linestring(valid_lines.into_iter().next().unwrap())
    } else {
        Geometry::multi_linestring(MultiLineString::new(valid_lines))
    }
}

/// Makes a multi-polygon valid by validating each component.
fn make_valid_multi_polygon(mp: &MultiPolygon) -> Geometry {
    let mut valid_polys: Vec<Polygon> = Vec::new();
    for poly in mp.polygons() {
        match make_valid_polygon(poly) {
            Geometry::Polygon(p) => valid_polys.push(p),
            Geometry::MultiPolygon(mp) => valid_polys.extend(mp.polygons().iter().cloned()),
            _ => {}
        }
    }

    if valid_polys.is_empty() {
        Geometry::multi_polygon(MultiPolygon::new(vec![]))
    } else if valid_polys.len() == 1 {
        Geometry::polygon(valid_polys.pop().unwrap())
    } else {
        Geometry::multi_polygon(MultiPolygon::new(valid_polys))
    }
}

/// Makes a geometry collection valid by validating each component.
fn make_valid_geometry_collection(gc: &GeometryCollection) -> Geometry {
    let mut valid_geoms: Vec<Geometry> = Vec::new();
    for g in gc.geometries() {
        let valid = make_valid_geometry(g);
        if !valid.is_empty() {
            valid_geoms.push(valid);
        }
    }

    if valid_geoms.is_empty() {
        Geometry::collection(GeometryCollection::new(vec![]))
    } else if valid_geoms.len() == 1 {
        valid_geoms.pop().unwrap()
    } else {
        Geometry::collection(GeometryCollection::new(valid_geoms))
    }
}

/// Makes any geometry valid, returning a valid geometry.
///
/// This is the top-level make-valid entry point, equivalent to
/// `GEOSMakeValid`.
pub(crate) fn make_valid_geometry(g: &Geometry) -> Geometry {
    match g {
        Geometry::Point(p) => {
            if is_valid_point(p) {
                Geometry::point(*p)
            } else {
                Geometry::point(Point::new(f64::NAN, f64::NAN))
            }
        }
        Geometry::LineString(ls) => make_valid_linestring(ls),
        Geometry::Polygon(p) => make_valid_polygon(p),
        Geometry::MultiPoint(mp) => make_valid_multi_point(mp),
        Geometry::MultiLineString(mls) => make_valid_multi_linestring(mls),
        Geometry::MultiPolygon(mp) => make_valid_multi_polygon(mp),
        Geometry::GeometryCollection(gc) => make_valid_geometry_collection(gc),
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

impl Point {
    /// Returns true if this point is valid.
    ///
    /// A point is valid unless its coordinates are NaN. Matches GEOS
    /// `GEOSisValid` for point types.
    pub fn is_valid(&self) -> bool {
        is_valid_point(self)
    }

    /// Returns the validity error for this point, if any.
    pub fn validity_error(&self) -> Option<ValidityError> {
        if is_valid_point(self) {
            None
        } else {
            Some(ValidityError::EmptyGeometry)
        }
    }

    /// Makes this point valid. Points are always valid unless NaN.
    pub fn make_valid(&self) -> Geometry {
        if is_valid_point(self) {
            Geometry::point(*self)
        } else {
            Geometry::point(Point::new(f64::NAN, f64::NAN))
        }
    }
}

impl LineString {
    /// Returns true if this linestring is valid.
    ///
    /// A linestring is valid when it has at least 2 points, no duplicate
    /// consecutive points, and no self-intersections. Matches GEOS
    /// `GEOSisValid` for linestring types.
    pub fn is_valid(&self) -> bool {
        linestring_validity_error(self).is_none()
    }

    /// Returns the validity error for this linestring, if any.
    pub fn validity_error(&self) -> Option<ValidityError> {
        linestring_validity_error(self)
    }

    /// Makes this linestring valid by removing duplicate consecutive points
    /// and splitting at self-intersections.
    pub fn make_valid(&self) -> Geometry {
        make_valid_linestring(self)
    }
}

impl Polygon {
    /// Returns true if this polygon is valid.
    ///
    /// A polygon is valid when:
    /// * the exterior ring has at least 4 points (closed triangle);
    /// * no ring has duplicate consecutive points;
    /// * no ring self-intersects;
    /// * all holes are inside the exterior ring;
    /// * holes do not overlap each other;
    /// * holes do not touch the exterior ring.
    ///
    /// Matches GEOS `GEOSisValid` for polygon types.
    pub fn is_valid(&self) -> bool {
        polygon_validity_error(self).is_none()
    }

    /// Returns the validity error for this polygon, if any.
    pub fn validity_error(&self) -> Option<ValidityError> {
        polygon_validity_error(self)
    }

    /// Makes this polygon valid by fixing ring orientation, removing
    /// duplicate consecutive points, and collapsing degenerate cases.
    pub fn make_valid(&self) -> Geometry {
        make_valid_polygon(self)
    }
}

impl MultiPoint {
    /// Returns true if this multi-point is valid.
    pub fn is_valid(&self) -> bool {
        multi_point_validity_error(self).is_none()
    }

    /// Returns the validity error for this multi-point, if any.
    pub fn validity_error(&self) -> Option<ValidityError> {
        multi_point_validity_error(self)
    }

    /// Makes this multi-point valid.
    pub fn make_valid(&self) -> Geometry {
        make_valid_multi_point(self)
    }
}

impl MultiLineString {
    /// Returns true if this multi-linestring is valid.
    pub fn is_valid(&self) -> bool {
        multi_linestring_validity_error(self).is_none()
    }

    /// Returns the validity error for this multi-linestring, if any.
    pub fn validity_error(&self) -> Option<ValidityError> {
        multi_linestring_validity_error(self)
    }

    /// Makes this multi-linestring valid.
    pub fn make_valid(&self) -> Geometry {
        make_valid_multi_linestring(self)
    }
}

impl MultiPolygon {
    /// Returns true if this multi-polygon is valid.
    pub fn is_valid(&self) -> bool {
        multi_polygon_validity_error(self).is_none()
    }

    /// Returns the validity error for this multi-polygon, if any.
    pub fn validity_error(&self) -> Option<ValidityError> {
        multi_polygon_validity_error(self)
    }

    /// Makes this multi-polygon valid.
    pub fn make_valid(&self) -> Geometry {
        make_valid_multi_polygon(self)
    }
}

impl GeometryCollection {
    /// Returns true if this geometry collection is valid.
    pub fn is_valid(&self) -> bool {
        geometry_collection_validity_error(self).is_none()
    }

    /// Returns the validity error for this geometry collection, if any.
    pub fn validity_error(&self) -> Option<ValidityError> {
        geometry_collection_validity_error(self)
    }

    /// Makes this geometry collection valid.
    pub fn make_valid(&self) -> Geometry {
        make_valid_geometry_collection(self)
    }
}

impl Geometry {
    /// Returns true if this geometry is valid.
    ///
    /// Matches GEOS `GEOSisValid` / QGIS `QgsGeometry::isValid()`.
    pub fn is_valid(&self) -> bool {
        geometry_validity_error(self).is_none()
    }

    /// Returns the validity error for this geometry, if any.
    ///
    /// Matches QGIS `QgsGeometry::isValid()` error string return.
    pub fn validity_error(&self) -> Option<ValidityError> {
        geometry_validity_error(self)
    }

    /// Makes this geometry valid.
    ///
    /// Matches GEOS `GEOSMakeValid` / QGIS `QgsGeometry::makeValid()`.
    /// Repairs invalid geometries by fixing ring orientation, removing
    /// duplicate consecutive points, splitting self-intersections, and
    /// collapsing degenerate cases.
    pub fn make_valid(&self) -> Geometry {
        make_valid_geometry(self)
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

    // ---- Point validity ----

    #[test]
    fn point_valid() {
        assert!(Point::new(0.0, 0.0).is_valid());
        assert!(Point::new(1.0, 2.0).is_valid());
        assert!(Point::new_z(0.0, 0.0, 5.0).is_valid());
        assert!(Point::new_m(0.0, 0.0, 1.0).is_valid());
    }

    #[test]
    fn point_invalid_nan() {
        assert!(!Point::new(f64::NAN, 0.0).is_valid());
        assert!(!Point::new(0.0, f64::NAN).is_valid());
        assert_eq!(
            Point::new(f64::NAN, 0.0).validity_error(),
            Some(ValidityError::EmptyGeometry)
        );
    }

    #[test]
    fn point_make_valid() {
        let p = Point::new(1.0, 2.0);
        let v = p.make_valid();
        assert!(v.is_valid());
        assert_eq!(v.as_point(), Some(&p));
    }

    #[test]
    fn point_make_valid_nan() {
        let p = Point::new(f64::NAN, 0.0);
        let v = p.make_valid();
        assert!(!v.is_valid());
    }

    // ---- LineString validity ----

    #[test]
    fn linestring_valid() {
        let ls = LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]);
        assert!(ls.is_valid());
        assert_eq!(ls.validity_error(), None);
    }

    #[test]
    fn linestring_too_few_points() {
        let ls = LineString::new(vec![Point::new(0.0, 0.0)]);
        assert!(!ls.is_valid());
        assert_eq!(
            ls.validity_error(),
            Some(ValidityError::TooFewPoints)
        );
    }

    #[test]
    fn linestring_empty() {
        let ls = LineString::new(vec![]);
        assert!(!ls.is_valid());
        assert_eq!(
            ls.validity_error(),
            Some(ValidityError::EmptyGeometry)
        );
    }

    #[test]
    fn linestring_duplicate_consecutive() {
        let ls = LineString::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
        ]);
        assert!(!ls.is_valid());
        assert_eq!(
            ls.validity_error(),
            Some(ValidityError::DuplicateConsecutivePoints)
        );
    }

    #[test]
    fn linestring_self_intersecting() {
        // Bowtie shape.
        let ls = LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0), (1.0, 0.0), (0.0, 1.0)]);
        assert!(!ls.is_valid());
        assert_eq!(
            ls.validity_error(),
            Some(ValidityError::SelfIntersection)
        );
    }

    #[test]
    fn linestring_not_self_intersecting() {
        // V shape — no self-intersection.
        let ls = LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)]);
        assert!(ls.is_valid());
    }

    // ---- LineString make_valid ----

    #[test]
    fn linestring_make_valid_already_valid() {
        let ls = LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]);
        let v = ls.make_valid();
        assert!(v.is_valid());
    }

    #[test]
    fn linestring_make_valid_removes_duplicates() {
        let ls = LineString::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
        ]);
        let v = ls.make_valid();
        assert!(v.is_valid());
    }

    #[test]
    fn linestring_make_valid_splits_self_intersection() {
        // Bowtie — should split into a MultiLineString or valid LineString.
        let ls = LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0), (1.0, 0.0), (0.0, 1.0)]);
        let v = ls.make_valid();
        assert!(v.is_valid());
    }

    // ---- Polygon validity ----

    #[test]
    fn polygon_valid() {
        let poly = Polygon::from_rings(vec![sq(0.0, 0.0, 10.0, 10.0)]).unwrap();
        assert!(poly.is_valid());
        assert_eq!(poly.validity_error(), None);
    }

    #[test]
    fn polygon_valid_with_hole() {
        let poly =
            Polygon::from_rings(vec![sq(0.0, 0.0, 10.0, 10.0), sq(2.0, 2.0, 8.0, 8.0)]).unwrap();
        assert!(poly.is_valid());
    }

    #[test]
    fn polygon_too_few_points() {
        let poly = Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 0.0),
        ]);
        assert!(!poly.is_valid());
        assert_eq!(
            poly.validity_error(),
            Some(ValidityError::TooFewPoints)
        );
    }

    #[test]
    fn polygon_duplicate_consecutive() {
        let ring = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
            Point::new(0.0, 0.0),
        ];
        // Use Polygon::new to bypass from_rings validation.
        let poly = Polygon::new(ring);
        assert!(!poly.is_valid());
        assert_eq!(
            poly.validity_error(),
            Some(ValidityError::DuplicateConsecutivePoints)
        );
    }

    #[test]
    fn polygon_self_intersecting_ring() {
        // Bowtie ring.
        let ring = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
            Point::new(0.0, 0.0),
        ];
        let poly = Polygon::from_rings(vec![ring]).unwrap();
        assert!(!poly.is_valid());
        assert_eq!(
            poly.validity_error(),
            Some(ValidityError::RingSelfIntersection)
        );
    }

    #[test]
    fn polygon_hole_outside_exterior() {
        let exterior = sq(0.0, 0.0, 5.0, 5.0);
        let hole = sq(10.0, 10.0, 15.0, 15.0);
        let poly = Polygon::from_rings(vec![exterior, hole]).unwrap();
        assert!(!poly.is_valid());
        assert_eq!(
            poly.validity_error(),
            Some(ValidityError::HoleOutsideExterior)
        );
    }

    #[test]
    fn polygon_holes_overlap() {
        let exterior = sq(0.0, 0.0, 10.0, 10.0);
        let hole1 = sq(2.0, 2.0, 5.0, 5.0);
        let hole2 = sq(4.0, 4.0, 7.0, 7.0);
        let poly = Polygon::from_rings(vec![exterior, hole1, hole2]).unwrap();
        assert!(!poly.is_valid());
        assert_eq!(
            poly.validity_error(),
            Some(ValidityError::HolesOverlap)
        );
    }

    // ---- Polygon make_valid ----

    #[test]
    fn polygon_make_valid_already_valid() {
        let poly = Polygon::from_rings(vec![sq(0.0, 0.0, 10.0, 10.0)]).unwrap();
        let v = poly.make_valid();
        assert!(v.is_valid());
    }

    #[test]
    fn polygon_make_valid_fixes_orientation() {
        // CW exterior ring — should be fixed to CCW.
        let cw_ring = vec![
            Point::new(0.0, 0.0),
            Point::new(0.0, 10.0),
            Point::new(10.0, 10.0),
            Point::new(10.0, 0.0),
            Point::new(0.0, 0.0),
        ];
        let poly = Polygon::from_rings(vec![cw_ring]).unwrap();
        // CW exterior is technically valid per OGC but GEOS prefers CCW.
        let v = poly.make_valid();
        assert!(v.is_valid());
        if let Some(p) = v.as_polygon() {
            assert_eq!(ring_orientation(p.exterior()), RingOrientation::CCW);
        }
    }

    #[test]
    fn polygon_make_valid_removes_duplicate_points() {
        let ring = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
            Point::new(0.0, 0.0),
        ];
        // Use Polygon::new to bypass from_rings validation.
        let poly = Polygon::new(ring);
        let v = poly.make_valid();
        assert!(v.is_valid());
    }

    // ---- MultiPoint validity ----

    #[test]
    fn multipoint_valid() {
        let mp = MultiPoint::new(vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)]);
        assert!(mp.is_valid());
    }

    #[test]
    fn multipoint_empty() {
        let mp = MultiPoint::new(vec![]);
        assert!(!mp.is_valid());
        assert_eq!(
            mp.validity_error(),
            Some(ValidityError::EmptyGeometry)
        );
    }

    #[test]
    fn multipoint_make_valid() {
        let mp = MultiPoint::new(vec![Point::new(0.0, 0.0)]);
        let v = mp.make_valid();
        assert!(v.is_valid());
    }

    // ---- MultiLineString validity ----

    #[test]
    fn multilinestring_valid() {
        let mls = MultiLineString::new(vec![
            LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)]),
            LineString::from_2d_coords(&[(2.0, 2.0), (3.0, 3.0)]),
        ]);
        assert!(mls.is_valid());
    }

    #[test]
    fn multilinestring_invalid_component() {
        let mls = MultiLineString::new(vec![
            LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)]),
            LineString::new(vec![Point::new(0.0, 0.0)]),
        ]);
        assert!(!mls.is_valid());
    }

    #[test]
    fn multilinestring_make_valid() {
        let mls = MultiLineString::new(vec![
            LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)]),
        ]);
        let v = mls.make_valid();
        assert!(v.is_valid());
    }

    // ---- MultiPolygon validity ----

    #[test]
    fn multipolygon_valid() {
        let mp = MultiPolygon::new(vec![
            Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap(),
            Polygon::from_rings(vec![sq(2.0, 2.0, 3.0, 3.0)]).unwrap(),
        ]);
        assert!(mp.is_valid());
    }

    #[test]
    fn multipolygon_overlapping() {
        let mp = MultiPolygon::new(vec![
            Polygon::from_rings(vec![sq(0.0, 0.0, 3.0, 3.0)]).unwrap(),
            Polygon::from_rings(vec![sq(2.0, 2.0, 5.0, 5.0)]).unwrap(),
        ]);
        assert!(!mp.is_valid());
        assert_eq!(
            mp.validity_error(),
            Some(ValidityError::OverlappingComponents)
        );
    }

    #[test]
    fn multipolygon_make_valid() {
        let mp = MultiPolygon::new(vec![
            Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap(),
        ]);
        let v = mp.make_valid();
        assert!(v.is_valid());
    }

    // ---- GeometryCollection validity ----

    #[test]
    fn geometry_collection_valid() {
        let gc = GeometryCollection::new(vec![
            Geometry::point(Point::new(0.0, 0.0)),
            Geometry::linestring(LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)])),
        ]);
        assert!(gc.is_valid());
    }

    #[test]
    fn geometry_collection_invalid_component() {
        let gc = GeometryCollection::new(vec![
            Geometry::point(Point::new(0.0, 0.0)),
            Geometry::linestring(LineString::new(vec![Point::new(0.0, 0.0)])),
        ]);
        assert!(!gc.is_valid());
    }

    #[test]
    fn geometry_collection_make_valid() {
        let gc = GeometryCollection::new(vec![
            Geometry::point(Point::new(0.0, 0.0)),
        ]);
        let v = gc.make_valid();
        assert!(v.is_valid());
    }

    // ---- Geometry enum dispatch ----

    #[test]
    fn geometry_is_valid_dispatch() {
        assert!(Geometry::point(Point::new(0.0, 0.0)).is_valid());
        assert!(Geometry::linestring(LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)])).is_valid());
        assert!(Geometry::polygon(Polygon::from_rings(vec![sq(0.0, 0.0, 1.0, 1.0)]).unwrap()).is_valid());
    }

    #[test]
    fn geometry_make_valid_dispatch() {
        let g = Geometry::linestring(LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)]));
        let v = g.make_valid();
        assert!(v.is_valid());
    }

    // ---- ValidityError display ----

    #[test]
    fn validity_error_display() {
        assert_eq!(
            ValidityError::EmptyGeometry.to_string(),
            "geometry is empty"
        );
        assert_eq!(
            ValidityError::SelfIntersection.to_string(),
            "geometry self-intersects"
        );
        assert_eq!(
            ValidityError::HoleOutsideExterior.to_string(),
            "hole lies outside exterior ring"
        );
    }

    // ---- Segment intersection edge cases ----

    #[test]
    fn segments_intersect_proper() {
        let a1 = Point::new(0.0, 0.0);
        let a2 = Point::new(1.0, 1.0);
        let b1 = Point::new(0.0, 1.0);
        let b2 = Point::new(1.0, 0.0);
        assert!(segments_intersect(&a1, &a2, &b1, &b2));
    }

    #[test]
    fn segments_intersect_endpoint() {
        let a1 = Point::new(0.0, 0.0);
        let a2 = Point::new(1.0, 0.0);
        let b1 = Point::new(1.0, 0.0);
        let b2 = Point::new(1.0, 1.0);
        assert!(segments_intersect(&a1, &a2, &b1, &b2));
    }

    #[test]
    fn segments_no_intersect() {
        let a1 = Point::new(0.0, 0.0);
        let a2 = Point::new(1.0, 0.0);
        let b1 = Point::new(0.0, 1.0);
        let b2 = Point::new(1.0, 1.0);
        assert!(!segments_intersect(&a1, &a2, &b1, &b2));
    }

    #[test]
    fn segments_collinear_overlap() {
        let a1 = Point::new(0.0, 0.0);
        let a2 = Point::new(2.0, 0.0);
        let b1 = Point::new(1.0, 0.0);
        let b2 = Point::new(3.0, 0.0);
        assert!(segments_intersect(&a1, &a2, &b1, &b2));
    }

    #[test]
    fn segments_collinear_no_overlap() {
        let a1 = Point::new(0.0, 0.0);
        let a2 = Point::new(1.0, 0.0);
        let b1 = Point::new(2.0, 0.0);
        let b2 = Point::new(3.0, 0.0);
        assert!(!segments_intersect(&a1, &a2, &b1, &b2));
    }

    // ---- Ring orientation ----

    #[test]
    fn ring_orientation_ccw() {
        let ring = sq(0.0, 0.0, 1.0, 1.0);
        assert_eq!(ring_orientation(&ring), RingOrientation::CCW);
    }

    #[test]
    fn ring_orientation_cw() {
        let ring = vec![
            Point::new(0.0, 0.0),
            Point::new(0.0, 1.0),
            Point::new(1.0, 1.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 0.0),
        ];
        assert_eq!(ring_orientation(&ring), RingOrientation::CW);
    }

    #[test]
    fn ring_orientation_degenerate() {
        let ring = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(0.0, 0.0),
        ];
        assert_eq!(ring_orientation(&ring), RingOrientation::Degenerate);
    }

    // ---- Remove consecutive duplicates ----

    #[test]
    fn remove_consecutive_duplicates_none() {
        let points = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
        ];
        let result = remove_consecutive_duplicates(&points);
        assert_eq!(result, points);
    }

    #[test]
    fn remove_consecutive_duplicates_some() {
        let points = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
        ];
        let result = remove_consecutive_duplicates(&points);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], Point::new(0.0, 0.0));
        assert_eq!(result[1], Point::new(1.0, 0.0));
        assert_eq!(result[2], Point::new(1.0, 1.0));
    }

    #[test]
    fn remove_consecutive_duplicates_closed_ring() {
        let points = sq(0.0, 0.0, 1.0, 1.0);
        let result = remove_consecutive_duplicates(&points);
        assert_eq!(result, points);
    }

    // ---- Point on ring boundary ----

    #[test]
    fn point_on_ring_boundary_true() {
        let ring = sq(0.0, 0.0, 1.0, 1.0);
        assert!(point_on_ring_boundary(&Point::new(0.5, 0.0), &ring));
        assert!(point_on_ring_boundary(&Point::new(0.0, 0.0), &ring));
    }

    #[test]
    fn point_on_ring_boundary_false() {
        let ring = sq(0.0, 0.0, 1.0, 1.0);
        assert!(!point_on_ring_boundary(&Point::new(0.5, 0.5), &ring));
        assert!(!point_on_ring_boundary(&Point::new(2.0, 2.0), &ring));
    }

    // ---- Ring inside ring ----

    #[test]
    fn ring_inside_ring_true() {
        let outer = sq(0.0, 0.0, 10.0, 10.0);
        let inner = sq(2.0, 2.0, 8.0, 8.0);
        assert!(ring_inside_ring(&inner, &outer));
    }

    #[test]
    fn ring_inside_ring_false() {
        let outer = sq(0.0, 0.0, 5.0, 5.0);
        let inner = sq(10.0, 10.0, 15.0, 15.0);
        assert!(!ring_inside_ring(&inner, &outer));
    }

    #[test]
    fn ring_inside_ring_touching() {
        let outer = sq(0.0, 0.0, 10.0, 10.0);
        let inner = sq(0.0, 0.0, 5.0, 5.0);
        // Inner shares the (0,0) corner with outer — should be false.
        assert!(!ring_inside_ring(&inner, &outer));
    }
}
