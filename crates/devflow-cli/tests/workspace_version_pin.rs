//! Regression guard: every `[workspace.dependencies]` entry that points at a
//! workspace member by `path` must carry a `version` equal to
//! `[workspace.package] version`.
//!
//! `version::write_version` (the engine behind the `VersionBump` ship hook)
//! rewrites exactly one dotted field — `field_for()` returns
//! `"workspace.package.version"` for a workspace `Cargo.toml`, and
//! `replace_version_in_contents` rewrites that field alone. But this workspace
//! states its version in *two* places: `[workspace.package] version`, and the
//! `devflow-core` self-pin under `[workspace.dependencies]`, which cannot use
//! `version.workspace = true` (Cargo has no interpolation for dependency
//! versions) and cannot be omitted either (a path dependency of a *published*
//! crate requires an explicit version).
//!
//! So `VersionBump` silently leaves the pin on the previous release's version.
//! This has now shipped broken twice:
//!
//! - v1.5.0 — fixed after the fact by `7ad260c`
//! - v1.6.0 — fixed by the release-prep PR (#15) that this guard accompanies
//!
//! The failure mode is genuinely nasty because it is invisible until the very
//! last step of a release: everything builds, every test passes, and clippy is
//! clean, because a `path` dependency resolves locally and ignores the stale
//! `version` field entirely. It only detonates at `cargo publish`, where the
//! registry rejects the upload as a duplicate of the already-published
//! version — on release day, after `main` has already been tagged.
//!
//! A guard asserting only "the pin is *some* valid semver" would not catch
//! this; the stale pin is always valid semver. The assertion has to be
//! equality against `[workspace.package] version`.
//!
//! Tracked for a proper fix in `VersionBump` as backlog 999.24 / DEN-49.

use std::path::PathBuf;

/// Cargo test binaries run with cwd = the crate dir, but the manifest under
/// test is the workspace root's.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

/// Strip a `# comment` tail, then trim.
fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(i) => line[..i].trim_end(),
        None => line,
    }
}

/// Extract the value of a `key = "value"` pair from a TOML fragment.
///
/// Deliberately literal rather than a full TOML parse: this project hand-rolls
/// its version TOML handling (see `version.rs`), and pulling a TOML parser in
/// as a dev-dependency just for one guard is not worth it.
fn value_of<'a>(fragment: &'a str, key: &str) -> Option<&'a str> {
    let at = fragment.find(key)?;
    let rest = &fragment[at + key.len()..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// Return the current TOML section header, e.g. `workspace.dependencies`.
fn section_of(trimmed: &str) -> Option<&str> {
    trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .map(str::trim)
}

#[test]
fn workspace_member_pins_match_the_workspace_version() {
    let manifest = repo_root().join("Cargo.toml");
    let contents = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest.display()));

    let mut section = String::new();
    let mut workspace_version: Option<String> = None;
    // (crate name, pinned version) for every path-dependency on a workspace member.
    let mut member_pins: Vec<(String, String)> = Vec::new();

    for raw in contents.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = section_of(line) {
            section = header.to_string();
            continue;
        }

        match section.as_str() {
            "workspace.package" => {
                if workspace_version.is_none()
                    && line.starts_with("version")
                    && let Some(v) = value_of(line, "version")
                {
                    workspace_version = Some(v.to_string());
                }
            }
            "workspace.dependencies" => {
                // Only entries that resolve to a crate inside this repo. A
                // third-party dep like `serde = { version = "1" }` has a
                // version but no local path, and must not be checked.
                if line.contains("path = \"crates/")
                    && let Some(version) = value_of(line, "version")
                {
                    let name = line
                        .split('=')
                        .next()
                        .map(str::trim)
                        .unwrap_or_default()
                        .to_string();
                    member_pins.push((name, version.to_string()));
                }
            }
            _ => {}
        }
    }

    let workspace_version =
        workspace_version.expect("[workspace.package] version not found in root Cargo.toml");

    assert!(
        !member_pins.is_empty(),
        "no [workspace.dependencies] entry with `path = \"crates/...\"` and a `version` was found \
         in {}. This guard exists to keep those pins in step with \
         [workspace.package] version = \"{workspace_version}\"; if the last such pin was \
         deliberately removed, delete this test in the same commit and say why.",
        manifest.display()
    );

    for (name, pinned) in &member_pins {
        assert_eq!(
            pinned, &workspace_version,
            "`{name}` is pinned to \"{pinned}\" under [workspace.dependencies], but \
             [workspace.package] version is \"{workspace_version}\".\n\n\
             `VersionBump` rewrites only [workspace.package] version, so this pin has to be \
             bumped alongside it. Leaving it stale builds, tests, and lints clean — a `path` \
             dependency ignores the `version` field locally — and then fails at `cargo publish` \
             with a duplicate-version rejection, on release day, after the tag is cut.\n\n\
             Fix: set `{name} = {{ path = \"...\", version = \"{workspace_version}\" }}` in the \
             root Cargo.toml."
        );
    }
}
