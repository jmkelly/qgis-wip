# Feature map: qgis-providers

## What it does

Data provider facade replacing GDAL/SpatiaLite/PDAL/MDAL. Manages
connections, layers, feature iteration, and field access across
multiple data source types.

## How to verify

```bash
./verify test --crate providers
./verify clippy --crate providers
```

## What proves each feature works

| Feature | Evidence |
|---------|----------|
| Provider registry | `registry.rs` tests |
| Connection management | `connection.rs` tests |
| Layer enumeration | `layer.rs` tests |
| Feature iteration | `feature.rs` tests |
| Field access | `field.rs` tests |

## Oracle coverage

Provider samples in `provider-samples/` are real QGIS test data.
Add oracle fixtures when providers start reading actual files.

## GeoPackage / sqlite inspection

GeoPackage files are SQLite databases. Use `sqlite3` (read-only where
possible) to inspect schema and contents before writing provider code:

```bash
# Layer inventory (table name, geometry type, srs, bounds)
sqlite3 /path/to/file.gpkg "SELECT table_name, data_type, srs_id FROM gpkg_contents;"

# Table schema (columns, types, constraints)
sqlite3 /path/to/file.gpkg ".schema <table_name>"

# Row counts per user table
sqlite3 /path/to/file.gpkg "SELECT name FROM sqlite_master WHERE type='table';"
sqlite3 /path/to/file.gpkg "SELECT COUNT(*) FROM <table_name>;"

# Geometry layout (gpkg_geometry_columns) and spatial refsys
sqlite3 /path/to/file.gpkg "SELECT * FROM gpkg_geometry_columns;"
```

Prefer `SELECT` / `.schema` queries; avoid writes against fixtures.
