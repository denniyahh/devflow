# Phase 50: Addressable Monitor Liveness - Pattern Map

**Mapped:** 2026-10-14 (HEAD `491be78` on `workspace/denniyahh`; crates unchanged since `b3c0c11`)
**Files analyzed:** 27 (8 new, 19 modified)
**Analogs found:** 26 / 27 (the socket wire protocol has no in-tree analog; the spike is the only reference)

Line numbers were read this session. They drift; re-check before quoting them in a PLAN.md gate. Every analog path
below was checked with `git ls-files` (all tracked; no gitignored mirrors involved).

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/devflow-core/src/supervisor.rs` (NEW: `SocketPath`, `probe`, `Liveness`, `PingReply`, wire protocol, `Sidecar`, `sweep_in`, `boot_id`, `request_shutdown`) | service (OS mechanism) | request-response + file-I/O | `crates/devflow-core/src/registry.rs` (cache-dir + `*_in` + read-only enumeration) and `lock.rs::publish_lock` (hard-link publish); `.planning/spikes/socket-supervisor/main.rs` `status`/`stop`/`sweep` | role-match (no socket code in tree) |
| `crates/devflow-core/src/supervisor/backstop.rs` (NEW) | service (guarded teardown) | batch (`/proc` scan) + event (signal) | `agent.rs::discover_stray_devflow_processes` (`/proc` scan, uid filter) + `monitor.rs::terminate_child_group` (group signal guards) + `lock.rs::classify_holder_record` (pure classifier) | exact for scan, exact for classifier |
| `crates/devflow-core/src/supervisor/runtime.rs` (NEW: self-pipe, control thread, terminal owner, `run_supervisor`) | service (supervisor process body) | event-driven | `monitor.rs::run_pipe_owning_monitor` (threads, `process_group(0)`, `128+n` exit code) | role-match |
| `crates/devflow-core/src/lib.rs` | config (module list) | n/a | itself, `lib.rs:54-86` | exact |
| `crates/devflow-core/src/monitor.rs` | service | event-driven | itself (`PipeOwning` arm, `monitor.rs:378-470`) | exact |
| `crates/devflow-core/src/lock.rs` (999.128) | utility | CRUD (lock file) | itself, `holder_status` at `lock.rs:391-401` | exact |
| `crates/devflow-core/src/recover.rs` (flip one test) | test | CRUD | itself, `recover.rs:1117` | exact |
| `crates/devflow-core/src/state.rs` (drop `monitor_pid`, add `supervisor`) | model | CRUD | `state.rs:284-289` (`monitor_pid` field shape) | exact |
| `crates/devflow-core/src/registry.rs` (`socket_dir_in`, widen `path_digest`) | utility | file-I/O | `registry.rs:66-98` (`roots_dir_in`, `entry_path_in`, `path_digest`) | exact |
| `crates/devflow-core/src/gates.rs` (`poll_response_cancellable`) | utility | polling | `gates.rs:284-310` (`poll_response`) | exact |
| `crates/devflow-core/tests/monitor_e2e.rs` | test | event-driven | itself (calls `spawn_monitor(.., MonitorLaunch::Legacy)`) | exact |
| `crates/devflow-cli/src/main.rs` (`__monitor` args, `Stop` doc) | route (clap dispatch) | request-response | `main.rs:137-166`, `main.rs:588-606` | exact |
| `crates/devflow-cli/src/pipeline_launch.rs` (`run_monitor`, `spawn_agent_and_record`, safe points, publish under lock) | controller | event-driven + CRUD | itself, `pipeline_launch.rs:918-1110`, `:1619-1660` | exact |
| `crates/devflow-cli/src/commands.rs` (`Liveness`, `status`, `cleanup`, `doctor`, `stop`, launch guard) | controller | request-response | itself, `commands.rs:878-1027`, `:1882-2160`, `:2470-2492`, `:3694-3964` | exact |
| `crates/devflow-cli/src/pipeline_gate.rs` (clear `supervisor`) | controller | CRUD | `pipeline_gate.rs:68-75` | exact |
| `crates/devflow-cli/src/test_support.rs` (`ReapMonitorOnDrop`) | test utility | event-driven | `test_support.rs:567-636` | exact |
| `crates/devflow-cli/src/preflight.rs` (tests asserting `monitor_pid`) | test | CRUD | `preflight.rs:2205-2439` | exact |
| `crates/devflow-cli/tests/common/mod.rs` (NEW: `hermetic_devflow`, reaper) | test utility | config | `gate_notify_persist_e2e.rs:111-136` (`isolated_cache`, `ReapOnDrop`); `start_lock_e2e.rs:74-94` (`child`) | role-match |
| `crates/devflow-cli/tests/test_home_isolation.rs` (NEW, V-26) | test | file-I/O | `crates/devflow-cli/tests/git_env_hermeticity.rs` (environment-leak guard) + `start_lock_e2e.rs::run_start` | role-match |
| `crates/devflow-cli/tests/supervisor_e2e.rs` (NEW, V-12 + fault fixtures) | test | event-driven | `crates/devflow-cli/tests/start_lock_e2e.rs` (CLI-boundary refusal, `LiveProcess`, `LaunchedRunReaper`) | role-match |
| `crates/devflow-cli/tests/stop_e2e.rs` (rewrite `:177`, add V-17) | test | request-response | itself, `stop_e2e.rs:107-231` | exact |
| `crates/devflow-cli/tests/start_lock_e2e.rs` (V-23, plant helpers) | test | request-response | itself, `start_lock_e2e.rs:308-336`, `:395-516`, `:587-613` | exact |
| `crates/devflow-cli/tests/recover_clean_e2e.rs` (999.128 repair) | test | CRUD | itself, `recover_clean_e2e.rs:65-120` | exact |
| `crates/devflow-cli/tests/phase7_cli.rs` + the other 15 e2e files (71 `Command::new(devflow_bin())` sites) | test | config | `common/mod.rs` once it exists | exact (mechanical) |
| `crates/devflow-cli/tests/snapshots/devflow-help.txt` | test fixture | n/a | `help_snapshot.rs:1-7` regen recipe | exact |
| `.planning/phases/50-addressable-monitor-liveness/tools/fault-arm.sh` (NEW) + P50 copies of rig tools | script | batch | `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/negctl.sh` | role-match |
| `OPERATIONS.md`, `CHANGELOG.md`, `.planning/ROADMAP.md` | docs | n/a | existing text | n/a |

## Pattern Assignments

### `crates/devflow-core/src/supervisor.rs` (service, request-response + file-I/O) — NEW

**Analogs:** `crates/devflow-core/src/registry.rs` (the hermetic `*_in` pattern), `crates/devflow-core/src/lock.rs`
(atomic publish), `.planning/spikes/socket-supervisor/main.rs` (probe/stop/sweep reference — copy shape, not code).

**Module header and imports** — copy `registry.rs:1-20`: a `//!` block that states the storage shape and why, then
`use crate::phase_id::PhaseId; use serde::{Deserialize, Serialize}; use std::path::{Path, PathBuf};`. Errors use
`thiserror` exactly as `registry.rs:35-44`:
```rust
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("registry I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("registry JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}
```
New `pub` error enums (`ProbeError`, `SocketPathError`) are new API on a published crate: mark them
`#[non_exhaustive]` from the start (R-9; `MonitorError` at `monitor.rs:28` is the counter-example that now costs a
break).

**Cache-dir resolution — never read env in new code.** The only env-reading resolver is `registry.rs:53-62`:
```rust
pub fn cache_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("DEVFLOW_CACHE_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
        return Some(PathBuf::from(dir).join("devflow"));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".cache").join("devflow"))
}
```
Every supervisor function takes `cache_dir: &Path` (`*_in`), with a thin env-resolving wrapper only where a CLI
needs one, exactly as `register` wraps `register_in` (`registry.rs:265-270`) and `load_roots` wraps `load_roots_in`
(`registry.rs:274-279`).

**Per-entry filename** — copy `registry.rs:75-81` and the stable FNV digest at `registry.rs:88-98` (private today;
RESEARCH Finding 2 says widen to `pub(crate)`):
```rust
pub fn entry_path_in(cache_dir: &Path, project_root: &Path, phase: PhaseId) -> PathBuf {
    let digest = path_digest(project_root);
    roots_dir_in(cache_dir).join(format!(
        "{digest:016x}-{padded}.json",
        padded = phase.padded()
    ))
}
```
The socket name extends this with `-<launch_id>.sock` under `sock/` (R-1 per-launch naming). The doc comment at
`registry.rs:70-74` ("the digest is only a filename disambiguator — the authoritative `project_root` lives inside the
file itself") is the same contract the sidecar must state: the sweep reads root and phase from the sidecar JSON, never
from the filename.

**Private directory** — copy `registry.rs:207-212` for `sock/` (mode `0700`):
```rust
fn ensure_private_dir(dir: &Path) -> Result<(), RegistryError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}
```

**Atomic socket publish (R-1)** — the tree already has the exact `hard_link` publish shape, for lock files,
`lock.rs:149-185`. Copy its structure (temp first, `hard_link` onto the public name, `AlreadyExists` is a normal
outcome, temp removed on every path):
```rust
fn publish_lock(path: &Path, before_publish: impl FnOnce()) -> io::Result<LockPublication> {
    let (tmp, mut file) =
        crate::workflow::create_unique_temp(path, std::process::id(), &LOCK_TEMP_SEQ)?;
    ...
    before_publish();
    let publish = fs::hard_link(&tmp, path);
    let cleanup = fs::remove_file(&tmp);
    match publish {
        Ok(()) => { /* warn if cleanup failed */ Ok(LockPublication::Published) }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let _ = cleanup;
            Ok(LockPublication::AlreadyExists)
        }
        Err(error) => { let _ = cleanup; Err(error) }
    }
}
```
For the socket, the "temp" is the bound listener path (`UnixListener::bind` binds and listens), so the `.part` name
comes from `workflow::unique_temp_path_with`'s shape, `workflow.rs:205-208`:
`target.with_file_name(format!(".{name}.{pid}.{sequence}.tmp"))`. That name ends in `.tmp`, not `.sock`, so a sweep
that filters on the `.sock` extension never probes a half-built socket. The `before_publish` closure seam is also what
the V-11 test needs to hold the bind-then-link window open (see the barrier test pattern under Shared Patterns).
Run the 107-byte check (`SocketPathError::TooLong`) on both the `.part` path and the final path before binding.

**Sidecar atomic write** — a sidecar is a regular file, so `rename` is fine. Copy `registry.rs:222-229`
(`write_atomic` with a unique per-call temp), or `workflow.rs:239-254` (`write_state_atomic`) if the write must remove
the temp on error. Write the sidecar before linking the `.sock` name (R-7).

**Read-only enumeration (sweep, D-06)** — copy `registry.rs:237-259`: `read_dir`, skip any entry whose name lacks the
expected extension, skip unreadable or unparsable entries, sort for deterministic output, return `Vec` not `Result`:
```rust
pub fn load_roots_in(cache_dir: &Path) -> Vec<RegisteredRoot> {
    let mut roots = Vec::new();
    let dir = roots_dir_in(cache_dir);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return roots;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.ends_with(".json") {
            continue;
        }
        ...
    }
    roots.sort_by(|a, b| (&a.project_root, a.phase).cmp(&(&b.project_root, b.phase)));
    roots
}
```
The doc comment at `registry.rs:132-139` is the D-06 rule in the codebase's own words: removal is
"deliberately NOT called from `load_roots_in` — that must stay side-effect-free so a read-only command cannot mutate
machine state; callers invoke this explicitly." `sweep_in` follows `load_roots_in`; teardown follows
`prune_missing_in` (`registry.rs:140-162`) and is called only from `stop`.

**Probe reference (shape only)** — the spike `status` at `.planning/spikes/socket-supervisor/main.rs:163-175`
classifies with `connect()` alone and has a fourth `UNKNOWN` outcome. RESEARCH "Total probe skeleton"
(`50-RESEARCH.md:906-925`) is the corrected version (adds `symlink_metadata`, `S_ISSOCK`, uid, ping-reply
requirement). Copy RESEARCH, not the spike.

**Tests** — copy `registry.rs:292-317` form: `let dir = tempfile::tempdir().unwrap();` and call the `*_in(dir.path())`
variant directly; the mode assertion pattern is `register_in_creates_cache_and_roots_dirs_with_mode_0700`
(`registry.rs:423`). A short tempdir base is needed for the 107/108 boundary test (V-06); build the path to an exact
byte length rather than relying on the tempdir's length.

---

### `crates/devflow-core/src/supervisor/backstop.rs` (service, batch scan + guarded signal) — NEW

**Analog 1, `/proc` enumeration with own-uid filter:** `agent.rs:419-472`:
```rust
pub fn discover_stray_devflow_processes() -> Vec<StrayProcess> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let my_uid = unsafe { libc::geteuid() };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<u32>().ok())
        else { continue };
        // Shared-machine safety: skip anything not owned by us before
        // reading anything else about it.
        let Ok(owner_metadata) = std::fs::metadata(entry.path()) else { continue };
        if std::os::unix::fs::MetadataExt::uid(&owner_metadata) != my_uid {
            continue;
        }
        ...
        let Some(start_time) = process_start_time(pid) else {
            continue; // exited between the directory listing and here
        };
```
The backstop needs `/proc/<pid>/stat` fields 3 (state, to drop zombies), 5 (pgrp) and 22 (start time). Parse them with
the same "split after the final `)`" rule `process_start_time` uses, `agent.rs:214-221`:
```rust
let rest = &stat[stat.rfind(')')? + 1..];
rest.split_whitespace().nth(19)?.parse::<u64>().ok()   // field 22; field 3 = nth(0), field 5 = nth(2)
```
Extract one private stat-fields parser beside `process_start_time` rather than a second ad-hoc split. Zombie detection
already exists as `status_is_zombie` (`agent.rs:69-75`); prefer field 3 of `stat` so one read serves all three fields.

**Analog 2, pure decision function plus I/O wrapper:** `lock.rs:366-401`. `classify_holder_record` takes already-read
facts and returns an enum; `holder_status` does the I/O and calls it:
```rust
fn classify_holder_record(
    record: HolderRecord,
    is_running: bool,
    observed_start_time: Option<u64>,
) -> HolderStatus {
    if !is_running {
        return HolderStatus::NoHolder;
    }
    match (record.recorded_start_time, observed_start_time) {
        (Some(recorded), Some(observed)) if recorded == observed => HolderStatus::Live { pid: record.pid },
        (Some(_), Some(_)) => HolderStatus::Recycled { pid: record.pid },
        _ => HolderStatus::Unconfirmable { pid: record.pid },
    }
}
```
The Finding 1 rule (boot mismatch, empty group, leader present and matching, leader pid recycled, leader gone with
members) maps onto this shape: a pure `fn classify(sidecar, boot_id, leader_stat, members) -> BackstopDecision`, then a
thin signalling wrapper. This is also what makes `cargo mutants -v --file .../backstop.rs` meaningful (RESEARCH
sampling rate). Doc the enum variants the way `HolderStatus` is documented at `lock.rs:39-56`.

**Analog 3, group-signal guards:** `monitor.rs:1494-1522`:
```rust
fn terminate_child_group(child_pid: u32) -> bool {
    let Ok(signed) = libc::pid_t::try_from(child_pid) else {
        warn!("idle timeout: child pid {child_pid} does not fit pid_t; not signalling");
        return false;
    };
    if signed <= 1 {
        warn!("idle timeout: refusing to signal group for pid {signed}");
        return false;
    }
    // SAFETY: `signed > 1`, so `-signed < -1` and the two catastrophic
    // targets (`0` = our own group, `-1` = everything) are both excluded.
    unsafe { libc::kill(-signed, libc::SIGTERM); }
    let dead = crate::agent::terminate_and_verify(
        child_pid, crate::agent::TERMINATE_VERIFY_WAIT, crate::agent::TERMINATE_VERIFY_POLL);
    unsafe { libc::kill(-signed, libc::SIGKILL); }
    dead
}
```
The backstop adds two refusals this function does not have: never signal our own `getpgrp()`, and only signal after
`pidfd_open(P)` plus a start-time re-read (RESEARCH Finding 1 step 4). Keep the `pid_t::try_from` plus `> 1` guard and
the `// SAFETY:` comment style. Grace and poll values come from `agent::TERMINATE_VERIFY_WAIT` / `_POLL`
(`agent.rs:97,102`); their doc says "Callers that need a different ceiling ... should pass their own `wait`/`poll`
rather than inventing new constants."

**Tests:** V-18…V-21. Spawn the group from the test, as `agent.rs:744-770` does
(`Command::new("sh").arg("-c").arg("trap '' TERM; sleep 30").spawn()`, short sleep for the trap, `child.wait()` at the
end), but with `.process_group(0)` so the test owns a real group. The recycled case is fabricated the way
`lock.rs:851-890` does it: record this process's own pid with `start.saturating_add(1)`.

---

### `crates/devflow-core/src/supervisor/runtime.rs` (service, event-driven) — NEW

**Analog:** `monitor.rs:961-1208` (`run_pipe_owning_monitor`), the only in-process Rust supervisor in the tree.

**Agent spawn (keep, and make it the single spawn for both arms, R-3)** — `monitor.rs:991-1015`:
```rust
let mut agent_cmd = hermetic_command(program, workdir);
// The coding agent never needs the gate notify command (WR-07).
strip_notify_env(&mut agent_cmd);
let mut child = agent_cmd
    .args(args)
    .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::from(stderr_handle))
    .process_group(0)
    .spawn()?;
let child_pid = child.id();
std::fs::write(&pid_file, format!("{child_pid}\n"))?;
```
The Legacy arm differs only in stdio: stdin `Stdio::null()`, stdout to the capture file (the deleted script did
`> {stdout_file} 2>{stderr_file}` with stdin from the parent's `/dev/null`, `monitor.rs:546,584-588`). Insert the
`getpgid(child_pid) == child_pid` assertion between `spawn()` and the pid-file write. Adapter env ordering rule from
`monitor.rs:386-404` and `:566-570`: `hermetic_command` removes at construction, `.envs(...)` runs after, so deliberate
adapter env survives. Under the new design the env must be re-applied on the agent `Command` (pass key names on argv,
not values — RESEARCH Anti-Patterns).

**Threading model doc** — copy the shape of the doc comment at `monitor.rs:941-959` (names each thread, what it owns,
and the one deadlock it avoids). The runtime adds a control thread (`poll` on self-pipe + listener) and the
`Mutex<OwnerPhase>` terminal owner; document them the same way.

**Exit code** — keep `monitor.rs:1197-1201` verbatim (shell convention, `143` for TERM):
```rust
let code = status.code().unwrap_or_else(|| {
    use std::os::unix::process::ExitStatusExt;
    status.signal().map_or(-1, |signal| 128 + signal)
});
std::fs::write(&exit_file, format!("{code}\n"))?;
```

**Listener loop reference and its anti-patterns** — `.planning/spikes/socket-supervisor/main.rs:99-160`. Copy the
single-line request read and the `0600` chmod (`:80`). Do NOT copy: the `stopping` flag checked before `try_wait` with
no recheck (`:108-124`, the R-2 race), `std::process::exit` on shutdown (`:124,156`, abandons the lock), the escalation
loop that `break`s once the leader exits (`:139-147`, Pitfall 9), and the unlink-then-bind takeover (`:70-76`, R-1).

**Return value, not advance** — the core never calls advance (the CLI owns it). Today `run_pipe_owning_monitor`
returns `Result<i32, MonitorError>` and the CLI advances unconditionally (`pipeline_launch.rs:979-992`). The new
`run_supervisor` returns `SupervisorOutcome::{Completed(code), Stopped}` and `run_monitor` advances only on
`Completed`.

**Test pattern for the R-2 interleaving test (V-14)** — make the owner decision a pure function and drive the
interleavings with barriers, as `lock.rs:751-787` does for reclaim serialization:
```rust
let reclaim_ready = std::sync::Arc::new(std::sync::Barrier::new(2));
let allow_reclaim = std::sync::Arc::new(std::sync::Barrier::new(2));
let first = std::thread::spawn(move || {
    let coordination = acquire_coordination(&first_path).expect("first reclaimer coordinates");
    acquire_after_existing(first_path, coordination, || {
        first_ready.wait();
        first_allow.wait();
    })
});
reclaim_ready.wait();
// act in the held window, then:
allow_reclaim.wait();
```
The `before_reclaim: impl FnOnce()` seam on a production function (`lock.rs:240-244`) is the established way to hold a
window open deterministically; copy it rather than sleeping.

**Signal-mask assertion for the parity test (V-08, Pitfall 2)** — the parser exists, `monitor.rs:3120-3139`
(`sigterm_in_mask(pid, field)`; it panics when the field is absent, so an unreadable mask never reads as clear). Reuse
it with `"SigBlk"` to assert an empty blocked mask in the agent.

---

### `crates/devflow-core/src/monitor.rs` (service, event-driven) — MODIFY

**Keep:** the `PipeOwning` re-exec at `monitor.rs:436-469` becomes the only arm:
```rust
let mut child_cmd = hermetic_command(&binary, workdir_path);
child_cmd
    .args(pipe_owning_monitor_args(project_root, state, workdir, prompt_file, idle.timeout.as_secs()))
    .arg(program)
    .args(args)
    .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())));
strip_notify_env(&mut child_cmd);
let child = child_cmd
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()?;
```
Add `.process_group(0)` on this `__monitor` command (RESEARCH Finding 6, a Claude-remit recommendation the plan must
state). The arg vector builder `pipe_owning_monitor_args` (`monitor.rs:595-620`) gains `--socket`, `--launch-id`,
`--launch legacy|pipe_owning`, and `--keep-env NAME` entries, before the `"--"` separator; its test
`pipe_owning_monitor_args_pass_the_stage` (`monitor.rs:1584-1594`) is the pattern for "flag parsed before child argv".

**Delete (D-03):** the Legacy script and stop-marker machinery `monitor.rs:472-593`, `MonitorTail` (`:318-327`),
`advance_tail` (`:622-630`), `shell_escape` (`:1560-1562`) and their tests `legacy_advance_tail_passes_the_launched_stage`
(`:1572`), `shell_escape_*` (`:2178`, `:2653`, `:2659`), the dash/bash trap-window tests (`:2869-3450` region:
`sigterm_before_the_agent_child_resets_its_traps_still_kills_the_agent` `:3166`, `..._after_a_held_agent_child_execs..`
`:3213`, `..._before_the_monitor_records_the_agent_pid..` `:3249`, `a_stale_stop_marker_does_not_stop_the_next_agent`
`:3305`, `a_term_after_the_agent_exits_leaves_no_stop_marker` `:3371`). Their behaviour (TERM always reaches the agent)
must be re-proved by V-16, not dropped: list each deleted test against the new test that covers it.
`crate::agent_result::stop_marker_path` becomes unused once the script goes; check for other callers before removing
it (it is `pub` in a published crate).

**`MonitorLaunch` doc** (`monitor.rs:261-282`) still describes Legacy as "the pre-31 detached `sh` script"; rewrite it
to "Rust supervisor, stdin null, stdout to file". Variants stay (R-9 table: no change).

**Error enum** (`monitor.rs:27-41`): add `SocketPath(..)`, `PgidMismatch`, `NotReady`, and `#[non_exhaustive]` in the
same break. Keep the existing `#[error("...")]` phrasing style.

**Test rewrites** — `spawn_monitor_captures_agent_pid_and_output` (`monitor.rs:2697-2729`) and
`sigterm_to_monitor_also_kills_the_agent` (`monitor.rs:2773-2867`) are the templates for both-arm tests: the
`proc_snapshot` diagnostic helper (`monitor.rs:2739-2771`, "Test-only; never used in a decision") and the rule in the
comment at `:2829-2830` ("Do NOT widen this window: a failure here is a lost TERM, not a slow one") carry over. These
tests spawn a real `current_exe()` re-exec once both arms re-exec `__monitor`; the in-crate test binary is not
`devflow`, so the core tests must either call `run_supervisor` in-process on a thread or go to a CLI test. Planner
decision; flag it in the plan (the existing comment at `crates/devflow-core/tests/monitor_e2e.rs:7-12` documents the
same limit for the old `advance` tail).

---

### `crates/devflow-core/src/lock.rs` (utility, CRUD) — MODIFY (999.128)

**Change site** `lock.rs:248-253` (today a pid-only check):
```rust
if crate::agent::agent_running(record.pid) {
    return Err(LockError::Contended { pid: record.pid.to_string(), path });
}
```
**Replacement pattern** is already in the file, `lock.rs:396-400`:
```rust
classify_holder_record(
    record,
    crate::agent::agent_running(record.pid),
    crate::agent::process_start_time(record.pid),
)
```
Reclaim on `NoHolder | Recycled { .. }`; stay `Contended` on `Live | Unconfirmable`. The change stays inside
`acquire_after_existing`, which already holds the coordination flock, so
`lock::tests::stale_reclaim_is_serialized_before_it_removes_the_lock_path` (`lock.rs:751`) keeps covering
serialization. Update the `tracing::warn!` text at `lock.rs:255-259` ("holder pid {} is not alive") so a recycled
reclaim says so.

**Test fixture** for the new test and its control — copy `lock.rs:855-864`:
```rust
let pid = std::process::id();
let start = crate::agent::process_start_time(pid).expect("the test process must expose its start time");
fs::write(&path, format!("{pid}\n{}", start.saturating_add(1))).unwrap();   // Recycled
// control: format!("{pid}\n{start}") must stay Contended
```

### `crates/devflow-core/src/recover.rs` — MODIFY (flip one test)

`recover::tests::clean_phase_returns_without_cleanup_for_a_recycled_pid_lock` (`recover.rs:1117-1150`) builds a
`Recycled` lock and asserts `RecoverError::Lock(LockError::Contended { .. })` plus byte-identical artifacts. That test
pins the 999.128 defect. Rename it and invert the assertions (cleanup happens, lock reclaimed). Keep a `Live` control
in the same module (the same fixture with `observed` rather than `observed + 1`).

---

### `crates/devflow-core/src/state.rs` (model) — MODIFY

Field to remove (named R-9 break), `state.rs:284-289`:
```rust
/// PID of the detached monitor process that owns the agent for the
/// current stage, recorded by `launch_stage` at spawn time. ...
#[serde(default)]
pub monitor_pid: Option<u32>,
```
Add `#[serde(default)] pub supervisor: Option<SupervisorRecord>` with the same doc style (what `None` means). `State`
is `#[non_exhaustive]` (`state.rs:34`, rationale `:28-32`), so the add is additive. `SupervisorRecord` should derive
`Debug, Clone, Serialize, Deserialize` like `State` (`state.rs:33`), and be `#[non_exhaustive]` itself.

Blast radius of the removal (counted this session with `rg -c monitor_pid`): `state.rs` 16, `pipeline_launch.rs` 36,
`commands.rs` 55, `test_support.rs` 10, `preflight.rs` 5, `pipeline_gate.rs` 2, `main.rs` 1 (doc), `registry.rs` 1
(doc), tests `phase7_cli.rs` 16, `start_lock_e2e.rs` 6, `stop_e2e.rs` 2, `snapshots/devflow-help.txt` 1. Grep string
literals too (`"monitor_pid"` in the `stage_launched` event, `pipeline_launch.rs:1101`, and the
`value.as_object_mut().unwrap().remove("monitor_pid")` at `pipeline_launch.rs:2865`).

### `crates/devflow-core/src/registry.rs` — MODIFY

Add `pub fn socket_dir_in(cache_dir: &Path) -> PathBuf { cache_dir.join("sock") }` next to `roots_dir_in`
(`registry.rs:66-68`, identical shape). Widen `path_digest` (`registry.rs:88`) to `pub(crate)`; it has its own test
`path_digest_is_stable_and_distinguishes_different_paths` (`registry.rs:543`). Update the module doc's "same code path
that already writes `state.monitor_pid`" sentence (`registry.rs:8-9`).

### `crates/devflow-core/src/gates.rs` — MODIFY (additive)

Copy `poll_response` (`gates.rs:284-310`) into `poll_response_cancellable(.., cancel: impl Fn() -> bool)`; the only
changes are `let sleep = backoff.min(deadline - waited).min(Duration::from_secs(1));` and a `cancel()` check each
iteration. Keep the original function unchanged (callers and semver).

### `crates/devflow-core/src/lib.rs` — MODIFY

Insert `pub mod supervisor;` alphabetically in the list at `lib.rs:54-86`:
after `pub mod state;` (`lib.rs:78`) and before the cfg-gated `test_support` (`lib.rs:82`).

### `crates/devflow-core/tests/monitor_e2e.rs` — MODIFY

Imports `spawn_monitor` / `MonitorLaunch` (`monitor_e2e.rs:18`) and calls the current signature. Update to the new
`spawn_monitor` shape and cache-dir argument; its file-level comment (`:7-12`) about the tail being a no-op in the
library harness must be re-checked against the re-exec design.

---

### `crates/devflow-cli/src/main.rs` (route) — MODIFY

Extend the hidden `__monitor` variant, `main.rs:137-166`, following its field style (`#[arg(long)]`, one doc line per
field explaining why it exists, `argv` last with `trailing_var_arg = true, allow_hyphen_values = true, required = true`).
Dispatch at `main.rs:588-606` passes the new fields to `run_monitor`. The `Stop` doc at `main.rs:343-350` says stop
"signals the process recorded in its per-phase lock file ... never `state.monitor_pid`"; it is clap help text, so a
rewrite changes `crates/devflow-cli/tests/snapshots/devflow-help.txt` and needs the regen recipe at
`help_snapshot.rs:1-7` (`cargo run -q -p devflow -- --help > crates/devflow-cli/tests/snapshots/devflow-help.txt`).

### `crates/devflow-cli/src/pipeline_launch.rs` (controller) — MODIFY

**`run_monitor`** (`pipeline_launch.rs:918-993`) becomes the body for both arms. Keep the RAII-guard-bound-to-a-named-
variable pattern and its warning, `:940-977`:
```rust
let _auto_chain_guard = match workflow::load_state(project_root, phase) {
    Ok(state) => Some(AutoChainGuard::engage(workdir, auto_chain_flag_eligible(state.stage, state.mode))),
    Err(err) => { warn!("monitor could not load state for phase {phase} ({err}) — running without the GSD chain-flag guard"); None }
};
```
The comment at `:951-960` ("being inside this function already implies a stream launch") becomes false once Legacy
also enters `run_monitor`; the chain-flag guard must be conditioned on the launch shape explicitly. The tail
`advance_for_stage(project_root, Some(phase), stage)` (`:992`) runs only on `SupervisorOutcome::Completed`.

**Stop token through safe points** — the scoped-guard analog is `AutoChainGuard` (`pipeline_launch.rs:865-901`,
`engage` + `Drop`). A `thread_local!` RAII guard set by `run_monitor` follows the same engage/drop shape; RESEARCH
prefers it over a process-global static because parallel in-crate tests would share a static.

**`spawn_agent_and_record`** (`pipeline_launch.rs:1007-1110`) — the WR-04 pre-clear at `:1015-1024`
(`state.monitor_pid = None; workflow::save_state(state)?;`) must NOT be copied for `supervisor` (RESEARCH Finding 2,
"WR-04 pre-clear pitfall"): keep the old record until one save replaces it, clear only on the failure path.
The record-after-spawn pattern at `:1062-1068` stays:
```rust
let pid = monitor::spawn_monitor(state, program, args, extra_env, launch)
    .map_err(|err| CliError::Message(format!("could not spawn monitor: {err}")))?;
state.monitor_pid = Some(pid);
workflow::save_state(state)?;
```
with `Published` (ping-confirmed) replacing `pid`. The registry call `let _ = devflow_core::registry::register(..)`
(`:1093`) is where in-crate tests leak into the real `HOME` (Finding 7c); route it through the injectable cache dir.
The `stage_launched` event payload (`:1094-1103`) and the `println!` (`:1104-1108`) both name the monitor pid; replace
with `socket_path`, `launch_id`, `launch`.

**`advance_with`** (`pipeline_launch.rs:1619-1660`) — lock acquisition and failure-event pattern to keep:
```rust
let _lock = match lock::acquire_blocking(project_root, phase, lock_wait) {
    Ok(guard) => guard,
    Err(lock::LockError::Contended { pid, path: _ }) => {
        events::emit(project_root, phase, "advance_failed",
            serde_json::json!({ "reason": "lock wait expired", "holder_pid": pid, "waited_secs": lock_wait.as_secs_f64() }));
        return Err(CliError::Message(format!(...)));
    }
    Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
};
let mut state = workflow::load_state(project_root, phase)?;
```
Safe point 1 goes right after this `load_state` (`:1654`). The stage-mismatch refusal at `:1660-1676` is the template
for a `state.stopped` refusal (`advance_failed {reason: "stopped"}`); RESEARCH A6 says grep every `advance_with`
caller first.

**`relaunch_checkpoint_session`** (`:1236-1282`) hardcodes `monitor::MonitorLaunch::Legacy` (`:1280`); unchanged in
shape, but it now reaches the Rust supervisor, which is how the live rig crosses both arms (D-16).

**`resume`** (`:1412-1535`) — lock-then-guard order to preserve, `:1418-1427`: `lock::acquire` →
`crate::commands::refuse_resume_over_a_live_run(project_root, phase)?` → `load_state`.

**In-crate tests (V-07, V-13, V-15)** — template `launch_stage_persists_monitor_pid_for_reload`
(`pipeline_launch.rs:2133-2194`): the child-process wrapper `in_child_test` / `run_test_in_child` /
`assert_child_ran_exactly_one_passing_test` (`:2136-2141`, or the `enter_agent_free_child!` macro `:1973-1985`),
`env_lock()`, a tempdir root with `init_repo`, and `ReapMonitorOnDrop::after_launch(&state)` bound right after the
last `&mut state` use. `devflow_core::test_support::run_test_in_child` already takes
`extra_env: &[(&str, &OsStr)]` and sets it on the child `Command` (`crates/devflow-core/src/test_support.rs:154-179`),
so tests that already use this wrapper can receive a per-test `DEVFLOW_CACHE_DIR` with no global `set_var` and no new
`OnceLock` override. Tests that call `launch_stage` without the wrapper still need the injectable cache dir; which
ones do was not enumerated this session (unverified).

For lock-holding interleavings, copy `advance_waits_for_a_held_lock_then_proceeds` (`:3214-3237`): hold
`lock::acquire` in the test, release from a `std::thread::scope` thread, call `advance_with` with an explicit wait.

### `crates/devflow-cli/src/commands.rs` (controller) — MODIFY

**`Liveness`** (`commands.rs:984-1027`): enum plus `describe()` plus the pure predicate `liveness(...)` with no I/O.
Keep that three-part shape for the replacement (pure mapping from `Result<supervisor::Liveness, ProbeError>` plus
`stopped`/stage to a presentation enum), so `check_supervisor` can reuse it the way `check_dead_monitor` reuses
`liveness` "so the two copies can never drift" (`:3811-3817`). The matrix test `liveness_matrix_covers_all_four_rows`
(`:5471-5477`) is rewritten, not deleted.

**`cleanup`** (`:899-944`) and **`status`** (`:1117-1139`) call sites: replace the `monitor_pid` reads
(`:906-908`, `:1117-1128`) with one probe; `recovery_hints(state, phase_liveness)` (`:1138`) is where the
"each GONE/STALE report names the repair verb" rule lands.

**`stop`** (`:1882-1895`): order is `stop_via_gate` → (new socket path) → `stop_via_lock` fallback →
`persist_stopped_state`. The identity-confirmation refusals in `stop_via_lock` (`:2058-2101`) are the model for every
backstop refusal message: name the pid, the recorded versus observed start time, and a manual inspection command. The
doc comments at `:1871-1881` and `:2033-2038` cite `monitor_pid`/T-23-51 and must be rewritten with the code.
`persist_stopped_state` (`:2121-2160`) stays, as an idempotent append after the final `stopped` reply or GONE.

**`refuse_launch_over_a_live_run`** (`:2470-2492`), the shared start/resume guard (R-12):
```rust
let loaded = workflow::load_state(project_root, phase);
if loaded.is_err() && !workflow::state_path(project_root, phase).exists() {
    return Ok(());
}
let monitor = loaded.ok().and_then(|state| state.monitor_pid).filter(|&pid| agent::agent_running(pid));
let agent = agent_pid_from_file(project_root, phase).filter(|&pid| agent::agent_running(pid));
let live: Vec<(&str, u32)> = [("monitor", monitor), ("agent", agent)]
    .into_iter().filter_map(|(role, pid)| pid.map(|pid| (role, pid))).collect();
```
Keep the no-state carve-out (first `if`) and the agent-pid-file half (D-12). Replace the `monitor` line with: state
loads → probe `state.supervisor`; state exists but will not load → `supervisor::sweep_in` filtered to this root and
phase. The refusal text builders `live_run_refusal` / `start_live_run_refusal` (`:2497-2510`) take `(&str, u32)`
pairs; the socket case needs a variant that names a socket path, not a pid. Several sentences of today's refusal
(`sh -c` script, `devflow advance` foreground child, "send SIGTERM to the confirmed monitor first") describe the
deleted Legacy shape; the pinned fragments in `start_lock_e2e.rs:427-482` and the golden at
`start_lock_e2e.rs:619-625` must change with it.

**`doctor`** — `PhaseFacts` (`:3694-3722`) replaces `monitor_pid`/`monitor_alive` with
`supervisor: Result<Liveness, ProbeError>`; facts are built with all I/O in `build_phase_facts` (`:3920-3964`) and
reconciled with zero I/O in `reconcile_phase` (`:3881-3894`). Add `check_supervisor` to the fixed-order list at
`:3882-3890`. Finding format is `PhaseFinding { phase, severity, detail, repair }` (`:3727-3732`), with the rule
"never carries a filesystem path or username (T-18-01)" (`:3724-3726`). That rule conflicts with printing a socket
path (which contains `$HOME`) in a finding; the plan must decide whether the sweep finding shows the path. Existing
`PhaseFacts` literals in tests that set `monitor_alive` (`:6992`, `:7175`, `:7227`, `:7249`, `:7267`, `:7288`) need
updating.

**`registry_reachable_pids`** (`:4337-4380`): drop the `state.monitor_pid` input at `:4372`.

**`recover_cmd`**: fix the stale "clear it regardless of staleness" comment at `:2739` (999.128 entry).

### `crates/devflow-cli/src/pipeline_gate.rs` — MODIFY

`pipeline_gate.rs:68-75`: `state.monitor_pid = None;` beside `state.gate_pending = false;` on the `--until` stop.
Becomes `state.supervisor = None;` (same comment, updated).

### `crates/devflow-cli/src/test_support.rs` — MODIFY

`ReapMonitorOnDrop` (`test_support.rs:567-636`) captures `state.monitor_pid` by value in `after_launch` (`:592-596`)
and reaps by pid in `Drop`. Rewire it to capture the socket path (and the agent pgid from the ping reply) and reap by
socket `shutdown`. Keep the double-panic interlock verbatim (`:604-633`): `if std::thread::panicking()` → raw
`writeln!(std::io::stderr(), ..)` not `eprintln!`, else `panic!`.

### `crates/devflow-cli/src/preflight.rs` — MODIFY (tests only)

Assertions `state.monitor_pid.is_some()` at `preflight.rs:2248`, `:2327`, `:2439` (and comments `:2205`, `:2279`)
become `state.supervisor.is_some()`.

---

### `crates/devflow-cli/tests/common/mod.rs` (test utility) — NEW (999.141, Wave 1, first)

**Analogs:** `gate_notify_persist_e2e.rs:111-136` and `start_lock_e2e.rs:74-94`.

The per-child cache isolation that exists today, `gate_notify_persist_e2e.rs:121-127`:
```rust
/// A private `DEVFLOW_CACHE_DIR` for one spawned child, so its root
/// registration never lands in the developer's real `~/.cache/devflow`.
fn isolated_cache(command: &mut Command) -> tempfile::TempDir {
    let cache = tempfile::tempdir().unwrap();
    command.env("DEVFLOW_CACHE_DIR", cache.path());
    cache
}
```
The owner struct that keeps the tempdir alive past the child, `gate_notify_persist_e2e.rs:115-136`:
```rust
struct ReapOnDrop(
    Child,
    #[expect(dead_code, reason = "held only so the cache dir outlives the child")]
    tempfile::TempDir,
);
impl Drop for ReapOnDrop {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) { let _ = self.0.kill(); }
        let _ = self.0.wait();
    }
}
```
The child builder with PATH prepended for a fake agent, `start_lock_e2e.rs:74-94` (`Command::new(devflow_bin())`,
`.current_dir(root)`, `.env("PATH", path)`). `hermetic_devflow(home: &TempDir) -> Command` combines these: set `HOME`,
`XDG_CACHE_HOME`, `XDG_CONFIG_HOME`, `DEVFLOW_CACHE_DIR`, and remove the `GIT_*` set with the loop from
`crates/devflow-core/src/git.rs:87-94`:
```rust
for var in REPO_LOCAL_GIT_VARS.iter().chain(ALSO_REDIRECTING_GIT_VARS) {
    cmd.env_remove(var);
}
```
(re-exported as `devflow_core::test_support::{REPO_LOCAL_GIT_VARS, ALSO_REDIRECTING_GIT_VARS}`,
`crates/devflow-core/src/test_support.rs:138-140`). `devflow_bin()` is `env!("CARGO_BIN_EXE_devflow")`
(`start_lock_e2e.rs:19-21`); it is defined separately in 16 files today and moves here. Integration-test shared modules
are included per test file with `mod common;`; an unused helper in one file needs `#[allow(dead_code)]` or per-fn
`#[expect]`, since `-D warnings` applies.

**Monitor reaper** — the current e2e reaper is `start_lock_e2e.rs:677-713` (`LaunchedRunReaper` reads
`state.monitor_pid` and the agent pid file on drop, spares test-owned pids, calls `terminate_and_verify`). The new one
reaps by socket `shutdown` from `state.supervisor`, then falls back to the agent pgid.

### `crates/devflow-cli/tests/test_home_isolation.rs` (test) — NEW (V-26)

**Analog:** `crates/devflow-cli/tests/git_env_hermeticity.rs` (a suite-level guard against environment leakage,
file doc `:1-22` explains detection-not-prevention). The run itself copies `start_lock_e2e.rs::run_start`
(`:377-383`) with a fake `claude` from `fake_bin_dir` (`:58-72`). The negative control (a plain `Command` whose `HOME`
names the fake "real home" must populate it) is the same "the assertion can fail" rule as
`assert_start_refuses_live_run`'s snapshot self-check (`start_lock_e2e.rs:404-417`).

### `crates/devflow-cli/tests/supervisor_e2e.rs` (test) — NEW (V-12, fault fixtures)

**Analog:** `crates/devflow-cli/tests/start_lock_e2e.rs`. Reuse its building blocks once they move to `common/`:
`init_repo` (`:35-51`), `fake_bin_dir` (`:58-72`), `wait_for` with a bounded timeout (`:96-105`),
`kill_and_reap` (`:107-112`), `LiveProcess` (`:276-300`, killed and waited on drop, no `expect` in `drop`). V-12 spawns
two `devflow start` children with `.spawn()` (not `.output()`), waits both, then counts `*.sock` in the child's
`sock/` dir: exactly one ALIVE, one refusal. The file-level doc should state the CLI-boundary rationale as
`stop_e2e.rs:7-11` does (`devflow` has no `lib.rs`, so integration tests drive the binary).

### `crates/devflow-cli/tests/stop_e2e.rs` (test) — MODIFY

- `stop_never_signals_the_recorded_monitor_pid` (`stop_e2e.rs:176-202`) plants `state.monitor_pid = Some(monitor.id())`
  and asserts the process survives. Rewrite it as "stop never signals by a recorded pid; it addresses the socket"
  (RESEARCH 7b), keeping the structure: plant, run `devflow stop --phase N --root`, assert survivor, `kill_and_reap`.
- V-17 `stop_ends_a_lock_free_live_run_through_the_socket`: template `stop_marks_stopped_after_the_signalled_lock_holder_exits`
  (`:107-140`) for the assertion style (`state.stopped`, `stop_reason` contains `devflow stop`).
- The `write_live_lock` fixture (`:92-98`) stays for the `stop_via_lock` fallback tests.
- All 27 `Command::new(devflow_bin())` sites here move to `common::hermetic_devflow` (none set a cache dir today).

### `crates/devflow-cli/tests/start_lock_e2e.rs` (test) — MODIFY

- `plant_leftover_run` (`:308-336`) and `plant_resumable_run` (`:633-656`) set `state.monitor_pid`; the planted live
  monitor becomes a test-owned supervisor (or a bound listener answering ping) plus a sidecar.
- V-23 `start_refuses_over_a_live_supervisor_when_the_state_will_not_load`: plant a corrupt `state-NN.json`
  (`devflow_core::workflow::state_path(root, phase)`) and assert via `assert_start_refuses_live_run`'s pattern
  (`:395-516`: snapshot before/after, no lock file left, no phase branch, no `workflow_started`).
- Controls that must stay green: `start_proceeds_when_no_state_exists_even_if_the_agent_pid_file_names_a_live_process`
  (`:587-613`).
- `resume_command` (`:658-675`) already sets `DEVFLOW_CACHE_DIR`; `child` (`:74-94`) does not.

### `crates/devflow-cli/tests/recover_clean_e2e.rs` (test) — MODIFY

999.128 CLI repair test: copy `explicit_clean_with_a_held_phase_lock_fails_and_cleans_nothing` (`:65`) and
`explicit_clean_without_a_lock_holder_cleans_and_says_so` (`:99`); the new case writes a `Recycled` lock record
(this test's own pid, start time + 1, as in `lock.rs:860`) and asserts the clean succeeds.

### `crates/devflow-cli/tests/phase7_cli.rs` and the other e2e files — MODIFY (mechanical)

Site counts from `rg -c "Command::new\(devflow_bin\(\)\)"`: `stop_e2e.rs` 27, `phase7_cli.rs` 12, `gate_wedge_e2e.rs` 7,
`gate_notify_persist_e2e.rs` 5, `gate_sweep_e2e.rs` 4, `start_reachability_e2e.rs`, `start_lock_e2e.rs`,
`log_format_env.rs`, `doctor_antigravity.rs`, `auto_chain_leak_repair_e2e.rs` 2 each, `yes_ship_config.rs`,
`release_check.rs`, `recover_clean_e2e.rs`, `reap_strays_e2e.rs`, `help_snapshot.rs`, `auto_chain_flag_e2e.rs` 1 each
(71 total, 16 files; matches RESEARCH 7c). `phase7_cli.rs` also has 16 `monitor_pid` references.

---

### `.planning/phases/50-addressable-monitor-liveness/tools/fault-arm.sh` (script, batch) — NEW

**Analog:** `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/negctl.sh` (259 lines): one unit
owned per run, a built-in `selftest()` that fakes `systemd-run`/`systemctl` in a `mktemp -d` dir cleaned by
`trap 'rm -rf -- "$tmp"' RETURN`, and the header:
```bash
#!/usr/bin/env bash
# Run the unconfigured scratch arm. Own exactly one unit; never publish.
set -euo pipefail
T="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SELF="$T/$(basename "${BASH_SOURCE[0]}")"
ROOT="$(git -C "$T" rev-parse --show-toplevel)"
```
The P50 copies of `launch-attempt.sh`, `collect-attempt.sh` (extend to copy `sock/*.json` sidecars), `check-evidence.sh`,
`sanitize-evidence.py`, `snapshot-hook.sh`, `surv-watch.py`, `EVIDENCE-LAYOUT.md` come from the same `tools/` dir.
RESEARCH Finding 10 notes the `p49-a<N>` unit names are hard-coded at 14 sites. Validate with `bash -n` and
`shellcheck` (global rule).

## Shared Patterns

### Process identity before any signal
**Source:** `crates/devflow-core/src/agent.rs:172-230` (`process_start_time`, `is_same_process`)
**Apply to:** backstop, lock reclaim (999.128), `stop`, sidecar validation
```rust
pub fn is_same_process(pid: u32, expected_start: u64) -> bool {
    process_start_time(pid) == Some(expected_start)
}
```
The doc at `agent.rs:200-210` records the 10 ms tick granularity: a parent and a child spawned within one tick share
a start time, so start time must never be used to tell a parent from its own child (compare pids for that).
`None` is the fail-closed direction (`agent.rs:212-213`).

### Pid hardening on anything read from disk
**Source:** `crates/devflow-core/src/agent.rs:36-46` (`agent_running`) and `:86-91` (`terminate`)
**Apply to:** every pid/pgid read from a sidecar or state
```rust
let Ok(signed) = libc::pid_t::try_from(pid) else { return false; };
if signed <= 0 || unsafe { libc::kill(signed, 0) } != 0 { return false; }
```
Zombies count as not running (`agent.rs:18-33`). For group signals the bound is `> 1` (`monitor.rs:1499`).

### TERM → wait → KILL escalation
**Source:** `crates/devflow-core/src/agent.rs:129-170` (`terminate_and_verify`) and `monitor.rs:1494-1522`
**Apply to:** supervisor shutdown (ALIVE path), backstop (leader-present path)
Return a verified fact, never an assumption; SIGKILL is not synchronous, so poll after it (`agent.rs:158-169`).

### Atomic writes and publishes
**Source:** `lock.rs:149-185` (`hard_link` publish, `EEXIST` normal), `workflow.rs:222-254`
(`create_unique_temp`, `write_state_atomic`), `registry.rs:222-229`
**Apply to:** socket publish (hard link only — `rename` replaces a live target, RESEARCH probe B), sidecar writes
(rename is fine), any new state write (through `workflow::save_state`, `workflow.rs:188-194`).

### Hermetic child commands
**Source:** `crates/devflow-core/src/git.rs:87-94` (`hermetic_command`), `monitor.rs:292-294` (`strip_notify_env`)
**Apply to:** the agent spawn and the `__monitor` re-exec in both arms; the e2e `common` helper
Order rule (`monitor.rs:566-570`): `hermetic_command`'s removals run at construction, `.envs(...)` after.

### Per-child test isolation, never global env
**Source:** `gate_notify_persist_e2e.rs:121-127` (e2e), `crates/devflow-core/src/test_support.rs:154-179`
(`run_test_in_child` with `extra_env`, in-crate)
**Apply to:** every test that reaches `registry::register`, a sweep, or a socket bind
Plus the reaper-on-drop rule from `test_support.rs:567-636`: bind the guard after the launch call, never panic in
`Drop` while already unwinding.

### Read-only versus acting commands (D-06)
**Source:** `registry.rs:132-139` (`prune_missing_in` is never called from `load_roots_in`), `commands.rs:3724-3726`
and `:3876-3880` (`doctor` reconciliation "takes no path, performs no I/O, and mutates nothing")
**Apply to:** `sweep_in`, `status`, `doctor`, `doctor --json` (report only); `stop` (the only verb that unlinks or
signals).

### Pure decision core, thin I/O shell
**Source:** `lock.rs:366-401` (`classify_holder_record` / `holder_status`), `commands.rs:1018-1027` (`liveness`),
`commands.rs:3881-3894` (`reconcile_phase`)
**Apply to:** backstop rule, terminal-owner state machine (V-14), liveness presentation. This is what makes the
deterministic interleaving test and `cargo mutants` runs possible.

### Operator-facing refusal text names the repair verb
**Source:** `commands.rs:2071-2099` (identity refusals in `stop_via_lock`), `start_lock_e2e.rs:427-482` (fragments
pinned by test, including "superseded guidance must be gone")
**Apply to:** every GONE/STALE report, backstop refusal, and launch-guard refusal. Pin new text the same way: each
safety sentence asserted, each removed sentence asserted absent.

## No Analog Found

| File / part | Role | Data Flow | Reason |
|---|---|---|---|
| Socket wire protocol inside `supervisor.rs` / `runtime.rs` (newline JSON `ping`/`shutdown`, 4 KiB cap, timeouts, `SO_PEERCRED`) | service | request-response | No Unix-socket or `poll(2)` code exists in the crates. Reference only: `.planning/spikes/socket-supervisor/main.rs:129-187` (shape) and RESEARCH Finding 8 table (`50-RESEARCH.md:673-682`). |
| Self-pipe `sigaction` handler in `runtime.rs` | service | event-driven | No signal handler is installed anywhere in Rust code today (999.134 is exactly that gap). Use RESEARCH Pattern 1 (`50-RESEARCH.md:796-810`). |
| `pidfd_open` / `pidfd_send_signal(PIDFD_SIGNAL_PROCESS_GROUP)` in `backstop.rs` | service | event-driven | No pidfd use in tree; RESEARCH Finding 1 step 4 plus the `EINVAL` → validate-then-`killpg` fallback. |

## Notes for the planner (found while mapping; not in RESEARCH)

1. **The in-crate cache leak has an existing seam.** `devflow_core::test_support::run_test_in_child` already sets
   `extra_env` on the child process (`crates/devflow-core/src/test_support.rs:154-179`), and the `--bin` tests that
   spawn monitors (for example `launch_stage_persists_monitor_pid_for_reload`, `pipeline_launch.rs:2133-2141`) already
   run through it. Passing `DEVFLOW_CACHE_DIR` there isolates those tests without `set_var` and without a
   `#[cfg(test)]` `OnceLock` override. Not established: whether every in-crate test that reaches
   `registry::register` uses the wrapper (not enumerated).
2. **Core monitor tests cannot re-exec `__monitor`.** Once both arms re-exec `current_exe()`, a `devflow-core` unit or
   integration test binary is not `devflow`, so the existing core tests that spawn a Legacy monitor
   (`monitor.rs:2697`, `:2773`, `crates/devflow-core/tests/monitor_e2e.rs:62`) cannot keep doing that. Either test
   `run_supervisor` in-process on a thread, or move the spawn-level tests to the CLI crate.
3. **`doctor`'s no-path rule versus socket paths.** `PhaseFinding` is documented as never carrying a filesystem path
   or username (`commands.rs:3724-3726`); a socket path under `$HOME/.cache` carries both. The sweep finding needs a
   decision on what it prints.
4. **Help snapshot.** Any edit to the `Stop` or `Monitor` doc comments in `main.rs` changes
   `tests/snapshots/devflow-help.txt` only if the variant is visible (`Monitor` is `hide = true`, so only `Stop`).
5. **`agent_result::stop_marker_path`** loses its only known caller when the Legacy script is deleted; it is `pub` in
   a published crate, so removing it belongs in the R-9 named list or it stays.

## Metadata

**Analog search scope:** `crates/devflow-core/src/` (monitor, lock, agent, registry, state, gates, recover, workflow,
git, test_support, lib), `crates/devflow-core/tests/`, `crates/devflow-cli/src/` (pipeline_launch, commands, main,
pipeline_gate, test_support, preflight), `crates/devflow-cli/tests/` (stop_e2e, start_lock_e2e,
gate_notify_persist_e2e, recover_clean_e2e, git_env_hermeticity, help_snapshot), `.planning/spikes/socket-supervisor/`,
`.planning/phases/49-…/tools/`
**Files scanned:** 24 read (targeted ranges), plus `rg` counts over both crates and their test trees
**Pattern extraction date:** 2026-10-14
