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

## Headless acceptance

The headless driver ([`src/headless.rs`](src/headless.rs)) wraps `AppState` and
feeds `Message` values through the real `app::update` path with no iced
runtime, window, or GPU, so GUI workflows can be scripted and asserted without
a display.

`examples/headless_acceptance.rs` ports the manual acceptance flow (load a
shapefile, style it, save/reopen a `.qgs`, drive the viewport) into nine
scenarios; `examples/headless_scenario.rs` demonstrates the fluent `Scenario`
builder.

```bash
./verify headless   # build examples, run acceptance scenarios, run harness tests
```

This is the `headless` job in CI (`rust-port.yml`) and the sixth gate in
`ci.sh`; it needs no display (no xvfb).
