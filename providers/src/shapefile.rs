//! Shapefile (SHP) + DBF record reader producing features.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::dbase::read_dbf_header;

pub type Value = serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct ShpHeader {
    pub file_code: i32,
    pub file_length: i32,
    pub version: i32,
    pub shape_type: i32,
    pub xmin: f64, pub ymin: f64, pub xmax: f64, pub ymax: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Null,
    Point { x: f64, y: f64 },
    PointZ { x: f64, y: f64, z: f64 },
    Polyline { parts: Vec<Vec<(f64, f64)>> },
    Polygon { parts: Vec<Vec<(f64, f64)>> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub geometry: Shape,
    pub attributes: BTreeMap<String, Value>,
}

pub fn read(path: impl AsRef<Path>) -> Result<ShpHeader, String> {
    read_shp_header(path)
}

pub fn read_shp_header(path: impl AsRef<Path>) -> Result<ShpHeader, String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    if data.len() < 100 { return Err("too short".into()); }
    let be_i32 = |off: usize| i32::from_be_bytes([data[off], data[off+1], data[off+2], data[off+3]]);
    let le_i32 = |off: usize| i32::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3]]);
    let le_f64 = |off: usize| f64::from_le_bytes(data[off..off+8].try_into().unwrap());
    Ok(ShpHeader {
        file_code: be_i32(0),
        file_length: be_i32(24),
        version: le_i32(28),
        shape_type: le_i32(32),
        xmin: le_f64(36),
        ymin: le_f64(44),
        xmax: le_f64(52),
        ymax: le_f64(60),
    })
}

fn le_i32(data: &[u8], off: usize) -> i32 {
    i32::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3]])
}
fn le_f64(data: &[u8], off: usize) -> f64 {
    f64::from_le_bytes(data[off..off+8].try_into().unwrap())
}

fn read_poly(rec: &[u8], off0: usize) -> Vec<Vec<(f64, f64)>> {
    let mut off = off0 + 32; // bbox
    let num_parts = le_i32(rec, off) as usize; off += 4;
    let num_points = le_i32(rec, off) as usize; off += 4;
    let mut parts_idx = Vec::with_capacity(num_parts);
    for _ in 0..num_parts {
        parts_idx.push(le_i32(rec, off) as usize); off += 4;
    }
    let mut points = Vec::with_capacity(num_points);
    for _ in 0..num_points {
        points.push((le_f64(rec, off), le_f64(rec, off+8))); off += 16;
    }
    let mut parts = Vec::with_capacity(num_parts);
    for i in 0..num_parts {
        let start = parts_idx[i];
        let end = if i+1 < num_parts { parts_idx[i+1] } else { num_points };
        parts.push(points[start..end].to_vec());
    }
    parts
}

pub fn read_shp_records(path: impl AsRef<Path>) -> Result<Vec<Shape>, String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    if data.len() < 100 { return Err("too short".into()); }
    let mut shapes = Vec::new();
    let mut off = 100usize;
    while off + 8 <= data.len() {
        let rec_len = i32::from_be_bytes([data[off+4], data[off+5], data[off+6], data[off+7]]) as usize * 2;
        off += 8;
        if rec_len == 0 || off + rec_len > data.len() { break; }
        let rec = &data[off..off+rec_len];
        let st = i32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]);
        let shape = match st {
            0 => Shape::Null,
            1 => {
                let x = le_f64(rec, 4);
                let y = le_f64(rec, 12);
                Shape::Point { x, y }
            }
            11 => {
                let x = le_f64(rec, 4);
                let y = le_f64(rec, 12);
                let z = le_f64(rec, 20);
                Shape::PointZ { x, y, z }
            }
            3 => Shape::Polyline { parts: read_poly(rec, 4) },
            5 => Shape::Polygon { parts: read_poly(rec, 4) },
            _ => Shape::Null,
        };
        shapes.push(shape);
        off += rec_len;
    }
    Ok(shapes)
}

fn read_dbf_records(path: &Path, header: &crate::dbase::DbfHeader) -> Vec<BTreeMap<String, Value>> {
    let data = match fs::read(path) { Ok(d) => d, Err(_) => return vec![] };
    let mut out = Vec::new();
    let mut off = header.header_len as usize;
    for _ in 0..header.record_count {
        if off + header.record_len as usize > data.len() { break; }
        let rec = &data[off..off + header.record_len as usize];
        off += header.record_len as usize;
        if rec.is_empty() || rec[0] == b'*' { continue; }
        let mut map = BTreeMap::new();
        let mut col_off = 1usize;
        for f in &header.fields {
            let len = f.length as usize;
            let end = col_off + len;
            if end > rec.len() { break; }
            let bytes = &rec[col_off..end];
            let s = String::from_utf8_lossy(bytes).trim().to_string();
            let val = match f.field_type {
                'C' | 'D' => Value::String(s),
                'N' | 'F' => {
                    if s.is_empty() { Value::Null }
                    else if f.decimals > 0 {
                        s.parse::<f64>().map(Value::from).unwrap_or(Value::String(s))
                    } else {
                        s.parse::<i64>().map(Value::from).unwrap_or_else(|_| s.parse::<f64>().map(Value::from).unwrap_or(Value::String(s)))
                    }
                }
                'L' => match s.as_str() {
                    "T" | "t" | "Y" | "y" => Value::Bool(true),
                    "F" | "f" | "N" | "n" => Value::Bool(false),
                    _ => Value::Null,
                },
                _ => Value::String(s),
            };
            map.insert(f.name.clone(), val);
            col_off = end;
        }
        out.push(map);
    }
    out
}

pub fn read_shapefile(path: impl AsRef<Path>) -> Result<Vec<Feature>, String> {
    let shp_path = path.as_ref();
    let shapes = read_shp_records(shp_path)?;
    let dbf_path = shp_path.with_extension("dbf");
    let dbf_records = if dbf_path.exists() {
        let header = read_dbf_header(&dbf_path)?;
        read_dbf_records(&dbf_path, &header)
    } else {
        vec![]
    };
    let mut features = Vec::with_capacity(shapes.len());
    for (i, g) in shapes.into_iter().enumerate() {
        let attrs = dbf_records.get(i).cloned().unwrap_or_default();
        features.push(Feature { geometry: g, attributes: attrs });
    }
    Ok(features)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_file_code() {
        let mut buf = vec![0u8; 100];
        buf[0..4].copy_from_slice(&9994i32.to_be_bytes());
        buf[24..28].copy_from_slice(&50i32.to_be_bytes());
        buf[28..32].copy_from_slice(&1000i32.to_le_bytes());
        buf[32..36].copy_from_slice(&1i32.to_le_bytes());
        let p = std::env::temp_dir().join("t.shp");
        std::fs::write(&p, &buf).unwrap();
        let h = read_shp_header(&p).unwrap();
        assert_eq!(h.file_code, 9994);
        assert_eq!(h.shape_type, 1);
        assert_eq!(h.version, 1000);
    }
}

use crate::Feature as CommonFeature;
use crate::FeatureCollection;
use qgis_geometry::Geometry as QGeom;

fn shape_to_geometry(s: &Shape) -> Option<QGeom> {
    use qgis_geometry::{Point, LineString, Polygon, MultiLineString};
    match s {
        Shape::Null => None,
        Shape::Point { x, y } => Some(QGeom::Point(Point::new(*x, *y))),
        Shape::PointZ { x, y, z } => {
            let mut p = Point::new(*x, *y);
            p.z = Some(*z);
            Some(QGeom::Point(p))
        }
        Shape::Polyline { parts } => {
            if parts.len() == 1 {
                let pts: Vec<Point> = parts[0].iter().map(|(x,y)| Point::new(*x,*y)).collect();
                Some(QGeom::LineString(LineString::new(pts)))
            } else {
                let lines: Vec<LineString> = parts.iter().map(|part| LineString::new(part.iter().map(|(x,y)| Point::new(*x,*y)).collect())).collect();
                Some(QGeom::MultiLineString(MultiLineString::new(lines)))
            }
        }
        Shape::Polygon { parts } => {
            if parts.len() == 1 {
                let pts: Vec<Point> = parts[0].iter().map(|(x,y)| Point::new(*x,*y)).collect();
                Some(QGeom::Polygon(Polygon::new(pts)))
            } else {
                let exterior: Vec<Point> = parts[0].iter().map(|(x,y)| Point::new(*x,*y)).collect();
                let holes: Vec<Vec<Point>> = parts[1..].iter().map(|p| p.iter().map(|(x,y)| Point::new(*x,*y)).collect()).collect();
                Some(QGeom::Polygon(Polygon::with_holes(exterior, holes)))
            }
        }
    }
}

pub fn read_to_features(path: impl AsRef<std::path::Path>) -> Result<FeatureCollection, String> {
    let feats = read_shapefile(path)?;
    let mut out = FeatureCollection::new();
    for f in feats {
        out.features.push(CommonFeature {
            ty: "Feature".into(),
            geometry: shape_to_geometry(&f.geometry),
            properties: f.attributes,
        });
    }
    Ok(out)
}

pub struct ShapefileProvider;
impl crate::FeatureProvider for ShapefileProvider {
    fn read_to_features(&self, path: &str) -> Result<FeatureCollection, String> {
        read_to_features(path)
    }
}
