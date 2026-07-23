//! Hybrid Git-based SemVer.
//!
//! DevFlow derives the version from a mix of the project's version file and git
//! history rather than a config-driven scheme:
//!
//! - **MAJOR** — read from the auto-detected version file (`Cargo.toml`,
//!   `pyproject.toml`, or `package.json`). This is the one component a human
//!   bumps deliberately.
//! - **MINOR** — the number of git tags (one tag per shipped milestone).
//! - **PATCH** — commits since the most recent tag.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A computed semantic version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    /// Major component, from the version file.
    pub major: u32,
    /// Minor component, from the git tag count.
    pub minor: u32,
    /// Patch component, from commits since the last tag.
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

/// Compute the full version: MAJOR from the version file, MINOR from the tag
/// count, PATCH from commits since the last tag.
pub fn compute_version(project_root: &Path) -> Result<Version, VersionError> {
    let major = match detect_version_file(project_root) {
        Some(path) => read_major_version(&path)?,
        None => 0,
    };
    let minor = count_git_tags(project_root)?;
    let patch = commits_since_last_minor_tag(project_root)?;
    Ok(Version {
        major,
        minor,
        patch,
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
    std::fs::write(&path, replaced)?;
    Ok(path)
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
            return Some(value.trim_matches(['"', '\'']).to_string());
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
    use std::process::Command;

    fn git(root: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(args)
            .current_dir(root)
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
    fn count_tags_and_commits_drive_minor_and_patch() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        std::fs::write(root.join("Cargo.toml"), "[package]\nversion = \"2.0.0\"\n").unwrap();
        commit(root, "a.txt");
        // No tags yet → minor 0, patch counts all commits.
        assert_eq!(count_git_tags(root).unwrap(), 0);
        let v = compute_version(root).unwrap();
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 0);
        assert!(v.patch >= 1);

        git(root, &["tag", "v2.0.0"]);
        commit(root, "b.txt");
        commit(root, "c.txt");
        assert_eq!(count_git_tags(root).unwrap(), 1);
        assert_eq!(commits_since_last_minor_tag(root).unwrap(), 2);

        let v = compute_version(root).unwrap();
        assert_eq!(
            v,
            Version {
                major: 2,
                minor: 1,
                patch: 2
            }
        );
        assert_eq!(v.to_string(), "2.1.2");
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
        // compute_version would see 1 tag + 2 commits since => 2.1.2.
        // read_version must still report exactly what's on disk: 2.0.0.
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
            contents.contains(
                "devflow-core = { path = \"crates/devflow-core\", version = \"1.7.0\" }"
            ),
            "expected the [workspace.dependencies] self-pin to be rewritten to 1.7.0 \
             alongside [workspace.package] version, got: {contents}"
        );
    }
}
