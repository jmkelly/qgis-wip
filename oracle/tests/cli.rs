//! End-to-end tests for the `qgis-oracle` CLI binary.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::Mutex;

/// Serializes tests that mutate process-wide environment variables.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn oracle() -> Command {
    Command::new(env!("CARGO_BIN_EXE_qgis-oracle"))
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qgis-oracle-it-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/trivial.txt")
}

fn run(command: &mut Command) -> Output {
    command.output().expect("failed to run qgis-oracle")
}

#[test]
fn verify_against_committed_golden_succeeds() {
    let _guard = ENV_LOCK.lock().unwrap();
    let output = run(oracle().arg(fixture_path()));
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("OK"), "stdout: {stdout}");
}

#[test]
fn verify_mismatch_exits_nonzero_and_prints_unified_diff() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = temp_dir("mismatch");
    fs::write(dir.join("trivial.expected"), "backend: rust/0.1.0\nwrong\n").unwrap();
    let output = run(
        oracle()
            .arg(fixture_path())
            .env("QGIS_ORACLE_GOLDEN_DIR", &dir),
    );
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MISMATCH"), "stdout: {stdout}");
    assert!(stdout.contains("--- "), "stdout: {stdout}");
    assert!(stdout.contains("+++ actual"), "stdout: {stdout}");
    assert!(stdout.contains("-wrong"), "stdout: {stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn record_mode_writes_golden_then_verify_passes() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = temp_dir("record");
    let output = run(
        oracle()
            .arg(fixture_path())
            .env("QGIS_ORACLE_GOLDEN_DIR", &dir)
            .env("ORACLE_UPDATE_GOLDENS", "1"),
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let golden = fs::read_to_string(dir.join("trivial.expected")).unwrap();
    assert!(
        golden.contains("backend: rust/0.1.0"),
        "golden: {golden}"
    );
    let output = run(
        oracle()
            .arg(fixture_path())
            .env("QGIS_ORACLE_GOLDEN_DIR", &dir),
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn diff_reports_cpp_backend_not_built() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::remove_var("QGIS_CPP_BINARY");
    let output = run(
        oracle()
            .arg("diff")
            .arg(fixture_path())
            .args(["--backend-a", "rust", "--backend-b", "cpp"]),
    );
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("C++ backend not built yet"),
        "stderr: {stderr}"
    );
}

#[test]
fn run_subcommand_prints_backend_stdout() {
    let _guard = ENV_LOCK.lock().unwrap();
    let output = run(oracle().arg("run").arg(fixture_path()));
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend: rust/0.1.0"), "stdout: {stdout}");
}

#[test]
fn help_exits_zero() {
    let _guard = ENV_LOCK.lock().unwrap();
    let output = run(oracle().arg("--help"));
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("USAGE:"), "stdout: {stdout}");
}
