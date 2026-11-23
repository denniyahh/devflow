# Graphify review triage: PR #344 (phase 53.2), 2026-11-23

Head reviewed: 697bf46 (= PR head). Gate: pass, 0 blocking, merge confidence 5/5. The bot reports 17 possible issues
(13 unconfirmed bugs, 4 cleanups) plus coupling notes. The GitHub review comment and the Graphify MCP `review_status`
expose 5 of the 17; the other 12 are only on the Graphify web page and were not triaged here. Coupling "worse" notes
are skipped per the close-out procedure.

| # | Finding (bot, unverified) | Verdict | Reason (checked against source at 697bf46) |
|---|---|---|---|
| 1 | opencode.rs:1416 reap check may count an unreaped zombie as running | dismissed | `agent_running` (agent.rs:36-45) passes kill(0) and then classifies from /proc/<pid>/status via `running_per_status`, so a zombie is not reported as running |
| 2 | opencode.rs:1417 diagnostic /proc snapshot taken after the liveness read | dismissed | diagnostic text only; the snapshot is taken in the same loop iteration that broke on the decisive reading (opencode.rs:1411-1415), and the assertion uses that reading |
| 3 | test_support.rs:1323 raw-writer guard misses other ways to set a mode | dismissed | known scope of the source-scan guard (scans `set_permissions(`); recorded as code-review F16, which tightened kept-entry matching; widening it is not a defect in this change |
| 4 | pipeline_launch.rs:1056 `advance_stopped` source now "runner", not "supervisor" | dismissed | intended rename (#324, the supervisor was removed in 52.2); no reader in crates/, scripts/ or docs filters on `"supervisor"` (rg) |
| 5 | test_support.rs:1294 reformatting a kept permission call fails the guard | dismissed | by design: each kept fragment must match exactly one line (F16), so a reformat is a deliberate, visible guard update |

Hit rate on the visible five: 0 real.
