//! End-to-end proof for the opt-in stray-reaping path (999.44/DEN-68,
//! Phase 25 plan 07): a process whose project root has been deleted out
//! from under it is unreachable by every registry/lock/state-file path —
//! `devflow stop --phase N --root PATH` cannot even resolve `PATH` once it
//! no longer exists on disk — yet the registry-independent primitives
//! [`devflow_core::agent::discover_stray_devflow_processes`],
//! [`devflow_core::agent::is_same_process`] and
//! [`devflow_core::agent::terminate_and_verify`] still find and clear it.
//!
//! **A note on what this file deliberately does NOT do.**
//! `commands::gate_sweep`'s opt-in `--reap-strays` flag is, by design,
//! registry-independent: when invoked for real (not `--dry-run`) it scans
//! the WHOLE machine's `/proc` for anything structurally shaped like a
//! DevFlow monitor wrapper or `advance` child, owned by the caller's uid,
//! and reaps every match — that is the entire point (999.44's own lesson:
//! a stray has no root to scope a search to). This project's own
//! `<project_test_traps>` and this plan's `<known_red_baseline>` both flag
//! that a reaping test must never act on anything it did not spawn itself,
//! and this specific development machine legitimately runs OTHER, live,
//! concurrent DevFlow processes (sibling `gsd-execute-phase` worktree
//! agents driving their own phases) whose monitor wrappers and `advance`
//! children match those same two structural shapes. Invoking the real
//! (non-dry-run) `devflow gate sweep --reap-strays` from an automated test
//! on such a machine would signal those unrelated, legitimate processes
//! too — there is no CLI flag that scopes it down, by design (adding one
//! would defeat the "a stray has no root" premise this feature exists to
//! serve).
//!
//! So instead of driving the full CLI end to end for the destructive case,
//! this file proves the exact claim the acceptance criteria make —
//! "discovery still finds it, and the reaping path clears it" — directly
//! against the same public primitives `commands::gate_sweep`'s
//! `reap_stray_candidates` composes (see `commands.rs`'s own
//! `#[cfg(test)]` unit tests for CLI-level flag-wiring coverage, all
//! either fully synthetic or `--dry-run`-scoped, which never signals
//! anything real). Every reap performed here acts ONLY on this file's own
//! spawned fixture pid, filtered explicitly out of whatever else discovery
//! returns — never on an unfiltered census.
//!
//! `commands::gate_sweep` is `pub(crate)` inside the `devflow` BINARY crate
//! (no `lib.rs`, so integration tests cannot link against it at all
//! either), which is the same constraint `stop_e2e.rs` and
//! `gate_sweep_e2e.rs` document — the one CLI-level assertion this file
//! does make (`devflow stop` reporting the deleted root as unreachable) is
//! driven via the real compiled binary, exactly like those files.

use devflow_core::agent;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

/// Poll `predicate` until it's true, panicking with `what` if `timeout_secs`
/// elapses first — mirrors `stop_e2e.rs`'s `wait_for`.
fn wait_for(mut predicate: impl FnMut() -> bool, timeout_secs: u64, what: &str) {
    let start = Instant::now();
    while !predicate() {
        assert!(
            start.elapsed() < Duration::from_secs(timeout_secs),
            "timed out after {timeout_secs}s waiting for: {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A nested tempdir: an OUTER guard that outlives an INNER directory the
/// test can delete out from under a live child while the outer tempdir
/// (and the test process itself) survive — the shape
/// `staleness.rs::worktree_staleness_fixture` establishes for the same
/// reason (999.44's read_first pointer). Returns `(outer_guard,
/// inner_root_path)`; the guard must be kept alive for the duration of the
/// test even after `inner_root` itself has been removed.
fn nested_root_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let outer = tempfile::tempdir().unwrap();
    let inner_root = outer.path().join("project-root");
    std::fs::create_dir_all(&inner_root).unwrap();
    (outer, inner_root)
}

/// Spawn a real child shaped exactly like the monitor wrapper
/// `discover_stray_devflow_processes` matches (Layer 1: `sh -c <script
/// containing MONITOR_WRAPPER_MARKER>`), rooted at `cwd` — 999.44's
/// reproduction shape. `sleep 30` is generous relative to this test's own
/// bounded waits; teardown always kills it explicitly rather than waiting
/// it out.
///
/// The returned child is exec-visible (999.47/25-11): this helper crosses
/// `wait_for_exec_visibility` before returning, so both callers inherit the
/// barrier without a per-caller change — the fixture is guaranteed to have
/// completed its own `execve()` by the time this function returns, closing
/// the fork/exec window before either caller reads a `/proc`-cmdline census
/// about it.
fn spawn_monitor_wrapper_fixture(cwd: &Path) -> std::process::Child {
    let child = std::process::Command::new("sh")
        .arg("-c")
        .arg("trap cleanup TERM INT; sleep 30")
        .current_dir(cwd)
        .spawn()
        .expect("spawn monitor-wrapper-shaped fixture");
    let pid = child.id();

    assert!(
        devflow_core::test_support::wait_for_exec_visibility(
            pid,
            "sh",
            devflow_core::test_support::EXEC_VISIBILITY_WAIT,
            devflow_core::test_support::EXEC_VISIBILITY_POLL,
        ),
        "pid {pid}: exec visibility timed out before the fixture became discoverable"
    );

    child
}

/// Task 2's core proof: a real process, structurally shaped like the
/// monitor wrapper, has its project root deleted out from under it while
/// it is still alive. `devflow stop` — the pre-25 recovery path — cannot
/// even resolve the deleted root, so it reports the process as
/// unreachable rather than clearing it (999.44's exact reproduction). The
/// registry-independent primitives this plan surfaces on the CLI still
/// find it (proving the root's deletion took nothing away from
/// discovery) and clear it with verified death.
#[test]
fn reap_clears_a_process_whose_root_was_deleted_which_devflow_stop_cannot_see() {
    let (outer, root) = nested_root_fixture();
    let mut child = spawn_monitor_wrapper_fixture(&root);
    let pid = child.id();

    // Give the shell a moment to actually start (and install its trap)
    // before we pull the rug out from under it.
    wait_for(
        || agent::agent_running(pid),
        5,
        "the fixture to start running",
    );

    // Record identity BEFORE deleting the root -- exactly what
    // `discover_stray_devflow_processes` does at discovery time, and what
    // a caller must re-confirm immediately before acting (999.47's
    // "Related TOCTOU").
    let start_time =
        agent::process_start_time(pid).expect("must read the fixture's own recorded start time");

    // Pull the root out from under the live process -- 999.44's exact
    // scenario. `root` itself is gone; `outer` (the tempdir guard) is not,
    // and the test process keeps running throughout.
    std::fs::remove_dir_all(&root).expect("delete the fixture's project root while it is alive");
    assert!(!root.exists(), "the root must actually be gone");
    assert!(
        agent::agent_running(pid),
        "the fixture must still be alive immediately after its root is deleted -- deleting a \
         directory does not touch a process using it only as a historical cwd"
    );

    // 999.44's exact reproduction: `devflow stop` cannot even resolve the
    // deleted path, so it reports the process as unreachable -- neither
    // clearing it nor pretending the machine is otherwise clean by
    // signalling something else.
    let stop_output = Command::new(devflow_bin())
        .args(["stop", "--phase", "900", "--root"])
        .arg(&root)
        .output()
        .expect("run devflow stop");
    assert!(
        !stop_output.status.success(),
        "devflow stop must fail to act on a root that no longer exists on disk, not silently \
         succeed against something else: stdout={} stderr={}",
        String::from_utf8_lossy(&stop_output.stdout),
        String::from_utf8_lossy(&stop_output.stderr)
    );
    assert!(
        agent::agent_running(pid),
        "devflow stop against a deleted root must not have touched the fixture"
    );

    // The registry-independent path: discovery does not consult the
    // deleted root, a lock file, or any state file at all -- it reads
    // `/proc` directly, so the root's absence changes nothing about what
    // it can see.
    let discovered = agent::discover_stray_devflow_processes();
    let candidate = discovered
        .iter()
        .find(|p| p.pid == pid)
        .expect("discovery must still find the fixture even though its root is gone");
    assert_eq!(candidate.layer, agent::StrayLayer::MonitorWrapper);

    // Re-confirm identity immediately before acting (mirrors
    // `commands::reap_stray_candidates`'s own re-confirmation), then clear
    // it with a VERIFIED signal -- never a bare, unverified one.
    assert!(
        agent::is_same_process(candidate.pid, candidate.start_time),
        "the freshly discovered candidate must match its own just-recorded start time"
    );
    assert_eq!(
        candidate.start_time, start_time,
        "discovery's recorded start time must agree with what we read directly"
    );

    let cleared = agent::terminate_and_verify(
        candidate.pid,
        agent::TERMINATE_VERIFY_WAIT,
        agent::TERMINATE_VERIFY_POLL,
    );
    assert!(
        cleared,
        "the reaping path must clear a process whose root has been deleted -- exactly the \
         condition file-based discovery structurally cannot see"
    );
    assert!(
        !agent::agent_running(pid),
        "the fixture must be verified dead after reaping, not merely assumed"
    );

    // Belt-and-braces teardown: `terminate_and_verify` already reports the
    // process dead, but always reap on every exit path (999.46's
    // constraint), including when an earlier assertion in this test would
    // have panicked first.
    let _ = child.kill();
    let _ = child.wait();
    drop(outer);
}

/// Task 2 behavior: a `SIGTERM`-ignoring stray -- 999.44's own measured
/// failure mode, where 15 of 15 orphaned wrappers survived `SIGTERM` and
/// only `SIGKILL` cleared them -- is still cleared through the same
/// escalating, verified path, within a bounded wait, even with its root
/// already deleted.
#[test]
fn reap_clears_a_sigterm_ignoring_stray_with_a_deleted_root() {
    let (outer, root) = nested_root_fixture();
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("trap '' TERM; sleep 30")
        .current_dir(&root)
        .spawn()
        .expect("spawn TERM-ignoring fixture");
    let pid = child.id();

    wait_for(
        || agent::agent_running(pid),
        5,
        "the fixture to start running",
    );
    // Give the shell a moment to install its TERM-ignoring trap before we
    // start signalling it.
    std::thread::sleep(Duration::from_millis(100));

    std::fs::remove_dir_all(&root).expect("delete the fixture's project root while it is alive");

    // This fixture's script has no wrapper marker, so discovery would not
    // structurally match it as Layer 1 -- that's fine here: the point of
    // this test is `terminate_and_verify`'s escalation on a real,
    // root-deleted process, which is exactly what
    // `commands::reap_stray_candidates` calls on every discovered
    // candidate regardless of which layer matched it.
    let start = Instant::now();
    let cleared = agent::terminate_and_verify(
        pid,
        agent::TERMINATE_VERIFY_WAIT,
        agent::TERMINATE_VERIFY_POLL,
    );
    let elapsed = start.elapsed();

    assert!(
        cleared,
        "a TERM-ignoring stray with a deleted root must still be cleared via SIGKILL escalation"
    );
    assert!(
        !agent::agent_running(pid),
        "the fixture must be verified dead, not merely signalled"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "escalation must complete within the bounded wait, took {elapsed:?}"
    );

    let _ = child.kill();
    let _ = child.wait();
    drop(outer);
}
