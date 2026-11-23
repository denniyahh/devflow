//! Behavioural tests for the stale-state marker that `scripts/hooks/post-commit`
//! writes when a commit lands a plan SUMMARY.
//!
//! Each test builds a scratch repository whose `core.hooksPath` points at a
//! `hook-checkout` directory beside it holding a byte copy of this checkout's
//! hook, then commits a plan SUMMARY. The tests prove the marker lands inside
//! the git dir, so the commit leaves `git status --porcelain` empty, and that
//! the marker is JSON with one entry per landed summary.
//!
//! Every spawned command gets a scratch `HOME` and `XDG_CONFIG_HOME`, no global
//! or system git config and no inherited repository-local `GIT_*` variable.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use devflow_core::test_support::write_executable;

const HOOK: &str = "scripts/hooks/post-commit";
const MARKER_NAME: &str = "devflow-stale-state.json";
const FIRST_SUMMARY: &str = ".planning/phases/01-x/01-01-SUMMARY.md";
const SECOND_SUMMARY: &str = ".planning/phases/01-x/01-02-SUMMARY.md";

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

/// A scratch workspace-branch repository with the hook installed through
/// `core.hooksPath` and a base commit carrying a plan but no summary.
struct Fixture {
    scratch: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let fixture = Self {
            scratch: tempfile::tempdir().expect("create scratch dir"),
        };
        let root = fixture.scratch.path();
        std::fs::create_dir_all(root.join("home/.config")).expect("create scratch home");
        let hook_source = repo_root().join(HOOK);
        let hook_text = std::fs::read_to_string(&hook_source)
            .unwrap_or_else(|error| panic!("read {}: {error}", hook_source.display()));
        let hook_checkout = root.join("hook-checkout");
        std::fs::create_dir_all(hook_checkout.join("scripts/hooks"))
            .expect("create the hook checkout");
        write_executable(&hook_checkout.join(HOOK), &hook_text);

        let init = fixture
            .hermetic("git", root)
            .args(["init", "-q", "-b", "workspace/x", "repo"])
            .output()
            .expect("run git init");
        assert!(init.status.success(), "init repo: {}", describe(&init));
        fixture.git(&["config", "user.email", "test@example.com"]);
        fixture.git(&["config", "user.name", "test"]);
        let hooks_path = hook_checkout.join("scripts/hooks");
        fixture.git(&["config", "core.hooksPath", &hooks_path.to_string_lossy()]);

        fixture.write(".workspace-divergence", "only scripts/hooks/post-commit\n");
        fixture.write(".planning/STATE.md", "---\nstatus: executing\n---\n");
        fixture.write(".planning/phases/01-x/01-01-PLAN.md", "plan\n");
        fixture.git(&["add", "-A"]);
        fixture.git(&["commit", "-q", "-m", "chore: base"]);
        fixture
    }

    fn repo(&self) -> PathBuf {
        self.scratch.path().join("repo")
    }

    /// A command that reads no git config or repository state from outside
    /// the scratch directory.
    fn hermetic(&self, program: &str, cwd: &Path) -> Command {
        let home = self.scratch.path().join("home");
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

    /// Runs a git command that must succeed and returns its trimmed stdout.
    fn git(&self, args: &[&str]) -> String {
        let out = self
            .hermetic("git", &self.repo())
            .args(args)
            .output()
            .unwrap_or_else(|error| panic!("run git {args:?}: {error}"));
        assert!(out.status.success(), "git {args:?}: {}", describe(&out));
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.repo().join(rel);
        std::fs::create_dir_all(path.parent().expect("a file path has a parent"))
            .expect("create parent dir");
        std::fs::write(&path, text).unwrap_or_else(|error| panic!("write {rel}: {error}"));
    }

    /// Commits the given summaries in one commit, which runs the hook.
    fn commit_summaries(&self, summaries: &[&str]) {
        for summary in summaries {
            self.write(summary, "summary\n");
        }
        let mut add = vec!["add", "--"];
        add.extend_from_slice(summaries);
        self.git(&add);
        self.git(&["commit", "-q", "-m", "docs: land summaries"]);
    }

    /// The text of the marker at the hook's git-dir path.
    fn marker_text(&self) -> String {
        let marker = PathBuf::from(self.git(&[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            MARKER_NAME,
        ]));
        std::fs::read_to_string(&marker).unwrap_or_else(|error| {
            panic!(
                "the stale-state marker is absent at {}: {error}",
                marker.display()
            )
        })
    }
}

#[test]
fn a_summary_commit_leaves_the_tree_clean() {
    let fixture = Fixture::new();
    fixture.commit_summaries(&[FIRST_SUMMARY]);

    let porcelain = fixture.git(&["status", "--porcelain", "--untracked-files=all"]);
    assert!(
        porcelain.is_empty(),
        "the post-commit hook left the tree dirty: {porcelain}"
    );
    let marker = fixture.marker_text();
    assert!(
        marker.contains("\"stale_since_commit\""),
        "the stale-state marker has no stale_since_commit: {marker}"
    );
}

#[test]
fn a_two_summary_commit_writes_a_marker_that_parses_as_json() {
    let fixture = Fixture::new();
    fixture.commit_summaries(&[FIRST_SUMMARY, SECOND_SUMMARY]);

    let text = fixture.marker_text();
    let marker: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("the stale-state marker is not JSON: {text} ({error})"));
    assert_eq!(
        marker["summaries_landed"],
        serde_json::json!([FIRST_SUMMARY, SECOND_SUMMARY]),
        "the stale-state marker does not list both landed summaries: {text}"
    );
}

/// F22: outside a workspace checkout (no `.workspace-divergence`) the hook skips its
/// workspace checks but still runs the post-commit the global `core.hooksPath` names.
#[test]
fn a_commit_outside_a_workspace_checkout_runs_the_global_post_commit() {
    let fixture = Fixture::new();
    fixture.git(&["rm", "-q", ".workspace-divergence"]);
    fixture.git(&["commit", "-q", "-m", "chore: leave the workspace branch"]);
    let scratch = fixture.scratch.path();
    let global_hooks = scratch.join("global-hooks");
    std::fs::create_dir_all(&global_hooks).expect("create the global hooks dir");
    let ran = scratch.join("global-post-commit-ran");
    write_executable(
        &global_hooks.join("post-commit"),
        &format!("#!/bin/sh\n: > '{}'\n", ran.display()),
    );
    let global_config = scratch.join("global-gitconfig");
    std::fs::write(
        &global_config,
        format!("[core]\n\thooksPath = {}\n", global_hooks.display()),
    )
    .expect("write the scratch global git config");

    fixture.write("other.txt", "not a summary\n");
    fixture.git(&["add", "other.txt"]);
    let out = fixture
        .hermetic("git", &fixture.repo())
        .env("GIT_CONFIG_GLOBAL", &global_config)
        .args(["commit", "-q", "-m", "chore: a plain commit"])
        .output()
        .expect("run git commit");
    assert!(out.status.success(), "commit: {}", describe(&out));
    assert!(
        ran.is_file(),
        "the global post-commit did not run outside a workspace checkout: {}",
        describe(&out)
    );
}
