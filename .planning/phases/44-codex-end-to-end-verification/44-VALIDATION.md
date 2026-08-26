---
phase: 44
slug: codex-end-to-end-verification
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
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

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 44-XX-XX | TBD | TBD | CODE-01 (dogfood evidence) | — | A real Codex-driven phase reaches Ship or surfaces a re-filed gap | manual + capture | N/A — evidence captured per 13-06 precedent (commands, `--json` capture, PR link, failure classification) | N/A | ⬜ pending |
| 44-XX-XX | TBD | TBD | CODE-01 (driver parity, D-04) | — | Codex argv/env/interactivity unchanged | unit | `cargo test -p devflow-core --lib codex_and_pi_drivers_reproduce_legacy_behavior codex_wraps_prompt_in_exec_and_json codex_grants_writable_roots_for_worktree_git_metadata codex_disables_signing_via_env_others_do_not codex_define_and_plan_require_an_existing_artifact` | ✅ `crates/devflow-core/src/agents/mod.rs` | ⬜ pending |
| 44-XX-XX | TBD | TBD | #147 (D-05/D-06/D-07) | — | `resume --agent` mutates only agent, saves before relaunch, emits handoff event | unit + integration | new tests in `pipeline_launch.rs`; extend `--help` snapshot | ❌ Wave 0 — new tests, new snapshot regen | ⬜ pending |
| 44-XX-XX | TBD | TBD | #147 (D-08) | — | Refuses unsafe handoff before mutation | unit | negative-control: `resume --agent codex` at `Stage::Define` in Auto mode with no `-CONTEXT.md` on develop must refuse and leave `state.agent` unchanged | ❌ Wave 0 | ⬜ pending |
| 44-XX-XX | TBD | TBD | #148 (D-10/D-14) | T-44-01 | `--from-devflow` string is gone; new command uses real flags | unit | rewrite `cron_instruction_hints_include_hermes_command_per_phase`, `cron_hint_line_appends_sanitized_reset_when_retry_after_present`, `cron_hint_line_omits_reset_fragment_when_retry_after_empty` (`commands.rs:4139-4200`) + new negative-control asserting absence | ✅ files exist, ❌ assertions need rewriting | ⬜ pending |
| 44-XX-XX | TBD | TBD | #148 (D-12) | — | Schedule is unambiguous regardless of scheduler timezone | unit | positive: ISO-with-offset schedule round-trips; negative control: OLD `M H D M W` UTC-field approach demonstrably fires at wrong instant in non-UTC zone | ✅ `ship.rs` schedule tests exist as base, ❌ new ISO-render tests needed | ⬜ pending |
| 44-XX-XX | TBD | TBD | #148 (D-13) | T-44-03 | Unparseable retry time still fails closed | unit | existing `cron_instructions_reject_unparseable_retry_time` (`ship.rs`) — re-verify after render-function swap | ✅ `crates/devflow-core/src/ship.rs` | ⬜ pending |
| 44-XX-XX | TBD | TBD | #153 (D-15/D-16) | — | Cron record deleted only after genuine relaunch; survives a failed launch | unit + integration | new test: stub failing `spawn_monitor`/agent binary, assert record survives; new test: successful relaunch deletes it | ❌ Wave 0 | ⬜ pending |
| 44-XX-XX | TBD | TBD | #153 (D-17) | — | Ship completion deletes any remaining record | integration | new test at `finish_workflow_with_gate_timeout` call site | ❌ Wave 0 | ⬜ pending |
| 44-XX-XX | TBD | TBD | #153 (D-18) | — | Deletion emits an audit event | unit | assert event presence/fields on both delete paths | ❌ Wave 0 | ⬜ pending |
| 44-XX-XX | TBD | TBD | Phase success criterion 3 | — | No regression to Codex driver behavior | full suite | `cargo test --workspace` (assert `N passed`, `0 failed`, per this repo's own false-green-avoidance rule) | ✅ existing baseline | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

*Task IDs, Plan, and Wave columns are TBD — the planner fills these in once plans/waves are assigned; the Requirement/Test-Type/Command/File-Exists columns above are carried verbatim from `44-RESEARCH.md`'s Phase Requirements → Test Map.*

---

## Wave 0 Requirements

- [ ] `pipeline_launch.rs` — handoff mutation tests (D-06/D-07/D-08 positive + negative)
- [ ] `commands.rs` — rewritten cron-hint tests + `--from-devflow`-absence negative control
- [ ] `ship.rs` — ISO-with-offset schedule render + round-trip tests, negative control for the old UTC-cron-field approach
- [ ] `pipeline_launch.rs` / `pipeline_gate.rs` — cron-deletion trigger tests (success deletes, failure preserves, ship-completion belt-and-braces)
- [ ] `crates/devflow-cli/tests/snapshots/devflow-help.txt` — regenerate after the `--agent` flag lands on `Resume`
- [ ] Codex dogfood evidence capture directory (per 13-06/34-evidence precedent): commands run, `--json` capture, PR link, failure classification if any gap surfaces

*(Framework itself needs no install — `cargo test` is already the project's only test runner.)*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| A real phase completes through `--agent codex`, or the run surfaces concrete, re-filed gaps | CODE-01 | Inherently a live dogfood run against a real Codex CLI session, not something a unit test can substitute for | Run a real phase through `devflow start --agent codex` (or `resume --agent codex`), capture commands run, verbatim Codex `--json` output, PR link, and a DevFlow-vs-external-workflow failure classification for anything that goes wrong — per the 13-06 dogfood evidence precedent |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
