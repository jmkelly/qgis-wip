//! Integration tests against the real fixture store at the workspace root
//! (resolved relative to this crate via `CARGO_MANIFEST_DIR`, overridable
//! with `QGIS_FIXTURES_MANIFEST`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use qgis_fixtures::{Category, Manifest};

const STORE_CATEGORY_DIRS: [&str; 4] = [
    "qgs-projects",
    "styles",
    "expressions",
    "provider-samples",
];

#[test]
fn default_manifest_covers_all_categories() {
    let manifest = Manifest::load_default().expect("default manifest parses");
    for category in [
        Category::QgsProject,
        Category::Style,
        Category::Expression,
        Category::ProviderSample,
    ] {
        assert!(
            manifest.by_category(category).next().is_some(),
            "no fixtures in category {category}"
        );
    }
}

#[test]
fn validate_finds_no_missing_files() {
    let manifest = Manifest::load_default().expect("default manifest parses");
    if let Err(errors) = manifest.validate() {
        let rendered: Vec<String> = errors.iter().map(ToString::to_string).collect();
        panic!("manifest entries missing on disk: {rendered:?}");
    }
}

#[test]
fn expected_fixture_counts_by_category() {
    let manifest = Manifest::load_default().expect("default manifest parses");
    assert_eq!(manifest.by_category(Category::QgsProject).count(), 9);
    assert_eq!(manifest.by_category(Category::Style).count(), 11);
    assert_eq!(manifest.by_category(Category::Expression).count(), 1);
    assert_eq!(manifest.by_category(Category::ProviderSample).count(), 11);
}

#[test]
fn every_store_file_is_cataloged() {
    let manifest = Manifest::load_default().expect("default manifest parses");

    let mut on_disk = BTreeSet::new();
    for dir in STORE_CATEGORY_DIRS {
        collect_files(manifest.store_root(), &manifest.store_root().join(dir), &mut on_disk);
    }

    let mut cataloged: BTreeSet<PathBuf> = BTreeSet::new();
    for entry in manifest.entries() {
        cataloged.insert(entry.path().to_path_buf());
        for sidecar in entry.sidecars() {
            cataloged.insert(sidecar.clone());
        }
    }

    let unlisted: Vec<_> = on_disk.difference(&cataloged).collect();
    let missing: Vec<_> = cataloged.difference(&on_disk).collect();
    assert!(unlisted.is_empty(), "files on disk but not in the manifest: {unlisted:?}");
    assert!(missing.is_empty(), "manifest entries not on disk: {missing:?}");
}

#[test]
fn shapefile_sidecars_are_listed_and_present() {
    let manifest = Manifest::load_default().expect("default manifest parses");
    let shp = manifest.find("provider_bug5598_shp").expect("shapefile fixture is cataloged");
    assert_eq!(shp.sidecars().len(), 3);
    assert!(shp.resolve(manifest.store_root()).is_file());
    for sidecar in shp.sidecars() {
        assert!(
            manifest.store_root().join(sidecar).is_file(),
            "missing sidecar {}",
            sidecar.display()
        );
    }
}

fn collect_files(root: &Path, dir: &Path, out: &mut BTreeSet<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("store directory is readable") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            collect_files(root, &path, out);
        } else {
            out.insert(path.strip_prefix(root).expect("path is under root").to_path_buf());
        }
    }
}
