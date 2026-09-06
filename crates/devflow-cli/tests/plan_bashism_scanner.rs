//! Behavioural tests for `scripts/lint-plan-bashisms.sh`.
//!
//! GSD executor subagents run their Bash tool under **zsh**, where `PIPESTATUS`
//! does not exist — `${PIPESTATUS[0]}` expands to the empty string and every
//! assertion about that exit code passes vacuously. The scanner refuses to let a
//! plan commit with that shape unless the command is genuinely wrapped so it runs
//! under bash.
//!
//! These tests **execute the scanner**. That is a deliberate departure from
//! `pre_commit_branch_guard.rs`, which asserts on the hook's *source* in the same
//! idiom as `pre_push_signing_policy.rs` and `ci_parity_guards.rs`. That idiom is
//! correct for statically-checkable properties and wrong here: review finding C-04
//! is entirely about behaviour that source inspection missed for the life of the
//! hook. Two bypasses were proven by *committing them* through the live hook —
//! a multi-line block, and a single-line block ending `# bash -c`. Both are
//! fixtures below, and both must fail.
//!
//! The allow rule under test is **containment, not ordering**: a bashism is
//! permitted only when it lies inside a single-quoted region that some `bash -c '`
//! opened. `sequenced-bypass` and `double-quoted-bypass` exist because an ordering
//! rule ("a `bash -c` appears before the bashism") accepts both of them while the
//! bashism still expands in the outer shell.
//!
//! `legit-PLAN.md` and `no-bashism-PLAN.md` are the load-bearing negative controls.
//! Without them a scanner that rejected every input would satisfy every other test
//! in this file.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

fn scanner() -> PathBuf {
    repo_root().join("scripts/lint-plan-bashisms.sh")
}

fn fixture(name: &str) -> PathBuf {
    repo_root()
        .join("crates/devflow-cli/tests/fixtures/plan-bashisms")
        .join(name)
}

/// Spawn the scanner with the given arguments. A spawn failure is reported with
/// the script path so the RED state of this suite is attributable to the missing
/// implementation and to nothing else.
fn run(args: &[&Path]) -> Output {
    let script = scanner();
    Command::new(&script)
        .args(args)
        .current_dir(repo_root())
        .output()
        .unwrap_or_else(|err| {
            panic!(
                "failed to spawn {}: {err}. The scanner script must exist and be executable.",
                script.display()
            )
        })
}

fn combined(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The scanner must reject `name`, and its message must name both the file and the
/// offending text. A scanner that exits correctly while explaining nothing leaves
/// the author with no way to find what tripped it, so the message is part of the
/// contract rather than a nicety.
fn assert_blocked(name: &str, expected_text: &str) {
    let path = fixture(name);
    let out = run(&[path.as_path()]);
    let text = combined(&out);
    assert!(
        !out.status.success(),
        "{name} must be REFUSED by the scanner, but it exited 0.\n--- output ---\n{text}"
    );
    // Assert on a SINGLE line carrying both the file name and the offending
    // text, not on the whole output. The scanner's guidance boilerplate itself
    // mentions `${PIPESTATUS[0]}`, so a whole-output `contains` would be
    // satisfied by the boilerplate for four of these fixtures without the
    // scanner having reported anything about the file at all.
    let hit = text
        .lines()
        .find(|line| line.contains(name) && line.contains(expected_text));
    assert!(
        hit.is_some(),
        "the refusal for {name} must print one line naming BOTH the file and the \
         offending text {expected_text:?}; a hit reported only in the generic \
         guidance text is not a report about this file.\n--- output ---\n{text}"
    );
}

/// The scanner must accept `name`. These are the negative controls: without them,
/// a scanner that refused every input would satisfy every `assert_blocked` case.
fn assert_accepted(name: &str) {
    let path = fixture(name);
    let out = run(&[path.as_path()]);
    let text = combined(&out);
    assert!(
        out.status.success(),
        "{name} is a legitimate plan and must be ACCEPTED, but the scanner exited \
         {:?}.\n--- output ---\n{text}",
        out.status.code()
    );
}

/// codex's first proven bypass: the bashism sits on its own line inside the block,
/// so a line-based scanner anchored to the opening tag never sees it. This exact
/// content was committed through the live hook.
#[test]
fn multiline_block_is_blocked() {
    assert_blocked("multiline-bypass-PLAN.md", "${PIPESTATUS[0]}");
}

/// codex's second proven bypass: a trailing `# bash -c` comment satisfies a
/// substring filter without wrapping anything. Also committed through the live hook.
#[test]
fn trailing_bash_c_comment_is_blocked() {
    assert_blocked("comment-bypass-PLAN.md", "${PIPESTATUS[0]}");
}

/// A real `bash -c ':'` precedes the bashism and its quoted region has already
/// closed; the bashism runs in the outer shell. Ordering accepts this, containment
/// does not.
#[test]
fn bash_c_that_closed_before_the_bashism_is_blocked() {
    assert_blocked("sequenced-bypass-PLAN.md", "${PIPESTATUS[0]}");
}

/// `bash -c "…"` opens no single-quoted region at all, so the outer shell expands
/// the bashism before bash is started. Ordering accepts this, containment does not.
#[test]
fn double_quoted_bash_c_is_blocked() {
    assert_blocked("double-quoted-bypass-PLAN.md", "${PIPESTATUS[0]}");
}

/// zsh indexes arrays from 1, so any `${name[N]}` yields the wrong element rather
/// than erroring — the same silent-wrong-answer class as PIPESTATUS.
#[test]
fn generic_array_index_is_blocked() {
    assert_blocked("array-index-PLAN.md", "${myarr[0]}");
}

/// zsh populates `$match`, not `$BASH_REMATCH`.
#[test]
fn bash_rematch_is_blocked() {
    assert_blocked("bash-rematch-PLAN.md", "${BASH_REMATCH[1]}");
}

/// An opening tag with no closing tag is a bypass by construction: a parser that
/// only matches complete blocks sees nothing to check. It must be reported as
/// malformed, not skipped.
#[test]
fn unterminated_block_is_reported_as_malformed() {
    assert_blocked("unterminated-PLAN.md", "unterminated <automated> block");
}

/// The load-bearing negative control. The bashism is inside the region that
/// `bash -c '` opened, so it genuinely runs under bash.
#[test]
fn legitimate_bash_c_wrapped_block_is_accepted() {
    assert_accepted("legit-PLAN.md");
}

/// The second negative control: the scanner must not react to the mere presence
/// of a verify block.
#[test]
fn block_with_no_bash_only_syntax_is_accepted() {
    assert_accepted("no-bashism-PLAN.md");
}

/// A named path that cannot be read is an ERROR, not a skip. A silent skip is the
/// same false-green class as the bypasses: a check reporting success having
/// inspected nothing.
#[test]
fn missing_path_is_an_error_naming_the_path() {
    let missing =
        repo_root().join("crates/devflow-cli/tests/fixtures/plan-bashisms/no-such-PLAN.md");
    let out = run(&[missing.as_path()]);
    let text = combined(&out);
    assert!(
        !out.status.success(),
        "a missing path must be an error, not a silent skip.\n--- output ---\n{text}"
    );
    assert!(
        text.contains("no-such-PLAN.md"),
        "the error must name the unreadable path.\n--- output ---\n{text}"
    );
}

/// Zero file arguments is the one legitimate empty case, and it must be VISIBLE in
/// the output rather than inferred from a bare exit 0 — an exit 0 alone is
/// indistinguishable from a scan that silently inspected nothing.
#[test]
fn zero_arguments_exits_zero_and_prints_a_zero_count() {
    let out = run(&[]);
    let text = combined(&out);
    assert!(
        out.status.success(),
        "an empty scan is legitimate and must exit 0.\n--- output ---\n{text}"
    );
    assert!(
        text.contains("scanned 0 file"),
        "an empty scan must print its scanned-file count so the emptiness is \
         visible.\n--- output ---\n{text}"
    );
}
