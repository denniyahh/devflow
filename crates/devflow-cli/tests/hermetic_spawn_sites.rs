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
];

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
