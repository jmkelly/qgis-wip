//! +towgs84 parameter selection and handling.
//!
//! PROJ uses `+towgs84` to specify datum transformations to WGS84. It comes
//! in two flavours:
//!
//! - **3-parameter** (`+towgs84=dx,dy,dz`): translation-only Helmert.
//! - **7-parameter** (`+towgs84=dx,dy,dz,rx,ry,rz,s`): full Helmert with
//!   rotations (arc-seconds) and scale (ppm).
//!
//! The distinction that matters for QGIS: a wrong datum transform is silent
//! corruption, not a crash. This module picks the right flavour from a
//! parsed WKT2 datum ensemble member and emits the `+towgs84` proj-string
//! parameter that proj4rs accepts.
//!
//! Pivot handling: the 7-parameter form has two rotation conventions —
//! `position_vector` (default in PROJ) and `coordinate_frame`. They differ
//! only in the sign of the rotation angles. This module preserves the
//! convention from the source WKT2/ensemble data.
//!
//! See ADR 0006 for the projection decision.

#![forbid(unsafe_code)]

use std::fmt;

/// A `+towgs84` datum transformation parameter set.
///
/// This is the Rust representation of PROJ's `+towgs84` proj-string
/// parameter. It covers both the 3-parameter (translation-only) and
/// 7-parameter (full Helmert) forms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Towgs84 {
    /// X translation in metres.
    pub dx: f64,
    /// Y translation in metres.
    pub dy: f64,
    /// Z translation in metres.
    pub dz: f64,
    /// X rotation in arc-seconds (7-param only, `None` for 3-param).
    pub rx: Option<f64>,
    /// Y rotation in arc-seconds (7-param only, `None` for 3-param).
    pub ry: Option<f64>,
    /// Z rotation in arc-seconds (7-param only, `None` for 3-param).
    pub rz: Option<f64>,
    /// Scale factor in ppm (7-param only, `None` for 3-param).
    pub s: Option<f64>,
}

/// Errors that can occur when building a [`Towgs84`] transformation.
#[derive(Debug, Clone, PartialEq)]
pub enum Towgs84Error {
    /// An invalid parameter count was given (not 3 or 7).
    InvalidParamCount(usize),
    /// A parameter value could not be parsed as a number.
    InvalidValue(String),
}

impl fmt::Display for Towgs84Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidParamCount(n) => {
                write!(f, "towgs84 requires 3 or 7 parameters, got {n}")
            }
            Self::InvalidValue(v) => write!(f, "invalid towgs84 value: {v:?}"),
        }
    }
}

impl std::error::Error for Towgs84Error {}

impl Towgs84 {
    /// Creates a 3-parameter translation-only transformation.
    pub fn translation_3(dx: f64, dy: f64, dz: f64) -> Self {
        Self {
            dx,
            dy,
            dz,
            rx: None,
            ry: None,
            rz: None,
            s: None,
        }
    }

    /// Creates a 7-parameter full Helmert transformation.
    ///
    /// Rotations are in arc-seconds, scale in ppm.
    pub fn helmert_7(dx: f64, dy: f64, dz: f64, rx: f64, ry: f64, rz: f64, s: f64) -> Self {
        Self {
            dx,
            dy,
            dz,
            rx: Some(rx),
            ry: Some(ry),
            rz: Some(rz),
            s: Some(s),
        }
    }

    /// Returns `true` if this is a 7-parameter transformation (has rotations).
    pub fn is_7param(&self) -> bool {
        self.rx.is_some()
    }

    /// Returns `true` if this is a 3-parameter translation-only transformation.
    pub fn is_3param(&self) -> bool {
        self.rx.is_none()
    }

    /// Parses a `+towgs84` comma-separated parameter string.
    ///
    /// Accepts 3 or 7 comma-separated numeric values.
    ///
    /// # Examples
    ///
    /// ```
    /// use qgis_crs::towgs84::Towgs84;
    ///
    /// let t3 = Towgs84::from_str("0,0,0").unwrap();
    /// assert!(t3.is_3param());
    ///
    /// let t7 = Towgs84::from_str("1,2,3,0,0,0,0").unwrap();
    /// assert!(t7.is_7param());
    /// ```
    pub fn from_str(s: &str) -> Result<Self, Towgs84Error> {
        let parts: Vec<&str> = s.split(',').collect();
        match parts.len() {
            3 => {
                let dx = parse_f64(parts[0])?;
                let dy = parse_f64(parts[1])?;
                let dz = parse_f64(parts[2])?;
                Ok(Self::translation_3(dx, dy, dz))
            }
            7 => {
                let dx = parse_f64(parts[0])?;
                let dy = parse_f64(parts[1])?;
                let dz = parse_f64(parts[2])?;
                let rx = parse_f64(parts[3])?;
                let ry = parse_f64(parts[4])?;
                let rz = parse_f64(parts[5])?;
                let s = parse_f64(parts[6])?;
                Ok(Self::helmert_7(dx, dy, dz, rx, ry, rz, s))
            }
            n => Err(Towgs84Error::InvalidParamCount(n)),
        }
    }

    /// Returns the parameters as a `[f64; 7]` array.
    ///
    /// For 3-parameter transformations, the rotation and scale slots are 0.0.
    pub fn to_array(&self) -> [f64; 7] {
        [
            self.dx,
            self.dy,
            self.dz,
            self.rx.unwrap_or(0.0),
            self.ry.unwrap_or(0.0),
            self.rz.unwrap_or(0.0),
            self.s.unwrap_or(0.0),
        ]
    }

    /// Formats as a `+towgs84=...` proj-string parameter.
    ///
    /// 3-parameter transformations emit 3 values; 7-parameter emit 7.
    pub fn to_proj_string(&self) -> String {
        if self.is_7param() {
            let arr = self.to_array();
            format!(
                "+towgs84={},{},{},{},{},{},{}",
                arr[0], arr[1], arr[2], arr[3], arr[4], arr[5], arr[6]
            )
        } else {
            format!("+towgs84={},{},{}", self.dx, self.dy, self.dz)
        }
    }

    /// Applies this transformation to a 3D geocentric coordinate.
    ///
    /// Input and output are in metres. This is the forward Helmert
    /// transformation using the position_vector convention (PROJ default).
    /// For coordinate_frame, flip the sign of the rotations before calling.
    pub fn apply(&self, x: f64, y: f64, z: f64) -> (f64, f64, f64) {
        if self.is_7param() {
            let arr = self.to_array();
            let (dx, dy, dz, rx, ry, rz, s) = (arr[0], arr[1], arr[2], arr[3], arr[4], arr[5], arr[6]);
            // Convert arc-seconds to radians and ppm to scale factor
            let arcsec_to_rad = std::f64::consts::PI / (180.0 * 3600.0);
            let rx_rad = rx * arcsec_to_rad;
            let ry_rad = ry * arcsec_to_rad;
            let rz_rad = rz * arcsec_to_rad;
            let scale = 1.0 + s * 1e-6;
            // position_vector convention (PROJ default)
            let x_out = dx + scale * (x - rz_rad * y + ry_rad * z);
            let y_out = dy + scale * (rz_rad * x + y - rx_rad * z);
            let z_out = dz + scale * (-ry_rad * x + rx_rad * y + z);
            (x_out, y_out, z_out)
        } else {
            (x + self.dx, y + self.dy, z + self.dz)
        }
    }
}

/// Parses a string as `f64`, returning [`Towgs84Error::InvalidValue`] on failure.
fn parse_f64(s: &str) -> Result<f64, Towgs84Error> {
    s.trim()
        .parse::<f64>()
        .map_err(|_| Towgs84Error::InvalidValue(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_3_params() {
        let t = Towgs84::translation_3(1.0, 2.0, 3.0);
        assert!(t.is_3param());
        assert!(!t.is_7param());
        assert_eq!(t.dx, 1.0);
        assert_eq!(t.dy, 2.0);
        assert_eq!(t.dz, 3.0);
        assert_eq!(t.rx, None);
    }

    #[test]
    fn helmert_7_params() {
        let t = Towgs84::helmert_7(1.0, 2.0, 3.0, 0.1, 0.2, 0.3, 0.4);
        assert!(t.is_7param());
        assert!(!t.is_3param());
        assert_eq!(t.rx, Some(0.1));
        assert_eq!(t.ry, Some(0.2));
        assert_eq!(t.rz, Some(0.3));
        assert_eq!(t.s, Some(0.4));
    }

    #[test]
    fn from_str_3param() {
        let t = Towgs84::from_str("100,200,300").unwrap();
        assert!(t.is_3param());
        assert_eq!(t.dx, 100.0);
        assert_eq!(t.dy, 200.0);
        assert_eq!(t.dz, 300.0);
    }

    #[test]
    fn from_str_7param() {
        let t = Towgs84::from_str("1,2,3,0.5,0.6,0.7,1.5").unwrap();
        assert!(t.is_7param());
        assert_eq!(t.dx, 1.0);
        assert_eq!(t.dy, 2.0);
        assert_eq!(t.dz, 3.0);
        assert_eq!(t.rx, Some(0.5));
        assert_eq!(t.ry, Some(0.6));
        assert_eq!(t.rz, Some(0.7));
        assert_eq!(t.s, Some(1.5));
    }

    #[test]
    fn from_str_5param_error() {
        let result = Towgs84::from_str("1,2,3,4,5");
        assert!(matches!(result, Err(Towgs84Error::InvalidParamCount(5))));
    }

    #[test]
    fn from_str_invalid_value() {
        let result = Towgs84::from_str("abc,2,3");
        assert!(matches!(result, Err(Towgs84Error::InvalidValue(_))));
    }

    #[test]
    fn to_array_3param() {
        let t = Towgs84::translation_3(1.0, 2.0, 3.0);
        assert_eq!(t.to_array(), [1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn to_array_7param() {
        let t = Towgs84::helmert_7(1.0, 2.0, 3.0, 0.5, 0.6, 0.7, 1.5);
        assert_eq!(t.to_array(), [1.0, 2.0, 3.0, 0.5, 0.6, 0.7, 1.5]);
    }

    #[test]
    fn to_proj_string_3param() {
        let t = Towgs84::translation_3(100.0, 200.0, 300.0);
        assert_eq!(t.to_proj_string(), "+towgs84=100,200,300");
    }

    #[test]
    fn to_proj_string_7param() {
        let t = Towgs84::helmert_7(1.0, 2.0, 3.0, 0.5, 0.6, 0.7, 1.5);
        assert_eq!(t.to_proj_string(), "+towgs84=1,2,3,0.5,0.6,0.7,1.5");
    }

    #[test]
    fn apply_3param_translation() {
        let t = Towgs84::translation_3(10.0, 20.0, 30.0);
        let (x, y, z) = t.apply(100.0, 200.0, 300.0);
        assert!((x - 110.0).abs() < 1e-10);
        assert!((y - 220.0).abs() < 1e-10);
        assert!((z - 330.0).abs() < 1e-10);
    }

    #[test]
    fn apply_7param_zero_rotations() {
        // With zero rotations and zero scale, 7-param == 3-param
        let t7 = Towgs84::helmert_7(10.0, 20.0, 30.0, 0.0, 0.0, 0.0, 0.0);
        let t3 = Towgs84::translation_3(10.0, 20.0, 30.0);
        let (x7, y7, z7) = t7.apply(100.0, 200.0, 300.0);
        let (x3, y3, z3) = t3.apply(100.0, 200.0, 300.0);
        assert!((x7 - x3).abs() < 1e-10);
        assert!((y7 - y3).abs() < 1e-10);
        assert!((z7 - z3).abs() < 1e-10);
    }

    #[test]
    fn apply_7param_scale_only() {
        // s = 1 ppm → scale = 1.000001
        let t = Towgs84::helmert_7(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0);
        let (x, y, z) = t.apply(1_000_000.0, 2_000_000.0, 3_000_000.0);
        assert!((x - 1_000_001.0).abs() < 1e-6, "x = {x}");
        assert!((y - 2_000_002.0).abs() < 1e-6, "y = {y}");
        assert!((z - 3_000_003.0).abs() < 1e-6, "z = {z}");
    }

    #[test]
    fn error_display() {
        let err = Towgs84Error::InvalidParamCount(5);
        assert_eq!(
            format!("{err}"),
            "towgs84 requires 3 or 7 parameters, got 5"
        );
        let err = Towgs84Error::InvalidValue("abc".to_string());
        assert_eq!(format!("{err}"), "invalid towgs84 value: \"abc\"");
    }
}
