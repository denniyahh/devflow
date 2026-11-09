## FINDINGS

Static reading only — I ran no tests or builds (per your constraint). Everything below is a claim about the text of the plan against the source in this snapshot; the two places I could not settle statically are labelled **UNVERIFIED**.

---

### 1. E3 races a window that the fix itself shrinks to ~3 s, and a missed window fails with the same `WR-03:` marker a real regression produces

`50-26-PLAN.md:240` (E3): *"The test polls the socket's ping for up to 30 s until `owner` is `advancing`, then calls `supervisor::request_shutdown(&socket, 20 s)`."*

- The `advancing` state is entered inside `StopToken::observe_exit` (`crates/devflow-core/src/supervisor/runtime.rs:256-262`) and the plan's fix ends it after `READER_DRAIN_WAIT` = `TERMINATE_VERIFY_WAIT` = **3 s** (`crates/devflow-core/src/agent.rs:97`) plus a 50 ms poll (`agent.rs:102`), after which the advance runs and `Supervisor::finish` (`runtime.rs:595-611`) removes the socket. **Before the fix the window is infinite; after the fix it is a 3-second wall-clock race** that the test process must win with a ping poll, a connect and a write.
- Nothing in the plan widens or pins it, and the repo's own mechanisms for exactly this cannot be reached here: `monitor::monitor_test_hold` is `#[cfg(test)]` (`crates/devflow-core/src/monitor.rs:1421`), `pipeline_launch::test_hold` is `#[cfg(test)]` (`crates/devflow-cli/src/pipeline_launch.rs:1153`), and `runtime::stopped_outcome_test_hold` returns immediately unless `stop_requested` (`runtime.rs:348-351`) — it can never hold a **Completed** run, which is what E3 needs. `common::hermetic_devflow` runs `env!("CARGO_BIN_EXE_devflow")` (`crates/devflow-cli/tests/common/mod.rs:80-94`), i.e. a target the `cfg(test)` hooks are compiled out of.
- The failure mode is the bad one: if the window is missed, the advance takes `pipeline_gate::transition`'s `stop_until` arm (`crates/devflow-cli/src/pipeline_gate.rs:68-90`), `finish` replies `handed_off`, and `request_shutdown` returns `ShutdownOutcome::HandedOff` (`crates/devflow-core/src/supervisor.rs:784-788`). The plan's assertion then panics **"with a message starting `WR-03:`"**. A scheduling flake therefore reports as the defect that was just fixed — and the revert check's pass/fail key (`rg -q -F "WR-03:"`, plan line 391) cannot tell that apart from genuine discrimination.

*What the plan needs:* either a deterministic hold compiled into the binary under `test-support` (a new `feature = "test-support"` hold in `run_monitor` between `run_supervised_stage` and the advance — the `stopped_outcome_test_hold` precedent, `runtime.rs:343-362`), or a bound wide enough to be schedulable (and then say what it is), plus a *distinct* marker for "fixture stopped answering" vs "the bound expired".

---

### 2. Criterion 4's "records exit code 143" is false on disk in exactly the new path, and the plan records `WR-03 | fixed` against it

`50-26-PLAN.md` success criterion: *"a socket stop taken then ends it at safe point 1 with 143, no advance and the socket removed"*; must_have line 25 quotes criterion 4 as holding.

- In the E3 path the agent **completed**, so `run_supervised_stage` writes `AgentOutcome::Completed(0)` to the exit file (`crates/devflow-core/src/monitor.rs:1347-1351`). E1 itself asserts that same file reads `0` (plan line 238). Nothing is signalled (`on_stop_request(Advancing)` is `FlagForSafePoint`, `runtime.rs:122-129`).
- The `143` exists only as a constant in the reply: `Reply::Stopped(ShutdownStopped { exit_code: STOPPED_EXIT_CODE, advance_suppressed: true })` (`runtime.rs:597-601`, `STOPPED_EXIT_CODE = 128 + SIGTERM`, `runtime.rs:41`). `State` has no exit-code field at all (`rg exit_code crates/devflow-core/src/state.rs` → nothing), and `record_advance_stop` (`pipeline_launch.rs:1063-1080`) records none.
- So after an E3 stop the durable record says "agent exited 0, run stopped during advance" while the operator was told 143 and "advance_suppressed". This is a **new instance of the class the operator already deferred as backlog 999.166 item 6 (IN-03: *"`stop`'s success line says 'agent group ended' for a stop honoured at a safe point, where nothing was signalled"*, `.planning/ROADMAP.md:~1252`)** — the plan neither extends IN-03 nor says it is accepting a second instance. Criterion 4's "records exit code 143" does not hold for this run under any reading; the plan should say so rather than assert the criterion.

---

### 3. The e2e lane's causal control is a tautology: `is_same_process(pid, start)` with `start` read at assertion time asserts nothing, and the same idiom in the fixture `Drop` would kill a recycled pid

Plan line 238 (E1): *"the holder is still alive (`devflow_core::agent::is_same_process`)"*; line 240 (E3): *"the holder is still alive at the monitor's exit"*; Task 2 fixture `Drop`: *"SIGKILLs the holder by its recorded pid, only while `devflow_core::agent::is_same_process(pid, start)` holds"*.

- The script records **only `$!`** (line 238: *"records `$!` (temp then `mv`)"*) — no start time.
- `is_same_process(pid, expected_start)` is literally `process_start_time(pid) == Some(expected_start)` (`crates/devflow-core/src/agent.rs:270-272`), and `process_start_time` reads `/proc/<pid>/stat` now (`agent.rs:214-221`). If the plan's `start` is read by the test after the monitor exits (the only available moment, since the file holds a bare pid), then `is_same_process(pid, process_start_time(pid))` is a self-comparison — it asserts "some process still has that pid", which is exactly the proxy measurement the assertion exists to avoid, **and it is the control the plan leans on for "the bound, not the holder's death, ended the wait"** (must_have line 27).
- The same expression in `Drop` is worse: read-then-compare-against-itself is not identity validation, so a recycled pid passes and gets SIGKILLed — contrary to the plan's own stated rule on line 163/B0: *"by the recorded pid, never by a name pattern"*, and to `agent.rs:205-213` ("must not be used to distinguish a parent from a child it just spawned").
- Task 1's `HeldStdout` gets this right (*"start time from `agent::process_start_time`; panic if the pid or its start time cannot be read"*, before release). Task 2 silently drops the discipline. The fix is one line — have the script write `/proc/$!/stat` field 22 next to `$!` (or have the parent read it while the holder is provably alive, before the monitor returns) — but as written E1/E3's aliveness assertion is unfalsifiable.

---

### 4. The e2e fixture instruction collides with the target file's existing `init_repo`, and the difference between the two is exactly what makes E1's advance reachable

Plan line 239: *"add a small `__monitor`-direct fixture modelled on `auto_chain_flag_e2e.rs`'s `Fixture` (copy its `init_repo` shape as a new helper here, and leave that file untouched)"*.

- `crates/devflow-cli/tests/supervisor_e2e.rs:41` **already defines** `fn init_repo(root: &Path, phase: PhaseId)` — same name, same signature as `crates/devflow-cli/tests/auto_chain_flag_e2e.rs:48`. A literal copy into the same module is a duplicate definition (E0428); "as a new helper" is ambiguous about which one E1 uses.
- The two differ in the way that matters: the target file's version (lines 41-60) creates **only `develop`** and writes `.planning/ROADMAP.md`; auto_chain's version creates **`feature/phase-NN` one commit ahead of `develop`**, which its own comment says is *"what lets Layer 2 classify the run `Success` rather than `Failed — no work done`"* (`auto_chain_flag_e2e.rs:52-53`). E1 needs the `--until code` halt from `pipeline_gate.rs:68-90`, which is only reached after `evaluate_layer1` is satisfied.
- Consequence: if the implementer reuses the existing `init_repo`, E1 fails **on the fixed tree too** — and on the reverted tree it still fails at the 30 s `WR-03:` timeout, so the revert check prints `REVERT fail_first=E1 rc=100 marker=yes` and the gate reports a discriminating test that is simply broken. The plan's stated remedy for a non-discriminating test (*"fix the test, not the gate"*) does not cover a test that fails on both trees.

---

### 5. The new env knob is deliberately shaped to be invisible to `doc_check`, and the plan books that as compliance

Plan line 163: *"`STAGE_IDLE_SECS_ENV` (`"DEVFLOW_TEST_STAGE_IDLE_SECS"`). Read it the way the others are read: `std::env::var_os` on the const, never a string literal, so `doc_check` (which flags only literal reads) stays as it is for the nine existing harness consts."* — and must_have line 31: *"no literal `DEVFLOW_*` env read … is added, so `doc_check` needs no new entry"*.

- The blind spot is real and narrow: `source_read_env_vars` collects `DEVFLOW_*` tokens then keeps only those with a **literal** read (`crates/devflow-core/src/doc_check.rs:223-233`), so a const-mediated read is invisible. The repo already carries allowlist entries for three exactly-equivalent test-only variables (`doc-check-allowlist.toml:60,65,70`: `DEVFLOW_TEST_MONITOR_FULL`, `DEVFLOW_TEST_HOLD`, `DEVFLOW_TEST_STOPPED_OUTCOME_HOLD`), i.e. the project's policy answer exists and is a one-line edit.
- CLAUDE.md's rule is about `var_os(` reads, and the plan satisfies it by the letter while stating out loud that it picked the shape the gate cannot see. That is a **vacuous gate** (the repo-trap category: "gates that pass vacuously"), asserted as compliance in a `must_haves` entry, and it propagates the blind spot to the tenth harness const instead of closing it.

---

### 6. `join_within` is not a bound if acquiring the capture mutex blocks — the same hang, one level down, untested

Plan line 191: *"If either is still running, it takes the capture handle out under the lock (setting it to `None` drops the `File`)"*, and the reader *"For each line, lock it … On `Some`, append and flush exactly as today, then unlock and send the line."*

- The reader therefore holds the same `Mutex` across `writeln!(capture, …)` **and** `capture.flush()` (`crates/devflow-core/src/monitor.rs:1080-1088` today). `join_within`'s `lock()` is an unbounded `std::sync::Mutex` acquisition. If that write blocks (capture on a hung/network/FUSE mount, a stalled 9p/virtiofs worktree — `.devflow/phase-NN-stdout` lives inside the project root, `monitor.rs:1291-1300`), `join_within` never returns and `run_supervised_stage` hangs again — now **inside** the fix, with `READER_DRAIN_WAIT` having expired.
- No test can catch it: every fixture's capture is a local `tempfile`. The cheapest correct shape is `try_lock` with the remaining budget, and treating "lock held past the deadline" as `is_finished == false` (detach, warn) — the plan does not consider it, and its threat model (T-50-96) claims the close "so no write can follow", which is only true if the lock is acquirable.
- **UNVERIFIED** in practice: I did not reproduce a blocking capture write. The class, not the instance, is the finding: a bound whose enforcement depends on an unbounded wait is not a bound, which is precisely what WR-03 is.

---

### Also, in passing (short)

7. **`must_haves` line 30 over-claims RED coverage.** *"Each fail-first test fails on the unfixed code at its own `WR-03:` assertion … at the test-only RED commit"* — D, E1 and E3 are added in Task 2, **after** the `fix(50-26)` commit, so they are never executed against unfixed code at the RED commit; only A and C are. Task 2 substitutes a synthetic revert. That may be acceptable, but the must_have as written cannot be satisfied.
8. **The rejection of alternative (a) does not price the combination.** Line 129-131 rejects killing leftover group members because *"It cannot reach a holder outside the agent's group … so it does not close the class."* The plan's own accepted DoS, T-50-97 / 999.166 item 7 (*"every line still arriving on the agent's stdout restarts the idle wait"*, `monitor.rs:1060-1076`, `1093-1105`), is caused by a writer **inside** the group — the population (a) does reach, and that the bound alone cannot help because `drive_stream_io` never returns at all. Bounding the join *and* killing in-group members closes strictly more than either alone; the plan files item 7 as `accept` without noting that the combination it rejected-by-implication would have covered it.
9. **E3's phase-74 fixture vs. `advance_stopped` assertion.** `record_advance_stop` emits `{"stage", "safe_point"}` (`pipeline_launch.rs:1069-1075`) and E3 asserts exactly one such event with `safe_point` 1 — correct — but it also asserts *"no `workflow_finished` event with reason `stopped_at` exists"*. On the fixed tree both events can be emitted only in different arms, so this holds; on a **fixed** tree where the stop lands after safe point 1 but before `transition`, the `--until` arm runs instead and E3 fails with the `WR-03:` label (same defect as finding 1, narrower).

---

**What these findings do not establish.** I attacked and could **not** break: the reuse of `TERMINATE_VERIFY_WAIT` as a private const (no `pub` change, `semver-gate` unaffected); the `--fault-current` control in Task 3 (`check-supervisor-evidence.sh:316-346` does select the newest `fault-*` whose `src_tree` equals `HEAD:crates …`, so `FAULT_CURRENT dir=ABSENT` is the correct pre-run answer); the `setsid` precedent for D (`monitor.rs:5733-5742`, and `/usr/bin/setsid` exists); `--idle-timeout-secs 2` having no floor (`pipeline_launch.rs:1353-1373`); Layer 1 accepting E1's JSON envelope (the envelope is decoded before `marker_from_line`, `agent_result.rs:344-357`, `2211-2252`); and the `supervise_stream_child` edit surviving `git revert` (the revert restores `join()` exactly). Those are coverage statements, not endorsement — the six findings above are all in the plan's *own* load-bearing paths: the test that proves the fix, the criterion it claims, the control it uses as evidence, the fixture that must exist for either to run, the gate it says it passes, and the bound itself.
