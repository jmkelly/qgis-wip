//! WMTS GetTile handler — renders a single map tile.
//!
//! Resolves the tile's geographic extent from the OGC tile matrix set and
//! renders the requested layer over it, reusing the WMS renderer.
//!
//! Reference: OGC 07-057r7 section 7 (GetTile).

use bytes::Bytes;
use qgis_render::Pixel;
use tracing::debug;

use crate::error::Result;
use crate::fcgi::FcgiResponse;
use crate::query::QueryParameters;
use crate::service::service_exception;
use crate::wms::get_map::{GetMapParams, load_layers_from_project, render_map};

use super::tile_matrix::{DEFAULT_TILE_HEIGHT, DEFAULT_TILE_WIDTH, tile_bounds, tile_matrix_set};

/// Default tile output format.
pub const DEFAULT_TILE_FORMAT: &str = "image/png";

/// Tile formats this server can encode.
pub const WMTS_TILE_FORMATS: &[&str] = &["image/png", "image/jpeg", "image/gif"];

/// Parsed and validated WMTS GetTile parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct GetTileParams {
    /// Layer identifier to render.
    pub layer: String,
    /// Style identifier (only the default style is published).
    pub style: String,
    /// Tile matrix set identifier.
    pub matrix_set: String,
    /// Tile matrix (zoom level) identifier.
    pub matrix: String,
    /// Zero-based tile row.
    pub row: usize,
    /// Zero-based tile column.
    pub col: usize,
    /// Output MIME type.
    pub format: String,
    /// Tile width in pixels.
    pub width: usize,
    /// Tile height in pixels.
    pub height: usize,
}

impl GetTileParams {
    /// Parses GetTile parameters from query string parameters.
    ///
    /// Returns a ServiceException response describing the first problem.
    pub fn from_query(query: &QueryParameters) -> std::result::Result<Self, FcgiResponse> {
        let layer = query.get("LAYER").unwrap_or("");
        if layer.is_empty() {
            return Err(service_exception(
                "MissingParameterValue",
                "LAYER parameter is required",
            ));
        }

        let style = query.get_or("STYLE", "default").to_string();

        let matrix_set = query.get("TILEMATRIXSET").unwrap_or("");
        if matrix_set.is_empty() {
            return Err(service_exception(
                "MissingParameterValue",
                "TILEMATRIXSET parameter is required",
            ));
        }

        let matrix = query.get("TILEMATRIX").unwrap_or("");
        if matrix.is_empty() {
            return Err(service_exception(
                "MissingParameterValue",
                "TILEMATRIX parameter is required",
            ));
        }

        let row = parse_index(query.get("TILEROW"), "TILEROW")?;
        let col = parse_index(query.get("TILECOL"), "TILECOL")?;

        let format = query.get_or("FORMAT", DEFAULT_TILE_FORMAT).to_string();
        if !WMTS_TILE_FORMATS.contains(&format.as_str()) {
            return Err(service_exception(
                "InvalidParameterValue",
                format!(
                    "Unsupported FORMAT '{}'. Supported: {}",
                    format,
                    WMTS_TILE_FORMATS.join(", ")
                ),
            ));
        }

        let Some(set) = tile_matrix_set(matrix_set) else {
            return Err(service_exception(
                "InvalidParameterValue",
                format!("Unknown TILEMATRIXSET '{}'", matrix_set),
            ));
        };

        if set.matrix(matrix).is_none() {
            return Err(service_exception(
                "InvalidParameterValue",
                format!("Unknown TILEMATRIX '{}'", matrix),
            ));
        }

        Ok(Self {
            layer: layer.to_string(),
            style,
            matrix_set: matrix_set.to_string(),
            matrix: matrix.to_string(),
            row,
            col,
            format,
            width: DEFAULT_TILE_WIDTH,
            height: DEFAULT_TILE_HEIGHT,
        })
    }

    /// Returns the geographic bounding box addressed by these parameters.
    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        tile_bounds(&self.matrix_set, &self.matrix, self.row, self.col)
    }
}

/// Parses a tile row or column index.
fn parse_index(value: Option<&str>, name: &str) -> std::result::Result<usize, FcgiResponse> {
    let raw = value.ok_or_else(|| {
        service_exception(
            "MissingParameterValue",
            format!("{name} parameter is required"),
        )
    })?;

    let index = raw.parse::<usize>().map_err(|_| {
        service_exception(
            "InvalidParameterValue",
            format!("{name} must be a non-negative integer"),
        )
    })?;

    Ok(index)
}

/// Renders a tile for the given parameters.
///
/// Returns a ServiceException response when the tile address is out of range
/// for the matrix set.
pub fn render_tile(
    params: &GetTileParams,
    layers: &[crate::wms::get_map::WmsLayer],
) -> std::result::Result<Vec<u8>, FcgiResponse> {
    let Some(bbox) = params.bounds() else {
        return Err(service_exception(
            "InvalidParameterValue",
            format!(
                "Tile ({}, {}) is outside TILEMATRIX '{}' of TILEMATRIXSET '{}'",
                params.row, params.col, params.matrix, params.matrix_set
            ),
        ));
    };

    let map_params = GetMapParams {
        layers: vec![params.layer.clone()],
        styles: vec![params.style.clone()],
        crs: crs_for_tiles(&params.matrix_set),
        bbox,
        width: params.width as u32,
        height: params.height as u32,
        format: params.format.clone(),
        transparent: false,
        bg_color: Pixel::rgb(255, 255, 255),
        exceptions: String::new(),
    };

    render_map(&map_params, layers)
}

/// Returns the CRS identifier a matrix set is addressed in.
///
/// Unknown matrix sets fall back to geographic coordinates so callers always
/// get a usable rendering CRS.
pub fn crs_for_tiles(matrix_set: &str) -> String {
    match tile_matrix_set(matrix_set) {
        Some(set) => set.crs().to_string(),
        None => "EPSG:4326".to_string(),
    }
}

/// WMTS GetTile handler.
pub fn handle_get_tile(query: &QueryParameters) -> Result<FcgiResponse> {
    let params = match GetTileParams::from_query(query) {
        Ok(p) => p,
        Err(resp) => return Ok(resp),
    };

    let map_path = query.map().unwrap_or("");
    if map_path.is_empty() {
        return Ok(service_exception(
            "MissingParameterValue",
            "MAP parameter is required",
        ));
    }

    debug!(
        layer = %params.layer,
        matrix_set = %params.matrix_set,
        matrix = %params.matrix,
        row = params.row,
        col = params.col,
        "WMTS GetTile request"
    );

    let layers = load_layers_from_project(map_path, std::slice::from_ref(&params.layer));

    match render_tile(&params, &layers) {
        Ok(tile) => Ok(FcgiResponse::new(Bytes::from(tile))),
        Err(resp) => Ok(resp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qgis_geometry::{Geometry, LineString, Point};
    use std::collections::HashMap;

    use crate::wms::get_map::{LayerStyle, WmsFeature, WmsLayer};

    /// Returns the body of an error response as a string.
    fn err_body(result: std::result::Result<GetTileParams, FcgiResponse>) -> String {
        String::from_utf8_lossy(&result.unwrap_err().body).to_string()
    }

    fn make_query(params: &[(&str, &str)]) -> QueryParameters {
        let query_string = params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        QueryParameters::parse(&query_string).unwrap()
    }

    fn base_params() -> GetTileParams {
        GetTileParams {
            layer: "roads".to_string(),
            style: "default".to_string(),
            matrix_set: "EPSG4326".to_string(),
            matrix: "1".to_string(),
            row: 0,
            col: 0,
            format: DEFAULT_TILE_FORMAT.to_string(),
            width: DEFAULT_TILE_WIDTH,
            height: DEFAULT_TILE_HEIGHT,
        }
    }

    #[test]
    fn parse_full_params() {
        let query = make_query(&[
            ("MAP", "/tmp/p.qgs"),
            ("LAYER", "roads"),
            ("STYLE", "default"),
            ("TILEMATRIXSET", "WebMercatorQuad"),
            ("TILEMATRIX", "3"),
            ("TILEROW", "2"),
            ("TILECOL", "5"),
            ("FORMAT", "image/png"),
        ]);
        let params = GetTileParams::from_query(&query).unwrap();
        assert_eq!(params.layer, "roads");
        assert_eq!(params.style, "default");
        assert_eq!(params.matrix_set, "WebMercatorQuad");
        assert_eq!(params.matrix, "3");
        assert_eq!(params.row, 2);
        assert_eq!(params.col, 5);
        assert_eq!(params.format, "image/png");
    }

    #[test]
    fn parse_defaults_style_and_format() {
        let query = make_query(&[
            ("LAYER", "roads"),
            ("TILEMATRIXSET", "EPSG4326"),
            ("TILEMATRIX", "0"),
            ("TILEROW", "0"),
            ("TILECOL", "0"),
        ]);
        let params = GetTileParams::from_query(&query).unwrap();
        assert_eq!(params.style, "default");
        assert_eq!(params.format, DEFAULT_TILE_FORMAT);
        assert_eq!(params.width, 256);
        assert_eq!(params.height, 256);
    }

    #[test]
    fn parse_missing_layer() {
        let query = make_query(&[
            ("TILEMATRIXSET", "EPSG4326"),
            ("TILEMATRIX", "0"),
            ("TILEROW", "0"),
            ("TILECOL", "0"),
        ]);
        let body = err_body(GetTileParams::from_query(&query));
        assert!(body.contains("LAYER"));
        assert!(body.contains("MissingParameterValue"));
    }

    #[test]
    fn parse_missing_matrix_set() {
        let query = make_query(&[
            ("LAYER", "roads"),
            ("TILEMATRIX", "0"),
            ("TILEROW", "0"),
            ("TILECOL", "0"),
        ]);
        let body = err_body(GetTileParams::from_query(&query));
        assert!(body.contains("TILEMATRIXSET"));
    }

    #[test]
    fn parse_missing_matrix() {
        let query = make_query(&[
            ("LAYER", "roads"),
            ("TILEMATRIXSET", "EPSG4326"),
            ("TILEROW", "0"),
            ("TILECOL", "0"),
        ]);
        let body = err_body(GetTileParams::from_query(&query));
        assert!(body.contains("TILEMATRIX"));
    }

    #[test]
    fn parse_missing_row_and_col() {
        let base = [
            ("LAYER", "roads"),
            ("TILEMATRIXSET", "EPSG4326"),
            ("TILEMATRIX", "0"),
        ];
        let mut missing_row = base.to_vec();
        missing_row.push(("TILECOL", "0"));
        let query = make_query(&missing_row);
        let body = err_body(GetTileParams::from_query(&query));
        assert!(body.contains("TILEROW"));

        let mut missing_col = base.to_vec();
        missing_col.push(("TILEROW", "0"));
        let query = make_query(&missing_col);
        let body = err_body(GetTileParams::from_query(&query));
        assert!(body.contains("TILECOL"));
    }

    #[test]
    fn parse_negative_index_rejected() {
        let query = make_query(&[
            ("LAYER", "roads"),
            ("TILEMATRIXSET", "EPSG4326"),
            ("TILEMATRIX", "0"),
            ("TILEROW", "-1"),
            ("TILECOL", "0"),
        ]);
        let body = err_body(GetTileParams::from_query(&query));
        assert!(body.contains("InvalidParameterValue"));
        assert!(body.contains("TILEROW"));
    }

    #[test]
    fn parse_non_numeric_index_rejected() {
        let query = make_query(&[
            ("LAYER", "roads"),
            ("TILEMATRIXSET", "EPSG4326"),
            ("TILEMATRIX", "0"),
            ("TILEROW", "abc"),
            ("TILECOL", "0"),
        ]);
        assert!(GetTileParams::from_query(&query).is_err());
    }

    #[test]
    fn parse_unknown_format_rejected() {
        let query = make_query(&[
            ("LAYER", "roads"),
            ("TILEMATRIXSET", "EPSG4326"),
            ("TILEMATRIX", "0"),
            ("TILEROW", "0"),
            ("TILECOL", "0"),
            ("FORMAT", "image/tiff"),
        ]);
        let body = err_body(GetTileParams::from_query(&query));
        assert!(body.contains("InvalidParameterValue"));
        assert!(body.contains("image/png"));
    }

    #[test]
    fn parse_unknown_matrix_set_rejected() {
        let query = make_query(&[
            ("LAYER", "roads"),
            ("TILEMATRIXSET", "NopeQuad"),
            ("TILEMATRIX", "0"),
            ("TILEROW", "0"),
            ("TILECOL", "0"),
        ]);
        let body = err_body(GetTileParams::from_query(&query));
        assert!(body.contains("TILEMATRIXSET"));
    }

    #[test]
    fn parse_unknown_matrix_rejected() {
        let query = make_query(&[
            ("LAYER", "roads"),
            ("TILEMATRIXSET", "EPSG4326"),
            ("TILEMATRIX", "42"),
            ("TILEROW", "0"),
            ("TILECOL", "0"),
        ]);
        let body = err_body(GetTileParams::from_query(&query));
        assert!(body.contains("TILEMATRIX"));
    }

    #[test]
    fn bounds_of_valid_tile() {
        // EPSG4326 level 1 tile (0,0) is the north-western quarter.
        let params = base_params();
        let (min_x, min_y, max_x, max_y) = params.bounds().unwrap();
        assert!((min_x + 180.0).abs() < 1e-9);
        assert!((max_x + 90.0).abs() < 1e-9);
        assert!((max_y - 90.0).abs() < 1e-9);
        assert!((min_y).abs() < 1e-9);
    }

    #[test]
    fn bounds_of_out_of_range_tile_is_none() {
        let mut params = base_params();
        params.row = 9;
        assert!(params.bounds().is_none());
    }

    #[test]
    fn crs_for_known_and_unknown() {
        assert_eq!(crs_for_tiles("EPSG4326"), "EPSG:4326");
        assert_eq!(crs_for_tiles("WebMercatorQuad"), "EPSG:3857");
        assert_eq!(crs_for_tiles("bogus"), "EPSG:4326");
    }

    #[test]
    fn render_tile_returns_png() {
        let params = base_params();
        let line = LineString::new(vec![Point::new(-170.0, 40.0), Point::new(-10.0, 40.0)]);
        let layers = vec![WmsLayer {
            name: "roads".to_string(),
            features: vec![WmsFeature {
                geometry: Geometry::LineString(line),
                attributes: HashMap::new(),
            }],
            style: LayerStyle::default(),
        }];

        let png = render_tile(&params, &layers).unwrap();
        assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn render_tile_with_no_layers() {
        let params = base_params();
        let png = render_tile(&params, &[]).unwrap();
        assert!(!png.is_empty());
    }

    #[test]
    fn render_tile_out_of_range() {
        let mut params = base_params();
        params.col = 42;
        let err = render_tile(&params, &[]).unwrap_err();
        let body = String::from_utf8_lossy(&err.body);
        assert!(body.contains("InvalidParameterValue"));
    }

    #[test]
    fn handle_get_tile_missing_map() {
        let query = make_query(&[
            ("LAYER", "roads"),
            ("TILEMATRIXSET", "EPSG4326"),
            ("TILEMATRIX", "0"),
            ("TILEROW", "0"),
            ("TILECOL", "0"),
        ]);
        let resp = handle_get_tile(&query).unwrap();
        let body = String::from_utf8_lossy(&resp.body);
        assert!(body.contains("MAP"));
    }

    #[test]
    fn handle_get_tile_returns_png() {
        let query = make_query(&[
            ("MAP", "/tmp/nonexistent.qgs"),
            ("LAYER", "roads"),
            ("TILEMATRIXSET", "EPSG4326"),
            ("TILEMATRIX", "0"),
            ("TILEROW", "0"),
            ("TILECOL", "0"),
        ]);
        let resp = handle_get_tile(&query).unwrap();
        assert_eq!(&resp.body[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }
}