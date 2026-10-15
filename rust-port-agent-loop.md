# QGIS Rust Port — Unattended Agent Loop Runbook

Model-agnostic. Any agent with shell + `bd` can run this.

## 1. Contract (do not renegotiate mid-loop)

- Read `GLOSSARY.md` and `docs/adr/0001-0006` before any work.
- Rules: pure Rust, no FFI to GDAL/GEOS/PROJ/Qt; headless first, iced GUI last;
  PyO3 preserves `qgis.core`/`qgis.gui`; close beads on code-complete, parity is verified in E9.
- Cartographic projections come from the `proj4rs` crate, never hand-written — ADR 0006. WKT2,
  datum ensembles, GSB/GTX grids and `projinfo`/cct parity are ours. Read ADR 0006 before touching
  `qgis-crs`.
- The dependency graph is a DAG with task-level edges (bead qgis-uic.4). Independent crates
  may overlap — E2 CRS, E3 expression, E6 server and E7 PyO3 start alongside E1 geometry.
  Never start a task whose blockers are still open.

## 2. Beads layout

10 epics in a dependency DAG (78 open issues). Task-level edges (bead qgis-uic.4) replace
the old linear chain so independent crates overlap:

```
E0 scaffold (qgis-49a)  [closed]
E0.5 GUI POC (qgis-ivl, qgis-rez)  [closed]
                    ↓
E1 geometry (qgis-s4i) ──┬──→ E4 providers (qgis-nna) ──┬──→ E9 hardening (qgis-11m)
                        │                                │
E2 CRS (qgis-qxu) ──────┼──→ E5 analysis (qgis-e9r) ───┤
                        │                                │
E3 expression (qgis-3ce)┘                                │
                                                         │
E6 server (qgis-99n) ────────────────────────────────────┤
                                                         │
E7 PyO3 (qgis-0tn) ──────────────────────────────────────┤
                                                         │
E8 GUI (qgis-u24) ─── (depends on E1, E2) ───────────────┘
```

E2 is a facade epic, not a rebuild: `proj4rs` supplies the `+proj` math and `crs-definitions` the
EPSG dataset (ADR 0006), so E2's beads are gap builds around them.

True data dependencies (task-level edges):
- E4 providers → E1 geometry (providers read/write geometry)
- E5 analysis → E1 geometry, E2 CRS, E3 expression
- E8 GUI → E1 geometry, E2 CRS (gui crate depends on both); E0.5 POC already closed
- E9 hardening → all epics (parity requires everything)
- E2 CRS, E3 expression, E6 server, E7 PyO3 need only a crate skeleton — no E1 dependency

`bd ready` is the scheduler. Only unblocked work is shown. Independent crates may overlap;
do not start a task whose blockers are still open.

### Bead template

Every new bead MUST carry an executable acceptance check — a runnable shell command
that returns 0 on success. See `.agents/skills/beads/template.md` for the standard
format. The acceptance command is written into the bead's `--acceptance` field and
verified by the reviewer.

## 3. Orchestrator loop (parallel worktree model)

Parallel implementers are safe when each works in an isolated Paseo worktree with its own
`CARGO_TARGET_DIR`. The previous serial-only rule existed because `rust/`, `Cargo.lock`, and
oracle fixtures all collided; worktree isolation and per-bead target dirs remove those collisions.

### Dispatch flow

```bash
bd prime
bd ready
# take top bead ID, e.g. qgis-49a.1
bd show <id>   # read description + acceptance + parent epic
```

For each ready bead, create an isolated worktree workspace:

```bash
paseo workspace create --mode branch-off --isolation worktree --worktree-slug <bead-id>
```

Spawn an implementation subagent per bead (foreground, wait for finish). Each implementer
gets:
- Its own Paseo worktree (isolation=worktree), not a branch in the shared checkout.
- Its own `CARGO_TARGET_DIR` (e.g., `target/<bead-id>`) so build caches never collide.

When an implementer reports done, close the bead, then:

```bash
bd close <id> --suggest-next   # --suggest-next prints newly unblocked beads
```

Review is batched per epic (see §5), not per bead.

Repeat until `bd ready` is empty.

### Concurrency rules

- **Oracle goldens stay single-writer**: only the orchestrator records goldens
  (`oracle record`). Implementers run `oracle verify` / `oracle diff` only.
- **Cargo.lock conflicts**: handled by taking the union of dependency additions.
  The orchestrator merges after all implementers finish (see §10).
- **Gates per worktree**: the orchestrator runs gates in each worktree independently,
  then merges the worktree branch back to the main branch.

## 4. Implementer prompt template

Copy-paste per bead, fill in `<id>`:

```
You are the implementer for QGIS Rust port bead <id>.

1. Run: bd show <id> and read GLOSSARY.md + docs/adr/0001-0006.
2. Claim: bd update <id> --claim
3. Worktree & target dir (set up by orchestrator, verify):
   - You are in a Paseo worktree workspace (isolation=worktree).
   - Set CARGO_TARGET_DIR so build caches never collide:
     export CARGO_TARGET_DIR=target/<id>
   - Confirm: git branch (should be the worktree branch, not main).
4. Implement ONLY that bead's scope. Pure Rust, no FFI
   (verify with cargo-geiger if Rust code exists yet).
   Projection math comes from the `proj4rs` crate (ADR 0006) — never hand-written.
   No new third-party crate without an ADR; approved crates must be pure Rust
   (cargo geiger), GPL-3.0-compatible, and pinned exact in rust/Cargo.toml.
   Keep crates small; each must hold <5 min `cargo test`.
   Log gaps/decisions with: bd remember "bead <id>: <insight>"
5. Quality gates before reporting done:
    - Run the bead's acceptance command (from `bd show <id>`) — must exit 0.
    - cargo test for touched crates
    - cargo clippy -- -D warnings (if toolchain present)
    - oracle diff if harness exists (E0+), else note "oracle pending"
    - No commits unless the orchestrator explicitly authorized them.
6. Report: files changed, test output, remaining risks.
    Do NOT close the bead yourself; the orchestrator closes it.
Rules: bd for tracking only, no markdown TODOs, no `bd edit`,
no `bd import`, no treating .beads/issues.jsonl as source of truth.
```

## 5. Reviewer prompt template (per epic)

Review is batched per epic, not per bead. When all beads in an epic are closed,
spawn one reviewer for the entire epic.

```
You are the reviewer for epic <epic-id>. Do NOT implement.
1. bd show <epic-id>; read all child beads and their acceptance commands.
2. For each child bead, run its acceptance command from the repo root.
3. Check: scope match, ADR compliance (no FFI), tests pass,
   acceptance criteria met for every child.
4. On failure: file a follow-up with
   bd create "Fix: <what>" --description="..." --type=bug --priority=1
   and report the new ID. Never silently fix + close.
```

## 6. Session close (every implementer/reviewer)

1. File follow-up beads for anything unfinished.
2. Run quality gates (§4.5), paste output in report.
3. `git status` — report, do not commit/push (conservative default).
4. Worktree audit (implementers in Paseo worktrees):
   - Confirm your worktree branch is clean (no uncommitted changes).
   - Report the worktree ID so the orchestrator can archive it.
   - Do NOT run `git worktree prune` yourself; the orchestrator does this
     after merging (see §10).
5. Hand off: bead ID, validation, blocked sync/push steps.

## 7. CI gates and the bead workflow

The CI pipeline (`.github/workflows/rust-port.yml`) runs the same gates the
implementer runs before reporting done. This ensures that code merged to
`main`/`master` is always in a known-good state.

### What CI runs

| Gate | Command | Purpose |
|------|---------|---------|
| Test | `cargo test --workspace` | Primary correctness gate |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | Enforce zero warnings |
| Bench compile | `cargo bench -p qgis-bench --no-run` | Verify criterion benches compile |
| Oracle smoke | `cargo run -p qgis-oracle -- oracle/fixtures/trivial.txt` | Verify oracle harness works |
| Geiger | `cargo geiger --workspace` | Informational: verify no unsafe/FFI |

### Local CI script

All gates can be run locally with a single command:

```bash
cd rust
./ci.sh           # run all gates
./ci.sh --geiger  # also run cargo-geiger
```

The script exits non-zero on the first failure, making it suitable for
pre-commit checks or manual verification.

### Relationship to bead workflow

- The implementer runs the same gates (§4.4) before reporting done.
- CI runs them again on every push/PR to catch regressions.
- If CI fails, the implementer should pull the failure, fix it, and re-run
  the local CI script before reporting done.
- The oracle smoke test in CI uses the trivial fixture; full oracle diffs
  are run by the implementer during development.

## 8. Current state (E0 complete)

The E0 scaffold epic is complete:

- **Oracle harness** (`oracle/`): C++/Rust diff oracle with
  golden-file store. CLI supports `verify`, `run`, `record`, and `diff`
  subcommands. Fixtures live in `oracle/fixtures/`, goldens in
  `oracle/goldens/`.
- **Fixture store** (`fixtures/`): Shared test fixtures for
  workspace crates.
- **Criterion benches** (`qgis-bench/`): Benchmarks for hot
  paths (render, index, expression, raster) using the `criterion` crate.
- **CI pipeline** (`.github/workflows/rust-port.yml`): Runs test, clippy,
  bench compile, oracle smoke, and geiger on every push/PR.
- **CI script** (`rust/ci.sh`): Local gate runner for developers.

The workspace has 11 crates: geometry, crs, expression, fixtures, oracle,
providers, analysis, server, pyo3, gui, and qgis-bench.

## 9. Paseo worktree lifecycle policy

Parallel implementers must never accumulate stale worktrees. The policy uses
Paseo for workspace management and git worktree prune for cleanup.

### Per-bead lifecycle

1. **Create** (orchestrator, before spawning implementer):
   ```bash
   paseo_create_workspace(mode=branch-off, isolation=worktree, worktreeSlug=<bead-id>)
   ```
2. **Work** (implementer): The implementer works in the Paseo worktree with
   its own `CARGO_TARGET_DIR` to avoid build artifact contention.
3. **Archive** (orchestrator, after `bd close`):
   ```bash
   paseo_archive_workspace(workspaceId)
   git worktree prune
   ```

### Orchestrator audit (every session close)

```bash
paseo_list_workspaces          # should show only local_checkout workspaces
git worktree list               # should show only the main checkout
```

If any worktree-isolation workspaces remain, archive them immediately.
Current state is clean: single checkout, all Paseo workspaces are
`local_checkout`. Keep it that way.

### CARGO_TARGET_DIR isolation

Each parallel implementer gets its own target directory to prevent build
artifact contention:

```bash
export CARGO_TARGET_DIR=target/<bead-id>
```

This is set by the orchestrator before spawning the implementer and is
documented in the implementer prompt.

## 10. Merge step (orchestrator)

After implementers report done and per-worktree gates pass, the orchestrator merges each
worktree branch back to the main branch.

### Merge procedure

1. **Verify gates passed** in each worktree (test + clippy).
2. **Merge** the worktree branch into main:
   ```bash
   cd <main-checkout>
   git merge --no-ff <worktree-branch> -m "merge: <bead-id>"
   ```
3. **Cargo.lock conflicts**: if both implementers added dependencies, take the union of
   additions. Resolve by keeping every dependency from both sides; do not drop either
   implementer's additions.
4. **Post-merge gates**: run full workspace gates on the merged result:
   ```bash
   cargo test --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   ```
5. **Archive** the worktree (see §9):
   ```bash
   paseo_archive_workspace(workspaceId)
   git worktree prune
   ```

This is set by the orchestrator before spawning the implementer and is
documented in the implementer prompt.
