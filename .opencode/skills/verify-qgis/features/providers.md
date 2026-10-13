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
