mod common;

use common::HermeticPath;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::workflow::load_state;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

struct FakeBin {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

fn git(root: &Path, args: &[&str]) -> Output {
    // Hermetic: pinning cwd alone does not stop an inherited GIT_DIR from
    // retargeting the real repository (999.37).
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn init_repo(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "tag.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    // 20b instance 2 (D-08, fixture-side only): a loose object write must be
    // durable before the very next index read, or a tight commit loop can
    // race a torn/partial object onto disk (the
    // start_worktree_mode_ignores_main_checkout_divergence flake). Applied
    // to every fixture repo here rather than only the flaky tests' own
    // helpers, since this is the single repo-init path both of them share.
    git(root, &["config", "core.fsyncObjectFiles", "true"]);
    git(root, &["config", "core.fsync", "all"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    // Pre-baked GSD context for every phase these tests launch — the
    // fresh-codex pre-flight (13-06) refuses codex runs on phases with no
    // CONTEXT.md on develop, and these fixtures exercise phases 7–9 with
    // both agents and phases 11–13 and 44 with codex.
    for phase in ["07", "08", "09", "11", "12", "13", "44"] {
        let dir = root.join(format!(".planning/phases/{phase}-test"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{phase}-CONTEXT.md")), "ctx\n").unwrap();
        fs::write(dir.join(format!("{phase}-01-PLAN.md")), "plan\n").unwrap();
    }
    // A GSD project config, because a real GSD-driven project has one and
    // 35.1-03's `preflight_unattended_launch_check` refuses an unattended
    // launch whose chain flag has nowhere to live. Written here rather than in
    // the individual tests so the fixture keeps modelling a real project
    // rather than a project that happens to satisfy one preflight condition.
    fs::create_dir_all(root.join(".planning")).unwrap();
    fs::write(
        root.join(".planning/config.json"),
        "{\n  \"workflow\": {\n    \"auto_advance\": false\n  }\n}\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    git(root, &["branch", "main"]);
}

fn fake_bin_dir(scripts: &[(&str, &str)]) -> FakeBin {
    let dir = tempfile::tempdir().unwrap();
    for (name, script) in scripts {
        let path = dir.path().join(name);
        fs::write(&path, script).unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
    }
    let path = dir.path().to_path_buf();
    FakeBin { _dir: dir, path }
}

fn path_with_fake_bin(fake_bin: &Path) -> String {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    format!("{}:{}", fake_bin.display(), existing.to_string_lossy())
}

fn run_devflow(home: &common::HermeticHome, root: &Path, fake_bin: &Path, args: &[&str]) -> Output {
    let mut command = common::hermetic_devflow(home);
    command
        .args(args)
        .arg(root)
        .hermetic_path(path_with_fake_bin(fake_bin))
        .env("DEVFLOW_TEST_ROOT", root)
        .current_dir(root);
    // HYG-01: bound the gate wait on the CHILD's env. Supervise-mode stages
    // that reach a gate (never-silent Plan/Code failures, Validate in
    // supervise) block `devflow advance` for the default 3-day gate timeout;
    // the sh monitor reaps cleanly but its advance child — spawned as the
    // script's last line — is orphaned and keeps polling. The tests finish
    // their own assertions far inside this window (the wait_for_* helpers cap
    // at 10s), and the orphaned advances exit on their own once the gate
    // times out instead of accumulating across runs.
    command.env("DEVFLOW_GATE_TIMEOUT_SECS", "60");
    let output = command.output().expect("run devflow");
    assert!(
        output.status.success(),
        "devflow {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// Suite-level registry of monitor PIDs the tests' `MonitorReapGuard`s are
/// responsible for (41-02 Task 1, codex-4).
///
/// Populated by [`MonitorReapGuard::after_launch`] (the pid the guard will
/// reap — the SETTLED-state monitor, not the first-stage one), and drained by
/// the guard's `Drop` after the verified reap. The suite audit asserts the
/// registry is EMPTY once every bound guard has dropped — so an empty
/// registry means "every monitor a test was responsible for was verified
/// reaped", which is the claim HYG-01 makes. An unguarded test registers
/// nothing and is therefore not what the registry detects; what it does
/// detect is a guard that bound but failed to reap (its pid stays registered
/// and alive), which `registered_monitors_alive` proves it can see.
static REGISTRY: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<u32>>> =
    std::sync::OnceLock::new();

fn registry() -> &'static std::sync::Mutex<std::collections::HashSet<u32>> {
    REGISTRY.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

/// Count of bound-but-not-yet-dropped `MonitorReapGuard`s — the suite audit's
/// ordering barrier: the audit waits for this to reach 0 so it cannot race a
/// still-running test into a false empty.
static ACTIVE_GUARDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Count of verified reaps performed by the suite's guards. The audit requires
/// this to be > 0 so an empty registry cannot be a vacuous "nothing ran" pass.
static REAPED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The suite-audit detection helper (codex-4): which registered PIDs are
/// still alive? Pure over the registry so it can be unit-tested to FAIL
/// against a deliberately-alive registered PID — the audit must be able to
/// redden, or a per-test guard that can never be wrong proves nothing.
fn registered_monitors_alive(registry: &std::collections::HashSet<u32>) -> Vec<u32> {
    registry
        .iter()
        .copied()
        .filter(|pid| devflow_core::agent::agent_running(*pid))
        .collect()
}

fn wait_for(path: &Path) {
    for _ in 0..200 {
        if path.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out waiting for {}", path.display());
}

/// Wait until a monitor-written pid file exists AND holds a parseable pid,
/// returning it. A plain `wait_for` + one-shot read is racy: each stage
/// transition's `archive_phase_files` briefly deletes the pid file before
/// the next monitor recreates it, so a read can land in the gap and hit
/// NotFound even though the pipeline is healthy.
fn wait_for_pid(path: &Path) -> u32 {
    for _ in 0..200 {
        if let Ok(contents) = fs::read_to_string(path)
            && let Ok(pid) = contents.trim().parse::<u32>()
        {
            return pid;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out waiting for a pid in {}", path.display());
}

/// The launch line of an agent stub that records its own pid in `pid_path` and
/// then becomes `sleep`, so the recorded pid is the hung agent process itself.
fn recording_sleeper(pid_path: &Path) -> String {
    format!("echo $$ > '{}'\nexec sleep 30\n", pid_path.display())
}

/// The runner pid the fake `systemd-run` recorded for `phase`'s unit under
/// `home`. The unit name is derived from the canonical root, as the binary does
/// (`service::unit_name`).
fn recorded_runner_pid(home: &common::HermeticHome, root: &Path, phase: PhaseId) -> Option<u32> {
    let root = root.canonicalize().ok()?;
    let unit = format!(
        "devflow-{:016x}-{}.service",
        devflow_core::registry::path_digest(&root),
        phase.padded()
    );
    fs::read_to_string(home.service_bin().join(format!("{unit}.pid")))
        .ok()
        .and_then(|text| text.trim().parse().ok())
}

/// Wait until a phase's persisted state is cleared (the run was aborted and
/// `abort` called `clear_state`). 20b's new liveness
/// guard in `cleanup` correctly refuses to remove a worktree whose monitor
/// is still actively driving stages (`BetweenStages`/`Healthy`) — fixtures
/// that call `cleanup` must first wait for the phase to actually finish,
/// the same way a real operator would.
fn wait_for_state_cleared(root: &Path, phase: PhaseId) {
    for _ in 0..400 {
        if devflow_core::workflow::load_state(root, phase).is_err() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out waiting for phase {phase} state to clear (pipeline never finished)");
}

/// Wait until no process holds a phase's runner lock: the runner has exited.
fn wait_for_runner_lock_free(root: &Path, phase: PhaseId) {
    let lock = devflow_core::liveness::runner_lock_path(root, phase);
    for _ in 0..400 {
        if devflow_core::liveness::probe(&lock).unwrap() != devflow_core::liveness::Held::Live {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out waiting for phase {phase}'s runner to let go of its runner lock");
}

/// Wait until a phase's persisted state has `stopped == true` (20c: a
/// `--until`-halted phase). Polls rather than reading once, since the fake
/// agent + monitor chain advances asynchronously.
fn wait_for_stopped(root: &Path, phase: PhaseId) -> devflow_core::state::State {
    for _ in 0..400 {
        if let Ok(state) = devflow_core::workflow::load_state(root, phase)
            && state.stopped
        {
            return state;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out waiting for phase {phase} state to report stopped == true");
}

fn seed_feature_branch(root: &Path, phase: PhaseId) {
    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    git(root, &["checkout", "-q", "-b", &branch]);
    fs::write(root.join("initial.txt"), "initial phase work\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "initial phase work"]);
    git(root, &["checkout", "-q", "develop"]);
}

#[test]
fn start_defaults_to_worktree() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[(
        "codex",
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
    )]);

    // No worktree flag at all — worktree-by-default (13d) means this must
    // create the isolated worktree without an explicit opt-in.
    //
    // A capture-shape agent (codex) pinned deliberately (34-06b): the subject
    // is where `start` puts the phase, not which transport its agent is
    // launched over.
    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            // `supervise` (35.1-03): the capture shape pinned above is
            // exactly what C2 refuses in `Mode::Auto`, and where `start` puts
            // the phase is mode-independent.
            "start",
            "--phase",
            "11",
            "--agent",
            "codex",
            "--mode",
            "supervise",
        ],
    );
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(11));

    // start returns before the detached monitor finishes; wait for the
    // worktree directory like the other integration tests do.
    wait_for(&root.join(".worktrees/phase-11"));
    assert!(root.join(".worktrees/phase-11").is_dir());

    let state = devflow_core::workflow::load_state(root, PhaseId::new(11)).expect("load state");
    assert!(
        state.worktree_path.is_some(),
        "expected worktree_path to be Some(_) by default, got {:?}",
        state.worktree_path
    );
    // Bind the guard to the SETTLED state: the chain advances through the
    // supervise stages and blocks at the Validate gate; that last monitor is
    // the one this test must reap (HYG-01).
    let settled = wait_for_settled(root, PhaseId::new(11));
    let _reap = MonitorReapGuard::after_launch(&settled);
}

/// WR-10 (13-REVIEW.md): the pre-start divergence check must not inspect the
/// main checkout's current HEAD when worktree mode is active (the default)
/// — worktree mode always forks fresh from `develop` regardless of what's
/// checked out in the main repo, so a stale/unrelated branch left checked
/// out there must not block `start`. Before the fix, this test's "ancient"
/// branch (60+ commits behind develop) would hard-fail `start` with a
/// "develop is N commits ahead" error that had nothing to do with the new
/// phase's worktree, which always starts at ahead=0, behind=0.
#[test]
fn start_worktree_mode_ignores_main_checkout_divergence() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);

    // Branch off develop, then leave develop far ahead — past the
    // `behind > 50` hard-fail threshold (commands.rs:158) — while the main
    // checkout stays on the stale branch. 51 is the smallest count that
    // still crosses `> 50` (20b/D-08: shrinking the window narrows the
    // object-store corruption race this loop otherwise widens for no
    // functional reason).
    git(root, &["checkout", "-q", "-b", "ancient", "develop"]);
    git(root, &["checkout", "-q", "develop"]);
    for i in 0..51 {
        fs::write(root.join(format!("f{i}.txt")), i.to_string()).unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", &format!("commit {i}")]);
    }
    git(root, &["checkout", "-q", "ancient"]);

    let fake_bin = fake_bin_dir(&[(
        "codex",
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
    )]);

    // Worktree mode is the default — no --no-worktree flag. This must
    // succeed (the helper asserts a zero exit status) despite the main
    // checkout being 51 commits behind develop.
    //
    // A capture-shape agent (codex) pinned deliberately (34-06b): the subject
    // is the divergence check's scope, not the launch transport.
    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            // `supervise` (35.1-03): same reason as phase 11 above — the
            // capture shape is refused in `Mode::Auto`, and this test's
            // subject (main-checkout divergence) is mode-independent.
            "start",
            "--phase",
            "13",
            "--agent",
            "codex",
            "--mode",
            "supervise",
        ],
    );
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(13));

    wait_for(&root.join(".worktrees/phase-13"));
    assert!(root.join(".worktrees/phase-13").is_dir());

    // Bind the guard to the SETTLED state (the chain blocks at the Validate
    // gate in supervise mode — that last monitor is this test's to reap).
    let settled = wait_for_settled(root, PhaseId::new(13));
    let _reap = MonitorReapGuard::after_launch(&settled);
}

#[test]
fn start_no_worktree_uses_feature_branch() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[(
        "codex",
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
    )]);

    // A capture-shape agent (codex) pinned deliberately (34-06b): the subject
    // is that `--no-worktree` keeps the phase on its feature branch, not which
    // transport its agent is launched over.
    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "12",
            "--agent",
            "codex",
            // `supervise` (35.1-03): same reason as phase 11 — the capture
            // shape pinned above is refused in `Mode::Auto`, and
            // `--no-worktree`'s branch placement is mode-independent.
            "--mode",
            "supervise",
            "--no-worktree",
        ],
    );
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(12));

    // start returns once its runner unit holds the runner lock (D-11); wait for
    // the run to settle on the feature-branch path before reading it.
    let settled = wait_for_settled(root, PhaseId::new(12));
    assert!(!root.join(".worktrees/phase-12").exists());

    let state = devflow_core::workflow::load_state(root, PhaseId::new(12)).expect("load state");
    assert!(
        state.worktree_path.is_none(),
        "expected worktree_path to be None with --no-worktree, got {:?}",
        state.worktree_path
    );
    assert!(
        settled.worktree_path.is_none(),
        "the settled run stays on the feature branch: {:?}",
        settled.worktree_path
    );
}

/// 20c (D-09 + review: Codex HIGH off-by-one): `devflow start --until plan`
/// must run Plan, the first stage, to completion, then halt BEFORE advancing
/// to Code — not stop before Plan ever runs. The fake agent always reports
/// success, so the monitor chain runs Plan→advance; that `advance` calls
/// `transition(.., Stage::Code)` with `state.stage == Plan`, which is exactly
/// the `stop_until == Some(from)` case this plan adds. A fresh run already
/// sits at Plan, so the persisted stage alone cannot show Plan ran: the
/// `stage_launched` event for Plan does.
#[test]
fn start_until_plan_halts_cleanly() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[(
        "codex",
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
    )]);

    // A capture-shape agent (codex) pinned deliberately (34-06b): the subject
    // is WHERE the `--until` cap halts the stage machine, not which transport
    // each stage's agent is launched over. Codex launches every stage, Plan
    // included, in the capture shape.
    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            // `supervise` (35.1-03): same reason as phase 11 above — the
            // capture shape is refused in `Mode::Auto`, and the `--until` cap
            // halts at Plan, well before supervise's Validate gate.
            "start",
            "--phase",
            "44",
            "--agent",
            "codex",
            "--mode",
            "supervise",
            "--until",
            "plan",
        ],
    );
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(44));

    let state = wait_for_stopped(root, PhaseId::new(44));
    let events = fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
    assert!(
        events
            .lines()
            .any(|line| line.contains("\"stage_launched\"") && line.contains("\"stage\":\"plan\"")),
        "Plan must have launched before the halt\nevents:\n{events}"
    );
    assert_eq!(
        state.stage,
        devflow_core::stage::Stage::Plan,
        "the persisted stage must be the COMPLETED target (Plan), proving Plan ran \
         before the halt — not that the pipeline stopped before Plan ever launched"
    );
    assert!(state.stopped, "stop marker must be set");
    assert_eq!(
        state.supervisor, None,
        "the stop path must clear the supervisor record so no monitor is left behind"
    );
    assert!(
        state.stop_reason.is_some(),
        "a human-readable stop_reason must be recorded"
    );
    // The stop path cleared the supervisor record; the guard still binds (no-op) so
    // every monitor-spawning test in this file follows the same teardown
    // shape (41-02 Task 1 systematic pass).
    let _reap = MonitorReapGuard::after_launch(&state);
}

/// [`init_repo`]'s committed GSD config as GSD's `config-set` rewrites it the
/// first time it writes `workflow._auto_chain_active: false`.
const CONFIG_WITH_GSDS_FALSE: &str = "{\n  \"workflow\": {\n    \"auto_advance\": false,\n    \"_auto_chain_active\": false\n  }\n}\n";

/// The subject of the commit that records GSD's own `false` (52.1 D-22).
const RECORD_GSD_FALSE_SUBJECT: &str =
    "chore(gsd): record GSD's workflow._auto_chain_active: false";

/// The words only the killed-run repair notice uses.
const KILLED_RUN_NOTICE: &str = "was killed before it could clear it";

/// Every `auto_chain_flag_repaired` line in a project's event log.
fn auto_chain_repair_events(root: &Path) -> Vec<String> {
    fs::read_to_string(devflow_core::events::events_path(root))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("\"auto_chain_flag_repaired\""))
        .map(str::to_owned)
        .collect()
}

/// Runs `devflow start --no-worktree --until plan` for phase 44 with a stub
/// `codex`, waits for the run to settle, and returns what `start` printed
/// (stdout, then stderr).
///
/// `--no-worktree` is what lets the launch repair meet an uncommitted
/// `.planning/config.json`: `start` cuts `feature/phase-44` in place, so the
/// working copy the caller left is the one the repair reads. A worktree start
/// forks a clean checkout and can never show a difference.
fn start_phase_44_in_place(home: &common::HermeticHome, root: &Path) -> String {
    let fake_bin = fake_bin_dir(&[(
        "codex",
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
    )]);
    let output = run_devflow(
        home,
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "44",
            "--agent",
            "codex",
            "--mode",
            "supervise",
            "--until",
            "plan",
            "--no-worktree",
        ],
    );
    let _reaper = common::LaunchedRunReaper::for_home(home, root, PhaseId::new(44));
    let settled = wait_for_settled(root, PhaseId::new(44));
    let _reap = MonitorReapGuard::after_launch(&settled);
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// 52.1 D-22 through `devflow start`: the committed config never had the chain
/// flag and GSD's first `config-set` wrote `false` into the working copy. `start`
/// records it on the phase branch under the accurate commit, and names no
/// killed run on either channel.
#[test]
fn start_records_gsds_first_false_without_a_killed_run_notice() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let committed =
        String::from_utf8_lossy(&git(root, &["show", "develop:.planning/config.json"]).stdout)
            .into_owned();
    assert!(
        !committed.contains("_auto_chain_active"),
        "premise: the committed config lacks the key: {committed}"
    );
    fs::write(root.join(".planning/config.json"), CONFIG_WITH_GSDS_FALSE).unwrap();

    let printed = start_phase_44_in_place(&home, root);

    let subjects = String::from_utf8_lossy(
        &git(root, &["log", "--format=%s", "develop..feature/phase-44"]).stdout,
    )
    .into_owned();
    assert_eq!(
        subjects.lines().collect::<Vec<_>>(),
        [RECORD_GSD_FALSE_SUBJECT],
        "start must record GSD's false in one commit on the phase branch\nprinted:\n{printed}"
    );
    let at_tip = String::from_utf8_lossy(
        &git(root, &["show", "feature/phase-44:.planning/config.json"]).stdout,
    )
    .into_owned();
    assert_eq!(
        at_tip, CONFIG_WITH_GSDS_FALSE,
        "the phase branch's tip must carry GSD's false"
    );
    let events = auto_chain_repair_events(root);
    assert!(
        events.is_empty(),
        "recording GSD's false is not a repair and writes no repair event: {events:?}"
    );
    assert!(
        printed.contains(
            "note: devflow start recorded GSD's workflow._auto_chain_active: false for phase 44"
        ),
        "start must announce the recording, or the absent killed-run notice below \
         proves nothing\nprinted:\n{printed}"
    );
    assert!(
        !printed.contains(KILLED_RUN_NOTICE),
        "nothing leaked, so no killed run may be named\nprinted:\n{printed}"
    );
    assert!(
        !printed.contains(LEFT_UNCOMMITTED_NOTE),
        "a recorded false leaves nothing uncommitted\nprinted:\n{printed}"
    );
}

/// The words only the left-uncommitted note uses.
const LEFT_UNCOMMITTED_NOTE: &str = "left .planning/config.json uncommitted";

/// Operator ruling 2026-11-15 through `devflow start`: the branch tip holds
/// GSD's `false` and the key is gone from the working copy. `start` commits
/// nothing and writes no repair event, and says in one foreground line that the
/// file is left uncommitted and what that costs.
#[test]
fn start_says_when_it_leaves_a_config_change_it_cannot_attribute_uncommitted() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let config = root.join(".planning/config.json");
    let without_key = fs::read_to_string(&config).unwrap();
    assert!(
        !without_key.contains("_auto_chain_active"),
        "premise: the fixture's config has no chain flag: {without_key}"
    );
    fs::write(&config, CONFIG_WITH_GSDS_FALSE).unwrap();
    git(root, &["add", ".planning/config.json"]);
    git(
        root,
        &["commit", "-q", "-m", "docs: record the chain flag as false"],
    );
    let develop_tip = String::from_utf8_lossy(&git(root, &["rev-parse", "develop"]).stdout)
        .trim()
        .to_owned();
    fs::write(&config, &without_key).unwrap();

    let printed = start_phase_44_in_place(&home, root);

    assert_eq!(
        String::from_utf8_lossy(&git(root, &["rev-parse", "feature/phase-44"]).stdout).trim(),
        develop_tip,
        "start must commit nothing on the phase branch in this row\nprinted:\n{printed}"
    );
    let status =
        String::from_utf8_lossy(&git(root, &["status", "--porcelain"]).stdout).into_owned();
    assert!(
        status
            .lines()
            .any(|line| line == " M .planning/config.json"),
        "the file stays modified and uncommitted, got status {status:?}"
    );
    let events = auto_chain_repair_events(root);
    assert!(
        events.is_empty(),
        "nothing was repaired, so no repair event may be written: {events:?}"
    );
    let notes: Vec<&str> = printed
        .lines()
        .filter(|line| line.contains(LEFT_UNCOMMITTED_NOTE))
        .collect();
    assert_eq!(
        notes.len(),
        1,
        "start must say in exactly one line that the file is left uncommitted\nprinted:\n{printed}"
    );
    assert!(
        notes[0]
            .starts_with("note: devflow start left .planning/config.json uncommitted for phase 44"),
        "the line must name the entry point, the file and the phase: {}",
        notes[0]
    );
    assert!(
        notes[0].contains("DevFlow cannot attribute the change"),
        "the line must say why nothing was committed: {}",
        notes[0]
    );
    assert!(
        notes[0].contains("Validate will not record a pass until it is committed"),
        "the line must say what the uncommitted file costs: {}",
        notes[0]
    );
    assert!(
        !printed.contains(KILLED_RUN_NOTICE),
        "nothing leaked, so no killed run may be named\nprinted:\n{printed}"
    );
}

/// `--until` values for stages v4.0 removed (D-07), in every case spelling. Later cut
/// plans append to this list.
/// Split so the removed-surface scan does not read this control as a leftover.
const REMOVED_UNTIL_STAGES: &[&str] = &[
    concat!("sh", "ip"),
    "Ship",
    "SHIP",
    concat!("def", "ine"),
    "Define",
    "DEFINE",
];

/// D-07 (CUT-01, CUT-02): the Ship and Define stages are gone, so no case spelling of
/// `--until ship` or `--until define` is a stage value, and clap refuses each as an
/// invalid value before `start` is dispatched. Control in the same test: every kept
/// stage `--until` accepts still parses, and the dry run exits 0. The accepted values
/// follow one case rule: whatever the parser does with `PLAN`, it does with `CODE` and
/// `VALIDATE` too.
#[test]
fn until_rejects_removed_stages() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[]);
    let dry_run_until = |stage: &str| {
        common::hermetic_devflow(&home)
            .args([
                "start",
                "--phase",
                "1",
                "--agent",
                "claude",
                "--mode",
                "auto",
                "--until",
                stage,
                "--dry-run",
            ])
            .arg(root)
            .hermetic_path(path_with_fake_bin(&fake_bin.path))
            .current_dir(root)
            .output()
            .expect("run devflow")
    };

    for removed in REMOVED_UNTIL_STAGES {
        let output = dry_run_until(removed);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "--until {removed} must be rejected\nstderr: {stderr}"
        );
        assert!(
            stderr.contains("invalid value"),
            "clap must refuse --until {removed} as an invalid value\nstderr: {stderr}"
        );
    }
    for kept in ["plan", "code", "validate"] {
        let output = dry_run_until(kept);
        assert!(
            output.status.success(),
            "--until {kept} must be accepted\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let upper_plan_accepted = dry_run_until("PLAN").status.success();
    for upper in ["CODE", "VALIDATE"] {
        let output = dry_run_until(upper);
        assert_eq!(
            output.status.success(),
            upper_plan_accepted,
            "--until {upper} must follow the case rule `PLAN` follows (accepted: \
             {upper_plan_accepted})\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(!root.join(".worktrees/phase-1").exists());
}

/// 20c edge-probe (20c/empty): `--until bogus` needs no new parsing surface —
/// it is rejected by the existing `Stage: FromStr` parser (via clap) before
/// `start` is ever dispatched.
#[test]
fn start_until_unknown_stage_is_rejected_by_clap() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[(
        "claude",
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
    )]);

    let output = common::hermetic_devflow(&home)
        .args([
            "start", "--phase", "46", "--agent", "claude", "--mode", "auto", "--until", "bogus",
        ])
        .arg(root)
        .hermetic_path(path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow");

    assert!(
        !output.status.success(),
        "--until bogus must be rejected by the existing Stage parser"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("bogus"),
        "clap's error must name the unrecognized value\nstderr: {stderr}"
    );
    assert!(!root.join(".worktrees/phase-46").exists());
}

/// Subcommands v4.0 removed (D-01). Later cut plans append to this list.
const REMOVED_SUBCOMMANDS: &[&str] = &[
    "list",
    "reference",
    "test",
    "release",
    "parallel",
    // Split so the removed-surface scan does not read this control as a leftover.
    concat!("sh", "ip"),
    "evidence",
];

/// The subcommand names listed under `Commands:` in a `devflow --help` output.
fn help_subcommand_names(help: &str) -> Vec<&str> {
    help.lines()
        .skip_while(|line| line.trim_end() != "Commands:")
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .collect()
}

/// A removed subcommand fails loudly as an unknown verb instead of running
/// something else, and `--help` no longer lists it. `status` is the control:
/// the same checks must see a verb that still exists.
#[test]
fn removed_subcommands_are_unknown() {
    let home = common::HermeticHome::new();
    let scratch = tempfile::tempdir().unwrap();

    let help = common::hermetic_devflow(&home)
        .arg("--help")
        .current_dir(scratch.path())
        .output()
        .expect("run devflow --help");
    assert!(help.status.success());
    let help = String::from_utf8_lossy(&help.stdout);
    let names = help_subcommand_names(&help);
    assert!(
        names.contains(&"status"),
        "control: the help parser must find `status`\n{help}"
    );

    for verb in REMOVED_SUBCOMMANDS {
        let output = common::hermetic_devflow(&home)
            .arg(verb)
            .current_dir(scratch.path())
            .output()
            .expect("run devflow");
        assert!(
            !output.status.success(),
            "`devflow {verb}` must be rejected"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("unrecognized subcommand"),
            "`devflow {verb}` must be rejected by clap as unknown\nstderr: {stderr}"
        );
        assert!(
            !names.contains(verb),
            "`devflow --help` still lists `{verb}`\n{help}"
        );
    }

    let status = common::hermetic_devflow(&home)
        .args(["status", "--help"])
        .current_dir(scratch.path())
        .output()
        .expect("run devflow status --help");
    assert!(
        status.status.success(),
        "control: `devflow status --help` must still succeed\nstderr: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

/// Flag and argument forms v4.0 removed (D-01), each paired with the token
/// clap must name as unexpected. Naming the token matters: before the removal
/// `start --phase 1 --worktree x --dry-run` was already refused, for its missing
/// `--mode`. Later cut plans append to this list.
const REMOVED_FLAG_INVOCATIONS: &[(&[&str], &str)] = &[
    (
        &["start", "--phase", "1", "--worktree", "x", "--dry-run"],
        "--worktree",
    ),
    (&["gate", "approve", "1", "code"], "code"),
    (&["gate", "reject", "1", "code", "--note", "n"], "code"),
    // `--dry-run` keeps this safe against a binary that still has the flag: a
    // real pass on the hermetic builder would treat every 2.x-shaped process
    // on the machine as a stray.
    // Split so the removed-surface scan does not read this control as a leftover.
    (
        &["gate", "sweep", concat!("--reap", "-strays"), "--dry-run"],
        concat!("--reap", "-strays"),
    ),
    // `--dry-run` keeps the start invocation from launching anything against
    // a binary that still accepts the flag; the resume one runs in an empty
    // scratch directory with no run to resume.
    (
        &[
            "start",
            "--phase",
            "1",
            concat!("--legacy-claude", "-launch"),
            "--dry-run",
        ],
        concat!("--legacy-claude", "-launch"),
    ),
    (
        &[
            "resume",
            "--phase",
            "1",
            concat!("--legacy-claude", "-launch"),
        ],
        concat!("--legacy-claude", "-launch"),
    ),
    (
        &[
            "start",
            "--phase",
            "1",
            concat!("--yes", "-ship"),
            "--dry-run",
        ],
        concat!("--yes", "-ship"),
    ),
];

/// A removed flag or positional argument is refused by clap as unexpected
/// instead of being accepted and ignored. `gate approve --help` is the
/// control: the subcommand that lost its positional stage still parses.
#[test]
fn removed_flags_are_unknown() {
    let home = common::HermeticHome::new();
    let scratch = tempfile::tempdir().unwrap();

    for (args, token) in REMOVED_FLAG_INVOCATIONS {
        let output = common::hermetic_devflow(&home)
            .args(*args)
            .current_dir(scratch.path())
            .output()
            .expect("run devflow");
        assert!(
            !output.status.success(),
            "`devflow {}` must be rejected",
            args.join(" ")
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let expected = format!("unexpected argument '{token}'");
        assert!(
            stderr.contains(&expected),
            "`devflow {}` must be rejected by clap with {expected:?}\nstderr: {stderr}",
            args.join(" ")
        );
    }

    let help = common::hermetic_devflow(&home)
        .args(["gate", "approve", "--help"])
        .current_dir(scratch.path())
        .output()
        .expect("run devflow gate approve --help");
    assert!(
        help.status.success(),
        "control: `devflow gate approve --help` must still succeed\nstderr: {}",
        String::from_utf8_lossy(&help.stderr)
    );
}

/// The stage names a `start --dry-run` preview lists, in order: the first word of each line
/// indented by exactly two spaces between the `stage pipeline:` header and the next blank line.
/// Deeper-indented continuation lines (3.0.0's `↳ hooks:`) are not stage lines.
fn dry_run_stage_names(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .skip_while(|line| line.trim() != "stage pipeline:")
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter(|line| line.starts_with("  ") && !line[2..].starts_with(' '))
        .filter_map(|line| line.split_whitespace().next().map(str::to_string))
        .collect()
}

/// `None` when the preview lists exactly `expected`, in order; otherwise what it listed.
fn dry_run_stage_mismatch(stdout: &str, expected: &[&str]) -> Option<String> {
    let listed = dry_run_stage_names(stdout);
    (listed != expected).then(|| format!("listed {listed:?}, expected {expected:?}"))
}

/// What 3.0.0 printed for `devflow start --dry-run --phase 52 --mode supervise`, copied from
/// `52-RUN-NOTES.md` (Run 2): a Define first line and a Ship last line. The hook names are
/// split so the removed-surface scan does not read this control as a leftover.
const DRY_RUN_3_0_0: &str = concat!(
    "dry run — phase 52 | agent claude | mode supervise

stage pipeline:
  define /gsd-discuss-phase 52
  plan /gsd-plan-phase 52
  code /gsd-execute-phase 52
  validate /gsd-validate-phase 52 [GATE]
            ↳ hooks: [Docs",
    "Update]
  ship /gsd-ship 52 [GATE]

ship gate: not pre-authorized

after ship: [Merge, Version",
    "Bump, Changelog",
    "Append, Branch",
    "Cleanup]
"
);

/// CUT-01: a fresh run starts at Plan, so the dry run lists exactly plan, code and validate, in
/// that order and nothing else. The comparison is checked against three outputs it must refuse:
/// an empty preview, one missing a stage, and the 3.0.0 preview with Define and Ship.
#[test]
fn start_dry_run_lists_exactly_plan_code_validate() {
    const EXPECTED: &[&str] = &["plan", "code", "validate"];

    // Controls: the comparison must report each of these as a mismatch.
    assert!(
        dry_run_stage_mismatch("", EXPECTED).is_some(),
        "control: an empty dry run must not compare equal"
    );
    let two_stages = "dry run — phase 1 | agent claude | mode supervise\n\nstage pipeline:\n  \
                      plan /gsd-plan-phase 1\n  code /gsd-execute-phase 1\n";
    assert!(
        dry_run_stage_mismatch(two_stages, EXPECTED).is_some(),
        "control: a dry run missing validate must not compare equal"
    );
    assert_eq!(
        dry_run_stage_names(DRY_RUN_3_0_0),
        // Split so the removed-surface scan does not read this control as a leftover.
        [
            concat!("def", "ine"),
            "plan",
            "code",
            "validate",
            concat!("sh", "ip")
        ],
        "control: the extraction must read every stage line of the 3.0.0 preview"
    );
    assert!(
        dry_run_stage_mismatch(DRY_RUN_3_0_0, EXPECTED).is_some(),
        "control: the 3.0.0 dry run (define … ship) must not compare equal"
    );

    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[]);

    let output = common::hermetic_devflow(&home)
        .args(["start", "--dry-run", "--phase", "1", "--mode", "supervise"])
        .arg(root)
        .hermetic_path(path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow");

    assert!(
        output.status.success(),
        "dry-run must not fail\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        dry_run_stage_mismatch(&stdout, EXPECTED),
        None,
        "the dry run must list exactly plan, code, validate\nstdout: {stdout}"
    );
}

const EXTERNAL_VERIFY_WARNING_PREFIX: &str = "warning: DEVFLOW_EXTERNAL_VERIFY_ENABLED=";
const EXTERNAL_VERIFY_YES_WARNING: &str =
    "warning: DEVFLOW_EXTERNAL_VERIFY_ENABLED=\"yes\" is not true or false";

/// The stderr lines that warn about `DEVFLOW_EXTERNAL_VERIFY_ENABLED`.
fn external_verify_warning_lines(stderr: &str) -> Vec<&str> {
    stderr
        .lines()
        .filter(|line| line.starts_with(EXTERNAL_VERIFY_WARNING_PREFIX))
        .collect()
}

/// `devflow start --dry-run` with `DEVFLOW_EXTERNAL_VERIFY_ENABLED` set on the
/// child only. Returns the child's stderr.
fn start_dry_run_stderr_with_external_verify(value: &str) -> String {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[]);

    let output = common::hermetic_devflow(&home)
        .args(["start", "--dry-run", "--phase", "1", "--mode", "supervise"])
        .arg(root)
        .hermetic_path(path_with_fake_bin(&fake_bin.path))
        .env("DEVFLOW_EXTERNAL_VERIFY_ENABLED", value)
        .current_dir(root)
        .output()
        .expect("run devflow");

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "dry-run must not fail\nstderr: {stderr}"
    );
    stderr
}

/// `devflow resume` on a project with no saved state, with
/// `DEVFLOW_EXTERNAL_VERIFY_ENABLED` set on the child only. Nothing can launch:
/// resume refuses for the missing state. Returns the child's stderr.
fn resume_without_state_stderr_with_external_verify(value: &str) -> String {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[]);

    let output = common::hermetic_devflow(&home)
        .args(["resume", "--phase", "12"])
        .arg(root)
        .hermetic_path(path_with_fake_bin(&fake_bin.path))
        .env("DEVFLOW_EXTERNAL_VERIFY_ENABLED", value)
        .current_dir(root)
        .output()
        .expect("run devflow");

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "premise: resume with no saved state must refuse\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("no active DevFlow state"),
        "premise: the refusal is the missing-state one\nstderr: {stderr}"
    );
    stderr
}

/// 999.229: an invalid `DEVFLOW_EXTERNAL_VERIFY_ENABLED` is said in the
/// foreground at `start`, a dry run included, as the first thing on stderr.
#[test]
fn start_dry_run_warns_on_an_invalid_external_verify_value() {
    let stderr = start_dry_run_stderr_with_external_verify("yes");

    let warnings = external_verify_warning_lines(&stderr);
    assert_eq!(
        warnings.len(),
        1,
        "exactly one warning line for the ignored value\nstderr: {stderr}"
    );
    assert!(
        warnings[0].starts_with(EXTERNAL_VERIFY_YES_WARNING),
        "the warning names the value and what is accepted: {}",
        warnings[0]
    );
    assert_eq!(
        stderr.lines().next(),
        Some(warnings[0]),
        "the warning is the first line on stderr\nstderr: {stderr}"
    );
}

/// Control for the test above: a value that is accepted prints no warning.
#[test]
fn start_dry_run_prints_no_external_verify_warning_for_a_valid_value() {
    let stderr = start_dry_run_stderr_with_external_verify("true");

    assert_eq!(
        external_verify_warning_lines(&stderr).len(),
        0,
        "an accepted value must print no warning\nstderr: {stderr}"
    );
}

/// 999.229: `resume` says it too, and before its own refusal.
#[test]
fn resume_warns_on_an_invalid_external_verify_value() {
    let stderr = resume_without_state_stderr_with_external_verify("yes");

    let warnings = external_verify_warning_lines(&stderr);
    assert_eq!(
        warnings.len(),
        1,
        "exactly one warning line for the ignored value\nstderr: {stderr}"
    );
    assert!(
        warnings[0].starts_with(EXTERNAL_VERIFY_YES_WARNING),
        "the warning names the value and what is accepted: {}",
        warnings[0]
    );
    let line_of = |needle: &str| stderr.lines().position(|line| line.contains(needle));
    assert!(
        line_of(EXTERNAL_VERIFY_WARNING_PREFIX) < line_of("no active DevFlow state"),
        "the warning prints before the refusal\nstderr: {stderr}"
    );
}

/// Control for the test above: a value that is accepted prints no warning.
#[test]
fn resume_prints_no_external_verify_warning_for_a_valid_value() {
    let stderr = resume_without_state_stderr_with_external_verify("true");

    assert_eq!(
        external_verify_warning_lines(&stderr).len(),
        0,
        "an accepted value must print no warning\nstderr: {stderr}"
    );
}

/// OPS-02 / D-35: `devflow start` says when it replaces a phase's existing state,
/// naming which: a finished run, a stopped run with its stop reason, or a state
/// file that does not load. `--dry-run` reaches the point where `start` loads the
/// state. Control: with no prior state, no replace line.
#[test]
fn start_says_when_it_replaces_a_finished_or_stale_state() {
    const FINISHED: &str = "replaces the state of a finished run";
    const STOPPED: &str = "replaces the state of a stopped run";
    const UNREADABLE: &str = "replaces an unreadable state";
    /// What a case writes before the dry run: a prior state, or nothing.
    type Prior<'a> = Option<&'a dyn Fn(&Path, PhaseId)>;
    let dry_run = |prior: Prior| -> String {
        let home = common::HermeticHome::new();
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        init_repo(root);
        let phase = PhaseId::new(7);
        if let Some(prior) = prior {
            prior(root, phase);
        }
        let fake_bin = fake_bin_dir(&[]);
        let output = common::hermetic_devflow(&home)
            .args(["start", "--dry-run", "--phase", "7", "--mode", "supervise"])
            .arg(root)
            .hermetic_path(path_with_fake_bin(&fake_bin.path))
            .current_dir(root)
            .output()
            .expect("run devflow");
        let text = String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "dry run must not fail: {text}");
        text
    };
    let saved = |shape: fn(&mut devflow_core::state::State)| {
        move |root: &Path, phase: PhaseId| {
            let mut state = devflow_core::state::State::new(
                phase,
                devflow_core::state::AgentKind::Claude,
                devflow_core::mode::Mode::Supervise,
                root.to_path_buf(),
            );
            shape(&mut state);
            devflow_core::workflow::save_state(&state).unwrap();
        }
    };

    let finished = saved(|state| {
        state.stopped = true;
        state.finished = true;
        state.stop_reason = Some(FINISHED_REASON.to_string());
    });
    let text = dry_run(Some(&finished));
    assert!(text.contains(FINISHED), "finished: {text}");
    assert!(
        !text.contains(STOPPED) && !text.contains(UNREADABLE),
        "finished names only itself: {text}"
    );

    let stopped = saved(|state| {
        state.stopped = true;
        state.stop_reason = Some(UNTIL_PLAN_REASON.to_string());
    });
    let text = dry_run(Some(&stopped));
    assert!(
        text.contains(STOPPED) && text.contains(UNTIL_PLAN_REASON),
        "stopped, with its reason: {text}"
    );
    assert!(
        !text.contains(FINISHED) && !text.contains(UNREADABLE),
        "stopped names only itself: {text}"
    );

    let unloadable = |root: &Path, phase: PhaseId| {
        let path = devflow_core::workflow::state_path(root, phase);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ not a state").unwrap();
    };
    let text = dry_run(Some(&unloadable));
    assert!(text.contains(UNREADABLE), "unloadable: {text}");
    assert!(
        !text.contains(FINISHED) && !text.contains(STOPPED),
        "unloadable names only itself: {text}"
    );

    let text = dry_run(None);
    assert!(
        !text.contains("replaces"),
        "control: a fresh start replaces nothing: {text}"
    );
}

/// WR-01 (phase 20 review): `--dry-run` must reflect `--until` in its
/// preview instead of always printing the full pipeline as if
/// `--until` had not been passed.
#[test]
fn start_dry_run_annotates_until_stage() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[]);

    let output = common::hermetic_devflow(&home)
        .args([
            "start",
            "--phase",
            "47",
            "--agent",
            "claude",
            "--mode",
            "auto",
            "--until",
            "plan",
            "--dry-run",
        ])
        .arg(root)
        .hermetic_path(path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow");

    assert!(
        output.status.success(),
        "dry-run must not fail\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        dry_run_stage_mismatch(&stdout, &["plan", "code", "validate"]),
        None,
        "the --until preview must still list exactly plan, code, validate\nstdout: {stdout}"
    );
    let annotated: Vec<String> = stdout
        .lines()
        .filter(|l| l.contains("[STOPS HERE — --until]"))
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect();
    assert_eq!(
        annotated,
        ["plan"],
        "exactly the plan stage line must be annotated as the --until stop point\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("--until plan"),
        "the preview must include a trailing note naming the --until stage\nstdout: {stdout}"
    );
    assert!(
        !root.join(".worktrees/phase-47").exists(),
        "dry-run must not create a worktree"
    );
}

/// WR-01 counterpart: without `--until`, the preview must NOT show any stop
/// annotation — the full pipeline runs to Validate.
#[test]
fn start_dry_run_without_until_has_no_stop_annotation() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[]);

    let output = common::hermetic_devflow(&home)
        .args([
            "start",
            "--phase",
            "48",
            "--agent",
            "claude",
            "--mode",
            "auto",
            "--dry-run",
        ])
        .arg(root)
        .hermetic_path(path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("STOPS HERE"),
        "no stop annotation must appear without --until\nstdout: {stdout}"
    );
}

/// `cleanup --force` removes a phase worktree once its run has ended.
#[test]
fn cleanup_worktree_cli_flow() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    // 31-03: the Code stage now runs the D-15 delivery canary before it
    // launches, and refuses outright unless a token DevFlow planted comes back
    // inside a TOP-LEVEL `result` event. A fake CLI that cannot do that is,
    // correctly, a CLI this pipeline will not run on — so the fixture models a
    // CLI that DOES deliver rather than working around the guard.
    //
    // `read -r turn` takes exactly one line and returns: the monitor writes the
    // user turn followed by a newline, and blocking on full EOF would hang
    // against a pipe deliberately held open past the first turn. On the legacy
    // stages stdin is `/dev/null`, so the read yields nothing and the ordinary
    // marker branch runs, exactly as before this change.
    let fake_bin = fake_bin_dir(&[(
        "claude",
        r#"#!/bin/sh
read -r turn
case "$turn" in
  *DEVFLOW_DELIVERY_CANARY_*)
    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
    printf '{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"%s"}\n' "$token"
    ;;
  *)
    printf 'DEVFLOW_RESULT: {"status":"success"}\n'
    ;;
esac
"#,
    )]);

    // 52-22: Validate reads GSD's own verification status, so the run gets a
    // `gsd_run` stub under its home and never a real GSD install. The agent
    // reports no verdict (a fail) and GSD reports a stale report, so the auto
    // loop back to Code runs to its cap; one reply per Validate, with room to
    // spare. (52-27: `gaps_found` would take the gap-closure pass through
    // Plan, whose post-check gates at Plan on this fixture, which writes no
    // report and no gap plan.)
    let gsd_logs = tempfile::tempdir().unwrap();
    let stale =
        devflow_core::test_support::gsd_status_json(devflow_core::gsd_reader::GsdStatus::Stale);
    common::stub_gsd_run(&home, &[stale; 10], gsd_logs.path());

    // start — creates the phase worktree by default
    let out = run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            "start", "--phase", "8", "--agent", "claude", "--mode", "auto",
        ],
    );
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(8));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("phase 8"));
    assert!(
        root.join(".worktrees/phase-08").is_dir(),
        "worktree not created"
    );

    // status — lists active worktrees
    let out = run_devflow(&home, root, &fake_bin.path, &["status"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(".worktrees/phase-08"),
        "status missing phase worktree\n{stdout}"
    );

    // 20b: cleanup now hard-refuses while a monitor is still actively
    // driving the phase (Healthy/BetweenStages) — a real operator would
    // resolve the phase before cleaning it up, not race the still-running
    // monitor. This fixture's fake agent never produces real work, so
    // Validate always loops back and forces a gate after
    // MAX_CONSECUTIVE_FAILURES; end it with `gate abort` (a rejection
    // would loop back and keep the run alive) so the monitor clears state,
    // then wait for that to land before invoking cleanup.
    wait_for(&root.join(".devflow/gates/08-validate.json"));
    let gate: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".devflow/gates/08-validate.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        gate["gate_kind"], "failure_cap",
        "the loop over GSD's stale report reaches its cap: {gate}"
    );

    // 31-03: reaching the Validate gate already implies the Code stage's
    // delivery canary confirmed — but only implicitly. Asserted explicitly so
    // that a future change which stops running the guard (e.g. narrowing
    // `STREAM_JSON_STAGES`) shows up here instead of passing silently.
    let events = fs::read_to_string(root.join(".devflow/events.jsonl")).unwrap_or_default();
    assert!(
        events.contains("claude_delivery_canary_confirmed"),
        "the Code launch must have run the delivery canary and confirmed it\n{events}"
    );

    // HYG-01 (41-02 review finding): `start --phase 8` spawned a monitor that
    // now blocks at the Validate gate. Bind a guard so the monitor is
    // verified-reaped on EVERY exit path — the `gate abort` teardown below is
    // the happy path, not a substitute for the guard.
    let settled = load_state(root, PhaseId::new(8)).expect("load state");
    let _reap = MonitorReapGuard::after_launch(&settled);

    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            "gate",
            "abort",
            "8",
            "--stage",
            "validate",
            "--note",
            "test teardown",
            // run_devflow appends the root, which now has to be --project's value.
            "--project",
        ],
    );
    wait_for_state_cleared(root, PhaseId::new(8));
    // The runner clears the state before it exits and lets go of its lock, and
    // cleanup refuses while the lock is held (D-07), so wait for the lock too.
    wait_for_runner_lock_free(root, PhaseId::new(8));

    // cleanup — removes worktrees
    let out = run_devflow(&home, root, &fake_bin.path, &["cleanup", "--force"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("removed"));

    assert!(!root.join(".worktrees/phase-08").is_dir());
}

/// 13-06 dogfood regression (Codex leg): a fresh headless Codex run can
/// never pass Plan, so `start --agent codex` on a phase with no CONTEXT.md
/// on develop must fail fast in pre-flight — before any worktree, branch, or
/// monitor is created.
#[test]
fn start_codex_without_context_fails_preflight() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    // codex IS installed (the 13-06 dogfood scenario) — the binary preflight
    // (14-CR-05) passes and the CONTEXT.md artifact check must fire next.
    let fake_bin = fake_bin_dir(&[("codex", "#!/bin/sh\nexit 0\n")]);

    let output = common::hermetic_devflow(&home)
        .args([
            "start", "--phase", "42", "--agent", "codex", "--mode", "auto",
        ])
        .arg(root)
        .hermetic_path(path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow");

    assert!(
        !output.status.success(),
        "codex start on a context-less phase must fail pre-flight"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no CONTEXT.md"),
        "pre-flight error must name the missing artifact\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("/gsd-discuss-phase 42"),
        "pre-flight error must include the remediation command\nstderr: {stderr}"
    );
    assert!(
        !root.join(".worktrees/phase-42").exists(),
        "pre-flight failure must not create a worktree"
    );
}

/// CR-02 (phase 20 review): a phase halted via `devflow start --until
/// <stage>` clears the supervisor record and its agent has already exited by
/// design — `Liveness::Unknown` with `agent_alive == false` sails straight
/// through the live-agent refusal, so an ordinary `devflow cleanup` (no
/// `--force`) must not delete the worktree of a phase the operator parked
/// for a later `devflow resume`. `doctor`'s `check_dead_agent`/
/// `check_dead_monitor` were already taught about `facts.stopped` in this
/// same phase; `cleanup` must recognize `state.stopped` too.
#[test]
fn cleanup_keeps_worktree_for_until_stopped_phase_without_force() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let phase = PhaseId::new(11);
    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    seed_feature_branch(root, phase);

    let wt_path = root
        .join(".worktrees")
        .join(format!("phase-{padded}", padded = phase.padded()));
    devflow_core::worktree::add(root, &wt_path, &branch, &branch, false).unwrap();

    // No agent pid file (the stage's agent has already exited normally) and
    // no supervisor record (cleared by the --until stop path) — Unknown
    // liveness, agent_alive == false. Only `state.stopped` distinguishes
    // this from a genuinely dead, safe-to-remove phase.
    let mut state = devflow_core::state::State::new(
        phase,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Auto,
        root.to_path_buf(),
    );
    state.worktree_path = Some(wt_path.clone());
    state.stopped = true;
    state.stop_reason = Some("stopped after plan completed (--until plan)".to_string());
    devflow_core::workflow::save_state(&state).unwrap();

    let fake_bin = fake_bin_dir(&[]);
    let output = common::hermetic_devflow(&home)
        .args(["cleanup"])
        .arg(root)
        .hermetic_path(path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow cleanup");

    assert!(
        output.status.success(),
        "cleanup must not error on a stopped phase — it should skip it, not fail\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("devflow resume") && combined.contains("--force"),
        "the kept-worktree message must name `devflow resume` and `--force` as the paths \
         forward, got:\n{combined}"
    );
    assert!(
        wt_path.is_dir(),
        "worktree for a --until-stopped phase must NOT be removed by a bare `devflow cleanup`"
    );
}

/// CR-02 counterpart: `--force` is the documented escape hatch — it must
/// still be able to discard a stopped phase's worktree.
#[test]
fn cleanup_force_removes_worktree_for_until_stopped_phase() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let phase = PhaseId::new(13);
    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    seed_feature_branch(root, phase);

    let wt_path = root
        .join(".worktrees")
        .join(format!("phase-{padded}", padded = phase.padded()));
    devflow_core::worktree::add(root, &wt_path, &branch, &branch, false).unwrap();

    let mut state = devflow_core::state::State::new(
        phase,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Auto,
        root.to_path_buf(),
    );
    state.worktree_path = Some(wt_path.clone());
    state.stopped = true;
    state.stop_reason = Some("stopped after plan completed (--until plan)".to_string());
    devflow_core::workflow::save_state(&state).unwrap();

    let fake_bin = fake_bin_dir(&[]);
    let output = common::hermetic_devflow(&home)
        .args(["cleanup", "--force"])
        .arg(root)
        .hermetic_path(path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow cleanup --force");

    assert!(
        output.status.success(),
        "cleanup --force must succeed on a stopped phase\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !wt_path.is_dir(),
        "cleanup --force must remove a --until-stopped phase's worktree"
    );
}

/// A fixture repo for `cleanup`: phase `phase`'s worktree under `.worktrees/` on
/// `feature/phase-NN`, and a saved state recording it, shaped by `shape`. With
/// `merged`, the branch is forked at `develop` with no commit of its own, so
/// `git branch --merged develop` lists it. Returns the worktree's path.
fn cleanup_fixture(
    root: &Path,
    phase: PhaseId,
    merged: bool,
    shape: impl FnOnce(&mut devflow_core::state::State),
) -> PathBuf {
    init_repo(root);
    add_phase(root, phase, merged, shape)
}

/// [`cleanup_fixture`]'s branch, worktree and saved state for `phase`, in a repository
/// that is already initialised, so one repository can hold more than one phase.
fn add_phase(
    root: &Path,
    phase: PhaseId,
    merged: bool,
    shape: impl FnOnce(&mut devflow_core::state::State),
) -> PathBuf {
    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    if merged {
        git(root, &["branch", &branch, "develop"]);
    } else {
        seed_feature_branch(root, phase);
    }
    let wt_path = root
        .join(".worktrees")
        .join(format!("phase-{padded}", padded = phase.padded()));
    devflow_core::worktree::add(root, &wt_path, &branch, &branch, false).unwrap();
    let mut state = devflow_core::state::State::new(
        phase,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Auto,
        root.to_path_buf(),
    );
    state.worktree_path = Some(wt_path.clone());
    shape(&mut state);
    devflow_core::workflow::save_state(&state).unwrap();
    wt_path
}

/// `devflow cleanup` (plus `extra`) on `root`, in a hermetic home.
fn run_cleanup(home: &common::HermeticHome, root: &Path, extra: &[&str]) -> Output {
    let fake_bin = fake_bin_dir(&[]);
    common::hermetic_devflow(home)
        .arg("cleanup")
        .args(extra)
        .arg(root)
        .hermetic_path(path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow cleanup")
}

/// The stop reason a finished run records (D-35), as the CLI writes it.
const FINISHED_REASON: &str = "finished: Validate passed; the worktree and branch are kept";
/// The stop reason a `--until plan` halt records.
const UNTIL_PLAN_REASON: &str = "stopped after plan completed (--until plan)";

/// OPS-02 / D-35: `cleanup` says why it keeps a worktree, and each case names only
/// its own reason: a finished run gives its stop reason and `--force` (never
/// `resume`, which refuses a finished run); an `--until plan` halt gives that halt's
/// reason and `resume`; a live run, whose runner lock another process holds, is
/// refused with a non-zero exit naming the runner and `devflow stop`.
#[test]
fn cleanup_names_why_it_keeps_a_worktree() {
    // A finished run.
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let phase = PhaseId::new(21);
    let wt = cleanup_fixture(repo.path(), phase, false, |state| {
        state.stopped = true;
        state.finished = true;
        state.stop_reason = Some(FINISHED_REASON.to_string());
    });
    let out = run_cleanup(&home, repo.path(), &[]);
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{text}");
    assert!(wt.is_dir(), "a finished run's worktree is kept: {text}");
    assert!(
        text.contains(FINISHED_REASON) && text.contains("--force"),
        "names the finished run's reason and --force: {text}"
    );
    for other in ["devflow resume", "--until", "runner holds"] {
        assert!(!text.contains(other), "finished names {other:?}: {text}");
    }

    // An `--until plan` halt.
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let phase = PhaseId::new(22);
    let wt = cleanup_fixture(repo.path(), phase, false, |state| {
        state.stopped = true;
        state.stop_until = Some(Stage::Plan);
        state.stop_reason = Some(UNTIL_PLAN_REASON.to_string());
    });
    let out = run_cleanup(&home, repo.path(), &[]);
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{text}");
    assert!(wt.is_dir(), "a halted run's worktree is kept: {text}");
    assert!(
        text.contains(UNTIL_PLAN_REASON) && text.contains("devflow resume --phase 22"),
        "names the halt's reason and resume: {text}"
    );
    for other in ["finished", "runner holds"] {
        assert!(!text.contains(other), "the halt names {other:?}: {text}");
    }

    // A live run (D-07, D-12a): another process holds the phase's runner lock. It is
    // refused with and without `--force`.
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let phase = PhaseId::new(23);
    let wt = cleanup_fixture(repo.path(), phase, false, |_| {});
    let holder = RunnerLockHolder::hold(repo.path(), phase);
    for args in [&[][..], &["--force"][..]] {
        let out = run_cleanup(&home, repo.path(), args);
        let text = String::from_utf8_lossy(&out.stdout).to_string()
            + &String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success(),
            "{args:?}: a live run is refused: {text}"
        );
        assert!(
            wt.is_dir(),
            "{args:?}: a live run's worktree is kept: {text}"
        );
        assert!(
            text.contains("its runner holds the runner lock")
                && text.contains(&format!("(pid {})", holder.pid()))
                && text.contains("devflow stop --phase 23"),
            "{args:?}: names the live runner and stop: {text}"
        );
        for other in ["finished", "--until", "devflow resume"] {
            assert!(
                !text.contains(other),
                "{args:?}: the live refusal names {other:?}: {text}"
            );
        }
    }
    drop(holder);
}

/// OPS-02 (999.213): `cleanup --force` removes the run's state file together with
/// its worktree, and only that phase's state.
#[test]
fn cleanup_force_removes_the_run_state_with_the_worktree() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(24);
    let wt = cleanup_fixture(root, phase, false, |state| {
        state.stopped = true;
        state.finished = true;
        state.stop_reason = Some(FINISHED_REASON.to_string());
    });
    // Another phase's state, with no worktree: `--force` must leave it alone.
    let other = PhaseId::new(25);
    let other_state = devflow_core::state::State::new(
        other,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Auto,
        root.to_path_buf(),
    );
    devflow_core::workflow::save_state(&other_state).unwrap();
    let state_file = devflow_core::workflow::state_path(root, phase);
    assert!(state_file.is_file(), "premise: the state file exists");

    let out = run_cleanup(&home, root, &["--force"]);
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{text}");
    assert!(!wt.is_dir(), "the worktree is removed: {text}");
    assert!(
        !state_file.exists(),
        "the run's state file is removed with its worktree: {text}"
    );
    assert!(
        devflow_core::workflow::state_path(root, other).is_file(),
        "another phase's state is untouched: {text}"
    );
}

/// `cleanup` never deletes a merged branch that a kept worktree still has checked
/// out: it skips the branch rather than leaving the refusal to git.
#[test]
fn cleanup_keeps_the_branch_of_a_kept_worktree() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(26);
    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    let wt = cleanup_fixture(root, phase, true, |state| {
        state.stopped = true;
        state.finished = true;
        state.stop_reason = Some(FINISHED_REASON.to_string());
    });
    let merged = git(root, &["branch", "--merged", "develop"]);
    assert!(
        String::from_utf8_lossy(&merged.stdout).contains(&branch),
        "premise: the branch reads as merged"
    );

    let out = run_cleanup(&home, root, &[]);
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{text}");
    assert!(wt.is_dir(), "premise: the worktree is kept: {text}");
    let listed = git(root, &["branch", "--list", &branch]);
    assert!(
        String::from_utf8_lossy(&listed.stdout).contains(&branch),
        "the kept worktree's branch still exists: {text}"
    );
    assert!(
        !text.contains(&format!("merged branch {branch}")),
        "cleanup does not try to delete the kept worktree's branch: {text}"
    );
}

/// Whether `branch` exists in `root`, and its tip when it does.
fn branch_tip(root: &Path, branch: &str) -> Option<String> {
    let out = devflow_core::test_support::git_command(root)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .output()
        .expect("spawn git rev-parse");
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// A finished run's branch holds the only copy of its work until it is closed out, so
/// `cleanup --force` keeps a branch with commits not merged into the base: it still
/// removes the worktree and the run's state, and names the branch, why it was kept and
/// the command that deletes it.
#[test]
fn cleanup_force_keeps_an_unmerged_branch_and_names_the_delete_command() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(27);
    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    let wt = cleanup_fixture(root, phase, false, |state| {
        state.stopped = true;
        state.finished = true;
        state.stop_reason = Some(FINISHED_REASON.to_string());
    });
    let tip = branch_tip(root, &branch).expect("premise: the branch exists");
    let state_file = devflow_core::workflow::state_path(root, phase);
    assert!(state_file.is_file(), "premise: the state file exists");

    let out = run_cleanup(&home, root, &["--force"]);
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{text}");
    assert!(!wt.is_dir(), "the worktree is removed: {text}");
    assert!(!state_file.exists(), "the run's state is removed: {text}");
    assert_eq!(
        branch_tip(root, &branch).as_deref(),
        Some(tip.as_str()),
        "the unmerged branch is kept at its tip: {text}"
    );
    let kept_line = text
        .lines()
        .find(|line| line.contains(&format!("branch {branch} kept")))
        .unwrap_or_else(|| panic!("a line names the kept branch: {text}"));
    assert!(
        kept_line.contains("1 commit not merged into `develop`"),
        "the line says why, with the count: {kept_line}"
    );
    assert!(
        kept_line.contains(&format!("git branch -D {branch}")),
        "the line names the delete command: {kept_line}"
    );
}

/// Control for the test above: `cleanup --force` still deletes a branch with no
/// commit beyond the base.
#[test]
fn cleanup_force_deletes_a_merged_branch() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(28);
    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    let wt = cleanup_fixture(root, phase, true, |state| {
        state.stopped = true;
        state.finished = true;
        state.stop_reason = Some(FINISHED_REASON.to_string());
    });
    assert!(
        branch_tip(root, &branch).is_some(),
        "premise: the branch exists"
    );

    let out = run_cleanup(&home, root, &["--force"]);
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{text}");
    assert!(!wt.is_dir(), "the worktree is removed: {text}");
    assert_eq!(
        branch_tip(root, &branch),
        None,
        "the merged branch is deleted: {text}"
    );
    assert!(!text.contains("git branch -D"), "{text}");
}

/// `devflow start` with `--force` (plus `extra`) for phase 12 over `root`, in a
/// hermetic home, with its exit status left to the caller. A reaper is bound before the
/// launch, so a start that does launch has its run ended when the test ends.
fn force_start_phase_12(
    home: &common::HermeticHome,
    root: &Path,
    extra: &[&str],
) -> (Output, common::LaunchedRunReaper) {
    let reaper = common::LaunchedRunReaper::for_home(home, root, PhaseId::new(12));
    let fake_bin = fake_bin_dir(&[(
        "codex",
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
    )]);
    let output = common::hermetic_devflow(home)
        .args([
            "start",
            "--phase",
            "12",
            "--agent",
            "codex",
            "--mode",
            "supervise",
            "--force",
        ])
        .args(extra)
        .arg(root)
        .hermetic_path(path_with_fake_bin(&fake_bin.path))
        .env("DEVFLOW_TEST_ROOT", root)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "60")
        .current_dir(root)
        .output()
        .expect("run devflow start --force");
    (output, reaper)
}

/// `start --force` cannot recreate a phase branch with commits not merged into the base
/// without deleting them, so it refuses before removing anything: the worktree and the
/// branch are both left, and the refusal names the branch and both ways on.
#[test]
fn start_force_refuses_over_an_unmerged_phase_branch() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let phase = PhaseId::new(12);
    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    seed_feature_branch(root, phase);
    let wt = root.join(".worktrees/phase-12");
    devflow_core::worktree::add(root, &wt, &branch, &branch, false).unwrap();
    let tip = branch_tip(root, &branch).expect("premise: the branch exists");

    let (out, _reaper) = force_start_phase_12(&home, root, &[]);
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "start --force is refused: {text}");
    assert!(wt.is_dir(), "the worktree is left in place: {text}");
    assert_eq!(
        branch_tip(root, &branch).as_deref(),
        Some(tip.as_str()),
        "the branch is left at its tip: {text}"
    );
    assert!(
        text.contains(&format!(
            "`{branch}` has 1 commit not merged into `develop`"
        )),
        "names the branch and its unmerged commit: {text}"
    );
    assert!(
        text.contains(&format!("git branch -D {branch}")),
        "names the delete command: {text}"
    );
}

/// The `--no-worktree` arm of the test above: `start --force` would reset the branch to
/// the base in place, so it refuses an unmerged one too and leaves it at its tip.
#[test]
fn start_force_no_worktree_refuses_over_an_unmerged_phase_branch() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let phase = PhaseId::new(12);
    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    seed_feature_branch(root, phase);
    let tip = branch_tip(root, &branch).expect("premise: the branch exists");

    let (out, _reaper) = force_start_phase_12(&home, root, &["--no-worktree"]);
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "start --force is refused: {text}");
    assert_eq!(
        branch_tip(root, &branch).as_deref(),
        Some(tip.as_str()),
        "the branch is left at its tip: {text}"
    );
    assert!(
        text.contains(&format!(
            "`{branch}` has 1 commit not merged into `develop`"
        )),
        "names the branch and its unmerged commit: {text}"
    );
}

/// 20b instance 1 (probe 20b/idempotency): `cleanup` run twice succeeds —
/// the second run finds the worktree already gone and does not error.
#[test]
fn cleanup_is_idempotent_when_worktree_already_removed() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let phase = PhaseId::new(10);
    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    seed_feature_branch(root, phase);

    let wt_path = root
        .join(".worktrees")
        .join(format!("phase-{padded}", padded = phase.padded()));
    devflow_core::worktree::add(root, &wt_path, &branch, &branch, false).unwrap();

    // Dead agent, dead monitor (a supervisor record whose socket does not
    // exist, so the probe reads GONE) — a genuinely dead phase, safe for
    // cleanup to proceed.
    let mut state = devflow_core::state::State::new(
        phase,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Auto,
        root.to_path_buf(),
    );
    state.worktree_path = Some(wt_path.clone());
    state.supervisor = Some(devflow_core::state::SupervisorRecord::new(
        root.join("dead-monitor-never-bound.sock"),
        "deadmonitor00000".to_string(),
        0x7FFF_FFFE,
        1,
        String::new(),
    ));
    devflow_core::workflow::save_state(&state).unwrap();

    let fake_bin = fake_bin_dir(&[]);
    let first = common::hermetic_devflow(&home)
        .args(["cleanup", "--force"])
        .arg(root)
        .hermetic_path(path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow cleanup (first)");
    assert!(
        first.status.success(),
        "first cleanup of a genuinely-dead phase must succeed\nstderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(!wt_path.is_dir(), "worktree must be removed on first run");

    let second = common::hermetic_devflow(&home)
        .args(["cleanup", "--force"])
        .arg(root)
        .hermetic_path(path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow cleanup (second)");
    assert!(
        second.status.success(),
        "second cleanup run must find the worktree already gone and not error\nstderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
}
/// A `pi` stub that answers the preflight health check (`pi auth check` →
/// `{"status":"ready"}`) and the capability probe (`pi list` → the vetted
/// `@bacnh85/pi-subagent` package), then runs `launch` for the real
/// `pi -p --no-approve` stage launch. The health check probes the provider in
/// the real `~/.pi/agent/settings.json`, so the stub must classify as `ready`
/// or preflight gates and the stage never runs.
fn pi_stub(launch: &str) -> String {
    format!(
        "#!/bin/sh\n\
         case \"$1\" in\n\
           auth) printf '{{\"status\":\"ready\"}}\\n'; exit 0 ;;\n\
           list) printf 'npm:@bacnh85/pi-subagent (user)\\n'; exit 0 ;;\n\
         esac\n\
         {launch}\n"
    )
}

/// Wait until a phase's persisted state reports `gate_pending == true` — the
/// never-silent failure/review gate (WR-11) that fires when a stage's agent
/// outcome does not advance. Polls rather than reading once, since the
/// detached monitor chain advances asynchronously.
/// Wait until a phase's chain settles in a state the tests can reap from:
/// either the never-silent gate fired (`gate_pending`) or the machine
/// stopped (`--until` / completion). Returns that state — whose
/// `supervisor` record names the LAST monitor the chain spawned, the one a
/// `MonitorReapGuard` must capture (a guard bound to an early state read
/// reaps an already-exited pid and leaks the gate-waiting monitor, HYG-01).
fn wait_for_settled(root: &Path, phase: PhaseId) -> devflow_core::state::State {
    for _ in 0..400 {
        if let Ok(state) = load_state(root, phase)
            && (state.gate_pending || state.stopped)
        {
            return state;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out waiting for phase {phase} to settle (gate or stop)");
}

fn wait_for_gate(root: &Path, phase: PhaseId) -> devflow_core::state::State {
    for _ in 0..400 {
        if let Ok(state) = load_state(root, phase)
            && state.gate_pending
        {
            return state;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out waiting for phase {phase} to report gate_pending");
}

/// A Pi run that exits 0 without emitting a DEVFLOW_RESULT marker must not
/// advance a commit-gated stage. Plan, the first stage and commit-gated,
/// runs the marker-less stub, which produces no commits and no marker, so it
/// is `Failed` and gates instead of advancing to Code.
#[test]
fn pi_marker_less_run_does_not_advance() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[("pi", &pi_stub("printf 'fake pi, no marker\\n'\nexit 0\n"))]);

    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "07",
            "--agent",
            "pi",
            "--mode",
            "supervise",
        ],
    );
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(7));

    let state = wait_for_gate(root, PhaseId::new(7));
    assert_eq!(
        state.stage,
        Stage::Plan,
        "a marker-less run must not advance past the commit-gated Plan stage"
    );
    assert!(state.gate_pending, "the never-silent gate must have fired");
    let _reap = MonitorReapGuard::after_launch(&state);
}

/// A Pi run that exits non-zero must not advance its stage: the failed exit is
/// not Success, so the pipeline gates instead of advancing.
#[test]
fn pi_nonzero_exit_does_not_advance() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[("pi", &pi_stub("printf 'fake pi, no marker\\n'\nexit 1\n"))]);

    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "07",
            "--agent",
            "pi",
            "--mode",
            "supervise",
        ],
    );
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(7));

    let state = wait_for_gate(root, PhaseId::new(7));
    assert_eq!(
        state.stage,
        Stage::Plan,
        "a non-zero-exit run must not advance its stage"
    );
    assert!(state.gate_pending, "the never-silent gate must have fired");
    let _reap = MonitorReapGuard::after_launch(&state);
}

/// A hung Pi process (never exits) is surfaced as alive by monitor liveness —
/// the stage never advances while it runs (no false Success) — and once killed,
/// the monitor reaps it and gates (never a silent advance).
#[test]
fn pi_hung_process_is_detected_not_left_running() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let aux = tempfile::tempdir().unwrap();
    let pid_path = aux.path().join("agent.pid");
    let fake_bin = fake_bin_dir(&[("pi", &pi_stub(&recording_sleeper(&pid_path)))]);

    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(7));
    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "07",
            "--agent",
            "pi",
            "--mode",
            "supervise",
        ],
    );

    let pid = wait_for_pid(&pid_path);

    // The hung process must be surfaced as alive, not silently reaped or
    // declared complete.
    assert!(
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .unwrap()
            .success(),
        "the hung pi process must still be alive"
    );
    let state = load_state(root, PhaseId::new(7)).expect("load state");
    assert_eq!(
        state.stage,
        Stage::Plan,
        "the stage must not advance while pi is hung"
    );

    // Kill the hung process; the monitor reaps it and gates — never advances.
    assert!(
        Command::new("kill")
            .arg(pid.to_string())
            .status()
            .unwrap()
            .success(),
        "should be able to kill the hung pi process"
    );
    let gated = wait_for_gate(root, PhaseId::new(7));
    assert_eq!(
        gated.stage,
        Stage::Plan,
        "a hung-then-killed run must not advance its stage"
    );
    let _reap = MonitorReapGuard::after_launch(&gated);
}

// ---------------------------------------------------------------------------
// Phase 41 Task 8 (ANTG-03, F4, codex-3, antigravity notice (a)): the
// Antigravity transport, end to end through `devflow start --agent
// antigravity` with a stubbed `agy` on PATH. The canary-aware stub answers
// the `DEVFLOW_DELIVERY_CANARY_` turn (AntigravityCanaryLauncher at Plan),
// schema-checks every turn (codex-3), and then behaves per mode.
// ---------------------------------------------------------------------------

/// The canary-aware `agy` stub (F4).
///
/// Three behaviours, selected by `mode`. EVERY variant first answers a
/// `*DEVFLOW_DELIVERY_CANARY_*` turn (the common prefix; the enum is named
/// without it) by echoing the token inside an antigravity-shaped
/// `event:result.response`, so `run_delivery_canary` reaches `Confirmed` at
/// Plan; then:
/// - `MarkerStream`: emits a marker stream for real stage turns — the happy
///   path.
/// - `Quiet`: exits 0 with NO events — the marker-less control (ANTG-03).
/// - `InitOnly`: emits `init` + `step_update` but NO `result` — the
///   discrimination control (transport alone never advances a commit-gated
///   stage).
///
/// Schema check (codex-3): EVERY turn must parse as an event-key user line
/// (`"event":"user"`); a Claude `type`-form turn exits 92 with a diagnostic —
/// an implementation that left the monitor's writer on `user_turn_line`
/// FAILS here, not just in the unit helper.
fn antigravity_stub(mode: StubMode) -> String {
    const SCHEMA_AND_CANARY: &str = r#"#!/bin/sh
IFS= read -r turn || exit 91
printf '%s' "$turn" | grep -q '"event":"user"' || { echo 'NON_EVENT_KEY_TURN' >&2; exit 92; }
case "$turn" in
  *DEVFLOW_DELIVERY_CANARY_*)
    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
    printf '%s\n' '{"event":"init","model":"stub"}'
    printf '%s\n' '{"event":"result","result":{"status":"SUCCESS","response":"canary: '"$token"'\nDEVFLOW_RESULT: {\"status\":\"success\"}"}}'
    exit 0 ;;
esac
"#;
    let body = match mode {
        StubMode::MarkerStream => {
            r#"printf '%s\n' '{"event":"init","model":"stub","inputFormat":"stream-json","outputFormat":"stream-json"}'
printf '%s\n' '{"event":"step_update","index":0,"text_delta":"working"}'
printf '%s\n' '{"event":"result","result":{"status":"SUCCESS","response":"DEVFLOW_RESULT: {\"status\":\"success\"}"}}'
exit 0
"#
        }
        StubMode::Quiet => "exit 0\n",
        StubMode::InitOnly => {
            r#"printf '%s\n' '{"event":"init","model":"stub","inputFormat":"stream-json","outputFormat":"stream-json"}'
printf '%s\n' '{"event":"step_update","index":0,"text_delta":"working"}'
exit 0
"#
        }
    };
    format!("{SCHEMA_AND_CANARY}{body}")
}

#[derive(Clone, Copy)]
enum StubMode {
    MarkerStream,
    Quiet,
    InitOnly,
}

/// Reap the detached `__monitor` wrapper a `devflow start` run left behind
/// (phase 41 Task 8, antigravity notice (a)): the integration-suite analogue
/// of the binary crate's `ReapMonitorOnDrop`, built on the PUBLIC
/// `devflow_core::agent` surface (integration tests cannot reach
/// `devflow-cli`'s test_support). TERM->KILL escalation with VERIFIED death,
/// keyed to the pid of the supervisor `state.supervisor` records (read from
/// its socket by [`live_supervisor_pid`]), bound strictly after the final
/// `&mut State` use. 41-02 Task 1 turns this into the systematic pass.
struct MonitorReapGuard {
    pid: Option<u32>,
}

/// The pid of the supervisor `state` records, from its socket's ping reply.
/// The state carries no monitor pid (50-22); before that field went, it held
/// this same pid. `None` when the state has no record or its socket does not
/// answer as alive: nothing is left for a guard to reap.
fn live_supervisor_pid(state: &devflow_core::state::State) -> Option<u32> {
    let record = state.supervisor.as_ref()?;
    match devflow_core::supervisor::probe(&record.socket_path) {
        Ok(devflow_core::supervisor::Liveness::Alive(reply)) => Some(reply.supervisor_pid),
        _ => None,
    }
}

impl MonitorReapGuard {
    fn after_launch(state: &devflow_core::state::State) -> Self {
        // Register the pid THIS guard is responsible for (the settled-state
        // monitor it will reap), and mark a guard in flight for the audit's
        // ordering barrier. `state` must be the SETTLED state
        // (`wait_for_settled` / `wait_for_gate`), whose supervisor record
        // names the chain's LAST monitor — the one that would leak if not
        // reaped.
        let pid = live_supervisor_pid(state);
        if let Some(pid) = pid {
            registry().lock().unwrap().insert(pid);
            ACTIVE_GUARDS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        Self { pid }
    }
}

impl Drop for MonitorReapGuard {
    fn drop(&mut self) {
        let Some(pid) = self.pid else {
            return;
        };
        devflow_core::agent::terminate_and_verify(
            pid,
            devflow_core::agent::TERMINATE_VERIFY_WAIT,
            devflow_core::agent::TERMINATE_VERIFY_POLL,
        );
        // Deregister after the verified reap: the suite audit's empty-registry
        // assertion then means "every monitor a guard was responsible for was
        // verified reaped".
        registry().lock().unwrap().remove(&pid);
        // Ordering-barrier bookkeeping, regardless of the reap verdict: the
        // guard is no longer in flight, and a reap was attempted.
        ACTIVE_GUARDS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        REAPED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if devflow_core::agent::agent_running(pid) {
            if std::thread::panicking() {
                use std::io::Write as _;
                let _ = writeln!(
                    std::io::stderr(),
                    "MonitorReapGuard: monitor wrapper pid {pid} still alive after reap \
                     during an unwind — not re-panicking because a panic is already in flight"
                );
            } else {
                panic!(
                    "monitor wrapper pid {pid}, spawned by this test's own start run, must be \
                     verified dead after reaping — not merely assumed dead"
                );
            }
        }
    }
}

/// ANTG-03: a stubbed `agy` that exits 0 with no stream events must not
/// advance a COMMIT-GATED stage. Plan — a commit-gated stage — produces no
/// marker and no commits, so it gates instead of advancing to Code.
#[test]
fn marker_less_antigravity_never_advances() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[("agy", &antigravity_stub(StubMode::Quiet))]);

    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "07",
            "--agent",
            "antigravity",
            "--mode",
            "supervise",
        ],
    );
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(7));

    let state = wait_for_gate(root, PhaseId::new(7));
    let _reap = MonitorReapGuard::after_launch(&state);
    assert_eq!(
        state.stage,
        Stage::Plan,
        "a marker-less antigravity run must not advance past the commit-gated Plan stage"
    );
    assert!(state.gate_pending, "the never-silent gate must have fired");
}

/// The happy path: a stubbed `agy` emitting ANTIGRAVITY-shaped events with
/// the `DEVFLOW_RESULT` marker inside `event:result.response` advances past
/// the commit gate. `--until plan` halts right after Plan completes, so the
/// assertion is: Plan RAN (the commit gate passed) and the machine is
/// stopped, NOT gated.
#[test]
fn antigravity_parses_devflow_result_from_stream() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[("agy", &antigravity_stub(StubMode::MarkerStream))]);

    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "07",
            "--agent",
            "antigravity",
            "--mode",
            "supervise",
            "--until",
            "plan",
        ],
    );
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(7));

    let state = wait_for_stopped(root, PhaseId::new(7));
    let _reap = MonitorReapGuard::after_launch(&state);
    assert_eq!(
        state.stage,
        Stage::Plan,
        "the persisted stage must be the COMPLETED target (Plan), proving Plan ran"
    );
    assert!(
        !state.gate_pending,
        "a marker-bearing antigravity stream must pass the commit gate, not gate"
    );
}

/// Discrimination control: a stream that emits `init` + `step_update` but NO
/// `result`/marker must still gate at Plan — the TRANSPORT alone never
/// advances a commit-gated stage, only the marker does (ANTG-03).
#[test]
fn antigravity_init_without_marker_gates_at_plan() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[("agy", &antigravity_stub(StubMode::InitOnly))]);

    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "07",
            "--agent",
            "antigravity",
            "--mode",
            "supervise",
        ],
    );
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(7));

    let state = wait_for_gate(root, PhaseId::new(7));
    let _reap = MonitorReapGuard::after_launch(&state);
    assert_eq!(
        state.stage,
        Stage::Plan,
        "a stream with events but no marker must not advance past Plan"
    );
    assert!(state.gate_pending);
}

// ---------------------------------------------------------------------------
// Phase 41 41-02 Task 1 (HYG-01, codex-4): the suite-level registered-PID
// audit. Per-test Drop guards cannot detect an unguarded test; the registry
// + audit make the leak provable, and the intentional opt-out proves the
// gate can fail.
// ---------------------------------------------------------------------------

/// The suite audit: after every monitor-spawning test has run (and reaped,
/// or seen its monitor exit naturally), the registry must DRAIN. Polls
/// bounded so a slow legitimate test does not flake; a genuinely leaked
/// monitor keeps the registry non-empty and the audit panics naming the pid.
///
/// The audit reads process-wide counters the other tests in this binary bump,
/// so it only means something when they share its process, as under
/// `cargo test` (what `scripts/check.sh` runs). cargo-nextest runs every test
/// in its own process and sets `NEXTEST`, where `REAPED` is always 0 and the
/// audit could only time out; there it returns without asserting.
#[test]
fn suite_reap_audit() {
    if std::env::var_os("NEXTEST").is_some() {
        return;
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    // Ordering barrier: wait for a SUSTAINED clean state, not a single empty
    // poll. A clean state means: no guard in flight (ACTIVE_GUARDS == 0), at
    // least one reap has happened (so an empty registry is not a vacuous
    // "nothing ran" pass), and the registry is empty with no registered pid
    // alive. The state must hold continuously for a quiescence window, so the
    // audit cannot race a still-running test into a false pass (a test whose
    // guard has bound but not yet dropped keeps ACTIVE_GUARDS > 0).
    let mut clean_since = None;
    loop {
        let in_flight = ACTIVE_GUARDS.load(std::sync::atomic::Ordering::SeqCst);
        let reaped = REAPED.load(std::sync::atomic::Ordering::SeqCst);
        // Scope the registry lock to the read only — do not hold it across the
        // sleep below, or the audit serializes against every guard's bind/drop.
        let (alive, registered) = {
            let reg = registry().lock().unwrap();
            (registered_monitors_alive(&reg), reg.len())
        };
        let clean = in_flight == 0 && reaped > 0 && registered == 0 && alive.is_empty();
        if clean {
            match clean_since {
                None => clean_since = Some(std::time::Instant::now()),
                Some(t)
                    if std::time::Instant::now().duration_since(t) >= Duration::from_secs(2) =>
                {
                    return;
                }
                Some(_) => {}
            }
        } else {
            clean_since = None;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "monitor(s) leaked after the suite: alive={alive:?}, in_flight={in_flight}, \
             registered={registered}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// codex-4: a run process the suite cannot account for is DETECTABLE. A start
/// leaves its runner ALIVE while its agent hangs; the test registers that pid
/// by hand, as a guard would have, proves the detection helper FAILS against it
/// (the gate can redden), then deregisters it and lets the reaper end the
/// runner.
#[test]
fn unguarded_monitor_is_detected_by_the_registry() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    // A HANGING pi keeps the runner's Plan stage alive past the launch — the
    // "leaked" shape this control must be able to see.
    let fake_bin = fake_bin_dir(&[("pi", &pi_stub("exec sleep 30\n"))]);

    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(7));
    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "07",
            "--agent",
            "pi",
            "--mode",
            "supervise",
        ],
    );

    let pid = recorded_runner_pid(&home, root, PhaseId::new(7))
        .expect("start launched a runner the fake systemd-run recorded");
    assert!(
        common::runs_the_runner_verb(pid),
        "premise: the runner stays alive while its agent hangs — nothing reaps it"
    );

    // Simulate what a guard's after_launch would have registered (the leak).
    registry().lock().unwrap().insert(pid);
    // The detection helper MUST see it — the audit can fail.
    let alive = registered_monitors_alive(&registry().lock().unwrap());
    assert!(
        alive.contains(&pid),
        "a live registered runner must be detected: {alive:?}"
    );

    // Deregister; the reaper ends the runner when the test returns.
    registry().lock().unwrap().remove(&pid);
}

// ---------------------------------------------------------------------------
// Phase 42 Task 4 (HRMS-03, D-03): Hermes transport integration tests.
// ---------------------------------------------------------------------------

fn hermes_stub(launch: &str) -> String {
    format!(
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then
    printf 'hermes 0.5.0\n'
    exit 0
fi
if [ "$1" = "tools" ] && [ "$2" = "list" ]; then
    printf 'Available Toolsets:\n  ✓ enabled delegation 👥 Task Delegation\n  ✓ enabled terminal 💻 Terminal Execution\n'
    exit 0
fi
{launch}
"#
    )
}

/// HRMS-03: a stubbed `hermes` that exits 0 with no marker must not advance a
/// commit-gated stage: Plan gates.
#[test]
fn hermes_marker_less_run_does_not_advance() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[(
        "hermes",
        &hermes_stub("printf 'fake hermes, no marker\\n'\nexit 0\n"),
    )]);

    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "07",
            "--agent",
            "hermes",
            "--mode",
            "supervise",
        ],
    );
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(7));

    let state = wait_for_gate(root, PhaseId::new(7));
    assert_eq!(
        state.stage,
        Stage::Plan,
        "a marker-less hermes run must not advance past the commit-gated Plan stage"
    );
    assert!(state.gate_pending, "the never-silent gate must have fired");
    let _reap = MonitorReapGuard::after_launch(&state);
}

/// HRMS-03: a Hermes run that exits non-zero must not advance its stage.
#[test]
fn hermes_nonzero_exit_does_not_advance() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[(
        "hermes",
        &hermes_stub("printf 'fake hermes error\\n'\nexit 1\n"),
    )]);

    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "07",
            "--agent",
            "hermes",
            "--mode",
            "supervise",
        ],
    );
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(7));

    let state = wait_for_gate(root, PhaseId::new(7));
    assert_eq!(
        state.stage,
        Stage::Plan,
        "a non-zero-exit hermes run must not advance its stage"
    );
    assert!(state.gate_pending, "the never-silent gate must have fired");
    let _reap = MonitorReapGuard::after_launch(&state);
}

/// HRMS-03: a hung Hermes process is detected as alive by monitor liveness,
/// does not falsely advance, and gates when killed.
#[test]
fn hermes_hung_process_is_detected_not_left_running() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let aux = tempfile::tempdir().unwrap();
    let pid_path = aux.path().join("agent.pid");
    let fake_bin = fake_bin_dir(&[("hermes", &hermes_stub(&recording_sleeper(&pid_path)))]);

    let _reaper = common::LaunchedRunReaper::for_home(&home, root, PhaseId::new(7));
    run_devflow(
        &home,
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "07",
            "--agent",
            "hermes",
            "--mode",
            "supervise",
        ],
    );

    let pid = wait_for_pid(&pid_path);

    assert!(
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .unwrap()
            .success(),
        "the hung hermes process must still be alive"
    );
    let state = load_state(root, PhaseId::new(7)).expect("load state");
    assert_eq!(
        state.stage,
        Stage::Plan,
        "the stage must not advance while hermes is hung"
    );

    assert!(
        Command::new("kill")
            .arg(pid.to_string())
            .status()
            .unwrap()
            .success(),
        "should be able to kill the hung hermes process"
    );
    let gated = wait_for_gate(root, PhaseId::new(7));
    assert_eq!(
        gated.stage,
        Stage::Plan,
        "a hung-then-killed hermes run must not advance its stage"
    );
    let _reap = MonitorReapGuard::after_launch(&gated);
}

/// A finished run, as [`cleanup_fixture`]'s `shape`.
fn finished(state: &mut devflow_core::state::State) {
    state.stopped = true;
    state.finished = true;
    state.stop_reason = Some(FINISHED_REASON.to_string());
}

/// `devflow discard --phase <phase>` (plus `extra`) on `root`, in a hermetic home.
fn run_discard(home: &common::HermeticHome, root: &Path, phase: PhaseId, extra: &[&str]) -> Output {
    let fake_bin = fake_bin_dir(&[]);
    common::hermetic_devflow(home)
        .args(["discard", "--phase", &phase.to_string()])
        .args(extra)
        .arg(root)
        .hermetic_path(path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow discard")
}

/// Everything `devflow discard` may remove for one phase, as it is on disk: read before
/// and after a run, so a refusal is shown to have removed none of it and a removal is
/// shown to have left another phase's alone.
#[derive(Debug, PartialEq)]
struct PhaseItems {
    /// Every file below `.worktrees/phase-NN` with its bytes, uncommitted ones included.
    /// `None` when nothing is at that path.
    worktree: Option<Vec<(PathBuf, Vec<u8>)>>,
    /// The bytes of `.devflow/state-NN.json`, or `None` with no such file.
    state: Option<Vec<u8>>,
    /// The tip of `feature/phase-NN`, or `None` with no such branch.
    branch_tip: Option<String>,
}

impl PhaseItems {
    /// A phase with no worktree, no state and no branch.
    const NONE: Self = Self {
        worktree: None,
        state: None,
        branch_tip: None,
    };
}

fn phase_items(root: &Path, phase: PhaseId) -> PhaseItems {
    let worktree = devflow_core::worktree::phase_path(root, phase);
    PhaseItems {
        worktree: worktree.exists().then(|| {
            common::entries_under(&worktree)
                .into_iter()
                .map(|file| {
                    let bytes = fs::read(&file).expect("read a worktree file");
                    (file, bytes)
                })
                .collect()
        }),
        state: fs::read(devflow_core::workflow::state_path(root, phase)).ok(),
        branch_tip: branch_tip(
            root,
            &format!("feature/phase-{padded}", padded = phase.padded()),
        ),
    }
}

/// The phase a discard of any other phase must leave alone.
const BYSTANDER_PHASE: PhaseId = PhaseId::new(90);
/// A branch no phase owns, holding a commit that is not in `develop`.
const BYSTANDER_BRANCH: &str = "keep/unrelated";

/// What a discard of another phase must leave alone: [`BYSTANDER_PHASE`]'s worktree
/// (with an uncommitted file), state and unmerged branch, and [`BYSTANDER_BRANCH`].
struct Bystanders {
    phase: PhaseItems,
    branch_tip: String,
}

/// Add the bystanders to an initialised repository and record them.
fn add_bystanders(root: &Path) -> Bystanders {
    let worktree = add_phase(root, BYSTANDER_PHASE, false, finished);
    fs::write(worktree.join("scratch.txt"), "the bystander's work\n").unwrap();
    let phase_branch = format!("feature/phase-{}", BYSTANDER_PHASE.padded());
    git(root, &["branch", BYSTANDER_BRANCH, &phase_branch]);
    let phase = phase_items(root, BYSTANDER_PHASE);
    assert!(
        phase.worktree.is_some() && phase.state.is_some() && phase.branch_tip.is_some(),
        "premise: the bystander phase has a worktree, a state and a branch"
    );
    Bystanders {
        phase,
        branch_tip: branch_tip(root, BYSTANDER_BRANCH).expect("premise: the unrelated branch"),
    }
}

impl Bystanders {
    /// Assert that the bystanders are exactly as [`add_bystanders`] recorded them.
    fn assert_untouched(&self, root: &Path, text: &str) {
        assert_eq!(
            phase_items(root, BYSTANDER_PHASE),
            self.phase,
            "another phase's worktree, state and branch are untouched: {text}"
        );
        assert_eq!(
            branch_tip(root, BYSTANDER_BRANCH).as_deref(),
            Some(self.branch_tip.as_str()),
            "an unrelated branch is untouched: {text}"
        );
    }
}

/// A discard's stdout and its stdout followed by its stderr.
fn discard_text(out: &Output) -> (String, String) {
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let text = format!("{stdout}{}", String::from_utf8_lossy(&out.stderr));
    (stdout, text)
}

/// The item lines of a discard's listing: the indented lines of its stdout.
fn discard_listing(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|line| line.starts_with("  "))
        .collect()
}

/// D-09.2: `devflow discard --phase N` lists the worktree, its uncommitted changes, the
/// run state and the branch, then removes exactly those: another phase's worktree, state
/// and branch and an unrelated branch are left as they were.
#[test]
fn discard_removes_a_finished_runs_worktree_state_and_merged_branch() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    let phase = PhaseId::new(31);
    let worktree = cleanup_fixture(&root, phase, true, finished);
    fs::write(worktree.join("scratch.txt"), "uncommitted\n").unwrap();
    let bystanders = add_bystanders(&root);
    let before = phase_items(&root, phase);
    assert!(
        before.worktree.is_some() && before.state.is_some() && before.branch_tip.is_some(),
        "premise: the phase has a worktree, a state and a branch"
    );

    let out = run_discard(&home, &root, phase, &[]);
    let (stdout, text) = discard_text(&out);
    assert!(out.status.success(), "{text}");

    let listing = discard_listing(&stdout);
    let listed = |fragment: &str| listing.iter().any(|line| line.contains(fragment));
    assert!(
        listed(&worktree.display().to_string()),
        "the listing names the worktree: {text}"
    );
    assert!(
        listed("1 uncommitted change"),
        "the listing counts the worktree's uncommitted changes: {text}"
    );
    let state_file = devflow_core::workflow::state_path(&root, phase);
    assert!(
        listed(&state_file.display().to_string()),
        "the listing names the run state: {text}"
    );
    assert!(
        listed("branch feature/phase-31 (merged into `develop`)"),
        "the listing names the branch and that it is merged: {text}"
    );
    let lines: Vec<&str> = stdout.lines().collect();
    let last_listed = lines.iter().rposition(|line| line.starts_with("  "));
    let first_removed = lines.iter().position(|line| line.starts_with("removed "));
    assert!(
        last_listed.is_some() && last_listed < first_removed,
        "every listing line comes before the first `removed` line: {text}"
    );

    assert_eq!(
        phase_items(&root, phase),
        PhaseItems::NONE,
        "the worktree, the state and the branch are gone: {text}"
    );
    bystanders.assert_untouched(&root, &text);
}

/// A hermetic home and an empty temporary directory, canonical because the CLI prints
/// the canonical project root, for one discard test.
struct DiscardCase {
    home: common::HermeticHome,
    _repo: tempfile::TempDir,
    root: PathBuf,
}

fn discard_case() -> DiscardCase {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    DiscardCase {
        home: common::HermeticHome::new(),
        _repo: repo,
        root,
    }
}

/// [`cleanup_fixture`] with an uncommitted file in the worktree: a phase with all three
/// of the items discard removes. Returns the worktree's path.
fn full_phase(
    root: &Path,
    phase: PhaseId,
    merged: bool,
    shape: impl FnOnce(&mut devflow_core::state::State),
) -> PathBuf {
    let worktree = cleanup_fixture(root, phase, merged, shape);
    fs::write(worktree.join("scratch.txt"), "uncommitted\n").unwrap();
    let items = phase_items(root, phase);
    assert!(
        items.worktree.is_some() && items.state.is_some() && items.branch_tip.is_some(),
        "premise: the phase has a worktree, a state and a branch"
    );
    worktree
}

/// Assert that `out` refused and removed nothing: `phase`'s worktree files, state bytes
/// and branch tip are exactly `before`, the exit is non-zero, stderr holds every
/// `fragment`, and stdout reports no removal. The effects are asserted first.
fn assert_discard_refused(
    out: &Output,
    root: &Path,
    phase: PhaseId,
    before: &PhaseItems,
    fragments: &[&str],
) {
    let (stdout, text) = discard_text(out);
    assert_eq!(
        &phase_items(root, phase),
        before,
        "a refused discard removes nothing: {text}"
    );
    assert!(!out.status.success(), "the discard is refused: {text}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    for fragment in fragments {
        assert!(
            stderr.contains(fragment),
            "the refusal names {fragment:?}: {text}"
        );
    }
    assert!(
        !stdout.lines().any(|line| line.starts_with("removed ")),
        "stdout reports no removal: {text}"
    );
}

/// D-10: an unmerged branch is not deleted unless the operator names it. Without the
/// name nothing is removed, and the refusal gives the command that names it.
#[test]
fn discard_refuses_an_unmerged_branch_without_its_name() {
    let case = discard_case();
    let phase = PhaseId::new(32);
    full_phase(&case.root, phase, false, finished);
    let before = phase_items(&case.root, phase);

    let out = run_discard(&case.home, &case.root, phase, &[]);
    assert_discard_refused(
        &out,
        &case.root,
        phase,
        &before,
        &[
            "1 commit not merged into `develop`",
            "nothing was removed",
            "devflow discard --phase 32 --delete-unmerged-branch feature/phase-32",
        ],
    );
}

/// D-10 / D-23: the name must be the phase's own branch. Another name is refused with
/// both names, and nothing is removed.
#[test]
fn discard_refuses_a_wrong_branch_name() {
    let case = discard_case();
    let phase = PhaseId::new(33);
    full_phase(&case.root, phase, false, finished);
    let before = phase_items(&case.root, phase);

    let out = run_discard(
        &case.home,
        &case.root,
        phase,
        &["--delete-unmerged-branch", "feature/phase-99"],
    );
    assert_discard_refused(
        &out,
        &case.root,
        phase,
        &before,
        &[
            "feature/phase-99",
            "feature/phase-33",
            "nothing was removed",
        ],
    );
}

/// D-23: the value must equal the phase's branch whatever the branch's merge state, so
/// a wrong name is refused over a merged branch too.
#[test]
fn discard_refuses_a_wrong_branch_name_over_a_merged_branch() {
    let case = discard_case();
    let phase = PhaseId::new(43);
    full_phase(&case.root, phase, true, finished);
    let before = phase_items(&case.root, phase);

    let out = run_discard(
        &case.home,
        &case.root,
        phase,
        &["--delete-unmerged-branch", "feature/phase-99"],
    );
    assert_discard_refused(
        &out,
        &case.root,
        phase,
        &before,
        &[
            "feature/phase-99",
            "feature/phase-43",
            "nothing was removed",
        ],
    );
}

/// D-10's one exception: with the phase's own branch named, an unmerged branch is
/// deleted with the worktree and the state. Another phase's unmerged branch and an
/// unrelated unmerged branch are left alone.
#[test]
fn discard_with_the_branch_name_deletes_an_unmerged_branch() {
    let case = discard_case();
    let phase = PhaseId::new(34);
    full_phase(&case.root, phase, false, finished);
    let bystanders = add_bystanders(&case.root);

    let out = run_discard(
        &case.home,
        &case.root,
        phase,
        &["--delete-unmerged-branch", "feature/phase-34"],
    );
    let (stdout, text) = discard_text(&out);
    assert!(out.status.success(), "{text}");
    assert!(
        discard_listing(&stdout)
            .iter()
            .any(|line| line
                .contains("branch feature/phase-34 (1 commit not merged into `develop`)")),
        "the listing says the branch is not merged: {text}"
    );
    assert_eq!(
        phase_items(&case.root, phase),
        PhaseItems::NONE,
        "the worktree, the state and the named branch are gone: {text}"
    );
    bystanders.assert_untouched(&case.root, &text);
}

/// The base a run may record instead of `develop`.
const OTHER_BASE: &str = "workspace/example";

/// A finished phase whose branch is forked from [`OTHER_BASE`], a branch holding one
/// commit `develop` lacks: the phase branch is merged into that base and has one
/// commit not merged into `develop`. The saved state records `recorded_base`.
fn phase_on_another_base(root: &Path, phase: PhaseId, recorded_base: Option<&str>) {
    init_repo(root);
    git(root, &["checkout", "-q", "-b", OTHER_BASE]);
    fs::write(root.join("planning.txt"), "only on the other base\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "planning"]);
    git(root, &["checkout", "-q", "develop"]);
    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    git(root, &["branch", &branch, OTHER_BASE]);
    let worktree = devflow_core::worktree::phase_path(root, phase);
    devflow_core::worktree::add(root, &worktree, &branch, &branch, false).unwrap();
    let mut state = devflow_core::state::State::new(
        phase,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Auto,
        root.to_path_buf(),
    );
    finished(&mut state);
    state.base_branch = recorded_base.map(str::to_string);
    devflow_core::workflow::save_state(&state).unwrap();
}

/// D-10: merged means merged into the base the run recorded. A branch merged into a
/// recorded non-default base is discarded without its name, though it has a commit
/// `develop` lacks.
#[test]
fn discard_counts_merged_against_the_recorded_base() {
    let case = discard_case();
    let phase = PhaseId::new(35);
    phase_on_another_base(&case.root, phase, Some(OTHER_BASE));
    let base_tip = branch_tip(&case.root, OTHER_BASE).expect("premise: the base exists");

    let out = run_discard(&case.home, &case.root, phase, &[]);
    let (stdout, text) = discard_text(&out);
    assert!(out.status.success(), "{text}");
    assert!(
        discard_listing(&stdout)
            .iter()
            .any(|line| line.contains("branch feature/phase-35 (merged into `workspace/example`)")),
        "the listing counts against the recorded base: {text}"
    );
    assert_eq!(
        phase_items(&case.root, phase),
        PhaseItems::NONE,
        "the worktree, the state and the branch are gone: {text}"
    );
    assert_eq!(
        branch_tip(&case.root, OTHER_BASE).as_deref(),
        Some(base_tip.as_str()),
        "the base branch is untouched: {text}"
    );
}

/// Control for the test above: the same branch with no recorded base is counted
/// against `develop`, where it is unmerged, so it is refused.
#[test]
fn discard_without_a_recorded_base_counts_merged_against_develop() {
    let case = discard_case();
    let phase = PhaseId::new(36);
    phase_on_another_base(&case.root, phase, None);
    let before = phase_items(&case.root, phase);
    assert!(
        before.worktree.is_some() && before.state.is_some() && before.branch_tip.is_some(),
        "premise: the phase has a worktree, a state and a branch"
    );

    let out = run_discard(&case.home, &case.root, phase, &[]);
    assert_discard_refused(
        &out,
        &case.root,
        phase,
        &before,
        &[
            "1 commit not merged into `develop`",
            "--delete-unmerged-branch feature/phase-36",
        ],
    );
}

/// The merge state is never assumed: when it cannot be read (here the recorded base
/// does not exist), discard refuses and removes nothing.
#[test]
fn discard_refuses_when_the_merge_state_cannot_be_read() {
    let case = discard_case();
    let phase = PhaseId::new(44);
    full_phase(&case.root, phase, true, |state| {
        finished(state);
        state.base_branch = Some("no/such-base".to_string());
    });
    let before = phase_items(&case.root, phase);

    let out = run_discard(&case.home, &case.root, phase, &[]);
    assert_discard_refused(
        &out,
        &case.root,
        phase,
        &before,
        &[
            "could not be checked for commits not merged into `no/such-base`",
            "nothing was removed",
        ],
    );
}

/// Whether a discard's listing has a line for `item` (`worktree`, `run state`, `branch`).
fn listing_has(stdout: &str, item: &str) -> bool {
    discard_listing(stdout)
        .iter()
        .any(|line| line.starts_with(&format!("  {item} ")))
}

/// D-09.1: a leftover branch with no worktree and no state is discarded on its own.
/// The items that do not exist are neither listed nor a failure.
#[test]
fn discard_removes_a_branch_with_no_worktree_or_state() {
    let case = discard_case();
    let phase = PhaseId::new(37);
    init_repo(&case.root);
    git(&case.root, &["branch", "feature/phase-37", "develop"]);
    let bystanders = add_bystanders(&case.root);
    let before = phase_items(&case.root, phase);
    assert!(
        before.worktree.is_none() && before.state.is_none() && before.branch_tip.is_some(),
        "premise: the phase has only a branch"
    );

    let out = run_discard(&case.home, &case.root, phase, &[]);
    let (stdout, text) = discard_text(&out);
    assert!(out.status.success(), "{text}");
    assert!(
        listing_has(&stdout, "branch"),
        "the listing names the branch: {text}"
    );
    assert!(
        !listing_has(&stdout, "worktree") && !listing_has(&stdout, "run state"),
        "the listing has no worktree line and no state line: {text}"
    );
    assert!(!text.contains("warning:"), "nothing failed: {text}");
    assert_eq!(
        phase_items(&case.root, phase),
        PhaseItems::NONE,
        "the branch is gone: {text}"
    );
    bystanders.assert_untouched(&case.root, &text);
}

/// D-09.1: a leftover worktree and branch with no state are discarded, and the
/// missing state is neither listed nor a failure.
#[test]
fn discard_removes_a_worktree_and_branch_with_no_state() {
    let case = discard_case();
    let phase = PhaseId::new(38);
    full_phase(&case.root, phase, true, finished);
    fs::remove_file(devflow_core::workflow::state_path(&case.root, phase)).unwrap();
    let bystanders = add_bystanders(&case.root);

    let out = run_discard(&case.home, &case.root, phase, &[]);
    let (stdout, text) = discard_text(&out);
    assert!(out.status.success(), "{text}");
    assert!(
        listing_has(&stdout, "worktree") && listing_has(&stdout, "branch"),
        "the listing names the worktree and the branch: {text}"
    );
    assert!(
        !listing_has(&stdout, "run state"),
        "the listing has no state line: {text}"
    );
    assert!(!text.contains("warning:"), "nothing failed: {text}");
    assert_eq!(
        phase_items(&case.root, phase),
        PhaseItems::NONE,
        "the worktree and the branch are gone: {text}"
    );
    bystanders.assert_untouched(&case.root, &text);
}

/// A directory at the phase path that git does not list as a worktree is not
/// discard's to remove: it refuses before anything is removed, the state and the
/// branch included.
#[test]
fn discard_refuses_a_directory_git_does_not_list_as_a_worktree() {
    let case = discard_case();
    let phase = PhaseId::new(39);
    init_repo(&case.root);
    let directory = devflow_core::worktree::phase_path(&case.root, phase);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("notes.txt"), "not a worktree\n").unwrap();
    git(&case.root, &["branch", "feature/phase-39", "develop"]);
    let mut state = devflow_core::state::State::new(
        phase,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Auto,
        case.root.clone(),
    );
    finished(&mut state);
    devflow_core::workflow::save_state(&state).unwrap();
    let before = phase_items(&case.root, phase);
    assert!(
        before
            .worktree
            .as_ref()
            .is_some_and(|files| files.len() == 1)
            && before.state.is_some()
            && before.branch_tip.is_some(),
        "premise: a directory with one file, a state and a branch"
    );

    let out = run_discard(&case.home, &case.root, phase, &[]);
    assert_discard_refused(
        &out,
        &case.root,
        phase,
        &before,
        &[
            &directory.display().to_string(),
            "git does not list it as a worktree",
            "nothing was removed",
        ],
    );
}

/// A phase worktree that is not on the phase branch may hold commits no branch
/// keeps, which the listing would not show: discard refuses it and removes nothing.
#[test]
fn discard_refuses_a_phase_worktree_that_is_not_on_the_phase_branch() {
    let case = discard_case();
    let phase = PhaseId::new(45);
    let worktree = full_phase(&case.root, phase, true, finished);
    git(&worktree, &["checkout", "-q", "--detach"]);
    fs::write(worktree.join("detached.txt"), "kept by no branch\n").unwrap();
    git(&worktree, &["add", "detached.txt"]);
    git(
        &worktree,
        &["commit", "-q", "-m", "a commit on a detached HEAD"],
    );
    let before = phase_items(&case.root, phase);

    let out = run_discard(&case.home, &case.root, phase, &[]);
    assert_discard_refused(
        &out,
        &case.root,
        phase,
        &before,
        &[
            &worktree.display().to_string(),
            "a detached HEAD",
            "feature/phase-45",
            "nothing was removed",
        ],
    );
}

/// A worktree git still lists whose directory is gone cannot be read, so discard
/// refuses, keeps the state and the branch, and names `git worktree prune`.
#[test]
fn discard_refuses_a_listed_worktree_whose_directory_is_gone() {
    let case = discard_case();
    let phase = PhaseId::new(46);
    let worktree = full_phase(&case.root, phase, true, finished);
    fs::remove_dir_all(&worktree).unwrap();
    let before = phase_items(&case.root, phase);
    assert!(
        before.worktree.is_none() && before.state.is_some() && before.branch_tip.is_some(),
        "premise: the directory is gone, the state and the branch are not"
    );
    let listed = git(&case.root, &["worktree", "list", "--porcelain"]);
    assert!(
        String::from_utf8_lossy(&listed.stdout).contains(&worktree.display().to_string()),
        "premise: git still lists the worktree"
    );

    let out = run_discard(&case.home, &case.root, phase, &[]);
    assert_discard_refused(
        &out,
        &case.root,
        phase,
        &before,
        &[
            &worktree.display().to_string(),
            "git worktree prune",
            "nothing was removed",
        ],
    );
}

/// A live run is never discarded (D-07, D-12a): with the phase's runner lock held by
/// another process, discard refuses naming the runner and `devflow stop`, and removes
/// nothing.
#[test]
fn discard_refuses_a_live_run() {
    let case = discard_case();
    let phase = PhaseId::new(47);
    full_phase(&case.root, phase, true, |_| {});
    let holder = RunnerLockHolder::hold(&case.root, phase);
    let before = phase_items(&case.root, phase);

    let out = run_discard(&case.home, &case.root, phase, &[]);
    assert_discard_refused(
        &out,
        &case.root,
        phase,
        &before,
        &[
            "its runner holds the runner lock",
            &format!("(pid {})", holder.pid()),
            "devflow stop --phase 47",
            "nothing was removed",
        ],
    );
    drop(holder);
}

/// The variables [`runner_lock_holder_entry`] reads: the project root and phase whose
/// runner lock it holds, and the directory it signals through.
const HOLDER_ROOT_ENV: &str = "RUNNER_LOCK_HOLDER_ROOT";
const HOLDER_PHASE_ENV: &str = "RUNNER_LOCK_HOLDER_PHASE";
const HOLDER_SIGNAL_ENV: &str = "RUNNER_LOCK_HOLDER_SIGNAL";

/// The bound on each wait between a [`RunnerLockHolder`] and its child.
const HOLDER_WAIT: Duration = Duration::from_secs(30);

/// Whether `path` exists within [`HOLDER_WAIT`].
fn appears_within(path: &Path) -> bool {
    let deadline = std::time::Instant::now() + HOLDER_WAIT;
    while std::time::Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    path.exists()
}

/// Not a test of its own: the child process [`RunnerLockHolder`] runs. Holds the
/// phase's runner lock through `claim_runner_lock`, as a runner does, writes `held`,
/// and lets go once `release` appears in its signal directory. Without the variables
/// it does nothing.
#[test]
#[ignore = "the runner-lock holder process RunnerLockHolder spawns, not a test"]
fn runner_lock_holder_entry() {
    let (Some(root), Some(phase), Some(signal)) = (
        std::env::var_os(HOLDER_ROOT_ENV),
        std::env::var_os(HOLDER_PHASE_ENV),
        std::env::var_os(HOLDER_SIGNAL_ENV),
    ) else {
        return;
    };
    let phase = PhaseId::new(phase.to_str().unwrap().parse().unwrap());
    let signal = PathBuf::from(signal);
    let _hold = devflow_core::liveness::claim_runner_lock(Path::new(&root), phase)
        .expect("hold the runner lock");
    fs::write(signal.join("held"), "").unwrap();
    assert!(
        appears_within(&signal.join("release")),
        "the holder was never released"
    );
}

/// A child process holding a phase's runner lock, as a live runner does. Dropping it
/// releases the lock and reaps the child, on a failed assertion too.
struct RunnerLockHolder {
    child: std::process::Child,
    signal: tempfile::TempDir,
}

impl RunnerLockHolder {
    /// Spawn this test binary's [`runner_lock_holder_entry`] for `(root, phase)` and
    /// wait until it holds the lock.
    fn hold(root: &Path, phase: PhaseId) -> Self {
        let signal = tempfile::tempdir().unwrap();
        let child = Command::new(std::env::current_exe().expect("this test binary"))
            .args([
                "--exact",
                "runner_lock_holder_entry",
                "--ignored",
                "--quiet",
            ])
            .env(HOLDER_ROOT_ENV, root)
            .env(HOLDER_PHASE_ENV, phase.to_string())
            .env(HOLDER_SIGNAL_ENV, signal.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn the runner-lock holder");
        let holder = RunnerLockHolder { child, signal };
        assert!(
            appears_within(&holder.signal.path().join("held")),
            "the holder never took the runner lock"
        );
        assert_eq!(
            devflow_core::liveness::probe(&devflow_core::liveness::runner_lock_path(root, phase))
                .unwrap(),
            devflow_core::liveness::Held::Live,
            "premise: the runner lock reads held"
        );
        holder
    }

    /// The holder's pid, which the lock's record names.
    fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for RunnerLockHolder {
    fn drop(&mut self) {
        let _ = fs::write(self.signal.path().join("release"), "");
        let deadline = std::time::Instant::now() + HOLDER_WAIT;
        while std::time::Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A STALE supervisor socket named for `root` (canonical) and `phase`, with no
/// sidecar beside it, in the hermetic cache: what the stateless sweep finds for a
/// supervisor no state names, and counts as possibly live. Returns the socket's path.
fn plant_swept_socket(home: &common::HermeticHome, root: &Path, phase: PhaseId) -> PathBuf {
    let cache = home.cache_dir();
    fs::create_dir_all(devflow_core::registry::socket_dir_in(&cache)).unwrap();
    let socket = devflow_core::supervisor::SocketPath::for_launch(
        &cache,
        root,
        phase,
        &devflow_core::supervisor::new_launch_id(),
    )
    .expect("a socket path that fits");
    drop(std::os::unix::net::UnixListener::bind(socket.as_path()).expect("bind the socket"));
    socket.as_path().to_path_buf()
}

/// A launch liveness lock in `root`'s `.devflow/`, taken by this test process: what a
/// supervisor holds from before it publishes its socket. It reads live for as long as
/// the returned hold is kept, and the launch has no socket and no sidecar. Returns the
/// launch id with the hold.
fn hold_launch_lock(root: &Path) -> (String, devflow_core::liveness::Hold) {
    let launch_id = devflow_core::supervisor::new_launch_id();
    let path = devflow_core::liveness::launch_liveness_path(root, &launch_id)
        .expect("a launch id of the launch-id format");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let hold = devflow_core::liveness::Hold::take(&path).expect("take the liveness lock");
    assert_eq!(
        devflow_core::liveness::probe(&path).unwrap(),
        devflow_core::liveness::Held::Live,
        "premise: the launch's liveness lock reads live"
    );
    (launch_id, hold)
}

/// The lock decides, not the file: a launch liveness file that nothing holds is an
/// abandoned reservation, so with no state discard still removes the worktree and the
/// branch, and leaves that file alone.
#[test]
fn discard_ignores_a_launch_lock_nothing_holds() {
    let case = discard_case();
    let phase = PhaseId::new(60);
    full_phase(&case.root, phase, true, finished);
    fs::remove_file(devflow_core::workflow::state_path(&case.root, phase)).unwrap();
    let (_, hold) = hold_launch_lock(&case.root);
    let lock = hold.path().to_path_buf();
    drop(hold);
    assert_eq!(
        devflow_core::liveness::probe(&lock).unwrap(),
        devflow_core::liveness::Held::Abandoned,
        "premise: the liveness file exists and nothing holds it"
    );

    let out = run_discard(&case.home, &case.root, phase, &[]);
    let (_, text) = discard_text(&out);
    assert!(out.status.success(), "{text}");
    assert_eq!(
        phase_items(&case.root, phase),
        PhaseItems::NONE,
        "the worktree and the branch are gone: {text}"
    );
    assert!(lock.exists(), "the unheld liveness file is left in place");
}

/// A held lock that another phase's loadable state records is that phase's launch, not
/// this one's: with no state of its own, the phase is still discarded, and the other
/// phase is left as it was.
#[test]
fn discard_ignores_a_launch_lock_another_phases_state_records() {
    let case = discard_case();
    let phase = PhaseId::new(61);
    full_phase(&case.root, phase, true, finished);
    fs::remove_file(devflow_core::workflow::state_path(&case.root, phase)).unwrap();
    let (launch_id, hold) = hold_launch_lock(&case.root);
    let socket = devflow_core::supervisor::SocketPath::for_launch(
        &case.home.cache_dir(),
        &case.root,
        BYSTANDER_PHASE,
        &launch_id,
    )
    .expect("a socket path that fits");
    let pending = devflow_core::state::PendingSupervisor::new(
        launch_id,
        socket.as_path().to_path_buf(),
        hold.path().to_path_buf(),
    );
    add_phase(&case.root, BYSTANDER_PHASE, false, |state| {
        state.pending_supervisor = Some(pending);
    });
    let other = phase_items(&case.root, BYSTANDER_PHASE);
    assert!(
        other.worktree.is_some() && other.state.is_some() && other.branch_tip.is_some(),
        "premise: the other phase has a worktree, a state and a branch"
    );

    let out = run_discard(&case.home, &case.root, phase, &[]);
    let (_, text) = discard_text(&out);
    assert!(out.status.success(), "{text}");
    assert_eq!(
        phase_items(&case.root, phase),
        PhaseItems::NONE,
        "the worktree and the branch are gone: {text}"
    );
    assert_eq!(
        phase_items(&case.root, BYSTANDER_PHASE),
        other,
        "the phase that records the launch is untouched: {text}"
    );
    assert!(hold.path().exists(), "the liveness lock is left in place");
}

/// D-20: discard is the way out of a state that does not load. It removes that file
/// with the worktree and the branch, and keeps no copy of it.
#[test]
fn discard_removes_a_state_that_does_not_load() {
    let case = discard_case();
    let phase = PhaseId::new(52);
    full_phase(&case.root, phase, true, finished);
    let state_file = devflow_core::workflow::state_path(&case.root, phase);
    fs::write(&state_file, "not json").unwrap();
    let bystanders = add_bystanders(&case.root);
    // Another phase's socket is not this phase's: the sweep must not refuse on it.
    let other_socket = plant_swept_socket(&case.home, &case.root, BYSTANDER_PHASE);

    let out = run_discard(&case.home, &case.root, phase, &[]);
    let (stdout, text) = discard_text(&out);
    assert!(out.status.success(), "{text}");
    assert!(
        other_socket.exists(),
        "another phase's socket is left in place: {text}"
    );
    assert!(
        listing_has(&stdout, "run state"),
        "the listing names the state file: {text}"
    );
    assert_eq!(
        phase_items(&case.root, phase),
        PhaseItems::NONE,
        "the state that does not load, the worktree and the branch are gone: {text}"
    );
    let copies: Vec<PathBuf> = common::entries_under(&case.root.join(".devflow"))
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("unloadable-state-"))
        })
        .collect();
    assert!(copies.is_empty(), "no copy is kept: {copies:?}\n{text}");
    bystanders.assert_untouched(&case.root, &text);
}

/// The branch the primary checkout of `root` has checked out.
fn primary_branch(root: &Path) -> String {
    let out = git(root, &["symbolic-ref", "--short", "HEAD"]);
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A finished state for `phase` that records no worktree, saved in `root`.
fn save_finished_state(root: &Path, phase: PhaseId) {
    let mut state = devflow_core::state::State::new(
        phase,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Auto,
        root.to_path_buf(),
    );
    finished(&mut state);
    devflow_core::workflow::save_state(&state).unwrap();
}

/// D-09.2, the `start --no-worktree` shape: with the phase branch checked out in the
/// primary checkout, discard refuses, names the branch to switch to and never
/// switches itself. Nothing is removed.
#[test]
fn discard_refuses_a_branch_checked_out_in_the_primary_checkout() {
    let case = discard_case();
    let phase = PhaseId::new(53);
    init_repo(&case.root);
    git(&case.root, &["checkout", "-q", "-b", "feature/phase-53"]);
    save_finished_state(&case.root, phase);
    let before = phase_items(&case.root, phase);
    assert!(
        before.worktree.is_none() && before.state.is_some() && before.branch_tip.is_some(),
        "premise: a state and a branch, and no worktree"
    );

    let out = run_discard(&case.home, &case.root, phase, &[]);
    assert_discard_refused(
        &out,
        &case.root,
        phase,
        &before,
        &[
            "feature/phase-53 is checked out in the primary checkout",
            "git switch develop",
            "nothing was removed",
        ],
    );
    assert_eq!(
        primary_branch(&case.root),
        "feature/phase-53",
        "discard does not switch the primary checkout's branch"
    );
}

/// git refuses to delete a branch any worktree has checked out, so with the phase
/// branch checked out in a worktree other than the phase's own, discard refuses
/// before it removes the state.
#[test]
fn discard_refuses_a_branch_checked_out_in_another_worktree() {
    let case = discard_case();
    let phase = PhaseId::new(56);
    init_repo(&case.root);
    git(&case.root, &["branch", "feature/phase-56", "develop"]);
    let elsewhere = case.root.join(".worktrees").join("elsewhere");
    devflow_core::worktree::add(
        &case.root,
        &elsewhere,
        "feature/phase-56",
        "feature/phase-56",
        false,
    )
    .unwrap();
    save_finished_state(&case.root, phase);
    let before = phase_items(&case.root, phase);
    assert!(
        before.worktree.is_none() && before.state.is_some() && before.branch_tip.is_some(),
        "premise: a state and a branch, and no worktree at the phase path"
    );

    let out = run_discard(&case.home, &case.root, phase, &[]);
    assert_discard_refused(
        &out,
        &case.root,
        phase,
        &before,
        &[
            "feature/phase-56 is checked out in the worktree",
            &elsewhere.display().to_string(),
            "nothing was removed",
        ],
    );
    assert!(
        elsewhere.join("README.md").is_file(),
        "the other worktree is left in place"
    );
}

/// D-09.2: discard holds the phase's foreground holder, so while another foreground
/// process holds the phase it refuses, naming the holder, and removes nothing.
#[test]
fn discard_refuses_while_a_foreground_holder_holds_the_phase() {
    let case = discard_case();
    let phase = PhaseId::new(57);
    full_phase(&case.root, phase, true, finished);
    let hold = devflow_core::liveness::claim_phase_holder(&case.root, phase)
        .unwrap_or_else(|_| panic!("premise: the test claims the phase's holder"));
    let before = phase_items(&case.root, phase);

    let out = run_discard(&case.home, &case.root, phase, &[]);
    assert_discard_refused(
        &out,
        &case.root,
        phase,
        &before,
        &[
            "another DevFlow verb holds phase 57",
            &format!("(pid {})", std::process::id()),
        ],
    );
    drop(hold);
}

/// A verb refuses every state it does not act on: with no state, worktree or branch
/// for the phase, discard says there is nothing to discard and exits non-zero.
#[test]
fn discard_refuses_when_nothing_exists_for_the_phase() {
    let case = discard_case();
    let phase = PhaseId::new(54);
    init_repo(&case.root);
    let bystanders = add_bystanders(&case.root);
    assert_eq!(
        phase_items(&case.root, phase),
        PhaseItems::NONE,
        "premise: the phase has nothing"
    );

    let out = run_discard(&case.home, &case.root, phase, &[]);
    assert_discard_refused(
        &out,
        &case.root,
        phase,
        &PhaseItems::NONE,
        &[
            "phase 54 has no run state, worktree or branch",
            "nothing to discard",
        ],
    );
    let (_, text) = discard_text(&out);
    bystanders.assert_untouched(&case.root, &text);
}
