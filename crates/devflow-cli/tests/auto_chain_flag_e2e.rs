//! End-to-end proof of the GSD chain-flag guard (35.1-01, D-01/D-06).
//!
//! DevFlow spawns Claude to run GSD slash commands, and GSD's
//! `checkpoint_handling` only auto-approves an ordinary `gate="blocking"`
//! checkpoint when `.planning/config.json` reads
//! `workflow._auto_chain_active: true`. This suite proves the whole mechanism
//! on one real path: a `Stage::Code`, `Mode::Auto`, pipe-owning-monitor launch
//! of the real `devflow __monitor` binary.
//!
//! **The load-bearing measurement is the CHILD's own reading of the config
//! file while it runs**, written to a file outside the repository. Reading the
//! config from the test process after the monitor returns would pass even with
//! the guard scoped to the wrong stack frame — that is RESEARCH Pitfall 2, and
//! it is the entire reason this is an end-to-end test rather than a unit test
//! of the writer.

use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::workflow::save_state;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

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

/// The supervised child: FIRST record the value of
/// `workflow._auto_chain_active` visible in the config file this process can
/// see (its cwd is the monitor's `--workdir`), THEN print the success marker.
///
/// The observation file is written OUTSIDE the repository so it survives any
/// git operation the pipeline performs, and so it can never be confused with
/// the config file itself.
///
/// `MISSING` is emitted when the key is absent. That distinction matters: a
/// `.get()`-style silent default would render an absent key as `false` and make
/// a broken fixture indistinguishable from a correct negative observation.
fn observer_script(obs_path: &Path) -> String {
    format!(
        "#!/bin/sh\n\
         line=$(grep '\"_auto_chain_active\"' .planning/config.json 2>/dev/null | head -n1)\n\
         if [ -z \"$line\" ]; then\n\
         \x20 printf 'MISSING' > '{obs}'\n\
         else\n\
         \x20 printf '%s' \"$line\" | sed 's/.*: *//; s/[,[:space:]]*$//' > '{obs}'\n\
         fi\n\
         printf 'DEVFLOW_RESULT: {{\"status\":\"success\"}}\\n'\n",
        obs = obs_path.display()
    )
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

struct Fixture {
    _repo: tempfile::TempDir,
    _aux: tempfile::TempDir,
    root: PathBuf,
    phase: PhaseId,
    config: PathBuf,
    observation: PathBuf,
    prompt_file: PathBuf,
    script: PathBuf,
}

impl Fixture {
    /// Build a Code-stage repo whose persisted state is ready for a
    /// `__monitor` run in `mode`, with the config flag seeded to
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

        let mut state = State::new(phase, AgentKind::Claude, mode, root.clone());
        state.stage = Stage::Code;
        // Halt at the Code→Validate boundary instead of launching another
        // agent: `transition`'s `stop_until == Some(from)` branch stops
        // cleanly once the just-completed stage is the requested stop point.
        state.stop_until = Some(Stage::Code);
        state.stopped = false;
        save_state(&state).unwrap();

        let observation = aux.path().join("child-observation.txt");
        let script = aux.path().join("agent.sh");
        write_executable(&script, &observer_script(&observation));

        let prompt_file = aux.path().join("prompt.txt");
        fs::write(&prompt_file, "run the code stage\n").unwrap();

        Self {
            _repo: repo,
            _aux: aux,
            root,
            phase,
            config,
            observation,
            prompt_file,
            script,
        }
    }

    /// Run the REAL binary's hidden `__monitor` subcommand over `argv`.
    fn run_monitor(&self, argv: &[&str]) -> Output {
        Command::new(devflow_bin())
            .arg("__monitor")
            .arg("--project")
            .arg(&self.root)
            .arg("--phase")
            .arg(self.phase.to_string())
            .arg("--workdir")
            .arg(&self.root)
            .arg("--prompt-file")
            .arg(&self.prompt_file)
            .arg("--idle-timeout-secs")
            .arg("30")
            .arg("--")
            .args(argv)
            .output()
            .expect("spawn devflow __monitor")
    }

    fn run_observer(&self) -> Output {
        let script = self.script.to_str().unwrap().to_string();
        self.run_monitor(&["sh", &script])
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
}

/// The positive arm. The child — a real process supervised by a real
/// `devflow __monitor` — reports having seen the chain flag SET while it ran,
/// and the flag is clear again once that monitor process returns.
#[test]
fn auto_mode_code_stage_child_observes_the_flag_set() {
    let fixture = Fixture::new(Mode::Auto, false);

    let output = fixture.run_observer();
    assert!(
        output.status.success(),
        "monitor exited {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

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
        "the guard must clear the flag when the monitor process returns"
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
    let before = fs::read(&fixture.config).unwrap();

    let output = fixture.run_observer();
    assert!(
        output.status.success(),
        "monitor exited {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

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

/// The in-process half of criterion 2: the flag is cleared even when the run
/// dies before `advance` is ever reached — `run_pipe_owning_monitor`'s
/// early-return `Err` path, driven here by a child program that cannot be
/// spawned at all. (`35.1-02` owns the kill half.)
///
/// The fixture seeds the flag TRUE so this assertion discriminates. Seeded
/// `false`, "reads false afterwards" would also be satisfied by a guard that
/// never ran at all.
#[test]
fn guard_clears_the_flag_when_the_supervised_child_fails() {
    let fixture = Fixture::new(Mode::Auto, true);

    let output = fixture.run_monitor(&["definitely-not-a-real-program-35-1-01"]);
    assert!(
        !output.status.success(),
        "a child that cannot be spawned must fail the monitor, not pass silently"
    );

    assert_eq!(
        fixture.flag_now(),
        "false",
        "the guard's Drop must clear the flag on the error exit path too, not \
         only on the path that reaches advance()"
    );
}
