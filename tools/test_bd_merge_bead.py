#!/usr/bin/env python3
"""Regression tests for `tools/bd-merge-bead.py` (bead qgis-9gw).

The false close this pins: on the qgis-cyk / qgis-ggs incident the tool saw
the runbook branch `bd/qgis-ggs` equal to `origin/master`, treated that as
"already merged", and closed the bead — while the only copy of the
deliverable (`gui/examples/headless_acceptance.rs`) sat in WIP commit
b90b444 on the paseo worktree branch `bd-qgis-ggs-1`, unmerged.

The shape, from the bead evidence:

    git rev-parse bd/qgis-ggs bd-qgis-ggs-1 origin/master
    # bd/qgis-ggs   b28e842  (== origin/master)
    # bd-qgis-ggs-1 b90b444  (deliverable, unmerged)

A runbook branch that never left master must not certify "already merged"
while a sibling branch of the same bead carries commits `origin/master`
cannot reach. The tool must refuse to close and name the stranded branch.

The tests run the real `main()` against a scratch git repo carrying this
exact ref layout; `bd` calls are recorded, never executed.

Run: python3 tools/test_bd_merge_bead.py
"""
from __future__ import annotations

import contextlib
import io
import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path

TOOL = Path(__file__).resolve().with_name("bd-merge-bead.py")

BEAD = "qgis-ggs"


def _load_tool():
    spec = importlib.util.spec_from_file_location("bd_merge_bead", TOOL)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


tool = _load_tool()


def git(repo, *args):
    return subprocess.run(["git", *args], cwd=repo, capture_output=True,
                          text=True)


def make_remote(path):
    """A bare origin with one commit on master."""
    git(path.parent, "init", "--bare", "-b", "master", str(path))
    return path


def make_repo(remote, *, runbook_branch=True):
    """A work repo whose branch layout matches the qgis-ggs incident.

    With `runbook_branch` the runbook branch `bd/<id>` exists at master
    (the incident shape); without it, only the paseo slug variant
    `bd-<slug>` exists — also at master — and the deliverable is stranded
    on `bd-<slug>-1` either way.
    """
    work = remote.parent / "work"
    git(remote.parent, "clone", str(remote), str(work))
    git(work, "config", "user.email", "test@example.com")
    git(work, "config", "user.name", "test")
    if runbook_branch:
        git(work, "branch", f"bd/{BEAD}")
    git(work, "branch", f"bd-{BEAD}")
    git(work, "checkout", "-b", f"bd-{BEAD}-1")
    (work / "deliverable.rs").write_text("// gui/examples/headless_acceptance.rs\n")
    git(work, "add", "deliverable.rs")
    git(work, "commit", "-m", "WIP: deliverable for qgis-ggs")
    for ref in ([f"bd/{BEAD}", f"bd-{BEAD}", f"bd-{BEAD}-1"]
                if runbook_branch else [f"bd-{BEAD}", f"bd-{BEAD}-1"]):
        git(work, "push", "origin", ref)
    return work


class Recorder:
    """Redirect the tool's subprocesses at a scratch repo; record bd calls.

    Git commands run for real in the scratch repo (the strongest form of
    regression test: the real `main()` meets the real incident refs).
    `bd` calls are captured and answered with success so no live queue is
    ever written; a test can still assert exactly which `bd` invocations
    the tool attempted.
    """

    def __init__(self, tool_module, repo):
        self.real_run = subprocess.run
        self.repo = repo
        self.bd_calls: list[list[str]] = []
        self._tool = tool_module

    def __enter__(self):
        def fake_sh(cmd, cwd=None):
            if cmd and cmd[0] == "bd":
                self.bd_calls.append(cmd)
                return subprocess.CompletedProcess(cmd, 0, "", "")
            return self.real_run(cmd, cwd=cwd or self.repo,
                                 capture_output=True, text=True)
        self._original_sh = self._tool.sh
        self._tool.sh = fake_sh
        return self

    def __exit__(self, *exc):
        self._tool.sh = self._original_sh
        return False


def run_tool(repo, *argv):
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        code = tool.main(list(argv))
    return code, out.getvalue()


class StrandedWorkIsNotAlreadyMerged(unittest.TestCase):
    def test_runbook_branch_at_master_does_not_close_bead(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            repo = make_repo(make_remote(tmp / "origin.git"))
            with Recorder(tool, repo) as rec:
                code, out = run_tool(repo, BEAD)

        self.assertEqual(code, 1, out)
        self.assertIn(f"bd-{BEAD}-1", out)
        self.assertFalse(
            [c for c in rec.bd_calls if c[:2] == ["bd", "close"]],
            f"tool attempted bd close: {rec.bd_calls}")
        # The refusal is recorded on the bead for the coordinator's notes.
        self.assertTrue(
            [c for c in rec.bd_calls if c[:2] == ["bd", "update"]],
            f"tool recorded no note: {rec.bd_calls}")

    def test_stranded_work_without_runbook_branch_also_refuses(self):
        # Same incident minus `bd/<id>`: the slug variant `bd-<slug>` is
        # at master and `bd-<slug>-1` still holds the deliverable.
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            repo = make_repo(make_remote(tmp / "origin.git"),
                             runbook_branch=False)
            with Recorder(tool, repo) as rec:
                code, out = run_tool(repo, BEAD)

        self.assertEqual(code, 1, out)
        self.assertIn(f"bd-{BEAD}-1", out)
        self.assertFalse(
            [c for c in rec.bd_calls if c[:2] == ["bd", "close"]],
            f"tool attempted bd close: {rec.bd_calls}")


class GenuinelyMergedStillCloses(unittest.TestCase):
    def test_clean_idempotent_close_still_fires(self):
        # The idempotent path must survive the fix: a bead whose only
        # branch is the runbook branch, merged into master, closes.
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            remote = make_remote(tmp / "origin.git")
            repo = remote.parent / "work"
            git(remote.parent, "clone", str(remote), str(repo))
            git(repo, "config", "user.email", "test@example.com")
            git(repo, "config", "user.name", "test")
            git(repo, "branch", f"bd/{BEAD}")
            git(repo, "push", "origin", f"bd/{BEAD}")
            with Recorder(tool, repo) as rec:
                code, out = run_tool(repo, BEAD)

        self.assertEqual(code, 0, out)
        self.assertTrue(
            [c for c in rec.bd_calls
             if c[:4] == ["bd", "close", BEAD, "--reason"]],
            f"tool did not close the merged bead: {rec.bd_calls}")


if __name__ == "__main__":
    unittest.main()
