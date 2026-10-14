//! Minimal GeoPackage (GPKG) geometry BLOB header parsing.

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
