---
phase: 33
slug: loop-back-correctness-for-multi-wave-validate-code-cycles-99
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-04
---

# Phase 33 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust's built-in `cargo test`, inline `#[cfg(test)] mod tests` colocated with each modified source file — no separate test directory or config file for this workspace's Rust crates |
| **Config file** | none — the workspace `Cargo.toml` plus each crate's own `Cargo.toml`; test behavior is governed by `scripts/check.sh`, not a pytest/jest-style test-config file |
| **Quick run command** | `cargo test -p devflow --lib pipeline_outcomes::` (and `mode::` / `state::` as needed) — package name is `devflow`, not `devflow-cli` |
| **Full suite command** | `scripts/check.sh all` (fmt + clippy `-D warnings` + `cargo test --workspace --no-fail-fast`) |
| **Estimated runtime** | Not benchmarked by research — crate-scoped `cargo test` slices are expected to complete in low tens of seconds; full-suite runtime is whatever `scripts/check.sh all` already takes today |

---

## Sampling Rate

- **After every task commit:** Run the relevant crate-scoped quick command above (e.g. `cargo test -p devflow --lib pipeline_outcomes::`)
- **After every plan wave:** Run `cargo test --workspace --no-fail-fast`
- **Before `/gsd-verify-work`:** `scripts/check.sh all` must be green
- **Max feedback latency:** 60 seconds (target, not a benchmark — these are fast Rust unit-test slices, not integration tests)

---

## Per-Task Verification Map

Task IDs are assigned once PLAN.md files exist; rows below carry the requirement → test mapping from research (`33-RESEARCH.md` § Validation Architecture) and will be reconciled against real task IDs at `/gsd-validate-phase`.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | TBD | TBD | DOGFOOD-01 (999.65, criterion 1) | — | Mid-arc Validate failure (no `{N}-VERIFICATION.md`) issues plain `/gsd-execute-phase {N}`, not `--gaps-only` | unit | `cargo test -p devflow --lib pipeline_outcomes::mid_arc_loop_back_issues_plain_command -- --exact` (new test, name illustrative) | ❌ Wave 0 — new test | ⬜ pending |
| TBD | TBD | TBD | DOGFOOD-01 (999.65, criterion 2) | — | Genuine-gaps Validate failure (`{N}-VERIFICATION.md` exists with findings) still issues `--gaps-only` | unit | `cargo test -p devflow --lib pipeline_outcomes::genuine_gaps_loop_back_still_issues_gaps_only -- --exact` (new test) | ❌ Wave 0 — new test | ⬜ pending |
| TBD | TBD | TBD | DOGFOOD-02 (999.66, criterion 3) | — | 3+ healthy wave transitions (new commits landing between cycles) do not false-gate | unit/integration | new variant of `consecutive_failures_reaches_ceiling_across_cycles` (`pipeline_outcomes.rs:1150-1201`) that commits real work between cycles and asserts the gate does NOT fire at cycle 3 | ❌ Wave 0 — new test | ⬜ pending |
| TBD | TBD | TBD | DOGFOOD-02 (999.66, criterion 4) | — | Same unresolved problem (no new commits between cycles) still reaches the ceiling and gates | unit | `consecutive_failures_reaches_ceiling_across_cycles` (`pipeline_outcomes.rs:1150-1201`) — existing test, should continue passing unchanged as the direct regression guard | ✅ Exists — regression guard | ⬜ pending |
| TBD | TBD | TBD | — (backward-compat) | — | New `State` field absent-from-JSON defaults correctly | unit | new test mirroring `infra_failures_absent_from_json_defaults_to_zero` (`state.rs:363-377`) | ❌ Wave 0 — new test | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Test-support helper to simulate "Code produced N new commits since the last Validate failure" inside a test — alongside the existing `test_support::*` helpers already used in `pipeline_outcomes.rs`'s test module (e.g. `init_repo`, `agent_free_git_only_path_dir`).
- [ ] The four new unit tests named in the Per-Task Verification Map above.
- [ ] No new test framework install needed — `cargo test` is already the workspace's only test runner.

---

## Manual-Only Verifications

*None — all phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
