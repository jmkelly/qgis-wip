---
name: qgis-swarm-tick
description: Coordinator runbook for the QGIS Rust swarm tick. Use when running or reviewing a swarm coordinator tick — drives publish gate, worktree cleanup, worker trim, bead recovery, gated merge, and ready-queue drain in fixed order.
agent: opencode
---

# qgis-swarm-tick — Coordinator Tick Runbook

The swarm coordinator runs a fixed multi-step tick every cycle. This skill
pins the order so runs do not drift and cleanup is never skipped (skipped
cleanup causes worktree pile-up).

Previously the tick lived only in the schedule prompt, which was easy to
edit inconsistently and hard to review. Treat this file as the source of
truth; if the schedule prompt disagrees, fix the prompt.

## Tick order (run all six, in order)

### 1. Publish gate

Push local beads state before doing anything else so workers see fresh
assignments and no decision is made on stale data:

```bash
bd dolt push
```

If the publish gate fails (network down, conflict), stop the tick and
report — do not continue on stale state.

### 2. Clean stale worktrees

List Paseo workspaces/worktrees and remove ones whose agent is closed,
archived, or whose bead is already merged/closed. Verify the worktree
branch is fully merged before deleting:

```bash
paseo_list_workspaces
git branch --merged master  # per worktree repo, before rm -rf
```

Skipping this step is what caused the historical worktree pile-up.

### 3. Trim to 3 workers

Cap concurrent workers at 3. If more than 3 worker agents are alive,
keep the 3 with the most recent progress and cancel/archive the rest.
Never spawn a 4th concurrent worker.

See "3-worker cap rationale" below.

### 4. Recover stale beads

Find beads that are `in_progress` but whose worker is dead, idle past
lease, or whose worktree is gone. For each:

- Check agent activity / lease expiry (`bd show <id>`).
- If the worker is truly gone, release the bead (`bd update <id>
  --release` or reset status) so it returns to the ready queue.
- If the worker is alive but quiet, leave it alone — do not steal live work.

### 5. Merge needs-merge with verify gate

For each bead labeled `needs-merge`:

1. Inspect the branch diff against master.
2. Run the matching `./verify` gate (see `verify-qgis` skill — at
   minimum the crate-scoped tests for the changed crate; docs-only
   changes skip the build).
3. Only on a green gate: merge to master, then `bd close <id>`.
4. On a red gate: leave the label, append the failure artifact as a
   bead note, and move on.

Never merge without the verify gate. Never close a bead that was not merged.

### 6. Drain ready queue

Fill free worker slots (up to the 3-worker cap) from the ready queue:

```bash
bd ready
```

Claim atomically (`bd update <id> --claim`) and spawn one worker per
bead with the bead context in the initial prompt. Highest priority
first; do not over-fill.

## 3-worker cap rationale

- **Merge bandwidth:** the coordinator merges serially with a verify
  gate per bead; more than 3 workers produces `needs-merge` faster
  than the coordinator can drain it.
- **Worktree pressure:** each worker owns a worktree; past runs showed
  4+ workers cause stale-worktree pile-up and disk/conflict churn.
- **Review quality:** 3 parallel diffs is the most a coordinator can
  meaningfully review per tick without rubber-stamping.
- Evidence: coordinator run a57b9018 plus 5+ prior coordinator runs.

Raise the cap only with measured evidence that merge keeps up; lower it
to 1–2 when merges are backlogged.

## Stop conditions

Stop the tick (do not proceed to later steps) when any stop condition holds:

1. **Publish gate failed** — state is stale; report and stop.
2. **No free worker slots and no stale work to recover** — nothing to
   drain; end the tick after steps 1–5.
3. **Ready queue is empty** — nothing to spawn; end after step 5.
4. **Verify gate red on all merge candidates** — leave labels, record
   artifacts, do not force-merge; end the tick.
5. **Explicit operator stop** (`cancel_agent`, "stop the swarm", or
   schedule paused) — halt immediately, leave beads as-is.

Each stop condition is terminal for this tick only; the next scheduled
tick retries from step 1.
