//! `devflow discard --phase N` on the debris a killed run can leave (D-06): gate files
//! nothing waits on and hold temps whose creator died, including a phase with no run
//! state, worktree or branch. A gate whose waiter registration is held is refused.
//!
//! Every devflow child is built by [`common::hermetic_devflow`].

mod common;

use devflow_core::gates::{GateKind, Gates};
use devflow_core::liveness;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

/// A pid no process has: above any `pid_max` Linux allows, and inside the `1..=i32::MAX`
/// range a hold temp's name may carry, so its creator reads as dead.
const DEAD_PID: i32 = i32::MAX;

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

/// A repository with one commit on `develop`, and the scratch home its devflow
/// children use.
struct Case {
    root: PathBuf,
    phase: PhaseId,
    _repo: tempfile::TempDir,
    /// Declared last so it drops after the rest of the case.
    home: common::HermeticHome,
}

impl Case {
    /// A repository with no run state, worktree or branch for `phase`, whose phase
    /// holder and runner lock files exist unheld, as a run that ended leaves them.
    fn new(phase: PhaseId) -> Self {
        let repo = tempfile::tempdir().unwrap();
        // The binary canonicalizes the project root, so the case must too.
        let root = repo.path().canonicalize().unwrap();
        git(&root, &["init", "-q"]);
        git(&root, &["config", "user.email", "devflow@example.com"]);
        git(&root, &["config", "user.name", "DevFlow Tests"]);
        git(&root, &["config", "commit.gpgsign", "false"]);
        git(&root, &["config", "core.hooksPath", "/dev/null"]);
        git(&root, &["checkout", "-q", "-b", "develop"]);
        fs::write(root.join("README.md"), "base\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "base"]);
        drop(liveness::claim_phase_holder(&root, phase).expect("create the holder file"));
        drop(liveness::claim_runner_lock(&root, phase).expect("create the runner lock file"));
        Self {
            root,
            phase,
            _repo: repo,
            home: common::HermeticHome::new(),
        }
    }

    /// `devflow discard --phase N` on the repository.
    fn discard(&self) -> Output {
        common::hermetic_devflow(&self.home)
            .args(["discard", "--phase", &self.phase.to_string()])
            .arg(&self.root)
            .current_dir(&self.root)
            .output()
            .expect("run devflow discard")
    }

    /// The phase's files in the gates directory, by name.
    fn gate_files(&self) -> Vec<String> {
        let prefix = format!("{}-", self.phase.padded());
        let Ok(entries) = fs::read_dir(Gates::dir(&self.root)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.trim_start_matches('.').starts_with(&prefix))
            .collect();
        names.sort();
        names
    }

    /// The lock files discard must never remove: the phase holder and the runner lock.
    fn assert_lock_files_remain(&self, out: &Output) {
        for path in [
            liveness::phase_holder_path(&self.root, self.phase),
            liveness::runner_lock_path(&self.root, self.phase),
        ] {
            assert!(
                path.is_file(),
                "lock file {} removed by discard:\n{}",
                path.display(),
                text(out)
            );
        }
    }
}

/// stdout and stderr of `out`, for failure messages.
fn text(out: &Output) -> String {
    format!(
        "status {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A hold temp beside `final_path`, as a creator with `pid` leaves one mid-take.
fn hold_temp(final_path: &Path, pid: i32) -> PathBuf {
    let name = final_path.file_name().unwrap().to_string_lossy();
    let temp = final_path.with_file_name(format!(".{name}.{pid}.0{}", liveness::HOLD_TEMP_SUFFIX));
    fs::create_dir_all(temp.parent().unwrap()).unwrap();
    fs::write(&temp, "").unwrap();
    temp
}

/// D-06: a phase with nothing but a gate request and an abandoned waiter registration
/// (no state, worktree or branch) is not refused: discard lists the gate files, removes
/// them and exits 0.
#[test]
fn discard_removes_the_orphan_gate_files_of_a_gate_only_phase() {
    let case = Case::new(PhaseId::new(71));
    let gate = Gates::open_gate(
        &case.root,
        case.phase,
        Stage::Code,
        GateKind::StageFailure,
        "a gate a killed run left",
    )
    .expect("open the gate")
    .gate_id;
    drop(Gates::register_waiter(&case.root, case.phase, Stage::Code, &gate).expect("register"));
    let request = Gates::gate_path(&case.root, case.phase, Stage::Code);
    assert_eq!(case.gate_files().len(), 2, "fixture: request and waiter");

    let out = case.discard();
    assert!(out.status.success(), "discard refused:\n{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let listing = stdout.split("\nremoved ").next().unwrap_or_default();
    assert!(
        listing.contains(&request.display().to_string()),
        "the listing names the gate request before any removal:\n{}",
        text(&out)
    );
    assert!(
        case.gate_files().is_empty(),
        "gate file left: {:?}\n{}",
        case.gate_files(),
        text(&out)
    );
    case.assert_lock_files_remain(&out);
}

/// D-06: discard removes the phase's hold temps whose creator died, in `.devflow/` and
/// in the gates directory, on a phase with nothing else. Control: a temp of the same
/// phase whose creator is this live test process is kept.
#[test]
fn discard_removes_a_phases_hold_temps() {
    let case = Case::new(PhaseId::new(72));
    let holder = liveness::phase_holder_path(&case.root, case.phase);
    let waiter = Gates::dir(&case.root).join(format!("{}-code.abc.waiter", case.phase.padded()));
    let dead = [hold_temp(&holder, DEAD_PID), hold_temp(&waiter, DEAD_PID)];
    let own_pid = i32::try_from(std::process::id()).unwrap();
    let alive = hold_temp(&liveness::runner_lock_path(&case.root, case.phase), own_pid);

    let out = case.discard();
    assert!(out.status.success(), "discard refused:\n{}", text(&out));
    for temp in &dead {
        assert!(
            !temp.exists(),
            "hold temp left: {}\n{}",
            temp.display(),
            text(&out)
        );
    }
    assert!(
        alive.exists(),
        "control: a temp whose creator is alive was removed:\n{}",
        text(&out)
    );
    case.assert_lock_files_remain(&out);
}

/// D-06, T-52.2-22: a gate whose waiter registration is held is refused, and its gate
/// file stays. Control: once the waiter lets go, the same discard removes it.
#[test]
fn discard_refuses_a_gate_whose_waiter_is_live() {
    let case = Case::new(PhaseId::new(73));
    let gate = Gates::open_gate(
        &case.root,
        case.phase,
        Stage::Plan,
        GateKind::StageFailure,
        "a gate something still waits on",
    )
    .expect("open the gate")
    .gate_id;
    let registration =
        Gates::register_waiter(&case.root, case.phase, Stage::Plan, &gate).expect("register");
    let request = Gates::gate_path(&case.root, case.phase, Stage::Plan);

    let out = case.discard();
    assert!(
        request.is_file(),
        "live waiter's gate removed:\n{}",
        text(&out)
    );
    assert!(!out.status.success(), "discard exited 0:\n{}", text(&out));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("live waiter") && stderr.contains("nothing was removed"),
        "the refusal names the live waiter and that nothing was removed:\n{}",
        text(&out)
    );
    case.assert_lock_files_remain(&out);

    drop(registration);
    let out = case.discard();
    assert!(
        out.status.success(),
        "control: discard proceeds:\n{}",
        text(&out)
    );
    assert!(
        case.gate_files().is_empty(),
        "control: gate file left once the waiter let go: {:?}\n{}",
        case.gate_files(),
        text(&out)
    );
    case.assert_lock_files_remain(&out);
}

/// A [`Case`] with a saved, stopped run state for `phase` and two answered
/// questions in the event log: `q-run` of that run and `q-other` of an
/// earlier one. Returns the run's `started_at`.
fn case_with_a_run(phase: PhaseId) -> (Case, String) {
    let case = Case::new(phase);
    let mut state = devflow_core::state::State::new(
        phase,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Auto,
        case.root.clone(),
    );
    state.stopped = true;
    devflow_core::workflow::save_state(&state).unwrap();
    let started_at = state.started_at.clone();
    let phase_number = phase.to_string().parse::<u64>().unwrap();
    let options = serde_json::json!([{"id": "a", "label": "A"}, {"id": "b", "label": "B"}]);
    let mut lines = String::new();
    for (ts, question_id, run) in [(10, "q-run", started_at.as_str()), (12, "q-other", "1")] {
        let asked = serde_json::json!({
            "v": 1, "ts": ts, "phase": phase_number, "event": "question_asked",
            "stage": "code", "question_id": question_id, "kind": "decision",
            "question": "Which?", "options": options, "run_started_at": run,
            "triage": {"outcome": "decided", "top_option": "a"},
        });
        let answered = serde_json::json!({
            "v": 1, "ts": ts + 1, "phase": phase_number, "event": "question_answered",
            "stage": "code", "question_id": question_id, "answer": {"option_id": "a"},
            "policy": "triage", "answered_by": "triage:jev",
        });
        lines.push_str(&format!("{asked}\n{answered}\n"));
    }
    fs::write(devflow_core::events::events_path(&case.root), lines).unwrap();
    (case, started_at)
}

impl Case {
    /// `devflow <args>` on the repository, as the operator.
    fn devflow(&self, args: &[&str]) -> Output {
        common::hermetic_devflow(&self.home)
            .args(args)
            .env_remove("DEVFLOW_ANSWERED_BY")
            .current_dir(&self.root)
            .output()
            .expect("run devflow")
    }

    fn log(&self) -> String {
        fs::read_to_string(devflow_core::events::events_path(&self.root)).unwrap()
    }

    /// `devflow ledger override` of `question_id` to option `b`.
    fn override_to_b(&self, question_id: &str) -> Output {
        let phase = self.phase.to_string();
        let root = self.root.display().to_string();
        let args = ["ledger", "override", &phase, "--question-id", question_id];
        self.devflow(&[&args[..], &["--option", "b", "--project", &root]].concat())
    }
}

/// The `run_discarded` events in `log`.
fn run_discarded_events(log: &str) -> Vec<serde_json::Value> {
    log.lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|event| event["event"] == "run_discarded")
        .collect()
}

/// D-16: discard keeps the event log, every earlier line unchanged, and adds
/// one `run_discarded` naming the run; the ledger still lists its questions.
#[test]
fn the_ledger_survives_discard() {
    let (case, started_at) = case_with_a_run(PhaseId::new(81));
    let before = case.log();

    let out = case.discard();
    assert!(out.status.success(), "discard refused:\n{}", text(&out));
    let after = case.log();
    assert!(
        after.starts_with(&before),
        "earlier lines changed:\n{after}"
    );
    let discarded = run_discarded_events(&after);
    assert_eq!(discarded.len(), 1, "{after}");
    assert_eq!(discarded[0]["started_at"], started_at.as_str(), "{after}");

    let root = case.root.display().to_string();
    let ledger = case.devflow(&["ledger", "81", &root, "--json"]);
    assert!(ledger.status.success(), "ledger failed:\n{}", text(&ledger));
    let json: serde_json::Value = serde_json::from_slice(&ledger.stdout).unwrap();
    let ids: Vec<&str> = json["decided"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["question_id"].as_str())
        .collect();
    assert_eq!(ids, ["q-run", "q-other"], "{json}");
}

/// T-54-35: a discard whose `run_discarded` record cannot be written removes
/// nothing. Control: the same fixture with a writable log is removed and
/// gains the record.
#[test]
fn a_discard_whose_record_cannot_be_written_removes_nothing() {
    let (case, _) = case_with_a_run(PhaseId::new(82));
    let log = devflow_core::events::events_path(&case.root);
    fs::remove_file(&log).unwrap();
    fs::create_dir(&log).unwrap();
    let state = devflow_core::workflow::state_path(&case.root, case.phase);

    let out = case.discard();
    assert!(!out.status.success(), "discard succeeded:\n{}", text(&out));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("events.jsonl"), "{}", text(&out));
    assert!(stderr.contains("nothing was removed"), "{}", text(&out));
    assert!(state.exists(), "the run state was removed:\n{}", text(&out));

    let (control, _) = case_with_a_run(PhaseId::new(82));
    let out = control.discard();
    assert!(
        out.status.success(),
        "control: discard refused:\n{}",
        text(&out)
    );
    assert!(
        !devflow_core::workflow::state_path(&control.root, control.phase).exists(),
        "control: the run state was kept:\n{}",
        text(&out)
    );
    assert_eq!(run_discarded_events(&control.log()).len(), 1);
}

/// D-16: after discard, an override of a question of the discarded run is
/// refused naming discard and writes nothing. Control: a question of another
/// run is accepted.
#[test]
fn an_override_from_a_discarded_run_names_discard() {
    let (case, _) = case_with_a_run(PhaseId::new(83));
    let out = case.discard();
    assert!(out.status.success(), "discard refused:\n{}", text(&out));
    let before = case.log();

    let out = case.override_to_b("q-run");
    assert!(
        !out.status.success(),
        "the override was accepted:\n{}",
        text(&out)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("devflow discard"), "{}", text(&out));
    assert_eq!(case.log(), before, "the refused override wrote to the log");

    let out = case.override_to_b("q-other");
    assert!(
        out.status.success(),
        "control: the override was refused:\n{}",
        text(&out)
    );
}
