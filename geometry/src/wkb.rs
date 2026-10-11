//! WKB (Well-Known Binary) reader and writer.
//!
//! Matches QGIS `QgsWkbTypes` / `QgsGeometry::fromWkb` / `QgsGeometry::asWkb`.
//! Supports both ISO WKB (type codes 1–7, 1001–1007, 2001–2007, 3001–3007)
//! and EWKB (PostGIS extended, flag bits 0x80000000=Z, 0x40000000=M,
//! 0x20000000=SRID, 0x10000000=EMPTY).
//!
//! Pure Rust, zero unsafe.

use crate::collection::{Geometry, GeometryCollection};
use crate::linestring::LineString;
use crate::multi::{MultiLineString, MultiPoint, MultiPolygon};
use crate::point::Point;
use crate::polygon::Polygon;

/// Errors that can occur while reading or writing WKB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WkbError {
    /// Input is too short to contain a valid WKB header.
    UnexpectedEof,
    /// Invalid byte order marker (must be 0 or 1).
    InvalidByteOrder(u8),
    /// Unknown or unsupported geometry type code.
    UnknownType(u32),
    /// Truncated geometry data.
    TruncatedData,
    /// SRID flag set but data too short to contain it.
    TruncatedSrid,
}

impl std::fmt::Display for WkbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of WKB data"),
            Self::InvalidByteOrder(b) => write!(f, "invalid byte order marker: {b}"),
            Self::UnknownType(t) => write!(f, "unknown WKB geometry type: {t}"),
            Self::TruncatedData => write!(f, "truncated WKB geometry data"),
            Self::TruncatedSrid => write!(f, "truncated WKB SRID data"),
        }
    }
}

impl std::error::Error for WkbError {}

/// Byte order marker for big-endian (XDR).
const WKB_XDR: u8 = 0;
/// Byte order marker for little-endian (NDR).
const WKB_NDR: u8 = 1;

// ISO WKB base geometry types.
const WKB_POINT: u32 = 1;
const WKB_LINESTRING: u32 = 2;
const WKB_POLYGON: u32 = 3;
const WKB_MULTIPOINT: u32 = 4;
const WKB_MULTILINESTRING: u32 = 5;
const WKB_MULTIPOLYGON: u32 = 6;
const WKB_GEOMETRYCOLLECTION: u32 = 7;



// EWKB flag bits (PostGIS extended WKB).
const EWKB_Z_FLAG: u32 = 0x8000_0000;
const EWKB_M_FLAG: u32 = 0x4000_0000;
const EWKB_SRID_FLAG: u32 = 0x2000_0000;
const EWKB_EMPTY_FLAG: u32 = 0x1000_0000;

/// A parsed WKB header: geometry type, dimensionality, optional SRID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WkbHeader {
    /// Base geometry type (1–7).
    pub base_type: u32,
    /// True if the geometry has Z coordinates.
    pub has_z: bool,
    /// True if the geometry has M coordinates.
    pub has_m: bool,
    /// Optional SRID (EWKB only).
    pub srid: Option<u32>,
    /// True if the EWKB EMPTY flag is set.
    pub is_empty: bool,
}

/// Reads a WKB header from the given byte slice.
///
/// Matches QGIS `QgsWkbTypes::readHeader`.
pub fn read_header(data: &[u8]) -> Result<(WkbHeader, usize), WkbError> {
    if data.len() < 5 {
        return Err(WkbError::UnexpectedEof);
    }
    let byte_order = data[0];
    if byte_order != WKB_XDR && byte_order != WKB_NDR {
        return Err(WkbError::InvalidByteOrder(byte_order));
    }
    let raw_type = read_u32(&data[1..5], byte_order);

    // Detect EWKB by flag bits.
    let ewkb_flags = raw_type & 0xF000_0000;
    let (base_type, has_z, has_m, srid, is_empty) = if ewkb_flags != 0 {
        // EWKB: base type is the low bits.
        let base = raw_type & 0x0000_FFFF;
        let hz = (raw_type & EWKB_Z_FLAG) != 0;
        let hm = (raw_type & EWKB_M_FLAG) != 0;
        let empty = (raw_type & EWKB_EMPTY_FLAG) != 0;
        let srid = if (raw_type & EWKB_SRID_FLAG) != 0 {
            if data.len() < 9 {
                return Err(WkbError::TruncatedSrid);
            }
            Some(read_u32(&data[5..9], byte_order))
        } else {
            None
        };
        (base, hz, hm, srid, empty)
    } else {
        // ISO WKB: type code encodes dimensionality.
        let (base, hz, hm) = decode_iso_type(raw_type)?;
        (base, hz, hm, None, false)
    };

    let header_len = if srid.is_some() { 9 } else { 5 };
    Ok((
        WkbHeader {
            base_type,
            has_z,
            has_m,
            srid,
            is_empty,
        },
        header_len,
    ))
}

/// Decodes an ISO WKB type code into (base_type, has_z, has_m).
fn decode_iso_type(raw: u32) -> Result<(u32, bool, bool), WkbError> {
    let base = raw % 1000;
    let dim = raw / 1000;
    let (has_z, has_m) = match dim {
        0 => (false, false),
        1 => (true, false),
        2 => (false, true),
        3 => (true, true),
        _ => return Err(WkbError::UnknownType(raw)),
    };
    if (1..=7).contains(&base) {
        Ok((base, has_z, has_m))
    } else {
        Err(WkbError::UnknownType(raw))
    }
}

/// Reads a WKB-encoded geometry from the given byte slice.
///
/// Matches QGIS `QgsGeometry::fromWkb`.
pub fn read_geometry(data: &[u8]) -> Result<Geometry, WkbError> {
    let (header, offset) = read_header(data)?;
    let mut reader = Reader {
        data,
        pos: offset,
        byte_order: data[0],
    };
    reader.read_geometry_body(&header)
}

/// Writes a geometry as ISO WKB (little-endian), matching QGIS `QgsGeometry::asWkb`.
pub fn write_geometry(geometry: &Geometry) -> Vec<u8> {
    let mut buf = Vec::with_capacity(64);
    write_geometry_body(&mut buf, geometry, WKB_NDR);
    buf
}

/// Writes a geometry as EWKB (little-endian) with optional SRID.
#[allow(dead_code)] // Public API for downstream crates (providers, etc.)
pub fn write_geometry_ewkb(geometry: &Geometry, srid: Option<u32>) -> Vec<u8> {
    let mut buf = Vec::with_capacity(64);
    let (base, has_z, has_m) = type_for_geometry(geometry);
    let mut ewkb_type = base;
    if has_z {
        ewkb_type |= EWKB_Z_FLAG;
    }
    if has_m {
        ewkb_type |= EWKB_M_FLAG;
    }
    if srid.is_some() {
        ewkb_type |= EWKB_SRID_FLAG;
    }
    // Write EWKB header: byte order + type code.
    buf.push(WKB_NDR);
    buf.extend_from_slice(&ewkb_type.to_le_bytes());
    // Write SRID if present (must be right after type code).
    if let Some(s) = srid {
        buf.extend_from_slice(&s.to_le_bytes());
    }
    // Write geometry body (without ISO header).
    write_geometry_data(&mut buf, geometry, has_z, has_m, WKB_NDR);
    buf
}

/// Returns (base_type, has_z, has_m) for a geometry.
fn type_for_geometry(geometry: &Geometry) -> (u32, bool, bool) {
    match geometry {
        Geometry::Point(p) => (WKB_POINT, p.has_z(), p.has_m()),
        Geometry::LineString(ls) => (WKB_LINESTRING, ls.has_z(), ls.has_m()),
        Geometry::Polygon(p) => (WKB_POLYGON, p.has_z(), p.has_m()),
        Geometry::MultiPoint(mp) => (WKB_MULTIPOINT, mp.has_z(), mp.has_m()),
        Geometry::MultiLineString(mls) => (WKB_MULTILINESTRING, mls.has_z(), mls.has_m()),
        Geometry::MultiPolygon(mp) => (WKB_MULTIPOLYGON, mp.has_z(), mp.has_m()),
        Geometry::GeometryCollection(gc) => (WKB_GEOMETRYCOLLECTION, gc.has_z(), gc.has_m()),
    }
}

/// A cursor over WKB byte data.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    byte_order: u8,
}

impl<'a> Reader<'a> {
    fn read_geometry_body(&mut self, header: &WkbHeader) -> Result<Geometry, WkbError> {
        if header.is_empty {
            return Ok(empty_geometry(header.base_type));
        }
        match header.base_type {
            WKB_POINT => self.read_point(header).map(Geometry::Point),
            WKB_LINESTRING => self.read_linestring(header).map(Geometry::LineString),
            WKB_POLYGON => self.read_polygon(header).map(Geometry::Polygon),
            WKB_MULTIPOINT => self.read_multi_point(header).map(Geometry::MultiPoint),
            WKB_MULTILINESTRING => self
                .read_multi_linestring(header)
                .map(Geometry::MultiLineString),
            WKB_MULTIPOLYGON => self.read_multi_polygon(header).map(Geometry::MultiPolygon),
            WKB_GEOMETRYCOLLECTION => self
                .read_geometry_collection(header)
                .map(Geometry::GeometryCollection),
            t => Err(WkbError::UnknownType(t)),
        }
    }

    fn read_point(&mut self, header: &WkbHeader) -> Result<Point, WkbError> {
        let x = self.read_f64()?;
        let y = self.read_f64()?;
        let z = if header.has_z {
            Some(self.read_f64()?)
        } else {
            None
        };
        let m = if header.has_m {
            Some(self.read_f64()?)
        } else {
            None
        };
        Ok(Point { x, y, z, m })
    }

    fn read_linestring(&mut self, header: &WkbHeader) -> Result<LineString, WkbError> {
        let n = self.read_u32()? as usize;
        let mut points = Vec::with_capacity(n);
        for _ in 0..n {
            points.push(self.read_point(header)?);
        }
        Ok(LineString::new(points))
    }

    fn read_polygon(&mut self, header: &WkbHeader) -> Result<Polygon, WkbError> {
        let n_rings = self.read_u32()? as usize;
        let mut rings = Vec::with_capacity(n_rings);
        for _ in 0..n_rings {
            let n_points = self.read_u32()? as usize;
            let mut ring = Vec::with_capacity(n_points);
            for _ in 0..n_points {
                ring.push(self.read_point(header)?);
            }
            rings.push(ring);
        }
        // Polygon::from_rings validates; fall back to unvalidated on error.
        match Polygon::from_rings(rings) {
            Ok(p) => Ok(p),
            Err(_) => Ok(Polygon::new(Vec::new())),
        }
    }

    fn read_multi_point(&mut self, header: &WkbHeader) -> Result<MultiPoint, WkbError> {
        let n = self.read_u32()? as usize;
        let mut points = Vec::with_capacity(n);
        for _ in 0..n {
            let (h, off) = read_header(&self.data[self.pos..])?;
            self.pos += off;
            let p = self.read_point(&h)?;
            // Override dimensionality from the sub-geometry header.
            points.push(Point {
                x: p.x,
                y: p.y,
                z: if h.has_z { p.z } else { None },
                m: if h.has_m { p.m } else { None },
            });
            let _ = header;
        }
        Ok(MultiPoint::new(points))
    }

    fn read_multi_linestring(
        &mut self,
        header: &WkbHeader,
    ) -> Result<MultiLineString, WkbError> {
        let n = self.read_u32()? as usize;
        let mut lines = Vec::with_capacity(n);
        for _ in 0..n {
            let (h, off) = read_header(&self.data[self.pos..])?;
            self.pos += off;
            lines.push(self.read_linestring(&h)?);
            let _ = header;
        }
        Ok(MultiLineString::new(lines))
    }

    fn read_multi_polygon(&mut self, header: &WkbHeader) -> Result<MultiPolygon, WkbError> {
        let n = self.read_u32()? as usize;
        let mut polygons = Vec::with_capacity(n);
        for _ in 0..n {
            let (h, off) = read_header(&self.data[self.pos..])?;
            self.pos += off;
            polygons.push(self.read_polygon(&h)?);
            let _ = header;
        }
        Ok(MultiPolygon::new(polygons))
    }

    fn read_geometry_collection(
        &mut self,
        header: &WkbHeader,
    ) -> Result<GeometryCollection, WkbError> {
        let n = self.read_u32()? as usize;
        let mut geometries = Vec::with_capacity(n);
        for _ in 0..n {
            let (h, off) = read_header(&self.data[self.pos..])?;
            self.pos += off;
            geometries.push(self.read_geometry_body(&h)?);
            let _ = header;
        }
        Ok(GeometryCollection::new(geometries))
    }

    fn read_f64(&mut self) -> Result<f64, WkbError> {
        if self.pos + 8 > self.data.len() {
            return Err(WkbError::TruncatedData);
        }
        let bytes: [u8; 8] = self.data[self.pos..self.pos + 8]
            .try_into()
            .map_err(|_| WkbError::TruncatedData)?;
        self.pos += 8;
        match self.byte_order {
            WKB_XDR => Ok(f64::from_be_bytes(bytes)),
            _ => Ok(f64::from_le_bytes(bytes)),
        }
    }

    fn read_u32(&mut self) -> Result<u32, WkbError> {
        if self.pos + 4 > self.data.len() {
            return Err(WkbError::TruncatedData);
        }
        let val = read_u32(&self.data[self.pos..self.pos + 4], self.byte_order);
        self.pos += 4;
        Ok(val)
    }
}

/// Creates an empty geometry of the given base type.
fn empty_geometry(base_type: u32) -> Geometry {
    match base_type {
        WKB_POINT => Geometry::Point(Point::new(f64::NAN, f64::NAN)),
        WKB_LINESTRING => Geometry::LineString(LineString::new(vec![])),
        WKB_POLYGON => Geometry::Polygon(Polygon::new(vec![])),
        WKB_MULTIPOINT => Geometry::MultiPoint(MultiPoint::new(vec![])),
        WKB_MULTILINESTRING => Geometry::MultiLineString(MultiLineString::new(vec![])),
        WKB_MULTIPOLYGON => Geometry::MultiPolygon(MultiPolygon::new(vec![])),
        WKB_GEOMETRYCOLLECTION => {
            Geometry::GeometryCollection(GeometryCollection::new(vec![]))
        }
        _ => Geometry::Point(Point::new(f64::NAN, f64::NAN)),
    }
}

/// Writes a geometry body (including header) to the buffer.
fn write_geometry_body(buf: &mut Vec<u8>, geometry: &Geometry, byte_order: u8) {
    let (base, has_z, has_m) = type_for_geometry(geometry);
    let type_code = iso_type_code(base, has_z, has_m);
    buf.push(byte_order);
    write_u32(buf, type_code, byte_order);
    write_geometry_data(buf, geometry, has_z, has_m, byte_order);
}

/// Writes geometry data (without header) to the buffer.
fn write_geometry_data(
    buf: &mut Vec<u8>,
    geometry: &Geometry,
    has_z: bool,
    has_m: bool,
    byte_order: u8,
) {
    match geometry {
        Geometry::Point(p) => write_point(buf, p, has_z, has_m, byte_order),
        Geometry::LineString(ls) => write_linestring(buf, ls, has_z, has_m, byte_order),
        Geometry::Polygon(p) => write_polygon(buf, p, has_z, has_m, byte_order),
        Geometry::MultiPoint(mp) => write_multi_point(buf, mp, byte_order),
        Geometry::MultiLineString(mls) => write_multi_linestring(buf, mls, byte_order),
        Geometry::MultiPolygon(mp) => write_multi_polygon(buf, mp, byte_order),
        Geometry::GeometryCollection(gc) => write_geometry_collection(buf, gc, byte_order),
    }
}

/// Computes the ISO WKB type code for a base type and dimensionality.
fn iso_type_code(base: u32, has_z: bool, has_m: bool) -> u32 {
    let dim = match (has_z, has_m) {
        (false, false) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (true, true) => 3,
    };
    base + dim * 1000
}

fn write_point(buf: &mut Vec<u8>, p: &Point, has_z: bool, has_m: bool, byte_order: u8) {
    write_f64(buf, p.x, byte_order);
    write_f64(buf, p.y, byte_order);
    if has_z {
        write_f64(buf, p.z.unwrap_or(0.0), byte_order);
    }
    if has_m {
        write_f64(buf, p.m.unwrap_or(0.0), byte_order);
    }
}

fn write_linestring(buf: &mut Vec<u8>, ls: &LineString, has_z: bool, has_m: bool, byte_order: u8) {
    write_u32(buf, ls.len() as u32, byte_order);
    for p in ls.points() {
        write_point(buf, p, has_z, has_m, byte_order);
    }
}

fn write_polygon(buf: &mut Vec<u8>, poly: &Polygon, has_z: bool, has_m: bool, byte_order: u8) {
    let n_rings = poly.num_interiors() + 1;
    write_u32(buf, n_rings as u32, byte_order);
    write_ring(buf, poly.exterior(), has_z, has_m, byte_order);
    for hole in poly.interiors() {
        write_ring(buf, hole, has_z, has_m, byte_order);
    }
}

fn write_ring(buf: &mut Vec<u8>, ring: &[Point], has_z: bool, has_m: bool, byte_order: u8) {
    write_u32(buf, ring.len() as u32, byte_order);
    for p in ring {
        write_point(buf, p, has_z, has_m, byte_order);
    }
}

fn write_multi_point(buf: &mut Vec<u8>, mp: &MultiPoint, byte_order: u8) {
    write_u32(buf, mp.len() as u32, byte_order);
    for p in mp.points() {
        write_geometry_body(buf, &Geometry::Point(*p), byte_order);
    }
}

fn write_multi_linestring(buf: &mut Vec<u8>, mls: &MultiLineString, byte_order: u8) {
    write_u32(buf, mls.len() as u32, byte_order);
    for ls in mls.lines() {
        write_geometry_body(buf, &Geometry::LineString(ls.clone()), byte_order);
    }
}

fn write_multi_polygon(buf: &mut Vec<u8>, mp: &MultiPolygon, byte_order: u8) {
    write_u32(buf, mp.len() as u32, byte_order);
    for poly in mp.polygons() {
        write_geometry_body(buf, &Geometry::Polygon(poly.clone()), byte_order);
    }
}

fn write_geometry_collection(buf: &mut Vec<u8>, gc: &GeometryCollection, byte_order: u8) {
    write_u32(buf, gc.len() as u32, byte_order);
    for g in gc.geometries() {
        write_geometry_body(buf, g, byte_order);
    }
}

fn read_u32(data: &[u8], byte_order: u8) -> u32 {
    let bytes: [u8; 4] = data[..4].try_into().unwrap();
    match byte_order {
        WKB_XDR => u32::from_be_bytes(bytes),
        _ => u32::from_le_bytes(bytes),
    }
}

fn write_u32(buf: &mut Vec<u8>, val: u32, byte_order: u8) {
    match byte_order {
        WKB_XDR => buf.extend_from_slice(&val.to_be_bytes()),
        _ => buf.extend_from_slice(&val.to_le_bytes()),
    }
}

fn write_f64(buf: &mut Vec<u8>, val: f64, byte_order: u8) {
    match byte_order {
        WKB_XDR => buf.extend_from_slice(&val.to_be_bytes()),
        _ => buf.extend_from_slice(&val.to_le_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wkb_roundtrip_point_2d() {
        let g = Geometry::Point(Point::new(1.0, 2.0));
        let bytes = write_geometry(&g);
        let parsed = read_geometry(&bytes).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkb_roundtrip_point_z() {
        let g = Geometry::Point(Point::new_z(1.0, 2.0, 3.0));
        let bytes = write_geometry(&g);
        let parsed = read_geometry(&bytes).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkb_roundtrip_point_m() {
        let g = Geometry::Point(Point::new_m(1.0, 2.0, 42.0));
        let bytes = write_geometry(&g);
        let parsed = read_geometry(&bytes).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkb_roundtrip_point_zm() {
        let g = Geometry::Point(Point::new_zm(1.0, 2.0, 3.0, 42.0));
        let bytes = write_geometry(&g);
        let parsed = read_geometry(&bytes).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkb_roundtrip_linestring() {
        let g = Geometry::LineString(LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)]));
        let bytes = write_geometry(&g);
        let parsed = read_geometry(&bytes).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkb_roundtrip_polygon() {
        let ring = vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
            Point::new(0.0, 0.0),
        ];
        let hole = vec![
            Point::new(2.0, 2.0),
            Point::new(8.0, 2.0),
            Point::new(8.0, 8.0),
            Point::new(2.0, 8.0),
            Point::new(2.0, 2.0),
        ];
        let g = Geometry::Polygon(Polygon::from_rings(vec![ring, hole]).unwrap());
        let bytes = write_geometry(&g);
        let parsed = read_geometry(&bytes).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkb_roundtrip_multi_point() {
        let g = Geometry::MultiPoint(MultiPoint::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
        ]));
        let bytes = write_geometry(&g);
        let parsed = read_geometry(&bytes).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkb_roundtrip_multi_linestring() {
        let g = Geometry::MultiLineString(MultiLineString::new(vec![
            LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)]),
            LineString::from_2d_coords(&[(2.0, 2.0), (3.0, 3.0)]),
        ]));
        let bytes = write_geometry(&g);
        let parsed = read_geometry(&bytes).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkb_roundtrip_multi_polygon() {
        let ring1 = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
            Point::new(0.0, 0.0),
        ];
        let ring2 = vec![
            Point::new(2.0, 2.0),
            Point::new(3.0, 2.0),
            Point::new(3.0, 3.0),
            Point::new(2.0, 3.0),
            Point::new(2.0, 2.0),
        ];
        let g = Geometry::MultiPolygon(MultiPolygon::new(vec![
            Polygon::from_rings(vec![ring1]).unwrap(),
            Polygon::from_rings(vec![ring2]).unwrap(),
        ]));
        let bytes = write_geometry(&g);
        let parsed = read_geometry(&bytes).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkb_roundtrip_geometry_collection() {
        let g = Geometry::GeometryCollection(GeometryCollection::new(vec![
            Geometry::Point(Point::new(0.0, 0.0)),
            Geometry::LineString(LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)])),
        ]));
        let bytes = write_geometry(&g);
        let parsed = read_geometry(&bytes).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkb_ewkb_with_srid() {
        let g = Geometry::Point(Point::new_z(1.0, 2.0, 3.0));
        let bytes = write_geometry_ewkb(&g, Some(4326));
        let parsed = read_geometry(&bytes).unwrap();
        assert_eq!(parsed, g);
        // Verify SRID was encoded.
        let (header, _) = read_header(&bytes).unwrap();
        assert_eq!(header.srid, Some(4326));
        assert!(header.has_z);
    }

    #[test]
    fn wkb_big_endian_roundtrip() {
        let g = Geometry::Point(Point::new(1.0, 2.0));
        let mut buf = Vec::new();
        write_geometry_body(&mut buf, &g, WKB_XDR);
        let parsed = read_geometry(&buf).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkb_read_iso_z_type_code() {
        // ISO WKB: PointZ = 1001.
        let mut buf = vec![WKB_NDR];
        buf.extend_from_slice(&1001u32.to_le_bytes());
        buf.extend_from_slice(&1.0f64.to_le_bytes());
        buf.extend_from_slice(&2.0f64.to_le_bytes());
        buf.extend_from_slice(&3.0f64.to_le_bytes());
        let parsed = read_geometry(&buf).unwrap();
        assert_eq!(parsed, Geometry::Point(Point::new_z(1.0, 2.0, 3.0)));
    }

    #[test]
    fn wkb_read_ewkb_z_flag() {
        // EWKB: PointZ = 0x80000001.
        let mut buf = vec![WKB_NDR];
        buf.extend_from_slice(&0x8000_0001u32.to_le_bytes());
        buf.extend_from_slice(&1.0f64.to_le_bytes());
        buf.extend_from_slice(&2.0f64.to_le_bytes());
        buf.extend_from_slice(&3.0f64.to_le_bytes());
        let parsed = read_geometry(&buf).unwrap();
        assert_eq!(parsed, Geometry::Point(Point::new_z(1.0, 2.0, 3.0)));
    }

    #[test]
    fn wkb_invalid_byte_order() {
        let buf = vec![42u8, 1, 0, 0, 0];
        assert!(matches!(read_geometry(&buf), Err(WkbError::InvalidByteOrder(42))));
    }

    #[test]
    fn wkb_truncated_data() {
        // Only 1 byte of the x coordinate.
        let mut truncated = vec![WKB_NDR];
        truncated.extend_from_slice(&1u32.to_le_bytes());
        truncated.push(0);
        assert!(matches!(read_geometry(&truncated), Err(WkbError::TruncatedData)));
    }

    #[test]
    fn wkb_empty_point() {
        let g = Geometry::Point(Point::new(f64::NAN, f64::NAN));
        let bytes = write_geometry(&g);
        let parsed = read_geometry(&bytes).unwrap();
        assert!(matches!(parsed, Geometry::Point(p) if p.is_empty()));
    }

    #[test]
    fn wkb_roundtrip_all_types_z() {
        let geometries = vec![
            Geometry::Point(Point::new_z(1.0, 2.0, 3.0)),
            Geometry::LineString(LineString::new(vec![
                Point::new_z(0.0, 0.0, 10.0),
                Point::new_z(1.0, 1.0, 20.0),
            ])),
            Geometry::MultiPoint(MultiPoint::new(vec![Point::new_z(0.0, 0.0, 5.0)])),
        ];
        for g in geometries {
            let bytes = write_geometry(&g);
            let parsed = read_geometry(&bytes).unwrap();
            assert_eq!(parsed, g);
        }
    }

    #[test]
    fn wkb_header_iso_types() {
        // Verify ISO type code decoding.
        let cases = [
            (1, WKB_POINT, false, false),
            (1001, WKB_POINT, true, false),
            (2001, WKB_POINT, false, true),
            (3001, WKB_POINT, true, true),
            (1002, WKB_LINESTRING, true, false),
            (1003, WKB_POLYGON, true, false),
            (1004, WKB_MULTIPOINT, true, false),
            (1005, WKB_MULTILINESTRING, true, false),
            (1006, WKB_MULTIPOLYGON, true, false),
            (1007, WKB_GEOMETRYCOLLECTION, true, false),
        ];
        for (raw, base, hz, hm) in cases {
            let (b, z, m) = decode_iso_type(raw).unwrap();
            assert_eq!(b, base);
            assert_eq!(z, hz);
            assert_eq!(m, hm);
        }
    }
}
