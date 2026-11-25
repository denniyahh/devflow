//! D-09 (Phase 54): `--no-triage` turns decision triage off for one run, on
//! `devflow start` and `devflow resume`, and is refused where it would change
//! nothing.
//!
//! The `start_*` cases are real launches that read what `start` saved in
//! `state-NN.json`. Every `devflow` child is built by `common::hermetic_devflow`
//! with its `PATH` set on the child, and the only agent on it is a stub, so no
//! case can reach a real agent.

mod common;

use common::HermeticPath;
use devflow_core::phase_id::PhaseId;
use std::fs;
use std::path::Path;
use std::process::Output;

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
