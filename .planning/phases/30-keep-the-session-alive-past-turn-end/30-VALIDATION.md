---
phase: 30
slug: keep-the-session-alive-past-turn-end
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-02
---

# Phase 30 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust workspace; package `devflow` for CLI, `devflow-core` for core) |
| **Config file** | Cargo.toml (workspace root) — no Wave 0 install needed |
| **Quick run command** | `cargo test -p devflow-core agent_result` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~{N} seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p devflow-core agent_result`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** {N} seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 30-01-01 | 01 | 1 | 30b | T-30-01 / — | {expected secure behavior or "N/A"} | unit | `{command}` | ✅ / ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

*Note: `cargo test --exact` with a bare test name matches nothing and still exits 0 — verification commands must assert on `1 passed` (or the expected count), never on exit code alone.*

---

## Wave 0 Requirements

- [ ] Fixture strategy for the archived v2/v3 JSONL captures (in-crate copies or `concat!` literals — never a cross-boundary `include_str!` into `.planning/`)

*If none: "Existing infrastructure covers all phase requirements."*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| {behavior} | 30c / 30d | {reason} | {steps} |

*If none: "All phase behaviors have automated verification."*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < {N}s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
