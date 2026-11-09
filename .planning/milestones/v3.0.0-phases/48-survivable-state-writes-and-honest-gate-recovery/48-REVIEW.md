---
phase: 48-survivable-state-writes-and-honest-gate-recovery
reviewed: 2026-09-23T23:27:57Z
depth: deep
files_reviewed: 4
files_reviewed_list:
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/tests/start_lock_e2e.rs
  - crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs
findings:
  critical: 0
  warning: 4
  info: 5
  total: 9
status: issues_found
---

# Phase 48: Code Review Report (plan 48-20)

**Reviewed:** 2026-09-23T23:27:57Z
**Depth:** deep
**Files Reviewed:** 4
**Status:** issues_found

> This report replaces the 48-18/48-19 review (CR-01, WR-01..WR-06, IN-01..IN-05). That report is in git history
> (`git log -- .planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-REVIEW.md`). Its WR-04
> (`resume` has no live-run check) is the gap 48-20 closes. The findings below are only about the 48-20 diff.

## Summary

Scope: `git diff 60da104..HEAD` of the four files. This covers commits `bb639f5` (48-19 fragment pins), `97a1c94`
(48-20 RED tests) and `fa04bfe` (48-20 GREEN guard). I traced these callees and callers in source:
`pipeline_launch::resume`, `lock::acquire` (including the `.coord` inode), `workflow::load_state` and
`migrate_legacy_state`, `agent_pid_from_file`, `agent::agent_running`/`is_zombie`, `spawn_agent_and_record`
(where it clears `monitor_pid`), `transition`'s `--until` clear, `archive_phase_files_with_stamp` (the only
remover of the agent pid file), `liveness`/`recovery_hints`, doctor's `check_dead_agent`/`check_dead_monitor`,
and `ship::build_single_agent_cron_instructions`. `pipeline_launch::resume` has exactly one production caller,
`main.rs:612`. No production code spawns `devflow resume`: the only non-CLI source of that command is the Hermes
cron text in `ship.rs:277-299`.

Answers to the orchestrator's four questions. Each answer is labelled with how it was established.

1. **Does `resume` refuse over a live recorded pid in every path?**
   - **Loadable state:** yes. Both refusal arms pass at HEAD (12/12 `start_lock_e2e`, run this session).
   - **No state file:** the carve-out returns Ok, then `load_state` fails with MissingState. Nothing launches.
     The test asserts this.
   - **Unloadable state:** nothing launches either way. I checked this with a manual probe of the HEAD binary
     against a corrupt `state-61.json`:
     - With a live agent pid, the probe printed the resume refusal (rc=1).
     - With a dead agent pid, it printed only `state JSON failed: …` (rc=1).
     - The state file's sha256 was unchanged in both runs.
   - **The gap on an unloadable state:** a live monitor is invisible there, and the operator gets a parse error
     with no live-run warning (999.139, backlog). No test covers any part of this branch (WR-01).
2. **Does the refusal leave state and pid files byte-identical?** Yes, for the state file, the agent pid file and
   the phase's gate files. The tests assert it. Mutant M2 below proves the snapshot comparison is sensitive to
   bytes. "Nothing was written" is still not literally true (IN-03).
3. **Can the tests fail against a reverted fix?** Yes. I ran mutants in a scratch copy (`git archive HEAD`, a
   separate target dir, deleted afterwards). The unmutated scratch copy passed 4/4 first.
   - **M1** deleted the guard call. Both refusal arms turned red with their named reasons ("resume must refuse a
     phase whose recorded monitor/agent is alive"). The controls stayed green.
   - **M4** disabled the no-state carve-out. The no-state test and the 48-19 start carve-out test turned red.
   - **M2** moved the guard to just before `launch_stage`. Only the stopped-agent arm caught it. The monitor arm
     passed (WR-02).
   - **M5** skipped the guard on any load error. It **survived all 12 tests** (WR-01).
4. **Does the shared guard change `start`?** No. In the diff of `refuse_launch_over_a_live_run`, the only change
   is that the verb now selects the message. The start call site at `commands.rs:370` is unchanged, between
   `_phase_lock` and `Gates::cleanup`. `start_live_run_refusal` is the old body with a new name. The start golden
   test and the four 48-19 start arms pass.

I found no Critical defects. The guard is correct where it runs, and the tests discriminate the obvious revert. The
weaknesses are two branches the tests do not pin, and two copies of the safety text that must be kept in step.

Not re-scored, still open by operator decision: H1-H10 production hints (999.142). `status` (Stuck), doctor's
`check_dead_monitor`/`check_dead_agent` (`commands.rs:3742/3801/3826`) and cleanup (`commands.rs:934`) still send
the operator to `devflow resume`. In the states this guard covers, that resume now always refuses. The refusal
text says so accurately (I checked the `liveness` matrix at `commands.rs:1012-1021`). Recycled-pid false
refusals are deferred to 999.143, and the lock-free window stays 999.136.

## Warnings

### WR-01: The unloadable-state branch of the shared guard is untested for both verbs; the mutant that deletes it survives the suite

**File:** `crates/devflow-cli/src/commands.rs:2469-2477`, `crates/devflow-cli/tests/start_lock_e2e.rs` (no arm)

**Issue:** When the state file exists but does not load, `refuse_launch_over_a_live_run` still checks the agent
pid file. The 48-20 plan's prohibition relies on that: "when the agent pid file names a live process, resume now
prints the live-run refusal instead of the load error".

Mutant M5 replaces the condition with `if loaded.is_err() { return Ok(()) }`. It passed all 12 `start_lock_e2e`
tests. No other test file references the refusal text: `rg` finds it only in `start_lock_e2e.rs` and
`commands.rs`, and `commands.rs` has no unit test of the guard.

- **Impact on `resume`:** only the error message changes, because `load_state` at `pipeline_launch.rs:1428` still
  fails.
- **Impact on `start`:** the same mutant is a real safety regression. `start` does not fail on a state it cannot
  load. `carried_phase_failures` (`commands.rs:283-300`) warns and restarts the failure budget at zero, and `start`
  then saves a fresh `State`. So with a corrupt state file and a live agent, M5's `start` would overwrite the state
  and launch a second agent beside the live one (reasoned from source; I did not run it), and no test would
  notice (verified: M5 survived). That holds with `--no-worktree` or
  `--force`. Without `--force`, a worktree-mode start would stop later, at the existing-worktree check.

The SUMMARY lists this branch as untested. It is the only line of defence on that path for `start`.

**Fix:** add e2e arms that plant a non-JSON `state-NN.json`:

```rust
// unloadable state + live agent pid → both verbs refuse, and the corrupt file stays byte-identical
fs::write(devflow_core::workflow::state_path(root, phase), b"{\"not\":\"a state").unwrap();
write_agent_pid(root, phase, agent.pid());
// assert: start → "refusing to start", resume → "refusing to resume", snapshot unchanged
// control: agent pid NEVER_LIVE_PID → resume stderr contains "state JSON failed", no "refusing to"
```

### WR-02: The guard's placement before any write is pinned only by a one-off plan gate; the tests catch a late guard only when the state is already stopped

**File:** `crates/devflow-cli/src/pipeline_launch.rs:1427`, `crates/devflow-cli/tests/start_lock_e2e.rs:825-891`

**Issue:** The plan requires the guard to run after `lock::acquire` and before `load_state`. That means before the
`--agent` handoff (`save_state` plus `agent_handoff`), before `repair_leaked_auto_chain_flag` (which writes and
commits `.planning/config.json`), and before `save_state`. The only check on this ordering was a line-number probe
in the plan gate. Nothing durable checks it.

- **M2** moved the guard to just before `launch_stage`, after `repair_leaked_auto_chain_flag` and `save_state`.
  The monitor arm (`stopped=false`) still passed, because `save_state` rewrites identical bytes. Only the
  stopped-agent arm failed, and only because the stop mark was cleared.
- **Unexercised writes:** the tests never pass `--agent`, and the snapshot (`phase_file_snapshot`, `:348`) does not
  cover `.planning/config.json`, git HEAD, or any event except `stage_launched`. So a guard placed after the
  handoff block or after the auto-chain repair is indistinguishable in tests whenever the state is not stopped.
- **Harm this hides:** a late guard over a live pipe-owning run would clear that live run's `_auto_chain_active`
  flag, commit the change, and emit `auto_chain_flag_repaired`, all before refusing. That breaks the chain-flag
  guard of the run it is supposed to protect.

The matrix also lacks three cells: both pids live (the two-role `monitor pid X, agent pid Y` rendering), monitor
live with `stopped=true`, and agent live with `stopped=false`.

**Fix:** extend the monitor arm to exercise the writes that precede the current guard position:
- plant `.planning/config.json` with `workflow._auto_chain_active: true`;
- run `resume --agent <other>` with a fake binary for that agent;
- assert that the config bytes and `git rev-parse HEAD` are unchanged, and that there is no `agent_handoff` or
  `auto_chain_flag_repaired` event.

Add one arm with both pids live to pin the two-role message.

### WR-03: The start and resume refusal texts are two ~40-line copies of the same safety guidance

**File:** `crates/devflow-cli/src/commands.rs:2498-2538` and `:2540-2580`

**Issue:** `start_live_run_refusal` and `resume_live_run_refusal` duplicate:
- the `named` and `checks` builders;
- the `stop` paragraph;
- the identity-check paragraph;
- the pid-alone warning;
- the monitor-first/advance-child paragraph;
- the "do neither while live" sentence.

These sentences needed three review rounds to get right (48-REVIEW CR-01, WR-01, WR-02, and T-48-19-06/07). The
next correction must be made in both copies. The tests pin selected fragments of each, so a change to an unpinned
sentence in one copy will drift silently. Only `start`'s text has a byte-for-byte golden.

**Fix:** build the shared paragraphs once and pass in the parts that differ by verb: the verb word, "as soon as
this {verb} exits", the stopped-state paragraph (resume only), and the repair paragraph. Keep
`start_refusal_text_stays_byte_identical_for_a_live_monitor` to prove `start`'s bytes do not change. Add a resume
golden so the refactor cannot change resume's bytes either.

### WR-04: The no-state carve-out test passes on any failure of `resume`

**File:** `crates/devflow-cli/tests/start_lock_e2e.rs:929-960`

**Issue:** `resume_without_phase_state_reports_the_missing_state_not_a_live_run` asserts only three things:
- the exit is non-zero;
- stderr lacks "refusing to resume";
- the snapshot is unchanged.

It never asserts the missing-state error its name promises. It would pass if `resume` failed for an unrelated
reason, for example a CLI argument change that rejects `--legacy-claude-launch` or a lock error. In that case it
would no longer show that the carve-out is reached. M4 shows it discriminates today, but it has no positive
control.

**Fix:** assert on the MissingState error text. `WorkflowError::MissingState` displays as
`no active DevFlow state at <path>` (`workflow.rs:37`), so assert
`stderr.contains("no active DevFlow state at")`.

## Info

### IN-01: The golden test's "sensitive to the launch verb" assertion always passes

**File:** `crates/devflow-cli/tests/start_lock_e2e.rs:984-989`

**Issue:** The assertion is
`!stderr.contains(&expected.replacen("refusing to start", "refusing to resume", 1))`. It runs after
`stderr.contains(&expected)` has passed, so it can only fail if stderr holds both texts. It proves nothing about
sensitivity. The earlier `contains(&expected)` assertion already fails on a resume text.

**Fix:** delete it. Alternatively, make it a real negative control: render the golden with the wrong verb and
assert that the equality check fails.

### IN-02: `live_run_refusal`'s doc comment still describes only `start`

**File:** `crates/devflow-cli/src/commands.rs:2488-2490`

**Issue:** The comment says "The refusal text for [`refuse_start_over_a_live_run`]. It fires while `start` holds
the lock". The function now dispatches both verbs.

**Fix:** reword it to cover both `start` and `resume`, or point it at `refuse_launch_over_a_live_run`.

### IN-03: A refused resume still writes to `.devflow/` (carried from the prior IN-03)

**File:** `crates/devflow-cli/src/commands.rs:2552`, `crates/devflow-cli/tests/start_lock_e2e.rs:810`

**Issue:** I probed a refused resume against a fresh `.devflow/`. It left `.devflow/.lock-61.coord` behind: the
coordination inode, which by design is never removed (`lock.rs:187`). It also left `.devflow/.gitignore`. The
guard's `load_state` can also migrate or remove a legacy `state.json`.

The test asserts only that `lock-NN` is absent, and the SUMMARY calls the lock "transient". The coord file is not
transient. All of this is harmless, but the refusal's "nothing was written" clause is broader than what holds.

**Fix:** narrow the clause to "state, pid and gate files are unchanged", or accept this as documented.

### IN-04: `resume` loads the state twice, and the guard's load has a side effect

**File:** `crates/devflow-cli/src/commands.rs:2469`, `crates/devflow-cli/src/pipeline_launch.rs:1428`

**Issue:** The guard's `load_state` result is discarded, and `resume` reads the file again one line later. Under
the lock only lock-free writers such as `stop` or `recover --clean` can change the file in between, so the risk is
small. The two reads can still disagree. The first read also runs `migrate_legacy_state`.

**Fix:** have the guard return the loaded `State`, or accept a `&Result<State, _>`, and let `resume` reuse it.

### IN-05: The Hermes cron resume is one-shot, so a false refusal on that path is never retried

**File:** `crates/devflow-core/src/ship.rs:299-300`, `crates/devflow-core/src/agent_result.rs:3184`

**Issue:** The cron job is built with `once: true`. The agent pid file survives the whole rate-limit pause,
because only the next launch archives it. So a recycled agent pid (999.143) makes the single cron `resume` refuse,
and that phase stays paused.

The operator sees this only through `status`'s cron-pending hint. That hint recommends `devflow resume` again,
which refuses again. The SUMMARY describes this path as "refused once", which undersells it: nothing fires a
second time.

The existing `sleep 60` zombie flake (prior IN-04) also applies to the new resume refusal arms. They use the same
`LiveProcess`.

**Fix:** record the one-shot consequence in 999.143. Consider having the cron-pending hint name the refusal case.

---

_Reviewed: 2026-09-23T23:27:57Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
