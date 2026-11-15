"""Offline checks of the scanner's plumbing; no model calls, no API key."""

import json
from pathlib import Path
import tempfile
import unittest

import security_scan


class ScanPlumbingTests(unittest.TestCase):
    def test_merge_orders_findings_and_tags_areas(self):
        low = {"severity": "low", "title": "a"}
        high = {"severity": "high", "title": "b"}
        merged = security_scan.merge([
            {"area": "records", "status": "completed",
             "review": {"summary": "s1", "findings": [low], "limitations": ["l1"]}},
            {"area": "handshake", "status": "completed",
             "review": {"summary": "s2", "findings": [high], "limitations": []}},
            {"area": "cli-servers", "status": "failed: TimeoutError; HTTP status: n/a"},
        ])
        self.assertEqual([f["title"] for f in merged["findings"]], ["b", "a"])
        self.assertIn("[records] s1", merged["summary"])
        self.assertIn("[cli-servers] No review: failed: TimeoutError; HTTP status: n/a",
                      merged["limitations"])
        self.assertIsNone(security_scan.merge([{"area": "records", "status": "failed: x"}]))

    def test_checks_are_validated_and_truncated(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "checks.jsonl"
            path.write_text(json.dumps({"title": "gitleaks", "exit_code": 0, "command": "c",
                                        "last_line": "x" * 500, "output_tail": "y" * 20000}) + "\n\n")
            [check] = security_scan.load_checks(path)
            self.assertEqual(len(check["last_line"]), 200)
            self.assertEqual(len(check["output_tail"]), 8000)
            path.write_text(json.dumps({"title": "t", "exit_code": "1"}) + "\n")
            with self.assertRaises(ValueError):
                security_scan.load_checks(path)

    def test_source_ref_is_validated(self):
        for ref in ("--output=x", "-x", "a b", "a;b", ""):
            with self.assertRaises(ValueError):
                security_scan.resolve_source(ref)
        self.assertIsNone(security_scan.resolve_source(None))

    def test_scopes_stay_within_budget(self):
        for name, area in security_scan.AREAS.items():
            for file in area.files:
                self.assertTrue((security_scan.ROOT / file).is_file(), file)
        worst = len(security_scan.SCOPES["all"]) * security_scan.cost(
            security_scan.INPUT_LIMIT, security_scan.THINKING_OUTPUT_LIMIT)
        self.assertLessEqual(worst, security_scan.MAX_RUN_USD)


if __name__ == "__main__":
    unittest.main()
