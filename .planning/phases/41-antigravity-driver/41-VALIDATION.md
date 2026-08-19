---
phase: 41
slug: antigravity-driver
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-19
---

# Phase 41 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust cargo test (existing) |
| **Config file** | none — Wave 0 uses existing infrastructure |
| **Quick run command** | `cargo test -p devflow --bin devflow -- --test-threads 1` |
| **Full suite command** | `cargo test -p devflow-core --lib && cargo test -p devflow --bin devflow` |
| **Estimated runtime** | ~120 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p devflow --bin devflow -- --test-threads 1`
- **After every plan wave:** Run `cargo test -p devflow-core --lib && cargo test -p devflow --bin devflow`
- **Before `$gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 41-01-01 | 01 | 1 | ANTG-01 | — | N/A | unit | `cargo test -p devflow-core --lib agent_kind` | ✅ | ⬜ pending |
| 41-01-02 | 01 | 1 | ANTG-02 | — | N/A | unit | `cargo test -p devflow-core --lib antigravity_driver` | ✅ | ⬜ pending |
| 41-01-03 | 01 | 1 | ANTG-03 | — | N/A | integration | `cargo test -p devflow --bin devflow marker_less_never_advances` | ❌ W0 | ⬜ pending |
| 41-02-01 | 02 | 2 | HYG-01 | — | N/A | integration | `scripts/check.sh all` + post-test process count | ✅ | ⬜ pending |
| 41-02-02 | 02 | 2 | HYG-02 | — | N/A | integration | `bash scripts/check-in-container.sh all` under uid 0 | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/devflow-core/src/agents/antigravity.rs` — AntigravityDriver impl + conformance tests
- [ ] `crates/devflow-core/tests/` — AgentKind::Antigravity FromStr/Display/driver_for unit tests
- [ ] `crates/devflow-cli/tests/phase7_cli.rs` — marker-less regression test (stubbed-PATH pattern)
- [ ] HYG-01 monitor-cleanup assertions in Phase-7 integration suite (existing tests, teardown reaping)
- [ ] HYG-02 debug failing git-env tests under root; fix or parametrize-skip

*Existing infrastructure covers all phase requirements; Wave 0 adds the ANTG-03 marker-less test and the HYG-01/02 fixes.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `agy` wrapper argv (no duplicate `--dangerously-skip-permissions`) | ANTG-02 | D-01 wrapper injects the flag; a duplicate is a silent-correctness risk | `agy -p --help` inspect argv; confirm driver build_command omits it |
| `devflow doctor` reports Antigravity installed | ANTG-01 | presence-only check (D-04) against live PATH | run `devflow doctor` with `agy` on PATH |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
