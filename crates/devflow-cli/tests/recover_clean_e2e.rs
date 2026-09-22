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

/// Fix-review finding C-3: `clean_phase` is idempotent, so an explicit clean
/// of a phase with nothing on disk succeeded and printed "cleaned up".
/// `explicit_clean_without_a_lock_holder_cleans_and_says_so` is the control.
#[test]
fn explicit_clean_of_a_phase_with_nothing_to_clean_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(66);

    let output = recover_clean(root, Some(phase));

    let text = combined(&output);
    assert!(
        output.status.success(),
        "a no-op clean is not an error: {text}"
    );
    assert!(
        text.contains("nothing to clean for phase 66"),
        "a no-op clean must say it found nothing: {text}"
    );
    assert!(
        !text.contains("cleaned up workflow state"),
        "a no-op clean must not claim a cleanup: {text}"
    );
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

/// R-6: removing a corrupt legacy state file is a cleanup that the sweep
/// must report, not a warning followed by an empty-sweep claim.
#[test]
fn sweep_removes_and_names_corrupt_legacy_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let legacy = root.join(".devflow/state.json");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::write(&legacy, "{\"stage\":").unwrap();

    let output = recover_clean(root, None);

    let text = combined(&output);
    assert!(output.status.success(), "sweep failed: {text}");
    assert!(
        !legacy.exists(),
        "the corrupt legacy state file must be removed"
    );
    assert!(
        text.lines().any(|line| {
            line.contains("legacy state.json")
                && line.contains("removed")
                && !line.starts_with("warning:")
        }),
        "the removal must have a non-warning report line: {text}"
    );
    assert!(
        !text.contains("no stale workflow state was cleaned"),
        "a sweep that removed legacy state must not say it cleaned nothing: {text}"
    );
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

/// 48-REVIEW WR-02: gate files whose phase has no state are removed by the
/// sweep and named, rather than left as open gates nothing will answer.
#[test]
fn sweep_removes_and_names_orphan_gate_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(67);
    let gate = devflow_core::gates::Gates::write_gate(
        root,
        phase,
        devflow_core::stage::Stage::Code,
        "orphaned",
    )
    .unwrap();

    let output = recover_clean(root, None);

    let text = combined(&output);
    assert!(output.status.success(), "sweep failed: {text}");
    assert!(
        text.contains("removed orphan gate files for phase 67"),
        "the sweep must name the orphan gate files it removed: {text}"
    );
    assert!(
        !text.contains("no stale workflow state was cleaned"),
        "a sweep that removed files must not say it cleaned nothing: {text}"
    );
    assert!(!gate.exists());
}

/// R-1: an orphan cron record is a cleanup the sweep must name, not an empty
/// state sweep.
#[test]
fn sweep_removes_and_names_an_orphan_cron_record() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(70);
    let record = devflow_core::ship::build_single_agent_cron_instructions(root, phase, "");
    devflow_core::ship::write_cron_instructions(root, &record).unwrap();
    let record_path = devflow_core::ship::cron_instructions_path(root, phase);

    let output = recover_clean(root, None);

    let text = combined(&output);
    assert!(output.status.success(), "sweep failed: {text}");
    assert!(
        !record_path.exists(),
        "the valid orphan cron record must be removed"
    );
    assert!(
        text.contains(&format!("phase {phase}"))
            && text.contains("cron")
            && text.contains("removed")
            && !text.contains("no stale workflow state was cleaned"),
        "a removed orphan cron record must be named as a cleanup, not an empty sweep: {text}"
    );
}

/// A cron record still owned by a persisted state is neither removed nor
/// reported as an orphan cleanup.
#[test]
fn sweep_keeps_a_cron_record_for_a_phase_that_still_has_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(71);
    save_state_aged(root, phase, 60);
    let record = devflow_core::ship::build_single_agent_cron_instructions(root, phase, "");
    devflow_core::ship::write_cron_instructions(root, &record).unwrap();
    let record_path = devflow_core::ship::cron_instructions_path(root, phase);

    let output = recover_clean(root, None);

    let text = combined(&output);
    assert!(output.status.success(), "sweep failed: {text}");
    assert!(
        record_path.exists(),
        "a cron record still owned by state must be retained"
    );
    assert!(
        !text
            .lines()
            .any(|line| line.contains("cron") && line.contains(&format!("phase {phase}"))),
        "a retained cron record must not be reported as removed: {text}"
    );
}

/// 48-REVIEW WR-03: removing a phase held only in a legacy `state.json` is a
/// cleanup, not "nothing to clean".
#[test]
fn explicit_clean_of_legacy_state_says_it_cleaned() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(68);
    let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    let legacy = root.join(".devflow/state.json");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::write(&legacy, serde_json::to_string(&state).unwrap()).unwrap();

    let output = recover_clean(root, Some(phase));

    let text = combined(&output);
    assert!(output.status.success(), "legacy clean failed: {text}");
    assert!(
        text.contains("cleaned up workflow state for phase 68"),
        "removing legacy state must be reported as a cleanup: {text}"
    );
    assert!(!legacy.exists());
    assert!(!devflow_core::workflow::state_path(root, phase).exists());
}

/// 48-REVIEW WR-03/WR-04: a removal that fails exits non-zero and never
/// claims a cleanup — for an explicit clean and for the sweep.
#[test]
fn a_failed_removal_exits_non_zero_without_claiming_a_cleanup() {
    for phase_arg in [Some(PhaseId::new(69)), None] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(69);
        // A directory in place of a gate file: removing it fails EISDIR, on
        // both the explicit path and the sweep's orphan-gate path.
        std::fs::create_dir_all(devflow_core::gates::Gates::gate_path(
            root,
            phase,
            devflow_core::stage::Stage::Code,
        ))
        .unwrap();

        let output = recover_clean(root, phase_arg);

        let text = combined(&output);
        assert!(
            !output.status.success(),
            "a failed removal must exit non-zero ({phase_arg:?}): {text}"
        );
        assert!(
            text.contains("could not remove everything"),
            "the failure must be stated ({phase_arg:?}): {text}"
        );
        assert!(
            !text.contains("cleaned up"),
            "a failed removal must not claim a cleanup ({phase_arg:?}): {text}"
        );
    }
}
