# qgis-gui

GUI crate for the QGIS Rust port: `iced` + `wgpu`, built last after the
headless core stabilizes (ADR-0002). The E0.5 rendering proof of concept
(ADR-0005) is the exception that lands first.

## Map canvas POC

`examples/map_canvas_poc.rs` (bead qgis-rez.4) is the GUI rendering proof of
concept. It answers the most urgent technical risk in the port: can the
retained iced+wgpu stack render a map canvas at interactive frame rates?

The example renders a procedural 100x100 grid of diamond cells — 10,000
polygons, no external data — on an iced `Canvas` widget, with:

- **Mouse-drag panning** — click and drag to move the map; the map sticks to
  the cursor at every zoom level.
- **Scroll-wheel zoom** — scroll up to zoom in, down to zoom out, centered on
  the cursor position.
- **Frame-rate counter** — FPS overlaid on the canvas, recomputed every second
  from actual rendered frames.
- **Viewport culling** — only cells intersecting the visible world rectangle
  become geometry, so pan/zoom stay cheap.

The viewport transform is managed by `qgis_crs::Camera`: longitude/latitude
(degrees) is projected to world radians with `equirectangular_project`, then
mapped to screen pixels by the camera (see `crs`).

### Running

```bash
cargo run -p qgis-gui --example map_canvas_poc
```

A 1024x768 window opens. Drag with the left mouse button to pan, scroll to
zoom. The HUD in the top-left corner shows live FPS, zoom level, and map
center.

### Pure-Rust boundary

The crate is `#![forbid(unsafe_code)]`; the only `unsafe` in the workspace is
the pyo3-ffi boundary in `qgis-pyo3` (ADR-0001/0003).
