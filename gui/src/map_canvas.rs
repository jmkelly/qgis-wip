//! Map canvas drawing: turns style symbols into iced canvas geometry.
//!
//! The style crate produces device-independent [`qgis_style::CanvasGeometry`]
//! (filled rings and stroked paths). This module adapts that to the `iced`
//! canvas backend so the desktop GUI can paint a styled layer.

use iced::widget::canvas::{self, Fill, LineCap, LineDash, LineJoin, Path, Stroke};
use qgis_style::symbols::{CanvasGeometry, DashPattern, Geometry};
use qgis_style::Color;

/// A style color as an iced color.
pub fn to_iced_color(c: Color) -> iced::Color {
    iced::Color::from_rgba(
        c.r as f32 / 255.0,
        c.g as f32 / 255.0,
        c.b as f32 / 255.0,
        c.a as f32 / 255.0,
    )
}

/// Converts a style dash pattern into iced stroke segments.
pub fn to_line_dash(dash: &DashPattern) -> Vec<f32> {
    dash.segments().iter().map(|s| *s as f32).collect()
}

/// Builds an iced path from canvas geometry, closing every ring.
///
/// Rings with fewer than two vertices produce an empty path.
pub fn to_path(geometry: &CanvasGeometry) -> Path {
    let rings: Vec<&Vec<qgis_style::Point2>> = geometry
        .paths()
        .iter()
        .filter(|ring| ring.len() >= 2)
        .collect();
    Path::new(|builder| {
        for ring in &rings {
            let first = ring[0];
            builder.move_to(iced::Point::new(first.x as f32, first.y as f32));
            for p in ring.iter().skip(1) {
                builder.line_to(iced::Point::new(p.x as f32, p.y as f32));
            }
            builder.close();
        }
    })
}

/// Paints one canvas geometry onto an iced frame.
pub fn paint(frame: &mut canvas::Frame, geometry: &CanvasGeometry) {
    let path = to_path(geometry);
    if path.raw().iter().next().is_none() {
        return;
    }
    match geometry {
        CanvasGeometry::Fill { color, pattern, .. } => {
            if !pattern.paints_interior() {
                return;
            }
            frame.fill(
                &path,
                Fill {
                    style: to_iced_color(*color).into(),
                    ..Fill::default()
                },
            );
            if let qgis_style::FillPattern::Hatch(hatch) = pattern {
                // Hatch lines are approximated by stroking a light diagonal
                // set; exact hatch rendering arrives with the print renderer.
                let segs = hatch_segments(&path_bounds(geometry), *hatch);
                for seg in segs {
                    let hatch_path = Path::new(|b| {
                        b.move_to(iced::Point::new(seg.0 .0 as f32, seg.0 .1 as f32));
                        b.line_to(iced::Point::new(seg.1 .0 as f32, seg.1 .1 as f32));
                    });
                    frame.stroke(
                        &hatch_path,
                        Stroke {
                            style: to_iced_color(*color).into(),
                            width: hatch.line_width as f32,
                            line_cap: LineCap::Butt,
                            line_join: LineJoin::Miter,
                            line_dash: LineDash::default(),
                        },
                    );
                }
            }
        }
        CanvasGeometry::Stroke { color, width, dash, .. } => {
            let segments = to_line_dash(dash);
            frame.stroke(
                &path,
                Stroke {
                    style: to_iced_color(*color).into(),
                    width: *width as f32,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Miter,
                    line_dash: LineDash { segments: &segments, offset: 0 },
                },
            );
        }
    }
}

/// Axis-aligned bounds `(min_x, min_y, max_x, max_y)` of canvas geometry.
fn path_bounds(geometry: &CanvasGeometry) -> (f64, f64, f64, f64) {
    let mut bounds = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in geometry.paths().iter().flat_map(|ring| ring.iter()) {
        bounds.0 = bounds.0.min(p.x);
        bounds.1 = bounds.1.min(p.y);
        bounds.2 = bounds.2.max(p.x);
        bounds.3 = bounds.3.max(p.y);
    }
    bounds
}

/// Hatch line segments covering the given bounds at the hatch angle/spacing.
fn hatch_segments(
    bounds: &(f64, f64, f64, f64),
    hatch: qgis_style::Hatch,
) -> Vec<((f64, f64), (f64, f64))> {
    let (min_x, min_y, max_x, max_y) = *bounds;
    if min_x > max_x || min_y > max_y {
        return Vec::new();
    }
    let spacing = if hatch.spacing > 0.0 { hatch.spacing } else { 1.0 };
    let diagonal = ((max_x - min_x).powi(2) + (max_y - min_y).powi(2)).sqrt();
    let steps = (diagonal / spacing).ceil().max(0.0) as usize;
    let angle = hatch.angle_deg.to_radians();
    let (dx, dy) = (angle.cos(), angle.sin());
    // Perpendicular direction: how far hatch lines are offset from each other.
    let (ox, oy) = (-dy, dx);
    let cx = (min_x + max_x) / 2.0;
    let cy = (min_y + max_y) / 2.0;
    let mut segs = Vec::with_capacity(steps);
    for i in 0..=steps {
        let offset = (i as f64 - steps as f64 / 2.0) * spacing;
        let px = cx + ox * offset;
        let py = cy + oy * offset;
        segs.push((
            (px - dx * diagonal, py - dy * diagonal),
            (px + dx * diagonal, py + dy * diagonal),
        ));
    }
    segs
}

/// A drawable styled feature: a symbol plus the geometry it paints.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawable {
    /// Symbol used to paint the geometry.
    pub symbol: qgis_style::symbols::Symbol,
    /// Feature geometry.
    pub geometry: Geometry,
}

impl Drawable {
    /// Creates a drawable feature.
    pub fn new(symbol: qgis_style::symbols::Symbol, geometry: Geometry) -> Self {
        Self { symbol, geometry }
    }

    /// Renders this feature into canvas geometry.
    pub fn render(&self) -> Vec<CanvasGeometry> {
        self.symbol.render(&self.geometry)
    }
}

/// Collects the canvas geometry for a set of styled features.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MapCanvas {
    /// Features to draw, in paint order.
    pub features: Vec<Drawable>,
}

impl MapCanvas {
    /// Creates an empty canvas.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a styled feature to the canvas.
    pub fn push(&mut self, drawable: Drawable) -> &mut Self {
        self.features.push(drawable);
        self
    }

    /// Renders every feature into canvas geometry.
    pub fn render(&self) -> Vec<CanvasGeometry> {
        self.features.iter().flat_map(|f| f.render()).collect()
    }

    /// Paints every feature onto an iced frame.
    pub fn paint(&self, frame: &mut canvas::Frame) {
        for geometry in self.render() {
            paint(frame, &geometry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qgis_geometry::{LineString, Point, Polygon};
    use qgis_style::symbols::{LineSymbol, MarkerShape, PointSymbol, PolygonSymbol, Symbol};

    fn grey(v: u8) -> Color {
        Color { r: v, g: v, b: v, a: 255 }
    }

    #[test]
    fn color_conversion_maps_channels_and_alpha() {
        let c = to_iced_color(Color { r: 255, g: 128, b: 0, a: 0 });
        assert!((c.r - 1.0).abs() < 1e-6);
        assert!((c.g - 128.0 / 255.0).abs() < 1e-6);
        assert!(c.b.abs() < 1e-6);
        assert!(c.a.abs() < 1e-6);
    }

    #[test]
    fn dash_segments_convert_to_f32() {
        assert!(to_line_dash(&DashPattern::Solid).is_empty());
        assert_eq!(to_line_dash(&DashPattern::Custom(vec![4.0, 2.0])), vec![4.0f32, 2.0]);
    }

    #[test]
    fn path_building_skips_degenerate_rings() {
        let empty = CanvasGeometry::Fill {
            rings: vec![vec![]],
            color: grey(0),
            pattern: qgis_style::FillPattern::Solid,
        };
        assert_eq!(to_path(&empty).raw().iter().count(), 0);

        let fill = CanvasGeometry::Fill {
            rings: vec![
                vec![qgis_style::Point2::new(0.0, 0.0), qgis_style::Point2::new(4.0, 0.0), qgis_style::Point2::new(4.0, 4.0)],
                vec![qgis_style::Point2::new(0.0, 0.0), qgis_style::Point2::new(1.0, 0.0)],
            ],
            color: grey(0),
            pattern: qgis_style::FillPattern::Solid,
        };
        // lyon emits one element per point plus one `close` per ring:
        // (3 + 1) + (2 + 1) = 7.
        assert_eq!(to_path(&fill).raw().iter().count(), 7);
    }

    #[test]
    fn hatch_segments_cover_bounds_and_respect_spacing() {
        let hatch = qgis_style::Hatch { spacing: 2.0, angle_deg: 0.0, line_width: 0.5 };
        let segs = hatch_segments(&(0.0, 0.0, 10.0, 10.0), hatch);
        assert!(!segs.is_empty());
        for ((x0, y0), (x1, y1)) in &segs {
            assert!((y0 - y1).abs() < 1e-9, "0-degree hatch lines must be horizontal");
            assert!((x1 - x0) > 0.0);
        }
        assert!(hatch_segments(&(5.0, 5.0, 1.0, 1.0), hatch).is_empty());
    }

    #[test]
    fn map_canvas_renders_all_features() {
        let poly = Polygon::new(vec![
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 4.0),
        ]);
        let mut canvas = MapCanvas::new();
        canvas.push(Drawable::new(
            Symbol::Polygon(PolygonSymbol::new(grey(10))),
            Geometry::Polygon(poly),
        ));
        canvas.push(Drawable::new(
            Symbol::Point(PointSymbol::new(2.0, grey(20))),
            Geometry::Point(Point::new(1.0, 1.0)),
        ));
        canvas.push(Drawable::new(
            Symbol::Line(LineSymbol::new(grey(30), 1.0)),
            Geometry::Line(LineString::from_2d_coords(&[(0.0, 0.0), (3.0, 3.0)])),
        ));
        let rendered = canvas.render();
        assert_eq!(rendered.len(), 3);
        assert!(matches!(rendered[0], CanvasGeometry::Fill { .. }));
        assert!(matches!(rendered[1], CanvasGeometry::Fill { .. }));
        assert!(matches!(rendered[2], CanvasGeometry::Stroke { .. }));
    }

    #[test]
    fn mismatched_symbol_and_geometry_draw_nothing() {
        let canvas = MapCanvas {
            features: vec![Drawable::new(
                Symbol::Point(PointSymbol {
                    shape: MarkerShape::Star,
                    ..PointSymbol::default()
                }),
                Geometry::Line(LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)])),
            )],
        };
        assert!(canvas.render().is_empty());
    }
}