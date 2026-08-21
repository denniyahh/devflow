---
phase: 41-antigravity-driver
plan: 02
subsystem: testing
tags: [hygiene, monitor-reap, container-parity, worktree]

requires:
  - phase: 41-01
    provides: MonitorReapGuard (defined in 41-01 Task 8), canary-aware agy stub, the three antigravity regressions
  - phase: 31+
    provides: the phase7_cli integration suite (the monitor-spawning surface to harden)

provides:
  - HYG-01: every monitor-spawning test in phase7_cli.rs reaps its own LAST monitor; suite-level registered-PID registry + audit (codex-4); intentional opt-out control proving the gate can fail; bounded gate-wait so gated advances self-clean. Post-suite stray count: 0 (was 43 per Phase 40).
  - HYG-02: check-in-container.sh mounts the worktree gitdir + commondir; verified from BOTH the worktree and the main checkout in the pinned image.

affects: [test hygiene, container parity, CI confidence]

actuals:
  tokens: 30000
  raw_tokens: 18000
  tasks: 2
  commits: 1 (122dedc)

tech-stack:
  added: [suite registry (OnceLock<Mutex<HashSet<u32>>>), wait_for_settled, DEVFLOW_GATE_TIMEOUT_SECS bound on test children]
  patterns: [registered-PID suite audit with a provably-failing detection helper, settled-state reap binding]

key-files:
  created: []
  modified:
    - crates/devflow-cli/tests/phase7_cli.rs
    - scripts/check-in-container.sh

key-decisions:
  - "Per-test Drop guards are necessary but not sufficient (codex-4): `run_devflow_inner` now REGISTERS each spawned monitor's pid in a suite registry and `MonitorReapGuard` deregisters after its verified reap, so `suite_reap_audit`'s empty-registry assertion means 'every guard ran'. The intentional opt-out control proves the detection helper can fail against a deliberately-alive registered pid."
  - "Guards bind to the SETTLED state (`wait_for_settled`: gate_pending or stopped), not the state read right after launch. A guard bound to an early read captures an already-exited early-stage monitor and leaks the chain's LAST monitor — the one that blocks at the supervise gate. Measured: the naive binding left 5 strays; the settled binding leaves 0."
  - "Supervise-mode gates block `devflow advance` for the 3-day default gate timeout; reaping the sh monitor orphans its gated advance child (spawned as the script's last line). Bound `DEVFLOW_GATE_TIMEOUT_SECS=60` on the test child env: the tests' assertions finish far inside the window (wait_for_* cap at 10s) and the orphaned advances exit on their own instead of accumulating across runs. Post-suite count: 0 strays after ~60s."
  - "HYG-02 re-derived (round-1 finding 4, D-06): the worktree's `.git` is a FILE pointing at `<main>/.git/worktrees/<N>`, and the COMMON gitdir (`<main>/.git`, where refs/objects live) is a second path outside the mount. Both must be bind-mounted at their absolute host paths; the main checkout (gitdir inside REPO_ROOT) is untouched. Option A chosen; no test-file changes, no skip_if_root."

patterns-established:
  - "No `ps aux | grep devflow` counting anywhere — per-PID + registry verification only (T-41-HYG-04)."
  - "check-in-container.sh computes the gitdir mount set with `git rev-parse --absolute-git-dir` + `--path-format=absolute --git-common-dir` and binds only the components outside REPO_ROOT."

requirements-completed:
  - HYG-01, HYG-02

coverage:
  - id: H1
    description: "Systematic MonitorReapGuard pass — every monitor-spawning test binds the settled-state guard"
    requirement: HYG-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs (all start/parallel tests)"
        status: pass
    human_judgment: false
  - id: H2
    description: "Suite registry + suite_reap_audit + detection helper with RED/GREEN proof"
    requirement: HYG-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#suite_reap_audit / unguarded_monitor_is_detected_by_the_registry"
        status: pass
    human_judgment: false
  - id: H3
    description: "0 suite-spawned monitors alive after a default-parallel phase7_cli run"
    requirement: HYG-01
    verification:
      - kind: integration
        ref: "post-run /proc census (per-PID, no ps-grep): 0 __monitor / sh-monitor processes; orphaned gated advances self-exit within the 60s gate bound"
        status: pass
    human_judgment: false
  - id: H4
    description: "check-in-container.sh passes from a git worktree AND the main checkout"
    requirement: HYG-02
    verification:
      - kind: integration
        ref: "bash scripts/check-in-container.sh all from .worktrees/phase-41 (exit 0) and from the main checkout (exit 0), pinned image mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm"
        status: pass
    human_judgment: false

duration: 3h
completed: 2026-08-20
status: complete
---

# Phase 41 Plan 02: Dogfood Hygiene — Wave 2 Summary

Closes the two hygiene defects Phase 40's real run surfaced: leaked test
monitors (HYG-01) and in-container git failures (HYG-02).

## Performance

- **Duration:** ~3h
- **Tasks:** 2/2 complete
- **Commits:** 1 (`122dedc`)

## Accomplishments

- **HYG-01 (Task 1).** Every monitor-spawning test in phase7_cli.rs reaps its
  own last monitor through `MonitorReapGuard` bound to the settled state;
  `run_devflow_inner` registers spawned pids in a suite registry;
  `suite_reap_audit` proves the registry drains; the intentional opt-out
  control proves the detection helper can fail (codex-4). Bound the gate wait
  on test children so orphaned gated `advance` processes self-exit instead of
  accumulating. Phase 40 measured 43 leaked monitors; this suite now ends at
  0.
- **HYG-02 (Task 2).** Re-derived the worktree container failure (`.git` file
  + commondir outside the mount) and fixed the SCRIPT — not the three test
  files, not `skip_if_root()`. Verified from the worktree (exit 0) and the
  main checkout (exit 0) in the pinned image.

## Verification

- `cargo test -p devflow --test phase7_cli` → 25 passed (was 23 + 2 new).
- Full `cargo test --workspace` green; clippy clean; fmt clean.
- Post-suite process census: 0 monitor processes; gate-waiting advances exit
  within the 60s bound (verified: 0 strays after 60s).
- Container: worktree run and main-checkout run both `check.sh: all OK`.

## Deviations from Plan

1. **Gate-wait bound (additive).** The plan's design reaps monitors; the
   orphaned gated `advance` child (spawned by the legacy sh monitor as its
   final line, then orphaned by the reap) needed the 60s
   `DEVFLOW_GATE_TIMEOUT_SECS` bound on test children to fully stop
   accumulation. The gate mechanism itself is untouched and still fires.

## Self-Check: PASSED
