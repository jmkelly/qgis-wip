//! PROJ pipeline parser.
//!
//! Parses `+proj=pipeline` syntax with step separation (`+step` and `||`),
//! step types (unitconvert, cart, push, pop, helmert, axisswap, noop,
//! pipeline), parameter extraction, quoted strings, comments, and nested
//! pipelines.
//!
//! This is the front half of the PROJ pipeline support gap (see ADR 0006).
//! It parses pipeline strings into a structured representation; executing
//! the pipeline is out of scope for this module.

use std::fmt;

/// A parsed PROJ pipeline.
///
/// A pipeline is an ordered sequence of steps. Each step is a single
/// operation (unit conversion, Cartesian conversion, Helmert transformation,
/// etc.) or a nested pipeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Pipeline {
    /// The steps in the pipeline, in execution order.
    pub steps: Vec<Step>,
}

impl Pipeline {
    /// Creates a new pipeline from a list of steps.
    pub fn new(steps: Vec<Step>) -> Self {
        Self { steps }
    }

    /// Returns the number of steps in the pipeline.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Returns `true` if the pipeline has no steps.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

impl fmt::Display for Pipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "+proj=pipeline")?;
        for step in &self.steps {
            write!(f, " +step {step}")?;
        }
        Ok(())
    }
}

/// A single step in a pipeline.
///
/// Each variant carries its relevant parameters as strongly-typed fields.
/// The [`Step::Projection`] variant captures projection methods (lcc, tmerc,
/// utm, webmerc, sterea, laea, hgridshift, vgridshift, etc.) that are not
/// one of the eight core pipeline operations.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Unit conversion step (`+proj=unitconvert`).
    ///
    /// Converts between angular or linear units (e.g., degrees to radians,
    /// metres to US survey feet).
    UnitConvert {
        /// Input unit (`+xy_in`).
        xy_in: String,
        /// Output unit (`+xy_out`).
        xy_out: String,
    },

    /// Cartesian conversion step (`+proj=cart`).
    ///
    /// Converts geodetic coordinates (longitude, latitude, height) to
    /// geocentric Cartesian coordinates (X, Y, Z).
    Cart {
        /// Ellipsoid name (`+ellps`).
        ellps: String,
    },

    /// Push step (`+proj=push`).
    ///
    /// Pushes one or more coordinate components onto the stack, setting them
    /// to zero. Used to ensure a 4D coordinate tuple before Cartesian
    /// conversion.
    Push {
        /// Values to push (`+v_1`, `+v_2`, `+v_3`).
        ///
        /// Each entry is the value that the corresponding component is set to.
        /// A value of `0.0` means the component is zeroed; a non-zero value
        /// sets the component to that value.
        v: Vec<f64>,
    },

    /// Pop step (`+proj=pop`).
    ///
    /// Pops one or more coordinate components off the stack, discarding
    /// them. Used to drop the height component after geocentric conversion.
    Pop {
        /// Values to pop (`+v_1`, `+v_2`, `+v_3`).
        ///
        /// The length indicates how many components to pop.
        v: Vec<f64>,
    },

    /// Helmert transformation step (`+proj=helmert`).
    ///
    /// Applies a 7-parameter (or 14-parameter) Helmert transformation for
    /// datum shifts.
    Helmert {
        /// X translation in metres (`+x`).
        dx: f64,
        /// Y translation in metres (`+y`).
        dy: f64,
        /// Z translation in metres (`+z`).
        dz: f64,
        /// X rotation in arc-seconds (`+rx`).
        rx: f64,
        /// Y rotation in arc-seconds (`+ry`).
        ry: f64,
        /// Z rotation in arc-seconds (`+rz`).
        rz: f64,
        /// Scale factor in ppm (`+s`).
        s: f64,
        /// Time reference in years (`+t`).
        ///
        /// `None` if not specified (time-independent transformation).
        t: Option<f64>,
        /// Convention (`+convention`): `"position_vector"` or
        /// `"coordinate_frame"`.
        convention: String,
    },

    /// Axis swap step (`+proj=axisswap`).
    ///
    /// Reorders coordinate axes (e.g., swapping latitude and longitude).
    AxisSwap {
        /// Axis order (`+order`), e.g., `[2, 1]` swaps the first two axes.
        ///
        /// 1-based axis indices. Values 1-4 select x, y, z, t respectively.
        order: Vec<i32>,
    },

    /// No-op step (`+proj=noop`).
    ///
    /// Does nothing. Used as a placeholder in some pipeline definitions.
    NoOp,

    /// Nested pipeline step (`+proj=pipeline`).
    ///
    /// A pipeline nested inside another pipeline. The nested pipeline is
    /// parsed recursively.
    Pipeline(Pipeline),

    /// Projection step (lcc, tmerc, utm, webmerc, sterea, laea, hgridshift,
    /// vgridshift, etc.).
    ///
    /// Captures any projection method that is not one of the eight core
    /// pipeline operations. The parameters are stored as key-value pairs.
    Projection {
        /// Projection method name (e.g., `"lcc"`, `"tmerc"`, `"utm"`).
        method: String,
        /// Projection parameters as `(key, value)` pairs.
        ///
        /// Includes all `+key=value` tokens that appeared after the
        /// `+proj=<method>` token, excluding `+step` and `+inv`.
        params: Vec<(String, String)>,
        /// Whether the step is inverted (`+inv`).
        inverse: bool,
    },
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnitConvert { xy_in, xy_out } => {
                write!(f, "+proj=unitconvert +xy_in={xy_in} +xy_out={xy_out}")
            }
            Self::Cart { ellps } => {
                write!(f, "+proj=cart +ellps={ellps}")
            }
            Self::Push { v } => {
                write!(f, "+proj=push")?;
                for (i, val) in v.iter().enumerate() {
                    write!(f, " +v_{}={val}", i + 1)?;
                }
                Ok(())
            }
            Self::Pop { v } => {
                write!(f, "+proj=pop")?;
                for (i, val) in v.iter().enumerate() {
                    write!(f, " +v_{}={val}", i + 1)?;
                }
                Ok(())
            }
            Self::Helmert {
                dx,
                dy,
                dz,
                rx,
                ry,
                rz,
                s,
                t,
                convention,
            } => {
                write!(
                    f,
                    "+proj=helmert +x={dx} +y={dy} +z={dz} +rx={rx} +ry={ry} +rz={rz} +s={s}"
                )?;
                if let Some(t) = t {
                    write!(f, " +t={t}")?;
                }
                write!(f, " +convention={convention}")
            }
            Self::AxisSwap { order } => {
                let order_str = order
                    .iter()
                    .map(|o| o.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                write!(f, "+proj=axisswap +order={order_str}")
            }
            Self::NoOp => {
                write!(f, "+proj=noop")
            }
            Self::Pipeline(p) => {
                write!(f, "+proj=pipeline")?;
                for step in &p.steps {
                    write!(f, " +step {step}")?;
                }
                Ok(())
            }
            Self::Projection {
                method,
                params,
                inverse,
            } => {
                if *inverse {
                    write!(f, "+inv ")?;
                }
                write!(f, "+proj={method}")?;
                for (key, value) in params {
                    write!(f, " +{key}={value}")?;
                }
                Ok(())
            }
        }
    }
}

/// Errors that can occur when parsing a pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineParseError {
    /// The input string is empty or contains only whitespace/comments.
    EmptyInput,

    /// The pipeline does not start with `+proj=pipeline`.
    NotAPipeline {
        /// Line number (1-based) where the error occurred.
        line: usize,
        /// Column number (1-based) where the error occurred.
        col: usize,
    },

    /// An unexpected character was encountered.
    UnexpectedChar {
        /// Line number (1-based).
        line: usize,
        /// Column number (1-based).
        col: usize,
        /// The unexpected character.
        ch: char,
    },

    /// A quoted string was not closed.
    UnclosedQuote {
        /// Line number (1-based) where the quote opened.
        line: usize,
        /// Column number (1-based) where the quote opened.
        col: usize,
        /// The quote character (`'` or `"`).
        quote: char,
    },

    /// A required parameter is missing for a step.
    MissingParameter {
        /// Line number (1-based).
        line: usize,
        /// Column number (1-based).
        col: usize,
        /// The missing parameter name (e.g., `"xy_in"`).
        param: String,
    },

    /// A parameter value could not be parsed as the expected type.
    InvalidValue {
        /// Line number (1-based).
        line: usize,
        /// Column number (1-based).
        col: usize,
        /// The parameter name.
        param: String,
        /// The unparseable value.
        value: String,
    },

    /// A step separator (`+step` or `||`) was expected but not found.
    ExpectedStepSeparator {
        /// Line number (1-based).
        line: usize,
        /// Column number (1-based).
        col: usize,
    },

    /// A projection step (`+proj=<method>`) was expected but not found.
    ExpectedProjection {
        /// Line number (1-based).
        line: usize,
        /// Column number (1-based).
        col: usize,
    },

    /// A step has no `+proj=` token.
    MissingProj {
        /// Line number (1-based).
        line: usize,
        /// Column number (1-based).
        col: usize,
    },
}

impl fmt::Display for PipelineParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "empty input"),
            Self::NotAPipeline { line, col } => {
                write!(f, "not a pipeline: expected '+proj=pipeline' at line {line}, column {col}")
            }
            Self::UnexpectedChar { line, col, ch } => {
                write!(f, "unexpected character {ch:?} at line {line}, column {col}")
            }
            Self::UnclosedQuote { line, col, quote } => {
                write!(f, "unclosed {quote:?} quote at line {line}, column {col}")
            }
            Self::MissingParameter { line, col, param } => {
                write!(f, "missing required parameter {param:?} at line {line}, column {col}")
            }
            Self::InvalidValue {
                line,
                col,
                param,
                value,
            } => {
                write!(
                    f,
                    "invalid value {value:?} for parameter {param:?} at line {line}, column {col}"
                )
            }
            Self::ExpectedStepSeparator { line, col } => {
                write!(f, "expected step separator (+step or ||) at line {line}, column {col}")
            }
            Self::ExpectedProjection { line, col } => {
                write!(f, "expected projection step (+proj=<method>) at line {line}, column {col}")
            }
            Self::MissingProj { line, col } => {
                write!(f, "missing +proj= token at line {line}, column {col}")
            }
        }
    }
}

impl std::error::Error for PipelineParseError {}

/// Parses a PROJ pipeline string into a [`Pipeline`].
///
/// The input must start with `+proj=pipeline` followed by one or more steps,
/// each separated by `+step` (or `||` in compact form). Each step begins
/// with `+proj=<method>` followed by method-specific parameters.
///
/// # Examples
///
/// ```
/// use qgis_crs::pipeline::parse;
///
/// let pipeline = parse("+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=cart +ellps=WGS84").unwrap();
/// assert_eq!(pipeline.steps.len(), 2);
/// ```
pub fn parse(input: &str) -> Result<Pipeline, PipelineParseError> {
    let tokens = tokenize(input)?;
    let mut parser = Parser::new(tokens);
    parser.parse_pipeline()
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

/// A token produced by the tokenizer.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    /// A parameter token: `+key=value` or `+key` (flag).
    Param {
        /// Parameter key (without leading `+`).
        key: String,
        /// Parameter value (empty string for flags).
        value: String,
        /// Line number (1-based) where the token starts.
        line: usize,
        /// Column number (1-based) where the token starts.
        col: usize,
    },
    /// A step separator: `+step` or `||`.
    StepSeparator {
        /// Line number (1-based).
        line: usize,
        /// Column number (1-based).
        col: usize,
    },
}

/// Tokenizes a pipeline string into a vector of tokens.
///
/// Handles:
/// - Whitespace separation (space, tab, CR, LF)
/// - Comments (`#` to end of line)
/// - Quoted strings (single `'` and double `"` quotes)
/// - Step separators (`+step` and `||`)
/// - Parameter tokens (`+key=value` or `+key`)
fn tokenize(input: &str) -> Result<Vec<Token>, PipelineParseError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    let mut line: usize = 1;
    let mut col: usize = 1;

    while let Some(&c) = chars.peek() {
        match c {
            // Whitespace
            ' ' | '\t' | '\r' | '\n' => {
                if c == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
                chars.next();
            }

            // Comment: skip to end of line
            '#' => {
                while let Some(&c) = chars.peek() {
                    if c == '\n' {
                        break;
                    }
                    chars.next();
                    col += 1;
                }
            }

            // Parameter token
            '+' => {
                chars.next();
                col += 1;
                let token_line = line;
                let token_col = col;

                // Read the key: everything up to '=', whitespace, or '#'
                let mut key = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '=' || c == ' ' || c == '\t' || c == '\r' || c == '\n' || c == '#' {
                        break;
                    }
                    key.push(c);
                    chars.next();
                    col += 1;
                }

                // Read the value if '=' follows
                let mut value = String::new();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    col += 1;

                    // Check for quoted string
                    if chars.peek() == Some(&'\'') || chars.peek() == Some(&'"') {
                        let quote = chars.next().unwrap();
                        col += 1;
                        let value_line = line;
                        let value_col = col;
                        let mut closed = false;
                        while let Some(&c) = chars.peek() {
                            if c == quote {
                                chars.next();
                                col += 1;
                                closed = true;
                                break;
                            }
                            if c == '\n' {
                                line += 1;
                                col = 1;
                            } else {
                                col += 1;
                            }
                            value.push(c);
                            chars.next();
                        }
                        if !closed {
                            return Err(PipelineParseError::UnclosedQuote {
                                line: value_line,
                                col: value_col,
                                quote,
                            });
                        }
                    } else {
                        // Unquoted value: read until whitespace or '#'
                        while let Some(&c) = chars.peek() {
                            if c == ' ' || c == '\t' || c == '\r' || c == '\n' || c == '#' {
                                break;
                            }
                            value.push(c);
                            chars.next();
                            col += 1;
                        }
                    }
                }

                // Classify: "+step" is a step separator
                if key == "step" {
                    tokens.push(Token::StepSeparator {
                        line: token_line,
                        col: token_col,
                    });
                } else {
                    tokens.push(Token::Param {
                        key,
                        value,
                        line: token_line,
                        col: token_col,
                    });
                }
            }

            // Compact step separator: ||
            '|' => {
                chars.next();
                col += 1;
                let sep_line = line;
                let sep_col = col;
                if chars.peek() == Some(&'|') {
                    chars.next();
                    col += 1;
                    tokens.push(Token::StepSeparator {
                        line: sep_line,
                        col: sep_col,
                    });
                } else {
                    return Err(PipelineParseError::UnexpectedChar {
                        line: sep_line,
                        col: sep_col,
                        ch: '|',
                    });
                }
            }

            // Any other character is an error
            _ => {
                return Err(PipelineParseError::UnexpectedChar { line, col, ch: c });
            }
        }
    }

    Ok(tokens)
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// A recursive-descent parser for PROJ pipeline strings.
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    /// Creates a new parser from a token vector.
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Returns the next token without consuming it.
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    /// Consumes and returns the next token, or `None` if at end.
    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).cloned();
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    /// Parses a full pipeline: `+proj=pipeline` followed by steps.
    fn parse_pipeline(&mut self) -> Result<Pipeline, PipelineParseError> {
        // Expect +proj=pipeline
        match self.next() {
            Some(Token::Param { key, value, line: _, col: _ }) if key == "proj" && value == "pipeline" => {}
            Some(Token::Param { line, col, .. }) => {
                return Err(PipelineParseError::NotAPipeline { line, col });
            }
            Some(Token::StepSeparator { line, col }) => {
                return Err(PipelineParseError::NotAPipeline { line, col });
            }
            None => {
                return Err(PipelineParseError::EmptyInput);
            }
        }

        let mut steps = Vec::new();

        // Allow the first step to omit the +step separator (lenient parsing).
        // If the next token is a step separator, consume it.
        if let Some(Token::StepSeparator { .. }) = self.peek() {
            self.next();
        }

        // Parse steps until we run out of tokens
        while self.peek().is_some() {
            let step = self.parse_step()?;
            steps.push(step);

            // After each step, expect a step separator before the next step.
            if self.peek().is_some() {
                match self.peek() {
                    Some(Token::StepSeparator { .. }) => {
                        self.next();
                    }
                    Some(Token::Param { line, col, .. }) => {
                        return Err(PipelineParseError::ExpectedStepSeparator {
                            line: *line,
                            col: *col,
                        });
                    }
                    None => break,
                }
            }
        }

        Ok(Pipeline { steps })
    }

    /// Parses a single step.
    fn parse_step(&mut self) -> Result<Step, PipelineParseError> {
        // Check for +inv flag
        let mut inverse = false;
        if let Some(Token::Param { key, value, .. }) = self.peek() {
            if key == "inv" && value.is_empty() {
                inverse = true;
                self.next();
            }
        }

        // Expect +proj=<method>
        let (method, _line, _col) = match self.next() {
            Some(Token::Param { key, value, line, col }) if key == "proj" => (value, line, col),
            Some(Token::Param { line, col, .. }) => {
                return Err(PipelineParseError::ExpectedProjection { line, col });
            }
            Some(Token::StepSeparator { line, col }) => {
                return Err(PipelineParseError::ExpectedProjection { line, col });
            }
            None => {
                return Err(PipelineParseError::ExpectedProjection { line: 0, col: 0 });
            }
        };

        // Parse method-specific parameters
        match method.as_str() {
            "unitconvert" => self.parse_unitconvert(),
            "cart" => self.parse_cart(),
            "push" => self.parse_push(),
            "pop" => self.parse_pop(),
            "helmert" => self.parse_helmert(),
            "axisswap" => self.parse_axisswap(),
            "noop" => self.parse_noop(),
            "pipeline" => self.parse_nested_pipeline(),
            _ => self.parse_projection(&method, inverse),
        }
    }

    /// Parses parameters until a step separator, a `+proj=` token (start of a
    /// new step), or end of tokens.
    fn parse_params(&mut self) -> Vec<(String, String)> {
        let mut params = Vec::new();
        while let Some(token) = self.peek() {
            match token {
                Token::Param { key, value, .. } if key != "proj" => {
                    params.push((key.clone(), value.clone()));
                    self.next();
                }
                Token::Param { .. } => break, // +proj= starts a new step
                Token::StepSeparator { .. } => break,
            }
        }
        params
    }

    /// Finds a parameter by key (case-sensitive) in a parameter list.
    fn find_param<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
        params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Parses a `unitconvert` step.
    fn parse_unitconvert(&mut self) -> Result<Step, PipelineParseError> {
        let params = self.parse_params();
        let xy_in = Self::find_param(&params, "xy_in")
            .ok_or(PipelineParseError::MissingParameter {
                line: 0,
                col: 0,
                param: "xy_in".to_string(),
            })?
            .to_string();
        let xy_out = Self::find_param(&params, "xy_out")
            .ok_or(PipelineParseError::MissingParameter {
                line: 0,
                col: 0,
                param: "xy_out".to_string(),
            })?
            .to_string();
        Ok(Step::UnitConvert { xy_in, xy_out })
    }

    /// Parses a `cart` step.
    fn parse_cart(&mut self) -> Result<Step, PipelineParseError> {
        let params = self.parse_params();
        let ellps = Self::find_param(&params, "ellps")
            .ok_or(PipelineParseError::MissingParameter {
                line: 0,
                col: 0,
                param: "ellps".to_string(),
            })?
            .to_string();
        Ok(Step::Cart { ellps })
    }

    /// Parses a `push` step.
    fn parse_push(&mut self) -> Result<Step, PipelineParseError> {
        let params = self.parse_params();
        let v = Self::parse_v_params(&params)?;
        Ok(Step::Push { v })
    }

    /// Parses a `pop` step.
    fn parse_pop(&mut self) -> Result<Step, PipelineParseError> {
        let params = self.parse_params();
        let v = Self::parse_v_params(&params)?;
        Ok(Step::Pop { v })
    }

    /// Parses `+v_1`, `+v_2`, `+v_3` parameters into a vector.
    ///
    /// A bare `+v_3` (no `=value`) is treated as `0.0`, matching PROJ's
    /// convention where `+v_3` means "push/pop the v_3 component, setting it
    /// to zero".
    fn parse_v_params(params: &[(String, String)]) -> Result<Vec<f64>, PipelineParseError> {
        let mut v = Vec::new();
        for i in 1..=3 {
            let key = format!("v_{i}");
            if let Some(val) = Self::find_param(params, &key) {
                let parsed: f64 = if val.is_empty() {
                    0.0
                } else {
                    val.parse().map_err(|_| PipelineParseError::InvalidValue {
                        line: 0,
                        col: 0,
                        param: key.clone(),
                        value: val.to_string(),
                    })?
                };
                v.push(parsed);
            }
        }
        Ok(v)
    }

    /// Parses a `helmert` step.
    fn parse_helmert(&mut self) -> Result<Step, PipelineParseError> {
        let params = self.parse_params();

        let get_f64 = |key: &str| -> Result<f64, PipelineParseError> {
            match Self::find_param(&params, key) {
                Some(val) => val.parse().map_err(|_| PipelineParseError::InvalidValue {
                    line: 0,
                    col: 0,
                    param: key.to_string(),
                    value: val.to_string(),
                }),
                None => Ok(0.0),
            }
        };

        let dx = get_f64("x")?;
        let dy = get_f64("y")?;
        let dz = get_f64("z")?;
        let rx = get_f64("rx")?;
        let ry = get_f64("ry")?;
        let rz = get_f64("rz")?;
        let s = get_f64("s")?;
        let t = match Self::find_param(&params, "t") {
            Some(val) => Some(val.parse().map_err(|_| PipelineParseError::InvalidValue {
                line: 0,
                col: 0,
                param: "t".to_string(),
                value: val.to_string(),
            })?),
            None => None,
        };
        let convention = Self::find_param(&params, "convention")
            .unwrap_or("position_vector")
            .to_string();

        Ok(Step::Helmert {
            dx,
            dy,
            dz,
            rx,
            ry,
            rz,
            s,
            t,
            convention,
        })
    }

    /// Parses an `axisswap` step.
    fn parse_axisswap(&mut self) -> Result<Step, PipelineParseError> {
        let params = self.parse_params();
        let order_str = Self::find_param(&params, "order")
            .ok_or(PipelineParseError::MissingParameter {
                line: 0,
                col: 0,
                param: "order".to_string(),
            })?;
        let order: Result<Vec<i32>, _> = order_str
            .split(',')
            .map(|s| s.trim().parse::<i32>())
            .collect();
        let order = order.map_err(|_| PipelineParseError::InvalidValue {
            line: 0,
            col: 0,
            param: "order".to_string(),
            value: order_str.to_string(),
        })?;
        Ok(Step::AxisSwap { order })
    }

    /// Parses a `noop` step.
    fn parse_noop(&mut self) -> Result<Step, PipelineParseError> {
        // noop takes no parameters; consume any that appear (lenient).
        self.parse_params();
        Ok(Step::NoOp)
    }

    /// Parses a nested `pipeline` step.
    fn parse_nested_pipeline(&mut self) -> Result<Step, PipelineParseError> {
        let mut steps = Vec::new();

        // Allow the first step to omit the +step separator.
        if let Some(Token::StepSeparator { .. }) = self.peek() {
            self.next();
        }

        while self.peek().is_some() {
            let step = self.parse_step()?;
            steps.push(step);

            if self.peek().is_some() {
                match self.peek() {
                    Some(Token::StepSeparator { .. }) => {
                        self.next();
                    }
                    Some(Token::Param { line, col, .. }) => {
                        return Err(PipelineParseError::ExpectedStepSeparator {
                            line: *line,
                            col: *col,
                        });
                    }
                    None => break,
                }
            }
        }

        Ok(Step::Pipeline(Pipeline { steps }))
    }

    /// Parses a generic projection step.
    fn parse_projection(
        &mut self,
        method: &str,
        inverse: bool,
    ) -> Result<Step, PipelineParseError> {
        let params = self.parse_params();
        Ok(Step::Projection {
            method: method.to_string(),
            params,
            inverse,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Simple pipelines
    // -----------------------------------------------------------------------

    #[test]
    fn parse_empty_pipeline() {
        let p = parse("+proj=pipeline").unwrap();
        assert!(p.is_empty());
        assert_eq!(p.len(), 0);
    }

    #[test]
    fn parse_single_step() {
        let p = parse("+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad").unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(
            p.steps[0],
            Step::UnitConvert {
                xy_in: "deg".to_string(),
                xy_out: "rad".to_string(),
            }
        );
    }

    #[test]
    fn parse_multi_step_pipeline() {
        let p = parse(
            "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=cart +ellps=WGS84",
        )
        .unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(
            p.steps[0],
            Step::UnitConvert {
                xy_in: "deg".to_string(),
                xy_out: "rad".to_string(),
            }
        );
        assert_eq!(
            p.steps[1],
            Step::Cart {
                ellps: "WGS84".to_string()
            }
        );
    }

    #[test]
    fn parse_compact_pipeline_with_pipe_separator() {
        let p = parse(
            "+proj=pipeline +proj=unitconvert +xy_in=deg +xy_out=rad || +proj=cart +ellps=WGS84",
        )
        .unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(
            p.steps[0],
            Step::UnitConvert {
                xy_in: "deg".to_string(),
                xy_out: "rad".to_string(),
            }
        );
        assert_eq!(
            p.steps[1],
            Step::Cart {
                ellps: "WGS84".to_string()
            }
        );
    }

    // -----------------------------------------------------------------------
    // Step types
    // -----------------------------------------------------------------------

    #[test]
    fn parse_unitconvert() {
        let p = parse(
            "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad",
        )
        .unwrap();
        assert_eq!(
            p.steps[0],
            Step::UnitConvert {
                xy_in: "deg".to_string(),
                xy_out: "rad".to_string(),
            }
        );
    }

    #[test]
    fn parse_unitconvert_metres_to_us_ft() {
        let p = parse(
            "+proj=pipeline +step +proj=unitconvert +xy_in=m +xy_out=us-ft",
        )
        .unwrap();
        assert_eq!(
            p.steps[0],
            Step::UnitConvert {
                xy_in: "m".to_string(),
                xy_out: "us-ft".to_string(),
            }
        );
    }

    #[test]
    fn parse_cart() {
        let p = parse(
            "+proj=pipeline +step +proj=cart +ellps=WGS84",
        )
        .unwrap();
        assert_eq!(
            p.steps[0],
            Step::Cart {
                ellps: "WGS84".to_string()
            }
        );
    }

    #[test]
    fn parse_push_v3() {
        let p = parse(
            "+proj=pipeline +step +proj=push +v_3",
        )
        .unwrap();
        assert_eq!(p.steps[0], Step::Push { v: vec![0.0] });
    }

    #[test]
    fn parse_push_v1_v2_v3() {
        let p = parse(
            "+proj=pipeline +step +proj=push +v_1 +v_2 +v_3",
        )
        .unwrap();
        assert_eq!(p.steps[0], Step::Push { v: vec![0.0, 0.0, 0.0] });
    }

    #[test]
    fn parse_pop_v3() {
        let p = parse(
            "+proj=pipeline +step +proj=pop +v_3",
        )
        .unwrap();
        assert_eq!(p.steps[0], Step::Pop { v: vec![0.0] });
    }

    #[test]
    fn parse_helmert_full() {
        let p = parse(
            "+proj=pipeline +step +proj=helmert +x=0.06155 +y=-0.01087 +z=-0.04019 +rx=-0.0394924 +ry=-0.0327221 +rz=-0.0328979 +s=-0.009994 +convention=coordinate_frame",
        )
        .unwrap();
        match &p.steps[0] {
            Step::Helmert {
                dx,
                dy,
                dz,
                rx,
                ry,
                rz,
                s,
                t,
                convention,
            } => {
                assert!((dx - 0.06155).abs() < 1e-10);
                assert!((dy - -0.01087).abs() < 1e-10);
                assert!((dz - -0.04019).abs() < 1e-10);
                assert!((rx - -0.0394924).abs() < 1e-10);
                assert!((ry - -0.0327221).abs() < 1e-10);
                assert!((rz - -0.0328979).abs() < 1e-10);
                assert!((s - -0.009994).abs() < 1e-10);
                assert_eq!(*t, None);
                assert_eq!(convention, "coordinate_frame");
            }
            other => panic!("expected Helmert, got {other:?}"),
        }
    }

    #[test]
    fn parse_helmert_minimal() {
        let p = parse(
            "+proj=pipeline +step +proj=helmert +x=-10 +y=158 +z=187",
        )
        .unwrap();
        match &p.steps[0] {
            Step::Helmert {
                dx,
                dy,
                dz,
                rx,
                ry,
                rz,
                s,
                t,
                convention,
            } => {
                assert_eq!(*dx, -10.0);
                assert_eq!(*dy, 158.0);
                assert_eq!(*dz, 187.0);
                assert_eq!(*rx, 0.0);
                assert_eq!(*ry, 0.0);
                assert_eq!(*rz, 0.0);
                assert_eq!(*s, 0.0);
                assert_eq!(*t, None);
                assert_eq!(convention, "position_vector");
            }
            other => panic!("expected Helmert, got {other:?}"),
        }
    }

    #[test]
    fn parse_helmert_with_time() {
        let p = parse(
            "+proj=pipeline +step +proj=helmert +x=1 +y=2 +z=3 +rx=4 +ry=5 +rz=6 +s=7 +t=2010.5 +convention=position_vector",
        )
        .unwrap();
        match &p.steps[0] {
            Step::Helmert { t, convention, .. } => {
                assert_eq!(*t, Some(2010.5));
                assert_eq!(convention, "position_vector");
            }
            other => panic!("expected Helmert, got {other:?}"),
        }
    }

    #[test]
    fn parse_axisswap() {
        let p = parse(
            "+proj=pipeline +step +proj=axisswap +order=2,1",
        )
        .unwrap();
        assert_eq!(
            p.steps[0],
            Step::AxisSwap { order: vec![2, 1] }
        );
    }

    #[test]
    fn parse_axisswap_four_axes() {
        let p = parse(
            "+proj=pipeline +step +proj=axisswap +order=4,1,2,3",
        )
        .unwrap();
        assert_eq!(
            p.steps[0],
            Step::AxisSwap { order: vec![4, 1, 2, 3] }
        );
    }

    #[test]
    fn parse_noop() {
        let p = parse(
            "+proj=pipeline +step +proj=noop",
        )
        .unwrap();
        assert_eq!(p.steps[0], Step::NoOp);
    }

    // -----------------------------------------------------------------------
    // Quoted strings
    // -----------------------------------------------------------------------

    #[test]
    fn parse_single_quoted_ellps() {
        let p = parse(
            "+proj=pipeline +step +proj=cart +ellps='WGS84'",
        )
        .unwrap();
        assert_eq!(
            p.steps[0],
            Step::Cart {
                ellps: "WGS84".to_string()
            }
        );
    }

    #[test]
    fn parse_double_quoted_ellps() {
        let p = parse(
            "+proj=pipeline +step +proj=cart +ellps=\"GRS80\"",
        )
        .unwrap();
        assert_eq!(
            p.steps[0],
            Step::Cart {
                ellps: "GRS80".to_string()
            }
        );
    }

    // -----------------------------------------------------------------------
    // Comments
    // -----------------------------------------------------------------------

    #[test]
    fn parse_with_comment() {
        let p = parse(
            "+proj=pipeline # this is a comment\n +step +proj=unitconvert +xy_in=deg +xy_out=rad",
        )
        .unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(
            p.steps[0],
            Step::UnitConvert {
                xy_in: "deg".to_string(),
                xy_out: "rad".to_string(),
            }
        );
    }

    #[test]
    fn parse_comment_after_step() {
        let p = parse(
            "+proj=pipeline +step +proj=noop # trailing comment",
        )
        .unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p.steps[0], Step::NoOp);
    }

    // -----------------------------------------------------------------------
    // Nested pipelines
    // -----------------------------------------------------------------------

    #[test]
    fn parse_nested_pipeline() {
        let p = parse(
            "+proj=pipeline +step +proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=cart +ellps=WGS84",
        )
        .unwrap();
        assert_eq!(p.len(), 1);
        match &p.steps[0] {
            Step::Pipeline(inner) => {
                assert_eq!(inner.len(), 2);
                assert_eq!(
                    inner.steps[0],
                    Step::UnitConvert {
                        xy_in: "deg".to_string(),
                        xy_out: "rad".to_string(),
                    }
                );
                assert_eq!(
                    inner.steps[1],
                    Step::Cart {
                        ellps: "WGS84".to_string()
                    }
                );
            }
            other => panic!("expected Pipeline, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Real QGIS pipeline strings
    // -----------------------------------------------------------------------

    #[test]
    fn parse_real_qgis_lcc_to_webmerc() {
        let p = parse(
            "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=webmerc +lat_0=0 +lon_0=0 +x_0=0 +y_0=0 +ellps=WGS84",
        )
        .unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(
            p.steps[0],
            Step::UnitConvert {
                xy_in: "deg".to_string(),
                xy_out: "rad".to_string(),
            }
        );
        match &p.steps[1] {
            Step::Projection { method, params, inverse } => {
                assert_eq!(method, "webmerc");
                assert!(!inverse);
                let keys: Vec<&String> = params.iter().map(|(k, _)| k).collect();
                assert!(keys.contains(&&"lat_0".to_string()));
                assert!(keys.contains(&&"lon_0".to_string()));
                assert!(keys.contains(&&"x_0".to_string()));
                assert!(keys.contains(&&"y_0".to_string()));
                assert!(keys.contains(&&"ellps".to_string()));
            }
            other => panic!("expected Projection, got {other:?}"),
        }
    }

    #[test]
    fn parse_real_qgis_inv_lcc_pipeline() {
        let p = parse(
            "+proj=pipeline +step +inv +proj=lcc +lat_0=-37 +lon_0=145 +lat_1=-36 +lat_2=-38 +x_0=2500000 +y_0=2500000 +ellps=GRS80 +step +proj=unitconvert +xy_in=rad +xy_out=deg",
        )
        .unwrap();
        assert_eq!(p.len(), 2);
        match &p.steps[0] {
            Step::Projection { method, inverse, .. } => {
                assert_eq!(method, "lcc");
                assert!(*inverse);
            }
            other => panic!("expected Projection, got {other:?}"),
        }
        assert_eq!(
            p.steps[1],
            Step::UnitConvert {
                xy_in: "rad".to_string(),
                xy_out: "deg".to_string(),
            }
        );
    }

    #[test]
    fn parse_real_qgis_helmert_pipeline() {
        let p = parse(
            "+proj=pipeline +step +inv +proj=sterea +lat_0=46 +lon_0=25 +k=0.99975 +x_0=500000 +y_0=500000 +ellps=krass +step +proj=push +v_3 +step +proj=cart +ellps=krass +step +proj=helmert +x=2.329 +y=-147.042 +z=-92.08 +rx=0.309 +ry=-0.325 +rz=-0.497 +s=5.69 +convention=coordinate_frame +step +inv +proj=cart +ellps=WGS84 +step +proj=pop +v_3 +step +proj=unitconvert +xy_in=rad +xy_out=deg",
        )
        .unwrap();
        assert_eq!(p.len(), 7);
        // Step 0: inv sterea
        match &p.steps[0] {
            Step::Projection { method, inverse, .. } => {
                assert_eq!(method, "sterea");
                assert!(*inverse);
            }
            other => panic!("expected Projection, got {other:?}"),
        }
        // Step 1: push v_3
        assert_eq!(p.steps[1], Step::Push { v: vec![0.0] });
        // Step 2: cart krass
        assert_eq!(
            p.steps[2],
            Step::Cart {
                ellps: "krass".to_string()
            }
        );
        // Step 3: helmert
        match &p.steps[3] {
            Step::Helmert { dx, convention, .. } => {
                assert!((dx - 2.329).abs() < 1e-10);
                assert_eq!(convention, "coordinate_frame");
            }
            other => panic!("expected Helmert, got {other:?}"),
        }
        // Step 4: inv cart WGS84
        match &p.steps[4] {
            Step::Cart { ellps } => assert_eq!(ellps, "WGS84"),
            other => panic!("expected Cart, got {other:?}"),
        }
        // Step 5: pop v_3
        assert_eq!(p.steps[5], Step::Pop { v: vec![0.0] });
        // Step 6: unitconvert
        assert_eq!(
            p.steps[6],
            Step::UnitConvert {
                xy_in: "rad".to_string(),
                xy_out: "deg".to_string(),
            }
        );
    }

    #[test]
    fn parse_real_qgis_axisswap_pipeline() {
        let p = parse(
            "+proj=pipeline +step +proj=axisswap +order=2,1 +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=push +v_3 +step +proj=cart +ellps=intl +step +proj=helmert +x=-150 +y=-250 +z=-1 +step +inv +proj=cart +ellps=WGS84 +step +proj=pop +v_3 +step +proj=unitconvert +xy_in=rad +xy_out=deg +step +proj=axisswap +order=2,1",
        )
        .unwrap();
        assert_eq!(p.len(), 9);
        assert_eq!(p.steps[0], Step::AxisSwap { order: vec![2, 1] });
        assert_eq!(
            p.steps[1],
            Step::UnitConvert {
                xy_in: "deg".to_string(),
                xy_out: "rad".to_string(),
            }
        );
        assert_eq!(p.steps[2], Step::Push { v: vec![0.0] });
        assert_eq!(
            p.steps[3],
            Step::Cart {
                ellps: "intl".to_string()
            }
        );
        match &p.steps[4] {
            Step::Helmert { dx, dy, dz, .. } => {
                assert_eq!(*dx, -150.0);
                assert_eq!(*dy, -250.0);
                assert_eq!(*dz, -1.0);
            }
            other => panic!("expected Helmert, got {other:?}"),
        }
        assert_eq!(
            p.steps[5],
            Step::Cart {
                ellps: "WGS84".to_string()
            }
        );
        assert_eq!(p.steps[6], Step::Pop { v: vec![0.0] });
        assert_eq!(
            p.steps[7],
            Step::UnitConvert {
                xy_in: "rad".to_string(),
                xy_out: "deg".to_string(),
            }
        );
        assert_eq!(p.steps[8], Step::AxisSwap { order: vec![2, 1] });
    }

    #[test]
    fn parse_real_qgis_hgridshift_pipeline() {
        let p = parse(
            "+proj=pipeline +step +proj=axisswap +order=2,1 +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +inv +proj=hgridshift +grids=au_icsm_GDA94_GDA2020_conformal_and_distortion.tif +step +proj=unitconvert +xy_in=rad +xy_out=deg +step +proj=axisswap +order=2,1",
        )
        .unwrap();
        assert_eq!(p.len(), 5);
        match &p.steps[2] {
            Step::Projection { method, params, inverse } => {
                assert_eq!(method, "hgridshift");
                assert!(*inverse);
                let grids = Step::find_param_value(params, "grids");
                assert_eq!(grids, Some("au_icsm_GDA94_GDA2020_conformal_and_distortion.tif"));
            }
            other => panic!("expected Projection, got {other:?}"),
        }
    }

    #[test]
    fn parse_real_qgis_utm_axisswap_pipeline() {
        let p = parse(
            "+proj=pipeline +step +inv +proj=tmerc +lat_0=49 +lon_0=-2 +k=0.9996012717 +x_0=400000 +y_0=-100000 +ellps=airy +step +proj=unitconvert +xy_in=rad +xy_out=deg +step +proj=axisswap +order=2,1",
        )
        .unwrap();
        assert_eq!(p.len(), 3);
        match &p.steps[0] {
            Step::Projection { method, inverse, .. } => {
                assert_eq!(method, "tmerc");
                assert!(*inverse);
            }
            other => panic!("expected Projection, got {other:?}"),
        }
        assert_eq!(
            p.steps[1],
            Step::UnitConvert {
                xy_in: "rad".to_string(),
                xy_out: "deg".to_string(),
            }
        );
        assert_eq!(p.steps[2], Step::AxisSwap { order: vec![2, 1] });
    }

    #[test]
    fn parse_real_qgis_vgridshift_pipeline() {
        let p = parse(
            "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +inv +proj=vgridshift +grids=au_ga_AGQG_20201120.tif +multiplier=1 +step +proj=unitconvert +xy_in=rad +xy_out=deg",
        )
        .unwrap();
        assert_eq!(p.len(), 3);
        match &p.steps[1] {
            Step::Projection { method, params, inverse } => {
                assert_eq!(method, "vgridshift");
                assert!(*inverse);
                let grids = Step::find_param_value(params, "grids");
                assert_eq!(grids, Some("au_ga_AGQG_20201120.tif"));
                let mult = Step::find_param_value(params, "multiplier");
                assert_eq!(mult, Some("1"));
            }
            other => panic!("expected Projection, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Round-trip tests
    // -----------------------------------------------------------------------

    #[test]
    fn roundtrip_simple_pipeline() {
        let input = "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=cart +ellps=WGS84";
        let p = parse(input).unwrap();
        let output = format!("{p}");
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_helmert_pipeline() {
        let input = "+proj=pipeline +step +proj=helmert +x=0.06155 +y=-0.01087 +z=-0.04019 +rx=-0.0394924 +ry=-0.0327221 +rz=-0.0328979 +s=-0.009994 +convention=coordinate_frame";
        let p = parse(input).unwrap();
        let output = format!("{p}");
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_axisswap() {
        let input = "+proj=pipeline +step +proj=axisswap +order=2,1";
        let p = parse(input).unwrap();
        let output = format!("{p}");
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_nested_pipeline() {
        let input = "+proj=pipeline +step +proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=cart +ellps=WGS84";
        let p = parse(input).unwrap();
        let output = format!("{p}");
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_projection_pipeline() {
        let input = "+proj=pipeline +step +inv +proj=lcc +lat_0=-37 +lon_0=145 +lat_1=-36 +lat_2=-38 +x_0=2500000 +y_0=2500000 +ellps=GRS80 +step +proj=unitconvert +xy_in=rad +xy_out=deg";
        let p = parse(input).unwrap();
        let output = format!("{p}");
        assert_eq!(output, input);
    }

    #[test]
    fn roundtrip_empty_pipeline() {
        let input = "+proj=pipeline";
        let p = parse(input).unwrap();
        let output = format!("{p}");
        assert_eq!(output, input);
    }

    // -----------------------------------------------------------------------
    // Error cases
    // -----------------------------------------------------------------------

    #[test]
    fn error_empty_input() {
        let result = parse("");
        assert!(matches!(result, Err(PipelineParseError::EmptyInput)));
    }

    #[test]
    fn error_whitespace_only() {
        let result = parse("   \n\t  ");
        assert!(matches!(result, Err(PipelineParseError::EmptyInput)));
    }

    #[test]
    fn error_not_a_pipeline() {
        let result = parse("+proj=utm +zone=33 +ellps=WGS84");
        assert!(matches!(
            result,
            Err(PipelineParseError::NotAPipeline { .. })
        ));
    }

    #[test]
    fn error_unclosed_single_quote() {
        let result = parse("+proj=pipeline +step +proj=cart +ellps='WGS84");
        assert!(matches!(
            result,
            Err(PipelineParseError::UnclosedQuote { .. })
        ));
    }

    #[test]
    fn error_unclosed_double_quote() {
        let result = parse("+proj=pipeline +step +proj=cart +ellps=\"WGS84");
        assert!(matches!(
            result,
            Err(PipelineParseError::UnclosedQuote { .. })
        ));
    }

    #[test]
    fn error_unexpected_char() {
        let result = parse("+proj=pipeline $ +step +proj=noop");
        assert!(matches!(
            result,
            Err(PipelineParseError::UnexpectedChar { .. })
        ));
    }

    #[test]
    fn error_missing_xy_in() {
        let result = parse("+proj=pipeline +step +proj=unitconvert +xy_out=rad");
        assert!(matches!(
            result,
            Err(PipelineParseError::MissingParameter { .. })
        ));
    }

    #[test]
    fn error_missing_xy_out() {
        let result = parse("+proj=pipeline +step +proj=unitconvert +xy_in=deg");
        assert!(matches!(
            result,
            Err(PipelineParseError::MissingParameter { .. })
        ));
    }

    #[test]
    fn error_missing_ellps() {
        let result = parse("+proj=pipeline +step +proj=cart");
        assert!(matches!(
            result,
            Err(PipelineParseError::MissingParameter { .. })
        ));
    }

    #[test]
    fn error_missing_order() {
        let result = parse("+proj=pipeline +step +proj=axisswap");
        assert!(matches!(
            result,
            Err(PipelineParseError::MissingParameter { .. })
        ));
    }

    #[test]
    fn error_invalid_order_value() {
        let result = parse("+proj=pipeline +step +proj=axisswap +order=abc");
        assert!(matches!(
            result,
            Err(PipelineParseError::InvalidValue { .. })
        ));
    }

    #[test]
    fn error_missing_step_separator() {
        let result = parse(
            "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +proj=cart +ellps=WGS84",
        );
        assert!(matches!(
            result,
            Err(PipelineParseError::ExpectedStepSeparator { .. })
        ));
    }

    #[test]
    fn error_missing_proj_token() {
        let result = parse("+proj=pipeline +step +xy_in=deg +xy_out=rad");
        assert!(matches!(
            result,
            Err(PipelineParseError::ExpectedProjection { .. })
        ));
    }

    #[test]
    fn error_invalid_helmert_value() {
        let result = parse("+proj=pipeline +step +proj=helmert +x=abc");
        assert!(matches!(
            result,
            Err(PipelineParseError::InvalidValue { .. })
        ));
    }

    // -----------------------------------------------------------------------
    // Display tests
    // -----------------------------------------------------------------------

    #[test]
    fn display_unitconvert() {
        let step = Step::UnitConvert {
            xy_in: "deg".to_string(),
            xy_out: "rad".to_string(),
        };
        assert_eq!(
            format!("{step}"),
            "+proj=unitconvert +xy_in=deg +xy_out=rad"
        );
    }

    #[test]
    fn display_helmert() {
        let step = Step::Helmert {
            dx: 1.0,
            dy: 2.0,
            dz: 3.0,
            rx: 4.0,
            ry: 5.0,
            rz: 6.0,
            s: 7.0,
            t: None,
            convention: "position_vector".to_string(),
        };
        assert_eq!(
            format!("{step}"),
            "+proj=helmert +x=1 +y=2 +z=3 +rx=4 +ry=5 +rz=6 +s=7 +convention=position_vector"
        );
    }

    #[test]
    fn display_axisswap() {
        let step = Step::AxisSwap { order: vec![2, 1] };
        assert_eq!(format!("{step}"), "+proj=axisswap +order=2,1");
    }

    #[test]
    fn display_noop() {
        assert_eq!(format!("{}", Step::NoOp), "+proj=noop");
    }

    #[test]
    fn display_push() {
        let step = Step::Push { v: vec![0.0] };
        assert_eq!(format!("{step}"), "+proj=push +v_1=0");
    }

    #[test]
    fn display_pop() {
        let step = Step::Pop { v: vec![0.0] };
        assert_eq!(format!("{step}"), "+proj=pop +v_1=0");
    }

    #[test]
    fn display_projection() {
        let step = Step::Projection {
            method: "lcc".to_string(),
            params: vec![
                ("lat_0".to_string(), "-37".to_string()),
                ("lon_0".to_string(), "145".to_string()),
            ],
            inverse: false,
        };
        assert_eq!(
            format!("{step}"),
            "+proj=lcc +lat_0=-37 +lon_0=145"
        );
    }

    #[test]
    fn display_inverse_projection() {
        let step = Step::Projection {
            method: "lcc".to_string(),
            params: vec![("lat_0".to_string(), "-37".to_string())],
            inverse: true,
        };
        assert_eq!(format!("{step}"), "+inv +proj=lcc +lat_0=-37");
    }

    // -----------------------------------------------------------------------
    // Helper for tests
    // -----------------------------------------------------------------------

    impl Step {
        fn find_param_value<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
            params
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        }
    }
}
