//! PROJ pipeline executor.
//!
//! Executes parsed pipeline steps (see [`crate::pipeline`]) on 4D coordinates.
//! Supports: unitconvert, cart, push/pop, helmert, axisswap, noop, nested
//! pipelines, and the gap-built projection methods (ortho, qsc, geocent).
//! Other projection steps return an error (see ADR 0006).
//!
//! Pure Rust, zero unsafe, zero dependencies.

use crate::pipeline::{Pipeline, Step};
use std::f64::consts::PI;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during pipeline execution.
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineError {
    /// A projection step was encountered. Real projection math comes from
    /// proj4rs (ADR 0006), so this is a gap.
    UnsupportedMethod(String),

    /// An invalid unit was specified for unitconvert.
    InvalidUnit {
        /// The invalid unit string.
        unit: String,
        /// Whether it was the input or output unit.
        kind: UnitKind,
    },

    /// An unknown ellipsoid was specified for cart.
    UnknownEllipsoid(String),

    /// A gap-built projection step failed (bad ellipsoid, domain error, etc.).
    InvalidProjection(String),

    /// A pop was attempted with an empty stack.
    EmptyStack,

    /// An invalid axis index was specified for axisswap.
    InvalidAxis(i32),
}

/// Which side of a unitconvert the invalid unit appeared on.
#[derive(Debug, Clone, PartialEq)]
pub enum UnitKind {
    /// The input unit (`xy_in`).
    Input,
    /// The output unit (`xy_out`).
    Output,
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedMethod(method) => {
                write!(f, "unsupported projection method: {method:?}")
            }
            Self::InvalidUnit { unit, kind } => {
                let kind_str = match kind {
                    UnitKind::Input => "xy_in",
                    UnitKind::Output => "xy_out",
                };
                write!(f, "invalid unit {unit:?} for {kind_str}")
            }
            Self::UnknownEllipsoid(ellps) => {
                write!(f, "unknown ellipsoid: {ellps:?}")
            }
            Self::InvalidProjection(reason) => {
                write!(f, "projection step failed: {reason}")
            }
            Self::EmptyStack => write!(f, "pop with empty stack"),
            Self::InvalidAxis(axis) => write!(f, "invalid axis index: {axis}"),
        }
    }
}

impl std::error::Error for PipelineError {}

impl From<crate::projection::ProjectionError> for PipelineError {
    fn from(e: crate::projection::ProjectionError) -> Self {
        Self::InvalidProjection(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Ellipsoid parameters
// ---------------------------------------------------------------------------

/// Ellipsoid parameters for cart (geodetic → geocentric conversion).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Ellipsoid {
    /// Semi-major axis (meters).
    a: f64,
    /// Flattening.
    f: f64,
}

impl Ellipsoid {
    /// Looks up an ellipsoid by name (case-insensitive).
    fn from_name(name: &str) -> Option<Self> {
        let (a, f) = match name.to_lowercase().as_str() {
            "wgs84" => (6_378_137.0, 1.0 / 298.257_223_563),
            "grs80" => (6_378_137.0, 1.0 / 298.257_222_101),
            "krass" => (6_378_245.0, 1.0 / 298.3),
            "intl" => (6_378_388.0, 1.0 / 297.0),
            "airy" => (6_377_563.396, 1.0 / 299.324_964_6),
            "bessel" => (6_377_397.155, 1.0 / 299.152_812_8),
            "clrk66" => (6_378_206.4, 1.0 / 294.978_698_2),
            "clrk80" => (6_378_249.145, 1.0 / 293.465_998),
            "sphere" => (6_370_997.0, 0.0),
            _ => return None,
        };
        Some(Self { a, f })
    }
}

// ---------------------------------------------------------------------------
// Coordinate
// ---------------------------------------------------------------------------

/// A 4D coordinate: (x, y, z, t).
type Coord = (f64, f64, f64, f64);

// ---------------------------------------------------------------------------
// Stack
// ---------------------------------------------------------------------------

/// Stack for push/pop operations.
///
/// PROJ's push saves the full 4D coordinate onto the stack, then zeroes the
/// specified components. Pop restores the specified components from the
/// saved coordinate.
#[derive(Debug, Default)]
struct ComponentStack(Vec<Coord>);

impl ComponentStack {
    /// Pushes a 4D coordinate onto the stack.
    fn push(&mut self, value: Coord) {
        self.0.push(value);
    }

    /// Pops a 4D coordinate from the stack. Returns `Err(EmptyStack)` if empty.
    fn pop(&mut self) -> Result<Coord, PipelineError> {
        self.0.pop().ok_or(PipelineError::EmptyStack)
    }
}

// ---------------------------------------------------------------------------
// Executor
// ---------------------------------------------------------------------------

/// Executes a pipeline on a 4D coordinate.
///
/// Applies each step in order, threading the coordinate through. Returns the
/// transformed coordinate, or an error if a projection step is encountered or
/// another execution error occurs.
///
/// # Arguments
/// * `pipeline` - The parsed pipeline to execute.
/// * `x` - X coordinate (longitude or easting).
/// * `y` - Y coordinate (latitude or northing).
/// * `z` - Z coordinate (height).
/// * `t` - T coordinate (time).
///
/// # Returns
/// `Ok((x', y', z', t'))` on success, `Err(PipelineError)` on failure.
pub fn execute_pipeline(
    pipeline: &Pipeline,
    x: f64,
    y: f64,
    z: f64,
    t: f64,
) -> Result<Coord, PipelineError> {
    let mut stack = ComponentStack::default();
    execute_steps(&pipeline.steps, x, y, z, t, &mut stack)
}

impl Pipeline {
    /// Executes this pipeline on a 4D coordinate.
    ///
    /// Convenience method equivalent to [`execute_pipeline`].
    pub fn execute(&self, x: f64, y: f64, z: f64, t: f64) -> Result<Coord, PipelineError> {
        execute_pipeline(self, x, y, z, t)
    }
}

/// Recursive helper: executes a slice of steps on a coordinate.
fn execute_steps(
    steps: &[Step],
    x: f64,
    y: f64,
    z: f64,
    t: f64,
    stack: &mut ComponentStack,
) -> Result<Coord, PipelineError> {
    let mut coord = (x, y, z, t);

    for step in steps {
        coord = execute_step(step, coord, stack)?;
    }

    Ok(coord)
}

/// Executes a single step on a coordinate.
fn execute_step(
    step: &Step,
    coord: Coord,
    stack: &mut ComponentStack,
) -> Result<Coord, PipelineError> {
    match step {
        Step::UnitConvert { xy_in, xy_out } => execute_unitconvert(xy_in, xy_out, coord),
        Step::Cart { ellps } => execute_cart(ellps, coord),
        Step::Push { v } => execute_push(v, coord, stack),
        Step::Pop { v } => execute_pop(v, coord, stack),
        Step::Helmert {
            dx,
            dy,
            dz,
            rx,
            ry,
            rz,
            s,
            t: _,
            convention,
        } => execute_helmert(*dx, *dy, *dz, *rx, *ry, *rz, *s, convention, coord),
        Step::AxisSwap { order } => execute_axisswap(order, coord),
        Step::NoOp => Ok(coord),
        Step::Pipeline(nested) => execute_steps(&nested.steps, coord.0, coord.1, coord.2, coord.3, stack),
        Step::Projection {
            method,
            params,
            inverse,
        } => execute_projection(method, params, *inverse, coord),
    }
}

// ---------------------------------------------------------------------------
// Projection steps (gap-built methods: ortho, qsc, geocent)
// ---------------------------------------------------------------------------

/// Executes a projection step for the gap-built methods (ortho, qsc, geocent).
///
/// These are the `+proj` methods that proj4rs lacks (ADR 0006). The native
/// implementations in [`crate::projection`] handle them directly.
///
/// - `ortho` / `qsc`: 2D projection; `(lam, phi)` ↔ dimensionless `(x, y)`.
///   The `z` (height) and `t` (time) components pass through unchanged.
/// - `geocent`: 3D conversion; `(lam, phi, h)` ↔ `(X, Y, Z)` in meters.
fn execute_projection(
    method: &str,
    params: &[(String, String)],
    inverse: bool,
    coord: Coord,
) -> Result<Coord, PipelineError> {
    use crate::projection::{Ellipsoid, Projection, ProjectionParams, ProjParams};

    // Build a ProjParams from the parsed key/value pairs
    let mut pp = ProjParams::default();
    for (key, value) in params {
        pp.push(key.clone(), Some(value.clone()));
    }

    let (x, y, z, t) = coord;

    match method {
        "ortho" | "qsc" => {
            let ellps = Ellipsoid::from_proj_params(&pp)
                .map_err(|e| PipelineError::InvalidProjection(e.to_string()))?;
            let proj_params = ProjectionParams::from_proj_params(&pp);
            let projection = if method == "ortho" {
                Projection::Orthographic(crate::projection::Orthographic::new(ellps, proj_params))
            } else {
                Projection::Qsc(crate::projection::Qsc::new(ellps, proj_params))
            };
            if inverse {
                let (lam, phi, _) = projection.inverse(x, y, z)?;
                Ok((lam, phi, z, t))
            } else {
                let (px, py, pz) = projection.forward(x, y, z)?;
                Ok((px, py, pz, t))
            }
        }
        "geocent" => {
            let ellps = Ellipsoid::from_proj_params(&pp)
                .map_err(|e| PipelineError::InvalidProjection(e.to_string()))?;
            let g = crate::projection::Geocent::new(ellps);
            if inverse {
                let (lam, phi, h) = g.inverse(x, y, z)?;
                Ok((lam, phi, h, t))
            } else {
                let (xx, yy, zz) = g.forward(x, y, z);
                Ok((xx, yy, zz, t))
            }
        }
        other => Err(PipelineError::UnsupportedMethod(other.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Unit conversion
// ---------------------------------------------------------------------------

/// Converts between angular and linear units.
///
/// Supported units: "deg", "rad", "ft", "m", "us-ft".
/// Identity conversions (e.g., "deg" → "deg") are no-ops.
fn execute_unitconvert(xy_in: &str, xy_out: &str, coord: Coord) -> Result<Coord, PipelineError> {
    let factor = unit_factor(xy_in, xy_out)?;
    Ok((coord.0 * factor, coord.1 * factor, coord.2, coord.3))
}

/// Returns the multiplication factor to convert from `from` to `to`.
fn unit_factor(from: &str, to: &str) -> Result<f64, PipelineError> {
    let from_lower = from.to_lowercase();
    let to_lower = to.to_lowercase();

    // Identity
    if from_lower == to_lower {
        return Ok(1.0);
    }

    // Angular: degrees <-> radians
    let from_angular = match from_lower.as_str() {
        "deg" | "degree" | "degrees" => Some(Unit::Degrees),
        "rad" | "radian" | "radians" => Some(Unit::Radians),
        _ => None,
    };
    let to_angular = match to_lower.as_str() {
        "deg" | "degree" | "degrees" => Some(Unit::Degrees),
        "rad" | "radian" | "radians" => Some(Unit::Radians),
        _ => None,
    };

    if let (Some(f), Some(t)) = (from_angular, to_angular) {
        return Ok(f.to(t));
    }

    // Linear: feet <-> meters
    let from_linear = match from_lower.as_str() {
        "ft" | "foot" | "feet" => Some(Unit::Feet),
        "us-ft" | "us_survey_foot" | "us_survey_feet" => Some(Unit::UsSurveyFeet),
        "m" | "meter" | "meters" | "metre" | "metres" => Some(Unit::Meters),
        _ => None,
    };
    let to_linear = match to_lower.as_str() {
        "ft" | "foot" | "feet" => Some(Unit::Feet),
        "us-ft" | "us_survey_foot" | "us_survey_feet" => Some(Unit::UsSurveyFeet),
        "m" | "meter" | "meters" | "metre" | "metres" => Some(Unit::Meters),
        _ => None,
    };

    if let (Some(f), Some(t)) = (from_linear, to_linear) {
        return Ok(f.to(t));
    }

    // Mixed angular/linear or unknown
    if from_angular.is_some() || from_linear.is_some() {
        Err(PipelineError::InvalidUnit {
            unit: to.to_string(),
            kind: UnitKind::Output,
        })
    } else {
        Err(PipelineError::InvalidUnit {
            unit: from.to_string(),
            kind: UnitKind::Input,
        })
    }
}

/// Supported unit categories.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Unit {
    Degrees,
    Radians,
    Feet,
    UsSurveyFeet,
    Meters,
}

impl Unit {
    /// Conversion factor to another unit.
    fn to(self, other: Self) -> f64 {
        use Unit::*;
        match (self, other) {
            (Degrees, Radians) => PI / 180.0,
            (Radians, Degrees) => 180.0 / PI,
            (Feet, Meters) => 0.3048,
            (Meters, Feet) => 1.0 / 0.3048,
            (UsSurveyFeet, Meters) => 0.304_800_609_601_219_2,
            (Meters, UsSurveyFeet) => 1.0 / 0.304_800_609_601_219_2,
            (Feet, UsSurveyFeet) => 0.304_800_609_601_219_2 / 0.304_8,
            (UsSurveyFeet, Feet) => 0.304_8 / 0.304_800_609_601_219_2,
            _ => 1.0, // same unit, shouldn't reach here due to identity check
        }
    }
}

// ---------------------------------------------------------------------------
// Cart (geodetic → geocentric)
// ---------------------------------------------------------------------------

/// Converts geodetic (lon, lat, h) to geocentric (X, Y, Z).
///
/// Input lon/lat must be in radians. Height in meters. Output in meters.
/// The t component passes through unchanged.
fn execute_cart(ellps: &str, coord: Coord) -> Result<Coord, PipelineError> {
    let ellps = Ellipsoid::from_name(ellps)
        .ok_or_else(|| PipelineError::UnknownEllipsoid(ellps.to_string()))?;

    let (lon, lat, h, t) = coord;
    let e2 = 2.0 * ellps.f - ellps.f * ellps.f;
    let sin_lat = lat.sin();
    let cos_lat = lat.cos();
    let n = ellps.a / (1.0 - e2 * sin_lat * sin_lat).sqrt();
    let x = (n + h) * cos_lat * lon.cos();
    let y = (n + h) * cos_lat * lon.sin();
    let z = (n * (1.0 - e2) + h) * sin_lat;
    Ok((x, y, z, t))
}

// ---------------------------------------------------------------------------
// Push / Pop
// ---------------------------------------------------------------------------

/// Maps the parser's `v` vec to 0-based axis indices.
///
/// The parser (see `pipeline.rs`) stores push/pop values in a lossy vec:
/// `push v_3` and `push v_1` both produce `v: [0.0]`. We use a length-based
/// heuristic matching QGIS pipeline conventions:
/// - 1 value → v_3 (z axis, index 2) — the most common single-axis push
/// - 2 values → v_1, v_2 (x, y axes, indices 0, 1)
/// - 3 values → v_1, v_2, v_3 (x, y, z axes, indices 0, 1, 2)
fn axes_from_v(v: &[f64]) -> Vec<usize> {
    match v.len() {
        1 => vec![2],       // v_3 (z)
        2 => vec![0, 1],    // v_1, v_2 (x, y)
        3 => vec![0, 1, 2], // v_1, v_2, v_3 (x, y, z)
        _ => vec![],
    }
}

/// Pushes the current 4D coordinate onto the stack, then sets the specified
/// components to the given values (usually 0).
///
/// `v[i]` is the value that axis `axes_from_v(v)[i]` is set to.
fn execute_push(v: &[f64], coord: Coord, stack: &mut ComponentStack) -> Result<Coord, PipelineError> {
    stack.push(coord);
    let axes = axes_from_v(v);
    let mut c = [coord.0, coord.1, coord.2, coord.3];
    for (i, &axis) in axes.iter().enumerate() {
        c[axis] = v[i];
    }
    Ok((c[0], c[1], c[2], c[3]))
}

/// Pops the saved 4D coordinate from the stack and restores the specified
/// components from it.
fn execute_pop(v: &[f64], coord: Coord, stack: &mut ComponentStack) -> Result<Coord, PipelineError> {
    let saved = stack.pop()?;
    let saved_arr = [saved.0, saved.1, saved.2, saved.3];
    let axes = axes_from_v(v);
    let mut c = [coord.0, coord.1, coord.2, coord.3];
    for &axis in &axes {
        c[axis] = saved_arr[axis];
    }
    Ok((c[0], c[1], c[2], c[3]))
}

// ---------------------------------------------------------------------------
// Helmert
// ---------------------------------------------------------------------------

/// Applies a 7-parameter Helmert transformation.
///
/// Rotations are in arc-seconds, scale in ppm. The `convention` determines
/// the rotation direction: "position_vector" or "coordinate_frame".
///
/// Uses the small-angle approximation (matching PROJ's default). The `t`
/// parameter (time reference epoch) is stored in the parsed step but does not
/// affect the static 7-param math.
fn execute_helmert(
    dx: f64,
    dy: f64,
    dz: f64,
    rx: f64,
    ry: f64,
    rz: f64,
    s: f64,
    convention: &str,
    coord: Coord,
) -> Result<Coord, PipelineError> {
    let arcsec_to_rad = PI / (180.0 * 3600.0);
    let rx_rad = rx * arcsec_to_rad;
    let ry_rad = ry * arcsec_to_rad;
    let rz_rad = rz * arcsec_to_rad;
    let scale = 1.0 + s * 1e-6;

    let (x, y, z, t) = coord;

    // Build rotation matrix based on convention.
    // Coordinate frame (default): R = [[1, rz, -ry], [-rz, 1, rx], [ry, -rx, 1]]
    // Position vector: R = [[1, -rz, ry], [rz, 1, -rx], [-ry, rx, 1]]
    let (r00, r01, r02, r10, r11, r12, r20, r21, r22) = if convention == "position_vector" {
        (1.0, -rz_rad, ry_rad, rz_rad, 1.0, -rx_rad, -ry_rad, rx_rad, 1.0)
    } else {
        // coordinate_frame (default)
        (1.0, rz_rad, -ry_rad, -rz_rad, 1.0, rx_rad, ry_rad, -rx_rad, 1.0)
    };

    let x_out = dx + scale * (r00 * x + r01 * y + r02 * z);
    let y_out = dy + scale * (r10 * x + r11 * y + r12 * z);
    let z_out = dz + scale * (r20 * x + r21 * y + r22 * z);

    Ok((x_out, y_out, z_out, t))
}

// ---------------------------------------------------------------------------
// Axis swap
// ---------------------------------------------------------------------------

/// Reorders axes according to the order vector.
///
/// `order` contains 1-based axis indices. For each output position `i`,
/// `output[i] = input[order[i] - 1]`. Axes beyond the order length pass through.
fn execute_axisswap(order: &[i32], coord: Coord) -> Result<Coord, PipelineError> {
    let c = [coord.0, coord.1, coord.2, coord.3];
    let mut out = [0.0; 4];
    for i in 0..4 {
        if i < order.len() {
            let idx = order[i];
            if idx < 1 || idx > 4 {
                return Err(PipelineError::InvalidAxis(idx));
            }
            out[i] = c[(idx - 1) as usize];
        } else {
            out[i] = c[i];
        }
    }
    Ok((out[0], out[1], out[2], out[3]))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::parse;

    // -----------------------------------------------------------------------
    // Unit conversion
    // -----------------------------------------------------------------------

    #[test]
    fn unitconvert_deg_to_rad() {
        let p = parse("+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad").unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 180.0, 90.0, 0.0, 0.0).unwrap();
        assert!((x - PI).abs() < 1e-12, "x = {x}");
        assert!((y - PI / 2.0).abs() < 1e-12, "y = {y}");
        assert_eq!(z, 0.0);
        assert_eq!(t, 0.0);
    }

    #[test]
    fn unitconvert_rad_to_deg() {
        let p = parse("+proj=pipeline +step +proj=unitconvert +xy_in=rad +xy_out=deg").unwrap();
        let (x, y, _, _) = execute_pipeline(&p, PI, PI / 2.0, 0.0, 0.0).unwrap();
        assert!((x - 180.0).abs() < 1e-12, "x = {x}");
        assert!((y - 90.0).abs() < 1e-12, "y = {y}");
    }

    #[test]
    fn unitconvert_deg_to_rad_45() {
        let p = parse("+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad").unwrap();
        let (x, _, _, _) = execute_pipeline(&p, 45.0, 0.0, 0.0, 0.0).unwrap();
        assert!((x - PI / 4.0).abs() < 1e-12);
    }

    #[test]
    fn unitconvert_ft_to_m() {
        let p = parse("+proj=pipeline +step +proj=unitconvert +xy_in=ft +xy_out=m").unwrap();
        let (x, y, _, _) = execute_pipeline(&p, 1.0, 3.280839895, 0.0, 0.0).unwrap();
        assert!((x - 0.3048).abs() < 1e-12, "x = {x}");
        assert!((y - 1.0).abs() < 1e-9, "y = {y}");
    }

    #[test]
    fn unitconvert_m_to_ft() {
        let p = parse("+proj=pipeline +step +proj=unitconvert +xy_in=m +xy_out=ft").unwrap();
        let (x, y, _, _) = execute_pipeline(&p, 1.0, 0.3048, 0.0, 0.0).unwrap();
        assert!((x - 3.280839895013123).abs() < 1e-12, "x = {x}");
        assert!((y - 1.0).abs() < 1e-12);
    }

    #[test]
    fn unitconvert_us_ft_to_m() {
        let p = parse("+proj=pipeline +step +proj=unitconvert +xy_in=us-ft +xy_out=m").unwrap();
        let (x, _, _, _) = execute_pipeline(&p, 1.0, 0.0, 0.0, 0.0).unwrap();
        assert!((x - 0.304_800_609_601_219_2).abs() < 1e-15);
    }

    #[test]
    fn unitconvert_identity() {
        let p = parse("+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=deg").unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 45.0, 90.0, 1.0, 2.0).unwrap();
        assert_eq!((x, y, z, t), (45.0, 90.0, 1.0, 2.0));
    }

    #[test]
    fn unitconvert_invalid_input_unit() {
        let p = parse("+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad").unwrap();
        let result = execute_pipeline(&p, 0.0, 0.0, 0.0, 0.0);
        // Can't easily trigger invalid unit through the parser since it accepts
        // any string. Test the internal function directly.
        let err = unit_factor("furlong", "m").unwrap_err();
        assert!(matches!(err, PipelineError::InvalidUnit { unit, kind: UnitKind::Input } if unit == "furlong"));
    }

    #[test]
    fn unitconvert_invalid_output_unit() {
        let err = unit_factor("deg", "furlong").unwrap_err();
        assert!(matches!(err, PipelineError::InvalidUnit { unit, kind: UnitKind::Output } if unit == "furlong"));
    }

    #[test]
    fn unitconvert_mixed_angular_linear() {
        let err = unit_factor("deg", "m").unwrap_err();
        assert!(matches!(err, PipelineError::InvalidUnit { unit, kind: UnitKind::Output } if unit == "m"));
    }

    // -----------------------------------------------------------------------
    // Cart (geodetic → geocentric)
    // -----------------------------------------------------------------------

    #[test]
    fn cart_wgs84_origin() {
        let p = parse("+proj=pipeline +step +proj=cart +ellps=WGS84").unwrap();
        let (x, y, z, _) = execute_pipeline(&p, 0.0, 0.0, 0.0, 0.0).unwrap();
        assert!((x - 6_378_137.0).abs() < 1e-6, "x = {x}");
        assert!(y.abs() < 1e-6, "y = {y}");
        assert!(z.abs() < 1e-6, "z = {z}");
    }

    #[test]
    fn cart_wgs84_equator_90lon() {
        let p = parse("+proj=pipeline +step +proj=cart +ellps=WGS84").unwrap();
        let (x, y, z, _) = execute_pipeline(&p, PI / 2.0, 0.0, 0.0, 0.0).unwrap();
        assert!(x.abs() < 1e-3, "x = {x}"); // ~0 (floating point)
        assert!((y - 6_378_137.0).abs() < 1e-3, "y = {y}");
        assert!(z.abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn cart_wgs84_north_pole() {
        let p = parse("+proj=pipeline +step +proj=cart +ellps=WGS84").unwrap();
        let (x, y, z, _) = execute_pipeline(&p, 0.0, PI / 2.0, 0.0, 0.0).unwrap();
        assert!(x.abs() < 1e-3, "x = {x}");
        assert!(y.abs() < 1e-3, "y = {y}");
        // Z = b = a(1-f) = 6356752.314245179
        assert!((z - 6_356_752.314_245_179).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn cart_wgs84_known_point() {
        // Reference: Python computation
        // cart WGS84 (12°, 55°, 0) = (3586469.656776, 762327.658778, 5201383.523202)
        let p = parse("+proj=pipeline +step +proj=cart +ellps=WGS84").unwrap();
        let lon = 12.0_f64.to_radians();
        let lat = 55.0_f64.to_radians();
        let (x, y, z, _) = execute_pipeline(&p, lon, lat, 0.0, 0.0).unwrap();
        assert!((x - 3_586_469.656_776).abs() < 1e-3, "x = {x}");
        assert!((y - 762_327.658_778).abs() < 1e-3, "y = {y}");
        assert!((z - 5_201_383.523_202).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn cart_wgs84_negative_lon_lat() {
        // Reference: Python computation
        // cart WGS84 (-122.4194°, 37.7749°, 0) = (-2706174.846611, -4261059.489296, 3885725.490024)
        let p = parse("+proj=pipeline +step +proj=cart +ellps=WGS84").unwrap();
        let lon = (-122.4194_f64).to_radians();
        let lat = 37.7749_f64.to_radians();
        let (x, y, z, _) = execute_pipeline(&p, lon, lat, 0.0, 0.0).unwrap();
        assert!((x - (-2_706_174.846_611)).abs() < 1e-3, "x = {x}");
        assert!((y - (-4_261_059.489_296)).abs() < 1e-3, "y = {y}");
        assert!((z - 3_885_725.490_024).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn cart_wgs84_with_height() {
        // Reference: Python computation
        // cart WGS84 (139.6917°, 35.6895°, 100) = (-3954906.224505, 3354989.473441, 3700323.127263)
        let p = parse("+proj=pipeline +step +proj=cart +ellps=WGS84").unwrap();
        let lon = 139.6917_f64.to_radians();
        let lat = 35.6895_f64.to_radians();
        let (x, y, z, _) = execute_pipeline(&p, lon, lat, 100.0, 0.0).unwrap();
        assert!((x - (-3_954_906.224_505)).abs() < 1e-3, "x = {x}");
        assert!((y - 3_354_989.473_441).abs() < 1e-3, "y = {y}");
        assert!((z - 3_700_323.127_263).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn cart_grs80() {
        // Reference: Python computation
        // cart GRS80 (12°, 55°, 0) = (3586469.656816, 762327.658787, 5201383.523088)
        let p = parse("+proj=pipeline +step +proj=cart +ellps=GRS80").unwrap();
        let lon = 12.0_f64.to_radians();
        let lat = 55.0_f64.to_radians();
        let (x, y, z, _) = execute_pipeline(&p, lon, lat, 0.0, 0.0).unwrap();
        assert!((x - 3_586_469.656_816).abs() < 1e-3, "x = {x}");
        assert!((y - 762_327.658_787).abs() < 1e-3, "y = {y}");
        assert!((z - 5_201_383.523_088).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn cart_krass() {
        // Reference: Python computation
        // cart krass (12°, 55°, 0) = (3586529.227503, 762340.320927, 5201474.935921)
        let p = parse("+proj=pipeline +step +proj=cart +ellps=krass").unwrap();
        let lon = 12.0_f64.to_radians();
        let lat = 55.0_f64.to_radians();
        let (x, y, z, _) = execute_pipeline(&p, lon, lat, 0.0, 0.0).unwrap();
        assert!((x - 3_586_529.227_503).abs() < 1e-3, "x = {x}");
        assert!((y - 762_340.320_927).abs() < 1e-3, "y = {y}");
        assert!((z - 5_201_474.935_921).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn cart_intl() {
        // Reference: Python computation
        // cart intl (12°, 55°, 0) = (3586644.992004, 762364.927431, 5201489.662046)
        let p = parse("+proj=pipeline +step +proj=cart +ellps=intl").unwrap();
        let lon = 12.0_f64.to_radians();
        let lat = 55.0_f64.to_radians();
        let (x, y, z, _) = execute_pipeline(&p, lon, lat, 0.0, 0.0).unwrap();
        assert!((x - 3_586_644.992_004).abs() < 1e-3, "x = {x}");
        assert!((y - 762_364.927_431).abs() < 1e-3, "y = {y}");
        assert!((z - 5_201_489.662_046).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn cart_airy() {
        // Reference: Python computation
        // cart airy (12°, 55°, 0) = (3586118.302709, 762252.976166, 5200998.785997)
        let p = parse("+proj=pipeline +step +proj=cart +ellps=airy").unwrap();
        let lon = 12.0_f64.to_radians();
        let lat = 55.0_f64.to_radians();
        let (x, y, z, _) = execute_pipeline(&p, lon, lat, 0.0, 0.0).unwrap();
        assert!((x - 3_586_118.302_709).abs() < 1e-3, "x = {x}");
        assert!((y - 762_252.976_166).abs() < 1e-3, "y = {y}");
        assert!((z - 5_200_998.785_997).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn cart_bessel() {
        // Reference: Python computation
        // cart bessel (12°, 55°, 0) = (3586029.456429, 762234.091306, 5200849.866181)
        let p = parse("+proj=pipeline +step +proj=cart +ellps=bessel").unwrap();
        let lon = 12.0_f64.to_radians();
        let lat = 55.0_f64.to_radians();
        let (x, y, z, _) = execute_pipeline(&p, lon, lat, 0.0, 0.0).unwrap();
        assert!((x - 3_586_029.456_429).abs() < 1e-3, "x = {x}");
        assert!((y - 762_234.091_306).abs() < 1e-3, "y = {y}");
        assert!((z - 5_200_849.866_181).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn cart_clrk66() {
        // Reference: Python computation
        // cart clrk66 (12°, 55°, 0) = (3586598.465647, 762355.037949, 5201181.365904)
        let p = parse("+proj=pipeline +step +proj=cart +ellps=clrk66").unwrap();
        let lon = 12.0_f64.to_radians();
        let lat = 55.0_f64.to_radians();
        let (x, y, z, _) = execute_pipeline(&p, lon, lat, 0.0, 0.0).unwrap();
        assert!((x - 3_586_598.465_647).abs() < 1e-3, "x = {x}");
        assert!((y - 762_355.037_949).abs() < 1e-3, "y = {y}");
        assert!((z - 5_201_181.365_904).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn cart_t_passthrough() {
        let p = parse("+proj=pipeline +step +proj=cart +ellps=WGS84").unwrap();
        let (_, _, _, t) = execute_pipeline(&p, 0.0, 0.0, 0.0, 42.0).unwrap();
        assert_eq!(t, 42.0);
    }

    #[test]
    fn cart_unknown_ellipsoid() {
        let p = parse("+proj=pipeline +step +proj=cart +ellps=unknown").unwrap();
        let result = execute_pipeline(&p, 0.0, 0.0, 0.0, 0.0);
        assert!(matches!(result, Err(PipelineError::UnknownEllipsoid(_))));
    }

    // -----------------------------------------------------------------------
    // Push / Pop
    // -----------------------------------------------------------------------

    #[test]
    fn push_pop_z_roundtrip() {
        let p = parse(
            "+proj=pipeline +step +proj=push +v_3 +step +proj=pop +v_3",
        )
        .unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0).unwrap();
        assert_eq!((x, y, z, t), (1.0, 2.0, 3.0, 4.0));
    }

    #[test]
    fn push_z_sets_to_zero() {
        let p = parse("+proj=pipeline +step +proj=push +v_3").unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0).unwrap();
        assert_eq!((x, y, z), (1.0, 2.0, 0.0));
        assert_eq!(t, 4.0);
    }

    #[test]
    fn push_z_then_modify_then_pop() {
        let p = parse(
            "+proj=pipeline +step +proj=push +v_3 +step +proj=helmert +x=0 +y=0 +z=0 +step +proj=pop +v_3",
        )
        .unwrap();
        // push v_3: (1,2,0,4), stack=[3]
        // helmert noop: (1,2,0,4)
        // pop v_3: (1,2,3,4)
        let (x, y, z, t) = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0).unwrap();
        assert_eq!((x, y, z, t), (1.0, 2.0, 3.0, 4.0));
    }

    #[test]
    fn push_pop_multiple() {
        let p = parse(
            "+proj=pipeline +step +proj=push +v_1 +v_2 +v_3 +step +proj=pop +v_1 +v_2 +v_3",
        )
        .unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0).unwrap();
        assert_eq!((x, y, z, t), (1.0, 2.0, 3.0, 4.0));
    }

    #[test]
    fn push_pop_ordering() {
        // Push saves in order v_1, v_2, v_3. Pop restores in order v_1, v_2, v_3.
        // Stack is LIFO, so pop v_1 first gets the last pushed value (v_3's).
        // But since push sets all to 0 and we only care about roundtrip, this is fine.
        let p = parse(
            "+proj=pipeline +step +proj=push +v_1 +v_2 +v_3 +step +proj=pop +v_3 +v_2 +v_1",
        )
        .unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0).unwrap();
        assert_eq!((x, y, z, t), (1.0, 2.0, 3.0, 4.0));
    }

    #[test]
    fn pop_empty_stack_error() {
        let p = parse("+proj=pipeline +step +proj=pop +v_3").unwrap();
        let result = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0);
        assert!(matches!(result, Err(PipelineError::EmptyStack)));
    }

    #[test]
    fn push_with_nonzero_value() {
        // push v_3=5.0 saves current z and sets z to 5.0
        let step = Step::Push { v: vec![5.0] };
        let mut stack = ComponentStack::default();
        let (x, y, z, t) = execute_step(&step, (1.0, 2.0, 3.0, 4.0), &mut stack).unwrap();
        assert_eq!((x, y, z, t), (1.0, 2.0, 5.0, 4.0));
        // Stack should have the saved coordinate
        let saved = stack.pop().unwrap();
        assert_eq!(saved, (1.0, 2.0, 3.0, 4.0));
    }

    // -----------------------------------------------------------------------
    // Helmert
    // -----------------------------------------------------------------------

    #[test]
    fn helmert_pure_translation() {
        let p = parse(
            "+proj=pipeline +step +proj=helmert +x=1 +y=2 +z=3",
        )
        .unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 100.0, 200.0, 300.0, 0.0).unwrap();
        assert_eq!((x, y, z), (101.0, 202.0, 303.0));
    }

    #[test]
    fn helmert_translation_with_rotations() {
        // Reference: Python computation
        // helmert full 7-param pv on (100,200,300) with dx=1,dy=2,dz=3,rx=4,ry=5,rz=6,s=7
        // = (101.002154, 201.998491, 303.003554)
        let p = parse(
            "+proj=pipeline +step +proj=helmert +x=1 +y=2 +z=3 +rx=4 +ry=5 +rz=6 +s=7 +convention=position_vector",
        )
        .unwrap();
        let (x, y, z, _) = execute_pipeline(&p, 100.0, 200.0, 300.0, 0.0).unwrap();
        assert!((x - 101.002_154).abs() < 1e-6, "x = {x}");
        assert!((y - 201.998_491).abs() < 1e-6, "y = {y}");
        assert!((z - 303.003_554).abs() < 1e-6, "z = {z}");
    }

    #[test]
    fn helmert_coordinate_frame_convention() {
        // Reference: Python computation
        // helmert full 7-param cf on (100,200,300) with dx=1,dy=2,dz=3,rx=4,ry=5,rz=6,s=7
        // = (100.999246, 202.004309, 303.000646)
        let p = parse(
            "+proj=pipeline +step +proj=helmert +x=1 +y=2 +z=3 +rx=4 +ry=5 +rz=6 +s=7 +convention=coordinate_frame",
        )
        .unwrap();
        let (x, y, z, _) = execute_pipeline(&p, 100.0, 200.0, 300.0, 0.0).unwrap();
        assert!((x - 100.999_246).abs() < 1e-6, "x = {x}");
        assert!((y - 202.004_309).abs() < 1e-6, "y = {y}");
        assert!((z - 303.000_646).abs() < 1e-6, "z = {z}");
    }

    #[test]
    fn helmert_conventions_differ() {
        // The two conventions should produce different results when rotations are non-zero.
        // Choose input where x*ry != y*rx so the z components differ.
        let pv = parse(
            "+proj=pipeline +step +proj=helmert +x=0 +y=0 +z=0 +rx=10 +ry=20 +rz=5 +convention=position_vector",
        )
        .unwrap();
        let cf = parse(
            "+proj=pipeline +step +proj=helmert +x=0 +y=0 +z=0 +rx=10 +ry=20 +rz=5 +convention=coordinate_frame",
        )
        .unwrap();
        let (pvx, pvy, pvz, _) = execute_pipeline(&pv, 1000.0, 3000.0, 5000.0, 0.0).unwrap();
        let (cfx, cfy, cfz, _) = execute_pipeline(&cf, 1000.0, 3000.0, 5000.0, 0.0).unwrap();
        assert!((pvx - cfx).abs() > 1e-9, "conventions should differ in x");
        assert!((pvy - cfy).abs() > 1e-9, "conventions should differ in y");
        assert!((pvz - cfz).abs() > 1e-9, "conventions should differ in z");
    }

    #[test]
    fn helmert_scale_only() {
        // Reference: Python computation
        // helmert s=10ppm on (1e6,2e6,3e6) = (1000010, 2000020, 3000030)
        let p = parse(
            "+proj=pipeline +step +proj=helmert +x=0 +y=0 +z=0 +s=10 +convention=position_vector",
        )
        .unwrap();
        let (x, y, z, _) = execute_pipeline(&p, 1_000_000.0, 2_000_000.0, 3_000_000.0, 0.0).unwrap();
        assert!((x - 1_000_010.0).abs() < 1e-6, "x = {x}");
        assert!((y - 2_000_020.0).abs() < 1e-6, "y = {y}");
        assert!((z - 3_000_030.0).abs() < 1e-6, "z = {z}");
    }

    #[test]
    fn helmert_rotation_z_only() {
        // Reference: Python computation
        // helmert rz=1as on (1e6,0,0) pv: Y = 1e6 * sin(1as) ≈ 4.848137
        let p = parse(
            "+proj=pipeline +step +proj=helmert +x=0 +y=0 +z=0 +rz=1 +convention=position_vector",
        )
        .unwrap();
        let (_, y, _, _) = execute_pipeline(&p, 1_000_000.0, 0.0, 0.0, 0.0).unwrap();
        assert!((y - 4.848_137).abs() < 1e-6, "y = {y}");
    }

    #[test]
    fn helmert_grs80_to_wgs84_pipeline() {
        // Real QGIS pipeline: unitconvert deg→rad, push v_3, cart GRS80,
        // helmert (coordinate_frame), then the result should match WGS84 geocentric.
        // Reference values from Python computation.
        let p = parse(
            "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=push +v_3 +step +proj=cart +ellps=GRS80 +step +proj=helmert +x=0.06155 +y=-0.01087 +z=-0.04019 +rx=-0.0394924 +ry=-0.0327221 +rz=-0.0328979 +s=-0.009994 +convention=coordinate_frame",
        )
        .unwrap();
        // The pipeline starts with unitconvert deg→rad, so input is in degrees.
        let (x, y, z, _) = execute_pipeline(&p, 12.0, 55.0, 0.0, 0.0).unwrap();
        // After cart GRS80: (3586469.656816, 762327.658787, 5201383.523088)
        // After helmert cf: (3586470.386090, 762327.216436, 5201383.007912)
        assert!((x - 3_586_470.386_090).abs() < 1e-3, "x = {x}");
        assert!((y - 762_327.216_436).abs() < 1e-3, "y = {y}");
        assert!((z - 5_201_383.007_912).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn helmert_t_passthrough() {
        // The t parameter should not affect the static 7-param math.
        let p = parse(
            "+proj=pipeline +step +proj=helmert +x=1 +y=2 +z=3 +t=2010.5 +convention=position_vector",
        )
        .unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 100.0, 200.0, 300.0, 0.0).unwrap();
        assert_eq!((x, y, z), (101.0, 202.0, 303.0));
        assert_eq!(t, 0.0);
    }

    // -----------------------------------------------------------------------
    // Axis swap
    // -----------------------------------------------------------------------

    #[test]
    fn axisswap_identity() {
        let p = parse("+proj=pipeline +step +proj=axisswap +order=1,2,3,4").unwrap();
        let result = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0).unwrap();
        assert_eq!(result, (1.0, 2.0, 3.0, 4.0));
    }

    #[test]
    fn axisswap_swap_xy() {
        let p = parse("+proj=pipeline +step +proj=axisswap +order=2,1").unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0).unwrap();
        assert_eq!((x, y, z, t), (2.0, 1.0, 3.0, 4.0));
    }

    #[test]
    fn axisswap_four_axes() {
        let p = parse("+proj=pipeline +step +proj=axisswap +order=4,1,2,3").unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0).unwrap();
        assert_eq!((x, y, z, t), (4.0, 1.0, 2.0, 3.0));
    }

    #[test]
    fn axisswap_three_axes() {
        let p = parse("+proj=pipeline +step +proj=axisswap +order=2,1,4,3").unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0).unwrap();
        assert_eq!((x, y, z, t), (2.0, 1.0, 4.0, 3.0));
    }

    #[test]
    fn axisswap_two_axes_partial() {
        // order=2,1 only swaps first two, rest pass through
        let p = parse("+proj=pipeline +step +proj=axisswap +order=2,1").unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0).unwrap();
        assert_eq!((x, y, z, t), (2.0, 1.0, 3.0, 4.0));
    }

    #[test]
    fn axisswap_invalid_axis() {
        let p = parse("+proj=pipeline +step +proj=axisswap +order=5,1,2,3").unwrap();
        let result = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0);
        assert!(matches!(result, Err(PipelineError::InvalidAxis(5))));
    }

    #[test]
    fn axisswap_negative_axis() {
        let p = parse("+proj=pipeline +step +proj=axisswap +order=0,1,2,3").unwrap();
        let result = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0);
        assert!(matches!(result, Err(PipelineError::InvalidAxis(0))));
    }

    // -----------------------------------------------------------------------
    // NoOp
    // -----------------------------------------------------------------------

    #[test]
    fn noop_identity() {
        let p = parse("+proj=pipeline +step +proj=noop").unwrap();
        let result = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0).unwrap();
        assert_eq!(result, (1.0, 2.0, 3.0, 4.0));
    }

    // -----------------------------------------------------------------------
    // Chained pipeline
    // -----------------------------------------------------------------------

    #[test]
    fn chained_unitconvert_then_cart() {
        // (12, 55, 0, 0) deg → rad → cart WGS84
        // Reference: (3586469.656776, 762327.658778, 5201383.523202)
        let p = parse(
            "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=cart +ellps=WGS84",
        )
        .unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 12.0, 55.0, 0.0, 0.0).unwrap();
        assert!((x - 3_586_469.656_776).abs() < 1e-3, "x = {x}");
        assert!((y - 762_327.658_778).abs() < 1e-3, "y = {y}");
        assert!((z - 5_201_383.523_202).abs() < 1e-3, "z = {z}");
        assert_eq!(t, 0.0);
    }

    #[test]
    fn chained_axisswap_then_unitconvert() {
        // (1, 2, 3, 4) → axisswap 2,1 → (2, 1, 3, 4) → unitconvert deg→rad
        // Reference: (0.0349065850, 0.0174532925, 3, 4)
        let p = parse(
            "+proj=pipeline +step +proj=axisswap +order=2,1 +step +proj=unitconvert +xy_in=deg +xy_out=rad",
        )
        .unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0).unwrap();
        let expected_x = 2.0_f64.to_radians();
        let expected_y = 1.0_f64.to_radians();
        assert!((x - expected_x).abs() < 1e-12, "x = {x}");
        assert!((y - expected_y).abs() < 1e-12, "y = {y}");
        assert_eq!(z, 3.0);
        assert_eq!(t, 4.0);
    }

    #[test]
    fn chained_push_helmert_pop() {
        // Push v_3, apply helmert translation, pop v_3
        let p = parse(
            "+proj=pipeline +step +proj=push +v_3 +step +proj=helmert +x=1 +y=2 +z=3 +step +proj=pop +v_3",
        )
        .unwrap();
        let (x, y, z, t) = execute_pipeline(&p, 10.0, 20.0, 30.0, 40.0).unwrap();
        // push: (10, 20, 0, 40), stack=[30]
        // helmert: (11, 22, 3, 40)
        // pop: (11, 22, 30, 40)
        assert_eq!((x, y, z, t), (11.0, 22.0, 30.0, 40.0));
    }

    #[test]
    fn chained_full_datum_shift() {
        // Full chain: unitconvert, push, cart, helmert, pop, unitconvert back
        // This is the core of a real datum shift pipeline.
        // We test the forward half (up to helmert) since inverse cart is out of scope.
        let p = parse(
            "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=push +v_3 +step +proj=cart +ellps=GRS80 +step +proj=helmert +x=0.06155 +y=-0.01087 +z=-0.04019 +rx=-0.0394924 +ry=-0.0327221 +rz=-0.0328979 +s=-0.009994 +convention=coordinate_frame +step +proj=pop +v_3 +step +proj=unitconvert +xy_in=rad +xy_out=deg",
        )
        .unwrap();
        let (_, _, z, t) = execute_pipeline(&p, 12.0, 55.0, 0.0, 0.0).unwrap();
        // After all steps, z should be restored to 0 (from push/pop)
        // x, y should be in degrees after the final unitconvert
        // The exact values depend on the full chain; we verify z and t
        assert_eq!(z, 0.0, "z should be restored by pop");
        assert_eq!(t, 0.0);
    }

    // -----------------------------------------------------------------------
    // Nested pipeline
    // -----------------------------------------------------------------------

    #[test]
    fn nested_pipeline_unitconvert_cart() {
        let p = parse(
            "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=pipeline +step +proj=cart +ellps=WGS84",
        )
        .unwrap();
        let (x, y, z, _) = execute_pipeline(&p, 12.0, 55.0, 0.0, 0.0).unwrap();
        assert!((x - 3_586_469.656_776).abs() < 1e-3, "x = {x}");
        assert!((y - 762_327.658_778).abs() < 1e-3, "y = {y}");
        assert!((z - 5_201_383.523_202).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn nested_pipeline_with_outer_steps() {
        // Outer: axisswap → nested[unitconvert, cart] → axisswap back
        let p = parse(
            "+proj=pipeline +step +proj=axisswap +order=2,1 +step +proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad +step +proj=cart +ellps=WGS84 +step +proj=axisswap +order=2,1",
        )
        .unwrap();
        let (x, y, z, _) = execute_pipeline(&p, 55.0, 12.0, 0.0, 0.0).unwrap();
        // After axisswap: (12, 55, 0, 0)
        // After nested: cart(12°, 55°) = (3586469.656776, 762327.658778, 5201383.523202, 0)
        // After axisswap back: (762327.658778, 3586469.656776, 5201383.523202, 0)
        assert!((x - 762_327.658_778).abs() < 1e-3, "x = {x}");
        assert!((y - 3_586_469.656_776).abs() < 1e-3, "y = {y}");
        assert!((z - 5_201_383.523_202).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn nested_pipeline_error_propagation() {
        // A projection step inside a nested pipeline should propagate the error.
        let p = parse(
            "+proj=pipeline +step +proj=pipeline +step +inv +proj=lcc +lat_0=-37 +lon_0=145",
        )
        .unwrap();
        let result = execute_pipeline(&p, 0.0, 0.0, 0.0, 0.0);
        assert!(matches!(result, Err(PipelineError::UnsupportedMethod(_))));
    }

    // -----------------------------------------------------------------------
    // Error cases
    // -----------------------------------------------------------------------

    #[test]
    fn projection_step_returns_unsupported() {
        let p = parse(
            "+proj=pipeline +step +proj=webmerc +lat_0=0 +lon_0=0 +x_0=0 +y_0=0 +ellps=WGS84",
        )
        .unwrap();
        let result = execute_pipeline(&p, 0.0, 0.0, 0.0, 0.0);
        assert!(matches!(result, Err(PipelineError::UnsupportedMethod(ref m)) if m == "webmerc"));
    }

    #[test]
    fn projection_step_with_params_returns_unsupported() {
        let p = parse(
            "+proj=pipeline +step +inv +proj=lcc +lat_0=-37 +lon_0=145 +lat_1=-36 +lat_2=-38",
        )
        .unwrap();
        let result = execute_pipeline(&p, 0.0, 0.0, 0.0, 0.0);
        assert!(matches!(result, Err(PipelineError::UnsupportedMethod(ref m)) if m == "lcc"));
    }

    #[test]
    fn error_display_unsupported_method() {
        let err = PipelineError::UnsupportedMethod("lcc".to_string());
        assert_eq!(format!("{err}"), "unsupported projection method: \"lcc\"");
    }

    #[test]
    fn error_display_invalid_unit() {
        let err = PipelineError::InvalidUnit {
            unit: "furlong".to_string(),
            kind: UnitKind::Input,
        };
        assert_eq!(format!("{err}"), "invalid unit \"furlong\" for xy_in");
    }

    #[test]
    fn error_display_unknown_ellipsoid() {
        let err = PipelineError::UnknownEllipsoid("foo".to_string());
        assert_eq!(format!("{err}"), "unknown ellipsoid: \"foo\"");
    }

    #[test]
    fn error_display_empty_stack() {
        let err = PipelineError::EmptyStack;
        assert_eq!(format!("{err}"), "pop with empty stack");
    }

    #[test]
    fn error_display_invalid_axis() {
        let err = PipelineError::InvalidAxis(5);
        assert_eq!(format!("{err}"), "invalid axis index: 5");
    }

    // -----------------------------------------------------------------------
    // Deferred capability: GSB NADCON grids (bead qgis-qxu.3.3.1, ADR 0006)
    // -----------------------------------------------------------------------
    //
    // ADR 0006 records GSB NADCON (.gsb) horizontal datum shifts as DEFERRED.
    // The contract for a deferred capability is "rejected by name, never
    // silently approximated": a request that needs a .gsb grid must fail with
    // an explicit error, not return an unshifted coordinate. These tests pin
    // that contract so the deferral cannot silently regress into an
    // accepted-and-ignored parameter.

    #[test]
    fn deferred_gsb_nadcon_hgridshift_rejected_by_name() {
        // A real QGIS-shaped NADCON pipeline: degrees → radians, an inverse
        // horizontal grid shift against a .gsb grid, then back to degrees.
        let p = parse(
            "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad \
             +step +inv +proj=hgridshift +grids=nadcon5.gsb \
             +step +proj=unitconvert +xy_in=rad +xy_out=deg",
        )
        .unwrap();
        let result = execute_pipeline(&p, 45.0, 45.0, 0.0, 0.0);
        assert!(
            matches!(result, Err(PipelineError::UnsupportedMethod(ref m)) if m == "hgridshift"),
            "expected hgridshift to be rejected by name, got {result:?}"
        );
    }

    #[test]
    fn deferred_gsb_nadcon_top_level_hgridshift_rejected() {
        // A bare +proj=hgridshift CRS must also be rejected rather than
        // silently treated as an identity transform.
        let result = crate::CrsTransform::from_proj_string("+proj=hgridshift +grids=nadcon5.gsb");
        assert!(
            result.is_err(),
            "top-level hgridshift must be rejected, got {result:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Deferred capability: GTX vertical grids (bead qgis-qxu.3.3.2, ADR 0006)
    // -----------------------------------------------------------------------
    //
    // ADR 0006 records GTX (.gtx) vertical datum shifts as DEFERRED. The
    // contract for a deferred capability is "rejected by name, never
    // silently approximated": a request that needs a .gtx grid must fail
    // with an explicit error, not return an unshifted coordinate. These
    // tests pin that contract so the deferral cannot silently regress into
    // an accepted-and-ignored parameter.

    #[test]
    fn deferred_gtx_vgridshift_rejected_by_name() {
        // A real QGIS-shaped vertical datum shift pipeline: degrees → radians,
        // an inverse vertical grid shift against a .gtx grid, then back to
        // degrees.
        let p = parse(
            "+proj=pipeline +step +proj=unitconvert +xy_in=deg +xy_out=rad \
             +step +inv +proj=vgridshift +grids=egm_2008_1.gtx \
             +step +proj=unitconvert +xy_in=rad +xy_out=deg",
        )
        .unwrap();
        let result = execute_pipeline(&p, 45.0, 45.0, 0.0, 0.0);
        assert!(
            matches!(result, Err(PipelineError::UnsupportedMethod(ref m)) if m == "vgridshift"),
            "expected vgridshift to be rejected by name, got {result:?}"
        );
    }

    #[test]
    fn deferred_gtx_top_level_vgridshift_rejected() {
        // A bare +proj=vgridshift CRS must also be rejected rather than
        // silently treated as an identity transform.
        let result = crate::CrsTransform::from_proj_string("+proj=vgridshift +grids=egm_2008_1.gtx");
        assert!(
            result.is_err(),
            "top-level vgridshift must be rejected, got {result:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Empty pipeline
    // -----------------------------------------------------------------------

    #[test]
    fn empty_pipeline_identity() {
        let p = parse("+proj=pipeline").unwrap();
        let result = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0).unwrap();
        assert_eq!(result, (1.0, 2.0, 3.0, 4.0));
    }

    // -----------------------------------------------------------------------
    // Stack isolation between top-level calls
    // -----------------------------------------------------------------------

    #[test]
    fn stack_isolated_between_calls() {
        // Each call to execute_pipeline gets a fresh stack.
        let p = parse("+proj=pipeline +step +proj=push +v_3").unwrap();
        let _ = execute_pipeline(&p, 1.0, 2.0, 3.0, 4.0).unwrap();
        // A new call should not see the previous stack
        let p2 = parse("+proj=pipeline +step +proj=pop +v_3").unwrap();
        let result = execute_pipeline(&p2, 1.0, 2.0, 3.0, 4.0);
        assert!(matches!(result, Err(PipelineError::EmptyStack)));
    }

    // -----------------------------------------------------------------------
    // Pipeline::execute convenience method
    // -----------------------------------------------------------------------

    #[test]
    fn pipeline_execute_method() {
        let p = parse("+proj=pipeline +step +proj=noop").unwrap();
        let result = p.execute(1.0, 2.0, 3.0, 4.0).unwrap();
        assert_eq!(result, (1.0, 2.0, 3.0, 4.0));
    }

    // -----------------------------------------------------------------------
    // Display for error kinds
    // -----------------------------------------------------------------------

    #[test]
    fn unit_kind_display() {
        // UnitKind doesn't need Display, but we use it in the error message.
        // This test just ensures the enum is constructible.
        let _ = UnitKind::Input;
        let _ = UnitKind::Output;
    }
}
