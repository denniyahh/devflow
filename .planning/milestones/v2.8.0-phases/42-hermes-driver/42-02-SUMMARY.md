---
phase: 42-hermes-driver
plan: 02
subsystem: agents
tags: [antigravity, dogfood, idle-timeout, cadence, preflight, unattended-launch, c2-gate]

requires:
  - phase: 41
    provides: AntigravityDriver, stream-json transport, per-agent idle timeout, C2 preflight refusal for the undogfooded driver (F5/D-04)
  - phase: 42/01
    provides: HermesDriver (used as part of the phase's own delivery, not by this plan directly)

provides:
  - Real supervised Antigravity dogfood run (this phase itself, executed via `devflow start --agent antigravity --mode supervise`) closing the 41-UAT deferred >5m negative control on `--print-timeout 60m`
  - Measured quiet-gap cadence during `cargo test --workspace` (~163s) against the 120s idle-timeout floor, with the remediation (`DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS=300`) documented
  - `--mode auto` unlocked for Antigravity in `preflight.rs`'s C2 gate (D-07)

affects: [unattended-launch policy, Antigravity C2 gate, idle-timeout operator guidance]

key-files:
  modified:
    - crates/devflow-cli/src/preflight.rs
  created:
    - .planning/phases/42-hermes-driver/42-VERIFICATION.md
    - .planning/phases/42-hermes-driver/42-UAT.md

key-decisions:
  - "D-07: Phase 42 is itself the supervised Antigravity dogfood run that graduates the driver to unattended-eligible — the phase's own execution is the evidence, not a separate synthetic exercise."
  - "The 120s idle-timeout floor (`DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS`) is too tight for `cargo test --workspace` quiet gaps (~163s observed) — two watchdog kills fired during the initial validation run (`.devflow/phase-42-monitor.log`). Documented as an operator-facing floor-vs-workload gap, not a defect: `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS=300` accommodates it. The unlock in preflight.rs does not raise the default floor — it only widens the C2 agent-match condition."
  - "unattended_launch_shape_condition widened from `state.agent == AgentKind::Claude` to `state.agent == AgentKind::Claude || state.agent == AgentKind::Antigravity` — Antigravity now joins Claude on the stream-json unattended launch path. This replaces the round-3 dogfood refusal (F5/D-04, phase 41) rather than adding a separate flag, as phase 41's doc comment had anticipated ('Antigravity joins by replacing state.agent == AgentKind::Claude with an explicit dogfood flag') — the actual implementation used a direct agent-match OR rather than a distinct dogfood-flag field."

patterns-established:
  - "A phase's own supervised execution under the driver being graduated is the dogfood evidence for that driver's unattended-mode unlock (same pattern as Phase 40 for Pi)."

requirements-completed:
  - ANTG-04

coverage:
  - id: T1
    description: "Dogfood preconditions — agy/hermes on PATH, devflow doctor green"
    requirement: ANTG-04
    verification:
      - kind: manual
        ref: "42-VERIFICATION.md Execution Summary"
        status: pass
    human_judgment: true
  - id: T2
    description: "Supervised Antigravity dogfood run through Validate; quiet-gap cadence measured; 60m print-timeout held; live gate honored"
    requirement: ANTG-04
    verification:
      - kind: manual
        ref: "42-VERIFICATION.md Dogfood Cadence & Quiet-Gap Measurement; 42-UAT.md test 5"
        status: pass
    human_judgment: true
  - id: T3
    description: "Antigravity unlocked for --mode auto in preflight.rs C2 gate"
    requirement: ANTG-04
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#unattended_launch_shape_condition_antigravity_allowed"
        status: pass
    human_judgment: false

duration: ~4h (17:30 first verification/UAT draft to 21:30 ship, including one adversarial review round and remediation)
completed: 2026-08-21
status: complete
---

# Phase 42 Plan 02: Antigravity Dogfood & Unattended-Mode Unlock — Summary

**Backfilled 2026-08-23** from the merged artifacts (PLAN.md, VERIFICATION.md,
UAT.md, REVIEW.md, ADVERSARIAL-REVIEW.md, and the git history on
`feature/phase-42`) — the executor's own SUMMARY.md was never written for
this plan.

Executes the supervised Antigravity dogfood run that phase 41 deferred
(F5/D-04): the pipeline itself was run under `--agent antigravity --mode
supervise`, cadence was measured against the 120s idle-timeout floor, and
the C2 preflight gate was widened to permit `--mode auto` for Antigravity.

## Accomplishments

- **Dogfood execution**: Phase 42's own pipeline ran under Antigravity in
  supervised mode through Define→Plan→Code→Validate. Stream events fired
  regularly during tool dispatch, file reads, and shell execs; `cargo test
  --workspace` produced quiet gaps of ~163s, exceeding the 120s default idle
  floor and triggering two watchdog terminations during the initial
  validation pass (`.devflow/phase-42-monitor.log`).
  `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS=300` was set to accommodate
  workspace-suite runs; the `--print-timeout 60m` override held continuously
  across long tool turns, closing 41-UAT's deferred >5m negative control.
- **C2 unlock** (`preflight.rs`): `unattended_launch_shape_condition` widened
  from a Claude-only agent match to `Claude || Antigravity`; refusal-cause
  message updated to name both agents (`"the agent is {agent}, not claude or
  antigravity"`); doc comment rewritten from the phase-41 refusal rationale
  to the phase-42 unlock rationale. Test
  `unattended_launch_shape_condition_antigravity_refused` (phase 41) was
  replaced with `unattended_launch_shape_condition_antigravity_allowed`
  (asserts `ConditionState::Holds`); a new
  `unattended_launch_shape_condition_non_stream_agent_refused` (using
  `AgentKind::Pi`) replaces the old refusal test to keep a refusal case
  covered.
- **Evidence docs**: `42-VERIFICATION.md` and `42-UAT.md` record the
  execution summary, cadence measurement, and the full automated-test table
  (7 suites, all passing, workspace suite >1,000 tests green).

## Verification

- `cargo test -p devflow --bin devflow unattended_launch_shape_condition_antigravity` → 1 passed.
- `cargo test --workspace` → all green (REVIEW.md records 685 tests at review
  time; 42-VERIFICATION.md's final table records >1,000 — the workspace grew
  between the two checks in the same session).
- 42-UAT.md: 7/7 tests passed, 0 issues.

## Deviations from Plan / Self-Correction

1. **First-draft VERIFICATION.md/UAT.md (`b5729ac`) were written ahead of the
   code they described.** They were committed 5 minutes after the
   implementation commit (`6b67cb1`) and already claimed the preflight C2
   unlock as passing, but `preflight.rs` was not actually touched until the
   later remediation commit (`759a9cd`, ~4h afterward). The adversarial
   review (Angle 1, CR-01) independently caught a related discrepancy — the
   docs claimed zero idle timeouts while the monitor log showed two — and
   the same remediation commit corrected both the code and the docs together
   before shipping. Net effect on the shipped state is nil (docs and code
   agree as of `759a9cd`), but the interim commit briefly had verification
   evidence describing code that didn't exist yet on disk.
2. **Task 3's C2 change was implemented as a direct agent-match OR, not the
   "explicit dogfood flag" phase 41's doc comment anticipated.** Functionally
   equivalent for the two agents that exist today; noted because a future
   third stream-json driver will need the same one-line widening rather than
   inheriting a flag.

## Self-Check: PASSED (per 42-VERIFICATION.md / 42-UAT.md, both `status:
passed`; 42-REVIEW.md final status `clean`, 0 critical / 0 warning / 6 info;
shipped as PR #137)
