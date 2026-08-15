# Phase 35: Loop-Termination and Baseline Correctness - Pattern Map

**Mapped:** 2026-08-06
**Files analyzed:** 10 existing files modified, 0 new production files, 1 new test-only primitive
(`NoGitPath`) to add to an existing file
**Analogs found:** 10 / 10 (every touched file has a directly-reusable in-repo analog; this phase
is pure refactor-and-harden, so "analog" mostly means "the existing shape of the same file")

This phase creates **no new production files**. Every citation below was re-opened and read this
session (not trusted from CONTEXT.md/RESEARCH.md) per the phase-specific instruction. Two 1-line
citation drifts were found and are called out inline; everything else CONTEXT.md/RESEARCH.md cited
was confirmed byte-for-byte.

## File Classification

| File to modify | Role | Data Flow | Analog / existing shape | Match Quality |
|---|---|---|---|---|
| `crates/devflow-cli/src/test_support.rs` | test-fixture utility (RAII env guard) | file-I/O + process-env mutation | `NeutralPath` (same file, lines 327-359) — `NoGitPath` is a structural sibling | exact |
| `crates/devflow-core/src/agent_result.rs` (`phase_commit_count`, `evaluate_layer2`) | pure classifier / git-shell-out primitive | CRUD-ish (read git state) → `Option<u32>` | own current body (1841-1958) plus the `Err(_) => return Ok(None)` idiom already 3 lines above the call site | exact |
| `crates/devflow-cli/src/pipeline_outcomes.rs` (`handle_validate_outcome`, gate message) | state-transition / orchestration | event-driven (Validate outcome → state mutation → gate) | own current body (353-458) | exact |
| `crates/devflow-core/src/state.rs` (2 new fields) | model / persisted state | CRUD (serde round-trip) | `last_validate_failure_commit_count` (44-100, 415-447) | exact |
| `crates/devflow-core/src/mode.rs` (new ceiling check) | pure predicate | request-response | `should_gate`/`MAX_CONSECUTIVE_FAILURES` (18, 163-179) | exact |
| `crates/devflow-cli/src/pipeline_launch.rs` (999.84 test) | test (regression) | request-response (synchronous `advance()`) | `advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records` (2302-2357) | exact |
| `crates/devflow-core/src/git.rs` (signing probe rewrite) | service (process-spawn probe) | request-response (spawn → exit code → verdict) | own current `check_ssh_signing_viability` (874-945) + `canary.rs`'s reap template | exact |
| `crates/devflow-cli/src/commands.rs` (`check_signing`, unaffected but a consumer) | controller-ish (CLI output mapper) | request-response | own current body (2380-2404) | exact, untouched shape |
| `crates/devflow-cli/tests/release_check.rs` (2 rewritten tests) | integration test | request-response (spawns the real binary) | `release_check_signing_degrades_when_ssh_add_absent` (462-496) + `git_only_path()` (446-457) | exact |
| `CHANGELOG.md` / crate docs (D-08 deliverable) | config/docs | — | `## 2.4.0` heading (`CHANGELOG.md:3`) | exact |

## Pattern Assignments

### 1. The RAII PATH-guard family — `NeutralPath` / `env_lock()` / `ENV_MUTEX`

**File:** `crates/devflow-cli/src/test_support.rs`
**This is the single most important excerpt in this document** — `NoGitPath` (criteria 1 and 6's
prerequisite) must mirror this shape exactly, in the same file.

**`ENV_MUTEX`, exact (line 50):**
```rust
pub(crate) static ENV_MUTEX: Mutex<()> = Mutex::new(());
```

**`env_lock()`, exact (lines 94-98) — the only sanctioned way to acquire it:**
```rust
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
```

**`NeutralPath`, exact (lines 327-359) — struct, `install()`, `Drop`:**
```rust
pub(crate) struct NeutralPath {
    _dir: tempfile::TempDir,
    original: Option<std::ffi::OsString>,
}

impl NeutralPath {
    /// Named `install`, not `new`: binding it is not bookkeeping, it mutates
    /// process-global state at the moment of the call.
    pub(crate) fn install() -> Self {
        let dir = agent_free_git_only_path_dir();
        let original = std::env::var_os("PATH");
        // SAFETY: the caller holds ENV_MUTEX (documented precondition), so
        // no other test thread is reading or writing PATH concurrently.
        unsafe { std::env::set_var("PATH", dir.path()) };
        Self {
            _dir: dir,
            original,
        }
    }
}

impl Drop for NeutralPath {
    fn drop(&mut self) {
        // SAFETY: still serialized under the ENV_MUTEX guard the caller holds
        // for at least as long as this guard's own scope.
        unsafe {
            match &self.original {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
    }
}
```

**`NoGitPath` is a structural sibling with ONE change: an empty `tempdir()` instead of
`agent_free_git_only_path_dir()`'s git-symlinked one** (RESEARCH's own drafted shape, verified
against the real `NeutralPath` this session — the shape is correct, add it beside `NeutralPath` in
this same file):
```rust
pub(crate) struct NoGitPath {
    _dir: tempfile::TempDir,
    original: Option<std::ffi::OsString>,
}

impl NoGitPath {
    pub(crate) fn install() -> Self {
        let dir = tempfile::tempdir().unwrap(); // deliberately empty — no `git`
        let original = std::env::var_os("PATH");
        unsafe { std::env::set_var("PATH", dir.path()) };
        Self { _dir: dir, original }
    }
}

impl Drop for NoGitPath {
    fn drop(&mut self) {
        unsafe {
            match &self.original {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
    }
}
```

**How callers use `NeutralPath` today (the shape `NoGitPath`'s callers must follow) — verified live
site at `pipeline_outcomes.rs:1994-2001`:**
```rust
// WR-05: RAII, so a panic inside the region restores PATH by `Drop`
// rather than by a trailing statement the unwind would skip. Scoped so
// the restore still happens before the assertions below, exactly as
// the trailing-statement form did.
{
    let _path_guard = NeutralPath::install();
    let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
}
```
Note `env_lock()` is acquired once, earlier, at the *enclosing test's* top (`let _guard =
env_lock();`), and `NeutralPath::install()`/`NoGitPath::install()` is nested inside its scope in a
separate `{ }` block so `Drop` fires — restoring `PATH` — **before** the test's own assertions run.
This is the exact two-guard nesting the 999.77 two-cycle test and the 999.87 `evaluate_layer2` test
must both use.

**Negative control, mandatory before trusting `NoGitPath` in a real test (RESEARCH's own
instruction, not yet built anywhere in the workspace):**
```rust
// Throwaway probe / permanent harness-sanity test:
{
    let _guard = env_lock();
    let before = devflow_core::test_support::git_command(tmp).arg("--version").output();
    assert!(before.is_ok());
    let result = {
        let _path_guard = NoGitPath::install();
        devflow_core::test_support::git_command(tmp).arg("--version").output()
    };
    assert!(result.is_err(), "NoGitPath must make `git` unresolvable");
    let after = devflow_core::test_support::git_command(tmp).arg("--version").output();
    assert!(after.is_ok(), "PATH must be restored after the guard drops");
}
```

**`stub_agent_binary` + `prepend_path`, exact (lines 393-402, 407-416) — the *different* PATH
primitive (prepend, not replace) the 999.84 test already uses and must keep using unchanged:**
```rust
pub(crate) fn stub_agent_binary(name: &str) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    dir
}

/// Prefix `PATH` with `stub_dir`, keeping the rest of `original` intact
/// so `sh`/`git` still resolve normally — only the stubbed binary name
/// is shadowed (it is found first).
pub(crate) fn prepend_path(
    stub_dir: &tempfile::TempDir,
    original: &Option<std::ffi::OsString>,
) -> std::ffi::OsString {
    let mut dirs = vec![stub_dir.path().to_path_buf()];
    if let Some(original) = original {
        dirs.extend(std::env::split_paths(original));
    }
    std::env::join_paths(dirs).unwrap()
}
```
**Do not confuse the two families.** `NeutralPath`/`NoGitPath` *replace* `PATH` (nothing real
resolves); `stub_agent_binary`+`prepend_path` *prepend* to the real `PATH` (real `git`/`sh` still
resolve, only the named agent binary is shadowed). The 999.84 test needs the prepend family (it
still needs real `git` for `init_repo` and real `sh` for the monitor's backgrounding script); the
999.77/999.87 tests need the replace family.

---

### 2. `State`'s serde backward-compat pair — the pattern for two new fields (999.78, 999.79)

**File:** `crates/devflow-core/src/state.rs`

**Field declaration pattern to copy verbatim in shape** (lines 99-100, `last_validate_failure_commit_count`):
```rust
/// A serde-absent value (state written by a binary predating this field)
/// deserializes to `None`, which is exactly the "no prior record"
/// meaning above — the same backward-compat pattern as every other
/// `#[serde(default)]` field added since 17-01.
///
/// Unlike [`Self::consecutive_failures`] and [`Self::infra_failures`],
/// this field is NOT touched by `transition()` — it is a baseline
/// observation rather than a counter, matching how
/// [`Self::preflight_retries`] and [`Self::checkpoint_resumes`] are
/// handled.
#[serde(default)]
pub last_validate_failure_commit_count: Option<u32>,
```

**The round-trip test pair to copy verbatim in shape** (lines 415-447 — confirmed exact, matches
CONTEXT.md's citation exactly):
```rust
#[test]
fn last_validate_failure_commit_count_round_trips_through_serde() {
    let mut state = State::new(1, AgentKind::Claude, Mode::Auto, PathBuf::from("/repo"));
    state.last_validate_failure_commit_count = Some(3);
    let json = serde_json::to_string(&state).unwrap();
    assert!(
        json.contains("last_validate_failure_commit_count"),
        "last_validate_failure_commit_count must appear in persisted JSON"
    );
    let loaded: State = serde_json::from_str(&json).unwrap();
    assert_eq!(
        loaded.last_validate_failure_commit_count,
        Some(3),
        "last_validate_failure_commit_count must round-trip through serde"
    );
}

/// A serde-absent `last_validate_failure_commit_count` (state written by
/// a binary predating this field) must deserialize to `None` — the
/// "no prior failure recorded" meaning — not to `Some(0)`, which would
/// misrepresent a never-observed baseline as an observed zero.
#[test]
fn last_validate_failure_commit_count_absent_from_json_defaults_to_none() {
    let json = r#"{
        "stage": "code",
        "phase": 1,
        "agent": "claude",
        "mode": "auto",
        "started_at": "0",
        "project_root": "/repo"
    }"#;
    let loaded: State = serde_json::from_str(json).unwrap();
    assert_eq!(loaded.last_validate_failure_commit_count, None);
}
```
Each of the 999.78 (ceiling counter) and 999.79 (verification fingerprint) new fields needs the
identical pair: one round-trip-with-a-value test, one absent-from-JSON-defaults test. Note the
JSON-`contains` assertion in the first test — it exists specifically to catch a field accidentally
marked `skip_serializing_if`, which would still pass a naive in-memory round trip while never
persisting anything.

**`State::new`, exact (lines 256-280) — every new field's default must be added here:**
```rust
impl State {
    /// Create a new state for starting a phase at the [`Stage::Define`] stage.
    pub fn new(phase: u32, agent: AgentKind, mode: Mode, project_root: PathBuf) -> Self {
        State {
            stage: Stage::Define,
            phase,
            agent,
            mode,
            gate_pending: false,
            consecutive_failures: 0,
            infra_failures: 0,
            preflight_retries: 0,
            last_validate_failure_commit_count: None,
            started_at: timestamp_now(),
            project_root,
            worktree_path: None,
            monitor_pid: None,
            session_id: None,
            checkpoint_resumes: 0,
            stop_until: None,
            stopped: false,
            stop_reason: None,
            yes_ship: false,
            canary: None,
            legacy_claude_launch: false,
        }
    }
}
```
**Confirms A-11/A-13 independently: this runs unconditionally in `commands::start()` at line 124,
`--force` included** (verified this session, exact):
```rust
pub(crate) fn start(
    project_root: &Path, phase: u32, agent: AgentKind, mode: Mode,
    force: bool, worktree: bool, dry_run: bool, until: Option<Stage>,
    yes_ship: bool, legacy_claude_launch: bool,
) -> Result<(), CliError> {
    let mut state = State::new(phase, agent, mode, project_root.to_path_buf());   // line 124
    state.stop_until = until;
    // ... force is only consulted much later:
    //   ensure_phase_worktree(project_root, phase, force)  — line 239
```

**`transition()`, exact (`pipeline_gate.rs:94-99`) — confirms which fields it touches:**
```rust
state.stage = to;
if mode::transition_resets_consecutive_failures(from, to) {   // conditional
    state.consecutive_failures = 0;
}
state.infra_failures = 0;                                      // unconditional
state.gate_pending = false;
```
`transition()` does **not** touch `preflight_retries`, `checkpoint_resumes`, or
`last_validate_failure_commit_count`. Both new 999.78/999.79 fields belong in that
not-touched-by-`transition()` group, per CONTEXT.md's corrected `code_context` bullet (A-01) —
**do not** justify either new field with "the existing counter is reset by `transition()` on this
hop" — inside the Code→Validate loop it explicitly is not
(`mode::transition_resets_consecutive_failures`, exact, `mode.rs:111-113`):
```rust
pub fn transition_resets_consecutive_failures(from: Stage, to: Stage) -> bool {
    !matches!((from, to), (Stage::Code, Stage::Validate))
}
```

---

### 3. `verify.rs`'s opposite-result / root-sensitivity test pair (D-06's mechanical control)

**File:** `crates/devflow-core/src/verify.rs`

**Small citation correction, confirmed:** CONTEXT.md cites `:340-400`/`:351`/`:376`. The function
body is `130-136`; the two tests are at exact lines **351** and **377** (one line later than
CONTEXT's second citation — immaterial to shape, noted per the phase's "verify every line number"
instruction).

**The function under test, exact (130-136):**
```rust
pub fn phase_has_blocking_human_checkpoint(project_root: &Path, phase: u32) -> bool {
    const HUMAN_BLOCKING_GATE: &str = r#"gate="blocking-human""#;
    phase_plan_files(project_root, phase)
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .any(|contents| contents.contains(HUMAN_BLOCKING_GATE))
}
```

**Test 1, exact (350-372) — this is THE shape D-05/D-06's 999.84 test must mirror:**
```rust
#[test]
fn phase_has_blocking_human_checkpoint_reads_the_execution_root_in_worktree_mode() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = dir.path().join("phase-worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    let body = format!(
        "---\nphase: 91\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE}\">\n</task>\n"
    );
    // The PLAN exists ONLY inside the worktree — the project root's own
    // `.planning/phases/` is deliberately never created.
    write_phase_file(&worktree, "91-probe", "91-01-PLAN.md", &body);

    assert!(
        phase_has_blocking_human_checkpoint(&worktree, 91),
        "the execution root holds the PLAN, so the declaration must be found"
    );
    assert!(
        !phase_has_blocking_human_checkpoint(dir.path(), 91),
        "opposite-result case: the project root has no PLAN and must return false — \
         if both roots returned true, this pair would be measuring the presence of a \
         file somewhere rather than which root is read"
    );
}
```
**This is the exact "opposite-result assertion in the same test" idiom (34/D-08 lineage) D-06
requires for the 999.84 extension** — a pair that returns the same answer for both roots is
measuring the wrong thing.

**Test 2, exact (374-395) — the no-worktree mirror, same idiom:**
```rust
/// The main-checkout mirror of the test above: with no worktree the two
/// roots coincide, so 999.76's call-site change leaves this path untouched.
#[test]
fn phase_has_blocking_human_checkpoint_still_reads_the_project_root_without_a_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let empty_sibling = dir.path().join("phase-worktree");
    std::fs::create_dir_all(&empty_sibling).unwrap();
    let body = format!(
        "---\nphase: 91\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE}\">\n</task>\n"
    );
    write_phase_file(dir.path(), "91-probe", "91-01-PLAN.md", &body);

    assert!(
        phase_has_blocking_human_checkpoint(dir.path(), 91),
        "without a worktree the execution root IS the project root"
    );
    assert!(
        !phase_has_blocking_human_checkpoint(&empty_sibling, 91),
        "opposite-result case: a root without the PLAN must return false, so the \
         assertion above is about which root is read and not about the file existing"
    );
}
```

---

### 4. `pipeline_launch.rs`'s two `advance()` harnesses — pick the right base (999.84)

**File:** `crates/devflow-cli/src/pipeline_launch.rs`

**The call site under test, exact (1067-1071 — confirmed to the exact line CONTEXT.md/RESEARCH cite):**
```rust
let mut reason = result.reason.clone();
let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
let checkpoint_confirmed = state.agent == AgentKind::Claude
    && verify::phase_has_blocking_human_checkpoint(execution_root, phase)   // line 1070
    && agent_result::checkpoint_reported_in_capture(project_root, phase);
```

**The CORRECT base test to extend, exact (2244-2357) — confirmed at the precise line CONTEXT.md's
A-02 amendment names:**
```rust
const HUMAN_GATE_VALUE_FOR_TEST: &str = "blocking-human";

/// Write a synthetic phase's plan declaring a `blocking-human`
/// checkpoint task, matching `verify::phase_plan_files`'s discovery
/// pattern (`.planning/phases/{NN}-*/{NN}-*-PLAN.md`).
fn write_declared_checkpoint_plan(root: &Path, phase: u32) {
    let dir = root
        .join(".planning/phases")
        .join(format!("{phase:02}-checkpoint-fixture"));
    std::fs::create_dir_all(&dir).unwrap();
    let body = format!(
        "---\nphase: {phase}\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE_FOR_TEST}\">\n</task>\n"
    );
    std::fs::write(dir.join(format!("{phase:02}-01-PLAN.md")), body).unwrap();
}

/// Write a captured stdout containing BOTH the `**Gate:**
/// blocking-human` confirmation literal and a `DEVFLOW_RESULT` failed
/// marker, so `advance()`'s Layer 1 deterministically classifies the
/// outcome as `Failed` (-> `Action::GateReview`) with no exit-code/pid
/// file or background thread needed.
fn write_confirmed_checkpoint_capture(root: &Path, phase: u32) {
    std::fs::create_dir_all(root.join(".devflow")).unwrap();
    let stdout = format!(
        "## CHECKPOINT REACHED\n\n**Type:** human-verify\n**Gate:** {HUMAN_GATE_VALUE_FOR_TEST}\n\nDEVFLOW_RESULT: {{\"status\": \"failed\", \"reason\": \"checkpoint pending\"}}\n"
    );
    std::fs::write(agent_result::stdout_path(root, phase), stdout).unwrap();
}

/// The positive case: declared + reported + Claude + session id + under
/// the ceiling -> resumes and records exactly one audit event, with no
/// `gate_fired` for this stage.
#[test]
fn advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records() {
    let _guard = env_lock();

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);

    let phase = 88;
    write_declared_checkpoint_plan(root, phase);
    write_confirmed_checkpoint_capture(root, phase);

    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    state.session_id = Some("sess-checkpoint-1".to_string());
    workflow::save_state(&state).unwrap();

    let stub_dir = stub_agent_binary("claude");
    let original_path = std::env::var_os("PATH");
    let stubbed_path = prepend_path(&stub_dir, &original_path);
    // SAFETY: serialized under ENV_MUTEX.
    unsafe {
        std::env::set_var("PATH", &stubbed_path);
    }

    let result = advance(root, Some(phase));

    // SAFETY: still serialized under ENV_MUTEX from above.
    unsafe {
        match &original_path {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
    }
    let reloaded_for_reap = workflow::load_state(root, phase).ok();
    let _reap_guard = reloaded_for_reap
        .as_ref()
        .map(ReapMonitorOnDrop::after_launch);

    result.unwrap();

    let auto_decided = events_of_kind(root, "checkpoint_auto_decided");
    assert_eq!(
        auto_decided.len(),
        1,
        "expected exactly one checkpoint_auto_decided event: {auto_decided:?}"
    );
    assert_eq!(auto_decided[0]["session_id"], "sess-checkpoint-1");
    assert_eq!(auto_decided[0]["stage"], "code");

    let gate_fired = events_of_kind(root, "gate_fired");
    assert!(
        gate_fired.iter().all(|e| e["stage"] != "code"),
        "a confirmed, auto-resolved checkpoint must never also fire the \
         generic gate for the same stage: {gate_fired:?}"
    );
}
```
**999.84's delta on this base, concretely:** (1) add `state.worktree_path = Some(worktree)` before
`save_state`; (2) call `write_declared_checkpoint_plan(&worktree, phase)` instead of `(root,
phase)` — the helper is generic over whatever `Path` it receives, zero changes needed to the
helper itself; (3) write D-05's decoy PLAN under `root` for the same phase number using the same
fixture shape **minus** `gate="blocking-human"` in the body; (4) add D-06's mechanical assertion
`assert!(!verify::phase_has_blocking_human_checkpoint(root, phase))` inside the same test; (5)
perform the revert of `execution_root` → `project_root` at line 1068, confirm the test fails,
revert the revert, confirm it passes again — record both outcomes in the phase's SUMMARY (D-06
explicitly rejects a heavier `35-evidence/` capture directory).

**The wrong base, kept only as a labelled contrast (do not extend this one) — `code_unknown_does_not_transition_to_validate`, exact (1452-1512):**
```rust
#[test]
fn code_unknown_does_not_transition_to_validate() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    // ... git commit fixture ...
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    workflow::save_state(&state).unwrap();

    let code_gate = Gates::gate_path(root, phase, Stage::Code);
    let validate_gate = Gates::gate_path(root, phase, Stage::Validate);
    let response_path = Gates::response_path(root, phase, Stage::Code);

    std::thread::scope(|scope| {
        scope.spawn(|| {
            advance(root, Some(phase)).unwrap();
        });
        // ... poll code_gate.exists() up to 150 * 20ms, then write an abort response ...
    });
}
```
This is a **scoped-thread + gate-file-polling** harness — a different, heavier shape (used because
that test drives a path with no deterministic single-pass Layer-1 classification). The 999.84
extension needs the *synchronous* `:2302` base, not this one — a new test that spawns a scoped
thread and polls a gate file has copied the wrong shape.

---

### 5. The bounded-timeout spawn template (D-01's `ssh-keygen -Y sign` probe)

**File:** `crates/devflow-core/src/canary.rs` (the `reap()` half) and `crates/devflow-core/src/agent.rs`
(`terminate_and_verify`'s deadline loop) — no timeout crate exists in `devflow-core`, and
`Command::output()` cannot time out on its own (A-07's correction: this is a real in-crate
precedent to copy, not a new dependency).

**`canary.rs::reap`, exact (403-433) — the shape to copy: `spawn → loop{try_wait, sleep} → kill →
wait`:**
```rust
/// Wait a bounded time for the canary child to exit, then kill it.
fn reap(child: &mut std::process::Child) {
    let deadline = Instant::now() + Duration::from_secs(CANARY_REAP_GRACE_SECS);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(err) => {
                warn!("could not poll the canary child: {err}");
                return;
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(REAP_POLL);
    }
    let _ = child.kill();
    let _ = child.wait();
}
```
**What NOT to copy from `canary.rs`:** the surrounding `mpsc`-channel reader/writer-thread
machinery (lines ~309-400) that streams stdout incrementally — the signing probe does not need to
read output while the child runs, only whether it finished and with what exit code. Only the
`spawn → try_wait loop → kill → wait` shape is needed, matching RESEARCH's own explicit statement
of what the template does and does not give for free.

**`agent.rs::terminate_and_verify`'s deadline loop, exact (135-141) — the second, simpler precedent
for the same "poll until a deadline, then escalate" idiom:**
```rust
let term_deadline = std::time::Instant::now() + wait;
while std::time::Instant::now() < term_deadline {
    if !agent_running(pid) {
        return true;
    }
    std::thread::sleep(poll);
}
```

**Concrete construction for the probe (`Command` builder + env var, no existing site sets this
yet — confirmed, `rg` for `SSH_ASKPASS_REQUIRE` returns nothing in the workspace):**
```rust
Command::new("ssh-keygen")
    .args(["-Y", "sign", "-n", "git", "-f", key_path_str, payload_path_str])
    .env("SSH_ASKPASS_REQUIRE", "never")
    .spawn()
```
then the `reap()`-shaped bounded wait, on the spawned `Child`. **The `-n git` namespace is
independently verified this session (Section F of RESEARCH.md), decoded directly from this
repository's own real SSHSIG tag signature — do not re-derive it from documentation or memory.**

---

### 6. `git.rs`'s signing-viability surface — every symbol this phase touches, exact bodies

**File:** `crates/devflow-core/src/git.rs`

**`SigningStatus` + `classify_ssh_add_status`, exact (727-750) — DELETED by D-04, together with the
one test that references either (`classify_ssh_add_status_maps_all_three_documented_exit_codes`,
exact lines 1828-1834):**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigningStatus {
    NoAgent,
    AgentEmpty,
    KeysListed,
    Unknown(i32),
}

pub fn classify_ssh_add_status(exit_code: i32) -> SigningStatus {
    match exit_code {
        2 => SigningStatus::NoAgent,
        1 => SigningStatus::AgentEmpty,
        0 => SigningStatus::KeysListed,
        other => SigningStatus::Unknown(other),
    }
}
```
```rust
#[test]
fn classify_ssh_add_status_maps_all_three_documented_exit_codes() {
    assert_eq!(classify_ssh_add_status(2), SigningStatus::NoAgent);
    assert_eq!(classify_ssh_add_status(1), SigningStatus::AgentEmpty);
    assert_eq!(classify_ssh_add_status(0), SigningStatus::KeysListed);
    assert_eq!(classify_ssh_add_status(7), SigningStatus::Unknown(7));
}
```

**`SigningViability`, exact (757-767) — PUBLIC, UNCHANGED shape (kept as the output contract):**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigningViability {
    Viable { fingerprint: Option<String> },
    NotViable { reason: String },
    Unknown { reason: String },
}
```

**`public_key_fingerprint`, exact (786-800) — KEPT under D-04, the only helper still needed once
D-03 routes only the path form to `Viable`:**
```rust
fn public_key_fingerprint(pub_key_path: &Path) -> Option<String> {
    let path_str = pub_key_path.to_str()?;
    let output = Command::new("ssh-keygen")
        .args(["-lf", path_str])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    // Format: "<bits> SHA256:<hash> <comment> (<type>)"
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}
```

**`inline_signing_key_blob`, exact (814-823) — STILL REQUIRED under D-03 (classification only):**
```rust
fn inline_signing_key_blob(signingkey: &str) -> Option<&str> {
    let trimmed = signingkey.trim();
    if let Some(remainder) = trimmed.strip_prefix("key::") {
        Some(remainder)
    } else if trimmed.starts_with("ssh-") {
        Some(trimmed)
    } else {
        None
    }
}
```

**`inline_key_fingerprint`, exact (841-866) — ORPHANED by D-03/D-04; delete with its test
(`inline_key_fingerprint_matches_the_path_branch_for_the_same_key`, starts at exact line 1996):**
```rust
fn inline_key_fingerprint(key_blob: &str) -> Option<String> {
    let mut child = Command::new("ssh-keygen")
        .args(["-lf", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    stdin.write_all(key_blob.as_bytes()).ok()?;
    drop(stdin);
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}
```

**`check_ssh_signing_viability`, exact (874-945) — THE FUNCTION D-02's probe replaces the body of.
Preserve the two pre-probe early returns (unset key, missing path file) byte-for-byte; replace
everything from the `ssh-add -l` spawn (line 901) onward:**
```rust
fn check_ssh_signing_viability(project_root: &Path) -> SigningViability {
    let Some(signingkey) = git_config(project_root, "user.signingkey") else {
        return SigningViability::NotViable {
            reason: "gpg.format=ssh but user.signingkey is not set".into(),
        };
    };

    let inline_blob = inline_signing_key_blob(&signingkey);

    // D-03: inline forms return Unknown here and are never probed. This
    // early return REPLACES the KeysListed arm's inline branch below.
    if inline_blob.is_some() {
        return SigningViability::Unknown {
            reason: /* D-03's new fixed reason string for the inline-key class, A-17 */,
        };
    }

    let key_path = Path::new(&signingkey);
    if !key_path.exists() {
        return SigningViability::NotViable {
            reason: "user.signingkey is set but the key file does not exist".into(),
        };
    }

    // --- D-02's replacement: spawn `ssh-keygen -Y sign`, bounded by a
    //     wall-clock timeout (Pattern 2 above), with SSH_ASKPASS_REQUIRE=never
    //     (D-01). Exit code is the sole verdict (D-02); ssh-keygen's stderr
    //     is never re-emitted (D-08's redaction contract — it embeds the
    //     configured path, e.g. "Couldn't load public key ./does-not-exist.pub").
    //     Three classes: timed out; exited non-zero; ssh-keygen absent -> Unknown.
    //     On success: SigningViability::Viable { fingerprint: public_key_fingerprint(key_path) }.
}
```
**Untouched, exact (949-975 and 983-988) — D-02 scopes this fix to SSH only:**
```rust
fn check_gpg_signing_viability(project_root: &Path) -> SigningViability { /* unchanged */ }

pub fn check_signing_viability(project_root: &Path) -> SigningViability {
    match git_config(project_root, "gpg.format").as_deref() {
        Some("ssh") => check_ssh_signing_viability(project_root),
        _ => check_gpg_signing_viability(project_root),
    }
}
```

**`SigningViability`'s only consumer, `commands.rs::check_signing`, exact (2380-2404 — one-line
correction: CONTEXT.md/RESEARCH cite the doc comment's line 2379 as the `fn` line; the `fn` itself
is at 2380):**
```rust
fn check_signing(project_root: &Path) -> Check {
    const NAME: &str = "tag-signing viability";
    match devflow_core::git::check_signing_viability(project_root) {
        devflow_core::git::SigningViability::Viable { fingerprint } => Check {
            name: NAME.into(),
            status: "ok".into(),
            version: Some(match fingerprint {
                Some(fp) => format!("signing viable ({fp})"),
                None => "signing viable".into(),
            }),
            install_hint: None,
        },
        devflow_core::git::SigningViability::NotViable { reason } => Check {
            name: NAME.into(),
            status: "fail".into(),
            version: Some(reason),
            install_hint: Some("resolve before attempting the signed release tag".into()),
        },
        devflow_core::git::SigningViability::Unknown { reason } => Check {
            name: NAME.into(),
            status: "warn".into(),
            version: Some(reason),
            install_hint: None,
        },
    }
}
```
This function's own shape (a plain `match` on the three variants, mapping to `Check{status,
install_hint}`) is **unchanged by this phase** — `install_hint` is `Some(...)` only on `NotViable`,
`None` on `Unknown`, which is the exact discriminator `release_check.rs`'s rewritten tests must
keep asserting on (see below).

---

### 7. `release_check.rs`'s existing integration-test shape — the two tests to rewrite

**File:** `crates/devflow-cli/tests/release_check.rs`

**`git_only_path()`, exact (446-457) — the "make one specific tool absent from PATH" fixture, an
integration-test-scope analog of `agent_free_git_only_path_dir` (a different mechanism: `which git`
+ `.symlink`, not a `tempdir()`-symlink walk, because this file lives outside `devflow-cli`'s own
`#[cfg(test)]` module and cannot import its private test_support):**
```rust
fn git_only_path() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let which = Command::new("which")
        .arg("git")
        .output()
        .expect("locate git via `which`");
    assert!(which.status.success(), "`which git` failed");
    let real_git = String::from_utf8_lossy(&which.stdout).trim().to_string();
    std::os::unix::fs::symlink(real_git, dir.path().join("git"))
        .expect("symlink git into the minimal PATH fixture");
    dir
}
```

**The test to rewrite (`ssh-add`-absent → `ssh-keygen`-absent), exact shape (461-496) — the
assertion `stdout.contains("ssh-add not found")` becomes `stdout.contains("ssh-keygen not
found")` (or whatever fixed D-02 string names this class), and `git_only_path()` already produces
exactly the PATH shape needed (only `git` resolvable — no separate `ssh-keygen`-present-but-no-`ssh-add`
variant is needed once D-04 removes `ssh-add` from the probe entirely):**
```rust
#[test]
fn release_check_signing_degrades_when_ssh_add_absent() {   // -> rename, e.g. _when_ssh_keygen_absent
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    commit(root, "base.txt");
    git(root, &["config", "gpg.format", "ssh"]);
    let key_path = root.join("fake-signing-key.pub");
    std::fs::write(&key_path, "ssh-ed25519 AAAAfixture placeholder\n").unwrap();
    git(root, &["config", "user.signingkey", key_path.to_str().unwrap()]);

    let isolated_home = tempfile::tempdir().unwrap();
    let path_dir = git_only_path();
    let output = Command::new(devflow_bin())
        .arg("release")
        .arg("--check")
        .arg(root)
        .env("HOME", isolated_home.path())
        .env("PATH", path_dir.path())
        .output()
        .expect("spawn devflow release");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("ssh-add not found"), /* -> the new fixed string */ );
    assert!(!stdout.contains("panicked"), "must not panic, got: {stdout}");
}
```
The sibling `release_check_inline_signingkey_degrades_to_warn_when_ssh_tooling_absent` (exact,
513-562) needs the same rename + string swap, plus — under D-03 — its whole *premise* changes:
today it proves an inline key degrades to `Unknown` when tooling is absent; after D-03, an inline
key **always** returns `Unknown` regardless of tooling presence (it is never probed at all). This
test's fixture (inline `key::` blob, no real keypair generated) still applies, but its assertions
should be re-derived from D-03's contract, not merely have a string swapped.

**Leak-check style precedent** (`release_check_signing_output_leaks_no_key_material_or_path`, exact
256-303) — the shape any new positive/negative probe test should also carry: assert `!stdout.contains(root path)`, `!stdout.contains("PRIVATE KEY")`, `!stdout.contains("panicked")`. D-08's
redaction contract binds the new probe exactly as it bound the old one.

---

## Shared Patterns

### A. `ENV_MUTEX` discipline (applies to every new/modified test in `devflow-cli`)
**Source:** `crates/devflow-cli/src/test_support.rs:50,94-98`
**Apply to:** the 999.77/999.87 harness tests, the 999.84 extension, any test mutating `PATH`.
Acquire `env_lock()` once at the test's top; construct the PATH-mutating guard (`NeutralPath`,
`NoGitPath`, or `stub_agent_binary`+`prepend_path`) nested inside a scoped block so `Drop` restores
`PATH` before assertions run. Never call `ENV_MUTEX.lock().unwrap()` directly — always
`env_lock()`, which tolerates poison because every mutation under it is unwind-safe by construction.

### B. Opposite-result / negative-control assertions in the same test
**Source:** `crates/devflow-core/src/verify.rs:351,377` (and 34/D-08's precedent)
**Apply to:** D-06's mechanical control (999.84); the both-directions test for 999.79 (A-12); the
two-cycle test for 999.77 (a single-cycle test is explicitly a proxy per CONTEXT.md); the
`evaluate_layer2`-unrunnable-git test for 999.87 (must not reuse the existing
`evaluate_layer2_exit_zero_no_commits_is_failed` real-git-empty-branch test, which is a DIFFERENT,
already-correct path per A-06's `Err`/`Ok(nonzero)` split).
A pair that returns the same answer under both conditions is measuring the wrong thing — every new
test this phase adds needs a stated case that must produce the opposite result.

### C. `Option<u32>`/structural-guard over hand-audited equality (34/D-06's line, continued by D-08)
**Source:** `crates/devflow-core/src/agent_result.rs:1841` (`phase_commit_count`'s signature change)
**Apply to:** every call site of `phase_commit_count` — the type change forces the compiler to
enumerate both consumers (`evaluate_layer2` at `:1905`, `handle_validate_outcome` at
`pipeline_outcomes.rs:400-401`) rather than relying on a hand audit to find them.

### D. The `Err(_) => return Ok(None)` fall-to-Layer-3 idiom
**Source:** `crates/devflow-core/src/agent_result.rs:1901`, three lines above the
`phase_commit_count` call site D-09 edits:
```rust
let exit_code: i32 = match std::fs::read_to_string(&exit_path) {
    Ok(s) => s.trim().parse().unwrap_or(-1),
    Err(_) => return Ok(None), // fall to Layer 3
};
```
**Apply to:** D-09's own fix — `evaluate_layer2`'s `None`-commit-count branch must return the
identical `Ok(None)` shape, not a new error variant or a different sentinel. "I could not read my
input" already has an established answer in this exact function.

### E. Doc-comment-as-deliverable (999.77's two comments)
**Source:** `agent_result.rs:1838-1840` (`phase_commit_count`'s "Every consumer treats all three
the same way") and `pipeline_outcomes.rs:337-340` (`handle_validate_outcome`'s over-promising
guarantee). Both must be corrected in the same commit as their code fix — ROADMAP criterion 1 names
the first explicitly; CONTEXT.md's discretion item names both.

---

## No Analog Found / Genuine Gaps

### The `NoGitPath` primitive does not exist anywhere yet — build it fresh (not a true "no analog," but flagged since nothing currently exercises a failing `git` spawn)
Confirmed this session: `rg -n "set_var\(\"PATH\"" crates/devflow-core/src/` returns **zero** hits,
and `NeutralPath` is the only PATH-replacing RAII guard in the workspace. This is Section 1 above —
build `NoGitPath` beside `NeutralPath` in `crates/devflow-cli/src/test_support.rs`, following its
shape exactly.

### Crate-boundary gap: `evaluate_layer2`'s test (999.87) cannot reuse `NoGitPath` at all
**This is the most important gap this pattern-mapping pass surfaced, and it is not called out as a
concrete blocker anywhere in CONTEXT.md or RESEARCH.md.** `evaluate_layer2` and
`phase_commit_count` live in `crates/devflow-core/src/agent_result.rs` — the **core** crate.
`NoGitPath` (like `NeutralPath`, `ENV_MUTEX`, `stub_agent_binary`) lives in
`crates/devflow-cli/src/test_support.rs` — the **cli** crate, which *depends on* `devflow-core`,
not the other way around. A `devflow-core` test cannot import a `devflow-cli` test helper.

Confirmed by reading `crates/devflow-core/src/test_support.rs` in full: it has no `Mutex`, no
`PATH` mutation, and no RAII env guard of any kind — only `wait_for_exec_visibility` (an unrelated
`/proc`-polling barrier) and re-exports of `git_command`/`hermetic_command`.

**There IS in-crate precedent for a `devflow-core`-local env mutex**, just not for `PATH`:
`crates/devflow-core/src/gates.rs:374` and `crates/devflow-core/src/config.rs:274` each declare
their own **module-scoped** `static ENV_MUTEX: Mutex<()> = Mutex::new(())` inside their own
`#[cfg(test)] mod tests`, guarding a different env var each (`DEVFLOW_GATE_NOTIFY_CMD`-class and
`DEVFLOW_CAPTURE_RETENTION`-class respectively). (Citation correction: `devflow-cli/test_support.rs`'s
own doc comment cites these as `gates.rs:348`/`config.rs:174`; the actual lines, confirmed this
session, are `gates.rs:374` and `config.rs:274` — a small drift in that doc comment itself, not in
CONTEXT.md/RESEARCH.md.)

**Consequence for the planner:** RESEARCH's "Wave 0 Gaps" list says 999.77's test "may need to live
in `pipeline_outcomes.rs` instead" of `agent_result.rs` (an escape hatch, since
`handle_validate_outcome` is `devflow-cli`-side and CAN use `NoGitPath`) — but 999.87's test has no
such escape hatch: `evaluate_layer2` is `devflow-core`-side by definition, so its
unrunnable-git regression test must either (a) build a **second**, `devflow-core`-local
PATH-replacing guard (own module-scoped `Mutex<()>`, following `gates.rs`/`config.rs`'s precedent
for the mutex and `NeutralPath`'s precedent for the RAII shape), or (b) drive the assertion
indirectly through `handle_validate_outcome` (which does live in `devflow-cli` and can use the real
`NoGitPath`) instead of calling `evaluate_layer2` directly — accepting a less direct test. This is
a real design decision the plan must make explicitly; it is not resolved by anything already in
CONTEXT.md or RESEARCH.md.

### No existing test anywhere spawns `ssh-keygen -Y sign` directly
`git.rs`'s current test module (999-2083) exercises `classify_ssh_add_status`,
`inline_key_fingerprint`, and the two-arg `check_ssh_signing_viability`/`check_signing_viability`
dispatch, all against real generated keys — but none of the existing fixtures spawn `-Y sign`
today. The closest analog for "generate a real ed25519 keypair and probe against it" is
`inline_key_fingerprint_matches_the_path_branch_for_the_same_key`'s `ssh-keygen -t ed25519 -f
<path> -N "" -q` fixture setup (git.rs:1996-2015) and `release_check.rs`'s identical
key-generation shape (256-282) — reuse the keypair-generation shape, not the assertion shape.

---

## Metadata

**Analog search scope:** `crates/devflow-cli/src/test_support.rs`,
`crates/devflow-core/src/{state,mode,agent_result,git,canary,agent,verify,test_support}.rs`,
`crates/devflow-cli/src/{pipeline_outcomes,pipeline_launch,pipeline_gate,commands}.rs`,
`crates/devflow-cli/tests/release_check.rs`. No `Glob`/directory-wide pattern search was needed —
every file in scope was named explicitly by CONTEXT.md/RESEARCH.md, and this phase adds no new
production files.
**Files scanned:** 13, all read this session (not grepped-and-inferred), each cross-checked against
CONTEXT.md's/RESEARCH.md's own citations.
**Citation corrections found (all immaterial to shape, recorded per the phase's own instruction to
verify every line number):**
- `commands.rs::check_signing`: cited as `:2379-2400`; the `fn` itself starts at `:2380` (2379 is
  the last line of its doc comment).
- `verify.rs`'s two root-sensitivity tests: cited by CONTEXT.md as `:351`/`:376`; confirmed exact at
  `:351`/`:377`.
- `devflow-cli/test_support.rs`'s own doc comment cites `gates.rs:348`/`config.rs:174` for the two
  other `ENV_MUTEX` instances; confirmed exact at `gates.rs:374`/`config.rs:274`. This drift is in
  the source's own doc comment, not in CONTEXT.md/RESEARCH.md.
Every other `file:line` citation in CONTEXT.md and RESEARCH.md that this pass checked (the
`NeutralPath`/`env_lock`/`ENV_MUTEX` family, `State`/`State::new`/the serde pair, `mode.rs`'s three
functions, `pipeline_gate.rs::transition`, `pipeline_outcomes.rs`'s baseline write and gate
message, `pipeline_launch.rs`'s call site and both `advance()` tests, `agent_result.rs`'s
`phase_commit_count`/`evaluate_layer2`/`phase_verification_exists`, `canary.rs::reap`,
`agent.rs::terminate_and_verify`, and every named symbol in `git.rs`'s signing section) was
confirmed byte-for-byte against current source.
**Pattern extraction date:** 2026-08-06
