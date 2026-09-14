---
phase: "48"
slug: "survivable-state-writes-and-honest-gate-recovery"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-09-14"
---

# Phase 48 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded from `48-RESEARCH.md` § Validation Architecture; the per-task map is filled from the PLAN.md set.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust libtest via `cargo test` |
| **Config file** | none for tests; `clippy.toml` is new in this phase (TEST-01 lint) |
| **Quick run command** | `cargo test -p devflow-core --lib <module>::tests::<name> -- --exact` or `cargo test -p devflow --bin devflow <module>::tests::<name> -- --exact` — output must contain `1 passed` and a non-zero `filtered out` (a name that is not module-qualified matches nothing and still exits 0) |
| **Full suite command** | `scripts/check.sh all` (fmt, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`) |
| **Estimated runtime** | Not measured — full-suite wall time is recorded before and after TEST-01 (D-08) |

---

## Sampling Rate

- **After every task commit:** the task's targeted `--exact` test(s) for the touched module, plus `cargo clippy -p <devflow-core|devflow> --all-targets -- -D warnings`
- **After every plan wave:** `scripts/check.sh all`
- **Before `/gsd-verify-work`:** full suite must be green; `scripts/check-in-container.sh`; one `taskset -c 0,1 scripts/check.sh test` run reported as a sanity check only (it does not prove the PATH flakes cannot recur)
- **Max feedback latency:** Not measured

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|

*No rows yet — populated from the PLAN.md set. Proposed tests per requirement: `48-RESEARCH.md` § Phase Requirements → Test Map.*

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/devflow-cli/tests/gate_wedge_e2e.rs` — SURV-02 criterion 4 (#200 wedge arm plus self-resolving control; new file)
- [ ] Generic child-process test helper in `crates/devflow-core/src/test_support.rs` — TEST-01 (every conversion depends on it)
- [ ] Real-plan checkpoint fixtures (task elements copied from the plans measured in research) — CHKPT-01, CHKPT-02
- [ ] Reusable "live foreign holder" fixture (a `devflow advance` child parked on a gate) — SURV-01, SURV-02

*Framework install: none.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|

*None identified in research. Revisit once the PLAN.md set exists.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency measured and recorded
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
