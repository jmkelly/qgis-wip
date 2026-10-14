//! Oracle harness for the QGIS Rust port.
//!
//! Runs identical fixture inputs through named backends (C++ and Rust),
//! captures their output, and diffs the two. A golden-file store captures
//! expected outputs for regression checks.
//!
//! See ADR-0004 (oracle + workspace first) and ADR-0001 (pure-Rust, no FFI):
//! this crate is pure Rust with no dependencies; the C++ side is reached
//! only by shelling out to a user-supplied binary via [`std::process::Command`].

#![forbid(unsafe_code)]

pub mod backend;
pub mod cli;
pub mod cpp_backend;
pub mod diff;
pub mod fixture;
pub mod goldens;
pub mod output;
pub mod pipeline;
pub mod rust_backend;

pub use backend::{BackendError, BackendKind};
pub use diff::Diff;
pub use fixture::{Fixture, FixtureError};
pub use goldens::{GoldenError, GoldenStore, Mode};
pub use output::BackendOutput;
pub use pipeline::{compare_to_golden, PipelineKind, Reference};

/// Process-wide lock for tests that mutate environment variables.
#[cfg(test)]
pub(crate) fn env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    &LOCK
}
