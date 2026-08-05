---
phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99
verified: 2026-08-05T01:35:07Z
status: gaps_found
score: 4/4 must-haves verified (ROADMAP success criteria) — 1 additional plan-level acceptance
  criterion (33-03 Task 2's "pipeline_gate:: slice reports test result: ok") fails intermittently
behavior_unverified: 0
overrides_applied: 0
gaps:
  - truth: "The `pipeline_gate::` and `pipeline_outcomes::` test slices reliably report `test result: ok`, and `scripts/check.sh all` reliably exits 0 — 33-03 Task 2's own stated acceptance criterion, and the basis for every SUMMARY.md's 'scripts/check.sh all exits 0' claim"
    status: failed
    reason: >
      A pre-existing regression test, `pipeline_gate::tests::abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response`
      (crates/devflow-cli/src/pipeline_gate.rs:1104), seeds `state.consecutive_failures =
      mode::MAX_CONSECUTIVE_FAILURES - 1` directly and calls `handle_validate_outcome(root, &mut
      state, ValidateOutcome::Failed)` — the same shape as three sibling tests in
      pipeline_outcomes.rs that 33-03's "Rule 1" auto-fixed deviation seeded with
      `state.last_validate_failure_commit_count = Some(0)`. This fourth test, in the sibling file,
      was not found or fixed. Because its baseline is left at the default `None`,
      `mode::consecutive_failures_made_progress(None, 0)` reads as "progress" and resets
      `consecutive_failures` to 1 instead of accumulating it to the ceiling the test's own name and
      doc comment (CR-01) require. `should_gate` then returns false, so the test no longer takes
      its intended gate/abort path — it falls through to `loop_back_to_code`, which calls
      `launch_stage` and attempts to launch a real `claude` agent process. This test does not hold
      `ENV_MUTEX` (confirmed by reading its body), so it also races the sibling Phase-33 tests that
      swap `PATH` under `ENV_MUTEX`. Reproduced live during this verification: `scripts/check.sh
      all` (run once, cold) failed with exactly this test panicking
      (`called Result::unwrap() on an Err value: Message("agent binary \`claude\` not found — is it
      installed?")` at pipeline_gate.rs:1124); the same test passed in isolation
      (`--test-threads=1`, 32.87s — confirming it really does attempt a live agent launch, not a
      compile/setup error); of 4 subsequent full `cargo test -p devflow --bin devflow` runs, 3
      passed clean and 1 (interleaved with other heavy runs, so not independently conclusive)
      showed a mass cascade unrelated to this specific test. The clean, isolated evidence (1 cold
      `check.sh all` failure + 1 reproduction in isolation showing the real-launch behavior) is
      sufficient on its own: this is a genuine, intermittent test-suite regression, not a
      one-off environment fluke, and it makes `scripts/check.sh all` — the single quality gate this
      repo's CLAUDE.md and the plan's own acceptance criteria treat as authoritative — unreliable.
    artifacts:
      - path: "crates/devflow-cli/src/pipeline_gate.rs"
        issue: "abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response (line ~1104) seeds consecutive_failures directly without seeding the new last_validate_failure_commit_count baseline, so it silently takes a different, non-deterministic code path (a real agent launch attempt) than the one it asserts against"
    missing:
      - "Seed `state.last_validate_failure_commit_count = Some(0)` alongside `state.consecutive_failures = mode::MAX_CONSECUTIVE_FAILURES - 1` in `abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response`, matching the pattern 33-03 already applied to `validate_failure_threshold_forces_gate_then_aborts`, `drive_validate_advance_and_read_gate_context`, and `consecutive_failures_increment_saturates` in pipeline_outcomes.rs."
      - "After the fix, re-run `scripts/check.sh all` at least twice (or the pipeline_gate:: slice under default parallel threading several times) to confirm the flake is actually closed, not just less likely — the same class of bug (a directly-seeded-counter test with no baseline) could exist in other crates/tests not scanned here."
---

# Phase 33: Loop-Back Correctness for Multi-Wave Validate→Code Cycles Verification Report

**Phase Goal:** A 3+ wave unattended `devflow start` phase can complete its Code↔Validate loop
without gating on an impossible `--gaps-only` command or a false "3 consecutive failures" ceiling —
the two defects that have blocked every unattended multi-wave phase since the Phase 29 dogfood run.

**Verified:** 2026-08-05
**Status:** gaps_found
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Mid-arc Validate failure (no `{N}-VERIFICATION.md`) issues plain `/gsd-execute-phase {N}`, not `--gaps-only` | ✓ VERIFIED | `select_loop_back_fix` (pipeline_outcomes.rs:243-249) reads `agent_result::phase_verification_exists`; `mid_arc_loop_back_issues_plain_execute_command` — `1 passed` (re-run live) |
| 2 | Genuine-gaps Validate failure (`{N}-VERIFICATION.md` exists) still issues `--gaps-only` | ✓ VERIFIED | `genuine_gaps_loop_back_still_issues_gaps_only` — `1 passed` (re-run live) |
| 3 | 3+ wave phase making genuine forward progress (real commits between failures) does not false-gate at wave 3 | ✓ VERIFIED | `handle_validate_outcome` (pipeline_outcomes.rs:300-324) calls `mode::consecutive_failures_made_progress`; `healthy_multi_wave_progress_does_not_reach_the_ceiling` — `1 passed` (re-run live, `MAX_CONSECUTIVE_FAILURES + 1` cycles, counter stays at 1) |
| 4 | `consecutive_failures` still gates when Validate finds the same unresolved problem repeatedly (no new commits) | ✓ VERIFIED | `repeated_failure_without_new_commits_still_reaches_the_ceiling` and the byte-unchanged `consecutive_failures_reaches_ceiling_across_cycles` — both `1 passed` (re-run live) |

**Score:** 4/4 ROADMAP success criteria verified at the unit boundary, by re-running the named
tests directly against the current source (not by trusting SUMMARY.md's reported numbers).

All four plan-level `must_haves.truths` blocks (33-01, 33-02, 33-03) were additionally checked
against source directly (not only via their tests) — see "Required Artifacts" and "Key Link
Verification" below. All hold.

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `crates/devflow-core/src/agent_result.rs::phase_verification_exists` | `{N}-VERIFICATION.md` existence probe | ✓ VERIFIED | Defined line 2578, mirrors `phase_review_path` idiom; test `phase_verification_exists_finds_the_artifact_by_prefix` passes |
| `crates/devflow-core/src/prompt.rs::FixType::FullExecute` + `#[non_exhaustive]` | plain execute command variant, breaking-change guard | ✓ VERIFIED | Variant at line 60, `fix_prompt` arm at line 302, `#[non_exhaustive]` at line 49 (immediately above `pub enum FixType`); `rg "_ =>" prompt.rs` returns no match inside `fix_prompt` — no wildcard arm added |
| `crates/devflow-cli/src/pipeline_outcomes.rs::select_loop_back_fix` | single D-01 decision point | ✓ VERIFIED | Defined line 243; called from all three in-scope arms (lines 291, 343, 354) |
| `crates/devflow-cli/src/pipeline_gate.rs` `loop_back` event `"fix"` field | operator-visible fix selection | ✓ VERIFIED | Line 151, `"fix": format!("{fix:?}")` |
| `crates/devflow-core/src/state.rs::last_validate_failure_commit_count` | persisted forward-progress baseline | ✓ VERIFIED | `Option<u32>` field line 100, `#[serde(default)]` confirmed, `State::new` initializes `None` line 266; both serde tests pass |
| `crates/devflow-core/src/mode.rs::consecutive_failures_made_progress` | pure reset-vs-accumulate predicate | ✓ VERIFIED | `(previous: Option<u32>, current: u32) -> bool` at line 149, body `previous.is_none_or(\|p\| current > p)`; both tests pass; `transition_resets_consecutive_failures`'s own signature unchanged (`(from: Stage, to: Stage) -> bool` still matches) |
| `crates/devflow-core/src/agent_result.rs::phase_commit_count` | single git-derived commit count | ✓ VERIFIED | Line 1841; `evaluate_layer2` calls it (line 1905) instead of re-deriving the count inline; `phase_commit_count_reports_zero_without_a_branch` passes |
| `crates/devflow-cli/src/test_support.rs::commit_on_feature_branch` | test-only commit helper | ✓ VERIFIED | `rg "fn commit_on_feature_branch"` matches; used by both new multi-wave tests |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `pipeline_outcomes.rs` | `agent_result.rs` | `select_loop_back_fix` calls `phase_verification_exists` | ✓ WIRED | Confirmed at pipeline_outcomes.rs:244 |
| `pipeline_outcomes.rs` | `prompt.rs` | returns `FixType::FullExecute`, rendered by `fix_prompt` | ✓ WIRED | Confirmed, `fix_prompt` match arm renders unflagged command |
| `pipeline_outcomes.rs` | `agent_result.rs` | `handle_validate_outcome` calls `phase_commit_count` | ✓ WIRED | pipeline_outcomes.rs:301-302 |
| `pipeline_outcomes.rs` | `mode.rs` | passes baseline + fresh count to `consecutive_failures_made_progress` | ✓ WIRED | pipeline_outcomes.rs:303-306 |
| `agent_result.rs` (`evaluate_layer2`) | `agent_result.rs` (`phase_commit_count`) | single implementation, no duplicated git block | ✓ WIRED | `evaluate_layer2` line 1905 calls the helper; `rev-parse`/`rev-list` pair appears only inside `phase_commit_count` |

### Prohibitions (must-NOT checks)

| Prohibition | Status | Evidence |
|---|---|---|
| `handle_ship_outcome`'s loop-back must not be rewired to consult D-01 | ✓ HOLDS | pipeline_outcomes.rs:384 still constructs the bare `FixType::GapsOnly` literal directly |
| Ship's out-of-scope call site unaffected | ✓ HOLDS | `ship_loop_back_still_issues_gaps_only_when_verification_absent` passes |
| `transition_resets_consecutive_failures` signature not widened | ✓ HOLDS | `pub fn transition_resets_consecutive_failures(from: Stage, to: Stage) -> bool` unchanged |
| `consecutive_failures` must not reset unconditionally on every loop-back | ✓ HOLDS | Reset only fires inside the `progress` branch of `handle_validate_outcome`, gated on `consecutive_failures_made_progress` |
| `{N}-VERIFICATION.md` signal not reused as forward-progress signal | ✓ HOLDS | Two independent primitives — `phase_verification_exists` (filesystem) vs `phase_commit_count` (git) |
| `prepare_loop_back_to_code` must not gain counter logic | ✓ HOLDS | `sed ... \| grep -c 'state\.consecutive_failures ='` on pipeline_gate.rs production code = 1 (only `transition()`'s existing reset) |

### Behavioral Spot-Checks (Step 7b)

| Behavior | Command | Result | Status |
|---|---|---|---|
| All 8 phase-33-named tests in `devflow` (CLI) | `cargo test -p devflow "pipeline_outcomes::tests::<name>" -- --exact` × 8 | each `1 passed; 0 failed` | ✓ PASS |
| All 5 phase-33-named tests in `devflow-core` | `cargo test -p devflow-core --lib <name> -- --exact` × 5 | each `1 passed; 0 failed` | ✓ PASS |
| `cargo build --workspace --all-targets` | — | clean, 0 errors | ✓ PASS |
| `cargo fmt --check` | — | exit 0 | ✓ PASS |
| `scripts/check.sh all` (cold, single run) | `scripts/check.sh all` | **exit 101** — `pipeline_gate::tests::abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response` panicked | ✗ FAIL — see Gaps |
| Same failing test, isolated | `cargo test -p devflow ...abort_cleans_up... -- --exact --test-threads=1` | `1 passed`, 32.87s (confirms it now attempts a real agent launch instead of its intended fast gate/abort path) | ⚠️ passes alone, fails under contention |
| Full suite re-run × 2 more (clean, not stacked) | `cargo test -p devflow --bin devflow` | `269 passed; 0 failed` both times | mixed — confirms intermittency, not a deterministic failure |

This is the one place SUMMARY.md's claims do not hold up under direct re-execution: 33-01-SUMMARY.md
and 33-03-SUMMARY.md both assert `scripts/check.sh all` "exits 0"; re-running it live, cold, once,
produced a real (if intermittent) failure. See Gaps below.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|---|---|---|---|---|
| DOGFOOD-01 | 33-01 | Mid-arc vs genuine-gaps loop-back dispatch | ✓ SATISFIED | All D-01 truths verified above; REQUIREMENTS.md maps it to Phase 33 (traceability table, line 48) — accounted for, though its own checkbox/status cell still reads unchecked/"Pending" (doc hygiene, not a code gap — see Anti-Patterns) |
| DOGFOOD-02 | 33-02, 33-03 | Forward-progress-aware consecutive-failures reset | ✓ SATISFIED | All D-02 truths verified above; same REQUIREMENTS.md traceability/staleness note applies |

No orphaned requirements: REQUIREMENTS.md's Phase 33 mapping (DOGFOOD-01, DOGFOOD-02) matches
exactly what 33-01/33-02/33-03's PLAN frontmatter `requirements:` fields declare.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| `crates/devflow-cli/src/pipeline_gate.rs` | ~1104 | Test-suite regression: pre-existing test silently changed behavior, now non-deterministically launches a real agent process during `cargo test` | 🛑 Blocker | See Gaps — breaks `scripts/check.sh all`'s reliability, the plan's own acceptance gate |
| `.planning/REQUIREMENTS.md` | 19-23, 48-49 | DOGFOOD-01/02 checkboxes still `[ ]` and Traceability status still "Pending" despite Phase 33 shipping both | ℹ️ Info | Documentation hygiene only — the mapping itself is correct and complete, just not marked done |
| `crates/devflow-core/src/mode.rs` / `pipeline_outcomes.rs` | doc comments | WR-01 (33-REVIEW.md): the forward-progress reset removes the pipeline's only *unconditional* bound on the Code↔Validate loop — a trivial-commit-every-cycle agent never reaches the ceiling | ⚠️ Warning | Accepted design tradeoff, documented in source and in the phase's own threat model (T-33-06, disposition: accept, below the `high` blocking threshold); carried forward for human awareness, not a phase-goal failure |
| `crates/devflow-core/src/agent_result.rs` | 2578-2596 | WR-02 (33-REVIEW.md): `phase_verification_exists` has no staleness check — a re-planned in-flight phase with a stale `{N}-VERIFICATION.md` will keep choosing `--gaps-only` | ⚠️ Warning | Accepted, documented (T-33-01: accept, low). Out of D-01's stated scope (the "never validated" case), but worth a doc-comment update per the reviewer's own recommendation |

No `TBD`/`FIXME`/`XXX` debt markers found in any of the 7 files this phase modified.

### Human Verification Required

None — every ROADMAP success criterion and every plan `must_haves` item has direct automated
evidence. The one open item (the pipeline_gate.rs test regression) is a mechanical, well-scoped fix
with a clear precedent already in the same codebase (three sibling tests fixed the identical way),
not a judgment call.

### Gaps Summary

The phase's actual production logic — the two ROADMAP-defining defects (999.65's impossible
`--gaps-only` command, 999.66's false 3-failure gate) — is genuinely fixed and directly verified by
re-running every named test against current source, not by trusting SUMMARY.md's reported numbers.
All four ROADMAP success criteria hold.

The one real gap found during verification is narrower than the phase goal but still a legitimate
`gaps_found`: 33-03's own "Rule 1" fix (seeding the new baseline field in tests that directly seed
`consecutive_failures`, to stop the reset-vs-accumulate change from misreading a pre-seeded streak
as a first-ever failure) covered three tests in `pipeline_outcomes.rs` but missed a fourth,
identically-shaped test in the sibling file `pipeline_gate.rs`
(`abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response`). That test now silently
takes a different code path than the one it was written to assert — a real agent-launch attempt
instead of a fast, deterministic gate/abort — making `scripts/check.sh all` intermittently red under
default parallel test execution, contradicting the plan's own Task 2 acceptance criterion and both
33-01/33-03 SUMMARY.md's claim that the full check was clean. The fix is small and precedented: seed
`state.last_validate_failure_commit_count = Some(0)` in that one test, the same way the other three
were fixed, then re-verify `scripts/check.sh all` stays green across a few repeated runs (not just
once) given the failure is a race, not a deterministic compile/logic error.

---

_Verified: 2026-08-05_
_Verifier: Claude (gsd-verifier)_
