//! Minimal GeoJSON parser for WMS vector data (pure Rust, no dependencies).
//!
//! Parses GeoJSON FeatureCollection documents into [`WmsFeature`] objects
//! for rendering and feature identification. Supports Point, LineString,
//! Polygon, MultiPoint, MultiLineString, MultiPolygon, and
//! GeometryCollection geometries.
//!
//! This is a constrained parser for the GeoJSON subset used by the QGIS
//! Rust port test suite — not a full RFC 7946 implementation.

use qgis_geometry::{Geometry, LineString, Point, Polygon};
use std::collections::HashMap;

use super::get_map::WmsFeature;

/// Errors that can occur while parsing GeoJSON.
#[derive(Debug, Clone, PartialEq)]
pub enum GeoJsonError {
    /// The document is not valid JSON.
    InvalidJson(String),
    /// The document is not a FeatureCollection.
    NotAFeatureCollection,
    /// A feature is missing required fields.
    InvalidFeature(String),
    /// A geometry type is not supported.
    UnsupportedGeometry(String),
}

impl std::fmt::Display for GeoJsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson(s) => write!(f, "invalid JSON: {s}"),
            Self::NotAFeatureCollection => write!(f, "not a FeatureCollection"),
            Self::InvalidFeature(s) => write!(f, "invalid feature: {s}"),
            Self::UnsupportedGeometry(s) => write!(f, "unsupported geometry: {s}"),
        }
    }
}

impl std::error::Error for GeoJsonError {}

/// A parsed GeoJSON FeatureCollection.
#[derive(Debug, Clone)]
pub struct FeatureCollection {
    /// Features in the collection.
    pub features: Vec<WmsFeature>,
    /// CRS identifier (e.g., "EPSG:4326"), if specified.
    pub crs: Option<String>,
}

/// Parses a GeoJSON FeatureCollection from a string.
pub fn parse_geojson(text: &str) -> Result<FeatureCollection, GeoJsonError> {
    let value = parse_json(text).map_err(GeoJsonError::InvalidJson)?;

    let obj = value.as_object().ok_or(GeoJsonError::NotAFeatureCollection)?;
    let type_name = JsonValue::get(&value, "type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GeoJsonError::InvalidJson("missing type".to_string()))?;

    if type_name != "FeatureCollection" {
        return Err(GeoJsonError::NotAFeatureCollection);
    }

    let crs = JsonValue::get(&value, "crs")
        .and_then(|c| c.get("properties"))
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());

    let features_arr = JsonValue::get(&value, "features")
        .and_then(|f| f.as_array())
        .ok_or_else(|| GeoJsonError::InvalidJson("missing features".to_string()))?;

    let mut features = Vec::new();
    for (idx, feat_val) in features_arr.iter().enumerate() {
        let feature = parse_feature(feat_val)
            .map_err(|e| GeoJsonError::InvalidFeature(format!("feature {idx}: {e}")))?;
        features.push(feature);
    }

    Ok(FeatureCollection { features, crs })
}

/// Parses a single GeoJSON feature.
fn parse_feature(value: &JsonValue) -> Result<WmsFeature, String> {
    let _obj = value.as_object().ok_or("not an object")?;

    let geometry_val = JsonValue::get(&value, "geometry")
        .ok_or_else(|| "missing geometry".to_string())?;
    let geometry = parse_geometry(geometry_val)?;

    let properties = JsonValue::get(&value, "properties")
        .and_then(|p| p.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| {
                    let val = match v {
                        JsonValue::String(s) => s.clone(),
                        JsonValue::Number(n) => n.to_string(),
                        JsonValue::Bool(b) => b.to_string(),
                        JsonValue::Null => String::new(),
                        other => format!("{other:?}"),
                    };
                    (k.clone(), val)
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();

    Ok(WmsFeature {
        geometry,
        attributes: properties,
    })
}

/// Parses a GeoJSON geometry object.
fn parse_geometry(value: &JsonValue) -> Result<Geometry, String> {
    let _obj = value.as_object().ok_or("geometry not an object")?;
    let type_name = JsonValue::get(&value, "type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing geometry type".to_string())?;
    let coords = JsonValue::get(&value, "coordinates")
        .and_then(|c| c.as_array())
        .ok_or_else(|| "missing coordinates".to_string())?;

    match type_name {
        "Point" => {
            let p = parse_point(coords)?;
            Ok(Geometry::Point(p))
        }
        "LineString" => {
            let points: Vec<Point> = coords
                .iter()
                .map(|c| parse_point(c.as_array().unwrap_or(&[])))
                .collect::<Result<_, _>>()?;
            Ok(Geometry::LineString(LineString::new(points)))
        }
        "Polygon" => parse_polygon(coords),
        "MultiPoint" => {
            let points: Vec<Point> = coords
                .iter()
                .map(|c| parse_point(c.as_array().unwrap_or(&[])))
                .collect::<Result<_, _>>()?;
            Ok(Geometry::MultiPoint(qgis_geometry::MultiPoint::new(points)))
        }
        "MultiLineString" => {
            let lines: Vec<LineString> = coords
                .iter()
                .map(|line| {
                    let points: Vec<Point> = line
                        .as_array()
                        .unwrap_or(&[])
                        .iter()
                        .map(|c| parse_point(c.as_array().unwrap_or(&[])))
                        .collect::<Result<_, _>>()?;
                    Ok::<_, String>(LineString::new(points))
                })
                .collect::<Result<_, _>>()?;
            Ok(Geometry::MultiLineString(
                qgis_geometry::MultiLineString::new(lines),
            ))
        }
        "MultiPolygon" => {
            let polys: Vec<Polygon> = coords
                .iter()
                .map(|p| parse_ring_set(p.as_array().unwrap_or(&[])))
                .collect::<Result<_, _>>()?;
            Ok(Geometry::MultiPolygon(qgis_geometry::MultiPolygon::new(polys)))
        }
        "GeometryCollection" => {
            let geometries_arr = JsonValue::get(&value, "geometries")
                .and_then(|g| g.as_array())
                .ok_or_else(|| "missing geometries".to_string())?;
            let geoms: Vec<Geometry> = geometries_arr
                .iter()
                .map(parse_geometry)
                .collect::<Result<_, _>>()?;
            Ok(Geometry::GeometryCollection(
                qgis_geometry::GeometryCollection::new(geoms),
            ))
        }
        other => Err(format!("unsupported geometry type: {other}")),
    }
}

/// Parses a GeoJSON position `[x, y]` or `[x, y, z]`.
fn parse_point(coords: &[JsonValue]) -> Result<Point, String> {
    if coords.len() < 2 {
        return Err("point must have at least 2 coordinates".to_string());
    }
    let x = coords[0]
        .as_f64()
        .ok_or_else(|| "invalid x coordinate".to_string())?;
    let y = coords[1]
        .as_f64()
        .ok_or_else(|| "invalid y coordinate".to_string())?;
    Ok(Point::new(x, y))
}

/// Parses a GeoJSON polygon (array of linear rings).
fn parse_polygon(rings: &[JsonValue]) -> Result<Geometry, String> {
    let poly = parse_ring_set(rings)?;
    Ok(Geometry::Polygon(poly))
}

/// Parses a set of linear rings into a Polygon.
fn parse_ring_set(rings: &[JsonValue]) -> Result<Polygon, String> {
    if rings.is_empty() {
        return Err("polygon must have at least one ring".to_string());
    }
    let exterior: Vec<Point> = rings[0]
        .as_array()
        .ok_or_else(|| "ring must be an array".to_string())?
        .iter()
        .map(|c| parse_point(c.as_array().unwrap_or(&[])))
        .collect::<Result<_, _>>()?;
    Ok(Polygon::new(exterior))
}

// ── Minimal JSON parser ─────────────────────────────────────────────────────

/// A parsed JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    /// Returns the value as an object's field vector.
    pub fn as_object(&self) -> Option<&[(String, JsonValue)]> {
        match self {
            Self::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// Returns the value as an array.
    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            Self::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Returns the value as a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the value as an f64.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Returns a field from an Object by key.
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            Self::Object(obj) => obj.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// Parses a JSON string into a [`JsonValue`].
pub fn parse_json(text: &str) -> Result<JsonValue, String> {
    let mut parser = JsonParser::new(text);
    parser.parse()
}

/// A minimal recursive-descent JSON parser.
struct JsonParser<'a> {
    text: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text: text.as_bytes(),
            pos: 0,
        }
    }

    fn parse(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        let value = self.parse_value()?;
        self.skip_ws();
        if self.pos != self.text.len() {
            return Err(format!(
                "trailing content at byte {}",
                self.pos
            ));
        }
        Ok(value)
    }

    fn skip_ws(&mut self) {
        while self.pos < self.text.len() {
            match self.text[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.pos).copied()
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => Ok(JsonValue::String(self.parse_string()?)),
            Some(b't') => self.parse_literal("true", JsonValue::Bool(true)),
            Some(b'f') => self.parse_literal("false", JsonValue::Bool(false)),
            Some(b'n') => self.parse_literal("null", JsonValue::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.parse_number(),
            other => Err(format!(
                "unexpected character {other:?} at byte {}",
                self.pos
            )),
        }
    }

    fn parse_literal(&mut self, lit: &str, value: JsonValue) -> Result<JsonValue, String> {
        if self.text[self.pos..].starts_with(lit.as_bytes()) {
            self.pos += lit.len();
            Ok(value)
        } else {
            Err(format!("expected '{lit}' at byte {}", self.pos))
        }
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || c == b'+' || c == b'-' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let s = std::str::from_utf8(&self.text[start..self.pos])
            .map_err(|e| e.to_string())?;
        s.parse::<f64>()
            .map(JsonValue::Number)
            .map_err(|e| format!("invalid number '{s}': {e}"))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.pos += 1; // skip opening quote
        let mut result = String::new();
        while let Some(c) = self.peek() {
            match c {
                b'"' => {
                    self.pos += 1;
                    return Ok(result);
                }
                b'\\' => {
                    self.pos += 1;
                    let escaped = self.peek().ok_or_else(|| "unterminated escape".to_string())?;
                    self.pos += 1;
                    match escaped {
                        b'"' => result.push('"'),
                        b'\\' => result.push('\\'),
                        b'/' => result.push('/'),
                        b'n' => result.push('\n'),
                        b't' => result.push('\t'),
                        b'r' => result.push('\r'),
                        _ => return Err(format!("invalid escape sequence \\{}", escaped as char)),
                    }
                }
                _ => {
                    result.push(c as char);
                    self.pos += 1;
                }
            }
        }
        Err(format!("unterminated string at byte {}", self.pos))
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.pos += 1; // skip '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(items));
        }
        loop {
            let value = self.parse_value()?;
            items.push(value);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(JsonValue::Array(items));
                }
                other => {
                    return Err(format!(
                        "expected ',' or ']' at byte {} (got {other:?})",
                        self.pos
                    ))
                }
            }
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.pos += 1; // skip '{'
        let mut fields = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(fields));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(format!(
                    "expected string key at byte {}",
                    self.pos
                ));
            }
            let key = self.parse_string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(format!("expected ':' at byte {}", self.pos));
            }
            self.pos += 1;
            let value = self.parse_value()?;
            fields.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(JsonValue::Object(fields));
                }
                other => {
                    return Err(format!(
                        "expected ',' or '}}' at byte {} (got {other:?})",
                        self.pos
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_point() {
        let text = r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{"name":"test"},"geometry":{"type":"Point","coordinates":[1.0,2.0]}}]}"#;
        let fc = parse_geojson(text).unwrap();
        assert_eq!(fc.features.len(), 1);
        match &fc.features[0].geometry {
            Geometry::Point(p) => {
                assert_eq!(p.x, 1.0);
                assert_eq!(p.y, 2.0);
            }
            other => panic!("expected Point, got {other:?}"),
        }
        assert_eq!(
            fc.features[0].attributes.get("name").unwrap(),
            &"test".to_string()
        );
    }

    #[test]
    fn parse_linestring() {
        let text = r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{},"geometry":{"type":"LineString","coordinates":[[0,0],[1,1],[2,0]]}}]}"#;
        let fc = parse_geojson(text).unwrap();
        match &fc.features[0].geometry {
            Geometry::LineString(ls) => {
                assert_eq!(ls.points().len(), 3);
            }
            other => panic!("expected LineString, got {other:?}"),
        }
    }

    #[test]
    fn parse_polygon() {
        let text = r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{},"geometry":{"type":"Polygon","coordinates":[[[0,0],[10,0],[10,10],[0,10],[0,0]]]}}]}"#;
        let fc = parse_geojson(text).unwrap();
        match &fc.features[0].geometry {
            Geometry::Polygon(_) => {}
            other => panic!("expected Polygon, got {other:?}"),
        }
    }

    #[test]
    fn parse_multipoint() {
        let text = r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{},"geometry":{"type":"MultiPoint","coordinates":[[0,0],[1,1]]}}]}"#;
        let fc = parse_geojson(text).unwrap();
        match &fc.features[0].geometry {
            Geometry::MultiPoint(mp) => {
                assert_eq!(mp.points().len(), 2);
            }
            other => panic!("expected MultiPoint, got {other:?}"),
        }
    }

    #[test]
    fn parse_multilinestring() {
        let text = r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{},"geometry":{"type":"MultiLineString","coordinates":[[[0,0],[1,1]],[[2,2],[3,3]]]}}]}"#;
        let fc = parse_geojson(text).unwrap();
        match &fc.features[0].geometry {
            Geometry::MultiLineString(mls) => {
                assert_eq!(mls.lines().len(), 2);
            }
            other => panic!("expected MultiLineString, got {other:?}"),
        }
    }

    #[test]
    fn parse_multipolygon() {
        let text = r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{},"geometry":{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,1],[0,0]]]]}}]}"#;
        let fc = parse_geojson(text).unwrap();
        match &fc.features[0].geometry {
            Geometry::MultiPolygon(mp) => {
                assert_eq!(mp.polygons().len(), 1);
            }
            other => panic!("expected MultiPolygon, got {other:?}"),
        }
    }

    #[test]
    fn parse_crs() {
        let text = r#"{"type":"FeatureCollection","crs":{"type":"name","properties":{"name":"urn:ogc:def:crs:OGC:1.3:CRS84"}},"features":[]}"#;
        let fc = parse_geojson(text).unwrap();
        assert_eq!(
            fc.crs.as_deref(),
            Some("urn:ogc:def:crs:OGC:1.3:CRS84")
        );
    }

    #[test]
    fn parse_empty_features() {
        let text = r#"{"type":"FeatureCollection","features":[]}"#;
        let fc = parse_geojson(text).unwrap();
        assert!(fc.features.is_empty());
    }

    #[test]
    fn parse_not_feature_collection() {
        let text = r#"{"type":"Feature","properties":{},"geometry":{"type":"Point","coordinates":[0,0]}}"#;
        let result = parse_geojson(text);
        assert!(result.is_err());
    }

    #[test]
    fn parse_invalid_json() {
        let text = "not json";
        let result = parse_geojson(text);
        assert!(result.is_err());
    }

    #[test]
    fn parse_properties_types() {
        let text = r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{"str":"hello","num":42,"float":3.14,"bool":true,"null":null},"geometry":{"type":"Point","coordinates":[0,0]}}]}"#;
        let fc = parse_geojson(text).unwrap();
        let attrs = &fc.features[0].attributes;
        assert_eq!(attrs.get("str").unwrap(), &"hello".to_string());
        assert_eq!(attrs.get("num").unwrap(), &"42".to_string());
        assert_eq!(attrs.get("float").unwrap(), &"3.14".to_string());
        assert_eq!(attrs.get("bool").unwrap(), &"true".to_string());
        assert_eq!(attrs.get("null").unwrap(), &"".to_string());
    }

    #[test]
    fn json_parser_basic_types() {
        assert_eq!(parse_json("null").unwrap(), JsonValue::Null);
        assert_eq!(parse_json("true").unwrap(), JsonValue::Bool(true));
        assert_eq!(parse_json("false").unwrap(), JsonValue::Bool(false));
        assert_eq!(parse_json("42").unwrap(), JsonValue::Number(42.0));
        assert_eq!(parse_json("-3.14").unwrap(), JsonValue::Number(-3.14));
        assert_eq!(
            parse_json("\"hello\"").unwrap(),
            JsonValue::String("hello".to_string())
        );
    }

    #[test]
    fn json_parser_array() {
        let v = parse_json("[1, 2, 3]").unwrap();
        assert_eq!(
            v,
            JsonValue::Array(vec![
                JsonValue::Number(1.0),
                JsonValue::Number(2.0),
                JsonValue::Number(3.0),
            ])
        );
    }

    #[test]
    fn json_parser_object() {
        let v = parse_json(r#"{"a": 1, "b": "two"}"#).unwrap();
        assert_eq!(
            v,
            JsonValue::Object(vec![
                ("a".to_string(), JsonValue::Number(1.0)),
                ("b".to_string(), JsonValue::String("two".to_string())),
            ])
        );
    }

    #[test]
    fn json_parser_nested() {
        let v = parse_json(r#"{"a": [1, {"b": true}]}"#).unwrap();
        assert_eq!(
            v,
            JsonValue::Object(vec![(
                "a".to_string(),
                JsonValue::Array(vec![
                    JsonValue::Number(1.0),
                    JsonValue::Object(vec![(
                        "b".to_string(),
                        JsonValue::Bool(true)
                    )]),
                ])
            )])
        );
    }

    #[test]
    fn json_parser_string_escapes() {
        let v = parse_json(r#""hello\\world""#).unwrap();
        assert_eq!(v, JsonValue::String("hello\\world".to_string()));
    }

    #[test]
    fn json_parser_error_trailing() {
        let result = parse_json("{} extra");
        assert!(result.is_err());
    }

    #[test]
    fn json_parser_error_invalid() {
        let result = parse_json("{invalid}");
        assert!(result.is_err());
    }
}
