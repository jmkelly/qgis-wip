//! Captured output from a backend run.

/// Everything captured from one backend invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendOutput {
    /// Standard output, UTF-8 lossy-decoded.
    pub stdout: String,
    /// Standard error, UTF-8 lossy-decoded.
    pub stderr: String,
    /// Exit code, when the process exited normally.
    pub exit_code: Option<i32>,
}

/// Normalizes backend stdout for comparison.
///
/// The leading `backend: <name>/<version>` line is provenance (which side
/// produced the output), not content: it is replaced with a constant
/// marker. Everything after the first newline — `fixture:`, `bytes:`,
/// `lines:`, `---`, and the body — is preserved byte-for-byte, so real
/// drift still fails comparisons. Input without a leading
/// `backend: ` line (including empty input) is returned unchanged.
pub fn strip_backend_line(stdout: &str) -> String {
    if let Some(nl) = stdout.find('\n') {
        if stdout.starts_with("backend: ") {
            let mut out = String::from("backend: _\n");
            out.push_str(&stdout[nl + 1..]);
            return out;
        }
    }
    stdout.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_leading_backend_line_with_marker() {
        let out = strip_backend_line("backend: rust/0.1.0\nfixture: a.txt\nbytes: 1\n");
        assert_eq!(out, "backend: _\nfixture: a.txt\nbytes: 1\n");
    }

    #[test]
    fn preserves_trailing_newline_and_body_bytes() {
        let out = strip_backend_line("backend: cpp/0.1.0\n---\nbody line\n");
        assert_eq!(out, "backend: _\n---\nbody line\n");
    }

    #[test]
    fn leaves_output_without_backend_line_unchanged() {
        assert_eq!(strip_backend_line("hello\nworld\n"), "hello\nworld\n");
        assert_eq!(strip_backend_line(""), "");
    }

    #[test]
    fn backend_line_without_newline_is_left_alone() {
        // Malformed output (no trailing newline) keeps its bytes so the
        // comparison can still see the difference.
        assert_eq!(strip_backend_line("backend: cpp/0.1.0"), "backend: cpp/0.1.0");
    }

    #[test]
    fn rust_and_cpp_output_normalize_equal() {
        let rust = "backend: rust/0.1.0\nfixture: t.txt\nbytes: 2\nlines: 1\n---\nx\n";
        let cpp = "backend: cpp/0.1.0\nfixture: t.txt\nbytes: 2\nlines: 1\n---\nx\n";
        assert_eq!(strip_backend_line(rust), strip_backend_line(cpp));
    }
}
