//! `release-preflight`: the read-only release checks `scripts/cut-release.sh`
//! runs before and after a release cut.
//!
//! ```text
//! release-preflight check  [PROJECT]   # pre-cut preflight
//! release-preflight verify [PROJECT]   # post-cut verification
//! ```
//!
//! `PROJECT` defaults to the current directory. Every check reads local state
//! only: no `git fetch`, no tag, no publish (`cut-release.sh` fetches before it
//! calls this tool). Each check reports `ok`, `warn` or `fail`; any `fail`
//! makes the run exit 1, a usage error exits 2, and warnings alone exit 0.
//!
//! This is repo tooling, not part of the published crates (`publish = false`).
//! It replaces the `devflow release --check`/`--verify` command, which left the
//! runner in v4 (Phase 52, D-01/D-02), and keeps that command's messages and
//! severities.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// How a check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Ok,
    Warn,
    Fail,
}

/// One check's result: its name, outcome, a one-line detail and, for a warn or
/// a fail, an optional hint telling the operator what to do.
struct Check {
    name: &'static str,
    status: Status,
    detail: String,
    hint: Option<String>,
}

impl Check {
    fn ok(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            status: Status::Ok,
            detail: detail.into(),
            hint: None,
        }
    }

    fn warn(name: &'static str, detail: impl Into<String>, hint: Option<String>) -> Self {
        Self {
            name,
            status: Status::Warn,
            detail: detail.into(),
            hint,
        }
    }

    fn fail(name: &'static str, detail: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            name,
            status: Status::Fail,
            detail: detail.into(),
            hint: Some(hint.into()),
        }
    }
}

const USAGE: &str = "usage: release-preflight <check|verify> [PROJECT]";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let subcommand = args.next();
    let project = args
        .next()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    if let Some(extra) = args.next() {
        eprintln!("release-preflight: unexpected argument {extra:?}\n{USAGE}");
        return ExitCode::from(2);
    }
    match subcommand.as_deref() {
        Some("check") => report("release preflight", &check(&project)),
        Some("verify") => report("release verification", &verify(&project)),
        Some(other) => {
            eprintln!("release-preflight: unknown subcommand {other:?}\n{USAGE}");
            ExitCode::from(2)
        }
        None => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// The pre-cut checks.
fn check(project_root: &Path) -> Vec<Check> {
    vec![check_self_pin(project_root)]
}

/// The post-cut checks.
fn verify(_project_root: &Path) -> Vec<Check> {
    Vec::new()
}

/// Print one line per check (and its hint on a warn or fail), then the verdict.
/// Exit 1 when any check failed.
fn report(what: &str, checks: &[Check]) -> ExitCode {
    for check in checks {
        let icon = match check.status {
            Status::Ok => "✓",
            Status::Warn => "⚠",
            Status::Fail => "✗",
        };
        println!("  {:<32} {icon}  {}", check.name, check.detail);
        if check.status != Status::Ok
            && let Some(hint) = &check.hint
        {
            println!("      — {hint}");
        }
    }
    if checks.iter().any(|check| check.status == Status::Fail) {
        eprintln!("{what} failed — see checks above");
        ExitCode::FAILURE
    } else {
        println!("\n{what} passed");
        ExitCode::SUCCESS
    }
}

/// A `[workspace.dependencies]` entry that points at a workspace member by
/// `path` and pins a `version`.
struct SelfPin {
    name: String,
    version: String,
}

/// The parts of a workspace root `Cargo.toml` the checks read.
struct Workspace {
    /// `[workspace.package] version`; `None` when the manifest is not a
    /// workspace root.
    version: Option<String>,
    pins: Vec<SelfPin>,
}

/// Read `[workspace.package] version` and every local-path
/// `[workspace.dependencies]` self-pin from `project_root/Cargo.toml`.
/// A path dependency with no `version` is not a pin and is skipped. `Err`
/// carries the reason the manifest could not be read or parsed.
fn read_workspace(project_root: &Path) -> Result<Workspace, String> {
    let path = project_root.join("Cargo.toml");
    let contents = std::fs::read_to_string(&path)
        .map_err(|err| format!("could not read Cargo.toml: {err}"))?;
    let manifest: toml::Table =
        toml::from_str(&contents).map_err(|err| format!("could not parse Cargo.toml: {err}"))?;
    let workspace = manifest.get("workspace").and_then(toml::Value::as_table);
    let version = workspace
        .and_then(|ws| ws.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_string);
    let pins = workspace
        .and_then(|ws| ws.get("dependencies"))
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
        .filter_map(|(name, spec)| {
            let spec = spec.as_table()?;
            spec.get("path")?;
            let version = spec.get("version")?.as_str()?;
            Some(SelfPin {
                name: name.clone(),
                version: version.to_string(),
            })
        })
        .collect();
    Ok(Workspace { version, pins })
}

/// Every `[workspace.dependencies]` self-pin must equal `[workspace.package]
/// version`. A path dependency resolves locally and ignores a stale pin, so
/// the drift only shows at `cargo publish`, after `main` is tagged.
fn check_self_pin(project_root: &Path) -> Check {
    const NAME: &str = "self-pin (workspace member versions)";

    let workspace = match read_workspace(project_root) {
        Ok(workspace) => workspace,
        Err(err) => return Check::warn(NAME, err, None),
    };
    let Some(workspace_version) = workspace.version else {
        return Check::warn(
            NAME,
            "not a workspace Cargo.toml (no [workspace.package] version)",
            None,
        );
    };

    let drifted: Vec<String> = workspace
        .pins
        .iter()
        .filter(|pin| pin.version != workspace_version)
        .map(|pin| format!("{} pinned {} != {workspace_version}", pin.name, pin.version))
        .collect();

    if drifted.is_empty() {
        Check::ok(
            NAME,
            format!(
                "{} member pin(s) match {workspace_version}",
                workspace.pins.len()
            ),
        )
    } else {
        Check::fail(
            NAME,
            drifted.join("; "),
            format!(
                "every [workspace.dependencies] self-pin must equal [workspace.package] \
                 version = \"{workspace_version}\""
            ),
        )
    }
}
