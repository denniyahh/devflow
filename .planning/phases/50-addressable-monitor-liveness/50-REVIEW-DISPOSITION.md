---
phase: 50
review: 50-REVIEW.md
titles: json
findings:
  - id: CR-01
    severity: critical
    disposition: fixed
    title: "A stop accepted during a Ship advance does not prevent the `--yes-ship` merge and release"
  - id: WR-01
    severity: warning
    disposition: deferred
    title: "`stop` reports \"nothing was signalled or recorded\" for a stop the supervisor accepted and will still carry out"
  - id: WR-02
    severity: warning
    disposition: deferred
    title: "Implicit `recover --clean` ignores the supervisor socket, though the docs say it probes it"
  - id: WR-03
    severity: warning
    disposition: fixed
    title: "A stream supervisor whose agent completed can hang forever in `threads.join()`, and no DevFlow verb can end it"
  - id: IN-01
    severity: info
    disposition: deferred
    title: "`poll_response_cancellable` keeps a backoff that can never take effect"
  - id: IN-02
    severity: info
    disposition: deferred
    title: "The control thread spins on a persistent `accept` error"
  - id: IN-03
    severity: info
    disposition: deferred
    title: "The stop success line claims \"agent group ended\" for stops honoured at a safe point"
open: 0
total: 7
recorded: 2026-10-20T21:36:15.055Z
---

# Phase 50: Code Review Disposition

| Finding | Severity | Disposition | Source |
|---------|----------|-------------|--------|
| CR-01 | critical | fixed | `8946d7d` (RED test) + `373f1e6` (safe points 5 and 6); operator chose to fix in Phase 50, 2026-10-21 |
| WR-01 | warning | deferred | backlog 999.166 (operator decision 2026-10-21; Phase 50.1's stop-ownership work is the natural home) |
| WR-02 | warning | deferred | backlog 999.166 (operator decision 2026-10-21; Phase 50.1's stop-ownership work is the natural home) |
| WR-03 | warning | fixed | `07e237b` (tests that failed on the old code) + `493156b` (the stream wait ends at the agent's exit; the thread join is bounded; the capture is closed at the bound) + `5b1c417` (tests pinning the fix for stopped and real-binary runs) + `60b1538` (the revert check); reproduced by the phase verifier and fixed in Phase 50 by operator decision, 2026-10-21; live attempt 2 ran on the fixed binary |
| IN-01 | info | deferred | backlog 999.166 (operator decision 2026-10-21; Phase 50.1's stop-ownership work is the natural home) |
| IN-02 | info | deferred | backlog 999.166 (operator decision 2026-10-21; Phase 50.1's stop-ownership work is the natural home) |
| IN-03 | info | deferred | backlog 999.166 (operator decision 2026-10-21; Phase 50.1's stop-ownership work is the natural home) |

Dispositions: `open` (recorded, not yet triaged), `fixed`, `skipped`, `deferred`.
Set `deferred` by hand and put the reason in the Source cell; both are preserved. A `|` in the reason is kept as prose and escaped on the next run.
Re-running the gate keeps every row it can. A row the current review no longer reports is kept and its Source cell flagged, so a finding does not leave this record silently. ONE exception: when a finding id is REUSED by a different finding, the earlier decision cannot keep a row — the id is taken — and it is dropped. A RECORDED decision (anything but `open`) is named on the console when that happens; a row still at `open` is replaced silently, because `open` records no decision to lose.
