## Deferred Items

- `liveness::tests::a_probe_in_flight_neither_reads_live_nor_fails_a_take` can fail under `cargo test` (found during 50.1-12)
  status: open
  **What:** one `scripts/check.sh all` at `d6003e2` failed it at `liveness.rs:805`: `take the exclusive lock: "WouldBlock"`, right after the test dropped its own shared-lock handles.
  **Measured:** 1 failure in 3 `check.sh` runs, 0 in 5 `cargo test -p devflow-core --lib` runs. 50.1-12 changed no `devflow-core` file.
  **Likely cause, not tested:** another test's fork holding a duplicate descriptor of the lock file, the case 50.1-08 moved its lock tests into child processes for. If so, running this test in a child would fix it.
  **Why deferred:** 50.1-12 is CLI test migration only and does not touch `liveness.rs`.

- `hermes_nonzero_exit_does_not_advance` can report its reaped monitor as alive (found during 50.1-12)
  status: open
  **What:** one `scripts/check.sh all` at `d6003e2` failed it at `tests/phase7_cli.rs:1656`: `monitor wrapper pid ... must be verified dead after reaping`.
  **Measured:** 1 failure in 3 `check.sh` runs, 0 in 8 `cargo test -p devflow --test phase7_cli` runs. 50.1-12's diff has no production line and does not touch this test file.
  **Likely cause, not tested:** the `agent_running` entry below, since the panic follows an `agent_running` call right after `terminate_and_verify`. No trace of this failure shows that.
  **Why deferred:** 50.1-12 does not touch `agent.rs` or `phase7_cli.rs`.

- `agent_running` can report a just-reaped process as running (found during 50.1-06)
  status: open
  **What:** `agents::opencode::tests::spawn_with_timeout_kills_a_descendant_after_its_parent_exits` failed once in a full core run at `06c39fe`: `polls: 0`, still running 237 µs later, and `survivor after: GONE`. So one `agent_running` call said not running and the next said running. `crates/devflow-core/src/agent.rs` `running_per_status` treats only `NotFound` as gone. Measured: reading an already-open `/proc/<pid>/status` after the reap fails with `ESRCH`, and `std::io::Error::from_raw_os_error(3).kind()` is `Uncategorized`, so that read counts as "cannot tell" and `kill(0)`'s answer stands.
  **Likely fix, not tested:** treat `ESRCH` as gone in `running_per_status`.
  **Not established:** the sequence is inferred from those two measurements and the failure's diagnostics, not traced. The test failed 0 of 60 run alone. Whether this also explains backlog 999.151 was not checked; that test fails on a different assertion, though 50.1-05 saw the same not-running-then-running shape there.
  **Why deferred:** 50.1-06 does not touch `agent.rs` or `agents/`, and `opencode.rs` does not use gates.

- `devflow ship` cannot see a Ship answer written by `gate approve` until 50.1-14 (found during 50.1-13)
  status: resolved
  **What:** `gate approve` now writes the keyed answer, while `ship_override` still checks and reads the unkeyed response. Probed: approve, then `ship_override`, refuses with "no Ship gate response written yet". It fails closed, and its wording is wrong for that one wave.
  **Who:** 50.1-14 replaced that read with the keyed read and claim. `pipeline_gate::tests::ship_override_ships_an_approval_written_by_gate_approve` failed on this refusal before the change and passes after it.

- `stop_writes_no_state_while_the_lock_holder_survives_the_signal` can fail when `stop` signals its holder before the holder's trap is set (found during 50.1-14)
  status: open
  **What:** one run of the plan's suite gate at `df4c973` failed it at `tests/stop_e2e.rs:158`: `stop must refuse to mark state while the holder survives`, with `signalled pid ...` and then the lock reclaimed as stale. The holder is `sh -c "trap '' TERM; exec sleep 60"`, and the test runs `devflow stop` as soon as `spawn` returns, so a TERM that lands before `trap` kills the shell.
  **Measured:** 1 failure in 3 runs of the integration binary under its suite (the `check.sh all` run and the rerun passed), 0 in 60 run alone. With a temporary `sleep 1;` before the `trap`, the test failed every time it ran (1 of 1), with the same message.
  **Likely fix, not tested:** have the holder write a ready file after its `trap`, and wait for that file before running `stop`.
  **Why deferred:** 50.1-14 did not change this test or `stop`'s holder path, and the test plants no gate.

- One `stop_e2e` failure at `361995e` whose test name was not captured (found during 50.1-18)
  status: open
  **What:** one run of `cargo nextest run -p devflow --test stop_e2e --no-tests=fail` reported `37/38 tests run: 36 passed, 1 failed`. A later loop overwrote the log, so the failing test is unknown.
  **Measured:** 1 failure in 23 runs at that commit: the failure, then 8 clean runs alone and 14 clean runs beside another nextest run. It may be `stop_writes_no_state_while_the_lock_holder_survives_the_signal`, logged above, whose failure rate looks similar, but that is unverified.
  **Why deferred:** it did not reproduce in 22 reruns. 50.1-18 changed `stop_in`, so its changes are not ruled out as the cause. Keep the failing log the next time this happens.
