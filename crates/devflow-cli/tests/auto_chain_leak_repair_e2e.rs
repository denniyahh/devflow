//! The kill half of ROADMAP criterion 2 (35.1-02, D-01/D-02/D-03): a real
//! `SIGKILL` is shown to leave GSD's `workflow._auto_chain_active` set, and the
//! next real `devflow resume` is shown to clear it and to say so on both
//! channels.
//!
//! **Deliberately a separate file from `auto_chain_flag_e2e.rs` (D-02).** That
//! suite owns the in-stage-error path — `guard_clears_the_flag_when_the_
//! supervised_child_fails`, driven through `run_monitor`'s `?` early-return.
//! This one owns the kill path. Two mechanisms, two separately-named tests, so
//! a regression in either is attributable rather than masked by the other.
//! Neither is a combined test.
//!
//! **The load-bearing assertion here is the one that says the leak PERSISTS**
//! after the kill. It is the negative control for the whole plan: a flag that
//! was never set is also a flag that reads clear, so without proving the leak
//! is real, every repair assertion downstream of it is vacuous. If that
//! assertion ever starts failing, this test is lying about what it measures.
//!
//! Integration test binaries cannot import each other, so the repo/config
//! scaffolding below is copied from `auto_chain_flag_e2e.rs` rather than
//! shared. Keep the two in step.

use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::workflow::save_state;
use std::cell::RefCell;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::time::{Duration, Instant};

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

/// Hermetic git invocation pinned to `root` (999.37) — never a bare
/// `Command::new("git")`.
fn git(root: &Path, args: &[&str]) {
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_output(root: &Path, args: &[&str]) -> String {
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Init a temp repo with `develop` and a `feature/phase-NN` branch holding one
/// commit ahead of `develop`.
fn init_repo(root: &Path, phase: PhaseId) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-q", "-m", "base"]);

    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    git(root, &["checkout", "-q", "-b", &branch]);
    fs::write(root.join("work.txt"), "agent work\n").unwrap();
    git(root, &["add", "work.txt"]);
    git(root, &["commit", "-q", "-m", "agent work"]);
}

/// A `.planning/config.json` mirroring this project's real file shape, so a
/// write that re-renders or drops anything shows up here rather than only in
/// production.
fn real_shape_config(active: bool) -> String {
    format!(
        r#"{{
  "commit_docs": true,
  "workflow": {{
    "granularity": "medium",
    "auto_mode": true,
    "auto_advance": true,
    "commit_docs": true,
    "subagent_timeout": 300000,
    "_auto_chain_active": {active},
    "nyquist_validation": true,
    "tdd_mode": true
  }},
  "git": {{
    "main": "main",
    "develop": "develop",
    "feature_prefix": "feature/"
  }},
  "intel": {{
    "enabled": true
  }},
  "review": {{
    "default_reviewers": [
      "codex"
    ]
  }},
  "model_overrides": {{
    "gsd-executor": "inherit"
  }},
  "mempalace": {{
    "enabled": true
  }}
}}
"#
    )
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

/// The supervised child: record its own pid, then become `sleep` so it holds
/// the monitor open long enough for this test to kill the monitor out from
/// under it.
///
/// `exec` rather than a plain `sleep` call is what keeps the recorded pid
/// meaningful: without it, `$$` names the shell and `sleep` is a grandchild
/// that survives a kill aimed at the pid file. With it there is exactly one
/// process to account for.
///
/// The 30s ceiling is a backstop, not the mechanism — the teardown below kills
/// this process explicitly. It exists so a test that dies before its teardown
/// still cannot leave something sleeping indefinitely.
fn sleeper_script(pid_path: &Path) -> String {
    format!(
        "#!/bin/sh\nprintf '%s' \"$$\" > '{pid}'\nexec sleep 30\n",
        pid = pid_path.display()
    )
}

/// Kills every registered pid on the way out, whether the test passed, failed
/// an assertion, or panicked partway.
///
/// This project already carries `reap_strays_e2e.rs` because leaked processes
/// from tests are a real failure class here; a sleeping child left behind by a
/// failed assertion would wedge the whole suite.
#[derive(Default)]
struct Reaper {
    pids: RefCell<Vec<u32>>,
}

impl Reaper {
    fn watch(&self, pid: u32) {
        self.pids.borrow_mut().push(pid);
    }
}

impl Drop for Reaper {
    fn drop(&mut self) {
        for pid in self.pids.borrow().iter().copied() {
            if devflow_core::agent::agent_running(pid) {
                devflow_core::agent::terminate_and_verify(
                    pid,
                    Duration::from_secs(2),
                    Duration::from_millis(20),
                );
            }
        }
    }
}

/// Poll `predicate` until true, panicking with a diagnostic naming what was
/// being waited for if `ceiling` elapses first. A stated ceiling and a named
/// subject, never an unbounded loop.
fn wait_for(mut predicate: impl FnMut() -> bool, ceiling: Duration, what: &str) {
    let start = Instant::now();
    while !predicate() {
        assert!(
            start.elapsed() < ceiling,
            "timed out after {ceiling:?} waiting for: {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

struct Fixture {
    _repo: tempfile::TempDir,
    _aux: tempfile::TempDir,
    root: PathBuf,
    phase: PhaseId,
    config: PathBuf,
    prompt_file: PathBuf,
    script: PathBuf,
    child_pid_file: PathBuf,
    reaper: Reaper,
}

impl Fixture {
    fn new(flag_before: bool) -> Self {
        let repo = tempfile::tempdir().unwrap();
        let aux = tempfile::tempdir().unwrap();
        // The binary canonicalizes `--project`, so the fixture must too or the
        // state file it writes is looked up under a different path.
        let root = repo.path().canonicalize().unwrap();
        let phase = PhaseId::new(78);
        init_repo(&root, phase);

        let planning = root.join(".planning");
        fs::create_dir_all(&planning).unwrap();
        let config = planning.join("config.json");
        fs::write(&config, real_shape_config(flag_before)).unwrap();

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.clone());
        state.stage = Stage::Code;
        state.stop_until = Some(Stage::Code);
        state.stopped = false;
        save_state(&state).unwrap();

        let script = aux.path().join("agent.sh");
        let child_pid_file = aux.path().join("child.pid");
        write_executable(&script, &sleeper_script(&child_pid_file));

        let prompt_file = aux.path().join("prompt.txt");
        fs::write(&prompt_file, "run the code stage\n").unwrap();

        Self {
            _repo: repo,
            _aux: aux,
            root,
            phase,
            config,
            prompt_file,
            script,
            child_pid_file,
            reaper: Reaper::default(),
        }
    }

    /// Spawn the REAL binary's hidden `__monitor` subcommand supervising the
    /// sleeper, as a child of this test process so it can be signalled
    /// directly.
    fn spawn_monitor(&self) -> Child {
        let child = Command::new(devflow_bin())
            .arg("__monitor")
            .arg("--project")
            .arg(&self.root)
            .arg("--phase")
            .arg(self.phase.to_string())
            .arg("--workdir")
            .arg(&self.root)
            .arg("--prompt-file")
            .arg(&self.prompt_file)
            .arg("--idle-timeout-secs")
            // Comfortably above the sleeper's own 30s ceiling, so the idle
            // timeout can never be what ends this run — the kill below is.
            .arg("120")
            .arg("--")
            .arg("sh")
            .arg(&self.script)
            .spawn()
            .expect("spawn devflow __monitor");
        self.reaper.watch(child.id());
        child
    }

    /// Run the REAL `devflow resume` for this phase.
    ///
    /// `PATH` is set to `git_only_path` for the CHILD ONLY — never mutated in
    /// this process. `cargo test` runs a binary's tests as threads in one
    /// process, so a process-global `PATH` change would leak across every other
    /// test in this binary (the hazard `devflow-core`'s `test_support` module
    /// documents at length); scoping it to the spawned command avoids that
    /// entirely.
    ///
    /// **The absent `claude` binary is deliberate, and it is what makes the
    /// repair assertions unambiguous.** `launch_stage` calls
    /// `ensure_agent_binary` before anything else, so `resume` performs the
    /// repair, prints, emits, and then refuses to launch — meaning NOTHING is
    /// spawned that could re-set the flag between the repair and this test's
    /// read of it. The plan proposed instead rewriting the persisted stage to
    /// one the eligibility predicate does not cover; this achieves the same
    /// "cannot mask the repair" property structurally rather than contingently,
    /// and it avoids leaving a detached monitor running that the plan's own "no
    /// process outlives the test binary" criterion forbids. See the SUMMARY for
    /// what this does NOT establish.
    fn run_resume(&self, git_only_path: &Path) -> Output {
        Command::new(devflow_bin())
            .arg("resume")
            .arg("--phase")
            .arg(self.phase.to_string())
            .arg(&self.root)
            .env("PATH", git_only_path)
            .output()
            .expect("spawn devflow resume")
    }

    /// The flag as the WORKING TREE holds it.
    fn flag_now(&self) -> String {
        let raw = fs::read_to_string(&self.config).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        // Indexing, not `.get()`: an absent key must raise here rather than
        // quietly render as `false` and agree with a correct cleared result.
        value["workflow"]["_auto_chain_active"].to_string()
    }

    /// The flag as the BRANCH TIP holds it — out of git, never out of the
    /// working tree. A working-tree read cannot distinguish "committed the fix"
    /// from "wrote the fix and forgot to commit".
    fn flag_at_head(&self) -> String {
        let raw = git_output(&self.root, &["show", "HEAD:.planning/config.json"]);
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        value["workflow"]["_auto_chain_active"].to_string()
    }

    fn repair_events(&self) -> Vec<serde_json::Value> {
        let path = devflow_core::events::events_path(&self.root);
        let Ok(raw) = fs::read_to_string(path) else {
            return Vec::new();
        };
        raw.lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|line| line["event"] == "auto_chain_flag_repaired")
            .collect()
    }

    fn child_pid(&self) -> Option<u32> {
        fs::read_to_string(&self.child_pid_file)
            .ok()
            .and_then(|raw| raw.trim().parse().ok())
    }
}

/// A `PATH` holding `git` and nothing else, so a spawned `devflow` can run its
/// git probes but cannot find an agent binary. Built by symlinking whatever
/// `git` this host resolves rather than hardcoding a directory, which would
/// break on a host that keeps git somewhere else — and which could
/// accidentally expose a real `claude` living in the same directory.
fn git_only_path() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let located = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .expect("locate git");
    assert!(
        located.status.success(),
        "this test needs a `git` on PATH to build its git-only PATH"
    );
    let git_path = String::from_utf8_lossy(&located.stdout).trim().to_string();
    std::os::unix::fs::symlink(&git_path, dir.path().join("git")).unwrap();
    assert!(
        !dir.path().join("claude").exists(),
        "the git-only PATH must not contain an agent binary — that absence is \
         what keeps the repair assertions unambiguous"
    );
    dir
}

/// D-02's kill case. `Drop` covers a normal return, a `?` early-return and a
/// panic-unwind, and structurally cannot cover a `SIGKILL`. This proves the
/// leak that follows is real, and then that the next real `devflow resume`
/// repairs it and says so on both channels.
#[test]
fn auto_chain_flag_survives_sigkill_and_is_repaired_on_the_next_start() {
    let fixture = Fixture::new(false);
    let path_dir = git_only_path();

    let mut monitor = fixture.spawn_monitor();

    // The fixture must actually engage the guard before anything downstream
    // means anything. A timeout here is "the fixture never set the flag", which
    // is a different failure from "the repair did not work" and must not be
    // confusable with it.
    wait_for(
        || fixture.flag_now() == "true",
        Duration::from_secs(30),
        "the supervised Code-stage launch to set workflow._auto_chain_active — \
         without this the fixture never engaged the guard and nothing below can \
         be concluded",
    );
    let child_pid = fixture.child_pid();

    // A real SIGKILL: `Child::kill` sends SIGKILL on Unix. The monitor dies
    // with its guard still engaged and no opportunity to run `Drop`.
    monitor.kill().expect("SIGKILL the monitor");
    monitor.wait().expect("reap the killed monitor");

    // ==================== NEGATIVE CONTROL ====================
    // THE load-bearing assertion of this plan. It proves the leak is real and
    // that `Drop` genuinely cannot cover a kill. A flag that was never set is
    // also a flag that reads clear, so if this assertion ever starts failing,
    // every repair assertion below became vacuous and this test is lying about
    // what it measures — fix the fixture, do not delete the assertion.
    assert_eq!(
        fixture.flag_now(),
        "true",
        "a SIGKILLed monitor must LEAVE the chain flag set — the in-process \
         guard's Drop cannot run on a kill, which is the entire reason the \
         force-clear repair exists (35.1 D-01)"
    );
    // ==========================================================

    // The orphaned sleeper is no longer supervised by anything; clear it before
    // the assertions below, and let the reaper cover the case where one of them
    // panics first.
    if let Some(pid) = child_pid {
        devflow_core::agent::terminate_and_verify(
            pid,
            Duration::from_secs(2),
            Duration::from_millis(20),
        );
    }

    let output = fixture.run_resume(path_dir.path());
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

    assert_eq!(
        fixture.flag_now(),
        "false",
        "the next devflow resume must clear the leaked value before any agent \
         is launched\nresume stdout:\n{stdout}\nresume stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // D-03, channel 1: stdout. The notice must say what a stale flag MEANS —
    // that a previous run for this phase was killed — not merely that a value
    // changed.
    assert!(
        stdout.contains("was killed before it could clear it"),
        "the repair must announce itself on stdout, saying what the stale flag \
         means\nstdout:\n{stdout}"
    );

    // D-03, channel 2: the event log, naming which entry point found it.
    let events = fixture.repair_events();
    assert_eq!(
        events.len(),
        1,
        "exactly one auto_chain_flag_repaired event: {events:?}"
    );
    assert_eq!(events[0]["entry_point"], "resume");
    assert_eq!(events[0]["working_tree_repaired"], true);

    // No process may outlive this test.
    if let Some(pid) = child_pid {
        assert!(
            !devflow_core::agent::agent_running(pid),
            "the supervised child (pid {pid}) must not outlive this test"
        );
    }
}

/// Criterion 2's committed half, at the binary level rather than the unit
/// level: a leak that reached the branch tip is repaired IN THE COMMIT too, so
/// the value Ship would merge into `develop` is the cleared one.
///
/// Kept separate from the SIGKILL test rather than folded into it, so a failure
/// names which half broke.
#[test]
fn a_leak_that_reached_the_branch_tip_is_repaired_in_the_commit_too() {
    let fixture = Fixture::new(true);
    let path_dir = git_only_path();

    // The leak reaches the branch tip — the state a `commit_docs` run or a
    // sweeping `git add` produces after a killed monitor.
    git(&fixture.root, &["add", ".planning/config.json"]);
    git(
        &fixture.root,
        &[
            "commit",
            "-q",
            "-m",
            "docs: commit carrying the leaked flag",
        ],
    );
    assert_eq!(
        fixture.flag_at_head(),
        "true",
        "the fixture must actually commit the leak, or the assertion below is \
         vacuous"
    );
    let head_before = git_output(&fixture.root, &["rev-parse", "HEAD"]);

    let output = fixture.run_resume(path_dir.path());
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

    assert_eq!(
        fixture.flag_at_head(),
        "false",
        "a leak that reached the branch tip must be repaired in the commit too \
         — a working-tree-only repair leaves the branch → merge → develop → \
         next-phase-chains path open\nresume stdout:\n{stdout}"
    );
    assert_eq!(
        fixture.flag_now(),
        "false",
        "and the working tree is cleared as well"
    );
    assert_ne!(
        git_output(&fixture.root, &["rev-parse", "HEAD"]),
        head_before,
        "the branch-tip repair is a commit, not a working-tree write"
    );

    let events = fixture.repair_events();
    assert_eq!(events.len(), 1, "exactly one repair event: {events:?}");
    assert_eq!(events[0]["committed_tree_repaired"], true);
    assert_eq!(events[0]["entry_point"], "resume");
}
