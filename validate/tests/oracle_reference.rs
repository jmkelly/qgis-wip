//! Reference-layer acceptance test for bead qgis-ph8 (E16.4).
//!
//! The `./verify agent-oracle` report only proves the load/style/save pipeline
//! runs; this test proves the *diff* half actually compares Rust artifacts
//! against the QGIS C++ reference. It plants a golden and a fake
//! `QGIS_CPP_BINARY` in a temp directory, points the suite at them via
//! `QGIS_ORACLE_GOLDEN_DIR`/`QGIS_CPP_BINARY`, and asserts the per-fixture
//! reference row and the `cpp` aggregate flip between pass and fail as the
//! reference matches or drifts.
//!
//! It lives in its own test binary so its process-wide environment mutation
//! cannot race the other `oracle` suite tests.

use std::fs;
use std::path::{Path, PathBuf};

use qgis_oracle::PipelineKind;
use qgis_validate::{suites, Check, Report, Status};

fn geometry_point() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/fixtures/geometry/point.wkt")
}

fn check<'a>(report: &'a Report, name: &str) -> &'a Check {
    report
        .checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("missing check '{name}'"))
}

#[test]
fn reference_layer_verifies_matches_and_catches_drift() {
    let kind = PipelineKind::Geometry;
    let path = geometry_point();
    let artifact = suites::oracle::canonical_artifact(kind, &path).expect("geometry pipeline runs");

    let store = std::env::temp_dir().join(format!("qgis-ph8-goldens-{}", std::process::id()));
    let _ = fs::remove_dir_all(&store);
    fs::create_dir_all(store.join("geometry")).unwrap();

    // 1. No golden recorded yet -> the per-fixture row skips and names the gap.
    std::env::set_var("QGIS_ORACLE_GOLDEN_DIR", &store);
    std::env::remove_var("QGIS_CPP_BINARY");
    let report = suites::run("oracle").expect("oracle suite is registered");
    assert_eq!(check(&report, "geometry/point.wkt").status, Status::Skip);

    // 2. Record the QGIS C++ reference (provenance line says `cpp`) -> the row
    //    passes, proving backend provenance is stripped before the diff.
    let reference = artifact.replacen("backend: rust/0.1.0", "backend: cpp/0.1.0", 1);
    fs::write(store.join("geometry/point.expected"), &reference).unwrap();
    let report = suites::run("oracle").expect("oracle suite is registered");
    assert_eq!(
        check(&report, "geometry/point.wkt").status,
        Status::Pass,
        "{:#?}",
        report.checks
    );
    assert_eq!(check(&report, "geometry:reference").status, Status::Pass);

    // 3. Drift the golden -> the row fails with a unified diff.
    fs::write(
        store.join("geometry/point.expected"),
        "backend: cpp/0.1.0\nkind: geometry\n---\nDRIFT\n",
    )
    .unwrap();
    let report = suites::run("oracle").expect("oracle suite is registered");
    let drifted = check(&report, "geometry/point.wkt");
    assert_eq!(drifted.status, Status::Fail);
    assert!(drifted.detail.contains("golden drift"), "detail: {}", drifted.detail);
    assert!(!report.passed());

    // 4. A fake QGIS C++ binary whose output drifts makes the live `cpp` row
    //    fail, proving the Rust-vs-C++ diff path is wired.
    let script = store.join("fake-cpp.sh");
    fs::write(
        &script,
        "#!/bin/sh\nprintf 'backend: cpp/0.1.0\\nkind: geometry\\n---\\nDRIFT\\n'\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::env::set_var("QGIS_CPP_BINARY", &script);
    let report = suites::run("oracle").expect("oracle suite is registered");
    assert_eq!(check(&report, "geometry:cpp").status, Status::Fail);

    std::env::remove_var("QGIS_CPP_BINARY");
    std::env::remove_var("QGIS_ORACLE_GOLDEN_DIR");
    let _ = fs::remove_dir_all(&store);
}
