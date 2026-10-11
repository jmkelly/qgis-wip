# Golden files

One golden per fixture: `<fixture-stem>.expected` in this directory.

- **Verify** (default): the oracle runs the fixture through a backend and
  compares stdout against the golden; a mismatch prints a unified diff and
  exits non-zero.
- **Record**: `ORACLE_UPDATE_GOLDENS=1` (or the `record` subcommand)
  overwrites the golden with current backend output.

Override the store location with `QGIS_ORACLE_GOLDEN_DIR`.

Comparisons ignore the leading `backend: <name>/<version>` line — it is
provenance (which side produced the output), not content — so a golden
recorded from the C++ reference (the `cpp` backend, e.g. the E0 harness
at `$QGIS_CPP_BINARY`) verifies against the `rust` backend too, and
vice versa. `fixture:`, `bytes:`, `lines:`, and the body are always
compared byte-for-byte.

Goldens currently capture the C++ harness reference output (regenerated
per bead qgis-uic.7). When real algorithms land (E1+), re-record from
the C++ side:

```sh
QGIS_CPP_BINARY=<qgis-binary> ORACLE_UPDATE_GOLDENS=1 \
  cargo run -p qgis-oracle -- <fixture> --backend cpp
```
