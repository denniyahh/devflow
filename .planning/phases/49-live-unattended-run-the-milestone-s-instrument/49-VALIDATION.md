---
phase: "49"
slug: "live-unattended-run-the-milestone-s-instrument"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-09-25"
---

# Phase 49 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (49.1 structure); bash + jq evidence assertions (Phase 49) |
| **Config file** | none — Wave 0 creates `.planning/phases/49-…/tools/` |
| **Quick run command** | `cargo test -p devflow --test phase7_cli`; `bash .planning/phases/49-*/tools/check-evidence.sh "$EV"` |
| **Full suite command** | `scripts/check.sh all` (container: `scripts/check-in-container.sh all`) |
| **Estimated runtime** | ~49 seconds |

---

## Sampling Rate

- **After every task commit:** Run the touched evidence tool's self-test (its negative control)
- **After every plan wave:** Run `scripts/check.sh all`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 49 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 49-01-01 | 01 | 1 | REQ-{XX} | T-49-01 / — | {expected secure behavior or "N/A"} | unit | `{command}` | ✅ / ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `tools/snapshot.sh` — read-only `.devflow/` + git facts per gate
- [ ] `tools/gate-watch.sh` — poll, notify, ledger (planted-gate test)
- [ ] `tools/surv-watch.py` — `--selftest` two-writer / one-writer controls
- [ ] `tools/negctl.sh` — negative-control arm (refusal text asserted)
- [ ] `tools/undo-ship-hooks.sh` — D-08 undo with refusing guards (scratch-repo test)
- [ ] `tools/check-evidence.sh` — criteria 1-3 assertions

*If none: "Existing infrastructure covers all phase requirements."*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| DECN-03 Followed / Not followed / Void / not-exercised | VERIFY-01 c4 | classification is a judgement over the resumed-session capture (47-PHASE49-OBSERVATION.md) | `jq 'select(.event=="checkpoint_auto_decided")'` over events + transcript review |
| Operator approvals at Define, Code, major-bump/Ship gates | VERIFY-01 c2 | human gates by design (D-03, D-07) | approve/reject commands named in the plan |

*If none: "All phase behaviors have automated verification."*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 49s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** {pending / approved YYYY-MM-DD}
