//! Deterministic operator-documentation and runtime-path invariants.

use std::path::{Path, PathBuf};

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
    read_files(files)
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
            agent_result::stdout_path(root, 16),
        ),
        (
            "agent_result::stderr_path",
            agent_result::stderr_path(root, 16),
        ),
        (
            "agent_result::exit_code_path",
            agent_result::exit_code_path(root, 16),
        ),
        (
            "agent_result::agent_pid_path",
            agent_result::agent_pid_path(root, 16),
        ),
        (
            "agent_result::history_dir",
            agent_result::history_dir(root, 16),
        ),
        ("workflow::state_path", workflow::state_path(root, 16)),
        (
            "workflow::legacy_state_path",
            workflow::legacy_state_path(root),
        ),
        ("Gates::dir", Gates::dir(root)),
        ("lock::lock_path", lock::lock_path(root, 16)),
        ("lock::project_lock_path", lock::project_lock_path(root)),
        (
            "ship::cron_instructions_path",
            ship::cron_instructions_path(root, 16),
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
