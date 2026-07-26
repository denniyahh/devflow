//! End-to-end proof that `devflow start` refuses to launch, and scaffolds
//! nothing, when the target phase is not reachable from `develop` — the
//! branch it forks its worktree (or feature branch) from. Reproduces the
//! exact 2026-07-26 acceptance-run failure shape (`23-FINDINGS.md` §B1):
//! Phase 24's ROADMAP heading and phase directory existed only on
//! `feature/phase-23`, never on `develop`, and `devflow start --phase 24`
//! floundered at Define for ~90 seconds before dying `workflow_aborted`.
//!
//! These tests run the real compiled binary and assert on its exit status,
//! stderr, and filesystem effects — never on `preflight.rs`/`commands.rs`
//! source text (test-signal-rejection pattern 4).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

/// Hermetic git invocation pinned to `root` (999.37) — never a bare
/// `Command::new("git")`.
fn git(root: &Path, args: &[&str]) -> Output {
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

struct FakeBin {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

/// A mode-0755 `claude` shell script so `ensure_agent_binary` passes on a
/// host with no real agent installed — the refusal under test must fire
/// before the agent is ever consulted, so the script's own behavior is
/// irrelevant, but its presence on PATH is required.
fn fake_bin_dir() -> FakeBin {
    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join("claude");
    fs::write(
        &claude,
        "#!/bin/sh\nprintf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
    )
    .unwrap();
    let mut perms = fs::metadata(&claude).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&claude, perms).unwrap();
    let path = dir.path().to_path_buf();
    FakeBin { _dir: dir, path }
}

/// Reproduces the exact 2026-07-26 shape: phase 24's ROADMAP heading and
/// `.planning/phases/24-*/` directory exist ONLY on `feature/phase-23`,
/// never on `develop` — the branch `devflow start` always forks its
/// worktree (or feature branch, in `--no-worktree` mode) from. Leaves the
/// working tree checked out on `feature/phase-23`, matching the tree the
/// 23-11 acceptance run launched from.
fn init_repo_with_phase_24_promoted_only_on_feature_branch(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);

    fs::create_dir_all(root.join(".planning")).unwrap();
    fs::write(
        root.join(".planning/ROADMAP.md"),
        "# Roadmap\n\n### Phase 1: Something else\n",
    )
    .unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "develop base — no phase 24"]);

    git(root, &["checkout", "-q", "-b", "feature/phase-23"]);
    fs::write(
        root.join(".planning/ROADMAP.md"),
        "# Roadmap\n\n### Phase 1: Something else\n\n### Phase 24: Acceptance target\n",
    )
    .unwrap();
    fs::create_dir_all(root.join(".planning/phases/24-acceptance-target")).unwrap();
    fs::write(
        root.join(".planning/phases/24-acceptance-target/.gitkeep"),
        "",
    )
    .unwrap();
    git(root, &["add", "-A"]);
    git(
        root,
        &[
            "commit",
            "-q",
            "-m",
            "promote phase 24 onto feature/phase-23 only",
        ],
    );
    // Deliberately left checked out on feature/phase-23 — the tree 23-11
    // launched `devflow start --phase 24` from.
}

fn path_with_fake_bin(fake_bin: &Path) -> String {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    format!("{}:{}", fake_bin.display(), existing.to_string_lossy())
}

#[test]
fn start_refuses_a_phase_promoted_only_on_the_working_branch_and_scaffolds_nothing() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo_with_phase_24_promoted_only_on_feature_branch(root);
    let fake_bin = fake_bin_dir();

    let output = Command::new(devflow_bin())
        .args([
            "start", "--phase", "24", "--agent", "claude", "--mode", "auto",
        ])
        .arg(root)
        .env("PATH", path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "devflow start must refuse an unreachable phase, but exited successfully\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("is not reachable from"),
        "stderr must carry the refusal phrase `is not reachable from`, got:\n{stderr}"
    );
    assert!(
        stderr.contains("develop"),
        "stderr must name the base branch `develop`, got:\n{stderr}"
    );
    assert!(
        stderr.contains("### Phase 24:"),
        "stderr must name the missing roadmap heading `### Phase 24:`, got:\n{stderr}"
    );
    assert!(
        stderr.contains(".planning/phases/24-"),
        "stderr must name the missing phase directory `.planning/phases/24-`, got:\n{stderr}"
    );

    assert!(
        !root.join(".worktrees/phase-24").exists(),
        "a refused start must not scaffold a phase-24 worktree"
    );
    assert!(
        !root.join(".worktrees").exists(),
        "a refused start must not create .worktrees at all"
    );
    assert!(
        !root.join(".devflow/state-24.json").exists(),
        "a refused start must not persist phase-24 state"
    );

    let branch_list = devflow_core::test_support::git_command(root)
        .args(["branch", "--list", "feature/phase-24"])
        .output()
        .expect("spawn git branch --list");
    assert!(
        String::from_utf8_lossy(&branch_list.stdout)
            .trim()
            .is_empty(),
        "a refused start must not create feature/phase-24"
    );
}

/// Proves the refusal precedes `GitFlow::feature_start` too, not merely
/// `ensure_phase_worktree` — `--no-worktree` must be refused before any
/// branch is created.
#[test]
fn start_refuses_before_creating_the_feature_branch_in_no_worktree_mode() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo_with_phase_24_promoted_only_on_feature_branch(root);
    let fake_bin = fake_bin_dir();

    let output = Command::new(devflow_bin())
        .args([
            "start",
            "--phase",
            "24",
            "--agent",
            "claude",
            "--mode",
            "auto",
            "--no-worktree",
        ])
        .arg(root)
        .env("PATH", path_with_fake_bin(&fake_bin.path))
        .current_dir(root)
        .output()
        .expect("run devflow");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "devflow start --no-worktree must refuse an unreachable phase, but exited \
         successfully\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("is not reachable from"),
        "stderr must carry the refusal phrase `is not reachable from`, got:\n{stderr}"
    );

    let branch_list = devflow_core::test_support::git_command(root)
        .args(["branch", "--list", "feature/phase-24"])
        .output()
        .expect("spawn git branch --list");
    assert!(
        String::from_utf8_lossy(&branch_list.stdout)
            .trim()
            .is_empty(),
        "a refused --no-worktree start must not create feature/phase-24 either"
    );
}
