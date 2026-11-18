#!/usr/bin/env python3
"""Deterministic security tools for the graph scan (harness/security_graph.py).

Runs each tool on the working tree, keeps its raw output under RUN_DIR/tools/
and writes one normalized JSON line per tool to RUN_DIR/tools.jsonl. A tool
that is not installed is reported as `missing`, one that fails or whose
output cannot be read as `failed`; neither is ever reported as clean.

The lines also carry the fields of harness/security-check.sh (`title`,
`exit_code`, `informational`, `last_line`, `output_tail`), so that
`security_scan.py --checks RUN_DIR/tools.jsonl` accepts them.

Usage: harness/security_tools.py RUN_DIR [--tools-dir DIR] [--only TOOL ...]
Install the tools first (docs/security-checks.md, harness/security-scan.md).
"""

import argparse
from collections import Counter
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
RUST_TOOLCHAIN = "1.96.0"
DEFAULT_TOOLS_DIR = Path.home() / ".local/security-tools"
# Pinned commit of https://github.com/semgrep/semgrep-rules (rust/ and
# python/lang/security/ are used). Fetched by harness/security-scan.md's steps.
SEMGREP_RULES_COMMIT = "a5ad1714f00c9f7fde980cab0ea1d6dc6d414fd2"
# Panic surface of non-test code, informational: lints the normal CI does not
# enable. unwrap_used and panic are included although the code has none today.
CLIPPY_LINTS = ("unwrap_used", "expect_used", "indexing_slicing", "panic",
                "arithmetic_side_effects", "cast_possible_truncation")
LOCKFILES = ("Cargo.lock", "harness/security-scan-requirements.txt",
             "harness/semgrep-requirements.txt")
MAX_FINDINGS = 400
TIMEOUT = 1200


def finding(rule, severity, message, file=None, line=None, package=None):
    return {"rule": rule, "severity": severity, "file": file, "line": line,
            "package": package, "message": str(message)[:300]}


class Runner:
    def __init__(self, run_dir, tools_dir):
        self.raw_dir = run_dir / "tools"
        self.raw_dir.mkdir(parents=True, exist_ok=True)
        self.tools_dir = tools_dir
        self.env = dict(os.environ, PATH=f"{tools_dir}:{os.environ.get('PATH', '')}",
                        CARGO_TERM_COLOR="never", NO_COLOR="1")
        # Keep the scan's build artifacts away from the repository's target/.
        self.env.setdefault("CARGO_TARGET_DIR", str(Path.home() / ".cache/golem-p2p-security-target"))

    def which(self, name):
        return shutil.which(name, path=self.env["PATH"])

    def run(self, cmd, cwd=ROOT, timeout=TIMEOUT):
        start = time.monotonic()
        try:
            proc = subprocess.run(cmd, cwd=cwd, env=self.env, capture_output=True,
                                  text=True, errors="replace", timeout=timeout)
        except subprocess.TimeoutExpired:
            return None, "", "timeout", time.monotonic() - start
        return proc.returncode, proc.stdout, proc.stderr, time.monotonic() - start

    def version(self, cmd):
        code, out, err, _ = self.run(cmd, timeout=60)
        text = (out or err).strip().splitlines()
        return text[0][:100] if code == 0 and text else "unknown"

    def save(self, name, content):
        path = self.raw_dir / name
        path.write_text(content, encoding="utf-8")
        return str(path.relative_to(self.raw_dir.parent))


def result(tool, title, status, summary, findings=(), *, exit_code, version="",
           command="", raw=(), informational=False, extra=None):
    findings = list(findings)
    record = {
        "tool": tool, "title": title, "status": status, "version": version,
        "command": command, "summary": summary, "finding_count": len(findings),
        "findings": findings[:MAX_FINDINGS], "raw": list(raw),
        # The fields of harness/security-check.sh lines.
        "exit_code": exit_code, "informational": informational,
        "last_line": summary[:200], "output_tail": "",
    }
    if extra:
        record.update(extra)
    return record


def missing(tool, title, what):
    return result(tool, title, "missing", f"not run: {what} is not installed", exit_code=127)


def failed(tool, title, why, **kw):
    return result(tool, title, "failed", f"failed: {why}", exit_code=kw.pop("exit_code", 1) or 1, **kw)


def json_lines(text):
    for line in text.splitlines():
        line = line.strip()
        if line.startswith("{"):
            try:
                yield json.loads(line)
            except json.JSONDecodeError:
                continue


# -- RustSec ---------------------------------------------------------------

def parse_cargo_audit(data):
    found = []
    for vuln in data["vulnerabilities"]["list"]:
        adv, pkg = vuln["advisory"], vuln["package"]
        found.append(finding(adv["id"], "high", adv["title"], file="Cargo.lock",
                             package=f"{pkg['name']} {pkg['version']}"))
    for kind, warnings in data.get("warnings", {}).items():
        for warning in warnings:
            adv, pkg = warning.get("advisory") or {}, warning["package"]
            found.append(finding(adv.get("id", kind), "medium" if kind == "unsound" else "low",
                                 f"{kind}: {adv.get('title', kind)}", file="Cargo.lock",
                                 package=f"{pkg['name']} {pkg['version']}"))
    return found


def cargo_audit(r):
    title = "cargo audit"
    if not r.which("cargo-audit"):
        return missing("cargo-audit", title, "cargo-audit")
    cmd = ["cargo", f"+{RUST_TOOLCHAIN}", "audit", "--json"]
    code, out, err, _ = r.run(cmd)
    raw = [r.save("cargo-audit.json", out), r.save("cargo-audit.stderr.txt", err)]
    kw = dict(version=r.version(["cargo-audit", "--version"]), command=" ".join(cmd), raw=raw)
    try:
        found = parse_cargo_audit(json.loads(out))
    except (json.JSONDecodeError, KeyError, TypeError):
        return failed("cargo-audit", title, f"unreadable output (exit {code})", exit_code=code, **kw)
    vulns = sum(f["severity"] == "high" for f in found)
    summary = f"{vulns} vulnerabilities, {len(found) - vulns} warnings (unmaintained, unsound, yanked)"
    return result("cargo-audit", title, "findings" if found else "clean", summary, found,
                  exit_code=code, **kw)


def parse_cargo_deny(text):
    found, warnings, summary = [], Counter(), None
    for item in json_lines(text):
        if item.get("type") == "summary":
            summary = item["fields"]
            continue
        if item.get("type") != "diagnostic":
            continue
        fields = item.get("fields", {})
        severity, code = fields.get("severity"), fields.get("code") or "diagnostic"
        if severity == "error":
            adv = fields.get("advisory") or {}
            span = next((label.get("span") for label in fields.get("labels", [])), None)
            found.append(finding(adv.get("id") or code, "medium", f"{code}: {fields.get('message', '')}",
                                 file="Cargo.lock", package=span))
        elif severity == "warning":
            warnings[code] += 1
    return found, warnings, summary


def cargo_deny(r):
    title = "cargo deny"
    if not r.which("cargo-deny"):
        return missing("cargo-deny", title, "cargo-deny")
    cmd = ["cargo", f"+{RUST_TOOLCHAIN}", "deny", "--locked", "--format", "json", "--color", "never",
           "check", "--hide-inclusion-graph"]
    code, out, err, _ = r.run(cmd)
    raw = [r.save("cargo-deny.jsonl", err)]
    kw = dict(version=r.version(["cargo-deny", "--version"]), command=" ".join(cmd), raw=raw)
    found, warnings, totals = parse_cargo_deny(err)
    if totals is None or (code != 0 and not found):
        return failed("cargo-deny", title, f"no summary or unexplained exit {code}", exit_code=code, **kw)
    errors = {k: v.get("errors", 0) for k, v in totals.items()}
    summary = ("errors: " + ", ".join(f"{k} {v}" for k, v in errors.items())
               + "; warnings: " + (", ".join(f"{k} {v}" for k, v in warnings.items()) or "none"))
    return result("cargo-deny", title, "findings" if found else "clean", summary, found,
                  exit_code=code, extra={"warning_counts": dict(warnings)}, **kw)


# -- OSV --------------------------------------------------------------------

def cvss_severity(score):
    try:
        value = float(score)
    except (TypeError, ValueError):
        return "unknown"
    return ("critical" if value >= 9 else "high" if value >= 7 else
            "medium" if value >= 4 else "low")


def parse_osv(data):
    found = []
    for res in data.get("results", []):
        path = res["source"]["path"]
        rel = os.path.relpath(path, ROOT) if os.path.isabs(path) else path
        for pkg in res.get("packages", []):
            info = pkg["package"]
            for group in pkg.get("groups", []):
                ids = group.get("ids", [])
                titles = [v.get("summary", "") for v in pkg.get("vulnerabilities", [])
                          if v.get("id") in ids]
                found.append(finding(", ".join(ids), cvss_severity(group.get("max_severity")),
                                     next((t for t in titles if t), "advisory"), file=rel,
                                     package=f"{info['name']} {info['version']}"))
    return found


def osv_scanner(r):
    title = "osv-scanner"
    if not r.which("osv-scanner"):
        return missing("osv-scanner", title, "osv-scanner")
    cmd = ["osv-scanner", "scan", "source", "--format", "json"]
    for lockfile in LOCKFILES:
        cmd += ["-L", lockfile if lockfile.endswith(".lock") else f"requirements.txt:{lockfile}"]
    code, out, err, _ = r.run(cmd)
    raw = [r.save("osv-scanner.json", out), r.save("osv-scanner.stderr.txt", err)]
    kw = dict(version=r.version(["osv-scanner", "--version"]), command=" ".join(cmd), raw=raw)
    # 0: nothing found, 1: vulnerabilities found; anything else is an error.
    if code not in (0, 1):
        return failed("osv-scanner", title, f"exit {code}", exit_code=code, **kw)
    try:
        found = parse_osv(json.loads(out))
    except (json.JSONDecodeError, KeyError, TypeError):
        return failed("osv-scanner", title, f"unreadable output (exit {code})", exit_code=code, **kw)
    by_file = Counter(f["file"] for f in found)
    summary = (f"{len(found)} advisory groups in " + ", ".join(f"{k} ({v})" for k, v in by_file.items())
               if found else "no advisories in " + ", ".join(LOCKFILES))
    return result("osv-scanner", title, "findings" if found else "clean", summary, found,
                  exit_code=code, **kw)


# -- Secrets ----------------------------------------------------------------

def sanitize_gitleaks(items, scope):
    """File, rule, line and commit only: never the match or the secret."""
    return [finding(item.get("RuleID", "?"), "high", f"gitleaks {scope}: {item.get('Description', '')}",
                    file=item.get("File"), line=item.get("StartLine"),
                    package=(item.get("Commit") or "")[:12] or None)
            for item in items]


def gitleaks(r):
    title = "gitleaks"
    if not r.which("gitleaks"):
        return missing("gitleaks", title, "gitleaks")
    common = ["--config", str(ROOT / ".gitleaks.toml"), "--redact", "--no-banner", "--no-color",
              "--exit-code", "1", "--report-format", "json"]
    found, logs, statuses = [], [], {}
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        tree = tmp / "tree"
        tree.mkdir()
        # The committed tree of HEAD: untracked and ignored files (keys, build
        # output, local results) are not part of the repository.
        archive = subprocess.run(["git", "archive", "--format=tar", "HEAD"], cwd=ROOT,
                                 capture_output=True, check=True).stdout
        subprocess.run(["tar", "-x", "-C", str(tree)], input=archive, check=True)
        for scope, cmd in (
            ("history", ["gitleaks", "git", "--log-opts=--all", *common,
                         "--report-path", str(tmp / "history.json"), "."]),
            ("tree", ["gitleaks", "dir", *common, "--report-path", str(tmp / "tree.json"), str(tree)]),
        ):
            code, out, err, _ = r.run(cmd)
            statuses[scope] = code
            # The log lines carry counts only ("no leaks found", "leaks found: N").
            logs.append(f"$ gitleaks {scope} (exit {code})\n" + "\n".join(
                line for line in err.splitlines() if " INF " in line or " WRN " in line))
            try:
                items = json.loads((tmp / f"{scope}.json").read_text())
            except (OSError, json.JSONDecodeError):
                items = None
            if code not in (0, 1) or items is None:
                return failed("gitleaks", title, f"{scope} scan exit {code}", exit_code=code,
                              version=r.version(["gitleaks", "version"]))
            for item in sanitize_gitleaks(items, scope):
                if scope == "tree":
                    item["file"] = os.path.relpath(item["file"], tree) if item["file"] else None
                found.append(item)
    raw = [r.save("gitleaks.json", json.dumps(found, indent=1)), r.save("gitleaks.log.txt", "\n".join(logs))]
    summary = (f"{len(found)} findings (history: all commits of all refs; tree: HEAD)" if found
               else "no leaks in history (all refs) or the HEAD tree")
    return result("gitleaks", title, "findings" if found else "clean", summary, found,
                  exit_code=max(statuses.values()), version=r.version(["gitleaks", "version"]),
                  command="gitleaks git --log-opts=--all; gitleaks dir <git archive HEAD>", raw=raw)


# -- Source -----------------------------------------------------------------

# Not inside strings such as a CSP's 'unsafe-inline'.
UNSAFE = re.compile(r"(?<![-'\"])\bunsafe\b(?!-)")


def count_unsafe(files):
    found, attrs = [], []
    for name in files:
        for number, line in enumerate((ROOT / name).read_text(encoding="utf-8").splitlines(), 1):
            code = line.split("//", 1)[0]
            if re.search(r"#!\[(forbid|deny)\(unsafe_code\)\]", code):
                attrs.append(f"{name}:{number} {code.strip()}")
            elif UNSAFE.search(code) and "unsafe_code" not in code:
                found.append(finding("unsafe", "low", code.strip(), file=name, line=number))
    return found, attrs


def unsafe_count(r):
    title = "unsafe count"
    files = subprocess.check_output(["git", "ls-files", "crates/*.rs"], cwd=ROOT, text=True).split()
    found, attrs = count_unsafe(files)
    content = "\n".join(attrs + [f"{f['file']}:{f['line']}: {f['message']}" for f in found])
    raw = [r.save("unsafe-count.txt", content + "\n")]
    summary = (f"{len(found)} unsafe uses in {len(files)} workspace .rs files; lint attributes: "
               + "; ".join(attrs) + ". Dependencies not counted (cargo-geiger not run).")
    return result("unsafe-count", title, "findings" if found else "clean", summary, found,
                  exit_code=0, version="grep (harness/security_tools.py)",
                  command="regex \\bunsafe\\b over git ls-files crates/*.rs", raw=raw,
                  informational=True)


def clippy_panics(r):
    title = "clippy panic surface"
    if not r.which("cargo"):
        return missing("clippy-panics", title, "cargo")
    lints = [arg for lint in CLIPPY_LINTS for arg in ("-W", f"clippy::{lint}")]
    cmd = ["cargo", f"+{RUST_TOOLCHAIN}", "clippy", "--locked", "--workspace", "--lib", "--bins",
           "--message-format=json", "--", *lints]
    code, out, err, _ = r.run(cmd)
    raw = [r.save("clippy-panics.jsonl", out), r.save("clippy-panics.stderr.txt", err)]
    kw = dict(version=r.version(["cargo", f"+{RUST_TOOLCHAIN}", "clippy", "--version"]),
              command=" ".join(cmd), raw=raw, informational=True)
    if code != 0:
        return failed("clippy-panics", title, f"exit {code}", exit_code=code, **kw)
    found = []
    for item in json_lines(out):
        if item.get("reason") != "compiler-message":
            continue
        message = item["message"]
        lint = (message.get("code") or {}).get("code") or ""
        if not lint.startswith("clippy::"):
            continue
        span = next((s for s in message["spans"] if s["is_primary"]), None)
        found.append(finding(lint, "info", message["message"],
                             file=span and span["file_name"], line=span and span["line_start"]))
    counts = Counter(f["rule"] for f in found)
    summary = (f"{len(found)} lint hits in non-test code: "
               + ", ".join(f"{k.split('::')[1]} {v}" for k, v in counts.most_common()))
    return result("clippy-panics", title, "findings" if found else "clean", summary, found,
                  exit_code=0, extra={"lint_counts": dict(counts)}, **kw)


def semgrep(r):
    title = "semgrep"
    binary = r.which("semgrep") or (r.tools_dir / "semgrep-venv/bin/semgrep")
    rules = r.tools_dir / "semgrep-rules"
    if not Path(binary).exists():
        return missing("semgrep", title, "semgrep")
    try:
        commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=rules, text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        commit = None
    if commit != SEMGREP_RULES_COMMIT:
        return missing("semgrep", title, f"semgrep-rules at {SEMGREP_RULES_COMMIT[:12]}")
    cmd = [str(binary), "scan", "--metrics", "off", "--disable-version-check", "--json", "--quiet",
           "--config", str(rules / "rust"), "--config", str(rules / "python/lang/security"),
           # Tracked files only: untracked local files and results stay out.
           *subprocess.check_output(["git", "ls-files", "crates/*.rs", "harness/*.py"],
                                    cwd=ROOT, text=True).split()]
    code, out, err, _ = r.run(cmd)
    raw = [r.save("semgrep.json", out), r.save("semgrep.stderr.txt", err)]
    kw = dict(version=r.version([str(binary), "--version"]) + f" (rules {commit[:12]})",
              command=" ".join(cmd[:11]).replace(str(r.tools_dir), "$TOOLS") + " <tracked .rs and .py files>",
              raw=raw)
    try:
        data = json.loads(out)
    except json.JSONDecodeError:
        return failed("semgrep", title, f"unreadable output (exit {code})", exit_code=code, **kw)
    if code not in (0, 1):
        return failed("semgrep", title, f"exit {code}", exit_code=code, **kw)
    severity = {"ERROR": "high", "WARNING": "medium", "INFO": "low"}
    found = [finding(item["check_id"].rsplit(".", 1)[-1],
                     severity.get(item["extra"].get("severity"), "low"),
                     item["extra"].get("message", ""), file=item["path"], line=item["start"]["line"])
             for item in data.get("results", [])]
    errors = len(data.get("errors", []))
    summary = f"{len(found)} matches of the pinned rust and python security rules; {errors} parse errors"
    return result("semgrep", title, "findings" if found else "clean", summary, found,
                  exit_code=code, **kw)


TOOLS = {
    "cargo-audit": cargo_audit,
    "cargo-deny": cargo_deny,
    "osv-scanner": osv_scanner,
    "gitleaks": gitleaks,
    "unsafe-count": unsafe_count,
    "clippy-panics": clippy_panics,
    "semgrep": semgrep,
}


def run_tools(run_dir, tools_dir=DEFAULT_TOOLS_DIR, only=None):
    runner = Runner(run_dir, tools_dir)
    records = []
    for name, tool in TOOLS.items():
        if only and name not in only:
            continue
        start = time.monotonic()
        try:
            record = tool(runner)
        except Exception as error:  # A broken tool is reported, never skipped.
            record = failed(name, name, f"{type(error).__name__} in the runner")
        record["seconds"] = round(time.monotonic() - start, 1)
        records.append(record)
        print(f"  {name}: {record['status']}; {record['summary'][:150]}")
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = subprocess.run(["git", "diff", "--quiet", "HEAD", "--", "crates", "Cargo.lock",
                            "Cargo.toml", "deny.toml"], cwd=ROOT).returncode != 0
    meta = {"git_head": head, "tracked_sources_differ_from_head": dirty,
            "completed_at_utc": datetime.now(timezone.utc).isoformat()}
    with (run_dir / "tools.jsonl").open("w", encoding="utf-8") as out:
        for record in records:
            out.write(json.dumps(record) + "\n")
    (run_dir / "tools-run.json").write_text(json.dumps(meta, indent=2) + "\n")
    return records, meta


def load_tools(path):
    records = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    for record in records:
        if not (isinstance(record, dict) and isinstance(record.get("tool"), str)
                and record.get("status") in ("clean", "findings", "missing", "failed")
                and isinstance(record.get("findings"), list)):
            raise ValueError("Invalid tool record")
    return records


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("run_dir", type=Path)
    parser.add_argument("--tools-dir", type=Path, default=DEFAULT_TOOLS_DIR)
    parser.add_argument("--only", nargs="+", choices=tuple(TOOLS))
    args = parser.parse_args()
    args.run_dir.mkdir(parents=True, exist_ok=True)
    run_tools(args.run_dir, args.tools_dir, args.only)
