//! Render hot paths, mirroring `QgsBench::render` in `tests/bench/qgsbench.cpp`.
//!
//! The qgsbench render loop iterates `QgsMapRendererSequentialJob` /
//! `QgsMapRendererParallelJob` over the project layers, collecting
//! user/sys/total/wall timings per iteration. These stubs stand in for the
//! per-pass hot paths that loop drives: polygon fill, per-layer render, and
//! labeling.

use crate::{Layer, MapSettings, RenderStats};

/// Stub for the polygon-fill hot path: rasterize every polygon in `layers`
/// into a `width` x `height` canvas. Eventually backed by the Rust renderer.
pub fn render_polygon_fill(layers: &[Layer], width: u32, height: u32) -> RenderStats {
    let mut pixels = 0u64;
    for layer in layers {
        for feature in &layer.features {
            pixels += u64::from(width) * u64::from(height) * feature.geometry.vertex_count() as u64;
        }
    }
    RenderStats {
        wall_ns: pixels,
        pixels,
    }
}

/// Stub for the per-layer render hot path: render each layer under `settings`.
pub fn render_layer(layers: &[Layer], settings: &MapSettings) -> RenderStats {
    let mut pixels = 0u64;
    for layer in layers {
        for feature in &layer.features {
            pixels += feature.geometry.vertex_count() as u64;
        }
        pixels *= u64::from(settings.width) * u64::from(settings.height);
    }
    RenderStats {
        wall_ns: pixels,
        pixels,
    }
}

/// Stub for the labeling hot path: place labels for every feature.
pub fn render_labeling(layers: &[Layer], settings: &MapSettings) -> RenderStats {
    let mut pixels = 0u64;
    for layer in layers {
        pixels += layer.features.len() as u64 * u64::from(settings.width);
    }
    RenderStats {
        wall_ns: pixels,
        pixels,
    }
}
