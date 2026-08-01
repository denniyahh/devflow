//! The release-cut walker (29b/29c scaffolding): observe, act, re-observe,
//! stop — the design rule (RD-2) as executable control flow.
//!
//! [`cut`] walks [`ReleaseStep::ALL`] in order, observing each step via
//! [`crate::release_observe::observe`]. `Present` records the step done and
//! continues; `Unreachable` stops the walk and refuses immediately — an
//! oracle that could not be reached must never lead to an action, because
//! acting on it risks redoing an already-completed irreversible step;
//! `Absent` checks for an in-flight pull request before running the step's
//! own action (if this build carries one), then stops either way.
//! [`ReleaseStep::VersionBumped`] and [`ReleaseStep::ChangelogWritten`] (29-05)
//! resolve to [`bump_and_changelog_pr`] — prepare the two-place version bump
//! and the changelog entry on a release branch in a scratch worktree, then
//! open and arm a pull request into `develop`. [`ReleaseStep::ReleasePrMerged`]
//! (29-06) resolves to [`release_pr_to_main`] — the release pull request from
//! `develop` into `main`. [`ReleaseStep::SyncMerged`] (29-06) resolves to
//! [`sync_back_pr`] — a port of `scripts/sync-main-to-develop.sh` that keeps
//! `main` a real ancestor of `develop`, landed by a real merge commit rather
//! than a direct push. [`ReleaseStep::SignedTagPresent`] (29-07) resolves to
//! [`sign_release_tag`] — the target commit is resolved as the tip of
//! `origin/main`, then [`crate::release_publish::create_and_push_tag`] runs
//! the exact command CONTRIBUTING.md documents and reports git's own result.
//! [`ReleaseStep::CratesPublished`] (29-07) resolves to [`publish_crates`] —
//! [`crate::release_publish::publish_all`], consuming
//! [`crate::git::publish_order`]'s computed sequence exactly. With this
//! plan, all six variants carry a real action: [`action_for`]'s match is
//! exhaustive over all six with no wildcard arm, so a new step cannot be
//! silently skipped, and `unit_for`'s naming (`29b`/`29c`) is now purely
//! historical — every step in this build has an action.
//!
//! This module performs **no writes** to `.devflow/`, to `devflow.toml`, or
//! to any other DevFlow-owned file, and holds no state across invocations.
//! Re-running is re-observing.

use crate::git::{self, GitFlow};
use crate::release_observe::{Observation, ReleaseStep, classify_changelog_heading, observe};
use crate::release_policy::{self, MergeIntent, MergeMethod};
use crate::version;
use crate::worktree::{self, WorktreeError};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The outcome of walking one [`ReleaseStep`].
///
/// Every arm carries a `String` (or, for [`NoActionInThisBuild`](Self::NoActionInThisBuild),
/// the unit identifier) — never a bare unit variant — so the CLI layer
/// always has something to print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    /// The step was already observed `Present` before this run touched it.
    AlreadyDone { detail: String },
    /// The step is `Absent` but a pull request for it is already open —
    /// under way, not a candidate for a duplicate.
    InFlight { detail: String },
    /// This run's own action attempt succeeded. Terminal on purpose: the
    /// action's real effect (e.g. GitHub's auto-merge) is asynchronous, and
    /// continuing past it would mean predicting that outcome, which RD-2's
    /// third layer forbids.
    Performed { detail: String },
    /// The walk stopped here — an unreachable oracle, an unanswerable
    /// in-flight check, or a failed action.
    Stopped { reason: String },
    /// The step is `Absent`, no pull request is in flight, and this build
    /// carries no action for it yet. `unit` names the unit that supplies it
    /// (`29b` or `29c`).
    NoActionInThisBuild { unit: &'static str },
}

impl StepOutcome {
    /// `true` for every variant except [`AlreadyDone`](Self::AlreadyDone) —
    /// the walk stops on all four terminal outcomes, and only `AlreadyDone`
    /// continues it.
    pub fn is_terminal(&self) -> bool {
        !matches!(self, StepOutcome::AlreadyDone { .. })
    }
}

/// The full record of one `cut` walk: one outcome per step actually
/// reached, in [`ReleaseStep::ALL`] order, and whether the run was
/// authorized at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutReport {
    pub steps: Vec<(ReleaseStep, StepOutcome)>,
    pub authorized: bool,
}

impl CutReport {
    /// The first step whose outcome is not [`StepOutcome::AlreadyDone`] —
    /// where the walk actually stopped, if it stopped at all.
    pub fn stopped_at(&self) -> Option<(ReleaseStep, &StepOutcome)> {
        self.steps.iter().find_map(|(step, outcome)| match outcome {
            StepOutcome::AlreadyDone { .. } => None,
            other => Some((*step, other)),
        })
    }

    /// `true` only when every one of the six steps observed `AlreadyDone` —
    /// the only case in which the release-cut walk has nothing left to do.
    pub fn all_done(&self) -> bool {
        self.stopped_at().is_none()
    }
}

/// A step's action: given the project root and the version, attempt the
/// step's real-world effect. `Ok` carries a human detail; `Err` carries the
/// real failure text.
type StepAction = fn(&Path, &str) -> Result<String, String>;

/// The action this build carries for `step`. `VersionBumped` and
/// `ChangelogWritten` (29-05) both resolve to [`bump_and_changelog_pr`];
/// `ReleasePrMerged` and `SyncMerged` (29-06) resolve to
/// [`release_pr_to_main`]/[`sync_back_pr`]; `SignedTagPresent` and
/// `CratesPublished` (29-07) resolve to
/// [`sign_release_tag`]/[`publish_crates`] — the two irreversible
/// commit-point operations. Every arm now returns `Some`: exhaustive with no
/// wildcard arm, a seventh [`ReleaseStep`] variant fails to compile here
/// rather than silently falling through to `None`.
fn action_for(step: ReleaseStep) -> Option<StepAction> {
    match step {
        ReleaseStep::VersionBumped => Some(bump_and_changelog_pr),
        ReleaseStep::ChangelogWritten => Some(bump_and_changelog_pr),
        ReleaseStep::ReleasePrMerged => Some(release_pr_to_main),
        ReleaseStep::SignedTagPresent => Some(sign_release_tag),
        ReleaseStep::SyncMerged => Some(sync_back_pr),
        ReleaseStep::CratesPublished => Some(publish_crates),
    }
}

/// The unit identifier that supplies `step`'s action — used only to make a
/// [`StepOutcome::NoActionInThisBuild`] report name which future unit closes
/// the gap. Steps 1, 2, 3, and 5 (the recoverable PR-backed actions) belong
/// to `29b`; steps 4 and 6 (the two irreversible commit-point operations)
/// belong to `29c`.
fn unit_for(step: ReleaseStep) -> &'static str {
    match step {
        ReleaseStep::VersionBumped => "29b",
        ReleaseStep::ChangelogWritten => "29b",
        ReleaseStep::ReleasePrMerged => "29b",
        ReleaseStep::SignedTagPresent => "29c",
        ReleaseStep::SyncMerged => "29b",
        ReleaseStep::CratesPublished => "29c",
    }
}

/// Whether an open pull request already exists for a pull-request-backed
/// step — three-valued like every other observation in this phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrPresence {
    Open { number: u64 },
    None,
    Unreachable { reason: String },
}

/// The head and base branch names for a pull-request-backed [`ReleaseStep`],
/// or `None` for [`ReleaseStep::SignedTagPresent`] and
/// [`ReleaseStep::CratesPublished`], which are not pull-request-backed. The
/// head branch names are deterministic functions of `version` so a re-run
/// finds the same branch.
pub fn pr_refs(step: ReleaseStep, version: &str) -> Option<(String, String)> {
    match step {
        ReleaseStep::VersionBumped | ReleaseStep::ChangelogWritten => {
            Some((release_branch_name(version), "develop".to_string()))
        }
        ReleaseStep::ReleasePrMerged => Some(("develop".to_string(), "main".to_string())),
        ReleaseStep::SignedTagPresent => None,
        ReleaseStep::SyncMerged => Some((sync_branch_name(version), "develop".to_string())),
        ReleaseStep::CratesPublished => None,
    }
}

/// The deterministic sync-back branch name for `version`, matching what
/// [`pr_refs`] already returns for [`ReleaseStep::SyncMerged`] — a test
/// asserts the two agree so they cannot drift, mirroring
/// [`release_branch_name`]'s own agreement guarantee for the bump branch.
pub fn sync_branch_name(version: &str) -> String {
    format!("sync/main-to-develop-v{version}")
}

/// The deterministic release-bump branch name for `version` — a single
/// function of the version, so [`pr_refs`]'s head for
/// [`ReleaseStep::VersionBumped`]/[`ReleaseStep::ChangelogWritten`] and
/// [`prepare_bump_branch`]'s own branch always agree, and a re-run targets
/// the same branch.
pub fn release_branch_name(version: &str) -> String {
    format!("release/bump-v{version}")
}

/// The outcome of preparing the release bump branch: the branch name, how
/// many commits this call actually created, and whether the branch already
/// carried both changes before this call ran (a re-run after a prior success
/// makes no new commit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedBranch {
    pub branch: String,
    pub commits_created: u32,
    pub already_prepared: bool,
}

/// `gh pr list --state open --head <head> --base <base> --json number --jq
/// '.[0].number'`, pinned to `project_root` (T-29-10). Empty stdout is
/// [`PrPresence::None`]; a parseable number is [`PrPresence::Open`]; a spawn
/// failure, non-zero exit, or unparseable stdout is
/// [`PrPresence::Unreachable`] with a failure-class reason that never embeds
/// `gh`'s raw output (T-29-03).
pub fn open_pr(project_root: &Path, head: &str, base: &str) -> PrPresence {
    let output = Command::new("gh")
        .current_dir(project_root)
        .args([
            "pr",
            "list",
            "--state",
            "open",
            "--head",
            head,
            "--base",
            base,
            "--json",
            "number",
            "--jq",
            ".[0].number",
        ])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if text.is_empty() {
                PrPresence::None
            } else {
                match text.parse::<u64>() {
                    Ok(number) => PrPresence::Open { number },
                    Err(_) => PrPresence::Unreachable {
                        reason: "gh pr list returned an unparseable pull request number".into(),
                    },
                }
            }
        }
        Ok(out) => PrPresence::Unreachable {
            reason: format!("gh pr list exited with status {}", out.status),
        },
        Err(err) => PrPresence::Unreachable {
            reason: format!("failed to spawn gh pr list: {}", err.kind()),
        },
    }
}

/// A scratch worktree that is removed on `Drop`, regardless of whether the
/// work it hosted succeeded or failed. This is what makes
/// [`prepare_bump_branch`]'s cleanup unconditional (T-29-19): the compiler
/// runs `drop` on every exit path out of the function — an early `?` return,
/// a panic during unwind, or a normal return — with no way for a future edit
/// to add a return that skips it.
struct ScratchWorktreeGuard<'a> {
    project_root: &'a Path,
    path: PathBuf,
}

impl Drop for ScratchWorktreeGuard<'_> {
    fn drop(&mut self) {
        if let Err(err) = worktree::remove(self.project_root, &self.path, true) {
            tracing::warn!("failed to remove scratch worktree {:?}: {err}", self.path);
        }
    }
}

/// Whether `branch` already exists on `origin` — checked before deciding
/// whether the scratch worktree should create a fresh branch off
/// `origin/develop` or check out the existing one, so a re-run after a prior
/// push reuses the same branch (and its commits) instead of recreating it
/// from `origin/develop` and discarding them.
fn branch_exists_on_origin(project_root: &Path, branch: &str) -> Result<bool, String> {
    let output = git::git_command(project_root)
        .args(["ls-remote", "--exit-code", "--heads", "origin", branch])
        .output()
        .map_err(|err| format!("failed to spawn git ls-remote: {}", err.kind()))?;
    match output.status.code() {
        Some(0) => Ok(true),
        // `ls-remote --exit-code` documents exit code 2 for "no matching
        // refs" — a real, reachable answer, not a failure.
        Some(2) => Ok(false),
        _ => Err(format!(
            "git ls-remote --heads origin {branch} exited with status {}",
            output.status
        )),
    }
}

/// `git fetch origin <ref_name>`, pinned to `project_root` (T-29-10).
fn fetch_ref(project_root: &Path, ref_name: &str) -> Result<(), String> {
    let output = git::git_command(project_root)
        .args(["fetch", "origin", ref_name])
        .output()
        .map_err(|err| format!("failed to spawn git fetch: {}", err.kind()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git fetch origin {ref_name} exited with status {}",
            output.status
        ))
    }
}

/// Create the scratch worktree at `scratch_path` on `branch`. When `branch`
/// already exists on `origin` (`create_branch` is `false`), checks it out
/// as-is — preserving whatever commits it already carries — instead of
/// recreating it from `origin/develop`. Retries once, after removing the
/// stale path, if `worktree::add` reports the path already exists
/// (`WorktreeError::Exists`) — a second failure is a real `Err`.
fn add_scratch_worktree(
    project_root: &Path,
    scratch_path: &Path,
    branch: &str,
    create_branch: bool,
) -> Result<(), String> {
    let start_point = if create_branch { "origin/develop" } else { "" };
    match worktree::add(
        project_root,
        scratch_path,
        branch,
        start_point,
        create_branch,
    ) {
        Ok(()) => Ok(()),
        Err(WorktreeError::Exists(_)) => {
            worktree::remove(project_root, scratch_path, true)
                .map_err(|err| format!("failed to remove stale scratch worktree: {err}"))?;
            worktree::add(
                project_root,
                scratch_path,
                branch,
                start_point,
                create_branch,
            )
            .map_err(|err| err.to_string())
        }
        Err(err) => Err(err.to_string()),
    }
}

/// Prepare the release branch in a scratch worktree: the two-place version
/// bump (via [`version::write_version`], never re-derived here) and the
/// changelog entry (via the existing `changelog_sections` /
/// `render_changelog_body` / `ship::prepend_changelog` chain), each in its
/// own commit, pushed to `origin`. The operator's own checkout — whatever it
/// currently is — is never touched: every write and commit happens inside
/// the scratch worktree `worktree::add` creates, and that worktree is always
/// removed before this function returns (see [`ScratchWorktreeGuard`]).
///
/// Idempotent: if `branch` already exists on `origin` and already carries
/// the requested version (and/or the changelog heading), the corresponding
/// step is skipped and no new commit is made for it. Calling this a second
/// time after a partial failure completes what is missing rather than
/// duplicating what already landed.
fn prepare_bump_branch(project_root: &Path, version: &str) -> Result<PreparedBranch, String> {
    let branch = release_branch_name(version);

    // 1. Fetch `develop` so the start point is current. This is an action
    // step, so a fetch is correct here (unlike the read-only observer).
    fetch_ref(project_root, "develop")?;

    let already_on_origin = branch_exists_on_origin(project_root, &branch)?;
    if already_on_origin {
        fetch_ref(project_root, &branch)?;
    }

    // 2. Create (or reuse) the scratch worktree, under the same directory
    // `worktree::worktrees_dir` already uses, named from the version so a
    // concurrent run on a different version cannot collide.
    let scratch_path =
        worktree::worktrees_dir(project_root).join(format!("release-bump-{version}"));
    add_scratch_worktree(project_root, &scratch_path, &branch, !already_on_origin)?;
    let _guard = ScratchWorktreeGuard {
        project_root,
        path: scratch_path.clone(),
    };

    let target = version::parse_version_str(version).map_err(|err| err.to_string())?;
    let git_flow = GitFlow::new(&scratch_path);
    let mut commits_created = 0u32;

    // 3-5. The two-place version bump, `cargo build` (so `Cargo.lock` picks
    // up the new version), and the commit — skipped if this branch already
    // carries the requested version.
    let current = version::read_version(&scratch_path).map_err(|err| err.to_string())?;
    if current != target {
        version::write_version(&scratch_path, &target).map_err(|err| err.to_string())?;

        let build = git::hermetic_command("cargo", &scratch_path)
            .arg("build")
            .output()
            .map_err(|err| format!("failed to spawn cargo build: {}", err.kind()))?;
        if !build.status.success() {
            return Err(format!(
                "cargo build exited with status {}: {}",
                build.status,
                String::from_utf8_lossy(&build.stderr).trim()
            ));
        }

        commits_created += commit_if_changed(
            &git_flow,
            "Cargo.toml",
            &format!("chore(release): bump version to {version}"),
        )?;
        commits_created += commit_if_changed(
            &git_flow,
            "Cargo.lock",
            &format!("chore(release): bump version to {version}"),
        )?;
    }

    // 6. The changelog entry, via the existing content chain — skipped if
    // this branch's CHANGELOG.md already carries the heading.
    let existing_changelog =
        std::fs::read_to_string(scratch_path.join("CHANGELOG.md")).unwrap_or_default();
    let changelog_already_written = matches!(
        classify_changelog_heading(&existing_changelog, version),
        Observation::Present { .. }
    );
    if !changelog_already_written {
        let baseline =
            version::reachable_semver_baseline(&scratch_path).map_err(|err| err.to_string())?;
        let range_start = match &baseline {
            Some(tag) => version::release_range_start(&scratch_path, &format!("v{tag}"))
                .map_err(|err| err.to_string())?,
            None => String::new(),
        };
        let sections = version::changelog_sections(&scratch_path, &range_start)
            .map_err(|err| err.to_string())?;
        let body = version::render_changelog_body(&sections);
        let updated = crate::ship::prepend_changelog(
            &existing_changelog,
            version,
            &crate::hooks::today(),
            &body,
        );
        std::fs::write(scratch_path.join("CHANGELOG.md"), updated)
            .map_err(|err| err.to_string())?;

        commits_created += commit_if_changed(
            &git_flow,
            "CHANGELOG.md",
            &format!("docs(release): add changelog entry for {version}"),
        )?;
    }

    // 7. Push the branch to origin.
    let push = git::git_command(&scratch_path)
        .args(["push", "-u", "origin", &branch])
        .output()
        .map_err(|err| format!("failed to spawn git push: {}", err.kind()))?;
    if !push.status.success() {
        return Err(format!(
            "git push origin {branch} exited with status {}: {}",
            push.status,
            String::from_utf8_lossy(&push.stderr).trim()
        ));
    }

    Ok(PreparedBranch {
        branch,
        commits_created,
        already_prepared: commits_created == 0,
    })
}

/// `git_flow.commit_path(relative_path, message)`, returning `1` if the
/// commit actually landed (the branch tip moved) or `0` if it was a genuine
/// no-op (nothing to commit) — `commit_path` itself does not distinguish the
/// two, since a scoped "nothing to commit" is intentionally folded into
/// `Ok(())` (19b/D-16).
fn commit_if_changed(
    git_flow: &GitFlow,
    relative_path: &str,
    message: &str,
) -> Result<u32, String> {
    let before = git_flow.branch_tip("HEAD").map_err(|err| err.to_string())?;
    git_flow
        .commit_path(relative_path, message)
        .map_err(|err| err.to_string())?;
    let after = git_flow.branch_tip("HEAD").map_err(|err| err.to_string())?;
    Ok(u32::from(after != before))
}

/// Pure argument-vector builder for `gh pr create` — returned as owned
/// strings so it can be asserted in tests without executing anything. The
/// body is passed by file (`--body-file`), never inline, so no changelog or
/// version text ever crosses a shell argument (T-29-01).
fn pr_argv(head: &str, base: &str, title: &str, body_file: &Path) -> Vec<String> {
    vec![
        "pr".to_string(),
        "create".to_string(),
        "--head".to_string(),
        head.to_string(),
        "--base".to_string(),
        base.to_string(),
        "--title".to_string(),
        title.to_string(),
        "--body-file".to_string(),
        body_file.to_string_lossy().to_string(),
    ]
}

/// Pure argument-vector builder for `gh pr merge --auto <method>`. Always
/// includes `method.flag()`; because [`MergeMethod`] has no unspecified
/// variant, an argument vector with no method flag is unrepresentable
/// (T-29-08).
fn merge_argv(number: u64, method: MergeMethod) -> Vec<String> {
    vec![
        "pr".to_string(),
        "merge".to_string(),
        number.to_string(),
        "--auto".to_string(),
        method.flag().to_string(),
    ]
}

/// Write `body` to a fresh temporary file under the OS temp directory,
/// returning its path. `devflow-core` has no runtime dependency on
/// `tempfile` (a dev-dependency only), so this is a small hand-rolled
/// unique-name allocator rather than a new production dependency.
fn write_temp_body_file(body: &str) -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let path =
        std::env::temp_dir().join(format!("devflow-pr-body-{}-{nanos}.md", std::process::id()));
    std::fs::write(&path, body)
        .map_err(|err| format!("failed to write temporary PR body file: {err}"))?;
    Ok(path)
}

/// The shared pull-request helper every pull-request-backed release-cut step
/// uses: resolve the merge method for `intent` against `base`'s discovered
/// allowed set *before* creating anything (a required method discovered
/// unavailable after a pull request already exists would leave an armed PR
/// that cannot be merged correctly), open the pull request, read back its
/// number, then arm auto-merge with the resolved method as an explicit flag
/// — never a bare `--auto` (T-29-08, the 2026-07-27 incident).
pub fn open_and_arm_pr(
    project_root: &Path,
    head: &str,
    base: &str,
    title: &str,
    body: &str,
    intent: MergeIntent,
) -> Result<u64, String> {
    let allowed = release_policy::discover_allowed_merge_methods(project_root, base)?;
    let method =
        release_policy::resolve_merge_method(intent, &allowed).map_err(|err| err.to_string())?;

    let body_file = write_temp_body_file(body)?;
    let create_output = Command::new("gh")
        .current_dir(project_root)
        .args(pr_argv(head, base, title, &body_file))
        .output();
    let _ = std::fs::remove_file(&body_file);
    let create_output =
        create_output.map_err(|err| format!("failed to spawn gh pr create: {}", err.kind()))?;
    if !create_output.status.success() {
        return Err(format!(
            "gh pr create exited with status {}",
            create_output.status
        ));
    }

    let view_output = Command::new("gh")
        .current_dir(project_root)
        .args(["pr", "view", head, "--json", "number", "--jq", ".number"])
        .output()
        .map_err(|err| format!("failed to spawn gh pr view: {}", err.kind()))?;
    if !view_output.status.success() {
        return Err(format!(
            "gh pr view exited with status {}",
            view_output.status
        ));
    }
    let number: u64 = String::from_utf8_lossy(&view_output.stdout)
        .trim()
        .parse()
        .map_err(|_| "gh pr view returned an unparseable pull request number".to_string())?;

    let merge_output = Command::new("gh")
        .current_dir(project_root)
        .args(merge_argv(number, method))
        .output()
        .map_err(|err| format!("failed to spawn gh pr merge: {}", err.kind()))?;
    if !merge_output.status.success() {
        return Err(format!(
            "gh pr merge exited with status {}",
            merge_output.status
        ));
    }

    Ok(number)
}

/// The action for [`ReleaseStep::VersionBumped`] and
/// [`ReleaseStep::ChangelogWritten`]: prepare the release branch (both
/// changes, in a scratch worktree), then open and arm the pull request into
/// `develop` with [`MergeIntent::VersionBump`].
fn bump_and_changelog_pr(project_root: &Path, version: &str) -> Result<String, String> {
    let prepared = prepare_bump_branch(project_root, version)?;
    let title = format!("chore(release): bump version to {version}");
    let body = format!(
        "Version bump and changelog entry for `{version}`, prepared by `devflow release cut`."
    );
    let number = open_and_arm_pr(
        project_root,
        &prepared.branch,
        "develop",
        &title,
        &body,
        MergeIntent::VersionBump,
    )?;
    let method = release_policy::required_method(MergeIntent::VersionBump);
    Ok(format!(
        "prepared branch `{}`, opened pull request #{number} into develop, armed auto-merge with {method}",
        prepared.branch
    ))
}

/// The release pull request's title: begins `release: v<version>`, matching
/// CONTRIBUTING.md step 3's documented form (`release: vX.Y.Z — <short
/// description>`), followed by a short description. The prefix is kept
/// exact — it is the string a human scans the pull-request list for.
pub fn release_pr_title(version: &str) -> String {
    format!("release: v{version} — release cut")
}

/// The action for [`ReleaseStep::ReleasePrMerged`]: opens the release pull
/// request from `develop` into `main` with [`MergeIntent::ReleaseCut`]. There
/// is no branch to prepare — `develop` already carries the version bump and
/// the changelog entry by the time this step is reached, which is precisely
/// what the [`ReleaseStep::VersionBumped`] and [`ReleaseStep::ChangelogWritten`]
/// oracles observed before the walk got here. The method is resolved through
/// [`MergeIntent::ReleaseCut`], never shortcut on the grounds that `main`
/// happens to allow only squash today — that fact can change without a code
/// change, and discovery is what keeps this correct when it does.
fn release_pr_to_main(project_root: &Path, version: &str) -> Result<String, String> {
    let title = release_pr_title(version);
    let body = format!(
        "Release `{version}`. See the `## {version}` section of CHANGELOG.md on \
         `develop` for details. Opened by `devflow release cut`."
    );
    let number = open_and_arm_pr(
        project_root,
        "develop",
        "main",
        &title,
        &body,
        MergeIntent::ReleaseCut,
    )?;
    let method = release_policy::required_method(MergeIntent::ReleaseCut);
    Ok(format!(
        "opened pull request #{number} from develop into main, armed auto-merge with {method}"
    ))
}

/// The sync-back commit message, carried verbatim from
/// `scripts/sync-main-to-develop.sh` — this is the port, not a redesign.
fn sync_merge_message() -> &'static str {
    "merge: sync main back into develop after release\n\n\
     Standing post-release step (scripts/sync-main-to-develop.sh) — keeps main\n\
     a real ancestor of develop so the next release PR doesn't conflict against\n\
     a stale merge-base. -X ours: develop's content is authoritative; this\n\
     should be a no-op content-wise (verified below)."
}

/// Whether `origin/main` is already an ancestor of `origin/develop` — the
/// script's own early exit (`git merge-base --is-ancestor origin/main
/// origin/develop`), checked immediately after both refs are freshly
/// fetched. A re-run after a prior successful sync finds this `true` and
/// makes no branch and no pull request, exactly like the script's own exit
/// 0 short circuit.
fn main_already_synced(project_root: &Path) -> Result<bool, String> {
    let output = git::git_command(project_root)
        .args([
            "merge-base",
            "--is-ancestor",
            "origin/main",
            "origin/develop",
        ])
        .output()
        .map_err(|err| format!("failed to spawn git merge-base: {}", err.kind()))?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(format!(
            "git merge-base --is-ancestor origin/main origin/develop exited with status {}",
            output.status
        )),
    }
}

/// The outcome of [`merge_main_into_sync_branch_and_push`] — either the
/// short-circuit (nothing to do) or a successfully pushed sync branch, ready
/// for [`open_and_arm_pr`].
#[derive(Debug, Clone, PartialEq, Eq)]
enum SyncMergeOutcome {
    AlreadySynced,
    Pushed { branch: String },
}

/// Steps 1-6 of the sync-back port, plus the push — everything except
/// opening the pull request, which is the only step that talks to `gh`
/// rather than plain `git`. Kept separate from [`sync_back_pr`] so the
/// merge/tree-identity/cleanup logic is testable against real git fixtures
/// with no `gh`/GitHub dependency at all.
///
/// A port of `scripts/sync-main-to-develop.sh`, moved from
/// bash-plus-direct-push to Rust-plus-pull-request while preserving every
/// check the script performs. The two guards the script needs that this
/// executor does not — a clean tree, and being on the correct branch — are
/// true by construction here: the merge happens inside a scratch worktree
/// this function creates, never the operator's own checkout, which is
/// always left untouched (asserted by this module's own tests on the
/// success, refusal, and forced-failure paths).
///
/// 1. Fetch `main` and `develop` from `origin`.
/// 2. Short-circuit if `origin/main` is already an ancestor of
///    `origin/develop` — [`main_already_synced`], the script's own early
///    exit.
/// 3. Create (or reuse) a scratch worktree on [`sync_branch_name`] off
///    `origin/develop`.
/// 4. Record the pre-merge tree object id.
/// 5. Merge `origin/main` with `-X ours --no-edit`, using the script's own
///    commit message ([`sync_merge_message`]).
/// 6. Record the post-merge tree object id. If it differs from the
///    pre-merge id, refuse: `main` carried content `develop` genuinely
///    lacked, and nothing is pushed — the script's own hard exit, never
///    downgraded to a warning.
/// 7. Push the branch.
fn merge_main_into_sync_branch_and_push(
    project_root: &Path,
    version: &str,
) -> Result<SyncMergeOutcome, String> {
    fetch_ref(project_root, "main")?;
    fetch_ref(project_root, "develop")?;

    if main_already_synced(project_root)? {
        return Ok(SyncMergeOutcome::AlreadySynced);
    }

    let branch = sync_branch_name(version);
    let already_on_origin = branch_exists_on_origin(project_root, &branch)?;
    if already_on_origin {
        fetch_ref(project_root, &branch)?;
    }

    let scratch_path =
        worktree::worktrees_dir(project_root).join(format!("release-sync-{version}"));
    add_scratch_worktree(project_root, &scratch_path, &branch, !already_on_origin)?;
    let _guard = ScratchWorktreeGuard {
        project_root,
        path: scratch_path.clone(),
    };

    let before_tree = tree_object_id(&scratch_path)?;

    let merge = git::git_command(&scratch_path)
        .args([
            "merge",
            "-X",
            "ours",
            "origin/main",
            "--no-edit",
            "-m",
            sync_merge_message(),
        ])
        .output()
        .map_err(|err| format!("failed to spawn git merge: {}", err.kind()))?;
    if !merge.status.success() {
        return Err(format!(
            "git merge -X ours origin/main exited with status {}: {}",
            merge.status,
            String::from_utf8_lossy(&merge.stderr).trim()
        ));
    }

    let after_tree = tree_object_id(&scratch_path)?;
    if before_tree != after_tree {
        return Err(format!(
            "the sync merge changed develop's tree (before: {before_tree}, after: {after_tree}) \
             — main carried content develop genuinely lacked; a human must inspect the merge \
             before anything is pushed"
        ));
    }

    let push = git::git_command(&scratch_path)
        .args(["push", "-u", "origin", &branch])
        .output()
        .map_err(|err| format!("failed to spawn git push: {}", err.kind()))?;
    if !push.status.success() {
        return Err(format!(
            "git push origin {branch} exited with status {}: {}",
            push.status,
            String::from_utf8_lossy(&push.stderr).trim()
        ));
    }

    Ok(SyncMergeOutcome::Pushed { branch })
}

/// The action for [`ReleaseStep::SyncMerged`]: [`merge_main_into_sync_branch_and_push`]
/// (the git-only port of `scripts/sync-main-to-develop.sh`), then — if a
/// branch was actually pushed — open and arm its pull request with
/// [`MergeIntent::SyncBack`], which resolves to a real merge commit, never
/// squash, by construction, not by convention (T-29-08).
fn sync_back_pr(project_root: &Path, version: &str) -> Result<String, String> {
    match merge_main_into_sync_branch_and_push(project_root, version)? {
        SyncMergeOutcome::AlreadySynced => Ok(
            "origin/main is already an ancestor of origin/develop — nothing to sync".to_string(),
        ),
        SyncMergeOutcome::Pushed { branch } => {
            let title = format!("sync: main → develop after v{version}");
            let body = "Standing post-release sync — links main's release commit as an \
                        ancestor of develop (scripts/sync-main-to-develop.sh, ported). Must be \
                        merged with a real merge commit, never squashed: squashing destroys the \
                        ancestry link this pull request exists to create."
                .to_string();
            let number = open_and_arm_pr(
                project_root,
                &branch,
                "develop",
                &title,
                &body,
                MergeIntent::SyncBack,
            )?;
            let method = release_policy::required_method(MergeIntent::SyncBack);
            Ok(format!(
                "pushed branch `{branch}`, opened pull request #{number} into develop, armed auto-merge with {method}"
            ))
        }
    }
}

/// `git rev-parse HEAD^{tree}` in `repo` — the working tree's object id,
/// used by [`sync_back_pr`] to compare the pre-merge and post-merge tree,
/// exactly as `scripts/sync-main-to-develop.sh` lines 51-62 do.
fn tree_object_id(repo: &Path) -> Result<String, String> {
    let output = git::git_command(repo)
        .args(["rev-parse", "HEAD^{tree}"])
        .output()
        .map_err(|err| format!("failed to spawn git rev-parse: {}", err.kind()))?;
    if !output.status.success() {
        return Err(format!(
            "git rev-parse HEAD^{{tree}} exited with status {}",
            output.status
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The action for [`ReleaseStep::SignedTagPresent`] (unit 29c, the first of
/// the two irreversible commit-point operations): resolve the target commit
/// as the current tip of `origin/main` — the commit the release pull request
/// ([`ReleaseStep::ReleasePrMerged`]) produced — then call
/// [`crate::release_publish::create_and_push_tag`]. No signing-viability
/// prediction happens here or in `release_publish` (D-10): the tag command
/// runs for real and git's own exit code and stderr are the report.
fn sign_release_tag(project_root: &Path, version: &str) -> Result<String, String> {
    fetch_ref(project_root, "main")?;
    let output = git::git_command(project_root)
        .args(["rev-parse", "origin/main"])
        .output()
        .map_err(|err| format!("failed to spawn git rev-parse: {}", err.kind()))?;
    if !output.status.success() {
        return Err(format!(
            "git rev-parse origin/main exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let target_commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
    crate::release_publish::create_and_push_tag(project_root, version, &target_commit)
}

/// The action for [`ReleaseStep::CratesPublished`] (unit 29c, the second and
/// most irreversible commit-point operation):
/// [`crate::release_publish::publish_all`], joined into the single-line
/// detail [`StepAction`] requires.
fn publish_crates(project_root: &Path, version: &str) -> Result<String, String> {
    let published = crate::release_publish::publish_all(project_root, version)?;
    Ok(published.join("; "))
}

/// The mandate-refusal reason, naming all three ways to grant a release-cut
/// mandate. Shared by [`cut_with`]'s unauthorized-refusal path so the CLI
/// layer and every test see byte-identical wording.
fn no_mandate_reason() -> String {
    "a release requires an explicit mandate — grant one via --yes-release, \
     `yes_release = true` in devflow.toml, or DEVFLOW_YES_RELEASE"
        .to_string()
}

/// Walk the six release-cut steps, observing each via
/// [`crate::release_observe::observe`]. See the module doc comment for the
/// walk's shape.
pub fn cut(project_root: &Path, version: &str, authorized: bool) -> CutReport {
    cut_with(project_root, version, authorized, |root, step, version| {
        observe(root, step, version)
    })
}

/// The walk's control flow, with the observation source injected — lets
/// tests exercise the walk against a sequence of [`Observation`]s with no
/// network. [`cut`] passes [`crate::release_observe::observe`].
fn cut_with(
    project_root: &Path,
    version: &str,
    authorized: bool,
    observe_fn: impl Fn(&Path, ReleaseStep, &str) -> Observation,
) -> CutReport {
    if !authorized {
        return CutReport {
            steps: vec![(
                ReleaseStep::ALL[0],
                StepOutcome::Stopped {
                    reason: no_mandate_reason(),
                },
            )],
            authorized: false,
        };
    }

    let mut steps = Vec::new();
    for step in ReleaseStep::ALL {
        match observe_fn(project_root, step, version) {
            Observation::Present { detail } => {
                steps.push((step, StepOutcome::AlreadyDone { detail }));
            }
            Observation::Unreachable { reason } => {
                steps.push((step, StepOutcome::Stopped { reason }));
                break;
            }
            Observation::Absent { .. } => {
                if let Some((head, base)) = pr_refs(step, version) {
                    match open_pr(project_root, &head, &base) {
                        PrPresence::Open { number } => {
                            steps.push((
                                step,
                                StepOutcome::InFlight {
                                    detail: format!(
                                        "pull request #{number} is already open ({head} -> {base})"
                                    ),
                                },
                            ));
                            break;
                        }
                        PrPresence::Unreachable { reason } => {
                            steps.push((step, StepOutcome::Stopped { reason }));
                            break;
                        }
                        PrPresence::None => {
                            // Fall through to the step's own action, below.
                        }
                    }
                }

                match action_for(step) {
                    None => {
                        steps.push((
                            step,
                            StepOutcome::NoActionInThisBuild {
                                unit: unit_for(step),
                            },
                        ));
                    }
                    Some(action) => match action(project_root, version) {
                        Ok(detail) => steps.push((step, StepOutcome::Performed { detail })),
                        Err(reason) => steps.push((step, StepOutcome::Stopped { reason })),
                    },
                }
                break;
            }
        }
    }

    CutReport {
        steps,
        authorized: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use tempfile::TempDir;

    fn present(detail: &str) -> Observation {
        Observation::Present {
            detail: detail.into(),
        }
    }

    fn absent(detail: &str) -> Observation {
        Observation::Absent {
            detail: detail.into(),
        }
    }

    fn unreachable(reason: &str) -> Observation {
        Observation::Unreachable {
            reason: reason.into(),
        }
    }

    // -- StepOutcome::is_terminal ---------------------------------------

    #[test]
    fn is_terminal_true_for_every_variant_but_already_done() {
        assert!(!StepOutcome::AlreadyDone { detail: "x".into() }.is_terminal());
        assert!(StepOutcome::InFlight { detail: "x".into() }.is_terminal());
        assert!(StepOutcome::Performed { detail: "x".into() }.is_terminal());
        assert!(StepOutcome::Stopped { reason: "x".into() }.is_terminal());
        assert!(StepOutcome::NoActionInThisBuild { unit: "29b" }.is_terminal());
    }

    // -- CutReport::stopped_at / all_done ---------------------------------

    #[test]
    fn all_done_true_only_when_every_step_is_already_done() {
        let steps = ReleaseStep::ALL
            .iter()
            .map(|&s| {
                (
                    s,
                    StepOutcome::AlreadyDone {
                        detail: "done".into(),
                    },
                )
            })
            .collect();
        let report = CutReport {
            steps,
            authorized: true,
        };
        assert!(report.all_done());
        assert!(report.stopped_at().is_none());
    }

    #[test]
    fn stopped_at_returns_the_first_non_already_done_entry() {
        let report = CutReport {
            steps: vec![
                (
                    ReleaseStep::VersionBumped,
                    StepOutcome::AlreadyDone {
                        detail: "done".into(),
                    },
                ),
                (
                    ReleaseStep::ChangelogWritten,
                    StepOutcome::Stopped {
                        reason: "stopped here".into(),
                    },
                ),
            ],
            authorized: true,
        };
        assert!(!report.all_done());
        let (step, outcome) = report.stopped_at().expect("expected a stop");
        assert_eq!(step, ReleaseStep::ChangelogWritten);
        match outcome {
            StepOutcome::Stopped { reason } => assert_eq!(reason, "stopped here"),
            other => panic!("expected Stopped, got {other:?}"),
        }
    }

    // -- action_for / unit_for --------------------------------------------

    #[test]
    fn action_for_returns_an_action_for_every_step_in_this_build() {
        // 29-07 wires the last two remaining arms (SignedTagPresent,
        // CratesPublished) — with unit 29c complete, no step in
        // `ReleaseStep::ALL` returns `None` anymore.
        for step in ReleaseStep::ALL {
            assert!(
                action_for(step).is_some(),
                "expected {step:?} to resolve to an action now that unit 29c is complete"
            );
        }
    }

    #[test]
    fn action_for_resolves_version_bumped_and_changelog_written_to_the_bump_action() {
        assert!(
            action_for(ReleaseStep::VersionBumped).is_some(),
            "expected VersionBumped to resolve to an action in this build"
        );
        assert!(
            action_for(ReleaseStep::ChangelogWritten).is_some(),
            "expected ChangelogWritten to resolve to an action in this build"
        );
    }

    #[test]
    fn action_for_returns_an_action_for_release_pr_merged() {
        assert!(
            action_for(ReleaseStep::ReleasePrMerged).is_some(),
            "expected ReleasePrMerged to resolve to an action in this build"
        );
    }

    #[test]
    fn action_for_returns_an_action_for_sync_merged() {
        assert!(
            action_for(ReleaseStep::SyncMerged).is_some(),
            "expected SyncMerged to resolve to an action in this build"
        );
    }

    #[test]
    fn unit_for_maps_pr_backed_steps_to_29b_and_commit_point_steps_to_29c() {
        assert_eq!(unit_for(ReleaseStep::VersionBumped), "29b");
        assert_eq!(unit_for(ReleaseStep::ChangelogWritten), "29b");
        assert_eq!(unit_for(ReleaseStep::ReleasePrMerged), "29b");
        assert_eq!(unit_for(ReleaseStep::SyncMerged), "29b");
        assert_eq!(unit_for(ReleaseStep::SignedTagPresent), "29c");
        assert_eq!(unit_for(ReleaseStep::CratesPublished), "29c");
    }

    // -- pr_refs ------------------------------------------------------------

    #[test]
    fn pr_refs_is_deterministic_and_none_for_non_pr_backed_steps() {
        assert_eq!(
            pr_refs(ReleaseStep::VersionBumped, "1.2.3"),
            Some(("release/bump-v1.2.3".to_string(), "develop".to_string()))
        );
        assert_eq!(
            pr_refs(ReleaseStep::ChangelogWritten, "1.2.3"),
            Some(("release/bump-v1.2.3".to_string(), "develop".to_string()))
        );
        assert_eq!(
            pr_refs(ReleaseStep::ReleasePrMerged, "1.2.3"),
            Some(("develop".to_string(), "main".to_string()))
        );
        assert_eq!(
            pr_refs(ReleaseStep::SyncMerged, "1.2.3"),
            Some((
                "sync/main-to-develop-v1.2.3".to_string(),
                "develop".to_string()
            ))
        );
        assert_eq!(pr_refs(ReleaseStep::SignedTagPresent, "1.2.3"), None);
        assert_eq!(pr_refs(ReleaseStep::CratesPublished, "1.2.3"), None);
    }

    #[test]
    fn release_branch_name_matches_pr_refs_head_for_version_bump_and_changelog_steps() {
        let version = "2.3.0";
        let expected = release_branch_name(version);
        assert_eq!(
            pr_refs(ReleaseStep::VersionBumped, version).unwrap().0,
            expected
        );
        assert_eq!(
            pr_refs(ReleaseStep::ChangelogWritten, version).unwrap().0,
            expected
        );
    }

    // -- cut_with: the walk's control flow --------------------------------

    #[test]
    fn present_then_absent_records_already_done_and_stops_on_absent() {
        // ChangelogWritten is pull-request-backed (`pr_refs` returns
        // `Some`), so the walk's own in-flight check runs a real `gh`
        // invocation against `/nonexistent` — which fails to even spawn
        // (an invalid `current_dir`), deterministically yielding
        // `PrPresence::Unreachable` regardless of the host's `gh`
        // installation. This test asserts on the walk's shape (stops on
        // the absent step), not on which terminal variant that failure
        // produces.
        let report = cut_with(
            Path::new("/nonexistent"),
            "1.2.3",
            true,
            |_, step, _| match step {
                ReleaseStep::VersionBumped => present("bumped"),
                ReleaseStep::ChangelogWritten => absent("not written"),
                _ => panic!("observer must not be called past the stop"),
            },
        );
        assert_eq!(report.steps.len(), 2);
        assert!(matches!(
            report.steps[0],
            (ReleaseStep::VersionBumped, StepOutcome::AlreadyDone { .. })
        ));
        assert_eq!(report.steps[1].0, ReleaseStep::ChangelogWritten);
        assert!(
            report.steps[1].1.is_terminal(),
            "the walk must stop on the absent step, got {:?}",
            report.steps[1].1
        );
    }

    #[test]
    fn unreachable_first_step_stops_immediately_with_the_reason_verbatim() {
        let report = cut_with(
            Path::new("/nonexistent"),
            "1.2.3",
            true,
            |_, step, _| match step {
                ReleaseStep::VersionBumped => unreachable("network partition"),
                _ => panic!("observer must not be called past an Unreachable stop"),
            },
        );
        assert_eq!(report.steps.len(), 1);
        match &report.steps[0] {
            (ReleaseStep::VersionBumped, StepOutcome::Stopped { reason }) => {
                assert_eq!(reason, "network partition");
            }
            other => panic!("expected Stopped naming the injected reason, got {other:?}"),
        }
    }

    #[test]
    fn a_stop_partway_through_records_no_outcome_for_later_steps() {
        // Stop at ReleasePrMerged (index 2, the third step): the report
        // must contain exactly index + 1 = 3 entries, and the observer must
        // never be consulted for SignedTagPresent, SyncMerged, or
        // CratesPublished.
        let report = cut_with(
            Path::new("/nonexistent"),
            "1.2.3",
            true,
            |_, step, _| match step {
                ReleaseStep::VersionBumped => present("bumped"),
                ReleaseStep::ChangelogWritten => present("written"),
                ReleaseStep::ReleasePrMerged => unreachable("gh api exited with status 1"),
                _ => panic!("observer must not be called for a step after the stop"),
            },
        );
        assert_eq!(
            report.steps.len(),
            3,
            "report length must equal the stop index (2) plus one"
        );
        assert!(matches!(
            report.steps[2],
            (ReleaseStep::ReleasePrMerged, StepOutcome::Stopped { .. })
        ));
    }

    #[test]
    fn unauthorized_yields_a_single_refusal_and_invokes_no_observer() {
        let observer_calls = RefCell::new(0u32);
        let report = cut_with(Path::new("/nonexistent"), "1.2.3", false, |_, _, _| {
            *observer_calls.borrow_mut() += 1;
            present("should never run")
        });
        assert_eq!(
            *observer_calls.borrow(),
            0,
            "authorized=false must not observe anything"
        );
        assert_eq!(report.steps.len(), 1);
        assert!(!report.authorized);
        match &report.steps[0].1 {
            StepOutcome::Stopped { reason } => {
                assert!(reason.contains("--yes-release"));
                assert!(reason.contains("yes_release"));
                assert!(reason.contains("DEVFLOW_YES_RELEASE"));
            }
            other => panic!("expected a mandate refusal, got {other:?}"),
        }
    }

    #[test]
    fn absent_signed_tag_step_now_attempts_its_real_action_instead_of_reporting_no_action() {
        // SignedTagPresent is not pull-request-backed (`pr_refs` returns
        // `None`), so the in-flight check is skipped entirely and the walk
        // goes straight to `action_for` — which, as of 29-07, is `Some`.
        // Against a nonexistent project root the real action
        // (`sign_release_tag`) fails to even spawn `git fetch`, so the walk
        // stops with a real, non-empty failure reason rather than the old
        // `NoActionInThisBuild` outcome this test used to assert on.
        let report = cut_with(
            Path::new("/nonexistent"),
            "1.2.3",
            true,
            |_, step, _| match step {
                ReleaseStep::VersionBumped => present("bumped"),
                ReleaseStep::ChangelogWritten => present("written"),
                ReleaseStep::ReleasePrMerged => present("merged"),
                ReleaseStep::SignedTagPresent => absent("no tag yet"),
                _ => panic!("observer must not be called past the stop"),
            },
        );
        assert_eq!(report.steps.len(), 4);
        match &report.steps[3] {
            (ReleaseStep::SignedTagPresent, StepOutcome::Stopped { reason }) => {
                assert!(
                    !reason.is_empty(),
                    "expected a real, non-empty failure reason from the real action"
                );
            }
            other => panic!("expected the real action to be attempted and fail, got {other:?}"),
        }
    }

    /// A pull-request-backed step observed `Absent`, checked against a
    /// directory that is not even a git repository: `gh pr list` cannot
    /// resolve a repository from it, so the in-flight check itself is
    /// `Unreachable` regardless of whether `gh` happens to be installed —
    /// deterministic without any network or fixture dependency.
    #[test]
    fn absent_pr_backed_step_with_an_unreachable_in_flight_check_stops() {
        let dir = tempfile::tempdir().unwrap();
        let report = cut_with(dir.path(), "1.2.3", true, |_, step, _| match step {
            ReleaseStep::VersionBumped => absent("not bumped"),
            _ => panic!("observer must not be called past the stop"),
        });
        assert_eq!(report.steps.len(), 1);
        match &report.steps[0] {
            (ReleaseStep::VersionBumped, StepOutcome::Stopped { reason }) => {
                assert!(!reason.is_empty());
            }
            other => panic!("expected Stopped from an unreachable in-flight check, got {other:?}"),
        }
    }

    /// End-to-end against a real repository with no remote at all: `cut`'s
    /// public entry point (not `cut_with`) runs the real observer, and the
    /// first step's failure carries git's/gh's own real failure text
    /// rather than a predicted one.
    #[test]
    fn cut_authorized_against_a_repo_with_no_remote_refuses_on_the_first_step() {
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
        git(&["checkout", "-q", "-b", "develop"]);

        let report = cut(root, "1.2.3", true);
        assert_eq!(
            report.steps.len(),
            1,
            "expected the walk to stop on the very first step with no remote"
        );
        match &report.steps[0] {
            (ReleaseStep::VersionBumped, StepOutcome::Stopped { reason }) => {
                assert!(
                    !reason.is_empty(),
                    "expected a real, non-empty failure reason"
                );
            }
            other => panic!("expected Stopped on the first step, got {other:?}"),
        }
    }

    // -- Task 1: prepare_bump_branch ---------------------------------------

    fn git_at(root: &Path, args: &[&str]) {
        let output = crate::test_support::git_command(root)
            .args(args)
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {args:?} in {root:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_output_at(root: &Path, args: &[&str]) -> String {
        let output = crate::test_support::git_command(root)
            .args(args)
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {args:?} in {root:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Build a two-crate workspace fixture mirroring this repository's own
    /// self-pin shape (a `[workspace.dependencies]` entry pinning a
    /// local-path member), with `origin` a real, local bare repository and
    /// `develop` already pushed to it. The workspace declares zero external
    /// dependencies, so `cargo build` is network-free and fast either way.
    /// `lib_rs` is the member crate's source — a broken-syntax variant lets
    /// a test force `cargo build` to fail deterministically, after the
    /// scratch worktree already exists.
    fn init_workspace_repo(version: &str, lib_rs: &str) -> (TempDir, TempDir) {
        let origin_dir = tempfile::tempdir().unwrap();
        git_at(origin_dir.path(), &["init", "-q", "--bare"]);

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_at(root, &["init", "-q"]);
        git_at(root, &["config", "user.email", "test@example.com"]);
        git_at(root, &["config", "user.name", "Test"]);
        git_at(root, &["config", "commit.gpgsign", "false"]);
        git_at(root, &["config", "core.hooksPath", "/dev/null"]);

        std::fs::create_dir_all(root.join("crates/core/src")).unwrap();
        std::fs::write(root.join("crates/core/src/lib.rs"), lib_rs).unwrap();
        std::fs::write(
            root.join("crates/core/Cargo.toml"),
            "[package]\nname = \"core\"\nversion.workspace = true\nedition.workspace = true\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            format!(
                "[workspace]\nmembers = [\"crates/core\"]\n\n\
                 [workspace.package]\nversion = \"{version}\"\nedition = \"2021\"\n\n\
                 [workspace.dependencies]\n\
                 core = {{ path = \"crates/core\", version = \"{version}\" }}\n"
            ),
        )
        .unwrap();
        std::fs::write(
            root.join("CHANGELOG.md"),
            "# Changelog\n\nAll notable changes to this project are documented here.\n",
        )
        .unwrap();
        std::fs::write(root.join(".gitignore"), "/target\n").unwrap();

        git_at(root, &["add", "."]);
        git_at(root, &["commit", "-q", "-m", "feat: base"]);
        git_at(root, &["branch", "-M", "main"]);
        git_at(root, &["checkout", "-q", "-b", "develop"]);
        git_at(
            root,
            &[
                "remote",
                "add",
                "origin",
                origin_dir.path().to_str().expect("utf-8 tempdir path"),
            ],
        );
        git_at(root, &["push", "-q", "-u", "origin", "develop"]);

        (dir, origin_dir)
    }

    #[test]
    fn prepare_bump_branch_rewrites_the_two_place_version_bump() {
        // The self-pin starts stale (1.0.0) relative to the requested
        // version (1.1.0) — a fixture with only [workspace.package]
        // rewritten would let the two-place requirement regress undetected.
        let (repo, _origin) = init_workspace_repo("1.0.0", "");
        let root = repo.path();

        let prepared = prepare_bump_branch(root, "1.1.0").expect("prepare_bump_branch");
        assert_eq!(prepared.branch, "release/bump-v1.1.0");
        assert!(prepared.commits_created > 0);

        // Verify against origin's own copy of the branch, not a leftover
        // local worktree (which prepare_bump_branch always removes).
        git_at(root, &["fetch", "-q", "origin", &prepared.branch]);
        let cargo_toml = git_output_at(
            root,
            &["show", &format!("origin/{}:Cargo.toml", prepared.branch)],
        );
        assert!(
            cargo_toml.contains("version = \"1.1.0\""),
            "expected [workspace.package] version to be rewritten, got: {cargo_toml}"
        );
        assert!(
            cargo_toml.contains("core = { path = \"crates/core\", version = \"1.1.0\" }"),
            "expected the self-pin to be rewritten alongside [workspace.package] version, got: {cargo_toml}"
        );
    }

    #[test]
    fn prepare_bump_branch_writes_the_changelog_heading() {
        let (repo, _origin) = init_workspace_repo("1.0.0", "");
        let root = repo.path();

        let prepared = prepare_bump_branch(root, "1.1.0").expect("prepare_bump_branch");

        git_at(root, &["fetch", "-q", "origin", &prepared.branch]);
        let changelog = git_output_at(
            root,
            &["show", &format!("origin/{}:CHANGELOG.md", prepared.branch)],
        );
        assert!(
            changelog
                .lines()
                .any(|line| line.trim().starts_with("## 1.1.0")),
            "expected a `## 1.1.0` heading, got: {changelog}"
        );
    }

    #[test]
    fn prepare_bump_branch_produces_commits_on_the_release_branch() {
        let (repo, _origin) = init_workspace_repo("1.0.0", "");
        let root = repo.path();

        let prepared = prepare_bump_branch(root, "1.1.0").expect("prepare_bump_branch");
        assert!(
            prepared.commits_created > 0,
            "expected at least one commit to be created"
        );

        git_at(root, &["fetch", "-q", "origin", &prepared.branch]);
        let log = git_output_at(
            root,
            &[
                "log",
                "--oneline",
                &format!("develop..origin/{}", prepared.branch),
            ],
        );
        assert!(
            !log.trim().is_empty(),
            "expected at least one commit ahead of develop on the release branch"
        );
    }

    #[test]
    fn prepare_bump_branch_leaves_the_scratch_worktrees_tree_clean() {
        let (repo, _origin) = init_workspace_repo("1.0.0", "");
        let root = repo.path();

        let prepared = prepare_bump_branch(root, "1.1.0").expect("prepare_bump_branch");

        // Check out the pushed branch fresh elsewhere: if the scratch
        // worktree had left anything uncommitted (e.g. `cargo build`'s
        // `target/` directory, or a half-written file), a fresh checkout of
        // exactly what was pushed still reports clean — proving the scratch
        // worktree's own tree was clean at the moment it was torn down.
        git_at(root, &["fetch", "-q", "origin", &prepared.branch]);
        let verify_dir = tempfile::tempdir().unwrap();
        let verify_path = verify_dir.path().join("verify");
        git_at(
            root,
            &[
                "worktree",
                "add",
                "--detach",
                verify_path.to_str().unwrap(),
                &format!("origin/{}", prepared.branch),
            ],
        );
        let status = git_output_at(&verify_path, &["status", "--porcelain"]);
        git_at(
            root,
            &[
                "worktree",
                "remove",
                "--force",
                verify_path.to_str().unwrap(),
            ],
        );
        assert!(
            status.is_empty(),
            "expected a clean tree on the pushed branch, got: {status}"
        );
    }

    #[test]
    fn prepare_bump_branch_leaves_the_operators_checkout_untouched() {
        let (repo, _origin) = init_workspace_repo("1.0.0", "");
        let root = repo.path();

        let before_head = git_output_at(root, &["rev-parse", "HEAD"]);
        let before_branch = git_output_at(root, &["rev-parse", "--abbrev-ref", "HEAD"]);
        let before_status = git_output_at(root, &["status", "--porcelain"]);

        prepare_bump_branch(root, "1.1.0").expect("prepare_bump_branch");

        let after_head = git_output_at(root, &["rev-parse", "HEAD"]);
        let after_branch = git_output_at(root, &["rev-parse", "--abbrev-ref", "HEAD"]);
        let after_status = git_output_at(root, &["status", "--porcelain"]);

        assert_eq!(before_head, after_head, "HEAD must be unchanged");
        assert_eq!(
            before_branch, after_branch,
            "current branch must be unchanged"
        );
        assert_eq!(
            before_status, after_status,
            "working tree status must be unchanged"
        );
    }

    #[test]
    fn prepare_bump_branch_removes_the_scratch_worktree_on_success() {
        let (repo, _origin) = init_workspace_repo("1.0.0", "");
        let root = repo.path();

        prepare_bump_branch(root, "1.1.0").expect("prepare_bump_branch");

        let listing = worktree::list(root).expect("list");
        assert_eq!(
            listing.len(),
            1,
            "expected only the original worktree to remain, got: {listing:?}"
        );
    }

    #[test]
    fn prepare_bump_branch_removes_the_scratch_worktree_on_a_forced_failure() {
        // A member crate with invalid Rust source forces `cargo build` to
        // fail deterministically, after the scratch worktree has already
        // been created — exercising the cleanup-on-failure path.
        let (repo, _origin) = init_workspace_repo("1.0.0", "fn broken( {");
        let root = repo.path();

        let result = prepare_bump_branch(root, "1.1.0");
        assert!(
            result.is_err(),
            "expected the forced build failure to propagate"
        );

        let listing = worktree::list(root).expect("list");
        assert_eq!(
            listing.len(),
            1,
            "expected the scratch worktree to be removed even after a forced failure, got: {listing:?}"
        );
    }

    #[test]
    fn prepare_bump_branch_pushes_the_branch_to_origin() {
        let (repo, origin) = init_workspace_repo("1.0.0", "");
        let root = repo.path();

        let prepared = prepare_bump_branch(root, "1.1.0").expect("prepare_bump_branch");

        let refs = git_output_at(
            origin.path(),
            &[
                "show-ref",
                "--verify",
                &format!("refs/heads/{}", prepared.branch),
            ],
        );
        assert!(
            !refs.is_empty(),
            "expected the release branch to exist on origin after prepare_bump_branch"
        );
    }

    #[test]
    fn prepare_bump_branch_rerun_after_success_makes_no_new_commit_and_reports_already_prepared() {
        let (repo, _origin) = init_workspace_repo("1.0.0", "");
        let root = repo.path();

        let first = prepare_bump_branch(root, "1.1.0").expect("first prepare_bump_branch");
        assert!(!first.already_prepared);
        assert!(first.commits_created > 0);

        let second = prepare_bump_branch(root, "1.1.0").expect("second prepare_bump_branch");
        assert!(
            second.already_prepared,
            "expected the second run to report already-prepared, got: {second:?}"
        );
        assert_eq!(
            second.commits_created, 0,
            "expected the second run to make no new commit, got: {second:?}"
        );
        assert_eq!(second.branch, first.branch);
    }

    // -- Task 2: pr_argv / merge_argv / open_and_arm_pr ---------------------

    #[test]
    fn pr_argv_never_inlines_the_body_and_always_uses_a_body_file_flag() {
        let body_file = Path::new("/tmp/devflow-pr-body-test.md");
        let argv = pr_argv(
            "release/bump-v1.2.3",
            "develop",
            "chore(release): bump version to 1.2.3",
            body_file,
        );
        assert!(argv.contains(&"--body-file".to_string()));
        assert!(
            !argv.iter().any(|arg| arg == "--body"),
            "the body must never be passed inline: {argv:?}"
        );
        assert!(argv.contains(&"release/bump-v1.2.3".to_string()));
        assert!(argv.contains(&"develop".to_string()));
    }

    #[test]
    fn merge_argv_contains_auto_and_the_pr_number() {
        let argv = merge_argv(99, MergeMethod::Squash);
        assert!(argv.contains(&"--auto".to_string()));
        assert!(argv.contains(&"99".to_string()));
        assert!(argv.contains(&"--squash".to_string()));
    }

    #[test]
    fn merge_argv_uses_the_merge_flag_for_merge_method() {
        let argv = merge_argv(1, MergeMethod::Merge);
        assert!(argv.contains(&"--merge".to_string()));
        assert!(!argv.contains(&"--squash".to_string()));
    }

    #[test]
    fn merge_argv_uses_the_squash_flag_for_squash_method() {
        let argv = merge_argv(1, MergeMethod::Squash);
        assert!(argv.contains(&"--squash".to_string()));
        assert!(!argv.contains(&"--merge".to_string()));
    }

    /// The invariant this phase's design promotes: every merge invocation
    /// carries an explicit method. Iterates every `MergeIntent` against
    /// every allowed set that resolves successfully and asserts each built
    /// argument vector contains one of the two flags — this test goes red
    /// the moment a future change reintroduces a hardcoded or omitted
    /// method (confirmed manually: removing `method.flag()` from
    /// `merge_argv` makes this test fail).
    #[test]
    fn merge_argv_always_carries_a_method_flag() {
        let intents = [
            MergeIntent::VersionBump,
            MergeIntent::ReleaseCut,
            MergeIntent::SyncBack,
        ];
        let allowed_candidates: [&[&str]; 3] = [&["merge"], &["squash"], &["merge", "squash"]];
        let mut checked_at_least_one = false;
        for intent in intents {
            for candidate in allowed_candidates {
                let allowed: Vec<String> = candidate.iter().map(|s| s.to_string()).collect();
                if let Ok(method) = release_policy::resolve_merge_method(intent, &allowed) {
                    checked_at_least_one = true;
                    let argv = merge_argv(42, method);
                    assert!(
                        argv.contains(&"--merge".to_string())
                            || argv.contains(&"--squash".to_string()),
                        "merge_argv for {intent:?}/{method:?} missing a method flag: {argv:?}"
                    );
                }
            }
        }
        assert!(
            checked_at_least_one,
            "expected at least one successful resolution to check"
        );
    }

    /// `open_and_arm_pr` resolves the merge method before contacting GitHub
    /// for pull-request creation or merge at all — the function body calls
    /// `discover_allowed_merge_methods(...)?` then
    /// `resolve_merge_method(...)...?` before any `gh pr create`/`gh pr
    /// merge` invocation, so a discovery/resolution failure short-circuits
    /// via `?` and no such call can ever be reached (a compiler-enforced
    /// control-flow fact). Proven here by pointing `project_root` at a
    /// directory `gh` cannot resolve any repository context from, which
    /// fails `discover_allowed_merge_methods` immediately — the same early
    /// code path a `resolve_merge_method` failure (already exhaustively
    /// unit-tested in `release_policy.rs`, e.g. the required-method-absent
    /// case) would take.
    #[test]
    fn open_and_arm_pr_refuses_before_contacting_github_when_resolution_fails() {
        let dir = tempfile::tempdir().unwrap();
        let result = open_and_arm_pr(
            dir.path(),
            "release/bump-v1.2.3",
            "develop",
            "chore(release): bump version to 1.2.3",
            "body",
            MergeIntent::VersionBump,
        );
        assert!(
            result.is_err(),
            "expected discovery/resolution failure to refuse before any gh pr create/merge call"
        );
    }

    // -- 29-06 Task 1: release_pr_title / release_pr_to_main ----------------

    #[test]
    fn release_pr_title_starts_with_the_documented_prefix() {
        assert!(
            release_pr_title("2.3.0").starts_with("release: v2.3.0"),
            "got: {}",
            release_pr_title("2.3.0")
        );
    }

    #[test]
    fn release_pr_to_main_refuses_before_creating_anything_when_resolution_fails() {
        // A directory `gh` cannot resolve any repository context from —
        // `discover_allowed_merge_methods` fails immediately, before any `gh
        // pr create`/`gh pr merge` call is reachable (same early-refusal
        // shape as `open_and_arm_pr_refuses_before_contacting_github_when_resolution_fails`).
        let dir = tempfile::tempdir().unwrap();
        let result = release_pr_to_main(dir.path(), "1.2.3");
        assert!(
            result.is_err(),
            "expected discovery/resolution failure to refuse before creating anything"
        );
    }

    /// The action resolves to squash whether `main`'s allowed set is squash
    /// only, or both merge and squash — the preference comes from the
    /// `ReleaseCut` intent, never from the fact that `main` happens to
    /// permit only one method today.
    #[test]
    fn release_pr_to_main_resolves_squash_against_squash_only_and_against_both_methods() {
        for allowed in [
            vec!["squash".to_string()],
            vec!["merge".to_string(), "squash".to_string()],
        ] {
            assert_eq!(
                release_policy::resolve_merge_method(MergeIntent::ReleaseCut, &allowed),
                Ok(MergeMethod::Squash),
                "expected ReleaseCut to resolve to Squash against allowed set {allowed:?}"
            );
        }
    }

    // -- 29-06 Task 2: sync_branch_name / merge_main_into_sync_branch_and_push

    #[test]
    fn sync_branch_name_matches_pr_refs_head_for_sync_merged_step() {
        let version = "2.3.0";
        let expected = sync_branch_name(version);
        assert_eq!(
            pr_refs(ReleaseStep::SyncMerged, version).unwrap().0,
            expected
        );
    }

    /// Build a repo with `main` and `develop` diverged on `origin`: both
    /// branch from a common commit, `develop` gets a commit unique to it,
    /// and `main` gets a *different* commit that happens to carry identical
    /// content for the same path — reproducing what a squash-merged release
    /// PR actually does (bring develop's own content over to `main`), so
    /// merging `origin/main` back into `develop` with `-X ours` is a pure
    /// history link: no conflict, no tree change.
    fn init_diverged_repo() -> (TempDir, TempDir) {
        let origin_dir = tempfile::tempdir().unwrap();
        git_at(origin_dir.path(), &["init", "-q", "--bare"]);

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_at(root, &["init", "-q"]);
        git_at(root, &["config", "user.email", "test@example.com"]);
        git_at(root, &["config", "user.name", "Test"]);
        git_at(root, &["config", "commit.gpgsign", "false"]);
        git_at(root, &["config", "core.hooksPath", "/dev/null"]);
        git_at(
            root,
            &[
                "remote",
                "add",
                "origin",
                origin_dir.path().to_str().expect("utf-8 tempdir path"),
            ],
        );

        std::fs::write(root.join("README.md"), "base\n").unwrap();
        git_at(root, &["add", "README.md"]);
        git_at(root, &["commit", "-q", "-m", "feat: base"]);
        git_at(root, &["branch", "-M", "main"]);
        git_at(root, &["push", "-q", "-u", "origin", "main"]);

        git_at(root, &["checkout", "-q", "-b", "develop"]);
        std::fs::write(root.join("shared.txt"), "brought over by the release PR\n").unwrap();
        git_at(root, &["add", "shared.txt"]);
        git_at(root, &["commit", "-q", "-m", "feat: develop change"]);
        git_at(root, &["push", "-q", "-u", "origin", "develop"]);

        git_at(root, &["checkout", "-q", "main"]);
        std::fs::write(root.join("shared.txt"), "brought over by the release PR\n").unwrap();
        git_at(root, &["add", "shared.txt"]);
        git_at(root, &["commit", "-q", "-m", "release: v1.2.3"]);
        git_at(root, &["push", "-q", "origin", "main"]);

        git_at(root, &["checkout", "-q", "develop"]);

        (dir, origin_dir)
    }

    /// As [`init_diverged_repo`], but `main`'s release commit also adds a
    /// file `develop` genuinely lacks — the fixture for the tree-identity
    /// refusal.
    fn init_diverged_repo_with_main_only_content() -> (TempDir, TempDir) {
        let (dir, origin_dir) = init_diverged_repo();
        let root = dir.path();
        git_at(root, &["checkout", "-q", "main"]);
        std::fs::write(root.join("main-only.txt"), "content develop lacks\n").unwrap();
        git_at(root, &["add", "main-only.txt"]);
        git_at(root, &["commit", "-q", "-m", "feat: content develop lacks"]);
        git_at(root, &["push", "-q", "origin", "main"]);
        git_at(root, &["checkout", "-q", "develop"]);
        (dir, origin_dir)
    }

    #[test]
    fn merge_main_into_sync_branch_and_push_short_circuits_when_already_an_ancestor() {
        let (repo, origin) = init_diverged_repo();
        let root = repo.path();
        // Sync origin/main into origin/develop out of band first, so the
        // precondition ("already an ancestor") is genuinely true on origin —
        // mirrors the script's own early exit.
        git_at(root, &["fetch", "origin", "main", "develop"]);
        git_at(
            root,
            &[
                "merge",
                "-X",
                "ours",
                "origin/main",
                "--no-edit",
                "-m",
                "pre-sync",
            ],
        );
        git_at(root, &["push", "-q", "origin", "develop"]);

        let outcome = merge_main_into_sync_branch_and_push(root, "1.2.3")
            .expect("merge_main_into_sync_branch_and_push");
        assert_eq!(outcome, SyncMergeOutcome::AlreadySynced);

        let listing = worktree::list(root).expect("list");
        assert_eq!(
            listing.len(),
            1,
            "expected no scratch worktree to be created on the short circuit, got: {listing:?}"
        );

        let branch = sync_branch_name("1.2.3");
        let refs = git_output_at(
            origin.path(),
            &["for-each-ref", &format!("refs/heads/{branch}")],
        );
        assert!(
            refs.is_empty(),
            "expected no sync branch to exist on origin after a short circuit, got: {refs}"
        );
    }

    #[test]
    fn merge_main_into_sync_branch_and_push_produces_a_two_parent_merge_commit_when_diverged() {
        let (repo, origin) = init_diverged_repo();
        let root = repo.path();

        let before_head = git_output_at(root, &["rev-parse", "HEAD"]);
        let before_branch = git_output_at(root, &["rev-parse", "--abbrev-ref", "HEAD"]);
        let before_status = git_output_at(root, &["status", "--porcelain"]);

        let outcome =
            merge_main_into_sync_branch_and_push(root, "1.2.3").expect("expected Ok(Pushed)");
        let branch = match outcome {
            SyncMergeOutcome::Pushed { branch } => branch,
            other => panic!("expected Pushed, got {other:?}"),
        };
        assert_eq!(branch, sync_branch_name("1.2.3"));

        let parents = git_output_at(origin.path(), &["log", "-1", "--pretty=%P", &branch]);
        assert_eq!(
            parents.split_whitespace().count(),
            2,
            "expected the pushed branch's tip to be a merge commit with two parents, got: {parents}"
        );

        // The operator's own checkout must be untouched.
        assert_eq!(before_head, git_output_at(root, &["rev-parse", "HEAD"]));
        assert_eq!(
            before_branch,
            git_output_at(root, &["rev-parse", "--abbrev-ref", "HEAD"])
        );
        assert_eq!(
            before_status,
            git_output_at(root, &["status", "--porcelain"])
        );

        let listing = worktree::list(root).expect("list");
        assert_eq!(
            listing.len(),
            1,
            "expected the scratch worktree to be removed after success, got: {listing:?}"
        );
    }

    #[test]
    fn merge_main_into_sync_branch_and_push_pushes_a_tree_identical_branch_when_diverged() {
        let (repo, origin) = init_diverged_repo();
        let root = repo.path();

        let develop_tree_before = git_output_at(root, &["rev-parse", "HEAD^{tree}"]);

        let outcome =
            merge_main_into_sync_branch_and_push(root, "1.2.3").expect("expected Ok(Pushed)");
        let branch = match outcome {
            SyncMergeOutcome::Pushed { branch } => branch,
            other => panic!("expected Pushed, got {other:?}"),
        };

        let pushed_tree =
            git_output_at(origin.path(), &["rev-parse", &format!("{branch}^{{tree}}")]);
        assert_eq!(
            develop_tree_before, pushed_tree,
            "expected the pushed branch's tree to be byte-identical to develop's pre-merge tree"
        );
    }

    #[test]
    fn merge_main_into_sync_branch_and_push_refuses_when_the_merge_changes_the_tree() {
        let (repo, origin) = init_diverged_repo_with_main_only_content();
        let root = repo.path();

        let result = merge_main_into_sync_branch_and_push(root, "1.2.3");
        let err = result.expect_err("expected the tree-identity check to refuse");
        assert!(
            err.contains("develop"),
            "expected the refusal to name develop's tree, got: {err}"
        );

        let branch = sync_branch_name("1.2.3");
        let refs = git_output_at(
            origin.path(),
            &["for-each-ref", &format!("refs/heads/{branch}")],
        );
        assert!(
            refs.is_empty(),
            "expected no branch to be pushed on refusal, got: {refs}"
        );

        let listing = worktree::list(root).expect("list");
        assert_eq!(
            listing.len(),
            1,
            "expected no scratch worktree to survive the refusal, got: {listing:?}"
        );
    }

    /// `resolve_merge_method(MergeIntent::SyncBack, &allowed)` still yields a
    /// real merge commit even when the allowed set contains both methods —
    /// the sync PR's method comes from intent, never from convenience.
    #[test]
    fn sync_intent_resolves_to_merge_even_when_squash_is_also_allowed() {
        let allowed = vec!["merge".to_string(), "squash".to_string()];
        assert_eq!(
            release_policy::resolve_merge_method(MergeIntent::SyncBack, &allowed),
            Ok(MergeMethod::Merge)
        );
    }
}
