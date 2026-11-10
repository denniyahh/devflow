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

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use devflow_core::git::{self, AncestorStatus};

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
    vec![
        check_self_pin(project_root),
        check_divergence(project_root),
        check_publish_order(project_root),
        check_changelog_version(project_root),
    ]
}

/// The post-cut checks: the invariants `check` cannot see, which were the
/// easy-to-miss manual steps of past cuts.
fn verify(project_root: &Path) -> Vec<Check> {
    vec![
        check_tag_on_main(project_root),
        check_sync_done(project_root),
    ]
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

/// The workspace version, or the warn result that explains why there is none.
fn workspace_version(name: &'static str, project_root: &Path) -> Result<String, Check> {
    let workspace = read_workspace(project_root).map_err(|err| Check::warn(name, err, None))?;
    workspace.version.ok_or_else(|| {
        Check::warn(
            name,
            "not a workspace Cargo.toml (no [workspace.package] version)",
            None,
        )
    })
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

/// `origin/main` must be an ancestor of `HEAD`, so that
/// `scripts/sync-main-to-develop.sh` would be a no-op. Reads already-fetched
/// refs only and never fetches.
fn check_divergence(project_root: &Path) -> Check {
    const NAME: &str = "develop/main divergence (origin/main ancestor)";
    match git::origin_main_ancestor_status(project_root) {
        AncestorStatus::Ancestor => Check::ok(
            NAME,
            "origin/main is an ancestor of HEAD — sync would be a no-op",
        ),
        AncestorStatus::Diverged => Check::fail(
            NAME,
            "origin/main is NOT an ancestor of HEAD — develop has diverged",
            "run scripts/sync-main-to-develop.sh before cutting the next release PR",
        ),
        AncestorStatus::RefAbsent => Check::warn(
            NAME,
            "origin/main not fetched — cannot determine divergence",
            Some("run `git fetch` first, then re-run this check".into()),
        ),
    }
}

/// crates.io needs `devflow-core` live before `devflow`, because publishing a
/// path dependency verifies against the registry. The order is derived from
/// the workspace's members and their dependencies (unpublished members such
/// as this tool are left out), never stated from a hardcoded list.
fn check_publish_order(project_root: &Path) -> Check {
    const NAME: &str = "crates.io publish order";
    let order = git::publish_order(project_root);
    if order.is_empty() {
        return Check::warn(NAME, "could not determine workspace publish order", None);
    }
    Check::ok(NAME, format!("publish in order: {}", order.join(" -> ")))
}

/// `CHANGELOG.md`'s topmost `## <version>` heading must equal the workspace
/// version, so a forgotten bump cannot ship notes under the old version.
fn check_changelog_version(project_root: &Path) -> Check {
    const NAME: &str = "changelog version (matches workspace)";

    let workspace_version = match workspace_version(NAME, project_root) {
        Ok(version) => version,
        Err(check) => return check,
    };
    let changelog = match std::fs::read_to_string(project_root.join("CHANGELOG.md")) {
        Ok(contents) => contents,
        Err(err) => {
            return Check::warn(NAME, format!("could not read CHANGELOG.md: {err}"), None);
        }
    };
    let Some(changelog_version) = top_changelog_version(&changelog) else {
        return Check::warn(
            NAME,
            "no `## <version>` heading found in CHANGELOG.md",
            None,
        );
    };

    if changelog_version == workspace_version {
        return Check::ok(
            NAME,
            format!("changelog {changelog_version} matches workspace"),
        );
    }
    let direction = match compare_versions(&changelog_version, &workspace_version) {
        Some(Ordering::Greater) => "changelog ahead of Cargo.toml (version bump not yet applied)",
        Some(Ordering::Less) => "Cargo.toml ahead of changelog (release notes missing)",
        _ => "direction undetermined (equal-numeric or unparseable version)",
    };
    Check::fail(
        NAME,
        format!("changelog {changelog_version} != workspace {workspace_version} — {direction}"),
        direction,
    )
}

/// The version in the first `## ` heading that names one. A version is a
/// leading digit, a dot, then another digit, so `## [2.5.0]` and `## v2.5.0`
/// parse while a numbered section such as `## 1. Overview` does not.
fn top_changelog_version(changelog: &str) -> Option<String> {
    changelog.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("## ")?;
        let word = rest.split_whitespace().next()?;
        let version = word
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim_start_matches('v');
        let dot = version.find('.')?;
        let bytes = version.as_bytes();
        (bytes.first().is_some_and(u8::is_ascii_digit)
            && bytes.get(dot + 1).is_some_and(u8::is_ascii_digit))
        .then(|| version.to_string())
    })
}

/// Compare two `X.Y.Z` version strings component-wise, with semver's
/// prerelease ordering (`2.5.0` > `2.5.0-rc.1`). `None` when either is
/// unparseable.
fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    let components = |v: &str| -> Option<(Vec<u32>, bool)> {
        let parts = v
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .take(3)
            .map(|s| s.parse().ok())
            .collect::<Option<Vec<_>>>()?;
        if parts.is_empty() {
            return None;
        }
        Some((parts, v.contains('-')))
    };
    let (a, a_pre) = components(a)?;
    let (b, b_pre) = components(b)?;
    let ordering = a.cmp(&b).then(match (a_pre, b_pre) {
        (false, true) => Ordering::Greater,
        (true, false) => Ordering::Less,
        _ => Ordering::Equal,
    });
    Some(ordering)
}

/// The release tag `v{workspace version}` must point at a commit on
/// `origin/main`. A tag on the develop release commit (the v2.5.0 mistake) is
/// not an ancestor of main's squash-merge lineage.
fn check_tag_on_main(project_root: &Path) -> Check {
    const NAME: &str = "release tag on main";

    let workspace_version = match read_workspace(project_root) {
        Ok(Workspace {
            version: Some(version),
            ..
        }) => version,
        Ok(_) => {
            return Check::warn(
                NAME,
                "no [workspace.package] version to derive the tag from",
                None,
            );
        }
        Err(err) => return Check::warn(NAME, err, None),
    };
    let tag = format!("v{workspace_version}");

    let tag_exists = git::git_command(project_root)
        .args(["rev-parse", "--verify", "--quiet", &tag])
        .output()
        .is_ok_and(|out| out.status.success());
    if !tag_exists {
        return Check::fail(
            NAME,
            format!("tag {tag} does not exist"),
            "tag the release commit on main before verifying",
        );
    }

    match git::ref_is_ancestor(project_root, &tag, "origin/main") {
        AncestorStatus::Ancestor => Check::ok(NAME, format!("{tag} is on origin/main")),
        AncestorStatus::Diverged => Check::fail(
            NAME,
            format!("{tag} is NOT an ancestor of origin/main — tagged the wrong branch"),
            "re-tag on main:  git -c user.signingkey=\"$(git config --get \
             devflow.releaseSigningKey)\" tag -s -f vX.Y.Z origin/main",
        ),
        AncestorStatus::RefAbsent => Check::warn(
            NAME,
            "origin/main not fetched — cannot compare tag placement",
            Some("run `git fetch` first, then re-run".into()),
        ),
    }
}

/// After a release `origin/main` must be an ancestor of `origin/develop` (the
/// `scripts/sync-main-to-develop.sh` step); skipping it makes the next release
/// PR conflict against a stale merge-base.
fn check_sync_done(project_root: &Path) -> Check {
    const NAME: &str = "main→develop sync";
    match git::ref_is_ancestor(project_root, "origin/main", "origin/develop") {
        AncestorStatus::Ancestor => Check::ok(NAME, "origin/main is an ancestor of origin/develop"),
        AncestorStatus::Diverged => Check::fail(
            NAME,
            "origin/main is NOT an ancestor of origin/develop — sync was skipped",
            "run scripts/sync-main-to-develop.sh, then PR the merge commit (not squash)",
        ),
        AncestorStatus::RefAbsent => Check::warn(
            NAME,
            "origin/main or origin/develop not fetched — cannot check sync",
            Some("run `git fetch` first, then re-run".into()),
        ),
    }
}
