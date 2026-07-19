---
phase: 17-pipeline-dogfood-followup
verified: 2026-07-19T12:05:00Z
status: passed
score: 12/12 must-haves verified
behavior_unverified: 0
overrides_applied: 1
overrides:
  - must_have: "A non-interactive plan, unavailable reviewer, missing security artifact, or invalid required credential is reported before the stage launch (retrospective AC-4)"
    reason: "D-14(b) named no concrete security-artifact path/key at planning time; cross-AI review consensus #6 deferred it and reviewer-set enforcement to Phase 18's Hermes adapter, which is the first adapter with real reviewer storage"
    accepted_by: "Dennis Kim (gap-closure disposition, /gsd-plan-phase --gaps, phase 17)"
    accepted_at: "2026-07-19T11:26:28Z"
re_verification:
  previous_status: gaps_found
  previous_score: 9/12
  gaps_closed:
    - "Infrastructure-outcome ceiling (state.infra_failures) bounds a stuck loop, not the phase's lifetime (CR-01) — transition() now resets state.infra_failures = 0 alongside state.consecutive_failures = 0 (main.rs:1641), proven in-memory and persisted by new regression test transition_resets_infra_failures, plus a subsequent-fault-starts-at-1 assertion"
    - "A stale self-dogfood binary built from a clean-tree strict ancestor of HEAD is now detected and blocked (WR-01) — embedded_commit_is_stale's merge-base exit-0 branch now additionally requires git rev-parse HEAD to exactly equal embedded_commit for Fresh; any other exit-0 case (strict ancestor) is Stale. Reproduced via the exact linear two-commit clean-tree fixture (test wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks) and the corrected embedded_commit_is_stale_maps_ancestry_exit_codes test (base assertion flipped Fresh->Stale, new genuine-HEAD-equality Fresh case added)"
    - "AC-4 scope narrowing (missing-security-artifact + reviewer-set sub-checks deferred to Phase 18) is now a formally attributed overrides: entry in this file's frontmatter, and ROADMAP.md's Phase 17 Requirements line discloses the narrowing and cites this file — no longer an undisclosed partial claim"
  gaps_remaining: []
  regressions: []
---

# Phase 17: Pipeline Dogfood Follow-Up Verification Report

**Phase Goal:** Close the pipeline-reliability holes the Phase 16 dogfood exposed — `Unknown` completion
must never auto-advance a stage (17a), typed agent outcomes with a deterministic retry policy (17b), a
preflight readiness gate that fails before agent time is consumed (17c), and build provenance in
`workflow_started` so a stale self-dogfood binary is detectable (17d).

**Verified:** 2026-07-19T12:05:00Z
**Status:** passed
**Re-verification:** Yes — after gap closure (plan 17-06, commits `cb9ddab`, `f73a968`, `d307f72`, `510e6d5`)

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `Unknown` completion never auto-advances any stage (17a, D-01/D-06) | ✓ VERIFIED | Regression check: `outcome_policy::decide_action` still maps `Unknown -> GateReview` exhaustively (`outcome_policy.rs:38-56`); `cargo test -p devflow --bin devflow` green (61 unit tests) |
| 2 | A legitimately external-only stage with zero commits and a passing approved probe completes cleanly (17a, D-02/D-03/D-05) | ✓ VERIFIED | Regression check: `agent_result::` module unaffected by this plan's diff; `cargo test -p devflow-core agent_result::` → 276 devflow-core tests green |
| 3 | Typed outcomes `ResourceKilled`/`AgentUnavailable` route through a fail-closed exhaustive policy table (17b, D-07/D-11/D-12) | ✓ VERIFIED | Regression check: unchanged by this plan's diff; full suite green |
| 4 | Infrastructure-outcome ceiling (`state.infra_failures`) bounds a **stuck loop**, not the phase's lifetime (17b, D-08) | ✓ **VERIFIED** (was FAILED — CR-01 closed) | `transition()` now contains `state.infra_failures = 0;` immediately after `state.consecutive_failures = 0;`, before `workflow::save_state` persists (`main.rs:1640-1641`), with a doc comment citing CR-01. New test `transition_resets_infra_failures` (`main.rs:4191-4243`) seeds `infra_failures = MAX_INFRA_FAILURES - 1`, calls `transition()`, and asserts the reset holds both in-memory and in reloaded persisted state, then drives a fresh `handle_infra_outcome` and asserts it starts counting from 1 — proving the "stuck loop, not lifetime" semantics. Verified by direct execution: `cargo test -p devflow transition_resets_infra_failures` → 1 passed. `state.rs`/`mode.rs` doc comments updated to match (lines 40-41, 31-33) |
| 5 | `RateLimited` outcomes auto-resume via a safe, per-phase `devflow resume` path (17b, D-09) | ✓ VERIFIED | Regression check: unchanged by this plan's diff; `primary_loop_rate_limited_writes_single_agent_cron_instructions` and `rate_limited_at_infra_ceiling_stops_resuming_and_aborts` both pass |
| 6 | Every terminal advance decision emits structured, machine-readable evidence (17b, D-10) | ✓ VERIFIED | Regression check: unchanged by this plan's diff; full suite green |
| 7 | A preflight readiness gate runs before every stage launch, never a hard exit (17c, D-13/D-15/D-16) | ✓ VERIFIED | Regression check: unchanged by this plan's diff; full suite green |
| 8 | A non-interactive plan, unavailable reviewer, missing security artifact, or invalid required credential is reported before stage launch (retrospective AC-4) | ✓ **PASSED (override)** | Interactivity + Ship-scoped `gh auth status` checks unchanged and still tested. The missing-security-artifact and reviewer-set sub-checks remain unimplemented, but this is now a formally attributed, disclosed scope narrowing: `overrides:` entry added to this file's frontmatter (Task 3, commit `d307f72`) and echoed in `ROADMAP.md`'s Phase 17 Requirements line (`.planning/ROADMAP.md:187-191`), which explicitly names the deferral to Phase 18's Hermes adapter and cites this file. No longer an undisclosed partial claim |
| 9 | `workflow_started` records executable and build provenance (17d, D-21) | ✓ VERIFIED | Regression check: unchanged by this plan's diff; `workflow_started_payload_carries_build_provenance` passes |
| 10 | A stale self-dogfood binary is detected and blocked before stage launch (17d, D-17/D-18/D-19, retrospective AC-2) | ✓ **VERIFIED** (was FAILED — WR-01 closed) | `embedded_commit_is_stale`'s `merge-base --is-ancestor` exit-0 branch now nests a `git rev-parse HEAD` comparison (`main.rs:870-874`): only an exact match to HEAD returns Fresh; any other exit-0 outcome (a strict ancestor — HEAD moved since build) returns Stale. New test `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` reproduces the prior verification's exact live-reproduction fixture (linear two-commit, clean tree, embedded commit is a strict ancestor) and asserts `combined_staleness` is Stale and `enforce_build_staleness` hard-blocks with the `self_dogfood_stale_blocked` event recorded. The pre-existing `embedded_commit_is_stale_maps_ancestry_exit_codes` test's `base` assertion was corrected from Fresh to Stale (it previously encoded the bug), with a new genuine HEAD-equality Fresh case added. Verified by direct execution: both `cargo test -p devflow wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` and `cargo test -p devflow embedded_commit_is_stale` → 1 passed each |
| 11 | AC-1 (Phase 16's failed-Merge terminal-hook regression) still holds against final HEAD | ✓ VERIFIED | `terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished` and `terminal_hook_failure_stops_before_branch_cleanup` both re-run directly and pass |
| 12 | Full workspace test suite, clippy, and fmt are green | ✓ VERIFIED | `cargo test --workspace` (reduced `--test-threads=4` to avoid resource-contention flake, see below) → all green: 61 devflow-cli unit tests, 276 devflow-core unit tests, 2 monitor e2e, all integration suites; `cargo clippy --workspace -- -D warnings` clean; `cargo fmt --check` clean |

**Score:** 12/12 truths verified (0 failed, 0 partial, 1 verified via attributed override)

### Gap Closure Detail (from prior `gaps_found` verification, score 9/12)

| Gap | Prior Status | Fix Commit | Resolution |
|-----|--------------|------------|------------|
| CR-01 — `infra_failures` never resets | ✗ FAILED | `cb9ddab` | `transition()` resets `state.infra_failures = 0` alongside `state.consecutive_failures = 0`; regression test `transition_resets_infra_failures` proves in-memory + persisted reset and correct post-reset counting |
| WR-01 — clean-tree strict-ancestor build misclassified Fresh | ✗ FAILED | `f73a968` | `embedded_commit_is_stale` gates the merge-base exit-0 branch on an additional `git rev-parse HEAD` equality check; regression test `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` reproduces the exact prior-verification fixture and proves the hard-block now fires |
| AC-4 — undisclosed scope narrowing | ⚠️ PARTIAL | `d307f72` | Attributed `overrides:` entry added to this file's frontmatter; `ROADMAP.md`'s Requirements line updated to disclose the narrowing and cite this file |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-cli/src/main.rs` (`transition()`) | Resets `infra_failures` alongside `consecutive_failures` | ✓ VERIFIED | `main.rs:1631-1654`; `state.infra_failures = 0;` at line 1641, before `workflow::save_state` at 1643 |
| `crates/devflow-cli/src/main.rs` (`embedded_commit_is_stale`) | Distinguishes exact-HEAD-match Fresh from strict-ancestor Stale | ✓ VERIFIED | `main.rs:861-878`; nested `run_git_stdout(..., ["rev-parse", "HEAD"])` comparison inside the exit-0 arm |
| `crates/devflow-core/src/state.rs` | `infra_failures` doc comment matches reset-on-transition behavior | ✓ VERIFIED | Lines 40-41: "resets to 0 ... on every successful stage transition, alongside `consecutive_failures` (CR-01, 17-06 gap closure)" |
| `crates/devflow-core/src/mode.rs` | `MAX_INFRA_FAILURES` doc comment matches reset-on-transition behavior | ✓ VERIFIED | Lines 31-33: reset "is what makes the '5 unobserved cycles' ceiling bound a stuck loop rather than a phase's entire lifetime" |
| `.planning/phases/17-pipeline-dogfood-followup/17-VERIFICATION.md` | Attributed `overrides:` entry for AC-4, Gap 1/2 entries untouched | ✓ VERIFIED | Diff of commit `d307f72` shows only `overrides_applied: 0->1` and a new `overrides:` block added; `gaps:` list's first two entries byte-for-byte unchanged in that commit |
| `.planning/ROADMAP.md` | Phase 17 Requirements line discloses AC-4 narrowing | ✓ VERIFIED | Lines 185-191: names the narrowed scope, cites `17-VERIFICATION.md`'s `overrides:` frontmatter |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `transition()` | `state.infra_failures = 0` | reset alongside `consecutive_failures` | ✓ WIRED | `main.rs:1640-1641`, confirmed by source read and passing regression test (was NOT WIRED in prior verification) |
| `embedded_commit_is_stale` exit-0 branch | `run_git_stdout(..., ["rev-parse", "HEAD"])` | equality-gated Fresh/Stale decision | ✓ WIRED | `main.rs:870-874`, confirmed by source read and passing regression test |
| `17-VERIFICATION.md` `overrides:` entry | `ROADMAP.md` Phase 17 Requirements line | both describe the same narrowed AC-4 scope | ✓ WIRED | Both texts name plan-interactivity + Ship-scoped `gh auth` as retained, security-artifact + reviewer-set as deferred to Phase 18's Hermes adapter |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| CR-01 fix: `infra_failures` resets in-memory and persisted, then counts from 1 | `cargo test -p devflow transition_resets_infra_failures` | 1 passed, 0 failed | ✓ PASS |
| WR-01 fix: clean-tree strict-ancestor build is Stale and hard-blocks | `cargo test -p devflow wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` | 1 passed, 0 failed | ✓ PASS |
| Corrected ancestry test (base->Stale, HEAD->Fresh) | `cargo test -p devflow embedded_commit_is_stale` | 1 passed, 0 failed | ✓ PASS |
| Pre-existing infra-ceiling tests unmodified and still pass | `cargo test -p devflow infra_ceiling_aborts_instead_of_gating`, `rate_limited_at_infra_ceiling_stops_resuming_and_aborts`, `resource_killed_on_validate_bumps_infra_not_consecutive_failures` | all 3: 1 passed, 0 failed | ✓ PASS |
| Pre-existing staleness tests unmodified and still pass | `cargo test -p devflow combined_staleness_mtime_arm_flags_dirty_tree_newer_than_build`, `enforce_build_staleness_blocks_self_dogfood_and_records_event_before_erroring`, `enforce_build_staleness_warns_for_ordinary_project_with_stale_commit`, `enforce_build_staleness_never_blocks_on_indeterminate`, `is_self_dogfood_workspace_matches_both_member_paths_only` | all 5: 1 passed, 0 failed | ✓ PASS |
| AC-1 regression still holds | `cargo test -p devflow terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished`, `terminal_hook_failure_stops_before_branch_cleanup` | both: 1 passed, 0 failed | ✓ PASS |
| Full workspace suite green | `cargo test --workspace -- --test-threads=4` | all green (61 + 276 + 2 + integration suites) | ✓ PASS |
| Lint/format clean | `cargo clippy --workspace -- -D warnings`, `cargo fmt --check` | clean, no diff | ✓ PASS |

**Note on flake:** An initial default-parallelism `cargo test --workspace` run showed one unrelated failure,
`parallel_creates_two_worktrees_and_spawns_two_monitors` (`phase7_cli.rs:199`, `assert!(phase7_stdout.exists())`).
This test spawns two real subprocess monitors and polls for stdout files under a fixed timeout; it is untouched
by this plan's diff (`git diff cb9ddab^..510e6d5 -- crates/devflow-cli/tests/phase7_cli.rs` is empty) and passed
both in isolation and in a reduced-parallelism full-suite re-run, consistent with resource-contention flakiness
under this host's full-suite load rather than a regression introduced by the gap-closure commits.

### Requirements Coverage

No `.planning/REQUIREMENTS.md` file exists in this project; per-phase requirements are tracked in
`17-DOGFOOD-RETROSPECTIVE.md` (P1–P4, mapped 1:1 to scope units 17a–17d) and `17-CONTEXT.md` (D-01…D-21),
exactly as in the prior verification pass.

| Requirement | Source | Description | Status | Evidence |
|-------------|--------|-------------|--------|----------|
| P1 / 17a (D-01–D-06) | Retrospective + CONTEXT | `Unknown` non-advance + Layer 0/3 rework | ✓ SATISFIED | Truths 1, 2 (regression-checked) |
| P2 / 17b (D-07–D-12) | Retrospective + CONTEXT | Typed outcomes + deterministic retry policy | ✓ SATISFIED | Truths 3, 5, 6 (regression-checked); Truth 4 now VERIFIED (was FAILED) |
| P3 / 17c (D-13–D-16) | Retrospective + CONTEXT | Preflight readiness gate | ✓ SATISFIED | Truth 7 (regression-checked); Truth 8 now PASSED (override) — narrowed scope formally accepted |
| P4 / 17d (D-17–D-21) | Retrospective + CONTEXT | Build provenance + stale-binary detection | ✓ SATISFIED | Truth 9 (regression-checked); Truth 10 now VERIFIED (was FAILED) |
| AC-1 (criterion 1) | Retrospective | Failed-Merge terminal contract, verify only | ✓ SATISFIED | Truth 11 |
| D-01…D-21 | 17-CONTEXT.md | All 21 numbered decisions | ✓ SATISFIED | Every decision has a corresponding must-have; no orphaned decision |

No requirement IDs from PLAN frontmatter (`17a`/`17b`/`17c`/`17d`) are orphaned — all four appear in at least
one plan's `requirements:` field (17-06 itself declares `[17b, 17c, 17d]`) and are covered above.

### Anti-Patterns Found

None in the gap-closure diff (`git diff cb9ddab^..510e6d5` across `main.rs`, `state.rs`, `mode.rs`): no
`TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` markers introduced. The two prior 🛑 Blocker findings (CR-01
missing reset, WR-01 ancestry false-negative) are resolved by this plan's code changes and are no longer
present. The three ℹ️ Info findings from the prior verification (`build_timestamp`/`dirty` as JSON strings,
Layer 0 gating every stage including Plan, `is_self_dogfood_workspace`'s string-scan fragility against a
future glob) are unchanged — informational only, not blockers, not touched by this gap-closure plan's scope.

### Human Verification Required

None. Both previously-failed gaps are closed by direct code fixes confirmed via source reading and live
test execution (not SUMMARY.md narration), and the AC-4 partial is closed via a formally attributed,
disclosed override rather than requiring further implementation or human judgment.

### Gaps Summary

No gaps remain. All three items from the prior `gaps_found` verification (score 9/12) are closed:

1. **CR-01 (infra_failures reset)** — closed by code fix (`cb9ddab`), confirmed by direct source read
   (`main.rs:1641`) and by running the new regression test directly (`transition_resets_infra_failures`:
   1 passed).
2. **WR-01 (clean-tree strict-ancestor staleness false-negative)** — closed by code fix (`f73a968`),
   confirmed by direct source read (`main.rs:870-874`) and by running the new regression test directly
   (`wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks`: 1 passed) plus the corrected
   pre-existing test (`embedded_commit_is_stale_maps_ancestry_exit_codes`: 1 passed).
3. **AC-4 scope narrowing** — closed by recording an attributed `overrides:` entry (`d307f72`), confirmed
   present in this file's frontmatter and echoed in `ROADMAP.md`'s Requirements line.

Phase 17's goal — closing the pipeline-reliability holes the Phase 16 dogfood exposed — is achieved.

---

## Addendum 2026-07-19 — plan 17-08 landed after this verification

This report was written at `12:05Z` against 7 plans. An eighth plan, `17-08`, landed afterward
(`b570114` → `708499c`) to close `17-VALIDATION.md`'s GAP-1.

**The 12/12 verdict is unchanged, and 17-08 does not require re-verification.** 17-08 carries
requirement `17c`, which this report already verified; it added no new phase requirement and
changed no must_have. It fixed a defect *within* 17c: `run_preflight` resolved a failing-check
gate by recursing into a full `launch_stage`, then returned `Ok(())` into the middle of the outer
`launch_stage`, which continued and spawned a second competing agent for the same stage. The
return type is now `Result<bool, CliError>` and the call site short-circuits on `Ok(false)`. Net
effect on this report: 17c's "preflight reports a named gate before `spawn_monitor`" must_have is
now true on the `Advance`/`LoopBack` gate arms too, not only the `Abort` arm.

**Naming collision, for future readers:** the `CR-01` closed in the Gaps Summary above
(`infra_failures` reset, `cb9ddab`) is a *different* finding from the `CR-01` closed by 17-08
(preflight double spawn, `c03498d`). They come from two successive review passes that each
numbered their findings from 1.

Orchestrator-verified at `708499c`, independently of the executor's self-report: both new
regression tests were re-run against a surgically reintroduced defect and both failed, proving
they catch the bug rather than passing vacuously; `cargo test --workspace` green (64/64 bin +
276/276 core, 0 ignored); `cargo clippy --workspace --all-targets -- -D warnings` and
`cargo fmt --check` clean.

---

_Verified: 2026-07-19T12:05:00Z_
_Verifier: Claude (gsd-verifier)_
_Addendum: 2026-07-19T19:25:00Z — orchestrator (/gsd-execute-phase 17 --gaps-only)_
