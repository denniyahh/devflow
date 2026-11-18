## CONFIRMED

### 1. Critical: Re-launch on `failed` unit state fails and wipes project state
- **File:Line:** [service.rs:360-372](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/service.rs#L360-L372), [commands.rs:2254-2257](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/commands.rs#L2254-L2257), [commands.rs:430-435](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/commands.rs#L430-L435)
- **Quoted Text:**
  ```rust
  // service.rs:360-363
  "inactive" | "failed" => {
      trace!("unit {unit} is inactive ({state})");
      return Ok(());
  }

  // commands.rs:430-434
  Err(LaunchError::Failed(why)) => {
      clear_after_failed_launch(project_root, phase);
      Err(CliError::Message(why))
  }
  ```
- **Concrete Interleaving:**
  1. A runner unit terminates abnormally or fails startup (e.g. unhandled crash, agent error, or start timeout), leaving the unit in systemd's `failed` active state.
  2. The user executes `devflow start` (or `devflow resume`). Both `wait_unit_inactive` and `runner_liveness` evaluate `is-active`, see `"failed"`, treat it as inactive, and proceed to invoke `launch_runner_unit`.
  3. `launch_runner_unit` executes `systemd-run --user --collect --unit=devflow-...`.
  4. Systemd retains failed transient units loaded until explicitly reset. `systemd-run` exits code 1 with: `Failed to start transient service unit: Unit devflow-... was already loaded or has a fragment file.`
  5. `launch_runner_unit` returns `LaunchError::Failed`. `commands::start` catches this and immediately runs `clear_after_failed_launch`, calling `clear_state` and deleting `.devflow/state-NN.json`.
  6. The unit test suite masked this by calling `systemctl --user reset-failed` out-of-band in [service_e2e.rs:115](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/tests/service_e2e.rs#L115) and [mod.rs:67](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/tests/common/mod.rs#L67), but production code never invokes `reset-failed`.
- **Expected vs Actual:**
  - *Expected:* Failed units must be cleared via `systemctl reset-failed` prior to launching, or launch errors must not destroy persistent state.
  - *Actual:* Re-launch is permanently blocked and destroys the existing state file.

---

### 2. Critical: `cleanup` force-deletes branches of stopped runs and `--no-worktree` runs
- **File:Line:** [commands.rs:1458](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/commands.rs#L1458), [commands.rs:1501-1506](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/commands.rs#L1501-L1506), [git.rs:367](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-core/src/git.rs#L367)
- **Quoted Text:**
  ```rust
  // commands.rs:1501-1506
  fn sweep_merged_branches(project_root: &Path, git: &GitFlow) -> Result<(), CliError> {
      let checked_out: Vec<String> = worktree::list(project_root)?
          .into_iter()
          .filter_map(|wt| wt.branch)
          .collect();
      match git.cleanup_merged_keeping(&checked_out) {

  // git.rs:367
  match self.git(["branch", "-D", branch]) {
  ```
- **Concrete Interleaving:**
  1. A run is started with `--no-worktree`, or a normal run is stopped during planning before any commits are made ahead of `develop`.
  2. The operator runs `devflow cleanup`.
  3. `cleanup_phase` runs: `cleanup_state_only` reports `"keeping run state...: the run stopped; run devflow resume to continue it"` and returns `CleanupOutcome::Kept`.
  4. Immediately following the phase loop, line 1458 unconditionally invokes `sweep_merged_branches`.
  5. `sweep_merged_branches` populates `keep` exclusively from `worktree::list()`. Because the phase has no separate worktree checkout, its branch `feature/phase-NN` is omitted from `keep`.
  6. Because `feature/phase-NN` has 0 commits ahead of `develop`, `git branch --merged develop` includes `feature/phase-NN`.
  7. Line 367 executes `git branch -D feature/phase-NN`, force-deleting the branch.
  8. When the operator follows the advice to `devflow resume`, the branch no longer exists.
- **Expected vs Actual:**
  - *Expected:* Branches of active or stopped runs must be preserved across sweeps as promised in `OPERATIONS.md:48`.
  - *Actual:* Unconditionally force-deletes the git branch of any active `--no-worktree` run or stopped run without commits.

---

### 3. Major: `refuse_discard_of_a_live_run` contradiction permits discarding a stopping runner
- **File:Line:** [commands.rs:2480-2498](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/commands.rs#L2480-L2498)
- **Quoted Text:**
  ```rust
  /// [`PhaseRunLiveness`] but `NotLive`, naming `devflow stop`. Nothing is removed.
  fn refuse_discard_of_a_live_run(phase: PhaseId, liveness: &PhaseRunLiveness) -> Result<(), CliError> {
      match liveness {
          PhaseRunLiveness::NotLive => Ok(()),
          PhaseRunLiveness::RunnerStopping { .. } => Ok(()),
          PhaseRunLiveness::RunnerHolding { .. }
          | PhaseRunLiveness::LiveAgentOnly { .. }
          | PhaseRunLiveness::StaleAgent { .. } => Err(CliError::ActionRefused(format!(
              "phase {phase} has a live run ({}): run devflow stop first",
              liveness.describe()
          ))),
      }
  }
  ```
- **Concrete Interleaving:**
  1. A runner is stopping (unit is in `deactivating` or the runner is executing its SIGTERM agent drain).
  2. The operator runs `devflow discard <phase>`.
  3. `refuse_discard_of_a_live_run` matches `PhaseRunLiveness::RunnerStopping` and returns `Ok(())`.
  4. `discard` proceeds into `discard_items`, unlinking `.devflow/state-NN.json`, removing the worktree, and clearing lock files while the runner process is still active and draining.
- **Expected vs Actual:**
  - *Expected:* Refusal of discard with `"phase {phase} has a live run...: run devflow stop first"` per documentation ("every live-run state is refused... Nothing is removed").
  - *Actual:* Permits `discard` to delete worktrees and state files out from underneath an active, stopping process.

---

### 4. Major: Failed launch on `resume` permanently erases `--until` cap from state
- **File:Line:** [pipeline_launch.rs:2536-2552](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/pipeline_launch.rs#L2536-L2552), [pipeline_launch.rs:2327-2336](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/pipeline_launch.rs#L2327-L2336)
- **Quoted Text:**
  ```rust
  // pipeline_launch.rs:2536-2544
  if until_cap_fired(s) {
      s.stop_until = None;
      s.stopped = false;
      s.stop_reason = None;
  }
  // pipeline_launch.rs:2550-2552
  Err(LaunchError::Failed(why)) => {
      re_mark_stopped_after_failed_launch(s_path, &st, &why);
      Err(CliError::Message(why))
  }
  ```
- **Concrete Interleaving:**
  1. A run pauses at an `--until <stage>` milestone cap (`until_cap_fired(s)` is true, `s.stop_until = Some(cap)`).
  2. The user runs `devflow resume`.
  3. `resume_loaded` unconditionally clears `s.stop_until = None` and writes the state to disk before launching the unit.
  4. `launch_runner_unit` encounters an error (e.g. systemd unit collision, start timeout, or D-Bus error).
  5. `re_mark_stopped_after_failed_launch` re-sets `s.stopped = true; s.stop_reason = Some(...)`, but never restores `s.stop_until`.
- **Expected vs Actual:**
  - *Expected:* Failed runner launches restore previous state values without losing user configuration.
  - *Actual:* The milestone cap is permanently erased on launch failure; future successful resumes execute the pipeline to completion rather than stopping at the desired stage.

---

### 5. Major: `devflow logs -f` exits after the first stage, breaking pipeline log streaming
- **File:Line:** [commands.rs:4201-4206](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/commands.rs#L4201-L4206), [commands.rs:4193-4197](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/commands.rs#L4193-L4197)
- **Quoted Text:**
  ```rust
  // commands.rs:4201-4206
  if exit_path.exists() && quiescent {
      let code = std::fs::read_to_string(&exit_path)
          .ok()
          .and_then(|s| s.trim().parse::<i32>().ok())
          .unwrap_or(1);
      return Ok(code);
  }
  ```
- **Concrete Interleaving:**
  1. The operator starts a multi-stage run and follows logs with `devflow logs -f`.
  2. The first stage (`Plan`) finishes; the agent writes `.devflow/exit-code-NN` and output ceases temporarily.
  3. `follow_capture` observes `exit_path.exists() && quiescent`, parses the exit code, and returns `Ok(code)`.
  4. The process exits immediately. The runner continues to the next stage (`Code`), truncating/re-opening the log, but log streaming is already dead.
  5. The rollover offset tracking in lines 4193-4197 is completely dead code because `follow_capture` terminates on the first stage transition.
- **Expected vs Actual:**
  - *Expected:* `devflow logs -f` streams output continuously across all pipeline stages until the entire run completes or fails.
  - *Actual:* Log following exits as soon as the first stage completes.

---

### 6. Major: Test doubles and mocks never simulate `failed` or `active` units, masking lifecycle bugs
- **File:Line:** [service.rs:428-439](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/service.rs#L428-L439), [mod.rs:58-66](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/tests/common/mod.rs#L58-L66)
- **Quoted Text:**
  ```bash
  # common/mod.rs:58-66
  elif [ "$cmd" = "is-active" ]; then
      if [ -f "$call_dir/unit-deactivating" ]; then echo deactivating;
      elif [ -f "$call_dir/unit-activating" ]; then echo activating;
      else echo inactive; fi
  ```
- **Concrete Interleaving:**
  1. In `common/mod.rs`, the fake `systemctl` returns only `deactivating`, `activating`, or `inactive`. It never returns `active` or `failed` unless driven by ad-hoc sentinels.
  2. In `service.rs:double`, `unit_state` defaults to `Ok("inactive")`, and `stop_unit` invokes `libc::kill` directly on the test thread PID.
  3. The unit test suite never exercises real systemd unit lifecycle paths, unit startup failures, or unit reuse collisions.
- **Expected vs Actual:**
  - *Expected:* Service test double simulates full unit lifecycle state transitions and verifies restart failure modes.
  - *Actual:* Test doubles return static `"inactive"` responses, hiding re-launch failure bugs.

---

### 7. Minor: `start --dry-run` blocks for up to 15 seconds waiting for stopping units
- **File:Line:** [commands.rs:1092](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/commands.rs#L1092), [commands.rs:624-645](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/commands.rs#L624-L645)
- **Quoted Text:**
  ```rust
  // commands.rs:1092
  let facts = gather_guard_facts(project_root, phase, dry_run)?;

  // commands.rs:624-627
  fn gather_guard_facts(project_root: &Path, phase: PhaseId, dry_run: bool) -> Result<GuardFacts, CliError> {
      let liveness = wait_out_a_stopping_unit(project_root, phase)?;
  ```
- **Concrete Interleaving:**
  1. A unit is in the `deactivating` state.
  2. The user runs `devflow start --dry-run <phase>`.
  3. `gather_guard_facts` disregards the `dry_run` parameter and calls `wait_out_a_stopping_unit(project_root, phase)`, which polls for up to `UNIT_STOP_WAIT` (15 seconds).
- **Expected vs Actual:**
  - *Expected:* `--dry-run` returns immediately without blocking, as documented in `OPERATIONS.md:37`.
  - *Actual:* Hangs synchronously for up to 15 seconds polling unit status.

---

### 8. Minor: `discard` deletes uncommitted changes in worktrees without confirmation or refusal
- **File:Line:** [commands.rs:1880-1884](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/commands.rs#L1880-L1884), [commands.rs:2719-2724](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/commands.rs#L2719-L2724)
- **Quoted Text:**
  ```rust
  // commands.rs:2719-2723
  fn refuse_discard_of_branch(git: &GitFlow, branch: &str) -> Result<(), CliError> {
      match git.unmerged_commits_to_trunk(branch) {
          Ok(0) => Ok(()),

  // commands.rs:1880-1884
  DiscardDebris::Worktree { path, .. } => {
      worktree::remove(project_root, path, true)?;
  }
  ```
- **Concrete Interleaving:**
  1. A worktree contains uncommitted modifications, but its branch commit has zero unmerged commits ahead of develop.
  2. The user runs `devflow discard <phase>`.
  3. `refuse_discard_of_branch` inspects committed history, finds 0 unmerged commits, and returns `Ok(())`.
  4. `remove_discarded` executes `worktree::remove(..., true)` with `force: true`.
- **Expected vs Actual:**
  - *Expected:* Discard halts and refuses removal when uncommitted worktree changes exist unless explicitly forced.
  - *Actual:* Irrevocably wipes unstaged worktree modifications without refusal or warning.

---

## ## SUSPECTED

1. **Deadlock hazard in `run_bounded` on oversized pipe output [Speculation]**
   - In [service.rs:75-107](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-cli/src/service.rs#L75-L107), `run_bounded` sets `stdout(Stdio::piped())` and `stderr(Stdio::piped())`, loops on `child.try_wait()`, and calls `child.wait_with_output()` only *after* process exit. No pipe reading occurs in the polling loop. If `systemd-run` or an invoked tool emits more output than the OS pipe buffer capacity (64 KB default on Linux), the child will block indefinitely on `write()`, forcing an unhandled artificial timeout and SIGKILL.
2. **PID recycling race in `live_holder` under restricted `/proc` [Speculation]**
   - In [liveness.rs:125-148](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/ce716dda-3d40-4f23-965d-a97136fcbfac/scratchpad/xreview-pr239/snapshot/crates/devflow-core/src/liveness.rs#L125-L148), `start_time` extraction falls back to `libc::kill(pid, 0) == 0` when `/proc` is mounted with `hidepid=2` or restricted by container LSMs. If a runner crashes and its PID is recycled by an unrelated user process before the lock is probed, DevFlow will misclassify the dead runner as live.

---

## ## CHECKED AND FINE

- **`passable_env_names` sanitization:** Correctly filters non-identifier environment variable names and strips systemd-reserved `MANAGER_OWNED_ENV` to prevent `systemd-run` command-line rejection.
- **Runner lock acquisition:** `claim_runner_lock` and `claim_phase_holder` correctly configure non-blocking `libc::fcntl` with `F_SETLK` and `FD_CLOEXEC`, guaranteeing OS-level release upon process exit or unhandled crash.
- **Signal handler safety:** `stop_on_signal` and `signal_dispatcher_thread` establish a self-pipe on SIGTERM/SIGINT with non-blocking writes and restore default handlers prior to thread termination.
- **State file persistence:** `update_state_with` writes via temporary files and atomic `rename`, holding exclusive flock throughout, preventing corrupted state files during concurrent reads.
- **Unit name scoping:** `unit_name` hashes the canonical project root using `path_digest`, preventing unit name collisions between separate checkouts or workspaces.
- **Subprocess orphan prevention:** Child agent spawning configures `PR_SET_PDEATHSIG` with `SIGTERM`, ensuring background agent processes terminate if `devflow __runner` is killed abruptly.
