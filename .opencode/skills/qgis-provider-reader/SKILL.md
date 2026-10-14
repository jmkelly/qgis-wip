---
name: qgis-provider-reader
description: Checklist and canonical structure for implementing a spatial data provider reader in the qgis-providers crate. Use when adding or updating a file/database provider reader (e.g., GeoJSON, Shapefile, GeoPackage, PostGIS, SQL Server, GeoParquet, FlatGeobuf, KML, GML, Spatialite, CSV).
agent: opencode
---

# qgis-provider-reader — Provider Reader Implementation Skill

Use this checklist whenever implementing or extending a vector/feature provider reader in `providers/` (`qgis-providers`). Follow the shared types and trait in `providers/src/lib.rs` instead of re-deriving custom feature collections.

## Shared Types (`providers/src/lib.rs`)

All providers map their format-specific records into the shared crate-level types:

```rust
pub use qgis_geometry::Geometry as CommonGeometry;

#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub ty: String, // "Feature"
    pub geometry: Option<qgis_geometry::Geometry>,
    pub properties: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeatureCollection {
    pub ty: String, // "FeatureCollection"
    pub features: Vec<Feature>,
}

pub trait FeatureProvider {
    fn read_to_features(&self, path: &str) -> Result<FeatureCollection, String>;
}
```

## Implementation Checklist

1. **Reuse `crate::Feature`, `crate::FeatureCollection`, and `crate::FeatureProvider`**
   - Import `crate::{Feature, FeatureCollection, FeatureProvider}` in `providers/src/<format>.rs` and register `pub mod <format>;` in `providers/src/lib.rs`.
   - Do not invent a divergent top-level `FeatureCollection` type for GUI/consumer handoff; if the format parser has low-level record structs, convert them into `crate::Feature` and `crate::FeatureCollection`.

2. **Map format records to `qgis_geometry::Geometry` and JSON properties**
   - Decode the format's geometry representation (e.g., RFC 7946 JSON coordinates, Shapefile part rings, GeoPackage `GP` binary header + WKB, EWKB, or text/delimiters) into `Option<qgis_geometry::Geometry>` (`Point`, `LineString`, `Polygon`, `MultiPoint`, `MultiLineString`, `MultiPolygon`, `GeometryCollection`).
   - Decode record attributes/columns into `BTreeMap<String, serde_json::Value>` (`Null`, `Bool`, `Number`, `String`).

3. **Expose `read_to_features(path)` and implement `FeatureProvider`**
   - Provide a module-level function:
     ```rust
     pub fn read_to_features(path: &str) -> Result<FeatureCollection, String>
     ```
     (or `path: impl AsRef<std::path::Path>` when delegating from `FeatureProvider::read_to_features(&self, path: &str)`).
   - Define a zero-sized provider struct (e.g., `pub struct GeoJsonProvider;`, `pub struct ShapefileProvider;`, `pub struct GpkgProvider;`) implementing `crate::FeatureProvider`:
     ```rust
     pub struct MyFormatProvider;

     impl crate::FeatureProvider for MyFormatProvider {
         fn read_to_features(&self, path: &str) -> Result<crate::FeatureCollection, String> {
             read_to_features(path)
         }
     }
     ```

4. **Add unit tests and a `providers/examples/load_<format>.rs` example**
   - Add unit tests in `providers/src/<format>.rs` (and integration tests in `providers/tests/` if applicable) covering header/record parsing, geometry + attribute extraction, and `read_to_features` against synthetic bytes or `../provider-samples/` fixtures.
   - Add an example binary at `providers/examples/load_<format>.rs` that takes a file/URI argument from `std::env::args().nth(1)`, loads features via the provider, and prints the feature count and sample geometry/properties.

5. **Run verification gates**
   - Verify with `./verify` (never ad-hoc cargo chains):
     ```bash
     ./verify test --crate providers
     ./verify clippy --crate providers
     ```

## Reference Implementations

- `providers/src/lib.rs` — `Feature`, `FeatureCollection`, `FeatureProvider` (`qgis-tdm`)
- `providers/src/geojson.rs` & `providers/examples/load_geojson.rs` — GeoJSON reader/serializer
- `providers/src/shapefile.rs` & `providers/examples/load_shapefile.rs` — Shapefile + DBF reader
- `providers/src/gpkg.rs` & `providers/examples/load_gpkg.rs` — GeoPackage SQLite + GP/WKB reader (`qgis-h3m`)
