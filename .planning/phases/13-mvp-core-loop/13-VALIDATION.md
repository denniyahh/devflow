---
phase: 13
slug: mvp-core-loop
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-14
---

# Phase 13 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust workspace: devflow-core + devflow-cli) |
| **Config file** | Cargo.toml (workspace root) |
| **Quick run command** | `cargo test --workspace` |
| **Full suite command** | `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace`
- **After every plan wave:** Run `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 90 seconds

---

## Per-Task Verification Map

*To be filled by the planner from PLAN.md tasks.*

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| — | — | — | 13a–13e, WR-11 | — | — | — | — | — | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements — `cargo test` harness,
temp-git-repo integration test patterns (see `crates/devflow-cli/tests/phase7_cli.rs`),
and the 12-09 advance()/finish test harness are already in place.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Full-Ship end-to-end (re-run of 12-12 BLOCKED item) | 13a, 13e | Requires live agent + real repo + real PR flow | Run full Define→Ship loop on a real project after ship.rs rewrite lands |
| Dogfood run with notify hook (Claude adapter) | 13c, 13e | Requires real gate round-trip via ntfy/desktop | `devflow start` unattended; answer gates via notify hook |
| Codex adapter through Code→Validate | 13b, 13e | Requires real Codex CLI + credentialed run | Exercise loop with Codex; confirm envelope parsing against real `--json` stream |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
