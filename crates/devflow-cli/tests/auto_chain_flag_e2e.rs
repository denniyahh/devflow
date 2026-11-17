//! End-to-end proof of the GSD chain-flag guard (35.1-01, D-01/D-06).
//!
//! DevFlow spawns Claude to run GSD slash commands, and GSD's
//! `checkpoint_handling` only auto-approves an ordinary `gate="blocking"`
//! checkpoint when `.planning/config.json` reads
//! `workflow._auto_chain_active: true`. This suite proves the whole mechanism
//! on one real path: a `Stage::Code`, `Mode::Auto`, pipe-owning (stream) launch
//! by the real `devflow __runner` binary, started directly over a saved state.
//!
//! **The load-bearing measurement is the CHILD's own reading of the config
//! file while it runs**, written to a file outside the repository. Reading the
//! config from the test process after the runner returns would pass even with
//! the guard scoped to the wrong stack frame — that is RESEARCH Pitfall 2, and
//! it is the entire reason this is an end-to-end test rather than a unit test
//! of the writer.
//!
//! Every `devflow` child is built by `common::hermetic_devflow`, so the
//! runner's registry entries land in the fixture's own tempdir `HOME`, never
//! in the operator's real `~/.cache/devflow` (999.141).

mod common;

use common::HermeticPath;
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::workflow::save_state;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

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

/// Init a temp repo with `develop` and a `feature/phase-NN` branch holding one
/// commit ahead of `develop` — the commit is what lets Layer 2 classify the
/// run `Success` rather than `Failed — no work done`.
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

/// A `.planning/config.json` mirroring this project's real file shape — every
/// top-level key the operator owns, plus the nested integer and
/// `workflow.auto_advance`, so a write that re-renders or drops anything shows
/// up here rather than only in production.
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

/// The Claude top-level result line the observer prints after its observation.
const SUCCESS_RESULT_LINE: &str = r#"{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}"#;

/// The body of the fake stream-shape `claude` the runner launches for Code: it
/// answers `--version`, reads its one stdin turn, FIRST records the value of
/// `workflow._auto_chain_active` visible in the config file this process can see
/// (its cwd is the run's project root), THEN prints a successful result line.
///
/// The observation file is written OUTSIDE the repository so it survives any
/// git operation the pipeline performs, and so it can never be confused with
/// the config file itself.
///
/// `MISSING` is emitted when the key is absent. That distinction matters: a
/// `.get()`-style silent default would render an absent key as `false` and make
/// a broken fixture indistinguishable from a correct negative observation.
fn observer_body(obs_path: &Path) -> String {
    format!(
        "if [ \"$1\" = \"--version\" ]; then echo \"2.1.220 (Claude Code)\"; exit 0; fi\n\
         IFS= read -r turn\n\
         line=$(grep '\"_auto_chain_active\"' .planning/config.json 2>/dev/null | head -n1)\n\
         if [ -z \"$line\" ]; then\n\
         \x20 printf 'MISSING' > '{obs}'\n\
         else\n\
         \x20 printf '%s' \"$line\" | sed 's/.*: *//; s/[,[:space:]]*$//' > '{obs}'\n\
         fi\n\
         printf '%s\\n' '{SUCCESS_RESULT_LINE}'\n\
         exit 0",
        obs = obs_path.display()
    )
}

/// A fake `claude` the runner finds on its PATH but cannot execute: its
/// interpreter does not exist, so its exec fails and the agent command exits 127.
const UNSPAWNABLE_CLAUDE: &str = "#!/definitely-not-a-real-interpreter-35-1-01\n";

/// How long a test waits for a runner halting at the Code boundary to exit.
const RUNNER_EXIT_WAIT: Duration = Duration::from_secs(60);

/// Owns a spawned `devflow __runner` so a panicking assertion never leaks it:
/// dropped, it SIGKILLs and reaps the runner if it still runs.
struct RunnerChild(Child);

impl Drop for RunnerChild {
    fn drop(&mut self) {
        common::kill_and_reap(&mut self.0);
    }
}

struct Fixture {
    _repo: tempfile::TempDir,
    _aux: tempfile::TempDir,
    root: PathBuf,
    phase: PhaseId,
    config: PathBuf,
    observation: PathBuf,
    /// Every `devflow` child's `HOME` and cache. Declared last so it drops
    /// after the rest of the fixture.
    home: common::HermeticHome,
}

impl Fixture {
    /// Build a Code-stage repo whose persisted state is ready for a
    /// `__runner` run in `mode`, with the config flag seeded to
    /// `flag_before`.
    fn new(mode: Mode, flag_before: bool) -> Self {
        let repo = tempfile::tempdir().unwrap();
        let aux = tempfile::tempdir().unwrap();
        // The binary canonicalizes `--project`, so the fixture must too or the
        // state file it writes is looked up under a different path.
        let root = repo.path().canonicalize().unwrap();
        let phase = PhaseId::new(77);
        init_repo(&root, phase);

        let planning = root.join(".planning");
        fs::create_dir_all(&planning).unwrap();
        let config = planning.join("config.json");
        fs::write(&config, real_shape_config(flag_before)).unwrap();
        // One plan with no checkpoint, committed on the phase branch: what an
        // unattended Code launch's preflight reads. Without it an auto-mode Code
        // launch stops at its preflight gate and no agent starts.
        let padded = phase.padded();
        let plan = format!(".planning/phases/{padded}-fixture/{padded}-01-PLAN.md");
        fs::create_dir_all(root.join(format!(".planning/phases/{padded}-fixture"))).unwrap();
        fs::write(
            root.join(&plan),
            format!(
                "---\nphase: {padded}-fixture\nplan: 01\ntype: execute\nautonomous: true\n---\n\n<tasks>\n<task type=\"auto\"><name>stub</name></task>\n</tasks>\n"
            ),
        )
        .unwrap();
        git(&root, &["add", &plan]);
        git(&root, &["commit", "-q", "-m", "plan"]);

        let mut state = State::new(phase, AgentKind::Claude, mode, root.clone());
        state.stage = Stage::Code;
        // Halt at the Code→Validate boundary instead of launching another
        // agent: `transition`'s `stop_until == Some(from)` branch stops
        // cleanly once the just-completed stage is the requested stop point.
        state.stop_until = Some(Stage::Code);
        state.stopped = false;
        // A recorded, confirmed delivery canary: the Code launch runs no canary of
        // its own, so the fake's one turn is the Code stage's.
        state.canary = Some(devflow_core::canary::CanaryOutcome::Confirmed);
        save_state(&state).unwrap();

        let observation = aux.path().join("child-observation.txt");

        Self {
            _repo: repo,
            _aux: aux,
            root,
            phase,
            config,
            observation,
            home: common::HermeticHome::new(),
        }
    }

    /// The REAL binary's hidden `__runner` subcommand over the saved state, with
    /// `agent_bin` first on its PATH so the Code agent it launches is that dir's
    /// fake `claude`.
    fn runner_command(&self, agent_bin: &common::FakeBin) -> Command {
        let existing = std::env::var_os("PATH").unwrap_or_default();
        let mut command = common::hermetic_devflow(&self.home);
        command
            .hermetic_path(format!(
                "{}:{}",
                agent_bin.path.display(),
                existing.to_string_lossy()
            ))
            .env_remove("NOTIFY_SOCKET")
            .current_dir(&self.root)
            .arg("__runner")
            .arg("--project")
            .arg(&self.root)
            .arg("--phase")
            .arg(self.phase.to_string())
            .stdin(std::process::Stdio::null());
        command
    }

    /// Run the observer as the Code agent and wait, bounded by
    /// [`RUNNER_EXIT_WAIT`], for the runner to exit. A runner still running then
    /// fails the test with its stderr and the run's events.
    fn run_observer(&self) -> std::process::ExitStatus {
        let agent_bin = common::fake_bin_dir(&observer_body(&self.observation));
        let stderr = self._aux.path().join("runner.stderr");
        let mut runner = RunnerChild(
            self.runner_command(&agent_bin)
                .stdout(std::process::Stdio::null())
                .stderr(fs::File::create(&stderr).expect("create the runner's stderr log"))
                .spawn()
                .expect("spawn devflow __runner"),
        );
        let mut status = None;
        let waited = std::time::Instant::now();
        while status.is_none() && waited.elapsed() < RUNNER_EXIT_WAIT {
            status = runner.0.try_wait().expect("poll the runner");
            std::thread::sleep(Duration::from_millis(25));
        }
        status.unwrap_or_else(|| {
            panic!(
                "the runner did not exit within {RUNNER_EXIT_WAIT:?}\nstderr:\n{}\nevents:\n{}",
                fs::read_to_string(&stderr).unwrap_or_default(),
                fs::read_to_string(devflow_core::events::events_path(&self.root))
                    .unwrap_or_default()
            )
        })
    }

    fn child_observed(&self) -> String {
        fs::read_to_string(&self.observation).unwrap_or_else(|err| {
            panic!(
                "the supervised child never wrote its observation to {}: {err}",
                self.observation.display()
            )
        })
    }

    fn flag_now(&self) -> String {
        let raw = fs::read_to_string(&self.config).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        // Indexing, not `.get()`: an absent key must raise here rather than
        // quietly render as `false` and agree with a correct negative result.
        value["workflow"]["_auto_chain_active"].to_string()
    }

    /// The `event` lines in the run's event log, parsed.
    fn events_named(&self, event: &str) -> Vec<serde_json::Value> {
        let Ok(text) = fs::read_to_string(devflow_core::events::events_path(&self.root)) else {
            return Vec::new();
        };
        text.lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|line| line["event"] == event)
            .collect()
    }
}

/// The positive arm. The child — a real process the real `devflow __runner`
/// launched for Code — reports having seen the chain flag SET while it ran, and
/// the flag is clear again once that runner returns.
#[test]
fn auto_mode_code_stage_child_observes_the_flag_set() {
    let fixture = Fixture::new(Mode::Auto, false);
    let _run_reaper =
        common::LaunchedRunReaper::for_home(&fixture.home, &fixture.root, fixture.phase);

    // The runner halts at the Code boundary (`stop_until`) and exits 0.
    let status = fixture.run_observer();
    assert!(status.success(), "runner exited {status:?}");

    assert_eq!(
        fixture.child_observed(),
        "true",
        "the supervised child must see workflow._auto_chain_active set WHILE it \
         runs — an after-the-fact read by this test process would pass even with \
         the guard scoped to the wrong frame (RESEARCH Pitfall 2)"
    );
    assert_eq!(
        fixture.flag_now(),
        "false",
        "the guard must clear the flag when the runner's stage returns"
    );
}

/// NEGATIVE CONTROL. Identical fixture, `Mode::Supervise`.
///
/// If this test and `auto_mode_code_stage_child_observes_the_flag_set` observe
/// the SAME value, the measurement is broken and neither result means anything
/// — no conclusion about the guard may be drawn from either. The two
/// observations disagreeing is what makes the positive arm evidence rather
/// than a value the fixture was always going to produce.
///
/// Also pins F-3's no-op property: an ineligible launch asserts `false`, and
/// because the file already holds `false` the writer must not rewrite it — the
/// config's bytes are unchanged, so an ineligible stage launch never dirties a
/// tracked file.
#[test]
fn supervise_mode_code_stage_child_observes_the_flag_clear() {
    let fixture = Fixture::new(Mode::Supervise, false);
    let _run_reaper =
        common::LaunchedRunReaper::for_home(&fixture.home, &fixture.root, fixture.phase);
    let before = fs::read(&fixture.config).unwrap();

    // The runner halts at the Code boundary (`stop_until`) and exits 0.
    let status = fixture.run_observer();
    assert!(status.success(), "runner exited {status:?}");

    assert_eq!(
        fixture.child_observed(),
        "false",
        "a run the operator chose to SUPERVISE must never get checkpoint \
         auto-approval — the child must see the flag clear while it runs"
    );

    let after = fs::read(&fixture.config).unwrap();
    assert_eq!(
        before, after,
        "an ineligible launch must be a genuine no-op on an already-false file \
         (F-3), not a rewrite of a tracked file on every stage launch"
    );
}

/// The in-process half of criterion 2: the flag is cleared even when the agent
/// never runs — here a `claude` whose interpreter does not exist, so its exec
/// fails (exit 127) and the runner fails the Code stage as `agent_unavailable` and
/// waits at its stage-failure gate. The guard drops when the stage returns, before
/// that advance. (`35.1-02` owns the kill half.)
///
/// The fixture seeds the flag TRUE so this assertion discriminates. Seeded
/// `false`, "reads false afterwards" would also be satisfied by a guard that
/// never ran at all.
#[test]
fn guard_clears_the_flag_when_the_supervised_child_fails() {
    let fixture = Fixture::new(Mode::Auto, true);
    let _run_reaper =
        common::LaunchedRunReaper::for_home(&fixture.home, &fixture.root, fixture.phase);
    let agent_bin = common::fake_bin_dir("");
    fs::write(agent_bin.path.join("claude"), UNSPAWNABLE_CLAUDE).unwrap();
    let stderr = fixture._aux.path().join("runner.stderr");

    // The failed stage opens a gate the runner then waits on, so the test reads
    // the flag once that gate has fired and ends the runner itself.
    let _runner = RunnerChild(
        fixture
            .runner_command(&agent_bin)
            .stdout(std::process::Stdio::null())
            .stderr(fs::File::create(&stderr).expect("create the runner's stderr log"))
            .spawn()
            .expect("spawn devflow __runner"),
    );
    let waited = std::time::Instant::now();
    while fixture.events_named("gate_fired").is_empty() {
        assert!(
            waited.elapsed() < RUNNER_EXIT_WAIT,
            "the runner opened no gate for the failed Code agent within {RUNNER_EXIT_WAIT:?}\nstderr:\n{}\n\
             events:\n{}",
            fs::read_to_string(&stderr).unwrap_or_default(),
            fs::read_to_string(devflow_core::events::events_path(&fixture.root))
                .unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    let evaluated = fixture.events_named("advance_evaluated");
    assert_eq!(
        evaluated.len(),
        1,
        "the failed Code stage is evaluated once: {evaluated:?}"
    );
    assert_eq!(
        evaluated[0]["status"], "agent_unavailable",
        "a child that cannot be executed must fail the stage as unavailable: {evaluated:?}"
    );

    assert_eq!(
        fixture.flag_now(),
        "false",
        "the guard's Drop must clear the flag on the error exit path too, not \
         only on the path that reaches advance()"
    );
}
