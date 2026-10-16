# Phase 50 — Crash-point table (launch, handoff, stop)

**Purpose.** One place that crosses each lifecycle step with each fault and states the expected outcome and the test
that proves it. The plan set states these outcomes in prose across 24 plans; this table is the reference the plan
checker and the executor check them against, and the input Phase 50.1's ownership token must keep true.
**Built:** 2026-10-16 at `2f1a461`, by Claude, from the plans' `must_haves` truths, threat tables and
`50-VALIDATION.md`. Operator decision (2026-10-16): write it before the final checker pass, keeping Phase 50 before
50.1, with a trip-wire: if more than a handful of rows have no answerable outcome without deciding who owns the run,
the ordering question goes back to the operator.

**Actors and records.** L = the launcher (`devflow start` / `resume`, holds the per-phase lock). S, S(N), S(N+1) =
`__monitor` supervisors (own process group, 50-11). A = an agent group (`process_group(0)`, 50-08). Records:
`state.supervisor` (50-06), the sidecar `<stem>.json`, the socket `<stem>.sock` (published from `<stem>.part`, 50-05),
the per-phase lock, `state.stopped`.

**Status values.** COVERED: a plan states the outcome and a named test proves it. STATED: a plan states it, no named
test for this timing. ACCEPTED: a recorded residual (threat id or backlog). GAP: no plan states the outcome.
OWNERSHIP: the correct outcome cannot be stated without deciding who owns the run at that moment (the trip-wire class,
50.1's launcher-to-monitor ownership token).

## A. First launch (L holds the per-phase lock)

| # | Step | Fault | Expected outcome | Proof | Status |
|---|------|-------|------------------|-------|--------|
| A1 | guard, then lock | a second launcher at the same time | exactly one ALIVE supervisor and one agent group; the loser refuses (lock or live-supervisor refusal) | V-12 | COVERED |
| A2 | lock held, before spawn | L SIGKILLed | lock holder dead; the next acquire reclaims it by start time | V-24 | COVERED |
| A3 | S: agent fork → sidecar rename | S SIGKILLed | agent orphaned with no record | T-50-70; backlog 999.156 | ACCEPTED |
| A4 | S: sidecar written → publish | S SIGKILLed | L ends the agent group the sidecar records, after `backstop::assess` validates it | V-34 | COVERED |
| A5 | S never publishes, or exits first | — | L ends the child and its agent group; `MonitorError::NotReady` | V-33 | COVERED |
| A6 | S: publish onto an existing name | a live socket holds the name | `hard_link` fails `EEXIST`; the live socket is never replaced | V-11 | COVERED |
| A7 | L: publish wait → record save | **L dies** (SIGKILL; or SIGINT: a terminal Ctrl-C reaches only L, since S has its own group, 50-11) | **Not stated.** S and its agent run on with no record (`start`) or under the previous GONE record (`resume`). `status` reads NotLaunched, or GoneWhileActive with the hint `devflow resume`. The guard, with a loadable state, probes only `state.supervisor` (50-15), so a second `start`/`resume` launches a second supervisor and agent in the same worktree. `doctor` would list the socket (50-12: launch id not the state's) | — | **ACCEPTED, OWNERSHIP**: T-50-90 (50-15), resolved in Phase 50.1 (operator, 2026-10-16) |
| A8 | L: record save | save fails | L shuts S down through its socket; an unconfirmed shutdown keeps the record and prints `devflow stop --phase N --launch <id>` | V-31, V-38: they test `publish_record`, which 50-23 calls from `spawn_agent_and_record` on the first launch too (`launch_stage_inner`); no end-to-end test (plan check R6C-I6) | COVERED |
| A9 | refused or failed launch | — | `state.supervisor` cleared on the failure path only; the run reads NotLaunched | V-30 | COVERED |

## B. Agent running (S owner phase `AgentRunning`)

| # | Step | Fault | Expected outcome | Proof | Status |
|---|------|-------|------------------|-------|--------|
| B1 | running | socket stop (`devflow stop`) | agent group killed (TERM, grace, KILL), 143, no advance, socket GONE, `stopped` | V-17 | COVERED |
| B2 | running | SIGTERM / SIGINT to S | the same as B1 (D-05) | V-16 | COVERED |
| B3 | running | S SIGKILLed | socket STALE, agent alive; `stop` runs the backstop, which signals a validated leader-present group | V-18; fault case `kill9-stale-stop` | COVERED |
| B4 | STALE | leader gone, or pgid reused | refuse and report; nothing signalled | V-19, V-20 | COVERED |
| B5 | STALE | host rebooted | previous boot: files removed, nothing signalled | V-21 | COVERED |
| B6 | STALE, off Linux | — | identity unprovable: refuse, print the manual repair | T-50-88; backlog 999.158 | ACCEPTED |
| B7 | agent exits | a stop at the same moment | ordered by the owner mutex; never advances | V-14 | COVERED |
| B8 | stop | a group member ignores SIGTERM | SIGKILLed after the leader exits | `supervisor::runtime::tests::a_term_ignoring_group_member_is_killed_after_the_leader_exits` (50-07) | COVERED |
| B9 | running | state file corrupt; `stop` | stop finds the socket by sidecar, ends the run, exits 0, says `stopped` was not recorded | V-39 (50-14 D-09 tests) | COVERED |
| B10 | running | state and sidecar both corrupt; `start` | the guard attributes the socket by its file name and refuses | V-36, V-37 | COVERED |

## C. Advance in S(N) (holds the per-phase lock)

| # | Step | Fault | Expected outcome | Proof | Status |
|---|------|-------|------------------|-------|--------|
| C1 | safe points 1 and 2 | stop | state stopped, lock released, no N+1 | V-14 interleavings | COVERED |
| C2 | gate wait | SIGTERM | wait aborts within about 1 s; lock released; GONE | V-15 | COVERED |
| C3 | gate wait | S SIGKILLed (lock held by a dead pid) | `stop` tears down to GONE; the lock is then reclaimable | fault case `kill9-at-gate`; 50-14 R-10 truth | COVERED |
| C4 | S(N+1) spawned → its record saved | **S(N) SIGKILLed** | **Not stated.** S(N+1) is live and unrecorded; state names S(N), whose socket reads STALE. `stop` runs the backstop on S(N)'s recorded group, which is empty (its agent finished), removes the files and reports the phase resumable with `devflow resume`. The guard then probes GONE and `resume` launches a second S(N+1). `doctor` would list the first as superseded | — | **ACCEPTED, OWNERSHIP**: T-50-91 (50-15), resolved in Phase 50.1 (operator, 2026-10-16) |
| C5 | S(N+1) never publishes | — | 50-24 teardown; `NotReady`; record cleared on the failure path | V-33, V-30 | COVERED |
| C6 | safe point 3 | stop | N+1 shut down through its socket; its record not saved | V-14 ("stop between spawn and publish"), 50-13 | COVERED |
| C7 | safe point 3 → save → return | SIGTERM/SIGINT or socket stop to S(N) | flagged (N stays `Advancing`); forwarded to N+1 right after the handed-off marking; N+1 stops | V-35, V-48 | COVERED |
| C8 | before N+1's record is saved | stop sent straight to N+1 | N's post-save re-probe sees it ended and records `stopped` | V-32 | COVERED |
| C9 | at the save | N+1 accepted a stop, outcome undecided | N+1 records its own stop once N releases the lock | V-50 | COVERED |
| C10 | record save | save fails | N+1 shut down; an unconfirmed shutdown keeps the record and prints the `--launch` repair | V-31, V-38 | COVERED |
| C11 | after the save, before N's unlink | S(N) SIGKILLed | state names the live N+1; N's leftover socket reads STALE with another launch id | 50-13 `a_kill_between_the_next_publish_and_the_old_unlink_leaves_state_on_the_live_next_supervisor`; fault pointer `publish-unlink-window` | COVERED |
| C12 | after the handed-off marking, before `finish` | signal to S(N) | not forwarded | T-50-74 | ACCEPTED |
| C13 | after the handed-off marking | socket stop to S(N) | answered `handed_off`; `stop` retries against the state's record (N+1) | V-57 (50-14): `commands::tests::stop_retries_a_handed_off_reply_against_the_reloaded_record`; control `commands::tests::stop_gives_up_after_three_handed_off_retries_and_records_nothing` (scripted sockets, not real supervisors) | COVERED |
| C14 | after the save, N still `Advancing` | `stop --launch <N's id>` | forwarded; stops N+1 | T-50-75 | ACCEPTED |
| C15 | during the handoff | `resume` by the operator | lock contended → refuse; after release, the guard probes N+1 ALIVE → refuse | after release: `resume_refuses_a_phase_whose_recorded_monitor_is_alive` once 50-15 plants a real supervisor (plan check R6C-I6); the contended-lock refusal for `resume` has no named test (V-12 covers start-vs-start) | STATED |

## D. Cross-cutting

| # | Fault | Expected outcome | Proof | Status |
|---|-------|------------------|-------|--------|
| D1 | power loss during a state write (no fsync of the file or directory) | the state may revert to an older or empty file | Phase 50.1 scope (fsync) | ACCEPTED (50.1) |
| D2 | a stale gate answer on disk decides the next gate | — | AR-48-04; Phase 50.1 scope (gate-waiter registration) | ACCEPTED (50.1) |
| D3 | the cache dir (`sock/`) is deleted while a run is live | probe reads GONE while S lives; `resume` would launch a second supervisor | — | GAP (low likelihood: operator action) |

## Result

- 37 rows: 27 COVERED, 1 STATED, 8 ACCEPTED (six in Phase 50's threat tables or backlog, two deferred to 50.1),
  1 GAP (D3). A7 and C4 were GAP and are accepted by operator decision (2026-10-16) for Phase 50.1 to resolve.
  Round-6 plan check (2026-10-16): A8 and C13 moved to COVERED (C13 by a new 50-14 test pair, V-57); C15 stays
  STATED for its contended-lock half.
- **Trip-wire: 2 OWNERSHIP rows (A7, C4), one root cause.** In both, a supervisor is live and its record was never
  saved because the process that should save it died. With a loadable state the start/resume guard trusts the record
  alone, so the unrecorded supervisor is invisible to it, and `status` (A7) or `stop` (C4) points the operator at
  `resume`. Two rows are below the "more than a handful" threshold, so the ordering question is not re-raised.
- **What this table does not establish.** It covers only the windows listed; a missing row is not a proven-safe
  window. "COVERED" means a plan names a test, not that the test exists or passes (none of the code exists yet). The
  rows were extracted by one reader (Claude) from plan text in one pass, and the plans, not this table, remain the
  authority when they disagree.
