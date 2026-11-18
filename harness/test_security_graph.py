"""Offline checks of the graph scan and the tool runner; no model calls, no API key.

The model tests need pydantic-ai (harness/security-scan-requirements.txt) and
are skipped without it.
"""

import asyncio
import json
import unittest

import render_security_report as render
import security_graph as graph
import security_tools as tools

try:
    from pydantic_ai.messages import ModelResponse, TextPart
    from pydantic_ai.models.function import FunctionModel
except ImportError:  # Plain python3 without the scan's virtualenv.
    FunctionModel = None


def fixed_model(payload):
    def respond(_messages, _info):
        return ModelResponse(parts=[TextPart(json.dumps(payload))])

    async def stream(_messages, _info):
        yield json.dumps(payload)
    return FunctionModel(respond, stream_function=stream)


class PlanTests(unittest.TestCase):
    def test_modules_exist_and_lenses_are_known(self):
        for name, module in graph.MODULES.items():
            self.assertTrue(module.lenses, name)
            for lens in module.lenses:
                self.assertIn(lens, graph.LENSES)
            for file in module.files:
                self.assertTrue((graph.ROOT / file).is_file(), file)

    def test_worst_case_within_graph_budget(self):
        plan = graph.plan_scanners()
        self.assertGreaterEqual(len(plan), 20)
        self.assertEqual(len(plan), len(set(plan)))
        self.assertLessEqual(graph.worst_case(len(plan)), graph.MAX_GRAPH_USD)
        # Each scanner stays in Haiku's cheaper price tier.
        self.assertLess(graph.SCANNER_INPUT_LIMIT, 100_000)

    def test_scanner_context_is_filtered_by_module_and_lens(self):
        records = [
            {"tool": "clippy-panics", "status": "findings", "version": "", "summary": "s",
             "findings": [tools.finding("clippy::indexing_slicing", "info", "m",
                                        file="crates/golem-p2p/src/wire.rs", line=3),
                          tools.finding("clippy::expect_used", "info", "m",
                                        file="crates/golem-p2p/src/record.rs", line=4)]},
            {"tool": "gitleaks", "status": "findings", "version": "", "summary": "1 finding",
             "findings": [tools.finding("generic-api-key", "high", "x",
                                        file="crates/golem-p2p/src/wire.rs", line=9)]},
            {"tool": "semgrep", "status": "missing", "version": "", "summary": "not run", "findings": []},
        ]
        module = graph.MODULES["handshake"]
        panics = graph.scanner_tool_context(records, module, "panics")
        self.assertIn("wire.rs:3 clippy::indexing_slicing", panics)
        self.assertNotIn("record.rs:4", panics)
        self.assertNotIn("generic-api-key", panics)
        self.assertIn("semgrep : NOT RUN (missing)", panics)
        self.assertNotIn("clippy::", graph.scanner_tool_context(records, module, "crypto").split("\n", 2)[2])
        self.assertIn("generic-api-key", graph.scanner_tool_context(records, module, "leaks"))

    def test_summary_checks_flag_unmerged_and_bad_locations(self):
        findings = [{"id": "records/dos#1"}, {"id": "records/auth#1"}]
        summary = {"candidates": [{"title": "t", "sources": ["records/dos#1"],
                                   "locations": [{"file": "a.rs", "line": 50}]}],
                   "noise": []}
        problems = graph.check_summary(summary, findings, {"a.rs": 10})
        self.assertEqual(len(problems), 2)
        self.assertIn("records/auth#1", problems[1])


class ToolParsingTests(unittest.TestCase):
    def test_cargo_audit(self):
        data = {"vulnerabilities": {"list": [{"advisory": {"id": "RUSTSEC-1", "title": "bad"},
                                              "package": {"name": "a", "version": "1"}}]},
                "warnings": {"unmaintained": [{"kind": "unmaintained", "advisory": {"id": "RUSTSEC-2", "title": "old"},
                                               "package": {"name": "b", "version": "2"}}]}}
        found = tools.parse_cargo_audit(data)
        self.assertEqual([(f["rule"], f["severity"]) for f in found],
                         [("RUSTSEC-1", "high"), ("RUSTSEC-2", "low")])

    def test_cargo_deny(self):
        text = "\n".join([
            json.dumps({"type": "diagnostic", "fields": {"severity": "error", "code": "unmaintained",
                                                         "message": "x", "advisory": {"id": "RUSTSEC-3"},
                                                         "labels": [{"span": "c 1"}]}}),
            json.dumps({"type": "diagnostic", "fields": {"severity": "warning", "code": "duplicate"}}),
            "not json",
            json.dumps({"type": "summary", "fields": {"advisories": {"errors": 1}}}),
        ])
        found, warnings, summary = tools.parse_cargo_deny(text)
        self.assertEqual(found[0]["rule"], "RUSTSEC-3")
        self.assertEqual(warnings["duplicate"], 1)
        self.assertEqual(summary["advisories"]["errors"], 1)

    def test_osv(self):
        data = {"results": [{"source": {"path": str(tools.ROOT / "Cargo.lock")}, "packages": [{
            "package": {"name": "a", "version": "1"},
            "vulnerabilities": [{"id": "GHSA-1", "summary": "s"}],
            "groups": [{"ids": ["GHSA-1"], "max_severity": "7.5"}]}]}]}
        [found] = tools.parse_osv(data)
        self.assertEqual((found["file"], found["severity"], found["message"]), ("Cargo.lock", "high", "s"))

    def test_gitleaks_keeps_no_secret(self):
        item = {"RuleID": "r", "File": "f", "StartLine": 3, "Commit": "abc", "Description": "d",
                "Secret": "s3cr3t-value", "Match": "key=s3cr3t-value", "Line": "key=s3cr3t-value"}
        [found] = tools.sanitize_gitleaks([item], "history")
        self.assertNotIn("s3cr3t", json.dumps(found))
        self.assertEqual((found["rule"], found["file"], found["line"]), ("r", "f", 3))

    def test_unsafe_regex(self):
        self.assertTrue(tools.UNSAFE.search("let m = unsafe { f() };"))
        self.assertFalse(tools.UNSAFE.search("\"script-src 'unsafe-inline'\""))

    def test_missing_tool_is_not_clean(self):
        record = tools.missing("x", "x", "x")
        self.assertEqual((record["status"], record["exit_code"]), ("missing", 127))


@unittest.skipIf(FunctionModel is None, "pydantic-ai is not installed")
class ModelPlumbingTests(unittest.TestCase):
    def test_scanner_tags_lens_and_drops_lines_outside_the_source(self):
        file = graph.MODULES["records"].files[0]
        base = {"title": "t", "severity": "high", "confidence": "low", "file": file,
                "prerequisites": "p", "impact": "i", "evidence": "e", "suggested_fix": "f",
                "validation_needed": "v"}
        model = fixed_model({"summary": "s", "limitations": [],
                             "findings": [dict(base, line=5), dict(base, line=99_999)]})
        manifest = [{"file": f, "lines": 100} for f in graph.MODULES["records"].files]
        review, dropped, usage = asyncio.run(graph.run_scanner(
            model, "records", "dos", "i", "p", manifest, count_tokens=False))
        self.assertEqual([f["id"] for f in review["findings"]], ["records/dos#1"])
        self.assertEqual(review["findings"][0]["lens"], "dos")
        self.assertEqual(len(dropped), 1)
        self.assertEqual(usage["requests"], 1)

    def test_summary_schema_round_trip(self):
        payload = {"overview": "o", "candidates": [{
            "title": "t", "severity": "medium", "confidence": "high", "verdict": "likely_real",
            "module": "records", "lenses": ["dos"], "locations": [{"file": "a.rs", "line": 1}],
            "sources": ["records/dos#1"], "reasoning": "r", "impact": "i", "suggested_fix": "f",
            "disagreement": ""}], "noise": [], "disagreements": [], "manual_checks": ["m"],
            "tool_notes": [{"tool": "cargo-deny", "assessment": "a", "action": "b"}], "limitations": []}
        summary, _ = asyncio.run(graph.run_summary(fixed_model(payload), "p", count_tokens=False))
        self.assertEqual(summary["candidates"][0]["verdict"], "likely_real")


class GraphRenderTests(unittest.TestCase):
    def report(self, summary=True):
        evil = "<script>alert(1)</script>"
        report = {
            "kind": "security_graph", "status": "completed_unvalidated_model_review",
            "summary_status": "completed", "scanner_model": graph.SCANNER_MODEL,
            "summary_model": graph.SUMMARY_MODEL, "measured_cost_usd": {"total": 0.5},
            "usage": {"scanners": {"input_tokens": 1, "output_tokens": 2}, "summary": None},
            "modules": {"records": {"files": ["a.rs"], "lenses": ["dos"]}},
            "tools": [tools.result("gitleaks", "gitleaks", "missing", evil, exit_code=127),
                      tools.result("clippy-panics", "c", "findings", "s",
                                   [tools.finding("clippy::expect_used", "info", "m", file="a.rs", line=1)],
                                   exit_code=0)],
            "scanners": [{"id": "records/dos", "module": "records", "lens": "dos", "status": "completed",
                          "findings": 1, "usage": {"input_tokens": 1, "output_tokens": 2}, "cost_usd": 0.1}],
            "scanner_findings": [{"id": "records/dos#1", "module": "records", "lens": "dos", "title": evil,
                                  "severity": "high", "confidence": "low", "file": "a.rs", "line": 3,
                                  **{key: evil for key, _ in render.DETAILS}}],
        }
        if summary:
            report["summary"] = {
                "overview": evil, "candidates": [{
                    "title": evil, "severity": "high", "confidence": "medium", "verdict": "likely_real",
                    "module": "records", "lenses": ["dos"], "locations": [{"file": "a.rs", "line": 3}],
                    "sources": ["records/dos#1"], "reasoning": evil, "impact": "i", "suggested_fix": "f",
                    "disagreement": ""}],
                "noise": [{"sources": ["x"], "reason": evil}], "disagreements": [evil],
                "manual_checks": ["check a.rs:3"], "tool_notes": [{"tool": "t", "assessment": evil, "action": "a"}],
                "limitations": []}
        return report

    def test_renders_escaped(self):
        html = render.render_graph_report(self.report())
        self.assertNotIn("<script>", html)
        self.assertIn("&lt;script&gt;", html)
        self.assertIn("NOT RUN (missing)", html)
        self.assertIn("Likely real", html)
        self.assertIn("records/dos#1", html)

    def test_without_summary_says_so(self):
        html = render.render_graph_report(self.report(summary=False))
        self.assertIn("No summary available", html)

    def test_rejects_invalid_verdict(self):
        report = self.report()
        report["summary"]["candidates"][0]["verdict"] = "\"><b>"
        with self.assertRaises(ValueError):
            render.render_graph_report(report)


if __name__ == "__main__":
    unittest.main()
