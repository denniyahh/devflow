//! Behavioural test for `scripts/scratch-dogfood-repo.sh --live-payload`.
//!
//! `devflow start` refuses a base branch whose phase has no `-CONTEXT.md`, so
//! the fixture must commit one before `integration` forks. The test scaffolds
//! a payload into a scratch directory outside this checkout and reads the file
//! from the `integration` branch.
//!
//! Every spawned command gets a scratch `HOME` and `XDG_CONFIG_HOME`, no global
//! or system git config and no inherited repository-local `GIT_*` variable.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CONTEXT_PATH: &str = ".planning/phases/01-retry-budget/01-CONTEXT.md";
const ABSENT_PATH: &str = ".planning/phases/01-retry-budget/01-NOPE.md";

/// Cargo test binaries run with cwd = the crate dir; the script lives at the
/// repo root.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

fn describe(out: &Output) -> String {
    format!(
        "status {:?}, stdout {:?}, stderr {:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A command that reads no git config or repository state from outside the
/// scratch `home`.
fn hermetic(program: &str, cwd: &Path, home: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(cwd)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("HOME", home)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    for name in devflow_core::test_support::REPO_LOCAL_GIT_VARS
        .iter()
        .chain(devflow_core::test_support::ALSO_REDIRECTING_GIT_VARS)
    {
        cmd.env_remove(name);
    }
    cmd
}

/// Whether `git cat-file -e integration:<path>` finds the file.
fn on_integration(payload: &Path, home: &Path, path: &str) -> bool {
    hermetic("git", payload, home)
        .args(["cat-file", "-e", &format!("integration:{path}")])
        .output()
        .expect("run git cat-file")
        .status
        .success()
}

#[test]
fn the_live_payload_commits_a_context_file_on_integration() {
    let scratch = tempfile::tempdir().expect("create scratch dir");
    let home = scratch.path().join("home");
    std::fs::create_dir_all(home.join(".config")).expect("create scratch home");
    let payload = scratch.path().join("lp");

    let out = hermetic("bash", &repo_root(), &home)
        .arg("scripts/scratch-dogfood-repo.sh")
        .arg("--live-payload")
        .arg(&payload)
        .output()
        .expect("run scripts/scratch-dogfood-repo.sh --live-payload");
    assert!(
        out.status.success(),
        "--live-payload failed: {}",
        describe(&out)
    );

    assert!(
        on_integration(&payload, &home, CONTEXT_PATH),
        "the live payload has no CONTEXT.md on integration, so `start --agent codex` refuses it"
    );
    assert!(
        !on_integration(&payload, &home, ABSENT_PATH),
        "cat-file found {ABSENT_PATH}, which the payload never writes, so the check cannot fail"
    );
}
