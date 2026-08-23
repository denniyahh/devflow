# Phase 28: Close the Checkpoint Answer Return Path - Pattern Map

**Mapped:** 2026-07-30
**Files analyzed:** 8 (all modified, no new files — see RESEARCH.md "Recommended Project Structure")
**Analogs found:** 8 / 8 (all analogs live in the same file being modified, or one file over — this phase is entirely additive extension of existing sibling patterns, confirmed by RESEARCH.md's Don't-Hand-Roll table)

This is a Rust workspace (`devflow-core` + `devflow-cli`), not a web project.
Every "analog" below is a sibling function/field/test in the SAME crate,
often the SAME file, that the planner should copy verbatim in shape. All
line numbers below were re-read live this session (2026-07-30) and matched
RESEARCH.md's citations exactly — no drift.

## File Classification

| Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/devflow-core/src/verify.rs` | utility (pure fn) | file-I/O (directory scan) | `external_verify_commands` (same file, lines 34-67) | exact — same file, same discovery loop, different predicate |
| `crates/devflow-core/src/state.rs` | model (struct field) | CRUD (serde persistence) | `monitor_pid` field (same file, lines 66-72) + its 2 tests | exact — same file, same `#[serde(default)] Option<T>` shape |
| `crates/devflow-core/src/agent_result.rs` | model/parser | transform (JSON envelope → struct) | `AgentResult` struct itself (same file, lines 18-42), `verdict`/`decided_by_layer` optional fields | exact — same struct, add one more optional field |
| `crates/devflow-core/src/agents/claude.rs` | service (subprocess command builder) | request-response (argv construction) | `ClaudeAgent::exec_command` (same file, lines 15-31) — the WHOLE file is 37 lines | exact — same file, new sibling method on the same impl |
| `crates/devflow-core/src/prompt.rs` | utility (prompt string builder) | transform | `idempotent_stage_prompt` (same file, lines 142-166) — self-analog, deletion not addition | exact — editing the function's own Define branch |
| `crates/devflow-core/src/config.rs` | config resolver | request-response (env→file→default) | `external_verify_enabled` + `DevflowConfig.external_verify_enabled` (same file, lines 52-59, 66, 151-163) | exact — same file, same 3-tier resolver shape |
| `crates/devflow-cli/src/pipeline_launch.rs` | orchestration (CLI dispatch) | event-driven (stage advance) | `resume()` (same file, lines 215-231) — self-analog, guard the existing clear | exact — editing the function's own body |
| `crates/devflow-cli/src/pipeline_outcomes.rs` | orchestration (gate/outcome handler) | event-driven | `handle_ship_outcome` + its `yes_ship` wiring, `events::emit` calls in `pipeline_gate.rs` (lines 315-329) | exact — same crate, sibling event kind |

## Pattern Assignments

### `crates/devflow-core/src/verify.rs` — D-01 static PLAN.md scan

**Analog:** `external_verify_commands` (same file, lines 34-67)

**Full function to mirror the discovery loop from** (verified live, unchanged from RESEARCH.md citation):
```rust
// Source: crates/devflow-core/src/verify.rs:34-67
pub fn external_verify_commands(project_root: &Path, phase: u32) -> Vec<String> {
    let phases_dir = project_root.join(".planning/phases");
    let phase_prefix = format!("{phase:02}-");
    let plan_prefix = format!("{phase:02}-");
    let mut plans = Vec::<PathBuf>::new();

    let Ok(phase_entries) = std::fs::read_dir(phases_dir) else {
        return Vec::new();
    };
    for phase_entry in phase_entries.flatten() {
        if !phase_entry
            .file_name()
            .to_string_lossy()
            .starts_with(&phase_prefix)
        {
            continue;
        }
        let Ok(plan_entries) = std::fs::read_dir(phase_entry.path()) else {
            continue;
        };
        plans.extend(plan_entries.flatten().filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            (name.starts_with(&plan_prefix) && name.ends_with("-PLAN.md")).then(|| entry.path())
        }));
    }
    plans.sort();

    plans
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .filter_map(|contents| command_from_frontmatter(&contents))
        .collect()
}
```

**New sibling to add:** `pub fn phase_has_blocking_human_checkpoint(project_root: &Path, phase: u32) -> bool` — identical directory-walk (lines 35-60 above, verbatim or factored into a shared helper), replacing the final `.filter_map(command_from_frontmatter)` stage with `.any(|contents| contents.contains(r#"gate="blocking-human""#))`. Module doc comment style to match (lines 1-5): explain the agent-writable trust boundary this reuses, per RESEARCH.md's Security Domain T-28-01 analysis.

---

### `crates/devflow-core/src/state.rs` — D-04 `session_id` field

**Analog:** `monitor_pid` field, same file, lines 66-72, plus its round-trip/absence test pair (lines 328-358).

**Field pattern to copy exactly:**
```rust
// Source: crates/devflow-core/src/state.rs:66-72
/// PID of the detached monitor process that owns the agent for the
/// current stage, recorded by `launch_stage` at spawn time. `None` means
/// no monitor has been spawned for this state yet, OR the state was
/// written by a binary predating this field — in both cases the
/// liveness probe reports Unknown, never Stuck.
#[serde(default)]
pub monitor_pid: Option<u32>,
```
Add `session_id: Option<String>` immediately adjacent (state.rs:73-101 is the block of `stop_until`/`stopped`/`stop_reason`/`yes_ship`, all following this exact `#[serde(default)]` shape — insert `session_id` as one more sibling in this block), with doc comment explaining both "never captured yet" and "written by a pre-28 binary" — the two-case explanation `monitor_pid`'s own comment already models.

Also update `State::new`'s field-literal list (state.rs:145-164, the `worktree_path: None, monitor_pid: None, stop_until: None, ...` block) to add `session_id: None,`.

**Test pattern to mirror** (two tests, `monitor_pid_round_trips_through_serde` and `monitor_pid_absent_from_json_defaults_to_none`, lines 326-358):
```rust
// Source: crates/devflow-core/src/state.rs:326-358 (structure to mirror)
#[test]
fn monitor_pid_round_trips_through_serde() {
    let mut state = /* ... */;
    state.monitor_pid = Some(4242);
    let json = /* serialize */;
    assert!(json.contains("monitor_pid"), "monitor_pid must appear in persisted JSON");
    let loaded = /* deserialize */;
    assert_eq!(loaded.monitor_pid, Some(4242), "monitor_pid must round-trip through serde");
}

/// A serde-absent `monitor_pid` (state written by a pre-18b binary) must ...
#[test]
fn monitor_pid_absent_from_json_defaults_to_none() {
    // write JSON without the field, deserialize, assert None
}
```
New tests: `session_id_round_trips_through_serde`, `session_id_absent_from_json_defaults_to_none` — RESEARCH.md names this exact pattern ("four existing `_absent_from_json_default*` sibling tests"); this becomes the fifth.

---

### `crates/devflow-core/src/agent_result.rs` — D-04 `session_id` capture

**Analog:** `AgentResult` struct itself, same file, lines 18-42 (add a field, don't restructure).

```rust
// Source: crates/devflow-core/src/agent_result.rs:18-42
pub struct AgentResult {
    pub status: AgentStatus,
    pub exit_code: Option<i32>,
    pub reason: Option<String>,
    pub commits: Option<u32>,
    pub summary: Option<String>,
    #[serde(default, deserialize_with = "deserialize_verdict_lenient")]
    pub verdict: Option<Verdict>,
    #[serde(default)]
    pub decided_by_layer: Option<u8>,
}
```
Add `#[serde(default)] pub session_id: Option<String>,` following the `decided_by_layer` pattern exactly (plain `#[serde(default)]`, no custom deserializer needed — unlike `verdict`, this is a plain string, no lenient-parse concern).

**Fixture line to extend, not move** (ground-truth envelope shape, RESEARCH.md-cited, re-confirm exact current line number before editing since this file has ~1887 total lines across the crate and >1300 in this one — grep `session_id` in agent_result.rs test fixtures before editing, do not assume line 1362 is unchanged from a prior read):
```
r#"{"type":"result","is_error":true,"num_turns":3,"result":"oops\nDEVFLOW_RESULT: {\"status\":\"success\"}","session_id":"abc"}"#
```
Extraction point: wherever the existing envelope-parsing helper (`extract_json_result_text` or the function that builds `AgentResult` from the parsed envelope) already reads `is_error`/`result`/`session_id`-adjacent keys — thread `session_id` through at the same call site, not a new parse pass.

---

### `crates/devflow-core/src/agents/claude.rs` — D-04/D-05 `--resume` relaunch

**Analog:** `ClaudeAgent::exec_command`, the entire 37-line file (verified live, unchanged from RESEARCH.md):
```rust
// Source: crates/devflow-core/src/agents/claude.rs (full file, 37 lines)
use super::AgentAdapter;

pub struct ClaudeAgent;

impl AgentAdapter for ClaudeAgent {
    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn exec_command(
        &self,
        _phase: u32,
        prompt: &str,
        _extra_writable_roots: &[std::path::PathBuf],
    ) -> (&'static str, Vec<String>) {
        (
            "claude",
            vec![
                "-p".into(),
                prompt.to_string(),
                "--output-format".into(),
                "json".into(),
                "--dangerously-skip-permissions".into(),
            ],
        )
    }

    fn completion_signal_detected(&self, _output: &str) -> bool {
        // Claude exits cleanly when done; monitor detects exit via kill -0.
        false
    }
}
```
**Confirmed:** `--dangerously-skip-permissions` and `--output-format json` are hardcoded flags on the ordinary launch path — Pitfall 1 (RESEARCH.md) requires both be re-passed on the new `--resume` path.

**New method to add** (not on `AgentAdapter` trait per D-05 — an inherent method on `ClaudeAgent`):
```rust
// New — inherent method, NOT on the AgentAdapter trait (D-05: Claude-only)
impl ClaudeAgent {
    pub fn exec_resume_command(session_id: &str, instruction: &str) -> (&'static str, Vec<String>) {
        (
            "claude",
            vec![
                "-p".into(),
                instruction.to_string(),
                "--resume".into(),
                session_id.to_string(),
                "--output-format".into(),
                "json".into(),
                // Pitfall 1: NOT restored by --resume — must be re-passed.
                "--dangerously-skip-permissions".into(),
            ],
        )
    }
}
```
Regression test to add (RESEARCH.md-named): `resume_command_includes_permission_bypass` — assert the returned `Vec<String>` contains both `"--dangerously-skip-permissions"` and `"--output-format"`.

---

### `crates/devflow-core/src/prompt.rs` — D-14 Define headless-safety fix

**Analog:** the function's own current body, `idempotent_stage_prompt`, lines 142-166 (verified live):
```rust
// Source: crates/devflow-core/src/prompt.rs:142-166 (current — to be changed)
fn idempotent_stage_prompt(stage: Stage, phase: u32) -> String {
    let artifact = match stage {
        Stage::Define => "CONTEXT.md",
        _ => "PLAN.md",
    };
    let command = gsd_command_for(stage, phase);
    let padded = format!("{phase:02}");
    format!(
        "First check whether this stage's deliverable already exists:\n\
        \n\
        ls .planning/phases/{padded}-*/{padded}-*{artifact} 2>/dev/null\n\
        \n\
        - If it EXISTS: the stage's work is already done. Do NOT run the GSD \
        command, do NOT ask for input, and do NOT modify the existing \
        artifacts. Your FINAL message must be exactly:\n\
        \n\
        DEVFLOW_RESULT: {{\"status\": \"success\"}}\n\
        \n\
        - If it does NOT exist: run the GSD workflow command for this stage:\n\
        \n\
        \x20   {command}\n\
        \n\
        {COMPLETION_PROTOCOL}"
    )
}
```
This single function is shared by BOTH `Stage::Define` and `Stage::Plan` (dispatch site: `stage_prompt_with_project`, line 197, `if matches!(stage, Stage::Define | Stage::Plan) { ... idempotent_stage_prompt ... }`). D-14 requires branching this function's "does NOT exist" arm on `stage`: Plan keeps invoking `gsd_command_for` (`/gsd-plan-phase {N}`, non-interactive); Define must proceed without CONTEXT.md instead of invoking `/gsd-discuss-phase {N}` (interactive, hangs headlessly). Do not touch `Stage::Plan`'s branch or the shared "EXISTS" arm — Pitfall 4 names this exact hazard.

**Existing test that must split** (verified live, lines 355-388 — confirms RESEARCH.md's cited range `365-388` was the inner `#[test]`, the doc comment starts at 361):
```rust
// Source: crates/devflow-core/src/prompt.rs:361-388 (current — must split into two tests)
/// 13-06 dogfood regression (Codex leg): GSD's discuss-phase demands an
/// interactive decision when CONTEXT.md already exists, which headless
/// Codex can never answer — Define/Plan must no-op with success when
/// their deliverable pre-exists.
#[test]
fn define_and_plan_prompts_are_idempotent() {
    let cases = [
        (Stage::Define, "/gsd-discuss-phase 9", "09-*CONTEXT.md"),
        (Stage::Plan, "/gsd-plan-phase 9", "09-*PLAN.md"),
    ];
    for (stage, command, artifact_glob) in cases {
        let prompt = stage_prompt(stage, 9);
        assert!(prompt.contains(command), "{stage} prompt missing {command}");
        assert!(
            prompt.contains(artifact_glob),
            "{stage} prompt must check for its pre-existing artifact"
        );
        assert!(
            prompt.contains("Do NOT run the GSD command"),
            "{stage} prompt must no-op when the artifact exists"
        );
        assert!(
            prompt.contains("do NOT ask for input"),
            "{stage} prompt must forbid interactive input"
        );
        assert!(prompt.contains("DEVFLOW_RESULT"));
    }
}
```
Split: keep this test (minus the `Stage::Define` case, or keep the loop shape but assert Define's NEW no-op-without-artifact wording) as `plan_prompt_is_idempotent`, and add a new `define_prompt_never_invokes_discuss_phase_when_context_missing` asserting `stage_prompt(Stage::Define, 9)` does NOT contain `/gsd-discuss-phase` and does not instruct interactive input, while still producing a valid `DEVFLOW_RESULT`-terminated prompt.

---

### `crates/devflow-core/src/config.rs` — D-12 `yes_ship` config option

**Analog:** `external_verify_enabled`, same file — struct field (line 58), default (line 66), accessor (lines 83-85), resolver (lines 151-163), and its env-override test (lines 244-256). All verified live, unchanged from RESEARCH.md.

```rust
// Source: crates/devflow-core/src/config.rs:50-69 (struct + Default — model for the new field)
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct DevflowConfig {
    pub capture_retention: usize,
    pub review_angles: Option<Vec<String>>,
    pub external_verify_enabled: bool,
}

impl Default for DevflowConfig {
    fn default() -> Self {
        Self {
            capture_retention: DEFAULT_CAPTURE_RETENTION,
            review_angles: None,
            external_verify_enabled: true,
        }
    }
}
```
```rust
// Source: crates/devflow-core/src/config.rs:149-163 (resolver — model for `yes_ship()`)
pub fn external_verify_enabled(project_root: &Path) -> bool {
    if let Some(value) = env_value("DEVFLOW_EXTERNAL_VERIFY_ENABLED") {
        match value.parse() {
            Ok(enabled) => return enabled,
            Err(error) => tracing::warn!(
                value,
                %error,
                "invalid DEVFLOW_EXTERNAL_VERIFY_ENABLED; using devflow.toml or default"
            ),
        }
    }
    load_config(project_root).external_verify_enabled
}
```
Add `pub yes_ship: bool` to the struct (default `false`, NOT `true` like `external_verify_enabled` — matches D-12's stated default-off intent), and `pub fn yes_ship(project_root: &Path) -> bool` mirroring the resolver exactly with `DEVFLOW_YES_SHIP` as the env override key.

**Test to mirror** (lines 244-256, `env_overrides_file_external_verification` — uses the `EnvOverride` RAII guard + `ENV_MUTEX` pattern already established in this file's test module, lines 172-192):
```rust
// Source: crates/devflow-core/src/config.rs:244-256
#[test]
fn env_overrides_file_external_verification() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("devflow.toml"),
        "external_verify_enabled = false\n",
    )
    .unwrap();
    let _env = EnvOverride::set("DEVFLOW_EXTERNAL_VERIFY_ENABLED", "true");

    assert!(external_verify_enabled(dir.path()));
}
```

**Call site to update** (RESEARCH.md-cited, `commands.rs:129` area — combine CLI flag with config via OR, since the bare `--yes-ship` flag has no `--no-yes-ship` counterpart):
```rust
state.yes_ship = yes_ship /* CLI flag */ || devflow_core::config::yes_ship(project_root);
```
**Pitfall 3 (RESEARCH.md):** `commands.rs:125-128`'s existing comment ("the only assignment in the crate that ever sets `yes_ship` to a non-default value") becomes stale the moment this lands — update it in the same edit, per CLAUDE.md's Surgical Changes rule (the comment directly describes the line being changed).

**Test that must flip** (`pipeline_outcomes.rs:1631-1648`, verified live — exact current text):
```rust
// Source: crates/devflow-cli/src/pipeline_outcomes.rs:1630-1648 (current — assertion inverts after D-12)
#[test]
fn config_file_with_yes_ship_key_loads_but_never_sets_the_flag() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("devflow.toml"), "yes_ship = true\n").unwrap();

    let _config = devflow_core::config::load_config(root);

    let state = State::new(1, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    assert!(
        !state.yes_ship,
        "no devflow.toml key may ever set the Ship pre-authorization (D-05)"
    );
}
```
Post-D-12 this test's premise is reversed (a `devflow.toml` `yes_ship = true` key DOES now set the flag, via `commands::start`'s new OR-combine — but note this specific test constructs `State::new` directly, not through `commands::start`, so `State::new` itself is unaffected; the assertion to add/flip belongs in a NEW test exercising the `commands::start`-level OR-combine, not this one). Rename this test to reflect what it actually still proves (`State::new` alone never reads config — true before and after D-12) and add a new sibling test asserting `commands::start`'s combined resolution picks up the config value when the CLI flag is absent.

---

### `crates/devflow-cli/src/pipeline_launch.rs` — D-15 guard the `resume()` clear

**Analog:** the function's own current body, `resume()`, lines 215-231 (verified live, unchanged from CONTEXT.md's citation):
```rust
// Source: crates/devflow-cli/src/pipeline_launch.rs:215-231 (current — to be changed)
pub(crate) fn resume(project_root: &Path, phase: u32) -> Result<(), CliError> {
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
    state.stopped = false;
    state.stop_reason = None;
    state.stop_until = None;
    workflow::save_state(&state)?;
    launch_stage(&mut state, None, None)
}
```
Fix (D-15): gate the three-line clear behind `if state.stopped { ... }` — an unfired `--until` cap has `state.stopped == false` and `state.stop_until == Some(target)`; today's unconditional clear wipes that cap even though it never fired. After the fix, only a state that IS `stopped` gets its stop markers cleared.

**Existing test to mirror the shape of, for the OPPOSITE case** (`resume_clears_stop_marker_and_advances_past_stop_point`, lines 456-526, verified live):
```rust
// Source: crates/devflow-cli/src/pipeline_launch.rs:456-526 (structure to mirror — the "stopped" case)
#[test]
fn resume_clears_stop_marker_and_advances_past_stop_point() {
    let _guard = ENV_MUTEX.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);

    let phase = 66;
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Plan;
    state.stop_until = Some(Stage::Plan);
    state.stopped = true;
    state.stop_reason = Some("stopped after plan completed (--until plan)".to_string());
    workflow::save_state(&state).unwrap();

    let stub_dir = stub_agent_binary("claude");
    // ... PATH stubbing via prepend_path, ENV_MUTEX-serialized ...

    let result = resume(root, phase);
    // ... PATH restore ...

    let reloaded_for_reap = workflow::load_state(root, phase).ok();
    let _reap_guard = reloaded_for_reap.as_ref().map(ReapMonitorOnDrop::after_launch);
    result.unwrap();

    let reloaded = workflow::load_state(root, phase).unwrap();
    assert!(!reloaded.stopped, "resume must clear stopped ...");
    assert_eq!(reloaded.stop_reason, None, "resume must clear stop_reason ...");
    assert_eq!(reloaded.stop_until, None, "resume must clear stop_until ...");
    assert!(reloaded.monitor_pid.is_some(), "resume() must have spawned a monitor ...");
}
```
New sibling test (proves the opposite/D-15 case): build a `State` with `stopped: false` and `stop_until: Some(Stage::Plan)` (a cap set via `--until` that never fired — e.g. the phase is still mid-flight at an earlier stage), call `resume()`, and assert `reloaded.stop_until` is UNCHANGED (`Some(Stage::Plan)`), not silently cleared — the exact opposite assertion from the existing test's `stop_until, None` line. Reuse the same `stub_agent_binary`/`ENV_MUTEX`/`ReapMonitorOnDrop` scaffolding verbatim (all already in this test module).

---

### `crates/devflow-cli/src/pipeline_outcomes.rs` — D-01 dispatch insertion + D-07 audit event

**Analog for the `events::emit` call shape:** `pipeline_gate.rs`, lines 315-329 (verified live — NOT in `pipeline_outcomes.rs` itself, but same crate, called from the gate-write path this new logic sits alongside):
```rust
// Source: crates/devflow-cli/src/pipeline_gate.rs:315-329
events::emit(
    project_root,
    state.phase,
    "gate_fired",
    serde_json::json!({
        "stage": stage.to_string(),
        "unexpected": unexpected,
        "context": context,
    }),
);
gates::fire_gate_notify(state.phase, stage, context, unexpected);
events::emit(
    project_root,
    state.phase,
    "notify_fired",
    serde_json::json!({ "stage": stage.to_string(), "unexpected": unexpected }),
);
```
`events::emit`'s signature (`events.rs:35`): `pub fn emit(project_root: &Path, phase: u32, event: &str, fields: serde_json::Value)`. D-07's new call: `events::emit(project_root, state.phase, "checkpoint_auto_decided", serde_json::json!({ "stage": stage.to_string(), "session_id": session_id, "synthesized_instruction": instruction, "response_excerpt": truncate_reason(&response) }))` — same three-positional-arg-plus-json shape, no new helper needed.

**Analog for the auto-response wiring pattern + its acceptance test:** `handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution`, `pipeline_outcomes.rs:1656-1713` (verified live) — the shape D-07's own audit-record test should mirror (arrange a `State`, act via the handler, assert exactly one `gate_fired`-class event plus a resolved/attributed event, never a silent path):
```rust
// Source: crates/devflow-cli/src/pipeline_outcomes.rs:1656-1713 (structure to mirror)
#[test]
fn handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    // ... branch setup ...
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Ship;
    state.yes_ship = true;
    workflow::save_state(&state).unwrap();

    handle_ship_outcome(root, &mut state).unwrap();

    // assert workflow completed unattended, no gate files left open
    // assert exactly one gate_fired event (counted from events.jsonl via serde_json::Value filtering)
    let contents = std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
    let gate_fired_count = contents
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["event"] == "gate_fired" && event["phase"] == phase && event["stage"] == "ship")
        .count();
    assert_eq!(gate_fired_count, 1, "the Ship gate must be written exactly once, not reopened");

    let resolved = devflow_core::events::last_event_of_kind_for_phase(root, phase, "gate_resolved")
        .expect("a gate_resolved event must be recorded");
    assert_eq!(resolved["responded_by"], "--yes-ship", /* ... */);
}
```
D-07's new test should follow this exact "assert exactly N events of a kind, by reading `events.jsonl` back with `serde_json::Value` line filtering" shape, asserting exactly one `checkpoint_auto_decided` event with `stage`/`session_id`/attribution fields, never silent (mirrors this test's own "never reopened" / "never silent" framing).

**Dispatch insertion point:** wherever `Action::GateReview` is handled today in `pipeline_launch::advance` (RESEARCH.md's Architecture Patterns diagram names this precisely) — insert `verify::phase_has_blocking_human_checkpoint(project_root, phase)` as a guard BEFORE today's unconditional fall-through to `handle_stage_failure`/`handle_ship_failure`. `truncate_reason`/`render_gate_context` (`pipeline_outcomes.rs:318-344`, verified live at lines 318-344 — confirmed the 300-char cap is applied at construction time, not at render time) is the existing helper to reuse for capping the synthesized-instruction and response-excerpt strings before they reach `events::emit`.

## Shared Patterns

### `#[serde(default)] Option<T>` backward-compat field
**Source:** `crates/devflow-core/src/state.rs:64-101` (five fields: `worktree_path`, `monitor_pid`, `stop_until`, `stopped`, `stop_reason`, `yes_ship` — `session_id` becomes the sixth)
**Apply to:** `State.session_id` (D-04)
```rust
#[serde(default)]
pub monitor_pid: Option<u32>,
```

### Env-var → `devflow.toml` → built-in-default config resolver
**Source:** `crates/devflow-core/src/config.rs:149-163` (`external_verify_enabled`)
**Apply to:** `config::yes_ship(project_root)` (D-12)
```rust
pub fn external_verify_enabled(project_root: &Path) -> bool {
    if let Some(value) = env_value("DEVFLOW_EXTERNAL_VERIFY_ENABLED") {
        match value.parse() {
            Ok(enabled) => return enabled,
            Err(error) => tracing::warn!(value, %error, "invalid ...; using devflow.toml or default"),
        }
    }
    load_config(project_root).external_verify_enabled
}
```

### Append-only `events::emit()` audit trail
**Source:** `crates/devflow-core/src/events.rs:35` (`pub fn emit(project_root: &Path, phase: u32, event: &str, fields: serde_json::Value)`), called from `crates/devflow-cli/src/pipeline_gate.rs:315-329`
**Apply to:** D-07's `checkpoint_auto_decided` event — no new file, no new subsystem, same `.devflow/events.jsonl` every other gate event already writes to.

### `ENV_MUTEX`-serialized env-var test isolation
**Source:** `crates/devflow-core/src/config.rs:172-192` (`EnvOverride` RAII guard + `static ENV_MUTEX: Mutex<()>`)
**Apply to:** any new test that sets `DEVFLOW_YES_SHIP` or reads/writes process env — reuse this exact guard type, do not hand-roll a new one (this pattern already exists in ≥3 files per RESEARCH.md's Wave 0 Gaps note).

### 300-char construction-time truncation before any gate/notify/event write
**Source:** `crates/devflow-cli/src/pipeline_outcomes.rs:318-323` (`truncate_reason` / `render_gate_context`)
**Apply to:** D-07's synthesized-instruction and response-excerpt fields before they reach `events::emit`.

## No Analog Found

None — every file in this phase's scope is an additive extension of an existing sibling pattern within the same crate, several within the exact same file. RESEARCH.md's own "Don't Hand-Roll" table (three rows) independently confirms this: PLAN.md discovery, audit-record storage, and config-precedence resolution all already have a load-bearing precedent to copy rather than invent.

## Metadata

**Analog search scope:** `crates/devflow-core/src/{verify,state,agent_result,agents/claude,prompt,config}.rs`, `crates/devflow-cli/src/{pipeline_launch,pipeline_outcomes,pipeline_gate,commands}.rs`, `crates/devflow-core/src/{events,stage}.rs`
**Files scanned:** 13 (all cited in RESEARCH.md's canonical refs and Sources section; all re-read live this session, zero line-number drift found)
**Pattern extraction date:** 2026-07-30

---

*Phase: 28-close-the-checkpoint-answer-return-path*
*Patterns mapped: 2026-07-30*
