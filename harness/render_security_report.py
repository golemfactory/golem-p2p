#!/usr/bin/env python3
"""Render a structured security report as self-contained, offline HTML."""

import argparse
from collections import Counter
from html import escape
import json
from pathlib import Path
import sys


SEVERITIES = ("critical", "high", "medium", "low")
DETAILS = (
    ("impact", "Impact"),
    ("prerequisites", "Prerequisites"),
    ("evidence", "Evidence"),
    ("suggested_fix", "Suggested fix"),
    ("validation_needed", "Validation needed"),
)
CSS = """
:root{color-scheme:light;--ink:#182b39;--muted:#526572;--line:#d9e3e8;
--paper:#fff;--accent:#006d77}
*{box-sizing:border-box}body{margin:0;background:#eef3f5;color:var(--ink);
font:16px/1.6 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}
main{max-width:1120px;margin:auto;padding:48px 24px 72px}
.eyebrow{color:var(--accent);font-size:12px;letter-spacing:.15em;font-weight:750;
text-transform:uppercase}h1{font-size:clamp(30px,5vw,48px);line-height:1.15;
letter-spacing:-.035em;margin:12px 0}h2{font-size:22px;line-height:1.35}
h3{font-size:13px;text-transform:uppercase;letter-spacing:.06em;color:var(--muted)}
p{margin:8px 0 18px}.muted,footer{color:var(--muted)}
.panel,.finding{background:var(--paper);border:1px solid var(--line);
border-radius:14px;padding:24px;margin-top:22px}
.metrics{display:grid;grid-template-columns:repeat(4,1fr);gap:12px;margin:28px 0}
.metric{background:var(--paper);border:1px solid var(--line);border-radius:12px;
padding:16px}.metric strong{display:block;font-size:30px;line-height:1.3}
.badge{display:inline-block;border-radius:6px;padding:3px 9px;font-size:12px;
font-weight:700;background:#edf2f5;color:#405360;margin:0 8px 4px 0}
.critical{background:#fbe0e9;color:#8f1741}.high{background:#fee7df;color:#9d3516}
.medium{background:#fff1cd;color:#77530a}.low{background:#def1f5;color:#145c6c}
.finding{border-left:4px solid var(--accent)}.finding h2{margin:10px 0}
.text{white-space:pre-wrap;overflow-wrap:anywhere}
code{font:13px/1.6 ui-monospace,SFMono-Regular,Consolas,monospace;
overflow-wrap:anywhere}dl{display:grid;grid-template-columns:170px 1fr;
gap:10px 20px;margin:0}dt{color:var(--muted)}dd{margin:0;overflow-wrap:anywhere}
details{margin-top:16px}summary{cursor:pointer;font-weight:650;color:var(--accent)}
.table-wrap{overflow-x:auto}table{width:100%;border-collapse:collapse;font-size:14px}
th,td{text-align:left;padding:12px 8px;border-bottom:1px solid var(--line);
vertical-align:top}th{color:var(--muted)}li{margin-bottom:10px}
footer{margin-top:32px;font-size:13px}
@media(max-width:640px){main{padding:24px 16px}.metrics{grid-template-columns:repeat(2,1fr)}
.panel,.finding{padding:18px}dl{grid-template-columns:1fr;gap:2px}dd{margin-bottom:12px}}
@media print{body{background:white}main{max-width:none;padding:0}.panel,.finding{
break-inside:avoid}details{display:block}details>*{display:block}summary{display:none}}
"""


def text(value):
    """All report content is text, including anything resembling HTML/Markdown."""
    return escape(str(value), quote=True)


def validate_review(review):
    if not isinstance(review, dict):
        raise ValueError("Review must be an object")
    if not isinstance(review.get("summary"), str):
        raise ValueError("Missing review summary")
    findings = review.get("findings")
    limitations = review.get("limitations")
    if not isinstance(findings, list) or not isinstance(limitations, list):
        raise ValueError("Expected findings and limitations lists")
    if not all(isinstance(item, str) for item in limitations):
        raise ValueError("Limitations must be text")
    for finding in findings:
        if not isinstance(finding, dict):
            raise ValueError("Finding must be an object")
        for key in ("title", "file", "severity", "confidence", *(key for key, _ in DETAILS)):
            if not isinstance(finding.get(key), str):
                raise ValueError("Missing finding text field")
        if finding["severity"] not in SEVERITIES:
            raise ValueError("Invalid severity")
        if finding["confidence"] not in ("high", "medium", "low"):
            raise ValueError("Invalid confidence")
        if type(finding.get("line")) is not int or finding["line"] < 1:
            raise ValueError("Invalid source line")


def definition_list(items):
    return "<dl>" + "".join(
        f"<dt>{text(label)}</dt><dd>{text(value)}</dd>" for label, value in items
    ) + "</dl>"


def render_report(report):
    if not isinstance(report, dict):
        raise ValueError("Report must be an object")
    # Accept either the scan envelope or the direct Pydantic Review JSON.
    review = report.get("review", report if "findings" in report else None)
    if review is not None:
        validate_review(review)
    elif "status" not in report:
        raise ValueError("Missing review or run status")
    parts = ["""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">
<title>golem-p2p · Security scan</title><style>""", CSS,
        "</style></head><body><main><header><div class=\"eyebrow\">golem-p2p / scanner report</div>"
        "<h1>Security scan</h1><p class=\"muted\">Structured output · Rendered locally</p></header>",
    ]
    metadata = [
        ("Run status", report.get("status", "Structured review; run status not supplied")),
        ("Model", report.get("model", "Not supplied")),
        ("Completed (UTC)", report.get("completed_at_utc", "Not supplied")),
        ("Git commit", report.get("git_head", "Not supplied")),
    ]
    cost = report.get("estimated_actual_usd")
    if isinstance(cost, (int, float)) and not isinstance(cost, bool):
        metadata.append(("Estimated cost", f"${cost:.6f} USD"))
    usage = report.get("usage", {})
    if isinstance(usage, dict):
        for key, label in (("input_tokens", "Input tokens"), ("output_tokens", "Output tokens"),
                           ("requests", "Model requests")):
            if key in usage:
                metadata.append((label, usage[key]))
    parts.append('<section class="panel" aria-label="Run metadata">' + definition_list(metadata) + '</section>')
    if review is None:
        parts.append('<section class="panel"><h2>No review available</h2>'
                     '<p>This run has metadata but no structured findings. This is not a zero-findings result.</p>')
        if "note" in report:
            parts.append(f'<p class="text">{text(report["note"])}</p>')
        parts.append('</section>')
    else:
        findings = review["findings"]
        counts = Counter(finding["severity"] for finding in findings)
        parts.append('<div class="metrics" aria-label="Findings by severity">')
        for severity in SEVERITIES:
            parts.append(f'<div class="metric"><span class="badge {severity}">{severity.title()}</span>'
                         f'<strong>{counts[severity]}</strong></div>')
        parts.append('</div><section class="panel"><h2>Summary</h2>'
                     f'<p class="text">{text(review["summary"])}</p></section>')
        parts.append(f'<section aria-label="Findings"><h2>Findings <span class="muted">({len(findings)})</span></h2>')
        if not findings:
            parts.append('<p>No findings reported within this scan’s scope.</p>')
        for index, finding in enumerate(findings, 1):
            severity = finding["severity"]  # Validated enum, never arbitrary attribute content.
            parts.append(f'<article class="finding"><span class="badge {severity}">{severity.title()}</span>'
                         f'<span class="badge">{text(finding["confidence"].title())} confidence</span>'
                         f'<h2>{index:02d}. {text(finding["title"])}</h2>'
                         f'<code>{text(finding["file"])}:{finding["line"]}</code>')
            for key, label in DETAILS:
                parts.append(f'<h3>{label}</h3><p class="text">{text(finding[key])}</p>')
            parts.append('</article>')
        parts.append('</section><section class="panel"><h2>Limitations</h2>')
        if review["limitations"]:
            parts.append('<ul>' + ''.join(f'<li class="text">{text(item)}</li>' for item in review["limitations"]) + '</ul>')
        else:
            parts.append('<p>No limitations supplied in the structured output.</p>')
        parts.append('</section>')
    scope = report.get("scope", [])
    if isinstance(scope, list) and scope:
        parts.append('<section class="panel"><h2>Source snapshot</h2><div class="table-wrap"><table>'
                     '<thead><tr><th scope="col">File</th><th scope="col">Lines</th><th scope="col">SHA-256</th></tr></thead><tbody>')
        for item in scope:
            if not isinstance(item, dict):
                raise ValueError("Invalid source manifest")
            parts.append(f'<tr><td><code>{text(item.get("file", ""))}</code></td>'
                         f'<td>{text(item.get("lines", ""))}</td>'
                         f'<td><code>{text(item.get("sha256", ""))}</code></td></tr>')
        parts.append('</tbody></table></div></section>')
    parts.append('<footer>Model-generated findings are reproduced as supplied. '
                 'Rendering does not validate or change their assessment. '
                 'This document is self-contained and makes no network requests.</footer></main></body></html>')
    return ''.join(parts)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path, help="Structured report JSON")
    parser.add_argument("--output", type=Path, help="HTML destination; defaults to the input name with .html")
    args = parser.parse_args()
    output = args.output or args.input.with_suffix(".html")
    try:
        report = json.loads(args.input.read_text(encoding="utf-8"))
        html = render_report(report)
        output.parent.mkdir(parents=True, exist_ok=True)
        # Preserve both the input and any existing report.
        with output.open("x", encoding="utf-8") as destination:
            destination.write(html)
    except (OSError, ValueError, TypeError):
        # Never print report content, JSON parser excerpts, or provider messages.
        print("Rendering failed: check the input schema and ensure the output path is writable and does not exist.", file=sys.stderr)
        return 1
    print(f"HTML report: {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
