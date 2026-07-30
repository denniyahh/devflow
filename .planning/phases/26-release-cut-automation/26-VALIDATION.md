---
phase: 26
slug: release-cut-automation
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-07-29
validated: 2026-07-30
# Third audit re-verified against the post-/gsd-audit-fix HEAD; the second
# audit (committed 8dc092e 23:08) predated fixes e4a3236/43a7a96/8f5f2d1/7bd9a37.
audits: 3
gaps_filled_by_audit: 3
---

# Phase 26 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Derived from `26-RESEARCH.md` § Validation Architecture.
> Audited three times — see § Validation Audit entries below:
> 2026-07-29 (mid-arc, PARTIAL), 2026-07-30 after the full 26-01..26-07 arc
> landed, and 2026-07-30 again after `/gsd-audit-fix` changed source
> **later the same evening than the second audit was written** (that audit was
> committed at `8dc092e` 23:08; commits `e4a3236`/`43a7a96`/`8f5f2d1`/`7bd9a37`
> landed 23:43–23:55). Every "green" below is re-observed against the
> post-fix HEAD, not carried forward.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` harness via `cargo test` (workspace) — no separate framework |
| **Config file** | none — `Cargo.toml`'s `[workspace]`/per-crate `[dev-dependencies]` is the only config |
| **Quick run command** | `cargo test --workspace --features devflow-core/test-support --lib <filter>` |
| **Full suite command** | `devflow test` / `scripts/check.sh all` / `scripts/check-in-container.sh all` |
| **Test count (lib target)** | **436** in `devflow-core` lib (406 pre-phase + 11 by 26-03 + 5 by 26-04 + 4 by 26-05 + 8 by 26-06 + 2 by the audit-fix pass) |
| **CLI integration target** | `crates/devflow-cli/tests/release_execute.rs` — **8** tests (6 by 26-07 + 2 by this audit), run via `cargo test -p devflow --test release_execute` |
| **CLI pre-gate target** | `crates/devflow-cli/tests/release_check.rs` — **11** tests (10 pre-existing + 1 by this audit), run via `cargo test -p devflow --test release_check` |
| **Estimated runtime** | ~180 seconds full suite; every filter used in this phase's map completes in under 2 seconds |

**Three invocation traps confirmed live during this phase — all recorded here so the commands in the map below are copy-pasteable:**

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
3. **`doc_check` is a module in the lib target, not a test target.** Row C14 cites
   `doc_check::` tests; `cargo test -p devflow --test doc_check` matches no target and
   prints only the target list — a silent no-op of the same family as trap 2. Run them
   with `cargo test --workspace --features devflow-core/test-support --lib -- 'doc_check::'`
   (6 passed). Confirmed live during the 2026-07-30 post-fix audit.

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

## Per-Task Verification Map — Part B: Previously Blocked, Now Delivered

> These 11 rows were seeded by plan-phase against the phase's full seven-plan
> arc and were **🚫 blocked** at the 2026-07-29 mid-arc audit, because only
> 26-01..26-03 had executed and the code each row targets did not exist. Plans
> 26-04 (sync module), 26-05 (publish primitives), 26-06 (release executor) and
> 26-07 (CLI surface) have since landed. **All 11 are now green**, each re-run
> live during the 2026-07-30 re-audit — not taken from the executing agents'
> self-reported `status: pass`.
>
> **Module relocation (B6–B8, B11):** the seeded rows named `git::tests::…`
> because no executor module was foreseen at seed time. The behaviors landed in
> the new `devflow_core::release` module instead, so their real test paths are
> `release::tests::…`. Same behaviors, same assertions — recorded here so the
> seeded name doesn't read as a missing test.

| # | Requirement | Behavior | Test Type | Automated Command (filter) | Status |
|---|-------------|----------|-----------|----------------------------|--------|
| B1 | 999.52 | `devflow sync` refuses on dirty tree, before the fetch, mutating nothing | unit (bare-remote fixture) | `sync::tests::refuses_on_dirty_tree` | ✅ green |
| B2 | 999.52 | `devflow sync` refuses when not on `develop` | unit (bare-remote fixture) | `sync::tests::refuses_off_develop` | ✅ green |
| B3 | 999.52 | Short-circuits when `origin/main` is already an ancestor of `develop` | unit (bare-remote fixture) | `sync::tests::noop_when_already_synced` | ✅ green |
| B4 | 999.52 | Tree-identity mismatch aborts, leaves `develop` untouched, no push — the remote ref proven byte-identical before and after (D-09) | unit (bare-remote fixture) | `sync::tests::aborts_on_tree_mismatch` | ✅ green |
| B5 | 999.52 | Successful sync pushes to origin via `push_ref`, no bare `git push` argv anywhere in `sync.rs` (D-08) | unit (bare-remote fixture) | `sync::tests::pushes_on_success` | ✅ green |
| B6 | 999.25 | Version-bump commit pushes to `develop` (D-01), fast-forward only, never `--force` | unit (bare-remote fixture) | `release::tests::version_bump_pushes_develop` *(seeded as `git::tests::`)* | ✅ green |
| B7 | 999.25 | Idempotent resume: `develop` already at/ahead of computed version → push step skipped, write and push as two independent predicates (D-06) | unit (bare-remote fixture) | `release::tests::skips_push_when_already_ahead` *(seeded as `git::tests::`)* | ✅ green |
| B8 | 999.25 | Tag step: existing annotated+reachable+pushed tag → step is a byte-identical no-op (D-06). The blocked *decision* is now covered, not just A14–A17's predicate | unit (bare-remote + SSH-signing fixtures) | `release::tests::skips_tag_when_already_released` *(seeded as `git::tests::`)* | ✅ green |
| B9 | 999.25 | Pre-publish check: `cargo info` exit 0 → already-published; **both** `could not find` *and* `registry` fragments → proceed; anything else (incl. missing-manifest, absent exit code, empty stderr) → `Ambiguous`, fail loud, never guess | unit (pure classifier, 6 documented cases) | `git::tests::publish_check_classifies_exit_codes` | ✅ green |
| B10 | 999.25 | `--yes-release` required per-invocation, never settable via `devflow.toml`, env var, or persisted `State` (D-03, mirrors `--yes-ship`) | integration (real binary, isolated HOME) | `release_execute::execute_without_yes_release_is_rejected` + `release_execute::yes_release_is_not_settable_via_config_or_env` | ✅ green |
| B11 | 999.25 | Fail-fast, no rollback (D-05): mid-sequence failure leaves prior steps landed — version-bump commit stays pushed, no tag exists, no compensating action | unit (bare-remote fixture) | `release::tests::partial_failure_leaves_prior_steps_landed` *(seeded as `release_execute::tests::`)* | ✅ green |

---

## Per-Task Verification Map — Part C: Additional Delivered Behavior (26-04 … 26-07)

> Behaviors the 26-04..26-07 plans delivered that had no seeded Part B row.
> Included so the map covers what actually shipped, not only what was foreseen.

| # | Plan | Requirement | Behavior | Test Type | Automated Command (filter) | Status |
|---|------|-------------|----------|-----------|----------------------------|--------|
| C1 | 26-05 | 999.25 | `cargo info` argv is pinned to the exact `info <name>@<version> --registry crates-io` form without spawning anything | unit | `git::tests::cargo_info_args_targets_the_exact_version_on_crates_io` | ✅ green |
| C2 | 26-05 | 999.25 | An `Ambiguous` verdict surfaces as `Err`, structurally incapable of degrading into either boolean | unit | `git::tests::crate_already_published_surfaces_an_ambiguous_check_as_an_error` | ✅ green |
| C3 | 26-05 | 999.25 | `cargo_publish` reports failure without publishing anything; no `--dry-run`, no retry loop (D-04/D-05) | unit (no-manifest fixture — structurally cannot reach the registry) | `git::tests::cargo_publish_reports_a_failure_without_publishing_anything` | ✅ green |
| C4 | 26-06 | 999.25 | The develop→main human gate is **content-based** (reads `origin/main`'s version-file text via `git show`, never ancestry, because `main` squash-merges) and halts cleanly creating no tag (D-02) | unit (bare-remote fixture) | `release::tests::halts_at_the_human_gate_when_main_does_not_declare_the_release` | ✅ green |
| C5 | 26-06 | 999.25 | A stray lightweight or mismatched tag is a terminal `TagCollision`, never auto-resolved; the existing tag is provably untouched (D-05) | unit (bare-remote fixture) | `release::tests::refuses_a_stray_lightweight_tag_rather_than_skipping` | ✅ green |
| C6 | 26-06 | 999.52 | The executor's sync step calls the identical `sync_main_to_develop` the standalone CLI calls (D-07); a refused sync stops the run before any publish is attempted | unit (bare-remote fixture) | `release::tests::a_refused_sync_stops_the_run_before_publishing` | ✅ green |
| C7 | 26-06 | 999.25 | Full five-step sequence completes in one call, one `StepReport` per `ReleaseStep` in sequence order, `publish_order`'s packages consulted in their own order and never re-sorted (D-04) | unit (bare-remote + SSH-signing fixtures) | `release::tests::completes_the_sequence_and_reports_every_step` | ✅ green |
| C8 | 26-06 | 999.25 | Truth 9 closure: `push_ref`, `release_tag_state`, `create_signed_release_tag` each have a real production (non-test) caller | guard | `rg -n 'push_ref\|release_tag_state\|create_signed_release_tag' crates/devflow-core/src/release.rs` → production matches at **445/528/540–542/562–563**, all before the `#[cfg(test)]` boundary at line **716** | ✅ green |
| C9 | 26-07 | 999.25 | `--execute --yes-release` reaches the real core executor and surfaces its refusal; Truth 7 closure | integration (real binary) | `release_execute::execute_reaches_the_core_executor_and_refuses_off_develop` | ✅ green |
| C10 | 26-07 | 999.25 | `--check` and `--execute` are mutually exclusive | integration (real binary) | `release_execute::check_and_execute_together_are_rejected` | ✅ green |
| C11 | 26-07 | 999.25 | Bare `devflow release` names both modes; the old "not yet built"/`DEN-50` deferral phrasing is **absent** (asserted as absence, not merely new-text presence) | integration (real binary) | `release_execute::bare_release_names_both_modes_and_no_deferred_executor` | ✅ green |
| C12 | 26-07 | 999.25 | `yes_release` has no config/state/env source surface at all | guard (source-surface) | `release_execute::yes_release_has_no_config_state_or_env_surface` | ✅ green |
| C13 | 26-07 | 999.25 | The phase adds no pull-request capability — `gh` subprocess call sites unchanged at one file (D-02) | guard | `rg -n 'Command::new("gh")' crates/ -g '*.rs'` → exactly `crates/devflow-cli/src/preflight.rs` | ✅ green |
| C14 | 26-04/26-07 | 999.52 | `devflow sync` and the new release flags are real, documented, `--help`-able subcommands — help snapshot and doc invariants hold | integration | `help_snapshot::help_output_matches_committed_snapshot` + all 6 `doc_check::` tests | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky/partial · 🚫 blocked (no implementation to test)*

**Observed run (2026-07-30, second audit — pre-audit-fix HEAD `8dc092e`):**
`sync::tests::` **5 passed** · `release::tests::` **8 passed** · publish primitives (B9/C1–C3) **4 passed** ·
`cargo test -p devflow --test release_execute` **6 passed** · Part A regression filter **23 passed** ·
full workspace `--lib` **434 passed / 0 failed** · `cargo test -p devflow` **0 failed across all 15 targets**.

**Re-observed (2026-07-30, third audit — post-audit-fix HEAD, the authoritative counts):**
All 39 named lib tests in Parts A/B/C/D confirmed present via `-- --list` first (**0 missing**), then
run: `changelog_sections` 6 · `sanitize_changelog_subject` 1 · `prepend_changelog` 3 ·
`changelog_append_writes_the_generated_body_end_to_end` 1 · `push_ref` 3 · `release_tag_state` 5 ·
`create_signed_release_tag` 3 · `publish_order` 2 · `workspace_member_paths` 2 ·
`publish_check_classifies_exit_codes` 1 · `cargo_info_args` 1 · `cargo_publish` 1 ·
`crate_already_published` 1 · `doc_check::` 6 · `sync::tests::` **5** · `release::tests::` **9** (was 8;
+1 from C-01) — **0 failed in every filter**. Full workspace `--lib` **436 passed / 0 failed** ·
`--test release_execute` **8 passed** · `--test release_check` **11 passed** · `--test help_snapshot`
**1 passed**. `cargo fmt --check` clean · `cargo clippy --workspace --all-targets -- -D warnings` clean.

Guard greps re-run at post-fix HEAD: `Released phase via DevFlow` absent (exit 1) ·
`check_ssh_signing_viability` still exactly `crates/devflow-core/src/git.rs:2` · `gh` call sites still
one file (`preflight.rs:617`) · zero `"push"` in `sync.rs` (exit 1) · `DEN-50`/`not yet built` present
only inside the assertions proving their absence.

**One prior guard claim corrected.** The second audit recorded "zero `--dry-run` in `git.rs`". That is
now inaccurate as literally written: `git.rs:1029` contains the string inside a doc comment that
*states the flag is never passed* ("Never passes `--dry-run`: this primitive exists to actually
publish"). No argv passes it. The D-04 guard therefore holds in substance, and the correct grep is
`rg -n -- '--dry-run' crates/devflow-core/src/git.rs` → exactly one doc-comment match, zero in any
`.args(...)` call.

---

## Per-Task Verification Map — Part D: Audit-Fix Pass Behavior (post-`8dc092e`)

> `/gsd-audit-fix 26` changed source **after** the second audit's VALIDATION.md
> was committed, so none of these behaviors existed when Parts A–C were
> written. This is why a third audit was required rather than a re-read: an
> mtime/status check would have reported the file current.
>
> Three of the six behaviors arrived with their own test (C-05, C-01, and the
> core half of C-03/C-04). The other three — every operator-facing string the
> fixes added to `crates/devflow-cli/src/commands.rs` — had **zero** coverage
> and were the gaps this audit filled (D4–D6). That asymmetry mattered:
> C-03's and C-04's whole purpose is what the *operator is told* about an
> irreversible sequence, so covering only the core-side data structure while
> leaving the rendering unverified would have left the actual fix untested.

| # | Finding | Behavior | Test Type | Automated Command (filter) | Status |
|---|---------|----------|-----------|----------------------------|--------|
| D1 | C-05 | `members_key_offset` matches the exact `members` key, so a `default-members` key can no longer silently truncate the publish set — asserted for both key orderings and the only-`default-members` case | unit | `git::tests::workspace_member_paths_ignores_default_members` | ✅ green |
| D2 | C-01 | A stray *unreachable* semver tag is refused (`StrayBaselineTag`) before any mutation instead of being adopted as the release version; the remote `develop` ref and the version file are proven byte-identical across the refusal. Also pins **WR-01/W-18** — exactly one ledger entry per step, in sequence order (the duplicate `VersionBump` was reproduced live by this assertion before the fix) | unit (bare-remote fixture) | `release::tests::refuses_a_stray_unreachable_tag_instead_of_adopting_its_version` | ✅ green |
| D3 | C-03 (core) | `execute_release` returns `ReleaseFailure { error, steps }`, so a failure carries the ledger of what already landed: the pushed `VersionBump` is reported `Completed`, and the step that failed reports nothing | unit (bare-remote fixture) — assertions added to two existing tests, not new ones | `release::tests::partial_failure_leaves_prior_steps_landed` (release.rs:1050) + `release::tests::a_refused_sync_stops_the_run_before_publishing` (release.rs:1310) | ✅ green |
| D4 | **C-03 (CLI)** | The failure path prints the landed-step ledger **and** the advisory "the steps above already landed and are NOT rolled back" — the operator's only record of irreversible mutations (D-05). Was unverified: no test referenced the string | integration (real binary, bare-remote fixture, isolated HOME) | `cargo test -p devflow --test release_execute execute_failure_reports_the_steps_that_already_landed` | ✅ green — **filled by this audit** |
| D5 | **C-04 (CLI)** | A release that tags and pushes but publishes nothing renders "NOTHING was published", and the unqualified "release cut complete" is asserted **absent** — the false-green guard on the one irreversible step | integration (real binary; bare remote, throwaway `ssh-keygen` ed25519 signing key, real verifiable signed tag, empty `publish_order`) | `cargo test -p devflow --test release_execute a_release_that_publishes_nothing_is_never_reported_as_complete` | ✅ green — **filled by this audit** |
| D6 | **C-04 (pre-gate)** | An empty publish order **warns rather than fails** `--check` (single-crate and non-Rust projects legitimately publish nothing) and its detail reads "no packages resolved — nothing would be published by a release", never as "checked, fine" | integration (real binary) | `cargo test -p devflow --test release_check release_check_states_when_the_publish_order_resolves_nothing` | ✅ green — **filled by this audit** |

**Why D4–D6 cannot be false greens.** Each asserts a string introduced by the
fix commit that created it — `NOT rolled back` (`commands.rs:2359`),
`NOTHING was published` (`commands.rs:2331`), `no packages resolved — nothing
would be published by a release` (`commands.rs:2492`). `rg` confirms none of
the three exists anywhere in the pre-fix tree, so all three tests are
necessarily red against the code they were written to pin, without needing a
revert to demonstrate it. Each drives the freshly-built binary via
`env!("CARGO_BIN_EXE_devflow")` under an isolated `HOME` with
`SSH_AUTH_SOCK`/`SSH_AGENT_PID` removed, so no result can come from a stale or
ambient install. All three were re-run 3× consecutively with identical counts —
no flakiness, and none is `#[ignore]`d, cfg'd out, or env-gated.

**Known assertion boundary in D5 (recorded, not a gap).** D5 asserts the
outcome *wording* and exit status; it does not separately assert the tag object
landed on the bare remote. It does not need to —
`ReleaseOutcome::CompletedWithoutPublish` is only produced at the end of the
full sequence (`release.rs:658`), so reaching it entails the tag step ran, and
`release::tests::completes_the_sequence_and_reports_every_step` (C7) already
asserts every step report and the tag's verifiability at core level.

---

## Wave 0 Requirements

- [x] Bare-remote git fixture helper — **DONE.** `init_bare_remote` in `git.rs`'s test module, the project's first hermetic local-bare-remote helper; reused by all 11 tests 26-03 added (A10–A12, A17, A20). Also `configure_ssh_tag_signing` (throwaway repo-local SSH keypair + allowed-signers) and `rev_parse`, neither of which the original Wave 0 list anticipated.
- [x] `crates/devflow-core/src/sync.rs` — **DONE** (26-04). Also became the shared `pub(crate)` fixture home (`init_repo`, `init_bare_remote`) that 26-06 imports rather than building a third copy. Unblocked B1–B5.
- [x] `crates/devflow-cli/tests/release_execute.rs` — **DONE** (26-07). 6 tests driving the real binary under an isolated `HOME`. Unblocked B10.
- [x] The hermeticity-vs-real-tool tradeoff for `cargo info` — **RESOLVED** (26-05): *neither* option was taken. No `PATH` shim (mutates process-global state and only covers `Command::spawn`) and no live-registry test (breaks offline/CI runs). Instead all branch logic lives in the pure `classify_cargo_info_result(exit_code, stderr)`, table-tested against 6 captured fixtures; the live tool's real behavior was re-confirmed once out-of-band via an oracle probe (`cargo info devflow-core@2.1.0` exit 0; `@0.0.1` exit 101). Unblocked B9.

**Wave 0 complete — 4 of 4 items built or explicitly resolved.**

---

## Manual-Only Verifications

> **No longer moot.** At the 2026-07-29 audit all three were pending on *missing
> code*. That code now exists and is green, so these are back to being what the
> section is actually for: **backstop truths that no test can close**, each
> requiring a real signing key, real registry credentials, or a real `origin`
> with the operator's own ruleset bypass. They are persisted as the three items
> in `26-UAT.md` (`status: testing`, 3 pending) and as `human_verification`
> entries in `26-VERIFICATION.md`. A verifier must **abstain** on these, not
> pass or fail them.
>
> These are *not* Nyquist gaps. Every behavior that is automatable has automated
> coverage; these three are categorically operator-only irreversible operations.

| Behavior | Requirement | Why Manual | Test Instructions | Status |
|----------|-------------|------------|-------------------|--------|
| Signed tag creation with the real operator signing key produces a `git tag -v`-verifiable tag | 999.25 (D-10) | No sandbox in research/CI has the operator's real (non-throwaway) signing key | Operator runs `devflow release --execute --yes-release` against a real repo with their own `devflow.releaseSigningKey` configured, confirms `git tag -v vX.Y.Z` passes | ⬜ pending operator — **UAT #1**. Reachable now (`--yes-release` exists). De-risked: A19 + `release::tests::skips_tag_when_already_released`/`completes_the_sequence_and_reports_every_step` prove the exact invocation form works against a throwaway SSH key. |
| `cargo publish` for both crates completes and the packages are live in the correct order | 999.25 (D-04) | Publish credentials and live registry state cannot be faked without risking a real, irreversible publish | Operator observes the first real `--yes-release` run's publish steps end-to-end; verify `devflow-core` lands before `devflow` per `publish_order` | ⬜ pending operator — **UAT #2**. Reachable now. De-risked: C3 proves the failure path; B9/C1–C2 pin the pre-publish classification; A22 pins the order. |
| Sync (`devflow sync`) direct-pushes `develop` without a human clicking a GitHub merge-strategy button | 999.52 (D-08) | Requires the operator's own out-of-scope GitHub ruleset bypass to already exist; cannot be simulated against the real `origin` in CI | Operator confirms the ruleset bypass is configured, then runs `devflow sync` once against the real repo and confirms the push lands as a direct push, not a PR | ⬜ pending operator — **UAT #3**. Reachable now (`devflow sync` exists). De-risked: B1–B5 prove the direct-push behavior against a local bare remote. |
| Operator authorization for direct-push-to-develop (D-01/D-08) and unattended `cargo publish` (D-04) | 999.25 | Categorically a human decision about an irreversible-in-effect capability; no test can substitute for the operator's actual authorization | Recorded in `26-01-SUMMARY.md` Decisions 1 & 2 | ✅ obtained — `direct-push` and `automate-publish`, double-confirmed (original discuss-phase session + live re-confirmation this run) |

**Coverage boundary made explicit by the C-04 fix (2026-07-30).**
`ReleaseOutcome::Completed` — the "crates were actually published" outcome — is
produced at exactly one place (`release.rs:685`) and is observed by **no test**:
`rg 'ReleaseOutcome::Completed\b'` returns only that production site, its doc
comment, and the CLI match arm. That is correct by construction, not an
oversight — reaching it requires a real `cargo publish` against a live
registry, which is precisely **UAT #2** above. The C-04 fix is what made this
visible: before it, the empty-publish fixture reported `Completed`, so the
full-sequence test appeared to cover the published path while publishing
nothing. It now asserts `CompletedWithoutPublish` (D5, C7), and `Completed`'s
CLI arm is honestly recorded here as operator-only rather than counted as
automated coverage.

**W-17 precondition warning (carried from `26-REVIEW.md`, not a validation
gap).** The review recorded that the live `develop` ruleset is
`enforcement: active` with an empty bypass list, so step 1's direct push cannot
land against this repository as configured. All three UAT items above are
gated on a precondition the review found *absent*. That is a
repository-settings action for the operator, not a code or test change, but it
means UAT #1–#3 are currently unreachable in practice — they should not be
read as "merely awaiting a spare afternoon".

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

## Validation Audit 2026-07-30 (re-audit, full arc)

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 (all 11 prior gaps closed by implementation, not by this audit) |
| Escalated | 0 |
| Prior audit's blocked rows (B1–B11) now green | 11 / 11 |
| Additional delivered behaviors mapped (Part C) | 14 |
| Total automated rows in map | 47 (22 Part A + 11 Part B + 14 Part C) |
| Manual-only backstop truths (operator-pending) | 3 — persisted as `26-UAT.md` #1–#3 |
| Auditor spawned | No — see rationale |
| Tests re-run live (not trusted from self-report) | 46 named across 4 filters, plus full suite 434 `--lib` + all `-p devflow` targets, **0 failed** |

**Why no auditor was spawned.** There were no gaps to fill. Between the
2026-07-29 audit and this one, plans 26-04 (`sync.rs`), 26-05 (publish
primitives), 26-06 (`release.rs` executor) and 26-07 (`--execute --yes-release`
CLI surface) landed, and each brought its own tests for the exact rows the prior
audit had recorded as blocked. `gsd-nyquist-auditor` fills *missing* coverage;
every automatable behavior in this phase already has green coverage, so
spawning it would have had nothing to write. This audit's work was therefore
**verification, not generation**: every named test was confirmed present via
`-- --list` and then re-run live, rather than trusted from the executing agents'
self-reported `status: pass`.

**Why `nyquist_compliant: true` is honest here.** The prior audit set it `false`
and warned that flipping it "on the grounds that nothing was fillable" would be
an overclaim. That warning does not apply now, and the distinction matters: then,
11 requirements had **no automated verification because they had no
implementation**; now they have both, re-run green in this session. The three
remaining manual-only rows are not un-run tests — they are irreversible
real-world operations (a real signing key, a real `cargo publish`, a real
`origin` push) that no test can perform by design, tracked separately as UAT
items and as `26-VERIFICATION.md`'s `human_verification` block. Corroborated
independently: `26-VERIFICATION.md` reports `11/11 must-haves verified`,
`behavior_unverified: 0`, `gaps_remaining: []`.

---

## Validation Audit 2026-07-30 (third audit, post-`/gsd-audit-fix`)

| Metric | Count |
|--------|-------|
| Gaps found | **3** |
| Resolved (tests generated) | **3** |
| Escalated | 0 |
| Prior map rows re-verified live at post-fix HEAD | 47 (Parts A+B+C) — all still green, 0 regressions |
| New rows added (Part D) | 6 (3 already covered by the fix commits, 3 filled here) |
| Total automated rows in map | **53** (22 A + 11 B + 14 C + 6 D) |
| Manual-only backstop truths (operator-pending) | 3 — unchanged, `26-UAT.md` #1–#3 |
| Auditor spawned | **Yes** — `gsd-nyquist-auditor`, 3/3 gaps filled, 0 debug iterations |
| Stale citations corrected | 2 (C8's line numbers; the `--dry-run` guard's wording) |

**Why a third audit was necessary.** The second audit's VALIDATION.md was
committed at `8dc092e` (23:08). `/gsd-audit-fix 26` then changed source at
23:43–23:55 (`e4a3236` C-05, `43a7a96` C-03, `8f5f2d1` C-04, `7bd9a37` C-01)
plus `0f0e17a` (C-07, README). Every "✅ green" in Parts A–C was therefore
observed against code that no longer existed, and three behaviors had been
added with no coverage at all. This is the mtime-staleness failure mode
directly: the file was newer than most of the phase but older than the fixes
that mattered most.

**The three gaps, and why they were real.** All three were the operator-facing
half of fixes whose core half *was* tested — `commands.rs`'s failure-path step
ledger and "NOT rolled back" advisory (C-03), the `CompletedWithoutPublish`
rendering that replaces a false "release cut complete" (C-04), and the
pre-gate's empty-publish-order detail (C-04). For an irreversible sequence with
no rollback (D-05), what the operator is *told* is the fix; a covered
`ReleaseFailure` struct with an unverified renderer is not a covered fix.

**Step 4 gate — auto-selected, recorded.** The workflow's gap-plan gate calls
`AskUserQuestion`, which has no channel to receive an operator answer under
DevFlow's one-shot `claude -p` launch model (the same structural limitation
filed as 999.57/DEN-82 for `checkpoint:decision`). Option 1 ("Fix all gaps") —
the workflow's primary path and the only one that produces the artifact this
step exists to produce — was auto-selected rather than blocking the run. The
alternative options were "skip, mark manual-only" and "cancel", neither of
which was defensible for three fillable gaps on the irreversible path.

**Verification was independent of the auditor's self-report.** The auditor
returned `## GAPS FILLED` claiming 3/3 green on first attempt. That claim was
not taken at face value: its two commits were checked to touch **only** the two
test files (`git show --stat`; no `src/` file modified, working tree clean),
each new test was confirmed present via `-- --list`, the assertion bodies were
read to confirm they pin the fixed behavior rather than passing vacuously, the
suites were re-run by the orchestrator, and the two new `release_execute` tests
were run 3× consecutively for flakiness. All counts matched the auditor's
report. The build-breakage risk that justified *not* spawning an auditor at the
first audit (a non-compiling test target fails the whole workspace, `check.sh`,
and the pre-push hook) did not materialize — `cargo fmt --check` and
`clippy --workspace --all-targets -D warnings` are both clean.

---

## Validation Sign-Off

- [x] All delivered tasks have `<automated>` verify — **53/53 green** (Parts A + B + C + D)
- [x] Sampling continuity: no 3 consecutive delivered tasks without automated verify
- [x] Wave 0 covers all MISSING references — 4 of 4 built or explicitly resolved
- [x] No watch-mode flags; nothing `#[ignore]`d, cfg'd out, or env-gated
- [x] Feedback latency < 180s — every filter used completes in under 5s; full `--lib` suite 4.1s
- [x] `nyquist_compliant: true` — 999.5, 999.25 and 999.52 all have green automated verification
- [x] Post-fix HEAD verified, not the HEAD this file was first written against
- [x] Full gate green at sign-off: 436 `--lib` / 8 `release_execute` / 11 `release_check` / 1 `help_snapshot`, 0 failed; `fmt --check` and `clippy -D warnings` clean

**Approval:** validated — **NYQUIST-COMPLIANT**. Every automatable behavior
Phase 26 delivered has green automated coverage, re-run live at the post-fix
HEAD during this audit, including the three operator-facing behaviors the
audit-fix pass added and this audit's tests now pin (D4–D6).

Three operator-only backstop truths (real signing key, real `cargo publish`,
real `origin` direct-push) remain open by design and are tracked in `26-UAT.md`,
not as validation gaps — with the caveat now recorded above that `26-REVIEW.md`
found their shared precondition (a `develop` ruleset bypass) **absent** on the
live repository, so they are blocked rather than merely pending.

**Nyquist compliance is not a ship recommendation.** `26-REVIEW.md`'s standing
verdict is *do not ship*: C-02 (unresumable publish) and C-06 (`project_root`
retargeting to an ancestor checkout) were classified manual-only and remain
unfixed, C-02 because the recommended persisted step ledger contradicts recorded
decision D-06 and needs the operator to re-open it. This section certifies that
what the phase built is automatically verified; it makes no claim that what it
built is ready to cut a release with.
