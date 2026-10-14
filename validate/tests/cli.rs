//! End-to-end test for the agent-runnable gate (E16 acceptance criteria):
//! "An LLM agent can validate an epic's acceptance criteria end-to-end without
//! human intervention."
//!
//! Drives `qgis_validate::cli` the way `./verify agent` does — by suite name —
//! and asserts the exit-code contract an agent relies on.

use std::process::ExitCode;

use qgis_validate::cli;

#[test]
fn agent_selfcheck_passes_end_to_end() {
    // The selfcheck suite validates E1 (qgis-geometry) acceptance criteria.
    let code = cli::run(&["selfcheck".to_string()]);
    assert_eq!(code, ExitCode::SUCCESS, "selfcheck must pass");
}

#[test]
fn agent_list_succeeds() {
    let code = cli::run(&["--list".to_string()]);
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn agent_project_passes_end_to_end() {
    // The project suite validates E15 (QGS project save/load) acceptance
    // criteria: save a multi-layer project to .qgs, read it back, compare
    // layer count, names, CRS, styles, and viewport.
    let code = cli::run(&["project".to_string()]);
    assert_eq!(code, ExitCode::SUCCESS, "project must pass");
}

#[test]
fn agent_unknown_suite_fails() {
    let code = cli::run(&["no-such-suite".to_string()]);
    assert_eq!(code, ExitCode::from(2));
}

#[test]
fn agent_no_args_is_usage_error() {
    let code = cli::run(&[]);
    assert_eq!(code, ExitCode::from(2));
}
