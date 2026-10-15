# Orchestrator runbook — QGIS Rust workspace

**This is the single source of truth** for the coordinator tick steps, cap
enforcement, stop conditions, and the verbatim worker prompt. Skills and
schedule prompts reference this file — do not duplicate its content elsewhere.

The maintained home for the coordinator steps and the verbatim worker prompt for
the QGIS Rust port. Read this first, then `AGENTS.md`, then `bd prime`. Work only
in the bead queue — do not implement anything yourself; keep implementation in
worker worktrees.

This runbook adapts the SpatialEngine swarm pattern to the QGIS Rust workspace:

- gates are the `./verify` CLI (and `ci.sh`), not `eng/verify.sh`;
- worktrees follow `worktree-split-guidelines.md` (split by crate, one writer
  per file);
- every parallel implementer gets its own `CARGO_TARGET_DIR` so build caches
  never collide.

---

## Configuration (settled)

| Setting | Value |
| --- | --- |
| Process epic | `qgis-uic` (velocity / parallelisation) |
| Port epics | `E0` scaffold … `E9` hardening (see `rust-port-agent-loop.md` §2) |
| Provider / model | `opencode` / `opencode-go/longcat-2.5-preview-free` (pinned for workers; re-check with `paseo provider ls` / `paseo provider models` if a spawn fails) |
| Mode | none required; the CLI agent runs in its configured mode |
| Concurrency cap | **6** worker worktrees (the coordinator is not a worktree and does not count) |
| Cron | `*/5` — every 5 minutes (Australia/Sydney) |
| Max runs | none — the schedule object is authoritative |
| Goal | **run to exhaustion** — wave limit is a scheduling fact, not a stopping point |
| Cwd | repository root (`.`) |
| Default branch | `master` (this repo; SpatialEngine uses `main`) |
| Bead namespace | `qgis-*` |

**No scope limit.** The run drains until `bd ready` is empty for the port and
process epics and no worker is in flight. Follow-up beads workers create are part
of the queue. When nothing is ready and nothing is in flight, report **"queue
drained"** and do nothing; the human deletes the schedule. Do not invent work.

`bd ready --exclude-label=process` keeps the port chain on the chain while
process work is scheduled separately. Treat a bead's acceptance command as the
definition of done.

### Coordinator spawn mechanism (persistent, not per-tick)

The coordinator is a **persistent agent**, not a fresh spawn each tick. The
`*/5` cron used to spawn a brand-new coordinator every five minutes via
`paseo run`; on Oct 13 alone 92 coordinator agents died instantly with
"Rate limit exceeded. Please try again later." — the single largest waste of
agent runs in the swarm. The provider rate-limits frequent *spawns*; reusing
a live agent via `paseo send` does not touch that limit.

**Spawn the coordinator once**, then drive every tick by sending the tick
prompt to the same agent id:

```bash
# one-time setup (human or orchestrator): create the persistent coordinator
paseo run --provider opencode --model opencode-go/longcat-2.5-preview-free \
  "You are the swarm coordinator … Read eng/runbook.md …"
# -> note the returned agent id

# each tick: reuse the same agent instead of spawning a new one
paseo send <coordinator-agent-id> "<one-tick coordinator prompt>"
```

The schedule object targets the persistent agent (`paseo send` to a fixed id),
not `new-agent` (`paseo run`). The runbook is the source of truth for this
mechanism — a spawn that fails with a rate-limit error is retried by reusing
the existing coordinator agent via `paseo send`, never by spawning again.

The `*/5` cadence is unchanged: the rate-limit cost was in the *spawn*, not
the tick frequency, and `paseo send` removes it. If a future provider
re-introduces a send-side limit, lower the cadence rather than reverting to
per-tick `paseo run`.

---

## Cap enforcement rules

- The cap is **6 concurrent worker worktrees**. The coordinator runs in the
  main checkout and never counts toward it.
- Count the agents *you* spawned — matched by the bead ids in their
  `in-flight` notes and by a worktree under `~/.paseo/worktrees/` — not the
  maintainer's own sessions, which `paseo ls` also shows.
- Enforce the cap at the **top of every tick**, not only before spawning: a cap
  checked only at spawn time drifts back up, because every tick re-counts what
  the last tick left running.
- Over 6, keep the 6 with the most invested work (a committed hand-off, or an
  unmerged P1 bead, beats a bead that has barely started). For each excess:
  - `paseo stop <agent-id>`;
  - in its worktree: `git add -A && git commit -m "WIP: preserve uncommitted
    work for <id> (coordinator tick <date>)" -m "Task: <id>"`;
  - `bd update <id> --add-label in-flight --append-notes "<agent> <branch>
    <worktree> <wip-commit>"`;
  - `bd unclaim <id>`;
  - **leave the worktree in place** — a later tick reuses it and resumes from
    the WIP commit.
- Never `git stash`: stash is shared across this repo's worktrees and has
  clobbered a worker mid-task. Never stash, and never tell a worker to.
- Shared files (`Cargo.lock`, root `Cargo.toml`, oracle goldens, `fixtures/`
  existing files) are orchestrator-owned; implementers report new dependencies
  and let the orchestrator take the union at merge time.
- Raise the cap only once file-overlap evidence has held for a full
  drain cycle and the incident counts (mis-dispatches, WIP-preservation
  commits, non-trivial merge conflicts, reclaim races) are falling.
- If the box is saturated, prefer merging (step 4) over spawning (step 5).

---

## Coordinator steps (one tick)

Do exactly these, in order. Stop early on any stop condition.

### 0. Publish gate

Local `master` must never be ahead of `origin/master` when a tick spawns work,
because every worker branches from `origin/master`.

```bash
git rev-list --count origin/master..master   # must be 0
```

If it is non-zero, push first and merge **nothing else this tick**:

```bash
git push origin master
```

Re-check after pushing: `git rev-list --count origin/master..master` must again
be `0`. A tick that did not finish `published: master is at origin/master` did
something unrecoverable.

### 1. CLEAN STALE WORKTREES

Run every tick, before any other work — worktrees pile up when this is
skipped. List the worktree workspaces with `paseo workspace ls`.

For each worktree workspace under `~/.paseo/worktrees/`:

- **Check for a running agent in that worktree, filtered by cwd.** An
  unfiltered agent listing may omit active agents living in worktree
  workspaces because of list limits/sorting. Before ANY archive or `rm -rf`,
  run the agent listing filtered by `cwd=<worktree-path>` (e.g.
  `paseo ls --cwd <worktree-path>`, and/or `paseo inspect <agent-id>`
  on the agent ids recorded in the bead notes). `status=running` means
  **leave it alone** — never archive or remove a worktree with a running
  agent.
- **Check the branch is fully merged** before deleting: `git branch --merged
  master` (per worktree repo, before `rm -rf`).
- Merged worktree, no running agent → archive the workspace
  (`paseo archive workspace <workspace-id>`), remove the directory
  (`rm -rf <worktree-path>`), and confirm removal (`ls <worktree-path>` must
  fail).
- Unmerged worktree, no running agent → if its bead is closed, archive and
  remove; if the bead is `in_progress` but stale, preserve uncommitted work
  with a `WIP:` commit first (see TRIM), then archive and remove.
- Report each cleanup action. If nothing needed cleanup, say `worktree
  cleanup: all clear`.

### 2. TRIM

Enforce the cap (above) before anything else.

### 3. RECOVER

```bash
python3 tools/bd-safe-reclaim.py --dry-run
```

Read the `KEEP` / `RECLAIM` / `WOULD` verdicts against `paseo ls`, then run it
without `--dry-run`:

```bash
python3 tools/bd-safe-reclaim.py --older-than 90m
```

It reclaims only leases no live agent holds. Bare `bd reclaim` does not, and on
a stale lease it kills live work.

- Reclaim is for dead workers only — never a bead you stopped in TRIM, and never
  a bead labelled `human` waiting on a maintainer decision.
- Any bead left `in_progress` for more than 90 minutes with no running paseo
  agent working its branch is stale: reclaim it, note why, let it re-enter
  `bd ready`.
- Rehearse against a scratch queue with `--db <path>`, never against the live
  one.

### 4. MERGE

For every bead labelled `needs-merge`:

```bash
python3 tools/bd-merge-bead.py --bead <id> [--crate <crate>]
```

The tool fetches, rebases the branch onto `origin/master`, runs the fast gate
(`./verify test --crate <crate>`), merges `--no-ff` into local `master`, pushes,
and closes only once the branch is an ancestor of `origin/master`.

- `--crate` scopes the gate to the touched crate and its dependents; default
  `core` (used only as a fallback — pass the real crate when you know it).
- Pass `--full` for a change broad enough to doubt the scoping.
- Do not do the steps by hand and do not call `bd close` yourself.
- If it exits non-zero, `bd update <id> --append-notes "..."` the output and
  move on. A red gate is re-run once on the same commit: green merges; red twice
  for the same reason is a stop condition.
- Never close a bead whose gate is red.
- Merges are **local**: no pull request, no review wait. The push to `master`
  is the merge.

Merge in DAG order when possible: leaf crates (geometry, crs, expression) →
mid-tier (providers, analysis, spatial-index, raster, mesh, pointcloud) →
top-level (server, pyo3, gui, hardening). Batch 2–3 when they do not block each
other.

### 5. DRAIN

Only if the merge queue is empty. Re-read `bd list --label needs-merge`
immediately before spawning; if anything is still labelled `needs-merge`, spawn
nothing this tick — a backlog of handed-off work outranks work not yet started.

Count running workers you spawned. While workers < 6:

- take from `bd ready`, in order:
  1. children of the port epics (E1…E9), then
  2. **any other ready bead in the repo** — follow-up beads created via
     `bd create` are the real work of the run; an "epic children only" filter
     strands them;
- skip anything labelled `in-flight`, `human`, or already claimed;
- honour task-level dependency edges: never start a task whose blockers are
  still open (`bd ready` already enforces this);
- apply the **file-overlap lock by derivation**, not from a hand-written list:
  - read each candidate's description and its file list; the beads in this
    project name exact crates and (mostly) files;
  - one bead per crate per wave; a candidate that touches a crate another
    in-flight bead already touches is `BLOCKED` — skip it and state the
    shared crate/file in the report;
  - shared files (`Cargo.lock`, root `Cargo.toml`, `oracle/goldens/`,
    existing `fixtures/`) are orchestrator-owned, so two beads adding
    dependencies in the same wave collide on root `Cargo.toml` — serialise
    them or drop one to a later wave;
  - a bead that names no file is "order by hand": the lock cannot invent a
    file set, so you decide and say so in the report;
  - if nothing is free, report the blocked list and spawn nothing this tick —
    spawning an overlap is the merge conflict this exists to prevent;
- resolve the read list once here: copy the bead's file/crate list out of
  `bd show <id>` and paste it into that worker's prompt (worker step 1 already
  reads the bead, but the coordinator's copy is what the cap check is based on);
- `bd update <id> --claim` (atomic; if it fails, someone else got it — move on);
- create or reuse a worktree:

```bash
paseo workspace create --isolation worktree --mode branch-off \
  --new-branch bd/<id> --base origin/master
```

  If a worktree for that bead already exists (an earlier tick's `WIP` commit),
  reuse it (`paseo workspace ls`, match the branch) instead of creating a
  second one — `branch-off` fails when the branch is already checked out.

- spawn the worker in the background, in its worktree:

```bash
paseo run --provider opencode --model opencode-go/longcat-2.5-preview-free \
  -d \
  --workspace <workspace-id> "<worker prompt below, BEAD_ID substituted>"
```

  `-d` is mandatory; without it the spawns serialise and one tick never
  finishes.

- label the bead `in-flight` and record the agent id, branch and workspace in
  its notes with `--append-notes` (a bare `--notes` has destroyed notes before):

```bash
bd update <id> --add-label in-flight \
  --append-notes "agent <agent-id> branch bd/<id> workspace <workspace-id>"
```

- set the worktree's `CARGO_TARGET_DIR` (record in the note / pass in the
  prompt): `target/<id>` relative to the worktree root.

### 6. REPORT

One short line per bead touched this tick, plus:

```bash
git rev-list --count origin/master..master   # publish gate result
```

If nothing was ready and nothing is in flight, say **"queue drained"** and stop
doing work. Do not poll, wait, or sleep — cron brings you back.

Counts to record each drain cycle: mis-dispatches, WIP-preservation commits,
non-trivial merge conflicts, reclaim races. A cap stays at 6 or below until
those fall over a full cycle.

STOP CONDITIONS (any one, then do nothing more this tick):
- a bead fails its verify gate twice for the same reason;
- a merge conflict that is not a trivial textual conflict;
- anything needing an architectural decision the bead did not already authorise
  (stop, `bd create` a bead on the epic recording the decision needed);
- the toolchain / environment is broken (`./verify doctor` failing on a clean
  baseline) — report it;
- Docker (if integration suites apply) has become unreachable.

---

## Worker prompt (verbatim template)

**Verification commands** are documented in the [`verify-qgis` skill](../.opencode/skills/verify-qgis/SKILL.md).
The commands below are the ones every worker must run; see the skill for the
full reference and feature maps.

**Substitution rule.** `BEAD_ID` is substituted everywhere it appears. The header
and the body must carry the *same* id — the header used to be filled from the
workspace name while the body was filled from the bead the coordinator intended,
and three workers each read the body and did bead `.2` inside the `.1`, `.3` and
`.4` worktrees. If you are copying this prompt, replace every `BEAD_ID` in one
pass and re-read the result before you send it: no `BEAD_ID` may survive, and
the id in the header and the id in step 1 must be the same string. Never put a
bare `<id>` in a spawned prompt.

```
You are working bead BEAD_ID. You are in a dedicated git worktree checked out
on branch bd/BEAD_ID. The bead is already claimed by you.

1. `bd show BEAD_ID` and read the whole thing, plus the parent epic's
   coordination note. The description names the exact files and lines; trust
   them. Read `worktree-split-guidelines.md` §3 for the shared-file list and
   confirm none of them is in your change set.
2. Read `AGENTS.md`, then `0001`–`0006` for the standing
   shape. The code is the documentation.
3. TEST FIRST. Write the failing reproduction test the acceptance criteria
   name (most beads name the exact wrong-behaviour case). Run it, watch it fail
   for the right reason. Only then fix.
4. Set your isolated target dir and use it for every cargo invocation:
   export CARGO_TARGET_DIR=target/BEAD_ID
   Confirm: git branch   (must be bd/BEAD_ID, not master)
5. Implement, staying inside the bead's scope. Pure Rust, no FFI (ADR 0001):
   cargo-geiger is the check. Projection math comes from the `proj4rs` crate
   (ADR 0006) — never hand-written. No new third-party crate without an ADR;
   approved crates must be pure Rust, GPL-3.0-compatible, and pinned exact in
   the root `Cargo.toml` `[workspace.dependencies]` table — you do not edit
   that table yourself; report the dependency in your report and let the
   orchestrator take the union at merge time. Keep each crate's `cargo test`
   under ~5 min.
   If you find a second problem, do not fix it — `bd create` a new bead with
   the evidence and carry on.
6. Gates before hand-off:
   - the bead's acceptance command (from `bd show BEAD_ID`) — must exit 0;
   - `./verify test --crate <crate>` for the crate you touched (and its
     dependents), using CARGO_TARGET_DIR above;
   - `./verify clippy --crate <crate>` — zero warnings;
   - `./verify oracle` if the change touches expression / geometry / CRS /
     oracle behaviour, else note "oracle not applicable";
   - `cargo geiger` if Rust code exists, informational (no unsafe / no FFI).
   A suite that fails for a reason unrelated to your change: prove it was
   already failing (name the bead and the failure) rather than editing the
   test to make it pass. A fast-gate run that skips a suite is scoping
   working as designed — CI on master covers what a merge did not run.
   Formatting is not a hand-off step: CI and an occasional run own it.
7. Commit with a `Task: BEAD_ID` trailer and push your branch to origin:
   git commit -m "feat: ... (BEAD_ID)" -m "Task: BEAD_ID"
   git push -u origin bd/BEAD_ID
   Do not open a pull request — the coordinator merges locally. The
   "Create a pull request for ... on GitHub" line the push prints is not an
   instruction.
   Then hand off:
   bd update BEAD_ID --add-label needs-merge --append-notes \
     "<commit-sha> branch bd/BEAD_ID agent <your-id> workspace <workspace-id> \
     (CARGO_TARGET_DIR=target/BEAD_ID)"
8. Sanity check before hand off:
   - `git log -1 --format=%s%n%b` must name `Task: BEAD_ID`;
   - `bd show BEAD_ID` must show *your* bead labelled `needs-merge`.
   If they disagree, you worked the wrong bead — say so in your summary rather
   than papering over it.
9. Report: files changed, tests run with output, CARGO_TARGET_DIR used, any new
   dependency the orchestrator must union in, and remaining risks.

Rules that are not negotiable: no public contract change without contract, SDK
and test updates together; no third-party type crossing a contract; raw SQL is
never reachable from a client-supplied string; a parameter is either honoured or
rejected by name, never accepted and ignored.
```

---

## Notes specific to this workspace

- `eng/` did not previously exist; this file is `eng/runbook.md` and gates are
  invoked from the repo root (`./verify`, `ci.sh`).
- Rust code lives at the repo root (one workspace, 17 crates). `Cargo.lock`
  contention is handled by pinning all external deps in
  `[workspace.dependencies]` (qgis-uic.1); parallel implementers therefore add
  deps to that one table and report them instead of hand-merging lock files.
- Worktrees are Paseo-managed (`isolation=worktree`, `branch-off`). Archive
  them after merge per `worktree-split-guidelines.md` §9. The whole sequence —
  fetch, rebase, `./verify test --crate <name>`, merge, push, `bd close`,
  `paseo archive workspace`, `rm -rf` the worktree, confirm removal — is one
  script:

  ```bash
  ./merge-and-cleanup --branch bd/<id> --worktree ~/.paseo/worktrees/<slug>/bd-<id> \
      --crate <crate> --bead <id> --workspace <workspace-id>
  ```

  It stops at the first gate failure, rebase conflict, or merge conflict, and
  leaves the worktree in place for the worker to resolve. `--dry-run` prints the
  plan; `--keep-worktree` merges and closes without deleting; `--no-verify`
  skips the gate when it already passed. This complements
  `tools/bd-merge-bead.py` (which does not know about workspace ids or
  worktree paths).
- Each worktree sets `CARGO_TARGET_DIR=target/<bead-id>` (or its own
  `target/<bead-id>`) so caches never collide; workers must use it.
- Quality gate wording: the fast gate is `./verify test --crate <name>`; the
  exhaustive lane is `./verify all` (or `ci.sh` with no args). Oracle goldens
  stay single-writer — workers run `./verify oracle` / diff only, the
  orchestrator records.
- Per-bead gates are expected to run well under 5 minutes for a touched crate;
  a crate that exceeds that should be split into smaller beads before dispatch
  (worktree-split-guidelines.md §8).
- If a `WIP:` commit exists from a capped tick, resume from it rather than
  recreating its worktree.
- Lean on `bd ready`, `bd show`, `bd update --append-notes`, and the atomic
  claim (`bd update <id> --claim`) — beads is the scheduler, not the
  coordinator's memory.