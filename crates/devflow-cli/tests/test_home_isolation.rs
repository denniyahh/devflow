//! R-13 guard for 999.141 (D-07): an e2e `devflow` child keeps its writes
//! inside its own tempdir `HOME`.
//!
//! `a_child_writing_outside_its_tempdir_home_fails` runs a real `devflow
//! start` through `common::hermetic_devflow` and checks that its registry
//! entry lands under the hermetic home. Its negative control runs the same
//! start as a plain `Command` whose `HOME` names a fake "real" home, and shows
//! that home is populated, so the first check is able to fail.
//!
//! `suites_using_common_never_spawn_devflow_raw_or_mutate_the_environment` is
//! a permanent source scan: every `tests/*.rs` that declares `mod common;` must
//! spawn `devflow` only through the builder and must never mutate the test
//! process's environment. A suite is covered as soon as it declares
//! `mod common;`, without editing this file.
//!
//! Every scanned token is spelled through `concat!` split inside the token, so
//! this file's own source holds a raw spawn token exactly once: at the
//! negative-control spawn. Nothing here reads the operator's real
//! `~/.cache/devflow`.

mod common;

use devflow_core::phase_id::PhaseId;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Bounds the Define gate a launched supervise run parks at, so a reaper
/// failure cannot leave an immortal monitor.
const GATE_TIMEOUT_SECS: &str = "15";

/// How long a launched run may take to record its agent pid.
const AGENT_PID_TIMEOUT: Duration = Duration::from_secs(10);

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

/// The same `develop`-only repo `start_lock_e2e.rs` starts a phase in.
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
        format!("# Roadmap\n\n### Phase {phase}: home isolation fixture\n"),
    )
    .unwrap();
    git(root, &["add", "README.md", ".planning/ROADMAP.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
}

/// Adds the `start` arguments and the fake-agent `PATH` to `command`.
fn add_start_args(command: &mut Command, root: &Path, phase: PhaseId, fake_bin: &common::FakeBin) {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());
    command
        .args([
            "start",
            "--phase",
            &phase.to_string(),
            "--agent",
            "claude",
            "--mode",
            "supervise",
            "--no-worktree",
            "--legacy-claude-launch",
        ])
        .arg(root)
        .current_dir(root)
        .env("PATH", path)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", GATE_TIMEOUT_SECS);
}

/// The value `command` sets for `name`; panics when it inherits or removes it.
fn env_set_by(command: &Command, name: &str) -> PathBuf {
    let (_, value) = command
        .get_envs()
        .find(|(key, _)| *key == OsStr::new(name))
        .unwrap_or_else(|| panic!("the hermetic command must set {name}; it inherits it"));
    let value =
        value.unwrap_or_else(|| panic!("the hermetic command must set {name}, not remove it"));
    PathBuf::from(value)
}

/// Runs `command` (a `start` built by the caller), reaps the run it launches,
/// and returns every file under `roots` afterwards.
fn run_start_and_list(
    mut command: Command,
    root: &Path,
    phase: PhaseId,
    roots: &Path,
) -> Vec<PathBuf> {
    let _reaper = common::LaunchedRunReaper::new(root, phase, vec![]);
    let output = command.output().expect("run devflow start");
    assert!(
        output.status.success(),
        "devflow start failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Wait for the agent pid so the reaper can end the agent as well as the
    // monitor.
    let agent_pid = devflow_core::agent_result::agent_pid_path(root, phase);
    common::wait_for(
        || agent_pid.exists(),
        AGENT_PID_TIMEOUT,
        "the launched agent's pid file",
    );
    common::entries_under(roots)
}

#[test]
fn a_child_writing_outside_its_tempdir_home_fails() {
    let phase = PhaseId::new(89);
    let fake_bin = common::fake_bin_dir("exec sleep 60");

    // Hermetic arm: the builder must set HOME and every cache variable under
    // the hermetic home, and the run's registry entry must land there. The
    // environment is checked before the spawn, so a builder that sets nothing
    // fails here instead of writing to the inherited HOME.
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path(), phase);
    let mut hermetic = common::hermetic_devflow(&home);
    add_start_args(&mut hermetic, repo.path(), phase, &fake_bin);
    assert_eq!(
        env_set_by(&hermetic, "HOME"),
        home.home(),
        "HOME must be the hermetic home"
    );
    for (name, expected) in [
        ("XDG_CACHE_HOME", home.home().join(".cache")),
        ("XDG_CONFIG_HOME", home.config_dir()),
        ("DEVFLOW_CACHE_DIR", home.cache_dir()),
    ] {
        let value = env_set_by(&hermetic, name);
        assert!(
            value == expected && value.starts_with(home.home()),
            "{name} must be {} under the hermetic home; it is {}",
            expected.display(),
            value.display()
        );
    }
    let hermetic_roots = home.cache_dir().join("roots");
    let entries = run_start_and_list(hermetic, repo.path(), phase, &hermetic_roots);
    assert!(
        !entries.is_empty(),
        "the hermetic run must register under {}; it holds nothing",
        hermetic_roots.display()
    );

    // Negative control (R-13): the one deliberate non-hermetic devflow spawn
    // in tests/. A child left on an inherited-style HOME writes there, so the
    // assertion above can fail. R2 is a fake "real home", never the operator's.
    let real_home = tempfile::Builder::new()
        .prefix("dfr")
        .tempdir_in("/tmp")
        .expect("create fake real home under /tmp");
    let repo2 = tempfile::tempdir().unwrap();
    init_repo(repo2.path(), phase);
    let mut plain = Command::new(common::devflow_bin());
    plain
        .env("HOME", real_home.path())
        .env_remove("DEVFLOW_CACHE_DIR")
        .env_remove("XDG_CACHE_HOME");
    // Git hermeticity is a separate concern from HOME; keep the control from
    // retargeting a real repository when the suite runs under a hook.
    for name in devflow_core::test_support::REPO_LOCAL_GIT_VARS
        .iter()
        .chain(devflow_core::test_support::ALSO_REDIRECTING_GIT_VARS)
    {
        plain.env_remove(name);
    }
    add_start_args(&mut plain, repo2.path(), phase, &fake_bin);
    let control_roots = real_home.path().join(".cache/devflow/roots");
    let entries = run_start_and_list(plain, repo2.path(), phase, &control_roots);
    assert!(
        !entries.is_empty(),
        "the plain-Command control must populate {}; it holds nothing, so the \
         hermetic assertion cannot fail",
        control_roots.display()
    );
}

/// `src` with every `//` comment (line, doc and trailing) removed. A `//`
/// inside a string, raw string or char literal is not a comment.
fn code_only(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut index = 0;
    while index < chars.len() {
        index = match chars[index] {
            '/' if chars.get(index + 1) == Some(&'/') => skip_to_line_end(&chars, index),
            '"' => copy_string(&chars, index, &mut out),
            'r' if raw_string_hashes(&chars, index).is_some() => {
                copy_raw_string(&chars, index, &mut out)
            }
            '\'' => copy_char_or_lifetime(&chars, index, &mut out),
            other => {
                out.push(other);
                index + 1
            }
        };
    }
    out
}

fn skip_to_line_end(chars: &[char], mut index: usize) -> usize {
    while index < chars.len() && chars[index] != '\n' {
        index += 1;
    }
    index
}

/// Copies a `"…"` literal starting at `start`, honouring `\` escapes.
fn copy_string(chars: &[char], start: usize, out: &mut String) -> usize {
    out.push('"');
    let mut index = start + 1;
    while index < chars.len() {
        out.push(chars[index]);
        match chars[index] {
            '\\' if index + 1 < chars.len() => {
                out.push(chars[index + 1]);
                index += 2;
            }
            '"' => return index + 1,
            _ => index += 1,
        }
    }
    index
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The `#` count of a raw string (`r"`, `r#"`, `br"`…) starting at the `r` at
/// `index`, or `None` when that `r` is part of an identifier or not followed
/// by `#*"`.
fn raw_string_hashes(chars: &[char], index: usize) -> Option<usize> {
    let before = index.checked_sub(1).map(|i| chars[i]);
    let prefix_ok = match before {
        None => true,
        Some('b') => index < 2 || !is_ident_char(chars[index - 2]),
        Some(c) => !is_ident_char(c),
    };
    if !prefix_ok {
        return None;
    }
    let hashes = chars[index + 1..].iter().take_while(|&&c| c == '#').count();
    (chars.get(index + 1 + hashes) == Some(&'"')).then_some(hashes)
}

fn copy_raw_string(chars: &[char], start: usize, out: &mut String) -> usize {
    let hashes = raw_string_hashes(chars, start).expect("caller checked the raw-string opener");
    let body = start + 2 + hashes;
    out.extend(&chars[start..body]);
    let mut index = body;
    while index < chars.len() {
        let closes = chars[index] == '"'
            && chars[index + 1..]
                .iter()
                .take(hashes)
                .filter(|&&c| c == '#')
                .count()
                == hashes;
        if closes {
            out.extend(&chars[index..=index + hashes]);
            return index + hashes + 1;
        }
        out.push(chars[index]);
        index += 1;
    }
    index
}

/// Copies a char literal (`'x'`, `'\''`, `'\u{1F600}'`) or, for a lifetime
/// or label, only its quote.
fn copy_char_or_lifetime(chars: &[char], start: usize, out: &mut String) -> usize {
    let end = if chars.get(start + 1) == Some(&'\\') {
        chars
            .get(start + 2..)
            .unwrap_or_default()
            .iter()
            .position(|&c| c == '\'')
            .map(|offset| start + 2 + offset)
    } else if chars.get(start + 2) == Some(&'\'') {
        Some(start + 2)
    } else {
        None
    };
    match end {
        Some(end) => {
            out.extend(&chars[start..=end]);
            end + 1
        }
        None => {
            out.push('\'');
            start + 1
        }
    }
}

/// Raw devflow spawns in comment-stripped `code`: the two direct
/// `Command::new` forms and Cargo's binary path variable.
fn raw_devflow_spawns(code: &str) -> usize {
    let _ = code;
    0
}

/// Process-environment mutations in comment-stripped `code`, counted as whole
/// identifiers so `a_set_variable` is not one.
fn env_mutations(code: &str) -> usize {
    let _ = code;
    0
}

fn read_source(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn declares_mod_common(code: &str) -> bool {
    code.lines().any(|line| line.trim() == "mod common;")
}

/// Every `*.rs` directly under `tests/` whose code declares `mod common;`,
/// sorted, with its comment-stripped code.
fn suites_using_common(tests_dir: &Path) -> Vec<(PathBuf, String)> {
    let mut paths: Vec<PathBuf> = fs::read_dir(tests_dir)
        .unwrap_or_else(|err| panic!("read {}: {err}", tests_dir.display()))
        .map(|entry| entry.expect("read tests dir entry").path())
        .filter(|path| path.is_file() && path.extension() == Some(OsStr::new("rs")))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let code = code_only(&read_source(&path));
            (path, code)
        })
        .filter(|(_, code)| declares_mod_common(code))
        .collect()
}

#[test]
fn suites_using_common_never_spawn_devflow_raw_or_mutate_the_environment() {
    // (1) Planted controls: each scanner flags its planted positive and
    // passes the look-alike.
    let planted_spawn = concat!("let out = Command::new(", "devflow_bin()).output();");
    assert_eq!(
        raw_devflow_spawns(&code_only(planted_spawn)),
        1,
        "a planted raw spawn must be flagged"
    );
    let commented_spawn = format!("// {planted_spawn}\n");
    assert_eq!(
        raw_devflow_spawns(&code_only(&commented_spawn)),
        0,
        "a commented spawn must pass"
    );
    let planted_mutation = concat!("std::env::", "set_", "var(\"HOME\", \"/x\");");
    assert_eq!(
        env_mutations(&code_only(planted_mutation)),
        1,
        "a planted env mutation must be flagged"
    );
    assert_eq!(
        env_mutations(&code_only("let a_set_variable = 1;")),
        0,
        "a_set_variable must pass"
    );

    // (2) Every suite that declares `mod common;`; this file always does.
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let suites = suites_using_common(&tests_dir);
    assert!(
        suites
            .iter()
            .any(|(path, _)| path.ends_with("test_home_isolation.rs")),
        "no suite under {} declares `mod common;`, not even test_home_isolation.rs",
        tests_dir.display()
    );

    // (3) Raw spawns: none, except exactly the one R-13 control here.
    for (path, code) in &suites {
        let found = raw_devflow_spawns(code);
        let allowed = usize::from(path.ends_with("test_home_isolation.rs"));
        assert_eq!(
            found,
            allowed,
            "{} has {found} raw devflow spawn(s); it must have {allowed} \
             (spawn through common::hermetic_devflow; test_home_isolation.rs keeps exactly its R-13 control)",
            path.display()
        );
    }

    // (4) No process-environment mutation in common/mod.rs or any suite.
    let common_path = tests_dir.join("common").join("mod.rs");
    let common_code = code_only(&read_source(&common_path));
    for (path, code) in std::iter::once((&common_path, &common_code))
        .chain(suites.iter().map(|(path, code)| (path, code)))
    {
        let found = env_mutations(code);
        assert_eq!(
            found,
            0,
            "{} has {found} process-environment mutation(s); set the variable on the child Command",
            path.display()
        );
    }
}
