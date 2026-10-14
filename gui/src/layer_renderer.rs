//! Map rendering pipeline: layer + style + camera/viewport → canvas geometry.
//!
//! This module implements the E13 map rendering pipeline that connects
//! [`qgis_core::Layer`] and [`qgis_style::StyleModel`] to [`CanvasGeometry`]
//! and the `iced` [`canvas::Frame`]:
//!
//! - **E13.1 ([`LayerRenderer`])**: Trait and concrete renderers for point
//!   ([`PointLayerRenderer`]), line ([`LineLayerRenderer`]), polygon
//!   ([`PolygonLayerRenderer`]), and mixed vector layers
//!   ([`VectorLayerRenderer`]).
//! - **E13.2 (Viewport culling)**: Builds an [`STRtree`] spatial index over
//!   feature extents and queries it with the camera's visible world bounding
//!   box ([`viewport_bounding_box`]) so only visible features are styled and
//!   tessellated.
//! - **E13.3 (Symbol rendering to canvas)**: Projects visible geometries
//!   through [`Camera::lon_lat_to_screen`], applies symbol styling (colors,
//!   sizes, stroke widths, fill patterns, and layer opacity), and emits
//!   [`CanvasGeometry`] primitives and `iced` canvas [`canvas::Path`]s.

use std::f64::consts::PI;
use std::path::Path;

use iced::widget::canvas;
use qgis_core::{Feature, Layer};
use qgis_crs::{equirectangular_project, Camera, SrsId};
use qgis_geometry::{Geometry, LineString, Point, Polygon};
use qgis_spatial_index::{BoundingBox, STRtree};
use qgis_style::symbols::{
    from_qml, CanvasGeometry, LineSymbol, PointSymbol, PolygonSymbol, Symbol as TypedSymbol,
};
use qgis_style::StyleModel;

use crate::map_canvas::{paint, to_path};
use crate::style_apply::{classify_feature, read_shapefile_layer};

/// Trait for rendering a styled [`Layer`] within a camera viewport into
/// device-space [`CanvasGeometry`] primitives.
///
/// Implementations use [`qgis_spatial_index::STRtree`] to cull features outside
/// the visible viewport before classifying and projecting geometries into
/// screen space.
pub trait LayerRenderer {
    /// Renders the visible features of `layer` styled with `style` into
    /// device-space [`CanvasGeometry`] primitives for the given `camera` and
    /// `viewport` `(width, height)` in screen pixels.
    fn render(
        &self,
        layer: &Layer,
        style: &StyleModel,
        camera: &Camera,
        viewport: (f64, f64),
    ) -> Vec<CanvasGeometry>;

    /// Renders the visible features of `layer` using a pre-built spatial
    /// `index` (mapping feature bounding boxes to indices in `layer.features`).
    fn render_with_index(
        &self,
        layer: &Layer,
        index: &STRtree<usize>,
        style: &StyleModel,
        camera: &Camera,
        viewport: (f64, f64),
    ) -> Vec<CanvasGeometry> {
        let _ = index;
        self.render(layer, style, camera, viewport)
    }
}

/// Renderer for point and multipoint layers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PointLayerRenderer;

/// Alias for [`PointLayerRenderer`].
pub type PointRenderer = PointLayerRenderer;

impl LayerRenderer for PointLayerRenderer {
    fn render(
        &self,
        layer: &Layer,
        style: &StyleModel,
        camera: &Camera,
        viewport: (f64, f64),
    ) -> Vec<CanvasGeometry> {
        if !is_layer_drawable(layer, viewport) {
            return Vec::new();
        }
        let index = build_spatial_index(layer);
        self.render_with_index(layer, &index, style, camera, viewport)
    }

    fn render_with_index(
        &self,
        layer: &Layer,
        index: &STRtree<usize>,
        style: &StyleModel,
        camera: &Camera,
        viewport: (f64, f64),
    ) -> Vec<CanvasGeometry> {
        if !is_layer_drawable(layer, viewport) {
            return Vec::new();
        }
        let cam = camera.with_viewport(viewport);
        let visible = visible_feature_indices_with_index(index, &cam, viewport);
        let mut out = Vec::new();
        for idx in visible {
            if let Some(feature) = layer.features.get(idx) {
                if let (Some(geom), Some(raw_sym)) =
                    (&feature.geometry, classify_feature(&style.renderer, feature))
                {
                    let mut sym = from_qml(&raw_sym);
                    apply_opacity(&mut sym, layer.opacity);
                    if let TypedSymbol::Point(ps) = &sym {
                        render_point_geometry(geom, ps, &cam, &mut out);
                    }
                }
            }
        }
        out
    }
}

/// Renderer for linestring and multilinestring layers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineLayerRenderer;

/// Alias for [`LineLayerRenderer`].
pub type LineRenderer = LineLayerRenderer;

impl LayerRenderer for LineLayerRenderer {
    fn render(
        &self,
        layer: &Layer,
        style: &StyleModel,
        camera: &Camera,
        viewport: (f64, f64),
    ) -> Vec<CanvasGeometry> {
        if !is_layer_drawable(layer, viewport) {
            return Vec::new();
        }
        let index = build_spatial_index(layer);
        self.render_with_index(layer, &index, style, camera, viewport)
    }

    fn render_with_index(
        &self,
        layer: &Layer,
        index: &STRtree<usize>,
        style: &StyleModel,
        camera: &Camera,
        viewport: (f64, f64),
    ) -> Vec<CanvasGeometry> {
        if !is_layer_drawable(layer, viewport) {
            return Vec::new();
        }
        let cam = camera.with_viewport(viewport);
        let visible = visible_feature_indices_with_index(index, &cam, viewport);
        let mut out = Vec::new();
        for idx in visible {
            if let Some(feature) = layer.features.get(idx) {
                if let (Some(geom), Some(raw_sym)) =
                    (&feature.geometry, classify_feature(&style.renderer, feature))
                {
                    let mut sym = from_qml(&raw_sym);
                    apply_opacity(&mut sym, layer.opacity);
                    if let TypedSymbol::Line(ls) = &sym {
                        render_line_geometry(geom, ls, &cam, &mut out);
                    }
                }
            }
        }
        out
    }
}

/// Renderer for polygon and multipolygon layers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PolygonLayerRenderer;

/// Alias for [`PolygonLayerRenderer`].
pub type PolygonRenderer = PolygonLayerRenderer;

impl LayerRenderer for PolygonLayerRenderer {
    fn render(
        &self,
        layer: &Layer,
        style: &StyleModel,
        camera: &Camera,
        viewport: (f64, f64),
    ) -> Vec<CanvasGeometry> {
        if !is_layer_drawable(layer, viewport) {
            return Vec::new();
        }
        let index = build_spatial_index(layer);
        self.render_with_index(layer, &index, style, camera, viewport)
    }

    fn render_with_index(
        &self,
        layer: &Layer,
        index: &STRtree<usize>,
        style: &StyleModel,
        camera: &Camera,
        viewport: (f64, f64),
    ) -> Vec<CanvasGeometry> {
        if !is_layer_drawable(layer, viewport) {
            return Vec::new();
        }
        let cam = camera.with_viewport(viewport);
        let visible = visible_feature_indices_with_index(index, &cam, viewport);
        let mut out = Vec::new();
        for idx in visible {
            if let Some(feature) = layer.features.get(idx) {
                if let (Some(geom), Some(raw_sym)) =
                    (&feature.geometry, classify_feature(&style.renderer, feature))
                {
                    let mut sym = from_qml(&raw_sym);
                    apply_opacity(&mut sym, layer.opacity);
                    if let TypedSymbol::Polygon(ps) = &sym {
                        render_polygon_geometry(geom, ps, &cam, &mut out);
                    }
                }
            }
        }
        out
    }
}

/// Unified vector layer renderer that dispatches points, lines, and polygons
/// (including multi-part geometries and geometry collections) with spatial-index
/// viewport culling.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VectorLayerRenderer;

/// Alias for [`VectorLayerRenderer`].
pub type DefaultLayerRenderer = VectorLayerRenderer;

impl LayerRenderer for VectorLayerRenderer {
    fn render(
        &self,
        layer: &Layer,
        style: &StyleModel,
        camera: &Camera,
        viewport: (f64, f64),
    ) -> Vec<CanvasGeometry> {
        if !is_layer_drawable(layer, viewport) {
            return Vec::new();
        }
        let index = build_spatial_index(layer);
        self.render_with_index(layer, &index, style, camera, viewport)
    }

    fn render_with_index(
        &self,
        layer: &Layer,
        index: &STRtree<usize>,
        style: &StyleModel,
        camera: &Camera,
        viewport: (f64, f64),
    ) -> Vec<CanvasGeometry> {
        if !is_layer_drawable(layer, viewport) {
            return Vec::new();
        }
        let cam = camera.with_viewport(viewport);
        let visible = visible_feature_indices_with_index(index, &cam, viewport);
        let mut out = Vec::new();
        for idx in visible {
            if let Some(feature) = layer.features.get(idx) {
                if let (Some(geom), Some(raw_sym)) =
                    (&feature.geometry, classify_feature(&style.renderer, feature))
                {
                    let mut sym = from_qml(&raw_sym);
                    apply_opacity(&mut sym, layer.opacity);
                    render_typed_symbol_geometry(geom, &sym, &cam, &mut out);
                }
            }
        }
        out
    }
}

/// Checks whether a layer can produce any visible geometry in the given viewport.
fn is_layer_drawable(layer: &Layer, viewport: (f64, f64)) -> bool {
    layer.visible && layer.opacity > 0.0 && viewport.0 > 0.0 && viewport.1 > 0.0
}

/// Builds an [`STRtree`] spatial index over the features of `layer`.
///
/// Each entry stores the feature's 2D bounding box and its index in
/// `layer.features`. Features without a geometry or with an empty geometry are
/// skipped.
pub fn build_spatial_index(layer: &Layer) -> STRtree<usize> {
    let items: Vec<(BoundingBox, usize)> = layer
        .features
        .iter()
        .enumerate()
        .filter_map(|(idx, feature)| {
            let (min_x, min_y, max_x, max_y) = feature.bounding_box()?;
            Some((BoundingBox::new(min_x, min_y, max_x, max_y), idx))
        })
        .collect();
    STRtree::bulk_load(items)
}

/// Computes the visible world/layer bounding box for `camera` and `viewport`
/// `(width, height)` in screen pixels.
///
/// Returns an empty [`BoundingBox`] when `viewport` has zero or negative width
/// or height.
pub fn viewport_bounding_box(camera: &Camera, viewport: (f64, f64)) -> BoundingBox {
    if viewport.0 <= 0.0 || viewport.1 <= 0.0 {
        return BoundingBox::default();
    }
    let cam = camera.with_viewport(viewport);
    let (x0, y0) = cam.screen_to_lon_lat((0.0, 0.0));
    let (x1, y1) = cam.screen_to_lon_lat(viewport);
    BoundingBox::from_corners(x0, y0, x1, y1)
}

/// Returns the sorted feature indices in `layer` whose bounding boxes intersect
/// the visible viewport, using a newly bulk-loaded [`STRtree`].
pub fn visible_feature_indices(
    layer: &Layer,
    camera: &Camera,
    viewport: (f64, f64),
) -> Vec<usize> {
    let index = build_spatial_index(layer);
    visible_feature_indices_with_index(&index, camera, viewport)
}

/// Returns the sorted feature indices from `index` whose bounding boxes
/// intersect the visible viewport of `camera` and `viewport`.
pub fn visible_feature_indices_with_index(
    index: &STRtree<usize>,
    camera: &Camera,
    viewport: (f64, f64),
) -> Vec<usize> {
    let query = viewport_bounding_box(camera, viewport);
    if query.is_empty() {
        return Vec::new();
    }
    let mut indices: Vec<usize> = index.query_bbox(&query).into_iter().copied().collect();
    indices.sort_unstable();
    indices.dedup();
    indices
}

/// Returns references to the features in `layer` whose bounding boxes intersect
/// the visible viewport, in layer order.
pub fn cull_visible_features<'a>(
    layer: &'a Layer,
    camera: &Camera,
    viewport: (f64, f64),
) -> Vec<&'a Feature> {
    visible_feature_indices(layer, camera, viewport)
        .into_iter()
        .filter_map(|idx| layer.features.get(idx))
        .collect()
}

/// Scales the alpha channel of every color in `symbol` by `opacity` (clamped to
/// `0.0..=1.0`).
pub fn apply_opacity(symbol: &mut TypedSymbol, opacity: f32) {
    let factor = opacity.clamp(0.0, 1.0);
    if (factor - 1.0).abs() <= f32::EPSILON {
        return;
    }
    let scale = |a: u8| -> u8 { ((a as f32) * factor).round().clamp(0.0, 255.0) as u8 };
    match symbol {
        TypedSymbol::Point(ps) => {
            ps.color.a = scale(ps.color.a);
            ps.outline.a = scale(ps.outline.a);
        }
        TypedSymbol::Line(ls) => {
            ls.color.a = scale(ls.color.a);
        }
        TypedSymbol::Polygon(ps) => {
            ps.fill.a = scale(ps.fill.a);
            ps.outline.a = scale(ps.outline.a);
        }
    }
}

/// Projects a single point from layer coordinates to screen pixel coordinates.
fn project_point(cam: &Camera, p: &Point) -> Point {
    let (sx, sy) = cam.lon_lat_to_screen(p.x, p.y);
    Point::new(sx, sy)
}

/// Projects a linestring from layer coordinates to screen pixel coordinates.
fn project_linestring(cam: &Camera, ls: &LineString) -> LineString {
    LineString::new(ls.points().iter().map(|p| project_point(cam, p)).collect())
}

/// Projects a polygon (exterior ring and interior holes) from layer coordinates
/// to screen pixel coordinates.
fn project_polygon(cam: &Camera, poly: &Polygon) -> Polygon {
    let exterior: Vec<Point> = poly
        .exterior
        .iter()
        .map(|p| project_point(cam, p))
        .collect();
    let interiors: Vec<Vec<Point>> = poly
        .interiors
        .iter()
        .map(|ring| ring.iter().map(|p| project_point(cam, p)).collect())
        .collect();
    if interiors.is_empty() {
        Polygon::new(exterior)
    } else {
        Polygon::with_holes(exterior, interiors)
    }
}

fn render_point_geometry(
    geom: &Geometry,
    sym: &PointSymbol,
    cam: &Camera,
    out: &mut Vec<CanvasGeometry>,
) {
    match geom {
        Geometry::Point(p) => {
            out.extend(sym.render(&project_point(cam, p)));
        }
        Geometry::MultiPoint(mp) => {
            for p in mp.points() {
                out.extend(sym.render(&project_point(cam, p)));
            }
        }
        Geometry::GeometryCollection(gc) => {
            for g in gc.geometries() {
                render_point_geometry(g, sym, cam, out);
            }
        }
        _ => {}
    }
}

fn render_line_geometry(
    geom: &Geometry,
    sym: &LineSymbol,
    cam: &Camera,
    out: &mut Vec<CanvasGeometry>,
) {
    match geom {
        Geometry::LineString(ls) => {
            out.extend(sym.render(&project_linestring(cam, ls)));
        }
        Geometry::MultiLineString(mls) => {
            for ls in mls.lines() {
                out.extend(sym.render(&project_linestring(cam, ls)));
            }
        }
        Geometry::GeometryCollection(gc) => {
            for g in gc.geometries() {
                render_line_geometry(g, sym, cam, out);
            }
        }
        _ => {}
    }
}

fn render_polygon_geometry(
    geom: &Geometry,
    sym: &PolygonSymbol,
    cam: &Camera,
    out: &mut Vec<CanvasGeometry>,
) {
    match geom {
        Geometry::Polygon(poly) => {
            out.extend(sym.render(&project_polygon(cam, poly)));
        }
        Geometry::MultiPolygon(mpoly) => {
            for poly in mpoly.polygons() {
                out.extend(sym.render(&project_polygon(cam, poly)));
            }
        }
        Geometry::GeometryCollection(gc) => {
            for g in gc.geometries() {
                render_polygon_geometry(g, sym, cam, out);
            }
        }
        _ => {}
    }
}

fn render_typed_symbol_geometry(
    geom: &Geometry,
    sym: &TypedSymbol,
    cam: &Camera,
    out: &mut Vec<CanvasGeometry>,
) {
    match sym {
        TypedSymbol::Point(ps) => render_point_geometry(geom, ps, cam, out),
        TypedSymbol::Line(ls) => render_line_geometry(geom, ls, cam, out),
        TypedSymbol::Polygon(ps) => render_polygon_geometry(geom, ps, cam, out),
    }
}

/// Convenience function: renders `layer` styled with `style` using
/// [`VectorLayerRenderer`].
pub fn render_layer(
    layer: &Layer,
    style: &StyleModel,
    camera: &Camera,
    viewport: (f64, f64),
) -> Vec<CanvasGeometry> {
    VectorLayerRenderer.render(layer, style, camera, viewport)
}

/// Convenience function: renders `layer` styled with `style` using a pre-built
/// spatial `index`.
pub fn render_layer_with_index(
    layer: &Layer,
    index: &STRtree<usize>,
    style: &StyleModel,
    camera: &Camera,
    viewport: (f64, f64),
) -> Vec<CanvasGeometry> {
    VectorLayerRenderer.render_with_index(layer, index, style, camera, viewport)
}

/// Builds `iced` canvas [`Path`](canvas::Path)s for all rendered primitives of a
/// styled layer.
pub fn render_layer_paths(
    layer: &Layer,
    style: &StyleModel,
    camera: &Camera,
    viewport: (f64, f64),
) -> Vec<canvas::Path> {
    render_layer(layer, style, camera, viewport)
        .iter()
        .map(to_path)
        .collect()
}

/// Paints a styled `layer` onto an `iced` [`canvas::Frame`] with viewport
/// culling.
pub fn paint_layer(
    frame: &mut canvas::Frame,
    layer: &Layer,
    style: &StyleModel,
    camera: &Camera,
    viewport: (f64, f64),
) {
    for geom in render_layer(layer, style, camera, viewport) {
        paint(frame, &geom);
    }
}

/// Produces a compiled `iced` [`canvas::Geometry`] ready for `Program::draw` by
/// painting the visible subset of `layer` onto a new [`canvas::Frame`].
pub fn draw_layer(
    renderer: &iced::Renderer,
    layer: &Layer,
    style: &StyleModel,
    camera: &Camera,
    viewport: (f64, f64),
) -> canvas::Geometry<iced::Renderer> {
    let size = iced::Size::new(viewport.0.max(1.0) as f32, viewport.1.max(1.0) as f32);
    let mut frame = canvas::Frame::new(renderer, size);
    paint_layer(&mut frame, layer, style, camera, viewport);
    frame.into_geometry()
}

/// Computes a [`Camera`] centered on `extent` `(min_x, min_y, max_x, max_y)`
/// that fits the extent inside `viewport` `(width, height)` with a 10% margin.
pub fn camera_for_extent(extent: (f64, f64, f64, f64), viewport: (f64, f64)) -> Camera {
    let (min_x, min_y, max_x, max_y) = extent;
    let cx = (min_x + max_x) / 2.0;
    let cy = (min_y + max_y) / 2.0;
    let center = equirectangular_project(cx, cy);

    let dx_rad = (max_x - min_x).abs().to_radians();
    let dy_rad = (max_y - min_y).abs().to_radians();

    let w = viewport.0.max(1.0) * 0.9;
    let h = viewport.1.max(1.0) * 0.9;

    let scale_x = if dx_rad > 1e-12 {
        w * (2.0 * PI) / dx_rad
    } else {
        f64::INFINITY
    };
    let scale_y = if dy_rad > 1e-12 {
        h * (2.0 * PI) / dy_rad
    } else {
        f64::INFINITY
    };
    let scale = scale_x.min(scale_y);
    let zoom = if scale.is_finite() && scale > 0.0 {
        (scale / 256.0).log2().clamp(-4.0, 24.0)
    } else {
        4.0
    };

    Camera::new(center, zoom, viewport)
}

/// Loads a shapefile from `path` into a [`Layer`] with EPSG:4326 CRS.
pub fn load_shapefile_as_layer(path: impl AsRef<Path>) -> Result<Layer, String> {
    let path_ref = path.as_ref();
    let features = read_shapefile_layer(path_ref)?;
    let name = path_ref
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("layer")
        .to_string();
    let mut layer = Layer::new(name, SrsId::from_epsg(4326));
    layer.features = features;
    Ok(layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qgis_core::Value;
    use qgis_geometry::{GeometryCollection, MultiLineString, MultiPoint, MultiPolygon};
    use qgis_style::{Color, FillPattern, Renderer, Symbol};

    fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    fn point_style(shape: &str, size: f64, color: Color) -> StyleModel {
        StyleModel {
            renderer: Renderer::Single(Symbol::Point {
                shape: shape.to_string(),
                size,
                color,
                outline: rgba(0, 0, 0, 255),
            }),
        }
    }

    fn line_style(color: Color, width: f64) -> StyleModel {
        StyleModel {
            renderer: Renderer::Single(Symbol::Line { color, width }),
        }
    }

    fn polygon_style(fill: Color, outline: Color, outline_width: f64, style: &str) -> StyleModel {
        StyleModel {
            renderer: Renderer::Single(Symbol::Polygon {
                fill,
                outline,
                outline_width,
                style: style.to_string(),
            }),
        }
    }

    #[test]
    fn point_layer_renderer_renders_visible_points_to_canvas() {
        let mut layer = Layer::new("pts", SrsId::from_epsg(4326));
        layer
            .features
            .push(Feature::new(1, Some(Geometry::Point(Point::new(0.0, 0.0)))));
        // Far outside the zoomed-in viewport:
        layer.features.push(Feature::new(
            2,
            Some(Geometry::Point(Point::new(150.0, 75.0))),
        ));

        let style = point_style("circle", 6.0, rgba(255, 0, 0, 255));
        let viewport = (800.0, 600.0);
        let camera = Camera::new((0.0, 0.0), 5.0, viewport);

        let geoms = PointLayerRenderer.render(&layer, &style, &camera, viewport);
        assert_eq!(geoms.len(), 1, "only the in-viewport point should render");

        match &geoms[0] {
            CanvasGeometry::Fill {
                rings,
                color,
                pattern,
            } => {
                assert_eq!(*color, rgba(255, 0, 0, 255));
                assert_eq!(*pattern, FillPattern::Solid);
                assert!(!rings[0].is_empty());
                // At center (0, 0), screen coordinates are centered around (400, 300) with radius 3.0.
                for p in &rings[0] {
                    let dist = ((p.x - 400.0).powi(2) + (p.y - 300.0).powi(2)).sqrt();
                    assert!(
                        (dist - 3.0).abs() < 1e-6,
                        "circle vertex should lie at radius 3.0 from viewport center, got {dist}"
                    );
                }
            }
            other => panic!("expected Fill for point marker, got {:?}", other),
        }

        // Verify iced Path construction succeeds.
        let paths = render_layer_paths(&layer, &style, &camera, viewport);
        assert_eq!(paths.len(), 1);
        assert!(paths[0].raw().iter().next().is_some());
    }

    #[test]
    fn line_layer_renderer_renders_visible_lines_to_canvas() {
        let mut layer = Layer::new("roads", SrsId::from_epsg(4326));
        let visible_line = LineString::new(vec![
            Point::new(-1.0, -1.0),
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
        ]);
        let offscreen_line =
            LineString::new(vec![Point::new(120.0, 60.0), Point::new(130.0, 65.0)]);
        layer
            .features
            .push(Feature::new(1, Some(Geometry::LineString(visible_line))));
        layer
            .features
            .push(Feature::new(2, Some(Geometry::LineString(offscreen_line))));

        let style = line_style(rgba(10, 20, 30, 200), 2.5);
        let viewport = (800.0, 600.0);
        let camera = Camera::new((0.0, 0.0), 5.0, viewport);

        let geoms = LineLayerRenderer.render(&layer, &style, &camera, viewport);
        assert_eq!(geoms.len(), 1);
        match &geoms[0] {
            CanvasGeometry::Stroke {
                path, color, width, ..
            } => {
                assert_eq!(*color, rgba(10, 20, 30, 200));
                assert_eq!(*width, 2.5);
                assert_eq!(path.len(), 1);
                assert_eq!(path[0].len(), 3);
                // Middle vertex at (0, 0) maps to viewport center (400, 300).
                assert!((path[0][1].x - 400.0).abs() < 1e-6);
                assert!((path[0][1].y - 300.0).abs() < 1e-6);
            }
            other => panic!("expected Stroke for line, got {:?}", other),
        }
    }

    #[test]
    fn polygon_layer_renderer_renders_fill_and_outline() {
        let mut layer = Layer::new("lakes", SrsId::from_epsg(4326));
        let poly = Polygon::with_holes(
            vec![
                Point::new(-2.0, -2.0),
                Point::new(2.0, -2.0),
                Point::new(2.0, 2.0),
                Point::new(-2.0, 2.0),
            ],
            vec![vec![
                Point::new(-0.5, -0.5),
                Point::new(0.5, -0.5),
                Point::new(0.5, 0.5),
                Point::new(-0.5, 0.5),
            ]],
        );
        layer
            .features
            .push(Feature::new(1, Some(Geometry::Polygon(poly))));

        let style = polygon_style(
            rgba(50, 100, 200, 255),
            rgba(10, 10, 10, 255),
            1.5,
            "solid",
        );
        let viewport = (800.0, 600.0);
        let camera = Camera::new((0.0, 0.0), 4.0, viewport);

        let geoms = PolygonLayerRenderer.render(&layer, &style, &camera, viewport);
        // 1 Fill + 1 Stroke (outline_width = 1.5 > 0)
        assert_eq!(geoms.len(), 2);
        match &geoms[0] {
            CanvasGeometry::Fill { rings, color, .. } => {
                assert_eq!(*color, rgba(50, 100, 200, 255));
                assert_eq!(rings.len(), 2, "exterior + 1 interior hole");
                assert_eq!(rings[0].first(), rings[0].last(), "closed exterior ring");
                assert_eq!(rings[1].first(), rings[1].last(), "closed interior ring");
            }
            other => panic!("expected Fill first, got {:?}", other),
        }
        match &geoms[1] {
            CanvasGeometry::Stroke {
                path, color, width, ..
            } => {
                assert_eq!(*color, rgba(10, 10, 10, 255));
                assert_eq!(*width, 1.5);
                assert_eq!(path.len(), 2);
            }
            other => panic!("expected Stroke second, got {:?}", other),
        }
    }

    #[test]
    fn viewport_culling_10k_features_renders_only_visible_subset() {
        let mut layer = Layer::new("grid_10k", SrsId::from_epsg(4326));
        let cols = 100;
        let rows = 100;
        let cell_w = 360.0 / cols as f64;
        let cell_h = 180.0 / rows as f64;

        let mut id = 0;
        for col in 0..cols {
            for row in 0..rows {
                let x0 = -180.0 + col as f64 * cell_w;
                let y0 = -90.0 + row as f64 * cell_h;
                let poly = Polygon::new(vec![
                    Point::new(x0 + 0.1, y0 + 0.1),
                    Point::new(x0 + cell_w - 0.1, y0 + 0.1),
                    Point::new(x0 + cell_w - 0.1, y0 + cell_h - 0.1),
                    Point::new(x0 + 0.1, y0 + cell_h - 0.1),
                ]);
                layer
                    .features
                    .push(Feature::new(id, Some(Geometry::Polygon(poly))));
                id += 1;
            }
        }
        assert_eq!(layer.feature_count(), 10_000);

        let index = build_spatial_index(&layer);
        assert_eq!(index.len(), 10_000);

        // Zoom in to zoom level 5 around (0, 0) on an 800x600 viewport.
        // At zoom 5, scale = 256 * 32 = 8192 px for 360 deg (~22.75 px/deg),
        // so 800x600 covers roughly 35.1 deg x 26.4 deg (< 2% of the globe).
        let viewport = (800.0, 600.0);
        let camera = Camera::new((0.0, 0.0), 5.0, viewport);

        let visible_features = cull_visible_features(&layer, &camera, viewport);
        assert!(
            !visible_features.is_empty(),
            "viewport around origin must contain features"
        );
        assert!(
            visible_features.len() < 300,
            "expected small visible subset (< 300 of 10,000), got {}",
            visible_features.len()
        );

        // Outline width 0.0 so each visible polygon emits exactly 1 Fill primitive.
        let style = polygon_style(
            rgba(100, 150, 200, 255),
            rgba(0, 0, 0, 255),
            0.0,
            "solid",
        );
        let rendered = PolygonLayerRenderer.render_with_index(
            &layer, &index, &style, &camera, viewport,
        );
        assert_eq!(rendered.len(), visible_features.len());
    }

    #[test]
    fn layer_opacity_and_visibility_are_respected() {
        let mut layer = Layer::new("semi", SrsId::from_epsg(4326));
        layer.opacity = 0.5;
        layer
            .features
            .push(Feature::new(1, Some(Geometry::Point(Point::new(0.0, 0.0)))));

        let style = point_style("square", 8.0, rgba(200, 100, 50, 200));
        let viewport = (400.0, 400.0);
        let camera = Camera::new((0.0, 0.0), 3.0, viewport);

        let rendered = render_layer(&layer, &style, &camera, viewport);
        assert_eq!(rendered.len(), 1);
        match &rendered[0] {
            CanvasGeometry::Fill { color, .. } => {
                // 200 * 0.5 = 100
                assert_eq!(color.a, 100);
            }
            other => panic!("expected Fill, got {:?}", other),
        }

        // Hidden layer renders nothing.
        layer.visible = false;
        assert!(render_layer(&layer, &style, &camera, viewport).is_empty());
    }

    #[test]
    fn categorized_and_graduated_styles_render_matching_features() {
        let styles_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../styles");
        let polys_style = qgis_style::parse_qml_file(styles_dir.join("polys.qml")).unwrap();

        let mut layer = Layer::new("water", SrsId::from_epsg(4326));
        let mut f_dam = Feature::new(
            1,
            Some(Geometry::Polygon(Polygon::new(vec![
                Point::new(0.0, 0.0),
                Point::new(1.0, 0.0),
                Point::new(1.0, 1.0),
                Point::new(0.0, 1.0),
            ]))),
        );
        f_dam.set_attribute("Name", Value::Text("Dam".into()));

        let mut f_lake = Feature::new(
            2,
            Some(Geometry::Polygon(Polygon::new(vec![
                Point::new(2.0, 0.0),
                Point::new(3.0, 0.0),
                Point::new(3.0, 1.0),
                Point::new(2.0, 1.0),
            ]))),
        );
        f_lake.set_attribute("Name", Value::Text("Lake".into()));

        let mut f_other = Feature::new(
            3,
            Some(Geometry::Polygon(Polygon::new(vec![
                Point::new(4.0, 0.0),
                Point::new(5.0, 0.0),
                Point::new(5.0, 1.0),
                Point::new(4.0, 1.0),
            ]))),
        );
        f_other.set_attribute("Name", Value::Text("Unknown".into()));

        layer.features = vec![f_dam, f_lake, f_other];

        let viewport = (800.0, 600.0);
        let camera = camera_for_extent(layer.extent().unwrap(), viewport);
        let geoms = PolygonLayerRenderer.render(&layer, &polys_style, &camera, viewport);
        // 2 matched polygons ("Dam" and "Lake"), each producing 1 Fill + 1 Stroke = 4 primitives.
        assert_eq!(geoms.len(), 4);
    }

    #[test]
    fn multi_geometries_and_collections_render_all_parts() {
        let mut layer = Layer::new("multi", SrsId::from_epsg(4326));
        let mp = MultiPoint::new(vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)]);
        let gc = GeometryCollection::new(vec![Geometry::Point(Point::new(2.0, 2.0))]);
        layer
            .features
            .push(Feature::new(1, Some(Geometry::MultiPoint(mp))));
        layer
            .features
            .push(Feature::new(2, Some(Geometry::GeometryCollection(gc))));

        let style = point_style("diamond", 4.0, rgba(10, 200, 10, 255));
        let viewport = (800.0, 600.0);
        let camera = Camera::new((0.0, 0.0), 3.0, viewport);

        let pt_geoms = PointLayerRenderer.render(&layer, &style, &camera, viewport);
        assert_eq!(pt_geoms.len(), 3, "2 from MultiPoint + 1 from GeometryCollection");

        // MultiLineString and MultiPolygon via VectorLayerRenderer:
        let mut line_layer = Layer::new("mls", SrsId::from_epsg(4326));
        let mls = MultiLineString::new(vec![
            LineString::new(vec![Point::new(0.0, 0.0), Point::new(1.0, 0.0)]),
            LineString::new(vec![Point::new(0.0, 1.0), Point::new(1.0, 1.0)]),
        ]);
        line_layer
            .features
            .push(Feature::new(1, Some(Geometry::MultiLineString(mls))));
        let l_style = line_style(rgba(0, 0, 255, 255), 1.0);
        assert_eq!(
            VectorLayerRenderer
                .render(&line_layer, &l_style, &camera, viewport)
                .len(),
            2
        );

        let mut poly_layer = Layer::new("mpoly", SrsId::from_epsg(4326));
        let mpoly = MultiPolygon::new(vec![
            Polygon::new(vec![
                Point::new(0.0, 0.0),
                Point::new(1.0, 0.0),
                Point::new(1.0, 1.0),
            ]),
            Polygon::new(vec![
                Point::new(2.0, 0.0),
                Point::new(3.0, 0.0),
                Point::new(3.0, 1.0),
            ]),
        ]);
        poly_layer
            .features
            .push(Feature::new(1, Some(Geometry::MultiPolygon(mpoly))));
        let p_style = polygon_style(rgba(100, 100, 100, 255), rgba(0, 0, 0, 255), 0.0, "solid");
        assert_eq!(
            VectorLayerRenderer
                .render(&poly_layer, &p_style, &camera, viewport)
                .len(),
            2
        );
    }
}
