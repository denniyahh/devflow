//! End-to-end proof for `devflow stop` (23c, the missing primitive
//! `23-ORPHAN-FORENSICS.md` names as the reason 54 orphaned processes
//! accumulated with no remedy but `kill(1)`): since 52.2-06, stop stops the
//! phase's user unit, and the run's runner records the stop itself.
//!
//! `commands::stop` is `pub(crate)` inside the `devflow` BINARY crate (no
//! `lib.rs`, so integration tests cannot link against it at all), so — like
//! every other devflow-cli integration test (`phase7_cli.rs`,
//! `gate_sweep_e2e.rs`) — this drives the real compiled binary via
//! `Command::new`, not an in-process call.

mod common;

use common::HermeticPath;
use devflow_core::gates::{GateResponse, Gates};
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::test_support::{answer_open_gate, gate_answer_files};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// Hermetic `git` invocation for fixture setup (999.37) — never a bare
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

/// Finding E: an answer is evidence of nothing unless a process will act on it.
/// With no registered waiter, `gate approve` writes no response, names the repair
/// and never claims that the workflow will advance.
#[test]
fn gate_approve_with_no_registered_waiter_does_not_claim_advance() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(110);
    Gates::write_gate(
        root,
        phase,
        Stage::Validate,
        devflow_core::gates::GateKind::StageFailure,
        "approve the finish",
    )
    .unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["gate", "approve", &phase.to_string(), "--project"])
        .arg(root)
        .output()
        .expect("run devflow gate approve");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(gate_answer_files(root, phase, Stage::Validate).is_empty());
    assert!(
        !stdout.contains("workflow will advance") && !stderr.contains("workflow will advance"),
        "a no-waiter refusal must not claim an outcome: {stdout}{stderr}"
    );
    assert!(
        stderr.contains("no registered waiter") && stderr.contains("no response was written"),
        "a no-waiter refusal must name the evidence limit: {stderr}"
    );
}

/// D-05: `stop` on a live run stops the phase's unit; its runner records `stopped` and
/// its stop reason, and `stop` reports that reason and exits 0.
#[test]
fn stop_marks_state_stopped_and_records_reason() {
    let run = LiveRun::start(PhaseId::new(96));

    let output = run.stop(&[]);
    let text = describe(&output);
    assert!(output.status.success(), "devflow stop failed: {text}");
    assert!(
        text.contains(&format!(
            "stop: phase {} stopped ({RUNNER_SIGTERM_REASON})",
            run.phase
        )),
        "stop reports the recorded stop: {text}"
    );
    assert!(!run.alive(), "the unit's runner is gone after stop: {text}");
    let reloaded = run.state();
    assert!(reloaded.stopped, "the runner must record stopped=true");
    assert_eq!(
        reloaded.stop_reason.as_deref(),
        Some(RUNNER_SIGTERM_REASON),
        "the runner records its stop reason"
    );
}

/// Task 1 behavior: `stop_until` means something different (the requested
/// halt stage for `devflow start --until`) and must be left untouched.
#[test]
fn stop_leaves_stop_until_unchanged() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(98);

    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stop_until = Some(Stage::Plan);
    devflow_core::workflow::save_state(&state).unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(output.status.success());

    let reloaded = devflow_core::workflow::load_state(root, phase).unwrap();
    assert_eq!(reloaded.stop_until, Some(Stage::Plan));
}

/// CLI discoverability: BOTH ways of naming the target must be documented in
/// the subcommand's own `--help` — the `--phase` flag, and (phase 46 / D-11)
/// the `[PROJECT]` positional root.
///
/// The name of this function is cited verbatim as the VALID-02 acceptance
/// command in `46-RESEARCH.md:886` and in `46-VALIDATION.md`; it is widened in
/// place rather than renamed, because a rename silently invalidates the
/// verification map.
#[test]
fn stop_help_documents_phase_flag() {
    let home = common::HermeticHome::new();
    let output = common::hermetic_devflow(&home)
        .args(["stop", "--help"])
        .output()
        .expect("run devflow stop --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--phase"),
        "--help missing --phase:\n{stdout}"
    );
    // The BRACKETED form specifically: the bare word "project" already occurs
    // case-insensitively in the long flag's own description text, so only
    // `[PROJECT]` unambiguously indicates the positional was added.
    assert!(
        stdout.contains("[PROJECT]"),
        "--help missing the [PROJECT] positional:\n{stdout}"
    );
}

/// The id `gate list` prints is the id `gate approve --gate-id` takes, and an id
/// for a gate that has since been replaced answers nothing. The approval that
/// follows with the listed id is the control: the refusal was the id comparison,
/// not a missing waiter.
#[test]
fn gate_approve_takes_the_id_gate_list_shows_and_refuses_a_replaced_one() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(111);
    let g1 = Gates::open_gate(
        root,
        phase,
        Stage::Validate,
        devflow_core::gates::GateKind::StageFailure,
        "first",
    )
    .unwrap()
    .gate_id;
    Gates::close(root, phase, Stage::Validate).unwrap();
    let (opened, registration) = Gates::open_registered(
        root,
        phase,
        Stage::Validate,
        devflow_core::gates::GateKind::StageFailure,
        "second",
    )
    .unwrap();
    let g2 = opened.gate_id;
    let approve_with = |gate_id: &str| {
        common::hermetic_devflow(&home)
            .args(["gate", "approve", &phase.to_string(), "--gate-id", gate_id])
            .arg("--project")
            .arg(root)
            .output()
            .expect("run devflow gate approve")
    };

    let list = common::hermetic_devflow(&home)
        .args(["gate", "list"])
        .arg(root)
        .output()
        .expect("run devflow gate list");
    let listed = String::from_utf8_lossy(&list.stdout);
    assert!(listed.contains(g2.as_str()), "{listed}");

    let refused = approve_with(g1.as_str());
    let refusal = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success(), "a replaced id must be refused");
    assert!(
        refusal.contains(g1.as_str()) && refusal.contains(g2.as_str()),
        "the refusal names both gates: {refusal}"
    );
    assert!(gate_answer_files(root, phase, Stage::Validate).is_empty());

    let answered = approve_with(g2.as_str());
    assert!(
        answered.status.success(),
        "{}",
        String::from_utf8_lossy(&answered.stderr)
    );
    assert_eq!(gate_answer_files(root, phase, Stage::Validate).len(), 1);
    registration.finish();
}

/// Task 3 behavior: a response written by hand before `stop` ever runs (a
/// human beat the operator to it) must survive byte-identical — `stop` is a
/// success no-op here, not a clobber.
#[test]
fn stop_against_a_hand_written_response_is_a_success_no_op() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(100);

    Gates::write_gate(
        root,
        phase,
        Stage::Validate,
        devflow_core::gates::GateKind::StageFailure,
        "approve the finish?",
    )
    .unwrap();
    let response_path = answer_open_gate(
        root,
        phase,
        Stage::Validate,
        &GateResponse {
            approved: false,
            abort: false,
            note: Some("hand-written rejection".into()),
            responded_by: Some("human".into()),
        },
    )
    .unwrap();
    let before = std::fs::read(&response_path).unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(
        output.status.success(),
        "devflow stop failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = std::fs::read(&response_path).unwrap();
    assert_eq!(
        before, after,
        "stop must not clobber an existing hand-written response"
    );
}

/// Task 3 behavior: a root with no persisted state at all — a phase that
/// was never started, or whose state was already cleared — is a success,
/// not an error.
#[test]
fn stop_against_a_root_with_no_state_is_a_success() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(101);

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(
        output.status.success(),
        "stop against a root with no state must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Task 3's composition proof (23-CONTEXT.md's Integration Points, T-23-54):
/// stop makes a phase not-live; `cleanup`'s existing `stopped && !force`
/// refusal (20c/CR-02) must still decline it, and `cleanup --force` must
/// then succeed — the two verbs compose in that order, and `cleanup`'s
/// fail-closed guarantee is provably unweakened by this plan.
#[test]
fn stop_then_cleanup_composes_refuse_then_force() {
    let run = LiveRun::start_in_a_worktree(PhaseId::new(102));
    let wt_path = run
        .state()
        .worktree_path
        .expect("the run used a phase worktree");
    assert!(wt_path.is_dir(), "the run's worktree exists");

    let stopped = run.stop(&[]);
    assert!(
        stopped.status.success(),
        "devflow stop failed: {}",
        describe(&stopped)
    );
    assert!(
        run.state().stopped,
        "stop must have marked the phase stopped before the composition check runs"
    );

    // cleanup without --force must still decline — the fail-closed
    // guarantee this plan's design depends on and does not weaken.
    let refused = common::hermetic_devflow(&run.home)
        .args(["cleanup"])
        .arg(&run.root)
        .output()
        .expect("run devflow cleanup");
    assert!(
        refused.status.success(),
        "cleanup on a stopped phase must not error, only skip: {}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(
        wt_path.is_dir(),
        "cleanup without --force must not remove a stop-marked phase's worktree"
    );

    // cleanup --force is the documented escape hatch — the two verbs
    // compose in that order.
    let forced = common::hermetic_devflow(&run.home)
        .args(["cleanup", "--force"])
        .arg(&run.root)
        .output()
        .expect("run devflow cleanup --force");
    assert!(
        forced.status.success(),
        "cleanup --force must succeed: {}",
        String::from_utf8_lossy(&forced.stderr)
    );
    assert!(
        !wt_path.is_dir(),
        "cleanup --force must remove the worktree after stop"
    );
}

/// Phase 46 / D-11 (VALID-02): `stop` must accept its project root as a
/// POSITIONAL argument, the way `start`, `resume`, `status` and
/// `approve` already do.
///
/// Asserting only the exit code here would not prove the positional was USED:
/// a subcommand that silently ignored it and defaulted to the current
/// directory would also exit zero. So `stop` runs from an unrelated
/// directory, and the run at the named root is asserted to have stopped.
#[test]
fn stop_accepts_the_project_root_as_a_positional_argument() {
    let run = LiveRun::start(PhaseId::new(103));
    let elsewhere = tempfile::tempdir().unwrap();

    let output = common::hermetic_devflow(&run.home)
        .args(["stop", "--phase", &run.phase.to_string()])
        .arg(&run.root)
        .current_dir(elsewhere.path())
        .output()
        .expect("run devflow stop");
    assert!(
        output.status.success(),
        "devflow stop with a positional root failed: {}",
        describe(&output)
    );

    assert!(
        !run.alive(),
        "the positional root's runner must be the one stopped"
    );
    let reloaded = run.state();
    assert!(
        reloaded.stopped,
        "the positional root must be the root that actually gets stopped"
    );
    assert_eq!(
        reloaded.stop_reason.as_deref(),
        Some(RUNNER_SIGTERM_REASON),
        "the stopped run's runner records its stop reason"
    );
}

/// Phase 46 / D-12 (VALID-02): when BOTH spellings are supplied the FLAG
/// wins, and the both-supplied case is NOT an error.
///
/// Proven in BOTH orderings deliberately. A single direction is equally
/// consistent with the POSITIONAL silently winning, so one direction alone
/// does not discriminate between the two hypotheses — it is a negative
/// control for the precedence claim, not a redundant second case.
///
/// Each ordering has its own pair of live runs, one per root, all under one home
/// so `stop` can reach either unit. `stop` runs from an unrelated directory, so
/// neither root is found through the current directory.
#[test]
fn stop_root_flag_takes_precedence_over_the_positional() {
    let home = common::HermeticHome::new();
    let phase = PhaseId::new(104);
    let elsewhere = tempfile::tempdir().unwrap();
    let fake_bin = common::fake_bin_dir(&fake_claude("sleep 120"));
    let stopped = |root: &Path| {
        devflow_core::workflow::load_state(root, phase)
            .expect("state must be loadable")
            .stopped
    };

    for ordering in ["flag A, positional B", "flag B, positional A"] {
        let dirs = [tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()];
        let roots: Vec<PathBuf> = dirs
            .iter()
            .map(|dir| dir.path().canonicalize().unwrap())
            .collect();
        let _reapers: Vec<common::LaunchedRunReaper> = roots
            .iter()
            .map(|root| common::LaunchedRunReaper::for_home(&home, root, phase))
            .collect();
        let runners: Vec<u32> = roots
            .iter()
            .map(|root| launch_unit_run(&home, root, phase, &fake_bin, &["--no-worktree"]))
            .collect();
        let (flag, positional) = if ordering.starts_with("flag A") {
            (0, 1)
        } else {
            (1, 0)
        };

        let output = common::hermetic_devflow(&home)
            .args(["stop", "--phase", &phase.to_string(), "--root"])
            .arg(&roots[flag])
            .arg(&roots[positional])
            .current_dir(elsewhere.path())
            .output()
            .expect("run devflow stop");
        assert!(
            output.status.success(),
            "{ordering}: supplying both spellings must not be an error: {}",
            describe(&output)
        );
        assert!(
            stopped(&roots[flag]) && !common::runs_the_runner_verb(runners[flag]),
            "{ordering}: the flag's root must stop"
        );
        assert!(
            !stopped(&roots[positional]) && common::runs_the_runner_verb(runners[positional]),
            "{ordering}: the positional's root must be left alone"
        );
    }
}

/// Phase 46 / D-13 (VALID-02): a genuinely wrong positional root must fail
/// with `project_root`'s own path-naming message, not with a bare clap usage
/// error that names no offending argument.
///
/// The non-zero-exit assertion ALONE is a FALSE POSITIVE and is not
/// acceptable evidence: the clap usage error this case exists to eliminate
/// also exits non-zero, so a bare exit-code check reads green both before and
/// after the fix. The two stderr assertions are the only halves that
/// discriminate.
#[test]
fn stop_against_a_nonexistent_positional_root_names_the_offending_path() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-such-project-root");
    let phase = PhaseId::new(105);

    assert!(
        !missing.exists(),
        "fixture error: the offending path must NOT exist"
    );

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string()])
        .arg(&missing)
        .output()
        .expect("run devflow stop");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a wrong root must fail:\n{stderr}"
    );
    let missing_display = missing.display().to_string();
    assert!(
        stderr.contains("project path does not exist:"),
        "a wrong root must reach project_root's message, not a clap usage error:\n{stderr}"
    );
    assert!(
        stderr.contains(&missing_display),
        "the message must NAME the offending path ({missing_display}):\n{stderr}"
    );
}

/// Phase 46 / VALID-02 edge "empty": what BOTH spellings do with an empty
/// root, MEASURED against the binary rather than predicted.
///
/// The measurement CORRECTS this plan's own stated expectation. The plan
/// predicted both spellings would reach `project_root` and render
/// `project path does not exist: ` with nothing after the colon. They do not.
/// clap rejects an empty value at the PARSER for both, exit code 2, with its
/// own `a value is required for '<ARG>' but none was supplied`. The two
/// spellings therefore CONVERGE on parser rejection while DIVERGING in which
/// argument the message names — `'--root <ROOT>'` for the flag, `'[PROJECT]'`
/// for the positional. Both name the offending argument, which is what D-13
/// asks for.
///
/// Each half asserts the ABSENCE of `project path does not exist:` as its
/// negative control. Without that, this test would still pass on a build that
/// let the empty value through to a message naming an invisible value — which
/// is the precise legibility failure the plan was worried about.
#[test]
fn stop_with_an_empty_project_root_is_refused_identically_for_both_spellings() {
    let home = common::HermeticHome::new();
    let phase = PhaseId::new(106);

    // Spelling 1 — empty POSITIONAL.
    let positional = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string()])
        .arg("")
        .output()
        .expect("run devflow stop");
    let positional_stderr = String::from_utf8_lossy(&positional.stderr);
    assert!(
        !positional.status.success(),
        "an empty positional root must be refused:\n{positional_stderr}"
    );
    assert!(
        positional_stderr.contains("a value is required for"),
        "the empty positional must be refused by clap:\n{positional_stderr}"
    );
    assert!(
        positional_stderr.contains("[PROJECT]"),
        "the refusal must NAME the positional it rejected:\n{positional_stderr}"
    );
    assert!(
        !positional_stderr.contains("project path does not exist:"),
        "an empty value must never reach project_root:\n{positional_stderr}"
    );

    // Spelling 2 — empty value supplied to the long FLAG.
    let flag = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg("")
        .output()
        .expect("run devflow stop");
    let flag_stderr = String::from_utf8_lossy(&flag.stderr);
    assert!(
        !flag.status.success(),
        "an empty --root value must be refused:\n{flag_stderr}"
    );
    assert!(
        flag_stderr.contains("a value is required for"),
        "the empty flag value must be refused by clap:\n{flag_stderr}"
    );
    assert!(
        flag_stderr.contains("--root"),
        "the refusal must NAME the flag it rejected:\n{flag_stderr}"
    );
    assert!(
        !flag_stderr.contains("project path does not exist:"),
        "an empty value must never reach project_root:\n{flag_stderr}"
    );
}

/// Phase 46 / VALID-02 edge "encoding": the positional is typed `PathBuf`, so
/// clap parses it from the raw `OsString` and a byte-invalid path SURVIVES to
/// `project_root` rather than being rejected as invalid UTF-8. A
/// `String`-typed argument would have failed earlier, with different text.
///
/// No assertion is made on the rendered path bytes: `Path::display` is lossy
/// by design, and asserting on its output would pin an implementation detail
/// of `std` rather than a behaviour DevFlow owns.
#[cfg(unix)]
#[test]
fn stop_positional_root_survives_a_non_utf8_path() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let phase = PhaseId::new(107);

    // A lone 0xFF byte is not valid UTF-8 in any position.
    let mut raw = b"root-".to_vec();
    raw.push(0xFF);
    let name = std::ffi::OsStr::from_bytes(&raw);
    assert!(
        name.to_str().is_none(),
        "fixture error: the name must NOT be valid UTF-8"
    );

    let missing = dir.path().join(name);
    assert!(
        !missing.exists(),
        "fixture error: the offending path must NOT exist"
    );

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string()])
        .arg(&missing)
        .output()
        .expect("run devflow stop");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a non-existent non-UTF-8 root must fail:\n{stderr}"
    );
    assert!(
        stderr.contains("project path does not exist:"),
        "a non-UTF-8 root must survive clap and reach project_root:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// The live-run fixtures: `devflow start --mode supervise` launches a runner unit
// through the `HermeticHome`'s fake `systemd-run`, and `stop` stops it through the
// fake `systemctl --user stop`, which SIGTERMs the runner the launcher recorded.
// ---------------------------------------------------------------------------

/// A repository on `develop` with a roadmap entry for `phase`, the shape
/// `supervisor_e2e.rs` launches a real supervised run from.
fn init_supervised_repo(root: &Path, phase: PhaseId) {
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
        format!("# Roadmap\n\n### Phase {phase}: stop fixture\n"),
    )
    .unwrap();
    git(root, &["add", "README.md", ".planning/ROADMAP.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
}

/// The fake `claude`'s health check and delivery canary, as in
/// `supervisor_e2e.rs`; `body` runs for the real stage turn.
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

/// What a [`triggered_agent`] does once released: one empty commit, the
/// success marker, exit 0.
const SUCCEEDS: &str = r#"git commit -q --allow-empty -m "fake stage work"
printf 'work done\nDEVFLOW_RESULT: {"status":"success","commits":1}\n'
exit 0"#;

/// A fake agent that keeps running until `trigger` exists, then runs
/// `released` ([`SUCCEEDS`] or [`FAILS_TO_A_GATE`]).
fn triggered_agent(trigger: &Path, released: &str) -> String {
    fake_claude(&format!(
        "while [ ! -e '{trigger}' ]; do sleep 0.05; done\n{released}",
        trigger = trigger.display()
    ))
}

/// The stop reason the runner records for a SIGTERM (`runner_stop_reason`), which is
/// how the fake `systemctl --user stop` ends a unit's runner, as the manager does.
const RUNNER_SIGTERM_REASON: &str = "stopped by SIGTERM to the runner";

/// The phase's unit name as the binary derives it (`service::unit_name`).
fn unit_name(root: &Path, phase: PhaseId) -> String {
    format!(
        "devflow-{:016x}-{}.service",
        devflow_core::registry::path_digest(root),
        phase.padded()
    )
}

/// Initialise `root` and launch a supervised run of `fake_bin`'s agent there with
/// `devflow start --mode supervise --until code` plus `extra`, as a runner unit through
/// `home`'s fake `systemd-run`; the runner pid the fake recorded. Bind a
/// `LaunchedRunReaper::for_home` for `home`, `root` and `phase` first.
fn launch_unit_run(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    fake_bin: &common::FakeBin,
    extra: &[&str],
) -> u32 {
    init_supervised_repo(root, phase);
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());
    let output = common::hermetic_devflow(home)
        .args([
            "start",
            "--phase",
            &phase.to_string(),
            "--agent",
            "claude",
            "--mode",
            "supervise",
            "--until",
            "code",
        ])
        .args(extra)
        .arg(root)
        .current_dir(root)
        .hermetic_path(path)
        .output()
        .expect("run devflow start");
    assert!(
        output.status.success(),
        "start failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(
        home.service_bin()
            .join(format!("{}.pid", unit_name(root, phase))),
    )
    .ok()
    .and_then(|text| text.trim().parse().ok())
    .expect("start launched a runner through the fake systemd-run")
}

/// One live supervised run of a [`triggered_agent`], launched by
/// `devflow start --mode supervise --until code` as a runner unit through the
/// home's fake `systemd-run`. Its reaper is declared first so it drops before
/// the home and repository it needs.
struct LiveRun {
    _launch_reaper: common::LaunchedRunReaper,
    home: common::HermeticHome,
    _repo: tempfile::TempDir,
    _fake_bin: common::FakeBin,
    root: PathBuf,
    phase: PhaseId,
    /// The runner the fake `systemd-run` recorded for the phase's unit.
    runner_pid: u32,
}

impl LiveRun {
    /// A run whose agent succeeds once released, on the main checkout.
    fn start(phase: PhaseId) -> Self {
        Self::launch(phase, &["--no-worktree"])
    }

    /// [`Self::start`] in a phase worktree, `start`'s default.
    fn start_in_a_worktree(phase: PhaseId) -> Self {
        Self::launch(phase, &[])
    }

    fn launch(phase: PhaseId, extra: &[&str]) -> Self {
        let home = common::HermeticHome::new();
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path().canonicalize().unwrap();
        let fake_bin = common::fake_bin_dir(&triggered_agent(
            &root.join(".git").join("devflow-test-trigger"),
            SUCCEEDS,
        ));
        let launch_reaper = common::LaunchedRunReaper::for_home(&home, &root, phase);
        let runner_pid = launch_unit_run(&home, &root, phase, &fake_bin, extra);
        let run = Self {
            _launch_reaper: launch_reaper,
            home,
            _repo: repo,
            _fake_bin: fake_bin,
            root,
            phase,
            runner_pid,
        };
        assert!(run.alive(), "the run's runner {runner_pid} is running");
        run
    }

    /// `devflow stop --phase N` plus `extra`, run from the project root.
    fn stop(&self, extra: &[&str]) -> std::process::Output {
        common::hermetic_devflow(&self.home)
            .args(["stop", "--phase", &self.phase.to_string()])
            .args(extra)
            .current_dir(&self.root)
            .output()
            .expect("run devflow stop")
    }

    fn state(&self) -> State {
        devflow_core::workflow::load_state(&self.root, self.phase).expect("load the run's state")
    }

    /// Whether the run's runner still runs.
    fn alive(&self) -> bool {
        common::runs_the_runner_verb(self.runner_pid)
    }
}

/// `stop`'s stdout and stderr, for an assertion message.
fn describe(output: &std::process::Output) -> String {
    format!(
        "exit {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
