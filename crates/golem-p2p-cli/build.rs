//! Embeds the git commit the CLI is built from, for `--version` and
//! telemetry reports: `GOLEM_P2P_COMMIT` if set, else `git rev-parse`, else
//! nothing.

use std::{path::PathBuf, process::Command};

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !s.is_empty()).then_some(s)
}

fn main() {
    println!("cargo:rerun-if-env-changed=GOLEM_P2P_COMMIT");
    // Rebuild when HEAD moves (commit, checkout); without git, never.
    if let Some(dir) = git(&["rev-parse", "--git-dir"]) {
        let dir = PathBuf::from(dir);
        for f in ["HEAD", "logs/HEAD"] {
            let p = dir.join(f);
            if p.exists() {
                println!("cargo:rerun-if-changed={}", p.display());
            }
        }
    } else {
        println!("cargo:rerun-if-changed=build.rs");
    }
    let commit = std::env::var("GOLEM_P2P_COMMIT")
        .ok()
        .filter(|c| !c.is_empty())
        .or_else(|| git(&["rev-parse", "--short=9", "HEAD"]));
    if let Some(c) = commit {
        println!("cargo:rustc-env=GOLEM_P2P_COMMIT={c}");
    }
}
