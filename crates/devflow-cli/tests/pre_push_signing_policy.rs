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
///
/// Scans the full commit range (`git log --name-only`), not an endpoint diff
/// (`git diff --diff-filter=A`) or a whole-tree `git ls-tree` — a range scan is
/// the only one of the three that catches every case: content added then
/// removed within the same push (net-zero at the endpoints, but permanently
/// reachable via the intermediate commit), a rename into a forbidden path
/// (git classifies renames separately from adds), and a modification of an
/// already-existing forbidden path — while an endpoint diff misses all three
/// and a whole-tree check false-positives on every push once any commit ever
/// added a legitimately-tracked file under one of these prefixes. Each of
/// these was verified against real git fixtures during review, not asserted
/// from reasoning alone (999.11x).
#[test]
fn pre_push_guards_against_personal_artifacts_on_clean_branches() {
    let source = hook();
    assert!(
        source.contains("git log --name-only") && source.contains("refs/heads/develop"),
        "pre-push must scan the full commit range (not an endpoint diff or a whole-tree \
         ls-tree) to forbid personal artifacts on develop/main/feature branches"
    );
    let regex_line = source
        .lines()
        .find(|line| line.contains("forbidden=") && line.contains("grep -E"))
        .expect("pre-push must build forbidden via a grep -E regex line");
    assert!(
        regex_line.contains(".planning")
            && regex_line.contains(".codex")
            && regex_line.contains(".claude")
            && regex_line.contains(".worktrees"),
        "pre-push's forbidden-artifact regex must cover .planning, .codex, .claude, and \
         .worktrees (this last one predates this test but was missing from pre-push despite \
         being present in pre-commit's equivalent regex -- a --no-verify commit containing \
         .worktrees/ would reach this branch unchallenged otherwise)\n  regex line: {regex_line}"
    );
}

/// A push whose commit range cannot be resolved (e.g. `remote_sha` names an
/// object this clone has never fetched — a real, not hypothetical, state) must
/// refuse the push, not silently treat an unreadable range as "nothing
/// forbidden found." The prior endpoint-diff version piped stderr to
/// `/dev/null` and `|| true`-d the exit code, which fails OPEN: a `fatal: bad
/// object` from git collapsed to an empty result and the push proceeded.
#[test]
fn pre_push_fails_closed_when_the_commit_range_is_unresolvable() {
    let source = hook();
    let range_lines: Vec<&str> = source
        .lines()
        .filter(|line| line.contains("git log --name-only"))
        .collect();
    assert!(
        !range_lines.is_empty(),
        "expected at least one `git log --name-only` invocation building the commit range"
    );
    assert!(
        source.contains("range_rc") || source.contains("range_files\" 2>&1"),
        "the range-scan command's exit status must be captured (not `2>/dev/null | ... || true`) \
         so an unresolvable range can be distinguished from a genuinely empty one and refused"
    );
}

/// A brand-new branch's first-ever push reports an all-zero `remote_sha` —
/// there is no old tip to diff against. Scanning `git log "$local_sha"` alone
/// in that case walks the WHOLE history back to the repo's first commit, not
/// just what this push introduces: on this repo that includes
/// personal-artifact commits from before this hygiene policy existed, so
/// every branch's first push would be refused regardless of what it actually
/// contains. Verified directly: `git log --name-only "$local_sha"` on this
/// repo's real `develop` finds 1847 forbidden-pattern matches purely from
/// history, while `git log --name-only "$local_sha" --not --remotes=origin`
/// (scoped to commits not already reachable from any known remote ref) finds
/// zero on that same commit and still catches a genuinely new forbidden file
/// added on a fresh branch.
#[test]
fn pre_push_scopes_a_new_branchs_first_push_to_what_it_actually_introduces() {
    let source = hook();
    let zero_sha_line = source
        .lines()
        .find(|line| line.trim_start().starts_with("*)") && line.contains("git log --name-only"))
        .expect(
            "pre-push must have a `*)` (all-zero remote_sha) case arm building the commit range",
        );

    assert!(
        zero_sha_line.contains("--not") && zero_sha_line.contains("--remotes"),
        "the all-zero-remote_sha arm must scope its `git log` to commits not already reachable \
         from a known remote ref (`--not --remotes=...`), not scan `\"$local_sha\"` alone — \
         otherwise a brand-new branch's first push walks the entire repo history and is refused \
         over content this push does not introduce.\n  offending line: {}",
        zero_sha_line.trim()
    );
}
