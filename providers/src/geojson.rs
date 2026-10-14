//! Minimal GeoJSON (RFC 7946) parser/serializer helpers.

use qgis_geometry::Geometry as QGeom;
use std::collections::BTreeMap;
pub use crate::Feature;
pub use crate::FeatureCollection;

impl crate::FeatureCollection {
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
    let x = c.first().and_then(|v| v.as_f64()).ok_or("invalid point x")?;
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

fn point_coords(p: &qgis_geometry::Point) -> serde_json::Value {
    let mut coords = vec![serde_json::json!(p.x), serde_json::json!(p.y)];
    if let Some(z) = p.z {
        coords.push(serde_json::json!(z));
        if let Some(m) = p.m {
            coords.push(serde_json::json!(m));
        }
    }
    serde_json::Value::Array(coords)
}

fn polygon_rings_to_json(poly: &qgis_geometry::Polygon) -> serde_json::Value {
    if poly.exterior.is_empty() && poly.interiors.is_empty() {
        return serde_json::Value::Array(vec![]);
    }
    let mut rings = Vec::with_capacity(1 + poly.interiors.len());
    rings.push(serde_json::Value::Array(
        poly.exterior.iter().map(point_coords).collect(),
    ));
    for hole in &poly.interiors {
        rings.push(serde_json::Value::Array(
            hole.iter().map(point_coords).collect(),
        ));
    }
    serde_json::Value::Array(rings)
}

fn geom_to_json_value(g: &QGeom) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    match g {
        QGeom::Point(p) => {
            obj.insert("type".into(), serde_json::Value::String("Point".into()));
            obj.insert("coordinates".into(), point_coords(p));
        }
        QGeom::LineString(ls) => {
            obj.insert("type".into(), serde_json::Value::String("LineString".into()));
            let coords = ls.points().iter().map(point_coords).collect();
            obj.insert("coordinates".into(), serde_json::Value::Array(coords));
        }
        QGeom::Polygon(poly) => {
            obj.insert("type".into(), serde_json::Value::String("Polygon".into()));
            obj.insert("coordinates".into(), polygon_rings_to_json(poly));
        }
        QGeom::MultiPoint(mp) => {
            obj.insert("type".into(), serde_json::Value::String("MultiPoint".into()));
            let coords = mp.points().iter().map(point_coords).collect();
            obj.insert("coordinates".into(), serde_json::Value::Array(coords));
        }
        QGeom::MultiLineString(mls) => {
            obj.insert("type".into(), serde_json::Value::String("MultiLineString".into()));
            let lines = mls
                .lines()
                .iter()
                .map(|ls| serde_json::Value::Array(ls.points().iter().map(point_coords).collect()))
                .collect();
            obj.insert("coordinates".into(), serde_json::Value::Array(lines));
        }
        QGeom::MultiPolygon(mp) => {
            obj.insert("type".into(), serde_json::Value::String("MultiPolygon".into()));
            let polys = mp.polygons().iter().map(polygon_rings_to_json).collect();
            obj.insert("coordinates".into(), serde_json::Value::Array(polys));
        }
        QGeom::GeometryCollection(gc) => {
            obj.insert("type".into(), serde_json::Value::String("GeometryCollection".into()));
            let geoms = gc.geometries().iter().map(geom_to_json_value).collect();
            obj.insert("geometries".into(), serde_json::Value::Array(geoms));
        }
    }
    serde_json::Value::Object(obj)
}

fn geometry_to_json(g: &Option<QGeom>) -> serde_json::Value {
    match g {
        None => serde_json::Value::Null,
        Some(geom) => geom_to_json_value(geom),
    }
}

pub fn read_to_features(path: &str) -> Result<FeatureCollection, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    FeatureCollection::from_json(&content)
}

pub struct GeoJsonProvider;
impl crate::FeatureProvider for GeoJsonProvider {
    fn read_to_features(&self, path: &str) -> Result<crate::FeatureCollection, String> {
        read_to_features(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qgis_geometry::{
        GeometryCollection, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon,
    };

    #[test]
    fn round_trip() {
        let mut fc = FeatureCollection::new();
        let mut props = BTreeMap::new();
        props.insert("name".into(), serde_json::json!("alpha"));

        let ext = vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
            Point::new(0.0, 0.0),
        ];
        let hole = vec![
            Point::new(2.0, 2.0),
            Point::new(2.0, 8.0),
            Point::new(8.0, 8.0),
            Point::new(8.0, 2.0),
            Point::new(2.0, 2.0),
        ];
        let poly_with_hole = Polygon::with_holes(ext.clone(), vec![hole]);
        let simple_poly = Polygon::new(ext);

        let geoms = vec![
            Some(QGeom::Point(Point::new(1.0, 2.0))),
            Some(QGeom::Point(Point::new_zm(1.0, 2.0, 3.0, 4.0))),
            Some(QGeom::LineString(LineString::new(vec![
                Point::new(0.0, 0.0),
                Point::new(1.5, 2.5),
            ]))),
            Some(QGeom::Polygon(poly_with_hole.clone())),
            Some(QGeom::MultiPoint(MultiPoint::new(vec![
                Point::new(1.0, 2.0),
                Point::new(3.0, 4.0),
            ]))),
            Some(QGeom::MultiLineString(MultiLineString::new(vec![
                LineString::new(vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)]),
                LineString::new(vec![Point::new(2.0, 2.0), Point::new(3.0, 3.0)]),
            ]))),
            Some(QGeom::MultiPolygon(MultiPolygon::new(vec![
                simple_poly,
                poly_with_hole,
            ]))),
            Some(QGeom::GeometryCollection(GeometryCollection::new(vec![
                QGeom::Point(Point::new(5.0, 6.0)),
                QGeom::LineString(LineString::new(vec![
                    Point::new(0.0, 0.0),
                    Point::new(1.0, 1.0),
                ])),
            ]))),
            None,
        ];

        for g in geoms {
            fc.features.push(Feature {
                ty: "Feature".into(),
                geometry: g,
                properties: props.clone(),
            });
        }

        let s = fc.to_json().unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(
            v["features"][0]["geometry"]["coordinates"],
            serde_json::json!([1.0, 2.0])
        );
        let back = FeatureCollection::from_json(&s).unwrap();
        assert_eq!(back, fc);
    }
}
