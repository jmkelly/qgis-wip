//! WMTS GetFeatureInfo handler — identifies features within a tile.
//!
//! Translates the tile address plus intra-tile pixel coordinates into a map
//! point and delegates to the WMS identify implementation.
//!
//! Reference: OGC 07-057r7 section 8 (GetFeatureInfo).

use bytes::Bytes;
use qgis_render::Pixel;
use tracing::debug;

use crate::error::Result;
use crate::fcgi::FcgiResponse;
use crate::query::QueryParameters;
use crate::service::service_exception;
use crate::wms::get_feature_info::{
    DEFAULT_FEATURE_COUNT, FeatureInfo, identify_features,
};
use crate::wms::get_feature_info::GetFeatureInfoParams as WmsIdentifyParams;
use crate::wms::get_map::{GetMapParams, load_layers_from_project};

use super::get_tile::{GetTileParams, crs_for_tiles};

/// Default info format for WMTS GetFeatureInfo responses.
pub const WMTS_DEFAULT_INFO_FORMAT: &str = "text/html";

/// Info formats supported by WMTS GetFeatureInfo.
pub const WMTS_INFO_FORMATS: &[&str] = &[
    "text/html",
    "text/plain",
    "text/xml",
    "application/json",
    "application/geo+json",
];

/// Parsed and validated WMTS GetFeatureInfo parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct GetFeatureInfoParams {
    /// Underlying tile parameters.
    pub tile: GetTileParams,
    /// Intra-tile pixel column.
    pub i: usize,
    /// Intra-tile pixel row.
    pub j: usize,
    /// Output info format MIME type.
    pub info_format: String,
    /// Maximum number of features per layer.
    pub feature_count: usize,
}

impl GetFeatureInfoParams {
    /// Parses GetFeatureInfo parameters from query string parameters.
    pub fn from_query(query: &QueryParameters) -> std::result::Result<Self, FcgiResponse> {
        // Tile addressing is shared with GetTile, so reuse its validation.
        let tile = GetTileParams::from_query(query)?;

        let i = parse_pixel(query.get("I"), "I", tile.width)?;
        let j = parse_pixel(query.get("J"), "J", tile.height)?;

        let info_format = query
            .get_or("INFOFORMAT", WMTS_DEFAULT_INFO_FORMAT)
            .to_string();
        if !WMTS_INFO_FORMATS.contains(&info_format.as_str()) {
            return Err(service_exception(
                "InvalidParameterValue",
                format!(
                    "Unsupported INFOFORMAT '{}'. Supported: {}",
                    info_format,
                    WMTS_INFO_FORMATS.join(", ")
                ),
            ));
        }

        let feature_count = query
            .get("FEATURECOUNT")
            .or_else(|| query.get("FEATURE_COUNT"))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(DEFAULT_FEATURE_COUNT);

        Ok(Self {
            tile,
            i,
            j,
            info_format,
            feature_count,
        })
    }

    /// Converts these parameters into the equivalent WMS identify parameters.
    ///
    /// Returns a ServiceException when the tile address does not resolve.
    pub fn to_wms_params(&self) -> std::result::Result<WmsIdentifyParams, FcgiResponse> {
        let bbox = self.tile.bounds().ok_or_else(|| {
            service_exception(
                "InvalidParameterValue",
                format!(
                    "Tile ({}, {}) is outside TILEMATRIX '{}' of TILEMATRIXSET '{}'",
                    self.tile.row, self.tile.col, self.tile.matrix, self.tile.matrix_set
                ),
            )
        })?;

        Ok(WmsIdentifyParams {
            query_layers: vec![self.tile.layer.clone()],
            i: self.i as u32,
            j: self.j as u32,
            info_format: self.info_format.clone(),
            feature_count: self.feature_count,
            map_params: GetMapParams {
                layers: vec![self.tile.layer.clone()],
                styles: vec![self.tile.style.clone()],
                crs: crs_for_tiles(&self.tile.matrix_set),
                bbox,
                width: self.tile.width as u32,
                height: self.tile.height as u32,
                format: self.tile.format.clone(),
                transparent: false,
                bg_color: Pixel::rgb(255, 255, 255),
                exceptions: String::new(),
            },
        })
    }
}

/// Parses an intra-tile pixel coordinate and checks it against the tile size.
fn parse_pixel(
    value: Option<&str>,
    name: &str,
    limit: usize,
) -> std::result::Result<usize, FcgiResponse> {
    let raw = value.ok_or_else(|| {
        service_exception(
            "MissingParameterValue",
            format!("{name} parameter is required"),
        )
    })?;

    let pixel = raw.parse::<usize>().map_err(|_| {
        service_exception(
            "InvalidParameterValue",
            format!("{name} must be a non-negative integer"),
        )
    })?;

    if pixel >= limit {
        return Err(service_exception(
            "InvalidParameterValue",
            format!("{name} ({pixel}) is outside the tile size ({limit})"),
        ));
    }

    Ok(pixel)
}

/// Formats identified features as a GeoJSON FeatureCollection.
fn format_geojson(results: &[FeatureInfo]) -> String {
    if results.is_empty() {
        return "{\n  \"type\": \"FeatureCollection\",\n  \"features\": []\n}\n".to_string();
    }

    let mut json = String::new();
    json.push_str("{\n  \"type\": \"FeatureCollection\",\n  \"features\": [\n");

    for (i, feature) in results.iter().enumerate() {
        json.push_str("    {\n");
        json.push_str("      \"type\": \"Feature\",\n");
        json.push_str(&format!(
            "      \"layer\": \"{}\",\n",
            escape_json(&feature.layer)
        ));
        json.push_str("      \"properties\": {");
        let props: Vec<(&String, &String)> = feature
            .attributes
            .iter()
            .filter(|(k, _)| !k.starts_with("__"))
            .collect();
        for (j, (key, value)) in props.iter().enumerate() {
            json.push_str(&format!(
                "\n        \"{}\": \"{}\"{}",
                escape_json(key),
                escape_json(value),
                if j + 1 < props.len() { "," } else { "" }
            ));
        }
        if !props.is_empty() {
            json.push('\n');
        }
        json.push_str("      }\n");
        json.push_str("    }");
        if i + 1 < results.len() {
            json.push(',');
        }
        json.push('\n');
    }

    json.push_str("  ]\n}\n");
    json
}

/// Formats identified features as XML.
fn format_xml(results: &[FeatureInfo]) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<FeatureInfoResponse>\n");
    for feature in results {
        xml.push_str(&format!(
            "  <Feature layer=\"{}\">\n",
            escape_xml(&feature.layer)
        ));
        for (key, value) in &feature.attributes {
            if key.starts_with("__") {
                continue;
            }
            xml.push_str(&format!(
                "    <Attribute name=\"{}\">{}</Attribute>\n",
                escape_xml(key),
                escape_xml(value)
            ));
        }
        xml.push_str("  </Feature>\n");
    }
    xml.push_str("</FeatureInfoResponse>\n");
    xml
}

/// Formats identified features as plain text.
fn format_text(results: &[FeatureInfo]) -> String {
    if results.is_empty() {
        return "No features found at the requested location.\n".to_string();
    }
    let mut text = String::new();
    for feature in results {
        text.push_str(&format!("Layer: {}\n", feature.layer));
        for (key, value) in &feature.attributes {
            if key.starts_with("__") {
                continue;
            }
            text.push_str(&format!("  {key}: {value}\n"));
        }
        text.push('\n');
    }
    text
}

/// Escapes XML special characters.
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Escapes JSON special characters.
fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// WMTS GetFeatureInfo handler.
pub fn handle_get_feature_info(query: &QueryParameters) -> Result<FcgiResponse> {
    let params = match GetFeatureInfoParams::from_query(query) {
        Ok(p) => p,
        Err(resp) => return Ok(resp),
    };

    let wms_params = match params.to_wms_params() {
        Ok(p) => p,
        Err(resp) => return Ok(resp),
    };

    debug!(
        layer = %params.tile.layer,
        i = params.i,
        j = params.j,
        info_format = %params.info_format,
        "WMTS GetFeatureInfo request"
    );

    let layers = load_layers_from_project(
        query.map().unwrap_or(""),
        std::slice::from_ref(&params.tile.layer),
    );
    let results = identify_features(&wms_params, &layers);

    let body = match params.info_format.as_str() {
        "text/plain" => format_text(&results),
        "text/xml" => format_xml(&results),
        "application/json" | "application/geo+json" => format_geojson(&results),
        _ => {
            // The WMS formatter is HTML-first and already escapes attributes.
            render_html(&results, &params.info_format)
        }
    };

    Ok(FcgiResponse::new(Bytes::from(body)))
}

/// Formats identified features as an HTML page.
fn render_html(results: &[FeatureInfo], _info_format: &str) -> String {
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html>\n<head><meta charset=\"utf-8\"/>");
    html.push_str("<title>WMTS Feature Info</title></head>\n<body>\n");

    if results.is_empty() {
        html.push_str("<p>No features found at the requested location.</p>\n");
    } else {
        for feature in results {
            html.push_str(&format!("<h3>{}</h3>\n", escape_xml(&feature.layer)));
            html.push_str("<table border=\"1\">\n");
            for (key, value) in &feature.attributes {
                if key.starts_with("__") {
                    continue;
                }
                html.push_str(&format!(
                    "<tr><td><b>{}</b></td><td>{}</td></tr>\n",
                    escape_xml(key),
                    escape_xml(value)
                ));
            }
            html.push_str("</table>\n");
        }
    }

    html.push_str("</body>\n</html>\n");
    html
}

#[cfg(test)]
mod tests {
    use super::*;
    use qgis_geometry::{Geometry, LineString, Point};
    use std::collections::HashMap;

    use crate::wms::get_map::{LayerStyle, WmsFeature, WmsLayer};

    /// Returns the body of an error response as a string.
    fn err_body(
        result: std::result::Result<GetFeatureInfoParams, FcgiResponse>,
    ) -> String {
        String::from_utf8_lossy(&result.unwrap_err().body).to_string()
    }

    fn make_query(params: &[(&str, &str)]) -> QueryParameters {
        let query_string = params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        QueryParameters::parse(&query_string).unwrap()
    }

    fn tile_params() -> &'static [(&'static str, &'static str)] {
        &[
            ("MAP", "/tmp/p.qgs"),
            ("LAYER", "roads"),
            ("STYLE", "default"),
            ("TILEMATRIXSET", "EPSG4326"),
            ("TILEMATRIX", "0"),
            ("TILEROW", "0"),
            ("TILECOL", "0"),
        ]
    }

    #[test]
    fn parse_full_params() {
        let mut pairs = tile_params().to_vec();
        pairs.push(("I", "10"));
        pairs.push(("J", "20"));
        pairs.push(("INFOFORMAT", "text/plain"));
        pairs.push(("FEATURECOUNT", "3"));

        let params = GetFeatureInfoParams::from_query(&make_query(&pairs)).unwrap();
        assert_eq!(params.i, 10);
        assert_eq!(params.j, 20);
        assert_eq!(params.info_format, "text/plain");
        assert_eq!(params.feature_count, 3);
    }

    #[test]
    fn parse_defaults() {
        let mut pairs = tile_params().to_vec();
        pairs.push(("I", "0"));
        pairs.push(("J", "0"));
        let params = GetFeatureInfoParams::from_query(&make_query(&pairs)).unwrap();
        assert_eq!(params.info_format, WMTS_DEFAULT_INFO_FORMAT);
        assert_eq!(params.feature_count, DEFAULT_FEATURE_COUNT);
    }

    #[test]
    fn parse_requires_tile_parameters() {
        let query = make_query(&[("LAYER", "roads"), ("I", "1"), ("J", "1")]);
        let body = err_body(GetFeatureInfoParams::from_query(&query));
        assert!(body.contains("TILEMATRIXSET"));
    }

    #[test]
    fn parse_requires_i_and_j() {
        let query = make_query(tile_params());
        let body = err_body(GetFeatureInfoParams::from_query(&query));
        assert!(body.contains("MissingParameterValue"));
        assert!(body.contains("I parameter is required"));

        let mut pairs = tile_params().to_vec();
        pairs.push(("I", "1"));
        let query = make_query(&pairs);
        let body = err_body(GetFeatureInfoParams::from_query(&query));
        assert!(body.contains("J parameter is required"));
    }

    #[test]
    fn parse_rejects_pixel_outside_tile() {
        let mut pairs = tile_params().to_vec();
        pairs.push(("I", "256"));
        pairs.push(("J", "0"));
        let body = err_body(GetFeatureInfoParams::from_query(&make_query(&pairs)));
        assert!(body.contains("InvalidParameterValue"));
        assert!(body.contains("tile size"));
    }

    #[test]
    fn parse_rejects_non_numeric_pixel() {
        let mut pairs = tile_params().to_vec();
        pairs.push(("I", "x"));
        pairs.push(("J", "0"));
        assert!(GetFeatureInfoParams::from_query(&make_query(&pairs)).is_err());
    }

    #[test]
    fn parse_rejects_unknown_infoformat() {
        let mut pairs = tile_params().to_vec();
        pairs.push(("I", "0"));
        pairs.push(("J", "0"));
        pairs.push(("INFOFORMAT", "image/tiff"));
        let body = err_body(GetFeatureInfoParams::from_query(&make_query(&pairs)));
        assert!(body.contains("INFOFORMAT"));
    }

    #[test]
    fn converts_to_wms_params() {
        let mut pairs = tile_params().to_vec();
        pairs.push(("I", "128"));
        pairs.push(("J", "64"));
        let params = GetFeatureInfoParams::from_query(&make_query(&pairs)).unwrap();
        let wms = params.to_wms_params().unwrap();
        assert_eq!(wms.query_layers, vec!["roads".to_string()]);
        assert_eq!(wms.i, 128);
        assert_eq!(wms.j, 64);
        assert_eq!(wms.map_params.crs, "EPSG:4326");
        assert_eq!(wms.map_params.width, 256);
    }

    #[test]
    fn converts_mercator_tile_to_wms_params() {
        let mut pairs = tile_params().to_vec();
        pairs[3] = ("TILEMATRIXSET", "WebMercatorQuad");
        pairs.push(("I", "0"));
        pairs.push(("J", "0"));
        let params = GetFeatureInfoParams::from_query(&make_query(&pairs)).unwrap();
        let wms = params.to_wms_params().unwrap();
        assert_eq!(wms.map_params.crs, "EPSG:3857");
    }

    #[test]
    fn out_of_range_tile_has_no_bounds() {
        let mut pairs = tile_params().to_vec();
        pairs[5] = ("TILEROW", "9");
        pairs.push(("I", "0"));
        pairs.push(("J", "0"));
        let params = GetFeatureInfoParams::from_query(&make_query(&pairs)).unwrap();
        let body = String::from_utf8_lossy(&params.to_wms_params().unwrap_err().body).to_string();
        assert!(body.contains("InvalidParameterValue"));
    }

    #[test]
    fn format_text_reports_empty() {
        assert!(format_text(&[]).contains("No features found"));
    }

    #[test]
    fn format_text_lists_attributes() {
        let feature = FeatureInfo {
            layer: "roads".to_string(),
            feature_id: None,
            attributes: HashMap::from([("name".to_string(), "Main St".to_string())]),
        };
        let text = format_text(&[feature]);
        assert!(text.contains("Layer: roads"));
        assert!(text.contains("name: Main St"));
    }

    #[test]
    fn format_geojson_is_a_feature_collection() {
        let feature = FeatureInfo {
            layer: "roads".to_string(),
            feature_id: None,
            attributes: HashMap::from([("name".to_string(), "Main St".to_string())]),
        };
        let json = format_geojson(&[feature]);
        assert!(json.contains("\"FeatureCollection\""));
        assert!(json.contains("\"roads\""));
        assert!(json.contains("Main St"));
    }

    #[test]
    fn format_geojson_empty() {
        assert!(format_geojson(&[]).contains("\"features\": []"));
    }

    #[test]
    fn format_xml_lists_attributes() {
        let feature = FeatureInfo {
            layer: "roads".to_string(),
            feature_id: None,
            attributes: HashMap::from([("name".to_string(), "Main St".to_string())]),
        };
        let xml = format_xml(&[feature]);
        assert!(xml.contains("FeatureInfoResponse"));
        assert!(xml.contains("layer=\"roads\""));
        assert!(xml.contains("Main St"));
    }

    #[test]
    fn format_html_reports_empty() {
        assert!(render_html(&[], "text/html").contains("No features found"));
    }

    #[test]
    fn format_html_escapes_attributes() {
        let feature = FeatureInfo {
            layer: "a&b".to_string(),
            feature_id: None,
            attributes: HashMap::from([("k".to_string(), "<x>".to_string())]),
        };
        let html = render_html(&[feature], "text/html");
        assert!(html.contains("a&amp;b"));
        assert!(!html.contains("<x>"));
    }

    #[test]
    fn escapes_handlers() {
        assert_eq!(escape_xml("<a>"), "&lt;a&gt;");
        assert_eq!(escape_json("\""), "\\\"");
        assert_eq!(escape_json("\\"), "\\\\");
        assert_eq!(escape_json("\n"), "\\n");
    }

    #[test]
    fn handle_get_feature_info_returns_html() {
        let mut pairs = tile_params().to_vec();
        pairs.push(("I", "10"));
        pairs.push(("J", "10"));
        let resp = handle_get_feature_info(&make_query(&pairs)).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("<html"));
    }

    #[test]
    fn handle_get_feature_info_returns_geojson() {
        let mut pairs = tile_params().to_vec();
        pairs.push(("I", "10"));
        pairs.push(("J", "10"));
        pairs.push(("INFOFORMAT", "application/json"));
        let resp = handle_get_feature_info(&make_query(&pairs)).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("FeatureCollection"));
    }

    #[test]
    fn handle_get_feature_info_finds_feature_in_tile() {
        // A line crossing the first tile of EPSG4326 level 0.
        let mut pairs = tile_params().to_vec();
        pairs.push(("I", "128"));
        pairs.push(("J", "128"));
        let query = make_query(&pairs);

        let line = LineString::new(vec![Point::new(-180.0, 0.0), Point::new(0.0, 0.0)]);
        let layer = WmsLayer {
            name: "roads".to_string(),
            features: vec![WmsFeature {
                geometry: Geometry::LineString(line),
                attributes: HashMap::from([("name".to_string(), "Equator".to_string())]),
            }],
            style: LayerStyle::default(),
        };

        let params = GetFeatureInfoParams::from_query(&query).unwrap();
        let wms = params.to_wms_params().unwrap();
        let results = identify_features(&wms, &[layer]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].layer, "roads");
    }
}