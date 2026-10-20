---
phase: 50
review: 50-REVIEW.md
titles: json
findings:
  - id: CR-01
    severity: critical
    disposition: open
    title: "A stop accepted during a Ship advance does not prevent the `--yes-ship` merge and release"
  - id: WR-01
    severity: warning
    disposition: open
    title: "`stop` reports \"nothing was signalled or recorded\" for a stop the supervisor accepted and will still carry out"
  - id: WR-02
    severity: warning
    disposition: open
    title: "Implicit `recover --clean` ignores the supervisor socket, though the docs say it probes it"
  - id: WR-03
    severity: warning
    disposition: open
    title: "A stream supervisor whose agent completed can hang forever in `threads.join()`, and no DevFlow verb can end it"
  - id: IN-01
    severity: info
    disposition: open
    title: "`poll_response_cancellable` keeps a backoff that can never take effect"
  - id: IN-02
    severity: info
    disposition: open
    title: "The control thread spins on a persistent `accept` error"
  - id: IN-03
    severity: info
    disposition: open
    title: "The stop success line claims \"agent group ended\" for stops honoured at a safe point"
open: 7
total: 7
recorded: 2026-10-20T21:36:15.055Z
---

# Phase 50: Code Review Disposition

| Finding | Severity | Disposition | Source |
|---------|----------|-------------|--------|
| CR-01 | critical | open | - |
| WR-01 | warning | open | - |
| WR-02 | warning | open | - |
| WR-03 | warning | open | - |
| IN-01 | info | open | - |
| IN-02 | info | open | - |
| IN-03 | info | open | - |

Dispositions: `open` (recorded, not yet triaged), `fixed`, `skipped`, `deferred`.
Set `deferred` by hand and put the reason in the Source cell; both are preserved. A `|` in the reason is kept as prose and escaped on the next run.
Re-running the gate keeps every row it can. A row the current review no longer reports is kept and its Source cell flagged, so a finding does not leave this record silently. ONE exception: when a finding id is REUSED by a different finding, the earlier decision cannot keep a row — the id is taken — and it is dropped. A RECORDED decision (anything but `open`) is named on the console when that happens; a row still at `open` is replaced silently, because `open` records no decision to lose.
