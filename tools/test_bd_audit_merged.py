#!/usr/bin/env python3
"""Regression tests for `tools/bd-audit-merged.py` (bead qgis-2yw).

The audit this pins: a bead closed with a merge-ish close reason
("merged", "work complete", "already merged", "satisfied by merged epic")
claims its work landed, but the deliverable file named in its description is
absent from master. qgis-ggs is the canonical case: closed "branch
bd/qgis-ggs merged into master; work complete" while
`gui/examples/headless_acceptance.rs` existed only in a preserved WIP commit
(see qgis-cyk).

The audit must:

  * flag qgis-ggs (deliverable absent by path and by basename);
  * not flag a bead whose deliverable is present on master;
  * not flag a stale/renamed path when a file with the same basename lives at
    a new location (the rust/ -> root and crates/ -> root moves);
  * not flag a non-merge-ish close reason (superseded, duplicate, container);
  * not mistake a model name / worktree / build path for a deliverable file.

Run: python3 tools/test_bd_audit_merged.py
"""
from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

TOOL = Path(__file__).resolve().with_name("bd-audit-merged.py")


def _load_tool():
    spec = importlib.util.spec_from_file_location("bd_audit_merged", TOOL)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


audit = _load_tool()


def bead(bead_id, reason, description):
    return {
        "id": bead_id,
        "status": "closed",
        "close_reason": reason,
        "description": description,
    }


# A plausible slice of master's tree for the tests.
MASTER = [
    "Cargo.toml",
    "manifest.toml",
    "gui/src/main.rs",
    "gui/src/layer_renderer.rs",
    "gui/examples/headless_scenario.rs",
    "crs/src/facade.rs",
    "oracle/goldens/trivial.expected",
    "providers/src/sqlserver.rs",
]


class MergeishDetection(unittest.TestCase):
    def test_mergeish_reasons(self):
        for reason in (
            "Merged",
            "merged and pushed",
            "Merged into master",
            "Merged into master, gate passed",
            "already merged",
            "Already merged to master (commit evidence)",
            "branch bd/qgis-ggs merged into master; work complete",
            "Satisfied by merged epic qgis-uzq: ...",
        ):
            with self.subTest(reason=reason):
                self.assertTrue(audit.is_mergeish(reason))

    def test_non_mergeish_reasons(self):
        for reason in (
            "Closed",
            "Superseded by qgis-rez (duplicate epic)",
            "Duplicate of merged qgis-99n.7 (already on master)",
            "All children complete",
            "Deferred gap rows added to ADR 0006",
        ):
            with self.subTest(reason=reason):
                self.assertFalse(audit.is_mergeish(reason))


class PathExtraction(unittest.TestCase):
    def test_extracts_repo_relative_file(self):
        desc = "Create gui/examples/headless_acceptance.rs with scenarios."
        self.assertIn("gui/examples/headless_acceptance.rs",
                      audit.extract_paths(desc))

    def test_ignores_model_name(self):
        desc = "agent opencode-go/longcat-2.5-preview-free workspace"
        self.assertEqual(audit.extract_paths(desc), [])

    def test_ignores_build_and_worktree_paths(self):
        desc = ("target/qgis-qxu.3.3.1 and /home/james/.paseo/worktrees/x "
                "and $CARGO_TARGET_DIR")
        self.assertEqual(audit.extract_paths(desc), [])

    def test_strips_relative_prefixes(self):
        desc = "path is <CARGO_MANIFEST_DIR>/../../fixtures/manifest.toml here"
        self.assertIn("fixtures/manifest.toml", audit.extract_paths(desc))


class AuditFindings(unittest.TestCase):
    def test_qgis_ggs_shape_is_reported(self):
        beads = [bead(
            "qgis-ggs",
            "branch bd/qgis-ggs merged into master; work complete",
            "Deliverables: gui/examples/headless_acceptance.rs",
        )]
        gaps = audit.audit(beads, MASTER)
        self.assertEqual([g.bead_id for g in gaps], ["qgis-ggs"])
        self.assertEqual(gaps[0].missing, ["gui/examples/headless_acceptance.rs"])

    def test_present_deliverable_is_not_reported(self):
        beads = [bead(
            "qgis-cci", "merged and pushed",
            "Create gui/src/main.rs as an iced application.",
        )]
        self.assertEqual(audit.audit(beads, MASTER), [])

    def test_renamed_path_is_not_reported(self):
        # rust/Cargo.toml is stale; Cargo.toml exists at the root.
        beads = [bead(
            "qgis-qxu.2.1", "Already merged to master (commit evidence)",
            "Pin proj4rs in rust/Cargo.toml [workspace.dependencies].",
        )]
        self.assertEqual(audit.audit(beads, MASTER), [])

    def test_non_mergeish_close_is_ignored(self):
        beads = [bead(
            "qgis-sup", "Superseded by qgis-rez (duplicate epic)",
            "Deliverables: gui/examples/gone.rs",
        )]
        self.assertEqual(audit.audit(beads, MASTER), [])

    def test_open_bead_is_ignored(self):
        b = bead("qgis-open", "merged and pushed",
                 "Deliverables: gui/examples/gone.rs")
        b["status"] = "open"
        self.assertEqual(audit.audit([b], MASTER), [])


class Report(unittest.TestCase):
    def test_format_lists_missing_paths(self):
        gaps = [audit.Gap("qgis-ggs", "merged; work complete",
                          ["gui/examples/headless_acceptance.rs"])]
        text = audit.format_report(gaps)
        self.assertIn("qgis-ggs", text)
        self.assertIn("gui/examples/headless_acceptance.rs", text)

    def test_format_clean_when_no_gaps(self):
        self.assertIn("no gaps", audit.format_report([]))


class CommandLine(unittest.TestCase):
    def _write_inputs(self, tmp, beads):
        import json
        beads_json = Path(tmp) / "beads.json"
        master_txt = Path(tmp) / "master.txt"
        beads_json.write_text(json.dumps(beads))
        master_txt.write_text("\n".join(MASTER) + "\n")
        return beads_json, master_txt

    def test_offline_strict_exits_nonzero_on_gap(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            beads_json, master_txt = self._write_inputs(tmp, [bead(
                "qgis-ggs",
                "branch bd/qgis-ggs merged into master; work complete",
                "New file gui/examples/headless_acceptance.rs",
            )])
            rc = audit.main(["--beads-json", str(beads_json),
                             "--master-files", str(master_txt), "--strict"])
            self.assertEqual(rc, 1)

    def test_offline_clean_exits_zero(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            beads_json, master_txt = self._write_inputs(tmp, [bead(
                "qgis-cci", "merged and pushed",
                "Create gui/src/main.rs as an iced application.",
            )])
            rc = audit.main(["--beads-json", str(beads_json),
                             "--master-files", str(master_txt), "--strict"])
            self.assertEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()
