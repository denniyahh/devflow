---
phase: 21-operator-usability-release-execution
plan: 02
subsystem: cli
tags: [clap, rust, operator-tooling, status, gates, observability]

# Dependency graph
requires:
  - phase: 21-01
    provides: staleness-guard sequencing (D-07 wave ordering; no code coupling)
provides:
  - "devflow gate show <phase> [--stage <stage>] — untruncated, sanitized gate context"
  - "status cron hints now surface the already-computed rate-limit reset time"
  - "status in-stage progress line derived from the real stage_launched event ts"
  - "status stuck-phase recovery-verb hints (resume/advance) via a pure, tested helper"
affects: [operator-usability-release-execution, cli-observability]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Pure render-then-println helpers for testability without capturing stdout (render_gate_show, render_stage_progress_line, cron_hint_line — mirrors the existing render_pending_gate_banner idiom)"
    - "Event-log-derived timestamps (latest_stage_launched_ts) instead of phase-level State fields for stage-scoped display data"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs

key-decisions:
  - "gate_show duplicates gate_respond's stage auto-resolve-single-open-gate match block rather than extracting a shared function — kept the diff minimal per the plan's 'reuse verbatim' instruction; both paths stay structurally identical so they can't silently drift, and no third caller yet justifies the extraction."
  - "recovery_hints only adds an `advance` hint when gate_pending is true (not for every Stuck phase) — the phase's own Review Incorporation section defers widening this predicate as a documented LOW-severity tradeoff; a phase stuck without a pending gate still gets `resume`."

requirements-completed: [21a, D-03]

coverage:
  - id: D1
    description: "devflow gate show <phase> [--stage] prints the FULL, sanitized gate context (never truncated), with the same none/one/many auto-resolve semantics as gate approve/reject"
    requirement: "21a"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_show_*"
        status: pass
      - kind: manual_procedural
        ref: "devflow gate show 15 <fixture> against a 150-char context — full text printed, no [truncated] marker; devflow gate list on the same fixture shows the truncated 100-char form for contrast"
        status: pass
    human_judgment: false
  - id: D2
    description: "status surfaces the Hermes cron rate-limit reset time from the existing CronInstructions.retry_after, sanitized, with no new detection logic"
    requirement: "21a"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::cron_hint_line_appends_sanitized_reset_when_retry_after_present"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::cron_hint_line_omits_reset_fragment_when_retry_after_empty"
        status: pass
    human_judgment: false
  - id: D3
    description: "status shows a real in-stage elapsed-time progress line derived from the latest stage_launched event, never from the phase-level State.started_at (closes the phase's single 3/3-review MUST-FIX)"
    requirement: "21a"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::latest_stage_launched_ts_reflects_event_age_not_phase_started_at"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::render_stage_progress_line_omits_age_without_stage_launched_event"
        status: pass
      - kind: manual_procedural
        ref: "devflow status on a synthetic fixture (started_at 30m ago, stage_launched 90s ago) — printed 'in stage code: 1m ago', never '30m ago'"
        status: pass
    human_judgment: false
  - id: D4
    description: "stuck phases surface resume/advance recovery verbs from a pure, unit-tested helper instead of an inline println"
    requirement: "21a"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::recovery_hints_includes_resume_for_stuck"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::recovery_hints_includes_advance_when_stuck_and_gate_pending"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::recovery_hints_empty_for_healthy"
        status: pass
    human_judgment: false

# Metrics
duration: ~25min
completed: 2026-07-23
status: complete
---

# Phase 21 Plan 02: Operator Discoverability (21a) Summary

**`devflow gate show <phase>` for untruncated gate context, plus `status` now surfaces the Hermes rate-limit reset time, a real event-derived in-stage progress line, and stuck-phase recovery verbs — all read-only presentation, zero pipeline behavior change.**

## Performance

- **Duration:** ~25 min
- **Completed:** 2026-07-23
- **Tasks:** 3 (1 tracer + 2 auto, all `tdd="true"`)
- **Files modified:** 2 (`crates/devflow-cli/src/main.rs`, `crates/devflow-cli/src/commands.rs`)

## Accomplishments
- `devflow gate show <phase> [--stage <stage>]` — new `GateCmd::Show` subcommand printing the full, control-char-sanitized gate context (`render_gate_context(ctx, usize::MAX)`), with the same none/one/many auto-resolve-single-open-gate semantics as `gate approve`/`gate reject`.
- `status`'s cron-instruction hints now append a sanitized `(rate-limit resets: …)` segment when `CronInstructions.retry_after` is non-empty — presentation of an already-computed field, no new rate-limit detection.
- `status` gained a per-phase `in stage {stage}: {age}` progress line sourced from the LAST `stage_launched` event's `ts` in the event log (`latest_stage_launched_ts`) — never from the phase-level `State.started_at`, closing the phase's single unanimous (3/3) cross-AI review MUST-FIX.
- Stuck phases now surface recovery verbs (`devflow resume`, plus `devflow advance` when gate-pending) via a pure, unit-tested `recovery_hints` helper, replacing the old inline `println!`.

## Task Commits

Each task followed RED → GREEN (TDD):

1. **Task 1: `devflow gate show <phase>` — untruncated gate context (tracer)**
   - `daa48fb` (test) — failing tests + `GateCmd::Show` variant/dispatch wiring (compile-fails: `gate_show`/`render_gate_show` not yet implemented)
   - `7839381` (feat) — `gate_show` + `render_gate_show` implementation; 5/5 tests green
2. **Task 2: surface the rate-limit reset time in status output**
   - `c3c59d8` (test) — failing tests for `cron_hint_line`
   - `5ef4090` (feat) — `cron_hint_line` implementation; `cron_instruction_hints` refactored to use it
3. **Task 3: in-stage progress line + recovery-verb hints in `status`**
   - `224b676` (test) — failing tests for `recovery_hints`/`latest_stage_launched_ts`/`render_stage_progress_line`
   - `c6a2f82` (feat) — implementation wired into `status`; auto-fixed a `clippy::double_ended_iterator_last` lint (`.last()` → `.next_back()`) during GREEN

**Plan metadata:** this SUMMARY's own commit (docs: complete plan — worktree mode commits SUMMARY.md only; STATE.md/ROADMAP.md are updated centrally by the orchestrator after merge).

## Files Created/Modified
- `crates/devflow-cli/src/main.rs` — `GateCmd::Show { phase, stage, project }` variant + dispatch arm; `gate_show` import.
- `crates/devflow-cli/src/commands.rs` — `gate_show`, `render_gate_show`, `cron_hint_line` (refactored `cron_instruction_hints` to use it), `recovery_hints`, `latest_stage_launched_ts`, `render_stage_progress_line`; `status()` wired to the new progress line and `recovery_hints` loop; 15 new unit tests plus one existing cron-hints test updated to isolate the new reset-time fragment.

## Decisions Made
- `gate_show` copies (rather than factors out into a shared function with) `gate_respond`'s stage auto-resolve-single-open-gate match block — matches the plan's explicit "reuse verbatim" instruction and keeps the diff surgical; both blocks are now structurally identical, and a third caller would be the trigger to extract a shared helper.
- `recovery_hints` only appends the `advance` hint when `state.gate_pending` is true — deliberately narrower than "every Stuck phase," per the phase's own Review Incorporation section (a phase stuck without a pending gate still correctly gets `resume` alone; widening the predicate risked suggesting `advance` where the source path doesn't prove it's right).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `clippy::double_ended_iterator_last` on `latest_stage_launched_ts`**
- **Found during:** Task 3 GREEN, `cargo clippy -p devflow --all-targets -- -D warnings`
- **Issue:** `.filter_map(...).last()` on a `DoubleEndedIterator` needlessly walks the entire iterator instead of taking from the back.
- **Fix:** Changed `.last()` to `.next_back()` — identical semantics (last matching `stage_launched` event's `ts`), no behavior change.
- **Files modified:** `crates/devflow-cli/src/commands.rs`
- **Verification:** `cargo clippy -p devflow --all-targets -- -D warnings` clean after the fix; all `latest_stage_launched_ts` tests still pass.
- **Committed in:** `c6a2f82` (Task 3 GREEN commit)

**2. [Rule 1 - Test isolation] Updated `cron_instruction_hints_include_hermes_command_per_phase` fixture**
- **Found during:** Task 2, before GREEN — the existing test's exact-match assertion on the hermes-command hint would have started failing once `cron_hint_line` appends a reset-time fragment for a non-empty `retry_after`, since that test's fixture used a non-empty `retry_after`.
- **Issue:** Test would break due to a real, intended behavior change from this task, not a bug in the test's original intent.
- **Fix:** Changed the fixture's `retry_after` to `""` so the exact-match assertion continues to isolate the base hermes-command hint from the new reset fragment; added two new dedicated tests (`cron_hint_line_appends_sanitized_reset_when_retry_after_present` / `_omits_reset_fragment_when_retry_after_empty`) to cover the reset-time behavior directly.
- **Files modified:** `crates/devflow-cli/src/commands.rs`
- **Verification:** All three tests (`cron_instruction_hints_include_hermes_command_per_phase` plus the two new `cron_hint_line_*` tests) pass.
- **Committed in:** `c3c59d8` (Task 2 RED commit, since the test edit was needed before the new code could go green)

---

**Total deviations:** 2 auto-fixed (1 Rule 1 lint fix, 1 Rule 1 test-isolation update caused directly by this task's own intended behavior change).
**Impact on plan:** Both fixes are mechanical and directly scoped to this plan's own changes. No scope creep, no pipeline behavior change.

## Issues Encountered
None — all three tasks' `<verify>` commands passed on the first GREEN attempt (after the one clippy fix above); full workspace suite (`cargo test --workspace`) green throughout, `cargo fmt --check` clean throughout.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `devflow gate show`, the rate-limit reset hint, the in-stage progress line, and stuck-phase recovery hints are all live, tested, and manually verified end-to-end against synthetic fixtures (both the truncated-vs-untruncated gate contrast and the 90s-stage-age-vs-30m-phase-age distinction).
- No blockers for the rest of Phase 21 (21b doctor staleness reconciliation, 21c sequentagent tracking, 21d dogfood staleness, optional 21e changelog content) — this plan touched only `commands.rs`/`main.rs` display paths and introduced no schema or pipeline changes those units would need to account for.

## Self-Check: PASSED

- FOUND: `crates/devflow-cli/src/main.rs`
- FOUND: `crates/devflow-cli/src/commands.rs`
- FOUND commit `daa48fb`
- FOUND commit `7839381`
- FOUND commit `c3c59d8`
- FOUND commit `5ef4090`
- FOUND commit `224b676`
- FOUND commit `c6a2f82`

---
*Phase: 21-operator-usability-release-execution*
*Completed: 2026-07-23*
