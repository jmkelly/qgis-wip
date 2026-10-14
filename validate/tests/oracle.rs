//! Reproduction test for bead qgis-ph8 (E16.4: Agent validation — oracle diff
//! against QGIS C++ reference).
//!
//! Acceptance criteria: "`./verify agent-oracle` produces a diff report for
//! geometry, styles, and project files; agent can run it and interpret
//! results."
//!
//! The wrong-behaviour case this pins: `oracle` is not a registered suite, so
//! `./verify agent-oracle` exits `2` with `error: unknown suite 'oracle'` and
//! an agent gets no diff report at all. These tests fail until the suite is
//! registered and drives the load/style/save pipeline over the fixture set,
//! comparing each artifact against its QGIS C++ golden.

use std::process::ExitCode;

use qgis_validate::{cli, suites};

/// The three load/style/save artifact families the report must cover.
const KINDS: [&str; 3] = ["geometry", "styles", "projects"];

#[test]
fn oracle_suite_is_registered() {
    assert!(
        suites::SUITES.contains(&"oracle"),
        "oracle must be a registered suite; got {:?}",
        suites::SUITES
    );
}

#[test]
fn agent_oracle_exits_zero() {
    // The acceptance command: `./verify agent-oracle` must pass end-to-end.
    assert_eq!(cli::run(&["oracle".to_string()]), ExitCode::SUCCESS, "agent-oracle must exit 0");
}

#[test]
fn oracle_report_covers_each_pipeline_kind() {
    let report = suites::run("oracle").expect("oracle suite is registered");

    // One reference-diff verdict per kind, plus one per-fixture row.
    for kind in KINDS {
        let aggregate = format!("{kind}:reference");
        assert!(
            report.checks.iter().any(|c| c.name == aggregate),
            "missing per-kind reference verdict '{aggregate}'; got {:?}",
            report.checks.iter().map(|c| c.name.as_str()).collect::<Vec<_>>()
        );
        let prefix = format!("{kind}/");
        assert!(
            report.checks.iter().any(|c| c.name.starts_with(&prefix)),
            "missing per-fixture rows for '{kind}'"
        );
    }

    assert!(report.passed(), "oracle report must pass: {:#?}", report.checks);
    assert_eq!(report.exit_code(), 0);
}
