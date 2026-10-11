//! Command-line interface for the oracle.
//!
//! ```text
//! qgis-oracle <fixture> [--backend <name>]     verify against the golden
//! qgis-oracle run <fixture> [--backend <name>] run a backend, print stdout
//! qgis-oracle record <fixture> [--backend <name>]  (re-)write the golden
//! qgis-oracle diff <fixture> --backend-a <a> --backend-b <b>
//! ```
//!
//! Exit codes: 0 = match/success, 1 = outputs differ, 2 = usage or
//! backend error.

use std::path::{Path, PathBuf};

use crate::backend::BackendKind;
use crate::diff::Diff;
use crate::fixture::Fixture;
use crate::goldens::{GoldenStore, Mode};
use crate::output::{strip_backend_line, BackendOutput};

/// Exit code when outputs differ.
pub const EXIT_MISMATCH: i32 = 1;
/// Exit code for usage or backend errors.
pub const EXIT_ERROR: i32 = 2;

/// Full usage text (also shown by `--help` and on argument errors).
pub const USAGE: &str = "\
qgis-oracle — run fixtures through C++/Rust backends and diff outputs

USAGE:
  qgis-oracle <fixture> [--backend <name>]          Verify backend output against the golden
  qgis-oracle run <fixture> [--backend <name>]      Run a backend and print its stdout
  qgis-oracle record <fixture> [--backend <name>]   (Re-)write the golden for a fixture
  qgis-oracle diff <fixture> --backend-a <name> --backend-b <name>
  qgis-oracle --help

OPTIONS:
  --backend <name>    Backend for verify/run/record (default: rust)
  --backend-a <name>  First backend for diff
  --backend-b <name>  Second backend for diff
  -h, --help         Show this help

ENV:
  ORACLE_UPDATE_GOLDENS=1  verify re-records goldens instead of comparing
  QGIS_ORACLE_GOLDEN_DIR    Override the golden store directory
  QGIS_CPP_BINARY          Path to a QGIS C++ binary (enables the cpp backend)

BACKENDS:
  rust  Pure-Rust implementation under test (deterministic placeholder at E0)
  cpp   Shells out to $QGIS_CPP_BINARY; fails with a clear message when unset
";

/// Subcommands that operate on one fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Subcommand {
    Verify,
    Record,
    Run,
    Diff,
}

/// A parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Verify {
        fixture: PathBuf,
        backend: BackendKind,
    },
    Record {
        fixture: PathBuf,
        backend: BackendKind,
    },
    Run {
        fixture: PathBuf,
        backend: BackendKind,
    },
    Diff {
        fixture: PathBuf,
        backend_a: BackendKind,
        backend_b: BackendKind,
    },
    Help,
}

/// A parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    pub command: Command,
}

/// Parses arguments into a [`Cli`].
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Cli, String> {
    let mut positional: Vec<String> = Vec::new();
    let mut backend = BackendKind::Rust;
    let mut backend_a: Option<BackendKind> = None;
    let mut backend_b: Option<BackendKind> = None;
    let mut subcommand: Option<Subcommand> = None;

    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        let s = arg.as_str();
        match s {
            "-h" | "--help" => return Ok(Cli { command: Command::Help }),
            "--backend" => backend = take_value(&mut it, &arg)?,
            "--backend-a" => backend_a = Some(take_value(&mut it, &arg)?),
            "--backend-b" => backend_b = Some(take_value(&mut it, &arg)?),
            s if let Some(v) = s.strip_prefix("--backend=") => backend = parse_value(v)?,
            s if let Some(v) = s.strip_prefix("--backend-a=") => backend_a = Some(parse_value(v)?),
            s if let Some(v) = s.strip_prefix("--backend-b=") => backend_b = Some(parse_value(v)?),
            "verify" | "record" | "run" | "diff" => {
                if subcommand.is_some() {
                    return Err(format!("unexpected subcommand `{s}`"));
                }
                subcommand = Some(match s {
                    "verify" => Subcommand::Verify,
                    "record" => Subcommand::Record,
                    "run" => Subcommand::Run,
                    _ => Subcommand::Diff,
                });
            }
            s if s.starts_with('-') => return Err(format!("unknown option `{s}`")),
            s => positional.push(s.to_string()),
        }
    }

    let command = match subcommand {
        None | Some(Subcommand::Verify) => Command::Verify {
            fixture: take_positional(&positional)?,
            backend,
        },
        Some(Subcommand::Record) => Command::Record {
            fixture: take_positional(&positional)?,
            backend,
        },
        Some(Subcommand::Run) => Command::Run {
            fixture: take_positional(&positional)?,
            backend,
        },
        Some(Subcommand::Diff) => Command::Diff {
            fixture: take_positional(&positional)?,
            backend_a: backend_a.ok_or("diff requires --backend-a <name>")?,
            backend_b: backend_b.ok_or("diff requires --backend-b <name>")?,
        },
    };

    if positional.len() > 1 {
        return Err(format!("unexpected argument `{}`", positional[1]));
    }
    Ok(Cli { command })
}

/// Executes a parsed command line, returning the process exit code.
pub fn run(cli: Cli) -> Result<i32, String> {
    match cli.command {
        Command::Help => {
            print!("{USAGE}");
            Ok(0)
        }
        Command::Run { fixture, backend } => {
            let fixture = load_fixture(&fixture)?;
            let output = run_backend(backend, &fixture)?;
            print!("{}", output.stdout);
            Ok(0)
        }
        Command::Record { fixture, backend } => {
            let fixture = load_fixture(&fixture)?;
            let output = run_backend(backend, &fixture)?;
            let store = GoldenStore::from_env();
            let name = fixture.golden_name();
            let path = store.golden_path(name);
            store
                .compare(name, &output.stdout, Mode::Record)
                .map_err(|e| format!("cannot record golden: {e}"))?;
            println!("recorded {}", path.display());
            Ok(0)
        }
        Command::Verify { fixture, backend } => {
            let fixture = load_fixture(&fixture)?;
            let output = run_backend(backend, &fixture)?;
            let store = GoldenStore::from_env();
            let name = fixture.golden_name();
            let path = store.golden_path(name);
            if Mode::from_env() == Mode::Record {
                store
                    .compare(name, &output.stdout, Mode::Record)
                    .map_err(|e| format!("cannot record golden: {e}"))?;
                println!("recorded {}", path.display());
                return Ok(0);
            }
            match store.compare(name, &output.stdout, Mode::Verify) {
                Ok(None) => {
                    println!("OK: {} matches {backend} backend", path.display());
                    Ok(0)
                }
                Ok(Some(diff)) => {
                    println!("MISMATCH: {} vs {backend} backend", path.display());
                    print!("{}", diff.render(&path.display().to_string(), "actual"));
                    Ok(EXIT_MISMATCH)
                }
                Err(e) => Err(format!("cannot verify golden: {e}")),
            }
        }
        Command::Diff {
            fixture,
            backend_a,
            backend_b,
        } => {
            let fixture = load_fixture(&fixture)?;
            let a = run_backend(backend_a, &fixture)?;
            let b = run_backend(backend_b, &fixture)?;
            // The `backend:` line is provenance, not content: strip it
            // from both sides so backend self-identification never shows
            // up as drift; everything else is compared byte-for-byte.
            let diff = Diff::between(
                &strip_backend_line(&a.stdout),
                &strip_backend_line(&b.stdout),
            );
            if diff.is_empty() {
                println!(
                    "IDENTICAL: {} and {} agree on {}",
                    backend_a.name(),
                    backend_b.name(),
                    fixture.path().display()
                );
                Ok(0)
            } else {
                print!("{}", diff.render(backend_a.name(), backend_b.name()));
                Ok(EXIT_MISMATCH)
            }
        }
    }
}

fn take_value(
    it: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<BackendKind, String> {
    let raw = it
        .next()
        .ok_or_else(|| format!("{flag} requires a value"))?;
    parse_value(&raw)
}

fn parse_value(raw: &str) -> Result<BackendKind, String> {
    BackendKind::parse(raw)
        .ok_or_else(|| format!("unknown backend `{raw}` (expected rust or cpp)"))
}

fn take_positional(positional: &[String]) -> Result<PathBuf, String> {
    positional
        .first()
        .map(PathBuf::from)
        .ok_or_else(|| "missing <fixture> argument".to_string())
}

fn load_fixture(path: &Path) -> Result<Fixture, String> {
    Fixture::load(path).map_err(|e| format!("cannot load fixture {}: {e}", path.display()))
}

fn run_backend(backend: BackendKind, fixture: &Fixture) -> Result<BackendOutput, String> {
    backend
        .run(fixture)
        .map_err(|e| format!("backend `{}` failed: {e}", backend.name()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qgis-oracle-cli-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_bare_fixture_is_verify_with_rust_backend() {
        let cli = parse_args(["fixtures/trivial.txt".to_string()]).unwrap();
        assert_eq!(
            cli.command,
            Command::Verify {
                fixture: PathBuf::from("fixtures/trivial.txt"),
                backend: BackendKind::Rust,
            }
        );
    }

    #[test]
    fn parse_verify_with_backend_flag() {
        let cli = parse_args([
            "verify".to_string(),
            "f.txt".to_string(),
            "--backend".to_string(),
            "cpp".to_string(),
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Command::Verify {
                fixture: PathBuf::from("f.txt"),
                backend: BackendKind::Cpp,
            }
        );
    }

    #[test]
    fn parse_run_and_record_subcommands() {
        let cli = parse_args(["run".to_string(), "f.txt".to_string()]).unwrap();
        assert_eq!(
            cli.command,
            Command::Run {
                fixture: PathBuf::from("f.txt"),
                backend: BackendKind::Rust,
            }
        );
        let cli = parse_args(["record".to_string(), "f.txt".to_string()]).unwrap();
        assert_eq!(
            cli.command,
            Command::Record {
                fixture: PathBuf::from("f.txt"),
                backend: BackendKind::Rust,
            }
        );
    }

    #[test]
    fn parse_diff_requires_both_backends() {
        assert!(parse_args(["diff".to_string(), "f.txt".to_string()]).is_err());
        assert!(parse_args([
            "diff".to_string(),
            "f.txt".to_string(),
            "--backend-a".to_string(),
            "rust".to_string(),
        ])
        .is_err());
        let cli = parse_args([
            "diff".to_string(),
            "f.txt".to_string(),
            "--backend-a".to_string(),
            "rust".to_string(),
            "--backend-b=cpp".to_string(),
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Command::Diff {
                fixture: PathBuf::from("f.txt"),
                backend_a: BackendKind::Rust,
                backend_b: BackendKind::Cpp,
            }
        );
    }

    #[test]
    fn parse_help_short_and_long() {
        assert_eq!(parse_args(["-h".to_string()]).unwrap().command, Command::Help);
        assert_eq!(
            parse_args(["--help".to_string()]).unwrap().command,
            Command::Help
        );
    }

    #[test]
    fn parse_rejects_unknown_option_and_backend() {
        assert!(parse_args(["--frobnicate".to_string()]).is_err());
        assert!(parse_args([
            "f.txt".to_string(),
            "--backend".to_string(),
            "java".to_string(),
        ])
        .is_err());
    }

    #[test]
    fn parse_rejects_missing_fixture_and_extra_positional() {
        assert!(parse_args(Vec::new()).is_err());
        assert!(parse_args(["a.txt".to_string(), "b.txt".to_string()]).is_err());
    }

    #[test]
    fn run_verify_match_returns_zero() {
        let _guard = crate::env_lock().lock().unwrap();
        let dir = temp_dir("verify-ok");
        let fixture_path = dir.join("trivial.txt");
        fs::write(&fixture_path, "hello oracle\nsecond line\n").unwrap();
        fs::write(
            dir.join("trivial.expected"),
            "backend: rust/0.1.0\nfixture: trivial.txt\nbytes: 25\nlines: 2\n---\nhello oracle\nsecond line\n",
        )
        .unwrap();
        std::env::set_var("QGIS_ORACLE_GOLDEN_DIR", &dir);
        let cli = Cli {
            command: Command::Verify {
                fixture: fixture_path,
                backend: BackendKind::Rust,
            },
        };
        assert_eq!(run(cli).unwrap(), 0);
        std::env::remove_var("QGIS_ORACLE_GOLDEN_DIR");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_verify_mismatch_returns_one() {
        let _guard = crate::env_lock().lock().unwrap();
        let dir = temp_dir("verify-bad");
        let fixture_path = dir.join("trivial.txt");
        fs::write(&fixture_path, "hello\n").unwrap();
        fs::write(dir.join("trivial.expected"), "stale golden\n").unwrap();
        std::env::set_var("QGIS_ORACLE_GOLDEN_DIR", &dir);
        let cli = Cli {
            command: Command::Verify {
                fixture: fixture_path,
                backend: BackendKind::Rust,
            },
        };
        assert_eq!(run(cli).unwrap(), EXIT_MISMATCH);
        std::env::remove_var("QGIS_ORACLE_GOLDEN_DIR");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_record_writes_golden() {
        let _guard = crate::env_lock().lock().unwrap();
        let dir = temp_dir("record");
        let fixture_path = dir.join("trivial.txt");
        fs::write(&fixture_path, "hello\n").unwrap();
        std::env::set_var("QGIS_ORACLE_GOLDEN_DIR", &dir);
        let cli = Cli {
            command: Command::Record {
                fixture: fixture_path.clone(),
                backend: BackendKind::Rust,
            },
        };
        assert_eq!(run(cli).unwrap(), 0);
        let golden = fs::read_to_string(dir.join("trivial.expected")).unwrap();
        assert!(golden.contains("backend: rust/0.1.0"), "golden: {golden}");
        std::env::remove_var("QGIS_ORACLE_GOLDEN_DIR");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_verify_missing_golden_is_error() {
        let _guard = crate::env_lock().lock().unwrap();
        let dir = temp_dir("verify-missing");
        let fixture_path = dir.join("trivial.txt");
        fs::write(&fixture_path, "hello\n").unwrap();
        std::env::set_var("QGIS_ORACLE_GOLDEN_DIR", &dir);
        let cli = Cli {
            command: Command::Verify {
                fixture: fixture_path,
                backend: BackendKind::Rust,
            },
        };
        assert!(run(cli).is_err());
        std::env::remove_var("QGIS_ORACLE_GOLDEN_DIR");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_diff_identical_backends_returns_zero() {
        let _guard = crate::env_lock().lock().unwrap();
        let dir = temp_dir("diff-identical");
        let fixture_path = dir.join("trivial.txt");
        fs::write(&fixture_path, "hello\n").unwrap();
        let cli = Cli {
            command: Command::Diff {
                fixture: fixture_path,
                backend_a: BackendKind::Rust,
                backend_b: BackendKind::Rust,
            },
        };
        assert_eq!(run(cli).unwrap(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_diff_ignores_backend_self_identification() {
        use std::os::unix::fs::PermissionsExt;

        let _guard = crate::env_lock().lock().unwrap();
        let dir = temp_dir("diff-backend-line");
        // Fake C++ binary: same canonical output as the Rust backend,
        // differing only in the `backend:` self-identification line.
        let script = dir.join("fake-cpp.sh");
        fs::write(
            &script,
            "#!/bin/sh\nprintf 'backend: cpp/0.1.0\\nfixture: trivial.txt\\nbytes: 6\\nlines: 1\\n---\\nhello\\n' \"$1\"\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("QGIS_CPP_BINARY", &script);

        let fixture_path = dir.join("trivial.txt");
        fs::write(&fixture_path, "hello\n").unwrap();
        let cli = Cli {
            command: Command::Diff {
                fixture: fixture_path,
                backend_a: BackendKind::Rust,
                backend_b: BackendKind::Cpp,
            },
        };
        assert_eq!(run(cli).unwrap(), 0);
        std::env::remove_var("QGIS_CPP_BINARY");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_diff_still_captures_real_drift() {
        use std::os::unix::fs::PermissionsExt;

        let _guard = crate::env_lock().lock().unwrap();
        let dir = temp_dir("diff-backend-drift");
        let script = dir.join("fake-cpp.sh");
        fs::write(
            &script,
            "#!/bin/sh\nprintf 'backend: cpp/0.1.0\\nfixture: trivial.txt\\nbytes: 6\\nlines: 1\\n---\\nHELLO\\n' \"$1\"\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("QGIS_CPP_BINARY", &script);

        let fixture_path = dir.join("trivial.txt");
        fs::write(&fixture_path, "hello\n").unwrap();
        let cli = Cli {
            command: Command::Diff {
                fixture: fixture_path,
                backend_a: BackendKind::Rust,
                backend_b: BackendKind::Cpp,
            },
        };
        assert_eq!(run(cli).unwrap(), EXIT_MISMATCH);
        std::env::remove_var("QGIS_CPP_BINARY");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_verify_ignores_backend_self_identification() {
        let _guard = crate::env_lock().lock().unwrap();
        let dir = temp_dir("verify-backend-line");
        let fixture_path = dir.join("trivial.txt");
        fs::write(&fixture_path, "hello\n").unwrap();
        // Golden recorded from the C++ reference backend...
        fs::write(
            dir.join("trivial.expected"),
            "backend: cpp/0.1.0\nfixture: trivial.txt\nbytes: 6\nlines: 1\n---\nhello\n",
        )
        .unwrap();
        std::env::set_var("QGIS_ORACLE_GOLDEN_DIR", &dir);
        let cli = Cli {
            command: Command::Verify {
                fixture: fixture_path,
                backend: BackendKind::Rust,
            },
        };
        // ...verifies against the Rust backend's output.
        assert_eq!(run(cli).unwrap(), 0);
        std::env::remove_var("QGIS_ORACLE_GOLDEN_DIR");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_diff_cpp_not_built_is_error() {
        let _guard = crate::env_lock().lock().unwrap();
        std::env::remove_var("QGIS_CPP_BINARY");
        let dir = temp_dir("diff-cpp");
        let fixture_path = dir.join("trivial.txt");
        fs::write(&fixture_path, "hello\n").unwrap();
        let cli = Cli {
            command: Command::Diff {
                fixture: fixture_path,
                backend_a: BackendKind::Rust,
                backend_b: BackendKind::Cpp,
            },
        };
        let err = run(cli).unwrap_err();
        assert!(err.contains("C++ backend not built yet"), "error: {err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_missing_fixture_is_error() {
        let cli = Cli {
            command: Command::Run {
                fixture: PathBuf::from("/nonexistent/missing.txt"),
                backend: BackendKind::Rust,
            },
        };
        assert!(run(cli).is_err());
    }
}
