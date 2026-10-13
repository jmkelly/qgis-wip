//! WMS GetFeatureInfo handler — identifies features at a point.
//!
//! Reference: `src/server/services/qgswmsgetfeatureinfo.cpp`

use bytes::Bytes;
use qgis_geometry::{Geometry, LineString, Point};
use qgis_render::Transform;
use std::collections::HashMap;
use tracing::debug;

use crate::error::Result;
use crate::fcgi::FcgiResponse;
use crate::query::QueryParameters;
use crate::service::service_exception;

use super::get_map::{load_layers_from_project, GetMapParams, WmsLayer};

/// Default info format for GetFeatureInfo responses.
pub const DEFAULT_INFO_FORMAT: &str = "text/html";

/// Maximum number of features to return per layer.
pub const MAX_FEATURES_PER_LAYER: usize = 100;

/// Default number of features to return.
pub const DEFAULT_FEATURE_COUNT: usize = 10;

/// Distance tolerance for line features (in map units).
pub const LINE_TOLERANCE: f64 = 1e-6;

/// Supported info formats.
pub const SUPPORTED_INFO_FORMATS: &[&str] = &[
    "text/html",
    "text/plain",
    "text/xml",
    "application/json",
    "application/geo+json",
    "application/vnd.ogc.gml",
];

/// Parsed and validated WMS GetFeatureInfo parameters.
#[derive(Debug, Clone)]
pub struct GetFeatureInfoParams {
    /// Comma-separated list of layer names to query.
    pub query_layers: Vec<String>,
    /// Pixel X coordinate (I parameter).
    pub i: u32,
    /// Pixel Y coordinate (J parameter).
    pub j: u32,
    /// Output info format MIME type.
    pub info_format: String,
    /// Maximum number of features to return.
    pub feature_count: usize,
    /// Underlying map parameters (bbox, crs, size, etc.).
    pub map_params: GetMapParams,
}

impl GetFeatureInfoParams {
    /// Parses GetFeatureInfo parameters from query string parameters.
    pub fn from_query(query: &QueryParameters) -> std::result::Result<Self, FcgiResponse> {
        // Parse the underlying GetMap parameters first. GetFeatureInfo
        // requires a fully specified map (LAYERS, CRS, BBOX, WIDTH, HEIGHT);
        // when those are missing the identify path is reported as not
        // implemented rather than as a parameter error.
        let map_params = match GetMapParams::from_query(query) {
            Ok(p) => p,
            Err(_) => {
                return Err(service_exception(
                    "NotImplemented",
                    "GetFeatureInfo requires LAYERS, CRS, BBOX, WIDTH, and HEIGHT parameters",
                ))
            }
        };

        let query_layers_str = query.get("QUERY_LAYERS").unwrap_or("");
        if query_layers_str.is_empty() {
            return Err(service_exception(
                "MissingParameterValue",
                "QUERY_LAYERS parameter is required",
            ));
        }
        let query_layers: Vec<String> = query_layers_str
            .split(',')
            .map(|s| s.to_string())
            .collect();

        let i_str = query.get("I").unwrap_or("");
        let i: u32 = i_str.parse().map_err(|_| {
            service_exception("InvalidParameterValue", "I must be a non-negative integer")
        })?;
        if i >= map_params.width {
            return Err(service_exception(
                "InvalidParameterValue",
                format!("I ({}) is outside image width ({})", i, map_params.width),
            ));
        }

        let j_str = query.get("J").unwrap_or("");
        let j: u32 = j_str.parse().map_err(|_| {
            service_exception("InvalidParameterValue", "J must be a non-negative integer")
        })?;
        if j >= map_params.height {
            return Err(service_exception(
                "InvalidParameterValue",
                format!("J ({}) is outside image height ({})", j, map_params.height),
            ));
        }

        let info_format = query
            .get("INFO_FORMAT")
            .unwrap_or(DEFAULT_INFO_FORMAT)
            .to_string();
        if !SUPPORTED_INFO_FORMATS.contains(&info_format.as_str()) {
            return Err(service_exception(
                "InvalidParameterValue",
                format!(
                    "Unsupported INFO_FORMAT '{}'. Supported: {}",
                    info_format,
                    SUPPORTED_INFO_FORMATS.join(", ")
                ),
            ));
        }

        let feature_count = query
            .get("FEATURE_COUNT")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(DEFAULT_FEATURE_COUNT);

        Ok(Self {
            query_layers,
            i,
            j,
            info_format,
            feature_count,
            map_params,
        })
    }
}

/// A feature identification result.
#[derive(Debug, Clone)]
pub struct FeatureInfo {
    /// Layer name.
    pub layer: String,
    /// Feature ID (if available).
    pub feature_id: Option<String>,
    /// Feature attributes.
    pub attributes: HashMap<String, String>,
}

/// Identifies features at the given pixel coordinates.
///
/// Transforms the pixel coordinate to map space, then tests each feature
/// in each query layer for intersection with that point.
pub fn identify_features(params: &GetFeatureInfoParams, layers: &[WmsLayer]) -> Vec<FeatureInfo> {
    let transform = Transform::map_to_pixel(
        params.map_params.bbox.0,
        params.map_params.bbox.1,
        params.map_params.bbox.2,
        params.map_params.bbox.3,
        params.map_params.width,
        params.map_params.height,
    );

    // Transform pixel coordinate to map coordinate
    let map_point = transform.inverse().map(|inv| {
        let (mx, my) = inv.transform_point(params.i as f64, params.j as f64);
        Point::new(mx, my)
    });

    let Some(map_point) = map_point else {
        debug!("transform is singular, cannot identify features");
        return Vec::new();
    };

    debug!(
        i = params.i,
        j = params.j,
        map_x = map_point.x,
        map_y = map_point.y,
        layers = layers.len(),
        "identifying features"
    );

    let mut results = Vec::new();

    for layer in layers {
        if !params.query_layers.contains(&layer.name) {
            continue;
        }

        for feature in &layer.features {
            if results.len() >= params.feature_count {
                break;
            }

            if feature_intersects(&map_point, &feature.geometry) {
                let mut attributes = feature.attributes.clone();
                attributes.insert(
                    "__geometry__".to_string(),
                    geometry_to_wkt(&feature.geometry),
                );

                results.push(FeatureInfo {
                    layer: layer.name.clone(),
                    feature_id: attributes.remove("__id__"),
                    attributes,
                });
            }
        }

        if results.len() >= params.feature_count {
            break;
        }
    }

    results
}

/// Tests if a point intersects with a geometry.
fn feature_intersects(point: &Point, geometry: &Geometry) -> bool {
    match geometry {
        Geometry::Point(p) => points_equal(point, p),
        Geometry::LineString(ls) => point_near_linestring(point, ls, LINE_TOLERANCE),
        Geometry::Polygon(poly) => poly.contains_point(point),
        Geometry::MultiPoint(mp) => mp.points().iter().any(|p| points_equal(point, p)),
        Geometry::MultiLineString(mls) => mls
            .lines()
            .iter()
            .any(|ls| point_near_linestring(point, ls, LINE_TOLERANCE)),
        Geometry::MultiPolygon(mpoly) => mpoly
            .polygons()
            .iter()
            .any(|poly| poly.contains_point(point)),
        Geometry::GeometryCollection(gc) => gc
            .geometries()
            .iter()
            .any(|g| feature_intersects(point, g)),
    }
}

/// Tests if two points are equal (within tolerance).
fn points_equal(a: &Point, b: &Point) -> bool {
    (a.x - b.x).abs() < LINE_TOLERANCE && (a.y - b.y).abs() < LINE_TOLERANCE
}

/// Tests if a point is near a linestring (within tolerance distance).
fn point_near_linestring(point: &Point, line: &LineString, tolerance: f64) -> bool {
    let points: &[Point] = line.points();

    for i in 0..points.len().saturating_sub(1) {
        if point_to_segment_distance(point, &points[i], &points[i + 1]) <= tolerance {
            return true;
        }
    }
    false
}

/// Computes the distance from a point to a line segment.
fn point_to_segment_distance(p: &Point, a: &Point, b: &Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length_sq = dx * dx + dy * dy;

    if length_sq == 0.0 {
        return p.distance(a);
    }

    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / length_sq;
    let t = t.clamp(0.0, 1.0);

    let closest = Point::new(a.x + t * dx, a.y + t * dy);
    p.distance(&closest)
}

/// Converts a geometry to a simple WKT string.
fn geometry_to_wkt(geometry: &Geometry) -> String {
    match geometry {
        Geometry::Point(p) => format!("POINT ({} {})", p.x, p.y),
        Geometry::LineString(ls) => {
            let points: Vec<String> = ls
                .points()
                .iter()
                .map(|p| format!("{} {}", p.x, p.y))
                .collect();
            format!("LINESTRING ({})", points.join(", "))
        }
        Geometry::Polygon(poly) => {
            let exterior = poly.exterior();
            let points: Vec<String> = exterior
                .iter()
                .map(|p| format!("{} {}", p.x, p.y))
                .collect();
            format!("POLYGON (({}))", points.join(", "))
        }
        _ => "GEOMETRYCOLLECTION EMPTY".to_string(),
    }
}

/// Formats feature info results as an HTML response.
fn format_html(results: &[FeatureInfo], _params: &GetFeatureInfoParams) -> String {
    let mut html = String::new();
    html.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    html.push_str("<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Strict//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd\">\n");
    html.push_str("<html xmlns=\"http://www.w3.org/1999/xhtml\">\n");
    html.push_str("<head><title>WMS Feature Info</title></head>\n");
    html.push_str("<body>\n");

    if results.is_empty() {
        html.push_str("<p>No features found at the requested location.</p>\n");
    } else {
        for feature in results {
            html.push_str(&format!("<h3>{}</h3>\n", feature.layer));
            html.push_str("<table border=\"1\">\n");
            for (key, value) in &feature.attributes {
                if key.starts_with("__") {
                    continue;
                }
                html.push_str(&format!(
                    "<tr><td><b>{}</b></td><td>{}</td></tr>\n",
                    html_escape(key),
                    html_escape(value)
                ));
            }
            html.push_str("</table>\n");
        }
    }

    html.push_str("</body>\n</html>");
    html
}

/// Formats feature info results as a plain text response.
fn format_text(results: &[FeatureInfo], _params: &GetFeatureInfoParams) -> String {
    let mut text = String::new();

    if results.is_empty() {
        text.push_str("No features found at the requested location.\n");
    } else {
        for feature in results {
            text.push_str(&format!("Layer: {}\n", feature.layer));
            for (key, value) in &feature.attributes {
                if key.starts_with("__") {
                    continue;
                }
                text.push_str(&format!("  {}: {}\n", key, value));
            }
            text.push('\n');
        }
    }

    text
}

/// Formats feature info results as a JSON response.
fn format_json(results: &[FeatureInfo], _params: &GetFeatureInfoParams) -> String {
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str("  \"type\": \"FeatureCollection\",\n");
    json.push_str("  \"features\": [\n");

    for (i, feature) in results.iter().enumerate() {
        json.push_str("    {\n");
        json.push_str("      \"type\": \"Feature\",\n");
        json.push_str(&format!(
            "      \"layer\": \"{}\",\n",
            json_escape(&feature.layer)
        ));

        json.push_str("      \"properties\": {\n");
        let props: Vec<(&String, &String)> = feature
            .attributes
            .iter()
            .filter(|(k, _)| !k.starts_with("__"))
            .collect();
        for (j, (key, value)) in props.iter().enumerate() {
            json.push_str(&format!(
                "        \"{}\": \"{}\"{}",
                json_escape(key),
                json_escape(value),
                if j < props.len() - 1 { "," } else { "" }
            ));
            json.push('\n');
        }
        json.push_str("      }\n");
        json.push_str("    }");
        if i < results.len() - 1 {
            json.push(',');
        }
        json.push('\n');
    }

    json.push_str("  ]\n");
    json.push_str("}\n");
    json
}

/// Escapes HTML special characters.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Escapes JSON special characters.
fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// WMS GetFeatureInfo handler.
pub fn handle_get_feature_info(query: &QueryParameters) -> Result<FcgiResponse> {
    let params = match GetFeatureInfoParams::from_query(query) {
        Ok(p) => p,
        Err(resp) => return Ok(resp),
    };

    debug!(
        i = params.i,
        j = params.j,
        query_layers = ?params.query_layers,
        info_format = %params.info_format,
        "WMS GetFeatureInfo request"
    );

    // Load layers from the project
    let layers: Vec<WmsLayer> =
        load_layers_from_project(query.map().unwrap_or(""), &params.query_layers);

    // Identify features at the given point
    let results = identify_features(&params, &layers);

    // Format the response
    let body = match params.info_format.as_str() {
        "text/plain" => format_text(&results, &params),
        "application/json" | "application/geo+json" => format_json(&results, &params),
        _ => format_html(&results, &params),
    };

    Ok(FcgiResponse::new(Bytes::from(body)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wms::get_map::{LayerStyle, WmsFeature, DEFAULT_EXCEPTIONS};
    use qgis_geometry::Polygon;
    use qgis_render::Pixel;

    fn make_query(params: &[(&str, &str)]) -> QueryParameters {
        let query_string = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");
        QueryParameters::parse(&query_string).unwrap()
    }

    #[test]
    fn parse_get_feature_info_params() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1,layer2"),
            ("QUERY_LAYERS", "layer1"),
            ("CRS", "EPSG:4326"),
            ("BBOX", "-180,-90,180,90"),
            ("WIDTH", "800"),
            ("HEIGHT", "600"),
            ("I", "400"),
            ("J", "300"),
        ]);

        let params = GetFeatureInfoParams::from_query(&query).unwrap();
        assert_eq!(params.query_layers, vec!["layer1"]);
        assert_eq!(params.i, 400);
        assert_eq!(params.j, 300);
        assert_eq!(params.info_format, DEFAULT_INFO_FORMAT);
    }

    #[test]
    fn parse_get_feature_info_missing_query_layers() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1"),
            ("CRS", "EPSG:4326"),
            ("BBOX", "-180,-90,180,90"),
            ("WIDTH", "800"),
            ("HEIGHT", "600"),
            ("I", "400"),
            ("J", "300"),
        ]);

        let result = GetFeatureInfoParams::from_query(&query);
        assert!(result.is_err());
    }

    #[test]
    fn parse_get_feature_info_i_out_of_bounds() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1"),
            ("QUERY_LAYERS", "layer1"),
            ("CRS", "EPSG:4326"),
            ("BBOX", "-180,-90,180,90"),
            ("WIDTH", "800"),
            ("HEIGHT", "600"),
            ("I", "900"),
            ("J", "300"),
        ]);

        let result = GetFeatureInfoParams::from_query(&query);
        assert!(result.is_err());
    }

    #[test]
    fn parse_get_feature_info_j_out_of_bounds() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1"),
            ("QUERY_LAYERS", "layer1"),
            ("CRS", "EPSG:4326"),
            ("BBOX", "-180,-90,180,90"),
            ("WIDTH", "800"),
            ("HEIGHT", "600"),
            ("I", "400"),
            ("J", "700"),
        ]);

        let result = GetFeatureInfoParams::from_query(&query);
        assert!(result.is_err());
    }

    #[test]
    fn identify_features_empty_layers() {
        let params = GetFeatureInfoParams {
            query_layers: vec!["layer1".to_string()],
            i: 100,
            j: 100,
            info_format: DEFAULT_INFO_FORMAT.to_string(),
            feature_count: DEFAULT_FEATURE_COUNT,
            map_params: GetMapParams {
                layers: vec![],
                styles: vec![],
                crs: "EPSG:4326".to_string(),
                bbox: (-180.0, -90.0, 180.0, 90.0),
                width: 200,
                height: 200,
                format: "image/png".to_string(),
                transparent: false,
                bg_color: Pixel::rgb(255, 255, 255),
                exceptions: DEFAULT_EXCEPTIONS.to_string(),
            },
        };

        let results = identify_features(&params, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn identify_features_point() {
        let params = GetFeatureInfoParams {
            query_layers: vec!["points".to_string()],
            i: 100,
            j: 100,
            info_format: DEFAULT_INFO_FORMAT.to_string(),
            feature_count: DEFAULT_FEATURE_COUNT,
            map_params: GetMapParams {
                layers: vec![],
                styles: vec![],
                crs: "EPSG:4326".to_string(),
                bbox: (-180.0, -90.0, 180.0, 90.0),
                width: 200,
                height: 200,
                format: "image/png".to_string(),
                transparent: false,
                bg_color: Pixel::rgb(255, 255, 255),
                exceptions: DEFAULT_EXCEPTIONS.to_string(),
            },
        };

        // Point at map (0, 0) — should be at pixel (100, 100) for this bbox/size
        let layer = WmsLayer {
            name: "points".to_string(),
            features: vec![WmsFeature {
                geometry: Geometry::Point(Point::new(0.0, 0.0)),
                attributes: HashMap::from([("name".to_string(), "origin".to_string())]),
            }],
            style: LayerStyle::default(),
        };

        let results = identify_features(&params, &[layer]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].layer, "points");
        assert_eq!(
            results[0].attributes.get("name").unwrap(),
            &"origin".to_string()
        );
    }

    #[test]
    fn identify_features_polygon() {
        let params = GetFeatureInfoParams {
            query_layers: vec!["polygons".to_string()],
            i: 50,
            j: 50,
            info_format: DEFAULT_INFO_FORMAT.to_string(),
            feature_count: DEFAULT_FEATURE_COUNT,
            map_params: GetMapParams {
                layers: vec![],
                styles: vec![],
                crs: "EPSG:4326".to_string(),
                bbox: (0.0, 0.0, 100.0, 100.0),
                width: 100,
                height: 100,
                format: "image/png".to_string(),
                transparent: false,
                bg_color: Pixel::rgb(255, 255, 255),
                exceptions: DEFAULT_EXCEPTIONS.to_string(),
            },
        };

        // Polygon covering (10,10) to (50,50)
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
                attributes: HashMap::from([("area".to_string(), "test".to_string())]),
            }],
            style: LayerStyle::default(),
        };

        let results = identify_features(&params, &[layer]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].layer, "polygons");
    }

    #[test]
    fn identify_features_no_match() {
        let params = GetFeatureInfoParams {
            query_layers: vec!["points".to_string()],
            i: 10,
            j: 10,
            info_format: DEFAULT_INFO_FORMAT.to_string(),
            feature_count: DEFAULT_FEATURE_COUNT,
            map_params: GetMapParams {
                layers: vec![],
                styles: vec![],
                crs: "EPSG:4326".to_string(),
                bbox: (-180.0, -90.0, 180.0, 90.0),
                width: 200,
                height: 200,
                format: "image/png".to_string(),
                transparent: false,
                bg_color: Pixel::rgb(255, 255, 255),
                exceptions: DEFAULT_EXCEPTIONS.to_string(),
            },
        };

        // Point at map (0, 0) — should be at pixel (100, 100)
        let layer = WmsLayer {
            name: "points".to_string(),
            features: vec![WmsFeature {
                geometry: Geometry::Point(Point::new(0.0, 0.0)),
                attributes: HashMap::new(),
            }],
            style: LayerStyle::default(),
        };

        // Query at pixel (10, 10) — far from the point
        let results = identify_features(&params, &[layer]);
        assert!(results.is_empty());
    }

    #[test]
    fn format_html_empty() {
        let params = GetFeatureInfoParams {
            query_layers: vec![],
            i: 0,
            j: 0,
            info_format: DEFAULT_INFO_FORMAT.to_string(),
            feature_count: DEFAULT_FEATURE_COUNT,
            map_params: GetMapParams {
                layers: vec![],
                styles: vec![],
                crs: "EPSG:4326".to_string(),
                bbox: (0.0, 0.0, 100.0, 100.0),
                width: 100,
                height: 100,
                format: "image/png".to_string(),
                transparent: false,
                bg_color: Pixel::rgb(255, 255, 255),
                exceptions: DEFAULT_EXCEPTIONS.to_string(),
            },
        };

        let html = format_html(&[], &params);
        assert!(html.contains("<html"));
        assert!(html.contains("No features found"));
    }

    #[test]
    fn format_html_with_features() {
        let params = GetFeatureInfoParams {
            query_layers: vec![],
            i: 0,
            j: 0,
            info_format: DEFAULT_INFO_FORMAT.to_string(),
            feature_count: DEFAULT_FEATURE_COUNT,
            map_params: GetMapParams {
                layers: vec![],
                styles: vec![],
                crs: "EPSG:4326".to_string(),
                bbox: (0.0, 0.0, 100.0, 100.0),
                width: 100,
                height: 100,
                format: "image/png".to_string(),
                transparent: false,
                bg_color: Pixel::rgb(255, 255, 255),
                exceptions: DEFAULT_EXCEPTIONS.to_string(),
            },
        };

        let feature = FeatureInfo {
            layer: "test".to_string(),
            feature_id: Some("1".to_string()),
            attributes: HashMap::from([("name".to_string(), "value".to_string())]),
        };

        let html = format_html(&[feature], &params);
        assert!(html.contains("test"));
        assert!(html.contains("value"));
    }

    #[test]
    fn format_text_empty() {
        let params = GetFeatureInfoParams {
            query_layers: vec![],
            i: 0,
            j: 0,
            info_format: DEFAULT_INFO_FORMAT.to_string(),
            feature_count: DEFAULT_FEATURE_COUNT,
            map_params: GetMapParams {
                layers: vec![],
                styles: vec![],
                crs: "EPSG:4326".to_string(),
                bbox: (0.0, 0.0, 100.0, 100.0),
                width: 100,
                height: 100,
                format: "image/png".to_string(),
                transparent: false,
                bg_color: Pixel::rgb(255, 255, 255),
                exceptions: DEFAULT_EXCEPTIONS.to_string(),
            },
        };

        let text = format_text(&[], &params);
        assert!(text.contains("No features found"));
    }

    #[test]
    fn format_json_empty() {
        let params = GetFeatureInfoParams {
            query_layers: vec![],
            i: 0,
            j: 0,
            info_format: DEFAULT_INFO_FORMAT.to_string(),
            feature_count: DEFAULT_FEATURE_COUNT,
            map_params: GetMapParams {
                layers: vec![],
                styles: vec![],
                crs: "EPSG:4326".to_string(),
                bbox: (0.0, 0.0, 100.0, 100.0),
                width: 100,
                height: 100,
                format: "image/png".to_string(),
                transparent: false,
                bg_color: Pixel::rgb(255, 255, 255),
                exceptions: DEFAULT_EXCEPTIONS.to_string(),
            },
        };

        let json = format_json(&[], &params);
        assert!(json.contains("FeatureCollection"));
    }

    #[test]
    fn html_escape_special_chars() {
        assert_eq!(html_escape("<b>"), "&lt;b&gt;");
        assert_eq!(html_escape("&"), "&amp;");
        assert_eq!(html_escape("\""), "&quot;");
    }

    #[test]
    fn json_escape_special_chars() {
        assert_eq!(json_escape("\""), "\\\"");
        assert_eq!(json_escape("\\"), "\\\\");
        assert_eq!(json_escape("\n"), "\\n");
    }

    #[test]
    fn test_point_to_segment_distance() {
        let p = Point::new(5.0, 5.0);
        let a = Point::new(0.0, 0.0);
        let b = Point::new(10.0, 0.0);
        let d = point_to_segment_distance(&p, &a, &b);
        assert!((d - 5.0).abs() < 1e-9);

        let p = Point::new(0.0, 0.0);
        let d = point_to_segment_distance(&p, &a, &b);
        assert!((d - 0.0).abs() < 1e-9);

        let p = Point::new(15.0, 0.0);
        let d = point_to_segment_distance(&p, &a, &b);
        assert!((d - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_point_near_linestring() {
        let line = LineString::new(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
        ]);

        // Point on the line
        assert!(point_near_linestring(
            &Point::new(5.0, 0.0),
            &line,
            1e-6
        ));
        // Point far from the line
        assert!(!point_near_linestring(
            &Point::new(5.0, 5.0),
            &line,
            1e-6
        ));
        // Point near the line
        assert!(point_near_linestring(
            &Point::new(5.0, 0.5),
            &line,
            1.0
        ));
    }

    #[test]
    fn handle_get_feature_info_returns_html() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1"),
            ("QUERY_LAYERS", "layer1"),
            ("CRS", "EPSG:4326"),
            ("BBOX", "-180,-90,180,90"),
            ("WIDTH", "800"),
            ("HEIGHT", "600"),
            ("I", "400"),
            ("J", "300"),
        ]);

        let response = handle_get_feature_info(&query).unwrap();
        let body = String::from_utf8_lossy(&response.body);
        assert!(body.contains("<html"));
    }

    #[test]
    fn handle_get_feature_info_missing_query_layers() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1"),
            ("CRS", "EPSG:4326"),
            ("BBOX", "-180,-90,180,90"),
            ("WIDTH", "800"),
            ("HEIGHT", "600"),
            ("I", "400"),
            ("J", "300"),
        ]);

        let response = handle_get_feature_info(&query).unwrap();
        let body = String::from_utf8_lossy(&response.body);
        assert!(body.contains("MissingParameterValue"));
        assert!(body.contains("QUERY_LAYERS"));
    }

    #[test]
    fn handle_get_feature_info_i_out_of_bounds() {
        let query = make_query(&[
            ("MAP", "/tmp/test.qgs"),
            ("LAYERS", "layer1"),
            ("QUERY_LAYERS", "layer1"),
            ("CRS", "EPSG:4326"),
            ("BBOX", "-180,-90,180,90"),
            ("WIDTH", "800"),
            ("HEIGHT", "600"),
            ("I", "900"),
            ("J", "300"),
        ]);

        let response = handle_get_feature_info(&query).unwrap();
        let body = String::from_utf8_lossy(&response.body);
        assert!(body.contains("InvalidParameterValue"));
    }

    #[test]
    fn geometry_to_wkt_point() {
        let p = Point::new(1.5, 2.5);
        assert_eq!(geometry_to_wkt(&Geometry::Point(p)), "POINT (1.5 2.5)");
    }

    #[test]
    fn geometry_to_wkt_linestring() {
        let ls = LineString::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
        ]);
        assert_eq!(
            geometry_to_wkt(&Geometry::LineString(ls)),
            "LINESTRING (0 0, 1 1)"
        );
    }

    #[test]
    fn geometry_to_wkt_polygon() {
        let ring = LineString::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
        ]);
        let poly = Polygon::new(ring.points().to_vec());
        let wkt = geometry_to_wkt(&Geometry::Polygon(poly));
        assert!(wkt.starts_with("POLYGON (("));
        assert!(wkt.ends_with("))"));
    }
}
