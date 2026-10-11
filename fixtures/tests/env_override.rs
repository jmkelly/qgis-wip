//! Tests for `Manifest::load_default()` environment override.
//!
//! This lives in a separate integration test binary (own process)
//! because it mutates the process-global `QGIS_FIXTURES_MANIFEST`
//! variable, which would race with the parallel store tests.

use std::path::PathBuf;

use qgis_fixtures::Manifest;

#[test]
fn env_var_overrides_default_location() {
    let path = std::env::temp_dir()
        .join(format!("qgis-fixtures-env-override-{}.toml", std::process::id()));
    std::fs::write(
        &path,
        "[fixture.synthetic]\ncategory = \"style\"\nsource = \"synthetic\"\npath = \"s.qml\"\ndescription = \"authored fixture\"\n",
    )
    .unwrap();

    std::env::set_var("QGIS_FIXTURES_MANIFEST", &path);
    let manifest = Manifest::load_default().expect("env-provided manifest parses");
    std::env::remove_var("QGIS_FIXTURES_MANIFEST");

    assert_eq!(manifest.entries().len(), 1);
    let entry = manifest.find("synthetic").expect("synthetic fixture is cataloged");
    assert_eq!(entry.source(), "synthetic");
    assert_eq!(entry.category(), qgis_fixtures::Category::Style);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn default_location_is_used_without_env_var() {
    // QGIS_FIXTURES_MANIFEST is not set in this process (the override
    // test cleans up after itself, and tests may run in any order).
    if std::env::var_os("QGIS_FIXTURES_MANIFEST").is_some() {
        return;
    }
    let manifest = Manifest::load_default().expect("default manifest parses");
    assert!(manifest.entries().len() > 1, "default manifest should describe the real store");
    let _unused: PathBuf = manifest.store_root().to_path_buf();
}
