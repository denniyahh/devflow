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

use std::fs;
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

/// Run the production scanner against the actual index of an owned disposable
/// repository. Staged-mode regressions must not use the worktree that owns this
/// test binary: their entire point is that the index can intentionally differ
/// from the filesystem.
fn run_staged(repo: &Path) -> Output {
    let script = scanner();
    Command::new(&script)
        .arg("--staged")
        .current_dir(repo)
        .output()
        .unwrap_or_else(|err| {
            panic!(
                "failed to spawn {} from {}: {err}",
                script.display(),
                repo.display()
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

/// Every Git subprocess in a disposable repository must use the project helper.
/// Its per-command environment scrubbing prevents a test launched from a linked
/// worktree from accidentally following an inherited `GIT_DIR` back to the
/// developer checkout.
fn git_ok(repo: &Path, args: &[&str]) -> Output {
    let output = devflow_core::test_support::git_command(repo)
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("failed to spawn git {args:?} in {}: {err}", repo.display()));
    assert!(
        output.status.success(),
        "git {args:?} failed in {} with status {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        repo.display(),
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

fn disposable_repo() -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("create disposable repository");
    let root = repo.path();
    git_ok(root, &["init", "-q"]);
    git_ok(root, &["config", "user.name", "DevFlow scanner test"]);
    git_ok(
        root,
        &["config", "user.email", "scanner-test@example.invalid"],
    );
    // Fixtures need baseline commits for rename/deletion classification. Disable
    // any developer-level signing policy inside the disposable repository so a
    // test never asks for an interactive key passphrase.
    git_ok(root, &["config", "commit.gpgSign", "false"]);
    repo
}

fn fixture_text(name: &str) -> String {
    fs::read_to_string(fixture(name)).unwrap_or_else(|err| panic!("read fixture {name}: {err}"))
}

fn write_plan(repo: &Path, path: &str, contents: &str) {
    let destination = repo.join(path);
    fs::create_dir_all(destination.parent().expect("plan has a parent directory"))
        .unwrap_or_else(|err| panic!("create plan directory for {path}: {err}"));
    fs::write(&destination, contents)
        .unwrap_or_else(|err| panic!("write disposable plan {}: {err}", destination.display()));
}

fn index_blob(repo: &Path, path: &str) -> Vec<u8> {
    let spec = format!(":{path}");
    git_ok(repo, &["show", &spec]).stdout
}

fn commit_plan(repo: &Path, path: &str, contents: &str) {
    write_plan(repo, path, contents);
    git_ok(repo, &["add", path]);
    git_ok(repo, &["commit", "-q", "-m", "seed plan"]);
}

fn assert_staged_refusal(out: &Output, path: &str) {
    let text = combined(out);
    assert!(
        !out.status.success(),
        "the staged scanner must refuse {path}, but it exited 0.\n--- output ---\n{text}"
    );
    assert!(
        text.contains("scanned 1 file(s)"),
        "the refusal must report that it inspected the selected staged plan.\n--- output ---\n{text}"
    );
    assert!(
        text.lines()
            .any(|line| line.contains(path) && line.contains("${PIPESTATUS[0]}")),
        "the refusal must name both {path} and the staged violating expression on one line.\n--- output ---\n{text}"
    );
}

fn assert_staged_zero_scan(out: &Output) {
    let text = combined(out);
    assert!(
        out.status.success(),
        "the staged scanner must accept an intentionally empty selection, but exited {:?}.\n--- output ---\n{text}",
        out.status.code()
    );
    assert!(
        text.contains("scanned 0 file(s)"),
        "the empty staged selection must be visible.\n--- output ---\n{text}"
    );
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

/// C-08: `--staged` must inspect the exact blob Git will commit. The filesystem
/// deliberately contains the clean fixture after the bad bytes were staged.
#[test]
fn staged_bad_working_tree_clean_is_blocked() {
    let repo = disposable_repo();
    let root = repo.path();
    let path = "docs/staged-bad-PLAN.md";
    let bad = fixture_text("comment-bypass-PLAN.md");
    let clean = fixture_text("legit-PLAN.md");

    write_plan(root, path, &bad);
    git_ok(root, &["add", path]);
    assert!(
        String::from_utf8(index_blob(root, path)).expect("index plan is UTF-8") == bad,
        "fixture precondition: the index must contain the violating plan"
    );
    write_plan(root, path, &clean);
    assert!(
        fs::read_to_string(root.join(path)).expect("read clean working-tree plan") == clean,
        "fixture precondition: the working tree must contain only the clean plan"
    );

    let out = run_staged(root);
    assert_staged_refusal(&out, path);
}

/// The opposite C-08 direction is a negative control. A dirty worktree is not
/// selected when its committed index blob is clean, so `--staged` must visibly
/// scan zero files instead of reading the unstaged violating text.
#[test]
fn staged_clean_working_tree_bad_is_accepted() {
    let repo = disposable_repo();
    let root = repo.path();
    let path = "docs/committed-clean-PLAN.md";
    let clean = fixture_text("legit-PLAN.md");
    let bad = fixture_text("comment-bypass-PLAN.md");

    commit_plan(root, path, &clean);
    assert!(
        String::from_utf8(index_blob(root, path)).expect("index plan is UTF-8") == clean,
        "fixture precondition: the committed index must contain the clean plan"
    );
    write_plan(root, path, &bad);
    assert!(
        fs::read_to_string(root.join(path)).expect("read violating working-tree plan") == bad,
        "fixture precondition: the working tree must contain the violating plan"
    );

    let out = run_staged(root);
    assert_staged_zero_scan(&out);
}

/// C-09: a staged rename has a destination blob that Git will commit. Its tab
/// exercises the NUL-delimited discovery branch: line-oriented Git output would
/// quote or split this name and prove the wrong path.
#[test]
fn staged_renamed_bad_plan_is_blocked() {
    let repo = disposable_repo();
    let root = repo.path();
    let old_path = "docs/old-PLAN.md";
    let new_path = "docs/new\tbad-PLAN.md";
    let bad = fixture_text("comment-bypass-PLAN.md");

    commit_plan(root, old_path, &bad);
    git_ok(root, &["mv", old_path, new_path]);
    let name_status = git_ok(root, &["diff", "--cached", "--name-status", "-z"]);
    let fields = name_status
        .stdout
        .split(|byte| *byte == b'\0')
        .collect::<Vec<_>>();
    assert!(
        fields
            .first()
            .is_some_and(|status| status.starts_with(b"R"))
            && fields.contains(&new_path.as_bytes()),
        "fixture precondition: cached NUL-delimited name status must identify a rename to {new_path:?}; got {:?}",
        fields
    );
    assert!(
        String::from_utf8(index_blob(root, new_path)).expect("renamed index plan is UTF-8") == bad,
        "fixture precondition: the renamed destination index blob must be violating"
    );

    let out = run_staged(root);
    assert_staged_refusal(&out, new_path);
}

/// Deletions are intentionally excluded: after `git rm` there is no index blob
/// to inspect, and the empty selection must remain a successful visible result.
#[test]
fn staged_deleted_plan_is_skipped() {
    let repo = disposable_repo();
    let root = repo.path();
    let path = "docs/deleted-PLAN.md";

    commit_plan(root, path, &fixture_text("legit-PLAN.md"));
    git_ok(root, &["rm", path]);
    assert!(
        !root.join(path).exists(),
        "fixture precondition: git rm must remove the working-tree path"
    );
    let name_status = git_ok(root, &["diff", "--cached", "--name-status", "-z"]);
    assert!(
        name_status.stdout.starts_with(b"D\0"),
        "fixture precondition: cached NUL-delimited name status must identify a deletion; got {:?}",
        name_status.stdout
    );

    let out = run_staged(root);
    assert_staged_zero_scan(&out);
}

/// Scan `contents` saved as `file_name` in a fresh scratch directory.
///
/// The rules below are exercised with inline plans instead of committed fixtures:
/// a committed `*PLAN.md` fixture that violates a rule is refused by the
/// pre-commit hook, which runs this same scanner over every staged plan.
fn run_inline(file_name: &str, contents: &str) -> Output {
    let dir = tempfile::tempdir().expect("create scratch directory for an inline plan");
    let path = dir.path().join(file_name);
    fs::write(&path, contents)
        .unwrap_or_else(|err| panic!("write inline plan {}: {err}", path.display()));
    run(&[path.as_path()])
}

/// A crash inside the scanner's awk program must never read as a refusal. An
/// apostrophe in one of its comments once closed the program's shell quoting, so
/// gawk aborted with "unterminated regexp" and every plan was refused without a
/// report naming any file.
fn assert_no_scanner_crash(file_name: &str, text: &str) {
    assert!(
        !text.contains("awk:"),
        "the scanner crashed while scanning {file_name} instead of evaluating its \
         rules.\n--- output ---\n{text}"
    );
}

fn assert_inline_blocked(file_name: &str, contents: &str, expected_text: &str) {
    let out = run_inline(file_name, contents);
    let text = combined(&out);
    assert_no_scanner_crash(file_name, &text);
    assert!(
        !out.status.success(),
        "{file_name} must be REFUSED by the scanner, but it exited 0.\n--- output ---\n{text}"
    );
    assert!(
        text.lines()
            .any(|line| line.contains(file_name) && line.contains(expected_text)),
        "the refusal for {file_name} must print one line naming BOTH the file and \
         {expected_text:?}.\n--- output ---\n{text}"
    );
}

fn assert_inline_accepted(file_name: &str, contents: &str) {
    let out = run_inline(file_name, contents);
    let text = combined(&out);
    assert_no_scanner_crash(file_name, &text);
    assert!(
        out.status.success(),
        "{file_name} is a legitimate plan and must be ACCEPTED, but the scanner exited \
         {:?}.\n--- output ---\n{text}",
        out.status.code()
    );
}

/// Wrap one command in a single `<automated>` block.
fn gate(command: &str) -> String {
    format!("<verify>\n  <automated>{command}</automated>\n</verify>\n")
}

/// `rg -c` prints nothing and exits 1 when nothing matches, so a count compared
/// in shell misreads the zero case. The old `| rg '^0$'` shape is still caught.
#[test]
fn rg_count_piped_to_zero_match_is_blocked() {
    assert_inline_blocked(
        "rg-count-zero-PLAN.md",
        &gate("rg -c needle src/lib.rs | rg '^0$'"),
        "rg count flag -c",
    );
}

#[test]
fn rg_count_in_a_flag_cluster_is_blocked() {
    assert_inline_blocked(
        "rg-count-cluster-PLAN.md",
        &gate("test \"$(rg -nc needle src/lib.rs)\" = 3"),
        "rg count flag -nc",
    );
}

#[test]
fn rg_count_matches_is_blocked() {
    assert_inline_blocked(
        "rg-count-matches-PLAN.md",
        &gate("rg --count-matches -F needle src/lib.rs || true"),
        "rg count flag --count-matches",
    );
}

/// Negative control for the count rule: `-c` as the value of `-e`, inside a
/// quoted pattern, and the context flags `-C`/`-A` are not counts.
#[test]
fn rg_without_a_count_flag_is_accepted() {
    assert_inline_accepted(
        "rg-no-count-PLAN.md",
        &gate("rg -q -e '-c' f && ! rg -n \"bash -c\" g && rg -C 2 x f && rg -A1 -e x f"),
    );
}

/// A `cargo test` name filter that matches nothing still exits 0, including a
/// bare name after `--exact` and a filter on a package with no lib target.
#[test]
fn cargo_test_in_a_gate_is_blocked() {
    for (file, command) in [
        (
            "cargo-test-suite-PLAN.md",
            "cargo test -p devflow-core --lib",
        ),
        (
            "exact-bare-name-PLAN.md",
            "cargo test -p devflow-core --lib -- --exact holder_status",
        ),
        (
            "devflow-lib-PLAN.md",
            "cargo test -p devflow --lib stop::tests::x",
        ),
        (
            "cargo-test-toolchain-PLAN.md",
            "cargo +1.97.1 test -p devflow --bin devflow lock::tests::x",
        ),
    ] {
        assert_inline_blocked(file, &gate(command), "cargo test in a verify gate");
    }
}

#[test]
fn nextest_without_no_tests_fail_is_blocked() {
    assert_inline_blocked(
        "nextest-no-fail-PLAN.md",
        &gate("cargo nextest run -p devflow-core --lib -E 'test(=a::b)'"),
        "without --no-tests=fail",
    );
}

/// `-E 'test(=real) | test(=wrong)'` runs the real test, skips the wrong name
/// silently and exits 0 (verified on cargo-nextest 0.9.148).
#[test]
fn nextest_filter_naming_two_tests_is_blocked() {
    assert_inline_blocked(
        "nextest-pipe-PLAN.md",
        &gate(
            "cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=a::b) | test(=c::d)'",
        ),
        "filter is not exactly test(=module::path::name)",
    );
}

#[test]
fn nextest_with_two_filter_flags_is_blocked() {
    assert_inline_blocked(
        "nextest-two-e-PLAN.md",
        &gate(
            "cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=a::b)' -E 'test(=c::d)'",
        ),
        "with 2 test filters",
    );
}

#[test]
fn nextest_substring_filter_is_blocked() {
    assert_inline_blocked(
        "nextest-substring-PLAN.md",
        &gate("cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(a::b)'"),
        "filter is not exactly test(=module::path::name)",
    );
}

#[test]
fn nextest_positional_filters_are_blocked() {
    for (file, command) in [
        (
            "nextest-positional-PLAN.md",
            "cargo nextest run -p devflow-core --lib --no-tests=fail a::b",
        ),
        (
            "nextest-dashdash-PLAN.md",
            "cargo nextest run -p devflow-core --lib --no-tests=fail -- --exact a::b",
        ),
    ] {
        assert_inline_blocked(file, &gate(command), "positional test filter");
    }
}

/// The rules read the command inside a `bash -c '...'` region too, where its
/// own quotes are double quotes.
#[test]
fn nextest_inside_bash_c_without_no_tests_fail_is_blocked() {
    assert_inline_blocked(
        "nextest-bash-c-PLAN.md",
        &gate(
            "bash -c 'cargo nextest run -p devflow-core --lib -E \"test(=a::b)\" 2>&1 | tail -3; echo \"x=${PIPESTATUS[0]}\"'",
        ),
        "without --no-tests=fail",
    );
}

#[test]
fn devflow_cli_package_is_blocked() {
    assert_inline_blocked(
        "devflow-cli-package-PLAN.md",
        &gate("cargo nextest run -p devflow-cli --lib --no-tests=fail -E 'test(=a::b)'"),
        "-p devflow-cli names no package",
    );
}

/// A quote that never closes leaves the command undecidable; refuse it.
#[test]
fn unparsable_command_is_blocked() {
    assert_inline_blocked(
        "nextest-unbalanced-PLAN.md",
        &gate("cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=a::b)"),
        "cannot parse this cargo command",
    );
}

/// An escaped quote inside double quotes does not end them, so the command is
/// still read and its count flag found.
#[test]
fn escaped_quote_inside_double_quotes_is_parsed() {
    assert_inline_blocked(
        "rg-escaped-quote-PLAN.md",
        &gate("bash -c 'n=$(rg -U --count-matches \"token = \\\"$t\\\"\" f || true)'"),
        "rg count flag --count-matches",
    );
    assert_inline_accepted(
        "rg-escaped-quote-ok-PLAN.md",
        &gate("bash -c 'rg -q \"token = \\\"$t\\\"\" f'"),
    );
}

/// The form the rules ask for: one exact test per command, chained with `&&`.
#[test]
fn exact_nextest_commands_chained_are_accepted() {
    assert_inline_accepted(
        "nextest-chain-PLAN.md",
        &gate(
            "cargo nextest run -p devflow --bin devflow --no-tests=fail -E 'test(=lock::tests::holder_status)' && cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=verify::tests::parser)'",
        ),
    );
}

/// A whole suite with no filter, the spaced `--no-tests fail` form, and the `2`
/// of `2>&1`, which is a file descriptor and not a positional filter.
#[test]
fn nextest_suite_run_is_accepted() {
    assert_inline_accepted(
        "nextest-suite-PLAN.md",
        &gate("cargo nextest run -p devflow --test start_lock_e2e --no-tests fail 2>&1"),
    );
}

#[test]
fn exact_nextest_inside_bash_c_is_accepted() {
    assert_inline_accepted(
        "nextest-bash-c-ok-PLAN.md",
        &gate(
            "bash -c 'cargo nextest run -p devflow-core --lib --no-tests=fail -E \"test(=a::b)\" 2>&1 | tail -3; echo \"x=${PIPESTATUS[0]}\"'",
        ),
    );
}

/// The rules read only `<automated>` blocks: plan prose may name the shapes.
#[test]
fn command_shapes_in_prose_are_accepted() {
    assert_inline_accepted(
        "prose-PLAN.md",
        "Prose: cargo test -p devflow-core --lib and rg -c x are refused in gates.\n\
         <verify>\n  <automated>cargo fmt --check</automated>\n</verify>\n",
    );
}
