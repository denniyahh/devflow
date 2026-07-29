---
phase: 26
slug: release-cut-automation
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-29
validated: 2026-07-29
---

# Phase 26 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Derived from `26-RESEARCH.md` § Validation Architecture.
> Audited 2026-07-29 by `/gsd-validate-phase 26` — see § Validation Audit.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` harness via `cargo test` (workspace) — no separate framework |
| **Config file** | none — `Cargo.toml`'s `[workspace]`/per-crate `[dev-dependencies]` is the only config |
| **Quick run command** | `cargo test --workspace --features devflow-core/test-support --lib <filter>` |
| **Full suite command** | `devflow test` / `scripts/check.sh all` / `scripts/check-in-container.sh all` |
| **Test count (lib target)** | 417 in `devflow-core` lib (406 pre-phase + 11 added by 26-03) |
| **Estimated runtime** | ~180 seconds full suite; every filter used in this phase's map completes in under 2 seconds |

**Two invocation traps confirmed live during this phase — both recorded here so the commands in the map below are copy-pasteable:**

1. **`-p devflow-core` alone does not compile.** It fails to enable the `test-support`
   feature that `devflow-cli`'s dev-dependency turns on via workspace feature
   unification, so 3 pre-existing integration-test targets fail to *compile* with
   `cannot find test_support in devflow_core`. Confirmed pre-existing (reproduces on
   the pre-phase commit), recorded independently by both `26-02-SUMMARY.md` and
   `26-03-SUMMARY.md`. Use `--workspace` (what `scripts/check.sh` actually runs) or
   `-p devflow-core --features test-support`.
2. **A misspelled `-- --exact <name>` filter exits 0 having run nothing.** `test result:
   ok. 0 passed; 0 failed` is a *pass* to the shell. Every verification below must be
   confirmed by reading the `N passed` count, never by exit status alone.

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace --features devflow-core/test-support --lib <module just touched>::` (targeted, fast)
- **After every plan wave:** Run `devflow test` — the same fmt+clippy+test triad CI/pre-push require
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** ~180 seconds (full suite); targeted filters well under 30s

**999.25 exception — local-green is necessary but not sufficient for the tag/publish code paths.** No sandbox in the research session had real signing keys or crates.io publish credentials to exercise the signed-tag and `cargo publish` steps end-to-end. Per this project's own Phase 20 precedent (UAT-style operator confirmation for irreversible operations), the tag and publish steps additionally need a live operator-observed confirmation against a real (non-production) registry/repo before being trusted unattended — automated tests alone cannot close this gap.

---

## Per-Task Verification Map — Part A: Delivered Behavior

> Every row below was re-run live during the 2026-07-29 audit, not taken from
> the executing agents' self-reported `status: pass`. Pass counts are the
> observed `N passed` values.

| # | Plan | Requirement | Behavior | Test Type | Automated Command (filter) | Status |
|---|------|-------------|----------|-----------|----------------------------|--------|
| A1 | 26-02 | 999.5 | Every recognized conventional type maps to its Keep-a-Changelog heading (`feat`→Added, `fix`/`perf`→Fixed, rest→Changed) | unit | `version::tests::changelog_sections_maps_every_recognized_type` | ✅ green |
| A2 | 26-02 | 999.5 | Breaking markers (`!` suffix and `BREAKING CHANGE:` footer, both forms) route to Breaking, checked before the type match | unit | `version::tests::changelog_sections_routes_breaking_changes_to_their_own_heading` | ✅ green |
| A3 | 26-02 | 999.5 | Messages that genuinely fail `git_conventional::Commit::parse` fall through to Changed, not dropped | unit | `version::tests::changelog_sections_treats_unparseable_messages_as_changed` | ✅ green |
| A4 | 26-02 | 999.5 | Empty range (no version-affecting commits) yields no sections, and `prepend_changelog` renders its "no changes recorded" fallback rather than an empty entry | unit | `version::tests::changelog_sections_returns_no_sections_for_an_empty_range` | ✅ green |
| A5 | 26-02 | 999.5 | `prepend_changelog` renders the generated body under the version heading (4th `body` param) | unit | `ship::tests::prepend_changelog_uses_the_generated_body` | ✅ green |
| A6 | 26-02 | 999.5 | End-to-end: a real `VersionBump`→`ChangelogAppend` hook run writes the generated body into `CHANGELOG.md` bytes | integration (fixture repo) | `hooks::tests::changelog_append_writes_the_generated_body_end_to_end` | ✅ green |
| A7 | 26-02 | 999.5 | Commit-derived subjects are stripped of control characters and capped at `CHANGELOG_SUBJECT_MAX_CHARS` (T-26-05) | unit | `version::tests::sanitize_changelog_subject_neutralizes_controls_and_caps_length` | ✅ green |
| A8 | 26-02 | 999.5 | Sanitization is applied at every `changelog_sections` push site, not once at the end | unit | `version::tests::changelog_sections_sanitizes_subjects_before_grouping` | ✅ green |
| A9 | 26-02 | 999.5 | Hardcoded `"Released phase via DevFlow."` placeholder fully retired (D-12) | guard | `rg -n 'Released phase via DevFlow' crates/` → no matches (exit 1) | ✅ green |
| A10 | 26-03 | 999.25 | `push_ref` lands a branch on a real (bare) remote — no `-u`, no force flag | unit (bare-remote fixture) | `git::tests::push_ref_lands_a_branch_on_the_remote` | ✅ green |
| A11 | 26-03 | 999.25 | `push_ref` lands a tag on the remote via the same primitive | unit (bare-remote fixture) | `git::tests::push_ref_lands_a_tag_on_the_remote` | ✅ green |
| A12 | 26-03 | 999.25 | A non-fast-forward push is rejected and the remote ref is byte-identical before and after (the observable proof a force implementation could not satisfy) | unit (bare-remote fixture) | `git::tests::push_ref_refuses_a_non_fast_forward_and_leaves_the_remote_unmoved` | ✅ green |
| A13 | 26-03 | 999.25 | `release_tag_state` → `Absent` when no tag exists | unit | `git::tests::release_tag_state_reports_absent_when_no_tag_exists` | ✅ green |
| A14 | 26-03 | 999.25 | A stray *lightweight* tag of the exact target name is **not** classified `Released` (RESEARCH.md Pitfall 2; this repo's real `v1.3.69`) | unit | `git::tests::release_tag_state_refuses_to_treat_a_lightweight_tag_as_released` | ✅ green |
| A15 | 26-03 | 999.25 | An unsigned annotated tag → `PresentUnverified`, with `git tag -v` stderr bounded via `sanitize_changelog_subject` (T-26-08) | unit | `git::tests::release_tag_state_reports_present_unverified_for_an_unsigned_annotated_tag` | ✅ green |
| A16 | 26-03 | 999.25 | A tag pointing at a different commit → `Mismatched` | unit | `git::tests::release_tag_state_reports_mismatched_when_the_tag_points_elsewhere` | ✅ green |
| A17 | 26-03 | 999.25 | A locally-valid tag absent from `origin` → `PresentUnverified`, not `Released` | unit (bare-remote fixture) | `git::tests::release_tag_state_reports_present_unverified_when_the_tag_is_not_on_origin` | ✅ green |
| A18 | 26-03 | 999.25 | Unset `devflow.releaseSigningKey` is a hard `Err` naming the config key, and creates no tag (a missing required argument, not a viability guess) | unit | `git::tests::create_signed_release_tag_names_the_missing_config_key` | ✅ green |
| A19 | 26-03 | 999.25 | `create_signed_release_tag` runs CONTRIBUTING.md step 5's exact `-c user.signingkey=… tag -s` form and produces a `git tag -v`-verifiable tag (throwaway repo-local SSH key) | unit (SSH-signing fixture) | `git::tests::create_signed_release_tag_produces_a_verifiable_annotated_tag` | ✅ green |
| A20 | 26-03 | 999.25 | Round trip: `create_signed_release_tag` → `push_ref` → `release_tag_state` reports `Released` | unit (both fixtures) | `git::tests::create_signed_release_tag_then_push_is_reported_as_released` | ✅ green |
| A21 | 26-03 | 999.25 | `check_ssh_signing_viability` gained no new caller (D-10 — report git's real result, never a viability prediction) | guard | `rg -c 'check_ssh_signing_viability' crates/ -g '*.rs'` → exactly `crates/devflow-core/src/git.rs:2` | ✅ green |
| A22 | pre-existing | 999.25 | Publish order still matches `publish_order`'s existing topo-sort — no recomputation introduced (D-04) | unit (existing) | `git::tests::publish_order_derives_core_before_cli_from_a_fixture_workspace` | ✅ green |

**Observed run (2026-07-29, `cargo test --workspace --features devflow-core/test-support --lib`):**
`changelog_sections` 6 passed · `sanitize_changelog_subject` 1 passed · `prepend_changelog` 3 passed ·
`changelog_append_writes_the_generated_body_end_to_end` 1 passed · `push_ref` 3 passed ·
`release_tag_state` 5 passed · `create_signed_release_tag` 3 passed · `publish_order_…` 1 passed —
**0 failed across all filters.** All 20 named tests confirmed present via `-- --list` before running
(guarding against trap 2 above).

---

## Per-Task Verification Map — Part B: Blocked on Missing Implementation

> These rows were seeded by plan-phase against the phase's full seven-plan arc
> (26-01 → 26-07). Only 26-01..26-03 were ever planned and executed, so the code
> each row targets **does not exist**. They are implementation gaps, already
> recorded as Truths 7–11 in `26-VERIFICATION.md` — *not* validation gaps.
>
> **They are deliberately not filled.** A test importing
> `devflow_core::sync::sync_main_to_develop` (or `cargo_publish`, or a
> `--yes-release` flag) would not compile, and a non-compiling test target fails
> the entire workspace build, `scripts/check.sh`, and the pre-push hook. Writing
> them would convert a clean red "not built" into a broken build. They unblock
> when their implementation lands, not before.

| # | Requirement | Behavior | Blocked By | Planned Command | Status |
|---|-------------|----------|------------|-----------------|--------|
| B1 | 999.52 | `devflow sync` refuses on dirty tree | `crates/devflow-core/src/sync.rs` does not exist | `sync::tests::refuses_on_dirty_tree` | 🚫 blocked |
| B2 | 999.52 | `devflow sync` refuses when not on `develop` | same | `sync::tests::refuses_off_develop` | 🚫 blocked |
| B3 | 999.52 | Short-circuits when `origin/main` is already an ancestor of `develop` | same | `sync::tests::noop_when_already_synced` | 🚫 blocked |
| B4 | 999.52 | Tree-identity mismatch aborts, leaves `develop` untouched, no push (D-09) | same | `sync::tests::aborts_on_tree_mismatch` | 🚫 blocked |
| B5 | 999.52 | Successful sync pushes to origin (D-08) | same | `sync::tests::pushes_on_success` | 🚫 blocked |
| B6 | 999.25 | Version-bump commit pushes to `develop` (D-01), fast-forward only, never `--force` | no release executor exists to call `push_ref` | `git::tests::version_bump_pushes_develop` | 🚫 blocked |
| B7 | 999.25 | Idempotent resume: `develop` already at/ahead of computed version → push step skipped (D-06) | same | `git::tests::skips_push_when_already_ahead` | 🚫 blocked |
| B8 | 999.25 | Tag step: existing annotated+reachable+pushed tag → step is a no-op (D-06) | executor's skip *decision* absent — the underlying `release_tag_state` → `Released` classification it would branch on **is** covered (A14–A17, A20) | `git::tests::skips_tag_when_already_released` | ⚠️ partial — predicate covered, decision blocked |
| B9 | 999.25 | Pre-publish check: `cargo info` exit 0 → already-published (skip); `could not find` → proceed; other → fail loud, do not guess | `classify_cargo_info_result`/`PublishCheck` never written (zero matches in `crates/`) | `git::tests::publish_check_classifies_exit_codes` | 🚫 blocked |
| B10 | 999.25 | `--yes-release` required per-invocation, never settable via config/env (D-03, mirrors `--yes-ship`) | flag does not exist — `rg 'yes_release' crates/` zero matches; `crates/devflow-cli/tests/release_execute.rs` absent | new `crates/devflow-cli/tests/release_execute.rs` | 🚫 blocked |
| B11 | 999.25 | Fail-fast, no rollback (D-05): mid-sequence failure (core publish succeeds, cli publish fails) leaves prior steps landed | no executor, no publish primitives | `release_execute::tests::partial_failure_leaves_prior_steps_landed` | 🚫 blocked |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky/partial · 🚫 blocked (no implementation to test)*

---

## Wave 0 Requirements

- [x] Bare-remote git fixture helper — **DONE.** `init_bare_remote` in `git.rs`'s test module, the project's first hermetic local-bare-remote helper; reused by all 11 tests 26-03 added (A10–A12, A17, A20). Also `configure_ssh_tag_signing` (throwaway repo-local SSH keypair + allowed-signers) and `rev_parse`, neither of which the original Wave 0 list anticipated.
- [ ] `crates/devflow-core/src/sync.rs` — new module, not created (blocks B1–B5)
- [ ] `crates/devflow-cli/tests/release_execute.rs` — new integration test file, not created (blocks B10–B11)
- [ ] A fake/stub `cargo` shim for hermetic classification of `cargo info` exit codes, OR tests against a known-immutable registry version — never chosen or built (blocks B9). The hermeticity-vs-real-tool tradeoff the original note asked the planner to resolve is **still open** and must be decided by whichever plan implements the publish primitives.

---

## Manual-Only Verifications

> ⚠️ All three remain **moot as of this audit** — the code paths they would
> exercise (`create_signed_release_tag`'s production caller, `cargo_publish`,
> `sync_main_to_develop`) were never built. These are not "verified by hand
> instead of by test"; they are pending, and become actionable only once the
> corresponding executor code lands.

| Behavior | Requirement | Why Manual | Test Instructions | Status |
|----------|-------------|------------|-------------------|--------|
| Signed tag creation with the real operator signing key produces a `git tag -v`-verifiable tag | 999.25 (D-10) | No sandbox in research/CI has real signing keys; the exact `git -c user.signingkey=... tag -s` invocation must be observed against a real key at least once | Operator runs `--yes-release` against a throwaway/non-production repo (or the real repo, operator's call) with their own `devflow.releaseSigningKey` configured, confirms `git tag -v vX.Y.Z` passes | ⬜ pending — no `--yes-release` flag exists yet. Partially de-risked: A19 proves the exact invocation form works against a throwaway SSH key. |
| `cargo publish` for both crates completes and the packages are live in the correct order | 999.25 (D-04) | Publish credentials and live registry state cannot be faked without risking a real, irreversible publish | Operator observes the first real `--yes-release` run's publish steps end-to-end; verify `devflow-core` lands before `devflow` per `publish_order` | ⬜ pending — no publish code exists to observe |
| Sync (`devflow sync`) direct-pushes `develop` without a human clicking a GitHub merge-strategy button | 999.52 (D-08) | Requires the operator's own out-of-scope GitHub ruleset bypass to already exist; cannot be simulated against the real `origin` in CI | Operator confirms the ruleset bypass is configured, then runs `devflow sync` once against the real repo and confirms the push lands as a direct push, not a PR | ⬜ pending — no `devflow sync` subcommand exists |
| Operator authorization for direct-push-to-develop (D-01/D-08) and unattended `cargo publish` (D-04) | 999.25 | Categorically a human decision about an irreversible-in-effect capability; no test can substitute for the operator's actual authorization | Recorded in `26-01-SUMMARY.md` Decisions 1 & 2 | ✅ obtained — `direct-push` and `automate-publish`, double-confirmed (original discuss-phase session + live re-confirmation this run) |

---

## Validation Audit 2026-07-29

| Metric | Count |
|--------|-------|
| Map rows audited (original seed) | 15 |
| Delivered behaviors with green automated coverage | 22 (Part A) |
| Gaps found | 11 |
| Resolved (tests generated) | 0 |
| Escalated / blocked on missing implementation | 11 (Part B) |
| Auditor spawned | No — see rationale |
| Tests re-run live (not trusted from self-report) | 20 named + 3 incidental, 0 failed |

**Why no tests were generated.** Zero of the 11 gaps are fillable. Every behavior
this phase actually implemented (26-02's changelog generation, 26-03's three git
primitives) already has green automated coverage — re-confirmed live above, not
taken on the executing agents' word. The 11 open rows all target code that does
not exist (`sync.rs`, the publish primitives, `--yes-release`, the executor
itself). `gsd-nyquist-auditor` is constrained to never modify implementation
files, so it could not close any of them; a test it wrote against a nonexistent
module would fail to *compile*, and one non-compiling test target fails the whole
workspace build, `scripts/check.sh`, and the pre-push hook. Spawning it would
have traded an honest red for a broken build.

**Resulting state: `status: validated`, `nyquist_compliant: false` — i.e. PARTIAL
per audit-milestone §5.5.** This is the accurate representation: the validation
contract has been audited and is complete for what shipped, and it is explicit
that 999.25 and 999.52 lack automated verification because they lack
implementation. Setting `nyquist_compliant: true` on the grounds that "nothing
was fillable" would repeat exactly the overclaim pattern `26-VERIFICATION.md`
flagged in `26-01-SUMMARY.md` and `26-03-SUMMARY.md`
(`requirements-completed: ["999.25", "999.52"]` against `files_modified: []`).

**Unblocking path.** Not a fix-up pass. `26-VERIFICATION.md` recommends
`/gsd-plan-phase 26 --gaps` to plan the remaining 26-04..26-07 arc (sync module,
publish primitives, and the executor that composes the primitives 26-03 already
built and left orphaned). Part B's rows are ready-made acceptance criteria for
those plans — each already names its behavior and intended test filter.

---

## Validation Sign-Off

- [x] All *delivered* tasks have `<automated>` verify — 22/22 green (Part A)
- [x] Sampling continuity: no 3 consecutive delivered tasks without automated verify
- [ ] Wave 0 covers all MISSING references — 1 of 4 items built (`init_bare_remote`); 3 still open
- [x] No watch-mode flags
- [x] Feedback latency < 180s — every filter used completes in under 2s
- [ ] `nyquist_compliant: true` — **deliberately false.** 999.5 is fully covered; 999.25 and 999.52 have no automated verification because they have no implementation (11 rows in Part B).

**Approval:** validated — PARTIAL. Phase 26 is Nyquist-complete for what it
shipped (999.5) and honestly red for what it did not (999.25, 999.52).
