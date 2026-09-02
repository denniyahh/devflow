//! Every CLI subcommand handler and the display/rendering helpers they
//! share: `start`, the gate/status/logs/history family, worktree listing,
//! recovery, and `devflow doctor`'s project-aware reconciliation core.
//!
//! D-07: this is deliberately one flat file, not a `commands/`
//! subdirectory. Mapping Phase 18's plans onto clusters showed this
//! cluster absorbed only 2 of 7 plans (pipeline absorbed 3), so a
//! per-subcommand directory buys zero measured wave reduction — and it
//! tends to re-centralise the shared display helpers this file already
//! keeps flat into a `common.rs`, recreating exactly the contention the
//! split is meant to remove.

use crate::CliError;
use crate::config_parse;
use crate::config_parse::GATE_ESCALATION_THRESHOLD_SECS;
use crate::parallel::ensure_phase_worktree;
use crate::pipeline_gate::print_dry_run;
use crate::pipeline_launch::{launch_stage, single_active_phase};
use crate::pipeline_outcomes::render_gate_context;
use crate::preflight::{
    agent_program, ensure_agent_binary, ensure_base_ref_current, ensure_phase_reachable_on_base,
};
use crate::staleness::{enforce_build_staleness, run_git_stdout};
use devflow_core::agent;
use devflow_core::agent_result;
use devflow_core::agents;
use devflow_core::config::{self, DEVELOP, FEATURE_PREFIX, MAIN};
use devflow_core::events;
use devflow_core::gates::{GateAction, GateError, GateResponse, Gates, OpenGate};
use devflow_core::git::{GitFlow, git_command, hermetic_command};
use devflow_core::history;
use devflow_core::lock;
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::recover;
use devflow_core::registry;
use devflow_core::ship_evidence;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::version;
use devflow_core::workflow;
use devflow_core::worktree;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn resolve_gate_target(
    positional: Option<String>,
    legacy_project: Option<PathBuf>,
    stage_option: Option<Stage>,
    project: PathBuf,
) -> Result<(Option<Stage>, PathBuf), CliError> {
    let Some(positional) = positional else {
        return Ok((stage_option, project));
    };
    if let Ok(positional_stage) = positional.parse::<Stage>() {
        if let Some(flagged_stage) = stage_option
            && flagged_stage != positional_stage
        {
            return Err(CliError::Message(format!(
                "conflicting stages: positional {positional_stage} and --stage {flagged_stage}"
            )));
        }
        let target = legacy_project.unwrap_or(project);
        return Ok((Some(stage_option.unwrap_or(positional_stage)), target));
    }
    if legacy_project.is_some() {
        return Err(CliError::Message(format!(
            "unsupported stage `{positional}`; expected define, plan, code, validate, or ship"
        )));
    }
    if project.as_path() != Path::new(".") {
        return Err(CliError::Message(
            "project was supplied both positionally and with --project".into(),
        ));
    }
    Ok((stage_option, PathBuf::from(positional)))
}

// ---------------------------------------------------------------------------
// start / pipeline driving
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
/// Whether phase `{NN}`'s GSD planning artifact (a `.planning/phases/{NN}-*/`
/// file ending in `suffix`, e.g. `-CONTEXT.md`) exists on `base` — the branch
/// phase worktrees fork from, resolved per project by
/// `config::base_branch` (45-01 / D-01) rather than hardcoded. A project
/// whose planning artifacts live on a planning branch was previously
/// invisible to this probe, so a `RequiresExistingArtifact` driver was
/// refused at Define for an artifact that existed.
///
/// Fail-open on git errors (base branch missing, not a repo): pre-flight must
/// never block a run the later, more specific checks would allow. That
/// fail-open is why every test of this function must assert BOTH directions —
/// `true` is also what a failed `git` invocation returns.
pub(crate) fn phase_artifact_on_base(
    project_root: &Path,
    phase: PhaseId,
    suffix: &str,
    base: &str,
) -> bool {
    let prefix = format!(".planning/phases/{padded}-", padded = phase.padded());
    let output = git_command(project_root)
        .args([
            "ls-tree",
            "-r",
            "--name-only",
            base,
            "--",
            ".planning/phases/",
        ])
        .output();
    let Ok(out) = output else { return true };
    if !out.status.success() {
        return true;
    }
    String::from_utf8_lossy(&out.stdout).lines().any(|path| {
        path.strip_prefix(&prefix)
            .is_some_and(|rest| rest.contains('/') && rest.ends_with(suffix))
    })
}

/// Verify that `base` names an existing LOCAL BRANCH in `project_root`.
///
/// Closes a bypass the pure string check in `config::validate_base_branch`
/// cannot: `preflight::phase_reachability_on_base` probes the base with
/// `git rev-parse --verify <base>`, which accepts ANY commit-ish, and
/// `worktree::add` forwards the value untouched to `git worktree add` as a
/// raw `<start_point>`. A value naming the production branch indirectly —
/// through a remote-tracking name (`origin/main`), a fully-qualified ref path
/// (`refs/heads/main`), an alias (`HEAD`), or a bare SHA — therefore
/// satisfies every other check and the "never fork from production" guard is
/// bypassable by spelling. Anchoring on `refs/heads/{base}` rejects all four
/// at once: none exists as a local branch under that spelling.
///
/// It is also the correct requirement independently of the bypass: D-01 makes
/// the base a MERGE TARGET as well as a fork point, and a merge target must
/// be a branch.
///
/// **Scoped by the caller to a non-`Default` base**, deliberately: a fresh
/// clone can legitimately have `develop` only as `origin/develop` with no
/// local branch, a case that falls open today through
/// `phase_reachability_on_base`'s `Undeterminable` arm. Applying this check
/// unconditionally would convert that fall-open into a hard refusal and
/// regress every existing project.
pub(crate) fn ensure_base_is_a_local_branch(
    project_root: &Path,
    base: &str,
) -> Result<(), CliError> {
    let ok = git_command(project_root)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{base}"),
        ])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if ok {
        return Ok(());
    }
    Err(CliError::Message(format!(
        "configured base branch `{base}` is not a local branch in this repository. \
         DevFlow forks phase worktrees from it and merges phase work back into it, so it \
         must be a branch — not a remote-tracking name, a `refs/heads/` path, `HEAD`, or a \
         commit SHA. Create it locally (e.g. `git branch {base} origin/{base}`) and re-run."
    )))
}

/// Build the fresh [`State`] a `devflow start` begins with, carrying the
/// per-phase Validate-failure total forward from any persisted state for the
/// SAME phase (999.78/A-11, D-07).
///
/// **Why exactly one field is treated differently from every other counter.**
/// `State::new` zeroes all of them, and `start()` calls it unconditionally —
/// `--force` included. A plain `State` field is therefore per-RUN, while D-07
/// specifies a per-PHASE bound, and those are different lifetimes: a bound a
/// restart resets does not bound the unattended case the bound was built for.
/// Everything else on `State` genuinely is per-run and must keep starting at
/// zero, so this copies `phase_validate_failures` and nothing else — a
/// wholesale copy would silently resurrect a stale streak, a stale baseline
/// and a stale stop point along with it.
///
/// An **absent** persisted state means zero, which is the correct reading for
/// both cases that produce it: a phase's genuine first start, and a phase
/// whose completion already cleared the file.
///
/// An **unreadable** one is a third case and does not mean zero (WR-02,
/// 35-REVIEW). A state file that exists but does not deserialize — hand-edited,
/// truncated by a full disk, or written by a future schema — carries a total
/// this function cannot see, and treating it as zero hands the phase a fresh
/// full budget silently, defeating the bound whose entire purpose is to survive
/// a restart. The total still restarts at zero, because there is nothing else
/// it could do, but the operator is told so rather than left to infer it.
///
/// **The two events that DO reset the total**, neither of which is "a new
/// process started":
///
/// 1. **Phase completion** — `finish_workflow_with_gate_timeout` calls
///    `workflow::clear_state`, deleting `.devflow/state-{NN}.json`, so the
///    next start for this phase finds nothing to carry. No code here.
/// 2. **Operator approval at the ceiling gate** — `handle_validate_outcome`
///    zeroes the total when a human advances or loops back AND
///    `mode::phase_failure_ceiling_reached` is true.
pub(crate) fn fresh_state_carrying_phase_failures(
    project_root: &Path,
    phase: PhaseId,
    agent: AgentKind,
    mode: Mode,
) -> State {
    let mut state = State::new(phase, agent, mode, project_root.to_path_buf());
    let (carried, warning) =
        carried_phase_failures(phase, workflow::load_state(project_root, phase));
    state.phase_validate_failures = carried;
    if let Some(warning) = warning {
        println!("{warning}");
    }
    state
}

/// The three-way carry-forward decision, split from its I/O so each case can
/// be asserted without reaching for a stdout capture (WR-02, 35-REVIEW).
///
/// This was `if let Ok(persisted) = load_state(..)`, which discarded every
/// `WorkflowError` identically. `load_state` returns `MissingState` for an
/// absent file and a `serde_json` error for one that exists but does not
/// deserialize, and only the first means "no failures recorded". The second
/// silently handed the phase a fresh full budget — defeating, with no operator
/// signal at all, the bound whose entire purpose is to survive a restart.
///
/// The corrupt case still yields zero, because the total it should have
/// carried is exactly what could not be read. What changes is that the
/// operator is told.
fn carried_phase_failures(
    phase: PhaseId,
    loaded: Result<State, workflow::WorkflowError>,
) -> (u32, Option<String>) {
    match loaded {
        Ok(persisted) => (persisted.phase_validate_failures, None),
        // Genuine zero: a phase's first start, or one whose completion already
        // cleared the file.
        // Genuine zero: a phase's first start, or one whose completion already
        // cleared the file.
        Err(workflow::WorkflowError::MissingState(_)) => (0, None),
        Err(err) => (
            0,
            Some(format!(
                "warning: phase {phase} state could not be read ({err}) — the per-phase \
                 Validate-failure budget restarts at zero"
            )),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start(
    project_root: &Path,
    phase: PhaseId,
    agent: AgentKind,
    mode: Mode,
    force: bool,
    worktree: bool,
    dry_run: bool,
    until: Option<Stage>,
    yes_ship: bool,
    legacy_claude_launch: bool,
) -> Result<(), CliError> {
    // 999.78/A-11: NOT a bare `State::new`. See
    // `fresh_state_carrying_phase_failures` for why one field survives a
    // forced restart and every other counter does not.
    let mut state = fresh_state_carrying_phase_failures(project_root, phase, agent, mode);
    state.stop_until = until;
    // The only assignment in the crate that ever sets `yes_ship` to a
    // non-default value. Provenance (D-12, `28-CONTEXT.md`): the typed
    // `--yes-ship` CLI flag OR a standing `yes_ship = true` in
    // `devflow.toml`, combined here with logical OR since the flag has no
    // negative form. Combined before the first `save_state` below, so the
    // persisted authorization exists before the detached monitor that will
    // later consult it is ever spawned. `run_gate_with_timeout` must never
    // re-derive this from `state` itself — see its own comment.
    let config_yes_ship = config::yes_ship(project_root);
    // D-12's compensating control: a standing default is never silent. Only
    // fires when config alone supplied the authorization — a typed flag
    // needs no explanation of where it came from.
    if config_yes_ship && !yes_ship {
        println!(
            "note: Ship gate pre-authorized by devflow.toml (yes_ship = true) — see D-12, 28-CONTEXT.md"
        );
    }
    state.yes_ship = yes_ship || config_yes_ship;

    // D-11's opt-out (31-04), combined on the same shape and for the same
    // reasons as `yes_ship` directly above: OR-ed with an environment override
    // rather than replacing it, before the first `save_state` below so the
    // persisted value exists before any detached monitor consults it, and with
    // a notice when the environment ALONE supplied it — a standing default is
    // never a silent one.
    if crate::pipeline_launch::apply_legacy_launch_opt_out(&mut state, legacy_claude_launch) {
        println!(
            "note: legacy Claude launch forced by DEVFLOW_CLAUDE_LEGACY_LAUNCH \
             (D-11, 31-CONTEXT.md) — a persisted default is never a silent one"
        );
    }

    if dry_run {
        print_dry_run(&state);
        return Ok(());
    }

    // 14-CR-05: fail on a missing agent binary BEFORE any branch/worktree is
    // scaffolded (launch_stage re-checks for the advance-time launch paths).
    ensure_agent_binary(agent_program(agent))?;

    // 45-01 / D-01 / AUTO-01: resolve the project's integration trunk once,
    // here, and use the SAME value for every guard and both fork paths below.
    // Resolved beside `yes_ship` and for the same reason — before anything
    // is spawned or mutated.
    //
    // The resolver is fail-hard on an explicitly supplied value (see its own
    // doc comment): a `main`, blank or flag-shaped base is refused here
    // rather than silently falling back to `develop`, which would make the
    // refusal unobservable for the most direct way to configure it.
    let resolved_base = config::base_branch(project_root).map_err(CliError::Message)?;
    let base = resolved_base.value.as_str();
    if resolved_base.source != config::BaseBranchSource::Default {
        // A commit-ish that is not a local branch passes `rev-parse --verify`
        // and is forwarded raw to `git worktree add` — scoped to an
        // explicitly configured base so the default path's existing
        // fall-open for a clone with no local `develop` is untouched.
        ensure_base_is_a_local_branch(project_root, base)?;
    }
    // T-45-02's compensating control, the same shape as D-12's above: a
    // standing or ambient trunk redirect is never silent.
    if base != DEVELOP {
        let source = match resolved_base.source {
            config::BaseBranchSource::Env => "DEVFLOW_BASE_BRANCH",
            config::BaseBranchSource::ConfigFile => "devflow.toml (base_branch)",
            config::BaseBranchSource::Default => "built-in default",
        };
        println!(
            "note: base branch is `{base}` (from {source}) — phase worktrees fork from it \
             and phase work merges back into it (D-01, 45-CONTEXT.md)"
        );
    }

    // 25e (999.51/D-18a): before even asking whether phase N is reachable,
    // make sure the base branch itself is current with its remote. A stale
    // base is the single most common cause of a phase heading appearing
    // absent — 999.51 recorded local `develop` sitting 21 commits behind
    // `origin/develop` while the phase heading existed only on the remote —
    // and the DANGEROUS variant is silent: a stale local `develop` that
    // still happens to carry the heading passes the reachability guard
    // below and forks a green run from stale code. Ordering here is
    // load-bearing for the same reason the comment below already gives for
    // the Codex leg: running reachability first would misdiagnose the cause
    // (base is stale) as the symptom (phase not found). `ensure_base_ref_current`
    // fast-forwards a safely-behind base and proceeds unattended, or refuses
    // loudly on divergence / an unsafe fast-forward (see that function's own
    // doc comment for the full decision).
    ensure_base_ref_current(project_root, base)?;

    // 23f (gap closure, 23-12): refuse before ANY git mutation when phase N
    // is not reachable from the resolved base — the exact branch
    // `ensure_phase_worktree` passes to `worktree::add` as `start_point`, so
    // the branch this guard inspects and the branch the run forks can never
    // disagree. That identity is now maintained by passing one resolved value
    // (45-01/D-01) rather than by both sites naming the same constant.
    // Precedes BOTH fork paths (`ensure_phase_worktree` below, and
    // `GitFlow::feature_start` in the `else` branch), and precedes the Codex
    // leg deliberately: if phase N is absent from the base entirely, "no
    // CONTEXT.md on the base" is a narrower and misleading diagnosis of the
    // same root fact.
    ensure_phase_reachable_on_base(project_root, phase, base)?;

    // 999.106 driver-driven pre-flight: whether a fresh headless run can pass
    // a stage is declared by the driver's `interactivity_mode`, not a
    // hardcoded `agent == Codex`. A `RequiresExistingArtifact` Define must
    // have CONTEXT.md on the base branch; a `RequiresExistingArtifact` Plan
    // warns without a PLAN.md. Fail in one second with instructions instead of
    // after a burned agent run and a dead-end gate. Checked on the RESOLVED
    // base (the branch worktrees fork from), so the result does not depend on
    // what the primary checkout happens to have checked out — and, since
    // 45-01, does not silently probe a branch the run will not use.
    let driver = agents::driver_for(agent);
    if driver.interactivity_mode(Stage::Define)
        == agents::InteractivityMode::RequiresExistingArtifact
        && !phase_artifact_on_base(project_root, phase, "-CONTEXT.md", base)
    {
        return Err(CliError::Message(format!(
            "phase {phase} has no CONTEXT.md on `{base}`, and {} cannot run an \
             interactive discussion headless. Run /gsd-discuss-phase {phase} \
             interactively first (any agent), or use --agent claude.",
            driver.name()
        )));
    }
    if driver.interactivity_mode(Stage::Plan) == agents::InteractivityMode::RequiresExistingArtifact
        && !phase_artifact_on_base(project_root, phase, "-PLAN.md", base)
    {
        println!(
            "warning: phase {phase} has no PLAN.md on `{base}` — headless {} \
             planning is untested and may need input; pre-writing plans is safer",
            driver.name()
        );
    }

    // Pre-start divergence check: runs on current HEAD before any git
    // mutation. WR-10 (13-REVIEW.md): only meaningful for the --no-worktree
    // (branch-in-place) flow, where `start` actually branches from the main
    // checkout's current HEAD. In worktree mode (the default) the agent's
    // work always forks fresh from the resolved base via `worktree::add`, independent
    // of whatever happens to be checked out in the main repo — checking the
    // main checkout's divergence there is unrelated to what's about to
    // happen and can either hard-fail on a stale unrelated branch or
    // silently no-op if the main checkout happens to be on develop.
    if !worktree
        && let Ok((_ahead, behind)) = GitFlow::for_project(project_root).divergence_from_develop()
    {
        if behind > 50 {
            return Err(CliError::Message(format!(
                "{base} is {behind} commits ahead — your branch is too far behind. \
                 Rebase onto {base} first, or use --force to override."
            )));
        }
        if behind > 10 {
            println!("warning: {base} is {behind} commits ahead — consider rebasing first");
        }
    }

    if worktree {
        let wt = ensure_phase_worktree(project_root, phase, force, base)?;
        println!(
            "created worktree: {} (branch {FEATURE_PREFIX}phase-{padded})",
            wt.display(),
            padded = phase.padded(),
        );
        state.worktree_path = Some(wt);
    } else {
        // Review round 2 (F3): `GitFlow::new` hardcodes the default trunk,
        // so this arm validated the configured base, checked its currency and
        // reachability, printed a note naming it — and then forked from
        // `develop` anyway. `for_project` makes both fork paths agree.
        let git = GitFlow::for_project(project_root);
        let result = if force {
            git.feature_start_force(phase)
        } else {
            git.feature_start(phase)
        };
        match result {
            Ok(branch) => println!("created feature branch: {branch}"),
            Err(err) => {
                if !force {
                    return Err(CliError::Message(format!(
                        "{err}\nUse --force to overwrite the existing branch."
                    )));
                }
                return Err(err.into());
            }
        }
    }

    // 35.1 D-01, `start`'s half: repair a leaked
    // `workflow._auto_chain_active` before anything is spawned.
    //
    // PLACEMENT IS LOAD-BEARING in both directions. It sits AFTER the
    // `if worktree { ... }` fork above, because that fork is what creates the
    // worktree and sets `state.worktree_path` — the repair targets the copy of
    // `.planning/config.json` the agent will actually read, and before the fork
    // that copy does not exist (F-10). It sits BEFORE the first
    // `workflow::save_state` below, matching this function's own
    // "combine before the first save_state" idiom for `yes_ship` and D-11's
    // opt-out.
    //
    // Why `start` needs this at all, and not just `resume`: a freshly forked
    // worktree inherits whatever `develop` carries. A leak that already reached
    // the base branch — through Ship, or any other route — arrives in every new
    // phase's worktree, and this call site is the one that catches it there.
    // See `pipeline_launch::repair_leaked_auto_chain_flag` for the D-01/D-03
    // reasoning in full. (35.1's `D-` numbers, a different sequence from phase
    // 31's D-11 cited above.)
    let launch_root: PathBuf = state
        .worktree_path
        .clone()
        .unwrap_or_else(|| project_root.to_path_buf());
    crate::pipeline_launch::repair_leaked_auto_chain_flag(
        project_root,
        &launch_root,
        phase,
        crate::pipeline_launch::AUTO_CHAIN_REPAIR_FROM_START,
    );

    // 999.79 (35-05, A-05): record what this phase's `{N}-VERIFICATION.md`
    // looked like BEFORE this run's Validate agent has had any opportunity to
    // rewrite it. `handle_validate_outcome`'s loop-back selector compares the
    // artifact against this baseline: unchanged means it was inherited from a
    // previous run and its verdict must not be reused; changed (or newly
    // present) means the Validate agent authored it during this run.
    //
    // Nothing deletes, dates or invalidates the artifact, and a
    // `devflow start --phase N --force` checks out a branch that still carries
    // the previous run's committed copy. Without this baseline that re-run —
    // mid-arc by construction — reads the inherited artifact as a verdict and
    // dispatches a `--gaps-only` pass against zero matching plans, gating
    // unresolvably.
    //
    // PLACEMENT IS LOAD-BEARING, and this is A-05's whole point: the capture
    // sits AFTER the `if worktree { ... }` fork above, where
    // `state.worktree_path` holds its final value for this run. The artifact
    // lives under the EVIDENCE ROOT, and in worktree mode that directory is
    // created by `ensure_phase_worktree` inside that fork — it does not exist
    // when `fresh_state_carrying_phase_failures` builds the state near the top
    // of this function. Capturing there would record "absent" for every
    // worktree run and make the very first freshness check read as fresh,
    // which is the failure direction this baseline exists to prevent.
    //
    // The evidence root is resolved the same way every loop-back arm resolves
    // it — the worktree path when present, the project root otherwise — and
    // NOT unconditionally as the project root. `.planning/` is tracked and the
    // Validate agent runs in the worktree, so the artifact lands on
    // `feature/phase-{N}` inside the worktree and is invisible from the main
    // checkout; reading the project root here is exactly the defect plan 33-05
    // closed (CR-01) and must not be reintroduced. Persisted by the
    // `workflow::save_state` below rather than by a second write.
    let evidence_root: PathBuf = state
        .worktree_path
        .clone()
        .unwrap_or_else(|| project_root.to_path_buf());
    state.last_verification_fingerprint =
        agent_result::phase_verification_fingerprint(&evidence_root, phase);
    // WR-06: read from the SAME evidence root in the same breath, so the pair
    // is one observation of one file rather than two readings that could
    // disagree about which artifact they saw.
    state.last_verification_mtime_nanos =
        agent_result::phase_verification_mtime_nanos(&evidence_root, phase);
    // WR-05: the observation happened. Without this flag a `None` fingerprint
    // from an old state file is indistinguishable from a `None` that means
    // "looked, found nothing", and the two demand opposite dispatches.
    state.verification_baseline_captured = true;

    // 25b (D-03): the self-dogfood build-staleness gate, hoisted here from
    // `pipeline_launch::launch_stage_inner` (17d originally placed it there,
    // re-running it before every stage launch — which meant a phase that
    // modified DevFlow's own source could never progress past the first
    // stage boundary once its own diff tripped the guard). Adjudicated
    // exactly once per run, here, so it evaluates once and the rest of the
    // pipeline never re-asks the question.
    //
    // Placement is load-bearing: this call sits AFTER `state.worktree_path`
    // is set by the `if worktree { ... }` branch above, so
    // `enforce_build_staleness` evaluates against the phase's own worktree
    // HEAD — not the main checkout, which is what would happen if this ran
    // any earlier. The block message explicitly promises the operator the
    // check runs against "this phase's WORKTREE HEAD, not the main
    // checkout"; placing the call above the worktree fork would silently
    // break that promise.
    //
    // Placement is also deliberately BEFORE `workflow::save_state` below: a
    // refusal here must never leave persisted state behind for a run that
    // is not going to start (mirroring the same "don't persist a run that
    // will never launch" reasoning `ensure_agent_binary` and
    // `ensure_phase_reachable_on_base` already follow earlier in this
    // function).
    //
    // D-04 (accepted trade, recorded here rather than hidden): this hoist
    // means `pipeline_launch::resume` (used after a rate-limit or infra
    // pause) no longer re-adjudicates staleness mid-run — a *different*
    // binary resuming a phase is never re-checked. This is accepted because
    // the scenario is already forbidden by 999.48's rejected alternatives
    // and the operator's standing 2026-07-27 position that only validated,
    // pushed code should ever drive a run. If this trade proves wrong, the
    // reversal path is a persisted `staleness_pin` field on `State`,
    // re-checked at resume — not reintroduced here.
    //
    // D-05 (not weakened): the check itself is relocated, not removed or
    // softened. `is_self_dogfood_workspace` still gates the whole module on
    // this repository's exact workspace shape and is unmodified by this
    // plan. Neither of 999.48's rejected alternatives — rebuilding the
    // driving binary mid-run, or a dogfood bypass flag — is introduced here
    // or anywhere else in this plan's diff.
    enforce_build_staleness(
        project_root,
        &state,
        env!("DEVFLOW_BUILD_COMMIT"),
        env!("DEVFLOW_BUILD_DIRTY") == "true",
    )?;

    // WR-11 (13-REVIEW.md), revised: state must be on disk BEFORE the monitor
    // exists. launch_stage spawns the detached monitor, which runs `devflow
    // advance` the moment the agent exits — and advance begins with
    // load_state. Launching first (the previous WR-11 order) raced a
    // fast-exiting agent against this save: the monitor's advance found no
    // state.json, died silently into /dev/null, and the save below then wrote
    // an in-progress state nothing would ever advance. Save first; if the
    // launch fails, clear the just-saved state so `devflow status`/`recover`
    // don't report a phantom run (the failure WR-11 originally targeted).
    workflow::save_state(&state)?;
    events::emit(
        project_root,
        phase,
        "workflow_started",
        workflow_started_payload(&state),
    );
    if let Err(err) = launch_stage(&mut state, None, None) {
        if let Err(clear_err) = workflow::clear_state(project_root, phase) {
            eprintln!("warning: could not clear state after failed launch: {clear_err}");
        }
        return Err(err);
    }
    println!(
        "started phase {} in {mode} mode at {} — monitor will auto-advance",
        state.phase, state.started_at
    );
    println!("  watch live: devflow logs -f --phase {phase}");
    Ok(())
}

// ---------------------------------------------------------------------------
// 17d: build provenance + self-dogfood staleness gate (D-17-D-21).
// ---------------------------------------------------------------------------

/// D-21: the `workflow_started` event payload, including build provenance —
/// factored out of `start()` so the payload shape is directly unit-testable
/// without spawning a real agent (`start()` calls `launch_stage` immediately
/// after emitting this event).
fn workflow_started_payload(state: &State) -> serde_json::Value {
    serde_json::json!({
        "agent": state.agent.to_string(),
        "mode": state.mode.to_string(),
        "worktree": state.worktree_path.as_ref().map(|p| p.display().to_string()),
        "version": env!("CARGO_PKG_VERSION"),
        "commit": env!("DEVFLOW_BUILD_COMMIT"),
        "dirty": env!("DEVFLOW_BUILD_DIRTY"),
        // WR-02: filename only, never the full path (leaks home dir/username
        // into OPERATIONS.md's tail-and-paste file); to_string_lossy (not
        // to_str) so non-UTF-8 names still yield a string, not null.
        "exe_path": std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())),
    })
}

// ---------------------------------------------------------------------------
// reference / cleanup / list / status / recover
// ---------------------------------------------------------------------------

/// Create or refresh the static reference worktree.
pub(crate) fn reference(
    project_root: &Path,
    branch: Option<String>,
    refresh: bool,
) -> Result<(), CliError> {
    // Project-resolved (45-01, review round 2): defaulting to the DEVELOP
    // constant snapshotted the wrong branch on a project whose trunk is
    // configured elsewhere — and then printed a message naming the branch it
    // snapshotted, so the operator was told the wrong thing confidently.
    let branch = branch.unwrap_or_else(|| config::git_flow_for_project(project_root).develop);
    let path = worktree::reference_path(project_root);

    // Detached snapshot: `branch` may already be checked out in the main
    // worktree, so we pin a detached HEAD at its tip rather than checking it out.
    if path.exists() {
        if !refresh {
            println!(
                "reference exists at {} (use --refresh to update it)",
                path.display()
            );
            return Ok(());
        }
        worktree::remove(project_root, &path, true)?;
        worktree::add_detached(project_root, &path, &branch)?;
        println!(
            "refreshed reference worktree at {} (snapshot of {branch})",
            path.display()
        );
    } else {
        worktree::add_detached(project_root, &path, &branch)?;
        println!(
            "created reference worktree at {} (snapshot of {branch})",
            path.display()
        );
    }
    Ok(())
}

/// Parse the phase number encoded in a `.worktrees/phase-NN[-agent]` path.
/// Used only as a fallback join key when no persisted `State.worktree_path`
/// matches the worktree entry (review: Codex MEDIUM — worktree->phase join).
/// Returns `None` for paths that don't follow this naming (e.g. the static
/// `reference` worktree), which correctly excludes it from the liveness
/// guard — a snapshot has no owning phase/agent to be alive.
fn phase_from_worktree_path(worktrees_dir: &Path, path: &Path) -> Option<PhaseId> {
    let name = path.strip_prefix(worktrees_dir).ok()?.to_str()?;
    let rest = name.strip_prefix("phase-")?;
    // A dot is part of the identifier (`phase-35.1`), so it must be consumed
    // here; stopping at the first non-digit would read `phase-35.1` as phase
    // 35 and join a decimal phase's worktree to its integer sibling's state.
    // The agent suffix (`phase-07-claude`) still terminates the run.
    let label: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    label.parse().ok()
}

/// Join a `git worktree list` entry to its owning phase `State`, preferring
/// the persisted `worktree_path` (set by `start`/`parallel`) and falling back
/// to worktree-directory-name or branch-name matching only when no
/// `worktree_path` match exists (review: Codex MEDIUM). Returns `None` when
/// no owning state can be found at all (e.g. the phase already shipped and
/// its state was cleared) — callers treat that as "no liveness signal",
/// not as an implicit "safe to remove."
fn state_for_worktree<'a>(
    states: &'a [State],
    worktrees_dir: &Path,
    wt: &worktree::WorktreeInfo,
) -> Option<&'a State> {
    if let Some(state) = states
        .iter()
        .find(|s| s.worktree_path.as_deref() == Some(wt.path.as_path()))
    {
        return Some(state);
    }
    if let Some(phase) = phase_from_worktree_path(worktrees_dir, &wt.path)
        && let Some(state) = states.iter().find(|s| s.phase == phase)
    {
        return Some(state);
    }
    if let Some(branch) = &wt.branch {
        return states
            .iter()
            .find(|s| *branch == format!("{FEATURE_PREFIX}phase-{}", s.phase.padded()));
    }
    None
}

/// Bounded-backoff retry around `worktree::remove`, absorbing the transient
/// `Directory not empty` race that can occur even after a phase is confirmed
/// dead (a lingering fd/writer from the just-exited agent). NOT a substitute
/// for the liveness guard above — only reached once a phase is confirmed
/// dead (agent dead AND monitor not active). `git worktree prune` is
/// deliberately not used here: it only clears metadata for already-absent
/// directories and would orphan leftover files on disk (Pitfall 3).
fn remove_worktree_with_retry(
    project_root: &Path,
    path: &Path,
    force: bool,
) -> Result<(), worktree::WorktreeError> {
    const ATTEMPTS: u32 = 3;
    const BASE_DELAY_MS: u64 = 50;
    let mut last_err = None;
    for attempt in 0..ATTEMPTS {
        match worktree::remove(project_root, path, force) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = Some(err);
                if attempt + 1 < ATTEMPTS {
                    std::thread::sleep(std::time::Duration::from_millis(
                        BASE_DELAY_MS * 2u64.pow(attempt),
                    ));
                }
            }
        }
    }
    Err(last_err.expect("loop runs ATTEMPTS >= 1 times"))
}

/// Remove phase worktrees (and the reference with --force), deleting their
/// associated feature branches, then prune and clean up merged branches.
///
/// Hard-refuses (D-06, no override flag) removal of any worktree whose owning
/// phase has a live agent (any monitor state, including Unknown/Stuck) or an
/// active monitor (Healthy/BetweenStages) — closing the race where a real
/// `cleanup --force` run could delete a worktree a live agent/monitor is
/// still writing into (review: Codex HIGH, fail-closed on a live agent).
pub(crate) fn cleanup(project_root: &Path, force: bool) -> Result<(), CliError> {
    // Project-resolved (45-01): `cleanup_merged` computes "merged" relative
    // to the trunk and treats it as protected — both must be the configured
    // one, or this sweep can delete a branch that was never merged.
    let git = GitFlow::for_project(project_root);
    let worktrees_dir = worktree::worktrees_dir(project_root);
    let reference = worktree::reference_path(project_root);
    let states = workflow::list_states(project_root);

    let worktrees = worktree::list(project_root)?;
    let mut removed = 0usize;
    for wt in &worktrees {
        // Only touch worktrees under `.worktrees/` (never the main checkout).
        if !wt.path.starts_with(&worktrees_dir) {
            continue;
        }
        if wt.path == reference && !force {
            println!("keeping reference worktree (use --force to remove it)");
            continue;
        }

        let matched_state = state_for_worktree(&states, &worktrees_dir, wt);
        let phase = matched_state
            .map(|s| s.phase)
            .or_else(|| phase_from_worktree_path(&worktrees_dir, &wt.path));
        let agent_alive = phase
            .and_then(|p| agent_pid_from_file(project_root, p))
            .is_some_and(agent::agent_running);
        let monitor_pid = matched_state.and_then(|s| s.monitor_pid);
        let monitor_alive = monitor_pid.is_some_and(agent::agent_running);
        let phase_liveness = liveness(monitor_pid, monitor_alive, agent_alive);

        // A phase halted via `devflow start --until <stage>` (20c) clears
        // `monitor_pid` and its agent has already exited by design — that
        // reads as `Liveness::Unknown` with `agent_alive == false`, which
        // would otherwise sail straight through the live-agent refusal
        // below. Treat it the same way `doctor`'s `check_dead_agent`/
        // `check_dead_monitor` were taught about `facts.stopped` in this
        // same phase: an intentionally-parked worktree is never implicitly
        // safe to remove — require `--force`, mirroring the `reference`
        // worktree's own precedent above.
        let stopped = matched_state.is_some_and(|s| s.stopped);
        if stopped && !force {
            let phase_label = phase
                .map(|p| p.to_string())
                .unwrap_or_else(|| "?".to_string());
            println!(
                "keeping worktree {} for phase {phase_label} — halted via --until; run `devflow resume --phase {phase_label}` first, or pass --force to discard it",
                wt.path.display()
            );
            continue;
        }

        // Fail-closed on a live agent: refuse whenever the agent is alive
        // (regardless of monitor liveness — Unknown/Stuck included) OR the
        // monitor is actively running the stage (Healthy/BetweenStages).
        // Only Stuck/Unknown WITHOUT a live agent proceeds.
        if agent_alive || matches!(phase_liveness, Liveness::Healthy | Liveness::BetweenStages) {
            let phase_label = phase
                .map(|p| p.to_string())
                .unwrap_or_else(|| "?".to_string());
            return Err(CliError::Message(format!(
                "refusing to remove worktree {} for phase {phase_label} ({}) — run `devflow resume --phase {phase_label}` or wait for it to finish",
                wt.path.display(),
                phase_liveness.describe(),
            )));
        }

        match remove_worktree_with_retry(project_root, &wt.path, force) {
            Ok(()) => {
                print!("removed worktree {}", wt.path.display());
                match &wt.branch {
                    Some(branch) if branch.starts_with(FEATURE_PREFIX) => {
                        match git.delete_branch(branch, force) {
                            Ok(()) => println!(" + deleted branch {branch}"),
                            Err(err) => println!(" (branch {branch} kept: {err})"),
                        }
                    }
                    _ => println!(),
                }
                removed += 1;
            }
            Err(err) => {
                println!(
                    "warning: could not remove worktree {} after retrying — manually delete this directory: {err}",
                    wt.path.display()
                );
            }
        }
    }

    worktree::prune(project_root)?;
    if removed == 0 {
        println!("no worktrees to clean up");
    }
    match git.cleanup_merged() {
        Ok(merged) => {
            for branch in merged {
                println!("deleted merged branch {branch}");
            }
        }
        Err(err) => println!("warning: could not prune merged branches: {err}"),
    }
    Ok(())
}

/// A phase's monitor/agent liveness, distinguishing a dead monitor (nothing
/// will call `devflow advance` when the agent exits) from a normal
/// between-stages moment (18b — "who watches the watcher").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Liveness {
    /// Monitor and agent are both alive — the stage is actively running.
    Healthy,
    /// Monitor is alive, agent has exited — normal between-stages moment;
    /// the monitor will advance the phase shortly.
    BetweenStages,
    /// The recorded monitor is dead. Whether or not the agent is also dead,
    /// nothing will call `devflow advance` for this phase — it needs a
    /// manual `devflow resume`.
    Stuck,
    /// No monitor PID has been recorded for this state — either none has
    /// been spawned yet, or the state was written by a binary predating
    /// this field. Never reported as a problem.
    Unknown,
}

impl Liveness {
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Liveness::Healthy => "healthy",
            Liveness::BetweenStages => "between stages",
            Liveness::Stuck => "stuck — needs devflow resume",
            Liveness::Unknown => "unknown (no monitor recorded)",
        }
    }
}

/// Pure liveness predicate — no I/O. `monitor_pid` is matched `None` first
/// so a state written by a pre-18b binary (carrying no `monitor_pid`) can
/// never be misclassified as `Stuck` (T-18-11).
fn liveness(monitor_pid: Option<u32>, monitor_alive: bool, agent_alive: bool) -> Liveness {
    match monitor_pid {
        None => Liveness::Unknown,
        Some(_) => match (monitor_alive, agent_alive) {
            (true, true) => Liveness::Healthy,
            (true, false) => Liveness::BetweenStages,
            (false, _) => Liveness::Stuck,
        },
    }
}

/// Recovery verbs discoverable from a phase's liveness (21a, D-03) — the
/// pure, testable counterpart to `status`'s old inline Stuck `println!`.
/// Always includes `devflow resume` for `Stuck`; additionally includes
/// `devflow advance` when the phase is gate-pending (the operator answers
/// the gate then advances — the primary footgun this closes; widening the
/// predicate further risks suggesting `advance` where nothing proves it is
/// right, per 21-CONTEXT.md's Review Incorporation). Empty for any other
/// liveness, so a healthy/between-stages/unknown phase prints nothing new.
fn recovery_hints(state: &State, liveness: Liveness) -> Vec<String> {
    if liveness != Liveness::Stuck {
        return Vec::new();
    }
    let mut hints = vec![format!("devflow resume --phase {}", state.phase)];
    if state.gate_pending {
        hints.push(format!("devflow advance --phase {}", state.phase));
    }
    hints
}

/// `status`'s in-stage progress line: real elapsed time since the phase's
/// most recent `stage_launched` event. `None` (no such event yet) renders
/// the stage name with no age, rather than mislabeling phase age as stage
/// age — never pass `state.started_at`'s age here (3/3 review MEDIUM).
fn render_stage_progress_line(stage: Stage, stage_launched_ts: Option<u64>) -> String {
    match stage_launched_ts {
        Some(ts) => format!(
            "  in stage {stage}: {}",
            recover::format_age(&ts.to_string())
        ),
        None => format!("  in stage {stage}"),
    }
}

pub(crate) fn status(project_root: &Path) -> Result<(), CliError> {
    // 13-DEFERRED-CR-03 acceptance: enumerate every active phase, not just
    // the last one started.
    let states = workflow::list_states(project_root);
    let mut current_worktree: Option<PathBuf> = None;
    if states.is_empty() {
        println!("stage: idle");
        println!("project_root: {}", project_root.display());
    } else {
        // 14-CR-10: one pass over events.jsonl for every phase's last event,
        // instead of a full-file scan per phase.
        let mut last_events = events::last_events_by_phase(project_root);
        println!("project_root: {}", project_root.display());
        println!(
            "active phases: {}",
            states
                .iter()
                .map(|s| s.phase.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        for state in &states {
            let gate = if state.gate_pending {
                "pending"
            } else {
                "none"
            };
            println!("\nphase {}:", state.phase);
            println!(
                "  stage: {} | mode: {} | gate: {}",
                state.stage, state.mode, gate
            );
            println!("  agent: {}", agents::driver_for(state.agent).name());
            if state.consecutive_failures > 0 {
                println!("  validate failures: {}", state.consecutive_failures);
            }
            println!(
                "  started: {} ({})",
                state.started_at,
                recover::format_age(&state.started_at)
            );
            if let Some(ref wt) = state.worktree_path {
                println!("  worktree: {}", wt.display());
            }
            current_worktree = current_worktree.or_else(|| state.worktree_path.clone());
            let agent_pid = agent_pid_from_file(project_root, state.phase);
            match agent_pid {
                Some(pid) => {
                    println!(
                        "  agent_pid: {pid} (running: {})",
                        agent::agent_running(pid)
                    );
                }
                None => println!("  agent_pid: none"),
            }
            match state.monitor_pid {
                Some(pid) => {
                    println!(
                        "  monitor_pid: {pid} (running: {})",
                        agent::agent_running(pid)
                    );
                }
                None => println!("  monitor_pid: none"),
            }
            let agent_alive = agent_pid.is_some_and(agent::agent_running);
            let monitor_alive = state.monitor_pid.is_some_and(agent::agent_running);
            let phase_liveness = liveness(state.monitor_pid, monitor_alive, agent_alive);
            println!("  liveness: {}", phase_liveness.describe());
            let phase_summary = last_events.remove(&state.phase);
            println!(
                "{}",
                render_stage_progress_line(
                    state.stage,
                    phase_summary.as_ref().and_then(|s| s.stage_launched_ts)
                )
            );
            for hint in recovery_hints(state, phase_liveness) {
                println!("    → {hint}");
            }
            if let Some(summary) = phase_summary {
                let ago = summary
                    .event
                    .get("ts")
                    .and_then(|t| t.as_u64())
                    .map(|t| format!(" ({})", recover::format_age(&t.to_string())))
                    .unwrap_or_default();
                println!("  last action: {}{ago}", events::describe(&summary.event));
            }
        }
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if let Some(banner) = render_pending_gate_banner(&Gates::list_open(project_root), now) {
        println!("\n{banner}");
    }
    print_open_branches(project_root);
    print_worktrees(project_root, current_worktree.as_deref());
    for hint in cron_instruction_hints(project_root) {
        println!("\n{hint}");
    }
    Ok(())
}

/// Report DevFlow's own structural record of whether `phase` shipped
/// (23-06) — reads `devflow_core::ship_evidence::collect`, a strictly
/// read-only oracle sourced from the append-only event log, never from an
/// agent-authored attestation document.
///
/// **Three dead-end predicates/placements this plan disproved. Documented
/// here so the next person reading this code does not re-derive and
/// re-attempt them:**
///
/// 1. **Pre-gate merge check.** `hooks_after_ship` (`hooks.rs:105-112`) only
///    runs from `finish_workflow_with_gate_timeout`, itself only reached from
///    `handle_ship_outcome`'s `GateAction::Advance` arm — i.e. AFTER the Ship
///    gate has already been approved. A check for a merge/tag/push placed
///    before gate approval would fail for every legitimate Ship; RESEARCH.md's
///    Question C / Pattern 3 recommendation to place it there was verified
///    wrong against live source.
/// 2. **Post-batch ancestry check.** `BranchCleanup` runs immediately after
///    `Merge` in that same `hooks_after_ship` batch and deletes the feature
///    branch. `GitFlow::is_merged_into_develop` fails closed on an absent
///    branch (`git.rs:89-97`: "an absent branch is not proof of a merge"), so
///    an ancestry check run after the batch completes returns `false` for
///    EVERY successfully shipped phase.
/// 3. **`workflow_finished` as the shipped predicate.** Emitted at TWO
///    sites: real Ship finalization, and `transition`'s `devflow start
///    --until <stage>` clean-stop branch
///    (`crates/devflow-cli/src/pipeline_gate.rs:67`, the
///    `state.stop_until == Some(from)` arm near the top of `transition`),
///    which returns with payload `{"reason": "stopped_at", …}` BEFORE any
///    checkout hook, before `state.stage = to`, before the `"transition"`
///    event, and before `launch_stage` runs — nothing resembling a Ship has
///    happened. A phase halted after one stage would read as shipped. This
///    is the dead end most likely to be reintroduced: a cross-AI review
///    caught it after it had already been written into this plan three
///    times as "the only site emitting `workflow_finished`."
pub(crate) fn evidence(
    project_root: &Path,
    phase: PhaseId,
    json: bool,
    require_shipped: bool,
) -> Result<(), CliError> {
    let evidence = ship_evidence::collect(project_root, phase);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&evidence).expect("ShipEvidence must serialize")
        );
    } else {
        println!("phase: {}", evidence.phase);
        println!("shipped: {}", evidence.shipped);
        println!(
            "workflow_finished_seen: {}",
            evidence.workflow_finished_seen
        );
        println!(
            "finished_reason: {}",
            evidence.finished_reason.as_deref().unwrap_or("none")
        );
        println!(
            "stage: {}",
            evidence
                .stage
                .map(|s| s.to_string())
                .unwrap_or_else(|| "none".into())
        );
        println!("state_present: {}", evidence.state_present);
        println!("feature_branch_exists: {}", evidence.feature_branch_exists);
        println!("merged_into_develop: {}", evidence.merged_into_develop);
        println!("has_remote: {}", evidence.has_remote);
    }

    if require_shipped && !evidence.shipped {
        // "It finished but it did not ship" is the confusing case a reader
        // hits first — say so explicitly rather than a generic "not shipped"
        // (Task 1 acceptance criteria).
        let detail = if ship_evidence::is_stopped_at(&evidence) {
            format!(
                "phase {phase} has not shipped — DevFlow's own record shows it stopped after \
                 one stage (--until) rather than reaching a finalized Ship"
            )
        } else {
            format!("phase {phase} has not shipped — DevFlow has no record of a completed Ship")
        };
        return Err(CliError::Message(detail));
    }

    Ok(())
}

/// Build the persistent status-side signal for gates awaiting an operator.
/// Context is agent-controlled, so it must use the same bounded rendering as
/// gate notifications and failure events.
fn render_pending_gate_banner(open: &[OpenGate], now: u64) -> Option<String> {
    if open.is_empty() {
        return None;
    }

    let mut banner = String::from("==================== PENDING GATE ====================\n");
    for gate in open {
        let timestamp = gate.timestamp.parse::<u64>().ok();
        let escalated = timestamp
            .and_then(|timestamp| now.checked_sub(timestamp))
            .is_some_and(|age| age >= GATE_ESCALATION_THRESHOLD_SECS);
        let marker = if escalated { "!!! ESCALATED" } else { "!!!" };
        let context = render_gate_context(&gate.context, 300);
        let stage = gate.stage.to_string();
        banner.push_str(&format!(
            "{marker}: phase {} {stage} ({})\n  {context}\n  approve: devflow gate approve {} --stage {stage}\n  reject:  devflow gate reject {} --stage {stage} --note <reason>\n",
            gate.phase,
            recover::format_age(&gate.timestamp),
            gate.phase,
            gate.phase,
        ));
    }
    banner.push_str("======================================================");
    Some(banner)
}

/// List every gate awaiting a human response. When `all_roots` is set,
/// answers "what is gated on this machine?" across every root this machine
/// has registered (`registry::load_roots`) in one invocation, with a
/// leading ROOT column and a per-gate age; behaviour is otherwise
/// byte-identical to the single-root listing (23-03).
pub(crate) fn gate_list(project_root: &Path, all_roots: bool) -> Result<(), CliError> {
    if all_roots {
        return gate_list_all_roots();
    }
    let open = Gates::list_open(project_root);
    if open.is_empty() {
        println!("no open gates");
        return Ok(());
    }
    println!("{:<6} {:<9} {:<9} CONTEXT", "PHASE", "STAGE", "AGE");
    for gate in &open {
        let context = render_gate_context(&gate.context, 100);
        println!(
            "{:<6} {:<9} {:<9} {context}",
            gate.phase,
            gate.stage.to_string(),
            recover::format_age(&gate.timestamp),
        );
    }
    println!(
        "\nanswer with: devflow gate approve <phase> [--note ...] | \
         devflow gate reject <phase> --note ... (note with \"abort\" ends the phase)"
    );
    Ok(())
}

/// The `--all-roots` half of [`gate_list`]: fan out `Gates::list_open`
/// across every registered root and render an additional leading ROOT
/// column. A registered root with no open gates simply contributes no rows.
fn gate_list_all_roots() -> Result<(), CliError> {
    let roots = registry::load_roots();
    let mut rows: Vec<(PathBuf, OpenGate)> = Vec::new();
    for root in &roots {
        for gate in Gates::list_open(&root.project_root) {
            rows.push((root.project_root.clone(), gate));
        }
    }
    if rows.is_empty() {
        println!("no open gates across {} registered root(s)", roots.len());
        return Ok(());
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    println!("{:<6} {:<9} {:<9} ROOT / CONTEXT", "PHASE", "STAGE", "AGE");
    for (root, gate) in &rows {
        println!("{}", render_all_roots_gate_row(root, gate, now));
    }
    Ok(())
}

/// Render one `--all-roots` gate row: the PHASE/STAGE/AGE/ROOT line plus a
/// second, indented context line. Pure given `now` — the same pattern
/// `render_pending_gate_banner` already uses — so it's unit-testable
/// without mutating wall-clock time.
fn render_all_roots_gate_row(root: &Path, gate: &OpenGate, now: u64) -> String {
    let context = render_gate_context(&gate.context, 100);
    format!(
        "{:<6} {:<9} {:<9} {}\n           {context}",
        gate.phase,
        gate.stage.to_string(),
        render_gate_age(&gate.timestamp, now),
        root.display(),
    )
}

/// Render a gate's `timestamp` as a compact age for `--all-roots`, with a
/// trailing urgency marker once it reaches
/// [`GATE_ESCALATION_THRESHOLD_SECS`] — reusing the same threshold
/// `render_pending_gate_banner` escalates on, rather than inventing a
/// second one. A `timestamp` that does not parse as `u64`, or that is
/// somehow in the future relative to `now`, renders `?` — the row is still
/// listed, matching the forensics record that dropping unusual rows is
/// exactly how the orphan population stayed invisible.
fn render_gate_age(timestamp: &str, now: u64) -> String {
    let Ok(ts) = timestamp.parse::<u64>() else {
        return "?".to_string();
    };
    let Some(age) = now.checked_sub(ts) else {
        return "?".to_string();
    };
    let compact = match age {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86400),
    };
    if age >= GATE_ESCALATION_THRESHOLD_SECS {
        format!("{compact}!")
    } else {
        compact
    }
}

/// Answer an open gate from the CLI — the dogfood-facing replacement for
/// hand-writing `.devflow/gates/NN-stage.response.json` (15a).
/// Resolve an omitted `--stage` to the single open gate for `phase` from an
/// already-fetched `Gates::list_open` collection — the shared no-open /
/// single-open / ambiguous-open behavior `gate_respond` and `gate_show` both
/// need, kept in one place so it cannot drift between them (Phase 21 review
/// WR-01). Callers own the `Gates::list_open` read so `gate_show` can resolve
/// and select from one fetched collection instead of reading twice (WR-03).
fn resolve_single_open_gate_stage(open: &[OpenGate], phase: PhaseId) -> Result<Stage, CliError> {
    let matching: Vec<&OpenGate> = open.iter().filter(|g| g.phase == phase).collect();
    match matching.as_slice() {
        [] => Err(CliError::Message(format!(
            "no open gate for phase {phase} — see `devflow gate list`"
        ))),
        [one] => Ok(one.stage),
        many => Err(CliError::Message(format!(
            "phase {phase} has several open gates ({}) — pass --stage",
            many.iter()
                .map(|g| g.stage.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

pub(crate) fn gate_respond(
    project_root: &Path,
    phase: PhaseId,
    stage: Option<Stage>,
    approved: bool,
    note: Option<String>,
) -> Result<(), CliError> {
    let stage = match stage {
        Some(stage) => stage,
        None => resolve_single_open_gate_stage(&Gates::list_open(project_root), phase)?,
    };
    let responded_by = std::env::var("USER")
        .ok()
        .filter(|user| !user.is_empty())
        .unwrap_or_else(|| "devflow-cli".into());
    let response = GateResponse {
        approved,
        note,
        responded_by: Some(responded_by),
    };
    let path = Gates::respond(project_root, phase, stage, &response)?;
    events::emit(
        project_root,
        phase,
        "gate_response_written",
        serde_json::json!({
            "stage": stage.to_string(),
            "approved": approved,
            "via": "cli",
        }),
    );
    let outcome = match GateAction::from_response(&response) {
        GateAction::Advance => "workflow will advance",
        GateAction::LoopBack(_) => "workflow will loop back to Code",
        GateAction::Abort(_) => "phase will abort",
    };
    println!(
        "{} gate for phase {phase} {stage} — {outcome} once the waiting monitor polls it \
         (response at {})",
        if approved { "approved" } else { "rejected" },
        path.display()
    );
    Ok(())
}

/// Answer or report every aged, unattended gate across every registered root
/// (or a single `root`, when given) — the acting half of 23b's bound gate
/// lifetime. Never signals any process; `Gates::reap` (structurally
/// incapable of approving, T-23-41) is the ONLY write path, so the sweep's
/// sole effect on the world is written bytes a live poller was already
/// watching for.
///
/// `GateError::NoOpenGate`/`AlreadyResponded` are treated as benign races —
/// a human or `--yes-ship` may have answered the same gate between
/// `list_open` and `reap` — and counted as skipped rather than returned as
/// an error; `Gates::respond`'s own refusal to clobber an unconsumed
/// response is the first-writer-wins resolution that makes this safe with
/// no new coordination code. Every successful reap emits `gate_reaped` in
/// this process (the audit trail half — `run_gate_with_timeout`'s own
/// `gate_resolved` fires independently in the target process when it picks
/// the response up).
pub(crate) fn gate_sweep(
    max_age_secs: Option<u64>,
    dry_run: bool,
    root: Option<PathBuf>,
    reap_strays: bool,
) -> Result<(), CliError> {
    let threshold = max_age_secs.unwrap_or_else(config_parse::gate_max_unattended_age_secs);

    // Cloned before the `match` below moves `root` into `roots` — the stray
    // pass (below) needs it too, unioned into the machine-wide reachable-pid
    // safety set rather than substituted for it (CR-01, 25-15).
    let explicit_root: Vec<PathBuf> = root.clone().into_iter().collect();

    let roots: Vec<PathBuf> = match root {
        Some(root) => vec![root],
        None => {
            registry::prune_missing();
            registry::load_roots()
                .into_iter()
                .map(|r| r.project_root)
                .collect()
        }
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut reaped = 0u32;
    let mut skipped = 0u32;
    let mut left_alone = 0u32;

    for project_root in &roots {
        for gate in Gates::list_open(project_root) {
            let Ok(ts) = gate.timestamp.parse::<u64>() else {
                left_alone += 1;
                continue;
            };
            let age = now.saturating_sub(ts);
            if age < threshold {
                left_alone += 1;
                continue;
            }
            if dry_run {
                reaped += 1;
                println!(
                    "would reap phase {} {} (age {age}s) at {}",
                    gate.phase,
                    gate.stage,
                    project_root.display()
                );
                continue;
            }
            match Gates::reap(
                project_root,
                gate.phase,
                gate.stage,
                "abort: reaped by devflow gate sweep (unattended gate exceeded max age)",
                "devflow-reap",
            ) {
                Ok(_) => {
                    reaped += 1;
                    events::emit(
                        project_root,
                        gate.phase,
                        "gate_reaped",
                        serde_json::json!({
                            "stage": gate.stage.to_string(),
                            "age_secs": age,
                            "max_age_secs": threshold,
                        }),
                    );
                    println!(
                        "reaped phase {} {} (age {age}s) at {}",
                        gate.phase,
                        gate.stage,
                        project_root.display()
                    );
                }
                Err(err) => {
                    skipped += 1;
                    println!(
                        "skipped phase {} {} at {} — already answered ({err})",
                        gate.phase,
                        gate.stage,
                        project_root.display()
                    );
                }
            }
        }
    }

    // 999.44/DEN-68: opt-in only. Registry-independent, so this pass is NOT
    // scoped to `roots` above — a stray by definition has no project root
    // for any registry entry, lock file, or state file to name, which is
    // exactly why the census above cannot see it either. Extends the SAME
    // reaped/skipped/left-alone counters and dry-run summary line rather
    // than printing a second, separate report.
    //
    // CR-01/25-15: the SAFETY half is likewise never narrowed by `--root`.
    // `unreachable_stray_candidates` unions `explicit_root` into the
    // machine-wide reachable-pid set rather than substituting it, because
    // that set is what gets PROTECTED, not what gets acted on — scoping it
    // to one root would leave every OTHER root's live processes unprotected
    // while this pass still reaps machine-wide, a strictly larger blast
    // radius than leaving it machine-wide (see `25-15-PLAN.md`'s
    // `<resolved_decision>`).
    if reap_strays {
        if !explicit_root.is_empty() {
            println!(
                "note: --root does not scope this stray pass -- a stray has no project root \
                 for any registry entry, lock file, or state file to name, so discovery and \
                 reaping are always machine-wide; the reachability safety filter is also \
                 computed across every registered root regardless of --root, deliberately, \
                 because narrowing it would leave other projects' live processes unprotected"
            );
        }
        let candidates = unreachable_stray_candidates(&explicit_root);
        let results = reap_stray_candidates(&candidates, dry_run, agent::STRAY_MIN_AGE);
        // The event's natural home: the explicit `--root`, when given
        // (exactly 999.44's own reproduction shape — an operator who
        // already knows which root's stray they're chasing), else the
        // first registered root, arbitrarily but deterministically, so the
        // audit trail lands somewhere tailable rather than nowhere. A
        // stray has no root of its own to prefer instead.
        let event_root = roots.first();
        for result in &results {
            let layer = stray_layer_label(result.layer);
            match result.outcome {
                StrayReapOutcome::Reaped if dry_run => {
                    reaped += 1;
                    println!("would reap stray pid {} ({layer})", result.pid);
                }
                StrayReapOutcome::Reaped => {
                    reaped += 1;
                    if let Some(event_root) = event_root {
                        events::emit(
                            event_root,
                            // Machine-scoped, not phase-scoped (999.44): a
                            // stray has no phase, so `0` is a sentinel
                            // meaning "not tied to any specific phase" —
                            // never a real phase number, which this
                            // project's phases never assign.
                            PhaseId::new(0),
                            "stray_reaped",
                            serde_json::json!({
                                "pid": result.pid,
                                "layer": layer,
                            }),
                        );
                    }
                    println!("reaped stray pid {} ({layer})", result.pid);
                }
                StrayReapOutcome::IdentityMismatch => {
                    skipped += 1;
                    println!(
                        "skipped stray pid {} ({layer}) — identity could not be re-confirmed \
                         immediately before signalling (the pid may have been recycled since \
                         discovery); inspect it manually (e.g. `ps -p {}`) before assuming it \
                         is safe",
                        result.pid, result.pid
                    );
                }
                StrayReapOutcome::ReapFailed => {
                    skipped += 1;
                    println!(
                        "failed to verify death for stray pid {} ({layer}) even after SIGKILL \
                         escalation — inspect it manually (e.g. `ps -p {}`)",
                        result.pid, result.pid
                    );
                }
                StrayReapOutcome::TooYoung => {
                    // No `stray_reaped` event: nothing was reaped.
                    skipped += 1;
                    println!(
                        "skipped stray pid {} ({layer}) — younger than the minimum age \
                         ({:?}); it may be a process that has not finished starting. A \
                         genuine stray will still be there on the next invocation",
                        result.pid,
                        agent::STRAY_MIN_AGE
                    );
                }
            }
        }
        // Reap both layers together (999.44): clearing only the wrapper
        // manufactures a fresh orphan out of its trailing advance child.
        // Re-run discovery once more after the pass to report anything
        // newly exposed rather than leaving it silently behind, instead of
        // looping unboundedly within one invocation.
        if !dry_run && !results.is_empty() {
            let remaining = agent::discover_stray_devflow_processes();
            if !remaining.is_empty() {
                println!(
                    "note: {} stray process(es) still discoverable after this pass — re-run \
                     `devflow gate sweep --reap-strays` to clear them",
                    remaining.len()
                );
            }
        }
    }

    if dry_run {
        println!(
            "sweep complete (dry run): {reaped} would be reaped, {skipped} skipped, {left_alone} left alone"
        );
    } else {
        println!("sweep complete: {reaped} reaped, {skipped} skipped, {left_alone} left alone");
    }
    Ok(())
}

/// What became of one [`agent::StrayProcess`] candidate the opt-in
/// stray-reaping pass considered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StrayReapOutcome {
    /// Signalled and verified dead (or, under `dry_run`, "would have been
    /// signalled" — nothing is actually sent in that case).
    Reaped,
    /// The pid's start time no longer matched what discovery recorded, so
    /// it was refused rather than signalled — 999.47's "Related TOCTOU":
    /// the pid could have been recycled between the census and this pass.
    IdentityMismatch,
    /// Identity re-confirmed, but the process was still alive even after
    /// [`agent::terminate_and_verify`]'s bounded `SIGKILL` escalation.
    ReapFailed,
    /// The candidate could be inside its own `fork()`->`execve()` window
    /// (25-12/999.47, the production half of the defect class): between
    /// `Command::spawn()` returning and the child completing `execve`,
    /// `/proc/<pid>/cmdline` reports the PARENT's argv, so the structural
    /// match [`agent::discover_stray_devflow_processes`] made was against
    /// the wrong process's argv. [`agent::is_same_process`]'s
    /// re-confirmation does NOT catch this — a mid-`execve` child is
    /// genuinely the same process with genuinely the same recorded start
    /// time as its parent, so that guard passes. [`agent::process_age`]
    /// and [`agent::STRAY_MIN_AGE`] are the guard that does: a candidate
    /// whose age is unknown, or below the floor, is refused rather than
    /// signalled.
    TooYoung,
}

/// One candidate's outcome, paired with its pid and layer so a caller can
/// report it without re-deriving either from side effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StrayReapResult {
    pid: u32,
    layer: agent::StrayLayer,
    outcome: StrayReapOutcome,
}

/// The opt-in reaper's pure(-ish) core: given an already-discovered
/// candidate list, re-confirm each one's identity immediately before acting
/// and clear it with a verified, escalating signal — never a bare
/// unverified one. Split from [`gate_sweep`] and injectable (a `&[..]`
/// slice, not a live call to [`agent::discover_stray_devflow_processes`])
/// the same way [`reconcile_planning_docs`] is split from
/// `collect_planning_doc_findings`, so this is directly unit-testable
/// against a synthetic or a real-but-fixture-owned candidate list, never
/// the whole machine's live census.
///
/// Its only side effect (beyond the `dry_run` preview, which signals
/// nothing) is [`agent::terminate_and_verify`]'s TERM→KILL escalation — a
/// single unverified `SIGTERM` is exactly what makes the CURRENT recovery
/// path report success while the process keeps running (999.44's own
/// lesson), so this never calls the bare [`agent::terminate`].
///
/// `min_age` (25-12/999.47): a candidate whose [`agent::process_age`] is
/// unknown or below this floor is refused (`TooYoung`) rather than
/// signalled — see [`StrayReapOutcome::TooYoung`] for why
/// `is_same_process` above does not already cover this. Taken as a
/// parameter rather than read from [`agent::STRAY_MIN_AGE`] directly so
/// the existing fixture-owned unit tests can drive this deterministically
/// (`Duration::ZERO` to disable the floor) without sleeping past it, and
/// so the real floor's value is visible at its one call site
/// (`gate_sweep`) instead of being implicit.
fn reap_stray_candidates(
    candidates: &[agent::StrayProcess],
    dry_run: bool,
    min_age: std::time::Duration,
) -> Vec<StrayReapResult> {
    candidates
        .iter()
        .map(|candidate| {
            // Re-confirm identity immediately before acting, closing the
            // discovery-to-signal TOCTOU window (999.47's "Related
            // TOCTOU") — copies `stop_via_lock`'s fail-closed posture:
            // refuse on any uncertainty, never signal on a mismatch.
            if !agent::is_same_process(candidate.pid, candidate.start_time) {
                return StrayReapResult {
                    pid: candidate.pid,
                    layer: candidate.layer,
                    outcome: StrayReapOutcome::IdentityMismatch,
                };
            }
            // Evaluated AFTER identity re-confirmation and BEFORE the
            // dry_run early return, so a dry run previews the same
            // verdict a real run would produce rather than promising a
            // reap the real path would refuse. `None` (age unresolvable)
            // and "younger than the floor" are both refused — fail
            // closed on uncertainty, matching `is_same_process`'s own
            // posture above.
            let old_enough = agent::process_age(candidate.pid).is_some_and(|age| age >= min_age);
            if !old_enough {
                return StrayReapResult {
                    pid: candidate.pid,
                    layer: candidate.layer,
                    outcome: StrayReapOutcome::TooYoung,
                };
            }
            if dry_run {
                return StrayReapResult {
                    pid: candidate.pid,
                    layer: candidate.layer,
                    outcome: StrayReapOutcome::Reaped,
                };
            }
            let cleared = agent::terminate_and_verify(
                candidate.pid,
                agent::TERMINATE_VERIFY_WAIT,
                agent::TERMINATE_VERIFY_POLL,
            );
            StrayReapResult {
                pid: candidate.pid,
                layer: candidate.layer,
                outcome: if cleared {
                    StrayReapOutcome::Reaped
                } else {
                    StrayReapOutcome::ReapFailed
                },
            }
        })
        .collect()
}

/// End a running phase cleanly (23c) — the missing primitive
/// `23-ORPHAN-FORENSICS.md` names as the reason 54 processes accumulated
/// with no remedy but `kill(1)`. Answers `phase`'s open gate if it has one —
/// the primary path, since it is the only one that produces a clean final
/// state: the target unwinds through its own `abort()` and releases its
/// lock; otherwise signals the process recorded in `phase`'s per-phase lock
/// file (never `state.monitor_pid` — T-23-51: the monitor shell script's
/// `trap` closure only ever captures the **agent's** pid, never the
/// trailing `advance` invocation, so by the time `advance` is the shell's
/// foreground child, `monitor_pid` already names a process that exited long
/// ago; `lock::holder`'s recorded pid is the only correct target).
pub(crate) fn stop(project_root: &Path, phase: PhaseId) -> Result<(), CliError> {
    if !stop_via_gate(project_root, phase)? {
        stop_via_lock(project_root, phase)?;
    }
    persist_stopped_state(project_root, phase)
}

/// The primary path: answer `phase`'s open gate with a rejection whose note
/// contains the abort keyword, so `GateAction::from_response` resolves to
/// `GateAction::Abort` rather than `LoopBack(Code)` — looping back would
/// relaunch an agent on a phase the operator just asked to stop.
///
/// Returns `Ok(true)` when a gate was found and handled — either reaped
/// here, or discovered already answered by a race — telling the caller not
/// to fall through to [`stop_via_lock`]. Returns `Ok(false)` when there is
/// no open gate for `phase` at all, including the race where `Gates::reap`
/// reports `NoOpenGate` after this function's own `Gates::list_open` scan
/// found one; that is the signal to fall through, not an error (cross-AI
/// review 23-10).
fn stop_via_gate(project_root: &Path, phase: PhaseId) -> Result<bool, CliError> {
    let Some(gate) = Gates::list_open(project_root)
        .into_iter()
        .find(|g| g.phase == phase)
    else {
        return Ok(false);
    };
    match Gates::reap(
        project_root,
        phase,
        gate.stage,
        "abort: stopped by `devflow stop`",
        "devflow-stop",
    ) {
        Ok(path) => {
            println!(
                "stop: wrote a rejection for phase {phase} {} at {} — the process waiting \
                 on it will pick this up on its next poll, within the 60s backoff cap",
                gate.stage,
                path.display()
            );
            Ok(true)
        }
        // A human, `--yes-ship`, or `devflow gate sweep` already answered
        // this gate between our `list_open` scan and this `reap` call. The
        // phase is already ending — that is success, not a failure to
        // report.
        Err(GateError::AlreadyResponded { .. }) => {
            println!(
                "stop: phase {phase} {} already has a response awaiting pickup — the phase \
                 is already ending",
                gate.stage
            );
            Ok(true)
        }
        Err(GateError::NoOpenGate { .. }) => Ok(false),
        Err(err) => Err(err.into()),
    }
}

/// The fallback: signal the process recorded in `phase`'s per-phase lock
/// file — the process actually running `advance()` and holding the lock
/// across the gate wait. Fail-closed on identity (T-23-52): a live pid that
/// does not look like a devflow process is refused, not signalled, since
/// the lock may be stale with a recycled pid. Never reads
/// `state.monitor_pid` — see [`stop`]'s doc comment.
fn stop_via_lock(project_root: &Path, phase: PhaseId) -> Result<(), CliError> {
    let Some((pid_str, _path)) = lock::holder(project_root, phase) else {
        println!("stop: no lock held for phase {phase} — nothing is running `advance()`");
        return Ok(());
    };
    let Ok(pid) = pid_str.parse::<u32>() else {
        println!(
            "stop: phase {phase}'s lock file holds a corrupt pid ({pid_str}) — treating it \
             as stale"
        );
        return Ok(());
    };
    if !agent::agent_running(pid) {
        println!(
            "stop: phase {phase}'s lock names pid {pid}, which is not alive — stale lock, \
             nothing to signal"
        );
        return Ok(());
    }
    // Identity must be MATCHED against what the lock recorded, never inferred
    // from /proc (999.47). A bare cmdline-basename check alone returns true
    // for any freshly forked child of a devflow process that has not finished
    // execve — during that window the child carries its parent's cmdline and
    // exe — so it would authorise signalling an unrelated process. Confirmed
    // in CI, not theorised.
    //
    // `(pid, starttime)` is unique for the life of a boot: a recycled pid
    // necessarily starts later than the one it replaced, and a mid-execve
    // child has its own start time distinct from its parent's.
    match lock::holder_identity(project_root, phase) {
        Some((recorded_pid, Some(recorded_start))) if recorded_pid == pid => {
            if !agent::is_same_process(pid, recorded_start) {
                return Err(CliError::Message(format!(
                    "refusing to signal pid {pid} for phase {phase} — it is not the \
                     process that took the lock. The lock recorded start time \
                     {recorded_start}, but pid {pid} now reports {:?}, so the pid has \
                     been recycled and belongs to something else. Inspect it manually \
                     (e.g. `ps -p {pid}`) before proceeding.",
                    agent::process_start_time(pid)
                )));
            }
        }
        Some((_, None)) => {
            // Legacy single-line lock, written before start times were
            // recorded. Identity cannot be confirmed, so fail closed rather
            // than fall back to the unsound cmdline check.
            return Err(CliError::Message(format!(
                "refusing to signal pid {pid} for phase {phase} — the lock file records \
                 no start time, so this process's identity cannot be confirmed. The lock \
                 predates identity recording; if the run is genuinely stuck, inspect the \
                 pid manually (e.g. `ps -p {pid}`) and remove \
                 .devflow/lock-{padded} once you are satisfied.",
                padded = phase.padded()
            )));
        }
        _ => {
            return Err(CliError::Message(format!(
                "refusing to signal pid {pid} for phase {phase} — the lock file's holder \
                 could not be read back for identity confirmation. Inspect it manually \
                 (e.g. `ps -p {pid}`) before proceeding."
            )));
        }
    }
    if agent::terminate(pid) {
        println!("stop: signalled pid {pid}, phase {phase}'s lock holder");
    } else {
        println!("stop: pid {pid} could not be signalled (it may have just exited)");
    }
    Ok(())
}

/// Persist the operator's intent: mark `stopped` and record why, preserving
/// any earlier reason (e.g. a prior `--until` halt) by appending rather
/// than overwriting. Never touches `stop_until` — that field means
/// something different (the requested halt stage for `devflow start
/// --until`) and `transition()` reads it. A phase with no persisted state
/// at all — never started, or already cleared by a completed abort — is
/// already stopped; that is success, not an error.
fn persist_stopped_state(project_root: &Path, phase: PhaseId) -> Result<(), CliError> {
    let mut state = match workflow::load_state(project_root, phase) {
        Ok(state) => state,
        Err(workflow::WorkflowError::MissingState(_)) => {
            println!("stop: no persisted state for phase {phase} — already stopped");
            return Ok(());
        }
        Err(err) => return Err(err.into()),
    };
    state.stopped = true;
    let reason = "stopped via `devflow stop`".to_string();
    state.stop_reason = Some(match state.stop_reason.take() {
        Some(existing) if !existing.is_empty() => format!("{existing}; {reason}"),
        _ => reason,
    });
    workflow::save_state(&state)?;
    Ok(())
}

/// Print an open gate's full, untruncated (but sanitized) context — the
/// discoverability counterpart to `gate_list`'s 100-char table truncation
/// (21a, D-03). Shares `resolve_single_open_gate_stage` with `gate_respond`
/// (999.30 / DEN-55 WR-01) so the two commands' gate-resolution behavior
/// cannot drift, and resolves/selects from one fetched open-gate collection
/// instead of reading twice (WR-03).
pub(crate) fn gate_show(
    project_root: &Path,
    phase: PhaseId,
    stage: Option<Stage>,
) -> Result<(), CliError> {
    let open = Gates::list_open(project_root);
    let stage = match stage {
        Some(stage) => stage,
        None => resolve_single_open_gate_stage(&open, phase)?,
    };
    let gate = open
        .into_iter()
        .find(|g| g.phase == phase && g.stage == stage)
        .ok_or_else(|| {
            CliError::Message(format!(
                "no open gate for phase {phase} stage {stage} — see `devflow gate list`"
            ))
        })?;
    println!("{}", render_gate_show(&gate));
    Ok(())
}

/// Pure render for `gate_show`'s output block — the FULL context via
/// `render_gate_context(.., usize::MAX)` (sanitize, never truncate; contrast
/// `gate_list`'s `render_gate_context(.., 100)`). Factored out of `gate_show`
/// so the untruncated-context guarantee is unit-testable without capturing
/// process stdout.
fn render_gate_show(gate: &OpenGate) -> String {
    format!(
        "phase {} {} ({})\n{}",
        gate.phase,
        gate.stage,
        recover::format_age(&gate.timestamp),
        render_gate_context(&gate.context, usize::MAX),
    )
}

/// Print (or follow) a phase's captured agent output.
pub(crate) fn logs(
    project_root: &Path,
    phase: Option<PhaseId>,
    follow: bool,
    stderr: bool,
) -> Result<(), CliError> {
    let phase = match phase {
        Some(p) => p,
        None => default_logs_phase(project_root)?,
    };
    let path = if stderr {
        agent_result::stderr_path(project_root, phase)
    } else {
        agent_result::stdout_path(project_root, phase)
    };
    if !path.exists() && !follow {
        return Err(CliError::Message(format!(
            "no capture file for phase {phase} at {}",
            path.display()
        )));
    }
    eprintln!("== phase {phase}: {} ==", path.display());
    let mut offset = print_capture_from(&path, 0)?;
    if !follow {
        return Ok(());
    }
    // Follow until the agent's exit code lands AND one further quiescent
    // poll produced no new bytes — the natural end of a run. (An operator
    // can always Ctrl-C sooner.)
    let exit_path = agent_result::exit_code_path(project_root, phase);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        // 14-CR-03: a stage transition archives and recreates the capture
        // file (launch_stage → archive_phase_files), so a shrunken file
        // means the next stage started — reset to the top instead of
        // seeking past EOF forever and silently skipping its output.
        let base = rollover_offset(&path, offset);
        if base != offset {
            eprintln!("== capture restarted (next stage) — following from the top ==");
        }
        let new_offset = print_capture_from(&path, base)?;
        // Quiescent only if no rollover happened AND no new bytes appeared.
        if exit_path.exists() && base == offset && new_offset == offset {
            if let Ok(code) = std::fs::read_to_string(&exit_path) {
                eprintln!("== agent exited with code {} ==", code.trim());
            }
            return Ok(());
        }
        offset = new_offset;
    }
}

/// Render the read-only cross-attempt view for one phase.
pub(crate) fn history_cmd(project_root: &Path, phase: Option<PhaseId>) -> Result<(), CliError> {
    let phase = match phase {
        Some(phase) => phase,
        None => single_active_phase(project_root)?.ok_or_else(|| {
            CliError::Message("no active phase — pass a phase number to `devflow history`".into())
        })?,
    };
    println!(
        "{}",
        history::render_timeline(&history::attempt_timeline(project_root, phase))
    );
    Ok(())
}

/// Detect capture-file rollover for `logs --follow` (14-CR-03): a file
/// shorter than the follower's offset was deleted and recreated by the next
/// stage's monitor, so following must restart from 0. A missing file (the
/// mid-rollover gap) keeps the current offset — the recreated file's shorter
/// length triggers the reset on a later poll if output restarted.
fn rollover_offset(path: &Path, offset: u64) -> u64 {
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() < offset => 0,
        _ => offset,
    }
}

/// Print the capture file's contents from `offset`, returning the new offset.
/// A missing file is treated as empty (it may not exist yet under --follow).
fn print_capture_from(path: &Path, offset: u64) -> Result<u64, CliError> {
    let stdout = std::io::stdout();
    write_capture_from(path, offset, &mut stdout.lock())
}

fn write_capture_from(
    path: &Path,
    offset: u64,
    output: &mut impl std::io::Write,
) -> Result<u64, CliError> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        return Ok(offset);
    };
    file.seek(SeekFrom::Start(offset))
        .map_err(|err| CliError::Message(format!("could not seek capture file: {err}")))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .map_err(|err| CliError::Message(format!("could not read capture file: {err}")))?;
    if !buf.is_empty() {
        let _ = output.write_all(&buf);
        let _ = output.flush();
    }
    Ok(offset + buf.len() as u64)
}

/// Pick the phase `devflow logs` should show when none is given: the single
/// active phase, else the phase with the most recently modified capture file.
fn default_logs_phase(project_root: &Path) -> Result<PhaseId, CliError> {
    if let Some(phase) = single_active_phase(project_root)? {
        return Ok(phase);
    }
    // No active state: fall back to the newest capture file on disk.
    let devflow = workflow::devflow_dir(project_root);
    let mut newest: Option<(std::time::SystemTime, PhaseId)> = None;
    if let Ok(entries) = std::fs::read_dir(&devflow) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(phase) = name
                .strip_prefix("phase-")
                .and_then(|rest| rest.strip_suffix("-stdout"))
                .and_then(|num| num.parse::<PhaseId>().ok())
            else {
                continue;
            };
            let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
                continue;
            };
            if newest.is_none_or(|(when, _)| modified > when) {
                newest = Some((modified, phase));
            }
        }
    }
    newest.map(|(_, phase)| phase).ok_or_else(|| {
        CliError::Message("no active phase and no capture files — nothing to show".into())
    })
}

/// Read the launched agent PID the monitor recorded for `phase`, if present.
fn agent_pid_from_file(project_root: &Path, phase: PhaseId) -> Option<u32> {
    let path = agent_result::agent_pid_path(project_root, phase);
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn cron_instruction_hints(project_root: &Path) -> Vec<String> {
    devflow_core::ship::list_cron_instructions(project_root)
        .iter()
        .map(cron_hint_line)
        .collect()
}

/// Build one cron-instruction hint line, appending a sanitized rate-limit
/// reset segment when `instructions.retry_after` is non-empty (21a, D-03) —
/// the reset time is already computed and persisted (`CronInstructions.
/// retry_after`, ship.rs), this only presents it; no new detection logic.
/// Pure so it's unit-testable without capturing process stdout.
fn cron_hint_line(instructions: &devflow_core::ship::CronInstructions) -> String {
    let retry_after = instructions.retry_after.trim();
    if instructions.hermes_cron.schedule.is_empty() {
        return format!(
            "Cron instruction pending (phase {}): unparseable retry-after (rate-limit resets: {}); resume manually with: devflow resume --phase {}",
            instructions.phase,
            render_gate_context(retry_after, 100),
            instructions.phase,
        );
    }
    let repeat = if instructions.hermes_cron.once {
        " --repeat 1"
    } else {
        ""
    };
    let reset = if retry_after.is_empty() {
        String::new()
    } else {
        format!(
            " (rate-limit resets: {})",
            render_gate_context(retry_after, 100)
        )
    };
    // `hermes_cron.command` is an unquoted composite shell command (e.g.
    // `cd <path> && devflow resume --phase N`); this is the single place it
    // gets quoted as one shell word — do not also quote it in `ship.rs`, or
    // the raw path inside it, or the quotes nest and break/inject (44-CORE-
    // REVIEW-FINDINGS.md finding 1).
    format!(
        "Cron instruction pending (phase {}): hermes cron create \"{}\" {}{repeat} --name {}{reset}",
        instructions.phase,
        instructions.hermes_cron.schedule,
        devflow_core::ship::shell_quote(&instructions.hermes_cron.command),
        instructions.hermes_cron.name,
    )
}

/// Print active phase worktrees with branch and inferred phase/agent.
fn print_worktrees(project_root: &Path, current: Option<&Path>) {
    let worktrees_dir = worktree::worktrees_dir(project_root);
    let worktrees = match worktree::list(project_root) {
        Ok(w) => w,
        Err(_) => return,
    };
    let active: Vec<_> = worktrees
        .iter()
        .filter(|w| w.path.starts_with(&worktrees_dir))
        .collect();
    if active.is_empty() {
        return;
    }
    println!("\nactive worktrees:");
    for wt in active {
        let label = wt
            .path
            .file_name()
            .map(|n| describe_worktree_dir(&n.to_string_lossy()))
            .unwrap_or_default();
        let branch = wt.branch.as_deref().unwrap_or("(detached)");
        let marker = if current == Some(wt.path.as_path()) {
            " *"
        } else {
            ""
        };
        println!("  {} [{branch}]{label}{marker}", wt.path.display());
    }
}

/// Turn a worktree dir name like `phase-07-claude` into ` — phase 7, agent claude`.
fn describe_worktree_dir(name: &str) -> String {
    let Some(rest) = name.strip_prefix("phase-") else {
        return String::new();
    };
    match rest.split_once('-') {
        Some((phase, agent)) => {
            format!(" — phase {}, agent {agent}", phase.trim_start_matches('0'))
        }
        None => format!(" — phase {}", rest.trim_start_matches('0')),
    }
}

pub(crate) fn list(project_root: &Path) -> Result<(), CliError> {
    let git = GitFlow::for_project(project_root);
    let branches = git.list_feature_branches()?;
    if branches.is_empty() {
        println!("no open feature branches");
        return Ok(());
    }
    println!(
        "{:<25} {:>6} {:>7}  LAST COMMIT",
        "BRANCH", "AHEAD", "BEHIND"
    );
    for b in &branches {
        println!(
            "{:<25} {:>6} {:>7}  {}",
            b.name, b.ahead, b.behind, b.last_commit
        );
    }
    Ok(())
}

fn print_open_branches(project_root: &Path) {
    let git = GitFlow::for_project(project_root);
    let base = config::git_flow_for_project(project_root).develop;
    let branches = match git.list_feature_branches() {
        Ok(b) => b,
        Err(_) => return,
    };
    if branches.is_empty() {
        return;
    }
    println!("\nopen branches:");
    for b in &branches {
        // Interpolated, not literal (45-01): the comparison this suffix
        // reports was computed against whatever trunk `list_feature_branches`
        // actually used, and naming a different one is a false statement to
        // the operator that reads as authoritative.
        let staleness = if b.behind > 0 {
            format!(" ({} behind {base})", b.behind)
        } else {
            String::new()
        };
        println!("  {} — {} ahead{staleness}", b.name, b.ahead);
    }
}

pub(crate) fn recover_cmd(
    project_root: &Path,
    do_clean: bool,
    phase: Option<PhaseId>,
) -> Result<(), CliError> {
    if do_clean {
        let warnings = match phase {
            // Explicit phase: clear it regardless of staleness (14-CR-01's
            // escape hatch for a wedged-but-fresh run).
            Some(phase) => recover::clean_phase(project_root, phase)?,
            // Implicit sweep: stale phases only.
            None => recover::clean(project_root)?,
        };
        for warning in &warnings {
            println!("warning: {warning}");
        }
        match phase {
            Some(phase) => println!("cleaned up workflow state for phase {phase}"),
            None => println!("cleaned up stale workflow state"),
        }
        return Ok(());
    }

    let statuses = match recover::inspect_all(project_root) {
        Ok(s) => s,
        Err(recover::RecoverError::NothingToRecover) => {
            println!("no state to recover — project is idle");
            return Ok(());
        }
        Err(err) => {
            return Err(CliError::Message(format!(
                "recover inspection failed: {err}"
            )));
        }
    };

    let mut any_stale = false;
    for status in &statuses {
        if let Some(only) = phase
            && status.state.phase != only
        {
            continue;
        }
        println!("phase: {}", status.state.phase);
        println!("  stage: {}", status.state.stage);
        println!("  mode: {}", status.state.mode);
        println!("  agent: {}", agents::driver_for(status.state.agent).name());
        println!("  started: {} ({})", status.state.started_at, status.age);
        match agent_pid_from_file(project_root, status.state.phase) {
            Some(pid) => {
                let running = agent::agent_running(pid);
                println!("  agent_pid: {pid} (running: {running})");
                if !running {
                    println!("  agent is not running — the monitor may have already advanced");
                }
            }
            None => println!("  agent_pid: none"),
        }
        if status.is_stale {
            any_stale = true;
            println!("  state is stale");
        }
    }

    if any_stale {
        println!(
            "\nstale state found — `devflow recover --clean` clears stale phases only; \
             use `--clean --phase N` for a specific phase"
        );
    }

    Ok(())
}

/// Run the local quality gate: cargo test, clippy, and fmt --check.
pub(crate) fn test_cmd(project_root: &Path) -> Result<(), CliError> {
    let checks = [
        ("cargo test", "cargo test"),
        (
            "cargo clippy",
            "cargo clippy --workspace --all-targets -- -D warnings",
        ),
        ("cargo fmt --check", "cargo fmt --check"),
    ];
    let mut failures = Vec::new();
    for (label, cmd) in checks {
        println!("=== {label} ===");
        let status = hermetic_command("sh", project_root)
            .arg("-c")
            .arg(cmd)
            .status()
            .map_err(|err| CliError::Message(format!("could not run `{cmd}`: {err}")))?;
        if status.success() {
            println!("  ✓ {label}");
        } else {
            println!("  ✗ {label}");
            failures.push(label);
        }
    }
    if failures.is_empty() {
        println!("\nall checks passed");
        Ok(())
    } else {
        Err(CliError::Message(format!(
            "quality checks failed: {}",
            failures.join(", ")
        )))
    }
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

/// One tool/environment check from `doctor`'s pre-existing audit (git,
/// cargo, agent CLIs, `RUST_LOG`, ...). Module-level (WR-01, 18-fix) so
/// `checks_json_value` and `doctor_json_body` can compose it into
/// `doctor --json`'s single output document without living inside `doctor`
/// itself.
pub(crate) struct Check {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) version: Option<String>,
    pub(crate) install_hint: Option<String>,
}

/// The environment checks `doctor` reports (phase 41 Task 7, F7): a named,
/// module-level seam so a unit test can assert the LIST without invoking the
/// whole doctor flow. `doctor()` calls this and renders the result. Presence
/// probes only — never a hard failure when a binary is absent (D-04).
fn doctor_checks() -> Vec<Check> {
    use std::process::Command;

    fn cmd_check(name: &str, cmd: &str, version_arg: &str, install_hint: &str) -> Check {
        match Command::new(cmd).arg(version_arg).output() {
            Ok(out) if out.status.success() => {
                let version = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("unknown")
                    .trim()
                    .to_string();
                Check {
                    name: name.into(),
                    status: "ok".into(),
                    version: Some(version),
                    install_hint: None,
                }
            }
            Ok(out) => {
                let detail = String::from_utf8_lossy(&out.stderr)
                    .lines()
                    .next()
                    .unwrap_or("unknown")
                    .trim()
                    .to_string();
                Check {
                    name: name.into(),
                    status: "warn".into(),
                    version: Some(detail),
                    install_hint: Some(format!(
                        "`{cmd} {version_arg}` exited non-zero — reinstall or check PATH"
                    )),
                }
            }
            Err(_) => Check {
                name: name.into(),
                status: "missing".into(),
                version: None,
                install_hint: Some(install_hint.into()),
            },
        }
    }

    fn bool_check(name: &str, ok: bool, version: &str, install_hint: &str) -> Check {
        Check {
            name: name.into(),
            status: if ok { "ok".into() } else { "missing".into() },
            version: Some(version.into()),
            install_hint: if ok { None } else { Some(install_hint.into()) },
        }
    }

    let devflow_version = env!("CARGO_PKG_VERSION");

    // RUST_LOG environment check: validate the value is a parsable log directive.
    let (rust_log_status, rust_log_version, rust_log_hint) = match std::env::var("RUST_LOG") {
        Ok(ref val) if val.is_empty() => (
            "warn",
            Some("empty (logging disabled)".into()),
            Some("Set RUST_LOG=info for better diagnostics".into()),
        ),
        Ok(val) => {
            let all_valid = val.split(',').all(|directive| {
                let directive = directive.trim();
                if let Some((_target, level)) = directive.split_once('=') {
                    matches!(level.trim(), "error" | "warn" | "info" | "debug" | "trace")
                } else {
                    matches!(directive, "error" | "warn" | "info" | "debug" | "trace")
                }
            });
            if all_valid {
                ("ok", Some(val), None)
            } else {
                (
                    "warn",
                    Some(val),
                    Some("RUST_LOG value may be invalid — expected error, warn, info, debug, or trace".into()),
                )
            }
        }
        Err(_) => (
            "missing",
            Some("not set — defaulting to info".into()),
            Some("Set RUST_LOG=info for better diagnostics".into()),
        ),
    };

    vec![
        cmd_check(
            "git",
            "git",
            "--version",
            "Install from https://git-scm.com/downloads",
        ),
        bool_check("sh (POSIX shell)", cfg!(unix), "built-in", "Unsupported OS"),
        cmd_check(
            "cargo/rust",
            "cargo",
            "--version",
            "curl https://sh.rustup.rs -sSf | sh",
        ),
        cmd_check(
            "gh CLI",
            "gh",
            "--version",
            "brew install gh / apt install gh",
        ),
        cmd_check(
            "claude",
            "claude",
            "--version",
            "npm i -g @anthropic-ai/claude-code",
        ),
        cmd_check("codex", "codex", "--version", "npm i -g @openai/codex"),
        cmd_check("opencode", "opencode", "--version", "npm i -g opencode-ai"),
        cmd_check(
            "pi",
            "pi",
            "--version",
            "Install Pi (see https://github.com/earendil-works/pi-mono)",
        ),
        // Phase 41 Task 7 (D-04/F7): presence-only probe of the operator's
        // `agy` wrapper. `agy --version` reports the CLI version WITHOUT
        // invoking the model — the `-p --help` hazard (a Go-flag string flag
        // that swallows the next token) does not apply to `--version`.
        cmd_check(
            "antigravity",
            "agy",
            "--version",
            "Install the Antigravity CLI so `agy` is on PATH (wrapper injects --dangerously-skip-permissions)",
        ),
        // Phase 42 Task 3 (HRMS-01, D-06): presence-only probe of the `hermes` binary.
        cmd_check(
            "hermes",
            "hermes",
            "--version",
            "Install the Hermes Agent CLI so `hermes` is on PATH",
        ),
        pi_subagent_dispatch_check(),
        opencode_subagent_dispatch_check(),
        Check {
            name: format!("devflow v{devflow_version}"),
            status: "ok".into(),
            version: Some(devflow_version.into()),
            install_hint: None,
        },
        Check {
            name: "RUST_LOG".into(),
            status: rust_log_status.into(),
            version: rust_log_version,
            install_hint: rust_log_hint,
        },
    ]
}

/// Audit the environment and report what's installed, missing, or broken.
pub(crate) fn doctor(project_root: &Path, json: bool) -> Result<(), CliError> {
    let checks = doctor_checks();

    let facts = collect_phase_facts(project_root);
    let doc_findings = collect_planning_doc_findings(project_root);
    // 999.44/DEN-68: a registry-independent, read-only /proc census — the
    // only I/O this adds is `agent::discover_stray_devflow_processes`'s
    // scan, which never signals anything (T-25-62, doctor's read-only
    // contract).
    let stray_findings = collect_stray_process_findings(project_root);

    if json {
        // WR-01 (18-fix): a single top-level JSON document —
        // `{"environment": [...], "reconciliation": [...],
        // "planning_doc_staleness": [...], "stray_processes": [...]}` —
        // instead of the pre-fix behavior of printing the tool checks as
        // one top-level `[...]` array and then printing a SECOND,
        // independent top-level array right after it. That concatenation
        // is not valid single-document JSON for any parser that isn't
        // NDJSON-aware (`json.load` raised "Extra data"). 21b's
        // planning-doc check (D-05) and this plan's stray-process finding
        // (999.44) each extend this SAME object with one more key rather
        // than forking a second array.
        let body = doctor_json_body(&checks, &facts, &doc_findings, &stray_findings);
        println!(
            "{}",
            serde_json::to_string_pretty(&body).expect("doctor --json body must serialize")
        );
    } else {
        for c in &checks {
            let icon = match c.status.as_str() {
                "ok" => "✓",
                "missing" => "✗",
                "warn" => "⚠",
                _ => "?",
            };
            let version_str = c.version.as_deref().unwrap_or("-");
            print!("  {:<20} {:<20} {}", c.name, version_str, icon);
            #[allow(clippy::collapsible_if)]
            if c.status == "missing" || c.status == "warn" {
                if let Some(hint) = &c.install_hint {
                    print!(" — {}", hint);
                }
            }
            println!();
        }
        print!("{}", render_reconciliation_text(&facts));
        print!("{}", render_planning_doc_text(&doc_findings));
        print!("{}", render_stray_process_text(&stray_findings));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// release --check (20d)
// ---------------------------------------------------------------------------

/// Read-only release-cut preflight. Ceiling is `--check` only (D-03): no
/// state-mutating helper, no `git tag`/publish, and (once Task 2/3 land) no
/// `git fetch` — every check here reads already-available local state. This
/// no-fetch property is scoped to `release --check`; it does NOT extend to
/// `devflow start`, whose `ensure_base_ref_current` (999.51) issues its own
/// `git fetch` on the start path — see that function's doc comment.
/// Follows `doctor`'s `Check`-list-then-report shape (reuses the same
/// `Check` struct) so the two commands stay visually consistent.
pub(crate) fn release_check(project_root: &Path) -> Result<(), CliError> {
    let checks: Vec<Check> = vec![
        check_self_pin(project_root),
        check_divergence(project_root),
        check_publish_order(project_root),
        check_changelog_version(project_root),
    ];

    let mut failed = false;
    for c in &checks {
        let icon = match c.status.as_str() {
            "ok" => "✓",
            "warn" => "⚠",
            "fail" => "✗",
            _ => "?",
        };
        let detail = c.version.as_deref().unwrap_or("-");
        println!("  {:<32} {icon}  {detail}", c.name);
        if matches!(c.status.as_str(), "warn" | "fail")
            && let Some(hint) = &c.install_hint
        {
            println!("      — {hint}");
        }
        if c.status == "fail" {
            failed = true;
        }
    }

    if failed {
        Err(CliError::Message(
            "release preflight failed — see checks above".into(),
        ))
    } else {
        println!("\nrelease preflight passed");
        Ok(())
    }
}

/// The `pi subagent dispatch` doctor check: reports whether the vetted
/// `@bacnh85/pi-subagent` extension is installed at user scope. Warns when
/// absent — the baseline single-agent path still works without it. Split out
/// (phase-39 code review) so the check's mapping is unit-testable.
fn pi_subagent_dispatch_check() -> Check {
    let dispatch = agents::driver_for(AgentKind::Pi)
        .capabilities()
        .subagent_dispatch;
    pi_subagent_dispatch_check_for(dispatch)
}

/// The pure boolean→`Check` mapping behind [`pi_subagent_dispatch_check`],
/// separated from the `pi list` probe so the doctor rendering is testable
/// without spawning a process.
fn pi_subagent_dispatch_check_for(dispatch: bool) -> Check {
    Check {
        name: "pi subagent dispatch".into(),
        status: if dispatch { "ok".into() } else { "warn".into() },
        version: Some(if dispatch {
            "available".into()
        } else {
            "not installed".into()
        }),
        install_hint: if dispatch {
            None
        } else {
            Some(
                "optional — `pi install npm:@bacnh85/pi-subagent` (user scope) enables subagent dispatch"
                    .into(),
            )
        },
    }
}

/// The `opencode subagent dispatch` doctor check (43-REVIEW.md WR-01): reports
/// whether OpenCode has a genuinely dispatchable subagent configured, mirroring
/// `pi_subagent_dispatch_check` exactly. Without this, `OpenCodeDriver::capabilities()`
/// — built for OPCD-03/D-10 — had no production caller and was dead code outside
/// its own test module.
fn opencode_subagent_dispatch_check() -> Check {
    let dispatch = agents::driver_for(AgentKind::OpenCode)
        .capabilities()
        .subagent_dispatch;
    opencode_subagent_dispatch_check_for(dispatch)
}

/// The pure boolean→`Check` mapping behind [`opencode_subagent_dispatch_check`],
/// separated from the `opencode agent list` probe so the doctor rendering is
/// testable without spawning a process — same split as
/// [`pi_subagent_dispatch_check_for`].
fn opencode_subagent_dispatch_check_for(dispatch: bool) -> Check {
    Check {
        name: "opencode subagent dispatch".into(),
        status: if dispatch { "ok".into() } else { "warn".into() },
        version: Some(if dispatch {
            "available".into()
        } else {
            "not configured".into()
        }),
        install_hint: if dispatch {
            None
        } else {
            Some(
                "optional — configure a (subagent)/(all)-mode agent via `opencode agent create` \
                 to enable subagent dispatch"
                    .into(),
            )
        },
    }
}

/// Self-pin check (asserts 20a's invariant): every local-path
/// `[workspace.dependencies]` self-pin must equal `[workspace.package]
/// version`, compared dynamically — never against a hardcoded expected
/// version.
fn check_self_pin(project_root: &Path) -> Check {
    const NAME: &str = "self-pin (workspace member versions)";

    let cargo_toml = project_root.join("Cargo.toml");
    let contents = match std::fs::read_to_string(&cargo_toml) {
        Ok(contents) => contents,
        Err(err) => {
            return Check {
                name: NAME.into(),
                status: "warn".into(),
                version: Some(format!("could not read Cargo.toml: {err}")),
                install_hint: None,
            };
        }
    };

    let (workspace_version, pins) = version::read_workspace_self_pins(&contents);
    let Some(workspace_version) = workspace_version else {
        return Check {
            name: NAME.into(),
            status: "warn".into(),
            version: Some("not a workspace Cargo.toml (no [workspace.package] version)".into()),
            install_hint: None,
        };
    };

    let drifted: Vec<String> = pins
        .iter()
        .filter(|pin| pin.version != workspace_version)
        .map(|pin| format!("{} pinned {} != {workspace_version}", pin.name, pin.version))
        .collect();

    if drifted.is_empty() {
        Check {
            name: NAME.into(),
            status: "ok".into(),
            version: Some(format!(
                "{} member pin(s) match {workspace_version}",
                pins.len()
            )),
            install_hint: None,
        }
    } else {
        Check {
            name: NAME.into(),
            status: "fail".into(),
            version: Some(drifted.join("; ")),
            install_hint: Some(format!(
                "every [workspace.dependencies] self-pin must equal [workspace.package] \
                 version = \"{workspace_version}\" — VersionBump should have rewritten this; \
                 see 20a/DEN-49"
            )),
        }
    }
}

/// Divergence check: whether `origin/main` is an ancestor of `HEAD` — i.e.
/// whether `scripts/sync-main-to-develop.sh` would be a no-op — read
/// against ALREADY-FETCHED local refs, issuing NO `git fetch` (review:
/// Codex HIGH — a "read-only" preflight must not depend on the network).
/// This property is scoped to `release --check`'s read-only preflight, not
/// a project-wide invariant: `devflow start`'s `ensure_base_ref_current`
/// (999.51) issues its own `git fetch` before comparing on the start path,
/// since a trustworthy currency comparison requires one either way.
fn check_divergence(project_root: &Path) -> Check {
    const NAME: &str = "develop/main divergence (origin/main ancestor)";
    match devflow_core::git::origin_main_ancestor_status(project_root) {
        devflow_core::git::AncestorStatus::Ancestor => Check {
            name: NAME.into(),
            status: "ok".into(),
            version: Some("origin/main is an ancestor of HEAD — sync would be a no-op".into()),
            install_hint: None,
        },
        devflow_core::git::AncestorStatus::Diverged => Check {
            name: NAME.into(),
            status: "fail".into(),
            version: Some("origin/main is NOT an ancestor of HEAD — develop has diverged".into()),
            install_hint: Some(
                "run scripts/sync-main-to-develop.sh before cutting the next release PR".into(),
            ),
        },
        devflow_core::git::AncestorStatus::RefAbsent => Check {
            name: NAME.into(),
            status: "warn".into(),
            version: Some("origin/main not fetched — cannot determine divergence".into()),
            install_hint: Some("run `git fetch` first, then re-run this check".into()),
        },
    }
}

/// Publish-order check: crates.io requires `devflow-core` to be live before
/// `devflow` (path-dependency `--dry-run`/verify resolves against the
/// *published* registry version, not local source). Sourced from the
/// workspace's own members/dependency graph, never a hardcoded prose
/// string.
fn check_publish_order(project_root: &Path) -> Check {
    const NAME: &str = "crates.io publish order";
    let order = devflow_core::git::publish_order(project_root);
    if order.is_empty() {
        return Check {
            name: NAME.into(),
            status: "warn".into(),
            version: Some("could not determine workspace publish order".into()),
            install_hint: None,
        };
    }
    Check {
        name: NAME.into(),
        status: "ok".into(),
        version: Some(format!("publish in order: {}", order.join(" -> "))),
        install_hint: None,
    }
}

/// Changelog-version check (999.96): `CHANGELOG.md`'s topmost `## <version>`
/// heading must agree with the workspace version, so a forgotten bump can't
/// ship a changelog under the old version. The negative control is a SYNTHETIC
/// mismatched fixture — not "the current tree", which the v2.5.0 cut already
/// brought into agreement.
fn check_changelog_version(project_root: &Path) -> Check {
    const NAME: &str = "changelog version (matches workspace)";

    let cargo_toml = project_root.join("Cargo.toml");
    let contents = match std::fs::read_to_string(&cargo_toml) {
        Ok(contents) => contents,
        Err(err) => {
            return Check {
                name: NAME.into(),
                status: "warn".into(),
                version: Some(format!("could not read Cargo.toml: {err}")),
                install_hint: None,
            };
        }
    };
    let (workspace_version, _) = version::read_workspace_self_pins(&contents);
    let Some(workspace_version) = workspace_version else {
        return Check {
            name: NAME.into(),
            status: "warn".into(),
            version: Some("not a workspace Cargo.toml (no [workspace.package] version)".into()),
            install_hint: None,
        };
    };

    let changelog_contents = match std::fs::read_to_string(project_root.join("CHANGELOG.md")) {
        Ok(contents) => contents,
        Err(err) => {
            return Check {
                name: NAME.into(),
                status: "warn".into(),
                version: Some(format!("could not read CHANGELOG.md: {err}")),
                install_hint: None,
            };
        }
    };
    let changelog_version = changelog_contents.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("## ")?;
        let version = rest.split_whitespace().next().map(|w| {
            w.trim_start_matches('[')
                .trim_end_matches(']')
                .trim_start_matches('v')
        });
        // A version looks like `X.Y` — a leading digit, a dot, then another
        // digit — so a `## [2.5.0]` link and a `## v2.5.0` prefix both parse,
        // while a numbered section like `## 1. Overview` does not.
        version
            .filter(|v| {
                let Some(dot) = v.find('.') else {
                    return false;
                };
                let bytes = v.as_bytes();
                bytes.first().is_some_and(|b| b.is_ascii_digit())
                    && bytes.get(dot + 1).is_some_and(|b| b.is_ascii_digit())
            })
            .map(str::to_string)
    });
    let Some(changelog_version) = changelog_version else {
        return Check {
            name: NAME.into(),
            status: "warn".into(),
            version: Some("no `## <version>` heading found in CHANGELOG.md".into()),
            install_hint: None,
        };
    };

    if changelog_version == workspace_version {
        Check {
            name: NAME.into(),
            status: "ok".into(),
            version: Some(format!("changelog {changelog_version} matches workspace")),
            install_hint: None,
        }
    } else {
        let direction = match compare_versions(&changelog_version, &workspace_version) {
            Some(std::cmp::Ordering::Greater) => {
                "changelog ahead of Cargo.toml (version bump not yet applied)"
            }
            Some(std::cmp::Ordering::Less) => {
                "Cargo.toml ahead of changelog (release notes missing)"
            }
            _ => "direction undetermined (equal-numeric or unparseable version)",
        };
        Check {
            name: NAME.into(),
            status: "fail".into(),
            version: Some(format!(
                "changelog {changelog_version} != workspace {workspace_version} — {direction}"
            )),
            install_hint: Some(direction.into()),
        }
    }
}

/// Best-effort comparison of two `X.Y.Z` version strings, component-wise, with
/// standard semver prerelease ordering (`2.5.0` > `2.5.0-rc.1`). `None` if
/// either is unparseable.
fn compare_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
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
        let has_prerelease = v.contains('-');
        Some((parts, has_prerelease))
    };
    let (a, a_pre) = components(a)?;
    let (b, b_pre) = components(b)?;
    let mut cmp = a.cmp(&b);
    if cmp == std::cmp::Ordering::Equal {
        cmp = match (a_pre, b_pre) {
            (false, true) => std::cmp::Ordering::Greater,
            (true, false) => std::cmp::Ordering::Less,
            _ => std::cmp::Ordering::Equal,
        };
    }
    Some(cmp)
}

// Tag-signing viability check removed (999.104): the probe was capability-only
// ("can this key sign"), never identity ("is this the maintainer's key"), and
// `scripts/cut-release.sh` now fails loudly when `devflow.releaseSigningKey` is
// unset. The identity check that matters lives in `scripts/hooks/pre-push`.

// ---------------------------------------------------------------------------
// release --verify (post-cut)
// ---------------------------------------------------------------------------

/// Read-only post-cut verifier. `release --check` guards the PRE-release
/// invariants; this guards the POST-release ones that `--check` cannot see
/// and that have been the easy-to-miss manual steps in practice: the release
/// tag must point at a commit on `main` (not the `develop` release commit),
/// and the `main`→`develop` sync must have been run. Same
/// `Check`-list-then-report shape as `release --check`.
pub(crate) fn release_verify(project_root: &Path) -> Result<(), CliError> {
    let checks: Vec<Check> = vec![
        check_tag_on_main(project_root),
        check_sync_done(project_root),
    ];

    let mut failed = false;
    for c in &checks {
        let icon = match c.status.as_str() {
            "ok" => "✓",
            "warn" => "⚠",
            "fail" => "✗",
            _ => "?",
        };
        let detail = c.version.as_deref().unwrap_or("-");
        println!("  {:<32} {icon}  {detail}", c.name);
        if matches!(c.status.as_str(), "warn" | "fail")
            && let Some(hint) = &c.install_hint
        {
            println!("      — {hint}");
        }
        if c.status == "fail" {
            failed = true;
        }
    }

    if failed {
        Err(CliError::Message(
            "release verification failed — see checks above".into(),
        ))
    } else {
        println!("\nrelease verification passed");
        Ok(())
    }
}

/// The release tag (`v{workspace version}`) must point at a commit on
/// `origin/main`. A tag placed on the `develop` release commit (the mistake
/// made cutting v2.5.0) is NOT an ancestor of `main`'s squash-merge lineage,
/// so an ancestry test cleanly separates correct from incorrect placement.
fn check_tag_on_main(project_root: &Path) -> Check {
    const NAME: &str = "release tag on main";

    let cargo_toml = project_root.join("Cargo.toml");
    let contents = match std::fs::read_to_string(&cargo_toml) {
        Ok(c) => c,
        Err(err) => {
            return Check {
                name: NAME.into(),
                status: "warn".into(),
                version: Some(format!("could not read Cargo.toml: {err}")),
                install_hint: None,
            };
        }
    };
    let (Some(workspace_version), _) = version::read_workspace_self_pins(&contents) else {
        return Check {
            name: NAME.into(),
            status: "warn".into(),
            version: Some("no [workspace.package] version to derive the tag from".into()),
            install_hint: None,
        };
    };
    let tag = format!("v{workspace_version}");

    let tag_exists = devflow_core::git::git_command(project_root)
        .args(["rev-parse", "--verify", "--quiet", &tag])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if !tag_exists {
        return Check {
            name: NAME.into(),
            status: "fail".into(),
            version: Some(format!("tag {tag} does not exist")),
            install_hint: Some("tag the release commit on main before verifying".into()),
        };
    }

    match devflow_core::git::ref_is_ancestor(project_root, &tag, "origin/main") {
        devflow_core::git::AncestorStatus::Ancestor => Check {
            name: NAME.into(),
            status: "ok".into(),
            version: Some(format!("{tag} is on origin/main")),
            install_hint: None,
        },
        devflow_core::git::AncestorStatus::Diverged => Check {
            name: NAME.into(),
            status: "fail".into(),
            version: Some(format!(
                "{tag} is NOT an ancestor of origin/main — tagged the wrong branch"
            )),
            install_hint: Some(
                "re-tag on main:  git -c user.signingkey=\"$(git config --get \
                 devflow.releaseSigningKey)\" tag -s -f vX.Y.Z origin/main"
                    .into(),
            ),
        },
        devflow_core::git::AncestorStatus::RefAbsent => Check {
            name: NAME.into(),
            status: "warn".into(),
            version: Some("origin/main not fetched — cannot compare tag placement".into()),
            install_hint: Some("run `git fetch` first, then re-run".into()),
        },
    }
}

/// After a release, `origin/main` must be an ancestor of `origin/develop`
/// (the `scripts/sync-main-to-develop.sh` step). Skipping it means the next
/// release PR conflicts against a stale merge-base.
fn check_sync_done(project_root: &Path) -> Check {
    const NAME: &str = "main→develop sync";
    match devflow_core::git::ref_is_ancestor(project_root, "origin/main", "origin/develop") {
        devflow_core::git::AncestorStatus::Ancestor => Check {
            name: NAME.into(),
            status: "ok".into(),
            version: Some("origin/main is an ancestor of origin/develop".into()),
            install_hint: None,
        },
        devflow_core::git::AncestorStatus::Diverged => Check {
            name: NAME.into(),
            status: "fail".into(),
            version: Some(
                "origin/main is NOT an ancestor of origin/develop — sync was skipped".into(),
            ),
            install_hint: Some(
                "run scripts/sync-main-to-develop.sh, then PR the merge commit (not squash)".into(),
            ),
        },
        devflow_core::git::AncestorStatus::RefAbsent => Check {
            name: NAME.into(),
            status: "warn".into(),
            version: Some("origin/main or origin/develop not fetched — cannot check sync".into()),
            install_hint: Some("run `git fetch` first, then re-run".into()),
        },
    }
}

// ---------------------------------------------------------------------------
// doctor reconciliation (18a)
// ---------------------------------------------------------------------------

/// Severity of a reconciliation finding, matching the existing `Check.status`
/// convention (lowercase strings) so both `doctor` renderers stay consistent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Severity {
    Ok,
    Warn,
    Problem,
}

impl Severity {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Severity::Ok => "ok",
            Severity::Warn => "warn",
            Severity::Problem => "problem",
        }
    }
}

/// The read-only facts `doctor` gathers for one active phase before
/// reconciling them. Collected by `collect_phase_facts` (all I/O); consumed
/// with zero I/O by `reconcile_phase`.
pub(crate) struct PhaseFacts {
    pub(crate) phase: PhaseId,
    pub(crate) stage: Stage,
    pub(crate) gate_pending: bool,
    pub(crate) agent_pid: Option<u32>,
    pub(crate) agent_alive: bool,
    /// The monitor pid recorded in `State.monitor_pid` (18b). `None` means
    /// no monitor has been spawned for this state yet, or the state was
    /// written by a binary predating the field — never treated as a problem.
    pub(crate) monitor_pid: Option<u32>,
    pub(crate) monitor_alive: bool,
    /// The most recent event's `event` field value, for display context.
    pub(crate) last_event: Option<String>,
    /// The `stage` field of the most recent `stage_launched` event; `None`
    /// when the last event recorded for this phase is not a launch.
    pub(crate) last_launched_stage: Option<Stage>,
    pub(crate) open_gate_stages: Vec<Stage>,
    pub(crate) feature_branch_exists: bool,
    /// Whether this phase was intentionally halted by `devflow start --until
    /// <stage>` (20c) — `State.stopped`. A stopped phase's dead agent
    /// pid/stale monitor pid are expected, not a crash; both
    /// `check_dead_agent` and `check_dead_monitor` must recognize this
    /// marker instead of reporting a `Problem` (review: Codex HIGH — the
    /// doctor gap is bigger than `check_dead_agent` alone).
    pub(crate) stopped: bool,
}

/// One diagnostic finding for a phase, with a copy-pasteable repair command
/// when one exists. Never carries a filesystem path or username (T-18-01) —
/// only phase numbers, stage names, and pids identify the disagreement.
pub(crate) struct PhaseFinding {
    pub(crate) phase: PhaseId,
    pub(crate) severity: Severity,
    pub(crate) detail: String,
    pub(crate) repair: Option<String>,
}

/// `gate_pending` is set but no gate file is open for this phase — the gate
/// answer path is stuck. `doctor` only reports this; it never repairs it
/// (T-18-02).
fn check_gate_pending_without_gate(facts: &PhaseFacts) -> Option<PhaseFinding> {
    if !facts.gate_pending || !facts.open_gate_stages.is_empty() {
        return None;
    }
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Problem,
        detail: format!(
            "phase {}: gate_pending is true at stage {} but no gate file is open",
            facts.phase, facts.stage
        ),
        repair: Some(format!("devflow resume --phase {}", facts.phase)),
    })
}

/// An open gate file exists but `gate_pending` is false — an unanswered
/// operator question that `status`/`doctor` isn't surfacing as pending.
fn check_orphan_gate(facts: &PhaseFacts) -> Option<PhaseFinding> {
    if facts.gate_pending || facts.open_gate_stages.is_empty() {
        return None;
    }
    let gate_stage = facts.open_gate_stages[0];
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Problem,
        detail: format!(
            "phase {}: gate open for stage {} but state.gate_pending is false",
            facts.phase, gate_stage
        ),
        repair: Some(format!(
            "devflow gate approve {} --stage {}",
            facts.phase, gate_stage
        )),
    })
}

/// The recorded agent pid is not alive while the phase sits at an
/// agent-driven stage — the "who watches the watcher" class of silent death
/// CONTEXT.md cites (two incidents, ~4h lost, found only via `ps`).
fn check_dead_agent(facts: &PhaseFacts) -> Option<PhaseFinding> {
    let pid = facts.agent_pid?;
    if facts.stopped || facts.agent_alive || !facts.stage.is_agent_stage() {
        return None;
    }
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Problem,
        detail: format!(
            "phase {}: agent pid {pid} recorded but not running at stage {}",
            facts.phase, facts.stage
        ),
        repair: Some(format!("devflow resume --phase {}", facts.phase)),
    })
}

/// The recorded monitor pid is dead — nothing will call `devflow advance`
/// for this phase, whether or not the agent is also dead (an agent that
/// outlived its monitor is orphaned too, since nothing will advance it when
/// it exits either). Reuses `liveness` rather than re-deriving the matrix,
/// so the two copies can never drift (18b, T-18-11's `Unknown` guard applies
/// here transitively — an unrecorded monitor is silently `Unknown`, never a
/// finding).
fn check_dead_monitor(facts: &PhaseFacts) -> Option<PhaseFinding> {
    if facts.stopped
        || liveness(facts.monitor_pid, facts.monitor_alive, facts.agent_alive) != Liveness::Stuck
    {
        return None;
    }
    let pid = facts.monitor_pid?;
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Problem,
        detail: format!(
            "phase {}: monitor pid {pid} recorded but not running at stage {}",
            facts.phase, facts.stage
        ),
        repair: Some(format!("devflow resume --phase {}", facts.phase)),
    })
}

/// The last `stage_launched` event named a different stage than
/// `state.stage`. A `Warn`, not a `Problem` — a healthy pipeline legitimately
/// has one stage in flight between the launch event and the next
/// transition; exact equality is agreement, never an off-by-one mismatch.
fn check_stage_event_drift(facts: &PhaseFacts) -> Option<PhaseFinding> {
    let launched = facts.last_launched_stage?;
    if launched == facts.stage {
        return None;
    }
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Warn,
        detail: format!(
            "phase {}: last stage_launched event named {launched} but state.stage is {}",
            facts.phase, facts.stage
        ),
        repair: None,
    })
}

/// The phase's feature branch does not exist even though its stage is past
/// `Define`. A `Warn` — a not-yet-pushed or manually deleted branch is
/// recoverable without state surgery.
fn check_missing_branch(facts: &PhaseFacts) -> Option<PhaseFinding> {
    if facts.feature_branch_exists || facts.stage == Stage::Define {
        return None;
    }
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Warn,
        detail: format!(
            "phase {}: feature/phase-{} does not exist but stage is {}",
            facts.phase,
            facts.phase.padded(),
            facts.stage
        ),
        repair: None,
    })
}

/// Pure reconciliation core: diffs `state.stage` against the latest event,
/// live agent pid, open gates, and branch existence, evaluating checks in a
/// fixed order so the returned findings never depend on how `facts` was
/// assembled (ordering edge). Takes no path, performs no I/O, and mutates
/// nothing (T-18-02) — directly unit-testable without a repository.
fn reconcile_phase(facts: &PhaseFacts) -> Vec<PhaseFinding> {
    [
        check_gate_pending_without_gate(facts),
        check_orphan_gate(facts),
        check_dead_agent(facts),
        check_dead_monitor(facts),
        check_stage_event_drift(facts),
        check_missing_branch(facts),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Gather the read-only facts `reconcile_phase` needs for every active
/// phase, sorted by phase ascending so output ordering never depends on
/// directory-read order (ordering edge). Every call here is a read-only
/// primitive already used elsewhere (`status`, `recover::inspect_all`) —
/// none of it is reimplemented.
fn collect_phase_facts(project_root: &Path) -> Vec<PhaseFacts> {
    let states = workflow::list_states(project_root);
    // 14-CR-10: one pass over events.jsonl for every phase's last event,
    // matching status()'s optimization, not a per-phase rescan.
    let mut last_events = events::last_events_by_phase(project_root);
    let open_gates = Gates::list_open(project_root);

    let mut facts: Vec<PhaseFacts> = states
        .into_iter()
        .map(|state| build_phase_facts(project_root, state, &mut last_events, &open_gates))
        .collect();

    facts.sort_by_key(|f| f.phase);
    facts
}

/// Build one phase's [`PhaseFacts`] from already-fetched state, events, and
/// gates — the per-phase half of `collect_phase_facts`, split out to keep
/// that function short.
fn build_phase_facts(
    project_root: &Path,
    state: State,
    last_events: &mut std::collections::HashMap<PhaseId, events::PhaseEventSummary>,
    open_gates: &[OpenGate],
) -> PhaseFacts {
    let phase = state.phase;
    let stopped = state.stopped;
    let agent_pid = agent_pid_from_file(project_root, phase);
    let agent_alive = agent_pid.is_some_and(agent::agent_running);
    let monitor_pid = state.monitor_pid;
    let monitor_alive = monitor_pid.is_some_and(agent::agent_running);
    let last_event = last_events.remove(&phase).map(|summary| summary.event);
    let last_launched_stage = last_event.as_ref().and_then(last_launched_stage_from_event);
    let last_event_name = last_event
        .as_ref()
        .and_then(|e| e.get("event"))
        .and_then(|e| e.as_str())
        .map(str::to_string);
    let open_gate_stages = open_gates
        .iter()
        .filter(|g| g.phase == phase)
        .map(|g| g.stage)
        .collect();
    let branch_ref = format!("refs/heads/feature/phase-{padded}", padded = phase.padded());
    let feature_branch_exists =
        run_git_stdout(project_root, &["rev-parse", "--verify", &branch_ref]).is_some();

    PhaseFacts {
        phase,
        stage: state.stage,
        gate_pending: state.gate_pending,
        agent_pid,
        agent_alive,
        monitor_pid,
        monitor_alive,
        last_event: last_event_name,
        last_launched_stage,
        open_gate_stages,
        feature_branch_exists,
        stopped,
    }
}

/// Derive the stage named by an event's `stage` field, but only when the
/// event's `event` field is `"stage_launched"` — any other event kind (or
/// an unparsable stage name) yields `None`, never a panic.
fn last_launched_stage_from_event(event: &serde_json::Value) -> Option<Stage> {
    if event.get("event").and_then(|e| e.as_str()) != Some("stage_launched") {
        return None;
    }
    event
        .get("stage")
        .and_then(|s| s.as_str())
        .and_then(|s| s.parse::<Stage>().ok())
}

/// The findings to display for one phase: real findings when any exist,
/// otherwise a single synthetic `ok` finding — the display-only counterpart
/// to `reconcile_phase`'s "zero findings" agreement case, shared by both
/// the text and `--json` renderers.
fn findings_for_display(facts: &PhaseFacts) -> Vec<PhaseFinding> {
    let findings = reconcile_phase(facts);
    if !findings.is_empty() {
        return findings;
    }
    vec![PhaseFinding {
        phase: facts.phase,
        severity: Severity::Ok,
        detail: format!("phase {}: ok", facts.phase),
        repair: None,
    }]
}

/// Build `doctor`'s per-phase reconciliation section (after the existing
/// tool/env checks), read-only: it never calls `workflow::save_state`,
/// `events::emit`, `Gates::cleanup`/`Gates::write`, or any `recover::clean*`
/// function (T-18-02). A pure string builder (not a direct `println!`) so
/// it's directly assertable in tests without capturing process stdout.
fn render_reconciliation_text(facts: &[PhaseFacts]) -> String {
    let mut out = String::from("\nreconciliation:\n");
    if facts.is_empty() {
        out.push_str("  no active phases — nothing to reconcile\n");
        return out;
    }
    for phase_facts in facts {
        for finding in findings_for_display(phase_facts) {
            out.push_str(&format!("  {}\n", finding.detail));
            if let Some(repair) = &finding.repair {
                out.push_str(&format!("    repair: {repair}\n"));
            }
        }
    }
    out
}

/// Build the `--json` reconciliation array as a `serde_json::Value` (WR-01,
/// 18-fix). No longer prints its own top-level `[...]` document — `doctor()`
/// nests this under `"reconciliation"` in the single object
/// `doctor_json_body` composes alongside `checks_json_value`'s
/// `"environment"` array.
fn render_reconciliation_json(facts: &[PhaseFacts]) -> serde_json::Value {
    // Pair each finding with its originating phase's last recorded event, so
    // a `--json` consumer gets that context without re-reading events.jsonl.
    let findings: Vec<(&PhaseFacts, PhaseFinding)> = facts
        .iter()
        .flat_map(|pf| findings_for_display(pf).into_iter().map(move |f| (pf, f)))
        .collect();
    serde_json::Value::Array(
        findings
            .iter()
            .map(|(phase_facts, finding)| {
                serde_json::json!({
                    "phase": finding.phase,
                    "severity": finding.severity.label(),
                    "detail": finding.detail,
                    "repair": finding.repair,
                    "last_event": phase_facts.last_event,
                })
            })
            .collect(),
    )
}

/// Build `doctor --json`'s `"environment"` array from the pre-existing
/// tool/env checks (WR-01, 18-fix). Extracted so it can be composed with
/// `render_reconciliation_json`'s array into ONE JSON document instead of
/// being printed as its own top-level array.
fn checks_json_value(checks: &[Check]) -> serde_json::Value {
    serde_json::Value::Array(
        checks
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "status": c.status,
                    "version": c.version,
                    "install_hint": c.install_hint,
                })
            })
            .collect(),
    )
}

/// Compose `doctor --json`'s single JSON document (WR-01, 18-fix). Pre-fix,
/// `doctor()` printed the tool checks as one top-level `[...]` array and
/// then printed `render_reconciliation_json`'s array as a SECOND,
/// independent top-level array right after it — invalid single-document
/// JSON for any parser that isn't NDJSON-aware (`json.load` raised "Extra
/// data" against a live fixture with one active phase). There is now
/// exactly one top-level value: `{"environment": [...], "reconciliation":
/// [...], "planning_doc_staleness": [...]}` — 21b's addition (D-05) extends
/// this SAME object with a third key rather than forking a second reporter
/// or printing a second top-level array.
fn doctor_json_body(
    checks: &[Check],
    facts: &[PhaseFacts],
    doc_findings: &[PlanningDocFinding],
    stray_findings: &[StrayProcessFinding],
) -> serde_json::Value {
    serde_json::json!({
        "environment": checks_json_value(checks),
        "reconciliation": render_reconciliation_json(facts),
        "planning_doc_staleness": render_planning_doc_findings_json(doc_findings),
        "stray_processes": render_stray_process_findings_json(stray_findings),
    })
}

// ---------------------------------------------------------------------------
// doctor planning-doc staleness reconciliation (21b, D-04/D-05)
// ---------------------------------------------------------------------------

/// The `v1.5.0` numeric-tuple cutoff (RESEARCH Pitfall #2): the first phase
/// whose version claim was consistently tagged. A claimed version at or
/// after this cutoff with no matching/reachable git tag is a real
/// `Severity::Problem`; a claim before it is legacy history and downgrades
/// to `Severity::Warn` — otherwise a naive per-row check floods `doctor`
/// with pre-Phase-18 noise, the exact alert-fatigue class 999.14 exists to
/// prevent. Compared as a NUMERIC `(major, minor, patch)` tuple, never a
/// string, so a real future `v1.10.0` is correctly treated as post-cutoff
/// (a lexicographic `"1.10.0" < "1.5.0"` would wrongly sort it as legacy).
const PLANNING_DOC_STALENESS_CUTOFF: (u32, u32, u32) = (1, 5, 0);

/// One detection-only finding produced by reconciling a `ROADMAP.md`/
/// `STATE.md` version claim against the repo's git tags. A sibling of
/// `PhaseFinding`, not a reuse of it: most claims here are about
/// already-shipped phases with no active `state-NN.json`/`PhaseFacts`
/// (RESEARCH Open Q1). `repair` is always `None` — D-04 forbids any
/// auto-correction of planning-doc prose; nothing in this module has a
/// write path to either file. Never carries a filesystem path or username
/// (T-18-01 discipline) — only the source label, the claimed version, and
/// a git tag name.
pub(crate) struct PlanningDocFinding {
    pub(crate) source: String,
    pub(crate) claim: String,
    pub(crate) severity: Severity,
    pub(crate) detail: String,
    pub(crate) repair: Option<String>,
}

/// Parse a table cell as a bare `(major, minor, patch)` semver tuple,
/// stripping an optional leading `v`. Returns `None` for anything that
/// isn't EXACTLY three dot-separated numeric components — this is what
/// keeps version ranges (`0.1.0–0.6.0`), em-dash placeholders (`—`), and
/// any other non-semver cell out of every downstream finding (RESEARCH
/// Pitfall #2), without needing a regex crate: the range's `–` makes its
/// middle component fail `str::parse::<u32>`, and the em-dash fails
/// outright.
pub(crate) fn parse_semver(cell: &str) -> Option<(u32, u32, u32)> {
    let cell = cell.strip_prefix('v').unwrap_or(cell);
    let mut parts = cell.split('.');
    let major = parts.next()?.trim().parse().ok()?;
    let minor = parts.next()?.trim().parse().ok()?;
    let patch = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() {
        return None; // more than three `.`-separated components
    }
    Some((major, minor, patch))
}

/// Scan a `## Shipped`/`## Completed`-shaped markdown table for `(label,
/// version)` rows whose version cell is a bare or `v`-prefixed single
/// semver. Hand-scans lines split on `|` rather than pulling in a
/// markdown-table parser crate — the `is_self_dogfood_workspace` (D-17)
/// convention this codebase already follows for small, fixed-shape
/// structured text. Skips the header row, the `|---|---|` separator row,
/// and any cell that isn't a bare semver (ranges, em-dashes, anything
/// else) outright; never panics on a malformed row (T-21b-03 — parse
/// defensively, degrade rather than die). `source` (e.g. `"ROADMAP.md"`)
/// is folded into the returned label so a caller that concatenates rows
/// from multiple documents can still tell them apart downstream.
pub(crate) fn parse_planning_doc_versions(text: &str, source: &str) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cells.len() < 2 {
            continue;
        }
        let label = cells[0];
        // Header row (`Phase | Name | Version`) and the `|---|---|---|`
        // separator row both have a first cell that is never a real phase
        // label — skip both rather than trying to special-case each shape.
        if label.is_empty()
            || label.eq_ignore_ascii_case("phase")
            || label.chars().all(|c| c == '-')
        {
            continue;
        }
        // The version column's position differs between ROADMAP.md's
        // `## Shipped` table (Phase | Name | Version) and STATE.md's
        // `## Completed` table (Phase | Description | Version | Date) —
        // scan every non-label cell and keep whichever ones parse as a
        // bare semver, rather than hardcoding a column index.
        for cell in &cells[1..] {
            if parse_semver(cell).is_some() {
                rows.push((format!("{source} phase {label}"), (*cell).to_string()));
            }
        }
    }
    rows
}

/// Whether `tag` exists in `project_root` AND is reachable from
/// `base_branch` — argv-array `git` shelling only (T-21b-02: tag strings
/// passed in here are already validated `^v?\d+\.\d+\.\d+$` cells, never
/// free-form; no `sh -c`). Two separate invocations, mirroring
/// `staleness::run_git_stdout`'s idiom: existence first, so a missing tag
/// short-circuits before the (more expensive) ancestry check.
pub(crate) fn tag_exists_and_reachable(project_root: &Path, tag: &str, base_branch: &str) -> bool {
    let exists = git_command(project_root)
        .args(["rev-parse", "--verify", &format!("refs/tags/{tag}")])
        .output()
        .is_ok_and(|o| o.status.success());
    exists
        && git_command(project_root)
            .args(["merge-base", "--is-ancestor", tag, base_branch])
            .output()
            .is_ok_and(|o| o.status.success())
}

/// Pure reconciliation core: for each `(label, version_cell)` row, ask the
/// caller-supplied `tag_lookup` closure whether a `v`-normalized tag exists
/// and is reachable — kept injectable (rather than calling
/// `tag_exists_and_reachable` directly) so this is unit-testable without a
/// real repository, mirroring `reconcile_phase`'s zero-I/O discipline. A
/// miss becomes a `PlanningDocFinding` at `Severity::Problem` when the
/// claimed version is at or after the `v1.5.0` cutoff — compared as a
/// NUMERIC tuple via `parse_semver`, never lexicographically — else
/// `Severity::Warn` (RESEARCH Pitfall #2). `repair` is always `None` (D-04:
/// detection-only). Skips any row whose cell doesn't parse as a semver,
/// defensively — `parse_planning_doc_versions` already filters these out,
/// but this must never panic even if called with a stray malformed row.
pub(crate) fn reconcile_planning_docs(
    rows: &[(String, String)],
    tag_lookup: &mut impl FnMut(&str) -> bool,
) -> Vec<PlanningDocFinding> {
    let mut findings = Vec::new();
    for (label, version_cell) in rows {
        let Some(parsed) = parse_semver(version_cell) else {
            continue;
        };
        let tag = if version_cell.starts_with('v') {
            version_cell.clone()
        } else {
            format!("v{version_cell}")
        };
        if tag_lookup(&tag) {
            continue;
        }
        let severity = if parsed >= PLANNING_DOC_STALENESS_CUTOFF {
            Severity::Problem
        } else {
            Severity::Warn
        };
        findings.push(PlanningDocFinding {
            source: label.clone(),
            claim: format!("{label} claims {tag}"),
            severity,
            detail: format!(
                "{label} claims {tag}, but no git tag `{tag}` exists (or it isn't reachable from the base branch)"
            ),
            repair: None,
        });
    }
    findings
}

/// Read `.planning/ROADMAP.md`'s `## Shipped` table and `.planning/STATE.md`'s
/// `## Completed` table (best-effort — a MISSING file yields no rows for
/// that document, never an error; `doctor` must not fabricate a `Problem`
/// from an absent doc), parse both, and reconcile every row against the
/// repo's git tags via `tag_exists_and_reachable(project_root, tag, MAIN)`.
/// `MAIN` is a LOCAL branch in this repo (verified: `git branch --list
/// main`; `git merge-base --is-ancestor v1.7.0 main` succeeds offline) —
/// deliberately not `origin/main` (no network dependency in `doctor`'s
/// read-only contract) and not `develop` (wrong base). The only I/O here is
/// two `std::fs::read_to_string` calls plus `tag_exists_and_reachable`'s
/// `git` subprocesses — `doctor` stays read-only (no write path to either
/// file).
fn collect_planning_doc_findings(project_root: &Path) -> Vec<PlanningDocFinding> {
    let roadmap =
        std::fs::read_to_string(project_root.join(".planning/ROADMAP.md")).unwrap_or_default();
    let state =
        std::fs::read_to_string(project_root.join(".planning/STATE.md")).unwrap_or_default();

    let mut rows = parse_planning_doc_versions(&roadmap, "ROADMAP.md");
    rows.extend(parse_planning_doc_versions(&state, "STATE.md"));

    let mut lookup = |tag: &str| tag_exists_and_reachable(project_root, tag, MAIN);
    reconcile_planning_docs(&rows, &mut lookup)
}

/// Build `doctor --json`'s `"planning_doc_staleness"` array (D-05, Pattern
/// 2), mirroring `render_reconciliation_json`'s array-building idiom.
fn render_planning_doc_findings_json(findings: &[PlanningDocFinding]) -> serde_json::Value {
    serde_json::Value::Array(
        findings
            .iter()
            .map(|f| {
                serde_json::json!({
                    "source": f.source,
                    "claim": f.claim,
                    "severity": f.severity.label(),
                    "detail": f.detail,
                    "repair": f.repair,
                })
            })
            .collect(),
    )
}

/// Build `doctor`'s text planning-docs section, printed after the
/// reconciliation section. A pure string builder (not a direct
/// `println!`), mirroring `render_reconciliation_text`'s shape so it's
/// directly assertable in tests without capturing process stdout. No
/// findings prints a single `"planning docs: consistent with git tags"`
/// line, matching the action spec.
fn render_planning_doc_text(findings: &[PlanningDocFinding]) -> String {
    if findings.is_empty() {
        return "\nplanning docs: consistent with git tags\n".to_string();
    }
    let mut out = String::from("\nplanning docs:\n");
    for finding in findings {
        out.push_str(&format!(
            "  [{}] {}\n",
            finding.severity.label(),
            finding.detail
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// doctor stray-process finding (999.44 / DEN-68)
// ---------------------------------------------------------------------------

/// Human-readable label for a [`agent::StrayLayer`], shared between
/// `doctor`'s finding and `gate_sweep`'s opt-in reaping path so the two
/// surfaces never describe the same layer with different words.
fn stray_layer_label(layer: agent::StrayLayer) -> &'static str {
    match layer {
        agent::StrayLayer::MonitorWrapper => "monitor wrapper",
        agent::StrayLayer::AdvanceChild => "advance child",
    }
}

/// The set of pids that a live registry entry still reaches: every
/// [`devflow_core::state::State::monitor_pid`] recorded under any of
/// `roots` (via [`workflow::list_states`]) plus every lock holder pid for
/// those same roots' phases (via [`lock::holder_identity`]). A pid in this
/// set is by definition not a stray (CR-01, 25-15-PLAN.md) — a live
/// registry entry, lock file, or state file still names it.
///
/// Deliberately excludes the per-phase agent pid file: the `/proc` census
/// this set filters ([`agent::discover_stray_devflow_processes`]) can only
/// ever produce a monitor-wrapper or `devflow advance` pid, never a
/// `claude`/`codex` agent process's own pid, so adding the agent pid file
/// would widen the safety set with pids the census structurally cannot
/// produce.
///
/// Read-only, always: uses [`lock::holder_identity`] (a pure read), never
/// [`lock::holder`] (which deletes an empty lock file — a write this
/// function must never perform, since it also runs on `doctor`'s strictly
/// read-only path).
///
/// RESIDUAL (T-25-15-08, not eliminated): this is a point-in-time read. A
/// root registered, or a `monitor_pid` written, between this read and a
/// caller's later signal is not covered by it. The untouched
/// `agent::is_same_process` identity re-confirmation and
/// [`agent::STRAY_MIN_AGE`] age floor downstream remain the last line of
/// defence in that window.
pub(crate) fn registry_reachable_pids(roots: &[PathBuf]) -> HashSet<u32> {
    let mut reachable = HashSet::new();
    let mut scanned = HashSet::new();
    for root in roots {
        if !scanned.insert(root.clone()) {
            // One root with N phases yields N registry entries;
            // `list_states` already covers every phase in one pass, so a
            // second entry for the same root would only repeat the scan.
            continue;
        }
        for state in workflow::list_states(root) {
            if let Some(pid) = state.monitor_pid {
                reachable.insert(pid);
            }
            if let Some((pid, _start_time)) = lock::holder_identity(root, state.phase) {
                reachable.insert(pid);
            }
        }
    }
    reachable
}

/// Pure filter: the entries in `strays` whose pid is absent from
/// `reachable`. Zero I/O — mirrors [`build_stray_process_findings`]'s own
/// pure-builder posture (split out the same way
/// [`collect_stray_process_findings`]'s I/O half is split from it), so this
/// is directly unit-testable with a synthetic stray list and a synthetic
/// reachable set, with no real orphan process required on the machine
/// running the test.
pub(crate) fn retain_unreachable_strays(
    strays: &[agent::StrayProcess],
    reachable: &HashSet<u32>,
) -> Vec<agent::StrayProcess> {
    strays
        .iter()
        .filter(|stray| !reachable.contains(&stray.pid))
        .copied()
        .collect()
}

/// Every registered root's `project_root` ([`registry::load_roots`]),
/// unioned with `extra`, deduplicated. NEVER narrowed by a caller's scope
/// argument (T-25-15-03) — this is a SAFETY set, not a scope. Narrowing it
/// to one root would un-protect every OTHER root's live processes while
/// the stray pass itself still acts machine-wide, which is a strictly
/// LARGER blast radius than leaving it machine-wide (see `25-15-PLAN.md`'s
/// `<resolved_decision>`).
///
/// Read-only: never calls [`registry::prune_missing`], which mutates the
/// registry and would break `doctor`'s read-only contract.
fn stray_safety_roots(extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = registry::load_roots()
        .into_iter()
        .map(|r| r.project_root)
        .collect();
    for root in extra {
        if !roots.contains(root) {
            roots.push(root.clone());
        }
    }
    roots
}

/// The ONE composition both `doctor` and `gate sweep --reap-strays` route
/// through, so the two surfaces' claim and action cannot drift apart
/// (T-25-15-07): the registry-independent `/proc` census
/// ([`agent::discover_stray_devflow_processes`]), filtered by
/// [`retain_unreachable_strays`] against
/// [`registry_reachable_pids`]`(&`[`stray_safety_roots`]`(extra_roots))`.
fn unreachable_stray_candidates(extra_roots: &[PathBuf]) -> Vec<agent::StrayProcess> {
    let reachable = registry_reachable_pids(&stray_safety_roots(extra_roots));
    retain_unreachable_strays(&agent::discover_stray_devflow_processes(), &reachable)
}

/// One `doctor` finding for a state-orphaned process (999.44): a process
/// [`agent::discover_stray_devflow_processes`] matched structurally, but
/// that no registry entry, lock file, or state file can reach — that
/// absence is exactly why the existing per-phase [`PhaseFinding`]s cannot
/// describe it. Machine-scoped, not phase-scoped: a stray by definition has
/// no project root, so this finding is never attached to any
/// [`PhaseFacts`]. Never carries a filesystem path (T-18-01/WR-02) — a
/// stray has no meaningful root to name, so there is nothing to redact.
pub(crate) struct StrayProcessFinding {
    pub(crate) pid: u32,
    pub(crate) layer: &'static str,
    pub(crate) severity: Severity,
    pub(crate) detail: String,
    pub(crate) repair: Option<String>,
}

/// Pure builder: turn already-discovered stray processes into `doctor`
/// findings — zero I/O, split out from [`collect_stray_process_findings`]
/// the same way [`reconcile_planning_docs`] is split from
/// `collect_planning_doc_findings`, so this is directly unit-testable with a
/// synthetic candidate list instead of requiring a real orphan process to
/// exist on the machine running the test.
pub(crate) fn build_stray_process_findings(
    strays: &[agent::StrayProcess],
) -> Vec<StrayProcessFinding> {
    strays
        .iter()
        .map(|stray| {
            let layer = stray_layer_label(stray.layer);
            StrayProcessFinding {
                pid: stray.pid,
                layer,
                severity: Severity::Problem,
                detail: format!(
                    "pid {} ({layer}) matches DevFlow's monitor-wrapper or advance-child \
                     argv shape, is owned by the calling user, and is named by no \
                     registered project root's state file or lock file",
                    stray.pid
                ),
                repair: Some(
                    "devflow gate sweep --reap-strays --dry-run (preview first; re-run \
                     without --dry-run to reap)"
                        .to_string(),
                ),
            }
        })
        .collect()
}

/// Gather `doctor`'s stray-process finding. The ONLY I/O here is
/// [`agent::discover_stray_devflow_processes`]'s read-only `/proc` census
/// (never a signal, T-25-62) and the reachable-pid computation's own
/// read-only `registry`/`lock`/`workflow` reads (CR-01, 25-15) — `doctor`
/// stays strictly read-only. `project_root` is `doctor`'s own root, unioned
/// into the reachable set via [`unreachable_stray_candidates`] so a project
/// the machine registry has not recorded still has its own live monitors
/// protected from being reported as orphans.
fn collect_stray_process_findings(project_root: &Path) -> Vec<StrayProcessFinding> {
    build_stray_process_findings(&unreachable_stray_candidates(&[project_root.to_path_buf()]))
}

/// Build `doctor --json`'s `"stray_processes"` array, mirroring
/// [`render_planning_doc_findings_json`]'s shape.
fn render_stray_process_findings_json(findings: &[StrayProcessFinding]) -> serde_json::Value {
    serde_json::Value::Array(
        findings
            .iter()
            .map(|f| {
                serde_json::json!({
                    "pid": f.pid,
                    "layer": f.layer,
                    "severity": f.severity.label(),
                    "detail": f.detail,
                    "repair": f.repair,
                })
            })
            .collect(),
    )
}

/// Build `doctor`'s text stray-process section. Unlike
/// [`render_reconciliation_text`] (which always prints a per-phase "ok"
/// line) this prints NOTHING when no stray is found — the no-stray case
/// must leave `doctor`'s existing output byte-for-byte unchanged, since a
/// machine-scoped finding with nothing to report is not a standing fact the
/// way "no active phases" is.
fn render_stray_process_text(findings: &[StrayProcessFinding]) -> String {
    if findings.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\nstray processes (state-orphaned -- no registry/lock/state file reaches them):\n",
    );
    for finding in findings {
        out.push_str(&format!("  {}\n", finding.detail));
        if let Some(repair) = &finding.repair {
            out.push_str(&format!("    repair: {repair}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cli, Command, GateCmd};
    use clap::Parser;

    /// The doctor check's rendering maps the `pi list` capability probe onto a
    /// `Check` without spawning `pi` — the pure mapping is what needs a test,
    /// the probe itself is covered by `PiDriver::capabilities()` tests
    /// (phase-39 code review, finding 5 / claude LOW #8).
    #[test]
    fn pi_subagent_dispatch_check_renders_both_arms() {
        let available = pi_subagent_dispatch_check_for(true);
        assert_eq!(available.name, "pi subagent dispatch");
        assert_eq!(available.status, "ok");
        assert_eq!(available.version.as_deref(), Some("available"));
        assert_eq!(available.install_hint, None);

        let missing = pi_subagent_dispatch_check_for(false);
        assert_eq!(missing.name, "pi subagent dispatch");
        assert_eq!(missing.status, "warn");
        assert_eq!(missing.version.as_deref(), Some("not installed"));
        assert!(missing.install_hint.is_some());
        assert!(
            missing
                .install_hint
                .as_deref()
                .is_some_and(|h| h.contains("@bacnh85/pi-subagent")),
            "the absent hint must name the vetted install command"
        );
    }

    /// 43-REVIEW.md WR-01: same pure-mapping test as
    /// `pi_subagent_dispatch_check_renders_both_arms`, for the OpenCode check
    /// this fix wires into `doctor_checks()`.
    #[test]
    fn opencode_subagent_dispatch_check_renders_both_arms() {
        let available = opencode_subagent_dispatch_check_for(true);
        assert_eq!(available.name, "opencode subagent dispatch");
        assert_eq!(available.status, "ok");
        assert_eq!(available.version.as_deref(), Some("available"));
        assert_eq!(available.install_hint, None);

        let missing = opencode_subagent_dispatch_check_for(false);
        assert_eq!(missing.name, "opencode subagent dispatch");
        assert_eq!(missing.status, "warn");
        assert_eq!(missing.version.as_deref(), Some("not configured"));
        assert!(
            missing
                .install_hint
                .as_deref()
                .is_some_and(|h| h.contains("opencode agent create")),
            "the absent hint must name the configuration command"
        );
    }

    /// 999.78/A-11, reset event ZERO — the one that must NOT happen. The
    /// per-phase Validate-failure total survives a `devflow start --force`
    /// restart of the same phase, because a bound a restart resets does not
    /// bound the unattended case D-07 exists for.
    ///
    /// Exercises the function `start()` itself calls, not a reimplementation
    /// of it — `start()`'s own body does git plumbing, agent-binary probes and
    /// worktree scaffolding that have nothing to do with the carry-forward.
    ///
    /// **The second half of the assertion is the control.** A carry-forward
    /// that copied the whole persisted state wholesale would satisfy the first
    /// assertion for entirely the wrong reason, and would silently resurrect a
    /// stale streak, a stale commit baseline and a stale stop point along with
    /// the total. Every other counter is seeded non-zero on disk here
    /// specifically so a wholesale copy cannot pass.
    #[test]
    fn phase_validate_failures_survive_a_forced_restart() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(71);

        let mut persisted = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        persisted.phase_validate_failures = 6;
        persisted.consecutive_failures = 2;
        persisted.infra_failures = 4;
        persisted.preflight_retries = 3;
        persisted.checkpoint_resumes = 1;
        persisted.last_validate_failure_commit_count = Some(9);
        persisted.stage = Stage::Validate;
        persisted.stop_until = Some(Stage::Code);
        workflow::save_state(&persisted).unwrap();

        let fresh = fresh_state_carrying_phase_failures(root, phase, AgentKind::Claude, Mode::Auto);

        assert_eq!(
            fresh.phase_validate_failures, 6,
            "the per-phase total must be carried across a forced restart — a new process \
             starting is not one of A-11's two reset events"
        );
        assert_eq!(
            fresh.consecutive_failures, 0,
            "the streak is per-run and must start at zero"
        );
        assert_eq!(fresh.infra_failures, 0);
        assert_eq!(fresh.preflight_retries, 0);
        assert_eq!(fresh.checkpoint_resumes, 0);
        assert_eq!(
            fresh.last_validate_failure_commit_count, None,
            "the commit baseline is per-run; carrying it would compare a new run's count \
             against an old run's observation"
        );
        assert_eq!(fresh.stage, Stage::Define);
        assert_eq!(fresh.stop_until, None);
    }

    /// A-11 reset event ONE: phase completion. `finish_workflow` calls
    /// `workflow::clear_state`, which deletes the phase's state file — so the
    /// next start for that phase finds nothing to carry and begins at zero.
    ///
    /// Written as clear-then-construct rather than never-write-at-all, so it is
    /// the opposite-result case for the carry-forward test above: same phase,
    /// same construction call, the ONLY difference being whether completion
    /// removed the file.
    #[test]
    fn phase_validate_failures_reset_when_the_phase_completes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(72);

        let mut persisted = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        persisted.phase_validate_failures = 8;
        workflow::save_state(&persisted).unwrap();
        assert_eq!(
            fresh_state_carrying_phase_failures(root, phase, AgentKind::Claude, Mode::Auto)
                .phase_validate_failures,
            8,
            "premise: while the state file exists, the total IS carried — otherwise the \
             assertion below proves nothing about completion"
        );

        // What `finish_workflow_with_gate_timeout` does on genuine completion.
        workflow::clear_state(root, phase).unwrap();

        assert_eq!(
            fresh_state_carrying_phase_failures(root, phase, AgentKind::Claude, Mode::Auto)
                .phase_validate_failures,
            0,
            "phase completion cleared the state, so the next start begins with a full budget"
        );
    }

    /// WR-02 (35-REVIEW): a state file that EXISTS but does not deserialize is
    /// a third case, and the old `if let Ok(..)` collapsed it into the absent
    /// case. Both still produce a zero total — the number that should have been
    /// carried is precisely what could not be read — so the ONLY observable
    /// difference is whether the operator is told, which is why the decision
    /// was split out from its `println!`.
    ///
    /// All three cases in one test with real `load_state` calls, because the
    /// pair is the measurement: an implementation that warned on every `Err`,
    /// or on none, fails one half. The absent case is the negative control and
    /// is the one a reader should check first.
    #[test]
    fn a_corrupt_state_file_warns_while_an_absent_one_is_silent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(73);

        // NEGATIVE CONTROL: nothing on disk — a genuine zero, no warning.
        let (absent_total, absent_warning) =
            carried_phase_failures(phase, workflow::load_state(root, phase));
        assert_eq!(absent_total, 0);
        assert_eq!(
            absent_warning, None,
            "a phase's first start is not an anomaly and must stay silent"
        );

        // A readable file: the total is carried and, again, nothing is said.
        let mut persisted = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        persisted.phase_validate_failures = 6;
        workflow::save_state(&persisted).unwrap();
        let (ok_total, ok_warning) =
            carried_phase_failures(phase, workflow::load_state(root, phase));
        assert_eq!(ok_total, 6);
        assert_eq!(ok_warning, None);

        // Truncate the same file in place — the shape a full disk leaves.
        let state_path = workflow::state_path(root, phase);
        assert!(
            state_path.exists(),
            "the fixture must corrupt an EXISTING file; a missing one is the case above"
        );
        std::fs::write(&state_path, "{\"phase\": 73, \"stage\":").unwrap();
        assert!(
            !matches!(
                workflow::load_state(root, phase),
                Err(workflow::WorkflowError::MissingState(_))
            ),
            "premise: a corrupt file must be a DIFFERENT error from an absent one, or \
             nothing downstream could tell them apart"
        );

        let (corrupt_total, corrupt_warning) =
            carried_phase_failures(phase, workflow::load_state(root, phase));
        assert_eq!(
            corrupt_total, 0,
            "there is no total to carry — that is the point of the warning"
        );
        let corrupt_warning =
            corrupt_warning.expect("an unreadable budget must not restart at zero silently");
        assert!(
            corrupt_warning.contains("restarts at zero"),
            "the operator must be told what the consequence is, got: {corrupt_warning:?}"
        );
    }

    #[test]
    fn gate_approve_arg_parsing_accepts_positional_stage() {
        let cli = Cli::try_parse_from(["devflow", "gate", "approve", "15", "ship"]).unwrap();
        let Command::Gate {
            action: GateCmd::Approve { stage, project, .. },
        } = cli.command
        else {
            panic!("expected gate approve command");
        };

        assert_eq!(stage.as_deref(), Some("ship"));
        assert_eq!(project, PathBuf::from("."));

        let flagged =
            Cli::try_parse_from(["devflow", "gate", "approve", "15", "--stage", "ship"]).unwrap();
        let Command::Gate {
            action:
                GateCmd::Approve {
                    stage,
                    stage_option,
                    ..
                },
        } = flagged.command
        else {
            panic!("expected flagged gate approve command");
        };
        assert_eq!(stage, None);
        assert_eq!(stage_option, Some(Stage::Ship));

        let bare = Cli::try_parse_from(["devflow", "gate", "approve", "15"]).unwrap();
        let Command::Gate {
            action:
                GateCmd::Approve {
                    stage,
                    stage_option,
                    ..
                },
        } = bare.command
        else {
            panic!("expected bare gate approve command");
        };
        assert_eq!(stage, None);
        assert_eq!(stage_option, None);

        let legacy =
            Cli::try_parse_from(["devflow", "gate", "approve", "15", "/tmp/example-project"])
                .unwrap();
        let Command::Gate {
            action:
                GateCmd::Approve {
                    stage,
                    legacy_project,
                    stage_option,
                    project,
                    ..
                },
        } = legacy.command
        else {
            panic!("expected legacy gate approve command");
        };
        let (stage, project) =
            resolve_gate_target(stage, legacy_project, stage_option, project).unwrap();
        assert_eq!(stage, None);
        assert_eq!(project, PathBuf::from("/tmp/example-project"));
    }

    #[test]
    fn gate_show_arg_parsing_accepts_phase_and_optional_stage() {
        let bare = Cli::try_parse_from(["devflow", "gate", "show", "15"]).unwrap();
        let Command::Gate {
            action: GateCmd::Show { phase, stage, .. },
        } = bare.command
        else {
            panic!("expected gate show command");
        };
        assert_eq!(phase, PhaseId::new(15));
        assert_eq!(stage, None);

        let flagged =
            Cli::try_parse_from(["devflow", "gate", "show", "15", "--stage", "ship"]).unwrap();
        let Command::Gate {
            action: GateCmd::Show { phase, stage, .. },
        } = flagged.command
        else {
            panic!("expected gate show command with stage");
        };
        assert_eq!(phase, PhaseId::new(15));
        assert_eq!(stage, Some(Stage::Ship));
    }

    #[test]
    fn gate_show_renders_full_untruncated_sanitized_context() {
        let dir = tempfile::tempdir().unwrap();
        let context = format!("first line\n\u{1b}[2J{}", "x".repeat(150));
        Gates::write_gate(dir.path(), PhaseId::new(15), Stage::Ship, &context).unwrap();
        let gate = Gates::list_open(dir.path())
            .into_iter()
            .find(|g| g.phase == PhaseId::new(15))
            .unwrap();

        let rendered = render_gate_show(&gate);

        assert!(rendered.contains(&"x".repeat(150)));
        assert!(!rendered.contains("[truncated"));
        assert!(!rendered.contains('\u{1b}'));
    }

    #[test]
    fn gate_show_errors_naming_gate_list_when_no_open_gate() {
        let dir = tempfile::tempdir().unwrap();
        let err = gate_show(dir.path(), PhaseId::new(15), None).unwrap_err();
        assert!(err.to_string().contains("devflow gate list"));
    }

    #[test]
    fn gate_show_errors_asking_for_stage_with_several_open_gates() {
        let dir = tempfile::tempdir().unwrap();
        Gates::write_gate(dir.path(), PhaseId::new(15), Stage::Ship, "ctx1").unwrap();
        Gates::write_gate(dir.path(), PhaseId::new(15), Stage::Validate, "ctx2").unwrap();

        let err = gate_show(dir.path(), PhaseId::new(15), None).unwrap_err();

        assert!(err.to_string().contains("--stage"));
    }

    #[test]
    fn gate_show_auto_resolves_single_open_gate() {
        let dir = tempfile::tempdir().unwrap();
        Gates::write_gate(
            dir.path(),
            PhaseId::new(15),
            Stage::Ship,
            "the only open gate",
        )
        .unwrap();

        assert!(gate_show(dir.path(), PhaseId::new(15), None).is_ok());
    }

    #[test]
    fn describe_worktree_dir_infers_phase_and_agent() {
        assert_eq!(
            describe_worktree_dir("phase-07-claude"),
            " — phase 7, agent claude"
        );
        assert_eq!(describe_worktree_dir("phase-08"), " — phase 8");
        assert_eq!(describe_worktree_dir("reference"), "");
    }

    #[test]
    fn cron_instruction_hints_include_hermes_command_per_phase() {
        let dir = tempfile::tempdir().unwrap();
        for phase in [PhaseId::new(7), PhaseId::new(9)] {
            let instructions = devflow_core::ship::build_single_agent_cron_instructions(
                dir.path(),
                phase,
                "2026-06-18T15:45:30Z",
            );
            devflow_core::ship::write_cron_instructions(dir.path(), &instructions).unwrap();
        }

        let hints = cron_instruction_hints(dir.path());

        assert_eq!(hints.len(), 2);
        assert!(hints[0].contains("hermes cron create \"2026-06-18T15:46:00Z\""));
        assert!(hints[0].contains("--repeat 1 --name devflow-phase-07-resume"));
        assert!(hints[1].contains("(phase 9)"));
    }

    #[test]
    fn cron_hint_line_appends_sanitized_reset_when_retry_after_present() {
        let dir = tempfile::tempdir().unwrap();
        let instructions = devflow_core::ship::build_single_agent_cron_instructions(
            dir.path(),
            PhaseId::new(7),
            "2026-06-18T15:45:30Z",
        );

        let hint = cron_hint_line(&instructions);

        assert!(hint.contains("hermes cron create \"2026-06-18T15:46:00Z\""));
        assert!(hint.contains("--repeat 1 --name devflow-phase-07-resume"));
        assert!(hint.contains("(rate-limit resets: 2026-06-18T15:45:30Z)"));
    }

    #[test]
    fn cron_hint_line_omits_reset_fragment_when_retry_after_empty() {
        let dir = tempfile::tempdir().unwrap();
        let instructions = devflow_core::ship::build_single_agent_cron_instructions(
            dir.path(),
            PhaseId::new(7),
            "unknown",
        );

        let hint = cron_hint_line(&instructions);

        assert!(hint.contains("resume manually with: devflow resume --phase 7"));
        assert!(!hint.contains("hermes cron create"));
    }

    #[test]
    fn cron_hint_line_never_emits_the_unsupported_devflow_intake_flag() {
        let dir = tempfile::tempdir().unwrap();
        let instructions = devflow_core::ship::build_single_agent_cron_instructions(
            dir.path(),
            PhaseId::new(7),
            "2026-06-18T15:45:30Z",
        );
        assert!(!cron_hint_line(&instructions).contains("--from-devflow"));
    }

    /// Regression for 44-CORE-REVIEW-FINDINGS.md finding 1: `ship.rs` used to
    /// pre-quote the project path inside `hermes_cron.command`, and this
    /// function then wrapped that already-quoted command in a second, raw
    /// quote layer. POSIX single quotes don't nest, so a path containing a
    /// space split into unquoted word fragments, and a path containing an
    /// apostrophe closed the outer quote early (a shell-injection vector).
    /// This asserts the fix holds: `cron_hint_line` embeds `shell_quote`'s
    /// output verbatim (no re-wrap), and the result round-trips through a
    /// real shell back to the original, unquoted command.
    #[test]
    fn cron_hint_line_command_quoting_roundtrips_through_shell_for_space_and_apostrophe_paths() {
        for raw_path in ["/tmp/My Project", "/tmp/o'connor/repo"] {
            let instructions = devflow_core::ship::build_single_agent_cron_instructions(
                Path::new(raw_path),
                PhaseId::new(7),
                "2026-06-18T15:45:30Z",
            );
            let hint = cron_hint_line(&instructions);
            let quoted = devflow_core::ship::shell_quote(&instructions.hermes_cron.command);

            assert!(
                hint.contains(&quoted),
                "hint does not embed the singly-quoted command verbatim for {raw_path:?}: {hint}"
            );

            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("printf '%s' {quoted}"))
                .output()
                .expect("sh must be available to run this test");
            assert!(
                output.status.success(),
                "sh failed to parse quoted command for {raw_path:?}: {quoted}"
            );
            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                instructions.hermes_cron.command,
                "quoting did not round-trip for {raw_path:?}"
            );
        }
    }

    #[test]
    fn default_logs_phase_prefers_single_active_state() {
        let dir = tempfile::tempdir().unwrap();
        let state = State::new(
            PhaseId::new(6),
            AgentKind::Claude,
            Mode::Auto,
            dir.path().to_path_buf(),
        );
        workflow::save_state(&state).unwrap();

        assert_eq!(default_logs_phase(dir.path()).unwrap(), PhaseId::new(6));
    }

    #[test]
    fn default_logs_phase_is_ambiguous_with_two_active_states() {
        let dir = tempfile::tempdir().unwrap();
        for phase in [PhaseId::new(6), PhaseId::new(7)] {
            let state = State::new(
                phase,
                AgentKind::Claude,
                Mode::Auto,
                dir.path().to_path_buf(),
            );
            workflow::save_state(&state).unwrap();
        }

        let err = default_logs_phase(dir.path()).unwrap_err();
        assert!(err.to_string().contains("--phase"));
    }

    #[test]
    fn default_logs_phase_falls_back_to_newest_capture_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            agent_result::stdout_path(dir.path(), PhaseId::new(3)),
            "old",
        )
        .unwrap();
        // Ensure a strictly newer mtime on the second capture.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            agent_result::stdout_path(dir.path(), PhaseId::new(5)),
            "new",
        )
        .unwrap();

        assert_eq!(default_logs_phase(dir.path()).unwrap(), PhaseId::new(5));
    }

    #[test]
    fn default_logs_phase_errors_with_nothing_to_show() {
        let dir = tempfile::tempdir().unwrap();
        assert!(default_logs_phase(dir.path()).is_err());
    }

    /// 18b: a state with no recorded monitor is never reported as stuck,
    /// regardless of the (unreliable, since no monitor was ever recorded)
    /// liveness bits passed alongside it.
    #[test]
    fn liveness_unknown_when_no_monitor_recorded() {
        assert_eq!(liveness(None, false, false), Liveness::Unknown);
        assert_eq!(liveness(None, false, true), Liveness::Unknown);
        assert_eq!(liveness(None, true, false), Liveness::Unknown);
        assert_eq!(liveness(None, true, true), Liveness::Unknown);
    }

    /// 18b: the full four-row matrix for a recorded monitor pid. A dead
    /// agent with a dead monitor OR a live monitor with a dead agent are
    /// different states — only the former is `Stuck` (nothing will call
    /// `devflow advance`); the latter is a normal between-stages moment. An
    /// agent that outlived its monitor is also `Stuck` — orphaned, since
    /// nothing will advance it when it exits either.
    #[test]
    fn liveness_matrix_covers_all_four_rows() {
        let pid = Some(4242);
        assert_eq!(liveness(pid, true, true), Liveness::Healthy);
        assert_eq!(liveness(pid, true, false), Liveness::BetweenStages);
        assert_eq!(liveness(pid, false, false), Liveness::Stuck);
        assert_eq!(liveness(pid, false, true), Liveness::Stuck);
    }

    /// 18b: a corrupt pid (0, or above `i32::MAX`) must never read as alive
    /// — `liveness` relies entirely on `agent::agent_running`'s existing
    /// hardening (no second probe is written), so it can only ever produce
    /// `Stuck` or `Unknown` for a corrupt pid, never a false `Healthy`.
    #[test]
    fn liveness_treats_zero_and_overflow_pids_as_dead() {
        assert!(!agent::agent_running(0));
        assert!(!agent::agent_running(u32::MAX));
    }

    /// 18b: persisting `monitor_pid` for one phase must not disturb a
    /// concurrently-active sibling phase's `monitor_pid` (concurrency edge).
    #[test]
    fn monitor_pid_persisted_for_one_phase_does_not_disturb_a_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let mut phase7 = State::new(
            PhaseId::new(7),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
        phase7.monitor_pid = Some(111);
        workflow::save_state(&phase7).unwrap();

        let mut phase8 = State::new(
            PhaseId::new(8),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
        phase8.monitor_pid = Some(222);
        workflow::save_state(&phase8).unwrap();

        let reloaded7 = workflow::load_state(root, PhaseId::new(7)).unwrap();
        let reloaded8 = workflow::load_state(root, PhaseId::new(8)).unwrap();
        assert_eq!(reloaded7.monitor_pid, Some(111));
        assert_eq!(reloaded8.monitor_pid, Some(222));
    }

    /// 18b (idempotency edge): running `devflow status` twice must produce
    /// byte-identical `.devflow/` state — the new monitor liveness probe is
    /// purely a read, same as the existing agent liveness probe it sits
    /// beside. Also exercises the `u32::MAX` boundary pid (precision edge,
    /// via `agent::agent_running`'s existing hardening) so the probe can
    /// only ever report `Stuck`, never a false `Healthy`.
    #[test]
    fn status_reading_monitor_liveness_writes_no_state_and_no_event() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(66);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.monitor_pid = Some(u32::MAX);
        workflow::save_state(&state).unwrap();

        let state_path = workflow::state_path(root, phase);
        let before_len = std::fs::metadata(&state_path).unwrap().len();
        let before_modified = std::fs::metadata(&state_path).unwrap().modified().unwrap();
        let events_log = events::events_path(root);
        let before_lines = std::fs::read_to_string(&events_log)
            .unwrap_or_default()
            .lines()
            .count();

        status(root).unwrap();
        status(root).unwrap();

        let after_len = std::fs::metadata(&state_path).unwrap().len();
        let after_modified = std::fs::metadata(&state_path).unwrap().modified().unwrap();
        let after_lines = std::fs::read_to_string(&events_log)
            .unwrap_or_default()
            .lines()
            .count();

        assert_eq!(
            before_len, after_len,
            "status must not rewrite the state file"
        );
        assert_eq!(
            before_modified, after_modified,
            "status must not touch the state file's mtime"
        );
        assert_eq!(
            before_lines, after_lines,
            "status must not append to events.jsonl"
        );
    }

    /// 15a: `devflow gate approve` resolves the stage automatically when a
    /// phase has exactly one open gate and writes a response the workflow's
    /// poller will consume.
    #[test]
    fn gate_respond_auto_resolves_single_open_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        Gates::write_gate(root, PhaseId::new(15), Stage::Ship, "approve merge?").unwrap();

        gate_respond(root, PhaseId::new(15), None, true, Some("lgtm".into())).unwrap();

        let polled = Gates::poll_response(root, PhaseId::new(15), Stage::Ship, 1)
            .expect("response readable");
        assert!(polled.approved);
        assert_eq!(polled.note.as_deref(), Some("lgtm"));
        let event = devflow_core::events::last_event_for_phase(root, PhaseId::new(15)).unwrap();
        assert_eq!(event["event"], "gate_response_written");
        assert_eq!(event["stage"], "ship");
    }

    #[test]
    fn gate_respond_requires_stage_when_ambiguous_and_errors_when_none_open() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let err = gate_respond(root, PhaseId::new(15), None, true, None).unwrap_err();
        assert!(err.to_string().contains("no open gate"), "{err}");

        Gates::write_gate(root, PhaseId::new(15), Stage::Validate, "a").unwrap();
        Gates::write_gate(root, PhaseId::new(15), Stage::Ship, "b").unwrap();
        let err =
            gate_respond(root, PhaseId::new(15), None, false, Some("nope".into())).unwrap_err();
        assert!(err.to_string().contains("--stage"), "{err}");

        // Explicit --stage disambiguates.
        gate_respond(
            root,
            PhaseId::new(15),
            Some(Stage::Validate),
            false,
            Some("gaps".into()),
        )
        .unwrap();
        assert!(
            Gates::response_path(root, PhaseId::new(15), Stage::Validate).exists(),
            "explicit-stage rejection must land"
        );
        assert!(!Gates::response_path(root, PhaseId::new(15), Stage::Ship).exists());
    }

    /// Backdate an already-written gate's `timestamp` so it reads as
    /// `age_secs` old — the deterministic way `gate_sweep` tests make a gate
    /// look abandoned without sleeping.
    /// An age comfortably past whatever the sweep's threshold currently is,
    /// derived from the same default `gate_sweep` reads instead of hard-coded.
    ///
    /// The literal `7 * 60 * 60` these tests used before outlived the six-hour
    /// default it had been chosen against. When the threshold moved to three
    /// days (equality with the gate poll timeout, so a sweep cannot reap a
    /// gate a live monitor is still polling), a seven-hour backdate stopped
    /// reaching the reap path entirely — and only ONE of the three tests below
    /// noticed. The other two kept passing while asserting nothing, because
    /// "no response was written" is equally true of a gate the sweep declined
    /// to consider.
    fn aged_past_threshold() -> u64 {
        config_parse::gate_max_unattended_age_secs() + 60 * 60
    }

    fn backdate_gate(root: &Path, phase: PhaseId, stage: Stage, age_secs: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let gate = devflow_core::gates::GateFile {
            phase,
            stage,
            context: "ctx".to_string(),
            timestamp: now.saturating_sub(age_secs).to_string(),
        };
        std::fs::write(
            Gates::gate_path(root, phase, stage),
            serde_json::to_string_pretty(&gate).unwrap(),
        )
        .unwrap();
    }

    /// Task 2 behavior: `--dry-run` computes and prints every decision but
    /// writes nothing — the aged gate stays open with no response file.
    #[test]
    fn gate_sweep_dry_run_does_not_write_a_response() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        Gates::write_gate(root, PhaseId::new(30), Stage::Ship, "ctx").unwrap();
        backdate_gate(root, PhaseId::new(30), Stage::Ship, aged_past_threshold());

        gate_sweep(None, true, Some(root.to_path_buf()), false).unwrap();

        assert!(
            !Gates::response_path(root, PhaseId::new(30), Stage::Ship).exists(),
            "dry-run must never write a response file"
        );
        let open = Gates::list_open(root);
        assert_eq!(open.len(), 1, "dry-run must leave the gate open");
    }

    /// Task 2 behavior: a gate that already has a response is a benign race
    /// (a human or `--yes-ship` may have answered it between `list_open`
    /// and `reap`) — the sweep must return `Ok` and leave the existing
    /// response byte-for-byte untouched, never overwrite it.
    #[test]
    fn gate_sweep_skips_already_responded_gate_without_clobbering() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        Gates::write_gate(root, PhaseId::new(31), Stage::Ship, "ctx").unwrap();
        backdate_gate(root, PhaseId::new(31), Stage::Ship, aged_past_threshold());
        let response = GateResponse {
            approved: true,
            note: None,
            responded_by: Some("human".into()),
        };
        Gates::respond(root, PhaseId::new(31), Stage::Ship, &response).unwrap();
        let before =
            std::fs::read_to_string(Gates::response_path(root, PhaseId::new(31), Stage::Ship))
                .unwrap();

        let result = gate_sweep(None, false, Some(root.to_path_buf()), false);

        assert!(result.is_ok(), "an already-answered gate must not error");
        let after =
            std::fs::read_to_string(Gates::response_path(root, PhaseId::new(31), Stage::Ship))
                .unwrap();
        assert_eq!(
            before, after,
            "the pre-existing response must be byte-identical afterwards"
        );
    }

    /// Task 2 behavior (T-23-43 audit trail): a successful reap emits
    /// `gate_reaped` in the sweep's own process — one of the two
    /// independent, attributed records a reaped gate leaves.
    #[test]
    fn gate_sweep_emits_gate_reaped_event_on_reap() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        Gates::write_gate(root, PhaseId::new(32), Stage::Ship, "ctx").unwrap();
        backdate_gate(root, PhaseId::new(32), Stage::Ship, aged_past_threshold());

        gate_sweep(None, false, Some(root.to_path_buf()), false).unwrap();

        let event = devflow_core::events::last_event_for_phase(root, PhaseId::new(32)).unwrap();
        assert_eq!(event["event"], "gate_reaped");
        assert_eq!(event["stage"], "ship");
    }

    /// Wrap a real, test-owned pid as the `agent::StrayProcess` shape
    /// `discover_stray_devflow_processes` would have produced for it,
    /// WITHOUT actually scanning the whole machine's `/proc` — 999.44's own
    /// per-test safety rule: a reaping test must never act on anything it
    /// did not spawn itself, and this machine's live process table
    /// legitimately contains other, unrelated devflow activity while these
    /// tests run.
    fn stray_candidate_for(pid: u32, layer: agent::StrayLayer) -> agent::StrayProcess {
        agent::StrayProcess {
            pid,
            start_time: agent::process_start_time(pid)
                .expect("must be able to read the fixture's own recorded start time"),
            layer,
        }
    }

    /// Task 2 behavior: `--dry-run` computes and reports the outcome but
    /// signals nothing — mirrors the existing gate-reaping dry-run
    /// contract, extended to strays.
    #[test]
    fn reap_stray_candidates_dry_run_never_signals() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep fixture");
        let pid = child.id();
        let candidate = stray_candidate_for(pid, agent::StrayLayer::MonitorWrapper);

        // 25-12: floor deliberately disabled (Duration::ZERO) — this test
        // asserts the dry-run signalling behavior, not the age gate,
        // which Task 1's dedicated process_age tests cover.
        let results = reap_stray_candidates(&[candidate], true, std::time::Duration::ZERO);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].outcome, StrayReapOutcome::Reaped);
        assert!(
            agent::agent_running(pid),
            "dry-run must never actually signal the candidate"
        );

        let _ = child.kill();
        let _ = child.wait();
    }

    /// Task 2 behavior: a real candidate is cleared with VERIFIED death —
    /// counted as reaped only once `agent::terminate_and_verify` confirms
    /// it, never on the basis of a signal alone (999.44's own lesson: an
    /// unverified `SIGTERM` is what makes the CURRENT recovery path report
    /// success while the process keeps running).
    #[test]
    fn reap_stray_candidates_clears_a_real_child_with_verified_death() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep fixture");
        let pid = child.id();
        let candidate = stray_candidate_for(pid, agent::StrayLayer::AdvanceChild);

        // 25-12: floor deliberately disabled (Duration::ZERO) — this test
        // asserts the signalling behavior, not the age gate, which
        // Task 1's dedicated process_age tests cover.
        let results = reap_stray_candidates(&[candidate], false, std::time::Duration::ZERO);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].outcome, StrayReapOutcome::Reaped);
        assert!(
            !agent::agent_running(pid),
            "a successful reap must leave the candidate verified dead, not merely signalled"
        );

        let _ = child.wait();
    }

    /// D-17's escalation, exercised through `gate_sweep`'s own reaping
    /// core (`reap_stray_candidates`) rather than the raw
    /// `terminate_and_verify` primitive 25-02 already covers directly — a
    /// `SIGTERM`-ignoring child must still be cleared, via `SIGKILL`,
    /// within the bounded wait.
    #[test]
    fn reap_stray_candidates_escalates_to_kill_for_a_term_ignoring_child() {
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("trap '' TERM; sleep 30")
            .spawn()
            .expect("spawn TERM-ignoring fixture");
        let pid = child.id();
        // Give the shell a moment to install its trap before signalling.
        std::thread::sleep(std::time::Duration::from_millis(100));
        let candidate = stray_candidate_for(pid, agent::StrayLayer::MonitorWrapper);

        // 25-12: floor deliberately disabled (Duration::ZERO) — this test
        // asserts the SIGKILL escalation behavior, not the age gate,
        // which Task 1's dedicated process_age tests cover.
        let results = reap_stray_candidates(&[candidate], false, std::time::Duration::ZERO);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].outcome, StrayReapOutcome::Reaped);
        assert!(
            !agent::agent_running(pid),
            "a TERM-ignoring candidate must still be cleared via SIGKILL escalation"
        );

        let _ = child.wait();
    }

    /// The safety-critical case (999.47's "Related TOCTOU"): a candidate
    /// whose recorded start time no longer matches at signal time — the pid
    /// could have been recycled between discovery and this pass — must be
    /// refused, counted separately from a successful reap, and, the
    /// assertion that actually matters, the live process behind that pid
    /// must NOT be signalled.
    #[test]
    fn reap_stray_candidates_refuses_on_identity_mismatch_without_signalling() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep fixture");
        let pid = child.id();
        let real_start = agent::process_start_time(pid).expect("read real start time");
        let mismatched = agent::StrayProcess {
            pid,
            start_time: real_start.wrapping_add(1),
            layer: agent::StrayLayer::MonitorWrapper,
        };

        // 25-12: floor deliberately disabled (Duration::ZERO) — this test
        // asserts the identity re-confirmation, not the age gate, which
        // Task 1's dedicated process_age tests cover.
        let results = reap_stray_candidates(&[mismatched], false, std::time::Duration::ZERO);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].outcome, StrayReapOutcome::IdentityMismatch);
        assert!(
            agent::agent_running(pid),
            "an identity mismatch must never be signalled — the whole point of the \
             re-confirmation"
        );

        let _ = child.kill();
        let _ = child.wait();
    }

    /// Task 1's whole point (25-12/999.47, production half): a candidate
    /// caught inside its own `fork()`->`execve()` window is genuinely the
    /// same process with genuinely the same recorded start time as its
    /// parent, so `is_same_process` alone cannot refuse it — the age floor
    /// must. The assertion that actually matters is not the outcome enum
    /// alone (a `TooYoung` outcome that still signalled would satisfy an
    /// enum-only check) but that the fixture is still alive afterwards.
    #[test]
    fn reap_stray_candidates_refuses_a_candidate_younger_than_the_minimum_age() {
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("trap cleanup TERM INT; sleep 30")
            .spawn()
            .expect("spawn monitor-wrapper-shaped fixture");
        let pid = child.id();

        // 999.47: cross the exec-visibility barrier before building the
        // candidate from a cmdline-derived census, or this test races the
        // fixture's own fork()->execve() window (25-11).
        assert!(
            devflow_core::test_support::wait_for_exec_visibility(
                pid,
                "sh",
                devflow_core::test_support::EXEC_VISIBILITY_WAIT,
                devflow_core::test_support::EXEC_VISIBILITY_POLL,
            ),
            "pid {pid}: exec visibility timed out before the fixture became discoverable"
        );

        let candidate = stray_candidate_for(pid, agent::StrayLayer::MonitorWrapper);

        let results = reap_stray_candidates(&[candidate], false, agent::STRAY_MIN_AGE);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].outcome, StrayReapOutcome::TooYoung);
        assert!(
            agent::agent_running(pid),
            "a candidate refused for youth must never actually be signalled"
        );

        let _ = child.kill();
        let _ = child.wait();
    }

    /// The floor is a floor, not a blanket refusal: the same fixture shape
    /// as the test above, but with the floor disabled, IS reaped — without
    /// this, the test above would pass equally well against a reaper that
    /// refused unconditionally.
    #[test]
    fn reap_stray_candidates_reaps_when_the_floor_is_zero() {
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("trap cleanup TERM INT; sleep 30")
            .spawn()
            .expect("spawn monitor-wrapper-shaped fixture");
        let pid = child.id();

        assert!(
            devflow_core::test_support::wait_for_exec_visibility(
                pid,
                "sh",
                devflow_core::test_support::EXEC_VISIBILITY_WAIT,
                devflow_core::test_support::EXEC_VISIBILITY_POLL,
            ),
            "pid {pid}: exec visibility timed out before the fixture became discoverable"
        );

        let candidate = stray_candidate_for(pid, agent::StrayLayer::MonitorWrapper);

        let results = reap_stray_candidates(&[candidate], false, std::time::Duration::ZERO);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].outcome, StrayReapOutcome::Reaped);
        assert!(
            !agent::agent_running(pid),
            "with the floor disabled, a genuine candidate must still be reaped and verified dead"
        );

        let _ = child.wait();
    }

    /// Task 2 behavior: fail-closed on an unresolvable age. Constructed
    /// from a dead pid (the same shape
    /// `stop_is_a_success_no_op_when_the_lock_names_a_dead_pid` uses:
    /// above the default kernel `pid_max`, guaranteed not alive) — this
    /// documents the actual interaction rather than assuming one:
    /// `agent::process_age` DOES return `None` for it, but
    /// `agent::is_same_process` is evaluated FIRST in
    /// `reap_stray_candidates` and ALSO fails for the identical reason
    /// (both derive from `agent::process_start_time`), so this candidate
    /// is classified `IdentityMismatch`, not `TooYoung` — the age check is
    /// never reached. That is not a design gap: an unconfirmable identity
    /// is refused regardless of which guard catches it first.
    ///
    /// The `TooYoung`-via-unresolvable-age arm specifically (identity
    /// re-confirmed alive, but `process_age` itself returns `None`)
    /// requires `/proc/uptime` or `sysconf(_SC_CLK_TCK)` to fail while
    /// `/proc/<pid>/stat` succeeds for the SAME live pid — a combination
    /// no black-box process fixture in this suite can construct without
    /// faking `/proc`. Left uncovered by assertion, following this file's
    /// own established precedent for an unreachable-by-black-box-test
    /// match arm (`stop_via_lock`'s wildcard arm, documented above this
    /// test group rather than faked). Covered by source reasoning
    /// instead: `reap_stray_candidates`'s age check —
    /// `agent::process_age(candidate.pid).is_some_and(|age| age >= min_age)`
    /// — treats `None` as `false` by construction of `Option::is_some_and`,
    /// so an unresolvable age can never accidentally satisfy the `>=`
    /// comparison and fall through to `Reaped`.
    #[test]
    fn reap_stray_candidates_refuses_a_dead_pid_as_identity_mismatch_before_the_age_check_runs() {
        let dead_pid = 9_999_999;
        assert!(
            agent::process_age(dead_pid).is_none(),
            "precondition: a dead pid's age must be unresolvable"
        );
        let candidate = agent::StrayProcess {
            pid: dead_pid,
            start_time: 0,
            layer: agent::StrayLayer::MonitorWrapper,
        };

        let results = reap_stray_candidates(&[candidate], false, agent::STRAY_MIN_AGE);

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].outcome,
            StrayReapOutcome::IdentityMismatch,
            "is_same_process is evaluated before the age check and fails first for a dead pid"
        );
    }

    /// Flag wiring, exercised through the real `gate_sweep` entry point.
    /// `--dry-run` is provably safe to run against the live machine (it
    /// never signals anything, no matter what else is discovered) — this
    /// is the one test in this file that drives the actual CLI-facing
    /// function with `reap_strays: true` rather than the injectable core
    /// directly, deliberately avoiding a non-dry-run invocation: this
    /// machine's live process table legitimately contains other, unrelated
    /// devflow activity (concurrent phases in sibling worktrees) while
    /// these tests run, and a non-dry-run sweep is registry-independent by
    /// design — it would act on that too, not just this fixture.
    #[test]
    fn gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling() {
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("trap cleanup TERM INT; sleep 30")
            .spawn()
            .expect("spawn monitor-wrapper-shaped fixture");
        let pid = child.id();
        let dir = tempfile::tempdir().unwrap();

        // 999.47: `spawn()` returns after `fork()`, before `execve()` — cross
        // the exec-visibility barrier before reading the cmdline-derived
        // census, or this assertion races the child's own exec (25-11).
        assert!(
            devflow_core::test_support::wait_for_exec_visibility(
                pid,
                "sh",
                devflow_core::test_support::EXEC_VISIBILITY_WAIT,
                devflow_core::test_support::EXEC_VISIBILITY_POLL,
            ),
            "pid {pid}: exec visibility timed out before the fixture became discoverable"
        );

        assert!(
            agent::discover_stray_devflow_processes()
                .iter()
                .any(|p| p.pid == pid),
            "the fixture must be part of the real discovery census gate_sweep would use"
        );

        gate_sweep(None, true, Some(dir.path().to_path_buf()), true).unwrap();

        assert!(
            agent::agent_running(pid),
            "--dry-run must never signal a discovered stray, no matter what else the machine \
             is running"
        );

        let _ = child.kill();
        let _ = child.wait();
    }

    /// Task 2 behavior: without the flag, `gate_sweep` never even looks at
    /// the process table — a live stray-shaped fixture is neither
    /// discovered nor touched, and `gate_sweep`'s existing behavior stays
    /// byte-for-byte unchanged.
    #[test]
    fn gate_sweep_without_reap_strays_flag_ignores_a_live_stray() {
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("trap cleanup TERM INT; sleep 30")
            .spawn()
            .expect("spawn monitor-wrapper-shaped fixture");
        let pid = child.id();
        let dir = tempfile::tempdir().unwrap();

        gate_sweep(None, false, Some(dir.path().to_path_buf()), false).unwrap();

        assert!(
            agent::agent_running(pid),
            "gate_sweep without --reap-strays must never signal anything discoverable only \
             via the process table"
        );

        let _ = child.kill();
        let _ = child.wait();
    }

    /// Retargeted (999.47/D-13). This test used to spawn a real `sleep`
    /// child and assert `stop()` refused to signal it, instrumented with
    /// the now-deprecated cmdline-basename predicate and a page of
    /// `/proc`-forensics helpers built to diagnose a CI flake
    /// ("MECHANISM CONFIRMED 2026-07-26": between `spawn()` returning and
    /// the child completing `execve`, its cmdline transiently reads as its
    /// parent's, which is what made a cmdline-based guard race). It is the
    /// serious one of 999.47's two tests: in CI it panicked on the error
    /// expectation, meaning the identity guard passed and `devflow stop`
    /// sent `SIGTERM` to an unrelated process — the guard's stated purpose
    /// failing end-to-end, not merely inferred.
    ///
    /// The retarget removes the race by construction rather than making it
    /// rarer: it asserts the `(pid, starttime)` guard `stop_via_lock`
    /// ACTUALLY uses today (the deprecated predicate is no longer even
    /// consulted for a decision — the cmdline-based mechanism it embodied
    /// was superseded before this retarget, and D-13 records that
    /// tightening it further to a single argv position was considered and
    /// rejected as ineffective, since the breaking marker sits inside the
    /// same inherited data). A LEGACY lock file — recording a pid with no
    /// start time at all, the format every lock file had before identity
    /// recording existed — must be refused, because identity cannot be
    /// confirmed for it. Uses this test's own pid, which is genuinely alive
    /// throughout, so there is no `spawn()` and therefore no `execve` to
    /// race.
    ///
    /// This is one of `stop_via_lock`'s three fail-closed match arms. The
    /// other two:
    /// - a recorded start time that no longer matches — covered
    ///   deterministically, no-spawn, by
    ///   `stop_refuses_when_the_recorded_start_time_does_not_match` below.
    /// - the match's final wildcard arm ("the lock file's holder could not
    ///   be read back for identity confirmation") — NOT exercised by any
    ///   test in this file. Source analysis: by the time `stop_via_lock`
    ///   reaches this match, `lock::holder()` has already confirmed this
    ///   SAME lock file's first line parses as a pid, via the identical
    ///   `read_holder_pid` helper `lock::holder_identity` calls again
    ///   moments later over UNCHANGED file content — so `holder_identity`'s
    ///   `recorded_pid` is guaranteed to equal the pid already in hand, and
    ///   its `Option<u64>` half is caught by the `Some((_, None))` arm
    ///   whenever it's absent. The wildcard arm is reachable only if the
    ///   lock file's content changes between those two sequential reads —
    ///   a genuine external race that no deterministic black-box test of
    ///   `stop()` can construct without reintroducing exactly the class of
    ///   flake this retarget exists to remove. Recorded here rather than
    ///   faked with a timing-dependent fixture.
    #[test]
    fn stop_refuses_to_signal_a_live_pid_that_fails_the_identity_check() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(200);

        // Our own pid is genuinely alive throughout — no spawn, so no
        // execve to race. The lock records ONLY the pid, matching every
        // lock file written before start times were recorded (999.47).
        let pid = std::process::id();
        let lock_path = root
            .join(".devflow")
            .join(format!("lock-{padded}", padded = phase.padded()));
        std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        std::fs::write(&lock_path, pid.to_string()).unwrap();

        // Confirm the fixture is genuinely the legacy shape this test
        // exercises — `lock::holder_identity` (what `stop_via_lock` itself
        // consults) must report the pid with NO recorded start time, the
        // exact input that drives `stop_via_lock`'s `Some((_, None))` arm.
        assert_eq!(
            lock::holder_identity(root, phase),
            Some((pid, None)),
            "the fixture must be a legacy lock (pid recorded, no start time) — the shape \
             stop_via_lock's identity guard treats as unconfirmable"
        );

        let err = stop(root, phase)
            .expect_err("a legacy lock with no recorded start time must be refused");
        let message = err.to_string();
        assert!(
            message.contains(&pid.to_string()),
            "error must name the pid it refused to signal, got: {message}"
        );
        assert!(
            message.contains("records no start time"),
            "error must say identity cannot be confirmed for a legacy lock, got: {message}"
        );
        assert!(
            lock_path.exists(),
            "the lock file must be untouched — stop must not signal anything"
        );
    }

    /// 999.47: a lock recording a start time that does not match the live
    /// process must be refused. This is the pid-recycling case — the lock
    /// named pid N, N died, and something unrelated now holds it.
    ///
    /// Unlike the cmdline check this replaced, the verdict does not depend
    /// on what the process *looks* like, so a `sleep` and a real devflow
    /// binary are rejected identically.
    #[test]
    fn stop_refuses_when_the_recorded_start_time_does_not_match() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(201);

        // Our own pid is genuinely alive and genuinely devflow-named, so the
        // old cmdline check would have happily signalled it. Only the start
        // time distinguishes "this process" from "whatever holds this pid".
        let pid = std::process::id();
        let real_start = devflow_core::agent::process_start_time(pid)
            .expect("read our own start time from /proc");
        let wrong_start = real_start + 1;

        let lock_path = root
            .join(".devflow")
            .join(format!("lock-{padded}", padded = phase.padded()));
        std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        std::fs::write(&lock_path, format!("{pid}\n{wrong_start}")).unwrap();

        let err = stop(root, phase).expect_err("a start-time mismatch must be refused");
        let message = err.to_string();
        assert!(
            message.contains(&pid.to_string()),
            "error must name the pid it refused to signal, got: {message}"
        );
        assert!(
            message.contains("not the process that took the lock"),
            "error must say the identity did not match, got: {message}"
        );
        assert!(
            lock_path.exists(),
            "the lock file must be untouched — stop must not signal anything"
        );
    }

    /// The positive case: when the lock's recorded identity matches the live
    /// process, `stop` proceeds. Without this, the refusal test above would
    /// pass equally well against a `stop` that refused unconditionally.
    #[test]
    fn stop_signals_the_holder_when_the_recorded_identity_matches() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(202);

        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let child_pid = child.id();

        // Record the child's OWN identity — the honest case, where the lock
        // holder really is the process named. Poll for the start time: a
        // freshly forked child may not have exec'd yet (999.47).
        let mut child_start = None;
        for _ in 0..100 {
            child_start = devflow_core::agent::process_start_time(child_pid);
            if child_start.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let child_start = child_start.expect("read the child's start time");

        let lock_path = root
            .join(".devflow")
            .join(format!("lock-{padded}", padded = phase.padded()));
        std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        std::fs::write(&lock_path, format!("{child_pid}\n{child_start}")).unwrap();

        let result = stop(root, phase);

        let _ = child.kill();
        let _ = child.wait();

        assert!(
            result.is_ok(),
            "a matching identity must be signalled, not refused: {result:?}"
        );
    }

    /// Task 2 behavior: a lock file naming a dead pid is stale — reclaimed
    /// silently, never signalled, and never an error. An explicit state
    /// file is present so this test exercises only the lock-fallback path
    /// under test, not Task 3's separate missing-state tolerance.
    #[test]
    fn stop_is_a_success_no_op_when_the_lock_names_a_dead_pid() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(202);
        let lock_path = root
            .join(".devflow")
            .join(format!("lock-{padded}", padded = phase.padded()));
        std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        // Above default kernel pid_max — guaranteed not alive.
        std::fs::write(&lock_path, "9999999").unwrap();
        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        workflow::save_state(&state).unwrap();

        stop(root, phase).expect("stop against a stale lock must succeed, not error");
    }

    /// T-23-51: a live `monitor_pid` recorded in `State` — with no open gate
    /// and no lock file — must never be treated as a signalling target.
    /// `stop` only ever looks at the lock file.
    #[test]
    fn stop_never_treats_monitor_pid_as_a_signalling_target() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(203);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.monitor_pid = Some(std::process::id());
        workflow::save_state(&state).unwrap();

        stop(root, phase).expect(
            "stop must succeed — a recorded monitor_pid alone must never be signalled or \
             treated as a blocker",
        );
    }

    /// Task 3 behavior: no open gate, no lock file, and no persisted state
    /// at all — a phase that was never started — is a successful no-op.
    #[test]
    fn stop_is_a_success_no_op_with_no_gate_and_no_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        stop(root, PhaseId::new(201)).expect("stop against nothing present must succeed");
    }

    /// 14-CR-03: a capture file SHORTER than the follower's offset means the
    /// next stage's monitor deleted and recreated it — the follower must
    /// restart from 0, not seek past EOF forever.
    #[test]
    fn rollover_offset_resets_on_shrunken_capture() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("capture");
        std::fs::write(&path, "abc").unwrap();

        // File (3 bytes) shorter than offset 10 → rollover → 0.
        assert_eq!(rollover_offset(&path, 10), 0);
        // File longer than or equal to the offset → keep the offset.
        assert_eq!(rollover_offset(&path, 3), 3);
        assert_eq!(rollover_offset(&path, 2), 2);
        // Missing file (mid-rollover gap) → keep the offset for now.
        assert_eq!(rollover_offset(&dir.path().join("gone"), 7), 7);
    }

    #[test]
    fn print_capture_from_tracks_offsets_across_appends() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("capture");
        std::fs::write(&path, "hello ").unwrap();
        let mut output = Vec::new();

        let offset = write_capture_from(&path, 0, &mut output).unwrap();
        assert_eq!(offset, 6);
        assert_eq!(output, b"hello ");

        // Nothing new: offset unchanged.
        output.clear();
        assert_eq!(write_capture_from(&path, offset, &mut output).unwrap(), 6);
        assert!(output.is_empty());

        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"world").unwrap();
        drop(f);
        assert_eq!(write_capture_from(&path, offset, &mut output).unwrap(), 11);
        assert_eq!(output, b"world");

        // Missing file is treated as "no new bytes yet".
        output.clear();
        assert_eq!(
            write_capture_from(Path::new("/nonexistent/x"), 4, &mut output).unwrap(),
            4
        );
        assert!(output.is_empty());
    }

    /// 13-06 dogfood regression (Codex leg): a fresh headless Codex run can
    /// never pass Define, so `start --agent codex` pre-flights on the
    /// phase's CONTEXT.md existing on the base branch.
    #[test]
    fn phase_artifact_on_base_detects_context_and_fails_open() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let run = |args: &[&str]| {
            let out = devflow_core::test_support::git_command(root)
                .args(args)
                .output()
                .expect("spawn git");
            assert!(out.status.success(), "git {args:?} failed");
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.email", "t@e.st"]);
        run(&["config", "user.name", "t"]);
        run(&["config", "commit.gpgsign", "false"]);
        run(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::create_dir_all(root.join(".planning/phases/03-widget")).unwrap();
        std::fs::write(root.join(".planning/phases/03-widget/03-CONTEXT.md"), "ctx").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "init"]);
        run(&["branch", "develop"]);

        assert!(phase_artifact_on_base(
            root,
            PhaseId::new(3),
            "-CONTEXT.md",
            "develop"
        ));
        assert!(!phase_artifact_on_base(
            root,
            PhaseId::new(3),
            "-PLAN.md",
            "develop"
        ));
        assert!(!phase_artifact_on_base(
            root,
            PhaseId::new(4),
            "-CONTEXT.md",
            "develop"
        ));

        // Fail-open: outside a repo (or with no develop branch) the
        // pre-flight must not block.
        let empty = tempfile::tempdir().unwrap();
        assert!(phase_artifact_on_base(
            empty.path(),
            PhaseId::new(3),
            "-CONTEXT.md",
            "develop"
        ));
    }

    /// 45-01 base fixture: a repo with a local `develop`, a local `main`, a
    /// local `workspace/example` carrying `.planning/config.json`, and a
    /// remote-tracking `origin/main` — the four spellings of "the production
    /// branch" a bare `rev-parse --verify` accepts.
    fn base_branch_fixture(root: &Path) {
        let run = |args: &[&str]| {
            let out = devflow_core::test_support::git_command(root)
                .args(args)
                .output()
                .expect("spawn git");
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.email", "t@e.st"]);
        run(&["config", "user.name", "t"]);
        run(&["config", "commit.gpgsign", "false"]);
        run(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("README.md"), "x").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "init"]);
        run(&["branch", "develop"]);
        run(&["checkout", "-q", "-b", "workspace/example"]);
        std::fs::create_dir_all(root.join(".planning")).unwrap();
        std::fs::write(root.join(".planning/config.json"), "{}").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "planning"]);
        run(&["checkout", "-q", "develop"]);
        // A remote-tracking ref with no remote configured — enough for
        // `git rev-parse --verify origin/main` to succeed.
        let sha = devflow_core::test_support::git_command(root)
            .args(["rev-parse", "main"])
            .output()
            .expect("rev-parse main");
        let sha = String::from_utf8_lossy(&sha.stdout).trim().to_string();
        run(&["update-ref", "refs/remotes/origin/main", &sha]);
    }

    /// Review round 2's base-validation bypass: `validate_base_branch`'s
    /// string comparison against `MAIN` is defeated by SPELLING. Verified at
    /// source that `git rev-parse --verify <base>` — the probe
    /// `phase_reachability_on_base` uses — accepts a remote-tracking name, a
    /// fully-qualified ref path, `HEAD`, and a bare SHA, and that
    /// `worktree::add` forwards the raw value to `git worktree add` as a
    /// start point. Anchoring on `refs/heads/{base}` rejects all four.
    #[test]
    fn ensure_base_is_a_local_branch_rejects_commit_ish_that_is_not_a_local_branch() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        base_branch_fixture(root);

        // NEGATIVE CONTROL: without this, every `Err` assertion below would
        // pass against a helper that returned `Err` unconditionally.
        assert!(
            ensure_base_is_a_local_branch(root, "workspace/example").is_ok(),
            "a real local branch must be accepted"
        );
        assert!(ensure_base_is_a_local_branch(root, "develop").is_ok());

        let head_sha = devflow_core::test_support::git_command(root)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("rev-parse HEAD");
        let head_sha = String::from_utf8_lossy(&head_sha.stdout).trim().to_string();

        for spelling in ["origin/main", "refs/heads/main", "HEAD", head_sha.as_str()] {
            // Each of these satisfies a bare `rev-parse --verify` — assert
            // that first, so the test proves the bypass exists rather than
            // merely that the helper is strict.
            let bare = devflow_core::test_support::git_command(root)
                .args(["rev-parse", "--verify", "--quiet", spelling])
                .output()
                .expect("rev-parse");
            assert!(
                bare.status.success(),
                "fixture is wrong: `{spelling}` must resolve as a commit-ish"
            );
            assert!(
                ensure_base_is_a_local_branch(root, spelling).is_err(),
                "`{spelling}` is not a local branch and must be refused"
            );
        }
    }

    /// Review round 2 (F4): the artifact-presence probe carried the trunk as
    /// a literal in its `ls-tree` argument array, so a planning branch
    /// holding CONTEXT.md/PLAN.md was invisible to it and a
    /// `RequiresExistingArtifact` driver was refused at Define for an
    /// artifact that existed — the same class of false refusal AUTO-01 exists
    /// to remove.
    ///
    /// BOTH directions are asserted here and both are required: the probe
    /// FAILS OPEN by returning `true` on a git error, so a test asserting
    /// only `true` cannot distinguish "found the artifact on the right
    /// branch" from "the git invocation failed" — the same answer for the
    /// wrong reason.
    #[test]
    fn phase_artifact_probe_reads_the_supplied_base_not_the_default_trunk() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let run = |args: &[&str]| {
            let out = devflow_core::test_support::git_command(root)
                .args(args)
                .output()
                .expect("spawn git");
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["init", "-q", "-b", "develop"]);
        run(&["config", "user.email", "t@e.st"]);
        run(&["config", "user.name", "t"]);
        run(&["config", "commit.gpgsign", "false"]);
        run(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("README.md"), "x").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "init"]);

        // The phase artifact exists ONLY on the planning branch.
        run(&["checkout", "-q", "-b", "workspace/example"]);
        std::fs::create_dir_all(root.join(".planning/phases/45-widget")).unwrap();
        std::fs::write(root.join(".planning/phases/45-widget/45-CONTEXT.md"), "ctx").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "planning"]);
        run(&["checkout", "-q", "develop"]);

        assert!(
            phase_artifact_on_base(root, PhaseId::new(45), "-CONTEXT.md", "workspace/example"),
            "the probe must see an artifact on the branch it was pointed at"
        );

        // NEGATIVE CONTROL: the identical repo and phase, probed against the
        // default trunk, must report absence.
        assert!(
            !phase_artifact_on_base(root, PhaseId::new(45), "-CONTEXT.md", "develop"),
            "the probe must not report an artifact that is absent from the branch probed"
        );
    }

    /// Review round 2 (F3): `devflow start --no-worktree` forked its feature
    /// branch from the default trunk even after every preceding check had
    /// validated the configured base — a silent failure of AUTO-01's core
    /// promise on a supported path.
    ///
    /// Asserted one level BELOW the CLI entry point, on the
    /// `GitFlow::for_project(..).feature_start(..)` call the `--no-worktree`
    /// arm was converted to use. Driving `commands::start` itself would
    /// require an agent binary, a network fetch and a monitor spawn, none of
    /// which bear on the fork point.
    #[test]
    fn no_worktree_start_forks_the_feature_branch_from_the_configured_base() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        base_branch_fixture(root);
        std::fs::write(
            root.join("devflow.toml"),
            "base_branch = \"workspace/example\"\n",
        )
        .unwrap();

        // Hermeticity precondition, asserted rather than assumed: an
        // exported DEVFLOW_BASE_BRANCH outranks the file, so with one set
        // this test would be measuring the developer's shell.
        assert!(
            std::env::var("DEVFLOW_BASE_BRANCH")
                .map(|v| v.is_empty())
                .unwrap_or(true),
            "DEVFLOW_BASE_BRANCH is exported in this environment — it outranks \
             devflow.toml, so this test cannot measure the file"
        );

        let branch = GitFlow::for_project(root)
            .feature_start(PhaseId::new(45))
            .expect("feature_start off the configured base");

        assert!(
            root.join(".planning/config.json").exists(),
            "the feature branch must descend from the configured base"
        );

        // NEGATIVE CONTROL: a branch forked from `develop` would have a tip
        // that IS `develop`'s tip, so `--is-ancestor` would succeed. It must
        // fail here. Asserting only "a branch was created" passes against
        // exactly the broken behaviour this test exists to catch.
        let is_ancestor = devflow_core::test_support::git_command(root)
            .args(["merge-base", "--is-ancestor", &branch, "develop"])
            .output()
            .expect("merge-base");
        assert!(
            !is_ancestor.status.success(),
            "`{branch}` is ancestor-equal of develop — it forked from the default trunk"
        );
    }

    // -----------------------------------------------------------------
    // 17d: build provenance + self-dogfood staleness gate (D-17-D-21, Task 2)
    // -----------------------------------------------------------------

    /// D-21: the `workflow_started` payload carries every provenance field,
    /// tested directly without spawning a real agent. No `build_timestamp`
    /// field any more (CR-02, 17-11) — it was removed from `build.rs`
    /// entirely, not just this payload. Also pins the WR-02 redaction: the
    /// `exe_path` field must never carry a directory component (the
    /// operator's home directory / OS username), since `OPERATIONS.md`
    /// documents `events.jsonl` as a file that's safe to tail and paste.
    #[test]
    fn workflow_started_payload_carries_build_provenance() {
        let state = State::new(
            PhaseId::new(66),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        let payload = workflow_started_payload(&state);
        assert_eq!(payload["agent"], "claude");
        assert_eq!(payload["mode"], "auto");
        assert!(payload["version"].as_str().is_some());
        assert!(payload["commit"].is_string());
        assert!(payload["dirty"].is_string());
        assert!(
            payload.get("build_timestamp").is_none(),
            "build_timestamp was removed (CR-02) and must not reappear"
        );
        assert!(
            payload.get("exe_path").is_some(),
            "WR-02: exe_path key must still exist — a future refactor must not \
             satisfy the redaction assertion by deleting the field"
        );
        assert!(payload["exe_path"].is_string() || payload["exe_path"].is_null());
        if let Some(exe_path) = payload["exe_path"].as_str() {
            assert!(
                !exe_path.contains('/') && !exe_path.contains('\\'),
                "WR-02: exe_path must be a bare filename with no directory \
                 separator — OPERATIONS.md documents events.jsonl as safe to \
                 tail and paste, so a full absolute path here leaks the \
                 operator's home directory and OS username; got {exe_path:?}"
            );
        }
    }

    #[test]
    fn status_shows_pending_gate_prominently() {
        let dir = tempfile::tempdir().unwrap();
        let context = format!("first line\n\u{1b}[2J{}", "sensitive detail ".repeat(80));
        Gates::write_gate(dir.path(), PhaseId::new(16), Stage::Ship, &context).unwrap();
        let open = Gates::list_open(dir.path());

        let banner = render_pending_gate_banner(&open, u64::MAX).unwrap();

        assert!(banner.contains("PENDING GATE"));
        assert!(banner.contains("phase 16"));
        assert!(banner.contains("ship"));
        assert!(banner.contains("devflow gate approve 16 --stage ship"));
        assert!(banner.contains("devflow gate reject 16 --stage ship"));
        assert!(banner.contains("[truncated; full output in .devflow/]"));
        assert!(!banner.contains(&context));
        assert!(!banner.contains('\u{1b}'));
        assert!(banner.contains("ESCALATED"));
    }

    /// 23-03 Task 3: a gate whose age has crossed the escalation threshold
    /// renders with a trailing urgency marker.
    #[test]
    fn render_gate_age_marks_escalated_gate_urgent() {
        let now = 10_000u64;
        let old_timestamp = (now - GATE_ESCALATION_THRESHOLD_SECS - 60).to_string();

        let age = render_gate_age(&old_timestamp, now);

        assert!(
            age.ends_with('!'),
            "an escalated gate's age must carry a trailing urgency marker, got {age:?}"
        );
    }

    /// A gate younger than the escalation threshold renders with an age and
    /// no urgency marker.
    #[test]
    fn render_gate_age_no_marker_for_fresh_gate() {
        let now = 10_000u64;
        let fresh_timestamp = (now - 30).to_string();

        let age = render_gate_age(&fresh_timestamp, now);

        assert!(
            !age.ends_with('!'),
            "a fresh gate must not carry an urgency marker, got {age:?}"
        );
    }

    /// A `timestamp` that does not parse as `u64` must render an unknown
    /// age (`?`) rather than panicking or being silently dropped — the
    /// forensics record shows dropping unusual rows is exactly how the
    /// orphan population stayed invisible.
    #[test]
    fn render_gate_age_unknown_for_non_numeric_timestamp() {
        let age = render_gate_age("not-a-number", 10_000);
        assert_eq!(age, "?");
    }

    /// The `--all-roots` row-rendering must still include a gate whose
    /// timestamp is non-numeric — the row is present in the output, just
    /// with an unknown age, matching the acceptance criterion literally.
    #[test]
    fn all_roots_row_includes_gate_with_non_numeric_timestamp() {
        let gate = OpenGate {
            phase: PhaseId::new(42),
            stage: Stage::Ship,
            context: "ctx".to_string(),
            timestamp: "not-a-number".to_string(),
        };

        let row = render_all_roots_gate_row(Path::new("/tmp/some-root"), &gate, 10_000);

        assert!(row.contains("42"), "row must still name the phase: {row}");
        assert!(row.contains('?'), "row must render the unknown age: {row}");
    }

    /// 21a: `recovery_hints` returns a `resume` hint for a stuck phase,
    /// additionally an `advance` hint when the phase is gate-pending
    /// (answer the gate, then advance), and nothing for a non-stuck phase.
    #[test]
    fn recovery_hints_includes_resume_for_stuck() {
        let dir = tempfile::tempdir().unwrap();
        let state = State::new(
            PhaseId::new(7),
            AgentKind::Claude,
            Mode::Auto,
            dir.path().to_path_buf(),
        );

        let hints = recovery_hints(&state, Liveness::Stuck);

        assert_eq!(hints, vec!["devflow resume --phase 7".to_string()]);
    }

    #[test]
    fn recovery_hints_includes_advance_when_stuck_and_gate_pending() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new(
            PhaseId::new(7),
            AgentKind::Claude,
            Mode::Auto,
            dir.path().to_path_buf(),
        );
        state.gate_pending = true;

        let hints = recovery_hints(&state, Liveness::Stuck);

        assert_eq!(
            hints,
            vec![
                "devflow resume --phase 7".to_string(),
                "devflow advance --phase 7".to_string(),
            ]
        );
    }

    #[test]
    fn recovery_hints_empty_for_healthy() {
        let dir = tempfile::tempdir().unwrap();
        let state = State::new(
            PhaseId::new(7),
            AgentKind::Claude,
            Mode::Auto,
            dir.path().to_path_buf(),
        );

        assert!(recovery_hints(&state, Liveness::Healthy).is_empty());
    }

    /// 21a / 999.30 IN-01: the shared one-pass event summary's
    /// `stage_launched_ts` is the LAST `stage_launched` event's `ts` — the
    /// real stage-entry time — and is `None` without one, never falling
    /// back to any other field. `status` now sources this from
    /// `events::last_events_by_phase` instead of a per-phase rescan.
    #[test]
    fn stage_launched_ts_none_without_event() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            events::last_events_by_phase(dir.path())
                .get(&PhaseId::new(7))
                .and_then(|s| s.stage_launched_ts),
            None
        );
    }

    /// The closing proof for the 3/3 cross-AI review MEDIUM
    /// (21-REVIEWS.md): a phase whose latest `stage_launched` event is ~90s
    /// old but whose phase-level `started_at` is ~30m old must report the
    /// ~90s stage age — the summary's `stage_launched_ts` must never be
    /// sourced from `state.started_at`.
    #[test]
    fn stage_launched_ts_reflects_event_age_not_phase_started_at() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let stage_ts = now - 90;
        let phase_started_at = now - 30 * 60;

        let mut state = State::new(
            PhaseId::new(7),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
        state.started_at = phase_started_at.to_string();
        workflow::save_state(&state).unwrap();

        events::emit(
            root,
            PhaseId::new(7),
            "stage_launched",
            serde_json::json!({"stage": "code", "agent": "claude", "monitor_pid": 1}),
        );
        // events::emit always stamps `ts` with the current time; rewrite it
        // to a fixed, known-past value so the assertion is deterministic
        // instead of racing the live clock.
        let events_path = devflow_core::events::events_path(root);
        let rewritten: String = std::fs::read_to_string(&events_path)
            .unwrap()
            .lines()
            .map(|line| {
                let mut value: serde_json::Value = serde_json::from_str(line).unwrap();
                value["ts"] = serde_json::json!(stage_ts);
                value.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(&events_path, rewritten).unwrap();

        let ts = events::last_events_by_phase(root)
            .get(&PhaseId::new(7))
            .and_then(|s| s.stage_launched_ts);
        assert_eq!(ts, Some(stage_ts));

        let line = render_stage_progress_line(Stage::Code, ts);
        assert!(line.contains("1m ago"), "expected ~90s age, got: {line}");
        assert!(
            !line.contains("30m ago"),
            "must not render phase-level started_at age: {line}"
        );
    }

    #[test]
    fn render_stage_progress_line_omits_age_without_stage_launched_event() {
        assert_eq!(
            render_stage_progress_line(Stage::Plan, None),
            "  in stage plan"
        );
    }

    /// Unit tests for the pure `doctor` reconciliation core (18a). Each test
    /// builds a `PhaseFacts` directly — no repository, no I/O — proving
    /// `reconcile_phase` is a predicate over facts alone.
    #[cfg(test)]
    mod doctor_reconciliation {
        use super::*;

        /// A fully-agreeing baseline: `reconcile_phase` over this returns
        /// zero findings. Each test overrides only the field(s) needed to
        /// trigger the one check it's proving.
        fn agreeing_facts(phase: PhaseId) -> PhaseFacts {
            PhaseFacts {
                phase,
                stage: Stage::Code,
                gate_pending: false,
                agent_pid: Some(4242),
                agent_alive: true,
                monitor_pid: Some(4343),
                monitor_alive: true,
                last_event: Some("stage_launched".into()),
                last_launched_stage: Some(Stage::Code),
                open_gate_stages: Vec::new(),
                feature_branch_exists: true,
                stopped: false,
            }
        }

        #[test]
        fn reconcile_phase_returns_no_findings_when_all_agree() {
            let facts = agreeing_facts(PhaseId::new(1));
            assert!(reconcile_phase(&facts).is_empty());
        }

        #[test]
        fn reconcile_phase_flags_gate_pending_without_open_gate() {
            let facts = PhaseFacts {
                gate_pending: true,
                open_gate_stages: Vec::new(),
                ..agreeing_facts(PhaseId::new(2))
            };
            let findings = reconcile_phase(&facts);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Problem);
            assert!(findings[0].detail.contains("gate_pending is true"));
            assert_eq!(
                findings[0].repair.as_deref(),
                Some("devflow resume --phase 2")
            );
        }

        #[test]
        fn reconcile_phase_flags_orphan_open_gate() {
            let facts = PhaseFacts {
                gate_pending: false,
                open_gate_stages: vec![Stage::Validate],
                ..agreeing_facts(PhaseId::new(3))
            };
            let findings = reconcile_phase(&facts);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Problem);
            assert!(findings[0].detail.contains("gate open for stage validate"));
            assert_eq!(
                findings[0].repair.as_deref(),
                Some("devflow gate approve 3 --stage validate")
            );
        }

        #[test]
        fn reconcile_phase_flags_dead_agent_at_agent_stage() {
            let facts = PhaseFacts {
                agent_pid: Some(999_999),
                agent_alive: false,
                ..agreeing_facts(PhaseId::new(4))
            };
            let findings = reconcile_phase(&facts);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Problem);
            assert!(findings[0].detail.contains("agent pid 999999"));
            assert_eq!(
                findings[0].repair.as_deref(),
                Some("devflow resume --phase 4")
            );
        }

        #[test]
        fn reconcile_phase_flags_stage_event_drift() {
            let facts = PhaseFacts {
                stage: Stage::Validate,
                last_launched_stage: Some(Stage::Code),
                ..agreeing_facts(PhaseId::new(5))
            };
            let findings = reconcile_phase(&facts);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Warn);
            assert!(
                findings[0]
                    .detail
                    .contains("last stage_launched event named code")
            );
            assert!(findings[0].repair.is_none());
        }

        #[test]
        fn reconcile_phase_flags_missing_feature_branch() {
            let facts = PhaseFacts {
                stage: Stage::Plan,
                last_launched_stage: Some(Stage::Plan),
                feature_branch_exists: false,
                ..agreeing_facts(PhaseId::new(6))
            };
            let findings = reconcile_phase(&facts);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Warn);
            assert!(findings[0].detail.contains("feature/phase-06"));
            assert!(findings[0].repair.is_none());
        }

        /// 18b: a dead monitor with a dead agent is `Stuck` — nothing will
        /// call `devflow advance` for this phase — and reports a `Problem`
        /// finding with a `devflow resume --phase N` repair.
        #[test]
        fn reconcile_reports_stuck_when_monitor_and_agent_are_both_dead() {
            let facts = PhaseFacts {
                monitor_pid: Some(5150),
                monitor_alive: false,
                agent_pid: Some(4242),
                agent_alive: false,
                ..agreeing_facts(PhaseId::new(8))
            };
            let findings = reconcile_phase(&facts);
            let monitor_finding = findings
                .iter()
                .find(|f| f.detail.contains("monitor pid"))
                .expect("expected a monitor finding when monitor and agent are both dead");
            assert_eq!(monitor_finding.severity, Severity::Problem);
            assert!(monitor_finding.detail.contains("monitor pid 5150"));
            assert_eq!(
                monitor_finding.repair.as_deref(),
                Some("devflow resume --phase 8")
            );
        }

        /// 20c (D-09 + review: Codex HIGH — the doctor gap is bigger than
        /// `check_dead_agent`): a phase intentionally halted by `devflow
        /// start --until <stage>` sits at an agent stage with a dead agent
        /// pid on disk. `check_dead_agent` must recognize `facts.stopped`
        /// and report ZERO findings — this is not a crash.
        #[test]
        fn reconcile_phase_ignores_dead_agent_when_stopped() {
            let facts = PhaseFacts {
                stage: Stage::Plan,
                agent_pid: Some(999_999),
                agent_alive: false,
                stopped: true,
                ..agreeing_facts(PhaseId::new(11))
            };
            let findings = reconcile_phase(&facts);
            assert!(
                findings.iter().all(|f| f.severity != Severity::Problem),
                "a --until-stopped phase must yield zero Problem findings, got: \
                 {:?}",
                findings.iter().map(|f| &f.detail).collect::<Vec<_>>()
            );
        }

        /// 20c (D-09 + review: Codex HIGH — the doctor gap is bigger than
        /// `check_dead_agent`): the same stopped phase may also carry a
        /// stale `monitor_pid` (though the stop path clears it — this
        /// proves the guard holds even if that clear were ever bypassed).
        /// `check_dead_monitor` must also recognize `facts.stopped` and
        /// report ZERO findings.
        #[test]
        fn reconcile_phase_ignores_dead_monitor_when_stopped() {
            let facts = PhaseFacts {
                stage: Stage::Plan,
                monitor_pid: Some(5150),
                monitor_alive: false,
                agent_pid: Some(4242),
                agent_alive: false,
                stopped: true,
                ..agreeing_facts(PhaseId::new(12))
            };
            let findings = reconcile_phase(&facts);
            assert!(
                findings.iter().all(|f| f.severity != Severity::Problem),
                "a --until-stopped phase must yield zero Problem findings even with a \
                 stale monitor_pid, got: {:?}",
                findings.iter().map(|f| &f.detail).collect::<Vec<_>>()
            );
        }

        /// 18b (T-18-11): an unrecorded monitor is unknown, not a problem —
        /// a state file written by a pre-18b binary must never render as
        /// stuck.
        #[test]
        fn reconcile_is_silent_when_monitor_pid_is_unrecorded() {
            let facts = PhaseFacts {
                monitor_pid: None,
                monitor_alive: false,
                ..agreeing_facts(PhaseId::new(9))
            };
            assert!(
                reconcile_phase(&facts).is_empty(),
                "an unrecorded monitor must never produce a finding"
            );
        }

        /// 18b: a live monitor with a dead agent is a normal between-stages
        /// moment (the monitor hasn't advanced the phase yet), not a monitor
        /// finding. `check_dead_agent`'s own pre-existing finding for the
        /// dead agent pid is unrelated to this check and out of this plan's
        /// scope.
        #[test]
        fn reconcile_is_silent_when_monitor_alive_and_agent_dead() {
            let facts = PhaseFacts {
                monitor_pid: Some(5150),
                monitor_alive: true,
                agent_alive: false,
                ..agreeing_facts(PhaseId::new(10))
            };
            let findings = reconcile_phase(&facts);
            assert!(
                findings.iter().all(|f| !f.detail.contains("monitor pid")),
                "a live monitor with a dead agent must not produce a monitor finding"
            );
        }

        /// Several checks trigger simultaneously; the returned findings must
        /// come back in the fixed order `reconcile_phase` evaluates checks
        /// in, not in whatever order the facts happen to be populated.
        #[test]
        fn reconcile_phase_ordering_is_input_order_independent() {
            let facts = PhaseFacts {
                gate_pending: true,
                agent_pid: Some(999_999),
                agent_alive: false,
                monitor_pid: Some(999_998),
                monitor_alive: false,
                last_launched_stage: Some(Stage::Validate),
                open_gate_stages: Vec::new(),
                feature_branch_exists: false,
                ..agreeing_facts(PhaseId::new(7))
            };
            let findings = reconcile_phase(&facts);
            let severities: Vec<Severity> = findings.iter().map(|f| f.severity).collect();
            assert_eq!(
                severities,
                vec![
                    Severity::Problem, // check_gate_pending_without_gate
                    Severity::Problem, // check_dead_agent
                    Severity::Problem, // check_dead_monitor
                    Severity::Warn,    // check_stage_event_drift
                    Severity::Warn,    // check_missing_branch
                ]
            );
            assert!(findings[0].detail.contains("gate_pending is true"));
            assert!(findings[1].detail.contains("agent pid 999999"));
            assert!(findings[2].detail.contains("monitor pid 999998"));
            assert!(
                findings[3]
                    .detail
                    .contains("last stage_launched event named validate")
            );
            assert!(findings[4].detail.contains("feature/phase-07"));
        }

        /// `doctor`'s idle-project path (Task 2, 18a): the exact code path
        /// `doctor(root, false)` runs for its reconciliation section is
        /// `collect_phase_facts` + `render_reconciliation_text` — asserted
        /// directly here rather than capturing process stdout, since this
        /// codebase has no stdout-capture dependency and this phase adds no
        /// new ones (18-RESEARCH.md).
        #[test]
        fn doctor_reports_no_active_phases_when_idle() {
            let dir = tempfile::tempdir().unwrap();
            let facts = collect_phase_facts(dir.path());
            assert!(facts.is_empty());
            assert!(render_reconciliation_text(&facts).contains("no active phases"));
        }

        #[test]
        fn doctor_reports_gate_pending_without_gate_file() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(90);
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Validate;
            state.gate_pending = true;
            workflow::save_state(&state).unwrap();

            let facts = collect_phase_facts(root);
            assert_eq!(facts.len(), 1);
            let text = render_reconciliation_text(&facts);
            assert!(text.contains(&format!("phase {phase}: gate_pending is true")));
            assert!(text.contains(&format!("repair: devflow resume --phase {phase}")));
        }

        /// WR-01 (18-fix): `doctor --json` must emit ONE JSON document, not
        /// two concatenated top-level arrays. Exercises the exact
        /// composition `doctor()`'s `--json` path uses (`doctor_json_body`),
        /// then round-trips it through `serde_json::to_string`/`from_str` —
        /// the failure mode this reproduces (pre-fix) is a single-document
        /// parser (`json.load`, `JSON.parse`) raising "Extra data" on the
        /// old two-array output; `jq` tolerated it (NDJSON-style streaming),
        /// which is why it went unnoticed.
        #[test]
        fn doctor_json_is_a_single_object_with_environment_and_reconciliation() {
            let checks = vec![Check {
                name: "git".into(),
                status: "ok".into(),
                version: Some("2.40.0".into()),
                install_hint: None,
            }];

            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(92);
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Validate;
            state.gate_pending = true; // mismatched: no gate file — produces a finding
            workflow::save_state(&state).unwrap();
            let facts = collect_phase_facts(root);

            let body = doctor_json_body(&checks, &facts, &[], &[]);
            let serialized = serde_json::to_string(&body).unwrap();
            let reparsed: serde_json::Value = serde_json::from_str(&serialized)
                .expect("doctor --json must be single-document JSON, not two concatenated arrays");

            assert!(
                reparsed.get("environment").is_some(),
                "must carry the tool checks under \"environment\": {reparsed}"
            );
            assert!(
                reparsed.get("reconciliation").is_some(),
                "must carry the reconciliation findings under \"reconciliation\": {reparsed}"
            );
            assert!(
                reparsed.get("planning_doc_staleness").is_some(),
                "21b: must carry the planning-doc findings under a THIRD key, \
                 never a second concatenated array: {reparsed}"
            );
            assert!(
                reparsed.get("stray_processes").is_some(),
                "999.44: must carry the stray-process findings under a FOURTH key, \
                 never a second concatenated array: {reparsed}"
            );
            assert_eq!(
                reparsed.as_object().unwrap().len(),
                4,
                "doctor --json must have exactly four top-level keys: {reparsed}"
            );
            assert!(reparsed["environment"].is_array());
            assert!(reparsed["reconciliation"].is_array());
            assert!(reparsed["planning_doc_staleness"].is_array());
            assert!(reparsed["stray_processes"].is_array());
            let reconciliation = reparsed["reconciliation"].as_array().unwrap();
            assert!(
                !reconciliation.is_empty(),
                "the mismatched gate_pending fixture must produce at least one finding"
            );
            assert!(
                reconciliation.iter().any(|f| f["detail"]
                    .as_str()
                    .unwrap_or("")
                    .contains("gate_pending is true")),
                "must carry the gate_pending finding: {reconciliation:?}"
            );
        }

        /// T-18-02: running `doctor` twice against a mismatched fixture must
        /// leave `.devflow/` byte-identical — no state rewrite, no event
        /// append, no gate file appears or disappears.
        #[test]
        fn doctor_is_read_only_on_a_mismatched_project() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(91);
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Validate;
            state.gate_pending = true; // mismatched: no gate file will exist
            workflow::save_state(&state).unwrap();
            events::emit(
                root,
                phase,
                "stage_launched",
                serde_json::json!({"stage": "code"}),
            );

            let state_path = workflow::state_path(root, phase);
            let before_len = std::fs::metadata(&state_path).unwrap().len();
            let before_modified = std::fs::metadata(&state_path).unwrap().modified().unwrap();
            let events_log = events::events_path(root);
            let before_lines = std::fs::read_to_string(&events_log)
                .unwrap()
                .lines()
                .count();

            doctor(root, false).unwrap();
            doctor(root, false).unwrap();

            let after_len = std::fs::metadata(&state_path).unwrap().len();
            let after_modified = std::fs::metadata(&state_path).unwrap().modified().unwrap();
            let after_lines = std::fs::read_to_string(&events_log)
                .unwrap()
                .lines()
                .count();

            assert_eq!(
                before_len, after_len,
                "doctor must not rewrite the state file"
            );
            assert_eq!(
                before_modified, after_modified,
                "doctor must not touch the state file's mtime"
            );
            assert_eq!(
                before_lines, after_lines,
                "doctor must not append to events.jsonl"
            );
        }
    }

    /// Unit tests for `doctor`'s stray-process finding (999.44 / DEN-68).
    /// `build_stray_process_findings` takes an injected `&[StrayProcess]`
    /// list, so most of these run with zero I/O and no real orphan process
    /// on the machine running the test — mirrors `reconcile_planning_docs`'s
    /// zero-I/O discipline above. The one test that DOES need a real
    /// process is read-only (`doctor` never signals), so it is safe to run
    /// on a machine that may have unrelated, legitimate devflow activity
    /// running concurrently — it only ever asserts the fixture it spawned
    /// itself is still alive, never acts on anything else `discover_stray_
    /// devflow_processes` might also see.
    #[cfg(test)]
    mod stray_process_finding {
        use super::*;

        #[test]
        fn build_stray_process_findings_is_empty_for_no_strays() {
            assert!(build_stray_process_findings(&[]).is_empty());
        }

        #[test]
        fn build_stray_process_findings_names_pid_layer_and_repair() {
            let strays = vec![agent::StrayProcess {
                pid: 424242,
                start_time: 0,
                layer: agent::StrayLayer::MonitorWrapper,
            }];
            let findings = build_stray_process_findings(&strays);
            assert_eq!(findings.len(), 1);
            let finding = &findings[0];
            assert_eq!(finding.severity, Severity::Problem);
            assert!(finding.detail.contains("424242"));
            assert!(finding.detail.contains("monitor wrapper"));
            assert!(
                finding
                    .repair
                    .as_deref()
                    .unwrap()
                    .contains("--reap-strays --dry-run"),
                "the repair must name the preview form first, not the destructive form alone: \
                 {:?}",
                finding.repair
            );
            assert!(
                !finding.detail.contains('/'),
                "no finding may embed a filesystem path (WR-02): {}",
                finding.detail
            );
        }

        #[test]
        fn build_stray_process_findings_names_advance_child_layer() {
            let strays = vec![agent::StrayProcess {
                pid: 424243,
                start_time: 0,
                layer: agent::StrayLayer::AdvanceChild,
            }];
            let findings = build_stray_process_findings(&strays);
            assert!(findings[0].detail.contains("advance child"));
        }

        /// `doctor`'s existing text output is byte-for-byte unchanged when
        /// there is no stray to report — the new section must contribute
        /// NOTHING, not even a header line, unlike the always-present
        /// reconciliation/planning-docs sections.
        #[test]
        fn render_stray_process_text_is_empty_when_no_strays() {
            assert_eq!(render_stray_process_text(&[]), "");
        }

        #[test]
        fn render_stray_process_text_names_pid_and_repair_when_present() {
            let strays = vec![agent::StrayProcess {
                pid: 555555,
                start_time: 0,
                layer: agent::StrayLayer::MonitorWrapper,
            }];
            let text = render_stray_process_text(&build_stray_process_findings(&strays));
            assert!(text.contains("555555"));
            assert!(text.contains("repair: devflow gate sweep --reap-strays --dry-run"));
        }

        #[test]
        fn doctor_json_body_carries_stray_processes_as_a_fourth_key() {
            let checks: Vec<Check> = Vec::new();
            let facts: Vec<PhaseFacts> = Vec::new();
            let stray_findings = vec![StrayProcessFinding {
                pid: 1,
                layer: "monitor wrapper",
                severity: Severity::Problem,
                detail: "detail".to_string(),
                repair: None,
            }];
            let body = doctor_json_body(&checks, &facts, &[], &stray_findings);
            let obj = body.as_object().unwrap();
            assert_eq!(obj.len(), 4, "must be exactly four top-level keys: {body}");
            assert!(obj.contains_key("stray_processes"));
            let strays = obj["stray_processes"].as_array().unwrap();
            assert_eq!(strays.len(), 1);
            assert_eq!(strays[0]["pid"], 1);
        }

        /// Behavior test (999.44's exact reproduction, Phase 18-01's
        /// read-only proof extended to strays): a real process shaped
        /// exactly like the monitor wrapper `discover_stray_devflow_
        /// processes` matches is spawned, and `doctor` is run TWICE
        /// against it — directly, and end to end through `doctor()`
        /// itself. Every run must report it as a finding and leave it
        /// alive: `doctor` never signals, no matter how many times it
        /// runs.
        #[test]
        fn doctor_finds_a_real_stray_and_never_signals_it_across_two_runs() {
            let mut child = std::process::Command::new("sh")
                .arg("-c")
                .arg("trap cleanup TERM INT; sleep 30")
                .spawn()
                .expect("spawn monitor-wrapper-shaped fixture");
            let pid = child.id();

            let dir = tempfile::tempdir().unwrap();

            // 999.47: cross the exec-visibility barrier before either census
            // read below, or both races the fixture's own fork()->execve()
            // window (25-11).
            assert!(
                devflow_core::test_support::wait_for_exec_visibility(
                    pid,
                    "sh",
                    devflow_core::test_support::EXEC_VISIBILITY_WAIT,
                    devflow_core::test_support::EXEC_VISIBILITY_POLL,
                ),
                "pid {pid}: exec visibility timed out before the fixture became discoverable"
            );

            let first = collect_stray_process_findings(dir.path());
            assert!(
                agent::agent_running(pid),
                "fixture must still be alive after the first collection"
            );
            let second = collect_stray_process_findings(dir.path());
            assert!(
                agent::agent_running(pid),
                "fixture must still be alive after the second collection — doctor's stray \
                 finding must never signal"
            );

            assert!(
                first.iter().any(|f| f.pid == pid),
                "the fixture must be reported by the first run"
            );
            assert!(
                second.iter().any(|f| f.pid == pid),
                "the fixture must be reported by the second run"
            );

            // Also exercise doctor() itself, end to end, twice — not just
            // the pure finding collector — proving the full read-only path.
            doctor(dir.path(), false).unwrap();
            assert!(agent::agent_running(pid), "doctor() itself must not signal");
            doctor(dir.path(), false).unwrap();
            assert!(
                agent::agent_running(pid),
                "doctor() must remain read-only across repeated runs"
            );

            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Task 2 (CR-01/T-25-15-02): the rewritten `detail` states only
    /// what the code actually checked — matched DevFlow's structural
    /// argv shape, owned by the caller, named by no registered root's
    /// state file or lock file — and no longer asserts a conclusion
    /// (unqualified "reachable through no registry entry, lock file, or
    /// state file") the code never established.
    #[test]
    fn stray_finding_detail_states_only_what_was_checked() {
        let strays = vec![agent::StrayProcess {
            pid: 909090,
            start_time: 0,
            layer: agent::StrayLayer::MonitorWrapper,
        }];
        let findings = build_stray_process_findings(&strays);
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];

        assert!(finding.detail.contains("909090"));
        assert!(finding.detail.contains("monitor-wrapper"));
        assert!(finding.detail.contains("owned by the calling user"));
        assert!(
            finding
                .detail
                .contains("named by no registered project root's state file or lock file"),
            "the detail must name the specific checked absence, not an unqualified \
             conclusion: {}",
            finding.detail
        );
        // Reconstructed from two halves rather than embedded as one literal
        // string constant: the acceptance gate greps the whole file for
        // this exact old phrase and expects a `0` count, and a literal
        // occurrence here (even inside a negative assertion) would make
        // that grep self-defeating.
        let old_unverified_orphan_phrase = format!(
            "{}{}",
            "reachable through no registry entry, ", "lock file, or state file"
        );
        assert!(
            !finding.detail.contains(&old_unverified_orphan_phrase),
            "the old, unverified-orphan phrasing must be gone: {}",
            finding.detail
        );
        assert!(
            !finding.detail.contains('/'),
            "no finding may embed a filesystem path (WR-02): {}",
            finding.detail
        );
        assert!(
            finding
                .repair
                .as_deref()
                .unwrap()
                .starts_with("devflow gate sweep --reap-strays --dry-run"),
            "the repair must name the --dry-run preview form first: {:?}",
            finding.repair
        );
    }

    /// Spawn a real process shaped like the monitor wrapper Layer 1
    /// matches, crossing the exec-visibility barrier before returning
    /// (999.47/25-11) so every caller of this helper is guaranteed the
    /// fixture's own `execve()` has completed before any census read.
    fn spawn_wrapper_shaped_fixture() -> std::process::Child {
        let child = std::process::Command::new("sh")
            .arg("-c")
            .arg("trap cleanup TERM INT; sleep 30")
            .spawn()
            .expect("spawn monitor-wrapper-shaped fixture");
        let pid = child.id();
        assert!(
            devflow_core::test_support::wait_for_exec_visibility(
                pid,
                "sh",
                devflow_core::test_support::EXEC_VISIBILITY_WAIT,
                devflow_core::test_support::EXEC_VISIBILITY_POLL,
            ),
            "pid {pid}: exec visibility timed out before the fixture became discoverable"
        );
        child
    }

    /// Task 1's tracer (CR-01, 999.44/DEN-68): one discovery pass names
    /// a live `monitor_pid`-recorded pid, a live lock-holder pid, and a
    /// genuine orphan under no registered root at all. Only the orphan
    /// may survive `unreachable_stray_candidates`'s filter. All three
    /// pids are real, spawned processes crossed through
    /// `wait_for_exec_visibility` — not synthesised numbers — so the
    /// census and the filter are both exercised for real, in one pass.
    #[test]
    fn reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates() {
        let mut state_child = spawn_wrapper_shaped_fixture();
        let mut lock_child = spawn_wrapper_shaped_fixture();
        let mut orphan_child = spawn_wrapper_shaped_fixture();
        let state_pid = state_child.id();
        let lock_pid = lock_child.id();
        let orphan_pid = orphan_child.id();

        let cache_dir = tempfile::tempdir().unwrap();
        let project_root_guard = tempfile::tempdir().unwrap();
        let project_root = project_root_guard.path().to_path_buf();

        // Phase 1: a real `State` naming `state_pid` as its monitor.
        let mut state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            project_root.clone(),
        );
        state.monitor_pid = Some(state_pid);
        workflow::save_state(&state).unwrap();

        // Phase 2: a real `State` (so `list_states` enumerates the
        // phase at all — mirrors real operation, where a phase with an
        // active lock also has a persisted state), deliberately with NO
        // `monitor_pid`, so this phase's only path into the reachable
        // set is the lock file below, not the monitor_pid path Phase 1
        // already exercises.
        let state_2 = State::new(
            PhaseId::new(2),
            AgentKind::Claude,
            Mode::Auto,
            project_root.clone(),
        );
        workflow::save_state(&state_2).unwrap();

        // A lock file for phase 2, written directly in the documented
        // `.devflow/lock-{phase:02}` shape (lock.rs:4, :236) — pid on
        // line 1, start time on line 2 — naming `lock_pid` as holder.
        let lock_start_time = agent::process_start_time(lock_pid)
            .expect("must read the lock fixture's own recorded start time");
        let devflow_dir = project_root.join(".devflow");
        std::fs::create_dir_all(&devflow_dir).unwrap();
        std::fs::write(
            devflow_dir.join("lock-02"),
            format!("{lock_pid}\n{lock_start_time}"),
        )
        .unwrap();
        // Load-bearing: turns the implicit lock-file-format coupling
        // into a self-checking one — a future format change fails this
        // test loudly instead of silently exercising only half of it.
        assert_eq!(
            lock::holder_identity(&project_root, PhaseId::new(2)),
            Some((lock_pid, Some(lock_start_time))),
            "the directly-written lock file must read back through holder_identity exactly \
             as one lock::acquire itself wrote would"
        );

        registry::register_in(cache_dir.path(), &project_root, PhaseId::new(1)).unwrap();
        let registered_roots: Vec<PathBuf> = registry::load_roots_in(cache_dir.path())
            .into_iter()
            .map(|r| r.project_root)
            .collect();

        let reachable = registry_reachable_pids(&registered_roots);
        assert!(
            reachable.contains(&state_pid),
            "state_pid must be reachable via its recorded monitor_pid"
        );
        assert!(
            reachable.contains(&lock_pid),
            "lock_pid must be reachable via the lock file's holder_identity"
        );
        assert!(
            !reachable.contains(&orphan_pid),
            "orphan_pid is named by no state file and no lock file, so it must not be \
             reachable"
        );

        // Without this, the test could pass vacuously because the
        // census never saw the fixtures at all (T-25-15-11).
        let census = agent::discover_stray_devflow_processes();
        for (pid, label) in [
            (state_pid, "state"),
            (lock_pid, "lock"),
            (orphan_pid, "orphan"),
        ] {
            assert!(
                census.iter().any(|p| p.pid == pid),
                "the {label} fixture (pid {pid}) must be part of the real /proc census"
            );
        }

        let retained = retain_unreachable_strays(&census, &reachable);
        assert!(
            retained.iter().any(|p| p.pid == orphan_pid),
            "the orphan must survive the filter"
        );
        assert!(
            !retained.iter().any(|p| p.pid == state_pid),
            "the state-named pid must be filtered out"
        );
        assert!(
            !retained.iter().any(|p| p.pid == lock_pid),
            "the lock-held pid must be filtered out"
        );

        let findings = build_stray_process_findings(&retained);
        assert!(
            findings.iter().any(|f| f.pid == orphan_pid),
            "the orphan must produce a finding"
        );
        assert!(
            !findings.iter().any(|f| f.pid == state_pid),
            "the state-named pid must produce no finding"
        );
        assert!(
            !findings.iter().any(|f| f.pid == lock_pid),
            "the lock-held pid must produce no finding"
        );

        // Reap all three with a VERIFIED signal, never a bare one
        // (mirrors `reap_strays_e2e.rs:202-223`'s own teardown), plus a
        // final `wait()` on each to reclaim the zombie regardless.
        for pid in [state_pid, lock_pid, orphan_pid] {
            agent::terminate_and_verify(
                pid,
                agent::TERMINATE_VERIFY_WAIT,
                agent::TERMINATE_VERIFY_POLL,
            );
        }
        let _ = state_child.wait();
        let _ = lock_child.wait();
        let _ = orphan_child.wait();
    }

    /// 999.44's originating case, as an executable assertion rather than
    /// prose (see `25-15-PLAN.md`'s `<why_this_does_not_violate_d17>`):
    /// once a registered root is deleted off disk, its state file and
    /// lock file are gone with it, so it contributes ZERO pids to the
    /// reachable set — even while the OS process it named is still
    /// alive. The filter is therefore structurally incapable of hiding
    /// this population.
    #[test]
    fn a_deleted_root_contributes_nothing_to_the_reachable_set() {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("project-root");
        std::fs::create_dir_all(&root).unwrap();

        let mut child = spawn_wrapper_shaped_fixture();
        let pid = child.id();

        let mut state = State::new(PhaseId::new(1), AgentKind::Claude, Mode::Auto, root.clone());
        state.monitor_pid = Some(pid);
        workflow::save_state(&state).unwrap();

        assert!(
            registry_reachable_pids(std::slice::from_ref(&root)).contains(&pid),
            "the pid must be reachable while its root and state file still exist"
        );

        std::fs::remove_dir_all(&root).unwrap();
        assert!(!root.exists(), "the root must actually be gone");
        assert!(
            agent::agent_running(pid),
            "deleting the root must not touch the still-alive process"
        );

        let reachable_after_deletion = registry_reachable_pids(std::slice::from_ref(&root));
        assert!(
            reachable_after_deletion.is_empty(),
            "a deleted root's state file and lock file are gone with it, so it must \
             contribute nothing to the reachable set: got {reachable_after_deletion:?}"
        );

        let census = vec![agent::StrayProcess {
            pid,
            start_time: agent::process_start_time(pid)
                .expect("fixture must still be alive and readable"),
            layer: agent::StrayLayer::MonitorWrapper,
        }];
        let retained = retain_unreachable_strays(&census, &reachable_after_deletion);
        assert!(
            retained.iter().any(|p| p.pid == pid),
            "with the reachable set empty, the pid must still be treated as a stray"
        );

        // Verified reap (mirrors `reap_strays_e2e.rs:202-223`), plus a
        // final `wait()` to reclaim the zombie regardless.
        agent::terminate_and_verify(
            pid,
            agent::TERMINATE_VERIFY_WAIT,
            agent::TERMINATE_VERIFY_POLL,
        );
        let _ = child.wait();
    }

    /// Unit tests for the pure planning-doc staleness core (21b, D-04/D-05).
    /// `reconcile_planning_docs` takes an injected `tag_lookup` closure, so
    /// every test here runs with zero I/O and no real repository — mirrors
    /// `doctor_reconciliation`'s zero-I/O discipline above.
    #[cfg(test)]
    mod planning_doc_staleness {
        use super::*;

        const SAMPLE_TABLE: &str = "\
| Phase | Name | Version |
|---|---|---|
| 20 | Release Correctness | 1.7.0 |
| 10 | Logging | — |
| 1–5 | Core workflow | 0.1.0–0.6.0 |
| 9 | OSS Polish | 1.2.0 |
| 11 | GSD-Native | 1.2.0 |
";

        #[test]
        fn parse_planning_doc_versions_skips_non_semver_cells() {
            let rows = parse_planning_doc_versions(SAMPLE_TABLE, "ROADMAP.md");
            assert_eq!(
                rows,
                vec![
                    ("ROADMAP.md phase 20".to_string(), "1.7.0".to_string()),
                    ("ROADMAP.md phase 9".to_string(), "1.2.0".to_string()),
                    ("ROADMAP.md phase 11".to_string(), "1.2.0".to_string()),
                ],
                "em-dash and range cells must be skipped; duplicate versions across \
                 phases (9 and 11 both claim 1.2.0) must both still parse"
            );
        }

        #[test]
        fn parse_planning_doc_versions_accepts_v_prefixed_cells() {
            let text = "| Phase | Description | Version | Date |\n\
                         |---|---|---|---|\n\
                         | 18 | Dogfood Hardening | v1.5.0 | 2026-07-21 |\n";
            let rows = parse_planning_doc_versions(text, "STATE.md");
            assert_eq!(
                rows,
                vec![("STATE.md phase 18".to_string(), "v1.5.0".to_string())]
            );
        }

        #[test]
        fn parse_semver_rejects_ranges_and_em_dash() {
            assert_eq!(parse_semver("1.7.0"), Some((1, 7, 0)));
            assert_eq!(parse_semver("v1.7.0"), Some((1, 7, 0)));
            assert_eq!(parse_semver("0.1.0–0.6.0"), None);
            assert_eq!(parse_semver("—"), None);
            assert_eq!(parse_semver("1.7"), None);
            assert_eq!(parse_semver("1.7.0.1"), None);
        }

        #[test]
        fn reconcile_planning_docs_flags_problem_for_unreachable_post_cutoff_version() {
            let rows = vec![("ROADMAP.md phase 20".to_string(), "1.7.0".to_string())];
            let mut lookup = |_tag: &str| false; // no tag exists / unreachable
            let findings = reconcile_planning_docs(&rows, &mut lookup);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Problem);
            assert!(
                findings[0].repair.is_none(),
                "D-04: detection-only, no repair"
            );
            assert!(findings[0].detail.contains("v1.7.0"));
        }

        #[test]
        fn reconcile_planning_docs_downgrades_pre_cutoff_mismatch_to_warn() {
            // Phase 7 claims 1.0.0 in this repo's real ROADMAP.md/STATE.md,
            // but no v1.0.0 tag exists (tags start at v1.0.1) — must never
            // surface as Problem (RESEARCH Pitfall #2).
            let rows = vec![("ROADMAP.md phase 7".to_string(), "1.0.0".to_string())];
            let mut lookup = |_tag: &str| false;
            let findings = reconcile_planning_docs(&rows, &mut lookup);
            assert_eq!(findings.len(), 1);
            assert_eq!(
                findings[0].severity,
                Severity::Warn,
                "pre-v1.5.0 mismatches must downgrade to Warn, never Problem"
            );
            assert!(findings[0].repair.is_none());
        }

        #[test]
        fn reconcile_planning_docs_numeric_cutoff_is_not_lexicographic() {
            // A lexicographic string compare would sort "1.10.0" < "1.5.0"
            // and wrongly downgrade a real future release to Warn (Codex
            // MEDIUM, cross-AI review). The cutoff must compare
            // parse_semver's numeric tuple instead.
            let rows = vec![
                ("label A".to_string(), "1.10.0".to_string()),
                ("label B".to_string(), "1.4.0".to_string()),
            ];
            let mut lookup = |_tag: &str| false;
            let findings = reconcile_planning_docs(&rows, &mut lookup);
            assert_eq!(findings.len(), 2);
            assert_eq!(
                findings[0].severity,
                Severity::Problem,
                "1.10.0 is numerically >= v1.5.0 (post-cutoff), even though \
                 \"1.10.0\" < \"1.5.0\" as a string"
            );
            assert_eq!(
                findings[1].severity,
                Severity::Warn,
                "1.4.0 is numerically < v1.5.0 (pre-cutoff)"
            );
        }

        #[test]
        fn reconcile_planning_docs_produces_no_finding_when_tag_is_reachable() {
            let rows = vec![("ROADMAP.md phase 20".to_string(), "1.7.0".to_string())];
            let mut lookup = |_tag: &str| true; // tag exists and is reachable
            let findings = reconcile_planning_docs(&rows, &mut lookup);
            assert!(findings.is_empty());
        }

        #[test]
        fn reconcile_planning_docs_normalizes_bare_cell_to_v_prefixed_tag() {
            let rows = vec![("ROADMAP.md phase 20".to_string(), "1.7.0".to_string())];
            let mut seen_tag = None;
            let mut lookup = |tag: &str| {
                seen_tag = Some(tag.to_string());
                true
            };
            reconcile_planning_docs(&rows, &mut lookup);
            assert_eq!(seen_tag.as_deref(), Some("v1.7.0"));
        }

        #[test]
        fn reconcile_planning_docs_skips_a_malformed_row_defensively() {
            // Defensive path: reconcile must never panic even if handed a
            // row whose version cell isn't a semver (parse_planning_doc_versions
            // already filters this upstream, but reconcile must degrade, not die).
            let rows = vec![("bad row".to_string(), "not-a-version".to_string())];
            let mut lookup = |_tag: &str| false;
            let findings = reconcile_planning_docs(&rows, &mut lookup);
            assert!(findings.is_empty());
        }

        /// Fixture-backed proof of `tag_exists_and_reachable`'s two-check
        /// contract, mirroring `staleness::init_repo_with_diverged_commit`'s
        /// idiom: a real tempdir git repo with a tagged, reachable commit,
        /// an untagged commit, and a commit on a diverged, unreachable branch.
        fn init_tagged_repo(root: &Path) {
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
            git(&["init", "-q", "-b", "main"]);
            git(&["config", "user.email", "t@e.st"]);
            git(&["config", "user.name", "t"]);
            git(&["config", "commit.gpgsign", "false"]);
            git(&["config", "tag.gpgsign", "false"]);
            git(&["config", "core.hooksPath", "/dev/null"]);
            std::fs::write(root.join("a.txt"), "one").unwrap();
            git(&["add", "."]);
            git(&["commit", "-q", "-m", "base"]);
            git(&["tag", "v1.7.0"]);

            git(&["checkout", "-q", "-b", "side"]);
            std::fs::write(root.join("side.txt"), "s").unwrap();
            git(&["add", "."]);
            git(&["commit", "-q", "-m", "side"]);
            git(&["tag", "v9.9.9"]); // tagged, but only reachable from `side`, not `main`

            git(&["checkout", "-q", "main"]);
        }

        #[test]
        fn tag_exists_and_reachable_true_for_a_tagged_ancestor() {
            let dir = tempfile::tempdir().unwrap();
            init_tagged_repo(dir.path());
            assert!(tag_exists_and_reachable(dir.path(), "v1.7.0", "main"));
        }

        #[test]
        fn tag_exists_and_reachable_false_for_a_missing_tag() {
            let dir = tempfile::tempdir().unwrap();
            init_tagged_repo(dir.path());
            assert!(!tag_exists_and_reachable(dir.path(), "v0.0.1", "main"));
        }

        #[test]
        fn tag_exists_and_reachable_false_for_a_tag_unreachable_from_base() {
            let dir = tempfile::tempdir().unwrap();
            init_tagged_repo(dir.path());
            assert!(!tag_exists_and_reachable(dir.path(), "v9.9.9", "main"));
        }

        // -----------------------------------------------------------------
        // 27-04 (D-01/D-03): tag_exists_and_reachable's two direct sites
        // (base-commit lines 2886, 2892) now scrubbed via the git_command
        // constructor, called with project_root
        // -----------------------------------------------------------------

        /// D-01/D-03: `tag_exists_and_reachable` produces the correct,
        /// non-hijacked answer for `project_root` even when a hostile
        /// `GIT_DIR` points at an unrelated foreign repository that DOES
        /// carry a tag by the name under test — the dangerous, false-positive
        /// direction T-27-01 names explicitly (a foreign repository
        /// asserting a release tag already exists feeds a release-cut
        /// decision). `GIT_DIR` is genuinely present in a process's
        /// environment for this proof — never via `std::env::set_var` on
        /// THIS test's own process (Rust 2024 `unsafe`, unsound under
        /// threaded tests — Phase 25 D-14, and the plan's own instruction).
        /// Instead it is set only on a freshly spawned CHILD process: this
        /// same test binary, re-invoked filtered to just this one test —
        /// the same "spawned child only" shape used by
        /// `staleness::tests::embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir`
        /// (27-04), needed here for the identical reason:
        /// `tag_exists_and_reachable` is a private function with no
        /// injection point of its own, and its two git subcommands
        /// (`rev-parse --verify`, `merge-base --is-ancestor`) are both
        /// ref-resolving — the class 27-01-SUMMARY.md Deviation 1 verified
        /// (git 2.55.0) genuinely honors a `GIT_DIR` chained directly onto a
        /// `git_command`-built Command, so a literal reproduction of that
        /// shape against the real function would prove nothing specific to
        /// it.
        #[test]
        fn tag_exists_and_reachable_resolves_caller_root_under_a_hostile_git_dir() {
            const INNER_ROOT: &str = "DEVFLOW_27_04_TAG_INNER_ROOT";
            const INNER_TAG: &str = "DEVFLOW_27_04_TAG_INNER_TAG";
            const INNER_BASE: &str = "DEVFLOW_27_04_TAG_INNER_BASE";

            if let Ok(root) = std::env::var(INNER_ROOT) {
                // Inner mode: this process was spawned by the outer half
                // below with GIT_DIR pointed at a foreign repository that
                // DOES carry the tag under test — scoped to this child
                // process only.
                let tag = std::env::var(INNER_TAG).expect("inner tag env set by parent");
                let base = std::env::var(INNER_BASE).expect("inner base env set by parent");
                assert!(
                    !tag_exists_and_reachable(Path::new(&root), &tag, &base),
                    "a hostile GIT_DIR pointed at a foreign repository that DOES \
                     carry this tag must not cause tag_exists_and_reachable to \
                     report it as belonging to project_root"
                );
                return;
            }

            // Outer mode: build the real repository with a base branch and
            // NO tag by the name under test, plus a second, unrelated
            // foreign repository (reusing init_tagged_repo verbatim) that
            // DOES carry that tag, reachable from its own base branch — the
            // dangerous direction.
            let real = tempfile::tempdir().unwrap();
            let real_root = real.path();
            let git = |args: &[&str]| {
                assert!(
                    devflow_core::test_support::git_command(real_root)
                        .args(args)
                        .output()
                        .unwrap()
                        .status
                        .success(),
                    "git {args:?} failed"
                );
            };
            git(&["init", "-q", "-b", "main"]);
            git(&["config", "user.email", "t@e.st"]);
            git(&["config", "user.name", "t"]);
            git(&["config", "commit.gpgsign", "false"]);
            git(&["config", "core.hooksPath", "/dev/null"]);
            std::fs::write(real_root.join("a.txt"), "one").unwrap();
            git(&["add", "."]);
            git(&["commit", "-q", "-m", "base"]);
            // real_root deliberately carries no tag named v1.7.0.

            let foreign = tempfile::tempdir().unwrap();
            init_tagged_repo(foreign.path());

            let exe = std::env::current_exe().expect("current_exe for child re-invocation");
            let status = std::process::Command::new(&exe)
                .arg("tag_exists_and_reachable_resolves_caller_root_under_a_hostile_git_dir")
                .arg("--test-threads=1")
                .env(INNER_ROOT, real_root.to_str().unwrap())
                .env(INNER_TAG, "v1.7.0")
                .env(INNER_BASE, "main")
                .env("GIT_DIR", foreign.path().join(".git"))
                .status()
                .expect("spawn hostile child test process");
            assert!(
                status.success(),
                "child test process (hostile GIT_DIR pointed at a foreign repo \
                 that DOES carry v1.7.0) must still report \
                 tag_exists_and_reachable == false for the real repository; \
                 child exit status {status:?}"
            );
        }

        /// D-05/D-04: a MISSING `.planning/ROADMAP.md`/`STATE.md` must yield
        /// no findings and never an error — `doctor` must not fabricate a
        /// `Problem` from an absent doc. Proven against a tempdir with no
        /// `.planning/` directory at all, not just asserted.
        #[test]
        fn collect_planning_doc_findings_missing_files_yield_no_findings_not_error() {
            let dir = tempfile::tempdir().unwrap();
            let findings = collect_planning_doc_findings(dir.path());
            assert!(
                findings.is_empty(),
                "a project with no .planning/ dir at all must yield zero findings, not an error"
            );
        }

        /// 999.30 / DEN-55 WR-02: `collect_planning_doc_findings` must
        /// reconcile against the named `MAIN` production branch, not an
        /// unlinked duplicate. `init_tagged_repo` tags `v9.9.9` only on
        /// `side`, unreachable from `main` — so a ROADMAP.md claiming
        /// `v9.9.9` must surface a `Problem` when reconciled against `MAIN`.
        #[test]
        fn collect_planning_doc_findings_reconciles_against_main() {
            let dir = tempfile::tempdir().unwrap();
            init_tagged_repo(dir.path());
            std::fs::create_dir_all(dir.path().join(".planning")).unwrap();
            std::fs::write(
                dir.path().join(".planning/ROADMAP.md"),
                "| Phase | Name | Version |\n|---|---|---|\n| 99 | Fixture | 9.9.9 |\n",
            )
            .unwrap();

            let findings = collect_planning_doc_findings(dir.path());

            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Problem);
        }

        #[test]
        fn render_planning_doc_text_reports_consistent_when_no_findings() {
            assert_eq!(
                render_planning_doc_text(&[]),
                "\nplanning docs: consistent with git tags\n"
            );
        }

        #[test]
        fn render_planning_doc_text_lists_each_finding_detail() {
            let findings = vec![PlanningDocFinding {
                source: "ROADMAP.md phase 20".to_string(),
                claim: "ROADMAP.md phase 20 claims v1.7.0".to_string(),
                severity: Severity::Problem,
                detail: "ROADMAP.md phase 20 claims v1.7.0, but no git tag `v1.7.0` exists"
                    .to_string(),
                repair: None,
            }];
            let text = render_planning_doc_text(&findings);
            assert!(text.contains("[problem]"));
            assert!(text.contains("ROADMAP.md phase 20 claims v1.7.0"));
        }

        #[test]
        fn render_planning_doc_findings_json_is_an_array_of_objects() {
            let findings = vec![PlanningDocFinding {
                source: "ROADMAP.md phase 20".to_string(),
                claim: "ROADMAP.md phase 20 claims v1.7.0".to_string(),
                severity: Severity::Problem,
                detail: "detail text".to_string(),
                repair: None,
            }];
            let value = render_planning_doc_findings_json(&findings);
            assert!(value.is_array());
            let arr = value.as_array().unwrap();
            assert_eq!(arr.len(), 1);
            assert_eq!(arr[0]["severity"], "problem");
            assert_eq!(arr[0]["source"], "ROADMAP.md phase 20");
            assert_eq!(arr[0]["repair"], serde_json::Value::Null);
        }

        /// D-05/Pattern 2: `doctor --json` must stay a SINGLE JSON object
        /// with `planning_doc_staleness` as a THIRD key, never a second
        /// top-level array — the exact WR-01 regression class this phase
        /// must not reintroduce.
        #[test]
        fn doctor_json_body_carries_planning_doc_staleness_as_a_third_key() {
            let checks: Vec<Check> = Vec::new();
            let facts: Vec<PhaseFacts> = Vec::new();
            let doc_findings = vec![PlanningDocFinding {
                source: "ROADMAP.md phase 20".to_string(),
                claim: "claim".to_string(),
                severity: Severity::Problem,
                detail: "detail".to_string(),
                repair: None,
            }];
            let body = doctor_json_body(&checks, &facts, &doc_findings, &[]);
            let obj = body.as_object().unwrap();
            assert_eq!(
                obj.len(),
                4,
                "must be exactly {{environment, reconciliation, planning_doc_staleness, \
                 stray_processes}}: {body}"
            );
            assert!(obj.contains_key("environment"));
            assert!(obj.contains_key("reconciliation"));
            let staleness = obj["planning_doc_staleness"].as_array().unwrap();
            assert_eq!(staleness.len(), 1);
            assert!(obj.contains_key("stray_processes"));
        }
    }

    /// 23-06 Task 3 acceptance: `--require-shipped` is exit-code-stable on
    /// the strict `shipped` predicate ALONE — a shipped phase's call
    /// succeeds, an otherwise-identical unshipped phase's call fails, with
    /// every other evidence field held constant (neither phase has any git
    /// state or persisted workflow state in this fixture).
    #[test]
    fn evidence_require_shipped_exits_ok_iff_the_phase_has_shipped() {
        let dir = tempfile::tempdir().unwrap();
        events::emit(
            dir.path(),
            PhaseId::new(30),
            "workflow_shipped",
            serde_json::json!({"stage": "ship"}),
        );

        assert!(evidence(dir.path(), PhaseId::new(30), false, true).is_ok());
        assert!(evidence(dir.path(), PhaseId::new(31), false, true).is_err());
    }

    /// 23-06 Task 3 acceptance: a phase that only stopped (`--until`) must
    /// fail `--require-shipped` with a message that says so explicitly,
    /// not a generic "not shipped" — this is the confusing case named in
    /// Task 1's acceptance criteria ("it finished but it did not ship").
    #[test]
    fn evidence_require_shipped_names_stopped_at_rather_than_generic_not_shipped() {
        let dir = tempfile::tempdir().unwrap();
        events::emit(
            dir.path(),
            PhaseId::new(32),
            "workflow_finished",
            serde_json::json!({"reason": "stopped_at", "stage": "plan"}),
        );

        let err = evidence(dir.path(), PhaseId::new(32), false, true).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("stopped"),
            "message must name the stopped-at case, got: {message}"
        );
    }

    /// 23-06 Task 3 acceptance: the `--require-shipped` failure message is a
    /// single line (it ends up verbatim in a gate context read on a phone)
    /// and names the phase.
    #[test]
    fn evidence_require_shipped_failure_message_is_single_line_and_names_phase() {
        let dir = tempfile::tempdir().unwrap();

        let err = evidence(dir.path(), PhaseId::new(33), false, true).unwrap_err();
        let message = err.to_string();
        assert!(
            !message.contains('\n'),
            "message must be one line: {message:?}"
        );
        assert!(
            message.contains("33"),
            "message must name the phase: {message}"
        );
    }

    #[test]
    fn changelog_version_check_flags_mismatch_and_passes_on_agreement() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let write_workspace = |version: &str| {
            std::fs::write(
                root.join("Cargo.toml"),
                format!("[workspace.package]\nversion = \"{version}\"\n"),
            )
            .unwrap();
        };

        // Mismatch: changelog 2.4.0 vs workspace 2.5.0 → fail, "Cargo.toml ahead".
        write_workspace("2.5.0");
        std::fs::write(root.join("CHANGELOG.md"), "## 2.4.0 — 2026-01-01\n").unwrap();
        let mismatch = check_changelog_version(root);
        assert_eq!(mismatch.status, "fail");
        assert!(
            mismatch.version.unwrap().contains("Cargo.toml ahead"),
            "workspace 2.5.0 is newer than changelog 2.4.0"
        );

        // Reverse: changelog 2.6.0 vs workspace 2.5.0 → fail, "changelog ahead".
        write_workspace("2.5.0");
        std::fs::write(root.join("CHANGELOG.md"), "## 2.6.0 — 2026-01-01\n").unwrap();
        let reverse = check_changelog_version(root);
        assert_eq!(reverse.status, "fail");
        assert!(
            reverse.version.unwrap().contains("changelog ahead"),
            "changelog 2.6.0 is newer than workspace 2.5.0"
        );

        // Agreement: both 2.5.0 → ok.
        write_workspace("2.5.0");
        std::fs::write(root.join("CHANGELOG.md"), "## 2.5.0 — 2026-08-15\n").unwrap();
        assert_eq!(check_changelog_version(root).status, "ok");

        // Bracketed Keep-a-Changelog heading `## [2.5.0]` → parses, ok.
        std::fs::write(root.join("CHANGELOG.md"), "## [2.5.0] - 2026-08-15\n").unwrap();
        assert_eq!(check_changelog_version(root).status, "ok");

        // Missing heading → warn, not a hard fail.
        std::fs::write(root.join("CHANGELOG.md"), "no heading here\n").unwrap();
        assert_eq!(check_changelog_version(root).status, "warn");
    }

    // ------------------------------------------------------------------
    // Phase 41 Task 7 (D-04/F7): the doctor antigravity entry lives behind an
    // assertable seam and reports presence, never a hard failure. The
    // PATH-based presence reporting tests live in tests/doctor_antigravity.rs
    // (integration — spawning the real binary from a unit-test harness
    // re-enters the suite).
    // ------------------------------------------------------------------

    #[test]
    fn doctor_includes_antigravity_check_in_the_seam() {
        // The seam asserts the LIST, not the machine's PATH state: the entry
        // exists with the agy probe shape regardless of whether `agy` happens
        // to be installed here.
        let checks = doctor_checks();
        let antg = checks
            .iter()
            .find(|c| c.name == "antigravity")
            .expect("doctor_checks() must contain the antigravity entry");
        // When the probe reports missing, the hint names the agy binary. When
        // agy IS installed on this machine the hint is None (status ok), so
        // the hint assertion is conditional.
        if antg.status == "missing" {
            assert!(
                antg.install_hint.as_deref().unwrap_or("").contains("agy"),
                "the hint must name the agy binary: {:?}",
                antg.install_hint
            );
        }
    }

    #[test]
    fn doctor_includes_hermes_check_in_the_seam() {
        let checks = doctor_checks();
        let hermes = checks
            .iter()
            .find(|c| c.name == "hermes")
            .expect("doctor_checks() must contain the hermes entry");
        if hermes.status == "missing" {
            assert!(
                hermes
                    .install_hint
                    .as_deref()
                    .unwrap_or("")
                    .contains("hermes"),
                "the hint must name the hermes binary: {:?}",
                hermes.install_hint
            );
        }
    }
}
