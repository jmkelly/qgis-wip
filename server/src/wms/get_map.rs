//! WMS GetMap handler — renders map layers to a raster image.
//!
//! Reference: `src/server/services/qgswmsgetmap.cpp`

use bytes::Bytes;
use qgis_geometry::{Geometry, LineString, Point, Polygon};
use qgis_render::{Canvas, Image, Pixel, Transform};
use std::collections::HashMap;
use tracing::debug;

use crate::error::Result;
use crate::fcgi::FcgiResponse;
use crate::query::QueryParameters;
use crate::service::service_exception;

/// Default image format for GetMap responses.
pub const DEFAULT_FORMAT: &str = "image/png";

/// Maximum allowed image dimension (pixels).
pub const MAX_DIMENSION: u32 = 4096;

/// Minimum allowed image dimension (pixels).
pub const MIN_DIMENSION: u32 = 1;

/// Supported image formats.
pub const SUPPORTED_FORMATS: &[&str] = &["image/png", "image/jpeg", "image/gif"];

/// Default exceptions format.
pub const DEFAULT_EXCEPTIONS: &str = "application/vnd.ogc.se_xml";

/// Parsed and validated WMS GetMap parameters.
#[derive(Debug, Clone)]
pub struct GetMapParams {
    /// Comma-separated list of layer names to render.
    pub layers: Vec<String>,
    /// Comma-separated list of style names.
    pub styles: Vec<String>,
    /// CRS identifier (e.g., "EPSG:4326").
    pub crs: String,
    /// Bounding box: (min_x, min_y, max_x, max_y).
    pub bbox: (f64, f64, f64, f64),
    /// Output image width in pixels.
    pub width: u32,
    /// Output image height in pixels.
    pub height: u32,
    /// Output image MIME type.
    pub format: String,
    /// Whether the background should be transparent.
    pub transparent: bool,
    /// Background color (used when transparent is false).
    pub bg_color: Pixel,
    /// Exceptions format.
    pub exceptions: String,
}

/// A map layer with renderable features.
#[derive(Debug, Clone)]
pub struct WmsLayer {
    /// Layer name.
    pub name: String,
    /// Features to render.
    pub features: Vec<WmsFeature>,
    /// Layer rendering style.
    pub style: LayerStyle,
}

/// A single feature within a layer.
#[derive(Debug, Clone)]
pub struct WmsFeature {
    /// Feature geometry.
    pub geometry: Geometry,
    /// Feature attributes.
    pub attributes: HashMap<String, String>,
}

/// Layer rendering style.
#[derive(Debug, Clone)]
pub struct LayerStyle {
    /// Fill color for polygons (None = no fill).
    pub fill_color: Option<Pixel>,
    /// Stroke color for lines and polygon outlines.
    pub stroke_color: Option<Pixel>,
    /// Stroke width in pixels.
    pub stroke_width: u32,
    /// Point radius in pixels.
    pub point_radius: u32,
}

impl Default for LayerStyle {
    fn default() -> Self {
        Self {
            fill_color: None,
            stroke_color: Some(Pixel::rgb(0, 0, 0)),
            stroke_width: 1,
            point_radius: 4,
        }
    }
}

impl GetMapParams {
    /// Parses GetMap parameters from query string parameters.
    ///
    /// Returns a ServiceException response on invalid parameters.
    pub fn from_query(query: &QueryParameters) -> std::result::Result<Self, FcgiResponse> {
        let map_path = query.map().unwrap_or("");
        if map_path.is_empty() {
            return Err(service_exception(
                "MissingParameterValue",
                "MAP parameter is required",
            ));
        }

        let layers_str = query.get("LAYERS").unwrap_or("");
        if layers_str.is_empty() {
            return Err(service_exception(
                "MissingParameterValue",
                "LAYERS parameter is required",
            ));
        }
        let layers: Vec<String> = layers_str.split(',').map(|s| s.to_string()).collect();

        let styles_str = query.get("STYLES").unwrap_or("");
        let styles: Vec<String> = if styles_str.is_empty() {
            layers.iter().map(|_| "default".to_string()).collect()
        } else {
            styles_str.split(',').map(|s| s.to_string()).collect()
        };

        let crs = query.crs().unwrap_or("EPSG:4326").to_string();

        let bbox_str = query.get("BBOX").unwrap_or("");
        let bbox = parse_bbox(bbox_str)?;

        let width = parse_dimension(query.get("WIDTH"), "WIDTH")?;
        let height = parse_dimension(query.get("HEIGHT"), "HEIGHT")?;

        let format = query.get("FORMAT").unwrap_or(DEFAULT_FORMAT).to_string();
        if !SUPPORTED_FORMATS.contains(&format.as_str()) {
            return Err(service_exception(
                "InvalidParameterValue",
                format!(
                    "Unsupported FORMAT '{}'. Supported: {}",
                    format,
                    SUPPORTED_FORMATS.join(", ")
                ),
            ));
        }

        let transparent = query
            .get("TRANSPARENT")
            .map(|v| v.eq_ignore_ascii_case("TRUE"))
            .unwrap_or(false);

        let bg_color = parse_bg_color(query.get("BGCOLOR"));

        let exceptions = query
            .get("EXCEPTIONS")
            .unwrap_or(DEFAULT_EXCEPTIONS)
            .to_string();

        Ok(Self {
            layers,
            styles,
            crs,
            bbox,
            width,
            height,
            format,
            transparent,
            bg_color,
            exceptions,
        })
    }
}

/// Parses a bounding box string "minx,miny,maxx,maxy" into a tuple.
fn parse_bbox(s: &str) -> std::result::Result<(f64, f64, f64, f64), FcgiResponse> {
    if s.is_empty() {
        return Err(service_exception(
            "MissingParameterValue",
            "BBOX parameter is required",
        ));
    }

    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 4 {
        return Err(service_exception(
            "InvalidParameterValue",
            format!("BBOX must have 4 values, got {}", parts.len()),
        ));
    }

    let values: std::result::Result<Vec<f64>, _> = parts.iter().map(|p| p.parse::<f64>()).collect();
    match values {
        Ok(v) => Ok((v[0], v[1], v[2], v[3])),
        Err(_) => Err(service_exception(
            "InvalidParameterValue",
            format!("BBOX contains invalid number: {}", s),
        )),
    }
}

/// Parses an image dimension (WIDTH or HEIGHT).
fn parse_dimension(value: Option<&str>, name: &str) -> std::result::Result<u32, FcgiResponse> {
    let val = value.ok_or_else(|| {
        service_exception(
            "MissingParameterValue",
            format!("{} parameter is required", name),
        )
    })?;

    let num: u32 = val.parse().map_err(|_| {
        service_exception(
            "InvalidParameterValue",
            format!("{} must be a positive integer", name),
        )
    })?;

    if !(MIN_DIMENSION..=MAX_DIMENSION).contains(&num) {
        return Err(service_exception(
            "InvalidParameterValue",
            format!(
                "{} must be between {} and {}",
                name, MIN_DIMENSION, MAX_DIMENSION
            ),
        ));
    }

    Ok(num)
}

/// Parses a background color string "RRGGBB" or "AARRGGBB".
fn parse_bg_color(s: Option<&str>) -> Pixel {
    let s = s.unwrap_or("FFFFFF");
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(255);
        Pixel::rgb(r, g, b)
    } else if s.len() == 8 {
        let a = u8::from_str_radix(&s[0..2], 16).unwrap_or(255);
        let r = u8::from_str_radix(&s[2..4], 16).unwrap_or(255);
        let g = u8::from_str_radix(&s[4..6], 16).unwrap_or(255);
        let b = u8::from_str_radix(&s[6..8], 16).unwrap_or(255);
        Pixel::new(r, g, b, a)
    } else {
        Pixel::rgb(255, 255, 255)
    }
}

/// Renders the map layers to a PNG image.
///
/// Takes the parsed parameters and a set of layers, renders each layer's
/// features onto the image, and encodes the result as PNG bytes.
pub fn render_map(
    params: &GetMapParams,
    layers: &[WmsLayer],
) -> std::result::Result<Vec<u8>, FcgiResponse> {
    debug!(
        width = params.width,
        height = params.height,
        layers = layers.len(),
        "rendering WMS GetMap"
    );

    // Create the output image
    let background = if params.transparent {
        Pixel::default() // Transparent
    } else {
        params.bg_color
    };

    let mut image = Image::new(params.width, params.height, background)
        .map_err(|e| service_exception("InternalError", e.to_string()))?;

    // Create the coordinate transform from map space to pixel space
    let transform = Transform::map_to_pixel(
        params.bbox.0,
        params.bbox.1,
        params.bbox.2,
        params.bbox.3,
        params.width,
        params.height,
    );

    // Render each layer
    let mut canvas = Canvas::new(&mut image);
    for layer in layers {
        render_layer(&mut canvas, layer, &transform);
    }

    // Encode as PNG
    qgis_render::encode_png(&image)
        .map_err(|e| service_exception("InternalError", e.to_string()))
}

/// Renders a single layer onto the canvas.
fn render_layer(canvas: &mut Canvas, layer: &WmsLayer, transform: &Transform) {
    for feature in &layer.features {
        render_feature(canvas, feature, &layer.style, transform);
    }
}

/// Renders a single feature according to its geometry type.
fn render_feature(canvas: &mut Canvas, feature: &WmsFeature, style: &LayerStyle, transform: &Transform) {
    match &feature.geometry {
        Geometry::Point(p) => render_point(canvas, p, style, transform),
        Geometry::LineString(ls) => render_linestring(canvas, ls, style, transform),
        Geometry::Polygon(poly) => render_polygon(canvas, poly, style, transform),
        Geometry::MultiPoint(mp) => {
            for p in mp.points() {
                render_point(canvas, p, style, transform);
            }
        }
        Geometry::MultiLineString(mls) => {
            for ls in mls.lines() {
                render_linestring(canvas, ls, style, transform);
            }
        }
        Geometry::MultiPolygon(mpoly) => {
            for poly in mpoly.polygons() {
                render_polygon(canvas, poly, style, transform);
            }
        }
        Geometry::GeometryCollection(gc) => {
            for geom in gc.geometries() {
                render_feature(
                    canvas,
                    &WmsFeature {
                        geometry: geom.clone(),
                        attributes: feature.attributes.clone(),
                    },
                    style,
                    transform,
                );
            }
        }
    }
}

/// Renders a point feature as a circle.
fn render_point(canvas: &mut Canvas, point: &Point, style: &LayerStyle, transform: &Transform) {
    let (px, py) = transform.transform_point(point.x, point.y);
    let cx = px.round() as i32;
    let cy = py.round() as i32;
    let radius = style.point_radius as i32;

    if let Some(fill) = style.fill_color {
        canvas.fill_circle(cx, cy, radius, fill);
    }
    if let Some(stroke) = style.stroke_color {
        canvas.draw_circle(cx, cy, radius, stroke);
    }
}

/// Renders a linestring feature.
fn render_linestring(
    canvas: &mut Canvas,
    line: &LineString,
    style: &LayerStyle,
    transform: &Transform,
) {
    if line.is_empty() {
        return;
    }

    let points: Vec<(i32, i32)> = line
        .points()
        .iter()
        .map(|p| {
            let (px, py) = transform.transform_point(p.x, p.y);
            (px.round() as i32, py.round() as i32)
        })
        .collect();

    if points.len() < 2 {
        return;
    }

    if let Some(stroke) = style.stroke_color {
        canvas.draw_polyline(&points, stroke);
    }
}

/// Renders a polygon feature.
fn render_polygon(
    canvas: &mut Canvas,
    poly: &Polygon,
    style: &LayerStyle,
    transform: &Transform,
) {
    let exterior = poly.exterior();
    if exterior.len() < 3 {
        return;
    }

    // Transform exterior ring
    let exterior_pts: Vec<(i32, i32)> = exterior
        .iter()
        .map(|p| {
            let (px, py) = transform.transform_point(p.x, p.y);
            (px.round() as i32, py.round() as i32)
        })
        .collect();

    // Fill
    if let Some(fill) = style.fill_color {
        canvas.fill_polygon(&exterior_pts, fill);
    }

    // Stroke outline
    if let Some(stroke) = style.stroke_color {
        let mut outline = exterior_pts.clone();
        if let Some(first) = exterior_pts.first() {
            outline.push(*first); // Close the ring
        }
        canvas.draw_polyline(&outline, stroke);
    }
}

/// WMS GetMap handler — parses parameters, renders the map, and returns the image.
pub fn handle_get_map(query: &QueryParameters) -> Result<FcgiResponse> {
    let params = match GetMapParams::from_query(query) {
        Ok(p) => p,
        Err(resp) => return Ok(resp),
    };

    debug!(
        layers = ?params.layers,
        bbox = ?params.bbox,
        size = format!("{}x{}", params.width, params.height),
        "WMS GetMap request"
    );

    // Load layers from the project (placeholder: empty layer set)
    // TODO: Load actual layers from the project file when project loading is implemented
    let layers: Vec<WmsLayer> = load_layers_from_project(query.map().unwrap_or(""), &params.layers);

    match render_map(&params, &layers) {
        Ok(png_bytes) => {
            let response = FcgiResponse::new(Bytes::from(png_bytes));
            Ok(response)
        }
        Err(resp) => Ok(resp),
    }
}

/// Loads layers from a project file.
///
/// Parses the .qgs project file, resolves data sources relative to the
/// project directory, and loads features from supported formats (GeoJSON).
/// Returns only the layers matching `layer_names`, in the order specified.
pub fn load_layers_from_project(
    project_path: &str,
    layer_names: &[String],
) -> Vec<WmsLayer> {
    let project = match crate::wms::project::load_project(project_path) {
        Ok(p) => p,
        Err(e) => {
            debug!("failed to load project {}: {}", project_path, e);
            return Vec::new();
        }
    };

    let project_dir = std::path::Path::new(project_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();

    let mut layers = Vec::new();

    for layer_name in layer_names {
        let Some(project_layer) = project
            .layers
            .iter()
            .find(|l| &l.name == layer_name)
        else {
            debug!("layer '{}' not found in project", layer_name);
            continue;
        };

        if !project_layer.visible {
            debug!("layer '{}' is hidden in project", layer_name);
            continue;
        }

        let datasource = &project_layer.datasource;
        let features = if datasource.ends_with(".geojson") || datasource.ends_with(".json") {
            let path = project_dir.join(datasource);
            match std::fs::read_to_string(&path) {
                Ok(text) => match crate::wms::geojson::parse_geojson(&text) {
                    Ok(fc) => fc.features,
                    Err(e) => {
                        debug!("failed to parse GeoJSON {}: {}", path.display(), e);
                        continue;
                    }
                },
                Err(e) => {
                    debug!("failed to read GeoJSON {}: {}", path.display(), e);
                    continue;
                }
            }
        } else {
            debug!(
                "unsupported data source format: {} for layer '{}'",
                datasource, layer_name
            );
            continue;
        };

        let style = project
            .styles
            .get(layer_name)
            .cloned()
            .unwrap_or_default();

        layers.push(WmsLayer {
            name: layer_name.clone(),
            features,
            style,
        });
    }

    layers
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_query(params: &[(&str, &str)]) -> QueryParameters {
        let query_string = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");
        QueryParameters::parse(&query_string).unwrap()
    }

    #[test]
    fn parse_get_map_params_basic() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1,layer2"),
            ("CRS", "EPSG:4326"),
            ("BBOX", "-180,-90,180,90"),
            ("WIDTH", "800"),
            ("HEIGHT", "600"),
        ]);

        let params = GetMapParams::from_query(&query).unwrap();
        assert_eq!(params.layers, vec!["layer1", "layer2"]);
        assert_eq!(params.crs, "EPSG:4326");
        assert_eq!(params.bbox, (-180.0, -90.0, 180.0, 90.0));
        assert_eq!(params.width, 800);
        assert_eq!(params.height, 600);
        assert_eq!(params.format, DEFAULT_FORMAT);
        assert!(!params.transparent);
    }

    #[test]
    fn parse_get_map_params_transparent() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1"),
            ("CRS", "EPSG:4326"),
            ("BBOX", "0,0,100,100"),
            ("WIDTH", "256"),
            ("HEIGHT", "256"),
            ("TRANSPARENT", "TRUE"),
        ]);

        let params = GetMapParams::from_query(&query).unwrap();
        assert!(params.transparent);
    }

    #[test]
    fn parse_get_map_params_missing_map() {
        let query = make_query(&[
            ("LAYERS", "layer1"),
            ("BBOX", "0,0,100,100"),
            ("WIDTH", "256"),
            ("HEIGHT", "256"),
        ]);

        let result = GetMapParams::from_query(&query);
        assert!(result.is_err());
    }

    #[test]
    fn parse_get_map_params_missing_layers() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("BBOX", "0,0,100,100"),
            ("WIDTH", "256"),
            ("HEIGHT", "256"),
        ]);

        let result = GetMapParams::from_query(&query);
        assert!(result.is_err());
    }

    #[test]
    fn parse_get_map_params_invalid_bbox() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1"),
            ("BBOX", "invalid"),
            ("WIDTH", "256"),
            ("HEIGHT", "256"),
        ]);

        let result = GetMapParams::from_query(&query);
        assert!(result.is_err());
    }

    #[test]
    fn parse_get_map_params_bbox_wrong_count() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1"),
            ("BBOX", "0,0,100"),
            ("WIDTH", "256"),
            ("HEIGHT", "256"),
        ]);

        let result = GetMapParams::from_query(&query);
        assert!(result.is_err());
    }

    #[test]
    fn parse_get_map_params_invalid_width() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1"),
            ("BBOX", "0,0,100,100"),
            ("WIDTH", "abc"),
            ("HEIGHT", "256"),
        ]);

        let result = GetMapParams::from_query(&query);
        assert!(result.is_err());
    }

    #[test]
    fn parse_get_map_params_width_too_large() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1"),
            ("BBOX", "0,0,100,100"),
            ("WIDTH", "10000"),
            ("HEIGHT", "256"),
        ]);

        let result = GetMapParams::from_query(&query);
        assert!(result.is_err());
    }

    #[test]
    fn parse_get_map_params_zero_dimension() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1"),
            ("BBOX", "0,0,100,100"),
            ("WIDTH", "0"),
            ("HEIGHT", "256"),
        ]);

        let result = GetMapParams::from_query(&query);
        assert!(result.is_err());
    }

    #[test]
    fn parse_bg_color_rgb() {
        let color = parse_bg_color(Some("FF0000"));
        assert_eq!(color, Pixel::rgb(255, 0, 0));
    }

    #[test]
    fn parse_bg_color_rgba() {
        let color = parse_bg_color(Some("80FF0000"));
        assert_eq!(color, Pixel::new(255, 0, 0, 128));
    }

    #[test]
    fn parse_bg_color_default() {
        let color = parse_bg_color(None);
        assert_eq!(color, Pixel::rgb(255, 255, 255));
    }

    #[test]
    fn render_empty_map() {
        let params = GetMapParams {
            layers: vec![],
            styles: vec![],
            crs: "EPSG:4326".to_string(),
            bbox: (-180.0, -90.0, 180.0, 90.0),
            width: 100,
            height: 100,
            format: DEFAULT_FORMAT.to_string(),
            transparent: false,
            bg_color: Pixel::rgb(255, 255, 255),
            exceptions: DEFAULT_EXCEPTIONS.to_string(),
        };

        let result = render_map(&params, &[]);
        assert!(result.is_ok());
        let png = result.unwrap();
        assert!(!png.is_empty());
        // PNG magic bytes
        assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn render_map_with_point_layer() {
        let params = GetMapParams {
            layers: vec!["points".to_string()],
            styles: vec![],
            crs: "EPSG:4326".to_string(),
            bbox: (-180.0, -90.0, 180.0, 90.0),
            width: 200,
            height: 200,
            format: DEFAULT_FORMAT.to_string(),
            transparent: false,
            bg_color: Pixel::rgb(255, 255, 255),
            exceptions: DEFAULT_EXCEPTIONS.to_string(),
        };

        let layer = WmsLayer {
            name: "points".to_string(),
            features: vec![WmsFeature {
                geometry: Geometry::Point(Point::new(0.0, 0.0)),
                attributes: HashMap::new(),
            }],
            style: LayerStyle {
                fill_color: Some(Pixel::rgb(255, 0, 0)),
                stroke_color: Some(Pixel::rgb(0, 0, 0)),
                stroke_width: 1,
                point_radius: 5,
            },
        };

        let result = render_map(&params, &[layer]);
        assert!(result.is_ok());
        let png = result.unwrap();
        assert!(!png.is_empty());
    }

    #[test]
    fn render_map_with_polygon_layer() {
        let params = GetMapParams {
            layers: vec!["polygons".to_string()],
            styles: vec![],
            crs: "EPSG:4326".to_string(),
            bbox: (0.0, 0.0, 100.0, 100.0),
            width: 200,
            height: 200,
            format: DEFAULT_FORMAT.to_string(),
            transparent: false,
            bg_color: Pixel::rgb(255, 255, 255),
            exceptions: DEFAULT_EXCEPTIONS.to_string(),
        };

        let ring = LineString::new(vec![
            Point::new(10.0, 10.0),
            Point::new(50.0, 10.0),
            Point::new(50.0, 50.0),
            Point::new(10.0, 50.0),
        ]);

        let layer = WmsLayer {
            name: "polygons".to_string(),
            features: vec![WmsFeature {
                geometry: Geometry::Polygon(Polygon::new(ring.points().to_vec())),
                attributes: HashMap::new(),
            }],
            style: LayerStyle {
                fill_color: Some(Pixel::rgb(0, 128, 255)),
                stroke_color: Some(Pixel::rgb(0, 0, 0)),
                stroke_width: 2,
                point_radius: 4,
            },
        };

        let result = render_map(&params, &[layer]);
        assert!(result.is_ok());
        let png = result.unwrap();
        assert!(!png.is_empty());
    }

    #[test]
    fn handle_get_map_returns_png() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1"),
            ("CRS", "EPSG:4326"),
            ("BBOX", "-180,-90,180,90"),
            ("WIDTH", "256"),
            ("HEIGHT", "256"),
        ]);

        let response = handle_get_map(&query).unwrap();
        assert!(!response.body.is_empty());
        // Check PNG magic bytes
        let body = &response.body;
        assert_eq!(&body[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn handle_get_map_missing_map() {
        let query = make_query(&[
            ("LAYERS", "layer1"),
            ("BBOX", "-180,-90,180,90"),
            ("WIDTH", "256"),
            ("HEIGHT", "256"),
        ]);

        let response = handle_get_map(&query).unwrap();
        let body = String::from_utf8_lossy(&response.body);
        assert!(body.contains("MissingParameterValue"));
        assert!(body.contains("MAP"));
    }

    #[test]
    fn handle_get_map_missing_layers() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("BBOX", "-180,-90,180,90"),
            ("WIDTH", "256"),
            ("HEIGHT", "256"),
        ]);

        let response = handle_get_map(&query).unwrap();
        let body = String::from_utf8_lossy(&response.body);
        assert!(body.contains("MissingParameterValue"));
        assert!(body.contains("LAYERS"));
    }

    #[test]
    fn handle_get_map_invalid_width() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1"),
            ("BBOX", "-180,-90,180,90"),
            ("WIDTH", "abc"),
            ("HEIGHT", "256"),
        ]);

        let response = handle_get_map(&query).unwrap();
        let body = String::from_utf8_lossy(&response.body);
        assert!(body.contains("InvalidParameterValue"));
    }
}
