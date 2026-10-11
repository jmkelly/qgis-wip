//! Backend abstraction: named runners that execute a fixture.

use std::fmt;

use crate::cpp_backend::CppBackend;
use crate::fixture::Fixture;
use crate::output::BackendOutput;
use crate::rust_backend::RustBackend;

/// Errors produced while running a backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    /// The backend has no implementation wired up yet.
    NotBuilt(String),
    /// The backend process could not be spawned.
    Spawn { backend: String, message: String },
    /// The backend wrote invalid UTF-8 to stdout or stderr.
    InvalidUtf8 { backend: String },
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendError::NotBuilt(message) => write!(f, "{message}"),
            BackendError::Spawn { backend, message } => {
                write!(f, "backend `{backend}` could not be run: {message}")
            }
            BackendError::InvalidUtf8 { backend } => {
                write!(f, "backend `{backend}` produced invalid UTF-8 output")
            }
        }
    }
}

impl std::error::Error for BackendError {}

impl fmt::Display for BackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The backends the oracle knows how to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// The pure-Rust implementation under test.
    Rust,
    /// The C++ implementation, reached by shelling out.
    Cpp,
}

impl BackendKind {
    /// Parses a backend name (`rust`, `cpp`, `c++`).
    pub fn parse(value: &str) -> Option<BackendKind> {
        match value.trim().to_ascii_lowercase().as_str() {
            "rust" => Some(BackendKind::Rust),
            "cpp" | "c++" => Some(BackendKind::Cpp),
            _ => None,
        }
    }

    /// The backend's canonical name.
    pub fn name(&self) -> &'static str {
        match self {
            BackendKind::Rust => RustBackend::NAME,
            BackendKind::Cpp => CppBackend::NAME,
        }
    }

    /// Runs `fixture` through this backend and captures its output.
    pub fn run(&self, fixture: &Fixture) -> Result<BackendOutput, BackendError> {
        match self {
            BackendKind::Rust => RustBackend::run(fixture),
            BackendKind::Cpp => CppBackend::run(fixture),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_known_backends() {
        assert_eq!(BackendKind::parse("rust"), Some(BackendKind::Rust));
        assert_eq!(BackendKind::parse("cpp"), Some(BackendKind::Cpp));
        assert_eq!(BackendKind::parse("c++"), Some(BackendKind::Cpp));
        assert_eq!(BackendKind::parse("RUST"), Some(BackendKind::Rust));
    }

    #[test]
    fn parse_unknown_backend_is_none() {
        assert_eq!(BackendKind::parse("java"), None);
        assert_eq!(BackendKind::parse(""), None);
    }

    #[test]
    fn names_are_stable() {
        assert_eq!(BackendKind::Rust.name(), "rust");
        assert_eq!(BackendKind::Cpp.name(), "cpp");
    }

    #[test]
    fn error_display_is_human_readable() {
        let err = BackendError::NotBuilt("C++ backend not built yet".to_string());
        assert_eq!(err.to_string(), "C++ backend not built yet");
        let err = BackendError::Spawn {
            backend: "cpp".to_string(),
            message: "No such file or directory".to_string(),
        };
        assert!(err.to_string().contains("cpp"));
    }

    #[test]
    fn error_is_std_error() {
        let err = BackendError::NotBuilt("x".to_string());
        let _boxed: Box<dyn std::error::Error> = Box::new(err);
    }
}
