---
phase: 18-dogfood-reliability-hardening
plan: 01
subsystem: cli
tags: [rust, cli, diagnostics, state-machine, doctor]

# Dependency graph
requires:
  - phase: 17-pipeline-dogfood-followup
    provides: typed AgentResult outcomes (17b) and build provenance (17d) that doctor's reconciliation reads facts alongside
provides:
  - "devflow doctor project-aware reconciliation: PhaseFacts/PhaseFinding/Severity/reconcile_phase pure core plus collect_phase_facts/render_reconciliation wiring"
  - "five named reconciliation checks: gate_pending-without-open-gate, orphan-open-gate, dead-agent-at-agent-stage, stage/event drift, missing-feature-branch"
  - "text and --json doctor output sections proven read-only by a twice-run fixture test"
affects: [18b-monitor-liveness, doctor, status]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "pure predicate over a plain-data facts struct (PhaseFacts -> Vec<PhaseFinding>), with all I/O isolated to a separate collect_* function — mirrors mode.rs's should_gate/should_auto_loop shape"
    - "five small named check_* helpers each returning Option<PhaseFinding>, composed via an array + flatten in a fixed order, keeping reconcile_phase under the ~40-line ceiling"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs

key-decisions:
  - "Two-commit split (Task 1 pure core, Task 2 wiring) required staging with #[allow(dead_code)] on Task 1's new items, removed in Task 2 — see Deviations"
  - "last_event surfaced in --json output (per-finding, sourced from the originating PhaseFacts) rather than left write-only, giving it a genuine non-test consumer"
  - "Doctor's per-phase idle-project and gate-pending tests assert against collect_phase_facts + render_reconciliation_text directly rather than capturing doctor()'s stdout — this repo has no stdout-capture dependency and the phase adds none; the read-only contract test (doctor_is_read_only_on_a_mismatched_project) still invokes doctor() for real"

requirements-completed: [18a]

coverage:
  - id: D1
    description: "devflow doctor prints a per-phase reconciliation section diffing state.stage, the latest events.jsonl event, the agent PID's liveness, open gates, and feature-branch existence, naming a repair command per disagreement"
    requirement: "18a"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::doctor_reconciliation::reconcile_phase_flags_gate_pending_without_open_gate"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::doctor_reconciliation::reconcile_phase_flags_orphan_open_gate"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::doctor_reconciliation::reconcile_phase_flags_dead_agent_at_agent_stage"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::doctor_reconciliation::reconcile_phase_flags_stage_event_drift"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::doctor_reconciliation::reconcile_phase_flags_missing_feature_branch"
        status: pass
      - kind: manual_procedural
        ref: "cargo run -p devflow -- doctor"
        status: pass
    human_judgment: false
  - id: D2
    description: "devflow doctor mutates nothing — a run that finds mismatches writes no state file, appends no event, creates or deletes no gate file"
    requirement: "18a"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::doctor_reconciliation::doctor_is_read_only_on_a_mismatched_project"
        status: pass
    human_judgment: false
  - id: D3
    description: "doctor reports agreement (zero findings), not a false mismatch, when state.stage exactly equals the last stage_launched event's stage"
    requirement: "18a"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::doctor_reconciliation::reconcile_phase_returns_no_findings_when_all_agree"
        status: pass
    human_judgment: false
  - id: D4
    description: "devflow doctor with zero active phases prints an explicit 'no active phases' line and exits 0"
    requirement: "18a"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::doctor_reconciliation::doctor_reports_no_active_phases_when_idle"
        status: pass
    human_judgment: false
  - id: D5
    description: "reconciliation findings for a phase are emitted in a fixed order, independent of the order underlying facts were collected"
    requirement: "18a"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::doctor_reconciliation::reconcile_phase_ordering_is_input_order_independent"
        status: pass
    human_judgment: false

duration: ~35min
completed: 2026-07-20
status: complete
---

# Phase 18 Plan 01: Doctor Project-Aware Reconciliation Summary

**`devflow doctor` now diffs per-phase state, events.jsonl, agent-PID liveness, open gates, and feature-branch existence into named, repair-command-bearing findings, proven read-only by a twice-run fixture test.**

## Performance

- **Duration:** ~35 min
- **Completed:** 2026-07-20T23:17:56-04:00
- **Tasks:** 2
- **Files modified:** 1 (`crates/devflow-cli/src/main.rs`, +563/-1 net across both commits)

## Accomplishments

- Pure reconciliation core: `Severity` (Ok/Warn/Problem), `PhaseFacts`, `PhaseFinding`, `reconcile_phase`, and five named checks (`check_gate_pending_without_gate`, `check_orphan_gate`, `check_dead_agent`, `check_stage_event_drift`, `check_missing_branch`) — no I/O, no filesystem paths in output (WR-02 leak class avoided per the plan's threat model)
- `doctor()`'s previously-unused `_project_root` parameter is now bound and drives `collect_phase_facts`/`build_phase_facts`, composing only existing read-only primitives (`workflow::list_states`, `events::last_events_by_phase`, `agent_pid_from_file` + `agent::agent_running`, `Gates::list_open`, `run_git_stdout` for `refs/heads/feature/phase-{NN}`)
- Text (`reconciliation:` header, `no active phases — nothing to reconcile`, per-phase `phase N: <detail>` + `repair:` lines, `phase N: ok`) and `--json` (phase/severity/detail/repair/last_event array) renderers, both driven by the same `findings_for_display` helper
- 10 new unit tests in `tests::doctor_reconciliation` — 7 for the pure predicate (one per check plus the fixed-order test) and 3 for the wiring (idle project, gate-pending-without-gate-file, and a twice-run read-only proof asserting state-file size/mtime and `events.jsonl` line count are byte-identical across both runs)
- Manual smoke verified: `cargo run -p devflow -- doctor` in this repo prints a `reconciliation:` section ending in `no active phases — nothing to reconcile`

## Task Commits

1. **Task 1: Add the pure phase-reconciliation core** - `8fdbd8a` (feat)
2. **Task 2: Wire reconciliation into doctor's text and JSON output, read-only** - `3ce77a1` (feat)

**Plan metadata:** (this commit, once created below)

## Files Created/Modified

- `crates/devflow-cli/src/main.rs` - `doctor()` bound to `project_root`; new `Severity`/`PhaseFacts`/`PhaseFinding`/`reconcile_phase`/five check helpers/`collect_phase_facts`/`build_phase_facts`/`last_launched_stage_from_event`/`findings_for_display`/`render_reconciliation`(+text/json)/10 new tests under `tests::doctor_reconciliation`

## Decisions Made

- **Two-commit split staging:** Task 1's pure core (types + `reconcile_phase` + 5 checks) is, by design, not called from any non-test code until Task 2 wires it into `doctor()`. Because `crates/devflow-cli` is a binary-only crate (no `[lib]` target), `cargo clippy --workspace --all-targets -- -D warnings` compiles the plain `bin` target *without* the `#[cfg(test)]` module — so unit-test usage alone does not satisfy that build's dead-code analysis. Resolved by adding `#[allow(dead_code)]` to Task 1's not-yet-wired items with a comment naming the exact commit that removes them, then removing all of them in Task 2's commit once `doctor()` is the real caller. Verified: `cargo clippy --workspace --all-targets -- -D warnings` was run and confirmed clean after *each* commit individually (not just at the end).
- **`cargo test -p devflow --lib <name>` (as written in 18-01-PLAN.md's own `<verify>`/`<acceptance_criteria>` blocks and 18-RESEARCH.md) does not work on this crate** — `devflow` (the `devflow-cli` package) has no library target, so `--lib` hard-errors with `error: no library targets found in package \`devflow\`` (exit 101, not a test failure). Verification throughout used the working equivalent, `cargo test -p devflow <name>`, which runs the same `unittests src/main.rs` binary. This matches the project's existing memory note that `cargo test --exact` gotchas exist for this package; the `--lib` flag is a second, related gotcha now also confirmed and documented here.
- **`last_event` field surfaced via `--json` output** rather than left as a write-only field: `render_reconciliation_json` now pairs each finding with its originating phase's `last_event`, giving the plan-mandated `PhaseFacts.last_event` field a genuine production consumer (not just test-cfg construction) — consistent with the "no speculative/unused code" constraint in the user's global coding-style rules.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Plan's own verify commands (`cargo test -p devflow --lib ...`) don't work on this crate**
- **Found during:** Task 1 verification
- **Issue:** `devflow-cli`'s package (`devflow`) is a binary-only crate — `cargo test -p devflow --lib <filter>` hard-errors (`no library targets found`, exit 101) rather than running the unit tests. This is the exact command given in both 18-01-PLAN.md's `<verify>`/`<acceptance_criteria>` blocks and 18-RESEARCH.md's Validation Architecture table.
- **Fix:** Ran the working equivalent (`cargo test -p devflow <filter>`, no `--lib`) for every verification step in this plan; confirmed test counts (`7 passed` for Task 1, `10 passed` for Task 2) via that invocation.
- **Files modified:** None (verification-only; no source change required).
- **Verification:** `cargo test -p devflow doctor_reconciliation` → `10 passed; 0 failed`.
- **Committed in:** N/A (verification-only deviation, not a code change).

**2. [Rule 3 - Blocking] Task 1's pure core is unavoidably "dead" until Task 2 wires it in, under this crate's strict clippy gate**
- **Found during:** Task 1 verification (`cargo clippy --workspace --all-targets -- -D warnings` failed with 10 `never constructed`/`never used` errors on the plain `bin` target, which excludes `#[cfg(test)]`)
- **Issue:** The plan's two-task split (pure core in Task 1, `doctor()` wiring in Task 2) is architecturally sound but not literally clippy-clean at the Task 1 checkpoint on a binary-only crate, since `--all-targets` independently compiles the plain bin (no test cfg, so unit-test-only usage doesn't count) and the test bin.
- **Fix:** Added `#[allow(dead_code)]` to each not-yet-wired item in Task 1 (`Severity`, `label()`, `PhaseFacts`, `PhaseFinding`, and the 5 check functions — `reconcile_phase` itself did not need one, since nothing calls it in Task 1 either, but marking every item it depends on transitively was unnecessary once each function was individually annotated), with a doc comment stating this is temporary staging removed by Task 2. Removed every one of these attributes in Task 2's commit once `doctor()`/`collect_phase_facts`/`render_reconciliation` became real callers.
- **Files modified:** `crates/devflow-cli/src/main.rs` (both commits).
- **Verification:** `cargo clippy --workspace --all-targets -- -D warnings` exits 0 after Task 1's commit AND after Task 2's commit (checked independently via `git stash`).
- **Committed in:** `8fdbd8a` (added), `3ce77a1` (removed).

---

**Total deviations:** 2 auto-fixed (both Rule 3 — blocking verification/build issues, no scope creep; no source-level behavior changed beyond what the plan specified).
**Impact on plan:** Both deviations are process/verification-command corrections, not functional changes. The shipped `doctor` behavior matches every `<behavior>` bullet in 18-01-PLAN.md exactly.

## Issues Encountered

None beyond the two auto-fixed deviations above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 18a (`devflow doctor` reconciliation) is complete and ready for 18b (monitor liveness, `18-03-PLAN.md`) to extend: `PhaseFacts`, `collect_phase_facts`, and `reconcile_phase` are the shape 18b's monitor-pid probe should slot into, per 18-RESEARCH.md's sequencing note.
- `cargo test --workspace` is green at 394 passed / 0 failed (verified baseline at this plan's start commit, `8d67d1e`: 384 passed — this plan adds exactly the 10 new tests, 7 from Task 1 + 3 from Task 2, confirmed by diffing `devflow-cli`'s unit-test count 71→81 across both commits). `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` both exit 0. One `phase7_cli.rs` test (`parallel_creates_two_worktrees_and_spawns_two_monitors`, the exact test 18-02/18g targets) was independently observed to flake once against the pre-this-plan baseline during verification — confirmed unrelated to this plan (it doesn't touch `phase7_cli.rs`) and matches the known pattern documented in this phase's own CONTEXT.md/RESEARCH.md.
- No blockers for 18-02 (WR-03 test stabilization, wave 1, independent of this plan) or subsequent waves.

---
*Phase: 18-dogfood-reliability-hardening*
*Completed: 2026-07-20*

## Self-Check: PASSED

- FOUND: crates/devflow-cli/src/main.rs
- FOUND: 8fdbd8a (Task 1 commit)
- FOUND: 3ce77a1 (Task 2 commit)
