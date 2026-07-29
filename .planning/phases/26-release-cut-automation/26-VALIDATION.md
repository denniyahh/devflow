---
phase: 26
slug: release-cut-automation
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-29
---

# Phase 26 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Derived from `26-RESEARCH.md` § Validation Architecture.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` harness via `cargo test` (workspace) — no separate framework |
| **Config file** | none — `Cargo.toml`'s `[workspace]`/per-crate `[dev-dependencies]` is the only config |
| **Quick run command** | `cargo test --workspace <module>::` (targeted) or `devflow test` |
| **Full suite command** | `devflow test` / `scripts/check.sh all` / `scripts/check-in-container.sh all` |
| **Estimated runtime** | ~180 seconds full suite (Phase 25's last measurement, 618 tests / 17 binaries — this phase adds new modules so expect modest growth); targeted module filters run in seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p devflow-core <module just touched>::` (targeted, fast)
- **After every plan wave:** Run `devflow test` — the same fmt+clippy+test triad CI/pre-push require
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** ~180 seconds (full suite); targeted filters well under 30s

**999.25 exception — local-green is necessary but not sufficient for the tag/publish code paths.** No sandbox in the research session had real signing keys or crates.io publish credentials to exercise the signed-tag and `cargo publish` steps end-to-end. Per this project's own Phase 20 precedent (UAT-style operator confirmation for irreversible operations), the tag and publish steps additionally need a live operator-observed confirmation against a real (non-production) registry/repo before being trusted unattended — automated tests alone cannot close this gap.

---

## Per-Task Verification Map

> Task IDs are assigned by the planner. This map is keyed by unit (999.52 / 999.5 / 999.25) until plans exist; the planner must expand it to per-task rows.

| Unit | Requirement | Behavior | Test Type | Automated Command | File Exists | Status |
|------|-------------|----------|-----------|-------------------|-------------|--------|
| 999.52 | sync | `devflow sync` refuses on dirty tree | unit | `cargo test -p devflow-core sync::tests::refuses_on_dirty_tree -- --exact` | ❌ W0 (new `sync.rs`) | ⬜ pending |
| 999.52 | sync | `devflow sync` refuses when not on `develop` | unit | `cargo test -p devflow-core sync::tests::refuses_off_develop -- --exact` | ❌ W0 | ⬜ pending |
| 999.52 | sync | Short-circuits when `origin/main` is already an ancestor of `develop` | unit (bare-remote fixture) | `cargo test -p devflow-core sync::tests::noop_when_already_synced -- --exact` | ❌ W0 | ⬜ pending |
| 999.52 | sync | Tree-identity mismatch aborts, leaves `develop` untouched, no push (D-09) | unit (bare-remote fixture) | `cargo test -p devflow-core sync::tests::aborts_on_tree_mismatch -- --exact` | ❌ W0 | ⬜ pending |
| 999.52 | sync | Successful sync pushes to origin (D-08) | unit (bare-remote fixture) | `cargo test -p devflow-core sync::tests::pushes_on_success -- --exact` | ❌ W0 | ⬜ pending |
| 999.5 | changelog | Changelog entry groups commits by type (feat→Added etc.), replaces hardcoded `"Released phase via DevFlow."` (D-12) | unit | `cargo test -p devflow-core version::tests::changelog_content_groups_by_type -- --exact` | ❌ W0 | ⬜ pending |
| 999.5 | changelog | Empty range (no version-affecting commits) still produces a sensible entry, not an empty one | unit | `cargo test -p devflow-core version::tests::changelog_content_handles_empty_range -- --exact` | ❌ W0 | ⬜ pending |
| 999.25 | executor | Version-bump commit pushes to `develop` (D-01), fast-forward only, never `--force` | unit (bare-remote fixture) | `cargo test -p devflow-core git::tests::version_bump_pushes_develop -- --exact` | ❌ W0 | ⬜ pending |
| 999.25 | executor | Idempotent resume: `develop` already at/ahead of computed version → push step skipped (D-06) | unit | `cargo test -p devflow-core git::tests::skips_push_when_already_ahead -- --exact` | ❌ W0 | ⬜ pending |
| 999.25 | executor | Tag step: existing annotated+reachable+pushed tag → skip (D-06) | unit | `cargo test -p devflow-core git::tests::skips_tag_when_already_released -- --exact` | ❌ W0 | ⬜ pending |
| 999.25 | executor | Tag step: existing *lightweight* tag with the same name does NOT silently count as released (collision with `hooks_after_ship`'s local unsigned tag — confirmed real via `v1.3.69` in this repo) | unit | `cargo test -p devflow-core git::tests::stray_lightweight_tag_is_not_treated_as_released -- --exact` | ❌ W0 | ⬜ pending |
| 999.25 | executor | Pre-publish check: `cargo info` exit 0 → already-published (skip); `could not find` in stderr → not-published (proceed); other → ambiguous error (fail loud, do not guess) | unit (fake `cargo` shim on PATH, or integration test against a known-immutable old version e.g. `serde@1.0.1`) | `cargo test -p devflow-core git::tests::publish_check_classifies_exit_codes -- --exact` | ❌ W0 | ⬜ pending |
| 999.25 | executor | Publish order still matches `publish_order`'s existing topo-sort (no recomputation, D-04) | unit (existing) | `cargo test -p devflow-core git::tests::publish_order_derives_core_before_cli_from_a_fixture_workspace -- --exact` | ✅ existing | ⬜ pending |
| 999.25 | executor | `--yes-release` required per-invocation, never settable via config/env (mirrors `--yes-ship`, D-03) | unit/CLI | new `crates/devflow-cli/tests/release_execute.rs`, following `release_check.rs`'s existing shape | ❌ W0 | ⬜ pending |
| 999.25 | executor | Fail-fast, no rollback (D-05): a mid-sequence failure (e.g. `devflow-core` publish succeeds, `devflow` publish fails) leaves prior steps landed | integration | `cargo test -p devflow-cli release_execute::tests::partial_failure_leaves_prior_steps_landed -- --exact` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/devflow-core/src/sync.rs` — new module, no existing file to extend
- [ ] `crates/devflow-cli/tests/release_execute.rs` — new integration test file, following `crates/devflow-cli/tests/release_check.rs`'s existing shape
- [ ] Bare-remote git fixture helper (extend `git.rs`'s existing `init_repo()`/`flow()` test pattern with a second bare-repo remote) — needed by every push-mutating test above; nothing today exercises a real push since no production function has ever pushed
- [ ] A fake/stub `cargo` shim for hermetic classification of `cargo info` exit codes, OR a small set of tests against the real crates.io registry using a known-immutable old version — the planner must pick one and note the hermeticity-vs-real-tool tradeoff

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Signed tag creation with the real operator signing key produces a `git tag -v`-verifiable tag | 999.25 (D-10) | No sandbox in research/CI has real signing keys; the exact `git -c user.signingkey=... tag -s` invocation must be observed against a real key at least once | Operator runs `--yes-release` against a throwaway/non-production repo (or the real repo, operator's call) with their own `devflow.releaseSigningKey` configured, confirms `git tag -v vX.Y.Z` passes |
| `cargo publish` for both crates completes and the packages are live in the correct order | 999.25 (D-04) | Publish credentials and the live registry state cannot be faked without risking a real, irreversible publish | Operator observes the first real `--yes-release` run's publish steps end-to-end; verify `devflow-core` lands before `devflow` per `publish_order` |
| Sync (`devflow sync`) direct-pushes `develop` without a human clicking a GitHub merge-strategy button | 999.52 (D-08) | Requires the operator's own out-of-scope GitHub ruleset bypass to already exist; cannot be simulated against the real `origin` in CI | Operator confirms the ruleset bypass is configured, then runs `devflow sync` (or the executor's internal sync step) once against the real repo and confirms the push lands as a direct push, not a PR |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 180s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
