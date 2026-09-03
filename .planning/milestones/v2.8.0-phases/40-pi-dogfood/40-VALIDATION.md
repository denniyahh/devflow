---
phase: 40
slug: pi-dogfood
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: false
created: 2026-08-19
---

# Phase 40 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` (`#[test]` attribute tests; integration tests under `crates/devflow-cli/tests/`) |
| **Config file** | `crates/devflow-cli/Cargo.toml`, `crates/devflow-core/Cargo.toml` |
| **Quick run command** | `cargo test -p devflow --test phase7_cli pi_` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~40 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p devflow --test phase7_cli pi_` (the phase's new tests)
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** ~2 seconds (targeted tests)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 40-01-01a | 01 | 1 | PIDG-01 | T-40-01 | A marker-less exit-0 Pi run must not advance a commit-gated stage | integration | `cargo test -p devflow --test phase7_cli pi_marker_less_run_does_not_advance` | ✅ | ✅ green |
| 40-01-01b | 01 | 1 | PIDG-01 | T-40-01 | A non-zero-exit Pi run must not advance its stage | integration | `cargo test -p devflow --test phase7_cli pi_nonzero_exit_does_not_advance` | ✅ | ✅ green |
| 40-01-02 | 01 | 1 | PIDG-01 | T-40-02 | A hung Pi must be surfaced alive, never silently advanced | integration | `cargo test -p devflow --test phase7_cli pi_hung_process_is_detected_not_left_running` | ✅ | ✅ green |
| 40-02-01 | 02 | 2 | PIDG-01 | — | Dogfood preconditions (pi on PATH, `@bacnh85/pi-subagent` installed) | smoke | `pi list --no-approve 2>&1 \| grep -q "@bacnh85/pi-subagent"` | ✅ | ✅ green |
| 40-02-03 | 02 | 2 | MAINT-01 | T-40-03 | `verdict: None` invariant held by the two structural defences the rewritten comments cite | unit | `cargo test -p devflow-core --lib agent_result` + `cargo test -p devflow --bin devflow pipeline_outcomes::tests::classify_validate_outcome_sweeps_all_forty_two_cells` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

*Existing infrastructure covers all phase requirements.*

The phase's new behaviour — three Pi-transport regression tests — landed green against the
pre-existing `stub-PATH` + `ENV_MUTEX` integration-test pattern (`phase7_cli.rs`). The `verdict: None`
invariant cited by the rewritten 999.85 comments is already regression-tested (`agent_result.rs`
166 tests incl. `stream_success_cannot_stand_against_nonzero_exit_code`; `pipeline_outcomes.rs`
`classify_validate_outcome_sweeps_all_forty_two_cells`). No stubs or new framework installs needed.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real supervised Define→Validate run through `--agent pi` | PIDG-01 | A dogfood outcome is the run itself — no test can stand in for the shipped driver surviving a real run | See `40-VERIFICATION.md`; the Code stage of this very execution is the run |
| Live Validate gate answered and honored | PIDG-01 | A human decision crossing into the pipeline; no automation can be the operator | `40-UAT.md` operator attestation |
| Subagent dispatch witnessed during Code | PIDG-01 | Process-level, human-observed evidence | `40-REVIEW.md` (reviewer subagent verified 5/5 claims) |
| 999.85 comment prose accuracy | MAINT-01 | No test asserts prose; comment accuracy is a correctness-critical human judgment | Reviewer subagent confirmed all claims against the classifier code; operator reviews the diff |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or a Manual-Only justification
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references — N/A, none missing
- [x] No watch-mode flags
- [x] Feedback latency < 5s (targeted tests ~2s)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-08-19
