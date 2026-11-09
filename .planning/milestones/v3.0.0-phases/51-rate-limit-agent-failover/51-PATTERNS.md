# Phase 51: Rate-Limit Agent Failover - Pattern Map

**Mapped:** 2026-11-05
**Files analyzed:** 16 (13 code/test files modified, 3 operator docs, 1 allowlist)
**Analogs found:** 15 / 16 (the 999.71 measurement deliverable has no in-repo analog)

All line numbers below were re-read in this worktree at `ce08a9d` (feature/phase-51) and every
analog path was checked with `git ls-files --error-unmatch` (all tracked; a nonexistent control
path was correctly reported untracked). `pipeline_launch.rs` is 14,608 lines and fast-moving:
re-check a cited line range before quoting it in a PLAN.md.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/devflow-core/src/state.rs` | model (persisted run state) | file-I/O (serde) | `State::canary` / `State::legacy_claude_launch` fields + `infra_failures_*` serde tests, same file | exact |
| `crates/devflow-core/src/prompt.rs` | utility (prompt renderer) | transform | `idempotent_stage_prompt` / `workflow_plan_prompt`, same file | exact |
| `crates/devflow-core/src/agent_result.rs` | service (classifier) | transform (capture → verdict) | `ParsedCapture::torn_json_after_last_matching`, `parse_idle_timeout_side_channel`, same file | exact |
| `crates/devflow-core/src/monitor.rs` | service (supervisor) | file-I/O + event-driven | `write_idle_timeout_record`, idle-timeout rate-limit suppression, `monitor_test_hold`, same file | exact |
| `crates/devflow-core/src/doc_check.rs` | test (source scan) | batch (scan all `.rs`) | `no_monitor_pid_field_json_key_or_help_mention_remains`, same file | exact |
| `crates/devflow-cli/src/main.rs` | config (clap surface) | request-response | `legacy_claude_launch` arg on `Start`/`Resume`, same file | exact |
| `crates/devflow-cli/src/commands.rs` (`start`) | controller (CLI verb) | request-response | `yes_ship` / legacy-launch persistence block in `start`, same file | exact |
| `crates/devflow-cli/src/pipeline_launch.rs` (handoff fn, failover branch, resume flag) | controller (in-supervisor advance) | event-driven | `resume_loaded` substitution + `handle_ambiguous_outcome` re-drive, same file | exact |
| `crates/devflow-cli/src/pipeline_launch.rs` (tests: e2e both arms, canary, D-05..D-10, D-12) | test | event-driven (full monitor) | 50-13 `HandoffRun` harness + `resume_with_agent_*` tests, same file | exact |
| `crates/devflow-cli/src/pipeline_gate.rs` | controller (gate / loop-back) | CRUD on state | `prepare_loop_back_to_code` commit block; `gate_resolution_keeps_a_notify_cmd_edited_during_the_wait`, same file | exact |
| `crates/devflow-cli/src/pipeline_outcomes.rs` | controller (cron arm) | file-I/O | `handle_rate_limited_outcome` (unchanged; becomes the else arm) | exact (reuse) |
| `crates/devflow-cli/src/preflight.rs` | service (checks) | request-response | used, not modified; possible visibility change only | reuse |
| `crates/devflow-cli/tests/gate_notify_persist_e2e.rs` | test (process e2e) | request-response | `resume_with` helper, same file | exact |
| `doc-check-allowlist.toml` | config | — | `DEVFLOW_TEST_HOLD` / `DEVFLOW_TEST_MONITOR_FULL` entries | exact |
| `OPERATIONS.md`, `docs/guides/unattended-mode.md`, `README.md` | docs | — | `OPERATIONS.md:32` resume row, `:334` legacy-launch row, `:359` cron row | exact |
| 999.71 measurement harness + archived measurements file | utility / evidence | batch | none (see No Analog Found) | none |

## Pattern Assignments

### `crates/devflow-core/src/state.rs` (model, file-I/O) — D-01, D-05, D-09 persistence

**Analog:** the `canary` and `legacy_claude_launch` fields of `State`, same file.

**Why additive is safe** (lines 16-36): `State` is `#[derive(Debug, Clone, Serialize, Deserialize)] #[non_exhaustive] pub struct State`; the doc comment records that every `#[serde(default)]` field added since is additive and old state files still load.

**Field pattern** (lines 385-408) — doc comment states what the default means for an old file and why it is persisted, not passed:
```rust
    /// ... `None` means EITHER "no canary has run for
    /// this run yet" OR "the state was written by a binary predating this
    /// field" — both cases behave identically: the canary runs.
    ///
    /// Persisted rather than held in memory for the same reason
    /// [`Self::yes_ship`] is: each stage launch happens in a SEPARATE
    /// `devflow` process ...
    #[serde(default)]
    pub canary: Option<crate::canary::CanaryOutcome>,
    ...
    #[serde(default)]
    pub legacy_claude_launch: bool,
```
New fields (planner names them; e.g. `fallback_agent: Option<AgentKind>`, `fallback_consumed: bool`, interrupted Code fix `Option<FixType>`) follow this shape. `gate_notify_cmd` (line 449) shows the `#[serde(default, skip_serializing_if = "Option::is_none")]` variant if an absent key is preferred on disk.

**Constructor** (lines 653-690): every field is listed explicitly in `State::new`; add the new ones there (`fallback_agent: None`, `fallback_consumed: false`, ...). Missing one is a compile error, which is the intended guard.

**Test pattern** (lines 970-1005) — one round-trip test asserting the key appears in JSON, plus an "absent from JSON defaults" test using a minimal literal state:
```rust
    #[test]
    fn infra_failures_round_trips_through_serde() {
        let mut state = State::new(PhaseId::new(1), AgentKind::Claude, Mode::Auto, PathBuf::from("/repo"));
        state.infra_failures = 4;
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("infra_failures"), "infra_failures must appear in persisted JSON");
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.infra_failures, 4, ...);
    }

    #[test]
    fn infra_failures_absent_from_json_defaults_to_zero() {
        let json = r#"{ "stage": "code", "phase": 1, "agent": "claude", "mode": "auto",
                        "started_at": "0", "project_root": "/repo" }"#;
        let loaded: State = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.infra_failures, 0);
    }
```
Use this for `state::tests::fallback_fields_round_trip_and_tolerate_states_predating_them` (VALIDATION map).

---

### `crates/devflow-core/src/prompt.rs` (utility, transform) — D-09

**Analog:** `idempotent_stage_prompt` (lines 387-408), `workflow_plan_prompt` (lines 516-535), `define_stage_prompt` (lines 420-433).

**The short-circuit to avoid on failover** (lines 387-408):
```rust
fn idempotent_stage_prompt(phase: PhaseId) -> String {
    let artifact = "PLAN.md";
    let command = gsd_command_for(Stage::Plan, phase);
    let padded = phase.padded();
    format!(
        "First check whether this stage's deliverable already exists:\n\
        \n\
        ls .planning/phases/{padded}-*/{padded}-*{artifact} 2>/dev/null\n\
        \n\
        - If it EXISTS: the stage's work is already done. Do NOT run the GSD \
        command, ...
        - If it does NOT exist: run the GSD workflow command for this stage:\n\
        \n\
        \x20   {command}\n\
        \n\
        {COMPLETION_PROTOCOL}"
    )
}
```
`workflow_plan_prompt(phase, workflow_root)` (516-535) is the Codex/Pi twin, ending in `read and follow the GSD workflow file at {workflow_root}/plan-phase.md for phase {phase}.` A new continuation prompt needs **both** renderer styles, built from the same pieces (`gsd_command_for`, `COMPLETION_PROTOCOL`, `phase.padded()`), and must not contain the "If it EXISTS ... already done" text (that is the D-09 negative-control assertion).

**Renderer dispatch** (lines 474-514): `render_claude_style` / `render_workflow_style` match on `StageIntent`. `StageIntent` (lines 165-184) is an **exhaustive** pub enum with no `#[non_exhaustive]` — do not add a variant or field (semver major, RESEARCH Pitfall 4). Add a separate `pub fn` instead.

**FixType** (lines 244-257): `#[derive(Debug, Clone, Copy, PartialEq, Eq)] #[non_exhaustive] pub enum FixType { AuditFix, GapsOnly, FullExecute }`. Adding `serde::Serialize, serde::Deserialize` to the derive is the additive change RESEARCH probed. The doc comment above it (lines 230-243) is the house style for recording a semver decision; extend it with one sentence on why serde was added.

---

### `crates/devflow-core/src/agent_result.rs` (service, transform) — D-15, D-16

**Analog 1 — torn-line signal (D-16):** `ParsedCapture` (lines 1235-1321).
```rust
struct ParsedCapture {
    events: Vec<serde_json::Value>,
    line_shapes: Vec<LineShape>,
}
...
    fn torn_json_after_last_matching(&self, pred: impl Fn(&serde_json::Value) -> bool) -> bool {
        let mut last_match_line = None;
        let mut event_idx = 0usize;
        for (line_idx, shape) in self.line_shapes.iter().enumerate() {
            if *shape == LineShape::Event {
                if pred(&self.events[event_idx]) { last_match_line = Some(line_idx); }
                event_idx += 1;
            }
        }
        self.line_shapes.iter().enumerate().any(|(line_idx, shape)| {
            *shape == LineShape::TornJson && last_match_line.is_none_or(|last| line_idx > last)
        })
    }
```
**Call site to change** (lines 2356-2362) — keep the pub signature, compute the torn signal inside:
```rust
#[must_use]
pub fn capture_shows_rate_limit_denial(project_root: &Path, phase: PhaseId) -> bool {
    let Some(raw) = read_capture(&stdout_path(project_root, phase)) else {
        return false;
    };
    detect_claude_stream_rate_limit(&ParsedCapture::parse(&raw).events).is_some()
}
```
`detect_claude_stream_rate_limit` (lines 1735-1775) reads `rate_limit_info.status` by direct `Value::get` against `CLAUDE_STREAM_RATE_LIMIT_DENIAL_STATUSES = &["rejected"]` (line 1692). Keep that; never substring-match a torn line (T-30-12). The tri-state is a new crate-private enum or a second private fn, not a changed pub return type.

**Analog 2 — idle-timeout record read (D-15):** `IdleTimeoutRecord` (lines 2273-2289) is `#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)] pub struct` with five `pub` fields and **no** `#[non_exhaustive]` — do not add fields to it (semver major, verified by RESEARCH). Read path `parse_idle_timeout_side_channel` (lines 2364-2410):
```rust
    let Ok(record) = serde_json::from_str::<IdleTimeoutRecord>(&raw) else {
        return Some(idle_timeout_result(format!(
                "idle timeout: DevFlow's monitor recorded a timeout verdict at {} but the \
                 record itself is unreadable, ...", path.display()), None));
    };
    ...
    Some(idle_timeout_result(
        format!(
            "idle timeout: the agent's output stream was silent for {}s, so DevFlow \
             terminated it (agent pid {}). {commit_phrase} Review the branch before deciding \
             what to keep — this run is TERMINAL and is not retried automatically.",
            record.idle_secs, record.agent_pid
        ),
        Some(record.commits.len() as u32),
    ))
```
Swap the `from_str::<IdleTimeoutRecord>` for a crate-private wrapper (`#[serde(flatten)] record: IdleTimeoutRecord` + `#[serde(default)]` cause fields) and append a cause clause only when present; the "fields absent → today's message" test pins the exact current string.

**Test analogs** (same file):
- Fixture writer `write_idle_timeout_record(root, phase, commits)` (lines 6806-6828) — builds the pub struct literal; a pre-51 record fixture is exactly this output (it has no new keys), so `a_pre_51_idle_timeout_record_still_parses` can reuse it directly.
- `an_unreadable_idle_timeout_record_still_produces_the_verdict` (lines 6979-7010) — shape for a verdict test with a negative control evaluated first via `evaluate_layer1`.
- `a_quota_denial_in_the_capture_is_visible_to_the_monitor` (7925-7943) and `an_ordinary_capture_is_not_mistaken_for_a_quota_denial` (7951-7981) — the positive/negative pair for D-16; the torn test writes `format!("{V3_INIT_EVENT}\n{}", <truncated rate_limit_event>)` and the control keeps the intact `v3_rate_limit_event("rejected")`.

**Reuse, do not re-implement:** `phase_commit_count(project_root, &git_flow, phase) -> Option<u32>` (line 2563) for D-10's commit count; `None` is "could not measure", never `0`.

---

### `crates/devflow-core/src/monitor.rs` (service, file-I/O) — D-15 write, D-16 idle path, D-12 seam, D-17 doc comment

**Analog 1 — record writer** `write_idle_timeout_record` (lines 2858-2888):
```rust
fn write_idle_timeout_record(project_root: &Path, phase: PhaseId, idle_secs: u64,
                             child_pid: u32, commits: &[IdleTimeoutCommit]) -> std::io::Result<()> {
    let record = IdleTimeoutRecord { status: ..., idle_secs, agent_pid: child_pid, written_at: ..., commits: commits.to_vec() };
    let json = serde_json::to_string(&record)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    let path = crate::agent_result::idle_timeout_path(project_root, phase);
    if let Some(parent) = path.parent() { crate::workflow::ensure_devflow_dir(parent)?; }
    let mut file = std::fs::File::create(&path)?;
    file.write_all(json.as_bytes())?;
    file.flush()?;
    file.sync_all()
}
```
Serialize the same crate-private wrapper the reader uses (one type for both directions, so they cannot drift).

**Analog 2 — idle-timeout suppression on a denial** (lines 2704-2720): the never-silent shape (warn! + `append_monitor_log` + terminate) to follow when the torn tri-state is introduced:
```rust
    if crate::agent_result::capture_shows_rate_limit_denial(project_root, phase) {
        warn!("idle timeout after {idle_secs}s, but the capture carries an explicit quota \
             denial — NOT recording an idle-timeout verdict, ...");
        append_monitor_log(project_root, phase, &format!("[idle-timeout] suppressed after {idle_secs}s: ..."));
        terminate_child_group(child_pid);
        return;
    }
```

**Analog 3 — monitor e2e test** `a_quota_denial_is_not_recorded_as_an_idle_timeout` (lines 4116-4153) with its negative control `idle_timeout_still_fires_when_no_background_task_is_open` (4158-4182): drive `supervise_stream_child(root, phase, root, "prompt", Duration::from_secs(1), "sh", &["-c".to_string(), script], &[], AgentKind::Claude)` with a stub script, then assert on `idle_timeout_path(root, phase)`. Use for `monitor::tests::the_idle_timeout_record_carries_the_last_result_cause` (stub prints a `result` event with `terminal_reason`/`errors`/`origin`, then goes silent).

**Analog 4 — D-12 publish-probe seam:** `monitor_test_hold` (lines 2062-2083) is the `#[cfg(test)]` env-gated seam form (`std::env::var("DEVFLOW_TEST_…")`, `split_once(':')`, early return when unset). The read to withhold is in `wait_for_publish` (lines 2440-2462): `if let Ok(crate::supervisor::Liveness::Alive(reply)) = crate::supervisor::probe(socket.as_path()) && reply.launch_id == launch_id { return Ok(reply); }`. Caution: CLI tests reach this through the CLI test binary's harness entry, so a `#[cfg(test)]` gate in **devflow-core** is not compiled into that build unless it uses `#[cfg(any(test, feature = "test-support"))]`, as `STREAM_WAIT_HOLD_WAIT` does (line 2087). Checked: `crates/devflow-cli/Cargo.toml` `[dev-dependencies]` enables `devflow-core`'s `test-support` feature, so `#[cfg(any(test, feature = "test-support"))]` is the gate that reaches CLI tests; core's `monitor_test_hold` is plain `#[cfg(test)]` and does not.

**D-17 doc comment** (lines 384-394) `strip_notify_env`: current text claims coverage of "the pipe-owning agent and the delivery canary"; correct only if the WR-13 work shows it overstates.

---

### `crates/devflow-core/src/doc_check.rs` (test, batch scan) — D-14

**Analog:** `no_monitor_pid_field_json_key_or_help_mention_remains` (lines 596-660) and its helpers (504-589).

**Self-exclusion by `concat!`** (line 506):
```rust
const REMOVED_STATE_FIELD: &str = concat!("monitor", "_pid");
```
Apply to `rebase_in`, `Sequentagent` and the `"n"` alias text so doc_check.rs itself never holds a token contiguously.

**Comment stripping and identifier boundary** (lines 510-533):
```rust
fn uncommented_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines().enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .map(|(index, line)| (index + 1, line))
}
fn is_identifier_char(ch: char) -> bool { ch.is_alphanumeric() || ch == '_' }
```
Limit (RESEARCH Pattern 5): whole-line only; a trailing `// rebase_in` or `/* */` block is not stripped. Either extend or document the limit in the test's doc comment.

**Pure finding fn + planted controls in the same test** (lines 549-639):
```rust
    let planted = |text: String| vec![(PathBuf::from("planted.rs"), text)];
    let declared = ["    pub ", field, ": Option<u32>,"].concat();
    assert_eq!(removed_field_findings(&declared, none, none).len(), 1);
    let doc_commented = ["    /// pub ", field, ": Option<u32>,"].concat();
    assert!(removed_field_findings(&doc_commented, none, none).is_empty());
    ...
    let rust_files = read_labelled(rust_file_paths());
    assert!(!rust_files.is_empty(), "the walker must find Rust sources");
```
**Variant extraction:** `enum_variants(source, "Command")` (lines 188-221) returns **lowercased**, all-alphabetic variant names; compare against `"sequentagent"`. Real-source decoys already present that must stay clean: doc comments at `crates/devflow-cli/src/pipeline_outcomes.rs:83` and `crates/devflow-core/src/ship.rs:922`, and the string literal at `ship.rs:940` (`assert!(!record.hermes_cron.command.contains("sequentagent"));`). `rg` finds no `rebase_in` anywhere in `crates/` today.

---

### `crates/devflow-cli/src/main.rs` (config, clap) — D-01

**Analog:** `legacy_claude_launch` on `Start` (lines 92-112) and `Resume` (lines 195-201); `Resume.agent: Option<AgentKind>` (lines 192-194):
```rust
        /// Hand the saved run to a different agent driver; omit to keep its saved agent.
        #[arg(long)]
        agent: Option<AgentKind>,
```
`--fallback-agent` is `#[arg(long)] fallback_agent: Option<AgentKind>` on both variants. Thread it through the destructuring at lines 577-617 (`Command::Start { ... } => start(...)`) and 656-661 (`Command::Resume { ... } => resume(...)`). No `alias`/`name` attribute (D-14 scans those).

---

### `crates/devflow-cli/src/commands.rs` `start` (controller) — D-01, D-02, D-03

**Analog:** the persistence block inside `start` (lines 346-405):
```rust
#[allow(clippy::too_many_arguments)]
pub(crate) fn start(project_root: &Path, phase: PhaseId, agent: AgentKind, mode: Mode, force: bool,
    worktree: bool, dry_run: bool, until: Option<Stage>, yes_ship: bool, legacy_claude_launch: bool,
) -> Result<(), CliError> {
    let mut state = fresh_state_carrying_phase_failures(project_root, phase, agent, mode);
    state.stop_until = until;
    ...
    state.yes_ship = yes_ship || config_yes_ship;
    state.gate_notify_cmd = devflow_core::gates::apply_notify_env(state.gate_notify_cmd.take());
    if crate::pipeline_launch::apply_legacy_launch_opt_out(&mut state, legacy_claude_launch) { println!(...); }

    if dry_run {
        print_dry_run(&state);
        return Ok(());
    }

    let _phase_holder = claim_foreground_holder(project_root, phase)?;
    let reserved = reserve_launch(...)?;
```
Set `state.fallback_agent` here, before `dry_run` (so `print_dry_run`, `pipeline_gate.rs:934`, can print it) and before `reserve_launch` (the comment at 412-413: "a refusal changes nothing"). D-02/D-03 refusals go before the dry-run return so a dry run also refuses.

**D-03 checks to reuse (preflight.rs, not modified except possibly visibility):**
- `pub(crate) fn ensure_agent_binary(program: &str)` (line 92); program from `agent_program(to)` as `resume_loaded` does.
- `agents::driver_for(to).health(&candidate)`.
- `fn unattended_launch_shape_condition(state: &State) -> ConditionState` (lines 1031-1059) is **private**; evaluating it for a candidate state from `commands.rs` needs a `pub(crate)` wrapper or a call through an existing pub(crate) entry. Only Claude/Antigravity hold it (line 1032-1035).

**Call-site cost:** `start` already takes 10 positional params; tests call it positionally (e.g. lines 15447, 15831, 16436, 18101), so a new param touches each. The child-process test shape for a refusal is `a_signal_to_a_start_parked_at_a_preflight_gate_keeps_the_resumable_state` (lines 18094-18117): `start(&root, phase, AgentKind::Claude, Mode::Auto, false, false, false, None, false, true).expect_err(...)`. A D-02 refusal that fires before `claim_foreground_holder` needs no child process.

---

### `crates/devflow-cli/src/pipeline_launch.rs` — shared handoff fn and failover branch (controller, event-driven) — D-04..D-08, D-10

**Analog 1 — the substitution to extract** `resume_loaded` (lines 3163-3219):
```rust
    if let Some(requested) = agent
        && requested != state.agent
    {
        let from_agent = state.agent;
        let mut candidate = state.clone();
        candidate.agent = requested;
        // Runs the FULL generic preflight bundle ... (44-CORE-REVIEW-FINDINGS.md finding 2b)
        generic_preflight_checks(project_root, &candidate).map_err(|reason| {
            CliError::Message(format!(
                "handoff to {requested} refused at saved {} stage: {reason}",
                state.stage
            ))
        })?;
        ensure_agent_binary(agent_program(requested))
            .map_err(|err| CliError::Message(format!("handoff to {requested} refused: {err}")))?;
        agents::driver_for(requested)
            .health(&candidate)
            .map_err(|err| CliError::Message(format!("handoff to {requested} refused: {err}")))?;

        commit(&mut state, |s| s.agent = requested)?;
        events::emit(project_root, phase, "agent_handoff", serde_json::json!({
                "stage": state.stage.to_string(),
                "from_agent": from_agent.to_string(),
                "to_agent": requested.to_string(),
                "reason": "resume --agent",
            }));
        println!("handoff: {from_agent} → {requested}");
    }
```
Extract into one CLI-private fn; the single `commit` closure also sets `s.canary = None` (D-04), the consumed mark (failover only), and optionally `s.session_id = None`. `resume_loaded` must keep the exact `"handoff to {requested} refused ..."` strings and `"reason": "resume --agent"` (pinned by `resume_with_agent_hands_off_and_relaunches_under_the_new_driver`, lines 5257-5306).

**Analog 2 — post-launch failure is never silent** `resume_loaded` (lines 3266-3289):
```rust
    match launch_stage(&mut state, None, None) {
        Ok(()) => Ok(()),
        Err(err) => {
            // ... (44-CORE-REVIEW-FINDINGS.md finding 2a). Re-mark the phase stopped ...
            let _ = commit(&mut state, |s| {
                s.stopped = true;
                s.stop_reason = Some(format!("resume launch failed: {err}"));
            });
            Err(err)
        }
    }
```
The failover equivalent routes the `Err` to an event + `handle_rate_limited_outcome` (or `gate_or_abort_infra`), except a `NotOwner` error, which is returned unchanged.

**Analog 3 — same-stage re-drive from the AutoResume arm** `handle_ambiguous_outcome` (lines 2906-2935):
```rust
pub(crate) fn handle_ambiguous_outcome(project_root: &Path, state: &mut State, stage: Stage,
                                       reason: Option<String>) -> Result<(), CliError> {
    commit(state, |s| { s.infra_failures = s.infra_failures.saturating_add(1) })?;
    if state.infra_failures >= mode::MAX_INFRA_FAILURES {
        return gate_or_abort_infra(project_root, state, stage, reason);
    }
    events::emit(project_root, state.phase, "ambiguous_transport_retry",
        serde_json::json!({ "stage": stage.to_string(), "infra_failures": state.infra_failures, "reason": reason }));
    launch_stage(state, None, Some(stage))
}
```
Failover copies the emit + `launch_stage(state, prompt, Some(stage))` tail but **drops the bump-and-ceiling head** (D-06).

**Branch point** (lines 3736-3741):
```rust
        Action::AutoResume => match result.status {
            agent_result::AgentStatus::Ambiguous => {
                handle_ambiguous_outcome(project_root, &mut state, stage, result.reason)
            }
            _ => handle_rate_limited_outcome(project_root, &mut state, phase, stage, result.reason),
        },
```
The `_` arm becomes: fallback set and unused → failover fn; else → `handle_rate_limited_outcome` unchanged. Do not add an `Action` variant (`outcome_policy.rs` `pub enum Action` is exhaustive).

**Relaunch prompt selection** `launch_stage` (lines 2856-2891): `prompt_override: None` renders `StageIntent::for_stage_in_project(state.stage, ...)` with the **current** `state.agent`'s driver (2861-2868). After the handoff commit `state.agent` is the fallback, so rendering after the commit uses the fallback's renderer. Pass `Some(rendered)` only for the D-09 cases (Plan continuation, persisted Code fix).

**Canary** `canary_gate` (lines 552-578): reuses `state.canary` when `Some` without running the closure — the reason the handoff commit must clear it. `canary_launcher_for` (542-550) picks the Antigravity launcher only for Antigravity.

**Locking rule** (lines 1095-1107): `fenced` is non-reentrant; never call `commit`/`close_gate` inside a `fenced` op. `commit` (line 1187) reloads under `update_engaged_state`, so sequential `commit` calls are the pattern.

**Resume flag (D-01):** `resume(project_root, phase, agent, legacy_claude_launch)` (line 2977). Caution: about 30 lines call `resume(` across `pipeline_launch.rs`, `commands.rs` and `main.rs` (rg line count, not a call-graph count); a new positional param edits each. Setting the fallback belongs next to the `apply_legacy_launch_opt_out` / `commit` block in `resume_loaded` (lines 3220-3265), before `launch_stage`, after every refusal.

---

### `crates/devflow-cli/src/pipeline_launch.rs` — tests (event-driven, full monitor) — D-04..D-12

**Analog 1 — full-monitor harness** (lines 9121-9338):
- `handoff_claude_dir(first_marker)` (9139-9156): fake `claude` whose first run waits for `go` then prints `first_marker`, later runs wait for `go2` and print `SUCCESS_MARKER`. For D-11, pass the rate-limit envelope as `first_marker` and add a second stub (the fallback, e.g. `codex`) to the same dir that prints `SUCCESS_MARKER`.
- `run_handoff_child(name, hold, first_marker)` (9161-9182): parent half; sets `TEST_MONITOR_FULL_ENV=1` and `DEVFLOW_GATE_TIMEOUT_SECS=120`, runs the named test in a hermetic child, asserts exactly one passing test.
- `HandoffRun::launch_with(phase, |root, state| { ... })` (9205-9237): sets `legacy_claude_launch = true` (capture shape; RESEARCH Pitfall 8), `stop_until = Some(Stage::Plan)`, launches Define. A D-11 test sets `state.fallback_agent` in the closure and likely a later `stop_until`.
- `Drop for HandoffRun` (9294-9326) reaps every supervisor in the cache dir; keep using it so a failing test leaves no process.
- Assertions via `events_named(&run.root, "agent_handoff")` (10152-10159), `stage_launched_count(root, phase)` (`test_support.rs:617`), `poll_until(HANDOFF_STEP_WAIT, || ...)` (helper at 4024).

Child-test skeleton (lines 10839-10862):
```rust
        const NAME: &str = "pipeline_launch::tests::<name>";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("after_safe_point_1"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch_with(PhaseId::new(1319), |_, state| { ... });
        run.go();
```
Use a fresh `PhaseId` per test (existing ones run 1301-1940).

**Analog 2 — direct resume tests** `resume_with_agent_from_a_rate_limited_state_relaunches` (5576-5619): `agent_free_dir_with_agent_stub("codex")` + `run_hermetic_test_in_child`, then `ReapMonitorOnDrop::after_launch`. Use for `resume_with_a_fallback_agent_rearms_a_consumed_fallback`.

**Analog 3 — spawn failure after commit (D-04)** `resume_re_marks_stopped_when_launch_stage_fails_outright` (5636-5677): `child_no_git_root()` / `run_test_without_git(NAME, root)`; the child's empty `PATH` makes `ensure_agent_binary` fail. Alternative seam: the test build of `launch_context()` (1994-2003) errors when `DEVFLOW_CACHE_DIR` is absent.

**Analog 4 — canary negative control (D-04)** `canary_gate_only_applies_to_the_stream_launch_path` (6361-6414) with `counting_canary(&calls, outcome)` (6266-6274) and `canary_state(root, phase)` (6276-6281). The house rule visible there: assert the predicate is non-vacuous before relying on it.

---

### `crates/devflow-cli/src/pipeline_gate.rs` (controller, CRUD) — D-09 Code fix persistence, D-17 WR-12/IN-10

**Analog:** `prepare_loop_back_to_code` (lines 178-221), the only place an audit-fix/gaps-only Code pass is entered:
```rust
    commit(state, |s| {
        s.stage = Stage::Code;
        s.gate_pending = false;
        s.last_verification_fingerprint = observed_fingerprint;
        s.last_verification_mtime_nanos = observed_mtime_nanos;
    })?;
    events::emit(project_root, state.phase, "loop_back", serde_json::json!({ ..., "fix": format!("{fix:?}") }));
    ...
    Ok(devflow_core::agents::driver_for(state.agent).render_prompt(
        &devflow_core::prompt::StageIntent::Code { phase: state.phase, fix: Some(fix) },
    ))
```
Add the persisted fix to that commit closure. Clearing it on an ordinary Code entry needs a matching write in `transition` (same file) or wherever Plan→Code is committed.

**Gate entry** (line 530): `commit(state, |s| s.gate_pending = true)?;` at the top of `run_gate_with_timeout` — the WR-12/IN-10 reproduction target.

**Test analog** `gate_resolution_keeps_a_notify_cmd_edited_during_the_wait` (lines 2603-2654): thread runs `run_gate_with_timeout` on a loaded state, the test edits `gate_notify_cmd` on disk while the gate waits, answers with `answer_open_gate`, then asserts both the in-memory and on-disk values. WR-12's reproduction edits on disk **before** gate entry (after load) instead; IN-10's writes unparseable bytes and asserts they are unchanged afterwards.

---

### `crates/devflow-cli/src/pipeline_outcomes.rs` (controller, file-I/O) — cron arm, reused unchanged

`handle_rate_limited_outcome` (lines 89-149): bump `infra_failures` → ceiling check → `build_single_agent_cron_instructions` + `write_cron_instructions` → empty schedule routes to `gate_or_abort_infra` → `rate_limit_resume_scheduled` event. It is the else arm and the D-07 refusal destination. The doc comment at line 83 mentions `sequentagent` (a decoy the D-14 scan must ignore; edit only if the planner wants the stale reference gone).

**Test analog** `rate_limited_at_infra_ceiling_stops_resuming_and_aborts` (lines 5033-5060): writes the capture-shape envelope directly and calls `advance(root, Some(phase))`:
```rust
        std::fs::write(
            agent_result::stdout_path(root, phase),
            r#"{"type":"result","subtype":"error_rate_limit","retry_after":"2026-06-18T15:45:30Z"}"#,
        ).unwrap();
        advance(root, Some(phase)).unwrap();
```
This is the cheapest rig for the D-06 boundary test (`infra_failures = MAX_INFRA_FAILURES - 1` + unused fallback → handoff instead of abort; control: same without a fallback aborts). It launches a real relaunch on the failover arm, so that test needs the child-process + agent-stub PATH form of `rate_limited_with_unparseable_retry_hint_gates_instead_of_stalling_silently` (lines 5069-5077).

---

### `crates/devflow-cli/tests/gate_notify_persist_e2e.rs` (test, process e2e) — D-17 WR-13

**Analog:** `resume_with(notify)` (lines 362-407). Reuse its setup verbatim (HermeticHome, `init_start_repo`, `gate_notify_cmd = Some("old")`, `verification_baseline_captured = true`, `fake_bin_dir()`, `MarkedReaper`, `--legacy-claude-launch`, `LaunchedRunReaper`). Replace only the wait loop: today it exits on `saved != Some("old")`, which never happens with `NotifyEnv::Unset` (RESEARCH Pitfall 7). Wait instead for a marker strictly after `commit_notify_env` (e.g. `stopped == false` with a supervisor or pending supervisor recorded), then assert `persisted_cmd(root, phase).as_deref() == Some("old")`. The mutation control named in RESEARCH is `pipeline_launch.rs:1201` (`apply_notify_env(state.gate_notify_cmd.clone())` → `apply_notify_env(None)`).

---

### `doc-check-allowlist.toml` (config) — D-12 seam

**Analog** (lines 64-66):
```toml
[[exceptions]]
kind = "source_to_docs"
token = "DEVFLOW_TEST_HOLD"
reason = "Test-only: #[cfg(test)] hold points ... compiled out of release builds, so it is not an operator setting."
```
`validate_allowlist` (`doc_check.rs:28-38`) fails on an empty `reason`.

---

### `OPERATIONS.md`, `docs/guides/unattended-mode.md`, `README.md` (docs)

- `OPERATIONS.md:32` — the `devflow resume --phase N` row; add `--fallback-agent` and the re-arm behaviour. A matching `devflow start` row sits in the same table.
- `OPERATIONS.md:334` — the `DEVFLOW_CLAUDE_LEGACY_LAUNCH` row is the style for "persisted to `state-NN.json`, never cleared by resume".
- `OPERATIONS.md:359` — the `cron-instructions-NN.json` row; note it is written only when no fallback is configured or the fallback is spent.
- `README.md:110` — rate-limit sentence; mention failover.
- `docs/guides/unattended-mode.md:69` — the legacy opt-out bullet is the analog for a fallback bullet (D-03: auto mode refuses a non-Claude/Antigravity fallback).

**doc_check enforcement to plan for** (`doc_check.rs:357-400`): every `--flag` written in these docs must exist in source as the literal or as `field_name:`; every backticked `Callable()`, `a::b` or UpperCamel identifier in them must exist in Rust source. Naming a new fn or event in backticks in the docs ties the docs to that exact name.

## Shared Patterns

### Persist-then-act on `State`
**Source:** `crates/devflow-cli/src/pipeline_launch.rs:1187-1193` (`commit`) and every call site above.
**Apply to:** handoff fn, failover branch, `prepare_loop_back_to_code`, resume flag.
One `commit(&mut state, |s| { ... })` per logical write; it reloads the on-disk state under the engaged lock and replaces the caller's copy. Never inside a `fenced` op.

### Never-silent outcomes
**Source:** `pipeline_outcomes.rs:60-78` (`gate_or_abort_infra`), `pipeline_launch.rs:3266-3289`, `monitor.rs:2704-2720`.
**Apply to:** D-04 relaunch failure, D-07 refusal, D-16 torn path. Every skip or refusal emits an `events::emit(project_root, phase, "<name>", json!({ "stage": ..., "reason": ... }))` and lands in a gate, abort or cron arm.

### Event emission
**Source:** `pipeline_launch.rs:3207-3217` (`agent_handoff`), `pipeline_outcomes.rs:138-147` (`rate_limit_resume_scheduled`).
**Apply to:** `agent_handoff` (new `"reason": "rate_limited"`, D-10 tree fields) and any refusal/relaunch-failure event. Keys are snake_case; values are `to_string()` of the enum (`stage`, agents).

### Child-process hermetic tests
**Source:** `devflow_core::test_support::in_child_test(NAME)` + `crate::test_support::run_hermetic_test_in_child(NAME, path_dir, &env)` + `assert_child_ran_exactly_one_passing_test` (e.g. `pipeline_launch.rs:5577-5586`).
**Apply to:** every test that launches a stage or empties `PATH`. `NAME` must be the full `module::tests::fn` path; a wrong name fails the parent's exactly-one assertion rather than passing vacuously.

### Negative controls inside the test
**Source:** `doc_check.rs:602-632`, `pipeline_launch.rs:6385-6393`, `agent_result.rs:6989-6995`.
**Apply to:** every new test (the VALIDATION map names the control for each). The control is asserted in the same test, before or beside the claim.

### Semver discipline for devflow-core
**Source:** `state.rs:16-36` (`#[non_exhaustive]` rationale), `prompt.rs:230-243`.
**Apply to:** every core edit. Fields only on `State`; serde derives on `FixType`; new `pub fn`s; no change to `IdleTimeoutRecord`, `StageIntent`, `Action` or any existing pub signature. Gate: `cargo semver-checks check-release -p devflow-core`.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| 999.71 measurement harness + archived measurements file (D-16 second deliverable) | utility / evidence | batch | No existing harness classifies capture final-line shapes over time. Closest pieces: `ParsedCapture::parse` + `LineShape` (`agent_result.rs:1215-1290`, crate-private) for the classification and `events::emit` for a per-evaluation record. RESEARCH Open Question 3 proposes the shape; the planner must choose where it lives (a `#[cfg(test)]`/`#[ignore]` probe vs an `events.jsonl` record) and whether it touches the pub surface. |

## Metadata

**Analog search scope:** `crates/devflow-cli/src/`, `crates/devflow-cli/tests/`, `crates/devflow-core/src/`, `doc-check-allowlist.toml`, `OPERATIONS.md`, `README.md`, `docs/guides/`.
**Files read (targeted ranges):** 16.
**What this map does not establish:** the line numbers are correct at `ce08a9d` only. The `resume(` call-site count is a ripgrep line count, not a call graph. The cfg visibility of a core seam to CLI tests was read from `Cargo.toml`, not proven by compiling one.
**Pattern extraction date:** 2026-11-05
