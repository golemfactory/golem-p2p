# Security review with Haiku (Pydantic AI)

A bounded LLM review of selected source areas, next to the deterministic
checks of `.github/workflows/security.yml` (docs/security-checks.md). Findings
are model hypotheses until checked against the code and reproduced; this is
not an audit or a clean bill of health.

## Areas and budget

The scanner sends whole source files of one area per request, with line
numbers, to Haiku 5.5 (`claude-haiku-5-5`), and gets a typed review back
(at most five findings, each with file, line, prerequisites, impact,
evidence, fix and what needs validating). The model has no shell,
filesystem, web or other tools; the response schema is all it gets.

| Scope | Files | Input tokens (measured) |
|---|---|---|
| `records` | record, identity, signer, store, directory | 33,000 |
| `handshake` | endpoint, wire, error | 45,000 |
| `closed-network` | membership, registry (library and CLI), discovery, lookup | 28,000 |
| `cli-servers` | the CLI's listen server, published addresses (addrs) | 18,000 |
| `all` (default) | each of the above, one request per area | 124,000 |

Limits per request: input 60,000 tokens (counted before sending, well below
the 100,000 tokens where Haiku's price per token rises fivefold), output
4,000 tokens, or 16,000 with thinking (thinking tokens are output tokens).
One request per area, no retries (SDK and model). The scanner refuses a
configuration whose worst case exceeds $0.25. At the standard pricing checked
on 2026-11-14 ($0.10 per million input tokens and $0.50 per million output
tokens, prompts up to 100K tokens), the worst cases are $0.008 per area
without thinking and $0.014 with it, so $0.056 for `all` with thinking.
Pricing may change; estimates are not billing guarantees.

`--thinking medium` (the default) or `low` turns on Haiku's adaptive
thinking at that effort; `off` disables it. With thinking, the output schema
is requested as the response format (structured output) instead of a forced
tool call, which thinking does not allow. Areas other than `records` get a
short list of what record verification guarantees, so that the model does
not report missing checks that `record.rs` already makes.

`--source-ref REV` reviews the files of another revision (read with
`git show`) with the current scanner, for example the commit before a fix,
to see whether a prompt change finds a known bug.

The prompt includes the one-line results of the deterministic checks of the
same revision (`--checks`, JSON lines written by `harness/security-check.sh`),
so that the model does not report dependency or workflow issues again; the
report shows those checks in their own section.

## What the runs of 2026-11-15 showed

Eleven runs, about $0.11 in total by the configured rates. Benchmarks used
`source_ref` to review revisions that still had a known bug:

| Bug (revision) | No thinking | Low | Medium |
|---|---|---|---|
| Record expiry overflow panic (01c6d10, `records`) | found 2 of 2, rated low | missed 1 of 1 | found 3 of 3, rated high |
| Handshake slot held by rejected dialers (9911b1d, `handshake`) | missed 2 of 2 | – | found 2 of 2, rated high |

The first scan of 2026-11-14 (the old prompt, no thinking) had missed the
overflow too. Hence the default: all areas, medium thinking, about $0.026
per run (124,000 input and 27,000 to 29,000 output tokens, thinking
included); one area with medium thinking costs $0.006 to $0.009, without
thinking about $0.004.

Fixed after these runs, each with a test that failed before:
`cc5d1a0` (expiry overflow; found by hand before, then by the scan),
`9911b1d` (a withdrawn identity resolved again once the withdrawing record
was evicted or expired), `87bf426` (an identity whose entry was evicted
resolved to a replayed older record), `883612e` and `1d2d30d` (rejected
dialers held handshake slots for 2 s; the waits are now bounded),
`7ad9104` (certificates with appended bytes decoded), `e3862a1` (peer bytes
in `listen` debug logs could forge lines), `4b60c3e` (unbounded registry
management answers).

Reported and not changed, for a decision or as known limits:

* Registry: any account can register a node ID it does not own, first come
  first served, which blocks the owner (and the squatter can later remove
  it). Proof of possession at registration (a signature by the node's key)
  would fix it; it changes the registry API.
* Handshake slots: a peer that connects and never sends its Hello holds one
  of the 128 slots until the handshake timeout (10 s), so about 13 such
  connections per second keep other incoming connections out. A shorter
  deadline for the Hello, or a per-address limit for direct connections,
  needs measurements on slow relayed links first.
* Store and directory: valid records cost nothing to make, so a flood of
  long-lived records evicts honest ones (docs/design.md, section 6), and
  every directory PUT costs up to 17 signature checks. Rate limits belong in
  the proxy in front of a public directory.
* The membership certificate is not covered by the record signatures:
  whoever relays a record can strip it. Members never store a record without
  a valid certificate, and a members-only directory refuses it, but in an
  open directory the stripped copy can shadow the original at the same
  sequence number until the node publishes again.
* `listen` (the test server) does not bound concurrent echo requests (up to
  1 MiB each), only uploads and downloads.

Typical false positives: missing checks that another file makes (the
`RECORD_FACTS` context now names those for records), overflow in arithmetic
on the local clock, and limits that QUIC flow control or the connection cap
already bound.

## Manual GitHub Action

In GitHub, open **Actions → Security scan · Haiku → Run workflow**, choose the
branch, the scope, thinking and optionally a source revision, and run. The
workflow has only a `workflow_dispatch` trigger: pushes, pull requests and
schedules never start it. It uses the secret `CLAUDE_API_KEY_SCX`, mapped to
`ANTHROPIC_API_KEY` only for the scan step. The scan job runs in the
`security-scan` environment: an admin should keep the secret there, limited
to `main` (docs/security-checks.md).

The run first calls the Security checks workflow (no secrets) and passes its
results into the report; findings there do not stop the review. Only one scan
runs at a time. The scan step has a ten-minute timeout, the job twenty
minutes. Actions are pinned to commit hashes, repository permissions are
read-only, and checkout credentials are not persisted.

The Actions run summary embeds the report: areas reviewed, deterministic
check results, severity counts, summary, expandable findings with source
locations and all detail fields, and limitations. It also shows operational
status, token usage and estimated cost. This uses GitHub-supported HTML and
Markdown, without custom CSS or JavaScript, and needs no Pages setup. Model
text is escaped and rendered literally. Findings stay out of logs and never
fail the job; operational failures do fail it. If one area fails, the others
are still reported and the job fails.

The downloadable artifact contains `report.html`, `report.json`,
`report.summary.md`, `scan-plan.json` and `checks.jsonl`; artifacts are
retained for 14 days. If generation fails, the artifact may contain only the
scan plan; unavailable usage is not evidence of zero API cost.

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
gh workflow run security-scan.yml --repo golemfactory/golem-p2p --ref main \
  -f scope=all -f thinking=off
```

## Local scan

Run from the repository root with Python 3.11 or newer:

```sh
python3 -m venv /tmp/golem-p2p-security-venv
/tmp/golem-p2p-security-venv/bin/pip install --require-hashes --only-binary :all: -r harness/security-scan-requirements.txt
/tmp/golem-p2p-security-venv/bin/python harness/security_scan.py --dry-run --scope all
/tmp/golem-p2p-security-venv/bin/python harness/security_scan.py --scope records --output /tmp/golem-p2p-security.json
```

Enter an Anthropic API key at the hidden prompt, or supply it through
`ANTHROPIC_API_KEY`. Never put a key in this repository or a shell command.

JSON reports include the source hashes, Git revision, working-tree status,
model, package version, per-area status and token usage. Existing reports are
not overwritten. Offline tests of the scanner's plumbing and the renderer:

```sh
python3 -m unittest discover -s harness -p 'test_*security*.py'
```

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
