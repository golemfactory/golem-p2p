# Small security review with Pydantic AI

## Manual GitHub Action

In GitHub, open **Actions → Security scan · Haiku → Run workflow**, choose the
revision, and run. The workflow has only a `workflow_dispatch` trigger: pushes,
pull requests, and schedules never start this scan. It uses the repository
secret `CLAUDE_API_KEY_SCX`, mapped to `ANTHROPIC_API_KEY` only for the scan step.

The fixed small preset reviews the five files listed below, allows one generation
request, caps input at 30,000 tokens and output at 3,500, and disables retries.
Only one scan job runs at a time. The scan step has a three-minute timeout;
the whole job has a ten-minute timeout. Actions are pinned to commit hashes,
repository permissions are read-only, and checkout credentials are not persisted.

The run summary contains operational status, token usage, and estimated cost.
Download its artifact and open `report.html` locally. The archive also contains
`report.json` and `scan-plan.json`; artifacts are retained for 14 days. Findings
stay out of the summary and logs, and never fail the job. Operational failures
do fail it. If generation fails, the artifact may contain only the scan plan;
unavailable usage is not evidence of zero API cost. No automatic retries run.

To dispatch from the command line:

```sh
gh workflow run security-scan.yml --repo golemfactory/golem-p2p --ref main
```

## Local scan

Run from the repository root with Python 3.11 or newer:

```sh
python3 -m venv /tmp/golem-p2p-security-venv
/tmp/golem-p2p-security-venv/bin/pip install -r harness/security-scan-requirements.txt
/tmp/golem-p2p-security-venv/bin/python harness/security_scan.py --dry-run
/tmp/golem-p2p-security-venv/bin/python harness/security_scan.py --output /tmp/golem-p2p-security.json
```

Enter an Anthropic API key at the hidden prompt, or supply it through
`ANTHROPIC_API_KEY`. Never put a key in this repository or a shell command.

The script sends exactly five source files to Anthropic: record validation,
identity signatures, signing, record storage and the HTTP directory. Tests in
those files are included. It uses Pydantic AI with Haiku 5.5 and a typed report,
with no model access to shell, filesystem, web or other tools. The response
schema is the only tool exposed to the model.

The input is counted before generation and capped at 30,000 tokens; output is
capped at 3,500 tokens. There is one generation request, with SDK and model
retries disabled. At the standard pricing checked on 2026-11-14 ($0.10 per
million input tokens and $0.50 per million output tokens), this is at most
approximately $0.00475 per successful run. Pricing may change; estimates are
not billing guarantees. A failed request or invalid response is not retried.

JSON reports include the source hashes, Git revision, working-tree status,
model, package version and token usage. Existing reports are not overwritten.
Findings are model hypotheses until checked against the code and reproduced.
This deliberately excludes endpoint handshakes, application streams, CLI,
dependency vulnerabilities and deployment configuration. It is not a complete
security audit or a clean bill of health.

## HTML reports

Convert a saved JSON report into a self-contained HTML file using only Python's
standard library. This performs no model calls and logs only the output path:

```sh
python3 harness/render_security_report.py harness/results/security-haiku.json
```

The default destination is the same filename with an `.html` extension. Use
`--output path/to/report.html` to choose another location. Existing files are
never overwritten. The renderer accepts the full scan report or a standalone
`Review` object containing `summary`, `findings`, and `limitations`. Runs with
status metadata but no review are displayed explicitly as missing results.

The HTML includes severity counts, findings, limitations, usage, estimated cost,
and the source manifest when supplied. All report text is escaped and displayed
literally; no scripts, remote assets, or external requests are used. In CI, run
the renderer after the scanner and upload both JSON and HTML as artifacts.

Run renderer checks with synthetic data:

```sh
python3 -m unittest discover -s harness -p 'test_render_security_report.py'
```
