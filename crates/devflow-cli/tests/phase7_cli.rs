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
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
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
    let output = Command::new(devflow_bin())
        .args(args)
        .arg(root)
        .env("PATH", path_with_fake_bin(fake_bin))
        .env("DEVFLOW_TEST_ROOT", root)
        .current_dir(root)
        .output()
        .expect("run devflow");
    assert!(
        output.status.success(),
        "devflow {args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
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
fn wait_for_state_cleared(root: &Path, phase: u32) {
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
fn wait_for_stopped(root: &Path, phase: u32) -> devflow_core::state::State {
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

fn git_stdout(root: &Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&git(root, args).stdout)
        .trim()
        .to_string()
}

fn seed_feature_branch(root: &Path, phase: u32) {
    let branch = format!("feature/phase-{phase:02}");
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
    let fake_bin = fake_bin_dir(&[
        (
            "claude",
            "#!/bin/sh\nprintf 'fake claude\\nDEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
        ),
        (
            "codex",
            "#!/bin/sh\nprintf 'fake codex\\nDEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
        ),
    ]);

    let output = run_devflow(
        root,
        &fake_bin.path,
        &["parallel", "--phases", "7,8", "--agents", "claude,codex"],
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
    let state7 = devflow_core::workflow::load_state(root, 7).expect("phase 7 state");
    let state8 = devflow_core::workflow::load_state(root, 8).expect("phase 8 state");
    assert_eq!(state7.phase, 7);
    assert_eq!(state8.phase, 8);
    assert!(
        !root.join(".devflow/state.json").exists(),
        "legacy single-slot state.json must not be written anymore"
    );
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
    run_devflow(
        root,
        &fake_bin.path,
        &[
            "start", "--phase", "11", "--agent", "claude", "--mode", "auto",
        ],
    );

    // start returns before the detached monitor finishes; wait for the
    // worktree directory like the other integration tests do.
    wait_for(&root.join(".worktrees/phase-11"));
    assert!(root.join(".worktrees/phase-11").is_dir());

    let state = devflow_core::workflow::load_state(root, 11).expect("load state");
    assert!(
        state.worktree_path.is_some(),
        "expected worktree_path to be Some(_) by default, got {:?}",
        state.worktree_path
    );
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
    // succeed (run_devflow asserts a zero exit status) despite the main
    // checkout being 51 commits behind develop.
    run_devflow(
        root,
        &fake_bin.path,
        &[
            "start", "--phase", "13", "--agent", "claude", "--mode", "auto",
        ],
    );

    wait_for(&root.join(".worktrees/phase-13"));
    assert!(root.join(".worktrees/phase-13").is_dir());
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

    run_devflow(
        root,
        &fake_bin.path,
        &[
            "start",
            "--phase",
            "12",
            "--agent",
            "claude",
            "--mode",
            "auto",
            "--no-worktree",
        ],
    );

    // start returns before the detached monitor finishes; wait for the
    // agent pid file that the monitor writes on the feature-branch path.
    wait_for(&root.join(".devflow/phase-12-agent-pid"));
    assert!(!root.join(".worktrees/phase-12").exists());

    let state = devflow_core::workflow::load_state(root, 12).expect("load state");
    assert!(
        state.worktree_path.is_none(),
        "expected worktree_path to be None with --no-worktree, got {:?}",
        state.worktree_path
    );
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

    run_devflow(
        root,
        &fake_bin.path,
        &[
            "start", "--phase", "44", "--agent", "claude", "--mode", "auto", "--until", "plan",
        ],
    );

    let state = wait_for_stopped(root, 44);
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

#[test]
fn sequentagent_integrates_agent_a_then_rebases_agent_b() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    seed_feature_branch(root, 7);
    let fake_bin = fake_bin_dir(&[
        (
            "claude",
            "#!/bin/sh\n\
             echo workA\n\
             printf 'from A\\n' > a.txt\n\
             git add a.txt\n\
             git commit --allow-empty -m A\n\
             printf 'DEVFLOW_RESULT: {\"status\":\"success\",\"commits\":1}\\n'\n",
        ),
        (
            "codex",
            "#!/bin/sh\n\
             if test -f a.txt; then printf 'saw A\\n' > \"$DEVFLOW_TEST_ROOT/b-saw-a\"; fi\n\
             git log --oneline > \"$DEVFLOW_TEST_ROOT/b-log\"\n\
             printf 'from B\\n' > b.txt\n\
             git add b.txt\n\
             git commit --allow-empty -m B\n\
             printf 'DEVFLOW_RESULT: {\"status\":\"success\",\"commits\":1}\\n'\n",
        ),
    ]);

    let output = run_devflow(
        root,
        &fake_bin.path,
        &[
            "sequentagent",
            "--phase",
            "7",
            "--agents",
            "claude,codex",
            "--force",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("sequentagent complete"));

    let base_log = git_stdout(root, &["log", "--oneline", "feature/phase-07"]);
    assert!(
        base_log.contains(" A"),
        "base branch missing A:\n{base_log}"
    );
    assert!(
        base_log.contains(" B"),
        "base branch missing B:\n{base_log}"
    );

    let b_log = git_stdout(root, &["log", "--oneline", "feature/phase-07-codex"]);
    assert!(
        b_log.contains(" A"),
        "agent B branch was not rebased onto A:\n{b_log}"
    );
    assert!(b_log.contains(" B"), "agent B branch missing B:\n{b_log}");
    assert!(
        root.join("b-saw-a").exists(),
        "agent B did not see A's file"
    );
}

/// CR-02 completion: Layer-1 silence is not success. An agent that crashes —
/// nonzero exit, no DEVFLOW_RESULT marker, no envelope — must fail
/// sequentagent instead of being silently fast-forwarded into the shared
/// base branch and handed off to agent B.
#[test]
fn sequentagent_fails_when_agent_a_crashes_without_result() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    seed_feature_branch(root, 7);
    let fake_bin = fake_bin_dir(&[
        (
            "claude",
            "#!/bin/sh\n\
             echo 'starting work'\n\
             exit 3\n",
        ),
        (
            "codex",
            "#!/bin/sh\n\
             : > \"$DEVFLOW_TEST_ROOT/crash-b-ran\"\n\
             printf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
        ),
    ]);

    let output = Command::new(devflow_bin())
        .args([
            "sequentagent",
            "--phase",
            "7",
            "--agents",
            "claude,codex",
            "--force",
        ])
        .arg(root)
        .env("PATH", path_with_fake_bin(&fake_bin.path))
        .env("DEVFLOW_TEST_ROOT", root)
        .current_dir(root)
        .output()
        .expect("run devflow");

    assert!(
        !output.status.success(),
        "sequentagent must fail when agent A crashes without a result\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("agent A (claude) failed")
            && stderr.contains("exited with code 3 without reporting a result"),
        "expected agent-A crash failure, got:\n{stderr}"
    );
    assert!(
        !root.join("crash-b-ran").exists(),
        "agent B must not run after agent A crashes"
    );
}

#[test]
fn sequentagent_hands_off_after_rate_limit_and_writes_cron_instructions() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    seed_feature_branch(root, 7);
    let fake_bin = fake_bin_dir(&[
        (
            "claude",
            "#!/bin/sh\n\
             printf 'partial from A\\n' > a.txt\n\
             git add a.txt\n\
             git commit -q --allow-empty -m A\n\
             printf '{\"type\":\"result\",\"subtype\":\"error_rate_limit\",\"retry_after\":\"2026-06-18T15:45:30Z\",\"result\":\"rate limited\"}\\n'\n",
        ),
        (
            "codex",
            "#!/bin/sh\n\
             if test -f a.txt; then printf 'saw A\\n' > \"$DEVFLOW_TEST_ROOT/rate-limit-b-saw-a\"; fi\n\
             printf 'from B\\n' > b.txt\n\
             git add b.txt\n\
             git commit -q --allow-empty -m B\n\
             printf 'DEVFLOW_RESULT: {\"status\":\"success\",\"commits\":1}\\n'\n",
        ),
    ]);

    let output = run_devflow(
        root,
        &fake_bin.path,
        &[
            "sequentagent",
            "--phase",
            "7",
            "--agents",
            "claude,codex",
            "--force",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Agent A rate-limited; handing off to agent B"));
    assert!(stdout.contains("sequentagent complete"));

    let base_log = git_stdout(root, &["log", "--oneline", "feature/phase-07"]);
    assert!(
        base_log.contains(" A"),
        "base branch missing A:\n{base_log}"
    );
    assert!(
        base_log.contains(" B"),
        "base branch missing B:\n{base_log}"
    );
    assert!(
        root.join("rate-limit-b-saw-a").exists(),
        "agent B did not run after A's rate limit"
    );

    // The cron instructions are written when A rate-limits — and the message
    // must name the per-phase file that is actually written (14-CR-08).
    assert!(
        stdout.contains("wrote .devflow/cron-instructions-07.json"),
        "message must name the real per-phase cron file:\n{stdout}"
    );
    // ...but WR-02 (13-REVIEW.md): once B completes successfully the phase
    // has shipped, so the stale file must be deleted rather than surviving
    // to mislead `devflow status`/a Hermes cron into re-running a completed
    // phase.
    assert!(
        !root.join(".devflow/cron-instructions-07.json").exists(),
        "cron instructions should be deleted after a successful post-rate-limit handoff"
    );
    assert!(
        !root.join(".devflow/cron-instructions.json").exists(),
        "the legacy single-slot record must never be written"
    );
}

#[test]
fn status_prints_cron_hint_when_cron_instructions_exist() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    let instructions = devflow_core::ship::build_cron_instructions(
        root,
        7,
        "2026-06-18T15:45:30Z",
        "claude,codex",
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
    let fake_bin = fake_bin_dir(&[(
        "claude",
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\n'\n",
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
    wait_for_state_cleared(root, 8);

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
    let phase = 8;
    let branch = format!("feature/phase-{phase:02}");
    seed_feature_branch(root, phase);

    let wt_path = root.join(".worktrees").join(format!("phase-{phase:02}"));
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
    let phase = 9;
    let branch = format!("feature/phase-{phase:02}");
    seed_feature_branch(root, phase);

    let wt_path = root.join(".worktrees").join(format!("phase-{phase:02}"));
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
    let phase = 11;
    let branch = format!("feature/phase-{phase:02}");
    seed_feature_branch(root, phase);

    let wt_path = root.join(".worktrees").join(format!("phase-{phase:02}"));
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
    let phase = 13;
    let branch = format!("feature/phase-{phase:02}");
    seed_feature_branch(root, phase);

    let wt_path = root.join(".worktrees").join(format!("phase-{phase:02}"));
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
    let phase = 10;
    let branch = format!("feature/phase-{phase:02}");
    seed_feature_branch(root, phase);

    let wt_path = root.join(".worktrees").join(format!("phase-{phase:02}"));
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
