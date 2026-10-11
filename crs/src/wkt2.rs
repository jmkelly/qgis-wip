//! WKT2:2019 parser for the constrained subset QGIS emits.
//!
//! Hand-written recursive-descent parser covering GEOGCRS, PROJCRS, GEODCRS,
//! VERTCRS, COMPD_CS, DATUM, ELLIPSOID, ENSEMBLE, PRIMEM, CS, AXIS,
//! CONVERSION, METHOD, PARAMETER, UNIT, ANGLEUNIT, LENGTHUNIT, SCALEUNIT,
//! ID, and BASEGEOGCRS nodes.
//!
//! Pure Rust, zero unsafe. Returns a [`ParsedCrs`] that callers can convert
//! into a `proj4rs::Proj` or compare against `crs-definitions` output.

#![forbid(unsafe_code)]

use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur while parsing WKT2:2019.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Wkt2Error {
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("expected '{token}' at position {pos}")]
    ExpectedToken { token: String, pos: usize },
    #[error("unexpected character {ch:?} at position {pos}")]
    UnexpectedChar { ch: char, pos: usize },
    #[error("invalid number {value:?} at position {pos}")]
    InvalidNumber { value: String, pos: usize },
    #[error("unknown keyword {keyword:?} at position {pos}")]
    UnknownKeyword { keyword: String, pos: usize },
    #[error("unsupported CRS type {crs_type:?}")]
    UnsupportedCrsType { crs_type: String },
    #[error("missing required node {node:?}")]
    MissingNode { node: String },
    #[error("unbalanced brackets at position {pos}")]
    UnbalancedBrackets { pos: usize },
}

/// A parsed WKT2:2019 CRS representation.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCrs {
    /// The CRS type keyword (GEOGCRS, PROJCRS, GEODCRS, VERTCRS, COMPD_CS).
    pub crs_type: String,
    /// The CRS name.
    pub name: String,
    /// Optional authority ID (e.g., EPSG:4326).
    pub id: Option<AuthorityId>,
    /// Datum information (for geodetic/projected CRS).
    pub datum: Option<Datum>,
    /// Ellipsoid information.
    pub ellipsoid: Option<Ellipsoid>,
    /// Prime meridian information.
    pub prime_meridian: Option<PrimeMeridian>,
    /// Coordinate system definition.
    pub cs: Option<CoordinateSystem>,
    /// Conversion method (for projected CRS).
    pub conversion: Option<Conversion>,
    /// Child CRS components (for COMPD_CS).
    pub components: Vec<ParsedCrs>,
    /// Axis information.
    pub axes: Vec<Axis>,
    /// Unit definitions.
    pub units: Vec<UnitDef>,
}

/// Authority identifier (e.g., EPSG:4326).
#[derive(Debug, Clone, PartialEq)]
pub struct AuthorityId {
    pub authority: String,
    pub code: String,
}

/// Datum definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Datum {
    pub name: String,
    pub ellipsoid: Option<Ellipsoid>,
    pub ensemble: Option<DatumEnsemble>,
}

/// Datum ensemble definition.
#[derive(Debug, Clone, PartialEq)]
pub struct DatumEnsemble {
    pub name: String,
    pub members: Vec<EnsembleMember>,
    pub accuracy: f64,
}

/// Ensemble member.
#[derive(Debug, Clone, PartialEq)]
pub struct EnsembleMember {
    pub name: String,
    pub id: Option<AuthorityId>,
}

/// Ellipsoid definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Ellipsoid {
    pub name: String,
    pub semi_major_axis: f64,
    pub inverse_flattening: f64,
    pub id: Option<AuthorityId>,
}

/// Prime meridian definition.
#[derive(Debug, Clone, PartialEq)]
pub struct PrimeMeridian {
    pub name: String,
    pub longitude: f64,
    pub id: Option<AuthorityId>,
}

/// Coordinate system definition.
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinateSystem {
    pub type_: String,
    pub axes: Vec<Axis>,
}

/// Axis definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    pub name: String,
    pub abbreviation: String,
    pub direction: String,
    pub unit: String,
}

/// Conversion (projection) definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversion {
    pub name: String,
    pub method: Method,
    pub parameters: Vec<Parameter>,
}

/// Projection method.
#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    pub name: String,
    pub id: Option<AuthorityId>,
}

/// Projection parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub value: f64,
    pub id: Option<AuthorityId>,
}

/// Unit definition.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitDef {
    pub type_: UnitType,
    pub name: String,
    pub conversion_factor: f64,
    pub id: Option<AuthorityId>,
}

/// Unit type classification.
#[derive(Debug, Clone, PartialEq)]
pub enum UnitType {
    Length,
    Angle,
    Scale,
    Time,
    Unknown,
}

/// Token types for the WKT2 lexer.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(String),
    Number(f64),
    StringLiteral(String),
    Comma,
    OpenBracket,
    CloseBracket,
}

/// Tokenizer for WKT2:2019 strings.
<<<<<<< HEAD
=======
///
/// Owns the input as a `Vec<char>` so the parser can hold it without
/// lifetime issues.
>>>>>>> qgis-qxu-5
struct Lexer {
    input: Vec<char>,
    pos: usize,
}

<<<<<<< HEAD
impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        let chars: &'a [char] = Box::leak(input.chars().collect::<Vec<_>>().into_boxed_slice());
        Self {
            input: chars,
=======
impl Lexer {
    fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
>>>>>>> qgis-qxu-5
            pos: 0,
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

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>, Wkt2Error> {
        self.skip_whitespace();

        match self.peek() {
            None => Ok(None),
            Some('[') => {
                self.advance();
                Ok(Some(Token::OpenBracket))
            }
            Some(']') => {
                self.advance();
                Ok(Some(Token::CloseBracket))
            }
            Some(',') => {
                self.advance();
                Ok(Some(Token::Comma))
            }
            Some('"') => self.read_string(),
            Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => self.read_word(),
            Some(ch) if ch.is_ascii_digit() || ch == '-' || ch == '+' || ch == '.' => {
                self.read_number()
            }
            Some(ch) => Err(Wkt2Error::UnexpectedChar {
                ch,
                pos: self.pos,
            }),
        }
    }

    fn read_word(&mut self) -> Result<Option<Token>, Wkt2Error> {
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                self.advance();
            } else {
                break;
            }
        }
        let word: String = self.input[start..self.pos].iter().collect();
        Ok(Some(Token::Word(word)))
    }

    fn read_number(&mut self) -> Result<Option<Token>, Wkt2Error> {
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
                if let Some(prev) = self.input.get(self.pos.wrapping_sub(1)) {
                    if *prev == 'e' || *prev == 'E' {
                        self.advance();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        let num_str: String = self.input[start..self.pos].iter().collect();
        match num_str.parse::<f64>() {
            Ok(n) => Ok(Some(Token::Number(n))),
            Err(_) => Err(Wkt2Error::InvalidNumber {
                value: num_str,
                pos: start,
            }),
        }
    }

    fn read_string(&mut self) -> Result<Option<Token>, Wkt2Error> {
        self.advance(); // skip opening quote
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if ch == '"' {
                break;
            }
            self.advance();
        }
        let s: String = self.input[start..self.pos].iter().collect();
        if self.peek() == Some('"') {
            self.advance(); // skip closing quote
        }
        Ok(Some(Token::StringLiteral(s)))
    }
}

/// Parser for WKT2:2019 CRS strings.
pub struct Wkt2Parser {
    lexer: Lexer,
    current: Option<Token>,
}

impl Wkt2Parser {
    pub fn new(input: &str) -> Self {
        let mut lexer = Lexer::new(input);
        let current = lexer.next_token().ok().flatten();
        Self { lexer, current }
    }

    pub fn parse(&mut self) -> Result<ParsedCrs, Wkt2Error> {
        let crs = self.parse_crs()?;
        self.lexer.skip_whitespace();
        if self.lexer.peek().is_some() {
            return Err(Wkt2Error::UnexpectedChar {
                ch: self.lexer.peek().unwrap(),
                pos: self.lexer.pos,
            });
        }
        Ok(crs)
    }

    fn advance(&mut self) -> Result<Option<Token>, Wkt2Error> {
        let old = self.current.take();
        self.current = self.lexer.next_token()?;
        Ok(old)
    }

    fn expect_word(&mut self, expected: &str) -> Result<(), Wkt2Error> {
        match self.current.take() {
            Some(Token::Word(w)) if w.eq_ignore_ascii_case(expected) => {
                self.current = self.lexer.next_token()?;
                Ok(())
            }
            Some(Token::Word(_)) => Err(Wkt2Error::ExpectedToken {
                token: expected.to_string(),
                pos: self.lexer.pos,
            }),
            Some(_) => Err(Wkt2Error::ExpectedToken {
                token: expected.to_string(),
                pos: self.lexer.pos,
            }),
            None => Err(Wkt2Error::UnexpectedEof),
        }
    }

    fn expect_bracket(&mut self, open: bool) -> Result<(), Wkt2Error> {
        let expected = if open {
            Token::OpenBracket
        } else {
            Token::CloseBracket
        };
        let token_str = if open { "[" } else { "]" };
        match self.current.take() {
            Some(ref t) if *t == expected => {
                self.current = self.lexer.next_token()?;
                Ok(())
            }
            Some(_) => Err(Wkt2Error::ExpectedToken {
                token: token_str.to_string(),
                pos: self.lexer.pos,
            }),
            None => Err(Wkt2Error::UnexpectedEof),
        }
    }

    fn expect_comma(&mut self) -> Result<(), Wkt2Error> {
        match self.current.take() {
            Some(Token::Comma) => {
                self.current = self.lexer.next_token()?;
                Ok(())
            }
            Some(_) => Err(Wkt2Error::ExpectedToken {
                token: ",".to_string(),
                pos: self.lexer.pos,
            }),
            None => Err(Wkt2Error::UnexpectedEof),
        }
    }

    fn parse_crs(&mut self) -> Result<ParsedCrs, Wkt2Error> {
        let crs_type = match self.current.take() {
            Some(Token::Word(w)) => {
                let wt = w.to_uppercase();
                match wt.as_str() {
<<<<<<< HEAD
                    "GEOGCRS" | "PROJCRS" | "GEODCRS" | "VERTCRS" | "COMPD_CS" | "BASEGEOGCRS" => {
=======
                    "GEOGCRS" | "PROJCRS" | "GEODCRS" | "VERTCRS" | "COMPD_CS"
                    | "BASEGEOGCRS" => {
>>>>>>> qgis-qxu-5
                        self.current = self.lexer.next_token()?;
                        wt
                    }
                    _ => {
                        return Err(Wkt2Error::UnknownKeyword {
                            keyword: w,
                            pos: self.lexer.pos,
                        })
                    }
                }
            }
            Some(Token::StringLiteral(s)) => {
                let wt = s.to_uppercase();
                match wt.as_str() {
<<<<<<< HEAD
                    "GEOGCRS" | "PROJCRS" | "GEODCRS" | "VERTCRS" | "COMPD_CS" | "BASEGEOGCRS" => {
=======
                    "GEOGCRS" | "PROJCRS" | "GEODCRS" | "VERTCRS" | "COMPD_CS"
                    | "BASEGEOGCRS" => {
>>>>>>> qgis-qxu-5
                        self.current = self.lexer.next_token()?;
                        wt
                    }
                    _ => {
                        return Err(Wkt2Error::UnknownKeyword {
                            keyword: s,
                            pos: self.lexer.pos,
                        })
                    }
                }
            }
            Some(_) => {
                return Err(Wkt2Error::UnexpectedChar {
                    ch: '\0',
                    pos: self.lexer.pos,
                })
            }
            None => return Err(Wkt2Error::UnexpectedEof),
        };

        self.expect_bracket(true)?;

        let name = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        let mut parsed = ParsedCrs {
            crs_type,
            name,
            id: None,
            datum: None,
            ellipsoid: None,
            prime_meridian: None,
            cs: None,
            conversion: None,
            components: Vec::new(),
            axes: Vec::new(),
            units: Vec::new(),
        };

        match parsed.crs_type.as_str() {
            "COMPD_CS" => {
                self.expect_comma()?;
                let child1 = self.parse_crs()?;
                self.expect_comma()?;
                let child2 = self.parse_crs()?;
                parsed.components.push(child1);
                parsed.components.push(child2);
                if self.current == Some(Token::Comma) {
                    self.expect_comma()?;
                    parsed.id = Some(self.parse_id()?);
                }
            }
            "PROJCRS" => {
                self.expect_comma()?;
                let base = self.parse_crs()?;
                parsed.datum = base.datum.clone();
                parsed.ellipsoid = base.ellipsoid.clone();
                parsed.prime_meridian = base.prime_meridian.clone();
                self.expect_comma()?;
                parsed.conversion = Some(self.parse_conversion()?);
                self.expect_comma()?;
                parsed.cs = Some(self.parse_cs()?);
                parsed.id = self.parse_optional_id()?;
            }
            "GEOGCRS" | "GEODCRS" | "BASEGEOGCRS" => {
                self.expect_comma()?;
<<<<<<< HEAD
                parsed.datum = Some(self.parse_datum()?);
=======
                // Parse datum, ensemble, or vertical datum
                parsed.datum = Some(self.parse_datum_or_ensemble()?);

>>>>>>> qgis-qxu-5
                self.expect_comma()?;
                parsed.prime_meridian = Some(self.parse_prime_meridian()?);
                self.expect_comma()?;
                parsed.cs = Some(self.parse_cs()?);
<<<<<<< HEAD
                parsed.id = self.parse_optional_id()?;
            }
            "VERTCRS" => {
                self.expect_comma()?;
                parsed.datum = Some(self.parse_datum()?);
=======

                // Optional ID (not present on BASEGEOGCRS)
                if parsed.crs_type != "BASEGEOGCRS" && self.current == Some(Token::Comma) {
                    self.expect_comma()?;
                    parsed.id = Some(self.parse_id()?);
                }
            }
            "VERTCRS" => {
                self.expect_comma()?;
                // Parse vertical datum (VDATUM) or ensemble
                parsed.datum = Some(self.parse_vdatum()?);

>>>>>>> qgis-qxu-5
                self.expect_comma()?;
                parsed.cs = Some(self.parse_cs()?);
                parsed.id = self.parse_optional_id()?;
            }
            _ => {
                return Err(Wkt2Error::UnsupportedCrsType {
                    crs_type: parsed.crs_type.clone(),
                })
            }
        }

        self.expect_bracket(false)?;
        Ok(parsed)
    }

    fn parse_id(&mut self) -> Result<AuthorityId, Wkt2Error> {
        self.expect_word("ID")?;
        self.expect_bracket(true)?;

        let authority = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        self.expect_comma()?;

        let code = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Number(n)) => {
                self.current = self.lexer.next_token()?;
                n.to_string()
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            self.skip_until_bracket()?;
        }

        self.expect_bracket(false)?;

        Ok(AuthorityId { authority, code })
    }

    fn parse_datum(&mut self) -> Result<Datum, Wkt2Error> {
        let is_vdatum = match self.current.take() {
            Some(Token::Word(w)) if w.eq_ignore_ascii_case("DATUM") => {
                self.current = self.lexer.next_token()?;
                false
            }
            Some(Token::Word(w)) if w.eq_ignore_ascii_case("VDATUM") => {
                self.current = self.lexer.next_token()?;
                true
            }
            _ => return Err(Wkt2Error::ExpectedToken {
                token: "DATUM".to_string(),
                pos: self.lexer.pos,
            }),
        };
        self.expect_bracket(true)?;

        let name = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        if is_vdatum {
            if self.current == Some(Token::Comma) {
                self.expect_comma()?;
                let _ = self.parse_id()?;
            }
            self.expect_bracket(false)?;
            return Ok(Datum {
                name,
                ellipsoid: None,
                ensemble: None,
            });
        }

        self.expect_comma()?;

        let mut ensemble = None;
        let ellipsoid = match self.current {
            Some(Token::Word(ref w)) if w.eq_ignore_ascii_case("ELLIPSOID") => {
                self.current = self.lexer.next_token()?;
                Some(self.parse_ellipsoid()?)
            }
            Some(Token::Word(ref w)) if w.eq_ignore_ascii_case("ENSEMBLE") => {
                self.current = self.lexer.next_token()?;
                ensemble = Some(self.parse_ensemble()?);
                None
            }
            _ => {
                return Err(Wkt2Error::MissingNode {
                    node: "ELLIPSOID or ENSEMBLE".to_string(),
                })
            }
        };

        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            let _ = self.parse_id()?;
        }

        self.expect_bracket(false)?;

        Ok(Datum {
            name,
            ellipsoid,
            ensemble,
        })
    }

    /// Parses either a DATUM node or a direct ENSEMBLE node.
    ///
    /// In WKT2, a GEODCRS can have either `DATUM[...]` or `ENSEMBLE[...]`
    /// directly (without a DATUM wrapper). This handles both forms.
    fn parse_datum_or_ensemble(&mut self) -> Result<Datum, Wkt2Error> {
        match self.current.take() {
            Some(Token::Word(ref w)) if w.eq_ignore_ascii_case("DATUM") => {
                self.current = self.lexer.next_token()?;
                self.parse_datum()
            }
            Some(Token::Word(ref w)) if w.eq_ignore_ascii_case("ENSEMBLE") => {
                self.current = self.lexer.next_token()?;
                let e = self.parse_ensemble()?;
                let name = e.name.clone();
                Ok(Datum {
                    name,
                    ellipsoid: None,
                    ensemble: Some(e),
                })
            }
            _ => Err(Wkt2Error::MissingNode {
                node: "DATUM or ENSEMBLE".to_string(),
            }),
        }
    }

    /// Parses a VDATUM node (vertical datum).
    ///
    /// VDATUM is simpler than DATUM: it has just a name and optional ID,
    /// with no ellipsoid or ensemble.
    fn parse_vdatum(&mut self) -> Result<Datum, Wkt2Error> {
        self.expect_word("VDATUM")?;
        self.expect_bracket(true)?;

        let name = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        // Optional ID
        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            let _ = self.parse_id()?;
        }

        self.expect_bracket(false)?;

        Ok(Datum {
            name,
            ellipsoid: None,
            ensemble: None,
        })
    }

    fn parse_ensemble(&mut self) -> Result<DatumEnsemble, Wkt2Error> {
        // The caller (parse_datum) has already consumed the ENSEMBLE token.
        self.expect_bracket(true)?;

        let name = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        let mut members = Vec::new();
        loop {
            self.expect_comma()?;
            let member = self.parse_ensemble_member()?;
            members.push(member);
            if self.current != Some(Token::Comma) {
                break;
            }
        }

        // After MEMBER nodes, there may be an ELLIPSOID node before the accuracy.
        if let Some(Token::Word(ref w)) = self.current {
            if w.eq_ignore_ascii_case("ELLIPSOID") {
                self.current = self.lexer.next_token()?;
                self.skip_balanced_node()?;
            }
        }

        self.expect_comma()?;
        let accuracy = match self.current.take() {
            Some(Token::Number(n)) => {
                self.current = self.lexer.next_token()?;
                n
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        // Skip ENSEMBLEACCURACY node that may follow
        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            if let Some(Token::Word(ref w)) = self.current {
                if w.eq_ignore_ascii_case("ENSEMBLEACCURACY") {
                    self.current = self.lexer.next_token()?;
                    self.skip_balanced_node()?;
                }
            }
        }

        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            let _ = self.parse_id()?;
        }

        self.expect_bracket(false)?;

        Ok(DatumEnsemble {
            name,
            members,
            accuracy,
        })
    }

    fn parse_ensemble_member(&mut self) -> Result<EnsembleMember, Wkt2Error> {
        self.expect_bracket(true)?;

        let name = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        let mut id = None;
        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            id = Some(self.parse_id()?);
        }

        self.expect_bracket(false)?;

        Ok(EnsembleMember { name, id })
    }

    fn parse_ellipsoid(&mut self) -> Result<Ellipsoid, Wkt2Error> {
        // The caller (parse_datum) has already consumed the ELLIPSOID token.
        self.expect_bracket(true)?;

        let name = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        self.expect_comma()?;

        let semi_major_axis = match self.current.take() {
            Some(Token::Number(n)) => {
                self.current = self.lexer.next_token()?;
                n
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        self.expect_comma()?;

        let inverse_flattening = match self.current.take() {
            Some(Token::Number(n)) => {
                self.current = self.lexer.next_token()?;
                n
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        let mut id = None;
        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            id = Some(self.parse_id()?);
        }

        self.expect_bracket(false)?;

        Ok(Ellipsoid {
            name,
            semi_major_axis,
            inverse_flattening,
            id,
        })
    }

    fn parse_prime_meridian(&mut self) -> Result<PrimeMeridian, Wkt2Error> {
        self.expect_word("PRIMEM")?;
        self.expect_bracket(true)?;

        let name = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        self.expect_comma()?;

        let longitude = match self.current.take() {
            Some(Token::Number(n)) => {
                self.current = self.lexer.next_token()?;
                n
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        let mut id = None;
        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            id = Some(self.parse_id()?);
        }

        self.expect_bracket(false)?;

        Ok(PrimeMeridian {
            name,
            longitude,
            id,
        })
    }

    fn parse_cs(&mut self) -> Result<CoordinateSystem, Wkt2Error> {
        self.expect_word("CS")?;
        self.expect_bracket(true)?;

        let type_ = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        self.expect_comma()?;

        let _num_axes = match self.current.take() {
            Some(Token::Number(n)) => {
                self.current = self.lexer.next_token()?;
                n
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        let mut id = None;
        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            id = Some(self.parse_id()?);
        }

        self.expect_bracket(false)?;

        // Parse AXIS nodes (siblings of CS, not children)
        let mut axes = Vec::new();
        while let Some(Token::Word(ref w)) = self.current {
            if w.eq_ignore_ascii_case("AXIS") {
                let axis = self.parse_axis()?;
                axes.push(axis);
                if self.current != Some(Token::Comma) {
                    break;
                }
                self.expect_comma()?;
            } else {
                break;
            }
        }

        Ok(CoordinateSystem { type_, axes })
    }

    fn parse_axis(&mut self) -> Result<Axis, Wkt2Error> {
        self.expect_word("AXIS")?;
        self.expect_bracket(true)?;

        let name = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        self.expect_comma()?;

        let direction = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        let mut abbreviation = String::new();
        let mut unit = String::new();

        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            abbreviation = match self.current.take() {
                Some(Token::StringLiteral(s)) => {
                    self.current = self.lexer.next_token()?;
                    s
                }
                Some(Token::Word(w)) => {
                    self.current = self.lexer.next_token()?;
                    w
                }
                _ => return Err(Wkt2Error::UnexpectedEof),
            };
        }

        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            unit = self.parse_unit_def()?;
        }

        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            let _ = self.parse_id()?;
        }

        self.expect_bracket(false)?;

        Ok(Axis {
            name,
            abbreviation,
            direction,
            unit,
        })
    }

    fn parse_conversion(&mut self) -> Result<Conversion, Wkt2Error> {
        self.expect_word("CONVERSION")?;
        self.expect_bracket(true)?;

        let name = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        self.expect_comma()?;

        let method = self.parse_method()?;

        let mut parameters = Vec::new();
        while self.current == Some(Token::Comma) {
            self.expect_comma()?;
            let param = self.parse_parameter()?;
            parameters.push(param);
        }

        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            let _ = self.parse_id()?;
        }

        self.expect_bracket(false)?;

        Ok(Conversion {
            name,
            method,
            parameters,
        })
    }

    fn parse_method(&mut self) -> Result<Method, Wkt2Error> {
        self.expect_word("METHOD")?;
        self.expect_bracket(true)?;

        let name = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        let mut id = None;
        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            id = Some(self.parse_id()?);
        }

        self.expect_bracket(false)?;

        Ok(Method { name, id })
    }

    fn parse_parameter(&mut self) -> Result<Parameter, Wkt2Error> {
        self.expect_word("PARAMETER")?;
        self.expect_bracket(true)?;

        let name = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        self.expect_comma()?;

        let value = match self.current.take() {
            Some(Token::Number(n)) => {
                self.current = self.lexer.next_token()?;
                n
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        let mut id = None;
        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            id = Some(self.parse_id()?);
        }

        self.expect_bracket(false)?;

        Ok(Parameter { name, value, id })
    }

    fn parse_unit_def(&mut self) -> Result<String, Wkt2Error> {
        let unit_type = match self.current.take() {
            Some(Token::Word(w)) => {
                let wt = w.to_uppercase();
                self.current = self.lexer.next_token()?;
                wt
            }
            Some(Token::StringLiteral(s)) => {
                let wt = s.to_uppercase();
                self.current = self.lexer.next_token()?;
                wt
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        let _name = match self.current.take() {
            Some(Token::StringLiteral(s)) => {
                self.current = self.lexer.next_token()?;
                s
            }
            Some(Token::Word(w)) => {
                self.current = self.lexer.next_token()?;
                w
            }
            _ => return Err(Wkt2Error::UnexpectedEof),
        };

        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            let _factor = match self.current.take() {
                Some(Token::Number(_n)) => {
                    self.current = self.lexer.next_token()?;
                }
                _ => return Err(Wkt2Error::UnexpectedEof),
            };
        }

        if self.current == Some(Token::Comma) {
            self.expect_comma()?;
            self.skip_until_bracket()?;
        }

        if self.current == Some(Token::CloseBracket) {
            self.current = self.lexer.next_token()?;
        }

        Ok(unit_type)
    }

    /// After parsing the CS, skip optional nodes (ANGLEUNIT, LENGTHUNIT,
    /// SCALEUNIT, etc.) and then parse the trailing ID if present.
    /// Returns the parsed ID, or `None` if no ID follows.
    fn parse_optional_id(&mut self) -> Result<Option<AuthorityId>, Wkt2Error> {
        while self.current == Some(Token::Comma) {
            // Consume the comma and check if the next token starts an ID node.
            self.expect_comma()?;
            match self.current.take() {
                Some(Token::Word(w)) if w.eq_ignore_ascii_case("ID") => {
                    // Put the token back and parse the ID.
                    self.current = Some(Token::Word(w));
                    let id = self.parse_id()?;
                    return Ok(Some(id));
                }
                Some(other) => {
                    // Not an ID — push the token back and skip the optional node
                    // using bracket counting (handles nested ID[...] inside).
                    self.current = Some(other);
                    self.skip_balanced_node()?;
                }
                None => return Err(Wkt2Error::UnexpectedEof),
            }
        }
        Ok(None)
    }

    /// Skips a balanced bracket node (e.g., ANGLEUNIT["degree",0.017,ID["EPSG",9122]]).
    /// Uses bracket counting so nested `]` inside the node are handled correctly.
    /// On return, `self.current` is the token after the node's closing `]`.
    fn skip_balanced_node(&mut self) -> Result<(), Wkt2Error> {
        // The current token is the node keyword (e.g., ANGLEUNIT). We need to
        // find the opening `[`, then count brackets until we find the matching `]`.
        let mut depth: i32 = 0;
        let mut started = false;

        while let Some(token) = self.current.take() {
            match token {
                Token::OpenBracket => {
                    depth += 1;
                    started = true;
                }
                Token::CloseBracket => {
                    depth -= 1;
                    if started && depth == 0 {
                        // Found the matching close — advance past it.
                        self.current = self.lexer.next_token()?;
                        return Ok(());
                    }
                }
                _ => {}
            }
            self.current = self.lexer.next_token()?;
        }

        Err(Wkt2Error::UnexpectedEof)
    }

    fn skip_until_bracket(&mut self) -> Result<(), Wkt2Error> {
        while self.current.is_some() && self.current != Some(Token::CloseBracket) {
            self.current = self.lexer.next_token()?;
        }
        Ok(())
    }
}

/// Convenience function to parse a WKT2:2019 string.
pub fn parse_wkt2(input: &str) -> Result<ParsedCrs, Wkt2Error> {
    let mut parser = Wkt2Parser::new(input);
    parser.parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_geogcrs_basic() {
        let wkt = r#"GEOGCRS["WGS 84",DATUM["World Geodetic System 1984",ELLIPSOID["WGS 84",6378137,298.257223563,ID["EPSG",7030]]],PRIMEM["Greenwich",0,ID["EPSG",8901]],CS[ellipsoidal,2],AXIS["longitude",east],AXIS["latitude",north],ANGLEUNIT["degree",0.0174532925199433,ID["EPSG",9122]],ID["EPSG",4326]]"#;
        let result = parse_wkt2(wkt);
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let crs = result.unwrap();
        assert_eq!(crs.crs_type, "GEOGCRS");
        assert_eq!(crs.name, "WGS 84");
        assert!(crs.datum.is_some());
        assert!(crs.prime_meridian.is_some());
        assert!(crs.cs.is_some());
    }

    #[test]
    fn parse_projCRS_basic() {
        let wkt = r#"PROJCRS["WGS 84 / Pseudo-Mercator",BASEGEOGCRS["WGS 84",DATUM["World Geodetic System 1984",ELLIPSOID["WGS 84",6378137,298.257223563,ID["EPSG",7030]]],PRIMEM["Greenwich",0,ID["EPSG",8901]]],CONVERSION["Popular Visualisation Pseudo-Mercator",METHOD["Popular Visualisation Pseudo Mercator",ID["EPSG",1024]],PARAMETER["Latitude of natural origin",0,ID["EPSG",8801]],PARAMETER["Longitude of natural origin",0,ID["EPSG",8802]],PARAMETER["False easting",0,ID["EPSG",8806]],PARAMETER["False northing",0,ID["EPSG",8807]]],CS[Cartesian,2],AXIS["easting",east],AXIS["northing",north],LENGTHUNIT["metre",1,ID["EPSG",9001]],ID["EPSG",3857]]"#;
        let result = parse_wkt2(wkt);
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let crs = result.unwrap();
        assert_eq!(crs.crs_type, "PROJCRS");
        assert_eq!(crs.name, "WGS 84 / Pseudo-Mercator");
        assert!(crs.conversion.is_some());
        assert!(crs.datum.is_some());
    }

    #[test]
    fn parse_compound_crs() {
        let wkt = r#"COMPD_CS["WGS 84 + EGM96 height",GEOGCRS["WGS 84",DATUM["World Geodetic System 1984",ELLIPSOID["WGS 84",6378137,298.257223563,ID["EPSG",7030]]],PRIMEM["Greenwich",0,ID["EPSG",8901]],CS[ellipsoidal,2],AXIS["longitude",east],AXIS["latitude",north],ANGLEUNIT["degree",0.0174532925199433,ID["EPSG",9122]],ID["EPSG",4326]],VERTCRS["EGM96 height",VDATUM["EGM96 geoid"],CS[vertical,1],AXIS["up",up],LENGTHUNIT["metre",1,ID["EPSG",9001]],ID["EPSG",5773]],ID["EPSG",5773]]"#;
        let result = parse_wkt2(wkt);
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let crs = result.unwrap();
        assert_eq!(crs.crs_type, "COMPD_CS");
        assert_eq!(crs.components.len(), 2);
    }

    #[test]
    fn parse_geodcrs_with_ensemble() {
        let wkt = r#"GEODCRS["WGS 84",ENSEMBLE["World Geodetic System 1984 ensemble",MEMBER["World Geodetic System 1984 (Transit)",ID["EPSG",1166]],MEMBER["World Geodetic System 1984 (G730)",ID["EPSG",1167]],MEMBER["World Geodetic System 1984 (G873)",ID["EPSG",1168]],MEMBER["World Geodetic System 1984 (G1150)",ID["EPSG",1169]],MEMBER["World Geodetic System 1984 (G1674)",ID["EPSG",1170]],MEMBER["World Geodetic System 1984 (G1762)",ID["EPSG",1171]],MEMBER["World Geodetic System 1984 (G2139)",ID["EPSG",1172]],MEMBER["World Geodetic System 1984 (G2296)",ID["EPSG",1173]],ELLIPSOID["WGS 84",6378137,298.257223563,ID["EPSG",7030]],ENSEMBLEACCURACY[2.0,ID["EPSG",6323]]],PRIMEM["Greenwich",0,ID["EPSG",8901]],CS[ellipsoidal,2],AXIS["longitude",east],AXIS["latitude",north],ANGLEUNIT["degree",0.0174532925199433,ID["EPSG",9122]],ID["EPSG",4326]]"#;
        let result = parse_wkt2(wkt);
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let crs = result.unwrap();
        assert_eq!(crs.crs_type, "GEODCRS");
    }

    #[test]
    fn parse_vertCRS() {
        let wkt = r#"VERTCRS["EGM96 height",VDATUM["EGM96 geoid"],CS[vertical,1],AXIS["up",up],LENGTHUNIT["metre",1,ID["EPSG",9001]],ID["EPSG",5773]]"#;
        let result = parse_wkt2(wkt);
        assert!(result.is_ok(), "parse failed: {:?}", result.err());
        let crs = result.unwrap();
        assert_eq!(crs.crs_type, "VERTCRS");
    }

    #[test]
    fn parse_id() {
        let wkt = r#"GEOGCRS["WGS 84",DATUM["World Geodetic System 1984",ELLIPSOID["WGS 84",6378137,298.257223563,ID["EPSG",7030]]],PRIMEM["Greenwich",0,ID["EPSG",8901]],CS[ellipsoidal,2],AXIS["longitude",east],AXIS["latitude",north],ANGLEUNIT["degree",0.0174532925199433,ID["EPSG",9122]],ID["EPSG",4326]]"#;
        let crs = parse_wkt2(wkt).unwrap();
        assert!(crs.id.is_some());
        let id = crs.id.unwrap();
        assert_eq!(id.authority, "EPSG");
        assert_eq!(id.code, "4326");
    }

    #[test]
    fn error_on_unknown_keyword() {
        let wkt = r#"UNKNOWN["test"]"#;
        let result = parse_wkt2(wkt);
        assert!(result.is_err());
    }

    #[test]
    fn error_on_unexpected_eof() {
        let wkt = r#"GEOGCRS["WGS 84""#;
        let result = parse_wkt2(wkt);
        assert!(result.is_err());
    }

    #[test]
    fn error_on_unbalanced_brackets() {
        let wkt = r#"GEOGCRS["WGS 84",DATUM["test",ELLIPSOID["WGS 84",6378137,298.257223563]]"#;
        let result = parse_wkt2(wkt);
        assert!(result.is_err());
    }
}