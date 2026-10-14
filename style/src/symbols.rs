//! Renderable symbol types: point markers, line styles, and polygon fills.
//!
//! The QML parser in the crate root produces a raw [`crate::Symbol`] model
//! with string-valued properties. This module turns that model into typed,
//! device-independent symbols ([`PointSymbol`], [`LineSymbol`],
//! [`PolygonSymbol`]) that can be [`Symbol::render`]ed into canvas geometry:
//! filled rings and stroked paths ready for a raster or vector backend.

use qgis_geometry::{LineString, Point, Polygon};

use crate::Color;

/// A 2D canvas-space coordinate in device units (points/pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2 {
    /// Horizontal coordinate, increasing to the right.
    pub x: f64,
    /// Vertical coordinate, increasing downwards (canvas convention).
    pub y: f64,
}

impl Point2 {
    /// Creates a canvas coordinate.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl From<Point> for Point2 {
    fn from(p: Point) -> Self {
        Point2::new(p.x, p.y)
    }
}

/// A device-independent drawing primitive produced by rendering a symbol.
#[derive(Debug, Clone, PartialEq)]
pub enum CanvasGeometry {
    /// Fills the closed `rings` (first ring is the exterior, the rest are
    /// holes) with `color` using `pattern`.
    Fill {
        /// Closed rings to fill; interior rings act as holes.
        rings: Vec<Vec<Point2>>,
        /// Fill color.
        color: Color,
        /// Fill pattern applied to the interior of the rings.
        pattern: FillPattern,
    },
    /// Strokes the open (or closed) `path` with `color`.
    Stroke {
        /// Rings/polylines to stroke.
        path: Vec<Vec<Point2>>,
        /// Stroke color.
        color: Color,
        /// Stroke width in device units.
        width: f64,
        /// Dash pattern applied along each polyline.
        dash: DashPattern,
    },
}

impl CanvasGeometry {
    /// Returns the rings/polylines carried by this primitive.
    pub fn paths(&self) -> &[Vec<Point2>] {
        match self {
            CanvasGeometry::Fill { rings, .. } => rings,
            CanvasGeometry::Stroke { path, .. } => path,
        }
    }
}

/// Marker shape for a point symbol.
///
/// Mirrors the QGIS simple-marker `name` property. Unknown names fall back to
/// [`MarkerShape::Circle`], which is also QGIS's own default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarkerShape {
    /// Filled circle (default).
    #[default]
    Circle,
    /// Square.
    Square,
    /// Diamond (square rotated 45°).
    Diamond,
    /// Upward-pointing triangle.
    Triangle,
    /// Regular pentagon.
    Pentagon,
    /// Regular hexagon.
    Hexagon,
    /// Five-pointed star.
    Star,
    /// Plus sign.
    Cross,
    /// Rotated plus sign (X).
    Cross2,
}

impl MarkerShape {
    /// Parses a QGIS marker name, falling back to [`MarkerShape::Circle`].
    pub fn from_qgis(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "square" | "rectangle" => MarkerShape::Square,
            "diamond" => MarkerShape::Diamond,
            "triangle" => MarkerShape::Triangle,
            "pentagon" => MarkerShape::Pentagon,
            "hexagon" => MarkerShape::Hexagon,
            "star" => MarkerShape::Star,
            "cross" => MarkerShape::Cross,
            "cross2" | "x" => MarkerShape::Cross2,
            _ => MarkerShape::Circle,
        }
    }

    /// Returns the canonical QGIS name for this shape.
    pub fn qgis_name(self) -> &'static str {
        match self {
            MarkerShape::Circle => "circle",
            MarkerShape::Square => "square",
            MarkerShape::Diamond => "diamond",
            MarkerShape::Triangle => "triangle",
            MarkerShape::Pentagon => "pentagon",
            MarkerShape::Hexagon => "hexagon",
            MarkerShape::Star => "star",
            MarkerShape::Cross => "cross",
            MarkerShape::Cross2 => "cross2",
        }
    }

    /// Number of straight edges in the shape's outline (circles are
    /// approximated with a polygon of [`CIRCLE_SEGMENTS`] edges).
    pub fn edge_count(self) -> usize {
        match self {
            MarkerShape::Circle => CIRCLE_SEGMENTS,
            MarkerShape::Square | MarkerShape::Diamond => 4,
            MarkerShape::Triangle => 3,
            MarkerShape::Pentagon => 5,
            MarkerShape::Hexagon => 6,
            MarkerShape::Star => 10,
            MarkerShape::Cross | MarkerShape::Cross2 => 12,
        }
    }

    /// Builds the outline polygon of the marker centered at (`cx`, `cy`).
    ///
    /// `radius` is the distance from the center to each vertex (the
    /// circumradius), so every shape occupies the same bounding circle. The
    /// returned ring is closed (the last point repeats the first).
    pub fn outline(self, cx: f64, cy: f64, radius: f64) -> Vec<Point2> {
        let r = if radius < 0.0 { 0.0 } else { radius };
        match self {
            MarkerShape::Circle => regular(cx, cy, r, CIRCLE_SEGMENTS, 0.0),
            MarkerShape::Square => regular(cx, cy, r, 4, std::f64::consts::FRAC_PI_4),
            MarkerShape::Diamond => regular(cx, cy, r, 4, 0.0),
            MarkerShape::Triangle => regular(cx, cy, r, 3, -std::f64::consts::FRAC_PI_2),
            MarkerShape::Pentagon => regular(cx, cy, r, 5, -std::f64::consts::FRAC_PI_2),
            MarkerShape::Hexagon => regular(cx, cy, r, 6, 0.0),
            MarkerShape::Star => star(cx, cy, r),
            MarkerShape::Cross => cross(cx, cy, r, false),
            MarkerShape::Cross2 => cross(cx, cy, r, true),
        }
    }
}

/// Inner corner offset of a plus/cross marker: `r * (√2 − 1)`.
const CROSS_ARM: f64 = std::f64::consts::SQRT_2 - 1.0;

/// Builds a plus sign (or the 45°-rotated X variant) as a 12-vertex outline.
fn cross(cx: f64, cy: f64, r: f64, rotated: bool) -> Vec<Point2> {
    let w = r * CROSS_ARM;
    let local: [(f64, f64); 12] = if rotated {
        [
            (r, w),
            (w, w),
            (w, r),
            (-w, r),
            (-w, w),
            (-r, w),
            (-r, -w),
            (-w, -w),
            (-w, -r),
            (w, -r),
            (w, -w),
            (r, -w),
        ]
    } else {
        [
            (w, r),
            (w, w),
            (r, w),
            (r, -w),
            (w, -w),
            (w, -r),
            (-w, -r),
            (-w, -w),
            (-r, -w),
            (-r, w),
            (-w, w),
            (-w, r),
        ]
    };
    // A plus sign's outer corners sit at `√(w² + r²)`, just outside the arm
    // half-extents. Normalize so the corners land exactly on `radius` and every
    // shape of a given `size` occupies the same bounding circle.
    let corner = (w * w + r * r).sqrt();
    let k = if corner > 0.0 { r / corner } else { 0.0 };
    let mut pts: Vec<Point2> = local
        .iter()
        .map(|(dx, dy)| Point2::new(cx + dx * k, cy + dy * k))
        .collect();
    pts.push(pts[0]);
    pts
}

/// Number of straight segments used to approximate a circular marker.
pub const CIRCLE_SEGMENTS: usize = 32;

/// Builds a regular polygon with `n` vertices of circumradius `r`.
fn regular(cx: f64, cy: f64, r: f64, n: usize, phase: f64) -> Vec<Point2> {
    let n = if n == 0 { 3 } else { n };
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..n {
        let a = phase + (i as f64) * std::f64::consts::TAU / (n as f64);
        pts.push(Point2::new(cx + r * a.cos(), cy + r * a.sin()));
    }
    // Close by reusing the first vertex: recomputing it would accumulate
    // float error and leave a hairline gap in the outline.
    pts.push(pts[0]);
    pts
}

/// Builds a five-pointed star outline (outer radius `r`).
fn star(cx: f64, cy: f64, r: f64) -> Vec<Point2> {
    let inner = r * 0.382; // sin(18°)/sin(126°) ≈ 0.382
    let mut pts = Vec::with_capacity(11);
    for i in 0..10 {
        let radius = if i % 2 == 0 { r } else { inner };
        let a = -std::f64::consts::FRAC_PI_2 + (i as f64) * std::f64::consts::PI / 5.0;
        pts.push(Point2::new(cx + radius * a.cos(), cy + radius * a.sin()));
    }
    pts.push(pts[0]);
    pts
}

/// A dash pattern used when stroking line geometry.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum DashPattern {
    /// Unbroken line.
    #[default]
    Solid,
    /// Alternating on/off lengths, starting with an "on" segment.
    Custom(Vec<f64>),
}

impl DashPattern {
    /// Parses a QGIS line `style`/`use_custom_dash` property.
    ///
    /// Recognises `solid`/`no` as solid and the named QGIS dash styles,
    /// which are expressed as `[on, off, ...]` device-unit runs.
    pub fn from_qgis(style: &str) -> Self {
        match style.trim().to_ascii_lowercase().as_str() {
            "" | "no" | "solid" => DashPattern::Solid,
            "dash" | "dashed" => DashPattern::Custom(vec![4.0, 4.0]),
            "dot" | "dotted" => DashPattern::Custom(vec![0.1, 2.0]),
            "dash dot" | "dashdot" => DashPattern::Custom(vec![4.0, 2.0, 0.1, 2.0]),
            "dash dot dot" | "dashdotdot" => DashPattern::Custom(vec![4.0, 2.0, 0.1, 2.0, 0.1, 2.0]),
            _ => DashPattern::Solid,
        }
    }

    /// Dash lengths in device units; empty for [`DashPattern::Solid`].
    pub fn segments(&self) -> &[f64] {
        match self {
            DashPattern::Solid => &[],
            DashPattern::Custom(v) => v,
        }
    }

    /// Whether the pattern draws an unbroken line.
    pub fn is_solid(&self) -> bool {
        self.segments().is_empty()
    }
}

/// A hatched fill pattern drawn over a polygon's interior.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hatch {
    /// Distance between hatch lines, in device units.
    pub spacing: f64,
    /// Hatch line angle in degrees, measured from the positive x axis.
    pub angle_deg: f64,
    /// Hatch line width in device units.
    pub line_width: f64,
}

impl Hatch {
    /// Creates a hatch with QGIS's default spacing/width for the given
    /// density rank (1 = densest).
    pub fn from_qgis_density(density: u32) -> Self {
        let d = density.clamp(1, 7);
        Self {
            spacing: 8.0 - d as f64,
            angle_deg: 45.0,
            line_width: 0.5,
        }
    }
}

/// Fill treatment for polygon interiors.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FillPattern {
    /// Interior is left unpainted.
    NoFill,
    /// Interior is filled with a single color (default).
    #[default]
    Solid,
    /// Interior is filled with a color and overlaid with hatch lines.
    Hatch(Hatch),
}

impl FillPattern {
    /// Parses a QGIS polygon `style` property.
    ///
    /// `no` becomes [`FillPattern::NoFill`], `solid` becomes
    /// [`FillPattern::Solid`], and the hatch/dense styles map onto
    /// [`FillPattern::Hatch`].
    pub fn from_qgis(style: &str) -> Self {
        match style.trim().to_ascii_lowercase().as_str() {
            "no" | "none" => FillPattern::NoFill,
            "" | "solid" => FillPattern::Solid,
            other => {
                let density = other
                    .strip_prefix("dense")
                    .and_then(|d| d.parse::<u32>().ok())
                    .unwrap_or(3);
                FillPattern::Hatch(Hatch {
                    angle_deg: match other {
                        "horizontal" => 0.0,
                        "vertical" => 90.0,
                        _ => 45.0,
                    },
                    ..Hatch::from_qgis_density(density)
                })
            }
        }
    }

    /// Whether the interior receives any paint at all.
    pub fn paints_interior(self) -> bool {
        !matches!(self, FillPattern::NoFill)
    }
}

/// Point (marker) symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct PointSymbol {
    /// Marker outline shape.
    pub shape: MarkerShape,
    /// Marker diameter in device units.
    pub size: f64,
    /// Fill color.
    pub color: Color,
    /// Outline color.
    pub outline: Color,
    /// Outline width in device units; `0.0` disables the outline.
    pub outline_width: f64,
}

impl Default for PointSymbol {
    fn default() -> Self {
        Self {
            shape: MarkerShape::Circle,
            size: 4.0,
            color: Color { r: 0, g: 0, b: 0, a: 255 },
            outline: Color { r: 0, g: 0, b: 0, a: 255 },
            outline_width: 0.0,
        }
    }
}

impl PointSymbol {
    /// Creates a solid-filled circle marker.
    pub fn new(size: f64, color: Color) -> Self {
        Self { size, color, ..Self::default() }
    }

    /// Renders the marker for a point feature into canvas geometry.
    pub fn render(&self, point: &Point) -> Vec<CanvasGeometry> {
        let cx = point.x;
        let cy = point.y;
        let radius = (self.size / 2.0).abs();
        let ring = self.shape.outline(cx, cy, radius);
        let mut out = Vec::new();
        out.push(CanvasGeometry::Fill {
            rings: vec![ring.clone()],
            color: self.color,
            pattern: FillPattern::Solid,
        });
        if self.outline_width > 0.0 {
            out.push(CanvasGeometry::Stroke {
                path: vec![ring],
                color: self.outline,
                width: self.outline_width,
                dash: DashPattern::Solid,
            });
        }
        out
    }
}

/// Line symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct LineSymbol {
    /// Stroke color.
    pub color: Color,
    /// Stroke width in device units.
    pub width: f64,
    /// Dash pattern.
    pub dash: DashPattern,
}

impl Default for LineSymbol {
    fn default() -> Self {
        Self { color: Color { r: 0, g: 0, b: 0, a: 255 }, width: 0.5, dash: DashPattern::Solid }
    }
}

impl LineSymbol {
    /// Creates a solid line of the given width and color.
    pub fn new(color: Color, width: f64) -> Self {
        Self { color, width, dash: DashPattern::Solid }
    }

    /// Sets the dash pattern, returning the symbol for chaining.
    pub fn with_dash(mut self, dash: DashPattern) -> Self {
        self.dash = dash;
        self
    }

    /// Renders the line for a linestring feature into canvas geometry.
    ///
    /// Returns an empty vector when the line has no width or fewer than two
    /// vertices, since neither is drawable.
    pub fn render(&self, line: &LineString) -> Vec<CanvasGeometry> {
        if self.width <= 0.0 || line.len() < 2 {
            return Vec::new();
        }
        vec![CanvasGeometry::Stroke {
            path: vec![line.points().iter().copied().map(Point2::from).collect()],
            color: self.color,
            width: self.width,
            dash: self.dash.clone(),
        }]
    }
}

/// Polygon symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct PolygonSymbol {
    /// Interior fill color.
    pub fill: Color,
    /// Interior fill pattern.
    pub fill_pattern: FillPattern,
    /// Outline color.
    pub outline: Color,
    /// Outline width in device units; `0.0` disables the outline.
    pub outline_width: f64,
}

impl Default for PolygonSymbol {
    fn default() -> Self {
        Self {
            fill: Color { r: 200, g: 200, b: 200, a: 255 },
            fill_pattern: FillPattern::Solid,
            outline: Color { r: 0, g: 0, b: 0, a: 255 },
            outline_width: 0.3,
        }
    }
}

impl PolygonSymbol {
    /// Creates a solid-fill polygon symbol with no outline.
    pub fn new(fill: Color) -> Self {
        Self { fill, outline_width: 0.0, ..Self::default() }
    }

    /// Renders the polygon feature into canvas geometry: an interior fill
    /// (exterior ring plus holes) followed by ring outlines.
    pub fn render(&self, polygon: &Polygon) -> Vec<CanvasGeometry> {
        let mut rings: Vec<Vec<Point2>> = Vec::new();
        if polygon.exterior.len() >= 3 {
            rings.push(closed(&polygon.exterior));
            for hole in &polygon.interiors {
                if hole.len() >= 3 {
                    rings.push(closed(hole));
                }
            }
        }
        if rings.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        if self.fill_pattern.paints_interior() {
            out.push(CanvasGeometry::Fill {
                rings: rings.clone(),
                color: self.fill,
                pattern: self.fill_pattern,
            });
        }
        if self.outline_width > 0.0 {
            out.push(CanvasGeometry::Stroke {
                path: rings,
                color: self.outline,
                width: self.outline_width,
                dash: DashPattern::Solid,
            });
        }
        out
    }
}

/// Closes a ring by repeating its first vertex at the end.
fn closed(points: &[Point]) -> Vec<Point2> {
    let mut pts: Vec<Point2> = points.iter().copied().map(Point2::from).collect();
    if let Some(first) = pts.first().copied() {
        if pts.last() != Some(&first) {
            pts.push(first);
        }
    }
    pts
}

/// A typed, renderable symbol.
#[derive(Debug, Clone, PartialEq)]
pub enum Symbol {
    /// Point feature symbol.
    Point(PointSymbol),
    /// Line feature symbol.
    Line(LineSymbol),
    /// Polygon feature symbol.
    Polygon(PolygonSymbol),
}

impl Symbol {
    /// Renders a geometry with this symbol.
    ///
    /// The geometry must match the symbol's kind; mismatched pairs render
    /// nothing rather than panicking, so callers can stay lenient about what a
    /// layer's geometry type turned out to be.
    pub fn render(&self, geom: &Geometry) -> Vec<CanvasGeometry> {
        match (self, geom) {
            (Symbol::Point(s), Geometry::Point(p)) => s.render(p),
            (Symbol::Line(s), Geometry::Line(l)) => s.render(l),
            (Symbol::Polygon(s), Geometry::Polygon(p)) => s.render(p),
            _ => Vec::new(),
        }
    }
}

/// A feature geometry a symbol can be applied to.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    /// Single point.
    Point(Point),
    /// Polyline.
    Line(LineString),
    /// Area.
    Polygon(Polygon),
}

/// Converts the parser's raw QML symbol model into typed symbols.
///
/// This is the bridge from [`crate::parse_qml`] output (which carries
/// string-valued properties) to renderable [`Symbol`] values.
pub fn from_qml(raw: &crate::Symbol) -> Symbol {
    match raw {
        crate::Symbol::Point { shape, size, color, outline } => Symbol::Point(PointSymbol {
            shape: MarkerShape::from_qgis(shape),
            size: *size,
            color: *color,
            outline: *outline,
            outline_width: 0.0,
        }),
        crate::Symbol::Line { color, width } => Symbol::Line(LineSymbol::new(*color, *width)),
        crate::Symbol::Polygon { fill, outline, outline_width, style } => Symbol::Polygon(PolygonSymbol {
            fill: *fill,
            fill_pattern: FillPattern::from_qgis(style),
            outline: *outline,
            outline_width: *outline_width,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey(v: u8) -> Color {
        Color { r: v, g: v, b: v, a: 255 }
    }

    #[test]
    fn marker_shapes_parse_qgis_names() {
        assert_eq!(MarkerShape::from_qgis("star"), MarkerShape::Star);
        assert_eq!(MarkerShape::from_qgis("  Triangle "), MarkerShape::Triangle);
        assert_eq!(MarkerShape::from_qgis("totally-unknown"), MarkerShape::Circle);
        assert_eq!(MarkerShape::Square.qgis_name(), "square");
    }

    #[test]
    fn marker_outlines_are_closed_and_centred() {
        for shape in [
            MarkerShape::Circle,
            MarkerShape::Square,
            MarkerShape::Diamond,
            MarkerShape::Triangle,
            MarkerShape::Pentagon,
            MarkerShape::Hexagon,
            MarkerShape::Star,
            MarkerShape::Cross,
            MarkerShape::Cross2,
        ] {
            let ring = shape.outline(10.0, 20.0, 5.0);
            assert_eq!(ring.first(), ring.last(), "{:?} ring must be closed", shape);
            assert_eq!(ring.len(), shape.edge_count() + 1, "{:?} vertex count", shape);
            // No vertex may escape the bounding circumcircle. Convex shapes
            // (circle, square, diamond, regular polygons) reach it exactly;
            // concave ones (star, cross, cross2) only reach it on their arms.
            let mut max_d: f64 = 0.0;
            for p in &ring {
                let d = ((p.x - 10.0).powi(2) + (p.y - 20.0).powi(2)).sqrt();
                assert!(d <= 5.0 + 1e-9, "{:?} vertex escapes circumradius: {}", shape, d);
                max_d = max_d.max(d);
            }
            assert!(
                (max_d - 5.0).abs() < 1e-9,
                "{:?} outline should reach the circumradius, got {}",
                shape,
                max_d
            );
        }
    }

    #[test]
    fn circle_marker_renders_fill_and_outline() {
        let sym = PointSymbol {
            shape: MarkerShape::Circle,
            size: 6.0,
            color: grey(200),
            outline: grey(0),
            outline_width: 1.0,
        };
        let geoms = sym.render(&Point::new(0.0, 0.0));
        assert_eq!(geoms.len(), 2);
        match &geoms[0] {
            CanvasGeometry::Fill { rings, color, pattern } => {
                assert_eq!(color, &grey(200));
                assert_eq!(*pattern, FillPattern::Solid);
                // 32 segments + closing vertex.
                assert_eq!(rings[0].len(), CIRCLE_SEGMENTS + 1);
            }
            other => panic!("expected fill first, got {:?}", other),
        }
        match &geoms[1] {
            CanvasGeometry::Stroke { width, color, .. } => {
                assert_eq!(*width, 1.0);
                assert_eq!(*color, grey(0));
            }
            other => panic!("expected stroke second, got {:?}", other),
        }
    }

    #[test]
    fn marker_without_outline_width_emits_fill_only() {
        let sym = PointSymbol::new(4.0, grey(10));
        let geoms = sym.render(&Point::new(1.0, 2.0));
        assert_eq!(geoms.len(), 1);
        assert!(matches!(geoms[0], CanvasGeometry::Fill { .. }));
    }

    #[test]
    fn zero_size_marker_collapses_to_a_point_ring() {
        let geoms = PointSymbol::new(0.0, grey(1)).render(&Point::new(3.0, 4.0));
        let ring = geoms[0].paths()[0].clone();
        assert!(ring.iter().all(|p| *p == Point2::new(3.0, 4.0)));
    }

    #[test]
    fn line_symbol_strokes_linestring_with_dash() {
        let line = LineString::from_2d_coords(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
        let sym = LineSymbol::new(grey(7), 2.0).with_dash(DashPattern::Custom(vec![4.0, 2.0]));
        let geoms = sym.render(&line);
        assert_eq!(geoms.len(), 1);
        match &geoms[0] {
            CanvasGeometry::Stroke { path, width, dash, .. } => {
                assert_eq!(path.len(), 1);
                assert_eq!(path[0].len(), 3);
                assert_eq!(*width, 2.0);
                assert_eq!(dash.segments(), &[4.0, 2.0]);
            }
            other => panic!("expected stroke, got {:?}", other),
        }
    }

    #[test]
    fn line_symbol_skips_degenerate_geometry() {
        let sym = LineSymbol::new(grey(0), 1.0);
        assert!(sym.render(&LineString::from_2d_coords(&[(0.0, 0.0)])).is_empty());
        let zero_width = LineSymbol::new(grey(0), 0.0);
        assert!(zero_width
            .render(&LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)]))
            .is_empty());
    }

    #[test]
    fn dash_patterns_parse_qgis_styles() {
        assert!(DashPattern::from_qgis("solid").is_solid());
        assert!(DashPattern::from_qgis("no").is_solid());
        assert_eq!(DashPattern::from_qgis("dash dot").segments(), &[4.0, 2.0, 0.1, 2.0]);
        assert_eq!(DashPattern::from_qgis("dot").segments(), &[0.1, 2.0]);
    }

    #[test]
    fn fill_patterns_parse_qgis_styles() {
        assert_eq!(FillPattern::from_qgis("no"), FillPattern::NoFill);
        assert_eq!(FillPattern::from_qgis("solid"), FillPattern::Solid);
        match FillPattern::from_qgis("dense4") {
            FillPattern::Hatch(h) => {
                assert_eq!(h.spacing, 4.0);
                assert_eq!(h.angle_deg, 45.0);
            }
            other => panic!("expected hatch, got {:?}", other),
        }
        match FillPattern::from_qgis("vertical") {
            FillPattern::Hatch(h) => assert_eq!(h.angle_deg, 90.0),
            other => panic!("expected hatch, got {:?}", other),
        }
    }

    #[test]
    fn polygon_symbol_fills_exterior_and_holes() {
        let poly = Polygon::with_holes(
            vec![
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 10.0),
                Point::new(0.0, 10.0),
            ],
            vec![vec![
                Point::new(2.0, 2.0),
                Point::new(4.0, 2.0),
                Point::new(4.0, 4.0),
                Point::new(2.0, 4.0),
            ]],
        );
        let sym = PolygonSymbol::new(grey(30));
        let geoms = sym.render(&poly);
        assert_eq!(geoms.len(), 1, "outline_width 0 means fill only");
        match &geoms[0] {
            CanvasGeometry::Fill { rings, .. } => {
                assert_eq!(rings.len(), 2);
                assert_eq!(rings[0].first(), rings[0].last(), "exterior must be closed");
                assert_eq!(rings[1].len(), 5, "4 vertices plus the closing one");
            }
            other => panic!("expected fill, got {:?}", other),
        }
    }

    #[test]
    fn polygon_symbol_outlines_all_rings() {
        let poly = Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
        ]);
        let sym = PolygonSymbol { outline_width: 0.26, ..PolygonSymbol::default() };
        let geoms = sym.render(&poly);
        assert_eq!(geoms.len(), 2);
        match &geoms[1] {
            CanvasGeometry::Stroke { path, width, .. } => {
                assert_eq!(path.len(), 1);
                assert_eq!(*width, 0.26);
            }
            other => panic!("expected stroke, got {:?}", other),
        }
    }

    #[test]
    fn no_fill_polygon_emits_outline_only() {
        let poly = Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
        ]);
        let sym = PolygonSymbol { fill_pattern: FillPattern::NoFill, ..PolygonSymbol::default() };
        let geoms = sym.render(&poly);
        assert_eq!(geoms.len(), 1);
        assert!(matches!(geoms[0], CanvasGeometry::Stroke { .. }));
    }

    #[test]
    fn degenerate_polygon_renders_nothing() {
        let sym = PolygonSymbol::default();
        assert!(sym.render(&Polygon::new(vec![Point::new(0.0, 0.0)])).is_empty());
    }

    #[test]
    fn symbol_dispatch_matches_geometry_kind() {
        let sym = Symbol::Point(PointSymbol::new(2.0, grey(9)));
        assert_eq!(sym.render(&Geometry::Point(Point::new(0.0, 0.0))).len(), 1);
        let poly = Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
        ]);
        assert!(sym.render(&Geometry::Polygon(poly)).is_empty());
    }

    #[test]
    fn converts_parsed_qml_symbols() {
        let raw = crate::Symbol::Point {
            shape: "star".into(),
            size: 5.0,
            color: grey(1),
            outline: grey(2),
        };
        match from_qml(&raw) {
            Symbol::Point(p) => {
                assert_eq!(p.shape, MarkerShape::Star);
                assert_eq!(p.size, 5.0);
                assert_eq!(p.outline_width, 0.0);
            }
            other => panic!("expected point symbol, got {:?}", other),
        }

        let raw = crate::Symbol::Polygon {
            fill: grey(3),
            outline: grey(4),
            outline_width: 1.5,
            style: "dense2".into(),
        };
        match from_qml(&raw) {
            Symbol::Polygon(p) => {
                assert_eq!(p.outline_width, 1.5);
                assert!(matches!(p.fill_pattern, FillPattern::Hatch(_)));
            }
            other => panic!("expected polygon symbol, got {:?}", other),
        }

        let raw = crate::Symbol::Line { color: grey(5), width: 0.8 };
        match from_qml(&raw) {
            Symbol::Line(l) => {
                assert_eq!(l.width, 0.8);
                assert!(l.dash.is_solid());
            }
            other => panic!("expected line symbol, got {:?}", other),
        }
    }

    #[test]
    fn end_to_end_from_fixture() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../styles");
        let model = crate::parse_qml_file(dir.join("points_single_symbol.qml")).unwrap();
        let raw = match model.renderer {
            crate::Renderer::Single(s) => s,
            other => panic!("expected single renderer, got {:?}", other),
        };
        let sym = from_qml(&raw);
        assert!(matches!(sym, Symbol::Point(_)));
        let geoms = sym.render(&Geometry::Point(Point::new(0.0, 0.0)));
        assert_eq!(geoms.len(), 1);
    }
}