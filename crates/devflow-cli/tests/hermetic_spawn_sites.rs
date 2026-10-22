//! Permanent source scan for 999.141 / D-07 (50-21, R6-G4): every e2e suite
//! spawns `devflow` through `common::hermetic_devflow`, so no suite's child
//! reaches the operator's real `~/.cache/devflow` (and, from 50-11, no suite
//! binds a socket there).
//!
//! It replaces the shell counts the migration plans used as gates. It spawns
//! nothing and does not include the shared test helpers. The scanned tokens
//! are spelled through `concat!`, so this file never holds the binary's env
//! name or a raw spawn as written and cannot match itself.
//!
//! The comment rule is deliberately simple: a line whose trimmed start is
//! `//` is a comment and is skipped; every other line is code.

use std::fs;
use std::path::{Path, PathBuf};

/// Suites migrated to the hermetic builder whose spawn sites this scan pins.
const HERMETIC_SUITES: &[&str] = &[
    "auto_chain_flag_e2e.rs",
    "yes_ship_config.rs",
    "release_check.rs",
    "reap_strays_e2e.rs",
    "help_snapshot.rs",
];

/// The only files under `tests/` (paths relative to it, `/`-separated) whose
/// code may name the binary's env var. `common/mod.rs` is the one legitimate
/// site. `recover_clean_e2e.rs` is 50-04's migration and is tolerated so the
/// scan holds whichever of 50-04 and 50-21 lands first.
const BINARY_NAMERS: &[&str] = &["common/mod.rs", "recover_clean_e2e.rs"];

/// `crates/devflow-cli/tests`.
fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// The lines of `source` whose trimmed start is not `//`.
fn code_lines(source: &str) -> Vec<&str> {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect()
}

/// Cargo's env var naming the compiled `devflow` binary, assembled so this
/// file never spells it.
fn binary_env_name() -> &'static str {
    concat!("CARGO_BIN_EXE_", "devflow")
}

/// Occurrences, in the code lines of `source`, of a raw `devflow` spawn
/// (`Command::new` over the local or the shared `devflow_bin()`) or of the
/// binary's env name.
fn raw_spawn_sites(source: &str) -> usize {
    let tokens = [
        concat!("Command::new(", "devflow_bin())"),
        concat!("Command::new(common::", "devflow_bin())"),
        binary_env_name(),
    ];
    code_lines(source)
        .iter()
        .map(|line| {
            tokens
                .iter()
                .map(|token| line.matches(token).count())
                .sum::<usize>()
        })
        .sum()
}

/// The source of `tests/<name>`. Panics naming the path when it cannot be
/// read, so a missing or renamed suite fails rather than reading as clean.
fn read_suite(name: &str) -> String {
    let path = tests_dir().join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("cannot read suite {}: {err}", path.display()))
}

/// Whether any code line of `source` names the binary's env var.
fn names_binary(source: &str) -> bool {
    code_lines(source)
        .iter()
        .any(|line| line.contains(binary_env_name()))
}

/// Every `.rs` file below `dir`, recursively, as a path relative to `base`
/// joined with `/`. Panics on an unreadable directory or entry, so a walk
/// that cannot see a file fails instead of reading as clean.
fn rust_files_under(base: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("cannot read directory {}: {err}", dir.display()));
    for entry in entries {
        let entry =
            entry.unwrap_or_else(|err| panic!("cannot read an entry of {}: {err}", dir.display()));
        let path = entry.path();
        // `file_type` does not follow symlinks, so a link loop cannot recurse.
        let file_type = entry
            .file_type()
            .unwrap_or_else(|err| panic!("cannot stat {}: {err}", path.display()));
        if file_type.is_dir() {
            rust_files_under(base, &path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let relative = path
                .strip_prefix(base)
                .expect("walked path is under the walk base")
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            files.push((relative, path));
        }
    }
}

#[test]
fn migrated_suites_spawn_devflow_only_through_the_hermetic_builder() {
    // Planted controls first, on inline strings: the matcher must flag each
    // raw form and pass a comment and the hermetic builder.
    let raw_spawn = concat!("let out = Command::new(", "devflow_bin()).output();");
    assert_eq!(
        raw_spawn_sites(raw_spawn),
        1,
        "a raw local spawn must count"
    );
    let shared_spawn = concat!(
        "let out = Command::new(common::",
        "devflow_bin()).output();"
    );
    assert_eq!(
        raw_spawn_sites(shared_spawn),
        1,
        "a raw spawn over the shared devflow_bin() must count"
    );
    let env_line = format!("let bin = env!(\"{}\");", binary_env_name());
    assert_eq!(
        raw_spawn_sites(&env_line),
        1,
        "a code line naming the binary's env var must count"
    );
    let commented = format!("// {raw_spawn}");
    assert_eq!(
        raw_spawn_sites(&commented),
        0,
        "a commented-out spawn must not count"
    );
    let hermetic = "let out = common::hermetic_devflow(&home).output();";
    assert_eq!(
        raw_spawn_sites(hermetic),
        0,
        "the hermetic builder must not count"
    );

    // Then every listed suite, read from disk.
    let mut failures = Vec::new();
    for name in HERMETIC_SUITES {
        let source = read_suite(name);
        let raw = raw_spawn_sites(&source);
        if raw != 0 {
            failures.push(format!(
                "{name}: {raw} raw devflow spawn site(s) or binary env name(s); expected 0"
            ));
        }
        // Real-data control: the scan read the migrated file, not an empty
        // or unrelated string.
        let uses_builder = code_lines(&source)
            .iter()
            .any(|line| line.contains("common::hermetic_devflow("));
        if !uses_builder {
            failures.push(format!(
                "{name}: no `common::hermetic_devflow(` call in its code lines"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "suites not spawning devflow only through the hermetic builder:\n{}",
        failures.join("\n")
    );
}

#[test]
fn only_common_names_the_devflow_binary() {
    // Planted controls first, on inline strings run through the same matcher.
    let naming = format!("    let bin = env!(\"{}\");", binary_env_name());
    assert!(
        names_binary(&naming),
        "a code line naming the binary's env var must be detected"
    );
    let commented = format!("// {naming}");
    assert!(
        !names_binary(&commented),
        "the same line behind `// ` must not be detected"
    );

    let base = tests_dir();
    let mut files = Vec::new();
    rust_files_under(&base, &base, &mut files);
    let mut namers: Vec<String> = files
        .into_iter()
        .filter(|(_, path)| {
            let source = fs::read_to_string(path)
                .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
            names_binary(&source)
        })
        .map(|(relative, _)| relative)
        .collect();
    namers.sort();

    // Real-data control: the walk and the matcher find the one legitimate
    // site.
    assert!(
        namers.iter().any(|path| path == "common/mod.rs"),
        "common/mod.rs was not found naming the binary under {}; found {namers:?}",
        base.display()
    );
    let unexpected: Vec<&String> = namers
        .iter()
        .filter(|path| !BINARY_NAMERS.contains(&path.as_str()))
        .collect();
    assert!(
        unexpected.is_empty(),
        "files under {} name the devflow binary outside common/mod.rs; spawn it through \
         common::hermetic_devflow instead:\n{}",
        base.display(),
        unexpected
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
