# Crate Map

Concise map of every workspace member: directory → package → purpose, key
modules, and how to verify it. Start here instead of `ls` / `cat Cargo.toml` /
`find -name '*.rs'` discovery.

Feature docs live in `.opencode/skills/verify-qgis/features/` (only some
crates have one — marked below). Generic gate for the rest:
`./verify test --crate <dir>` + `./verify clippy --crate <dir>`.

| Dir | Package | Purpose (replaces) | Key modules | Verify |
|---|---|---|---|---|
| `geometry/` | `qgis-geometry` | Geometry primitives (GEOS) | `point`, `linestring`, `polygon`, `multi`, `collection`, `wkt`, `wkt2`, `wkb`, `predicates`, `overlay`, `buffer`, `simplify`, `validity`, `geo_bridge` | `./verify test --crate geometry` · [features/geometry.md](.opencode/skills/verify-qgis/features/geometry.md) |
| `crs/` | `qgis-crs` | Coordinate reference systems (PROJ) | `facade` (`CrsTransform`), `wkt2`, `pipeline`, `pipeline_exec`, `projection`, `ensemble`, `towgs84`, `grids`, `st` | `./verify test --crate crs` · [features/crs.md](.opencode/skills/verify-qgis/features/crs.md) |
| `expression/` | `qgis-expression` | Expression engine (Qwt-expression) | `lexer`, `parser`, `ast`, `eval`, `errors`, `functions/`, `regex` | `./verify test --crate expression` · [features/expression.md](.opencode/skills/verify-qgis/features/expression.md) |
| `providers/` | `qgis-providers` | Vector data providers (GDAL/SpatiaLite) | `geojson`, `gpkg`, `shapefile`, `dbase`, `vrt`, `vsi`, `driver_manager` | `./verify test --crate providers` · [features/providers.md](.opencode/skills/verify-qgis/features/providers.md) |
| `core/` | `qgis-core` | Layer + Feature model (`Feature`, `Value`) | single `lib.rs` (`Feature`, `Value`) | `./verify test --crate core` |
| `spatial-index/` | `qgis-spatial-index` | STRtree spatial index (libspatialindex) | `bbox` (`BoundingBox`), `lib.rs` (tree) | `./verify test --crate spatial-index` |
| `style/` | `qgis-style` | QML style engine (`Color`, `Symbol`) | single `lib.rs` (QML parse, symbols) | `./verify test --crate style` |
| `raster/` | `qgis-raster` | Raster types (GDAL raster) | `block` (`RasterBlock`), `layer` (`RasterLayer`), `statistics` | `./verify test --crate raster` |
| `mesh/` | `qgis-mesh` | Mesh types (MDAL) | `layer`, `dataset`, `vertex`, `edge`, `face`, `frame` | `./verify test --crate mesh` |
| `pointcloud/` | `qgis-pointcloud` | Point-cloud types (PDAL) | `layer`, `las`, `block`, `attribute`, `octree` | `./verify test --crate pointcloud` |
| `analysis/` | `qgis-analysis` | Spatial analysis ops (stub at E0) | single `lib.rs` (`euclidean_distance`) | `./verify test --crate analysis` |
| `render/` | `qgis-render` | 2D raster rendering for WMS GetMap | single `lib.rs` (`Pixel`, canvas, PNG) | `./verify test --crate render` |
| `server/` | `qgis-server` | OGC server: WMS/WCS/WMTS + FCGI/HTTP | `wms`, `wcs`, `wmts`, `service`, `config`, `config_cache`, `http_server`, `fcgi`, `fcgi_listener`, `auth`, `query`, `landing` | `./verify test --crate server` |
| `gui/` | `qgis-gui` | Desktop GUI, built last per ADR-0002 (iced+wgpu) | `attribute_table`, `print_layout` | `./verify test --crate gui` |
| `pyo3/` | `qgis-pyo3` | Python bindings (SIP/PyQt6) | `processing`, `gui`, `three_d`, `plugin_paths` | `./verify test --crate pyo3` |
| `oracle/` | `qgis-oracle` | C++/Rust diff oracle + golden store | `backend`, `cpp_backend`, `rust_backend`, `diff`, `fixture`, `goldens`, `cli`, `output` | `./verify oracle` · [features/oracle.md](.opencode/skills/verify-qgis/features/oracle.md) |
| `fixtures/` | `qgis-fixtures` | Fixture store catalog for the oracle | `parser`, `lib.rs` (catalog) | `./verify test --crate fixtures` |
| `qgis-bench/` | `qgis-bench` | Criterion benches porting qgsbench | `expression`, `index`, `raster`, `render` | `./verify bench` · [features/bench.md](.opencode/skills/verify-qgis/features/bench.md) |
| `validate/` | `qgis-validate` | Agent-runnable validation harness + report format (E16) | `lib.rs` (`Report`, `Runner`, `Outcome`, `Status`), `cli`, `suites/` | `./verify test --crate validate` · `./verify agent selfcheck` |

## Notes

- Dir name and package name differ by the `qgis-` prefix: `./verify test
  --crate <dir>` accepts either (e.g. `geometry` or `qgis-geometry`).
- `expressions/` at root is empty/reserved — the real crate is `expression/`.
- Cross-crate or golden changes: escalate to `./verify all` / `./verify oracle`.
- Never run raw `cargo` — cargo is not on the agent PATH; always use `./verify`.
