//! Coordinate Reference Systems for the QGIS Rust port.
//!
//! Facade over the pure-Rust `proj4rs` crate for cartographic projection math and
//! `crs-definitions` for the EPSG dataset. WKT2:2019 parse/emit, datum ensembles,
//! GSB/GTX grids and `projinfo`/cct parity are built here, in Rust.
//!
//! See ADR-0006 for the projection decision and ADR-0001 for the no-FFI boundary.
//!
//! Current scope (bead qgis-rez.2, E0.5 GUI rendering POC): the
//! equirectangular projection ([`equirectangular_project`] /
//! [`equirectangular_unproject`]) and a [`Camera`] that maps between
//! projected world coordinates and screen pixels. PROJ-parity for other
//! projections is deferred to later beads.

#![forbid(unsafe_code)]

pub mod ensemble;
pub mod facade;
#[cfg(feature = "grids")]
pub mod grids;
pub mod pipeline;
pub mod pipeline_exec;
pub mod towgs84;
pub mod wkt2;

pub use ensemble::{resolve_ensemble, EnsembleError, ResolvedMember};
pub use facade::{CrsTransform, TransformError};
#[cfg(feature = "grids")]
pub use grids::{load_ntv2, load_ntv2_from_file, GridCatalog, GridError, Ntv2Grid};
pub use pipeline_exec::{execute_pipeline, PipelineError, UnitKind};
pub use towgs84::{Towgs84, Towgs84Error};
pub use wkt2::{emit_wkt2, parse_wkt2, ParsedCrs, Wkt2Error};

use std::f64::consts::PI;

/// A Spatial Reference System identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SrsId {
    id: i32,
}

impl SrsId {
    /// Creates a new SRS identifier from an EPSG code.
    pub fn from_epsg(epsg: i32) -> Self {
        Self { id: epsg }
    }

    /// Returns the EPSG code.
    pub fn epsg(&self) -> i32 {
        self.id
    }
}

impl std::fmt::Display for SrsId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EPSG:{}", self.id)
    }
}

/// Number of screen pixels the whole world spans at zoom 0.
///
/// This matches the standard web-mapping convention: at zoom 0 the full
/// 360° of longitude is squeezed into one 256 px tile.
const TILE_SIZE: f64 = 256.0;

/// Equirectangular projection: longitude/latitude (degrees) → world x/y (radians).
///
/// The simplest possible projection — each degree maps linearly to its
/// radian measure, so the world spans exactly `2 * PI` units in x and
/// `PI` units in y. World coordinates are kept in radians (rather than
/// degrees or metres) because the camera transform (see [`Camera`]) divides
/// the tile scale by `2 * PI`, which is only dimensionally correct when
/// world units are radians. This makes the whole pipeline
/// `lon/lat → project → screen` a pure scaling with no distortion
/// correction, suitable as the baseline projection for the E0.5
/// rendering POC.
pub fn equirectangular_project(lon: f64, lat: f64) -> (f64, f64) {
    (lon.to_radians(), lat.to_radians())
}

/// Inverse of [`equirectangular_project`]: world x/y (radians) → (lon, lat) in degrees.
pub fn equirectangular_unproject(x: f64, y: f64) -> (f64, f64) {
    (x.to_degrees(), y.to_degrees())
}

/// Pixel scale of the world at a zoom level: `TILE_SIZE * 2^zoom`.
///
/// At zoom 0 the whole world is [`TILE_SIZE`] pixels wide; each zoom level
/// doubles the scale.
fn world_scale(zoom: f64) -> f64 {
    TILE_SIZE * 2.0_f64.powf(zoom)
}

/// A camera defining the transformation between world and screen coordinates.
///
/// The camera tracks a center point in world coordinates (radians, as
/// produced by [`equirectangular_project`]), a zoom level, and the
/// viewport size in screen pixels. Screen coordinates have their origin at
/// the top-left corner of the viewport with y pointing down, matching
/// standard graphics conventions.
///
/// # Math
///
/// With `scale = 256 * 2^zoom` (the world width in pixels at this zoom):
///
/// ```text
/// screen_x = (world_x - center_x) * scale / (2 * PI) + viewport_width  / 2
/// screen_y = (center_y - world_y) * scale / (2 * PI) + viewport_height / 2
/// ```
///
/// The y axis is flipped because world y (latitude) increases northwards
/// while screen y increases downwards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Center of the viewport in world coordinates (radians): `(x, y)`.
    pub center: (f64, f64),
    /// Zoom level; `0.0` means the whole world spans 256 pixels.
    pub zoom: f64,
    /// Viewport size in screen pixels: `(width, height)`.
    pub viewport: (f64, f64),
}

impl Camera {
    /// Creates a new camera with the given center, zoom, and viewport.
    pub fn new(center: (f64, f64), zoom: f64, viewport: (f64, f64)) -> Self {
        Self {
            center,
            zoom,
            viewport,
        }
    }

    /// Returns a copy of this camera centered on a different world point.
    pub fn with_center(mut self, center: (f64, f64)) -> Self {
        self.center = center;
        self
    }

    /// Returns a copy of this camera at a different zoom level.
    pub fn with_zoom(mut self, zoom: f64) -> Self {
        self.zoom = zoom;
        self
    }

    /// Returns a copy of this camera with a different viewport size.
    pub fn with_viewport(mut self, viewport: (f64, f64)) -> Self {
        self.viewport = viewport;
        self
    }

    /// Transforms a world coordinate (radians) to screen pixel coordinates.
    ///
    /// The camera center lands on the viewport center; the world x axis
    /// increases rightwards, the world y axis increases upwards (screen y
    /// is flipped).
    pub fn world_to_screen(&self, world: (f64, f64)) -> (f64, f64) {
        let scale = world_scale(self.zoom);
        let sx = (world.0 - self.center.0) * scale / (2.0 * PI) + self.viewport.0 / 2.0;
        let sy = (self.center.1 - world.1) * scale / (2.0 * PI) + self.viewport.1 / 2.0;
        (sx, sy)
    }

    /// Inverse of [`world_to_screen`]: screen pixel coordinates to world
    /// coordinates (radians).
    pub fn screen_to_world(&self, screen: (f64, f64)) -> (f64, f64) {
        let scale = world_scale(self.zoom);
        let wx = (screen.0 - self.viewport.0 / 2.0) * (2.0 * PI) / scale + self.center.0;
        let wy = self.center.1 - (screen.1 - self.viewport.1 / 2.0) * (2.0 * PI) / scale;
        (wx, wy)
    }

    /// Projects longitude/latitude (degrees) straight to screen pixels.
    ///
    /// Convenience for the standard rendering pipeline:
    /// `equirectangular_project` followed by [`world_to_screen`].
    pub fn lon_lat_to_screen(&self, lon: f64, lat: f64) -> (f64, f64) {
        self.world_to_screen(equirectangular_project(lon, lat))
    }

    /// Inverse of [`lon_lat_to_screen`]: screen pixels to (lon, lat) in degrees.
    pub fn screen_to_lon_lat(&self, screen: (f64, f64)) -> (f64, f64) {
        let world = self.screen_to_world(screen);
        equirectangular_unproject(world.0, world.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{E, FRAC_PI_2, FRAC_PI_4, SQRT_2};

    #[test]
    fn srs_id_roundtrip() {
        let srs = SrsId::from_epsg(4326);
        assert_eq!(srs.epsg(), 4326);
    }

    #[test]
    fn srs_id_display() {
        let srs = SrsId::from_epsg(3857);
        assert_eq!(format!("{}", srs), "EPSG:3857");
    }

    #[test]
    fn equirectangular_project_known_values() {
        assert_eq!(equirectangular_project(0.0, 0.0), (0.0, 0.0));
        assert_eq!(equirectangular_project(180.0, 0.0), (PI, 0.0));
        assert_eq!(equirectangular_project(-180.0, 90.0), (-PI, FRAC_PI_2));
        assert_eq!(equirectangular_project(90.0, -45.0), (FRAC_PI_2, -FRAC_PI_4));
    }

    #[test]
    fn equirectangular_unproject_known_values() {
        assert_eq!(equirectangular_unproject(0.0, 0.0), (0.0, 0.0));
        assert_eq!(equirectangular_unproject(PI, 0.0), (180.0, 0.0));
        assert_eq!(equirectangular_unproject(-PI, FRAC_PI_2), (-180.0, 90.0));
    }

    #[test]
    fn equirectangular_roundtrip() {
        let points = [
            (0.0, 0.0),
            (180.0, 90.0),
            (-180.0, -90.0),
            (-122.4194, 37.7749),
            (139.6917, 35.6895),
            (0.0, -90.0),
            (179.9999, 89.9999),
        ];
        for (lon, lat) in points {
            let (x, y) = equirectangular_project(lon, lat);
            let (lon2, lat2) = equirectangular_unproject(x, y);
            assert!(
                (lon2 - lon).abs() < 1e-9,
                "lon roundtrip failed for {lon}: got {lon2}"
            );
            assert!(
                (lat2 - lat).abs() < 1e-9,
                "lat roundtrip failed for {lat}: got {lat2}"
            );
        }
    }

    #[test]
    fn camera_center_maps_to_viewport_center() {
        // Any center maps to the middle of the viewport, at any zoom.
        let cam = Camera::new((0.5, -1.25), 3.0, (800.0, 600.0));
        let (sx, sy) = cam.world_to_screen((0.5, -1.25));
        assert!((sx - 400.0).abs() < 1e-9);
        assert!((sy - 300.0).abs() < 1e-9);
    }

    #[test]
    fn camera_zoom_zero_known_scale() {
        // Zoom 0: scale = 256, so the full 2*PI world spans 256 px and
        // quarter-world offsets land exactly on tile boundaries.
        let cam = Camera::new((0.0, 0.0), 0.0, (256.0, 256.0));
        let (sx, sy) = cam.world_to_screen((PI, 0.0)); // lon 180°
        assert!((sx - 256.0).abs() < 1e-9, "sx = {sx}");
        assert!((sy - 128.0).abs() < 1e-9, "sy = {sy}");

        let (sx, sy) = cam.world_to_screen((-PI, 0.0)); // lon -180°
        assert!((sx - 0.0).abs() < 1e-9, "sx = {sx}");
        assert!((sy - 128.0).abs() < 1e-9, "sy = {sy}");

        // Latitudes span PI while the viewport spans 2*PI vertically, so
        // lat ±90° sit at 1/4 and 3/4 of the viewport height.
        let (sx, sy) = cam.world_to_screen((0.0, FRAC_PI_2)); // lat 90°
        assert!((sx - 128.0).abs() < 1e-9, "sx = {sx}");
        assert!((sy - 64.0).abs() < 1e-9, "sy = {sy}");

        let (sx, sy) = cam.world_to_screen((0.0, -FRAC_PI_2)); // lat -90°
        assert!((sx - 128.0).abs() < 1e-9, "sx = {sx}");
        assert!((sy - 192.0).abs() < 1e-9, "sy = {sy}");
    }

    #[test]
    fn camera_zoom_one_doubles_scale() {
        // Zoom 1: scale = 512, so the same world offsets cover twice the pixels.
        let cam = Camera::new((0.0, 0.0), 1.0, (256.0, 256.0));
        let (sx, sy) = cam.world_to_screen((FRAC_PI_2, 0.0)); // lon 90°
        assert!((sx - 256.0).abs() < 1e-9, "sx = {sx}");
        assert!((sy - 128.0).abs() < 1e-9, "sy = {sy}");

        let (sx, sy) = cam.world_to_screen((PI, 0.0)); // lon 180°
        assert!((sx - 384.0).abs() < 1e-9, "sx = {sx}");
        assert!((sy - 128.0).abs() < 1e-9, "sy = {sy}");
    }

    #[test]
    fn camera_world_to_screen_then_back_roundtrip() {
        let cameras = [
            Camera::new((0.0, 0.0), 0.0, (256.0, 256.0)),
            Camera::new((1.0, -2.0), 2.5, (1024.0, 768.0)),
            Camera::new((-3.0, 0.5), 10.0, (800.0, 600.0)),
        ];
        let worlds = [
            (0.0, 0.0),
            (PI, FRAC_PI_2),
            (-PI, -FRAC_PI_2),
            (E, -SQRT_2),
        ];
        for cam in cameras {
            for world in worlds {
                let screen = cam.world_to_screen(world);
                let recovered = cam.screen_to_world(screen);
                assert!(
                    (recovered.0 - world.0).abs() < 1e-9,
                    "world x roundtrip failed for {world:?} at zoom {}: got {:?}",
                    cam.zoom,
                    recovered
                );
                assert!(
                    (recovered.1 - world.1).abs() < 1e-9,
                    "world y roundtrip failed for {world:?} at zoom {}: got {:?}",
                    cam.zoom,
                    recovered
                );
            }
        }
    }

    #[test]
    fn camera_screen_to_world_then_back_roundtrip() {
        let cam = Camera::new((0.3, -0.7), 1.5, (800.0, 600.0));
        let screens = [(0.0, 0.0), (800.0, 600.0), (400.0, 300.0), (123.4, 456.7)];
        for screen in screens {
            let world = cam.screen_to_world(screen);
            let recovered = cam.world_to_screen(world);
            assert!((recovered.0 - screen.0).abs() < 1e-9, "sx roundtrip failed");
            assert!((recovered.1 - screen.1).abs() < 1e-9, "sy roundtrip failed");
        }
    }

    #[test]
    fn camera_viewport_corners() {
        // Zoom 0, center (0,0), viewport (800,600): the viewport center is
        // world (0,0); corners are quarter-world offsets of
        // (400, 300) px * 2*PI / 256.
        let cam = Camera::new((0.0, 0.0), 0.0, (800.0, 600.0));
        let corner_world = |sx: f64, sy: f64| {
            let wx = (sx - 400.0) * (2.0 * PI) / 256.0;
            let wy = -(sy - 300.0) * (2.0 * PI) / 256.0;
            (wx, wy)
        };
        for (sx, sy) in [(0.0, 0.0), (800.0, 0.0), (0.0, 600.0), (800.0, 600.0)] {
            let world = corner_world(sx, sy);
            let (rx, ry) = cam.world_to_screen(world);
            assert!((rx - sx).abs() < 1e-9, "corner ({sx}, {sy}): rx = {rx}");
            assert!((ry - sy).abs() < 1e-9, "corner ({sx}, {sy}): ry = {ry}");
        }

        // The viewport center screen point is exactly world (0, 0).
        let world = cam.screen_to_world((400.0, 300.0));
        assert!(world.0.abs() < 1e-9);
        assert!(world.1.abs() < 1e-9);
    }

    #[test]
    fn camera_negative_coordinates() {
        // Southern/western hemisphere: negative center, world points at
        // negative (south/west) offsets from it.
        let cam = Camera::new((-2.0, -1.0), 0.0, (256.0, 256.0));
        let (sx, sy) = cam.world_to_screen((-2.0 - PI, -1.0));
        assert!((sx - 0.0).abs() < 1e-9, "sx = {sx}");
        assert!((sy - 128.0).abs() < 1e-9, "sy = {sy}");

        let (sx, sy) = cam.world_to_screen((-2.0, -1.0 - FRAC_PI_2));
        assert!((sx - 128.0).abs() < 1e-9, "sx = {sx}");
        assert!((sy - 192.0).abs() < 1e-9, "sy = {sy}");

        let recovered = cam.screen_to_world((sx, sy));
        assert!((recovered.0 - -2.0).abs() < 1e-9);
        assert!((recovered.1 - (-1.0 - FRAC_PI_2)).abs() < 1e-9);

        // Roundtrips with negative lon/lat through the full lon/lat pipeline.
        for (lon, lat) in [(-122.4194, -37.7749), (-0.0001, -0.0001), (-180.0, -90.0)] {
            let screen = cam.lon_lat_to_screen(lon, lat);
            let (lon2, lat2) = cam.screen_to_lon_lat(screen);
            assert!((lon2 - lon).abs() < 1e-9, "lon {lon} -> {lon2}");
            assert!((lat2 - lat).abs() < 1e-9, "lat {lat} -> {lat2}");
        }
    }

    #[test]
    fn camera_builder_methods() {
        let cam = Camera::new((0.0, 0.0), 0.0, (256.0, 256.0));
        let moved = cam.with_center((1.0, 2.0)).with_zoom(4.0).with_viewport((100.0, 50.0));
        assert_eq!(moved.center, (1.0, 2.0));
        assert_eq!(moved.zoom, 4.0);
        assert_eq!(moved.viewport, (100.0, 50.0));

        // Builders mutate a copy; the original is unchanged.
        assert_eq!(cam.center, (0.0, 0.0));
        assert_eq!(cam.zoom, 0.0);
        assert_eq!(cam.viewport, (256.0, 256.0));
    }

    #[test]
    fn camera_fractional_zoom() {
        // Fractional zoom is allowed; scale = 256 * 2^0.5.
        let cam = Camera::new((0.0, 0.0), 0.5, (256.0, 256.0));
        let world = (PI, 0.0);
        let screen = cam.world_to_screen(world);
        let expected_sx = 128.0 * 2.0_f64.sqrt() + 128.0;
        assert!((screen.0 - expected_sx).abs() < 1e-9, "sx = {}", screen.0);
        assert!((screen.1 - 128.0).abs() < 1e-9);
        let recovered = cam.screen_to_world(screen);
        assert!((recovered.0 - world.0).abs() < 1e-9);
        assert!((recovered.1 - world.1).abs() < 1e-9);
    }

    #[test]
    fn lon_lat_pipeline_end_to_end() {
        // Full pipeline at zoom 0: lon/lat -> project -> screen, on a
        // 256x256 viewport centered on (0, 0).
        let cam = Camera::new((0.0, 0.0), 0.0, (256.0, 256.0));
        assert_eq!(cam.lon_lat_to_screen(0.0, 0.0), (128.0, 128.0));
        assert_eq!(cam.lon_lat_to_screen(180.0, 0.0), (256.0, 128.0));
        assert_eq!(cam.lon_lat_to_screen(-180.0, 0.0), (0.0, 128.0));
        assert_eq!(cam.lon_lat_to_screen(0.0, 90.0), (128.0, 64.0));
        assert_eq!(cam.lon_lat_to_screen(0.0, -90.0), (128.0, 192.0));

        // And back: screen -> world -> unproject. Screen (160, 96) is a
        // quarter-world offset of (32, -32) px, i.e. exactly 45°.
        let (lon, lat) = cam.screen_to_lon_lat((160.0, 96.0));
        assert!((lon - 45.0).abs() < 1e-9, "lon = {lon}");
        assert!((lat - 45.0).abs() < 1e-9, "lat = {lat}");
    }
}
