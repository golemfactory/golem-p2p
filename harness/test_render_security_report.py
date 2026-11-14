"""Renderer checks using synthetic data only; no saved scan reports are read."""

from contextlib import redirect_stderr, redirect_stdout
from html.parser import HTMLParser
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from render_security_report import main, render_report, render_github_summary


class Tags(HTMLParser):
    def __init__(self, document):
        super().__init__()
        self.tags = []
        self.feed(document)

    def handle_starttag(self, tag, attrs):
        self.tags.append(tag)


class RenderTests(unittest.TestCase):
    def test_github_summary_escapes_findings_and_has_expandable_sections(self):
        payload = '</p><script>example()</script><img src="https://example.com/tracker">\n![x](https://example.com/image)'
        finding = dict.fromkeys(("title", "file", "impact", "prerequisites", "evidence",
                                 "suggested_fix", "validation_needed"), payload)
        finding.update(severity="high", confidence="low", line=9)
        result = render_github_summary({"summary": payload, "findings": [finding], "limitations": [payload]})
        tags = Tags(result).tags
        self.assertNotIn('script', tags)
        self.assertNotIn('img', tags)
        self.assertEqual(tags.count('details'), 2)
        self.assertIn('<td align="center"><strong>1</strong></td>', result)
        self.assertIn('&lt;script&gt;', result)

    def test_github_summary_size_and_missing_result(self):
        result = render_github_summary({"summary": '🟠' * 300_000, "findings": [], "limitations": []})
        self.assertLess(len(result.encode()), 900_000)
        self.assertIn('too large', result)
        self.assertIn('not a zero-findings', render_github_summary({"status": "failed"}))

    def test_untrusted_text_cannot_create_html(self):
        payload = '<script>alert("example")</script><img src=x onerror=alert(1)>'
        finding = dict.fromkeys(("title", "file", "impact", "prerequisites", "evidence",
                                 "suggested_fix", "validation_needed"), payload)
        finding.update(severity="high", confidence="low", line=7)
        document = render_report({
            "model": payload,
            "scope": [{"file": payload, "lines": 1, "sha256": payload}],
            "review": {"summary": payload, "findings": [finding], "limitations": [payload]},
        })
        self.assertNotIn("script", Tags(document).tags)
        self.assertNotIn("img", Tags(document).tags)
        self.assertIn("&lt;script&gt;", document)
        self.assertIn("Content-Security-Policy", document)

    def test_missing_review_is_not_a_clean_scan(self):
        document = render_report({"status": "generation_failed"})
        self.assertIn("No review available", document)
        self.assertNotIn("Findings (0)", document)
        with self.assertRaises(ValueError):
            render_report({})
        with self.assertRaises(ValueError):
            render_report({"review": {"summary": "Example", "findings": [{}], "limitations": []}})

    def test_cli_does_not_log_findings_or_overwrite_files(self):
        marker = "SYNTHETIC_REPORT_CONTENT"
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "report.json"
            source.write_text(json.dumps({"summary": marker, "findings": [], "limitations": []}))
            stdout, stderr = io.StringIO(), io.StringIO()
            with patch("sys.argv", ["render_security_report.py", str(source)]), redirect_stdout(stdout), redirect_stderr(stderr):
                self.assertEqual(main(), 0)
                destination = source.with_suffix(".html")
                original = destination.read_bytes()
                self.assertEqual(main(), 1)
                self.assertEqual(destination.read_bytes(), original)
            self.assertNotIn(marker, stdout.getvalue() + stderr.getvalue())
            self.assertIn(marker, original.decode())
            summary_log = io.StringIO()
            with patch("sys.argv", ["render_security_report.py", str(source), "--format", "github"]), redirect_stdout(summary_log):
                self.assertEqual(main(), 0)
            self.assertNotIn(marker, summary_log.getvalue())
            self.assertIn(marker, source.with_suffix('.summary.md').read_text())


if __name__ == "__main__":
    unittest.main()
