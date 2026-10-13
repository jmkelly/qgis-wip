//! ST_* CRS functions.

use crate::facade::{CrsTransform, TransformError};
use std::fmt;

#[derive(Debug)]
pub enum StError {
    Transform(TransformError),
    InvalidEpsg(i32),
}

impl fmt::Display for StError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StError::Transform(e) => write!(f, "{e}"),
            StError::InvalidEpsg(c) => write!(f, "invalid epsg: {c}"),
        }
    }
}
impl std::error::Error for StError {}

pub fn st_set_srid_wkt(wkt: &str, srid: i32) -> String {
    format!("SRID={};{}", srid, wkt)
}

pub fn st_geom_from_wkt(wkt: &str, srid: i32) -> String {
    if srid == 0 { wkt.to_string() } else { format!("SRID={};{}", srid, wkt) }
}

pub fn st_geom_from_ewkt(ewkt: &str) -> (i32, String) {
    if let Some(rest) = ewkt.strip_prefix("SRID=") {
        if let Some(idx) = rest.find(';') {
            let srid_str = &rest[..idx];
            let wkt = &rest[idx + 1..];
            let srid = srid_str.parse().unwrap_or(0);
            return (srid, wkt.to_string());
        }
    }
    (0, ewkt.to_string())
}

pub fn st_as_ewkt(wkt: &str, srid: i32) -> String {
    if srid == 0 { wkt.to_string() } else { format!("SRID={};{}", srid, wkt) }
}

pub fn st_transform_point(x: f64, y: f64, from_epsg: i32, to_epsg: i32) -> Result<(f64, f64), StError> {
    let src = CrsTransform::from_epsg(from_epsg).map_err(StError::Transform)?;
    let dst = CrsTransform::from_epsg(to_epsg).map_err(StError::Transform)?;
    src.transform_point(x, y, &dst).map_err(StError::Transform)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_ewkt() {
        let e = st_geom_from_wkt("POINT(1 2)", 4326);
        assert!(e.starts_with("SRID=4326;"));
        let (s, w) = st_geom_from_ewkt(&e);
        assert_eq!(s, 4326);
        assert_eq!(w, "POINT(1 2)");
    }
    #[test]
    fn transform_same() {
        let r = st_transform_point(10.0, 20.0, 4326, 4326).unwrap();
        assert!((r.0 - 10.0).abs() < 1e-9);
    }
}
