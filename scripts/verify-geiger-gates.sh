#!/usr/bin/env bash
# Acceptance / regression guard for bead qgis-6qa.
#
# ci.sh (Gate 5) and verify (cmd_all) run cargo geiger --workspace || true.
# cargo-geiger 0.14.0 rejects the virtual workspace manifest at the repo root
# ("manifest path ... is a virtual manifest"), and the || true swallows that
# error, so both gates report success while auditing nothing. The fix is a
# per-package loop over `cargo metadata` manifest paths.
#
# This guard extracts the live geiger gate block from each script, runs it
# with a stub cargo-geiger on PATH (logs its argv, exits 1 like real
# findings), and asserts the stub was invoked exactly once per workspace
# package, each time with --manifest-path <manifest>, never with
# --workspace, and that the block itself still exits 0 (findings advisory).
#
# Exit 0 when both gates audit every package; non-zero with the stub log
# when not.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0

# --- stub cargo-geiger: log argv, exit 1 (findings are advisory) ---
STUB_DIR="$(mktemp -d)"
STUB_LOG="$STUB_DIR/invocations.log"
cat > "$STUB_DIR/cargo-geiger" <<'STUB'
#!/usr/bin/env bash
echo "$@" >> "$STUB_LOG"
exit 1
STUB
chmod +x "$STUB_DIR/cargo-geiger"
export STUB_LOG

# cargo and cargo metadata must resolve; the stub shadows any real
# cargo-geiger so `command -v cargo-geiger` and `cargo geiger` both hit it.
export PATH="$STUB_DIR:$HOME/.cargo/bin:$PATH"

# --- expected audit set: every workspace package manifest ---
mapfile -t EXPECTED_MANIFESTS < <(
    cargo metadata --no-deps --format-version 1 | jq -r '.packages[].manifest_path' | sort
)
if [ "${#EXPECTED_MANIFESTS[@]}" -eq 0 ]; then
    echo "FAIL: cargo metadata returned no packages"
    exit 1
fi

# Run an extracted gate block with stub_log as the stub's log, then verify
# the stub saw exactly the expected per-package invocations.
check_gate() {
    local name="$1" block="$2" env_var="$3" log="$4"
    : > "$log"
    local rc=0
    ( export STUB_LOG="$log" "$env_var"=1; eval "$block" ) || rc=$?

    if [ "$rc" -ne 0 ]; then
        echo "FAIL [$name]: geiger gate exited $rc (must stay 0: findings are advisory, || true)"
        fail=1
    fi

    local actual
    actual=$(sed -n 's/^geiger --manifest-path //p' "$log" | sort)
    if [ -z "$actual" ]; then
        echo "FAIL [$name]: cargo-geiger was never invoked with --manifest-path; audited nothing"
        sed 's/^/    stub saw: /' "$log"
        fail=1
        return
    fi
    if [ "$actual" != "$(printf '%s\n' "${EXPECTED_MANIFESTS[@]}")" ]; then
        echo "FAIL [$name]: per-package audit set mismatch"
        diff <(printf '%s\n' "${EXPECTED_MANIFESTS[@]}") <(echo "$actual") | sed 's/^/    /'
        fail=1
        return
    fi
    if grep -q -- '--workspace' "$log"; then
        echo "FAIL [$name]: gate still invokes 'cargo geiger --workspace' (dead on virtual manifest)"
        fail=1
        return
    fi
    echo "ok [$name]: audited ${#EXPECTED_MANIFESTS[@]} packages via --manifest-path"
}

# --- extract the live gate blocks and check them ---
# ci.sh: Gate 5 sits between its marker comment and the Gate 6 marker.
CI_BLOCK="$(awk '/^# --- Gate 5: cargo-geiger/,/^# --- Gate 6:/' ci.sh)"
check_gate "ci.sh" "$CI_BLOCK" GEIGER "$STUB_DIR/ci.log"

# verify: the geiger block in cmd_all runs from its `if` to the first
# 4-space-indented `fi` (the inner `fi` is indented further).
VERIFY_BLOCK="$(awk '/^    if \[ "\$geiger" -eq 1 \]; then$/{flag=1} flag{print} flag && /^    fi$/{exit}' verify)"
check_gate "verify" "$VERIFY_BLOCK" geiger "$STUB_DIR/verify.log"

rm -rf "$STUB_DIR"

if [ "$fail" -ne 0 ]; then
    echo "FAIL: geiger gate(s) silently no-op (bead qgis-6qa)"
    exit 1
fi
echo "ok: both geiger gates audit every workspace package"
