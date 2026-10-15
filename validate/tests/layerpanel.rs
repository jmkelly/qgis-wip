//! End-to-end test for the E14.3 acceptance criteria: "Layer panel shows
//! loaded layers, allows toggling visibility, reordering, and deleting; agent
//! can run it".
//!
//! The wrong-behaviour case this pins: there is no headless way for an agent to
//! check what the layer panel shows, so `./verify agent-layerpanel` exits 2
//! with "unknown suite 'layerpanel'" and E14.3 cannot be validated end-to-end.
//! `cli::run(["layerpanel"])` must return SUCCESS, and the report must carry a
//! pass/fail check for showing loaded layers, toggling visibility, reordering,
//! and deleting.

use std::process::ExitCode;

use qgis_validate::{cli, suites};

#[test]
fn agent_layerpanel_suite_is_registered_and_passes() {
    // This is exactly what `./verify agent-layerpanel` runs.
    let code = cli::run(&["layerpanel".to_string()]);
    assert_eq!(
        code,
        ExitCode::SUCCESS,
        "layerpanel suite must be registered, run, and pass — got exit {code:?}"
    );
}

#[test]
fn layerpanel_suite_covers_the_acceptance() {
    let report = suites::run("layerpanel").expect("layerpanel suite must be registered");
    let names: Vec<&str> = report.checks.iter().map(|c| c.name.as_str()).collect();
    for want in ["loaded-layers", "toggle", "reorder", "delete"] {
        assert!(
            names.iter().any(|n| n.contains(want)),
            "no check covers '{want}'; checks were: {names:?}"
        );
    }
    assert!(
        report.passed(),
        "the layer panel acceptance checks must pass: {:?}",
        report
            .checks
            .iter()
            .map(|c| (c.name.as_str(), c.status.token()))
            .collect::<Vec<_>>()
    );
    assert_eq!(report.counts().1, 0, "no layer-panel check may fail");
}
