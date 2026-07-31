//! Release-cut observation oracles (29a) — pure classification logic plus
//! thin, read-only I/O wrappers around `git`/`gh`/`curl`.
//!
//! Every result is [`Observation`], a three-valued enum. `Unreachable` is
//! never conflated with `Absent` (RD-8's "unreachable != absent" rule): a
//! network partition, an unauthenticated `gh`, or a tool that fails to run
//! must always surface as `Unreachable`, carrying the real tool's own
//! failure text — never silently reported as "not done yet." This mirrors
//! `git.rs`'s existing `SigningViability::Unknown { reason }` /
//! `AncestorStatus` convention; nothing in this module invents a new oracle
//! shape.
//!
//! This module performs **no writes** — every function here only reads
//! remote or local state and reports what it observed.

use crate::git::git_command;
use std::path::Path;
use std::process::Command;

/// Three-valued oracle result for every observation in this module.
///
/// Every arm carries a `String` — never a bare unit variant — so the CLI
/// layer always has something to print. Two-valued (boolean) results are
/// forbidden anywhere in this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    /// The observed state is confirmed present.
    Present { detail: String },
    /// The observed state is confirmed absent (not "unknown" — a real
    /// negative answer from a reachable oracle).
    Absent { detail: String },
    /// The oracle could not be reached, or its answer could not be
    /// classified. Never collapsed into [`Absent`](Self::Absent) — a
    /// network or tool failure is not evidence of absence.
    Unreachable { reason: String },
}

/// The six release-cut questions this phase answers, in the exact order the
/// release sequence itself runs: version bump -> changelog -> release PR ->
/// signed tag -> sync back -> publish.
///
/// Only [`SignedTagPresent`](Self::SignedTagPresent) is wired to a real
/// oracle by this plan; [`CratesPublished`](Self::CratesPublished) is wired
/// by Task 2. The remaining four are wired in `29-02-PLAN.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStep {
    VersionBumped,
    ChangelogWritten,
    ReleasePrMerged,
    SignedTagPresent,
    SyncMerged,
    CratesPublished,
}

impl ReleaseStep {
    /// All six steps, in release-sequence order.
    pub const ALL: [ReleaseStep; 6] = [
        ReleaseStep::VersionBumped,
        ReleaseStep::ChangelogWritten,
        ReleaseStep::ReleasePrMerged,
        ReleaseStep::SignedTagPresent,
        ReleaseStep::SyncMerged,
        ReleaseStep::CratesPublished,
    ];

    /// A short human label for the `release status` report.
    pub fn label(self) -> &'static str {
        match self {
            ReleaseStep::VersionBumped => "version bumped on develop",
            ReleaseStep::ChangelogWritten => "changelog written on develop",
            ReleaseStep::ReleasePrMerged => "release PR merged to main",
            ReleaseStep::SignedTagPresent => "signed tag on origin",
            ReleaseStep::SyncMerged => "main synced back into develop",
            ReleaseStep::CratesPublished => "crates published",
        }
    }
}

/// A parsed `git ls-remote --tags` result for one tag name: the tag's own
/// object sha, and (if present) the peeled `^{}` sha that proves the tag is
/// annotated rather than lightweight.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TagRefs {
    pub object: Option<String>,
    pub peeled: Option<String>,
}

impl TagRefs {
    /// `true` when a peeled entry was found — proves the tag is annotated,
    /// not that it is signed.
    pub fn is_annotated(&self) -> bool {
        self.peeled.is_some()
    }

    /// `true` when the tag's own ref was found at all.
    pub fn exists(&self) -> bool {
        self.object.is_some()
    }
}

/// Whether an annotated tag object carries a verified signature, as
/// determined by `gh api .../git/tags/<sha> --jq .verification.verified`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagSignature {
    Present,
    Missing,
    /// The GitHub API call did not answer (spawn failure, non-zero exit,
    /// unparseable output). Must propagate to [`Observation::Unreachable`],
    /// never collapse to [`Missing`](Self::Missing).
    Undetermined {
        reason: String,
    },
}

/// Parse `git ls-remote --tags` stdout for a specific tag. Pure — no I/O, no
/// `Command`, fully unit-testable. Tab-separated `<sha>\t<ref>` lines;
/// matches `refs/tags/<tag>` for the object sha and `refs/tags/<tag>^{}` for
/// the peeled sha. Every other line is ignored.
pub fn classify_tag_refs(ls_remote_stdout: &str, tag: &str) -> TagRefs {
    let want_object = format!("refs/tags/{tag}");
    let want_peeled = format!("refs/tags/{tag}^{{}}");
    let mut refs = TagRefs::default();
    for line in ls_remote_stdout.lines() {
        let Some((sha, refname)) = line.split_once('\t') else {
            continue;
        };
        if refname == want_object {
            refs.object = Some(sha.to_string());
        } else if refname == want_peeled {
            refs.peeled = Some(sha.to_string());
        }
    }
    refs
}

/// Classify a parsed [`TagRefs`] plus its resolved [`TagSignature`] into an
/// [`Observation`]. Pure — no I/O. Absent when the ref does not exist;
/// Absent (naming "lightweight") when it exists but is not annotated; then
/// delegates to the signature arm.
pub fn classify_signed_tag(refs: &TagRefs, signature: TagSignature) -> Observation {
    if !refs.exists() {
        return Observation::Absent {
            detail: "tag not found on origin".into(),
        };
    }
    if !refs.is_annotated() {
        return Observation::Absent {
            detail: "tag exists but is lightweight, not an annotated signed tag".into(),
        };
    }
    match signature {
        TagSignature::Present => Observation::Present {
            detail: "tag is annotated and signed".into(),
        },
        TagSignature::Missing => Observation::Absent {
            detail: "tag is annotated but carries no signature (unsigned)".into(),
        },
        TagSignature::Undetermined { reason } => Observation::Unreachable { reason },
    }
}

/// I/O wrapper: is `v<version>` present, annotated, and signed on `origin`?
/// Runs `git ls-remote --tags origin` through [`git_command`] (never a
/// direct, unscrubbed git invocation), passing the tag refspec as an
/// argument via `Command::args` — never string-interpolated into a shell
/// (T-29-01).
pub fn signed_tag_on_remote(project_root: &Path, version: &str) -> Observation {
    let tag = format!("v{version}");
    let pattern = format!("refs/tags/{tag}*");
    let output = git_command(project_root)
        .args(["ls-remote", "--tags", "origin", &pattern])
        .output();
    let output = match output {
        Ok(out) if out.status.success() => out,
        Ok(out) => {
            return Observation::Unreachable {
                reason: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            };
        }
        Err(err) => {
            return Observation::Unreachable {
                reason: err.to_string(),
            };
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let refs = classify_tag_refs(&stdout, &tag);
    // The tag OBJECT's own sha (the unpeeled `refs.object` entry) is what
    // `gh api .../git/tags/<sha>` expects — the peeled `^{}` entry
    // dereferences to the *commit* the tag object points to, a different
    // object with no `.verification` field of its own.
    let signature = if refs.is_annotated() {
        match &refs.object {
            Some(sha) => tag_signature_via_gh(project_root, sha),
            None => TagSignature::Missing,
        }
    } else {
        // Not annotated (or absent) — `classify_signed_tag` short-circuits
        // before consulting `signature` in that case, so this value is
        // never actually read; `Missing` is the harmless placeholder.
        TagSignature::Missing
    };
    classify_signed_tag(&refs, signature)
}

/// `gh api repos/{owner}/{repo}/git/tags/<sha> --jq .verification.verified`,
/// pinned to `project_root` so `gh`'s `{owner}`/`{repo}` placeholders
/// resolve to the project under observation rather than to whatever
/// repository the ambient shell happens to be in (T-29-10). `gh`
/// substitutes those placeholders from the repo context itself; the remote
/// URL is never parsed here.
///
/// The reason string on [`TagSignature::Undetermined`] describes the
/// failure class and may quote `gh`'s exit status, but never includes `gh`'s
/// raw stdout or stderr (T-17-13, inherited from `preflight.rs`, not
/// re-decided here).
fn tag_signature_via_gh(project_root: &Path, tag_object_sha: &str) -> TagSignature {
    let output = Command::new("gh")
        .current_dir(project_root)
        .args([
            "api",
            &format!("repos/{{owner}}/{{repo}}/git/tags/{tag_object_sha}"),
            "--jq",
            ".verification.verified",
        ])
        .output();
    match output {
        Ok(out) if out.status.success() => match String::from_utf8_lossy(&out.stdout).trim() {
            "true" => TagSignature::Present,
            "false" => TagSignature::Missing,
            _ => TagSignature::Undetermined {
                reason: "gh api returned an unexpected verification value".into(),
            },
        },
        Ok(out) => TagSignature::Undetermined {
            reason: format!("gh api exited with status {}", out.status),
        },
        Err(err) => TagSignature::Undetermined {
            reason: format!("failed to spawn gh: {}", err.kind()),
        },
    }
}

/// Classify a `curl -w '%{http_code}'` result into an [`Observation`]. Pure
/// — no I/O. Exactly `"200"` is Present, exactly `"404"` is Absent, and
/// every other value — including empty, non-numeric, `"000"` (curl's code
/// for "could not connect"), and any other 3xx/4xx/5xx — is Unreachable
/// with a reason naming the code that was seen.
pub fn classify_http_status(code: &str) -> Observation {
    match code {
        "200" => Observation::Present {
            detail: "published (HTTP 200)".into(),
        },
        "404" => Observation::Absent {
            detail: "not published (HTTP 404)".into(),
        },
        "" => Observation::Unreachable {
            reason: "curl returned an empty HTTP status".into(),
        },
        other => Observation::Unreachable {
            reason: format!("unexpected HTTP status {other}"),
        },
    }
}

/// Fold each workspace member's own crates.io [`Observation`] into one
/// answer for "are all published crates live at this version." Pure — no
/// I/O. `Unreachable` dominates everything (an unreachable registry can
/// never be summarized as "not published"); otherwise `Absent` dominates
/// `Present`; all-`Present` is `Present`; an empty slice is `Unreachable`
/// (an empty `publish_order` means the workspace manifest could not be
/// read, which is a failure to observe, not an answer).
pub fn combine_crate_observations(per_crate: &[(String, Observation)]) -> Observation {
    if per_crate.is_empty() {
        return Observation::Unreachable {
            reason: "no publishable crates found in this workspace".into(),
        };
    }

    let unreachable: Vec<&str> = per_crate
        .iter()
        .filter_map(|(_, obs)| match obs {
            Observation::Unreachable { reason } => Some(reason.as_str()),
            _ => None,
        })
        .collect();
    if !unreachable.is_empty() {
        return Observation::Unreachable {
            reason: format!("could not observe every crate: {}", unreachable.join("; ")),
        };
    }

    let absent: Vec<&str> = per_crate
        .iter()
        .filter_map(|(name, obs)| match obs {
            Observation::Absent { .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    if !absent.is_empty() {
        return Observation::Absent {
            detail: format!("not yet published: {}", absent.join(", ")),
        };
    }

    let present: Vec<&str> = per_crate.iter().map(|(name, _)| name.as_str()).collect();
    Observation::Present {
        detail: format!("published: {}", present.join(", ")),
    }
}

/// `curl -s -o /dev/null -w '%{http_code}' <crates.io API URL>`, pinned to
/// `project_root`. Builds the URL with `format!` and passes it as a single
/// `Command::args` element — never through a shell, never with any
/// interpolation into a shell string (T-29-01). Uses the `/api/v1` JSON
/// endpoint, never the registry's CDN-cached sparse-index mirror, which
/// lags a real publish by seconds and would reintroduce the exact
/// absent-versus-unreachable ambiguity this phase exists to eliminate (see
/// 29-RESEARCH.md's Pitfall 2). A
/// spawn error or a non-zero `curl` exit returns `Err` with the failure
/// text; the caller converts that to `Unreachable`.
fn crate_version_http_status(
    project_root: &Path,
    name: &str,
    version: &str,
) -> Result<String, String> {
    let url = format!("https://crates.io/api/v1/crates/{name}/{version}");
    let output = Command::new("curl")
        .current_dir(project_root)
        .args([
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "--max-time",
            "20",
            "-A",
            "devflow-release-executor (https://github.com/denniyahh/devflow)",
            &url,
        ])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        }
        Ok(out) => Err(format!("curl exited with status {}", out.status)),
        Err(err) => Err(format!("failed to spawn curl: {err}")),
    }
}

/// I/O wrapper: are all of this workspace's publishable crates live on
/// crates.io at `version`? Calls [`crate::git::publish_order`] for the
/// member list — never hardcodes the crate names — queries each member in
/// that order, classifies each with [`classify_http_status`], and folds
/// with [`combine_crate_observations`].
pub fn crates_published(project_root: &Path, version: &str) -> Observation {
    let members = crate::git::publish_order(project_root);
    let per_crate: Vec<(String, Observation)> = members
        .into_iter()
        .map(|name| {
            let observation = match crate_version_http_status(project_root, &name, version) {
                Ok(code) => classify_http_status(&code),
                Err(reason) => Observation::Unreachable { reason },
            };
            (name, observation)
        })
        .collect();
    combine_crate_observations(&per_crate)
}

// -- 29-02 Task 1: content-at-ref oracles ---------------------------------

/// Read a file's raw content at a specific git ref via `gh api
/// repos/{owner}/{repo}/contents/<path>?ref=<git_ref>`, requesting the raw
/// bytes (`Accept: application/vnd.github.raw`) so the response is the
/// file's literal content rather than an encoded JSON envelope requiring a
/// decode step.
///
/// Reads GitHub's copy of the file rather than fetching into the local
/// object database on purpose: a `git fetch` writes `FETCH_HEAD` and can
/// opportunistically update remote-tracking refs, which would make this
/// observer a mutator (T-29-12). `gh` is pinned to `project_root` via
/// `.current_dir` (T-29-10) so its `{owner}`/`{repo}` placeholders resolve
/// to the project under observation, not the ambient shell's repository.
/// Every argument is a discrete `Command::args` element — never
/// string-interpolated into a shell (T-29-01). On spawn failure, non-zero
/// exit, or empty stdout, returns `Err` with a failure-class description
/// naming `path`/`git_ref` (harmless context, not sensitive) but never
/// embedding `gh`'s raw stdout or stderr (T-29-03, T-17-13). The caller
/// converts every `Err` into [`Observation::Unreachable`].
fn file_at_ref(project_root: &Path, path: &str, git_ref: &str) -> Result<String, String> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/contents/{path}?ref={git_ref}");
    let output = Command::new("gh")
        .current_dir(project_root)
        .args(["api", &endpoint, "-H", "Accept: application/vnd.github.raw"])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let content = String::from_utf8_lossy(&out.stdout).to_string();
            if content.is_empty() {
                Err(format!("gh returned empty content for {path}@{git_ref}"))
            } else {
                Ok(content)
            }
        }
        Ok(out) => Err(format!(
            "gh api exited with status {} fetching {path}@{git_ref}",
            out.status
        )),
        Err(err) => Err(format!(
            "failed to spawn gh fetching {path}@{git_ref}: {}",
            err.kind()
        )),
    }
}

/// Classify a workspace root `Cargo.toml`'s text against an `expected`
/// version. Pure — no I/O. Calls
/// [`crate::version::read_workspace_self_pins`] — the same hand-rolled
/// workspace-manifest parser the existing local self-pin check already
/// uses, so remote and local agree by construction.
///
/// `Unreachable` when the manifest has no `[workspace.package]` version at
/// all (empty or unparseable input is a failure to observe, not evidence of
/// absence). `Absent` when the workspace version doesn't match `expected`
/// (detail names both), or when it matches but any local-path self-pin
/// doesn't (detail names the drifted pin) — CONTRIBUTING.md step 1 makes the
/// two-place bump a single fact, and "bumped only the first" is the
/// documented easy miss, so it must not read as done. `Present` only when
/// the workspace version and every self-pin equal `expected`.
pub fn classify_manifest_version(cargo_toml: &str, expected: &str) -> Observation {
    let (workspace_version, pins) = crate::version::read_workspace_self_pins(cargo_toml);
    let Some(workspace_version) = workspace_version else {
        return Observation::Unreachable {
            reason: "manifest has no [workspace.package] version — empty or unparseable".into(),
        };
    };
    if workspace_version != expected {
        return Observation::Absent {
            detail: format!("workspace version is {workspace_version}, expected {expected}"),
        };
    }
    for pin in &pins {
        if pin.version != expected {
            return Observation::Absent {
                detail: format!(
                    "workspace version matches but self-pin `{}` is {}, expected {expected}",
                    pin.name, pin.version
                ),
            };
        }
    }
    Observation::Present {
        detail: format!("workspace version and all self-pins match {expected}"),
    }
}

/// Classify a `CHANGELOG.md`'s text for a top-level `## <version>` heading.
/// Pure — no I/O. `Unreachable` on empty input (a failure to observe, not
/// evidence of absence). `Present` when any line, after trimming leading
/// whitespace, begins with `## ` followed by `version` as its first
/// whitespace-delimited token — so `## 2.3.0` and `## 2.3.0 - 2026-08-01`
/// both count, while `##2.3.0` (no space after the hashes) and a bare
/// inline mention do not. `Absent` otherwise.
pub fn classify_changelog_heading(changelog: &str, version: &str) -> Observation {
    if changelog.is_empty() {
        return Observation::Unreachable {
            reason: "changelog content is empty".into(),
        };
    }
    for line in changelog.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("## ") {
            let first_token = rest.split_whitespace().next().unwrap_or("");
            if first_token == version {
                return Observation::Present {
                    detail: format!("found heading `## {version}`"),
                };
            }
        }
    }
    Observation::Absent {
        detail: format!("no `## {version}` heading found"),
    }
}

/// I/O wrapper: is `Cargo.toml` on `develop` bumped to `version` in both
/// required places? Reads GitHub's copy of the file via [`file_at_ref`] —
/// never a local fetch — then classifies it with [`classify_manifest_version`].
pub fn version_bumped_on_develop(project_root: &Path, version: &str) -> Observation {
    match file_at_ref(project_root, "Cargo.toml", "develop") {
        Ok(contents) => classify_manifest_version(&contents, version),
        Err(reason) => Observation::Unreachable { reason },
    }
}

/// I/O wrapper: does `CHANGELOG.md` on `develop` carry a top-level `##
/// <version>` heading? Reads GitHub's copy of the file via [`file_at_ref`]
/// then classifies it with [`classify_changelog_heading`].
pub fn changelog_written_on_develop(project_root: &Path, version: &str) -> Observation {
    match file_at_ref(project_root, "CHANGELOG.md", "develop") {
        Ok(contents) => classify_changelog_heading(&contents, version),
        Err(reason) => Observation::Unreachable { reason },
    }
}

/// I/O wrapper: has the release PR landed on `main` — i.e. does `main`'s own
/// `Cargo.toml` already carry `version`? The observable is the outcome
/// (`main` carrying version X), not the pull request object itself — a
/// merged PR is only the means by which it became true, and querying the PR
/// list instead would make the answer depend on a mutable GitHub search
/// index (RD-8).
pub fn release_pr_merged_to_main(project_root: &Path, version: &str) -> Observation {
    match file_at_ref(project_root, "Cargo.toml", "main") {
        Ok(contents) => classify_manifest_version(&contents, version),
        Err(reason) => Observation::Unreachable { reason },
    }
}

// -- 29-02 Task 2: sync-ancestry oracle (stub body, RED) ------------------
// TODO(29-02 GREEN): replace with real classification logic.

/// Classify GitHub's compare API `status` field (`compare/main...develop`),
/// answering whether `main` is an ancestor of `develop` — i.e. whether the
/// post-release sync has landed. Pure — no I/O. `ahead`/`identical` are
/// Present (base `main` is an ancestor of head `develop`); `behind`/
/// `diverged` are Absent; anything else, including empty, is Unreachable
/// naming the value seen.
pub fn classify_compare_status(status: &str) -> Observation {
    match status {
        "ahead" | "identical" => Observation::Present {
            detail: format!("compare status: {status}"),
        },
        "behind" | "diverged" => Observation::Absent {
            detail: format!("compare status: {status}"),
        },
        "" => Observation::Unreachable {
            reason: "compare status was empty".into(),
        },
        other => Observation::Unreachable {
            reason: format!("unexpected compare status `{other}`"),
        },
    }
}

/// I/O wrapper: has `main` been synced back into `develop`
/// (`scripts/sync-main-to-develop.sh`'s own question, asked remotely)? Runs
/// `gh api repos/{owner}/{repo}/compare/main...develop --jq .status`, pinned
/// to `project_root` (T-29-10), then [`classify_compare_status`].
///
/// This is deliberately a DIFFERENT function from
/// [`crate::git::origin_main_ancestor_status`], which reads already-fetched
/// local refs with no network access and must keep that no-fetch behavior
/// for `release --check` (20d) — collapsing the two would silently change
/// that command's documented no-network property. One function answers
/// "what is true on the remote right now"; the other answers "what does
/// this checkout already know."
pub fn sync_merged(project_root: &Path) -> Observation {
    let output = Command::new("gh")
        .current_dir(project_root)
        .args([
            "api",
            "repos/{owner}/{repo}/compare/main...develop",
            "--jq",
            ".status",
        ])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let status = String::from_utf8_lossy(&out.stdout).trim().to_string();
            classify_compare_status(&status)
        }
        Ok(out) => Observation::Unreachable {
            reason: format!(
                "gh api exited with status {} for compare/main...develop",
                out.status
            ),
        },
        Err(err) => Observation::Unreachable {
            reason: format!(
                "failed to spawn gh for compare/main...develop: {}",
                err.kind()
            ),
        },
    }
}

/// Dispatch one of the six release-cut questions to its own oracle.
/// Exhaustive — no wildcard arm — so a future seventh [`ReleaseStep`]
/// variant fails to compile rather than silently returning a default.
pub fn observe(project_root: &Path, step: ReleaseStep, version: &str) -> Observation {
    match step {
        ReleaseStep::VersionBumped => version_bumped_on_develop(project_root, version),
        ReleaseStep::ChangelogWritten => changelog_written_on_develop(project_root, version),
        ReleaseStep::ReleasePrMerged => release_pr_merged_to_main(project_root, version),
        ReleaseStep::SignedTagPresent => signed_tag_on_remote(project_root, version),
        ReleaseStep::SyncMerged => sync_merged(project_root),
        ReleaseStep::CratesPublished => crates_published(project_root, version),
    }
}

/// Answer all six release-cut questions, preserving `ReleaseStep::ALL`'s
/// order.
pub fn observe_all(project_root: &Path, version: &str) -> Vec<(ReleaseStep, Observation)> {
    ReleaseStep::ALL
        .iter()
        .map(|&step| (step, observe(project_root, step, version)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annotated_refs() -> TagRefs {
        TagRefs {
            object: Some("sha-object".into()),
            peeled: Some("sha-peeled".into()),
        }
    }

    fn lightweight_refs() -> TagRefs {
        TagRefs {
            object: Some("sha-object".into()),
            peeled: None,
        }
    }

    fn absent_refs() -> TagRefs {
        TagRefs::default()
    }

    #[test]
    fn classify_tag_refs_both_lines_yields_object_and_peeled() {
        let stdout = "abc123\trefs/tags/v2.1.0\ndef456\trefs/tags/v2.1.0^{}\n";
        let refs = classify_tag_refs(stdout, "v2.1.0");
        assert_eq!(refs.object.as_deref(), Some("abc123"));
        assert_eq!(refs.peeled.as_deref(), Some("def456"));
        assert!(refs.is_annotated());
        assert!(refs.exists());
    }

    #[test]
    fn classify_tag_refs_unpeeled_only_is_lightweight() {
        let stdout = "abc123\trefs/tags/v2.1.0\n";
        let refs = classify_tag_refs(stdout, "v2.1.0");
        assert_eq!(refs.object.as_deref(), Some("abc123"));
        assert_eq!(refs.peeled, None);
        assert!(!refs.is_annotated());
        assert!(refs.exists());
    }

    #[test]
    fn classify_tag_refs_empty_output_is_absent() {
        let refs = classify_tag_refs("", "v2.1.0");
        assert_eq!(refs.object, None);
        assert_eq!(refs.peeled, None);
        assert!(!refs.exists());
        assert!(!refs.is_annotated());
    }

    #[test]
    fn classify_tag_refs_ignores_unrelated_lines() {
        let stdout = "abc123\trefs/tags/v9.9.9\ndef456\trefs/heads/develop\n";
        let refs = classify_tag_refs(stdout, "v2.1.0");
        assert!(!refs.exists());
    }

    #[test]
    fn classify_signed_tag_absent_refs_is_absent() {
        let obs = classify_signed_tag(&absent_refs(), TagSignature::Missing);
        match obs {
            Observation::Absent { detail } => assert!(
                detail.to_lowercase().contains("tag"),
                "expected the detail to name the tag, got: {detail}"
            ),
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn classify_signed_tag_lightweight_is_absent_naming_lightweight() {
        let obs = classify_signed_tag(&lightweight_refs(), TagSignature::Missing);
        match obs {
            Observation::Absent { detail } => assert!(
                detail.to_lowercase().contains("lightweight"),
                "expected the detail to state the tag is lightweight, got: {detail}"
            ),
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn classify_signed_tag_annotated_present_is_present() {
        let obs = classify_signed_tag(&annotated_refs(), TagSignature::Present);
        assert!(matches!(obs, Observation::Present { .. }));
    }

    #[test]
    fn classify_signed_tag_annotated_missing_is_absent_unsigned() {
        let obs = classify_signed_tag(&annotated_refs(), TagSignature::Missing);
        match obs {
            Observation::Absent { detail } => assert!(
                detail.to_lowercase().contains("signature")
                    || detail.to_lowercase().contains("unsigned"),
                "expected the detail to state the tag is annotated but unsigned, got: {detail}"
            ),
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn classify_signed_tag_annotated_undetermined_is_unreachable_with_reason_carried() {
        let obs = classify_signed_tag(
            &annotated_refs(),
            TagSignature::Undetermined {
                reason: "gh api exited with status 1".into(),
            },
        );
        match obs {
            Observation::Unreachable { reason } => {
                assert_eq!(reason, "gh api exited with status 1");
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    #[test]
    fn unreachable_is_never_absent() {
        // Constructing an Unreachable never produces something PartialEq to
        // an Absent with the same text — the two variants must never
        // compare equal even with identical strings, proving the type
        // system (not convention) keeps them apart.
        let unreachable = Observation::Unreachable {
            reason: "network unreachable".into(),
        };
        let absent = Observation::Absent {
            detail: "network unreachable".into(),
        };
        assert_ne!(unreachable, absent);
    }

    // -- Task 2: crates.io publish oracle -----------------------------

    #[test]
    fn classify_http_status_200_is_present() {
        assert!(matches!(
            classify_http_status("200"),
            Observation::Present { .. }
        ));
    }

    #[test]
    fn classify_http_status_404_is_absent() {
        assert!(matches!(
            classify_http_status("404"),
            Observation::Absent { .. }
        ));
    }

    #[test]
    fn classify_http_status_000_is_unreachable() {
        // curl's own code for "could not connect".
        assert!(matches!(
            classify_http_status("000"),
            Observation::Unreachable { .. }
        ));
    }

    #[test]
    fn classify_http_status_other_values_are_unreachable_naming_the_code() {
        for code in ["500", "403", "", "not-a-number"] {
            match classify_http_status(code) {
                Observation::Unreachable { reason } => {
                    assert!(
                        reason.contains(code) || (code.is_empty() && reason.contains("empty")),
                        "expected the reason to name the status code {code:?}, got: {reason}"
                    );
                }
                other => panic!("expected Unreachable for {code:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn combine_crate_observations_two_present_is_present() {
        let per_crate = vec![
            (
                "a".to_string(),
                Observation::Present {
                    detail: "live".into(),
                },
            ),
            (
                "b".to_string(),
                Observation::Present {
                    detail: "live".into(),
                },
            ),
        ];
        assert!(matches!(
            combine_crate_observations(&per_crate),
            Observation::Present { .. }
        ));
    }

    #[test]
    fn combine_crate_observations_present_and_absent_is_absent() {
        let per_crate = vec![
            (
                "a".to_string(),
                Observation::Present {
                    detail: "live".into(),
                },
            ),
            (
                "b".to_string(),
                Observation::Absent {
                    detail: "not yet".into(),
                },
            ),
        ];
        assert!(matches!(
            combine_crate_observations(&per_crate),
            Observation::Absent { .. }
        ));
    }

    #[test]
    fn combine_crate_observations_any_unreachable_dominates() {
        let per_crate = vec![
            (
                "a".to_string(),
                Observation::Present {
                    detail: "live".into(),
                },
            ),
            (
                "b".to_string(),
                Observation::Absent {
                    detail: "not yet".into(),
                },
            ),
            (
                "c".to_string(),
                Observation::Unreachable {
                    reason: "timed out".into(),
                },
            ),
        ];
        assert!(matches!(
            combine_crate_observations(&per_crate),
            Observation::Unreachable { .. }
        ));
    }

    #[test]
    fn combine_crate_observations_empty_slice_is_unreachable() {
        assert!(matches!(
            combine_crate_observations(&[]),
            Observation::Unreachable { .. }
        ));
    }

    // -- 29-02 Task 1: content-at-ref oracles ---------------------------

    #[test]
    fn classify_manifest_version_present_when_workspace_and_pins_match() {
        assert!(matches!(
            classify_manifest_version("[workspace.package]\nversion = \"2.3.0\"\n", "2.3.0"),
            Observation::Present { .. }
        ));
    }

    #[test]
    fn classify_manifest_version_absent_when_workspace_version_differs() {
        let manifest = "[workspace.package]\nversion = \"2.2.0\"\n";
        match classify_manifest_version(manifest, "2.3.0") {
            Observation::Absent { detail } => {
                assert!(
                    detail.contains("2.2.0") && detail.contains("2.3.0"),
                    "expected the detail to name both versions, got: {detail}"
                );
            }
            other => panic!("expected Absent naming both versions, got {other:?}"),
        }
    }

    #[test]
    fn classify_manifest_version_empty_is_unreachable() {
        assert!(matches!(
            classify_manifest_version("", "2.3.0"),
            Observation::Unreachable { .. }
        ));
    }

    #[test]
    fn classify_manifest_version_absent_when_self_pin_drifted() {
        let manifest = "[workspace]\nmembers = [\"crates/devflow-core\"]\n\n\
             [workspace.package]\nversion = \"2.3.0\"\nedition = \"2024\"\n\n\
             [workspace.dependencies]\n\
             devflow-core = { path = \"crates/devflow-core\", version = \"2.2.0\" }\n";
        match classify_manifest_version(manifest, "2.3.0") {
            Observation::Absent { detail } => {
                assert!(
                    detail.contains("devflow-core"),
                    "expected the detail to name the drifted pin, got: {detail}"
                );
            }
            other => panic!("expected Absent naming the drifted pin, got {other:?}"),
        }
    }

    #[test]
    fn classify_changelog_heading_present_with_body_after() {
        assert!(matches!(
            classify_changelog_heading("## 2.3.0\n\n### Added\n", "2.3.0"),
            Observation::Present { .. }
        ));
    }

    #[test]
    fn classify_changelog_heading_absent_when_version_mismatches() {
        assert!(matches!(
            classify_changelog_heading("## 2.2.0\n", "2.3.0"),
            Observation::Absent { .. }
        ));
    }

    #[test]
    fn classify_changelog_heading_absent_for_bare_inline_mention() {
        assert!(matches!(
            classify_changelog_heading("Some prose mentioning 2.3.0 inline\n", "2.3.0"),
            Observation::Absent { .. }
        ));
    }

    #[test]
    fn classify_changelog_heading_empty_is_unreachable() {
        assert!(matches!(
            classify_changelog_heading("", "2.3.0"),
            Observation::Unreachable { .. }
        ));
    }

    #[test]
    fn classify_changelog_heading_no_space_after_hashes_is_absent() {
        assert!(matches!(
            classify_changelog_heading("##2.3.0\n", "2.3.0"),
            Observation::Absent { .. }
        ));
    }

    #[test]
    fn classify_changelog_heading_leading_whitespace_still_matches() {
        assert!(matches!(
            classify_changelog_heading("  ## 2.3.0\n", "2.3.0"),
            Observation::Present { .. }
        ));
    }

    // -- 29-02 Task 2: sync-ancestry oracle and dispatcher --------------

    #[test]
    fn classify_compare_status_ahead_is_present() {
        assert!(matches!(
            classify_compare_status("ahead"),
            Observation::Present { .. }
        ));
    }

    #[test]
    fn classify_compare_status_identical_is_present() {
        assert!(matches!(
            classify_compare_status("identical"),
            Observation::Present { .. }
        ));
    }

    #[test]
    fn classify_compare_status_behind_is_absent() {
        assert!(matches!(
            classify_compare_status("behind"),
            Observation::Absent { .. }
        ));
    }

    #[test]
    fn classify_compare_status_diverged_is_absent() {
        assert!(matches!(
            classify_compare_status("diverged"),
            Observation::Absent { .. }
        ));
    }

    #[test]
    fn classify_compare_status_empty_is_unreachable_naming_the_value() {
        match classify_compare_status("") {
            Observation::Unreachable { reason } => assert!(!reason.is_empty()),
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    #[test]
    fn classify_compare_status_unexpected_value_is_unreachable_naming_it() {
        match classify_compare_status("unexpected") {
            Observation::Unreachable { reason } => assert!(
                reason.contains("unexpected"),
                "expected the reason to name the value seen, got: {reason}"
            ),
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    /// Initialize a bare git repo with no remote — every oracle in
    /// `observe` must independently fail to observe.
    fn init_repo_without_remote() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            let output = crate::test_support::git_command(root)
                .args(args)
                .output()
                .expect("spawn git");
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);
        dir
    }

    #[test]
    fn observe_dispatches_all_six_variants_to_distinct_unreachable_reasons_with_no_remote() {
        let dir = init_repo_without_remote();
        let root = dir.path();
        let mut reasons = Vec::new();
        for step in ReleaseStep::ALL {
            match observe(root, step, "1.2.3") {
                Observation::Unreachable { reason } => {
                    assert!(!reason.is_empty(), "{step:?} produced an empty reason");
                    reasons.push(reason);
                }
                other => {
                    panic!("expected {step:?} to be Unreachable with no remote, got {other:?}")
                }
            }
        }
        let mut deduped = reasons.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(
            deduped.len(),
            reasons.len(),
            "expected six distinct reasons (no arm falling through to a shared \
             default), got: {reasons:?}"
        );
    }

    #[test]
    fn observe_all_preserves_release_step_all_order() {
        let dir = init_repo_without_remote();
        let root = dir.path();
        let results = observe_all(root, "1.2.3");
        let steps: Vec<ReleaseStep> = results.iter().map(|(step, _)| *step).collect();
        assert_eq!(steps, ReleaseStep::ALL.to_vec());
    }
}
