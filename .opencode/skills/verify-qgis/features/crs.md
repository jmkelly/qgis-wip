# Feature map: qgis-crs

## What it does

Coordinate Reference Systems — WKT2:2019 parse/emit, CRS/EPSG facade,
PROJ pipeline parsing and execution, datum ensemble resolution,
towgs84 selection, and grid loading. Uses `proj4rs` for transformation
math and `crs-definitions` for the embedded EPSG dataset.

## How to verify

```bash
./verify test --crate crs
./verify clippy --crate crs
```

## What proves each feature works

| Feature | Evidence |
|---------|----------|
| WKT2:2019 parse | `wkt2.rs` tests |
| WKT2:2019 emit | `wkt2.rs` tests |
| EPSG code lookup | `epsg.rs` tests |
| PROJ pipeline parse | `pipeline.rs` tests |
| PROJ pipeline execute (push/pop/helmert) | `pipeline.rs` tests |
| Datum ensemble resolution | `ensemble.rs` tests |
| towgs84 3-param vs 7-param selection | `towgs84.rs` tests |
| NTV2 grid loading | `grids.rs` tests |
| Camera (world_to_screen, screen_to_world) | `camera.rs` tests |
| Equirectangular project (degrees→radians) | `project.rs` tests |

## Oracle coverage

No oracle fixtures yet. CRS is pure computation — add fixtures when
it starts processing real .prj or WKT files.
