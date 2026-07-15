---
phase: 13-mvp-core-loop
reviewed: 2026-07-15T00:00:00Z
depth: standard
files_reviewed: 13
files_reviewed_list:
  - crates/devflow-cli/Cargo.toml
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/tests/phase7_cli.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/agent.rs
  - crates/devflow-core/src/agents/codex.rs
  - crates/devflow-core/src/agents/mod.rs
  - crates/devflow-core/src/gates.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/lock.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/ship.rs
findings:
  critical: 2
  warning: 8
  info: 3
  total: 13
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
`agents/*`), the gate/human-handoff protocol (`gates.rs`), git-flow plumbing (`git.rs`), the
project lock (`lock.rs`), the detached monitor daemon (`monitor.rs`), stage prompts (`prompt.rs`),
and ship/cron bookkeeping (`ship.rs`).

The code is generally careful about the specific failure modes it explicitly set out to fix (stale
gates, silent Validate/Ship advances, shell injection via monitor argv, GPG-sign hangs, stale
locks) — those areas are well tested. However, two cross-cutting correctness bugs undermine the
pipeline's core guarantee ("the agent's self-reported result is authoritative"): output capture
silently discards the entire stdout buffer on invalid UTF-8 (which can turn a Ship-stage rejection
into an apparent success), and `sequentagent`'s synchronous agent runner never actually parses a
real Codex agent's JSONL-wrapped result marker, only the (unused-here) async monitor path does.
Several other warnings cover a doctor mis-report, a stale cron-instruction artifact, a `git branch
--merged` parsing gap that can break the whole cleanup pass, and a few smaller edge cases.

## Narrative Findings (AI reviewer)

### Critical Issues

#### CR-01: Agent stdout capture silently discards all output on invalid UTF-8

**File:** `crates/devflow-core/src/agent.rs:87-91`
**Issue:** `capture_agent_output` reads the agent's stdout pipe with
`let _ = pipe.read_to_string(&mut stdout);`. `Read::read_to_string` guarantees that on invalid
UTF-8 the destination buffer is left **unchanged** (i.e. still empty, since `stdout` starts as
`String::new()`) and returns an `Err` — which is silently discarded here. Any agent run whose
stdout contains even one invalid UTF-8 byte anywhere in the stream (e.g. a tool echoing a binary
file, a truncated multi-byte sequence in terminal-color output) causes the *entire* captured
output — including a perfectly valid `DEVFLOW_RESULT` marker — to be lost. The empty string is
then written verbatim to `.devflow/phase-NN-stdout`.

Downstream impact is worst for the Ship stage: `evaluate_layer1` finds nothing (empty file) and
falls through to Layer 2, which for `Stage::Ship` is *not* commit-gated, so `exit_code == 0` alone
yields `AgentStatus::Success`. A real agent that decided `DEVFLOW_RESULT: {"status":"failed",
"reason":"review: <critical findings>"}` (the ReviewFailed contract `handle_ship_failure` depends
on) would have that entire self-report vanish, and the pipeline would proceed to
`handle_ship_outcome` → "Ship complete — approve merge?" instead of looping back to Code — exactly
the outcome the `review:` contract exists to prevent.
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
It never calls `parse_codex_event_result`. That function — the one that actually knows how to
find the `DEVFLOW_RESULT` marker inside a Codex `--json` event stream's `item.completed` /
`agent_message.text` field (per the 13-06 dogfood finding documented directly above it) — is a
private (non-`pub`) function in `agent_result.rs`, wired in only via `evaluate_layer1`, which is
only reached through `evaluate_agent_result` in the async `advance()` path (`main.rs`'s
`Command::Advance`), not through `run_agent_blocking`.

Real Codex output (`codex exec --json`) never contains a raw top-level `DEVFLOW_RESULT:` line —
`parse_devflow_result`'s line scan cannot see it. So for `sequentagent`'s Codex leg, `parse_marker_
lines` returns `None`, `detect_rate_limit`'s Codex plain-text heuristic also finds nothing (real
output is pure JSONL, no "try again at" text), and `run_agent_blocking` returns `Ok(None)`. Back in
`sequentagent`, `if let Some(result) = run_agent_blocking(...)? { match result.status { ... } }` —
with `None`, the whole match is skipped and execution falls straight through to
`integrate_agent_branch`, treating a Codex agent that self-reported `"status":"failed"` (or
anything else) exactly the same as a successful run. The existing tests don't catch this because
the fake Codex fixture scripts print a raw `DEVFLOW_RESULT:` line directly instead of Codex's real
JSONL envelope.
**Fix:** Make `parse_codex_event_result` `pub(crate)` and have `run_agent_blocking` mirror the
same precedence `evaluate_layer1` uses, e.g.:
```rust
agent_result::parse_devflow_result(&capture.stdout)
    .or_else(|| agent_result::parse_codex_event_result(&capture.stdout))
    .or_else(|| agent_result::detect_rate_limit(&capture.stdout).map(...))
```
(or, better, refactor `run_agent_blocking` to reuse the shared `evaluate_layer1`-equivalent logic
directly so the two callers can never drift again.)

### Warnings

#### WR-01: `doctor`'s `cmd_check` reports a command as "ok" even when it exits non-zero

**File:** `crates/devflow-cli/src/main.rs:1474-1487`
**Issue:** In `cmd_check`, the `Ok(out)` arm (i.e. the command spawned but returned a non-success
exit status) still sets `status: "ok".into()` and surfaces `stderr`'s first line as if it were a
version string. `doctor`'s entire purpose is to report "what's installed, missing, or broken" —
this branch means a broken/misbehaving binary (wrong flag, corrupted install, permission error) is
reported identically to a healthy one, just with garbage text in the version column.
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

**File:** `crates/devflow-cli/src/main.rs:1011-1025, 1069-1084`
**Issue:** When agent A is rate-limited but has produced commits, `write_rate_limit_cron` always
writes `.devflow/cron-instructions.json` (line 1013) before falling through to hand off to agent
B. If agent B then completes successfully, `sequentagent` finishes with `"sequentagent complete"`
but never deletes the cron-instructions file it just wrote — `delete_cron_instructions` is only
called once, at the very start of `sequentagent` (line 972), not on success. `devflow status`
(`cron_instruction_hint`) will keep telling the operator "Cron instruction pending: hermes cron
create --from-devflow ..." indefinitely for a phase that has already shipped, and if a Hermes cron
job actually gets created from that stale file, it will needlessly re-run `sequentagent` on an
already-completed phase.
**Fix:** Delete the cron-instructions file once agent B completes successfully (end of
`sequentagent`, right before "sequentagent complete"):
```rust
integrate_agent_branch(&git, &base, &branch_b)?;
let _ = devflow_core::ship::delete_cron_instructions(project_root);
println!("\nsequentagent complete — both agents integrated into {base}");
```

#### WR-03: `git.rs`'s `cleanup_merged` doesn't strip the `+` worktree prefix from `git branch --merged`

**File:** `crates/devflow-core/src/git.rs:236-250`
**Issue:** `git branch --merged` prefixes the *current* branch with `*` and any branch checked out
in a **linked worktree** with `+` (not `*`). `cleanup_merged` only strips `*`:
```rust
let branch = line.trim().trim_start_matches('*').trim();
```
If a merged branch happens to still be checked out in a worktree at the time this runs (e.g. the
`reference` worktree pinned to a now-merged branch, or any consumer calling this API directly
rather than through the `cleanup` CLI command's worktree-removal-first ordering), `branch` ends up
as `"+branch-name"` — an invalid ref — and `self.git(["branch", "-d", branch])?` fails, propagating
an `Err` via `?` that aborts the *entire* `cleanup_merged` call (losing any branches already
identified for deletion earlier in the loop), not just the one problem branch.
**Fix:**
```rust
let branch = line.trim().trim_start_matches(['*', '+']).trim();
```

#### WR-04: `agent_running(0)` can report a dead/corrupt PID as running

**File:** `crates/devflow-core/src/agent.rs:69-73`
**Issue:** `agent_running` calls `libc::kill(pid as libc::pid_t, 0)`. POSIX `kill(0, sig)` sends
the signal to *every process in the caller's own process group*, not to a specific PID, and
typically succeeds (returns 0) since the caller belongs to that group. If `agent_pid_from_file`
ever reads a PID file containing `"0"` (corrupted/truncated write), `agent_running(0)` would
report the (nonexistent) "agent" as running rather than failing safe.
**Fix:**
```rust
pub fn agent_running(pid: u32) -> bool {
    pid != 0 && unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}
```

#### WR-05: `evaluate_layer2` mixes two different `project_root` sources within one function

**File:** `crates/devflow-core/src/agent_result.rs:468-502`
**Issue:** The function takes an explicit `project_root: &Path` parameter and uses it for the
`.devflow/phase-NN-exit` file path, but then switches to `state.project_root` for both git
subprocess calls (`branch_exists`, commit counting). Every current call site passes the same value
for both, so there's no live bug today, but the function has no way to detect or reject a caller
that passes them inconsistently — a latent trap for future refactors or direct API callers (this
function is `pub`).
**Fix:** Use the `project_root` parameter consistently for both the file paths and the git
subprocess `current_dir`, and drop the redundant reliance on `state.project_root` inside this
function.

#### WR-06: `parse_offset_minutes` doesn't validate the offset format, risking a badly wrong cron schedule

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
non-colon numeric offset like `+0530` (a valid ISO-8601 variant, just not the RFC3339 form the
tests exercise) has no `:` to split on, so `parts.next()` consumes the whole string as `hours`
(`"0530"` → `530`), yielding an offset of 530 *hours* instead of 5 hours 30 minutes. Since this
feeds `cron_schedule_from_retry_after`, which schedules an unattended Hermes cron job to resume a
rate-limited `sequentagent` run, a malformed-but-parseable offset silently produces a wildly wrong
schedule instead of failing safe (the explicit design goal stated in the `WR-06` comment already in
this file — "never turn unparseable agent output into an every-minute cron" — doesn't cover this
case, since it *does* parse, just to the wrong value).
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

#### WR-07: Monitor's TERM/INT trap exits without terminating the orphaned agent, permanently stalling auto-advance

**File:** `crates/devflow-core/src/monitor.rs:103-117`
**Issue:** The spawned shell script installs `cleanup() { exit 0; }; trap cleanup TERM INT;` but
`cleanup` only exits the monitor shell itself — it never kills `$apid` (the backgrounded agent
process). If an operator (or the stale-lock-reclaim path in `lock.rs`, whose own doc comment
anticipates "a killed or crashed holder") sends SIGTERM/SIGINT to a monitor to abort a run, the
underlying agent process is orphaned and keeps running/committing unsupervised, while the
`wait $apid; echo $? > {exit_file}; {binary} advance {project_root}` tail of the script never
executes — so once the orphaned agent eventually finishes, nothing calls `devflow advance` for it,
and the workflow is stuck until a human runs `devflow recover`.
**Fix:** Kill the process group (or `$apid` explicitly) in the trap handler before exiting:
```sh
cleanup() { kill "$apid" 2>/dev/null; exit 0; }; trap cleanup TERM INT;
```
(note `$apid` must be set before the trap can use it, or track it via a separate variable
initialized before backgrounding).

#### WR-08: `deserialize_verdict_lenient` only tolerates malformed string *values*, not wrong JSON *types*

**File:** `crates/devflow-core/src/agent_result.rs:72-82`
**Issue:** The custom deserializer decodes the `verdict` field as `Option<String>` first, then maps
unknown/mis-cased strings to `None`. This handles `"verdict": "wat"` or `"verdict": "Pass"`
gracefully (covered by tests), but a `verdict` field present with a **non-string** JSON type (e.g.
`"verdict": true` or `"verdict": 123`) fails at the `Option<String>::deserialize` step itself,
which errors out the *entire* `AgentResult` parse for that marker — precisely the failure mode the
surrounding doc comment says this exists to prevent ("a malformed verdict must never silently drop
a valid `status` to Layer 2"). In practice this is mitigated for the Validate stage specifically,
because Layer 2 is not commit-gated for Validate and always sets `verdict: None`, so `passed`
still computes to `false` (fail-safe) — but the mitigation is incidental to Layer 2's design, not a
property of `deserialize_verdict_lenient` itself, and the stated guarantee is not actually met.
**Fix:** Deserialize as `serde_json::Value` first and only pattern-match on the string case,
treating any other type the same as absent/unknown:
```rust
let raw = <Option<serde_json::Value> as serde::Deserialize>::deserialize(deserializer)?;
Ok(raw.and_then(|v| v.as_str().and_then(|s| match s {
    "pass" => Some(Verdict::Pass),
    "gaps" => Some(Verdict::Gaps),
    _ => None,
})))
```

### Info

#### IN-01: `GateAction::from_response` silently ignores a contradictory "abort" note when `approved: true`

**File:** `crates/devflow-core/src/gates.rs:69-79`
**Issue:** The approval check (`if response.approved { return GateAction::Advance; }`) short-
circuits before the note is ever inspected, so a human response of `{"approved": true, "note":
"abort: changed my mind"}` silently advances instead of surfacing the contradiction.
**Fix:** Consider treating an "abort"-containing note as taking precedence regardless of
`approved`, or logging a warning when both signals are present and disagree.

#### IN-02: Process-liveness check duplicated with two different implementations

**File:** `crates/devflow-core/src/lock.rs:88-97` vs `crates/devflow-core/src/agent.rs:69-73`
**Issue:** `agent::agent_running` checks liveness via `libc::kill(pid, 0)` directly (no
subprocess). `lock::pid_is_alive` re-implements the same check by shelling out to an external
`kill -0 <pid>` binary via `std::process::Command`, which additionally requires an actual `kill`
executable on `PATH` (not just a shell builtin, since it isn't invoked through `sh -c`).
**Fix:** Have `pid_is_alive` call `agent::agent_running` (or move the shared check into one place
both modules depend on) instead of maintaining two divergent implementations.

#### IN-03: `devflow-core` path dependency hardcodes a version instead of using workspace inheritance

**File:** `crates/devflow-cli/Cargo.toml:13`
**Issue:** `devflow-core = { path = "../devflow-core", version = "1.2.0" }` pins an explicit
version string while `devflow-cli`'s own `version.workspace = true` uses workspace inheritance.
This risks silent drift if `devflow-core`'s version is bumped without updating this line.
**Fix:** If the workspace publishes both crates in lockstep, use `version.workspace = true` here as
well (or drop the version key entirely for the internal path dependency).

---

_Reviewed: 2026-07-15T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
