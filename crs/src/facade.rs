//! qgis-crs facade API: degrees-in / degrees-out.
//!
//! proj4rs uses radians internally. This module wraps `Proj::from_proj_string`
//! and `proj4rs::transform::transform` so that all public APIs accept and return
//! degrees. The conversion happens at the boundary and never leaks.

#![forbid(unsafe_code)]

use std::f64::consts::PI;

/// A coordinate reference system transform.
///
/// Wraps a proj4rs `Proj` and provides degrees-in / degrees-out transformation.
/// Internally proj4rs works in radians; we convert at the boundary.
#[derive(Debug, Clone)]
pub struct CrsTransform {
    inner: proj4rs::Proj,
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
        let inner = proj4rs::Proj::from_proj_string(proj_string)
            .map_err(|e| TransformError::InvalidProjString(e.to_string()))?;
        Ok(Self { inner })
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
        // Convert degrees to radians for proj4rs
        let mut point = (x.to_radians(), y.to_radians());


        proj4rs::transform::transform(
            &self.inner,
            &target.inner,
            &mut point,
        ).map_err(|e| TransformError::TransformFailed {
            reason: e.to_string(),
        })?;
        let (x_out_rad, y_out_rad) = point;

        // Convert radians back to degrees
        Ok((point.0.to_degrees(), point.1.to_degrees()))
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
}
