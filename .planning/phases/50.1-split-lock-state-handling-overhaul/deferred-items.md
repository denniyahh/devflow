## Deferred Items

- `agent_running` can report a just-reaped process as running (found during 50.1-06)
  status: open
  **What:** `agents::opencode::tests::spawn_with_timeout_kills_a_descendant_after_its_parent_exits` failed once in a full core run at `06c39fe`: `polls: 0`, still running 237 µs later, and `survivor after: GONE`. So one `agent_running` call said not running and the next said running. `crates/devflow-core/src/agent.rs` `running_per_status` treats only `NotFound` as gone. Measured: reading an already-open `/proc/<pid>/status` after the reap fails with `ESRCH`, and `std::io::Error::from_raw_os_error(3).kind()` is `Uncategorized`, so that read counts as "cannot tell" and `kill(0)`'s answer stands.
  **Likely fix, not tested:** treat `ESRCH` as gone in `running_per_status`.
  **Not established:** the sequence is inferred from those two measurements and the failure's diagnostics, not traced. The test failed 0 of 60 run alone. Whether this also explains backlog 999.151 was not checked; that test fails on a different assertion, though 50.1-05 saw the same not-running-then-running shape there.
  **Why deferred:** 50.1-06 does not touch `agent.rs` or `agents/`, and `opencode.rs` does not use gates.
