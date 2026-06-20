---
phase: 11-remediation
plan: 01
type: execute
wave: 1
depends_on: [11-refactor-gsd-native]
files_modified:
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-cli/src/main.rs
autonomous: true
must_haves:
  truths:
    - "consecutive_failures persists across devflow advance calls (state.json contains the field)"
    - "Auto-mode gate fires after 3 consecutive Validate failures without requiring process continuity"
    - "Killing devflow between ack and save_state does not leave gate_pending:true + no response file"
    - "Divergence check runs before any git mutation in start()"
    - "Agent stderr is captured to .devflow/phase-NN-stderr.log, not /dev/null"
    - "cargo test passes with zero regressions"
    - "cargo clippy -- -D warnings is clean"
  artifacts: []
---

# Phase 11-Remediation Plan — Fix 5 Criticals

**Branch:** `feature/phase-11` (continue on same branch)
**Gate:** `cargo test && cargo clippy -- -D warnings && cargo fmt -- --check`

---

## Task 11r-A — Fix CR-02: Persist `consecutive_failures`

**Critical:** CR-02 + CR-05 (subsumed)
**Files:** `crates/devflow-core/src/state.rs`
**Estimated:** ~15 lines changed, 1 commit

### What to Change

**`crates/devflow-core/src/state.rs:33`** — Remove `#[serde(skip)]`:

```rust
// BEFORE (broken):
/// ...
/// Not persisted: always starts at 0 on monitor restart (runtime-only).
#[serde(skip)]
pub consecutive_failures: u32,

// AFTER (fixed):
/// Consecutive Validate failures — drives the Auto-mode forced gate after
/// [`crate::mode::MAX_CONSECUTIVE_FAILURES`] failures. Persisted across
/// `devflow advance` invocations so the counter survives monitor restarts.
pub consecutive_failures: u32,
```

Update the doc comment to remove the incorrect "Not persisted / runtime-only" claim.

### Test Update Required

The test `consecutive_failures_is_runtime_only_not_persisted` (around line 181 in
`state.rs`) currently asserts:
- The field is NOT in the serialized JSON
- The field resets to 0 on deserialization

This test must be rewritten to assert the CORRECT behavior:
- The field IS in the serialized JSON when non-zero
- The field round-trips correctly through serde
- A state with `consecutive_failures = 3` serializes and deserializes to 3

Rename the test to `consecutive_failures_persists_across_advance_calls`.

Also update `state_serde_round_trips` (around line 170) if it asserts anything
about `consecutive_failures` being absent from JSON.

### Verification Gate

```
cargo test -p devflow-core -- state
```
Must pass. Specifically: `consecutive_failures_persists_across_advance_calls` must
pass, and the old `_is_runtime_only_not_persisted` test must no longer exist.

```
cargo test
```
Full suite must pass.

---

## Task 11r-B — Fix CR-04: Swap `save_state` before `ack` in `run_gate()`

**Critical:** CR-04
**Files:** `crates/devflow-cli/src/main.rs`
**Estimated:** ~5 lines changed, 1 commit

### What to Change

**`crates/devflow-cli/src/main.rs:513–517`** — In `run_gate()`, swap the order
of `gate_pending = false`, `save_state`, and `Gates::ack`:

```rust
// BEFORE (unsafe ordering):
Some(response) => {
    Gates::ack(project_root, state.phase, stage)?;   // ack first
    state.gate_pending = false;
    workflow::save_state(state)?;                    // save last
    Ok(GateAction::from_response(&response))
}

// AFTER (safe ordering):
Some(response) => {
    state.gate_pending = false;
    workflow::save_state(state)?;                    // persist first
    Gates::ack(project_root, state.phase, stage)?;   // then signal Hermes
    Ok(GateAction::from_response(&response))
}
```

No other changes. The `run_gate()` function signature, callers, and the gate
file protocol itself are unchanged.

### Verification Gate

```
cargo test
```
Full suite must pass.

Manual crash simulation (optional, not a CI check): verify that if a test kills
the process after `save_state` but before `Gates::ack`, the next run loads
`gate_pending: false` and does not block.

---

## Task 11r-C — Fix CR-03: Move divergence check before branch creation in `start()`

**Critical:** CR-03
**Files:** `crates/devflow-cli/src/main.rs`
**Estimated:** ~15 lines moved (no new logic), 1 commit

### What to Change

**`crates/devflow-cli/src/main.rs`** — In the `start()` function, move the
divergence check block (currently lines 295–306) to BEFORE the worktree/branch
creation block (currently lines 264–293).

The divergence check block to move:
```rust
// Pre-start divergence check: warn if develop has advanced significantly.
if let Ok((_ahead, behind)) = GitFlow::new(project_root).divergence_from_develop() {
    if behind > 50 {
        return Err(CliError::Message(format!(
            "develop is {behind} commits ahead — your branch is too far behind. \
             Rebase onto develop first, or use --force to override."
        )));
    }
    if behind > 10 {
        println!("warning: develop is {behind} commits ahead — consider rebasing first");
    }
}
```

Target position: immediately after state construction and before the
`if worktree { ... } else { ... }` block that creates the branch/worktree.

The new ordering in `start()`:
1. Build `State::new(...)` 
2. **[MOVED HERE] Divergence check** — runs before any git mutation, on current HEAD (develop)
3. Create worktree OR `git.feature_start(phase)` — only reached if divergence is acceptable
4. `workflow::save_state(&state)?`
5. `launch_stage(&state, None)?`

### Verification Gate

```
cargo test
```
Full suite must pass.

```
cargo test -p devflow-cli
```
CLI integration tests must pass.

---

## Task 11r-D — Fix CR-01: Capture agent stderr to file in `monitor.rs`

**Critical:** CR-01
**Files:** `crates/devflow-core/src/agent_result.rs`, `crates/devflow-core/src/monitor.rs`
**Estimated:** ~20 lines added/changed, 1 commit

### What to Change

**Step 1 — `crates/devflow-core/src/agent_result.rs`:**

Add `stderr_path()` alongside the existing `stdout_path()`:

```rust
/// Path where the agent's stderr is captured for a given phase.
/// Lives alongside `stdout_path` under `.devflow/`.
pub fn stderr_path(project_root: &Path, phase: u32) -> PathBuf {
    crate::workflow::devflow_dir(project_root)
        .join(format!("phase-{phase:02}-stderr.log"))
}
```

Verify that `stdout_path()` uses the same pattern so paths are consistent:
`.devflow/phase-NN-stdout.log` and `.devflow/phase-NN-stderr.log`.

**Step 2 — `crates/devflow-core/src/monitor.rs`:**

In `build_monitor_script()` (or the function containing the `format!` at line 102),
add `stderr_file` to the variables computed alongside `stdout_file`, and change
`2>/dev/null` to `2>{stderr_file}`:

```rust
let stderr_file = shell_escape(
    &crate::agent_result::stderr_path(project_root, phase)
        .to_string_lossy()
);

// In the format! string, change:
//   {agent_cmd} > {stdout_file} 2>/dev/null &
// to:
//   {agent_cmd} > {stdout_file} 2>{stderr_file} &
```

Update the format! call to include `stderr_file = stderr_file` in its named args.

Update the comment above the script (currently `// stderr is discarded so it cannot corrupt the JSON stdout capture`):
```rust
// stderr is captured to a separate file so it cannot corrupt the (possibly JSON)
// stdout capture that DevFlow parses for DEVFLOW_RESULT. Inspect
// .devflow/phase-NN-stderr.log for agent error output on failures.
```

### Verification Gate

```
cargo test
```
Full suite must pass.

```
cargo test -p devflow-core -- monitor
```
Monitor-specific tests must pass.

---

## Execution Order Summary

```
11r-A (CR-02: persist consecutive_failures)
  → 11r-B (CR-04: save_state before ack)
    → 11r-C (CR-03: divergence before branch)
      → 11r-D (CR-01: stderr capture)
```

All tasks are independent at the code level (different files/functions). The
sequential ordering ensures:
- CR-02 state model is correct before any state-adjacent fixes
- Cargo test is confirmed green at each step

---

## Final Verification

After all 4 tasks complete:

```bash
cargo test && cargo clippy -- -D warnings && cargo fmt -- --check
```

All must pass. Then:

| CR    | Verification Check |
|-------|--------------------|
| CR-02 | `rg "serde.skip" crates/devflow-core/src/state.rs` → no match on `consecutive_failures` |
| CR-04 | `rg -A5 "gate_pending = false" crates/devflow-cli/src/main.rs` → `save_state` appears before `Gates::ack` |
| CR-03 | `rg -n "divergence_from_develop" crates/devflow-cli/src/main.rs` → line number is before `feature_start` line number |
| CR-01 | `rg "2>/dev/null" crates/devflow-core/src/monitor.rs` → no match |
| CR-05 | Subsumed by CR-02; `consecutive_failures_persists_across_advance_calls` test passes |
