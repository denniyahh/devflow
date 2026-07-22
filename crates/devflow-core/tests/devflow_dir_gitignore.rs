//! Regression coverage for 19a-WR-01/WR-03 (closed by `workflow::ensure_devflow_dir`,
//! 19-01-PLAN.md): every production path that creates a `.devflow/` directory
//! must leave a self-ignoring `.devflow/.gitignore` (`*`) behind, and a
//! downstream user's routine `git add . && git commit` must never sweep
//! `.devflow/` into their repository's history.
//!
//! **Masking interaction with `crates/devflow-cli/tests/gitignore_coverage.rs`
//! (T-19-06, accepted-low per 19-01-PLAN.md's threat register):** once
//! `.devflow/.gitignore` exists on disk in *this* repo (which it now does,
//! because DevFlow runs against its own checkout during development and
//! dogfooding), that guard's `git check-ignore` assertions can be satisfied by
//! the new file rather than by the root `.gitignore`, so it no longer
//! independently proves root-`.gitignore` coverage on its own. This is a
//! deliberate, recorded trade — `devflow-core/src/doc_check.rs`'s
//! `gitignore_covers_all_devflow_paths` remains strict because it parses the
//! root `.gitignore` text directly rather than asking git to resolve ignore
//! rules, so a root-`.gitignore` regression is still caught. No change to
//! either existing guard is made here.

use devflow_core::agent_result;
use devflow_core::events;
use devflow_core::gates::Gates;
use devflow_core::lock;
use devflow_core::monitor;
use devflow_core::ship;
use devflow_core::workflow;
use devflow_core::{AgentKind, Mode, Stage, State};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// `true` iff `root/.devflow/.gitignore` exists with trimmed content `*`.
fn gitignore_is_star(root: &Path) -> bool {
    std::fs::read_to_string(root.join(".devflow").join(".gitignore"))
        .map(|contents| contents.trim() == "*")
        .unwrap_or(false)
}

/// Poll `path` until it exists (or `timeout` elapses, at which point this
/// panics naming the path — a monitor that never writes its exit-code
/// capture file is itself a bug, not a thing to silently tolerate).
fn wait_for_file(path: &Path, timeout: Duration) {
    let start = Instant::now();
    while !path.exists() {
        assert!(
            start.elapsed() < timeout,
            "expected {} to appear within {timeout:?}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Reap `pid`, then poll until it is no longer a live process (or `timeout`
/// elapses).
///
/// Codex review: `spawn_monitor_no_advance` returns only a detached child's
/// `u32` pid, not a `Child` handle the test could `.wait()` on. But this test
/// binary is still the OS-level parent of that process (it called
/// `Command::spawn()` internally), so it never gets reparented to init —
/// without an explicit `waitpid`, the exited monitor shell would sit as a
/// zombie (which still answers `kill(pid, 0)` successfully) for the rest of
/// the test binary's lifetime.
fn wait_for_pid_to_die(pid: u32, timeout: Duration) {
    unsafe {
        let mut status: libc::c_int = 0;
        libc::waitpid(pid as libc::pid_t, &mut status, 0);
    }
    let start = Instant::now();
    while devflow_core::agent::agent_running(pid) {
        assert!(
            start.elapsed() < timeout,
            "monitor process pid {pid} did not terminate within {timeout:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Every one of the 7 production `.devflow/`-creating entry points, driven
/// through its real public API (not `ensure_devflow_dir` directly — that
/// would prove nothing about whether the site was actually converted),
/// against its own fresh temp project root. Failures accumulate into a
/// vector so one regressed site doesn't hide another (mirrors the
/// accumulate-then-assert shape `gitignore_coverage.rs` already uses).
#[test]
fn all_seven_devflow_constructors_produce_the_gitignore() {
    let mut failures: Vec<&str> = Vec::new();

    // 1. workflow::save_state -> workflow.rs write_state_atomic
    {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let state = State::new(1, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        workflow::save_state(&state).expect("save_state");
        if !gitignore_is_star(root) {
            failures.push("workflow::save_state (workflow.rs)");
        }
    }

    // 2. gates::Gates::write_gate -> gates.rs write_atomic
    {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        Gates::write_gate(root, 1, Stage::Validate, "context").expect("write_gate");
        if !gitignore_is_star(root) {
            failures.push("gates::Gates::write_gate (gates.rs)");
        }
    }

    // 3. monitor::spawn_monitor_no_advance -> monitor.rs capture dir
    //    (the sequentagent/parallel path — never calls save_state, D-14).
    {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let state = State::new(1, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        let pid = monitor::spawn_monitor_no_advance(
            &state,
            "sh",
            &["-c".to_string(), "exit 0".to_string()],
            &[],
        )
        .expect("spawn_monitor_no_advance");

        wait_for_file(
            &agent_result::exit_code_path(root, 1),
            Duration::from_secs(5),
        );
        wait_for_pid_to_die(pid, Duration::from_secs(5));

        if !gitignore_is_star(root) {
            failures.push("monitor::spawn_monitor_no_advance (monitor.rs)");
        }
    }

    // 4. agent_result::archive_phase_files -> agent_result.rs history_dir
    //    (the sequentagent/parallel path — also never calls save_state).
    {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::stdout_path(root, 1), "stdout capture\n").unwrap();
        archive_phase_files_or_panic(root, 1);
        if !gitignore_is_star(root) {
            failures.push("agent_result::archive_phase_files (agent_result.rs)");
        }
    }

    // 5. events::emit -> events.rs fail-soft let-chain
    {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        events::emit(root, 1, "test_event", serde_json::json!({}));
        if !gitignore_is_star(root) {
            failures.push("events::emit (events.rs)");
        }
    }

    // 6. ship::write_cron_instructions -> ship.rs write_cron_instructions
    {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let instructions = ship::build_cron_instructions(root, 1, "2026-01-01T00:00:00Z", "claude");
        ship::write_cron_instructions(root, &instructions).expect("write_cron_instructions");
        if !gitignore_is_star(root) {
            failures.push("ship::write_cron_instructions (ship.rs)");
        }
    }

    // 7. lock::acquire -> lock.rs acquire_path
    {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let _guard = lock::acquire(root, 1).expect("lock::acquire");
        if !gitignore_is_star(root) {
            failures.push("lock::acquire (lock.rs)");
        }
    }

    assert!(
        failures.is_empty(),
        "the following .devflow/ constructor(s) did not produce a .devflow/.gitignore: {failures:?}"
    );
}

/// Small helper so a failing `archive_phase_files` reports through the same
/// accumulate-then-assert shape as the other 6 sites, instead of panicking
/// mid-test via `.expect()` and hiding which of the 7 sites regressed.
fn archive_phase_files_or_panic(root: &Path, phase: u32) {
    agent_result::archive_phase_files(root, root, phase, 5).expect("archive_phase_files");
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A scratch git repo whose root `.gitignore` mentions nothing about
/// `.devflow` — reproducing a downstream user's project that DevFlow does
/// not control.
fn init_scratch_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    // Disable any globally-configured hooks (e.g. gitleaks) for isolation,
    // matching the idiom in git.rs's own test helpers.
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    std::fs::write(root.join(".gitignore"), "*.log\n").unwrap();
    std::fs::write(root.join("README.md"), "# scratch\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "initial commit"]);
    dir
}

/// The `17-REVIEW.md` reproduction this plan closes: a downstream repo whose
/// root `.gitignore` has no `.devflow` pattern at all must still commit zero
/// `.devflow/` paths after a routine `git add . && git commit`, because
/// `ensure_devflow_dir` writes its own self-ignoring `.gitignore` inside
/// `.devflow/` itself.
#[test]
fn git_add_all_no_longer_sweeps_devflow_into_a_commit() {
    let repo = init_scratch_repo();
    let root = repo.path();

    let devflow_dir = root.join(".devflow");
    workflow::ensure_devflow_dir(&devflow_dir).expect("ensure_devflow_dir");
    std::fs::write(
        devflow_dir.join("phase-01-stdout"),
        "SENTINEL: unredacted agent stdout would go here\n",
    )
    .unwrap();
    // A real, non-.devflow change so the commit has something legitimate to
    // record — proving .devflow/ is excluded, not that the commit is a no-op.
    std::fs::write(root.join("README.md"), "# scratch\n\nupdated.\n").unwrap();

    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "routine commit"]);

    let committed = Command::new("git")
        .args(["log", "-1", "--name-only", "--pretty=format:"])
        .current_dir(root)
        .output()
        .expect("git log");
    let committed_files = String::from_utf8_lossy(&committed.stdout);
    assert!(
        !committed_files.contains(".devflow/"),
        "commit swept .devflow/ paths into history: {committed_files}"
    );

    // Empty-input edge: a .devflow/ containing ONLY the .gitignore (no other
    // files yet) must likewise stage and commit nothing.
    let repo2 = init_scratch_repo();
    let root2 = repo2.path();
    workflow::ensure_devflow_dir(&root2.join(".devflow")).expect("ensure_devflow_dir");

    git(root2, &["add", "."]);
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root2)
        .output()
        .expect("git status");
    assert!(
        String::from_utf8_lossy(&status.stdout).trim().is_empty(),
        "a .devflow/ containing only .gitignore must stage nothing"
    );
}
