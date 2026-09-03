---
phase: 44
slug: codex-end-to-end-verification
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-26
---

# Phase 44 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `cargo test` (no external test framework) |
| **Config file** | none — workspace `Cargo.toml` at repo root |
| **Quick run command** | `cargo test -p devflow --bin devflow <filter>` / `cargo test -p devflow-core --lib <filter>` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~90 seconds (existing workspace baseline) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p devflow --bin devflow <new/changed test names>` (or `-p devflow-core --lib` for core-crate tests)
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd-verify-work`:** Full suite must be green, and `cargo run -q -p devflow -- --help` diffed against the regenerated snapshot
- **Max feedback latency:** 90 seconds

---

## Per-Task Verification Map

> Reconstructed by `/gsd-validate-phase 44` on 2026-09-03 from the 5 shipped plans
> (44-00…44-04), their SUMMARYs, and `44-VERIFICATION.md` (8/8 truths, independently
> re-derived). The `44-XX-XX TBD` skeleton rows below were never reconciled during
> execution (#2117 NOT-VALIDATED). All commands re-run green against the post-sync tree.

| Task / Plan | Requirement | Behavior | Test Type | Automated Command | Status |
|-------------|-------------|----------|-----------|-------------------|--------|
| 44-04 (dogfood) | CODE-01 | A real Codex-driven phase reaches a clean finish OR surfaces re-filed gaps | manual + capture | N/A — evidence in `44-CODEX-E2E.md` + `44-evidence/` (21 real `--json` stream files; commits `154162c`/`557877c` resolve as git objects) | ✅ recorded (manual-only) |
| 44-04 (parity) | CODE-01 / D-04 | Codex argv/env/interactivity unchanged | unit | `cargo test -p devflow-core --lib agents::tests::codex` + `git diff origin/develop -- crates/devflow-core/src/agents/codex.rs` | ✅ 5 passed / 759 filtered; diff = 0 lines |
| 44-01 | #147 / D-05..D-08 | `resume --agent` mutates only agent, runs full pre-mutation preflight, re-marks `stopped` on launch failure | unit | `cargo test -p devflow --bin devflow resume_with_agent` (+ `resume_re_marks_stopped_when_launch_stage_fails_outright`, `resume_with_agent_refuses_auto_mode_handoff_that_would_fail_the_later_unattended_launch_check`) | ✅ 6 + 1 + 1 passed / 357–362 filtered |
| 44-03 | #148 / D-10..D-14 | `--from-devflow` string gone; cron hint uses real flags; shell-quote round-trips | unit | `cargo test -p devflow --bin devflow cron_hint_line_command_quoting_roundtrips_through_shell_for_space_and_apostrophe_paths` + `cargo test -p devflow --test phase7_cli status_prints_cron_hint_when_cron_instructions_exist` | ✅ 1 + 1 passed |
| 44-03 | #148 / D-12..D-13 | ISO-8601 UTC schedule; unparseable retry time fails closed | unit | `cargo test -p devflow-core --lib hermes_schedule` + `cargo test -p devflow-core --lib cron_instructions_reject_unparseable_retry_time` | ✅ 3 + 1 passed / 761–763 filtered |
| 44-00 / 44-02 | #153 / D-15..D-18 | Cron record consumed only on genuine relaunch; survives failed launch; deletion is audit-safe (TOCTOU-hardened) | unit + integration | `cargo test -p devflow-core --lib consume_cron_instructions` (incl. `consume_cron_instructions_tolerates_a_racing_concurrent_consumer`) | ✅ 6 passed / 758 filtered |
| 44-REVIEW CR-01 | Phase SC3 | `pre-push` fail-closed diagnostic reachable under `set -e` | integration | `cargo test -p devflow --test pre_push_signing_policy` | ✅ 8 passed |
| — | Phase SC3 | No regression to Codex driver / workspace | full suite | `cargo test --workspace` | ✅ 1235 passed / 0 failed (2026-09-03, exit 0) |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

## Wave 0 Requirements

- [x] `pipeline_launch.rs` — handoff mutation tests (D-06/D-07/D-08 positive + negative)
- [x] `commands.rs` — rewritten cron-hint tests + `--from-devflow`-absence negative control
- [x] `ship.rs` — ISO schedule render + round-trip tests, negative control for the old UTC-cron-field approach
- [x] `pipeline_launch.rs` / `pipeline_gate.rs` — cron-deletion trigger tests (success deletes, failure preserves, ship-completion belt-and-braces)
- [x] `crates/devflow-cli/tests/snapshots/devflow-help.txt` — regenerated after the `--agent` flag lands on `Resume`
- [x] Codex dogfood evidence capture directory (per 13-06/34-evidence precedent): commands run, `--json` capture, PR link, failure classification if any gap surfaces

*(Framework itself needs no install — `cargo test` is already the project's only test runner.)*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| A real phase completes through `--agent codex`, or the run surfaces concrete, re-filed gaps | CODE-01 | Inherently a live dogfood run against a real Codex CLI session, not something a unit test can substitute for | Run a real phase through `devflow start --agent codex` (or `resume --agent codex`), capture commands run, verbatim Codex `--json` output, PR link, and a DevFlow-vs-external-workflow failure classification for anything that goes wrong — per the 13-06 dogfood evidence precedent |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 90s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated 2026-09-03 (`/gsd-validate-phase 44`) — Nyquist-compliant, 0 gaps

---

## Validation Audit 2026-09-03

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |

State A audit against a `draft` skeleton that execution never reconciled (#2117
NOT-VALIDATED — all rows were `44-XX-XX TBD`). Per-Task Map reconstructed from the 5
shipped plans + `44-VERIFICATION.md`. CODE-01's automated portion (Codex driver parity
D-04 + `cargo test --workspace` no-regression) is green; the dogfood portion is an
inherently-manual live run, recorded in `44-CODEX-E2E.md` and `44-evidence/`. Every
surfaced-gap fix (#147 / #148 / #153 / CR-01) has a named regression test, all re-run
green with non-zero `filtered out`. No auditor spawn required (0 MISSING).
