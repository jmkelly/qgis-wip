//! C++ backend: runs the fixture through a QGIS C++ binary.
//!
//! At E0 there is no built QGIS binary to point at, so the backend
//! fails with a clear "not built yet" message unless the `QGIS_CPP_BINARY`
//! environment variable names an executable to shell out to.

use std::process::Command;

use crate::backend::BackendError;
use crate::fixture::Fixture;
use crate::output::BackendOutput;

/// C++-side runner.
pub struct CppBackend;

impl CppBackend {
    pub const NAME: &'static str = "cpp";

    /// Error message used when no C++ binary is configured.
    pub const NOT_BUILT: &str = "C++ backend not built yet; build rust/cpp_harness and set \
                                QGIS_CPP_BINARY to enable it (see crates/oracle/README.md)";

    /// Runs the fixture through the configured C++ binary.
    ///
    /// The binary is invoked as `<QGIS_CPP_BINARY> <fixture-path>` and its
    /// stdout/stderr/exit code are captured. No FFI is involved: this is a
    /// plain process spawn.
    pub fn run(fixture: &Fixture) -> Result<BackendOutput, BackendError> {
        let Ok(binary) = std::env::var("QGIS_CPP_BINARY") else {
            return Err(BackendError::NotBuilt(Self::NOT_BUILT.to_string()));
        };
        let output = Command::new(binary.trim())
            .arg(fixture.path())
            .output()
            .map_err(|e| BackendError::Spawn {
                backend: Self::NAME.to_string(),
                message: e.to_string(),
            })?;
        let stdout = String::from_utf8(output.stdout).map_err(|_| BackendError::InvalidUtf8 {
            backend: Self::NAME.to_string(),
        })?;
        let stderr = String::from_utf8(output.stderr).map_err(|_| BackendError::InvalidUtf8 {
            backend: Self::NAME.to_string(),
        })?;
        Ok(BackendOutput {
            stdout,
            stderr,
            exit_code: output.status.code(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Fixture;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qgis-oracle-cpp-{tag}-{}", std::process::id()));
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
    fn reports_not_built_without_binary() {
        let _guard = crate::env_lock().lock().unwrap();
        std::env::remove_var("QGIS_CPP_BINARY");
        let fixture = load("notbuilt", "in.txt", "x\n");
        let err = CppBackend::run(&fixture).unwrap_err();
        assert_eq!(
            err,
            BackendError::NotBuilt(CppBackend::NOT_BUILT.to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn shells_out_to_configured_binary() {
        use std::os::unix::fs::PermissionsExt;

        let _guard = crate::env_lock().lock().unwrap();
        let dir = temp_dir("shellout");
        let script = dir.join("fake-qgis.sh");
        fs::write(&script, "#!/bin/sh\ncat \"$1\"\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("QGIS_CPP_BINARY", &script);

        let fixture = load("shellout-fixture", "in.txt", "hello from fixture\n");
        let out = CppBackend::run(&fixture).unwrap();
        assert_eq!(out.stdout, "hello from fixture\n");
        assert_eq!(out.stderr, "");
        assert_eq!(out.exit_code, Some(0));

        std::env::remove_var("QGIS_CPP_BINARY");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn spawn_failure_is_reported() {
        let _guard = crate::env_lock().lock().unwrap();
        std::env::set_var("QGIS_CPP_BINARY", "/nonexistent/qgis-binary");
        let fixture = load("spawnfail", "in.txt", "x\n");
        let err = CppBackend::run(&fixture).unwrap_err();
        match err {
            BackendError::Spawn { backend, .. } => assert_eq!(backend, "cpp"),
            other => panic!("expected Spawn, got {other}"),
        }
        std::env::remove_var("QGIS_CPP_BINARY");
    }
}
