---
phase: 42
slug: hermes-driver
status: pending
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-21
---

# Phase 42 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> **Frontmatter note:** `status: pending` / `nyquist_compliant: true` / `wave_0_complete: true` reflect pre-execution state.
> **Verify rules:** (a) no trailing `::`; (b) integration tests use `--test phase7_cli`, NOT `--bin devflow`; (c) every filter names tests UNIQUE to the new work and FAILS (0 passed) against the unmodified tree; assert a real `1 passed` with non-zero `filtered out`.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust cargo test (existing) |
| **Config file** | none — uses existing infrastructure |
| **Quick run command** | `cargo test -p devflow-core --lib hermes -- --nocapture && cargo test -p devflow-core --lib hermes_conformance_enrollment -- --nocapture && cargo test -p devflow --bin devflow doctor_includes_hermes -- --nocapture && cargo test -p devflow --test phase7_cli hermes -- --nocapture` |
| **Full suite command** | `cargo test -p devflow-core --lib && cargo test -p devflow --bin devflow && cargo test -p devflow --test phase7_cli` |
| **Estimated runtime** | ~120 seconds (+ live dogfood run) |

---

## Sampling Rate

- **After every task commit:** Run the task's `<automated>` verify and assert `1 passed` with non-zero `filtered out`
- **After every plan wave:** `cargo test -p devflow-core --lib && cargo test -p devflow --bin devflow && cargo test -p devflow --test phase7_cli`
- **Before `$gsd-verify-work`:** Full suite green, plus live dogfood probe evidence recorded
- **Max feedback latency:** 120 seconds

---

## One-Time Execution Probes (not unit tests)

| Probe | Requirement | Why | Command / Evidence |
|-------|-------------|-----|--------------------|
| Antigravity Cadence & 60m `--print-timeout` probe (ANTG-04) | ANTG-04 | Verify real quiet-gap event cadence and confirm `--print-timeout 60m` survives long operations | Live dogfood run: `devflow start --agent antigravity --phase 42 --mode supervise` — record event cadence vs 120s floor in `42-UAT.md` / `42-VERIFICATION.md` |
| Hermes subagent delegation probe (D-04) | HRMS-02 | Verify dynamic probe correctly inspects `hermes tools list` | Live: `hermes tools list` contains `enabled delegation`, `HermesDriver.capabilities().subagent_dispatch == true` |

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 42-01-01 | 01 | 1 | HRMS-02 | T-42-01/02 | HermesDriver argv (-z, --yolo, --accept-hooks, HERMES_ACCEPT_HOOKS=1) + render_claude_style + subagent capability probe (hermes.rs) | unit | `cargo test -p devflow-core --lib hermes_driver -- --nocapture` | ✅ hermes.rs (new) | ⬜ pending |
| 42-01-02 | 01 | 1 | HRMS-01 | T-42-03 | AgentKind variant + FromStr/Display + driver_for dispatch + conformance enrollment 5→6 (state.rs, agents/mod.rs) | unit | `cargo test -p devflow-core --lib agent_kind_hermes -- --nocapture && cargo test -p devflow-core --lib hermes_conformance_enrollment -- --nocapture` | ✅ state.rs / agents/mod.rs | ⬜ pending |
| 42-01-03 | 01 | 1 | HRMS-01 | T-42-04 | doctor_checks() seam + hermes entry (commands.rs) | unit | `cargo test -p devflow --bin devflow doctor_includes_hermes -- --nocapture` | ✅ commands.rs | ⬜ pending |
| 42-01-04 | 01 | 1 | HRMS-03 | T-42-05/06 | stubbed hermes binary (marker-less, non-zero exit, hung process detection) with MonitorReapGuard (phase7_cli.rs) | integration | `cargo test -p devflow --test phase7_cli hermes -- --nocapture` | ✅ phase7_cli.rs | ⬜ pending |
| 42-02-01 | 02 | 2 | ANTG-04 | T-42-07 | Verify dogfood pre-conditions, launch supervised Antigravity run on Phase 42, measure event cadence and confirm 60m print-timeout | integration / dogfood | `devflow status --phase 42 2>&1 | grep -qE "Validate|complete" && echo "dogfood reached Validate"` | ✅ state / worktree | ⬜ pending |
| 42-02-02 | 02 | 2 | ANTG-04 | T-42-08 | Unlock unattended mode for Antigravity in preflight.rs (C2 gate), update tests | unit | `cargo test -p devflow --bin devflow unattended_launch_shape_condition_antigravity_allowed -- --nocapture` | ✅ preflight.rs | ⬜ pending |

---

## Wave 0 Requirements

- [x] `crates/devflow-core/src/agents/hermes.rs` — HermesDriver implementation (argv, prompt rendering, capabilities, environment)
- [x] `crates/devflow-core/src/state.rs` — `AgentKind::Hermes` registration
- [x] `crates/devflow-core/src/agents/mod.rs` — `driver_for` mapping and conformance suite enrollment (5 → 6 drivers)
- [x] `crates/devflow-cli/src/commands.rs` — `doctor_checks` hermes entry
- [x] `crates/devflow-cli/tests/phase7_cli.rs` — hermes stub fixtures and marker-less/exit/hung regression tests
- [x] `crates/devflow-cli/src/preflight.rs` — C2 gate unlock for Antigravity after dogfooding

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `hermes` live oneshot launch | HRMS-02 | Verify real CLI behavior with `--accept-hooks` | Run `hermes -z "echo test" --yolo --accept-hooks` and confirm execution without interactive prompt |
| Antigravity cadence observation | ANTG-04 | Real timing distribution | Review timestamps in `.devflow/phase-42.log` during dogfood run |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Every verify filter named to unique new-work tests and proven RED on unmodified tree
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all dependencies
- [x] One-time probes recorded
- [x] Feedback latency < 120s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
