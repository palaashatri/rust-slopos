"""Regression tests for the portable P1 coordinator. No Cargo, VM or network needed."""
from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

MODULE = Path(__file__).with_name("p1_goal.py")
spec = importlib.util.spec_from_file_location("p1_goal", MODULE)
assert spec and spec.loader
p1 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p1)


class GoalContractTests(unittest.TestCase):
    def test_real_manifest_is_acyclic_and_contains_all_release_apps(self):
        m = p1.manifest()
        ids = {t["id"] for t in m["tasks"]}
        for name in ("UI-01", "FILES-02", "IMAGE-02", "MEDIA-02",
                     "PANELS-01", "STORE-02", "SHELL-01", "P1-RELEASE"):
            self.assertIn(name, ids)
        self.assertEqual(m["tasks"][-1]["id"], "P1-RELEASE")

    def test_no_tasks_are_claimed_or_marked_done_by_default(self):
        m = p1.manifest()
        with tempfile.TemporaryDirectory() as temp:
            with patch.object(p1, "PROGRESS", Path(temp) / "state.json"):
                state = p1.progress(m)
                self.assertEqual([t["id"] for t in p1.next_tasks(m, state)], ["P1-00"])
                self.assertTrue(all(p1.task_status(state, t["id"]) == "PLANNED" for t in m["tasks"]))

    def test_duplicate_and_cycle_are_rejected(self):
        m = p1.manifest()
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "manifest.json"
            m["tasks"][1]["deps"] = ["P1-RELEASE"]
            path.write_text(json.dumps(m), encoding="utf-8")
            with patch.object(p1, "GOAL", path), self.assertRaisesRegex(ValueError, "cycle"):
                p1.manifest()
            m["tasks"][1]["deps"] = ["P1-00"]
            m["tasks"][1]["id"] = "P1-00"
            path.write_text(json.dumps(m), encoding="utf-8")
            with patch.object(p1, "GOAL", path), self.assertRaisesRegex(ValueError, "duplicate"):
                p1.manifest()

    def test_tampered_evidence_is_rejected_and_real_file_is_verified(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            evidence = root / ".work/p1/evidence"
            evidence.mkdir(parents=True)
            log = evidence / "unit.log"
            log.write_text("11 tests passed", encoding="utf-8")
            sample = {
                "source_sha": "a" * 40,
                "checks": [{
                    "kind": "unit",
                    "result": "PASS",
                    "command": "python3 -m unittest discover -s scripts",
                    "artifact": ".work/p1/evidence/unit.log",
                    "sha256": hashlib.sha256(log.read_bytes()).hexdigest(),
                }],
            }
            path = root / "proof.json"
            path.write_text(json.dumps(sample), encoding="utf-8")
            with patch.object(p1, "ROOT", root), patch.object(p1, "EVIDENCE", evidence), \
                 patch.object(p1, "current_sha", return_value="a" * 40):
                self.assertEqual(p1.check_evidence(path, ["unit"])["source_sha"], "a" * 40)
                with self.assertRaisesRegex(ValueError, "required proof missing"):
                    p1.check_evidence(path, ["unit", "visual"])
                log.write_text("corrupted", encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "artifact bytes changed"):
                    p1.check_evidence(path, ["unit"])

    def test_evidence_must_not_escape_allowlisted_directory(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            evidence = root / ".work/p1/evidence"
            evidence.mkdir(parents=True)
            private = root / "secret.txt"
            private.write_text("secret", encoding="utf-8")
            with patch.object(p1, "ROOT", root), patch.object(p1, "EVIDENCE", evidence):
                with self.assertRaisesRegex(ValueError, "under .work/p1/evidence"):
                    p1.safe_evidence_path("secret.txt")

    def test_progress_survives_reload_and_does_not_drop_other_tasks(self):
        with tempfile.TemporaryDirectory() as temp:
            state_path = Path(temp) / "qa/p1-progress.json"
            with patch.object(p1, "PROGRESS", state_path):
                m = p1.manifest()
                state = p1.progress(m)
                state["tasks"]["P1-00"] = {"status": "CLAIMED", "base_sha": "a" * 40}
                p1.write_progress(state)
                reread = p1.progress(m)
                self.assertEqual(p1.task_status(reread, "P1-00"), "CLAIMED")
                self.assertEqual(p1.task_status(reread, "UI-01"), "PLANNED")
                self.assertEqual(p1.next_tasks(m, reread), [])


if __name__ == "__main__":
    unittest.main()
