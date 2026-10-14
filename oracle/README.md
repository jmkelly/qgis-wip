# qgis-oracle

C++/Rust diff oracle for the QGIS Rust port (ADR-0004). Runs identical
fixture inputs through named backends, captures stdout/stderr/exit code,
and diffs the outputs. Golden-file store for regression checks. Pure Rust,
zero dependencies — the C++ side is reached only by shelling out to a
user-supplied binary (`std::process::Command`), never via FFI
(ADR-0001).

## Layout

- `src/backend.rs` — backend registry (`rust`, `cpp`) and error type
- `src/rust_backend.rs` — Rust side under test (deterministic placeholder at E0)
- `src/cpp_backend.rs` — C++ side; shells out to `$QGIS_CPP_BINARY`, else
  fails with a clear "C++ backend not built yet" message
- `src/diff.rs` — LCS line diff with unified rendering
- `src/goldens.rs` — golden store (`goldens/<fixture-stem>.expected`),
  verify/record modes via `ORACLE_UPDATE_GOLDENS=1`
- `src/pipeline.rs` — load/style/save pipeline support: classifies
  geometry/style/project fixtures and diffs their Rust artifacts against
  the QGIS C++ goldens (`goldens/<kind>/<stem>.expected`)
- `src/cli.rs` — CLI parsing and execution
- `fixtures/` — sample inputs
- `goldens/` — expected outputs (see `goldens/README.md`)

## Usage

```sh
# Verify the Rust backend's output for a fixture against its golden
cargo run -p qgis-oracle -- fixtures/trivial.txt

# Run a backend and print its stdout
cargo run -p qgis-oracle -- run fixtures/trivial.txt --backend rust

# (Re-)record a golden (same as ORACLE_UPDATE_GOLDENS=1)
cargo run -p qgis-oracle -- record fixtures/trivial.txt

# Diff two backends on the same fixture
cargo run -p qgis-oracle -- diff fixtures/trivial.txt --backend-a rust --backend-b cpp
```

Exit codes: `0` match/success, `1` outputs differ, `2` usage/backend
error.

## Load/style/save pipeline (E16.4)

`src/pipeline.rs` extends the harness to the E14 pipeline (load → style →
save). It classifies a fixture by extension into one of three families —
`geometry` (`.wkt`), `styles` (`.qml`), `projects` (`.qgs`) — and diffs the
Rust artifact against the QGIS C++ golden stored at
`goldens/<kind>/<stem>.expected`. The `qgis-validate` crate drives the real
`qgis-geometry`/`qgis-style`/`qgis-gui` crates and renders the report:

```sh
./verify agent-oracle    # per-fixture diff report for geometry, styles, projects
```

A missing golden is reported as a `skip` naming the path (the orchestrator
records goldens from the C++ reference with `oracle record`); when
`QGIS_CPP_BINARY` is set the suite also runs a live Rust-vs-C++ diff.

## C++ harness (E0)

A full QGIS build is impractical on most machines (no system cmake,
30-60+ minute compile), so the repo ships a minimal C++ harness that
plays the role of `QGIS_CPP_BINARY` at E0 (bead qgis-uic.7). It emits
the same canonical output format as the Rust backend, with the
backend self-identification line set to `cpp`:

```text
backend: cpp/0.1.0
fixture: <file-name>
bytes: <byte-count>
lines: <line-count>
---
<fixture content with trailing blank lines trimmed>
```

Build it (out of tree; requires `cmake` >= 3.16 and a C++17 compiler on
`PATH`):

```sh
cmake -S rust/cpp_harness -B rust/cpp_harness/build
cmake --build rust/cpp_harness/build
```

The binary lands at `rust/cpp_harness/build/qgis_oracle_harness` (build
output is gitignored). Then point the oracle at it and run a live
C++ diff:

```sh
export QGIS_CPP_BINARY=rust/cpp_harness/build/qgis_oracle_harness
cargo run -p qgis-oracle -- diff crates/oracle/fixtures/trivial.txt --backend-a rust --backend-b cpp
```

`./ci.sh` runs this live diff for every fixture in
`crates/oracle/fixtures/` when `QGIS_CPP_BINARY` is set; without it the
gate prints a warning and skips the C++ diff (the Rust-side smoke
verify still runs). At E1+, point `QGIS_CPP_BINARY` at a real QGIS
binary instead — the interface (`<binary> <fixture-path>`, canonical
output on stdout) does not change.

### Backend self-identification

The leading `backend: <name>/<version>` line is provenance (which side
produced the output), not content. The oracle strips it wherever
outputs are compared (golden verify and `diff`), so backend name and
version changes never show up as drift; `fixture:`, `bytes:`,
`lines:`, and the body are always compared byte-for-byte.

## Environment

| Variable | Effect |
| --- | --- |
| `ORACLE_UPDATE_GOLDENS=1` | `verify` re-records goldens instead of comparing |
| `QGIS_ORACLE_GOLDEN_DIR` | Override the golden store directory |
| `QGIS_CPP_BINARY` | Path to a QGIS C++ binary; enables the `cpp` backend |

## Status

E0: harness mechanics are code-complete, and the C++ oracle pipeline
is live. The Rust backend is a deterministic placeholder (canonical
echo of the fixture); the C++ side is the minimal harness at
`rust/cpp_harness/` (see above), selected via `QGIS_CPP_BINARY` and
diffed against Rust by `./ci.sh` over the E0 fixture set. Goldens
capture the C++ harness reference output. Parity hardening (C++ vs
Rust on real algorithms) comes in E1+, when `QGIS_CPP_BINARY` moves
to a real QGIS binary.
