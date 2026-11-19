---
phase: 53-the-question-channel
plan: 12
subsystem: tests (devflow CLI e2e, antigravity driver)
tags: [tests, refused-agents, failover, capture-shape]
status: complete

requires: []
provides:
  - "phase7_cli: the three capture-shape properties (marker-less run gates at Plan, non-zero exit does not advance, hung agent detected and killable) pinned on `--agent codex`"
  - "agents::antigravity::tests::a_marker_less_or_init_only_stream_is_not_a_success: driver-level pin for the streams the deleted antigravity e2e stub printed"
  - "failover_e2e: handoff suite on claude and codex, `--mode supervise` for handoff runs; rate-limit-gate tests still claude, `--mode auto`"
  - "fallback_agent_config: flag `codex` wins over key `antigravity`"
affects: [53-08]

actuals:
  tokens: 14170
  tasks: 2
  commits: 2
plan_head_before: 43abdba382a231c956d7081cee45d85fd8fda3fa
plan_head_after: 9e155ced06aeccee0be2ca38589b0288773a37df

tech-stack:
  added: []
  patterns:
    - "failover_e2e Rig takes a `Shape` (primary, mode, Claude canary answer, Codex Code-turn behaviour); both the claude and codex stubs are always on PATH"
    - "The codex stub tells Plan from Code by the GSD workflow file its last argument names (plan-phase.md / execute-phase.md)"

key-files:
  created: []
  modified:
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-core/src/agents/antigravity.rs
    - crates/devflow-cli/tests/fallback_agent_config.rs
    - crates/devflow-cli/tests/failover_e2e.rs

key-decisions:
  - "failover_e2e fixture repo now commits a phase CONTEXT.md on develop: a codex primary is refused at start without one (codex interactivity preflight, preflight.rs 733), even in supervise mode at start"
  - "The worktree handoff test's inherited-status list drops ` M .planning/config.json` and its config check becomes exact equality: a supervised run writes no auto-chain flag, so the config is unchanged"
  - "The no-fallback rate-limit-gate negative control now watches the codex stub instead of agy, since agy is no longer on PATH; the property (a fallback-capable agent on PATH is never launched) is kept"
  - "After `resume --agent codex`, state.canary is asserted None: the handoff cleared the recorded Absent and codex has no canary to record"
  - "ASK-01 not marked complete: 53-01, 53-05, 53-07, 53-08, 53-09, 53-13 and 53-14 also carry it"

patterns-established: []

duration: 13min
completed: 2026-11-19
---

# Phase 53 Plan 12: Retarget tests off refused agents Summary

Every e2e test that launched pi, hermes or antigravity now runs on codex or claude, or at driver level, so 53-08 can refuse those agents without breaking `scripts/check.sh all`.

## What was done

**Task 1 (f8e1f58)**, phase7_cli, antigravity driver, fallback_agent_config:

| Test | Action | Reason |
|------|--------|--------|
| `pi_marker_less_run_does_not_advance` | converted to `codex_marker_less_run_does_not_advance` | same body, exit code and assertions, `--agent codex` |
| `pi_nonzero_exit_does_not_advance` | converted to `codex_nonzero_exit_does_not_advance` | same |
| `pi_hung_process_is_detected_not_left_running` | converted to `codex_hung_process_is_detected_not_left_running` | same |
| `hermes_marker_less_run_does_not_advance` | deleted | launch refused by operator answer (2); property pinned by `codex_marker_less_run_does_not_advance` |
| `hermes_nonzero_exit_does_not_advance` | deleted | launch refused by operator answer (2); property pinned by `codex_nonzero_exit_does_not_advance` |
| `hermes_hung_process_is_detected_not_left_running` | deleted | launch refused by operator answer (2); property pinned by `codex_hung_process_is_detected_not_left_running` |
| `marker_less_antigravity_never_advances` | deleted | launch refused by operator answer (2); property pinned by `agents::antigravity::tests::a_marker_less_or_init_only_stream_is_not_a_success` (quiet stream) |
| `antigravity_init_without_marker_gates_at_plan` | deleted | launch refused by operator answer (2); property pinned by `agents::antigravity::tests::a_marker_less_or_init_only_stream_is_not_a_success` (init-only stream) |
| `antigravity_parses_devflow_result_from_stream` | deleted | launch refused by operator answer (2); property pinned by `antigravity_driver_parse_completion_delegates` |
| `fallback_flag_wins_over_the_config_key` | converted | key `antigravity`, flag `codex`; codex line printed, antigravity line not; the `agy` stub dropped from PATH |

Orphaned helpers deleted: `pi_stub`, `hermes_stub`, `antigravity_stub`, `StubMode`. New helper `codex_stub(launch)`.

**Task 2 (9e155ce)**, failover_e2e:

| Test | Action |
|------|--------|
| `a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code` | converted to `a_rate_limited_claude_run_hands_off_to_codex_and_completes_code`: supervise, claude to codex; event order keeps claude canary, handoff, finish (antigravity canary event dropped, codex has no canary) |
| `a_handoff_inside_a_phase_worktree_keeps_the_tree_branch_and_uncommitted_work` | converted: claude to codex, supervise |
| `a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate` | converted to `a_handoff_to_claude_that_fails_its_canary_ends_at_a_gate`: codex primary, claude fallback withholds its canary token |
| `approving_the_failed_canary_gate_stops_the_run_and_names_the_way_on` | converted: codex to claude |
| `rejecting_the_failed_canary_gate_stops_the_run_and_names_the_way_on` | converted: codex to claude |
| `after_the_refused_relaunch_status_and_resume_lead_to_another_agent` | converted: resume leads to codex; the later handoff is claude to codex |

Deleted: the `agy` stub, `AgyCanary`, the antigravity constants. The claude stub now takes its canary answer as a placeholder; a codex stub replaces agy (rate-limited Code turn as primary, committing Code turn as fallback). No test was deleted in Task 2.

## Verification

- Task 1 gates: the five named tests pass (`cargo nextest run ... --no-tests=fail -E 'test(=...)'`, each `1 passed`).
- Task 2 gates: the six named tests pass, each `1 passed`.
- `cargo nextest run -p devflow --test failover_e2e`: 14 passed, including the eight unchanged rate-limit-gate tests.
- `cargo nextest run -p devflow --test phase7_cli --test fallback_agent_config`: 52 passed; `cargo nextest run -p devflow-core --lib`: 922 passed.
- `scripts/check.sh all` exit 0 on the working tree that became 9e155ce (fmt, clippy `-D warnings`, tests).
- Negative control for the canary assertions: on the same rig, the claude canary that returns its token gives a finished Code stage, and the one that withholds it gives the Code gate.
- No run in the edited test files starts with pi, hermes, opencode or antigravity as agent or fallback (rg over the three files). The only place antigravity still appears is the overridden key in `fallback_flag_wins_over_the_config_key`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] failover fixture needs a CONTEXT.md for a codex primary**
- **Found during:** Task 2
- **Issue:** `devflow start --agent codex` refused the fixture: "phase 5123 has no CONTEXT.md on `develop`, and OpenAI Codex cannot run an interactive discussion headless".
- **Fix:** `init_repo` commits `.planning/phases/<N>-failover/<N>-CONTEXT.md` on develop, as phase7_cli's fixture already does.
- **Files modified:** crates/devflow-cli/tests/failover_e2e.rs
- **Commit:** 9e155ce

**2. [Rule 1 - Test expectation] worktree handoff status list under supervise**
- **Found during:** Task 2
- **Issue:** the expected `git status` listed ` M .planning/config.json` (DevFlow's auto-chain flag write); a supervised run makes no such write, so the list was three entries.
- **Fix:** dropped that entry and replaced the "equal after removing the flag" check with exact equality of the on-disk and committed config, which is stronger.
- **Commit:** 9e155ce

**3. [Rule 3 - Blocking] no-fallback negative control referenced agy**
- **Found during:** Task 2
- **Issue:** the plan kept the rate-limit-gate tests unchanged, but `a_rate_limited_claude_run_without_a_fallback_waits_at_a_gate` asserted the agy stub on PATH was never launched; with agy deleted that check would pass vacuously.
- **Fix:** it now asserts the codex stub, which is on PATH in every rig, ran no stage turn.
- **Commit:** 9e155ce

## Self-Check: PASSED

- All four modified files exist; commits f8e1f58 and 9e155ce are ancestors of HEAD.
