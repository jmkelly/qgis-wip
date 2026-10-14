---
name: qgis-swarm-tick
description: Coordinator runbook for the QGIS Rust swarm tick. Use when running or reviewing a swarm coordinator tick — drives publish gate, worktree cleanup, worker trim, bead recovery, gated merge, and ready-queue drain in fixed order.
agent: opencode
---

# qgis-swarm-tick — Coordinator Tick Runbook

The tick is defined in [`eng/runbook.md`](../../../eng/runbook.md). Read it and
follow it exactly — it is the single source of truth for the tick order, the
worker cap, stop conditions, and the verbatim worker prompt.

If this skill and the runbook ever disagree, the runbook wins.
