# Feature map: qgis-geometry

## What it does

Geometry primitives replacing GEOS. Points, linestrings, polygons,
multi-types, WKT/WKB I/O, validity checking, and planar operations
(area, length, contains).

## How to verify

```bash
./verify test --crate geometry
./verify clippy --crate geometry
```

## What proves each feature works

| Feature | Evidence |
|---------|----------|
| Point / LineString / Polygon construction | Unit tests in `geometry/src/` |
| WKT parse + emit roundtrip | `wkt.rs` tests |
| WKB parse + emit (incl. EWKB SRID header) | `wkb.rs` tests |
| Area (signed shoelace) | `area.rs` tests |
| Length | `length.rs` tests |
| Contains point (boundary semantics) | `contains.rs` tests |
| is_valid | `validity.rs` tests |
| make_valid (split self-intersections) | `validity.rs` tests |
| Ring orientation (CCW exterior, CW holes) | `validity.rs` tests |
| Dimension handling (2D/Z/M/ZM) | `dimension.rs` tests |

## Oracle coverage

No oracle fixtures yet — geometry is pure computation, not I/O.
Add oracle fixtures when geometry starts processing real files.
