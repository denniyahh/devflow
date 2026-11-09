---
phase: 51
review: 51-REVIEW.md
titles: json
findings:
  - id: WR-01
    severity: warning
    disposition: fixed
    title: "The IN-03 \"way on\" sentence is always cut from the pending-gate banner and `gate list`"
  - id: WR-02
    severity: warning
    disposition: deferred
    title: "The X-01 fix closes the monitor/verdict disagreement only for a torn line that is already in the capture; a killed child's partial line still reproduces it"
  - id: IN-01
    severity: info
    disposition: fixed
    title: "The status hint ignores the environment's legacy opt-out, but `resume`'s refusal applies it"
  - id: IN-02
    severity: info
    disposition: fixed
    title: "A stale \"100-char\" doc comment on `gate_show`"
  - id: IN-03
    severity: info
    disposition: fixed
    title: "Nothing ties `GATE_CONTEXT_DISPLAY_CHARS` to the 300 that `MAX_IDLE_TIMEOUT_CLAUSE_CHARS` was derived from"
  - id: IN-04
    severity: info
    disposition: fixed
    title: "The monitor's torn-line warning now says \"a Claude quota denial cannot be ruled out\" on every agent's run, including the Antigravity run IN-04 was about"
open: 0
total: 6
recorded: 2026-11-08T19:51:53.647Z
---

# Phase 51: Code Review Disposition

| Finding | Severity | Disposition | Source |
|---------|----------|-------------|--------|
| WR-01 | warning | fixed | 51-REVIEW-FIX.md |
| WR-02 | warning | deferred | docs-only in Phase 51 (ff649ff: residual documented); code fix deferred to 999.207 (operator, option A) |
| IN-01 | info | fixed | 51-REVIEW-FIX.md |
| IN-02 | info | fixed | 51-REVIEW-FIX.md |
| IN-03 | info | fixed | 51-REVIEW-FIX.md |
| IN-04 | info | fixed | 51-REVIEW-FIX.md (not in the current review) |

Dispositions: `open` (recorded, not yet triaged), `fixed`, `skipped`, `deferred`.
Set `deferred` by hand and put the reason in the Source cell; both are preserved. A `|` in the reason is kept as prose and escaped on the next run.
Re-running the gate keeps every row it can. A row the current review no longer reports is kept and its Source cell flagged, so a finding does not leave this record silently. ONE exception: when a finding id is REUSED by a different finding, the earlier decision cannot keep a row — the id is taken — and it is dropped. A RECORDED decision (anything but `open`) is named on the console when that happens; a row still at `open` is replaced silently, because `open` records no decision to lose.
