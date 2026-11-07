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
        ("Gates::dir", Gates::dir(root)),
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
    "ps -o pid,pgid,sid,lstart,args -p \"$(pgrep -d, -g",
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

/// The helper removed with the rebase handoff (D-14), assembled from pieces
/// so this file's code never holds it contiguously.
const REMOVED_HELPER: &str = concat!("rebase", "_in");

/// The removed verb's long spelling, assembled the same way.
const REMOVED_VERB: &str = concat!("sequent", "agent");

/// Both spellings criterion 4 names for the removed verb.
const REMOVED_VERB_SPELLINGS: [&str; 2] = [REMOVED_VERB, "n"];

/// The `#[command(...)]` keys whose value is a name the subcommand answers to.
const COMMAND_NAME_KEYS: &[&str] = &[
    "name",
    "alias",
    "visible_alias",
    "aliases",
    "visible_aliases",
];

/// The literal starting at `chars[start]`, as `(opening delimiter length, end
/// index exclusive, closing delimiter length)`: a `"…"` string (with `\`
/// escapes), an `r"…"` / `r#"…"#` raw string, or a char literal such as
/// `'"'` or `'\''`. A lifetime or label (`'a`) is not a literal.
fn literal_at(chars: &[char], start: usize) -> Option<(usize, usize, usize)> {
    let at = |index: usize| chars.get(index).copied();
    match chars[start] {
        '"' => {
            let mut index = start + 1;
            while index < chars.len() {
                match chars[index] {
                    '\\' => index += 2,
                    '"' => return Some((1, index + 1, 1)),
                    _ => index += 1,
                }
            }
            Some((1, chars.len(), 0))
        }
        'r' => {
            let starts_token = |index: usize| index == 0 || !is_identifier_char(chars[index - 1]);
            let byte_prefixed = start > 0 && chars[start - 1] == 'b' && starts_token(start - 1);
            if !starts_token(start) && !byte_prefixed {
                return None;
            }
            let hashes = chars[start + 1..]
                .iter()
                .take_while(|&&ch| ch == '#')
                .count();
            if at(start + 1 + hashes) != Some('"') {
                return None;
            }
            let open = 2 + hashes;
            let mut index = start + open;
            while index < chars.len() {
                if chars[index] == '"'
                    && chars[index + 1..]
                        .iter()
                        .take(hashes)
                        .filter(|&&ch| ch == '#')
                        .count()
                        == hashes
                {
                    return Some((open, index + 1 + hashes, 1 + hashes));
                }
                index += 1;
            }
            Some((open, chars.len(), 0))
        }
        '\'' => match at(start + 1) {
            Some('\\') => {
                let close = chars[start + 3..]
                    .iter()
                    .position(|&ch| ch == '\'')
                    .map(|offset| start + 3 + offset)?;
                Some((1, close + 1, 1))
            }
            Some(ch) if ch != '\'' && at(start + 2) == Some('\'') => Some((1, start + 3, 1)),
            _ => None,
        },
        _ => None,
    }
}

/// The end (exclusive) of the block comment opening at `chars[start]`.
/// Rust block comments nest; an unterminated one runs to the end.
fn block_comment_end(chars: &[char], start: usize) -> usize {
    let mut depth = 0_usize;
    let mut index = start;
    while index < chars.len() {
        match (chars[index], chars.get(index + 1)) {
            ('/', Some('*')) => {
                depth += 1;
                index += 2;
            }
            ('*', Some('/')) => {
                depth -= 1;
                index += 2;
                if depth == 0 {
                    return index;
                }
            }
            _ => index += 1,
        }
    }
    chars.len()
}

/// One pass over Rust source that recognises comments and literals. Comments
/// are dropped when `drop_comments` (a block comment leaves a space and its
/// newlines); literal contents are blanked when `blank_literals`, each char
/// replaced by spaces of the same byte length so offsets still line up with
/// the input. Newlines always survive.
fn rewrite_rust(text: &str, drop_comments: bool, blank_literals: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        let next = chars.get(index + 1).copied();
        if chars[index] == '/' && next == Some('/') {
            let end = chars[index..]
                .iter()
                .position(|&ch| ch == '\n')
                .map_or(chars.len(), |offset| index + offset);
            if !drop_comments {
                out.extend(&chars[index..end]);
            }
            index = end;
        } else if chars[index] == '/' && next == Some('*') {
            let end = block_comment_end(&chars, index);
            if drop_comments {
                out.push(' ');
                out.extend(chars[index..end].iter().filter(|&&ch| ch == '\n'));
            } else {
                out.extend(&chars[index..end]);
            }
            index = end;
        } else if let Some((open, end, close)) = literal_at(&chars, index) {
            out.extend(&chars[index..index + open]);
            for &ch in &chars[index + open..end - close] {
                if !blank_literals || ch == '\n' {
                    out.push(ch);
                } else {
                    out.extend(std::iter::repeat_n(' ', ch.len_utf8()));
                }
            }
            out.extend(&chars[end - close..end]);
            index = end;
        } else {
            out.push(chars[index]);
            index += 1;
        }
    }
    out
}

/// `text` with every `//` line comment (doc comments included) and every
/// `/* … */` block comment removed. String and char literals are kept whole,
/// so a `//` inside one is not a comment. Newlines survive, so line numbers
/// still match the input.
fn strip_rust_comments(text: &str) -> String {
    rewrite_rust(text, true, false)
}

/// `text` with the contents of every string and char literal blanked, so a
/// quoted spelling never counts as code. Byte offsets match the input, so a
/// position found here slices the same span of `text`.
fn strip_string_literal_contents(text: &str) -> String {
    rewrite_rust(text, false, true)
}

/// Comment-stripped `text` and its literal-blanked twin, byte-aligned.
fn code_and_mask(text: &str) -> (String, String) {
    let code = strip_rust_comments(text);
    let mask = strip_string_literal_contents(&code);
    (code, mask)
}

/// The 1-based line holding byte `index` of `text`.
fn line_at(text: &str, index: usize) -> usize {
    text[..index].matches('\n').count() + 1
}

/// The index of the delimiter matching the opener at `bytes[open]`, counting
/// every `(`, `[` and `{` pair. Run over literal-blanked text, so a delimiter
/// inside a literal is never counted.
fn matching_close(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0_usize;
    for (index, &byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// The `[start, end)` ranges of `mask[start..end]` split at `separator`
/// bytes that sit outside any `()`, `[]` or `{}` nesting.
fn split_top_level(mask: &str, start: usize, end: usize, separator: u8) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut depth = 0_usize;
    let mut piece = start;
    for (index, &byte) in mask.as_bytes()[start..end].iter().enumerate() {
        let index = start + index;
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            byte if byte == separator && depth == 0 => {
                ranges.push((piece, index));
                piece = index + 1;
            }
            _ => {}
        }
    }
    ranges.push((piece, end));
    ranges
}

/// The outer attributes (`#[…]`, as written in `mask`) directly before the
/// item whose keyword starts at `item_start`, after any visibility.
fn outer_attributes_before(mask: &str, item_start: usize) -> Vec<&str> {
    let mut rest = mask[..item_start].trim_end();
    if rest.ends_with(')') {
        let bytes = rest.as_bytes();
        let open = (0..bytes.len()).rev().find(|&index| {
            bytes[index] == b'(' && matching_close(bytes, index) == Some(bytes.len() - 1)
        });
        if let Some(open) = open
            && rest[..open].trim_end().ends_with("pub")
        {
            rest = rest[..open].trim_end();
        }
    }
    if let Some(before) = rest.strip_suffix("pub")
        && before
            .chars()
            .next_back()
            .is_none_or(|ch| !is_identifier_char(ch))
    {
        rest = before.trim_end();
    }
    let mut attributes = Vec::new();
    while rest.ends_with(']') {
        let bytes = rest.as_bytes();
        let Some(open) = (0..bytes.len()).rev().find(|&index| {
            bytes[index] == b'[' && matching_close(bytes, index) == Some(bytes.len() - 1)
        }) else {
            break;
        };
        let before = rest[..open].trim_end();
        let Some(hash) = before.strip_suffix('#') else {
            break;
        };
        attributes.push(&rest[hash.len()..]);
        rest = hash.trim_end();
    }
    attributes
}

/// True when `attribute` (`#[…]`) is a `derive(…)` list naming `Subcommand`,
/// bare or path-qualified.
fn derives_subcommand(attribute: &str) -> bool {
    let body = attribute
        .trim_start_matches('#')
        .trim_start()
        .trim_start_matches('[')
        .trim_start();
    body.strip_prefix("derive")
        .map(str::trim_start)
        .and_then(|rest| rest.strip_prefix('('))
        .is_some_and(|list| {
            list.trim_end_matches(']')
                .trim_end()
                .trim_end_matches(')')
                .split(',')
                .any(|item| item.trim().rsplit("::").next() == Some("Subcommand"))
        })
}

/// The variants directly inside the enum body opening at `mask[open]`, as
/// `(name as written, line)`. Attributes, fields and discriminants nest
/// deeper and are skipped.
fn enum_body_variants(mask: &str, open: usize) -> Vec<(String, usize)> {
    let bytes = mask.as_bytes();
    let mut variants = Vec::new();
    let mut depth = 0_usize;
    let mut expect_variant = true;
    let mut index = open + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            b',' if depth == 0 => expect_variant = true,
            byte if depth == 0
                && expect_variant
                && (byte.is_ascii_alphabetic() || byte == b'_') =>
            {
                let end = mask[index..]
                    .find(|ch: char| !is_identifier_char(ch))
                    .map_or(mask.len(), |offset| index + offset);
                variants.push((mask[index..end].to_owned(), line_at(mask, index)));
                expect_variant = false;
                index = end;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    variants
}

/// Every enum in `mask` (comment-stripped, literal-blanked) whose outer
/// attributes derive `Subcommand`, as `(name, variants)`.
fn subcommand_enums(mask: &str) -> Vec<(String, Vec<(String, usize)>)> {
    let mut enums = Vec::new();
    for (start, _) in mask.match_indices("enum") {
        let before = mask[..start].chars().next_back();
        let rest = &mask[start + "enum".len()..];
        if before.is_some_and(is_identifier_char) || !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let name_start = start + "enum".len() + (rest.len() - rest.trim_start().len());
        let name: String = mask[name_start..]
            .chars()
            .take_while(|&ch| is_identifier_char(ch))
            .collect();
        let Some(open) = mask[name_start..]
            .find('{')
            .map(|offset| name_start + offset)
        else {
            continue;
        };
        if name.is_empty()
            || mask[name_start + name.len()..open].contains(';')
            || !outer_attributes_before(mask, start)
                .into_iter()
                .any(derives_subcommand)
        {
            continue;
        }
        enums.push((name, enum_body_variants(mask, open)));
    }
    enums
}

/// The lowercased name of every enum in `source` that derives `Subcommand`.
fn subcommand_enum_names(source: &str) -> Vec<String> {
    let (_, mask) = code_and_mask(source);
    subcommand_enums(&mask)
        .into_iter()
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect()
}

/// Every identifier-boundary use of the removed helper (definition or call)
/// in comment-stripped, literal-blanked code.
fn rebase_in_findings(files: &[(PathBuf, String)]) -> Vec<String> {
    let token = REMOVED_HELPER;
    let mut findings = Vec::new();
    for (path, text) in files {
        let (_, mask) = code_and_mask(text);
        for (index, _) in mask.match_indices(token) {
            let before = mask[..index].chars().next_back();
            let after = mask[index + token.len()..].chars().next();
            if before.is_none_or(|ch| !is_identifier_char(ch))
                && after.is_none_or(|ch| !is_identifier_char(ch))
            {
                findings.push(format!(
                    "{}:{}: `{token}` in code",
                    path.display(),
                    line_at(&mask, index)
                ));
            }
        }
    }
    findings
}

/// Every `Subcommand` enum variant whose lowercased name is a removed spelling.
fn removed_verb_findings(cli_files: &[(PathBuf, String)]) -> Vec<String> {
    let mut findings = Vec::new();
    for (path, text) in cli_files {
        let (_, mask) = code_and_mask(text);
        for (enum_name, variants) in subcommand_enums(&mask) {
            for (variant, line) in variants {
                let lowered = variant.to_ascii_lowercase();
                if REMOVED_VERB_SPELLINGS.contains(&lowered.as_str()) {
                    findings.push(format!(
                        "{}:{line}: subcommand variant `{enum_name}::{variant}`",
                        path.display()
                    ));
                }
            }
        }
    }
    findings
}

/// The contents of `value` when it is one whole string literal (`"…"` or a
/// raw `r#"…"#`), else `None`.
fn string_literal_value(value: &str) -> Option<&str> {
    let unprefixed = value.strip_prefix('r').unwrap_or(value);
    let hashes = unprefixed.len() - unprefixed.trim_start_matches('#').len();
    let closing = "#".repeat(hashes);
    unprefixed[hashes..]
        .strip_suffix(closing.as_str())?
        .strip_prefix('"')?
        .strip_suffix('"')
}

/// The byte index just past `#[command(` when one opens at `mask[start]`,
/// whitespace tolerated between the tokens.
fn command_attribute_body_start(mask: &str, start: usize) -> Option<usize> {
    let after_hash = mask[start..].strip_prefix('#')?.trim_start();
    let after_bracket = after_hash.strip_prefix('[')?.trim_start();
    let after_name = after_bracket.strip_prefix("command")?;
    if after_name.starts_with(is_identifier_char) {
        return None;
    }
    let after_paren = after_name.trim_start().strip_prefix('(')?;
    Some(mask.len() - after_paren.len())
}

/// Every `#[command(...)]` attribute whose `name` or alias value equals a
/// removed spelling: a string value, or one element of a bracketed list,
/// compared whole after unquoting, never by substring. The body may span
/// lines; the finding names the line the attribute opens on. `#[arg(...)]`
/// is not a command name and is not read.
fn removed_verb_attr_findings(cli_files: &[(PathBuf, String)]) -> Vec<String> {
    let mut findings = Vec::new();
    for (path, text) in cli_files {
        let (code, mask) = code_and_mask(text);
        for (start, _) in mask.match_indices('#') {
            let Some(body_start) = command_attribute_body_start(&mask, start) else {
                continue;
            };
            let Some(body_end) = matching_close(mask.as_bytes(), body_start - 1) else {
                continue;
            };
            for (pair_start, pair_end) in split_top_level(&mask, body_start, body_end, b',') {
                let Some(equals) = mask[pair_start..pair_end]
                    .find('=')
                    .map(|at| pair_start + at)
                else {
                    continue;
                };
                let key = mask[pair_start..equals].trim();
                if !COMMAND_NAME_KEYS.contains(&key) {
                    continue;
                }
                let value_mask = &mask[equals + 1..pair_end];
                let value_start = equals + 1 + (value_mask.len() - value_mask.trim_start().len());
                let value_end = equals + 1 + value_mask.trim_end().len();
                let elements = if mask[value_start..value_end].starts_with('[')
                    && mask[value_start..value_end].ends_with(']')
                {
                    split_top_level(&mask, value_start + 1, value_end - 1, b',')
                } else {
                    vec![(value_start, value_end)]
                };
                for (element_start, element_end) in elements {
                    let Some(value) = string_literal_value(code[element_start..element_end].trim())
                    else {
                        continue;
                    };
                    if REMOVED_VERB_SPELLINGS.contains(&value) {
                        findings.push(format!(
                            "{}:{}: `#[command({key} = …)]` names removed verb `{value}`",
                            path.display(),
                            line_at(&mask, start)
                        ));
                    }
                }
            }
        }
    }
    findings
}

/// All three checks: the helper over every crate source, the verb over the
/// CLI sources.
fn removed_handoff_findings(
    files: &[(PathBuf, String)],
    cli_files: &[(PathBuf, String)],
) -> Vec<String> {
    let mut findings = rebase_in_findings(files);
    findings.extend(removed_verb_findings(cli_files));
    findings.extend(removed_verb_attr_findings(cli_files));
    findings
}

/// Criterion 4 (D-14): the rebase handoff removed from issue #180 stays dead.
///
/// History: the removed verb was `Command::Sequentagent` (dbab01a), and the
/// helper was `GitFlow::rebase_in` (0833a6c). No `n` verb ever existed under
/// that spelling (`git log -G` finds none), but criterion 4 names it, so both
/// readings are covered.
///
/// The scan reads declarations, not free text: a variant of any enum that
/// derives `Subcommand`, and the `name` and alias keys of any
/// `#[command(...)]` attribute, compared whole against each spelling. Free
/// text would count the doc comments and messages that legitimately mention
/// the history, this one included. Comments are stripped before any count, so
/// this doc comment may name the tokens.
#[test]
fn the_removed_sequentagent_handoff_stays_dead() {
    let helper = REMOVED_HELPER;
    let verb = REMOVED_VERB;
    let capital_verb = format!("S{}", &verb[1..]);
    let command = "Command";
    let planted = |text: &str| vec![(PathBuf::from("planted.rs"), text.to_owned())];
    let findings = |text: &str| removed_handoff_findings(&planted(text), &planted(text));

    // Planted positives: exactly one finding each.
    for positive in [
        format!("fn {helper}(base: &str) {{}}"),
        format!("    git.{helper}(base)?;"),
        format!(
            "#[derive(Debug, Subcommand)]\nenum {command} {{ {capital_verb} {{ phase: u32 }}, Start }}"
        ),
        format!("#[derive(Debug, Subcommand)]\nenum {command} {{ N, Start }}"),
        format!(
            "#[derive(Subcommand)]\nenum {command} {{ #[command(name = \"{verb}\")] Handoff, Start }}"
        ),
        "#[command(alias = \"n\")]\nStart,".to_owned(),
        // Every other declaration route, both spellings.
        "#[command(alias=\"n\")]\nStart,".to_owned(),
        "#[command(visible_alias = \"n\")]\nStart,".to_owned(),
        "#[command(name = \"n\")]\nStart,".to_owned(),
        "#[command(aliases = [\"x\", \"n\"])]\nStart,".to_owned(),
        "#[command(aliases = [\"nn\", \"n\"])]\nStart,".to_owned(),
        format!("#[command(alias = \"{verb}\")]\nStart,"),
        format!("#[command(visible_aliases = [\"{verb}\"])]\nStart,"),
        // A second subcommand enum: the scan is not limited to `Command`.
        "#[derive(Debug, Subcommand)]\nenum GateCmd { List, N }".to_owned(),
        // A `//` inside a literal is not a comment, so the call after it counts.
        format!("let s = \"// x\"; git.{helper}(base)?;"),
    ] {
        let found = findings(&positive);
        assert_eq!(
            found.len(),
            1,
            "planted positive {positive:?} gave {found:?}"
        );
    }

    // Planted decoys: no finding.
    for decoy in [
        format!("/// {helper}"),
        format!("x(); // {helper}"),
        format!("/* {helper} */"),
        format!("assert!(!s.contains(\"{verb}\"));"),
        "let hint = \"git rebase origin/develop\";".to_owned(),
        "#[derive(Debug)]\nenum Axis { N, S }".to_owned(),
        "#[arg(short = 'n')]\nforce: bool,".to_owned(),
        "#[arg(name = \"n\")]\ncount: u32,".to_owned(),
        format!("let s = \"// {helper}\"; let t = s.len();"),
        format!("/* multi-line\n {helper}\n */\nfn kept() {{}}"),
        // Exact element equality: a value or element that merely contains a
        // spelling is not that spelling.
        format!(
            "#[derive(Debug, Subcommand)]\nenum {command} {{\n    #[command(name = \"__monitor\", hide = true)]\n    Monitor,\n    Start,\n}}"
        ),
        "#[command(alias = \"nx\")]\nStart,".to_owned(),
        "#[command(aliases = [\"nn\", \"x\"])]\nStart,".to_owned(),
        format!("#[command(name = \"{verb}-old\")]\nStart,"),
    ] {
        let found = findings(&decoy);
        assert!(found.is_empty(), "planted decoy {decoy:?} gave {found:?}");
    }

    // A variant and an attribute in one enum: two findings.
    let combined = format!(
        "#[derive(Subcommand)]\nenum {command} {{ N, #[command(name = \"{verb}\")] Handoff }}"
    );
    let found = findings(&combined);
    assert_eq!(found.len(), 2, "combined enum gave {found:?}");

    // Multi-line attributes (the real `#[command(` shape): one finding each,
    // reported at the line the attribute opens on.
    for multi_line in [
        "Start,\n#[command(\n    alias = \"n\",\n)]\nProbe,".to_owned(),
        format!("Start,\n#[command(\n    name = \"{verb}\",\n    about = \"x\"\n)]\nProbe,"),
    ] {
        let found = findings(&multi_line);
        assert_eq!(found.len(), 1, "multi-line {multi_line:?} gave {found:?}");
        assert!(
            found[0].starts_with("planted.rs:2:"),
            "multi-line {multi_line:?} reported the wrong line: {found:?}"
        );
    }

    // The stripper keeps a `//` inside a literal and drops a trailing comment.
    let url = strip_rust_comments("let u = \"https://example.invalid\";");
    assert!(url.contains("\"https://example.invalid\""), "{url:?}");
    let trailing = strip_rust_comments("let a = 1; // note");
    assert!(
        trailing.contains("let a = 1;") && !trailing.contains("note"),
        "{trailing:?}"
    );

    // The real tree: every crate source, and every subcommand enum in the CLI.
    let files = read_labelled(rust_file_paths());
    assert!(
        !files.is_empty(),
        "the walker must find Rust sources under crates/"
    );
    let cli_root = workspace_root().join("crates/devflow-cli/src");
    let cli_files: Vec<_> = files
        .iter()
        .filter(|(path, _)| path.starts_with(&cli_root))
        .cloned()
        .collect();
    assert!(
        !cli_files.is_empty(),
        "the walker must find Rust sources under crates/devflow-cli/src/"
    );
    let enum_names: Vec<String> = cli_files
        .iter()
        .flat_map(|(_, text)| subcommand_enum_names(text))
        .collect();
    for expected in ["command", "gatecmd"] {
        assert!(
            enum_names.iter().any(|name| name == expected),
            "the subcommand-enum finder missed `{expected}`: {enum_names:?}"
        );
    }
    let command_variants: usize = cli_files
        .iter()
        .flat_map(|(_, text)| subcommand_enums(&code_and_mask(text).1))
        .filter(|(name, _)| name == command)
        .map(|(_, variants)| variants.len())
        .sum();
    assert!(
        command_variants > 5,
        "only {command_variants} `Command` variants were read"
    );

    let found = removed_handoff_findings(&files, &cli_files);
    assert!(
        found.is_empty(),
        "the removed rebase handoff is back (D-14):\n{}",
        found.join("\n")
    );
}

/// RED stub: the scan's contract is pinned by
/// `no_environment_variable_selects_the_fallback_agent`.
fn fallback_env_name_findings(_sources: &[(PathBuf, String)]) -> Vec<String> {
    Vec::new()
}

/// A planted `DEVFLOW_`-prefixed name containing FALLBACK, split inside the
/// word so this file never holds an env-shaped piece containing it (R2-11).
const PLANTED_PREFIXED_NAME: &str = concat!("DEVFLOW_X_FALL", "BACK_AGENT");

/// A planted unprefixed name containing FALLBACK, split the same way.
const PLANTED_UNPREFIXED_NAME: &str = concat!("X_FALL", "BACK_MODE");

/// R-3 (Phase 51): no environment variable selects the fallback agent. Over
/// the comment-stripped source of every `.rs` file under `crates/` (tests
/// included), no string literal is an environment-variable-shaped name (an
/// ASCII uppercase letter, then only uppercase letters, digits and `_`)
/// containing `FALLBACK`, whatever its prefix. Matching literals rather than
/// call sites reaches a name held in a `const` and passed by identifier, the
/// form most of this tree's environment reads use; `source_read_env_vars`
/// sees only `DEVFLOW_` names written at the call.
///
/// Limit: a name assembled at runtime (`format!`, `concat!`) is not seen;
/// this test's own planted names are built that way so the real-tree pass
/// cannot report them.
#[test]
fn no_environment_variable_selects_the_fallback_agent() {
    let planted = |text: String| vec![(PathBuf::from("planted.rs"), text)];

    // Planted controls: a prefixed name read by literal, and an unprefixed
    // name held in a `const` and read through it, are each reported once.
    let by_literal = format!("fn f() {{ let _ = std::env::var(\"{PLANTED_PREFIXED_NAME}\"); }}\n");
    assert_eq!(
        fallback_env_name_findings(&planted(by_literal)).len(),
        1,
        "a literal read of a FALLBACK name is reported"
    );
    let by_const = format!(
        "const E: &str = \"{PLANTED_UNPREFIXED_NAME}\";\nfn f() {{ let _ = std::env::var_os(E); }}\n"
    );
    let found = fallback_env_name_findings(&planted(by_const));
    assert_eq!(
        found.len(),
        1,
        "a const-held FALLBACK name is reported: {found:?}"
    );
    assert!(
        found[0].contains("planted.rs:1"),
        "a finding names the file and line: {found:?}"
    );

    // Decoys: the lowercase TOML key, `ship.rs`'s `FALLBACK` identifier, and
    // a quoted name inside a `//` comment are not reported.
    for decoy in [
        "fn f() { let key = \"fallback_agent\"; }\n".to_string(),
        "const FALLBACK: &str = \"- No changes recorded.\";\n".to_string(),
        format!("// let _ = std::env::var(\"{PLANTED_PREFIXED_NAME}\");\nfn f() {{}}\n"),
    ] {
        assert!(
            fallback_env_name_findings(&planted(decoy.clone())).is_empty(),
            "decoy reported: {decoy}"
        );
    }

    // R2-11 self-check: this file's own pieces are not findings, so the
    // real-tree assertion below cannot be failed by this test.
    let this_file = workspace_root().join("crates/devflow-core/src/doc_check.rs");
    let own = fallback_env_name_findings(&read_labelled(vec![this_file]));
    assert!(own.is_empty(), "doc_check.rs reports itself: {own:?}");

    let sources = read_labelled(rust_file_paths());
    assert!(
        sources.len() > 1,
        "the walker read {} Rust sources",
        sources.len()
    );
    let found = fallback_env_name_findings(&sources);
    assert!(
        found.is_empty(),
        "an environment-variable name containing FALLBACK exists (R-3):\n{}",
        found.join("\n")
    );
}
