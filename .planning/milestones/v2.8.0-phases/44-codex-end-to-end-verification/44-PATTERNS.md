# Phase 44: Codex End-to-End Verification - Pattern Map

**Mapped:** 2026-08-26
**Files analyzed:** 8 modified source files (no new files) + 2 evidence/snapshot artifacts
**Analogs found:** 8 / 8 (all in-file — this phase extends existing functions, it does not add new
modules)

**Note on analogs:** Every file this phase touches already contains the closest possible analog to
itself — a sibling function or a pre-existing test in the SAME file, doing the SAME job for a
different case (e.g. `Start`'s `--agent` flag is the analog for `Resume`'s new `--agent` flag, in
the same `main.rs` enum). RESEARCH.md already pinned every integration point to an exact
file:line read this session; this document adds the surrounding code context (full imports,
neighboring precedent, test harness idioms) RESEARCH.md's excerpts trimmed for brevity.

## File Classification

| Modified File | Role | Data Flow | Closest Analog (same file unless noted) | Match Quality |
|---|---|---|---|---|
| `crates/devflow-cli/src/main.rs` (`Command::Resume` + dispatch) | route/config (clap CLI surface) | request-response | `Command::Start`'s `--agent: AgentKind` field (same file, line 54-55) | exact |
| `crates/devflow-cli/src/pipeline_launch.rs` (`resume()`, handoff mutation) | controller (pipeline orchestration) | CRUD (state mutate) + event-driven | `apply_legacy_launch_opt_out` + `repair_leaked_auto_chain_flag` (same file, lines 344-368) — the existing "operator override mutates persisted state before relaunch, prints + will be event-logged" shape | exact |
| `crates/devflow-cli/src/pipeline_launch.rs` (`spawn_agent_and_record`, resume-side delete) | controller | event-driven | `capture_archived` emission immediately after a durable write, same function (lines 1012-1023) | exact |
| `crates/devflow-cli/src/pipeline_gate.rs` (`finish_workflow_with_gate_timeout`, ship-side delete) | controller | event-driven | `workflow_shipped` emission immediately after `workflow::clear_state` + `registry::deregister`, same function (lines 275-299) | exact |
| `crates/devflow-cli/src/commands.rs` (`cron_hint_line`) | utility (string rendering) | transform | `describe_worktree_dir` (same file, lines 2019-2029) — pure string-transform-with-tests sibling | exact |
| `crates/devflow-core/src/ship.rs` (`HermesCronJob.schedule` render + `cron_schedule_from_retry_after`) | utility (pure data transform) | transform | `shell_quote` (same file, lines 374-389) — pure string-safety transform with its own dedicated unit tests | exact |
| `crates/devflow-cli/src/preflight.rs` (read-only reference; `resume`'s pre-check must mirror this predicate, not duplicate a stricter one) | middleware (guard) | request-response | `preflight_interactivity_check` itself (lines 607-634) is the pattern source — no new file needed, extract-and-share or call directly | exact |
| `crates/devflow-cli/tests/snapshots/devflow-help.txt` | config (golden fixture) | file-I/O | regenerated via `cargo run -q -p devflow -- --help > ...` per `help_output_matches_committed_snapshot`'s own doc comment | exact |

## Pattern Assignments

### `crates/devflow-cli/src/main.rs` — add `--agent: Option<AgentKind>` to `Command::Resume`

**Analog:** `Command::Start`'s existing `agent` field, same file.

**Imports** (lines 1-5, already present, no new imports needed):
```rust
use clap::{Parser, Subcommand};
use devflow_core::mode::Mode;
use devflow_core::stage::Stage;
use devflow_core::state::AgentKind;
use std::path::PathBuf;
```

**Core pattern — `Start`'s required `--agent` (lines 49-58), to mirror as `Resume`'s OPTIONAL
`--agent`:**
```rust
Start {
    /// Phase number to work on.
    #[arg(long)]
    phase: PhaseId,
    /// Agent to launch.
    #[arg(long, default_value = "claude")]
    agent: AgentKind,
    ...
```

**Current `Resume` definition to extend** (lines 161-181):
```rust
/// Resume a phase from its saved stage after a rate limit or infrastructure pause.
///
/// Unlike `start`, this loads the persisted per-phase state and
/// relaunches its saved stage — it does NOT create a new branch/worktree
/// or reset the workflow to Define (review consensus #5); agent and mode
/// come from the saved state.
Resume {
    /// Phase to resume.
    #[arg(long)]
    phase: PhaseId,
    /// Force the pre-31 single-document Claude launch for the rest of this
    /// run (D-11, `31-CONTEXT.md`). Same semantics as `devflow start
    /// --legacy-claude-launch`, offered here so a run already in flight can
    /// be moved onto the legacy path without restarting it. Never cleared
    /// by a later plain `devflow resume`.
    #[arg(long)]
    legacy_claude_launch: bool,
    /// Project root.
    #[arg(default_value = ".")]
    project: PathBuf,
},
```
New field must be `Option<AgentKind>` (not `AgentKind` with a default) since the doc comment's own
"agent... come[s] from the saved state" invariant must remain true when the flag is omitted — an
`AgentKind::Claude` default would silently force every plain `devflow resume` onto Claude.

**Dispatch site to extend** (lines 589-593):
```rust
Command::Resume {
    phase,
    legacy_claude_launch,
    project,
} => resume(&project_root(project)?, phase, legacy_claude_launch),
```
Becomes `resume(&project_root(project)?, phase, agent, legacy_claude_launch)` (or a small options
struct if the parameter count grows unwieldy — no existing precedent for a struct here, positional
params are this codebase's convention for `resume`/`launch_stage`).

**Validation pattern:** `AgentKind` already implements clap's value parser via `FromStr` (used
identically by `Start`'s `agent` field and `Monitor`'s `agent` field, line 156) — no new parsing
surface, per RESEARCH.md's ASVS V5 note.

---

### `crates/devflow-cli/src/pipeline_launch.rs` — `resume()` handoff mutation (D-05/D-06/D-07/D-08)

**Analog:** `apply_legacy_launch_opt_out` + its call site inside `resume()`, same file.

**Imports** (lines 23-42, already present):
```rust
use crate::CliError;
use crate::pipeline_gate::transition;
use crate::pipeline_outcomes::{
    ValidateOutcome, classify_validate_outcome, handle_infra_outcome, handle_rate_limited_outcome,
    handle_ship_failure, handle_ship_outcome, handle_stage_failure, handle_validate_outcome,
    truncate_reason,
};
use crate::preflight::{ensure_agent_binary, run_preflight, worktree_writable_roots};
use devflow_core::config::{GitFlowConfig, capture_retention};
use devflow_core::mode::Mode;
use devflow_core::outcome_policy::{self, Action};
use devflow_core::phase_id::PhaseId;
use devflow_core::prompt;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::{
    agent_result, agents, canary, events, gsd_config, lock, mode, monitor, verify, workflow,
};
use std::path::{Path, PathBuf};
use tracing::{info, warn};
```
`crate::preflight::{...}` will need a new re-export (or a `pub(crate)` visibility bump) if D-08's
pre-check reuses `preflight_interactivity_check` directly — it is currently a private `fn` in
`preflight.rs` (line 607, no `pub(crate)`).

**Current full `resume()` body** (lines 1237-1286) — the exact function this phase extends:
```rust
pub(crate) fn resume(
    project_root: &Path,
    phase: PhaseId,
    legacy_claude_launch: bool,
) -> Result<(), CliError> {
    let _lock = match lock::acquire(project_root, phase) {
        Ok(guard) => guard,
        Err(lock::LockError::Contended { pid, path: _ }) => {
            return Err(CliError::Message(format!(
                "another devflow process (pid {pid}) is already running"
            )));
        }
        Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
    };
    let mut state = workflow::load_state(project_root, phase)?;
    if state.stopped {
        state.stopped = false;
        state.stop_reason = None;
        state.stop_until = None;
    }
    // Combined BEFORE the save below, so the persisted value exists before the
    // detached monitor this relaunch spawns ever consults it.
    if apply_legacy_launch_opt_out(&mut state, legacy_claude_launch) {
        println!(
            "note: legacy Claude launch forced by DEVFLOW_CLAUDE_LEGACY_LAUNCH \
             (D-11, 31-CONTEXT.md) — a persisted default is never a silent one"
        );
    }
    let launch_root = state
        .worktree_path
        .clone()
        .unwrap_or_else(|| project_root.to_path_buf());
    repair_leaked_auto_chain_flag(
        project_root,
        &launch_root,
        phase,
        AUTO_CHAIN_REPAIR_FROM_RESUME,
    );
    workflow::save_state(&state)?;
    launch_stage(&mut state, None, None)
}
```
D-08's pre-check must run BEFORE any of the mutations above (before `state.stopped` is even
cleared) — a refused handoff must leave the loaded state byte-identical to what `load_state`
returned, matching D-08's "fail before state mutation."

**Auth/guard pattern — the pre-check to mirror (D-08), full source** (`preflight.rs:607-634`):
```rust
fn preflight_interactivity_check(project_root: &Path, state: &State) -> Result<(), String> {
    use devflow_core::agents::InteractivityMode;
    let driver = agents::driver_for(state.agent);
    match driver.interactivity_mode(state.stage) {
        InteractivityMode::HeadlessSafe => Ok(()),
        InteractivityMode::RequiresExistingArtifact => {
            if state.mode == Mode::Auto
                && state.stage == Stage::Define
                && !phase_artifact_on_develop(project_root, state.phase, "-CONTEXT.md")
            {
                return Err(format!(
                    "phase {} has no -CONTEXT.md on develop — {} cannot run the {} \
                     stage headlessly in auto mode",
                    state.phase,
                    driver.name(),
                    state.stage,
                ));
            }
            Ok(())
        }
        other => Err(format!(
            "{} declares {} as {:?} — that stage cannot run headless",
            driver.name(),
            state.stage,
            other,
        )),
    }
}
```
**Pitfall (from RESEARCH.md, load-bearing):** this predicate does NOT refuse `Stage::Plan` for
Codex even though `CodexDriver::interactivity_mode` declares it `RequiresExistingArtifact` — only
`Stage::Define` in `Mode::Auto` without an on-develop `-CONTEXT.md` is refused. A handoff gate that
checks `interactivity_mode(target, stage) == HeadlessSafe` directly (bypassing this function)
would over-refuse a legitimate `resume --agent codex` at `Stage::Plan`. Reuse this exact function
against a hypothetical state whose `.agent` is the candidate target, not a stricter rewrite.

**Error handling pattern:** every fallible step in `resume()` uses `?` propagation through
`CliError::Message(format!(...))` — no custom error enum for this function; match this idiom for
the handoff refusal (`return Err(CliError::Message(format!("...")))`) rather than introducing a new
error type.

**Event-emission pattern to copy for the `handoff` event (D-07)** — closest full precedent,
`checkpoint_auto_decided` (`pipeline_launch.rs:1083-1094`):
```rust
events::emit(
    &state.project_root,
    state.phase,
    "checkpoint_auto_decided",
    serde_json::json!({
        "stage": state.stage.to_string(),
        "session_id": session_id,
        "instruction": truncate_reason(&instruction),
        "attempt": state.checkpoint_resumes,
        "policy": "D-03: unconditional agent auto-decide, no flag/config toggle",
    }),
);
```
Suggested `handoff` event fields (Claude's Discretion per CONTEXT.md, but must satisfy D-07's
literal requirement — "phase, from-agent, to-agent, stage, and reason/source"):
`{"stage": ..., "from_agent": ..., "to_agent": ..., "reason": "resume --agent"}` (phase is already
in the envelope every `events::emit` call stamps automatically — see `events.rs` core pattern
below).

**State-preservation checklist (D-06)** — the exact field list a handoff must leave untouched,
read directly from `pub struct State` (`crates/devflow-core/src/state.rs`):
```
stage, phase, mode, gate_pending, consecutive_failures, infra_failures,
preflight_retries, last_validate_failure_commit_count, phase_validate_failures,
last_verification_fingerprint, verification_baseline_captured,
last_verification_mtime_nanos, verification_run_nonce, started_at, project_root,
worktree_path, session_id, checkpoint_resumes, stop_until, stopped, stop_reason,
yes_ship, canary, legacy_claude_launch
```
Only `agent` (the target of the mutation) and `monitor_pid` (an existing, unrelated relaunch side
effect already reset by `spawn_agent_and_record`) may change.

---

### `crates/devflow-cli/src/pipeline_launch.rs` — resume-side cron deletion (D-15/D-16)

**Analog:** the `capture_archived` emission in the same function, immediately after its own
durable write.

**Exact insertion point — `spawn_agent_and_record`, full relevant tail** (lines 969-1030-ish,
verified this session):
```rust
fn spawn_agent_and_record(
    state: &mut State,
    program: &str,
    args: &[String],
    extra_env: &[(String, String)],
    archived_stage: Option<Stage>,
    launch: monitor::MonitorLaunch,
) -> Result<(), CliError> {
    state.monitor_pid = None;
    workflow::save_state(state)?;

    ensure_agent_binary(program)?;

    if let Some(stamp) = agent_result::archive_phase_files(/* ... */)
        .map_err(|err| CliError::Message(format!(/* ... */)))?
    {
        events::emit(
            &state.project_root,
            state.phase,
            "capture_archived",
            serde_json::json!({ /* ... */ }),
        );
    }
    let pid = monitor::spawn_monitor(state, program, args, extra_env, launch)
        .map_err(|err| CliError::Message(format!("could not spawn monitor: {err}")))?;
    state.monitor_pid = Some(pid);
    workflow::save_state(state)?;
    // <-- D-16 insertion point: delete_cron_instructions(&state.project_root, state.phase)
    //     belongs HERE, after the second `workflow::save_state` durably persists the pid.
    //     A `spawn_monitor` failure above returns via `?` before reaching this line, so
    //     a failed relaunch never deletes the record (D-15 negative control).
    let _ = devflow_core::registry::register(&state.project_root, state.phase);
    events::emit(&state.project_root, state.phase, "stage_launched", /* ... */);
    Ok(())
}
```
This function is the SHARED tail for both an ordinary launch and a handoff relaunch (`resume` →
`launch_stage` → `launch_stage_inner` → here), so #147 and #153 compose for free — no extra call
site needed for "handoff also deletes the cron record."

**Error-handling pattern for the deletion call itself:** follow `ship::delete_cron_instructions`'s
own idiom at its two EXISTING call sites in `recover.rs` (lines 117, 140) — fail-soft, never abort
the launch over a deletion failure:
```rust
// Source: crates/devflow-core/src/recover.rs:117 (existing call site, read this session)
if let Err(err) = crate::ship::delete_cron_instructions(project_root, instructions.phase) {
    // (recover.rs logs/warns here rather than propagating — match this shape)
}
```

**Deletion-audit event (D-18):** same `events::emit` shape as the handoff event above; suggested
fields `{"phase": ..., "trigger": "resume_consumed"}` (or `"ship_complete"` at the other site).

---

### `crates/devflow-cli/src/pipeline_gate.rs` — ship-side cron deletion (D-17)

**Analog:** the `workflow_shipped` emission immediately following `workflow::clear_state` and
`registry::deregister` in the same function.

**Imports** (lines 22-35, already present):
```rust
use crate::CliError;
use crate::config_parse::{foreground_gate_timeout_secs, gate_timeout_secs};
use crate::pipeline_launch::launch_stage;
use crate::pipeline_outcomes::{run_checkout_hooks, truncate_reason};
use devflow_core::gates::{self, GateAction, GateError, GateResponse, Gates};
use devflow_core::hooks;
use devflow_core::mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::prompt::FixType;
use devflow_core::stage::Stage;
use devflow_core::state::State;
use devflow_core::{events, lock, registry, workflow};
use std::path::Path;
use tracing::info;
```
`devflow_core::ship` is NOT yet imported here — add `ship::delete_cron_instructions` to the
existing `use devflow_core::{events, lock, registry, workflow};` line (or a separate `use
devflow_core::ship;` per the file's style of one `use` per logically distinct module — this file
already groups core imports on one combined line, so extending that line matches local style).

**Exact insertion point — `finish_workflow_with_gate_timeout`, verified tail** (lines 273-304):
```rust
let _ = Gates::cleanup(project_root, state.phase, Stage::Validate);
let _ = Gates::cleanup(project_root, state.phase, Stage::Ship);
workflow::clear_state(project_root, state.phase)?;
// 23b: the workflow is genuinely over — deregister this (project_root,
// phase) from the machine-global registry so `devflow gate list
// --all-roots` stops naming a phase that no longer exists.
registry::deregister(project_root, state.phase);
// <-- D-17 insertion point: delete_cron_instructions(project_root, state.phase)
//     belongs HERE — "belt-and-braces," after the workflow is durably cleared,
//     using the already-idempotent primitive (no existence check needed first).
events::emit(
    project_root,
    state.phase,
    "workflow_shipped",
    serde_json::json!({ "stage": Stage::Ship.to_string() }),
);
events::emit(
    project_root,
    state.phase,
    "workflow_finished",
    serde_json::Value::Null,
);
```
**Ordering constraint (load-bearing, per the existing code comment at line 280-291):** an existing
test asserts a phase's event stream ENDS in `workflow_finished` — place the D-17 deletion (and its
D-18 audit event, if emitted as a *separate* event) before `workflow_shipped`/`workflow_finished`,
not after, or that existing invariant breaks.

---

### `crates/devflow-cli/src/commands.rs` — `cron_hint_line` rewrite (D-10/D-11/D-14)

**Analog:** `describe_worktree_dir`, same file — closest pure-transform-with-tests sibling.

**Current implementation to rewrite** (lines 1970-1986):
```rust
fn cron_hint_line(
    instructions: &devflow_core::ship::CronInstructions,
    project_root: &Path,
) -> String {
    let base = format!(
        "Cron instruction pending (phase {}): hermes cron create --from-devflow {}",
        instructions.phase,
        project_root.display()
    );
    let retry_after = instructions.retry_after.trim();
    if retry_after.is_empty() {
        base
    } else {
        let reset = render_gate_context(retry_after, 100);
        format!("{base} (rate-limit resets: {reset})")
    }
}
```
Per Pitfall 4 in RESEARCH.md, the new hint must use the `prompt`-based `hermes cron create`
invocation (not `--script`, which requires an out-of-band `~/.hermes/scripts/` install this phase
cannot perform) built from `instructions.hermes_cron` fields already present in
`devflow_core::ship::CronInstructions` — do not hand-roll a second copy of the schedule/command
logic already computed by `build_single_agent_cron_instructions` in `ship.rs`.

**Existing tests that hardcode the literal broken string — MUST be rewritten, not just deleted**
(`commands.rs:4139-4200`, full text):
```rust
#[test]
fn cron_instruction_hints_include_hermes_command_per_phase() {
    let dir = tempfile::tempdir().unwrap();
    for phase in [PhaseId::new(7), PhaseId::new(9)] {
        let instructions =
            devflow_core::ship::build_single_agent_cron_instructions(dir.path(), phase, "");
        devflow_core::ship::write_cron_instructions(dir.path(), &instructions).unwrap();
    }
    let hints = cron_instruction_hints(dir.path());
    assert_eq!(hints.len(), 2);
    assert_eq!(
        hints[0],
        format!(
            "Cron instruction pending (phase 7): hermes cron create --from-devflow {}",
            dir.path().display()
        )
    );
    assert!(hints[1].contains("(phase 9)"));
}

#[test]
fn cron_hint_line_appends_sanitized_reset_when_retry_after_present() {
    let dir = tempfile::tempdir().unwrap();
    let instructions = devflow_core::ship::build_single_agent_cron_instructions(
        dir.path(), PhaseId::new(7), "2026-06-18T15:45:30Z",
    );
    let hint = cron_hint_line(&instructions, dir.path());
    assert!(hint.starts_with(&format!(
        "Cron instruction pending (phase 7): hermes cron create --from-devflow {}",
        dir.path().display()
    )));
    assert!(hint.contains("(rate-limit resets: 2026-06-18T15:45:30Z)"));
}

#[test]
fn cron_hint_line_omits_reset_fragment_when_retry_after_empty() {
    let dir = tempfile::tempdir().unwrap();
    let instructions =
        devflow_core::ship::build_single_agent_cron_instructions(dir.path(), PhaseId::new(7), "");
    let hint = cron_hint_line(&instructions, dir.path());
    assert_eq!(
        hint,
        format!(
            "Cron instruction pending (phase 7): hermes cron create --from-devflow {}",
            dir.path().display()
        )
    );
    assert!(!hint.contains("resets"));
}
```
D-14 requires a NEW test (in addition to rewriting these three) asserting
`!hint.contains("--from-devflow")` — this is a negative control the current suite does not have
(the current tests assert the string IS present, since it asserts the bug).

---

### `crates/devflow-core/src/ship.rs` — ISO-8601-with-offset schedule render (D-12/D-13)

**Analog:** `shell_quote`, same file — pure string-transform with dedicated unit tests, same
"never under-produce a wrong-but-plausible-looking value" safety discipline this fix needs.

**Imports** (lines 1-9, already present, no new dependency per RESEARCH.md's Package Legitimacy
Audit — do NOT add `chrono`/`chrono-tz`):
```rust
use crate::phase_id::PhaseId;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
```

**Struct to extend** — `HermesCronJob.schedule`'s doc comment currently says `M H D M W` (line 40)
and must be updated to describe the ISO-8601-with-offset form:
```rust
pub struct HermesCronJob {
    /// Cron schedule in `M H D M W` format.
    pub schedule: String,
    ...
```

**Core pattern to add — a `to_iso_utc()` sibling to the existing `to_cron()`** (existing method,
lines 218-223, full struct context lines 200-243):
```rust
impl RetryTimestamp {
    fn round_up_minute(self) -> Self { /* ... unchanged ... */ }

    fn to_cron(self) -> String {
        format!(
            "{} {} {} {} *",
            self.minute, self.hour, self.day, self.month
        )
    }
    // NEW sibling method, same struct, same visibility:
    // fn to_iso_utc(self) -> String {
    //     format!("{:04}-{:02}-{:02}T{:02}:{:02}:00Z",
    //         self.year, self.month, self.day, self.hour, self.minute)
    // }

    fn to_epoch_minutes(self) -> i64 { /* ... unchanged, needed by round_up_minute ... */ }
    fn from_epoch_minutes(minutes: i64) -> Self { /* ... unchanged ... */ }
}
```

**Fail-closed pattern to preserve verbatim (D-13)** — `cron_schedule_from_retry_after`, full
current body (lines 195-198):
```rust
/// Convert a retry timestamp to `M H D M W` cron syntax, rounding up to the
/// nearest minute. Supports RFC3339-like timestamps and Unix epoch seconds.
pub fn cron_schedule_from_retry_after(retry_after: &str) -> Option<String> {
    // WR-06: never turn unparseable agent output into an every-minute cron.
    parse_retry_timestamp(retry_after).map(|ts| ts.round_up_minute().to_cron())
}
```
The `.map(|ts| ts.round_up_minute().to_cron())` tail is the ONLY line that needs `.to_cron()`
swapped for the new `.to_iso_utc()` — `parse_retry_timestamp` returning `None` on unparseable input
(the fail-closed contract) is untouched, satisfying D-13 without new code.

**Build-site to update** — `build_single_agent_cron_instructions`, full current body (lines
161-191), specifically the `hermes_cron:` block:
```rust
hermes_cron: HermesCronJob {
    schedule: cron_schedule_from_retry_after(retry_after).unwrap_or_default(),
    name: format!("devflow-phase-{padded}-resume", padded = phase.padded()),
    command: format!(
        "cd {} && devflow resume --phase {phase}",
        shell_quote(&project)
    ),
    once: true,
},
```
The `command` field's `shell_quote(&project)` usage is the existing injection-safety precedent
(RESEARCH.md's STRIDE table) — any new command construction for the `hermes cron create` CLI
invocation itself (built in `commands.rs`'s `cron_hint_line`, not here) must route the project path
through this same `shell_quote` helper, not string-interpolate it raw.

**Testing pattern — existing round-trip and idempotency tests to extend, not replace**
(lines 429-459, full text):
```rust
#[test]
fn cron_instructions_save_load_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let record = build_single_agent_cron_instructions(
        dir.path(), PhaseId::new(7), "2026-06-18T15:45:30Z",
    );
    write_cron_instructions(dir.path(), &record).unwrap();
    assert_eq!(
        load_cron_instructions(dir.path(), PhaseId::new(7)).unwrap(),
        record
    );
}

#[test]
fn delete_cron_instructions_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let record = build_single_agent_cron_instructions(
        dir.path(), PhaseId::new(7), "2026-06-18T15:45:30Z",
    );
    write_cron_instructions(dir.path(), &record).unwrap();
    delete_cron_instructions(dir.path(), PhaseId::new(7)).unwrap();
    assert!(!cron_instructions_path(dir.path(), PhaseId::new(7)).exists());
    delete_cron_instructions(dir.path(), PhaseId::new(7)).unwrap();
}
```
D-14's negative control ("the old UTC-field schedule would fire at the wrong local instant") should
be a NEW test alongside these, not a rewrite of them — these two are orthogonal to the
schedule-format change (round-trip/idempotency, not schedule content) and should stay green
untouched.

---

## Shared Patterns

### Audit-event emission (applies to #147's handoff event AND #153's two deletion events)
**Source:** `crates/devflow-core/src/events.rs:36-56` (`pub fn emit`), full function:
```rust
pub fn emit(project_root: &Path, phase: PhaseId, event: &str, fields: serde_json::Value) {
    let mut line = serde_json::json!({
        "v": SCHEMA_VERSION,
        "ts": unix_now(),
        "phase": phase,
        "event": event,
    });
    match fields {
        serde_json::Value::Object(map) => {
            let base = line.as_object_mut().expect("line is an object");
            for (key, value) in map {
                // Envelope keys win — a payload must not be able to forge
                // another phase's identity or a different event kind.
                base.entry(key).or_insert(value);
            }
        }
        serde_json::Value::Null => {}
        other => { line["data"] = other; }
    }
    // ... append to .devflow/events.jsonl, fail-soft on write error ...
}
```
**Apply to:** every `events::emit(...)` call this phase adds (`handoff`, cron-deletion x2). Payload
keys `phase`/`event`/`v`/`ts` are stamped automatically by the envelope — do not re-supply `phase`
inside the `fields` object (it would be silently ignored by `entry().or_insert()`, matching the
comment's own "envelope keys win" rule).

### Fail-soft, never-abort-the-workflow deletion calls
**Source:** `crates/devflow-core/src/recover.rs:117, 140` (existing `delete_cron_instructions`
call sites).
**Apply to:** both new call sites in `pipeline_launch.rs` and `pipeline_gate.rs` — a cron-deletion
failure must never fail an otherwise-successful relaunch or ship completion. Match the existing
`if let Err(err) = ... { /* warn, do not propagate */ }` shape rather than `?`.

### Positional-parameter function signatures (this codebase's convention)
**Source:** `resume(project_root: &Path, phase: PhaseId, legacy_claude_launch: bool)`,
`spawn_agent_and_record(state, program, args, extra_env, archived_stage, launch)`.
**Apply to:** the extended `resume()` signature for the new `agent: Option<AgentKind>` parameter —
no options-struct precedent exists in this file for functions under ~6 parameters; stay positional.

### `--help` snapshot regeneration (Pitfall 2, RESEARCH.md)
**Source:** `crates/devflow-cli/tests/help_snapshot.rs:27-43`, doc comment instruction embedded in
the test's own failure message: `cargo run -q -p devflow -- --help >
crates/devflow-cli/tests/snapshots/devflow-help.txt`.
**Apply to:** required after `main.rs`'s `Command::Resume` gains `--agent` — this is a MANDATORY
follow-up step, not optional, or `help_output_matches_committed_snapshot` fails.

## Dogfood Evidence Capture (CODE-01, no source-code analog — process precedent only)

**Analog:** `.planning/phases/43-opencode-driver-completion/43-evidence/` — the most recent
same-shaped dogfood evidence directory (opencode driver verification, Phase 43), containing raw
JSONL captures per outcome class:
```
43-evidence/
├── opencode_error.jsonl
├── opencode_success.jsonl
└── opencode_tool_use.jsonl
```
**Apply to:** CODE-01's evidence directory (suggested `44-evidence/`) should follow the same
shape — raw captured agent output per outcome class the run actually produced (completion vs.
re-filed-gap, per D-03), not a prose summary alone. RESEARCH.md's Wave 0 Gaps item names this
explicitly: "commands run, `--json` capture, PR link, failure classification if any gap surfaces."

## No Analog Found

None. Every file this phase modifies already contains an adequate same-file precedent (see table
above) — this is a hardening/wiring phase over existing machinery (RESEARCH.md's own framing:
"every piece of machinery #147/#148/#153 need already exists... The phase's work is wiring, not
new subsystems"), not new-module or new-pattern work.

## Metadata

**Analog search scope:** `crates/devflow-cli/src/{main,pipeline_launch,pipeline_gate,commands,
preflight}.rs`, `crates/devflow-core/src/{ship,state,events,recover,agents/mod,agents/codex}.rs`,
`crates/devflow-cli/tests/help_snapshot.rs`, `.planning/phases/43-opencode-driver-completion/
43-evidence/`.
**Files scanned:** 10 source files + 1 test file + 1 evidence directory, all read directly this
session (no analog claim in this document is inferred from RESEARCH.md alone without independent
confirmation).
**Pattern extraction date:** 2026-08-26
