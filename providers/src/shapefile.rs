//! Minimal Shapefile (SHP) header + record parsing.

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct ShpHeader {
    pub file_code: i32,
    pub file_length: i32, // 16-bit words
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

pub fn read (path: impl AsRef<Path>) -> Result<ShpHeader, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_file_code() {
        // Build a minimal 100-byte header with file code 9994
        let mut buf = vec![0u8; 100];
        buf[0..4].copy_from_slice(&9994i32.to_be_bytes());
        buf[24..28].copy_from_slice(&50i32.to_be_bytes());
        buf[28..32].copy_from_slice(&1000i32.to_le_bytes());
        buf[32..36].copy_from_slice(&1i32.to_le_bytes()); // point
        let p = std::env::temp_dir().join("t.shp");
        std::fs::write(&p, &buf).unwrap();
        let h = read_shp_header(&p).unwrap();
        assert_eq!(h.file_code, 9994);
        assert_eq!(h.shape_type, 1);
        assert_eq!(h.version, 1000);
    }
}
