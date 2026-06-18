//! Version reading and bumping helpers.

use crate::config::VersionConfig;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Errors produced by version operations.
#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    /// Filesystem operation failed.
    #[error("version file I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// Version field could not be found or parsed.
    #[error("version parse failed: {0}")]
    Parse(String),
    /// Build-number command failed.
    #[error("build number failed: {0}")]
    BuildNumber(String),
}

/// Read the configured version string.
pub fn read_version(project_root: &Path, config: &VersionConfig) -> Result<String, VersionError> {
    let path = project_root.join(&config.file);
    let contents = std::fs::read_to_string(&path)?;
    find_version_in_contents(&contents, &config.field)
        .ok_or_else(|| VersionError::Parse(format!("field `{}` not found", config.field)))
}

/// Bump a semantic version component.
pub fn bump(version: &str, component: &str) -> Result<String, VersionError> {
    if component == "none" {
        return Ok(version.to_string());
    }
    let core = version.split(['+', '-']).next().unwrap_or(version);
    let mut parts = core
        .split('.')
        .map(|part| part.parse::<u64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| VersionError::Parse(format!("invalid semver `{version}`: {err}")))?;
    if parts.len() != 3 {
        return Err(VersionError::Parse(format!(
            "expected major.minor.patch, got `{version}`"
        )));
    }

    match component {
        "major" => {
            parts[0] += 1;
            parts[1] = 0;
            parts[2] = 0;
        }
        "minor" => {
            parts[1] += 1;
            parts[2] = 0;
        }
        "patch" => parts[2] += 1,
        other => {
            return Err(VersionError::Parse(format!(
                "unsupported bump component `{other}`"
            )));
        }
    }
    Ok(format!("{}.{}.{}", parts[0], parts[1], parts[2]))
}

/// Generate a build number from git commit count or current timestamp.
pub fn build_number(project_root: &Path, config: &VersionConfig) -> Result<String, VersionError> {
    match config.build_number.as_str() {
        "git" => {
            let output = Command::new("git")
                .args(["rev-list", "--count", "HEAD"])
                .current_dir(project_root)
                .output()
                .map_err(|err| VersionError::BuildNumber(err.to_string()))?;
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                Err(VersionError::BuildNumber(
                    String::from_utf8_lossy(&output.stderr).trim().to_string(),
                ))
            }
        }
        "timestamp" => match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => Ok(duration.as_secs().to_string()),
            Err(err) => Err(VersionError::BuildNumber(err.to_string())),
        },
        other => Err(VersionError::BuildNumber(format!(
            "unsupported build number source `{other}`"
        ))),
    }
}

/// Write the configured version string back to the configured file.
pub fn write_version(
    project_root: &Path,
    config: &VersionConfig,
    new_version: &str,
) -> Result<PathBuf, VersionError> {
    let path = project_root.join(&config.file);
    let contents = std::fs::read_to_string(&path)?;
    let replaced = replace_version_in_contents(&contents, &config.field, new_version)
        .ok_or_else(|| VersionError::Parse(format!("field `{}` not found", config.field)))?;
    std::fs::write(&path, replaced)?;
    Ok(path)
}

/// Split a dotted field path into its TOML section path and the final key.
///
/// `workspace.package.version` -> (`workspace.package`, `version`), matching the
/// `[workspace.package]` table header. A field with no dot (e.g. `version`)
/// targets the root/top-level scope, which is also how flat YAML files behave.
fn split_field(field: &str) -> (&str, &str) {
    match field.rsplit_once('.') {
        Some((section, key)) => (section, key),
        None => ("", field),
    }
}

/// Return the dotted table path for a TOML section header line, if any.
///
/// `[workspace.package]` -> `workspace.package`. Returns `None` for non-header
/// lines so the caller leaves the current section untouched.
fn parse_section_header(trimmed: &str) -> Option<&str> {
    let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
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
        // Strip JSON-style quotes from the key (e.g., `"version": "1.0"`)
        let lhs_key = lhs.trim().trim_matches('"').trim_matches('\'');
        if lhs_key != key {
            continue;
        }
        return Some(value.trim().trim_matches(['"', '\'']).to_string());
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
        if !changed && current == section {
            if let Some((left, value)) = line.split_once(['=', ':']) {
                let left_key = left.trim().trim_matches('"').trim_matches('\'');
                if left_key == key {
                    let separator: &str = if trimmed.contains('=') { " = " } else { ": " };
                    let quote_char: &str = if value.trim().starts_with('\'') {
                        "'"
                    } else {
                        "\""
                    };
                    let needs_quote =
                        value.trim().starts_with('"') || value.trim().starts_with('\'');
                    output.push_str(left.trim_end());
                    output.push_str(separator);
                    if needs_quote {
                        output.push_str(quote_char);
                        output.push_str(new_version);
                        output.push_str(quote_char);
                    } else {
                        output.push_str(new_version);
                    }
                    output.push('\n');
                    changed = true;
                    continue;
                }
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

    fn version_config(file: &str, field: &str) -> VersionConfig {
        VersionConfig {
            scheme: "semver".into(),
            file: file.into(),
            field: field.into(),
            build_number: "git".into(),
        }
    }

    #[test]
    fn bumps_semver_components() {
        assert_eq!(bump("1.2.3", "patch").expect("patch"), "1.2.4");
        assert_eq!(bump("1.2.3", "minor").expect("minor"), "1.3.0");
        assert_eq!(bump("1.2.3", "major").expect("major"), "2.0.0");
    }

    #[test]
    fn bump_strips_prerelease_and_build_metadata() {
        assert_eq!(bump("1.2.3-rc.1", "patch").expect("patch"), "1.2.4");
        assert_eq!(bump("1.2.3+build.9", "minor").expect("minor"), "1.3.0");
    }

    #[test]
    fn bump_none_is_identity() {
        assert_eq!(bump("1.2.3", "none").expect("none"), "1.2.3");
        // "none" short-circuits before any parsing.
        assert_eq!(
            bump("not-a-version", "none").expect("none"),
            "not-a-version"
        );
    }

    #[test]
    fn bump_rejects_non_numeric_parts() {
        assert!(bump("1.x.3", "patch").is_err());
    }

    #[test]
    fn bump_rejects_wrong_part_count() {
        assert!(bump("1.2", "patch").is_err());
        assert!(bump("1.2.3.4", "patch").is_err());
    }

    #[test]
    fn bump_rejects_unknown_component() {
        let err = bump("1.2.3", "epoch").unwrap_err();
        assert!(err.to_string().contains("epoch"));
    }

    #[test]
    fn read_version_from_toml_style_field() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.5.0\"\n",
        )
        .unwrap();
        let cfg = version_config("Cargo.toml", "package.version");
        assert_eq!(read_version(dir.path(), &cfg).unwrap(), "0.5.0");
    }

    /// A workspace root Cargo.toml: version lives under `[workspace.package]`,
    /// and `[workspace.dependencies]` carries unrelated inline `version` keys.
    const WORKSPACE_CARGO_TOML: &str = "\
[workspace]
members = [\"crates/core\"]

[workspace.package]
version = \"0.5.0\"
edition = \"2024\"

[workspace.dependencies]
serde = { version = \"1\", features = [\"derive\"] }
clap = \"4\"
";

    #[test]
    fn read_version_from_workspace_package_section() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), WORKSPACE_CARGO_TOML).unwrap();
        let cfg = version_config("Cargo.toml", "workspace.package.version");
        // Must return the workspace package version, not a dependency's version.
        assert_eq!(read_version(dir.path(), &cfg).unwrap(), "0.5.0");
    }

    #[test]
    fn read_version_ignores_dependency_versions_in_wrong_section() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), WORKSPACE_CARGO_TOML).unwrap();
        // Asking for a non-existent root [package] version must not fall through
        // to the dependency `version = "1"` line.
        let cfg = version_config("Cargo.toml", "package.version");
        assert!(matches!(
            read_version(dir.path(), &cfg).unwrap_err(),
            VersionError::Parse(_)
        ));
    }

    #[test]
    fn write_version_in_workspace_package_leaves_dependencies_intact() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Cargo.toml");
        std::fs::write(&file, WORKSPACE_CARGO_TOML).unwrap();
        let cfg = version_config("Cargo.toml", "workspace.package.version");

        write_version(dir.path(), &cfg, "0.6.0").unwrap();
        let contents = std::fs::read_to_string(&file).unwrap();

        assert!(contents.contains("version = \"0.6.0\""));
        // The dependency pin must be untouched.
        assert!(contents.contains("serde = { version = \"1\""));
        assert_eq!(read_version(dir.path(), &cfg).unwrap(), "0.6.0");
    }

    #[test]
    fn read_version_from_root_package_section() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"1.2.3\"\n\n[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();
        let cfg = version_config("Cargo.toml", "package.version");
        assert_eq!(read_version(dir.path(), &cfg).unwrap(), "1.2.3");
    }

    #[test]
    fn read_version_from_yaml_style_field() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("info.yaml"), "name: x\nversion: 1.4.2\n").unwrap();
        let cfg = version_config("info.yaml", "version");
        assert_eq!(read_version(dir.path(), &cfg).unwrap(), "1.4.2");
    }

    #[test]
    fn read_version_errors_when_field_missing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        let cfg = version_config("Cargo.toml", "package.version");
        let err = read_version(dir.path(), &cfg).unwrap_err();
        assert!(matches!(err, VersionError::Parse(_)));
    }

    #[test]
    fn read_version_errors_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = version_config("nope.toml", "version");
        assert!(matches!(
            read_version(dir.path(), &cfg).unwrap_err(),
            VersionError::Io(_)
        ));
    }

    #[test]
    fn write_version_replaces_and_preserves_quote_style() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Cargo.toml");
        std::fs::write(&file, "[package]\nversion = \"0.5.0\"\n").unwrap();
        let cfg = version_config("Cargo.toml", "package.version");

        let written = write_version(dir.path(), &cfg, "0.6.0").unwrap();
        assert_eq!(written, file);

        let contents = std::fs::read_to_string(&file).unwrap();
        assert!(contents.contains("version = \"0.6.0\""));
        // Round-trips: reading back gives the new version.
        assert_eq!(read_version(dir.path(), &cfg).unwrap(), "0.6.0");
    }

    #[test]
    fn write_version_handles_yaml_colon_field() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("info.yaml");
        std::fs::write(&file, "version: 1.0.0\n").unwrap();
        let cfg = version_config("info.yaml", "version");

        write_version(dir.path(), &cfg, "1.0.1").unwrap();
        let contents = std::fs::read_to_string(&file).unwrap();
        assert!(contents.contains("version: 1.0.1"));
    }

    #[test]
    fn write_version_errors_when_field_missing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        let cfg = version_config("Cargo.toml", "package.version");
        assert!(matches!(
            write_version(dir.path(), &cfg, "1.0.0").unwrap_err(),
            VersionError::Parse(_)
        ));
    }

    #[test]
    fn read_and_write_package_json_version() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("package.json");
        std::fs::write(
            &file,
            "{\n  \"name\": \"myapp\",\n  \"version\": \"2.1.0\"\n}\n",
        )
        .unwrap();
        let cfg = version_config("package.json", "version");
        assert_eq!(read_version(dir.path(), &cfg).unwrap(), "2.1.0");
        write_version(dir.path(), &cfg, "3.0.0").unwrap();
        let contents = std::fs::read_to_string(&file).unwrap();
        assert!(contents.contains("\"version\": \"3.0.0\""));
        assert_eq!(read_version(dir.path(), &cfg).unwrap(), "3.0.0");
    }

    #[test]
    fn build_number_timestamp_is_numeric() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = version_config("Cargo.toml", "version");
        cfg.build_number = "timestamp".into();
        let out = build_number(dir.path(), &cfg).unwrap();
        assert!(out.parse::<u64>().is_ok());
        assert!(out.parse::<u64>().unwrap() > 0);
    }

    #[test]
    fn build_number_rejects_unknown_source() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = version_config("Cargo.toml", "version");
        cfg.build_number = "moon-phase".into();
        let err = build_number(dir.path(), &cfg).unwrap_err();
        assert!(err.to_string().contains("moon-phase"));
    }

    #[test]
    fn build_number_git_counts_commits() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let run = |args: &[&str]| {
            let ok = Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?} failed");
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test"]);
        run(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("a.txt"), "hi").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "first"]);

        let cfg = version_config("Cargo.toml", "version");
        assert_eq!(build_number(root, &cfg).unwrap(), "1");
    }
}
