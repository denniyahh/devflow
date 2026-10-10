---
phase: "49"
slug: "live-unattended-run-the-milestone-s-instrument"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: false
wave_0_complete: true
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
| 49-01-01 | 01 | 1 | VERIFY-01 | see 49-01-PLAN threat model | — | doc/text gate | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 |
| 49-01-02 | 01 | 1 | VERIFY-01 | see 49-01-PLAN threat model | — | doc/text gate | plan `<verify>` `<automated>` | ✅ | ✅ green (stale slice) — gate_rc=1 only because its `### Phase 49:`→`### Phase 50:` slice now includes inserted Phase 49.1; bounded at `### Phase 49.1:` the section has 5 criteria and 0 `49.1`/`landing hop` mentions (2026-10-09) |
| 49-01-03 | 01 | 1 | VERIFY-01 | see 49-01-PLAN threat model | — | doc/text gate | plan `<verify>` `<automated>` | absent by design (task deletes it) | ✅ green — gate_rc=0 (asserts the plan file is gone) |
| 49-02-01 | 02 | 1 | VERIFY-01 | see 49-02-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 CHECK passed=20 failed=0; SELFTEST passed=26 failed=0 |
| 49-02-02 | 02 | 1 | VERIFY-01 | see 49-02-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=26 failed=0 |
| 49-03-01 | 03 | 1 | VERIFY-01 | see 49-03-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=6 failed=0 |
| 49-03-02 | 03 | 1 | VERIFY-01 | see 49-03-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=11 failed=0 |
| 49-04-01 | 04 | 1 | VERIFY-01 | see 49-04-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 after removing a stray gitignored `tools/__pycache__` left by the 2026-10-07 run (2026-10-09) |
| 49-04-02 | 04 | 1 | VERIFY-01 | see 49-04-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=7 failed=0 |
| 49-05-01 | 05 | 1 | VERIFY-01 | see 49-05-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=12 failed=0 |
| 49-05-02 | 05 | 1 | VERIFY-01 | see 49-05-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=12 failed=0 |
| 49-05-03 | 05 | 1 | VERIFY-01 | see 49-05-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=15 failed=0 |
| 49-06-01 | 06 | 2 | VERIFY-01 | see 49-06-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=26 failed=0 |
| 49-06-02 | 06 | 2 | VERIFY-01 | see 49-06-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 CHECK passed=20 failed=0; SELFTEST passed=26 failed=0 |
| 49-06-03 | 06 | 2 | VERIFY-01 | see 49-06-PLAN threat model | — | selftest+cargo test+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=26 failed=0; one_passed=1 |
| 49-07-01 | 07 | 2 | VERIFY-01 | see 49-07-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=34 failed=0; SELFTEST passed=8 failed=0 |
| 49-07-02 | 07 | 2 | VERIFY-01 | see 49-07-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=34 failed=0; SELFTEST passed=8 failed=0 |
| 49-07-03 | 07 | 2 | VERIFY-01 | see 49-07-PLAN threat model | — | doc/text gate | plan `<verify>` `<automated>` | ✅ | manual-only — not run: launches real devflow binary under a systemd --user unit (safety rule) |
| 49-08-01 | 08 | 3 | VERIFY-01 | see 49-08-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=34 failed=0 |
| 49-08-02 | 08 | 3 | VERIFY-01 | see 49-08-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=34 failed=0 |
| 49-08-03 | 08 | 3 | VERIFY-01 | see 49-08-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=34 failed=0 |
| 49-09-01 | 09 | 3 | VERIFY-01 | see 49-09-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | manual-only — not run: real prepare + probe-auth launches claude auth probe via systemd; selftest portion covered by 49-09-02 (17/0) |
| 49-09-02 | 09 | 3 | VERIFY-01 | see 49-09-PLAN threat model | — | selftest+lint | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 SELFTEST passed=17 failed=0 |
| 49-10-01 | 10 | 4 | VERIFY-01 | see 49-10-PLAN threat model | — | doc/text gate | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 |
| 49-10-02 | 10 | 4 | VERIFY-01 | see 49-10-PLAN threat model | — | doc/text gate | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 |
| 49-11-01 | 11 | 5 | VERIFY-01 | see 49-11-PLAN threat model | — | manual checkpoint | plan `<verify>` `<automated>` | n/a | manual-only — not run: blocking-human live attempt checkpoint; its read-only attempt-gate re-run inside 49-12-01 gave verdict=shipped |
| 49-12-01 | 12 | 6 | VERIFY-01 | see 49-12-PLAN threat model | — | doc/text gate | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 CHECK passed=11 failed=0; CHECK passed=3 failed=0; CHECK passed=5 failed=0; attempt_gate_verdict=shipped |
| 49-12-02 | 12 | 6 | VERIFY-01 | see 49-12-PLAN threat model | — | doc/text gate | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 |
| 49-12-03 | 12 | 6 | VERIFY-01 | see 49-12-PLAN threat model | — | doc/text gate | plan `<verify>` `<automated>` | ✅ | ✅ green — gate_rc=0 |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

*Filled by `/gsd-validate-phase` on 2026-10-09. Every command was run from a scratch-safe copy with its mktemp dir replaced by a scratch dir and no `rm -rf`; nothing else in the command changed. Text/doc gates show required text is present, not that it is correct.*

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
| Attempt negative-control launch (49-07-03) | VERIFY-01 c3 | launches the real devflow binary under a systemd --user unit | the runbook's negctl step; its outcome is checked by `check-evidence.sh --arm negative` (5/0 on attempt 3) |
| Attempt prepare + auth probe (49-09-01) | VERIFY-01 | real `prepare` plus a claude auth probe through systemd | runbook §prepare; the selftest portion runs in 49-09-02 (17/0) |
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

## Validation Audit 2026-10-09

| Metric | Count |
|---|---|
| Gaps found | 2 |
| Resolved | 2 |
| Escalated | 0 |
| Manual-only | 3 |
