//! qgis-validate — agent-runnable validation harness for the QGIS Rust port.
//!
//! This crate is the shared infrastructure behind the `./verify agent-*`
//! acceptance gates (epic E16, bead qgis-gzq). It provides:
//!
//! * A [`Report`] — the validation report format every agent gate emits, so an
//!   LLM agent can read a structured pass/fail verdict with no human in the loop.
//! * A [`Runner`] — executes a named suite of checks and collects timings.
//! * An exit-code contract — `0` iff the report has no `Fail` check.
//!
//! The crate is pure Rust and dependency-light: the only external crate is
//! `serde_json` (already pinned in the workspace) for machine-readable output.
//! It performs no I/O and links nothing, so it satisfies ADR-0001.

#![forbid(unsafe_code)]

use std::time::Instant;

pub mod cli;
pub mod suites;

/// Outcome of a single validation check.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Pass,
    Fail,
    Skip,
}

impl Status {
    /// Machine-readable token used in JSON output and the human table.
    pub fn token(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail => "fail",
            Status::Skip => "skip",
        }
    }

    /// Uppercase token used in the human-readable table.
    pub fn label(self) -> &'static str {
        match self {
            Status::Pass => "PASS",
            Status::Fail => "FAIL",
            Status::Skip => "SKIP",
        }
    }
}

/// A single named check and its result.
#[derive(Clone, Debug)]
pub struct Check {
    pub name: String,
    pub status: Status,
    pub duration_ms: u128,
    pub detail: String,
}

impl Check {
    pub fn new(
        name: impl Into<String>,
        status: Status,
        duration_ms: u128,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status,
            duration_ms,
            detail: detail.into(),
        }
    }
}

/// A validation check: run something, return `Ok(())` on pass or `Err(detail)`
/// on fail. Boxed so a suite can mix heterogeneous checks in one vector.
pub type CheckFn = Box<dyn Fn() -> Result<(), String>>;

/// The validation report format (E16 deliverable). One report per suite run.
#[derive(Clone, Debug)]
pub struct Report {
    pub suite: String,
    pub checks: Vec<Check>,
}

impl Report {
    pub fn new(suite: impl Into<String>) -> Self {
        Self {
            suite: suite.into(),
            checks: Vec::new(),
        }
    }

    pub fn add(&mut self, check: Check) {
        self.checks.push(check);
    }

    /// `(passed, failed, skipped)` counts.
    pub fn counts(&self) -> (usize, usize, usize) {
        let (mut p, mut f, mut s) = (0, 0, 0);
        for c in &self.checks {
            match c.status {
                Status::Pass => p += 1,
                Status::Fail => f += 1,
                Status::Skip => s += 1,
            }
        }
        (p, f, s)
    }

    /// The run passes iff no check failed (skips do not fail the run).
    pub fn passed(&self) -> bool {
        !self.checks.iter().any(|c| c.status == Status::Fail)
    }

    /// Process exit code for the run: `0` iff [`passed`](Report::passed).
    pub fn exit_code(&self) -> i32 {
        if self.passed() {
            0
        } else {
            1
        }
    }

    /// Human-readable summary table.
    pub fn render(&self) -> String {
        let (p, f, s) = self.counts();
        let mut out = String::new();
        out.push_str(&format!("validation report: {}\n", self.suite));
        out.push_str(&format!(
            "checks: {} passed, {} failed, {} skipped\n",
            p, f, s
        ));
        out.push_str("----------------------------------------\n");
        for c in &self.checks {
            out.push_str(&format!(
                "{:<5} {:<24} {:>6}ms  {}\n",
                c.status.label(),
                c.name,
                c.duration_ms,
                c.detail
            ));
        }
        out.push_str("----------------------------------------\n");
        out.push_str(&format!(
            "result: {}\n",
            if self.passed() { "PASS" } else { "FAIL" }
        ));
        out
    }

    /// Machine-readable JSON rendering of the report.
    pub fn to_json(&self) -> String {
        let (p, f, s) = self.counts();
        let checks: Vec<serde_json::Value> = self
            .checks
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "status": c.status.token(),
                    "duration_ms": c.duration_ms,
                    "detail": c.detail,
                })
            })
            .collect();
        let value = serde_json::json!({
            "suite": self.suite,
            "passed": self.passed(),
            "counts": { "pass": p, "fail": f, "skip": s },
            "checks": checks,
        });
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Runs a named suite of checks, timing each one, and collects the [`Report`].
pub struct Runner;

impl Runner {
    pub fn run(suite: impl Into<String>, checks: Vec<(String, CheckFn)>) -> Report {
        let mut report = Report::new(suite);
        for (name, run) in checks {
            let start = Instant::now();
            let (status, detail) = match run() {
                Ok(()) => (Status::Pass, "ok".to_string()),
                Err(e) => (Status::Fail, e),
            };
            let duration_ms = start.elapsed().as_millis();
            report.add(Check::new(name, status, duration_ms, detail));
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_tokens() {
        assert_eq!(Status::Pass.token(), "pass");
        assert_eq!(Status::Fail.token(), "fail");
        assert_eq!(Status::Skip.token(), "skip");
    }
}
