---
phase: 39
slug: pi-end-to-end
status: validated
nyquist_compliant: false
wave_0_complete: true
created: 2026-08-18
---

# Phase 39 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust (`cargo test`) + `cargo clippy -D warnings` |
| **Config file** | `Cargo.toml` (workspace) |
| **Quick run command** | `cargo test -p devflow-core --lib agents::pi` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~4s quick · ~1–2 min full |

---

## Sampling Rate

- **After every task commit:** `cargo test -p devflow-core --lib agents::pi`
- **After every plan wave:** `cargo test --workspace`
- **Before `$gsd-verify-work`:** Full suite green + `clippy -D warnings` clean
- **Max feedback latency:** < 60s

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | Status |
|---------|------|------|-------------|-----------|-------------------|--------|
| 39-01 | 39 | 1 | provider-aware health (`settings.json` defaultProvider, google fallback) | unit | `cargo test -p devflow-core --lib agents::pi` | ✅ green |
| 39-02 | 39 | 1 | Pi → `MonitorLaunch::Legacy` | unit | `cargo test -p devflow --bin devflow pi_resolves_to_legacy_launch` | ✅ green |
| 39-03 | 39 | 1 | generic `DEVFLOW_RESULT` marker completion | unit | `cargo test -p devflow-core --lib agent_result` | ✅ green |
| 39-04 | 39 | 2 | vetted `@bacnh85/pi-subagent` detection | unit | `cargo test -p devflow-core --lib agents::pi` | ✅ green |
| 39-05 | 39 | 2 | trust boundary (fails closed headless) | manual | — | ⬜ manual |
| 39-06 | 39 | 2 | live subagent dispatch e2e | e2e (recorded) | — | ⬜ manual |

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements — no new test framework or fixtures
needed. The `PiDriver` unit-test harness (stubbed `pi` on `PATH`, `PI_CODING_AGENT_DIR`
isolation) was extended in this phase; the CLI e2e harness (`fake_bin_dir` + `CARGO_BIN_EXE_devflow`)
already exists in `crates/devflow-cli/tests/`.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Stage-2 subagent dispatch completes under `Legacy` + `DEVFLOW_RESULT` | 39-06 (D4) | A live LLM run against a real Pi profile (`@bacnh85/pi-subagent` + `litellm`) — not reducible to a unit test; the discriminating evidence is the recorded session transcript, not a re-runnable assertion. | Confirm `.planning/phases/39-pi-end-to-end/39-E2E-SESSION.jsonl` shows parent `toolCall: subagent` (provider `litellm`) → nested subagent `bash` → `DEVFLOW_RESULT` after the result returns. |
| `devflow start --agent pi` prints the full 5-stage pipeline | dry-run acceptance | CLI smoke against a live repo; display-only, no launch. | Run `devflow start --phase 39 --agent pi --mode auto --dry-run` from the phase-39 worktree and confirm Define → Plan → Code → Validate → Ship all render for Pi. |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or a documented Manual-Only entry
- [x] Sampling continuity: every code task (39-01..39-04) has automated verify
- [x] Wave 0 covers all MISSING references (none — infra pre-exists)
- [x] No watch-mode flags
- [x] Feedback latency < 60s
- [ ] `nyquist_compliant: true` — **NOT met**: two behaviors (39-05 trust-boundary, 39-06 live e2e + dry-run) are inherently manual, so the phase validates as **PARTIAL**, not Nyquist-compliant.

**Approval:** pending
