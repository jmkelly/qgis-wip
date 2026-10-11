//! Fixture inputs for the oracle.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// A fixture file loaded for a backend run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixture {
    path: PathBuf,
    content: String,
}

/// Errors that can occur while loading a fixture.
#[derive(Debug)]
pub enum FixtureError {
    /// The file could not be read.
    Io { path: PathBuf, message: String },
    /// The file is not valid UTF-8.
    NotUtf8 { path: PathBuf },
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FixtureError::Io { path, message } => {
                write!(f, "cannot read fixture {}: {message}", path.display())
            }
            FixtureError::NotUtf8 { path } => {
                write!(f, "fixture {} is not valid UTF-8", path.display())
            }
        }
    }
}

impl std::error::Error for FixtureError {}

impl Fixture {
    /// Loads a fixture from disk. Content must be valid UTF-8.
    pub fn load(path: impl AsRef<Path>) -> Result<Fixture, FixtureError> {
        let path = path.as_ref().to_path_buf();
        let bytes = fs::read(&path).map_err(|e| FixtureError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let content = String::from_utf8(bytes).map_err(|_| FixtureError::NotUtf8 {
            path: path.clone(),
        })?;
        Ok(Fixture { path, content })
    }

    /// The fixture's path on disk.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The fixture's file name. Used in backend output so goldens stay
    /// independent of the caller's working directory.
    pub fn name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("fixture")
    }

    /// The golden name for this fixture: its file stem.
    pub fn golden_name(&self) -> &str {
        self.path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("fixture")
    }

    /// The fixture's raw content.
    pub fn content(&self) -> &str {
        &self.content
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qgis-oracle-fixture-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_reads_content_and_derives_names() {
        let dir = temp_dir("load");
        let path = dir.join("trivial.txt");
        fs::write(&path, "hello\n").unwrap();
        let fixture = Fixture::load(&path).unwrap();
        assert_eq!(fixture.content(), "hello\n");
        assert_eq!(fixture.name(), "trivial.txt");
        assert_eq!(fixture.golden_name(), "trivial");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_is_io_error() {
        let err = Fixture::load("/nonexistent/definitely-missing.txt").unwrap_err();
        match err {
            FixtureError::Io { .. } => {}
            other => panic!("expected Io, got {other}"),
        }
    }

    #[test]
    fn load_non_utf8_is_rejected() {
        let dir = temp_dir("nonutf8");
        let path = dir.join("binary.bin");
        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        let err = Fixture::load(&path).unwrap_err();
        match err {
            FixtureError::NotUtf8 { .. } => {}
            other => panic!("expected NotUtf8, got {other}"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn display_is_human_readable() {
        let err = Fixture::load("/nonexistent/x.txt").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("/nonexistent/x.txt"), "message: {msg}");
        assert!(msg.contains("cannot read"), "message: {msg}");
    }

    #[test]
    fn io_error_is_std_error() {
        let err = Fixture::load("/nonexistent/x.txt").unwrap_err();
        let _boxed: Box<dyn std::error::Error> = Box::new(err);
    }
}
