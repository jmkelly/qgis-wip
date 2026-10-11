//! Raster hot paths.
//!
//! QGIS raster rendering reprojects and resamples raster bands onto the map
//! canvas. These stubs stand in for the reprojection and render hot paths.

use crate::{Crs, Extent, MapSettings, RenderStats};

/// A single-band raster stub.
#[derive(Debug, Clone, PartialEq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    /// Cell values in row-major order.
    pub cells: Vec<f64>,
    pub crs: Crs,
    pub extent: Extent,
}

impl Raster {
    /// Creates a new raster filled with zeros.
    pub fn new(width: u32, height: u32, crs: Crs, extent: Extent) -> Self {
        let cells = vec![0.0; width as usize * height as usize];
        Self {
            width,
            height,
            cells,
            crs,
            extent,
        }
    }
}

/// Stub for the raster reprojection hot path: resample `src` onto `dst_crs`
/// over `extent`. Eventually backed by the Rust raster reprojection.
pub fn raster_reproject(src: &Raster, dst_crs: &Crs, extent: &Extent) -> Raster {
    let mut cells = src.cells.clone();
    // Deterministic stand-in so the harness measures real work.
    for (i, cell) in cells.iter_mut().enumerate() {
        *cell += (i % 7) as f64;
    }
    Raster {
        width: src.width,
        height: src.height,
        cells,
        crs: Crs::new(dst_crs.auth_id.clone()),
        extent: *extent,
    }
}

/// Stub for the raster render hot path: render `raster` under `settings`.
pub fn raster_render(raster: &Raster, settings: &MapSettings) -> RenderStats {
    let pixels = raster
        .cells
        .iter()
        .map(|c| c.to_bits())
        .sum::<u64>()
        .rem_euclid(u64::from(settings.width) * u64::from(settings.height) + 1);
    RenderStats {
        wall_ns: pixels,
        pixels,
    }
}
