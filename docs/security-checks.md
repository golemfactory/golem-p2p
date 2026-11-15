# Security checks

Two workflows look at security:

* **Security checks** (`.github/workflows/security.yml`): deterministic
  checks with pinned tools. Runs on every push to `main`, on pull requests,
  every Monday (advisory databases change while the code does not), and by
  hand. No API keys; the token is read-only.
* **Security scan · Haiku** (`.github/workflows/security-scan.yml`): a
  bounded LLM review of selected source files. Manual only, because it costs
  money. See `harness/security-scan.md`.

## What the deterministic checks cover

| Job | Tool | Covers | Fails the run |
|---|---|---|---|
| Rust dependencies | cargo-deny 0.20.2 (`deny.toml`) | RustSec advisories for every crate in `Cargo.lock` that a feature combination can pull in: vulnerabilities, unsound and unmaintained crates, yanked versions. Licenses (permissive only). Bans: `portmapper` (no port mapping on the user's router, design.md section 9), `openssl-sys` (rustls only), wildcard versions. Sources: crates.io only, no git dependencies. Duplicate versions are listed as warnings. | yes, except duplicate warnings |
| | cargo-audit 0.23.0 | RustSec vulnerabilities in `Cargo.lock`, second implementation. Unmaintained, unsound and yanked show as warnings. | on vulnerabilities |
| GitHub Actions workflows | zizmor 1.32.0, pedantic persona | Workflow security: template injection, credentials persisted by checkout, excessive permissions, unpinned or impostor action commits, known-vulnerable actions, cache poisoning, secrets outside environments, and more. | yes |
| | zizmor, auditor persona | The same with low-confidence audits. | no (informational) |
| | actionlint 1.7.12 with shellcheck 0.11.0 | Workflow syntax, expressions, types, and the shell scripts in `run:` steps. | yes |
| Secrets in git history | gitleaks 8.30.1 (`.gitleaks.toml`) | All commits of all branches: the default rules (API keys, tokens, private keys), plus the secret files of a golem-p2p deployment, which hold bare hex: `*.key`, `*.token`, `registry.json`, `.env`, and a registry's `"secret_key"`. Output is redacted. | yes |
| Python dependencies | pip-audit 2.10.1 | PyPI advisories for `harness/security-scan-requirements.txt`, which pins every package with hashes. | yes |

The summary of each run has a table of the jobs and, per check, its result,
the last line of its output and the full output.

Also enforced at compile time, by the normal CI: the library has
`#![forbid(unsafe_code)]`; the CLI denies unsafe code except the glibc heap
statistics call of `soak`.

Not covered: the Rust code itself beyond clippy and the tests (no static
analysis for logic bugs, no fuzzing), and the deployment. The manual Haiku
review looks at selected files; its findings are hypotheses to check.

## Ignoring a finding

Fix it if possible: `cargo update -p <crate>` for a patched release, a
different crate, a workflow change. If a finding cannot be fixed yet, ignore
it narrowly and say why, next to the ignore:

* RustSec advisory: an entry in `[advisories] ignore` of `deny.toml` with
  `id` and `reason`. For a vulnerability, cargo-audit needs the same ID in
  `.cargo/audit.toml` (`[advisories] ignore = [...]`) with a comment.
* License: an `exceptions` entry for the one crate in `deny.toml`.
* zizmor: an inline `# zizmor: ignore[<audit>]` comment on the line, with a
  reason.
* gitleaks: a false positive goes into an `[allowlist]` of `.gitleaks.toml`
  (by commit or path). A real secret is never ignored: revoke it first, then
  remove it from history.

Current ignores:

| Ignore | Why |
|---|---|
| RUSTSEC-2024-0436 (`paste` unmaintained) | Compile-time proc-macro of `netlink-packet-core` 0.8, used by iroh 1.4.1 through `netwatch`; nothing of it runs at run time, and there is no successor release in that line. Remove when iroh moves on. |

## Running the checks locally

The tools are pinned in `harness/security-tools.sh`, which installs the
release binaries (Linux x86_64) after checking their SHA-256:

```sh
harness/security-tools.sh ~/.local/security-tools cargo-deny cargo-audit zizmor actionlint shellcheck gitleaks
export PATH=~/.local/security-tools:$PATH

cargo deny --locked check --hide-inclusion-graph
cargo audit
GH_TOKEN=... zizmor --persona pedantic .github/workflows   # without GH_TOKEN the online audits are skipped
actionlint
gitleaks git --config .gitleaks.toml --log-opts=--all --redact .
gitleaks dir --config .gitleaks.toml --redact .              # also uncommitted files

python3 -m venv /tmp/pip-audit && /tmp/pip-audit/bin/pip install pip-audit==2.10.1
/tmp/pip-audit/bin/pip-audit --strict --disable-pip --require-hashes -r harness/security-scan-requirements.txt
```

`harness/security-check.sh TITLE -- COMMAND...` is what the workflow runs
around each check (it writes the run summary when `$GITHUB_STEP_SUMMARY` is
set).

To update a tool, change its version and SHA-256 together in
`harness/security-tools.sh`; take the digest from the release (the GitHub API
`digest` of the asset, or the project's checksum file). To update the Python
lock file, see `harness/security-scan.md`.

## Repository settings (need admin rights)

These are not files in the repository; an admin sets them:

* **Secret scanning and push protection** (Settings → Code security). Free
  for public repositories; push protection stops a known token type before it
  lands in history, which gitleaks can only report afterwards.
* **Dependabot alerts** (and optionally Dependabot security updates). They
  use the GitHub Advisory Database, so they complement cargo-deny's RustSec
  checks.
* **Private vulnerability reporting**, so that people can report issues
  without opening a public issue.
* **The `security-scan` environment**: the Haiku workflow's scan job runs in
  it. Move the secret `CLAUDE_API_KEY_SCX` from the repository secrets into
  this environment's secrets and limit the environment to the `main` branch
  (optionally with required reviewers). Until then, a run from any branch,
  with whatever scan script that branch has, gets the key.
* **Branch protection** for `main`, requiring the CI and Security checks
  jobs, once pull requests are the way changes land.
