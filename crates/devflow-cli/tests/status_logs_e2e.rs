//! End-to-end proof that `devflow status` and `devflow logs -f` read the one liveness
//! answer (D-09, D-09a, D-10): a run launched as a runner unit through the home's fake
//! `systemd-run` shows as live naming `stop` and `logs`, every phase found from its
//! state, runner-lock and gate files gets one row, and `logs -f` ends with the run's
//! state line.
//!
//! `commands` is `pub(crate)` inside the `devflow` binary crate, so these drive the
//! compiled binary, like every other devflow-cli integration test.

mod common;

use common::HermeticPath;
use devflow_core::phase_id::PhaseId;
use std::path::{Path, PathBuf};

/// Hermetic `git` invocation for fixture setup (999.37).
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

/// A repository on `develop` with a roadmap entry for `phase`.
fn init_repo(root: &Path, phase: PhaseId) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    std::fs::create_dir_all(root.join(".planning")).unwrap();
    std::fs::write(root.join("README.md"), "base\n").unwrap();
    std::fs::write(
        root.join(".planning/ROADMAP.md"),
        format!("# Roadmap\n\n### Phase {phase}: status fixture\n"),
    )
    .unwrap();
    git(root, &["add", "README.md", ".planning/ROADMAP.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
}

/// The fake `claude`'s health check and delivery canary; `body` runs for the stage turn.
fn fake_claude(body: &str) -> String {
    format!(
        r#"if [ "$1" = "--version" ]; then echo "2.1.220 (Claude Code)"; exit 0; fi
read -r turn
case "$turn" in
  *DEVFLOW_DELIVERY_CANARY_*)
    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
    printf '{{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"%s"}}\n' "$token"
    exit 0
    ;;
esac
{body}"#
    )
}

/// A stage turn that keeps running until `trigger` exists, then runs `released`.
fn triggered_agent(trigger: &Path, released: &str) -> String {
    fake_claude(&format!(
        "while [ ! -e '{trigger}' ]; do sleep 0.05; done\n{released}",
        trigger = trigger.display()
    ))
}

/// The phase's unit name as the binary derives it (`service::unit_name`).
fn unit_name(root: &Path, phase: PhaseId) -> String {
    format!(
        "devflow-{:016x}-{}.service",
        devflow_core::registry::path_digest(root),
        phase.padded()
    )
}

/// One run launched by `devflow start --mode supervise --until code --no-worktree` as a
/// runner unit through the home's fake `systemd-run`, whose agent waits for its
/// trigger file. The reaper is declared first so it drops before the home and repo.
struct Rig {
    _reaper: common::LaunchedRunReaper,
    home: common::HermeticHome,
    _repo: tempfile::TempDir,
    _fake_bin: common::FakeBin,
    root: PathBuf,
    phase: PhaseId,
    /// The runner the fake `systemd-run` recorded for the phase's unit.
    runner_pid: u32,
}

impl Rig {
    /// Launch a run whose agent runs `released` once [`Self::release`] is called.
    fn launch(phase: PhaseId, released: &str) -> Self {
        let home = common::HermeticHome::new();
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path().canonicalize().unwrap();
        let fake_bin = common::fake_bin_dir(&triggered_agent(&Self::trigger_path(&root), released));
        let reaper = common::LaunchedRunReaper::for_home(&home, &root, phase);
        init_repo(&root, phase);
        let existing = std::env::var_os("PATH").unwrap_or_default();
        let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());
        let output = common::hermetic_devflow(&home)
            .args(["start", "--phase", &phase.to_string()])
            .args([
                "--agent",
                "claude",
                "--mode",
                "supervise",
                "--until",
                "code",
            ])
            .arg("--no-worktree")
            .arg(&root)
            .current_dir(&root)
            .hermetic_path(path)
            .output()
            .expect("run devflow start");
        assert!(
            output.status.success(),
            "start failed: {}",
            describe(&output)
        );
        let runner_pid = std::fs::read_to_string(
            home.service_bin()
                .join(format!("{}.pid", unit_name(&root, phase))),
        )
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .expect("start launched a runner through the fake systemd-run");
        Self {
            _reaper: reaper,
            home,
            _repo: repo,
            _fake_bin: fake_bin,
            root,
            phase,
            runner_pid,
        }
    }

    fn trigger_path(root: &Path) -> PathBuf {
        root.join(".git").join("devflow-test-trigger")
    }

    /// Let the agent's stage turn run its `released` body.
    fn release(&self) {
        std::fs::write(Self::trigger_path(&self.root), "").unwrap();
    }

    /// `devflow status` over the rig's root.
    fn status(&self) -> std::process::Output {
        status_of(&self.home, &self.root)
    }
}

/// `devflow status ROOT` under `home`.
fn status_of(home: &common::HermeticHome, root: &Path) -> std::process::Output {
    common::hermetic_devflow(home)
        .arg("status")
        .arg(root)
        .current_dir(root)
        .output()
        .expect("run devflow status")
}

/// A command's exit code, stdout and stderr, for an assertion message.
fn describe(output: &std::process::Output) -> String {
    format!(
        "exit {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The line `status` prints for `phase`'s row: `phase N: <word> …`.
fn phase_row(stdout: &str, phase: PhaseId) -> Option<String> {
    let head = format!("phase {phase}: ");
    stdout
        .lines()
        .find(|line| line.starts_with(&head))
        .map(str::to_string)
}

/// D-09: a run whose runner holds its lock shows as live, naming `stop` and `logs`, and
/// `status` prints no agent pid line.
#[test]
fn status_shows_a_running_unit_as_live_naming_stop_and_logs() {
    let rig = Rig::launch(PhaseId::new(181), "exit 0");
    let phase = rig.phase;
    assert!(
        common::runs_the_runner_verb(rig.runner_pid),
        "the run's runner {} is running",
        rig.runner_pid
    );

    let output = rig.status();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "status failed: {}",
        describe(&output)
    );
    let row = phase_row(&stdout, phase).unwrap_or_default();
    assert!(
        row.starts_with(&format!("phase {phase}: live")),
        "status did not say live: {}",
        describe(&output)
    );
    assert!(
        row.contains(&format!("devflow stop --phase {phase}"))
            && row.contains(&format!("devflow logs --phase {phase} -f")),
        "the live row names stop and logs: {row}"
    );
    assert!(
        !stdout.contains("agent_pid"),
        "agent pid line still printed: {stdout}"
    );
    assert!(
        !stdout.contains("supervisor:"),
        "supervisor probe line still printed: {stdout}"
    );
}
