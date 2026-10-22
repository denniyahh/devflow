//! End-to-end proof for `devflow gate sweep` (23b, the acting half of bound
//! gate lifetime): an aged, abandoned gate is answered with a rejection a
//! real live poller picks up and resolves to abort — no process is ever
//! signalled.
//!
//! `commands::gate_sweep` is `pub(crate)` inside the `devflow` BINARY crate
//! (no `lib.rs`, so integration tests cannot link against it at all), so —
//! like every other devflow-cli integration test (`phase7_cli.rs`,
//! `release_check.rs`) — this drives the real compiled binary via
//! `Command::new`, not an in-process call.

mod common;

use devflow_core::gates::{GateAction, GateFile, Gates};
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use std::path::Path;
use std::time::{Duration, Instant};

/// Hermetic `git` invocation for fixture setup (999.37) — never a bare
/// `Command::new("git")`.
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

/// A minimal repo with `develop` and a `feature/phase-NN` branch holding one
/// commit, hooks disabled — the same shape `pipeline_launch.rs`'s
/// `code_unknown_does_not_transition_to_validate` test uses to reach an
/// `AgentStatus::Unknown` Code outcome (no exit-code capture file written).
fn init_repo(root: &Path, phase: PhaseId) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    std::fs::write(root.join("README.md"), "base\n").unwrap();
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-q", "-m", "base"]);

    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    git(root, &["checkout", "-q", "-b", &branch]);
    std::fs::write(root.join("work.txt"), "agent work\n").unwrap();
    git(root, &["add", "work.txt"]);
    git(root, &["commit", "-q", "-m", "agent work"]);
}

/// Poll `predicate` until it's true, panicking with `what` if `timeout_secs`
/// elapses first.
fn wait_for(mut predicate: impl FnMut() -> bool, timeout_secs: u64, what: &str) {
    let start = Instant::now();
    while !predicate() {
        assert!(
            start.elapsed() < Duration::from_secs(timeout_secs),
            "timed out after {timeout_secs}s waiting for: {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// `DEVFLOW_E2E_CHILD_TIMEOUT_SECS` (test-only, read here — never by
/// production code): how long this test's patience with the spawned child
/// extends before the bounded wait below panics instead of hanging CI
/// indefinitely. Defaults comfortably above `Gates::poll_response`'s 60s
/// backoff cap; tighten via the env var in CI if 90s is too generous.
fn e2e_child_timeout() -> Duration {
    let secs: u64 = std::env::var("DEVFLOW_E2E_CHILD_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(90);
    Duration::from_secs(secs)
}

/// Wait for `child` to exit, `try_wait`-polling on a short interval rather
/// than blocking indefinitely on `wait()`. On expiry, reaps the child (via
/// a bounded `wait()` — the child's own deliberately short
/// `DEVFLOW_GATE_TIMEOUT_SECS` bounds this, so it is never an unbounded
/// block) so a failing test never leaks a process into the CI runner, then
/// `panic!`s naming the elapsed budget, the child's pid, and what was still
/// true on disk — a diagnosable failure instead of a silent CI hang. Never
/// sends a signal to the child — no process-termination call appears
/// anywhere in this file — the only path off this wait is the child
/// exiting on its own.
fn wait_for_child_exit(
    child: &mut std::process::Child,
    root: &Path,
    phase: PhaseId,
    deadline: Duration,
) -> std::process::ExitStatus {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("try_wait on devflow advance child") {
            return status;
        }
        if start.elapsed() >= deadline {
            let pid = child.id();
            let lock_present = devflow_core::lock::holder(root, phase).is_some();
            let gate_present = Gates::gate_path(root, phase, Stage::Code).exists();
            let response_present = Gates::response_path(root, phase, Stage::Code).exists();
            // Reap before panicking — see doc comment above for why this
            // stays bounded without ever signalling the child.
            let _ = child.wait();
            panic!(
                "devflow advance (pid {pid}) did not exit within {deadline:?}; \
                 on disk: lock_present={lock_present} gate_present={gate_present} \
                 response_present={response_present}"
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Overwrite an already-written gate file's `timestamp` so it reads as
/// `age_secs` old — the deterministic way to make a gate look abandoned
/// without sleeping in the test. Round-trips through the same [`GateFile`]
/// shape `Gates::write_gate` itself writes.
fn backdate_gate(root: &Path, phase: PhaseId, stage: Stage, age_secs: u64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let gate = GateFile {
        phase,
        stage,
        context: "abandoned run".to_string(),
        timestamp: now.saturating_sub(age_secs).to_string(),
    };
    std::fs::write(
        Gates::gate_path(root, phase, stage),
        serde_json::to_string_pretty(&gate).unwrap(),
    )
    .unwrap();
}

/// An age past any plausible `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` default.
///
/// Deliberately a wide margin rather than "just past the threshold": this is
/// an integration test, so it cannot read `config_parse`'s private default,
/// and the previous literal (7h, chosen against a six-hour default) silently
/// stopped reaching the reap path when that default moved to three days. The
/// boundary itself is covered by the unit tests in `commands.rs`; what this
/// test exists to prove is the live-poller seam, which needs only that the
/// gate be unambiguously aged.
const AGED_WELL_PAST_DEFAULT_THRESHOLD_SECS: u64 = 30 * 24 * 60 * 60;

/// The plan's core claim: a gate older than the default threshold is answered
/// with a rejection, and a real `Gates::poll_response` thread — the exact
/// live-poller seam `run_gate_with_timeout` blocks on in production — picks
/// it up and resolves to `GateAction::Abort`.
#[test]
fn sweep_reaps_an_aged_gate_and_a_real_poller_resolves_to_abort() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(91);
    let stage = Stage::Ship;

    // The poller runs in this test process. Record that live process as the
    // gate holder so sweep has evidence that a consumer exists before it
    // writes an abort response.
    let pid = std::process::id();
    let start = devflow_core::agent::process_start_time(pid).unwrap();
    let lock_path = root
        .join(".devflow")
        .join(format!("lock-{}", phase.padded()));
    std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    std::fs::write(lock_path, format!("{pid}\n{start}")).unwrap();

    Gates::write_gate(root, phase, stage, "approve merge?").unwrap();
    backdate_gate(root, phase, stage, AGED_WELL_PAST_DEFAULT_THRESHOLD_SECS);

    std::thread::scope(|scope| {
        let poller = scope.spawn(move || Gates::poll_response(root, phase, stage, 30));

        let output = common::hermetic_devflow(&home)
            .args(["gate", "sweep", "--root"])
            .arg(root)
            .output()
            .expect("run devflow gate sweep");
        assert!(
            output.status.success(),
            "devflow gate sweep failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let response = poller
            .join()
            .expect("poller thread")
            .expect("a live Gates::poll_response must observe the sweep's response within 30s");
        assert!(
            !response.approved,
            "a reap must never write an approval (T-23-41)"
        );
        assert!(
            matches!(GateAction::from_response(&response), GateAction::Abort(_)),
            "an aged gate's reap must resolve to Abort, not a Code loop-back"
        );
    });
}

/// The fail-safe counterpart: a gate younger than the threshold is left
/// completely untouched by an invoked sweep — no response file, no state
/// change.
#[test]
fn sweep_leaves_a_fresh_gate_untouched() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(92);
    let stage = Stage::Validate;

    // Freshly written — `unix_now()`'s age is ~0s, well under the 6h default.
    Gates::write_gate(root, phase, stage, "review gaps").unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["gate", "sweep", "--root"])
        .arg(root)
        .output()
        .expect("run devflow gate sweep");
    assert!(
        output.status.success(),
        "devflow gate sweep failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let open = Gates::list_open(root);
    assert_eq!(
        open.len(),
        1,
        "a fresh gate must remain open after an invoked sweep"
    );
    assert_eq!(open[0].phase, phase);
    assert_eq!(open[0].stage, stage);
    assert!(
        !Gates::response_path(root, phase, stage).exists(),
        "a fresh gate must get no response file at all"
    );
}

/// CLI discoverability: `--max-age-secs` and `--dry-run` must be documented
/// in the subcommand's own `--help`.
#[test]
fn sweep_help_documents_max_age_and_dry_run() {
    let home = common::HermeticHome::new();
    let output = common::hermetic_devflow(&home)
        .args(["gate", "sweep", "--help"])
        .output()
        .expect("run devflow gate sweep --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--max-age-secs"),
        "--help missing --max-age-secs:\n{stdout}"
    );
    assert!(
        stdout.contains("--dry-run"),
        "--help missing --dry-run:\n{stdout}"
    );
}

/// Task 3's strongest proof: a **real, separate `devflow advance` process**
/// parked on a gate is ended by a file write, unwinds through its own
/// `abort()` path, releases its per-phase lock, and leaves an audit record —
/// the orphan class `23-ORPHAN-FORENSICS.md` documented, now remedied
/// without `kill(1)`.
///
/// Setup mirrors `pipeline_launch.rs`'s `code_unknown_does_not_transition_to_validate`
/// test exactly (a Code-stage phase with no exit-code capture file yields
/// `AgentStatus::Unknown`, which `handle_stage_failure`'s never-silent gate
/// answers with a Code gate) — except the target here is spawned as a real
/// OS process via `Command::new`, not driven in-process on a thread.
#[test]
fn sweep_ends_a_real_advance_process_through_its_own_abort_path() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(95);

    init_repo(root, phase);

    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    devflow_core::workflow::save_state(&state).unwrap();

    // The child's OWN gate timeout — deliberately short (comfortably above
    // this setup's own latency, comfortably below this test's outer
    // patience) so that even a totally failed reap still lets
    // `wait_for_child_exit`'s cleanup `wait()` return promptly rather than
    // block for `gate_timeout_secs()`'s multi-day production default.
    // Set per-`Command` via `.env(...)`, not via a process-global env
    // mutation (999.37) — so this cannot race any other test.
    let mut child = common::hermetic_devflow(&home)
        .args(["advance", "--phase", &phase.to_string()])
        .arg(root)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "15")
        .spawn()
        .expect("spawn devflow advance");

    // Wait for the child to acquire the per-phase lock (proves it is
    // genuinely inside `advance()`, holding it across the gate wait) and to
    // write the Code gate (proves it reached `run_gate_with_timeout` and is
    // now polling for a response) — the two on-disk facts the plan's
    // objective names as the "live poller" evidence.
    wait_for(
        || devflow_core::lock::holder(root, phase).is_some(),
        10,
        "the child to acquire .devflow/lock-95",
    );
    let gate_path = Gates::gate_path(root, phase, Stage::Code);
    wait_for(
        || gate_path.exists(),
        10,
        "the child to write the Code gate",
    );
    assert!(
        devflow_core::lock::holder(root, phase).is_some(),
        "lock must still be held once the gate is written — this is the \
         proof that a live poller is genuinely blocking on it"
    );

    // The reap itself: the same `Gates::reap` primitive Task 1/2's tests
    // exercise against a thread-based live poller. This test's job is to
    // prove the OTHER end of that same write — a real, separate process
    // consuming it.
    Gates::reap(
        root,
        phase,
        Stage::Code,
        "abort: reaped by devflow gate sweep (unattended gate exceeded max age)",
        "devflow-reap",
    )
    .expect("reap the aged Code gate");

    let status = wait_for_child_exit(&mut child, root, phase, e2e_child_timeout());

    assert!(
        status.success(),
        "devflow advance must exit cleanly once its own abort() path runs, got {status:?}"
    );
    assert!(
        devflow_core::lock::holder(root, phase).is_none(),
        "LockGuard's Drop must have released the per-phase lock — proof the \
         process unwound cleanly through its own code, not by being \
         terminated from outside"
    );

    let events = std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap();
    assert!(
        events
            .lines()
            .any(|line| line.contains("\"workflow_aborted\"")),
        "the target process must have run its own abort() path, recording \
         workflow_aborted — the whole claim this plan rests on\nevents:\n{events}"
    );

    // The negative that separates this plan from a reaper that terminates
    // processes: neither the sweep's write path (`Gates::reap`, exercised
    // above) nor this test itself ever sent the child a signal. Checked
    // mechanically by this file's own acceptance grep for signalling call
    // forms (must be zero), not asserted at runtime — there is no API
    // surface here that could send one.
}

// ---------------------------------------------------------------------------
// 50-03 (999.141, T-50-05): the machine-wide stray pass stays dry-run in every
// suite on the hermetic builder.
//
// `gate sweep --reap-strays` scans the whole machine's `/proc` and reaps every
// DevFlow-shaped process the caller owns, sparing only the ones the registry
// knows about. On `common::hermetic_devflow` the registry holds nothing but
// the test's own roots, so a real (non-dry-run) pass from a test would treat
// every live run on the developer's machine as a stray. This scan refuses one.
// ---------------------------------------------------------------------------

/// The `"--reap-strays"` argument literal, quotes included. Spelled through
/// `concat!` split inside the token so this file's own source never holds it.
const REAP_STRAYS_LITERAL: &str = concat!("\"--reap-", "strays\"");

/// The `"--dry-run"` argument literal, quotes included, spelled the same way.
const DRY_RUN_LITERAL: &str = concat!("\"--dry-", "run\"");

/// `src` with every `//` comment (line, doc and trailing) removed. A `//`
/// inside a string, raw string or char literal is not a comment. A local copy
/// of `test_home_isolation.rs`'s scanner: integration test binaries cannot
/// share private helpers, and `common/mod.rs` holds no scan code.
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

/// The `;`-separated statements of comment-stripped `code` that pass
/// `"--reap-strays"` without `"--dry-run"`, trimmed. A test that adds
/// `--dry-run` in a separate statement is flagged too: the conservative
/// direction.
fn real_reap_strays_statements(code: &str) -> Vec<String> {
    code.split(';')
        .filter(|statement| {
            statement.contains(REAP_STRAYS_LITERAL) && !statement.contains(DRY_RUN_LITERAL)
        })
        .map(|statement| statement.trim().to_string())
        .collect()
}

#[test]
fn no_suite_on_the_common_builder_runs_a_real_reap_strays_pass() {
    // (1) Planted controls: a real pass is flagged, a dry-run pass and a
    // commented-out real pass are not.
    let planted = concat!(
        "let out = common::hermetic_devflow(&home).args([\"gate\", \"sweep\", \"--reap-",
        "strays\"]).output();"
    );
    assert_eq!(
        real_reap_strays_statements(&code_only(planted)).len(),
        1,
        "a planted real --reap-strays pass must be flagged"
    );
    let planted_dry_run = concat!(
        "let out = common::hermetic_devflow(&home).args([\"gate\", \"sweep\", \"--reap-",
        "strays\", \"--dry-",
        "run\"]).output();"
    );
    assert_eq!(
        real_reap_strays_statements(&code_only(planted_dry_run)),
        Vec::<String>::new(),
        "a planted --reap-strays pass with --dry-run must pass"
    );
    let commented = format!("// {planted}\n");
    assert_eq!(
        real_reap_strays_statements(&code_only(&commented)),
        Vec::<String>::new(),
        "a commented-out real --reap-strays pass must pass"
    );

    // (2) Every suite directly under `tests/` that declares `mod common;`;
    // this file always does.
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut paths: Vec<_> = std::fs::read_dir(&tests_dir)
        .unwrap_or_else(|err| panic!("read {}: {err}", tests_dir.display()))
        .map(|entry| entry.expect("read tests dir entry").path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    paths.sort();
    let mut scanned = 0;
    let mut offenders = Vec::new();
    for path in paths {
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        let code = code_only(&source);
        if !code.lines().any(|line| line.trim() == "mod common;") {
            continue;
        }
        scanned += 1;
        for statement in real_reap_strays_statements(&code) {
            offenders.push(format!("{}: {statement}", path.display()));
        }
    }
    assert!(
        scanned > 0,
        "no suite under {} declares `mod common;`, so the scan checked nothing",
        tests_dir.display()
    );
    assert!(
        offenders.is_empty(),
        "a suite on the hermetic builder runs a real (non-dry-run) --reap-strays pass, \
         whose safety set would shrink to the test's own roots:\n{}",
        offenders.join("\n")
    );
}
