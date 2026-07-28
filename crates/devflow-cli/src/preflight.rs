//! Agent preflight readiness gate (17c, D-13-D-16, shaped by 18-06/18f):
//! generic universal checks (interactivity, `gh auth`) plus an adapter-
//! specific hook, run from [`crate::pipeline_launch::launch_stage`] before
//! `monitor::spawn_monitor` so a readiness failure is caught before any
//! agent time is spent. Extracted mechanically (19-07, D-09 pure move) out
//! of `main.rs` — every function below is byte-identical to its pre-move
//! body modulo an added `pub(crate)` and adjusted `use` paths.
//!
//! **This module and `pipeline_launch`'s functions call each other
//! directly, and that is intentional (D-18f, 18-07, repointed 19-08):**
//! [`run_preflight`]'s `GateAction::Advance` arm calls
//! [`crate::pipeline_launch::launch_stage_inner`] directly so it skips the
//! just-adjudicated check on the retry, while
//! [`crate::pipeline_launch::launch_stage`] calls [`run_preflight`] on the
//! way in. Rust permits cyclic module references (only the crate
//! dependency graph must be acyclic), so this compiles cleanly; a reviewer
//! should expect to see this file's diff alongside `pipeline_launch.rs` for
//! any future change to either side of the pair.

use crate::CliError;
use crate::commands::phase_artifact_on_develop;
use crate::pipeline_gate::{abort, run_gate};
use crate::pipeline_launch::launch_stage;
use crate::pipeline_launch::launch_stage_inner;
use crate::pipeline_outcomes::truncate_reason;
use devflow_core::gates::{GateAction, Gates};
use devflow_core::mode::{self, Mode};
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::{agents, events, version, workflow};
use std::path::{Path, PathBuf};

/// The sandbox writable roots a worktree-hosted agent needs to commit: the
/// main repo's common `.git/` (objects, refs) and the linked worktree's
/// admin dir (`index.lock`, `HEAD`) — resolved from the worktree's `.git`
/// gitdir pointer when readable, with the creation-convention path as
/// fallback (13-06 dogfood finding).
pub(crate) fn worktree_writable_roots(project_root: &Path, worktree: &Path) -> Vec<PathBuf> {
    let git_dir = project_root.join(".git");
    let admin = std::fs::read_to_string(worktree.join(".git"))
        .ok()
        .and_then(|s| {
            s.trim()
                .strip_prefix("gitdir:")
                .map(|p| PathBuf::from(p.trim()))
        })
        .unwrap_or_else(|| {
            git_dir
                .join("worktrees")
                .join(worktree.file_name().unwrap_or_default())
        });
    vec![git_dir, admin]
}

/// Whether `program` resolves to an executable — a direct check for paths
/// containing a separator, a PATH scan otherwise. Restores the fail-fast
/// "is it installed?" diagnosis (14-CR-05) that the deleted synchronous
/// launch path used to get from `ErrorKind::NotFound`: the monitor's `sh`
/// exec of a missing binary only surfaces as a cryptic exit 127 after
/// worktrees and monitors were already set up.
fn agent_binary_available(program: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let executable = |path: &Path| {
        path.is_file()
            && std::fs::metadata(path)
                .map(|m| m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
    };
    if program.contains('/') {
        return executable(Path::new(program));
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| executable(&dir.join(program))))
        .unwrap_or(false)
}

/// The executable an agent kind launches, for preflighting before any
/// scaffolding. The prompt/roots passed here are throwaways — adapters
/// return a static program name regardless.
pub(crate) fn agent_program(agent: AgentKind) -> &'static str {
    agents::adapter_for(agent).exec_command(0, "", &[]).0
}

pub(crate) fn ensure_agent_binary(program: &str) -> Result<(), CliError> {
    if agent_binary_available(program) {
        return Ok(());
    }
    Err(CliError::Message(format!(
        "agent binary `{program}` not found — is it installed? (run `devflow doctor`)"
    )))
}

// ---------------------------------------------------------------------------
// 23f (gap closure, 23-12): phase-reachability guard. `devflow start` forks
// a worktree (or, in `--no-worktree` mode, a feature branch) from `develop`
// — if the target phase's ROADMAP.md heading or `.planning/phases/NN-*/`
// directory is not actually present on `develop`, the run is invisible to
// itself and floundered silently through Define before finally aborting
// (the 2026-07-26 acceptance-run failure, `23-FINDINGS.md` §B1). This
// probe catches that class of precondition failure BEFORE any git mutation.
// ---------------------------------------------------------------------------

/// The result of probing whether a phase is reachable from a base branch:
/// both the ROADMAP heading and the phase directory must be present for
/// `Reachable`; either being present-and-checkable but absent yields
/// `Unreachable`, whose two fields record whether each half was **found**
/// (at least one is always `false` in that variant). `Undeterminable` means
/// the probe could not see the base branch at all (no such branch, no
/// repository, or no `.planning/ROADMAP.md` on it) — the guard fails open
/// in that case rather than block a repository that never gave DevFlow a
/// basis to judge reachability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PhaseReachability {
    /// Both the ROADMAP heading and the phase directory are present on the
    /// base branch.
    Reachable,
    /// The probe could not determine reachability (missing base branch, not
    /// a repository, or the base branch carries no `.planning/ROADMAP.md`
    /// at all) — the guard must never refuse in this case.
    Undeterminable,
    /// At least one half is confirmed absent from the base branch.
    Unreachable {
        /// Whether the `### Phase {N}:` ROADMAP heading was found.
        roadmap_entry_found: bool,
        /// Whether a `.planning/phases/{NN}-*/` directory was found.
        phase_dir_found: bool,
    },
}

/// Probe whether `phase` is reachable from `base` in the repository at
/// `project_root`: four ordered git invocations, each a spawn error or
/// non-success status short-circuiting to `Undeterminable` per the
/// fail-open-where-blind contract this project's `phase_artifact_on_develop`
/// (`commands.rs`) already establishes.
///
/// NOTE: `base` is always `devflow_core::config::DEVELOP` at the one call
/// site today. If the base branch ever becomes configurable (999.30 WR-02
/// already flagged the sibling hardcoded `"main"`), this function's callers
/// must be re-pointed at that configuration alongside it.
pub(crate) fn phase_reachability_on_base(
    project_root: &Path,
    phase: u32,
    base: &str,
) -> PhaseReachability {
    // Step 1: does the base branch even exist here?
    let verify = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", base])
        .current_dir(project_root)
        .output();
    match verify {
        Ok(out) if out.status.success() => {}
        _ => return PhaseReachability::Undeterminable,
    }

    // Step 2: does the base branch carry a `.planning/ROADMAP.md` at all?
    // A repository with no roadmap has given DevFlow no basis to judge
    // reachability — fail open rather than treat "no roadmap" as "phase
    // missing" (this is what keeps `phase7_cli.rs`'s no-ROADMAP fixtures,
    // and any repository that simply doesn't keep a roadmap, unaffected).
    let roadmap = std::process::Command::new("git")
        .args(["show", &format!("{base}:.planning/ROADMAP.md")])
        .current_dir(project_root)
        .output();
    let roadmap_text = match roadmap {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
        _ => return PhaseReachability::Undeterminable,
    };

    // Step 3: scan for the phase heading. The trailing colon is
    // load-bearing — it stops a probe for phase 2 from matching
    // `### Phase 24:`; every heading in this project's ROADMAP already uses
    // this exact form.
    let heading = format!("### Phase {phase}:");
    let roadmap_entry_found = roadmap_text
        .lines()
        .any(|line| line.trim_start().starts_with(&heading));

    // Step 4: does the phase directory exist on the base branch? Same
    // `strip_prefix` + `rest.contains('/')` idiom as
    // `commands::phase_artifact_on_develop` — a directory holding only a
    // `.gitkeep` still counts as present, and phase numbers are
    // zero-padded (phase 7 is `07-`, phase 24 is `24-`).
    let ls_tree = std::process::Command::new("git")
        .args([
            "ls-tree",
            "-r",
            "--name-only",
            base,
            "--",
            ".planning/phases/",
        ])
        .current_dir(project_root)
        .output();
    let phase_dir_found = match ls_tree {
        Ok(out) if out.status.success() => {
            let prefix = format!(".planning/phases/{phase:02}-");
            String::from_utf8_lossy(&out.stdout).lines().any(|path| {
                path.strip_prefix(&prefix)
                    .is_some_and(|rest| rest.contains('/'))
            })
        }
        _ => return PhaseReachability::Undeterminable,
    };

    if roadmap_entry_found && phase_dir_found {
        PhaseReachability::Reachable
    } else {
        PhaseReachability::Unreachable {
            roadmap_entry_found,
            phase_dir_found,
        }
    }
}

/// Builds the operator-facing refusal message for an `Unreachable` result —
/// a pure function so the message is unit-testable without git. Names the
/// base branch and each missing half; contains no absolute filesystem path,
/// no `project_root`, and no username — every path named is
/// repository-relative. This repository has leaked absolute paths into
/// operator-facing strings three times (999.10, and again in 18-07's
/// `self_dogfood_stale_blocked` reason); this must not be a fourth.
pub(crate) fn unreachable_message(
    phase: u32,
    base: &str,
    roadmap_entry_found: bool,
    phase_dir_found: bool,
) -> String {
    let mut msg = format!(
        "phase {phase} is not reachable from `{base}` — the branch `devflow start` \
         forks its worktree from:\n"
    );
    if !roadmap_entry_found {
        msg.push_str(&format!(
            "  missing: the `### Phase {phase}:` heading in `ROADMAP.md` on `{base}`\n"
        ));
    }
    if !phase_dir_found {
        msg.push_str(&format!(
            "  missing: a `.planning/phases/{phase:02}-*/` directory on `{base}`\n"
        ));
    }
    msg.push_str(&format!(
        "a phase promoted only on another branch is invisible to this run — merge that \
         branch into `{base}` first, then re-run."
    ));
    msg
}

/// Refuse before `devflow start` forks anything when `phase` is not
/// reachable from `base`. `Reachable` and `Undeterminable` both return
/// `Ok(())` — the guard fails open where it cannot see.
pub(crate) fn ensure_phase_reachable_on_base(
    project_root: &Path,
    phase: u32,
    base: &str,
) -> Result<(), CliError> {
    match phase_reachability_on_base(project_root, phase, base) {
        PhaseReachability::Reachable | PhaseReachability::Undeterminable => Ok(()),
        PhaseReachability::Unreachable {
            roadmap_entry_found,
            phase_dir_found,
        } => Err(CliError::Message(unreachable_message(
            phase,
            base,
            roadmap_entry_found,
            phase_dir_found,
        ))),
    }
}

// ---------------------------------------------------------------------------
// 25e (999.51/D-18a): base-ref currency probe, siblings of the phase-
// reachability probe above. `devflow start` forks its worktree from `base`
// (DEVELOP) — if the local ref is behind its remote-tracking counterpart,
// the fork happens against stale code while the reachability probe above
// still passes, because the phase heading and directory exist on the STALE
// local ref just as they do on the fresh remote one. This is 999.51's
// explicitly-named "dangerous variant": a green run against the wrong
// source, arriving through the base ref instead of the binary — the same
// false-evidence shape D-18 exists to prevent.
// ---------------------------------------------------------------------------

/// Remote name this probe fetches and compares against — hardcoded, matching
/// `devflow_core::git::origin_main_ancestor_status`'s existing convention
/// (this project has no remote-name configuration knob).
const ORIGIN: &str = "origin";

/// The result of comparing a local base branch to its remote-tracking ref.
/// Mirrors [`PhaseReachability`]'s shape: `Undeterminable` carries the same
/// fail-open-where-blind meaning this module already documents for that
/// sibling probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BaseRefCurrency {
    /// The local base ref equals its remote-tracking ref.
    Current,
    /// The local base ref is strictly AHEAD of its remote-tracking ref
    /// (unpushed local work) — never treated as staleness.
    Ahead,
    /// The local base ref is strictly BEHIND its remote-tracking ref by
    /// `count` commits, with no local divergence — the remote is a
    /// lossless fast-forward target.
    Behind { count: u32 },
    /// Local and remote have both moved past their common ancestor — mutual
    /// non-ancestors. A divergent base is never silently forked from.
    Diverged,
    /// The remote-tracking ref could not be resolved at all (no remote
    /// configured, offline clone, or the fetch failed and the ref never
    /// existed locally either) — the guard fails open in this case, per the
    /// module's existing fail-open-where-blind contract.
    Undeterminable,
}

/// Probe whether `base` is current with its remote-tracking ref in the
/// repository at `project_root`. Follows `origin_main_ancestor_status`'s
/// shape verbatim (`git rev-parse --verify --quiet` for ref existence, then
/// `git merge-base --is-ancestor` — here in both directions, to distinguish
/// behind/ahead/equal/diverged), but is preceded by a soft-failing
/// `git fetch` so the comparison is made against a freshly-updated
/// remote-tracking ref rather than merely "nobody has fetched recently."
///
/// The fetch updates ONLY the remote-tracking ref (`git fetch <remote>
/// <base>`) — it never touches the local branch or the working tree, so it
/// cannot fail with a "branch is checked out" error the way a refspec fetch
/// into the local branch would. On spawn error or non-zero exit the fetch
/// fails SOFT: a warning is printed and the comparison proceeds against
/// whatever the remote-tracking ref currently holds (Phase 24's inherited
/// D-06 precedent — a new code path must not introduce a false hard-block
/// on correct work, and an offline machine is correct work).
pub(crate) fn base_ref_currency(project_root: &Path, base: &str) -> BaseRefCurrency {
    let remote_ref = format!("{ORIGIN}/{base}");

    let fetch_ok = std::process::Command::new("git")
        .args(["fetch", "--quiet", ORIGIN, base])
        .current_dir(project_root)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if !fetch_ok {
        println!(
            "warning: could not fetch `{ORIGIN} {base}` — comparing `{base}` against \
             whatever `{remote_ref}` currently resolves to locally, which may be stale"
        );
    }

    let ref_exists = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", &remote_ref])
        .current_dir(project_root)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if !ref_exists {
        return BaseRefCurrency::Undeterminable;
    }

    let is_ancestor = |ancestor: &str, descendant: &str| {
        std::process::Command::new("git")
            .args(["merge-base", "--is-ancestor", ancestor, descendant])
            .current_dir(project_root)
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    };
    let local_is_ancestor_of_remote = is_ancestor(base, &remote_ref);
    let remote_is_ancestor_of_local = is_ancestor(&remote_ref, base);

    match (local_is_ancestor_of_remote, remote_is_ancestor_of_local) {
        (true, true) => BaseRefCurrency::Current,
        (false, true) => BaseRefCurrency::Ahead,
        (false, false) => BaseRefCurrency::Diverged,
        (true, false) => {
            let count = std::process::Command::new("git")
                .args(["rev-list", "--count", &format!("{base}..{remote_ref}")])
                .current_dir(project_root)
                .output()
                .ok()
                .filter(|out| out.status.success())
                .and_then(|out| {
                    String::from_utf8_lossy(&out.stdout)
                        .trim()
                        .parse::<u32>()
                        .ok()
                })
                .unwrap_or(0);
            BaseRefCurrency::Behind { count }
        }
    }
}

/// Builds the operator-facing refusal message for a `Behind` result that
/// could not be safely fast-forwarded — modelled on `unreachable_message`:
/// self-contained, actionable, naming the exact repair command, and
/// containing **no absolute filesystem path** (WR-02 — on Linux a path
/// embeds the operator's username, and this string reaches operator
/// output). Names refs and commands only.
pub(crate) fn stale_base_message(base: &str, remote_ref: &str, count: u32) -> String {
    format!(
        "`{base}` is {count} commit(s) behind `{remote_ref}` and could not be safely \
         fast-forwarded (it is currently checked out, or the fast-forward itself failed) \
         — `devflow start` refuses to fork a phase worktree from a stale base. Switch off \
         `{base}` if it is currently checked out, then run `git fetch {ORIGIN} {base} && \
         git checkout {base} && git merge --ff-only {remote_ref}`, then re-run `devflow start`."
    )
}

/// Whether `base` is checked out in ANY worktree of the repository at
/// `project_root` — not just `project_root`'s own `HEAD`. `git update-ref`
/// has no checked-out-branch protection of its own (unlike `git branch -f`,
/// which refuses on its own for the checked-out branch of any worktree), so
/// this predicate is the only thing standing between a fast-forward write
/// and a live worktree whose HEAD, index and working tree are pinned to
/// `base`.
///
/// Parses `git worktree list --porcelain`, whose output is one stanza per
/// worktree; an attached worktree's stanza carries a line reading exactly
/// `branch refs/heads/<name>` (a detached worktree emits `detached`, a bare
/// main repository emits `bare`). A worktree that is registered but
/// `prunable` (its directory has been deleted but not yet pruned) still
/// counts as checked out here, matching `git branch -f`'s own behaviour —
/// deliberately conservative.
///
/// Returns `true` (refuse-safe) on a spawn error or a non-zero exit — the
/// OPPOSITE polarity to this module's fail-open-where-blind contract for
/// `Undeterminable` elsewhere, because the consequence of a wrong answer
/// here is a destructive ref write, not a refusal to start.
pub(crate) fn base_is_checked_out_anywhere(project_root: &Path, base: &str) -> bool {
    let out = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(project_root)
        .output();
    match out {
        Ok(out) if out.status.success() => {
            let needle = format!("branch refs/heads/{base}");
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .any(|line| line.trim() == needle)
        }
        _ => true,
    }
}

/// Compare-and-swap fast-forward of `refs/heads/<base>` to `new`, conditional
/// on the ref currently equalling `expected_old` — git's `<oldvalue>`
/// parameter to `update-ref`. Supplying it is what makes this write refuse
/// when the ref has moved since the caller last observed it; WITHOUT it,
/// `git update-ref` is an unconditional write that will move a ref
/// backwards onto a non-descendant if the ref changed between the caller's
/// read and this write.
///
/// Returns `false` on a spawn error or a non-zero exit (including a
/// mismatched `expected_old`), never panics.
pub(crate) fn fast_forward_base_ref(
    project_root: &Path,
    base: &str,
    expected_old: &str,
    new: &str,
) -> bool {
    std::process::Command::new("git")
        .args([
            "update-ref",
            &format!("refs/heads/{base}"),
            new,
            expected_old,
        ])
        .current_dir(project_root)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Refuse before `devflow start` forks anything when `base` is stale
/// relative to its remote. `Current`, `Ahead` and `Undeterminable` all
/// return `Ok(())` (the last with a warning — the guard fails open where it
/// cannot see, mirroring [`ensure_phase_reachable_on_base`]).
///
/// The `Behind` arm implements the operator's 2026-07-27 adjudication
/// (999.51/D-18a): fast-forward when it is safe to do so, else refuse
/// loudly. `Behind` establishes losslessness at the instant it was read — it
/// says nothing about the instant of the write, since another writer, a
/// hook, or a concurrent `devflow` may move `base` in between. The
/// fast-forward is therefore a compare-and-swap:
/// [`fast_forward_base_ref`] is called with the local base SHA resolved
/// immediately before the write as `expected_old`, so a base that moved in
/// that window causes the write to refuse rather than silently discard the
/// intervening commit(s).
///
/// The checked-out precondition is evaluated across EVERY worktree of the
/// repository via [`base_is_checked_out_anywhere`], not just
/// `project_root`'s own `HEAD` — necessary because `git update-ref` carries
/// no checked-out-branch protection of its own, unlike `git branch -f`.
///
/// RESIDUAL, documented rather than eliminated: a worktree that checks out
/// `base` in the window between the repository-wide scan and the
/// compare-and-swap is not protected by the scan; the compare-and-swap still
/// prevents a lost update in that window, but it cannot prevent that
/// worktree from observing a moved HEAD.
///
/// On ANY failure (checked out anywhere, either endpoint unresolvable, the
/// compare-and-swap refusing, spawn error, non-zero exit) this falls
/// through to the same refusal — never proceed silently after a failed
/// fast-forward.
///
/// `Diverged` always refuses — a divergent base is never silently forked
/// from.
pub(crate) fn ensure_base_ref_current(project_root: &Path, base: &str) -> Result<(), CliError> {
    match base_ref_currency(project_root, base) {
        BaseRefCurrency::Current | BaseRefCurrency::Ahead => Ok(()),
        BaseRefCurrency::Undeterminable => {
            println!(
                "warning: could not determine whether `{base}` is current with `{ORIGIN}/{base}` \
                 — proceeding without a currency check (fail-open, per this module's \
                 fail-open-where-blind contract)"
            );
            Ok(())
        }
        BaseRefCurrency::Diverged => {
            let remote_ref = format!("{ORIGIN}/{base}");
            Err(CliError::Message(format!(
                "`{base}` and `{remote_ref}` have diverged — neither is an ancestor of the \
                 other, so `devflow start` refuses to fork a phase worktree from either. \
                 Resolve manually (e.g. `git checkout {base} && git rebase {remote_ref}`, or \
                 `git reset --hard {remote_ref}` if `{base}`'s local commits are disposable), \
                 then re-run `devflow start`."
            )))
        }
        BaseRefCurrency::Behind { count } => {
            let remote_ref = format!("{ORIGIN}/{base}");

            if !base_is_checked_out_anywhere(project_root, base) {
                let resolve = |rref: &str| {
                    std::process::Command::new("git")
                        .args(["rev-parse", "--verify", "--quiet", rref])
                        .current_dir(project_root)
                        .output()
                        .ok()
                        .filter(|out| out.status.success())
                        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
                };
                let local_sha = resolve(&format!("refs/heads/{base}"));
                let remote_sha = resolve(&format!("refs/remotes/{remote_ref}"));

                if let (Some(local_sha), Some(remote_sha)) = (local_sha, remote_sha) {
                    let fast_forwarded =
                        fast_forward_base_ref(project_root, base, &local_sha, &remote_sha);
                    if fast_forwarded {
                        println!(
                            "advanced `{base}` to `{remote_ref}` ({count} commit(s) fast-forwarded)"
                        );
                        return Ok(());
                    }
                }
            }
            Err(CliError::Message(stale_base_message(
                base,
                &remote_ref,
                count,
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// 17c: preflight readiness gate (D-13-D-16) — generic universal checks +
// adapter hook, run from `launch_stage` before `monitor::spawn_monitor` so a
// readiness failure is caught before any agent time is spent.
// ---------------------------------------------------------------------------

/// D-14 (universal, generic layer): a headless/auto Codex run cannot pass
/// Define's discuss-phase interview — Codex's `exec` mode has no route to
/// answer an interactive interview (`request_user_input is unavailable in
/// Default mode`), unlike Claude/OpenCode's headless Define, which can and
/// does complete it non-interactively (verified live, 13-06; the existing
/// integration tests exercise exactly this: `--agent claude --mode auto`
/// with no pre-existing CONTEXT.md succeeds). This check reuses the same
/// `phase_artifact_on_develop` predicate as the existing pre-state Codex
/// check in `start()`, but routes the failure through the preflight gate
/// (D-15) instead of a hard error — closing the gap that check leaves open
/// for non-`start()` launch paths (`resume`, gate retries, loop-backs). The
/// pre-state Codex check itself is intentionally left unmigrated (Review
/// dispositions, out of scope for this plan).
fn preflight_interactivity_check(project_root: &Path, state: &State) -> Result<(), String> {
    if state.agent == AgentKind::Codex
        && state.mode == Mode::Auto
        && state.stage == Stage::Define
        && !phase_artifact_on_develop(project_root, state.phase, "-CONTEXT.md")
    {
        return Err(format!(
            "phase {} has no CONTEXT.md on develop — codex cannot run Define's \
             discuss-phase interview headlessly in auto mode",
            state.phase
        ));
    }
    Ok(())
}

/// D-14 (universal, generic layer): whether the gh-auth credential probe
/// applies to `stage` — hardcoded to `Stage::Ship` rather than a dynamic
/// hook-scan (review Plan 05 MEDIUM, Codex+OpenCode): Ship's terminal hooks
/// (`hooks::hooks_after_ship()` = Merge/VersionBump/ChangelogAppend/BranchCleanup,
/// `hooks.rs:99-106`) are the only hooks that push to a remote. Split out as
/// its own pure predicate so "does not run for a non-Ship stage" is directly
/// unit-testable without shelling out to `gh`.
fn gh_auth_check_applies(stage: Stage) -> bool {
    stage == Stage::Ship
}

/// D-14 (universal, generic layer): external credential validity via `gh
/// auth status`, run ONLY when [`gh_auth_check_applies`] (Ship). Fails soft
/// to a warning when the `gh` binary itself is absent — a missing optional
/// tool must not hard-fail the pipeline (T-17-14). Fails preflight only when
/// `gh` is present and reports unauthenticated. Records only a boolean
/// pass/fail plus a short reason string — raw `gh auth status` stdout/stderr
/// is NEVER captured or logged (T-17-13, Information Disclosure).
fn preflight_gh_auth_check(state: &State) -> Result<(), String> {
    if !gh_auth_check_applies(state.stage) {
        return Ok(());
    }
    match std::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
    {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => Err("gh auth status reports not authenticated".to_string()),
        Err(_) => {
            println!(
                "warning: `gh` binary not found — cannot verify GitHub credential validity \
                 before Ship (fail-soft, not a preflight failure)"
            );
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// D-09 (999.49 / DEN-74, second half): a major version bump must never ship
// unattended. `hooks_after_ship` runs Merge -> VersionBump -> ChangelogAppend
// -> BranchCleanup as a fail-fast batch with **no rollback** once Merge has
// committed (`merge_feature`'s own doc comment, `devflow-core/src/hooks.rs`)
// — a gate opening inside `VersionBump` itself would open AFTER the merge to
// `develop` had already landed. **Placement here, at preflight, is
// load-bearing, not cosmetic:** this is what makes the gate meaningful — it
// is evaluated strictly before `hooks_after_ship` runs at all.
//
// D-18f's `Advance` semantics apply exactly as they do to the sibling
// generic checks above: a human `Advance` on this gate relaunches via
// `launch_stage_inner` directly and skips the re-check, which is correct
// here because the classification is a deterministic idempotent predicate —
// a gate approval cannot change what the commit range classifies to, so
// re-running it would fail identically.
//
// **This check cannot be auto-approved.** `run_gate_with_timeout` (see its
// doc comment in `pipeline_gate.rs`) deliberately does not derive
// auto-approval from `state.yes_ship`, and forbids a future refactor from
// folding it in. This check inherits that property rather than
// re-implementing it — it performs no `yes_ship` handling of its own.
//
// **25-RESEARCH.md Open Question 1 is resolved here, not carried forward as
// an open risk:** the Ship-stage agent's own commits (the code-review
// report, and the ship-workflow's state-file commit) are both `docs`-typed
// under D-08's classifier and so contribute no bump — no breaking-change
// commit can land between this gate and `hooks_after_ship` running. No
// second classification immediately before `Merge` is added.
// ---------------------------------------------------------------------------

/// D-09: whether the major-bump preflight check applies to `stage` —
/// hardcoded to `Stage::Ship`, exactly the way [`gh_auth_check_applies`] is
/// hardcoded, and split out as its own pure predicate for the same reason:
/// so "does not run for a non-Ship stage" is directly unit-testable without
/// any git fixture.
fn major_bump_check_applies(stage: Stage) -> bool {
    stage == Stage::Ship
}

/// D-09: a major bump opens a `[never-silent]` gate before `hooks_after_ship`
/// runs, run ONLY when [`major_bump_check_applies`] (Ship). Classifies the
/// SAME range [`version::compute_version`] will classify, by calling the
/// same helpers it calls — [`version::highest_semver_tag`],
/// [`version::reachable_semver_baseline`], [`version::release_range_start`],
/// [`version::classify_range_bump`] — so this check and `VersionBump`'s
/// later evaluation can never disagree.
///
/// A derivation error, including D-10's unreachable-baseline refusal, is
/// surfaced as a preflight failure rather than silently treated as "no major
/// bump": a version derivation that cannot be trusted must not proceed into
/// the no-rollback batch (T-25-54).
///
/// The `Err` string names the classified bump kind, the baseline tag, and
/// the resulting version, plus (for the major case) the deciding commits'
/// subjects so a human can adjudicate — passed through [`truncate_reason`]
/// (T-25-52), since commit subjects are attacker-influenced text and this
/// reason reaches a persisted gate file and operator output. It contains no
/// absolute filesystem path (WR-02).
///
/// CR-02 (`25-REVIEW.md`, 25-08): classification runs against
/// `execution_root = state.worktree_path.as_deref().unwrap_or(project_root)`
/// — the phase's worktree when `state.worktree_path` is set, else
/// `project_root` — exactly the idiom `staleness.rs::enforce_build_staleness`
/// established for the identical distinction. In `devflow start`'s default
/// (worktree) execution mode, the phase's own commits live only on the
/// worktree's feature branch and are unreachable from `project_root`'s HEAD
/// until `hooks_after_ship`'s Merge step, which runs AFTER this preflight —
/// evaluating against `project_root` alone would make a `feat!:` commit
/// invisible to this gate.
fn preflight_major_bump_check(project_root: &Path, state: &State) -> Result<(), String> {
    if !major_bump_check_applies(state.stage) {
        return Ok(());
    }
    let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);

    let highest = version::highest_semver_tag(execution_root).map_err(|err| err.to_string())?;
    let baseline =
        version::reachable_semver_baseline(execution_root).map_err(|err| err.to_string())?;

    // D-10: refuse rather than proceed when the true highest tag exists but
    // is not reachable from HEAD — mirrors `compute_version`'s own refusal
    // exactly, since this check must never disagree with it.
    if let Some(highest) = &highest {
        let unreachable = match &baseline {
            Some(reachable) => highest > reachable,
            None => true,
        };
        if unreachable {
            return Err(truncate_reason(&format!(
                "version derivation refused: highest semver tag `v{highest}` is not reachable \
                 from HEAD (D-10) — a major-bump classification cannot be trusted here, so \
                 preflight refuses rather than proceed toward a no-rollback ship batch"
            )));
        }
    }

    let baseline_tag = baseline.as_ref().map(|tag| format!("v{tag}"));
    let range_start = match &baseline_tag {
        Some(tag) => {
            version::release_range_start(execution_root, tag).map_err(|err| err.to_string())?
        }
        None => String::new(),
    };
    let bump = version::classify_range_bump(execution_root, &range_start)
        .map_err(|err| err.to_string())?;

    if bump != version::Bump::Major {
        return Ok(());
    }

    let baseline_display = baseline_tag.as_deref().unwrap_or("(none)").to_string();
    let baseline_major = baseline.as_ref().map(|v| v.major).unwrap_or(0);
    let resulting_major = baseline_major + 1;
    let subjects = breaking_commit_subjects(execution_root, &range_start);
    let subjects_display = if subjects.is_empty() {
        String::new()
    } else {
        format!(" — deciding commit(s): {}", subjects.join("; "))
    };

    Err(truncate_reason(&format!(
        "classified bump is MAJOR — baseline `{baseline_display}`, resulting version \
         `v{resulting_major}.0.0`; a major version bump never ships unattended (D-09){subjects_display}"
    )))
}

/// D-09 diagnostic aid: the deciding commit subjects for a major-bump gate's
/// `Err` message — re-scans the same range [`version::classify_range_bump`]
/// classified, using the identical `%H%x1f%B%x1e` git-log idiom, so a human
/// reviewing the gate can see which commit(s) carry a breaking marker
/// without re-deriving the range themselves. Best-effort: a git spawn
/// failure or non-zero exit yields an empty list rather than propagating an
/// error — this is a diagnostic aid for the message, not part of the
/// classification itself.
///
/// CR-02 (25-08): `execution_root` (the phase's worktree when set, else
/// `project_root`) — see [`preflight_major_bump_check`]'s doc comment — so
/// this diagnostic scan agrees with the classification it explains.
fn breaking_commit_subjects(execution_root: &Path, range_start: &str) -> Vec<String> {
    let range = if range_start.is_empty() {
        "HEAD".to_string()
    } else {
        format!("{range_start}..HEAD")
    };
    let Ok(output) = std::process::Command::new("git")
        .args(["log", "--no-merges", &range, "--format=%H%x1f%B%x1e"])
        .current_dir(execution_root)
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut subjects = Vec::new();
    for record in stdout.split('\u{1e}') {
        let record = record.trim_matches('\n');
        if record.is_empty() {
            continue;
        }
        let Some((_hash, message)) = record.split_once('\u{1f}') else {
            continue;
        };
        let message = message.trim();
        let subject = message.lines().next().unwrap_or_default();
        let is_breaking = subject
            .split_once(':')
            .is_some_and(|(prefix, _)| prefix.contains('!'))
            || message.contains("BREAKING CHANGE:")
            || message.contains("BREAKING-CHANGE:");
        if is_breaking {
            subjects.push(subject.to_string());
        }
    }
    subjects
}

/// The generic (universal) preflight checks (D-14) — the adapter-specific
/// hook is composed separately in [`run_preflight`].
///
/// CR-01 (`25-REVIEW.md`, 25-08): runs all three checks unconditionally and
/// aggregates every `Err` into one reason, rather than `?`-short-circuiting
/// on the first failure. `run_preflight`'s `GateAction::Advance` arm
/// relaunches via `launch_stage_inner` directly and never re-runs this
/// function — so under the old `?`-chain, a check that never ran once
/// (because an earlier check in the chain failed first) would never run at
/// all for that stage launch, and a human approving that earlier gate would
/// never have been shown the unrun check's reason. Aggregation closes that
/// hole for every check in this chain, not just the major-bump check that
/// surfaced it (`25-VERIFICATION.md`'s named fix, `25-REVIEW.md`'s option
/// (a); option (b), special-casing the `Advance` arm instead, was
/// deliberately not taken — it would not close the same hole for a future
/// check added to this chain).
///
/// Reasons are ordered by consequence, **major-bump FIRST**, then
/// interactivity, then gh-auth — load-bearing, not cosmetic:
/// `run_preflight` passes the joined string through [`truncate_reason`] (a
/// hard 300-character cap) before it reaches the gate context, and the
/// major-bump reason is both the longest of the three and the only one
/// whose loss would silently re-open the unattended-ship hole D-09 exists
/// to close.
///
/// The adapter-specific hook (composed by [`run_preflight`] via
/// `.and_then`) is deliberately NOT folded into this aggregation —
/// `25-VERIFICATION.md`'s gap-closure scope names only these three generic
/// checks; this is a scope boundary, not an oversight.
fn generic_preflight_checks(project_root: &Path, state: &State) -> Result<(), String> {
    let mut reasons = Vec::new();
    if let Err(reason) = preflight_major_bump_check(project_root, state) {
        reasons.push(reason);
    }
    if let Err(reason) = preflight_interactivity_check(project_root, state) {
        reasons.push(reason);
    }
    if let Err(reason) = preflight_gh_auth_check(state) {
        reasons.push(reason);
    }
    if reasons.is_empty() {
        Ok(())
    } else {
        Err(reasons.join("; "))
    }
}

/// Gate a stage launch on readiness (17c, D-13-D-16): the generic universal
/// checks (D-14) plus the adapter-specific hook, called from `launch_stage`
/// BEFORE `monitor::spawn_monitor` so a readiness failure is caught before
/// any agent time is spent. A failing check is NEVER a hard exit — it
/// surfaces as a named preflight gate + notify (WR-11 idiom, D-15), mirroring
/// `handle_stage_failure`'s dispatch shape exactly.
///
/// Returns `Ok(true)` when the caller should continue the rest of
/// `launch_stage` (preflight passed). Returns `Ok(false)` when a failing
/// check was resolved via a gate that ALREADY completed a full retried
/// launch (Advance/LoopBack), reached the retry ceiling, or aborted —
/// the caller must not run any more launch steps for this invocation
/// (CR-01, 17-08 gap closure: the old `Result<(), CliError>` return
/// couldn't distinguish these cases, so the caller always continued and
/// spawned the agent a second time).
///
/// 18f (D-18f): `GateAction::Advance` on a preflight gate is an explicit
/// override — the check has already been adjudicated by a human, and every
/// generic check (`preflight_major_bump_check`,
/// `preflight_interactivity_check`, `preflight_gh_auth_check`) is a
/// deterministic idempotent predicate a gate approval cannot change, so
/// re-running them is guaranteed to fail identically. **This justification
/// is sound only because [`generic_preflight_checks`] aggregates rather than
/// `?`-short-circuits (CR-01, 25-08): every applicable check has actually
/// been evaluated and its reason shown to the human before the gate opens,
/// so skipping the re-check on `Advance` cannot silently skip a check that
/// never ran once.** The `Advance` arm therefore relaunches via
/// [`launch_stage_inner`] directly, SKIPPING this function entirely on the
/// retry. `GateAction::LoopBack` still calls the full [`launch_stage`]
/// (re-entering this function), because that path means the operator will
/// fix the condition and retry, and the state may genuinely have changed.
/// Either arm's recursion is bounded by `state.preflight_retries` /
/// [`mode::MAX_PREFLIGHT_RETRIES`]: the ceiling is checked BEFORE writing
/// another gate, so reaching it aborts with a logged
/// `preflight_retry_ceiling_reached` event instead of polling a second
/// 7-day gate timeout nobody will ever answer (T-18-27, T-18-30).
pub(crate) fn run_preflight(
    project_root: &Path,
    state: &mut State,
    adapter: &dyn agents::AgentAdapter,
) -> Result<bool, CliError> {
    let stage = state.stage;
    if let Err(reason) =
        generic_preflight_checks(project_root, state).and_then(|()| adapter.preflight(state))
    {
        // Check the ceiling BEFORE writing another gate — writing the gate
        // first would let the ceiling case open yet another gate nobody
        // will answer (T-18-27).
        if state.preflight_retries >= mode::MAX_PREFLIGHT_RETRIES {
            let ceiling_reason = format!(
                "preflight retry ceiling ({}) reached for stage {stage}: {}",
                mode::MAX_PREFLIGHT_RETRIES,
                truncate_reason(&reason)
            );
            events::emit(
                project_root,
                state.phase,
                "preflight_retry_ceiling_reached",
                serde_json::json!({
                    "stage": stage.to_string(),
                    "reason": truncate_reason(&reason),
                    "ceiling": mode::MAX_PREFLIGHT_RETRIES,
                }),
            );
            abort(project_root, state, &ceiling_reason)?;
            return Ok(false);
        }
        state.preflight_retries = state.preflight_retries.saturating_add(1);
        workflow::save_state(state)?;

        let context = format!(
            "[never-silent] preflight failed for stage {stage}: {} — human review needed \
             (retry, loop-to-code, or abort)",
            truncate_reason(&reason)
        );
        match run_gate(project_root, state, stage, &context)? {
            GateAction::Advance => {
                // D-18f: approval is an explicit override — skip the
                // just-adjudicated check on the retry (see the function
                // doc comment above).
                let _ = Gates::cleanup(project_root, state.phase, stage);
                state.gate_pending = false;
                state.preflight_retries = 0;
                workflow::save_state(state)?;
                launch_stage_inner(state, None, None)?;
            }
            GateAction::LoopBack(_) => {
                // D-18f: "I will fix it, then retry" — re-check deliberately,
                // bounded by the ceiling above.
                let _ = Gates::cleanup(project_root, state.phase, stage);
                launch_stage(state, None, None)?;
            }
            GateAction::Abort(reason) => abort(project_root, state, &reason)?,
        }
        return Ok(false);
    }

    // Preflight passed: reset the retry counter, persisted (the wedge this
    // counter bounds spans separate `devflow` invocations, so an in-memory
    // reset alone would not survive a monitor restart). Guarded so a
    // passing preflight on an already-zero counter does not rewrite state
    // on every single launch.
    if state.preflight_retries != 0 {
        state.preflight_retries = 0;
        workflow::save_state(state)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    /// 14-CR-05: a missing agent binary must fail fast with the actionable
    /// "is it installed?" message, not a post-worktree exit-127 mystery.
    #[test]
    fn ensure_agent_binary_diagnoses_missing_program() {
        // `sh` is guaranteed present on any host that can run devflow.
        assert!(ensure_agent_binary("sh").is_ok());
        assert!(ensure_agent_binary("/bin/sh").is_ok());

        let err = ensure_agent_binary("definitely-not-a-real-agent-xyz").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not found — is it installed?"), "{msg}");
        assert!(msg.contains("devflow doctor"), "{msg}");
        assert!(ensure_agent_binary("/nonexistent/path/agent").is_err());
    }

    // -----------------------------------------------------------------
    // 17c: preflight readiness gate (D-13-D-16, Task 1)
    // -----------------------------------------------------------------

    /// D-14 interactivity check: a headless Auto-mode Codex Define run with
    /// no CONTEXT.md on develop is flagged; Supervise mode, a non-Define
    /// stage, a non-Codex agent (Claude/OpenCode can complete Define
    /// headlessly, verified live 13-06 — the existing `start_defaults_to_
    /// worktree` integration test exercises exactly this), and a CONTEXT.md
    /// that does exist are all unaffected.
    #[test]
    fn preflight_interactivity_check_flags_auto_define_without_context_md() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let mut state = State::new(60, AgentKind::Codex, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Define;
        assert!(preflight_interactivity_check(root, &state).is_err());

        state.mode = Mode::Supervise;
        assert!(preflight_interactivity_check(root, &state).is_ok());

        state.mode = Mode::Auto;
        state.stage = Stage::Plan;
        assert!(preflight_interactivity_check(root, &state).is_ok());

        state.stage = Stage::Define;
        state.agent = AgentKind::Claude;
        assert!(
            preflight_interactivity_check(root, &state).is_ok(),
            "Claude/OpenCode can complete Define headlessly — only Codex is flagged"
        );
        state.agent = AgentKind::Codex;

        let git = |args: &[&str]| {
            assert!(
                devflow_core::test_support::git_command(root)
                    .args(args)
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "git {args:?} failed"
            );
        };
        std::fs::create_dir_all(root.join(".planning/phases/60-widget")).unwrap();
        std::fs::write(root.join(".planning/phases/60-widget/60-CONTEXT.md"), "ctx").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "context"]);

        state.stage = Stage::Define;
        assert!(preflight_interactivity_check(root, &state).is_ok());
    }

    /// D-14 gh-auth scope: hardcoded to Stage::Ship, not a dynamic hook-scan.
    #[test]
    fn gh_auth_check_applies_only_to_ship_stage() {
        assert!(gh_auth_check_applies(Stage::Ship));
        for stage in [Stage::Define, Stage::Plan, Stage::Code, Stage::Validate] {
            assert!(!gh_auth_check_applies(stage));
        }
    }

    // -----------------------------------------------------------------
    // D-09 (999.49/DEN-74): major-bump preflight check.
    // -----------------------------------------------------------------

    /// D-09 scope: hardcoded to Stage::Ship, mirroring `gh_auth_check_applies`.
    #[test]
    fn major_bump_check_applies_only_to_ship_stage() {
        assert!(major_bump_check_applies(Stage::Ship));
        for stage in [Stage::Define, Stage::Plan, Stage::Code, Stage::Validate] {
            assert!(!major_bump_check_applies(stage));
        }
    }

    fn commit_msg(root: &Path, name: &str, message: &str) {
        std::fs::write(root.join(name), name).unwrap();
        run_git(root, &["add", "."]);
        run_git(root, &["commit", "-q", "-m", message]);
    }

    fn tag(root: &Path, name: &str) {
        run_git(root, &["tag", name]);
    }

    /// A minimal repo with `v1.0.0` tagged at the initial commit — the
    /// baseline every major-bump fixture below builds on.
    fn major_bump_fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        run_git(root, &["init", "-q"]);
        run_git(root, &["config", "user.email", "t@e.st"]);
        run_git(root, &["config", "user.name", "t"]);
        run_git(root, &["config", "commit.gpgsign", "false"]);
        run_git(root, &["config", "tag.gpgsign", "false"]);
        run_git(root, &["config", "core.hooksPath", "/dev/null"]);
        commit_msg(root, "a.txt", "chore: init");
        tag(root, "v1.0.0");
        dir
    }

    /// A non-Ship stage returns `Ok(())` without ever shelling out to git —
    /// the fixture directory is not even a git repository, so a git spawn
    /// or exit failure would surface as `Err` were the check to ever reach
    /// the git-shelling branch. `Ok(())` here is evidence of the early
    /// return in `major_bump_check_applies`.
    #[test]
    fn major_bump_short_circuits_for_non_ship_stage() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = State::new(70, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        for stage in [Stage::Define, Stage::Plan, Stage::Code, Stage::Validate] {
            state.stage = stage;
            assert!(
                preflight_major_bump_check(root, &state).is_ok(),
                "stage {stage} must short-circuit before ever shelling out to git"
            );
        }
    }

    /// A range classifying to patch or minor passes silently at Ship.
    #[test]
    fn major_bump_ok_for_patch_or_minor_bump_at_ship() {
        let dir = major_bump_fixture();
        let root = dir.path();
        commit_msg(root, "b.txt", "fix(x): correct off-by-one");

        let mut state = State::new(71, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        assert!(preflight_major_bump_check(root, &state).is_ok());

        commit_msg(root, "c.txt", "feat(x): add capability");
        assert!(preflight_major_bump_check(root, &state).is_ok());
    }

    /// A breaking-change commit at Ship fails, naming the bump kind, the
    /// baseline tag, the resulting version, and the deciding commit subject.
    #[test]
    fn major_bump_errs_naming_bump_baseline_and_version_for_major_at_ship() {
        let dir = major_bump_fixture();
        let root = dir.path();
        commit_msg(root, "b.txt", "feat(scope)!: drop legacy api");

        let mut state = State::new(72, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        let err = preflight_major_bump_check(root, &state).unwrap_err();
        assert!(err.contains("MAJOR"), "{err}");
        assert!(err.contains("v1.0.0"), "{err}");
        assert!(err.contains("v2.0.0"), "{err}");
        assert!(err.contains("drop legacy api"), "{err}");
    }

    /// D-10: an unreachable highest tag is surfaced as a preflight failure
    /// (never a silent pass) — mirrors `version.rs`'s own
    /// `unreachable_highest_tag_refuses_rather_than_falling_back` fixture.
    #[test]
    fn major_bump_surfaces_unreachable_baseline_refusal() {
        let dir = major_bump_fixture();
        let root = dir.path();
        let main_branch = {
            let out = devflow_core::test_support::git_command(root)
                .args(["symbolic-ref", "--short", "HEAD"])
                .output()
                .unwrap();
            assert!(out.status.success());
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };

        run_git(root, &["checkout", "--orphan", "orphan-release"]);
        run_git(
            root,
            &["commit", "--allow-empty", "-q", "-m", "chore: orphan"],
        );
        tag(root, "v9.9.9");
        run_git(root, &["checkout", &main_branch]);

        let mut state = State::new(73, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        let err = preflight_major_bump_check(root, &state).unwrap_err();
        assert!(err.contains("v9.9.9"), "{err}");
        assert!(err.contains("not reachable"), "{err}");
    }

    /// CR-02 (`25-REVIEW.md`): a real `git worktree add` fixture proving the
    /// D-09 major-bump gate must classify the WORKTREE's HEAD, not
    /// `project_root`'s, in `devflow start`'s default (worktree) execution
    /// mode. Mirrors `staleness.rs::worktree_staleness_fixture`'s
    /// construction exactly: `project_root` and `worktree_path` are SIBLING
    /// directories under one outer tempdir (never nested — a nested worktree
    /// path would contain `project_root`'s path as a string prefix, making
    /// path-discriminating assertions mutually exclusive), `git init -q -b
    /// develop` plus the same five config lines `major_bump_fixture` uses,
    /// one `chore: init` commit tagged `v1.0.0` on `develop`, then `git
    /// worktree add -b feature/phase-90 <worktree_path> develop` from
    /// `project_root`. Exactly ONE commit is made, and made ONLY inside
    /// `worktree_path`, with the message `feat(scope)!: drop legacy api` —
    /// the identical breaking-marker shape and message
    /// `major_bump_errs_naming_bump_baseline_and_version_for_major_at_ship`
    /// uses, so a classification difference cannot be attributed to message
    /// shape. `project_root`'s HEAD (`develop`) never moves.
    ///
    /// Tags live in the shared object database, so `v1.0.0` is visible and
    /// reachable from the worktree's HEAD (`git tag --merged HEAD` run in the
    /// worktree returns it) — this is the property that makes the fixture
    /// meaningful: the baseline resolves identically from either root, only
    /// the classified range differs.
    ///
    /// Returns `(tempdir_guard, worktree_path)`. `project_root` is
    /// `tempdir_guard.path().join("project")`, matching
    /// `worktree_staleness_fixture`'s return contract. The guard must be kept
    /// alive for the duration of the test.
    fn major_bump_worktree_fixture() -> (tempfile::TempDir, PathBuf) {
        let outer = tempfile::tempdir().unwrap();
        let project_root = outer.path().join("project");
        std::fs::create_dir_all(&project_root).unwrap();
        let worktree_path = outer.path().join("worktree");

        run_git(&project_root, &["init", "-q", "-b", "develop"]);
        run_git(&project_root, &["config", "user.email", "t@e.st"]);
        run_git(&project_root, &["config", "user.name", "t"]);
        run_git(&project_root, &["config", "commit.gpgsign", "false"]);
        run_git(&project_root, &["config", "tag.gpgsign", "false"]);
        run_git(&project_root, &["config", "core.hooksPath", "/dev/null"]);
        commit_msg(&project_root, "a.txt", "chore: init");
        tag(&project_root, "v1.0.0");

        run_git(
            &project_root,
            &[
                "worktree",
                "add",
                "-b",
                "feature/phase-90",
                worktree_path.to_str().unwrap(),
                "develop",
            ],
        );

        // ONE commit made ONLY inside the worktree — project_root's HEAD
        // (develop) never moves. This asymmetry is exactly CR-02's mechanism.
        commit_msg(&worktree_path, "b.txt", "feat(scope)!: drop legacy api");

        (outer, worktree_path)
    }

    /// CR-02 (`25-REVIEW.md`): `preflight_major_bump_check` must classify the
    /// tree the phase's code actually lives in. Before Task 2's fix, this
    /// check always shells git against `project_root`, so a breaking commit
    /// that exists ONLY on the worktree's feature branch (the default
    /// `devflow start` execution mode) is invisible — this test is RED until
    /// Task 2 lands. Both halves are asserted in one test, exactly as
    /// `embedded_commit_is_stale_uses_worktree_head` does, because a single
    /// assertion would pass for the wrong reason if the fixture were built
    /// incorrectly.
    #[test]
    fn preflight_major_bump_check_fires_against_the_worktree_head() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let (outer, worktree_path) = major_bump_worktree_fixture();
        let project_root = outer.path().join("project");

        let mut state = State::new(76, AgentKind::Claude, Mode::Auto, project_root.clone());
        state.stage = Stage::Ship;
        state.worktree_path = Some(worktree_path.clone());

        let err = preflight_major_bump_check(&project_root, &state).unwrap_err();
        assert!(err.contains("MAJOR"), "{err}");
        assert!(err.contains("v1.0.0"), "{err}");
        assert!(err.contains("v2.0.0"), "{err}");
        assert!(err.contains("drop legacy api"), "{err}");

        // Negative half: with no worktree set, the SAME call classifies
        // project_root's HEAD (develop), where the breaking commit does not
        // exist — proving the fixture discriminates on execution root and
        // that the positive assertion above is not passing for an unrelated
        // reason.
        state.worktree_path = None;
        assert!(
            preflight_major_bump_check(&project_root, &state).is_ok(),
            "with no worktree set, the check must classify project_root's own HEAD, which \
             never received the breaking commit"
        );
    }

    /// D-09 integration (mirrors `run_preflight_failing_check_gates_and_never_
    /// reaches_spawn_monitor`): a breaking-commit range at Stage::Ship drives
    /// `run_preflight` into the never-silent gate rather than continuing
    /// toward `hooks_after_ship`, and never reaches `monitor::spawn_monitor`.
    /// PATH is replaced (never prepended) with a `git`-only directory so
    /// `gh` never resolves — this test's outcome must not depend on whether
    /// the host running the suite happens to have `gh` installed and
    /// authenticated, which would otherwise make `preflight_gh_auth_check`
    /// (composed earlier in the same chain) the check that actually fails
    /// instead of this one.
    #[test]
    fn run_preflight_major_bump_gates_and_never_ships_unattended() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let git_only_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", git_only_dir.path());
        }

        let dir = major_bump_fixture();
        let root = dir.path();
        commit_msg(root, "b.txt", "feat(scope)!: drop legacy api");

        let phase = 74;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        state.yes_ship = true;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Ship);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        let adapter = agents::adapter_for(AgentKind::Claude);
        let should_continue = run_preflight(root, &mut state, adapter.as_ref()).unwrap();

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert!(
            !should_continue,
            "a major bump at Ship must gate rather than tell its caller to continue launch_stage"
        );
        assert!(
            workflow::load_state(root, phase).is_err(),
            "abort() must clear state — spawn_monitor was never reached"
        );
        let last = devflow_core::events::last_event_for_phase(root, phase)
            .expect("gate_fired/gate_resolved must have been recorded");
        assert_ne!(last["event"], "stage_launched");
    }

    /// D-09/T-25-51: `state.yes_ship` must never auto-approve this gate.
    /// `run_preflight`'s gate call (`run_gate`, which always passes `None`
    /// for the auto-response — see that parameter's doc comment in
    /// `pipeline_gate.rs`) structurally cannot read `state.yes_ship` at all,
    /// and this test is the concrete proof: with `yes_ship` set and NO
    /// response ever written, the gate poll must block for the full bounded
    /// timeout and then surface a gate-timeout `Err` — an (incorrect)
    /// auto-approval would instead resolve immediately, well inside the
    /// timeout window. `DEVFLOW_GATE_TIMEOUT_SECS` is bounded under
    /// `ENV_MUTEX` so a regression here fails fast instead of hanging the
    /// suite for 7 days (mirrors
    /// `run_preflight_advance_skips_recheck_on_idempotently_failing_check`).
    #[test]
    fn run_preflight_major_bump_gate_not_auto_approved_by_yes_ship() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let original_gate_timeout = std::env::var_os("DEVFLOW_GATE_TIMEOUT_SECS");
        let git_only_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("DEVFLOW_GATE_TIMEOUT_SECS", "1");
            std::env::set_var("PATH", git_only_dir.path());
        }

        let dir = major_bump_fixture();
        let root = dir.path();
        commit_msg(root, "b.txt", "feat(scope)!: drop legacy api");

        let phase = 75;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        state.yes_ship = true;
        workflow::save_state(&state).unwrap();

        let adapter = agents::adapter_for(AgentKind::Claude);
        let result = run_preflight(root, &mut state, adapter.as_ref());

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
            match &original_gate_timeout {
                Some(value) => std::env::set_var("DEVFLOW_GATE_TIMEOUT_SECS", value),
                None => std::env::remove_var("DEVFLOW_GATE_TIMEOUT_SECS"),
            }
        }

        assert!(
            result.is_err(),
            "yes_ship must never auto-approve this gate — an unattended \
             Ok(_) within the bounded timeout would mean it did, got {result:?}"
        );
    }

    /// [`agent_free_git_only_path_dir`], extended with an executable `gh`
    /// stub that always exits 1 — makes [`preflight_gh_auth_check`] take its
    /// hard `Err` branch (`gh` present, reports unauthenticated) rather than
    /// its fail-soft binary-absent branch, which is the only way to compose
    /// a gh-auth failure with a major-bump failure at the same stage. Mirrors
    /// `agent_free_dir_with_agent_stub`'s stub-writing construction (content
    /// plus `PermissionsExt` mode `0o755`).
    fn git_only_path_dir_with_failing_gh() -> tempfile::TempDir {
        use std::os::unix::fs::PermissionsExt;
        let dir = agent_free_git_only_path_dir();
        let path = dir.path().join("gh");
        std::fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        dir
    }

    /// CR-01 (`25-REVIEW.md`, 25-08): `generic_preflight_checks` must not
    /// `?`-short-circuit — a gh-auth failure must not hide a major-bump
    /// failure that also applies at the same stage. Composes Task 1's
    /// worktree fixture (Task 2's fix) with a failing `gh` stub, so the two
    /// defects this plan closes are exercised together, exactly as they
    /// compose in production.
    #[test]
    fn generic_preflight_checks_reports_major_bump_even_when_gh_auth_fails_first() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let git_only_dir = git_only_path_dir_with_failing_gh();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", git_only_dir.path());
        }

        let (outer, worktree_path) = major_bump_worktree_fixture();
        let project_root = outer.path().join("project");
        let mut state = State::new(77, AgentKind::Claude, Mode::Auto, project_root.clone());
        state.stage = Stage::Ship;
        state.worktree_path = Some(worktree_path);

        let err = generic_preflight_checks(&project_root, &state).unwrap_err();

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert!(err.contains("MAJOR"), "{err}");
        assert!(err.contains("drop legacy api"), "{err}");
        assert!(err.contains("not authenticated"), "{err}");

        // T-25-08-03: the 300-character truncation cap cannot elide the
        // highest-consequence reason — proven by ordering the major-bump
        // reason first in generic_preflight_checks.
        assert!(
            truncate_reason(&err).contains("MAJOR"),
            "{}",
            truncate_reason(&err)
        );
    }

    /// A failing preflight check routes through the never-silent gate and,
    /// on Abort, never reaches `monitor::spawn_monitor` — no `stage_launched`
    /// event is ever recorded. The Abort response is pre-seeded so
    /// `run_gate`'s poll resolves immediately.
    #[test]
    fn run_preflight_failing_check_gates_and_never_reaches_spawn_monitor() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 61;
        let mut state = State::new(phase, AgentKind::Codex, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Define;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Define);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        let adapter = agents::adapter_for(AgentKind::Codex);
        let should_continue = run_preflight(root, &mut state, adapter.as_ref()).unwrap();

        assert!(
            !should_continue,
            "an aborted preflight must tell its caller not to continue launch_stage"
        );
        assert!(
            workflow::load_state(root, phase).is_err(),
            "abort() must clear state — spawn_monitor was never reached"
        );
        let last = devflow_core::events::last_event_for_phase(root, phase)
            .expect("gate_fired/gate_resolved must have been recorded");
        assert_ne!(last["event"], "stage_launched");
    }

    /// The adapter-specific hook (D-14 adapter) is actually consulted by
    /// `run_preflight` — a TEST-ONLY adapter that always rejects still routes
    /// through the same gate+abort path as a generic-check failure.
    #[test]
    fn run_preflight_adapter_hook_override_fires() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = 62;
        // Plan is unaffected by the interactivity/gh-auth generic checks, so
        // only the adapter hook can be the source of this failure.
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Plan);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        let should_continue = run_preflight(root, &mut state, &AlwaysFailAdapter).unwrap();

        assert!(
            !should_continue,
            "an aborted preflight must tell its caller not to continue launch_stage"
        );
        assert!(workflow::load_state(root, phase).is_err());
        let last = devflow_core::events::last_event_for_phase(root, phase).unwrap();
        assert_eq!(last["event"], "workflow_aborted");
    }

    // -----------------------------------------------------------------
    // 17-08 gap closure (CR-01): run_preflight's Advance/LoopBack arms must
    // not spawn the agent twice.
    // -----------------------------------------------------------------

    /// CR-01 regression (Advance arm, 17-08 gap closure): a preflight
    /// failure resolved by `GateAction::Advance` must launch the agent
    /// exactly once. `run_preflight` returns `Ok(false)` when the recursive
    /// retry it just ran already spawned the agent — the call site (main.rs
    /// call site inside `launch_stage`) must not run any more launch steps
    /// in that case. This mirrors the call site's exact contract: only run
    /// the explicit `launch_stage(&mut state, None, None)` continuation when
    /// `run_preflight` says to.
    #[test]
    fn run_preflight_advance_gate_launches_agent_exactly_once() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 63;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        // Plan is unaffected by the interactivity/gh-auth generic checks
        // (D-14) — only the injected adapter's `preflight` fails; the real
        // Claude adapter's default (Ok) preflight passes every other check.
        state.stage = Stage::Plan;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Plan);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(&response_path, r#"{"approved":true,"responded_by":"test"}"#).unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let adapter = FailOnceAdapter::new();
        let preflight = run_preflight(root, &mut state, &adapter);
        let continuation = match &preflight {
            Ok(true) => launch_stage(&mut state, None, None),
            _ => Ok(()),
        };

        // WR-05 / 999.44: `run_preflight`'s `Advance` arm calls
        // `launch_stage_inner` directly (`:943`), which spawns a real
        // detached monitor wrapper and records its pid on this same
        // `&mut State` — reached indirectly, through recursion, rather than
        // through this test's own explicit `launch_stage` call above. The
        // guard must bind here, the first line after the final `&mut state`
        // use, because every line below is a panic site, including
        // unwrapping `preflight` itself: that call can spawn the monitor and
        // then still fail a later `?`. The wrapper must die before `dir`
        // unlinks the project root out from under it.
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        // Unwrapping moves below the PATH restore above — deliberate, not an
        // accidental reorder: on the error path, PATH is now restored before
        // the panic instead of after it, narrowing the window in which a
        // failing test leaves a mutated PATH behind for whatever runs next.
        let should_continue = preflight.unwrap();
        continuation.unwrap();

        assert!(
            !should_continue,
            "an Advance-resolved preflight failure must tell its caller not \
             to continue launch_stage — the recursive retry already did"
        );
        let launches = stage_launched_count(root, phase);
        assert_eq!(
            launches, 1,
            "a preflight failure resolved by Advance must launch the agent \
             exactly once, not {launches}"
        );
        assert!(
            state.monitor_pid.is_some(),
            "this test is expected to drive a real monitor::spawn_monitor \
             through run_preflight's Advance arm — None here means WR-05's \
             premise was wrong and the guard above reaped nothing"
        );
    }

    /// CR-01 regression (LoopBack arm, 17-08 gap closure): same defect as
    /// the Advance arm above, but through `GateAction::LoopBack` — per
    /// `GateAction::from_response` (gates.rs:69-78) a rejection whose note
    /// doesn't mention "abort" yields `LoopBack(Stage::Code)`, which
    /// `run_preflight` routes through the identical recursive-relaunch code
    /// path as Advance.
    #[test]
    fn run_preflight_loopback_gate_launches_agent_exactly_once() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 64;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Plan);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"retry","responded_by":"test"}"#,
        )
        .unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let adapter = FailOnceAdapter::new();
        let preflight = run_preflight(root, &mut state, &adapter);
        let continuation = match &preflight {
            Ok(true) => launch_stage(&mut state, None, None),
            _ => Ok(()),
        };

        // WR-05 / 999.44: this test's spawn arrives through the
        // `GateAction::LoopBack(_)` arm (`preflight.rs:945-950`), which calls
        // `launch_stage` at `:949`. That re-resolves the REAL Claude adapter
        // (whose default preflight passes), re-runs `run_preflight` against
        // it, and falls through to the same `launch_stage_inner` and the
        // same `monitor::spawn_monitor` — one spawn, on the same
        // `&mut State`, reached by a longer road than the Advance arm above.
        // The guard must bind here, the first line after the final
        // `&mut state` use, because every line below is a panic site,
        // including unwrapping `preflight` itself: that call can spawn the
        // monitor and then still fail a later `?`. The wrapper must die
        // before `dir` unlinks the project root out from under it.
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        // Unwrapping moves below the PATH restore above — deliberate, not an
        // accidental reorder: on the error path, PATH is now restored before
        // the panic instead of after it, narrowing the window in which a
        // failing test leaves a mutated PATH behind for whatever runs next.
        let should_continue = preflight.unwrap();
        continuation.unwrap();

        assert!(
            !should_continue,
            "a LoopBack-resolved preflight failure must tell its caller not \
             to continue launch_stage — the recursive retry already did"
        );
        let launches = stage_launched_count(root, phase);
        assert_eq!(
            launches, 1,
            "a preflight failure resolved by LoopBack must launch the agent \
             exactly once, not {launches}"
        );
        assert!(
            state.monitor_pid.is_some(),
            "this test is expected to drive a real monitor::spawn_monitor \
             through run_preflight's LoopBack arm — None here means WR-05's \
             premise was wrong and the guard above reaped nothing"
        );
    }

    // -----------------------------------------------------------------
    // 18f (D-18f): approving a preflight gate must not re-run the just-
    // adjudicated check, LoopBack's re-check must be bounded, and the
    // bound's reset must persist.
    //
    // These three tests deliberately fail via `preflight_interactivity_check`
    // (Codex + Auto + Define + no CONTEXT.md on develop), NOT via
    // `AlwaysFailAdapter`'s adapter hook. `AlwaysFailAdapter` is still
    // passed as the `adapter` argument (defense in depth — it would also
    // fail were it ever reached), but it structurally CANNOT be what
    // reproduces the wedge across a relaunch: `launch_stage`'s internal
    // recursion always re-resolves the REAL production adapter via
    // `agents::adapter_for(state.agent)`, discarding whatever adapter
    // reference was passed into the OUTER `run_preflight` call (confirmed
    // by `run_preflight_advance_gate_launches_agent_exactly_once`'s own
    // comment above: "the real Claude adapter's default (Ok) preflight
    // passes every other check"). The generic checks, by contrast, are a
    // pure function of `state` alone and so fail IDENTICALLY on every
    // invocation — exactly the property CONTEXT.md attributes to
    // `preflight_interactivity_check`/`preflight_gh_auth_check` in its
    // description of the wedge.
    // -----------------------------------------------------------------

    /// D-18f: `GateAction::Advance` must skip the just-adjudicated check
    /// entirely — with the pre-18f code (full `launch_stage` recursion),
    /// the SAME deterministic `preflight_interactivity_check` failure would
    /// fire again on the retry, write a SECOND gate nobody answers (only
    /// one response is ever seeded here), and `run_preflight` would return
    /// `Err` (a bounded gate-timeout error) instead of `Ok(false)` — that
    /// bounded `Err` is the RED signal this test would observe pre-fix,
    /// confirmed manually before restoring the fix. `DEVFLOW_GATE_TIMEOUT_SECS`
    /// is bounded under `ENV_MUTEX` so a regression here fails fast instead
    /// of hanging the suite for 7 days.
    #[test]
    fn run_preflight_advance_skips_recheck_on_idempotently_failing_check() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let original_gate_timeout = std::env::var_os("DEVFLOW_GATE_TIMEOUT_SECS");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("DEVFLOW_GATE_TIMEOUT_SECS", "2");
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 620;
        // Codex + Auto + Define + no `.planning/phases/620-*/620-CONTEXT.md`
        // on `develop` deterministically fails `preflight_interactivity_check`
        // — see the section doc comment above for why this (not the adapter
        // hook) is what actually reproduces the wedge across a relaunch.
        let mut state = State::new(phase, AgentKind::Codex, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Define;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Define);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(&response_path, r#"{"approved":true,"responded_by":"test"}"#).unwrap();

        let agent_dir = agent_free_dir_with_agent_stub("codex");
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", agent_dir.path());
        }

        let result = run_preflight(root, &mut state, &AlwaysFailAdapter);

        // WR-05 / 999.44 (residual finding, 25-18 verification step 6): the
        // "approved" response here drives run_preflight's GateAction::Advance
        // arm, which calls launch_stage_inner UNCONDITIONALLY — skipping the
        // just-adjudicated check is the entire point of this test, so there
        // is no recursive re-check to fail first. With a working `codex` +
        // `sh` stub on PATH (`agent_free_dir_with_agent_stub`), this spawns a
        // REAL detached monitor wrapper — confirmed empirically while
        // re-deriving this plan's verification-step-6 enumeration, a leak
        // this plan's originally-declared scope (the two `_exactly_once`
        // tests) did not name. The guard must bind here, the first line
        // after the final `&mut state` use, ahead of every panicking
        // checkpoint below, for the same reason as the two conversions above.
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
            match &original_gate_timeout {
                Some(value) => std::env::set_var("DEVFLOW_GATE_TIMEOUT_SECS", value),
                None => std::env::remove_var("DEVFLOW_GATE_TIMEOUT_SECS"),
            }
        }

        assert!(
            matches!(result, Ok(false)),
            "Advance on a preflight gate must skip the just-adjudicated \
             check and return Ok(false), not {result:?}"
        );
        assert!(
            !Gates::gate_path(root, phase, Stage::Define).exists(),
            "no second gate should ever be written once Advance skips the recheck"
        );
        assert_eq!(
            state.preflight_retries, 0,
            "a human Advance must reset the retry counter"
        );
        assert!(
            state.monitor_pid.is_some(),
            "this test's Advance arm is expected to drive a real \
             monitor::spawn_monitor via launch_stage_inner — None here means \
             this residual finding's premise was wrong and the guard above \
             reaped nothing"
        );
    }

    /// D-18f backstop: `GateAction::LoopBack` deliberately keeps re-running
    /// the check (unlike Advance), so the recursion must be bounded
    /// separately. `state.preflight_retries` starts one below the ceiling —
    /// exercising the bound via a REAL recursive `run_preflight` call
    /// (through `launch_stage`) rather than simulating multiple cycles: with
    /// only ONE gate response ever seeded, and `Gates::poll_response`
    /// blocking synchronously in this same thread, nothing could seed a
    /// SECOND response file mid-recursion inside one call stack — deferring
    /// to the ceiling on the very next cycle instead genuinely exercises
    /// "one retry short of the ceiling" → "ceiling reached" without a racy
    /// background writer.
    #[test]
    fn run_preflight_loopback_bounds_recursion() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let original_gate_timeout = std::env::var_os("DEVFLOW_GATE_TIMEOUT_SECS");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("DEVFLOW_GATE_TIMEOUT_SECS", "2");
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 621;
        let mut state = State::new(phase, AgentKind::Codex, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Define;
        state.preflight_retries = mode::MAX_PREFLIGHT_RETRIES - 1;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Define);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"retry","responded_by":"test"}"#,
        )
        .unwrap();

        let agent_dir = agent_free_dir_with_agent_stub("codex");
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", agent_dir.path());
        }

        let result = run_preflight(root, &mut state, &AlwaysFailAdapter);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
            match &original_gate_timeout {
                Some(value) => std::env::set_var("DEVFLOW_GATE_TIMEOUT_SECS", value),
                None => std::env::remove_var("DEVFLOW_GATE_TIMEOUT_SECS"),
            }
        }

        assert!(
            matches!(result, Ok(false)),
            "the ceiling must abort cleanly, not error out, got {result:?}"
        );
        assert!(
            workflow::load_state(root, phase).is_err(),
            "the ceiling must abort() and clear state, not leave it gate_pending forever"
        );
        let last = devflow_core::events::last_event_for_phase(root, phase)
            .expect("a ceiling or abort event must have been recorded");
        assert!(
            last["event"] == "preflight_retry_ceiling_reached"
                || last["event"] == "workflow_aborted",
            "expected a ceiling or abort event, got {last:?}"
        );
    }

    /// D-18f (assumption_delta, Open Question 2): the reset on a passing
    /// preflight must be PERSISTED, not merely in-memory — the wedge this
    /// counter bounds spans separate `devflow` invocations (a monitor
    /// restart reloads state from disk), so an in-memory-only reset would
    /// not survive one.
    #[test]
    fn preflight_retries_reset_on_pass() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = 622;
        // Plan + Claude bypasses the generic checks and the real Claude
        // adapter's default preflight passes — the same "unaffected" shape
        // used by `run_preflight_adapter_hook_override_fires` above.
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        state.preflight_retries = 2;
        workflow::save_state(&state).unwrap();

        let adapter = agents::adapter_for(AgentKind::Claude);
        let result = run_preflight(root, &mut state, adapter.as_ref());

        assert!(
            matches!(result, Ok(true)),
            "a passing preflight must return Ok(true), got {result:?}"
        );
        assert_eq!(
            state.preflight_retries, 0,
            "the in-memory counter must reset immediately on a pass"
        );

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.preflight_retries, 0,
            "the reset must be persisted to disk, not just held in memory"
        );
    }

    // -----------------------------------------------------------------
    // 23f (gap closure, 23-12, Task 2): phase-reachability discrimination —
    // the guard must refuse for the right reason, and fail open where it
    // cannot see.
    // -----------------------------------------------------------------

    /// Builds a git repo with a `develop` branch whose `.planning/ROADMAP.md`
    /// content is controlled per-test, optionally committing a
    /// `.planning/phases/{phase:02}-{slug}/.gitkeep` alongside it.
    fn reachability_fixture(roadmap: &str, phase_dir: Option<(u32, &str)>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let run = |args: &[&str]| {
            assert!(
                devflow_core::test_support::git_command(root)
                    .args(args)
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "git {args:?} failed"
            );
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@e.st"]);
        run(&["config", "user.name", "t"]);
        run(&["config", "commit.gpgsign", "false"]);
        run(&["config", "core.hooksPath", "/dev/null"]);
        run(&["checkout", "-q", "-b", "develop"]);

        std::fs::create_dir_all(root.join(".planning")).unwrap();
        std::fs::write(root.join(".planning/ROADMAP.md"), roadmap).unwrap();
        if let Some((phase, slug)) = phase_dir {
            let d = root.join(format!(".planning/phases/{phase:02}-{slug}"));
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(".gitkeep"), "").unwrap();
        }
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "fixture"]);
        dir
    }

    #[test]
    fn reachability_is_reachable_when_roadmap_entry_and_phase_dir_are_both_on_base() {
        let dir = reachability_fixture("### Phase 24: Something\n", Some((24, "something")));
        let root = dir.path();
        assert_eq!(
            phase_reachability_on_base(root, 24, "develop"),
            PhaseReachability::Reachable
        );
        assert!(ensure_phase_reachable_on_base(root, 24, "develop").is_ok());
    }

    #[test]
    fn reachability_is_unreachable_when_the_phase_dir_is_absent_from_base() {
        let dir = reachability_fixture("### Phase 24: Something\n", None);
        let root = dir.path();
        assert_eq!(
            phase_reachability_on_base(root, 24, "develop"),
            PhaseReachability::Unreachable {
                roadmap_entry_found: true,
                phase_dir_found: false,
            }
        );
    }

    #[test]
    fn reachability_is_unreachable_when_the_roadmap_entry_is_absent_from_base() {
        let dir = reachability_fixture("### Phase 1: Something else\n", Some((24, "something")));
        let root = dir.path();
        assert_eq!(
            phase_reachability_on_base(root, 24, "develop"),
            PhaseReachability::Unreachable {
                roadmap_entry_found: false,
                phase_dir_found: true,
            }
        );
    }

    /// D-fail-open: a probe which cannot see must never refuse.
    #[test]
    fn reachability_is_undeterminable_outside_a_git_repo() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert_eq!(
            phase_reachability_on_base(root, 24, "develop"),
            PhaseReachability::Undeterminable
        );
        assert!(
            ensure_phase_reachable_on_base(root, 24, "develop").is_ok(),
            "a probe that cannot see must never refuse (fail-open contract)"
        );
    }

    /// This is precisely the shape of every existing `phase7_cli.rs` fixture
    /// (a real `develop` branch with a committed file but no
    /// `.planning/ROADMAP.md`) — why the guard is not a breaking change.
    #[test]
    fn reachability_is_undeterminable_when_base_has_no_roadmap_at_all() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let run = |args: &[&str]| {
            assert!(
                devflow_core::test_support::git_command(root)
                    .args(args)
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "git {args:?} failed"
            );
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@e.st"]);
        run(&["config", "user.name", "t"]);
        run(&["config", "commit.gpgsign", "false"]);
        run(&["config", "core.hooksPath", "/dev/null"]);
        run(&["checkout", "-q", "-b", "develop"]);
        std::fs::write(root.join("README.md"), "no roadmap here\n").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "no roadmap"]);

        assert_eq!(
            phase_reachability_on_base(root, 24, "develop"),
            PhaseReachability::Undeterminable
        );
        assert!(ensure_phase_reachable_on_base(root, 24, "develop").is_ok());
    }

    /// Pins the trailing colon's job: it stops a probe for phase 24 from
    /// matching a heading for a different phase number that merely starts
    /// with the same digits.
    #[test]
    fn phase_heading_probe_does_not_confuse_a_phase_number_prefix() {
        let dir = reachability_fixture("### Phase 240: Later\n", Some((24, "something")));
        let root = dir.path();
        assert_eq!(
            phase_reachability_on_base(root, 24, "develop"),
            PhaseReachability::Unreachable {
                roadmap_entry_found: false,
                phase_dir_found: true,
            }
        );
    }

    #[test]
    fn unreachable_message_names_the_base_branch_and_each_missing_half() {
        let roadmap_missing = unreachable_message(24, "develop", false, true);
        assert!(roadmap_missing.contains("is not reachable from"));
        assert!(roadmap_missing.contains("develop"));
        assert!(roadmap_missing.contains("### Phase 24:"));
        assert!(!roadmap_missing.contains(".planning/phases/24-"));

        let dir_missing = unreachable_message(24, "develop", true, false);
        assert!(dir_missing.contains("is not reachable from"));
        assert!(dir_missing.contains("develop"));
        assert!(!dir_missing.contains("### Phase 24:"));
        assert!(dir_missing.contains(".planning/phases/24-"));

        let both_missing = unreachable_message(24, "develop", false, false);
        assert!(both_missing.contains("is not reachable from"));
        assert!(both_missing.contains("develop"));
        assert!(both_missing.contains("### Phase 24:"));
        assert!(both_missing.contains(".planning/phases/24-"));
    }

    /// 999.10: the message must contain no absolute filesystem path or
    /// username — every path it names is repository-relative.
    #[test]
    fn unreachable_message_contains_no_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let fixture_root = dir.path().to_string_lossy().into_owned();
        let msg = unreachable_message(24, "develop", false, false);
        assert!(!msg.contains(&fixture_root));
        assert!(!msg.contains("/home/"));
        assert!(!msg.contains("/Users/"));
    }

    // -----------------------------------------------------------------
    // 25e (999.51/D-18a): base-ref currency probe.
    // -----------------------------------------------------------------

    /// Runs `args` in `root` via the hermetic `test_support::git_command`
    /// (never a bare git command) and asserts success.
    fn run_git(root: &Path, args: &[&str]) {
        assert!(
            devflow_core::test_support::git_command(root)
                .args(args)
                .output()
                .unwrap()
                .status
                .success(),
            "git {args:?} failed in {root:?}"
        );
    }

    /// Builds a real "remote" repository (checked out on `develop`, one
    /// commit) and a real local clone of it, so `origin/develop` genuinely
    /// exists in the local clone via git's own clone machinery and can be
    /// moved independently of the local branch by committing directly in
    /// the remote's own working tree. Returns `(remote_dir, local_dir)` —
    /// both `TempDir`s must be kept alive by the caller.
    fn currency_fixture() -> (tempfile::TempDir, tempfile::TempDir) {
        let remote_dir = tempfile::tempdir().unwrap();
        let remote_root = remote_dir.path();
        run_git(remote_root, &["init", "-q"]);
        run_git(remote_root, &["config", "user.email", "t@e.st"]);
        run_git(remote_root, &["config", "user.name", "t"]);
        run_git(remote_root, &["config", "commit.gpgsign", "false"]);
        run_git(remote_root, &["config", "core.hooksPath", "/dev/null"]);
        run_git(remote_root, &["checkout", "-q", "-b", "develop"]);
        std::fs::write(remote_root.join("f.txt"), "1").unwrap();
        run_git(remote_root, &["add", "-A"]);
        run_git(remote_root, &["commit", "-q", "-m", "c1"]);

        let local_dir = tempfile::tempdir().unwrap();
        let local_root = local_dir.path();
        run_git(
            local_root,
            &["clone", "-q", remote_root.to_str().unwrap(), "."],
        );
        run_git(local_root, &["config", "user.email", "t@e.st"]);
        run_git(local_root, &["config", "user.name", "t"]);
        run_git(local_root, &["config", "commit.gpgsign", "false"]);
        run_git(local_root, &["config", "core.hooksPath", "/dev/null"]);

        (remote_dir, local_dir)
    }

    /// Advances the remote fixture's `develop` branch by one commit, in its
    /// own working tree — independent of whatever the local clone has done.
    fn advance_remote(remote_root: &Path, filename: &str) {
        std::fs::write(remote_root.join(filename), "2").unwrap();
        run_git(remote_root, &["add", "-A"]);
        run_git(remote_root, &["commit", "-q", "-m", "c2"]);
    }

    #[test]
    fn currency_is_current_when_local_equals_remote() {
        let (_remote, local) = currency_fixture();
        let root = local.path();
        assert_eq!(base_ref_currency(root, "develop"), BaseRefCurrency::Current);
        assert!(ensure_base_ref_current(root, "develop").is_ok());
    }

    #[test]
    fn currency_behind_and_not_checked_out_fast_forwards_and_proceeds() {
        let (remote, local) = currency_fixture();
        let remote_root = remote.path();
        let local_root = local.path();
        advance_remote(remote_root, "f2.txt");

        // Switch the local clone off `develop` so the fast-forward is safe
        // to attempt (it does not touch the working tree, but the guard
        // only attempts it when `develop` is not the checked-out branch).
        run_git(local_root, &["checkout", "-q", "-b", "other"]);

        assert_eq!(
            base_ref_currency(local_root, "develop"),
            BaseRefCurrency::Behind { count: 1 }
        );

        assert!(
            ensure_base_ref_current(local_root, "develop").is_ok(),
            "a safely-behind base must fast-forward and proceed unattended"
        );

        let rev_parse = |rref: &str| {
            let out = devflow_core::test_support::git_command(local_root)
                .args(["rev-parse", rref])
                .output()
                .unwrap();
            assert!(out.status.success(), "rev-parse {rref} failed");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(
            rev_parse("develop"),
            rev_parse("origin/develop"),
            "the local ref must now equal the remote-tracking ref after the fast-forward"
        );
    }

    #[test]
    fn currency_behind_and_checked_out_refuses_with_actionable_message() {
        let (remote, local) = currency_fixture();
        let remote_root = remote.path();
        let local_root = local.path();
        advance_remote(remote_root, "f2.txt");
        // `develop` stays checked out (the clone's default checkout).

        assert_eq!(
            base_ref_currency(local_root, "develop"),
            BaseRefCurrency::Behind { count: 1 }
        );

        let err = ensure_base_ref_current(local_root, "develop").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("develop"), "{msg}");
        assert!(msg.contains("origin/develop"), "{msg}");
        assert!(msg.contains('1'), "{msg}");
        assert!(msg.contains("git "), "{msg}");
        let fixture_root = local_root.to_string_lossy().into_owned();
        assert!(!msg.contains(&fixture_root), "{msg}");
    }

    #[test]
    fn currency_behind_fast_forward_failure_falls_through_to_refusal() {
        let (remote, local) = currency_fixture();
        let remote_root = remote.path();
        let local_root = local.path();
        advance_remote(remote_root, "f2.txt");
        run_git(local_root, &["checkout", "-q", "-b", "other"]);

        // Force `git update-ref refs/heads/develop ...` to fail by
        // pre-seeding its lock file (a stale lock is exactly the shape git
        // itself reports: "Unable to create '...develop.lock': File exists").
        std::fs::create_dir_all(local_root.join(".git/refs/heads")).unwrap();
        std::fs::write(local_root.join(".git/refs/heads/develop.lock"), "").unwrap();

        let result = ensure_base_ref_current(local_root, "develop");
        assert!(
            result.is_err(),
            "a failed fast-forward must fall through to refusal, never a silent proceed"
        );
    }

    #[test]
    fn base_is_checked_out_anywhere_sees_a_linked_worktree() {
        let (_remote, local) = currency_fixture();
        let local_root = local.path();
        // Move the local clone off `develop` first — `git worktree add`
        // requires the branch not already be checked out anywhere.
        run_git(local_root, &["checkout", "-q", "-b", "other"]);

        let linked = tempfile::tempdir().unwrap();
        run_git(
            local_root,
            &[
                "worktree",
                "add",
                linked.path().to_str().unwrap(),
                "develop",
            ],
        );

        assert!(
            base_is_checked_out_anywhere(local_root, "develop"),
            "a linked worktree with `develop` checked out must be seen, even though \
             `project_root`'s own HEAD is on `other`"
        );
        assert!(
            !base_is_checked_out_anywhere(local_root, "no-such-branch"),
            "a branch no worktree holds must not be reported as checked out"
        );
        // Keep both TempDir guards alive to the end of the test.
        drop(linked);
    }

    #[test]
    fn currency_behind_refuses_when_base_is_checked_out_in_another_worktree() {
        let (remote, local) = currency_fixture();
        let remote_root = remote.path();
        let local_root = local.path();
        advance_remote(remote_root, "f2.txt");

        // Move the local clone off `develop` FIRST, then add the linked
        // worktree on `develop` — the reverse order fails `worktree add`.
        run_git(local_root, &["checkout", "-q", "-b", "other"]);
        let linked = tempfile::tempdir().unwrap();
        run_git(
            local_root,
            &[
                "worktree",
                "add",
                linked.path().to_str().unwrap(),
                "develop",
            ],
        );

        let rev_parse = |rref: &str| {
            let out = devflow_core::test_support::git_command(local_root)
                .args(["rev-parse", rref])
                .output()
                .unwrap();
            assert!(out.status.success(), "rev-parse {rref} failed");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        let before = rev_parse("develop");

        assert_eq!(
            base_ref_currency(local_root, "develop"),
            BaseRefCurrency::Behind { count: 1 }
        );

        let err = ensure_base_ref_current(local_root, "develop").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("develop"), "{msg}");
        assert!(msg.contains("origin/develop"), "{msg}");
        let fixture_root = local_root.to_string_lossy().into_owned();
        assert!(!msg.contains(&fixture_root), "{msg}");

        assert_eq!(
            rev_parse("develop"),
            before,
            "`develop` must be unmoved when it is checked out in a linked worktree, even \
             though `project_root`'s own HEAD is on `other`"
        );
        drop(linked);
    }

    #[test]
    fn fast_forward_base_ref_refuses_a_stale_expected_old_value() {
        let (remote, local) = currency_fixture();
        let remote_root = remote.path();
        let local_root = local.path();
        advance_remote(remote_root, "f2.txt");
        run_git(local_root, &["checkout", "-q", "-b", "other"]);
        // The clone's `origin/develop` remote-tracking ref is stale until
        // fetched — without this it still equals `develop`, which would
        // make the "wrong" expected-old value below accidentally correct.
        run_git(local_root, &["fetch", "-q", "origin", "develop"]);

        let rev_parse = |rref: &str| {
            let out = devflow_core::test_support::git_command(local_root)
                .args(["rev-parse", rref])
                .output()
                .unwrap();
            assert!(out.status.success(), "rev-parse {rref} failed");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        let before = rev_parse("develop");
        let remote_sha = rev_parse("origin/develop");

        // `remote_sha` is a real, valid object in this fixture — it simply
        // is not where `develop` currently points. A syntactically valid
        // but wrong expectation, the exact shape of the race being defended
        // against.
        assert!(
            !fast_forward_base_ref(local_root, "develop", &remote_sha, &remote_sha),
            "a stale expected-old value must refuse the write"
        );
        assert_eq!(
            rev_parse("develop"),
            before,
            "a refused compare-and-swap must leave the ref byte-identical"
        );

        // Positive control: without a stale expectation, the same call
        // succeeds and moves the ref — otherwise an always-`false`
        // implementation would also pass the assertion above.
        assert!(
            fast_forward_base_ref(local_root, "develop", &before, &remote_sha),
            "the correct expected-old value must succeed"
        );
        assert_eq!(rev_parse("develop"), remote_sha);
    }

    #[test]
    fn currency_is_ahead_for_unpushed_local_work() {
        let (_remote, local) = currency_fixture();
        let local_root = local.path();
        std::fs::write(local_root.join("local-only.txt"), "x").unwrap();
        run_git(local_root, &["add", "-A"]);
        run_git(local_root, &["commit", "-q", "-m", "unpushed local work"]);

        assert_eq!(
            base_ref_currency(local_root, "develop"),
            BaseRefCurrency::Ahead
        );
        assert!(
            ensure_base_ref_current(local_root, "develop").is_ok(),
            "unpushed local work must not be misreported as staleness"
        );
    }

    #[test]
    fn currency_is_diverged_when_local_and_remote_both_moved_independently() {
        let (remote, local) = currency_fixture();
        let remote_root = remote.path();
        let local_root = local.path();

        // Advance the remote independently...
        advance_remote(remote_root, "remote-only.txt");
        // ...and the local branch independently, from the same ancestor.
        std::fs::write(local_root.join("local-only.txt"), "x").unwrap();
        run_git(local_root, &["add", "-A"]);
        run_git(
            local_root,
            &["commit", "-q", "-m", "local-only divergent commit"],
        );

        assert_eq!(
            base_ref_currency(local_root, "develop"),
            BaseRefCurrency::Diverged
        );

        let err = ensure_base_ref_current(local_root, "develop").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("develop"), "{msg}");
        assert!(msg.contains("origin/develop"), "{msg}");
    }

    #[test]
    fn currency_is_undeterminable_with_no_remote_configured() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        assert_eq!(
            base_ref_currency(root, "develop"),
            BaseRefCurrency::Undeterminable
        );
        assert!(
            ensure_base_ref_current(root, "develop").is_ok(),
            "a probe that cannot see the remote at all must never refuse"
        );
    }

    /// The fetch subprocess failing must not itself fail the check — the
    /// comparison proceeds against whatever the remote-tracking ref
    /// currently resolves to locally (here, still `Current` from the
    /// initial clone), rather than collapsing to `Undeterminable`.
    #[test]
    fn currency_fetch_failure_falls_back_to_existing_remote_ref() {
        let (remote, local) = currency_fixture();
        let local_root = local.path();
        // Delete the remote so any subsequent `git fetch` fails — but
        // `origin/develop` already resolves locally from the initial clone.
        drop(remote);

        assert_eq!(
            base_ref_currency(local_root, "develop"),
            BaseRefCurrency::Current,
            "a failing fetch must fall back to the already-resolved remote-tracking ref, \
             not collapse to Undeterminable"
        );
    }

    #[test]
    fn currency_message_contains_no_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let fixture_root = dir.path().to_string_lossy().into_owned();
        let msg = stale_base_message("develop", "origin/develop", 3);
        assert!(!msg.contains(&fixture_root));
        assert!(!msg.contains("/home/"));
        assert!(!msg.contains("/Users/"));
        assert!(msg.contains("develop"));
        assert!(msg.contains("origin/develop"));
        assert!(msg.contains('3'));
    }
}
