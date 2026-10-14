//! Thin binary entry point for the agent-runnable validation harness.
//!
//! All logic lives in the `qgis-validate` library ([`qgis_validate::cli`]) so
//! it stays testable; this `main` only forwards arguments.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(2).collect();
    qgis_validate::cli::run(&args)
}
