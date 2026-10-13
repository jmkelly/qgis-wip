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

## 8. Conflict prevention checklist (orchestrator, before dispatch)

- [ ] Every bead's file list is disjoint from every other bead in the same wave
- [ ] Shared files (Cargo.lock, root Cargo.toml, goldens) are marked orchestrator-only
- [ ] Public APIs for cross-crate dependencies are locked and recorded as beads
- [ ] Merge order follows DAG (leaves → mid-tier → top-level)
- [ ] Each worktree has its own CARGO_TARGET_DIR

## What this looks like in practice

```bash
# Wave 1 — independent crates, no shared files
paseo_create_workspace(worktreeSlug=qgis-s4i.5)   # geometry
paseo_create_workspace(worktreeSlug=qgis-qxu.2)   # CRS
paseo_create_workspace(worktreeSlug=qgis-3ce.1)   # expression

# --- wait for wave 1 to merge ---

# Wave 2 — depends on wave 1 APIs (now locked)
paseo_create_workspace(worktreeSlug=qgis-nna.1)   # providers
paseo_create_workspace(worktreeSlug=qgis-e9r.1)   # analysis
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
