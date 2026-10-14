# Worktree Split Guidelines

Inspired by pstack's principle **"Separate before serializing shared state"** —
remove the sharing *before* adding coordination. Merge conflicts arise when two
worktrees edit the same file; the fix is not better merge conflict resolution,
it is eliminating the overlap upstream.

## Golden rule

> **Each file has exactly one writer.** No two concurrent worktrees edit the
> same file. Shared infrastructure (Cargo.lock, Cargo.toml, oracle goldens,
> lib.rs re-exports) is owned by the orchestrator, not by implementers.

## 1. Split by crate, not by feature

| ✅ Good split | ❌ Bad split |
|---|---|
| Agent A → `geometry/` | Agent A → "polygon feature" |
| Agent B → `crs/` | Agent B → "projection feature" |
| Agent C → `expression/` | Agent C → "server feature" |

Crates are the natural isolation boundary. Each has its own `Cargo.toml`,
its own `src/` tree, and its own test suite. When you split by *feature*,
two agents inevitably both touch `expression/src/functions/mod.rs` or
`server/src/service.rs`. When you split by *crate*, they don't.

**Exception:** if a single crate is too large for one agent (e.g. `expression`
with its parser, lexer, eval, and function modules), split it by *submodule*
within the crate — but only if the submodules are genuinely independent
files with no shared `mod.rs` churn.

## 2. Stabilize interfaces before fanning out

Before spawning parallel work:

1. **Lock the public API** of each crate that others depend on. Write the
   `pub trait` / `pub struct` / `pub fn` signatures in a skeleton file.
2. **Record the locked API** as a bead so everyone codes against the same
   contract, not against a moving target.
3. **Only then** spawn implementers to fill in the bodies.

pstack's *"Sequence work into verifiable units"* applies: each interface
definition is a small unit that ends in a check (does it compile? does it
express the right contract?) before implementation begins.

## 3. Exclusive file ownership map

Before dispatch, the orchestrator builds a **file ownership map** listing
every file each implementer will touch. The rule:

> If a file appears on two agents' lists, re-scope one agent.

Common collision points in this repo (from git history — most frequently
committed files):

| File | Why it collides | Fix |
|---|---|---|
| `Cargo.lock` | every dependency addition | orchestrator-only |
| root `Cargo.toml` | new crate members | orchestrator-only |
| `crates/*/Cargo.toml` | dependency additions | one agent per crate; if same crate, serialize |
| `*/src/lib.rs` | new module declarations | one agent per crate |
| `*/src/functions/mod.rs` | new function registration | one agent per module subtree |
| `oracle/goldens/` | golden file updates | single-writer (orchestrator) |
| `fixtures/` | shared test data | append-only; new files only, never edit existing |

## 4. Use the dependency DAG to decide what parallelizes

```
E1 geometry ──┬──→ E4 providers ──┬──→ E9 hardening
              │                    │
E2 CRS ───────┼──→ E5 analysis ───┤
              │                    │
E3 expression─┘                    │
                                    │
E6 server ──────────────────────────┤
                                    │
E7 PyO3 ────────────────────────────┤
                                    │
E8 GUI ─── (depends on E1, E2) ─────┘
```

**Parallelize freely:** E1, E2, E3, E6, E7 (independent crates, no shared files).

**Serialize or split carefully:** E4, E5, E8, E9 — they depend on the output
of E1/E2/E3 and will touch files those crates own. Either:
- Wait for E1+E2+E3 to merge, then start E4/E5/E8/E9, **or**
- Lock the interface (§2), then start E4/E5/E8/E9 against the locked API.

## 5. Single-writer shared resources

These files are **never** edited by implementers:

| Resource | Who writes | What implementers do |
|---|---|---|
| `Cargo.lock` | orchestrator at merge time | run `cargo update` locally, report new deps |
| root `Cargo.toml` | orchestrator | don't edit |
| `crates/*/Cargo.toml` | one agent per crate (if same crate, serialize) | don't edit |
| `oracle/goldens/` | orchestrator (`oracle record`) | `oracle verify` / `oracle diff` only |
| `fixtures/` | append-only: new files only | never edit existing fixture |

## 6. Per-worktree setup (recap)

Each implementer gets:
- A Paseo worktree (`isolation=worktree`, `branch-off`)
- Its own `CARGO_TARGET_DIR=target/<bead-id>`
- A file ownership list (from §3's map)
- A locked interface to code against (from §2)

## 7. Merge order matters

When merging back to main, order reduces conflicts:

1. **Leaf crates first** (geometry, CRS, expression) — they have no
   downstream dependents in the same merge batch.
2. **Mid-tier crates** (providers, analysis) — depend on leaves.
3. **Top-level crates** (server, PyO3, GUI, hardening) — depend on everything.

Merge in DAG order so each merge only touches files the previous merge
didn't touch.

## 8. Bead size discipline

**Target: one bead = one agent session (~30 min of work).**

If a bead can't be completed in a single agent session, it's too large. Split it
into sub-beads before dispatch. The cost of a merge (~2 min) is far less than
the cost of a WIP cleanup (~10 min + lost context + orphaned work).

Signs a bead is too large:
- The description mentions "and" more than once
- The acceptance criteria span multiple files
- The estimated work is > 1 hour
- A previous agent had to WIP-cleanup the bead (check bead notes for "WIP" or "workspace cleanup")

**When in doubt, split.** A merge is cheap; a lost context window is not.

## 9. Pre-session preparation

All orchestration overhead must happen *before* the swarm starts, not during:

- [ ] Guidelines written and committed
- [ ] Interface skeletons locked (pub trait/struct/fn signatures)
- [ ] File ownership maps built
- [ ] Bead queue deep enough to fill all parallel DAG slots
- [ ] Merge order planned (leaves → mid-tier → top-level)

The first commit of a session should be a feature commit, not a docs commit.
If you're writing guidelines mid-session, you lost 13+ minutes of feature time.

## 10. Batch merges

Don't merge each branch the moment it lands. Collect 2-3 completed branches
and merge them in one batch:

```bash
# Wait for 2-3 branches to complete, then:
git merge bd/qgis-3ce.5 bd/qgis-qxu.4 bd/qgis-0tn.7
```

This reduces orchestrator context-switching. Each merge costs ~2-3 min of
orchestrator attention; batching 3 merges saves ~5 min.

**Exception:** if a branch is blocking another (e.g., a leaf crate that a
mid-tier crate depends on), merge it immediately.

## 11. Intermediate commits

Instruct agents to commit every 15-20 minutes, even if the work isn't fully
done. Use a `WIP:` prefix. The orchestrator can squash or rebase later.

A commit is a checkpoint, not a promise. Frequent commits:
- Give the orchestrator visibility into progress
- Reduce WIP loss when a session ends
- Make it easier to identify which bead is stuck

## 12. Rebase before merge

Before merging a worktree branch, rebase it onto master and fast-forward:

```bash
git checkout bd/qgis-3ce.5
git rebase master
git checkout master
git merge --ff-only bd/qgis-3ce.5
```

This keeps history linear, avoids merge commits, and makes `git log` more
readable. Merge commits add noise and obscure the feature commit history.

## 13. Conflict prevention checklist (orchestrator, before dispatch)

- [ ] Every bead's file list is disjoint from every other bead in the same wave
- [ ] Shared files (Cargo.lock, root Cargo.toml, goldens) are marked orchestrator-only
- [ ] Public APIs for cross-crate dependencies are locked and recorded as beads
- [ ] Merge order follows DAG (leaves → mid-tier → top-level)
- [ ] Each worktree has its own CARGO_TARGET_DIR
- [ ] Beads are small enough for one agent session (~30 min)
- [ ] Guidelines and interfaces are committed *before* the session starts

## What this looks like in practice

```bash
# Wave 1 — independent crates, no shared files
paseo workspace create --isolation worktree --worktree-slug qgis-s4i.5   # geometry
paseo workspace create --isolation worktree --worktree-slug qgis-qxu.2   # CRS
paseo workspace create --isolation worktree --worktree-slug qgis-3ce.1   # expression

# --- wait for wave 1 to merge ---

# Wave 2 — depends on wave 1 APIs (now locked)
paseo workspace create --isolation worktree --worktree-slug qgis-nna.1   # providers
paseo workspace create --isolation worktree --worktree-slug qgis-e9r.1   # analysis
```

## Connection to pstack principles

- **Separate before serializing shared state** — the core principle. Don't
  add merge conflict tooling; remove the overlap.
- **Sequence work into verifiable units** — lock interfaces as small
  verifiable units before fanning out.
- **Guard the context window** — the file ownership map keeps each
  implementer's scope small enough to hold in context.
- **Prove it works** — per-worktree gates (test + clippy) prove each
  agent's work before merge, not after.

## Review findings (bead qgis-pqg, 2026-10-14)

Validated against `rust-port-agent-loop.md`, root `Cargo.toml`, and the
current crate layout. Findings ordered by severity.

### A. Contradictions with the agent loop (must resolve)

**A1. §12 "Rebase before merge" contradicts agent loop §10.**
The guidelines prescribe `git rebase master && git merge --ff-only`; the
agent loop prescribes `git merge --no-ff <branch> -m "merge: <bead-id>"`.
These are incompatible histories. Pick one. The agent loop's `--no-ff`
approach preserves the association between a branch and its bead ID,
which `git log --grep` and audit tooling depend on. **Proposed fix:** delete
§12 and replace with a pointer to agent loop §10.

**A2. §11 "Intermediate commits" conflicts with the implementer rule
"No commits unless the orchestrator explicitly authorized them."**
WIP commits every 15-20 minutes contradict the conservative git policy in
AGENTS.md. **Proposed fix:** remove §11, or gate it behind an explicit
orchestrator opt-in ("only when the orchestrator passes --allow-commits").

**A3. §3 and §5 use `crates/*/Cargo.toml` paths — this repo has crates at
the root.** The workspace members are `geometry/`, `crs/`, `expression/`, etc.,
not `crates/geometry/`. **Proposed fix:** change all `crates/*/` references
to root-level `*/Cargo.toml` and `*/src/`.

### B. Incomplete coverage (should add)

**B1. §4 DAG is missing several crates.** The workspace has 16 crates:
geometry, spatial-index, crs, expression, fixtures, oracle, providers,
analysis, server, pyo3, gui, qgis-bench, raster, mesh, pointcloud, render.
The guidelines only mention 9. **Proposed fix:** add spatial-index, raster,
mesh, pointcloud, and render to the DAG. Most are leaf crates (no
interdependencies) and parallelize freely with E1/E2/E3.

**B2. §5 single-writer table is missing `fixtures/manifest.toml`.** The
fixture manifest is a shared resource that any implementer adding test data
would touch. **Proposed fix:** add it to the §5 table as orchestrator-owned
(or append-only with a pattern like `fixtures/`).

**B3. No mention of the Paseo worktree lifecycle (agent loop §9).**
Create → work → archive → prune. **Proposed fix:** add a §14 referencing
agent loop §9 so the orchestrator knows to archive + prune after merge.

**B4. No mention of the `./verify` CLI or per-worktree gates.** The
guidelines say "per-worktree gates (test + clippy)" but don't reference the
composable `./verify` tool that the project actually uses. **Proposed fix:**
reference `./verify test` and `./verify clippy` in §6 and §13.

**B5. §2 API locking doesn't mention ADRs or GLOSSARY.md.** The agent loop
requires reading GLOSSARY.md + docs/adr/0001-0006 before any work, and new
third-party crates need an ADR. **Proposed fix:** add a note in §2 that
interface locking must be ADR-recorded if it introduces a new dependency.

### C. Nice-to-have adjustments (consider)

**C1. §10 batch merges vs. per-branch merges.** The agent loop merges each
branch individually. Batching 2-3 is a reasonable optimization but only
applies within a single wave. **Proposed fix:** add "within a wave" as a
qualifier.

**C2. §8 bead size discipline** is well-aligned with the agent loop. No
changes needed.

**C3. §1 "Split by crate, not by feature"** is sound but should note the
`render/` crate as a potential dependency of `gui/` (E8) and `providers/`
(E4) — rendering cuts across multiple epics. **Proposed fix:** add a note
that `render/` is a cross-cutting concern and should be locked early.

### D. Summary of proposed adjustments

| Section | Action | Priority |
|---|---|---|
| §2 | Add ADR/GLOSSARY reference | Medium |
| §3 | Fix `crates/*/` → root-level paths | High |
| §4 | Add missing crates to DAG | High |
| §5 | Fix paths; add `fixtures/manifest.toml` | High |
| §6 | Add `./verify` CLI reference | Medium |
| §10 | Add "within a wave" qualifier | Low |
| §11 | Remove or gate behind orchestrator opt-in | High |
| §12 | Delete; point to agent loop §10 | High |
| §13 | Add `./verify` CLI; fix paths | Medium |
| New §14 | Add Paseo worktree lifecycle pointer | Medium |
