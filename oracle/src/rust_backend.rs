//! Rust backend: the side under test.
//!
//! At E0 no algorithms are ported yet, so this is a deterministic
//! placeholder: it canonicalizes the fixture into a stable summary.
//! E1+ replaces the body with real ported-algorithm dispatch; the
//! golden files get re-recorded at that point.

use crate::backend::BackendError;
use crate::fixture::Fixture;
use crate::output::BackendOutput;

/// Deterministic Rust-side runner.
pub struct RustBackend;

impl RustBackend {
    pub const NAME: &'static str = "rust";

    /// Runs the fixture through the Rust backend.
    ///
    /// The output is a stable canonical form of the fixture: a small
    /// header (backend, fixture name, byte/line counts) followed by the
    /// fixture content with trailing blank lines removed. It depends only
    /// on the fixture's file name and content, never on the current
    /// working directory, so goldens are reproducible.
    pub fn run(fixture: &Fixture) -> Result<BackendOutput, BackendError> {
        let trimmed = fixture.content().trim_end_matches(['\n', '\r']);
        let body = if trimmed.is_empty() {
            String::new()
        } else {
            format!("{trimmed}\n")
        };
        let stdout = format!(
            "backend: {}/0.1.0\nfixture: {}\nbytes: {}\nlines: {}\n---\n{}",
            Self::NAME,
            fixture.name(),
            fixture.content().len(),
            fixture.content().lines().count(),
            body,
        );
        Ok(BackendOutput {
            stdout,
            stderr: String::new(),
            exit_code: Some(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Fixture;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qgis-oracle-rust-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn load(tag: &str, name: &str, content: &str) -> Fixture {
        let dir = temp_dir(tag);
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        Fixture::load(&path).unwrap()
    }

    #[test]
    fn output_is_deterministic() {
        let a = load("det-a", "trivial.txt", "hello oracle\nsecond line\n");
        let b = load("det-b", "trivial.txt", "hello oracle\nsecond line\n");
        assert_eq!(RustBackend::run(&a).unwrap(), RustBackend::run(&b).unwrap());
    }

    #[test]
    fn output_contains_header_and_body() {
        let fixture = load("shape", "trivial.txt", "hello oracle\nsecond line\n");
        let out = RustBackend::run(&fixture).unwrap();
        let expected = "backend: rust/0.1.0\nfixture: trivial.txt\nbytes: 25\nlines: 2\n---\nhello oracle\nsecond line\n";
        assert_eq!(out.stdout, expected);
        assert_eq!(out.stderr, "");
        assert_eq!(out.exit_code, Some(0));
    }

    #[test]
    fn trailing_blank_lines_are_stripped() {
        let fixture = load("blanks", "b.txt", "data\n\n\n");
        let out = RustBackend::run(&fixture).unwrap();
        assert!(out.stdout.ends_with("---\ndata\n"), "stdout: {:?}", out.stdout);
    }

    #[test]
    fn empty_fixture_produces_header_only() {
        let fixture = load("empty", "e.txt", "");
        let out = RustBackend::run(&fixture).unwrap();
        assert_eq!(out.stdout, "backend: rust/0.1.0\nfixture: e.txt\nbytes: 0\nlines: 0\n---\n");
    }

    #[test]
    fn output_uses_file_name_not_full_path() {
        let fixture = load("cwd", "trivial.txt", "x\n");
        let out = RustBackend::run(&fixture).unwrap();
        assert!(out.stdout.contains("fixture: trivial.txt\n"));
        assert!(!out.stdout.contains("temp_dir"));
    }
}
