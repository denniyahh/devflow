//! Build-time provenance embedding (D-20, Phase 17d; CR-02 fix, 17-11).
//!
//! Shells out to `git` to capture the commit and dirty flag, then embeds
//! them as compile-time env vars via `cargo:rustc-env` so
//! `crates/devflow-cli/src` can read them with `env!(...)` at runtime
//! (consumed by `workflow_started`'s payload and the staleness check).
//!
//! Hand-rolled per D-20 — no `[build-dependencies]` (no `vergen`, `git2`,
//! `chrono`/`time`). Must never fail the build when git metadata is
//! unavailable (e.g. a `cargo install` from crates.io with no `.git`
//! present) — absence of provenance is not staleness.
//!
//! **Always re-runs (CR-02).** `git status --porcelain` (the whole working
//! tree) and `SystemTime::now()` are inputs cargo cannot fingerprint by
//! path — no `rerun-if-changed` path expresses "the working tree changed"
//! or "time passed". The previous version watched only `HEAD`/`refs`/
//! `packed-refs`, so editing a tracked source file without moving a ref
//! left the build script's cached output stale: `DEVFLOW_BUILD_DIRTY`
//! stayed `false` after an edit that should have flipped it `true`. It was
//! masked on a developer machine by accident — `.git/packed-refs` doesn't
//! exist locally, and cargo treats a missing `rerun-if-changed` path as
//! *always rerun* — but any `git gc` or CI checkout (which packs refs)
//! exposed it (17-REVIEW.md CR-02).
//!
//! The fix: declare a single sentinel path that can never exist, so cargo
//! always reruns this script. That alone would recompile `devflow-cli` on
//! every build if the embedded `rustc-env` values changed every run — but
//! the previous version's per-second `DEVFLOW_BUILD_TIMESTAMP` was the only
//! thing that did that. Dropping it (see below) means `rustc-env` only
//! changes when the commit or the dirty flag actually changes, so
//! always-rerunning this script (a few cheap git calls) does not cause
//! spurious recompiles of the crate it builds for.
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Always re-run (CR-02): no `rerun-if-changed` path can express "the
    // working tree's dirty state may have changed since the last build" or
    // "time passed", so this script must run on every `cargo build` rather
    // than relying on cargo's path-fingerprinted rerun cache. A path that
    // can never exist forces cargo's "missing input => always rerun" rule
    // unconditionally, replacing the old HEAD/refs/packed-refs watch list.
    println!("cargo:rerun-if-changed=NEVER-EXISTS-devflow-cli-always-rerun-build-rs");

    // CARGO_MANIFEST_DIR is crates/devflow-cli/ — one level below the
    // workspace root; `run_git` below runs with this as its cwd, which is
    // sufficient for git to resolve the enclosing repo on its own.
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));

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
    // No DEVFLOW_BUILD_TIMESTAMP (CR-02): a per-second wall-clock value
    // changed every build, forcing `rustc-env` (and thus a recompile of the
    // crate reading it) on every single `cargo build` once this script
    // always reruns. Only `commit`/`dirty` are embedded now; staleness is
    // decided from those two plus a live working-tree check at call time
    // (`main.rs`'s `combined_staleness`), not a cached timestamp.
    println!("cargo:rustc-env=DEVFLOW_BUILD_DIRTY={dirty}");
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
