//! Behavioural tests for the gitleaks secret scan in `scripts/hooks/pre-commit`.
//!
//! Each test commits in a scratch repository whose `core.hooksPath` borrows
//! this checkout's `scripts/hooks`, with a stand-in `gitleaks` on `PATH` so the
//! result does not depend on whether the real one is installed (the CI image
//! has none). Global and system git config are nulled, so the hook's chain to
//! a machine-wide hook never runs and cannot mask the repo's own scan.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Directories the hook needs (git, bash, awk, grep, sed) and that must not
/// hold a real `gitleaks`, so the missing-scanner case really is missing.
const SYSTEM_PATH: &str = "/usr/bin:/bin";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

/// A stand-in `gitleaks` that appends its arguments to `args.log` beside it
/// and exits with `exit_code`.
fn fake_gitleaks(dir: &Path, exit_code: i32) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).expect("create fake bin dir");
    let script = bin.join("gitleaks");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$(dirname \"$0\")/args.log\"\nexit {exit_code}\n"
        ),
    )
    .expect("write fake gitleaks");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("chmod fake gitleaks");
    bin
}

fn git(repo: &Path, path: &str, args: &[&str]) -> Output {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo)
        .args(args)
        .env("PATH", path)
        .env("HOME", repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_CONFIG",
        "GIT_PREFIX",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        cmd.env_remove(var);
    }
    cmd.output().expect("run git")
}

/// A scratch repository on a feature branch, hooked to this checkout's hooks,
/// with one file staged.
fn staged_repo(path: &str) -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("create scratch repo");
    let hooks = repo_root().join("scripts/hooks");
    for args in [
        vec!["init", "-q", "-b", "feature/secret-scan"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "test"],
        vec![
            "config",
            "core.hooksPath",
            hooks.to_str().expect("utf-8 hooks path"),
        ],
    ] {
        let out = git(repo.path(), path, &args);
        assert!(out.status.success(), "git {args:?}: {}", describe(&out));
    }
    std::fs::write(repo.path().join("file.txt"), "content\n").expect("write file");
    let out = git(repo.path(), path, &["add", "file.txt"]);
    assert!(out.status.success(), "git add: {}", describe(&out));
    repo
}

fn commit(repo: &Path, path: &str) -> Output {
    git(repo, path, &["commit", "-q", "-m", "test: commit"])
}

fn has_commit(repo: &Path, path: &str) -> bool {
    git(repo, path, &["rev-parse", "--verify", "-q", "HEAD"])
        .status
        .success()
}

fn describe(out: &Output) -> String {
    format!(
        "status {:?}, stdout {:?}, stderr {:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn a_commit_is_refused_when_gitleaks_reports_a_finding() {
    let fake = tempfile::tempdir().expect("create fake dir");
    let path = format!("{}:{SYSTEM_PATH}", fake_gitleaks(fake.path(), 1).display());
    let repo = staged_repo(&path);

    let out = commit(repo.path(), &path);

    assert!(
        !out.status.success(),
        "the commit must be refused: {}",
        describe(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("gitleaks refused the staged changes"),
        "the refusal must name the scan: {}",
        describe(&out)
    );
    assert!(!has_commit(repo.path(), &path), "no commit may be written");
}

#[test]
fn a_clean_scan_commits_and_scans_only_the_staged_changes() {
    let fake = tempfile::tempdir().expect("create fake dir");
    let bin = fake_gitleaks(fake.path(), 0);
    let path = format!("{}:{SYSTEM_PATH}", bin.display());
    let repo = staged_repo(&path);

    let out = commit(repo.path(), &path);

    assert!(
        out.status.success(),
        "a clean scan must commit: {}",
        describe(&out)
    );
    assert!(has_commit(repo.path(), &path), "the commit must be written");
    let args = std::fs::read_to_string(bin.join("args.log")).expect("gitleaks was not run");
    let args = args.trim_end();
    assert!(
        args.starts_with("git ")
            && args.contains("--pre-commit")
            && args.contains("--staged")
            && args.contains("--redact"),
        "gitleaks must scan the staged diff with secrets redacted, got {args:?}"
    );
}

#[test]
fn a_commit_is_refused_when_gitleaks_is_not_installed() {
    assert!(
        !Path::new("/usr/bin/gitleaks").exists() && !Path::new("/bin/gitleaks").exists(),
        "precondition: a gitleaks under {SYSTEM_PATH} would make this case vacuous"
    );
    let repo = staged_repo(SYSTEM_PATH);

    let out = commit(repo.path(), SYSTEM_PATH);

    assert!(
        !out.status.success(),
        "the commit must be refused: {}",
        describe(&out)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("gitleaks is not on PATH"),
        "the refusal must say the scanner is missing: {}",
        describe(&out)
    );
    assert!(
        !has_commit(repo.path(), SYSTEM_PATH),
        "no commit may be written"
    );
}
