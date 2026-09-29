---
phase: "49"
slug: "live-unattended-run-the-milestone-s-instrument"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-09-28"
---

# Phase 49 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Script selftests (`--selftest` printing `  ok   <case>` lines and `SELFTEST passed=N failed=M`), `bash -n` + `shellcheck`, `ruff check` / `ruff format --check`, `python3 -m py_compile`; Rust suites only if a defect fix touches `crates/` |
| **Config file** | none — each tool carries its own fixtures in temp dirs |
| **Quick run command** | `bash <tool> --selftest` / `python3 <tool> --selftest` |
| **Full suite command** | every tool selftest + `bash scripts/scratch-dogfood-repo.sh --selftest`; if `crates/` changed: `cargo test -p devflow-core --lib && cargo test -p devflow --bin devflow && scripts/check.sh all` |
| **Estimated runtime** | ~30 seconds per tool selftest |

---

## Sampling Rate

- **After every task commit:** Run the touched tool's `--selftest` plus `shellcheck` / `ruff`
- **After every plan wave:** Run every tool selftest
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds for the faked selftests. Four gates run longer by design against real inputs: 49-05 Task 3 (real captures; `timeout 120`), 49-06 Task 3 (a `cargo test` that can exceed 30 s on a cold `target/`; no explicit bound, reason in its acceptance criteria), 49-07 Task 3 (real negative arm; `timeout 300`) and 49-09 Task 1 (real `prepare`, `timeout 900`, and the real auth probe, `timeout 300`). *(Note added in plan-phase revision, 2026-09-29, after checker iteration 2.)*

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 49-01-01 | 01 | 1 | REQ-{XX} | T-49-01 / — | {expected secure behavior or "N/A"} | unit | `{command}` | ✅ / ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

*The row above is the template placeholder. `/gsd-validate-phase` fills this map from the executed plans (`status: draft` until then); plan-phase does not hand-fill it. Each plan's `<verify>` block is the per-task command in the meantime. (Note added in plan-phase revision, 2026-09-29.)*

---

## Wave 0 Requirements

- [ ] `scripts/scratch-dogfood-repo.sh --selftest` (new) — covers the setup step
- [ ] Evidence tools (snapshot hook, attempt collector, negative-control driver, evidence checker, SURV watcher, sanitizer) — each with positive and negative selftest cases
- [ ] Every PLAN `<automated>` block that uses bash-only syntax wrapped in `bash -c '…'` (pre-commit enforced)

*If none: "Existing infrastructure covers all phase requirements."*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live attempt (criteria 1-5 evidence produced) | VERIFY-01 | a real `devflow start --mode auto` run with operator-answered Define and Code gates (D-18, D-22) | the single `checkpoint:human-action` plan and its runbook; `<verify>` reads the attempt evidence record (R-1) |
| DECN-03 Followed / Not followed / Void / not exercised | VERIFY-01 c4 | classification is a judgement over the resumed-session capture (`47-PHASE49-OBSERVATION.md`) | collected after the run from `.devflow/history/` and the Claude transcript (R-7) |

*If none: "All phase behaviors have automated verification."*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** {pending / approved YYYY-MM-DD}
