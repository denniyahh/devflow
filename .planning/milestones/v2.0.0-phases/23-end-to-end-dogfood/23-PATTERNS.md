# Phase 23: End-to-End Dogfood - Pattern Map

**Mapped:** 2026-07-25
**Files analyzed:** 20 (8 core 23b migration consumers + 1 rewritten module + 6 23d deletion targets + 3 new-file/new-command surfaces + 2 test files)
**Analogs found:** 20 / 20 (every file has at least a role-match analog; several have an exact same-file predecessor since this phase mostly rewrites in place)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/devflow-core/src/monitor.rs` (rewritten in place) | service (process supervision) | event-driven | `.planning/spikes/socket-supervisor/main.rs` (the spike) + itself pre-rewrite | exact (spike is the proof-of-mechanism for the exact replacement) |
| `crates/devflow-core/src/state.rs` (+`supervisor: Option<SupervisorHandle>` field) | model | CRUD (serde round-trip) | itself — `monitor_pid: Option<u32>` field + its two tests, `state.rs:66-72`, `312-345` | exact |
| `crates/devflow-cli/src/main.rs` (+`Stop` command, +`--yes-ship` flag on `Start`, −`Sequentagent` variant) | route/controller (clap surface) | request-response | itself — `Cleanup`/`Doctor` variants (`:186-234`) for the new `stop` shape; `Start`'s `until: Option<Stage>` (`:71-76`) for flag-threading | exact |
| `crates/devflow-cli/src/commands.rs` (new `stop()` handler; `status`/`doctor` liveness re-pointed at socket probe) | controller/handler | request-response | itself — `cleanup()` (`:380-420`) for the "refuse to touch a live agent without `--force`" shape; `liveness()` (`:517-526`) for the probe re-point | exact |
| `crates/devflow-cli/src/pipeline_launch.rs` (`launch_stage_inner`, spawn call site) | service (pipeline orchestration) | request-response / event-driven | itself, `:55-135` (current `monitor::spawn_monitor` call + `state.monitor_pid` bookkeping) | exact (same file, mechanical swap) |
| `crates/devflow-cli/src/pipeline_outcomes.rs` (`handle_ship_outcome` + new `--yes-ship` auto-approve wrapper) | service (gate orchestration) | request-response | itself, `:275-286` | exact |
| `crates/devflow-core/src/gates.rs` (no new code, but the API `--yes-ship` calls) | service | CRUD (file-based IPC) | itself — `Gates::write_gate`/`Gates::respond`/`GateResponse` (`:37-46`, `179-216`) | exact (reused verbatim, not extended) |
| `crates/devflow-core/src/mode.rs` (read-only reference for `Ship always gates`) | config/policy | request-response | itself, `:82-105` (`Mode::should_gate`) | exact (no change expected, just the rule `--yes-ship` must respect) |
| `crates/devflow-cli/src/preflight.rs` (comment/test-name updates only) | middleware | request-response | itself, `:4,95,174,361,365` | exact (doc-only churn) |
| `crates/devflow-cli/src/staleness.rs` (comment update only) | middleware (gate) | request-response | itself, `:288` | exact (doc-only churn) |
| `crates/devflow-cli/src/test_support.rs` (comment updates only) | test utility | — | itself, `:187,218` | exact (doc-only churn) |
| `crates/devflow-cli/src/parallel.rs` (delete `sequentagent`-only functions; keep `parallel`) | controller | event-driven (was) | itself — the file's own boundary between `parallel()` (kept) and `sequentagent()`/its helpers (deleted); `:201`/`:217` are the two functional monitor-API call sites scoped exclusively to the deleted path | exact |
| `crates/devflow-core/src/agent_result.rs` (strip `SequentagentSlotKind`/`write_sequentagent_slot` + tests) | model/renderer | transform | itself | exact |
| `crates/devflow-core/src/ship.rs` (strip `sequentagent` references) | service | CRUD | itself | exact |
| `crates/devflow-cli/tests/phase7_cli.rs` (strip `sequentagent` cases) | test | request-response | itself | exact |
| `crates/devflow-cli/tests/help_snapshot.rs` + `snapshots/devflow-help.txt` (regenerate) | test (CLI-surface guard) | request-response | itself, full file read | exact |
| `crates/devflow-core/tests/monitor_e2e.rs` (rewrite both tests against socket API + add GONE/STALE/ALIVE cases) | test (integration) | event-driven | itself, full file read | exact |
| `crates/devflow-core/tests/devflow_dir_gitignore.rs` (repoint off `spawn_monitor_no_advance`) | test (integration) | file-I/O | itself, `:56,109,115,121,130` (not fully re-read this pass — repoint mechanically onto the surviving spawn fn) | role-match |
| `README.md` / `CHANGELOG.md` (strip `sequentagent` mentions, D-12 changelog entry) | docs | — | none needed — plain text edit | n/a |
| `crates/devflow-cli/src/config_parse.rs` or `main.rs` (new test: `yes_ship` not settable via config/env) | test | request-response | `crates/devflow-core/src/config.rs` (`review_angles`/`capture_retention` env-precedence pattern, `:116-158`) — used as the **anti-pattern to avoid**, see Shared Patterns | role-match (anti-pattern reference, not to copy) |

## Pattern Assignments

### `crates/devflow-core/src/monitor.rs` (service, event-driven) — the rewrite

**Analog:** `.planning/spikes/socket-supervisor/main.rs` (proof-of-mechanism, `std`+`libc` only, re-run to reproduce) and the current `monitor.rs` (for error type, capture-file paths, and env-propagation conventions to preserve).

**Current error-type pattern to keep** (`monitor.rs:20-32`):
```rust
#[derive(Debug, thiserror::Error)]
pub enum MonitorError {
    #[error("failed to spawn monitor: {0}")]
    Io(#[from] std::io::Error),
    #[error("project path is not valid UTF-8")]
    NonUtf8Path,
    #[error("could not determine devflow binary path")]
    NoBinaryPath,
}
```
Add new variants here (e.g. `SocketBindFailed`, `LiveMonitorAlreadyOwnsSocket`) rather than a parallel error enum — `thiserror` + `#[error(...)]` is the house convention (RESEARCH "Standard Stack").

**Capture-file paths to preserve exactly** (`monitor.rs:91-104`) — the `.devflow/phase-NN-*` convention (R-C/R-D parity):
```rust
let stdout_file = crate::agent_result::stdout_path(&state.project_root, state.phase);
let stderr_file = crate::agent_result::stderr_path(&state.project_root, state.phase);
let exit_file = crate::agent_result::exit_code_path(&state.project_root, state.phase);
let pid_file = crate::agent_result::agent_pid_path(&state.project_root, state.phase);
if let Some(parent) = stdout_file.parent() {
    crate::workflow::ensure_devflow_dir(parent)?;
}
```
These four accessor functions and the `ensure_devflow_dir` call must be called by the new supervisor at spawn time exactly as today — the socket redesign changes *how* the agent is spawned/monitored, not where captures land.

**Core socket bind/spawn/liveness pattern** (from the spike, `main.rs:68-127`, adapt into production):
```rust
// Takeover safety: never steal a socket a live monitor owns.
if Path::new(&sock).exists() {
    if UnixStream::connect(&sock).is_ok() {
        return Err(MonitorError::LiveMonitorAlreadyOwnsSocket); // don't exit(3) in production — propagate
    }
    let _ = std::fs::remove_file(&sock);
}
if let Some(p) = Path::new(&sock).parent() { std::fs::create_dir_all(p)?; }
let listener = UnixListener::bind(&sock)?;
let _ = std::fs::set_permissions(&sock, std::os::unix::fs::PermissionsExt::from_mode(0o600));
```
Note the security note from RESEARCH: bind the *directory* (`~/.cache/devflow/`) as `0700` too, to close the TOCTOU window between `bind` and `set_permissions` on the socket file itself.

**Spawn with `process_group(0)`** (spike `main.rs:86-93`, matches the problem-definition doc's validated building block):
```rust
let mut cmd = Command::new(&c.cmd[0]);
cmd.args(&c.cmd[1..])
    .current_dir(&c.workdir)
    .stdin(Stdio::null())
    .stdout(Stdio::from(out))
    .stderr(Stdio::from(err))
    .process_group(0);
for (k, v) in &c.envs { cmd.env(k, v); } // adapter-scoped env, e.g. Codex unsigned-commit override — Pitfall 4
```

**Liveness probe (GONE/STALE/ALIVE), verbatim contract** (spike `main.rs:163-175`, already excerpted above in RESEARCH — copy this shape into a production `pub fn liveness_probe(socket_path: &Path) -> Liveness`):
```rust
fn status(sock: &str) {
    if !Path::new(sock).exists() { println!("GONE"); return }
    match UnixStream::connect(sock) {
        Ok(mut s) => { /* ping/read one line -> ALIVE */ }
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => println!("STALE"),
        Err(e) => println!("UNKNOWN {e}"),
    }
}
```

**Shutdown / stop path (R-M: a stop is not a completion)** (spike `main.rs:135-157`):
```rust
"shutdown" => {
    stopping.store(true, Ordering::SeqCst);      // suppress the advance tail
    unsafe { libc::kill(-(apid as i32), libc::SIGTERM); }
    // ... wait with deadline, escalate to SIGKILL ...
    let _ = std::fs::write(&exit_file, "143\n");  // R-K: still records a definite exit code
    let _ = writeln!(s, "stopped");
    let _ = std::fs::remove_file(&sock);
    std::process::exit(0);
}
o => { let _ = writeln!(s, "unknown {o}"); }      // V5: reject unrecognized commands, never execute
```
This exact `stopping: AtomicBool` guard around the natural-exit-detection thread is what `devflow stop` (23c) must trigger via the socket, and what suppresses `advance_in_process` from running after an explicit stop.

**In-process advance tail (D-10)** (spike `main.rs:108-126`, adapted):
```rust
if let Ok(Some(st)) = child.lock().unwrap().try_wait() {
    let code = st.code().unwrap_or_else(|| 128 + st.signal().unwrap_or(0));
    std::fs::write(&exit_file, format!("{code}\n"))?;
    devflow_core::advance_in_process(&project_root, phase)?; // NOT Command::new(binary).arg("advance")
    std::fs::remove_file(&sock)?;
    std::process::exit(0);
}
```
Compare against what is being deleted — `monitor.rs:138-160`'s `sh -c` script builds a forked `; {binary} advance ...` tail string; this entire shell-script body (including `shell_escape`, `:236-239`) is replaced, not incrementally patched (State of the Art table).

**No `.unwrap()`/`.expect()` outside tests** — every `Result`-returning spike call above (`bind`, `create_dir_all`, `spawn`) uses `.expect(...)` in the spike because it is throwaway proof code; the production port must convert every one of these to `?` against `MonitorError`, per house convention.

---

### `crates/devflow-core/src/state.rs` (model, CRUD) — `supervisor` field

**Analog:** the existing `monitor_pid` field and its two round-trip tests — this is the single most load-bearing analog in the whole phase because RESEARCH explicitly names it as the pattern to follow.

**Field declaration to mirror** (`state.rs:66-72`):
```rust
/// PID of the detached monitor process that owns the agent for the
/// current stage, recorded by `launch_stage` at spawn time. `None` means
/// no monitor has been spawned for this state yet, OR the state was
/// written by a binary predating this field — in both cases the
/// liveness probe reports Unknown, never Stuck.
#[serde(default)]
pub monitor_pid: Option<u32>,
```
New field should read:
```rust
#[serde(default)]
pub supervisor: Option<SupervisorHandle>,
```
with a doc comment following the identical "`None` means not-yet-spawned OR pre-23b binary — both read as Unknown, never Stuck" phrasing (RESEARCH "In-flight-phase behaviour" mandates this exact semantic).

**`State::new()` initializer to extend** (`state.rs:135-153`) — add `supervisor: None,` alongside `monitor_pid: None,` in the struct literal.

**The two tests to reproduce exactly, renamed for the new field** (`state.rs:312-345`):
```rust
#[test]
fn monitor_pid_round_trips_through_serde() {
    let mut state = State::new(1, AgentKind::Claude, Mode::Auto, PathBuf::from("/repo"));
    state.monitor_pid = Some(4242);
    let json = serde_json::to_string(&state).unwrap();
    assert!(json.contains("monitor_pid"), "monitor_pid must appear in persisted JSON");
    let loaded: State = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.monitor_pid, Some(4242), "monitor_pid must round-trip through serde");
}

#[test]
fn monitor_pid_absent_from_json_defaults_to_none() {
    let json = r#"{"stage":"code","phase":1,"agent":"claude","mode":"auto","started_at":"0","project_root":"/repo"}"#;
    let loaded: State = serde_json::from_str(json).unwrap();
    assert_eq!(loaded.monitor_pid, None);
}
```
Write `supervisor_round_trips_through_serde` and `supervisor_absent_from_json_defaults_to_none` against the identical minimal-JSON fixture (no `supervisor` key present) — this is exactly the "pre-23b binary upgraded mid-run" scenario RESEARCH flags as the one genuinely new risk.

**Also add**: a `yes_ship: bool` field, `#[serde(default)]`, per Pitfall 2 — same mechanical pattern, but note it is a per-run flag threaded from CLI at `State::new()` time, not something any code path mutates later (unlike `monitor_pid`, which is cleared/reset across relaunches).

---

### `crates/devflow-cli/src/main.rs` + `commands.rs` (route + controller) — `devflow stop`

**Analog:** `Cleanup` (clap variant `main.rs:186-193`, handler `commands.rs:380-420`) — closest existing "refuse to touch a live thing without an explicit override" shape, and `Doctor` (`main.rs:227-234`) for the simplest recent subcommand's full plumbing shape (dispatch arm at `main.rs:503`).

**Clap variant shape to copy** (`main.rs:186-193`, adapt):
```rust
/// Remove phase worktrees and their feature branches.
Cleanup {
    #[arg(default_value = ".")]
    project: PathBuf,
    #[arg(long)]
    force: bool,
},
```
`Stop` should follow this shape: `phase: u32` (required, like `Sequentagent`'s `phase` at `:161-162`), `project: PathBuf` (default `.`), no `force` flag needed per D-06/R-M (stop is always explicit, never silently skipped).

**Dispatch arm pattern** (`main.rs:494`):
```rust
Command::Cleanup { project, force } => cleanup(&project_root(project)?, force),
```
`Stop`'s arm: `Command::Stop { phase, project } => stop(&project_root(project)?, phase),`.

**Handler pattern — refuse-without-force / explicit-decision shape** (`commands.rs:380-420`, the exact excerpt to imitate for "don't silently act on a live process"):
```rust
pub(crate) fn cleanup(project_root: &Path, force: bool) -> Result<(), CliError> {
    ...
    let monitor_pid = matched_state.and_then(|s| s.monitor_pid);
    let monitor_alive = monitor_pid.is_some_and(agent::agent_running);
    let phase_liveness = liveness(monitor_pid, monitor_alive, agent_alive);
    ...
    let stopped = matched_state.is_some_and(|s| s.stopped);
    if stopped && !force {
        // ... require --force ...
    }
}
```
`devflow stop`'s handler should reuse the exact liveness-check idiom (`liveness()`, re-pointed at the socket probe per 23b) rather than reading `monitor_pid`/`agent_running` directly — this is precisely what makes `stop` and `status`/`doctor` consistent post-migration.

**Events are append-only and never-silent** — mirror the pattern implied by `commands.rs` and `events::emit` calls seen in `pipeline_launch.rs` (`events::emit(&state.project_root, state.phase, "capture_archived", ...)`), i.e. `devflow stop` must emit a `workflow_stopped` event (already named in RESEARCH's diagram) via `events::emit` before/alongside writing `state.stopped = true`.

---

### `crates/devflow-cli/src/pipeline_outcomes.rs` (service, request-response) — `--yes-ship`

**Analog:** itself, `handle_ship_outcome` (`:275-286`) — the exact call site RESEARCH identifies with no ambiguity.

**Current code (the seam to modify)**:
```rust
pub(crate) fn handle_ship_outcome(project_root: &Path, state: &mut State) -> Result<(), CliError> {
    match run_gate(
        project_root,
        state,
        Stage::Ship,
        "Ship complete — approve merge?",
    )? {
        GateAction::Advance => finish_workflow(project_root, state),
        GateAction::LoopBack(_) => loop_back_to_code(project_root, state, FixType::GapsOnly),
        GateAction::Abort(reason) => abort(project_root, state, &reason),
    }
}
```

**Reusable API to auto-answer with, verbatim** (`crates/devflow-core/src/gates.rs`):
```rust
// GateResponse (gates.rs:37-46)
pub struct GateResponse {
    pub approved: bool,
    #[serde(default)] pub note: Option<String>,
    #[serde(default)] pub responded_by: Option<String>,
}
// Gates::respond (gates.rs:179-198) — errors if no open gate, or already responded
pub fn respond(project_root: &Path, phase: u32, stage: Stage, response: &GateResponse) -> Result<PathBuf, GateError>
// Gates::write_gate (gates.rs:201-217)
pub fn write_gate(project_root: &Path, phase: u32, stage: Stage, context: &str) -> Result<PathBuf, GateError>
```
D-06 requires the gate to still fire and still record a decision — do not skip `run_gate`'s own event/notify emission. Per Pitfall 3, scope this to a dedicated wrapper (`run_gate_auto_approved` or equivalent) called only from `handle_ship_outcome`, never a `stage == Stage::Ship` boolean check inside `run_gate_with_timeout` generically (that would also silently pre-approve the separate finalization-retry gate in `pipeline_gate.rs`).

---

### `crates/devflow-cli/src/main.rs` (`Start` variant) — threading `--yes-ship`

**Analog for flag shape:** `Start`'s existing `until: Option<Stage>` field (`main.rs:71-76`) — an optional, well-documented flag threaded through to state.
```rust
#[arg(long)]
until: Option<Stage>,
```
`--yes-ship` should be `#[arg(long)] yes_ship: bool` on `Start` only (never a top-level global flag, and never on `Advance`/`Resume` — D-05).

**Anti-pattern to avoid — config-persistable settings, do NOT follow this shape for `--yes-ship`** (`crates/devflow-core/src/config.rs:116-158`, the `capture_retention`/`review_angles` precedence chain: env var → `devflow.toml` → built-in default):
```rust
// This is the WRONG shape for --yes-ship — D-05 forbids a devflow.toml or
// env-var path ever setting it. Existing precedent (capture_retention) is
// reproduced here ONLY as the pattern to avoid:
pub fn capture_retention(project_root: &Path) -> u32 {
    // env var takes precedence over devflow.toml and the built-in default
}
```
`--yes-ship` must be a `State`-persisted, per-run boolean set exactly once at `State::new()` time from the CLI flag (Pitfall 2's recommendation), with no `config.rs`/`devflow.toml` reader ever populating it. The dedicated test (RESEARCH's Phase Requirements table row) should assert this directly: no `devflow.toml` key or env var can set `state.yes_ship`.

---

### `crates/devflow-core/tests/monitor_e2e.rs` (test, event-driven integration)

**Analog:** itself, full file (100% reusable shape) — fakes the agent binary via `sh -c`, asserts on captured files.

**Setup pattern to keep** (`:26-58`, hermetic git init):
```rust
fn git(root: &Path, args: &[&str]) {
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
}
```
Any new test that shells out to git for a supervisor fixture must call `devflow_core::test_support::git_command`/`hermetic_command`, never a bare `Command::new("git", ...)` (999.37 — see Shared Patterns).

**Assertion shape to reproduce for the new socket API** (`:60-113`, replacing `spawn_monitor`/`wait_for_agent_pid` with the new spawn/liveness functions):
```rust
spawn_monitor(&state, "sh", &args, &[]).expect("spawn monitor");
let agent_pid = wait_for_agent_pid(root, phase).expect("agent pid recorded");
// -> becomes: spawn_supervisor(...) + liveness_probe(&socket_path) == Liveness::Alive
for _ in 0..200 {
    if exit_path.exists() { exit_seen = true; break; }
    std::thread::sleep(Duration::from_millis(20));
}
```
Add new cases for GONE (no socket file) / STALE (socket file present, monitor process killed, `ECONNREFUSED`) / ALIVE (connect succeeds) — this is the "Distinguishing healthy pause from silent stall" validation requirement from RESEARCH, and this file is its natural home.

---

### `crates/devflow-cli/tests/help_snapshot.rs` (test, CLI-surface guard)

**Analog:** itself, full file — no code change needed, only regenerate the committed snapshot after `Sequentagent` is deleted from `main.rs`.
```rust
// crates/devflow-cli/tests/help_snapshot.rs — regenerate via:
cargo run -q -p devflow -- --help > crates/devflow-cli/tests/snapshots/devflow-help.txt
```
`snapshots/devflow-help.txt:12` currently lists `sequentagent  Run two agents sequentially on one phase, each in its own worktree.` — this line must disappear from the regenerated snapshot, and the test will fail loudly (by design) if the snapshot isn't regenerated to match the deleted variant.

---

### `sequentagent` deletion targets (23d) — role: controller/service/model per file, data flow: was event-driven, now pure subtraction

**Analog:** none needed (subtractive only) — but the boundary-drawing analog is `parallel.rs` itself: the file's own split between the `parallel()` function (N-phases-concurrently, **kept**) and every `sequentagent`-prefixed function/struct (**deleted**). Verify the split at each call site listed in RESEARCH's "23d Deletion Inventory" table before deleting — in particular:
- `crates/devflow-cli/src/pipeline_outcomes.rs:1` reference — **do not delete**: `2026-07-24-process-teardown-solution-research.md` §6 warns `retry_after_from_reason` must *move*, not be deleted (used by the primary rate-limit auto-resume loop at `pipeline_outcomes.rs:92`). Confirm at implementation time whether this file's single `sequentagent` hit is that function or an unrelated mention.
- `crates/devflow-core/src/monitor.rs:138-160` doc-comment references to `sequentagent`'s synchronous handoff (`no_advance_monitor_plus_wait_returns_exit_code_and_captures` test, `wait_for_agent_exit_errors_when_monitor_is_gone` test) are deleted alongside `spawn_monitor_no_advance`/`wait_for_agent_exit` themselves (both dead code once `sequentagent` is gone).

## Shared Patterns

### No `.unwrap()`/`.expect()` outside tests
**Source:** house convention, demonstrated by `MonitorError`'s `#[from]` conversions (`monitor.rs:20-32`) and every production function in `pipeline_launch.rs`/`gates.rs` using `?`.
**Apply to:** every non-test line in the rewritten `monitor.rs` (the spike's own `.expect(...)` calls on `bind`/`spawn`/`create_dir_all` must all become `?` against `MonitorError` in production), and any new `commands::stop`/`--yes-ship` code.

### Per-`Command` `env_remove`, never process-global `set_var`
**Source:** `crates/devflow-core/src/test_support.rs:69-84`
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
**Apply to:** any new test spawning git or the fake-agent shell (`monitor_e2e.rs`, `devflow_dir_gitignore.rs`, and any new supervisor integration test) — always via `git_command`/`hermetic_command`, never `std::env::set_var` (unsafe/unsound in a threaded test binary, 999.37) and never a bare `Command::new("git", ...)`.

### Events are append-only and never-silent
**Source:** `events::emit` call convention seen at `pipeline_launch.rs` (`events::emit(&state.project_root, state.phase, "capture_archived", serde_json::json!({...}))`) and `gates.rs`'s `write_gate`/`respond` info-logging on every call.
**Apply to:** the supervisor's new liveness states (must not silently fold STALE into GONE/Unknown — RESEARCH's core observability requirement), `devflow stop` (must emit `workflow_stopped` before/with the state write), and `--yes-ship`'s auto-approval (must still call `Gates::write_gate` + `Gates::respond`, producing the identical event shape a human approval would).

### `State` field addition with `#[serde(default)]`
**Source:** `state.rs:66-72` (`monitor_pid`) and its two tests at `state.rs:312-345`.
**Apply to:** the new `supervisor: Option<SupervisorHandle>` field and the new `yes_ship: bool` field — both must default correctly for JSON written by a pre-23b binary, and both need dedicated round-trip + absent-defaults tests following the exact naming/assertion pattern shown above.

### `std` + `libc` only — no new dependency
**Source:** RESEARCH "Standard Stack" (re-verified against `Cargo.toml:19` this session) and the spike's own imports (`main.rs:1-13`: `std::os::unix::net`, `std::os::unix::process::{CommandExt, ExitStatusExt}`, `libc::kill`).
**Apply to:** the entire 23b implementation — do not add `command-group`, `duct`, `nix`, `rustix`, `signal-hook`, or any tokio-based crate (all explicitly ruled out in RESEARCH's Alternatives Considered table).

### Reused-not-rebuilt gate API for `--yes-ship`
**Source:** `crates/devflow-core/src/gates.rs:37-46, 179-217` (`GateResponse`, `Gates::write_gate`, `Gates::respond`).
**Apply to:** `pipeline_outcomes.rs`'s new auto-approve wrapper — this is a "don't hand-roll" case per RESEARCH's table: the audit-trail mechanism already exists, `--yes-ship` is a new *producer* of a `GateResponse`, not a new API.

## No Analog Found

None — every file in scope has at least a role-match analog, mostly because 23b/23d are in-place rewrites/deletions of files that already exist, and 23c/`--yes-ship` have exact, RESEARCH-verified call sites with existing sibling commands/APIs to imitate.

## Metadata

**Analog search scope:** `crates/devflow-core/src/{monitor,state,gates,mode,test_support,agent,agent_result,ship}.rs`, `crates/devflow-cli/src/{main,commands,pipeline_launch,pipeline_outcomes,parallel,preflight,staleness,test_support,config_parse}.rs`, `crates/devflow-core/tests/{monitor_e2e,devflow_dir_gitignore}.rs`, `crates/devflow-cli/tests/{help_snapshot,phase7_cli}.rs`, `.planning/spikes/socket-supervisor/{main.rs,README.md}`
**Files scanned:** ~20 read directly this session (targeted ranges via grep+offset for large files), plus `.planning/phases/23-end-to-end-dogfood/{23-CONTEXT.md,23-RESEARCH.md}` and the socket-supervisor spike in full
**Pattern extraction date:** 2026-07-25
