---
phase: 42-hermes-driver
plan: 01
subsystem: agents
tags: [hermes, headless-oneshot, process-exit, conformance, doctor, transport]

requires:
  - phase: 40/41
    provides: modular AgentDriver contract, shared conformance suite, MonitorReapGuard, render_claude_style

provides:
  - HermesDriver end-to-end: `-z <prompt> --yolo --accept-hooks` argv, `HERMES_ACCEPT_HOOKS=1` child-scoped env, claude-style prompt rendering, dynamic `hermes tools list` delegation probe, presence-only health check
  - AgentKind::Hermes registered (Display/FromStr/serde), driver_for wired, conformance enrollment 5→6 drivers
  - devflow doctor hermes presence check
  - Transport regressions: marker-less, non-zero exit, and hung-process handling on the legacy (non-stream) subprocess arm

affects: [agent driver registry, doctor surface, conformance suite]

key-files:
  created:
    - crates/devflow-core/src/agents/hermes.rs
  modified:
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/agents/mod.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/phase7_cli.rs

key-decisions:
  - "D-01: headless oneshot launch — build_command returns (\"hermes\", [\"-z\", prompt, \"--yolo\", \"--accept-hooks\"]), environment() sets HERMES_ACCEPT_HOOKS=1 via .envs() (child-scoped, parent environment untouched)."
  - "D-02: render_prompt delegates to crate::prompt::render_claude_style(intent) — no Hermes-specific prompt formatting."
  - "D-03: completion is process-exit + DEVFLOW_RESULT marker contract (the legacy/non-stream arm), not a stream-json parser — Hermes runs on the same subprocess wait-and-parse path as Codex/OpenCode/Pi, not the stream transport built for Claude/Antigravity."
  - "D-04: capabilities() probes `hermes tools list` dynamically via hermes_subagent_dispatch_available_with(...) rather than a static flag, fails closed to false on any probe error."
  - "D-05/D-06: AgentKind::Hermes is a full enum citizen (Display/FromStr/serde) and enrolled in the shared 7-check conformance suite (hermes_conformance_enrollment), matching the pattern every prior driver used."

patterns-established:
  - "hermes_stub(launch) fixture in phase7_cli.rs answers both `tools list` (delegation probe) and stage-run launches, mirroring the pi/opencode stub shape for legacy-arm agents."
  - "Delegation substring parser (parse_hermes_tools_list_for_delegation) initially matched on `contains(\"delegation\") && contains(\"enabled\")` alone — the adversarial review round in 42-02 found this false-positives on lines like `✗ delegation (not enabled)`; fixed same-day in 759a9cd with explicit `disabled`/`not enabled` exclusion guards and negative-control tests."

requirements-completed:
  - HRMS-01
  - HRMS-02
  - HRMS-03

coverage:
  - id: T1
    description: "HermesDriver implementation — argv shape, environment, prompt rendering, dynamic delegation probe"
    requirement: HRMS-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/hermes.rs#hermes_driver_* / parse_hermes_tools_list_* / hermes_subagent_dispatch_with_mock"
        status: pass
    human_judgment: false
  - id: T2
    description: "AgentKind::Hermes registration, driver_for dispatch, 6-driver conformance enrollment"
    requirement: HRMS-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#agent_kind_hermes_* (5); crates/devflow-core/src/agents/mod.rs#hermes_conformance_enrollment"
        status: pass
    human_judgment: false
  - id: T3
    description: "devflow doctor hermes presence probe"
    requirement: HRMS-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#doctor_includes_hermes_check_in_the_seam"
        status: pass
    human_judgment: false
  - id: T4
    description: "Transport integration regressions on the legacy subprocess arm — marker-less, non-zero exit, hung process"
    requirement: HRMS-03
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#hermes_marker_less_run_does_not_advance / hermes_nonzero_exit_does_not_advance / hermes_hung_process_is_detected_not_left_running"
        status: pass
    human_judgment: false

duration: ~1h (17:25 implementation commit, following a 16:30 context session)
completed: 2026-08-21
status: complete
---

# Phase 42 Plan 01: Hermes Driver — Summary

**Backfilled 2026-08-23** from the merged artifacts (PLAN.md, VERIFICATION.md,
UAT.md, REVIEW.md, ADVERSARIAL-REVIEW.md, and the git history on
`feature/phase-42`) — the executor's own SUMMARY.md was never written for
this plan; commits `6b67cb1`/`759a9cd` and the docs in this directory are the
source of truth this file reconstructs from, not a live session transcript.

Implements `HermesDriver` (`AgentKind::Hermes`) as a headless-oneshot,
process-exit-completion driver on the legacy subprocess arm, registers it
across `devflow-core`/`devflow-cli`, and enrolls it in the shared conformance
suite. All 4 tasks landed in a single commit; a delegation-parser false
positive (Task 1) was found by the round of adversarial review 42-02 ran and
fixed same-day.

## Accomplishments

- **Driver** (`hermes.rs`, new file): `HermesDriver` implementing
  `AgentDriver` — `name()` → `"Hermes"`; `build_command()` → `("hermes",
  ["-z", prompt, "--yolo", "--accept-hooks"])`; `environment()` →
  `HERMES_ACCEPT_HOOKS=1`; `render_prompt()` delegates to
  `render_claude_style`; `capabilities()` probes `hermes tools list` via
  `hermes_subagent_dispatch_available_with`; presence-only `health()`.
- **Registration** (`state.rs`): `AgentKind::Hermes` variant, `Display`/
  `FromStr` (case-insensitive), serde round-trip, updated `AgentParseError`
  message listing all 6 agents.
- **Dispatch & conformance** (`agents/mod.rs`): `pub mod hermes;` /
  `pub use hermes::HermesDriver;`, `driver_for(AgentKind::Hermes)`, the
  hardcoded conformance array widened 5→6, and a uniquely-named
  `hermes_conformance_enrollment` test asserting Hermes passes all 7 contract
  checks.
- **Doctor** (`commands.rs`): `"hermes"` `cmd_check` entry in
  `doctor_checks()` probing `hermes --version`.
- **Transport regressions** (`phase7_cli.rs`): `hermes_stub(launch)` fixture
  (answers `tools list` and stage-run invocations);
  `hermes_marker_less_run_does_not_advance`,
  `hermes_nonzero_exit_does_not_advance`, and
  `hermes_hung_process_is_detected_not_left_running` — the last proving PID
  tracking and gate hold on external kill, not an autonomous idle-timeout
  watchdog (that mechanism is stream-json-only; Hermes runs on the legacy
  wait-on-exit arm, per REVIEW.md WR-02).

## Verification

- `cargo test -p devflow-core --lib hermes` → 14 passed.
- `cargo test -p devflow-core --lib agent_kind_hermes` → 5 passed.
- `cargo test -p devflow-core --lib hermes_conformance_enrollment` → 1 passed
  (6 drivers, 7 contract checks each).
- `cargo test -p devflow --bin devflow doctor_includes_hermes` → 1 passed
  (actual test name: `doctor_includes_hermes_check_in_the_seam` — the plan's
  artifact list named it `doctor_includes_hermes`; REVIEW.md IN-06 notes the
  mismatch as non-blocking since substring filters still match).
- `cargo test -p devflow --test phase7_cli hermes` → 3 passed.
- Full workspace suite green (`cargo test --workspace`).

## Deviations from Plan

1. **Delegation parser false positive, fixed post-review.** The initial
   `parse_hermes_tools_list_for_delegation` matched on
   `contains("delegation") && contains("enabled")` alone, which a Codex-led
   adversarial review round (run as part of 42-02) found true-positives on
   lines like `✗ disabled delegation (can be enabled in config)`. Fixed in
   `759a9cd` with explicit `!contains("disabled") && !contains("not
   enabled")` guards plus two negative-control tests. Flagged in this plan's
   scope because the defect was in Task 1's code, even though the review and
   fix commit are dated after 42-02's dogfood run.
2. **Doctor test name diverges from the plan's artifact list** (`
   doctor_includes_hermes_check_in_the_seam` vs. the plan's
   `doctor_includes_hermes`) — non-blocking, noted above.

## Self-Check: PASSED (per 42-VERIFICATION.md / 42-UAT.md, both `status:
passed`; 42-REVIEW.md final status `clean`, 0 critical / 0 warning / 6 info)
