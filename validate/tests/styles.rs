//! Reproduction test for bead qgis-sxu (E16.2: Agent validation — style engine).
//!
//! Acceptance criteria: "`./verify agent-styles` produces a pass/fail report for
//! all 11 .qml fixtures; agent can run it and interpret results."
//!
//! The wrong-behaviour case this pins: `styles` is not a registered suite, so
//! `./verify agent-styles` exits `2` with `error: unknown suite 'styles'` and an
//! agent gets no report at all. These tests fail until the suite is registered
//! and drives the real `qgis-style` engine over the fixture directory.

use std::process::ExitCode;

use qgis_validate::{cli, suites};

/// The three renderer families the style engine understands, as the report
/// names them.
const RENDERER_TYPES: [&str; 3] = ["singleSymbol", "categorizedSymbol", "graduatedSymbol"];

#[test]
fn styles_suite_is_registered() {
    assert!(
        suites::SUITES.contains(&"styles"),
        "styles must be a registered suite; got {:?}",
        suites::SUITES
    );
}

#[test]
fn agent_styles_exits_zero() {
    // The acceptance command: `./verify agent-styles` must pass end-to-end.
    assert_eq!(
        cli::run(&["styles".to_string()]),
        ExitCode::SUCCESS,
        "agent-styles must exit 0"
    );
}

#[test]
fn styles_report_covers_all_eleven_fixtures() {
    let report = suites::run("styles").expect("styles suite is registered");

    // One check per .qml fixture, and every one of them must pass.
    let fixture_checks: Vec<_> = report
        .checks
        .iter()
        .filter(|c| c.name.ends_with(".qml"))
        .collect();
    assert_eq!(
        fixture_checks.len(),
        11,
        "expected a check for each of the 11 .qml fixtures, got {:?}",
        report
            .checks
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
    );
    assert!(report.passed(), "report must pass: {:#?}", report.checks);
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn styles_report_has_a_verdict_per_renderer_type() {
    let report = suites::run("styles").expect("styles suite is registered");
    for renderer in RENDERER_TYPES {
        let name = format!("renderer:{renderer}");
        assert!(
            report.checks.iter().any(|c| c.name == name),
            "missing per-renderer-type check '{name}'"
        );
    }
}
