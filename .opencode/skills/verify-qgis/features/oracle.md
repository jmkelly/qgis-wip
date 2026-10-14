# Feature map: qgis-oracle

## What it does

C++/Rust diff oracle harness (ADR-0004). Runs identical fixture inputs
through named backends, captures stdout/stderr/exit code, and diffs
outputs. Golden-file store for regression checks. Pure Rust, zero deps —
C++ reached only by shelling out to `$QGIS_CPP_BINARY`.

## How to verify

```bash
./verify oracle                    # verify all goldens
./verify oracle --fixture <path>   # verify one fixture
./verify diff <fixture>            # rust vs golden
./verify diff <fixture> --cpp      # rust vs C++ (needs QGIS_CPP_BINARY)
```

## What proves each feature works

| Feature | Evidence |
|---------|----------|
| Rust backend output matches golden | `./verify oracle` |
| C++ backend output matches Rust | `./verify diff <f> --cpp` |
| Load/style/save pipeline diff report | `./verify agent-oracle` |
| Golden store integrity | `goldens/` directory has `.expected` files |
| Backend self-ID stripped in diffs | `output.rs` tests |
| LCS line diff + unified render | `diff.rs` tests |
| Fixture loading + canonical echo | `fixture.rs` tests |
| Pipeline fixture classification + reference diff | `pipeline.rs` tests |

## Fixture set

32 real QGIS4 test data entries in `fixtures/` (qgs-projects, styles,
expressions, provider-samples). All from `tests/testdata/`.

## Golden management

```bash
./verify oracle --update-goldens    # re-record all goldens
./verify oracle --fixture <f> --update-goldens  # re-record one
```

If goldens drift, investigate — don't blindly re-record. Goldens capture
the C++ harness reference output. Backend self-ID lines are stripped
before comparison so version changes don't show as drift.
