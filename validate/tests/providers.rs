//! End-to-end test for the E16.1 acceptance criteria:
//! "./verify agent-providers produces a pass/fail report for all 5 providers;
//! agent can run it and interpret results".
//!
//! The wrong-behaviour case this pins: the `providers` suite is not
//! registered, so `./verify agent-providers` exits 2 with "unknown suite
//! 'providers'" and an LLM agent CANNOT validate the providers epic
//! end-to-end. `cli::run(["providers"])` must return SUCCESS, and the
//! report must carry one pass/fail check per provider (shapefile, geojson,
//! gpkg, postgis, sqlserver).

use std::process::ExitCode;

use qgis_validate::{cli, suites};

#[test]
fn agent_providers_suite_is_registered_and_passes() {
    // This is exactly what `./verify agent-providers` runs.
    let code = cli::run(&["providers".to_string()]);
    assert_eq!(
        code,
        ExitCode::SUCCESS,
        "providers suite must be registered, run, and pass — got exit {code:?}"
    );
}

#[test]
fn providers_suite_reports_one_check_per_provider() {
    let report = suites::run("providers").expect("providers suite must be registered");
    let names: Vec<&str> = report.checks.iter().map(|c| c.name.as_str()).collect();
    for provider in ["shapefile", "geojson", "gpkg", "postgis", "sqlserver"] {
        assert!(
            names.iter().any(|n| n.contains(provider)),
            "no check covers provider '{provider}'; checks were: {names:?}"
        );
    }
    assert!(
        report.passed(),
        "all five providers must load their fixtures: {:?}",
        report
            .checks
            .iter()
            .map(|c| (c.name.as_str(), c.status.token()))
            .collect::<Vec<_>>()
    );
    assert_eq!(report.counts().1, 0, "no provider check may fail");
}
