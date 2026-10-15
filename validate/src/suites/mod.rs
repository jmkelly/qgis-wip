//! Suite registry for the `qgis-validate agent` CLI.
//!
//! Each suite is a named set of checks that validates one epic's acceptance
//! criteria end-to-end. The parent epic (E16, bead qgis-gzq) ships
//! [`selfcheck`]; the E16.x children add `providers`, `styles`, `project`, and
//! `oracle` here as their own modules plus a match arm below.

use crate::{CheckFn, Report, Runner};

pub mod layerpanel;
pub mod oracle;
pub mod project;
pub mod providers;
pub mod selfcheck;
pub mod styles;

/// Names of every registered suite, for `--list`.
pub const SUITES: &[&str] = &[
    "layerpanel",
    "oracle",
    "project",
    "selfcheck",
    "providers",
    "styles",
];

/// Run a named suite, or `None` if no such suite is registered.
pub fn run(name: &str) -> Option<Report> {
    let checks: Vec<(String, CheckFn)> = match name {
        "layerpanel" => layerpanel::checks(),
        "oracle" => oracle::checks(),
        "project" => project::checks(),
        "selfcheck" => selfcheck::checks(),
        "providers" => providers::checks(),
        "styles" => styles::checks(),
        _ => return None,
    };
    Some(Runner::run(name, checks))
}
