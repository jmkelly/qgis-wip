#!/usr/bin/env python3
"""Regression tests for `tools/bd-safe-reclaim.py` (bead qgis-23u).

The false positive this pins: on 2026-10-16 the reaper reported WOULD reclaim
`qgis-5gj` while its worker (`134ca49c`) was running in that bead's worktree.

Two independent mismatches caused both liveness arms to miss:

  * `paseo workspace ls` names a worktree workspace with a human title
    ("qgis-5gj headless driver"), never the branch `bd/<id>`, so
    `branch_worktree()` always returned None.
  * paseo disambiguated the worktree directory to `bd-qgis-5gj-1`, and the
    slug fallback compared the final path segment *exactly* against
    `bd-qgis-5gj`, so the live agent never matched.

A live worker must be KEEP; a stale lease with no live agent must still be
WOULD/RECLAIM.

Run: python3 tools/test_bd_safe_reclaim.py
"""
from __future__ import annotations

import importlib.util
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

TOOL = Path(__file__).resolve().with_name("bd-safe-reclaim.py")


def _load_tool():
    spec = importlib.util.spec_from_file_location("bd_safe_reclaim", TOOL)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


reclaim = _load_tool()

NOW = datetime(2026, 10, 16, 8, 0, tzinfo=timezone.utc)
GRACE = timedelta(minutes=10)


def stale_bead(bead_id, notes=""):
    """An in_progress bead whose lease expired an hour ago."""
    return {
        "id": bead_id,
        "labels": [],
        "notes": notes,
        "lease_expires_at": (NOW - timedelta(hours=1)).isoformat(),
    }


def agent(agent_id, cwd, status="running"):
    return {
        "id": agent_id,
        "shortId": agent_id[:7],
        "cwd": cwd,
        "status": status,
    }


# The exact shapes observed with `paseo ls --json` / `paseo workspace ls --json`
# while qgis-5gj's worker was live.
LIVE_AGENT_ID = "134ca49c-b470-49d0-84cd-db9adcb86c1a"
LIVE_AGENT_CWD = "~/.paseo/worktrees/1a7z1cik/bd-qgis-5gj-1"
TITLED_WORKSPACE = {
    "name": "qgis-5gj headless driver",
    "cwd": "/home/james/.paseo/worktrees/1a7z1cik/bd-qgis-5gj-1",
}


class LiveWorkerIsKept(unittest.TestCase):
    def test_titled_workspace_and_numbered_worktree_hold_the_bead(self):
        bead = stale_bead("qgis-5gj",
                          notes=f"agent {LIVE_AGENT_ID} branch bd/qgis-5gj")
        agents = [agent(LIVE_AGENT_ID, LIVE_AGENT_CWD)]
        workspaces = [TITLED_WORKSPACE]

        self.assertIsNotNone(reclaim.protect_reason(bead, agents, workspaces))
        decisions = reclaim.plan([bead], agents, workspaces, NOW, GRACE)
        self.assertEqual(len(decisions), 1)
        self.assertFalse(decisions[0].reclaim)

    def test_numbered_suffix_is_the_same_bead(self):
        # No notes, no workspace listing: the slug fallback alone must hold.
        bead = stale_bead("qgis-5gj")
        agents = [agent(LIVE_AGENT_ID, LIVE_AGENT_CWD)]
        self.assertIsNotNone(reclaim.protect_reason(bead, agents, []))

    def test_numbered_suffix_does_not_match_a_different_worktree(self):
        bead = stale_bead("qgis-5gj")
        agents = [agent(LIVE_AGENT_ID,
                        "~/.paseo/worktrees/1a7z1cik/bd-qgis-5gj-extra")]
        self.assertIsNone(reclaim.protect_reason(bead, agents, []))

    def test_numbered_suffix_does_not_match_a_shorter_bead_id(self):
        # `bd-qgis-5gj-1` must not be read as bead `qgis-5g`.
        bead = stale_bead("qgis-5g")
        agents = [agent(LIVE_AGENT_ID, LIVE_AGENT_CWD)]
        self.assertIsNone(reclaim.protect_reason(bead, agents, []))


class DeadWorkerIsReclaimed(unittest.TestCase):
    def test_stale_lease_without_live_agent_is_would(self):
        bead = stale_bead("qgis-dead")
        decisions = reclaim.plan([bead], [], [], NOW, GRACE)
        self.assertEqual(len(decisions), 1)
        self.assertTrue(decisions[0].reclaim)

    def test_closed_agent_does_not_hold_the_bead(self):
        bead = stale_bead("qgis-dead")
        agents = [agent(LIVE_AGENT_ID,
                        "~/.paseo/worktrees/1a7z1cik/bd-qgis-dead-2",
                        status="closed")]
        decisions = reclaim.plan([bead], agents, [], NOW, GRACE)
        self.assertEqual(len(decisions), 1)
        self.assertTrue(decisions[0].reclaim)


class NotesArm(unittest.TestCase):
    def test_notes_agent_with_numbered_worktree_holds(self):
        bead = stale_bead("qgis-5gj", notes="agent 134ca49")
        agents = [agent(LIVE_AGENT_ID, LIVE_AGENT_CWD)]
        self.assertIsNotNone(reclaim.protect_reason(bead, agents, []))

    def test_notes_agent_elsewhere_is_not_a_hold(self):
        # A parent epic's notes name many agents; only the one in *this*
        # worktree holds the bead.
        bead = stale_bead("qgis-5gj", notes="agent 134ca49")
        agents = [agent(LIVE_AGENT_ID,
                        "~/.paseo/worktrees/1a7z1cik/bd-other-1")]
        self.assertIsNone(reclaim.protect_reason(bead, agents, []))


if __name__ == "__main__":
    unittest.main()
