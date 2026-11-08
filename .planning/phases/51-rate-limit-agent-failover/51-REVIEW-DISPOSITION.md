---
phase: 51
review: 51-REVIEW.md
titles: json
findings:
  - id: WR-01
    severity: warning
    disposition: open
    title: "The IN-03 \"way on\" sentence is always cut from the pending-gate banner and `gate list`"
  - id: WR-02
    severity: warning
    disposition: open
    title: "The X-01 fix closes the monitor/verdict disagreement only for a torn line that is already in the capture; a killed child's partial line still reproduces it"
  - id: IN-01
    severity: info
    disposition: open
    title: "The status hint ignores the environment's legacy opt-out, but `resume`'s refusal applies it"
  - id: IN-02
    severity: info
    disposition: open
    title: "A stale \"100-char\" doc comment on `gate_show`"
  - id: IN-03
    severity: info
    disposition: open
    title: "Nothing ties `GATE_CONTEXT_DISPLAY_CHARS` to the 300 that `MAX_IDLE_TIMEOUT_CLAUSE_CHARS` was derived from"
  - id: IN-04
    severity: info
    disposition: fixed
    title: "The monitor's torn-line warning now says \"a Claude quota denial cannot be ruled out\" on every agent's run, including the Antigravity run IN-04 was about"
open: 5
total: 6
recorded: 2026-11-08T18:32:04.220Z
---

# Phase 51: Code Review Disposition

| Finding | Severity | Disposition | Source |
|---------|----------|-------------|--------|
| WR-01 | warning | open | - |
| WR-02 | warning | open | - |
| IN-01 | info | open | - |
| IN-02 | info | open | - |
| IN-03 | info | open | - |
| IN-04 | info | fixed | 51-REVIEW-FIX.md (not in the current review) |

Dispositions: `open` (recorded, not yet triaged), `fixed`, `skipped`, `deferred`.
Set `deferred` by hand and put the reason in the Source cell; both are preserved. A `|` in the reason is kept as prose and escaped on the next run.
Re-running the gate keeps every row it can. A row the current review no longer reports is kept and its Source cell flagged, so a finding does not leave this record silently. ONE exception: when a finding id is REUSED by a different finding, the earlier decision cannot keep a row — the id is taken — and it is dropped. A RECORDED decision (anything but `open`) is named on the console when that happens; a row still at `open` is replaced silently, because `open` records no decision to lose.
