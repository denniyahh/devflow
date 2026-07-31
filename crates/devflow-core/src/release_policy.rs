//! Merge-intent resolution against a discovered allowed-method set (29b).
//!
//! Pull requests are classified by what they are *for* — [`MergeIntent`] —
//! never by which branch they target. Two different PRs into `develop` (a
//! version bump and a sync-back) require different merge methods, which is
//! exactly why a branch-name lookup is insufficient (RD-9).
//!
//! [`resolve_merge_method`] refuses loudly, rather than silently
//! substituting the other method, when the required method for an intent is
//! absent from the discovered allowed set. This reproduces the correct
//! outcome of this repository's own documented incident (CONTRIBUTING.md §
//! "Cutting a Release" step 6): on 2026-07-27, an unspecified-method
//! auto-merge on the sync-back PR defaulted to squash, destroying the
//! ancestry link the step exists to create and requiring a repair PR.
//!
//! This module performs **no writes** and issues no `gh pr merge` calls
//! itself — it only resolves which method a caller (`29-05`/`29-06`) must
//! pass explicitly.

use std::path::Path;
use std::process::Command;

/// The three pull-request intents this executor creates. The durable
/// concept: the concrete merge method is resolved from this intent against
/// facts discovered at runtime, never hardcoded and never keyed on branch
/// name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeIntent {
    VersionBump,
    ReleaseCut,
    SyncBack,
}

/// The two merge methods this executor will ever request. No third,
/// "unspecified" variant exists — a method-less merge is unrepresentable in
/// this type system, which is what makes Pitfall 3 (a bare `gh pr merge
/// --auto` silently picking squash) impossible to reintroduce by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMethod {
    Merge,
    Squash,
}

impl MergeMethod {
    /// The literal `gh pr merge` flag for this method.
    pub fn flag(self) -> &'static str {
        match self {
            MergeMethod::Merge => "--merge",
            MergeMethod::Squash => "--squash",
        }
    }

    /// The lowercase name as it appears in GitHub's allowed-merge-method
    /// lists (`allow_merge_commit`/`allow_squash_merge`, and the
    /// `allowed_merge_methods` array on a branch ruleset's `pull_request`
    /// rule).
    pub fn api_name(self) -> &'static str {
        match self {
            MergeMethod::Merge => "merge",
            MergeMethod::Squash => "squash",
        }
    }
}

impl std::fmt::Display for MergeMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.api_name())
    }
}

/// Errors produced by merge-method resolution.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MergePolicyError {
    /// The method this intent requires is not present in the discovered
    /// allowed set. Never substituted for the other method — refusing
    /// loudly is the required behavior (RD-9).
    #[error(
        "required merge method '{required}' for {intent:?} is not in the discovered allowed set {allowed:?} — refusing rather than silently substituting"
    )]
    NotAllowed {
        intent: MergeIntent,
        required: MergeMethod,
        allowed: Vec<String>,
    },
    /// The discovered allowed set is empty — a failure to discover, never a
    /// license to pick a default.
    #[error("cannot resolve a merge method: allowed-method set is unknown ({reason})")]
    AllowedSetUnknown { reason: String },
}

/// The fixed internal policy: the one place a per-intent merge-method
/// preference is written down. `VersionBump` and `ReleaseCut` both require
/// `Squash`; `SyncBack` requires `Merge` because a squashed sync collapses
/// the two parents and discards the ancestry link the step exists to
/// create — a hard constraint of the operation, not a repository setting,
/// so it is expressed here rather than discovered.
pub fn required_method(intent: MergeIntent) -> MergeMethod {
    match intent {
        MergeIntent::VersionBump => MergeMethod::Squash,
        MergeIntent::ReleaseCut => MergeMethod::Squash,
        MergeIntent::SyncBack => MergeMethod::Merge,
    }
}

/// Resolve the merge method for `intent` against a discovered `allowed` set.
/// Pure — no I/O. `allowed` entries are compared case-insensitively and
/// after trimming surrounding whitespace. An empty `allowed` set is a
/// failure to discover ([`MergePolicyError::AllowedSetUnknown`]), never a
/// default. A miss returns [`MergePolicyError::NotAllowed`] — this function
/// must never return the other method on a miss.
pub fn resolve_merge_method(
    intent: MergeIntent,
    allowed: &[String],
) -> Result<MergeMethod, MergePolicyError> {
    if allowed.is_empty() {
        return Err(MergePolicyError::AllowedSetUnknown {
            reason: "discovered allowed-method set is empty".into(),
        });
    }

    let required = required_method(intent);
    let required_name = required.api_name();
    let is_allowed = allowed
        .iter()
        .any(|entry| entry.trim().eq_ignore_ascii_case(required_name));

    if is_allowed {
        Ok(required)
    } else {
        Err(MergePolicyError::NotAllowed {
            intent,
            required,
            allowed: allowed.to_vec(),
        })
    }
}

/// The effective allowed set is the intersection of what the repository's
/// merge settings permit and what the branch's ruleset permits — either one
/// can forbid a method independently. Pure — no I/O. If either input is
/// empty, the result is empty.
pub fn intersect_allowed(repo_level: &[String], branch_level: &[String]) -> Vec<String> {
    repo_level
        .iter()
        .filter(|entry| {
            branch_level
                .iter()
                .any(|other| other.trim().eq_ignore_ascii_case(entry.trim()))
        })
        .cloned()
        .collect()
}

/// Discover the allowed merge methods for `branch` by querying the live
/// GitHub API — never a cached copy, never a compiled-in constant. Combines
/// the repository's own merge settings (`allow_merge_commit`,
/// `allow_squash_merge`) with the branch's effective ruleset
/// (`allowed_merge_methods` on its `pull_request` rule, if any) via
/// [`intersect_allowed`]. If the branch-rules endpoint reports no
/// `pull_request` rule, the branch imposes no additional restriction and
/// the repository-level set stands alone.
///
/// Every `gh` invocation is pinned to `project_root` via `.current_dir` so
/// the `{owner}`/`{repo}` placeholders resolve to the project under
/// operation, never to whatever repository the ambient shell sits in
/// (T-29-10). Every failure returns a failure-class description; `gh`'s raw
/// stdout/stderr is never embedded (T-29-03).
pub fn discover_allowed_merge_methods(
    project_root: &Path,
    branch: &str,
) -> Result<Vec<String>, String> {
    let repo_level = repo_level_allowed_methods(project_root)?;
    let branch_level = branch_level_allowed_methods(project_root, branch)?;
    if branch_level.is_empty() {
        Ok(repo_level)
    } else {
        Ok(intersect_allowed(&repo_level, &branch_level))
    }
}

/// `gh api repos/{owner}/{repo} --jq '.<field>'`, reading a single boolean
/// merge-setting field. Pinned to `project_root` (T-29-10). Never embeds
/// `gh`'s raw output in the returned error (T-29-03).
fn gh_bool_field(project_root: &Path, path: &str, jq: &str) -> Result<bool, String> {
    let output = Command::new("gh")
        .current_dir(project_root)
        .args(["api", path, "--jq", jq])
        .output();
    match output {
        Ok(out) if out.status.success() => match String::from_utf8_lossy(&out.stdout).trim() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(format!("gh api returned an unexpected value for {jq}")),
        },
        Ok(out) => Err(format!("gh api exited with status {}", out.status)),
        Err(err) => Err(format!("failed to spawn gh: {}", err.kind())),
    }
}

/// The repository's own merge settings (`allow_merge_commit`,
/// `allow_squash_merge`), read live via `gh api repos/{owner}/{repo}` —
/// never cached, never a compiled-in copy.
fn repo_level_allowed_methods(project_root: &Path) -> Result<Vec<String>, String> {
    let allow_merge = gh_bool_field(project_root, "repos/{owner}/{repo}", ".allow_merge_commit")?;
    let allow_squash = gh_bool_field(project_root, "repos/{owner}/{repo}", ".allow_squash_merge")?;
    let mut methods = Vec::new();
    if allow_merge {
        methods.push(MergeMethod::Merge.api_name().to_string());
    }
    if allow_squash {
        methods.push(MergeMethod::Squash.api_name().to_string());
    }
    Ok(methods)
}

/// The branch's effective ruleset, read live via
/// `gh api repos/{owner}/{repo}/rules/branches/<branch>` — the
/// `allowed_merge_methods` array on its `pull_request` rule, if one exists.
/// An empty result means no `pull_request` rule was found, which the caller
/// interprets as "no additional restriction," never as "no methods
/// allowed."
fn branch_level_allowed_methods(project_root: &Path, branch: &str) -> Result<Vec<String>, String> {
    let path = format!("repos/{{owner}}/{{repo}}/rules/branches/{branch}");
    let jq = r#"[.[] | select(.type=="pull_request") | .parameters.allowed_merge_methods[]?] | join(",")"#;
    let output = Command::new("gh")
        .current_dir(project_root)
        .args(["api", &path, "--jq", jq])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if text.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(text.split(',').map(str::to_string).collect())
            }
        }
        Ok(out) => Err(format!("gh api exited with status {}", out.status)),
        Err(err) => Err(format!("failed to spawn gh: {}", err.kind())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|s| s.to_string()).collect()
    }

    /// Live-measured fixture (`.planning/ROADMAP.md`'s ruleset finding,
    /// 2026-07-31): `develop` allows both merge and squash.
    fn develop_allowed() -> Vec<String> {
        strings(&["merge", "squash"])
    }

    /// Live-measured fixture: `main` allows squash only.
    fn main_allowed() -> Vec<String> {
        strings(&["squash"])
    }

    /// A hypothetical configuration that is not this repository's, proving
    /// the policy is general rather than tuned to today's settings.
    fn hypothetical_merge_only() -> Vec<String> {
        strings(&["merge"])
    }

    // -- MergeMethod::flag / api_name --------------------------------

    #[test]
    fn merge_method_flags_are_correct_and_never_empty() {
        assert_eq!(MergeMethod::Merge.flag(), "--merge");
        assert_eq!(MergeMethod::Squash.flag(), "--squash");
        assert!(!MergeMethod::Merge.flag().is_empty());
        assert!(!MergeMethod::Squash.flag().is_empty());
    }

    #[test]
    fn merge_method_api_names_are_lowercase() {
        assert_eq!(MergeMethod::Merge.api_name(), "merge");
        assert_eq!(MergeMethod::Squash.api_name(), "squash");
    }

    // -- required_method ----------------------------------------------

    #[test]
    fn required_method_sync_back_is_merge() {
        assert_eq!(required_method(MergeIntent::SyncBack), MergeMethod::Merge);
    }

    #[test]
    fn required_method_version_bump_and_release_cut_are_squash() {
        assert_eq!(
            required_method(MergeIntent::VersionBump),
            MergeMethod::Squash
        );
        assert_eq!(
            required_method(MergeIntent::ReleaseCut),
            MergeMethod::Squash
        );
    }

    // -- resolve_merge_method -------------------------------------------

    /// The single most important case in this module: the sync PR takes a
    /// real merge commit even though squash is also allowed. Reproduces
    /// the 2026-07-27 incident's correct outcome.
    #[test]
    fn resolve_merge_method_sync_back_on_develop_yields_merge() {
        let resolved = resolve_merge_method(MergeIntent::SyncBack, &develop_allowed());
        assert_eq!(resolved, Ok(MergeMethod::Merge));
    }

    /// A squash-only allowed set refuses the sync-back intent rather than
    /// falling back to squash. The error text names the intent, the
    /// required method, and the discovered set.
    #[test]
    fn resolve_merge_method_sync_back_on_squash_only_refuses_naming_all_three_facts() {
        let result = resolve_merge_method(MergeIntent::SyncBack, &main_allowed());
        let err = result.expect_err("sync-back must refuse on a squash-only allowed set");
        let message = err.to_string();
        assert!(message.contains("SyncBack"), "message: {message}");
        assert!(message.contains("merge"), "message: {message}");
        assert!(message.contains("squash"), "message: {message}");
    }

    #[test]
    fn resolve_merge_method_release_cut_on_main_yields_squash() {
        let resolved = resolve_merge_method(MergeIntent::ReleaseCut, &main_allowed());
        assert_eq!(resolved, Ok(MergeMethod::Squash));
    }

    #[test]
    fn resolve_merge_method_release_cut_on_merge_only_refuses_naming_squash() {
        let result = resolve_merge_method(MergeIntent::ReleaseCut, &hypothetical_merge_only());
        let err = result.expect_err("release-cut must refuse when squash is unavailable");
        assert!(err.to_string().contains("squash"));
    }

    #[test]
    fn resolve_merge_method_version_bump_on_develop_yields_squash() {
        let resolved = resolve_merge_method(MergeIntent::VersionBump, &develop_allowed());
        assert_eq!(resolved, Ok(MergeMethod::Squash));
    }

    #[test]
    fn resolve_merge_method_empty_allowed_set_is_allowed_set_unknown() {
        for intent in [
            MergeIntent::VersionBump,
            MergeIntent::ReleaseCut,
            MergeIntent::SyncBack,
        ] {
            let result = resolve_merge_method(intent, &[]);
            assert!(
                matches!(result, Err(MergePolicyError::AllowedSetUnknown { .. })),
                "expected AllowedSetUnknown for {intent:?}, got {result:?}"
            );
        }
    }

    #[test]
    fn resolve_merge_method_is_case_insensitive_and_whitespace_tolerant() {
        let upper = strings(&["SQUASH"]);
        let padded = strings(&[" squash "]);
        assert_eq!(
            resolve_merge_method(MergeIntent::ReleaseCut, &upper),
            Ok(MergeMethod::Squash)
        );
        assert_eq!(
            resolve_merge_method(MergeIntent::ReleaseCut, &padded),
            Ok(MergeMethod::Squash)
        );
    }

    // -- intersect_allowed ------------------------------------------------

    #[test]
    fn intersect_allowed_returns_only_common_methods() {
        let repo_level = strings(&["merge", "squash"]);
        let branch_level = strings(&["squash"]);
        assert_eq!(
            intersect_allowed(&repo_level, &branch_level),
            strings(&["squash"])
        );
    }

    #[test]
    fn intersect_allowed_empty_repo_level_is_empty() {
        assert_eq!(
            intersect_allowed(&[], &strings(&["squash"])),
            Vec::<String>::new()
        );
    }

    #[test]
    fn intersect_allowed_empty_branch_level_is_empty() {
        assert_eq!(
            intersect_allowed(&strings(&["merge", "squash"]), &[]),
            Vec::<String>::new()
        );
    }
}
