//! Deterministic operator-documentation and runtime-path invariants.

use std::path::{Path, PathBuf};

use crate::phase_id::PhaseId;
use crate::{agent_result, events, gates::Gates, lock, ship, workflow};

const SCOPED_DOCS: &[&str] = &[
    "README.md",
    "ARCHITECTURE.md",
    "CONTRIBUTING.md",
    "OPERATIONS.md",
];

#[derive(Debug, Default, serde::Deserialize)]
struct Allowlist {
    #[serde(default)]
    exceptions: Vec<AllowlistEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct AllowlistEntry {
    kind: String,
    token: String,
    reason: Option<String>,
}

fn validate_allowlist(allowlist: &Allowlist) -> Result<(), String> {
    for entry in &allowlist.exceptions {
        if entry.reason.as_deref().is_none_or(str::is_empty) {
            return Err(format!(
                "allowlist exception `{}` ({}) requires a non-empty reason",
                entry.token, entry.kind
            ));
        }
    }
    Ok(())
}

fn load_allowlist() -> Allowlist {
    let contents = std::fs::read_to_string(workspace_root().join("doc-check-allowlist.toml"))
        .expect("doc-check-allowlist.toml must be readable");
    let allowlist: Allowlist = toml::from_str(&contents).expect("allowlist must be valid TOML");
    validate_allowlist(&allowlist).expect("every allowlist entry must explain its exception");
    allowlist
}

fn is_allowlisted(allowlist: &Allowlist, kind: &str, token: &str) -> bool {
    allowlist
        .exceptions
        .iter()
        .any(|entry| entry.kind == kind && entry.token == token)
}

fn scoped_doc_paths() -> Vec<PathBuf> {
    let root = workspace_root();
    let mut paths: Vec<_> = SCOPED_DOCS.iter().map(|path| root.join(path)).collect();
    let guides = root.join("docs/guides");
    if let Ok(entries) = std::fs::read_dir(guides) {
        paths.extend(entries.flatten().filter_map(|entry| {
            (entry.path().extension().and_then(|ext| ext.to_str()) == Some("md"))
                .then(|| entry.path())
        }));
    }
    paths.sort();
    paths
}

fn read_files(paths: impl IntoIterator<Item = PathBuf>) -> String {
    paths
        .into_iter()
        .map(|path| {
            std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn rust_source() -> String {
    read_files(rust_file_paths())
}

/// Every `.rs` file under `<workspace>/crates`, both crates, src and tests,
/// in sorted order.
fn rust_file_paths() -> Vec<PathBuf> {
    fn visit(dir: &Path, files: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    visit(&workspace_root().join("crates"), &mut files);
    files.sort();
    files
}

fn extract_tokens(text: &str, prefix: &str, allowed: impl Fn(char) -> bool) -> Vec<String> {
    let mut found = Vec::new();
    let mut offset = 0;
    while let Some(index) = text[offset..].find(prefix) {
        let start = offset + index;
        let end = text[start..]
            .char_indices()
            .take_while(|(index, ch)| *index < prefix.len() || allowed(*ch))
            .last()
            .map(|(index, ch)| start + index + ch.len_utf8())
            .unwrap_or(start + prefix.len());
        found.push(text[start..end].to_owned());
        offset = end.max(start + prefix.len());
    }
    found.sort();
    found.dedup();
    found
}

fn documented_subcommands(docs: &str) -> Vec<String> {
    extract_tokens(docs, "devflow ", |ch| ch.is_ascii_lowercase() || ch == '-')
        .into_iter()
        .filter_map(|token| token.split_whitespace().nth(1).map(str::to_owned))
        .collect()
}

fn documented_flags(docs: &str) -> Vec<String> {
    let mut flags: Vec<_> = docs
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|ch: char| {
                matches!(
                    ch,
                    '`' | '[' | ']' | '(' | ')' | ',' | '.' | ':' | '|' | '"'
                )
            })
        })
        .filter(|word| {
            word.starts_with("--")
                && word.len() > 2
                && word[2..]
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch == '-')
        })
        .map(str::to_owned)
        .collect();
    flags.sort();
    flags.dedup();
    flags
}

fn documented_rust_identifiers(docs: &str) -> Vec<String> {
    let mut identifiers = Vec::new();
    for (index, span) in docs.split('`').enumerate() {
        if index % 2 == 0 || span.chars().any(char::is_whitespace) {
            continue;
        }
        let callable = span.contains('(');
        let qualified = span.contains("::");
        let base = span
            .split('(')
            .next()
            .unwrap_or(span)
            .trim_end_matches("()");
        let identifier = base.rsplit("::").next().unwrap_or(base);
        let upper_camel = identifier.chars().next().is_some_and(char::is_uppercase)
            && identifier.chars().any(char::is_lowercase);
        if (callable || qualified || upper_camel)
            && !identifier.is_empty()
            && identifier
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            identifiers.push(identifier.to_owned());
        }
    }
    identifiers.sort();
    identifiers.dedup();
    identifiers
}

fn enum_variants(source: &str, enum_name: &str) -> Vec<String> {
    let marker = format!("enum {enum_name} {{");
    let body = source
        .split_once(&marker)
        .map(|(_, body)| body)
        .unwrap_or("");
    let mut depth = 1_i32;
    let mut variants = Vec::new();
    for line in body.lines() {
        if depth == 1 {
            let trimmed = line.trim();
            if let Some(name) = trimmed
                .split(['{', ','])
                .next()
                .map(str::trim)
                .filter(|name| {
                    !name.is_empty()
                        && name.chars().next().is_some_and(char::is_uppercase)
                        && name.chars().all(char::is_alphabetic)
                })
            {
                variants.push(name.to_ascii_lowercase());
            }
        }
        depth += line.matches('{').count() as i32;
        depth -= line.matches('}').count() as i32;
        if depth <= 0 {
            break;
        }
    }
    variants.sort();
    variants.dedup();
    variants
}

fn source_read_env_vars(source: &str) -> Vec<String> {
    let all = extract_tokens(source, "DEVFLOW_", |ch| {
        ch.is_ascii_uppercase() || ch == '_' || ch.is_ascii_digit()
    });
    all.into_iter()
        .filter(|token| {
            source.contains(&format!("std::env::var(\"{token}\""))
                || source.contains(&format!("std::env::var_os(\"{token}\""))
                || source.contains(&format!("env_value(\"{token}\""))
        })
        .collect()
}

fn source_contains_path_shape(source: &str, token: &str) -> bool {
    let fragments: Vec<_> = token
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| part.len() >= 3 && !matches!(*part, "devflow" | "phase" | "NN"))
        .collect();
    !fragments.is_empty() && fragments.iter().all(|fragment| source.contains(fragment))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("devflow-core must live under <workspace>/crates")
        .to_path_buf()
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    let mut rest = value;
    let parts = pattern.split('*');
    let anchored_start = !pattern.starts_with('*');
    let anchored_end = !pattern.ends_with('*');
    let mut first = true;

    for part in parts {
        if part.is_empty() {
            first = false;
            continue;
        }
        if first && anchored_start {
            let Some(next) = rest.strip_prefix(part) else {
                return false;
            };
            rest = next;
        } else if let Some(index) = rest.find(part) {
            rest = &rest[index + part.len()..];
        } else {
            return false;
        }
        first = false;
    }

    !anchored_end || rest.is_empty()
}

fn pattern_covers(pattern: &str, relative_path: &str) -> bool {
    let pattern = pattern.trim_start_matches('/');
    if pattern.ends_with('/') {
        relative_path == pattern.trim_end_matches('/') || relative_path.starts_with(pattern)
    } else {
        glob_matches(pattern, relative_path)
    }
}

#[test]
fn gitignore_covers_all_devflow_paths() {
    let root = Path::new("/representative-project");
    let paths = [
        ("events::events_path", events::events_path(root)),
        (
            "agent_result::stdout_path",
            agent_result::stdout_path(root, PhaseId::new(16)),
        ),
        (
            "agent_result::stderr_path",
            agent_result::stderr_path(root, PhaseId::new(16)),
        ),
        (
            "agent_result::exit_code_path",
            agent_result::exit_code_path(root, PhaseId::new(16)),
        ),
        (
            "agent_result::agent_pid_path",
            agent_result::agent_pid_path(root, PhaseId::new(16)),
        ),
        (
            "agent_result::stop_marker_path",
            agent_result::stop_marker_path(root, PhaseId::new(16)),
        ),
        (
            "agent_result::history_dir",
            agent_result::history_dir(root, PhaseId::new(16)),
        ),
        (
            "workflow::state_path",
            workflow::state_path(root, PhaseId::new(16)),
        ),
        (
            "workflow::legacy_state_path",
            workflow::legacy_state_path(root),
        ),
        ("Gates::dir", Gates::dir(root)),
        ("lock::lock_path", lock::lock_path(root, PhaseId::new(16))),
        ("lock::project_lock_path", lock::project_lock_path(root)),
        (
            "ship::cron_instructions_path",
            ship::cron_instructions_path(root, PhaseId::new(16)),
        ),
        (
            "ship::legacy_cron_instructions_path",
            ship::legacy_cron_instructions_path(root),
        ),
    ];
    let ignore = std::fs::read_to_string(workspace_root().join(".gitignore"))
        .expect("workspace .gitignore must be readable");
    let patterns: Vec<_> = ignore
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('!'))
        .collect();

    for (constructor, path) in paths {
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .trim_start_matches('/')
            .to_owned();
        assert!(
            patterns
                .iter()
                .any(|pattern| pattern_covers(pattern, &relative)),
            "{constructor} produced uncovered runtime path `{relative}`; add a matching .gitignore rule"
        );
    }
}

#[test]
fn doc_referenced_identifiers_exist_in_source() {
    let docs = read_files(scoped_doc_paths());
    let source = rust_source();
    let allowlist = load_allowlist();

    for token in extract_tokens(&docs, "DEVFLOW_", |ch| {
        ch.is_ascii_uppercase() || ch == '_' || ch.is_ascii_digit()
    }) {
        assert!(
            source.contains(&token) || is_allowlisted(&allowlist, "docs_to_source", &token),
            "documented environment variable `{token}` does not exist in Rust source"
        );
    }

    let command_variants = enum_variants(&source, "Command");
    let gate_variants = enum_variants(&source, "GateCmd");
    for token in documented_subcommands(&docs) {
        assert!(
            command_variants.contains(&token)
                || gate_variants.contains(&token)
                || is_allowlisted(&allowlist, "docs_to_source", &format!("devflow {token}")),
            "documented command `devflow {token}` does not exist in the CLI enum surface"
        );
    }

    for token in documented_flags(&docs) {
        let field = token.trim_start_matches('-').replace('-', "_");
        assert!(
            source.contains(&token)
                || source.contains(&format!("{field}:"))
                || is_allowlisted(&allowlist, "docs_to_source", &token),
            "documented CLI flag `{token}` does not exist in Rust source"
        );
    }

    for token in documented_rust_identifiers(&docs) {
        assert!(
            source.contains(&token) || is_allowlisted(&allowlist, "docs_to_source", &token),
            "documented Rust identifier `{token}` does not exist in Rust source"
        );
    }

    for token in extract_tokens(&docs, ".devflow/", |ch| {
        ch.is_ascii_alphanumeric() || "_-.*/{}<>".contains(ch)
    }) {
        assert!(
            source.contains(&token)
                || source_contains_path_shape(&source, &token)
                || is_allowlisted(&allowlist, "docs_to_source", &token),
            "documented runtime path `{token}` has no matching source constructor"
        );
    }
}

#[test]
fn source_devflow_env_vars_and_subcommands_are_documented() {
    let docs = read_files(scoped_doc_paths());
    let source = rust_source();
    let allowlist = load_allowlist();

    for token in source_read_env_vars(&source) {
        assert!(
            docs.contains(&token) || is_allowlisted(&allowlist, "source_to_docs", &token),
            "source-read environment variable `{token}` is missing from scoped operator docs"
        );
    }

    for command in enum_variants(&source, "Command") {
        let token = format!("devflow {command}");
        assert!(
            docs.to_ascii_lowercase().contains(&token)
                || is_allowlisted(&allowlist, "source_to_docs", &token),
            "CLI subcommand `{token}` is missing from scoped operator docs"
        );
    }
}

#[test]
fn pinned_doc_claims_match_source() {
    let docs = read_files(scoped_doc_paths());
    let cli = std::fs::read_to_string(workspace_root().join("crates/devflow-cli/src/main.rs"))
        .expect("CLI source must be readable");

    assert!(
        cli.matches("EnvFilter::new(\"info\")").count() >= 2,
        "RUST_LOG's source fallback must remain info in both log-format branches"
    );
    assert!(
        docs.lines()
            .any(|line| line.contains("`RUST_LOG`") && line.contains("`info`")),
        "operator docs must pin RUST_LOG's default to the source fallback `info`"
    );
}

#[test]
fn allowlist_entries_require_reasons() {
    let reasonless: Allowlist =
        toml::from_str("[[exceptions]]\nkind = \"docs_to_source\"\ntoken = \"example\"\n").unwrap();
    assert!(validate_allowlist(&reasonless).is_err());
    validate_allowlist(&load_allowlist()).unwrap();
}

#[derive(serde::Deserialize)]
struct RootCargoToml {
    workspace: RootWorkspaceSection,
}

#[derive(serde::Deserialize)]
struct RootWorkspaceSection {
    package: RootWorkspacePackage,
    dependencies: RootWorkspaceDependencies,
}

#[derive(serde::Deserialize)]
struct RootWorkspacePackage {
    version: String,
}

#[derive(serde::Deserialize)]
struct RootWorkspaceDependencies {
    #[serde(rename = "devflow-core")]
    devflow_core: DevflowCoreDependency,
}

#[derive(serde::Deserialize)]
struct DevflowCoreDependency {
    version: String,
}

/// `devflow-core`'s pinned `version` in `[workspace.dependencies]` must equal
/// `[workspace.package].version` (a path dependency's `version` field does not
/// auto-inherit from the workspace, unlike ordinary `.workspace = true`
/// fields). This drifted twice by hand before the pin was consolidated into
/// a single root-Cargo.toml location — this test is the guard against a
/// third occurrence.
#[test]
fn devflow_core_dependency_pin_matches_workspace_version() {
    let contents = std::fs::read_to_string(workspace_root().join("Cargo.toml"))
        .expect("root Cargo.toml must be readable");
    let parsed: RootCargoToml = toml::from_str(&contents).expect("root Cargo.toml must parse");
    assert_eq!(
        parsed.workspace.dependencies.devflow_core.version, parsed.workspace.package.version,
        "workspace.dependencies.devflow-core's pinned version must match workspace.package.version"
    );
}

/// The `State` field 50-16 removed (R-9), assembled from pieces so this file
/// never holds the name contiguously and the scan below cannot flag itself.
const REMOVED_STATE_FIELD: &str = concat!("monitor", "_pid");

/// Lines that survive comment stripping, numbered from 1: a line whose trimmed
/// start is `//` (doc comments included) is dropped.
fn uncommented_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .map(|(index, line)| (index + 1, line))
}

fn is_identifier_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// True when `line`'s trimmed start is `pub ` plus `field`, followed by a
/// non-identifier character (or nothing): the declaration, not a longer name.
fn declares_pub_field(line: &str, field: &str) -> bool {
    line.trim_start()
        .strip_prefix("pub ")
        .and_then(|rest| rest.strip_prefix(field))
        .is_some_and(|after| {
            after
                .chars()
                .next()
                .is_none_or(|ch| !is_identifier_char(ch))
        })
}

/// True when `line` holds `field` wrapped in double quotes — a JSON or event
/// key — whether the closing quote is bare (`"k"`, raw strings, `json!`) or
/// escaped inside an ordinary string literal (`"{\"k\": 1}"`).
fn holds_quoted_key(line: &str, field: &str) -> bool {
    let opened = ["\"", field].concat();
    line.match_indices(&opened).any(|(start, _)| {
        let after = &line[start + opened.len()..];
        after.starts_with('"') || after.starts_with("\\\"")
    })
}

/// Every place the removed field survives (T-50-43): (a) its `pub`
/// declaration on an uncommented line of state.rs, (b) its quoted key on an
/// uncommented line of any Rust file, (c) any mention in a snapshot file.
fn removed_field_findings(
    state_source: &str,
    rust_files: &[(PathBuf, String)],
    snapshots: &[(PathBuf, String)],
) -> Vec<String> {
    let field = REMOVED_STATE_FIELD;
    let mut findings = Vec::new();
    for (number, line) in uncommented_lines(state_source) {
        if declares_pub_field(line, field) {
            findings.push(format!("state.rs:{number}: declaration: {}", line.trim()));
        }
    }
    for (path, text) in rust_files {
        for (number, line) in uncommented_lines(text) {
            if holds_quoted_key(line, field) {
                findings.push(format!(
                    "{}:{number}: quoted key: {}",
                    path.display(),
                    line.trim()
                ));
            }
        }
    }
    for (path, text) in snapshots {
        if text.contains(field) {
            findings.push(format!("{}: snapshot mention", path.display()));
        }
    }
    findings
}

fn read_labelled(paths: Vec<PathBuf>) -> Vec<(PathBuf, String)> {
    paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            (path, text)
        })
        .collect()
}

/// T-50-43: the socket is the only liveness source once `State`'s monitor
/// pid field is gone (R-9, 50-16). The compiler proves no Rust code reads or
/// writes the field; this scan covers what it cannot see — the declaration
/// itself, a JSON or event key of that name in either crate, and a mention in
/// the CLI's help snapshot. Planted controls prove each check decides.
#[test]
fn no_monitor_pid_field_json_key_or_help_mention_remains() {
    let field = REMOVED_STATE_FIELD;
    let none: &[(PathBuf, String)] = &[];
    let planted = |text: String| vec![(PathBuf::from("planted.rs"), text)];

    // (a) declaration: flagged; a longer name and a commented one are not.
    let declared = ["    pub ", field, ": Option<u32>,"].concat();
    assert_eq!(removed_field_findings(&declared, none, none).len(), 1);
    let longer = ["    pub ", field, "_launch: Option<u32>,"].concat();
    assert!(removed_field_findings(&longer, none, none).is_empty());
    let commented = ["    // pub ", field, ": Option<u32>,"].concat();
    let doc_commented = ["    /// pub ", field, ": Option<u32>,"].concat();
    assert!(removed_field_findings(&commented, none, none).is_empty());
    assert!(removed_field_findings(&doc_commented, none, none).is_empty());

    // (b) quoted key: flagged on a code line, bare or escaped.
    let raw_key = ["    let json = r#\"{\"", field, "\": 4242}\"#;"].concat();
    assert_eq!(removed_field_findings("", &planted(raw_key), none).len(), 1);
    let escaped_key = ["    let json = \"{\\\"", field, "\\\": 4242}\";"].concat();
    assert_eq!(
        removed_field_findings("", &planted(escaped_key), none).len(),
        1
    );
    // ...and not on a comment line, as a local, in a helper's name, or as a
    // prefix of a longer quoted string.
    for clean in [
        ["    // json.contains(\"", field, "\")"].concat(),
        ["    let ", field, " = 4242;"].concat(),
        ["    fn wait_for_", field, "_exit() {}"].concat(),
        ["    let message = \"", field, " must not appear\";"].concat(),
    ] {
        assert!(
            removed_field_findings("", &planted(clean.clone()), none).is_empty(),
            "planted clean line was flagged: {clean}"
        );
    }

    // (c) snapshot mention: flagged.
    let snapshot = vec![(
        PathBuf::from("planted-help.txt"),
        ["      --", field, "  <PID>\n"].concat(),
    )];
    assert_eq!(removed_field_findings("", none, &snapshot).len(), 1);

    // The real scan: state.rs, every Rust file under crates/, every snapshot.
    let root = workspace_root();
    let state_source = std::fs::read_to_string(root.join("crates/devflow-core/src/state.rs"))
        .expect("state.rs must be readable");
    let rust_files = read_labelled(rust_file_paths());
    assert!(!rust_files.is_empty(), "the walker must find Rust sources");
    let snapshot_dir = root.join("crates/devflow-cli/tests/snapshots");
    let mut snapshot_paths: Vec<PathBuf> = std::fs::read_dir(&snapshot_dir)
        .unwrap_or_else(|error| panic!("{} must exist: {error}", snapshot_dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    snapshot_paths.sort();
    assert!(
        !snapshot_paths.is_empty(),
        "{} must hold at least one snapshot",
        snapshot_dir.display()
    );
    let snapshots = read_labelled(snapshot_paths);

    let findings = removed_field_findings(&state_source, &rust_files, &snapshots);
    assert!(
        findings.is_empty(),
        "the removed State field survives (T-50-43):\n{}",
        findings.join("\n")
    );
}

/// `text` with every `<!--` … `-->` span removed (an unterminated one runs to
/// the end of the text), so a fragment hidden in an HTML comment never counts
/// as documented.
fn strip_html_comments(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("<!--") {
        kept.push_str(&rest[..open]);
        let after_open = &rest[open + "<!--".len()..];
        match after_open.find("-->") {
            Some(close) => rest = &after_open[close + "-->".len()..],
            None => return kept,
        }
    }
    kept.push_str(rest);
    kept
}

/// The `fragments` that `text` does not contain, in the order given.
fn missing_fragments<'a>(text: &str, fragments: &[&'a str]) -> Vec<&'a str> {
    fragments
        .iter()
        .copied()
        .filter(|fragment| !text.contains(fragment))
        .collect()
}

/// Fragments OPERATIONS.md must hold, compared as written (50-17).
const RUN_LIVENESS_FRAGMENTS: &[&str] = &[
    "GONE",
    "STALE",
    "ALIVE",
    "devflow stop --phase N",
    "devflow resume --phase N",
    "ps -o pid,pgid,sid,lstart,args -g",
    "DEVFLOW_CACHE_DIR",
];

/// Fragments OPERATIONS.md must hold, compared lowercased the way
/// `source_devflow_env_vars_and_subcommands_are_documented` compares (RV-06).
const INTERNAL_SUBCOMMAND_FRAGMENTS: &[&str] = &["devflow monitor", "devflow advance"];

/// 50-17 (R6-G4): OPERATIONS.md documents run liveness as the supervisor
/// socket answers it (GONE / STALE / ALIVE and the repair verbs), `devflow
/// stop`'s arms including the backstop's refusal and its inspection command,
/// the socket location's override, and both internal subcommands, outside
/// HTML comments.
#[test]
fn operations_md_documents_run_liveness_and_the_stop_arms() {
    let every: String = RUN_LIVENESS_FRAGMENTS.join("\n");
    let all_but_stale = every.replace("STALE", "");
    assert_eq!(
        missing_fragments(&all_but_stale, RUN_LIVENESS_FRAGMENTS),
        vec!["STALE"],
        "planted control: a text missing only STALE must report exactly STALE"
    );
    let gone_only_in_a_comment = every.replace("GONE", "<!-- GONE -->");
    assert!(
        missing_fragments(&gone_only_in_a_comment, RUN_LIVENESS_FRAGMENTS).is_empty(),
        "planted control: before stripping, the commented GONE still counts"
    );
    assert_eq!(
        missing_fragments(
            &strip_html_comments(&gone_only_in_a_comment),
            RUN_LIVENESS_FRAGMENTS
        ),
        vec!["GONE"],
        "planted control: a GONE only inside an HTML comment must be missing after stripping"
    );
    assert_eq!(
        strip_html_comments("kept <!-- dropped"),
        "kept ",
        "planted control: an unterminated comment runs to the end of the text"
    );
    assert!(
        missing_fragments(&every, RUN_LIVENESS_FRAGMENTS).is_empty(),
        "planted control: a text holding every fragment reports none"
    );

    let path = workspace_root().join("OPERATIONS.md");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let text = strip_html_comments(&text);
    let mut missing = missing_fragments(&text, RUN_LIVENESS_FRAGMENTS);
    missing.extend(missing_fragments(
        &text.to_ascii_lowercase(),
        INTERNAL_SUBCOMMAND_FRAGMENTS,
    ));
    assert!(
        missing.is_empty(),
        "{} does not document run liveness and the stop arms; missing outside HTML comments:\n{}",
        path.display(),
        missing.join("\n")
    );
}

/// The lines of `text` from the line equal to `heading` up to, not
/// including, the next line starting with `## `, or to the end. `None` when no
/// line equals `heading`.
fn markdown_section<'a>(text: &'a str, heading: &str) -> Option<&'a str> {
    let mut offset = 0;
    let mut start = None;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        match start {
            None if bare == heading => start = Some(offset),
            Some(begin) if bare.starts_with("## ") => return Some(&text[begin..offset]),
            _ => {}
        }
        offset += line.len();
    }
    start.map(|begin| &text[begin..])
}

/// True when `text`, with every whitespace run (line breaks included)
/// collapsed to one space and lowercased, describes a shell monitor or a
/// shell script — so a phrase wrapped across lines still counts.
fn mentions_shell_monitor(text: &str) -> bool {
    let collapsed = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    collapsed.contains("shell monitor") || collapsed.contains("shell script")
}

/// True when some line of `text`, lowercased, claims macOS support.
fn claims_macos_support(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.to_lowercase();
        line.contains("supports macos") || line.contains("macos is supported")
    })
}

/// What ARCHITECTURE.md's `## Monitor daemon` section gets wrong about the
/// supervisor (50-17, D-02/D-03): too short to describe it, not naming its
/// socket or its in-process advance, or still naming the forked `advance`
/// command or a `sh -c` runner.
fn monitor_section_problems(section: &str) -> Vec<String> {
    const MIN_LINES: usize = 8;
    let mut problems = Vec::new();
    let lines = section.lines().count();
    if lines < MIN_LINES {
        problems.push(format!("spans {lines} lines, fewer than {MIN_LINES}"));
    }
    for required in ["in-process", "socket"] {
        if !section.contains(required) {
            problems.push(format!("does not name `{required}`"));
        }
    }
    for forbidden in ["devflow advance", "sh -c"] {
        if section.contains(forbidden) {
            problems.push(format!("still names `{forbidden}`"));
        }
    }
    problems
}

/// Every file under `dir`, recursively, skipping hidden entries and
/// `skip_dir`, in sorted order.
fn files_under(dir: &Path, skip_dir: &Path) -> Vec<PathBuf> {
    fn visit(dir: &Path, skip_dir: &Path, files: &mut Vec<PathBuf>) {
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|error| panic!("failed to list {}: {error}", dir.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') || path == skip_dir {
                continue;
            }
            if path.is_dir() {
                visit(&path, skip_dir, files);
            } else {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    visit(dir, skip_dir, &mut files);
    files.sort();
    files
}

/// A doc's text through `workspace_root()`, HTML comments stripped; panics
/// naming the path when it cannot be read.
fn read_doc(relative: &str) -> String {
    let path = workspace_root().join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    strip_html_comments(&text)
}

/// 50-17 (R6-G4, T-50-44, T-50-66): the docs describe the socket supervisor
/// as built and claim nothing unestablished. OPERATIONS.md carries the macOS
/// caveat and D-20's install rule; no operator doc claims macOS support;
/// ARCHITECTURE.md's `## Monitor daemon` section describes the socket and the
/// in-process advance, not a forked `advance` command; the unattended-mode
/// guide gives the capture shape, not a shell monitor, as the reason for its
/// refusals.
#[test]
fn docs_describe_the_socket_supervisor_not_the_shell_monitor() {
    // Planted controls, through the same helpers as the real checks.
    let two_sections = "# Title\n## First\none\ntwo\n## Second\nthree\n";
    assert_eq!(
        markdown_section(two_sections, "## First"),
        Some("## First\none\ntwo\n")
    );
    assert_eq!(markdown_section(two_sections, "## Absent"), None);
    let planted_section = [
        "## Monitor daemon",
        "",
        "The supervisor publishes a socket and advances in-process.",
        "",
        "Then it runs `devflow advance <project> --phase N`.",
        "",
        "It owns the agent.",
        "",
        "It answers ping and shutdown.",
    ]
    .join("\n");
    assert_eq!(
        monitor_section_problems(&planted_section),
        vec!["still names `devflow advance`".to_string()],
        "planted control: a section naming the forked advance must fail"
    );
    assert!(mentions_shell_monitor("a detached shell\n  monitor"));
    assert!(!mentions_shell_monitor("the capture shape"));
    assert!(claims_macos_support("DevFlow supports macOS."));
    assert!(!claims_macos_support(
        "Socket supervision is unverified on macOS"
    ));

    let operations = read_doc("OPERATIONS.md");
    for claim in ["unverified on macOS", "only while no 2.x run is"] {
        assert!(
            operations.contains(claim),
            "OPERATIONS.md does not state `{claim}`"
        );
    }

    let root = workspace_root();
    let mut operator_docs = vec![(root.join("OPERATIONS.md"), operations.clone())];
    for path in files_under(&root.join("docs"), &root.join("docs/site")) {
        // A binary asset cannot describe the monitor.
        if let Ok(text) = std::fs::read_to_string(&path) {
            operator_docs.push((path, strip_html_comments(&text)));
        }
    }
    assert!(
        operator_docs.len() > 1,
        "the docs/ walk must find files to scan"
    );
    for (path, text) in &operator_docs {
        assert!(
            !claims_macos_support(text),
            "{} claims macOS support, which no macOS host or CI has verified",
            path.display()
        );
    }

    let architecture = read_doc("ARCHITECTURE.md");
    let section = markdown_section(&architecture, "## Monitor daemon")
        .unwrap_or_else(|| panic!("ARCHITECTURE.md has no `## Monitor daemon` section"));
    let problems = monitor_section_problems(section);
    assert!(
        problems.is_empty(),
        "ARCHITECTURE.md's `## Monitor daemon` section does not describe the socket supervisor:\n{}",
        problems.join("\n")
    );

    let unattended = read_doc("docs/guides/unattended-mode.md");
    assert!(
        unattended.contains("capture shape"),
        "docs/guides/unattended-mode.md does not name the capture shape"
    );
    assert!(
        !mentions_shell_monitor(&unattended),
        "docs/guides/unattended-mode.md still describes a shell monitor or shell script"
    );
}
