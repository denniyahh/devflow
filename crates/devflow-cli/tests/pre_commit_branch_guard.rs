//! Structural guards for the protected-branch commit guard in
//! `scripts/hooks/pre-commit`.
//!
//! `develop` and `main` are PR-protected: a repository ruleset rejects any
//! direct push. A commit made directly on either is therefore doomed to be
//! rejected at push time — but only after it has already been written, and the
//! remote rejection reads as a CI/ruleset failure rather than a local mistake.
//! The pre-commit hook refuses the commit up front, while the staged changes
//! can still be carried onto a branch with `git switch -c`.
//!
//! These tests assert on the hook's *source*, in the same idiom as
//! `pre_push_signing_policy.rs` and `ci_parity_guards.rs`, rather than
//! executing it. The properties that actually matter are statically checkable.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

fn hook() -> String {
    std::fs::read_to_string(repo_root().join("scripts/hooks/pre-commit"))
        .expect("read scripts/hooks/pre-commit")
}

/// The branch is detected via `git symbolic-ref --short HEAD`, not
/// `git rev-parse --abbrev-ref HEAD`. The latter returns `HEAD` (and errors)
/// on an unborn branch — the exact state of a fresh `git init` before its
/// first commit — so it would silently miss a commit on a protected branch
/// that has not yet been born.
#[test]
fn branch_is_detected_via_symbolic_ref_not_rev_parse() {
    let source = hook();
    assert!(
        source.contains("git symbolic-ref --short HEAD"),
        "the guard must detect the branch via `git symbolic-ref --short HEAD`; \
         `git rev-parse --abbrev-ref HEAD` returns `HEAD` on an unborn branch and \
         would miss the very first commit on develop/main"
    );
}

/// Both protected branches are named in the case arm, and the arm exits
/// non-zero. A guard that names only one branch, or that warns without
/// exiting, silently reopens the push-time rejection this hook exists to
/// prevent.
#[test]
fn both_protected_branches_refuse_with_a_nonzero_exit() {
    let source = hook();
    assert!(
        source.lines().any(|line| line.trim() == "develop | main)"),
        "the guard's case arm must match `develop | main`"
    );
    assert!(
        source.contains("exit 1"),
        "the guard must exit non-zero when it matches a protected branch"
    );
}
