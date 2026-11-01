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
  status: open
  **What:** `gate approve` now writes the keyed answer, while `ship_override` still checks and reads the unkeyed response. Probed: approve, then `ship_override`, refuses with "no Ship gate response written yet". It fails closed, and its wording is wrong for that one wave.
  **Who:** 50.1-14 replaces that read with the keyed read and claim, and already lists `ship_override`.
