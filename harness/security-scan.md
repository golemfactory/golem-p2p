# Small security review with Pydantic AI

## Manual GitHub Action

In GitHub, open **Actions → Security scan · Haiku → Run workflow**, choose the
revision, and run. The workflow has only a `workflow_dispatch` trigger: pushes,
pull requests, and schedules never start this scan. It uses the secret
`CLAUDE_API_KEY_SCX`, mapped to `ANTHROPIC_API_KEY` only for the scan step.
The scan job runs in the `security-scan` environment: an admin should keep
the secret there, limited to `main` (docs/security-checks.md).

The fixed small preset reviews the five files listed below, allows one generation
request, caps input at 30,000 tokens and output at 3,500, and disables retries.
Only one scan job runs at a time. The scan step has a three-minute timeout;
the whole job has a ten-minute timeout. Actions are pinned to commit hashes,
repository permissions are read-only, and checkout credentials are not persisted.

The Actions run summary embeds the report directly: severity counts, summary,
expandable findings with source locations and all detail fields, and limitations.
It also shows operational status, token usage, and estimated cost. This uses
GitHub-supported HTML and Markdown, without custom CSS or JavaScript, and needs
no Pages setup. Model text is escaped and rendered literally. Findings stay
out of logs and never fail the job; operational failures do fail it.

The downloadable artifact contains `report.html`, `report.json`,
`report.summary.md`, and `scan-plan.json`; artifacts are retained for 14 days.
If generation fails, the artifact may contain only the scan plan;
unavailable usage is not evidence of zero API cost. No automatic retries run.

Pages publishing is optional and **off by default**. After an administrator sets
up Pages, check **Publish to Pages** when manually starting a run on `main` to
also publish the report to
<https://golemfactory.github.io/golem-p2p/>. This is a **public** GitHub Pages
site. The latest HTML report is served at the site root, with `report.json`
and `scan-plan.json` alongside it. Each deployment replaces the previous site;
older reports remain in Actions artifacts for their retention period. Failed
scans leave the published report unchanged. Runs on other branches still
produce downloadable artifacts but do not publish to Pages.

Pages must use **GitHub Actions** as its publishing source in repository
settings. Only the separate deployment job receives `pages: write` and
`id-token: write`; it has no API key or source checkout. The scan job retains
read-only repository permissions. The deployment summary links to the live
report. Publishing does not add model calls or alter the scan limits.

To dispatch from the command line:

```sh
gh workflow run security-scan.yml --repo golemfactory/golem-p2p --ref main
```

## Local scan

Run from the repository root with Python 3.11 or newer:

```sh
python3 -m venv /tmp/golem-p2p-security-venv
/tmp/golem-p2p-security-venv/bin/pip install --require-hashes --only-binary :all: -r harness/security-scan-requirements.txt
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

## Dependencies

`harness/security-scan-requirements.in` names the top-level package;
`harness/security-scan-requirements.txt` pins it and everything it pulls in,
with hashes, so the job that later holds the API key installs exactly the
reviewed set. The deterministic security workflow audits that file with
pip-audit (docs/security-checks.md). After changing the `.in` file, or to
pick up fixed releases, regenerate the lock file with
[uv](https://docs.astral.sh/uv/):

```sh
uv pip compile harness/security-scan-requirements.in --universal --python-version 3.11 --generate-hashes -o harness/security-scan-requirements.txt
```

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

To generate the GitHub Actions summary fragment instead:

```sh
python3 harness/render_security_report.py harness/results/security-haiku.json --format github
```

This writes `security-haiku.summary.md`. The workflow appends it directly to
`GITHUB_STEP_SUMMARY` without printing it. Oversized reports get a download
notice in the summary to stay below GitHub's 1 MiB per-step limit; full reports
remain in the artifacts.

The HTML includes severity counts, findings, limitations, usage, estimated cost,
and the source manifest when supplied. All report text is escaped and displayed
literally; no scripts, remote assets, or external requests are used. In CI, run
the renderer after the scanner and upload both JSON and HTML as artifacts.

Run renderer checks with synthetic data:

```sh
python3 -m unittest discover -s harness -p 'test_render_security_report.py'
```
