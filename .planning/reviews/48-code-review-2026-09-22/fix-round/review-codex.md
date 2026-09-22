## CONFIRMED

### C-1. `status` can move a live legacy phase’s state without its lock

**File:** snapshot/crates/devflow-cli/src/commands.rs:1055; snapshot/crates/devflow-core/src/workflow.rs:146; snapshot/crates/devflow-core/src/workflow.rs:264

**Mechanism:** `status` calls `workflow::list_states` without taking a phase lock. `list_states` unconditionally calls `migrate_legacy_state`, which renames `.devflow/state.json` to `.devflow/state-NN.json` or deletes the legacy file. A live pre-per-phase binary holding `lock-NN` can therefore lose the state file it still reads.

**Reproduction:** In a copied snapshot, I created a valid legacy state, acquired that phase’s lock, then called `list_states`. `cargo test -p devflow-core workflow::tests::review_probe_list_migrates_a_held_legacy_phase_without_its_lock -- --exact` ran 1 test: the lock remained held while the read moved the state. An older holder’s next legacy-state load fails.

**Severity:** medium — upgrade/status interaction can strand a live legacy run; it violates the stated “every state writer takes the lock” contract.

### C-2. `abort` reports success after failing to clear state

**File:** snapshot/crates/devflow-cli/src/pipeline_gate.rs:461; snapshot/crates/devflow-core/src/workflow.rs:309

**Mechanism:** `abort` prints “workflow aborted,” discards both gate-cleanup and state-clear errors, emits an aborted event, then returns `Ok(())`. A failed state deletion leaves active state while the monitor treats abort as successful.

**Reproduction:** In a copied snapshot, I made the phase state path a directory so `clear_state` fails. The focused probe printed `workflow aborted for phase 486: probe`, returned `Ok`, and the state artifact remained. The exact focused test ran 1 test.

**Severity:** medium — a terminal workflow outcome is falsely recorded and reported, leaving stale recoverable state behind.

### C-3. Explicit clean still claims cleanup for an absent phase

**File:** snapshot/crates/devflow-cli/src/commands.rs:2561; snapshot/crates/devflow-core/src/recover.rs:169; snapshot/crates/devflow-core/src/workflow.rs:309

**Mechanism:** `clean_phase` is intentionally idempotent: absent state, gates, and cron records all produce `Ok`. `recover_cmd` then unconditionally prints `cleaned up workflow state for phase N`.

**Reproduction:** On an empty initialized repository:

```text
$ devflow recover --clean --phase 66 <empty-repo>
cleaned up workflow state for phase 66
$ echo $?
0
```

No state existed before or after. The new E2E tests cover contention and an existing unlocked state, but not this no-op control.

**Severity:** low — operator-facing false success; directly repeats defect class D.

## SUSPECTED

None.

## CHECKED AND CLEAN

- A implicit sweep: `clean_report` takes the per-phase lock before `clear_state` and records only successfully cleared phases; contended phases are preserved. snapshot/crates/devflow-core/src/recover.rs:109
- A explicit clean: the lock spans all five gate cleanups, state deletion, and per-phase cron deletion; only global stale-lock cleanup runs after release. snapshot/crates/devflow-core/src/recover.rs:169
- A post-sweep legacy-corrupt cleanup and orphan-cron pruning are unguarded, but current live writers save state before cron records, and a held phase retains its state; no current live-holder harm found. snapshot/crates/devflow-core/src/recover.rs:141
- State/gate/cron writes during start, resume, advance, and manual Ship inherit a held phase lock. snapshot/crates/devflow-cli/src/commands.rs:360; snapshot/crates/devflow-cli/src/pipeline_launch.rs:1414; snapshot/crates/devflow-cli/src/pipeline_launch.rs:1621; snapshot/crates/devflow-cli/src/pipeline_gate.rs:518
- Gate CLI approve/reject now permits only Live/Unconfirmable holders, plus Ship with NoHolder; Recycled is rejected. snapshot/crates/devflow-cli/src/commands.rs:1414; snapshot/crates/devflow-core/src/lock.rs:57
- `stop` is stricter than the shared rule: it writes only for Live, except Ship/NoHolder; it cannot give a Recycled or Unconfirmable holder a response. snapshot/crates/devflow-cli/src/commands.rs:1924
- `gate sweep` is also stricter: it reaps only for Live, including Ship; this can leave an Unconfirmable waiter untouched but does not create the weaker-holder defect. snapshot/crates/devflow-cli/src/commands.rs:1556; snapshot/crates/devflow-cli/src/commands.rs:1736
- `--yes-ship` auto-response is emitted only by the already-lock-owning advance path. snapshot/crates/devflow-cli/src/pipeline_outcomes.rs:838; snapshot/crates/devflow-cli/src/pipeline_gate.rs:414
- Core `Gates::respond`/`reap` intentionally do not acquire the holder’s lock; their production callers provide either the holder check or the lock-owning pipeline context. snapshot/crates/devflow-core/src/gates.rs:189
- `stop`’s no-lock/no-state exits are explicitly idempotent and say no signal/state action occurred. snapshot/crates/devflow-cli/src/commands.rs:2032; snapshot/crates/devflow-cli/src/commands.rs:2150
- `gate approve|reject` returns an error before publication on refusal and reports success only after `Gates::respond` succeeds. snapshot/crates/devflow-cli/src/commands.rs:1419
- `gate sweep` reports counted outcomes rather than claiming a reap when none occurred. snapshot/crates/devflow-cli/src/commands.rs:1604; snapshot/crates/devflow-cli/src/commands.rs:1726
- Ship and resume refusal paths return errors; doctor is read-only here, with no `doctor --fix` implementation found. snapshot/crates/devflow-cli/src/pipeline_gate.rs:518; snapshot/crates/devflow-cli/src/pipeline_launch.rs:1414; snapshot/crates/devflow-cli/src/commands.rs:3767
- `cleanup` prints removal only after successful worktree removal and accurately reports an empty sweep. snapshot/crates/devflow-cli/src/commands.rs:941; snapshot/crates/devflow-cli/src/commands.rs:964
- The added recovery E2E suite passed 4/4, including held-lock refusal and unlocked-clean controls. It does not establish the absent-explicit-phase case in C-3.


