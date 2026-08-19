# 40-01 Summary: Pi-Transport Regression Tests

**Plan:** 40-01
**Status:** complete (Task 1) — Task 2 descoped
**Requirements:** PIDG-01

## What was done

Added two integration regression tests proving Pi's `-p`/Legacy transport feeds DevFlow's generic
completion/exit machinery correctly — the "simulate separately" half of the hardening bar (D-05):

- `pi_markerless_exit_does_not_advance` — a `pi` that exits 0 without a `DEVFLOW_RESULT` marker never
  advances past the agent stages to Validate/Ship.
- `pi_nonzero_exit_does_not_advance` — a `pi` that exits non-zero never advances past the agent stages.

Both drive the real `devflow start --agent pi` binary against a stubbed `pi` on PATH (the existing
`fake_bin_dir` + `spawn_devflow` pattern), answer Pi's Define gate, and assert the run does not reach
Validate/Ship.

## Finding surfaced

`devflow start --agent pi --mode supervise` gates at **Define** — the 35.1-03 unattended-launch
prerequisite check requires "Code would launch on the pipe-owning arm", which is structurally false
for Pi (Legacy launch). Confirmed **expected** by the operator; the gate is approved to proceed.
This is also the live gate 40-02 will surface.

## Task 2 (hung detection) — descoped

The hung-agent test was attempted and removed. The underlying liveness detection (a hung process is
not silently advanced) is **already regression-tested generically** (Phase 18 `liveness()` +
dead-monitor fixture). A full-pipeline hung-agent integration test is not cleanly writable: the
monitor legitimately blocks on the agent's exit, and the subsequent marker-less re-gate requires a
second operator response, leaving orphaned processes. The Pi-specific delta (agent-pid recording on
Legacy launch) is exercised by these two tests' monitor spawns. Recorded rather than silently
dropped.

## Verification

- `cargo test -p devflow --test phase7_cli pi_` — 2 passed, 0 failed.
- `cargo clippy -p devflow --test phase7_cli` — clean.
- `cargo fmt --check` — clean.
