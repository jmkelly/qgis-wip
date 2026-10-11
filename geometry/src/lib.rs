//! Geometry primitives for the QGIS Rust port.
//!
//! Replaces GEOS functionality with pure-Rust implementations.
//! See ADR-0001 for the no-FFI policy decision.
//!
//! Current scope (bead qgis-rez.1, E0.5 GUI rendering POC): 2D-only
//! [`Point`] and [`Polygon`] with validated construction, signed area,
//! and even-odd point-in-polygon tests. Full GEOS parity (robust
//! predicates, overlay operations) is deferred to later hardening beads.

#![forbid(unsafe_code)]

/// Tolerance for the collinearity test in boundary detection.
///
/// The POC test cases use exactly representable coordinates, so the
/// epsilon only absorbs floating-point noise.
const BOUNDARY_EPSILON: f64 = 1e-12;

/// A 2D point in space.
///
/// 2D-only for the E0.5 POC: QGIS `QgsPoint` also carries z/m, but adding
/// them would touch every constructor and the rendering POC needs only
/// planar coordinates. Revisit when 3D/M support is scoped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    /// Creates a new point at the given coordinates.
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// Errors that can occur when constructing a [`Polygon`] via
/// [`Polygon::from_rings`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolygonError {
    /// No rings were provided; a polygon needs at least an exterior ring.
    MissingExteriorRing,
    /// The exterior ring is not closed (first and last vertices differ).
    UnclosedExteriorRing,
    /// An interior ring (hole) is not closed. Contains the index of the
    /// offending ring in the input (`0` is the exterior ring).
    UnclosedInteriorRing(usize),
    /// A ring repeats a vertex consecutively. Contains the index of the
    /// offending ring in the input (`0` is the exterior ring).
    DuplicateConsecutiveVertices(usize),
}

impl std::fmt::Display for PolygonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingExteriorRing => {
                write!(f, "no rings provided: a polygon needs at least an exterior ring")
            }
            Self::UnclosedExteriorRing => write!(f, "exterior ring is not closed"),
            Self::UnclosedInteriorRing(i) => write!(f, "interior ring {i} is not closed"),
            Self::DuplicateConsecutiveVertices(i) => {
                write!(f, "ring {i} has duplicate consecutive vertices")
            }
        }
    }
}

impl std::error::Error for PolygonError {}

/// A 2D polygon with an exterior ring and zero or more interior rings (holes).
///
/// * Construction via [`Polygon::from_rings`] validates that every ring is
///   closed and has no duplicate consecutive vertices.
/// * [`Polygon::area`] returns the signed area (shoelace formula); positive
///   when the exterior ring is counter-clockwise, with hole areas subtracted.
/// * [`Polygon::contains_point`] uses even-odd ray casting. Points on the
///   exterior boundary count as contained; points on a hole boundary count
///   as outside. Exact GEOS-consistent boundary semantics are deferred to
///   later hardening beads (E9), per the bead scope.
#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    /// The exterior ring (boundary) of the polygon.
    pub exterior: Vec<Point>,
    /// Interior rings (holes) of the polygon.
    pub interiors: Vec<Vec<Point>>,
}

impl Polygon {
    /// Creates a new polygon from an exterior ring, without validation.
    ///
    /// Use [`Polygon::from_rings`] for validated construction.
    pub fn new(exterior: Vec<Point>) -> Self {
        Self {
            exterior,
            interiors: Vec::new(),
        }
    }

    /// Creates a new polygon with exterior and interior rings, without
    /// validation.
    ///
    /// Use [`Polygon::from_rings`] for validated construction.
    pub fn with_holes(exterior: Vec<Point>, interiors: Vec<Vec<Point>>) -> Self {
        Self {
            exterior,
            interiors,
        }
    }

    /// Creates a polygon from a vector of rings, validating the structure.
    ///
    /// `rings[0]` is the exterior ring; the remaining rings are interior
    /// rings (holes). Validation:
    ///
    /// * at least one ring must be provided
    ///   ([`PolygonError::MissingExteriorRing`]);
    /// * every ring must be closed — first vertex equals last
    ///   ([`PolygonError::UnclosedExteriorRing`] /
    ///   [`PolygonError::UnclosedInteriorRing`]);
    /// * no ring may contain duplicate consecutive vertices
    ///   ([`PolygonError::DuplicateConsecutiveVertices`]).
    ///
    /// An empty exterior ring passes validation and yields an empty polygon
    /// (see [`Polygon::is_empty`]).
    pub fn from_rings(rings: Vec<Vec<Point>>) -> Result<Self, PolygonError> {
        let (exterior, interiors) = rings
            .split_first()
            .ok_or(PolygonError::MissingExteriorRing)?;
        validate_ring(exterior, 0)?;
        for (i, ring) in interiors.iter().enumerate() {
            validate_ring(ring, i + 1)?;
        }
        Ok(Self {
            exterior: exterior.clone(),
            interiors: interiors.to_vec(),
        })
    }

    /// Adds an interior ring (hole) to the polygon, without validation.
    pub fn add_hole(&mut self, hole: Vec<Point>) {
        self.interiors.push(hole);
    }

    /// Returns the exterior ring.
    pub fn exterior(&self) -> &[Point] {
        &self.exterior
    }

    /// Returns the interior rings (holes).
    pub fn interiors(&self) -> &[Vec<Point>] {
        &self.interiors
    }

    /// Returns the number of interior rings (holes).
    pub fn num_interiors(&self) -> usize {
        self.interiors.len()
    }

    /// Returns true if the polygon has no exterior vertices.
    pub fn is_empty(&self) -> bool {
        self.exterior.is_empty()
    }

    /// Returns the number of vertices in the exterior ring.
    pub fn exterior_len(&self) -> usize {
        self.exterior.len()
    }

    /// Returns true if the polygon has no holes.
    pub fn is_simple(&self) -> bool {
        self.interiors.is_empty()
    }

    /// Returns the signed area via the shoelace formula.
    ///
    /// Positive when the exterior ring is counter-clockwise. Hole areas
    /// (absolute value) are subtracted from the exterior area, so the result
    /// is positive for a CCW exterior with CW holes. Returns `0.0` for an
    /// empty polygon.
    pub fn area(&self) -> f64 {
        let exterior = signed_ring_area(&self.exterior);
        let holes: f64 = self
            .interiors
            .iter()
            .map(|hole| signed_ring_area(hole).abs())
            .sum();
        exterior - holes
    }

    /// Returns true if the point lies inside the polygon or on its
    /// exterior boundary.
    ///
    /// Even-odd ray casting over all rings: the point is contained when a
    /// ray from the point crosses the ring edges an odd number of times.
    /// Boundary rules (POC scope):
    ///
    /// * on the exterior ring boundary → contained (`true`);
    /// * on a hole boundary → not contained (`false`);
    /// * strictly inside a hole → not contained (`false`).
    ///
    /// GEOS-consistent boundary semantics are deferred (E9 hardening).
    pub fn contains_point(&self, point: &Point) -> bool {
        if self.is_empty() {
            return false;
        }
        if point_on_ring_boundary(point, &self.exterior) {
            return true;
        }
        for hole in &self.interiors {
            if point_on_ring_boundary(point, hole) {
                return false;
            }
        }
        let mut crossings = 0usize;
        for ring in std::iter::once(&self.exterior).chain(&self.interiors) {
            for edge in ring.windows(2) {
                if ray_crosses_edge(point, &edge[0], &edge[1]) {
                    crossings += 1;
                }
            }
        }
        crossings % 2 == 1
    }
}

/// Validates a single ring; `index` is the ring's position in the input
/// (`0` = exterior).
fn validate_ring(ring: &[Point], index: usize) -> Result<(), PolygonError> {
    if ring.first() != ring.last() {
        return Err(if index == 0 {
            PolygonError::UnclosedExteriorRing
        } else {
            PolygonError::UnclosedInteriorRing(index)
        });
    }
    for pair in ring.windows(2) {
        if pair[0] == pair[1] {
            return Err(PolygonError::DuplicateConsecutiveVertices(index));
        }
    }
    Ok(())
}

/// Signed area of a ring via the shoelace formula (positive for CCW).
fn signed_ring_area(ring: &[Point]) -> f64 {
    let mut sum = 0.0;
    for pair in ring.windows(2) {
        sum += pair[0].x * pair[1].y - pair[1].x * pair[0].y;
    }
    sum * 0.5
}

/// True if `p` lies on any edge of `ring` (within [`BOUNDARY_EPSILON`]).
fn point_on_ring_boundary(p: &Point, ring: &[Point]) -> bool {
    for pair in ring.windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        // Cross product of (b - a) and (p - a): zero when collinear.
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if cross.abs() > BOUNDARY_EPSILON {
            continue;
        }
        let (min_x, max_x) = if a.x < b.x { (a.x, b.x) } else { (b.x, a.x) };
        let (min_y, max_y) = if a.y < b.y { (a.y, b.y) } else { (b.y, a.y) };
        if p.x >= min_x - BOUNDARY_EPSILON
            && p.x <= max_x + BOUNDARY_EPSILON
            && p.y >= min_y - BOUNDARY_EPSILON
            && p.y <= max_y + BOUNDARY_EPSILON
        {
            return true;
        }
    }
    false
}

/// True if the horizontal ray from `p` in the +x direction crosses edge
/// `a`→`b`. Uses the half-open straddle convention so shared vertices are
/// counted once.
fn ray_crosses_edge(p: &Point, a: &Point, b: &Point) -> bool {
    if (a.y > p.y) == (b.y > p.y) {
        return false;
    }
    let x_intersect = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
    x_intersect > p.x
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Closed axis-aligned square ring from (x0, y0) to (x1, y1), CCW.
    fn sq(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
        vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
            Point::new(x0, y0),
        ]
    }

    fn unit_square() -> Vec<Point> {
        sq(0.0, 0.0, 1.0, 1.0)
    }

    #[test]
    fn point_new_sets_coordinates() {
        let p = Point::new(1.0, 2.0);
        assert_eq!(p.x, 1.0);
        assert_eq!(p.y, 2.0);
    }

    #[test]
    fn point_clone_equal() {
        let p1 = Point::new(3.0, 4.0);
        let p2 = p1;
        assert_eq!(p1, p2);
    }

    #[test]
    fn polygon_new_creates_simple_polygon() {
        let exterior = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
        ];
        let poly = Polygon::new(exterior.clone());
        assert_eq!(poly.exterior.len(), 4);
        assert!(poly.is_simple());
        assert_eq!(poly.exterior, exterior);
    }

    #[test]
    fn polygon_with_holes() {
        let exterior = vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        let hole = vec![
            Point::new(2.0, 2.0),
            Point::new(8.0, 2.0),
            Point::new(8.0, 8.0),
            Point::new(2.0, 8.0),
        ];
        let poly = Polygon::with_holes(exterior, vec![hole]);
        assert!(!poly.is_simple());
        assert_eq!(poly.interiors.len(), 1);
    }

    #[test]
    fn polygon_add_hole() {
        let exterior = vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        let mut poly = Polygon::new(exterior);
        assert!(poly.is_simple());
        poly.add_hole(vec![
            Point::new(2.0, 2.0),
            Point::new(8.0, 2.0),
            Point::new(8.0, 8.0),
            Point::new(2.0, 8.0),
        ]);
        assert!(!poly.is_simple());
        assert_eq!(poly.interiors.len(), 1);
    }

    #[test]
    fn polygon_from_rings_valid() {
        let rings = vec![sq(0.0, 0.0, 10.0, 10.0), sq(2.0, 2.0, 8.0, 8.0)];
        let poly = Polygon::from_rings(rings).unwrap();
        assert_eq!(poly.exterior().len(), 5);
        assert_eq!(poly.interiors().len(), 1);
        assert_eq!(poly.num_interiors(), 1);
        assert!(!poly.is_empty());
        assert!(!poly.is_simple());
    }

    #[test]
    fn polygon_from_rings_missing_exterior() {
        assert_eq!(
            Polygon::from_rings(vec![]),
            Err(PolygonError::MissingExteriorRing)
        );
    }

    #[test]
    fn polygon_from_rings_unclosed_exterior() {
        let unclosed = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
        ];
        assert_eq!(
            Polygon::from_rings(vec![unclosed]),
            Err(PolygonError::UnclosedExteriorRing)
        );
    }

    #[test]
    fn polygon_from_rings_unclosed_interior() {
        let rings = vec![
            unit_square(),
            vec![
                Point::new(2.0, 2.0),
                Point::new(3.0, 2.0),
                Point::new(3.0, 3.0),
                Point::new(2.0, 3.0),
            ],
        ];
        assert_eq!(
            Polygon::from_rings(rings),
            Err(PolygonError::UnclosedInteriorRing(1))
        );
    }

    #[test]
    fn polygon_from_rings_duplicate_consecutive_vertices() {
        let dup = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
            Point::new(0.0, 0.0),
        ];
        assert_eq!(
            Polygon::from_rings(vec![dup]),
            Err(PolygonError::DuplicateConsecutiveVertices(0))
        );
    }

    #[test]
    fn polygon_from_rings_duplicate_consecutive_vertices_in_hole() {
        let rings = vec![
            sq(0.0, 0.0, 10.0, 10.0),
            sq(1.0, 1.0, 2.0, 2.0),
            vec![
                Point::new(4.0, 4.0),
                Point::new(5.0, 4.0),
                Point::new(5.0, 4.0),
                Point::new(5.0, 5.0),
                Point::new(4.0, 5.0),
                Point::new(4.0, 4.0),
            ],
        ];
        assert_eq!(
            Polygon::from_rings(rings),
            Err(PolygonError::DuplicateConsecutiveVertices(2))
        );
    }

    #[test]
    fn polygon_from_rings_empty_exterior_yields_empty_polygon() {
        let poly = Polygon::from_rings(vec![vec![]]).unwrap();
        assert!(poly.is_empty());
        assert_eq!(poly.area(), 0.0);
        assert!(!poly.contains_point(&Point::new(0.0, 0.0)));
    }

    #[test]
    fn polygon_area_unit_square_ccw() {
        let poly = Polygon::from_rings(vec![unit_square()]).unwrap();
        assert_eq!(poly.area(), 1.0);
    }

    #[test]
    fn polygon_area_unit_square_cw() {
        let cw = vec![
            Point::new(0.0, 0.0),
            Point::new(0.0, 1.0),
            Point::new(1.0, 1.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 0.0),
        ];
        let poly = Polygon::from_rings(vec![cw]).unwrap();
        assert_eq!(poly.area(), -1.0);
    }

    #[test]
    fn polygon_area_triangle() {
        let triangle = vec![
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(0.0, 3.0),
            Point::new(0.0, 0.0),
        ];
        let poly = Polygon::from_rings(vec![triangle]).unwrap();
        assert_eq!(poly.area(), 6.0);
    }

    #[test]
    fn polygon_area_with_hole() {
        let poly = Polygon::from_rings(vec![sq(0.0, 0.0, 10.0, 10.0), sq(2.0, 2.0, 8.0, 8.0)])
            .unwrap();
        // 10x10 exterior minus 6x6 hole.
        assert_eq!(poly.area(), 64.0);
    }

    #[test]
    fn polygon_area_empty() {
        let poly = Polygon::new(vec![]);
        assert_eq!(poly.area(), 0.0);
    }

    #[test]
    fn polygon_contains_point_inside() {
        let poly = Polygon::from_rings(vec![sq(0.0, 0.0, 10.0, 10.0)]).unwrap();
        assert!(poly.contains_point(&Point::new(5.0, 5.0)));
    }

    #[test]
    fn polygon_contains_point_outside() {
        let poly = Polygon::from_rings(vec![unit_square()]).unwrap();
        assert!(!poly.contains_point(&Point::new(2.0, 2.0)));
        assert!(!poly.contains_point(&Point::new(-1.0, 0.5)));
    }

    #[test]
    fn polygon_contains_point_on_exterior_boundary() {
        let poly = Polygon::from_rings(vec![unit_square()]).unwrap();
        assert!(poly.contains_point(&Point::new(0.5, 0.0)));
        assert!(poly.contains_point(&Point::new(1.0, 0.5)));
        assert!(poly.contains_point(&Point::new(0.0, 1.0)));
    }

    #[test]
    fn polygon_contains_point_on_exterior_vertex() {
        let poly = Polygon::from_rings(vec![unit_square()]).unwrap();
        assert!(poly.contains_point(&Point::new(0.0, 0.0)));
        assert!(poly.contains_point(&Point::new(1.0, 1.0)));
    }

    #[test]
    fn polygon_contains_point_on_diagonal_boundary() {
        let triangle = vec![
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(0.0, 3.0),
            Point::new(0.0, 0.0),
        ];
        let poly = Polygon::from_rings(vec![triangle]).unwrap();
        // Midpoint of the (4,0)-(0,3) edge.
        assert!(poly.contains_point(&Point::new(2.0, 1.5)));
    }

    #[test]
    fn polygon_contains_point_on_hole_boundary() {
        let poly =
            Polygon::from_rings(vec![sq(0.0, 0.0, 10.0, 10.0), sq(2.0, 2.0, 8.0, 8.0)]).unwrap();
        assert!(!poly.contains_point(&Point::new(5.0, 2.0)));
        assert!(!poly.contains_point(&Point::new(8.0, 5.0)));
    }

    #[test]
    fn polygon_contains_point_in_hole() {
        let poly =
            Polygon::from_rings(vec![sq(0.0, 0.0, 10.0, 10.0), sq(2.0, 2.0, 8.0, 8.0)]).unwrap();
        assert!(!poly.contains_point(&Point::new(5.0, 5.0)));
    }

    #[test]
    fn polygon_contains_point_between_two_holes() {
        let poly = Polygon::from_rings(vec![
            sq(0.0, 0.0, 10.0, 10.0),
            sq(1.0, 1.0, 2.0, 2.0),
            sq(4.0, 4.0, 5.0, 5.0),
        ])
        .unwrap();
        // Between the two holes.
        assert!(poly.contains_point(&Point::new(3.0, 3.0)));
        // Inside each hole.
        assert!(!poly.contains_point(&Point::new(1.5, 1.5)));
        assert!(!poly.contains_point(&Point::new(4.5, 4.5)));
        // Near the exterior boundary, outside both holes.
        assert!(poly.contains_point(&Point::new(9.0, 9.0)));
    }

    #[test]
    fn polygon_contains_point_empty() {
        let poly = Polygon::new(vec![]);
        assert!(!poly.contains_point(&Point::new(0.0, 0.0)));
    }

    #[test]
    fn polygon_accessors() {
        let exterior = sq(0.0, 0.0, 10.0, 10.0);
        let hole = sq(2.0, 2.0, 8.0, 8.0);
        let poly = Polygon::from_rings(vec![exterior.clone(), hole.clone()]).unwrap();
        assert_eq!(poly.exterior(), exterior.as_slice());
        assert_eq!(poly.interiors(), std::slice::from_ref(&hole));
        assert_eq!(poly.num_interiors(), 1);
        assert!(!poly.is_empty());
        assert_eq!(poly.exterior_len(), 5);

        let empty = Polygon::new(vec![]);
        assert!(empty.is_empty());
        assert!(empty.exterior().is_empty());
        assert!(empty.interiors().is_empty());
        assert_eq!(empty.num_interiors(), 0);
        assert!(empty.is_simple());
    }

    #[test]
    fn polygon_equality_and_debug() {
        let a = Polygon::from_rings(vec![unit_square()]).unwrap();
        let b = Polygon::from_rings(vec![unit_square()]).unwrap();
        let c = Polygon::new(unit_square());
        assert_eq!(a, b);
        assert_eq!(a, c);
        let debug = format!("{a:?}");
        assert!(debug.starts_with("Polygon"));
    }

    #[test]
    fn polygon_error_display() {
        assert_eq!(
            PolygonError::MissingExteriorRing.to_string(),
            "no rings provided: a polygon needs at least an exterior ring"
        );
        assert_eq!(
            PolygonError::UnclosedExteriorRing.to_string(),
            "exterior ring is not closed"
        );
        assert_eq!(
            PolygonError::UnclosedInteriorRing(3).to_string(),
            "interior ring 3 is not closed"
        );
        assert_eq!(
            PolygonError::DuplicateConsecutiveVertices(2).to_string(),
            "ring 2 has duplicate consecutive vertices"
        );
    }
}
