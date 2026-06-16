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

fn find_version_in_contents(contents: &str, field: &str) -> Option<String> {
    let key = field.rsplit('.').next()?;
    for line in contents.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with(key) {
            continue;
        }
        let (_, value) = trimmed.split_once(['=', ':'])?;
        return Some(value.trim().trim_matches(['"', '\'']).to_string());
    }
    None
}

fn replace_version_in_contents(contents: &str, field: &str, new_version: &str) -> Option<String> {
    let key = field.rsplit('.').next()?;
    let mut changed = false;
    let mut output = String::new();
    for line in contents.lines() {
        let trimmed = line.trim_start();
        if !changed && trimmed.starts_with(key) {
            if let Some((left, value)) = line.split_once('=') {
                let quote = if value.contains('\'') { "'" } else { "\"" };
                output.push_str(left.trim_end());
                output.push_str(" = ");
                output.push_str(quote);
                output.push_str(new_version);
                output.push_str(quote);
                output.push('\n');
                changed = true;
                continue;
            }
            if let Some((left, _value)) = line.split_once(':') {
                output.push_str(left.trim_end());
                output.push_str(": ");
                output.push_str(new_version);
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

    #[test]
    fn bumps_semver_components() {
        assert_eq!(bump("1.2.3", "patch").expect("patch"), "1.2.4");
        assert_eq!(bump("1.2.3", "minor").expect("minor"), "1.3.0");
        assert_eq!(bump("1.2.3", "major").expect("major"), "2.0.0");
    }
}
