//! qgis-crs facade API: degrees-in / degrees-out.
//!
//! proj4rs uses radians internally. This module wraps `Proj::from_proj_string`
//! and `proj4rs::transform::transform` so that all public APIs accept and return
//! degrees. The conversion happens at the boundary and never leaks.
//!
//! Projection methods that proj4rs lacks (ortho, qsc — see ADR 0006) are
//! routed to the native implementations in [`crate::projection`]. Geocent
//! stays on proj4rs, whose transform layer already performs the full
//! geodetic↔geocentric conversion.

#![forbid(unsafe_code)]

use crate::projection::{Projection, ProjectionError};

/// A coordinate reference system transform.
///
/// Wraps a proj4rs `Proj` (or a native gap-built [`Projection`]) and provides
/// degrees-in / degrees-out transformation. Internally proj4rs works in
/// radians; we convert at the boundary.
#[derive(Debug, Clone)]
pub struct CrsTransform {
    inner: InnerTransform,
}

/// The underlying transform backend.
#[derive(Debug, Clone)]
enum InnerTransform {
    /// A proj4rs projection.
    ///
    /// Boxed: `proj4rs::Proj` is much larger than the native variant, and
    /// boxing keeps the enum variant sizes balanced.
    Proj4rs(Box<proj4rs::Proj>),
    /// A native gap-built projection (ortho, qsc).
    Native(Projection),
}

/// Errors that can occur during CRS transformation.
#[derive(Debug, Clone, PartialEq)]
pub enum TransformError {
    /// The proj string could not be parsed.
    InvalidProjString(String),
    /// The transformation failed for the given input.
    TransformFailed { reason: String },
    /// The EPSG code is not recognized.
    UnknownEpsgCode(i32),
}

impl std::fmt::Display for TransformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<>) -> std::fmt::Result {
        match self {
            Self::InvalidProjString(s) => write!(f, "invalid proj string: {s}"),
            Self::TransformFailed { reason } => write!(f, "transform failed: {reason}"),
            Self::UnknownEpsgCode(code) => write!(f, "unknown EPSG code: {code}"),
        }
    }
}

impl std::error::Error for TransformError {}

impl From<ProjectionError> for TransformError {
    fn from(e: ProjectionError) -> Self {
        Self::TransformFailed {
            reason: e.to_string(),
        }
    }
}

impl CrsTransform {
    /// Creates a new CRS transform from a proj string.
    ///
    /// # Arguments
    ///
    /// * `proj_string` - A PROJ string (e.g., `"+proj=longlat +datum=WGS84"`)
    ///
    /// # Errors
    ///
    /// Returns [`TransformError::InvalidProjString`] if the string cannot be parsed.
    pub fn from_proj_string(proj_string: &str) -> Result<Self, TransformError> {
        // Route gap-built methods to the native implementations
        let params = crate::projection::ProjParams::parse(proj_string);
        match params.get_str("proj") {
            Some("ortho") | Some("qsc") => {
                let inner = Projection::from_proj_string(proj_string)
                    .map(InnerTransform::Native)
                    .map_err(|e| TransformError::InvalidProjString(e.to_string()))?;
                return Ok(Self { inner });
            }
            _ => {}
        }

        let inner = proj4rs::Proj::from_proj_string(proj_string)
            .map_err(|e| TransformError::InvalidProjString(e.to_string()))?;
        Ok(Self {
            inner: InnerTransform::Proj4rs(Box::new(inner)),
        })
    }

    /// Creates a new CRS transform from an EPSG code.
    ///
    /// Uses crs-definitions to resolve the code to a proj string.
    ///
    /// # Arguments
    ///
    /// * `epsg` - An EPSG code (e.g., 4326 for WGS 84)
    ///
    /// # Errors
    ///
    /// Returns [`TransformError::UnknownEpsgCode`] if the code is not in the dataset.
    pub fn from_epsg(epsg: i32) -> Result<Self, TransformError> {
        let code = u16::try_from(epsg).map_err(|_| TransformError::UnknownEpsgCode(epsg))?;
        let proj_string = crs_definitions::from_code(code)
            .ok_or(TransformError::UnknownEpsgCode(epsg))?
            .proj4
            .to_string();
        Self::from_proj_string(&proj_string)
    }

    /// Returns the projection method name, if known.
    pub fn projname(&self) -> String {
        match &self.inner {
            InnerTransform::Proj4rs(p) => p.projname().to_string(),
            InnerTransform::Native(p) => p.name().to_string(),
        }
    }

    /// Transforms a point from this CRS to another CRS.
    ///
    /// # Arguments
    ///
    /// * `x` - X coordinate in degrees (longitude for geographic CRS)
    /// * `y` - Y coordinate in degrees (latitude for geographic CRS)
    /// * `target` - The target CRS transform
    ///
    /// # Returns
    ///
    /// A tuple `(x, y)` in degrees, or a [`TransformError`] on failure.
    pub fn transform_point(
        &self,
        x: f64,
        y: f64,
        target: &CrsTransform,
    ) -> Result<(f64, f64), TransformError> {
        use InnerTransform::*;

        match (&self.inner, &target.inner) {
            (Proj4rs(src), Proj4rs(dst)) => {
                // Convert degrees to radians for proj4rs
                let mut point = (x.to_radians(), y.to_radians());

                proj4rs::transform::transform(src, dst, &mut point).map_err(|e| {
                    TransformError::TransformFailed {
                        reason: e.to_string(),
                    }
                })?;

                // Convert radians back to degrees
                Ok((point.0.to_degrees(), point.1.to_degrees()))
            }
            (Native(src), Native(dst)) => {
                // Both sides native: inverse-project source, forward-project target
                let (lam, phi, _) = src.inverse(x.to_radians(), y.to_radians(), 0.0)?;
                let (xr, yr, _) = dst.forward(lam, phi, 0.0)?;
                Ok((xr.to_degrees(), yr.to_degrees()))
            }
            (Native(src), Proj4rs(dst)) => {
                // Native source → geographic (radians) → proj4rs target
                let (lam, phi, _) = src.inverse(x.to_radians(), y.to_radians(), 0.0)?;
                let latlong = latlong_wgs84();
                let mut point = (lam, phi, 0.0);
                proj4rs::transform::transform(&latlong, dst, &mut point).map_err(|e| {
                    TransformError::TransformFailed {
                        reason: e.to_string(),
                    }
                })?;
                Ok((point.0.to_degrees(), point.1.to_degrees()))
            }
            (Proj4rs(src), Native(dst)) => {
                // proj4rs source → geographic (radians) → native target
                let latlong = latlong_wgs84();
                let mut point = (x.to_radians(), y.to_radians(), 0.0);
                proj4rs::transform::transform(src, &latlong, &mut point).map_err(|e| {
                    TransformError::TransformFailed {
                        reason: e.to_string(),
                    }
                })?;
                let (xr, yr, _) = dst.forward(point.0, point.1, 0.0)?;
                Ok((xr.to_degrees(), yr.to_degrees()))
            }
        }
    }

    /// Transforms a point in-place from this CRS to another CRS.
    ///
    /// This is a convenience wrapper around [`CrsTransform::transform_point`].
    pub fn transform_point_mut(
        &self,
        point: &mut (f64, f64),
        target: &CrsTransform,
    ) -> Result<(), TransformError> {
        let (x, y) = self.transform_point(point.0, point.1, target)?;
        *point = (x, y);
        Ok(())
    }
}

/// A WGS 84 latlong `Proj` used as the geographic intermediate when mixing
/// proj4rs and native projections.
fn latlong_wgs84() -> proj4rs::Proj {
    proj4rs::Proj::from_proj_string("+proj=longlat +ellps=WGS84")
        .expect("WGS84 latlong proj string is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_epsg_wgs84() {
        let transform = CrsTransform::from_epsg(4326);
        assert!(transform.is_ok(), "failed: {:?}", transform.err());
    }

    #[test]
    fn from_epsg_web_mercator() {
        let transform = CrsTransform::from_epsg(3857);
        assert!(transform.is_ok(), "failed: {:?}", transform.err());
    }

    #[test]
    fn invalid_epsg_code() {
        let transform = CrsTransform::from_epsg(999999);
        assert!(matches!(transform, Err(TransformError::UnknownEpsgCode(999999))));
    }

    #[test]
    fn identity_transform() {
        let crs = CrsTransform::from_epsg(4326).unwrap();
        let mut point = (10.0, 20.0);
        crs.transform_point_mut(&mut point, &crs).unwrap();
        assert!((point.0 - 10.0).abs() < 1e-9);
        assert!((point.1 - 20.0).abs() < 1e-9);
    }

    #[test]
    fn wgs84_to_web_mercator() {
        let wgs84 = CrsTransform::from_epsg(4326).unwrap();
        let web_merc = CrsTransform::from_epsg(3857).unwrap();
        let mut point = (0.0, 0.0);
        wgs84.transform_point_mut(&mut point, &web_merc).unwrap();
        assert!((point.0 - 0.0).abs() < 1e-9);
        assert!((point.1 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn from_epsg_nad83() {
        let transform = CrsTransform::from_epsg(4269);
        assert!(transform.is_ok(), "failed: {:?}", transform.err());
    }

    #[test]
    fn from_epsg_utm_zone_33n() {
        let transform = CrsTransform::from_epsg(32633);
        assert!(transform.is_ok(), "failed: {:?}", transform.err());
    }

    #[test]
    fn from_epsg_etrs89_utm_zone_32n() {
        let transform = CrsTransform::from_epsg(25832);
        assert!(transform.is_ok(), "failed: {:?}", transform.err());
    }

    #[test]
    fn from_epsg_etrs89_laea_europe() {
        let transform = CrsTransform::from_epsg(3035);
        assert!(transform.is_ok(), "failed: {:?}", transform.err());
    }

    #[test]
    fn from_epsg_rgf93_lambert_93() {
        let transform = CrsTransform::from_epsg(2154);
        assert!(transform.is_ok(), "failed: {:?}", transform.err());
    }

    #[test]
    fn from_epsg_wgs84_3d_geographic() {
        let transform = CrsTransform::from_epsg(4979);
        assert!(transform.is_ok(), "failed: {:?}", transform.err());
    }

    #[test]
    fn from_epsg_wgs84_geocentric() {
        let transform = CrsTransform::from_epsg(4978);
        assert!(transform.is_ok(), "failed: {:?}", transform.err());
    }

    #[test]
    fn from_epsg_nad83_to_wgs84() {
        let nad83 = CrsTransform::from_epsg(4269).unwrap();
        let wgs84 = CrsTransform::from_epsg(4326).unwrap();
        let mut point = (-122.0, 37.0);
        nad83.transform_point_mut(&mut point, &wgs84).unwrap();
        // NAD83 to WGS84 should be very close (sub-meter)
        assert!((point.0 - -122.0).abs() < 0.01);
        assert!((point.1 - 37.0).abs() < 0.01);
    }

    #[test]
    fn from_epsg_web_mercator_to_wgs84_roundtrip() {
        let wgs84 = CrsTransform::from_epsg(4326).unwrap();
        let web_merc = CrsTransform::from_epsg(3857).unwrap();
        let original = (10.0, 50.0);
        let mut projected = original;
        wgs84.transform_point_mut(&mut projected, &web_merc).unwrap();
        let mut recovered = projected;
        web_merc.transform_point_mut(&mut recovered, &wgs84).unwrap();
        assert!((recovered.0 - original.0).abs() < 1e-6);
        assert!((recovered.1 - original.1).abs() < 1e-6);
    }

    // -------------------------------------------------------------------
    // Native projection integration (ADR 0006 gap build)
    // -------------------------------------------------------------------

    #[test]
    fn from_proj_string_ortho_parses() {
        let t = CrsTransform::from_proj_string(
            "+proj=ortho +lat_0=40 +lon_0=-100 +a=6378137 +rf=298.257223563",
        );
        assert!(t.is_ok(), "failed: {:?}", t.err());
        assert_eq!(t.unwrap().projname(), "ortho");
    }

    #[test]
    fn from_proj_string_qsc_parses() {
        let t = CrsTransform::from_proj_string("+proj=qsc +lat_0=0 +lon_0=0 +ellps=WGS84");
        assert!(t.is_ok(), "failed: {:?}", t.err());
        assert_eq!(t.unwrap().projname(), "qsc");
    }

    #[test]
    fn ortho_native_roundtrip_through_facade() {
        let ortho = CrsTransform::from_proj_string(
            "+proj=ortho +lat_0=0 +lon_0=0 +R=6370997",
        )
        .unwrap();
        let mut point = (45.0, 30.0);
        ortho.transform_point_mut(&mut point, &ortho).unwrap();
        assert!((point.0 - 45.0).abs() < 1e-9, "x = {}", point.0);
        assert!((point.1 - 30.0).abs() < 1e-9, "y = {}", point.1);
    }

    #[test]
    fn qsc_native_roundtrip_through_facade() {
        let qsc = CrsTransform::from_proj_string("+proj=qsc +lat_0=0 +lon_0=0 +ellps=WGS84")
            .unwrap();
        let mut point = (20.0, 10.0);
        qsc.transform_point_mut(&mut point, &qsc).unwrap();
        assert!((point.0 - 20.0).abs() < 1e-9, "x = {}", point.0);
        assert!((point.1 - 10.0).abs() < 1e-9, "y = {}", point.1);
    }

    #[test]
    fn ortho_to_wgs84_transform() {
        // Transform from ortho (spherical, R=6370997) to WGS84 latlong.
        // The ortho CRS is spherical so the ellipsoid differs from WGS84;
        // the facade routes through a WGS84 latlong intermediate.
        let ortho = CrsTransform::from_proj_string(
            "+proj=ortho +lat_0=0 +lon_0=0 +R=6370997",
        )
        .unwrap();
        let wgs84 = CrsTransform::from_epsg(4326).unwrap();
        // Center of projection maps to (0, 0)
        let mut point = (0.0, 0.0);
        ortho.transform_point_mut(&mut point, &wgs84).unwrap();
        assert!(point.0.abs() < 1e-6, "x = {}", point.0);
        assert!(point.1.abs() < 1e-6, "y = {}", point.1);
    }

    #[test]
    fn wgs84_to_ortho_transform() {
        let wgs84 = CrsTransform::from_epsg(4326).unwrap();
        let ortho = CrsTransform::from_proj_string(
            "+proj=ortho +lat_0=0 +lon_0=0 +R=6370997",
        )
        .unwrap();
        let mut point = (0.0, 0.0);
        wgs84.transform_point_mut(&mut point, &ortho).unwrap();
        assert!(point.0.abs() < 1e-6, "x = {}", point.0);
        assert!(point.1.abs() < 1e-6, "y = {}", point.1);
    }
}
