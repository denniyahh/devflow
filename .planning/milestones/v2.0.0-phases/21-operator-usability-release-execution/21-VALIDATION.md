---
phase: 21
slug: operator-legibility-observability
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-23
---

# Phase 21 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (Rust, edition 2024) |
| **Config file** | none — workspace `Cargo.toml` |
| **Quick run command** | `cargo test -p devflow-cli staleness::` (per-unit; swap module) |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~full suite dominated by `build_provenance.rs`; per-module quick runs are seconds |

---

## Sampling Rate

- **After every task commit:** Run the relevant per-module quick command (e.g. `cargo test -p devflow-cli staleness::`)
- **After every plan wave:** Run `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** seconds (per-module) / minutes (full suite)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| (planner fills) | | | no REQ-IDs — map to CONTEXT D-01..D-08 | | | unit | `cargo test …` | | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- Existing infrastructure covers all phase units (`cargo test` present; each unit adds tests into existing modules — `staleness.rs`, `commands.rs`, `parallel.rs`, `ship.rs`).

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| (planner fills, if any) | | | |

*If none: "All phase behaviors have automated verification."*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s (per-module)
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
