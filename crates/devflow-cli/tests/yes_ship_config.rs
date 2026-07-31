//! D-12 (`28-CONTEXT.md`, `28-06-PLAN.md` Task 2): CLI-boundary coverage of
//! `devflow start`'s combined resolution of the `--yes-ship` flag and a
//! `yes_ship` key in `devflow.toml`. All five behavior cases run through
//! `--dry-run` — a cheap, no-worktree observation point, matching
//! `phase7_cli.rs`'s `start_dry_run_annotates_until_stage` pattern.
//!
//! Does NOT set `DEVFLOW_YES_SHIP` here — env precedence is already covered
//! by `devflow-core`'s `config::tests::env_overrides_file_yes_ship`, and the
//! binary here runs as a separate process so this suite stays out of the
//! `ENV_MUTEX`-guarded unit-test convention.

use std::fs;
use std::path::Path;
use std::process::Command;

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

fn git(root: &Path, args: &[&str]) {
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
}

/// Copied from `phase7_cli.rs`'s `init_repo` — the hermetic fixture-repo
/// shape shared by every CLI-boundary test that invokes the built binary.
fn init_repo(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "tag.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["config", "core.fsyncObjectFiles", "true"]);
    git(root, &["config", "core.fsync", "all"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    let dir = root.join(".planning/phases/60-test");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("60-CONTEXT.md"), "ctx\n").unwrap();
    fs::write(dir.join("60-01-PLAN.md"), "plan\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    git(root, &["branch", "main"]);
}

fn run_dry_run(root: &Path, phase: u32, extra_args: &[&str]) -> String {
    let mut args = vec![
        "start".to_string(),
        "--phase".to_string(),
        phase.to_string(),
        "--agent".to_string(),
        "claude".to_string(),
        "--mode".to_string(),
        "auto".to_string(),
        "--dry-run".to_string(),
    ];
    args.extend(extra_args.iter().map(|s| s.to_string()));

    let output = Command::new(devflow_bin())
        .args(&args)
        .arg(root)
        .current_dir(root)
        .output()
        .expect("run devflow");

    assert!(
        output.status.success(),
        "dry-run must not fail\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Case 1: no `devflow.toml`, no flag → not pre-authorized.
#[test]
fn no_config_no_flag_is_not_preauthorized() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);

    let stdout = run_dry_run(root, 60, &[]);

    assert!(
        stdout.contains("ship gate: not pre-authorized"),
        "expected not-pre-authorized report\nstdout: {stdout}"
    );
    assert!(
        !stdout.contains("devflow.toml"),
        "no source notice must appear when nothing pre-authorized\nstdout: {stdout}"
    );
}

/// Case 2: `devflow.toml` sets `yes_ship = true`, no flag → pre-authorized,
/// and stdout names `devflow.toml` as the source (the never-silent notice).
#[test]
fn config_true_no_flag_is_preauthorized_and_announces_source() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    fs::write(root.join("devflow.toml"), "yes_ship = true\n").unwrap();

    let stdout = run_dry_run(root, 61, &[]);

    assert!(
        stdout.contains("ship gate: pre-authorized"),
        "expected pre-authorized report\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("devflow.toml"),
        "a config-sourced authorization must name devflow.toml on stdout\nstdout: {stdout}"
    );
}

/// Case 3: `--yes-ship` flag, no `devflow.toml` → pre-authorized, and stdout
/// does NOT claim a config source (the flag is the source here).
#[test]
fn flag_no_config_is_preauthorized_without_config_claim() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);

    let stdout = run_dry_run(root, 62, &["--yes-ship"]);

    assert!(
        stdout.contains("ship gate: pre-authorized"),
        "expected pre-authorized report\nstdout: {stdout}"
    );
    assert!(
        !stdout.contains("devflow.toml"),
        "a flag-sourced authorization must not claim a config source\nstdout: {stdout}"
    );
}

/// Case 4: `--yes-ship` flag with `devflow.toml` set `false` → still
/// pre-authorized (the flag ORs in; it has no negative form).
#[test]
fn flag_overrides_false_config() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    fs::write(root.join("devflow.toml"), "yes_ship = false\n").unwrap();

    let stdout = run_dry_run(root, 63, &["--yes-ship"]);

    assert!(
        stdout.contains("ship gate: pre-authorized"),
        "the CLI flag must win regardless of a false config value\nstdout: {stdout}"
    );
}

/// Case 5: `devflow.toml` sets `yes_ship = false`, no flag → not
/// pre-authorized.
#[test]
fn config_false_no_flag_is_not_preauthorized() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    fs::write(root.join("devflow.toml"), "yes_ship = false\n").unwrap();

    let stdout = run_dry_run(root, 64, &[]);

    assert!(
        stdout.contains("ship gate: not pre-authorized"),
        "expected not-pre-authorized report\nstdout: {stdout}"
    );
}
