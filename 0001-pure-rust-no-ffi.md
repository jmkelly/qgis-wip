# Pure Rust, no geospatial / Qt FFI

QGIS depends on GDAL, GEOS, PROJ, SpatiaLite, PDAL, MDAL, and Qt6; we decided to rebuild missing capability in pure Rust instead of binding via FFI, because the goal is memory safety with no compromise.

Considered Options: audited-FFI strangler vs pure rebuild. Rejected FFI because any `unsafe` link to those libs voids the safety claim and keeps C++ crash/CVE surface.

Consequences: only `geo` covers part of GEOS; GDAL/PROJ/SpatiaLite/PDAL/MDAL/Qwt-expression need new builds. This is the dominant cost driver.

Exception (see [ADR 0006](0006-proj4rs-projections.md)): a pure-Rust, memory-safe third-party *crate* is not FFI and is allowed where it removes a large rebuild risk. `proj4rs` supplies PROJ's cartographic projection math and `crs-definitions` supplies the EPSG dataset; the WKT2, datum-ensemble, grid and `projinfo`/cct gaps around them are still ours to build. Add new crate dependencies deliberately — they must stay pure Rust (verify with `cargo geiger`) and be pinned exact in `rust/Cargo.toml`.
