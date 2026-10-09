---
phase: 49-live-unattended-run-the-milestone-s-instrument
reviewed: 2026-10-09T17:53:52Z
depth: deep
files_reviewed: 6
files_reviewed_list:
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/tests/gate_notify_persist_e2e.rs
  - crates/devflow-core/src/gates.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/state.rs
findings:
  critical: 0
  warning: 3
  info: 3
  total: 6
status: issues_found
---

# Phase 49: Code Review Report (incremental re-review)

**Reviewed:** 2026-10-09T17:53:52Z
**Depth:** deep
**Files Reviewed:** 6
**Status:** issues_found

## Summary

This is an incremental re-review of `git diff df0c4ad HEAD` over the six listed files: commits c5d7d46 (WR-03), af38901 (WR-01, WR-02, IN-02) and 53d7d25 (e2e cache isolation). Call chains were traced into `pipeline_gate.rs`, `staleness.rs`, `git.rs::hermetic_command` and the monitor's `advance` tail.

New finding IDs continue from the prior review (WR-07 onward, IN-06 onward). The disposition gate drops a recorded decision when an ID is reused, so reusing WR-01..WR-03 would have erased their `fixed` rows.

**Do the fixes close their findings?**

| Prior finding | Verdict | Basis |
|---|---|---|
| CR-02 (fmt) | Closed | `cargo fmt --all -- --check -l` exits 0 at HEAD. `cargo clippy --workspace --all-targets -- -D warnings` is clean on a `git archive` snapshot of HEAD. |
| WR-01 (a) empty disables | **Partially closed** | Closed for foreground processes. Not closed for the detached monitor chain, where it is now worse than before the fix (WR-07). |
| WR-01 (b) one precedence | **Partially closed** | The foreground and the *first* monitor spawn now agree. Every later process in the chain resolves from an inherited env value, so it never re-reads the persisted state (WR-07). |
| WR-01 (c) constructor reads env | Closed | `State::new` sets `None` (state.rs:607). The only capture is in `start` (commands.rs:346). The test that claims to pin this depends on the environment (IN-06). |
| WR-01 (d) state file is executable input | Not addressed | No change. Recorded as `fixed` with WR-01 as a whole. |
| WR-02 revert-sensitivity | **Partially closed** | Mutation probe on a HEAD snapshot (full `cargo test --workspace --no-fail-fast`). Reverting the `pipeline_gate.rs` fallback fails `advance_gate_runs_the_persisted_command_when_the_variable_is_unset`; this is the positive control and it discriminates. Removing **both** monitor injection calls fails no new test. Reverting the `staleness.rs` fallback to `None` fails no new test. The e2e file header claims "reverting any piece of its wiring fails a test" (WR-08). One of the five new e2e tests cannot fail (WR-09). |
| WR-03 hard-coded `main`/`integration` | Closed | The literal `integration` is gone, and `main` is interpolated from `config::MAIN`. Both renderers are asserted by `ship_prompts_name_the_configured_trunk_and_no_fixture_branch`. Wording nit in IN-07. |
| IN-02 | Closed (by documentation) | Kept as public API, which is reasonable for a published `devflow-core`. The doc now says it ignores the persisted command. |

**What the checks do not establish:**

- The baseline full-workspace run in the scratch snapshot had 7 pre-existing failures in `plan_bashism_scanner.rs`. They appear identically in every mutant run, so they are an artifact of the snapshot rather than of these changes. Mutant verdicts were read as "failures beyond those 7".
- The WR-07 behaviour is established by reading the code: `hermetic_command` does not scrub the variable (git.rs:87-94), and the shape of the `advance` tail. No live chain run was done.
- Prior deferred findings (CR-01, WR-04..WR-06, IN-01, IN-03..IN-05) are outside this diff and are not re-reported.

## Warnings

### WR-07: The detached monitor chain freezes the notify command, so an in-flight run's hook cannot be disabled or rotated (regression for the chain)

**File:** `crates/devflow-core/src/monitor.rs:284-306`, `:471`, `:594`; `crates/devflow-core/src/gates.rs:424-433`; `crates/devflow-core/src/git.rs:87-94`
**Issue:** Precedence is now "a set env var always wins over the persisted value". The first monitor spawn either injects the persisted value or inherits the operator's value. `hermetic_command` scrubs only git variables, so `DEVFLOW_GATE_NOTIFY_CMD` is then **set** in the monitor and in everything it runs. That includes the `devflow advance` tail, which fires the chain's gates, and the next `spawn_monitor_inner`. There `persisted_notify_cmd_for_child(Some(_), _)` returns `None` and the value is inherited again.

From the first spawn on, every process in the chain resolves the frozen env value, and `state.gate_notify_cmd` is never consulted again:
- An operator who exports `DEVFLOW_GATE_NOTIFY_CMD=` in their own shell affects only foreground commands. The detached chain is where Auto-mode gates fire.
- Hand-editing `.devflow/state-NN.json`, to `""` or to a new command, has **no** effect on the chain.

Before af38901, `spawn_monitor_inner` unconditionally re-applied `.env(state value)`. A state edit therefore took effect at the next stage's monitor, and an edit to `""` disabled it via the empty-env fallback. That escape hatch was the one the prior WR-01(a) named, and it is now gone. The only way left to silence a bad or leaking hook is to kill the chain.

The injection is also redundant for DevFlow's own gates. Both production fire sites (`pipeline_gate.rs:400-406`, `staleness.rs:467-473`) already pass `state.gate_notify_cmd` as the fallback, and every chain process loads state from disk. Its only other effect is to place the command, which may embed a webhook token, into the coding agent's environment even when the launching shell did not export it.

The doc comment at monitor.rs:288 ("The chain therefore resolves the same command the foreground does") is true only until the persisted value changes or the operator's env differs.
**Fix:** Make the persisted state the chain's single source. Strip the variable from the detached child and delete the injection helpers:
```rust
// spawn_monitor_inner, both arms, after `.envs(...)`:
child_cmd.env_remove(crate::gates::GATE_NOTIFY_CMD_ENV);
// every chain process then resolves via fire_gate_notify_with_fallback(.., state.gate_notify_cmd)
// with the env unset -> the CURRENT persisted value, re-read at each gate.
```
`start` already folds the operator's env, including set-and-empty, into the persisted value, so nothing is lost. Add an e2e that edits the state file between two chain gates and asserts that the second gate uses the edited value, with a control that leaves it unedited. If the env should still be able to override within the chain, document that the chain is deliberately frozen and provide a verb to rotate the hook. Do not leave state edits silently ineffective.

### WR-08: Two of the four wiring points still fail no test when reverted, contrary to the e2e file's claim

**File:** `crates/devflow-cli/tests/gate_notify_persist_e2e.rs:1-2`; `crates/devflow-core/src/monitor.rs:471,594`; `crates/devflow-cli/src/staleness.rs:467-473`
**Issue:** The file header says it pins the persisted command "so reverting any piece of its wiring fails a test", and the disposition marks WR-02 `fixed` on that basis. Measured on a HEAD snapshot with full `cargo test --workspace --no-fail-fast`, comparing against a baseline run and the `pipeline_gate` positive control:
- **Mutant `monitor`** (both `inject_persisted_notify_cmd(...)` calls replaced with no-ops): no failure beyond baseline. `persisted_notify_cmd_for_child_*` tests only the pure helper, not that either spawn site calls it.
- **Mutant `staleness`** (`state.gate_notify_cmd.as_deref()` replaced with `None` at the stale-build gate): no failure beyond baseline.
- **Mutant `pipeline_gate`** (control): `advance_gate_runs_the_persisted_command_when_the_variable_is_unset` fails at :280, so the harness does discriminate.

**Fix:** If WR-07's fix is taken, the monitor test becomes "the chain child has the variable removed". Assert it on the spawned `Command`'s `get_envs()` (an entry of `(GATE_NOTIFY_CMD_ENV, None)`) through a small extracted builder, for both arms. For staleness, add a test that drives the `StalenessOutcome::Block` arm with the env unset and a persisted `touch <sentinel>`, asserting the sentinel exists, plus a `None` control. Until then, narrow the file header's claim to the two call paths it actually pins.

### WR-09: `advance_gate_runs_nothing_without_a_persisted_command` cannot fail

**File:** `crates/devflow-cli/tests/gate_notify_persist_e2e.rs:283-298`
**Issue:** `sentinel_dir` is created and then never referenced by any command: `persisted` is `None` and the env is removed. Nothing in any code path could write into a fresh, unnamed temp directory, so the assertion holds by construction, whatever the notify resolution does. `notify_fired` is emitted unconditionally after the fire call (pipeline_gate.rs:407-412), so the wait also succeeds whether or not a hook ran. The test reads as a negative control but controls nothing. The real negative arm is `advance_gate_with_an_empty_variable_skips_the_persisted_command`.
**Fix:** Give the test a command that would leave evidence if a stale source leaked through. For example, write `touch <sentinel>` into a source that must be ignored: a pre-existing state field from a previous run that `--force` must not carry, or the parent's own env with only the child's env removed. Then assert the sentinel is absent. Otherwise delete the test rather than keep a test that is green by construction.

## Info

### IN-06: The `State::new` purity assertion only fails when the developer's shell exports the variable

**File:** `crates/devflow-core/src/state.rs:1366-1369`
**Issue:** `assert_eq!(state.gate_notify_cmd, None, "State::new must not capture a command")` detects a revert to the env-reading constructor only if `DEVFLOW_GATE_NOTIFY_CMD` is set in the test process. In CI and in the scratch run it is unset, so the revert passes. The guarantee holds today by reading `State::new`, not because this assertion enforces it.
**Fix:** Run the assertion in a child process with the variable set, for example via the cli crate's `run_test_in_child` pattern or a tiny integration test that spawns with `.env(GATE_NOTIFY_CMD_ENV, "x")`. Alternatively, drop the claim from the test's message.

### IN-07: "Configured trunk" wording names a hard-coded constant

**File:** `crates/devflow-core/src/prompt.rs:236-251`, `:784`
**Issue:** The doc comment and the test name say "configured trunk", but `config::MAIN` is `pub const MAIN: &str = "main"` (config.rs:39), and no setting can change it. A reader may look for a config key that does not exist. A repository whose trunk is `master` would be told to target a nonexistent `main`. That assumption is project-wide (git-flow uses the same constant), so it is not new here, but the wording hides it.
**Fix:** Say "the fixed production branch ([`crate::config::MAIN`])" in the doc and the test name.

### IN-08: Non-UTF-8 env values are now run (and persisted) lossily instead of treated as unset

**File:** `crates/devflow-core/src/gates.rs:430`, `crates/devflow-cli/src/commands.rs:346-349`
**Issue:** The previous code used `std::env::var`, which returns `Err(NotUnicode)`, so a non-UTF-8 value fell through to the fallback. `resolve_gate_notify_cmd` now converts the value with `to_string_lossy`. A command containing invalid bytes is executed with U+FFFD substitutions, and `start` persists that mangled string into the state file. This is an edge case, but it is a silent behaviour change.
**Fix:** Treat `value.to_str() == None` as an error to warn about, or as unset, rather than running a different command than the operator wrote.

---

_Reviewed: 2026-10-09T17:53:52Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
