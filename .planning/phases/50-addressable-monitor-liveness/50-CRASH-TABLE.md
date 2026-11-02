# Phase 50 — Crash-point table (launch, handoff, stop)

**Purpose.** One place that crosses each lifecycle step with each fault and states the expected outcome and the test
that proves it. The plan set states these outcomes in prose across 24 plans; this table is the reference the plan
checker and the executor check them against, and the input Phase 50.1's ownership token must keep true.
**Built:** 2026-10-16 at `2f1a461`, by Claude, from the plans' `must_haves` truths, threat tables and
`50-VALIDATION.md`. Operator decision (2026-10-16): write it before the final checker pass, keeping Phase 50 before
50.1, with a trip-wire: if more than a handful of rows have no answerable outcome without deciding who owns the run,
the ordering question goes back to the operator.

**Actors and records.** L = the launcher (`devflow start` / `resume`; held the per-phase lock in Phase 50, and since Phase 50.1, which retired that lock, holds `.devflow/.holder-NN` before its spawn and a launch's reservation). S, S(N), S(N+1) =
`__monitor` supervisors (own process group, 50-11). A = an agent group (`process_group(0)`, 50-08). Records:
`state.supervisor` (50-06), the sidecar `<stem>.json`, the socket `<stem>.sock` (published from `<stem>.part`, 50-05),
the per-phase lock (Phase 50 only), `state.stopped`.

**Status values.** COVERED: a plan states the outcome and a named test proves it. STATED: a plan states it, no named
test for this timing. ACCEPTED: a recorded residual (threat id or backlog). GAP: no plan states the outcome.
OWNERSHIP: the correct outcome cannot be stated without deciding who owns the run at that moment (the trip-wire class,
50.1's launcher-to-monitor ownership token).

**Re-derived 2026-11-02 for Phase 50.1 (50.1-25).** Rows A7, C1-C4, C9, C15, D1 and D2 are restated with their 50.1
outcome, and A10, A11, C16-C19 and D4-D6 are new. Test names in those rows were checked against `cargo nextest list`
at this edit. "V-NN" alone in other rows is Phase 50's validation numbering; "50.1 V-NN" is Phase 50.1's. Phase 50.1
retired the per-phase lock (D-08): a launcher now reserves through a pending record and a held liveness file, a state
write takes the short `.devflow/.state-NN.lock`, a gate waiter holds its registration, and a foreground process holds
`.devflow/.holder-NN`. Rows A1, A2, B and the other C rows keep their Phase 50 wording, so "lock" there names that
retired lock; for A1 and A2 read A10, A11 and `commands::tests::two_launchers_cannot_both_reserve`.

## A. First launch (L reserves, Phase 50.1; L held the per-phase lock in Phase 50)

| # | Step | Fault | Expected outcome | Proof | Status |
|---|------|-------|------------------|-------|--------|
| A1 | guard, then lock | a second launcher at the same time | exactly one ALIVE supervisor and one agent group; the loser refuses (lock or live-supervisor refusal) | V-12 | COVERED |
| A2 | lock held, before spawn | L SIGKILLed | lock holder dead; the next acquire reclaims it by start time | V-24 | COVERED |
| A3 | S: agent fork → sidecar rename | S SIGKILLed | agent orphaned with no record | T-50-70; backlog 999.156 | ACCEPTED |
| A4 | S: sidecar written → publish | S SIGKILLed | L ends the agent group the sidecar records, after `backstop::assess` validates it | V-34 | COVERED |
| A5 | S never publishes, or exits first | — | L ends the child and its agent group; `MonitorError::NotReady` | V-33 | COVERED |
| A6 | S: publish onto an existing name | a live socket holds the name | `hard_link` fails `EEXIST`; the live socket is never replaced | V-11 | COVERED |
| A7 | L: publish wait → record save | **L dies** (SIGKILL; or SIGINT: a terminal Ctrl-C reaches only L, since S has its own group, 50-11) | The pending record and S's held liveness lock name the live S, so a second `start` or `resume` refuses naming `devflow stop`, and `status` names `stop`, not `resume`. With no promoter left, S's own promotion wait ends it (C16) | 50.1 V-01: `commands::tests::start_refuses_over_a_pending_supervisor_whose_launcher_died`, `commands::tests::resume_refuses_over_a_pending_supervisor_whose_launcher_died`; 50.1 V-07: `commands::tests::status_names_stop_not_resume_for_a_live_pending_supervisor` | COVERED (50.1; was ACCEPTED, OWNERSHIP) |
| A8 | L: record save | save fails | L shuts S down through its socket; an unconfirmed shutdown keeps the record and prints `devflow stop --phase N --launch <id>` | V-31, V-38: they test `publish_record`, which 50-23 calls from `spawn_agent_and_record` on the first launch too (`launch_stage_inner`); no end-to-end test (plan check R6C-I6) | COVERED |
| A9 | refused or failed launch | — | `state.supervisor` cleared on the failure path only; the run reads NotLaunched | V-30 | COVERED |
| A10 | L: pending saved → spawn | **pending saved, spawn failed** | The failure path clears the record, so no reservation is left and the run does not read as a live launch. A next-stage spawn behaves the same whether it fails before the pending save or after it, when the spawned supervisor exits unpublished | `pipeline_launch::tests::a_next_stage_spawn_that_fails_clears_its_pending_record`, `pipeline_launch::tests::a_failed_monitor_spawn_clears_the_supervisor_record` | COVERED |
| A11 | L: pending saved → spawn | **launcher dies after the pending save, before spawn** | Nothing holds the liveness lock, so the reservation reads abandoned and blocks nothing. `status` names `resume` with a note; the next launch clears it and says so; `stop` clears it and reports it | `pipeline_launch::tests::a_launcher_killed_after_its_pending_save_leaves_an_abandoned_reservation`, `commands::tests::an_abandoned_reservation_does_not_block_start`, `commands::tests::status_suggests_resume_once_the_pending_supervisor_is_gone`, `commands::tests::stop_clears_an_abandoned_reservation_and_says_so` | COVERED |

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
| B9 | running | state file corrupt; `stop` | stop finds the socket by sidecar, ends the run, exits 0, says `stopped` was not recorded | 50-14's `stop_ends_a_live_run_whose_state_will_not_load` (`stop_e2e`, gated in 50-14 Task 1; V-39 is the `--launch` variant) | COVERED |
| B10 | running | state and sidecar both corrupt; `start` | the guard attributes the socket by its file name and refuses | V-36, V-37 | COVERED |

## C. Advance in S(N) (takes the state write lock per write, Phase 50.1; held the per-phase lock in Phase 50)

| # | Step | Fault | Expected outcome | Proof | Status |
|---|------|-------|------------------|-------|--------|
| C1 | safe points 1 and 2 | stop | state stopped in one short write-lock hold, no N+1; no lock is held across the safe point | V-14 interleavings: `supervisor::runtime::tests::a_stop_racing_natural_completion_never_advances` | COVERED |
| C2 | gate wait | SIGTERM | wait aborts within about 1 s; the gate registration no longer reads live, and no state write lock was held across the wait; state stopped; GONE | V-15: `pipeline_launch::tests::a_signal_during_a_gate_wait_aborts_and_releases_the_lock` (its name predates the registration; it asserts the registration); 50.1 V-12: `pipeline_gate::tests::a_gate_wait_never_holds_the_write_lock` | COVERED |
| C3 | gate wait | S SIGKILLed (its registration ends with it) | `stop` writes no gate answer (no live waiter), tears the STALE socket down to GONE and names `devflow resume --phase N`; the dead waiter's registration reads absent | `stop_after_a_supervisor_killed_at_a_gate_reaps_and_reports_gone` (`stop_e2e`); fault case `kill9-at-gate` | COVERED |
| C4 | S(N+1) spawned → its record saved | **S(N) SIGKILLed** | State names N+1 as pending before N spawns it, so its held liveness lock keeps it live: `resume` refuses naming `devflow stop`, `status` names `stop`, and `doctor` does not call it superseded. With no promoter left, its promotion wait ends it (C16) | 50.1 V-02: `pipeline_launch::tests::resume_refuses_while_the_unpromoted_next_supervisor_lives`; `commands::tests::doctor_does_not_call_a_pending_next_supervisor_superseded` | COVERED (50.1; was ACCEPTED, OWNERSHIP) |
| C5 | S(N+1) never publishes | — | 50-24 teardown; `NotReady`; record cleared on the failure path | V-33, V-30 | COVERED |
| C6 | safe point 3 | stop | N+1 shut down through its socket; its record not saved | V-14 ("stop between spawn and publish"), 50-13 | COVERED |
| C7 | safe point 3 → save → return | SIGTERM/SIGINT or socket stop to S(N) | flagged (N stays `Advancing`); forwarded to N+1 right after the handed-off marking; N+1 stops | V-35, V-48 | COVERED |
| C8 | before N+1's record is saved | stop sent straight to N+1 | N's post-save re-probe sees it ended and records `stopped` | V-32 | COVERED |
| C9 | at the save | N+1 accepted a stop, outcome undecided | N+1 records its own stop once N's save names it; N's write lock is held only for that save, and the release order alone selects which of the two records it | V-50: `pipeline_launch::tests::a_next_supervisor_still_in_its_kill_grace_at_publish_records_its_own_stop` | COVERED |
| C10 | record save | save fails | N+1 shut down; an unconfirmed shutdown keeps the record and prints the `--launch` repair | V-31, V-38 | COVERED |
| C11 | after the save, before N's unlink | S(N) SIGKILLed | state names the live N+1; N's leftover socket reads STALE with another launch id | 50-13 `a_kill_between_the_next_publish_and_the_old_unlink_leaves_state_on_the_live_next_supervisor`; fault pointer `publish-unlink-window` | COVERED |
| C12 | after the handed-off marking, before `finish` | signal to S(N) | not forwarded | T-50-74 | ACCEPTED |
| C13 | after the handed-off marking | socket stop to S(N) | answered `handed_off`; `stop` retries against the state's record (N+1) | V-57 (50-14): `commands::tests::stop_retries_a_handed_off_reply_against_the_reloaded_record`; control `commands::tests::stop_gives_up_after_three_handed_off_retries_and_records_nothing` (scripted sockets, not real supervisors) | COVERED |
| C14 | after the save, N still `Advancing` | `stop --launch <N's id>` | forwarded; stops N+1 | T-50-75 | ACCEPTED |
| C15 | during the handoff | `resume` by the operator | the live-run guard and the reservation share one write-lock hold: N+1 pending and live → refuse naming `stop`; N+1 recorded and ALIVE → refuse. No lock wait to contend on any more, so the contended-lock half of this row is gone | `pipeline_launch::tests::resume_refuses_while_the_unpromoted_next_supervisor_lives`; `resume_refuses_a_phase_whose_recorded_monitor_is_alive` (`start_lock_e2e`) | COVERED |
| C16 | S(id) pending: serving → promotion | **S(id)'s promotion wait expires** (its promoter died) | The wait runs from when the orphan serves its socket, beside its agent. At `PROMOTION_WAIT` (15 s) the orphan ends its agent's whole process group and exits, writing nothing and not promoting itself; it logs `supervisor_superseded` with reason `orphaned_pending`. Until then it is a live pending supervisor that `status` reports with the `devflow stop --phase N` repair. The run is left for `resume` | `pipeline_launch::tests::an_orphaned_pending_supervisor_ends_itself_when_its_promotion_wait_expires` (5 s in the test build); control `pipeline_launch::tests::a_promoted_supervisor_keeps_its_running_agent_past_the_promotion_wait` | COVERED |
| C17 | any write | **superseded but live supervisor** (the state records another launch) | It cannot record its own stop: `record_supervisor_stop` writes nothing for a launch the state does not record, and the fence refuses its other writes. It ends its agent's group, logs `supervisor_superseded` with `agent_group`, and exits. Its socket, while it lasts, is an orphan `doctor` lists and `stop --phase N --launch <id>` tears down, writing no state. A bare `stop` addresses the state's record and leaves it; the launch guard probes only the recorded and pending sockets | `pipeline_launch::tests::a_superseded_supervisor_writes_no_state_ack_or_cleanup_and_ends_its_agent`; `bare_stop_addresses_the_state_record_and_leaves_a_superseded_socket` (`stop_e2e`) | COVERED |
| C18 | `stop`'s third arm → its write | **signalled holder publishes a supervisor before releasing** | `stop` does not mark the run `stopped` beside a launch none of its arms addressed, and says to run it again; that second `stop` stops the launch, or finds it gone and marks the run. Residual: the fixture plants the end state, so no real launcher was driven through the interleaving | `commands::tests::stop_does_not_mark_stopped_beside_a_supervisor_it_never_saw` | COVERED (end state planted) |
| C19 | `devflow ship` finalization | **stop during a devflow ship finalization** | The stop lands before `Merge`, at safe point 6, or after the pass ends. Once the pass starts, every hook runs: a successful pass ships, and a failed one stops at its retry gate (safe point 4). `stop` prints its not-marked message or its ended-run line; the record is the shipped one, with no `advance_stopped`. Limit: only a pause inside `Merge` is driven, not a stop during `VersionBump`, `ChangelogAppend` or `BranchCleanup` | `pipeline_gate::tests::a_stop_inside_the_ship_finalization_pass_lands_after_the_pass_ends` (with a holder-without-handler control, and one new tag); `a_real_stop_lets_a_real_ship_finish_the_pass_it_started` (`stop_e2e`); `pipeline_gate::tests::a_stop_during_a_finalization_retry_is_honoured_before_the_next_pass`; `pipeline_launch::tests::a_holder_stop_ends_a_foreground_gate_wait_at_safe_point_4` | COVERED |

## D. Cross-cutting

| # | Fault | Expected outcome | Proof | Status |
|---|-------|------------------|-------|--------|
| D1 | power loss during a state write | State and gate writes, publishes and removals go through `durable.rs`, which fsyncs the file and its directory; a source scan holds `workflow.rs` and `gates.rs` to the helpers. Power loss itself is untested: this proves the calls are made, not what survives a real cut (D-09) | 50.1 V-10: `durable::tests::state_and_gate_modules_mutate_files_only_through_the_durable_helpers`; 50.1 V-14: `durable::tests::replace_fsyncs_the_file_and_its_directory` | COVERED in code; power loss untested |
| D2 | a stale gate answer on disk decides the next gate | It cannot: every gate has a fresh id and an answer binds to one id. A missing or other id is set aside, never consumed or deleted; `resume` opens a new id; a late answer finds its gate closed and is set aside; a publish names the gate it observed; one approval has one consumer; a Supervise re-scan rejection does not answer the follow-on gate | 50.1 V-15: `gates::tests::an_answer_without_a_gate_id_is_quarantined_and_a_valid_one_is_then_accepted`, `gates::tests::await_answer_returns_an_unbound_answer_without_moving_it`, `pipeline_gate::tests::a_gate_wait_quarantines_an_answer_without_a_gate_id_and_accepts_a_valid_one`; V-16: `gates::tests::an_answer_for_another_gate_id_is_quarantined`; V-17: `pipeline_launch::tests::resume_quarantines_an_unused_gate_answer_and_opens_a_new_gate_id`; V-18: `gates::tests::a_late_respond_after_cleanup_cannot_decide_the_next_gate`, `commands::tests::a_gate_answer_published_as_its_gate_closed_is_reported_as_set_aside`; V-19: `commands::tests::gate_respond_publishes_only_for_the_observed_gate_id`, `commands::tests::gate_sweep_reaps_only_the_observed_gate_id`, `commands::tests::stop_answers_only_the_observed_gate_id`; V-20: `commands::tests::a_leftover_answer_start_could_not_delete_never_decides_the_new_run`; V-21: `pipeline_gate::tests::ship_override_and_a_live_waiter_cannot_both_consume_one_approval`; V-22: `pipeline_launch::tests::rejecting_the_rescan_gate_leaves_the_follow_on_gate_waiting_for_its_own_answer` | COVERED |
| D3 | the cache dir (`sock/`) is deleted while a run is live | probe reads GONE while S lives; `resume` would launch a second supervisor | — | GAP (low likelihood: operator action) |
| D4 | **claim published, consumer killed before recording** | `resume` closes the gate and says the answer "was claimed, with no record that it was applied", logging `resume_found_unapplied_answer`; at Ship it adds that a merge, version bump or tag may already have run. The rerun asks again. The same line also appears for a claim that was acted on with its gate left open, since the run cannot tell them apart | `pipeline_launch::tests::resume_reports_an_answer_claimed_but_never_applied`, `gates::tests::a_claim_whose_consumer_died_is_reported_when_its_gate_closes` | COVERED |
| D5 | **supervisor killed, socket gone, agent alive** | `status` and `stop` name the agent and the manual kill; `stop` writes nothing and exits non-zero; `start` and `resume` refuse. `devflow stop` cannot end the agent: that is backlog 999.170 | `stop_and_status_name_an_agent_whose_supervisor_is_gone` (`stop_e2e`), `commands::tests::status_and_doctor_name_an_orphaned_agent_not_resume`, `commands::tests::stop_names_an_orphaned_agent_and_writes_nothing` | COVERED (ending the agent: backlog 999.170) |
| D6 | **recover races a new start** | `recover --clean` decides on a fresh read under the state write lock, so a run that reserved after the sweep listed its phase is kept | `recover::tests::clean_keeps_a_run_that_reserved_after_the_sweep_listed_its_phase` | COVERED |

## Result

- **After Phase 50.1 (2026-11-02):** 46 rows: 41 COVERED (D1 for the code path only, with power loss untested; C18 with its end state planted; D5 except for ending the agent), 4 ACCEPTED, 1 GAP (D3). The trip-wire's two OWNERSHIP rows are resolved. Still ACCEPTED or GAP: A3, B6, C12, C14 and D3, none of them 50.1's scope.
- At Phase 50's close, 37 rows: 27 COVERED, 1 STATED, 8 ACCEPTED (six in Phase 50's threat tables or backlog, two deferred to 50.1),
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
