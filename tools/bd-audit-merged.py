#!/usr/bin/env python3
"""Audit closed beads whose close reason claims a merge.

Closing a bead is the only signal that its work landed. When a bead is closed
with a merge-ish reason ("merged", "merged and pushed", "already merged",
"work complete", "satisfied by merged epic") but the deliverable file named in
its description is absent from master, the close hides lost work until a
downstream bead trips over it.

qgis-ggs is the canonical case (see qgis-cyk): it was closed "branch
bd/qgis-ggs merged into master; work complete" while its only deliverable,
`gui/examples/headless_acceptance.rs`, existed solely in a preserved WIP commit
on a different branch.

Usage:
    python3 tools/bd-audit-merged.py                 # live audit against master
    python3 tools/bd-audit-merged.py --strict        # exit 1 when gaps found
    python3 tools/bd-audit-merged.py --beads-json f --master-files f   # offline

The audit is read-only. It never closes or re-closes a bead. Candidate gaps are
printed for a human to triage and file as follow-up beads; a path that only
moved (rust/ -> root, crates/ -> root) is not a gap because a file with the same
basename still exists on master.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# A close reason that asserts the work landed on master.
_MERGEISH = re.compile(r"merg|work complete|satisfied by", re.I)
# Close reasons that reference another bead instead of claiming this bead's
# own deliverable; auditing the referenced bead covers them.
_NOT_MERGEISH = re.compile(r"duplicate|superseded|children complete", re.I)

# A repo-relative file path: at least one directory segment plus a filename.
_PATH_RE = re.compile(r"(?:[A-Za-z0-9_.-]+/)+[A-Za-z0-9_.-]+\.[A-Za-z0-9]+")

# Only these extensions are treated as deliverable files. This rejects model
# names (`longcat-2.5`), build dirs (`target/qgis-qxu.3.3.1`), and versions.
_FILE_EXTS = {
    "rs", "toml", "lock", "md", "py", "sh", "bash", "yml", "yaml", "json",
    "qml", "qgs", "qgz", "txt", "csv", "tsv", "xml", "html", "css", "js",
    "ts", "tsx", "jsx", "sql", "cpp", "cc", "cxx", "h", "hpp", "cmake",
    "proto", "wgsl", "glsl", "vert", "frag", "svg", "png", "jpg", "jpeg",
    "gif", "ico", "ttf", "otf", "woff", "woff2", "expected", "golden",
    "snap", "shp", "shx", "dbf", "prj", "gpkg", "geojson", "cfg", "ini",
    "conf", "env", "example",
}

# Paths under these roots are never deliverables.
_SKIP_SUBSTR = (
    "target/", "node_modules/", ".paseo/", ".cargo/", ".git/",
    "http://", "https://", "github.com/", "qgis.org/",
)
_SKIP_PREFIX = ("/home/", "/Users/", "/tmp/", "/opt/", "/usr/")


class Gap:
    """A closed merge-ish bead whose deliverable is absent from master."""

    def __init__(self, bead_id, reason, missing):
        self.bead_id = bead_id
        self.reason = reason
        self.missing = missing


def is_mergeish(reason: str) -> bool:
    """True when the close reason claims this bead's work landed on master."""
    if _NOT_MERGEISH.search(reason):
        return False
    return bool(_MERGEISH.search(reason))


def _normalize(token: str) -> str:
    parts = [p for p in token.split("/") if p not in ("", ".", "..")]
    return "/".join(parts)


def extract_paths(description: str) -> list[str]:
    """Return repo-relative deliverable file paths named in a description."""
    found: list[str] = []
    for raw in _PATH_RE.findall(description or ""):
        if raw.startswith(_SKIP_PREFIX) or any(s in raw for s in _SKIP_SUBSTR):
            continue
        path = _normalize(raw)
        if not path:
            continue
        ext = path.rsplit(".", 1)[-1].lower()
        if ext not in _FILE_EXTS:
            continue
        if path not in found:
            found.append(path)
    return found


def audit(beads: list[dict], master_files: list[str]) -> list[Gap]:
    """Report closed merge-ish beads whose named files are absent from master."""
    master = set(master_files)
    master_basenames = {Path(f).name for f in master_files}

    gaps: list[Gap] = []
    for bead in beads:
        if bead.get("status") != "closed":
            continue
        reason = bead.get("close_reason") or ""
        if not is_mergeish(reason):
            continue
        missing = [
            path
            for path in extract_paths(bead.get("description") or "")
            if path not in master and Path(path).name not in master_basenames
        ]
        if missing:
            gaps.append(Gap(bead["id"], reason, missing))
    return gaps


def format_report(gaps: list[Gap]) -> str:
    if not gaps:
        return "merged-close audit: no gaps (all named deliverables present on master)"
    lines = [
        f"merged-close audit: {len(gaps)} candidate bead(s) with missing deliverables",
        "(triage each: a path may name another project's file rather than a deliverable)",
        "",
    ]
    for gap in gaps:
        lines.append(f"  {gap.bead_id}: {gap.reason}")
        for path in gap.missing:
            lines.append(f"      missing on master: {path}")
    return "\n".join(lines)


def _run(cmd: list[str]) -> str:
    result = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(f"command failed: {' '.join(cmd)}\n{result.stderr.strip()}")
    return result.stdout


def load_live() -> tuple[list[dict], list[str]]:
    beads = json.loads(_run(["bd", "list", "--all", "--json"]))
    master_files = _run(["git", "ls-tree", "-r", "master", "--name-only"]).splitlines()
    return beads, master_files


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--beads-json", help="offline: bd list --all --json output")
    parser.add_argument("--master-files", help="offline: git ls-tree -r master --name-only")
    parser.add_argument("--strict", action="store_true",
                        help="exit non-zero when gaps are found")
    args = parser.parse_args(argv)

    if args.beads_json or args.master_files:
        if not (args.beads_json and args.master_files):
            parser.error("--beads-json and --master-files must be given together")
        beads = json.loads(Path(args.beads_json).read_text())
        master_files = Path(args.master_files).read_text().splitlines()
    else:
        beads, master_files = load_live()

    gaps = audit(beads, master_files)
    print(format_report(gaps))
    return 1 if (gaps and args.strict) else 0


if __name__ == "__main__":
    sys.exit(main())
