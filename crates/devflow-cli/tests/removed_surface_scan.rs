//! Scans that keep the removed `--force` surface from coming back (52.1 D-09, D-11,
//! D-09.1): no CLI message offers `--force` or a raw `git branch -D` as the way on, and
//! no scoped doc does. `doc_check`'s `documented_flags` cannot catch either, since a
//! `--force` git argument in devflow-core's `worktree.rs` satisfies it.

use std::path::{Path, PathBuf};

/// The workspace root: two levels above this crate's manifest.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate sits two levels below the workspace root")
        .to_path_buf()
}

/// Every file under `dir` with extension `ext`, recursively, sorted.
fn files_with_extension(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in
            std::fs::read_dir(&dir).unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|found| found == ext) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// Whether `line` carries the `--force` flag itself, not `--force-with-lease`.
fn names_force(line: &str) -> bool {
    line.match_indices("--force")
        .any(|(at, _)| !line[at..].starts_with("--force-with-lease"))
}

/// The code lines of CLI `source` that offer `--force` or a raw `git branch -D` to
/// the operator, as `line: text`. `source` is cut at its first `mod tests {` line, a
/// line whose trimmed start is `//` is a comment and not counted, and a line passing
/// `"worktree"` as an argument is a `git worktree` call site, whose `--force` is git's.
fn cli_lines_offering_force(source: &str) -> Vec<String> {
    source
        .lines()
        .enumerate()
        .take_while(|(_, line)| !line.contains("mod tests {"))
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| !line.contains("\"worktree\""))
        .filter(|(_, line)| names_force(line) || line.contains("git branch -D"))
        .map(|(index, line)| format!("{}: {}", index + 1, line.trim()))
        .collect()
}

/// The lines of a doc that offer `--force` or a raw `git branch -D`, as `line: text`.
/// A `cargo install` line's `--force` is cargo's reinstall flag, not DevFlow's.
fn doc_lines_offering_force(doc: &str) -> Vec<String> {
    doc.lines()
        .enumerate()
        .filter(|(_, line)| {
            (names_force(line) && !line.contains("cargo install")) || line.contains("git branch -D")
        })
        .map(|(index, line)| format!("{}: {}", index + 1, line.trim()))
        .collect()
}

/// `find` over each of `files`, as `path:line: text` relative to the workspace root.
fn findings_in(files: &[PathBuf], find: fn(&str) -> Vec<String>) -> Vec<String> {
    let root = workspace_root();
    files
        .iter()
        .flat_map(|path| {
            let text = std::fs::read_to_string(path)
                .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
            let shown = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .display()
                .to_string();
            find(&text)
                .into_iter()
                .map(move |finding| format!("{shown}:{finding}"))
        })
        .collect()
}

/// 52.1 D-09, D-09.1: no CLI string offers `--force` or a raw `git branch -D` as the
/// way on. Controls: each planted message counts 1; the same text as a comment, after a
/// `mod tests {` line, as a `git worktree` argument or as `--force-with-lease` counts 0.
#[test]
fn no_cli_message_offers_force_as_the_way_on() {
    for planted in [
        "        \"run devflow cleanup --force to remove it\"",
        "    format!(\"or pass --force to recreate phase {phase}\")",
        "        \"to delete it yourself, run `git branch -D {branch}`\"",
    ] {
        assert_eq!(
            cli_lines_offering_force(planted).len(),
            1,
            "planted message: {planted}"
        );
    }
    for decoy in [
        "    // a later start --force replaced it",
        "mod tests {\n    \"run devflow cleanup --force\"",
        "    run(root, &[\"worktree\", \"remove\", \"--force\", &path])",
        "    \"git push --force-with-lease\"",
        "    \"devflow discard --phase 7\"",
    ] {
        assert_eq!(
            cli_lines_offering_force(decoy),
            Vec::<String>::new(),
            "decoy: {decoy}"
        );
    }

    let sources = files_with_extension(&workspace_root().join("crates/devflow-cli/src"), "rs");
    assert!(
        sources.len() > 10,
        "the CLI sources were not found: {sources:?}"
    );
    let found = findings_in(&sources, cli_lines_offering_force);
    assert_eq!(
        found,
        Vec::<String>::new(),
        "a CLI message offers --force or git branch -D"
    );
}

/// The scoped docs `doc_check` reads: README, ARCHITECTURE, CONTRIBUTING, OPERATIONS
/// and `docs/guides/*.md`.
fn scoped_docs() -> Vec<PathBuf> {
    let root = workspace_root();
    let mut docs: Vec<PathBuf> = [
        "README.md",
        "ARCHITECTURE.md",
        "CONTRIBUTING.md",
        "OPERATIONS.md",
    ]
    .iter()
    .map(|name| root.join(name))
    .collect();
    docs.extend(files_with_extension(&root.join("docs/guides"), "md"));
    docs
}

/// 52.1 D-11, D-09.1: no scoped doc offers `--force` or a raw `git worktree remove
/// --force && git branch -D`. Controls: a flag row, a raw removal line (one finding for
/// its two tokens) and a bare `git branch -D` count 1 each; `--force-with-lease`, a
/// `cargo install --force` line and `devflow discard` count 0.
#[test]
fn no_doc_offers_force_or_a_raw_branch_delete() {
    for planted in [
        "| `--force` | Recreate the phase worktree and feature branch |",
        "run `git worktree remove --force .worktrees/phase-07 && git branch -D feature/phase-07`",
        "delete it with `git branch -D feature/phase-07`",
    ] {
        assert_eq!(
            doc_lines_offering_force(planted).len(),
            1,
            "planted doc text: {planted}"
        );
    }
    for decoy in [
        "push with `git push --force-with-lease`",
        "`cargo install --path crates/devflow-cli --locked --force`",
        "remove it with `devflow discard --phase 7`",
    ] {
        assert_eq!(
            doc_lines_offering_force(decoy),
            Vec::<String>::new(),
            "decoy: {decoy}"
        );
    }

    let docs = scoped_docs();
    assert!(docs.len() > 4, "the guides were not found: {docs:?}");
    let found = findings_in(&docs, doc_lines_offering_force);
    assert_eq!(
        found,
        Vec::<String>::new(),
        "a doc offers --force or git branch -D"
    );
}
