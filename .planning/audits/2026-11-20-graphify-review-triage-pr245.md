# Graphify review triage — PR #245 (Phase 53), head f2c8349

Triaged 2026-11-20. The two confirmed bugs and findings 8, 9 and 11 were checked by the session. Findings 3–10 were
checked by a read-only helper agent, and the session spot-checked two of them (8 and 10).

| # | Finding | Verdict | Disposition |
|---|---|---|---|
| 1 | A failed abort save still closes the question gate (bug) | REAL (ask_server.rs:1192, :1237) | 999.256; operator decides fix-before-merge |
| 2 | The same P2, restated | REAL | 999.256 |
| 3 | A wait under 0.5 s rounds to a zero timeout | FALSE: `backoff_wait` reads the slot once before timing out (devflow-core gates.rs:1893-1911) | dismissed |
| 4 | Human-only checkpoint approval recording removed | INTENDED: 53-10-PLAN action | dismissed |
| 5 | A test admits pi, but the doc comment says pi is refused | FALSE as a bug; the doc comment is stale (preflight.rs:1355) | 999.258 |
| 6 | The devflow_ask check runs only on fallback agents | FALSE: also checked at start, dry run, launch and resume | dismissed |
| 7 | The checkpoint resume budget no longer resets per stage | INTENDED: removed with the relaunch loop (53-10, 53-03) | dismissed |
| 8 | Every launch gets the extended idle window | FALSE: extended only while a question gate is open (monitor.rs:1060-1100) | dismissed |
| 9 | A dry run can write a registration file | FALSE: `ensure_ask_registration` only queries `ask_support()` | dismissed |
| 10 | Status hides an unparsable question gate id with no warning | REAL (commands.rs:3160) | 999.258 |
| 11 | A late cancel stays in the cancelled set | REAL, minor (ask_server.rs:365-370) | 999.258 |
| 12 | The notify-hook test reads the marker before it is written | REAL test race (ask_e2e.rs:276-280) | 999.257 |
