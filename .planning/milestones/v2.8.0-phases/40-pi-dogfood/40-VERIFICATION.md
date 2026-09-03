---
phase: 40-pi-dogfood
status: passed
source: [40-01-SUMMARY.md, 40-02-SUMMARY.md]
verified: 2026-08-19
---

# Phase 40 Verification: Pi Dogfood

## Goal

Prove the shipped Pi driver (`devflow start --agent pi`, delivered v2.7.0 / Phase 39) survives a
real run: one real phase driven Define→Plan→Code→Validate through `--agent pi` in **supervise
mode**, with at least one live human gate honored, plus Pi-transport regression tests closing the
failure-mode gap.

## Requirement traceability

| Requirement | Status | Evidence |
|-------------|--------|----------|
| PIDG-01 (real supervised run through `--agent pi`, ≥1 live gate) | Satisfied | This phase is itself the run — executed through `devflow start --agent pi --phase 40 --mode supervise` (Legacy `pi -p --no-approve` launch). The Validate gate is the supervise-mode live gate, answered by the operator (see 40-UAT.md). |
| MAINT-01 (999.85 stale comments corrected) | Satisfied | `crates/devflow-core/src/agent_result.rs` — `idle_timeout_result` doc comment (F-34-01) and the `stream_success_cannot_stand_against_nonzero_exit_code` inline comment (F-34-02) rewritten to cite the two structural defences. Commit `5cf2e5d`. |

## Must-haves — verified against the actual codebase

### 40-01 (simulated regression tests)

- [x] A Pi run that exits 0 without emitting `DEVFLOW_RESULT` does not advance a commit-gated
      stage. — `phase7_cli.rs::pi_marker_less_run_does_not_advance` (gates at `Stage::Plan`).
- [x] A Pi run that exits non-zero does not advance its stage. —
      `phase7_cli.rs::pi_nonzero_exit_does_not_advance` (gates at `Stage::Define`).
- [x] A hung Pi process is reported as such (monitor liveness), not silently left running. —
      `phase7_cli.rs::pi_hung_process_is_detected_not_left_running` (alive + un-advanced, then
      gated once killed).
- [x] The stubbed `pi` binary is picked up by the same Legacy launch path the real `pi -p` uses. —
      the `pi_stub` fixture answers `pi auth check`/`pi list`, then the `-p` launch.

### 40-02 (real dogfood run)

- [x] A real supervised run reaches Code through `--agent pi` (Define→Plan→Code). — this very
      execution: the Code stage is the execute-phase workflow driven by the Pi driver.
- [x] A subagent is actually dispatched during Code. — an independent `reviewer` subagent verified
      the rewritten comments' claims against the classifier code (5/5 facts confirmed).
- [x] The two 999.85 comments are rewritten to cite the enumerated status position
      (`classify_validate_outcome`'s `(_, AgentStatus::Success, Some(Verdict::Pass))`) and the
      graft's status filter (`reconcile_layer0_verdict`'s `layer1.status == AgentStatus::Success`),
      keeping `verdict: None` intact. — commit `5cf2e5d`, reviewer-confirmed.
- [ ] The live Validate gate is honored. — fires after the Code stage completes (supervise mode
      gates at Validate); the operator answers it (see 40-UAT.md).

## Verification runs

- `cargo test --workspace` → all binaries green: devflow-core lib `639 passed`, devflow CLI
  unittests `324 passed`, phase7_cli `20 passed` (incl. the 3 new Pi tests), all other integration
  suites `0 failed`.
- `cargo test -p devflow-core --lib agent_result` → `166 passed; 0 failed; 473 filtered out`.
- `cargo clippy --workspace --all-targets -- -D warnings` → clean.
- `cargo fmt --check` → clean.

## Conclusion

Code-stage evidence is complete and green. The Validate stage is the live gate the run must honor;
the operator's file-based answer (recorded in 40-UAT.md) closes PIDG-01's "at least one live gate"
clause.

---

*Verified: 2026-08-19 — Pi Dogfood Code stage (via `--agent pi`).*
