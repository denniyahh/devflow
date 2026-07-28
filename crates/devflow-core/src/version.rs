//! Hybrid Git-based SemVer.
//!
//! DevFlow derives the version entirely from git history (D-11) — the
//! version file (`Cargo.toml`, `pyproject.toml`, or `package.json`) is no
//! longer an input to [`compute_version`], only an output [`write_version`]
//! produces:
//!
//! - **Baseline** — the highest semver tag reachable from `HEAD`
//!   ([`reachable_semver_baseline`], D-07). If the highest semver tag in the
//!   repository overall is NOT reachable from `HEAD`, `compute_version`
//!   refuses rather than silently falling back to a smaller reachable tag
//!   (D-10).
//! - **Bump** — classified from the conventional-commit intent of the
//!   commits added since that baseline was released
//!   ([`classify_range_bump`], D-08), over a range anchored by
//!   [`release_range_start`] to survive this repository's squash-merge +
//!   sync-back release topology.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A semantic version, whether read from disk or computed from git history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    /// Major version component.
    pub major: u32,
    /// Minor version component.
    pub minor: u32,
    /// Patch version component.
    pub patch: u32,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Errors produced by version operations.
#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    /// Filesystem operation failed.
    #[error("version file I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// Version field could not be found or parsed.
    #[error("version parse failed: {0}")]
    Parse(String),
    /// A git command failed.
    #[error("git command failed: {0}")]
    Git(String),
}

/// Detect the project's version file, checking Cargo.toml, then pyproject.toml,
/// then package.json. Returns the first that exists.
pub fn detect_version_file(project_root: &Path) -> Option<PathBuf> {
    for name in ["Cargo.toml", "pyproject.toml", "package.json"] {
        let path = project_root.join(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// The dotted field path that holds the version in a given file.
fn field_for(path: &Path, contents: &str) -> &'static str {
    match path.file_name().and_then(|n| n.to_str()) {
        Some("Cargo.toml") => {
            if contents.contains("[workspace.package]") {
                "workspace.package.version"
            } else {
                "package.version"
            }
        }
        Some("pyproject.toml") => "project.version",
        Some("package.json") => "version",
        _ => "version",
    }
}

/// Read the MAJOR version component from a version file.
pub fn read_major_version(path: &Path) -> Result<u32, VersionError> {
    let contents = std::fs::read_to_string(path)?;
    let field = field_for(path, &contents);
    let version = find_version_in_contents(&contents, field)
        .ok_or_else(|| VersionError::Parse(format!("field `{field}` not found in {path:?}")))?;
    let major = version
        .split(['.', '+', '-'])
        .next()
        .unwrap_or("0")
        .parse::<u32>()
        .map_err(|err| VersionError::Parse(format!("invalid major in `{version}`: {err}")))?;
    Ok(major)
}

/// Count all git tags (the MINOR component).
pub fn count_git_tags(project_root: &Path) -> Result<u32, VersionError> {
    let output = Command::new("git")
        .arg("tag")
        .current_dir(project_root)
        .output()
        .map_err(|err| VersionError::Git(err.to_string()))?;
    if !output.status.success() {
        return Err(VersionError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let count = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    Ok(count as u32)
}

/// Count commits since the most recent tag (the PATCH component). If there are
/// no tags yet, counts all commits reachable from HEAD.
pub fn commits_since_last_minor_tag(project_root: &Path) -> Result<u32, VersionError> {
    let last_tag = Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .current_dir(project_root)
        .output()
        .map_err(|err| VersionError::Git(err.to_string()))?;

    let range = if last_tag.status.success() {
        let tag = String::from_utf8_lossy(&last_tag.stdout).trim().to_string();
        format!("{tag}..HEAD")
    } else {
        "HEAD".to_string()
    };

    let output = Command::new("git")
        .args(["rev-list", "--count", &range])
        .current_dir(project_root)
        .output()
        .map_err(|err| VersionError::Git(err.to_string()))?;
    if !output.status.success() {
        // No commits yet (e.g. empty repo) → zero patch.
        return Ok(0);
    }
    let count = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .unwrap_or(0);
    Ok(count)
}

/// Enumerate every tag in the repository (no reachability restriction), keep
/// only values that parse as `vMAJOR.MINOR.PATCH` semver (a leading `v` is
/// stripped first — the `semver` crate's grammar is bare `MAJOR.MINOR.PATCH`),
/// and return the maximum by semver ordering (D-07). A stray non-semver tag
/// (e.g. this repository's `archive-planning-docs-2026-07-24`) is silently
/// excluded via `filter_map(...ok())` rather than erroring — a malformed tag
/// can never crash this path (T-25-02).
pub fn highest_semver_tag(project_root: &Path) -> Result<Option<semver::Version>, VersionError> {
    let output = Command::new("git")
        .arg("tag")
        .current_dir(project_root)
        .output()
        .map_err(|err| VersionError::Git(err.to_string()))?;
    if !output.status.success() {
        return Err(VersionError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().strip_prefix('v'))
        .filter_map(|stripped| semver::Version::parse(stripped).ok())
        .max())
}

/// As [`highest_semver_tag`], but restricted to tags reachable from `HEAD`
/// via `git tag --merged HEAD` — one spawn instead of an O(n) per-tag
/// `merge-base --is-ancestor` loop, mirroring `GitFlow::cleanup_merged`'s
/// existing `branch --merged` precedent in `git.rs`. This is `compute_version`'s
/// baseline (D-07).
///
/// **D-12 coupling:** this predicate's correctness depends on the `develop`
/// → `main` sync PR being MERGED, not squashed — a squashed sync breaks the
/// ancestry link this `--merged` check relies on. `compute_version`'s
/// refusal (D-10, `VersionError::UnreachableBaseline`) is the mitigation if
/// that discipline is ever violated; 999.52 is the backlog item that would
/// ship a structural repair, deliberately not in this phase.
pub fn reachable_semver_baseline(
    project_root: &Path,
) -> Result<Option<semver::Version>, VersionError> {
    let output = Command::new("git")
        .args(["tag", "--merged", "HEAD"])
        .current_dir(project_root)
        .output()
        .map_err(|err| VersionError::Git(err.to_string()))?;
    if !output.status.success() {
        return Err(VersionError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().strip_prefix('v'))
        .filter_map(|stripped| semver::Version::parse(stripped).ok())
        .max())
}

/// Resolve the commit range start for D-08's conventional-commit classifier,
/// given the baseline tag name (e.g. `"v2.0.0"`).
///
/// This exists because every release in this repository squash-merges
/// `develop` into `main`, so no develop-side commit is ever an ancestor of
/// the release tag it was squashed into — a `-X ours` sync merge-back
/// restores ancestry in the OTHER direction only (the tag becomes an
/// ancestor of `HEAD`, which is what makes D-07's `--merged HEAD`
/// reachability filter work), but the commits the tag *released* stay
/// outside its ancestry forever. A literal `baseline..HEAD` range therefore
/// re-includes the entire pre-release history on every subsequent ship —
/// measured live 2026-07-27: `v2.0.0..HEAD` is 677 non-merge commits (62
/// `feat`), against 5 (0 `feat`) for the anchored range this function
/// computes. See 25-01-PLAN.md's `<measured_correction>`.
///
/// Anchor rule:
/// - `C1` = the earliest commit on the ancestry path from `baseline_tag` to
///   `HEAD` (`git rev-list --ancestry-path --reverse <tag>..HEAD`, first
///   line).
/// - If `C1` does not exist, the tag is at `HEAD` — range start is the tag.
/// - If the tag is an ancestor of `C1`'s first parent, the tag already sat
///   on this mainline (the ordinary, non-squashed case, e.g. `v1.8.0..v1.8.1`)
///   — range start is the tag, unchanged from the literal D-08 rule.
/// - Otherwise the tag entered `HEAD`'s history through `C1` (the sync
///   merge-back) — range start is `C1`.
pub fn release_range_start(
    project_root: &Path,
    baseline_tag: &str,
) -> Result<String, VersionError> {
    let ancestry = Command::new("git")
        .args([
            "rev-list",
            "--ancestry-path",
            "--reverse",
            &format!("{baseline_tag}..HEAD"),
        ])
        .current_dir(project_root)
        .output()
        .map_err(|err| VersionError::Git(err.to_string()))?;
    if !ancestry.status.success() {
        return Err(VersionError::Git(
            String::from_utf8_lossy(&ancestry.stderr).trim().to_string(),
        ));
    }
    let c1 = String::from_utf8_lossy(&ancestry.stdout)
        .lines()
        .next()
        .map(str::to_string);
    let Some(c1) = c1 else {
        // Nothing after the tag — it sits at HEAD.
        return Ok(baseline_tag.to_string());
    };

    let first_parent = Command::new("git")
        .args(["rev-parse", &format!("{c1}^1")])
        .current_dir(project_root)
        .output()
        .map_err(|err| VersionError::Git(err.to_string()))?;
    if !first_parent.status.success() {
        // C1 is a root commit with no first parent — the tag cannot be an
        // ancestor of something that doesn't exist; treat C1 itself as the
        // anchor (same branch as the sync-merge case below).
        return Ok(c1);
    }
    let first_parent = String::from_utf8_lossy(&first_parent.stdout)
        .trim()
        .to_string();

    let tag_is_ancestor_of_first_parent = Command::new("git")
        .args(["merge-base", "--is-ancestor", baseline_tag, &first_parent])
        .current_dir(project_root)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);

    if tag_is_ancestor_of_first_parent {
        Ok(baseline_tag.to_string())
    } else {
        Ok(c1)
    }
}

/// The classified conventional-commit bump for a range of commits (D-08).
/// Declaration order is the precedence order (lowest to highest), so
/// `Iterator::max()`/[`Ord::max`] over a range's individual classifications
/// yields the highest-precedence result directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bump {
    /// No commit's type maps to a version-affecting change (`docs`, `test`,
    /// `chore`, `ci`, `refactor`, `style`). `compute_version` collapses this
    /// to [`Bump::Patch`] at the call site (D-10's floor) so a range with
    /// nothing bumping still yields a distinct version.
    None,
    /// `fix`/`perf`; any recognised-but-unlisted conventional-commit type
    /// (D-10's same floor); or a commit message that failed to parse as a
    /// conventional commit at all (D-10: unrecognised/malformed → patch).
    Patch,
    /// `feat`.
    Minor,
    /// A breaking change: `!` after an optional scope and before the colon
    /// (`feat(scope)!: ...`), or a `BREAKING CHANGE:`/`BREAKING-CHANGE:`
    /// footer, regardless of the commit's own type.
    Major,
}

/// Classify the highest-precedence conventional-commit bump over
/// `--no-merges` commits in `range_start..HEAD`. `range_start` may be the
/// empty string, meaning "no baseline tag exists" — the whole history
/// reachable from `HEAD` is classified instead (`git log --no-merges HEAD`,
/// no exclusion).
///
/// Commits are read via `%H%x1f%B%x1e`: `%B` is the raw message (subject,
/// blank line, body and footers) in exactly the shape
/// `git_conventional::Commit::parse` expects, and `%x1f`/`%x1e` are git's own
/// unit/record separators — safe against arbitrary characters a commit
/// message may contain, unlike splitting on newlines.
pub fn classify_range_bump(project_root: &Path, range_start: &str) -> Result<Bump, VersionError> {
    let range = if range_start.is_empty() {
        "HEAD".to_string()
    } else {
        format!("{range_start}..HEAD")
    };
    let output = Command::new("git")
        .args(["log", "--no-merges", &range, "--format=%H%x1f%B%x1e"])
        .current_dir(project_root)
        .output()
        .map_err(|err| VersionError::Git(err.to_string()))?;
    if !output.status.success() {
        return Err(VersionError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut bump = Bump::None;
    for record in stdout.split('\u{1e}') {
        let record = record.trim_matches('\n');
        if record.is_empty() {
            continue;
        }
        let Some((_hash, message)) = record.split_once('\u{1f}') else {
            continue;
        };
        let this_bump = classify_commit_message(message.trim());
        bump = bump.max(this_bump);
    }
    Ok(bump)
}

/// Classify one commit message's bump per D-08/D-10. An unparseable message
/// (D-10: unrecognised/malformed) and a breaking-change marker (regardless of
/// type) are both checked before the type match, since either overrides a
/// recognised type's own precedence.
fn classify_commit_message(message: &str) -> Bump {
    let Ok(commit) = git_conventional::Commit::parse(message) else {
        return Bump::Patch;
    };
    if commit.breaking() {
        return Bump::Major;
    }
    let ty = commit.type_();
    if ty == git_conventional::Type::FEAT {
        Bump::Minor
    } else if ty == git_conventional::Type::FIX || ty == git_conventional::Type::PERF {
        Bump::Patch
    } else if ty == git_conventional::Type::DOCS
        || ty == git_conventional::Type::TEST
        || ty == git_conventional::Type::CHORE
        || ty == "ci"
        || ty == git_conventional::Type::REFACTOR
        || ty == git_conventional::Type::STYLE
    {
        Bump::None
    } else {
        // Any other recognised-but-unlisted type — D-10's same floor.
        Bump::Patch
    }
}

/// Apply a classified [`Bump`] to a baseline version (D-08/D-10).
fn apply_bump(baseline: &semver::Version, bump: Bump) -> semver::Version {
    match bump {
        Bump::Major => semver::Version::new(baseline.major + 1, 0, 0),
        Bump::Minor => semver::Version::new(baseline.major, baseline.minor + 1, 0),
        // D-10: no-bump collapses to patch so every completed ship still
        // yields a distinct version.
        Bump::Patch | Bump::None => {
            semver::Version::new(baseline.major, baseline.minor, baseline.patch + 1)
        }
    }
}

/// Compute the full version: the baseline resolved from the highest
/// reachable semver tag (D-07), bumped by the conventional-commit
/// classification of the commits added since that baseline was released
/// (D-08). The version file is NOT read here (D-11) — [`write_version`] is
/// the only writer, and [`read_version`] is the only reader of what's on
/// disk.
pub fn compute_version(project_root: &Path) -> Result<Version, VersionError> {
    let baseline = reachable_semver_baseline(project_root)?;
    let baseline_version = baseline
        .clone()
        .unwrap_or_else(|| semver::Version::new(0, 0, 0));

    let range_start = match &baseline {
        Some(tag) => release_range_start(project_root, &format!("v{tag}"))?,
        None => String::new(),
    };
    let bump = classify_range_bump(project_root, &range_start)?;
    let bumped = apply_bump(&baseline_version, bump);

    Ok(Version {
        major: bumped.major as u32,
        minor: bumped.minor as u32,
        patch: bumped.patch as u32,
    })
}

/// Read the full [`Version`] (major/minor/patch) out of whatever version file
/// `detect_version_file` resolves, mirroring [`write_version`]'s format
/// handling (including `[workspace.package]`).
///
/// Unlike [`compute_version`], this never touches git — it reports exactly
/// what was last written to the version file, not a freshly recomputed
/// minor/patch. Callers that need the version a prior [`write_version`] call
/// actually wrote (e.g. after a tag was just cut) must use this instead of
/// `compute_version`, which would see the new tag and return a different,
/// larger version.
///
/// D-11 changed what `compute_version` reads (git history only, never the
/// version file) — it did not change this function's role: `read_version`
/// still reports exactly what's on disk, unconditionally.
pub fn read_version(project_root: &Path) -> Result<Version, VersionError> {
    let path = detect_version_file(project_root)
        .ok_or_else(|| VersionError::Parse("no version file found".into()))?;
    let contents = std::fs::read_to_string(&path)?;
    let field = field_for(&path, &contents);
    let version_str = find_version_in_contents(&contents, field)
        .ok_or_else(|| VersionError::Parse(format!("field `{field}` not found in {path:?}")))?;
    parse_version_str(&version_str)
}

/// Parse a `MAJOR.MINOR.PATCH` string (optionally followed by `-`/`+`
/// metadata) into a [`Version`].
fn parse_version_str(version: &str) -> Result<Version, VersionError> {
    let mut parts = version.split(['.', '+', '-']);
    let mut next =
        |label: &str| -> Result<u32, VersionError> {
            parts.next().unwrap_or("0").parse::<u32>().map_err(|err| {
                VersionError::Parse(format!("invalid {label} in `{version}`: {err}"))
            })
        };
    let major = next("major")?;
    let minor = next("minor")?;
    let patch = next("patch")?;
    Ok(Version {
        major,
        minor,
        patch,
    })
}

/// Write `version` into the project's auto-detected version file.
pub fn write_version(project_root: &Path, version: &Version) -> Result<PathBuf, VersionError> {
    let path = detect_version_file(project_root)
        .ok_or_else(|| VersionError::Parse("no version file found".into()))?;
    let contents = std::fs::read_to_string(&path)?;
    let field = field_for(&path, &contents);
    let replaced = replace_version_in_contents(&contents, field, &version.to_string())
        .ok_or_else(|| VersionError::Parse(format!("field `{field}` not found")))?;
    // 20a / DEN-49: a workspace Cargo.toml states its version twice — once in
    // [workspace.package] version (just rewritten above), and again as an
    // explicit `version` pin on every [workspace.dependencies] entry that
    // points at a workspace member by `path`. This second pass is additive,
    // not a modification of `replace_version_in_contents`'s single-field
    // logic — pyproject.toml/package.json/plain Cargo.toml callers never
    // reach it.
    let replaced = if field == "workspace.package.version" {
        rewrite_workspace_member_pins(&replaced, &version.to_string())
    } else {
        replaced
    };
    std::fs::write(&path, replaced)?;
    Ok(path)
}

/// Additive pass (20a / DEN-49): rewrite the `version` sub-value of every
/// SINGLE-LINE `[workspace.dependencies]` inline-table entry that pins a
/// local workspace member by `path` (e.g. `devflow-core = { path =
/// "crates/devflow-core", version = "1.6.0" }`).
///
/// This is deliberately additive to `replace_version_in_contents` rather than
/// a modification of it — that function's `starts_with('{')` guard exists so
/// single-field callers (`field_for` for pyproject.toml/package.json/plain
/// Cargo.toml) never touch an inline table, and stays intact.
///
/// Scope, by construction:
/// - Only entries with a local `path` key (one starting with `crates/`) are
///   rewritten. A `version`-only third-party dependency (`serde = { version
///   = "1" }`) is left untouched — a dependency on a crate INSIDE this
///   workspace carries this workspace's version; anything else does not.
/// - Only SINGLE-LINE inline tables are handled (opening and closing `}` on
///   the same line as `path`/`version`). A multi-line inline table is a
///   documented out-of-scope limitation (review: Antigravity/Hermes MEDIUM)
///   — this repo's own self-pins are single-line (Cargo.toml:20), and the
///   line-level `starts_with('{')` guard in `find_version_in_contents`/
///   `replace_version_in_contents` could not see into one anyway.
/// - The `version = "..."` sub-value is located and replaced independent of
///   its position relative to `path` within the line (key-order-independent,
///   anchored to the `version =` token itself, not a column offset) — a
///   self-pin written `{ version = "1.6.0", path = "crates/..." }` is
///   rewritten identically to the `path`-before-`version` case.
/// - Whitespace, quote style, and any trailing comma/comment after the
///   `version` token are preserved exactly (GAP-6).
fn rewrite_workspace_member_pins(contents: &str, new_version: &str) -> String {
    let mut current = String::new();
    let mut output = String::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(header) = parse_section_header(trimmed) {
            current = header.to_string();
            output.push_str(line);
            output.push('\n');
            continue;
        }
        if current == "workspace.dependencies"
            && trimmed.contains('{')
            && trimmed.contains('}')
            && workspace_dependency_has_local_path(trimmed)
            && let Some(rewritten) = rewrite_inline_table_version(line, new_version)
        {
            output.push_str(&rewritten);
            output.push('\n');
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    output
}

/// Split a single-line inline table's interior (`{ ... }`, braces excluded)
/// into its top-level `key = value` fragments, alongside each fragment's
/// absolute byte offset within `line`. Fragments are separated on `,` — this
/// is a hand-rolled, single-line-only split (see `rewrite_workspace_member_pins`
/// doc comment), not a general TOML parser.
fn inline_table_fragments(line: &str) -> Option<Vec<(usize, &str)>> {
    let brace_start = line.find('{')?;
    let brace_end = line.rfind('}')?;
    if brace_end <= brace_start {
        return None;
    }
    let inner = &line[brace_start + 1..brace_end];
    let mut fragments = Vec::new();
    let mut offset = brace_start + 1;
    for fragment in inner.split(',') {
        fragments.push((offset, fragment));
        offset += fragment.len() + 1; // +1 for the consumed comma
    }
    Some(fragments)
}

/// Whether a `[workspace.dependencies]` inline-table line carries a `path`
/// key whose value points at a local workspace member (starts with
/// `crates/`).
fn workspace_dependency_has_local_path(line: &str) -> bool {
    let Some(fragments) = inline_table_fragments(line) else {
        return false;
    };
    for (_, fragment) in fragments {
        let trimmed = fragment.trim();
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() != "path" {
            continue;
        }
        let value = value.trim();
        let Some(quote) = value.chars().next() else {
            return false;
        };
        if quote != '"' && quote != '\'' {
            return false;
        }
        let inner_value = &value[1..value.len().saturating_sub(1)];
        return inner_value.starts_with("crates/");
    }
    false
}

/// Rewrite the `version = "..."` sub-value on a single-line inline-table
/// line, preserving everything else on the line byte-for-byte. Returns
/// `None` if the line has no `version` fragment to anchor to (e.g. a
/// `path`-only member with no explicit version — nothing to rewrite).
fn rewrite_inline_table_version(line: &str, new_version: &str) -> Option<String> {
    let fragments = inline_table_fragments(line)?;
    for (frag_start, fragment) in fragments {
        let trimmed = fragment.trim();
        let Some((key, _value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() != "version" {
            continue;
        }
        // Locate `=` in the ORIGINAL (untrimmed) fragment to compute an
        // absolute offset into `line`.
        let eq_rel = fragment.find('=')?;
        let eq_abs = frag_start + eq_rel;
        let after_eq = eq_abs + 1;
        let rest = &line[after_eq..];
        let ws_len = rest.len() - rest.trim_start().len();
        let value_start = after_eq + ws_len;
        let value_rest = &line[value_start..];
        let quote_char = value_rest.chars().next()?;
        if quote_char != '"' && quote_char != '\'' {
            return None;
        }
        let after_quote = &value_rest[1..];
        let end_rel = after_quote.find(quote_char)?;
        let value_end = value_start + 1 + end_rel + 1;
        let remainder = &line[value_end..];

        let mut rewritten = String::with_capacity(line.len() + new_version.len());
        rewritten.push_str(&line[..value_start]);
        rewritten.push(quote_char);
        rewritten.push_str(new_version);
        rewritten.push(quote_char);
        rewritten.push_str(remainder);
        return Some(rewritten);
    }
    None
}

/// One `[workspace.dependencies]` self-pin discovered by
/// [`read_workspace_self_pins`] — a local-path dependency's name and its
/// pinned `version` sub-value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfPin {
    /// The dependency's name (left-hand side of `=` in `[workspace.dependencies]`).
    pub name: String,
    /// The `version = "..."` value currently pinned in the inline table.
    pub version: String,
}

/// Extract `[workspace.package] version` and every local-path
/// `[workspace.dependencies]` self-pin (crate name + pinned version) from a
/// workspace Cargo.toml's contents.
///
/// Read-only (20d / `devflow release --check`): asserts 20a's invariant
/// (`write_version` keeps every self-pin equal to the workspace version)
/// without re-implementing TOML scanning — reuses the same
/// `parse_section_header`/`find_version_in_contents`/
/// `workspace_dependency_has_local_path`/`inline_table_fragments` helpers
/// `write_version`'s additive rewrite pass already uses.
///
/// Returns `(workspace_version, pins)`. `workspace_version` is `None` when
/// the contents have no `[workspace.package] version` field (not a workspace
/// root Cargo.toml) — callers must treat that as "nothing to assert", not a
/// drift.
pub fn read_workspace_self_pins(contents: &str) -> (Option<String>, Vec<SelfPin>) {
    let workspace_version = find_version_in_contents(contents, "workspace.package.version");

    let mut current = String::new();
    let mut pins = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(header) = parse_section_header(trimmed) {
            current = header.to_string();
            continue;
        }
        if current == "workspace.dependencies"
            && trimmed.contains('{')
            && trimmed.contains('}')
            && workspace_dependency_has_local_path(trimmed)
            && let Some(fragments) = inline_table_fragments(trimmed)
        {
            let name = trimmed
                .split_once('=')
                .map(|(n, _)| n.trim().to_string())
                .unwrap_or_default();
            for (_, fragment) in fragments {
                let frag = fragment.trim();
                let Some((key, value)) = frag.split_once('=') else {
                    continue;
                };
                if key.trim() != "version" {
                    continue;
                }
                let value = value.trim().trim_matches(['"', '\'']);
                pins.push(SelfPin {
                    name: name.clone(),
                    version: value.to_string(),
                });
            }
        }
    }
    (workspace_version, pins)
}

/// Split a dotted field path into its TOML section path and the final key.
fn split_field(field: &str) -> (&str, &str) {
    match field.rsplit_once('.') {
        Some((section, key)) => (section, key),
        None => ("", field),
    }
}

/// Return the dotted table path for a TOML section header line, if any.
fn parse_section_header(trimmed: &str) -> Option<&str> {
    let inner = if trimmed.starts_with("[[") && trimmed.ends_with("]]") {
        trimmed.strip_prefix("[[")?.strip_suffix("]]")?
    } else {
        trimmed.strip_prefix('[')?.strip_suffix(']')?
    };
    Some(inner.trim())
}

fn find_version_in_contents(contents: &str, field: &str) -> Option<String> {
    let (section, key) = split_field(field);
    let mut current = "";
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(header) = parse_section_header(trimmed) {
            current = header;
            continue;
        }
        if current != section {
            continue;
        }
        if let Some((lhs, value)) = trimmed.split_once(['=', ':']) {
            let lhs_key = lhs.trim().trim_matches('"').trim_matches('\'');
            if lhs_key != key {
                continue;
            }
            let value = value.trim();
            if value.starts_with('{') {
                continue;
            }
            // Anchor on the opening quote and scan forward for the matching
            // closing quote, ignoring everything after it (e.g. a trailing
            // `# comment`), rather than `trim_matches` on the whole tail —
            // that would only strip a quote sitting at the very end of the
            // remaining string, missing it entirely when a comment follows
            // the closing quote on the same line. Symmetric with
            // `replace_version_in_contents`'s write-path remainder handling.
            return match value.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    value[1..].find(q).map(|end| value[1..1 + end].to_string())
                }
                _ => {
                    let end = value.find([' ', '\t', ',', '#']).unwrap_or(value.len());
                    Some(value[..end].to_string())
                }
            };
        }
    }
    None
}

fn replace_version_in_contents(contents: &str, field: &str, new_version: &str) -> Option<String> {
    let (section, key) = split_field(field);
    let mut current = "";
    let mut changed = false;
    let mut output = String::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(header) = parse_section_header(trimmed) {
            current = header;
            output.push_str(line);
            output.push('\n');
            continue;
        }
        if !changed
            && current == section
            && let Some((left, value)) = line.split_once(['=', ':'])
        {
            let left_key = left.trim().trim_matches('"').trim_matches('\'');
            if left_key == key && !value.trim().starts_with('{') {
                let separator: &str = if trimmed.contains('=') { " = " } else { ": " };
                let trimmed_value = value.trim();
                let needs_quote = trimmed_value.starts_with('"') || trimmed_value.starts_with('\'');
                let quote_char: &str = if trimmed_value.starts_with('\'') {
                    "'"
                } else {
                    "\""
                };
                // Capture whatever follows the version token itself (a
                // trailing `,` in JSON, a trailing `# comment` in TOML) so it
                // survives the rewrite instead of being silently dropped
                // (GAP-6).
                let remainder = if needs_quote {
                    // Token ends at the closing quote; skip the opening
                    // quote and scan for the matching close.
                    trimmed_value[1..]
                        .find(quote_char)
                        .map(|end| &trimmed_value[end + 2..])
                        .unwrap_or("")
                } else {
                    // Unquoted: token ends at the first whitespace, `,`, or `#`.
                    let end = trimmed_value
                        .find([' ', '\t', ',', '#'])
                        .unwrap_or(trimmed_value.len());
                    &trimmed_value[end..]
                };
                output.push_str(left.trim_end());
                output.push_str(separator);
                if needs_quote {
                    output.push_str(quote_char);
                    output.push_str(new_version);
                    output.push_str(quote_char);
                } else {
                    output.push_str(new_version);
                }
                output.push_str(remainder.trim_end());
                output.push('\n');
                changed = true;
                continue;
            }
        }
        output.push_str(line);
        output.push('\n');
    }
    changed.then_some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(root: &Path, args: &[&str]) {
        let ok = crate::test_support::git_command(root)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    }

    fn init_repo(root: &Path) {
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "user.name", "Test"]);
        git(root, &["config", "commit.gpgsign", "false"]);
        git(root, &["config", "tag.gpgsign", "false"]);
        git(root, &["config", "core.hooksPath", "/dev/null"]);
    }

    fn commit(root: &Path, name: &str) {
        std::fs::write(root.join(name), name).unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", &format!("add {name}")]);
    }

    /// As [`commit`], but with an explicit commit message — needed for
    /// conventional-commit classification fixtures, where the message
    /// content (not the file name) is what's under test.
    fn commit_msg(root: &Path, name: &str, message: &str) {
        std::fs::write(root.join(name), name).unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", message]);
    }

    /// One-line `tag` helper of the same shape as `git`/`init_repo`/`commit`.
    fn tag(root: &Path, name: &str) {
        git(root, &["tag", name]);
    }

    fn current_branch(root: &Path) -> String {
        let output = crate::test_support::git_command(root)
            .args(["symbolic-ref", "--short", "HEAD"])
            .output()
            .unwrap();
        assert!(output.status.success(), "symbolic-ref --short HEAD failed");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn checkout_new(root: &Path, branch: &str) {
        git(root, &["checkout", "-b", branch]);
    }

    fn checkout(root: &Path, branch: &str) {
        git(root, &["checkout", branch]);
    }

    /// Simulate `scripts/sync-main-to-develop.sh`'s content-preserving
    /// `-X ours` merge: a real merge commit (so ancestry is restored) whose
    /// tree is unaffected (so nothing about `develop`'s own content changes).
    fn merge_ours(root: &Path, branch: &str, message: &str) {
        git(root, &["merge", "-s", "ours", "-m", message, branch]);
    }

    #[test]
    fn detect_prefers_cargo_then_pyproject_then_package_json() {
        let dir = tempfile::tempdir().unwrap();
        assert!(detect_version_file(dir.path()).is_none());
        std::fs::write(dir.path().join("package.json"), "{\"version\":\"1.0.0\"}").unwrap();
        assert!(
            detect_version_file(dir.path())
                .unwrap()
                .ends_with("package.json")
        );
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nversion=\"1.0.0\"",
        )
        .unwrap();
        assert!(
            detect_version_file(dir.path())
                .unwrap()
                .ends_with("Cargo.toml")
        );
    }

    #[test]
    fn read_major_from_workspace_package() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Cargo.toml");
        std::fs::write(
            &file,
            "[workspace.package]\nversion = \"2.5.7\"\nedition = \"2024\"\n",
        )
        .unwrap();
        assert_eq!(read_major_version(&file).unwrap(), 2);
    }

    #[test]
    fn inline_table_version_does_not_shadow_workspace_package() {
        assert_eq!(parse_section_header("[[bin]]"), Some("bin"));

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Cargo.toml");
        std::fs::write(
            &file,
            "[[bin]]\nname = \"devflow\"\n\
             [workspace.dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\n\
             [workspace.package]\nversion = \"1.2.0\"\n",
        )
        .unwrap();

        assert_eq!(read_major_version(&file).unwrap(), 1);
        write_version(
            dir.path(),
            &Version {
                major: 2,
                minor: 3,
                patch: 4,
            },
        )
        .unwrap();
        let contents = std::fs::read_to_string(file).unwrap();
        assert!(contents.contains("serde = { version = \"1\""));
        assert!(contents.contains("[workspace.package]\nversion = \"2.3.4\""));
    }

    #[test]
    fn read_major_from_package_json() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("package.json");
        std::fs::write(&file, "{\n  \"version\": \"3.1.0\"\n}\n").unwrap();
        assert_eq!(read_major_version(&file).unwrap(), 3);
    }

    #[test]
    fn docs_only_commits_after_tag_yield_patch_floor() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        commit_msg(root, "a.txt", "chore: init");
        tag(root, "v2.0.0");
        commit_msg(root, "b.txt", "docs: update readme");
        commit_msg(root, "c.txt", "docs: fix typo");

        let v = compute_version(root).unwrap();
        assert_eq!(
            v,
            Version {
                major: 2,
                minor: 0,
                patch: 1
            }
        );
    }

    #[test]
    fn feat_commit_after_tag_yields_minor_bump() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        commit_msg(root, "a.txt", "chore: init");
        tag(root, "v2.0.0");
        commit_msg(root, "b.txt", "docs: update readme");
        commit_msg(root, "c.txt", "feat(x): add new capability");

        let v = compute_version(root).unwrap();
        assert_eq!(
            v,
            Version {
                major: 2,
                minor: 1,
                patch: 0
            }
        );
    }

    #[test]
    fn fix_commit_after_tag_yields_patch_bump() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        commit_msg(root, "a.txt", "chore: init");
        tag(root, "v2.0.0");
        commit_msg(root, "b.txt", "fix(x): correct off-by-one");

        let v = compute_version(root).unwrap();
        assert_eq!(
            v,
            Version {
                major: 2,
                minor: 0,
                patch: 1
            }
        );
    }

    #[test]
    fn no_semver_tag_at_all_yields_documented_empty_repo_contract() {
        // Empty-repo contract (D-07/D-08 with no baseline tag): baseline is
        // 0.0.0, and the very first commit's own classification applies
        // directly — a `feat` yields the minor floor, `0.1.0`.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        commit_msg(root, "a.txt", "feat: initial capability");

        let v = compute_version(root).unwrap();
        assert_eq!(
            v,
            Version {
                major: 0,
                minor: 1,
                patch: 0
            }
        );
    }

    #[test]
    fn squash_sync_topology_classifies_only_post_merge_commits() {
        // Reproduces this repository's real release shape: `develop` work is
        // squash-merged into a fresh commit on the trunk (no ancestry back to
        // develop's originals), then a content-preserving `-X ours` merge
        // syncs the trunk back into develop, restoring ancestry in the OTHER
        // direction only. The classifier must see only the commit(s) added
        // AFTER that sync merge, not develop's pre-squash originals.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        commit_msg(root, "base.txt", "chore: init");
        let trunk = current_branch(root);

        checkout_new(root, "develop");
        commit_msg(root, "d1.txt", "feat: develop work one");
        commit_msg(root, "d2.txt", "feat: develop work two");

        checkout(root, &trunk);
        commit_msg(root, "sq1.txt", "feat: squashed release of develop work");
        tag(root, "v2.0.0");

        checkout(root, "develop");
        merge_ours(
            root,
            &trunk,
            "merge: sync main back into develop after release",
        );
        commit_msg(root, "f1.txt", "fix: patch after sync");

        let v = compute_version(root).unwrap();
        assert_eq!(
            v,
            Version {
                major: 2,
                minor: 0,
                patch: 1
            }
        );
    }

    #[test]
    fn two_squash_sync_cycles_anchor_to_the_second_merge_only() {
        // Pins the property release_range_start's doc comment names: because
        // reachable_semver_baseline always selects the highest reachable
        // tag, the ancestry path from that tag to HEAD crosses exactly one
        // sync merge — so inspecting only C1's first parent is sufficient
        // even with TWO release cycles in history. If baseline selection
        // ever regressed to anchor at the first cycle's merge instead of the
        // second, this fixture's first-cycle `feat` (d1) would leak back
        // into the classified range and wrongly produce a minor bump.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        commit_msg(root, "base.txt", "chore: init");
        let trunk = current_branch(root);

        checkout_new(root, "develop");
        commit_msg(root, "d1.txt", "feat: first cycle work");

        checkout(root, &trunk);
        commit_msg(root, "sq1.txt", "feat: first squashed release");
        tag(root, "v2.0.0");

        checkout(root, "develop");
        merge_ours(
            root,
            &trunk,
            "merge: sync main back into develop after release (1)",
        );
        commit_msg(root, "d3.txt", "feat: second cycle work");

        checkout(root, &trunk);
        commit_msg(root, "sq2.txt", "feat: second squashed release");
        tag(root, "v2.1.0");

        checkout(root, "develop");
        merge_ours(
            root,
            &trunk,
            "merge: sync main back into develop after release (2)",
        );
        commit_msg(root, "f1.txt", "fix: patch after second sync");

        let v = compute_version(root).unwrap();
        assert_eq!(
            v,
            Version {
                major: 2,
                minor: 1,
                patch: 1
            }
        );
    }

    #[test]
    fn unreachable_highest_tag_refuses_rather_than_falling_back() {
        // D-10: when the highest semver tag overall is not reachable from
        // HEAD, compute_version must refuse — never silently fall back to
        // the highest *reachable* tag (which would compute a version below
        // the real release history, T-25-04).
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        commit_msg(root, "a.txt", "chore: init");
        tag(root, "v1.0.0");
        let main_branch = current_branch(root);

        git(root, &["checkout", "--orphan", "orphan-release"]);
        git(root, &["commit", "--allow-empty", "-q", "-m", "chore: orphan"]);
        tag(root, "v9.9.9");
        git(root, &["checkout", &main_branch]);

        let err = compute_version(root).unwrap_err();
        match err {
            VersionError::UnreachableBaseline { tag } => {
                assert_eq!(tag, "v9.9.9", "refusal must name the unreachable tag");
            }
            other => panic!(
                "expected UnreachableBaseline (never a silent smaller Ok), got: {other:?}"
            ),
        }
    }

    #[test]
    fn range_with_no_bumping_commits_yields_patch_floor() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        commit_msg(root, "a.txt", "chore: init");
        tag(root, "v1.0.0");
        commit_msg(root, "b.txt", "docs: update readme");
        commit_msg(root, "c.txt", "chore: tidy up");
        commit_msg(root, "d.txt", "ci: tweak workflow");

        let v = compute_version(root).unwrap();
        assert_eq!(
            v,
            Version {
                major: 1,
                minor: 0,
                patch: 1
            }
        );
    }

    #[test]
    fn malformed_commit_message_yields_patch_not_crash_or_major() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        commit_msg(root, "a.txt", "chore: init");
        tag(root, "v1.0.0");
        commit_msg(
            root,
            "b.txt",
            "just a plain message with no conventional type prefix!!!",
        );

        let v = compute_version(root).unwrap();
        assert_eq!(
            v,
            Version {
                major: 1,
                minor: 0,
                patch: 1
            }
        );
    }

    #[test]
    fn exclamation_before_colon_yields_major() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        commit_msg(root, "a.txt", "chore: init");
        tag(root, "v1.0.0");
        commit_msg(root, "b.txt", "feat(scope)!: drop legacy api");

        let v = compute_version(root).unwrap();
        assert_eq!(
            v,
            Version {
                major: 2,
                minor: 0,
                patch: 0
            }
        );
    }

    #[test]
    fn breaking_change_footer_yields_major_even_with_fix_subject() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        commit_msg(root, "a.txt", "chore: init");
        tag(root, "v1.0.0");
        git(
            root,
            &[
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                "fix: patch a thing\n\nBREAKING CHANGE: removes an implicit default",
            ],
        );

        let v = compute_version(root).unwrap();
        assert_eq!(
            v,
            Version {
                major: 2,
                minor: 0,
                patch: 0
            }
        );
    }

    #[test]
    fn exclamation_only_in_description_does_not_yield_major() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        commit_msg(root, "a.txt", "chore: init");
        tag(root, "v1.0.0");
        commit_msg(root, "b.txt", "fix: stop the crash!!!");

        let v = compute_version(root).unwrap();
        assert_eq!(
            v,
            Version {
                major: 1,
                minor: 0,
                patch: 1
            }
        );
    }

    #[test]
    fn write_version_replaces_in_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let path = write_version(
            dir.path(),
            &Version {
                major: 2,
                minor: 3,
                patch: 4,
            },
        )
        .unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("version = \"2.3.4\""));
    }

    #[test]
    fn write_version_replaces_in_workspace_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace.package]\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .unwrap();
        let path = write_version(
            dir.path(),
            &Version {
                major: 2,
                minor: 3,
                patch: 4,
            },
        )
        .unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[workspace.package]\nversion = \"2.3.4\""));
    }

    #[test]
    fn write_version_errors_without_version_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            write_version(
                dir.path(),
                &Version {
                    major: 1,
                    minor: 0,
                    patch: 0
                }
            ),
            Err(VersionError::Parse(_))
        ));
    }

    #[test]
    fn read_version_round_trips_through_write_version_in_plain_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let written = Version {
            major: 2,
            minor: 3,
            patch: 4,
        };
        write_version(dir.path(), &written).unwrap();
        assert_eq!(read_version(dir.path()).unwrap(), written);
    }

    #[test]
    fn read_version_round_trips_through_write_version_in_workspace_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace.package]\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .unwrap();
        let written = Version {
            major: 5,
            minor: 6,
            patch: 7,
        };
        write_version(dir.path(), &written).unwrap();
        assert_eq!(read_version(dir.path()).unwrap(), written);
    }

    #[test]
    fn read_version_round_trips_through_write_version_in_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            "{\n  \"version\": \"0.1.0\"\n}\n",
        )
        .unwrap();
        let written = Version {
            major: 1,
            minor: 9,
            patch: 12,
        };
        write_version(dir.path(), &written).unwrap();
        assert_eq!(read_version(dir.path()).unwrap(), written);
    }

    #[test]
    fn read_version_errors_without_version_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            read_version(dir.path()),
            Err(VersionError::Parse(_))
        ));
    }

    #[test]
    fn write_version_preserves_trailing_comma_in_package_json() {
        // GAP-6: replace_version_in_contents reassembles the matched line as
        // `left.trim_end() + separator + quoted_version + '\n'`, discarding
        // everything in `value` after the version token. For a real
        // package.json where `version` is not the last key, that eats the
        // mandatory trailing comma and produces invalid JSON. Parsing is the
        // assertion that matters here — a substring check would be a
        // vacuous fixture that can't reach this defect.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            "{\n  \"name\": \"x\",\n  \"version\": \"0.1.0\",\n  \"private\": true\n}\n",
        )
        .unwrap();
        write_version(
            dir.path(),
            &Version {
                major: 2,
                minor: 3,
                patch: 4,
            },
        )
        .unwrap();
        let contents = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap_or_else(|err| {
            panic!("package.json no longer parses as JSON: {err}\n{contents}")
        });
        assert_eq!(parsed["name"], "x");
        assert_eq!(parsed["private"], true);
        assert_eq!(parsed["version"], "2.3.4");
    }

    #[test]
    fn write_version_preserves_trailing_comment_in_toml() {
        // GAP-6, TOML variant: a trailing `# comment` after the quoted
        // version is discarded by the same line-reassembly defect.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nversion = \"0.1.0\"  # pinned\n",
        )
        .unwrap();
        write_version(
            dir.path(),
            &Version {
                major: 2,
                minor: 3,
                patch: 4,
            },
        )
        .unwrap();
        let contents = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(
            contents.contains("version = \"2.3.4\"  # pinned"),
            "expected trailing comment to survive, got: {contents}"
        );
    }

    #[test]
    fn write_version_preserves_trailing_comment_in_single_quoted_toml() {
        // GAP-6, TOML literal-string variant (17-13 review IN-03): the
        // remainder scan keys off the OPENING quote character, so the
        // single-quote branch is a distinct path from the double-quote case
        // above and needs its own fixture.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nversion = '0.1.0'  # pinned\n",
        )
        .unwrap();
        write_version(
            dir.path(),
            &Version {
                major: 2,
                minor: 3,
                patch: 4,
            },
        )
        .unwrap();
        let contents = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(
            contents.contains("version = '2.3.4'  # pinned"),
            "expected single-quoted value and trailing comment to survive, got: {contents}"
        );
    }

    #[test]
    fn read_version_extracts_clean_value_with_trailing_comment() {
        // CR-01 (phase 20 review): `find_version_in_contents` used to
        // `trim_matches` the whole tail of the line, which only strips a
        // quote sitting at the very end of the remaining string. With a
        // trailing `# comment` after the closing quote, the real closing
        // quote is never stripped and the corrupted value fails to parse.
        // `write_version` already preserves this exact pattern (GAP-6); the
        // read path must be symmetric with it.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nversion = \"1.7.0\"  # pinned release version\n",
        )
        .unwrap();
        assert_eq!(
            read_version(dir.path()).unwrap(),
            Version {
                major: 1,
                minor: 7,
                patch: 0
            }
        );
    }

    #[test]
    fn read_version_extracts_clean_value_without_trailing_comment() {
        // Bare `version = "1.7.0"` (no comment) must still work.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nversion = \"1.7.0\"\n",
        )
        .unwrap();
        assert_eq!(
            read_version(dir.path()).unwrap(),
            Version {
                major: 1,
                minor: 7,
                patch: 0
            }
        );
    }

    #[test]
    fn read_workspace_self_pins_extracts_clean_workspace_version_with_trailing_comment() {
        // CR-01: `read_workspace_self_pins` calls `find_version_in_contents`
        // for `workspace_version` too — a trailing comment next to
        // `[workspace.package] version` must not corrupt the value
        // `check_self_pin` compares pins against.
        let (workspace_version, _pins) = read_workspace_self_pins(
            "[workspace.package]\nversion = \"1.7.0\"  # pinned release version\nedition = \"2024\"\n",
        );
        assert_eq!(workspace_version.as_deref(), Some("1.7.0"));
    }

    #[test]
    fn read_version_does_not_recompute_from_git_tags() {
        // read_version must report exactly what's on disk, not a freshly
        // computed minor/patch — this is the property VersionBump/
        // ChangelogAppend ordering depends on (version.rs must never see a
        // tag VersionBump just created and derive a different number).
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        std::fs::write(root.join("Cargo.toml"), "[package]\nversion = \"2.0.0\"\n").unwrap();
        commit(root, "a.txt");
        write_version(
            root,
            &Version {
                major: 2,
                minor: 0,
                patch: 0,
            },
        )
        .unwrap();
        git(root, &["tag", "v2.0.0"]);
        commit(root, "b.txt");
        commit(root, "c.txt");
        // compute_version would recompute from git history (baseline v2.0.0,
        // bumped by whatever the two later commits classify to) instead of
        // reporting the version file. read_version must still report exactly
        // what's on disk: 2.0.0.
        assert_eq!(
            read_version(root).unwrap(),
            Version {
                major: 2,
                minor: 0,
                patch: 0
            }
        );
    }

    #[test]
    fn write_version_rewrites_workspace_dependency_self_pin() {
        // 20a / DEN-49: a published Cargo workspace states its version twice —
        // once in [workspace.package] version, and again as an explicit
        // `version` pin on every [workspace.dependencies] entry that points
        // at a workspace member by `path` (Cargo has no interpolation for
        // dependency versions, and a path dependency of a *published* crate
        // requires an explicit version). write_version must rewrite BOTH in
        // one write, or the self-pin ships stale and `cargo publish` rejects
        // the upload as a duplicate on release day (shipped broken twice:
        // v1.5.0 by 7ad260c, v1.6.0 by PR #15).
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace.package]\nversion = \"1.6.0\"\nedition = \"2024\"\n\n\
             [workspace.dependencies]\n\
             devflow-core = { path = \"crates/devflow-core\", version = \"1.6.0\" }\n",
        )
        .unwrap();
        write_version(
            dir.path(),
            &Version {
                major: 1,
                minor: 7,
                patch: 0,
            },
        )
        .unwrap();
        let contents = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(
            contents.contains("[workspace.package]\nversion = \"1.7.0\""),
            "expected [workspace.package] version to be rewritten, got: {contents}"
        );
        assert!(
            contents
                .contains("devflow-core = { path = \"crates/devflow-core\", version = \"1.7.0\" }"),
            "expected the [workspace.dependencies] self-pin to be rewritten to 1.7.0 \
             alongside [workspace.package] version, got: {contents}"
        );
    }

    #[test]
    fn write_version_no_ops_on_missing_workspace_dependencies_section() {
        // 20a/empty: a workspace Cargo.toml with no [workspace.dependencies]
        // section at all must not panic — the additive pass simply never
        // matches and the file is otherwise rewritten normally.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace.package]\nversion = \"1.6.0\"\nedition = \"2024\"\n",
        )
        .unwrap();
        write_version(
            dir.path(),
            &Version {
                major: 1,
                minor: 7,
                patch: 0,
            },
        )
        .unwrap();
        let contents = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert_eq!(
            contents,
            "[workspace.package]\nversion = \"1.7.0\"\nedition = \"2024\"\n"
        );
    }

    #[test]
    fn write_version_no_ops_on_member_with_no_version_key() {
        // 20a/empty: a [workspace.dependencies] entry with a local `path`
        // but no `version` key at all is left unchanged — nothing to
        // rewrite, and no panic.
        let dir = tempfile::tempdir().unwrap();
        let toml = "[workspace.package]\nversion = \"1.6.0\"\nedition = \"2024\"\n\n\
             [workspace.dependencies]\n\
             devflow-core = { path = \"crates/devflow-core\" }\n";
        std::fs::write(dir.path().join("Cargo.toml"), toml).unwrap();
        write_version(
            dir.path(),
            &Version {
                major: 1,
                minor: 7,
                patch: 0,
            },
        )
        .unwrap();
        let contents = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(
            contents.contains("devflow-core = { path = \"crates/devflow-core\" }"),
            "expected the version-less path member to be left byte-identical, got: {contents}"
        );
    }

    #[test]
    fn write_version_leaves_third_party_version_only_dep_untouched() {
        // 20a/adjacency: a third-party version-only dep sitting adjacent to
        // a local path member is left byte-for-byte unchanged — only the
        // path member's version sub-value is rewritten.
        let dir = tempfile::tempdir().unwrap();
        let third_party_line = "serde = { version = \"1\", features = [\"derive\"] }";
        let toml = format!(
            "[workspace.package]\nversion = \"1.6.0\"\nedition = \"2024\"\n\n\
             [workspace.dependencies]\n\
             devflow-core = {{ path = \"crates/devflow-core\", version = \"1.6.0\" }}\n\
             {third_party_line}\n"
        );
        std::fs::write(dir.path().join("Cargo.toml"), &toml).unwrap();
        write_version(
            dir.path(),
            &Version {
                major: 1,
                minor: 7,
                patch: 0,
            },
        )
        .unwrap();
        let contents = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(
            contents
                .contains("devflow-core = { path = \"crates/devflow-core\", version = \"1.7.0\" }"),
            "expected the local path member's version to be rewritten, got: {contents}"
        );
        assert!(
            contents.contains(third_party_line),
            "expected the third-party version-only dep to be byte-identical, got: {contents}"
        );
    }

    #[test]
    fn write_version_preserves_comment_and_quote_in_workspace_dependency_pin() {
        // GAP-6, inline-table variant: a self-pin line with a trailing
        // comment and single-quoted values keeps its comment and quote
        // style after rewrite.
        let dir = tempfile::tempdir().unwrap();
        let toml = "[workspace.package]\nversion = \"1.6.0\"\nedition = \"2024\"\n\n\
             [workspace.dependencies]\n\
             devflow-core = { path = 'crates/devflow-core', version = '1.6.0' }  # pinned\n";
        std::fs::write(dir.path().join("Cargo.toml"), toml).unwrap();
        write_version(
            dir.path(),
            &Version {
                major: 1,
                minor: 7,
                patch: 0,
            },
        )
        .unwrap();
        let contents = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(
            contents.contains(
                "devflow-core = { path = 'crates/devflow-core', version = '1.7.0' }  # pinned"
            ),
            "expected single-quote style and trailing comment to survive the rewrite, got: {contents}"
        );
    }

    #[test]
    fn write_version_rewrites_self_pin_regardless_of_key_order() {
        // review: inline-table key-order — the version sub-value is
        // rewritten whether it appears BEFORE or AFTER path in the inline
        // table; the replacement is anchored strictly to the path=/
        // version= tokens, not a column offset.
        let dir = tempfile::tempdir().unwrap();
        let toml = "[workspace.package]\nversion = \"1.6.0\"\nedition = \"2024\"\n\n\
             [workspace.dependencies]\n\
             devflow-core = { version = \"1.6.0\", path = \"crates/devflow-core\" }\n";
        std::fs::write(dir.path().join("Cargo.toml"), toml).unwrap();
        write_version(
            dir.path(),
            &Version {
                major: 1,
                minor: 7,
                patch: 0,
            },
        )
        .unwrap();
        let contents = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(
            contents
                .contains("devflow-core = { version = \"1.7.0\", path = \"crates/devflow-core\" }"),
            "expected version to be rewritten regardless of key order, got: {contents}"
        );
    }
}
