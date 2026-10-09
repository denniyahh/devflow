---
phase: 49-live-unattended-run-the-milestone-s-instrument
reviewed: 2026-10-09T19:10:00Z
depth: deep
files_reviewed: 5
files_reviewed_list:
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/canary.rs
  - crates/devflow-core/src/gates.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-cli/tests/gate_notify_persist_e2e.rs
findings:
  critical: 1
  warning: 1
  info: 1
  total: 3
status: issues_found
---

# Phase 49: Code Review Report (incremental re-review of 9e78c83)

**Reviewed:** 2026-10-09T19:10:00Z
**Depth:** deep
**Files Reviewed:** 5
**Status:** issues_found

## Summary

Scope: `git diff 798874f HEAD` over the five listed files. The only code commit in that range is 9e78c83. Call chains were traced into `commands.rs::start` (the only writer of `gate_notify_cmd`), `pipeline_launch.rs::resume` and `run_monitor`, `pipeline_gate.rs::run_gate_with_timeout`, `staleness.rs`, and `git.rs::hermetic_command`.

New IDs continue from the prior review: CR-03, WR-10 and IN-09. No prior ID is reused.

**Main finding:** the commit makes the persisted `gate_notify_cmd` the only source for every gate in the detached chain. That value is written only by `devflow start`. Setting `DEVFLOW_GATE_NOTIFY_CMD` at `devflow resume` or `devflow advance` is now silently ignored by the chain, and Auto-mode gates fire in the chain. This applies to a run started without the variable and to a run whose state file predates the field. Before 9e78c83, and before Phase 49, the chain inherited the variable. That is CR-03.

**Do the fixes close their findings?**

| Prior finding | Verdict | Basis |
|---|---|---|
| WR-07 chain freezes the command | **Closed as specified, with two new defects** | Both monitor spawns (monitor.rs:459, :582), the pipe-owning agent (:993) and the canary (canary.rs:374) now remove the variable. Grep shows only two production fire sites, `pipeline_gate.rs:400` and `staleness.rs:467`. Both pass `state.gate_notify_cmd`, so no gate in the chain lost a command that the state holds. The fix exposes CR-03: the chain no longer sees a command that is only in the env. The fix's documented escape hatch (edit the state file while the run is in flight) is reverted by a parked gate's save (WR-10). The e2e the prior review asked for (edit the state between two chain gates) was not added. |
| WR-08 wiring points untested | **Closed by narrowing the claim** (the fix option the prior review allowed) | Mutation run on a `git archive` snapshot of HEAD, `cargo test -p devflow --test gate_notify_persist_e2e`. Baseline: 7/7 pass. Removing the legacy strip (monitor.rs:582) fails `agent_via_legacy_monitor_never_receives_the_notify_variable`. Removing the canary strip (canary.rs:374) fails `agent_via_default_launch_canary_never_receives_the_notify_variable`. Both controls discriminate. Removing the pipe `__monitor` strip (monitor.rs:459) fails **nothing**, and neither does removing the pipe agent strip (:993). The header now names both as uncovered, so the claim is accurate. Residual risk: the default production arm's WR-07 fix is the unpinned site, so reverting it re-freezes the chain with every test green. The prior review's cheap fix still applies: assert `(GATE_NOTIFY_CMD_ENV, None)` in `get_envs()` of the built `__monitor` `Command`. |
| WR-09 test that cannot fail | Closed | It was replaced by `advance_gate_prefers_a_set_variable_over_the_persisted_command`. That test asserts the override ran and the persisted command did not, so reversing precedence or running both fails it. The removed "nothing configured" case is still covered by `resolve_gate_notify_cmd(None, None)`. |
| IN-07 "configured trunk" wording | Closed | No "configured trunk" text is left in prompt.rs. The doc and the test name now say "the trunk, `config::MAIN`". |
| IN-08 non-UTF-8 run lossily | Closed | `to_str()` returning `None` now falls through to the fallback. At `start`, the fallback is `None`, so nothing mangled is persisted. A unit case was added (gates.rs:975-980). The core `gates::` and `monitor::tests::strip` tests pass (28). |

**What the checks do not establish:**

- The mutation runs prove only which e2e tests notice a missing strip. They do not exercise a gate firing inside a real chain. No live chain run was done.
- CR-03 and WR-10 are established by reading the code, not by running them. The basis:
  - `gate_notify_cmd` has exactly one writer (`rg` over `crates/`: commands.rs:346).
  - `resume` (pipeline_launch.rs:1412) loads state and never touches the field.
  - `run_gate_with_timeout` calls `save_state(state)` with the in-memory struct after the poll returns (pipeline_gate.rs:429-432).
- After the runs, no `sleep 60` or snapshot-target processes were left (`pgrep`).

## Critical Issues

### CR-03: A notify command set at `resume`/`advance` never reaches the detached chain, so unattended gates go silent for runs whose state holds no command

**File:** `crates/devflow-core/src/monitor.rs:284-294`, `:459`, `:582`; `crates/devflow-cli/src/commands.rs:346-349` (sole writer); `crates/devflow-cli/src/pipeline_launch.rs:1412` (`resume`, never writes the field)

**Issue:** After 9e78c83, each chain process resolves the command as `resolve_gate_notify_cmd(None, state.gate_notify_cmd)`. That field is written only by `devflow start`. Since the chain is where Auto-mode gates fire, the chain fires nothing whenever the state holds no command, whatever the environment of the command that launched the chain. Concrete paths:

1. **Run started without the variable, exported later.** An operator exports the variable and runs `devflow resume`. The README's route after a gate timeout is "it just needs a `devflow resume`". The resume's own foreground gates fire. Every later chain gate is silent.
2. **Run whose state predates the field.** The field is `#[serde(default)]`, so a run started by any binary before af38901 loads it as `None`. The staleness gate itself tells the operator to "rebuild and reinstall the binary before resuming". After that rebuild-and-resume, the chain is silent even though the variable is exported in every shell.
3. **`devflow advance` run by hand with the variable set.** The foreground gate fires the env command, and the chain this invocation spawns does not. One command now produces two different hook behaviours.

At 798874f and before Phase 49, `hermetic_command` (git.rs:87-94) did not scrub the variable, so the chain inherited it in all three cases. This is a regression of the never-silent guarantee (WR-11) that the hook exists for. It also contradicts OPERATIONS.md:92 ("Set `DEVFLOW_GATE_NOTIFY_CMD` ...; it runs on every gate"), docs/guides/configuration.md:35, and state.rs:432-434 ("A set environment variable still takes precedence"), none of which changed.

The commit also changes how an empty variable behaves. `DEVFLOW_GATE_NOTIFY_CMD=` at `resume` no longer disables the chain's hook.

**Fix:** Keep the state as the chain's single source, but have every foreground entry that launches a chain fold its own env into the state, with the same three-state rule `start` uses. Unset keeps the persisted value, set replaces it, and empty clears it:

```rust
// shared helper, called by start (replacing commands.rs:346-349), resume and
// advance before their first save_state / spawn_monitor:
pub fn fold_notify_env_into(state: &mut State) {
    let env = std::env::var_os(GATE_NOTIFY_CMD_ENV);
    if env.is_some() {
        // set (incl. empty = disable) overrides; non-UTF-8 counts as unset per IN-08
        if let Some(Some(v)) = env.as_deref().map(std::ffi::OsStr::to_str) {
            state.gate_notify_cmd = (!v.is_empty()).then(|| v.to_owned());
        }
    }
}
```

Add an e2e per entry point. It creates a state with `gate_notify_cmd: None`, runs `resume` with `DEVFLOW_GATE_NOTIFY_CMD=touch <sentinel>`, and asserts the persisted field afterwards. As a control, the same run with the variable unset must leave the field `None`. Update OPERATIONS.md, configuration.md and the `State::gate_notify_cmd` doc to say that `start`/`resume`/`advance` capture the variable and that the chain does not read it.

## Warnings

### WR-10: Editing `gate_notify_cmd` while a gate is parked is silently reverted when the gate is answered

**File:** `crates/devflow-core/src/monitor.rs:284-290` (the claim); `crates/devflow-cli/src/pipeline_gate.rs:368-370`, `:429-432`

**Issue:** The new doc comment says that editing `gate_notify_cmd` in the state file "changes or disables the hook for a run in flight". This is the replacement for the env escape hatch that WR-07 removed, and there is no verb for it. However, `run_gate_with_timeout` holds a `&mut State` loaded before the gate fired. It saves the gate, blocks in `Gates::poll_response` for up to `DEVFLOW_GATE_TIMEOUT_SECS` (3 days by default), and then calls `workflow::save_state(state)` with that whole in-memory struct (:431-432). The operator is most likely to edit the field right after a hook fired, because they just saw it misbehave or leak. That is exactly while the process is parked. An edit to `""` or to a new command is overwritten when the operator answers the gate, and every later chain gate fires the old command again. Nothing reports that the edit was lost.

**Fix:** Before the post-poll `save_state`, re-read `gate_notify_cmd` from disk and carry it into the in-memory state:

```rust
Some(response) => {
    if let Ok(on_disk) = workflow::load_state(project_root, state.phase) {
        state.gate_notify_cmd = on_disk.gate_notify_cmd;
    }
    state.gate_pending = false;
    workflow::save_state(state)?;
```

Better: provide a `devflow gate notify --phase N <cmd|--off>` verb that edits under the phase lock, and document it instead of hand-editing. Pin the fix with a test that parks a gate, rewrites the field on disk, answers the gate, and asserts the field survives. As a control, the same run without the rewrite keeps the original value.

## Info

### IN-09: The new chain-environment e2e leaks the detached chain when it fails

**File:** `crates/devflow-cli/tests/gate_notify_persist_e2e.rs:384-420`

**Issue:** `agent_environment_after_define` kills the marked processes only on its success path. If `assert!(approve.status.success())` (:391) or the 20-second wait assertion (:405-408) panics, the detached monitor, the fake agent and its `sleep 60` survive the test. `ReapOnDrop` reaps only the foreground `start`. The leak is bounded (sleep 60, gate timeout 30s), but it appears exactly when the test is diagnosing a regression. A later run's `/proc` scan with a different marker is unaffected, so this does not cause cross-test interference.

**Fix:** Move the kill loop into a guard whose `Drop` runs `devflow stop` and kills `marked_processes(&marker)`. Create the guard before the approve call.

---

_Reviewed: 2026-10-09T19:10:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
