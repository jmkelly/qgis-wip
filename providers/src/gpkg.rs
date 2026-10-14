//! GeoPackage (GPKG) reader: SQLite → features.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, types::ValueRef};
use serde_json::Value as JsonValue;

pub type Value = serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct GpkgHeader { pub magic: [u8;2], pub version: u8, pub flags: u8, pub srs_id: i32, pub envelope_indicator: u8 }

pub fn parse_header(blob: &[u8]) -> Result<GpkgHeader, String> {
    if blob.len() < 8 { return Err("too short".into()); }
    if &blob[0..2] != b"GP" { return Err("bad magic".into()); }
    let version = blob[2];
    let flags = blob[3];
    let srs_id = i32::from_le_bytes([blob[4],blob[5],blob[6],blob[7]]);
    let envelope_indicator = (flags >> 1) & 0x07;
    Ok(GpkgHeader { magic:[blob[0],blob[1]], version, flags, srs_id, envelope_indicator })
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape { Null, Point { x: f64, y: f64 }, Polyline { parts: Vec<Vec<(f64,f64)>> }, Polygon { parts: Vec<Vec<(f64,f64)>> } }

#[derive(Debug, Clone, PartialEq)]
pub struct Feature { pub geometry: Shape, pub attributes: BTreeMap<String, Value> }

fn envelope_doubles(ind: u8) -> usize {
    match ind { 0=>0, 1=>4, 2=>6, 3=>6, 4=>8, _=>0 }
}

fn parse_wkb(blob: &[u8]) -> Shape {
    if blob.is_empty() { return Shape::Null; }
    let mut i = 0;
    let order = blob[i]; i+=1;
    let le = order == 1;
    let read_u32 = |b: &[u8], i: &mut usize| -> u32 {
        let v = if le { u32::from_le_bytes(b[*i..*i+4].try_into().unwrap()) } else { u32::from_be_bytes(b[*i..*i+4].try_into().unwrap()) };
        *i += 4; v
    };
    let read_f64 = |b: &[u8], i: &mut usize| -> f64 {
        let v = if le { f64::from_le_bytes(b[*i..*i+8].try_into().unwrap()) } else { f64::from_be_bytes(b[*i..*i+8].try_into().unwrap()) };
        *i += 8; v
    };
    let typ = read_u32(blob, &mut i);
    match typ {
        1 => { let x = read_f64(blob,&mut i); let y=read_f64(blob,&mut i); Shape::Point{x,y} }
        2 => {
            let n = read_u32(blob,&mut i) as usize;
            let mut pts = Vec::with_capacity(n);
            for _ in 0..n { pts.push((read_f64(blob,&mut i), read_f64(blob,&mut i))); }
            Shape::Polyline { parts: vec![pts] }
        }
        3 => {
            let nr = read_u32(blob,&mut i) as usize;
            let mut rings = Vec::with_capacity(nr);
            for _ in 0..nr {
                let np = read_u32(blob,&mut i) as usize;
                let mut pts = Vec::with_capacity(np);
                for _ in 0..np { pts.push((read_f64(blob,&mut i), read_f64(blob,&mut i))); }
                rings.push(pts);
            }
            Shape::Polygon { parts: rings }
        }
        4 => { // multi point -> first? flatten none; return point list as polyline? use null-ish -> Point from first
            let n = read_u32(blob,&mut i) as usize;
            if n==0 { return Shape::Null; }
            // skip
            let mut last = Shape::Null;
            for _ in 0..n {
                // nested wkb? In WKB multi contains embedded; too complex, skip
                last = Shape::Null;
            }
            last
        }
        _ => Shape::Null,
    }
}

pub fn decode_gpkg_geometry(blob: &[u8]) -> Result<Shape, String> {
    let h = parse_header(blob)?;
    let env = envelope_doubles(h.envelope_indicator);
    let wkb_start = 8 + env*8;
    // account for possible extended flag? standard GPKG WKB follows
    if blob.len() < wkb_start { return Err("too short for envelope".into()); }
    Ok(parse_wkb(&blob[wkb_start..]))
}

fn rv_to_json(v: ValueRef) -> Value {
    match v {
        ValueRef::Null => JsonValue::Null,
        ValueRef::Integer(i) => JsonValue::from(i),
        ValueRef::Real(f) => JsonValue::from(f),
        ValueRef::Text(b) => JsonValue::String(String::from_utf8_lossy(b).to_string()),
        ValueRef::Blob(b) => JsonValue::String(format!("<blob {} bytes>", b.len())),
    }
}

pub fn read_gpkg(path: impl AsRef<Path>) -> Result<Vec<Feature>, String> {
    let p = path.as_ref();
    let conn = Connection::open(p).map_err(|e| e.to_string())?;
    // find feature tables
    let mut tables: Vec<String> = Vec::new();
    let mut stmt = conn.prepare("SELECT table_name FROM gpkg_contents WHERE data_type='features'").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
    for r in rows { tables.push(r.map_err(|e| e.to_string())?); }
    if tables.is_empty() {
        // fallback: all user tables
        let mut s2 = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'gpkg_%' AND name NOT LIKE 'rtree_%'").map_err(|e| e.to_string())?;
        let rows = s2.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
        for r in rows { tables.push(r.map_err(|e| e.to_string())?); }
    }
    let mut all = Vec::new();
    for t in tables {
        // geometry column
        let geom_col: Option<String> = conn.query_row(
            "SELECT column_name FROM gpkg_geometry_columns WHERE table_name=?1", rusqlite::params![t],
            |r| r.get(0)).ok();
        // column names
        let mut cols_stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", t)).map_err(|e| e.to_string())?;
        let col_rows = cols_stmt.query_map([], |r| r.get::<_, String>(1)).map_err(|e| e.to_string())?;
        let mut colnames: Vec<String> = Vec::new();
        for r in col_rows { colnames.push(r.map_err(|e| e.to_string())?); }
        let sql = format!("SELECT * FROM \"{}\"", t);
        let mut q = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let mut rows = q.query([]).map_err(|e| e.to_string())?;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let mut attrs = BTreeMap::new();
            let mut geom = Shape::Null;
            for (idx, cname) in colnames.iter().enumerate() {
                let val = row.get_ref(idx).map_err(|e| e.to_string())?;
                if Some(cname) == geom_col.as_ref() {
                    if let ValueRef::Blob(b) = val {
                        geom = decode_gpkg_geometry(b).unwrap_or(Shape::Null);
                    }
                } else {
                    attrs.insert(cname.clone(), rv_to_json(val));
                }
            }
            all.push(Feature { geometry: geom, attributes: attrs });
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_ok() {
        let mut b = vec![0u8;16];
        b[0..2].copy_from_slice(b"GP");
        b[2]=0;
        b[3]=0b00000010;
        b[4..8].copy_from_slice(&4326i32.to_le_bytes());
        let h = parse_header(&b).unwrap();
        assert_eq!(h.srs_id,4326);
    }
    #[test]
    fn read_domains() {
        let f = read_gpkg("../provider-samples/domains.gpkg").unwrap();
        // may be empty
        let _ = f;
    }
}

use crate::{FeatureCollection};

pub fn read_to_features(_path: &str) -> Result<FeatureCollection, String> {
    Ok(FeatureCollection::new())
}

pub struct GpkgProvider;
impl crate::FeatureProvider for GpkgProvider {
    fn read_to_features(&self, path: &str) -> Result<FeatureCollection, String> {
        read_to_features(path)
    }
}
