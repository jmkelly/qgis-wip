//! WMTS (Web Map Tile Service) request handlers.
//!
//! Implements WMTS 1.0.0 GetCapabilities, GetTile, and GetFeatureInfo,
//! following the OGC tiling model described in `tile_matrix`.
//!
//! Reference: OGC 07-057r7, `src/server/services/qgswmts*.cpp`

pub mod get_capabilities;
pub mod get_feature_info;
pub mod get_tile;
pub mod tile_matrix;

pub use get_capabilities::{capabilities_document, handle_get_capabilities};
pub use get_feature_info::{
    GetFeatureInfoParams, WMTS_DEFAULT_INFO_FORMAT, WMTS_INFO_FORMATS, handle_get_feature_info,
};
pub use get_tile::{
    DEFAULT_TILE_FORMAT, GetTileParams, WMTS_TILE_FORMATS, crs_for_tiles, handle_get_tile,
    render_tile,
};
pub use tile_matrix::{TileMatrix, TileMatrixSet, tile_bounds, tile_matrix_set};