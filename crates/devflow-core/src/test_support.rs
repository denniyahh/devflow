//! This crate's test-only helper surface, reachable cross-crate through the
//! `test-support` feature (`lib.rs:76-79`'s
//! `#[cfg(any(test, feature = "test-support"))]` gate keeps all of it absent
//! from a normal build). Two unrelated hazards live here, each documented at
//! its own definitions below: hermetic git invocation for test fixtures
//! (999.37, the module's original scope) and the 999.47 exec-visibility
//! barrier (25-11).
//!
//! ## Hermetic git invocation (999.37)
//!
//! Test fixtures build throwaway repositories in tempdirs and shell out to
//! `git` against them. Pinning the working directory is **not** sufficient to
//! keep those commands inside the fixture: git's repository-local environment
//! variables — `GIT_DIR` above all — outrank a process's working directory
//! when git resolves which repository to act on. `git -C <dir>` does not
//! override `GIT_DIR` either (only the `--git-dir` flag does), and
//! `GIT_CEILING_DIRECTORIES` does not contain it, so clearing the variables is
//! the only reliable containment.
//!
//! This matters because Rust runs a test binary's tests as threads in ONE
//! process: if the suite is launched with those variables set, every fixture
//! inherits them at once. That is exactly what happened when a `git push` from
//! a linked worktree ran the pre-push hook (git exports `GIT_DIR` to hooks
//! when the gitdir is non-default) — fixtures retargeted the real checkout,
//! setting `core.bare=true` on it, rewriting its committer identity, and
//! stacking fixture commits onto its `main` branch.
//!
//! `scripts/hooks/pre-push` clears these before running the suite, and
//! `git_env_hermeticity.rs` fails fast if they are present. Both are process
//! level. This helper is the per-command layer: a fixture built through it is
//! contained even when the environment is dirty and whatever the launch path.

// ## Exec-visibility barrier (25-11/999.47)
//
// `process_start_time`'s doc comment (`crate::agent`) is this codebase's own
// authoritative statement of the mechanism: between `Command::spawn()`
// returning and the child completing `execve`, the child is a copy of its
// parent, so `/proc/<pid>/cmdline` transiently reports the PARENT's argv,
// not the child's own. A test that spawns a child and immediately reads a
// `/proc`-cmdline census about it (via
// `crate::agent::discover_stray_devflow_processes` or its CLI-side
// equivalent) races that window, load-sensitively — 0 failures across 17
// warm local runs, 2 failures in 2 attempts under the loaded shape
// `scripts/check-in-container.sh all` runs (`25-CI-OBSERVATION.md`).
//
// `wait_for_exec_visibility` is the barrier a test must cross before
// asserting on such a census. `crate::agent::agent_running` is NOT such a
// barrier: `kill(pid, 0)` succeeds for a forked-but-unexec'd child, because
// the pid is allocated at `fork()`, well before `execve` runs — a liveness
// poll closes no window at all here.

/// Bounded default wait for [`wait_for_exec_visibility`]. The window this
/// barrier waits out is sub-millisecond in the normal case and the function
/// returns immediately once the child has exec'd, so a generous ceiling
/// costs nothing in the common path — it exists only so a pathological case
/// (a child that never execs, or a wrong `expected_argv0_basename`) fails
/// loudly within a bounded time instead of hanging a test binary.
pub const EXEC_VISIBILITY_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Poll interval for [`wait_for_exec_visibility`]. Matches the granularity
/// [`crate::agent::TERMINATE_VERIFY_POLL`] uses for the same reason: fine
/// enough that the barrier resolves promptly once the condition is true,
/// coarse enough not to busy-loop.
pub const EXEC_VISIBILITY_POLL: std::time::Duration = std::time::Duration::from_millis(2);

/// Poll `/proc/<pid>/cmdline` until `pid`'s argv is genuinely its OWN — not
/// its parent's, transiently inherited across the `fork()`->`execve()`
/// window — and report whether that happened within `wait`.
///
/// Returns `true` only when BOTH hold, checked on every poll:
///
/// (i) argv[0]'s basename equals `expected_argv0_basename`, via
///     [`crate::agent::argv_basename`] — the exact idiom
///     `classify_stray_layer` uses, reused rather than copied so a second
///     basename idiom cannot drift from the first.
/// (ii) the observed cmdline is not byte-identical to the caller's own
///     `/proc/self/cmdline`, captured once at call time. This is the guard
///     against the degenerate case where the caller's own argv[0] basename
///     happens to equal `expected_argv0_basename` — without it, a test
///     asserting on its OWN pid could pass merely because it started out
///     matching, which would make the barrier's answer
///     probabilistically-correct instead of unambiguous.
///
/// Parses the NUL-separated cmdline the same way
/// [`crate::agent::discover_stray_devflow_processes`] does. An unreadable
/// `/proc/<pid>/cmdline` is not itself a failure — a pid that has not yet
/// appeared, or has already exited, is exactly the kind of transient state
/// this function polls through — but a pid that is verifiably not alive
/// (checked via [`crate::agent::agent_running`] on every iteration) returns
/// `false` immediately rather than waiting out the full `wait` ceiling: a
/// dead pid can never become exec-visible, so there is nothing to wait for.
pub fn wait_for_exec_visibility(
    pid: u32,
    expected_argv0_basename: &str,
    wait: std::time::Duration,
    poll: std::time::Duration,
) -> bool {
    let self_cmdline = std::fs::read(format!("/proc/{}/cmdline", std::process::id())).ok();
    let deadline = std::time::Instant::now() + wait;

    loop {
        if !crate::agent::agent_running(pid) {
            return false;
        }

        if let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) {
            let args: Vec<String> = raw
                .split(|&byte| byte == 0)
                .filter(|arg| !arg.is_empty())
                .map(|arg| String::from_utf8_lossy(arg).into_owned())
                .collect();

            let basename_matches = args
                .first()
                .and_then(|argv0| crate::agent::argv_basename(argv0))
                .is_some_and(|name| name == expected_argv0_basename);
            let differs_from_caller = self_cmdline.as_deref() != Some(raw.as_slice());

            if basename_matches && differs_from_caller {
                return true;
            }
        }

        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(poll);
    }
}

// Hermetic git invocation constants and constructors now live in
// `crate::git` — the always-compiled home (this module is gated
// `#[cfg(any(test, feature = "test-support"))]`, absent from a normal
// build, so it cannot be the canonical home for a production-reachable
// constructor — 27-01/D-01). Re-exported here so every existing fixture
// call site (~40 across both crates' test targets) keeps compiling
// unchanged, and so the two lists can never drift apart.
pub use crate::git::{
    ALSO_REDIRECTING_GIT_VARS, REPO_LOCAL_GIT_VARS, git_command, hermetic_command,
};

/// Environment marker for a test process that was re-invoked by
/// [`run_test_in_child`]. Its value is the exact module-qualified test name.
pub const CHILD_TEST_ENV: &str = "DEVFLOW_CHILD_TEST";

/// Returns whether this process is the child half for `test_name`.
pub fn in_child_test(test_name: &str) -> bool {
    std::env::var_os(CHILD_TEST_ENV).is_some_and(|value| value == std::ffi::OsStr::new(test_name))
}

/// The pids of the live members of process group `pgid`: every `/proc/<pid>`
/// whose group is `pgid` and whose state is not `Z`.
///
/// A zombie has exited and only waits to be reaped, and an orphan is reaped
/// by PID 1. A GitHub Actions job container's PID 1 is `tail -f /dev/null`,
/// which never reaps, so there a killed agent's orphaned children stay
/// zombies for the life of the job and `kill(-pgid, 0)` keeps reporting the
/// group. "Is the group gone?" must ask this, never `kill(-pgid, 0)`. It is
/// the rule `supervisor::backstop`'s census applies.
pub fn live_group_members(pgid: u32) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter(|&pid| {
            crate::agent::process_stat_fields(pid)
                .is_some_and(|stat| i64::from(stat.pgrp) == i64::from(pgid) && stat.state != 'Z')
        })
        .collect()
}

/// Whether `pid` is a live process: `/proc/<pid>` exists and is not a
/// zombie. See [`live_group_members`] for why a zombie never counts.
pub fn process_is_live(pid: u32) -> bool {
    crate::agent::process_stat_fields(pid).is_some_and(|stat| stat.state != 'Z')
}

/// Re-invokes this test binary for exactly `test_name` with a child-local
/// `PATH` and environment. The caller owns `path_dir` for the duration of the
/// child invocation.
pub fn run_test_in_child(
    test_name: &str,
    path_dir: &std::path::Path,
    extra_env: &[(&str, &std::ffi::OsStr)],
) -> std::process::Output {
    child_test_command(test_name, path_dir, extra_env)
        .output()
        .expect("spawn child test process with an isolated PATH")
}

/// [`run_test_in_child`], but spawned rather than waited for: the caller gets
/// the running [`std::process::Child`] (stdout and stderr piped) and owns it,
/// so it can act on the child while it runs and must wait on it afterwards.
pub fn spawn_test_in_child(
    test_name: &str,
    path_dir: &std::path::Path,
    extra_env: &[(&str, &std::ffi::OsStr)],
) -> std::process::Child {
    child_test_command(test_name, path_dir, extra_env)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn child test process with an isolated PATH")
}

/// The command both child-test helpers run: this test binary, filtered to
/// exactly `test_name`, with a child-local `PATH` and environment.
fn child_test_command(
    test_name: &str,
    path_dir: &std::path::Path,
    extra_env: &[(&str, &std::ffi::OsStr)],
) -> std::process::Command {
    assert!(
        path_dir.is_dir(),
        "child test PATH must be an existing directory: {}",
        path_dir.display()
    );

    let exe = std::env::current_exe().expect("current_exe for child test re-invocation");
    let mut command = std::process::Command::new(exe);
    command
        .arg(test_name)
        .arg("--exact")
        .arg("--test-threads=1")
        .env("PATH", path_dir)
        .env(CHILD_TEST_ENV, test_name);
    for (name, value) in extra_env {
        command.env(name, value);
    }
    command
}

/// Asserts that a child test process ran precisely its requested test and did
/// not turn an unmatched `--exact` filter into a vacuous green result.
pub fn assert_child_ran_exactly_one_passing_test(output: &std::process::Output, test_name: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let context = format!(
        "\n--- child stdout ---\n{stdout}\n--- child stderr ---\n{stderr}\n--- end child output ---"
    );

    assert!(
        output.status.success(),
        "child test process must exit 0; status {status:?}{context}",
        status = output.status
    );
    assert!(
        stdout.contains("1 passed"),
        "child must report exactly `1 passed`{context}"
    );
    let test_prefix = format!("test {test_name} ...");
    let result_summary = "test result: ok. 1 passed;";
    let test_completed = stdout.contains(&format!("{test_prefix} ok"))
        || stdout.contains(&format!("\nok\n\n{result_summary}"));
    assert!(
        stdout.contains(&test_prefix) && test_completed && stdout.contains(result_summary),
        "child stdout must show the requested test and its successful one-test result — test output may appear between `{test_prefix}` and libtest's `ok`, but the child may not run another test or none{context}"
    );

    let words: Vec<&str> = stdout.split_whitespace().collect();
    let filtered_out: u64 = words
        .windows(3)
        .find(|words| words[1] == "filtered" && words[2].starts_with("out"))
        .and_then(|words| words[0].parse().ok())
        .unwrap_or_else(|| {
            panic!("child stdout must carry a parseable `N filtered out` count{context}")
        });
    assert!(
        filtered_out > 0,
        "child must report a NON-ZERO `filtered out` count — zero is consistent with a binary containing only this test, in which case the filter proved nothing; got {filtered_out}{context}"
    );
}

/// The answer files of a stage: keyed `.response.json` files and the legacy
/// unkeyed response, sorted. Not quarantine, gate, claim or waiter files.
pub fn gate_answer_files(
    root: &std::path::Path,
    phase: crate::phase_id::PhaseId,
    stage: crate::stage::Stage,
) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(crate::gates::Gates::dir(root)) else {
        return Vec::new();
    };
    let prefix = format!("{}-{stage}.", phase.padded());
    let mut files: Vec<std::path::PathBuf> = entries
        .map(|entry| entry.expect("read a gates directory entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(std::ffi::OsStr::to_str)
                .and_then(|name| name.strip_prefix(&prefix))
                .is_some_and(|rest| rest == "response.json" || rest.ends_with(".response.json"))
        })
        .collect();
    files.sort();
    files
}

/// Answer a gate that has not opened yet: the next gate opened at this stage gets
/// `response` as its answer. Only for tests whose subject is what an answer leads
/// to, never for the binding itself.
pub fn preanswer_gate(
    root: &std::path::Path,
    phase: crate::phase_id::PhaseId,
    stage: crate::stage::Stage,
    response: &crate::gates::GateResponse,
) {
    crate::workflow::ensure_devflow_dir(&crate::gates::Gates::dir(root))
        .expect("create the gates directory");
    let body = serde_json::to_string_pretty(response).expect("serialize the response");
    let path = crate::gates::Gates::preanswer_path(root, phase, stage);
    crate::durable::replace_durably(&path, body.as_bytes()).expect("write the pre-answer");
}

/// Answer the gate the gate file names, as a human answering the gate they see.
///
/// # Errors
/// [`crate::gates::GateError::NoOpenGate`] with no gate file, or whatever
/// [`crate::gates::Gates::answer`] returns.
pub fn answer_open_gate(
    root: &std::path::Path,
    phase: crate::phase_id::PhaseId,
    stage: crate::stage::Stage,
    response: &crate::gates::GateResponse,
) -> Result<std::path::PathBuf, crate::gates::GateError> {
    let Some(live) = crate::gates::Gates::current_gate_id(root, phase, stage)? else {
        return Err(crate::gates::GateError::NoOpenGate { phase, stage });
    };
    let gate = crate::gates::GateId::parse(&live)?;
    crate::gates::Gates::answer(root, phase, stage, &gate, response)
}

/// Run `body` with `hook` installed for this thread, to run once inside the first
/// answer `body` publishes, after its id check and before its publish. That is the
/// only point at which a gate can close under a writer's publish, so a CLI test of how
/// a writer reports `GateClosedAfterPublish` installs it here; a hook run before
/// `Gates::answer` yields `GateIdMismatch` instead. No gate lock is held when it runs,
/// so it may close and open gates. A hook `body` never reached is dropped unrun.
pub fn with_answer_before_publish<R>(hook: impl FnOnce() + 'static, body: impl FnOnce() -> R) -> R {
    /// Clears the slot however `body` ends, so a later answer on this thread runs
    /// no hook.
    struct ClearOnDrop;
    impl Drop for ClearOnDrop {
        fn drop(&mut self) {
            crate::gates::set_answer_publish_hook(None);
        }
    }
    crate::gates::set_answer_publish_hook(Some(Box::new(hook)));
    let _clear = ClearOnDrop;
    body()
}

/// The file under a GSD stub's `logs` dir that holds one line per call: the
/// call's physical working directory, a tab, then its arguments joined by
/// spaces.
pub const GSD_RUN_LOG: &str = "gsd-run.log";

/// The version the stub launcher answers `runtime-identity --raw` with.
pub const GSD_STUB_VERSION: &str = "0.0.0-stub";

/// Plant a stub GSD launcher at `<home>/.claude/gsd-core/bin/gsd_run`, the first
/// place [`crate::gsd_reader::locate_launcher`] looks, so no test reaches a real
/// GSD install.
///
/// Every call appends its working directory and argv to `logs/`[`GSD_RUN_LOG`].
/// `runtime-identity` answers [`GSD_STUB_VERSION`] and consumes nothing.
/// `query verification status <dir>` answers `missing` when `<dir>` is not a
/// directory, consuming nothing. Every other call prints the next of
/// `responses`, in order, one per call; once they are used up the stub prints
/// an error and exits 97, so a test that reads more often than it scripted sees
/// a reader failure, never a repeated answer.
///
/// The script uses shell builtins only, so it runs under an agent-free `PATH`.
pub fn plant_gsd_run_stub(home: &std::path::Path, responses: &[&str], logs: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(logs).expect("create the GSD stub's logs dir");
    let logs_text = logs.to_str().expect("a UTF-8 logs path");
    assert!(
        !logs_text.contains('\''),
        "the logs path is single-quoted in the stub: {logs_text}"
    );
    for (index, response) in responses.iter().enumerate() {
        std::fs::write(
            logs.join(format!("gsd-response-{}", index + 1)),
            format!("{response}\n"),
        )
        .expect("write a scripted GSD reply");
    }
    let launcher = home.join(".claude/gsd-core/bin/gsd_run");
    std::fs::create_dir_all(launcher.parent().expect("a launcher dir"))
        .expect("create the launcher dir");
    let script = format!(
        r#"#!/bin/sh
logs='{logs_text}'
printf '%s\t%s\n' "$(pwd -P)" "$*" >> "$logs/{GSD_RUN_LOG}"
if [ "$1" = "runtime-identity" ]; then
  printf '%s\n' '{{"packageName":"@opengsd/gsd-core","version":"{GSD_STUB_VERSION}"}}'
  exit 0
fi
if [ "$1 $2 $3" = "query verification status" ] && [ ! -d "$4" ]; then
  printf '%s\n' '{{"status":"missing","next_action":"No verification report found.","next_command":"","route":"execute-phase"}}'
  exit 0
fi
n=1
if [ -f "$logs/gsd-next" ]; then read -r n < "$logs/gsd-next"; fi
if [ ! -f "$logs/gsd-response-$n" ]; then
  printf 'gsd_run stub: no scripted reply %s\n' "$n" >&2
  exit 97
fi
printf '%s\n' "$((n + 1))" > "$logs/gsd-next"
while IFS= read -r line || [ -n "$line" ]; do printf '%s\n' "$line"; done < "$logs/gsd-response-$n"
"#
    );
    std::fs::write(&launcher, script).expect("write the GSD stub launcher");
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755))
        .expect("make the GSD stub executable");
}

/// The calls a stub from [`plant_gsd_run_stub`] logged, as
/// `(working directory, argv joined by spaces)`, in call order.
pub fn gsd_run_calls(logs: &std::path::Path) -> Vec<(std::path::PathBuf, String)> {
    std::fs::read_to_string(logs.join(GSD_RUN_LOG))
        .unwrap_or_default()
        .lines()
        .map(|line| {
            let (cwd, argv) = line.split_once('\t').expect("a cwd<TAB>argv log line");
            (std::path::PathBuf::from(cwd), argv.to_string())
        })
        .collect()
}

/// GSD's `query verification status` JSON for `status` (gsd-core 1.20.1's
/// shapes, as `gsd_reader`'s own tests record them).
pub fn gsd_status_json(status: crate::gsd_reader::GsdStatus) -> &'static str {
    use crate::gsd_reader::GsdStatus;
    match status {
        GsdStatus::Passed => {
            r#"{"status":"passed","next_action":"Verification passed — continue.","next_command":"","route":""}"#
        }
        GsdStatus::GapsFound => {
            r#"{"status":"gaps_found","next_action":"Gaps found. Plan the fixes, then re-run execute-phase before shipping.","next_command":"/gsd-plan-phase 01 --gaps","route":"plan-phase"}"#
        }
        GsdStatus::HumanNeeded => {
            r#"{"status":"human_needed","next_action":"Human verification required.","next_command":"/gsd-verify-work 01","route":"verify-work"}"#
        }
        GsdStatus::Stale => {
            r#"{"status":"stale","next_action":"Verification is stale — covered source files changed after the verifier last ran.","next_command":"/gsd-execute-phase 01","route":"execute-phase","moved_files":["a.txt"]}"#
        }
        GsdStatus::Missing => {
            r#"{"status":"missing","next_action":"No verification report found — the verify step never completed.","next_command":"/gsd-execute-phase 01","route":"execute-phase"}"#
        }
    }
}

// ## Why there is no in-process absent-`git` harness in THIS crate
//
// 35-01 planned one empty-`PATH` guard per crate, so that criterion 6's tests
// could force `git` to be unresolvable and drive `phase_commit_count`'s
// could-not-measure branch from inside `devflow-core`. **That process-global
// guard was built here, measured, and removed.** It is recorded rather than
// silently omitted, because the next author to need a failing `git` will
// otherwise rebuild it.
//
// A `PATH`-replacing guard mutates process-global state, and `cargo test` runs
// this crate's whole suite as threads in ONE process. `devflow-core` shells out
// to `git` from several modules (`git`, `worktree`, `agent_result`, `monitor`,
// `gsd_config`, and this one), and — decisively — its
// tests reach `git` by calling PRODUCTION code that spawns it, not only through
// fixture helpers. So no fixture-level lock can cover them: serializing this
// module's own `git()` helper still left
// `agent_result::tests::evaluate_layer2_exit_zero_no_commits_is_failed` failing,
// because its `git` call happens inside `evaluate_layer2` itself.
//
// Measured, with a control:
//
//   - guard used by three regression tests ....... 1-5 unrelated failures/run
//   - guard used by its own sanity test only ..... 1 failure in 8 runs
//   - sanity test `#[ignore]`d (control) ......... 0 failures in 10 runs
//
// The asymmetry between the last two lines is what identifies the guard itself
// as the cause rather than a pre-existing flake. Ten clean runs is a weak bound
// on the control arm, not a proof of zero flake rate; it is enough to establish
// the direction, which is all that is being claimed.
//
// **What to use instead.** `hermetic_command` sets `cmd.current_dir(dir)`, so
// passing a path that does not exist makes the spawn itself fail and
// `.output()` return `Err` — the identical arm a missing binary produces,
// reached with no environment mutation and therefore no effect on any other
// test. `agent_result`'s `phase_commit_count_reports_none_when_git_cannot_run`
// and `evaluate_layer3_unmeasurable_count_is_unknown_not_failed` both take that
// route. It is also immune to the latent fragility of a `PATH` guard, which a
// future refactor to an absolute `git` path would disarm silently.
//
// **When that is not enough**, because the code under test must also READ a
// file from `project_root` (`evaluate_layer2` reads its exit file there, so a
// non-existent root would fail for the wrong reason), use the child-process
// pattern in `devflow-cli`: the parent builds the fixture, one re-executed
// child gets an empty `PATH`, and the parent verifies the child ran. The empty
// `PATH` is then process-local rather than something an `ENV_MUTEX` merely
// serializes. A future core-crate test needing this shape should copy that
// approach. Criterion 6's layer-level and cascade-level tests live in
// `devflow-cli/src/pipeline_outcomes.rs` for exactly this reason; they call the
// same `pub` functions, so only the binary differs.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gates::{AnswerRead, GateError, GateFile, GateId, GateResponse, Gates};
    use crate::phase_id::PhaseId;
    use crate::stage::Stage;
    use std::cell::Cell;
    use std::rc::Rc;

    const PHASE: PhaseId = PhaseId::new(7);
    const STAGE: Stage = Stage::Code;

    fn approved() -> GateResponse {
        GateResponse {
            approved: true,
            abort: false,
            note: None,
            responded_by: Some("human".into()),
        }
    }

    fn live_gate(root: &std::path::Path) -> GateId {
        let body = std::fs::read_to_string(Gates::gate_path(root, PHASE, STAGE)).unwrap();
        GateId::parse(&serde_json::from_str::<GateFile>(&body).unwrap().gate_id).unwrap()
    }

    fn close_and_open(root: &std::path::Path) {
        Gates::close(root, PHASE, STAGE).unwrap();
        Gates::open_gate(root, PHASE, STAGE, "next").unwrap();
    }

    #[test]
    fn with_answer_before_publish_runs_between_the_check_and_the_publish() {
        let dir = tempfile::tempdir().unwrap();
        let first = Gates::open_gate(dir.path(), PHASE, STAGE, "first")
            .unwrap()
            .gate_id;
        let root = dir.path().to_path_buf();
        let runs = Rc::new(Cell::new(0));
        let hook_runs = runs.clone();

        let refused = with_answer_before_publish(
            move || {
                hook_runs.set(hook_runs.get() + 1);
                close_and_open(&root);
            },
            || Gates::answer(dir.path(), PHASE, STAGE, &first, &approved()),
        );

        assert!(
            matches!(
                &refused,
                Err(GateError::GateClosedAfterPublish { observed, .. }) if observed == first.as_str()
            ),
            "{refused:?}"
        );
        assert_eq!(runs.get(), 1);
        assert_eq!(
            Gates::read_answer(dir.path(), PHASE, STAGE, &live_gate(dir.path())),
            AnswerRead::None,
            "a late answer never decides the next gate"
        );

        // The control: run just before `answer`, the same step fails its id check,
        // so the hook above ran after that check.
        let before = tempfile::tempdir().unwrap();
        let observed = Gates::open_gate(before.path(), PHASE, STAGE, "first")
            .unwrap()
            .gate_id;
        close_and_open(before.path());
        let refused = Gates::answer(before.path(), PHASE, STAGE, &observed, &approved());
        assert!(
            matches!(&refused, Err(GateError::GateIdMismatch { .. })),
            "{refused:?}"
        );
    }

    #[test]
    fn with_answer_before_publish_runs_its_hook_once_and_never_after_its_body() {
        let dir = tempfile::tempdir().unwrap();
        let gate = Gates::open_gate(dir.path(), PHASE, STAGE, "ctx")
            .unwrap()
            .gate_id;
        let runs = Rc::new(Cell::new(0));
        let counted = |runs: &Rc<Cell<i32>>| {
            let runs = runs.clone();
            move || runs.set(runs.get() + 1)
        };

        with_answer_before_publish(counted(&runs), || ());
        Gates::answer(dir.path(), PHASE, STAGE, &gate, &approved()).unwrap();
        assert_eq!(runs.get(), 0, "an answer after the body runs no hook");

        with_answer_before_publish(counted(&runs), || {
            let again = Gates::answer(dir.path(), PHASE, STAGE, &gate, &approved());
            assert!(matches!(again, Err(GateError::AlreadyResponded { .. })));
            let once_more = Gates::answer(dir.path(), PHASE, STAGE, &gate, &approved());
            assert!(matches!(once_more, Err(GateError::AlreadyResponded { .. })));
        });
        assert_eq!(runs.get(), 1, "the hook runs once");
    }

    /// `stop` publishes through `reap_answer`, so the hook runs there too (R2-04).
    #[test]
    fn with_answer_before_publish_also_runs_inside_reap_answer() {
        let dir = tempfile::tempdir().unwrap();
        let gate = Gates::open_gate(dir.path(), PHASE, STAGE, "ctx")
            .unwrap()
            .gate_id;
        let runs = Rc::new(Cell::new(0));
        let hook_runs = runs.clone();

        with_answer_before_publish(
            move || hook_runs.set(hook_runs.get() + 1),
            || Gates::reap_answer(dir.path(), PHASE, STAGE, &gate, "abort", "stop").unwrap(),
        );

        assert_eq!(runs.get(), 1);
    }

    #[test]
    fn gate_answer_files_lists_keyed_and_legacy_answers() {
        let dir = tempfile::tempdir().unwrap();
        let gate = Gates::open_gate(dir.path(), PHASE, STAGE, "ctx")
            .unwrap()
            .gate_id;
        let keyed = Gates::answer(dir.path(), PHASE, STAGE, &gate, &approved()).unwrap();
        let legacy = Gates::dir(dir.path()).join("07-code.response.json");
        std::fs::write(&legacy, "{}").unwrap();
        Gates::claim(dir.path(), PHASE, STAGE, &gate).unwrap();
        let _registration = Gates::register_waiter(dir.path(), PHASE, STAGE, &gate).unwrap();
        let gates_dir = Gates::dir(dir.path());
        std::fs::write(gates_dir.join("07-code.noid.1.2.0.quarantined.json"), "{}").unwrap();
        std::fs::write(gates_dir.join("07-validate.response.json"), "{}").unwrap();

        let mut expected = vec![keyed, legacy];
        expected.sort();
        assert_eq!(gate_answer_files(dir.path(), PHASE, STAGE), expected);
    }

    #[test]
    fn answer_open_gate_answers_the_live_gate_id() {
        let dir = tempfile::tempdir().unwrap();
        let refused = answer_open_gate(dir.path(), PHASE, STAGE, &approved());
        assert!(
            matches!(&refused, Err(GateError::NoOpenGate { .. })),
            "{refused:?}"
        );

        Gates::write_gate(dir.path(), PHASE, STAGE, "ctx").unwrap();
        let live = live_gate(dir.path());
        let path = answer_open_gate(dir.path(), PHASE, STAGE, &approved()).unwrap();

        assert_eq!(path, Gates::answer_path(dir.path(), PHASE, STAGE, &live));
        assert!(matches!(
            Gates::read_answer(dir.path(), PHASE, STAGE, &live),
            AnswerRead::Bound(answer) if answer.response == approved()
        ));
    }

    #[test]
    fn child_with_an_empty_path_dir_cannot_spawn_git_while_the_parent_can() {
        const NAME: &str = "test_support::tests::child_with_an_empty_path_dir_cannot_spawn_git_while_the_parent_can";

        if in_child_test(NAME) {
            let err = std::process::Command::new("git")
                .arg("--version")
                .output()
                .expect_err("an empty child PATH must not resolve git");
            assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
            return;
        }

        std::process::Command::new("git")
            .arg("--version")
            .output()
            .expect("the parent PATH must resolve git for this control");
        let empty_dir = tempfile::tempdir().expect("create empty PATH directory");
        let output = run_test_in_child(NAME, empty_dir.path(), &[]);
        assert_child_ran_exactly_one_passing_test(&output, NAME);
    }

    #[test]
    fn child_guard_accepts_output_emitted_by_the_passing_child() {
        const NAME: &str =
            "test_support::tests::child_guard_accepts_output_emitted_by_the_passing_child";

        if in_child_test(NAME) {
            println!("child emits output before libtest writes the final status");
            return;
        }

        let empty_dir = tempfile::tempdir().expect("create empty PATH directory");
        let output = run_test_in_child(NAME, empty_dir.path(), &[]);
        assert_child_ran_exactly_one_passing_test(&output, NAME);
    }

    #[test]
    #[should_panic(expected = "child must report exactly `1 passed`")]
    fn child_guard_rejects_a_name_that_matches_no_test() {
        let empty_dir = tempfile::tempdir().expect("create empty PATH directory");
        let output = run_test_in_child(
            "test_support::tests::this_module_qualified_test_does_not_exist",
            empty_dir.path(),
            &[],
        );
        assert_child_ran_exactly_one_passing_test(
            &output,
            "test_support::tests::this_module_qualified_test_does_not_exist",
        );
    }

    #[test]
    #[should_panic(expected = "child test PATH must be an existing directory")]
    fn run_test_in_child_refuses_a_path_that_is_not_a_directory() {
        let missing = std::path::Path::new("this-path-must-not-exist-for-child-test");
        run_test_in_child(
            "test_support::tests::run_test_in_child_refuses_a_path_that_is_not_a_directory",
            missing,
            &[],
        );
    }

    /// Positive case (25-11/999.47, the whole point of the barrier): a real
    /// child whose argv[0] basename is known and differs from this test
    /// binary's own must be reported exec-visible, and after the function
    /// returns, `/proc/<pid>/cmdline` must genuinely hold the child's own
    /// argv — never the caller's.
    #[test]
    fn wait_for_exec_visibility_detects_a_real_child_and_leaves_it_exec_visible() {
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("spawn sleep fixture");
        let pid = child.id();

        let visible = wait_for_exec_visibility(
            pid,
            "sleep",
            std::time::Duration::from_secs(5),
            std::time::Duration::from_millis(2),
        );
        assert!(visible, "a real sleep child must become exec-visible");

        let raw = std::fs::read(format!("/proc/{pid}/cmdline"))
            .expect("must be able to read the child's cmdline after the barrier returns");
        let args: Vec<String> = raw
            .split(|&b| b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect();
        assert_eq!(
            args.first().map(String::as_str),
            Some("sleep"),
            "after the barrier returns, cmdline must be the child's own argv, not the caller's"
        );

        let _ = child.kill();
        let _ = child.wait();
    }

    /// Timeout is a value, not a hang (25-11/999.47): calling against this
    /// process's OWN pid with an `expected_argv0_basename` that can never
    /// match must return `false` within roughly `wait`, not indefinitely.
    #[test]
    fn wait_for_exec_visibility_times_out_bounded_when_it_never_matches() {
        let start = std::time::Instant::now();
        let visible = wait_for_exec_visibility(
            std::process::id(),
            "this-basename-can-never-match-anything",
            std::time::Duration::from_millis(200),
            std::time::Duration::from_millis(2),
        );
        let elapsed = start.elapsed();

        assert!(
            !visible,
            "a basename that can never match must return false"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "must return within roughly `wait`, not hang indefinitely — took {elapsed:?}"
        );
    }

    /// Dead pid (25-11/999.47): a pid that is not alive must return `false`
    /// promptly, never panicking and never blocking for the full `wait`
    /// ceiling — the caller cannot distinguish "will never be alive" from
    /// "still forking" without this short-circuit.
    #[test]
    fn wait_for_exec_visibility_returns_false_promptly_for_a_dead_pid() {
        let start = std::time::Instant::now();
        let visible = wait_for_exec_visibility(
            0x7FFF_FFFE,
            "anything",
            std::time::Duration::from_secs(5),
            std::time::Duration::from_millis(20),
        );
        let elapsed = start.elapsed();

        assert!(!visible, "a dead pid must never be reported exec-visible");
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "a dead pid must not wait out the full ceiling, took {elapsed:?}"
        );
    }

    /// Self-argv guard (25-11/999.47): the function must not report success
    /// merely because argv[0]'s basename matches while the observed cmdline
    /// is still byte-identical to the caller's own `/proc/self/cmdline`.
    /// Calling it against the caller's own pid with the caller's own argv[0]
    /// basename matches condition (i) by construction — the result must
    /// still be `false`, because condition (ii) never holds for the caller's
    /// own unchanging cmdline.
    #[test]
    fn wait_for_exec_visibility_rejects_a_self_match_on_unchanged_cmdline() {
        let self_pid = std::process::id();
        let raw = std::fs::read(format!("/proc/{self_pid}/cmdline"))
            .expect("must be able to read this process's own cmdline");
        let self_basename = raw
            .split(|&b| b == 0)
            .find(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .and_then(|arg0| {
                std::path::Path::new(&arg0)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
            })
            .expect("must be able to derive this process's own argv[0] basename");

        let visible = wait_for_exec_visibility(
            self_pid,
            &self_basename,
            std::time::Duration::from_millis(200),
            std::time::Duration::from_millis(2),
        );

        assert!(
            !visible,
            "a self-match on unchanged cmdline must never report exec-visible, even though \
             argv[0]'s basename matches by construction"
        );
    }

    /// An unreaped zombie is not a live group member, though `kill(-pgid, 0)`
    /// still finds its group: the case a non-reaping PID 1 leaves behind.
    /// Control: the same process counts while it runs.
    #[test]
    fn a_zombie_is_not_a_live_group_member() {
        use std::os::unix::process::CommandExt;
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();
        assert_eq!(live_group_members(pid), vec![pid], "control: it runs");
        assert!(process_is_live(pid), "control: it runs");

        child.kill().expect("SIGKILL the sleeper");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while process_is_live(pid) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "it is a zombie, not gone: nothing has reaped it"
        );
        let signed = libc::pid_t::try_from(pid).expect("pid fits pid_t");
        // SAFETY: signal 0 only checks for existence; `signed` is our child's
        // own group, never 0, 1 or ours.
        assert_eq!(
            unsafe { libc::kill(-signed, 0) },
            0,
            "kill(-pgid, 0) still reports the zombie's group"
        );
        assert!(
            live_group_members(pid).is_empty(),
            "a zombie is not a member"
        );
        assert!(!process_is_live(pid), "a zombie is not live");
        child.wait().expect("reap the sleeper");
    }
}
