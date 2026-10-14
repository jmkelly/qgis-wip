//! The `providers` suite — E16.1: all five providers, end-to-end.
//!
//! Drives each provider through the unified [`FeatureProvider`] trait (E10.4,
//! bead qgis-tdm): load a fixture, check the feature count, the geometry
//! types, and the attribute fields. An LLM agent runs
//! `./verify agent-providers` and reads one pass/fail row per provider with
//! no human in the loop — the E16 promise applied to E4.
//!
//! The three file providers read committed fixtures from `provider-samples/`,
//! resolved via `CARGO_MANIFEST_DIR` so the suite behaves identically under
//! `cargo test` (crate-root CWD) and `./verify agent-providers` (repo-root
//! CWD). The two database providers speak their real wire protocols
//! (PostgreSQL v3.0, MS-TDS v7.4) against the in-process mock servers their
//! own tests use, so the validation exercises the full client stack —
//! handshake, metadata discovery, query, spatial decoding — with no external
//! database required.

use qgis_geometry::Geometry;
use qgis_providers::FeatureCollection;
use qgis_providers::FeatureProvider;
use qgis_providers::geojson::GeoJsonProvider;
use qgis_providers::gpkg::GpkgProvider;
use qgis_providers::postgis::{MockPostgisLayer, PostgisProvider, spawn_mock_postgis_server};
use qgis_providers::shapefile::ShapefileProvider;
use qgis_providers::sqlserver::{MockSqlServerLayer, SqlServerProvider, spawn_mock_sqlserver};

use crate::CheckFn;

/// Resolves a repo-relative fixture path that works from any CWD.
fn fixture_path(repo_relative: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(repo_relative)
        .to_string_lossy()
        .into_owned()
}

/// The OGC geometry type name of a feature's geometry, `"None"` when null.
fn geom_type_name(geometry: &Option<Geometry>) -> &'static str {
    match geometry {
        Some(Geometry::Point(_)) => "Point",
        Some(Geometry::LineString(_)) => "LineString",
        Some(Geometry::Polygon(_)) => "Polygon",
        Some(Geometry::MultiPoint(_)) => "MultiPoint",
        Some(Geometry::MultiLineString(_)) => "MultiLineString",
        Some(Geometry::MultiPolygon(_)) => "MultiPolygon",
        Some(Geometry::GeometryCollection(_)) => "GeometryCollection",
        None => "None",
    }
}

fn assert_count(fc: &FeatureCollection, expected: usize) -> Result<(), String> {
    if fc.features.len() != expected {
        return Err(format!(
            "expected {expected} features, got {}",
            fc.features.len()
        ));
    }
    Ok(())
}

fn assert_geometry_sequence(fc: &FeatureCollection, expected: &[&str]) -> Result<(), String> {
    let actual: Vec<&str> = fc
        .features
        .iter()
        .map(|f| geom_type_name(&f.geometry))
        .collect();
    if actual != expected {
        return Err(format!("geometry types {actual:?} != expected {expected:?}"));
    }
    Ok(())
}

fn assert_fields(fc: &FeatureCollection, fields: &[&str]) -> Result<(), String> {
    let first = fc.features.first().ok_or("no features to inspect")?;
    for field in fields {
        if !first.properties.contains_key(*field) {
            return Err(format!("missing attribute field: {field}"));
        }
    }
    Ok(())
}

/// One pass/fail check per provider: load the fixture, then verify the
/// feature count, the geometry types, and the attribute fields.
fn shapefile_load() -> Result<(), String> {
    let fc = ShapefileProvider
        .read_to_features(&fixture_path("../provider-samples/france_parts.shp"))?;
    assert_count(&fc, 4)?;
    assert_geometry_sequence(&fc, &["Polygon"; 4])?;
    assert_fields(&fc, &["OBJECTID", "ISO", "NAME_0"])?;
    Ok(())
}

fn geojson_load() -> Result<(), String> {
    let fc = GeoJsonProvider
        .read_to_features(&fixture_path("../provider-samples/points.geojson"))?;
    assert_count(&fc, 17)?;
    assert_geometry_sequence(&fc, &["Point"; 17])?;
    assert_fields(&fc, &["Class", "Heading", "Importance"])?;
    Ok(())
}

fn gpkg_load() -> Result<(), String> {
    let fc = GpkgProvider
        .read_to_features(&fixture_path("../provider-samples/validation_points.gpkg"))?;
    assert_count(&fc, 3)?;
    assert_geometry_sequence(&fc, &["Point"; 3])?;
    assert_fields(&fc, &["name", "value"])?;
    Ok(())
}

fn postgis_load() -> Result<(), String> {
    let port = spawn_mock_postgis_server(MockPostgisLayer::default())?;
    let uri = format!("host=127.0.0.1 port={port} dbname=gis strict=true");
    let fc = PostgisProvider.read_to_features(&uri)?;
    assert_count(&fc, 3)?;
    assert_geometry_sequence(&fc, &["Point", "LineString", "Polygon"])?;
    assert_fields(&fc, &["id", "name", "active", "elevation"])?;
    Ok(())
}

fn sqlserver_load() -> Result<(), String> {
    let port = spawn_mock_sqlserver(MockSqlServerLayer::default())?;
    let uri = format!("host=127.0.0.1 port={port} dbname=gis strict=true");
    let fc = SqlServerProvider.read_to_features(&uri)?;
    assert_count(&fc, 3)?;
    assert_geometry_sequence(&fc, &["Point", "LineString", "Polygon"])?;
    assert_fields(&fc, &["id", "name", "height_m", "open"])?;
    Ok(())
}

pub fn checks() -> Vec<(String, CheckFn)> {
    vec![
        ("shapefile-load".into(), Box::new(|| shapefile_load().into())),
        ("geojson-load".into(), Box::new(|| geojson_load().into())),
        ("gpkg-load".into(), Box::new(|| gpkg_load().into())),
        ("postgis-load".into(), Box::new(|| postgis_load().into())),
        ("sqlserver-load".into(), Box::new(|| sqlserver_load().into())),
    ]
}
