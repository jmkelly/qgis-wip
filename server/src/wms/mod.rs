//! WMS (Web Map Service) request handlers.
//!
//! Implements WMS GetMap (render layers to image) and GetFeatureInfo
//! (identify features at a point) per OGC WMS 1.3.0 specification.
//!
//! Reference: `src/server/services/qgswmsgetmap.cpp`,
//! `src/server/services/qgswmsgetfeatureinfo.cpp`

pub mod geojson;
pub mod get_feature_info;
pub mod get_map;
pub mod project;

pub use get_feature_info::{
    FeatureInfo, GetFeatureInfoParams, LINE_TOLERANCE, MAX_FEATURES_PER_LAYER,
    DEFAULT_INFO_FORMAT, identify_features, handle_get_feature_info,
};
pub use get_map::{
    GetMapParams, WmsFeature, WmsLayer, LayerStyle, DEFAULT_FORMAT, MAX_DIMENSION, MIN_DIMENSION,
    handle_get_map, render_map,
};
