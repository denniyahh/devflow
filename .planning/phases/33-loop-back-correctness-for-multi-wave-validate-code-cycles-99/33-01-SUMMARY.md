---
phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99
plan: 01
subsystem: infra
tags: [rust, state-machine, gate, loop-back, devflow-core, devflow-cli]

# Dependency graph
requires:
  - phase: 30
    provides: consecutive_failures counter reachability (18d/18e), the safety-gate ceiling this plan's tests drive against but must not perturb
provides:
  - "agent_result::phase_verification_exists — {N}-VERIFICATION.md existence probe"
  - "prompt::FixType::FullExecute — plain /gsd-execute-phase {N} command, plus #[non_exhaustive] on FixType"
  - "pipeline_outcomes::select_loop_back_fix — the single D-01 decision point for all three in-scope loop-back arms"
  - "loop_back event payload now carries the selected fix variant"
affects: [33-02, 33-03, phase-34]

# Actuals (#2632)
actuals:
  tokens: 5553
  tasks: 2
  commits: 2

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Single decision-point helper (select_loop_back_fix) consulted by every in-scope call site instead of the D-01 check being expressed three times"
    - "#[non_exhaustive] applied to a published enum at the same release that already pays a breaking-change cost for a new variant, verified empirically (measured zero wildcard-arm impact, with a negative control) before applying"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs

key-decisions:
  - "FixType gains a new variant (FullExecute) AND the mid-arc/gaps branch happens upstream in one pipeline_outcomes.rs helper — locked in CONTEXT.md's Claude's-Discretion resolution, implemented as written"
  - "#[non_exhaustive] added to FixType in the same release as the new variant (operator decision, 2026-08-04) — verified zero wildcard arms needed anywhere in the workspace, with a negative control proving the attribute actually bites"

patterns-established:
  - "Loop-back fix selection is now a single pure predicate (select_loop_back_fix) rather than three independent bare-literal call sites — future loop-back arms should call it, not construct FixType directly"

requirements-completed: [DOGFOOD-01]

coverage:
  - id: D1
    description: "A Validate failure on a phase with no {N}-VERIFICATION.md loops back to Code with the plain /gsd-execute-phase {N} command (ROADMAP criterion 1), across all three in-scope arms: the plain-Failed tail, the Ambiguous gate, and the consecutive-failure gate"
    requirement: DOGFOOD-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#mid_arc_loop_back_issues_plain_execute_command"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#ambiguous_gate_loop_back_respects_the_mid_arc_check"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#failure_gate_loop_back_respects_the_mid_arc_check"
        status: pass
    human_judgment: false
  - id: D2
    description: "A Validate failure on a phase whose {N}-VERIFICATION.md exists still loops back with --gaps-only, unchanged (ROADMAP criterion 2)"
    requirement: DOGFOOD-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#genuine_gaps_loop_back_still_issues_gaps_only"
        status: pass
    human_judgment: false
  - id: D3
    description: "The Ship loop-back is provably unaffected — issues --gaps-only even with no verification artifact present (D-02 out-of-scope call site)"
    requirement: DOGFOOD-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#ship_loop_back_still_issues_gaps_only_when_verification_absent"
        status: pass
    human_judgment: false
  - id: D4
    description: "The loop_back event's fix field lets an operator see which command was dispatched from .devflow/events.jsonl without reading source"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs (all four new tests read last[\"fix\"] back from events::last_event_of_kind_for_phase)"
        status: pass
    human_judgment: false
  - id: D5
    description: "FixType gains #[non_exhaustive] in the same release as the new variant, with zero wildcard match arms needed anywhere in the workspace"
    verification:
      - kind: other
        ref: "cargo check --workspace --all-targets (0 errors, no E0004); rg -n \"_ =>\" crates/devflow-core/src/prompt.rs (no match inside fix_prompt)"
        status: pass
    human_judgment: false

duration: 35min
completed: 2026-08-04
status: complete
---

# Phase 33 Plan 01: Loop-Back Correctness — Mid-Arc vs Genuine-Gaps Summary

**Closed 999.65 (DOGFOOD-01): a Validate→Code loop-back on a mid-arc phase now issues the plain
`/gsd-execute-phase {N}` command instead of `--gaps-only`, which matched zero plans and gated
unresolvably — the defect that blocked every unattended multi-wave `devflow start` run since the
Phase 29 dogfood.**

## Performance

- **Duration:** ~35 min (base commit 19:51:21 → last task commit 20:26:04, 2026-08-04)
- **Tasks:** 2/2 completed
- **Files modified:** 4 (`agent_result.rs`, `prompt.rs`, `pipeline_gate.rs`, `pipeline_outcomes.rs`)

## Accomplishments

- `agent_result::phase_verification_exists` — a `{N}-VERIFICATION.md` existence probe by
  phase-directory prefix scan, mirroring the existing `phase_review_path` idiom exactly, returning
  `bool` rather than `Option<PathBuf>` since no caller needs the artifact's path.
- `prompt::FixType::FullExecute` — the third `FixType` variant, rendering the unflagged
  `/gsd-execute-phase {N}` command — plus `#[non_exhaustive]` on the enum itself (operator decision,
  2026-08-04), paying the breaking-change cost once rather than twice.
- `pipeline_outcomes::select_loop_back_fix` — the single D-01 decision point, now consulted by all
  three in-scope loop-back arms inside `handle_validate_outcome` (the plain-Failed tail, the
  Ambiguous gate, and the consecutive-failure gate). `handle_ship_outcome`'s loop-back is
  deliberately untouched per D-02.
- The `loop_back` event payload now carries a `"fix"` key (the `Debug` rendering of the chosen
  `FixType`), so an operator can read `.devflow/events.jsonl` and see which fix command a loop-back
  chose without reading source.
- Six new tests (one more than the plan's four required-by-name — `1 passed` confirmed for each):
  `phase_verification_exists_finds_the_artifact_by_prefix`, the extended
  `fix_prompts_select_the_right_command`, `mid_arc_loop_back_issues_plain_execute_command`,
  `genuine_gaps_loop_back_still_issues_gaps_only`,
  `ambiguous_gate_loop_back_respects_the_mid_arc_check`,
  `failure_gate_loop_back_respects_the_mid_arc_check`, and
  `ship_loop_back_still_issues_gaps_only_when_verification_absent` — the last is the negative
  control for the whole D-01 change.

## RED-first evidence (ai-change-acceptance requirement 1 + 3)

Per the plan's instruction, `FixType::FullExecute` and `select_loop_back_fix`'s signature were
added first (to get past the inevitable compile-error RED), then the two Task 1 tracer tests were
run against the still-unwired plain-Failed tail arm (which still constructed the bare
`FixType::GapsOnly` literal) before wiring the call site:

- `mid_arc_loop_back_issues_plain_execute_command` — genuine assertion failure, not a compile error
  or panic:
  ```
  thread '...' panicked at crates/devflow-cli/src/pipeline_outcomes.rs:1265:9:
  assertion `left == right` failed: a mid-arc phase (no {N}-VERIFICATION.md) must dispatch
  FullExecute, not GapsOnly
    left: String("GapsOnly")
   right: "FullExecute"
  ```
- `genuine_gaps_loop_back_still_issues_gaps_only` — passed immediately (expected: its precondition
  has a verification artifact, and the unwired literal already emitted `GapsOnly`), which is the
  correct negative-control shape: only the mid-arc test should flip red-to-green on wiring.

After wiring line 311's tail arm to call `select_loop_back_fix`, both tests passed
(`1 passed; 0 failed` for each, confirmed by name with `--exact`).

## Task Commits

1. **Task 1: End-to-end mid-arc loop-back — one path through every layer** - `57f1d62` (feat)
2. **Task 2: Expand to the remaining two in-scope arms, and prove the Ship arm untouched** -
   `ff28032` (feat)

_Both tasks were TDD (`tdd="true"`); each commit bundles the test additions with the wiring change
that made them pass, per the plan's own combined-diff task shape rather than separate
test-then-feat commits._

## Files Created/Modified

- `crates/devflow-core/src/agent_result.rs` — `phase_verification_exists` + its test
- `crates/devflow-core/src/prompt.rs` — `FixType::FullExecute`, `#[non_exhaustive]`, the new
  `fix_prompt` match arm, extended `fix_prompts_select_the_right_command`
- `crates/devflow-cli/src/pipeline_gate.rs` — `"fix"` key on the `loop_back` event payload
- `crates/devflow-cli/src/pipeline_outcomes.rs` — `select_loop_back_fix`, three call-site rewires,
  and five new tests

## Decisions Made

- **`FixType` gains a new variant, and the branch happens upstream** — locked in CONTEXT.md's
  "Claude's Discretion" section, implemented exactly as recorded there: `select_loop_back_fix`
  lives in `pipeline_outcomes.rs`, keeping `prompt::fix_prompt` a pure string builder with no
  filesystem knowledge.
- **`#[non_exhaustive]` added to `FixType` in this same release** (operator decision, 2026-08-04).
  Verified empirically before applying — the only `match` over a `FixType` value anywhere in the
  workspace is `fix_prompt` itself, which lives in the defining crate and is unaffected. Confirmed
  live: `cargo check --workspace --all-targets` exits 0 with no `E0004`, and
  `rg -n "_ =>" crates/devflow-core/src/prompt.rs` returns no match inside `fix_prompt` — no
  wildcard arm was added or needed anywhere.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Task 2's three new tests spawned a real agent CLI process on first attempt**
- **Found during:** Task 2, running the `ambiguous_gate_loop_back_respects_the_mid_arc_check`,
  `failure_gate_loop_back_respects_the_mid_arc_check`, and
  `ship_loop_back_still_issues_gaps_only_when_verification_absent` tests for the first time.
- **Issue:** The initial draft of these three tests (unlike Task 1's two tests) omitted the
  PATH-neutralization step the plan's action text explicitly specified ("all three tests, all
  following Task 1's PATH-neutralized-under-`ENV_MUTEX` shape"). Without a neutralized `PATH`, the
  `LoopBack` action's `launch_stage` call found the real `claude` binary and ran DevFlow's own
  D-15 delivery-canary preflight — an actual throwaway `claude` session — before attempting the
  real stage launch. The Ambiguous and failure-gate tests each took ~40s; the Ship test exceeded a
  120s foreground timeout and had to be backgrounded, eventually failing after 60+ real seconds
  with a delivery-canary-absent error rather than a fast, deterministic assertion.
- **Fix:** Rewrote all three tests to hold `ENV_MUTEX`, replace `PATH` with
  `agent_free_git_only_path_dir()` before driving the outcome handler, and restore the original
  `PATH` afterward — identical to Task 1's `mid_arc_loop_back_issues_plain_execute_command` shape.
  Confirmed no orphaned processes or stray filesystem writes resulted from the earlier attempt
  (the canary's capture directory was a tempdir; `git status --short` showed no unexpected changes
  in the worktree).
- **Files modified:** `crates/devflow-cli/src/pipeline_outcomes.rs` (same file already in scope for
  Task 2 — no additional files touched)
- **Verification:** All three tests now complete in ≤0.02s each; re-ran `cargo test -p devflow
  <name> -- --exact` for each and confirmed `1 passed; 0 failed`.
- **Committed in:** `ff28032` (Task 2 commit — the fix landed before the commit, so no separate
  fix commit was needed)

---

**Total deviations:** 1 auto-fixed (Rule 1 — a bug in the test's own environment isolation, not in
production code).
**Impact on plan:** No scope creep; the fix only touched the same test functions Task 2 already
specified, bringing them into conformance with the plan's own PATH-neutralization instruction.

## Issues Encountered

None beyond the deviation above.

## User Setup Required

None — no external service configuration required.

## Verification Summary

All plan-mandated verification commands pass with a literal `1 passed` line:

```
cargo test -p devflow-core --lib prompt::tests::fix_prompts_select_the_right_command -- --exact
cargo test -p devflow-core --lib agent_result::tests::phase_verification_exists_finds_the_artifact_by_prefix -- --exact
cargo test -p devflow pipeline_outcomes::tests::mid_arc_loop_back_issues_plain_execute_command -- --exact
cargo test -p devflow pipeline_outcomes::tests::genuine_gaps_loop_back_still_issues_gaps_only -- --exact
cargo test -p devflow pipeline_outcomes::tests::ambiguous_gate_loop_back_respects_the_mid_arc_check -- --exact
cargo test -p devflow pipeline_outcomes::tests::failure_gate_loop_back_respects_the_mid_arc_check -- --exact
cargo test -p devflow pipeline_outcomes::tests::ship_loop_back_still_issues_gaps_only_when_verification_absent -- --exact
```

`scripts/check.sh all` (fmt + `clippy --workspace --all-targets -- -D warnings` + `cargo test
--workspace --no-fail-fast`) exits 0 — 542 tests passed in `devflow` (the CLI package) plus the
full `devflow-core` suite, 0 failed. `consecutive_failures_reaches_ceiling_across_cycles` (the
999.66 safety gate 33-03 will later narrow) passed unmodified, confirmed by name in the full-suite
run.

Additional structural acceptance checks confirmed live:
- `rg -n "phase_verification_exists" crates/devflow-cli/src/pipeline_outcomes.rs
  crates/devflow-core/src/agent_result.rs` shows the definition plus the `select_loop_back_fix`
  call.
- `rg -n -B3 "pub enum FixType" crates/devflow-core/src/prompt.rs` shows `#[non_exhaustive]`
  between the `derive` line and the enum.
- `rg -c "select_loop_back_fix" crates/devflow-cli/src/pipeline_outcomes.rs` = 6 (1 definition + 3
  call sites + 2 doc-comment mentions — at least 4 required).
- `rg -n "FixType::GapsOnly" crates/devflow-cli/src/pipeline_outcomes.rs` still shows the bare
  literal on `handle_ship_outcome`'s `loop_back_to_code` call (line 350) — confirmed byte-unchanged.
- `sed -n '1,/^#\[cfg(test)\]/p' crates/devflow-cli/src/pipeline_gate.rs | grep -c
  'state\.consecutive_failures ='` prints `1` — the only production assignment in this file is
  still `transition()`'s existing reset, confirming the `loop_back` event edit added a field and
  nothing else.

## Next Phase Readiness

- `FixType::FullExecute`, `phase_verification_exists`, and `select_loop_back_fix` are now available
  for 33-02 and 33-03, which own the remaining phase artifacts (the 999.66 `State` field, the
  `mode` predicate, the commit-count helper, and the test-support commit helper) per this plan's
  "Artifacts this phase produces" table.
- No blockers. `consecutive_failures_reaches_ceiling_across_cycles` — the safety gate 33-03 will
  narrow — was confirmed passing unmodified, so 33-03 starts from a known-good baseline.

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/agent_result.rs`
- FOUND: `crates/devflow-core/src/prompt.rs`
- FOUND: `crates/devflow-cli/src/pipeline_gate.rs`
- FOUND: `crates/devflow-cli/src/pipeline_outcomes.rs`
- FOUND: `.planning/phases/33-loop-back-correctness-for-multi-wave-validate-code-cycles-99/33-01-SUMMARY.md`
- FOUND commit `57f1d62` (Task 1)
- FOUND commit `ff28032` (Task 2)
- FOUND commit `1c57dce` (this SUMMARY)

---
*Phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99*
*Completed: 2026-08-04*
