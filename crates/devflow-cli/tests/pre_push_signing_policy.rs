//! Structural guards for the release-signing policy in
//! `scripts/hooks/pre-push`.
//!
//! This repository is developed with an AI agent that signs its own commits
//! with its own key, while releases must carry the human maintainer's key.
//! Both keys are configured with the same `user.email`, so a tag signed with
//! the wrong one renders identically everywhere a human looks — `git log`,
//! `git tag -v`'s signer line, and GitHub's "Verified" badge. Only the key
//! fingerprint differs.
//!
//! These tests assert on the hook's *source*, in the same idiom as
//! `ci_parity_guards.rs`, rather than executing it. Executing it would need a
//! fixture repository with real signing keys; the properties that actually
//! broke in review are all statically checkable, and a structural guard costs
//! nothing to run on every build.

use std::path::{Path, PathBuf};

/// Cargo test binaries run with cwd = the crate dir; the hook lives at the
/// repo root.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn hook() -> String {
    read(&repo_root().join("scripts/hooks/pre-push"))
}

/// REGRESSION (introduced and caught during review, 2026-07-27).
///
/// The fingerprint is extracted with `grep -oE 'SHA256:…' | head -1`. When a
/// tag carries no signature at all, `grep` matches nothing and exits 1 —
/// which, under this hook's `set -euo pipefail`, aborts the script *at the
/// assignment*. The push is still refused (non-zero exit), but the
/// "it is not signed" diagnostic below is never reached, so the operator sees
/// a bare non-zero exit and no reason.
///
/// A silent refusal is precisely the failure mode this project exists to
/// eliminate, so the `|| true` that keeps the script alive long enough to
/// explain itself is load-bearing, not defensive noise.
#[test]
fn unsigned_tag_extraction_cannot_abort_the_hook_before_it_explains_itself() {
    let source = hook();
    let line = source
        .lines()
        .find(|line| line.contains("got_fpr=") && line.contains("grep -oE"))
        .expect("pre-push must extract a signature fingerprint into got_fpr via grep");

    assert!(
        line.contains("|| true"),
        "the got_fpr extraction must tolerate a no-match grep. Without `|| true`, \
         `set -e` aborts the hook at this assignment for an UNSIGNED tag, refusing \
         the push silently instead of printing the 'it is not signed' diagnostic \
         that follows.\n  offending line: {}",
        line.trim()
    );
}

/// The two keys share a `user.email`, so the signer string is identical for
/// both and cannot distinguish them. Only the fingerprint can. A future
/// simplification that compares `%GS` or the "Good signature for <email>"
/// text would silently stop detecting the exact case this hook exists for.
#[test]
fn policy_compares_key_fingerprints_not_signer_identity() {
    let source = hook();

    assert!(
        source.contains("RELEASE_FPR") && source.contains("ssh-keygen -lf"),
        "the release key must be reduced to a fingerprint with `ssh-keygen -lf` \
         and compared as RELEASE_FPR"
    );
    assert!(
        source.contains(r#""$got_fpr" != "$RELEASE_FPR""#),
        "the policy check must compare the tag's fingerprint against the \
         configured release key's fingerprint"
    );
}

/// The container check takes minutes; this check takes milliseconds. A policy
/// violation must not wait on a build it is going to reject anyway.
#[test]
fn policy_runs_before_the_expensive_container_check() {
    let source = hook();
    let policy = source
        .find("RELEASE_KEY=")
        .expect("pre-push must read devflow.releaseSigningKey");
    let container = source
        .find("check-in-container.sh")
        .expect("pre-push must run the container check");

    assert!(
        policy < container,
        "the signing-policy guard must run before the container check so a \
         rejected push fails in milliseconds rather than minutes"
    );
}

/// `main` is squash-only via pull request. A direct push would bypass that and
/// carry whatever key signed its commits — which, on an agent-driven machine,
/// is the agent's.
#[test]
fn direct_pushes_to_main_are_refused() {
    let source = hook();
    assert!(
        source.contains("refs/heads/main"),
        "pre-push must refuse a direct push to main"
    );
}

/// Enforcement is opt-in so a contributor who never cuts a release needs no
/// setup — but once configured it must have no override, because an override
/// is exactly what a mistaken release reaches for.
#[test]
fn policy_is_opt_in_by_config_and_has_no_override_escape_hatch() {
    let source = hook();
    assert!(
        source.contains("devflow.releaseSigningKey"),
        "enforcement must key off devflow.releaseSigningKey"
    );

    for escape in [
        "DEVFLOW_SKIP_SIGNING",
        "SKIP_SIGNING",
        "DEVFLOW_ALLOW_AGENT_TAG",
    ] {
        assert!(
            !source.contains(escape),
            "no environment override may bypass the signing policy (found `{escape}`). \
             The escape hatch is to re-sign the tag with the correct key."
        );
    }
}

/// Personal development artifacts (.agents, .codex, .claude, .planning, .gsd, etc.)
/// must be rejected by pre-push on shared upstream branches (develop, main, feature/*).
#[test]
fn pre_push_guards_against_personal_artifacts_on_clean_branches() {
    let source = hook();
    assert!(
        source.contains("git ls-tree -r --name-only") && source.contains("refs/heads/develop"),
        "pre-push must inspect commit trees and forbid personal artifacts on develop/main/feature branches"
    );
    assert!(
        source.contains(".planning") && source.contains(".codex") && source.contains(".claude"),
        "pre-push forbidden artifact regex must cover .planning, .codex, and .claude"
    );
}
