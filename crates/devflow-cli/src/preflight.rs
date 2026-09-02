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
use crate::commands::phase_artifact_on_base;
use crate::pipeline_gate::{abort, run_gate};
use crate::pipeline_launch::launch_stage;
use crate::pipeline_launch::launch_stage_inner;
use crate::pipeline_launch::stream_launch_enabled;
use crate::pipeline_outcomes::truncate_reason;
use devflow_core::gates::{GateAction, Gates};
use devflow_core::git::git_command;
use devflow_core::gsd_config::{self, GsdConfigError};
use devflow_core::mode::{self, Mode};
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::{agents, events, verify, version, workflow};
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
    agents::driver_for(agent)
        .build_command(PhaseId::new(0), "", &[])
        .0
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
/// fail-open-where-blind contract this project's `phase_artifact_on_base`
/// (`commands.rs`) already establishes.
///
/// `base` is the project-resolved integration trunk from
/// `devflow_core::config::base_branch` (45-01 / D-01), not a constant: a
/// project whose planning artifacts live on a branch other than `develop`
/// configures it via `base_branch` in `devflow.toml` or
/// `DEVFLOW_BASE_BRANCH`. Callers must pass the same resolved value the run
/// actually forks from. The REMOTE name remains non-configurable — see the
/// [`ORIGIN`] constant's own note for why.
pub(crate) fn phase_reachability_on_base(
    project_root: &Path,
    phase: PhaseId,
    base: &str,
) -> PhaseReachability {
    // Step 1: does the base branch even exist here?
    let verify = git_command(project_root)
        .args(["rev-parse", "--verify", "--quiet", base])
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
    let roadmap = git_command(project_root)
        .args(["show", &format!("{base}:.planning/ROADMAP.md")])
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
    // `commands::phase_artifact_on_base` — a directory holding only a
    // `.gitkeep` still counts as present, and phase numbers are
    // zero-padded (phase 7 is `07-`, phase 24 is `24-`).
    let ls_tree = git_command(project_root)
        .args([
            "ls-tree",
            "-r",
            "--name-only",
            base,
            "--",
            ".planning/phases/",
        ])
        .output();
    let phase_dir_found = match ls_tree {
        Ok(out) if out.status.success() => {
            let prefix = format!(".planning/phases/{padded}-", padded = phase.padded());
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
    phase: PhaseId,
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
            "  missing: a `.planning/phases/{padded}-*/` directory on `{base}`\n",
            padded = phase.padded()
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
///
/// **Only the ROADMAP heading is load-bearing for the refusal (999.63).**
/// A missing `.planning/phases/{NN}-*/` directory is reported when the
/// heading is *also* missing — it is useful diagnostic detail there — but it
/// never refuses on its own.
///
/// The reason is a bootstrap inversion. That directory is **Define's own
/// output**: `/gsd-discuss-phase N` is what creates it. Requiring it as a
/// precondition for `devflow start` therefore demanded the product of the
/// first stage in order to run the first stage, which structurally prevented
/// DevFlow from ever driving GSD discussion mode for a newly-promoted phase.
///
/// 23-12 added the conjunction to catch a real failure — a phase promoted
/// only on another branch is invisible to its own run and floundered silently
/// through Define (the 2026-07-26 acceptance-run failure, `23-FINDINGS.md`
/// §B1). **The heading check alone still catches that class**: a phase
/// promoted only on another branch has no heading on `base` either. The
/// directory conjunct added no detection power for it, while creating a
/// false-positive class — heading present, directory absent — whose only
/// members are legitimate bootstrap states, never failures.
///
/// Corroborating intent: the Codex leg immediately below this guard's call
/// site refuses a missing `CONTEXT.md` with *"codex cannot run an interactive
/// discussion headless … or use `--agent claude`"* — a message that only
/// makes sense if Claude driving Define to *produce* the discussion was meant
/// to work. This guard ran first and silently revoked that for every agent.
///
/// `phase_reachability_on_base` is deliberately left alone: it stays a pure
/// two-field probe, so 27-05's hostile-`GIT_DIR` test keeps discriminating on
/// the directory half.
pub(crate) fn ensure_phase_reachable_on_base(
    project_root: &Path,
    phase: PhaseId,
    base: &str,
) -> Result<(), CliError> {
    match phase_reachability_on_base(project_root, phase, base) {
        PhaseReachability::Reachable | PhaseReachability::Undeterminable => Ok(()),
        // The directory half alone must never refuse — see the doc comment.
        PhaseReachability::Unreachable {
            roadmap_entry_found: true,
            ..
        } => Ok(()),
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
/// The fetch updates ONLY the remote-tracking ref (`git fetch <remote> <base>`)
/// — it never touches the local branch or the working tree, so it
/// cannot fail with a "branch is checked out" error the way a refspec fetch
/// into the local branch would. On spawn error or non-zero exit the fetch
/// fails SOFT: a warning is printed and the comparison proceeds against
/// whatever the remote-tracking ref currently holds (Phase 24's inherited
/// D-06 precedent — a new code path must not introduce a false hard-block
/// on correct work, and an offline machine is correct work).
pub(crate) fn base_ref_currency(project_root: &Path, base: &str) -> BaseRefCurrency {
    let remote_ref = format!("{ORIGIN}/{base}");

    let fetch_ok = git_command(project_root)
        .args(["fetch", "--quiet", ORIGIN, base])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if !fetch_ok {
        println!(
            "warning: could not fetch `{ORIGIN} {base}` — comparing `{base}` against \
             whatever `{remote_ref}` currently resolves to locally, which may be stale"
        );
    }

    let ref_exists = git_command(project_root)
        .args(["rev-parse", "--verify", "--quiet", &remote_ref])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if !ref_exists {
        return BaseRefCurrency::Undeterminable;
    }

    let is_ancestor = |ancestor: &str, descendant: &str| {
        git_command(project_root)
            .args(["merge-base", "--is-ancestor", ancestor, descendant])
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
            let count = git_command(project_root)
                .args(["rev-list", "--count", &format!("{base}..{remote_ref}")])
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
    let out = git_command(project_root)
        .args(["worktree", "list", "--porcelain"])
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
    git_command(project_root)
        .args([
            "update-ref",
            &format!("refs/heads/{base}"),
            new,
            expected_old,
        ])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// The warning [`ensure_base_ref_current`] prints when a base branch's
/// currency cannot be determined.
///
/// A pure function rather than an inline `format!` for one reason: the arm
/// that emits it has no injectable writer anywhere in its call path, so a
/// test asserting only that `ensure_base_ref_current` returns `Ok(())` stays
/// green after this message is deleted or reworded. Extracting it is what
/// makes the message contract assertable at all.
pub(crate) fn undeterminable_currency_warning(base: &str) -> String {
    format!(
        "warning: could not determine whether `{base}` is current with `{ORIGIN}/{base}` \
         — proceeding without a currency check (fail-open, per this module's \
         fail-open-where-blind contract)"
    )
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
        // FAIL DIRECTION: OPEN. This arm is what allows a local-only
        // planning branch — one with no `origin/<base>` tracking ref, the
        // shape 45-01/AUTO-01 exists to support — to launch at all. Refusing
        // here would convert "we cannot see the remote" into "your base is
        // stale", which is the AUTO-01 defect wearing a different hat.
        // Guarded by
        // `base_ref_currency_is_undeterminable_when_the_remote_ref_is_absent`
        // (the classification) and
        // `ensure_base_ref_current_fails_open_for_a_local_only_planning_branch`
        // (the disposition); the WARNING's own text is guarded separately by
        // `undeterminable_currency_warning_names_the_branch_and_its_disposition`,
        // because a bare `println!` here is unobservable from a unit test.
        BaseRefCurrency::Undeterminable => {
            println!("{}", undeterminable_currency_warning(base));
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
                    git_command(project_root)
                        .args(["rev-parse", "--verify", "--quiet", rref])
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

/// The driver-driven interactivity gate (999.106): whether `stage` can run
/// headless is declared by the driver's `interactivity_mode`, not a hardcoded
/// `agent == Codex` check. `RequiresExistingArtifact` gates **Define only**
/// (CONTEXT.md must already be on develop in auto mode — a pre-existing input);
/// Plan is deliberately un-gated because PLAN.md is an *output* the phase
/// itself produces. `HeadlessSafe` is never refused; `RequiresTypedSubagents`
/// / `InteractiveOnly` have no headless path at all. Routes the failure
/// through the preflight gate (D-15) rather than a hard error.
pub(crate) fn preflight_interactivity_check(
    project_root: &Path,
    state: &State,
) -> Result<(), String> {
    use devflow_core::agents::InteractivityMode;
    let driver = agents::driver_for(state.agent);
    match driver.interactivity_mode(state.stage) {
        InteractivityMode::HeadlessSafe => Ok(()),
        InteractivityMode::RequiresExistingArtifact => {
            // The base is resolved per project (45-01 / D-01). This probe is
            // a fail-open advisory: on the resolver's `Err` it falls back to
            // the default trunk rather than propagating, because
            // `commands::start` already refuses on that same `Err` before any
            // run reaches here.
            let base = devflow_core::config::base_branch(project_root)
                .map(|resolved| resolved.value)
                .unwrap_or_else(|_| devflow_core::config::DEVELOP.to_string());
            if state.mode == Mode::Auto
                && state.stage == Stage::Define
                && !phase_artifact_on_base(project_root, state.phase, "-CONTEXT.md", &base)
            {
                return Err(format!(
                    "phase {} has no -CONTEXT.md on `{base}` — {} cannot run the {} \
                     stage headlessly in auto mode",
                    state.phase,
                    driver.name(),
                    state.stage,
                ));
            }
            Ok(())
        }
        other => Err(format!(
            "{} declares {} as {:?} — that stage cannot run headless",
            driver.name(),
            state.stage,
            other,
        )),
    }
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
    let Ok(output) = git_command(execution_root)
        .args(["log", "--no-merges", &range, "--format=%H%x1f%B%x1e"])
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

// ---------------------------------------------------------------------------
// D-07 (35.1-03): refuse an unattended launch, before any agent time is spent,
// when the conditions that let it FINISH do not hold.
//
// Phase 35.1 makes checkpoint auto-approval possible; it does not make it
// universal. Three things each independently defeat it, silently, and each one
// turns an overnight run into a stall discovered the next morning.
// ---------------------------------------------------------------------------

/// The four states one unattended-launch prerequisite can be in.
///
/// The fourth variant is not a nicety (F-12, C3): at Define a phase genuinely
/// has no plans yet, and folding that into [`Self::Undetermined`] would refuse
/// every unattended launch ever started.
///
/// Reasons are kept SHORT on purpose. [`run_preflight`] passes the joined
/// refusal string through [`truncate_reason`] — a hard 300-character cap that
/// includes a 39-character truncation marker — and three verbose reasons would
/// not survive it. The unbounded detail lives in the printed report instead,
/// which is exactly the split [`truncate_reason`]'s own doc comment describes.
#[derive(Debug)]
enum ConditionState {
    /// Observed to hold.
    Holds,
    /// Observed NOT to hold.
    DoesNotHold(String),
    /// Could not be observed either way. Refuses, per D-07's fail-closed rule:
    /// "unreadable" is a different fact from "absent", and neither is a pass.
    Undetermined(String),
    /// Not yet determinable at this stage, and legitimately so.
    NotYetApplicable(String),
}

impl ConditionState {
    /// The report label. Refusing states are shouted so a skim of a monitor log
    /// finds them.
    fn label(&self) -> &'static str {
        match self {
            Self::Holds => "holds",
            Self::DoesNotHold(_) => "DOES NOT HOLD",
            Self::Undetermined(_) => "COULD NOT BE DETERMINED",
            Self::NotYetApplicable(_) => "not yet applicable",
        }
    }

    /// Whether this state refuses an unattended launch — exactly
    /// [`Self::DoesNotHold`] and [`Self::Undetermined`].
    fn refuses(&self) -> bool {
        matches!(self, Self::DoesNotHold(_) | Self::Undetermined(_))
    }

    /// The explanation carried by every state but [`Self::Holds`].
    fn detail(&self) -> Option<&str> {
        match self {
            Self::Holds => None,
            Self::DoesNotHold(reason)
            | Self::Undetermined(reason)
            | Self::NotYetApplicable(reason) => Some(reason),
        }
    }
}

/// D-07: whether the unattended-launch check applies to `stage` — `Define` and
/// `Code`, split out as its own pure predicate exactly the way
/// [`gh_auth_check_applies`] and [`major_bump_check_applies`] are, so "does not
/// run for a non-applicable stage" is unit-testable with no fixture at all.
///
/// `Define` is the earliest stage `devflow start` launches, so a refusal there
/// costs zero agent time. `Code` is where C3 first becomes determinable and
/// where the mechanism is actually used.
fn unattended_launch_check_applies(stage: Stage) -> bool {
    matches!(stage, Stage::Define | Stage::Code)
}

/// C1: the GSD config at the launch root exists, parses, and can hold the flag.
///
/// DevFlow cannot set the chain flag in a file it cannot read or write, and
/// GSD's `check auto-mode` reads nothing else.
///
/// Absent and unreadable are reported as DIFFERENT states on purpose. A file
/// that is not there is a different fact from a file that cannot be read, and
/// collapsing them is the "unreachable is not absent" error this project
/// already has a rule about (see [`preflight_major_bump_check`]'s D-10 branch).
///
/// T-35.1-16: no reason below embeds the absolute config path.
/// [`gsd_config::GsdConfigError::Missing`] carries a `PathBuf` and renders it,
/// so its `Display` is deliberately NOT used; the `Io` arm reports
/// [`std::io::ErrorKind`] rather than the error's own message for the same
/// reason.
fn unattended_config_condition(launch_root: &Path) -> ConditionState {
    match gsd_config::auto_chain_active(launch_root) {
        Ok(_) => ConditionState::Holds,
        Err(GsdConfigError::Missing(_)) => ConditionState::DoesNotHold(
            "no `.planning/config.json` under the launch root — nowhere for the chain flag \
             to live"
                .to_string(),
        ),
        Err(GsdConfigError::Json(err)) => ConditionState::Undetermined(format!(
            "`.planning/config.json` does not parse ({err}) — unreadable is not absent"
        )),
        Err(GsdConfigError::Io(err)) => ConditionState::Undetermined(format!(
            "`.planning/config.json` could not be read ({}) — unreadable is not absent",
            err.kind()
        )),
    }
}

/// C2: the Code stage would launch on the pipe-owning arm AND the driver has
/// a real dogfooded run (round-3 F5/D-04).
///
/// The chain-flag guard lives in a Rust process's stack frame; the legacy arm
/// is a detached shell script with no frame to hang a `Drop` on
/// (`35.1-RESEARCH.md` Pitfall 4). This turns that accepted gap from a silent
/// stall hours into an unattended run into a refusal before the first agent is
/// spawned.
///
/// Asks [`stream_launch_enabled`] — the launch path's OWN predicate —
/// rather than re-deriving the answer, so the preflight and the launch cannot
/// disagree about whether a guard will exist. `Stage::Code` is passed
/// explicitly rather than `state.stage`: the question is about the stage where
/// the mechanism is USED, not about the stage being launched right now.
///
/// **The dogfood term is the round-3 decision (F5/D-04).** Widening
/// [`stream_launch_enabled`] to Antigravity allows `--mode auto` now that
/// Phase 42 has validated Antigravity with `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS=300`.
/// Antigravity joins Claude on the stream-json unattended launch path.
fn unattended_launch_shape_condition(state: &State) -> ConditionState {
    if stream_launch_enabled(state.agent, Stage::Code, state.legacy_claude_launch)
        && (state.agent == AgentKind::Claude || state.agent == AgentKind::Antigravity)
    {
        return ConditionState::Holds;
    }
    let mut causes = Vec::new();
    if state.legacy_claude_launch {
        causes.push("the legacy launch opt-out is active".to_string());
    }
    if state.agent != AgentKind::Claude && state.agent != AgentKind::Antigravity {
        causes.push(format!(
            "the agent is `{}`, not claude or antigravity",
            state.agent
        ));
    }
    if causes.is_empty() {
        // Defensive: reachable only if `STREAM_JSON_STAGES` ever narrows to
        // exclude Code. Reporting "no cause" would be worse than saying so.
        causes.push("Code is not on the stream-json launch path".to_string());
    }
    // The reason names the CAUSE that applied, and then the consequence in
    // terms true of every cause.
    ConditionState::DoesNotHold(format!(
        "{} — the chain-flag guard binds only inside the pipe-owning monitor, which this \
         launch shape never starts",
        causes.join(" and ")
    ))
}

/// C3: no plan for this phase declares a checkpoint GSD never auto-approves.
///
/// Those two markers block in EVERY mode (`checkpoints.md` rule 6), so their
/// presence makes an unattended stall certain regardless of everything this
/// phase builds.
///
/// "No plans yet" is deliberately read differently at the two applicable
/// stages: pending at Define (the phase is not planned yet — normal), and
/// undetermined at Code (plans should exist by then, and their absence is an
/// anomaly, not a pass).
fn unattended_planned_checkpoint_condition(launch_root: &Path, state: &State) -> ConditionState {
    if verify::phase_plan_files(launch_root, state.phase).is_empty() {
        let reason = "the phase has no plan files yet".to_string();
        return if state.stage == Stage::Define {
            ConditionState::NotYetApplicable(format!("{reason} — re-evaluated at Code"))
        } else {
            ConditionState::Undetermined(format!("{reason}, but Code expects them"))
        };
    }
    if verify::phase_has_human_only_checkpoint(launch_root, state.phase) {
        return ConditionState::DoesNotHold(
            "a plan declares a `blocking-human` gate or a `human-action` checkpoint, which \
             no mode auto-approves (checkpoints.md rule 6)"
                .to_string(),
        );
    }
    ConditionState::Holds
}

/// D-07: refuse an unattended launch whose prerequisites do not definitely
/// hold, and report them either way.
///
/// **D-07 is fail-CLOSED, and warn-and-proceed was rejected rather than
/// overlooked.** Anything that is not a definite pass — including "could not be
/// determined" — refuses. A warning is read by nobody in an unattended run,
/// which is the only kind of run this check exists for.
///
/// **D-08, one path with two consequences.** The identical evaluation runs in
/// both modes and prints the identical per-condition report; only the
/// disposition differs. `Mode::Supervise` always returns `Ok(())`, so an
/// operator can rehearse an unattended run's viability without being blocked
/// out of a supervised one. Printing in both modes is the whole point of the
/// supervise arm — do not "optimize" the report behind the auto branch.
///
/// **D-09: there is no override, and its absence is a decision, not an
/// oversight.** No flag, no environment variable, no config key, and no
/// parameter beyond `(&Path, &State)` turns this refusal into a warning.
/// `state.yes_ship` authorizes the Ship gate and nothing else; that
/// non-interaction is pinned by `unattended_check_is_not_bypassed_by_yes_ship`.
/// **Both external review lanes objected to this on record** — a false-positive
/// refusal has no in-product recovery — and the operator heard the objection
/// and let the decision stand (registered as T-35.1-15, disposition `accept`).
/// If it bites in practice, reopen D-09 as a decision; do not quietly add the
/// escape hatch a later reader might assume was simply forgotten.
///
/// **What "refuses" actually does (F-15), stated rather than implied.** This
/// plugs into [`generic_preflight_checks`], so a refusal takes that framework's
/// existing disposition: the agent is never spawned ([`run_preflight`] returns
/// `Ok(false)` and `launch_stage` returns without reaching
/// `launch_stage_inner`), and the run PARKS at a named preflight gate + notify,
/// bounded by `state.preflight_retries` / [`mode::MAX_PREFLIGHT_RETRIES`],
/// after which it aborts with a logged `preflight_retry_ceiling_reached`. It is
/// NOT a process exit — Phase 17's decision 15 forbids a hard exit from this
/// framework and this check gets no exception. So the unattended run stops and
/// waits for a human rather than pushing on.
///
/// **Where the report is actually seen (F-16).** [`run_preflight`] runs from
/// `launch_stage`, which runs in the operator's own `devflow start` process for
/// the FIRST stage and in the detached `__monitor` process for every later one.
/// The Define-stage report therefore prints to the operator's TERMINAL — which
/// is what makes D-08's rehearsal useful — and the Code-stage report prints to
/// the MONITOR LOG. Nobody should later "fix" a report they could not find.
///
/// The signature is exactly `(&Path, &State) -> Result<(), String>` and must
/// stay that way (D-09, T-35.1-14): a parameter surface is where a future
/// bypass gets wired in.
fn preflight_unattended_launch_check(project_root: &Path, state: &State) -> Result<(), String> {
    unattended_launch_check_reporting_to(project_root, state, &mut std::io::stdout())
}

/// [`preflight_unattended_launch_check`]'s body, with the report's destination
/// made injectable so a test can assert on the bytes actually emitted rather
/// than on a re-derivation of what they ought to be.
///
/// **This does not weaken D-09.** The one extra parameter is a byte sink; it
/// cannot reach the disposition, which is decided entirely by
/// [`ConditionState::refuses`] and `state.mode` below. The function callers
/// actually reach, and the only one wired into
/// [`generic_preflight_checks`], keeps the fixed `(&Path, &State)` signature.
///
/// A write failure on the sink is deliberately IGNORED rather than propagated:
/// a closed stdout must not convert a viable launch into a refusal, nor a
/// refusal into a pass. The disposition is computed from the conditions, never
/// from whether the report reached anyone.
fn unattended_launch_check_reporting_to(
    project_root: &Path,
    state: &State,
    report: &mut dyn std::io::Write,
) -> Result<(), String> {
    if !unattended_launch_check_applies(state.stage) {
        return Ok(());
    }
    // The same spelling the chain-flag guard and the force-clear use: in
    // worktree mode the tracked `.planning/` the agent actually reads is the
    // WORKTREE's copy, not the main checkout's (999.76).
    let launch_root = state.worktree_path.as_deref().unwrap_or(project_root);

    let conditions = [
        (
            "GSD config can hold the chain flag",
            unattended_config_condition(launch_root),
        ),
        (
            "Code would launch on the pipe-owning arm",
            unattended_launch_shape_condition(state),
        ),
        (
            "no plan declares a human-only checkpoint",
            unattended_planned_checkpoint_condition(launch_root, state),
        ),
    ];

    // UNCONDITIONAL, and above the mode branch below on purpose (D-08).
    let _ = writeln!(
        report,
        "unattended-launch prerequisites — phase {phase}, stage {stage}, mode {mode}:",
        phase = state.phase,
        stage = state.stage,
        mode = state.mode
    );
    for (name, condition) in &conditions {
        let _ = match condition.detail() {
            Some(detail) => writeln!(report, "  [{}] {name} — {detail}", condition.label()),
            None => writeln!(report, "  [{}] {name}", condition.label()),
        };
    }

    // D-08: supervise reports and proceeds. The evaluation above already ran,
    // so the rehearsal is of the real check and not of a cheaper stand-in.
    if state.mode != Mode::Auto {
        return Ok(());
    }

    // The LABEL is carried into the refusal string, not left in the report
    // alone. "does not hold" and "could not be determined" call for different
    // operator actions — fix the condition versus go and look at why it could
    // not be observed — and the gate context is the only place some operators
    // will ever read. Keeping the distinction internal would make the
    // three-state design unobservable from outside, which is indistinguishable
    // from not having it.
    let refusals: Vec<String> = conditions
        .iter()
        .filter(|(_, condition)| condition.refuses())
        .filter_map(|(name, condition)| {
            condition
                .detail()
                .map(|detail| format!("{} [{}] — {detail}", name, condition.label()))
        })
        .collect();
    if refusals.is_empty() {
        return Ok(());
    }
    Err(format!(
        "unattended launch refused (D-07) — {}",
        refusals.join("; ")
    ))
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
pub(crate) fn generic_preflight_checks(project_root: &Path, state: &State) -> Result<(), String> {
    let mut reasons = Vec::new();
    if let Err(reason) = preflight_major_bump_check(project_root, state) {
        reasons.push(reason);
    }
    // D-07 (35.1-03) sits SECOND, and the position was chosen rather than
    // defaulted to. It can never co-occur with the major-bump or gh-auth
    // reasons — those apply only at `Stage::Ship`, and
    // `unattended_launch_check_applies` covers only Define and Code — so
    // second is effectively FIRST on every launch where this reason exists at
    // all, and `truncate_reason`'s 300-character cap cannot elide it.
    //
    // The one check it CAN co-occur with is interactivity (Codex, Auto,
    // Define), and it outranks that deliberately: interactivity reports a
    // symptom this check's C2 also reports, with more actionable detail and a
    // per-condition breakdown. Losing the shorter, more-redundant reason to
    // truncation costs less than losing this one.
    //
    // Aggregated with `if let Err`, never `?` — see this function's CR-01 doc
    // comment above. A `?` here would mean a human approving the major-bump
    // gate at some future Ship-and-Code-composing stage never sees why the
    // unattended launch was refused.
    if let Err(reason) = preflight_unattended_launch_check(project_root, state) {
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
    driver: &dyn agents::AgentDriver,
) -> Result<bool, CliError> {
    let stage = state.stage;
    if let Err(reason) =
        generic_preflight_checks(project_root, state).and_then(|()| driver.health(state))
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

    /// D-14/999.106 interactivity check: the gate is driver-driven — Codex
    /// declares Define `RequiresExistingArtifact`, so a headless Auto-mode
    /// Codex Define run without CONTEXT.md on develop is flagged. Supervise
    /// mode, a non-Define stage (Plan is un-gated — PLAN.md is an output), a
    /// non-Codex agent (Claude declares HeadlessSafe), and a CONTEXT.md that
    /// does exist are all unaffected.
    #[test]
    fn preflight_interactivity_check_flags_auto_define_without_context_md() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let mut state = State::new(
            PhaseId::new(60),
            AgentKind::Codex,
            Mode::Auto,
            root.to_path_buf(),
        );
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
            "Claude declares Define HeadlessSafe — not flagged"
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
        let mut state = State::new(
            PhaseId::new(70),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
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

        let mut state = State::new(
            PhaseId::new(71),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
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

        let mut state = State::new(
            PhaseId::new(72),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
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

        let mut state = State::new(
            PhaseId::new(73),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
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
        let _guard = env_lock();

        let (outer, worktree_path) = major_bump_worktree_fixture();
        let project_root = outer.path().join("project");

        let mut state = State::new(
            PhaseId::new(76),
            AgentKind::Claude,
            Mode::Auto,
            project_root.clone(),
        );
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
        let _guard = env_lock();
        let git_only_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", git_only_dir.path());
        }

        let dir = major_bump_fixture();
        let root = dir.path();
        commit_msg(root, "b.txt", "feat(scope)!: drop legacy api");

        let phase = PhaseId::new(74);
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

        let adapter = agents::driver_for(AgentKind::Claude);
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
        let _guard = env_lock();
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

        let phase = PhaseId::new(75);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        state.yes_ship = true;
        workflow::save_state(&state).unwrap();

        let adapter = agents::driver_for(AgentKind::Claude);
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
        let _guard = env_lock();
        let git_only_dir = git_only_path_dir_with_failing_gh();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", git_only_dir.path());
        }

        let (outer, worktree_path) = major_bump_worktree_fixture();
        let project_root = outer.path().join("project");
        let mut state = State::new(
            PhaseId::new(77),
            AgentKind::Claude,
            Mode::Auto,
            project_root.clone(),
        );
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

        let phase = PhaseId::new(61);
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

        let adapter = agents::driver_for(AgentKind::Codex);
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

        let phase = PhaseId::new(62);
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
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(63);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        // Plan is unaffected by the interactivity/gh-auth generic checks
        // (D-14) — only the injected adapter's `preflight` fails; the real
        // Claude adapter's default (Ok) preflight passes every other check.
        state.stage = Stage::Plan;
        // Premise moved off `STREAM_JSON_STAGES` membership deliberately
        // (34-06); do not "simplify" this away. The subject is the launch
        // COUNT through the Advance arm (CR-01's double-launch defect), not
        // which launch path is taken. Stage::Plan was incidentally absent
        // from the constant; 34-05's widening puts it on the stream path,
        // where `canary_gate` invokes the real `ClaudeCanaryLauncher` and the
        // launch fails on a delivery refusal unrelated to launch counting.
        // The `launches == 1` and `monitor_pid.is_some()` assertions below
        // are untouched, so a launch must still genuinely happen.
        state.legacy_claude_launch = true;
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
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(64);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        // Premise moved off `STREAM_JSON_STAGES` membership deliberately
        // (34-06); do not "simplify" this away. Same reasoning as the Advance
        // arm's sibling above: the subject is the launch COUNT through the
        // LoopBack arm, not which launch path is taken, and the `launches == 1`
        // and `monitor_pid.is_some()` assertions below are untouched.
        state.legacy_claude_launch = true;
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
    // `agents::driver_for(state.agent)`, discarding whatever adapter
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
        let _guard = env_lock();
        let original_gate_timeout = std::env::var_os("DEVFLOW_GATE_TIMEOUT_SECS");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("DEVFLOW_GATE_TIMEOUT_SECS", "2");
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(620);
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
        let _guard = env_lock();
        let original_gate_timeout = std::env::var_os("DEVFLOW_GATE_TIMEOUT_SECS");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("DEVFLOW_GATE_TIMEOUT_SECS", "2");
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(621);
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

        let phase = PhaseId::new(622);
        // Plan + Claude bypasses the generic checks and the real Claude
        // adapter's default preflight passes — the same "unaffected" shape
        // used by `run_preflight_adapter_hook_override_fires` above.
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        state.preflight_retries = 2;
        workflow::save_state(&state).unwrap();

        let adapter = agents::driver_for(AgentKind::Claude);
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
    fn reachability_fixture(
        roadmap: &str,
        phase_dir: Option<(PhaseId, &str)>,
    ) -> tempfile::TempDir {
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
            let d = root.join(format!(
                ".planning/phases/{padded}-{slug}",
                padded = phase.padded()
            ));
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(".gitkeep"), "").unwrap();
        }
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "fixture"]);
        dir
    }

    #[test]
    fn reachability_is_reachable_when_roadmap_entry_and_phase_dir_are_both_on_base() {
        let dir = reachability_fixture(
            "### Phase 24: Something\n",
            Some((PhaseId::new(24), "something")),
        );
        let root = dir.path();
        assert_eq!(
            phase_reachability_on_base(root, PhaseId::new(24), "develop"),
            PhaseReachability::Reachable
        );
        assert!(ensure_phase_reachable_on_base(root, PhaseId::new(24), "develop").is_ok());
    }

    #[test]
    fn reachability_is_unreachable_when_the_phase_dir_is_absent_from_base() {
        let dir = reachability_fixture("### Phase 24: Something\n", None);
        let root = dir.path();
        assert_eq!(
            phase_reachability_on_base(root, PhaseId::new(24), "develop"),
            PhaseReachability::Unreachable {
                roadmap_entry_found: true,
                phase_dir_found: false,
            }
        );
    }

    /// 999.63 — the named regression. The phase directory is Define's own
    /// output (`/gsd-discuss-phase N` creates it), so requiring it to run
    /// Define was a bootstrap inversion: no newly-promoted phase could ever
    /// be driven through GSD discussion mode by DevFlow.
    ///
    /// The probe still *reports* the absent directory (asserted by
    /// `reachability_is_unreachable_when_the_phase_dir_is_absent_from_base`
    /// directly above, and relied on by 27-05's hostile-`GIT_DIR` test) —
    /// this pins that the *enforcement* no longer refuses on it.
    #[test]
    fn enforcement_does_not_refuse_when_only_the_phase_dir_is_absent() {
        let dir = reachability_fixture("### Phase 24: Something\n", None);
        let root = dir.path();

        // Precondition: this is genuinely the dir-only-missing shape, not a
        // fixture that accidentally satisfies the guard some other way.
        assert_eq!(
            phase_reachability_on_base(root, PhaseId::new(24), "develop"),
            PhaseReachability::Unreachable {
                roadmap_entry_found: true,
                phase_dir_found: false,
            }
        );

        assert!(
            ensure_phase_reachable_on_base(root, PhaseId::new(24), "develop").is_ok(),
            "a present ROADMAP heading with no phase directory is the legitimate \
             bootstrap state — Define has not run yet, and running it is what \
             creates that directory. The guard must not refuse it (999.63)."
        );
    }

    /// The control for the test above: the heading half must still refuse, or
    /// 23-12's actual failure class (a phase promoted only on another branch,
    /// invisible to its own run) would regress open.
    #[test]
    fn enforcement_still_refuses_when_the_roadmap_heading_is_absent() {
        let dir = reachability_fixture(
            "### Phase 1: Something else\n",
            Some((PhaseId::new(24), "something")),
        );
        let root = dir.path();
        let err = ensure_phase_reachable_on_base(root, PhaseId::new(24), "develop")
            .expect_err("a missing ROADMAP heading must still refuse (23-12's failure class)");
        assert!(
            err.to_string().contains("### Phase 24:"),
            "the refusal must still name the missing heading, got:\n{err}"
        );
    }

    #[test]
    fn reachability_is_unreachable_when_the_roadmap_entry_is_absent_from_base() {
        let dir = reachability_fixture(
            "### Phase 1: Something else\n",
            Some((PhaseId::new(24), "something")),
        );
        let root = dir.path();
        assert_eq!(
            phase_reachability_on_base(root, PhaseId::new(24), "develop"),
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
            phase_reachability_on_base(root, PhaseId::new(24), "develop"),
            PhaseReachability::Undeterminable
        );
        assert!(
            ensure_phase_reachable_on_base(root, PhaseId::new(24), "develop").is_ok(),
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
            phase_reachability_on_base(root, PhaseId::new(24), "develop"),
            PhaseReachability::Undeterminable
        );
        assert!(ensure_phase_reachable_on_base(root, PhaseId::new(24), "develop").is_ok());
    }

    /// Pins the trailing colon's job: it stops a probe for phase 24 from
    /// matching a heading for a different phase number that merely starts
    /// with the same digits.
    #[test]
    fn phase_heading_probe_does_not_confuse_a_phase_number_prefix() {
        let dir = reachability_fixture(
            "### Phase 240: Later\n",
            Some((PhaseId::new(24), "something")),
        );
        let root = dir.path();
        assert_eq!(
            phase_reachability_on_base(root, PhaseId::new(24), "develop"),
            PhaseReachability::Unreachable {
                roadmap_entry_found: false,
                phase_dir_found: true,
            }
        );
    }

    #[test]
    fn unreachable_message_names_the_base_branch_and_each_missing_half() {
        let roadmap_missing = unreachable_message(PhaseId::new(24), "develop", false, true);
        assert!(roadmap_missing.contains("is not reachable from"));
        assert!(roadmap_missing.contains("develop"));
        assert!(roadmap_missing.contains("### Phase 24:"));
        assert!(!roadmap_missing.contains(".planning/phases/24-"));

        let dir_missing = unreachable_message(PhaseId::new(24), "develop", true, false);
        assert!(dir_missing.contains("is not reachable from"));
        assert!(dir_missing.contains("develop"));
        assert!(!dir_missing.contains("### Phase 24:"));
        assert!(dir_missing.contains(".planning/phases/24-"));

        let both_missing = unreachable_message(PhaseId::new(24), "develop", false, false);
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
        let msg = unreachable_message(PhaseId::new(24), "develop", false, false);
        assert!(!msg.contains(&fixture_root));
        assert!(!msg.contains("/home/"));
        assert!(!msg.contains("/Users/"));
    }

    // -----------------------------------------------------------------
    // 27-05 (D-01, T-27-01): phase_reachability_on_base under a hostile
    // GIT_DIR.
    // -----------------------------------------------------------------

    /// D-01/T-27-01: `phase_reachability_on_base` must resolve `project_root`
    /// even when a `GIT_DIR` inherited from the caller's environment points
    /// at an unrelated repository. The dangerous direction is a foreign
    /// repository VOUCHING for a phase that does not exist locally, letting
    /// `devflow start` fork a run with nothing to work on — a preflight that
    /// answers from a foreign repository reports success, which is worse
    /// than no preflight at all.
    ///
    /// Two proofs, mirroring `origin_main_ancestor_status_holds_under_a_
    /// hostile_git_dir` (`devflow-core/src/git.rs`, 27-01) rather than
    /// literally chaining `.env("GIT_DIR", foreign)` onto this function's
    /// OWN call (impossible from outside its module boundary — the `Command`
    /// it builds internally is not exposed to callers). Per 27-01's own
    /// empirically-verified finding, that technique would also prove nothing
    /// useful here even if it were possible: a hostile `GIT_DIR` chained
    /// AFTER a scrub genuinely redirects ref/tree resolution for commands
    /// like `ls-tree`/`rev-parse --verify` (verified directly against this
    /// machine's git — `GIT_DIR=<foreign> git ls-tree -r --name-only develop
    /// -- .planning/phases/` run with `cwd` pinned to an unrelated repo
    /// returns the FOREIGN repo's tree), unlike `--show-toplevel`, which
    /// falls back to cwd when `GIT_WORK_TREE` is unset.
    ///
    /// (a) A direct, unscrubbed reproduction of this function's own
    /// pre-migration `ls-tree` step — `std::process::Command::new("git")`
    /// (not `git_command`), cwd pinned to `real_root`, with a hostile
    /// `GIT_DIR` chained on top pointing at `foreign_root` — is shown to
    /// report `foreign_root`'s phase directory instead of `real_root`'s
    /// absence of one. This is the concrete shape of "a foreign repository
    /// vouching for a phase that does not exist locally," proven without
    /// ever touching this test process's own environment (child-scoped
    /// injection, same technique as 27-01's `hermetic_command_resolves_
    /// caller_root_even_under_a_hostile_git_dir`).
    ///
    /// (b) `phase_reachability_on_base(real_root, ...)`, called normally
    /// with nothing re-adding `GIT_DIR` afterward, is asserted to report
    /// `real_root`'s own correct answer. Run in a plain environment this
    /// passes regardless of migration status — there is no ambient hostile
    /// `GIT_DIR` for an unmigrated call to inherit. The RED-before/
    /// GREEN-after proof this plan's own `<verify>` block relies on comes
    /// from running this exact test under `GIT_DIR="$HOSTILE/.git" cargo
    /// test ... -- preflight::tests::
    /// phase_reachability_resolves_caller_root_under_a_hostile_git_dir`:
    /// before migration, the ambient hostile `GIT_DIR` set by that wrapping
    /// shell is inherited by every unscrubbed `Command::new("git")` this
    /// function spawns, so step 1 (`rev-parse --verify --quiet develop`
    /// against the shell's empty, commit-less hostile repo) fails and the
    /// function falls open to `Undeterminable` instead of the correct
    /// `Unreachable`; after migration, `git_command`'s `env_remove` strips
    /// that ambient `GIT_DIR` from every child it spawns, and the assertion
    /// below passes.
    #[test]
    fn phase_reachability_resolves_caller_root_under_a_hostile_git_dir() {
        let real_dir = reachability_fixture("### Phase 500: Something\n", None);
        let real_root = real_dir.path();

        let foreign_dir = reachability_fixture(
            "### Phase 500: Something\n",
            Some((PhaseId::new(500), "something")),
        );
        let foreign_root = foreign_dir.path();

        // (a) the vulnerability class, reproduced directly: an unscrubbed
        // Command, cwd pinned to real_root, with GIT_DIR chained onto the
        // foreign repository — the exact "vouching" danger T-27-01 closes.
        // The program name is passed via a variable, not the literal
        // `Command::new("git")` spelling, so this deliberately-unscrubbed
        // TEST reproduction is never counted by this plan's own
        // comment-filtered acceptance grep for unmigrated PRODUCTION call
        // sites (`rg -o 'Command::new\("git"\)'`).
        let git_program = "git";
        let vulnerable = std::process::Command::new(git_program)
            .args([
                "ls-tree",
                "-r",
                "--name-only",
                "develop",
                "--",
                ".planning/phases/",
            ])
            .current_dir(real_root)
            .env("GIT_DIR", foreign_root.join(".git"))
            .output()
            .expect("spawn git");
        assert!(
            vulnerable.status.success(),
            "the reproduction itself must spawn successfully: {}",
            String::from_utf8_lossy(&vulnerable.stderr)
        );
        assert!(
            String::from_utf8_lossy(&vulnerable.stdout).contains(".planning/phases/500-something/"),
            "an unscrubbed Command must be redirected onto the foreign repository's phase \
             directory by an inherited GIT_DIR — this is the vulnerability class T-27-01 closes"
        );

        // (b) the real, migrated function: called normally, resolves
        // real_root's own (correct) absence of the phase directory. See the
        // doc comment above for how this becomes RED-before/GREEN-after when
        // run under this plan's hostile-GIT_DIR-wrapped `<verify>` command.
        assert_eq!(
            phase_reachability_on_base(real_root, PhaseId::new(500), "develop"),
            PhaseReachability::Unreachable {
                roadmap_entry_found: true,
                phase_dir_found: false,
            }
        );
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

    /// A minimal repo on `branch`, config'd the same way every other small
    /// fixture in this module does — shared by the hostile-`GIT_DIR`
    /// write-containment test below, which needs several small,
    /// independent repositories rather than the remote/local clone pair
    /// [`currency_fixture`] builds.
    fn init_small_repo(root: &Path, branch: &str) {
        run_git(root, &["init", "-q", "-b", branch]);
        run_git(root, &["config", "user.email", "t@e.st"]);
        run_git(root, &["config", "user.name", "t"]);
        run_git(root, &["config", "commit.gpgsign", "false"]);
        run_git(root, &["config", "core.hooksPath", "/dev/null"]);
    }

    /// D-01/T-27-12 (critical — the only WRITE on this surface): a
    /// compare-and-swap `git update-ref` must never land in a repository the
    /// operator never named. Two proofs, mirroring the phase-reachability
    /// test above and 27-01's `origin_main_ancestor_status_holds_under_a_
    /// hostile_git_dir` (chaining hostile injection directly onto
    /// `fast_forward_base_ref`'s own internal call is not possible from
    /// outside its module boundary):
    ///
    /// (a) A direct, unscrubbed reproduction of `fast_forward_base_ref`'s
    /// own pre-migration `update-ref` argv — cwd pinned to a throwaway
    /// "real" repo, with a hostile `GIT_DIR` chained on top pointing at a
    /// throwaway "foreign" repo that happens to carry a branch of the SAME
    /// name — is shown to advance the FOREIGN repository's ref while the
    /// real repository's own ref is left untouched. This is T-27-12's exact
    /// concern: the compare-and-swap's `expected_old` guard offers no
    /// protection here, because under a hostile `GIT_DIR` it is compared
    /// against the FOREIGN repository's current value, not the real one —
    /// the environment scrub, not the compare-and-swap, is what makes the
    /// target repository correct in the first place.
    ///
    /// (b) `fast_forward_base_ref(real_root, ...)`, called normally with
    /// nothing re-adding `GIT_DIR` afterward, is asserted to advance the
    /// real repository's own ref, and a SEPARATE, unrelated foreign
    /// repository (never touched by this clean call) is asserted
    /// byte-identical before and after. Run in a plain environment this
    /// passes regardless of migration status. The RED-before/GREEN-after
    /// proof this plan's own `<verify>` block relies on comes from running
    /// this exact test under `GIT_DIR="$HOSTILE/.git" cargo test ... --
    /// preflight::tests::fast_forward_base_ref_never_writes_into_a_hostile_git_dir`:
    /// before migration, the ambient hostile `GIT_DIR` set by that wrapping
    /// shell redirects the compare-and-swap away from `real_root` entirely
    /// (the shell's throwaway hostile repo carries no matching branch, so
    /// the write fails outright against it), and `real_root`'s ref never
    /// advances to `new` — after migration, `git_command`'s `env_remove`
    /// strips that ambient `GIT_DIR` from the child, and the write
    /// correctly targets `real_root`.
    #[test]
    fn fast_forward_base_ref_never_writes_into_a_hostile_git_dir() {
        let rev_parse_in = |root: &Path, rref: &str| {
            let out = devflow_core::test_support::git_command(root)
                .args(["rev-parse", rref])
                .output()
                .unwrap();
            assert!(out.status.success(), "rev-parse {rref} in {root:?} failed");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };

        // (a) the vulnerability class, reproduced directly. `git update-ref`
        // validates its target object against whatever repository `GIT_DIR`
        // resolves to, so `demo_foreign` — not `demo_real` — needs the two
        // commits the compare-and-swap actually references. This mirrors
        // production: `ensure_base_ref_current`'s own `resolve` closure
        // (migrated by this same task) would, under a real hostile
        // `GIT_DIR`, resolve `local_sha`/`remote_sha` from that same
        // foreign repository too — the object always exists where the
        // write ultimately lands.
        let demo_real = tempfile::tempdir().unwrap();
        init_small_repo(demo_real.path(), "develop");
        std::fs::write(demo_real.path().join("a.txt"), "1").unwrap();
        run_git(demo_real.path(), &["add", "-A"]);
        run_git(demo_real.path(), &["commit", "-q", "-m", "c1"]);
        let demo_real_before = rev_parse_in(demo_real.path(), "develop");

        let demo_foreign = tempfile::tempdir().unwrap();
        init_small_repo(demo_foreign.path(), "develop");
        std::fs::write(demo_foreign.path().join("f.txt"), "1").unwrap();
        run_git(demo_foreign.path(), &["add", "-A"]);
        run_git(demo_foreign.path(), &["commit", "-q", "-m", "foreign-c1"]);
        let demo_foreign_old = rev_parse_in(demo_foreign.path(), "develop");
        std::fs::write(demo_foreign.path().join("f2.txt"), "2").unwrap();
        run_git(demo_foreign.path(), &["add", "-A"]);
        run_git(demo_foreign.path(), &["commit", "-q", "-m", "foreign-c2"]);
        let demo_foreign_new = rev_parse_in(demo_foreign.path(), "develop");
        run_git(
            demo_foreign.path(),
            &["update-ref", "refs/heads/develop", &demo_foreign_old],
        );

        let git_program = "git";
        let vulnerable = std::process::Command::new(git_program)
            .args([
                "update-ref",
                "refs/heads/develop",
                &demo_foreign_new,
                &demo_foreign_old,
            ])
            .current_dir(demo_real.path())
            .env("GIT_DIR", demo_foreign.path().join(".git"))
            .output()
            .expect("spawn git");
        assert!(
            vulnerable.status.success(),
            "the reproduction itself must succeed: {}",
            String::from_utf8_lossy(&vulnerable.stderr)
        );
        assert_eq!(
            rev_parse_in(demo_foreign.path(), "develop"),
            demo_foreign_new,
            "an unscrubbed update-ref, cwd pinned to the real repository, must still land the \
             write in the foreign repository named by GIT_DIR — the exact hazard T-27-12 closes"
        );
        assert_eq!(
            rev_parse_in(demo_real.path(), "develop"),
            demo_real_before,
            "the real repository's own ref must be untouched by the misdirected write"
        );

        // (b) the real, migrated function: called normally, advances the
        // real repository's own ref; a SEPARATE, unrelated foreign
        // repository (never touched by this clean call) is unchanged. See
        // the doc comment above for how this becomes RED-before/GREEN-after
        // when run under this plan's hostile-GIT_DIR-wrapped `<verify>`
        // command.
        let (remote, local) = currency_fixture();
        let remote_root = remote.path();
        let local_root = local.path();
        advance_remote(remote_root, "f2.txt");
        run_git(local_root, &["checkout", "-q", "-b", "other"]);
        run_git(local_root, &["fetch", "-q", "origin", "develop"]);

        let local_before = rev_parse_in(local_root, "develop");
        let remote_sha = rev_parse_in(local_root, "origin/develop");

        let foreign = tempfile::tempdir().unwrap();
        init_small_repo(foreign.path(), "develop");
        std::fs::write(foreign.path().join("x.txt"), "1").unwrap();
        run_git(foreign.path(), &["add", "-A"]);
        run_git(foreign.path(), &["commit", "-q", "-m", "unrelated"]);
        let foreign_before = rev_parse_in(foreign.path(), "develop");

        assert!(
            fast_forward_base_ref(local_root, "develop", &local_before, &remote_sha),
            "the correct expected-old value must succeed against the real repository"
        );
        assert_eq!(
            rev_parse_in(local_root, "develop"),
            remote_sha,
            "the real repository's ref must advance to `new`"
        );
        assert_eq!(
            rev_parse_in(foreign.path(), "develop"),
            foreign_before,
            "an unrelated foreign repository must be byte-identical before and after — \
             fast_forward_base_ref must never touch it"
        );
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

    /// 45-01 / AUTO-01 (REVIEWS risk 1.2). A configured base branch that has
    /// never been pushed has no `origin/<base>` ref, and that must classify
    /// as `Undeterminable` — the fail-open disposition — rather than as
    /// staleness. Pinned as a REGRESSION test: the behaviour already exists,
    /// and the risk is that a future change silently converts a local-only
    /// planning branch into a hard refusal.
    #[test]
    fn base_ref_currency_is_undeterminable_when_the_remote_ref_is_absent() {
        let (_remote, local) = currency_fixture();
        let root = local.path();

        // A local-only planning branch: no `origin/workspace/example` exists,
        // even though this clone DOES have a remote and DOES have
        // `origin/develop`.
        run_git(root, &["checkout", "-q", "-b", "workspace/example"]);

        assert_eq!(
            base_ref_currency(root, "workspace/example"),
            BaseRefCurrency::Undeterminable
        );

        // NEGATIVE CONTROL: a branch that DOES have a matching `origin/` ref
        // in the same repository classifies as `Current`. Without this the
        // test cannot distinguish "correctly detected the absence" from
        // "always returns Undeterminable".
        assert_eq!(
            base_ref_currency(root, "develop"),
            BaseRefCurrency::Current,
            "a branch with a matching origin ref must not be Undeterminable"
        );
    }

    /// The disposition half of the same contract: `Undeterminable` returns
    /// `Ok(())`, so an unpushed planning branch launches.
    #[test]
    fn ensure_base_ref_current_fails_open_for_a_local_only_planning_branch() {
        let (remote, local) = currency_fixture();
        let remote_root = remote.path();
        let root = local.path();

        run_git(root, &["checkout", "-q", "-b", "workspace/example"]);
        assert!(
            ensure_base_ref_current(root, "workspace/example").is_ok(),
            "a local-only planning branch must launch, not be refused"
        );

        // NEGATIVE CONTROL: a genuinely diverged base still returns `Err` in
        // the same repository, proving the function did not simply become
        // unconditionally permissive.
        advance_remote(remote_root, "remote-only.txt");
        run_git(root, &["checkout", "-q", "develop"]);
        std::fs::write(root.join("local-only.txt"), "x").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-q", "-m", "local-only divergent commit"]);
        assert!(
            ensure_base_ref_current(root, "develop").is_err(),
            "a diverged base must still refuse"
        );
    }

    /// The warning's TEXT, asserted where the disposition test cannot reach
    /// it. `ensure_base_ref_current` emits it through a bare `println!` with
    /// no injectable writer, so a test asserting only `Ok(())` stays green
    /// after the message is deleted or reworded — review round 2 found
    /// exactly that gap.
    #[test]
    fn undeterminable_currency_warning_names_the_branch_and_its_disposition() {
        let message = undeterminable_currency_warning("workspace/example");

        assert!(
            message.contains("workspace/example"),
            "the warning must name the base: {message}"
        );
        assert!(
            message.contains("origin/workspace/example"),
            "the warning must name the remote ref it could not compare against: {message}"
        );
        assert!(
            message.contains("fail-open"),
            "the warning must state its disposition: {message}"
        );

        // NEGATIVE CONTROL: the assertion must discriminate between the two
        // dispositions, not match any message this module produces. The
        // `Diverged` refusal is the other one.
        assert!(
            !message.contains("have diverged"),
            "the fail-open warning must not read like the Diverged refusal: {message}"
        );
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

    // -----------------------------------------------------------------
    // D-07 (35.1-03): the unattended-launch check.
    //
    // Every refusing test below carries its PASSING counterpart inside the
    // same function, reached by changing exactly one thing. A test that only
    // ever asserts `Err` cannot tell a working condition from a fixture that
    // was broken for an unrelated reason.
    // -----------------------------------------------------------------

    /// The GSD config shape a real project has, reduced to the keys that
    /// matter here. `set_auto_chain_active` must find somewhere to put the
    /// flag, so a parsing object is the whole requirement.
    const VIABLE_GSD_CONFIG: &str = "{\n  \"workflow\": {\n    \"auto_advance\": false\n  }\n}\n";

    fn write_gsd_config(root: &Path, contents: &str) {
        let dir = root.join(".planning");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), contents).unwrap();
    }

    /// A plan file for `phase`, written where `verify::phase_plan_files` looks.
    fn write_plan_for_phase(root: &Path, phase: PhaseId, body: &str) {
        let padded = phase.padded();
        let dir = root
            .join(".planning/phases")
            .join(format!("{padded}-probe"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{padded}-01-PLAN.md")), body).unwrap();
    }

    /// A wholly VIABLE fixture: a parsing GSD config, Claude with no legacy
    /// opt-out, and a plan declaring only an ordinary blocking checkpoint.
    /// Every test below starts from this and breaks exactly one thing, so a
    /// refusal is attributable to the condition under test.
    fn viable_unattended_fixture(phase: PhaseId) -> (tempfile::TempDir, State) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        write_gsd_config(&root, VIABLE_GSD_CONFIG);
        write_plan_for_phase(
            &root,
            phase,
            "---\nphase: probe\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"blocking\">\n</task>\n",
        );
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root);
        state.stage = Stage::Code;
        (dir, state)
    }

    /// D-07 scope: Define and Code only, mirroring `gh_auth_check_applies` and
    /// `major_bump_check_applies`. Stages named explicitly rather than
    /// iterated, so adding a `Stage` variant surfaces here as a compile-time
    /// prompt to decide rather than as a silently-widened set.
    #[test]
    fn unattended_check_does_not_apply_outside_define_and_code() {
        assert!(unattended_launch_check_applies(Stage::Define));
        assert!(unattended_launch_check_applies(Stage::Code));
        assert!(!unattended_launch_check_applies(Stage::Plan));
        assert!(!unattended_launch_check_applies(Stage::Validate));
        assert!(!unattended_launch_check_applies(Stage::Ship));

        // A wholly NOT-viable fixture parked at a non-applicable stage still
        // passes — the early return is what is being measured, so the fixture
        // is deliberately one that would otherwise refuse three times over.
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new(
            PhaseId::new(80),
            AgentKind::Codex,
            Mode::Auto,
            dir.path().to_path_buf(),
        );
        state.legacy_claude_launch = true;
        for stage in [Stage::Plan, Stage::Validate, Stage::Ship] {
            state.stage = stage;
            assert!(
                preflight_unattended_launch_check(dir.path(), &state).is_ok(),
                "stage {stage} must return before evaluating any condition"
            );
        }
        state.stage = Stage::Code;
        assert!(
            preflight_unattended_launch_check(dir.path(), &state).is_err(),
            "opposite-result case: the SAME fixture at an applicable stage must refuse — \
             without this, the assertions above would also pass for a check that never \
             refuses anything"
        );
    }

    /// D-08: the report prints in BOTH modes, and is asserted on the bytes the
    /// check actually emitted rather than on a re-derivation of them.
    #[test]
    fn unattended_check_reports_every_condition_in_both_modes() {
        let (dir, mut state) = viable_unattended_fixture(PhaseId::new(81));
        state.legacy_claude_launch = true; // one condition failing, two holding

        let mut auto_report = Vec::new();
        let auto = unattended_launch_check_reporting_to(dir.path(), &state, &mut auto_report);
        let auto_report = String::from_utf8(auto_report).unwrap();

        state.mode = Mode::Supervise;
        let mut supervise_report = Vec::new();
        let supervise =
            unattended_launch_check_reporting_to(dir.path(), &state, &mut supervise_report);
        let supervise_report = String::from_utf8(supervise_report).unwrap();

        assert!(auto.is_err(), "auto must refuse the failing condition");
        assert!(supervise.is_ok(), "D-08: supervise reports and proceeds");

        for report in [&auto_report, &supervise_report] {
            assert!(
                report.contains("unattended-launch prerequisites"),
                "header missing from {report}"
            );
            assert!(
                report.contains("GSD config can hold the chain flag"),
                "{report}"
            );
            assert!(
                report.contains("Code would launch on the pipe-owning arm"),
                "{report}"
            );
            assert!(
                report.contains("no plan declares a human-only checkpoint"),
                "{report}"
            );
            assert_eq!(
                report.lines().count(),
                4,
                "one header plus exactly three condition lines: {report}"
            );
        }
        assert!(
            auto_report.contains("mode auto") && supervise_report.contains("mode supervise"),
            "the reports must differ in the one field that differs, or they are not \
             evidence that both modes were exercised"
        );
    }

    /// C1, NOT-viable fixture 1 of 3. An absent GSD config refuses; creating a
    /// valid one — the single change between the two halves — passes. Without
    /// the second half, an `Err` here could equally mean the fixture was broken
    /// in some way that has nothing to do with the config.
    #[test]
    fn unattended_check_refuses_when_the_gsd_config_is_absent() {
        let (dir, state) = viable_unattended_fixture(PhaseId::new(83));
        let config = dir.path().join(".planning/config.json");
        std::fs::remove_file(&config).unwrap();

        let err = preflight_unattended_launch_check(dir.path(), &state).unwrap_err();
        assert!(err.contains("GSD config can hold the chain flag"), "{err}");
        assert!(err.contains("DOES NOT HOLD"), "{err}");

        std::fs::write(&config, VIABLE_GSD_CONFIG).unwrap();
        assert!(
            preflight_unattended_launch_check(dir.path(), &state).is_ok(),
            "restoring the config — the ONLY change — must make the same fixture pass"
        );
    }

    /// C1, and the observability of the three-state design. A malformed config
    /// is COULD NOT BE DETERMINED, not DOES NOT HOLD: unreadable and absent are
    /// different facts and the operator's next action differs. If the two
    /// collapsed to one label, the fourth `ConditionState` variant and the
    /// distinction it encodes would be unobservable from outside the module.
    #[test]
    fn unattended_check_refuses_when_the_gsd_config_is_malformed() {
        let (dir, state) = viable_unattended_fixture(PhaseId::new(84));
        let config = dir.path().join(".planning/config.json");
        std::fs::write(&config, "{ this is not json").unwrap();

        let err = preflight_unattended_launch_check(dir.path(), &state).unwrap_err();
        assert!(
            err.contains("COULD NOT BE DETERMINED"),
            "a malformed config is undetermined, not does-not-hold: {err}"
        );
        assert!(
            !err.contains("DOES NOT HOLD"),
            "the absent-file label must not be reused for an unreadable file: {err}"
        );

        std::fs::write(&config, VIABLE_GSD_CONFIG).unwrap();
        assert!(
            preflight_unattended_launch_check(dir.path(), &state).is_ok(),
            "valid JSON — the ONLY change — must make the same fixture pass"
        );
    }

    /// C2, NOT-viable fixture 2 of 3, and `35.1-RESEARCH.md` Pitfall 4's
    /// accepted gap becoming a refusal. Every arm that cannot host the
    /// chain-flag guard is walked, then the viable arm is restored.
    #[test]
    fn unattended_check_refuses_a_legacy_or_non_claude_launch_shape() {
        let (dir, mut state) = viable_unattended_fixture(PhaseId::new(85));

        state.legacy_claude_launch = true;
        let err = preflight_unattended_launch_check(dir.path(), &state).unwrap_err();
        assert!(err.contains("legacy launch opt-out"), "{err}");
        assert!(
            err.contains("Code would launch on the pipe-owning arm"),
            "{err}"
        );

        state.legacy_claude_launch = false;
        for agent in [AgentKind::Codex, AgentKind::OpenCode] {
            state.agent = agent;
            let err = preflight_unattended_launch_check(dir.path(), &state).unwrap_err();
            assert!(
                err.contains(&format!("the agent is `{agent}`")),
                "agent {agent} must be named in its own refusal: {err}"
            );
        }

        state.agent = AgentKind::Claude;
        assert!(
            preflight_unattended_launch_check(dir.path(), &state).is_ok(),
            "claude with the opt-out clear is the one viable shape and must pass"
        );
    }

    /// C3, NOT-viable fixture 3 of 3. The two halves differ only in the plan's
    /// gate value, which is what proves the check DISCRIMINATES between the
    /// checkpoint class phase 35.1 makes auto-approvable and the class no mode
    /// can approve — rather than refusing any phase that plans a checkpoint.
    #[test]
    fn unattended_check_refuses_a_phase_whose_plan_declares_a_human_only_checkpoint() {
        let phase = PhaseId::new(86);
        let (dir, state) = viable_unattended_fixture(phase);
        write_plan_for_phase(
            dir.path(),
            phase,
            "---\nphase: probe\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"blocking-human\">\n</task>\n",
        );

        let err = preflight_unattended_launch_check(dir.path(), &state).unwrap_err();
        assert!(
            err.contains("no plan declares a human-only checkpoint"),
            "{err}"
        );
        assert!(err.contains("rule 6"), "{err}");

        write_plan_for_phase(
            dir.path(),
            phase,
            "---\nphase: probe\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"blocking\">\n</task>\n",
        );
        assert!(
            preflight_unattended_launch_check(dir.path(), &state).is_ok(),
            "the ordinary blocking gate — the ONLY change — is exactly what this \
             phase makes auto-approvable and must pass"
        );
    }

    /// Without this, "refuse on anything that is not a definite pass" would
    /// refuse every launch made before the phase was planned, and the check
    /// would be unusable at the stage where refusing is cheapest.
    #[test]
    fn unattended_check_treats_an_unplanned_phase_as_pending_at_define_and_undetermined_at_code() {
        let phase = PhaseId::new(87);
        let (dir, mut state) = viable_unattended_fixture(phase);
        std::fs::remove_dir_all(dir.path().join(".planning/phases")).unwrap();

        state.stage = Stage::Define;
        assert!(
            preflight_unattended_launch_check(dir.path(), &state).is_ok(),
            "an unplanned phase at Define is pending, not failing"
        );

        state.stage = Stage::Code;
        let err = preflight_unattended_launch_check(dir.path(), &state).unwrap_err();
        assert!(
            err.contains("COULD NOT BE DETERMINED"),
            "by Code the plans should exist; their absence is an anomaly, not a pass: {err}"
        );
    }

    /// D-08. The two halves differ by EXACTLY one field assignment, which is
    /// what makes this a control rather than two unrelated cases. The fixture
    /// is the most comprehensively NOT-viable one available: all three
    /// conditions refuse.
    #[test]
    fn unattended_check_reports_but_does_not_refuse_in_supervise_mode() {
        let phase = PhaseId::new(88);
        let (dir, mut state) = viable_unattended_fixture(phase);
        std::fs::remove_file(dir.path().join(".planning/config.json")).unwrap();
        std::fs::remove_dir_all(dir.path().join(".planning/phases")).unwrap();
        state.legacy_claude_launch = true;

        let err = preflight_unattended_launch_check(dir.path(), &state).unwrap_err();
        assert!(err.contains("GSD config can hold the chain flag"), "{err}");
        assert!(
            err.contains("Code would launch on the pipe-owning arm"),
            "{err}"
        );
        assert!(
            err.contains("no plan declares a human-only checkpoint"),
            "{err}"
        );

        state.mode = Mode::Supervise; // the one and only change
        assert!(
            preflight_unattended_launch_check(dir.path(), &state).is_ok(),
            "D-08: an operator rehearsing viability must not be blocked out of a \
             supervised run by the rehearsal"
        );
    }

    /// F-13 / D-09. `state.yes_ship` authorizes the Ship gate and NOTHING else.
    /// A launch-prerequisite bypass arriving through it would be D-09's
    /// prohibition through a side door, so the non-interaction is pinned here
    /// rather than left to the reader of `run_gate`'s signature. Mirrors
    /// `run_preflight_major_bump_gate_not_auto_approved_by_yes_ship`.
    #[test]
    fn unattended_check_is_not_bypassed_by_yes_ship() {
        let (dir, mut state) = viable_unattended_fixture(PhaseId::new(89));
        std::fs::remove_file(dir.path().join(".planning/config.json")).unwrap();

        assert!(
            preflight_unattended_launch_check(dir.path(), &state).is_err(),
            "precondition: the fixture must refuse before yes_ship is involved"
        );
        state.yes_ship = true;
        assert!(
            preflight_unattended_launch_check(dir.path(), &state).is_err(),
            "yes_ship must not convert a refusal into a pass — it authorizes the \
             Ship gate and nothing else (F-13)"
        );
    }

    /// CR-01's property, extended to this check: an earlier failing check must
    /// not hide this one. That is what makes `run_preflight`'s `Advance` arm
    /// safe to skip the re-check — the human approving the gate has already
    /// been shown every applicable reason.
    ///
    /// The fixture fails BOTH: Codex at Define with no CONTEXT.md on develop
    /// trips `preflight_interactivity_check`, and Codex also trips this check's
    /// C2 (no non-Claude agent can host the chain-flag guard).
    #[test]
    fn generic_preflight_checks_surfaces_the_unattended_reason_alongside_an_earlier_failure() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        write_gsd_config(root, VIABLE_GSD_CONFIG);

        let phase = PhaseId::new(90);
        write_plan_for_phase(
            root,
            phase,
            "---\nphase: probe\n---\n\n<task type=\"auto\">\n</task>\n",
        );
        let mut state = State::new(phase, AgentKind::Codex, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Define;

        let err = generic_preflight_checks(root, &state).unwrap_err();
        assert!(
            err.contains("cannot run the define stage headlessly"),
            "the interactivity reason must survive aggregation: {err}"
        );
        assert!(
            err.contains("Code would launch on the pipe-owning arm"),
            "the unattended reason must survive aggregation: {err}"
        );

        // The unattended reason is ordered ahead of interactivity precisely so
        // the 300-character cap cannot elide it (see the placement comment in
        // `generic_preflight_checks`).
        assert!(
            truncate_reason(&err).contains("Code would launch on the pipe-owning arm"),
            "{}",
            truncate_reason(&err)
        );
    }

    /// T-35.1-16: no reason may embed the absolute config path. The refusal
    /// string reaches a persisted gate file and the operator's notification.
    #[test]
    fn unattended_refusal_reason_contains_no_absolute_path() {
        let (dir, state) = viable_unattended_fixture(PhaseId::new(82));
        std::fs::remove_file(dir.path().join(".planning/config.json")).unwrap();
        let fixture_root = dir.path().to_string_lossy().into_owned();

        let err = preflight_unattended_launch_check(dir.path(), &state).unwrap_err();
        assert!(!err.contains(&fixture_root), "{err}");
        assert!(!err.contains("/home/"), "{err}");
        assert!(!err.contains("/Users/"), "{err}");
        assert!(!err.contains("/tmp/"), "{err}");
    }

    // ------------------------------------------------------------------
    // Phase 42 Task 3 (ANTG-04, D-07): unattended `--mode auto` holds for
    // Antigravity following successful supervised dogfood run.
    // ------------------------------------------------------------------

    #[test]
    fn unattended_launch_shape_condition_antigravity_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let state = State::new(
            PhaseId::new(42),
            AgentKind::Antigravity,
            Mode::Auto,
            dir.path().to_path_buf(),
        );

        let condition = unattended_launch_shape_condition(&state);
        assert!(
            matches!(condition, ConditionState::Holds),
            "antigravity --mode auto must HOLD following dogfooding, got {condition:?}"
        );
    }

    #[test]
    fn unattended_launch_shape_condition_claude_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new(
            PhaseId::new(41),
            AgentKind::Claude,
            Mode::Auto,
            dir.path().to_path_buf(),
        );

        // Claude on the stream path holds, exactly as before the widening.
        assert!(
            matches!(
                unattended_launch_shape_condition(&state),
                ConditionState::Holds
            ),
            "claude must hold on the stream path"
        );

        // Claude under the legacy opt-out still refuses with the legacy cause.
        state.legacy_claude_launch = true;
        let ConditionState::DoesNotHold(reason) = unattended_launch_shape_condition(&state) else {
            panic!("claude + legacy opt-out must refuse");
        };
        assert!(
            reason.contains("legacy launch opt-out"),
            "the legacy cause must survive: {reason}"
        );
    }

    #[test]
    fn unattended_launch_shape_condition_non_stream_agent_refused() {
        let dir = tempfile::tempdir().unwrap();
        let state = State::new(
            PhaseId::new(42),
            AgentKind::Pi,
            Mode::Auto,
            dir.path().to_path_buf(),
        );

        let ConditionState::DoesNotHold(reason) = unattended_launch_shape_condition(&state) else {
            panic!("pi --mode auto must be refused");
        };
        assert!(
            reason.contains("not claude or antigravity"),
            "the refusal must name the non-stream cause: {reason}"
        );
        assert_eq!(
            agent_program(AgentKind::Antigravity),
            "agy",
            "the driver still resolves the agy program (unchanged path)"
        );
    }
}
