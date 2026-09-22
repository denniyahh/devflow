//! `devflow recover --clean` must report what it actually did (Phase 48
//! code review, finding D): an explicit clean refused by a held phase lock
//! exits non-zero, and a sweep that cleared nothing does not claim a cleanup.
//! Each refusal test has a control that must still clean and say so.

use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::state::{AgentKind, State};
use std::path::Path;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

/// A PID that is essentially certain not to map to a live process.
const DEAD_PID: u32 = 0x7FFF_FFFE;

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

fn init_repo(root: &Path) {
    let output = devflow_core::test_support::git_command(root)
        .args(["init", "-q"])
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Save a state for `phase` that started `age_secs` ago and whose recorded
/// agent pid is dead, so staleness turns on age alone.
fn save_state_aged(root: &Path, phase: PhaseId, age_secs: u64) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.started_at = now.saturating_sub(age_secs).to_string();
    devflow_core::workflow::save_state(&state).unwrap();
    let pid_path = devflow_core::agent_result::agent_pid_path(root, phase);
    std::fs::create_dir_all(pid_path.parent().unwrap()).unwrap();
    std::fs::write(pid_path, DEAD_PID.to_string()).unwrap();
}

fn recover_clean(root: &Path, phase: Option<PhaseId>) -> Output {
    let mut command = Command::new(devflow_bin());
    command.args(["recover", "--clean"]);
    if let Some(phase) = phase {
        command.args(["--phase", &phase.to_string()]);
    }
    command.arg(root).output().expect("run devflow recover")
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn explicit_clean_with_a_held_phase_lock_fails_and_cleans_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(61);
    save_state_aged(root, phase, 60);
    let state_path = devflow_core::workflow::state_path(root, phase);
    let before = std::fs::read(&state_path).unwrap();
    let guard = devflow_core::lock::acquire(root, phase).expect("hold phase lock");

    let output = recover_clean(root, Some(phase));
    drop(guard);

    let text = combined(&output);
    assert!(
        !output.status.success(),
        "a clean refused by a held lock must exit non-zero: {text}"
    );
    assert!(
        text.contains("nothing was cleaned"),
        "the refusal must say nothing was cleaned: {text}"
    );
    assert!(
        !text.contains("cleaned up workflow state"),
        "a refused clean must not claim a cleanup: {text}"
    );
    assert_eq!(
        std::fs::read(&state_path).ok(),
        Some(before),
        "a refused clean must leave the state byte-identical"
    );
}

#[test]
fn explicit_clean_without_a_lock_holder_cleans_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(62);
    save_state_aged(root, phase, 60);

    let output = recover_clean(root, Some(phase));

    let text = combined(&output);
    assert!(output.status.success(), "unlocked clean failed: {text}");
    assert!(
        text.contains("cleaned up workflow state for phase 62"),
        "a real clean must say what it cleaned: {text}"
    );
    assert!(!devflow_core::workflow::state_path(root, phase).exists());
}

#[test]
fn sweep_that_clears_nothing_does_not_claim_a_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(63);
    save_state_aged(root, phase, 60);

    let output = recover_clean(root, None);

    let text = combined(&output);
    assert!(output.status.success(), "sweep failed: {text}");
    assert!(
        text.contains("no stale workflow state was cleaned"),
        "an empty sweep must say it cleaned nothing: {text}"
    );
    assert!(
        !text.contains("cleaned up stale workflow state"),
        "an empty sweep must not claim a cleanup: {text}"
    );
    assert!(devflow_core::workflow::state_path(root, phase).exists());
}

#[test]
fn sweep_names_the_stale_phase_it_cleared() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(64);
    save_state_aged(
        root,
        phase,
        devflow_core::recover::STALE_THRESHOLD.as_secs() + 60,
    );

    let output = recover_clean(root, None);

    let text = combined(&output);
    assert!(output.status.success(), "sweep failed: {text}");
    assert!(
        text.contains("cleaned up stale workflow state for phase 64"),
        "a sweep must name the phase it cleared: {text}"
    );
    assert!(!devflow_core::workflow::state_path(root, phase).exists());
}
