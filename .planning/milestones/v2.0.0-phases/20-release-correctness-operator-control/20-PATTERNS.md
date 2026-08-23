# Phase 20: Release Correctness + Operator Control - Pattern Map

**Mapped:** 2026-07-22
**Files analyzed:** 8 (all extended-in-place; no net-new files except one likely new test file for 20d)
**Analogs found:** 8 / 8 — every unit extends code with a direct, already-tested sibling in the same file or an adjacent one.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/devflow-core/src/version.rs` (`write_version`, `replace_version_in_contents`) — 20a | utility (file-mutation) | transform | itself — extend `write_version`'s existing single-field rewrite | exact (self-extension) |
| `crates/devflow-cli/tests/workspace_version_pin.rs` — 20a (stays passing, no edits expected) | test | CRUD (assertion) | itself — existing RED-proven guard | exact |
| `crates/devflow-cli/src/commands.rs::cleanup` (~292–335) — 20b | controller (CLI command handler) | request-response / file-I/O | `commands.rs::liveness()` (`:371`) + `check_dead_monitor` (`:1270`) | exact (same file, existing predicate to reuse) |
| `crates/devflow-cli/tests/phase7_cli.rs` (fixture durability, both instances) — 20b | test | file-I/O (git fixture) | itself — existing flaking tests, stabilize in place | exact |
| `crates/devflow-cli/src/main.rs::Command::Start` (`--until` flag) — 20c | CLI arg surface | request-response | `Command::Start`'s existing flag block (`:46–73`) | exact |
| `crates/devflow-cli/src/pipeline_gate.rs::transition` (`:51–80`) — 20c | controller (state-machine funnel) | event-driven | itself — existing `transition` function, add stop-check branch | exact |
| `crates/devflow-core/src/state.rs::State` (new `stopped`/`stop_reason` field) — 20c | model | CRUD (persisted state) | `monitor_pid`/`preflight_retries` field additions (`:44–72`) | exact |
| `crates/devflow-cli/src/commands.rs::check_dead_agent`/`reconcile_phase` (new stop-aware branch) — 20c | service (reconciliation check) | transform (pure, no I/O) | `check_dead_agent` (`:1247`), `check_dead_monitor` (`:1270`) | exact |
| `crates/devflow-cli/tests/help_snapshot.rs` (regenerate) — 20c/20e | test | file-I/O (snapshot diff) | itself — existing snapshot test | exact |
| `crates/devflow-cli/src/main.rs::Command::Release{Check}` (new subcommand) — 20d | route/CLI dispatch | request-response | `Command::Doctor` (`:220–227`, `main.rs:426`) | role-match (both read-only audit commands) |
| `crates/devflow-cli/src/commands.rs::release_check` (new fn) — 20d | controller (read-only preflight) | request-response | `commands.rs::doctor` (`:975`) + its `Check`/`cmd_check` helpers (`:962–1026`) | exact |
| `crates/devflow-core/src/git.rs` or new small module (ancestor-check/signing helpers) — 20d | service | request-response (shell-out) | `scripts/sync-main-to-develop.sh:41` (`git merge-base --is-ancestor`) + existing `version.rs` `Command::new("git")` shell-out pattern | role-match |
| `crates/devflow-cli/tests/release_check.rs` (new file) — 20d | test | request-response | `crates/devflow-cli/tests/workspace_version_pin.rs` (structure/fixture style) | role-match |
| `crates/devflow-cli/src/main.rs::Command::Ship{phase,force}` (new subcommand) — 20e | route/CLI dispatch | request-response | `Command::Gate{action: GateCmd}` (`:99–104`) + `GateCmd::Approve` (`:239–259`) | exact |
| `crates/devflow-cli/src/pipeline_gate.rs::ship_override` (new fn) or `commands.rs` handler — 20e | controller | event-driven (second consumer of on-disk record) | `pipeline_gate::finish_workflow` (`:130–164`) + `gates::Gates::respond`/`response_path` (`gates.rs:127,179–198`) | exact |
| test for 20e (new, in `pipeline_gate.rs`'s test module) | test | event-driven | `advance_ship_success_runs_finish_workflow` (`pipeline_gate.rs:309`) | exact |

## Pattern Assignments

### `crates/devflow-core/src/version.rs` (utility, transform) — 20a

**Analog:** itself, `write_version` (lines 196–206) + `replace_version_in_contents` (lines 253–onward) + `field_for` (58–72)

**Core pattern to extend** (lines 196–206):
```rust
pub fn write_version(project_root: &Path, version: &Version) -> Result<PathBuf, VersionError> {
    let path = detect_version_file(project_root)
        .ok_or_else(|| VersionError::Parse("no version file found".into()))?;
    let contents = std::fs::read_to_string(&path)?;
    let field = field_for(&path, &contents);
    let replaced = replace_version_in_contents(&contents, field, &version.to_string())
        .ok_or_else(|| VersionError::Parse(format!("field `{field}` not found")))?;
    std::fs::write(&path, replaced)?;
    Ok(path)
}
```

**Section-scan helpers to extend, not replace** (lines 208–251 — `split_field`, `parse_section_header`, `find_version_in_contents`):
```rust
fn find_version_in_contents(contents: &str, field: &str) -> Option<String> {
    let (section, key) = split_field(field);
    let mut current = "";
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(header) = parse_section_header(trimmed) {
            current = header;
            continue;
        }
        if current != section { continue; }
        if let Some((lhs, value)) = trimmed.split_once(['=', ':']) {
            let lhs_key = lhs.trim().trim_matches('"').trim_matches('\'');
            if lhs_key != key { continue; }
            let value = value.trim();
            if value.starts_with('{') { continue; }   // <-- inline tables (self-pins) are SKIPPED today
            return Some(value.trim_matches(['"', '\'']).to_string());
        }
    }
    None
}
```
**Concrete fix shape:** the `if value.starts_with('{') { continue; }` guard is exactly why `[workspace.dependencies] devflow-core = { path = "...", version = "..." }` is invisible to the existing scan — it treats inline tables as noise. The extension needs a **second, additive pass** (not a modification of this guard, which single-field callers still need) that: (1) enters the `[workspace.dependencies]` section, (2) for each line matching `<crate> = { ... path = "crates/..." ... version = "..." }`, rewrites only the `version = "..."` sub-value inside that inline table, leaving `path` and any other keys untouched. Reuse `parse_section_header`/line-iteration verbatim; add a small inline-table key=value extractor rather than a general TOML parser (Don't Hand-Roll: keep hand-rolled per GAP-6 comment/quote-preservation tests).

**Existing regression tests this must not break** (GAP-6 family — search `write_version_preserves_trailing_comment_in_toml`, `_in_single_quoted_toml`, `_trailing_comma_in_package_json` in `version.rs`'s test module): any new inline-table rewrite path must preserve trailing comments/quote style the same way the single-field path does.

**Guard test to satisfy without editing:** `crates/devflow-cli/tests/workspace_version_pin.rs` (PR #17) — asserts every `[workspace.dependencies]` path-dependency pin equals `[workspace.package].version`. Do not weaken/delete it.

---

### `crates/devflow-cli/src/commands.rs::cleanup` (controller, request-response) — 20b instance 1

**Analog:** `liveness()` (lines 368–380) and `check_dead_monitor` (lines 1270+), same file

**Current unguarded core pattern** (lines 292–335, the exact removal loop needing a liveness check inserted before line 308's `worktree::remove` call):
```rust
pub(crate) fn cleanup(project_root: &Path, force: bool) -> Result<(), CliError> {
    let git = GitFlow::new(project_root);
    let worktrees_dir = worktree::worktrees_dir(project_root);
    let reference = worktree::reference_path(project_root);
    let worktrees = worktree::list(project_root)?;
    let mut removed = 0usize;
    for wt in &worktrees {
        if !wt.path.starts_with(&worktrees_dir) { continue; }
        if wt.path == reference && !force {
            println!("keeping reference worktree (use --force to remove it)");
            continue;
        }
        worktree::remove(project_root, &wt.path, force)?;   // <-- INSERT liveness() gate before this call
        ...
    }
    ...
}
```

**Liveness predicate to reuse, not re-derive** (lines 368–380):
```rust
/// Pure liveness predicate — no I/O. `monitor_pid` is matched `None` first
/// so a state written by a pre-18b binary (carrying no `monitor_pid`) can
/// never be misclassified as `Stuck` (T-18-11).
fn liveness(monitor_pid: Option<u32>, monitor_alive: bool, agent_alive: bool) -> Liveness {
    match monitor_pid {
        None => Liveness::Unknown,
        Some(_) => match (monitor_alive, agent_alive) {
            (true, true) => Liveness::Healthy,
            (true, false) => Liveness::BetweenStages,
            (false, _) => Liveness::Stuck,
        },
    }
}
```
**Enum to branch on** (`Liveness`, lines 340–366): `Healthy`/`BetweenStages` → hard-refuse (per D-06, no override flag — `cleanup --force`'s existing meaning at line 304 is "also remove reference worktree," do not overload). `Stuck`/`Unknown` → proceed, with bounded-backoff retry around the existing `worktree::remove` call at line 308 (retry shape has no in-repo analog yet; write a small local backoff loop, e.g. 3 attempts with short sleeps, matching the "Instance 2 addressed with stronger fsync settings" style of local, contained fix — no new dependency).

**Error-shape analog for the hard-refuse message:** mirror `check_dead_agent`'s `PhaseFinding.repair` pattern (`"devflow resume --phase {}"`, line 1259) — i.e. the refuse error text should name the concrete unblocking action (`devflow resume`/wait), not just say "no").

---

### `crates/devflow-cli/tests/phase7_cli.rs` (test, file-I/O) — 20b instances 1 & 2

**Analog:** itself. No code-pattern excerpt needed beyond fixture durability knobs — apply `core.fsyncObjectFiles=true`/`core.fsync=all` via `git config` in the test's repo-init helper, and/or shrink the 60-commit loop's window at line 246 where it isn't needed to cross the `>50` threshold. New test for instance 1's liveness guard: construct a `State` with `monitor_pid` set to a still-running pid before invoking `cleanup --force`, assert refusal + exit code, following the existing CLI-invocation harness style already used at `phase7_cli.rs:534`.

---

### `crates/devflow-cli/src/main.rs::Command::Start` (CLI arg surface, request-response) — 20c

**Analog:** the existing flag block on `Start` itself (lines 46–73):
```rust
Start {
    #[arg(long)] phase: u32,
    #[arg(long, default_value = "claude")] agent: AgentKind,
    #[arg(long)] mode: Mode,
    #[arg(long)] force: bool,
    #[arg(long, hide = true)] worktree: bool,
    #[arg(long)] no_worktree: bool,
    #[arg(long)] dry_run: bool,
    #[arg(default_value = ".")] project: PathBuf,
},
```
**Add:** `#[arg(long)] until: Option<Stage>` — `Stage` already implements `FromStr`/clap-parseable (per `GateCmd::Approve`'s `stage_option: Option<Stage>` at line 252, the exact same optional-enum-flag idiom). Reject `--until ship` explicitly at the dispatch site (D-07) — no analog needed, a simple `if until == Some(Stage::Ship) { return Err(...) }` guard.

---

### `crates/devflow-cli/src/pipeline_gate.rs::transition` (controller, event-driven) — 20c

**Analog:** itself, full function (lines 51–80):
```rust
pub(crate) fn transition(
    project_root: &Path,
    state: &mut State,
    to: Stage,
) -> Result<(), CliError> {
    let from = state.stage;
    let _ = run_checkout_hooks(project_root, state, &hooks::hooks_for_transition(from, to), to);
    state.stage = to;
    if mode::transition_resets_consecutive_failures(from, to) { state.consecutive_failures = 0; }
    state.infra_failures = 0;
    state.gate_pending = false;
    workflow::save_state(state)?;
    events::emit(project_root, state.phase, "transition",
        serde_json::json!({ "from": from.to_string(), "to": to.to_string() }));
    launch_stage(state, None, Some(from))
}
```
**Insertion point:** after `state.stage = to;` and before `launch_stage(...)` — if `state.stop_until == Some(to)` (or `from` depending on exact semantics chosen), set `state.stopped = true` / `state.stop_reason = Some(...)`, persist via the same `workflow::save_state(state)?` call already present, emit a `workflow_finished` event mirroring `finish_workflow`'s terminal emission below (do NOT call `launch_stage`). **Do not touch** `loop_back_to_code` (lines 84+) — it calls `launch_stage` directly, bypassing `transition`, and must stay untouched per D-07/Pattern 3.

**Terminal-emission analog to copy the shape of** (`finish_workflow`, lines ~130–160):
```rust
pub(crate) fn finish_workflow(project_root: &Path, state: &mut State) -> Result<(), CliError> {
    loop {
        if run_checkout_hooks(project_root, state, &hooks::hooks_after_ship(), Stage::Ship) { break; }
        ...
    }
    let _ = Gates::cleanup(project_root, state.phase, Stage::Validate);
    let _ = Gates::cleanup(project_root, state.phase, Stage::Ship);
    workflow::clear_state(project_root, state.phase)?;
    events::emit(project_root, state.phase, "workflow_finished", serde_json::Value::Null);
    println!("phase {} shipped — workflow complete", state.phase);
    Ok(())
}
```
For 20c's stop path: emit `workflow_finished` with a reason payload (e.g. `serde_json::json!({"reason": "stopped_at", "stage": to.to_string()})`) instead of `Value::Null`, and do **not** call `workflow::clear_state` (that would lose the stop-marker per D-09 — only `finish_workflow`'s true Ship-terminal path clears state).

---

### `crates/devflow-core/src/state.rs::State` (model, CRUD) — 20c new field

**Analog:** the `monitor_pid`/`preflight_retries` addition pattern (lines 44–72), exact `#[serde(default)]` idiom every prior field addition uses:
```rust
/// PID of the detached monitor process ... `None` means no monitor has been
/// spawned for this state yet, OR the state was written by a binary
/// predating this field ...
#[serde(default)]
pub monitor_pid: Option<u32>,
```
**New fields to add, following this exact shape:**
```rust
#[serde(default)]
pub stopped: bool,
#[serde(default)]
pub stop_reason: Option<String>,
```
Also update `State::new` (lines 118–133) to initialize both to `false`/`None`, mirroring how `monitor_pid: None` and `preflight_retries: 0` are initialized there today. Add round-trip serde tests mirroring `infra_failures_round_trips_through_serde` (state.rs:226) and `infra_failures_absent_from_json_defaults_to_zero` (state.rs:244) for backward-compat proof.

---

### `crates/devflow-cli/src/commands.rs::check_dead_agent`/`reconcile_phase` (service, transform) — 20c doctor gap

**Analog:** `check_dead_agent` itself (lines 1247–1261):
```rust
fn check_dead_agent(facts: &PhaseFacts) -> Option<PhaseFinding> {
    let pid = facts.agent_pid?;
    if facts.agent_alive || !facts.stage.is_agent_stage() { return None; }
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Problem,
        detail: format!("phase {}: agent pid {pid} recorded but not running at stage {}", facts.phase, facts.stage),
        repair: Some(format!("devflow resume --phase {}", facts.phase)),
    })
}
```
**Fix shape:** add a `facts.stopped: bool` (or equivalent) field to `PhaseFacts` (struct at lines 1175–1193, same additive style as `monitor_pid` there), populated from `state.stopped` in `collect_phase_facts` (uncited helper that builds `PhaseFacts` — locate and extend it the same way `monitor_pid`/`monitor_alive` are already populated). Then guard `check_dead_agent`'s early return: `if facts.stopped || facts.agent_alive || !facts.stage.is_agent_stage() { return None; }` — this is the single-line fix that closes the doctor false-positive gap (Pitfall 2). Write a regression test in the `reconcile_phase` test module (pattern: `reconcile_phase_flags_dead_agent_at_agent_stage`, line 2039) asserting **zero** `Problem` findings for a `stopped: true` phase with a dead agent pid at Plan.

---

### `crates/devflow-cli/src/commands.rs::doctor` (controller, request-response) — 20d analog

**Analog:** `doctor` itself (lines 975+) + its `Check`/`cmd_check`/`bool_check` helpers (962–1026):
```rust
pub(crate) struct Check {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) version: Option<String>,
    pub(crate) install_hint: Option<String>,
}

pub(crate) fn doctor(project_root: &Path, json: bool) -> Result<(), CliError> {
    fn cmd_check(name: &str, cmd: &str, version_arg: &str, install_hint: &str) -> Check {
        match Command::new(cmd).arg(version_arg).output() {
            Ok(out) if out.status.success() => { /* ok */ }
            Ok(out) => { /* warn, with install_hint */ }
            Err(_) => { /* missing, with install_hint */ }
        }
    }
    fn bool_check(name: &str, ok: bool, version: &str, install_hint: &str) -> Check { ... }
    ...
}
```
**20d's `release_check` should follow this exact `Check`-list-then-report shape**: each of the four checks (self-pin, develop/main ancestor, publish order, signing viability) becomes a `Check`-producing closure/function returning `ok`/`warn`/`missing`-equivalent status + an actionable `install_hint`-equivalent message, aggregated and printed/JSON-emitted the same way `doctor` does. Dispatch analog: `Command::Doctor { json, project }` → `main.rs:426` `Command::Doctor { json, project } => doctor(&project_root(project)?, json)`; mirror for `Command::Release { check: bool, project }` (or a `ReleaseCmd` subcommand enum if more shapes are added later, mirroring `Command::Gate { action: GateCmd }`'s nesting style at lines 99–104 if `release` grows beyond one verb).

**Ancestor-check shell-out to reuse verbatim, not reimplement:**
```bash
# Source: scripts/sync-main-to-develop.sh:41 (verified at HEAD)
git fetch origin main develop --quiet
git merge-base --is-ancestor origin/main HEAD   # exit 0 = already an ancestor (no-op)
```
Shell it out via `std::process::Command::new("git")` exactly as `version.rs`'s `count_git_tags`/`commits_since_last_minor_tag` already do (lines 90–138) — same `.current_dir(project_root).output()` + `output.status.success()` idiom.

**Signing-viability check (`gpg.format`-aware, Pattern 4):** no direct in-repo analog for `ssh-add`/`gpg-connect-agent` shell-outs — model it on the same `Command::new(...).output()` + exit-code branching idiom `doctor`'s `cmd_check` already uses (lines 978–1017), branching on `ssh-add -l`'s three exit codes (2/1/0) as three distinct `Check` statuses.

---

### `crates/devflow-cli/src/main.rs::Command::Gate`/`GateCmd::Approve` (route, request-response) — 20e analog

**Analog:** `GateCmd::Approve` (lines 238–259):
```rust
Approve {
    phase: u32,
    #[arg(value_name = "STAGE_OR_PROJECT")] stage: Option<String>,
    #[arg(value_name = "PROJECT")] legacy_project: Option<PathBuf>,
    #[arg(long = "stage")] stage_option: Option<Stage>,
    #[arg(long)] note: Option<String>,
    #[arg(long, default_value = ".")] project: PathBuf,
},
```
**New `Command::Ship` shape, simpler (no legacy positional baggage needed since this is new):**
```rust
Ship {
    #[arg(long)] phase: u32,
    #[arg(long)] force: bool,
    #[arg(default_value = ".")] project: PathBuf,
},
```
Dispatch mirrors `GateCmd::Approve => { ... }` at `main.rs:361`.

---

### `crates/devflow-cli/src/pipeline_gate.rs` (controller, event-driven) — 20e core pattern

**Analog:** `finish_workflow` (lines ~130–164, D-01's exact reuse target) + the gate-response read protocol from `gates.rs`:
```rust
// gates.rs:127 — response_path
pub fn response_path(project_root: &Path, phase: u32, stage: Stage) -> PathBuf { ... }

// gates.rs:179-198 — respond() writes unconditionally, no "is anyone listening" check
pub fn respond(project_root: &Path, phase: u32, stage: Stage, response: &GateResponse)
    -> Result<PathBuf, GateError> {
    if !Self::gate_path(project_root, phase, stage).exists() {
        return Err(GateError::NoOpenGate { phase, stage });
    }
    let path = Self::response_path(project_root, phase, stage);
    if path.exists() { return Err(GateError::AlreadyResponded { phase, stage }); }
    write_atomic(&path, &serde_json::to_string_pretty(response)?)?;
    Ok(path)
}
```
**20e's new function** (e.g. `pipeline_gate::ship_override` or a `commands.rs` handler) must:
1. `workflow::load_state(project_root, phase)` — error if `state.stage != Stage::Ship` (D-02, hard requirement, no `--force` bypass of this check).
2. Check `Gates::response_path(project_root, phase, Stage::Ship).exists()` — if absent, error directing the operator to resolve Ship's gate first (no gate written yet = nothing to consume).
3. Parse the `GateResponse`, convert via `GateAction::from_response(&response)`.
4. On `GateAction::Advance` → call `finish_workflow(project_root, &mut state)` directly — the exact same function `run_gate`'s in-process `Advance` branch calls via `handle_ship_outcome` (`pipeline_outcomes.rs:282`). Do not reimplement any part of `finish_workflow`'s hook batch/fail-closed retry-gate-reopen logic.
5. On `GateAction::LoopBack`/`Abort` → same handling `run_gate`'s callers already do (`loop_back_to_code(project_root, state, FixType::AuditFix)` / `abort(project_root, state, &reason)`), for symmetry — do not special-case Ship's LoopBack/Abort paths differently from the live-monitor path.

**Test analog to mirror exactly:** `advance_ship_success_runs_finish_workflow` (`pipeline_gate.rs:309`):
```rust
fn advance_ship_success_runs_finish_workflow() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = 21;
    let branch = format!("feature/phase-{phase:02}");
    // ... create branch, State::new(...), state.stage = Stage::Ship, workflow::save_state(&state)
    // seed DEVFLOW_RESULT success marker via agent_result::stdout_path
    // then assert finish_workflow (or the new ship-override path) completes and clears state
}
```
20e's own test should set up the identical fixture but write a `GateResponse` via `Gates::respond` (not by directly calling `finish_workflow`) and drive it through the new command handler, asserting `workflow_finished` is emitted and state is cleared — proving the second-consumer path reaches the same terminal state as the first.

**`--force` scope test (D-02 regression, no existing analog — write new):** assert the new command errors (does not call `finish_workflow`) when `state.stage` is any value other than `Stage::Ship`, `--force` present or not — mirroring the discipline of Phase 16/17's terminal-Ship invariant tests (search `pipeline_outcomes.rs`/`pipeline_gate.rs` test modules for existing "fail-closed" assertions on Merge failure to match the assertion style).

---

## Shared Patterns

### Backward-compatible `State` field additions (20c)
**Source:** `crates/devflow-core/src/state.rs:27–72`
**Apply to:** any new persisted field (20c's `stopped`/`stop_reason`)
```rust
#[serde(default)]
pub monitor_pid: Option<u32>,
```
Every field since 17-01 (`consecutive_failures`, `infra_failures`, `preflight_retries`, `monitor_pid`) uses `#[serde(default)]` plus an explicit doc comment stating what an absent/older-binary value means. New fields must follow both halves of this pattern, not just the annotation.

### Read-only, no-lock CLI audit commands (20d)
**Source:** `crates/devflow-cli/src/commands.rs::doctor` (`:975`), dispatched read-only with no `workflow::save_state` calls anywhere in its body.
**Apply to:** `devflow release --check` — must remain strictly read-only per D-03; no task in 20d's plan should call any state-mutating helper (`workflow::save_state`, `Gates::respond`, `git tag`, etc.).

### Liveness classification, one source of truth
**Source:** `crates/devflow-cli/src/commands.rs::liveness()` (`:371`) + `Liveness` enum (`:341–366`)
**Apply to:** 20b's `cleanup` guard and 20c's stop-marker interaction with `check_dead_agent`/`check_dead_monitor` — both must consult this one predicate/enum rather than deriving a parallel "is this phase alive" notion, per the Don't-Hand-Roll table and `check_dead_monitor`'s own doc comment ("Reuses `liveness` rather than re-deriving the matrix, so the two copies can never drift").

### `events::emit` for every state-machine-visible transition
**Source:** `pipeline_gate.rs::transition` (emits `"transition"`) and `finish_workflow` (emits `"workflow_finished"`)
**Apply to:** 20c's stop path (new `workflow_finished` variant with a `reason` payload) and 20e's ship-override path (should emit the same events `finish_workflow` already does — no new event type needed since it calls the same function).

### On-disk gate-response record as single source of truth (20e)
**Source:** `crates/devflow-core/src/gates.rs::respond`/`response_path`/`GateAction::from_response`
**Apply to:** 20e's entire design — D-01 explicitly locks "one record, two consumers"; no new schema, no new poll loop.

## No Analog Found

None outright — every file/function in scope has at least a role-match analog in the same file or an immediately adjacent module. The closest to "no analog" is 20d's `ssh-add -l`/`gpg-connect-agent` exit-code branching, which has no in-repo shell-out precedent for *this specific* command but follows the same `Command::new(...).output()` idiom used throughout `version.rs` and `commands.rs::doctor`'s `cmd_check` — classified as role-match, not "no analog," in the table above.

## Metadata

**Analog search scope:** `crates/devflow-core/src/{version,state,gates,stage,mode}.rs`, `crates/devflow-cli/src/{main,commands,pipeline_gate,pipeline_outcomes,pipeline_launch}.rs`, `crates/devflow-cli/tests/{workspace_version_pin,phase7_cli,help_snapshot}.rs`, `scripts/sync-main-to-develop.sh`.
**Files scanned:** 13 (all read directly at HEAD `46a5f7b`/`8ecbdf9`; no re-reads of overlapping ranges).
**Pattern extraction date:** 2026-07-22
