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
use devflow_core::{agents, events, workflow};
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

/// Refuse before `devflow start` forks anything when `base` is stale
/// relative to its remote. `Current`, `Ahead` and `Undeterminable` all
/// return `Ok(())` (the last with a warning — the guard fails open where it
/// cannot see, mirroring [`ensure_phase_reachable_on_base`]).
///
/// The `Behind` arm implements the operator's 2026-07-27 adjudication
/// (999.51/D-18a): fast-forward when it is safe to do so, else refuse
/// loudly. The fast-forward is attempted only when the base branch is not
/// the currently checked-out branch — `Behind` itself already establishes
/// losslessness (the local ref is a strict ancestor of the remote, so no
/// divergence and no local commits are at risk). It is performed with
/// `git update-ref refs/heads/<base> refs/remotes/<remote>/<base>`, which
/// advances the ref without touching the working tree — the reason the
/// not-checked-out precondition is sufficient. On ANY failure (checked out,
/// spawn error, non-zero exit) this falls through to the same refusal —
/// never proceed silently after a failed fast-forward.
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
            let checked_out_branch = std::process::Command::new("git")
                .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
                .current_dir(project_root)
                .output()
                .ok()
                .filter(|out| out.status.success())
                .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string());
            let is_checked_out = checked_out_branch.as_deref() == Some(base);

            if !is_checked_out {
                let fast_forwarded = std::process::Command::new("git")
                    .args([
                        "update-ref",
                        &format!("refs/heads/{base}"),
                        &format!("refs/remotes/{remote_ref}"),
                    ])
                    .current_dir(project_root)
                    .output()
                    .map(|out| out.status.success())
                    .unwrap_or(false);
                if fast_forwarded {
                    println!(
                        "advanced `{base}` to `{remote_ref}` ({count} commit(s) fast-forwarded)"
                    );
                    return Ok(());
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

/// The generic (universal) preflight checks (D-14) — the adapter-specific
/// hook is composed separately in [`run_preflight`].
fn generic_preflight_checks(project_root: &Path, state: &State) -> Result<(), String> {
    preflight_interactivity_check(project_root, state)?;
    preflight_gh_auth_check(state)
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
/// override — the check has already been adjudicated by a human, both
/// production checks (`preflight_interactivity_check`,
/// `preflight_gh_auth_check`) are deterministic idempotent predicates a
/// gate approval cannot change, so re-running them is guaranteed to fail
/// identically. The `Advance` arm therefore relaunches via
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
        let should_continue = run_preflight(root, &mut state, &adapter).unwrap();
        if should_continue {
            launch_stage(&mut state, None, None).unwrap();
        }

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

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
        let should_continue = run_preflight(root, &mut state, &adapter).unwrap();
        if should_continue {
            launch_stage(&mut state, None, None).unwrap();
        }

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

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
