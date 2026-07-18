//! Build-time provenance embedding (D-20, Phase 17d).
//!
//! Shells out to `git` to capture the commit, dirty flag, and a build
//! timestamp, then embeds them as compile-time env vars via
//! `cargo:rustc-env` so `crates/devflow-cli/src` can read them with
//! `env!(...)` at runtime (consumed by Plan 05's `workflow_started`
//! payload and staleness check).
//!
//! Hand-rolled per D-20 — no `[build-dependencies]` (no `vergen`, `git2`,
//! `chrono`/`time`). Must never fail the build when git metadata is
//! unavailable (e.g. a `cargo install` from crates.io with no `.git`
//! present) — absence of provenance is not staleness.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // CARGO_MANIFEST_DIR is crates/devflow-cli/ — one level below the
    // workspace root. A relative `.git/HEAD` rerun-if-changed path would
    // resolve against this directory and watch a path that does not
    // exist, so first resolve the actual git common dir from git itself.
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));

    if let Some(git_common_dir) = run_git(&manifest_dir, &["rev-parse", "--git-common-dir"]) {
        let git_dir = PathBuf::from(&git_common_dir);
        let git_dir = if git_dir.is_absolute() {
            git_dir
        } else {
            manifest_dir.join(git_dir)
        };
        // Re-run only when git refs actually move — not on every `cargo
        // build`. `packed-refs` is required alongside `HEAD`/`refs` so
        // tag/branch movement in packed-ref repos and fetch-only ref
        // updates also trigger a rebuild (review consensus #7).
        println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
        println!("cargo:rerun-if-changed={}", git_dir.join("refs").display());
        println!(
            "cargo:rerun-if-changed={}",
            git_dir.join("packed-refs").display()
        );
    }
    // When `rev-parse --git-common-dir` fails (no-git / crates.io case),
    // skip the rerun-if-changed lines entirely rather than emitting a
    // broken relative path.

    let commit = run_git(&manifest_dir, &["rev-parse", "HEAD"]);
    let dirty = run_git(&manifest_dir, &["status", "--porcelain"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    // D-20: MUST degrade gracefully when git metadata is unavailable —
    // emit an empty string rather than failing the build. Absence of
    // provenance is not staleness.
    println!(
        "cargo:rustc-env=DEVFLOW_BUILD_COMMIT={}",
        commit.unwrap_or_default()
    );
    println!("cargo:rustc-env=DEVFLOW_BUILD_DIRTY={dirty}");
    println!(
        "cargo:rustc-env=DEVFLOW_BUILD_TIMESTAMP={}",
        // This is the BUILD MACHINE's wall-clock at compile time, NOT
        // the commit timestamp. That's correct for Plan 05's mtime
        // staleness comparison, but it is not a forensic commit time.
        // Hand-rolled per ship.rs's no-date-crate precedent; defaults
        // to 0 on clock error.
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
}

/// Shell out to `git` with an argv-array `Command` (never `sh -c` string
/// interpolation). Returns `None` on any failure — missing `git` binary,
/// non-git directory, non-zero exit — so the build never panics or fails
/// when git metadata is unavailable.
fn run_git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
