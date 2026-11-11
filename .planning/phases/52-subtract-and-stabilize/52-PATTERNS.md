# Phase 52: Subtract and Stabilize - Pattern Map

**Mapped:** 2026-11-09 (tree `49e5f66`, worktree `.worktrees/phase-52`)
**Files analyzed:** 16 new/modified groups (subtraction-only deletions are not mapped; RESEARCH.md §Cut graph covers them)
**Analogs found:** 14 / 16. Every analog path below is git-tracked (`git ls-files` checked).

Line numbers were read this session. Excerpts are trimmed; open the cited range before you copy.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match |
|---|---|---|---|---|
| `crates/devflow-core/src/gsd_reader.rs` (NEW) | service (subprocess reader) | request-response, JSON parse | `git.rs::hermetic_command` + `agent_result.rs::phase_commit_count` + `gsd_config.rs` error enum | role-match (no gsd subprocess exists today) |
| `crates/devflow-cli/tests/common/mod.rs` (+ stub `gsd_run`, capture-shape agent) | test helper | file-I/O | `common::fake_bin_dir_for` + `HermeticHome` | exact |
| `crates/devflow-core/src/gates.rs` (`GateResponse.abort`, `GateAction`, `reap_answer`) | model/protocol | event-driven (file answers) | itself (L52-108, L680-695) | exact |
| `crates/devflow-cli/src/main.rs` `GateCmd::Abort` + `commands.rs::gate_respond` | controller (CLI verb) | request-response | `GateCmd::Reject` (main.rs L476-501, dispatch L714-733) | exact |
| `crates/devflow-core/src/prompt.rs` (note carrier, quick-fix, stale re-verify, gaps Plan) | utility (renderer) | transform | `fix_prompt` L713-741, `workflow_code_prompt` L558-585 | exact |
| `crates/devflow-core/src/state.rs` (note, auto-relaunch-used flag, fix base commit) | model | CRUD (persisted state) | `fallback_consumed` L483-491, `code_fix` L493-507 | exact |
| `crates/devflow-cli/src/pipeline_outcomes.rs` rate-limit gate (replaces L80-148) | service (pipeline policy) | event-driven + timer | `run_gate_with_timeout` auto_response path (`pipeline_gate.rs` L521-700) | role-match |
| new reset-time module (relocated from `ship.rs` L304-490 + `parallel.rs` L165-170) | utility | transform | `ship.rs` parser and its tests | exact (relocation) |
| `crates/devflow-cli/src/pipeline_gate.rs` `finish_run` (replaces `finish_workflow` L238-388) | service | event-driven | `transition`'s `--until` halt branch L73-97 | exact |
| `pipeline_outcomes.rs::handle_validate_outcome` (verdict pairing, routes) | service | request-response | itself L523-766 (evidence root L538-541) | exact |
| `crates/devflow-core/src/mode.rs` streak predicates (D-25, D-34) | utility | transform | itself L142-144, L180-181; count from `phase_commit_count` | exact |
| `crates/devflow-cli/src/commands.rs` sweep skip for rate-limit gate | controller | batch | `gate_sweep_may_reap` L2684-2686 | exact |
| `crates/devflow-core/src/doc_check.rs` `no_removed_surface_literal_remains` (NEW test) | test (source scan) | batch | `no_monitor_pid_field_json_key_or_help_mention_remains` L597-671 | exact |
| e2e: `failover_e2e.rs`, `supervisor_e2e.rs`, `gate_wedge_e2e.rs` (rewrites + new tests) | test (e2e) | event-driven | themselves (Rig, Shape) | exact |
| `crates/devflow-cli/build.rs` + `main.rs` `#[command(version)]` (OPS-01) | config | build-time | `build.rs` L50-69 | exact |
| release preflight port (D-02/D-31; tool or script) | tooling | batch | `tests/release_check.rs` (fixture L71-83), `tests/cut_release_guard.rs` | partial |

## Pattern Assignments

### `crates/devflow-core/src/gsd_reader.rs` (NEW; service, subprocess + JSON)

**Subprocess construction: copy `git.rs` L87-94** (`hermetic_command`). It sets argv, `current_dir`, and removes every `GIT_*` redirecting variable:
```rust
pub fn hermetic_command(program: &str, dir: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(dir);
    for var in REPO_LOCAL_GIT_VARS.iter().chain(ALSO_REDIRECTING_GIT_VARS) {
        cmd.env_remove(var);
    }
    cmd
}
```
It takes `program: &str`. The launcher is a `PathBuf` (`$HOME/.claude/gsd-core/bin/gsd_run`), so either refuse a non-UTF-8 path or add a `&Path` sibling. Do not build a bare `Command::new`. Copy the unit test that checks the built command, `git.rs` L1748-1764 (`git_command_marks_every_redirecting_var_for_removal`, using `cmd.get_envs()`).

**Fail-closed split, "did it run" vs "what did it say": copy `agent_result.rs` L2858-2900** (`phase_commit_count`). An `Err` from `.output()` means it did not run. A non-zero exit means it ran and refused. A success whose stdout does not parse means nothing was established:
```rust
match git_command(project_root).args([...]).output() {
    Err(_) => return None,                               // could not execute
    Ok(output) if !output.status.success() => return Some(0),
    Ok(_) => {}
}
```
For GSD every one of these arms becomes a distinct `Err(ReaderFailure::…)`. Exit 0 is **not** success: `phase_dir_not_found` and `{"error":"Field not found"}` both exit 0 (RESEARCH §GSD Integration). Index the JSON and do not `.get().unwrap_or` (classify sketch in RESEARCH L588-609).

**Error type shape: copy `gsd_config.rs` L44-59**, a `thiserror` enum with `#[from]` Io/Json plus named variants carrying the path:
```rust
#[derive(Debug, thiserror::Error)]
pub enum GsdConfigError {
    #[error("GSD config I/O failed: {0}")] Io(#[from] std::io::Error),
    #[error("GSD config JSON failed: {0}")] Json(#[from] serde_json::Error),
    #[error("no GSD config at {0}")] Missing(PathBuf),
}
```
Also copy the module-doc style of `gsd_config.rs` L1-31: it lists what DevFlow does not own, then the properties each test pins.

**Phase-dir resolution: copy the prefix scan in `agent_result.rs` L3817-3833** (`phase_verification_path`, scanning `"{padded}-"` under `.planning/phases`). Return the **directory**, not the report, so `missing` is reachable. Callers pass the evidence root (see `handle_validate_outcome` below).

**Timeout:** no production one-shot subprocess-with-deadline helper exists in `crates/*/src`. I searched `rg 'try_wait|wait_timeout'`: the hits are tests or the supervisor runtime. Spawn, then poll `try_wait` against a deadline, and kill/reap on expiry. Make the expiry its own `ReaderFailure`.

**Stub-launcher unit tests: copy `agents/antigravity.rs` L170-215.** It writes a `#!/bin/sh` stub that logs `"$@"` to a file, `chmod 0o755`, runs it, then asserts the received argv equals the built argv:
```rust
let stub = format!(r#"#!/bin/sh
printf '%s\n' "$@" > '{}'
printf '%s\n' '{{"event":"result",...}}'
"#, argv_file.display());
std::fs::write(bin_dir.join("agy"), stub).unwrap();
std::fs::set_permissions(bin_dir.join("agy"), std::fs::Permissions::from_mode(0o755)).unwrap();
```
Feed the recorded gsd-core 1.20.1 JSON from RESEARCH L454-464 as fixtures, one test per failure class (D-24).

---

### `crates/devflow-cli/tests/common/mod.rs` (stub `gsd_run`, capture-shape agent)

**Analog: `fake_bin_dir_for` L110-124, `HermeticHome` L39-68.**
```rust
pub fn fake_bin_dir_for(program: &str, agent_body: &str) -> FakeBin {
    let dir = tempfile::tempdir().unwrap();
    let agent = dir.path().join(program);
    fs::write(&agent, format!("#!/bin/sh\n{agent_body}\n")).unwrap();
    let mut permissions = fs::metadata(&agent).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&agent, permissions).unwrap();
    for linked in ["git", "sh"] { symlink(format!("/usr/bin/{linked}"), dir.path().join(linked)).unwrap(); }
    FakeBin { path: dir.path().to_path_buf(), _dir: dir }
}
```
- Put the stub `gsd_run` at `home.home().join(".claude/gsd-core/bin/gsd_run")` (the child's `HOME` is the hermetic one, `hermetic_devflow` L80-94). Script a sequence (gaps_found, then passed) with a counter file in a logs tempdir, and log argv. The `CLAUDE_STUB` `@LOGS@` substitution in `failover_e2e.rs` L66-99 / L269-276 shows the logging shape.
- File header rule (L1-19): helpers only, no `#[test]`, and keep `#![allow(dead_code, reason = …)]`.
- The capture-shape agent replaces `--legacy-claude-launch`. No existing e2e drives a full stage with a codex fake. `phase7_cli.rs` L975-979 only starts with `#!/bin/sh\nexit 0`. Codex gets its prompt as argv (`agents/codex.rs` L26-41, `-a never exec --sandbox workspace-write --json …`), so a fake codex that logs `"$@"` covers the capture-shape note assertion.

---

### `crates/devflow-core/src/gates.rs` (abort field; GATE-01, D-13, D-28)

**Current schema, L52-63** (add `#[serde(default)] pub abort: bool` here):
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateResponse {
    pub approved: bool,
    #[serde(default)] pub note: Option<String>,
    #[serde(default)] pub responded_by: Option<String>,
}
```
**Watch:** there are about 66 `GateResponse {` struct literals (`rg -c`: gates.rs 23, pipeline_gate.rs 12, commands.rs 8, pipeline_launch.rs 7, recover.rs 5, outcomes 3, stop_e2e 3, test_support 2, start_lock_e2e 2, recover_clean_e2e 1). RESEARCH suggests `#[non_exhaustive]`. That attribute forbids struct literals **outside devflow-core**, so every devflow-cli and tests/ literal would need a constructor. Either add the field to every literal, or add `#[non_exhaustive]` together with constructors (`GateResponse::approve/reject/abort`). The planner picks one.

**Action mapping, L94-107** (delete the substring arm; read the field; a note-carrying LoopBack replaces `LoopBack(Stage)`, D-26):
```rust
pub fn from_response(response: &GateResponse) -> GateAction {
    if response.approved { return GateAction::Advance; }
    match response.note.as_deref() {
        Some(note) if note.to_ascii_lowercase().contains("abort") => GateAction::Abort(note.to_string()),
        _ => GateAction::LoopBack(Stage::Code),
    }
}
```
**Privileged writer with no boolean, L671-695** (`reap_answer`, T-23-41). It hard-codes `approved: false`. Set `abort: true` here and rewrite its doc L672-679. Use the **same shape** for the D-27 timer writer: a dedicated fn that hard-codes approve, takes no boolean, and refuses any gate that is not a rate-limit gate.

**Tests to copy/rewrite:** `gate_action_aborts_when_note_says_abort` L2469-2479 becomes the negative control ("do not abort, fix the test" must yield LoopBack). `reap_answer_writes_a_rejection_for_the_observed_gate_only` L3840-3867 shows the open → reap → `read_answer` → `from_response` round-trip to reuse for the abort field and the timer writer.

**Approve+abort refusal:** refuse at write (`gate_respond`) **and** at read. A hand-written file can bypass the CLI (RESEARCH §Security).

---

### `main.rs` `GateCmd::Abort` + `commands.rs::gate_respond` (controller)

**Copy `GateCmd::Reject` main.rs L476-501** without the positional `stage`/`legacy_project` args (cut in Wave 1). Keep `--stage`, `--note` (optional for abort), `--gate-id`, `--project`. Rewrite the Reject doc at L474-475 and L490-491 ("include abort to end the phase").

**Dispatch: copy main.rs L714-733.** `resolve_gate_target` collapses to `--stage` only after the cut.

**Writer: `commands.rs` L2241-2298** (`gate_respond`). It resolves the gate id, then calls `require_gate_file_names` and `refuse_a_gate_with_no_waiter`, then `Gates::answer`, emits the `gate_response_written` event, and prints the pickup line. Extend it with an abort input (or a sibling). The `gate_response_written` event payload (L2274-2283) should carry `"abort"`. The print at L2293-2297 uses `if approved {"approved"} else {"rejected"}` and needs a third word. D-10 says the reject print names the route it will run.

**Other consumers that match on the old substring/variants:** `stop_with_existing_response` commands.rs L4150-4169, the sweep note at L2741-2747, and the stop note at L4058-4064.

**doc_check:** `GateCmd` variants are checked docs→source only (RESEARCH §doc_check). Document `devflow gate abort` in OPERATIONS.md "Answering gates"; no allowlist entry is needed.

**e2e that breaks:** `gate_wedge_e2e.rs` L100-116 `reject()` passes positional `"code"` and `--note "abort: #200 e2e"`. It loses both the positional and the substring meaning, so switch it to `gate abort <phase> --stage code`. The same applies to `reject_define` L228-243, which dies with Define.

---

### `crates/devflow-core/src/prompt.rs` (note carrier; D-10, D-14, D-26, D-32)

- `StageIntent::Code { phase, fix: Option<FixType> }` at L172-175 is where the note joins (or add a quick-fix intent).
- `FixType` at L248-262 is `#[non_exhaustive]` and `snake_case`-serialized into `State.code_fix`. Add the `Quick`/stale-reverify arms here.
- **Both renderers** must carry the note:
  - `fix_prompt` L713-741 (claude style; today `fix_prompt(fix_type, phase)` has no note);
  - `workflow_code_prompt` L558-585 (codex/pi).

  Both pick the instruction by `match fix`, then append `CODE_STAGE_POLICY`/`COMPLETION_PROTOCOL` via `code_policy_applies_to_fix_arm`.
- Gaps Plan: `workflow_plan_prompt` L521-539 and the claude idempotent Plan prompt short-circuit when PLAN.md exists (Pitfall 5). The gaps Plan needs its own intent.
- **Test pattern, L1238-1251:** assert the right command is present **and** the wrong one absent (`!full_execute_prompt.contains("--gaps-only")`). For the note, assert byte-identity of the printable text inside the delimited block, with a control-char case. Snapshot pattern: `claude_style_full_execute_fix_prompt_snapshot` L1519-1525 (`insta::assert_snapshot!`); per-adapter baselines at L1548.

### `crates/devflow-core/src/state.rs` (persisted note, used-flag, fix base)

**Copy the `fallback_consumed` doc and serde shape, L483-491.** It is a one-shot durable flag, `#[serde(default)] pub …: bool`. Its doc states what `false` means for old state ("not used" OR "older binary"). The note persists beside `code_fix` L493-507 (`#[serde(default, skip_serializing_if = "Option::is_none")]`), so a failover/resume replay keeps it (Phase 51 D-09).

**Set-in-the-same-write pattern: `pipeline_launch.rs` L3217-3228.** The flag flips inside the same `commit(state, |s| …)` that changes the run's agent, so no second launch can see it unset:
```rust
commit(state, |s| {
    s.agent = to;
    if reason == HandoffReason::RateLimited { s.fallback_consumed = true; }
    s.canary = None; s.session_id = None;
})?;
```
Under D-27.3, `rate_limit_auto_relaunch_used = true` is committed **before** the timer writes its approve.

---

### Rate-limit wait-and-relaunch gate (CUT-03; D-15, D-16, D-27)

**Replace `pipeline_outcomes.rs` L80-148** (`handle_rate_limited_outcome`: infra bump L96-98, cron write L103-104, `rate_limit_resume_scheduled` event L136-145). Per D-20 there is no infra bump. Its dispatch is `pipeline_launch.rs` L4211-4236 (`RateLimited` → `failover_target` → `fail_over_on_rate_limit` or this handler; the `_` arm is unreachable).

**Gate + pre-authorized answer: `pipeline_gate.rs::run_gate_with_timeout` L521-537 (signature), L590-606 (gate_fired / notify / `notify_fired`), L607-628 (auto_response write):**
```rust
if let Some(response) = auto_response {
    let answered = fenced(project_root, state.phase, || {
        match Gates::answer(project_root, state.phase, stage, gate_id, response) {
            Ok(_) => Ok(()),
            Err(GateError::AlreadyResponded { .. }) => Ok(()),   // first-writer-wins
            Err(err @ GateError::PublishedUnconfirmed { .. }) => { warn!("{err}"); Ok(()) }
            Err(err) => Err(err.into()),
        }
    });
    ...
}
let wait_end = match gate_wait_call(project_root, state.phase, stage, gate_id, timeout_secs) { ... };
```
The timer differs from `auto_response` in one way: it writes **after** a first bounded wait (`timeout = resetsAt + margin − now`) ends `TimedOut`, then waits again. The single `Gates::claim` at L640-672 arbitrates timer vs human. `GateWaitEnd::{Answered, Stopped, TimedOut}` is handled at L638-733, and `TimedOut` currently emits `gate_timeout` and errors. The wait bound is `max(gate_timeout_secs(), reset+margin−now)` (D-27.2); `gate_timeout_secs` is in `config_parse.rs` L42.

**Approve-to-relaunch:** `handle_stage_failure` L948-975 shows Advance → `close_gate` → `relaunch_failed_stage` (L985) → `launch_stage(state, None, Some(stage))`. D-27.4 asks for a distinct action so `Advance` is not read as "advance stage".

**Way-on sentence first: copy `failure_gate_context` L932-945.** The lead names `devflow resume --phase N --agent …` before the cause, because notify and the banner show only the start. The resume command is presented as "when the waiter is gone" (D-27.5; refusal at `commands.rs` L4687 `refuse_launch_over_a_live_run`).

**Sweep skip: `commands.rs` L2684-2686.**
```rust
fn gate_sweep_may_reap(_stage: Stage, waiter_live: bool) -> bool { waiter_live }
```
Add a not-before input read from the gate file. `GateFile` is at gates.rs L37-49; it is not `#[non_exhaustive]`, so the change is semver. Its e2e analog is `tests/gate_sweep_e2e.rs`.

**Reset-time parsing: relocate `ship.rs` L307-490** (`parse_retry_timestamp` L357, `parse_unix_seconds` L361, `parse_rfc3339ish` L367, `split_time_and_offset` L412, `parse_offset_minutes` L433, `days_from_civil` L462, `civil_from_days` L472) with tests L836-903.
- **Caveat:** `parse_unix_seconds` rounds **up to the minute**, and `RetryTimestamp` has no epoch-seconds accessor. Add one for the deadline, plus a test.
- `civil_from_days` is also used by kept `supervisor.rs` L509 (`rfc3339_from_unix`). Move it first.
- `retry_after_from_reason` (`parallel.rs` L165-170, test L280-287) strips `"rate limited until "`, the format written by `agent_result.rs` L2786-2790 (`rate_limited_result`).

**e2e: rewrite `failover_e2e.rs` L838-891.** It currently asserts the cron file. Reuse `Rig` (L235-415: `start`, `wait_for_any`, `events_named`, `invocations`, `state`) and the `CLAUDE_STUB` template L66-99. The `resetsAt` fixture const at L45 is `1785645600` (already past), so it relaunches at once. Build computed past and future values with `.replace`, the way the `@CLAUDE_RATE_LIMIT@` substitution does at L275. Delete `Rig::cron_file` L410-412.

---

### `pipeline_gate.rs::finish_run` (CUT-02; D-04, D-06, D-29, D-35)

**Copy `transition`'s `--until` halt branch, L73-97.** It keeps state, marks it stopped with a reason, clears the supervisor and gate_pending, and emits `workflow_finished`:
```rust
commit(state, |s| {
    s.stopped = true;
    s.stop_reason = Some(format!("stopped after {from} completed (--until {from})"));
    s.supervisor = None; s.gate_pending = false; s.code_fix = None;
})?;
events::emit(project_root, state.phase, "workflow_finished",
    serde_json::json!({ "reason": "stopped_at", "stage": from.to_string() }));
```
- The terminal payload is `{"reason":"finished","shipped":false}`.
- Fire notify for both sites with `gates::fire_gate_notify_with_fallback(phase, stage, ctx, unexpected, state.gate_notify_cmd.as_deref())`, then emit `notify_fired`, as at L595-606.
- Keep `registry::deregister(project_root, state.phase)` (L359).
- Do **not** copy `clear_run_state` (L355). D-35 keeps the state.
- Delete `finish_workflow*` L238-388 together with the cron consume at L360-379.
- `notify` passes context unbounded (`gates.rs` L1546-1552). Bound it there when the write-time 300 cap moves (Pitfall 6).

### `pipeline_outcomes.rs::handle_validate_outcome` (GATE-02/03; D-08, D-24, D-36)

- Evidence root binding L538-541: `state.worktree_path.clone().unwrap_or_else(|| project_root.to_path_buf())`. Pass it to the GSD reader, never `project_root`.
- Three `transition(…, Stage::Ship)` sites become `finish_run`: L556 (ambiguous approve), L744 (gate approve), L762 (auto pass).
- Gate text L697 `"Validation passed — approve to ship?"` gets "finish" wording plus the GSD status.
- Reader failure → `run_gate` with a `[never-silent]` context, the same shape as the ambiguous arm L551-555.
- Write-time cap: `truncate_reason` L888-890 → `render_gate_context(reason, 300)` (L893-914); `stage_failure_context` L920-926 puts the way-on sentence **last**. GATE-04 reorders it and stores the full text.

### `mode.rs` streak (D-25, D-34)

- `transition_resets_consecutive_failures` L142-144: `!matches!((from, to), (Stage::Code, Stage::Validate))`. Applied at `pipeline_gate.rs` L109-111.
- `consecutive_failures_made_progress` L180-181: `previous.is_none_or(|p| current > p)`. Fed by `phase_commit_count` at `pipeline_outcomes.rs` ~L604-624.
- For D-34, write a sibling of `phase_commit_count` (agent_result.rs L2858) that counts commits touching paths outside `.planning/` (pathspec exclude on `rev-list --count`), with the same `Option` semantics (`None` = not measured).

---

### `doc_check.rs::no_removed_surface_literal_remains` (NEW; source-scan test, CUT-04)

**Analog: `no_monitor_pid_field_json_key_or_help_mention_remains` L597-671.**
- Forbidden token assembled so the file cannot match itself (L506): `const REMOVED_STATE_FIELD: &str = concat!("monitor", "_pid");`. Use a `&[&str]` of `concat!` tokens.
- Comment stripping: `uncommented_lines` L510-516 (line-prefix `//`); `strip_rust_comments` L1233 (full); `strip_html_comments` L673-687 for Markdown.
- Word-boundary helper: `is_identifier_char` L517 and `declares_pub_field` L523-532, which shows the "not a longer name" check.
- Findings builder `removed_field_findings` L549-578 returns `Vec<String>` labelled `path:line: kind: text`. File loading: `read_labelled` L580-589.
- Planted controls first, in the same test: flagged line, longer-name line, `//` and `///` comment lines (L604-641). Then the real scan (L644-670) with non-empty asserts on the walkers (`assert!(!rust_files.is_empty(), …)`).
- Walkers: `rust_file_paths` L86, `SCOPED_DOCS` L8-13, `scoped_doc_paths` L55, `workspace_root` L244. Exclude `CHANGELOG.md`.
- `doc_check.rs` L6 imports `lock, ship`. Those imports go with the modules, and the fixtures at L323-331 and L784-842 get rewritten (D-30).

**Related source scan that the cut breaks:** `tests/hermetic_spawn_sites.rs` L18-24 `HERMETIC_SUITES` lists `yes_ship_config.rs`, `release_check.rs` and `reap_strays_e2e.rs`. `read_suite` L73-77 **panics** on a missing file, so remove each entry in the plan that deletes the file.

---

### e2e suites (supervisor / failover / gate wedge)

- **`supervisor_e2e.rs` `Shape`** L77-99 maps `Shape::Capture => &["--legacy-claude-launch"]` (L96). Re-point it to a capture-shape agent (`--agent codex` + `fake_bin_dir_for("codex", …)`). Keep `capture_supervisor_owns_a_fake_agent_and_records_devflow_result` (L423-487) as the shape witness. It uses `--until define` at L439, which changes to `plan`, and it asserts on `stdout_path`/`exit_code_path` plus the `advance_evaluated` event.
- **Start helper:** `run_start_with` L113-140 sets PATH prepend, `hermetic_devflow`, `--mode supervise --no-worktree`, and `.args(shape.start_flags())`.
- **Gated foreground advance:** `gate_wedge_e2e.rs` L68-98 (`spawn_gated_advance`) saves a `State` at `Stage::Code`, spawns `advance` with `DEVFLOW_GATE_TIMEOUT_SECS=15`, and waits for gate file + `WaiterStatus::Live`. Use this rig for the "note with abort keeps the run alive" e2e and for note-verbatim (the fake agent logs stdin/argv). L350-384 shows answering a live waiter and timing its pickup.
- **Failover rig:** see the rate-limit section. Count relaunches with `rig.invocations("claude")` (L389-396).
- Every spawn goes through `common::hermetic_devflow`; `test_home_isolation.rs` and `hermetic_spawn_sites.rs` scan for raw spawns.

### `build.rs` / `main.rs` `--version` (OPS-01)

`build.rs` L50-69 emits `cargo:rustc-env=DEVFLOW_BUILD_COMMIT` (empty on no git, D-20) and `DEVFLOW_BUILD_DIRTY`. `main.rs` L36-40 has `#[command(name = "devflow", version, …)]`. The existing readers are `commands.rs` L812-813 and L1154-1155. Test analog: `tests/build_provenance.rs` L13-30 (`build_dirty_is_exactly_true_or_false`, `build_commit_is_empty_or_a_full_hex_sha`). Never compare binary sha256 (CLAUDE.md).

## Shared Patterns

### State writes
**Source:** `commit(state, |s| …)?` (e.g. `pipeline_gate.rs` L74-87, L107-117). Every state mutation is one closure that re-reads disk, so concurrent edits (stop, notify cmd) survive. A flag that must gate a launch is set in the same commit as the launch decision.

### Events and never-silent gates
**Source:** `events::emit(project_root, phase, "name", json!({…}))`; gate context prefixed `[never-silent]` (`pipeline_outcomes.rs` L551-553, L920-926). Never put GSD's `message` (it holds absolute paths) into `events.jsonl`; stderr and gate context only (Pitfall 11; `staleness.rs` L474-483).

### Subprocess hermeticity
**Source:** `git.rs::hermetic_command` L87-94 for production; `common::hermetic_devflow` L80-94 for e2e children. Use argv, never `sh -c`, except the notify hook (`gates.rs` L1546), which passes data as env.

### Test gates
One named test per command: `cargo nextest run -p <devflow-core|devflow> {--lib|--bin devflow|--test <file>} --no-tests=fail -E 'test(=module::path::name)'`. Each test carries a planted control that must fail (doc_check L604-641 and prompt L1238-1251 show the present+absent habit).

### Semver
`pub` changes in `devflow-core` (`GateResponse`, `GateAction`, `GateFile`, `FixType` arms, `State` fields, `Stage` variants, `StageIntent`) are covered by the 4.0.0 bump. Run `cargo semver-checks check-release -p devflow-core` after the bump (Pitfall 9).

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| bounded-timeout one-shot subprocess inside `gsd_reader` | utility | request-response | No production deadline-bounded `Command` helper in `crates/*/src`; build spawn + `try_wait` poll + kill |
| release preflight port (D-02/D-31) | tooling | batch | No unpublished workspace tool crate exists. Partial analogs: `tests/release_check.rs` fixture builder L71-83 + e2e asserts L86-120 (to port), `tests/cut_release_guard.rs` (static source guards on `scripts/cut-release.sh`, which calls `run_devflow release --check` at L88 and `--verify` at L335). The planner states script vs tool and why |

## Metadata

**Analog search scope:** `crates/devflow-core/src/**`, `crates/devflow-cli/src/**`, `crates/devflow-cli/tests/**`, `crates/devflow-cli/build.rs`, `scripts/cut-release.sh`
**Files scanned:** ~30 (targeted reads; no builds or tests run)
**What this does not establish:** that the excerpts compile after the cut; whether line numbers survive the Wave 1 deletions (re-anchor by symbol name after each subtraction plan).
