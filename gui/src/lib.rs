//! GUI for the QGIS Rust port.
//!
//! Uses `iced` with shared `wgpu` map/3D viewports, built last after headless
//! core stabilizes. See ADR-0002 for the GUI toolkit decision.
//!
//! The E14 application binary lives in [`app`] and [`main`]; the rendering
//! pipeline is in [`layer_renderer`] and [`map_canvas`]; the map canvas
//! program (multi-layer pan/zoom) is in [`map_program`].

#![forbid(unsafe_code)]

pub mod app;
pub mod attribute_table;
pub mod dialogs;
pub mod layer_panel;
pub mod layer_renderer;
pub mod map_canvas;
pub mod map_program;
pub mod print_layout;
pub mod project;
pub mod style_apply;
pub mod style_dialog;

pub use layer_renderer::{
    apply_opacity, build_spatial_index, camera_for_extent, cull_visible_features, draw_layer,
    load_shapefile_as_layer, paint_layer, render_layer, render_layer_paths,
    render_layer_with_index, viewport_bounding_box, visible_feature_indices,
    visible_feature_indices_with_index, DefaultLayerRenderer, LayerRenderer, LineLayerRenderer,
    LineRenderer, PointLayerRenderer, PointRenderer, PolygonLayerRenderer, PolygonRenderer,
    VectorLayerRenderer,
};
