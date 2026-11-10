//! `devflow recover --clean` must report what it actually did (Phase 48
//! code review, finding D): an explicit clean refused because a registered gate
//! waiter or a live pending supervisor may still run exits non-zero, and a sweep
//! that cleared nothing does not claim a cleanup. Each refusal test has a control
//! that must still clean and say so.

mod common;

use devflow_core::gates::Gates;
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, PendingSupervisor, State};
use std::path::Path;
use std::process::Output;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A PID that is essentially certain not to map to a live process.
const DEAD_PID: u32 = 0x7FFF_FFFE;

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
    let home = common::HermeticHome::new();
    let mut command = common::hermetic_devflow(&home);
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
fn explicit_clean_with_a_registered_waiter_fails_and_cleans_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(61);
    save_state_aged(root, phase, 60);
    let state_path = devflow_core::workflow::state_path(root, phase);
    let before = std::fs::read(&state_path).unwrap();
    let (opened, registration) = Gates::open_registered(root, phase, Stage::Code, "waiting")
        .expect("register as the gate's waiter");
    let gate_before = std::fs::read(&opened.path).unwrap();

    let output = recover_clean(root, Some(phase));
    drop(registration);

    let text = combined(&output);
    assert!(
        !output.status.success(),
        "a clean refused by a registered waiter must exit non-zero: {text}"
    );
    assert!(
        text.contains("registered gate waiter")
            && text.contains(&format!("devflow stop --phase {phase}")),
        "the refusal must name the waiter and the repair: {text}"
    );
    assert!(
        text.contains("nothing was removed"),
        "the refusal must say nothing was removed: {text}"
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
    assert_eq!(std::fs::read(&opened.path).ok(), Some(gate_before));
}

/// 50.1-16: a launch's supervisor holding its reservation, with no socket yet. Both the
/// CLI's guard and the decision under the write lock refuse this; the units beside each
/// (`recover_refuses_a_live_pending_supervisor`,
/// `explicit_clean_refuses_a_live_pending_supervisor`) prove each one alone.
#[test]
fn explicit_clean_with_a_live_pending_supervisor_fails_and_cleans_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(72);
    save_state_aged(root, phase, 60);
    let state_path = devflow_core::workflow::state_path(root, phase);
    let liveness_path = root.join(".devflow/supervisor-aaaa0000aaaa0072.live");
    let mut state = devflow_core::workflow::load_state(root, phase).unwrap();
    state.pending_supervisor = Some(PendingSupervisor::new(
        "aaaa0000aaaa0072".to_string(),
        root.join("no-such.sock"),
        liveness_path.clone(),
    ));
    devflow_core::workflow::save_state(&state).unwrap();
    let before = std::fs::read(&state_path).unwrap();
    let hold = devflow_core::liveness::Hold::take(&liveness_path).expect("hold the reservation");

    let output = recover_clean(root, Some(phase));
    drop(hold);

    let text = combined(&output);
    assert!(
        !output.status.success(),
        "a clean over a live pending supervisor must exit non-zero: {text}"
    );
    assert!(
        text.contains("recover --clean deleted nothing")
            && text.contains(&format!("devflow stop --phase {phase}")),
        "the refusal must say so and name the repair: {text}"
    );
    assert_eq!(std::fs::read(&state_path).ok(), Some(before));
}

#[test]
fn explicit_clean_of_a_phase_with_nothing_live_cleans_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(62);
    save_state_aged(root, phase, 60);

    let output = recover_clean(root, Some(phase));

    let text = combined(&output);
    assert!(output.status.success(), "the clean failed: {text}");
    assert!(
        text.contains("cleaned up workflow state for phase 62"),
        "a real clean must say what it cleaned: {text}"
    );
    assert!(!devflow_core::workflow::state_path(root, phase).exists());
}

/// The repair `stop` and `doctor` name for a crashed waiter must work: an unheld
/// registration says nobody waits, so it keeps nothing.
/// `explicit_clean_with_a_registered_waiter_fails_and_cleans_nothing` is the live control.
#[test]
fn explicit_clean_with_an_unheld_registration_cleans() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(65);
    save_state_aged(root, phase, 60);
    let (opened, registration) =
        Gates::open_registered(root, phase, Stage::Code, "wedged at a crashed waiter")
            .expect("register as the gate's waiter");
    let waiter = Gates::waiter_path(root, phase, Stage::Code, &opened.gate_id);
    drop(registration);
    assert!(waiter.exists(), "precondition: the crash leaves the file");

    // A released flock can read held while another test's fork is before its exec, and
    // a refused clean changes nothing, so a retry starts from the same place.
    let deadline = Instant::now() + Duration::from_secs(10);
    let text = loop {
        let output = recover_clean(root, Some(phase));
        let text = combined(&output);
        if output.status.success() {
            break text;
        }
        assert!(Instant::now() < deadline, "the clean kept refusing: {text}");
        std::thread::sleep(Duration::from_millis(20));
    };

    assert!(
        text.contains("cleaned up workflow state for phase 65"),
        "the clean must say what it cleaned: {text}"
    );
    assert!(!devflow_core::workflow::state_path(root, phase).exists());
    assert!(
        !opened.path.exists(),
        "the phase's gate file must be removed"
    );
    assert!(!waiter.exists(), "the unheld registration must be removed");
}

/// `recover` with no flags says why a phase may be live; a phase with nothing live prints
/// no such line.
#[test]
fn inspect_prints_why_a_phase_may_be_live() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let (waited, idle) = (PhaseId::new(73), PhaseId::new(74));
    save_state_aged(root, waited, 60);
    save_state_aged(root, idle, 60);
    let (_opened, registration) = Gates::open_registered(root, waited, Stage::Code, "waiting")
        .expect("register as the gate's waiter");
    let home = common::HermeticHome::new();

    let output = common::hermetic_devflow(&home)
        .arg("recover")
        .arg(root)
        .output()
        .expect("run devflow recover");
    drop(registration);

    let text = combined(&output);
    assert!(output.status.success(), "inspect failed: {text}");
    let idle_at = text.find("phase: 74").expect(&text);
    let (waited_part, idle_part) = text.split_at(idle_at);
    assert!(
        waited_part.contains("may be live:") && waited_part.contains("registered gate waiter"),
        "{text}"
    );
    assert!(!idle_part.contains("may be live:"), "{text}");
}

/// The sweep names each abandoned marker it removed, and lists a set-aside answer it left.
#[test]
fn sweep_names_the_markers_it_removed_and_the_answers_it_kept() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(75);
    let gate = Gates::open_gate(root, phase, Stage::Code, "ctx")
        .unwrap()
        .gate_id;
    Gates::answer(
        root,
        phase,
        Stage::Code,
        &gate,
        &devflow_core::gates::GateResponse {
            approved: true,
            note: None,
            responded_by: Some("test".into()),
        },
    )
    .unwrap();
    Gates::close(root, phase, Stage::Code).unwrap();
    let answers = Gates::quarantined(root);
    assert_eq!(answers.len(), 1, "fixture: {answers:?}");
    let marker = root.join(".devflow/supervisor-aaaa0000aaaa0075.live");
    drop(devflow_core::liveness::Hold::take(&marker).unwrap());

    // A released flock can read held while another test's fork is before its exec.
    let deadline = Instant::now() + Duration::from_secs(10);
    let text = loop {
        let text = combined(&recover_clean(root, None));
        if !marker.exists() {
            break text;
        }
        assert!(Instant::now() < deadline, "never removed: {text}");
        std::thread::sleep(Duration::from_millis(20));
    };

    assert!(
        text.contains(&format!("removed abandoned marker {}", marker.display())),
        "{text}"
    );
    assert!(
        text.contains("set-aside gate answers kept")
            && text.contains(&answers[0].path.display().to_string()),
        "{text}"
    );
    assert!(
        answers[0].path.exists(),
        "a set-aside answer is never removed"
    );
    assert!(
        !text.contains("no stale workflow state was cleaned"),
        "a sweep that removed a marker must not say it cleaned nothing: {text}"
    );
}

/// Fix-review finding C-3: `clean_phase` is idempotent, so an explicit clean
/// of a phase with nothing on disk succeeded and printed "cleaned up".
/// `explicit_clean_of_a_phase_with_nothing_live_cleans_and_says_so` is the control.
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
    // The explicit clean names only what it looks for. There is no cron record
    // to look for any more (D-01), so the no-op line does not claim one.
    assert!(
        text.lines()
            .any(|line| line == "nothing to clean for phase 66: no workflow state or gate files"),
        "the no-op line names exactly the state and gate files it looked for: {text}"
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

/// Phase 50 D-20: the sweep leaves a `state.json` from an old binary alone, so it
/// has cleaned nothing and says so.
#[test]
fn sweep_leaves_a_legacy_state_file_alone_and_says_it_cleaned_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let legacy = root.join(".devflow/state.json");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::write(&legacy, "{\"stage\":").unwrap();

    let output = recover_clean(root, None);

    let text = combined(&output);
    assert!(output.status.success(), "sweep failed: {text}");
    assert_eq!(std::fs::read_to_string(&legacy).unwrap(), "{\"stage\":");
    assert!(
        text.contains("no stale workflow state was cleaned"),
        "a sweep that removed nothing must say so: {text}"
    );
    assert!(
        !text.contains("legacy"),
        "the sweep has no legacy state to report: {text}"
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

/// Phase 50 D-20: a `state.json` naming the phase is not that phase's state, so an
/// explicit clean finds nothing to clean and leaves the file alone.
#[test]
fn explicit_clean_leaves_a_legacy_state_file_alone_and_finds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = PhaseId::new(68);
    let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    let legacy = root.join(".devflow/state.json");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    let bytes = serde_json::to_string(&state).unwrap();
    std::fs::write(&legacy, &bytes).unwrap();

    let output = recover_clean(root, Some(phase));

    let text = combined(&output);
    assert!(output.status.success(), "explicit clean failed: {text}");
    assert!(
        text.contains("nothing to clean for phase 68"),
        "a legacy file is not the phase's state: {text}"
    );
    assert_eq!(std::fs::read_to_string(&legacy).unwrap(), bytes);
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
