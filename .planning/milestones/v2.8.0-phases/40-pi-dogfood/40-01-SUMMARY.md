---
phase: 40-pi-dogfood
plan: 01
subsystem: testing
tags: [pi, pi-transport, legacy-launch, regression, devflow-result-marker, monitor-liveness]

requires:
  - phase: 39
    provides: PiDriver (`pi -p --no-approve`, Legacy launch), fail-closed capability detection
  - phase: 13/17/18
    provides: generic marker/exit/liveness completion machinery (not re-proven here)

provides:
  - Three Pi-transport failure-mode regression tests in phase7_cli.rs
  - A `pi_stub` integration fixture (preflight-ready, vetted-package) + `wait_for_gate` helper

affects: [pi-transport hardening, agents/pi.rs conformance, D-05 hardening bar]

actuals:
  tokens: 7000
  tasks: 2
  commits: 1

tech-stack:
  added: []
  patterns: [stubbed-agent-binary integration test (process boundary), bounded gate-polling]

key-files:
  created: []
  modified:
    - crates/devflow-cli/tests/phase7_cli.rs

key-decisions:
  - "The marker-less failure mode is observable only at commit-gated stages (Plan/Code): Define is not commit-gated, so exit-0-without-marker is a legitimate Success there by design (evaluate_layer2 decision matrix). The test therefore asserts the gate fires at Plan, not Define."
  - "The `pi` stub answers `pi auth check` with `{\"status\":\"ready\"}` and `pi list` with the vetted `@bacnh85/pi-subagent` package so the preflight health/capability probes pass before the launch path is exercised — the same seam pi.rs's unit helpers use, across the process boundary."
  - "The hung-pi test asserts liveness (process alive + stage un-advanced) on a bounded wait and then SIGTERMs the process, observing the monitor reap it and gate (exit 143 → Failed → GateReview) — never a silent advance, and never an unbounded wait."

patterns-established:
  - "Stub `pi` binary on tempdir PATH answering `auth`/`list` subcommands, then the `-p` launch — preflight passes and the real `pi -p --no-approve` path is exercised."

requirements-completed:
  - PIDG-01

coverage:
  - id: D1
    description: "Pi marker-less run does not advance a commit-gated stage"
    requirement: PIDG-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#pi_marker_less_run_does_not_advance"
        status: pass
    human_judgment: false
  - id: D2
    description: "Pi non-zero-exit run does not advance its stage"
    requirement: PIDG-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#pi_nonzero_exit_does_not_advance"
        status: pass
    human_judgment: false
  - id: D3
    description: "Hung Pi process is surfaced alive and never silently advanced"
    requirement: PIDG-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#pi_hung_process_is_detected_not_left_running"
        status: pass
    human_judgment: false

duration: 40min
completed: 2026-08-19
status: complete
---

# Phase 40 Plan 01: Pi-Transport Regression Tests Summary

Three regression tests proving Pi's `-p`/Legacy transport feeds DevFlow's generic completion, exit, and liveness machinery — the "simulate separately" half of the hardening bar (D-05).

## Performance

- **Duration:** ~40 min (includes a full re-derivation of the marker-less stage semantics)
- **Tasks:** 2/2 complete
- **Commits:** 1

## Accomplishments

- **`pi_marker_less_run_does_not_advance`** — a stub `pi` exiting 0 with no `DEVFLOW_RESULT` marker does not advance a commit-gated stage: the run gates at Plan (never reaches Code).
- **`pi_nonzero_exit_does_not_advance`** — a stub `pi` exiting non-zero gates at Define (exit≠0 is `Failed` at every stage).
- **`pi_hung_process_is_detected_not_left_running`** — a never-exiting `pi` is surfaced alive (pid + liveness signal), the stage stays un-advanced, and once SIGTERMed the monitor reaps it and gates — never a silent advance.
- Added a `pi_stub` fixture (answers `auth`/`list` probes, then the `-p` launch) and a bounded `wait_for_gate` poller.

## Verification

- `cargo test -p devflow --test phase7_cli` → `test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
- `cargo clippy --workspace --all-targets -- -D warnings` → clean
- `cargo fmt --check` → clean

## Deviations from Plan

None — plan executed as written. One clarification surfaced during execution: the marker-less failure mode is only observable at commit-gated stages (Define is not commit-gated by design), so the test asserts the gate at `Stage::Plan` rather than `Define`. This is recorded above as a key decision, not a plan deviation.

## Self-Check: PASSED
