//! GUI for the QGIS Rust port.
//!
//! Uses `iced` with shared `wgpu` map/3D viewports, built last after headless
//! core stabilizes. See ADR-0002 for the GUI toolkit decision.

#![forbid(unsafe_code)]

pub mod attribute_table;
pub mod layer_renderer;
pub mod map_canvas;
pub mod print_layout;
pub mod style_apply;

pub use layer_renderer::{
    apply_opacity, build_spatial_index, camera_for_extent, cull_visible_features, draw_layer,
    load_shapefile_as_layer, paint_layer, render_layer, render_layer_paths,
    render_layer_with_index, viewport_bounding_box, visible_feature_indices,
    visible_feature_indices_with_index, DefaultLayerRenderer, LayerRenderer, LineLayerRenderer,
    LineRenderer, PointLayerRenderer, PointRenderer, PolygonLayerRenderer, PolygonRenderer,
    VectorLayerRenderer,
};

/// Application state for the main window.
#[derive(Debug, Default)]
pub struct AppState {
    // Placeholder: will hold map layers, tool state, etc.
}

impl AppState {
    /// Creates a new, empty application state.
    pub fn new() -> Self {
        Self {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_is_default() {
        let state = AppState::new();
        assert_eq!(format!("{:?}", state), "AppState");
    }
}
