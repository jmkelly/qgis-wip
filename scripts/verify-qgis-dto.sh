#!/usr/bin/env bash
# Acceptance / regression guard for bead qgis-dto.
#
# The workspace crates live at the repository root (oracle/, geometry/, crs/,
# ...), not under the pre-move `rust/crates/` layout. A `crates/<crate>/...`
# path therefore does not resolve. This checks that the files named in the
# bead carry no such stale reference and that every path they point at exists.
#
# Exit 0 when clean; non-zero with the offending lines when not.
set -euo pipefail
cd "$(dirname "$0")/.."

FILES=(
    README.md
    main.cpp
    CMakeLists.txt
    .agents/skills/beads/template.md
    oracle/src/cpp_backend.rs
    rust-port.yml
    rust-port-agent-loop.md
    ci.sh
)

fail=0

# 1. No workspace path may keep the pre-move `crates/<crate>` prefix. The
#    pattern requires a path component after `crates/`, so prose like "the
#    crates/ prefix" is not flagged; crates.io URLs are legitimate and excluded.
if matches=$(grep -nHE 'crates/[a-zA-Z$]' "${FILES[@]}" | grep -v 'crates\.io'); then
    echo "FAIL: stale crates/ path reference(s):"
    echo "$matches"
    fail=1
fi

# 2. Every path the files now point at must resolve.
for path in \
    oracle/fixtures/trivial.txt \
    oracle/goldens \
    oracle/README.md \
    oracle/src/rust_backend.rs \
    fixtures \
    qgis-bench
do
    if [ ! -e "$path" ]; then
        echo "FAIL: referenced path does not exist: $path"
        fail=1
    fi
done

if [ "$fail" -ne 0 ]; then
    exit 1
fi
echo "=== qgis-dto: no stale crates/ paths; referenced paths resolve ==="
