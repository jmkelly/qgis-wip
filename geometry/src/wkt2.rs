//! WKT2:2019 geometry reader and writer.
//!
//! Matches QGIS `QgsGeometry::fromWkt` with `Qgis::WktVariant::WKT2_2019` and
//! `QgsGeometry::asWkt` with the same variant.
//! Supports Point, LineString, Polygon, MultiPoint, MultiLineString,
//! MultiPolygon, GeometryCollection — all with Z/M variants.
//!
//! WKT2:2019 syntax differences from WKT1:
//! * Geometry type keywords are the same (POINT, LINESTRING, etc.)
//! * Z/M suffixes are the same (Z, M, ZM)
//! * EMPTY keyword is the same
//! * The main difference is in CRS WKT2, not geometry WKT2 — geometry
//!   WKT2:2019 uses the same syntax as WKT1 for the geometry portion.
//!
//! Pure Rust, zero unsafe.

use crate::collection::{Geometry, GeometryCollection};
use crate::linestring::LineString;
use crate::multi::{MultiLineString, MultiPoint, MultiPolygon};
use crate::point::Point;
use crate::polygon::Polygon;

/// Errors that can occur while parsing or writing WKT2:2019.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wkt2Error {
    /// Input is empty or contains only whitespace.
    EmptyInput,
    /// Unexpected end of input.
    UnexpectedEof,
    /// Expected a specific token but found something else.
    ExpectedToken { expected: String, found: String },
    /// Invalid number literal.
    InvalidNumber(String),
    /// Unknown geometry type keyword.
    UnknownType(String),
    /// Unbalanced parentheses.
    UnbalancedParens,
    /// Trailing characters after the geometry.
    TrailingCharacters(String),
    /// Invalid EMPTY geometry specification.
    InvalidEmpty(String),
}

impl std::fmt::Display for Wkt2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "empty WKT2 input"),
            Self::UnexpectedEof => write!(f, "unexpected end of WKT2 input"),
            Self::ExpectedToken { expected, found } => {
                write!(f, "expected '{expected}', found '{found}'")
            }
            Self::InvalidNumber(s) => write!(f, "invalid number: {s}"),
            Self::UnknownType(s) => write!(f, "unknown WKT2 geometry type: {s}"),
            Self::UnbalancedParens => write!(f, "unbalanced parentheses in WKT2"),
            Self::TrailingCharacters(s) => write!(f, "trailing characters: {s}"),
            Self::InvalidEmpty(s) => write!(f, "invalid EMPTY geometry: {s}"),
        }
    }
}

impl std::error::Error for Wkt2Error {}

/// A WKT2:2019 parser using recursive descent.
pub struct Wkt2Parser {
    input: Vec<char>,
    pos: usize,
}

impl Wkt2Parser {
    /// Creates a new WKT2:2019 parser for the given input string.
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    /// Parses a WKT2:2019 geometry from the input string.
    pub fn parse(&mut self) -> Result<Geometry, Wkt2Error> {
        self.skip_whitespace();
        if self.pos >= self.input.len() {
            return Err(Wkt2Error::EmptyInput);
        }
        let geom = self.parse_geometry()?;
        self.skip_whitespace();
        if self.pos < self.input.len() {
            let remaining: String = self.input[self.pos..].iter().collect();
            return Err(Wkt2Error::TrailingCharacters(remaining));
        }
        Ok(geom)
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn parse_geometry(&mut self) -> Result<Geometry, Wkt2Error> {
        let type_str = self.read_keyword()?;
        let type_upper = type_str.to_uppercase();

        self.skip_whitespace();
        if self.peek().is_none() {
            return Err(Wkt2Error::UnexpectedEof);
        }

        // Check for Z/M/ZM suffix
        let mut has_z = false;
        let mut has_m = false;
        if self.peek() == Some('Z') {
            self.advance();
            has_z = true;
            self.skip_whitespace();
            if self.peek() == Some('M') {
                self.advance();
                has_m = true;
                self.skip_whitespace();
            }
        } else if self.peek() == Some('M') {
            self.advance();
            has_m = true;
            self.skip_whitespace();
        }

        // Check for EMPTY keyword
        self.skip_whitespace();
        if self.pos + 5 <= self.input.len() {
            let maybe_empty: String = self.input[self.pos..self.pos + 5].iter().collect();
            if maybe_empty.eq_ignore_ascii_case("EMPTY") {
                self.pos += 5;
                return Ok(empty_geometry(&type_upper));
            }
        }

        match type_upper.as_str() {
            "POINT" => self.parse_point(has_z, has_m).map(Geometry::Point),
            "LINESTRING" => self.parse_linestring(has_z, has_m).map(Geometry::LineString),
            "POLYGON" => self.parse_polygon(has_z, has_m).map(Geometry::Polygon),
            "MULTIPOINT" => self.parse_multi_point(has_z, has_m).map(Geometry::MultiPoint),
            "MULTILINESTRING" => self
                .parse_multi_linestring(has_z, has_m)
                .map(Geometry::MultiLineString),
            "MULTIPOLYGON" => self
                .parse_multi_polygon(has_z, has_m)
                .map(Geometry::MultiPolygon),
            "GEOMETRYCOLLECTION" => self
                .parse_geometry_collection()
                .map(Geometry::GeometryCollection),
            _ => Err(Wkt2Error::UnknownType(type_str)),
        }
    }

    fn read_keyword(&mut self) -> Result<String, Wkt2Error> {
        self.skip_whitespace();
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphabetic() {
                self.advance();
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(Wkt2Error::UnexpectedEof);
        }
        Ok(self.input[start..self.pos].iter().collect())
    }

    fn expect_char(&mut self, expected: char) -> Result<(), Wkt2Error> {
        self.skip_whitespace();
        match self.advance() {
            Some(c) if c == expected => Ok(()),
            Some(c) => Err(Wkt2Error::ExpectedToken {
                expected: expected.to_string(),
                found: c.to_string(),
            }),
            None => Err(Wkt2Error::UnexpectedEof),
        }
    }

    fn parse_number(&mut self) -> Result<f64, Wkt2Error> {
        self.skip_whitespace();
        let start = self.pos;
        if let Some(ch) = self.peek() {
            if ch == '-' || ch == '+' {
                self.advance();
            }
        }
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() || ch == '.' || ch == 'e' || ch == 'E' {
                self.advance();
            } else if ch == '-' || ch == '+' {
                if self.pos > 0 && (self.input[self.pos - 1] == 'e' || self.input[self.pos - 1] == 'E')
                {
                    self.advance();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        let num_str: String = self.input[start..self.pos].iter().collect();
        num_str
            .parse::<f64>()
            .map_err(|_| Wkt2Error::InvalidNumber(num_str))
    }

    fn parse_point(&mut self, has_z: bool, has_m: bool) -> Result<Point, Wkt2Error> {
        self.expect_char('(')?;
        let x = self.parse_number()?;
        let y = self.parse_number()?;
        let z = if has_z { Some(self.parse_number()?) } else { None };
        let m = if has_m { Some(self.parse_number()?) } else { None };
        self.expect_char(')')?;
        Ok(Point { x, y, z, m })
    }

    fn parse_linestring(&mut self, has_z: bool, has_m: bool) -> Result<LineString, Wkt2Error> {
        self.expect_char('(')?;
        let mut points = Vec::new();
        loop {
            points.push(self.parse_point_inner(has_z, has_m)?);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.advance();
                }
                Some(')') => {
                    self.advance();
                    break;
                }
                _ => {
                    return Err(Wkt2Error::ExpectedToken {
                        expected: ", or )".to_string(),
                        found: self.peek().map(|c| c.to_string()).unwrap_or_default(),
                    });
                }
            }
        }
        Ok(LineString::new(points))
    }

    fn parse_point_inner(&mut self, has_z: bool, has_m: bool) -> Result<Point, Wkt2Error> {
        let x = self.parse_number()?;
        let y = self.parse_number()?;
        let z = if has_z { Some(self.parse_number()?) } else { None };
        let m = if has_m { Some(self.parse_number()?) } else { None };
        Ok(Point { x, y, z, m })
    }

    fn parse_polygon(&mut self, has_z: bool, has_m: bool) -> Result<Polygon, Wkt2Error> {
        self.expect_char('(')?;
        let mut rings = Vec::new();
        loop {
            rings.push(self.parse_ring(has_z, has_m)?);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.advance();
                }
                Some(')') => {
                    self.advance();
                    break;
                }
                _ => {
                    return Err(Wkt2Error::ExpectedToken {
                        expected: ", or )".to_string(),
                        found: self.peek().map(|c| c.to_string()).unwrap_or_default(),
                    });
                }
            }
        }
        match Polygon::from_rings(rings) {
            Ok(p) => Ok(p),
            Err(_) => Ok(Polygon::new(Vec::new())),
        }
    }

    fn parse_ring(&mut self, has_z: bool, has_m: bool) -> Result<Vec<Point>, Wkt2Error> {
        self.expect_char('(')?;
        let mut points = Vec::new();
        loop {
            points.push(self.parse_point_inner(has_z, has_m)?);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.advance();
                }
                Some(')') => {
                    self.advance();
                    break;
                }
                _ => {
                    return Err(Wkt2Error::ExpectedToken {
                        expected: ", or )".to_string(),
                        found: self.peek().map(|c| c.to_string()).unwrap_or_default(),
                    });
                }
            }
        }
        Ok(points)
    }

    fn parse_multi_point(&mut self, has_z: bool, has_m: bool) -> Result<MultiPoint, Wkt2Error> {
        self.expect_char('(')?;
        let mut points = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some('(') => {
                    self.advance();
                    let p = self.parse_point_inner(has_z, has_m)?;
                    self.expect_char(')')?;
                    points.push(p);
                }
                Some(c) if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' => {
                    let p = self.parse_point_inner(has_z, has_m)?;
                    points.push(p);
                }
                _ => {
                    return Err(Wkt2Error::ExpectedToken {
                        expected: "point".to_string(),
                        found: self.peek().map(|c| c.to_string()).unwrap_or_default(),
                    });
                }
            }
            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.advance();
                }
                Some(')') => {
                    self.advance();
                    break;
                }
                _ => {
                    return Err(Wkt2Error::ExpectedToken {
                        expected: ", or )".to_string(),
                        found: self.peek().map(|c| c.to_string()).unwrap_or_default(),
                    });
                }
            }
        }
        Ok(MultiPoint::new(points))
    }

    fn parse_multi_linestring(
        &mut self,
        has_z: bool,
        has_m: bool,
    ) -> Result<MultiLineString, Wkt2Error> {
        self.expect_char('(')?;
        let mut lines = Vec::new();
        loop {
            lines.push(self.parse_linestring(has_z, has_m)?);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.advance();
                }
                Some(')') => {
                    self.advance();
                    break;
                }
                _ => {
                    return Err(Wkt2Error::ExpectedToken {
                        expected: ", or )".to_string(),
                        found: self.peek().map(|c| c.to_string()).unwrap_or_default(),
                    });
                }
            }
        }
        Ok(MultiLineString::new(lines))
    }

    fn parse_multi_polygon(
        &mut self,
        has_z: bool,
        has_m: bool,
    ) -> Result<MultiPolygon, Wkt2Error> {
        self.expect_char('(')?;
        let mut polygons = Vec::new();
        loop {
            polygons.push(self.parse_polygon(has_z, has_m)?);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.advance();
                }
                Some(')') => {
                    self.advance();
                    break;
                }
                _ => {
                    return Err(Wkt2Error::ExpectedToken {
                        expected: ", or )".to_string(),
                        found: self.peek().map(|c| c.to_string()).unwrap_or_default(),
                    });
                }
            }
        }
        Ok(MultiPolygon::new(polygons))
    }

    fn parse_geometry_collection(&mut self) -> Result<GeometryCollection, Wkt2Error> {
        self.expect_char('(')?;
        let mut geometries = Vec::new();
        loop {
            self.skip_whitespace();
            if self.peek() == Some(')') && geometries.is_empty() {
                self.advance();
                break;
            }
            geometries.push(self.parse_geometry()?);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.advance();
                }
                Some(')') => {
                    self.advance();
                    break;
                }
                _ => {
                    return Err(Wkt2Error::ExpectedToken {
                        expected: ", or )".to_string(),
                        found: self.peek().map(|c| c.to_string()).unwrap_or_default(),
                    });
                }
            }
        }
        Ok(GeometryCollection::new(geometries))
    }
}

/// Creates an empty geometry of the given type.
fn empty_geometry(type_: &str) -> Geometry {
    match type_ {
        "POINT" => Geometry::Point(Point::new(f64::NAN, f64::NAN)),
        "LINESTRING" => Geometry::LineString(LineString::new(vec![])),
        "POLYGON" => Geometry::Polygon(Polygon::new(vec![])),
        "MULTIPOINT" => Geometry::MultiPoint(MultiPoint::new(vec![])),
        "MULTILINESTRING" => Geometry::MultiLineString(MultiLineString::new(vec![])),
        "MULTIPOLYGON" => Geometry::MultiPolygon(MultiPolygon::new(vec![])),
        "GEOMETRYCOLLECTION" => {
            Geometry::GeometryCollection(GeometryCollection::new(vec![]))
        }
        _ => Geometry::Point(Point::new(f64::NAN, f64::NAN)),
    }
}

/// Parses a WKT2:2019 string into a [`Geometry`].
///
/// Matches QGIS `QgsGeometry::fromWkt` with `Qgis::WktVariant::WKT2_2019`.
pub fn read_geometry(input: &str) -> Result<Geometry, Wkt2Error> {
    let mut parser = Wkt2Parser::new(input);
    parser.parse()
}

/// Writes a [`Geometry`] as a WKT2:2019 string.
///
/// Matches QGIS `QgsGeometry::asWkt` with `Qgis::WktVariant::WKT2_2019`.
pub fn write_geometry(geometry: &Geometry) -> String {
    let mut buf = String::with_capacity(128);
    write_geometry_to(geometry, &mut buf);
    buf
}

fn write_geometry_to(geometry: &Geometry, buf: &mut String) {
    match geometry {
        Geometry::Point(p) => write_point(p, buf),
        Geometry::LineString(ls) => write_linestring(ls, buf),
        Geometry::Polygon(p) => write_polygon(p, buf),
        Geometry::MultiPoint(mp) => write_multi_point(mp, buf),
        Geometry::MultiLineString(mls) => write_multi_linestring(mls, buf),
        Geometry::MultiPolygon(mp) => write_multi_polygon(mp, buf),
        Geometry::GeometryCollection(gc) => write_geometry_collection(gc, buf),
    }
}

fn write_point(p: &Point, buf: &mut String) {
    buf.push_str("POINT");
    write_dimension_suffix(p.has_z(), p.has_m(), buf);
    if p.is_empty() {
        buf.push_str(" EMPTY");
    } else {
        buf.push(' ');
        write_point_inner(p, buf);
    }
}

fn write_point_inner(p: &Point, buf: &mut String) {
    buf.push('(');
    write_point_coords(p, buf);
    buf.push(')');
}

/// Writes point coordinates without surrounding parentheses.
fn write_point_coords(p: &Point, buf: &mut String) {
    write_f64(p.x, buf);
    buf.push(' ');
    write_f64(p.y, buf);
    if let Some(z) = p.z {
        buf.push(' ');
        write_f64(z, buf);
    }
    if let Some(m) = p.m {
        buf.push(' ');
        write_f64(m, buf);
    }
}

fn write_linestring(ls: &LineString, buf: &mut String) {
    buf.push_str("LINESTRING");
    write_dimension_suffix(ls.has_z(), ls.has_m(), buf);
    if ls.is_empty() {
        buf.push_str(" EMPTY");
    } else {
        buf.push(' ');
        write_linestring_inner(ls, buf);
    }
}

fn write_linestring_inner(ls: &LineString, buf: &mut String) {
    buf.push('(');
    for (i, p) in ls.points().iter().enumerate() {
        if i > 0 {
            buf.push_str(", ");
        }
        write_point_coords(p, buf);
    }
    buf.push(')');
}

fn write_polygon(poly: &Polygon, buf: &mut String) {
    buf.push_str("POLYGON");
    write_dimension_suffix(poly.has_z(), poly.has_m(), buf);
    if poly.is_empty() {
        buf.push_str(" EMPTY");
    } else {
        buf.push(' ');
        write_polygon_inner(poly, buf);
    }
}

fn write_polygon_inner(poly: &Polygon, buf: &mut String) {
    buf.push('(');
    write_ring(poly.exterior(), buf);
    for hole in poly.interiors() {
        buf.push_str(", ");
        write_ring(hole, buf);
    }
    buf.push(')');
}

fn write_ring(ring: &[Point], buf: &mut String) {
    buf.push('(');
    for (i, p) in ring.iter().enumerate() {
        if i > 0 {
            buf.push_str(", ");
        }
        write_point_coords(p, buf);
    }
    buf.push(')');
}

fn write_multi_point(mp: &MultiPoint, buf: &mut String) {
    buf.push_str("MULTIPOINT");
    write_dimension_suffix(mp.has_z(), mp.has_m(), buf);
    if mp.is_empty() {
        buf.push_str(" EMPTY");
    } else {
        buf.push(' ');
        buf.push('(');
        for (i, p) in mp.points().iter().enumerate() {
            if i > 0 {
                buf.push_str(", ");
            }
            write_point_inner(p, buf);
        }
        buf.push(')');
    }
}

fn write_multi_linestring(mls: &MultiLineString, buf: &mut String) {
    buf.push_str("MULTILINESTRING");
    write_dimension_suffix(mls.has_z(), mls.has_m(), buf);
    if mls.is_empty() {
        buf.push_str(" EMPTY");
    } else {
        buf.push(' ');
        buf.push('(');
        for (i, ls) in mls.lines().iter().enumerate() {
            if i > 0 {
                buf.push_str(", ");
            }
            write_linestring_inner(ls, buf);
        }
        buf.push(')');
    }
}

fn write_multi_polygon(mp: &MultiPolygon, buf: &mut String) {
    buf.push_str("MULTIPOLYGON");
    write_dimension_suffix(mp.has_z(), mp.has_m(), buf);
    if mp.is_empty() {
        buf.push_str(" EMPTY");
    } else {
        buf.push(' ');
        buf.push('(');
        for (i, poly) in mp.polygons().iter().enumerate() {
            if i > 0 {
                buf.push_str(", ");
            }
            write_polygon_inner(poly, buf);
        }
        buf.push(')');
    }
}

fn write_geometry_collection(gc: &GeometryCollection, buf: &mut String) {
    buf.push_str("GEOMETRYCOLLECTION");
    if gc.is_empty() {
        buf.push_str(" EMPTY");
    } else {
        buf.push(' ');
        buf.push('(');
        for (i, g) in gc.geometries().iter().enumerate() {
            if i > 0 {
                buf.push_str(", ");
            }
            write_geometry_to(g, buf);
        }
        buf.push(')');
    }
}

fn write_dimension_suffix(has_z: bool, has_m: bool, buf: &mut String) {
    match (has_z, has_m) {
        (true, true) => buf.push_str(" ZM"),
        (true, false) => buf.push_str(" Z"),
        (false, true) => buf.push_str(" M"),
        (false, false) => {}
    }
}

/// Formats an f64 for WKT2 output, matching QGIS behavior.
fn write_f64(val: f64, buf: &mut String) {
    if val.is_nan() {
        buf.push_str("nan");
    } else if val.is_infinite() {
        if val > 0.0 {
            buf.push_str("inf");
        } else {
            buf.push_str("-inf");
        }
    } else {
        let s = format!("{val:.15}");
        let s = if s.contains('.') {
            let trimmed = s.trim_end_matches('0');
            trimmed.trim_end_matches('.')
        } else {
            &s
        };
        buf.push_str(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wkt2_roundtrip_point_2d() {
        let g = Geometry::Point(Point::new(1.0, 2.0));
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "POINT (1 2)");
        let parsed = read_geometry(&wkt).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkt2_roundtrip_point_z() {
        let g = Geometry::Point(Point::new_z(1.0, 2.0, 3.0));
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "POINT Z (1 2 3)");
        let parsed = read_geometry(&wkt).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkt2_roundtrip_point_m() {
        let g = Geometry::Point(Point::new_m(1.0, 2.0, 42.0));
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "POINT M (1 2 42)");
        let parsed = read_geometry(&wkt).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkt2_roundtrip_point_zm() {
        let g = Geometry::Point(Point::new_zm(1.0, 2.0, 3.0, 42.0));
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "POINT ZM (1 2 3 42)");
        let parsed = read_geometry(&wkt).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkt2_roundtrip_linestring() {
        let g = Geometry::LineString(LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)]));
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "LINESTRING (0 0, 1 1, 2 0)");
        let parsed = read_geometry(&wkt).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkt2_roundtrip_polygon() {
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
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "POLYGON ((0 0, 10 0, 10 10, 0 10, 0 0), (2 2, 8 2, 8 8, 2 8, 2 2))");
        let parsed = read_geometry(&wkt).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkt2_roundtrip_multi_point() {
        let g = Geometry::MultiPoint(MultiPoint::new(vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
        ]));
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "MULTIPOINT ((0 0), (1 1))");
        let parsed = read_geometry(&wkt).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkt2_roundtrip_multi_linestring() {
        let g = Geometry::MultiLineString(MultiLineString::new(vec![
            LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)]),
            LineString::from_2d_coords(&[(2.0, 2.0), (3.0, 3.0)]),
        ]));
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "MULTILINESTRING ((0 0, 1 1), (2 2, 3 3))");
        let parsed = read_geometry(&wkt).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkt2_roundtrip_multi_polygon() {
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
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "MULTIPOLYGON (((0 0, 1 0, 1 1, 0 1, 0 0)), ((2 2, 3 2, 3 3, 2 3, 2 2)))");
        let parsed = read_geometry(&wkt).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkt2_roundtrip_geometry_collection() {
        let g = Geometry::GeometryCollection(GeometryCollection::new(vec![
            Geometry::Point(Point::new(0.0, 0.0)),
            Geometry::LineString(LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)])),
        ]));
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "GEOMETRYCOLLECTION (POINT (0 0), LINESTRING (0 0, 1 1))");
        let parsed = read_geometry(&wkt).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkt2_empty_point() {
        let g = Geometry::Point(Point::new(f64::NAN, f64::NAN));
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "POINT EMPTY");
        let parsed = read_geometry(&wkt).unwrap();
        assert!(matches!(parsed, Geometry::Point(p) if p.is_empty()));
    }

    #[test]
    fn wkt2_empty_linestring() {
        let g = Geometry::LineString(LineString::new(vec![]));
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "LINESTRING EMPTY");
        let parsed = read_geometry(&wkt).unwrap();
        assert!(matches!(parsed, Geometry::LineString(ls) if ls.is_empty()));
    }

    #[test]
    fn wkt2_empty_polygon() {
        let g = Geometry::Polygon(Polygon::new(vec![]));
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "POLYGON EMPTY");
        let parsed = read_geometry(&wkt).unwrap();
        assert!(matches!(parsed, Geometry::Polygon(p) if p.is_empty()));
    }

    #[test]
    fn wkt2_empty_multi_point() {
        let g = Geometry::MultiPoint(MultiPoint::new(vec![]));
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "MULTIPOINT EMPTY");
        let parsed = read_geometry(&wkt).unwrap();
        assert!(matches!(parsed, Geometry::MultiPoint(mp) if mp.is_empty()));
    }

    #[test]
    fn wkt2_empty_geometry_collection() {
        let g = Geometry::GeometryCollection(GeometryCollection::new(vec![]));
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "GEOMETRYCOLLECTION EMPTY");
        let parsed = read_geometry(&wkt).unwrap();
        assert!(matches!(parsed, Geometry::GeometryCollection(gc) if gc.is_empty()));
    }

    #[test]
    fn wkt2_parse_point_with_extra_whitespace() {
        let wkt = "  POINT   (  1   2  )  ";
        let parsed = read_geometry(wkt).unwrap();
        assert_eq!(parsed, Geometry::Point(Point::new(1.0, 2.0)));
    }

    #[test]
    fn wkt2_parse_case_insensitive() {
        let wkt = "point (1 2)";
        let parsed = read_geometry(wkt).unwrap();
        assert_eq!(parsed, Geometry::Point(Point::new(1.0, 2.0)));
    }

    #[test]
    fn wkt2_parse_negative_coordinates() {
        let wkt = "POINT (-1.5 -2.5)";
        let parsed = read_geometry(wkt).unwrap();
        assert_eq!(parsed, Geometry::Point(Point::new(-1.5, -2.5)));
    }

    #[test]
    fn wkt2_parse_scientific_notation() {
        let wkt = "POINT (1e2 2.5e-1)";
        let parsed = read_geometry(wkt).unwrap();
        assert_eq!(parsed, Geometry::Point(Point::new(100.0, 0.25)));
    }

    #[test]
    fn wkt2_error_empty_input() {
        assert!(matches!(read_geometry(""), Err(Wkt2Error::EmptyInput)));
        assert!(matches!(read_geometry("   "), Err(Wkt2Error::EmptyInput)));
    }

    #[test]
    fn wkt2_error_unknown_type() {
        assert!(matches!(
            read_geometry("CIRCLE (1 2)"),
            Err(Wkt2Error::UnknownType(_))
        ));
    }

    #[test]
    fn wkt2_error_unbalanced_parens() {
        assert!(read_geometry("POINT (1 2").is_err());
        assert!(read_geometry("POINT 1 2)").is_err());
    }

    #[test]
    fn wkt2_error_trailing_characters() {
        assert!(matches!(
            read_geometry("POINT (1 2) extra"),
            Err(Wkt2Error::TrailingCharacters(_))
        ));
    }

    #[test]
    fn wkt2_error_invalid_number() {
        assert!(matches!(
            read_geometry("POINT (abc 2)"),
            Err(Wkt2Error::InvalidNumber(_))
        ));
    }

    #[test]
    fn wkt2_roundtrip_all_types() {
        let geometries = vec![
            Geometry::Point(Point::new(1.0, 2.0)),
            Geometry::Point(Point::new_z(1.0, 2.0, 3.0)),
            Geometry::Point(Point::new_m(1.0, 2.0, 42.0)),
            Geometry::Point(Point::new_zm(1.0, 2.0, 3.0, 42.0)),
            Geometry::LineString(LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)])),
            Geometry::LineString(LineString::new(vec![
                Point::new_z(0.0, 0.0, 10.0),
                Point::new_z(1.0, 1.0, 20.0),
            ])),
            Geometry::MultiPoint(MultiPoint::new(vec![Point::new(0.0, 0.0)])),
            Geometry::MultiLineString(MultiLineString::new(vec![LineString::from_2d_coords(&[(0.0, 0.0), (1.0, 1.0)])])),
        ];
        for g in geometries {
            let wkt = write_geometry(&g);
            let parsed = read_geometry(&wkt).unwrap();
            assert_eq!(parsed, g, "roundtrip failed for {wkt}");
        }
    }

    #[test]
    fn wkt2_f64_formatting() {
        let mut buf = String::new();
        write_f64(1.0, &mut buf);
        assert_eq!(buf, "1");
        buf.clear();
        write_f64(1.5, &mut buf);
        assert_eq!(buf, "1.5");
        buf.clear();
        write_f64(0.1 + 0.2, &mut buf);
        assert_eq!(buf, "0.3");
        buf.clear();
        write_f64(-0.0, &mut buf);
        assert_eq!(buf, "-0");
    }

    #[test]
    fn wkt2_multi_point_with_z() {
        let g = Geometry::MultiPoint(MultiPoint::new(vec![
            Point::new_z(0.0, 0.0, 5.0),
            Point::new_z(1.0, 1.0, 10.0),
        ]));
        let wkt = write_geometry(&g);
        assert_eq!(wkt, "MULTIPOINT Z ((0 0 5), (1 1 10))");
        let parsed = read_geometry(&wkt).unwrap();
        assert_eq!(parsed, g);
    }

    #[test]
    fn wkt2_nested_geometry_collection() {
        let inner = GeometryCollection::new(vec![Geometry::Point(Point::new(0.0, 0.0))]);
        let outer = GeometryCollection::new(vec![Geometry::collection(inner)]);
        let wkt = write_geometry(&Geometry::GeometryCollection(outer));
        assert_eq!(wkt, "GEOMETRYCOLLECTION (GEOMETRYCOLLECTION (POINT (0 0)))");
        let parsed = read_geometry(&wkt).unwrap();
        assert!(matches!(parsed, Geometry::GeometryCollection(_)));
    }
}
