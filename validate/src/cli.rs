//! CLI for the agent-runnable validation harness.
//!
//! Canonical invocation: `qgis-validate agent <suite>` runs one registered
//! suite, prints a human-readable report followed by a machine-readable JSON
//! block, and exits with the report's exit code (`0` iff no check failed).
//!
//! `qgis-validate agent --list` prints the registered suite names.

use std::process::ExitCode;

use crate::suites;

const USAGE: &str = "usage: qgis-validate agent <suite> | qgis-validate agent --list";

/// Entry point for the `agent` subcommand. Returns the process exit code.
pub fn run(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    match args[0].as_str() {
        "--list" | "-l" => {
            for name in suites::SUITES {
                println!("{name}");
            }
            ExitCode::SUCCESS
        }
        name => match suites::run(name) {
            Some(report) => {
                print!("{}", report.render());
                println!();
                println!("json:");
                println!("{}", report.to_json());
                ExitCode::from(report.exit_code() as u8)
            }
            None => {
                eprintln!("error: unknown suite '{name}'");
                eprintln!("available suites: {}", suites::SUITES.join(", "));
                eprintln!("{USAGE}");
                ExitCode::from(2)
            }
        },
    }
}
