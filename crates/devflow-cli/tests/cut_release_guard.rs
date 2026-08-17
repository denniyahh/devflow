//! Structural guards for the release-signing guard in `scripts/cut-release.sh`.
//!
//! `scripts/cut-release.sh` hand-cuts the release tag. Before `tag -s` it must
//! resolve `devflow.releaseSigningKey` and fail loudly when the key is unset or
//! its file is unreadable — otherwise `git config --get` returns empty and the
//! tag silently signs with whatever `user.signingkey` defaults to (the agent's
//! key, not the maintainer's) — the wrong-identity trap 999.104 catalogues.
//!
//! These tests assert on the script's *source*, in the same idiom as
//! `pre_push_signing_policy.rs` / `ci_parity_guards.rs`, rather than executing
//! it. Executing `cut-release.sh` would need a real remote, a merged PR, and a
//! real signing key; the properties that actually matter are statically
//! checkable and cost nothing to run on every build.

use std::path::{Path, PathBuf};

/// Cargo test binaries run with cwd = the crate dir; the script lives at the
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

fn script() -> String {
    read(&repo_root().join("scripts/cut-release.sh"))
}

/// The unset-key guard must run BEFORE the `tag -s` invocation. `git config
/// --get` on a missing key returns empty under `|| true`, so the guard is the
/// only thing standing between a silent wrong-identity signature and a loud
/// failure. (999.104.)
#[test]
fn unset_signing_key_fails_loudly_before_tagging() {
    let source = script();

    assert!(
        source.contains("devflow.releaseSigningKey is not set"),
        "cut-release.sh must print a 'devflow.releaseSigningKey is not set' \
         diagnostic when the key is unset"
    );

    // The `|| true` keeps `set -e` from aborting at the assignment before the
    // guard can explain itself (the same trap `pre_push_signing_policy.rs`
    // guards for the hook).
    let resolve_line = source
        .lines()
        .find(|l| l.contains("local release_key; release_key="))
        .expect("cut-release.sh must resolve devflow.releaseSigningKey via git config --get");
    assert!(
        resolve_line.contains("|| true"),
        "the release-key resolution must tolerate an unset key (`|| true`), \
         otherwise `set -e` aborts at the assignment and the guard never \
         explains itself\n  offending line: {}",
        resolve_line.trim()
    );

    // Ordering: the guard's `exit 1` must precede the `tag -s` line.
    let unset_guard = source
        .find("if [ -z \"$release_key\" ]")
        .expect("cut-release.sh must guard the unset-key case");
    let tag = source
        .find("tag -s")
        .expect("cut-release.sh must invoke tag -s");
    assert!(
        unset_guard < tag,
        "the unset-key guard must fire before `tag -s` so a missing key fails \
         loudly rather than silently signing with the wrong identity"
    );
}

/// An unreadable key file must fail just as loudly as an unset key — a present
/// but unreadable path would otherwise be handed to `git -c user.signingkey=`
/// and fail later (or worse, silently fall back).
#[test]
fn unreadable_key_file_fails_loudly() {
    let source = script();
    assert!(
        source.contains(r#"[ ! -r "$release_key_expanded" ]"#),
        "cut-release.sh must guard against an unreadable key file with a \
         readability test on the tilde-expanded path"
    );
    assert!(
        source.contains("points at an unreadable file"),
        "cut-release.sh must name the unreadable-file failure mode in its \
         diagnostic"
    );
}

/// The guard is additive: it must not change the sign invocation's format. The
/// deterministic `git -c user.signingkey=` override is what pins the maintainer
/// key to the tag.
#[test]
fn deterministic_override_is_preserved() {
    let source = script();
    assert!(
        source.contains(r#"git -c user.signingkey="$release_key_expanded""#),
        "the tag must still be signed with the resolved maintainer key via the \
         in-code `git -c user.signingkey=` override"
    );
}
