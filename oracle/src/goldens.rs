//! Golden-file store for oracle outputs.
//!
//! Convention: one golden per fixture, named `<fixture-stem>.expected`,
//! living in `goldens/` under the crate root (overridable with
//! `QGIS_ORACLE_GOLDEN_DIR`). Verify mode compares backend output against
//! the golden; Record mode overwrites it.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::diff::Diff;
use crate::output::strip_backend_line;

/// Whether the oracle verifies against goldens or re-records them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Compare backend output against the stored golden.
    Verify,
    /// Overwrite the golden with the current backend output.
    Record,
}

impl Mode {
    /// Parses an environment-variable value. `None` when unrecognized.
    pub fn parse(value: &str) -> Option<Mode> {
        match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(Mode::Record),
            "0" | "false" | "no" | "off" | "" => Some(Mode::Verify),
            _ => None,
        }
    }

    /// Reads `ORACLE_UPDATE_GOLDENS`; defaults to [`Mode::Verify`].
    pub fn from_env() -> Mode {
        std::env::var("ORACLE_UPDATE_GOLDENS")
            .ok()
            .and_then(|v| Mode::parse(&v))
            .unwrap_or(Mode::Verify)
    }
}

/// Errors from golden store operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoldenError {
    /// No golden file exists yet for this name.
    MissingGolden { path: PathBuf },
    /// The golden file could not be read or written.
    Io { path: PathBuf, message: String },
}

impl fmt::Display for GoldenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GoldenError::MissingGolden { path } => {
                write!(f, "no golden file at {}", path.display())
            }
            GoldenError::Io { path, message } => {
                write!(f, "golden file {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for GoldenError {}

/// Store of expected outputs, one file per fixture.
pub struct GoldenStore {
    root: PathBuf,
}

impl GoldenStore {
    /// Creates a store rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        GoldenStore { root: root.into() }
    }

    /// Creates a store from `QGIS_ORACLE_GOLDEN_DIR`, falling back to
    /// `<crate>/goldens`.
    pub fn from_env() -> Self {
        match std::env::var("QGIS_ORACLE_GOLDEN_DIR") {
            Ok(dir) if !dir.trim().is_empty() => GoldenStore::new(dir),
            _ => GoldenStore::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens")),
        }
    }

    /// Path of the golden file for `name`.
    pub fn golden_path(&self, name: &str) -> PathBuf {
        self.root.join(format!("{name}.expected"))
    }

    /// Compares `actual` against the golden for `name`.
    ///
    /// * [`Mode::Verify`]: `Ok(None)` when identical, `Ok(Some(diff))`
    ///   when they differ, or an error when the golden is missing or
    ///   unreadable. Both sides have their leading `backend:` line
    ///   stripped first (see [`strip_backend_line`]), so goldens are
    ///   comparable across backends.
    /// * [`Mode::Record`]: writes `actual` to the golden, `Ok(None)`.
    pub fn compare(
        &self,
        name: &str,
        actual: &str,
        mode: Mode,
    ) -> Result<Option<Diff>, GoldenError> {
        let path = self.golden_path(name);
        match mode {
            Mode::Record => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|e| GoldenError::Io {
                        path: path.clone(),
                        message: e.to_string(),
                    })?;
                }
                fs::write(&path, actual).map_err(|e| GoldenError::Io {
                    path: path.clone(),
                    message: e.to_string(),
                })?;
                Ok(None)
            }
            Mode::Verify => {
                let expected = fs::read_to_string(&path).map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        GoldenError::MissingGolden { path: path.clone() }
                    } else {
                        GoldenError::Io {
                            path: path.clone(),
                            message: e.to_string(),
                        }
                    }
                })?;
                // The leading `backend:` line is provenance, not content:
                // strip it from both sides so a golden recorded from the
                // C++ reference verifies against the Rust backend (and
                // vice versa); everything else is compared byte-for-byte.
                let expected = strip_backend_line(&expected);
                let actual = strip_backend_line(actual);
                if expected == actual {
                    Ok(None)
                } else {
                    Ok(Some(Diff::between(&expected, &actual)))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qgis-oracle-goldens-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn mode_parse_values() {
        assert_eq!(Mode::parse("1"), Some(Mode::Record));
        assert_eq!(Mode::parse("true"), Some(Mode::Record));
        assert_eq!(Mode::parse("YES"), Some(Mode::Record));
        assert_eq!(Mode::parse("on"), Some(Mode::Record));
        assert_eq!(Mode::parse("0"), Some(Mode::Verify));
        assert_eq!(Mode::parse("false"), Some(Mode::Verify));
        assert_eq!(Mode::parse("off"), Some(Mode::Verify));
        assert_eq!(Mode::parse(""), Some(Mode::Verify));
        assert_eq!(Mode::parse("bogus"), None);
    }

    #[test]
    fn mode_from_env_record_and_verify() {
        let _guard = crate::env_lock().lock().unwrap();
        std::env::set_var("ORACLE_UPDATE_GOLDENS", "1");
        assert_eq!(Mode::from_env(), Mode::Record);
        std::env::set_var("ORACLE_UPDATE_GOLDENS", "0");
        assert_eq!(Mode::from_env(), Mode::Verify);
        std::env::remove_var("ORACLE_UPDATE_GOLDENS");
        assert_eq!(Mode::from_env(), Mode::Verify);
    }

    #[test]
    fn golden_path_appends_expected_suffix() {
        let store = GoldenStore::new("/tmp/somewhere");
        assert_eq!(
            store.golden_path("trivial"),
            PathBuf::from("/tmp/somewhere/trivial.expected")
        );
    }

    #[test]
    fn verify_match_returns_none() {
        let dir = temp_dir("verify-match");
        let store = GoldenStore::new(&dir);
        fs::write(dir.join("trivial.expected"), "same\n").unwrap();
        let result = store.compare("trivial", "same\n", Mode::Verify).unwrap();
        assert!(result.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_mismatch_returns_diff() {
        let dir = temp_dir("verify-mismatch");
        let store = GoldenStore::new(&dir);
        fs::write(dir.join("trivial.expected"), "expected line\n").unwrap();
        let diff = store
            .compare("trivial", "actual line\n", Mode::Verify)
            .unwrap()
            .expect("mismatch should produce a diff");
        assert!(!diff.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_missing_golden_is_an_error() {
        let dir = temp_dir("verify-missing");
        let store = GoldenStore::new(&dir);
        let err = store.compare("trivial", "x\n", Mode::Verify).unwrap_err();
        assert_eq!(
            err,
            GoldenError::MissingGolden {
                path: dir.join("trivial.expected")
            }
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_writes_golden_and_creates_directories() {
        let dir = temp_dir("record");
        let store = GoldenStore::new(dir.join("nested").join("goldens"));
        store
            .compare("trivial", "recorded output\n", Mode::Record)
            .unwrap();
        let written = fs::read_to_string(dir.join("nested/goldens/trivial.expected")).unwrap();
        assert_eq!(written, "recorded output\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_then_verify_round_trips() {
        let dir = temp_dir("roundtrip");
        let store = GoldenStore::new(&dir);
        let content = "backend: rust/0.1.0\nfixture: trivial.txt\n";
        store.compare("trivial", content, Mode::Record).unwrap();
        assert!(store.compare("trivial", content, Mode::Verify).unwrap().is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn from_env_honors_override() {
        let _guard = crate::env_lock().lock().unwrap();
        let dir = temp_dir("from-env");
        std::env::set_var("QGIS_ORACLE_GOLDEN_DIR", &dir);
        let store = GoldenStore::from_env();
        assert_eq!(store.root, dir);
        std::env::remove_var("QGIS_ORACLE_GOLDEN_DIR");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn from_env_falls_back_on_empty_and_whitespace_override() {
        let _guard = crate::env_lock().lock().unwrap();
        let default_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("goldens")
            .join("trivial.expected");
        for value in ["", "   "] {
            std::env::set_var("QGIS_ORACLE_GOLDEN_DIR", value);
            let store = GoldenStore::from_env();
            assert_eq!(
                store.golden_path("trivial"),
                default_path,
                "QGIS_ORACLE_GOLDEN_DIR set to {value:?} must fall back to the default goldens dir"
            );
        }
        std::env::remove_var("QGIS_ORACLE_GOLDEN_DIR");
    }

    #[test]
    fn verify_ignores_backend_self_identification_line() {
        let dir = temp_dir("verify-backend-line");
        let store = GoldenStore::new(&dir);
        // Golden recorded from the C++ reference backend...
        fs::write(
            dir.join("trivial.expected"),
            "backend: cpp/0.1.0\nfixture: trivial.txt\nbytes: 2\nlines: 1\n---\nx\n",
        )
        .unwrap();
        // ...verifies against the Rust backend's output.
        let result = store
            .compare(
                "trivial",
                "backend: rust/0.1.0\nfixture: trivial.txt\nbytes: 2\nlines: 1\n---\nx\n",
                Mode::Verify,
            )
            .unwrap();
        assert!(result.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_still_captures_drift_next_to_backend_line() {
        let dir = temp_dir("verify-backend-drift");
        let store = GoldenStore::new(&dir);
        fs::write(
            dir.join("trivial.expected"),
            "backend: cpp/0.1.0\nfixture: trivial.txt\nbytes: 2\nlines: 1\n---\nx\n",
        )
        .unwrap();
        let diff = store
            .compare(
                "trivial",
                "backend: rust/0.1.0\nfixture: trivial.txt\nbytes: 2\nlines: 1\n---\ny\n",
                Mode::Verify,
            )
            .unwrap()
            .expect("body drift must be caught");
        assert!(!diff.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn error_display_is_human_readable() {
        let err = GoldenError::MissingGolden {
            path: PathBuf::from("/tmp/x/trivial.expected"),
        };
        assert!(err.to_string().contains("/tmp/x/trivial.expected"));
    }

    #[test]
    fn error_is_std_error() {
        let err = GoldenError::MissingGolden {
            path: PathBuf::from("/tmp/x/y.expected"),
        };
        let _boxed: Box<dyn std::error::Error> = Box::new(err);
    }
}
