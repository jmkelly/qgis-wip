# QGIS Rust Workspace

Pure-Rust reimplementation of QGIS 4.x. No FFI to GDAL, GEOS, PROJ, Qt, or any C/C++ library.

## Workspace layout

| Crate | Package name | Purpose |
|-------|-------------|---------|
| `geometry` | `qgis-geometry` | Geometry primitives (replacing GEOS) |
| `crs` | `qgis-crs` | Coordinate Reference Systems — projections via `proj4rs`, CRS/EPSG/WKT2 facade |
| `expression` | `qgis-expression` | Expression engine (replacing Qwt-expression) |
| `providers` | `qgis-providers` | Data providers (replacing GDAL/SpatiaLite/PDAL/MDAL) |
| `analysis` | `qgis-analysis` | Analysis tools |
| `server` | `qgis-server` | Server (WMS/WFS/etc.) |
| `pyo3` | `qgis-pyo3` | PyO3 Python bindings (replacing SIP/PyQt6) |
| `gui` | `qgis-gui` | GUI (iced + wgpu, built last) |
| `oracle` | `qgis-oracle` | C++/Rust diff oracle harness |
| `fixtures` | `qgis-fixtures` | Shared test fixtures |
| `qgis-bench` | `qgis-bench` | Criterion benchmarks |

## Build order

Following ADR-0004: core-geometry/expression/crs -> providers -> analysis -> server -> gui

For `crs`, "core" means the `proj4rs` facade plus the gap builds, not the projection math itself —
see ADR-0006.

## Policy

- Pure Rust, no FFI. Use `cargo geiger` to verify no unsafe/FFI dependencies.
- **Cartographic projections are not rebuilt from scratch.** `qgis-crs` uses the pure-Rust crate
  [`proj4rs`](https://crates.io/crates/proj4rs) for `+proj` transformation math and
  [`crs-definitions`](https://crates.io/crates/crs-definitions) for the embedded EPSG dataset, per
  [ADR 0006](0006-proj4rs-projections.md). The gaps around them — WKT2:2019 parse/emit,
  datum ensembles, GSB/GTX grids, `projinfo`/cct parity — are ours to build. New third-party crate
  dependencies must be pure Rust, GPL-compatible, and pinned exact in `Cargo.toml`.
- Each crate must run `cargo test` in under 5 minutes.
- Crates are small and focused, mapping one-to-one with QGIS modules.

## Quick start

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo geiger --workspace  # verify no FFI
```

## CI

All quality gates are automated in CI (`rust-port.yml`) as six parallel jobs:
`test`, `clippy`, `bench-compile`, `oracle`, `geiger`, and `headless`. They can
be run locally with a single script:

```bash
./ci.sh              # run all gates (test, clippy, bench compile, oracle, headless)
./ci.sh --geiger     # also run cargo-geiger
./verify headless    # just the headless GUI acceptance scenarios + harness tests
```

The `headless` gate builds the qgis-gui examples, runs the headless acceptance
scenarios, and runs the headless harness unit tests. It drives the real
`app::update` path with no iced runtime, window, or GPU, so it needs no display
(no xvfb) and finishes in milliseconds.

See [rust-port-agent-loop.md](rust-port-agent-loop.md) for the
full unattended agent loop runbook.
