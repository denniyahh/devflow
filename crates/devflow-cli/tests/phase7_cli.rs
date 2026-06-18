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
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    git(root, &["branch", "main"]);
    write_config(root);
}

fn write_config(root: &Path) {
    fs::write(
        root.join(".devflow.yaml"),
        "version:\n  scheme: semver\n  file: Cargo.toml\n  field: package.version\n  build_number: git\n\
         automation:\n  auto_branch: true\n  auto_verify: false\n  auto_docs: false\n  auto_version: patch\n  auto_ship: false\n  auto_cleanup: true\n  verify_command: \"true\"\n  lint_command: \"true\"\n  docs_command: \"true\"\n  continue_on_error: false\n  docs_auto_commit: false\n\
         git_flow:\n  main: main\n  develop: develop\n  feature_prefix: feature/\n",
    )
    .unwrap();
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
    wait_for(&phase7_pid);
    wait_for(&phase8_pid);
    assert!(
        fs::read_to_string(phase7_pid)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap()
            > 0
    );
    assert!(
        fs::read_to_string(phase8_pid)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap()
            > 0
    );

    assert!(root.join(".devflow/state.json").exists());
    assert!(root.join(".devflow/phase-07-stdout").exists());
    assert!(root.join(".devflow/phase-08-stdout").exists());
}
