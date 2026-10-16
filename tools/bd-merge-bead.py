#!/usr/bin/env python3
"""Merge a bead's branch into master, gate, push, and close the bead.

Usage:
    python3 tools/bd-merge-bead.py <bead-id>
    python3 tools/bd-merge-bead.py --bead <bead-id> [--crate <crate>] [--full]
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def sh(cmd, cwd=ROOT):
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)


def git(*args, cwd=ROOT):
    return sh(["git", *args], cwd=cwd)


def bead_branch(bead_id):
    for b in (f"bd/{bead_id}", f"bd-{bead_id}", bead_id):
        if git("rev-parse", "--verify", f"origin/{b}").returncode == 0:
            return b
        if git("rev-parse", "--verify", b).returncode == 0:
            return b
    return f"bd/{bead_id}"


def note(bead_id, text):
    sh(["bd", "update", bead_id, "--append-notes", text])


def main():
    parser = argparse.ArgumentParser(description="Merge a bead (branch) into origin/master with gates.")
    parser.add_argument("bead", nargs="?", help="bead id to merge")
    parser.add_argument("--bead", dest="bead_opt", help="bead id (alias)")
    parser.add_argument("--crate", default="core", help="crate to run ./verify test --crate on")
    parser.add_argument("--full", action="store_true", help="run full lane")
    args = parser.parse_args()

    bead_id = args.bead or args.bead_opt
    if not bead_id:
        parser.error("bead id is required")

    branch = bead_branch(bead_id)
    base = "origin/master"
    main_ref = "master"

    r = git("fetch", "origin")
    if r.returncode != 0:
        print(f"fetch failed: {r.stderr.strip()}")
        return 1
    git("fetch", "origin", branch)

    ref_branch = branch if git("rev-parse", "--verify", branch).returncode == 0 else f"origin/{branch}"
    # already-merged (idempotent)
    r = git("merge-base", "--is-ancestor", ref_branch, base)
    if r.returncode == 0:
        sh(["bd", "close", bead_id, "--reason", "already merged"])
        print(f"{bead_id}: already merged into {base}")
        return 0

    # publish gate
    r = git("rev-list", "--count", f"{base}..{main_ref}")
    if r.returncode == 0 and r.stdout.strip() not in ("0", ""):
        print("local master has unpublished commits; push first")
        return 1

    r = git("checkout", main_ref)
    if r.returncode != 0:
        print(r.stderr.strip()); return 1
    r = git("merge", "--ff-only", base)
    if r.returncode != 0:
        print(r.stderr.strip()); return 1

    if git("rev-parse", "--verify", branch).returncode != 0:
        git("checkout", "-b", branch, f"origin/{branch}")
    else:
        git("checkout", branch)
    git("reset", "--hard", f"origin/{branch}")

    r = git("rebase", base)
    if r.returncode != 0:
        git("rebase", "--abort")
        note(bead_id, "Blocked: rebase conflict")
        print("rebase conflict; aborted")
        return 1

    verify_cmd = ["./verify", "test", "--crate", args.crate]
    r = sh(verify_cmd, cwd=ROOT)
    if r.returncode != 0:
        note(bead_id, f"Gate failed: {(r.stderr.strip() or r.stdout.strip())[:500]}")
        print("gate failed; bead left open")
        return 1

    git("checkout", main_ref)
    r = git("merge", "--no-ff", branch, "-m", f"Merge {bead_id}")
    if r.returncode != 0:
        print(r.stderr.strip()); return 1

    r = git("push", "origin", main_ref)
    if r.returncode != 0:
        note(bead_id, f"Push failed: {r.stderr.strip()}")
        print("push failed; bead left open")
        return 1

    sh(["bd", "close", bead_id, "--reason", "merged and pushed"])
    print(f"{bead_id}: merged and closed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
