# Use rusqlite for the GeoPackage reader, with a recorded FFI exception

QGIS reads GeoPackage (`.gpkg`) through GDAL/SpatiaLite on top of SQLite.
GeoPackage is the OGC SQLite container standard: the geometry, tile-matrix,
and metadata tables all live inside a plain SQLite database file, so any
GeoPackage reader must first be a SQLite reader. ADR 0001 forbade FFI to
GDAL/GEOS/PROJ/SpatiaLite/Qt, which read strictly would mean reimplementing
SQLite itself in Rust. We are not reimplementing SQLite.

## Decision

`rusqlite` (`=0.32.1`, MIT, via `[workspace.dependencies]`, referenced from
`qgis-providers` with `rusqlite = { workspace = true }`) is the SQLite
backend for the GeoPackage reader (`providers/src/gpkg.rs`). Default
features are used: no `bundled` build, so `libsqlite3-sys` (`0.30.1`, MIT)
links the **system SQLite** (`pkg-config`/`vcpkg` probe) instead of compiling
the vendored C amalgamation.

This is a third-party crate with a **C FFI boundary**, not pure Rust. ADR
0001's rule is "no `unsafe` link to GDAL/GEOS/PROJ/Qt", and this links
`libsqlite3` — a far narrower, stable, single-library surface with no
geospatial logic in it. The safety claim therefore narrows explicitly: memory
safety of the GeoPackage reader rests on `rusqlite`'s safe wrapper plus the
maturity of system SQLite, not on pure Rust. The exception is recorded here
so the boundary is explicit rather than discovered later via `cargo geiger`.

## Policy check (ADR 0001)

- **Pinned exact**: `rusqlite = "=0.32.1"` in the workspace `[workspace.dependencies]`
  table, per the exact-pin policy; `Cargo.lock` resolves `libsqlite3-sys 0.30.1`.
- **GPL-compatible**: `rusqlite` and `libsqlite3-sys` are both MIT, compatible
  with our `GPL-3.0-or-later`. The SQLite library itself is public domain
  (SQLite blessing), so there is no copyleft conflict at the link boundary.
- **Pure Rust**: **fails** — this is the recorded exception. `cargo geiger`
  will flag `libsqlite3-sys` (and transitively `qgis-providers`) as containing
  `unsafe` FFI bindings. That flag is expected and accepted for this crate
  pair only; it must not be used as precedent for GDAL/GEOS/PROJ/Qt bindings.

## Considered Options

- Hand-build a pure-Rust SQLite engine — rejected. A correct SQLite file-format
  reader plus SQL planner is a multi-year project dwarfing the GeoPackage work
  it would serve; unlike projection math, it has no small documented core.
- `rusqlite` with the `bundled` feature — rejected. Compiling the vendored C
  amalgamation reintroduces a C build into every crate compile, forfeits
  distro security patching of SQLite, and widens the C surface from "system
  library we call" to "C code we ship".
- FFI to GDAL/SpatiaLite for GeoPackage — rejected by ADR 0001; voids the
  safety claim and drags the full GDAL dependency tree back in.
- Pure-Rust SQLite crates (e.g. `sqlite` wrappers still need C; `rusqlite`
  alternatives like `sqlx-sqlite` share the same `libsqlite3-sys` boundary) —
  no option removes the FFI; `rusqlite` is the most adopted, best audited one.

## Consequences

- `qgis-providers` gains one `unsafe` dependency subtree (`rusqlite` ->
  `libsqlite3-sys` -> system `libsqlite3`). `cargo geiger` output for the
  workspace must be read with this exception in mind.
- Platform requirement: a system SQLite development library must be present
  at build time (`pkg-config`-findable `sqlite3`). CI images and contributor
  docs must state this; failure mode is a link-time error, not a silent
  fallback.
- No `unsafe` code of ours: all SQLite access goes through `rusqlite`'s safe
  API (`Connection`, prepared statements, `params!`). Direct `libsqlite3-sys`
  use from our code is forbidden; review should reject any such import.
- Revisit trigger: if a mature pure-Rust SQLite implementation reaches
  `rusqlite` parity for read paths (SQLite file-format reads, prepared
  statements, RTree/geopackage extensions), re-evaluate and supersede this ADR.

**Status**: accepted

**Supersedes**: ADR 0001 (narrowly — permits one audited FFI-to-system-SQLite
exception for the GeoPackage reader; the no-GDAL/GEOS/PROJ/Qt-FFI rule and the
pure-Rust default for all other new crates are unchanged)
