---
phase: 12-bootstrap-housekeeping
reviewed: 2026-07-10T23:34:13Z
depth: standard
files_reviewed: 24
files_reviewed_list:
  - crates/devflow-cli/Cargo.toml
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/tests/phase7_cli.rs
  - crates/devflow-core/Cargo.toml
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
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/recover.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/stage.rs
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/src/version.rs
  - crates/devflow-core/src/workflow.rs
  - crates/devflow-core/tests/monitor_e2e.rs
findings:
  critical: 1
  warning: 3
  info: 2
  total: 6
status: issues_found
---

# Phase 12: Code Review Report

**Reviewed:** 2026-07-10T23:34:13Z
**Depth:** standard
**Files Reviewed:** 24
**Status:** issues_found

## Summary

Phase 12 closes out the Phase 11 code-review debt list (WR-01 through WR-10,
IN-02 through IN-05) and adds targeted orchestration test coverage. I verified
`cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D
warnings`, and read every file in scope in full (not just the diff) against
each plan's stated intent.

**The five specifically-flagged focus items all check out as genuinely
fixed:**

- **WR-07 (`workflow.rs`)** — `save_state` now writes to a sibling `.tmp` file
  and `rename`s it over `state.json`. `rename(2)` is atomic on the same
  filesystem, so a mid-write kill leaves the old `state.json` (or nothing, on
  first write) intact — never a truncated/partial file. Confirmed correct.
- **WR-01 (`monitor.rs`)** — the agent program/args no longer pass through
  string interpolation into the shell script body; they are appended as
  literal `Command::args()` after a `sh -c <script> sh` argv0 placeholder and
  invoked inside the script via `"$@"`. Traced the argv boundary by hand and
  confirmed the `spawn_monitor_treats_agent_args_as_literal_argv` test
  actually exercises a shell-metacharacter-bearing argument end-to-end. No
  injection surface remains for agent-controlled data.
- **WR-06 (`main.rs` / `ship.rs`)** — `retry_after_from_reason` now falls back
  to the literal string `"unknown"` instead of `.or(reason)`, and
  `cron_schedule_from_retry_after` returns `Option<String>`; an unparseable
  reason now serializes to an **empty** `hermes_cron.schedule`, never `* * * *
  *`. The runaway-schedule bug is gone.
- **IN-03 (`agents/mod.rs`, `state.rs`, all call sites)** — grepped every
  `\bAgent\b` occurrence workspace-wide; the enum is `AgentKind` (alias
  deleted) and the trait is `AgentAdapter` everywhere, including the
  `Box<dyn AgentAdapter>` return of `adapter_for`, all three
  `impl AgentAdapter for XAgent` blocks, and every test module. Dispatch is
  intact (`cargo build --workspace` is clean, `adapter_for_returns_correct_names`
  passes).
- **IN-02 (`state.rs`)** — `agent_result`/`agent_stdout_path` and the
  now-unused `AgentResult` import are gone; nothing else in the workspace
  referenced them (grep confirms).

**However, one real, previously-unflagged defect survived all twelve plans**
and is not caught by any of the new orchestration tests added in 12-08/12-09:
gate response files are never deleted outside the terminal success path, so a
second gate for the same phase+stage can silently resolve from stale,
previously-consumed human input. See CR-01.

## Critical Issues

### CR-01: Stale gate response files are silently reused by a later gate for the same phase+stage

**File:** `crates/devflow-core/src/gates.rs:176-188`, `crates/devflow-cli/src/main.rs:426-475`

**Issue:**

`Gates::cleanup` (removes the `.json` request, `.response.json`, and
`.ack.json` files for a phase+stage) is only ever called from
`finish_workflow()` in `main.rs:491-492` — the terminal *Ship-approved*
success path. It is **not** called from:

- `loop_back_to_code()` (`main.rs:462-475`) — reached when a gate's
  `GateAction::LoopBack` fires (a Validate or Ship gate rejected without an
  "abort" note), or
- `abort()` (`main.rs:527-531`) — reached when a gate's `GateAction::Abort`
  fires.

`run_gate()` (`main.rs:500-524`) writes a fresh gate *request* file each time
via `Gates::write_gate`, but never touches the *response* file — it only reads
it via `Gates::poll_response` and later writes an `.ack.json`. Since
`response_path`/`ack_path` are keyed purely by `(phase, stage)`
(`gates.rs:107-113`), the previous response/ack files are left on disk after a
LoopBack or an Abort.

Concretely: phase 22 fails Validate repeatedly, a gate is forced, a human
rejects with `"abort: requirements changed"` → `GateAction::Abort` →
`abort()` clears `state.json` but leaves
`.devflow/gates/22-validate.response.json` and `.ack.json` on disk forever.
If phase 22 is later restarted (e.g. `devflow start --phase 22 --force` after
fixing the issue) and reaches Validate's forced gate again, `run_gate` writes
a new request file, then `Gates::poll_response` immediately reads the
**old, already-consumed** response file and returns it — with no human ever
having looked at the new occurrence. Depending on what that stale response
says, this can re-trigger an unwanted abort/loop-back, or — if the leftover
response happened to be an approval — silently advance the workflow (e.g.
approve a Ship) without a fresh human decision. This defeats the entire
purpose of the gate-file protocol (a deliberate human-in-the-loop pause) and
is exactly the class of defect Phase 12's own new orchestration tests
(`validate_failure_threshold_forces_gate_then_aborts`,
`advance_ship_success_runs_finish_workflow`) were meant to harden — but
neither asserts that gate files are actually cleaned up on the LoopBack/Abort
paths, only on the terminal success path.

This predates Phase 12 (the `abort`/`loop_back_to_code` functions are
unchanged since the Phase 11 CLI rewrite, `7bac473`), but it lives squarely in
files this phase modified and tested (`gates.rs` via 12-08,
`main.rs` via 12-02/12-09/12-11), and none of the dozen plans in this phase
caught it.

**Fix:**

Clean up the stage's gate files on every terminal `GateAction`, not just
`Advance`. Capture `state.stage` before it gets mutated to `Stage::Code`:

```rust
fn loop_back_to_code(project_root: &Path, state: &mut State) -> Result<(), CliError> {
    let _ = Gates::cleanup(project_root, state.phase, state.stage);
    state.stage = Stage::Code;
    state.gate_pending = false;
    workflow::save_state(state)?;
    ...
}

fn abort(project_root: &Path, state: &State, reason: &str) -> Result<(), CliError> {
    println!("workflow aborted for phase {}: {reason}", state.phase);
    let _ = Gates::cleanup(project_root, state.phase, state.stage);
    let _ = workflow::clear_state(project_root);
    Ok(())
}
```

Add a regression test that seeds a response file, drives a gate to
`LoopBack` (or `Abort`), then asserts `Gates::response_path(...).exists() ==
false` before re-writing a gate for the same phase+stage and confirming
`poll_response` actually blocks/times out instead of returning instantly.

## Warnings

### WR-01-review: `devflow doctor`'s `cmd_check` reports "ok" for a failing command

**File:** `crates/devflow-cli/src/main.rs:1238-1251`

**Issue:** `cmd_check`'s `Ok(out)` arm (command spawned but exited non-zero)
sets `status: "ok".into()` — identical to the `Ok(out) if out.status.success()`
arm. A tool that is installed but broken (e.g. `claude --version` exiting
non-zero due to a corrupt install, license issue, or incompatible flag) is
reported to the user/CI as `✓ ok`, defeating the stated purpose of `devflow
doctor` ("report what's installed, missing, or **broken**"). This predates
Phase 12 (introduced in `6063d3d6`, before Phase 11), but it lives in a file
this phase repeatedly touched (12-02, 12-09, 12-11) and none of those plans'
`cargo run -p devflow -- doctor` verification runs would have caught it since
they only exercised the happy path.

**Fix:**

```rust
Ok(out) => {
    let version = String::from_utf8_lossy(&out.stderr)
        .lines()
        .next()
        .unwrap_or("unknown")
        .trim()
        .to_string();
    Check {
        name: name.into(),
        status: "warn".into(),
        version: Some(version),
        install_hint: Some(format!("`{cmd} {version_arg}` exited non-zero — reinstall or check {name}")),
    }
}
```

### WR-08-review: widened `shell_quote` safe set includes `~`, which the shell tilde-expands at word start

**File:** `crates/devflow-core/src/ship.rs:456-471`

**Issue:** The 12-10 fix widens `shell_quote`'s unquoted-safe character set to
include `~ : @ + = %`, with the stated rationale that widening "can never
under-quote" because unsafe input still falls through to single-quoting.
That rationale doesn't hold for `~`: an unquoted `~` at the *start* of a
shell word triggers POSIX tilde expansion (substituting the invoking user's
home directory), so a value like `"~/proj"` passed through `shell_quote`
unquoted is not passed to the shell literally — it is expanded. This is a
real semantic difference, not just "reduced over-quoting." The current call
site (`build_cron_instructions`'s `cd {} && devflow sequentagent ...`,
quoting `project_root.display()`) is not exploitable in practice because an
absolute path never starts with `~`, but the helper is public-shaped/general
enough that this is a latent correctness trap for any future caller passing
a user-supplied or relative value.

**Fix:** Special-case a leading `~` to fall through to quoting regardless of
the rest of the string:

```rust
if !value.starts_with('~')
    && value.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '/' | '.' | '_' | '-' | '~' | ':' | '@' | '+' | '=' | '%')
    })
{
    value.to_string()
} else {
    format!("'{}'", value.replace('\'', "'\\''"))
}
```

### WR-04-review: `docs_update()` hook commits via `git add .`, not just docs changes

**File:** `crates/devflow-core/src/hooks.rs:117-137`

**Issue:** `docs_update()` calls `GitFlow::commit_all("docs: update generated
docs")`, which internally runs `git add .` (`git.rs:243`) before committing.
If any other file in the working tree is dirty at the moment this hook fires
(e.g. a stray file the agent left uncommitted, or a concurrent worktree
change bleeding into the main checkout), it gets swept into the commit under
a misleading "docs: update generated docs" message. This is unchanged by
Phase 12, but 12-04 added direct test coverage for this exact hook
(`validate_to_ship_hooks_append_changelog`) without exercising or asserting
scoped staging, so the gap remains undetected by the phase's own new tests.

**Fix:** Stage only the doc output path(s) (e.g. `target/doc` or whatever
`cargo doc` writes) instead of `git add .`, or add a `git status --porcelain`
guard that skips the commit (with a warning) when unrelated paths are dirty.

## Info

### IN-review-1: Locale-dependent unmerged-branch classification in `branch_cleanup`

**File:** `crates/devflow-core/src/hooks.rs:98-114`

**Issue:** WR-03's fix classifies a failed branch delete as "not merged yet"
by substring-matching git's English stderr text (`"not fully merged"` /
`"not yet merged"`). Under a non-English `LANG`/`LC_ALL` in the environment
running `devflow`, git emits a localized message, the match silently misses,
and the cleanup falls through to the generic `"could not delete {branch}:
{err}"` warning instead of the friendlier one. Both paths are already
fail-soft (warn, not error), so this is a message-quality gap, not a
correctness bug.

**Fix:** Either force `LC_ALL=C` on the underlying `git` invocation in
`GitFlow::git`/`git_raw`, or document the English-locale assumption next to
the match.

### IN-review-2: `write_atomic` duplicated verbatim between `gates.rs` and `workflow.rs`

**File:** `crates/devflow-core/src/gates.rs:193-201`, `crates/devflow-core/src/workflow.rs:43-51`

**Issue:** `Gates`'s private `write_atomic` and `Workflow`'s private
`write_state_atomic` are structurally identical (temp-file-in-same-dir +
`rename`), differing only in their error type. This is a deliberate,
documented tradeoff from 12-01's decision log ("keep the atomic-write helper
local and typed to `WorkflowError`... to avoid cross-module error
coupling"), so this is not a defect — just a note that a third caller
needing the same pattern should prompt extracting a shared
`fs::write_atomic(path, contents) -> io::Result<()>` helper that both error
types wrap via `#[from]`.

**Fix:** No action required now; revisit if a third module needs
atomic-write semantics.

---

_Reviewed: 2026-07-10T23:34:13Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
