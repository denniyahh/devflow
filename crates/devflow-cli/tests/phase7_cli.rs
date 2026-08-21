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

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
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
    // both agents.
    for phase in ["07", "08", "09"] {
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

fn run_devflow(root: &Path, fake_bin: &Path, args: &[&str]) -> Output {
    run_devflow_inner(root, fake_bin, args, false)
}

/// [`run_devflow`] with D-11's legacy-launch opt-out forced on for the spawned
/// process.
///
/// **This is the integration-suite counterpart of 34-06's in-process
/// `state.legacy_claude_launch = true`, and it exists for the same reason: to
/// state a test's launch-path premise explicitly instead of inheriting it from
/// `STREAM_JSON_STAGES` membership.** A test whose subject is `start`'s own
/// behaviour — which branch it forks from, whether it creates a worktree, where
/// a `--until` cap halts it — is orthogonal to the transport the stage's agent
/// is launched over. Left implicit, that premise silently depends on the
/// stage's ABSENCE from `STREAM_JSON_STAGES`, and 34-05's widening destroys it:
/// `canary_gate` then invokes the real `ClaudeCanaryLauncher` at `Stage::Define`
/// and the launch dies on a delivery refusal that has nothing to do with the
/// subject under test.
///
/// The variable is read by `devflow_core::config::claude_legacy_launch()` inside
/// the child, folded into the persisted `state.legacy_claude_launch` by
/// `commands::start`'s `apply_legacy_launch_opt_out`, and therefore survives to
/// every later stage the DETACHED monitor launches — which is what these tests
/// need, since the monitor chain, not this process, runs Plan and beyond.
///
/// **Do NOT reach for this to silence a canary refusal in a test whose subject
/// IS the stream path.** Pinning such a test to the legacy transport deletes the
/// coverage rather than stabilising it; see
/// `parallel_creates_two_worktrees_and_spawns_two_monitors`, which deliberately
/// does not use this helper.
fn run_devflow_legacy_launch(root: &Path, fake_bin: &Path, args: &[&str]) -> Output {
    run_devflow_inner(root, fake_bin, args, true)
}

fn run_devflow_inner(root: &Path, fake_bin: &Path, args: &[&str], legacy_launch: bool) -> Output {
    let mut command = Command::new(devflow_bin());
    command
        .args(args)
        .arg(root)
        .env("PATH", path_with_fake_bin(fake_bin))
        .env("DEVFLOW_TEST_ROOT", root)
        .current_dir(root);
    if legacy_launch {
        command.env("DEVFLOW_CLAUDE_LEGACY_LAUNCH", "true");
    }
    // HYG-01: bound the gate wait on the CHILD's env. Supervise-mode stages
    // that reach a gate (never-silent Define/Plan failures, Validate in
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

/// Wait until a phase's persisted state is cleared (the pipeline reached
/// Ship and `finish_workflow` called `clear_state`). 20b's new liveness
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
fn parallel_creates_two_worktrees_and_spawns_two_monitors() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    // 34-06b: this test does NOT take D-11's legacy opt-out, and that is the
    // whole point of it. It is the multi-plan wave test, and the D-15 delivery
    // canary exists precisely to guarantee that a concurrent wave does not
    // silently orphan its dispatched work (999.64). Pinning it to the legacy
    // single-document transport would make the wave it covers the one shape the
    // canary is not protecting — deleting the coverage instead of stabilising
    // it, which is the "repaired by the wrong mechanism" failure 34-06 declined
    // to commit on this test's behalf.
    //
    // So the canary outcome is STUBBED rather than bypassed, through the seam
    // this suite already has across the process boundary: the fake `claude` on
    // PATH. The script below is the one
    // `reference_and_cleanup_worktree_cli_flow` already uses — it models a CLI
    // that DOES deliver the planted token back inside a top-level `result`
    // event, so `run_delivery_canary` reaches a deterministic `Confirmed`
    // without any real agent invocation, and every step of the guard it is
    // meant to exercise (declare, plant, capture, trust-decide) actually runs.
    // No production code has a test-only escape hatch added for this.
    //
    // `read -r turn` takes exactly one line and returns: the monitor writes the
    // user turn followed by a newline, and blocking on full EOF would hang
    // against a pipe deliberately held open past the first turn. On the legacy
    // stages stdin is `/dev/null`, so the read yields nothing and the ordinary
    // marker branch runs, exactly as before this change — which is why phase 7
    // behaves identically under the committed `&[Stage::Code]` constant and
    // under 34-05's widening.
    let fake_bin = fake_bin_dir(&[
        (
            "claude",
            r#"#!/bin/sh
read -r turn
case "$turn" in
  *DEVFLOW_DELIVERY_CANARY_*)
    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
    printf '{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"%s"}\n' "$token"
    ;;
  *)
    printf 'fake claude\nDEVFLOW_RESULT: {"status":"success"}\n'
    ;;
esac
"#,
        ),
        (
            "codex",
            "#!/bin/sh\nprintf 'fake codex\\nDEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
        ),
    ]);

    // `--mode supervise` (35.1-03): phase 8 runs on `codex`, and
    // `preflight_unattended_launch_check`'s C2 refuses a `Mode::Auto` launch on
    // any agent that cannot host the chain-flag guard — the guard binds inside
    // the pipe-owning monitor, which is the Claude stream path only. This
    // test's subject is two worktrees and two monitors, which is
    // mode-independent; the agent diversity is the point and is preserved.
    let output = run_devflow(
        root,
        &fake_bin.path,
        &[
            "parallel",
            "--phases",
            "7,8",
            "--agents",
            "claude,codex",
            "--mode",
            "supervise",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("phase 7"));
    assert!(stdout.contains("phase 8"));

    assert!(root.join(".worktrees/phase-07").is_dir());
    assert!(root.join(".worktrees/phase-08").is_dir());

    let phase7_pid = root.join(".devflow/phase-07-agent-pid");
    let phase8_pid = root.join(".devflow/phase-08-agent-pid");
    assert!(wait_for_pid(&phase7_pid) > 0);
    assert!(wait_for_pid(&phase8_pid) > 0);

    let phase7_stdout = root.join(".devflow/phase-07-stdout");
    let phase8_stdout = root.join(".devflow/phase-08-stdout");
    // WR-03: assert each capture immediately after the `wait_for` call that
    // established it, not after both waits complete. Same mechanism as
    // `wait_for_pid` above: each stage transition's `archive_phase_files`
    // briefly deletes the capture before the next monitor recreates it. A
    // combined assertion placed after both `wait_for` calls is still racy —
    // the second `wait_for`'s own polling loop gives a fast monitor enough
    // time to archive the first capture in the interim (observed directly:
    // this exact ordering flaked at run 15/25 during 18-02 verification).
    // Asserting inside each capture's own wait window closes that gap.
    wait_for(&phase7_stdout);
    assert!(phase7_stdout.exists());
    wait_for(&phase8_stdout);
    assert!(phase8_stdout.exists());

    // 13-DEFERRED-CR-03: each parallel phase persists its own state file —
    // the second start no longer clobbers the first phase's state.
    let state7 = devflow_core::workflow::load_state(root, PhaseId::new(7)).expect("phase 7 state");
    let state8 = devflow_core::workflow::load_state(root, PhaseId::new(8)).expect("phase 8 state");
    assert_eq!(state7.phase, PhaseId::new(7));
    assert_eq!(state8.phase, PhaseId::new(8));
    assert!(
        !root.join(".devflow/state.json").exists(),
        "legacy single-slot state.json must not be written anymore"
    );

    // HYG-01: both parallel monitors are this test's to reap.
    let _reap7 = MonitorReapGuard::after_launch(&state7);
    let _reap8 = MonitorReapGuard::after_launch(&state8);
}

#[test]
fn start_defaults_to_worktree() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[(
        "claude",
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
    )]);

    // No worktree flag at all — worktree-by-default (13d) means this must
    // create the isolated worktree without an explicit opt-in.
    //
    // Legacy launch pinned deliberately (34-06b): the subject is where `start`
    // puts the phase, not which transport its agent is launched over. See
    // `run_devflow_legacy_launch`.
    run_devflow_legacy_launch(
        root,
        &fake_bin.path,
        &[
            // `supervise` (35.1-03): the legacy opt-out pinned above is
            // exactly what C2 refuses in `Mode::Auto`, and where `start` puts
            // the phase is mode-independent.
            "start",
            "--phase",
            "11",
            "--agent",
            "claude",
            "--mode",
            "supervise",
        ],
    );

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
        "claude",
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
    )]);

    // Worktree mode is the default — no --no-worktree flag. This must
    // succeed (the helper asserts a zero exit status) despite the main
    // checkout being 51 commits behind develop.
    //
    // Legacy launch pinned deliberately (34-06b): the subject is the
    // divergence check's scope, not the launch transport. See
    // `run_devflow_legacy_launch`.
    run_devflow_legacy_launch(
        root,
        &fake_bin.path,
        &[
            // `supervise` (35.1-03): same reason as phase 11 above — the
            // legacy opt-out is refused in `Mode::Auto`, and this test's
            // subject (main-checkout divergence) is mode-independent.
            "start",
            "--phase",
            "13",
            "--agent",
            "claude",
            "--mode",
            "supervise",
        ],
    );

    wait_for(&root.join(".worktrees/phase-13"));
    assert!(root.join(".worktrees/phase-13").is_dir());

    // Bind the guard to the SETTLED state (the chain blocks at the Validate
    // gate in supervise mode — that last monitor is this test's to reap).
    let settled = wait_for_settled(root, PhaseId::new(13));
    let _reap = MonitorReapGuard::after_launch(&settled);
}

#[test]
fn start_no_worktree_uses_feature_branch() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[(
        "claude",
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
    )]);

    // Legacy launch pinned deliberately (34-06b): the subject is that
    // `--no-worktree` keeps the phase on its feature branch, not which
    // transport its agent is launched over. See `run_devflow_legacy_launch`.
    run_devflow_legacy_launch(
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "12",
            "--agent",
            "claude",
            // `supervise` (35.1-03): same reason as phase 11 — the legacy
            // opt-out pinned above is refused in `Mode::Auto`, and
            // `--no-worktree`'s branch placement is mode-independent.
            "--mode",
            "supervise",
            "--no-worktree",
        ],
    );

    // start returns before the detached monitor finishes; wait for the
    // agent pid file that the monitor writes on the feature-branch path.
    wait_for(&root.join(".devflow/phase-12-agent-pid"));
    assert!(!root.join(".worktrees/phase-12").exists());

    let state = devflow_core::workflow::load_state(root, PhaseId::new(12)).expect("load state");
    assert!(
        state.worktree_path.is_none(),
        "expected worktree_path to be None with --no-worktree, got {:?}",
        state.worktree_path
    );
    // Bind the guard to the SETTLED state (the chain blocks at the Validate
    // gate in supervise mode — that last monitor is this test's to reap).
    let settled = wait_for_settled(root, PhaseId::new(12));
    let _reap = MonitorReapGuard::after_launch(&settled);
}

/// 20c (D-09 + review: Codex HIGH off-by-one): `devflow start --until plan`
/// must run Define AND Plan to completion, then halt BEFORE advancing to
/// Code — not stop before Plan ever runs. The fake `claude` script always
/// reports success, so the monitor chain runs Define→advance→Plan→advance;
/// the second `advance` calls `transition(.., Stage::Code)` with
/// `state.stage == Plan`, which is exactly the `stop_until == Some(from)`
/// case this plan adds.
#[test]
fn start_until_plan_halts_cleanly() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[(
        "claude",
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
    )]);

    // Legacy launch pinned deliberately (34-06b): the subject is WHERE the
    // `--until` cap halts the stage machine, not which transport each stage's
    // agent is launched over. The opt-out reaches Plan too — it is persisted
    // into `state.legacy_claude_launch` by `start`, so the detached monitor
    // that launches Plan reads it back. See `run_devflow_legacy_launch`.
    run_devflow_legacy_launch(
        root,
        &fake_bin.path,
        &[
            // `supervise` (35.1-03): same reason as phase 11 above — the
            // legacy opt-out is refused in `Mode::Auto`, and the `--until` cap
            // halts at Plan, well before supervise's Validate gate.
            "start",
            "--phase",
            "44",
            "--agent",
            "claude",
            "--mode",
            "supervise",
            "--until",
            "plan",
        ],
    );

    let state = wait_for_stopped(root, PhaseId::new(44));
    assert_eq!(
        state.stage,
        devflow_core::stage::Stage::Plan,
        "the persisted stage must be the COMPLETED target (Plan), proving Plan ran \
         before the halt — not that the pipeline stopped before Plan ever launched"
    );
    assert!(state.stopped, "stop marker must be set");
    assert_eq!(
        state.monitor_pid, None,
        "the stop path must clear monitor_pid so no monitor is left behind"
    );
    assert!(
        state.stop_reason.is_some(),
        "a human-readable stop_reason must be recorded"
    );
    // The stop path cleared monitor_pid; the guard still binds (no-op) so
    // every monitor-spawning test in this file follows the same teardown
    // shape (41-02 Task 1 systematic pass).
    let _reap = MonitorReapGuard::after_launch(&state);
}

/// 20c (D-07): `--until ship` is a semantic no-op — Ship never calls
/// `transition` (`handle_ship_outcome` calls `finish_workflow` directly), so
/// the full pipeline already stops there today. It must be rejected before
/// any stage runs, not silently accepted as if it intercepted anything.
#[test]
fn start_until_ship_is_rejected() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[(
        "claude",
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
    )]);

    let output = Command::new(devflow_bin())
        .args([
            "start", "--phase", "45", "--agent", "claude", "--mode", "auto", "--until", "ship",
        ])
        .arg(root)
        .env("PATH", path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow");

    assert!(
        !output.status.success(),
        "--until ship must be rejected, not silently accepted"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ship") && stderr.contains("no-op"),
        "the rejection must explain Ship is already terminal\nstderr: {stderr}"
    );
    assert!(
        !root.join(".worktrees/phase-45").exists(),
        "a rejected --until ship must not run any stage or create a worktree"
    );
}

/// 20c edge-probe (20c/empty): `--until bogus` needs no new parsing surface —
/// it is rejected by the existing `Stage: FromStr` parser (via clap) before
/// `start` is ever dispatched.
#[test]
fn start_until_unknown_stage_is_rejected_by_clap() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[(
        "claude",
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
    )]);

    let output = Command::new(devflow_bin())
        .args([
            "start", "--phase", "46", "--agent", "claude", "--mode", "auto", "--until", "bogus",
        ])
        .arg(root)
        .env("PATH", path_with_fake_bin(&fake_bin.path))
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

/// WR-01 (phase 20 review): `--dry-run` must reflect `--until` in its
/// preview instead of always printing the full Define→Ship pipeline as if
/// `--until` had not been passed.
#[test]
fn start_dry_run_annotates_until_stage() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[]);

    let output = Command::new(devflow_bin())
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
        .env("PATH", path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow");

    assert!(
        output.status.success(),
        "dry-run must not fail\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|l| l.contains("plan") && l.contains("[STOPS HERE — --until]")),
        "the plan stage line must be annotated as the --until stop point\nstdout: {stdout}"
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
/// annotation — the full pipeline runs to Ship.
#[test]
fn start_dry_run_without_until_has_no_stop_annotation() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[]);

    let output = Command::new(devflow_bin())
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
        .env("PATH", path_with_fake_bin(&fake_bin.path))
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

#[test]
fn status_prints_cron_hint_when_cron_instructions_exist() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let instructions = devflow_core::ship::build_single_agent_cron_instructions(
        root,
        PhaseId::new(7),
        "2026-06-18T15:45:30Z",
    );
    devflow_core::ship::write_cron_instructions(root, &instructions).unwrap();
    let fake_bin = fake_bin_dir(&[]);

    let output = run_devflow(root, &fake_bin.path, &["status"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains(&format!(
        "Cron instruction pending (phase 7): hermes cron create --from-devflow {}",
        root.display()
    )));
}

#[test]
fn reference_and_cleanup_worktree_cli_flow() {
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

    // reference — creates static snapshot
    let out = run_devflow(root, &fake_bin.path, &["reference"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("reference worktree"));
    assert!(root.join(".worktrees/reference").is_dir());

    // start --worktree — creates phase worktree
    let out = run_devflow(
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "8",
            "--agent",
            "claude",
            "--mode",
            "auto",
            "--worktree",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("phase 8"));
    assert!(
        root.join(".worktrees/phase-08").is_dir(),
        "worktree not created"
    );

    // status — lists active worktrees
    let out = run_devflow(root, &fake_bin.path, &["status"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(".worktrees/reference"),
        "status missing reference\n{stdout}"
    );
    assert!(
        stdout.contains(".worktrees/phase-08"),
        "status missing phase worktree\n{stdout}"
    );

    // 20b: cleanup now hard-refuses while a monitor is still actively
    // driving the phase (Healthy/BetweenStages) — a real operator would
    // resolve the phase before cleaning it up, not race the still-running
    // monitor. This fixture's fake agent never produces real work, so
    // Validate always loops back and forces a gate after
    // MAX_CONSECUTIVE_FAILURES; abort it (note containing "abort" —
    // gates.rs::GateAction::from_response) so the monitor clears state,
    // then wait for that to land before invoking cleanup.
    wait_for(&root.join(".devflow/gates/08-validate.json"));

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
    // verified-reaped on EVERY exit path — the `gate reject` teardown below is
    // the happy path, not a substitute for the guard.
    let settled = load_state(root, PhaseId::new(8)).expect("load state");
    let _reap = MonitorReapGuard::after_launch(&settled);

    run_devflow(
        root,
        &fake_bin.path,
        &[
            "gate",
            "reject",
            "8",
            "--stage",
            "validate",
            "--note",
            "abort test teardown",
        ],
    );
    wait_for_state_cleared(root, PhaseId::new(8));

    // cleanup — removes worktrees
    let out = run_devflow(root, &fake_bin.path, &["cleanup", "--force"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("removed"));

    // cleanup --force removes everything including reference
    assert!(!root.join(".worktrees/reference").is_dir());
    assert!(!root.join(".worktrees/phase-08").is_dir());
}

/// 13-06 dogfood regression (Codex leg): a fresh headless Codex run can
/// never pass Define, so `start --agent codex` on a phase with no CONTEXT.md
/// on develop must fail fast in pre-flight — before any worktree, branch, or
/// monitor is created.
#[test]
fn start_codex_without_context_fails_preflight() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    // codex IS installed (the 13-06 dogfood scenario) — the binary preflight
    // (14-CR-05) passes and the CONTEXT.md artifact check must fire next.
    let fake_bin = fake_bin_dir(&[("codex", "#!/bin/sh\nexit 0\n")]);

    let output = Command::new(devflow_bin())
        .args([
            "start", "--phase", "42", "--agent", "codex", "--mode", "auto",
        ])
        .arg(root)
        .env("PATH", path_with_fake_bin(&fake_bin.path))
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

/// 20b instance 1 (D-06, review: Codex HIGH fail-closed-on-live-agent):
/// `cleanup --force` must refuse to remove a worktree whose agent pid is
/// genuinely alive, even when the persisted `State` carries `monitor_pid =
/// None` — a classification `liveness()` reports as `Unknown`, NOT `Healthy`.
/// A guard that only refuses on `Healthy`/`BetweenStages` would still delete
/// this worktree out from under a live agent. Against unmodified `cleanup`
/// (no liveness check at all today) this test FAILS: cleanup removes the
/// worktree unconditionally and exits 0.
#[test]
fn cleanup_force_refuses_on_live_agent_unknown_monitor() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let phase = PhaseId::new(8);
    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    seed_feature_branch(root, phase);

    let wt_path = root
        .join(".worktrees")
        .join(format!("phase-{padded}", padded = phase.padded()));
    devflow_core::worktree::add(root, &wt_path, &branch, &branch, false).unwrap();

    // The agent pid file holds a genuinely alive pid — the test process
    // itself, trivially alive for the test's duration.
    let pid_path = devflow_core::agent_result::agent_pid_path(root, phase);
    fs::create_dir_all(pid_path.parent().unwrap()).unwrap();
    fs::write(&pid_path, std::process::id().to_string()).unwrap();

    // Persist a State with monitor_pid = None (Unknown liveness) and
    // worktree_path pointing at the created worktree (the worktree->phase
    // join key, review: Codex MEDIUM).
    let mut state = devflow_core::state::State::new(
        phase,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Auto,
        root.to_path_buf(),
    );
    state.worktree_path = Some(wt_path.clone());
    devflow_core::workflow::save_state(&state).unwrap();

    let fake_bin = fake_bin_dir(&[]);
    let output = Command::new(devflow_bin())
        .args(["cleanup", "--force"])
        .arg(root)
        .env("PATH", path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow cleanup");

    assert!(
        !output.status.success(),
        "cleanup --force must refuse to remove a live agent's worktree even \
         under Unknown liveness (monitor_pid = None)"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("devflow resume"),
        "refusal must name `devflow resume` as the unblocking action, got:\n{combined}"
    );
    assert!(
        wt_path.is_dir(),
        "worktree must NOT have been removed while the agent is alive"
    );
}

/// 20b instance 1 (D-06, review: Codex HIGH fail-closed-on-live-agent),
/// case (b): a dead monitor (`Stuck` liveness) must NOT be treated as
/// "safe to proceed" when the agent it was watching is still alive — the
/// guard keys on agent liveness, not on the monitor's Healthy/BetweenStages
/// classification alone.
#[test]
fn cleanup_force_refuses_on_dead_monitor_live_agent() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let phase = PhaseId::new(9);
    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    seed_feature_branch(root, phase);

    let wt_path = root
        .join(".worktrees")
        .join(format!("phase-{padded}", padded = phase.padded()));
    devflow_core::worktree::add(root, &wt_path, &branch, &branch, false).unwrap();

    // Agent pid file holds a genuinely alive pid (the test process itself).
    let pid_path = devflow_core::agent_result::agent_pid_path(root, phase);
    fs::create_dir_all(pid_path.parent().unwrap()).unwrap();
    fs::write(&pid_path, std::process::id().to_string()).unwrap();

    // Persist a State with a DEAD monitor pid (`liveness()` classifies this
    // Stuck, since monitor_alive is false), plus worktree_path.
    let mut state = devflow_core::state::State::new(
        phase,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Auto,
        root.to_path_buf(),
    );
    state.worktree_path = Some(wt_path.clone());
    state.monitor_pid = Some(0x7FFF_FFFE); // essentially never a live pid
    devflow_core::workflow::save_state(&state).unwrap();

    let fake_bin = fake_bin_dir(&[]);
    let output = Command::new(devflow_bin())
        .args(["cleanup", "--force"])
        .arg(root)
        .env("PATH", path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow cleanup");

    assert!(
        !output.status.success(),
        "cleanup --force must refuse to remove a worktree whose agent is \
         alive even when its monitor is dead (Stuck liveness)"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("devflow resume"),
        "refusal must name `devflow resume` as the unblocking action, got:\n{combined}"
    );
    assert!(
        wt_path.is_dir(),
        "worktree must NOT have been removed while the agent is alive"
    );
}

/// CR-02 (phase 20 review): a phase halted via `devflow start --until
/// <stage>` clears `monitor_pid` and its agent has already exited by
/// design — `Liveness::Unknown` with `agent_alive == false` sails straight
/// through the live-agent refusal, so an ordinary `devflow cleanup` (no
/// `--force`) must not delete the worktree of a phase the operator parked
/// for a later `devflow resume`. `doctor`'s `check_dead_agent`/
/// `check_dead_monitor` were already taught about `facts.stopped` in this
/// same phase; `cleanup` must recognize `state.stopped` too.
#[test]
fn cleanup_keeps_worktree_for_until_stopped_phase_without_force() {
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
    // monitor_pid = None (cleared by the --until stop path) — Unknown
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
    let output = Command::new(devflow_bin())
        .args(["cleanup"])
        .arg(root)
        .env("PATH", path_with_fake_bin(&fake_bin.path))
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
    let output = Command::new(devflow_bin())
        .args(["cleanup", "--force"])
        .arg(root)
        .env("PATH", path_with_fake_bin(&fake_bin.path))
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

/// 20b instance 1 (probe 20b/idempotency): `cleanup` run twice succeeds —
/// the second run finds the worktree already gone and does not error.
#[test]
fn cleanup_is_idempotent_when_worktree_already_removed() {
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

    // Dead agent, dead monitor (Stuck liveness) — a genuinely dead phase,
    // safe for cleanup to proceed.
    let mut state = devflow_core::state::State::new(
        phase,
        devflow_core::state::AgentKind::Claude,
        devflow_core::mode::Mode::Auto,
        root.to_path_buf(),
    );
    state.worktree_path = Some(wt_path.clone());
    state.monitor_pid = Some(0x7FFF_FFFE);
    devflow_core::workflow::save_state(&state).unwrap();

    let fake_bin = fake_bin_dir(&[]);
    let first = Command::new(devflow_bin())
        .args(["cleanup", "--force"])
        .arg(root)
        .env("PATH", path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow cleanup (first)");
    assert!(
        first.status.success(),
        "first cleanup of a genuinely-dead phase must succeed\nstderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(!wt_path.is_dir(), "worktree must be removed on first run");

    let second = Command::new(devflow_bin())
        .args(["cleanup", "--force"])
        .arg(root)
        .env("PATH", path_with_fake_bin(&fake_bin.path))
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
/// `monitor_pid` is the LAST monitor the chain spawned, the one a
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
/// advance a commit-gated stage. Define (not commit-gated) legitimately
/// advances on exit 0, but Plan — a commit-gated stage — runs the same
/// marker-less stub, which produces no commits and no marker, so it is
/// `Failed` and gates instead of advancing to Code.
#[test]
fn pi_marker_less_run_does_not_advance() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[("pi", &pi_stub("printf 'fake pi, no marker\\n'\nexit 0\n"))]);

    run_devflow(
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
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[("pi", &pi_stub("printf 'fake pi, no marker\\n'\nexit 1\n"))]);

    run_devflow(
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

    let state = wait_for_gate(root, PhaseId::new(7));
    assert_eq!(
        state.stage,
        Stage::Define,
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
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[("pi", &pi_stub("exec sleep 30\n"))]);

    run_devflow(
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

    let pid_path = root.join(".devflow/phase-07-agent-pid");
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
        Stage::Define,
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
        Stage::Define,
        "a hung-then-killed run must not advance its stage"
    );
    let _reap = MonitorReapGuard::after_launch(&gated);
}

// ---------------------------------------------------------------------------
// Phase 41 Task 8 (ANTG-03, F4, codex-3, antigravity notice (a)): the
// Antigravity transport, end to end through `devflow start --agent
// antigravity` with a stubbed `agy` on PATH. The canary-aware stub answers
// the `DEVFLOW_DELIVERY_CANARY_` turn (AntigravityCanaryLauncher at Define),
// schema-checks every turn (codex-3), and then behaves per mode.
// ---------------------------------------------------------------------------

/// The canary-aware `agy` stub (F4).
///
/// Three behaviours, selected by `mode`. EVERY variant first answers a
/// `*DEVFLOW_DELIVERY_CANARY_*` turn (the common prefix; the enum is named
/// without it) by echoing the token inside an antigravity-shaped
/// `event:result.response`, so `run_delivery_canary` reaches `Confirmed` at
/// Define; then:
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
/// keyed to `state.monitor_pid`, bound strictly after the final
/// `&mut State` use. 41-02 Task 1 turns this into the systematic pass.
struct MonitorReapGuard {
    pid: Option<u32>,
}

impl MonitorReapGuard {
    fn after_launch(state: &devflow_core::state::State) -> Self {
        // Register the pid THIS guard is responsible for (the settled-state
        // monitor it will reap), and mark a guard in flight for the audit's
        // ordering barrier. `state` must be the SETTLED state
        // (`wait_for_settled` / `wait_for_gate`), whose `monitor_pid` is the
        // chain's LAST monitor — the one that would leak if not reaped.
        if let Some(pid) = state.monitor_pid {
            registry().lock().unwrap().insert(pid);
            ACTIVE_GUARDS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        Self {
            pid: state.monitor_pid,
        }
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
/// advance a COMMIT-GATED stage. Define (not commit-gated) legitimately
/// advances on exit 0; Plan — a commit-gated stage — produces no marker and
/// no commits, so it gates instead of advancing to Code.
#[test]
fn marker_less_antigravity_never_advances() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[("agy", &antigravity_stub(StubMode::Quiet))]);

    run_devflow(
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
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[("agy", &antigravity_stub(StubMode::MarkerStream))]);

    run_devflow(
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
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[("agy", &antigravity_stub(StubMode::InitOnly))]);

    run_devflow(
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
#[test]
fn suite_reap_audit() {
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

/// codex-4: a monitor the suite cannot account for is DETECTABLE. With NO
/// guard bound, a start leaves its monitor ALIVE; the test registers that
/// pid by hand — exactly what [`MonitorReapGuard::after_launch`] would have
/// registered had the test bound a guard — proves the detection helper FAILS
/// against it (the gate can redden), then reaps and deregisters through the
/// normal guard.
#[test]
fn unguarded_monitor_is_detected_by_the_registry() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    // A HANGING pi keeps the Define monitor alive past the launch — the
    // "leaked" shape this control must be able to see.
    let fake_bin = fake_bin_dir(&[("pi", &pi_stub("exec sleep 30\n"))]);

    run_devflow(
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

    let state = load_state(root, PhaseId::new(7)).expect("load state");
    let pid = state
        .monitor_pid
        .expect("a launched stage must record a monitor pid");
    assert!(
        devflow_core::agent::agent_running(pid),
        "premise: with no guard the monitor stays alive — nothing reaps it"
    );

    // Simulate what a guard's after_launch would have registered (the leak).
    registry().lock().unwrap().insert(pid);
    // The detection helper MUST see it — the audit can fail.
    let alive = registered_monitors_alive(&registry().lock().unwrap());
    assert!(
        alive.contains(&pid),
        "a live registered monitor must be detected: {alive:?}"
    );

    // Clean up through the normal guard: verified reap + deregister.
    let _reap = MonitorReapGuard::after_launch(&state);
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
/// commit-gated stage. Define advances on exit 0; Plan gates.
#[test]
fn hermes_marker_less_run_does_not_advance() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[("hermes", &hermes_stub("printf 'fake hermes, no marker\\n'\nexit 0\n"))]);

    run_devflow(
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
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[("hermes", &hermes_stub("printf 'fake hermes error\\n'\nexit 1\n"))]);

    run_devflow(
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

    let state = wait_for_gate(root, PhaseId::new(7));
    assert_eq!(
        state.stage,
        Stage::Define,
        "a non-zero-exit hermes run must not advance its stage"
    );
    assert!(state.gate_pending, "the never-silent gate must have fired");
    let _reap = MonitorReapGuard::after_launch(&state);
}

/// HRMS-03: a hung Hermes process is detected as alive by monitor liveness,
/// does not falsely advance, and gates when killed.
#[test]
fn hermes_hung_process_is_detected_not_left_running() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let fake_bin = fake_bin_dir(&[("hermes", &hermes_stub("exec sleep 30\n"))]);

    run_devflow(
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

    let pid_path = root.join(".devflow/phase-07-agent-pid");
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
        Stage::Define,
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
        Stage::Define,
        "a hung-then-killed hermes run must not advance its stage"
    );
    let _reap = MonitorReapGuard::after_launch(&gated);
}
