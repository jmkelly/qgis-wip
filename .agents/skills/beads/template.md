# Bead Template

Every new bead MUST carry an executable acceptance check. This is a runnable command
that returns 0 on success and non-zero on failure. It is written into the bead's
`--acceptance` field and verified by the reviewer.

## Standard bead format

```bash
bd create "Short title" \
  --description="Why this exists and what needs to be done" \
  --acceptance="<runnable command>" \
  --type=task \
  --priority=2 \
  --parent=<epic-id>
```

## Acceptance check format

The acceptance field must contain a shell command that can be run from the
repository root. Preferred forms:

```bash
# Rust crate with tests
cargo test -p <crate-name>
cargo test -p <crate-name> --lib

# Oracle golden verify
cargo run -p qgis-oracle -- verify crates/oracle/fixtures/<fixture>.txt

# Workspace gates
./ci.sh

# Custom script
bash scripts/verify-<bead-id>.sh
```

The command must:
- Return exit code 0 on success
- Return non-zero on failure
- Run in under 5 minutes
- Not require interactive input

## Example

```bash
bd create "WKT2:2019 parser" \
  --description="Hand-written recursive-descent parser for the QGIS WKT2:2019 subset" \
  --acceptance="cargo test -p qgis-crs --lib" \
  --type=task \
  --priority=2 \
  --parent=qgis-qxu
```

## Reviewer usage

The reviewer runs the acceptance command directly:

```bash
# From the repository root
<acceptance command from bead>
```

If the command fails, the reviewer files a follow-up bead instead of closing.
