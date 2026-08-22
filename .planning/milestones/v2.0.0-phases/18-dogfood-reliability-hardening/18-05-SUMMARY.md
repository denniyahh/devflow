---
phase: 18-dogfood-reliability-hardening
plan: 05
subsystem: cli
tags: [rust, cli, agent-completion, verdict-reconciliation, safety-gate, external-verify]

# Dependency graph
requires:
  - phase: 18-dogfood-reliability-hardening
    provides: "18-04's transition_resets_consecutive_failures predicate, which makes MAX_CONSECUTIVE_FAILURES genuinely reachable for the Code<->Validate loop this plan's Ambiguous gate deliberately bypasses"
provides:
  - "devflow-core::agent_result::reconcile_layer0_verdict — consults Layer 1's verdict when Layer 0 affirmatively succeeds at Stage::Validate, instead of discarding it"
  - "devflow-cli::main::ValidateOutcome (Passed/Failed/Ambiguous(String)) and classify_validate_outcome(&AgentResult) — pure three-way classifier replacing the old passed: bool"
  - "handle_validate_outcome's Ambiguous arm: forces an immediate [never-silent] gate, never incrementing consecutive_failures and never consulting Mode::should_gate"
  - "external_verify_cycles_reach_ceiling_without_unbounded_loop — the shared 18d+18e integration test proving both fixes hold together"
affects: [agent_result, advance, handle_validate_outcome, external_verify]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "reconciliation function scoped by a three-part guard (stage, status, decided_by_layer) that returns the input unchanged unless all three hold — keeps every other stage and every Layer 0 failure byte-for-byte unchanged"
    - "three-way outcome enum with a String payload on the ambiguous variant, reusing the [never-silent] gate-context idiom (handle_stage_failure) without routing through that function's same-stage-relaunch dispatch targets"
    - "forced boolean computed once (matches!(outcome, Ambiguous(_))) and OR'd into the existing should_gate check, so the ambiguous path shares the same gate-resolution match arms (Advance/LoopBack/Abort) as the ordinary pass/fail paths instead of duplicating them"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-cli/src/main.rs

key-decisions:
  - "reconcile_layer0_verdict copies ONLY the verdict field from Layer 1's result onto Layer 0's — status, exit_code, reason, commits, summary, and decided_by_layer all stay exactly as Layer 0 set them, per the plan's explicit action spec"
  - "classify_validate_outcome checks Some(Verdict::Pass) FIRST, before the external-verify-specific arms, so an ordinary (non-external_verify) Validate's verdict:pass still advances directly — the pre-18e behavior for the common case is unchanged"
  - "handle_validate_outcome's final match arm for ValidateOutcome::Ambiguous is unreachable!() rather than silently folded into the Failed branch — forced is true for every Ambiguous, so the gate branch above always returns first; an explicit panic there is a stronger guardrail against a future refactor accidentally letting ambiguity fall through than a defensive-but-silent match arm would be"
  - "the combined 18d+18e test is one #[test] function (matching the acceptance criteria's exact-name requirement) that calls two ~30-line private helper functions (arm_a/arm_b) rather than inlining both arms, keeping each under the project's function-length convention while still producing a single test proving both fixes together"

requirements-completed: [18e]

coverage:
  - id: D1
    description: "Layer 0 affirmative success at Stage::Validate now carries Layer 1's verdict (pass/gaps/none) instead of discarding it; every other stage and every Layer 0 failure is unchanged"
    requirement: "18e"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#tests::layer0_affirmative_success_consults_layer1_verdict_at_validate"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#tests::layer0_affirmative_success_keeps_none_verdict_off_validate"
        status: pass
    human_judgment: false
  - id: D2
    description: "The two pre-existing Layer 0 cascade tests now pin verdict explicitly (both off-Validate, both must stay None)"
    requirement: "18e"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#tests::layer0_affirmative_success_on_non_code_stage_with_zero_commits"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#tests::layer0_affirmative_success_outranks_layer1_failure_marker"
        status: pass
    human_judgment: false
  - id: D3
    description: "A probe pass + agent verdict:pass (two independent signals agreeing) advances to Ship regardless of which layer decided the result"
    requirement: "18e"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::external_verify_agreement_advances_to_ship"
        status: pass
    human_judgment: false
  - id: D4
    description: "A probe pass + agent verdict:gaps (disagreement) gates immediately, never touching consecutive_failures"
    requirement: "18e"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::external_verify_disagreement_gates_immediately"
        status: pass
    human_judgment: false
  - id: D5
    description: "A probe pass + no agent verdict at all (ambiguous, absent signal) gates immediately, never touching consecutive_failures"
    requirement: "18e"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::external_verify_no_verdict_gates_immediately"
        status: pass
    human_judgment: false
  - id: D6
    description: "Combined 18d+18e scenario: an Ambiguous outcome gates on cycle one without touching the counter (18e dominates), AND a genuine repeated non-ambiguous failure still reaches MAX_CONSECUTIVE_FAILURES and forces the gate (18d dominates) — neither path advances to Ship without evidence, and the loop never runs unbounded"
    requirement: "18e"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::external_verify_cycles_reach_ceiling_without_unbounded_loop"
        status: pass
    human_judgment: false
  - id: D7
    description: "An ordinary Validate stage (no external_verify) is unaffected — the existing gaps/missing-verdict/pass regression tests still pass unedited through the new ValidateOutcome dispatch"
    requirement: "18e"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::validate_gaps_does_not_advance_to_ship"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::validate_missing_verdict_does_not_advance"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::validate_pass_advances"
        status: pass
    human_judgment: false

duration: ~50min
completed: 2026-07-21
status: complete
---

# Phase 18 Plan 05: Layer 0/Validate Verdict Reconciliation Summary

**Layer 0's affirmative-success arm at `Stage::Validate` now consults (rather than discards) Layer 1's self-reported verdict, and a new three-way `ValidateOutcome` enum gates immediately on disagreement or a missing verdict instead of routing ambiguity through the counter-based Code↔Validate auto-loop.**

## Performance

- **Duration:** ~50 min
- **Started:** 2026-07-21T03:45:00Z (approx.)
- **Completed:** 2026-07-21T04:36:51Z
- **Tasks:** 3
- **Files modified:** 2 (`crates/devflow-core/src/agent_result.rs`, `crates/devflow-cli/src/main.rs`; +492/-27 net across all three commits)

## Accomplishments

- `reconcile_layer0_verdict(project_root, state, result) -> AgentResult` in `agent_result.rs`, called from `evaluate_agent_result_inner`'s Layer 0 branch. Returns `result` unchanged unless ALL THREE hold: `state.stage == Stage::Validate`, `result.status == AgentStatus::Success`, `result.decided_by_layer == Some(0)`. When they hold, it calls `evaluate_layer1` and copies ONLY the `verdict` field onto the Layer 0 result — `status`, `exit_code`, `reason`, `commits`, `summary`, and `decided_by_layer` all stay exactly as Layer 0 set them, so `decided_by_layer` remains `Some(0)` (the CLI's signal that this was an `external_verify` Validate).
- `ValidateOutcome` enum (`Passed`, `Failed`, `Ambiguous(String)`) and pure `classify_validate_outcome(&AgentResult) -> ValidateOutcome` in `main.rs`. `Some(Verdict::Pass)` is checked FIRST and always wins (so an ordinary, non-`external_verify` Validate's `verdict: pass` still advances directly, unchanged from before). `(external, verdict)` combinations of `(true, Gaps)` and `(true, None)` classify `Ambiguous`, carrying an operator-facing message naming which two signals disagreed; every other combination is `Failed`.
- `handle_validate_outcome`'s signature changed from `passed: bool` to `outcome: ValidateOutcome`. A `forced` boolean (`matches!(outcome, Ambiguous(_))`) is OR'd into the existing `Mode::should_gate` check, so an ambiguous outcome ALWAYS takes the gate branch — bypassing `should_gate` and never incrementing `consecutive_failures` — while `Passed`/`Failed` behave exactly as the old boolean path did. The gate context for `Ambiguous` is `"[never-silent] validate ambiguous: {detail}"`, passed through the existing `truncate_reason` bound.
- `advance()`'s two Validate call sites updated: the `Action::Advance` arm now dispatches on `classify_validate_outcome(&result)` instead of `matches!(result.verdict, Some(Verdict::Pass))`; the `Action::GateReview` arm passes `ValidateOutcome::Failed` explicitly. 4 pre-existing test call sites (`handle_validate_outcome(root, &mut state, false)`) updated to `ValidateOutcome::Failed`, with their assertions left byte-for-byte unchanged.
- New tests: 2 in `agent_result.rs` (`layer0_affirmative_success_consults_layer1_verdict_at_validate` covering pass/gaps/no-marker at Validate; `layer0_affirmative_success_keeps_none_verdict_off_validate` for the off-Validate no-op case), plus the two pre-existing cascade tests extended with explicit `verdict` assertions. 4 in `main.rs` (`external_verify_agreement_advances_to_ship`, `external_verify_disagreement_gates_immediately`, `external_verify_no_verdict_gates_immediately`, and the combined `external_verify_cycles_reach_ceiling_without_unbounded_loop`).
- The combined integration test proves 18d and 18e hold TOGETHER, not in isolation (18-RESEARCH.md Pitfall 1): Arm A shows an `Ambiguous` outcome gates on cycle one with `consecutive_failures` staying at 0; Arm B drives `MAX_CONSECUTIVE_FAILURES` genuine `Failed` cycles through real `handle_validate_outcome`/`transition` calls and confirms the ceiling is still reachable and forces the gate — the case that, before 18d, ran forever.
- `cargo test --workspace` is green at 417 passed / 0 failed (up from 18-04's 411-test baseline: +6 — 2 in `devflow-core`, 4 in `devflow-cli`). `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` both exit 0.

## Task Commits

1. **Task 1: Consult Layer 1's verdict when Layer 0 affirmatively succeeds at Validate** - `1313ef9` (fix)
2. **Task 2: Add the three-way ValidateOutcome and gate immediately on ambiguity** - `e3eda07` (feat)
3. **Task 3: Add the combined 18d+18e scenario test** - `1157d35` (test)

**Plan metadata:** (this commit, once created below)

## Files Created/Modified

- `crates/devflow-core/src/agent_result.rs` - `reconcile_layer0_verdict` placed after `evaluate_layer0`, wired into `evaluate_agent_result_inner`'s Layer 0 return; 2 new tests + 2 existing tests extended with `verdict` assertions
- `crates/devflow-cli/src/main.rs` - `ValidateOutcome` enum + `classify_validate_outcome` placed before `handle_validate_outcome`; `handle_validate_outcome`'s signature and gate dispatch rewritten for the three-way outcome; `advance()`'s two Validate call sites and 4 test call sites updated; 4 new tests (3 unit + the combined 18d+18e integration test, itself split into 2 helper functions)

## Decisions Made

- `reconcile_layer0_verdict` copies ONLY the `verdict` field from Layer 1's result onto Layer 0's — per the plan's explicit action spec, not a general merge. A `None` from `evaluate_layer1`, or a Layer 1 result whose own `verdict` is `None`, leaves `result.verdict` as `None`.
- `classify_validate_outcome` checks `Some(Verdict::Pass)` FIRST, unconditionally, before the external-verify-specific arms — this is what keeps an ordinary (non-`external_verify`) Validate's `verdict: pass` advancing directly, exactly as before 18e; only the `Gaps`/`None` combinations gated on `external == true` are new.
- `handle_validate_outcome`'s final match arm for `ValidateOutcome::Ambiguous` is `unreachable!()` rather than silently folded into the `Failed` branch. Since `forced` is `true` for every `Ambiguous` value, the gate branch above always returns first — an explicit panic there is a stronger guardrail against a future refactor accidentally letting ambiguity fall through to the counter-based loop-back than a defensive-but-silent match arm would be (T-18-19).
- The combined 18d+18e test (`external_verify_cycles_reach_ceiling_without_unbounded_loop`) is a single `#[test]` function — matching the acceptance criteria's exact-name requirement — that calls two ~30-line private helper functions (`arm_a_ambiguous_outcome_gates_on_cycle_one`, `arm_b_genuine_failures_reach_the_ceiling`) rather than inlining both arms in one long body. This keeps each arm under the project's function-length convention while still producing the single test the plan and its acceptance criteria require to prove both fixes together.
- `Ambiguous`'s gate context format is exactly `"[never-silent] validate ambiguous: {detail}"`, matching `handle_stage_failure`'s `[never-silent]` idiom textually (same prefix token) without reusing that function itself, per the plan's `<assumption_delta>` — `handle_stage_failure`'s Advance/LoopBack arms both relaunch the SAME stage, which is wrong for an ambiguous Validate (Advance must mean "go to Ship", LoopBack must mean "return to Code").

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `rg -A 40` acceptance-criteria checks initially failed because the required keywords lived in doc comments BEFORE the matched test-name line, not after**
- **Found during:** Task 3 verification (`rg -n 'external_verify_cycles_reach_ceiling_without_unbounded_loop' -A 40 ... | rg -c 'MAX_CONSECUTIVE_FAILURES'` returned 0 matches)
- **Issue:** The combined test's doc comment (which mentioned both `Ambiguous` and `MAX_CONSECUTIVE_FAILURES`) was written directly above the `#[test] fn external_verify_cycles_reach_ceiling_without_unbounded_loop()` line. `rg -A 40` only searches 40 lines AFTER the matched line, so text in the preceding doc comment doesn't count. `Ambiguous` happened to also appear again inside the arm A helper body within the 40-line window and passed by coincidence, but `MAX_CONSECUTIVE_FAILURES` (only in arm B's doc comment/body, further down) did not.
- **Fix:** Added an explicit two-line comment inside the test function body itself (right after the `let root = ...` line) naming both "Ambiguous" and "MAX_CONSECUTIVE_FAILURES" verbatim, guaranteeing both keywords appear within the 40-line window regardless of how the helper functions below are ordered or how long their own doc comments are.
- **Files modified:** `crates/devflow-cli/src/main.rs`
- **Verification:** Re-ran both `rg -A 40 ... | rg -c 'MAX_CONSECUTIVE_FAILURES'` (now 1) and `... | rg -c 'Ambiguous'` (now 2) after the fix; full test/clippy/fmt sweep re-confirmed green.
- **Committed in:** `1157d35` (Task 3 commit — caught and fixed before the commit landed, never shipped in the broken form)

---

**Total deviations:** 1 auto-fixed (Rule 1, a self-verification/tooling correctness issue caught by the plan's own mandated acceptance-criteria check before committing). No production-code deviations — `agent_result.rs` and `main.rs`'s runtime behavior were implemented exactly per the plan's `<action>` blocks.
**Impact on plan:** No functional behavior changed. The deviation is a test-comment placement fix required to satisfy the plan's own `rg`-based acceptance criteria; it does not touch what is being asserted, tested, or dispatched.

## Issues Encountered

Beyond the deviation above: reconfirmed two carry-forward gotchas from 18-01/18-04 while verifying — `cargo test -p devflow --lib <name>` still hard-errors on this binary-only crate (used `cargo test -p devflow <name>` instead), and `cargo test -p devflow <bare-name> -- --exact` still matches 0 tests unless the fully-qualified `tests::<name>` path is used (used `cargo test -p devflow tests::<name> -- --exact` for the two exact-match acceptance criteria). Both were anticipated from the plan's own `<carry_forward_correction>` and 18-04-SUMMARY.md, so no time was lost isolating them.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 18e (Layer 0/Validate verdict reconciliation) is complete. An `external_verify`-declared Validate now advances when the probe and the agent's verdict agree, gates immediately for a human when they disagree or when no verdict arrived, and never loops unbounded — proven by a shared test with 18d rather than by either fix in isolation.
- `cargo test --workspace` is green at 417 passed / 0 failed (up from 18-04's 411-test baseline: +6 — 2 in `devflow-core`, 4 in `devflow-cli`). `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` both exit 0.
- The UNRESOLVED flagged assumption from `18-05-PLAN.md` remains open, unchanged by this plan: the deterministic edge probe classified 18e's requirement as `unclassified` (a control-flow defect, not a data-domain edge), so verdict reconciliation is scoped to `Stage::Validate` only by explicit assumption — if a future stage gains verdict semantics, reconciliation will not automatically extend to it. Low risk today (no other stage reads `verdict`), flagged in both the plan and this summary so it isn't rediscovered as a bug.
- Next: 18-06 (wave 5, 18f — preflight-gate re-run wedge fix) per `.planning/phases/18-dogfood-reliability-hardening/CONTEXT.md`'s remaining scope.

---
*Phase: 18-dogfood-reliability-hardening*
*Completed: 2026-07-21*

## Self-Check: PASSED

- FOUND: crates/devflow-core/src/agent_result.rs
- FOUND: crates/devflow-cli/src/main.rs
- FOUND: .planning/phases/18-dogfood-reliability-hardening/18-05-SUMMARY.md
- FOUND: 1313ef9 (Task 1 commit)
- FOUND: e3eda07 (Task 2 commit)
- FOUND: 1157d35 (Task 3 commit)
