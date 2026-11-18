## CONFIRMED

Source-confirmed defects; the interleavings below were not executed.

1. **major — A timed-out launch deletes a run that already stopped successfully.**  
   `crates/devflow-cli/src/commands.rs:451`: `"TimedOutLaunch::NotLive => Err(clear_after_failed_launch("`; line 474: `"workflow::clear_state(project_root, phase)"`.  
   **Interleaving:** `start --until plan` launches its runner; Plan completes and records the stop; the runner exits; the launch client remains stuck until its deadline. The subsequent liveness read sees an inactive unit and deletes the saved state.  
   **Expected:** preserve the completed stage and resumable stop. **Actual:** erase them and report that the run never launched. “Not running now” does not prove “never ran.”

2. **major — `stop` can classify a replacement runner as crashed.**  
   `crates/devflow-cli/src/commands.rs:3986`: `"crate::service::stop_unit(&unit)"`; line 3988: `"stop_outcome(phase, workflow::load_state(project_root, phase))"`.  
   **Interleaving:** stop finishes stopping runner A; before its state reload, concurrent `resume` acquires the unheld phase holder, clears `stopped`, and launches runner B. Stop reads B’s state.  
   **Expected:** report A’s stop. **Actual:** report `"crashed: its runner ended without recording a stop"` while B is running. The stop operation neither holds the phase holder nor binds its result to a runner generation.

3. **major — The final cleanup sweep bypasses the run’s recorded merge base.**  
   `crates/devflow-cli/src/commands.rs:1506`: `"git.cleanup_merged_keeping(&checked_out)"`; its GitFlow comes from current project configuration at line 1418. By contrast, line 2137 explicitly reads `"state.base_branch.clone()"`.  
   **Input:** keep a prepared state without its worktree; its recorded base is `release/x`; its phase branch contains commits absent from `release/x` but merged into the currently configured `develop`.  
   **Expected:** retain the branch under the recorded-base rule. **Actual:** the state-only pass keeps the state, then the sweep force-deletes its branch through `crates/devflow-core/src/git.rs:367`: `"self.git([\"branch\", \"-D\", branch])"`. Only checked-out branches enter the sweep’s exclusion list.

4. **major — The command deadline stops applying once the direct child exits.**  
   `crates/devflow-cli/src/service.rs:84`: `"Ok(Some(_)) => return child.wait_with_output()"`.  
   **Input:** a service-tool wrapper executes `sh -c 'sleep 600 & exit 0'`, leaving its background child holding stdout/stderr open.  
   **Expected:** the configured deadline bounds the entire call. **Actual:** `wait_with_output()` waits for pipe EOF outside the deadline loop. Redirecting the background child’s output to `/dev/null` is the opposite-result control. A wrapper can therefore wedge start, status, stop, or cleanup past their advertised bounds.

5. **minor — The service double accepts stale lock bytes as readiness.**  
   `crates/devflow-cli/tests/common/mod.rs:137`: `"if [ \"$first\" = \"$pid\" ]; then exit 0; fi"`. The process-death check comes afterward, at lines 140–144.  
   **Input:** the runner writes its identity, then fails before sending readiness—for example, `notify_ready()` rejects an unusable socket address.  
   **Expected:** the fake launch fails, matching `Type=notify`. **Actual:** it can return success from the abandoned identity record, even after the runner exits. Removing the readiness notification also leaves this success condition intact. These E2E tests cannot establish the readiness contract.

## SUSPECTED

- **major — Speculation: a non-timeout client failure can clear state underneath a live runner.**  
  `crates/devflow-cli/src/service.rs:722` maps every nonzero launch exit to `"LaunchError::Failed"`; `commands.rs:430` routes that directly to state deletion. If the manager accepts the unit and the client subsequently fails while awaiting its response, the timeout-specific liveness protection is bypassed. Confirm with a post-acceptance client-disconnection fault; a rejection before acceptance is the control.

## CHECKED AND FINE

- Held runner locks block discard independently of saved state (`commands.rs:2481`).
- Discard rejects unmerged branches unless the flag names the exact phase branch (`commands.rs:2691`).
- State updates reread under the write lock and use durable replacement (`workflow.rs:443`, `workflow.rs:241`).
- No files modified, temporary paths created, or persistent processes started.


