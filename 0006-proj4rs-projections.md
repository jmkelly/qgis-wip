# Use proj4rs for cartographic projections, gap-build the rest

QGIS delegates all coordinate-reference-system work to PROJ: the EPSG dataset, WKT2 parsing, datum
ensembles, grid shifts, `projinfo`/cct coordinate-operation selection, and `ST_Transform`. ADR 0001
forbade FFI to PROJ, which read as "rebuild it in Rust". We are not rebuilding it. The scariest part
of PROJ is not the projection *math* — that is well documented, small, and already ported many times
— but the dataset and transform-selection machinery around it. So we take the math and own the rest.

## Decision

`proj4rs` (3liz, 0.2.x, MIT/Apache-2.0) is the projection backend for `qgis-crs`. It is a pure-Rust
PROJ.4 implementation, transforming point coordinates from one CRS to another, WASM-compatible, and
requiring no `libproj`/`sqlite3`. `crs-definitions` (0.6, CC0) supplies the embedded
EPSG code to proj-string / WKT dataset.

This is a third-party crate, **not** FFI. ADR 0001's rule is "no `unsafe` link to GDAL/GEOS/PROJ/Qt",
and `proj4rs` is memory-safe Rust, so the safety claim holds. The exception is recorded here so the
boundary is explicit rather than discovered later.

## What proj4rs gives us

- `Proj::from_proj_string(...)` plus `proj4rs::transform::transform`, covering the common `+proj`
  family (tmerc, etmerc, utm, merc, lcc, aea, stere, aeqd, longlat, and friends).
- Optional `crs-definitions` feature: EPSG code in, proj-string out.
- Optional `geo-types` feature: works directly on `geo_types` geometry.
- Nadgrids: NTV2 multi-grids and GDAL-generated GeoTIFF multi-grids (native + WASM).

## Measured gap table (bead qgis-uic.6, 2026-10-11)

This table replaces the assumed gaps with measured data from the QGIS source tree
(`src/`, `python/`, `tests/`) and the fixture store (`fixtures/`).

### +proj method coverage

Enumerated from `grep -rohP '\+proj=\w+' src/ python/ tests/`:

| +proj method | Count | proj4rs status | Gap? |
| --- | --- | --- | --- |
| `longlat` | 405 | ✅ `latlong` stub | closed |
| `merc` | 215 | ✅ `merc` | closed |
| `utm` | 138 | ✅ `tmerc`/`etmerc` (zone params) | closed |
| `pipeline` | 121 | ❌ not supported | **open** |
| `unitconvert` | 113 | ❌ not supported | **open** |
| `cart` | 108 | ❌ `geocent` is stub only | **open** |
| `lcc` | 68 | ✅ `lcc` | closed |
| `push` | 54 | ❌ not supported | **open** |
| `pop` | 54 | ❌ not supported | **open** |
| `helmert` | 54 | ❌ not supported | **open** |
| `webmerc` | 53 | ⚠️ via `merc` + `lat_ts=0` | closed (workaround) |
| `sterea` | 29 | ✅ `sterea` | closed |
| `hgridshift` | 25 | ❌ experimental nadgrids only | **open** |
| `axisswap` | 22 | ❌ not supported | **open** |
| `aea` | 20 | ✅ `aea` | closed |
| `somerc` | 19 | ✅ `somerc` | closed |
| `geocent` | 13 | ⚠️ stub only | **open** |
| `tmerc` | 10 | ✅ `tmerc` | closed |
| `ortho` | 9 | ❌ not supported | **open** |
| `vgridshift` | 4 | ❌ not supported | **open** |
| `qsc` | 3 | ❌ not supported | **open** |
| `laea` | 2 | ✅ `laea` | closed |
| `noop` | 1 | ❌ not supported | **open** |

**Summary**: 10 of 22 methods closed (45%), 1 open with workaround (webmerc), 11 open.
The open methods fall into three categories:
1. **Pipeline steps** (`pipeline`, `unitconvert`, `cart`, `push`, `pop`, `helmert`, `axisswap`, `noop`) — 497 occurrences. These are PROJ pipeline operations that proj4rs does not implement.
2. **Grid shifts** (`hgridshift`, `vgridshift`) — 29 occurrences. proj4rs has experimental NTV2/GeoTIFF nadgrids but not the general grid-shift mechanism.
3. **Missing projections** (`ortho`, `qsc`, `geocent` stub) — 25 occurrences. These are individual projection methods not yet ported.

### crs-definitions coverage

Fixture store EPSG codes (from `fixtures/`):

| EPSG code | Fixture occurrences | crs-definitions 0.6.0 | Status |
| --- | --- | --- | --- |
| EPSG:4326 (WGS 84) | 51 | ✅ `EPSG_4326` constant | closed |
| EPSG:7030 (WGS 84 ellipsoid) | 10 | ✅ in dataset | closed |
| EPSG:3857 (WGS 84 / Pseudo-Mercator) | 2 | ✅ `EPSG_3857` constant | closed |

All fixture CRS codes resolve through crs-definitions. The crate provides `from_code()`,
`from_code_const::<N>()`, and `Def { proj4, wkt }` for each code. Coverage is complete for the
fixture store. The broader QGIS test suite references ~200 additional EPSG codes (e.g., EPSG:2154,
EPSG:27700, EPSG:3111, EPSG:32733) — all within the crs-definitions range (EPSG:2000–9019+).

### WKT2:2019 parser scope assessment

QGIS emits WKT2:2019 as its preferred CRS serialization format (`Qgis::CrsWktVariant::Preferred = Wkt2_2019`).
Measured from source and fixtures:

- **WKT2 node types in QGIS output**: `GEOGCRS`, `PROJCRS`, `GEODCRS`, `VERTCRS`, `COMPD_CS`, `DATUM`,
  `ELLIPSOID`, `ENSEMBLE`, `PRIMEM`, `CS`, `AXIS`, `CONVERSION`, `METHOD`, `PARAMETER`, `UNIT`,
  `ANGLEUNIT`, `LENGTHUNIT`, `SCALEUNIT`, `ID`, `BASEGEOGCRS`.
- **Fixture WKT2 strings**: The fixture store uses WKT1 (`.prj` files) and proj strings (`.qgs` files),
  not WKT2. However, QGIS's `toWkt(Preferred)` always emits WKT2:2019, so the oracle will need to
  parse and emit WKT2:2019 for CRS comparison.
- **Complexity**: A typical `GEOGCRS` string is 300–800 chars with 8–12 nested nodes. A `PROJCRS` adds
  `CONVERSION` + `METHOD` + 5–10 `PARAMETER` nodes. Full WKT2:2019 parse+emit is a **~2–3K SLoC**
  parser/serializer project.
- **Tractability**: A hand-written recursive-descent parser for the subset QGIS emits is tractable
  (~1.5K SLoC for parse, ~1K SLoC for emit). The full ISO 19162:2019 grammar is not needed — QGIS
  emits a constrained subset. However, this is still a significant build, not a spike.

**Verdict**: Hand-written WKT2:2019 parser/serializer is tractable but sized at ~2–3K SLoC.
This is a gap build, not a spike.

### No-3D/4D limitation impact

QGIS uses 3D/4D CRS and transformations extensively:

- `+proj=geocent` (13 occurrences) — geocentric 3D CRS
- `+proj=cart` (108 occurrences) — Cartesian conversion in pipelines
- `+proj=push`/`+proj=pop` (54 each) — 3D push/pop in pipelines
- `QgsCoordinateTransform::transform3D()` — 3D point transformation
- `QgsCoordinateTransform::transformBox3D()` — 3D bounding box transformation
- Compound CRS (horizontal + vertical) via `QgsProjUtils::createCompoundCrs()`
- Vertical CRS with `+proj=vgridshift`

proj4rs's `geocent` is a **stub** (not a full implementation). The crate transforms 2D `(x, y)` or
3D `(x, y, z)` points but does not handle geocentric datum shifts, push/pop, or compound CRS
natively.

**Impact**: 3D and compound CRS transformations are **not supported** by proj4rs. This affects:
- Geocentric CRS (EPSG:4978, EPSG:4979)
- Compound CRS (e.g., EPSG:4326+5703)
- Any transformation involving a vertical datum shift
- Bounding box transformations that cross ellipsoidal/geocentric boundaries

This is a **hard gap** for E2 CRS parity. The fixture store does not include 3D/compound CRS fixtures,
so the oracle will not catch this until E9 hardening.

## Gaps we accept and own

These are the E2 acceptance boundary. Re-check every one at E9 hardening (`qgis-11m.3`).

| Gap | Consequence | Size | Owner |
| --- | --- | --- | --- |
| No PROJ pipeline support (`pipeline`, `unitconvert`, `cart`, `push`, `pop`, `helmert`, `axisswap`, `noop`) — 497 occurrences | Multi-step datum shifts, 3D transforms, axis swaps, and unit conversions fail | ~3K SLoC | gap build (qgis-qxu.1) |
| No 3D/4D transformation — `geocent` is stub, no push/pop/cart/helmert | Geocentric CRS, compound CRS, vertical datum shifts | ~2K SLoC | gap build (qgis-qxu.3) |
| No WKT2:2019 parse/emit — `proj4wkt-rs` not on crates.io | CRS serialization/deserialization for oracle | ~2.5K SLoC | gap build (qgis-qxu.1) |
| No general grid shifts — `hgridshift`/`vgridshift` unsupported (experimental NTV2/GeoTIFF only) | NADCON (.gsb), GTX, and other grid formats | ~1.5K SLoC | gap build (qgis-qxu.3) |
| Missing projections: `ortho`, `qsc`, `noop` — 13 occurrences | Orthographic, Quadrilateralized Spherical Cube | ~500 SLoC | gap build (qgis-qxu.2) |
| Not PROJ — no coordinate-operation pipelines | `projinfo`/cct parity and PROJ's automatic transform selection | **resolved (b)**: degraded automatic selection via first-match heuristic; parity claim scoped to proj4rs-supported transforms only | qgis-qxu.4 |
| Radians are the native angular unit | Convert at the crate boundary; degrees everywhere in `qgis-crs`'s public API | us | qgis-qxu.2 |
| Upstream aims at proj4js parity, "not a replacement of PROJ" | We cannot assume upstream closes our gaps; track releases | us | track |
| **DEFERRED** — GSB NADCON grids | `hgridshift` with .gsb files fails; NADCON datum shifts unsupported | — | qgis-qxu.3.3.1 |
| **DEFERRED** — GTX vertical grids | `vgridshift` with .gtx files fails; vertical datum shifts unsupported | — | qgis-qxu.3.3.2 |
| **DEFERRED** — Velocity models | PROJ velocity model support (`+model=`) not available | — | qgis-qxu.3.3.3 |
| **DEFERRED** — Compound CRS (horizontal + vertical) | COMPD_CS transforms fail; vertical + horizontal combination unsupported | — | qgis-qxu.3.3.4 |

## projinfo/cct parity decision (bead qgis-qxu.4, 2026-10-11)

**Decision**: Option (b) — accept degraded automatic selection via first-match heuristic.
Scope the parity claim to proj4rs-supported transforms only.

**Rationale**: Building a minimal operation-pipeline evaluator (option a) would require
reimplementing PROJ's coordinate-operation selection logic — the exact machinery ADR 0006
was written to avoid. The facade approach means we accept that PROJ's automatic transform
selection (which considers accuracy, area of use, and grid availability) cannot be
reproduced by delegation to proj4rs.

**Heuristic**: When QGIS requests a transform between two CRS pairs:
1. Look up both CRS definitions via crs-definitions.
2. If both have proj4 strings, use proj4rs directly (first-match).
3. If a pipeline is required (datum shift, unit conversion, axis swap), attempt
   step-by-step composition using proj4rs-supported operations.
4. If composition fails, return a clear error naming the unsupported operation.

**Parity claim scope**: We claim parity only for CRS pairs where proj4rs supports
all required operations. For pairs requiring pipeline operations proj4rs does not
support (helmert, cart, push/pop, axisswap), we return an explicit error rather
than a silently wrong result. This caps E9 parity at proj4rs's coverage boundary.

**Tests**: At least 10 real-world CRS pairs covering:
- EPSG:4326 → EPSG:3857 (WGS 84 to Web Mercator, no pipeline needed)
- EPSG:4326 → EPSG:2154 (WGS 84 to Lambert-93, datum shift)
- EPSG:4326 → EPSG:27700 (WGS 84 to British National Grid, datum shift)
- EPSG:4326 → EPSG:3111 (WGS 84 to GDA94 / MGA zone 55, UTM)
- EPSG:4326 → EPSG:32733 (WGS 84 to WGS 84 / UTM zone 33S, UTM)
- EPSG:4326 → EPSG:4978 (WGS 84 to WGS 84 geocentric, geocent stub)
- EPSG:4326 → EPSG:4979 (WGS 84 to WGS 84 3D, geocent stub)
- EPSG:4326+5703 → EPSG:4326 (compound CRS, vertical)
- EPSG:4326 → EPSG:3035 (WGS 84 to ETRS89-extended / LAEA Europe)
- EPSG:4326 → EPSG:3857 via EPSG:4269 (NAD83 intermediate, datum shift)

## Considered Options

- Hand-build the projection math — rejected. ~9K SLoC of heavily trodden mathematical ground, and
  E2 was the scariest epic in the plan for exactly this reason.
- FFI to `libproj` — rejected by ADR 0001; voids the safety claim.
- `geodesy` — rejected. Pure Rust, but a different transformation model and API, far smaller
  adoption, and it does not consume `+proj` strings, so every QGIS-side proj string needs rewriting.
- `crs-definitions` alone — insufficient; it is a dataset with no transformation engine.
- `proj4wkt-rs` + `proj4rs` — attractive for WKT, but `proj4wkt-rs` has no crates.io release, so it
  would be a git dependency. Write the WKT2 parser ourselves instead.

## Consequences

E2 (`qgis-qxu`) shrinks from "rebuild PROJ" to "integrate proj4rs, then gap-build the table above".
`qgis-crs` becomes a thin, honest facade: proj strings and EPSG codes in, `geo_types` out, degrees at
the boundary. Licences are compatible with our GPL-3.0-or-later: MIT/Apache-2.0 for proj4rs, CC0 for
crs-definitions. Both are pinned exact in `Cargo.toml` `[workspace.dependencies]` so parallel
implementers do not churn `Cargo.lock`.

The E0.5 GUI POC's `Equirectangular` is unrelated to this: it maps degrees straight onto the render
surface as an identity transform, which is not a cartographic projection and does not need proj4rs.

**Status**: accepted (updated 2026-10-11 with measured gap table from bead qgis-uic.6)

**Supersedes**: ADR 0001 (narrowly — permits one audited pure-Rust third-party crate for projection
math; the no-FFI safety rule is unchanged)
