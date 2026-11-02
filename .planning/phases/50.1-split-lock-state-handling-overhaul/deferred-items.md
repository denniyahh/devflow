## Deferred Items

- A `devflow stop` that signals a `devflow parallel` would also stop the phases it launches later (found during 50.1-29)
  status: open
  **What:** `install_holder_stop_handler`'s flag is process-wide and never cleared, and `parallel` runs `start` for each phase in one process, so after one signal every later `start` in that process stops at safe point 2.
  **Measured:** nothing. This is from reading `parallel.rs` and `stop_requested_at_safe_point`, not from a run.
  **Why deferred:** D-18 specifies one flag per process, and changing it is outside 50.1-29. The result is over-stopping, recorded in each phase's state, and `devflow resume` continues them.

- `stop`'s not-marked message can name the wrong pid if the holder changes during its wait (found during 50.1-29)
  status: open
  **What:** `holder_still_running` reads the pid from the record after the wait. If the signalled holder exits and another foreground process claims within `TERMINATE_VERIFY_WAIT`, the message calls the new pid signalled.
  **Why deferred:** the message is the only effect, nothing is signalled or written, and it needs two foreground commands racing inside 3 s. `signal_sent` is a bool by plan.

- `liveness::tests::a_probe_in_flight_neither_reads_live_nor_fails_a_take` can fail under `cargo test` (found during 50.1-12)
  status: open
  **What:** one `scripts/check.sh all` at `d6003e2` failed it at `liveness.rs:805`: `take the exclusive lock: "WouldBlock"`, right after the test dropped its own shared-lock handles.
  **Measured:** 1 failure in 3 `check.sh` runs, 0 in 5 `cargo test -p devflow-core --lib` runs. 50.1-12 changed no `devflow-core` file.
  **Likely cause, not tested:** another test's fork holding a duplicate descriptor of the lock file, the case 50.1-08 moved its lock tests into child processes for. If so, running this test in a child would fix it.
  **Why deferred:** 50.1-12 is CLI test migration only and does not touch `liveness.rs`.

- `hermes_nonzero_exit_does_not_advance` can report its reaped monitor as alive (found during 50.1-12)
  status: likely fixed (0268976): the `agent_running` fix below. This failure was not replayed, so the link is inferred; 3 full runs at f1b7859 were clean, a weak bound at about 1 failure in 3.
  **What:** one `scripts/check.sh all` at `d6003e2` failed it at `tests/phase7_cli.rs:1656`: `monitor wrapper pid ... must be verified dead after reaping`.
  **Measured:** 1 failure in 3 `check.sh` runs, 0 in 8 `cargo test -p devflow --test phase7_cli` runs. 50.1-12's diff has no production line and does not touch this test file.
  **Likely cause, not tested:** the `agent_running` entry below, since the panic follows an `agent_running` call right after `terminate_and_verify`. No trace of this failure shows that.
  **Why deferred:** 50.1-12 does not touch `agent.rs` or `phase7_cli.rs`.

- `agent_running` can report a just-reaped process as running (found during 50.1-06)
  status: resolved (0268976): two causes, not one. `ESRCH`, and the `X (dead)` state a process shows while being reaped, neither of which read as gone. With 3 pollers beside a reap: 2185 false-then-true flips in 4000 rounds before, 667 with only `ESRCH` fixed, 0 in 12000 after. All 558 flips classified after the first fix were `X (dead)`.
  **What:** `agents::opencode::tests::spawn_with_timeout_kills_a_descendant_after_its_parent_exits` failed once in a full core run at `06c39fe`: `polls: 0`, still running 237 µs later, and `survivor after: GONE`. So one `agent_running` call said not running and the next said running. `crates/devflow-core/src/agent.rs` `running_per_status` treats only `NotFound` as gone. Measured: reading an already-open `/proc/<pid>/status` after the reap fails with `ESRCH`, and `std::io::Error::from_raw_os_error(3).kind()` is `Uncategorized`, so that read counts as "cannot tell" and `kill(0)`'s answer stands.
  **Likely fix, not tested:** treat `ESRCH` as gone in `running_per_status`.
  **Not established:** the sequence is inferred from those two measurements and the failure's diagnostics, not traced. The test failed 0 of 60 run alone. Whether this also explains backlog 999.151 was not checked; that test fails on a different assertion, though 50.1-05 saw the same not-running-then-running shape there.
  **Why deferred:** 50.1-06 does not touch `agent.rs` or `agents/`, and `opencode.rs` does not use gates.

- `devflow ship` cannot see a Ship answer written by `gate approve` until 50.1-14 (found during 50.1-13)
  status: resolved
  **What:** `gate approve` now writes the keyed answer, while `ship_override` still checks and reads the unkeyed response. Probed: approve, then `ship_override`, refuses with "no Ship gate response written yet". It fails closed, and its wording is wrong for that one wave.
  **Who:** 50.1-14 replaced that read with the keyed read and claim. `pipeline_gate::tests::ship_override_ships_an_approval_written_by_gate_approve` failed on this refusal before the change and passes after it.

- `stop_writes_no_state_while_the_lock_holder_survives_the_signal` can fail when `stop` signals its holder before the holder's trap is set (found during 50.1-14)
  status: resolved (f1b7859): the holder prints a ready line after its `trap`, and the test reads it before `stop`. With a `sleep 1` before the trap: passes with the wait, fails with the original message without it.
  **What:** one run of the plan's suite gate at `df4c973` failed it at `tests/stop_e2e.rs:158`: `stop must refuse to mark state while the holder survives`, with `signalled pid ...` and then the lock reclaimed as stale. The holder is `sh -c "trap '' TERM; exec sleep 60"`, and the test runs `devflow stop` as soon as `spawn` returns, so a TERM that lands before `trap` kills the shell.
  **Measured:** 1 failure in 3 runs of the integration binary under its suite (the `check.sh all` run and the rerun passed), 0 in 60 run alone. With a temporary `sleep 1;` before the `trap`, the test failed every time it ran (1 of 1), with the same message.
  **Likely fix, not tested:** have the holder write a ready file after its `trap`, and wait for that file before running `stop`.
  **Why deferred:** 50.1-14 did not change this test or `stop`'s holder path, and the test plants no gate.

- One `stop_e2e` failure at `361995e` whose test name was not captured (found during 50.1-18)
  status: open, test still unidentified. It may be the holder race fixed in f1b7859, unverified; this phase's wave gate now keeps failing logs.
  **What:** one run of `cargo nextest run -p devflow --test stop_e2e --no-tests=fail` reported `37/38 tests run: 36 passed, 1 failed`. A later loop overwrote the log, so the failing test is unknown.
  **Measured:** 1 failure in 23 runs at that commit: the failure, then 8 clean runs alone and 14 clean runs beside another nextest run. It may be `stop_writes_no_state_while_the_lock_holder_survives_the_signal`, logged above, whose failure rate looks similar, but that is unverified.
  **Why deferred:** it did not reproduce in 22 reruns. 50.1-18 changed `stop_in`, so its changes are not ruled out as the cause. Keep the failing log the next time this happens.

- `a_probe_in_flight_neither_reads_live_nor_fails_a_take` and a `while_owner` control read a lock right after its release (found during 50.1-05 and by a fork storm)
  status: resolved (f1b7859)
  **What:** another test's fork holds a duplicate of the descriptor, and so the flock, until its exec. Measured with 4 forking threads: a released lock read held in 9710 of 20000 rounds, 0 of 20000 with none. A temporary test forking for the first 120 s of the core lib suite failed the probe test in 2 of 2 runs and the `while_owner` control in 1 of 2; after the fix 0 failures in 2 runs.
  **Production:** read, not exercised: every production `try_lock` site polls with a bound, or fails toward "held". A fork can only make a lock read held for longer, never free, so the effect is a brief wait or a refusal.
  **Not established:** the storm covers the core lib suite only, and only tests that ran inside its 120 s.

- Three comments and help lines still name the per-phase lock after 50.1-20 (found during 50.1-20)
  status: resolved. 50.1-21 rewrote `commands.rs`'s `recover --clean` comment and the same file's "advance taking the lock" line. 50.1-22 rewrote `pipeline_gate.rs`'s safe-point-4 comment and `main.rs`'s `stop`, `--launch`, `ship` and `--force` help, and regenerated the help snapshot.
  **What:** `pipeline_gate.rs`'s safe-point-4 comment ("which releases the per-phase lock"), `main.rs`'s `stop` help ("per-phase lock holder when GONE", "signals no lock holder") and `commands.rs`'s `recover --clean` comment ("cleaned when its lock holder is dead").
  **Why deferred:** 50.1-22 owns the `stop` help and 50.1-21 deletes the lock API, so each goes with its owner. Nothing reads them as behaviour.

- `RecoverError::Lock` has no constructor left after 50.1-21 (found during 50.1-21)
  status: open
  **What:** nothing in `devflow-core` returns it now that `recover` takes no per-phase lock. Its `#[from] LockError` keeps it compiling and its doc says so.
  **Why deferred:** removing a `pub` variant is a semver break no 50.1 plan declared. It goes on 50.1-28's list to keep or remove.

- `recover --clean --phase N`'s guard can pass a STALE socket that the decision then refuses (found during 50.1-21)
  status: open, by plan
  **What:** `refuse_a_live_supervisor` lets through a STALE socket whose agent group the backstop proves gone, and `phase_may_be_live` keeps any STALE socket, as this plan specifies. So that case now prints the keep warning and exits non-zero until `devflow stop` removes the socket, where before it cleaned. `stop_stale_launch` runs the backstop that removes it (read, not run end to end here).
  **Why deferred:** the plan says STALE is not resolved through the backstop in recover. 50.1-25's docs should say `stop` comes first.

- `resume` calls an applied answer "claimed but never applied" in three reachable cases (found during 50.1-23)
  status: open, raised to the orchestrator as the plan requires
  **What:** 50.1-23 reports every claimed pair at `state.stage`, on R2-05's premise that an applied decision closes its gate before the stage runs again or moves `state.stage` on. The premise does not hold in these three cases, and each leaves the pair at `state.stage`:
  1. **`--until validate`:** an approved Validate gate reaches `transition`, whose `stop_until == Some(from)` early return records the stop and closes nothing (`pipeline_gate.rs`, the `stop_until` arm of `transition`).
  2. **SIGKILL during Ship finalization:** `finish_workflow` runs the terminal hooks with the claim still on disk and closes the gate only after they pass. The line then says "never applied" though a merge, bump or tag may have run.
  3. **A close that fails:** `handle_stage_failure`'s two arms (`pipeline_outcomes.rs` ~L929, ~L938) still `let _ = close_gate(..)` at an unchanged stage. So do the preflight arms (`preflight.rs` ~L1421, ~L1460), the re-scan arm (`pipeline_launch.rs`, "CR-01: close the gate now") and the finalization retry (`pipeline_gate.rs` ~L299).
  **Measured:** case 1 with a temporary test (`run_gate_with_timeout` with an approval, `transition`, then a real `resume`): the claim survived and one `resume_found_unapplied_answer` event was written. The test was not committed and did not go through a real supervisor. Cases 2 and 3 are from reading the code, not exercised.
  **Effect:** a wrong line and event on a run that is about to ask again. No answer is applied or lost by it. Case 2's wording is the one that could mislead an operator about a merge.
  **Why deferred:** the plan says to raise a counter-example and not to work around it, and these files are not 50.1-23's.
  **Settled (orchestrator, wave 17):** the three cases stay, because each is a claim the run cannot classify and the rerun asks again either way. `resume` now says only what is known: "claimed, with no record that it was applied", and at Ship that a merge, version bump or tag may already have run. The event name `resume_found_unapplied_answer` stays, meaning that record is missing. 50.1-25 documents this wording.

- A `stopped` flag written to disk during a gate wait does not stop the run (found during 50.1-24, carried over from 50.1-10)
  status: moved to backlog 999.172
  **What:** the supervisor's safe points read its stop token, not the state's `stopped` field. Measured with a temporary test on a real supervisor parked at a failed Define gate: writing `stopped = true` through `update_state`, then approving the gate. The answer was applied and the stage was launched again (`stage_launched` twice, no `advance_stopped`), so a new agent ran under a state that reads `stopped`. The exit file was absent, because the launch archived it. 50.1-10 predicted the next advance refuses; that last step was not measured here.
  **Reach:** by reading, `devflow stop` does not produce this against a live waiter: arm 1 answers the gate with an abort, and the socket and holder arms set the token or the holder's flag. A flag written by hand, or by a path added later, would. No fix is proposed here.
  **Why deferred:** 50.1-24's reporting covers the token path only (see its SUMMARY). This needs a ruling on whether a safe point should also read the state.
  **Settled (orchestrator, wave 18):** no change in this phase, moved to backlog 999.172. Every production writer of `stopped` was read: each runs in the process that owns the run, or refuses a launch it did not end (`unaddressed_launch_refusal`). So only a hand edit reaches it.

- `stop` over a live pid in the agent pid file cannot record `stopped` until that pid ends (found during 50.1-24, by plan)
  status: open, by plan
  **What:** D-19's no-holder branch uses 50.1-18's `live_phase_agent`, whose second source is the agent pid file. A pid reused by an unrelated process satisfies it, so `stop` exits non-zero with the `ps` check, and keeps doing so while that process lives. The launch guard already refuses `start` and `resume` on the same predicate. The operator's way out is to end the process or remove the pid file; the line does not name the second.
  **Why deferred:** the plan specifies this source, and wording of 50.1-18's shared line is not this plan's to change. 50.1-25's docs should name the case.
