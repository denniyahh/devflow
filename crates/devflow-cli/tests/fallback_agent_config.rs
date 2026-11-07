//! R-3 (Phase 51): CLI-boundary coverage of `devflow start`'s combined
//! resolution of the `--fallback-agent` flag and a `fallback_agent` key in
//! `devflow.toml`, mirroring `yes_ship_config.rs`. The flag wins when given;
//! the key applies when the flag is absent, and a key-only fallback names
//! `devflow.toml` as its source.
//!
//! Most cases run through `--dry-run`, which returns before the run is
//! persisted, so `config_key_alone_is_persisted_in_the_run_state` is the one
//! real launch: it reads what `start` saved.
//!
//! Every `devflow` child is built by `common::hermetic_devflow`, with its
//! `PATH` set on the child command. A child that must not launch an agent gets
//! a `PATH` holding only the stubs it needs plus `git` and `sh`, so a lost
//! refusal ends at a missing binary instead of reaching a real agent.

mod common;

use devflow_core::phase_id::PhaseId;
use std::fs;
use std::path::Path;
use std::process::Output;

/// A fallback agent stub: `validate_fallback_agent` needs its binary on `PATH`
/// and its driver's health check to pass, which answers without running it.
const AGENT_STUB: &str = "exit 0";

/// Copied from `supervisor_e2e.rs`: a fake `claude` that answers `--version`,
/// the delivery canary's planted token, and otherwise waits until it is ended.
const WAITING_CLAUDE: &str = r#"if [ "$1" = "--version" ]; then echo "2.1.220 (Claude Code)"; exit 0; fi
read -r turn
case "$turn" in
  *DEVFLOW_DELIVERY_CANARY_*)
    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
    printf '{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"%s"}\n' "$token"
    exit 0
    ;;
esac
sleep 120"#;

/// The line `start` prints when it arms a fallback (51-04's notice).
fn fallback_line(fallback: &str) -> String {
    format!("fallback agent: {fallback} takes over the Code stage once if claude is rate-limited")
}

fn git(root: &Path, args: &[&str]) {
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Copied from `supervisor_e2e.rs`: a repository on `develop` with a roadmap
/// entry for `phase`.
fn init_repo(root: &Path, phase: PhaseId) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    fs::create_dir_all(root.join(".planning")).unwrap();
    fs::write(root.join("README.md"), "base\n").unwrap();
    fs::write(
        root.join(".planning/ROADMAP.md"),
        format!("# Roadmap\n\n### Phase {phase}: fallback config fixture\n"),
    )
    .unwrap();
    git(root, &["add", "README.md", ".planning/ROADMAP.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
}

/// A stub directory holding `stubs` (program, body) plus `git` and `sh`.
fn stub_dir(stubs: &[(&str, &str)]) -> common::FakeBin {
    let (first, rest) = stubs.split_first().expect("at least one stub");
    let fake = common::fake_bin_dir_for(first.0, first.1);
    for (program, body) in rest {
        let path = fake.path.join(program);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fake
}

/// `devflow start --phase <phase> --agent claude --mode supervise
/// --no-worktree <extra>` with `path` as the child's whole `PATH`.
fn run_start(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    path: &str,
    extra: &[&str],
) -> Output {
    common::hermetic_devflow(home)
        .args([
            "start",
            "--phase",
            &phase.to_string(),
            "--agent",
            "claude",
            "--mode",
            "supervise",
            "--no-worktree",
        ])
        .args(extra)
        .arg(root)
        .current_dir(root)
        .env("PATH", path)
        .output()
        .expect("run devflow start")
}

/// A `--dry-run` start whose `PATH` is only `fake`; returns the output.
fn dry_run(root: &Path, phase: PhaseId, fake: &common::FakeBin, extra: &[&str]) -> Output {
    // `--dry-run` launches nothing, so the home only has to outlive `output()`.
    let home = common::HermeticHome::new();
    let mut args = extra.to_vec();
    args.push("--dry-run");
    run_start(&home, root, phase, &fake.path.display().to_string(), &args)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_success(output: &Output, what: &str) {
    assert!(
        output.status.success(),
        "{what} must succeed\nstdout: {}\nstderr: {}",
        stdout(output),
        stderr(output)
    );
}

/// The key alone arms the fallback, and stdout names `devflow.toml` as its
/// source: a standing default is never a silent one.
#[test]
fn config_key_alone_arms_the_fallback_and_names_its_source() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(5151);
    init_repo(root, phase);
    fs::write(root.join("devflow.toml"), "fallback_agent = \"codex\"\n").unwrap();
    let fake = stub_dir(&[("codex", AGENT_STUB)]);

    let output = dry_run(root, phase, &fake, &[]);

    assert_success(&output, "a key-only dry run");
    let printed = stdout(&output);
    assert!(
        printed.contains(&fallback_line("codex")),
        "the key arms codex as the fallback\nstdout: {printed}"
    );
    assert!(
        printed.contains("note: fallback agent codex set by devflow.toml (fallback_agent)"),
        "a key-only fallback names devflow.toml as its source\nstdout: {printed}"
    );
}

/// A typed `--fallback-agent` wins over the key, and stdout does not claim
/// `devflow.toml` as the source.
#[test]
fn fallback_flag_wins_over_the_config_key() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(5152);
    init_repo(root, phase);
    fs::write(root.join("devflow.toml"), "fallback_agent = \"codex\"\n").unwrap();
    let fake = stub_dir(&[("codex", AGENT_STUB), ("agy", AGENT_STUB)]);

    let output = dry_run(root, phase, &fake, &["--fallback-agent", "antigravity"]);

    assert_success(&output, "a flag-and-key dry run");
    let printed = stdout(&output);
    assert!(
        printed.contains(&fallback_line("antigravity")),
        "the flag's agent is the fallback\nstdout: {printed}"
    );
    assert!(
        !printed.contains(&fallback_line("codex")),
        "the key's agent is not armed when the flag is given\nstdout: {printed}"
    );
    assert!(
        !printed.contains("devflow.toml"),
        "a flag-supplied fallback does not claim devflow.toml as its source\nstdout: {printed}"
    );
}

/// No flag and no key: no fallback is armed and no fallback line is printed.
#[test]
fn no_flag_and_no_key_arms_no_fallback() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(5153);
    init_repo(root, phase);
    let fake = stub_dir(&[("codex", AGENT_STUB)]);

    let output = dry_run(root, phase, &fake, &[]);

    assert_success(&output, "a dry run with no fallback");
    let printed = stdout(&output);
    assert!(
        !printed.contains("fallback agent"),
        "nothing arms a fallback\nstdout: {printed}"
    );
}

/// R2-5: the key reaches the persisted run, not only the dry-run printout. A
/// real key-only `start` saves `fallback_agent: "codex"`, unused.
#[test]
fn config_key_alone_is_persisted_in_the_run_state() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().canonicalize().unwrap();
    let phase = PhaseId::new(5157);
    init_repo(&root, phase);
    fs::write(root.join("devflow.toml"), "fallback_agent = \"codex\"\n").unwrap();
    let fake = stub_dir(&[("claude", WAITING_CLAUDE), ("codex", AGENT_STUB)]);
    let _reaper = common::LaunchedRunReaper::new(&root, phase, vec![]);
    let state_path = devflow_core::workflow::state_path(&root, phase);
    assert!(!state_path.exists(), "no state may exist before start");

    // The waiting stub needs `grep`, `head` and `sleep`, so the host PATH
    // follows the stubs, as in `supervisor_e2e.rs`.
    let host_path = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake.path.display(), host_path.to_string_lossy());
    let output = run_start(&home, &root, phase, &path, &["--until", "define"]);

    assert_success(&output, "a key-only start");
    assert!(
        stdout(&output).contains("note: fallback agent codex set by devflow.toml (fallback_agent)"),
        "the real start names the key as the source\nstdout: {}",
        stdout(&output)
    );
    let saved: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&fs::read_to_string(&state_path).expect("start saved the state"))
            .expect("the state is a JSON object");
    // Indexed on the map, which panics on an absent key, so a missing field
    // fails here instead of reading as null.
    assert_eq!(
        saved["fallback_agent"], "codex",
        "the saved run carries the key's fallback: {saved:?}"
    );
    assert_eq!(
        saved["fallback_consumed"], false,
        "a new fallback is unused: {saved:?}"
    );
}
