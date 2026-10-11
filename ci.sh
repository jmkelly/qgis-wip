#!/usr/bin/env bash
# CI gate script for the QGIS Rust port.
# Runs all gates locally or in CI. Exits non-zero on first failure.
#
# Usage:
#   ./ci.sh [--help] [-p <crate>] [--offline] [--nextest] [--geiger]
#
# Options:
#   -p <crate>   Scope all gates to <crate> and its workspace dependents only.
#                Without -p, all gates run on the full workspace.
#   --offline    Pass --offline to all cargo commands (useful for CI caching).
#   --nextest    Use 'cargo nextest run' instead of 'cargo test' when available.
#                Falls back to 'cargo test' with a warning if nextest is missing.
#   --geiger     Also run cargo-geiger (informational, not a hard gate).
#   --help, -h   Show this help message and exit.
set -euo pipefail

cd "$(dirname "$0")"

# --- Argument parsing ---
CRATE=""
OFFLINE=0
NEXTEST=0
GEIGER=0

usage() {
    sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
    case "$1" in
        -p)
            if [ $# -lt 2 ]; then
                echo "error: -p requires a crate name" >&2
                exit 2
            fi
            CRATE="$2"
            shift 2
            ;;
        --offline)
            OFFLINE=1
            shift
            ;;
        --nextest)
            NEXTEST=1
            shift
            ;;
        --geiger)
            GEIGER=1
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

# Ensure cargo is on PATH (rustup installs to ~/.cargo/bin by default).
if ! command -v cargo &>/dev/null; then
    if [ -f "$HOME/.cargo/env" ]; then
        # shellcheck disable=SC1091
        source "$HOME/.cargo/env"
    else
        echo "error: cargo not found on PATH and ~/.cargo/env missing" >&2
        exit 1
    fi
fi

# --- Build cargo flags ---
OFFLINE_FLAG=()
if [ "$OFFLINE" -eq 1 ]; then
    OFFLINE_FLAG=(--offline)
fi

# --- Determine test command ---
if [ "$NEXTEST" -eq 1 ]; then
    if cargo nextest --version &>/dev/null; then
        TEST_CMD=(cargo nextest run)
        TEST_LABEL="cargo nextest run"
    else
        echo "warning: cargo-nextest not installed; falling back to cargo test" >&2
        TEST_CMD=(cargo test)
        TEST_LABEL="cargo test"
    fi
else
    TEST_CMD=(cargo test)
    TEST_LABEL="cargo test"
fi

# --- Compute package scope ---
if [ -n "$CRATE" ]; then
    # Find the crate and its transitive dependents using cargo tree -i
    SCOPE_RAW=$(cargo tree --workspace -i "$CRATE" --prefix none 2>/dev/null || true)
    if [ -z "$SCOPE_RAW" ]; then
        echo "error: crate '$CRATE' not found in workspace" >&2
        exit 1
    fi
    # Extract package names (first field of each line)
    mapfile -t PKG_NAMES < <(echo "$SCOPE_RAW" | awk '{print $1}')
    # Build -p flags
    PKG_FLAGS=()
    for pkg in "${PKG_NAMES[@]}"; do
        PKG_FLAGS+=(-p "$pkg")
    done
    SCOPE_DESC="$CRATE + dependents (${#PKG_NAMES[@]} packages)"
else
    PKG_FLAGS=(--workspace)
    SCOPE_DESC="full workspace"
fi

echo "=== Rust CI gates ==="
echo "Toolchain: $(rustc --version)"
echo "Scope: $SCOPE_DESC"
echo "Test command: $TEST_LABEL"
echo ""

# --- Gate 1: tests ---
echo "--- [1/5] ${TEST_LABEL} ${PKG_FLAGS[*]} ---"
"${TEST_CMD[@]}" "${PKG_FLAGS[@]}" ${OFFLINE_FLAG[@]+"${OFFLINE_FLAG[@]}"}
echo ""

# --- Gate 2: clippy ---
echo "--- [2/5] cargo clippy ${PKG_FLAGS[*]} --all-targets -- -D warnings ---"
cargo clippy "${PKG_FLAGS[@]}" --all-targets -- -D warnings ${OFFLINE_FLAG[@]+"${OFFLINE_FLAG[@]}"}
echo ""

# --- Gate 3: bench compile ---
echo "--- [3/5] bench compile ---"
if [ -n "$CRATE" ]; then
    # Check if the crate has benches
    CRATE_DIR="crates/$CRATE"
    if [ -d "$CRATE_DIR/benches" ] || grep -q '\[\[bench\]\]' "$CRATE_DIR/Cargo.toml" 2>/dev/null; then
        echo "cargo bench -p $CRATE --no-run"
        cargo bench -p "$CRATE" --no-run ${OFFLINE_FLAG[@]+"${OFFLINE_FLAG[@]}"}
    else
        echo "crate '$CRATE' has no benches; skipping bench compile"
    fi
else
    echo "cargo bench -p qgis-bench --no-run"
    cargo bench -p qgis-bench --no-run ${OFFLINE_FLAG[@]+"${OFFLINE_FLAG[@]}"}
fi
echo ""

# --- Gate 4: oracle ---
# Rust-side smoke verify always runs; the live C++ diff runs only when
# QGIS_CPP_BINARY names an executable (bead qgis-uic.7).
echo "--- [4/5] oracle ---"
if [ -n "$CRATE" ] && [ "$CRATE" != "qgis-oracle" ]; then
    echo "crate '$CRATE' has no oracle integration; skipping oracle"
else
    echo "cargo run -p qgis-oracle -- crates/oracle/fixtures/trivial.txt"
    cargo run -p qgis-oracle -- crates/oracle/fixtures/trivial.txt ${OFFLINE_FLAG[@]+"${OFFLINE_FLAG[@]}"}
    echo ""

    if [ -n "${QGIS_CPP_BINARY:-}" ]; then
        if [ ! -x "$QGIS_CPP_BINARY" ]; then
            echo "error: QGIS_CPP_BINARY='$QGIS_CPP_BINARY' is not an executable" >&2
            exit 1
        fi
        echo "Live C++ oracle diff against $QGIS_CPP_BINARY:"
        for fixture in crates/oracle/fixtures/*; do
            [ -f "$fixture" ] || continue
            echo "cargo run -p qgis-oracle -- diff $fixture --backend-a rust --backend-b cpp"
            cargo run -p qgis-oracle -- diff "$fixture" --backend-a rust --backend-b cpp ${OFFLINE_FLAG[@]+"${OFFLINE_FLAG[@]}"}
        done
    else
        echo "warning: QGIS_CPP_BINARY not set; skipping live C++ oracle diff."
        echo "         Build rust/cpp_harness and set QGIS_CPP_BINARY to enable it (see crates/oracle/README.md)."
    fi
fi
echo ""

# --- Gate 5: cargo-geiger (optional) ---
if [ "$GEIGER" -eq 1 ]; then
    echo "--- [5/5] cargo geiger --workspace (informational) ---"
    if ! command -v cargo-geiger &>/dev/null; then
        echo "cargo-geiger not installed; skipping (run: cargo install cargo-geiger)"
    else
        cargo geiger --workspace || true
    fi
else
    echo "--- [5/5] cargo-geiger skipped (pass --geiger to enable) ---"
fi

echo ""
echo "=== All CI gates passed ==="
