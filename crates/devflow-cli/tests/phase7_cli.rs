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
/// transition's `cleanup_phase_files` briefly deletes the pid file before
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
fn devflow_ignores_stray_devflow_yaml() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    fs::write(
        root.join(".devflow.yaml"),
        "this: is: deliberately: not: valid: config",
    )
    .unwrap();
    let fake_bin = fake_bin_dir(&[]);

    let output = run_devflow(root, &fake_bin.path, &["doctor"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("devflow v1.2.0"));
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
    wait_for(&phase7_stdout);
    wait_for(&phase8_stdout);

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
    assert!(phase7_stdout.exists());
    assert!(phase8_stdout.exists());
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

    // Branch off develop, then leave develop far ahead (60 commits — past
    // the `behind > 50` hard-fail threshold) while the main checkout stays
    // on the stale branch.
    git(root, &["checkout", "-q", "-b", "ancient", "develop"]);
    git(root, &["checkout", "-q", "develop"]);
    for i in 0..60 {
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
    // checkout being 60 commits behind develop.
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
