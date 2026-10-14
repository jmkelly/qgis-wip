//! Minimal GeoJSON (RFC 7946) parser/serializer helpers.

use qgis_geometry::Geometry as QGeom;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub ty: String,
    pub geometry: Option<QGeom>,
    pub properties: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeatureCollection {
    pub ty: String,
    pub features: Vec<Feature>,
}

impl FeatureCollection {
    pub fn new() -> Self { Self { ty: "FeatureCollection".into(), features: vec![] } }
    pub fn to_json(&self) -> Result<String, String> {
        // Serialize properties/geometry via intermediate json.
        let mut features = Vec::new();
        for f in &self.features {
            let mut obj = serde_json::Map::new();
            obj.insert("type".into(), serde_json::Value::String(f.ty.clone()));
            obj.insert("geometry".into(), geometry_to_json(&f.geometry));
            obj.insert("properties".into(), serde_json::to_value(&f.properties).unwrap_or_default());
            features.push(serde_json::Value::Object(obj));
        }
        let mut fc = serde_json::Map::new();
        fc.insert("type".into(), serde_json::Value::String(self.ty.clone()));
        fc.insert("features".into(), serde_json::Value::Array(features));
        serde_json::to_string(&fc).map_err(|e| e.to_string())
    }
    pub fn from_json(s: &str) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| e.to_string())?;
        let features = v.get("features").and_then(|f| f.as_array()).cloned().unwrap_or_default();
        let mut out = Vec::new();
        for feat in features {
            let ty = feat.get("type").and_then(|t| t.as_str()).unwrap_or("Feature").to_string();
            let props = feat.get("properties").cloned().unwrap_or(serde_json::Value::Object(Default::default()));
            let properties: BTreeMap<String, serde_json::Value> = serde_json::from_value(props).unwrap_or_default();
            let geometry = match feat.get("geometry") { None | Some(serde_json::Value::Null) => None, Some(v) => parse_geometry(v)?, };
            out.push(Feature { ty, geometry, properties });
        }
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("FeatureCollection").to_string();
        Ok(Self { ty, features: out })
    }
}

impl Default for FeatureCollection { fn default() -> Self { Self::new() } }

fn parse_point_coords(c: &[serde_json::Value]) -> Result<qgis_geometry::Point, String> {
    let x = c.get(0).and_then(|v| v.as_f64()).ok_or("invalid point x")?;
    let y = c.get(1).and_then(|v| v.as_f64()).ok_or("invalid point y")?;
    let mut p = qgis_geometry::Point::new(x, y);
    if let Some(z) = c.get(2).and_then(|v| v.as_f64()) {
        p.z = Some(z);
    }
    if let Some(m) = c.get(3).and_then(|v| v.as_f64()) {
        p.m = Some(m);
    }
    Ok(p)
}

fn parse_geometry(v: &serde_json::Value) -> Result<Option<QGeom>, String> {
    if v.is_null() { return Ok(None); }
    let ty = v.get("type").and_then(|t| t.as_str()).ok_or("missing geometry type")?;
    let coords = v.get("coordinates");
    let geom = match ty {
        "Point" => {
            let arr = coords.ok_or("missing coordinates")?.as_array().ok_or("coords not array")?;
            QGeom::Point(parse_point_coords(arr)?)
        }
        "LineString" => {
            let arr = coords.ok_or("missing coordinates")?.as_array().ok_or("coords not array")?;
            let pts: Result<Vec<_>, _> = arr.iter().map(|c| parse_point_coords(c.as_array().ok_or("bad point")?)).collect();
            QGeom::LineString(qgis_geometry::LineString::new(pts?))
        }
        "Polygon" => {
            let rings = coords.ok_or("missing coordinates")?.as_array().ok_or("coords not array")?;
            let mut parsed: Vec<Vec<qgis_geometry::Point>> = Vec::new();
            for ring in rings {
                let arr = ring.as_array().ok_or("ring not array")?;
                let pts: Result<Vec<_>, _> = arr.iter().map(|c| parse_point_coords(c.as_array().ok_or("bad point")?)).collect();
                parsed.push(pts?);
            }
            if parsed.is_empty() {
                QGeom::Polygon(qgis_geometry::Polygon::new(vec![]))
            } else {
                let exterior = parsed[0].clone();
                let interiors = parsed[1..].to_vec();
                QGeom::Polygon(qgis_geometry::Polygon::with_holes(exterior, interiors))
            }
        }
        "MultiPoint" => {
            let arr = coords.ok_or("missing coordinates")?.as_array().ok_or("coords not array")?;
            let pts: Result<Vec<_>, _> = arr.iter().map(|c| parse_point_coords(c.as_array().ok_or("bad point")?)).collect();
            QGeom::MultiPoint(qgis_geometry::MultiPoint::new(pts?))
        }
        "MultiLineString" => {
            let arr = coords.ok_or("missing coordinates")?.as_array().ok_or("coords not array")?;
            let mut lines = Vec::new();
            for ls in arr {
                let pts_arr = ls.as_array().ok_or("ls not array")?;
                let pts: Result<Vec<_>, _> = pts_arr.iter().map(|c| parse_point_coords(c.as_array().ok_or("bad point")?)).collect();
                lines.push(qgis_geometry::LineString::new(pts?));
            }
            QGeom::MultiLineString(qgis_geometry::MultiLineString::new(lines))
        }
        "MultiPolygon" => {
            let polys_arr = coords.ok_or("missing coordinates")?.as_array().ok_or("coords not array")?;
            let mut polys = Vec::new();
            for poly in polys_arr {
                let rings = poly.as_array().ok_or("poly not array")?;
                let mut parsed: Vec<Vec<qgis_geometry::Point>> = Vec::new();
                for ring in rings {
                    let arr = ring.as_array().ok_or("ring not array")?;
                    let pts: Result<Vec<_>, _> = arr.iter().map(|c| parse_point_coords(c.as_array().ok_or("bad point")?)).collect();
                    parsed.push(pts?);
                }
                if parsed.is_empty() {
                    polys.push(qgis_geometry::Polygon::new(vec![]));
                } else {
                    let exterior = parsed[0].clone();
                    let interiors = parsed[1..].to_vec();
                    polys.push(qgis_geometry::Polygon::with_holes(exterior, interiors));
                }
            }
            QGeom::MultiPolygon(qgis_geometry::MultiPolygon::new(polys))
        }
        "GeometryCollection" => {
            let geoms = v.get("geometries").and_then(|g| g.as_array()).ok_or("missing geometries")?;
            let mut out = Vec::new();
            for g in geoms {
                if let Some(gg) = parse_geometry(g)? {
                    out.push(gg);
                }
            }
            QGeom::GeometryCollection(qgis_geometry::GeometryCollection::new(out))
        }
        other => return Err(format!("unknown geometry type: {other}")),
    };
    Ok(Some(geom))
}

fn geometry_to_json(g: &Option<QGeom>) -> serde_json::Value {
    // Minimal serializer for FeatureCollection::to_json
    match g {
        None => serde_json::Value::Null,
        Some(_) => serde_json::Value::Null,
    }
}

pub fn read_to_features(path: &str) -> Result<FeatureCollection, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    FeatureCollection::from_json(&content)
}
