# 36-02 Summary — Release-Path Hardening

**Plan:** 36-02 (wave 2) — 999.104 (deterministic release signing + probe removal) + 999.96
(version-bump row).
**Status:** complete — both tasks verified.

## What landed

### 999.104 — deterministic signing, probe removed, hook retained
- `scripts/cut-release.sh` `step_tag` now resolves `devflow.releaseSigningKey` and **fails loudly**
  (non-zero, clear message) before `tag -s` when it is unset or its file is unreadable — closing
  the `-c user.signingkey=` (empty) silent-wrong-signing trap.
- The capability-only signing probe is **removed** — `check_signing_viability`,
  `check_ssh_signing_viability`, `check_gpg_signing_viability`, `sign_probe_verdict`,
  `run_ssh_sign_probe`, `sign_probe_within`, `public_key_fingerprint`, `inline_signing_key_blob`,
  `probe_workspace_name`, the `SigningViability`/`SignProbeOutcome` enums, `SSH_SIGN_*` constants,
  and ~16 test functions + their helpers (a contiguous block at the end of `git.rs`'s test module).
- `scripts/hooks/pre-push` is **untouched** — it remains the identity check (agent key vs maintainer
  key) on the hand-cut release path, per the review.

### 999.96 — version-bump row
- `release --check` gains a `changelog version (matches workspace)` row comparing `CHANGELOG.md`'s
  top `## <version>` heading to the workspace version, reporting `fail` with direction
  ("changelog ahead of Cargo.toml" vs "Cargo.toml ahead of changelog") on a synthetic mismatched
  fixture, `ok` on agreement, `warn` on missing/malformed.

## Verification
- `cargo test -p devflow-core --lib`: **621 passed, 0 failed** (16 probe tests removed).
- `cargo test -p devflow --bin devflow`: **322 passed, 0 failed** (includes the new
  `changelog_version_check_flags_mismatch_and_passes_on_agreement`).
- `cargo clippy` on both crates: **clean** (no dead_code, no unused imports).
- `devflow release --check` output shows the new changelog row and no tag-signing row.

## Pre-existing issue noted (not from this change)
- `cargo test -p devflow-core` (integration tests `devflow_dir_gitignore`/`monitor_e2e`) fail to
  compile: they reference `devflow_core::test_support`, which is `#[cfg(test)]`-gated and thus
  unavailable to integration tests. This predates this phase (also flagged by the codex review).
