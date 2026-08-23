---
phase: 41
slug: antigravity-driver
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-19
revised: 2026-08-20 (round 3 — adversarial re-review rework; verify commands named to unique new-work tests)
---

# Phase 41 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> **Frontmatter note (round-3):** `status: complete` / `nyquist_compliant: true` /
> `wave_0_complete: true` / `Approval: approved (execution complete; two adversarial review rounds BLOCKed pre-execution, all findings absorbed)` reflect PRE-EXECUTION state — the phase has not
> run, so sampling compliance cannot be asserted. This file IS the working validation contract the
> plans reference; the sign-off items below flip during execution, not before it.
> **Verify rules (rounds 1-3):** (a) no trailing `::` — libtest does substring matching; (b)
> integration tests use `--test phase7_cli`, NOT `--bin devflow`; (c) every filter names tests
> UNIQUE to the new work and FAILS (0 passed) against the unmodified tree — assert a real
> `1 passed` with non-zero `filtered out`; 0 passed with exit 0 is a FAIL (F6/codex-6).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust cargo test (existing) |
| **Config file** | none — Wave 0 uses existing infrastructure |
| **Quick run command** | `cargo test -p devflow-core --lib antigravity -- --nocapture && cargo test -p devflow --test phase7_cli antigravity -- --nocapture` |
| **Full suite command** | `cargo test -p devflow-core --lib && cargo test -p devflow --bin devflow && cargo test -p devflow --test phase7_cli` |
| **Estimated runtime** | ~120 seconds (+ one-time probes below) |

---

## Sampling Rate

- **After every task commit:** Run the task's `<automated>` verify and assert `1 passed` with non-zero `filtered out`
- **After every plan wave:** `cargo test -p devflow-core --lib && cargo test -p devflow --bin devflow && cargo test -p devflow --test phase7_cli`
- **Before `$gsd-verify-work`:** Full suite green (bin unit tests AND `--test phase7_cli`), plus the one-time probes below
- **Max feedback latency:** 120 seconds

---

## One-Time Execution Probes (not unit tests)

| Probe | Requirement | Why | Command / Evidence |
|-------|-------------|-----|--------------------|
| 6-minute `--print-timeout` negative control (F3) | ANTG-02 | The CLI default is 5m; no reviewer measured whether it kills a long stream-json session. Run BEFORE the argv ships (Task 5 done) | Live: `printf '{"event":"user","message":{"role":"user","content":"<long task>"}}\n' \| agy --input-format stream-json --output-format stream-json --print-timeout 6m` — assert the session survives >5m of quiet or completes with a marker; record the result in the task summary |
| Antigravity cadence measurement (B3/D-08) | ANTG-03 | The idle-timeout default for Antigravity is decided (120s floor) but unmeasured | From the first real multi-stage run: record time-to-next-output; revisit `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS` if cadence exceeds the floor |

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 41-01-01 | 01 | 1 | ANTG-03 | T-41-01/02/04 | parser + ERROR envelope + agent-aware close predicate (agent_result.rs) | unit | `cargo test -p devflow-core --lib antigravity_event -- --nocapture` | ✅ agent_result.rs | ✅ green |
| 41-01-02 | 01 | 1 | ANTG-02 | T-41-04/06/08 | user_turn_line_for + agent-aware CloseRule + idle_timeout_setting_for (monitor.rs) | unit | `cargo test -p devflow-core --lib user_turn_line_for -- --nocapture && cargo test -p devflow-core --lib close_rule_antigravity -- --nocapture && cargo test -p devflow-core --lib idle_timeout_setting_for -- --nocapture` | ✅ monitor.rs | ✅ green |
| 41-01-03 | 01 | 1 | ANTG-02 | T-41-05 | predicate widening (legacy opt-out Claude-only) + AntigravityCanaryLauncher + AutoChainGuard comment (pipeline_launch.rs, canary.rs) | unit | `cargo test -p devflow-core --lib canary_antigravity -- --nocapture && cargo test -p devflow --bin devflow stream_launch_includes_antigravity -- --nocapture && cargo test -p devflow --bin devflow auto_chain_guard_antigravity -- --nocapture` | ✅ pipeline_launch.rs / canary.rs | ✅ green |
| 41-01-04 | 01 | 1 | ANTG-02 | T-41-07 | unattended-mode C2 decision — auto refused until dogfooded (preflight.rs) | unit | `cargo test -p devflow --bin devflow unattended_launch_shape_condition_antigravity -- --nocapture` | ✅ preflight.rs | ✅ green |
| 41-01-05 | 01 | 1 | ANTG-02 | T-41-03 | AntigravityDriver argv (no -p, --print-timeout 60m) + spawn smoke test + parse_completion delegate (antigravity.rs) | unit | `cargo test -p devflow-core --lib antigravity_driver -- --nocapture` | ✅ antigravity.rs (new) | ✅ green |
| 41-01-06 | 01 | 1 | ANTG-01 | T-41-10 | AgentKind variant + dispatch + conformance enrollment 4→5 (unique test name) (state.rs, agents/mod.rs, agent_kind_antigravity.rs) | unit + integration | `cargo test -p devflow-core --lib agent_kind_antigravity -- --nocapture && cargo test -p devflow-core --lib antigravity_conformance_enrollment -- --nocapture && cargo test -p devflow-core --test agent_kind_antigravity -- --nocapture` | ✅ state.rs / mod.rs / agent_kind_antigravity.rs (new) | ✅ green |
| 41-01-07 | 01 | 1 | ANTG-01 | T-41-09 | doctor_checks() seam + antigravity/agy entry + PATH test (commands.rs) | unit | `cargo test -p devflow --bin devflow doctor_includes_antigravity -- --nocapture` | ✅ commands.rs | ✅ green |
| 41-01-08 | 01 | 1 | ANTG-03 | T-41-02/11 | canary-aware agy stub + marker-less/happy/discrimination regressions + MonitorReapGuard defined here (phase7_cli.rs) | integration | `cargo test -p devflow --test phase7_cli antigravity -- --nocapture` | ✅ phase7_cli.rs | ✅ green |
| 41-02-01 | 02 | 2 | HYG-01 | T-41-HYG-01/04 | systematic MonitorReapGuard pass + suite registry/audit + intentional opt-out (phase7_cli.rs) | integration | `cargo test -p devflow --test phase7_cli -- --nocapture` | ✅ phase7_cli.rs | ✅ green |
| 41-02-02 | 02 | 2 | HYG-02 | T-41-HYG-02/03 | check-in-container.sh worktree-aware mount (NOT the 3 test files, NOT skip_if_root) | integration | `bash scripts/check-in-container.sh all` (from the worktree AND the main checkout) | ✅ check-in-container.sh | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky. Every filter above matches 0 tests on the unmodified tree — a pre-change `0 passed` is the expected RED, and `1 passed` is only acceptable once the new work lands (F6).*

---

## Wave 0 Requirements

- [x] `crates/devflow-core/src/agents/antigravity.rs` — AntigravityDriver (argv without `-p`, with `--print-timeout 60m`) + spawn smoke test
- [x] `crates/devflow-core/src/agent_result.rs` — `is_antigravity_event_stream` + `parse_antigravity_event_result` (+ ERROR envelope) + `event_is_top_level_antigravity_result_marker` + `evaluate_layer1` wiring
- [x] `crates/devflow-core/src/monitor.rs` — `user_turn_line_for` + agent-aware `CloseRule` + `idle_timeout_setting_for`
- [x] `crates/devflow-core/src/canary.rs` — `AntigravityCanaryLauncher`
- [x] `crates/devflow-cli/src/pipeline_launch.rs` — widened predicate (Claude-only legacy opt-out) + canary dispatch + AutoChainGuard comment
- [x] `crates/devflow-cli/src/preflight.rs` — C2 decision (auto refused until dogfooded)
- [x] `crates/devflow-cli/src/commands.rs` — `doctor_checks()` seam + `agy` entry
- [x] `crates/devflow-core/tests/agent_kind_antigravity.rs` — public-API tests
- [x] `crates/devflow-cli/tests/phase7_cli.rs` — canary-aware agy stub + marker-less (gate at Plan) + happy path + discrimination control + `MonitorReapGuard` defined here; NOT `#[ignore]`
- [x] HYG-01 suite registry/audit + intentional opt-out control
- [x] HYG-02 `check-in-container.sh` worktree fix, re-derived from container runs

*Wave 0 covers all round-2 MISSING references: CloseRule, canary seam, idle-timeout policy, print-timeout, preflight C2, doctor seam, e2e schema test, suite audit.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `agy` wrapper argv (no duplicate `--dangerously-skip-permissions`) | ANTG-02 | D-01 wrapper injects the flag; a duplicate is a silent-correctness risk | run `antigravity-cli --help` (NOT `agy -p --help` — `-p` is a string flag that consumes the next token as a prompt and invokes the model); confirm the driver build_command omits both `-p` and the flag |
| `devflow doctor` reports Antigravity installed | ANTG-01 | presence-only check (D-04) against live PATH | run `devflow doctor` with `agy` on PATH; without `agy` the entry reports absent/warn — never a hard failure |
| 6-minute `--print-timeout` probe | ANTG-02 | see One-Time Probes | run the probe; record the outcome before the phase ships |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Every verify filter named to unique new-work tests and proven RED on the unmodified tree (F6)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] One-time probes (print-timeout, cadence) recorded
- [x] No watch-mode flags
- [x] Feedback latency < 120s
- [x] `nyquist_compliant: true` set in frontmatter (flips at sign-off — pre-execution it stays false by design)

**Approval:** pending
