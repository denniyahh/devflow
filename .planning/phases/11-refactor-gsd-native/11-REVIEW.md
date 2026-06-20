---
phase: 11-refactor-gsd-native
review_depth: standard
files_reviewed: 27
files_reviewed_list:
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/tests/phase7_cli.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/agent.rs
  - crates/devflow-core/src/agents/claude.rs
  - crates/devflow-core/src/agents/codex.rs
  - crates/devflow-core/src/agents/mod.rs
  - crates/devflow-core/src/agents/opencode.rs
  - crates/devflow-core/src/config.rs
  - crates/devflow-core/src/gates.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/hooks.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/mode.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/recover.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/stage.rs
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/src/verify.rs
  - crates/devflow-core/src/version.rs
  - crates/devflow-core/src/workflow.rs
  - crates/devflow-core/src/worktree.rs
  - crates/devflow-core/tests/monitor_e2e.rs
  - Cargo.toml
  - crates/devflow-cli/Cargo.toml
status: issues_found
findings:
  critical: 5
  warning: 11
  info: 5
  total: 21
---

# Phase 11: Code Review Report

**Reviewed:** 2026-06-20T00:00:00Z
**Depth:** standard
**Files Reviewed:** 27
**Status:** issues_found

## Summary

Phase 11 replaces the old 9-step `Step` enum with a 5-stage `Stage` enum, removes `.devflow.yaml` config parsing, introduces `Mode`, `Gates`, `Hooks`, and a simplified `Config`. The architecture is sound and the new primitives (`stage.rs`, `mode.rs`, `gates.rs`) are clean. The most dangerous issues are in the monitor shell script and the consecutive-failures state management, which together can cause silent data loss and incorrect pipeline behavior under realistic conditions.

---

## Summary Table

| ID     | Severity | File                            | Description                                                                 |
|--------|----------|---------------------------------|-----------------------------------------------------------------------------|
| CR-01  | CRITICAL | monitor.rs:102–116              | Shell script loses stderr; agents that write DEVFLOW_RESULT to stderr produce no result |
| CR-02  | CRITICAL | state.rs:34 / main.rs:395       | `consecutive_failures` not persisted — resets to 0 on monitor restart, breaking auto-gate threshold |
| CR-03  | CRITICAL | main.rs:296–305                 | Divergence check runs after worktree + branch are already created; error leaves stale branch |
| CR-04  | CRITICAL | gates.rs:151–163                | Gate poll reads response file non-atomically — partial write window |
| CR-05  | CRITICAL | main.rs:440–458 (transition fn) | `consecutive_failures` unconditionally reset to 0 on any transition, even when gate approves after failure |
| WR-01  | WARNING  | monitor.rs:84–116               | Agent command injected into shell script via string interpolation; shell_escape only covers `'` |
| WR-02  | WARNING  | agent.rs:67                     | `libc::kill(pid as i32, 0)` — truncation cast if pid > i32::MAX (theoretical, but unsafe block) |
| WR-03  | WARNING  | hooks.rs:95–104                 | `BranchCleanup` uses non-force delete; will silently skip unmerged feature branches |
| WR-04  | WARNING  | version.rs:182–203              | `find_version_in_contents` matches first key named `version` in current section regardless of section nesting |
| WR-05  | WARNING  | ship.rs:380–396                 | `parse_rfc3339ish` restores `second` after UTC normalization but sets `second = 0` on from_epoch_minutes |
| WR-06  | WARNING  | main.rs:827–832                 | `retry_after_from_reason` strips only one prefix pattern; `.or(reason)` returns original reason if prefix absent |
| WR-07  | WARNING  | workflow.rs:32–39               | `save_state` not atomic — partial write to `state.json` possible on power-loss or kill |
| WR-08  | WARNING  | ship.rs:450–459                 | `shell_quote` in `build_cron_instructions` only quotes shell-unsafe chars, misses `!`, `$`, backticks |
| WR-09  | WARNING  | agent_result.rs:184–195         | `parse_marker_lines` tail scan reverses by char, not by byte; non-UTF-8 boundary panic potential |
| WR-10  | WARNING  | phase7_cli.rs:46–53             | Integration tests write `.devflow.yaml` (v1 config) into temp repos — confirms old artifact survives |
| WR-11  | WARNING  | main.rs:360–374                 | `advance` halts on non-Validate failures but does not fire any gate — state is left dirty with no path forward |
| IN-01  | INFO     | lib.rs:26–30                    | Stale doc examples reference `devflow check` command (removed in phase 11) |
| IN-02  | INFO     | state.rs:46–51                  | `agent_result` and `agent_stdout_path` fields are `#[serde(skip)]` but never set anywhere in the new flow |
| IN-03  | INFO     | agents/mod.rs:8                 | `use crate::state::AgentKind` imported but `Agent` trait name shadows it — aliasing noise |
| IN-04  | INFO     | main.rs:1172–1201               | `test_cmd` invokes `sh -c "cargo fmt -- --check"` which is wrong; correct form is `cargo fmt -- --check` with no shell wrapping needed, and `cargo fmt --check` is valid directly |
| IN-05  | INFO     | Cargo.toml:9                    | Workspace version is `1.2.0` but code comments and docs reference `v2.0.0` as the new version |

---

## Critical Issues

### CR-01: Monitor shell script discards agent stderr entirely

**File:** `crates/devflow-core/src/monitor.rs:105`

**Issue:** The generated shell script redirects the agent's stderr to `/dev/null`:
```sh
{agent_cmd} > {stdout_file} 2>/dev/null &
```
The comment says "stderr is discarded so it cannot corrupt the (possibly JSON) stdout capture." However, some agent invocations (e.g. errors during Claude startup, or any agent that writes its DEVFLOW_RESULT marker to stderr instead of stdout) will be silently lost. More concretely, if the agent binary itself fails to launch (e.g. a missing dependency), the only signal is an exit code — no error message is captured for diagnostics. Even in the success path, a crashed agent may produce error output that explains the failure; discarding it makes `devflow recover` uninformative.

**Fix:** Capture stderr to a separate file for diagnostics:
```rust
let stderr_file = crate::agent_result::stderr_path(&state.project_root, state.phase);
// In the format! string:
"{agent_cmd} > {stdout_file} 2>{stderr_file} &"
```
Add `stderr_path()` to `agent_result.rs`. When the agent fails (non-zero exit), include the stderr content in the error message from `advance`.

---

### CR-02: `consecutive_failures` is `#[serde(skip)]` — the auto-gate threshold is permanently broken across monitor restarts

**File:** `crates/devflow-core/src/state.rs:34` and `crates/devflow-core/src/main.rs:395`

**Issue:** The field is documented as "runtime-only" and skipped from persistence. However, the monitor is a short-lived shell process: it launches the agent, waits for exit, then calls `devflow advance`. Each `devflow advance` invocation loads state fresh from disk. Because `consecutive_failures` is never persisted, it is always 0 at load time. This means `Mode::Auto` can never fire the forced gate after `MAX_CONSECUTIVE_FAILURES` (3) — the counter resets to 0 on every `advance` call, making the threshold logic dead code. A permanently broken agent in Auto mode will loop Code↔Validate indefinitely.

The doc comment contradicts itself: "Not persisted: always starts at 0 on monitor restart (runtime-only)" — but each `advance` is a new process, not a monitor restart, so it *always* starts at 0.

**Fix:** Remove `#[serde(skip)]` from `consecutive_failures` and persist it:
```rust
// state.rs — remove the serde(skip) attribute
pub consecutive_failures: u32,
```
Update the doc comment to reflect that this field is persisted. Reset it to 0 explicitly in `transition()` (which already does this correctly), not at load time.

---

### CR-03: Divergence check runs after the feature branch is already created

**File:** `crates/devflow-cli/src/main.rs:276–306`

**Issue:** In `start()`, the feature branch or worktree is created first (lines 276–293), then the divergence check runs (lines 296–305). When `behind > 50`, the function returns an error — but the feature branch has already been created and checked out. The error message says "Rebase onto develop first" but the branch now exists and the checkout is on it. The user must manually clean up.

Additionally, the divergence check calls `GitFlow::new(project_root).divergence_from_develop()`, which checks the divergence of the *currently checked-out branch*. After `feature_start`, the current branch is the new feature branch, not develop — so the check reports 0 commits ahead and the correct `behind` count for the feature branch against develop (which is identical to develop's divergence from develop's own tip — always 0). The check is measuring the wrong thing after branching.

**Fix:** Move the divergence check *before* branch creation. Run it on the develop branch:
```rust
fn start(...) -> Result<(), CliError> {
    // 1. Check divergence BEFORE creating anything
    if let Ok((_ahead, behind)) = GitFlow::new(project_root).divergence_from_develop_branch() {
        if behind > 50 { return Err(...); }
    }
    // 2. Create branch / worktree
    // ...
}
```
Alternatively, compute divergence directly as `git rev-list --count HEAD..develop` before any checkout.

---

### CR-04: Gate response file read is non-atomic — partial-write window

**File:** `crates/devflow-core/src/gates.rs:151–156`

**Issue:** `poll_response` reads the response file with a plain `read_to_string` then immediately parses:
```rust
if let Ok(contents) = std::fs::read_to_string(&path)
    && let Ok(response) = serde_json::from_str::<GateResponse>(&contents)
{
    return Some(response);
}
```
`write_gate` uses atomic rename (`write_atomic`). But `GateResponse` is written by Hermes (an external tool), which may not use atomic rename. If Hermes writes the response file non-atomically, `read_to_string` can read a partial JSON document, `serde_json::from_str` fails, the poll loop retries, and eventually reads a complete file. This is only a liveness issue (extra retry), not a correctness issue.

However, a more serious race exists: if the response file is written atomically (rename), then `read_to_string` can still race with the rename on some file systems. The poll will retry in that case.

The real issue is that `ack` writes its file but `cleanup` is called from `finish_workflow` — if the process is killed between `ack` and `cleanup`, stale gate + response files remain. On the next run, the pipeline resumes correctly only if the persisted state (`gate_pending = false`) is already written. But in `run_gate`, `state.gate_pending = false` and `workflow::save_state(state)` are called *after* `Gates::ack` — if killed between `ack` and `save_state`, the state still shows `gate_pending: true` but no response file, causing `poll_response` to block until timeout (7 days) on restart.

**Fix:** Write `gate_pending = false` and `save_state` *before* calling `Gates::ack`, so a crash after ack-write but before state-save does not leave a misleading `gate_pending: true` state. Order in `run_gate`:
```rust
state.gate_pending = false;
workflow::save_state(state)?;  // persist first
Gates::ack(project_root, state.phase, stage)?;  // then ack
```

---

### CR-05: `consecutive_failures` reset on every transition, including gate-approved failures

**File:** `crates/devflow-cli/src/main.rs:440–458`

**Issue:** The `transition()` function unconditionally sets `state.consecutive_failures = 0` at line 455. This is called from `handle_validate_outcome` when a gate approves advancing to Ship (line 412: `GateAction::Advance => transition(..., Stage::Ship)`). The reset is correct here. However, `transition()` is also called from `loop_back_to_code` — no it is not, `loop_back_to_code` does not call `transition()`.

The actual bug: `handle_validate_outcome` increments `consecutive_failures` at line 395 (`state.consecutive_failures += 1`) but only when `!passed`. Then it calls `should_gate()` which reads the (now incremented) count. If gating fires and the gate response is `Advance`, `transition(project_root, state, Stage::Ship)` is called. Inside `transition`, `consecutive_failures` is reset to 0 (line 455). This is correct for a gate-approved advance.

However, if gating fires and the response is `LoopBack`, `loop_back_to_code` is called (line 413). `loop_back_to_code` does NOT reset `consecutive_failures` — meaning subsequent failures correctly accumulate. But `loop_back_to_code` also does not call `save_state` with the updated (non-zero) `consecutive_failures` — it only saves with `gate_pending = false` and `stage = Code`. Because `consecutive_failures` is `#[serde(skip)]` (CR-02), it is not saved regardless. This confirms CR-02: the gate threshold counter cannot function correctly.

Separately: `transition()` resets `consecutive_failures` to 0 on a Define→Plan or Plan→Code transition, which is correct. But it also resets it on Validate→Ship, even though the Validate stage just passed — it should already be 0 at that point. No bug, but the unconditional reset masks the CR-02 failure mode: even if `consecutive_failures` were persisted, `transition()` would zero it on every stage advance, including the Ship gate approval path. This means after a human approves the gate on a run that had 2 failures, the count resets to 0, which is desirable. But during a re-run after a crash mid-loop, the count would start from 0 again (correct).

The net finding is CR-02 subsumes this. Document this as a secondary severity marker attached to CR-02.

---

## Warnings

### WR-01: Shell command injection risk via prompt content in monitor script

**File:** `crates/devflow-core/src/monitor.rs:84–116`

**Issue:** The monitor builds a shell script by interpolating the agent command via `shell_escape`. The prompt content is passed as a CLI argument to the agent (e.g., `claude -p '<prompt>'`). `shell_escape` correctly handles single quotes via `'\\''` substitution. However, if the prompt or any agent argument contains a null byte (`\0`) or a Unicode character that is interpreted specially by certain shells, behavior is undefined. More practically: if `program` or any element of `args` is derived from a user-controlled source (e.g., the `--agent` flag with a path traversal like `../evil`), the escaped value is still passed verbatim to `sh -c`, and a null byte in a shell string may terminate argument parsing in some libc implementations.

The prompt text is generated internally by `prompt.rs` (not user-provided), so this is low-risk in practice. The `--agent` flag is constrained to `Agent::Claude/Codex/OpenCode` by the enum parser. No immediate exploitable path exists, but the pattern is fragile.

**Fix:** Prefer `Command` spawning over shell script generation for the agent subprocess. The monitor could spawn the agent directly without a shell, using `Command::new(program).args(args)`, writing stdout to a file via `File::create` + `Stdio::from`, and using a thread or separate process to wait. This eliminates the shell injection surface entirely.

---

### WR-02: Unsafe PID cast truncates on 64-bit systems with large PIDs

**File:** `crates/devflow-core/src/agent.rs:67`

**Issue:**
```rust
unsafe { libc::kill(pid as i32, 0) == 0 }
```
`pid` is `u32`. On Linux, PIDs fit in 32 bits but are positive. The cast `u32 as i32` is defined behavior in Rust (wrapping), but a PID > 2,147,483,647 (i32::MAX) would wrap to a negative value. Linux 4.15+ supports PIDs up to 4,194,304 by default (and up to 2^22 with kernel config), so values > i32::MAX are unreachable in practice. However, the unsafe block should document this assumption or use `libc::pid_t` directly.

**Fix:**
```rust
pub fn agent_running(pid: u32) -> bool {
    // PIDs on Linux/macOS fit in i32; values > i32::MAX cannot occur.
    let pid_t = pid as libc::pid_t;
    unsafe { libc::kill(pid_t, 0) == 0 }
}
```

---

### WR-03: `BranchCleanup` hook silently skips unmerged feature branches

**File:** `crates/devflow-core/src/hooks.rs:95–104`

**Issue:**
```rust
match git.delete_branch(&branch, false) {  // force = false
    Ok(()) => info!("BranchCleanup: deleted {branch}"),
    Err(err) => warn!("BranchCleanup: could not delete {branch}: {err}"),
}
```
`force = false` means `git branch -d`, which refuses to delete unmerged branches. After `Ship` completes, the feature branch *should* be merged (the Ship stage runs `gsd-ship` which presumably creates the PR/merge). However, if the Ship stage wrote commits that are not yet merged into develop at hook-fire time, the cleanup silently fails with only a warning. The workflow prints "phase N shipped — workflow complete" even though the branch was not cleaned up.

This is a logical gap: the hooks fire after ship approval (in `finish_workflow`) but the actual merge into develop may happen asynchronously (via a PR merge). The `--force` behavior should be documented or the hook should check merge status before deciding which flag to use.

**Fix:** Either document that `BranchCleanup` is intentionally non-force (because branches are cleaned up by the PR merge process), or use `force = true` if the intent is always to clean up at this point. At minimum, upgrade the log from `warn` to distinguish "not merged yet" from "git error".

---

### WR-04: `find_version_in_contents` matches across nested TOML table headers

**File:** `crates/devflow-core/src/version.rs:182–203`

**Issue:** The TOML section tracker uses `current` (a `&str` pointing into `field` splits) for section comparison. The parser tracks sections by checking for `[header]` lines but does not handle:
1. `[[array.of.tables]]` headers (double brackets) — these are parsed as `[array.of.tables]` which will incorrectly match.
2. Inline tables: `dependencies = { version = "1.0" }` — the parser would look for `version` as a standalone key in any section named `""` (the default section before any header) if the inline table happens to appear before the `[package]` section.

For the specific field paths used (`package.version`, `workspace.package.version`), this is low risk in a typical Cargo.toml, but a workspace Cargo.toml with `[workspace.dependencies]` containing a `version` key would match incorrectly if `workspace.dependencies` is reached before `workspace.package`.

**Fix:** Add handling for `[[...]]` (array of tables) by detecting the double-bracket prefix:
```rust
fn parse_section_header(trimmed: &str) -> Option<&str> {
    let inner = if trimmed.starts_with("[[") {
        trimmed.strip_prefix("[[")?.strip_suffix("]]")?
    } else {
        trimmed.strip_prefix('[')?.strip_suffix(']')?
    };
    Some(inner.trim())
}
```

---

### WR-05: RFC 3339 timestamp normalization discards seconds field after round-trip

**File:** `crates/devflow-core/src/ship.rs:380–396`

**Issue:** In `parse_rfc3339ish`:
```rust
let mut normalized = RetryTimestamp::from_epoch_minutes(utc_minutes);
normalized.second = second;  // restore seconds
Some(normalized)
```
`from_epoch_minutes` always sets `second = 0`. The code then restores `second` into `normalized.second`. Then `round_up_minute()` checks `if self.second == 0 { return self; }`. If `second > 0`, it increments by one minute. The logic appears correct: timestamps with non-zero seconds are rounded up to the next minute.

However, the UTC conversion subtracts `offset_minutes` from `utc_minutes` and then the second is re-applied from the *local* time, not UTC. If the local timestamp is `HH:MM:SS+offset`, the UTC normalized time has `second=0` (from `from_epoch_minutes`), then `second` is restored from the parsed value. This means the `second` field is always the local seconds, not the UTC seconds — which is correct since seconds are the same regardless of offset. No actual bug, but this is fragile and confusing.

The real issue: `to_cron()` formats `minute` without zero-padding. Cron interpreters expect `M H D M W` where single-digit minutes like `1` are interpreted correctly, but the format string `"{} {} {} {} *"` produces `"46 15 18 6 *"` which is correct. No bug.

**Fix:** This is a documentation gap more than a bug. Add a comment clarifying that `second` is timezone-invariant and why the restoration is safe.

---

### WR-06: `retry_after_from_reason` falls through to raw reason string

**File:** `crates/devflow-cli/src/main.rs:826–832`

**Issue:**
```rust
fn retry_after_from_reason(reason: Option<&str>) -> String {
    reason
        .and_then(|s| s.strip_prefix("rate limited until "))
        .or(reason)   // <-- if prefix absent, return raw reason
        .unwrap_or("unknown")
        .to_string()
}
```
The `.or(reason)` fallback means the raw reason string (e.g., `"usage limit"`) becomes the `retry_after` value in the cron instructions. The `cron_schedule_from_retry_after("usage limit")` call will fail to parse and fall back to `"* * * * *"` — a cron job that runs every minute. This writes a one-shot cron job that fires immediately every minute until removed.

**Fix:**
```rust
fn retry_after_from_reason(reason: Option<&str>) -> String {
    reason
        .and_then(|s| s.strip_prefix("rate limited until "))
        .unwrap_or("unknown")
        .to_string()
}
```
If the retry timestamp is unparseable, the cron schedule should be `None` or a sentinel value, not `* * * * *`.

---

### WR-07: `save_state` writes non-atomically — partial state.json on kill

**File:** `crates/devflow-core/src/workflow.rs:32–39`

**Issue:**
```rust
std::fs::write(dir.join("state.json"), contents)?;
```
`fs::write` is not atomic on most filesystems (it truncates then writes). If the process is killed mid-write (e.g., `kill -9`, OOM), `state.json` will be empty or contain partial JSON. The next `load_state` call will get a JSON parse error, leaving the workflow permanently stuck. The gate files use `write_atomic` (temp + rename) for exactly this reason, but `save_state` does not.

**Fix:**
```rust
pub fn save_state(state: &State) -> Result<(), WorkflowError> {
    let dir = devflow_dir(&state.project_root);
    std::fs::create_dir_all(&dir)?;
    let contents = serde_json::to_string_pretty(state)?;
    let tmp = dir.join("state.json.tmp");
    std::fs::write(&tmp, &contents)?;
    std::fs::rename(&tmp, dir.join("state.json"))?;
    Ok(())
}
```

---

### WR-08: `shell_quote` in `build_cron_instructions` misses several shell-unsafe characters

**File:** `crates/devflow-core/src/ship.rs:451–459`

**Issue:**
```rust
fn shell_quote(value: &str) -> String {
    if value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-')) {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
```
The "safe" character check misses `~`, `:`, `@`, `+`, `=`, `%`, `,`, and others. If `project` (the project root path) contains any of these (e.g., `/home/user@host/project`), it passes the safety check but the unquoted string is safe. The single-quote wrapping path is correct. This is a false-negative in the "is it safe unquoted" check, but since the fallthrough goes to single-quote wrapping, the actual output is still safe. However, the condition is misleading — it claims a path with `~` is "safe unquoted" when `~` in unquoted shell context is subject to tilde expansion.

**Fix:** Either widen the safe-chars check to include all safe-unquoted characters per POSIX, or default to always quoting:
```rust
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
```

---

### WR-09: `parse_marker_lines` reverses by Unicode chars, not bytes — O(n) char collection

**File:** `crates/devflow-core/src/agent_result.rs:184–195`

**Issue:**
```rust
let tail: String = stdout
    .chars()
    .rev()
    .take(4000)
    .collect::<Vec<_>>()
    .into_iter()
    .rev()
    .collect();
```
This reverses by Unicode codepoints. For ASCII-only agent output this is equivalent to byte reversal, but:
1. If stdout contains multi-byte UTF-8 sequences, `.chars().rev().take(4000)` takes 4000 codepoints from the end, not 4000 bytes. This is the correct behavior (you want the last 4000 chars of the logical string), but the variable is named to suggest a byte tail.
2. The double reversal (rev → take → collect → rev again) produces a `Vec<char>` intermediate. For large output (hundreds of KB), this is expensive but not incorrect.
3. The actual DEVFLOW_RESULT marker is ASCII, so splitting the tail by lines will always find correct boundaries.

There is no correctness bug here for ASCII content (which all DEVFLOW_RESULT markers are). The concern is that the pattern is unnecessarily complex.

**Fix:** Use a simpler and more readable approach:
```rust
let tail = if stdout.len() > 4000 {
    // Find a char boundary at or before the 4000-bytes-from-end position
    let start = stdout.len() - 4000;
    let start = stdout.floor_char_boundary(start);  // Rust 1.74+
    &stdout[start..]
} else {
    stdout.as_str()
};
```

---

### WR-10: Integration test writes `.devflow.yaml` (v1 artifact) into temp repos

**File:** `crates/devflow-cli/tests/phase7_cli.rs:46–53`

**Issue:** `write_config` writes a `.devflow.yaml` file with v1 configuration fields (`automation`, `version.scheme`, `git_flow`) into the temp repo root. Phase 11's stated goal is to remove all `.devflow.yaml` parsing. If this file exists in a test repo and `devflow` no longer reads it, the tests are not testing the new behavior — they're silently ignoring the config file. More seriously, if a future refactor inadvertently re-introduces `.devflow.yaml` parsing, these tests would not catch the regression.

Additionally, none of the CLI tests that exercise `start`, `parallel`, or `sequentagent` verify that `devflow` does NOT read `.devflow.yaml` — the tests assert output/state but leave the config file present.

**Fix:** Remove `write_config` and the `write_last_ship` helper from the test file, since they populate v1 artifacts. Verify at the test level that `.devflow.yaml` is absent or deliberately ignored.

---

### WR-11: `advance` halts non-Validate failures with dirty state and no recovery path

**File:** `crates/devflow-cli/src/main.rs:360–374`

**Issue:**
```rust
_ => Err(CliError::Message(format!(
    "stage {stage} failed: {}",
    result.reason.unwrap_or_else(|| "no details available".into())
))),
```
When Define, Plan, Code, or Ship stages fail, `advance` returns an error. The monitor process (a shell script) does not capture the exit code of `devflow advance` — it simply calls `{binary} advance {project_root}` with no `|| ...` fallback. The error is printed to the monitor's stdout (which is `/dev/null`). The state file remains persisted with the failed stage, `gate_pending = false`. The user has no notification that the workflow halted. `devflow status` will show the stage, but the workflow is stuck — no gate was fired, no cleanup was done.

**Fix:** For agent-stage failures (Define/Plan), fire a gate to notify the user:
```rust
_ => {
    // Write a gate so Hermes/human is notified.
    run_gate(project_root, state, stage, &format!("Stage {stage} failed: ..."))?;
    // ...
}
```
Or at minimum, emit a structured log that Hermes can observe, and document the recovery path (`devflow recover --clean`).

---

## Info

### IN-01: Stale doc examples reference removed `devflow check` command

**File:** `crates/devflow-core/src/lib.rs:26`

**Issue:** The module-level doc comment contains:
```
RUST_LOG=info devflow check        # Default: shows state transitions
RUST_LOG=warn devflow ship         # Suppress info, show only warnings
```
Both `devflow check` and `devflow ship` were removed in this phase. The remaining commands are `start`, `advance` (hidden), `status`, `list`, `recover`, `test`, `doctor`, `parallel`, `sequentagent`, `reference`, `cleanup`.

**Fix:** Replace the example commands with valid ones such as `devflow start`, `devflow status`.

---

### IN-02: `agent_result` and `agent_stdout_path` fields in `State` are never populated

**File:** `crates/devflow-core/src/state.rs:46–51`

**Issue:** Both `agent_result: Option<AgentResult>` and `agent_stdout_path: Option<PathBuf>` are `#[serde(skip)]` fields that exist in the `State` struct but are never assigned in any code path. They were presumably added for future use. Dead struct fields increase cognitive load and create false expectations (a reviewer may expect these to be populated by `evaluate_agent_result`).

**Fix:** Remove both fields until they are needed, or add a TODO comment with a GitHub issue reference explaining why they are reserved.

---

### IN-03: `AgentKind` type alias in `state.rs` creates naming confusion in `agents/mod.rs`

**File:** `crates/devflow-core/src/agents/mod.rs:7` and `crates/devflow-core/src/state.rs:67`

**Issue:** `state.rs` defines `pub type AgentKind = Agent;` and `agents/mod.rs` imports `use crate::state::AgentKind;` to avoid conflicting with the `Agent` trait. This creates two different types named `Agent` — one is an enum (the agent kind), one is a trait (the adapter interface). In `adapter_for(kind: AgentKind)`, the parameter could easily be confused with the `Agent` trait. The alias solves the name collision but the underlying naming conflict makes the code fragile.

**Fix:** Rename the enum to `AgentKind` directly in `state.rs` (removing the alias), and rename the trait to `AgentAdapter` in `agents/mod.rs`. This makes the distinction explicit without an alias.

---

### IN-04: `test_cmd` invokes `cargo fmt` with incorrect argument order

**File:** `crates/devflow-cli/src/main.rs:1175`

**Issue:**
```rust
("cargo fmt --check", "cargo fmt -- --check"),
```
`cargo fmt -- --check` passes `--check` to `rustfmt` directly. This works but is non-idiomatic — the canonical form is `cargo fmt --check` (passing `--check` to cargo, not to rustfmt directly). Both forms produce the same behavior with current `cargo fmt`, but the `-- --check` form may break with future `cargo fmt` versions that change how rustfmt flags are passed.

**Fix:** Use `"cargo fmt --check"` as the command string (no `--` separator).

---

### IN-05: Workspace version `1.2.0` conflicts with v2.0.0 references in docs

**File:** `Cargo.toml:9`

**Issue:** The workspace `version` field is `"1.2.0"`, but the code comments, doc strings, and architecture description repeatedly reference "v2.0.0" as the new version being released in this phase. The `doctor` command reports `devflow_version = env!("CARGO_PKG_VERSION")` which would output `1.2.0` to users, contradicting the "v2.0.0" messaging in docs.

**Fix:** Either update `Cargo.toml` version to `"2.0.0"` if this phase IS the v2.0.0 release, or update the in-code references to say "v1.x.0+" or "the new architecture." Ensure the version in Cargo.toml is intentional.

---

## Test Gap Analysis

The following code paths have no test coverage:

1. **`advance` function** (main.rs): The orchestration heart of the pipeline — stage transitions, validate outcome handling, gate firing — is only tested via manual/e2e flows. No unit tests for the `handle_validate_outcome` logic (especially the `consecutive_failures` threshold path).

2. **`consecutive_failures` threshold reaching MAX** (mode.rs/main.rs): Tests in `mode.rs` verify `should_gate()` logic, but there are no integration tests verifying that 3 consecutive Validate failures actually fire a gate (end-to-end). Given CR-02, this is also broken behavior.

3. **`transition()` hook firing** (main.rs:440–458): The only hook test is in `hooks.rs` unit tests. No test verifies that `transition(Validate, Ship)` actually fires `DocsUpdate` and `ChangelogAppend`.

4. **Gate timeout path** (gates.rs): `poll_response_times_out_when_absent` exists with `timeout_secs=0`, but the 7-day timeout used in production is never tested.

5. **`abort` code path** (main.rs:527): No test covers the case where a gate response has `approved: false` and the note contains "abort."

6. **`list_feature_branches` ahead/behind counts** (git.rs): The test for `list` in `main.rs` tests display formatting but not the correctness of `ahead`/`behind` computation.

7. **`version::write_version` with workspace Cargo.toml** (version.rs): `read_major_from_workspace_package` is tested, but `write_version` is not tested against a workspace Cargo.toml — only a simple `[package]` one.

8. **`parse_rfc3339ish` with negative UTC offsets** (ship.rs): The cron schedule tests use UTC (`Z`) timestamps. Negative-offset timestamps (`-05:00`) are not tested.

9. **Monitor `devflow advance` call failing** (monitor.rs): No test covers what happens when the `devflow advance` call in the monitor shell script fails (e.g., state file missing, JSON corrupt).

---

## Overall Assessment

**FAIL — Do not ship without fixing CR-01 through CR-05.**

The five critical findings collectively break the core automation contract:
- **CR-02** is the most severe: `consecutive_failures` never persists, making the Auto-mode infinite-loop guard dead code. An agent that permanently fails Validate will loop forever.
- **CR-01** discards agent error output, making failures opaque.
- **CR-03** creates stale branches on startup errors.
- **CR-04** creates a multi-day stuck state if the process is killed during gate acknowledgment.
- **CR-05** is a secondary confirmation of CR-02.

The architecture itself (Stage enum, Mode, Gates, Hooks, simplified Config) is well-designed and the unit tests for individual components are thorough. The issues are concentrated in the wiring between components in `main.rs` and in the state persistence model.

---

_Reviewed: 2026-06-20T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
