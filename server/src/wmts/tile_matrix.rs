//! WMTS tile matrix set definitions and tile addressing arithmetic.
//!
//! Implements the OGC WMTS 1.0.0 tiling model: a tile matrix set is an
//! ordered list of tile matrices, each with a scale denominator, a top-left
//! corner, a matrix width/height in tiles, and a tile size in pixels.
//!
//! Two well-known matrix sets are provided:
//! - `EPSG4326` — global geodetic, level 0 is 2x1 tiles of 180°x90°
//! - `WebMercatorQuad` — spherical Mercator, level 0 is a single tile
//!
//! Tile bounds are returned in the matrix set's **native** CRS. The server
//! currently has no reprojection in the tile path, so the caller renders the
//! returned bounds directly against the same native CRS; reprojected
//! WebMercatorQuad tiles are tracked as follow-up work.
//!
//! Reference: OGC 07-057r7 WMTS, section 6 (Tiling schemes) and 7.2 (Tile addressing).

/// Default tile width in pixels.
pub const DEFAULT_TILE_WIDTH: usize = 256;

/// Default tile height in pixels.
pub const DEFAULT_TILE_HEIGHT: usize = 256;

/// WGS 84 equatorial radius in metres, the sphere radius of the spherical
/// Mercator development.
pub const EARTH_RADIUS: f64 = 6_378_137.0;

/// Half the Mercator extent in metres; equal to `pi * EARTH_RADIUS`.
pub const WEB_MERCATOR_HALF: f64 = 20_037_508.342_789_248;

/// Full Mercator span in metres.
pub const WEB_MERCATOR_SPAN: f64 = 40_075_016.685_578_496;

/// Latitude limit of the spherical Mercator projection, in degrees.
pub const WEB_MERCATOR_MAX_LAT: f64 = 85.051_128_78;

/// Scale denominator at level 0 of `EPSG4326` (pixels of 0.35277777 mm).
pub const GEOGRAPHIC_SCALE_DENOMINATOR_L0: f64 = 279_541_132.014_358_9;

/// Scale denominator at level 0 of `WebMercatorQuad`.
pub const WEB_MERCATOR_SCALE_DENOMINATOR_L0: f64 = 559_082_264.028_717_8;

/// Number of zoom levels published for each built-in matrix set.
pub const GEOGRAPHIC_LEVELS: usize = 19;

/// A single tile matrix (one zoom level).
#[derive(Debug, Clone, PartialEq)]
pub struct TileMatrix {
    /// Matrix identifier (the zoom level as a string).
    pub identifier: String,
    /// Scale denominator.
    pub scale_denominator: f64,
    /// Top-left corner as `[min_x, max_y]`.
    pub top_left_corner: [f64; 2],
    /// Tile width in pixels.
    pub tile_width: usize,
    /// Tile height in pixels.
    pub tile_height: usize,
    /// Number of tiles across at this level.
    pub matrix_width: usize,
    /// Number of tiles down at this level.
    pub matrix_height: usize,
}

impl TileMatrix {
    /// Returns the identifier of this matrix.
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Returns the scale denominator.
    pub fn scale_denominator(&self) -> f64 {
        self.scale_denominator
    }

    /// Returns the native-CRS bounding box of a tile at (row, col).
    ///
    /// `origin` is the north-west corner of the whole matrix and `span` is
    /// the total extent covered across the matrix. Returns `None` when the
    /// row or column is out of range.
    pub fn tile_bounds(
        &self,
        origin: [f64; 2],
        span: (f64, f64),
        row: usize,
        col: usize,
    ) -> Option<(f64, f64, f64, f64)> {
        if row >= self.matrix_height || col >= self.matrix_width {
            return None;
        }

        let span_x = span.0 / self.matrix_width as f64;
        let span_y = span.1 / self.matrix_height as f64;

        let min_x = origin[0] + col as f64 * span_x;
        let max_y = origin[1] - row as f64 * span_y;

        Some((min_x, max_y - span_y, min_x + span_x, max_y))
    }
}

/// A tile matrix set: an ordered list of tile matrices in one CRS.
#[derive(Debug, Clone, PartialEq)]
pub struct TileMatrixSet {
    /// Matrix set identifier.
    pub identifier: String,
    /// CRS identifier the tiles are addressed in.
    pub crs: String,
    /// North-west corner of the whole matrix set as `[min_x, max_y]`.
    pub top_left_corner: [f64; 2],
    /// Total extent covered by the matrix set as `(span_x, span_y)`.
    pub span: (f64, f64),
    /// Tile matrices ordered from level 0 upwards.
    pub matrices: Vec<TileMatrix>,
}

impl TileMatrixSet {
    /// Returns the matrix set identifier.
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Returns the CRS identifier.
    pub fn crs(&self) -> &str {
        &self.crs
    }

    /// Returns the tile matrices.
    pub fn matrices(&self) -> &[TileMatrix] {
        &self.matrices
    }

    /// Looks up a tile matrix by identifier.
    pub fn matrix(&self, identifier: &str) -> Option<&TileMatrix> {
        self.matrices.iter().find(|m| m.identifier == identifier)
    }

    /// Returns the native-CRS bounding box of a tile addressed by
    /// matrix identifier, row and column.
    pub fn tile_bounds(&self, matrix: &str, row: usize, col: usize) -> Option<(f64, f64, f64, f64)> {
        let found = self.matrix(matrix)?;
        found.tile_bounds(self.top_left_corner, self.span, row, col)
    }
}

/// Builds the `EPSG4326` global geodetic tile matrix set.
///
/// Level 0 is 2x1 tiles of 180°x90°; each subsequent level doubles the tile
/// count in both directions.
pub fn geographic_matrix_set() -> TileMatrixSet {
    let mut matrices = Vec::with_capacity(GEOGRAPHIC_LEVELS);

    for level in 0..GEOGRAPHIC_LEVELS {
        let factor = 1u32 << level;
        matrices.push(TileMatrix {
            identifier: level.to_string(),
            scale_denominator: GEOGRAPHIC_SCALE_DENOMINATOR_L0 / factor as f64,
            top_left_corner: [-180.0, 90.0],
            tile_width: DEFAULT_TILE_WIDTH,
            tile_height: DEFAULT_TILE_HEIGHT,
            matrix_width: 2 * factor as usize,
            matrix_height: factor as usize,
        });
    }

    TileMatrixSet {
        identifier: "EPSG4326".to_string(),
        crs: "EPSG:4326".to_string(),
        top_left_corner: [-180.0, 90.0],
        span: (360.0, 180.0),
        matrices,
    }
}

/// Builds the `WebMercatorQuad` spherical Mercator tile matrix set.
///
/// Level 0 is a single tile covering the whole Mercator extent.
pub fn web_mercator_matrix_set() -> TileMatrixSet {
    let mut matrices = Vec::with_capacity(GEOGRAPHIC_LEVELS);
    let span = (WEB_MERCATOR_SPAN, WEB_MERCATOR_SPAN);

    for level in 0..GEOGRAPHIC_LEVELS {
        let factor = 1u32 << level;
        matrices.push(TileMatrix {
            identifier: level.to_string(),
            scale_denominator: WEB_MERCATOR_SCALE_DENOMINATOR_L0 / factor as f64,
            top_left_corner: [-WEB_MERCATOR_HALF, WEB_MERCATOR_HALF],
            tile_width: DEFAULT_TILE_WIDTH,
            tile_height: DEFAULT_TILE_HEIGHT,
            matrix_width: factor as usize,
            matrix_height: factor as usize,
        });
    }

    TileMatrixSet {
        identifier: "WebMercatorQuad".to_string(),
        crs: "EPSG:3857".to_string(),
        top_left_corner: [-WEB_MERCATOR_HALF, WEB_MERCATOR_HALF],
        span,
        matrices,
    }
}

/// Returns the built-in tile matrix set with the given identifier.
///
/// Recognises `EPSG4326`/`EPSG:4326`/`Geographic` and
/// `WebMercatorQuad`/`EPSG3857`/`EPSG:3857`.
pub fn tile_matrix_set(identifier: &str) -> Option<TileMatrixSet> {
    match identifier.to_uppercase().as_str() {
        "EPSG4326" | "EPSG:4326" | "GEOGRAPHIC" => Some(geographic_matrix_set()),
        "WEBMERCATORQUAD" | "EPSG3857" | "EPSG:3857" | "GOOGLEMAPSCOMPATIBLE" => {
            Some(web_mercator_matrix_set())
        }
        _ => None,
    }
}

/// Returns the native-CRS bounding box of a tile addressed by matrix set
/// identifier, matrix identifier, row and column.
///
/// Returns `None` when the matrix set, matrix, row or column is unknown.
pub fn tile_bounds(
    matrix_set_id: &str,
    matrix_id: &str,
    row: usize,
    col: usize,
) -> Option<(f64, f64, f64, f64)> {
    tile_matrix_set(matrix_set_id)?.tile_bounds(matrix_id, row, col)
}

/// Projects a longitude (degrees) onto the Web Mercator x axis (metres).
pub fn mercator_x(lon: f64) -> f64 {
    lon * WEB_MERCATOR_HALF / 180.0
}

/// Projects a latitude (degrees) onto the Web Mercator y axis (metres).
///
/// Uses the spherical development `y = R * atanh(sin(phi))`, so the
/// projection limit latitude maps exactly onto the Mercator extent.
pub fn mercator_y(lat: f64) -> f64 {
    let clamped = lat.clamp(-WEB_MERCATOR_MAX_LAT, WEB_MERCATOR_MAX_LAT);
    EARTH_RADIUS * clamped.to_radians().sin().atanh()
}

/// Converts a Web Mercator x ordinate (metres) back to a longitude.
pub fn mercator_to_lon(x: f64) -> f64 {
    x * 180.0 / WEB_MERCATOR_HALF
}

/// Converts a Web Mercator y ordinate (metres) back to a latitude.
pub fn mercator_to_lat(y: f64) -> f64 {
    (y / EARTH_RADIUS).tanh().asin().to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geographic_level_zero() {
        let set = geographic_matrix_set();
        assert_eq!(set.identifier(), "EPSG4326");
        assert_eq!(set.crs(), "EPSG:4326");
        assert_eq!(set.matrices().len(), GEOGRAPHIC_LEVELS);

        let level0 = set.matrix("0").unwrap();
        assert_eq!(level0.matrix_width, 2);
        assert_eq!(level0.matrix_height, 1);
        assert_eq!(level0.tile_width, 256);
        assert_eq!(level0.tile_height, 256);
        assert!((level0.scale_denominator - GEOGRAPHIC_SCALE_DENOMINATOR_L0).abs() < 1e-6);
    }

    #[test]
    fn geographic_doubles_each_level() {
        let set = geographic_matrix_set();
        let l0 = set.matrix("0").unwrap();
        let l1 = set.matrix("1").unwrap();
        let l5 = set.matrix("5").unwrap();
        assert_eq!(l1.matrix_width, 4);
        assert_eq!(l1.matrix_height, 2);
        assert_eq!(l5.matrix_width, 64);
        assert_eq!(l5.matrix_height, 32);
        assert!((l0.scale_denominator / l1.scale_denominator - 2.0).abs() < 1e-9);
    }

    #[test]
    fn geographic_top_left_is_north_west() {
        let set = geographic_matrix_set();
        assert_eq!(set.matrix("3").unwrap().top_left_corner, [-180.0, 90.0]);
        assert_eq!(set.top_left_corner, [-180.0, 90.0]);
        assert_eq!(set.span, (360.0, 180.0));
    }

    #[test]
    fn geographic_level0_tiles_cover_the_globe() {
        let set = geographic_matrix_set();
        // Two tiles: western [-180, 0] and eastern [0, 180], both full height.
        let west = set.tile_bounds("0", 0, 0).unwrap();
        assert_eq!(west, (-180.0, -90.0, 0.0, 90.0));
        let east = set.tile_bounds("0", 0, 1).unwrap();
        assert_eq!(east, (0.0, -90.0, 180.0, 90.0));
    }

    #[test]
    fn geographic_level1_quarter_tiles() {
        let set = geographic_matrix_set();
        // 4x2 tiles of 90°x90°.
        assert_eq!(set.tile_bounds("1", 0, 0).unwrap(), (-180.0, 0.0, -90.0, 90.0));
        assert_eq!(set.tile_bounds("1", 1, 3).unwrap(), (90.0, -90.0, 180.0, 0.0));
    }

    #[test]
    fn geographic_tiles_are_disjoint_and_tiling() {
        let set = geographic_matrix_set();
        let matrix = set.matrix("3").unwrap();
        let mut seen = 0usize;
        for row in 0..matrix.matrix_height {
            for col in 0..matrix.matrix_width {
                let (min_x, min_y, max_x, max_y) = set.tile_bounds("3", row, col).unwrap();
                assert!(min_x < max_x && min_y < max_y);
                seen += 1;
            }
        }
        assert_eq!(seen, matrix.matrix_width * matrix.matrix_height);
    }

    #[test]
    fn geographic_tile_out_of_range() {
        let set = geographic_matrix_set();
        assert!(set.tile_bounds("0", 1, 0).is_none());
        assert!(set.tile_bounds("0", 0, 2).is_none());
    }

    #[test]
    fn matrix_lookup_missing() {
        let set = geographic_matrix_set();
        assert!(set.matrix("999").is_none());
        assert!(set.tile_bounds("999", 0, 0).is_none());
    }

    #[test]
    fn web_mercator_level_zero_is_single_tile() {
        let set = web_mercator_matrix_set();
        assert_eq!(set.identifier(), "WebMercatorQuad");
        assert_eq!(set.crs(), "EPSG:3857");
        let l0 = set.matrix("0").unwrap();
        assert_eq!(l0.matrix_width, 1);
        assert_eq!(l0.matrix_height, 1);
        assert_eq!(l0.top_left_corner[0], -WEB_MERCATOR_HALF);
        assert_eq!(set.top_left_corner[1], WEB_MERCATOR_HALF);
    }

    #[test]
    fn web_mercator_level0_covers_full_extent() {
        let set = web_mercator_matrix_set();
        let (min_x, min_y, max_x, max_y) = set.tile_bounds("0", 0, 0).unwrap();
        assert!((min_x + WEB_MERCATOR_HALF).abs() < 1e-6);
        assert!((max_x - WEB_MERCATOR_HALF).abs() < 1e-6);
        assert!((max_y - WEB_MERCATOR_HALF).abs() < 1e-6);
        assert!((min_y + WEB_MERCATOR_HALF).abs() < 1e-6);
    }

    #[test]
    fn web_mercator_level1_splits_east_west() {
        let set = web_mercator_matrix_set();
        let (min_x, _, max_x, _) = set.tile_bounds("1", 0, 0).unwrap();
        assert!((min_x + WEB_MERCATOR_HALF).abs() < 1e-6);
        assert!(max_x.abs() < 1e-6, "max_x {max_x}");
        let (min_x, _, _, _) = set.tile_bounds("1", 0, 1).unwrap();
        assert!(min_x.abs() < 1e-6);
    }

    #[test]
    fn matrix_set_lookup_aliases() {
        assert!(tile_matrix_set("EPSG4326").is_some());
        assert!(tile_matrix_set("epsg:4326").is_some());
        assert!(tile_matrix_set("WebMercatorQuad").is_some());
        assert!(tile_matrix_set("EPSG:3857").is_some());
        assert!(tile_matrix_set("nope").is_none());
    }

    #[test]
    fn mercator_projection_roundtrip() {
        for lon in [-180.0f64, -90.0, 0.0, 45.0, 180.0] {
            let x = mercator_x(lon);
            assert!((mercator_to_lon(x) - lon).abs() < 1e-9, "lon {lon}");
        }
        for lat in [-85.0f64, -45.0, 0.0, 45.0, 85.0] {
            let y = mercator_y(lat);
            assert!((mercator_to_lat(y) - lat).abs() < 1e-9, "lat {lat}");
        }
    }

    #[test]
    fn mercator_corners() {
        assert!((mercator_x(-180.0) + WEB_MERCATOR_HALF).abs() < 1e-6);
        assert!((mercator_x(180.0) - WEB_MERCATOR_HALF).abs() < 1e-6);
        assert!(mercator_y(0.0).abs() < 1e-9);
        // The projection clamps beyond its latitude limit.
        assert!(mercator_y(89.0) >= WEB_MERCATOR_HALF - 1.0);
    }

    #[test]
    fn mercator_lat_limit_matches_extent() {
        assert!(
            (mercator_y(WEB_MERCATOR_MAX_LAT) - WEB_MERCATOR_HALF).abs() < 1.0,
            "y at limit {}",
            mercator_y(WEB_MERCATOR_MAX_LAT)
        );
    }

    #[test]
    fn tile_bounds_invalid_inputs() {
        assert!(tile_bounds("nope", "0", 0, 0).is_none());
        assert!(tile_bounds("EPSG4326", "99", 0, 0).is_none());
        assert!(tile_bounds("EPSG4326", "0", 5, 0).is_none());
        assert!(tile_bounds("EPSG4326", "0", 0, 5).is_none());
        assert!(tile_bounds("WebMercatorQuad", "0", 1, 0).is_none());
    }

    #[test]
    fn tile_bounds_helper_matches_set_lookup() {
        let a = tile_bounds("EPSG4326", "2", 1, 2).unwrap();
        let b = geographic_matrix_set().tile_bounds("2", 1, 2).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn matrix_own_bounds_helper_matches_set() {
        let set = geographic_matrix_set();
        let matrix = set.matrix("1").unwrap();
        let direct = matrix.tile_bounds(set.top_left_corner, set.span, 1, 2).unwrap();
        assert_eq!(direct, set.tile_bounds("1", 1, 2).unwrap());
        assert!(matrix.tile_bounds(set.top_left_corner, set.span, 9, 0).is_none());
    }
}