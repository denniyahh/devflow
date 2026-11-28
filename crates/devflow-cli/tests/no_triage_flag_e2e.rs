//! D-09 (Phase 54): `--no-triage` turns decision triage off for one run, on
//! `devflow start` and `devflow resume`, and is refused where it would change
//! nothing.
//!
//! The `start_*` launch cases read what `start` saved in `state-NN.json`; the
//! refusal runs through `--dry-run`. The `resume_*` cases drive `devflow resume`
//! itself, and their agent stub copies the state file when the relaunched stage
//! starts it, so they read the state that stage ran with. Every `devflow` child is built by `common::hermetic_devflow`
//! with its `PATH` set on the child, and the only agent on it is a stub, so no
//! case can reach a real agent.

mod common;

use common::HermeticPath;
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::state::{AgentKind, State};
use std::fs;
use std::path::Path;
use std::process::Output;
use std::time::Duration;

/// Copied from `fallback_agent_config.rs`: a fake `claude` that answers
/// `--version`, the delivery canary's planted token, and otherwise waits until
/// it is ended.
const WAITING_CLAUDE: &str = r#"if [ "$1" = "--version" ]; then echo "2.1.220 (Claude Code)"; exit 0; fi
read -r turn
case "$turn" in
  *DEVFLOW_DELIVERY_CANARY_*)
    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
    printf '{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"%s"}\n' "$token"
    exit 0
    ;;
esac
sleep 120"#;

/// A `devflow.toml` that turns triage on for the project.
const TRIAGE_ENABLED: &str = "[triage]\nenabled = true\n";

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

/// Copied from `fallback_agent_config.rs`: a repository on `develop` with a
/// roadmap entry for `phase`.
fn init_repo(root: &Path, phase: PhaseId) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    fs::create_dir_all(root.join(".planning")).unwrap();
    fs::write(root.join("README.md"), "base\n").unwrap();
    fs::write(
        root.join(".planning/ROADMAP.md"),
        format!("# Roadmap\n\n### Phase {phase}: no-triage fixture\n"),
    )
    .unwrap();
    git(root, &["add", "README.md", ".planning/ROADMAP.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
}

/// `devflow start --phase <phase> --agent claude --mode supervise
/// --no-worktree <extra>` with `path` as the child's whole `PATH`.
fn run_start(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    path: &str,
    extra: &[&str],
) -> Output {
    common::hermetic_devflow(home)
        .args([
            "start",
            "--phase",
            &phase.to_string(),
            "--agent",
            "claude",
            "--mode",
            "supervise",
            "--no-worktree",
        ])
        .args(extra)
        .arg(root)
        .current_dir(root)
        .hermetic_path(path)
        .output()
        .expect("run devflow start")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_success(output: &Output, what: &str) {
    assert!(
        output.status.success(),
        "{what} must succeed\nstdout: {}\nstderr: {}",
        stdout(output),
        stderr(output)
    );
}

/// The saved run state of a real `start` with `extra` on a project whose
/// devflow.toml enables triage, indexed so a missing field panics.
fn saved_state_after_start(
    phase: PhaseId,
    extra: &[&str],
) -> serde_json::Map<String, serde_json::Value> {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    init_repo(&root, phase);
    fs::write(root.join("devflow.toml"), TRIAGE_ENABLED).unwrap();
    let fake = common::fake_bin_dir_for("claude", WAITING_CLAUDE);
    let _reaper = common::LaunchedRunReaper::for_home(&home, &root, phase);
    let state_path = devflow_core::workflow::state_path(&root, phase);
    assert!(!state_path.exists(), "no state may exist before start");

    // The waiting stub needs `grep`, `head` and `sleep`, so the host PATH
    // follows the stub, as in `fallback_agent_config.rs`.
    let host_path = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake.path.display(), host_path.to_string_lossy());
    let mut args = vec!["--until", "plan"];
    args.extend_from_slice(extra);
    let output = run_start(&home, &root, phase, &path, &args);
    assert_success(&output, "a triage-enabled start");
    serde_json::from_str(&fs::read_to_string(&state_path).expect("start saved the state"))
        .expect("the state is a JSON object")
}

/// `start --no-triage` saves triage off in the run's state, which `__ask`
/// reads before it would send a Jev request.
#[test]
fn start_with_no_triage_saves_it_in_the_run_state() {
    let saved = saved_state_after_start(PhaseId::new(5481), &["--no-triage"]);
    assert_eq!(
        saved["triage_off"], true,
        "the run carries triage off: {saved:?}"
    );
}

/// Control: the same start without the flag leaves triage on for the run.
#[test]
fn start_without_no_triage_leaves_triage_on() {
    let saved = saved_state_after_start(PhaseId::new(5482), &[]);
    assert_eq!(
        saved["triage_off"], false,
        "a run started without the flag keeps triage on: {saved:?}"
    );
}

/// Restrictive verbs: on a project whose devflow.toml does not enable triage the
/// flag would change nothing, so even a dry run refuses it, naming the file, and
/// writes nothing under `.devflow/`.
#[test]
fn start_with_no_triage_is_refused_when_triage_is_not_enabled() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(5483);
    init_repo(root, phase);
    let fake = common::fake_bin_dir_for("claude", "exit 0");

    let output = run_start(
        &home,
        root,
        phase,
        &fake.path.display().to_string(),
        &["--no-triage", "--dry-run"],
    );

    assert!(
        !output.status.success(),
        "the dry run must refuse\nstdout: {}\nstderr: {}",
        stdout(&output),
        stderr(&output)
    );
    let refused = stderr(&output);
    assert!(
        refused.contains("--no-triage"),
        "names the flag\nstderr: {refused}"
    );
    assert!(
        refused.contains("devflow.toml"),
        "names the file\nstderr: {refused}"
    );
    assert!(
        !root.join(".devflow").exists(),
        "a refused start writes nothing under .devflow/"
    );
}

/// The resumed run's agent stub: it copies the run's state file to `marker`
/// when the relaunched stage starts it, then waits until it is ended.
fn state_copying_codex(state_path: &Path, marker: &Path) -> String {
    format!(
        "cp '{}' '{marker}.tmp' && mv '{marker}.tmp' '{marker}'\nsleep 60",
        state_path.display(),
        marker = marker.display()
    )
}

/// `devflow resume --phase <phase> <extra>` of a stopped Code run saved with
/// `triage_off` on a project whose devflow.toml enables triage. Returns the
/// state the relaunched stage ran with and the `started_at` the run was saved
/// with, each indexed so a missing field panics.
fn resumed_stage_state(
    phase: PhaseId,
    triage_off: bool,
    extra: &[&str],
) -> (serde_json::Value, serde_json::Value) {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    init_repo(&root, phase);
    fs::write(root.join("devflow.toml"), TRIAGE_ENABLED).unwrap();
    let context = common::write_phase_context(&root, phase);
    git(&root, &["add", "devflow.toml", &context]);
    git(&root, &["commit", "-q", "-m", "triage on"]);
    let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.clone());
    state.verification_baseline_captured = true;
    state.stage = devflow_core::stage::Stage::Code;
    state.stopped = true;
    state.triage_off = triage_off;
    devflow_core::workflow::save_state(&state).unwrap();
    let state_path = devflow_core::workflow::state_path(&root, phase);
    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&state_path).unwrap()).unwrap();
    let started_at = saved["started_at"].clone();

    let scratch = tempfile::tempdir().unwrap();
    let marker = scratch.path().join("launched-state.json");
    let fake = common::fake_bin_dir_for("codex", &state_copying_codex(&state_path, &marker));
    let _reaper = common::LaunchedRunReaper::for_home(&home, &root, phase);
    // The stub needs `cp`, `mv` and `sleep`, so the host PATH follows it.
    let host_path = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake.path.display(), host_path.to_string_lossy());
    let output = common::hermetic_devflow(&home)
        .args(["resume", "--phase", &phase.to_string()])
        .args(extra)
        .arg(&root)
        .current_dir(&root)
        .hermetic_path(path)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "30")
        .output()
        .expect("run devflow resume");
    assert_success(&output, "the resume");
    common::wait_for(
        || marker.exists(),
        Duration::from_secs(30),
        "the relaunched stage's agent",
    );
    let launched = serde_json::from_str(&fs::read_to_string(&marker).unwrap()).unwrap();
    (launched, started_at)
}

/// D-09: `resume --no-triage` turns triage off for the resumed run, the same
/// run (its `started_at`), in the state the relaunched stage runs with.
#[test]
fn resume_with_no_triage_turns_triage_off_for_the_run() {
    let (launched, started_at) = resumed_stage_state(PhaseId::new(5484), false, &["--no-triage"]);
    assert_eq!(
        launched["triage_off"], true,
        "the resumed stage runs with triage off: {launched}"
    );
    assert_eq!(
        launched["started_at"], started_at,
        "the resume keeps the run: {launched}"
    );
}

/// Control: a resume without the flag keeps a run's triage off as it was.
#[test]
fn resume_without_the_flag_keeps_the_runs_setting() {
    let (launched, _) = resumed_stage_state(PhaseId::new(5485), true, &[]);
    assert_eq!(
        launched["triage_off"], true,
        "a plain resume keeps the run's triage off: {launched}"
    );
}
