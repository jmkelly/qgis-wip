//! Reproduction test for bead qgis-gzq (E16: LLM-agent validation harness).
//!
//! Acceptance criteria: "An LLM agent can validate any epic's acceptance
//! criteria end-to-end without human intervention."
//!
//! The wrong-behaviour case this pins: there is no shared validation report
//! format and no agent-runnable gate, so an agent CANNOT drive a validation
//! end-to-end and read a structured pass/fail verdict. These tests fail to
//! compile until `qgis-validate` provides `Report`, `Runner`, and the
//! pass/fail + exit-code + JSON contract.

use qgis_validate::{Outcome, Report, Runner, Status};

#[test]
fn report_classifies_pass_fail_skip() {
    let mut report = Report::new("epic-e1-geometry");
    report.add(qgis_validate::Check::new("wkt-roundtrip", Status::Pass, 1, "ok"));
    report.add(qgis_validate::Check::new("area-positive", Status::Fail, 1, "area was negative"));
    report.add(qgis_validate::Check::new("skipped-check", Status::Skip, 0, "not applicable"));

    assert_eq!(report.counts(), (1, 1, 1));
    // A single Fail makes the whole run fail.
    assert!(!report.passed());
}

#[test]
fn report_passes_when_no_failures() {
    let mut report = Report::new("epic-e1-geometry");
    report.add(qgis_validate::Check::new("a", Status::Pass, 1, "ok"));
    report.add(qgis_validate::Check::new("b", Status::Skip, 0, "n/a"));
    assert!(report.passed());
}

#[test]
fn runner_times_checks_and_builds_report() {
    let checks = vec![
        (
            "passing".to_string(),
            Box::new(|| Outcome::Pass) as qgis_validate::CheckFn,
        ),
        (
            "failing".to_string(),
            Box::new(|| Outcome::fail("boom")) as qgis_validate::CheckFn,
        ),
    ];
    let report = Runner::run("epic-e1-geometry", checks);
    assert_eq!(report.counts(), (1, 1, 0));
    assert!(!report.passed());
    // The failing check's detail carries the error message.
    let failed = report
        .checks
        .iter()
        .find(|c| c.name == "failing")
        .expect("failing check present");
    assert_eq!(failed.detail, "boom");
    assert_eq!(failed.status, Status::Fail);
}

#[test]
fn runner_marks_unavailable_check_as_skipped() {
    // A dependency-gated check (e.g. PostGIS/SQL Server) that cannot run must
    // be reported as Skip, not Fail. Otherwise `./verify agent-providers`
    // would report a failure on a machine with no database, and the agent
    // could not tell a real regression from an absent service.
    let checks = vec![
        (
            "shapefile-load".to_string(),
            Box::new(|| Outcome::Pass) as qgis_validate::CheckFn,
        ),
        (
            "postgis-load".to_string(),
            Box::new(|| Outcome::skip("PostGIS not reachable")) as qgis_validate::CheckFn,
        ),
    ];
    let report = Runner::run("providers", checks);
    assert_eq!(report.counts(), (1, 0, 1));
    assert!(report.passed(), "a skipped check must not fail the run");
    assert_eq!(report.exit_code(), 0);
    let skipped = report
        .checks
        .iter()
        .find(|c| c.name == "postgis-load")
        .expect("skipped check present");
    assert_eq!(skipped.status, Status::Skip);
    assert_eq!(skipped.detail, "PostGIS not reachable");
    assert!(
        report.to_json().contains("\"status\": \"skip\""),
        "skip must survive into the machine-readable report"
    );
}

#[test]
fn report_renders_human_table() {
    let mut report = Report::new("epic-e1-geometry");
    report.add(qgis_validate::Check::new("wkt-roundtrip", Status::Pass, 3, "ok"));
    let rendered = report.render();
    assert!(rendered.contains("epic-e1-geometry"), "suite name in header");
    assert!(rendered.contains("wkt-roundtrip"), "check name in row");
    assert!(rendered.contains("PASS"), "status token in row");
}

#[test]
fn report_emits_machine_readable_json() {
    let mut report = Report::new("epic-e1-geometry");
    report.add(qgis_validate::Check::new("wkt-roundtrip", Status::Pass, 3, "ok"));
    let json = report.to_json();
    assert!(json.contains("\"suite\""), "suite field");
    assert!(json.contains("epic-e1-geometry"), "suite value");
    assert!(json.contains("\"status\": \"pass\""), "status value");
    assert!(json.contains("\"passed\": true"), "top-level passed flag");
}

#[test]
fn exit_code_contract() {
    let mut ok = Report::new("s");
    ok.add(qgis_validate::Check::new("a", Status::Pass, 0, "ok"));
    assert_eq!(ok.exit_code(), 0);

    let mut bad = Report::new("s");
    bad.add(qgis_validate::Check::new("a", Status::Fail, 0, "no"));
    assert_eq!(bad.exit_code(), 1);
}
