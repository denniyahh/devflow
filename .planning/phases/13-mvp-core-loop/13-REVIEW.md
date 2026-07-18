---
phase: 13-mvp-core-loop
reviewed: 2026-07-15T00:00:00Z
depth: standard
files_reviewed: 13
files_reviewed_list:
  - crates/devflow-cli/Cargo.toml
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/tests/phase7_cli.rs
  - crates/devflow-core/src/agent.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/agents/codex.rs
  - crates/devflow-core/src/agents/mod.rs
  - crates/devflow-core/src/gates.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/lock.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/ship.rs
findings:
  critical: 3
  warning: 12
  info: 3
  total: 18
status: issues_found
---

# Phase 13: Code Review Report

**Reviewed:** 2026-07-15T00:00:00Z
**Depth:** standard
**Files Reviewed:** 13
**Status:** issues_found

## Summary

Reviewed the MVP core-loop pipeline: the stage-advance state machine (`main.rs`), the three-layer
agent-completion decision engine (`agent_result.rs`), agent process launching/capture (`agent.rs`,
`agents/*`), the gate/human-handoff protocol (`gates.rs`), git-flow plumbing (`git.rs`), the project
lock (`lock.rs`), the detached monitor daemon (`monitor.rs`), stage prompts (`prompt.rs`), and
ship/cron bookkeeping (`ship.rs`).

The code is deliberate and well-tested against the specific dogfood regressions it explicitly set
out to fix (stale gates, silent Validate/Ship advances, shell injection via monitor argv, GPG-sign
hangs, stale locks). But tracing call chains across module boundaries surfaces three severe,
independently-verified correctness bugs that undermine the pipeline's core guarantees: (1) agent
stdout capture silently discards the *entire* captured buffer — including a valid `DEVFLOW_RESULT`
marker — the moment it contains a single invalid UTF-8 byte, which can turn a Ship-stage rejection
into an apparent success; (2) `sequentagent`'s synchronous agent runner never calls the function that
knows how to parse a real Codex agent's JSONL-wrapped result marker, so a Codex agent's self-reported
failure on that path is silently treated as success; and (3) `advance()`'s project-scoped (not
phase-scoped) lock is held across a gate's multi-day blocking wait, which — combined with every
successful run ending at a mandatory Ship gate — means `devflow parallel` will routinely starve
sibling phases' `advance` calls with no retry and no operator notification. Several further
warnings cover a `doctor` mis-report, a stale cron-instruction artifact, a `git branch --merged`
parsing gap, monitor signal-handling that orphans the agent process, and a handful of smaller edge
cases.

## Narrative Findings (AI reviewer)

### Critical Issues

#### CR-01: Agent stdout capture silently discards all output on invalid UTF-8

**File:** `crates/devflow-core/src/agent.rs:87-91`
**Issue:** `capture_agent_output` reads the agent's stdout pipe with
`let _ = pipe.read_to_string(&mut stdout);`. `Read::read_to_string` guarantees that on invalid
UTF-8 the destination buffer is left **unchanged** (still empty, since `stdout` starts as
`String::new()`) and returns an `Err`, which is silently discarded here. Any agent run whose
stdout contains even one invalid UTF-8 byte anywhere in the stream (a tool echoing a binary file, a
truncated multi-byte sequence in terminal-color output, etc.) causes the *entire* captured
output — including a perfectly valid `DEVFLOW_RESULT` marker — to be lost. The empty string is then
written verbatim to `.devflow/phase-NN-stdout`.

Downstream impact is worst for the Ship stage: `evaluate_layer1` finds nothing (empty file) and
falls through to Layer 2, which for `Stage::Ship` is *not* commit-gated, so `exit_code == 0` alone
yields `AgentStatus::Success`. A real agent that decided
`DEVFLOW_RESULT: {"status":"failed","reason":"review: <critical findings>"}` (the ReviewFailed
contract `handle_ship_failure` depends on) would have that entire self-report vanish, and the
pipeline would proceed to `handle_ship_outcome` → "Ship complete — approve merge?" instead of
looping back to Code — exactly the outcome the `review:` contract exists to prevent.
**Fix:**
```rust
// Read raw bytes, then convert lossily instead of failing the whole capture
// on invalid UTF-8 — never silently drop output that may carry the
// DEVFLOW_RESULT marker.
let mut buf = Vec::new();
if let Some(ref mut pipe) = child.stdout {
    let _ = pipe.read_to_end(&mut buf);
}
let stdout = String::from_utf8_lossy(&buf).into_owned();
```

#### CR-02: `sequentagent` never parses a real Codex agent's self-reported result

**File:** `crates/devflow-cli/src/main.rs:932-948`, `crates/devflow-core/src/agent_result.rs:326`
**Issue:** `run_agent_blocking` (used exclusively by `sequentagent`) resolves the agent's outcome
with:
```rust
agent_result::parse_devflow_result(&capture.stdout).or_else(|| {
    agent_result::detect_rate_limit(&capture.stdout).map(...)
})
```
It never calls `parse_codex_event_result`. That function — the one that knows how to find the
`DEVFLOW_RESULT` marker inside a Codex `--json` event stream's `item.completed` /
`agent_message.text` field (per the 13-06 dogfood finding documented directly above it in
`agent_result.rs`) — is a private (non-`pub`) function, wired in only via `evaluate_layer1`, which
is only reachable through `evaluate_agent_result` in the async `advance()` path
(`Command::Advance`), not through `run_agent_blocking`.

Real Codex output (`codex exec --json`) never contains a raw top-level `DEVFLOW_RESULT:` line, so
`parse_devflow_result`'s line scan cannot see it. For `sequentagent`'s Codex leg, `parse_marker_
lines` returns `None`, `detect_rate_limit`'s Codex plain-text heuristic also finds nothing (real
output is pure JSONL, no "try again at" text), and `run_agent_blocking` returns `Ok(None)`. Back in
`sequentagent`, `if let Some(result) = run_agent_blocking(...)? { match result.status { ... } }` —
with `None`, the whole match is skipped and execution falls straight through to
`integrate_agent_branch`, treating a Codex agent that self-reported `"status":"failed"` (or
anything else) exactly the same as a successful run. The existing integration tests don't catch
this: the fake Codex fixture scripts in `phase7_cli.rs` print a raw `DEVFLOW_RESULT:` line directly
instead of Codex's real JSONL envelope, masking the gap.
**Fix:** Make `parse_codex_event_result` `pub(crate)` and have `run_agent_blocking` mirror the same
precedence `evaluate_layer1` uses:
```rust
agent_result::parse_devflow_result(&capture.stdout)
    .or_else(|| agent_result::parse_codex_event_result(&capture.stdout))
    .or_else(|| agent_result::detect_rate_limit(&capture.stdout).map(...))
```
(better still: refactor so both callers share one code path and cannot drift again).

#### CR-03: Project-wide lock held across multi-day gate waits breaks `devflow parallel`

**File:** `crates/devflow-cli/src/main.rs:447-456`, `crates/devflow-cli/src/main.rs:541-553`,
`crates/devflow-cli/src/main.rs:713-748`, `crates/devflow-core/src/lock.rs:130-132`
**Issue:** `advance()` acquires a single, **project-scoped** (not phase-scoped) lock at the top of
the function and holds it for the entire call:
```rust
fn advance(project_root: &Path) -> Result<(), CliError> {
    let _lock = match lock::acquire(project_root) {
        Ok(guard) => guard,
        Err(lock::LockError::Contended { pid, path: _ }) => {
            return Err(CliError::Message(format!(
                "another devflow process (pid {pid}) is already running"
            )));
        }
        ...
    };
    ...
}
```
`lock::lock_path` keys the lock purely on `project_root`:
```rust
fn lock_path(project_root: &Path) -> PathBuf {
    project_root.join(".devflow").join("lock")
}
```
Every phase run's `advance()` eventually reaches `handle_ship_outcome`, which **unconditionally**
gates on Ship regardless of mode, and `run_gate` blocks inside that same `advance()` call (still
holding `_lock`) in `Gates::poll_response`, whose default timeout is seven days
(`gate_timeout_secs()`).

`devflow parallel --phases 7,8` (`Command::Parallel`) launches one monitor per phase in the same
project. As soon as either phase's monitor calls `devflow advance` for its Ship stage (the normal
terminal step of every run), that `advance` process holds the project's only lock file for up to
seven days while it blocks on human gate approval. Any other phase's monitor that calls `devflow
advance` during that window — even for a routine, non-gated transition like Define→Plan —
immediately gets `LockError::Contended` and returns non-zero, with no retry: the monitor's shell
script (`monitor.rs:103-117`) runs `{binary} advance {project_root}` as its final statement with
`stdout`/`stderr` both `Stdio::null()` and no error handling. The affected phase's workflow is left
permanently stuck (state still says "in progress", no further monitor spawned) with **no
notification to the operator**.

Because every phase run ends at a mandatory Ship gate, running two or more phases concurrently in
the same project via `devflow parallel` will very likely deadlock/starve one of them within the
lifetime of a normal run — defeating the documented purpose of `parallel` ("Run multiple phases
concurrently, each in its own worktree + monitor"). Even outside `parallel`, any transient overlap
between two `advance` calls for different phases hits the same non-retried failure.
**Fix:** Scope the lock to phase, not project (e.g. `.devflow/lock-{phase:02}`), and/or restructure
`run_gate` so the lock guard is dropped before `Gates::poll_response` blocks, re-acquiring it only
to persist the resulting action:
```rust
fn lock_path(project_root: &Path, phase: u32) -> PathBuf {
    project_root.join(".devflow").join(format!("lock-{phase:02}"))
}
```

### Warnings

#### WR-01: `doctor`'s `cmd_check` reports a failed command as "ok"

**File:** `crates/devflow-cli/src/main.rs:1474-1487`
**Issue:** The second `Ok(out)` arm (command spawned but exited non-zero) is missing the
`out.status.success()` guard the first arm has, and hardcodes `status: "ok"` anyway:
```rust
Ok(out) => {
    let version = String::from_utf8_lossy(&out.stderr)
        .lines().next().unwrap_or("unknown").trim().to_string();
    Check {
        name: name.into(),
        status: "ok".into(),   // BUG: exit code was non-zero
        version: Some(version),
        install_hint: None,
    }
}
```
`doctor`'s entire purpose is to report "what's installed, missing, or broken"; this branch means a
broken/misbehaving binary (wrong flag, corrupted install, permission error) is reported identically
to a healthy one, just with stderr text mislabeled as the version.
**Fix:**
```rust
Ok(out) => {
    let detail = String::from_utf8_lossy(&out.stderr)
        .lines().next().unwrap_or("unknown").trim().to_string();
    Check {
        name: name.into(),
        status: "warn".into(),
        version: Some(detail),
        install_hint: Some(format!("`{cmd} {version_arg}` exited non-zero — reinstall or check PATH")),
    }
}
```

#### WR-02: Stale `cron-instructions.json` survives a successful post-rate-limit `sequentagent` run

**File:** `crates/devflow-cli/src/main.rs:972, 1011-1025, 1069-1084`
**Issue:** When agent A is rate-limited but has produced commits, `write_rate_limit_cron` writes
`.devflow/cron-instructions.json` before handing off to agent B. If agent B then completes
successfully, `sequentagent` finishes with "sequentagent complete" but never deletes the
cron-instructions file it just wrote — `delete_cron_instructions` is only called once, at the very
start of `sequentagent` (line 972), not on success. `devflow status`'s `cron_instruction_hint` will
keep telling the operator "Cron instruction pending: hermes cron create --from-devflow ..."
indefinitely for a phase that has already shipped, and if a Hermes cron job actually gets created
from that stale file, it will needlessly re-run `sequentagent` on an already-completed phase.
**Fix:** Delete the cron-instructions file once agent B completes successfully, right before
"sequentagent complete":
```rust
integrate_agent_branch(&git, &base, &branch_b)?;
let _ = devflow_core::ship::delete_cron_instructions(project_root);
println!("\nsequentagent complete — both agents integrated into {base}");
```

#### WR-03: `cleanup_merged` doesn't strip the `+` worktree prefix from `git branch --merged`

**File:** `crates/devflow-core/src/git.rs:236-250`
**Issue:** `git branch --merged` prefixes the *current* branch with `*` and any branch checked out
in a **linked worktree** with `+` (not `*`). `cleanup_merged` only strips `*`:
```rust
let branch = line.trim().trim_start_matches('*').trim();
```
If a merged branch happens to still be checked out in a worktree when this runs (e.g. the
`reference` worktree pinned to a now-merged branch, or any caller of this API that doesn't order
worktree removal before this call the way the `cleanup` CLI command does), `branch` ends up as
`"+branch-name"` — an invalid ref — and `self.git(["branch", "-d", branch])?` fails, propagating an
`Err` via `?` that aborts the *entire* `cleanup_merged` call, losing any branches already identified
for deletion earlier in the loop, not just the one problem branch.
**Fix:**
```rust
let branch = line.trim().trim_start_matches(['*', '+']).trim();
```

#### WR-04: `cleanup_merged` computes "merged" relative to the main checkout's current HEAD, not explicitly `develop`

**File:** `crates/devflow-core/src/git.rs:236-250`
**Issue:** `git branch --merged` (no explicit ref argument) reports branches merged into whatever
the current HEAD of the **main checkout** is, not `develop` specifically. `git branch -d` itself is
locally safe (it refuses to delete a branch not actually merged into HEAD, so no commits are lost),
but if the main checkout is ever left on a branch other than `develop` when `devflow cleanup` runs,
this silently prunes branches merged into that other branch instead of the documented "delete local
branches already merged into develop" behavior.
**Fix:** Pass the baseline explicitly: `self.git_output(["branch", "--merged", &self.config.develop])`.

#### WR-05: `agent_running(0)` can report a dead/corrupt PID as running

**File:** `crates/devflow-core/src/agent.rs:69-73`
**Issue:** `agent_running` calls `libc::kill(pid as libc::pid_t, 0)`. POSIX `kill(0, sig)` sends the
signal to *every process in the caller's own process group* rather than to a specific PID, and
typically succeeds (returns 0) since the caller belongs to that group. If `agent_pid_from_file` ever
reads a PID file containing `"0"` (a corrupted/truncated write), `agent_running(0)` would report the
nonexistent "agent" as running instead of failing safe.
**Fix:**
```rust
pub fn agent_running(pid: u32) -> bool {
    pid != 0 && unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}
```

#### WR-06: `evaluate_layer2` mixes two different `project_root` sources within one function

**File:** `crates/devflow-core/src/agent_result.rs:468-502`
**Issue:** The function takes an explicit `project_root: &Path` parameter and uses it for the
`.devflow/phase-NN-exit` file path, but switches to `state.project_root` for the git subprocess
calls (`branch_exists`, commit counting). Every current call site passes the same value for both, so
there's no live bug today, but the function (which is `pub`) has no way to detect or reject a caller
that passes them inconsistently — a latent trap for future refactors or direct API callers.
**Fix:** Use the `project_root` parameter consistently for both the file paths and the git
subprocess `current_dir`, dropping the redundant reliance on `state.project_root` inside this
function.

#### WR-07: `parse_offset_minutes` doesn't validate the offset format, risking a badly wrong cron schedule

**File:** `crates/devflow-core/src/ship.rs:263-269`
**Issue:**
```rust
fn parse_offset_minutes(offset: &str) -> Option<i32> {
    let sign = if offset.starts_with('-') { -1 } else { 1 };
    let mut parts = offset.get(1..)?.split(':');
    let hours = parts.next()?.parse::<i32>().ok()?;
    let minutes = parts.next().unwrap_or("0").parse::<i32>().ok()?;
    Some(sign * (hours * 60 + minutes))
}
```
This assumes a colon-separated `HH:MM` offset and does not bound-check `hours`/`minutes`. A
non-colon numeric offset like `+0530` (a valid ISO-8601 variant, just not the RFC3339 form the tests
exercise) has no `:` to split on, so `parts.next()` consumes the whole string as `hours`
(`"0530"` → `530`), yielding an offset of 530 *hours* instead of 5 hours 30 minutes. Since this feeds
`cron_schedule_from_retry_after`, which schedules an unattended Hermes cron job to resume a
rate-limited `sequentagent` run, a malformed-but-parseable offset silently produces a wildly wrong
schedule instead of failing safe — the file's own stated design goal ("never turn unparseable agent
output into an every-minute cron") doesn't cover this case, since the input *does* parse, just to
the wrong value.
**Fix:** Require a colon and bound-check the parsed values:
```rust
fn parse_offset_minutes(offset: &str) -> Option<i32> {
    let sign = if offset.starts_with('-') { -1 } else { 1 };
    let rest = offset.get(1..)?;
    let (h, m) = rest.split_once(':')?;
    let hours = h.parse::<i32>().ok()?;
    let minutes = m.parse::<i32>().ok()?;
    if !(0..=23).contains(&hours) || !(0..=59).contains(&minutes) {
        return None;
    }
    Some(sign * (hours * 60 + minutes))
}
```

#### WR-08: Monitor's TERM/INT trap exits without terminating the orphaned agent, permanently stalling auto-advance

**File:** `crates/devflow-core/src/monitor.rs:103-117`
**Issue:** The spawned shell script installs `cleanup() { exit 0; }; trap cleanup TERM INT;` but
`cleanup` only exits the monitor shell itself — it never signals `$apid` (the backgrounded agent
process):
```rust
let script = format!(
    "cleanup() {{ exit 0; }}; trap cleanup TERM INT; \
     cd {workdir} || exit 1; \
     \"$@\" > {stdout_file} 2>{stderr_file} & \
     apid=$!; echo $apid > {pid_file}; \
     wait $apid; echo $? > {exit_file}; \
     {binary} advance {project_root}",
    ...
);
```
If an operator (or the stale-lock-reclaim path in `lock.rs`, whose own doc comment anticipates "a
killed or crashed holder") sends SIGTERM/SIGINT to a monitor to abort a run, the underlying agent
process is orphaned and keeps running/committing unsupervised, while the
`wait $apid; echo $? > {exit_file}; {binary} advance {project_root}` tail of the script never
executes — so once the orphaned agent eventually finishes, nothing calls `devflow advance` for it,
and the phase's workflow is stuck until a human runs `devflow recover`. This directly contradicts
the "Traps SIGTERM and SIGINT for clean shutdown" doc comment on the same script.
**Fix:** Kill the process (group) in the trap handler before exiting, e.g.
`cleanup() { kill "$apid" 2>/dev/null; exit 0; }; trap cleanup TERM INT;` (ensure `$apid` is set
before the trap can reference it, e.g. by initializing the variable before backgrounding).

#### WR-09: `deserialize_verdict_lenient` only tolerates malformed string *values*, not wrong JSON *types*

**File:** `crates/devflow-core/src/agent_result.rs:72-82`
**Issue:** The custom deserializer decodes `verdict` as `Option<String>` first, then maps
unknown/mis-cased strings to `None`. This handles `"verdict": "wat"` or `"verdict": "Pass"`
gracefully (covered by tests), but a `verdict` field present with a **non-string** JSON type (e.g.
`"verdict": true` or `"verdict": 123`) fails at the `Option<String>::deserialize` step itself, which
errors out the *entire* `AgentResult` parse for that marker line — precisely the failure mode the
surrounding doc comment says this exists to prevent ("a malformed verdict must never silently drop
a valid `status` to Layer 2"). This is only incidentally mitigated for the Validate stage (Layer 2
isn't commit-gated for Validate and defaults `verdict: None`, so `passed` still computes `false`) —
that mitigation is a property of Layer 2's design, not of `deserialize_verdict_lenient` itself, so
the documented guarantee is not actually met for a hypothetical non-string `verdict`.
**Fix:** Deserialize as `serde_json::Value` first and only pattern-match on the string case:
```rust
let raw = <Option<serde_json::Value> as serde::Deserialize>::deserialize(deserializer)?;
Ok(raw.and_then(|v| v.as_str().and_then(|s| match s {
    "pass" => Some(Verdict::Pass),
    "gaps" => Some(Verdict::Gaps),
    _ => None,
})))
```

#### WR-10: `start`'s pre-flight divergence check inspects the wrong branch for the (default) worktree path

**File:** `crates/devflow-cli/src/main.rs:346-357`
**Issue:**
```rust
// Pre-start divergence check: runs on current HEAD before any git mutation.
if let Ok((_ahead, behind)) = GitFlow::new(project_root).divergence_from_develop() {
    if behind > 50 {
        return Err(CliError::Message(format!(
            "develop is {behind} commits ahead — your branch is too far behind. \
             Rebase onto develop first, or use --force to override."
        )));
    }
    ...
}
```
`divergence_from_develop()` compares `develop` against whatever branch is currently checked out in
the **main** repository checkout. Since worktree mode is the default (`--no-worktree` is the
opt-out), the actual work always forks fresh from `develop` via
`worktree::add(..., DEVELOP, ...)` — independent of what happens to be checked out in the primary
working tree. A stale/unrelated branch left checked out in the main repo (e.g. an old feature branch
from an unrelated task) can cause `start` to hard-fail with a "develop is N commits ahead" error
that has nothing to do with the new phase's worktree, which will always start at `ahead=0,
behind=0`. Conversely, if the main checkout happens to be on `develop` itself, the check silently
no-ops regardless of anything meaningful.
**Fix:** Drop this check for the worktree-mode path (it is meaningful for the `--no-worktree`
branch-in-place flow), or compute divergence against the target worktree branch's actual start point
instead of the main checkout's current HEAD.

#### WR-11: `start` persists in-progress state before the agent launch is confirmed to succeed

**File:** `crates/devflow-cli/src/main.rs:386-387`
**Issue:**
```rust
workflow::save_state(&state)?;
launch_stage(&state, None)?;
```
State is written to disk *before* `launch_stage` (which spawns the monitor/agent) is known to
succeed. If `launch_stage` fails — e.g. the configured agent binary is missing
(`AgentError::NotFound`), or `monitor::spawn_monitor` fails to spawn `sh` — `start` returns an
error, but `.devflow/state.json` already reflects a phase "in progress" with no agent PID and no
monitor running. `devflow status`/`devflow recover` then report a stuck-looking state that requires
the operator to know to run `devflow recover --clean`, even though nothing was actually launched.
**Fix:** Save state after confirming `launch_stage` succeeded, or roll back
(`workflow::clear_state`) on a `launch_stage` error.

#### WR-12: Unbounded recursion in JSON traversal helpers used on agent-controlled stdout

**File:** `crates/devflow-core/src/agent_result.rs:203-234`
**Issue:** `json_has_str`, `json_has_i64`, and `json_find_key` recurse into every nested
object/array value with no depth limit:
```rust
fn json_has_str(value: &serde_json::Value, key: &str, expected: &str) -> bool {
    match value {
        serde_json::Value::Object(map) => map.iter().any(|(k, v)| {
            (k == key && v.as_str() == Some(expected)) || json_has_str(v, key, expected)
        }),
        serde_json::Value::Array(values) => values.iter().any(|v| json_has_str(v, key, expected)),
        _ => false,
    }
}
```
These are called on the coding agent's raw stdout (`detect_claude_rate_limit`, via
`evaluate_layer1`, which every `devflow advance` invocation runs). Deeply nested JSON — accidental
(an agent echoing a large structured document) or adversarial (prompt injection convincing the agent
to emit deeply nested output) — can stack-overflow the `devflow advance` process, aborting the whole
stage-advance flow.
**Fix:** Convert to an iterative traversal with an explicit work-stack, or cap recursion depth and
treat over-depth input as "no match" rather than crashing.

### Info

#### IN-01: `GateAction::from_response` silently ignores a contradictory "abort" note when `approved: true`

**File:** `crates/devflow-core/src/gates.rs:69-79`
**Issue:** The approval check (`if response.approved { return GateAction::Advance; }`) short-circuits
before the note is ever inspected, so a human response of
`{"approved": true, "note": "abort: changed my mind"}` silently advances instead of surfacing the
contradiction.
**Fix:** Treat an "abort"-containing note as taking precedence regardless of `approved`, or at least
log a warning when both signals are present and disagree.

#### IN-02: Process-liveness check duplicated with two different implementations

**File:** `crates/devflow-core/src/lock.rs:88-97` vs `crates/devflow-core/src/agent.rs:69-73`
**Issue:** `agent::agent_running` checks liveness via `libc::kill(pid, 0)` directly (no subprocess).
`lock::pid_is_alive` re-implements the same check by shelling out to an external `kill -0 <pid>`
binary via `std::process::Command`, which additionally requires a standalone `kill` executable on
`PATH` (it isn't invoked through `sh -c`, so a shell builtin won't satisfy it).
**Fix:** Have `pid_is_alive` call `agent::agent_running` (or factor the shared check into one place
both modules depend on) instead of maintaining two divergent implementations.

#### IN-03: `devflow-core` path dependency hardcodes a version instead of using workspace inheritance

**File:** `crates/devflow-cli/Cargo.toml:13`
**Issue:** `devflow-core = { path = "../devflow-core", version = "1.2.0" }` pins an explicit version
string while `devflow-cli`'s own `version.workspace = true` uses workspace inheritance. This risks
silent drift if `devflow-core`'s version is bumped without this line being updated to match.
**Fix:** Use `version.workspace = true` for this dependency as well (or drop the `version` key
entirely for the internal path dependency).

---

_Reviewed: 2026-07-15T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
