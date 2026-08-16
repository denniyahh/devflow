---
phase: 37
slug: modular-agent-driver-architecture-pi-driver-999-31-pi-999-94
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-15
---

# Phase 37 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `#[test]` (cargo) — `devflow-core` unit tests + `devflow` integration tests; byte-equality snapshot tests for zero-regression |
| **Config file** | workspace `Cargo.toml` (no per-phase test config) |
| **Quick run command** | `cargo test -p devflow-core --lib agents` |
| **Full suite command** | `cargo test --workspace` + `scripts/check.sh` |
| **Estimated runtime** | quick ~1s; full workspace ~2–4 min |

---

## Sampling Rate

- **After every task commit:** `cargo test -p devflow-core --lib agents` (driver/adapter changes)
- **After every plan wave:** `cargo test --workspace` + `scripts/check.sh`
- **Before `/gsd-verify-work`:** full suite green + zero-regression snapshots intact
- **Max feedback latency:** ~60s (devflow-core lib)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 37-01-01 | 01 | 1 | StageIntent de-Claude-ification (31a) | T-37-01/02 | Codex renders no `/gsd-*`; Claude/OpenCode byte-identical | unit | `cargo test -p devflow-core --lib` (byte snapshots + negative control) | ✅ | ⬜ pending |
| 37-01-02 | 01 | 1 | retire shared-prompt invariant | T-37-03 | invariant deleted, not skipped | unit | `grep -rn every_adapter_receives_identical_prompt_text crates/` (zero) | ✅ | ⬜ pending |
| 37-02-01 | 02 | 2 | AgentDriver trait (31c) | T-37-06 | capabilities `#[non_exhaustive]` + Default | unit | `cargo check -p devflow-core` + capabilities test | ✅ | ⬜ pending |
| 37-02-02 | 02 | 2 | Claude/OpenCode zero-regression | T-37-04/05 | Claude → PipeOwning by default, Legacy under `--legacy-claude-launch`; argv/prompt byte-equal | unit | `cargo test -p devflow-core --lib` (byte-equality) + `cargo test -p devflow --bin devflow` (routing) | ✅ | ⬜ pending |
| 37-03-01 | 03 | 3 | CodexDriver + 31b hardening | T-37-07/08 | verified non-interactive approval flag (spawn-tested); parsing relocated fail-closed | unit | `cargo test -p devflow-core --lib` (golden fixtures + spawn negative control) | ✅ | ⬜ pending |
| 37-03-02 | 03 | 3 | PiDriver on print mode | T-37-09 | `pi auth check` (not env sniffing) | unit | `cargo test -p devflow-core --lib agents::pi` | ✅ | ⬜ pending |
| 37-04-01 | 04 | 4 | conformance suite + DriverHealth + InteractivityMode | T-37-10 | test_contract passes all four drivers; InteractivityMode consumed in BOTH preflight.rs + commands.rs:289 | unit | `cargo test -p devflow-core --lib` (test_contract) | ❌ W0 | ⬜ pending |
| 37-04-02 | 04 | 4 | docs de-Claude-ification + AgentAdapter removal | T-37-11/12 | removal enumerated (canary.rs, test_support.rs, preflight.rs:1266, pipeline_launch.rs:190); docs grep-clean | source | `cargo test --workspace` + `scripts/check.sh` + grep | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/devflow-core/src/agents/conformance.rs` — the `test_contract()` suite (37-04 authors it)

*Existing infrastructure that carries over: cargo test, byte-equality snapshot fixtures, the
Phase-36 pi preflight stub tests, `scripts/check.sh` (clippy -D warnings).*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `devflow start --agent pi` reaches terminal completion | CONTEXT D-04 (deferred) | End-to-end Pi parity is deferred to 37.1/38 — NOT verified this phase | N/A this phase (recorded as deferred) |

*All in-scope behaviors (StageIntent, driver migration, conformance, docs) have automated
verification. The one manual-shaped item — Pi end-to-end — is explicitly out of scope.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
