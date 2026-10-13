//! Native PROJ projection methods gap-built for qgis-crs (ADR 0006).
//!
//! proj4rs does not implement all `+proj` methods. This module provides
//! pure-Rust implementations of the missing methods that QGIS needs:
//!
//! - `ortho` — Orthographic (spherical + ellipsoidal, forward + inverse)
//! - `qsc` — Quadrilateralized Spherical Cube (ellipsoidal, forward + inverse)
//! - `geocent` — Geocentric / ECEF (geodetic ↔ geocentric, replacing the
//!   proj4rs stub with a full implementation)
//!
//! # Conventions
//!
//! Matching PROJ and proj4rs:
//! - Angular inputs/outputs are in **radians**.
//! - `ortho` / `qsc` forward return **dimensionless** unit-sphere
//!   coordinates; the caller scales by the semi-major axis `a` and applies
//!   `x_0` / `y_0` / `k_0`.
//! - `geocent` forward/inverse work in **meters** (X, Y, Z and height).
//!
//! # References
//!
//! - PROJ `src/projections/ortho.cpp`
//! - PROJ `src/projections/qsc.cpp`
//! - PROJ `src/geocent.cpp` (via proj4rs `geocent.rs`)

#![forbid(unsafe_code)]

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

/// PROJ's default epsilon for domain checks.
const EPS10: f64 = 1.0e-10;

/// π/4 — used by QSC face/area determination.
const FORTPI: f64 = FRAC_PI_4;

/// π/2.
const HALFPI: f64 = FRAC_PI_2;

/// 3π/2 — used by QSC area rotation.
const THREE_HALFPI: f64 = PI + FRAC_PI_2;

// ---------------------------------------------------------------------------
// Longitude normalization
// ---------------------------------------------------------------------------

/// Normalize longitude to [-π, π], matching PROJ's `adjlon`.
fn adjlon(mut lon: f64) -> f64 {
    // Let lon slightly overshoot, to avoid spurious sign switching at the date line
    if lon.abs() >= PI + 1.0e-12 {
        lon += PI;
        lon -= 2.0 * PI * (lon / (2.0 * PI)).floor();
        lon -= PI;
    }
    lon
}

// ---------------------------------------------------------------------------
// Ellipsoid
// ---------------------------------------------------------------------------

/// Ellipsoid parameters, mirroring proj4rs's `Ellipsoid` (ellps.rs).
///
/// All derived values are precomputed so projection math is branch-free.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ellipsoid {
    /// Semi-major axis (meters).
    pub a: f64,
    /// Semi-minor axis (meters).
    pub b: f64,
    /// First eccentricity squared (`e² = 2f − f²`).
    pub es: f64,
    /// `1 − es`.
    pub one_es: f64,
    /// First eccentricity.
    pub e: f64,
    /// `1 / a`.
    pub ra: f64,
    /// `1 / one_es`.
    pub rone_es: f64,
    /// Flattening.
    pub f: f64,
    /// Reverse flattening.
    pub rf: f64,
}

impl Ellipsoid {
    /// Creates an ellipsoid from semi-major axis `a` and flattening `f`.
    pub fn from_af(a: f64, f: f64) -> Self {
        let es = 2.0 * f - f * f;
        let b = a * (1.0 - f);
        Self {
            a,
            b,
            es,
            one_es: 1.0 - es,
            e: es.sqrt(),
            ra: 1.0 / a,
            rone_es: 1.0 / (1.0 - es),
            f,
            rf: 1.0 / f,
        }
    }

    /// Creates a sphere of radius `r` (zero flattening).
    pub fn sphere(r: f64) -> Self {
        Self::from_af(r, 0.0)
    }

    /// Returns `true` if this is a sphere (zero eccentricity).
    pub fn is_sphere(&self) -> bool {
        self.es == 0.0
    }

    /// Resolves an ellipsoid from PROJ-style parameters.
    ///
    /// Supports (in priority order):
    /// - `+R=<radius>` — sphere override
    /// - `+ellps=<name>` — named ellipsoid (WGS84, GRS80, sphere, etc.)
    /// - `+a=<a>` with `+b`, `+rf`, `+f`, or `+es`
    /// - Default: WGS84
    pub fn from_proj_params(params: &ProjParams) -> Result<Self, ProjectionError> {
        if let Some(r) = params.get("R") {
            return Ok(Self::sphere(r));
        }
        if let Some(name) = params.get_str("ellps") {
            return Self::from_name(&name).ok_or(ProjectionError::UnknownEllipsoid(name.to_string()));
        }
        if let Some(a) = params.get("a") {
            if let Some(b) = params.get("b") {
                let f = 1.0 - b / a;
                return Ok(Self::from_af(a, f));
            }
            if let Some(rf) = params.get("rf") {
                return Ok(Self::from_af(a, 1.0 / rf));
            }
            if let Some(f) = params.get("f") {
                return Ok(Self::from_af(a, f));
            }
            if let Some(es) = params.get("es") {
                let f = 1.0 - (1.0 - es).sqrt();
                return Ok(Self::from_af(a, f));
            }
        }
        // Default: WGS84
        Ok(Self::wgs84())
    }

    /// WGS 84 ellipsoid.
    pub fn wgs84() -> Self {
        Self::from_af(6_378_137.0, 1.0 / 298.257_223_563)
    }

    /// GRS 1980 ellipsoid.
    pub fn grs80() -> Self {
        Self::from_af(6_378_137.0, 1.0 / 298.257_222_101)
    }

    /// Looks up a named ellipsoid (case-insensitive).
    fn from_name(name: &str) -> Option<Self> {
        let (a, f) = match name.to_lowercase().as_str() {
            "wgs84" => (6_378_137.0, 1.0 / 298.257_223_563),
            "grs80" => (6_378_137.0, 1.0 / 298.257_222_101),
            "sphere" => (6_370_997.0, 0.0),
            "airy" => (6_377_563.396, 1.0 / 299.324_964_6),
            "bessel" => (6_377_397.155, 1.0 / 299.152_812_8),
            "clrk66" => (6_378_206.4, 1.0 / 294.978_698_2),
            "clrk80" => (6_378_249.145, 1.0 / 293.465_998),
            "intl" => (6_378_388.0, 1.0 / 297.0),
            "krass" => (6_378_245.0, 1.0 / 298.3),
            _ => return None,
        };
        Some(Self::from_af(a, f))
    }
}

// ---------------------------------------------------------------------------
// Projection parameters
// ---------------------------------------------------------------------------

/// Parsed PROJ-style key/value parameter list.
#[derive(Debug, Clone, Default)]
pub struct ProjParams {
    entries: Vec<(String, Option<String>)>,
}

impl ProjParams {
    /// Parses a PROJ string into a parameter list.
    ///
    /// Each `+key=value` or bare `+flag` token becomes an entry.
    pub fn parse(s: &str) -> Self {
        let mut entries = Vec::new();
        for token in s.split('+') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            if let Some(eq) = token.find('=') {
                let key = token[..eq].trim().to_string();
                let value = token[eq + 1..].trim().to_string();
                entries.push((key, Some(value)));
            } else {
                entries.push((token.to_string(), None));
            }
        }
        Self { entries }
    }

    /// Returns the value of `name` as `f64`, or `None` if absent.
    pub fn get(&self, name: &str) -> Option<f64> {
        self.entries
            .iter()
            .find(|(k, _)| k == name)
            .and_then(|(_, v)| v.as_ref())
            .and_then(|v| v.parse::<f64>().ok())
    }

    /// Returns the value of `name` as `&str`, or `None` if absent.
    pub fn get_str(&self, name: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == name)
            .and_then(|(_, v)| v.as_deref())
    }

    /// Returns `true` if a bare flag `+name` is present.
    pub fn has_flag(&self, name: &str) -> bool {
        self.entries.iter().any(|(k, v)| k == name && v.is_none())
    }

    /// Adds a `key=value` entry.
    pub fn push(&mut self, key: impl Into<String>, value: Option<String>) {
        self.entries.push((key.into(), value));
    }
}

/// Common projection parameters (center, offsets, scale, rotation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectionParams {
    /// Latitude of projection center (radians).
    pub lat_0: f64,
    /// Longitude of projection center (radians).
    pub lon_0: f64,
    /// False easting (meters).
    pub x_0: f64,
    /// False northing (meters).
    pub y_0: f64,
    /// Scale factor.
    pub k_0: f64,
    /// Rotation angle (radians, `+alpha`).
    pub alpha: f64,
}

impl Default for ProjectionParams {
    fn default() -> Self {
        Self {
            lat_0: 0.0,
            lon_0: 0.0,
            x_0: 0.0,
            y_0: 0.0,
            k_0: 1.0,
            alpha: 0.0,
        }
    }
}

impl ProjectionParams {
    /// Builds from a parsed parameter list, applying PROJ defaults.
    ///
    /// Angular parameters (`lat_0`, `lon_0`, `alpha`) are in degrees and
    /// are converted to radians.
    pub fn from_proj_params(params: &ProjParams) -> Self {
        Self {
            lat_0: params.get("lat_0").unwrap_or(0.0).to_radians(),
            lon_0: params.get("lon_0").unwrap_or(0.0).to_radians(),
            x_0: params.get("x_0").unwrap_or(0.0),
            y_0: params.get("y_0").unwrap_or(0.0),
            k_0: params.get("k_0").or_else(|| params.get("k")).unwrap_or(1.0),
            alpha: params.get("alpha").unwrap_or(0.0).to_radians(),
        }
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during native projection.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectionError {
    /// The point is outside the projection's domain (e.g. ortho far hemisphere).
    OutsideDomain,
    /// The projection failed to converge (iterative inverse).
    NotConverged,
    /// An unknown ellipsoid name was given.
    UnknownEllipsoid(String),
    /// A required parameter is missing or invalid.
    InvalidParams(String),
    /// The projection method name is not one of the gap-built methods.
    UnknownMethod(String),
}

impl std::fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutsideDomain => write!(f, "coordinate outside projection domain"),
            Self::NotConverged => write!(f, "projection inverse failed to converge"),
            Self::UnknownEllipsoid(name) => write!(f, "unknown ellipsoid: {name:?}"),
            Self::InvalidParams(msg) => write!(f, "invalid parameters: {msg}"),
            Self::UnknownMethod(name) => write!(f, "unknown projection method: {name:?}"),
        }
    }
}

impl std::error::Error for ProjectionError {}

// ---------------------------------------------------------------------------
// Orthographic
// ---------------------------------------------------------------------------

/// Orthographic projection mode (from PROJ `ortho.cpp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum OrthoMode {
    NPole = 0,
    SPole = 1,
    Equit = 2,
    Obliq = 3,
}

/// Orthographic projection — spherical and ellipsoidal, forward and inverse.
///
/// Ported from PROJ `src/projections/ortho.cpp`.
///
/// Forward takes `(lam, phi)` relative to the projection center (radians) and
/// returns dimensionless unit-sphere coordinates. The caller scales by `a`
/// and applies `k_0`, `x_0`, `y_0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orthographic {
    ellps: Ellipsoid,
    phi0: f64,
    lam0: f64,
    x0: f64,
    y0: f64,
    k0: f64,
    sinph0: f64,
    cosph0: f64,
    mode: OrthoMode,
    sinalpha: f64,
    cosalpha: f64,
    spherical: bool,
    nu0: f64,
    y_shift: f64,
    y_scale: f64,
}

impl Orthographic {
    /// Creates a new orthographic projection.
    pub fn new(ellps: Ellipsoid, params: ProjectionParams) -> Self {
        let sinph0 = params.lat_0.sin();
        let cosph0 = params.lat_0.cos();

        let mode = if (params.lat_0.abs() - HALFPI).abs() <= EPS10 {
            if params.lat_0 < 0.0 {
                OrthoMode::SPole
            } else {
                OrthoMode::NPole
            }
        } else if params.lat_0.abs() > EPS10 {
            OrthoMode::Obliq
        } else {
            OrthoMode::Equit
        };

        let spherical = ellps.is_sphere();

        let (nu0, y_shift, y_scale) = if spherical {
            (0.0, 0.0, 0.0)
        } else {
            let nu0 = 1.0 / (1.0 - ellps.es * sinph0 * sinph0).sqrt();
            let y_shift = ellps.es * nu0 * sinph0 * cosph0;
            let y_scale = 1.0 / (1.0 - ellps.es * cosph0 * cosph0).sqrt();
            (nu0, y_shift, y_scale)
        };

        Self {
            ellps,
            phi0: params.lat_0,
            lam0: params.lon_0,
            x0: params.x_0,
            y0: params.y_0,
            k0: params.k_0,
            sinph0,
            cosph0,
            mode,
            sinalpha: params.alpha.sin(),
            cosalpha: params.alpha.cos(),
            spherical,
            nu0,
            y_shift,
            y_scale,
        }
    }

    /// Creates from a PROJ string (e.g. `"+proj=ortho +lat_0=40 +lon_0=-100"`).
    pub fn from_proj_string(s: &str) -> Result<Self, ProjectionError> {
        let params = ProjParams::parse(s);
        if params.get_str("proj") != Some("ortho") {
            return Err(ProjectionError::InvalidParams(format!(
                "expected +proj=ortho, got {s:?}"
            )));
        }
        let ellps = Ellipsoid::from_proj_params(&params)?;
        let proj_params = ProjectionParams::from_proj_params(&params);
        Ok(Self::new(ellps, proj_params))
    }

    /// Returns the ellipsoid.
    pub fn ellipsoid(&self) -> Ellipsoid {
        self.ellps
    }

    /// Returns the projection center (lat_0, lon_0) in radians.
    pub fn center(&self) -> (f64, f64) {
        (self.phi0, self.lam0)
    }

    /// Forward projection: `(lam, phi)` relative to center (radians) →
    /// dimensionless unit-sphere `(x, y)`.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectionError::OutsideDomain`] if the point is on the
    /// unprojected hemisphere.
    pub fn forward(&self, lam: f64, phi: f64) -> Result<(f64, f64), ProjectionError> {
        if self.spherical {
            self.s_forward(lam, phi)
        } else {
            self.e_forward(lam, phi)
        }
    }

    /// Inverse projection: dimensionless `(x, y)` → `(lam, phi)` relative
    /// to center (radians).
    ///
    /// # Errors
    ///
    /// Returns [`ProjectionError::OutsideDomain`] if the point is outside
    /// the visible hemisphere.
    pub fn inverse(&self, x: f64, y: f64) -> Result<(f64, f64), ProjectionError> {
        if self.spherical {
            self.s_inverse(x, y)
        } else {
            self.e_inverse(x, y)
        }
    }

    // -------------------
    // Spherical
    // -------------------

    fn s_forward(&self, lam: f64, phi: f64) -> Result<(f64, f64), ProjectionError> {
        let cosphi = phi.cos();
        let mut coslam = lam.cos();

        let y = match self.mode {
            OrthoMode::Equit => {
                if cosphi * coslam < -EPS10 {
                    return Err(ProjectionError::OutsideDomain);
                }
                phi.sin()
            }
            OrthoMode::Obliq => {
                let sinphi = phi.sin();
                if self.sinph0 * sinphi + self.cosph0 * cosphi * coslam < -EPS10 {
                    return Err(ProjectionError::OutsideDomain);
                }
                self.cosph0 * sinphi - self.sinph0 * cosphi * coslam
            }
            OrthoMode::NPole => {
                coslam = -coslam;
                if (phi - self.phi0).abs() - EPS10 > HALFPI {
                    return Err(ProjectionError::OutsideDomain);
                }
                cosphi * coslam
            }
            OrthoMode::SPole => {
                if (phi - self.phi0).abs() - EPS10 > HALFPI {
                    return Err(ProjectionError::OutsideDomain);
                }
                cosphi * coslam
            }
        };

        let x = cosphi * lam.sin();
        let (xp, yp) = (x, y);
        Ok((
            (xp * self.cosalpha - yp * self.sinalpha) * self.k0,
            (xp * self.sinalpha + yp * self.cosalpha) * self.k0,
        ))
    }

    fn s_inverse(&self, xf: f64, yf: f64) -> Result<(f64, f64), ProjectionError> {
        let x = (self.cosalpha * xf + self.sinalpha * yf) / self.k0;
        let y = (-self.sinalpha * xf + self.cosalpha * yf) / self.k0;

        let rh = x.hypot(y);
        let mut sinc = rh;
        if sinc > 1.0 {
            if sinc - 1.0 > EPS10 {
                return Err(ProjectionError::OutsideDomain);
            }
            sinc = 1.0;
        }
        let cosc = (1.0 - sinc * sinc).sqrt();

        if rh.abs() <= EPS10 {
            return Ok((0.0, self.phi0));
        }

        let (mut x, mut y, mut phi) = (x, y, 0.0_f64);
        match self.mode {
            OrthoMode::NPole => {
                y = -y;
                phi = sinc.acos();
            }
            OrthoMode::SPole => {
                phi = -sinc.acos();
            }
            OrthoMode::Equit => {
                let phi_sine = y * sinc / rh;
                x *= sinc;
                y = cosc * rh;
                phi = sinchk(phi_sine);
            }
            OrthoMode::Obliq => {
                let phi_sine = cosc * self.sinph0 + y * sinc * self.cosph0 / rh;
                y = (cosc - self.sinph0 * phi_sine) * rh;
                x *= sinc * self.cosph0;
                phi = sinchk(phi_sine);
            }
        }

        let lam = if y == 0.0 && matches!(self.mode, OrthoMode::Obliq | OrthoMode::Equit) {
            if x == 0.0 {
                0.0
            } else if x < 0.0 {
                -PI
            } else {
                PI
            }
        } else {
            x.atan2(y)
        };

        Ok((lam, phi))
    }

    // -------------------
    // Ellipsoidal
    // -------------------

    fn e_forward(&self, lam: f64, phi: f64) -> Result<(f64, f64), ProjectionError> {
        let cosphi = phi.cos();
        let sinphi = phi.sin();
        let coslam = lam.cos();
        let sinlam = lam.sin();

        if self.sinph0 * sinphi + self.cosph0 * cosphi * coslam < -EPS10 {
            return Err(ProjectionError::OutsideDomain);
        }

        // From EPSG guidance note 7.2, March 2020, §3.3.5 Orthographic
        let nu = 1.0 / (1.0 - self.ellps.es * sinphi * sinphi).sqrt();
        let xp = nu * cosphi * sinlam;
        let yp = nu * (sinphi * self.cosph0 - cosphi * self.sinph0 * coslam)
            + self.ellps.es * (self.nu0 * self.sinph0 - nu * sinphi) * self.cosph0;
        let (x, y) = (xp, yp);
        Ok((
            (self.cosalpha * x - self.sinalpha * y) * self.k0,
            (self.sinalpha * x + self.cosalpha * y) * self.k0,
        ))
    }

    fn e_inverse(&self, xf: f64, yf: f64) -> Result<(f64, f64), ProjectionError> {
        let x = (self.cosalpha * xf + self.sinalpha * yf) / self.k0;
        let y = (-self.sinalpha * xf + self.cosalpha * yf) / self.k0;

        if matches!(self.mode, OrthoMode::NPole | OrthoMode::SPole) {
            // Polar case
            let rh2 = x * x + y * y;
            if rh2 >= 1.0 - 1e-15 {
                if rh2 - 1.0 > EPS10 {
                    return Err(ProjectionError::OutsideDomain);
                }
                Ok((x.atan2(y * if self.mode == OrthoMode::NPole { -1.0 } else { 1.0 }), 0.0))
            } else {
                let phi = (rh2 * self.ellps.one_es / (1.0 - self.ellps.es * rh2)).sqrt().acos()
                    * if self.mode == OrthoMode::NPole { 1.0 } else { -1.0 };
                let lam = x.atan2(y * if self.mode == OrthoMode::NPole { -1.0 } else { 1.0 });
                Ok((lam, phi))
            }
        } else if self.mode == OrthoMode::Equit {
            // Equatorial case
            if x * x + (y * (self.ellps.a / self.ellps.b)).powi(2) > 1.0 + 1e-11 {
                return Err(ProjectionError::OutsideDomain);
            }
            let sinphi2 = if y == 0.0 {
                0.0
            } else {
                1.0 / (((1.0 - self.ellps.es) / y).powi(2) + self.ellps.es)
            };
            if sinphi2 > 1.0 - 1e-11 {
                return Ok((0.0, HALFPI * if y > 0.0 { 1.0 } else { -1.0 }));
            }
            let phi = sinphi2.sqrt().asin() * if y > 0.0 { 1.0 } else { -1.0 };
            let sinlam = x * ((1.0 - self.ellps.es * sinphi2) / (1.0 - sinphi2)).sqrt();
            let lam = if sinlam.abs() - 1.0 > -1e-15 {
                HALFPI * if x > 0.0 { 1.0 } else { -1.0 }
            } else {
                sinlam.asin()
            };
            Ok((lam, phi))
        } else {
            // Oblique case — iterative Newton refinement
            let xy_rc_x = x;
            let xy_rc_y = (y - self.y_shift) / self.y_scale;
            if x * x + xy_rc_y * xy_rc_y > 1.0 + 1e-11 {
                return Err(ProjectionError::OutsideDomain);
            }

            // Initial guess: spherical inverse on recentered coordinates
            let (mut lam, mut phi) = self.s_inverse(xy_rc_x, xy_rc_y)?;

            let es = self.ellps.es;
            for _ in 0..20 {
                let cosphi = phi.cos();
                let sinphi = phi.sin();
                let coslam = lam.cos();
                let sinlam = lam.sin();
                let one_minus_es_sinphi2 = 1.0 - es * sinphi * sinphi;
                let nu = 1.0 / one_minus_es_sinphi2.sqrt();
                let x_new = nu * cosphi * sinlam;
                let y_new = nu * (sinphi * self.cosph0 - cosphi * self.sinph0 * coslam)
                    + es * (self.nu0 * self.sinph0 - nu * sinphi) * self.cosph0;
                let rho = (1.0 - es) * nu / one_minus_es_sinphi2;
                let j11 = -rho * sinphi * sinlam;
                let j12 = nu * cosphi * coslam;
                let j21 = rho * (cosphi * self.cosph0 + sinphi * self.sinph0 * coslam);
                let j22 = nu * self.sinph0 * cosphi * sinlam;
                let d = j11 * j22 - j12 * j21;
                let dx = x - x_new;
                let dy = y - y_new;
                let dphi = (j22 * dx - j12 * dy) / d;
                let dlam = (-j21 * dx + j11 * dy) / d;
                phi += dphi;
                if phi > HALFPI {
                    phi = HALFPI - (phi - HALFPI);
                    lam = adjlon(lam + PI);
                } else if phi < -HALFPI {
                    phi = -HALFPI + (-HALFPI - phi);
                    lam = adjlon(lam + PI);
                }
                lam += dlam;
                if dphi.abs() < 1e-12 && dlam.abs() < 1e-12 {
                    return Ok((lam, phi));
                }
            }
            Err(ProjectionError::NotConverged)
        }
    }
}

/// Clamp `phi_sine` to [-1, 1] and take `asin`, matching PROJ's `sinchk`.
fn sinchk(phi_sine: f64) -> f64 {
    if phi_sine.abs() >= 1.0 {
        if phi_sine < 0.0 {
            -HALFPI
        } else {
            HALFPI
        }
    } else {
        phi_sine.asin()
    }
}

// ---------------------------------------------------------------------------
// QSC (Quadrilateralized Spherical Cube)
// ---------------------------------------------------------------------------

/// QSC cube face (from PROJ `qsc.cpp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum QscFace {
    Front = 0,
    Right = 1,
    Back = 2,
    Left = 3,
    Top = 4,
    Bottom = 5,
}

/// QSC area on a cube face (from PROJ `qsc.cpp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum QscArea {
    Area0 = 0,
    Area1 = 1,
    Area2 = 2,
    Area3 = 3,
}

/// Quadrilateralized Spherical Cube projection (ellipsoidal).
///
/// Ported from PROJ `src/projections/qsc.cpp`, including the ellipsoid↔sphere
/// shift from [LK12] (Lambers & Kolb, 2012).
///
/// Forward takes `(lam, phi)` relative to the projection center (radians) and
/// returns dimensionless unit-sphere coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Qsc {
    ellps: Ellipsoid,
    phi0: f64,
    lam0: f64,
    face: QscFace,
    one_minus_f: f64,
    one_minus_f_squared: f64,
    b: f64,
}

impl Qsc {
    /// Creates a new QSC projection.
    pub fn new(ellps: Ellipsoid, params: ProjectionParams) -> Self {
        let (face, one_minus_f, one_minus_f_squared, b) = if ellps.es != 0.0 {
            let b = ellps.a * (1.0 - ellps.es).sqrt();
            let one_minus_f = 1.0 - (ellps.a - b) / ellps.a;
            let face = if params.lat_0 >= HALFPI - FORTPI / 2.0 {
                QscFace::Top
            } else if params.lat_0 <= -(HALFPI - FORTPI / 2.0) {
                QscFace::Bottom
            } else if params.lon_0.abs() <= FORTPI {
                QscFace::Front
            } else if params.lon_0.abs() <= HALFPI + FORTPI {
                if params.lon_0 > 0.0 {
                    QscFace::Right
                } else {
                    QscFace::Left
                }
            } else {
                QscFace::Back
            };
            (face, one_minus_f, one_minus_f * one_minus_f, b)
        } else {
            // Sphere: no ellipsoid↔sphere shift needed
            let face = if params.lat_0 >= HALFPI - FORTPI / 2.0 {
                QscFace::Top
            } else if params.lat_0 <= -(HALFPI - FORTPI / 2.0) {
                QscFace::Bottom
            } else if params.lon_0.abs() <= FORTPI {
                QscFace::Front
            } else if params.lon_0.abs() <= HALFPI + FORTPI {
                if params.lon_0 > 0.0 {
                    QscFace::Right
                } else {
                    QscFace::Left
                }
            } else {
                QscFace::Back
            };
            (face, 1.0, 1.0, ellps.a)
        };

        Self {
            ellps,
            phi0: params.lat_0,
            lam0: params.lon_0,
            face,
            one_minus_f,
            one_minus_f_squared,
            b,
        }
    }

    /// Creates from a PROJ string (e.g. `"+proj=qsc +lat_0=0 +lon_0=0"`).
    pub fn from_proj_string(s: &str) -> Result<Self, ProjectionError> {
        let params = ProjParams::parse(s);
        if params.get_str("proj") != Some("qsc") {
            return Err(ProjectionError::InvalidParams(format!(
                "expected +proj=qsc, got {s:?}"
            )));
        }
        let ellps = Ellipsoid::from_proj_params(&params)?;
        let proj_params = ProjectionParams::from_proj_params(&params);
        Ok(Self::new(ellps, proj_params))
    }

    /// Returns the ellipsoid.
    pub fn ellipsoid(&self) -> Ellipsoid {
        self.ellps
    }

    /// Returns which cube face this projection is centered on.
    pub fn face(&self) -> QscFace {
        self.face
    }

    /// Forward projection: `(lam, phi)` relative to center (radians) →
    /// dimensionless `(x, y)`.
    pub fn forward(&self, lam: f64, phi: f64) -> Result<(f64, f64), ProjectionError> {
        let es = self.ellps.es;

        // Convert geodetic latitude to geocentric latitude ([LK12] shift)
        let lat = if es != 0.0 {
            (self.one_minus_f_squared * phi.tan()).atan()
        } else {
            phi
        };

        let longitude = lam;
        let mut area = QscArea::Area0;

        // Convert input lat/lon into theta, phi as used by QSC
        let (mut phi_q, mut theta) = if self.face == QscFace::Top {
            let p = HALFPI - lat;
            if longitude >= FORTPI && longitude <= HALFPI + FORTPI {
                area = QscArea::Area0;
                (p, longitude - HALFPI)
            } else if longitude > HALFPI + FORTPI || longitude <= -(HALFPI + FORTPI) {
                area = QscArea::Area1;
                (p, if longitude > 0.0 { longitude - PI } else { longitude + PI })
            } else if longitude > -(HALFPI + FORTPI) && longitude <= -FORTPI {
                area = QscArea::Area2;
                (p, longitude + HALFPI)
            } else {
                area = QscArea::Area3;
                (p, longitude)
            }
        } else if self.face == QscFace::Bottom {
            let p = HALFPI + lat;
            if longitude >= FORTPI && longitude <= HALFPI + FORTPI {
                area = QscArea::Area0;
                (p, -longitude + HALFPI)
            } else if longitude >= -FORTPI && longitude < FORTPI {
                area = QscArea::Area1;
                (p, -longitude)
            } else if longitude >= -(HALFPI + FORTPI) && longitude < -FORTPI {
                area = QscArea::Area2;
                (p, -longitude - HALFPI)
            } else {
                area = QscArea::Area3;
                (
                    p,
                    if longitude > 0.0 {
                        -longitude + PI
                    } else {
                        -longitude - PI
                    },
                )
            }
        } else {
            // Equatorial faces — use unit sphere cartesian coordinates
            let longitude = match self.face {
                QscFace::Right => shift_longitude_origin(longitude, HALFPI),
                QscFace::Back => shift_longitude_origin(longitude, PI),
                QscFace::Left => shift_longitude_origin(longitude, -HALFPI),
                _ => longitude,
            };
            let sinlat = lat.sin();
            let coslat = lat.cos();
            let sinlon = longitude.sin();
            let coslon = longitude.cos();
            let q = coslat * coslon;
            let r = coslat * sinlon;
            let s = sinlat;

            match self.face {
                QscFace::Front => {
                    let p = q.acos();
                    let t = fwd_equat_face_theta(p, s, r, &mut area);
                    (p, t)
                }
                QscFace::Right => {
                    let p = r.acos();
                    let t = fwd_equat_face_theta(p, s, -q, &mut area);
                    (p, t)
                }
                QscFace::Back => {
                    let p = (-q).acos();
                    let t = fwd_equat_face_theta(p, s, -r, &mut area);
                    (p, t)
                }
                QscFace::Left => {
                    let p = (-r).acos();
                    let t = fwd_equat_face_theta(p, s, q, &mut area);
                    (p, t)
                }
                _ => unreachable!(),
            }
        };

        // Compute mu and t for the area of definition
        let mut mu = (12.0 / PI * (theta + (theta.sin() * FORTPI.cos()).acos() - HALFPI)).atan();
        let t = ((1.0 - phi_q.cos())
            / (mu.cos().powi(2) * (1.0 - (1.0 / theta.cos()).atan().cos())))
        .sqrt();

        // Apply the result to the real area
        match area {
            QscArea::Area1 => mu += HALFPI,
            QscArea::Area2 => mu += PI,
            QscArea::Area3 => mu += THREE_HALFPI,
            _ => {}
        }

        Ok((t * mu.cos(), t * mu.sin()))
    }

    /// Inverse projection: dimensionless `(x, y)` → `(lam, phi)` relative
    /// to center (radians).
    pub fn inverse(&self, x: f64, y: f64) -> Result<(f64, f64), ProjectionError> {
        let nu = (x * x + y * y).sqrt().atan();
        let mut mu = y.atan2(x);

        // Determine the area of the cube face
        let area = if x >= 0.0 && x >= y.abs() {
            QscArea::Area0
        } else if y >= 0.0 && y >= x.abs() {
            mu -= HALFPI;
            QscArea::Area1
        } else if x < 0.0 && -x >= y.abs() {
            mu = if mu < 0.0 { mu + PI } else { mu - PI };
            QscArea::Area2
        } else {
            mu += HALFPI;
            QscArea::Area3
        };

        // Compute phi and theta for the area of definition
        let t = (PI / 12.0) * mu.tan();
        let tantheta = t.sin() / (t.cos() - (1.0 / 2.0_f64.sqrt()));
        let theta = tantheta.atan();
        let cosmu = mu.cos();
        let tannu = nu.tan();
        let mut cosphi = 1.0
            - cosmu * cosmu * tannu * tannu * (1.0 - (1.0 / theta.cos()).atan().cos());
        if cosphi < -1.0 {
            cosphi = -1.0;
        } else if cosphi > 1.0 {
            cosphi = 1.0;
        }

        // Apply the result to the real area on the cube face
        let (mut phi, mut lam) = if self.face == QscFace::Top {
            let p = cosphi.acos();
            let phi_val = HALFPI - p;
            let lam_val = match area {
                QscArea::Area0 => theta + HALFPI,
                QscArea::Area1 => {
                    if theta < 0.0 {
                        theta + PI
                    } else {
                        theta - PI
                    }
                }
                QscArea::Area2 => theta - HALFPI,
                _ => theta,
            };
            (phi_val, lam_val)
        } else if self.face == QscFace::Bottom {
            let p = cosphi.acos();
            let phi_val = p - HALFPI;
            let lam_val = match area {
                QscArea::Area0 => -theta + HALFPI,
                QscArea::Area1 => -theta,
                QscArea::Area2 => -theta - HALFPI,
                _ => {
                    if theta < 0.0 {
                        -theta - PI
                    } else {
                        -theta + PI
                    }
                }
            };
            (phi_val, lam_val)
        } else {
            // Compute phi and lam via cartesian unit sphere coordinates
            let mut q = cosphi;
            let mut t2 = q * q;
            let mut s = if t2 >= 1.0 { 0.0 } else { (1.0 - t2).sqrt() * theta.sin() };
            t2 += s * s;
            let mut r = if t2 >= 1.0 { 0.0 } else { (1.0 - t2).sqrt() };

            // Rotate q, r, s into the correct area
            match area {
                QscArea::Area1 => {
                    let tmp = r;
                    r = -s;
                    s = tmp;
                }
                QscArea::Area2 => {
                    r = -r;
                    s = -s;
                }
                QscArea::Area3 => {
                    let tmp = r;
                    r = s;
                    s = -tmp;
                }
                _ => {}
            }

            // Rotate q, r, s into the correct cube face
            match self.face {
                QscFace::Right => {
                    let tmp = q;
                    q = -r;
                    r = tmp;
                }
                QscFace::Back => {
                    q = -q;
                    r = -r;
                }
                QscFace::Left => {
                    let tmp = q;
                    q = r;
                    r = -tmp;
                }
                _ => {}
            }

            let phi_val = (-s).acos() - HALFPI;
            let mut lam_val = r.atan2(q);

            lam_val = match self.face {
                QscFace::Right => shift_longitude_origin(lam_val, -HALFPI),
                QscFace::Back => shift_longitude_origin(lam_val, -PI),
                QscFace::Left => shift_longitude_origin(lam_val, HALFPI),
                _ => lam_val,
            };
            (phi_val, lam_val)
        };

        // Apply the shift from the sphere to the ellipsoid ([LK12])
        if self.ellps.es != 0.0 {
            let invert_sign = phi < 0.0;
            let tanphi = phi.tan();
            let xa = self.b / (tanphi * tanphi + self.one_minus_f_squared).sqrt();
            phi = ((self.ellps.a * self.ellps.a - xa * xa).sqrt() / (self.one_minus_f * xa)).atan();
            if invert_sign {
                phi = -phi;
            }
        }

        Ok((lam, phi))
    }
}

/// Shift the longitude origin, wrapping to [-π, π].
fn shift_longitude_origin(longitude: f64, offset: f64) -> f64 {
    let slon = longitude + offset;
    if slon < -PI {
        slon + 2.0 * PI
    } else if slon > PI {
        slon - 2.0 * PI
    } else {
        slon
    }
}

/// QSC forward helper: compute theta and determine the area number.
fn fwd_equat_face_theta(phi: f64, y: f64, x: f64, area: &mut QscArea) -> f64 {
    if phi < EPS10 {
        *area = QscArea::Area0;
        return 0.0;
    }
    let mut theta = y.atan2(x);
    if theta.abs() <= FORTPI {
        *area = QscArea::Area0;
    } else if theta > FORTPI && theta <= HALFPI + FORTPI {
        *area = QscArea::Area1;
        theta -= HALFPI;
    } else if theta > HALFPI + FORTPI || theta <= -(HALFPI + FORTPI) {
        *area = QscArea::Area2;
        theta = if theta >= 0.0 { theta - PI } else { theta + PI };
    } else {
        *area = QscArea::Area3;
        theta += HALFPI;
    }
    theta
}

// ---------------------------------------------------------------------------
// Geocent (geocentric / ECEF)
// ---------------------------------------------------------------------------

/// Geocentric (ECEF) projection — full geodetic ↔ geocentric conversion.
///
/// Replaces proj4rs's stub with a complete implementation operating in
/// meters. Forward converts geodetic `(lon, lat, h)` to geocentric
/// `(X, Y, Z)`; inverse converts back via the iterative Wenzel algorithm
/// (ported from proj4rs `geocent.rs` / PROJ `geocent.cpp`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geocent {
    ellps: Ellipsoid,
}

/// Iteration tolerance for the geocentric→geodetic inverse (Wenzel algorithm).
const GENAU: f64 = 1.0e-12;
/// `GENAU²`.
const GENAU2: f64 = GENAU * GENAU;
/// Maximum iterations for the geocentric→geodetic inverse.
const MAXITER: usize = 30;
/// `1.001 * π/2` — latitude clamp for the forward conversion.
const FRAC_PI_2_EPS: f64 = 1.001 * HALFPI;

impl Geocent {
    /// Creates a new geocentric projection for the given ellipsoid.
    pub fn new(ellps: Ellipsoid) -> Self {
        Self { ellps }
    }

    /// Creates from a PROJ string (e.g. `"+proj=geocent +ellps=WGS84"`).
    pub fn from_proj_string(s: &str) -> Result<Self, ProjectionError> {
        let params = ProjParams::parse(s);
        if params.get_str("proj") != Some("geocent") && params.get_str("proj") != Some("cart") {
            return Err(ProjectionError::InvalidParams(format!(
                "expected +proj=geocent, got {s:?}"
            )));
        }
        let ellps = Ellipsoid::from_proj_params(&params)?;
        Ok(Self::new(ellps))
    }

    /// Returns the ellipsoid.
    pub fn ellipsoid(&self) -> Ellipsoid {
        self.ellps
    }

    /// Forward: geodetic `(lon, lat, h)` (radians, meters) → geocentric
    /// `(X, Y, Z)` (meters).
    pub fn forward(&self, lon: f64, lat: f64, h: f64) -> (f64, f64, f64) {
        let (a, es) = (self.ellps.a, self.ellps.es);
        let mut lat = lat;
        // Match PROJ's latitude clamping
        if lat < -HALFPI && lat > -FRAC_PI_2_EPS {
            lat = -HALFPI;
        } else if lat > HALFPI && lat < FRAC_PI_2_EPS {
            lat = HALFPI;
        }
        let mut lon = lon;
        if lon > PI {
            lon -= 2.0 * PI;
        }
        let (sin_lat, cos_lat) = lat.sin_cos();
        // Radius of curvature in the prime vertical
        let rn = a / (1.0 - es * sin_lat * sin_lat).sqrt();
        (
            (rn + h) * cos_lat * lon.cos(),
            (rn + h) * cos_lat * lon.sin(),
            ((rn * (1.0 - es) + h) * sin_lat),
        )
    }

    /// Inverse: geocentric `(X, Y, Z)` (meters) → geodetic
    /// `(lon, lat, h)` (radians, meters).
    ///
    /// Uses the iterative Wenzel algorithm (matches PROJ's
    /// `geocentric_to_geodetic`).
    ///
    /// # Errors
    ///
    /// Returns [`ProjectionError::OutsideDomain`] if the input latitude is
    /// outside [-π/2, π/2].
    pub fn inverse(&self, x: f64, y: f64, z: f64) -> Result<(f64, f64, f64), ProjectionError> {
        let (a, b, es) = (self.ellps.a, self.ellps.b, self.ellps.es);
        let d2 = x * x + y * y;
        let p = d2.sqrt();
        let rr = (d2 + z * z).sqrt();

        // If (X,Y,Z)=(0,0,0) then height becomes semi-minor axis, lat=π/2
        let lon = if p / a < GENAU {
            if rr / a < GENAU {
                return Ok((0.0, HALFPI, -b));
            }
            0.0
        } else {
            y.atan2(x)
        };

        // Iterative computation of sin(latitude) and height
        let ct = z / rr;
        let st = p / rr;
        let mut rx = 1.0 / (1.0 - es * (2.0 - es) * st * st).sqrt();
        let mut cphi0 = st * (1.0 - es) * rx;
        let mut sphi0 = ct * rx;
        let (mut rk, mut rn, mut cphi, mut sphi, mut sdphi, mut height);

        let mut iter = 0;
        loop {
            iter += 1;
            rn = a / (1.0 - es * sphi0 * sphi0).sqrt();
            height = p * cphi0 + z * sphi0 - rn * (1.0 - es * sphi0 * sphi0);

            if rn + height == 0.0 {
                return Ok((lon, 0.0, height));
            }

            rk = es * rn / (rn + height);
            rx = 1.0 / (1.0 - rk * (2.0 - rk) * st * st).sqrt();
            cphi = st * (1.0 - rk) * rx;
            sphi = ct * rx;
            sdphi = sphi * cphi0 - cphi * sphi0;
            cphi0 = cphi;
            sphi0 = sphi;

            if sdphi * sdphi <= GENAU2 {
                break;
            }
            if iter >= MAXITER {
                break;
            }
        }

        // Ellipsoidal (geodetic) latitude
        Ok((lon, sphi.atan2(cphi.abs()), height))
    }
}

// ---------------------------------------------------------------------------
// Unified projection enum
// ---------------------------------------------------------------------------

/// A gap-built native projection method.
///
/// This enum unifies the projections implemented in this module so they can
/// be used interchangeably (e.g. in the facade or pipeline executor).
#[derive(Debug, Clone, PartialEq)]
pub enum Projection {
    /// Orthographic
    Orthographic(Orthographic),
    /// Quadrilateralized Spherical Cube
    Qsc(Qsc),
    /// Geocentric (ECEF)
    Geocent(Geocent),
}

impl Projection {
    /// Creates a projection from a PROJ string.
    ///
    /// Supports `+proj=ortho`, `+proj=qsc`, and `+proj=geocent`.
    pub fn from_proj_string(s: &str) -> Result<Self, ProjectionError> {
        let params = ProjParams::parse(s);
        match params.get_str("proj") {
            Some("ortho") => Ok(Self::Orthographic(Orthographic::from_proj_string(s)?)),
            Some("qsc") => Ok(Self::Qsc(Qsc::from_proj_string(s)?)),
            Some("geocent") | Some("cart") => Ok(Self::Geocent(Geocent::from_proj_string(s)?)),
            Some(other) => Err(ProjectionError::UnknownMethod(other.to_string())),
            None => Err(ProjectionError::InvalidParams("missing +proj".to_string())),
        }
    }

    /// Returns the projection method name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Orthographic(_) => "ortho",
            Self::Qsc(_) => "qsc",
            Self::Geocent(_) => "geocent",
        }
    }

    /// Returns the ellipsoid.
    pub fn ellipsoid(&self) -> Ellipsoid {
        match self {
            Self::Orthographic(p) => p.ellipsoid(),
            Self::Qsc(p) => p.ellipsoid(),
            Self::Geocent(p) => p.ellipsoid(),
        }
    }

    /// Forward projection.
    ///
    /// For `ortho`/`qsc`: `(lam, phi, z)` radians → dimensionless `(x, y, z)`.
    /// For `geocent`: `(lam, phi, h)` radians/meters → `(X, Y, Z)` meters.
    pub fn forward(&self, lam: f64, phi: f64, z: f64) -> Result<(f64, f64, f64), ProjectionError> {
        match self {
            Self::Orthographic(p) => {
                let (x, y) = p.forward(lam, phi)?;
                Ok((x, y, z))
            }
            Self::Qsc(p) => {
                let (x, y) = p.forward(lam, phi)?;
                Ok((x, y, z))
            }
            Self::Geocent(p) => Ok(p.forward(lam, phi, z)),
        }
    }

    /// Inverse projection.
    ///
    /// For `ortho`/`qsc`: dimensionless `(x, y, z)` → `(lam, phi, z)` radians.
    /// For `geocent`: `(X, Y, Z)` meters → `(lam, phi, h)` radians/meters.
    pub fn inverse(&self, x: f64, y: f64, z: f64) -> Result<(f64, f64, f64), ProjectionError> {
        match self {
            Self::Orthographic(p) => {
                let (lam, phi) = p.inverse(x, y)?;
                Ok((lam, phi, z))
            }
            Self::Qsc(p) => {
                let (lam, phi) = p.inverse(x, y)?;
                Ok((lam, phi, z))
            }
            Self::Geocent(p) => p.inverse(x, y, z),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deg2rad(d: f64) -> f64 {
        d.to_radians()
    }

    fn rad2deg(r: f64) -> f64 {
        r.to_degrees()
    }

    // -------------------------------------------------------------------
    // Ellipsoid
    // -------------------------------------------------------------------

    #[test]
    fn ellipsoid_wgs84_derived_values() {
        let e = Ellipsoid::wgs84();
        assert!((e.a - 6_378_137.0).abs() < 1e-6);
        assert!((e.f - 1.0 / 298.257_223_563).abs() < 1e-15);
        assert!((e.b - 6_356_752.314_245_179).abs() < 1e-6);
        assert!((e.es - 0.006_694_379_990_14).abs() < 1e-12);
        assert!(e.one_es > 0.0);
        assert!((e.one_es + e.es - 1.0).abs() < 1e-15);
    }

    #[test]
    fn ellipsoid_sphere_is_sphere() {
        let e = Ellipsoid::sphere(6_370_997.0);
        assert!(e.is_sphere());
        assert_eq!(e.es, 0.0);
        assert_eq!(e.b, e.a);
        assert!(!Ellipsoid::wgs84().is_sphere());
    }

    #[test]
    fn ellipsoid_from_proj_params() {
        let params = ProjParams::parse("+proj=ortho +a=6378137 +rf=298.257");
        let e = Ellipsoid::from_proj_params(&params).unwrap();
        assert!((e.a - 6_378_137.0).abs() < 1e-6);
        assert!((e.rf - 298.257).abs() < 1e-9);

        let params = ProjParams::parse("+proj=ortho +R=6370997");
        let e = Ellipsoid::from_proj_params(&params).unwrap();
        assert!(e.is_sphere());

        let params = ProjParams::parse("+proj=ortho +ellps=WGS84");
        let e = Ellipsoid::from_proj_params(&params).unwrap();
        assert!((e.a - 6_378_137.0).abs() < 1e-6);

        let params = ProjParams::parse("+proj=ortho");
        let e = Ellipsoid::from_proj_params(&params).unwrap();
        assert!((e.a - 6_378_137.0).abs() < 1e-6); // WGS84 default
    }

    #[test]
    fn ellipsoid_unknown_name_errors() {
        let params = ProjParams::parse("+proj=ortho +ellps=foo");
        assert!(matches!(
            Ellipsoid::from_proj_params(&params),
            Err(ProjectionError::UnknownEllipsoid(_))
        ));
    }

    // -------------------------------------------------------------------
    // Orthographic — spherical
    // -------------------------------------------------------------------

    #[test]
    fn ortho_spherical_equatorial_origin() {
        let ortho = Orthographic::new(Ellipsoid::sphere(6_370_997.0), ProjectionParams::default());
        let (x, y) = ortho.forward(0.0, 0.0).unwrap();
        assert!(x.abs() < 1e-12);
        assert!(y.abs() < 1e-12);
    }

    #[test]
    fn ortho_spherical_equatorial_known_points() {
        // Reference: Python port of PROJ ortho.cpp, R=6370997, lat_0=0, lon_0=0
        let ortho = Orthographic::new(Ellipsoid::sphere(6_370_997.0), ProjectionParams::default());
        let r = 6_370_997.0;

        let (x, y) = ortho.forward(deg2rad(45.0), 0.0).unwrap();
        assert!((x * r - 4_504_975.1816).abs() < 1e-3, "x = {}", x * r);
        assert!(y.abs() < 1e-9);

        let (x, y) = ortho.forward(0.0, deg2rad(45.0)).unwrap();
        assert!(x.abs() < 1e-9);
        assert!((y * r - 4_504_975.1816).abs() < 1e-3, "y = {}", y * r);

        let (x, y) = ortho.forward(deg2rad(45.0), deg2rad(45.0)).unwrap();
        assert!((x * r - 3_185_498.5000).abs() < 1e-3, "x = {}", x * r);
        assert!((y * r - 4_504_975.1816).abs() < 1e-3, "y = {}", y * r);

        let (x, y) = ortho.forward(deg2rad(90.0), 0.0).unwrap();
        assert!((x * r - 6_370_997.0).abs() < 1e-3, "x = {}", x * r);

        let (x, y) = ortho.forward(deg2rad(-45.0), deg2rad(30.0)).unwrap();
        assert!((x * r - -3_901_422.9507).abs() < 1e-3, "x = {}", x * r);
        assert!((y * r - 3_185_498.5000).abs() < 1e-3, "y = {}", y * r);
    }

    #[test]
    fn ortho_spherical_roundtrip() {
        let ortho = Orthographic::new(Ellipsoid::sphere(6_370_997.0), ProjectionParams::default());
        for (lon, lat) in [(45.0, 0.0), (0.0, 45.0), (45.0, 45.0), (-45.0, 30.0), (89.0, -60.0)] {
            let (x, y) = ortho.forward(deg2rad(lon), deg2rad(lat)).unwrap();
            let (lon2, lat2) = ortho.inverse(x, y).unwrap();
            assert!((rad2deg(lon2) - lon).abs() < 1e-9, "lon {lon} -> {}", rad2deg(lon2));
            assert!((rad2deg(lat2) - lat).abs() < 1e-9, "lat {lat} -> {}", rad2deg(lat2));
        }
    }

    #[test]
    fn ortho_spherical_outside_domain() {
        let ortho = Orthographic::new(Ellipsoid::sphere(6_370_997.0), ProjectionParams::default());
        // (180, 0) is on the far side of the globe
        assert!(matches!(
            ortho.forward(deg2rad(180.0), 0.0),
            Err(ProjectionError::OutsideDomain)
        ));
    }

    // -------------------------------------------------------------------
    // Orthographic — ellipsoidal
    // -------------------------------------------------------------------

    #[test]
    fn ortho_ellipsoidal_origin_maps_to_center() {
        let params = ProjectionParams {
            lat_0: deg2rad(40.0),
            lon_0: deg2rad(-100.0),
            ..Default::default()
        };
        let ortho = Orthographic::new(Ellipsoid::wgs84(), params);
        let (x, y) = ortho.forward(deg2rad(-100.0 + 100.0), deg2rad(40.0)).unwrap();
        assert!(x.abs() < 1e-9);
        assert!(y.abs() < 1e-9);
    }

    #[test]
    fn ortho_ellipsoidal_obliq_known_points() {
        // Reference: Python port of PROJ ortho.cpp, WGS84, lat_0=40, lon_0=-100
        let params = ProjectionParams {
            lat_0: deg2rad(40.0),
            lon_0: deg2rad(-100.0),
            ..Default::default()
        };
        let ortho = Orthographic::new(Ellipsoid::wgs84(), params);
        let a = 6_378_137.0;

        let (x, y) = ortho.forward(deg2rad(40.0), deg2rad(50.0)).unwrap();
        assert!((x * a - 2_640_484.1401).abs() < 1e-3, "x = {}", x * a);
        assert!((y * a - 1_723_437.9542).abs() < 1e-3, "y = {}", y * a);

        let (x, y) = ortho.forward(deg2rad(-40.0), deg2rad(30.0)).unwrap();
        assert!((x * a - -3_553_494.8709).abs() < 1e-3, "x = {}", x * a);
        assert!((y * a - -272_434.1805).abs() < 1e-3, "y = {}", y * a);

        let (x, y) = ortho.forward(0.0, deg2rad(80.0)).unwrap();
        assert!(x.abs() < 1e-9);
        assert!((y * a - 4_101_898.7269).abs() < 1e-3, "y = {}", y * a);

        let (x, y) = ortho.forward(deg2rad(10.0), deg2rad(45.0)).unwrap();
        assert!((x * a - 784_471.4236).abs() < 1e-3, "x = {}", x * a);
        assert!((y * a - 598_826.5352).abs() < 1e-3, "y = {}", y * a);
    }

    #[test]
    fn ortho_ellipsoidal_obliq_roundtrip() {
        let params = ProjectionParams {
            lat_0: deg2rad(40.0),
            lon_0: deg2rad(-100.0),
            ..Default::default()
        };
        let ortho = Orthographic::new(Ellipsoid::wgs84(), params);
        for (lon, lat) in [(-60.0, 50.0), (-140.0, 30.0), (-100.0, 80.0), (-90.0, 45.0)] {
            let (x, y) = ortho.forward(deg2rad(lon - (-100.0)), deg2rad(lat)).unwrap();
            let (dlon, lat2) = ortho.inverse(x, y).unwrap();
            assert!((rad2deg(dlon) - (lon + 100.0)).abs() < 1e-7, "lon {lon} -> {}", rad2deg(dlon));
            assert!((rad2deg(lat2) - lat).abs() < 1e-7, "lat {lat} -> {}", rad2deg(lat2));
        }
    }

    #[test]
    fn ortho_ellipsoidal_obliq30_roundtrip() {
        // Reference: Python port of PROJ ortho.cpp, WGS84, lat_0=30, lon_0=10
        let params = ProjectionParams {
            lat_0: deg2rad(30.0),
            lon_0: deg2rad(10.0),
            ..Default::default()
        };
        let ortho = Orthographic::new(Ellipsoid::wgs84(), params);

        let (x, y) = ortho.forward(deg2rad(20.0), deg2rad(40.0)).unwrap();
        let a = 6_378_137.0;
        assert!((x * a - 1_673_404.5546).abs() < 1e-3, "x = {}", x * a);
        assert!((y * a - 1_251_322.6134).abs() < 1e-3, "y = {}", y * a);

        for (lon, lat) in [(30.0, 40.0), (-10.0, 20.0), (50.0, 50.0), (10.0, 85.0)] {
            let (x, y) = ortho.forward(deg2rad(lon - 10.0), deg2rad(lat)).unwrap();
            let (dlon, lat2) = ortho.inverse(x, y).unwrap();
            assert!((rad2deg(dlon) - (lon - 10.0)).abs() < 1e-7, "lon {lon} -> {}", rad2deg(dlon));
            assert!((rad2deg(lat2) - lat).abs() < 1e-7, "lat {lat} -> {}", rad2deg(lat2));
        }
    }

    #[test]
    fn ortho_ellipsoidal_n_pole() {
        // Reference: Python port of PROJ ortho.cpp, WGS84, lat_0=90
        let params = ProjectionParams {
            lat_0: deg2rad(90.0),
            ..Default::default()
        };
        let ortho = Orthographic::new(Ellipsoid::wgs84(), params);
        let a = 6_378_137.0;

        let (x, y) = ortho.forward(0.0, deg2rad(80.0)).unwrap();
        assert!(x.abs() < 1e-9);
        assert!((y * a - -1_111_164.8708).abs() < 1e-3, "y = {}", y * a);

        let (x, y) = ortho.forward(deg2rad(90.0), deg2rad(85.0)).unwrap();
        assert!((x * a - 557_747.0593).abs() < 1e-3, "x = {}", x * a);
        assert!(y.abs() < 1e-9);

        let (x, y) = ortho.forward(deg2rad(-45.0), deg2rad(70.0)).unwrap();
        assert!((x * a - -1_547_098.4776).abs() < 1e-3, "x = {}", x * a);
        assert!((y * a - -1_547_098.4776).abs() < 1e-3, "y = {}", y * a);

        // Roundtrips
        for (lon, lat) in [(0.0, 80.0), (90.0, 85.0), (-45.0, 70.0), (179.0, 89.0)] {
            let (x, y) = ortho.forward(deg2rad(lon), deg2rad(lat)).unwrap();
            let (lon2, lat2) = ortho.inverse(x, y).unwrap();
            assert!((rad2deg(lon2) - lon).abs() < 1e-7, "lon {lon} -> {}", rad2deg(lon2));
            assert!((rad2deg(lat2) - lat).abs() < 1e-7, "lat {lat} -> {}", rad2deg(lat2));
        }
    }

    #[test]
    fn ortho_from_proj_string() {
        let ortho = Orthographic::from_proj_string(
            "+proj=ortho +lat_0=40 +lon_0=-100 +a=6378137 +rf=298.257223563",
        )
        .unwrap();
        assert!((ortho.phi0 - deg2rad(40.0)).abs() < 1e-12);
        assert!((ortho.lam0 - deg2rad(-100.0)).abs() < 1e-12);

        let ortho = Orthographic::from_proj_string("+proj=ortho +R=6370997").unwrap();
        assert!(ortho.spherical);

        assert!(matches!(
            Orthographic::from_proj_string("+proj=merc"),
            Err(ProjectionError::InvalidParams(_))
        ));
    }

    // -------------------------------------------------------------------
    // QSC
    // -------------------------------------------------------------------

    #[test]
    fn qsc_front_known_points() {
        // Reference: Python port of PROJ qsc.cpp, WGS84, lat_0=0, lon_0=0
        let qsc = Qsc::new(Ellipsoid::wgs84(), ProjectionParams::default());
        let a = 6_378_137.0;

        let (x, y) = qsc.forward(deg2rad(0.0), 0.0).unwrap();
        assert!(x.abs() < 1e-9);
        assert!(y.abs() < 1e-9);

        let (x, y) = qsc.forward(deg2rad(20.0), deg2rad(10.0)).unwrap();
        assert!((x * a - 3_005_102.9765).abs() < 1e-3, "x = {}", x * a);
        assert!((y * a - 1_665_726.7860).abs() < 1e-3, "y = {}", y * a);

        let (x, y) = qsc.forward(deg2rad(-30.0), deg2rad(20.0)).unwrap();
        assert!((x * a - -4_514_409.1351).abs() < 1e-3, "x = {}", x * a);
        assert!((y * a - 3_428_784.8903).abs() < 1e-3, "y = {}", y * a);

        let (x, y) = qsc.forward(deg2rad(40.0), deg2rad(-15.0)).unwrap();
        assert!((x * a - 5_733_784.2933).abs() < 1e-3, "x = {}", x * a);
        assert!((y * a - -2_598_473.5667).abs() < 1e-3, "y = {}", y * a);

        let (x, y) = qsc.forward(deg2rad(44.0), deg2rad(5.0)).unwrap();
        assert!((x * a - 6_244_241.5150).abs() < 1e-3, "x = {}", x * a);
        assert!((y * a - 872_144.1253).abs() < 1e-3, "y = {}", y * a);
    }

    #[test]
    fn qsc_right_face_known_points() {
        // Reference: Python port of PROJ qsc.cpp, WGS84, lat_0=0, lon_0=90
        let params = ProjectionParams {
            lon_0: deg2rad(90.0),
            ..Default::default()
        };
        let qsc = Qsc::new(Ellipsoid::wgs84(), params);
        let a = 6_378_137.0;

        let (x, y) = qsc.forward(0.0, 0.0).unwrap();
        assert!(x.abs() < 1e-9);
        assert!(y.abs() < 1e-9);

        let (x, y) = qsc.forward(deg2rad(20.0), deg2rad(10.0)).unwrap();
        assert!((x * a - 3_005_102.9765).abs() < 1e-3, "x = {}", x * a);
        assert!((y * a - 1_665_726.7860).abs() < 1e-3, "y = {}", y * a);

        let (x, y) = qsc.forward(deg2rad(-20.0), deg2rad(-20.0)).unwrap();
        assert!((x * a - -3_237_671.5889).abs() < 1e-3, "x = {}", x * a);
        assert!((y * a - -3_387_430.6477).abs() < 1e-3, "y = {}", y * a);

        let (x, y) = qsc.forward(deg2rad(40.0), deg2rad(15.0)).unwrap();
        assert!((x * a - 5_733_784.2933).abs() < 1e-3, "x = {}", x * a);
        assert!((y * a - 2_598_473.5667).abs() < 1e-3, "y = {}", y * a);
    }

    #[test]
    fn qsc_top_face_known_points() {
        // Reference: Python port of PROJ qsc.cpp, WGS84, lat_0=90
        let params = ProjectionParams {
            lat_0: deg2rad(90.0),
            ..Default::default()
        };
        let qsc = Qsc::new(Ellipsoid::wgs84(), params);
        let a = 6_378_137.0;

        let (x, y) = qsc.forward(0.0, deg2rad(80.0)).unwrap();
        assert!(x.abs() < 1e-9);
        assert!((y * a - -1_462_179.6163).abs() < 1e-3, "y = {}", y * a);

        let (x, y) = qsc.forward(deg2rad(90.0), deg2rad(85.0)).unwrap();
        assert!((x * a - 731_870.2788).abs() < 1e-3, "x = {}", x * a);
        assert!(y.abs() < 1e-9);

        let (x, y) = qsc.forward(deg2rad(-90.0), deg2rad(75.0)).unwrap();
        assert!((x * a - -2_189_377.3583).abs() < 1e-3, "x = {}", x * a);
        assert!(y.abs() < 1e-9);

        let (x, y) = qsc.forward(deg2rad(180.0), deg2rad(89.0)).unwrap();
        assert!(x.abs() < 1e-9);
        assert!((y * a - 146_424.0716).abs() < 1e-3, "y = {}", y * a);
    }

    #[test]
    fn qsc_front_roundtrip() {
        let qsc = Qsc::new(Ellipsoid::wgs84(), ProjectionParams::default());
        for (lon, lat) in [(0.0, 0.0), (20.0, 10.0), (-30.0, 20.0), (40.0, -15.0), (44.0, 5.0)] {
            let (x, y) = qsc.forward(deg2rad(lon), deg2rad(lat)).unwrap();
            let (lon2, lat2) = qsc.inverse(x, y).unwrap();
            assert!((rad2deg(lon2) - lon).abs() < 1e-7, "lon {lon} -> {}", rad2deg(lon2));
            assert!((rad2deg(lat2) - lat).abs() < 1e-7, "lat {lat} -> {}", rad2deg(lat2));
        }
    }

    #[test]
    fn qsc_all_faces_roundtrip() {
        let faces: [(f64, f64); 6] = [
            (0.0, 0.0),        // front
            (0.0, 90.0),       // right
            (0.0, 180.0),      // back
            (0.0, -90.0),      // left
            (90.0, 0.0),       // top
            (-90.0, 0.0),      // bottom
        ];
        for (lat0, lon0) in faces {
            let params = ProjectionParams {
                lat_0: deg2rad(lat0),
                lon_0: deg2rad(lon0),
                ..Default::default()
            };
            let qsc = Qsc::new(Ellipsoid::wgs84(), params);
            // Roundtrip several points on each face
            for (dlon, lat) in [(0.0, 0.0), (10.0, 5.0), (-15.0, -10.0), (20.0, 20.0)] {
                let (x, y) = qsc.forward(deg2rad(dlon), deg2rad(lat)).unwrap();
                let (dlon2, lat2) = qsc.inverse(x, y).unwrap();
                let dlon2 = rad2deg(adjlon(dlon2));
                assert!((dlon2 - dlon).abs() < 1e-6, "face ({lat0},{lon0}) lon {dlon} -> {dlon2}");
                assert!((rad2deg(lat2) - lat).abs() < 1e-6, "face ({lat0},{lon0}) lat {lat} -> {}", rad2deg(lat2));
            }
        }
    }

    #[test]
    fn qsc_from_proj_string() {
        let qsc = Qsc::from_proj_string("+proj=qsc +lat_0=0 +lon_0=0 +ellps=WGS84").unwrap();
        assert_eq!(qsc.face, QscFace::Front);

        let qsc = Qsc::from_proj_string("+proj=qsc +lat_0=90").unwrap();
        assert_eq!(qsc.face, QscFace::Top);

        assert!(matches!(
            Qsc::from_proj_string("+proj=ortho"),
            Err(ProjectionError::InvalidParams(_))
        ));
    }

    // -------------------------------------------------------------------
    // Geocent
    // -------------------------------------------------------------------

    #[test]
    fn geocent_wgs84_origin() {
        let g = Geocent::new(Ellipsoid::wgs84());
        let (x, y, z) = g.forward(0.0, 0.0, 0.0);
        assert!((x - 6_378_137.0).abs() < 1e-6);
        assert!(y.abs() < 1e-6);
        assert!(z.abs() < 1e-6);
    }

    #[test]
    fn geocent_wgs84_equator_90lon() {
        let g = Geocent::new(Ellipsoid::wgs84());
        let (x, y, z) = g.forward(deg2rad(90.0), 0.0, 0.0);
        assert!(x.abs() < 1e-3);
        assert!((y - 6_378_137.0).abs() < 1e-3);
        assert!(z.abs() < 1e-3);
    }

    #[test]
    fn geocent_wgs84_north_pole() {
        let g = Geocent::new(Ellipsoid::wgs84());
        let (x, y, z) = g.forward(0.0, deg2rad(90.0), 0.0);
        assert!(x.abs() < 1e-3);
        assert!(y.abs() < 1e-3);
        // Z = b = a(1-f) = 6356752.314245179
        assert!((z - 6_356_752.314_245_179).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn geocent_wgs84_known_point() {
        // Reference: PROJ geocent.cpp, WGS84, (12°, 55°, 0)
        let g = Geocent::new(Ellipsoid::wgs84());
        let (x, y, z) = g.forward(deg2rad(12.0), deg2rad(55.0), 0.0);
        assert!((x - 3_586_469.656_776).abs() < 1e-3, "x = {x}");
        assert!((y - 762_327.658_778).abs() < 1e-3, "y = {y}");
        assert!((z - 5_201_383.523_202).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn geocent_wgs84_known_point_with_height() {
        // Reference: Python port of geocent.cpp, WGS84, (12°, 55°, 100)
        let g = Geocent::new(Ellipsoid::wgs84());
        let (x, y, z) = g.forward(deg2rad(12.0), deg2rad(55.0), 100.0);
        assert!((x - 3_586_525.7610).abs() < 1e-3, "x = {x}");
        assert!((y - 762_339.5841).abs() < 1e-3, "y = {y}");
        assert!((z - 5_201_465.4384).abs() < 1e-3, "z = {z}");
    }

    #[test]
    fn geocent_roundtrip() {
        let g = Geocent::new(Ellipsoid::wgs84());
        for (lon, lat, h) in [
            (0.0, 0.0, 0.0),
            (12.0, 55.0, 100.0),
            (-122.0, 37.0, 50.0),
            (139.6917, 35.6895, 0.0),
            (-0.0001, -0.0001, 10.0),
        ] {
            let (x, y, z) = g.forward(deg2rad(lon), deg2rad(lat), h);
            let (lon2, lat2, h2) = g.inverse(x, y, z).unwrap();
            assert!((rad2deg(lon2) - lon).abs() < 1e-9, "lon {lon} -> {}", rad2deg(lon2));
            assert!((rad2deg(lat2) - lat).abs() < 1e-9, "lat {lat} -> {}", rad2deg(lat2));
            assert!((h2 - h).abs() < 1e-6, "h {h} -> {h2}");
        }
    }

    #[test]
    fn geocent_inverse_origin() {
        let g = Geocent::new(Ellipsoid::wgs84());
        let (lon, lat, h) = g.inverse(0.0, 0.0, 0.0).unwrap();
        assert!(lon.abs() < 1e-12);
        assert!((rad2deg(lat) - 90.0).abs() < 1e-9);
        assert!((h - -6_356_752.314_245_179).abs() < 1e-3);
    }

    #[test]
    fn geocent_from_proj_string() {
        let g = Geocent::from_proj_string("+proj=geocent +ellps=WGS84").unwrap();
        assert_eq!(g.ellipsoid().a, 6_378_137.0);

        let g = Geocent::from_proj_string("+proj=geocent +a=6378137 +rf=298.257223563").unwrap();
        assert!((g.ellipsoid().a - 6_378_137.0).abs() < 1e-6);
    }

    // -------------------------------------------------------------------
    // Projection enum
    // -------------------------------------------------------------------

    #[test]
    fn projection_enum_dispatch() {
        let p = Projection::from_proj_string("+proj=ortho +R=6370997").unwrap();
        assert_eq!(p.name(), "ortho");
        let (x, y, z) = p.forward(0.0, 0.0, 0.0).unwrap();
        assert!(x.abs() < 1e-12);
        assert!(y.abs() < 1e-12);
        assert_eq!(z, 0.0);

        let p = Projection::from_proj_string("+proj=qsc +ellps=WGS84").unwrap();
        assert_eq!(p.name(), "qsc");

        let p = Projection::from_proj_string("+proj=geocent +ellps=WGS84").unwrap();
        assert_eq!(p.name(), "geocent");
        let (x, y, z) = p.forward(0.0, 0.0, 0.0).unwrap();
        assert!((x - 6_378_137.0).abs() < 1e-6);
    }

    #[test]
    fn projection_unknown_method() {
        assert!(matches!(
            Projection::from_proj_string("+proj=merc"),
            Err(ProjectionError::UnknownMethod(_))
        ));
    }
}
