# Phase 48: Survivable State Writes and Honest Gate Recovery - Pattern Map

**Mapped:** 2026-09-14
**Files analyzed:** 27 (new or modified; test-module groups counted once)
**Analogs found:** 27 / 27 have at least a partial analog. 4 sub-mechanisms have none (see "No Analog Found")

**Requirement IDs:** SURV-01, SURV-02, CHKPT-01 (999.125), CHKPT-02 (999.126), TEST-01 (999.38 + 999.80), per 48-CONTEXT.md D-11.

**Line-number provenance.** Every `file:line` below was read this session at `feature/phase-48` HEAD `2be3907`.
`git log fbf6e98..HEAD -- crates` is empty, so CONTEXT and RESEARCH citations into `crates/` still match. Line
numbers drift after the first Phase 48 commit, so cite symbols in acceptance gates (RESEARCH Pitfall 9).

**Tracked-source gate.** Every analog path below was confirmed with `git ls-files` (all tracked). The control
behaved as expected: the untracked `.planning/research/.cache` printed nothing. No path below is a gitignored mirror.

---

## File Classification

| # | New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|-------------------|------|-----------|----------------|---------------|
| 1 | `crates/devflow-core/src/workflow.rs` (`write_state_atomic`, `clear_state` temp sweep) | utility (persistence) | file-I/O | `crates/devflow-core/src/registry.rs:214-229` (`write_atomic`, unique temp) | exact |
| 2 | `crates/devflow-core/src/gates.rs` (`respond` exclusive publish, `cleanup` temp sweep) | service (gate protocol) | file-I/O, request-response | self (`respond` :186-205) + `registry.rs:222-229` + `lock.rs:85-90` (`create_new` / `AlreadyExists`) | role-match |
| 3 | `crates/devflow-core/src/lock.rs` (per-phase blocking acquire) | utility (lock) | event-driven (poll) | self `acquire_project_blocking` :55-74 | exact |
| 4 | `crates/devflow-core/src/verify.rs` (shared checkpoint parser) | utility (parser) | transform | self `phase_has_human_only_checkpoint` :207-220 | role-match (fence tracking has no analog) |
| 5 | `crates/devflow-core/src/state.rs` (approved-checkpoint field) | model | CRUD (serde) | self `base_branch` :342-355, `canary` :356-373 | exact |
| 6 | `crates/devflow-core/src/recover.rs` (`clean_phase` gate cleanup under R-1) | service | file-I/O | self `clean` :87-125 (liveness skip) + `lock::remove_stale_locks` :258-285 | exact |
| 7 | `crates/devflow-core/src/monitor.rs` (`--stage` in Legacy tail + `__monitor` args) | service (spawner) | event-driven | self `advance_tail` :473-482, `__monitor` args :423-444 | exact |
| 8 | `crates/devflow-core/src/test_support.rs` (generic child-process test helper) | test utility | request-response (child process) | `crates/devflow-cli/src/test_support.rs:411-489` | exact (cross-crate) |
| 9 | `crates/devflow-core/src/agents/pi.rs`, `agents/opencode.rs` (tests: `PathGuard` to child) | test | request-response | `crates/devflow-cli/src/test_support.rs:752-803` (parent/child test) | exact |
| 10 | `crates/devflow-core/src/config.rs`, `agents/pi.rs` `EnvGuard`, cli env guards (`#[expect]` annotations) | test | n/a | none in repo (`#[expect(` count is 0); `config.rs:494-527` is the target shape | partial |
| 11 | `crates/devflow-cli/src/commands.rs::start` (take per-phase lock) | controller (CLI verb) | request-response | `pipeline_launch.rs::resume` :1293-1301 | exact |
| 12 | `crates/devflow-cli/src/commands.rs` `stop` / `stop_via_gate` / `stop_via_lock` / `persist_stopped_state` | controller | request-response, file-I/O | self :1787-1947 + `pipeline_gate.rs::ship_override` :518-529 (lock-then-load) | exact |
| 13 | `crates/devflow-cli/src/commands.rs` `gate_respond` / `gate_sweep` + shared no-waiter message fn | controller | request-response | `stop_via_lock` identity match :1853-1914 | role-match (shared message fn: partial) |
| 14 | `crates/devflow-cli/src/commands.rs` doctor (`PhaseFacts`, new finding, repair strings) | controller (report) | transform | self `check_orphan_gate` :3222-3239, `build_phase_facts` :3368-3410 | exact |
| 15 | `crates/devflow-cli/src/pipeline_launch.rs::advance` (bounded wait + stage binding) + `run_monitor` | controller (hidden verb) | event-driven | self :1452-1496 (`advance_failed` :1468-1473) | exact |
| 16 | `crates/devflow-cli/src/pipeline_launch.rs` `Action::GateReview` re-scan gate | service (policy) | request-response | self :1567-1624 + `preflight.rs::run_preflight` gate arms :1367-1385 | role-match |
| 17 | `crates/devflow-cli/src/preflight.rs::run_preflight` (record checkpoint set) | service | CRUD | self :1368-1377 (Advance arm save) | exact |
| 18 | `crates/devflow-cli/src/main.rs` (`--stage` on `Advance` and `__monitor`) | config (clap) | request-response | self `Advance` :117-127, `Monitor` :134-160, dispatch :577-594 | exact |
| 19 | `crates/devflow-cli/src/test_support.rs` (generalize/re-export helper, retire `NeutralPath`) | test utility | request-response | self :328-489 | exact |
| 20 | cli test modules: `pipeline_launch.rs`, `pipeline_outcomes.rs`, `pipeline_gate.rs`, `preflight.rs`, `staleness.rs` (PATH blocks + 26 `abort: test cleanup` fixtures) | test | request-response | before: `pipeline_launch.rs:3612-3628`; after: `test_support.rs:752-803` | exact |
| 21 | `crates/devflow-cli/tests/gate_wedge_e2e.rs` (**new**, #200 both arms) | test (e2e) | request-response, event-driven | `crates/devflow-cli/tests/gate_sweep_e2e.rs:274-362` + `stop_e2e.rs:136-218` | exact |
| 22 | `crates/devflow-cli/tests/stop_e2e.rs` (live-holder refusal, F-1 path) | test (e2e) | request-response | self :136-256 | exact |
| 23 | `crates/devflow-cli/tests/start_reachability_e2e.rs` or a new start-lock e2e | test (e2e) | request-response | self :1-60 (`fake_bin_dir`) + `gate_sweep_e2e.rs:292-319` (live holder) | role-match |
| 24 | `clippy.toml` (**new**, workspace root) | config | n/a | `Cargo.toml:43-46` (`[workspace.lints.clippy]`) | partial |
| 25 | `.planning/codebase/TESTING.md` § Environment Mutation Rule | docs | n/a | self :88-96 | exact |
| 26 | `.planning/ROADMAP.md` (Phase 48 IDs and criteria 1-3, 999.38/999.80 note) | docs | n/a | self :30, :155, :175, :178, :180 | exact |
| 27 | `.planning/REQUIREMENTS.md` (CHKPT-01/02, TEST-01 entries + traceability) | docs | n/a | self SURV section :63-80, traceability :162-175 | exact |

---

## Pattern Assignments

### 1. `crates/devflow-core/src/workflow.rs` (utility, file-I/O) — SURV-01 D-02.4, R-7

**Analog:** `crates/devflow-core/src/registry.rs:214-229`. It is the repo's existing unique-temp atomic write, and
its doc comment names the fixed-`.tmp` hazard this phase closes:

```rust
/// Write `contents` to `path` atomically: write a uniquely-named temp file
/// in the same directory, then `rename` over the target so a reader never
/// observes a partial file. The temp name is unique per call (process id +
/// a monotonic counter), unlike `gates.rs::write_atomic`'s fixed `.tmp`
/// suffix — ...
fn write_atomic(path: &Path, contents: &str) -> Result<(), RegistryError> {
    static TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp.{}.{n}", std::process::id()));
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
```

**Code to replace** (`workflow.rs:183-193`):
```rust
fn write_state_atomic(path: &Path, contents: &str) -> Result<(), WorkflowError> {
    if let Some(parent) = path.parent() {
        ensure_devflow_dir(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
```

**Two differences from the registry analog, both required by RESEARCH:**
- Create the temp with `std::fs::File::create_new(&tmp)` instead of `std::fs::write`. `std::fs::write` follows a
  pre-planted symlink and truncates it (RESEARCH Security, "Pre-planted symlink"). In-repo `create_new` usage:
  `lock.rs:85` (`match File::create_new(&path)`), plus `workflow.rs:114` (`.create_new(true)`). `use std::io::Write;`
  is already imported at `workflow.rs:11`.
- Remove the temp when `rename` fails (RESEARCH Pattern 1).

**Temp name must not end in `.json` (F-11).** `path.with_extension("tmp.{pid}.{n}")` on `state-48.json` yields
`state-48.tmp.{pid}.{n}`, which starts with `state-` but does not end with `.json`. The scanner at `workflow.rs:221`
therefore skips it:
```rust
if !name.starts_with(STATE_FILE_PREFIX) || !name.ends_with(".json") {
    continue;
}
```
RESEARCH Pattern 1 proposes `.{name}.{pid}.{seq}.tmp` instead. Either passes F-11; the planner picks one name shape
and uses it in both `workflow.rs` and `gates.rs`, so one sweep pattern covers both (R-7).

**Error type:** `WorkflowError::Io(#[from] std::io::Error)` (`workflow.rs:17-20`); `?` converts.

**Orphan sweep (R-7) goes in `clear_state`** (`workflow.rs:257-273`). Copy the directory-scan shape from
`lock::remove_stale_locks` (`lock.rs:258-285`: `read_dir`, `name.to_str()`, prefix filter, collect warnings rather than
fail):
```rust
pub fn remove_stale_locks(project_root: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    let devflow_dir = project_root.join(".devflow");
    let Ok(entries) = fs::read_dir(&devflow_dir) else {
        return warnings;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(LOCK_FILE_PREFIX) {
            continue;
        }
        ...
        if let Err(err) = fs::remove_file(&path) {
            warnings.push(format!("could not remove {}: {err}", path.display()));
        }
    }
    warnings
}
```
Scope the sweep to the cleared phase's own temp prefix (`state-NN.tmp.` or the chosen shape). A sweep across all phases
could delete a sibling phase's in-flight temp under `devflow parallel`.

**Test to add** (RESEARCH Pitfall 4): plant a temp next to a real state file. Assert `list_states` returns only the
real one (control: the real file still lists), and that `clear_state` removes the planted temp.

---

### 2. `crates/devflow-core/src/gates.rs` (service, file-I/O) — SURV-02 D-04, R-4, R-7

**Analog:** self. The code to change is `respond` (`gates.rs:186-205`):
```rust
pub fn respond(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
    response: &GateResponse,
) -> Result<PathBuf, GateError> {
    if !Self::gate_path(project_root, phase, stage).exists() {
        return Err(GateError::NoOpenGate { phase, stage });
    }
    let path = Self::response_path(project_root, phase, stage);
    if path.exists() {
        return Err(GateError::AlreadyResponded { phase, stage });
    }
    write_atomic(&path, &serde_json::to_string_pretty(response)?)?;
    info!(
        "gate response written for phase {phase} {stage}: approved={}",
        response.approved
    );
    Ok(path)
}
```

**Exclusive-publish idiom, copied from `lock.rs:85-90`** (the repo's `AlreadyExists` match):
```rust
match File::create_new(&path) {
    Ok(mut f) => { ... }
    Err(err) if err.kind() == io::ErrorKind::AlreadyExists => { ... Err(LockError::Contended { pid, path }) }
    Err(err) => Err(err.into()),
}
```
Apply the same three-arm match to `std::fs::hard_link(&tmp, &path)`:
- `AlreadyExists` → `GateError::AlreadyResponded { phase, stage }` (`gates.rs:95-97`).
- any other error → `GateError::Io` (`gates.rs:87-88`). No rename fallback (RESEARCH Pattern 2: a fallback would
  bring D-04 back on filesystems without hard links).

Keep the `path.exists()` pre-check at :196-198 as a fast path. Unlink the temp in every arm.

**R-4 scope:** only `respond` changes. `write_gate` (:234-250) and `ack` (:284-291) keep `write_atomic` (:356-364) with
rename-overwrite. Change only `write_atomic`'s temp name to the shared unique shape from File 1:
```rust
fn write_atomic(path: &Path, contents: &str) -> Result<(), GateError> {
    if let Some(parent) = path.parent() {
        crate::workflow::ensure_devflow_dir(parent)?;
    }
    let tmp = path.with_extension("tmp");   // <- fixed name, replace
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
```

**F-11 scanner constraint** (`gates.rs:155-160`): a temp ending in `.json` would be listed as an open gate.
```rust
if !name.ends_with(".json")
    || name.ends_with(".response.json")
    || name.ends_with(".ack.json")
{
    continue;
}
```
`with_extension("tmp.{pid}.{n}")` on `48-code.response.json` yields `48-code.response.tmp.{pid}.{n}`, which is safe.

**R-7 sweep goes in `cleanup`** (`gates.rs:294-305`). It is idempotent today; extend it to remove this phase+stage's
temp orphans:
```rust
pub fn cleanup(project_root: &Path, phase: PhaseId, stage: Stage) -> Result<(), GateError> {
    for path in [
        Self::gate_path(project_root, phase, stage),
        Self::response_path(project_root, phase, stage),
        Self::ack_path(project_root, phase, stage),
    ] {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}
```

**Test analog** (`gates.rs:551-567`; extend it into the two-responder test with distinct bytes):
```rust
#[test]
fn respond_refuses_to_clobber_unconsumed_response() {
    let dir = tempfile::tempdir().unwrap();
    Gates::write_gate(dir.path(), PhaseId::new(4), Stage::Validate, "ctx").unwrap();
    let response = GateResponse { approved: true, note: None, responded_by: None };
    Gates::respond(dir.path(), PhaseId::new(4), Stage::Validate, &response).unwrap();

    let err =
        Gates::respond(dir.path(), PhaseId::new(4), Stage::Validate, &response).unwrap_err();
    assert!(
        matches!(err, GateError::AlreadyResponded { phase, .. } if phase == PhaseId::new(4))
    );
}
```
The new test gives the two responders different `approved` values and asserts the first responder's bytes survive.
This existing test cannot tell rename-overwrite from exclusive publish when the pre-check fires first. The new test
also needs a case that skips the pre-check, for example two threads racing past `exists()`, or a direct call to the
publish helper.

---

### 3. `crates/devflow-core/src/lock.rs` (utility, poll) — SURV-01 D-02.2

**Analog:** self, `acquire_project_blocking` (`lock.rs:50-74`). Copy it, calling `acquire(project_root, phase)`:
```rust
pub fn acquire_project_blocking(
    project_root: &Path,
    timeout: std::time::Duration,
) -> Result<LockGuard, LockError> {
    let start = std::time::Instant::now();
    let mut backoff = std::time::Duration::from_millis(100);
    loop {
        match acquire_project(project_root) {
            Ok(guard) => return Ok(guard),
            Err(err @ LockError::Contended { .. }) => {
                if start.elapsed() >= timeout {
                    return Err(err);
                }
                std::thread::sleep(backoff.min(timeout.saturating_sub(start.elapsed())));
                backoff = (backoff * 2).min(std::time::Duration::from_secs(2));
            }
            Err(err) => return Err(err),
        }
    }
}
```
Every retry goes through `acquire_path` (:76-122), which already reclaims a dead holder's lock at :100-117. The wait
therefore also covers F-1's "signalled holder exits" case with no extra code.

**Test analogs** (`lock.rs:437-464`). Mirror both with `acquire` / `acquire_blocking`. The in-process holder is valid
for contention (RESEARCH Pattern 3). It does not prove a foreign writer is excluded (Pitfall 1).
```rust
#[test]
fn project_lock_blocking_waits_for_release() {
    let dir = tempfile::tempdir().unwrap();
    let held = acquire_project(dir.path()).expect("first acquire");
    let root = dir.path().to_path_buf();
    std::thread::scope(|scope| {
        let waiter = scope
            .spawn(move || acquire_project_blocking(&root, std::time::Duration::from_secs(10)));
        std::thread::sleep(std::time::Duration::from_millis(300));
        drop(held);
        waiter.join().expect("waiter thread")
            .expect("blocking acquire must succeed once the holder releases");
    });
}

#[test]
fn project_lock_blocking_times_out_against_live_holder() {
    let dir = tempfile::tempdir().unwrap();
    let _held = acquire_project(dir.path()).expect("first acquire");
    let err = acquire_project_blocking(dir.path(), std::time::Duration::from_millis(300))
        .expect_err("must time out while the live holder keeps the lock");
    assert!(matches!(err, LockError::Contended { .. }));
}
```

**Waiter-identity primitives reused by Files 12, 13, 14 and 6** (do not reimplement them):
- `lock::holder(project_root, phase) -> Option<(String, PathBuf)>` (:198-211)
- `lock::holder_identity(project_root, phase) -> Option<(u32, Option<u64>)>` (:178-182). A `None` start time is a
  legacy lock, so identity cannot be confirmed (:174-177).
- `agent::is_same_process(pid, expected_start) -> bool` (`agent.rs:217-219`), which is Linux `/proc` only
  (`agent.rs:203-210`).
- `agent::TERMINATE_VERIFY_WAIT` = 3 s (`agent.rs:86`). This is the F-1 retry window RESEARCH Open Question 2
  recommends.

---

### 4. `crates/devflow-core/src/verify.rs` (utility, transform) — CHKPT-01 D-06, R-3

**Analog:** self. Both predicates to unify:

`phase_has_blocking_human_checkpoint` (`verify.rs:131-137`) is unanchored whole-file `contains`. This is the CHKPT-01
defect on the resume route.
```rust
pub fn phase_has_blocking_human_checkpoint(project_root: &Path, phase: PhaseId) -> bool {
    const HUMAN_BLOCKING_GATE: &str = r#"gate="blocking-human""#;
    phase_plan_files(project_root, phase)
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .any(|contents| contents.contains(HUMAN_BLOCKING_GATE))
}
```

`phase_has_human_only_checkpoint` (`verify.rs:207-220`) is line-anchored but uses `contains("<task")`, not a line-start
match (F-4), and has no fence tracking:
```rust
const HUMAN_ONLY_CHECKPOINT_MARKERS: [&str; 2] = [
    concat!("gate=", "\"", "blocking-human", "\""),
    concat!("type=", "\"", "checkpoint:human-action", "\""),
];
const TASK_ELEMENT_OPENING: &str = "<task";

pub fn phase_has_human_only_checkpoint(project_root: &Path, phase: PhaseId) -> bool {
    phase_plan_files(project_root, phase)
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .any(|contents| contents.lines().any(line_declares_human_only_checkpoint))
}

fn line_declares_human_only_checkpoint(line: &str) -> bool {
    line.contains(TASK_ELEMENT_OPENING)
        && HUMAN_ONLY_CHECKPOINT_MARKERS
            .iter()
            .any(|marker| line.contains(marker))
}
```

**Keep, verbatim:**
- the two marker literals and their "closing quote is load-bearing" doc (:139-158)
- plan discovery through `phase_plan_files(project_root, phase)` (:37)
- caller-owned root resolution (doc :123-130, :203-206; contract C-9)
- both public names and signatures. Callers are `preflight.rs:1076` and `pipeline_launch.rs:1595`.

**Replace:**
- doc paragraph :185-189 (known limit)
- doc paragraph :191-196 (stale "fails SAFE" rationale; D-06)

**Test fixture conventions** (`verify.rs:329-342`). Marker values are assembled at runtime so the `.rs` file never holds
the raw literal, and plan files are written under `.planning/phases/<dir>/`:
```rust
// The gate value is assembled at runtime from this const, not written as a
// literal in fixture bodies below, so this test file itself never contains
// the raw `gate="blocking-human"` string (28-01 Task 2 action note).
const HUMAN_GATE_VALUE: &str = "blocking-human";
const PLAIN_GATE_VALUE: &str = "blocking";
const HUMAN_ACTION_TYPE_VALUE: &str = "checkpoint:human-action";

fn write_phase_file(root: &std::path::Path, phase_dir: &str, file_name: &str, contents: &str) {
    let dir = root.join(".planning/phases").join(phase_dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(file_name), contents).unwrap();
}
```

**Test to flip** (`verify.rs:608-626`). Change the assertion to `!phase_has_human_only_checkpoint(...)`, rename the
test, and rewrite its doc comment:
```rust
#[test]
fn human_only_checkpoint_still_matches_a_task_tag_inside_a_fenced_example() {
    let dir = tempfile::tempdir().unwrap();
    let body = format!(
        "---\nphase: 92\n---\n\n\
         Here is what such a task looks like:\n\n\
         ```xml\n\
         <task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE}\">\n\
         </task>\n\
         ```\n"
    );
    write_phase_file(dir.path(), "92-probe", "92-01-PLAN.md", &body);
    assert!(
        phase_has_human_only_checkpoint(dir.path(), PhaseId::new(92)),
        ...
    );
}
```

**Model for the new acceptance pair:** `human_only_checkpoint_ignores_a_marker_mentioned_only_in_prose` (:571-592).
Apply the same fixture to `phase_has_blocking_human_checkpoint`, which matches it today (red). Also add the R-3 negative:
an unterminated fence placed before a real task must still match.

**Real-plan fixtures (F-3):**
- task-line declarations: `19-05-PLAN.md:83`, `19-11-PLAN.md:160`, `44-04-PLAN.md:168`, `15-05-PLAN.md:73`
- prose-only mentions (must not match): `33-02`, `34-04`, `43-01`, `47-03`, `47-05`

---

### 5. `crates/devflow-core/src/state.rs` (model, serde) — CHKPT-02 D-07

**Analog:** self. The closest field is `base_branch: Option<String>` (`state.rs:342-355`). It is an `Option` field whose
doc states what `None` means and why the value is persisted rather than passed:
```rust
/// ... every later entry
/// point (`advance`, `resume`, and the checkout hooks that merge the
/// phase branch) runs in a SEPARATE process whose environment need not
/// carry the export. ...
///
/// `None` means EITHER "written by a binary predating this field" OR
/// "nothing was configured" — both fall back to the resolver, which is
/// the pre-existing behaviour.
#[serde(default)]
pub base_branch: Option<String>,
```
The new field's doc must separate `None` ("never recorded", which gates) from `Some(vec![])` ("recorded, none present").
That is RESEARCH Pattern 5's tri-state. Also add the field to `State::new` (the initializer block near :462-474, which
sets `preflight_retries: 0` and `checkpoint_resumes: 0`).

**Serde test analog:** `preflight_retries_round_trips_through_serde` (`state.rs:849-879`). It checks round-trip, key
presence, and a JSON without the field:
```rust
let json = serde_json::to_string(&state).unwrap();
assert!(json.contains("preflight_retries"), "preflight_retries must appear in persisted JSON");
let loaded: State = serde_json::from_str(&json).unwrap();
assert_eq!(loaded.preflight_retries, 2, ...);

let absent_json = r#"{
    "stage": "code",
    "phase": 1,
    "agent": "claude",
    "mode": "auto",
    "started_at": "0",
    "project_root": "/repo"
}"#;
let loaded_absent: State = serde_json::from_str(absent_json).unwrap();
assert_eq!(loaded_absent.preflight_retries, 0);
```
For the new field, assert `None` on `absent_json`, and that `Some(vec![])` round-trips as `Some(vec![])`, not `None`.

---

### 6. `crates/devflow-core/src/recover.rs` (service, file-I/O) — SURV-02 D-05.4, R-1, R-7

**Analog:** self, `clean` (`recover.rs:87-104`). It already skips phases whose process is alive, with a warning:
```rust
for state in workflow::list_states(project_root) {
    let phase = state.phase;
    if agent_pid_for(&state).is_some_and(crate::agent::agent_running) {
        warnings.push(format!(
            "kept phase {phase} — its agent is still running (clear explicitly with --phase {phase})"
        ));
        continue;
    }
    ...
    workflow::clear_state(project_root, phase)?;
}
```

**Code to change** (`recover.rs:130-145`). Today it checks only the agent pid and never calls `Gates::cleanup`:
```rust
pub fn clean_phase(project_root: &Path, phase: PhaseId) -> Result<Vec<String>, RecoverError> {
    let mut warnings = Vec::new();
    if let Ok(state) = workflow::load_state(project_root, phase)
        && agent_pid_for(&state).is_some_and(crate::agent::agent_running)
    {
        warnings.push(format!(
            "phase {phase}'s agent appears to still be running — cleared anyway (explicit --phase)"
        ));
    }
    workflow::clear_state(project_root, phase)?;
    if let Err(err) = crate::ship::delete_cron_instructions(project_root, phase) {
        warnings.push(format!("could not remove cron-instructions: {err}"));
    }
    warnings.append(&mut crate::lock::remove_stale_locks(project_root));
    Ok(warnings)
}
```
**R-1:** call `crate::gates::Gates::cleanup` for each stage only when no live process holds `.devflow/lock-NN`. Use
`lock::holder` plus the pid liveness check that `remove_stale_locks` already applies (`lock.rs:272-278`). While a holder
is alive, push a warning and leave the gate files. Iterating stages means the `Stage` enum's variants; `abort` cleans
only `state.stage` (`pipeline_gate.rs:465`). If state is already gone, iterate every stage.

**Test fixture analog** (`recover.rs:204-231`): `state_aged_phase(...)` and `const DEAD_PID: u32 = 0x7FFF_FFFE;`. For the
live-holder arm, hold `crate::lock::acquire(root, phase)` in-process. The holder is the test pid, which is alive, so this
exercises "keep while alive". It does not exercise foreign identity, and does not need to.

---

### 7. `crates/devflow-core/src/monitor.rs` (service, spawner) — SURV-01 R-2, F-8

**Analog:** self. The Legacy tail (`monitor.rs:473-482`):
```rust
let advance_tail = if run_advance {
    format!(
        "; {binary} advance {project_root} --phase {phase}",
        binary = shell_escape(&binary),
        project_root = shell_escape(project_root),
        phase = state.phase,
    )
} else {
    String::new()
};
```
Append `--stage {stage}`, where `stage = state.stage`, the stage being launched. `Stage`'s `Display` output is what clap
parses back, as with `--agent` below.

The PipeOwning `__monitor` args (`monitor.rs:423-444`) follow the same `.arg("--flag").arg(value.to_string())` shape:
```rust
let child = hermetic_command(&binary, workdir_path)
    .arg("__monitor")
    .arg("--project")
    .arg(project_root)
    .arg("--phase")
    .arg(state.phase.to_string())
    ...
    .arg("--agent")
    .arg(state.agent.to_string())
    .arg("--")
```
Insert `--stage` before `--`; everything after `--` is the child's argv (`main.rs:157-159`). Existing script-shape tests
in this file assert on the rendered script, so extend one to require `--stage`.

---

### 8. `crates/devflow-core/src/test_support.rs` (test utility, child process) — TEST-01 D-08

**Analog:** `crates/devflow-cli/src/test_support.rs:411-426` (`run_test_without_git`):
```rust
pub(crate) fn run_test_without_git(test_name: &str, root: &Path) -> std::process::Output {
    // Deliberately empty. Nothing is written into it, by design.
    let empty = tempfile::tempdir().unwrap();
    let exe = std::env::current_exe().expect("current_exe for child test re-invocation");
    let output = std::process::Command::new(&exe)
        .arg(test_name)
        .arg("--exact")
        .arg("--test-threads=1")
        .env("PATH", empty.path())
        .env(CHILD_NO_GIT_ROOT, root)
        .output()
        .expect("spawn child test process with an empty PATH");
    // Explicit, not incidental: the empty directory must outlive the child.
    drop(empty);
    output
}
```
Also copy `assert_child_ran_exactly_one_passing_test` (`crates/devflow-cli/src/test_support.rs:449-489`). It makes four
assertions: exit 0; `test {name} ... ok`; `1 passed`; a non-zero `filtered out` count.

**Precedent inside core:** `crates/devflow-core/src/test_support.rs:181-190` says a core test needing an empty `PATH`
"should copy that approach", meaning the cli child pattern.

**VERIFIED compile hazard: no `tempfile` in this module outside `#[cfg(test)]`.** The module is gated
`#[cfg(any(test, feature = "test-support"))]` (`crates/devflow-core/src/lib.rs:80-82`). The feature is
`test-support = []` (`crates/devflow-core/Cargo.toml:18`) and brings no optional dependencies. `tempfile = "3"` is a
`[dev-dependencies]` entry only (`Cargo.toml:30-31`). devflow-cli enables the feature on its dev-dependency
(`crates/devflow-cli/Cargo.toml:27`). In that build devflow-core compiles as a library, without its own dev-dependencies,
so a `tempfile::tempdir()` call in a `pub fn` here fails to compile. `rg tempfile` over the file returns 0 hits today.
**The generalized helper takes the `PATH` directory as `&Path` and the caller owns the `TempDir`.** This matches D-08's
"caller-built PATH" and RESEARCH Pattern 8.

**Do NOT copy the weaker in-core re-exec** at `crates/devflow-core/src/monitor.rs:2854-2882`. It uses a substring filter
without `--exact` and checks only `1 passed` plus exit status:
```rust
let out = std::process::Command::new(&exe)
    // Substring filter, NOT `--exact`: ...
    .arg("spawn_monitor_agent_git_calls_resolve_workdir_not_a_hostile_git_dir")
    .arg("--test-threads=1")
    .env(INNER_ROOT, root.to_str().unwrap())
    .env("GIT_DIR", foreign.path().join(".git"))
    .output()
```
Its comment notes that `--exact` on a bare name is a false green. The cli helper avoids that by passing the
module-qualified name (`test_support.rs:753-754`) together with `--exact`. Use that form.

---

### 9. `crates/devflow-core/src/agents/pi.rs`, `agents/opencode.rs` (test) — TEST-01

**Before shape** (`pi.rs:287-307`, used at 8 call sites from :342; `opencode.rs:423-440`, 7 call sites):
```rust
struct PathGuard { original: Option<std::ffi::OsString> }
impl PathGuard {
    fn set(path: &std::path::Path) -> Self {
        let original = std::env::var_os("PATH");
        // SAFETY: held under ENV_MUTEX; no other thread reads/writes PATH.
        unsafe { std::env::set_var("PATH", path) };
        Self { original }
    }
}
impl Drop for PathGuard { fn drop(&mut self) { match &self.original {
    Some(prev) => unsafe { std::env::set_var("PATH", prev) },
    None => unsafe { std::env::remove_var("PATH") },
} } }
```
Typical use (`pi.rs:338-354`):
```rust
#[test]
fn preflight_invokes_pi_auth_check_and_accepts_ready() {
    let _guard = ENV_MUTEX.lock().unwrap();
    let stub_dir = stub_pi_with_provider(r#"{"status":"ready"}"#, 0, "litellm");
    let _path = PathGuard::set(stub_dir.path());
    let _cfgdir = EnvGuard::set("PI_CODING_AGENT_DIR", stub_dir.path());
    PiDriver.health(&test_state()).expect("a `ready` stub should pass preflight");
    ...
}
```

**After shape.** Copy the parent/child split from `crates/devflow-cli/src/test_support.rs:752-803`:
```rust
#[test]
fn an_empty_path_child_cannot_resolve_git_while_the_parent_still_can() {
    const NAME: &str = "test_support::tests::\
                        an_empty_path_child_cannot_resolve_git_while_the_parent_still_can";

    if let Some(root) = child_no_git_root() {
        // CHILD half: `PATH` is ... set by the parent on this process's `Command` only.
        ...assertions...
        return;
    }

    // PARENT half.
    let _env = env_lock();
    let dir = tempfile::tempdir().unwrap();
    ...control...
    let out = run_test_without_git(NAME, dir.path());
    assert_child_ran_exactly_one_passing_test(&out, NAME);
    ...control...
}
```
The child half builds its fixture from a directory the parent passes in. The stub directory is created in the parent
and handed over through the child-mode env var.

**`EnvGuard` (`pi.rs:311-332`)** sets `PI_CODING_AGENT_DIR`, which is not `PATH`. Either move it into the child `Command`
as well, which is free because the child already exists, or keep it with an `#[expect]` (File 10). Moving it is
recommended: it removes an `#[expect]` and the env helper's premise at the same time.

---

### 10. `#[expect(clippy::disallowed_methods, reason = "...")]` targets (test) — TEST-01 D-09

**No in-repo analog:** `#[expect(` occurs 0 times in `crates/` (CONTEXT D-09).

**Target shape** (`crates/devflow-core/src/config.rs:494-527`). Each `unsafe { std::env::set_var/remove_var }` line inside
the guard is one site:
```rust
static ENV_MUTEX: Mutex<()> = Mutex::new(());

struct EnvOverride(&'static str);

impl EnvOverride {
    fn set(key: &'static str, value: &str) -> Self {
        // SAFETY: Tests that mutate this process-global variable are
        // serialized by ENV_MUTEX and the guard removes it on drop.
        unsafe { std::env::set_var(key, value) };
        Self(key)
    }
    fn clear(key: &'static str) -> Self {
        unsafe { std::env::remove_var(key) };
        Self(key)
    }
}
impl Drop for EnvOverride {
    fn drop(&mut self) {
        unsafe { std::env::remove_var(self.0) };
    }
}
```
Put the attribute on the smallest enclosing item: the `fn set` / `fn clear` / `fn drop`, or the statement. Keep it inside
`#[cfg(test)]` (RESEARCH Pitfall 6). The authoritative site list is `cargo clippy --workspace --all-targets --message-format=json`
output after `clippy.toml` lands, not an rg count (F-9).

---

### 11. `crates/devflow-cli/src/commands.rs::start` (controller) — SURV-01 D-02.1, F-5

**Analog:** `crates/devflow-cli/src/pipeline_launch.rs::resume` (`:1293-1301`), which takes the lock at entry and holds it
until return:
```rust
let _lock = match lock::acquire(project_root, phase) {
    Ok(guard) => guard,
    Err(lock::LockError::Contended { pid, path: _ }) => {
        return Err(CliError::Message(format!(
            "another devflow process (pid {pid}) is already running"
        )));
    }
    Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
};
```
`ship_override` (`pipeline_gate.rs:518-527`) has the more informative wording, naming the phase and why it refuses:
```rust
Err(lock::LockError::Contended { pid, .. }) => {
    return Err(CliError::Message(format!(
        "phase {phase}: another devflow process (pid {pid}) holds the per-phase lock — \
         refusing to race its poll of the Ship gate response"
    )));
}
```

**Placement (F-5).** Take the lock after the dry-run return (`commands.rs:355-358`) and before the first side effect,
`ensure_phase_worktree` (`:503`) or `feature_start` (`:521`):
```rust
if dry_run {
    print_dry_run(&state);
    return Ok(());
}
// <- take `_lock` here; it lives until `start` returns
```

**Contract to preserve** (C-1, WR-11, `commands.rs:661-682`): save before launch, clear on launch failure. Only the lock
lifetime around it changes:
```rust
workflow::save_state(&state)?;
events::emit(project_root, phase, "workflow_started", workflow_started_payload(&state));
if let Err(err) = launch_stage(&mut state, None, None) {
    if let Err(clear_err) = workflow::clear_state(project_root, phase) {
        eprintln!("warning: could not clear state after failed launch: {clear_err}");
    }
    return Err(err);
}
```

**D-05.4 fresh-start cleanup:** after the lock is taken, call `Gates::cleanup` for this phase's stages (R-1: `start` holds
the lock, so no live waiter can exist). The cleanup call shape is `pipeline_gate.rs:465`:
`let _ = Gates::cleanup(project_root, state.phase, state.stage);`.

---

### 12. `crates/devflow-cli/src/commands.rs` stop family (controller) — SURV-01 R-5, F-1; SURV-02 D-05, R-6

**Current flow** (`commands.rs:1787-1792`). `persist_stopped_state` runs even after the gate path succeeded; that is
D-01 hazard (b):
```rust
pub(crate) fn stop(project_root: &Path, phase: PhaseId) -> Result<(), CliError> {
    if !stop_via_gate(project_root, phase)? {
        stop_via_lock(project_root, phase)?;
    }
    persist_stopped_state(project_root, phase)
}
```

**Lock-then-load analog (R-5):** `pipeline_gate.rs::ship_override` (`:518-529`) acquires the lock and only then calls
`workflow::load_state`. `persist_stopped_state` (`commands.rs:1930-1947`) loads first today:
```rust
fn persist_stopped_state(project_root: &Path, phase: PhaseId) -> Result<(), CliError> {
    let mut state = match workflow::load_state(project_root, phase) {
        Ok(state) => state,
        Err(workflow::WorkflowError::MissingState(_)) => {
            println!("stop: no persisted state for phase {phase} — already stopped");
            return Ok(());
        }
        Err(err) => return Err(err.into()),
    };
    state.stopped = true;
    let reason = "stopped via `devflow stop`".to_string();
    state.stop_reason = Some(match state.stop_reason.take() {
        Some(existing) if !existing.is_empty() => format!("{existing}; {reason}"),
        _ => reason,
    });
    workflow::save_state(&state)?;
    Ok(())
}
```
Keep the `MissingState` arm and the append-not-overwrite reason logic (C-5). Wrap them in: lock (bounded retry after a
successful signal, F-1, using the File 3 helper with `agent::TERMINATE_VERIFY_WAIT`) → load → mutate → save → release.
When the lock is still held, print that the holder is alive and that state was not marked.

**Waiter identity check to reuse** (`stop_via_lock`, `commands.rs:1853-1914`; keep C-2/C-3 unchanged):
```rust
let Some((pid_str, _path)) = lock::holder(project_root, phase) else { ... };
let Ok(pid) = pid_str.parse::<u32>() else { ... };
if !agent::agent_running(pid) { ... stale lock ... }
match lock::holder_identity(project_root, phase) {
    Some((recorded_pid, Some(recorded_start))) if recorded_pid == pid => {
        if !agent::is_same_process(pid, recorded_start) { /* recycled pid: refuse */ }
    }
    Some((_, None)) => { /* legacy single-line lock: identity cannot be confirmed */ }
    _ => { /* unreadable */ }
}
```
Extract this matrix into one function returning a three-way result (live waiter / no waiter / unconfirmable), so
`stop_via_gate`, `gate_respond` and `gate_sweep` (File 13) and doctor (File 14) share it. `stop_via_lock` keeps its
signal-refusal wording for the two non-confirmed arms.

**`stop_via_gate` (`commands.rs:1806-1844`) — the claim to remove** (D-05.1):
```rust
Ok(path) => {
    println!(
        "stop: wrote a rejection for phase {phase} {} at {} — the process waiting \
         on it will pick this up on its next poll, within the 60s backoff cap",
        gate.stage,
        path.display()
    );
    Ok(true)
}
Err(GateError::AlreadyResponded { .. }) => { ... Ok(true) }   // C-16 benign race: keep
Err(GateError::NoOpenGate { .. }) => Ok(false),
```
Run the waiter check before `Gates::reap` (:1813). If no waiter exists at a non-Ship gate, write nothing (D-05.3) and
print the shared no-waiter message. At a Ship gate the write stays (C-6, `stop_e2e.rs:346-427`; RESEARCH Pitfall 8).

---

### 13. `crates/devflow-cli/src/commands.rs` `gate_respond` / `gate_sweep` (controller) — SURV-02 D-05, R-6

**`gate_respond` (`commands.rs:1381-1424`) — message to replace** (:1417-1422):
```rust
println!(
    "{} gate for phase {phase} {stage} — {outcome} once the waiting monitor polls it \
     (response at {})",
    if approved { "approved" } else { "rejected" },
    path.display()
);
```
Place the waiter check between stage resolution (:1388-1391) and `Gates::respond` (:1401). Keep the event emission
shape (:1402-1411):
```rust
events::emit(
    project_root,
    phase,
    "gate_response_written",
    serde_json::json!({ "stage": stage.to_string(), "approved": approved, "via": "cli" }),
);
```

**`gate_sweep` reap loop (`commands.rs:1496-1531`)** has no waiter check today (R-6). Insert the check before `Gates::reap`,
and on a no-waiter non-Ship gate count it `left_alone` with the shared message. The loop already classifies every gate
into `reaped` / `skipped` / `left_alone` (:1471-1473), so the new outcome fits that counter set:
```rust
match Gates::reap(
    project_root,
    gate.phase,
    gate.stage,
    "abort: reaped by devflow gate sweep (unattended gate exceeded max age)",
    "devflow-reap",
) {
    Ok(_) => { reaped += 1; events::emit(..., "gate_reaped", ...); println!(...); }
    Err(err) => { skipped += 1; println!("skipped ... — already answered ({err})"); }
}
```
C-15 / T-23-41: `Gates::reap` remains the sweep's only write path (`gates.rs:207-231`). `gate_sweep_e2e.rs:274-362`
drives a real live waiter and must stay green (control: the live-waiter arm still writes).

**Shared no-waiter message function (partial analog).** No existing function is shared by a CLI verb and doctor. Model
it on `PhaseFinding.repair: Option<String>` (`commands.rs:3195-3200`) so the verbs print it and doctor stores it:
- Ship gate → `devflow ship --phase N`
- other gates → `devflow resume --phase N` or `devflow recover --clean --phase N`
- unconfirmable identity → says a process holds the lock but claims neither a waiter nor its absence

---

### 14. `crates/devflow-cli/src/commands.rs` doctor (controller, report) — SURV-02 D-05.5, F-6

**Facts struct** (`commands.rs:3165-3190`). Add a waiter field with a doc comment like `monitor_pid`'s (:3171-3174):
```rust
pub(crate) struct PhaseFacts {
    pub(crate) phase: PhaseId,
    pub(crate) stage: Stage,
    pub(crate) gate_pending: bool,
    pub(crate) agent_pid: Option<u32>,
    pub(crate) agent_alive: bool,
    /// The monitor pid recorded in `State.monitor_pid` (18b). `None` means
    /// no monitor has been spawned for this state yet, or the state was
    /// written by a binary predating the field — never treated as a problem.
    pub(crate) monitor_pid: Option<u32>,
    ...
    pub(crate) open_gate_stages: Vec<Stage>,
    ...
    pub(crate) stopped: bool,
}
```
All I/O happens in `build_phase_facts` (`:3368-3410`, "collect all I/O; reconcile with zero I/O", doc :3162-3164). Compute
the waiter state there, never inside a `check_*` function.

**Check pattern** (`commands.rs:3222-3239`). Its repair string is the dishonest repair F-6 identifies:
```rust
fn check_orphan_gate(facts: &PhaseFacts) -> Option<PhaseFinding> {
    if facts.gate_pending || facts.open_gate_stages.is_empty() {
        return None;
    }
    let gate_stage = facts.open_gate_stages[0];
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Problem,
        detail: format!(
            "phase {}: gate open for stage {} but state.gate_pending is false",
            facts.phase, gate_stage
        ),
        repair: Some(format!(
            "devflow gate approve {} --stage {}",
            facts.phase, gate_stage
        )),
    })
}
```
`check_gate_pending_without_gate` (:3205-3218) hard-codes `devflow resume --phase {}`. The new "gate open, nothing
waiting" check uses the same `Option<PhaseFinding>` shape with `repair` from the shared message function (File 13). It
stays report-only (T-18-02, doc :3202-3204).

**Test analog** (`commands.rs:6026-6081`, module `doctor_reconciliation`). There is a baseline builder, and each case
overrides only the fields it needs:
```rust
fn agreeing_facts(phase: PhaseId) -> PhaseFacts {
    PhaseFacts { phase, stage: Stage::Code, gate_pending: false, agent_pid: Some(4242),
                 agent_alive: true, monitor_pid: Some(4343), monitor_alive: true,
                 last_event: Some("stage_launched".into()), last_launched_stage: Some(Stage::Code),
                 open_gate_stages: Vec::new(), feature_branch_exists: true, stopped: false }
}

#[test]
fn reconcile_phase_flags_orphan_open_gate() {
    let facts = PhaseFacts {
        gate_pending: false,
        open_gate_stages: vec![Stage::Validate],
        ..agreeing_facts(PhaseId::new(3))
    };
    let findings = reconcile_phase(&facts);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Problem);
    assert!(findings[0].detail.contains("gate open for stage validate"));
    assert_eq!(findings[0].repair.as_deref(), Some("devflow gate approve 3 --stage validate"));
}
```
Adding a field to `PhaseFacts` breaks every struct literal: `agreeing_facts` and the explicit literals at :6051, :6068,
:6085, :6102, :6120 (search hits). The repair-string assertions F-6 lists (:5812, :5893, :5912, :6062, :6079, :6096,
:6154, :6310) are updated on purpose (C-28).

---

### 15. `crates/devflow-cli/src/pipeline_launch.rs::advance` + `run_monitor` (controller) — SURV-01 D-02.2, R-2, F-8

**Analog:** self (`pipeline_launch.rs:1452-1496`). The event emission for a failure nobody sees (`:1463-1474`, 14-CR-06)
is the shape to reuse for wait expiry and stage mismatch:
```rust
events::emit(
    project_root,
    PhaseId::new(0),
    "advance_failed",
    serde_json::json!({ "reason": err.to_string() }),
);
return Err(err);
```
With a known phase, pass `phase`, not `PhaseId::new(0)`. The sentinel exists only because phase resolution failed at
that point.

**Code to change** (`:1482-1496`):
```rust
let _lock = match lock::acquire(project_root, phase) {
    Ok(guard) => guard,
    Err(lock::LockError::Contended { pid, path: _ }) => {
        return Err(CliError::Message(format!(
            "another devflow process (pid {pid}) is already running"
        )));
    }
    Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
};
// Load under the lock: ... a duplicate advance of THIS phase is excluded by
// the lock itself.
let mut state = workflow::load_state(project_root, phase)?;
```
- Swap to the File 3 blocking acquire with a production constant. Tests call an inner function that takes the
  `Duration` (RESEARCH Pattern 3).
- Rewrite the :1491-1495 comment, since a bounded wait means a duplicate is no longer simply excluded (R-2).
- After `load_state`, compare `state.stage` against `Option<Stage>`. `Some(mismatch)` → `advance_failed` plus refuse.
  `None` → proceed and emit an event naming the legacy invocation (RESEARCH Pitfall 3).

**`run_monitor` signature** (`:904-912`; the call at `:977` is `advance(project_root, Some(phase))`). Thread a `stage`
parameter through it.

**Test analog for `advance` with a stubbed agent** (`:3596-3651`). It shows `advance(root, Some(phase))` called
in-process with a fixture plan and capture. Its PATH block is the TEST-01 before-shape (File 20):
```rust
let _guard = env_lock();
let dir = tempfile::tempdir().unwrap();
let root = dir.path();
init_repo(root);
let phase = PhaseId::new(88);
write_declared_checkpoint_plan(root, phase);
write_confirmed_checkpoint_capture(root, phase);
let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
state.stage = Stage::Code;
state.session_id = Some("sess-checkpoint-1".to_string());
workflow::save_state(&state).unwrap();
...
let result = advance(root, Some(phase));
...
let reloaded_for_reap = workflow::load_state(root, phase).ok();
let _reap_guard = reloaded_for_reap.as_ref().map(ReapMonitorOnDrop::after_launch);
result.unwrap();
let auto_decided = events_of_kind(root, "checkpoint_auto_decided");
```

---

### 16. `crates/devflow-cli/src/pipeline_launch.rs` `Action::GateReview` re-scan (service) — CHKPT-01, CHKPT-02 D-07, R-8

**Analog:** self (`pipeline_launch.rs:1592-1624`):
```rust
let mut reason = result.reason.clone();
let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
let checkpoint_confirmed = state.agent == AgentKind::Claude
    && verify::phase_has_blocking_human_checkpoint(execution_root, phase)
    && agent_result::checkpoint_reported_in_capture(project_root, phase);
if checkpoint_confirmed {
    let ceiling_ok = state.checkpoint_resumes < mode::MAX_CHECKPOINT_RESUMES;
    match (&state.session_id, ceiling_ok) {
        (Some(session_id), true) => {
            let session_id = session_id.clone();
            return relaunch_checkpoint_session(&mut state, &session_id);
        }
        (Some(_), false) => {
            reason = Some(augment_unresolved_checkpoint_reason(
                reason,
                &format!("resume ceiling ({}) exhausted", mode::MAX_CHECKPOINT_RESUMES),
            ));
        }
        (None, _) => {
            reason = Some(augment_unresolved_checkpoint_reason(reason, "no session id on record"));
        }
    }
}
```
- Keep the five-step order comment (:1568-1591; T-28-01, C-8) and the two-root split (999.76, C-9).
- Insert the recorded-set compare inside `if checkpoint_confirmed`, before `relaunch_checkpoint_session` (:1602).

**Gate-arm analog for the re-scan gate:** `preflight.rs::run_preflight` (`:1362-1385`). It builds a `[never-silent]`
context, calls `run_gate`, and handles each `GateAction` arm explicitly:
```rust
let context = format!(
    "[never-silent] preflight failed for stage {stage}: {} — human review needed \
     (retry, loop-to-code, or abort)",
    truncate_reason(&reason)
);
match run_gate(project_root, state, stage, &context)? {
    GateAction::Advance => {
        let _ = Gates::cleanup(project_root, state.phase, stage);
        state.gate_pending = false;
        state.preflight_retries = 0;
        workflow::save_state(state)?;
        launch_stage_inner(state, None, None)?;
    }
    GateAction::LoopBack(_) => {
        let _ = Gates::cleanup(project_root, state.phase, stage);
        launch_stage(state, None, None)?;
    }
    GateAction::Abort(reason) => abort(project_root, state, &reason)?,
}
```
For the re-scan gate, `Advance` means "record the current set (R-8), save, then continue into
`relaunch_checkpoint_session`". It must not mean advancing to Validate (RESEARCH Pattern 5).

**Test analog:** `advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records` (:3596-3651; see File 15).
The unchanged-plan control must keep emitting exactly one `checkpoint_auto_decided`. A new-checkpoint case must instead
emit `gate_fired` for `code`, the inverse of the assertion at :3645-3650.

---

### 17. `crates/devflow-cli/src/preflight.rs::run_preflight` (service) — CHKPT-02 D-07, F-7, R-8

**Analog:** self. The recording site is the refusal gate's `Advance` arm (`preflight.rs:1368-1377`, shown in File 16),
which already mutates and saves state. Record the set before its `workflow::save_state(state)?` at :1375.

The other recording site is the pass path after `generic_preflight_checks` (:1334-1335) at `Stage::Code`, when the field
is `None`. It follows the existing guarded-save shape (:1389-1394):
```rust
// Preflight passed: reset the retry counter, persisted ... Guarded so a
// passing preflight on an already-zero counter does not rewrite state
// on every single launch.
if state.preflight_retries != 0 {
```

**Do not record inside** `unattended_planned_checkpoint_condition` (`preflight.rs:1067-1084`) or anywhere under
`generic_preflight_checks`. Its signature is fixed by D-09 (C-11, doc :1130-1133), and `resume` runs it on a cloned
candidate that may be discarded (`pipeline_launch.rs:1307-1328`):
```rust
fn unattended_planned_checkpoint_condition(launch_root: &Path, state: &State) -> ConditionState {
    if verify::phase_plan_files(launch_root, state.phase).is_empty() { ... }
    if verify::phase_has_human_only_checkpoint(launch_root, state.phase) {
        return ConditionState::DoesNotHold(...);
    }
    ConditionState::Holds
}
```

---

### 18. `crates/devflow-cli/src/main.rs` (config, clap) — SURV-01 R-2, F-8

**Analog:** self. `Advance` (`main.rs:117-127`) already carries an optional `--phase` whose doc explains the monitor
recording it at spawn time:
```rust
/// Internal: advance the stage machine after a monitored agent exits.
#[command(hide = true)]
Advance {
    /// Project root.
    #[arg(default_value = ".")]
    project: PathBuf,
    /// Phase whose stage machine to advance. Recorded by the monitor at
    /// spawn time so advance never depends on a shared state singleton.
    #[arg(long)]
    phase: Option<PhaseId>,
},
```
- Add `#[arg(long)] stage: Option<Stage>` with the same kind of doc. It is hidden, so `help_snapshot.rs` is unaffected
  (F-8).
- `Monitor` (:134-160) uses non-optional `#[arg(long)] phase: PhaseId` and `agent: AgentKind`. Add `stage` before the
  trailing `argv` (:157-159). Making it `Option<Stage>` there too keeps an old `__monitor` invocation parsing.
- Thread the value through the dispatch arms (:577 `Command::Advance { project, phase } => advance(...)`, :578-594
  `Command::Monitor { ... } => run_monitor(...)`).

---

### 19. `crates/devflow-cli/src/test_support.rs` (test utility) — TEST-01

**Analog:** self. The helper pair to generalize or re-export is `run_test_without_git` (:411-426) and
`assert_child_ran_exactly_one_passing_test` (:449-489), shown in File 8. Keep `CHILD_NO_GIT_ROOT` / `child_no_git_root`
(:373-383) or generalize the marker.

**PATH-directory builders to reuse for the child's `Command`.** Each returns a `TempDir` the caller holds across
`.output()`:
- `agent_free_git_only_path_dir()` (:287-299): `git` symlink only.
- `agent_free_dir_with_agent_stub(program)` (:503-521): adds `git`, `sh`, and a stub `program`.
- `stub_agent_binary(name)` (:523-532) plus `prepend_path(stub_dir, original)` (:537-546). This is prepend-to-real-`PATH`
  and still resolves real agents. Do not use it for a child whose purpose is "no real agent" (D-10).

**`NeutralPath` (:328-360)** installs process-global `PATH`. Delete it only once it is unused. The poison-recovery
premise in `env_lock` depends on RAII restores, and so does the doc at :53-99 (contract C-25):
> **Without those guards this accessor WOULD be unsound.** ... If you are removing an unwind-safe guard, you are changing
> this function's premise; re-read [`NeutralPath`] before you do.

The `ENV_MUTEX` doc (:22-51) lists `PATH` among five guarded variables. Update that list when `PATH` leaves it.

---

### 20. cli test modules with PATH blocks + 999.80 fixtures (test) — TEST-01 D-08, D-10, F-2

**Before shape A**, the trailing-statement PATH block (`pipeline_launch.rs:3612-3628`):
```rust
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
```

**Before shape B**, a 999.80 fixture with no PATH neutralization in its body (`pipeline_outcomes.rs:5298-5317`). The only
thing keeping it from spawning an agent is the note content `"abort: test cleanup"`:
```rust
#[test]
fn stage_failure_retry_cleans_stale_response() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(43);
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    workflow::save_state(&state).unwrap();

    // Pre-write an Abort response so the first failure resolves
    // immediately without spawning a monitor.
    let response_path = Gates::response_path(root, phase, Stage::Code);
    std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
    std::fs::write(
        &response_path,
        r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
    )
    .unwrap();

    handle_stage_failure(root, &mut state, Stage::Code, Some("first failure".into())).unwrap();
    ...
}
```
All 26 sites (F-2): `pipeline_outcomes.rs` ×18, `preflight.rs` :1850/:2022/:2061, `pipeline_launch.rs` :2584/:2745/:3587,
`pipeline_gate.rs` :1085/:1405. Reachability to a spawn is unmeasured; the planner confirms it first (D-10).

**After shape:** the parent/child split in File 9. The child's `Command` gets `PATH` set to an
`agent_free_git_only_path_dir()` (plus `sh`/stub where a launch must complete). Every child is guarded by
`assert_child_ran_exactly_one_passing_test`. For an in-process test that must still run a stubbed agent (shape A), use
`agent_free_dir_with_agent_stub("claude")` rather than `prepend_path`.

**Structural gate** (RESEARCH Validation):
`rg -n 'set_var\("PATH"|remove_var\("PATH"' crates --glob '*.rs' | rg -v '^\S+:\d+:\s*//'` must return nothing. Pair it
with a positive control: the same rg on the pre-change tree reports 156 PATH lines (F-9), proving the pattern matches.

---

### 21. `crates/devflow-cli/tests/gate_wedge_e2e.rs` (NEW, e2e) — SURV-02 criterion 4

**Analog:** `crates/devflow-cli/tests/gate_sweep_e2e.rs`. Integration test files are separate crates, so the helpers are
copied locally (TESTING.md:86). Copy these:
- imports (:12-19):
  ```rust
  use devflow_core::gates::{GateAction, GateFile, Gates};
  use devflow_core::mode::Mode;
  use devflow_core::phase_id::PhaseId;
  use devflow_core::stage::Stage;
  use devflow_core::state::{AgentKind, State};
  use std::path::Path;
  use std::process::Command;
  use std::time::{Duration, Instant};
  ```
- `devflow_bin()` (:21-23) → `env!("CARGO_BIN_EXE_devflow")`
- `git()` (:27-37) through `devflow_core::test_support::git_command(root)` (never a bare `Command::new("git")`, 999.37)
- `init_repo(root, phase)` (:43-59)
- `wait_for(predicate, timeout_secs, what)` (:63-72)
- `e2e_child_timeout()` (:79-85), 90 s default via `DEVFLOW_E2E_CHILD_TIMEOUT_SECS`
- `wait_for_child_exit(child, root, phase, deadline)` (:97-124). It reaps before panicking and reports on-disk facts;
  change the hard-coded `Stage::Code` paths at :111-112 to the Define gate.

**Live-waiter wait and per-`Command` env** (`gate_sweep_e2e.rs:292-319`):
```rust
let mut child = Command::new(devflow_bin())
    .args(["advance", "--phase", &phase.to_string()])
    .arg(root)
    .env("DEVFLOW_GATE_TIMEOUT_SECS", "15")
    .spawn()
    .expect("spawn devflow advance");

wait_for(|| devflow_core::lock::holder(root, phase).is_some(), 10, "the child to acquire .devflow/lock-95");
let gate_path = Gates::gate_path(root, phase, Stage::Code);
wait_for(|| gate_path.exists(), 10, "the child to write the Code gate");
```
For #200, spawn `start --phase N --mode auto` against a repo with no `.planning/config.json`, so it parks at
`Gates::gate_path(root, phase, Stage::Define)` (RESEARCH Pattern 7). The child `Command` needs a `PATH` holding a stub
`claude` plus `git`/`sh`. Copy `fake_bin_dir()` from `start_reachability_e2e.rs:38-60`, but set `PATH` to that directory
plus `git`/`sh` symlinks, not a prepend to the real `PATH`.

**Post-exit assertions** (`gate_sweep_e2e.rs:336-354`):
```rust
assert!(status.success(), "devflow advance must exit cleanly once its own abort() path runs, got {status:?}");
assert!(devflow_core::lock::holder(root, phase).is_none(), "LockGuard's Drop must have released ...");
let events = std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap();
assert!(events.lines().any(|line| line.contains("\"workflow_aborted\"")), "...\nevents:\n{events}");
```

**Differences from the analog:**
- The wedge arm must signal the child (`child.kill()`). `gate_sweep_e2e.rs` and `stop_e2e.rs` forbid signalling by their
  own acceptance grep (:356-361, :212-217), so do not copy that header comment.
- After D-02.1, a killed `start` leaves `lock-NN` naming a dead pid. Assert that `recover --clean --phase N` removes it
  (`lock::remove_stale_locks`, `lock.rs:258-285`).
- Assert the on-disk outcome, not elapsed time. Print the self-resolving arm's pickup interval as an observation of
  poll-backoff position (F-10), never as a timing claim.

---

### 22. `crates/devflow-cli/tests/stop_e2e.rs` (extend, e2e) — SURV-01, SURV-02

**Analog:** self. The live-holder fixture is `stop_ends_a_gated_phase_through_its_own_abort_path_with_no_signal_sent`
(:136-218). The no-holder control is `stop_marks_state_stopped_and_records_reason` (:225-256). Its doc comment already
names the hazard SURV-01 fixes:
```rust
/// No live process contends for the state file here, so this is
/// deterministic (unlike the gated-path fixture above, where a live poller
/// may race a second writer).
#[test]
fn stop_marks_state_stopped_and_records_reason() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(96);
    Gates::write_gate(root, phase, Stage::Ship, "approve merge?").unwrap();
    let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    devflow_core::workflow::save_state(&state).unwrap();
    let output = Command::new(devflow_bin())
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    ...
    let reloaded = devflow_core::workflow::load_state(root, phase).unwrap();
    assert!(reloaded.stopped, "stop must set stopped=true");
}
```
New subject (RESEARCH Pattern 6): with a real `devflow advance` child parked on a gate (the :151-177 shape), snapshot the
state-file bytes, run `stop`, and assert the bytes are unchanged apart from what the holder's own abort does, and that
the output names the live holder. The holder must be a different process (RESEARCH Pitfall 1): never hand-write a lock
file for an arbitrary pid, which would exercise reclaim instead.

**Contracts that must stay green:** C-6 `stop_is_idempotent_against_an_already_answered_gate` (:347), C-27 root-parsing
tests (:549-810).

---

### 23. `start` refusal e2e (in `start_reachability_e2e.rs` or a new file) — SURV-01 F-5

**Analog:** `crates/devflow-cli/tests/start_reachability_e2e.rs:1-60`. It asserts on the real binary's exit status,
stderr and filesystem effects, "never on source text" (doc :9-11), and defines a `FakeBin` stub:
```rust
struct FakeBin {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

fn fake_bin_dir() -> FakeBin {
    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join("claude");
    fs::write(&claude, "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n").unwrap();
    let mut perms = fs::metadata(&claude).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&claude, perms).unwrap();
    let path = dir.path().to_path_buf();
    FakeBin { _dir: dir, path }
}
```
Subject: a live holder (a `devflow advance` child parked on a gate, as in File 21) → `start --phase N` refuses and creates
no worktree or feature branch. Control: no holder → proceeds.

---

### 24. `clippy.toml` (NEW, config) — TEST-01 D-09

**Partial analog:** `Cargo.toml:43-46`, the workspace's only clippy configuration. Both crates inherit it
(`crates/devflow-cli/Cargo.toml:12-13`, `crates/devflow-core/Cargo.toml:12-13`: `[lints] workspace = true`):
```toml
[workspace.lints.clippy]
dbg_macro = "warn"
todo = "warn"
unimplemented = "warn"
```
`disallowed_methods` is warn-by-default, and `scripts/check.sh:36-40` runs
`cargo clippy --workspace --all-targets -- -D warnings`. No `[workspace.lints.clippy]` entry is needed; only the
`clippy.toml` path list is. The file content was verified by the RESEARCH probe (Pattern 8):
```toml
disallowed-methods = [
    { path = "std::env::set_var", reason = "process-global env mutation races concurrent spawns; set env on the child Command instead" },
    { path = "std::env::remove_var", reason = "process-global env mutation races concurrent spawns; use Command::env_remove instead" },
]
```
Negative control: a deliberately added `std::env::set_var` must fail clippy. Assert on the `disallowed method` text, not
the exit code (memory: control must fail for the right reason). F-13: `clippy.toml` does not match the post-commit
dev-setup regex (`scripts/hooks/post-commit:80-82`), so updating the checklist is a judgement call.

---

### 25. `.planning/codebase/TESTING.md` (docs) — TEST-01 D-09

**Section to rewrite** (`TESTING.md:88-96`):
> Any CLI unit test that changes `PATH`, `DEVFLOW_GATE_TIMEOUT_SECS`, `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`, or
> `DEVFLOW_GATE_NOTIFY_CMD` must hold `crate::test_support::ENV_MUTEX` for the complete save, mutate, exercise, and
> restore sequence. Do not declare a second mutex.
>
> **D-04 invariant: every env var is guarded by exactly one mutex, and no var is touched under two.** This is a
> reviewer-enforced convention; no type or lint checks it mechanically.

Remove `PATH` from the list and add the child-process rule. The "no lint checks it" sentence becomes false once
`clippy.toml` lands, so replace it. Keep the False-Green Traps section (:123-128) as is; conversion gates rely on traps
1-3. Awareness: `TESTING.md:107` cites `.claude/skills/ai-change-acceptance/`, which does not exist (F-12). That fix is
not in scope.

---

### 26-27. `.planning/ROADMAP.md`, `.planning/REQUIREMENTS.md` (docs) — D-11, D-01, D-05, D-10

**ROADMAP anchors (read this session):**
- `:30` progress row: `| 48 | ... | SURV-01, SURV-02, GATE-01 (999.125), GATE-02 (999.126), 999.38 | 2 | Not started |`
- `:155` `**Requirements**: SURV-01, SURV-02, GATE-01 (999.125), GATE-02 (999.126), 999.38`
- `:175` `5. **GATE-01 (999.125):** ...`, `:178` `6. **GATE-02 (999.126):** ...`, `:180` `7. **999.38:** ...`
- criteria 1-3 in the Phase 48 block, which starts at `:146`
- `§ 999.80` at `:1964` (sequencing note :1996-2000) and `§ 999.38` at `:3073`, for the "worked together" note (D-10)

Keep active-milestone phase headings inside the milestone window and keep the `## Progress` table intact (global GSD
rule). Use the Phase 47 ROADMAP amendment commit as the precedent for a plan-task doc edit (D-11).

**REQUIREMENTS anchors:**
- `### Run Survivability (SURV)` `:63`, with entries shaped `- [ ] **SURV-01**: ...` (`:65`, `:73`). Add CHKPT and TEST
  entries in sibling `###` sections of the same shape; `### Test Infrastructure (INFRA)` at `:120` is the nearest
  existing test-category heading.
- Traceability rows `:174-175` are shaped `| SURV-01 | Phase 48 — ... | 2 | Pending |`.
- Future table `:143-147` keeps `GATE-01 (#184)` / `GATE-02 (#170)` unchanged.
- The per-requirement acceptance notes at `:194`ff take the shape `- **SURV-01** — Phase 48's acceptance is ...`.

---

## Shared Patterns

### A. Per-phase lock acquisition at command entry
**Source:** `crates/devflow-cli/src/pipeline_launch.rs:1293-1301` (`resume`), `crates/devflow-cli/src/pipeline_gate.rs:518-527` (`ship_override`)
**Apply to:** `start` (File 11), `stop`'s state write (File 12), `advance` (File 15, blocking variant), `recover --clean` gate cleanup check (File 6)
```rust
let _lock = match lock::acquire(project_root, phase) {
    Ok(guard) => guard,
    Err(lock::LockError::Contended { pid, .. }) => {
        return Err(CliError::Message(format!(
            "phase {phase}: another devflow process (pid {pid}) holds the per-phase lock — ..."
        )));
    }
    Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
};
let mut state = workflow::load_state(project_root, phase)?;   // load AFTER the lock (R-5)
```
Bind to a named `_lock`, never `let _ =`, which drops the guard immediately.

### B. Live-waiter identity (fail-closed, three-way)
**Source:** `crates/devflow-cli/src/commands.rs:1853-1914` (`stop_via_lock`)
**Apply to:** `stop_via_gate`, `gate_respond`, `gate_sweep`, doctor facts, `recover::clean_phase` (Files 6, 12, 13, 14)
Chain: `lock::holder` → parse pid → `agent::agent_running` → `lock::holder_identity` → `agent::is_same_process`. The
`Some((_, None))` legacy-lock arm and an unreadable holder are "unconfirmable" and claim neither answer (D-05). Never use
`state.monitor_pid` (T-23-51, `commands.rs:1780-1786`).

### C. Loud refusal in unattended paths
**Source:** `crates/devflow-cli/src/pipeline_launch.rs:1468-1473`
**Apply to:** `advance` wait expiry, stage mismatch, and legacy no-stage invocation (File 15)
```rust
events::emit(project_root, phase, "advance_failed", serde_json::json!({ "reason": ... }));
```
Monitor stdout and stderr go to `/dev/null` (`monitor.rs:520-522`), so an event is the only visible record.

### D. Atomic file publish
**Source:** `crates/devflow-core/src/registry.rs:222-229` (unique temp + rename), `crates/devflow-core/src/lock.rs:85-90` (`create_new` + `AlreadyExists`)
**Apply to:** `workflow::write_state_atomic`, `gates::write_atomic`, `Gates::respond` (Files 1, 2)
Temp names must not end in `.json` (F-11) and must share one sweepable pattern (R-7).

### E. Persisted `State` field
**Source:** `crates/devflow-core/src/state.rs:342-373`
**Apply to:** the CHKPT-02 field (File 5)
`#[serde(default)]`, plus a doc that states what `None` means and why the value is persisted (the next reader is a
separate process). Also add it to `State::new`.

### F. No process-global env in tests; per-`Command` env only
**Source:** `crates/devflow-cli/src/test_support.rs:411-489`, `crates/devflow-cli/tests/gate_sweep_e2e.rs:292-297`
**Apply to:** every new or converted test (Files 8, 9, 19-23)
```rust
Command::new(devflow_bin()).args([...]).env("DEVFLOW_GATE_TIMEOUT_SECS", "15").spawn()
```
Set `PATH` to a directory, never remove it (RESEARCH probe subject C: a removed `PATH` falls back to `/bin:/usr/bin`).
Guard every child with the four-assertion checker and module-qualified `--exact` names.

### G. Doctor findings are report-only, with facts gathered separately from reconciliation
**Source:** `crates/devflow-cli/src/commands.rs:3162-3164`, `:3202-3204`, `:3368-3410`
**Apply to:** the new doctor finding (File 14)
I/O happens only in `build_phase_facts`; `check_*` functions are pure `&PhaseFacts -> Option<PhaseFinding>`.

### H. Every test carries a case that must produce the opposite result
**Source:** `crates/devflow-cli/src/test_support.rs:777-802` (parent-side controls before and after the child), `crates/devflow-core/src/verify.rs:558-566` (discriminating fixtures)
**Apply to:** all plans. The pairings named in CONTEXT/RESEARCH: live holder ↔ no holder, prose marker ↔ task-line
marker, two responders ↔ one responder, unchanged plan ↔ added checkpoint, deliberate `set_var` ↔ clean tree.

---

## No Analog Found

Mechanisms with no in-repo precedent. The planner uses the named RESEARCH pattern.

| Mechanism | Lives in | Why no analog | Use instead |
|-----------|----------|---------------|-------------|
| Markdown fence tracking (char + run length, R-3 unclosed-fence rescan) | `verify.rs` | `rg -i fence` over `crates/**/*.rs` hits only the known-limit test at `verify.rs:609,623` | RESEARCH Pattern 4 (rules 1-4; A1 CommonMark assumption) |
| Hard-link exclusive publish | `gates.rs::respond` | `rg hard_link crates` returns 0 hits | RESEARCH Pattern 2; `lock.rs:85-90` for the `AlreadyExists` match shape only |
| Shared no-waiter message used by both CLI verbs and doctor | `commands.rs` | no function today is called from both a verb and a `check_*` | `PhaseFinding.repair` shape (`commands.rs:3195-3200`) as the return type |
| `#[expect(lint, reason)]` | test modules | 0 occurrences in `crates/` | RESEARCH Pattern 8 probe results |
| Bounded lock retry after a successful signal (F-1) | `commands.rs` stop | no code waits on a lock after `agent::terminate` | compose `lock::acquire_blocking` (File 3) with `agent::TERMINATE_VERIFY_WAIT` (`agent.rs:86`) |

---

## Metadata

**Analog search scope:** `crates/devflow-core/src/` (workflow, gates, lock, verify, state, recover, monitor, agent,
test_support, registry, canary, config, agents/pi, lib), `crates/devflow-cli/src/` (commands, pipeline_launch, preflight,
pipeline_gate, pipeline_outcomes, main, test_support), `crates/devflow-cli/tests/` (gate_sweep_e2e, stop_e2e,
start_reachability_e2e), `Cargo.toml`, both crate `Cargo.toml`s, `scripts/check.sh`, `.planning/codebase/TESTING.md`,
`.planning/ROADMAP.md`, `.planning/REQUIREMENTS.md`.
**Files scanned:** 30 source/config/doc files, read in targeted ranges (no file over 2,000 lines loaded whole).
**Pattern extraction date:** 2026-09-14.
**Not established by this map:** reachability of the 26 `abort: test cleanup` fixtures to an agent spawn; the exact
`PhaseFacts` literal count in tests (search hits, not opened); whether `opencode.rs`'s `PathGuard` body is byte-identical
to `pi.rs`'s (line locations confirmed at :423-440, body not read).
