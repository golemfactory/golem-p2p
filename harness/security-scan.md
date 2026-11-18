# Security review with Haiku (Pydantic AI)

A bounded LLM review of selected source areas, next to the deterministic
checks of `.github/workflows/security.yml` (docs/security-checks.md). Findings
are model hypotheses until checked against the code and reproduced; this is
not an audit or a clean bill of health.

The graph scan (last section) extends this: more deterministic tools, then
30 narrow Haiku scanners (module × problem type), then one Opus summary.
It is manual only and runs locally; no workflow starts it.

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

Twelve runs, about $0.14 in total by the configured rates. Benchmarks used
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
management answers), `1634cc3` (reading the times of an unchecked
certificate could panic; the registry state's `Debug` showed its key).

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
  the proxy in front of a public directory. Once a flood has evicted an
  endpoint's newest record, the store has forgotten it, and a replayed older
  record of that endpoint is accepted again (an identity it withdrew then
  resolves to an endpoint that refuses it) until the node publishes again.
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

## Graph scan

`harness/security_graph.py` (also `harness/security_scan.py graph`) runs
three stages:

1. **Deterministic tools** (`harness/security_tools.py`), on the working
   tree. Each writes its raw output to `RUN_DIR/tools/` and one normalized
   JSON line to `RUN_DIR/tools.jsonl` (status `clean`, `findings`,
   `missing` or `failed`; a missing or failing tool is never reported as
   clean). The lines also have the fields of `security-check.sh`, so
   `security_scan.py --checks RUN_DIR/tools.jsonl` accepts them.
2. **Scanners**: Haiku 5.5 (`claude-haiku-5-5`), medium thinking, one request
   per module and lens. Each gets the module's files, one lens's
   instructions, the tool results for that module and lens, and the known
   limits listed above. It returns the review schema of the single-area scan;
   every finding is tagged with `module`, `lens` and an ID (`records/dos#2`).
   Findings whose line lies outside the supplied file are dropped and counted.
3. **Summary**: Opus 5.5 (`claude-opus-5-5`), high thinking, streamed. It gets
   every scanner's output, every tool's results, the known limits and the
   source lines around each scanner finding (±8 lines, at most 150 KB). It
   merges duplicates, gives each candidate a verdict (`likely_real`,
   `plausible_needs_test`, `known_limit`), ranks by severity and confidence,
   lists disagreements between scanners, dismisses noise with a reason,
   assesses the tools and lists what to verify by hand. The script then
   checks that every scanner finding ID is merged or dismissed and that each
   location exists, and records any problem in `summary_checks`.

The models have no tools; the response schema is all they get.

### Tools

| Tool | Version, pin | What | In CI too |
|---|---|---|---|
| cargo-audit | 0.23.0, `security-tools.sh` | RustSec advisories for `Cargo.lock`, warnings included | yes |
| cargo-deny | 0.20.2, `security-tools.sh`, `deny.toml` | advisories, licenses, bans, sources (the golemfactory/iroh fork is allowed in `deny.toml`) | yes |
| osv-scanner | 2.7.1, `security-tools.sh` | OSV (RustSec and GitHub advisories) for `Cargo.lock` and both Python lock files | no |
| gitleaks | 8.30.1, `security-tools.sh`, `.gitleaks.toml` | all commits of all refs, plus the tree of HEAD (`git archive`, so untracked keys and results stay out); only rule, file, line and commit are kept | history only |
| clippy panic surface | Rust 1.96.0 | `unwrap_used`, `expect_used`, `indexing_slicing`, `panic`, `arithmetic_side_effects`, `cast_possible_truncation` on non-test code; informational, leads for the `panics` and `dos` lenses | no |
| unsafe count | regex | `unsafe` in the workspace's `.rs` files and the crate-level lint attributes; informational | no (compile-time `forbid`/`deny`) |
| semgrep | 1.183.0, `semgrep-requirements.txt` (hashes); rules at semgrep-rules `a5ad1714f00c` | `rust/` and `python/lang/security/` rules over the tracked `.rs` and `.py` files | no |

Not included: cargo-geiger (it builds every dependency; the workspace's own
code already forbids unsafe code, see the unsafe count) and pip-audit (osv-
scanner covers the same lock files; CI runs pip-audit). Clippy uses its own
target directory (`~/.cache/golem-p2p-security-target`).

Install into user space (no sudo), pinned and checked:

```sh
harness/security-tools.sh ~/.local/security-tools cargo-deny cargo-audit gitleaks osv-scanner
python3 -m venv ~/.local/security-tools/semgrep-venv
~/.local/security-tools/semgrep-venv/bin/pip install --require-hashes --only-binary :all: -r harness/semgrep-requirements.txt
git init ~/.local/security-tools/semgrep-rules && cd ~/.local/security-tools/semgrep-rules \
  && git fetch --depth 1 https://github.com/semgrep/semgrep-rules a5ad1714f00c9f7fde980cab0ea1d6dc6d414fd2 \
  && git checkout FETCH_HEAD
```

`harness/semgrep-requirements.txt` is generated from the `.in` file like the
scan requirements (`uv pip compile ... --universal --python-version 3.11
--generate-hashes`).

### Modules and lenses

Lenses: `dos` (input parsing, denial of service, resource bounds), `auth`
(authentication, authorization, membership, certificates), `crypto`
(signatures and keys), `panics` (concurrency, panics, unwraps), `leaks`
(secrets, logging, information leaks).

| Module | Files | Lenses |
|---|---|---|
| `records` | record, identity, signer, store, directory | all five |
| `handshake` | endpoint, wire, error | all five |
| `closed-network` | membership, registry (library and CLI), discovery, lookup | all five |
| `telemetry` | library telemetry, CLI telemetry and collector | all five |
| `cli-servers` | the CLI's listen server, addrs | dos, panics, leaks (peers are authenticated by the endpoint) |
| `cli-client` | CLI main, parse, out, observe, probe; library lib.rs | dos, panics, leaks |
| `bench-soak` | bench (mod, local), soak | dos, panics (measurement clients; the operator is trusted) |
| `harness-python` | lab reflector and natcheck, security scan, graph, tools and renderer | dos, leaks |

Not scanned: `bench/report.rs` and `bench/verdict.rs` (formatting of local
measurements), `build.rs` and the integration tests.

### Budget

Per scanner request: input at most 60,000 tokens (counted before sending, so
below Haiku's 100K price step), output at most 16,000 tokens including
thinking. Summary: input at most 150,000 tokens, output at most 48,000. One
request each, no retries (SDK and model), four scanners at a time. Prices
checked on 2026-11-18: Haiku 5.5 $0.10 / $0.50 per million input / output
tokens (prompts up to 100K), Opus 5.5 $4 / $20. Worst case with every
request at its limits: 30 × $0.014 + $1.56 = $1.98. The script refuses to
start above $3.00. `summary.json` records the token usage the API reported
and the cost at these prices (`measured_cost_usd`); it is not a bill.

The scanner prompts are 43 to 132 KB (about 18,000 to 53,000 tokens). A
prompt over the input limit is refused before sending and that scanner is
reported as failed; the others continue.

### Running it

```sh
python3 -m venv /tmp/golem-p2p-security-venv
/tmp/golem-p2p-security-venv/bin/pip install --require-hashes --only-binary :all: -r harness/security-scan-requirements.txt
# Tools only, plan and worst case; no API calls:
/tmp/golem-p2p-security-venv/bin/python harness/security_scan.py graph harness/results/security-graph-DATE --dry-run
# The full graph (key from the environment or the hidden prompt):
/tmp/golem-p2p-security-venv/bin/python harness/security_scan.py graph harness/results/security-graph-DATE --reuse-tools
python3 harness/render_security_report.py harness/results/security-graph-DATE/summary.json --output harness/results/security-graph-DATE/report.html
```

Sources are read from `--source-ref` (default `HEAD`) with `git show`;
`--working-tree` reads the working tree instead. `--modules` limits the run,
`--no-summary` skips Opus. A run directory that already has a
`summary.json` is refused. Output: `tools/` (raw tool output), `tools.jsonl`,
`plan.json`, `scanners/MODULE--LENS.json`, `summary.json`, and the HTML.
Offline tests (synthetic data and a stand-in model):
`python3 -m unittest discover -s harness -p 'test_*security*.py'`.

### Results of 2026-11-18

Tools on `main` at `a0fdc67`, in `harness/results/security-graph-2026-11-18/`
(with `plan.json`: 30 scanners, worst case $1.98):

* cargo-deny fails: RUSTSEC-2026-0543, chrono 0.4.45 is unmaintained. chrono
  comes only from `wmi` (Windows) via iroh's `netwatch`, so Linux builds do
  not compile it; the CI job fails on it all the same. cargo-audit reports it
  as a warning, osv-scanner as an advisory. 22 duplicate-version warnings.
* osv-scanner: six advisory groups in the semgrep tool environment
  (`mcp` 1.29.0: four, `opentelemetry-api` 1.37.0: two). They concern the
  local tool, not golem-p2p. Nothing for the scan requirements.
* gitleaks: no leaks in the history of all refs or in the tree of HEAD.
* Unsafe: four uses, all the glibc `mallinfo2` call of soak (CLI and its
  test copy); the library forbids unsafe code.
* Clippy panic surface: 325 hits in non-test code (arithmetic 187, expect
  81, truncating casts 30, indexing 27); most in the telemetry collector,
  endpoint, soak, bench and probe.
* Semgrep: 16 matches. One rated high, `subprocess.run` in
  `security_tools.py`, takes a constant argument list without a shell (no
  injection). One medium, a urllib URL in the soak sampler, comes from the
  operator. The rest are low: `temp_dir` in tests, binds to all interfaces
  in natcheck, the soak `unsafe`, `csv` without defusedcsv in a summarizer.

The model stages did not run on 2026-11-18: no Anthropic API key was
available on the host. Run them on the same directory with `--reuse-tools`.
