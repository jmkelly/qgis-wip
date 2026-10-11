# qgis-bench

Criterion benchmarks for the QGIS Rust port, ported from the hot paths in
`tests/bench/qgsbench.cpp` (the `QgsBench` render loop and the surrounding
render / index / expression / raster hot paths it exercises).

The real Rust implementations of these algorithms do not exist yet (E1+ in
the epic), so every benchmark drives a **placeholder stub** whose signature
matches the qgsbench hot path it will eventually exercise. The goal is the
[criterion](https://bheisler.github.io/criterion.rs/book/) infrastructure —
groups, naming, and harness wiring — not real algorithm performance.

## Benchmark groups

| Criterion group | Benchmarks | qgsbench hot path |
|---|---|---|
| `render` | `render/polygon_fill`, `render/layer_render`, `render/labeling` | `QgsBench::render` map-render loop (layer render, polygon fill, labeling) |
| `index` | `index/rtree_insert`, `index/rtree_query` | spatial-index (R-tree) build + query |
| `expression` | `expression/parse`, `expression/evaluate` | expression parse + evaluation |
| `raster` | `raster/reproject`, `raster/render` | raster reprojection + render |

## Running

```bash
cargo bench -p qgis-bench
```

Run a single group or benchmark:

```bash
cargo bench -p qgis-bench --bench render
cargo bench -p qgis-bench render/polygon_fill
```

## Layout

- `src/lib.rs` — shared domain stubs (points, extents, polygons, features,
  layers, settings, rasters) plus the four hot-path modules (`render`,
  `index`, `expression`, `raster`). Each stub documents the qgsbench hot path
  it stands in for.
- `benches/*.rs` — one criterion harness per category, wired with
  `criterion_group!` / `criterion_main!`.

## Safety

The crate is `#![forbid(unsafe_code)]` — pure Rust, zero `unsafe`, per
ADR-0001. `cargo geiger` confirms no `unsafe`/FFI in this crate's own code.
