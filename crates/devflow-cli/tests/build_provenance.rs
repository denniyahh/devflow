//! Asserts `build.rs`'s compile-time provenance env vars (D-20, Phase 17d)
//! resolve at test-target compile time via `env!`. Tolerant of the
//! degraded no-git build case (`DEVFLOW_BUILD_COMMIT` may be empty) per
//! D-20 — absence of provenance is not staleness.

#[test]
fn build_timestamp_is_a_parseable_u64() {
    let raw = env!("DEVFLOW_BUILD_TIMESTAMP");
    let parsed: u64 = raw
        .parse()
        .unwrap_or_else(|e| panic!("DEVFLOW_BUILD_TIMESTAMP {raw:?} did not parse as u64: {e}"));
    // This test target is built from a real git checkout (this repo), so a
    // normal build should produce a non-zero wall-clock timestamp. D-20's
    // degraded/no-git case is allowed to emit 0, but that path isn't
    // exercised by this workspace's own test build.
    assert!(
        parsed > 0,
        "expected a non-zero build timestamp in a normal build, got {parsed}"
    );
}

#[test]
fn build_dirty_is_exactly_true_or_false() {
    let dirty = env!("DEVFLOW_BUILD_DIRTY");
    assert!(
        dirty == "true" || dirty == "false",
        "DEVFLOW_BUILD_DIRTY must be exactly \"true\" or \"false\", got {dirty:?}"
    );
}

#[test]
fn build_commit_is_accessible_and_does_not_panic() {
    // May be empty in a no-git (crates.io) build per D-20 — accessing it
    // must never panic either way.
    let commit = env!("DEVFLOW_BUILD_COMMIT");
    let _ = commit.len();
}
