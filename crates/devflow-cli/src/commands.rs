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
    print_fallback_notice, validate_fallback_agent,
};
use crate::staleness::{enforce_build_staleness, run_git_stdout};
use devflow_core::agent;
use devflow_core::agent_result;
use devflow_core::agents;
use devflow_core::config::{self, DEVELOP, FEATURE_PREFIX, MAIN};
use devflow_core::events;
use devflow_core::gates::{
    AnswerRead, GateAction, GateError, GateFile, GateId, GateResponse, Gates, OpenGate,
    QuarantinedAnswer, WaiterStatus, is_gate_id,
};
use devflow_core::git::{GitFlow, git_command, hermetic_command};
use devflow_core::history;
use devflow_core::liveness;
use devflow_core::mode::Mode;
use devflow_core::monitor;
use devflow_core::phase_id::PhaseId;
use devflow_core::recover;
use devflow_core::registry;
use devflow_core::ship_evidence;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, PendingSupervisor, State, SupervisorRecord};
use devflow_core::supervisor;
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
///
/// **Two ORDERED calls (46-02, D-07/D-08).** `rev-parse --verify` is a
/// revision PARSER, and using one as a ref existence check is the root of
/// VALID-01: revision SUFFIX syntax survives the `refs/heads/` prefix, so
/// `workspace/example~1`, `develop@{0}` and `develop^{}` all satisfied it.
/// `show-ref --verify` is a ref LOOKUP and refuses all three. It runs FIRST
/// and alone decides accept-versus-reject. `check-ref-format` then runs only
/// on an ALREADY-DECIDED refusal, purely to label it — it cannot decide
/// existence (it accepts `nonexistent-xyz`), so reversing the order would
/// accept and reject nothing correctly. Ref-name validity is delegated wholly
/// to that plumbing rather than to a hand-rolled denylist, which would drift
/// from git's own rules (`..`, trailing `.lock`, a bare `@`, control
/// characters) while the plumbing cannot.
///
/// **What this does NOT close (R-01, `46-REVIEWS.md`).** This function
/// qualifies `refs/heads/{base}`, but `crates/devflow-core/src/worktree.rs:64-83`
/// forwards the RAW `{base}` to `git worktree add`. A value that is both a
/// legal literal ref name and a revision expression (`@` is the demonstrated
/// case) passes here and then forks from something else. That asymmetry is
/// pre-existing, is not narrowed by the switch to `show-ref`, and is tracked
/// as R-01 — closing it needs validator and consumer to agree on one spelling.
pub(crate) fn ensure_base_is_a_local_branch(
    project_root: &Path,
    base: &str,
) -> Result<(), CliError> {
    // Bound ONCE and shared by both calls, so the two can never diverge. The
    // raw `base` is passed to neither: it is option-shaped for a value
    // beginning with `-` (`check-ref-format -x` exits 129, parsed as a flag),
    // and it also misclassifies plain one-level branch names as malformed
    // unless `--allow-onelevel` is added. The qualified form needs no flags.
    let qualified = format!("refs/heads/{base}");

    // D-07: existence.
    let exists = git_command(project_root)
        .args(["show-ref", "--verify", "--quiet", qualified.as_str()])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if exists {
        return Ok(());
    }

    // D-08: classify the refusal that has already been decided above.
    //
    // The two `unwrap_or` defaults deliberately differ. Existence fails CLOSED
    // (`false`): an unspawnable git refuses the base, exactly as before this
    // change. Classification fails OPEN toward the EXISTING message (`true`):
    // an unspawnable git must not emit a new "malformed" claim it has no
    // evidence for. `check-ref-format` needs no repository — it is a pure text
    // check — so it cannot be perturbed by repository state.
    let well_formed = git_command(project_root)
        .args(["check-ref-format", qualified.as_str()])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(true);

    if well_formed {
        return Err(CliError::Message(format!(
            "configured base branch `{base}` is not a local branch in this repository. \
             DevFlow forks phase worktrees from it and merges phase work back into it, so it \
             must be a branch — not a remote-tracking name, a `refs/heads/` path, `HEAD`, or a \
             commit SHA. Create it locally (e.g. `git branch {base} origin/{base}`) and re-run."
        )));
    }

    Err(CliError::Message(format!(
        "configured base branch `{base}` is not a well-formed branch name — git rejects \
         `{qualified}` as a ref. DevFlow forks phase worktrees from the base and merges phase \
         work back into it, so it must name a branch, not a revision expression such as \
         `~1`, `@{{0}}` or `^{{}}`. Set the base to the branch itself and re-run."
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

/// Close every stage's leftover gate before a new run starts. An unused answer is set
/// aside, never deleted, and each one is named on `warnings`. A close that fails only
/// warns: a leftover answer cannot decide the new run's gate anyway, because that gate
/// opens under a new id and sets aside whatever is in the stage's slots (D-05, D-06).
fn set_aside_leftover_gates(
    project_root: &Path,
    phase: PhaseId,
    warnings: &mut dyn std::io::Write,
) {
    for stage in [
        Stage::Define,
        Stage::Plan,
        Stage::Code,
        Stage::Validate,
        Stage::Ship,
    ] {
        match Gates::close(project_root, phase, stage) {
            Ok(report) => {
                for path in report.quarantined {
                    let _ = writeln!(
                        warnings,
                        "warning: set aside an unused answer for phase {phase} {stage}: {}",
                        path.display()
                    );
                }
            }
            Err(err) => {
                let _ = writeln!(
                    warnings,
                    "warning: could not clear stale gate files for phase {phase} {stage}: {err}"
                );
            }
        }
    }
}

/// Where `start` takes the run's rate-limit fallback agent from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FallbackSource {
    /// `devflow start`: the `--fallback-agent` flag when given, else
    /// `devflow.toml`'s `fallback_agent` key (R-3).
    FlagOrConfig(Option<AgentKind>),
    /// `devflow parallel`, which takes no `--fallback-agent`: no fallback, and
    /// `devflow.toml`'s key is not read (the key is read by `devflow start` only).
    NoFallback,
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
    fallback: FallbackSource,
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
    // negative form. Combined before `create_run_state` below, so the
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
    // Persist the operator's gate notify hook for the processes that outlive
    // this one (monitor chain, rate-limit resume), on the same "before
    // `create_run_state`" idiom. Captured here rather than in `State::new`
    // so constructing a state never reads the environment (49-REVIEW.md
    // WR-01); `resume` and `advance` apply the same rule (CR-03).
    state.gate_notify_cmd = devflow_core::gates::apply_notify_env(state.gate_notify_cmd.take());

    // D-11's opt-out (31-04), combined on the same shape and for the same
    // reasons as `yes_ship` directly above: OR-ed with an environment override
    // rather than replacing it, before `create_run_state` below so the
    // persisted value exists before any detached monitor consults it, and with
    // a notice when the environment ALONE supplied it — a standing default is
    // never a silent one.
    if crate::pipeline_launch::apply_legacy_launch_opt_out(&mut state, legacy_claude_launch) {
        println!(
            "note: legacy Claude launch forced by DEVFLOW_CLAUDE_LEGACY_LAUNCH \
             (D-11, 31-CONTEXT.md) — a persisted default is never a silent one"
        );
    }

    // R-3: `devflow.toml`'s `fallback_agent` key, combined with the flag the way
    // `yes_ship` is above: the flag wins when given, the key applies when it is
    // absent. Resolved unconditionally, also when the flag is given, so a broken
    // key refuses `start` whatever the flag says: the flag wins the value, it does
    // not excuse a broken config file (stricter than `base_branch`, whose env
    // override skips the file). `devflow parallel` passes `NoFallback`: it has no
    // flag to override the key with, so it arms none, and checks and names the key
    // itself before it launches anything.
    let (fallback_agent, from_config) = match fallback {
        FallbackSource::FlagOrConfig(flag) => {
            let config_fallback =
                config::fallback_agent(project_root).map_err(CliError::Message)?;
            (
                flag.or(config_fallback),
                flag.is_none() && config_fallback.is_some(),
            )
        }
        FallbackSource::NoFallback => (None, false),
    };

    // D-01, D-02, D-03: the fallback is checked against the state as combined above
    // (agent, mode, legacy opt-out) and set before `dry_run`, the holder and the
    // reservation, so a refused fallback changes nothing and a dry run shows it.
    if let Some(fallback) = fallback_agent {
        // D-02 and D-03 apply to the key's value exactly as to the flag's; a refusal
        // of a key-supplied fallback also names the key and how to override it.
        validate_fallback_agent(&state, fallback).map_err(|refusal| {
            if from_config {
                CliError::Message(format!(
                    "devflow.toml's fallback_agent ({fallback}) is refused: {refusal}\npass \
                     --fallback-agent <other> to choose another fallback, or remove \
                     fallback_agent from devflow.toml"
                ))
            } else {
                refusal
            }
        })?;
        state.fallback_agent = Some(fallback);
        print_fallback_notice(fallback, state.agent);
        // A standing default is never a silent one: name the key when it alone
        // supplied the fallback, as the `yes_ship` notice does.
        if from_config {
            println!(
                "note: fallback agent {fallback} set by devflow.toml (fallback_agent) — pass \
                 --fallback-agent to choose another"
            );
        }
    }

    if dry_run {
        print_dry_run(&state);
        return Ok(());
    }

    // D-17: hold the phase from before the reservation until this returns, so `stop`
    // reaches this process before its spawn. A launcher refused here has reserved nothing,
    // so a racing holder's own launch never meets a pending record left by it.
    let _phase_holder = claim_foreground_holder(project_root, phase)?;

    // D-01, D-08: the reservation excludes a second launcher. Its guard runs before
    // any gate cleanup, git mutation or run state, so a refusal changes nothing.
    let reserved = reserve_launch(
        project_root,
        phase,
        LaunchVerb::Start,
        Some(&state),
        registry::cache_dir().as_deref(),
    )?;
    let launch_id = reserved.reservation.launch_id().to_string();
    let mut created = reserved.created;
    crate::pipeline_launch::hold_reservation(reserved.reservation);
    #[cfg(test)]
    crate::pipeline_launch::launch_test_hold("holder_claimed");
    let result = start_reserved(
        project_root,
        state,
        force,
        worktree,
        &launch_id,
        &mut created,
    );
    crate::pipeline_launch::abandon_reservation(project_root, phase, created);
    result
}

/// [`start`] after its reservation: everything that can fail or return without
/// launching, so `start` gives the reservation back whichever way this ends.
/// `created` goes `false` once the state file holds this run, not only the
/// reservation.
fn start_reserved(
    project_root: &Path,
    mut state: State,
    force: bool,
    worktree: bool,
    launch_id: &str,
    created: &mut bool,
) -> Result<(), CliError> {
    let (phase, agent, mode) = (state.phase, state.agent, state.mode);
    set_aside_leftover_gates(project_root, phase, &mut std::io::stderr());

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
        // A commit-ish that is not a local branch is forwarded raw to
        // `git worktree add`, so the base is checked with `show-ref --verify`
        // — a ref lookup, not the revision parser that let suffix syntax
        // through (46-02, D-07). Still scoped to an explicitly configured
        // base so the default path's existing fall-open for a clone with no
        // local `develop` is untouched.
        ensure_base_is_a_local_branch(project_root, base)?;
    }
    // CR-02 (45-REVIEW.md): persist the resolved trunk on the same "before
    // `create_run_state`" idiom `yes_ship` uses directly above, and for
    // the same reason — `advance`, `resume` and the checkout hooks that
    // merge this phase branch all run in SEPARATE processes whose
    // environment need not carry `DEVFLOW_BASE_BRANCH`. Without this the
    // resolved value dies with the `start` process and every later site
    // silently re-resolves `develop`.
    state.base_branch = Some(resolved_base.value.clone());
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
    // CR-45-03: `for_project` reads `devflow.toml` / the environment AGAIN,
    // which is a second resolution of a value this function already resolved
    // fail-hard at the top and persisted onto `State`. The two can disagree
    // — `git_flow_for_project` falls SOFT to `develop` where `base_branch`
    // fails hard — and the message below interpolates `{base}` from the first
    // resolution while the number came from the second, so an operator could
    // be told how far behind `workspace/x` they are from a count taken
    // against `develop`. One resolution, reused.
    let resolved_git_flow = devflow_core::config::GitFlowConfig {
        develop: resolved_base.value.clone(),
        ..devflow_core::config::GitFlowConfig::default()
    };
    if !worktree
        && let Ok((_ahead, behind)) =
            GitFlow::with_config(project_root, resolved_git_flow.clone()).divergence_from_develop()
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
        // `develop` anyway. Both fork paths now agree, and (CR-45-03) they
        // agree on the value resolved ONCE above and persisted onto `State`,
        // not on a fresh read that can differ from it.
        let git = GitFlow::with_config(project_root, resolved_git_flow.clone());
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
    // that copy does not exist (F-10). It sits BEFORE
    // `create_run_state` below, matching this function's own
    // "combine before `create_run_state`" idiom for `yes_ship` and D-11's
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
    // closed (CR-01) and must not be reintroduced. Persisted by
    // `create_run_state` below rather than by a second write.
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
    // Placement is also deliberately BEFORE `create_run_state` below: a
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
    // That holds even for a supervisor that did not confirm a stop: with no
    // state the next launch sweeps the sockets, which finds it, and a state
    // naming neither record would skip that sweep.
    create_run_state(project_root, phase, &mut state, launch_id)?;
    *created = false;
    events::emit(
        project_root,
        phase,
        "workflow_started",
        workflow_started_payload(&state),
    );
    if let Err(err) = launch_stage(&mut state, None, None) {
        // A stop at a safe point is recorded and ends the launch with an error; a stopped
        // run keeps its state for `devflow resume`.
        if state.stopped {
            return Err(err);
        }
        if let Err(clear_err) = workflow::clear_state(project_root, phase) {
            eprintln!("warning: could not clear state after failed launch: {clear_err}");
        }
        return Err(err);
    }
    // A stop honoured before the spawn is an `Ok` from `launch_stage`, with nothing running.
    if state.stopped {
        println!(
            "phase {phase} stopped before anything was launched ({}); run `devflow resume --phase \
             {phase}` to continue it",
            state.stop_reason.as_deref().unwrap_or("no reason recorded")
        );
        return Ok(());
    }
    println!(
        "started phase {} in {mode} mode at {} — monitor will auto-advance",
        state.phase, state.started_at
    );
    println!("  watch live: devflow logs -f --phase {phase}");
    Ok(())
}

/// Whether a Ship answer waits for `devflow ship`: `state` is at Ship and the gate's slot
/// holds an answer bound to that gate, not yet claimed. `Gates::list_open` hides such a gate.
fn ship_answer_stored(state: &State) -> bool {
    let (root, phase, stage) = (&state.project_root, state.phase, Stage::Ship);
    if state.stage != stage {
        return false;
    }
    let Ok(Some(id)) = gate_file_id(root, phase, stage) else {
        return false;
    };
    let Ok(gate_id) = GateId::parse(&id) else {
        return false;
    };
    matches!(
        Gates::read_answer(root, phase, stage, &gate_id),
        AnswerRead::Bound(_)
    ) && !Gates::claim_path(root, phase, stage, &gate_id).exists()
}

/// Whether `state` is only what a `start`'s reservation wrote: that `start` died before
/// preparing the run, so no worktree was forked, no base resolved and none of its guards
/// passed. Every prepared run captures its verification baseline in [`start_reserved`], and
/// nothing resets it. `worktree_path` cannot say this, because `--no-worktree` leaves it
/// `None` for a prepared run.
pub(crate) fn run_never_prepared(state: &State) -> bool {
    state.stage == Stage::Define && !state.verification_baseline_captured
}

/// The `start` that prepares what [`run_never_prepared`] says is missing. A worktree already
/// at the phase's path makes a plain `start` refuse, so that case names `--force`, with what
/// it costs after a `#` so the line still pastes into a shell.
fn restart_verb(state: &State) -> String {
    let verb = format!(
        "devflow start --phase {} --agent {} --mode {}",
        state.phase, state.agent, state.mode
    );
    let worktree_path = worktree::phase_path(&state.project_root, state.phase);
    if worktree_path.exists() {
        format!(
            "{verb} --force  # recreates {} from the base, discarding what is in it",
            worktree_path.display()
        )
    } else {
        verb
    }
}

/// `resume`'s and a hand-run `advance`'s refusal of a run [`run_never_prepared`] names.
/// Launching it would run the agent in the primary checkout, on whatever branch is
/// checked out.
pub(crate) fn refuse_unprepared_run(state: &State) -> Result<(), CliError> {
    if !run_never_prepared(state) {
        return Ok(());
    }
    Err(CliError::Message(format!(
        "phase {phase}: its `devflow start` never finished preparing the run, so there is \
         nothing to continue; run `{verb}`",
        phase = state.phase,
        verb = restart_verb(state),
    )))
}

/// Write a fresh run's state for `start`, under the write lock, over the state its
/// reservation left. This is the one documented write of a whole state: it replaces
/// what is on disk, keeping the reservation, and only while the state still names
/// `launch_id`. If it no longer does, the reservation was cleared, and nothing launches.
fn create_run_state(
    project_root: &Path,
    phase: PhaseId,
    state: &mut State,
    launch_id: &str,
) -> Result<(), CliError> {
    workflow::update_state_with(project_root, phase, |existing| {
        let pending = match existing {
            workflow::Existing::Loaded(reserved) => reserved.pending_supervisor,
            _ => None,
        }
        .filter(|pending| pending.launch_id == launch_id);
        if pending.is_none() {
            return Err(CliError::Message(format!(
                "phase {phase}: launch reservation {launch_id} was cleared while `start` \
                 prepared the run — nothing was launched"
            )));
        }
        state.pending_supervisor = pending;
        Ok::<_, CliError>((Some(state.clone()), ()))
    })
}

/// Copy a state file that does not load aside before a reservation replaces it, since a
/// refused `start` then removes what the reservation wrote. A copy that cannot be made
/// refuses the launch. Its name does not read as a state ([`workflow::list_states`]).
fn keep_unloadable_state(state_file: &Path, kept: &Path) -> Result<(), CliError> {
    std::fs::copy(state_file, kept)
        .map(|_| ())
        .map_err(|error| {
            CliError::Message(format!(
                "could not keep a copy of the state file {} at {}, which does not load, so it was \
             not replaced: {error}",
                state_file.display(),
                kept.display()
            ))
        })
}

/// A launch's reservation, made at the top of `start` and `resume`.
pub(crate) struct Reserved {
    /// The launch id and held liveness lock the supervisor will take over.
    pub(crate) reservation: monitor::Reservation,
    /// Whether the reservation's write created the state file, so that giving the
    /// reservation back removes it.
    pub(crate) created: bool,
    /// The state as the reservation wrote it.
    pub(crate) state: State,
}

/// Record the launch before its supervisor exists (D-01): take a reservation, then
/// in one write-lock closure run the live-run guard and write
/// `state.pending_supervisor`, so two launchers cannot both pass the guard (V-05).
/// `fresh` is `start`'s new state, written when none loads; `resume` passes `None`
/// and gets its missing-state or load error once the guard has passed.
///
/// A refused launch gives the reservation back, so it leaves no liveness file and
/// changes no state. The closure's only I/O is the guard's bounded probes: no spawn,
/// hook or gate wait happens under the write lock (D-08).
pub(crate) fn reserve_launch(
    project_root: &Path,
    phase: PhaseId,
    verb: LaunchVerb,
    fresh: Option<&State>,
    cache_dir: Option<&Path>,
) -> Result<Reserved, CliError> {
    let ctx = crate::pipeline_launch::launch_context()?;
    let reservation =
        monitor::Reservation::for_phase(&ctx, project_root, phase).map_err(|err| {
            CliError::Message(format!(
                "could not reserve a launch of phase {phase}: {err}"
            ))
        })?;
    let pending = reservation.pending_record();
    let kept_unloadable_path = project_root.join(".devflow").join(format!(
        "unloadable-state-{}-{}.json",
        phase.padded(),
        reservation.launch_id()
    ));
    let written = workflow::update_state_with(project_root, phase, |existing| {
        refuse_launch_over_a_live_run(project_root, phase, verb, &existing, cache_dir)?;
        if fresh.is_some() {
            refuse_stateless_launch_beside_a_live_lock(
                project_root,
                phase,
                reservation.launch_id(),
                &existing,
            )?;
        }
        #[cfg(test)]
        crate::pipeline_launch::launch_test_hold("guarded");
        let state_file = workflow::state_path(project_root, phase);
        let (mut next, created, replaced) = match (existing, fresh) {
            (workflow::Existing::Loaded(state), _) => (state, false, false),
            (workflow::Existing::Missing, Some(fresh)) => (fresh.clone(), true, false),
            (workflow::Existing::Unloadable(_), Some(fresh)) => {
                keep_unloadable_state(&state_file, &kept_unloadable_path)?;
                (fresh.clone(), true, true)
            }
            (workflow::Existing::Missing, None) => {
                return Err(workflow::WorkflowError::MissingState(state_file).into());
            }
            (workflow::Existing::Unloadable(error), None) => return Err(error.into()),
        };
        // The guard has passed, so a reservation still here is abandoned.
        let abandoned = next.pending_supervisor.replace(pending);
        if let Some(abandoned) = &abandoned {
            remove_abandoned_liveness_file(project_root, abandoned);
        }
        let outcome = ReserveOutcome {
            state: next.clone(),
            created,
            replaced: replaced.then(|| (state_file, kept_unloadable_path.clone())),
            abandoned: abandoned.map(|abandoned| abandoned.launch_id),
        };
        Ok::<_, CliError>((Some(next), outcome))
    });
    let outcome = match written {
        Ok(outcome) => outcome,
        Err(err) => {
            reservation.discard();
            return Err(err);
        }
    };
    if let Some((path, kept)) = outcome.replaced {
        println!(
            "note: replaced the state file {}, which did not load, with this launch's \
             reservation; its bytes are kept at {}",
            path.display(),
            kept.display()
        );
    }
    if let Some(launch_id) = outcome.abandoned {
        println!("cleared an abandoned launch reservation {launch_id}");
    }
    #[cfg(test)]
    crate::pipeline_launch::launch_test_hold("reserved");
    Ok(Reserved {
        reservation,
        created: outcome.created,
        state: outcome.state,
    })
}

/// A state that is missing or does not load names no pending launch, and a supervisor
/// that has not published shows in no socket sweep, so creating or replacing the state
/// could launch a second one beside it. Only its held liveness lock shows. That file
/// names no phase, so a lock another phase's loadable state records is that phase's;
/// any other held lock refuses: fail closed, naming its holder. A lock that cannot be
/// probed counts as held (T-50-40).
fn refuse_stateless_launch_beside_a_live_lock(
    project_root: &Path,
    phase: PhaseId,
    own_launch_id: &str,
    existing: &workflow::Existing,
) -> Result<(), CliError> {
    let condition = match existing {
        workflow::Existing::Loaded(_) => return Ok(()),
        workflow::Existing::Missing => "is missing",
        workflow::Existing::Unloadable(_) => "does not load",
    };
    let live = recover::unrecorded_live_launches(project_root, phase, Some(own_launch_id))
        .map_err(|error| {
            CliError::Message(format!(
                "phase {phase}: its state file {condition}, and {} could not be listed for live \
                 launches ({error}); nothing was launched",
                project_root.join(".devflow").display()
            ))
        })?;
    match live.first() {
        None => Ok(()),
        Some(launch) => Err(CliError::Message(format!(
            "phase {phase}: its state file {condition}, so it cannot name its launch, and {}. \
             Nothing was launched. Retry once it has published or exited; `devflow stop` never \
             signals an unpublished launch, so if it stays, end that process yourself",
            launch.describe()
        ))),
    }
}

/// What [`reserve_launch`]'s write-lock closure hands back to print and return.
struct ReserveOutcome {
    state: State,
    created: bool,
    /// The state file a reservation replaced because it did not load, and the copy of
    /// its bytes.
    replaced: Option<(PathBuf, PathBuf)>,
    /// The launch id of the abandoned reservation this one replaced.
    abandoned: Option<String>,
}

/// Remove an abandoned reservation's liveness file, best effort: `recover --clean`
/// sweeps one left behind. Only the path this project's launch id derives to, and only
/// while nothing holds it, because the path was read from a state file. An unheld hold
/// file can never read live again, so removing it races no holder.
pub(crate) fn remove_abandoned_liveness_file(project_root: &Path, abandoned: &PendingSupervisor) {
    let path = &abandoned.liveness_path;
    if liveness::launch_liveness_path(project_root, &abandoned.launch_id).as_ref() != Some(path)
        || !matches!(liveness::probe(path), Ok(liveness::Held::Abandoned))
    {
        return;
    }
    if let Err(err) = std::fs::remove_file(path) {
        eprintln!(
            "warning: could not remove the abandoned liveness file {}: {err}",
            path.display()
        );
    }
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
/// phase has a live agent (whatever its supervisor's liveness) or a
/// supervisor that answers ALIVE — closing the race where a real
/// `cleanup --force` run could delete a worktree a live agent/supervisor is
/// still writing into (review: Codex HIGH, fail-closed on a live agent).
/// A STALE supervisor (or a failed probe) keeps the worktree unless
/// `--force` (50-12); GONE and NotLaunched follow the rules above.
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
        // 50-12: the supervisor's liveness comes from its recorded socket,
        // never a pid.
        let phase_liveness = matched_state.map_or(PhaseLiveness::NotLaunched, probe_phase);

        // A phase halted via `devflow start --until <stage>` (20c) has no
        // running supervisor and its agent has already exited by design —
        // which would otherwise sail straight through the live-agent refusal
        // below. Treat it the same way `doctor`'s `check_dead_agent`/
        // `check_supervisor` were taught about `facts.stopped`: an
        // intentionally-parked worktree is never implicitly safe to remove —
        // require `--force`, mirroring the `reference` worktree's own
        // precedent above.
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

        // Fail-closed on a live run: refuse whenever the agent is alive
        // (whatever the supervisor's liveness) OR the supervisor answers
        // ALIVE or is a live pending launch, `--force` or not. The verb that
        // ends a live run is `stop` (50-14); `resume` refuses while one is live.
        if agent_alive
            || matches!(
                phase_liveness,
                PhaseLiveness::Alive { .. }
                    | PhaseLiveness::LaunchPending { .. }
                    | PhaseLiveness::HeldNoSocket { .. }
            )
        {
            let phase_label = phase
                .map(|p| p.to_string())
                .unwrap_or_else(|| "?".to_string());
            return Err(CliError::Message(format!(
                "refusing to remove worktree {} for phase {phase_label} ({}) — run `devflow stop --phase {phase_label}` or wait for it to finish",
                wt.path.display(),
                phase_liveness.describe(),
            )));
        }

        // 50-12: a STALE supervisor may have left its agent group running,
        // and a failed probe proves nothing either way: keep the worktree
        // unless `--force`, and name the verb that ends the orphaned run.
        if !force
            && matches!(
                phase_liveness,
                PhaseLiveness::Stale | PhaseLiveness::ProbeFailed(_)
            )
        {
            let phase_label = phase
                .map(|p| p.to_string())
                .unwrap_or_else(|| "?".to_string());
            println!(
                "keeping worktree {} for phase {phase_label} — supervisor {}; run `devflow stop --phase {phase_label}` first, or pass --force to discard it",
                wt.path.display(),
                phase_liveness.describe(),
            );
            continue;
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

/// A phase's supervisor liveness as `status`, `cleanup` and `doctor` report
/// it (50-12): answered from the socket recorded in `state.supervisor`
/// through [`supervisor::probe`], never from a pid. Three parts, like the
/// 18b presentation it replaces: this enum, [`PhaseLiveness::describe`], and
/// the pure mapping [`phase_liveness`], which `doctor`'s `check_supervisor`
/// reuses so the two can never drift.
///
/// A supervisor runs its own advance in-process, so a moment between stages
/// is ALIVE (its owner reads `advancing`), not a separate state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PhaseLiveness {
    /// The recorded socket answered a ping.
    Alive {
        /// Who owns the run right now, from the ping reply.
        owner: String,
        /// The launch shape (`capture` or `stream`), from the ping reply.
        launch: String,
    },
    /// The socket file remains but nothing listens on it: the supervisor
    /// died without cleaning up, and its agent group may still run.
    Stale,
    /// No socket at the recorded path, and the phase is stopped: a normal
    /// end.
    Gone,
    /// No socket at the recorded path while the phase is not stopped:
    /// nothing will advance it. A finding, not a normal state.
    GoneWhileActive,
    /// The state records no supervisor: nothing has been launched yet, or the
    /// state predates the record. Never a finding.
    NotLaunched,
    /// The probe could not answer; a path-free summary of why (see
    /// [`probe_error_summary`]). Neither ALIVE nor GONE is guessed.
    ProbeFailed(String),
    /// The state names a pending supervisor that is live: its liveness lock is held,
    /// or its socket is not GONE (D-02). It may not be the recorded one yet, so this
    /// comes first, and its one repair is `devflow stop`.
    LaunchPending {
        /// The reserved launch's id.
        launch_id: String,
        /// Whether its socket has appeared.
        published: bool,
    },
    /// The recorded socket is gone, but a process still holds the launch's liveness
    /// lock, so `stop` cannot reach it and `resume` refuses it.
    HeldNoSocket {
        /// The holder's pid, when its record can be read.
        pid: Option<u32>,
    },
}

impl PhaseLiveness {
    /// The operator-facing text. The three states the operator acts on lead
    /// with `ALIVE`, `STALE` and `GONE`.
    pub(crate) fn describe(&self) -> String {
        match self {
            PhaseLiveness::Alive { owner, launch } => {
                format!("ALIVE (owner: {owner}, launch: {launch})")
            }
            PhaseLiveness::Stale => {
                "STALE — the socket remains but no supervisor answers".to_string()
            }
            PhaseLiveness::Gone => {
                "GONE — the supervisor has exited and the phase is stopped".to_string()
            }
            PhaseLiveness::GoneWhileActive => {
                "GONE — no supervisor runs this phase, but it is not stopped".to_string()
            }
            PhaseLiveness::NotLaunched => "not launched (no supervisor recorded)".to_string(),
            PhaseLiveness::ProbeFailed(why) => format!("UNKNOWN — the probe failed: {why}"),
            PhaseLiveness::LaunchPending {
                launch_id,
                published,
            } => {
                let published = if *published {
                    "published"
                } else {
                    "not yet published"
                };
                format!(
                    "PENDING — launch {launch_id} is reserved and its supervisor has {published}; \
                     it is not yet the recorded supervisor"
                )
            }
            PhaseLiveness::HeldNoSocket { pid } => {
                let holder = pid.map_or("a process".to_string(), |pid| format!("pid {pid}"));
                format!("HELD — its socket is gone, but {holder} holds its liveness lock")
            }
        }
    }

    /// A stable machine label for `doctor --json`.
    pub(crate) fn label(&self) -> &'static str {
        match self {
            PhaseLiveness::Alive { .. } => "alive",
            PhaseLiveness::Stale => "stale",
            PhaseLiveness::Gone => "gone",
            PhaseLiveness::GoneWhileActive => "gone_while_active",
            PhaseLiveness::NotLaunched => "not_launched",
            PhaseLiveness::ProbeFailed(_) => "probe_failed",
            PhaseLiveness::LaunchPending { .. } => "launch_pending",
            PhaseLiveness::HeldNoSocket { .. } => "held_no_socket",
        }
    }
}

/// Pure mapping from a phase's supervisor record and its probe result to a
/// [`PhaseLiveness`]. No I/O. No record (or no probe) is `NotLaunched`; GONE
/// splits on `stopped`; a probe error is `ProbeFailed`, never a guess.
fn phase_liveness(
    record: Option<&SupervisorRecord>,
    probe: Option<Result<supervisor::Liveness, supervisor::ProbeError>>,
    stopped: bool,
) -> PhaseLiveness {
    let (Some(_), Some(probe)) = (record, probe) else {
        return PhaseLiveness::NotLaunched;
    };
    match probe {
        Ok(supervisor::Liveness::Alive(reply)) => PhaseLiveness::Alive {
            owner: reply.owner,
            launch: reply.launch,
        },
        Ok(supervisor::Liveness::Stale) => PhaseLiveness::Stale,
        Ok(supervisor::Liveness::Gone) if stopped => PhaseLiveness::Gone,
        Ok(supervisor::Liveness::Gone) => PhaseLiveness::GoneWhileActive,
        Err(error) => PhaseLiveness::ProbeFailed(probe_error_summary(&error)),
    }
}

/// A [`supervisor::ProbeError`] as a path-free kind and cause (T-18-01):
/// `NotASocket`'s own text names the path, and a `NoReply` text may quote
/// whatever the peer sent, so neither is passed through.
fn probe_error_summary(error: &supervisor::ProbeError) -> String {
    match error {
        supervisor::ProbeError::Stat(cause) => {
            format!("could not stat the socket ({})", cause.kind())
        }
        supervisor::ProbeError::NotASocket(_) => "the recorded path is not a socket".to_string(),
        supervisor::ProbeError::ForeignOwner { uid } => {
            format!("the socket is owned by uid {uid}, not this user")
        }
        supervisor::ProbeError::Connect(cause) => {
            format!("could not connect to the socket ({})", cause.kind())
        }
        supervisor::ProbeError::NoReply(_) => "no valid ping reply".to_string(),
        _ => "the probe failed".to_string(),
    }
}

/// A pending record as [`PhaseLiveness::LaunchPending`], or `None` when its
/// reservation is abandoned. The launch guard's own predicate
/// ([`pending_live_socket`]), so `status` and a refusal cannot disagree.
fn pending_liveness(pending: &PendingSupervisor) -> Option<PhaseLiveness> {
    pending_live_socket(pending).map(|live| PhaseLiveness::LaunchPending {
        launch_id: pending.launch_id.clone(),
        published: live.pending == PendingKind::Published,
    })
}

/// A phase's supervisor liveness: a live pending supervisor first (D-02), else the
/// socket `state.supervisor` records, probed once and mapped by [`phase_liveness`].
/// Reads no pid. A reservation whose supervisor is gone left a launch behind, so with
/// no record it reads GONE, not "not launched".
fn probe_phase(state: &State) -> PhaseLiveness {
    let pending = state.pending_supervisor.as_ref();
    if let Some(live) = pending.and_then(pending_liveness) {
        return live;
    }
    let record = state.supervisor.as_ref();
    let probe = record.map(|record| supervisor::probe(&record.socket_path));
    match phase_liveness(record, probe, state.stopped) {
        PhaseLiveness::Gone | PhaseLiveness::GoneWhileActive
            if record.is_some_and(|record| {
                held_without_socket(&state.project_root, record).is_some()
            }) =>
        {
            let path = record.and_then(|record| {
                liveness::launch_liveness_path(&state.project_root, &record.launch_id)
            });
            let holder = path.and_then(|path| liveness::live_holder(&path).ok().flatten());
            PhaseLiveness::HeldNoSocket {
                pid: holder.map(|(pid, _)| pid),
            }
        }
        PhaseLiveness::NotLaunched if pending.is_some() && state.stopped => PhaseLiveness::Gone,
        PhaseLiveness::NotLaunched if pending.is_some() => PhaseLiveness::GoneWhileActive,
        liveness => liveness,
    }
}

/// The repair for [`PhaseLiveness::HeldNoSocket`], whose text names the process.
const HELD_NO_SOCKET_REPAIR: &str =
    "`devflow stop` cannot reach that process and `devflow resume` refuses it; end it first";

/// Recovery verbs discoverable from a phase's liveness (21a, D-03; 50-12) —
/// the pure, testable counterpart to `status`'s hints. STALE names
/// `devflow stop` (it ends the orphaned agent group and removes the socket)
/// and then `devflow resume`; GONE while active names `devflow resume`, plus
/// `devflow ship` when the Ship gate is pending, the one stage whose stored answer
/// `ship_override` applies (D-06). No hint names `devflow advance`: its gate open sets
/// an earlier answer aside, so answering and then advancing loses the answer. A
/// failed probe names the socket to inspect. A live pending supervisor names only
/// `devflow stop` (criterion 2). A stopped phase names `devflow resume`, since nothing
/// else continues it; a phase that never launched and is not stopped names nothing, and
/// neither does ALIVE. A `start` that died before its run was prepared names `start`, as
/// `resume` would refuse it ([`run_never_prepared`]). A stored Ship answer names `ship`
/// instead of the `resume` that would set it aside ([`ship_answer_stored`]). A run whose
/// recorded delivery canary refuses its agent names `resume --agent <other>`, since that
/// canary refuses a plain `resume` and only a handoff clears it
/// ([`recorded_canary_refusal`](crate::pipeline_launch::recorded_canary_refusal)).
fn recovery_hints(state: &State, liveness: &PhaseLiveness) -> Vec<String> {
    let phase = state.phase;
    if run_never_prepared(state)
        && matches!(
            liveness,
            PhaseLiveness::GoneWhileActive | PhaseLiveness::Gone | PhaseLiveness::NotLaunched
        )
    {
        return vec![restart_verb(state)];
    }
    let stored = ship_answer_stored(state);
    let continue_verb = if stored {
        format!("devflow ship --phase {phase}")
    } else if crate::pipeline_launch::recorded_canary_refusal(state).is_some() {
        format!("devflow resume --phase {phase} --agent <other>")
    } else {
        format!("devflow resume --phase {phase}")
    };
    match liveness {
        PhaseLiveness::LaunchPending { .. } => vec![format!("devflow stop --phase {phase}")],
        PhaseLiveness::HeldNoSocket { .. } => vec![HELD_NO_SOCKET_REPAIR.to_string()],
        PhaseLiveness::Stale => vec![format!("devflow stop --phase {phase}"), continue_verb],
        PhaseLiveness::GoneWhileActive => {
            let mut hints = vec![continue_verb];
            if !stored && state.gate_pending && state.stage == Stage::Ship {
                hints.push(format!("devflow ship --phase {phase}"));
            }
            hints
        }
        PhaseLiveness::ProbeFailed(why) => {
            let socket = state
                .supervisor
                .as_ref()
                .map(|record| record.socket_path.display().to_string())
                .unwrap_or_default();
            vec![format!(
                "the probe failed ({why}); inspect the socket at {socket}"
            )]
        }
        PhaseLiveness::Gone | PhaseLiveness::NotLaunched if state.stopped => {
            vec![continue_verb]
        }
        PhaseLiveness::Alive { .. } | PhaseLiveness::Gone | PhaseLiveness::NotLaunched => {
            Vec::new()
        }
    }
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

/// `status`'s lines about a phase's supervisor. 50-12: liveness comes from the
/// recorded socket, never a pid, and a live pending supervisor is named by its own
/// socket. `status` speaks only about the operator's own project, so it names the
/// socket path (doctor does not, T-18-01). An abandoned reservation gets a note.
fn status_supervisor_lines(
    project_root: &Path,
    state: &State,
    liveness: &PhaseLiveness,
) -> Vec<String> {
    let pending = state.pending_supervisor.as_ref();
    let live_pending = matches!(liveness, PhaseLiveness::LaunchPending { .. });
    let socket = pending
        .filter(|_| live_pending)
        .map(|pending| &pending.socket_path)
        .or(state.supervisor.as_ref().map(|record| &record.socket_path))
        .map_or_else(|| "none".to_string(), |path| path.display().to_string());
    let mut lines = vec![format!("  supervisor: {socket} — {}", liveness.describe())];
    if let Some(pending) = pending.filter(|_| !live_pending) {
        lines.push(format!(
            "  note: launch reservation {} is abandoned (no process holds it); the next launch \
             clears it",
            pending.launch_id
        ));
    }
    match workflow::write_lock_status(project_root, state.phase) {
        Ok(status) => lines.extend(write_lock_line(&status)),
        Err(error) => lines.push(format!("  state write lock: could not be read ({error})")),
    }
    lines
}

/// `status`'s repair lines for a phase. The live agent is looked up only when no
/// supervisor of the phase is live (D-19).
fn status_hint_lines(project_root: &Path, state: &State, liveness: &PhaseLiveness) -> Vec<String> {
    let live_agent = no_supervisor_live(liveness)
        .then(|| live_phase_agent(project_root, state))
        .flatten();
    status_hints(state, liveness, live_agent.as_ref())
        .into_iter()
        .map(|hint| format!("    → {hint}"))
        .collect()
}

/// Whether `liveness` says no supervisor of the phase is live: the states D-19 names.
fn no_supervisor_live(liveness: &PhaseLiveness) -> bool {
    matches!(
        liveness,
        PhaseLiveness::Gone | PhaseLiveness::GoneWhileActive | PhaseLiveness::NotLaunched
    )
}

/// A phase's agent still running with no live supervisor (D-19).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveAgent {
    /// The agent's pid, which leads its own process group.
    pub(crate) pid: u32,
    /// Whether the recorded start time confirms it is this phase's agent. An
    /// unconfirmed pid comes only from the agent pid file and may have been reused.
    pub(crate) confirmed: bool,
}

/// The phase's agent, if it runs: the recorded supervisor's agent group in this boot,
/// confirmed by its start time; else a live pid in the agent pid file, the launch guard's
/// own predicate, which a reused pid can satisfy. Reads no socket, so callers ask only
/// when no supervisor is live. Pid 0 or 1 is never returned, because the caller prints it
/// as a process group to signal.
pub(crate) fn live_phase_agent(project_root: &Path, state: &State) -> Option<LiveAgent> {
    let is_group = |pid: &u32| *pid > 1;
    if let Some(record) = &state.supervisor
        && is_group(&record.agent_pgid)
        && supervisor::boot_id().as_deref() == Some(record.boot_id.as_str())
        && agent::agent_running(record.agent_pgid)
        && agent::is_same_process(record.agent_pgid, record.agent_start_time)
    {
        return Some(LiveAgent {
            pid: record.agent_pgid,
            confirmed: true,
        });
    }
    agent_pid_from_file(project_root, state.phase)
        .filter(is_group)
        .filter(|&pid| agent::agent_running(pid))
        .map(|pid| LiveAgent {
            pid,
            confirmed: false,
        })
}

/// What to do about a live agent with no live supervisor (D-19). Only the operator
/// signals it; an unconfirmed pid comes with the check to run first.
pub(crate) fn orphaned_agent_line(phase: PhaseId, agent: &LiveAgent) -> String {
    let pid = agent.pid;
    if agent.confirmed {
        format!(
            "phase {phase}'s agent (pid {pid}) is still running with no live supervisor (its \
             supervisor was killed or its socket is gone); `devflow stop` cannot end it (backlog \
             999.170) — end its process group by hand with `kill -TERM -- -{pid}`, then run \
             `devflow status`"
        )
    } else {
        format!(
            "pid {pid} from .devflow/phase-{padded}-agent-pid is alive and no supervisor of phase \
             {phase} is live; it may be the phase's orphaned agent or an unrelated process that \
             reused the pid — check it with `ps -o pid,pgid,lstart,args -p {pid}`, and if it is \
             the agent end its process group with `kill -TERM -- -{pid}`; `devflow stop` cannot \
             end it (backlog 999.170), and `devflow start`/`resume` refuse while it lives",
            padded = phase.padded()
        )
    }
}

/// The repairs `status` prints: the manual kill while an agent outlives its supervisor
/// (D-19), since the launch guard refuses the `resume` it would otherwise name; else
/// [`recovery_hints`].
fn status_hints(
    state: &State,
    liveness: &PhaseLiveness,
    live_agent: Option<&LiveAgent>,
) -> Vec<String> {
    match live_agent {
        Some(agent) if no_supervisor_live(liveness) => {
            vec![orphaned_agent_line(state.phase, agent)]
        }
        _ => recovery_hints(state, liveness),
    }
}

/// How to judge one reading of the state write lock, which `status` and `doctor` take
/// once and do not time. The bounds are the holds D-08 allows (R2-12).
const WRITE_LOCK_HOLD_NOTE: &str = "a state write usually takes milliseconds, but a stalled gate \
     lock or a socket dying mid-ping can stretch it to about 10 s in a gate answer, 13 s while \
     `recover` decides a phase and 16 s in a reservation; a hold past 20 s is the fault";

/// `status`'s line for a held state write lock (R-03); `None` when it is free.
pub(crate) fn write_lock_line(status: &workflow::WriteLockStatus) -> Option<String> {
    match status {
        workflow::WriteLockStatus::Free => None,
        workflow::WriteLockStatus::Held { pid: Some(pid) } => Some(format!(
            "  state write lock: held by pid {pid} — {WRITE_LOCK_HOLD_NOTE}: that process is \
             stuck (`ps -p {pid}`)"
        )),
        workflow::WriteLockStatus::Held { pid: None } => Some(format!(
            "  state write lock: held by an unknown holder — {WRITE_LOCK_HOLD_NOTE}: that process \
             is stuck"
        )),
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
            let phase_liveness = probe_phase(state);
            for line in status_supervisor_lines(project_root, state, &phase_liveness) {
                println!("{line}");
            }
            let phase_summary = last_events.remove(&state.phase);
            println!(
                "{}",
                render_stage_progress_line(
                    state.stage,
                    phase_summary.as_ref().and_then(|s| s.stage_launched_ts)
                )
            );
            for line in status_hint_lines(project_root, state, &phase_liveness) {
                println!("{line}");
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
    if let Some(block) = render_set_aside_answers(&Gates::quarantined(project_root)) {
        println!("\n{block}");
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
    println!("{}", render_gate_list(&open));
    println!(
        "\nanswer with: devflow gate approve <phase> [--gate-id <id>] [--note ...] | \
         devflow gate reject <phase> [--gate-id <id>] --note ... (note with \"abort\" ends the phase)"
    );
    Ok(())
}

/// Pure render of [`gate_list`]'s table, so what it shows is testable without
/// capturing process stdout.
fn render_gate_list(open: &[OpenGate]) -> String {
    let mut table = format!(
        "{:<6} {:<9} {:<9} {:<16} CONTEXT",
        "PHASE", "STAGE", "AGE", "GATE ID"
    );
    for gate in open {
        let context = render_gate_context(&gate.context, 100);
        table.push_str(&format!(
            "\n{:<6} {:<9} {:<9} {:<16} {context}",
            gate.phase,
            gate.stage.to_string(),
            recover::format_age(&gate.timestamp),
            printable_gate_id(gate),
        ));
    }
    table
}

/// A gate's id for display. The gate file is untrusted text, so only a well-formed
/// id reaches the terminal; `-` stands for a missing or malformed one.
fn printable_gate_id(gate: &OpenGate) -> &str {
    if is_gate_id(&gate.gate_id) {
        &gate.gate_id
    } else {
        "-"
    }
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
    println!(
        "{:<6} {:<9} {:<9} {:<16} ROOT / CONTEXT",
        "PHASE", "STAGE", "AGE", "GATE ID"
    );
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
        "{:<6} {:<9} {:<9} {:<16} {}\n           {context}",
        gate.phase,
        gate.stage.to_string(),
        render_gate_age(&gate.timestamp, now),
        printable_gate_id(gate),
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

/// Answer the gate the operator observed: the id they passed, or the id of the
/// stage's open gate in this command's one [`Gates::list_open`] read. The id is
/// fixed here and only re-checked afterwards, so a decision about one gate cannot
/// land on the next (D-04).
pub(crate) fn gate_respond(
    project_root: &Path,
    phase: PhaseId,
    stage: Option<Stage>,
    approved: bool,
    note: Option<String>,
    observed_gate_id: Option<String>,
) -> Result<(), CliError> {
    // A malformed id is refused before anything is read, so it can never name a
    // path (T-50.1-42).
    let given_gate_id = observed_gate_id.as_deref().map(GateId::parse).transpose()?;
    let open = Gates::list_open(project_root);
    let stage = match stage {
        Some(stage) => stage,
        None => resolve_single_open_gate_stage(&open, phase)?,
    };
    let observed = match given_gate_id {
        Some(gate_id) => gate_id,
        None => open_gate_id_at_stage(&open, phase, stage)?,
    };
    require_gate_file_names(project_root, phase, stage, &observed)?;
    refuse_a_gate_with_no_waiter(project_root, phase, stage, &observed)?;
    let responded_by = std::env::var("USER")
        .ok()
        .filter(|user| !user.is_empty())
        .unwrap_or_else(|| "devflow-cli".into());
    let response = GateResponse {
        approved,
        note,
        responded_by: Some(responded_by),
    };
    let path = Gates::answer(project_root, phase, stage, &observed, &response)
        .map_err(gate_answer_error)?;
    events::emit(
        project_root,
        phase,
        "gate_response_written",
        serde_json::json!({
            "stage": stage.to_string(),
            "gate_id": observed.as_str(),
            "approved": approved,
            "via": "cli",
        }),
    );
    let pickup = if waiter_is_live(project_root, phase, stage, &observed) {
        response_pickup_message(stage, WaiterStatus::Live).to_owned()
    } else {
        format!(
            "response was written, but no registered waiter will act. {}",
            no_waiter_repair(phase, stage)
        )
    };
    println!(
        "{} gate for phase {phase} {stage} — response was written; {pickup} (response at {})",
        if approved { "approved" } else { "rejected" },
        path.display()
    );
    Ok(())
}

/// The id of the open gate at `phase` + `stage`, from an already-fetched listing.
fn open_gate_id_at_stage(
    open: &[OpenGate],
    phase: PhaseId,
    stage: Stage,
) -> Result<GateId, CliError> {
    let gate = open
        .iter()
        .find(|gate| gate.phase == phase && gate.stage == stage)
        .ok_or(GateError::NoOpenGate { phase, stage })?;
    GateId::parse(&gate.gate_id).map_err(|_| {
        CliError::Message(format!(
            "phase {phase} {stage}'s gate file has no valid gate id, so it cannot be answered; \
             no response was written. {}",
            no_waiter_repair(phase, stage)
        ))
    })
}

/// The id the stage's gate file names, or `None` with no gate file.
pub(crate) fn gate_file_id(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
) -> Result<Option<String>, GateError> {
    match std::fs::read_to_string(Gates::gate_path(project_root, phase, stage)) {
        Ok(contents) => Ok(Some(serde_json::from_str::<GateFile>(&contents)?.gate_id)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Refuse unless the gate file still names `observed`. This runs before any
/// liveness read: a stale id's registration is normally gone once its gate closed
/// (its waiter finishes it, and `close` removes only abandoned ones), so reading
/// liveness first would report a changed gate as a missing waiter, or, while the
/// old waiter still lives, as a live one.
fn require_gate_file_names(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
    observed: &GateId,
) -> Result<(), CliError> {
    match gate_file_id(project_root, phase, stage)? {
        None => Err(GateError::NoOpenGate { phase, stage }.into()),
        Some(current) if current == observed.as_str() => Ok(()),
        Some(current) => Err(CliError::Message(gate_changed_message(
            phase,
            stage,
            observed.as_str(),
            &current,
        ))),
    }
}

fn gate_changed_message(phase: PhaseId, stage: Stage, observed: &str, current: &str) -> String {
    format!(
        "phase {phase} {stage} is at gate {current}, not gate {observed}; nothing was written — \
         run `devflow gate list` and answer again"
    )
}

/// What a writer that listed a gate may do with that listing.
enum ListedGate {
    /// The gate file still names this id: it may answer it.
    Current(GateId),
    /// The listing's id is empty or malformed (a gate file with no id at all does
    /// not parse, so it is never listed): no answer can bind to it.
    NoValidId,
    /// The gate file no longer names the listed id, or cannot be read.
    Changed,
}

/// Compare a listed gate with the gate file, before any liveness read (see
/// [`require_gate_file_names`] for why).
fn classify_listed_gate(project_root: &Path, gate: &OpenGate) -> ListedGate {
    let Ok(listed) = GateId::parse(&gate.gate_id) else {
        return ListedGate::NoValidId;
    };
    match gate_file_id(project_root, gate.phase, gate.stage) {
        Ok(Some(current)) if current == listed.as_str() => ListedGate::Current(listed),
        Ok(_) => ListedGate::Changed,
        Err(error) => {
            eprintln!(
                "warning: could not read phase {} {}'s gate file: {error}",
                gate.phase, gate.stage
            );
            ListedGate::Changed
        }
    }
}

/// The liveness half of [`gate_respond`] (D-04). With no registered waiter, a Ship
/// answer is still stored for `devflow ship`; any other stage has nothing to act on it.
fn refuse_a_gate_with_no_waiter(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
    observed: &GateId,
) -> Result<(), CliError> {
    if stage == Stage::Ship || waiter_is_live(project_root, phase, stage, observed) {
        return Ok(());
    }
    Err(CliError::Message(format!(
        "no registered waiter for phase {phase} {stage}; no response was written. {}",
        no_waiter_repair(phase, stage)
    )))
}

/// What the operator is told when [`Gates::answer`] refuses or sets aside.
fn gate_answer_error(error: GateError) -> CliError {
    match error {
        GateError::GateIdMismatch {
            phase,
            stage,
            observed,
            current,
        } => CliError::Message(gate_changed_message(phase, stage, &observed, &current)),
        GateError::GateClosedAfterPublish {
            phase,
            stage,
            observed,
        } => CliError::Message(format!(
            "the answer for gate {observed} at phase {phase} {stage} was published after that \
             gate closed: it has been set aside; nothing will act on it, unless that gate's \
             waiter consumed it before it closed — run `devflow gate list`"
        )),
        other => other.into(),
    }
}

/// Whether `observed`'s gate has a live waiter: its registration reads live, and
/// nothing else counts (D-04).
///
/// A writer may read the registration the instant it sees a gate file, because
/// every waiter registers before it publishes ([`Gates::open_registered`], which
/// every gate wait uses from 50.1-14).
fn waiter_is_live(project_root: &Path, phase: PhaseId, stage: Stage, observed: &GateId) -> bool {
    match Gates::waiter_status(project_root, phase, stage, observed) {
        Ok(status) => status == WaiterStatus::Live,
        Err(error) => {
            eprintln!(
                "warning: could not probe the waiter registration of phase {phase} {stage} \
                 gate {observed}: {error}"
            );
            false
        }
    }
}

/// Operator recovery for a gate with no confirmed consumer. Ship is special:
/// its answer remains the input to the explicit `devflow ship` workflow.
/// [`no_waiter_repair`] for `stop`'s own lines. `stop` goes on to the supervisor's socket,
/// and while a supervisor of the phase is live a `resume` would refuse (criterion 2).
fn stop_no_waiter_repair(project_root: &Path, phase: PhaseId, stage: Stage) -> String {
    let supervisor_live = workflow::load_state(project_root, phase).is_ok_and(|state| {
        matches!(
            probe_phase(&state),
            PhaseLiveness::Alive { .. } | PhaseLiveness::LaunchPending { .. }
        )
    });
    if supervisor_live && stage != Stage::Ship {
        return format!("Stopping phase {phase}'s live supervisor through its socket instead.");
    }
    no_waiter_repair(phase, stage)
}

fn no_waiter_repair(phase: PhaseId, stage: Stage) -> String {
    match stage {
        Stage::Ship => {
            format!("run `devflow ship --phase {phase}` when you are ready to complete Ship")
        }
        _ => format!(
            "run `devflow resume --phase {phase}` or `devflow recover --clean --phase {phase}` to establish recovery"
        ),
    }
}

/// Conservative post-publication wording. The second status observation only
/// narrows the message race; it cannot establish a waiter's lifetime.
fn response_pickup_message(stage: Stage, status: WaiterStatus) -> &'static str {
    match status {
        WaiterStatus::Live => {
            "a registered waiter may pick up this response; this is not a lifetime guarantee"
        }
        WaiterStatus::Absent => {
            if stage == Stage::Ship {
                "no confirmed waiter remains; use `devflow ship --phase` for Ship recovery"
            } else {
                "no confirmed waiter remains; the response may be stale, so use the named recovery"
            }
        }
    }
}

/// Answer or report every aged, unattended gate across every registered root
/// (or a single `root`, when given) — the acting half of 23b's bound gate
/// lifetime. Never signals any process; `Gates::reap_answer` (structurally
/// incapable of approving, T-23-41) is the ONLY write path, so the sweep's
/// sole effect on the world is written bytes a live poller was already
/// watching for.
///
/// `GateError::NoOpenGate`/`AlreadyResponded` are treated as benign races —
/// a human or `--yes-ship` may have answered the same gate between
/// `list_open` and `reap` — and counted as skipped rather than returned as
/// an error; `Gates::reap_answer`'s own refusal to clobber an unconsumed
/// answer is the first-writer-wins resolution that makes this safe with
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
            match sweep_one_gate(project_root, &gate, age, threshold, dry_run)? {
                SweepStep::Reaped => reaped += 1,
                SweepStep::Skipped => skipped += 1,
                SweepStep::LeftAlone => left_alone += 1,
            }
        }
    }

    // 999.44/DEN-68: opt-in only. Registry-independent, so this pass is NOT
    // scoped to `roots` above — a stray by definition has no project root
    // for any registry entry, held file, or state file to name, which is
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
                 for any registry entry, held liveness, gate-registration or foreground-holder \
                 file, or state file to name, so discovery and \
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

fn gate_sweep_may_reap(_stage: Stage, waiter_live: bool) -> bool {
    waiter_live
}

/// What the sweep did with one aged gate it listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SweepStep {
    /// Answered with an abort, or, under `--dry-run`, would have been.
    Reaped,
    /// Not answered: answered already, changed since it was listed, or unanswerable.
    Skipped,
    /// Not answered: nothing is waiting on it.
    LeftAlone,
}

/// The sweep's per-gate step, for the gate as it was listed: compare its id with
/// the gate file, read its waiter, then answer that id and no other.
///
/// A held gate lock ends the sweep rather than being reported as a skip, and so does an
/// answer published while the lock stayed held: later gates are never reported reaped
/// past one the sweep could not settle.
fn sweep_one_gate(
    project_root: &Path,
    gate: &OpenGate,
    age: u64,
    threshold: u64,
    dry_run: bool,
) -> Result<SweepStep, CliError> {
    let at = project_root.display();
    let (phase, stage) = (gate.phase, gate.stage);
    let observed = match classify_listed_gate(project_root, gate) {
        ListedGate::Current(observed) => observed,
        ListedGate::NoValidId => {
            println!(
                "skipped phase {phase} {stage} at {at} — its gate file has no valid gate id; not reaped"
            );
            return Ok(SweepStep::Skipped);
        }
        ListedGate::Changed => {
            println!(
                "skipped phase {phase} {stage} at {at} — gate changed since it was listed; not reaped"
            );
            return Ok(SweepStep::Skipped);
        }
    };
    let waiter_live = waiter_is_live(project_root, phase, stage, &observed);
    if !gate_sweep_may_reap(stage, waiter_live) {
        println!(
            "left phase {phase} {stage} alone at {at} — no confirmed waiter; {}",
            no_waiter_repair(phase, stage)
        );
        return Ok(SweepStep::LeftAlone);
    }
    if dry_run {
        println!("would reap phase {phase} {stage} (age {age}s) at {at}");
        return Ok(SweepStep::Reaped);
    }
    match Gates::reap_answer(
        project_root,
        phase,
        stage,
        &observed,
        "abort: reaped by devflow gate sweep (unattended gate exceeded max age)",
        "devflow-reap",
    ) {
        Ok(_) => {
            events::emit(
                project_root,
                phase,
                "gate_reaped",
                serde_json::json!({
                    "stage": stage.to_string(),
                    "age_secs": age,
                    "max_age_secs": threshold,
                }),
            );
            println!("reaped phase {phase} {stage} (age {age}s) at {at}");
            Ok(SweepStep::Reaped)
        }
        Err(err @ GateError::GateIdMismatch { .. }) => {
            println!(
                "skipped phase {phase} {stage} at {at} — gate changed since it was listed; not reaped ({err})"
            );
            Ok(SweepStep::Skipped)
        }
        Err(err @ (GateError::AlreadyResponded { .. } | GateError::NoOpenGate { .. })) => {
            println!("skipped phase {phase} {stage} at {at} — already answered ({err})");
            Ok(SweepStep::Skipped)
        }
        Err(err @ (GateError::GateLockTimeout { .. } | GateError::PublishedUnconfirmed { .. })) => {
            Err(err.into())
        }
        Err(err) => {
            println!("skipped phase {phase} {stage} at {at} — {err}");
            Ok(SweepStep::Skipped)
        }
    }
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
            // TOCTOU") — copies `stop_via_holder`'s fail-closed posture:
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

/// The gate [`stop_via_gate`] answered, and the launch the state named as that run's
/// supervisor when it read the gate as current. `stop` keeps the launch id in its list of
/// launches it addressed. The flag says the abort was published but its gate may have
/// closed first, so no waiter may ever act on it.
type AnsweredGate = (Stage, GateId, Option<String>, bool);

/// End a running phase cleanly (23c, D-04, D-06): the one teardown verb, at
/// every point of a run, that never claims a stop it did not perform.
///
/// 1. An open gate with a confirmed live waiter is answered with an abort
///    ([`stop_via_gate`]); the waiter unwinds through its own `abort()`.
/// 2. Otherwise the run's supervisor socket is addressed
///    ([`stop_via_socket`]): the state's `supervisor` record names it, or,
///    when the state records none, is missing or does not load, the one live
///    sidecar-found socket for this root and phase. Its probe picks the arm:
///    ALIVE gets a `shutdown` (999.138).
/// 3. Only when no live socket remains (GONE, or nothing recorded) does the
///    identity-confirmed foreground holder of `.devflow/.holder-NN` get signalled
///    ([`stop_via_holder`]). It catches the signal and ends at its next safe point
///    (D-18), so `stop` waits for it, boundedly, and marks nothing while it runs. With no
///    holder either, a phase agent that still runs is named with its manual kill, and
///    nothing is signalled or marked (D-19).
///
/// A pending supervisor comes after the recorded one, whose own safe point 3 ends an
/// unpublished successor: only its socket reaches it ([`stop_pending_launch`]), an
/// abandoned reservation is cleared with a report, and a launcher still busy before its
/// spawn is reached through the holder file ([`stop_pending_launcher`]).
///
/// No pid read from state is ever signalled. `stopped` is recorded only for
/// a stop performed or confirmed not live, and never into a state file that
/// does not load.
pub(crate) fn stop(project_root: &Path, phase: PhaseId) -> Result<(), CliError> {
    stop_in(project_root, phase, registry::cache_dir().as_deref())
}

/// [`stop`] with the cache dir whose `sock/` holds the sidecars it may
/// search, so tests never sweep the operator's real cache.
fn stop_in(project_root: &Path, phase: PhaseId, cache_dir: Option<&Path>) -> Result<(), CliError> {
    // The launches this stop ended, answered, cleared or found not live, and only those:
    // no state read seeds it, so a launch none of the arms addressed blocks the mark.
    let mut known_launches = Vec::new();
    if let Some((_, _, run_launch, unconfirmed)) = stop_via_gate(project_root, phase)? {
        if unconfirmed {
            // Only the socket: signalling a foreground holder here would be a second
            // action on top of the published abort (R2-04). The launch counts as addressed
            // only if this arm addresses it.
            note_socket_stop(
                stop_via_socket(project_root, phase, cache_dir)?,
                &mut known_launches,
            );
        } else {
            known_launches.extend(run_launch);
        }
        return persist_stopped_state(project_root, phase, false, &known_launches);
    }
    let socket_stop = stop_via_socket(project_root, phase, cache_dir)?;
    if let SocketStop::EndedStateUnloadable(state_file) = &socket_stop {
        println!(
            "stop: phase {phase}'s state file {} does not load, so `stopped` was not \
             recorded — repair or remove it before `devflow resume --phase {phase}`",
            state_file.display()
        );
        return Ok(());
    }
    let mut ended = note_socket_stop(socket_stop, &mut known_launches);
    let mut launcher_signalled = false;
    if let Some(pending) = stop_pending_launch(project_root, phase, &mut known_launches)? {
        ended |= pending == PendingStop::SupervisorEnded;
        launcher_signalled = pending == PendingStop::LauncherSignalled;
        // A launch promoted while `stop` waited on it is the recorded supervisor now.
        ended |= note_socket_stop(
            stop_via_socket(project_root, phase, cache_dir)?,
            &mut known_launches,
        );
    }
    if launcher_signalled {
        return persist_stopped_state(project_root, phase, true, &known_launches);
    }
    if ended {
        return persist_stopped_state(project_root, phase, false, &known_launches);
    }
    let signal_sent = stop_via_holder(project_root, phase)?;
    persist_stopped_state(project_root, phase, signal_sent, &known_launches)
}

/// Add what one socket stop addressed to `known_launches`; whether it ended a run.
fn note_socket_stop(stop: SocketStop, known_launches: &mut Vec<String>) -> bool {
    match stop {
        SocketStop::NoLiveSocket { gone } => {
            known_launches.extend(gone);
            false
        }
        SocketStop::Ended { launch_id } => {
            known_launches.extend(launch_id);
            true
        }
        SocketStop::EndedStateUnloadable(_) => true,
    }
}

/// What the pending half of [`stop`] did about a launch the state names.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PendingStop {
    /// No supervisor was ended and nothing was signalled, such as an abandoned reservation
    /// that was cleared.
    NothingEnded,
    /// A pending supervisor was ended through its socket.
    SupervisorEnded,
    /// The launcher holding the reservation was signalled through the holder file, and it
    /// has exited.
    LauncherSignalled,
}

/// How long `stop` waits for a live pending supervisor to publish its socket: the
/// supervisor's own publish wait (10 s, `PUBLISH_WAIT`, private to `devflow_core::monitor`)
/// and a margin, so it outlasts a launch that is going to publish.
const PENDING_PUBLISH_WAIT: std::time::Duration = std::time::Duration::from_secs(15);

/// How often `stop` looks again during [`PENDING_PUBLISH_WAIT`].
const PENDING_PUBLISH_POLL: std::time::Duration = std::time::Duration::from_millis(100);

/// The pending half of [`stop`] (D-02), on a fresh read each pass: a socket that answers
/// gets a `shutdown`; a held liveness lock with no answering socket is waited for
/// (`Live`, never signalled, Phase 50 D-05/D-06); a STALE socket no lock vouches for goes
/// to the backstop; with neither left, the reservation is abandoned and cleared. A live
/// lock beats a STALE socket because the backstop is for a supervisor that is gone. When
/// the wait expires, the launcher may simply be busy before its spawn, so it is reached
/// through the holder file ([`stop_pending_launcher`]).
///
/// `Ok(None)` when the state names no pending launch, else what was done about it. The
/// caller looks at the recorded socket again, since the record can end by promotion. A
/// launch this ended or cleared joins `known_launches`.
fn stop_pending_launch(
    project_root: &Path,
    phase: PhaseId,
    known_launches: &mut Vec<String>,
) -> Result<Option<PendingStop>, CliError> {
    let mut outcome = None;
    let mut ended = false;
    let mut handed_off = 0;
    let mut deadline = None;
    loop {
        let Some(pending) = workflow::load_state(project_root, phase)
            .ok()
            .and_then(|state| state.pending_supervisor)
        else {
            return Ok(outcome);
        };
        outcome = Some(if ended {
            PendingStop::SupervisorEnded
        } else {
            PendingStop::NothingEnded
        });
        let located = LocatedSocket {
            socket: pending.socket_path.clone(),
            launch_id: Some(pending.launch_id.clone()),
            record: None,
            unloadable_state: None,
            pending_recorded: true,
        };
        let socket = supervisor::probe(&located.socket)
            .map_err(|error| probe_refusal(phase, &located.socket, &error))?;
        let held = liveness::probe(&pending.liveness_path).map_err(|error| {
            CliError::Message(format!(
                "stop: could not read launch {}'s liveness lock ({error}); nothing was signalled \
                 or recorded",
                pending.launch_id
            ))
        })?;
        match (socket, held) {
            (supervisor::Liveness::Alive(_), _) => match shutdown_supervisor(phase, &located)? {
                ShutdownResult::Ended => {
                    ended = true;
                    deadline = None;
                    known_launches.push(pending.launch_id.clone());
                }
                ShutdownResult::AcceptedPending => {
                    known_launches.push(pending.launch_id.clone());
                    return Ok(Some(PendingStop::SupervisorEnded));
                }
                ShutdownResult::Relocate => {}
                ShutdownResult::HandedOff => {
                    handed_off += 1;
                    if handed_off >= MAX_SHUTDOWN_REQUESTS {
                        return Err(CliError::Message(format!(
                            "stop: launch {} answered `handed_off` to {handed_off} shutdown \
                             requests; nothing was recorded — run `devflow status` and `devflow \
                             stop --phase {phase}` again",
                            pending.launch_id
                        )));
                    }
                    std::thread::sleep(PENDING_PUBLISH_POLL);
                }
            },
            (_, liveness::Held::Live) => {
                let until = *deadline
                    .get_or_insert_with(|| std::time::Instant::now() + PENDING_PUBLISH_WAIT);
                if std::time::Instant::now() >= until {
                    return if ended {
                        Ok(Some(PendingStop::SupervisorEnded))
                    } else {
                        stop_pending_launcher(project_root, phase, &pending).map(Some)
                    };
                }
                std::thread::sleep(PENDING_PUBLISH_POLL);
            }
            (supervisor::Liveness::Stale, _) => {
                stop_stale_socket(phase, &located)?;
                ended = true;
                known_launches.push(pending.launch_id.clone());
            }
            (supervisor::Liveness::Gone, _) => {
                if clear_abandoned_reservation(project_root, phase, &pending)? {
                    known_launches.push(pending.launch_id.clone());
                }
                return Ok(Some(if ended {
                    PendingStop::SupervisorEnded
                } else {
                    PendingStop::NothingEnded
                }));
            }
        }
    }
}

/// The expiry of [`stop_pending_launch`]'s wait: no socket came, and the pending liveness
/// lock is still held. If a foreground launcher holds the phase, it is signalled, and it
/// stops at safe point 2, before its spawn, or at safe point 3, shutting an unpublished
/// supervisor down (D-02, D-18). The pending supervisor itself is reached only through
/// its socket: the process signalled here is its launcher. With no holder, nothing is
/// signalled and [`pending_not_published`] stands.
fn stop_pending_launcher(
    project_root: &Path,
    phase: PhaseId,
    pending: &PendingSupervisor,
) -> Result<PendingStop, CliError> {
    let unheld = matches!(
        liveness::probe(&liveness::phase_holder_path(project_root, phase)),
        Ok(liveness::Held::Abandoned | liveness::Held::Missing)
    );
    if unheld || !stop_via_holder(project_root, phase)? {
        return Err(pending_not_published(phase, pending));
    }
    let launch_id = &pending.launch_id;
    if !wait_for_holder_release(project_root, phase) {
        return Err(CliError::Message(format!(
            "{}; it gives its reservation for launch {launch_id} back itself at its next safe \
             point, before its supervisor is spawned",
            holder_still_running(project_root, phase)
        )));
    }
    // The launcher has exited. Its own safe point gave the reservation back, or its exit
    // released a lock that now reads abandoned. A lock still held belongs to a supervisor
    // it spawned, and that is no stop to claim.
    let fresh = workflow::load_state(project_root, phase)
        .ok()
        .and_then(|state| state.pending_supervisor)
        .filter(|fresh| fresh.launch_id == *launch_id);
    if let Some(fresh) = fresh {
        let held = liveness::probe(&fresh.liveness_path);
        if !matches!(
            held,
            Ok(liveness::Held::Abandoned | liveness::Held::Missing)
        ) {
            return Err(CliError::Message(format!(
                "stop: phase {phase}'s signalled foreground holder exited, but launch {launch_id} \
                 still holds its liveness lock and has not published a supervisor; phase state \
                 was not marked stopped — run `devflow stop --phase {phase}` again"
            )));
        }
        clear_abandoned_reservation(project_root, phase, &fresh)?;
    }
    Ok(PendingStop::LauncherSignalled)
}

/// `stop`'s refusal over a live pending lock with no socket by the end of the wait and
/// no foreground launcher to reach. `stop` never signals an unpublished launch, so
/// running it again repeats this while the holder lives: name the holder, from the
/// lock's record, and the manual route instead.
fn pending_not_published(phase: PhaseId, pending: &PendingSupervisor) -> CliError {
    let launch_id = &pending.launch_id;
    let lock = pending.liveness_path.display();
    let holder = match liveness::live_holder(&pending.liveness_path) {
        Ok(Some((pid, _))) => format!(
            "pid {pid} holds its liveness lock {lock}; inspect it with `ps -o pid,lstart,args -p \
             {pid}` and, if it is a stuck `devflow __monitor`, end it yourself, then run \
             `devflow stop --phase {phase}` again to clear the reservation"
        ),
        _ => format!(
            "its liveness lock {lock} is held, by a process its record does not name; end that \
             process yourself, then run `devflow stop --phase {phase}` again to clear the \
             reservation"
        ),
    };
    CliError::Message(format!(
        "stop: launch {launch_id} for phase {phase} is reserved by a live process and has not \
         published a supervisor, and no foreground launcher holds the phase; nothing was \
         signalled or recorded, because `stop` never signals an unpublished launch. {holder}"
    ))
}

/// Clear a reservation nothing holds, if the fresh read still names it, then remove its
/// liveness file and say so. R-03: `stop` is a repair verb, so a held write lock ends it
/// within the bound, naming the holder, with nothing cleared. `true` when it cleared it.
fn clear_abandoned_reservation(
    project_root: &Path,
    phase: PhaseId,
    pending: &PendingSupervisor,
) -> Result<bool, CliError> {
    let cleared = workflow::update_state_with_within(
        project_root,
        phase,
        workflow::WRITE_LOCK_REPAIR_WAIT,
        |existing| {
            let workflow::Existing::Loaded(mut state) = existing else {
                return Ok::<_, CliError>((None, false));
            };
            let names_it = state
                .pending_supervisor
                .as_ref()
                .is_some_and(|fresh| fresh.launch_id == pending.launch_id);
            if !names_it {
                return Ok((None, false));
            }
            state.pending_supervisor = None;
            Ok((Some(state), true))
        },
    )?;
    if cleared {
        remove_abandoned_liveness_file(project_root, pending);
        println!(
            "stop: cleared the abandoned launch reservation {} (no process held it)",
            pending.launch_id
        );
    }
    Ok(cleared)
}

/// `devflow stop --phase N --launch <id>` (D-06): tear down only the
/// supervisor socket with that launch id, the form `devflow doctor` prints
/// for an orphan. The launch id is the address, so a deleted root's orphan
/// is reachable from any directory.
///
/// The socket is the one whose sidecar carries `<id>`, or, when its sidecar
/// is missing or does not parse, whose file name carries it (RV-05b). If this
/// project's state loads and records that same launch, the plain [`stop`]
/// runs. Otherwise only that socket is acted on: no gate is answered, no
/// holder signalled and no state written.
pub(crate) fn stop_launch(
    project_root: &Path,
    phase: PhaseId,
    launch_id: &str,
) -> Result<(), CliError> {
    let cache_dir = registry::cache_dir().ok_or_else(|| {
        CliError::Message(
            "stop: no cache directory (set HOME, XDG_CACHE_HOME or DEVFLOW_CACHE_DIR), so no \
             supervisor socket can be found by launch id"
                .to_string(),
        )
    })?;
    stop_launch_in(project_root, phase, launch_id, &cache_dir)
}

/// [`stop_launch`] against the sockets under `cache_dir`.
fn stop_launch_in(
    project_root: &Path,
    phase: PhaseId,
    launch_id: &str,
    cache_dir: &Path,
) -> Result<(), CliError> {
    let Some(entry) = find_launch(cache_dir, launch_id)? else {
        println!("stop: no supervisor socket with launch id {launch_id}; nothing to stop");
        return Ok(());
    };
    check_launch_belongs(project_root, phase, launch_id, &entry, cache_dir)?;
    let recorded = workflow::load_state(project_root, phase).is_ok_and(|state| {
        state
            .supervisor
            .is_some_and(|record| record.launch_id == launch_id)
    });
    if recorded {
        return stop_in(project_root, phase, Some(cache_dir));
    }
    stop_one_launch(launch_id, &entry)
}

/// The one socket under `cache_dir` whose sidecar carries `launch_id`, or,
/// with no usable sidecar, whose file name does. Two such sockets are a
/// refusal: the id no longer names one launch.
fn find_launch(
    cache_dir: &Path,
    launch_id: &str,
) -> Result<Option<supervisor::SweepEntry>, CliError> {
    let mut matches: Vec<supervisor::SweepEntry> = supervisor::sweep_in(cache_dir)
        .into_iter()
        .filter(|entry| match &entry.sidecar {
            Some(sidecar) => sidecar.launch_id == launch_id,
            None => entry.launch_id_in_name.as_deref() == Some(launch_id),
        })
        .collect();
    if matches.len() > 1 {
        let paths: Vec<String> = matches
            .iter()
            .map(|entry| entry.socket_path.display().to_string())
            .collect();
        return Err(CliError::Message(format!(
            "stop: launch id {launch_id} names {} sockets ({}); nothing was changed",
            matches.len(),
            paths.join(", ")
        )));
    }
    Ok(matches.pop())
}

/// Refuse a launch that is not this project's phase `phase`, changing
/// nothing. A parsed sidecar decides: its phase must be `phase`, and its
/// project root must be this one or no longer exist (a deleted root's
/// orphan). With no usable sidecar, the socket must be one
/// [`supervisor::sweep_for_in`] attributes to this root and phase by its
/// file name.
fn check_launch_belongs(
    project_root: &Path,
    phase: PhaseId,
    launch_id: &str,
    entry: &supervisor::SweepEntry,
    cache_dir: &Path,
) -> Result<(), CliError> {
    let refuse = |why: String| {
        Err(CliError::Message(format!(
            "stop: launch {launch_id} {why}; nothing was changed"
        )))
    };
    match &entry.sidecar {
        Some(sidecar) if sidecar.phase != phase => {
            refuse(format!("belongs to phase {}, not {phase}", sidecar.phase))
        }
        Some(sidecar) if sidecar.project_root != project_root && sidecar.project_root.is_dir() => {
            refuse(format!(
                "belongs to project {}, not {}; run stop from that project",
                sidecar.project_root.display(),
                project_root.display()
            ))
        }
        Some(_) => Ok(()),
        None => {
            let ours = supervisor::sweep_for_in(cache_dir, project_root, phase)
                .iter()
                .any(|candidate| candidate.socket_path == entry.socket_path);
            if ours {
                Ok(())
            } else {
                refuse(format!(
                    "has no usable sidecar, and its socket file name {} names another project \
                     or phase than {} phase {phase}",
                    entry.socket_path.display(),
                    project_root.display()
                ))
            }
        }
    }
}

/// Act on one launch's socket alone: ALIVE → `shutdown` (a `handed_off`
/// answer is reported, never retried against the successor); STALE → the
/// backstop with its parsed sidecar and no state record, or a refusal when
/// it has none; GONE → nothing to stop; a probe error → a refusal.
fn stop_one_launch(launch_id: &str, entry: &supervisor::SweepEntry) -> Result<(), CliError> {
    let subject = format!("launch {launch_id}");
    let socket = &entry.socket_path;
    match supervisor::probe(socket) {
        Ok(supervisor::Liveness::Gone) => {
            println!(
                "stop: {subject}: socket {} is gone; nothing to stop",
                socket.display()
            );
            Ok(())
        }
        Err(error) => Err(CliError::Message(format!(
            "stop: {subject}: could not probe socket {} ({}: {error}); nothing was signalled, \
             removed or recorded",
            socket.display(),
            probe_error_kind(&error)
        ))),
        Ok(supervisor::Liveness::Stale) => stop_stale_launch(&subject, entry),
        Ok(supervisor::Liveness::Alive(_)) => shutdown_one_launch(&subject, entry),
    }
}

/// [`stop_one_launch`]'s ALIVE arm. A request that errors is probed once
/// more, as [`shutdown_supervisor`] does: STALE goes to the backstop, GONE
/// leaves nothing to stop.
fn shutdown_one_launch(subject: &str, entry: &supervisor::SweepEntry) -> Result<(), CliError> {
    let socket = &entry.socket_path;
    let error = match supervisor::request_shutdown(socket, shutdown_final_timeout()) {
        Ok(supervisor::ShutdownOutcome::Stopped {
            exit_code,
            advance_suppressed,
            group_signalled,
        }) => {
            println!(
                "{}",
                stopped_line(subject, exit_code, advance_suppressed, group_signalled)
            );
            return Ok(());
        }
        Ok(supervisor::ShutdownOutcome::AcceptedPending { owner }) => {
            println!("{}", accepted_pending_line(subject, &owner));
            return Ok(());
        }
        Ok(supervisor::ShutdownOutcome::HandedOff) => {
            println!("stop: {subject} already handed off to its successor; nothing stopped");
            return Ok(());
        }
        Ok(other) => {
            return Err(CliError::Message(format!(
                "stop: {subject}: the supervisor did not stop ({other:?}); nothing was recorded"
            )));
        }
        Err(error) => error,
    };
    match supervisor::probe(socket) {
        Ok(supervisor::Liveness::Stale) => stop_stale_launch(subject, entry),
        Ok(supervisor::Liveness::Gone) => {
            println!(
                "stop: {subject}: socket {} went away under the shutdown request ({error}); \
                 nothing left to stop",
                socket.display()
            );
            Ok(())
        }
        _ => Err(CliError::Message(format!(
            "stop: {subject}: the supervisor at {} did not answer the shutdown ({error}); \
             nothing was signalled or recorded",
            socket.display()
        ))),
    }
}

/// [`stop_one_launch`]'s STALE arm: the backstop with the sidecar the sweep
/// parsed and no state record, or, with no usable sidecar, a refusal that
/// signals and removes nothing (RV-05b): there is no pgid, start time or
/// boot id to validate.
fn stop_stale_launch(subject: &str, entry: &supervisor::SweepEntry) -> Result<(), CliError> {
    let Some(sidecar) = &entry.sidecar else {
        let why = entry.sidecar_error.as_ref().map_or_else(
            || "its sidecar is missing".to_string(),
            |error| format!("its sidecar is unusable ({error})"),
        );
        return Err(stale_refusal(subject, &entry.socket_path, &why));
    };
    let socket = supervisor::SocketPath::new(entry.socket_path.clone())
        .map_err(|error| stale_refusal(subject, &entry.socket_path, &error.to_string()))?;
    run_backstop(subject, &socket, sidecar, None, None)
}

/// `stop`'s line for a stop its supervisor confirmed (D-13 item 6). It claims a group ended
/// only when the supervisor signalled one; otherwise the stop was honoured at an advance
/// safe point and nothing was signalled.
pub(crate) fn stopped_line(
    subject: &str,
    exit_code: i32,
    advance_suppressed: bool,
    group_signalled: bool,
) -> String {
    if !group_signalled {
        return format!(
            "stop: stopped {subject} through its supervisor (honoured at an advance safe point; \
             nothing was signalled; exit code {exit_code})"
        );
    }
    let advance = if advance_suppressed {
        "advance suppressed"
    } else {
        "advance not suppressed"
    };
    format!(
        "stop: stopped {subject} through its supervisor (agent group ended, exit code \
         {exit_code}, {advance})"
    )
}

/// `stop`'s line for a stop the supervisor accepted and has not confirmed yet (D-13 item 1).
fn accepted_pending_line(subject: &str, owner: &str) -> String {
    format!(
        "stop: {subject}'s supervisor accepted the stop (owner {owner}) and its final answer did \
         not arrive within {secs}s; it will honour the stop at its next safe point — `devflow \
         status` shows when it has",
        secs = shutdown_final_timeout().as_secs()
    )
}

/// How long `stop` waits for a supervisor's final shutdown line: the
/// supervisor's own terminate-and-verify wait, twice, then its wait for the
/// state write lock before it records its stop and answers
/// ([`crate::pipeline_launch::SUPERVISOR_STOP_WAIT`]), plus time to persist.
/// Shorter, and a busy lock reads a supervisor that did stop as one that did
/// not answer.
fn shutdown_final_timeout() -> std::time::Duration {
    2 * agent::TERMINATE_VERIFY_WAIT
        + crate::pipeline_launch::SUPERVISOR_STOP_WAIT
        + std::time::Duration::from_secs(5)
}

/// Shutdown requests `stop` sends in all when each is answered `handed_off`:
/// the first and 3 retries, each against the record a fresh state reload
/// names (crash row C13).
const MAX_SHUTDOWN_REQUESTS: usize = 4;

/// What [`stop_via_socket`] did.
enum SocketStop {
    /// No live supervisor socket: nothing recorded, or its probe says GONE. `gone` is the
    /// launch the GONE socket names, which this stop found not live; `None` when no socket
    /// was located or it names no launch.
    NoLiveSocket { gone: Option<String> },
    /// The run was ended through its socket; record `stopped`. `None` when the socket
    /// names no launch: nothing joins the known list, so a state that names the launch is
    /// refused and the second `stop` finds it GONE.
    Ended { launch_id: Option<String> },
    /// The run was ended through a sidecar-found socket, but the state file
    /// at this path exists and does not load, so nothing can be recorded.
    EndedStateUnloadable(PathBuf),
}

/// The supervisor socket `stop` addresses for a phase, and its provenance.
struct LocatedSocket {
    socket: PathBuf,
    /// The launch this socket belongs to: the state's record, else the sidecar, else the
    /// socket's file name. `None` when none of them names one.
    launch_id: Option<String>,
    /// The state's record, when the socket came from it (the backstop
    /// cross-checks the sidecar against it).
    record: Option<SupervisorRecord>,
    /// The state file, when it exists but does not load and the socket came
    /// from its sidecar.
    unloadable_state: Option<PathBuf>,
    /// Whether the state names a pending launch. Until that is dealt with, no line
    /// about this socket may call the phase resumable (criterion 2).
    pending_recorded: bool,
}

/// Find the socket for a bare `stop`: the loaded state's `supervisor` record;
/// or, when the state records none, is missing or does not load, the one
/// socket of this root and phase that a sweep finds not GONE. Several such
/// sockets are a refusal listing each, since `stop` cannot tell which is the
/// run.
///
/// A loaded state with no record is swept because a `start` launcher saves
/// the record only after its spawned supervisor has published: a stop in
/// that window must end the supervisor through its socket, not signal the
/// launcher (the foreground holder), whose death would leave the supervisor and
/// its agent running unrecorded.
fn locate_phase_socket(
    project_root: &Path,
    phase: PhaseId,
    cache_dir: Option<&Path>,
) -> Result<Option<LocatedSocket>, CliError> {
    let (unloadable_state, pending_recorded) = match workflow::load_state(project_root, phase) {
        Ok(state) => {
            let pending_recorded = state.pending_supervisor.is_some();
            if let Some(record) = state.supervisor {
                return Ok(Some(LocatedSocket {
                    socket: record.socket_path.clone(),
                    launch_id: Some(record.launch_id.clone()),
                    record: Some(record),
                    unloadable_state: None,
                    pending_recorded,
                }));
            }
            (None, pending_recorded)
        }
        Err(workflow::WorkflowError::MissingState(_)) => (None, false),
        Err(_) => (Some(workflow::state_path(project_root, phase)), false),
    };
    let Some(cache_dir) = cache_dir else {
        return Ok(None);
    };
    let live: Vec<supervisor::SweepEntry> =
        supervisor::sweep_for_in(cache_dir, project_root, phase)
            .into_iter()
            .filter(|entry| {
                entry.socket_present && !matches!(entry.liveness, Ok(supervisor::Liveness::Gone))
            })
            .collect();
    match live.as_slice() {
        [] => Ok(None),
        [entry] => Ok(Some(LocatedSocket {
            socket: entry.socket_path.clone(),
            launch_id: entry
                .sidecar
                .as_ref()
                .map(|sidecar| sidecar.launch_id.clone())
                .or_else(|| entry.launch_id_in_name.clone()),
            record: None,
            unloadable_state,
            pending_recorded,
        })),
        several => Err(several_sockets_refusal(
            phase,
            unloadable_state.as_deref(),
            several,
        )),
    }
}

/// The refusal for several live sockets of one root and phase: each with its
/// launch id, liveness and the `devflow stop --phase N --launch <id>` form
/// that addresses it alone. Nothing is signalled, removed or recorded.
fn several_sockets_refusal(
    phase: PhaseId,
    unloadable_state: Option<&Path>,
    entries: &[supervisor::SweepEntry],
) -> CliError {
    use std::fmt::Write as _;
    let why = unloadable_state.map_or_else(
        || "no state file names one".to_string(),
        |path| format!("its state file {} does not load", path.display()),
    );
    let mut text = format!(
        "stop: phase {phase} has {} live supervisor sockets for this project and {why}, so stop \
         cannot tell which one is the run; nothing was signalled, removed or recorded. Stop each \
         one by its launch id:",
        entries.len()
    );
    for entry in entries {
        let _ = write!(text, "\n  {}", sweep_entry_stop_line(phase, entry));
    }
    CliError::Message(text)
}

/// One line of [`several_sockets_refusal`]. A socket with no usable sidecar
/// is named by the launch id in its file name (RV-05b); one whose name is
/// outside the DevFlow scheme has no `--launch` form, so it is listed by its
/// path and its ping reply's launch id.
fn sweep_entry_stop_line(phase: PhaseId, entry: &supervisor::SweepEntry) -> String {
    let liveness = match &entry.liveness {
        Ok(supervisor::Liveness::Alive(_)) => "ALIVE",
        Ok(supervisor::Liveness::Stale) => "STALE",
        Ok(supervisor::Liveness::Gone) => "GONE",
        Err(_) => "probe failed",
    };
    let sidecar_note = match (&entry.sidecar, &entry.sidecar_error) {
        (Some(_), _) => "",
        (None, Some(_)) => ", sidecar does not parse",
        (None, None) => ", sidecar missing",
    };
    let launch_id = entry
        .sidecar
        .as_ref()
        .map(|sidecar| sidecar.launch_id.clone())
        .or_else(|| entry.launch_id_in_name.clone());
    match launch_id {
        Some(id) => format!(
            "launch {id} ({liveness}{sidecar_note}): devflow stop --phase {phase} --launch {id}"
        ),
        None => {
            let pinged = match &entry.liveness {
                Ok(supervisor::Liveness::Alive(reply)) => reply.launch_id.as_str(),
                _ => "unknown",
            };
            format!(
                "socket {} ({liveness}{sidecar_note}, ping names launch {pinged}): its name is \
                 outside the DevFlow socket scheme, so `--launch` cannot address it",
                entry.socket_path.display()
            )
        }
    }
}

/// The socket arm of [`stop`]. Probes the located socket and acts on its
/// state: ALIVE → `shutdown`; STALE → [`stop_stale_socket`]; GONE → nothing
/// here (the caller falls back to the foreground holder); a probe error → a
/// refusal that reaches no teardown (R-6).
///
/// A `handed_off` reply means the run moved to its successor: the state is
/// reloaded and the record it names is asked, up to
/// [`MAX_SHUTDOWN_REQUESTS`] requests in all; then `stop` fails, recording
/// nothing (crash row C13).
fn stop_via_socket(
    project_root: &Path,
    phase: PhaseId,
    cache_dir: Option<&Path>,
) -> Result<SocketStop, CliError> {
    let mut handed_off = 0;
    for _ in 0..MAX_SHUTDOWN_REQUESTS {
        let Some(located) = locate_phase_socket(project_root, phase, cache_dir)? else {
            return Ok(SocketStop::NoLiveSocket { gone: None });
        };
        let ended = match supervisor::probe(&located.socket) {
            Ok(supervisor::Liveness::Gone) => {
                refuse_stop_beside_a_held_lock(project_root, phase, &located)?;
                return Ok(SocketStop::NoLiveSocket {
                    gone: located.launch_id,
                });
            }
            Err(error) => return Err(probe_refusal(phase, &located.socket, &error)),
            Ok(supervisor::Liveness::Stale) => {
                stop_stale_socket(phase, &located)?;
                true
            }
            Ok(supervisor::Liveness::Alive(_)) => match shutdown_supervisor(phase, &located)? {
                ShutdownResult::Ended | ShutdownResult::AcceptedPending => true,
                ShutdownResult::Relocate => false,
                ShutdownResult::HandedOff => {
                    handed_off += 1;
                    false
                }
            },
        };
        if ended {
            return Ok(match located.unloadable_state {
                Some(state_file) => SocketStop::EndedStateUnloadable(state_file),
                None => SocketStop::Ended {
                    launch_id: located.launch_id,
                },
            });
        }
    }
    Err(CliError::Message(format!(
        "stop: phase {phase}'s supervisor answered `handed_off` to {handed_off} of \
         {MAX_SHUTDOWN_REQUESTS} shutdown requests (the first and 3 retries, each against the \
         record a fresh state reload named); nothing was stopped or recorded — run `devflow \
         status` and `devflow stop --phase {phase}` again"
    )))
}

/// How long `stop` and the launch guards let a GONE socket's supervisor release its liveness
/// lock: it unlinks its socket before it exits, and a shutdown just sent can land in that gap.
const STOP_LOCK_RELEASE_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// Refuse to count a launch as gone while it still holds its liveness lock. `stop` has no
/// socket to end it through, and marking the run `stopped` would claim it had.
fn refuse_stop_beside_a_held_lock(
    project_root: &Path,
    phase: PhaseId,
    located: &LocatedSocket,
) -> Result<(), CliError> {
    let Some(launch_id) = located.launch_id.as_deref() else {
        return Ok(());
    };
    let Some(path) = liveness::launch_liveness_path(project_root, launch_id) else {
        return Ok(());
    };
    let deadline = std::time::Instant::now() + STOP_LOCK_RELEASE_WAIT;
    while let Some(holder) = held_liveness_holder(&path) {
        if std::time::Instant::now() >= deadline {
            return Err(CliError::Message(format!(
                "stop: phase {phase} launch {launch_id} ({holder}): `devflow stop` cannot reach \
                 it; nothing was signalled or recorded. End that process, then run `devflow \
                 stop --phase {phase}` again"
            )));
        }
        std::thread::sleep(PENDING_PUBLISH_POLL);
    }
    Ok(())
}

/// What one shutdown attempt in [`stop_via_socket`] came to.
enum ShutdownResult {
    /// The supervisor (or, after it died, the backstop) ended the run.
    Ended,
    /// The supervisor accepted the stop and will honour it at its next safe point; it has
    /// not confirmed it yet. Asking again would only wait again.
    AcceptedPending,
    /// The supervisor had handed the run to its successor.
    HandedOff,
    /// The socket went away under the request: locate the run again.
    Relocate,
}

/// Ask the ALIVE supervisor at `located` to stop.
///
/// A request that fails to reach an answer (a connect refused, a reset or a
/// broken pipe) may have met a supervisor dying under it: its listener is
/// released while the connection waits in the backlog, which
/// `request_shutdown` reports as an error (6d46008 made `probe` re-connect
/// for this, not `request_shutdown`). So the socket is probed once more:
/// STALE goes to [`stop_stale_socket`], GONE re-locates the run, and only a
/// supervisor that still answers, or a failed probe, makes the error stand.
fn shutdown_supervisor(
    phase: PhaseId,
    located: &LocatedSocket,
) -> Result<ShutdownResult, CliError> {
    let error = match supervisor::request_shutdown(&located.socket, shutdown_final_timeout()) {
        Ok(supervisor::ShutdownOutcome::Stopped {
            exit_code,
            advance_suppressed,
            group_signalled,
        }) => {
            println!(
                "{}",
                stopped_line(
                    &format!("phase {phase}"),
                    exit_code,
                    advance_suppressed,
                    group_signalled
                )
            );
            return Ok(ShutdownResult::Ended);
        }
        Ok(supervisor::ShutdownOutcome::AcceptedPending { owner }) => {
            println!(
                "{}",
                accepted_pending_line(&format!("phase {phase}"), &owner)
            );
            return Ok(ShutdownResult::AcceptedPending);
        }
        Ok(supervisor::ShutdownOutcome::HandedOff) => return Ok(ShutdownResult::HandedOff),
        Ok(supervisor::ShutdownOutcome::Refused(reason)) => {
            return Err(CliError::Message(format!(
                "stop: phase {phase}'s supervisor at {} refused the shutdown ({reason}); nothing \
                 was stopped or recorded",
                located.socket.display()
            )));
        }
        Ok(other) => {
            return Err(CliError::Message(format!(
                "stop: phase {phase}'s supervisor gave an unknown shutdown answer ({other:?}); \
                 nothing was recorded"
            )));
        }
        Err(error) => error,
    };
    match supervisor::probe(&located.socket) {
        Ok(supervisor::Liveness::Stale) => {
            println!(
                "stop: phase {phase}'s supervisor went away under the shutdown request ({error}); \
                 its socket is now STALE"
            );
            stop_stale_socket(phase, located).map(|()| ShutdownResult::Ended)
        }
        Ok(supervisor::Liveness::Gone) => Ok(ShutdownResult::Relocate),
        _ => Err(CliError::Message(format!(
            "stop: phase {phase}'s supervisor at {} did not answer the shutdown ({error}); \
             nothing was signalled or recorded",
            located.socket.display()
        ))),
    }
}

/// A probe that could not answer ALIVE, STALE or GONE: refused, naming the
/// error kind and the socket path, and never handed to the backstop (R-6).
fn probe_refusal(phase: PhaseId, socket: &Path, error: &supervisor::ProbeError) -> CliError {
    CliError::Message(format!(
        "stop: could not probe phase {phase}'s supervisor socket {} ({}: {error}); nothing was \
         signalled, removed or recorded",
        socket.display(),
        probe_error_kind(error)
    ))
}

/// A short name for a [`supervisor::ProbeError`]'s kind.
fn probe_error_kind(error: &supervisor::ProbeError) -> &'static str {
    match error {
        supervisor::ProbeError::Stat(_) => "stat failed",
        supervisor::ProbeError::NotASocket(_) => "not a socket",
        supervisor::ProbeError::ForeignOwner { .. } => "owned by another user",
        supervisor::ProbeError::Connect(_) => "connect failed",
        supervisor::ProbeError::NoReply(_) => "no valid ping reply",
        _ => "probe failed",
    }
}

/// A STALE socket: its supervisor is gone, and its agent group may not be.
/// The sidecar beside the socket is read and handed, with the state's record
/// when there is one, to the 50-09 backstop, which validates the group's
/// identity before it signals anything. A sidecar that cannot be read is a
/// refusal: nothing else can validate the group.
fn stop_stale_socket(phase: PhaseId, located: &LocatedSocket) -> Result<(), CliError> {
    let subject = format!("phase {phase}");
    let socket = supervisor::SocketPath::new(located.socket.clone())
        .map_err(|error| stale_refusal(&subject, &located.socket, &error.to_string()))?;
    let sidecar = read_stale_sidecar(&subject, &socket)?;
    run_backstop(
        &subject,
        &socket,
        &sidecar,
        located.record.as_ref(),
        (!located.pending_recorded).then_some(phase),
    )
}

/// The sidecar of a STALE socket, or the refusal that names why it cannot be
/// used.
fn read_stale_sidecar(
    subject: &str,
    socket: &supervisor::SocketPath,
) -> Result<supervisor::Sidecar, CliError> {
    supervisor::read_sidecar(&socket.sidecar_path()).map_err(|error| {
        let why = match &error {
            supervisor::SidecarError::Io(io) if io.kind() == std::io::ErrorKind::NotFound => {
                "is missing".to_string()
            }
            supervisor::SidecarError::Json(parse) => format!("does not parse: {parse}"),
            other => format!("could not be read: {other}"),
        };
        stale_refusal(subject, socket.as_path(), &format!("its sidecar {why}"))
    })
}

/// The refusal for a STALE socket whose agent group nothing can validate:
/// nothing is signalled or removed, and the manual repair is named.
fn stale_refusal(subject: &str, socket: &Path, why: &str) -> CliError {
    CliError::Message(format!(
        "stop: {subject}: socket {} is STALE and {why}; nothing recorded can validate its agent \
         group, so stop signals nothing — end its agent yourself and remove {} and {}",
        socket.display(),
        socket.display(),
        socket.with_extension("json").display()
    ))
}

/// Run the backstop on a STALE socket and report what it did. A group it
/// ended, or one it proved gone (previous boot, empty group, recycled
/// leader), is a stop; a refusal prints the backstop's own text (the
/// members, the residual F1 limit, the inspection command) and fails.
/// `resumable` names the phase a teardown that signalled nothing leaves
/// resumable (R-8); a targeted stop of a superseded launch passes `None`.
fn run_backstop(
    subject: &str,
    socket: &supervisor::SocketPath,
    sidecar: &supervisor::Sidecar,
    record: Option<&SupervisorRecord>,
    resumable: Option<PhaseId>,
) -> Result<(), CliError> {
    use supervisor::backstop::{BackstopDecision, BackstopOutcome};
    let outcome =
        supervisor::backstop::teardown_stale(socket, sidecar, record).map_err(|error| {
            CliError::Message(format!(
                "stop: {subject}: the stale-socket backstop did not act ({error}); nothing was \
             recorded"
            ))
        })?;
    match outcome {
        BackstopOutcome::Killed { members, .. } => {
            let pids: Vec<String> = members.iter().map(|m| m.pid.to_string()).collect();
            println!(
                "stop: {subject}: its supervisor is gone (STALE socket); ended its agent group {} \
                 ({} member(s): pid {}) and removed {} and its sidecar",
                sidecar.agent_pgid,
                members.len(),
                pids.join(", "),
                socket.as_path().display()
            );
            Ok(())
        }
        BackstopOutcome::RemovedNothingSignalled(decision) => {
            let why = match decision {
                BackstopDecision::PreviousBoot => "previous boot",
                BackstopDecision::EmptyGroup => "empty group",
                BackstopDecision::LeaderRecycled => "recycled leader",
                _ => "nothing to signal",
            };
            let resume = resumable.map_or_else(String::new, |phase| {
                format!("; the phase is resumable: run `devflow resume --phase {phase}`")
            });
            println!(
                "stop: {subject}: nothing was running ({why}); removed the stale socket {} and its \
                 sidecar{resume}",
                socket.as_path().display()
            );
            Ok(())
        }
        BackstopOutcome::Refused(decision) => Err(CliError::Message(format!(
            "stop: {subject}: {}",
            decision.describe(socket, sidecar)
        ))),
        other => Err(CliError::Message(format!(
            "stop: {subject}: the stale-socket backstop gave an unknown outcome ({other:?}); \
             nothing was recorded"
        ))),
    }
}

/// The primary path: answer `phase`'s open gate with a rejection whose note
/// contains the abort keyword, so `GateAction::from_response` resolves to
/// `GateAction::Abort` rather than `LoopBack(Code)` — looping back would
/// relaunch an agent on a phase the operator just asked to stop.
///
/// Returns the [`AnsweredGate`] when a gate was found and handled — either reaped
/// here, or discovered already answered by a race — telling the caller not
/// to fall through to [`stop_via_holder`]. Returns `Ok(None)` when there is
/// no open gate for `phase` at all, including the race where `Gates::reap_answer`
/// reports `NoOpenGate` after this function's own `Gates::list_open` scan
/// found one; that is the signal to fall through, not an error (cross-AI
/// review 23-10).
fn stop_via_gate(project_root: &Path, phase: PhaseId) -> Result<Option<AnsweredGate>, CliError> {
    let Some(gate) = Gates::list_open(project_root)
        .into_iter()
        .find(|g| g.phase == phase)
    else {
        return Ok(None);
    };
    stop_reap_observed_gate(project_root, phase, &gate)
}

/// [`stop_via_gate`]'s reap of the gate as it was listed: compare its id with the
/// gate file, read its waiter, then answer that id and no other. A gate it cannot
/// answer falls through to the socket path.
///
/// A gate whose answer a consumer already claimed is declined before anything else: a
/// finalizing `devflow ship` took it, and only the holder arm reaches that process.
fn stop_reap_observed_gate(
    project_root: &Path,
    phase: PhaseId,
    gate: &OpenGate,
) -> Result<Option<AnsweredGate>, CliError> {
    let observed = match classify_listed_gate(project_root, gate) {
        ListedGate::Current(observed) => observed,
        ListedGate::NoValidId => {
            println!(
                "stop: phase {phase} {} has a gate file with no valid gate id; no response was written. {}",
                gate.stage,
                stop_no_waiter_repair(project_root, phase, gate.stage)
            );
            return Ok(None);
        }
        ListedGate::Changed => {
            println!(
                "stop: phase {phase} {} gate changed since it was listed; not reaped",
                gate.stage
            );
            return Ok(None);
        }
    };
    if Gates::claim_path(project_root, phase, gate.stage, &observed).exists() {
        println!(
            "stop: phase {phase}'s {} gate (gate {observed}) was already answered and its answer \
             claimed by its consumer; stop did not answer it",
            gate.stage
        );
        return Ok(None);
    }
    let run = workflow::load_state(project_root, phase)
        .ok()
        .filter(|state| state.gate_pending && state.stage == gate.stage);
    let Some(run) = run else {
        println!(
            "stop: phase {phase} {} has a stale gate request; no response was written. {}",
            gate.stage,
            stop_no_waiter_repair(project_root, phase, gate.stage)
        );
        return Ok(None);
    };
    let run_launch = run.supervisor.map(|record| record.launch_id);
    // With no registered waiter, a Ship answer is still stored for `devflow ship` (D-04).
    if !waiter_is_live(project_root, phase, gate.stage, &observed) && gate.stage != Stage::Ship {
        println!(
            "stop: phase {phase} {} has no confirmed live waiter; no response was written. {}",
            gate.stage,
            stop_no_waiter_repair(project_root, phase, gate.stage)
        );
        return Ok(None);
    }
    match Gates::reap_answer(
        project_root,
        phase,
        gate.stage,
        &observed,
        "abort: stopped by `devflow stop`",
        "devflow-stop",
    ) {
        Ok(path) => {
            let waiter = if waiter_is_live(project_root, phase, gate.stage, &observed) {
                WaiterStatus::Live
            } else {
                WaiterStatus::Absent
            };
            println!(
                "stop: wrote a rejection for phase {phase} {} at {} — {}",
                gate.stage,
                path.display(),
                response_pickup_message(gate.stage, waiter)
            );
            Ok(Some((gate.stage, observed, run_launch, false)))
        }
        // A human, `--yes-ship`, or `devflow gate sweep` answered this gate
        // between our scan and this reap. Read its action: an approval or
        // loop-back is not `stop`'s outcome and must fall through to the
        // socket and holder arms; only an abort response can be treated as the
        // requested stop action, and only with a still-live waiter.
        Err(GateError::AlreadyResponded { .. }) => {
            stop_with_existing_response(project_root, phase, gate.stage, &observed, run_launch)
        }
        Err(GateError::NoOpenGate { .. }) => Ok(None),
        // The gate this stop observed is gone or replaced: the stop itself still has
        // to happen, through the socket path.
        Err(err @ GateError::GateIdMismatch { .. }) => {
            println!("stop: gate changed since it was listed; not reaped ({err})");
            Ok(None)
        }
        Err(err @ GateError::GateClosedAfterPublish { .. }) => {
            println!("stop: {err}");
            Ok(None)
        }
        // A wedged gate-lock holder, such as a suspended `gate approve`, must not leave
        // the run unstoppable: nothing was published, so the other arms may still act.
        Err(err @ GateError::GateLockTimeout { .. }) => {
            println!(
                "stop: phase {phase}'s {} gate was not answered — {err}; trying its supervisor \
                 socket and foreground holder",
                gate.stage
            );
            Ok(None)
        }
        // The abort is out, so the holder arm stays out of it: one `stop`, one action
        // (R2-04). If the gate had in fact closed, nothing reads the persisted `stopped` flag
        // at a safe point (999.172), so the caller still shuts a supervisor down.
        Err(err @ GateError::PublishedUnconfirmed { .. }) => {
            println!(
                "stop: phase {phase}'s {} gate (gate {observed}): the abort was published, but \
                 stop could not confirm the gate was still open — {err}",
                gate.stage
            );
            Ok(Some((gate.stage, observed, run_launch, true)))
        }
        Err(err) => Err(err.into()),
    }
}

/// Classify an answer which appeared after `stop_via_gate` saw an open gate.
/// An approval or loop-back is not attributable to `stop`, so that path must
/// fall through to the socket and holder arms. An abort can be the requested action
/// only while a waiter is registered. `run_launch` rides along in the [`AnsweredGate`].
fn stop_with_existing_response(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
    observed: &GateId,
    run_launch: Option<String>,
) -> Result<Option<AnsweredGate>, CliError> {
    let answer = match Gates::read_answer(project_root, phase, stage, observed) {
        AnswerRead::Bound(answer) => answer,
        AnswerRead::Unbound(reason) => {
            println!(
                "stop: phase {phase} {stage} has an answer that will be set aside ({reason}); stop did not write it"
            );
            return Ok(None);
        }
        AnswerRead::None => {
            println!(
                "stop: phase {phase} {stage}'s answer is gone, so its gate was decided; stop did not write it"
            );
            return Ok(None);
        }
    };
    match GateAction::from_response(&answer.response) {
        GateAction::Abort(_) if waiter_is_live(project_root, phase, stage, observed) => {
            println!(
                "stop: phase {phase} {stage} already has an abort response awaiting its registered waiter; stop did not write it"
            );
            Ok(Some((stage, observed.clone(), run_launch, false)))
        }
        GateAction::Abort(_) => {
            println!(
                "stop: phase {phase} {stage} already has an abort response, but no registered waiter will act; stop did not write it. {}",
                stop_no_waiter_repair(project_root, phase, stage)
            );
            Ok(None)
        }
        GateAction::Advance | GateAction::LoopBack(_) => {
            println!(
                "stop: phase {phase} {stage} already has a non-abort response; stop did not write it or claim its outcome"
            );
            Ok(None)
        }
    }
}

/// The fallback once no live supervisor socket remains (GONE, or none recorded): signal
/// the process that holds `phase`'s holder file (D-17), a foreground `start`, `resume`,
/// `devflow ship` or hand-run `advance`. Fail-closed on identity (T-23-52): the pid is
/// signalled only while the lock is held and only when it still has the start time the
/// record names, since a record can name a recycled pid. The holder file is the only
/// place this reads a pid from; state is never one.
///
/// `Ok(true)` when a holder was signalled.
fn stop_via_holder(project_root: &Path, phase: PhaseId) -> Result<bool, CliError> {
    let padded = phase.padded();
    let holder = liveness::live_holder(&liveness::phase_holder_path(project_root, phase));
    // Identity must be MATCHED against what the holder recorded, never inferred from
    // /proc (999.47). A bare cmdline-basename check alone returns true for any freshly
    // forked child of a devflow process that has not finished execve — during that window
    // the child carries its parent's cmdline and exe — so it would authorise signalling
    // an unrelated process. Confirmed in CI, not theorised.
    //
    // `(pid, starttime)` is unique for the life of a boot: a recycled pid necessarily
    // starts later than the one it replaced, and a mid-execve child has its own start
    // time distinct from its parent's.
    let pid = match holder {
        Ok(None) => {
            // D-19: with no supervisor left to ask, the phase's agent may still run. This
            // signals nothing, and `stopped` is not written beside an agent that still edits
            // the worktree; once the operator ends it, `devflow stop` records the stop.
            if let Ok(state) = workflow::load_state(project_root, phase)
                && let Some(agent) = live_phase_agent(project_root, &state)
            {
                return Err(CliError::Message(format!(
                    "stop: no live supervisor socket, pending launch or foreground holder for \
                     phase {phase}, but {}; phase state was not marked stopped",
                    orphaned_agent_line(phase, &agent)
                )));
            }
            println!(
                "stop: no live supervisor socket, pending launch or foreground holder for phase \
                 {phase} — nothing is running"
            );
            return Ok(false);
        }
        Ok(Some((pid, None))) => {
            return Err(CliError::Message(format!(
                "refusing to signal pid {pid} for phase {phase} — .devflow/.holder-{padded} \
                 records no start time, so this process's identity cannot be confirmed. If the \
                 run is genuinely stuck, inspect the pid manually (e.g. `ps -p {pid}`) before \
                 ending it yourself."
            )));
        }
        Ok(Some((pid, Some(recorded_start)))) => {
            if !agent::is_same_process(pid, recorded_start) {
                return Err(CliError::Message(format!(
                    "refusing to signal pid {pid} for phase {phase} — it is not the process that \
                     claimed .devflow/.holder-{padded}. The record names start time \
                     {recorded_start}, but pid {pid} now reports {:?}, so the pid has been \
                     recycled and belongs to something else. Inspect it manually (e.g. `ps -p \
                     {pid}`) before proceeding.",
                    agent::process_start_time(pid)
                )));
            }
            pid
        }
        Err(err) => {
            return Err(CliError::Message(format!(
                "refusing to signal anything for phase {phase} — .devflow/.holder-{padded} could \
                 not be read back for identity confirmation ({err}), so nothing was signalled. \
                 Run `devflow stop --phase {phase}` again, or inspect the process holding it."
            )));
        }
    };
    if agent::terminate(pid) {
        println!("stop: signalled pid {pid}, phase {phase}'s foreground holder");
        Ok(true)
    } else {
        println!("stop: pid {pid} could not be signalled (it may have just exited)");
        Ok(false)
    }
}

/// Wait up to [`agent::TERMINATE_VERIFY_WAIT`] for `phase`'s holder file to stop reading
/// live: `true` once nothing holds it. A probe that fails counts as held.
fn wait_for_holder_release(project_root: &Path, phase: PhaseId) -> bool {
    let path = liveness::phase_holder_path(project_root, phase);
    let released = || {
        matches!(
            liveness::probe(&path),
            Ok(liveness::Held::Abandoned | liveness::Held::Missing)
        )
    };
    let deadline = std::time::Instant::now() + agent::TERMINATE_VERIFY_WAIT;
    while std::time::Instant::now() < deadline {
        if released() {
            return true;
        }
        std::thread::sleep(agent::TERMINATE_VERIFY_POLL);
    }
    released()
}

/// `stop`'s report over a signalled holder that is still running (D-18): the expected
/// case while it finishes a hook or a git step, and no stop `stop` can claim.
fn holder_still_running(project_root: &Path, phase: PhaseId) -> String {
    let pid = liveness::live_holder(&liveness::phase_holder_path(project_root, phase))
        .ok()
        .flatten()
        .map_or_else(|| "unknown".to_string(), |(pid, _)| pid.to_string());
    format!(
        "stop: signalled pid {pid}, phase {phase}'s foreground holder, which is still running \
         {secs}s later, so phase state was not marked stopped. A DevFlow foreground process ends \
         at its next safe point and records the stop itself, except a `devflow ship` that has \
         started its finalization pass: it runs the pass to the end and, if the pass succeeds, \
         ships instead of stopping",
        secs = agent::TERMINATE_VERIFY_WAIT.as_secs()
    )
}

/// Persist the operator's intent: mark `stopped` and record why, preserving
/// any earlier reason (e.g. a prior `--until` halt) by appending rather
/// than overwriting. Never touches `stop_until` — that field means
/// something different (the requested halt stage for `devflow start
/// --until`) and `transition()` reads it. A phase with no persisted state
/// at all — never started, or already cleared by a completed abort — is
/// already stopped; that is success, not an error.
///
/// `signal_sent` means one fact: [`stop_via_holder`] signalled a foreground holder. A
/// supervisor ended through its socket is not that, because a supervisor claims no holder
/// (D-17). It decides both the wait for the holder's release and the line printed over a
/// missing state.
///
/// `known_launches` holds the launches this stop ended, answered, cleared or found not
/// live. The fresh read under the write lock decides: a launch outside it writes nothing
/// ([`unaddressed_launch_refusal`]), so a run never reads `stopped` beside a supervisor
/// that keeps working. R-03: `stop` is a repair verb, so a held write lock ends it within
/// the bound, naming the holder.
fn persist_stopped_state(
    project_root: &Path,
    phase: PhaseId,
    signal_sent: bool,
    known_launches: &[String],
) -> Result<(), CliError> {
    if signal_sent && !wait_for_holder_release(project_root, phase) {
        return Err(CliError::Message(holder_still_running(project_root, phase)));
    }
    let found_state = workflow::update_state_with_within(
        project_root,
        phase,
        workflow::WRITE_LOCK_REPAIR_WAIT,
        |existing| {
            let mut state = match existing {
                workflow::Existing::Loaded(state) => state,
                workflow::Existing::Missing => return Ok((None, false)),
                workflow::Existing::Unloadable(err) => return Err(CliError::from(err)),
            };
            if let Some(refusal) = unaddressed_launch_refusal(&state, known_launches) {
                return Err(refusal);
            }
            state.stopped = true;
            let reason = "stopped via `devflow stop`".to_string();
            state.stop_reason = Some(match state.stop_reason.take() {
                Some(existing) if !existing.is_empty() => format!("{existing}; {reason}"),
                _ => reason,
            });
            Ok((Some(state), true))
        },
    )?;
    if !found_state {
        println!("{}", missing_state_line(phase, signal_sent));
    }
    Ok(())
}

/// The refusal over a launch `state` names and this stop did not address: one a signalled
/// holder published before releasing, one started after the addressed launch exited, or
/// one that was pending when `stop` began and promoted while no arm addressed it. A dead
/// one is refused the same way, and the second `stop` finds it gone (T-50.1-159).
fn unaddressed_launch_refusal(state: &State, known_launches: &[String]) -> Option<CliError> {
    let recorded = state
        .supervisor
        .as_ref()
        .map(|record| (record.launch_id.as_str(), "supervisor"));
    let pending = state
        .pending_supervisor
        .as_ref()
        .map(|pending| (pending.launch_id.as_str(), "pending_supervisor"));
    let (id, field) = [recorded, pending]
        .into_iter()
        .flatten()
        .find(|(id, _)| !known_launches.iter().any(|known| known == id))?;
    let phase = state.phase;
    Some(CliError::Message(format!(
        "stop: phase {phase}'s state records launch {id} in its {field} field, a launch this \
         stop did not end, answer, clear or find not running; phase state was not marked \
         stopped — run `devflow stop --phase {phase}` again: it stops that launch if it still \
         runs, and if it does not, finds it gone or clears its reservation, then marks the \
         phase stopped"
    )))
}

/// What `stop` prints when the fresh read finds no state. With a signalled holder that
/// exited, the run ended on its own rather than at a safe point: a `devflow ship` already
/// in its pass shipped and cleared it, or it aborted. Nobody stopped it (D-18).
fn missing_state_line(phase: PhaseId, signal_sent: bool) -> String {
    if signal_sent {
        format!(
            "stop: phase {phase}'s signalled foreground holder exited and left no phase state: \
             the run ended on its own rather than at a safe point (a `devflow ship` that had \
             started its finalization pass runs it to the end and ships), so nothing was marked \
             stopped"
        )
    } else {
        format!("stop: no persisted state for phase {phase} — already stopped")
    }
}

/// Hold `phase` for this foreground process's lifetime (D-17), with the stop handler in
/// place first (D-18), so no recorded holder is ever a default-disposition SIGTERM target.
/// The one path all four foreground entry points claim through. A supervisor never calls
/// it: it has its own signal pipe, and its process holds a liveness file, not this.
///
/// Passing once through the state write lock after the claim is a barrier against a
/// recovery: one already past its liveness check, under that lock, could not see this
/// holder and may be deleting the state, so it finishes first and a state read after this
/// returns sees what it left. Any later recovery sees the holder. Bounded, because a
/// stuck lock holder must not hang a foreground verb, and refusing then releases the claim.
pub(crate) fn claim_foreground_holder(
    project_root: &Path,
    phase: PhaseId,
) -> Result<liveness::Hold, CliError> {
    liveness::install_holder_stop_handler().map_err(|err| {
        CliError::Message(format!(
            "phase {phase}: could not install the stop handler ({err}); refusing to run, \
             because `devflow stop` could then end this process inside a finalization hook"
        ))
    })?;
    let holder = liveness::claim_phase_holder(project_root, phase).map_err(|err| match err {
        liveness::HolderError::Held { holder } => {
            let pid = holder.map_or_else(|| "unknown".to_string(), |(pid, _)| pid.to_string());
            CliError::Message(format!(
                "phase {phase} is held by a foreground DevFlow process (pid {pid}); run \
                 `devflow stop --phase {phase}`"
            ))
        }
        liveness::HolderError::Io(err) => CliError::Message(format!(
            "phase {phase}: could not claim the holder file {}: {err}",
            liveness::phase_holder_path(project_root, phase).display()
        )),
    })?;
    workflow::update_state_with_within(
        project_root,
        phase,
        workflow::WRITE_LOCK_REPAIR_WAIT,
        |_| Ok::<_, CliError>((None, ())),
    )?;
    Ok(holder)
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
        "phase {} {} ({})\ngate id: {}\n{}",
        gate.phase,
        gate.stage,
        recover::format_age(&gate.timestamp),
        printable_gate_id(gate),
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
    let state_path = workflow::state_path(project_root, phase);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let state_exists = state_path.exists();
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
        // A supervisor whose state vanished writes no exit file (D-03) and has no
        // successor, so none will come. A state that was gone before the first poll is
        // the same case: every launch saves its state before it spawns (D-01), so there
        // is no run still to start here.
        if !state_exists && base == offset && new_offset == offset {
            eprintln!("== phase {phase}'s state is gone and no exit code was recorded ==");
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LaunchVerb {
    Start,
    Resume,
}

impl LaunchVerb {
    fn label(self) -> &'static str {
        match self {
            LaunchVerb::Start => "start",
            LaunchVerb::Resume => "resume",
        }
    }

    /// The command a refusal tells the operator to run again.
    fn rerun(self, phase: PhaseId) -> String {
        match self {
            LaunchVerb::Start => "devflow start".to_string(),
            LaunchVerb::Resume => format!("devflow resume --phase {phase}"),
        }
    }
}

/// The shared start/resume guard (D-09, R-12, 999.139). Two halves, either of
/// which refuses:
///
/// - **Supervisor** ([`possibly_live_sockets`]). The state loads → probe its
///   `supervisor` record: ALIVE is live; STALE is live unless
///   [`supervisor::backstop::assess`] proves its agent group gone
///   ([`stale_group_may_live`]); a probe error is live (fail closed,
///   T-50-40); GONE is not. The state is missing or will not load →
///   [`supervisor::sweep_for_in`] for this root and phase: an entry whose
///   sidecar parsed is classified the same way (with no record); an entry
///   with no usable sidecar, which 50-06 attributes by its file name or ping
///   reply, is live unless it probes GONE (RV-05: nothing recorded can prove
///   its group dead). A socket directory that exists but cannot be listed
///   refuses (the 50-06 threat flag: the sweep reads it as empty). With no
///   cache dir, no sweep runs, as for `stop` and `recover`.
/// - **Agent pid file** (unchanged, D-12): a live pid there is live. Pid
///   liveness, not identity: a recycled pid gives a fail-closed false
///   refusal (Option A, 2026-09-22). Not read without a state file (the
///   no-state carve-out below).
///
/// A loaded state's `pending_supervisor` (D-01, T-50-90) counts too, from the moment
/// it is reserved: a held liveness lock is live, with or without a socket, and its
/// refusal names only `devflow stop`. An unheld or missing lock is an abandoned
/// reservation, which does not refuse and which [`reserve_launch`] replaces.
///
/// Runs on the state [`reserve_launch`]'s closure read (criterion 2, D-01: one
/// writer per phase), before any gate cleanup, git mutation or state write, so a
/// refusal writes nothing. `--force` does not bypass it. `resume`'s missing-state
/// error comes after it; so does its load error, for a state the guard could only
/// find through its sidecar (R-12, 999.139). Only the `devflow resume` CLI arm
/// reaches this for `resume`, and an agent that runs it for its own phase is refused
/// correctly.
///
/// No-state carve-out (operator-approved 2026-09-22, unchanged by D-12): without a
/// state file the agent pid file is not read, so a live process it names does not
/// refuse, because `recover --clean` leaves that file behind and a recycled pid would
/// otherwise block `start` with no DevFlow repair. The supervisor sockets of this
/// root and phase are still swept.
///
/// Out of scope: the window between agent exit and `advance` taking over stays backlog
/// 999.136. This check says nothing about a gate waiter.
fn refuse_launch_over_a_live_run(
    project_root: &Path,
    phase: PhaseId,
    verb: LaunchVerb,
    existing: &workflow::Existing,
    cache_dir: Option<&Path>,
) -> Result<(), CliError> {
    let state_file = workflow::state_path(project_root, phase);
    let state = match existing {
        workflow::Existing::Loaded(state) => Some(state),
        _ => None,
    };
    let no_state = matches!(existing, workflow::Existing::Missing);
    let sockets =
        possibly_live_sockets(project_root, phase, state, cache_dir).map_err(|(dir, why)| {
            unlistable_socket_dir_refusal(phase, verb, (&state_file, no_state, &dir), &why)
        })?;
    let agent = (!no_state)
        .then(|| agent_pid_from_file(project_root, phase))
        .flatten()
        .filter(|&pid| agent::agent_running(pid));
    if sockets.is_empty() && agent.is_none() {
        return Ok(());
    }
    Err(CliError::Message(live_run_refusal(
        phase, verb, &sockets, agent,
    )))
}

/// What a foreground process's live-supervisor check found: the phase's recorded and
/// pending supervisors, read as [`refuse_launch_over_a_live_run`] reads them, as refusal
/// text and launch ids. `None` when none is live. A socket directory that cannot be listed
/// cannot rule one out, so it refuses too (fail closed). It only reads: the
/// abandoned-reservation clearing stays in [`reserve_launch`], and it reads no agent pid
/// file, because the property here is "no live supervisor".
fn find_live_supervisors(project_root: &Path, phase: PhaseId) -> Option<(String, Vec<String>)> {
    let state = workflow::load_state(project_root, phase).ok();
    let cache_dir = registry::cache_dir();
    match possibly_live_sockets(project_root, phase, state.as_ref(), cache_dir.as_deref()) {
        Ok(sockets) if sockets.is_empty() => None,
        Ok(sockets) => {
            let listed: Vec<String> = sockets
                .iter()
                .map(|socket| format!("launch {} ({})", socket.launch_id, socket.liveness))
                .collect();
            Some((
                format!("has a live supervisor ({})", listed.join(", ")),
                sockets.into_iter().map(|socket| socket.launch_id).collect(),
            ))
        }
        Err((dir, why)) => Some((
            format!(
                "may have a live supervisor: the socket directory {} could not be listed ({why})",
                dir.display()
            ),
            Vec::new(),
        )),
    }
}

/// D-08, D-17: a hand-run `devflow advance` refuses beside a live supervisor of its phase.
/// The long advance lock gave this exclusion until it was deleted. It writes no state.
pub(crate) fn refuse_advance_over_a_live_supervisor(
    project_root: &Path,
    phase: PhaseId,
) -> Result<(), CliError> {
    let Some((found, launch_ids)) = find_live_supervisors(project_root, phase) else {
        return Ok(());
    };
    events::emit(
        project_root,
        phase,
        "advance_failed",
        serde_json::json!({ "reason": "live supervisor", "launch_ids": launch_ids }),
    );
    Err(CliError::Message(format!(
        "advance refused: phase {phase} {found}; a hand-run advance would run beside it. Run \
         `devflow stop --phase {phase}` to end it"
    )))
}

/// R-17: `devflow ship` drives a Ship answer only when the monitor that would consume it
/// is dead, so it refuses beside a live supervisor of its phase. It would otherwise win
/// that answer from it and, on a loop-back, launch a second supervisor beside it. It
/// writes no state.
pub(crate) fn refuse_ship_over_a_live_supervisor(
    project_root: &Path,
    phase: PhaseId,
) -> Result<(), CliError> {
    let Some((found, launch_ids)) = find_live_supervisors(project_root, phase) else {
        return Ok(());
    };
    events::emit(
        project_root,
        phase,
        "ship_override_refused",
        serde_json::json!({ "reason": "live supervisor", "launch_ids": launch_ids }),
    );
    Err(CliError::Message(format!(
        "ship refused: phase {phase} {found}; `devflow ship` drives a Ship answer only when the \
         monitor that would consume it is dead. Answer the Ship gate with `devflow gate approve` \
         or `devflow gate reject` and the live supervisor consumes it, or run `devflow stop \
         --phase {phase}` first"
    )))
}

/// The supervisor half of [`refuse_launch_over_a_live_run`], shared with
/// `recover --clean --phase N` ([`refuse_a_live_supervisor`]): the sockets of
/// this root and phase that may still run. `state` is the loaded state, or
/// `None` when it is missing or does not load, which sweeps the socket
/// directory under `cache_dir` instead. `Err` names that directory and why
/// it cannot be listed.
fn possibly_live_sockets(
    project_root: &Path,
    phase: PhaseId,
    state: Option<&State>,
    cache_dir: Option<&Path>,
) -> Result<Vec<LiveSocket>, (PathBuf, String)> {
    if let Some(state) = state {
        return Ok(state
            .supervisor
            .as_ref()
            .and_then(|record| {
                recorded_live_socket(record).or_else(|| held_without_socket(project_root, record))
            })
            .into_iter()
            .chain(
                state
                    .pending_supervisor
                    .as_ref()
                    .and_then(pending_live_socket),
            )
            .collect());
    }
    let Some(cache_dir) = cache_dir else {
        return Ok(Vec::new());
    };
    if let Some(why) = socket_dir_unlistable(cache_dir) {
        return Err((registry::socket_dir_in(cache_dir), why));
    }
    Ok(supervisor::sweep_for_in(cache_dir, project_root, phase)
        .iter()
        .filter_map(swept_live_socket)
        .collect())
}

/// A supervisor socket the launch guard counts as possibly live.
struct LiveSocket {
    socket: PathBuf,
    /// The record's or sidecar's launch id; for a socket with no usable
    /// sidecar, its file name's, else its ping reply's, else `unknown`.
    launch_id: String,
    /// `ALIVE`, `STALE`, [`PENDING_NO_SOCKET`], or why it could not be probed.
    liveness: String,
    /// Why the sidecar cannot be used, for a socket with none that parses:
    /// `stop --launch` cannot validate it, so the refusal names bare `stop`.
    sidecar_problem: Option<String>,
    /// How the state names this supervisor, if it does.
    pending: PendingKind,
}

/// Whether a [`LiveSocket`] is a state's pending supervisor. While one is live, no
/// refusal suggests `devflow resume` (ROADMAP 50.1 criterion 2).
#[derive(Clone, Copy, PartialEq, Eq)]
enum PendingKind {
    /// Not pending: a recorded or swept supervisor.
    No,
    /// Pending, with a socket that was probed.
    Published,
    /// Pending, with no socket yet: only its held liveness lock says it is live.
    Reserved,
    /// Recorded, with its socket gone: only its held liveness lock says it is live, and
    /// `stop` has no socket to reach it through.
    RecordedNoSocket,
}

/// The liveness a refusal names for a pending supervisor with no socket yet.
const PENDING_NO_SOCKET: &str = "PENDING (reserved, no socket yet)";

impl LiveSocket {
    /// The refusal line that names this socket's repair.
    fn repair_line(&self, phase: PhaseId) -> String {
        let (id, liveness, socket) = (&self.launch_id, &self.liveness, self.socket.display());
        // No `--launch`: `stop --launch` validates a sidecar, and this has none yet.
        if self.pending == PendingKind::Reserved {
            return format!("launch {id} ({liveness}): run `devflow stop --phase {phase}` first");
        }
        if self.pending == PendingKind::RecordedNoSocket {
            return format!(
                "launch {id} ({liveness}): `devflow stop` cannot reach it; end that process first"
            );
        }
        match &self.sidecar_problem {
            None => format!(
                "launch {id} ({liveness}, socket {socket}): run \
                 `devflow stop --phase {phase} --launch {id}` first"
            ),
            Some(problem) => format!(
                "socket {socket} ({liveness}, launch {id}, its sidecar {problem}): run \
                 `devflow stop --phase {phase}` first — it shuts a supervisor that answers down \
                 through its socket; one that does not answer cannot be validated without its \
                 sidecar, so end its agent yourself and remove `{socket}` and `{}`",
                self.socket.with_extension("json").display()
            ),
        }
    }
}

/// The loaded state's record as a [`LiveSocket`], or `None` when its socket
/// is GONE or STALE over a group the backstop proves gone.
fn recorded_live_socket(record: &SupervisorRecord) -> Option<LiveSocket> {
    socket_live_socket(&record.socket_path, &record.launch_id, Some(record))
}

/// The live supervisor `record` names when its GONE socket cannot say so: it holds its
/// launch's liveness file for its whole life. It unlinks its socket before it exits, so a
/// release within [`STOP_LOCK_RELEASE_WAIT`] is one exiting. `None` when nothing holds that
/// file. A probe that fails is live (fail closed, T-50-40).
fn held_without_socket(project_root: &Path, record: &SupervisorRecord) -> Option<LiveSocket> {
    let path = liveness::launch_liveness_path(project_root, &record.launch_id)?;
    let deadline = std::time::Instant::now() + STOP_LOCK_RELEASE_WAIT;
    let holder = loop {
        let holder = held_liveness_holder(&path)?;
        if std::time::Instant::now() >= deadline {
            break holder;
        }
        std::thread::sleep(PENDING_PUBLISH_POLL);
    };
    Some(LiveSocket {
        socket: record.socket_path.clone(),
        launch_id: record.launch_id.clone(),
        liveness: holder,
        sidecar_problem: None,
        pending: PendingKind::RecordedNoSocket,
    })
}

/// Who holds the liveness file at `path`, in words, or `None` when nothing does.
fn held_liveness_holder(path: &Path) -> Option<String> {
    match liveness::probe(path) {
        Ok(liveness::Held::Abandoned | liveness::Held::Missing) => None,
        Ok(liveness::Held::Live) => Some(match liveness::live_holder(path) {
            Ok(Some((pid, _))) => format!("socket gone, pid {pid} holds {}", path.display()),
            _ => format!("socket gone, {} is held", path.display()),
        }),
        Err(error) => Some(format!("liveness probe failed ({error})")),
    }
}

/// A state's pending supervisor as a [`LiveSocket`], or `None` when its reservation
/// is abandoned. A published socket is probed as a recorded one is, and with or
/// without one, a held liveness lock is live: only the lock and the socket decide,
/// never a pid (D-01). A probe that fails is live (fail closed, T-50-40).
fn pending_live_socket(pending: &PendingSupervisor) -> Option<LiveSocket> {
    if pending.socket_path.exists()
        && let Some(live) = socket_live_socket(&pending.socket_path, &pending.launch_id, None)
    {
        return Some(LiveSocket {
            pending: PendingKind::Published,
            ..live
        });
    }
    let liveness = match liveness::probe(&pending.liveness_path) {
        Ok(liveness::Held::Live) => PENDING_NO_SOCKET.to_string(),
        Ok(liveness::Held::Abandoned | liveness::Held::Missing) => return None,
        Err(error) => format!("liveness probe failed ({error})"),
    };
    Some(LiveSocket {
        socket: pending.socket_path.clone(),
        launch_id: pending.launch_id.clone(),
        liveness,
        sidecar_problem: None,
        pending: PendingKind::Reserved,
    })
}

/// The supervisor at `socket` as a [`LiveSocket`], or `None` when it is GONE or
/// STALE over a group the backstop proves gone. `record` is the state's record of
/// it, when the state has published one.
fn socket_live_socket(
    socket: &Path,
    launch_id: &str,
    record: Option<&SupervisorRecord>,
) -> Option<LiveSocket> {
    let live = |liveness: &str, sidecar_problem: Option<String>| LiveSocket {
        socket: socket.to_path_buf(),
        launch_id: launch_id.to_string(),
        liveness: liveness.to_string(),
        sidecar_problem,
        pending: PendingKind::No,
    };
    match supervisor::probe(socket) {
        Ok(supervisor::Liveness::Gone) => None,
        Ok(supervisor::Liveness::Alive(_)) => Some(live("ALIVE", None)),
        Err(error) => Some(live(&probe_failure(&error), None)),
        Ok(supervisor::Liveness::Stale) => {
            match supervisor::read_sidecar(&socket.with_extension("json")) {
                Ok(sidecar) => {
                    let decision = supervisor::backstop::assess(&sidecar, record);
                    stale_group_may_live(&decision).then(|| live("STALE", None))
                }
                Err(supervisor::SidecarError::Io(error))
                    if error.kind() == std::io::ErrorKind::NotFound =>
                {
                    Some(live("STALE", Some("is missing".to_string())))
                }
                Err(error) => Some(live("STALE", Some(format!("is unusable ({error})")))),
            }
        }
    }
}

/// A sweep entry for this root and phase as a [`LiveSocket`], or `None`
/// when it is GONE, or STALE with a parsed sidecar whose group the backstop
/// proves gone. An entry with no usable sidecar is live unless GONE (RV-05).
fn swept_live_socket(entry: &supervisor::SweepEntry) -> Option<LiveSocket> {
    let liveness = match &entry.liveness {
        Ok(supervisor::Liveness::Gone) => return None,
        Ok(supervisor::Liveness::Alive(_)) => "ALIVE".to_string(),
        Ok(supervisor::Liveness::Stale) => "STALE".to_string(),
        Err(error) => probe_failure(error),
    };
    let (launch_id, sidecar_problem) = match &entry.sidecar {
        Some(sidecar) => {
            let stale = matches!(entry.liveness, Ok(supervisor::Liveness::Stale));
            if stale && !stale_group_may_live(&supervisor::backstop::assess(sidecar, None)) {
                return None;
            }
            (sidecar.launch_id.clone(), None)
        }
        None => {
            let pinged = match &entry.liveness {
                Ok(supervisor::Liveness::Alive(reply)) => Some(reply.launch_id.clone()),
                _ => None,
            };
            let problem = entry.sidecar_error.as_ref().map_or_else(
                || "is missing".to_string(),
                |error| format!("is unusable ({error})"),
            );
            let id = entry.launch_id_in_name.clone().or(pinged);
            (id.unwrap_or_else(|| "unknown".to_string()), Some(problem))
        }
    };
    Some(LiveSocket {
        socket: entry.socket_path.clone(),
        launch_id,
        liveness,
        sidecar_problem,
        pending: PendingKind::No,
    })
}

/// A probe error as the liveness a refusal names.
fn probe_failure(error: &supervisor::ProbeError) -> String {
    format!("probe failed ({}: {error})", probe_error_kind(error))
}

/// Whether a STALE socket's agent group may still run. Only the three
/// backstop decisions that prove the group gone (a previous boot, an empty
/// group, a recycled leader) say no; every refusal, an unprovable identity
/// (MAC-2), a live leader and any decision added later fail closed (T-50-40).
fn stale_group_may_live(decision: &supervisor::backstop::BackstopDecision) -> bool {
    use supervisor::backstop::BackstopDecision;
    !matches!(
        decision,
        BackstopDecision::PreviousBoot
            | BackstopDecision::EmptyGroup
            | BackstopDecision::LeaderRecycled
    )
}

/// The refusal for a state that is missing (`no_state`) or will not load
/// over a socket directory that cannot be listed: a live run may be named
/// only by its socket there, so the launch fails closed (50-06 threat flag).
fn unlistable_socket_dir_refusal(
    phase: PhaseId,
    verb: LaunchVerb,
    (state_file, no_state, dir): (&Path, bool, &Path),
    why: &str,
) -> CliError {
    let label = verb.label();
    let state = if no_state {
        format!("there is no state file {}", state_file.display())
    } else {
        format!("the state file {} does not load", state_file.display())
    };
    CliError::Message(format!(
        "phase {phase}: {state}, and the supervisor socket directory {} cannot be listed ({why}) \
         — refusing to {label}; nothing was written\n\
         A live run of this phase may be named only by its socket there, so {label} fails \
         closed. Make {} listable by this user (DevFlow creates it mode 0700), then run `{}` again.",
        dir.display(),
        dir.display(),
        verb.rerun(phase)
    ))
}

/// The refusal text for [`refuse_launch_over_a_live_run`]: each live socket is named with the `stop` line
/// that ends it (50-14), and a live agent pid with the identity checks the
/// operator must make before signalling it.
fn live_run_refusal(
    phase: PhaseId,
    verb: LaunchVerb,
    sockets: &[LiveSocket],
    agent: Option<u32>,
) -> String {
    let mut named: Vec<String> = sockets
        .iter()
        .map(|live| format!("supervisor launch {} {}", live.launch_id, live.liveness))
        .collect();
    named.extend(agent.map(|pid| format!("agent pid {pid} alive")));
    let mut text = format!(
        "phase {phase}: a run is still live ({}) — refusing to {}; nothing was written",
        named.join(", "),
        verb.label()
    );
    if !sockets.is_empty() {
        text.push_str(
            "\n`devflow stop` ends a supervisor through its socket, whether or not the state \
             file loads; on a STALE socket it signals the agent group only after proving that \
             group is this launch's. Run each line below first:",
        );
        for live in sockets {
            text.push_str("\n  ");
            text.push_str(&live.repair_line(phase));
        }
    }
    // Criterion 2: `resume`'s own lines name `devflow resume`, and while a pending
    // supervisor is live none of them may be printed. Once it is gone, `status` says
    // what is next. A live agent is named above, and `stop` ends a supervisor's agent.
    let pending_blocks_resume =
        verb == LaunchVerb::Resume && sockets.iter().any(|live| live.pending != PendingKind::No);
    if let Some(pid) = agent.filter(|_| !pending_blocks_resume) {
        text.push('\n');
        text.push_str(&agent_refusal_guidance(phase, verb, pid));
    }
    if pending_blocks_resume {
        text.push_str(
            "\nOnce every supervisor and process named above is gone, `devflow status` names \
             what to do next.",
        );
    } else {
        text.push_str(&format!(
            "\nOnce every supervisor and process named above is gone, run `{}` again.",
            verb.rerun(phase)
        ));
    }
    text
}

/// The live-run refusal's paragraph for a live agent its pid file names:
/// `stop` never signals that pid, so the operator confirms its identity
/// before signalling it, and a recycled pid has its own repair per verb.
fn agent_refusal_guidance(phase: PhaseId, verb: LaunchVerb, pid: u32) -> String {
    let stopped_note = match verb {
        LaunchVerb::Start => "",
        LaunchVerb::Resume => {
            " A state already marked stopped does not mean these processes have exited: \
             with no live supervisor, `devflow status` names a live agent's pid and the manual \
             kill, and resume refuses until every named process has exited."
        }
    };
    let recycled = match verb {
        LaunchVerb::Start => format!(
            "If a named pid is live but is not this phase's process (a recycled pid), `start` \
             refuses again: run `devflow recover --clean --phase {phase}` first, then \
             `devflow start`."
        ),
        LaunchVerb::Resume => format!(
            "If a named pid is live but is not this phase's process (a recycled pid), `resume` \
             refuses again, and no DevFlow verb clears a recorded pid while keeping the phase's \
             work: `devflow recover --clean --phase {phase}` removes the phase state, after \
             which `resume` has nothing to resume, and `devflow start` refuses while the \
             phase's worktree or branch exists; `devflow start --force` recreates them from \
             the base and discards the phase's work."
        ),
    };
    format!(
        "The agent pid file names a live process, and `devflow stop` never signals a pid from \
         the agent pid file.{stopped_note} Check each pid before signalling it. `ps -p` shows \
         only the executable name, which cannot tell phases apart; `-ww` keeps a long command \
         line from being cut at the terminal width:\n  \
         agent pid {pid}: ps -ww -o pid=,lstart=,args= -p {pid}\n\
         This phase's agent has its supervisor, a `__monitor` process with this project's path \
         and `--phase {phase}` in its args, as its parent (`ps -o ppid= -p <pid>`); an agent \
         whose supervisor has exited cannot be tied to this phase that way, so do not signal it \
         on the pid alone. {recycled} Do neither while this phase's processes are live: \
         `recover --clean` clears state even while an agent runs, and a later `start` would \
         launch a second agent beside it."
    )
}

/// Read the launched agent PID the monitor recorded for `phase`, if present.
pub(crate) fn agent_pid_from_file(project_root: &Path, phase: PhaseId) -> Option<u32> {
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

/// `recover --clean --phase N` is a file cleaner, not a teardown (D-06): it
/// refuses while the phase's supervisor may still run, by the launch guard's
/// supervisor rule ([`possibly_live_sockets`]): an ALIVE socket, a STALE one
/// whose agent group the backstop does not prove gone, or a probe that
/// fails, found through the state's record, or, when the state is missing or
/// does not load, through a sweep of this root and phase (which refuses when
/// the socket directory cannot be listed). The agent pid file is not read.
/// Nothing is deleted.
fn refuse_a_live_supervisor(
    project_root: &Path,
    phase: PhaseId,
    cache_dir: Option<&Path>,
) -> Result<(), CliError> {
    let state = workflow::load_state(project_root, phase).ok();
    let sockets = possibly_live_sockets(project_root, phase, state.as_ref(), cache_dir).map_err(
        |(dir, why)| {
            CliError::Message(format!(
                "phase {phase}: the supervisor socket directory {} cannot be listed ({why}), so a \
                 live supervisor cannot be ruled out — recover --clean deleted nothing",
                dir.display()
            ))
        },
    )?;
    if sockets.is_empty() {
        return Ok(());
    }
    let mut text = format!(
        "phase {phase} has a supervisor that may still run — recover --clean deleted nothing. \
         Run each line below first:"
    );
    for live in &sockets {
        text.push_str("\n  ");
        text.push_str(&live.repair_line(phase));
    }
    Err(CliError::Message(text))
}

pub(crate) fn recover_cmd(
    project_root: &Path,
    do_clean: bool,
    phase: Option<PhaseId>,
) -> Result<(), CliError> {
    if do_clean {
        match phase {
            // Explicit phase: cleaned unless its supervisor, pending supervisor, gate
            // waiter or foreground holder may be live, so a wedged live run needs
            // `devflow stop` first. This guard is the one that can read a STALE socket
            // through the backstop; `clean_phase_report` decides again under the write
            // lock, with what is on disk by then.
            Some(phase) => {
                refuse_a_live_supervisor(project_root, phase, registry::cache_dir().as_deref())?;
                let report = recover::clean_phase_report(project_root, phase)?;
                for warning in &report.warnings {
                    println!("warning: {warning}");
                }
                if report.removal_failed {
                    let removed = if report.removed_anything {
                        "some of its files were removed"
                    } else {
                        "nothing was removed"
                    };
                    return Err(CliError::Message(format!(
                        "recover --clean could not remove everything for phase {phase} \
                         ({removed}) — see the warnings above"
                    )));
                }
                if report.removed_anything {
                    println!("cleaned up workflow state for phase {phase}");
                } else {
                    println!(
                        "nothing to clean for phase {phase}: no workflow state, gate files or \
                         cron record"
                    );
                }
            }
            // Implicit sweep: stale phases only.
            None => {
                let report = recover::clean_report(project_root)?;
                for warning in &report.warnings {
                    println!("warning: {warning}");
                }
                let join = |phases: &[PhaseId]| {
                    phases
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                if !report.cleared.is_empty() {
                    println!(
                        "cleaned up stale workflow state for phase {}",
                        join(&report.cleared)
                    );
                }
                if !report.orphan_gates_cleared.is_empty() {
                    println!(
                        "removed orphan gate files for phase {}",
                        join(&report.orphan_gates_cleared)
                    );
                }
                if !report.orphan_cron_records_removed.is_empty() {
                    println!(
                        "removed orphan cron records for phase {}",
                        join(&report.orphan_cron_records_removed)
                    );
                }
                for path in &report.markers_removed {
                    println!("removed abandoned marker {}", path.display());
                }
                if report.cleared.is_empty()
                    && report.orphan_gates_cleared.is_empty()
                    && report.orphan_cron_records_removed.is_empty()
                    && report.markers_removed.is_empty()
                {
                    println!("no stale workflow state was cleaned");
                }
                if !report.quarantined.is_empty() {
                    println!("set-aside gate answers kept (D-05; remove each by hand once read):");
                    for path in &report.quarantined {
                        println!("  {}", path.display());
                    }
                }
                if report.removal_failed {
                    return Err(CliError::Message(
                        "recover --clean could not remove everything — see the warnings above"
                            .into(),
                    ));
                }
            }
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
        if let Some(reason) = &status.live {
            println!("  may be live: {reason}");
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
    doctor_in(project_root, json, registry::cache_dir().as_deref())
}

/// [`doctor`] against an explicit cache dir, whose `sock/` directory the
/// read-only supervisor sweep lists (50-12); `None` (no cache dir can be
/// resolved) skips the sweep without an error. Split out so tests sweep a
/// tempdir, never the operator's real cache.
fn doctor_in(project_root: &Path, json: bool, cache_dir: Option<&Path>) -> Result<(), CliError> {
    let checks = doctor_checks();

    let facts = collect_phase_facts(project_root);
    let doc_findings = collect_planning_doc_findings(project_root);
    // 999.44/DEN-68: a registry-independent, read-only /proc census — the
    // only I/O this adds is `agent::discover_stray_devflow_processes`'s
    // scan, which never signals anything (T-25-62, doctor's read-only
    // contract).
    let stray_findings = collect_stray_process_findings(project_root);
    // 50-12 (D-06): the read-only socket sweep — orphaned, superseded and
    // deleted-root supervisors. It removes nothing and signals nothing.
    let orphans = cache_dir
        .map(|cache| collect_orphan_supervisors(project_root, cache))
        .unwrap_or_default();
    let sweep_unlistable = cache_dir.and_then(socket_dir_unlistable);
    let set_aside = set_aside_answer_findings(project_root, &Gates::quarantined(project_root));

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
        let body = doctor_json_body(
            &checks,
            &facts,
            &doc_findings,
            &stray_findings,
            (&orphans, sweep_unlistable.as_deref()),
            &set_aside,
        );
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
        print!("{}", render_set_aside_findings_text(&set_aside));
        print!("{}", render_planning_doc_text(&doc_findings));
        print!("{}", render_stray_process_text(&stray_findings));
        print!(
            "{}",
            render_orphan_supervisors_text(&orphans, sweep_unlistable.as_deref())
        );
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
    Info,
    Warn,
    Problem,
}

impl Severity {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Severity::Ok => "ok",
            Severity::Info => "info",
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
    /// Whether the phase's open gate has a held registration, computed in
    /// `build_phase_facts`; all its I/O stays there so reconciliation remains pure.
    pub(crate) waiter: WaiterStatus,
    /// The stage and id of the open gate [`PhaseFacts::waiter`] describes; `None` when
    /// the phase has no open gate. The id is the gate file's, unchecked.
    pub(crate) waiter_gate: Option<(Stage, String)>,
    pub(crate) agent_pid: Option<u32>,
    pub(crate) agent_alive: bool,
    /// The supervisor's liveness, from one probe of the socket
    /// `state.supervisor` records (50-12) — never a pid.
    pub(crate) supervisor: PhaseLiveness,
    /// The launch id `state.supervisor` records; `None` when no supervisor
    /// is recorded. It identifies the supervisor in findings in place of its
    /// socket path (T-18-01).
    pub(crate) supervisor_launch_id: Option<String>,
    /// The most recent event's `event` field value, for display context.
    pub(crate) last_event: Option<String>,
    /// The `stage` field of the most recent `stage_launched` event; `None`
    /// when the last event recorded for this phase is not a launch.
    pub(crate) last_launched_stage: Option<Stage>,
    pub(crate) open_gate_stages: Vec<Stage>,
    pub(crate) feature_branch_exists: bool,
    /// Whether this phase was intentionally halted by `devflow start --until
    /// <stage>` (20c) — `State.stopped`. A stopped phase's dead agent
    /// pid/exited supervisor are expected, not a crash; both
    /// `check_dead_agent` and `check_supervisor` must recognize this
    /// marker instead of reporting a `Problem` (review: Codex HIGH — the
    /// doctor gap is bigger than `check_dead_agent` alone).
    pub(crate) stopped: bool,
    /// [`ship_answer_stored`]: `devflow ship` applies it and `resume` would set it aside.
    pub(crate) stored_ship_answer: bool,
    /// The `start` to run instead of `resume` when [`run_never_prepared`] holds.
    pub(crate) unprepared_restart: Option<String>,
    /// The pending launch's id and liveness, from `state.pending_supervisor`. The
    /// liveness is `LaunchPending` while it lives and a GONE state once abandoned.
    pub(crate) pending: Option<(String, PhaseLiveness)>,
    /// The phase's agent, when no supervisor is live and it still runs (D-19).
    pub(crate) live_agent: Option<LiveAgent>,
    /// The state write lock, read once without waiting (R-03).
    pub(crate) write_lock: workflow::WriteLockStatus,
    /// Why [`PhaseFacts::write_lock`] could not be read; it then reads free.
    pub(crate) write_lock_error: Option<String>,
}

/// One diagnostic finding for a phase, with a copy-pasteable repair command
/// when one exists. Never carries an absolute filesystem path or username
/// (T-18-01): phase numbers, stage names and pids identify the disagreement, and
/// a set-aside answer names its file from the project root (D-05).
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
        detail: if facts.stored_ship_answer {
            format!(
                "phase {}: gate_pending is true at stage {} and its gate holds a stored answer",
                facts.phase, facts.stage
            )
        } else {
            format!(
                "phase {}: gate_pending is true at stage {} but no gate file is open",
                facts.phase, facts.stage
            )
        },
        repair: Some(repair_instead_of_resume(
            facts,
            format!("devflow resume --phase {}", facts.phase),
        )),
    })
}

/// A repair that would send the operator to `devflow resume`, unless the launch guard
/// would refuse it: `stop` while a supervisor of the phase is live (criterion 2), and the
/// manual kill while an agent outlives its supervisor (D-19). An ALIVE supervisor may be
/// healthy and between stages, so `stop` is offered, not prescribed.
fn repair_instead_of_resume(facts: &PhaseFacts, resume: String) -> String {
    let phase = facts.phase;
    match (&facts.supervisor, &facts.live_agent) {
        (PhaseLiveness::LaunchPending { .. }, _) => format!("devflow stop --phase {phase}"),
        (PhaseLiveness::HeldNoSocket { .. }, _) => HELD_NO_SOCKET_REPAIR.to_string(),
        (PhaseLiveness::Alive { .. }, _) => {
            format!("wait for its live supervisor, or `devflow stop --phase {phase}`")
        }
        (_, Some(agent)) => orphaned_agent_line(phase, agent),
        _ if facts.stored_ship_answer => format!("devflow ship --phase {phase}"),
        _ => facts.unprepared_restart.clone().unwrap_or(resume),
    }
}

/// An open gate has no process that may still be waiting to consume an
/// answer. Report the same recovery command the gate verbs already name;
/// doctor remains report-only.
fn check_open_gate_without_waiter(facts: &PhaseFacts) -> Option<PhaseFinding> {
    if facts.open_gate_stages.is_empty() || facts.waiter == WaiterStatus::Live {
        return None;
    }
    let gate_stage = facts.open_gate_stages[0];
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Problem,
        detail: format!(
            "phase {}: gate open for stage {} but no waiter may still be waiting",
            facts.phase, gate_stage
        ),
        repair: Some(if gate_stage == Stage::Ship {
            no_waiter_repair(facts.phase, gate_stage)
        } else {
            repair_instead_of_resume(facts, no_waiter_repair(facts.phase, gate_stage))
        }),
    })
}

/// An open gate file exists but `gate_pending` is false — an unanswered
/// operator question that `status`/`doctor` isn't surfacing as pending.
fn check_orphan_gate(facts: &PhaseFacts) -> Option<PhaseFinding> {
    if facts.gate_pending || facts.open_gate_stages.is_empty() || facts.waiter != WaiterStatus::Live
    {
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
        repair: Some(repair_instead_of_resume(
            facts,
            format!("devflow resume --phase {}", facts.phase),
        )),
    })
}

/// The supervisor's liveness needs a repair (50-12): STALE (its socket
/// remains, nothing answers, its agent group may still run), GONE while the
/// phase is not stopped (nothing will advance it), or a probe that could not
/// answer. Reuses [`phase_liveness`]'s result from the facts rather than
/// re-deriving it, so `status` and `doctor` cannot drift. ALIVE, a stopped
/// GONE and NotLaunched are never findings (T-18-11: an unrecorded
/// supervisor is never a problem), unless its agent still runs (D-19). A live
/// pending launch is [`check_pending_launch`]'s. Identifies the supervisor by phase
/// and launch id, never by its socket path (T-18-01).
fn check_supervisor(facts: &PhaseFacts) -> Option<PhaseFinding> {
    let phase = facts.phase;
    if let (PhaseLiveness::Gone | PhaseLiveness::NotLaunched, Some(agent)) =
        (&facts.supervisor, &facts.live_agent)
    {
        return Some(PhaseFinding {
            phase,
            severity: Severity::Problem,
            detail: format!(
                "phase {phase}: no supervisor is live but its agent (pid {}) still runs",
                agent.pid
            ),
            repair: Some(orphaned_agent_line(phase, agent)),
        });
    }
    let repair = match &facts.supervisor {
        PhaseLiveness::Stale => {
            let verb = if facts.stored_ship_answer {
                "ship"
            } else {
                "resume"
            };
            format!("devflow stop --phase {phase} && devflow {verb} --phase {phase}")
        }
        PhaseLiveness::GoneWhileActive => {
            repair_instead_of_resume(facts, format!("devflow resume --phase {phase}"))
        }
        PhaseLiveness::HeldNoSocket { .. } => HELD_NO_SOCKET_REPAIR.to_string(),
        // `status` prints the operator's own socket path, which doctor
        // must not.
        PhaseLiveness::ProbeFailed(_) => "devflow status".to_string(),
        PhaseLiveness::Alive { .. }
        | PhaseLiveness::Gone
        | PhaseLiveness::NotLaunched
        | PhaseLiveness::LaunchPending { .. } => {
            return None;
        }
    };
    let launch = facts
        .supervisor_launch_id
        .as_deref()
        .or(facts.pending.as_ref().map(|(id, _)| id.as_str()))
        .unwrap_or("?");
    Some(PhaseFinding {
        phase,
        severity: Severity::Problem,
        detail: format!(
            "phase {phase}: supervisor launch {launch} at stage {} is {}",
            facts.stage,
            facts.supervisor.describe()
        ),
        repair: Some(repair),
    })
}

/// A reservation the state names (D-02). While its supervisor lives, `stop` is the
/// repair. A `Warn`, since a healthy launch reads this way until it is promoted. Once
/// abandoned it blocks nothing, so it is a note with no repair of its own.
fn check_pending_launch(facts: &PhaseFacts) -> Option<PhaseFinding> {
    let phase = facts.phase;
    let (launch_id, liveness) = facts.pending.as_ref()?;
    let (detail, repair) = if matches!(liveness, PhaseLiveness::LaunchPending { .. }) {
        (
            format!("phase {phase}: {}", liveness.describe()),
            Some(format!("devflow stop --phase {phase}")),
        )
    } else {
        (
            format!(
                "phase {phase}: launch reservation {launch_id} is abandoned (no process holds \
                 it); the next launch or `devflow stop --phase {phase}` clears it"
            ),
            None,
        )
    };
    Some(PhaseFinding {
        phase,
        severity: Severity::Warn,
        detail,
        repair,
    })
}

/// A held state write lock (R-03), from one reading that did not wait; a lock that could
/// not be read is named too. A `Warn`, because a single reading cannot tell a brief hold
/// from a stuck one: [`WRITE_LOCK_HOLD_NOTE`] says how to judge it.
fn check_write_lock(facts: &PhaseFacts) -> Option<PhaseFinding> {
    let phase = facts.phase;
    let (detail, repair) = if let Some(error) = &facts.write_lock_error {
        (
            format!("phase {phase}: could not read the state write lock ({error})"),
            None,
        )
    } else {
        let workflow::WriteLockStatus::Held { pid } = facts.write_lock else {
            return None;
        };
        let give_up = "`devflow stop` and `devflow recover --clean` give up after 10 s naming it";
        match pid {
            Some(pid) => (
                format!(
                    "phase {phase}: the state write lock is held by pid {pid} — \
                     {WRITE_LOCK_HOLD_NOTE}"
                ),
                Some(format!("inspect it with `ps -p {pid}`; {give_up}")),
            ),
            None => (
                format!(
                    "phase {phase}: the state write lock is held by an unknown holder — \
                     {WRITE_LOCK_HOLD_NOTE}"
                ),
                Some(give_up.to_string()),
            ),
        }
    };
    Some(PhaseFinding {
        phase,
        severity: Severity::Warn,
        detail,
        repair,
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
        check_open_gate_without_waiter(facts),
        check_orphan_gate(facts),
        check_dead_agent(facts),
        check_supervisor(facts),
        check_pending_launch(facts),
        check_stage_event_drift(facts),
        check_missing_branch(facts),
        check_write_lock(facts),
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
    let unprepared_restart = run_never_prepared(&state).then(|| restart_verb(&state));
    let stored_ship_answer = ship_answer_stored(&state);
    let (waiter, waiter_gate) = phase_gate_waiter(project_root, phase, open_gates);
    let agent_pid = agent_pid_from_file(project_root, phase);
    let agent_alive = agent_pid.is_some_and(agent::agent_running);
    let supervisor = probe_phase(&state);
    let pending = state.pending_supervisor.as_ref().map(|pending| {
        let gone = if stopped {
            PhaseLiveness::Gone
        } else {
            PhaseLiveness::GoneWhileActive
        };
        (
            pending.launch_id.clone(),
            pending_liveness(pending).unwrap_or(gone),
        )
    });
    let live_agent = no_supervisor_live(&supervisor)
        .then(|| live_phase_agent(project_root, &state))
        .flatten();
    let (write_lock, write_lock_error) = match workflow::write_lock_status(project_root, phase) {
        Ok(status) => (status, None),
        Err(error) => (workflow::WriteLockStatus::Free, Some(error.to_string())),
    };
    let supervisor_launch_id = state
        .supervisor
        .as_ref()
        .map(|record| record.launch_id.clone());
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
        waiter,
        waiter_gate,
        agent_pid,
        agent_alive,
        supervisor,
        supervisor_launch_id,
        last_event: last_event_name,
        last_launched_stage,
        open_gate_stages,
        feature_branch_exists,
        stopped,
        stored_ship_answer,
        unprepared_restart,
        pending,
        live_agent,
        write_lock,
        write_lock_error,
    }
}

/// [`PhaseFacts::waiter`] and [`PhaseFacts::waiter_gate`]: `Live` and the gate, while
/// one of the phase's open gates has a held registration; else `Absent` and the
/// phase's first open gate, the one its findings name. The doctor reads this the way
/// a writer reads [`waiter_is_live`].
fn phase_gate_waiter(
    project_root: &Path,
    phase: PhaseId,
    open_gates: &[OpenGate],
) -> (WaiterStatus, Option<(Stage, String)>) {
    let mut phase_gates = open_gates.iter().filter(|gate| gate.phase == phase);
    let first = phase_gates.clone().next();
    let live = phase_gates.find(|gate| {
        GateId::parse(&gate.gate_id)
            .is_ok_and(|id| waiter_is_live(project_root, phase, gate.stage, &id))
    });
    let status = if live.is_some() {
        WaiterStatus::Live
    } else {
        WaiterStatus::Absent
    };
    let gate = live
        .or(first)
        .map(|gate| (gate.stage, gate.gate_id.clone()));
    (status, gate)
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

/// The heading `status` and `doctor` print over set-aside answers (D-05).
const SET_ASIDE_ANSWERS_HEADING: &str = "set-aside gate answers:";

/// One info finding per set-aside answer (D-05). The path is relative to the project
/// root: a finding never carries a username (T-18-01), and `doctor` already says which
/// project it reads.
fn set_aside_answer_findings(
    project_root: &Path,
    answers: &[QuarantinedAnswer],
) -> Vec<PhaseFinding> {
    answers
        .iter()
        .map(|answer| {
            let relative = answer
                .path
                .strip_prefix(project_root)
                .unwrap_or(&answer.path)
                .display();
            PhaseFinding {
                phase: answer.phase,
                severity: Severity::Info,
                detail: format!(
                    "phase {}: an unused answer for the {} gate (gate {}) was set aside: {relative}",
                    answer.phase, answer.stage, answer.label
                ),
                repair: Some(
                    "inspect it; answer the live gate with `devflow gate approve|reject`"
                        .to_string(),
                ),
            }
        })
        .collect()
}

/// `doctor`'s set-aside section: nothing when there are none.
fn render_set_aside_findings_text(findings: &[PhaseFinding]) -> String {
    if findings.is_empty() {
        return String::new();
    }
    let mut out = format!("\n{SET_ASIDE_ANSWERS_HEADING}\n");
    for finding in findings {
        out.push_str(&format!("  {}\n", finding.detail));
        if let Some(repair) = &finding.repair {
            out.push_str(&format!("    repair: {repair}\n"));
        }
    }
    out
}

/// `doctor --json`'s `"set_aside_gate_answers"` array.
fn render_set_aside_findings_json(findings: &[PhaseFinding]) -> serde_json::Value {
    serde_json::Value::Array(
        findings
            .iter()
            .map(|finding| {
                serde_json::json!({
                    "phase": finding.phase,
                    "severity": finding.severity.label(),
                    "detail": finding.detail,
                    "repair": finding.repair,
                })
            })
            .collect(),
    )
}

/// `status`'s block of set-aside answers, one group per phase (D-05), or `None` when there
/// are none. [`Gates::quarantined`] sorts by path, which keeps each phase's answers
/// together. `status` speaks only about the operator's own project, so it names full paths.
fn render_set_aside_answers(answers: &[QuarantinedAnswer]) -> Option<String> {
    if answers.is_empty() {
        return None;
    }
    let mut block = SET_ASIDE_ANSWERS_HEADING.to_string();
    for group in answers.chunk_by(|a, b| a.phase == b.phase) {
        block.push_str(&format!("\n  phase {}:", group[0].phase));
        for answer in group {
            block.push_str(&format!(
                "\n    {} gate (gate {}): {}",
                answer.stage,
                answer.label,
                answer.path.display()
            ));
        }
    }
    Some(block)
}

/// Build `doctor`'s per-phase reconciliation section (after the existing
/// tool/env checks), read-only: it never writes state or calls
/// `events::emit`, `Gates::close`/`Gates::open_gate`, or any `recover::clean*`
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
                    "supervisor": supervisor_json(phase_facts),
                    "gate_waiter": gate_waiter_json(phase_facts),
                })
            })
            .collect(),
    )
}

/// A phase's supervisor facts for `doctor --json` (50-12): its liveness
/// label, the owner and launch shape an ALIVE ping reported, the recorded
/// launch id, and the pending launch (`null`, or its id and liveness label, R2-01).
/// Never a socket or liveness-file path (T-18-01, T-50-32).
fn supervisor_json(facts: &PhaseFacts) -> serde_json::Value {
    let (owner, launch) = match &facts.supervisor {
        PhaseLiveness::Alive { owner, launch } => (Some(owner), Some(launch)),
        _ => (None, None),
    };
    serde_json::json!({
        "liveness": facts.supervisor.label(),
        "owner": owner,
        "launch": launch,
        "launch_id": facts.supervisor_launch_id,
        "pending": facts.pending.as_ref().map(|(launch_id, liveness)| {
            serde_json::json!({"launch_id": launch_id, "liveness": liveness.label()})
        }),
    })
}

/// A phase's open gate and its waiter for `doctor --json` (R2-01, D-04), the fact the
/// Phase 50 fault arm reads: `stage` and `gate_id` are `null` and `status` is `absent`
/// with no open gate; else the gate's stage label, its id, and `live` or `absent` from
/// its registration. Ids and labels only, never a path (T-18-01): the id comes from
/// the gate file unchecked, so one [`GateId::parse`] refuses reads `null`.
fn gate_waiter_json(facts: &PhaseFacts) -> serde_json::Value {
    let Some((stage, gate_id)) = &facts.waiter_gate else {
        return serde_json::json!({"stage": null, "gate_id": null, "status": "absent"});
    };
    let status = match facts.waiter {
        WaiterStatus::Live => "live",
        WaiterStatus::Absent => "absent",
    };
    serde_json::json!({
        "stage": stage.to_string(),
        "gate_id": GateId::parse(gate_id).is_ok().then_some(gate_id),
        "status": status,
    })
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
/// 50-12 adds `"orphan_supervisors"` (the socket sweep's findings) and
/// `"orphan_supervisor_sweep_error"` (`null`, or why the socket directory
/// could not be listed, so an empty array is never mistaken for a clean
/// sweep). D-05 adds `"set_aside_gate_answers"`, an empty array when there are none.
fn doctor_json_body(
    checks: &[Check],
    facts: &[PhaseFacts],
    doc_findings: &[PlanningDocFinding],
    stray_findings: &[StrayProcessFinding],
    (orphans, sweep_unlistable): (&[OrphanSupervisorFinding], Option<&str>),
    set_aside: &[PhaseFinding],
) -> serde_json::Value {
    serde_json::json!({
        "environment": checks_json_value(checks),
        "reconciliation": render_reconciliation_json(facts),
        "planning_doc_staleness": render_planning_doc_findings_json(doc_findings),
        "stray_processes": render_stray_process_findings_json(stray_findings),
        "orphan_supervisors": render_orphan_supervisors_json(orphans),
        "orphan_supervisor_sweep_error": sweep_unlistable,
        "set_aside_gate_answers": render_set_aside_findings_json(set_aside),
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

/// The set of pids that a live registry entry still reaches: the holder of every held
/// launch-liveness file, gate-registration file and phase holder file (D-17) under
/// `roots`. A pid in this set is by definition not a stray (CR-01, 25-15-PLAN.md): it
/// holds one of those files now. An unheld file's pid is not in it.
///
/// A 3.x supervisor is covered through its liveness file, though it needs no entry: the
/// `/proc` census this set filters ([`agent::discover_stray_devflow_processes`]) matches
/// only the 2.x monitor-wrapper and `devflow advance` argv shapes and cannot match a
/// supervisor's (50-12, pinned by
/// `agent::tests::the_stray_census_matches_only_the_2x_process_shapes` and by
/// `census_never_sees_a_live_supervisor` in the supervisor e2e), so a supervisor's pid can
/// never reach a reap through this filter.
///
/// Deliberately excludes the per-phase agent pid file: the `/proc` census
/// this set filters ([`agent::discover_stray_devflow_processes`]) can only
/// ever produce a monitor-wrapper or `devflow advance` pid, never a
/// `claude`/`codex` agent process's own pid, so adding the agent pid file
/// would widen the safety set with pids the census structurally cannot
/// produce.
///
/// Read-only, always: it probes and reads these files and never writes one, since it also
/// runs on `doctor`'s strictly read-only path.
///
/// RESIDUAL (T-25-15-08, not eliminated): this is a point-in-time read. A
/// root registered, or a file taken, between this read and a caller's later
/// signal is not covered by it. The untouched
/// `agent::is_same_process` identity re-confirmation and
/// [`agent::STRAY_MIN_AGE`] age floor downstream remain the last line of
/// defence in that window.
pub(crate) fn registry_reachable_pids(roots: &[PathBuf]) -> HashSet<u32> {
    let mut reachable = HashSet::new();
    let mut scanned = HashSet::new();
    for root in roots {
        if !scanned.insert(root.clone()) {
            // One root with N phases yields N registry entries, and one pass over its
            // `.devflow/` already covers every phase.
            continue;
        }
        reachable.extend(held_launch_and_phase_holders(root));
        reachable.extend(
            Gates::live_waiter_holders(root)
                .into_iter()
                .map(|(pid, _start_time)| pid),
        );
    }
    reachable
}

/// The pid recorded by each held `.devflow/supervisor-*.live` and `.devflow/.holder-*`
/// under `root`. A file whose record cannot be read is reported and left out.
fn held_launch_and_phase_holders(root: &Path) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir(root.join(".devflow")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            (name.starts_with("supervisor-") && name.ends_with(".live"))
                || name.starts_with(".holder-")
        })
        .filter_map(|entry| match liveness::live_holder(&entry.path()) {
            Ok(holder) => holder.map(|(pid, _start_time)| pid),
            Err(error) => {
                eprintln!(
                    "warning: could not read {}: {error}",
                    entry.path().display()
                );
                None
            }
        })
        .collect()
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
/// that no registry entry, held file, or state file can reach — that
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
                     argv shape, is owned by the calling user, and is named by no registered \
                     project root's state file and held by none of its live liveness, \
                     gate-registration or foreground-holder files",
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
///
/// The census matches only the two 2.x process shapes (the `sh -c` monitor
/// wrapper and its `devflow advance` child), which a 3.x binary never
/// starts, so this finding covers 2.x leftovers only (50-12). A 3.x
/// supervisor is found through its socket by [`check_supervisor`] and
/// [`collect_orphan_supervisors`] (which also takes over the deleted-root
/// class) and ended with `devflow stop --phase N --launch <id>`.
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

/// One supervisor socket `doctor`'s read-only sweep reports (50-12, D-06).
pub(crate) struct OrphanSupervisorFinding {
    /// The path-free finding: detail and `devflow stop --phase N --launch
    /// <id>` repair.
    pub(crate) finding: PhaseFinding,
    /// The launch id that addresses exactly this socket.
    pub(crate) launch_id: String,
    /// `alive` or `stale`.
    pub(crate) liveness: &'static str,
    /// Whether the project root the socket names still exists. `true` for a
    /// sidecar-less socket, which is attributed to this (existing) root.
    pub(crate) root_exists: bool,
}

/// Every ALIVE or STALE supervisor socket under `cache_dir` that nothing
/// claims (50-12, D-06): (1) this root's sockets whose phase has no loadable
/// state, or whose state records no supervisor or another launch
/// (superseded) and does not name it as pending; (2) machine-wide, sockets whose sidecar names a project
/// root that no longer exists (the 999.44 deleted-root class the 2.x `/proc`
/// census used to cover); (3) (R5-I3) sockets of this root whose sidecar is
/// missing or does not parse, attributed by file name through
/// [`supervisor::sweep_for_in`] so 50-06's digest rule stays in one place.
///
/// Not listed, by design: a socket named outside the 50-05 scheme (it
/// carries no launch id, so no verb addresses it), and a sidecar-less socket
/// of a deleted root (only the digest survives, so nothing attributes it).
///
/// Read-only (D-06, T-50-31): it sweeps, probes and loads state, and removes
/// nothing and signals nothing. Every finding's repair is `devflow stop
/// --phase N --launch <id>` (50-14), never the bare stop, which addresses
/// the state's own record and would stop a live successor (T-50-51).
fn collect_orphan_supervisors(
    project_root: &Path,
    cache_dir: &Path,
) -> Vec<OrphanSupervisorFinding> {
    let mut findings = Vec::new();
    for entry in supervisor::sweep_in(cache_dir) {
        let liveness = match &entry.liveness {
            Ok(supervisor::Liveness::Alive(_)) => "alive",
            Ok(supervisor::Liveness::Stale) => "stale",
            _ => continue,
        };
        if let Some(sidecar) = &entry.sidecar {
            // A root that cannot be examined counts as present: only a
            // proven-missing root is the deleted-root class.
            let root_exists = sidecar.project_root.try_exists().unwrap_or(true);
            let reason = if !root_exists {
                Some(OrphanReason::RootDeleted)
            } else if sidecar.project_root == project_root {
                unclaimed_by_state(project_root, sidecar.phase, &sidecar.launch_id)
            } else {
                None
            };
            if let Some(reason) = reason {
                let launch = (sidecar.phase, sidecar.launch_id.as_str());
                findings.push(orphan_finding(launch, liveness, root_exists, &reason));
            }
            continue;
        }
        // R5-I3: no usable sidecar, so the file name is the only address.
        let (Some(launch_id), Some(phase)) = (
            entry.launch_id_in_name.as_deref(),
            phase_in_socket_name(&entry.socket_path),
        ) else {
            continue;
        };
        let ours = supervisor::sweep_for_in(cache_dir, project_root, phase)
            .iter()
            .any(|candidate| candidate.socket_path == entry.socket_path);
        let Some(unclaimed) = ours
            .then(|| unclaimed_by_state(project_root, phase, launch_id))
            .flatten()
        else {
            continue;
        };
        let reason = OrphanReason::NoUsableSidecar {
            broken: entry.sidecar_error.is_some(),
            unclaimed: Box::new(unclaimed),
        };
        findings.push(orphan_finding((phase, launch_id), liveness, true, &reason));
    }
    findings
}

/// Why [`collect_orphan_supervisors`] lists a socket.
enum OrphanReason {
    /// This root's phase has no state file.
    NoState,
    /// This root's phase has a state file that does not load.
    StateUnloadable,
    /// The phase's state records no supervisor.
    NoRecord,
    /// The phase's state records another launch (the R-10 aftermath).
    Superseded,
    /// The sidecar names a project root that no longer exists.
    RootDeleted,
    /// The sidecar is missing (`broken: false`) or does not parse, and the
    /// state does not claim the launch for `unclaimed`'s reason.
    NoUsableSidecar {
        broken: bool,
        unclaimed: Box<OrphanReason>,
    },
}

impl OrphanReason {
    /// Path-free text for the finding's detail (T-18-01).
    fn describe(&self) -> String {
        match self {
            OrphanReason::NoState => "its phase has no state".to_string(),
            OrphanReason::StateUnloadable => "its phase's state does not load".to_string(),
            OrphanReason::NoRecord => "its phase's state records no supervisor".to_string(),
            OrphanReason::Superseded => {
                "its phase's state records another launch (superseded)".to_string()
            }
            OrphanReason::RootDeleted => "its project root no longer exists".to_string(),
            OrphanReason::NoUsableSidecar { broken, unclaimed } => format!(
                "its sidecar {}, and {}",
                if *broken {
                    "does not parse"
                } else {
                    "is missing"
                },
                unclaimed.describe()
            ),
        }
    }
}

/// Whether this root's state for `phase` claims `launch_id`; the reason
/// when it does not. A launch the state names as pending is claimed, since it may
/// be the next stage's supervisor before its promotion (D-01). Reads the state file
/// only.
fn unclaimed_by_state(
    project_root: &Path,
    phase: PhaseId,
    launch_id: &str,
) -> Option<OrphanReason> {
    match workflow::load_state(project_root, phase) {
        Err(workflow::WorkflowError::MissingState(_)) => Some(OrphanReason::NoState),
        Err(_) => Some(OrphanReason::StateUnloadable),
        Ok(state)
            if state
                .pending_supervisor
                .as_ref()
                .is_some_and(|pending| pending.launch_id == launch_id) =>
        {
            None
        }
        Ok(state) => match state.supervisor {
            None => Some(OrphanReason::NoRecord),
            Some(record) if record.launch_id != launch_id => Some(OrphanReason::Superseded),
            Some(_) => None,
        },
    }
}

/// The phase segment of a socket file name in the 50-05 scheme
/// (`<016x digest>-<padded phase>-<launch id>.sock`): the text between the
/// first and second `-`. `None` when it does not parse as a phase.
fn phase_in_socket_name(socket: &Path) -> Option<PhaseId> {
    let stem = socket.file_stem()?.to_str()?;
    stem.split('-').nth(1)?.parse().ok()
}

/// One orphan finding for `(phase, launch_id)`, path-free, with the
/// `--launch` repair that tears down exactly that socket.
fn orphan_finding(
    (phase, launch_id): (PhaseId, &str),
    liveness: &'static str,
    root_exists: bool,
    reason: &OrphanReason,
) -> OrphanSupervisorFinding {
    OrphanSupervisorFinding {
        finding: PhaseFinding {
            phase,
            severity: Severity::Problem,
            detail: format!(
                "phase {phase}: supervisor launch {launch_id} is {} but {}",
                liveness.to_uppercase(),
                reason.describe()
            ),
            repair: Some(format!("devflow stop --phase {phase} --launch {launch_id}")),
        },
        launch_id: launch_id.to_string(),
        liveness,
        root_exists,
    }
}

/// Whether doctor's socket sweep could look (50-12): `None` when the socket
/// directory is absent or listable, else why it cannot be listed.
/// [`supervisor::sweep_in`] answers an unlistable directory with no entries,
/// so without this check doctor would report "nothing" when it saw nothing.
fn socket_dir_unlistable(cache_dir: &Path) -> Option<String> {
    match std::fs::read_dir(registry::socket_dir_in(cache_dir)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Some(error.kind().to_string())
        }
        _ => None,
    }
}

/// Build `doctor --json`'s `"orphan_supervisors"` array (50-12).
fn render_orphan_supervisors_json(findings: &[OrphanSupervisorFinding]) -> serde_json::Value {
    serde_json::Value::Array(
        findings
            .iter()
            .map(|orphan| {
                serde_json::json!({
                    "phase": orphan.finding.phase,
                    "launch_id": orphan.launch_id,
                    "liveness": orphan.liveness,
                    "root_exists": orphan.root_exists,
                    "severity": orphan.finding.severity.label(),
                    "detail": orphan.finding.detail,
                    "repair": orphan.finding.repair,
                })
            })
            .collect(),
    )
}

/// Build `doctor`'s text section for the socket sweep (50-12). Like
/// [`render_stray_process_text`] it prints nothing when there is nothing to
/// report, but a socket directory it could not list is reported, never
/// shown as "nothing".
fn render_orphan_supervisors_text(
    findings: &[OrphanSupervisorFinding],
    unlistable: Option<&str>,
) -> String {
    if findings.is_empty() && unlistable.is_none() {
        return String::new();
    }
    let mut out = String::from("\nsupervisor sockets no state claims:\n");
    if let Some(why) = unlistable {
        out.push_str(&format!(
            "  warn: the socket directory could not be listed ({why}); its sockets are not \
             reported\n"
        ));
    }
    for orphan in findings {
        out.push_str(&format!("  {}\n", orphan.finding.detail));
        if let Some(repair) = &orphan.finding.repair {
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
    use devflow_core::gates::WaiterRegistration;
    use devflow_core::test_support::{gate_answer_files, with_answer_before_publish};

    #[test]
    fn gate_response_message_does_not_claim_pickup_after_the_waiter_goes() {
        let live = response_pickup_message(Stage::Code, WaiterStatus::Live);
        assert!(live.contains("may pick up"), "live message: {live}");
        assert!(!live.contains("lock"), "live message: {live}");
        let gone = response_pickup_message(Stage::Code, WaiterStatus::Absent);
        assert!(
            gone.contains("no confirmed waiter"),
            "no-holder message: {gone}"
        );
        assert!(
            !gone.contains("will pick this up"),
            "no-holder message: {gone}"
        );
    }

    /// [`stop`] for a root with no state file, whose socket search would
    /// otherwise sweep the operator's real cache: an empty tempdir cache.
    fn stop_with_an_empty_cache(root: &Path, phase: PhaseId) -> Result<(), CliError> {
        let cache = tempfile::tempdir().unwrap();
        stop_in(root, phase, Some(cache.path()))
    }

    #[test]
    fn stop_via_gate_with_no_waiter_at_a_non_ship_gate_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4801);
        Gates::write_gate(root, phase, Stage::Code, "paused").unwrap();
        assert!(stop_via_gate(root, phase).unwrap().is_none());
        assert!(gate_answer_files(root, phase, Stage::Code).is_empty());
    }

    #[test]
    fn stop_via_gate_with_a_live_waiter_writes_the_rejection() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4805);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.gate_pending = true;
        workflow::save_state(&state).unwrap();
        let (_, registration) = Gates::open_registered(root, phase, Stage::Code, "paused").unwrap();
        assert!(stop_via_gate(root, phase).unwrap().is_some());
        assert!(!gate_answer_files(root, phase, Stage::Code).is_empty());
        registration.finish();
    }

    #[test]
    fn gate_sweep_leaves_a_no_waiter_non_ship_gate_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4802);
        Gates::write_gate(root, phase, Stage::Code, "paused").unwrap();
        gate_sweep(Some(0), false, Some(root.to_path_buf()), false).unwrap();
        assert!(gate_answer_files(root, phase, Stage::Code).is_empty());
    }

    #[test]
    fn gate_sweep_preview_uses_the_no_waiter_matrix() {
        for stage in [Stage::Code, Stage::Ship] {
            assert!(!gate_sweep_may_reap(stage, false), "{stage} with no waiter");
            assert!(gate_sweep_may_reap(stage, true), "{stage} with a waiter");
        }
    }

    #[test]
    fn gate_approve_with_no_waiter_at_the_ship_gate_writes_and_names_ship() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4803);
        Gates::write_gate(root, phase, Stage::Ship, "merge").unwrap();
        gate_respond(root, phase, Some(Stage::Ship), true, None, None).unwrap();
        assert!(!gate_answer_files(root, phase, Stage::Ship).is_empty());
        assert!(no_waiter_repair(phase, Stage::Ship).contains("devflow ship --phase 4803"));
    }

    #[test]
    fn gate_respond_with_no_registered_waiter_at_a_non_ship_gate_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4804);
        Gates::write_gate(root, phase, Stage::Code, "paused").unwrap();
        let err = gate_respond(
            root,
            phase,
            Some(Stage::Code),
            false,
            Some("abort".into()),
            None,
        )
        .expect_err("an unregistered non-Ship gate must not receive a response");
        assert!(
            err.to_string().contains("devflow resume --phase 4804"),
            "repair must name the concrete phase: {err}"
        );
        assert!(gate_answer_files(root, phase, Stage::Code).is_empty());
    }

    /// D-04: with no registered waiter, a Ship abort is stored for `devflow ship`.
    #[test]
    fn stop_at_a_ship_gate_without_a_holder_keeps_manual_recovery_available() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4810);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        state.gate_pending = true;
        workflow::save_state(&state).unwrap();
        Gates::write_gate(root, phase, Stage::Ship, "merge").unwrap();

        stop(root, phase).expect("Ship with no holder may retain its abort response");

        assert!(
            !gate_answer_files(root, phase, Stage::Ship).is_empty(),
            "the manual-recovery Ship exception must remain available"
        );
        assert!(
            workflow::load_state(root, phase).unwrap().stopped,
            "the stop request itself is still recorded"
        );
    }

    #[test]
    fn stop_at_a_non_ship_gate_with_an_unconfirmable_holder_refuses_success() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4807);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.gate_pending = true;
        workflow::save_state(&state).unwrap();
        Gates::write_gate(root, phase, Stage::Code, "paused").unwrap();
        // Not the test process: a broken identity check signals this child, not the runner.
        let (mut holder, _) = sleep_child_with_start();
        let _hold = hold_phase_with_record(root, phase, holder.id(), None);
        let result = stop(root, phase);
        let holder_alive = devflow_core::test_support::process_is_live(holder.id());
        let _ = holder.kill();
        let _ = holder.wait();
        let error = result.unwrap_err().to_string();
        assert!(holder_alive, "a record with no start time signals nothing");
        assert!(gate_answer_files(root, phase, Stage::Code).is_empty());
        assert!(error.contains("refusing to signal"), "{error}");
    }

    #[test]
    fn existing_response_stops_only_for_an_abort_with_a_registered_waiter() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4809);
        let stage = Stage::Code;

        // A new gate per case: a gate keeps its first answer.
        let answered_gate = |approved, note: Option<&str>| {
            let gate = Gates::open_gate(root, phase, stage, "g").unwrap().gate_id;
            let response = GateResponse {
                approved,
                note: note.map(str::to_owned),
                responded_by: Some("test".into()),
            };
            Gates::answer(root, phase, stage, &gate, &response).unwrap();
            gate
        };

        let approved = answered_gate(true, None);
        let registration = Gates::register_waiter(root, phase, stage, &approved).unwrap();
        let non_abort = stop_with_existing_response(root, phase, stage, &approved, None).unwrap();
        assert_eq!(non_abort, None);
        registration.finish();

        let no_waiter = answered_gate(false, Some("abort"));
        let unregistered =
            stop_with_existing_response(root, phase, stage, &no_waiter, None).unwrap();
        assert_eq!(unregistered, None, "nothing on disk is no waiter");

        let live = answered_gate(false, Some("abort"));
        let registration = Gates::register_waiter(root, phase, stage, &live).unwrap();
        let live_abort =
            stop_with_existing_response(root, phase, stage, &live, Some("launch-a".to_string()))
                .unwrap();
        assert_eq!(
            live_abort,
            Some((stage, live.clone(), Some("launch-a".to_string()), false))
        );
        registration.finish();
    }

    /// State at `stage` with its gate pending, the shape `stop` treats as current.
    fn state_pending_at(root: &Path, phase: PhaseId, stage: Stage) {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = stage;
        state.gate_pending = true;
        workflow::save_state(&state).unwrap();
    }

    /// The open gate of `phase` as `Gates::list_open` shows it, as a writer holds
    /// it after listing.
    fn listed_gate(root: &Path, phase: PhaseId) -> OpenGate {
        Gates::list_open(root)
            .into_iter()
            .find(|gate| gate.phase == phase)
            .expect("the gate is listed")
    }

    #[test]
    fn gate_sweep_reaps_only_the_observed_gate_id() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(4911), Stage::Code);
        Gates::open_gate(root, phase, stage, "g1").unwrap();
        let g1_entry = listed_gate(root, phase);
        Gates::close(root, phase, stage).unwrap();
        let (g2, registration) = Gates::open_registered(root, phase, stage, "g2").unwrap();
        let g2 = g2.gate_id;

        assert_eq!(
            sweep_one_gate(root, &g1_entry, 1, 0, false).unwrap(),
            SweepStep::Skipped,
            "a gate changed since it was listed is reported, not left as waiterless"
        );
        assert_eq!(
            Gates::read_answer(root, phase, stage, &g2),
            AnswerRead::None
        );
        assert_eq!(listed_gate(root, phase).gate_id, g2.as_str());

        let g2_entry = listed_gate(root, phase);
        assert_eq!(
            sweep_one_gate(root, &g2_entry, 1, 0, false).unwrap(),
            SweepStep::Reaped
        );
        let AnswerRead::Bound(answer) = Gates::read_answer(root, phase, stage, &g2) else {
            panic!("the sweep answers the gate it observed");
        };
        assert!(!answer.response.approved, "a reap never approves (T-23-41)");
        registration.finish();
    }

    /// A gate file naming an empty id can be reported, and nothing else: no answer
    /// can bind to it. Both writers skip it before reading liveness.
    #[test]
    fn a_gate_with_no_gate_id_is_reported_and_never_answered() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(4912), Stage::Code);
        state_pending_at(root, phase, stage);
        Gates::open_gate(root, phase, stage, "g").unwrap();
        let mut entry = listed_gate(root, phase);
        entry.gate_id = String::new();
        let file = GateFile {
            phase,
            stage,
            context: entry.context.clone(),
            timestamp: entry.timestamp.clone(),
            gate_id: String::new(),
        };
        std::fs::write(
            Gates::gate_path(root, phase, stage),
            serde_json::to_string_pretty(&file).unwrap(),
        )
        .unwrap();

        assert_eq!(
            sweep_one_gate(root, &entry, 1, 0, false).unwrap(),
            SweepStep::Skipped
        );
        assert_eq!(stop_reap_observed_gate(root, phase, &entry).unwrap(), None);
        assert!(gate_answer_files(root, phase, stage).is_empty());
    }

    #[test]
    fn stop_answers_only_the_observed_gate_id() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(4913), Stage::Code);
        state_pending_at(root, phase, stage);
        Gates::open_gate(root, phase, stage, "g1").unwrap();
        let g1_entry = listed_gate(root, phase);
        Gates::close(root, phase, stage).unwrap();
        let (g2, registration) = Gates::open_registered(root, phase, stage, "g2").unwrap();
        let g2 = g2.gate_id;

        assert_eq!(
            stop_reap_observed_gate(root, phase, &g1_entry).unwrap(),
            None,
            "a stale entry falls through to the socket path"
        );
        assert_eq!(
            Gates::read_answer(root, phase, stage, &g2),
            AnswerRead::None
        );

        // Control: with G2's own entry the same waiter is answered, so the refusal
        // above was the id comparison, not a missing waiter.
        let g2_entry = listed_gate(root, phase);
        assert!(
            stop_reap_observed_gate(root, phase, &g2_entry)
                .unwrap()
                .is_some()
        );
        let AnswerRead::Bound(answer) = Gates::read_answer(root, phase, stage, &g2) else {
            panic!("stop answers the gate it observed");
        };
        assert!(matches!(
            GateAction::from_response(&answer.response),
            GateAction::Abort(_)
        ));
        registration.finish();
    }

    #[test]
    fn stop_reads_an_existing_answer_for_the_observed_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(4914), Stage::Code);
        state_pending_at(root, phase, stage);
        let answer_with = |approved, note: &str| {
            let (opened, registration) = Gates::open_registered(root, phase, stage, "g").unwrap();
            let entry = listed_gate(root, phase);
            let response = GateResponse {
                approved,
                note: Some(note.to_owned()),
                responded_by: Some("human".into()),
            };
            Gates::answer(root, phase, stage, &opened.gate_id, &response).unwrap();
            (opened.gate_id, entry, registration)
        };

        let (gate, entry, registration) = answer_with(false, "abort: by a human");
        let answered = stop_reap_observed_gate(root, phase, &entry).unwrap();
        assert_eq!(
            answered,
            Some((stage, gate.clone(), None, false)),
            "a live abort is the requested stop"
        );
        let AnswerRead::Bound(kept) = Gates::read_answer(root, phase, stage, &gate) else {
            panic!("the first answer stays");
        };
        assert_eq!(kept.response.responded_by.as_deref(), Some("human"));
        registration.finish();
        Gates::close(root, phase, stage).unwrap();

        let (gate, entry, registration) = answer_with(true, "approved by a human");
        assert_eq!(
            stop_reap_observed_gate(root, phase, &entry).unwrap(),
            None,
            "an approval is not stop's outcome"
        );
        assert!(matches!(
            Gates::read_answer(root, phase, stage, &gate),
            AnswerRead::Bound(kept) if kept.response.approved
        ));
        registration.finish();
    }

    /// Open G1, close it, and open G2 at the same stage with a held registration:
    /// the shape in which a writer that observed G1 could answer G2.
    fn g1_replaced_by_registered_g2(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
    ) -> (GateId, GateId, WaiterRegistration) {
        let g1 = Gates::open_gate(root, phase, stage, "g1").unwrap().gate_id;
        Gates::close(root, phase, stage).unwrap();
        let (g2, registration) = Gates::open_registered(root, phase, stage, "g2").unwrap();
        (g1, g2.gate_id, registration)
    }

    #[test]
    fn gate_respond_publishes_only_for_the_observed_gate_id() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(4901), Stage::Code);
        let (g1, g2, registration) = g1_replaced_by_registered_g2(root, phase, stage);

        let err = gate_respond(root, phase, Some(stage), true, None, Some(g1.to_string()))
            .expect_err("an answer about G1 must not land while G2 is open")
            .to_string();

        assert!(
            err.contains(g1.as_str()) && err.contains(g2.as_str()),
            "the refusal must name both gates: {err}"
        );
        assert!(err.contains("devflow gate list"), "{err}");
        assert!(
            !err.contains("no registered waiter"),
            "a changed gate must not read as a missing waiter: {err}"
        );
        assert_eq!(
            Gates::read_answer(root, phase, stage, &g2),
            AnswerRead::None
        );
        assert!(!Gates::answer_path(root, phase, stage, &g1).exists());

        gate_respond(root, phase, Some(stage), true, None, None).unwrap();

        let AnswerRead::Bound(answer) = Gates::read_answer(root, phase, stage, &g2) else {
            panic!("with no id given, the command answers the gate open now");
        };
        assert_eq!(answer.gate_id, g2.as_str());
        assert!(answer.response.approved);
        registration.finish();
    }

    /// T-50.1-42: a malformed `--gate-id` is refused before it can name a path.
    #[test]
    fn gate_respond_refuses_a_malformed_gate_id_before_touching_anything() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(4902), Stage::Code);
        let (opened, registration) = Gates::open_registered(root, phase, stage, "g").unwrap();

        let err = gate_respond(
            root,
            phase,
            Some(stage),
            true,
            None,
            Some("../../etc".into()),
        )
        .expect_err("a malformed gate id must be refused")
        .to_string();

        assert!(err.contains("is not a gate id"), "{err}");
        assert_eq!(
            Gates::read_answer(root, phase, stage, &opened.gate_id),
            AnswerRead::None
        );
        assert!(gate_answer_files(root, phase, stage).is_empty());
        registration.finish();
    }

    /// The window this mapping covers, the gate changing between `gate_respond`'s
    /// comparison and `Gates::answer`'s re-check, has no seam, so the mapping is
    /// tested on its own.
    #[test]
    fn a_gate_changed_at_publish_is_reported_like_one_changed_before_it() {
        let message = gate_answer_error(GateError::GateIdMismatch {
            phase: PhaseId::new(4907),
            stage: Stage::Code,
            observed: "g1".into(),
            current: "g2".into(),
        })
        .to_string();

        assert_eq!(
            message,
            gate_changed_message(PhaseId::new(4907), Stage::Code, "g1", "g2")
        );
        assert!(message.contains("nothing was written"), "{message}");
    }

    #[test]
    fn gate_list_and_show_print_the_gate_id() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let opened = Gates::open_gate(root, PhaseId::new(4903), Stage::Ship, "ctx").unwrap();
        let open = Gates::list_open(root);

        let list = render_gate_list(&open);
        let show = render_gate_show(&open[0]);

        assert!(list.contains(opened.gate_id.as_str()), "{list}");
        assert!(
            show.contains(&format!("gate id: {}", opened.gate_id)),
            "{show}"
        );
    }

    #[test]
    fn doctor_facts_read_a_registered_waiter_as_the_live_holder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(4906), Stage::Code);
        let (_, registration) = Gates::open_registered(root, phase, stage, "g").unwrap();
        let open = Gates::list_open(root);

        assert_eq!(phase_gate_waiter(root, phase, &open).0, WaiterStatus::Live);
        registration.finish();
        assert_eq!(
            phase_gate_waiter(root, phase, &open).0,
            WaiterStatus::Absent
        );
    }

    /// A process recorded in a per-phase lock file, as no waiter records itself any more.
    fn plant_live_lock_record(root: &Path, phase: PhaseId) {
        let pid = std::process::id();
        let start = agent::process_start_time(pid).unwrap();
        let path = root
            .join(".devflow")
            .join(format!("lock-{}", phase.padded()));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("{pid}\n{start}")).unwrap();
    }

    /// D-04: a registered waiter's gate is answered with no lock file anywhere, and with
    /// none registered it is refused even beside a live lock record. A Ship answer is
    /// still stored for `devflow ship`.
    #[test]
    fn gate_respond_answers_a_registered_waiter_and_refuses_when_none_is_registered() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let (registered, stage) = (PhaseId::new(5001), Stage::Code);
        let (opened, registration) = Gates::open_registered(root, registered, stage, "g").unwrap();
        gate_respond(root, registered, Some(stage), true, None, None)
            .expect("a registered waiter's gate is answered");
        assert!(
            matches!(
                Gates::read_answer(root, registered, stage, &opened.gate_id),
                AnswerRead::Bound(answer) if answer.response.approved
            ),
            "the answer binds to the observed gate"
        );
        assert!(
            !root
                .join(".devflow")
                .join(format!("lock-{}", registered.padded()))
                .exists(),
            "no lock file played any part"
        );
        registration.finish();

        let beside_a_lock = PhaseId::new(5003);
        Gates::open_gate(root, beside_a_lock, stage, "g").unwrap();
        plant_live_lock_record(root, beside_a_lock);
        let refused = gate_respond(root, beside_a_lock, Some(stage), true, None, None)
            .expect_err("a live lock record is no waiter")
            .to_string();
        assert!(refused.contains("no registered waiter"), "{refused}");
        assert!(gate_answer_files(root, beside_a_lock, stage).is_empty());

        let released = PhaseId::new(5002);
        let (_, registration) = Gates::open_registered(root, released, stage, "g").unwrap();
        registration.finish();
        let refused = gate_respond(root, released, Some(stage), true, None, None)
            .expect_err("a finished registration is no waiter")
            .to_string();
        assert!(refused.contains("no registered waiter"), "{refused}");
        assert!(refused.contains("no response was written"), "{refused}");
        assert!(gate_answer_files(root, released, stage).is_empty());

        let ship = PhaseId::new(5004);
        let gate = Gates::open_gate(root, ship, Stage::Ship, "g")
            .unwrap()
            .gate_id;
        gate_respond(root, ship, Some(Stage::Ship), true, None, None)
            .expect("a Ship answer is stored for `devflow ship`");
        assert!(matches!(
            Gates::read_answer(root, ship, Stage::Ship, &gate),
            AnswerRead::Bound(answer) if answer.response.approved
        ));
        assert!(no_waiter_repair(ship, Stage::Ship).contains("devflow ship --phase 5004"));
    }

    /// Hold `phase`'s gate lock through a separate open, as a suspended writer would.
    fn hold_gate_lock(root: &Path, phase: PhaseId) -> std::fs::File {
        let path = root
            .join(".devflow")
            .join(format!(".gates-{}.lock", phase.padded()));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .unwrap();
        file.lock().unwrap();
        file
    }

    /// Whether `child` ends within `within`, reaping it if it does.
    fn child_exits_within(child: &mut std::process::Child, within: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + within;
        while std::time::Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        child.try_wait().unwrap().is_some()
    }

    /// A `sleep` child that is killed and reaped when this drops, on a failed assertion too.
    struct SleepingChild {
        child: std::process::Child,
        start: u64,
    }

    impl SleepingChild {
        fn spawn() -> Self {
            let (child, start) = sleep_child_with_start();
            Self { child, start }
        }

        fn pid(&self) -> u32 {
            self.child.id()
        }

        fn is_live(&self) -> bool {
            devflow_core::test_support::process_is_live(self.pid())
        }
    }

    impl Drop for SleepingChild {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// `GateLockTimeout`: a gate writer gives up within the bound and writes nothing,
    /// and is never told there is no waiter. `stop` goes on to the arms that reach the
    /// run without the gate. Control: with the lock free, the same answer lands.
    #[test]
    fn a_gate_writer_gives_up_on_a_held_gate_lock_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let stage = Stage::Code;

        let phase = PhaseId::new(5005);
        let (opened, registration) = Gates::open_registered(root, phase, stage, "g").unwrap();
        let gate_lock = hold_gate_lock(root, phase);
        let started = std::time::Instant::now();
        let error = gate_respond(
            root,
            phase,
            Some(stage),
            true,
            None,
            Some(opened.gate_id.to_string()),
        )
        .expect_err("a held gate lock ends the writer");
        let took = started.elapsed();
        assert!(
            took >= devflow_core::gates::GATE_LOCK_WAIT,
            "gave up after {took:?}"
        );
        assert!(
            took < devflow_core::gates::GATE_LOCK_WAIT + std::time::Duration::from_secs(2),
            "waited {took:?}"
        );
        let message = error.to_string();
        assert!(
            matches!(error, CliError::Gate(GateError::GateLockTimeout { .. })),
            "{message}"
        );
        assert!(message.contains(".gates-"), "{message}");
        assert!(message.contains("nothing was changed"), "{message}");
        assert!(gate_answer_files(root, phase, stage).is_empty());

        let stopping = PhaseId::new(5006);
        state_pending_at(root, stopping, stage);
        let (_, stop_registration) = Gates::open_registered(root, stopping, stage, "g").unwrap();
        let stop_gate_lock = hold_gate_lock(root, stopping);
        let mut holder = SleepingChild::spawn();
        let _hold = hold_phase_with_record(root, stopping, holder.pid(), Some(holder.start));
        let cache = tempfile::tempdir().unwrap();
        let stopped = stop_in(root, stopping, Some(cache.path()));
        assert!(
            child_exits_within(&mut holder.child, agent::TERMINATE_VERIFY_WAIT),
            "stop reaches the holder past a gate it could not answer: {stopped:?}"
        );
        assert!(stopped.is_err(), "the test's hold outlives its child");
        assert!(
            gate_answer_files(root, stopping, stage).is_empty(),
            "stop wrote no answer under the held lock"
        );
        stop_registration.finish();
        drop(stop_gate_lock);

        drop(gate_lock);
        gate_respond(
            root,
            phase,
            Some(stage),
            true,
            None,
            Some(opened.gate_id.to_string()),
        )
        .expect("with the lock free the same answer lands");
        assert!(matches!(
            Gates::read_answer(root, phase, stage, &opened.gate_id),
            AnswerRead::Bound(_)
        ));
        registration.finish();
    }

    /// Wait, boundedly, for `path` to read unheld: another test's fork keeps a released
    /// flock held until its exec.
    fn wait_until_unheld(path: &Path) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while matches!(liveness::probe(path), Ok(liveness::Held::Live)) {
            assert!(
                std::time::Instant::now() < deadline,
                "{} still reads held",
                path.display()
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// D-04, T-50.1-67/68: a registered waiter's aged gate is reaped, and a crashed
    /// waiter's, whose registration file is there but unheld, is left alone.
    #[test]
    fn gate_sweep_reaps_only_a_gate_with_a_registered_waiter() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let stage = Stage::Code;
        let (live, crashed) = (PhaseId::new(5011), PhaseId::new(5012));
        let (live_gate, registration) = Gates::open_registered(root, live, stage, "g").unwrap();
        let (crashed_gate, crashed_registration) =
            Gates::open_registered(root, crashed, stage, "g").unwrap();
        drop(crashed_registration);
        let crashed_file = Gates::waiter_path(root, crashed, stage, &crashed_gate.gate_id);
        wait_until_unheld(&crashed_file);
        assert!(crashed_file.exists(), "a crash leaves its file behind");

        gate_sweep(Some(0), false, Some(root.to_path_buf()), false).unwrap();

        assert!(
            matches!(
                Gates::read_answer(root, live, stage, &live_gate.gate_id),
                AnswerRead::Bound(answer) if !answer.response.approved
            ),
            "a registered waiter's gate is reaped with an abort"
        );
        assert_eq!(
            Gates::read_answer(root, crashed, stage, &crashed_gate.gate_id),
            AnswerRead::None,
            "a crashed waiter's gate is reported, not reaped"
        );
        registration.finish();
    }

    /// D-04, T-50.1-69: a foreground waiter is reached by its gate, and no process is
    /// signalled: the holder arm never runs once the gate arm has answered.
    #[test]
    fn stop_answers_a_gate_whose_waiter_is_registered() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(5013), Stage::Code);
        state_pending_at(root, phase, stage);
        let (opened, registration) = Gates::open_registered(root, phase, stage, "g").unwrap();
        let holder = SleepingChild::spawn();
        let _hold = hold_phase_with_record(root, phase, holder.pid(), Some(holder.start));
        let cache = tempfile::tempdir().unwrap();

        stop_in(root, phase, Some(cache.path())).expect("the answered gate is the stop");

        let AnswerRead::Bound(answer) = Gates::read_answer(root, phase, stage, &opened.gate_id)
        else {
            panic!("stop answers the observed gate");
        };
        assert!(matches!(
            GateAction::from_response(&answer.response),
            GateAction::Abort(_)
        ));
        assert!(workflow::load_state(root, phase).unwrap().stopped);
        assert!(holder.is_live(), "no process is signalled at a gate");
        registration.finish();
    }

    /// D-04: `doctor`'s fact is the registration. A lock record plays no part.
    #[test]
    fn doctor_reads_the_waiter_from_the_registration() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(5014), Stage::Code);
        state_pending_at(root, phase, stage);
        plant_live_lock_record(root, phase);
        let (_, registration) = Gates::open_registered(root, phase, stage, "g").unwrap();

        let held = collect_phase_facts(root);
        assert_eq!(held[0].waiter, WaiterStatus::Live);
        assert!(check_open_gate_without_waiter(&held[0]).is_none());

        registration.finish();
        let released = collect_phase_facts(root);
        assert_eq!(released[0].waiter, WaiterStatus::Absent);
        assert!(check_open_gate_without_waiter(&released[0]).is_some());
    }

    /// The `gate_waiter` of every `doctor --json` entry for the states under `root`.
    /// A missing key panics here, so it can never read as a `null`.
    fn doctor_json_gate_waiters(root: &Path) -> Vec<serde_json::Value> {
        let body = render_reconciliation_json(&collect_phase_facts(root));
        let waiters: Vec<serde_json::Value> = body
            .as_array()
            .expect("an array")
            .iter()
            .map(|entry| {
                entry
                    .get("gate_waiter")
                    .unwrap_or_else(|| panic!("an entry has no gate_waiter: {entry}"))
                    .clone()
            })
            .collect();
        assert!(!waiters.is_empty(), "a saved state must produce an entry");
        waiters
    }

    /// R2-01: every entry names the open gate and its registration, and with no gate
    /// open both ids are `null`. `finish` removes the registration file, so the
    /// released read finds it missing, with no wait on a lock's release.
    #[test]
    fn doctor_json_reports_the_open_gates_waiter() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5016);
        state_pending_at(root, phase, Stage::Define);
        let (opened, registration) =
            Gates::open_registered(root, phase, Stage::Define, "g").unwrap();
        let gate_id = opened.gate_id.as_str();

        for waiter in doctor_json_gate_waiters(root) {
            assert_eq!(
                waiter,
                serde_json::json!({"stage": "define", "gate_id": gate_id, "status": "live"})
            );
        }

        registration.finish();
        for waiter in doctor_json_gate_waiters(root) {
            assert_eq!(
                waiter,
                serde_json::json!({"stage": "define", "gate_id": gate_id, "status": "absent"})
            );
        }

        let no_gate = tempfile::tempdir().unwrap();
        state_pending_at(no_gate.path(), phase, Stage::Define);
        for waiter in doctor_json_gate_waiters(no_gate.path()) {
            assert_eq!(
                waiter,
                serde_json::json!({"stage": null, "gate_id": null, "status": "absent"})
            );
        }
    }

    /// With several gates open, the status describes the gate named beside it: the one
    /// with a live registration, and once none has one, the first the findings name.
    #[test]
    fn doctor_json_names_the_live_gate_among_several_open_ones() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5017);
        state_pending_at(root, phase, Stage::Validate);
        let unwaited = Gates::open_gate(root, phase, Stage::Code, "no waiter").unwrap();
        let (waited, registration) =
            Gates::open_registered(root, phase, Stage::Validate, "waiter").unwrap();

        for waiter in doctor_json_gate_waiters(root) {
            assert_eq!(
                waiter,
                serde_json::json!({
                    "stage": "validate",
                    "gate_id": waited.gate_id.as_str(),
                    "status": "live",
                })
            );
        }

        registration.finish();
        for waiter in doctor_json_gate_waiters(root) {
            assert_eq!(
                waiter,
                serde_json::json!({
                    "stage": "code",
                    "gate_id": unwaited.gate_id.as_str(),
                    "status": "absent",
                })
            );
        }
    }

    /// T-50.1-70: each of the three held files protects its holder from the stray reaper,
    /// and an unheld one protects nothing.
    #[test]
    fn registry_reachable_pids_includes_held_reservation_and_waiter_holders() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let (phase, stage) = (PhaseId::new(5015), Stage::Code);
        let pid = std::process::id();
        let roots = std::slice::from_ref(&root);
        assert!(
            !registry_reachable_pids(roots).contains(&pid),
            "nothing held"
        );

        let live_path = liveness::launch_liveness_path(&root, "aaaa0000aaaa5015").unwrap();
        std::fs::create_dir_all(live_path.parent().unwrap()).unwrap();
        let reservation = liveness::Hold::take(&live_path).unwrap();
        assert!(registry_reachable_pids(roots).contains(&pid), "reservation");
        drop(reservation);
        wait_until_unheld(&live_path);
        assert!(live_path.exists(), "the file outlives its holder");
        assert!(
            !registry_reachable_pids(roots).contains(&pid),
            "unheld file"
        );

        let gate = Gates::open_gate(&root, phase, stage, "g").unwrap().gate_id;
        let waiter = Gates::register_waiter(&root, phase, stage, &gate).unwrap();
        assert!(registry_reachable_pids(roots).contains(&pid), "waiter");
        drop(waiter);
        let waiter_path = Gates::waiter_path(&root, phase, stage, &gate);
        wait_until_unheld(&waiter_path);
        assert!(waiter_path.exists(), "the file outlives its holder");
        assert!(
            !registry_reachable_pids(roots).contains(&pid),
            "unheld file"
        );

        let holder = liveness::claim_phase_holder(&root, phase).unwrap();
        assert!(registry_reachable_pids(roots).contains(&pid), "holder");
        drop(holder);
        let holder_path = liveness::phase_holder_path(&root, phase);
        wait_until_unheld(&holder_path);
        assert!(holder_path.exists(), "the file outlives its holder");
        assert!(
            !registry_reachable_pids(roots).contains(&pid),
            "unheld file"
        );
    }

    /// A child killed and reaped when this drops, on a failed assertion too.
    struct KilledOnDrop(std::process::Child);

    impl Drop for KilledOnDrop {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// Wait up to 10 s for `path` to exist.
    fn wait_for_file(path: &Path) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !path.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "{} never appeared",
                path.display()
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// R-03: a stopped write-lock holder ends `stop` within the bound, naming the holder,
    /// with nothing written. Control: once the holder is dead the same `stop` records it.
    #[test]
    fn stop_names_a_stopped_write_lock_holder_and_returns_within_its_bound() {
        const NAME: &str = "commands::tests::\
                            stop_names_a_stopped_write_lock_holder_and_returns_within_its_bound";
        let phase = PhaseId::new(5016);
        if devflow_core::test_support::in_child_test(NAME) {
            let root = holder_child_root();
            workflow::update_state(&root, phase, |_| {
                std::fs::write(root.join("holder.ready"), "").unwrap();
                std::thread::sleep(std::time::Duration::from_secs(60));
                Ok::<(), workflow::WorkflowError>(())
            })
            .unwrap();
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let path_dir = tempfile::tempdir().unwrap();
        workflow::save_state(&State::new(
            phase,
            AgentKind::Claude,
            Mode::Auto,
            root.clone(),
        ))
        .unwrap();
        let mut holder = KilledOnDrop(devflow_core::test_support::spawn_test_in_child(
            NAME,
            path_dir.path(),
            &[(HOLDER_ROOT_ENV, root.as_os_str())],
        ));
        let holder_pid = holder.0.id();
        wait_for_file(&root.join("holder.ready"));
        let raw_pid = libc::pid_t::try_from(holder_pid).unwrap();
        // SAFETY: kill reads no memory, and the pid is this test's own unreaped child.
        assert_eq!(unsafe { libc::kill(raw_pid, libc::SIGSTOP) }, 0);
        let before = std::fs::read(workflow::state_path(&root, phase)).unwrap();
        let cache = tempfile::tempdir().unwrap();

        // On a thread, so a `stop` that does not give up fails here rather than hanging.
        let (finished, wait_finished) = std::sync::mpsc::channel();
        let (stop_root, stop_cache) = (root.clone(), cache.path().to_path_buf());
        let started = std::time::Instant::now();
        std::thread::spawn(move || {
            let result = stop_in(&stop_root, phase, Some(&stop_cache));
            let _ = finished.send((result, started.elapsed()));
        });
        let (result, took) = wait_finished
            .recv_timeout(workflow::WRITE_LOCK_REPAIR_WAIT + std::time::Duration::from_secs(5))
            .expect("stop returns within its bound over a stopped holder");

        let error = result.expect_err("nothing can be written under the held lock");
        assert!(
            took >= workflow::WRITE_LOCK_REPAIR_WAIT,
            "gave up after {took:?}"
        );
        assert!(
            took < workflow::WRITE_LOCK_REPAIR_WAIT + std::time::Duration::from_secs(2),
            "waited {took:?}"
        );
        let message = error.to_string();
        assert!(
            matches!(
                error,
                CliError::Workflow(workflow::WorkflowError::WriteLockTimeout {
                    holder: Some(pid),
                    ..
                }) if pid == holder_pid
            ),
            "{message}"
        );
        assert!(message.contains(&format!("pid {holder_pid}")), "{message}");
        assert!(message.contains("nothing was written"), "{message}");
        assert_eq!(
            std::fs::read(workflow::state_path(&root, phase)).unwrap(),
            before,
            "nothing is written"
        );

        holder.0.kill().unwrap();
        holder.0.wait().unwrap();
        stop_in(&root, phase, Some(cache.path())).expect("the lock is free once it is dead");
        assert!(workflow::load_state(&root, phase).unwrap().stopped);
    }

    /// Every file under [`Gates::dir`], with its bytes.
    fn gate_files(root: &Path) -> std::collections::BTreeMap<std::ffi::OsString, Vec<u8>> {
        std::fs::read_dir(Gates::dir(root))
            .unwrap()
            .flatten()
            .map(|entry| (entry.file_name(), std::fs::read(entry.path()).unwrap()))
            .collect()
    }

    /// A Ship gate with its gate file, an approval and that approval's claim: a finalizing
    /// `devflow ship`, as its registration has finished.
    fn claimed_ship_gate(root: &Path, phase: PhaseId) -> GateId {
        state_pending_at(root, phase, Stage::Ship);
        let (opened, registration) = Gates::open_registered(root, phase, Stage::Ship, "g").unwrap();
        registration.finish();
        let response = GateResponse {
            approved: true,
            note: None,
            responded_by: Some("human".into()),
        };
        Gates::answer(root, phase, Stage::Ship, &opened.gate_id, &response).unwrap();
        assert!(matches!(
            Gates::claim(root, phase, Stage::Ship, &opened.gate_id).unwrap(),
            devflow_core::gates::Claim::Claimed
        ));
        opened.gate_id
    }

    /// `stop` over `phase` with a holder child: it adds nothing under the gate directory,
    /// reaches the child, and reports the not-marked outcome because this hold outlives it.
    fn assert_stop_reaches_the_holder_and_writes_no_gate_file(root: &Path, phase: PhaseId) {
        let mut holder = SleepingChild::spawn();
        let _hold = hold_phase_with_record(root, phase, holder.pid(), Some(holder.start));
        let before = gate_files(root);
        let cache = tempfile::tempdir().unwrap();

        let stopped = stop_in(root, phase, Some(cache.path()));

        assert_eq!(gate_files(root), before, "no answer, claim or other file");
        assert!(
            child_exits_within(&mut holder.child, agent::TERMINATE_VERIFY_WAIT),
            "the holder is reached: {stopped:?}"
        );
        let error = stopped
            .expect_err("this test's hold outlives its child")
            .to_string();
        assert!(error.contains("not marked stopped"), "{error}");
    }

    /// Planner C1's MUST-DO for the D-18 fixture: a gate whose answer a consumer claimed is
    /// never answered again, and `stop` reaches that consumer through the holder. Control:
    /// an unclaimed Ship gate with no waiter still gets the abort stored for `devflow ship`.
    #[test]
    fn stop_reaches_the_holder_past_a_claimed_ship_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Listed by no one: `list_open` hides a gate whose live answer exists.
        let claimed = PhaseId::new(5017);
        claimed_ship_gate(root, claimed);
        assert_stop_reaches_the_holder_and_writes_no_gate_file(root, claimed);

        // A partial close: the answer is gone, so the gate is listed, and the claim is all
        // that tells `stop` a consumer took it.
        let partly_closed = PhaseId::new(5018);
        let gate = claimed_ship_gate(root, partly_closed);
        std::fs::remove_file(Gates::answer_path(root, partly_closed, Stage::Ship, &gate)).unwrap();
        assert!(
            Gates::list_open(root)
                .iter()
                .any(|open| open.phase == partly_closed),
            "the fixture reaches the claim check"
        );
        assert_stop_reaches_the_holder_and_writes_no_gate_file(root, partly_closed);

        let unclaimed = PhaseId::new(5019);
        state_pending_at(root, unclaimed, Stage::Ship);
        let gate = Gates::open_gate(root, unclaimed, Stage::Ship, "g")
            .unwrap()
            .gate_id;
        let holder = SleepingChild::spawn();
        let _hold = hold_phase_with_record(root, unclaimed, holder.pid(), Some(holder.start));
        let cache = tempfile::tempdir().unwrap();
        stop_in(root, unclaimed, Some(cache.path())).expect("the stored abort is the stop");
        assert!(matches!(
            Gates::read_answer(root, unclaimed, Stage::Ship, &gate),
            AnswerRead::Bound(answer) if !answer.response.approved
        ));
        assert!(holder.is_live(), "the control signals no holder");
    }

    /// The stable parts of `persist_stopped_state`'s refusal over a launch it did not address.
    fn assert_names_an_unaddressed_launch(error: &str, launch: &str, field: &str, phase: u32) {
        assert!(
            error.contains("this stop did not end, answer, clear or find not running"),
            "{error}"
        );
        assert!(
            error.contains(&format!("launch {launch} in its {field} field")),
            "{error}"
        );
        assert!(
            error.contains(&format!("devflow stop --phase {phase}")),
            "{error}"
        );
    }

    /// The holder-release window: `stop` never marks a run `stopped` beside a launch none of
    /// its arms addressed. Control: a state naming only the launch it found GONE is marked.
    #[test]
    fn stop_does_not_mark_stopped_beside_a_supervisor_it_never_saw() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let known = ["launch-a".to_string()];

        let recorded = PhaseId::new(5020);
        save_state_naming(root, recorded, &root.join("b.sock"), "launch-b");
        let before = std::fs::read(workflow::state_path(root, recorded)).unwrap();
        let error = persist_stopped_state(root, recorded, false, &known)
            .expect_err("launch-b was never addressed")
            .to_string();
        assert_names_an_unaddressed_launch(&error, "launch-b", "supervisor", 5020);
        assert_eq!(
            std::fs::read(workflow::state_path(root, recorded)).unwrap(),
            before
        );

        let pending = PhaseId::new(5021);
        save_state_naming(root, pending, &root.join("a.sock"), "launch-a");
        let mut state = workflow::load_state(root, pending).unwrap();
        state.pending_supervisor = Some(PendingSupervisor::new(
            "launch-b".to_string(),
            root.join("b.sock"),
            root.join("b.live"),
        ));
        workflow::save_state(&state).unwrap();
        let before = std::fs::read(workflow::state_path(root, pending)).unwrap();
        let error = persist_stopped_state(root, pending, false, &known)
            .expect_err("launch-b was never addressed")
            .to_string();
        assert_names_an_unaddressed_launch(&error, "launch-b", "pending_supervisor", 5021);
        assert_eq!(
            std::fs::read(workflow::state_path(root, pending)).unwrap(),
            before
        );

        let found_gone = PhaseId::new(5022);
        save_state_naming(root, found_gone, &root.join("a.sock"), "launch-a");
        persist_stopped_state(root, found_gone, false, &known).unwrap();
        assert!(workflow::load_state(root, found_gone).unwrap().stopped);
    }

    /// The reply of a supervisor that stops when asked, then takes its socket away.
    fn stopped_and_gone() -> ShutdownScript {
        ShutdownScript {
            lines: vec![ACCEPTED_LINE.to_string(), STOPPED_LINE.to_string()],
            unlink_after: true,
        }
    }

    type SharedBytes = std::sync::Arc<std::sync::Mutex<Vec<u8>>>;

    /// Rewrite `phase`'s state as a concurrent launcher would and keep the resulting bytes.
    fn rewrite_state_keeping_bytes(
        root: &Path,
        phase: PhaseId,
        kept: &SharedBytes,
        change: impl FnOnce(&mut State),
    ) {
        workflow::update_state(root, phase, |state| {
            change(state);
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();
        *kept.lock().unwrap() = std::fs::read(workflow::state_path(root, phase)).unwrap();
    }

    /// R2-06: a launch that appears after the addressed one exited is never marked. One
    /// recorded as the supervisor reaches the write and is refused there. One reserved and
    /// held reaches `stop`'s pending branch first, which ends in its own not-recorded
    /// refusal. Control: with no replacement the run is marked.
    #[test]
    fn a_socket_stop_never_marks_a_replacement_launch_stopped() {
        const A: &str = "aaaa0000aaaa5023";
        const B: &str = "bbbb0000bbbb5023";
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let (_base, sock) = stop_socket_dir();
        let cache = tempfile::tempdir().unwrap();

        let recorded = PhaseId::new(5023);
        let a_path = sock.join("a23.sock");
        save_state_naming(&root, recorded, &a_path, A);
        let kept = SharedBytes::default();
        let a = ScriptedSupervisor::serve(&a_path, A, recorded, "agent_running", {
            let (root, kept) = (root.clone(), kept.clone());
            move |_| {
                rewrite_state_keeping_bytes(&root, recorded, &kept, |state| {
                    state.supervisor = Some(SupervisorRecord::new(
                        root.join("b.sock"),
                        B.to_string(),
                        1,
                        0,
                        "boot".to_string(),
                    ));
                });
                stopped_and_gone()
            }
        });
        let error = stop_in(&root, recorded, Some(cache.path()))
            .expect_err("the replacement was never addressed")
            .to_string();
        assert_eq!(a.shutdowns(), 1, "the addressed launch was ended");
        assert_names_an_unaddressed_launch(&error, B, "supervisor", 5023);
        assert_eq!(
            std::fs::read(workflow::state_path(&root, recorded)).unwrap(),
            *kept.lock().unwrap(),
            "nothing is written over the replacement"
        );

        let reserved = PhaseId::new(5024);
        let a_path = sock.join("a24.sock");
        save_state_naming(&root, reserved, &a_path, A);
        let b_liveness = liveness::launch_liveness_path(&root, B).unwrap();
        let b_hold = liveness::Hold::take(&b_liveness).unwrap();
        let kept = SharedBytes::default();
        let a = ScriptedSupervisor::serve(&a_path, A, reserved, "agent_running", {
            let (root, kept) = (root.clone(), kept.clone());
            move |_| {
                rewrite_state_keeping_bytes(&root, reserved, &kept, |state| {
                    state.pending_supervisor = Some(PendingSupervisor::new(
                        B.to_string(),
                        root.join("never-published.sock"),
                        liveness::launch_liveness_path(&root, B).unwrap(),
                    ));
                });
                stopped_and_gone()
            }
        });
        let error = stop_in(&root, reserved, Some(cache.path()))
            .expect_err("the reservation is live")
            .to_string();
        assert_eq!(a.shutdowns(), 1, "the addressed launch was ended");
        assert!(error.contains(B), "{error}");
        assert!(error.contains("devflow stop --phase 5024"), "{error}");
        assert_eq!(
            std::fs::read(workflow::state_path(&root, reserved)).unwrap(),
            *kept.lock().unwrap(),
            "nothing is written over the reservation"
        );
        drop(b_hold);

        let control = PhaseId::new(5025);
        let a_path = sock.join("a25.sock");
        save_state_naming(&root, control, &a_path, A);
        let a =
            ScriptedSupervisor::serve(&a_path, A, control, "agent_running", |_| stopped_and_gone());
        stop_in(&root, control, Some(cache.path())).expect("nothing replaced the launch");
        assert_eq!(a.shutdowns(), 1);
        assert!(workflow::load_state(&root, control).unwrap().stopped);
    }

    /// A saved state naming a dead recorded launch (its socket does not exist) beside a
    /// pending launch served at `socket`, the shape a resume leaves after a crash.
    fn state_with_dead_recorded_and_pending(
        root: &Path,
        phase: PhaseId,
        dead: &str,
        pending: &str,
        socket: &Path,
    ) {
        save_state_naming(root, phase, &root.join("gone.sock"), dead);
        let mut state = workflow::load_state(root, phase).unwrap();
        state.pending_supervisor = Some(PendingSupervisor::new(
            pending.to_string(),
            socket.to_path_buf(),
            liveness::launch_liveness_path(root, pending).unwrap(),
        ));
        workflow::save_state(&state).unwrap();
    }

    /// T-50.1-157: `stop` ends a live pending supervisor beside a dead recorded one and marks
    /// the run, because its socket arm found the dead one GONE. Control: a launch published
    /// during that shutdown, still reserved and unaddressed, is refused.
    #[test]
    fn stop_marks_stopped_after_ending_a_pending_supervisor_beside_a_dead_recorded_one() {
        const A: &str = "aaaa0000aaaa5026";
        const B: &str = "bbbb0000bbbb5026";
        const C: &str = "cccc0000cccc5026";
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let (_base, sock) = stop_socket_dir();
        let cache = tempfile::tempdir().unwrap();

        let marked = PhaseId::new(5026);
        let b_path = sock.join("b26.sock");
        state_with_dead_recorded_and_pending(&root, marked, A, B, &b_path);
        let mut b_hold =
            Some(liveness::Hold::take(&liveness::launch_liveness_path(&root, B).unwrap()).unwrap());
        let b = ScriptedSupervisor::serve(&b_path, B, marked, "agent_running", move |_| {
            b_hold.take();
            stopped_and_gone()
        });
        stop_in(&root, marked, Some(cache.path())).expect("both launches were addressed");
        assert_eq!(b.shutdowns(), 1, "the pending launch was ended");
        assert!(workflow::load_state(&root, marked).unwrap().stopped);

        // Its lock outlives its reply, so the reservation is still recorded when `stop` writes.
        let lingering = PhaseId::new(5030);
        let (b3, b_path) = ("bbbb0000bbbb5030", sock.join("b30.sock"));
        state_with_dead_recorded_and_pending(&root, lingering, A, b3, &b_path);
        let _lingering_hold =
            liveness::Hold::take(&liveness::launch_liveness_path(&root, b3).unwrap()).unwrap();
        let b = ScriptedSupervisor::serve(&b_path, b3, lingering, "agent_running", |_| {
            stopped_and_gone()
        });
        stop_in(&root, lingering, Some(cache.path())).expect("this stop ended the reservation");
        assert_eq!(b.shutdowns(), 1, "the pending launch was ended");
        assert!(workflow::load_state(&root, lingering).unwrap().stopped);

        let refused = PhaseId::new(5027);
        let (b2, b_path) = ("bbbb0000bbbb5027", sock.join("b27.sock"));
        state_with_dead_recorded_and_pending(&root, refused, A, b2, &b_path);
        let _b_hold =
            liveness::Hold::take(&liveness::launch_liveness_path(&root, b2).unwrap()).unwrap();
        // Held, so the pending branch waits on it and does not clear it as abandoned.
        let _c_hold =
            liveness::Hold::take(&liveness::launch_liveness_path(&root, C).unwrap()).unwrap();
        let kept = SharedBytes::default();
        let b = ScriptedSupervisor::serve(&b_path, b2, refused, "agent_running", {
            let (root, kept) = (root.clone(), kept.clone());
            move |_| {
                rewrite_state_keeping_bytes(&root, refused, &kept, |state| {
                    state.pending_supervisor = Some(PendingSupervisor::new(
                        C.to_string(),
                        root.join("never-published.sock"),
                        liveness::launch_liveness_path(&root, C).unwrap(),
                    ));
                });
                stopped_and_gone()
            }
        });
        let error = stop_in(&root, refused, Some(cache.path()))
            .expect_err("the published launch was never addressed")
            .to_string();
        assert_eq!(b.shutdowns(), 1, "the pending launch was ended");
        assert_names_an_unaddressed_launch(&error, C, "pending_supervisor", 5027);
        assert_eq!(
            std::fs::read(workflow::state_path(&root, refused)).unwrap(),
            *kept.lock().unwrap(),
            "nothing is written over the new reservation"
        );
    }

    /// Plan-check iteration 2: a launch pending when `stop` began that promotes while no arm
    /// addresses it is refused, since no state read seeds the list. Control: the run whose
    /// gate the arm answered is marked.
    #[test]
    fn stop_refuses_to_mark_stopped_beside_a_pending_launch_that_promoted_unaddressed() {
        const A: &str = "aaaa0000aaaa5028";
        const B: &str = "bbbb0000bbbb5028";
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let stage = Stage::Code;
        let cache = tempfile::tempdir().unwrap();
        let at_gate = |phase: PhaseId| {
            state_pending_at(&root, phase, stage);
            save_state_naming(&root, phase, &root.join("a.sock"), A);
        };

        let promoted = PhaseId::new(5028);
        at_gate(promoted);
        let mut state = workflow::load_state(&root, promoted).unwrap();
        state.pending_supervisor = Some(PendingSupervisor::new(
            B.to_string(),
            root.join("b.sock"),
            root.join("b.live"),
        ));
        workflow::save_state(&state).unwrap();
        let (opened, registration) = Gates::open_registered(&root, promoted, stage, "g").unwrap();
        let kept = SharedBytes::default();
        let hook = {
            let (root, kept) = (root.clone(), kept.clone());
            move || {
                rewrite_state_keeping_bytes(&root, promoted, &kept, |state| {
                    state.supervisor = Some(SupervisorRecord::new(
                        root.join("b.sock"),
                        B.to_string(),
                        1,
                        0,
                        "boot".to_string(),
                    ));
                    state.pending_supervisor = None;
                });
            }
        };
        let error =
            with_answer_before_publish(hook, || stop_in(&root, promoted, Some(cache.path())))
                .expect_err("the promoted launch was never addressed")
                .to_string();
        assert!(matches!(
            Gates::read_answer(&root, promoted, stage, &opened.gate_id),
            AnswerRead::Bound(answer) if !answer.response.approved
        ));
        assert_names_an_unaddressed_launch(&error, B, "supervisor", 5028);
        assert_eq!(
            std::fs::read(workflow::state_path(&root, promoted)).unwrap(),
            *kept.lock().unwrap(),
            "nothing is written over the promotion"
        );
        registration.finish();

        let answered = PhaseId::new(5029);
        at_gate(answered);
        let (_, registration) = Gates::open_registered(&root, answered, stage, "g").unwrap();
        stop_in(&root, answered, Some(cache.path())).expect("the answered run is the only one");
        assert!(workflow::load_state(&root, answered).unwrap().stopped);
        registration.finish();
    }

    /// `gate sweep` stops at a gate lock it cannot take, rather than skipping that gate
    /// and reporting the rest. Control: with the lock free, the same sweep reaps it.
    #[test]
    fn gate_sweep_ends_on_a_held_gate_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(5008), Stage::Code);
        let (opened, registration) = Gates::open_registered(root, phase, stage, "g").unwrap();
        let gate_lock = hold_gate_lock(root, phase);

        let error = gate_sweep(Some(0), false, Some(root.to_path_buf()), false)
            .expect_err("a held gate lock ends the sweep");

        assert!(
            matches!(error, CliError::Gate(GateError::GateLockTimeout { .. })),
            "{error}"
        );
        assert!(gate_answer_files(root, phase, stage).is_empty());

        drop(gate_lock);
        gate_sweep(Some(0), false, Some(root.to_path_buf()), false).unwrap();
        assert!(matches!(
            Gates::read_answer(root, phase, stage, &opened.gate_id),
            AnswerRead::Bound(answer) if !answer.response.approved
        ));
        registration.finish();
    }

    /// R2-04: a writer whose answer was published but could not be confirmed exits
    /// non-zero and never says nothing changed, and the answer stays where it is.
    #[test]
    fn gate_writers_report_a_published_but_unconfirmed_answer() {
        let check = |phase: PhaseId, write: &dyn Fn(&Path) -> Result<(), CliError>| {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let (opened, registration) =
                Gates::open_registered(root, phase, Stage::Code, "g").unwrap();
            let parked = std::rc::Rc::new(std::cell::RefCell::new(None));
            let (hook_root, hook_parked) = (root.to_path_buf(), parked.clone());
            let error = with_answer_before_publish(
                move || {
                    *hook_parked.borrow_mut() = Some(hold_gate_lock(&hook_root, phase));
                },
                || write(root),
            )
            .expect_err("an unconfirmed answer exits non-zero");
            let message = error.to_string();
            assert!(
                matches!(
                    error,
                    CliError::Gate(GateError::PublishedUnconfirmed { .. })
                ),
                "{message}"
            );
            assert!(message.contains("published"), "{message}");
            assert!(!message.contains("nothing was changed"), "{message}");
            drop(parked.borrow_mut().take());
            assert!(matches!(
                Gates::read_answer(root, phase, Stage::Code, &opened.gate_id),
                AnswerRead::Bound(_)
            ));
            registration.finish();
        };
        let responding = PhaseId::new(5009);
        check(responding, &|root| {
            gate_respond(root, responding, Some(Stage::Code), true, None, None)
        });
        check(PhaseId::new(5010), &|root| {
            gate_sweep(Some(0), false, Some(root.to_path_buf()), false)
        });
    }

    /// R2-04: one `stop`, one action. An abort published with its re-check timed out is
    /// answered: `stop` goes on to record the stop and never reaches the holder.
    #[test]
    fn stop_treats_a_published_but_unconfirmed_abort_as_answered() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(5007), Stage::Code);
        state_pending_at(root, phase, stage);
        let (opened, registration) = Gates::open_registered(root, phase, stage, "g").unwrap();
        let holder = SleepingChild::spawn();
        let _hold = hold_phase_with_record(root, phase, holder.pid(), Some(holder.start));
        let cache = tempfile::tempdir().unwrap();
        let parked = std::rc::Rc::new(std::cell::RefCell::new(None));

        let started = std::time::Instant::now();
        let hook_root = root.to_path_buf();
        let hook_parked = parked.clone();
        let stopped = with_answer_before_publish(
            move || {
                *hook_parked.borrow_mut() = Some(hold_gate_lock(&hook_root, phase));
            },
            || stop_in(root, phase, Some(cache.path())),
        );
        let took = started.elapsed();

        stopped.expect("a published abort is the requested stop");
        assert!(
            took >= devflow_core::gates::GATE_LOCK_WAIT,
            "the re-check waited for the lock: {took:?}"
        );
        std::thread::sleep(std::time::Duration::from_secs(1));
        assert!(holder.is_live(), "the holder arm never ran");
        assert!(workflow::load_state(root, phase).unwrap().stopped);
        drop(parked.borrow_mut().take());
        assert!(matches!(
            Gates::read_answer(root, phase, stage, &opened.gate_id),
            AnswerRead::Bound(answer) if !answer.response.approved
        ));
        registration.finish();
    }

    /// An unconfirmed abort may have reached no waiter, so `stop` still shuts a recorded
    /// supervisor down through its socket rather than record `stopped` beside it.
    #[test]
    fn stop_shuts_the_supervisor_down_after_a_published_but_unconfirmed_abort() {
        const LAUNCH: &str = "aaaa0000aaaa5306";
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(5306), Stage::Code);
        state_pending_at(root, phase, stage);
        let socket = root.join("a.sock");
        save_state_naming(root, phase, &socket, LAUNCH);
        let supervisor = ScriptedSupervisor::serve(&socket, LAUNCH, phase, "agent_running", |_| {
            stopped_and_gone()
        });
        let (_, registration) = Gates::open_registered(root, phase, stage, "g").unwrap();
        let cache = tempfile::tempdir().unwrap();
        let parked = std::rc::Rc::new(std::cell::RefCell::new(None));
        let (hook_root, hook_parked) = (root.to_path_buf(), parked.clone());

        with_answer_before_publish(
            move || {
                *hook_parked.borrow_mut() = Some(hold_gate_lock(&hook_root, phase));
            },
            || stop_in(root, phase, Some(cache.path())),
        )
        .unwrap();

        assert_eq!(supervisor.shutdowns(), 1, "the socket arm ran");
        assert!(workflow::load_state(root, phase).unwrap().stopped);
        drop(parked.borrow_mut().take());
        registration.finish();
    }

    /// D-04: only the registration counts. A live lock record alone reads not live.
    #[test]
    fn a_waiter_is_live_only_by_its_registration() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(4904), Stage::Code);
        let gate = Gates::open_gate(root, phase, stage, "g").unwrap().gate_id;
        assert!(!waiter_is_live(root, phase, stage, &gate), "nothing at all");

        let registration = Gates::register_waiter(root, phase, stage, &gate).unwrap();
        assert!(waiter_is_live(root, phase, stage, &gate), "registration");
        registration.finish();
        assert!(!waiter_is_live(root, phase, stage, &gate), "finished");

        plant_live_lock_record(root, phase);
        assert!(!waiter_is_live(root, phase, stage, &gate), "a lock record");
    }

    #[test]
    fn a_gate_answer_published_as_its_gate_closed_is_reported_as_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(4905), Stage::Code);
        let (opened, registration) = Gates::open_registered(root, phase, stage, "g1").unwrap();
        let g1 = opened.gate_id;

        let hook_root = root.to_path_buf();
        let err = with_answer_before_publish(
            move || {
                Gates::close(&hook_root, phase, stage).unwrap();
                Gates::open_gate(&hook_root, phase, stage, "g2").unwrap();
            },
            || gate_respond(root, phase, Some(stage), true, None, Some(g1.to_string())),
        )
        .expect_err("an answer set aside must exit non-zero")
        .to_string();

        assert!(err.contains(g1.as_str()), "{err}");
        assert!(err.contains("has been set aside"), "{err}");
        assert!(err.contains("nothing will act on it"), "{err}");
        let quarantined = Gates::quarantined(root);
        assert_eq!(quarantined.len(), 1, "{quarantined:?}");
        assert_eq!(quarantined[0].label, g1.as_str());
        assert!(!Gates::answer_path(root, phase, stage, &g1).exists());
        registration.finish();
    }

    /// D-05: a set-aside answer is reported per phase by `status` and `doctor`, and nothing
    /// is said when there is none. `status` names the full path; `doctor`'s findings name it
    /// from the project root, because they never carry a username (T-18-01).
    #[test]
    fn status_and_doctor_list_quarantined_gate_answers() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(7);
        let doctor_json = |findings: &[PhaseFinding]| {
            doctor_json_body(&[], &[], &[], &[], (&[], None), findings)["set_aside_gate_answers"]
                .as_array()
                .unwrap()
                .clone()
        };

        let none = Gates::quarantined(root);
        assert_eq!(render_set_aside_answers(&none), None);
        let no_findings = set_aside_answer_findings(root, &none);
        assert!(no_findings.is_empty());
        assert_eq!(render_set_aside_findings_text(&no_findings), "");
        assert_eq!(doctor_json(&no_findings), Vec::<serde_json::Value>::new());

        let opened = Gates::open_gate(root, phase, Stage::Code, "ctx").unwrap();
        let response = GateResponse {
            approved: true,
            note: None,
            responded_by: None,
        };
        Gates::answer(root, phase, Stage::Code, &opened.gate_id, &response).unwrap();
        Gates::close(root, phase, Stage::Code).unwrap();
        // A legacy unkeyed answer: the form that names no gate.
        std::fs::write(
            Gates::dir(root).join(format!("{}-plan.response.json", phase.padded())),
            serde_json::to_vec(&response).unwrap(),
        )
        .unwrap();
        Gates::close(root, phase, Stage::Plan).unwrap();
        let answers = Gates::quarantined(root);
        let mut labels: Vec<&str> = answers.iter().map(|a| a.label.as_str()).collect();
        labels.sort_unstable();
        let mut expected = vec![opened.gate_id.as_str(), "noid"];
        expected.sort_unstable();
        assert_eq!(labels, expected, "one for a gate id, one with none");

        let block = render_set_aside_answers(&answers).expect("two answers are listed");
        assert!(block.starts_with("set-aside gate answers:"), "{block}");
        assert!(block.contains("phase 7:"), "{block}");
        for answer in &answers {
            assert!(
                block.contains(&format!("{}", answer.path.display())),
                "{block}"
            );
        }

        let findings = set_aside_answer_findings(root, &answers);
        assert_eq!(findings.len(), 2);
        let root_text = root.display().to_string();
        let text = render_set_aside_findings_text(&findings);
        assert!(text.contains("set-aside gate answers:"), "{text}");
        let json = doctor_json(&findings);
        assert_eq!(json.len(), 2, "{json:?}");
        for (index, answer) in answers.iter().enumerate() {
            let relative = answer
                .path
                .strip_prefix(root)
                .unwrap()
                .display()
                .to_string();
            let finding = &findings[index];
            assert_eq!(finding.phase, phase);
            assert_eq!(finding.severity, Severity::Info);
            assert!(finding.detail.contains(&relative), "{}", finding.detail);
            assert!(!finding.detail.contains(&root_text), "{}", finding.detail);
            assert!(text.contains(&relative), "{text}");
            assert_eq!(
                finding.repair.as_deref(),
                Some("inspect it; answer the live gate with `devflow gate approve|reject`")
            );
            assert_eq!(json[index]["severity"], "info");
            assert_eq!(json[index]["phase"], serde_json::json!(phase));
            assert!(
                json[index]["detail"].as_str().unwrap().contains(&relative),
                "{json:?}"
            );
        }
        assert!(!text.contains(&root_text), "{text}");
    }

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
    fn gate_id_flag_is_taken_by_approve_and_reject() {
        let approve =
            Cli::try_parse_from(["devflow", "gate", "approve", "15", "--gate-id", "abc"]).unwrap();
        let Command::Gate {
            action: GateCmd::Approve { gate_id, .. },
        } = approve.command
        else {
            panic!("expected gate approve");
        };
        assert_eq!(gate_id.as_deref(), Some("abc"));

        let reject = Cli::try_parse_from([
            "devflow",
            "gate",
            "reject",
            "15",
            "--note",
            "n",
            "--gate-id",
            "abc",
        ])
        .unwrap();
        let Command::Gate {
            action: GateCmd::Reject { gate_id, .. },
        } = reject.command
        else {
            panic!("expected gate reject");
        };
        assert_eq!(gate_id.as_deref(), Some("abc"));
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

    /// A ping reply as a supervisor sends it; built through serde because
    /// `PingReply` is `#[non_exhaustive]`.
    fn ping_reply(owner: &str, launch: &str) -> devflow_core::supervisor::protocol::PingReply {
        serde_json::from_value(serde_json::json!({
            "launch_id": "0123456789abcdef",
            "phase": "7",
            "stage": "code",
            "launch": launch,
            "owner": owner,
            "agent_pid": 10,
            "agent_pgid": 10,
            "agent_start_time": 20,
            "boot_id": null,
            "supervisor_pid": 11,
        }))
        .expect("a complete ping reply")
    }

    /// 50-12: the pure mapping over every probe outcome, with and without
    /// `stopped`. Each row of one `stopped` value is a distinct answer, and
    /// GONE splits by `stopped`: an abandoned active phase is not a stopped
    /// one.
    #[test]
    fn liveness_matrix_covers_every_probe_outcome() {
        use devflow_core::supervisor::{Liveness as Probe, ProbeError};
        let record = devflow_core::state::SupervisorRecord::new(
            PathBuf::from("/cache/sock/x.sock"),
            "0123456789abcdef".to_string(),
            10,
            20,
            "boot".to_string(),
        );
        for stopped in [false, true] {
            let rows = [
                phase_liveness(None, None, stopped),
                phase_liveness(Some(&record), Some(Ok(Probe::Gone)), stopped),
                phase_liveness(Some(&record), Some(Ok(Probe::Stale)), stopped),
                phase_liveness(
                    Some(&record),
                    Some(Ok(Probe::Alive(ping_reply("agent_running", "stream")))),
                    stopped,
                ),
                phase_liveness(
                    Some(&record),
                    Some(Err(ProbeError::NoReply("timed out".to_string()))),
                    stopped,
                ),
            ];
            assert_eq!(rows[0], PhaseLiveness::NotLaunched, "stopped={stopped}");
            let gone = if stopped {
                PhaseLiveness::Gone
            } else {
                PhaseLiveness::GoneWhileActive
            };
            assert_eq!(rows[1], gone, "stopped={stopped}");
            assert_eq!(rows[2], PhaseLiveness::Stale, "stopped={stopped}");
            assert_eq!(
                rows[3],
                PhaseLiveness::Alive {
                    owner: "agent_running".to_string(),
                    launch: "stream".to_string(),
                },
                "stopped={stopped}"
            );
            match &rows[4] {
                PhaseLiveness::ProbeFailed(why) => assert!(
                    why.contains("no valid ping reply") && !why.contains('/'),
                    "a path-free probe failure, got {why:?}"
                ),
                other => panic!("a probe error must be ProbeFailed, got {other:?}"),
            }
            for (at, row) in rows.iter().enumerate() {
                for other in &rows[at + 1..] {
                    assert_ne!(
                        row, other,
                        "stopped={stopped}: two outcomes share one answer"
                    );
                }
            }
        }
        assert_ne!(
            phase_liveness(Some(&record), Some(Ok(Probe::Gone)), true),
            phase_liveness(Some(&record), Some(Ok(Probe::Gone)), false),
            "GONE while stopped must differ from GONE while active"
        );
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

    /// 18b: persisting the supervisor record for one phase must not disturb
    /// a concurrently-active sibling phase's record (concurrency edge).
    #[test]
    fn supervisor_record_persisted_for_one_phase_does_not_disturb_a_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let mut phase7 = State::new(
            PhaseId::new(7),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
        let record7 = devflow_core::state::SupervisorRecord::new(
            root.join("seven.sock"),
            "seven00000000000".to_string(),
            111,
            1,
            String::new(),
        );
        phase7.supervisor = Some(record7.clone());
        workflow::save_state(&phase7).unwrap();

        let mut phase8 = State::new(
            PhaseId::new(8),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
        let record8 = devflow_core::state::SupervisorRecord::new(
            root.join("eight.sock"),
            "eight00000000000".to_string(),
            222,
            2,
            String::new(),
        );
        phase8.supervisor = Some(record8.clone());
        workflow::save_state(&phase8).unwrap();

        let reloaded7 = workflow::load_state(root, PhaseId::new(7)).unwrap();
        let reloaded8 = workflow::load_state(root, PhaseId::new(8)).unwrap();
        assert_eq!(reloaded7.supervisor, Some(record7));
        assert_eq!(reloaded8.supervisor, Some(record8));
    }

    /// 18b (idempotency edge): running `devflow status` twice must produce
    /// byte-identical `.devflow/` state — the supervisor liveness probe is
    /// purely a read, same as the existing agent liveness probe it sits
    /// beside. The planted record names a socket that does not exist and the
    /// `u32::MAX` boundary agent group (precision edge), so the probe can
    /// only read it GONE, never a false ALIVE.
    #[test]
    fn status_reading_monitor_liveness_writes_no_state_and_no_event() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(66);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.supervisor = Some(devflow_core::state::SupervisorRecord::new(
            root.join("never-bound.sock"),
            "statusidem000000".to_string(),
            u32::MAX,
            1,
            String::new(),
        ));
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
    /// A saved Define-stage state with both kinds of answer an earlier run can leave:
    /// a keyed answer for its gate, which an abort, and an unkeyed approval from before
    /// gate ids. Returns the state and the two answers' bytes.
    fn plant_leftover_define_answers(root: &Path, phase: PhaseId) -> (State, Vec<Vec<u8>>) {
        let mut state = State::new(
            phase,
            devflow_core::state::AgentKind::Claude,
            devflow_core::mode::Mode::Auto,
            root.to_path_buf(),
        );
        state.gate_notify_cmd = Some("true".to_string());
        workflow::save_state(&state).unwrap();

        let old = Gates::open_gate(root, phase, Stage::Define, "the earlier run's gate").unwrap();
        let keyed = Gates::answer(
            root,
            phase,
            Stage::Define,
            &old.gate_id,
            &GateResponse {
                approved: false,
                note: Some("abort: the earlier run".into()),
                responded_by: Some("test".into()),
            },
        )
        .unwrap();
        let unkeyed = Gates::dir(root).join(format!("{}-define.response.json", phase.padded()));
        std::fs::write(
            &unkeyed,
            br#"{"approved":true,"note":"legacy","responded_by":"test"}"#,
        )
        .unwrap();
        let bytes = vec![
            std::fs::read(&keyed).unwrap(),
            std::fs::read(&unkeyed).unwrap(),
        ];
        (state, bytes)
    }

    /// The bytes of every answer that was set aside, sorted, to compare with what was
    /// planted.
    fn quarantined_bytes(root: &Path) -> Vec<Vec<u8>> {
        let mut bytes: Vec<Vec<u8>> = Gates::quarantined(root)
            .iter()
            .map(|answer| std::fs::read(&answer.path).unwrap())
            .collect();
        bytes.sort();
        bytes
    }

    /// V-20 (D-05): with `start`'s own close skipped, as if it had failed, neither kind
    /// of leftover decides the new run's gate. It times out and both are set aside. The
    /// control is the same wait with an answer that does bind: it decides.
    #[test]
    fn a_leftover_answer_start_could_not_delete_never_decides_the_new_run() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4910);
        let (mut state, mut planted) = plant_leftover_define_answers(root, phase);
        planted.sort();

        let err = crate::pipeline_gate::run_gate_with_timeout(
            root,
            &mut state,
            Stage::Define,
            "the new run's gate",
            1,
            None,
        )
        .expect_err("a leftover must not decide the new run's gate");
        assert!(
            err.to_string().contains("timed out awaiting a response"),
            "{err}"
        );
        assert_eq!(quarantined_bytes(root), planted, "both leftovers set aside");
        assert_eq!(
            gate_answer_files(root, phase, Stage::Define),
            Vec::<std::path::PathBuf>::new()
        );

        devflow_core::test_support::preanswer_gate(
            root,
            phase,
            Stage::Define,
            &GateResponse {
                approved: true,
                note: None,
                responded_by: Some("test".into()),
            },
        );
        let action = crate::pipeline_gate::run_gate_with_timeout(
            root,
            &mut state,
            Stage::Define,
            "control: an answer that binds",
            1,
            None,
        )
        .expect("control: an answer that binds decides the gate");
        assert_eq!(action, GateAction::Advance);
    }

    /// D-05 / T-50.1-48: `start`'s leftover step deletes no answer, and names each one
    /// it set aside.
    #[test]
    fn start_sets_leftover_answers_aside_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4911);
        let (_state, mut planted) = plant_leftover_define_answers(root, phase);
        planted.sort();

        let mut warnings = Vec::new();
        set_aside_leftover_gates(root, phase, &mut warnings);
        let warnings = String::from_utf8(warnings).unwrap();

        assert_eq!(quarantined_bytes(root), planted, "no leftover was deleted");
        for answer in Gates::quarantined(root) {
            let line = format!(
                "warning: set aside an unused answer for phase {phase} define: {}",
                answer.path.display()
            );
            assert!(warnings.contains(&line), "{line} not in: {warnings}");
        }
        assert!(!Gates::gate_path(root, phase, Stage::Define).exists());
        assert_eq!(
            gate_answer_files(root, phase, Stage::Define),
            Vec::<std::path::PathBuf>::new()
        );
    }

    #[test]
    fn gate_respond_auto_resolves_single_open_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        Gates::write_gate(root, PhaseId::new(15), Stage::Ship, "approve merge?").unwrap();

        gate_respond(
            root,
            PhaseId::new(15),
            None,
            true,
            Some("lgtm".into()),
            None,
        )
        .unwrap();

        let gate = crate::test_support::live_gate_id(root, PhaseId::new(15), Stage::Ship);
        let AnswerRead::Bound(answer) =
            Gates::read_answer(root, PhaseId::new(15), Stage::Ship, &gate)
        else {
            panic!("the live gate holds the answer gate_respond wrote");
        };
        assert!(answer.response.approved);
        assert_eq!(answer.response.note.as_deref(), Some("lgtm"));
        let event = devflow_core::events::last_event_for_phase(root, PhaseId::new(15)).unwrap();
        assert_eq!(event["event"], "gate_response_written");
        assert_eq!(event["stage"], "ship");
    }

    #[test]
    fn gate_respond_requires_stage_when_ambiguous_and_errors_when_none_open() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let err = gate_respond(root, PhaseId::new(15), None, true, None, None).unwrap_err();
        assert!(err.to_string().contains("no open gate"), "{err}");

        Gates::write_gate(root, PhaseId::new(15), Stage::Validate, "a").unwrap();
        Gates::write_gate(root, PhaseId::new(15), Stage::Ship, "b").unwrap();
        let err = gate_respond(
            root,
            PhaseId::new(15),
            None,
            false,
            Some("nope".into()),
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("--stage"), "{err}");

        // Explicit Ship disambiguates. Non-Ship gates need a confirmed
        // waiter before a response can be published.
        gate_respond(
            root,
            PhaseId::new(15),
            Some(Stage::Ship),
            false,
            Some("gaps".into()),
            None,
        )
        .unwrap();
        assert!(
            !gate_answer_files(root, PhaseId::new(15), Stage::Ship).is_empty(),
            "explicit-stage rejection must land"
        );
        assert!(gate_answer_files(root, PhaseId::new(15), Stage::Validate).is_empty());
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
        let path = Gates::gate_path(root, phase, stage);
        let mut gate: GateFile =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        gate.timestamp = now.saturating_sub(age_secs).to_string();
        std::fs::write(path, serde_json::to_string_pretty(&gate).unwrap()).unwrap();
    }

    /// Task 2 behavior: `--dry-run` computes and prints every decision but
    /// writes nothing: an aged gate with a live waiter, which the sweep would
    /// otherwise answer, stays open with no response file. The second sweep is
    /// the control: without `--dry-run` the same gate is reaped.
    #[test]
    fn gate_sweep_dry_run_does_not_write_a_response() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(30), Stage::Ship);
        let (_, registration) = Gates::open_registered(root, phase, stage, "ctx").unwrap();
        backdate_gate(root, phase, stage, aged_past_threshold());

        gate_sweep(None, true, Some(root.to_path_buf()), false).unwrap();

        assert!(
            gate_answer_files(root, phase, stage).is_empty(),
            "dry-run must never write a response file"
        );
        let open = Gates::list_open(root);
        assert_eq!(open.len(), 1, "dry-run must leave the gate open");

        gate_sweep(None, false, Some(root.to_path_buf()), false).unwrap();
        assert_eq!(
            gate_answer_files(root, phase, stage).len(),
            1,
            "the control: without --dry-run the same gate and waiter are reaped"
        );
        registration.finish();
    }

    /// Task 2 behavior: a gate that already has a response is a benign race
    /// (a human or `--yes-ship` may have answered it between `list_open`
    /// and `reap`) — the sweep must return `Ok` and leave the existing
    /// response byte-for-byte untouched, never overwrite it.
    #[test]
    fn gate_sweep_skips_already_responded_gate_without_clobbering() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(31), Stage::Ship);
        let (opened, registration) = Gates::open_registered(root, phase, stage, "ctx").unwrap();
        backdate_gate(root, phase, stage, aged_past_threshold());
        let listed_before_the_answer = listed_gate(root, phase);
        let response = GateResponse {
            approved: true,
            note: None,
            responded_by: Some("human".into()),
        };
        let path = Gates::answer(root, phase, stage, &opened.gate_id, &response).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();

        let result = gate_sweep(None, false, Some(root.to_path_buf()), false);
        let raced = sweep_one_gate(root, &listed_before_the_answer, 1, 0, false).unwrap();

        assert!(result.is_ok(), "an already-answered gate must not error");
        assert_eq!(raced, SweepStep::Skipped, "the race is a skip");
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            before, after,
            "the pre-existing response must be byte-identical afterwards"
        );
        registration.finish();
    }

    /// Task 2 behavior (T-23-43 audit trail): a successful reap emits
    /// `gate_reaped` in the sweep's own process — one of the two
    /// independent, attributed records a reaped gate leaves.
    #[test]
    fn gate_sweep_emits_gate_reaped_event_on_reap() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(32);
        let (_, registration) = Gates::open_registered(root, phase, Stage::Ship, "ctx").unwrap();
        backdate_gate(root, phase, Stage::Ship, aged_past_threshold());

        gate_sweep(None, false, Some(root.to_path_buf()), false).unwrap();

        let event = devflow_core::events::last_event_for_phase(root, phase).unwrap();
        assert_eq!(event["event"], "gate_reaped");
        assert_eq!(event["stage"], "ship");
        registration.finish();
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
    /// faking `/proc`. Left uncovered by assertion rather than faked. Covered
    /// by source reasoning instead: `reap_stray_candidates`'s age check —
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

    /// V-30: a held record with no start time cannot be confirmed, so `stop` refuses and
    /// signals nothing. `(pid, starttime)` is the guard because a cmdline check races a
    /// forked child that has not finished `execve` (999.47/D-13). The child is a real,
    /// live `sleep`, so a `stop` that signalled it would be seen.
    ///
    /// This is one of `stop_via_holder`'s three fail-closed arms. The others:
    /// - a recorded start time that no longer matches, covered by
    ///   `stop_e2e::stop_refuses_to_signal_a_holder_whose_start_time_does_not_match`.
    /// - a held record that does not read. `liveness::live_holder` returns `InvalidData`
    ///   for an empty record, which a reader racing the rewrite can get, so that arm is
    ///   reachable from a black box and the second half here reaches it.
    #[test]
    fn stop_refuses_a_holder_record_with_no_start_time() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(200);
        workflow::save_state(&State::new(
            phase,
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        ))
        .unwrap();
        let (mut child, _) = sleep_child_with_start();
        let pid = child.id();
        let hold = hold_phase_with_record(root, phase, pid, None);

        let no_start = stop_with_an_empty_cache(root, phase);
        let live_after_no_start = devflow_core::test_support::process_is_live(pid);
        std::fs::write(liveness::phase_holder_path(root, phase), "").unwrap();
        let unreadable = stop_with_an_empty_cache(root, phase);
        let live_after_unreadable = devflow_core::test_support::process_is_live(pid);
        drop(hold);
        let _ = child.kill();
        let _ = child.wait();

        let message = no_start.expect_err("no start time refuses").to_string();
        assert!(
            message.contains(&pid.to_string()),
            "names the pid: {message}"
        );
        assert!(message.contains("records no start time"), "{message}");
        assert!(live_after_no_start, "the child is not signalled");
        let message = unreadable.expect_err("an empty record refuses").to_string();
        assert!(message.contains("could not be read back"), "{message}");
        assert!(live_after_unreadable, "the child is not signalled");
        assert!(
            !workflow::load_state(root, phase).unwrap().stopped,
            "the state is not marked stopped"
        );
    }

    /// V-30: with no process holding the file, nothing is signalled, even though the
    /// record names a live process with its correct start time: the record a held file
    /// would have `stop` signal. No file at all is the other case.
    #[test]
    fn stop_signals_nothing_when_no_process_holds_the_phase() {
        const NAME: &str = "commands::tests::stop_signals_nothing_when_no_process_holds_the_phase";
        // A released flock could still read held while another test forks.
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = tempfile::tempdir().unwrap();
            let path = std::env::var_os("PATH").expect("PATH is set");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[("PATH", path.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        for planted in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(202);
            workflow::save_state(&State::new(
                phase,
                AgentKind::Claude,
                Mode::Auto,
                root.to_path_buf(),
            ))
            .unwrap();
            let (mut child, start) = sleep_child_with_start();
            let pid = child.id();
            if planted {
                drop(hold_phase_with_record(root, phase, pid, Some(start)));
                assert_eq!(
                    liveness::probe(&liveness::phase_holder_path(root, phase)).unwrap(),
                    liveness::Held::Abandoned,
                    "the file is there and unheld"
                );
            }

            let result = stop_with_an_empty_cache(root, phase);
            let child_alive = devflow_core::test_support::process_is_live(pid);
            let _ = child.kill();
            let _ = child.wait();

            result.unwrap_or_else(|err| panic!("planted={planted}: nothing to stop: {err}"));
            assert!(child_alive, "planted={planted}: nothing is signalled");
            assert!(
                workflow::load_state(root, phase).unwrap().stopped,
                "planted={planted}: `stopped` is recorded as today"
            );
        }
    }

    /// Task 3 behavior: no open gate, no lock file, and no persisted state
    /// at all — a phase that was never started — is a successful no-op.
    #[test]
    fn stop_is_a_success_no_op_with_no_gate_and_no_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        stop_with_an_empty_cache(root, PhaseId::new(201))
            .expect("stop against nothing present must succeed");
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
    ///
    /// **46-02 extension (VALID-01).** The four arms above are all spellings
    /// the `refs/heads/` prefix already defeats. Revision **suffix** syntax
    /// survives that prefix, and `rev-parse --verify` — a revision PARSER
    /// being used as a ref existence check — accepts it. Measured against a
    /// faithful replica of `base_branch_fixture` (46-RESEARCH.md's truth
    /// table, re-measured under git 2.55.0 at execution): `workspace/example~1`,
    /// `develop@{0}` and `develop^{}` all exit 0 under
    /// `rev-parse --verify --quiet refs/heads/<v>` and 1 under
    /// `show-ref --verify --quiet refs/heads/<v>`. Those three arms are the
    /// live bug (46-CONTEXT.md D-07).
    ///
    /// The remaining arms pin D-08's **two distinguishable messages**. A
    /// well-formed but merely MISSING branch keeps today's message verbatim,
    /// `git branch {base} origin/{base}` advice included; a MALFORMED value
    /// gets its own message and must NOT repeat that advice, which would
    /// instruct the operator to create a branch literally named `develop@{0}`.
    /// The `contains("git branch …")` and `!contains("git branch")` assertions
    /// are a PAIR: drop either half and two messages can silently collapse
    /// into one while the test stays green. Extended in place rather than
    /// duplicated because this test already carries its own accept-arm
    /// negative control (D-10).
    ///
    /// `develop~1` is deliberately NOT among the arms. `base_branch_fixture`
    /// creates `develop` at the fixture's ROOT commit, so `develop~1` has no
    /// parent to resolve, is already refused today, and would pass identically
    /// before and after the fix — proving nothing. ROADMAP Success Criterion 2
    /// names `refs/heads/develop~1`; the `refs/heads/` spelling class is
    /// already covered by the `refs/heads/main` arm above, and
    /// `workspace/example~1` is the substitution that actually discriminates.
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

        // 46-02 / D-07: revision SUFFIX syntax, which survives the
        // `refs/heads/` prefix the helper applies. Unlike the arms above,
        // these are bypasses only AFTER the prefix — so the prove-the-bypass
        // probe runs on the QUALIFIED form the helper actually builds. A bare
        // probe would be asserting a different fact.
        for spelling in ["workspace/example~1", "develop@{0}", "develop^{}"] {
            let qualified = format!("refs/heads/{spelling}");
            let bare = devflow_core::test_support::git_command(root)
                .args(["rev-parse", "--verify", "--quiet", qualified.as_str()])
                .output()
                .expect("rev-parse");
            assert!(
                bare.status.success(),
                "fixture is wrong: `{qualified}` must resolve as a commit-ish today"
            );
            let refusal = ensure_base_is_a_local_branch(root, spelling);
            assert!(
                refusal.is_err(),
                "`{spelling}` is revision syntax surviving the `refs/heads/` \
                 prefix, not a local branch, and must be refused"
            );
            let msg = refusal.unwrap_err().to_string();
            assert!(
                msg.contains(spelling),
                "the refusal must name the offending value `{spelling}`: {msg}"
            );
            // D-08, first half of the discriminator.
            assert!(
                !msg.contains("git branch"),
                "a revision-syntax refusal must not repeat the create-a-branch \
                 advice, which would say to create `{spelling}`: {msg}"
            );
        }

        // EDGE VALID-01/empty — MEASURED, not predicted (46-02 Task 1 Step 0,
        // git 2.55.0, replica fixture). An empty base yields the qualified
        // string `refs/heads/`, on which `show-ref --verify --quiet` exits 1
        // AND `check-ref-format` also exits 1 (trailing slash). It therefore
        // lands in the MALFORMED arm. Asserting `contains("")` would be
        // trivially true and prove nothing, so the arm is asserted only.
        let empty_refusal = ensure_base_is_a_local_branch(root, "");
        assert!(empty_refusal.is_err(), "an empty base must be refused");
        let empty_msg = empty_refusal.unwrap_err().to_string();
        assert!(
            !empty_msg.contains("git branch"),
            "an empty base is not a well-formed ref name and must land in the \
             malformed arm, which carries no create-a-branch advice: {empty_msg}"
        );

        // D-08, second half of the discriminator, and its negative control: a
        // well-formed but merely MISSING branch keeps today's message with its
        // advice intact. Without this arm every `!contains` above would pass
        // against an implementation that emitted ONE generic message.
        let missing_refusal = ensure_base_is_a_local_branch(root, "nonexistent-xyz");
        assert!(
            missing_refusal.is_err(),
            "`nonexistent-xyz` does not exist and must be refused"
        );
        let missing_msg = missing_refusal.unwrap_err().to_string();
        assert!(
            missing_msg.contains("git branch nonexistent-xyz origin/nonexistent-xyz"),
            "a well-formed but missing branch must keep the create-a-branch \
             advice verbatim: {missing_msg}"
        );
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
            gate_id: String::new(),
        };

        let row = render_all_roots_gate_row(Path::new("/tmp/some-root"), &gate, 10_000);

        assert!(row.contains("42"), "row must still name the phase: {row}");
        assert!(row.contains('?'), "row must render the unknown age: {row}");
    }

    /// 21a / 50-12: `recovery_hints` returns a `resume` hint for a phase
    /// whose supervisor is GONE while it is not stopped, additionally a
    /// `ship` hint when its Ship gate is pending (D-06), `stop` then `resume` for
    /// a STALE socket, and nothing for an ALIVE or unlaunched phase.
    #[test]
    fn recovery_hints_includes_resume_for_gone_while_active() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new(
            PhaseId::new(7),
            AgentKind::Claude,
            Mode::Auto,
            dir.path().to_path_buf(),
        );
        state.verification_baseline_captured = true;

        let hints = recovery_hints(&state, &PhaseLiveness::GoneWhileActive);

        assert_eq!(hints, vec!["devflow resume --phase 7".to_string()]);
    }

    /// Whether a hint list sends the operator through a hand-run `advance`, whose gate open
    /// sets an earlier answer aside (D-06).
    fn hint_routes_an_answer_through_advance(hints: &[String]) -> bool {
        hints.iter().any(|hint| hint.contains("devflow advance"))
    }

    /// V-33, D-06: a gate-pending phase whose supervisor is gone is repaired by `resume`, which
    /// asks again, plus `devflow ship` at Ship, the one stage whose stored answer it applies.
    /// No liveness, gate state or stage names `advance`.
    #[test]
    fn recovery_hints_for_a_gone_gate_pending_phase_name_resume_or_ship_not_advance() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new(
            PhaseId::new(7),
            AgentKind::Claude,
            Mode::Auto,
            dir.path().to_path_buf(),
        );
        state.gate_pending = true;

        state.stage = Stage::Code;
        assert_eq!(
            recovery_hints(&state, &PhaseLiveness::GoneWhileActive),
            vec!["devflow resume --phase 7".to_string()]
        );
        state.stage = Stage::Ship;
        assert_eq!(
            recovery_hints(&state, &PhaseLiveness::GoneWhileActive),
            vec![
                "devflow resume --phase 7".to_string(),
                "devflow ship --phase 7".to_string(),
            ]
        );

        let every_liveness = [
            PhaseLiveness::Alive {
                owner: "gate_wait".to_string(),
                launch: "stream".to_string(),
            },
            PhaseLiveness::Stale,
            PhaseLiveness::Gone,
            PhaseLiveness::GoneWhileActive,
            PhaseLiveness::NotLaunched,
            PhaseLiveness::LaunchPending {
                launch_id: "launch".to_string(),
                published: true,
            },
            PhaseLiveness::ProbeFailed("no reply".to_string()),
        ];
        for liveness in &every_liveness {
            for gate_pending in [false, true] {
                for stage in [Stage::Code, Stage::Ship] {
                    state.gate_pending = gate_pending;
                    state.stage = stage;
                    let hints = recovery_hints(&state, liveness);
                    assert!(
                        !hint_routes_an_answer_through_advance(&hints),
                        "{liveness:?} gate_pending={gate_pending} {stage}: {hints:?}"
                    );
                }
            }
        }

        let hints_before_this_rule = [
            "devflow resume --phase 7".to_string(),
            "devflow advance --phase 7".to_string(),
        ];
        assert!(
            hint_routes_an_answer_through_advance(&hints_before_this_rule),
            "the check must catch the list that sent an answer through advance"
        );
    }

    #[test]
    fn recovery_hints_names_stop_then_resume_for_stale() {
        let dir = tempfile::tempdir().unwrap();
        let state = State::new(
            PhaseId::new(7),
            AgentKind::Claude,
            Mode::Auto,
            dir.path().to_path_buf(),
        );

        assert_eq!(
            recovery_hints(&state, &PhaseLiveness::Stale),
            vec![
                "devflow stop --phase 7".to_string(),
                "devflow resume --phase 7".to_string(),
            ]
        );
    }

    #[test]
    fn recovery_hints_empty_for_alive_and_unlaunched() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new(
            PhaseId::new(7),
            AgentKind::Claude,
            Mode::Auto,
            dir.path().to_path_buf(),
        );
        state.verification_baseline_captured = true;
        let alive = PhaseLiveness::Alive {
            owner: "agent_running".to_string(),
            launch: "stream".to_string(),
        };

        for liveness in [alive, PhaseLiveness::NotLaunched] {
            assert!(
                recovery_hints(&state, &liveness).is_empty(),
                "{liveness:?} needs no repair"
            );
        }
    }

    /// Once `stop` has ended a run nothing else continues it, so `status` names `resume`,
    /// with or without a record. A stopped phase whose supervisor still answers does not.
    #[test]
    fn recovery_hints_names_resume_for_a_stopped_phase() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new(
            PhaseId::new(7),
            AgentKind::Claude,
            Mode::Auto,
            dir.path().to_path_buf(),
        );
        state.verification_baseline_captured = true;
        state.stopped = true;

        for liveness in [PhaseLiveness::Gone, PhaseLiveness::NotLaunched] {
            assert_eq!(
                recovery_hints(&state, &liveness),
                vec!["devflow resume --phase 7".to_string()],
                "{liveness:?}"
            );
        }
        let alive = PhaseLiveness::Alive {
            owner: "agent_running".to_string(),
            launch: "stream".to_string(),
        };
        assert!(recovery_hints(&state, &alive).is_empty(), "control");
    }

    /// A stopped run whose recorded delivery canary refuses its agent names `resume
    /// --agent <other>`, not the plain `resume` that canary would refuse. Control: the same
    /// run with a recorded `Confirmed` names the plain `resume`.
    #[test]
    fn recovery_hints_name_another_agent_when_the_recorded_canary_refuses_the_run() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new(
            PhaseId::new(7),
            AgentKind::Antigravity,
            Mode::Auto,
            dir.path().to_path_buf(),
        );
        state.verification_baseline_captured = true;
        state.stopped = true;
        state.stage = Stage::Code;

        state.canary = Some(devflow_core::canary::CanaryOutcome::Confirmed);
        assert_eq!(
            recovery_hints(&state, &PhaseLiveness::Gone),
            vec!["devflow resume --phase 7".to_string()],
            "control: a recorded Confirmed keeps the plain resume"
        );

        for refusal in [
            devflow_core::canary::CanaryOutcome::Absent,
            devflow_core::canary::CanaryOutcome::Unverified("no binary".to_string()),
        ] {
            state.canary = Some(refusal.clone());
            for liveness in [PhaseLiveness::Gone, PhaseLiveness::NotLaunched] {
                assert_eq!(
                    recovery_hints(&state, &liveness),
                    vec!["devflow resume --phase 7 --agent <other>".to_string()],
                    "{refusal:?} {liveness:?}"
                );
            }
        }
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
            serde_json::json!({"stage": "code", "agent": "claude", "supervisor_pid": 1}),
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

    /// Run `body` on a new thread without `CAP_DAC_OVERRIDE` and
    /// `CAP_DAC_READ_SEARCH` in its effective set (WINDOWS.md #19). Either
    /// lets uid 0 list a mode-0o000 directory, so the pinned container (root)
    /// could not produce an unlistable socket directory; without them root is
    /// refused like any other owner. Capabilities are per-thread and
    /// `capset(2)` changes only the calling thread's, so the harness keeps
    /// its own, and they end with the thread. A non-root thread holds neither
    /// capability, so nothing changes there.
    fn without_dac_override<T: Send>(body: impl FnOnce() -> T + Send) -> T {
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    drop_dac_override_on_this_thread();
                    body()
                })
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        })
    }

    /// Clear `CAP_DAC_OVERRIDE` and `CAP_DAC_READ_SEARCH` from the calling
    /// thread's effective capability set; the permitted set is kept.
    #[cfg(target_os = "linux")]
    fn drop_dac_override_on_this_thread() {
        // `<linux/capability.h>`, version 3: one header, two data structs.
        #[repr(C)]
        struct CapHeader {
            version: u32,
            pid: libc::c_int,
        }
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct CapData {
            effective: u32,
            permitted: u32,
            inheritable: u32,
        }
        const LINUX_CAPABILITY_VERSION_3: u32 = 0x2008_0522;
        const CAP_DAC_OVERRIDE: u32 = 1;
        const CAP_DAC_READ_SEARCH: u32 = 2;

        let mut header = CapHeader {
            version: LINUX_CAPABILITY_VERSION_3,
            pid: 0,
        };
        let mut data = [CapData {
            effective: 0,
            permitted: 0,
            inheritable: 0,
        }; 2];
        // SAFETY: `header` and `data` have capget(2)'s version-3 layout (one
        // header, two data structs); pid 0 is the calling thread.
        let got = unsafe { libc::syscall(libc::SYS_capget, &raw mut header, data.as_mut_ptr()) };
        assert_eq!(got, 0, "capget: {}", std::io::Error::last_os_error());
        data[0].effective &= !((1 << CAP_DAC_OVERRIDE) | (1 << CAP_DAC_READ_SEARCH));
        // SAFETY: as above; capset(2) with pid 0 changes only this thread.
        let set = unsafe { libc::syscall(libc::SYS_capset, &raw const header, data.as_ptr()) };
        assert_eq!(set, 0, "capset: {}", std::io::Error::last_os_error());
    }

    /// No per-thread capabilities to drop off Linux: there a root run still
    /// lists a 0o000 directory, and the tests that use this fail their own
    /// precondition rather than pass without asserting.
    #[cfg(not(target_os = "linux"))]
    fn drop_dac_override_on_this_thread() {}

    /// Unit tests for the pure `doctor` reconciliation core (18a). Each test
    /// builds a `PhaseFacts` directly — no repository, no I/O — proving
    /// `reconcile_phase` is a predicate over facts alone.
    #[cfg(test)]
    mod doctor_reconciliation {
        use super::*;

        /// The launch id every fixture's supervisor record carries.
        const SUPERVISOR_LAUNCH_ID: &str = "0123456789abcdef";

        /// A path a damaged gate file could name where its gate id belongs.
        const PATH_AS_GATE_ID: &str = "/home/someone/.devflow/gates/x";

        /// Every string anywhere inside `value`.
        fn strings_in(value: &serde_json::Value, out: &mut Vec<String>) {
            match value {
                serde_json::Value::String(text) => out.push(text.clone()),
                serde_json::Value::Array(items) => {
                    items.iter().for_each(|item| strings_in(item, out));
                }
                serde_json::Value::Object(map) => {
                    map.values().for_each(|item| strings_in(item, out));
                }
                _ => {}
            }
        }

        /// 50-12: a STALE supervisor, one GONE while its phase is not
        /// stopped, and a failed probe are each a `Problem` naming the
        /// launch, never its socket path; STALE's repair stops then resumes,
        /// GONE's resumes, and a failed probe points at `devflow status`
        /// (which prints the operator's own socket path).
        #[test]
        fn check_supervisor_reports_stale_and_gone_while_active() {
            let stale = check_supervisor(&PhaseFacts {
                supervisor: PhaseLiveness::Stale,
                ..agreeing_facts(PhaseId::new(21))
            })
            .expect("a STALE supervisor is a finding");
            assert_eq!(stale.severity, Severity::Problem);
            assert_eq!(
                stale.repair.as_deref(),
                Some("devflow stop --phase 21 && devflow resume --phase 21")
            );
            assert!(
                stale.detail.contains("STALE") && stale.detail.contains(SUPERVISOR_LAUNCH_ID),
                "{}",
                stale.detail
            );

            let gone = check_supervisor(&PhaseFacts {
                supervisor: PhaseLiveness::GoneWhileActive,
                ..agreeing_facts(PhaseId::new(22))
            })
            .expect("GONE while active is a finding");
            assert_eq!(gone.severity, Severity::Problem);
            assert_eq!(gone.repair.as_deref(), Some("devflow resume --phase 22"));
            assert!(gone.detail.contains("GONE"), "{}", gone.detail);

            let record = devflow_core::state::SupervisorRecord::new(
                PathBuf::from("/home/someone/.cache/devflow/sock/x.sock"),
                SUPERVISOR_LAUNCH_ID.to_string(),
                10,
                20,
                "boot".to_string(),
            );
            let failed = phase_liveness(
                Some(&record),
                Some(Err(devflow_core::supervisor::ProbeError::NotASocket(
                    record.socket_path.clone(),
                ))),
                false,
            );
            let failed = check_supervisor(&PhaseFacts {
                supervisor: failed,
                ..agreeing_facts(PhaseId::new(23))
            })
            .expect("a failed probe is a finding");
            assert_eq!(failed.severity, Severity::Problem);
            assert!(
                failed.detail.contains("not a socket") && !failed.detail.contains('/'),
                "a failed probe names its kind and no path: {}",
                failed.detail
            );
            assert_eq!(failed.repair.as_deref(), Some("devflow status"));

            let reconciled = reconcile_phase(&PhaseFacts {
                supervisor: PhaseLiveness::Stale,
                ..agreeing_facts(PhaseId::new(21))
            });
            assert!(
                reconciled.iter().any(|f| f.detail == stale.detail),
                "reconcile_phase runs check_supervisor"
            );
        }

        /// 50-12 control: ALIVE, GONE while stopped, and NotLaunched are not
        /// findings, so the checks above can only fire for the three states
        /// that need a repair.
        #[test]
        fn check_supervisor_is_silent_when_alive_or_stopped() {
            let alive = agreeing_facts(PhaseId::new(24));
            assert!(check_supervisor(&alive).is_none());
            assert!(reconcile_phase(&alive).is_empty());
            let stopped = PhaseFacts {
                supervisor: PhaseLiveness::Gone,
                stopped: true,
                ..agreeing_facts(PhaseId::new(25))
            };
            assert!(check_supervisor(&stopped).is_none());
            let unlaunched = PhaseFacts {
                supervisor: PhaseLiveness::NotLaunched,
                supervisor_launch_id: None,
                ..agreeing_facts(PhaseId::new(26))
            };
            assert!(check_supervisor(&unlaunched).is_none());
        }

        /// 50-12 (T-50-32): `doctor --json` phase facts carry the
        /// supervisor's liveness, owner, launch and launch id, and no string
        /// in the reconciliation section holds a path.
        #[test]
        fn doctor_json_reports_supervisor_liveness_without_paths() {
            let gate_id = GateId::new().to_string();
            let record = devflow_core::state::SupervisorRecord::new(
                PathBuf::from("/home/someone/.cache/devflow/sock/x.sock"),
                SUPERVISOR_LAUNCH_ID.to_string(),
                10,
                20,
                "boot".to_string(),
            );
            let failed = phase_liveness(
                Some(&record),
                Some(Err(devflow_core::supervisor::ProbeError::NotASocket(
                    record.socket_path.clone(),
                ))),
                false,
            );
            let pending = PhaseLiveness::LaunchPending {
                launch_id: SUPERVISOR_LAUNCH_ID.to_string(),
                published: false,
            };
            let facts = vec![
                agreeing_facts(PhaseId::new(27)),
                PhaseFacts {
                    supervisor: failed,
                    pending: Some((SUPERVISOR_LAUNCH_ID.to_string(), pending)),
                    ..agreeing_facts(PhaseId::new(28))
                },
                PhaseFacts {
                    waiter: WaiterStatus::Live,
                    waiter_gate: Some((Stage::Define, gate_id.clone())),
                    open_gate_stages: vec![Stage::Define],
                    ..agreeing_facts(PhaseId::new(29))
                },
                PhaseFacts {
                    waiter_gate: Some((Stage::Define, PATH_AS_GATE_ID.to_string())),
                    open_gate_stages: vec![Stage::Define],
                    ..agreeing_facts(PhaseId::new(30))
                },
            ];
            let body = render_reconciliation_json(&facts);
            let entries = body.as_array().expect("an array");
            let alive = &entries[0]["supervisor"];
            assert_eq!(alive["liveness"], "alive", "{body}");
            assert_eq!(alive["owner"], "agent_running", "{body}");
            assert_eq!(alive["launch"], "stream", "{body}");
            assert_eq!(alive["launch_id"], SUPERVISOR_LAUNCH_ID, "{body}");
            let failed = &entries
                .iter()
                .find(|entry| entry["phase"] == 28)
                .expect("phase 28's finding")["supervisor"];
            assert_eq!(failed["liveness"], "probe_failed", "{body}");
            assert_eq!(failed["pending"]["liveness"], "launch_pending", "{body}");
            assert!(failed["owner"].is_null(), "{body}");
            let mut strings = Vec::new();
            strings_in(&body, &mut strings);
            assert!(!strings.is_empty(), "the scan must see strings: {body}");
            assert!(
                strings.contains(&gate_id),
                "the scan must read a gate id, not only nulls: {strings:?}"
            );
            assert!(
                strings.iter().all(|text| !text.contains('/')),
                "no string in the reconciliation section may hold a path: {strings:?}"
            );
        }

        /// A gate file's id is unchecked, so one that is no gate id reads `null`: the
        /// JSON cannot carry a path in it (T-50.1-156). The stage still names the gate.
        #[test]
        fn doctor_json_gives_a_refused_gate_id_as_null() {
            let facts = vec![PhaseFacts {
                waiter_gate: Some((Stage::Define, PATH_AS_GATE_ID.to_string())),
                open_gate_stages: vec![Stage::Define],
                ..agreeing_facts(PhaseId::new(31))
            }];
            let body = render_reconciliation_json(&facts);
            let entries = body.as_array().expect("an array");
            assert!(!entries.is_empty(), "{body}");
            for entry in entries {
                assert_eq!(
                    entry["gate_waiter"],
                    serde_json::json!({"stage": "define", "gate_id": null, "status": "absent"}),
                    "{body}"
                );
            }
        }

        /// A phase with several findings repeats one `gate_waiter`, so a consumer that
        /// reads any entry for the phase reads the phase's fact.
        #[test]
        fn doctor_json_repeats_the_gate_waiter_on_every_finding_of_a_phase() {
            let gate_id = GateId::new().to_string();
            let facts = vec![PhaseFacts {
                agent_pid: Some(999_999),
                agent_alive: false,
                waiter_gate: Some((Stage::Code, gate_id.clone())),
                open_gate_stages: vec![Stage::Code],
                ..agreeing_facts(PhaseId::new(32))
            }];
            let body = render_reconciliation_json(&facts);
            let entries = body.as_array().expect("an array");
            assert!(entries.len() >= 2, "needs several findings: {body}");
            for entry in entries {
                assert_eq!(
                    entry["gate_waiter"],
                    serde_json::json!({"stage": "code", "gate_id": gate_id, "status": "absent"}),
                    "{body}"
                );
            }
        }

        /// A short cache dir directly under `/tmp`, so every socket path
        /// below it fits `sun_path`.
        pub(super) fn short_cache() -> tempfile::TempDir {
            tempfile::Builder::new()
                .prefix("dfs")
                .tempdir_in("/tmp")
                .expect("create a short cache dir under /tmp")
        }

        /// Publish the socket for `(root, phase, launch_id)` under `cache`.
        /// Dropping the returned listener leaves the socket STALE.
        pub(super) fn publish_launch(
            cache: &Path,
            root: &Path,
            phase: u32,
            launch_id: &str,
        ) -> (supervisor::SocketPath, std::os::unix::net::UnixListener) {
            let socket =
                supervisor::SocketPath::for_launch(cache, root, PhaseId::new(phase), launch_id)
                    .expect("socket path fits sun_path");
            let listener = supervisor::publish(&socket).expect("publish the socket");
            (socket, listener)
        }

        /// A complete sidecar for `(root, phase, launch_id)`, as a supervisor
        /// writes it.
        pub(super) fn write_sidecar_for(
            socket: &supervisor::SocketPath,
            root: &Path,
            phase: u32,
            id: &str,
        ) {
            let sidecar = serde_json::json!({
                "launch_id": id,
                "project_root": root,
                "phase": PhaseId::new(phase),
                "stage": "code",
                "agent": "claude",
                "launch": "stream",
                "agent_pid": 1,
                "agent_pgid": 1,
                "agent_start_time": 0,
                "boot_id": "boot",
                "supervisor_pid": 1,
                "supervisor_start_time": 0,
                "created_at": "2026-10-17T00:00:00Z",
            });
            std::fs::write(socket.sidecar_path(), sidecar.to_string()).unwrap();
            supervisor::read_sidecar(&socket.sidecar_path()).expect("the planted sidecar parses");
        }

        /// A loadable state for `phase` whose supervisor record carries
        /// `launch_id`.
        pub(super) fn save_state_with_launch(root: &Path, phase: u32, launch_id: &str) {
            let mut state = State::new(
                PhaseId::new(phase),
                AgentKind::Claude,
                Mode::Auto,
                root.to_path_buf(),
            );
            state.supervisor = Some(devflow_core::state::SupervisorRecord::new(
                PathBuf::from("/elsewhere.sock"),
                launch_id.to_string(),
                1,
                0,
                "boot".to_string(),
            ));
            workflow::save_state(&state).unwrap();
        }

        /// Answers every ping on `listener` as a supervisor would, until
        /// [`PingResponder::stop`].
        struct PingResponder {
            done: std::sync::Arc<std::sync::atomic::AtomicBool>,
            socket: PathBuf,
            thread: Option<std::thread::JoinHandle<()>>,
        }

        impl PingResponder {
            fn start(
                socket: &supervisor::SocketPath,
                listener: std::os::unix::net::UnixListener,
                launch_id: &str,
                phase: u32,
            ) -> Self {
                use std::io::{BufRead as _, Write as _};
                let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let reply = serde_json::json!({
                    "ok": true,
                    "launch_id": launch_id,
                    "phase": phase.to_string(),
                    "stage": "code",
                    "launch": "stream",
                    "owner": "agent_running",
                    "agent_pid": 1,
                    "agent_pgid": 1,
                    "agent_start_time": 0,
                    "boot_id": null,
                    "supervisor_pid": 1,
                });
                let stop = std::sync::Arc::clone(&done);
                let thread = std::thread::spawn(move || {
                    for stream in listener.incoming() {
                        if stop.load(std::sync::atomic::Ordering::SeqCst) {
                            break;
                        }
                        let Ok(stream) = stream else { continue };
                        let mut line = String::new();
                        if std::io::BufReader::new(&stream)
                            .read_line(&mut line)
                            .is_ok()
                        {
                            let _ = (&stream).write_all(format!("{reply}\n").as_bytes());
                        }
                    }
                });
                Self {
                    done,
                    socket: socket.as_path().to_path_buf(),
                    thread: Some(thread),
                }
            }

            fn stop(&mut self) {
                self.done.store(true, std::sync::atomic::Ordering::SeqCst);
                // Wake the blocked accept so the thread sees the flag.
                let _ = std::os::unix::net::UnixStream::connect(&self.socket);
                if let Some(thread) = self.thread.take() {
                    thread.join().expect("the ping responder thread");
                }
            }
        }

        impl Drop for PingResponder {
            fn drop(&mut self) {
                self.stop();
            }
        }

        /// Every file under `dir` as (name, length, mtime), sorted.
        fn snapshot(dir: &Path) -> Vec<(String, u64, std::time::SystemTime)> {
            let mut files: Vec<_> = std::fs::read_dir(dir)
                .expect("list the socket dir")
                .map(|entry| {
                    let entry = entry.unwrap();
                    let meta = std::fs::symlink_metadata(entry.path()).unwrap();
                    (
                        entry.file_name().to_string_lossy().into_owned(),
                        meta.len(),
                        meta.modified().unwrap(),
                    )
                })
                .collect();
            files.sort();
            files
        }

        /// 50-12 (D-06, R5-I3, T-50-31, T-50-51, T-50-84): doctor's sweep
        /// lists the ALIVE and STALE sockets that no loadable state of this
        /// root claims — no state, an unparsable state, a superseded launch,
        /// a missing or broken sidecar attributed by file name — and,
        /// machine-wide, those whose project root is gone. Each names the
        /// `--launch` teardown, never the bare stop. Another root's sockets
        /// and the ones a state claims are not listed, and nothing under
        /// `sock/` changes.
        #[test]
        fn doctor_reports_an_orphan_socket_without_removing_it() {
            let cache = short_cache();
            let cache = cache.path();
            let repo = tempfile::tempdir().unwrap();
            let root = repo.path();
            let other = tempfile::tempdir().unwrap();
            let other_root = other.path();
            let deleted = tempfile::tempdir().unwrap();
            let deleted_root = deleted.path().to_path_buf();

            // (a) STALE, sidecar for phase 7, whose state file is corrupt.
            let (a, _) = publish_launch(cache, root, 7, "aaaa0000aaaa0007");
            write_sidecar_for(&a, root, 7, "aaaa0000aaaa0007");
            let state7 = workflow::state_path(root, PhaseId::new(7));
            std::fs::create_dir_all(state7.parent().unwrap()).unwrap();
            std::fs::write(&state7, "{").unwrap();
            // (b) STALE, superseded: phase 8's state names its successor.
            let (b, _) = publish_launch(cache, root, 8, "aaaa0000aaaa0008");
            write_sidecar_for(&b, root, 8, "aaaa0000aaaa0008");
            save_state_with_launch(root, 8, "ffff0000ffff0008");
            // (c) STALE, another root that exists: not ours.
            let (c, _) = publish_launch(cache, other_root, 7, "aaaa0000aaaa000c");
            write_sidecar_for(&c, other_root, 7, "aaaa0000aaaa000c");
            // (d) STALE, a project root deleted after the launch.
            let (d, _) = publish_launch(cache, &deleted_root, 5, "aaaa0000aaaa000d");
            write_sidecar_for(&d, &deleted_root, 5, "aaaa0000aaaa000d");
            drop(deleted);
            assert!(!deleted_root.exists(), "the root of (d) is deleted");
            // (e) ALIVE, this root's phase 9, no state, sidecar `{`.
            let (e, listener) = publish_launch(cache, root, 9, "bbbb0000bbbb0000");
            std::fs::write(e.sidecar_path(), "{").unwrap();
            let mut responder = PingResponder::start(&e, listener, "bbbb0000bbbb0000", 9);
            // (f) STALE, this root's phase 10, no sidecar, no state.
            let (_f, _) = publish_launch(cache, root, 10, "cccc0000cccc0000");
            // (g) control: STALE, the other root's phase 9, no sidecar.
            let (_g, _) = publish_launch(cache, other_root, 9, "dddd0000dddd0000");
            // (h) control: STALE, no sidecar, whose state claims it.
            let (_h, _) = publish_launch(cache, root, 11, "eeee0000eeee0000");
            save_state_with_launch(root, 11, "eeee0000eeee0000");
            // (i) control: STALE with a sidecar, whose state claims it.
            let (i, _) = publish_launch(cache, root, 12, "aaaa0000aaaa0012");
            write_sidecar_for(&i, root, 12, "aaaa0000aaaa0012");
            save_state_with_launch(root, 12, "aaaa0000aaaa0012");

            let sock_dir = registry::socket_dir_in(cache);
            let before = snapshot(&sock_dir);
            let findings = collect_orphan_supervisors(root, cache);
            assert_eq!(
                snapshot(&sock_dir),
                before,
                "the sweep must change nothing under sock/"
            );
            responder.stop();

            let mut listed: Vec<(PhaseId, &str, &str, bool)> = findings
                .iter()
                .map(|orphan| {
                    (
                        orphan.finding.phase,
                        orphan.launch_id.as_str(),
                        orphan.liveness,
                        orphan.root_exists,
                    )
                })
                .collect();
            listed.sort();
            assert_eq!(
                listed,
                vec![
                    (PhaseId::new(5), "aaaa0000aaaa000d", "stale", false),
                    (PhaseId::new(7), "aaaa0000aaaa0007", "stale", true),
                    (PhaseId::new(8), "aaaa0000aaaa0008", "stale", true),
                    (PhaseId::new(9), "bbbb0000bbbb0000", "alive", true),
                    (PhaseId::new(10), "cccc0000cccc0000", "stale", true),
                ],
                "exactly (a), (b), (d), (e) and (f) are listed"
            );
            for orphan in &findings {
                let finding = &orphan.finding;
                let repair = finding
                    .repair
                    .as_deref()
                    .expect("every orphan names a repair");
                assert_eq!(
                    repair,
                    format!(
                        "devflow stop --phase {} --launch {}",
                        finding.phase, orphan.launch_id
                    ),
                    "the repair addresses exactly this socket"
                );
                assert!(
                    repair.contains("--launch")
                        && repair != format!("devflow stop --phase {}", finding.phase),
                    "never the bare stop, which would stop the state's own record: {repair}"
                );
                assert!(
                    !finding.detail.contains('/'),
                    "a path-free detail: {}",
                    finding.detail
                );
                assert_eq!(finding.severity, Severity::Problem);
            }
            let detail_of = |id: &str| {
                findings
                    .iter()
                    .find(|orphan| orphan.launch_id == id)
                    .map(|orphan| orphan.finding.detail.clone())
                    .unwrap()
            };
            assert!(detail_of("aaaa0000aaaa000d").contains("no longer exists"));
            assert!(detail_of("bbbb0000bbbb0000").contains("does not parse"));
            assert!(detail_of("cccc0000cccc0000").contains("missing"));
            assert!(detail_of("aaaa0000aaaa0008").contains("another launch"));

            // Control: removing (a)'s files changes the snapshot, so the
            // comparison above can fail.
            supervisor::remove_launch_files(&a).unwrap();
            assert_ne!(
                snapshot(&sock_dir),
                before,
                "the snapshot must see a removal"
            );
        }

        /// 50-12: `supervisor::sweep_in` answers a socket directory it
        /// cannot list with no entries, so doctor checks that case itself and
        /// reports it rather than printing nothing. Controls: an absent and a
        /// listable directory report nothing.
        #[test]
        fn doctor_reports_an_unlistable_socket_dir_instead_of_nothing() {
            use std::os::unix::fs::PermissionsExt as _;
            let cache = tempfile::tempdir().unwrap();
            assert_eq!(socket_dir_unlistable(cache.path()), None, "absent");
            assert_eq!(render_orphan_supervisors_text(&[], None), "");
            let sock = registry::socket_dir_in(cache.path());
            std::fs::create_dir_all(&sock).unwrap();
            assert_eq!(socket_dir_unlistable(cache.path()), None, "listable");

            std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o000)).unwrap();
            // Without DAC override, so uid 0 (the pinned container) is
            // refused too (WINDOWS.md #19).
            let unlistable = without_dac_override(|| socket_dir_unlistable(cache.path()));
            std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o700)).unwrap();
            let why = unlistable.expect("a directory without read permission is unlistable");
            assert_eq!(
                why,
                std::io::ErrorKind::PermissionDenied.to_string(),
                "the 0o000 directory itself refuses the listing"
            );
            assert!(!why.contains('/'), "path-free: {why}");
            let text = render_orphan_supervisors_text(&[], Some(&why));
            assert!(
                text.contains("could not be listed") && text.contains(&why),
                "{text}"
            );
            let body = doctor_json_body(&[], &[], &[], &[], (&[], Some(&why)), &[]);
            assert_eq!(body["orphan_supervisor_sweep_error"], why.as_str());
        }

        /// The comment-stripped body of `fn collect_orphan_supervisors(` in
        /// `source` (cut at its first `mod tests {` line), from that line to
        /// the next line that is exactly `}`; `None` when absent.
        fn orphan_sweep_body(source: &str) -> Option<Vec<&str>> {
            let lines: Vec<&str> = source
                .lines()
                .take_while(|line| !line.contains("mod tests {"))
                .collect();
            let start = lines
                .iter()
                .position(|line| line.contains("fn collect_orphan_supervisors("))?;
            let end = start + lines[start..].iter().position(|line| *line == "}")?;
            Some(
                lines[start..=end]
                    .iter()
                    .copied()
                    .filter(|line| !line.trim_start().starts_with("//"))
                    .collect(),
            )
        }

        /// Why `source`'s `collect_orphan_supervisors` breaks R6-G4: absent
        /// or under 5 lines, an acting call, or no `sweep_for_in(` call.
        fn orphan_sweep_violations(source: &str) -> Vec<String> {
            const ACTING: [&str; 5] = [
                "remove_launch_files",
                "remove_file",
                "teardown_stale",
                "request_shutdown",
                "kill(",
            ];
            let Some(body) = orphan_sweep_body(source) else {
                return vec!["fn collect_orphan_supervisors( not found".to_string()];
            };
            let mut violations = Vec::new();
            if body.len() < 5 {
                violations.push(format!("body has {} lines, under 5", body.len()));
            }
            for line in &body {
                for call in ACTING {
                    if line.contains(call) {
                        violations.push(format!("acting call {call}: {line}"));
                    }
                }
            }
            if !body.iter().any(|line| line.contains("sweep_for_in(")) {
                violations.push("no sweep_for_in( call".to_string());
            }
            violations
        }

        /// 50-12 R6-G4 (T-50-31, T-50-84): doctor's orphan sweep only reads,
        /// and attributes a sidecar-less socket through `sweep_for_in`, so
        /// 50-06's digest rule stays in one place. Planted controls run the
        /// same check.
        #[test]
        fn the_orphan_sweep_only_reads_and_attributes_through_sweep_for_in() {
            let reads = "fn collect_orphan_supervisors(a: &Path, b: &Path) -> Vec<X> {\n    let mut out = Vec::new();\n    for entry in supervisor::sweep_for_in(b, a, p) {\n        out.push(entry);\n    }\n    out\n}\n";
            assert!(
                orphan_sweep_violations(reads).is_empty(),
                "a read-only fn passes"
            );
            let acts = "fn collect_orphan_supervisors(a: &Path, b: &Path) -> Vec<X> {\n    let mut out = Vec::new();\n    for entry in supervisor::sweep_for_in(b, a, p) {\n        supervisor::remove_launch_files(&entry);\n        out.push(entry);\n    }\n    out\n}\n";
            let found = orphan_sweep_violations(acts);
            assert!(
                found.len() == 1 && found[0].contains("remove_launch_files"),
                "an acting call fails: {found:?}"
            );
            let reimplements = "fn collect_orphan_supervisors(a: &Path, b: &Path) -> Vec<X> {\n    let mut out = Vec::new();\n    // sweep_for_in(b, a, p) re-derived below\n    for entry in supervisor::sweep_in(b) {\n        out.push(entry);\n    }\n    out\n}\n";
            let found = orphan_sweep_violations(reimplements);
            assert!(
                found.len() == 1 && found[0].contains("no sweep_for_in"),
                "a commented-out sweep_for_in fails: {found:?}"
            );
            let absent = "fn other() {\n}\n";
            assert_eq!(
                orphan_sweep_violations(absent),
                vec!["fn collect_orphan_supervisors( not found".to_string()]
            );

            let source = std::fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands.rs"),
            )
            .expect("read src/commands.rs");
            assert_eq!(
                orphan_sweep_violations(&source),
                Vec::<String>::new(),
                "collect_orphan_supervisors must only read and must call sweep_for_in"
            );
        }

        /// A fully-agreeing baseline: `reconcile_phase` over this returns
        /// zero findings. Each test overrides only the field(s) needed to
        /// trigger the one check it's proving.
        fn agreeing_facts(phase: PhaseId) -> PhaseFacts {
            PhaseFacts {
                phase,
                stage: Stage::Code,
                gate_pending: false,
                waiter: WaiterStatus::Absent,
                waiter_gate: None,
                agent_pid: Some(4242),
                agent_alive: true,
                supervisor: PhaseLiveness::Alive {
                    owner: "agent_running".to_string(),
                    launch: "stream".to_string(),
                },
                supervisor_launch_id: Some(SUPERVISOR_LAUNCH_ID.to_string()),
                last_event: Some("stage_launched".into()),
                last_launched_stage: Some(Stage::Code),
                open_gate_stages: Vec::new(),
                unprepared_restart: None,
                stored_ship_answer: false,
                feature_branch_exists: true,
                stopped: false,
                pending: None,
                live_agent: None,
                write_lock: workflow::WriteLockStatus::Free,
                write_lock_error: None,
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
                supervisor: PhaseLiveness::NotLaunched,
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
                supervisor: PhaseLiveness::NotLaunched,
                ..agreeing_facts(PhaseId::new(3))
            };
            let findings = reconcile_phase(&facts);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Problem);
            assert!(findings[0].detail.contains("gate open for stage validate"));
            assert_eq!(
                findings[0].repair.as_deref(),
                Some(no_waiter_repair(PhaseId::new(3), Stage::Validate).as_str())
            );
        }

        #[test]
        fn reconcile_phase_flags_open_gate_with_no_waiter_with_the_cli_repair() {
            let phase = PhaseId::new(13);
            let facts = PhaseFacts {
                open_gate_stages: vec![Stage::Validate],
                waiter: WaiterStatus::Absent,
                supervisor: PhaseLiveness::NotLaunched,
                ..agreeing_facts(phase)
            };

            let findings = reconcile_phase(&facts);

            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Problem);
            assert_eq!(
                findings[0].repair.as_deref(),
                Some(no_waiter_repair(phase, Stage::Validate).as_str())
            );
        }

        #[test]
        fn reconcile_phase_open_ship_gate_with_no_waiter_names_ship() {
            let phase = PhaseId::new(14);
            let facts = PhaseFacts {
                open_gate_stages: vec![Stage::Ship],
                waiter: WaiterStatus::Absent,
                ..agreeing_facts(phase)
            };

            let findings = reconcile_phase(&facts);

            assert_eq!(findings.len(), 1);
            assert_eq!(
                findings[0].repair.as_deref(),
                Some(no_waiter_repair(phase, Stage::Ship).as_str())
            );
            assert!(
                findings[0]
                    .repair
                    .as_deref()
                    .is_some_and(|repair| repair.contains("devflow ship --phase 14"))
            );
        }

        #[test]
        fn reconcile_phase_open_gate_with_a_live_waiter_is_not_a_no_waiter_finding() {
            let phase = PhaseId::new(16);
            let facts = PhaseFacts {
                open_gate_stages: vec![Stage::Validate],
                waiter: WaiterStatus::Live,
                ..agreeing_facts(phase)
            };

            let findings = reconcile_phase(&facts);

            assert!(findings.iter().all(|finding| {
                finding.repair.as_deref() != Some(no_waiter_repair(phase, Stage::Validate).as_str())
            }));
        }

        /// Criterion 2 for the doctor repairs that name `resume`: none does while a
        /// supervisor of the phase is live or an agent outlives its supervisor (D-19).
        /// Control: with neither, each one does.
        #[test]
        fn no_repair_names_resume_while_a_supervisor_or_agent_lives() {
            let phase = PhaseId::new(29);
            let gate_pending = |facts: PhaseFacts| PhaseFacts {
                gate_pending: true,
                ..facts
            };
            let dead_agent = |facts: PhaseFacts| PhaseFacts {
                agent_pid: Some(999_999),
                agent_alive: false,
                ..facts
            };
            let open_gate = |facts: PhaseFacts| PhaseFacts {
                gate_pending: true,
                open_gate_stages: vec![Stage::Code],
                ..facts
            };
            let shapes: [&dyn Fn(PhaseFacts) -> PhaseFacts; 3] =
                [&gate_pending, &dead_agent, &open_gate];
            let pending = PhaseLiveness::LaunchPending {
                launch_id: SUPERVISOR_LAUNCH_ID.to_string(),
                published: true,
            };
            let agent = LiveAgent {
                pid: 4242,
                confirmed: true,
            };
            for (index, shape) in shapes.iter().enumerate() {
                for (case, supervisor, live_agent) in [
                    ("alive", agreeing_facts(phase).supervisor, None),
                    ("pending", pending.clone(), None),
                    (
                        "orphan",
                        PhaseLiveness::GoneWhileActive,
                        Some(agent.clone()),
                    ),
                ] {
                    let findings = reconcile_phase(&shape(PhaseFacts {
                        supervisor,
                        live_agent,
                        ..agreeing_facts(phase)
                    }));
                    assert!(!findings.is_empty(), "shape {index}, {case}");
                    for finding in &findings {
                        let repair = finding.repair.as_deref().unwrap_or_default();
                        assert!(
                            !repair.contains("devflow resume"),
                            "shape {index}, {case}: {repair}"
                        );
                    }
                }
                let findings = reconcile_phase(&shape(PhaseFacts {
                    supervisor: PhaseLiveness::NotLaunched,
                    ..agreeing_facts(phase)
                }));
                assert!(
                    findings.iter().any(|finding| finding
                        .repair
                        .as_deref()
                        .is_some_and(|repair| repair.contains("devflow resume"))),
                    "control, shape {index}"
                );
            }
        }

        #[test]
        fn reconcile_phase_flags_dead_agent_at_agent_stage() {
            let facts = PhaseFacts {
                agent_pid: Some(999_999),
                agent_alive: false,
                supervisor: PhaseLiveness::NotLaunched,
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
                unprepared_restart: None,
                stored_ship_answer: false,
                feature_branch_exists: false,
                ..agreeing_facts(PhaseId::new(6))
            };
            let findings = reconcile_phase(&facts);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Warn);
            assert!(findings[0].detail.contains("feature/phase-06"));
            assert!(findings[0].repair.is_none());
        }

        /// 18b / 50-12: a supervisor GONE while its phase is not stopped,
        /// with a dead agent — nothing will call `devflow advance` for this
        /// phase — reports a `Problem` finding with a `devflow resume --phase
        /// N` repair.
        #[test]
        fn reconcile_reports_gone_supervisor_and_dead_agent() {
            let facts = PhaseFacts {
                supervisor: PhaseLiveness::GoneWhileActive,
                agent_pid: Some(4242),
                agent_alive: false,
                ..agreeing_facts(PhaseId::new(8))
            };
            let findings = reconcile_phase(&facts);
            let supervisor_finding = findings
                .iter()
                .find(|f| f.detail.contains("supervisor launch"))
                .expect("expected a supervisor finding when the supervisor is gone");
            assert_eq!(supervisor_finding.severity, Severity::Problem);
            assert!(supervisor_finding.detail.contains(SUPERVISOR_LAUNCH_ID));
            assert_eq!(
                supervisor_finding.repair.as_deref(),
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
        /// `check_dead_agent`): the same stopped phase's supervisor has
        /// exited. `check_supervisor` must also recognize `facts.stopped`
        /// and report ZERO findings.
        #[test]
        fn reconcile_phase_ignores_gone_supervisor_when_stopped() {
            let facts = PhaseFacts {
                stage: Stage::Plan,
                supervisor: PhaseLiveness::Gone,
                agent_pid: Some(4242),
                agent_alive: false,
                stopped: true,
                ..agreeing_facts(PhaseId::new(12))
            };
            let findings = reconcile_phase(&facts);
            assert!(
                findings.iter().all(|f| f.severity != Severity::Problem),
                "a --until-stopped phase must yield zero Problem findings even with an \
                 exited supervisor, got: {:?}",
                findings.iter().map(|f| &f.detail).collect::<Vec<_>>()
            );
        }

        /// 18b (T-18-11): an unrecorded supervisor is not launched, not a
        /// problem — a state file written before the record existed must
        /// never render as a finding.
        #[test]
        fn reconcile_is_silent_when_no_supervisor_is_recorded() {
            let facts = PhaseFacts {
                supervisor: PhaseLiveness::NotLaunched,
                supervisor_launch_id: None,
                ..agreeing_facts(PhaseId::new(9))
            };
            assert!(
                reconcile_phase(&facts).is_empty(),
                "an unrecorded supervisor must never produce a finding"
            );
        }

        /// 18b / 50-12: an ALIVE supervisor with a dead agent is a normal
        /// between-stages moment (the supervisor runs the advance itself),
        /// not a supervisor finding. `check_dead_agent`'s own pre-existing
        /// finding for the dead agent pid is unrelated to this check.
        #[test]
        fn reconcile_is_silent_when_supervisor_alive_and_agent_dead() {
            let facts = PhaseFacts {
                supervisor: PhaseLiveness::Alive {
                    owner: "advancing".to_string(),
                    launch: "stream".to_string(),
                },
                agent_alive: false,
                ..agreeing_facts(PhaseId::new(10))
            };
            let findings = reconcile_phase(&facts);
            assert!(
                findings
                    .iter()
                    .all(|f| !f.detail.contains("supervisor launch")),
                "a live supervisor with a dead agent must not produce a supervisor finding"
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
                supervisor: PhaseLiveness::GoneWhileActive,
                last_launched_stage: Some(Stage::Validate),
                open_gate_stages: Vec::new(),
                unprepared_restart: None,
                stored_ship_answer: false,
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
                    Severity::Problem, // check_supervisor
                    Severity::Warn,    // check_stage_event_drift
                    Severity::Warn,    // check_missing_branch
                ]
            );
            assert!(findings[0].detail.contains("gate_pending is true"));
            assert!(findings[1].detail.contains("agent pid 999999"));
            assert!(findings[2].detail.contains("supervisor launch"));
            assert!(findings[2].detail.contains("GONE"));
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
        fn doctor_reads_no_lock_file() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let empty_phase = PhaseId::new(93);
            let corrupt_phase = PhaseId::new(94);
            for (phase, contents) in [(empty_phase, ""), (corrupt_phase, "not-a-pid\nnever")] {
                workflow::save_state(&State::new(
                    phase,
                    AgentKind::Claude,
                    Mode::Auto,
                    root.to_path_buf(),
                ))
                .unwrap();
                let lock_path = root
                    .join(".devflow")
                    .join(format!("lock-{}", phase.padded()));
                std::fs::write(lock_path, contents).unwrap();
            }

            let facts = collect_phase_facts(root);

            assert_eq!(facts.len(), 2);
            assert!(facts.iter().all(|fact| fact.waiter == WaiterStatus::Absent));
            assert_eq!(
                std::fs::read_to_string(root.join(".devflow/lock-93")).unwrap(),
                ""
            );
            assert_eq!(
                std::fs::read_to_string(root.join(".devflow/lock-94")).unwrap(),
                "not-a-pid\nnever"
            );
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

            let body = doctor_json_body(&checks, &facts, &[], &[], (&[], None), &[]);
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
            assert!(
                reparsed.get("orphan_supervisors").is_some(),
                "50-12: must carry the socket sweep's findings under their own key: {reparsed}"
            );
            assert_eq!(
                reparsed.as_object().unwrap().len(),
                7,
                "doctor --json must have exactly seven top-level keys: {reparsed}"
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

            // 50-12: a hermetic cache whose STALE socket no state claims, so
            // the run also exercises the socket sweep, which must leave it.
            let cache = short_cache();
            let (orphan, _) = publish_launch(cache.path(), root, 91, "aaaa0000aaaa0091");
            write_sidecar_for(&orphan, root, 91, "aaaa0000aaaa0091");

            doctor_in(root, false, Some(cache.path())).unwrap();
            doctor_in(root, false, Some(cache.path())).unwrap();
            assert!(
                orphan.as_path().exists() && orphan.sidecar_path().exists(),
                "doctor must not remove a socket or sidecar it reports"
            );

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
            let body = doctor_json_body(&checks, &facts, &[], &stray_findings, (&[], None), &[]);
            let obj = body.as_object().unwrap();
            assert_eq!(obj.len(), 7, "must be exactly seven top-level keys: {body}");
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
            let dir = tempfile::tempdir().unwrap();
            let fifo = dir.path().join("monitor-wrapper-block");
            let status = std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .expect("create private monitor-wrapper FIFO");
            assert!(status.success(), "mkfifo must create the fixture FIFO");

            let mut child = std::process::Command::new("sh")
                .arg("-c")
                // Keep the wrapper-shaped shell blocked in its own builtin:
                // no `sleep` child exists to outlive `child.kill()` if an
                // assertion panics. `$1` is supplied as a distinct argv
                // element, never interpolated into shell source.
                .arg("trap cleanup TERM INT; read _ < \"$1\"")
                .arg("sh")
                .arg(&fifo)
                .spawn()
                .expect("spawn monitor-wrapper-shaped fixture");
            let pid = child.id();

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
            // A hermetic, empty cache: never sweep the operator's real one.
            let cache = tempfile::tempdir().unwrap();
            doctor_in(dir.path(), false, Some(cache.path())).unwrap();
            assert!(agent::agent_running(pid), "doctor() itself must not signal");
            doctor_in(dir.path(), false, Some(cache.path())).unwrap();
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
    /// state file and held by none of its live files — and no longer asserts a conclusion
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
            finding.detail.contains(
                "named by no registered project root's state file and held by none of its live \
                 liveness, gate-registration or foreground-holder files"
            ),
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
    /// a live holder pid and a genuine orphan under no registered root
    /// at all. Only the orphan may survive `unreachable_stray_candidates`'s
    /// filter. Both pids are real, spawned processes crossed through
    /// `wait_for_exec_visibility` — not synthesised numbers — so the
    /// census and the filter are both exercised for real, in one pass.
    ///
    /// A third, state-recorded fixture (a pid in the state's monitor-pid
    /// field) was removed with that input (50-22): held files are the
    /// only reachable source, because the census cannot match a 3.x
    /// supervisor (`agent::tests::the_stray_census_matches_only_the_2x_process_shapes`).
    #[test]
    fn reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates() {
        let mut lock_child = spawn_wrapper_shaped_fixture();
        let mut orphan_child = spawn_wrapper_shaped_fixture();
        let lock_pid = lock_child.id();
        let orphan_pid = orphan_child.id();

        let cache_dir = tempfile::tempdir().unwrap();
        let project_root_guard = tempfile::tempdir().unwrap();
        let project_root = project_root_guard.path().to_path_buf();

        // Phase 1: a real `State` with nothing held, the registered phase.
        let state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            project_root.clone(),
        );
        workflow::save_state(&state).unwrap();

        // Phase 2's holder file, held here with its record naming `lock_pid`. This test's
        // own process really holds it; the record is what the set reads.
        let lock_start_time = agent::process_start_time(lock_pid)
            .expect("must read the holder fixture's own recorded start time");
        let _hold = hold_phase_with_record(
            &project_root,
            PhaseId::new(2),
            lock_pid,
            Some(lock_start_time),
        );
        // Load-bearing: a record format change fails this test loudly instead of silently
        // exercising only half of it.
        assert_eq!(
            liveness::live_holder(&liveness::phase_holder_path(&project_root, PhaseId::new(2)))
                .unwrap(),
            Some((lock_pid, Some(lock_start_time))),
            "the rewritten record must read back as a claim's own record would"
        );

        registry::register_in(cache_dir.path(), &project_root, PhaseId::new(1)).unwrap();
        let registered_roots: Vec<PathBuf> = registry::load_roots_in(cache_dir.path())
            .into_iter()
            .map(|r| r.project_root)
            .collect();

        let reachable = registry_reachable_pids(&registered_roots);
        assert!(
            reachable.contains(&lock_pid),
            "lock_pid must be reachable through the held holder file"
        );
        assert!(
            !reachable.contains(&orphan_pid),
            "orphan_pid is named by no state file and holds no file, so it must not be \
             reachable"
        );

        // Without this, the test could pass vacuously because the
        // census never saw the fixtures at all (T-25-15-11).
        let census = agent::discover_stray_devflow_processes();
        for (pid, label) in [(lock_pid, "lock"), (orphan_pid, "orphan")] {
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
            !retained.iter().any(|p| p.pid == lock_pid),
            "the holder's pid must be filtered out"
        );

        let findings = build_stray_process_findings(&retained);
        assert!(
            findings.iter().any(|f| f.pid == orphan_pid),
            "the orphan must produce a finding"
        );
        assert!(
            !findings.iter().any(|f| f.pid == lock_pid),
            "the holder's pid must produce no finding"
        );

        // Reap both with a VERIFIED signal, never a bare one
        // (mirrors `reap_strays_e2e.rs:202-223`'s own teardown), plus a
        // final `wait()` on each to reclaim the zombie regardless.
        for pid in [lock_pid, orphan_pid] {
            agent::terminate_and_verify(
                pid,
                agent::TERMINATE_VERIFY_WAIT,
                agent::TERMINATE_VERIFY_POLL,
            );
        }
        let _ = lock_child.wait();
        let _ = orphan_child.wait();
    }

    /// 999.44's originating case, as an executable assertion rather than
    /// prose (see `25-15-PLAN.md`'s `<why_this_does_not_violate_d17>`):
    /// once a registered root is deleted off disk, its state file and
    /// held files are gone with it, so it contributes ZERO pids to the
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

        let state = State::new(PhaseId::new(1), AgentKind::Claude, Mode::Auto, root.clone());
        workflow::save_state(&state).unwrap();
        // The pid's path into the reachable set: a held holder file whose record names it.
        let start_time = agent::process_start_time(pid)
            .expect("must read the holder fixture's own recorded start time");
        let _hold = hold_phase_with_record(&root, PhaseId::new(1), pid, Some(start_time));

        assert!(
            registry_reachable_pids(std::slice::from_ref(&root)).contains(&pid),
            "the pid must be reachable while its root, state file and holder file still exist"
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
            "a deleted root's state file and held files are gone with it, so it must \
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
            let body = doctor_json_body(&checks, &facts, &doc_findings, &[], (&[], None), &[]);
            let obj = body.as_object().unwrap();
            assert_eq!(
                obj.len(),
                7,
                "must be exactly {{environment, reconciliation, planning_doc_staleness, \
                 stray_processes, orphan_supervisors, orphan_supervisor_sweep_error, \
                 set_aside_gate_answers}}: {body}"
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

    // -----------------------------------------------------------------------
    // 50-14: `stop` through the run's supervisor socket (crash row C13, and
    // a supervisor that dies under its shutdown request).
    // -----------------------------------------------------------------------

    /// A 0700 socket directory under a short tempdir in `/tmp`, so every
    /// socket path below it fits `sun_path`. The tempdir removes it on drop.
    fn stop_socket_dir() -> (tempfile::TempDir, PathBuf) {
        use std::os::unix::fs::PermissionsExt as _;
        let base = tempfile::Builder::new()
            .prefix("dfs")
            .tempdir_in("/tmp")
            .expect("create a short socket base under /tmp");
        let dir = base.path().join("sock");
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        (base, dir)
    }

    /// Bind a listener at `path` with mode 0600, as a published supervisor
    /// socket is.
    fn bind_supervisor_socket(path: &Path) -> std::os::unix::net::UnixListener {
        use std::os::unix::fs::PermissionsExt as _;
        let listener = std::os::unix::net::UnixListener::bind(path).expect("bind the socket");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        listener
    }

    /// Wait up to `within` for `fd` to be readable (a queued connection on a
    /// listener, data on a stream); whether it became readable.
    fn readable_within(fd: &impl std::os::fd::AsRawFd, within: std::time::Duration) -> bool {
        let mut entry = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let millis = libc::c_int::try_from(within.as_millis()).unwrap_or(libc::c_int::MAX);
        loop {
            // SAFETY: one initialized pollfd naming a descriptor the caller
            // holds open for the whole call.
            let ready = unsafe { libc::poll(&raw mut entry, 1, millis) };
            if ready >= 0
                || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                return ready == 1;
            }
        }
    }

    /// A well-formed ping reply line for a scripted supervisor.
    fn scripted_ping_reply(launch_id: &str, phase: PhaseId, owner: &str) -> String {
        serde_json::json!({
            "ok": true,
            "launch_id": launch_id,
            "phase": phase.to_string(),
            "stage": "code",
            "launch": "stream",
            "owner": owner,
            "agent_pid": 1,
            "agent_pgid": 1,
            "agent_start_time": 0,
            "boot_id": null,
            "supervisor_pid": 1,
        })
        .to_string()
    }

    /// Accept one connection on `listener` within `within`, or `None`.
    fn accept_within(
        listener: &std::os::unix::net::UnixListener,
        within: std::time::Duration,
    ) -> Option<std::os::unix::net::UnixStream> {
        if !readable_within(listener, within) {
            return None;
        }
        Some(listener.accept().expect("accept a queued connection").0)
    }

    /// Read one request line from `stream`, or `None` when none arrives
    /// within the read timeout.
    fn read_request_line(stream: &std::os::unix::net::UnixStream) -> Option<String> {
        use std::io::BufRead as _;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut line = String::new();
        std::io::BufReader::new(stream).read_line(&mut line).ok()?;
        (!line.is_empty()).then(|| line.trim_end().to_string())
    }

    /// The lines a scripted supervisor sends for one `shutdown` request, and
    /// whether it then unlinks its socket and stops serving.
    struct ShutdownScript {
        lines: Vec<String>,
        unlink_after: bool,
    }

    /// A scripted supervisor: serves `socket` on a thread, logs each
    /// connection's one request line, answers `ping` with
    /// [`scripted_ping_reply`] and the n-th `shutdown` (1-based) with
    /// `on_shutdown(n)`. Dropping it stops the thread and removes the socket.
    struct ScriptedSupervisor {
        socket: PathBuf,
        log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        done: std::sync::Arc<std::sync::atomic::AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl ScriptedSupervisor {
        fn serve(
            socket: &Path,
            launch_id: &str,
            phase: PhaseId,
            owner: &str,
            mut on_shutdown: impl FnMut(usize) -> ShutdownScript + Send + 'static,
        ) -> Self {
            use std::io::Write as _;
            use std::sync::atomic::Ordering;
            let listener = bind_supervisor_socket(socket);
            let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let ping = scripted_ping_reply(launch_id, phase, owner);
            let (thread_log, thread_done, path) = (log.clone(), done.clone(), socket.to_path_buf());
            let thread = std::thread::spawn(move || {
                let mut shutdowns = 0;
                let mut parked = Vec::new();
                while !thread_done.load(Ordering::SeqCst) {
                    let Some(mut stream) =
                        accept_within(&listener, std::time::Duration::from_millis(50))
                    else {
                        continue;
                    };
                    let Some(request) = read_request_line(&stream) else {
                        continue;
                    };
                    thread_log.lock().unwrap().push(request.clone());
                    if request.contains("\"ping\"") {
                        let _ = writeln!(stream, "{ping}");
                        continue;
                    }
                    shutdowns += 1;
                    let script = on_shutdown(shutdowns);
                    for line in &script.lines {
                        let _ = writeln!(stream, "{line}");
                    }
                    // A supervisor with a stop pending keeps the connection for its final
                    // line, so a script that ends on `accepted` does too.
                    if script
                        .lines
                        .last()
                        .is_some_and(|line| line.contains("\"accepted\":true"))
                    {
                        parked.push(stream);
                        continue;
                    }
                    if script.unlink_after {
                        let _ = std::fs::remove_file(&path);
                        return;
                    }
                }
            });
            Self {
                socket: socket.to_path_buf(),
                log,
                done,
                thread: Some(thread),
            }
        }

        /// How many `shutdown` requests this supervisor has received.
        fn shutdowns(&self) -> usize {
            self.log
                .lock()
                .unwrap()
                .iter()
                .filter(|line| line.contains("\"shutdown\""))
                .count()
        }
    }

    impl Drop for ScriptedSupervisor {
        fn drop(&mut self) {
            self.done.store(true, std::sync::atomic::Ordering::SeqCst);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            let _ = std::fs::remove_file(&self.socket);
        }
    }

    /// Save a state for `phase` whose supervisor record names `socket` and
    /// `launch_id`.
    fn save_state_naming(root: &Path, phase: PhaseId, socket: &Path, launch_id: &str) {
        let mut state = match workflow::load_state(root, phase) {
            Ok(state) => state,
            Err(_) => State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf()),
        };
        state.supervisor = Some(SupervisorRecord::new(
            socket.to_path_buf(),
            launch_id.to_string(),
            1,
            0,
            "boot".to_string(),
        ));
        workflow::save_state(&state).unwrap();
    }

    const HANDED_OFF_LINE: &str = r#"{"ok":false,"error":"handed_off"}"#;
    const ACCEPTED_ADVANCING_LINE: &str = r#"{"ok":true,"accepted":true,"owner":"advancing"}"#;
    const ACCEPTED_LINE: &str = r#"{"ok":true,"accepted":true,"owner":"agent_running"}"#;
    const STOPPED_LINE: &str = r#"{"ok":true,"stopped":true,"exit_code":143,"advance_suppressed":true,"group_signalled":true}"#;

    /// D-16 / D-13 item 6: the success line says what the supervisor did. A safe-point
    /// stop signalled nothing and says so; only a signalled group is "ended".
    #[test]
    fn the_stop_line_names_the_safe_point_when_no_agent_group_was_signalled() {
        let safe_point = stopped_line("phase 7", 143, true, false);
        assert_eq!(
            safe_point,
            "stop: stopped phase 7 through its supervisor (honoured at an advance safe point; \
             nothing was signalled; exit code 143)"
        );
        assert!(!safe_point.contains("agent group ended"), "{safe_point}");
        let signalled = stopped_line("launch abc", 143, true, true);
        assert_eq!(
            signalled,
            "stop: stopped launch abc through its supervisor (agent group ended, exit code 143, \
             advance suppressed)"
        );
    }

    /// D-13 item 1: a supervisor that accepted the stop and has not sent its final line
    /// within `stop`'s wait will still honour it. `stop` says so, records `stopped` as for a
    /// confirmed stop, and does not report a failure.
    #[test]
    fn stop_reports_an_accepted_stop_it_could_not_confirm_as_pending() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(1420);
        let (base, sock) = stop_socket_dir();
        let socket = sock.join("pending.sock");
        save_state_naming(&root, phase, &socket, "launch-pending");
        let supervisor =
            ScriptedSupervisor::serve(&socket, "launch-pending", phase, "advancing", |_| {
                ShutdownScript {
                    lines: vec![ACCEPTED_ADVANCING_LINE.to_string()],
                    unlink_after: false,
                }
            });

        let started = std::time::Instant::now();
        let result = stop_in(&root, phase, Some(base.path()));

        assert!(result.is_ok(), "an accepted stop is no failure: {result:?}");
        assert!(
            started.elapsed() >= shutdown_final_timeout(),
            "stop gave the final line its whole wait: {:?}",
            started.elapsed()
        );
        assert_eq!(supervisor.shutdowns(), 1, "one stop, one request");
        let state = workflow::load_state(&root, phase).unwrap();
        assert!(state.stopped, "recorded as for a confirmed stop");
        assert!(
            state
                .stop_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("devflow stop")),
            "{:?}",
            state.stop_reason
        );

        let line = accepted_pending_line(&format!("phase {phase}"), "advancing");
        for fragment in [
            "accepted the stop",
            "advancing",
            "next safe point",
            "`devflow status`",
        ] {
            assert!(line.contains(fragment), "missing {fragment:?}: {line}");
        }
        assert!(!line.contains("nothing was signalled"), "{line}");
    }

    /// A process of its own that is killed and reaped on every exit path.
    struct OwnGroupSleep(std::process::Child);

    impl OwnGroupSleep {
        fn spawn() -> Self {
            use std::os::unix::process::CommandExt as _;
            Self(
                std::process::Command::new("sleep")
                    .arg("60")
                    .process_group(0)
                    .spawn()
                    .expect("spawn the agent stand-in"),
            )
        }
    }

    impl Drop for OwnGroupSleep {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// D-19, `stop`'s half: a GONE socket, no pending record and no holder, with the phase's
    /// agent still running. `stop` names it and the manual kill, signals nothing and writes
    /// nothing. Control: once the agent is dead, today's path records `stopped`.
    #[test]
    fn stop_names_an_orphaned_agent_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(1421);
        let (base, sock) = stop_socket_dir();
        let mut agent = OwnGroupSleep::spawn();
        let pid = agent.0.id();
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.clone());
        state.supervisor = Some(SupervisorRecord::new(
            sock.join("missing.sock"),
            "launch-gone".to_string(),
            pid,
            agent::process_start_time(pid).expect("the agent's start time"),
            supervisor::boot_id().expect("this host has a boot id"),
        ));
        workflow::save_state(&state).unwrap();
        let before = std::fs::read(workflow::state_path(&root, phase)).unwrap();

        let error = stop_in(&root, phase, Some(base.path()))
            .expect_err("stop must not report a stop beside a live agent")
            .to_string();

        for fragment in [
            pid.to_string(),
            format!("kill -TERM -- -{pid}"),
            "999.170".to_string(),
        ] {
            assert!(error.contains(&fragment), "missing {fragment:?}: {error}");
        }
        assert!(!error.contains("nothing is running"), "{error}");
        assert!(!error.contains("devflow resume"), "{error}");
        assert_eq!(
            std::fs::read(workflow::state_path(&root, phase)).unwrap(),
            before,
            "nothing is written"
        );
        assert!(
            matches!(agent.0.try_wait(), Ok(None)),
            "the agent is not signalled"
        );

        agent.0.kill().unwrap();
        agent.0.wait().unwrap();
        stop_in(&root, phase, Some(base.path())).expect("control: nothing runs now");
        assert!(workflow::load_state(&root, phase).unwrap().stopped);
    }

    /// D-19's other source: no supervisor record, and a live pid in the agent pid file,
    /// which a reused pid can satisfy. `stop` names it with the `ps` check to run first,
    /// signals nothing and writes nothing. Control: once it is dead, `stopped` is recorded.
    #[test]
    fn stop_names_an_unconfirmed_agent_pid_with_the_check_to_run_first() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(1422);
        let (base, _sock) = stop_socket_dir();
        let mut agent = OwnGroupSleep::spawn();
        let pid = agent.0.id();
        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.clone());
        workflow::save_state(&state).unwrap();
        std::fs::write(
            devflow_core::agent_result::agent_pid_path(&root, phase),
            format!("{pid}\n"),
        )
        .unwrap();
        let before = std::fs::read(workflow::state_path(&root, phase)).unwrap();

        let error = stop_in(&root, phase, Some(base.path()))
            .expect_err("stop must not report a stop beside a live pid")
            .to_string();

        assert!(
            error.contains(&format!("ps -o pid,pgid,lstart,args -p {pid}")),
            "{error}"
        );
        assert!(!error.contains("nothing is running"), "{error}");
        assert_eq!(
            std::fs::read(workflow::state_path(&root, phase)).unwrap(),
            before,
            "nothing is written"
        );
        assert!(
            matches!(agent.0.try_wait(), Ok(None)),
            "the pid is not signalled"
        );

        agent.0.kill().unwrap();
        agent.0.wait().unwrap();
        stop_in(&root, phase, Some(base.path())).expect("control: nothing runs now");
        assert!(workflow::load_state(&root, phase).unwrap().stopped);
    }

    /// Crash row C13 (V-57): the state names supervisor A; A answers the
    /// shutdown by saving the state naming B (N+1's record is saved before N
    /// is marked handed off) and replying `handed_off`. `stop` reloads the
    /// state and stops B: A is asked once, B once, and `stopped` is recorded.
    #[test]
    fn stop_retries_a_handed_off_reply_against_the_reloaded_record() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(1401);
        let (_base, sock) = stop_socket_dir();
        let (a_path, b_path) = (sock.join("a.sock"), sock.join("b.sock"));
        save_state_naming(&root, phase, &a_path, "launch-a");

        let b = ScriptedSupervisor::serve(&b_path, "launch-b", phase, "agent_running", |_| {
            ShutdownScript {
                lines: vec![ACCEPTED_LINE.to_string(), STOPPED_LINE.to_string()],
                unlink_after: false,
            }
        });
        let (a_root, a_b_path) = (root.clone(), b_path.clone());
        let a = ScriptedSupervisor::serve(&a_path, "launch-a", phase, "handed_off", move |_| {
            save_state_naming(&a_root, phase, &a_b_path, "launch-b");
            ShutdownScript {
                lines: vec![HANDED_OFF_LINE.to_string()],
                unlink_after: false,
            }
        });

        let result = stop(&root, phase);
        assert!(
            result.is_ok(),
            "stop must reach B through the reloaded record: {result:?}"
        );
        assert_eq!(a.shutdowns(), 1, "A is asked exactly once");
        assert_eq!(b.shutdowns(), 1, "B is asked exactly once");
        let state = workflow::load_state(&root, phase).unwrap();
        assert!(state.stopped, "the stop B performed is recorded");
    }

    /// C13 control (V-57): every reload names A again, so `stop` gives up
    /// after the first request and 3 retries, with an error naming
    /// `handed_off` and the attempt count, and records nothing. A unlinks its
    /// socket after a fifth request, so a loop past the cap ends as Gone and
    /// fails the count instead of hanging.
    #[test]
    fn stop_gives_up_after_three_handed_off_retries_and_records_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(1402);
        let (_base, sock) = stop_socket_dir();
        let a_path = sock.join("a.sock");
        save_state_naming(&root, phase, &a_path, "launch-a");
        let state_file = workflow::state_path(&root, phase);
        let before = std::fs::read(&state_file).unwrap();

        let a = ScriptedSupervisor::serve(&a_path, "launch-a", phase, "handed_off", |count| {
            ShutdownScript {
                lines: vec![HANDED_OFF_LINE.to_string()],
                unlink_after: count >= 5,
            }
        });

        let error = stop(&root, phase)
            .expect_err("a supervisor that keeps answering handed_off is not a stop")
            .to_string();
        assert!(
            error.contains("handed_off") && error.contains('4'),
            "the error names handed_off and the attempt count: {error}"
        );
        assert_eq!(a.shutdowns(), 4, "the first request plus exactly 3 retries");
        assert_eq!(
            std::fs::read(&state_file).unwrap(),
            before,
            "nothing is recorded"
        );
    }

    /// The sidecar and state record of a launch recorded in an earlier boot:
    /// the backstop proves its group gone without reading a process, so it
    /// removes the files and signals nothing.
    fn plant_previous_boot_launch(root: &Path, phase: PhaseId, socket: &Path) {
        let sidecar = serde_json::json!({
            "launch_id": "aaaa0000aaaa000f",
            "project_root": root,
            "phase": phase,
            "stage": "code",
            "agent": "claude",
            "launch": "stream",
            "agent_pid": 99_999_999,
            "agent_pgid": 99_999_999,
            "agent_start_time": 1,
            "boot_id": "an-earlier-boot",
            "supervisor_pid": 99_999_998,
            "supervisor_start_time": 1,
            "created_at": "2026-10-17T00:00:00Z",
        });
        std::fs::write(socket.with_extension("json"), sidecar.to_string()).unwrap();
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.supervisor = Some(SupervisorRecord::new(
            socket.to_path_buf(),
            "aaaa0000aaaa000f".to_string(),
            99_999_999,
            1,
            "an-earlier-boot".to_string(),
        ));
        workflow::save_state(&state).unwrap();
    }

    /// The race 6d46008 left to `stop`: the supervisor answers the probe's
    /// ping, then its listener is released with the shutdown's connect still
    /// queued, so `request_shutdown` reads a reset or a broken pipe. That is
    /// a supervisor that died, not a failed stop: `stop` probes once more,
    /// reads STALE, runs the backstop (which removes the files and signals
    /// nothing for this earlier-boot launch) and records `stopped`.
    #[test]
    fn stop_acts_on_a_supervisor_that_dies_under_its_shutdown_request() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(1403);
        let (_base, sock) = stop_socket_dir();
        let socket = sock.join("r.sock");
        plant_previous_boot_launch(&root, phase, &socket);
        let listener = bind_supervisor_socket(&socket);
        let ping = scripted_ping_reply("aaaa0000aaaa000f", phase, "agent_running");
        let server = std::thread::spawn(move || {
            use std::io::Write as _;
            let wait = std::time::Duration::from_secs(30);
            let Some(mut first) = accept_within(&listener, wait) else {
                return "no probe connected";
            };
            if read_request_line(&first).is_none_or(|line| !line.contains("\"ping\"")) {
                return "the first request was not a ping";
            }
            let _ = writeln!(first, "{ping}");
            drop(first);
            // Release the listener with the shutdown's connect queued in it
            // and never accepted.
            if !readable_within(&listener, wait) {
                return "no shutdown connect was queued";
            }
            drop(listener);
            "released"
        });

        let result = stop(&root, phase);
        assert_eq!(server.join().expect("server thread"), "released");
        assert!(
            result.is_ok(),
            "a supervisor that died under the shutdown is torn down, not an error: {result:?}"
        );
        assert!(
            !socket.exists() && !socket.with_extension("json").exists(),
            "the backstop removed the stale socket and its sidecar"
        );
        assert!(
            workflow::load_state(&root, phase).unwrap().stopped,
            "the confirmed-not-live stop is recorded"
        );
    }

    /// Control for the race: a listener that stays up but closes the
    /// shutdown (and every later connection) with its request unread is not
    /// a dead supervisor. `stop` fails, removes nothing and records nothing.
    #[test]
    fn stop_reports_a_shutdown_closed_by_a_live_listener() {
        /// Stops an unbounded re-probe from hanging this test.
        const ACCEPT_AT_MOST: usize = 6;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(1404);
        let (_base, sock) = stop_socket_dir();
        let socket = sock.join("l.sock");
        plant_previous_boot_launch(&root, phase, &socket);
        let state_file = workflow::state_path(&root, phase);
        let before = std::fs::read(&state_file).unwrap();
        let listener = bind_supervisor_socket(&socket);
        let ping = scripted_ping_reply("aaaa0000aaaa000f", phase, "agent_running");
        let (stop_server, stopped) = std::sync::mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            use std::io::Write as _;
            let mut accepted = 0;
            while accepted < ACCEPT_AT_MOST && stopped.try_recv().is_err() {
                let Some(mut stream) =
                    accept_within(&listener, std::time::Duration::from_millis(50))
                else {
                    continue;
                };
                accepted += 1;
                if accepted == 1 {
                    let _ = read_request_line(&stream);
                    let _ = writeln!(stream, "{ping}");
                    continue;
                }
                // Close with the request unread, which resets the client.
                let _ = readable_within(&stream, std::time::Duration::from_secs(5));
                drop(stream);
            }
            (listener, accepted)
        });

        let result = stop(&root, phase);
        stop_server.send(()).expect("stop the server");
        let (listener, accepted) = server.join().expect("server thread");
        drop(listener);
        assert!(
            accepted >= 2,
            "stop never sent its shutdown ({accepted} connects)"
        );
        assert!(
            result.is_err(),
            "a live listener that closes the shutdown is not a stop: {result:?}"
        );
        assert!(
            socket.exists() && socket.with_extension("json").exists(),
            "nothing is removed"
        );
        assert_eq!(
            std::fs::read(&state_file).unwrap(),
            before,
            "nothing is recorded"
        );
    }

    /// `root`'s holder file for `phase` held here, its record naming a `sleep` child with
    /// its real pid and start time: the identity `stop_via_holder` would signal. Returns
    /// the child and the hold.
    fn hold_lock_by_a_child(root: &Path, phase: PhaseId) -> (std::process::Child, liveness::Hold) {
        let (child, start) = sleep_child_with_start();
        let hold = hold_phase_with_record(root, phase, child.id(), Some(start));
        (child, hold)
    }

    /// `stop` during a `start` launcher's publish wait: the state loads but
    /// records no supervisor yet, the spawned supervisor already serves its
    /// socket, and the launcher holds the phase. `stop` must end that
    /// supervisor through its socket, not signal the launcher. The scripted
    /// supervisor drops the hold as it stops, as the launcher does once its
    /// supervisor is gone.
    #[test]
    fn stop_ends_an_unrecorded_supervisor_through_its_socket_not_the_launcher() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(1405);
        workflow::save_state(&State::new(
            phase,
            AgentKind::Claude,
            Mode::Auto,
            root.clone(),
        ))
        .unwrap();
        let (base, _sock) = stop_socket_dir();
        let launch_id = "aaaa0000aaaa1405";
        let socket = supervisor::SocketPath::for_launch(base.path(), &root, phase, launch_id)
            .expect("socket path fits sun_path");
        let (mut launcher, hold) = hold_lock_by_a_child(&root, phase);
        let mut hold = Some(hold);
        let unrecorded =
            ScriptedSupervisor::serve(socket.as_path(), launch_id, phase, "agent_running", {
                move |_| {
                    drop(hold.take());
                    ShutdownScript {
                        lines: vec![ACCEPTED_LINE.to_string(), STOPPED_LINE.to_string()],
                        unlink_after: true,
                    }
                }
            });

        let result = stop_in(&root, phase, Some(base.path()));
        let launcher_alive = launcher.try_wait().unwrap().is_none();
        let _ = launcher.kill();
        let _ = launcher.wait();
        assert_eq!(
            unrecorded.shutdowns(),
            1,
            "stop must shut the unrecorded supervisor down through its socket: {result:?}"
        );
        assert!(launcher_alive, "the launcher must not be signalled");
        assert!(result.is_ok(), "the stop happened: {result:?}");
        assert!(
            workflow::load_state(&root, phase).unwrap().stopped,
            "the stop is recorded"
        );
    }

    /// D-18 (V-83): a socket stop is not a signalled holder. The launcher keeps its hold
    /// for the whole `stop`, as one still in its publish wait does, and the run leaves no
    /// state: `stop` neither waits on the holder nor calls its pid signalled. The planted
    /// holder reads `Live` before and after, so a release wait could not have passed.
    ///
    /// Limit: an in-process test cannot read `stop`'s printed line. Which line prints is
    /// pinned by `a_holder_that_ended_its_run_after_the_signal_is_not_reported_as_stopped`.
    #[test]
    fn a_supervisor_stopped_through_its_socket_is_never_reported_as_a_signalled_holder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(1407);
        workflow::save_state(&State::new(
            phase,
            AgentKind::Claude,
            Mode::Auto,
            root.clone(),
        ))
        .unwrap();
        let (base, _sock) = stop_socket_dir();
        let launch_id = "aaaa0000aaaa1407";
        let socket = supervisor::SocketPath::for_launch(base.path(), &root, phase, launch_id)
            .expect("socket path fits sun_path");
        let (mut launcher, _hold) = hold_lock_by_a_child(&root, phase);
        let holder_path = liveness::phase_holder_path(&root, phase);
        assert_eq!(
            liveness::probe(&holder_path).unwrap(),
            liveness::Held::Live,
            "the planted holder reads live before `stop`"
        );
        let unrecorded =
            ScriptedSupervisor::serve(socket.as_path(), launch_id, phase, "agent_running", {
                let root = root.clone();
                move |_| {
                    workflow::clear_state(&root, phase).expect("the run ends and leaves no state");
                    ShutdownScript {
                        lines: vec![ACCEPTED_LINE.to_string(), STOPPED_LINE.to_string()],
                        unlink_after: true,
                    }
                }
            });

        let started = std::time::Instant::now();
        let result = stop_in(&root, phase, Some(base.path()));
        let elapsed = started.elapsed();
        let launcher_alive = devflow_core::test_support::process_is_live(launcher.id());
        let after = liveness::probe(&holder_path).unwrap();
        let _ = launcher.kill();
        let _ = launcher.wait();

        result.unwrap_or_else(|err| panic!("after {elapsed:?}: {err}"));
        assert!(
            elapsed < agent::TERMINATE_VERIFY_WAIT,
            "`stop` does not wait on a holder it did not signal: {elapsed:?}"
        );
        assert_eq!(unrecorded.shutdowns(), 1, "exactly one shutdown");
        assert!(launcher_alive, "the launcher is never signalled");
        assert_eq!(
            after,
            liveness::Held::Live,
            "a holder wait would have bitten"
        );
        assert!(
            !workflow::state_path(&root, phase).exists(),
            "nothing is written over the cleared state"
        );
    }

    /// The several-sockets refusal also covers a loaded state with no
    /// record: with two live sockets for the phase, `stop` cannot tell which
    /// is the run, so it asks neither and signals no holder.
    #[test]
    fn stop_refuses_several_unrecorded_sockets_and_signals_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(1406);
        workflow::save_state(&State::new(
            phase,
            AgentKind::Claude,
            Mode::Auto,
            root.clone(),
        ))
        .unwrap();
        let (base, _sock) = stop_socket_dir();
        let serve = |launch_id: &str| {
            let socket = supervisor::SocketPath::for_launch(base.path(), &root, phase, launch_id)
                .expect("socket path fits sun_path");
            ScriptedSupervisor::serve(socket.as_path(), launch_id, phase, "agent_running", |_| {
                ShutdownScript {
                    lines: vec![ACCEPTED_LINE.to_string(), STOPPED_LINE.to_string()],
                    unlink_after: true,
                }
            })
        };
        let (a, b) = (serve("aaaa0000aaaa140a"), serve("aaaa0000aaaa140b"));
        let (mut launcher, _hold) = hold_lock_by_a_child(&root, phase);

        let result = stop_in(&root, phase, Some(base.path()));
        let launcher_alive = launcher.try_wait().unwrap().is_none();
        let _ = launcher.kill();
        let _ = launcher.wait();
        let Err(CliError::Message(text)) = &result else {
            panic!("several live sockets must refuse: {result:?}");
        };
        assert!(
            text.contains("2 live supervisor sockets") && text.contains("no state file names one"),
            "{text}"
        );
        assert_eq!((a.shutdowns(), b.shutdowns()), (0, 0), "neither is asked");
        assert!(launcher_alive, "no holder is signalled");
    }

    /// 50-15: the start/resume guard over a state file that will not load
    /// (D-09). These drive the guard in-process against a tempdir cache, so
    /// nothing reads the operator's real `~/.cache/devflow`.
    mod launch_guard {
        use super::*;
        use supervisor::backstop::BackstopDecision;

        /// The guard over what is on disk now, as `reserve_launch`'s closure sees it.
        fn refuse_launch_on_disk(
            root: &Path,
            phase: PhaseId,
            verb: LaunchVerb,
            cache: &Path,
        ) -> Result<(), CliError> {
            let existing = match workflow::load_state(root, phase) {
                Ok(state) => workflow::Existing::Loaded(state),
                Err(workflow::WorkflowError::MissingState(_)) => workflow::Existing::Missing,
                Err(error) => workflow::Existing::Unloadable(error),
            };
            refuse_launch_over_a_live_run(root, phase, verb, &existing, Some(cache))
        }

        /// A project root whose phase state file exists and will not load.
        fn root_with_an_unloadable_state(phase: PhaseId) -> tempfile::TempDir {
            let root = tempfile::tempdir().unwrap();
            let state = workflow::state_path(root.path(), phase);
            std::fs::create_dir_all(state.parent().unwrap()).unwrap();
            std::fs::write(&state, b"{ this does not parse").unwrap();
            assert!(workflow::load_state(root.path(), phase).is_err());
            root
        }

        /// The 50-06 threat flag: `sweep_in` answers a socket directory it
        /// cannot list with no entries, which must not read as "nothing
        /// live" here. The guard fails closed and names the directory.
        #[test]
        fn the_launch_guard_refuses_an_unlistable_socket_dir_over_an_unloadable_state() {
            use std::os::unix::fs::PermissionsExt as _;
            let phase = PhaseId::new(1501);
            let root = root_with_an_unloadable_state(phase);
            let cache = tempfile::tempdir().unwrap();
            let sock = registry::socket_dir_in(cache.path());
            std::fs::create_dir_all(&sock).unwrap();
            std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o000)).unwrap();
            // Both reads run without DAC override, so uid 0 (the pinned
            // container) is refused too (WINDOWS.md #19).
            let (listable, result) = without_dac_override(|| {
                (
                    std::fs::read_dir(&sock).is_ok(),
                    refuse_launch_on_disk(root.path(), phase, LaunchVerb::Start, cache.path()),
                )
            });
            std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o700)).unwrap();
            assert!(
                !listable,
                "precondition: a 0o000 directory cannot be listed"
            );
            let Err(CliError::Message(text)) = result else {
                panic!("an unlistable socket directory must refuse: {result:?}");
            };
            for fragment in [
                sock.display().to_string(),
                format!(
                    "cannot be listed ({})",
                    std::io::ErrorKind::PermissionDenied
                ),
                "refusing to start; nothing was written".to_string(),
            ] {
                assert!(text.contains(&fragment), "missing {fragment:?}: {text}");
            }
        }

        /// Control for the test above: a socket directory that does not
        /// exist means no launch has published a socket, so nothing is live.
        #[test]
        fn the_launch_guard_proceeds_over_an_unloadable_state_when_the_socket_dir_is_missing() {
            let phase = PhaseId::new(1502);
            let root = root_with_an_unloadable_state(phase);
            let cache = tempfile::tempdir().unwrap();
            assert!(!registry::socket_dir_in(cache.path()).exists());
            let result =
                refuse_launch_on_disk(root.path(), phase, LaunchVerb::Resume, cache.path());
            assert!(
                result.is_ok(),
                "a missing socket dir is not a refusal: {result:?}"
            );
        }

        /// T-50-40: only the three backstop decisions that prove a STALE
        /// socket's agent group gone let a launch through; every refusal,
        /// an unprovable identity and a live leader count as live.
        #[test]
        fn only_a_provably_gone_group_lets_a_stale_socket_through_the_launch_guard() {
            for gone in [
                BackstopDecision::PreviousBoot,
                BackstopDecision::EmptyGroup,
                BackstopDecision::LeaderRecycled,
            ] {
                assert!(
                    !stale_group_may_live(&gone),
                    "{gone:?} proves the group gone"
                );
            }
            for live in [
                BackstopDecision::LeaderPresentSignal,
                BackstopDecision::RefuseLeaderGone(Vec::new()),
                BackstopDecision::RefuseMismatch("agent_pgid".to_string()),
                BackstopDecision::RefuseUnsafeTarget,
                BackstopDecision::RefuseIdentityUnavailable,
            ] {
                assert!(stale_group_may_live(&live), "{live:?} must fail closed");
            }
        }

        /// An ALIVE scripted supervisor at this root and phase's socket under
        /// `cache`, with no sidecar, so the sweep attributes it by file name.
        fn serve_unrecorded(cache: &Path, root: &Path, phase: PhaseId) -> ScriptedSupervisor {
            let launch_id = "aaaa0000aaaa1503";
            let socket = supervisor::SocketPath::for_launch(cache, root, phase, launch_id)
                .expect("socket path fits sun_path");
            ScriptedSupervisor::serve(socket.as_path(), launch_id, phase, "agent_running", |_| {
                ShutdownScript {
                    lines: Vec::new(),
                    unlink_after: false,
                }
            })
        }

        /// A launch whose state file is missing is still found through its
        /// socket, as `stop` finds it: `recover --clean` leaves a live
        /// supervisor's socket behind without a state file.
        #[test]
        fn the_launch_guard_finds_a_live_socket_when_the_state_file_is_missing() {
            let phase = PhaseId::new(1503);
            let root = tempfile::tempdir().unwrap();
            assert!(!workflow::state_path(root.path(), phase).exists());
            let (base, _sock) = stop_socket_dir();
            let _alive = serve_unrecorded(base.path(), root.path(), phase);
            let result = refuse_launch_on_disk(root.path(), phase, LaunchVerb::Start, base.path());
            let Err(CliError::Message(text)) = result else {
                panic!("a live socket with no state file must refuse: {result:?}");
            };
            for fragment in [
                "supervisor launch aaaa0000aaaa1503 ALIVE",
                "refusing to start; nothing was written",
            ] {
                assert!(text.contains(fragment), "missing {fragment:?}: {text}");
            }
        }

        /// The unlistable-directory refusal also covers a missing state file:
        /// with neither a state nor a listing, nothing can rule a run out.
        #[test]
        fn the_launch_guard_refuses_an_unlistable_socket_dir_when_the_state_file_is_missing() {
            use std::os::unix::fs::PermissionsExt as _;
            let phase = PhaseId::new(1504);
            let root = tempfile::tempdir().unwrap();
            let cache = tempfile::tempdir().unwrap();
            let sock = registry::socket_dir_in(cache.path());
            std::fs::create_dir_all(&sock).unwrap();
            std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o000)).unwrap();
            let (listable, result) = without_dac_override(|| {
                (
                    std::fs::read_dir(&sock).is_ok(),
                    refuse_launch_on_disk(root.path(), phase, LaunchVerb::Start, cache.path()),
                )
            });
            std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o700)).unwrap();
            assert!(
                !listable,
                "precondition: a 0o000 directory cannot be listed"
            );
            let Err(CliError::Message(text)) = result else {
                panic!("an unlistable socket directory must refuse: {result:?}");
            };
            for fragment in [
                "there is no state file".to_string(),
                sock.display().to_string(),
                "refusing to start; nothing was written".to_string(),
            ] {
                assert!(text.contains(&fragment), "missing {fragment:?}: {text}");
            }
        }

        /// Controls for the two tests above: a fresh start (no state, and no
        /// socket directory or an empty one) proceeds, and the no-state
        /// carve-out still ignores a live pid in the agent pid file.
        #[test]
        fn the_launch_guard_proceeds_with_no_state_and_no_live_socket() {
            let phase = PhaseId::new(1505);
            let root = tempfile::tempdir().unwrap();
            let cache = tempfile::tempdir().unwrap();
            let guard =
                || refuse_launch_on_disk(root.path(), phase, LaunchVerb::Start, cache.path());
            assert!(guard().is_ok(), "no socket directory: {:?}", guard());
            std::fs::create_dir_all(registry::socket_dir_in(cache.path())).unwrap();
            assert!(guard().is_ok(), "an empty socket directory: {:?}", guard());
            let pid_file = agent_result::agent_pid_path(root.path(), phase);
            std::fs::create_dir_all(pid_file.parent().unwrap()).unwrap();
            std::fs::write(&pid_file, std::process::id().to_string()).unwrap();
            assert!(
                guard().is_ok(),
                "the no-state carve-out ignores a live agent pid: {:?}",
                guard()
            );
        }
    }

    /// A sidecar whose launch id is not `new_launch_id`'s format is read as
    /// unusable, so no copy-paste `--launch` line ever carries that id: the
    /// stop line names the id from the socket's file name instead.
    #[test]
    fn a_hostile_sidecar_launch_id_never_reaches_a_stop_line() {
        let (base, _sock) = stop_socket_dir();
        let root = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(1506);
        let name_id = "aaaa0000aaaa1506";
        let hostile = "x;touch${IFS}pwned";
        let socket = supervisor::SocketPath::for_launch(base.path(), root.path(), phase, name_id)
            .expect("socket path fits sun_path");
        drop(bind_supervisor_socket(socket.as_path()));
        let sidecar = serde_json::json!({
            "launch_id": hostile,
            "project_root": root.path(),
            "phase": phase,
            "stage": "code",
            "agent": "claude",
            "launch": "stream",
            "agent_pid": 1,
            "agent_pgid": 1,
            "agent_start_time": 1,
            "boot_id": "boot",
            "supervisor_pid": 1,
            "supervisor_start_time": 1,
            "created_at": "2026-10-17T00:00:00Z",
        });
        std::fs::write(socket.sidecar_path(), sidecar.to_string()).unwrap();
        let entries = supervisor::sweep_for_in(base.path(), root.path(), phase);
        let [entry] = entries.as_slice() else {
            panic!("one entry for this root and phase: {entries:?}");
        };
        let line = sweep_entry_stop_line(phase, entry);
        assert!(!line.contains(hostile), "{line}");
        assert!(
            line.contains(&format!("--launch {name_id}")) && line.contains("does not parse"),
            "{line}"
        );
        let live = swept_live_socket(entry).expect("a STALE socket without a usable sidecar");
        let repair = live.repair_line(phase);
        assert!(!repair.contains(hostile), "{repair}");
    }

    /// `recover --clean --phase N` uses the launch guard's supervisor rule:
    /// a STALE socket whose group the backstop cannot prove gone, or a probe
    /// that fails, refuses and deletes nothing, as an ALIVE one does.
    mod recover_guard {
        use super::*;

        /// A loadable state for `phase` whose record names `socket`, with a
        /// sidecar beside it that records `agent_pgid` in this boot.
        fn plant_stale_launch(root: &Path, phase: PhaseId, socket: &Path, agent_pgid: u32) {
            let boot = supervisor::boot_id().expect("a readable boot id");
            let sidecar = serde_json::json!({
                "launch_id": "aaaa0000aaaa1601",
                "project_root": root,
                "phase": phase,
                "stage": "code",
                "agent": "claude",
                "launch": "stream",
                "agent_pid": agent_pgid,
                "agent_pgid": agent_pgid,
                "agent_start_time": 1,
                "boot_id": boot,
                "supervisor_pid": 99_999_998,
                "supervisor_start_time": 1,
                "created_at": "2026-10-17T00:00:00Z",
            });
            std::fs::write(socket.with_extension("json"), sidecar.to_string()).unwrap();
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.supervisor = Some(SupervisorRecord::new(
                socket.to_path_buf(),
                "aaaa0000aaaa1601".to_string(),
                agent_pgid,
                1,
                boot,
            ));
            workflow::save_state(&state).unwrap();
        }

        /// A STALE socket over pgid 1 (the backstop refuses it as an unsafe
        /// target, so its group is not proved gone) refuses `recover`.
        #[test]
        fn recover_refuses_a_stale_socket_whose_group_may_live() {
            let dir = tempfile::tempdir().unwrap();
            let phase = PhaseId::new(1601);
            let (_base, sock) = stop_socket_dir();
            let socket = sock.join("s.sock");
            plant_stale_launch(dir.path(), phase, &socket, 1);
            drop(bind_supervisor_socket(&socket));
            assert!(matches!(
                supervisor::probe(&socket),
                Ok(supervisor::Liveness::Stale)
            ));
            let result = refuse_a_live_supervisor(dir.path(), phase, None);
            let Err(CliError::Message(text)) = result else {
                panic!("a STALE socket whose group may live must refuse: {result:?}");
            };
            assert!(
                text.contains("recover --clean deleted nothing") && text.contains("STALE"),
                "{text}"
            );
        }

        /// A recorded socket path whose probe fails (a regular file) refuses.
        #[test]
        fn recover_refuses_a_recorded_socket_whose_probe_fails() {
            let dir = tempfile::tempdir().unwrap();
            let phase = PhaseId::new(1602);
            let (_base, sock) = stop_socket_dir();
            let socket = sock.join("f.sock");
            plant_stale_launch(dir.path(), phase, &socket, 1);
            std::fs::write(&socket, b"not a socket").unwrap();
            let result = refuse_a_live_supervisor(dir.path(), phase, None);
            let Err(CliError::Message(text)) = result else {
                panic!("a failed probe must refuse: {result:?}");
            };
            assert!(
                text.contains("recover --clean deleted nothing") && text.contains("probe failed"),
                "{text}"
            );
        }

        /// 50.1-16: a pending supervisor holding its reservation, with no socket yet,
        /// refuses `recover`. Control: released, the same call lets it through.
        #[test]
        fn recover_refuses_a_live_pending_supervisor() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(1604);
            let id = "aaaa0000aaaa1604".to_string();
            let liveness_path = root.join(format!(".devflow/supervisor-{id}.live"));
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.pending_supervisor = Some(PendingSupervisor::new(
                id.clone(),
                root.join("no-such.sock"),
                liveness_path.clone(),
            ));
            workflow::save_state(&state).unwrap();
            let hold = liveness::Hold::take(&liveness_path).unwrap();

            let result = refuse_a_live_supervisor(root, phase, None);
            let Err(CliError::Message(text)) = result else {
                panic!("a live pending supervisor must refuse: {result:?}");
            };
            assert!(
                text.contains("recover --clean deleted nothing") && text.contains(&id),
                "{text}"
            );

            // A released flock can read held while another test's fork is before its exec.
            drop(hold);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while let Err(error) = refuse_a_live_supervisor(root, phase, None) {
                assert!(std::time::Instant::now() < deadline, "{error:?}");
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }

        /// Controls: a GONE socket, and a STALE one whose group the backstop
        /// proves gone (a previous boot), let `recover` through.
        #[test]
        fn recover_proceeds_over_a_gone_or_provably_dead_socket() {
            let dir = tempfile::tempdir().unwrap();
            let phase = PhaseId::new(1603);
            let (_base, sock) = stop_socket_dir();
            let socket = sock.join("g.sock");
            plant_previous_boot_launch(dir.path(), phase, &socket);
            let gone = refuse_a_live_supervisor(dir.path(), phase, None);
            assert!(gone.is_ok(), "a GONE socket is not live: {gone:?}");
            drop(bind_supervisor_socket(&socket));
            let stale = refuse_a_live_supervisor(dir.path(), phase, None);
            assert!(stale.is_ok(), "a previous boot's group is gone: {stale:?}");
        }
    }

    /// How long a test leaves a writer blocked on the write lock before it lets the
    /// holder go. Long enough that a writer not taking the lock has finished.
    const BLOCKED_WRITER_GRACE: std::time::Duration = std::time::Duration::from_millis(200);
    const HOLDER_ENTERS_WITHIN: std::time::Duration = std::time::Duration::from_secs(30);

    /// The state file as JSON, without the two fields `stop` owns.
    fn state_file_without_stop_fields(root: &Path, phase: PhaseId) -> serde_json::Value {
        let contents = std::fs::read_to_string(workflow::state_path(root, phase)).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&contents).unwrap();
        let fields = value.as_object_mut().unwrap();
        fields.remove("stopped").unwrap();
        fields.remove("stop_reason");
        value
    }

    /// D-08: `stop` appends to the reason on disk when it writes, not to one it read
    /// earlier. A writer holds the write lock while `stop` runs, then records a
    /// `--until` reason, so a `stop` that does not wait loses its write to that writer.
    #[test]
    fn persist_stopped_state_appends_its_reason_on_the_current_state() {
        const UNTIL_REASON: &str = "stopped after code completed (--until code)";
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4901);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.yes_ship = true;
        state.infra_failures = 2;
        workflow::save_state(&state).unwrap();
        let others_before = state_file_without_stop_fields(root, phase);

        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let writer = scope.spawn(move || {
                workflow::update_state(root, phase, |on_disk| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    on_disk.stop_reason = Some(UNTIL_REASON.to_string());
                    Ok::<(), workflow::WorkflowError>(())
                })
                .unwrap();
            });
            entered_rx.recv_timeout(HOLDER_ENTERS_WITHIN).unwrap();
            let stopper = scope.spawn(move || {
                persist_stopped_state(root, phase, false, &[]).unwrap();
            });
            std::thread::sleep(BLOCKED_WRITER_GRACE);
            release_tx.send(()).unwrap();
            writer.join().unwrap();
            stopper.join().unwrap();
        });

        let after = workflow::load_state(root, phase).unwrap();
        assert!(after.stopped, "stop's write landed after the writer's");
        assert_eq!(
            after.stop_reason.as_deref(),
            Some("stopped after code completed (--until code); stopped via `devflow stop`")
        );
        assert_eq!(
            state_file_without_stop_fields(root, phase),
            others_before,
            "every other field is byte-identical"
        );
    }

    // The launch reservation (50.1-16, D-01). Every test here runs its launch in a
    // child: `launch_context` needs a hermetic DEVFLOW_CACHE_DIR, and the test holds
    // read their variables from that child's environment.

    const LAUNCH_HOLD_ENV: &str = "DEVFLOW_TEST_LAUNCH_HOLD";
    /// Names which arm of a case-split reservation test a child runs. Not a DevFlow
    /// setting, so it carries no `DEVFLOW_` prefix the doc check would require
    /// documented.
    const RESERVE_CASE_ENV: &str = "P50_1_16_CASE";
    /// How long a test waits for a launcher to reach a hold point.
    const REACH_HOLD_WITHIN: std::time::Duration = std::time::Duration::from_secs(30);

    /// `<point>:<release path>`, with the release path in `dir`.
    fn launch_hold_spec(point: &str, dir: &Path) -> std::ffi::OsString {
        format!("{point}:{}", dir.join("release").display()).into()
    }

    /// The release path this child's [`LAUNCH_HOLD_ENV`] names.
    fn launch_hold_release() -> PathBuf {
        let spec = std::env::var(LAUNCH_HOLD_ENV).expect("the parent sets the hold");
        let (_point, release) = spec.split_once(':').expect("<point>:<path>");
        PathBuf::from(release)
    }

    /// `release`'s marker file, which a hold writes when it is reached.
    fn held_marker(release: &Path) -> PathBuf {
        let mut held = release.as_os_str().to_os_string();
        held.push(".held");
        PathBuf::from(held)
    }

    /// Wait until the hold on `release` is reached: `true`, or `false` once `worker`
    /// has finished without reaching it or [`REACH_HOLD_WITHIN`] has passed.
    fn reached_hold<T>(release: &Path, worker: &std::thread::ScopedJoinHandle<'_, T>) -> bool {
        let deadline = std::time::Instant::now() + REACH_HOLD_WITHIN;
        let held = held_marker(release);
        while !held.exists() {
            if worker.is_finished() || std::time::Instant::now() > deadline {
                return held.exists();
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        true
    }

    /// A repo `start` can launch from: `develop` carries a ROADMAP naming `phase`.
    fn init_start_repo(root: &Path, phase: PhaseId) {
        crate::test_support::init_repo(root);
        std::fs::create_dir_all(root.join(".planning")).unwrap();
        std::fs::write(
            root.join(".planning/ROADMAP.md"),
            format!("# Roadmap\n\n### Phase {phase}: reservation fixture\n"),
        )
        .unwrap();
        for args in [
            &["add", ".planning/ROADMAP.md"][..],
            &["commit", "-q", "-m", "roadmap"][..],
        ] {
            let status = devflow_core::test_support::git_command(root)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?} failed");
        }
    }

    /// `start` as these tests run it: Supervise, no worktree, the capture shape.
    fn start_for_test(root: &Path, phase: PhaseId) -> Result<(), String> {
        start(
            root,
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            false,
            false,
            false,
            None,
            false,
            true,
            FallbackSource::FlagOrConfig(None),
        )
        .map_err(|err| err.to_string())
    }

    /// The names of `root`'s liveness files.
    fn liveness_files(root: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(root.join(".devflow")) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("supervisor-") && name.ends_with(".live"))
            .collect();
        names.sort();
        names
    }

    /// D-01's tracer: `start` names its supervisor before that supervisor exists,
    /// the supervisor publishes under that id, and the publish save promotes it.
    #[test]
    fn start_reserves_then_its_supervisor_is_promoted_and_the_reservation_cleared() {
        const NAME: &str = "commands::tests::\
                            start_reserves_then_its_supervisor_is_promoted_and_the_reservation_cleared";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::go_waiting_claude_dir();
            let hold = tempfile::tempdir().unwrap();
            let spec = launch_hold_spec("reserved", hold.path());
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(LAUNCH_HOLD_ENV, spec.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5016);
        init_start_repo(root, phase);
        let release = launch_hold_release();

        let (observed, result) = std::thread::scope(|scope| {
            let worker = scope.spawn(|| start_for_test(root, phase));
            let observed =
                reached_hold(&release, &worker).then(|| workflow::load_state(root, phase).unwrap());
            std::fs::write(&release, "").unwrap();
            (observed, worker.join().unwrap())
        });
        let finished = workflow::load_state(root, phase).unwrap();
        let _reap = crate::test_support::ReapMonitorOnDrop::after_launch(&finished);
        result.expect("the start launches");

        let observed = observed.expect("start returned without reaching its reservation hold");
        let pending = observed
            .pending_supervisor
            .expect("the state names the reservation before anything is spawned");
        assert_eq!(observed.supervisor, None, "nothing is published yet");
        assert_eq!(
            finished.supervisor.as_ref().map(|r| r.launch_id.as_str()),
            Some(pending.launch_id.as_str()),
            "the supervisor publishes under the reserved id"
        );
        assert_eq!(
            finished.pending_supervisor, None,
            "the publish save promotes the reservation"
        );
        assert_eq!(
            liveness::probe(&pending.liveness_path).unwrap(),
            liveness::Held::Live,
            "the supervisor holds the reserved lock after the launcher has returned"
        );
    }

    /// `start` as the fallback tests run it: Supervise, no worktree, the capture shape,
    /// `agent` as the primary and `mode` and `fallback` as given.
    fn start_with_fallback(
        root: &Path,
        phase: PhaseId,
        agent: AgentKind,
        mode: Mode,
        fallback: Option<AgentKind>,
    ) -> Result<(), CliError> {
        start(
            root,
            phase,
            agent,
            mode,
            false,
            false,
            false,
            None,
            false,
            true,
            FallbackSource::FlagOrConfig(fallback),
        )
    }

    /// D-01: `start --fallback-agent` persists the fallback, unused, in the run state the
    /// first supervisor reads, and says which agent it stands behind.
    #[test]
    fn start_persists_the_fallback_agent_in_the_run_state() {
        const NAME: &str = "commands::tests::start_persists_the_fallback_agent_in_the_run_state";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::go_waiting_claude_dir();
            crate::test_support::add_agent_stub(path_dir.path(), "codex");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            let printed = String::from_utf8_lossy(&output.stdout);
            assert!(
                printed.contains(
                    "fallback agent: codex takes over the Code stage once if claude is rate-limited"
                ),
                "start names the fallback and the primary: {printed}"
            );
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5141);
        init_start_repo(root, phase);

        let result = start_with_fallback(
            root,
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            Some(AgentKind::Codex),
        );
        let state = workflow::load_state(root, phase).unwrap();
        let _reap = crate::test_support::ReapMonitorOnDrop::after_launch(&state);
        result.expect("the start launches");

        assert_eq!(state.agent, AgentKind::Claude);
        assert_eq!(state.fallback_agent, Some(AgentKind::Codex));
        assert!(!state.fallback_consumed, "a new fallback is unused");
        assert!(state.supervisor.is_some(), "the run launched");
    }

    /// Run the test `name` in a hermetic child whose `PATH` holds no agent binary, returning
    /// `true` in the parent, whose work is done, and `false` in the child. A refusal test
    /// runs there so that, were its refusal ever lost, the start it drives would stop at a
    /// missing binary under a scratch cache rather than reach the host's.
    fn run_in_an_agent_free_child(name: &str) -> bool {
        if devflow_core::test_support::in_child_test(name) {
            return false;
        }
        let path_dir = crate::test_support::agent_free_git_only_path_dir();
        let output = crate::test_support::run_hermetic_test_in_child(name, path_dir.path(), &[]);
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, name);
        true
    }

    /// D-02: a fallback equal to the agent is refused before anything is written, and the
    /// refusal names the agent in both roles.
    #[test]
    fn start_refuses_a_fallback_equal_to_the_agent() {
        const NAME: &str = "commands::tests::start_refuses_a_fallback_equal_to_the_agent";
        if run_in_an_agent_free_child(NAME) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5142);

        let err = start_with_fallback(
            root,
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            Some(AgentKind::Claude),
        )
        .expect_err("a fallback equal to the agent is refused")
        .to_string();

        assert!(
            err.contains("--fallback-agent claude"),
            "names the fallback: {err}"
        );
        assert!(
            err.contains("run's own agent (claude)"),
            "names the agent: {err}"
        );
        assert!(
            !workflow::state_path(root, phase).exists(),
            "no state was written"
        );
        assert!(
            !root.join(".devflow").exists(),
            "nothing was written under .devflow"
        );
    }

    /// D-03: in `--mode auto` a fallback outside the unattended launch shape is refused
    /// before anything is written, naming the fallback and the condition.
    #[test]
    fn start_in_auto_mode_refuses_a_fallback_outside_the_unattended_launch_shape() {
        const NAME: &str = "commands::tests::\
                            start_in_auto_mode_refuses_a_fallback_outside_the_unattended_launch_shape";
        if run_in_an_agent_free_child(NAME) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5143);

        let err = start_with_fallback(
            root,
            phase,
            AgentKind::Claude,
            Mode::Auto,
            Some(AgentKind::Codex),
        )
        .expect_err("codex cannot take over an unattended Code stage")
        .to_string();

        assert!(
            err.contains("--fallback-agent codex"),
            "names the fallback: {err}"
        );
        assert!(
            err.contains("unattended launch shape"),
            "names the condition: {err}"
        );
        assert!(
            !workflow::state_path(root, phase).exists(),
            "no state was written"
        );
        assert!(
            !root.join(".devflow").exists(),
            "nothing was written under .devflow"
        );
    }

    /// The launcher keeps its copy of the lock until the publish save has returned, so
    /// a supervisor that dies before it is recorded still reads live, and the launcher
    /// decides what happens next. Once `start` has returned, the launcher's copy is gone
    /// too, and the same file reads abandoned.
    #[test]
    fn the_launcher_keeps_its_lock_until_the_publish_save() {
        const NAME: &str = "commands::tests::the_launcher_keeps_its_lock_until_the_publish_save";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::go_waiting_claude_dir();
            let hold = tempfile::tempdir().unwrap();
            let spec = launch_hold_spec("before_publish_save", hold.path());
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(LAUNCH_HOLD_ENV, spec.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5024);
        init_start_repo(root, phase);
        let release = launch_hold_release();

        let (at_hold, result) = std::thread::scope(|scope| {
            let worker = scope.spawn(|| start_for_test(root, phase));
            assert!(reached_hold(&release, &worker), "start reached the publish");
            let pending = workflow::load_state(root, phase)
                .unwrap()
                .pending_supervisor
                .expect("nothing has promoted the reservation yet");
            let sidecar =
                supervisor::read_sidecar(&pending.socket_path.with_extension("json")).unwrap();
            // Only the launcher's copy is left once the supervisor is dead, and the
            // agent never held one.
            let supervisor_pid = libc::pid_t::try_from(sidecar.supervisor_pid).unwrap();
            let agent_pgid = libc::pid_t::try_from(sidecar.agent_pgid).unwrap();
            // SAFETY: both name processes this test's own start just launched, read
            // from the sidecar they wrote while they are still running.
            unsafe {
                libc::kill(supervisor_pid, libc::SIGKILL);
                libc::kill(-agent_pgid, libc::SIGKILL);
            }
            let deadline = std::time::Instant::now() + REACH_HOLD_WITHIN;
            while agent::agent_running(sidecar.supervisor_pid)
                && std::time::Instant::now() < deadline
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            // A SIGKILLed process's leader reads gone before its other threads have
            // closed its files, so watch for a while: the lock must stay held throughout.
            let watch_until = std::time::Instant::now() + std::time::Duration::from_millis(500);
            let mut at_hold = liveness::probe(&pending.liveness_path).unwrap();
            while at_hold == liveness::Held::Live && std::time::Instant::now() < watch_until {
                std::thread::sleep(std::time::Duration::from_millis(10));
                at_hold = liveness::probe(&pending.liveness_path).unwrap();
            }
            std::fs::write(&release, "").unwrap();
            (at_hold, (worker.join().unwrap(), pending.liveness_path))
        });
        let (result, liveness_path) = result;
        result.expect("the publish save itself succeeds");

        assert_eq!(
            at_hold,
            liveness::Held::Live,
            "the launcher still holds its copy before the publish save"
        );
        assert_eq!(
            liveness::probe(&liveness_path).unwrap(),
            liveness::Held::Abandoned,
            "the launcher released its copy once the publish was done"
        );
    }

    /// What a `start`'s reservation does over a state file that does not load. On success
    /// the reservation's lock is released and the state it wrote stays.
    fn reserve_over_an_unloadable_state(root: &Path, phase: PhaseId) -> Result<(), String> {
        let path = workflow::state_path(root, phase);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ this is not a state").unwrap();
        let fresh = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        let cache_dir = registry::cache_dir();
        reserve_launch(
            root,
            phase,
            LaunchVerb::Start,
            Some(&fresh),
            cache_dir.as_deref(),
        )
        .map(|reserved| reserved.reservation.discard())
        .map_err(|err| err.to_string())
    }

    /// No state names a pending launch when the state does not load, and a supervisor that
    /// has not published shows in no socket sweep: only its held lock says it is there. A
    /// `start` must not overwrite the state and launch a second one beside it.
    #[test]
    fn a_start_refuses_an_unloadable_state_beside_an_unpublished_supervisors_lock() {
        const NAME: &str = "commands::tests::a_start_refuses_an_unloadable_state_beside_an_unpublished_supervisors_lock";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::go_waiting_claude_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5035);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        let launch_id = supervisor::new_launch_id();
        let lock = liveness::launch_liveness_path(root, &launch_id).unwrap();
        let _unpublished = liveness::Hold::take(&lock).unwrap();

        let error = reserve_over_an_unloadable_state(root, phase)
            .expect_err("a live lock no state can account for refuses the start");

        assert!(error.contains(&launch_id), "{error}");
        assert_eq!(
            std::fs::read_to_string(workflow::state_path(root, phase)).unwrap(),
            "{ this is not a state",
            "the refused start left the state file as it was"
        );
        assert_eq!(
            liveness_files(root),
            vec![format!("supervisor-{launch_id}.live")],
            "the refused reservation left no liveness file"
        );
    }

    fn reserve_over_a_missing_state(root: &Path, phase: PhaseId) -> Result<(), String> {
        assert!(!workflow::state_path(root, phase).exists());
        let fresh = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        let cache_dir = registry::cache_dir();
        reserve_launch(
            root,
            phase,
            LaunchVerb::Start,
            Some(&fresh),
            cache_dir.as_deref(),
        )
        .map(|reserved| reserved.reservation.discard())
        .map_err(|err| err.to_string())
    }

    /// A `recover --clean` that removed a state beside an unpublished supervisor leaves
    /// that supervisor's held lock as the only sign of it. A `start` over the missing
    /// state must refuse, naming the holder, rather than launch a second one. Control:
    /// the same lock recorded by another phase's state is that phase's and refuses nothing.
    #[test]
    fn a_start_refuses_a_missing_state_beside_an_unrecorded_live_lock() {
        const NAME: &str =
            "commands::tests::a_start_refuses_a_missing_state_beside_an_unrecorded_live_lock";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::go_waiting_claude_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5037);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        let launch_id = supervisor::new_launch_id();
        let lock = liveness::launch_liveness_path(root, &launch_id).unwrap();
        let _unpublished = liveness::Hold::take(&lock).unwrap();

        let error = reserve_over_a_missing_state(root, phase)
            .expect_err("a live lock no state records refuses the start");

        assert!(error.contains(&launch_id), "{error}");
        assert!(
            error.contains(&format!("pid {}", std::process::id())),
            "the refusal names the holder: {error}"
        );
        assert!(!error.contains("recover --clean"), "{error}");
        assert!(!workflow::state_path(root, phase).exists(), "nothing saved");

        let mut recorder = State::new(
            PhaseId::new(5038),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
        recorder.pending_supervisor = Some(PendingSupervisor::new(
            launch_id.clone(),
            root.join("unused.sock"),
            lock.clone(),
        ));
        workflow::save_state(&recorder).unwrap();
        reserve_over_a_missing_state(root, phase)
            .expect("control: a live lock another phase records refuses nothing");
    }

    /// The control: with no live lock, or only a released one, a `start` still replaces a
    /// state file that does not load, the repair the reservation's note names.
    #[test]
    fn a_start_replaces_an_unloadable_state_beside_a_released_lock() {
        const NAME: &str =
            "commands::tests::a_start_replaces_an_unloadable_state_beside_a_released_lock";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::go_waiting_claude_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5036);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        let lock = liveness::launch_liveness_path(root, &supervisor::new_launch_id()).unwrap();
        drop(liveness::Hold::take(&lock).unwrap());

        reserve_over_an_unloadable_state(root, phase).expect("a released lock refuses nothing");

        assert!(
            workflow::load_state(root, phase).is_ok(),
            "the reservation replaced the state file"
        );
    }

    /// What [`start_failing_its_publish_save`] left. Dropping it ends the supervisor and
    /// its agent, so a failed assertion leaks neither.
    struct FailedStart {
        error: String,
        launch_id: String,
        socket: PathBuf,
        supervisor_pid: libc::pid_t,
        agent_pgid: libc::pid_t,
        state_left: bool,
    }

    impl Drop for FailedStart {
        fn drop(&mut self) {
            // SAFETY: both name processes this test's own start launched; a SIGKILL also
            // ends a stopped process. A group already gone only makes these fail.
            unsafe {
                libc::kill(self.supervisor_pid, libc::SIGKILL);
                libc::kill(-self.agent_pgid, libc::SIGKILL);
            }
        }
    }

    /// A `start` whose publish save fails, with `stop_supervisor` deciding whether the
    /// launcher can get a confirmed stop out of the supervisor it spawned. The reservation
    /// is cleared at the hold, which is what fails the save.
    fn start_failing_its_publish_save(
        root: &Path,
        phase: PhaseId,
        stop_supervisor: bool,
    ) -> FailedStart {
        init_start_repo(root, phase);
        let release = launch_hold_release();
        let (result, pending, sidecar) = std::thread::scope(|scope| {
            let worker = scope.spawn(|| start_for_test(root, phase));
            assert!(reached_hold(&release, &worker), "start reached the publish");
            let pending = workflow::load_state(root, phase)
                .unwrap()
                .pending_supervisor
                .expect("nothing has promoted the reservation yet");
            let sidecar =
                supervisor::read_sidecar(&pending.socket_path.with_extension("json")).unwrap();
            if stop_supervisor {
                // SAFETY: names the supervisor this test's own start just launched, read
                // from the sidecar it wrote while it is still running.
                unsafe {
                    libc::kill(
                        libc::pid_t::try_from(sidecar.supervisor_pid).unwrap(),
                        libc::SIGSTOP,
                    );
                }
            }
            workflow::update_state_with(root, phase, |existing| {
                let workflow::Existing::Loaded(mut state) = existing else {
                    panic!("the reservation's state loads");
                };
                state.pending_supervisor = None;
                Ok::<_, CliError>((Some(state), ()))
            })
            .unwrap();
            std::fs::write(&release, "").unwrap();
            (worker.join().unwrap(), pending, sidecar)
        });
        FailedStart {
            error: result.expect_err("the publish save fails"),
            launch_id: pending.launch_id,
            socket: pending.socket_path,
            supervisor_pid: libc::pid_t::try_from(sidecar.supervisor_pid).unwrap(),
            agent_pgid: libc::pid_t::try_from(sidecar.agent_pgid).unwrap(),
            state_left: workflow::state_path(root, phase).exists(),
        }
    }

    /// The `start --force` an operator runs after a failed one: the failed start left its
    /// feature branch, which a plain `start` refuses only after the live-run guard has passed.
    fn start_again_with_force(root: &Path, phase: PhaseId) -> Result<(), String> {
        start(
            root,
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            true,
            false,
            false,
            None,
            false,
            true,
            FallbackSource::FlagOrConfig(None),
        )
        .map_err(|err| err.to_string())
    }

    /// Run the test `name` in a hermetic child with the publish hold set, returning `true`
    /// in the parent, whose work is done, and `false` in the child.
    fn parent_of_a_failed_publish_child(name: &str) -> bool {
        if devflow_core::test_support::in_child_test(name) {
            return false;
        }
        let path_dir = crate::test_support::go_waiting_claude_dir();
        let hold = tempfile::tempdir().unwrap();
        let spec = launch_hold_spec("before_publish_save", hold.path());
        let output = crate::test_support::run_hermetic_test_in_child(
            name,
            path_dir.path(),
            &[(LAUNCH_HOLD_ENV, spec.as_os_str())],
        );
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, name);
        true
    }

    /// A failed `start` whose supervisor never confirmed a stop leaves no state, so the
    /// next `start` reads `Missing` and sweeps the sockets. A state naming neither the record
    /// nor the reservation would skip that sweep and launch beside the supervisor.
    #[test]
    fn a_second_start_refuses_beside_a_failed_starts_stopped_supervisor() {
        const NAME: &str =
            "commands::tests::a_second_start_refuses_beside_a_failed_starts_stopped_supervisor";
        if parent_of_a_failed_publish_child(NAME) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (root, phase) = (dir.path(), PhaseId::new(5033));
        let failed = start_failing_its_publish_save(root, phase, true);
        assert!(
            failed.error.contains("did not confirm a stop"),
            "{}",
            failed.error
        );
        // Only the supervisor is left, so its socket is all that can refuse: a live agent
        // would also trip the agent pid file check on any state that kept that check on.
        // SAFETY: names the agent group this test's own start launched.
        unsafe { libc::kill(-failed.agent_pgid, libc::SIGKILL) };

        let second = start_again_with_force(root, phase);
        let _reap = workflow::load_state(root, phase)
            .ok()
            .map(|state| crate::test_support::ReapMonitorOnDrop::after_launch(&state));

        let refused = second.expect_err("a second supervisor was launched beside the first");
        assert!(refused.contains(&failed.launch_id), "{refused}");
    }

    /// With the socket and sidecar gone the sweep has nothing to find, but the stopped
    /// supervisor still holds its liveness lock, which no state records: the same second
    /// `start` refuses through that lock, naming the supervisor's pid, instead of
    /// launching a second supervisor beside it.
    #[test]
    fn a_second_start_refuses_through_the_held_lock_when_the_sweep_has_nothing_to_find() {
        const NAME: &str = "commands::tests::a_second_start_refuses_through_the_held_lock_when_the_sweep_has_nothing_to_find";
        if parent_of_a_failed_publish_child(NAME) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (root, phase) = (dir.path(), PhaseId::new(5037));
        let failed = start_failing_its_publish_save(root, phase, true);
        std::fs::remove_file(&failed.socket).unwrap();
        std::fs::remove_file(failed.socket.with_extension("json")).unwrap();

        let second = start_again_with_force(root, phase);
        let _reap = workflow::load_state(root, phase)
            .ok()
            .map(|state| crate::test_support::ReapMonitorOnDrop::after_launch(&state));

        let refused = second.expect_err("a second supervisor was launched beside the first");
        assert!(refused.contains(&failed.launch_id), "{refused}");
        assert!(
            refused.contains(&format!("held by pid {}", failed.supervisor_pid)),
            "the refusal comes from the held lock and names its holder: {refused}"
        );
        // SAFETY: names the agent group and supervisor this test's own start launched.
        unsafe {
            libc::kill(-failed.agent_pgid, libc::SIGKILL);
            libc::kill(failed.supervisor_pid, libc::SIGKILL);
        }
    }

    /// The sidecar alone is not what the sweep needs: the socket's name still attributes it
    /// to this root and phase, so a missing sidecar refuses too.
    #[test]
    fn a_second_start_refuses_when_only_the_sidecar_is_gone() {
        const NAME: &str = "commands::tests::a_second_start_refuses_when_only_the_sidecar_is_gone";
        if parent_of_a_failed_publish_child(NAME) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (root, phase) = (dir.path(), PhaseId::new(5038));
        let failed = start_failing_its_publish_save(root, phase, true);
        std::fs::remove_file(failed.socket.with_extension("json")).unwrap();

        let second = start_again_with_force(root, phase);
        let _reap = workflow::load_state(root, phase)
            .ok()
            .map(|state| crate::test_support::ReapMonitorOnDrop::after_launch(&state));

        let refused = second.expect_err("the socket's name still attributes it");
        assert!(refused.contains(&failed.launch_id), "{refused}");
    }

    /// A known limit, and what the failed `start`'s error has to stay true about: `stop`
    /// finds a suspended supervisor with no state file but cannot get an answer out of it,
    /// so it refuses and changes nothing. The error names the way out.
    #[test]
    fn the_stop_a_failed_start_names_refuses_over_a_suspended_supervisor() {
        const NAME: &str =
            "commands::tests::the_stop_a_failed_start_names_refuses_over_a_suspended_supervisor";
        if parent_of_a_failed_publish_child(NAME) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (root, phase) = (dir.path(), PhaseId::new(5039));
        let failed = start_failing_its_publish_save(root, phase, true);
        assert!(!failed.state_left, "the run's state is gone");
        for named in [
            format!("devflow stop --phase {phase} --launch {}", failed.launch_id),
            format!("kill -CONT {}", failed.supervisor_pid),
        ] {
            assert!(failed.error.contains(&named), "{named}: {}", failed.error);
        }

        let refused = stop_launch(root, phase, &failed.launch_id)
            .expect_err("no answer comes from a suspended supervisor")
            .to_string();

        assert!(refused.contains("could not probe socket"), "{refused}");
        assert!(refused.contains("nothing was signalled"), "{refused}");
    }

    /// Once a suspended supervisor runs again it ends. Which way is a race and not asserted:
    /// the shutdown still queued from the failed `start` can end it under this `stop`, which
    /// then returns Ok, or an error when its sidecar has gone mid-lookup.
    #[test]
    fn a_failed_starts_suspended_supervisor_ends_once_it_runs_and_is_stopped() {
        const NAME: &str = "commands::tests::a_failed_starts_suspended_supervisor_ends_once_it_runs_and_is_stopped";
        if parent_of_a_failed_publish_child(NAME) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (root, phase) = (dir.path(), PhaseId::new(5040));
        let failed = start_failing_its_publish_save(root, phase, true);
        // SAFETY: names the supervisor this test's own start launched.
        unsafe { libc::kill(failed.supervisor_pid, libc::SIGCONT) };

        let _either_outcome = stop_launch(root, phase, &failed.launch_id);

        let deadline = std::time::Instant::now() + REACH_HOLD_WITHIN;
        while agent::agent_running(u32::try_from(failed.supervisor_pid).unwrap()) {
            assert!(std::time::Instant::now() < deadline, "the supervisor ended");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// The same failed publish, with a supervisor that confirms its stop, still leaves no
    /// phantom run. This is what shows the check above reads the supervisor, not the
    /// failure.
    #[test]
    fn a_start_clears_the_state_when_its_supervisor_confirmed_the_stop() {
        const NAME: &str =
            "commands::tests::a_start_clears_the_state_when_its_supervisor_confirmed_the_stop";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::go_waiting_claude_dir();
            let hold = tempfile::tempdir().unwrap();
            let spec = launch_hold_spec("before_publish_save", hold.path());
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(LAUNCH_HOLD_ENV, spec.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let failed = start_failing_its_publish_save(dir.path(), PhaseId::new(5034), false);
        assert!(
            failed.error.contains("was shut down through its socket"),
            "{}",
            failed.error
        );
        assert!(
            !failed.state_left,
            "no supervisor is left: {}",
            failed.error
        );
    }

    /// A `start` that fails after reserving gives the reservation back: a state it
    /// created is gone, an earlier run's state is as it was, and no liveness file is
    /// left. The hold shows the reservation was really made, or a start that never
    /// reserved would pass every check after it.
    #[test]
    fn a_start_that_fails_after_reserving_gives_the_reservation_back() {
        const NAME: &str =
            "commands::tests::a_start_that_fails_after_reserving_gives_the_reservation_back";
        if !devflow_core::test_support::in_child_test(NAME) {
            // No `claude` on this PATH, so `ensure_agent_binary` refuses the start.
            let path_dir = crate::test_support::agent_free_git_only_path_dir();
            for case in ["no_state", "dead_run"] {
                let hold = tempfile::tempdir().unwrap();
                let spec = launch_hold_spec("reserved", hold.path());
                let output = crate::test_support::run_hermetic_test_in_child(
                    NAME,
                    path_dir.path(),
                    &[
                        (LAUNCH_HOLD_ENV, spec.as_os_str()),
                        (RESERVE_CASE_ENV, std::ffi::OsStr::new(case)),
                    ],
                );
                devflow_core::test_support::assert_child_ran_exactly_one_passing_test(
                    &output, NAME,
                );
            }
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5017);
        let dead_run = std::env::var(RESERVE_CASE_ENV).unwrap() == "dead_run";
        let state_file = workflow::state_path(root, phase);
        if dead_run {
            let mut old = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            old.stage = Stage::Code;
            old.stopped = true;
            workflow::save_state(&old).unwrap();
        }
        let before = std::fs::read(&state_file).ok();
        let release = launch_hold_release();

        let (observed, result) = std::thread::scope(|scope| {
            let worker = scope.spawn(|| start_for_test(root, phase));
            let observed =
                reached_hold(&release, &worker).then(|| workflow::load_state(root, phase).unwrap());
            std::fs::write(&release, "").unwrap();
            (observed, worker.join().unwrap())
        });

        let observed = observed.expect("start returned without reaching its reservation hold");
        assert!(observed.pending_supervisor.is_some(), "the start reserved");
        let err = result.expect_err("no agent binary resolves");
        assert!(err.contains("not found"), "{err}");
        assert_eq!(
            std::fs::read(&state_file).ok(),
            before,
            "the state file is back as it was before the start"
        );
        assert_eq!(liveness_files(root), Vec::<String>::new());
    }

    /// The state of a run whose recorded supervisor's socket is gone, with the launch's
    /// liveness file taken as that supervisor would hold it.
    fn state_with_a_held_lock_and_no_socket(
        root: &Path,
        phase: PhaseId,
    ) -> (State, liveness::Hold) {
        let mut state = state_with_pending(root, phase, true);
        state.pending_supervisor = None;
        state.verification_baseline_captured = true;
        workflow::save_state(&state).unwrap();
        let launch_id = &state.supervisor.as_ref().unwrap().launch_id;
        let path = liveness::launch_liveness_path(root, launch_id).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let hold = liveness::Hold::take(&path).unwrap();
        (state, hold)
    }

    /// A supervisor whose state vanished ends with no exit file and no successor, so a
    /// follower must end too, saying why. Control: while the state exists it keeps following.
    #[test]
    fn logs_follow_ends_when_the_followed_state_is_gone_and_no_exit_file_will_come() {
        const WAIT: std::time::Duration = std::time::Duration::from_secs(10);
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(5306);
        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.clone());
        workflow::save_state(&state).unwrap();

        let (sender, receiver) = std::sync::mpsc::channel();
        let follower_root = root.clone();
        std::thread::spawn(move || {
            let _ = sender.send(logs(&follower_root, Some(phase), true, false));
        });
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_millis(1500))
                .is_err(),
            "control: a follower keeps following while the state exists"
        );
        std::fs::remove_file(workflow::state_path(&root, phase)).unwrap();
        receiver
            .recv_timeout(WAIT)
            .expect("the follower ends once the state is gone")
            .unwrap();
    }

    /// A follower started after the state is already gone, with no exit file, has no run
    /// to wait for and must end too: no state is ever observed, so nothing may hinge on
    /// having seen one. Control: the test above keeps following while the state exists.
    #[test]
    fn logs_follow_ends_when_no_state_exists_from_the_start_and_no_exit_file_will_come() {
        const WAIT: std::time::Duration = std::time::Duration::from_secs(10);
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(5307);
        let capture = agent_result::stdout_path(&root, phase);
        std::fs::create_dir_all(capture.parent().unwrap()).unwrap();
        std::fs::write(&capture, "earlier output\n").unwrap();
        assert!(!workflow::state_path(&root, phase).exists());

        let (sender, receiver) = std::sync::mpsc::channel();
        let follower_root = root.clone();
        std::thread::spawn(move || {
            let _ = sender.send(logs(&follower_root, Some(phase), true, false));
        });
        receiver
            .recv_timeout(WAIT)
            .expect("the follower ends when there was never a state to follow")
            .unwrap();
    }

    /// The launch guard refuses beside a recorded supervisor that holds its liveness file
    /// with its socket gone, names that process rather than a `stop` that cannot reach it,
    /// and stops refusing once the lock is released.
    #[test]
    fn a_launch_refuses_beside_a_held_lock_with_its_socket_gone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5304);
        let (state, hold) = state_with_a_held_lock_and_no_socket(root, phase);
        let existing = workflow::Existing::Loaded(state);

        let refusal =
            refuse_launch_over_a_live_run(root, phase, LaunchVerb::Resume, &existing, None)
                .unwrap_err()
                .to_string();
        assert!(refusal.contains("aaaa0000aaaa5018"), "{refusal}");
        assert!(
            refusal.contains(&format!("pid {}", std::process::id())),
            "{refusal}"
        );
        assert!(refusal.contains("cannot reach it"), "{refusal}");
        assert!(
            !refusal.contains("devflow stop --phase 5304 --launch"),
            "{refusal}"
        );

        let path = hold.path().to_path_buf();
        drop(hold);
        wait_until_unheld(&path);
        refuse_launch_over_a_live_run(root, phase, LaunchVerb::Resume, &existing, None).unwrap();
    }

    /// A supervisor unlinks its socket before it exits, so a lock released a moment later
    /// is one exiting, not a live run: the guard waits it out rather than refuse.
    #[test]
    fn a_launch_waits_out_a_supervisor_exiting_with_its_socket_gone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5305);
        let (state, hold) = state_with_a_held_lock_and_no_socket(root, phase);
        let existing = workflow::Existing::Loaded(state);

        let releaser = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            drop(hold);
        });
        let guarded =
            refuse_launch_over_a_live_run(root, phase, LaunchVerb::Resume, &existing, None);
        releaser.join().unwrap();
        guarded.expect("a lock released within the wait is not a live supervisor");
    }

    /// `stop` does not record `stopped` and exit 0 beside a supervisor whose socket is
    /// gone, which it could not have ended. Control: with the lock released, it does.
    #[test]
    fn stop_refuses_a_held_lock_with_its_socket_gone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5305);
        let (_state, hold) = state_with_a_held_lock_and_no_socket(root, phase);
        let cache = tempfile::tempdir().unwrap();

        let err = stop_in(root, phase, Some(cache.path()))
            .unwrap_err()
            .to_string();
        assert!(err.contains("cannot reach it"), "{err}");
        assert!(!workflow::load_state(root, phase).unwrap().stopped);

        let path = hold.path().to_path_buf();
        drop(hold);
        wait_until_unheld(&path);
        stop_in(root, phase, Some(cache.path())).unwrap();
        assert!(workflow::load_state(root, phase).unwrap().stopped);
    }

    /// `status` and `doctor` report a held liveness lock with no socket as a process holding
    /// the lock, by pid, and name no `resume`, which refuses it. Control: with the lock
    /// released, both read the run as gone and name `resume` again.
    #[test]
    fn status_and_doctor_name_the_process_holding_a_lock_with_no_socket() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5306);
        let (state, hold) = state_with_a_held_lock_and_no_socket(root, phase);
        let pid = format!("pid {}", std::process::id());
        let resume = format!("devflow resume --phase {phase}");
        let doctor = |state: &State| {
            reconcile_phase(&facts_for(root, state))
                .iter()
                .map(|finding| format!("{} -> {:?}", finding.detail, finding.repair))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let status = rendered_status(&state);
        assert!(status.contains("holds its liveness lock"), "{status}");
        assert!(status.contains(&pid), "{status}");
        assert!(!status.contains(&resume), "{status}");
        let found = doctor(&state);
        assert!(found.contains("holds its liveness lock"), "{found}");
        assert!(found.contains(&pid), "{found}");
        assert!(!found.contains(&resume), "{found}");

        let path = hold.path().to_path_buf();
        drop(hold);
        wait_until_unheld(&path);
        let status = rendered_status(&state);
        assert!(status.contains(&resume), "{status}");
        assert!(!status.contains("holds its liveness lock"), "{status}");
        assert!(doctor(&state).contains(&resume), "{}", doctor(&state));
    }

    /// A stored Ship answer with a dead supervisor is `devflow ship`'s: `status` and `doctor`
    /// name it and never the `resume` that sets the answer aside. Controls: with no answer,
    /// and with the answer already claimed, both name `resume` as before.
    #[test]
    fn status_and_doctor_name_ship_for_a_stored_ship_answer() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5307);
        state_pending_at(root, phase, Stage::Ship);
        let opened = Gates::open_gate(root, phase, Stage::Ship, "g").unwrap();
        let state = workflow::load_state(root, phase).unwrap();
        let ship = format!("devflow ship --phase {phase}");
        let resume = format!("devflow resume --phase {phase}");
        let stop = format!("devflow stop --phase {phase}");

        assert!(
            recovery_hints(&state, &PhaseLiveness::GoneWhileActive).contains(&resume),
            "control: nothing stored yet"
        );

        gate_respond(root, phase, Some(Stage::Ship), true, None, None).unwrap();

        assert_eq!(
            recovery_hints(&state, &PhaseLiveness::GoneWhileActive),
            vec![ship.clone()]
        );
        assert_eq!(
            recovery_hints(&state, &PhaseLiveness::Stale),
            vec![stop, ship.clone()]
        );
        let mut stopped = state.clone();
        stopped.stopped = true;
        assert_eq!(
            recovery_hints(&stopped, &PhaseLiveness::Gone),
            vec![ship.clone()]
        );
        let open_gates = Gates::list_open(root);
        let facts = build_phase_facts(
            root,
            state.clone(),
            &mut std::collections::HashMap::new(),
            &open_gates,
        );
        let findings = reconcile_phase(&facts);
        let gate_finding = findings
            .iter()
            .find(|finding| finding.detail.contains("gate_pending"))
            .expect("the finding about gate_pending");
        assert!(
            gate_finding.detail.contains("holds a stored answer"),
            "{}",
            gate_finding.detail
        );
        assert_eq!(gate_finding.repair.as_deref(), Some(ship.as_str()));
        for finding in &findings {
            let repair = finding.repair.as_deref().unwrap_or_default();
            assert!(!repair.contains("devflow resume"), "{repair}");
        }

        std::fs::write(
            Gates::claim_path(root, phase, Stage::Ship, &opened.gate_id),
            "",
        )
        .unwrap();
        assert!(
            recovery_hints(&state, &PhaseLiveness::GoneWhileActive).contains(&resume),
            "control: a claimed answer is out of `devflow ship`'s reach"
        );
    }

    /// The state a `start` leaves when it is killed during preparation: its real
    /// reservation, with the lock released as a dead process's is.
    fn state_of_a_start_killed_during_preparation(root: &Path, phase: PhaseId) -> State {
        state_of_a_start_killed_in_mode(root, phase, Mode::Auto)
    }

    /// [`state_of_a_start_killed_during_preparation`] for a start in `mode`.
    fn state_of_a_start_killed_in_mode(root: &Path, phase: PhaseId, mode: Mode) -> State {
        let fresh = State::new(phase, AgentKind::Claude, mode, root.to_path_buf());
        let cache = tempfile::tempdir().unwrap();
        let reserved = reserve_launch(
            root,
            phase,
            LaunchVerb::Start,
            Some(&fresh),
            Some(cache.path()),
        )
        .unwrap();
        reserved.reservation.discard();
        workflow::load_state(root, phase).unwrap()
    }

    /// `resume` over a `start` killed during preparation refuses before anything is
    /// written, and names `start`: the state has no worktree, so a launch would run the
    /// agent in the primary checkout. Control: the same state once preparation has run
    /// gets past the refusal to the agent binary check, so the refusal is about this
    /// state and not about every `resume`.
    #[test]
    fn resume_refuses_a_start_killed_during_preparation() {
        const NAME: &str = "commands::tests::resume_refuses_a_start_killed_during_preparation";
        if !devflow_core::test_support::in_child_test(NAME) {
            // No `claude` on this PATH, so a resume that gets past the refusal fails
            // on the binary check and spawns nothing.
            let path_dir = crate::test_support::agent_free_git_only_path_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5301);
        let killed = state_of_a_start_killed_during_preparation(root, phase);
        assert!(killed.pending_supervisor.is_some(), "the start reserved");

        let err = crate::pipeline_launch::resume(root, phase, None, false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("never finished preparing"), "{err}");
        assert!(
            err.contains(&format!(
                "devflow start --phase {phase} --agent claude --mode auto"
            )),
            "{err}"
        );
        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(after.stage, Stage::Define);
        assert!(after.supervisor.is_none());
        assert!(!after.stopped, "the refusal marks nothing");

        let err = crate::pipeline_launch::advance_command(root, Some(phase), None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("never finished preparing"), "{err}");
        assert_eq!(
            workflow::load_state(root, phase).unwrap().stage,
            Stage::Define,
            "a hand-run advance launched nothing"
        );

        let prepared = PhaseId::new(5302);
        let mut state = State::new(prepared, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.verification_baseline_captured = true;
        workflow::save_state(&state).unwrap();
        let err = crate::pipeline_launch::resume(root, prepared, None, false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("not found"), "{err}");
    }

    /// A real `start` of what a killed one's refusal names, over `root`: a worktree start
    /// in Supervise mode with a stub agent, which returns once it has launched.
    fn start_as_refusal_names(root: &Path, phase: PhaseId, force: bool) -> Result<(), String> {
        start(
            root,
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            force,
            true,
            false,
            None,
            false,
            true,
            FallbackSource::FlagOrConfig(None),
        )
        .map_err(|err| err.to_string())
    }

    /// With no worktree, the refusal names a plain `start`, and that start launches.
    #[test]
    fn a_killed_start_with_no_worktree_names_a_plain_start_that_launches() {
        const NAME: &str =
            "commands::tests::a_killed_start_with_no_worktree_names_a_plain_start_that_launches";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::go_waiting_claude_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (root, phase) = (dir.path(), PhaseId::new(5304));
        init_start_repo(root, phase);
        let killed = state_of_a_start_killed_in_mode(root, phase, Mode::Supervise);

        let err = refuse_unprepared_run(&killed).unwrap_err().to_string();
        assert!(
            err.contains(&format!(
                "run `devflow start --phase {phase} --agent claude --mode supervise`"
            )),
            "{err}"
        );

        let result = start_as_refusal_names(root, phase, false);
        let launched = workflow::load_state(root, phase).unwrap();
        let _reap = crate::test_support::ReapMonitorOnDrop::after_launch(&launched);
        result.expect("the start the refusal names launches");
    }

    /// With a worktree already there, a plain `start` refuses, so the refusal and the hints
    /// name `--force` and say what it discards, and that start launches.
    #[test]
    fn a_killed_start_with_a_worktree_names_a_force_start_that_launches() {
        const NAME: &str =
            "commands::tests::a_killed_start_with_a_worktree_names_a_force_start_that_launches";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::go_waiting_claude_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (root, phase) = (dir.path(), PhaseId::new(5305));
        init_start_repo(root, phase);
        let killed = state_of_a_start_killed_in_mode(root, phase, Mode::Supervise);
        let worktree_path = ensure_phase_worktree(root, phase, false, DEVELOP).unwrap();

        let err = refuse_unprepared_run(&killed).unwrap_err().to_string();
        let verb = format!("devflow start --phase {phase} --agent claude --mode supervise --force");
        assert!(err.contains(&format!("run `{verb}")), "{err}");
        assert!(err.contains("discard"), "{err}");
        assert!(err.contains(&*worktree_path.to_string_lossy()), "{err}");
        assert!(!err.contains("no worktree"), "{err}");
        let hints = recovery_hints(&killed, &PhaseLiveness::NotLaunched);
        assert!(hints[0].starts_with(&verb), "{hints:?}");

        let refused = start_as_refusal_names(root, phase, false).unwrap_err();
        assert!(refused.contains("use --force"), "{refused}");

        let result = start_as_refusal_names(root, phase, true);
        let launched = workflow::load_state(root, phase).unwrap();
        let _reap = crate::test_support::ReapMonitorOnDrop::after_launch(&launched);
        result.expect("the start the refusal names launches");
    }

    /// `status` and `doctor` name `start`, never `resume`, for a `start` killed during
    /// preparation, stopped or not. Control: with preparation done, both name `resume`.
    #[test]
    fn status_and_doctor_name_start_for_a_start_killed_during_preparation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5303);
        let start_verb = format!("devflow start --phase {phase} --agent claude --mode supervise");
        let resume_verb = format!("devflow resume --phase {phase}");
        let killed = state_with_pending(root, phase, false);
        let mut stopped = killed.clone();
        stopped.stopped = true;
        let mut unreserved = killed.clone();
        unreserved.pending_supervisor = None;

        for (case, state) in [
            ("killed", &killed),
            ("stopped after", &stopped),
            ("a refused resume after", &unreserved),
        ] {
            let liveness = probe_phase(state);
            assert_eq!(
                recovery_hints(state, &liveness),
                vec![start_verb.clone()],
                "{case}"
            );
        }
        let findings = reconcile_phase(&facts_for(root, &killed));
        let repairs: Vec<&str> = findings
            .iter()
            .filter_map(|finding| finding.repair.as_deref())
            .collect();
        assert!(repairs.contains(&start_verb.as_str()), "{repairs:?}");
        assert!(
            !repairs
                .iter()
                .any(|repair| repair.contains("devflow resume")),
            "{repairs:?}"
        );

        let mut prepared = killed.clone();
        prepared.verification_baseline_captured = true;
        assert_eq!(
            recovery_hints(&prepared, &probe_phase(&prepared)),
            vec![resume_verb.clone()]
        );
        let findings = reconcile_phase(&facts_for(root, &prepared));
        assert!(
            findings
                .iter()
                .any(|finding| finding.repair.as_deref() == Some(resume_verb.as_str())),
            "control: a prepared run names resume"
        );
    }

    /// A `start` refused during preparation, over a state file that does not load, leaves
    /// that file's bytes on disk. Nothing else keeps them: its reservation replaced the file,
    /// and giving the reservation back removes what it wrote.
    #[test]
    fn a_start_refused_during_preparation_keeps_an_unloadable_states_bytes() {
        const NAME: &str =
            "commands::tests::a_start_refused_during_preparation_keeps_an_unloadable_states_bytes";
        const BYTES: &[u8] = b"{ damaged by hand";
        if !devflow_core::test_support::in_child_test(NAME) {
            // No `claude` on this PATH, so `ensure_agent_binary` refuses the start.
            let path_dir = crate::test_support::agent_free_git_only_path_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5308);
        let state_file = workflow::state_path(root, phase);
        std::fs::create_dir_all(state_file.parent().unwrap()).unwrap();
        std::fs::write(&state_file, BYTES).unwrap();

        let err = start_for_test(root, phase).expect_err("no agent binary resolves");
        assert!(err.contains("not found"), "{err}");

        let kept: Vec<Vec<u8>> = std::fs::read_dir(state_file.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains("unloadable")
            })
            .map(|path| std::fs::read(path).unwrap())
            .collect();
        assert_eq!(
            kept,
            vec![BYTES.to_vec()],
            "the bytes survive under one name"
        );
        assert_eq!(
            workflow::list_states(root).len(),
            0,
            "the copy is not a state"
        );
    }

    /// RESEARCH Pitfall 7: a socket whose launch id is the state's pending supervisor is
    /// claimed, with or without a recorded supervisor beside it, so `doctor` does not
    /// call a legitimate next supervisor an orphan. Controls: the same socket with no
    /// pending record, the state naming another launch, is reported as superseded; and
    /// with the pending record naming another launch, too.
    #[test]
    fn doctor_does_not_call_a_pending_next_supervisor_superseded() {
        use doctor_reconciliation::{
            publish_launch, save_state_with_launch, short_cache, write_sidecar_for,
        };
        const RECORDED: &str = "aaaa0000aaaa1709";
        const NEXT: &str = "bbbb0000bbbb1709";
        const OTHER: &str = "cccc0000cccc1709";
        const PHASE: u32 = 1709;
        let cache = short_cache();
        let cache = cache.path();
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let phase = PhaseId::new(PHASE);
        let (next, _) = publish_launch(cache, root, PHASE, NEXT);
        write_sidecar_for(&next, root, PHASE, NEXT);
        save_state_with_launch(root, PHASE, RECORDED);
        let set_pending = |id: Option<&str>| {
            workflow::update_state(root, phase, |state| {
                state.pending_supervisor = id.map(|id| {
                    PendingSupervisor::new(
                        id.to_string(),
                        next.as_path().to_path_buf(),
                        root.join(format!("{id}.live")),
                    )
                });
                Ok::<(), workflow::WorkflowError>(())
            })
            .unwrap();
        };
        let details = || -> Vec<String> {
            collect_orphan_supervisors(root, cache)
                .into_iter()
                .map(|orphan| orphan.finding.detail)
                .collect()
        };

        set_pending(Some(NEXT));
        assert_eq!(details(), Vec::<String>::new(), "beside a recorded one");
        workflow::update_state(root, phase, |state| {
            state.supervisor = None;
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();
        assert_eq!(details(), Vec::<String>::new(), "with no recorded one");

        save_state_with_launch(root, PHASE, RECORDED);
        for (case, pending) in [("no pending record", None), ("another's", Some(OTHER))] {
            set_pending(pending);
            let listed = details();
            assert_eq!(listed.len(), 1, "control, {case}: {listed:?}");
            assert!(
                listed[0].contains(NEXT) && listed[0].contains("superseded"),
                "control, {case}: {listed:?}"
            );
        }
    }

    const AGENT_SPAWN_HOLD_ENV: &str = "DEVFLOW_TEST_AGENT_SPAWN_HOLD";
    /// Carries a fixture's project root to its children. Not a DevFlow setting.
    const RESERVE_ROOT_ENV: &str = "P50_1_16_ROOT";

    /// `verb` as the second launcher in these tests runs it.
    fn launch_for_test(root: &Path, phase: PhaseId, verb: LaunchVerb) -> Result<(), String> {
        match verb {
            LaunchVerb::Start => start_for_test(root, phase),
            LaunchVerb::Resume => crate::pipeline_launch::resume(root, phase, None, true)
                .map_err(|err| err.to_string()),
        }
    }

    /// A supervisor held before its agent spawn, ended through its socket by
    /// [`HeldSupervisor::end`] or, if an assertion fails first, on drop.
    struct HeldSupervisor {
        release: PathBuf,
        cache: PathBuf,
    }

    impl HeldSupervisor {
        /// Release the hold, wait for the supervisor to publish and stop it through
        /// its socket. Whether a supervisor confirmed a stop.
        fn end(&self) -> bool {
            let _ = std::fs::write(&self.release, "");
            let deadline = std::time::Instant::now() + REACH_HOLD_WITHIN;
            while std::time::Instant::now() < deadline {
                let alive: Vec<PathBuf> = supervisor::sweep_in(&self.cache)
                    .into_iter()
                    .filter(|entry| matches!(entry.liveness, Ok(supervisor::Liveness::Alive(_))))
                    .map(|entry| entry.socket_path)
                    .collect();
                if !alive.is_empty() {
                    return alive.iter().fold(true, |stopped, socket| {
                        let outcome = supervisor::request_shutdown(socket, REACH_HOLD_WITHIN);
                        stopped
                            && matches!(outcome, Ok(supervisor::ShutdownOutcome::Stopped { .. }))
                    });
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            false
        }
    }

    impl Drop for HeldSupervisor {
        fn drop(&mut self) {
            if !self.release.exists() {
                self.end();
            }
        }
    }

    /// V-01: a launcher SIGKILLed during its publish wait leaves a live supervisor
    /// that the state names only as pending. It is held before its agent spawn, so
    /// the agent pid file cannot be what refuses the second launch.
    fn second_launch_refuses_over_a_dead_launchers_supervisor(
        name: &str,
        phase: PhaseId,
        verb: LaunchVerb,
    ) {
        if devflow_core::test_support::in_child_test(name) {
            let root = PathBuf::from(std::env::var_os(RESERVE_ROOT_ENV).unwrap());
            match std::env::var(RESERVE_CASE_ENV).unwrap().as_str() {
                "launcher" => {
                    // Killed by the parent while it waits for the publish.
                    let _ = launch_for_test(&root, phase, verb);
                }
                _ => assert_second_launch_is_refused(&root, phase, verb),
            }
            return;
        }
        let path_dir = crate::test_support::go_waiting_claude_dir();
        let cache = crate::test_support::hermetic_cache_dir();
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        init_start_repo(root, phase);
        if verb == LaunchVerb::Resume {
            let mut stopped = State::new(
                phase,
                AgentKind::Claude,
                Mode::Supervise,
                root.to_path_buf(),
            );
            stopped.stopped = true;
            stopped.verification_baseline_captured = true;
            stopped.legacy_claude_launch = true;
            workflow::save_state(&stopped).unwrap();
        }
        let hold = tempfile::tempdir().unwrap();
        let release = hold.path().join("release");
        let spawn_hold: std::ffi::OsString = format!("define:{}", release.display()).into();
        let mut launcher = devflow_core::test_support::spawn_test_in_child(
            name,
            path_dir.path(),
            &[
                ("DEVFLOW_CACHE_DIR", cache.path().as_os_str()),
                (RESERVE_ROOT_ENV, root.as_os_str()),
                (RESERVE_CASE_ENV, std::ffi::OsStr::new("launcher")),
                (AGENT_SPAWN_HOLD_ENV, spawn_hold.as_os_str()),
            ],
        );
        let supervisor = HeldSupervisor {
            release: release.clone(),
            cache: cache.path().to_path_buf(),
        };
        let held = held_marker(&release);
        let deadline = std::time::Instant::now() + REACH_HOLD_WITHIN;
        while !held.exists() {
            let launcher_ended = launcher.try_wait().unwrap().is_some();
            assert!(
                !launcher_ended && std::time::Instant::now() < deadline,
                "the launcher's supervisor never reached its hold (launcher ended: \
                 {launcher_ended})"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        launcher.kill().expect("SIGKILL the launcher");
        launcher.wait().expect("reap the launcher");

        assert!(
            agent_pid_from_file(root, phase).is_none_or(|pid| !agent::agent_running(pid)),
            "the agent pid file must not be able to cause the refusal"
        );
        let second = devflow_core::test_support::run_test_in_child(
            name,
            path_dir.path(),
            &[
                ("DEVFLOW_CACHE_DIR", cache.path().as_os_str()),
                (RESERVE_ROOT_ENV, root.as_os_str()),
                (RESERVE_CASE_ENV, std::ffi::OsStr::new("second")),
            ],
        );
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&second, name);
        assert!(
            supervisor.end(),
            "the held supervisor stops through its socket once it publishes"
        );
    }

    /// The second launcher's half of the V-01 fixture.
    fn assert_second_launch_is_refused(root: &Path, phase: PhaseId, verb: LaunchVerb) {
        let state_file = workflow::state_path(root, phase);
        let before = std::fs::read(&state_file).unwrap();
        let live_before = liveness_files(root);
        let pending = workflow::load_state(root, phase)
            .unwrap()
            .pending_supervisor
            .expect("the dead launcher's reservation names its supervisor");
        assert_eq!(
            liveness::probe(&pending.liveness_path).unwrap(),
            liveness::Held::Live,
            "the supervisor kept the lock after its launcher died"
        );

        let result = launch_for_test(root, phase, verb);
        // A launch that wrongly proceeded is ended before the assertions below fail.
        let after = workflow::load_state(root, phase).ok();
        let _reap = after
            .as_ref()
            .map(crate::test_support::ReapMonitorOnDrop::after_launch);

        let text = result.expect_err("a second launch must refuse a live pending supervisor");
        for expected in [
            format!("devflow stop --phase {phase}"),
            pending.launch_id.clone(),
            PENDING_NO_SOCKET.to_string(),
        ] {
            assert!(text.contains(&expected), "missing {expected:?}: {text}");
        }
        assert!(
            !text.contains("devflow resume"),
            "no resume hint while its lock is held: {text}"
        );
        assert!(
            !text.contains("agent pid"),
            "the refusal must not come from the agent pid file: {text}"
        );
        assert_eq!(
            std::fs::read(&state_file).unwrap(),
            before,
            "a refused launch writes no state"
        );
        assert_eq!(
            liveness_files(root),
            live_before,
            "a refused launch leaves no liveness file of its own"
        );
    }

    #[test]
    fn start_refuses_over_a_pending_supervisor_whose_launcher_died() {
        second_launch_refuses_over_a_dead_launchers_supervisor(
            "commands::tests::start_refuses_over_a_pending_supervisor_whose_launcher_died",
            PhaseId::new(5018),
            LaunchVerb::Start,
        );
    }

    #[test]
    fn resume_refuses_over_a_pending_supervisor_whose_launcher_died() {
        second_launch_refuses_over_a_dead_launchers_supervisor(
            "commands::tests::resume_refuses_over_a_pending_supervisor_whose_launcher_died",
            PhaseId::new(5019),
            LaunchVerb::Resume,
        );
    }

    /// Criterion 2: while a pending supervisor is live, a refused `resume` never
    /// suggests `devflow resume`, even when an agent is live beside it. The control is
    /// the same refusal over a recorded supervisor, which does name it.
    #[test]
    fn a_resume_refused_over_a_pending_supervisor_never_names_resume() {
        let phase = PhaseId::new(5022);
        let live = |pending| LiveSocket {
            socket: PathBuf::from("/s.sock"),
            launch_id: "aaaa0000aaaa5022".to_string(),
            liveness: PENDING_NO_SOCKET.to_string(),
            sidecar_problem: None,
            pending,
        };
        for pending in [PendingKind::Published, PendingKind::Reserved] {
            let text = live_run_refusal(phase, LaunchVerb::Resume, &[live(pending)], Some(77));
            assert!(!text.contains("devflow resume"), "{text}");
            assert!(text.contains("agent pid 77 alive"), "{text}");
            assert!(text.contains("`devflow status` names"), "{text}");
        }
        let text = live_run_refusal(phase, LaunchVerb::Resume, &[live(PendingKind::No)], None);
        assert!(text.contains("devflow resume"), "control: {text}");
    }

    /// How long the second launcher gets to reach the guard if the write lock lets it.
    const SECOND_LAUNCHER_GRACE: std::time::Duration = std::time::Duration::from_millis(300);

    /// V-05: the guard and the pending write are one step. Two launchers held between
    /// them: under the write lock the second never reaches the hold and is refused once
    /// the first has written. The control skips the lock and both reserve, which is
    /// what shows the lock, not the hold's timing, is what excludes the second.
    #[test]
    fn two_launchers_cannot_both_reserve() {
        const NAME: &str = "commands::tests::two_launchers_cannot_both_reserve";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::agent_free_git_only_path_dir();
            for case in ["lock", "control"] {
                let hold = tempfile::tempdir().unwrap();
                let spec = launch_hold_spec("guarded", hold.path());
                let mut env = vec![
                    (LAUNCH_HOLD_ENV, spec.as_os_str()),
                    (RESERVE_CASE_ENV, std::ffi::OsStr::new(case)),
                ];
                if case == "control" {
                    env.push(("DEVFLOW_TEST_SKIP_WRITE_LOCK", std::ffi::OsStr::new("1")));
                }
                let output =
                    crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &env);
                devflow_core::test_support::assert_child_ran_exactly_one_passing_test(
                    &output, NAME,
                );
            }
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5023);
        let control = std::env::var(RESERVE_CASE_ENV).unwrap() == "control";
        let release = launch_hold_release();
        let held = held_marker(&release);
        let arrivals = || {
            std::fs::read_to_string(&held)
                .map(|text| text.lines().count())
                .unwrap_or(0)
        };
        let fresh = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        // A saved state, so neither launcher meets the stateless check for a lock no
        // state records, which would refuse the control's second launcher too: the lock
        // must be the only thing that can tell the two cases apart.
        workflow::save_state(&fresh).unwrap();
        let cache = std::env::var_os("DEVFLOW_CACHE_DIR").map(PathBuf::from);
        let reserve = || {
            reserve_launch(
                root,
                phase,
                LaunchVerb::Start,
                Some(&fresh),
                cache.as_deref(),
            )
            .map_err(|err| err.to_string())
        };

        let (first, second, arrived) = std::thread::scope(|scope| {
            let first = scope.spawn(reserve);
            assert!(
                reached_hold(&release, &first),
                "the first launcher reached its hold"
            );
            let second = scope.spawn(reserve);
            let deadline = std::time::Instant::now() + REACH_HOLD_WITHIN;
            while control && arrivals() < 2 && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if !control {
                std::thread::sleep(SECOND_LAUNCHER_GRACE);
            }
            let arrived = arrivals();
            std::fs::write(&release, "").unwrap();
            // The first keeps its reservation, and so its lock, until the second has
            // answered: joining hands back the value without dropping it first.
            let second = second.join().unwrap();
            (first.join().unwrap(), second, arrived)
        });

        println!(
            "V-05 {}: {arrived} at the hold, first ok: {}, second ok: {}",
            std::env::var(RESERVE_CASE_ENV).unwrap(),
            first.is_ok(),
            second.is_ok()
        );
        if control {
            assert_eq!(arrived, 2, "with no lock, both launchers pass the guard");
            assert!(
                first.is_ok() && second.is_ok(),
                "with no lock, both reserve: {:?} {:?}",
                first.as_ref().map(|r| r.reservation.launch_id()),
                second.as_ref().map(|r| r.reservation.launch_id()),
            );
        } else {
            assert_eq!(arrived, 1, "the second launcher is held at the write lock");
            let (won, refusal) = match (&first, &second) {
                (Ok(won), Err(refusal)) | (Err(refusal), Ok(won)) => (won, refusal),
                _ => panic!(
                    "exactly one launcher reserves: {:?} {:?}",
                    first.as_ref().map(|r| r.reservation.launch_id()),
                    second.as_ref().map(|r| r.reservation.launch_id()),
                ),
            };
            assert!(
                refusal.contains(&format!("devflow stop --phase {phase}")),
                "{refusal}"
            );
            assert_eq!(
                workflow::load_state(root, phase)
                    .unwrap()
                    .pending_supervisor
                    .map(|pending| pending.launch_id),
                Some(won.reservation.launch_id().to_string()),
                "the state names the launcher that reserved"
            );
            assert_eq!(
                liveness_files(root).len(),
                1,
                "the refused one left no file"
            );
        }
        for reserved in [first, second].into_iter().flatten() {
            reserved.reservation.discard();
        }
    }

    /// V-03: a reservation whose lock nothing holds, or whose file is gone, is
    /// abandoned. It blocks nothing, and the next launch replaces it and says so.
    #[test]
    fn an_abandoned_reservation_does_not_block_start() {
        const NAME: &str = "commands::tests::an_abandoned_reservation_does_not_block_start";
        const ABANDONED: &str = "aaaa0000aaaa5020";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::agent_free_git_only_path_dir();
            for case in ["unheld", "missing"] {
                let output = crate::test_support::run_hermetic_test_in_child(
                    NAME,
                    path_dir.path(),
                    &[
                        (RESERVE_CASE_ENV, std::ffi::OsStr::new(case)),
                        // libtest would otherwise swallow the line this test is about.
                        ("RUST_TEST_NOCAPTURE", std::ffi::OsStr::new("1")),
                    ],
                );
                let stdout = String::from_utf8_lossy(&output.stdout);
                assert!(
                    output.status.success() && stdout.contains("1 passed"),
                    "{case}: {stdout}\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let line = format!("cleared an abandoned launch reservation {ABANDONED}");
                assert!(stdout.contains(&line), "{case}: {stdout}");
            }
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5020);
        let liveness_path = liveness::launch_liveness_path(root, ABANDONED).unwrap();
        std::fs::create_dir_all(liveness_path.parent().unwrap()).unwrap();
        let unheld = std::env::var(RESERVE_CASE_ENV).unwrap() == "unheld";
        if unheld {
            drop(liveness::Hold::take(&liveness_path).unwrap());
        }
        let mut abandoned = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        abandoned.stage = Stage::Code;
        abandoned.pending_supervisor = Some(devflow_core::state::PendingSupervisor::new(
            ABANDONED.to_string(),
            root.join("never-published.sock"),
            liveness_path.clone(),
        ));
        workflow::save_state(&abandoned).unwrap();
        let fresh = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());

        let cache = std::env::var_os("DEVFLOW_CACHE_DIR").map(PathBuf::from);
        let reserved = reserve_launch(
            root,
            phase,
            LaunchVerb::Start,
            Some(&fresh),
            cache.as_deref(),
        )
        .unwrap_or_else(|err| panic!("an abandoned reservation blocks nothing: {err}"));

        let on_disk = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            on_disk
                .pending_supervisor
                .as_ref()
                .map(|p| p.launch_id.as_str()),
            Some(reserved.reservation.launch_id()),
            "the new reservation replaces the abandoned one"
        );
        assert_eq!(
            on_disk.stage,
            Stage::Code,
            "the earlier run's state is kept"
        );
        assert!(
            !liveness_path.exists(),
            "the abandoned liveness file is gone"
        );
        reserved.reservation.discard();
    }

    /// V-04: a live agent named by the pid file still refuses, with the agent
    /// guidance. It is the control for V-01's fixture, which rules that cause out.
    #[test]
    fn a_live_agent_pid_still_refuses_start() {
        const NAME: &str = "commands::tests::a_live_agent_pid_still_refuses_start";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::go_waiting_claude_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5021);
        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        workflow::save_state(&state).unwrap();
        let before = std::fs::read(workflow::state_path(root, phase)).unwrap();
        let mut agent_process = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn the live agent stand-in");
        let pid = agent_process.id();
        std::fs::write(
            agent_result::agent_pid_path(root, phase),
            format!("{pid}\n"),
        )
        .unwrap();

        let result = start_for_test(root, phase);
        let _ = agent_process.kill();
        let _ = agent_process.wait();

        let text = result.expect_err("a live agent refuses the start");
        for expected in [
            format!("agent pid {pid} alive"),
            "The agent pid file names a live process".to_string(),
        ] {
            assert!(text.contains(&expected), "missing {expected:?}: {text}");
        }
        assert_eq!(
            std::fs::read(workflow::state_path(root, phase)).unwrap(),
            before,
            "a refused start writes no state"
        );
    }

    /// `start` writes a fresh run's state only while holding the write lock: with
    /// another writer holding it, the create returns after that writer is done.
    #[test]
    fn start_creates_its_state_under_the_write_lock() {
        const HOLDER_RELEASES: &str = "holder releases";
        const CREATE_RETURNS: &str = "create returns";
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4902);
        let launch_id = "aaaa0000aaaa4902";
        let mut reserved = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        reserved.pending_supervisor = Some(devflow_core::state::PendingSupervisor::new(
            launch_id.to_string(),
            root.join("reserved.sock"),
            root.join("reserved.live"),
        ));
        workflow::save_state(&reserved).unwrap();
        let mut fresh = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        let log = std::sync::Mutex::new(Vec::new());

        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let log = &log;
            let fresh = &mut fresh;
            let holder = scope.spawn(move || {
                workflow::update_state_with(root, phase, |_existing| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    log.lock().unwrap().push(HOLDER_RELEASES);
                    Ok::<_, workflow::WorkflowError>((None, ()))
                })
                .unwrap();
            });
            entered_rx.recv_timeout(HOLDER_ENTERS_WITHIN).unwrap();
            let creator = scope.spawn(move || {
                create_run_state(root, phase, fresh, launch_id).unwrap();
                log.lock().unwrap().push(CREATE_RETURNS);
            });
            std::thread::sleep(BLOCKED_WRITER_GRACE);
            release_tx.send(()).unwrap();
            holder.join().unwrap();
            creator.join().unwrap();
        });

        assert_eq!(
            log.into_inner().unwrap(),
            vec![HOLDER_RELEASES, CREATE_RETURNS]
        );
        assert_eq!(
            serde_json::to_value(workflow::load_state(root, phase).unwrap()).unwrap(),
            serde_json::to_value(&fresh).unwrap(),
            "the file holds the fresh state"
        );
        assert_eq!(fresh.mode, Mode::Supervise, "the fresh state replaced it");
        assert_eq!(
            fresh.pending_supervisor, reserved.pending_supervisor,
            "the fresh state keeps the reservation"
        );
    }

    /// The reservation was cleared while `start` prepared the run: the create refuses
    /// and writes nothing, for a state naming another launch and for no state at all.
    #[test]
    fn a_fresh_run_is_refused_once_its_reservation_was_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4903);
        let mut fresh = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());

        let err = create_run_state(root, phase, &mut fresh, "aaaa0000aaaa4903").unwrap_err();
        assert!(err.to_string().contains("was cleared"), "{err}");
        assert!(!workflow::state_path(root, phase).exists());

        let mut other = fresh.clone();
        other.pending_supervisor = Some(devflow_core::state::PendingSupervisor::new(
            "bbbb0000bbbb4903".to_string(),
            root.join("other.sock"),
            root.join("other.live"),
        ));
        workflow::save_state(&other).unwrap();
        let before = std::fs::read(workflow::state_path(root, phase)).unwrap();
        let err = create_run_state(root, phase, &mut fresh, "aaaa0000aaaa4903").unwrap_err();
        assert!(err.to_string().contains("was cleared"), "{err}");
        assert_eq!(
            std::fs::read(workflow::state_path(root, phase)).unwrap(),
            before
        );
    }

    // ---- 50.1-18: status, doctor and stop see the pending supervisor (D-02) ----

    const PENDING_LAUNCH: &str = "bbbb0000bbbb5018";

    /// A saved, not-stopped state naming `PENDING_LAUNCH` as pending, its socket never
    /// published and its liveness file not yet created. With `recorded`, it also names
    /// a supervisor whose socket does not exist (GONE).
    fn state_with_pending(root: &Path, phase: PhaseId, recorded: bool) -> State {
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        if recorded {
            state.supervisor = Some(SupervisorRecord::new(
                root.join("gone.sock"),
                "aaaa0000aaaa5018".to_string(),
                1,
                0,
                "boot".to_string(),
            ));
        }
        let liveness_path = liveness::launch_liveness_path(root, PENDING_LAUNCH).unwrap();
        std::fs::create_dir_all(liveness_path.parent().unwrap()).unwrap();
        state.pending_supervisor = Some(PendingSupervisor::new(
            PENDING_LAUNCH.to_string(),
            root.join("never-published.sock"),
            liveness_path,
        ));
        workflow::save_state(&state).unwrap();
        state
    }

    /// Take the lock a live pending supervisor would hold.
    fn hold_pending(state: &State) -> liveness::Hold {
        let pending = state.pending_supervisor.as_ref().unwrap();
        liveness::Hold::take(&pending.liveness_path).unwrap()
    }

    /// The lines `status` prints about `state`'s supervisor and its repair.
    fn rendered_status(state: &State) -> String {
        let phase_liveness = probe_phase(state);
        let root = &state.project_root;
        let mut lines = status_supervisor_lines(root, state, &phase_liveness);
        lines.extend(status_hint_lines(root, state, &phase_liveness));
        lines.join("\n")
    }

    /// V-07 (criterion 2): while the pending supervisor's lock is held, `status` names
    /// `stop` and never `resume`, whether the state records a GONE supervisor or none.
    #[test]
    fn status_names_stop_not_resume_for_a_live_pending_supervisor() {
        for (case, recorded) in [("a GONE record", true), ("no record", false)] {
            let dir = tempfile::tempdir().unwrap();
            let phase = PhaseId::new(5180);
            let state = state_with_pending(dir.path(), phase, recorded);
            let _hold = hold_pending(&state);

            let phase_liveness = probe_phase(&state);
            assert_eq!(
                phase_liveness,
                PhaseLiveness::LaunchPending {
                    launch_id: PENDING_LAUNCH.to_string(),
                    published: false,
                },
                "{case}"
            );
            assert_eq!(phase_liveness.label(), "launch_pending", "{case}");
            assert_eq!(
                recovery_hints(&state, &phase_liveness),
                vec![format!("devflow stop --phase {phase}")],
                "{case}"
            );
            let rendered = rendered_status(&state);
            assert!(
                rendered.contains(&format!("devflow stop --phase {phase}")),
                "{case}: {rendered}"
            );
            assert!(rendered.contains(PENDING_LAUNCH), "{case}: {rendered}");
            assert!(!rendered.contains("devflow resume"), "{case}: {rendered}");
        }
    }

    /// V-08, criterion 2's negative control: the same state once the pending
    /// supervisor is gone (its lock released) suggests `resume` again and notes the
    /// abandoned reservation.
    #[test]
    fn status_suggests_resume_once_the_pending_supervisor_is_gone() {
        for (case, recorded) in [("a GONE record", true), ("no record", false)] {
            let dir = tempfile::tempdir().unwrap();
            let phase = PhaseId::new(5181);
            let mut state = state_with_pending(dir.path(), phase, recorded);
            state.verification_baseline_captured = true;
            drop(hold_pending(&state));

            let phase_liveness = probe_phase(&state);
            assert_eq!(phase_liveness, PhaseLiveness::GoneWhileActive, "{case}");
            let resume = format!("devflow resume --phase {phase}");
            assert_eq!(
                recovery_hints(&state, &phase_liveness),
                vec![resume.clone()],
                "{case}"
            );
            let rendered = rendered_status(&state);
            assert!(rendered.contains(&resume), "{case}: {rendered}");
            assert!(rendered.contains(PENDING_LAUNCH), "{case}: {rendered}");
            assert!(rendered.contains("abandoned"), "{case}: {rendered}");
        }
    }

    /// V-09: `stop` over an abandoned reservation clears it, removes its liveness file
    /// and says what it cleared, whether the file is unheld or already gone.
    #[test]
    fn stop_clears_an_abandoned_reservation_and_says_so() {
        const NAME: &str = "commands::tests::stop_clears_an_abandoned_reservation_and_says_so";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::agent_free_git_only_path_dir();
            for case in ["unheld", "missing"] {
                let output = crate::test_support::run_hermetic_test_in_child(
                    NAME,
                    path_dir.path(),
                    &[
                        (RESERVE_CASE_ENV, std::ffi::OsStr::new(case)),
                        // libtest would otherwise swallow the line this test is about.
                        ("RUST_TEST_NOCAPTURE", std::ffi::OsStr::new("1")),
                    ],
                );
                let stdout = String::from_utf8_lossy(&output.stdout);
                assert!(
                    output.status.success() && stdout.contains("1 passed"),
                    "{case}: {stdout}\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let line = format!(
                    "stop: cleared the abandoned launch reservation {PENDING_LAUNCH} (no process \
                     held it)"
                );
                assert!(stdout.contains(&line), "{case}: {stdout}");
            }
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5182);
        let state = state_with_pending(root, phase, false);
        let liveness_path = state
            .pending_supervisor
            .as_ref()
            .unwrap()
            .liveness_path
            .clone();
        if std::env::var(RESERVE_CASE_ENV).unwrap() == "unheld" {
            drop(hold_pending(&state));
            assert!(
                liveness_path.exists(),
                "precondition: the unheld file is there"
            );
        }

        let cache = std::env::var_os("DEVFLOW_CACHE_DIR").map(PathBuf::from);
        stop_in(root, phase, cache.as_deref()).unwrap();

        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(after.pending_supervisor, None, "the reservation is cleared");
        assert!(after.stopped, "the phase is marked stopped as today");
        assert!(!liveness_path.exists(), "its liveness file is removed");
    }

    /// `stop` never claims a stop it did not perform: a pending lock held with no socket
    /// ever appearing ends it non-zero after the bounded wait, naming the launch, with
    /// nothing written and the lock's file left alone.
    #[test]
    fn stop_waits_for_an_unpublished_pending_supervisor_then_reports_honestly() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5183);
        let state = state_with_pending(root, phase, false);
        let _hold = hold_pending(&state);
        let liveness_path = state
            .pending_supervisor
            .as_ref()
            .unwrap()
            .liveness_path
            .clone();
        let before = std::fs::read(workflow::state_path(root, phase)).unwrap();
        let cache = tempfile::tempdir().unwrap();

        let started = std::time::Instant::now();
        let error = stop_in(root, phase, Some(cache.path()))
            .expect_err("stop must not claim a stop it did not perform")
            .to_string();

        assert!(
            started.elapsed() >= PENDING_PUBLISH_WAIT,
            "stop gives the supervisor the whole wait: {:?}",
            started.elapsed()
        );
        assert!(error.contains(PENDING_LAUNCH), "{error}");
        assert!(
            error.contains("nothing was signalled or recorded"),
            "{error}"
        );
        assert!(!error.contains("devflow resume"), "{error}");
        assert!(
            error.contains(&format!(
                "pid {} holds its liveness lock",
                std::process::id()
            )),
            "a repeat of stop cannot end it, so the holder is named: {error}"
        );
        assert_eq!(
            std::fs::read(workflow::state_path(root, phase)).unwrap(),
            before,
            "nothing is written"
        );
        assert!(liveness_path.exists(), "a held lock's file stays");
    }

    /// R-03: clearing an abandoned reservation gives up on a held write lock within the
    /// bound, naming the holder and clearing nothing.
    #[test]
    fn stop_gives_up_clearing_a_reservation_under_a_held_write_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(5184);
        let state = state_with_pending(&root, phase, false);
        drop(hold_pending(&state));
        let before = std::fs::read(workflow::state_path(&root, phase)).unwrap();
        let (holding, wait_holding) = std::sync::mpsc::channel();
        let (release, wait_release) = std::sync::mpsc::channel::<()>();
        let holder = {
            let root = root.clone();
            std::thread::spawn(move || {
                workflow::update_state(&root, phase, |_| {
                    holding.send(()).unwrap();
                    // Bounded, so a `stop` that blocks on the lock fails below, not hangs.
                    let _ = wait_release.recv_timeout(std::time::Duration::from_secs(20));
                    Ok::<(), workflow::WorkflowError>(())
                })
                .unwrap();
            })
        };
        wait_holding.recv().unwrap();
        let cache = tempfile::tempdir().unwrap();

        let started = std::time::Instant::now();
        let result = stop_in(&root, phase, Some(cache.path()));
        let elapsed = started.elapsed();
        drop(release);
        holder.join().unwrap();

        let error = result
            .expect_err("a held write lock must end stop")
            .to_string();
        assert!(error.contains(&std::process::id().to_string()), "{error}");
        assert!(
            elapsed < workflow::WRITE_LOCK_REPAIR_WAIT + std::time::Duration::from_secs(2),
            "{elapsed:?}"
        );
        assert_eq!(
            std::fs::read(workflow::state_path(&root, phase)).unwrap(),
            before,
            "nothing is cleared"
        );
    }

    /// A launcher still running can promote its launch while `stop` waits on it, which
    /// ends the pending record and not the supervisor. `stop` must look at the record
    /// again: here a STALE one, whose socket only that second look removes.
    #[test]
    fn stop_addresses_a_launch_promoted_during_its_wait() {
        const PROMOTED: &str = "cccc0000cccc5186";
        /// Above any pid, so the backstop proves the group gone and signals nothing.
        const NO_SUCH_PGID: u32 = 99_999_997;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(5186);
        let state = state_with_pending(&root, phase, false);
        let hold = hold_pending(&state);
        let socket = root.join("promoted.sock");
        drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
        let boot = supervisor::boot_id().expect("a readable boot id");
        let sidecar = serde_json::json!({
            "launch_id": PROMOTED,
            "project_root": root,
            "phase": phase,
            "stage": "define",
            "agent": "claude",
            "launch": "stream",
            "agent_pid": NO_SUCH_PGID,
            "agent_pgid": NO_SUCH_PGID,
            "agent_start_time": 1,
            "boot_id": boot,
            "supervisor_pid": 99_999_998,
            "supervisor_start_time": 1,
            "created_at": "2026-11-02T00:00:00Z",
        });
        std::fs::write(socket.with_extension("json"), sidecar.to_string()).unwrap();
        let promoter = {
            let (root, socket) = (root.clone(), socket.clone());
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                workflow::update_state(&root, phase, |state| {
                    state.pending_supervisor = None;
                    state.supervisor = Some(SupervisorRecord::new(
                        socket,
                        PROMOTED.to_string(),
                        NO_SUCH_PGID,
                        1,
                        boot,
                    ));
                    Ok::<(), workflow::WorkflowError>(())
                })
                .unwrap();
                hold
            })
        };
        let cache = tempfile::tempdir().unwrap();

        let result = stop_in(&root, phase, Some(cache.path()));
        drop(promoter.join().unwrap());

        result.unwrap();
        assert!(
            !socket.exists(),
            "the promoted launch's socket is addressed"
        );
        assert!(workflow::load_state(&root, phase).unwrap().stopped);
    }

    /// The one phase's facts `doctor` would gather for `state`.
    fn facts_for(root: &Path, state: &State) -> PhaseFacts {
        build_phase_facts(
            root,
            state.clone(),
            &mut std::collections::HashMap::new(),
            &[],
        )
    }

    /// `doctor` names a live pending launch and its repair, in text and `--json`, and
    /// never `resume`, including over a recorded STALE socket (whose own repair is
    /// `stop && resume`); released, the finding is the abandoned note.
    #[test]
    fn doctor_reports_a_pending_launch_and_names_stop() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5187);
        let stop_verb = format!("devflow stop --phase {phase}");
        let mut state = state_with_pending(root, phase, true);
        let hold = hold_pending(&state);

        let assert_names_stop_only = |state: &State, case: &str| {
            let facts = facts_for(root, state);
            let findings = reconcile_phase(&facts);
            let finding = findings
                .iter()
                .find(|finding| finding.detail.contains(PENDING_LAUNCH))
                .unwrap_or_else(|| panic!("{case}: no finding names the launch"));
            assert_eq!(
                finding.repair.as_deref(),
                Some(stop_verb.as_str()),
                "{case}"
            );
            for finding in &findings {
                let repair = finding.repair.as_deref().unwrap_or_default();
                assert!(!repair.contains("devflow resume"), "{case}: {repair}");
            }
            let body = render_reconciliation_json(std::slice::from_ref(&facts));
            let pending = &body[0]["supervisor"]["pending"];
            assert_eq!(pending["launch_id"], PENDING_LAUNCH, "{case}: {body}");
            assert_eq!(pending["liveness"], "launch_pending", "{case}: {body}");
        };
        assert_names_stop_only(&state, "a GONE record");

        let stale = root.join("stale.sock");
        drop(std::os::unix::net::UnixListener::bind(&stale).unwrap());
        state.supervisor.as_mut().unwrap().socket_path = stale;
        assert_names_stop_only(&state, "a STALE record");

        drop(hold);
        let facts = facts_for(root, &state);
        let findings = reconcile_phase(&facts);
        let note = findings
            .iter()
            .find(|finding| finding.detail.contains(PENDING_LAUNCH))
            .expect("the abandoned reservation is noted");
        assert!(note.detail.contains("abandoned"), "{}", note.detail);

        state.pending_supervisor = None;
        let body = render_reconciliation_json(std::slice::from_ref(&facts_for(root, &state)));
        assert!(body[0]["supervisor"]["pending"].is_null(), "{body}");
    }

    /// Ends and reaps the fixture's stand-in agent if an assertion fails first.
    struct StandInAgent(std::process::Child);

    impl StandInAgent {
        /// A `sleep` leading its own process group, as an agent does.
        fn spawn() -> Self {
            use std::os::unix::process::CommandExt as _;
            let child = std::process::Command::new("sleep")
                .arg("60")
                .process_group(0)
                .spawn()
                .expect("spawn the stand-in agent");
            Self(child)
        }

        fn pid(&self) -> u32 {
            self.0.id()
        }

        fn kill_and_reap(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    impl Drop for StandInAgent {
        fn drop(&mut self) {
            self.kill_and_reap();
        }
    }

    // The phase holder (50.1-29, D-17, D-18). A holder that `stop` signals runs as its own
    // process: `stop` would otherwise be signalling the test runner.

    /// Names a holder-test child's project root. Not a DevFlow setting, so it carries no
    /// `DEVFLOW_` prefix the doc check would require documented.
    const HOLDER_ROOT_ENV: &str = "P50_1_29_ROOT";

    /// The project root a holder-test child was given.
    fn holder_child_root() -> PathBuf {
        PathBuf::from(std::env::var_os(HOLDER_ROOT_ENV).expect("the parent names the root"))
    }

    /// A process of this test binary, on the test named `name`, parked at the launch hold
    /// point `holder_claimed`. It is killed and reaped when this drops, on a failed
    /// assertion too.
    struct ParkedHolder {
        child: Option<std::process::Child>,
        release: PathBuf,
        _dirs: (tempfile::TempDir, tempfile::TempDir),
    }

    impl ParkedHolder {
        /// Spawn it and wait until it reaches the hold. `path_dir` is the child's PATH.
        fn spawn(name: &str, root: &Path, path_dir: tempfile::TempDir) -> Self {
            let hold_dir = crate::test_support::hermetic_cache_dir();
            let release = hold_dir.path().join("release");
            let spec: std::ffi::OsString = format!("holder_claimed:{}", release.display()).into();
            let mut child = devflow_core::test_support::spawn_test_in_child(
                name,
                path_dir.path(),
                &[
                    ("DEVFLOW_CACHE_DIR", hold_dir.path().as_os_str()),
                    (LAUNCH_HOLD_ENV, spec.as_os_str()),
                    (HOLDER_ROOT_ENV, root.as_os_str()),
                    // libtest would otherwise swallow what a child prints.
                    ("RUST_TEST_NOCAPTURE", std::ffi::OsStr::new("1")),
                ],
            );
            let held = held_marker(&release);
            let deadline = std::time::Instant::now() + REACH_HOLD_WITHIN;
            while !held.exists() {
                if std::time::Instant::now() > deadline || child.try_wait().unwrap().is_some() {
                    let _ = child.kill();
                    panic!(
                        "the child never reached holder_claimed: {:?}",
                        child.wait_with_output()
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Self {
                child: Some(child),
                release,
                _dirs: (hold_dir, path_dir),
            }
        }

        fn pid(&self) -> u32 {
            self.child.as_ref().expect("not yet finished").id()
        }

        /// Release the hold and wait for the child to end, within `REACH_HOLD_WITHIN`.
        fn release_and_finish(&mut self) -> std::process::Output {
            std::fs::write(&self.release, "").unwrap();
            let mut child = self.child.take().expect("finished once");
            let deadline = std::time::Instant::now() + REACH_HOLD_WITHIN;
            while child.try_wait().unwrap().is_none() {
                if std::time::Instant::now() > deadline {
                    let _ = child.kill();
                    panic!(
                        "the child did not end after its release: {:?}",
                        child.wait_with_output()
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            child.wait_with_output().unwrap()
        }
    }

    impl Drop for ParkedHolder {
        fn drop(&mut self) {
            if let Some(child) = &mut self.child {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    /// A `sleep` child with its own start time, read once it is readable: a freshly forked
    /// child may not have exec'd yet (999.47).
    fn sleep_child_with_start() -> (std::process::Child, u64) {
        let child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();
        let mut start = None;
        for _ in 0..100 {
            start = agent::process_start_time(pid);
            if start.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        (child, start.expect("read the child's start time"))
    }

    /// Hold `phase`'s holder file in this process, with its record naming `pid`. The
    /// record is rewritten through a separate truncating open, never the hold's own
    /// descriptor, whose offset sits past the claim's record. Bind the hold to a named
    /// local: `_` would drop it at once.
    fn hold_phase_with_record(
        root: &Path,
        phase: PhaseId,
        pid: u32,
        start: Option<u64>,
    ) -> liveness::Hold {
        let hold = liveness::claim_phase_holder(root, phase).unwrap();
        let record = match start {
            Some(start) => format!("{pid} {start}\n"),
            None => format!("{pid}\n"),
        };
        std::fs::write(liveness::phase_holder_path(root, phase), record).unwrap();
        hold
    }

    /// The `name` events of `root`'s event log, parsed.
    fn holder_events_named(root: &Path, name: &str) -> Vec<serde_json::Value> {
        std::fs::read_to_string(events::events_path(root))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|event| event["event"] == name)
            .collect()
    }

    /// D-17 (V-29), D-18: `stop` reaches a hand-run `advance` through its holder after the
    /// identity check. The holder catches the signal, so it is still running when `stop`
    /// gives up and marks nothing; released, it stops at safe point 1 and records it.
    #[test]
    fn stop_signals_a_hand_run_advance_through_its_phase_holder() {
        use std::os::unix::process::ExitStatusExt;
        const NAME: &str =
            "commands::tests::stop_signals_a_hand_run_advance_through_its_phase_holder";
        let phase = PhaseId::new(2902);
        if devflow_core::test_support::in_child_test(NAME) {
            crate::pipeline_launch::advance_command(
                &holder_child_root(),
                Some(phase),
                Some(Stage::Define),
            )
            .unwrap();
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.verification_baseline_captured = true;
        workflow::save_state(&state).unwrap();
        let mut child = ParkedHolder::spawn(NAME, root, tempfile::tempdir().unwrap());
        let pid = child.pid();
        let holder_path = liveness::phase_holder_path(root, phase);
        assert_eq!(
            liveness::live_holder(&holder_path).unwrap(),
            Some((pid, agent::process_start_time(pid))),
            "the holder names the child"
        );
        let before = std::fs::read(workflow::state_path(root, phase)).unwrap();

        let error = stop_with_an_empty_cache(root, phase)
            .expect_err("a holder still running is not a stop")
            .to_string();
        eprintln!("stop: {error}");

        assert!(error.contains(&format!("signalled pid {pid}")), "{error}");
        assert!(error.contains("foreground holder"), "{error}");
        assert!(error.contains("not marked"), "{error}");
        assert!(error.contains("next safe point"), "{error}");
        assert_eq!(
            std::fs::read(workflow::state_path(root, phase)).unwrap(),
            before,
            "`stop` writes no state"
        );
        assert!(
            devflow_core::test_support::process_is_live(pid),
            "the signal was caught, not fatal"
        );

        let output = child.release_and_finish();
        assert_eq!(output.status.signal(), None, "{output:?}");
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
        let state = workflow::load_state(root, phase).unwrap();
        assert_eq!(state.stage, Stage::Define, "the stage does not move");
        assert!(state.stopped, "the holder records the stop");
        assert_eq!(
            state.stop_reason.as_deref(),
            Some(crate::pipeline_launch::HOLDER_STOP_REASON)
        );
        let stopped = holder_events_named(root, "advance_stopped");
        assert_eq!(stopped.len(), 1, "{stopped:?}");
        assert_eq!(stopped[0]["safe_point"], 1, "{}", stopped[0]);
        assert_eq!(stopped[0]["source"], "foreground_holder", "{}", stopped[0]);
    }

    /// D-17 (V-31): `start` refuses while another process holds the phase, before it
    /// reserves. The control is the same fixture through `reserve_launch` alone: the
    /// reservation write the byte comparison would see from a `start` that claimed late.
    #[test]
    fn start_refuses_while_another_process_holds_the_phase_before_reserving() {
        const NAME: &str =
            "commands::tests::start_refuses_while_another_process_holds_the_phase_before_reserving";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::agent_free_dir_with_agent_stub("claude");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let assert_refused = |error: &str, phase: PhaseId| {
            assert!(
                error.contains(&format!("(pid {})", std::process::id())),
                "the refusal names the holder's pid: {error}"
            );
            assert!(
                error.contains(&format!("devflow stop --phase {phase}")),
                "{error}"
            );
            assert!(!error.contains("devflow resume"), "{error}");
        };

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(2907);
        init_start_repo(root, phase);
        let mut dead = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        dead.stage = Stage::Plan;
        workflow::save_state(&dead).unwrap();
        let _hold = liveness::claim_phase_holder(root, phase).unwrap();
        let state_file = workflow::state_path(root, phase);
        let before = std::fs::read(&state_file).unwrap();

        let error = start_for_test(root, phase).expect_err("a held phase refuses");

        assert_refused(&error, phase);
        assert_eq!(
            std::fs::read(&state_file).unwrap(),
            before,
            "byte-identical"
        );
        assert_eq!(
            workflow::load_state(root, phase)
                .unwrap()
                .pending_supervisor,
            None
        );
        assert!(liveness_files(root).is_empty(), "no liveness file is made");

        let no_state = tempfile::tempdir().unwrap();
        let empty = no_state.path();
        init_start_repo(empty, phase);
        let _empty_hold = liveness::claim_phase_holder(empty, phase).unwrap();
        let error = start_for_test(empty, phase).expect_err("a held phase refuses");
        assert_refused(&error, phase);
        assert!(
            !workflow::state_path(empty, phase).exists(),
            "with no state, none is created"
        );

        let cache = std::env::var_os("DEVFLOW_CACHE_DIR").map(PathBuf::from);
        let reserved = reserve_launch(
            root,
            phase,
            LaunchVerb::Start,
            Some(&dead),
            cache.as_deref(),
        )
        .unwrap_or_else(|err| panic!("the control reserves: {err}"));
        assert_ne!(
            std::fs::read(&state_file).unwrap(),
            before,
            "control: a reservation changes the file, so the comparison would see one"
        );
        assert!(
            workflow::load_state(root, phase)
                .unwrap()
                .pending_supervisor
                .is_some(),
            "control: the reservation is written while the holder is held"
        );
        let created = reserved.created;
        crate::pipeline_launch::hold_reservation(reserved.reservation);
        crate::pipeline_launch::abandon_reservation(root, phase, created);
    }

    /// D-17 (V-29), D-02, D-18: a `start` busy before its spawn is reached once the bounded
    /// wait for a socket expires. The launcher catches the signal, so `stop` marks nothing;
    /// released, it stops at safe point 2 and gives its reservation back.
    #[test]
    fn stop_signals_a_start_that_has_not_spawned_its_supervisor() {
        use std::os::unix::process::ExitStatusExt;
        const NAME: &str =
            "commands::tests::stop_signals_a_start_that_has_not_spawned_its_supervisor";
        let phase = PhaseId::new(2911);
        if devflow_core::test_support::in_child_test(NAME) {
            start_for_test(&holder_child_root(), phase).unwrap();
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_start_repo(root, phase);
        let mut child = ParkedHolder::spawn(
            NAME,
            root,
            crate::test_support::agent_free_dir_with_agent_stub("claude"),
        );
        let pid = child.pid();
        let pending = workflow::load_state(root, phase)
            .unwrap()
            .pending_supervisor
            .expect("its reservation is pending");
        assert_eq!(
            liveness::probe(&pending.liveness_path).unwrap(),
            liveness::Held::Live,
            "the launcher holds its own reservation"
        );
        let before = std::fs::read(workflow::state_path(root, phase)).unwrap();

        let error = stop_with_an_empty_cache(root, phase)
            .expect_err("a launcher still running is not a stop")
            .to_string();
        eprintln!("stop: {error}");

        assert!(error.contains(&format!("signalled pid {pid}")), "{error}");
        assert!(error.contains("not marked"), "{error}");
        assert!(
            error.contains(&format!(
                "gives its reservation for launch {} back itself at its next safe point",
                pending.launch_id
            )),
            "{error}"
        );
        assert!(!error.contains("devflow resume"), "{error}");
        assert_eq!(
            std::fs::read(workflow::state_path(root, phase)).unwrap(),
            before,
            "`stop` writes no state"
        );
        assert!(
            devflow_core::test_support::process_is_live(pid),
            "the signal was caught, not fatal"
        );

        let output = child.release_and_finish();
        assert_eq!(output.status.signal(), None, "{output:?}");
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
        let printed = String::from_utf8_lossy(&output.stdout);
        assert!(!printed.contains("monitor will auto-advance"), "{printed}");
        assert!(
            printed.contains(&format!(
                "phase {phase} stopped before anything was launched"
            )),
            "{printed}"
        );
        assert!(
            printed.contains(&format!("devflow resume --phase {phase}")),
            "{printed}"
        );
        let state = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            state.pending_supervisor, None,
            "the reservation is given back"
        );
        assert!(!pending.liveness_path.exists(), "and its liveness file");
        assert_eq!(state.supervisor, None, "no supervisor was spawned");
        assert!(state.stopped, "the launcher records the stop");
        assert_eq!(
            state.stop_reason.as_deref(),
            Some(crate::pipeline_launch::HOLDER_STOP_REASON)
        );
        let stopped = holder_events_named(root, "advance_stopped");
        assert_eq!(stopped.len(), 1, "{stopped:?}");
        assert_eq!(stopped[0]["safe_point"], 2, "{}", stopped[0]);
        assert_eq!(stopped[0]["source"], "foreground_holder", "{}", stopped[0]);
        assert!(
            holder_events_named(root, "stage_launched").is_empty(),
            "nothing was launched"
        );
    }

    /// A foreground `start` parked at a preflight gate and then signalled records a stop at
    /// safe point 4, and that stop survives its own launch error, so `resume` has a state to
    /// continue. The preflight fails because an Auto run needs a GSD config this repo lacks.
    #[test]
    fn a_signal_to_a_start_parked_at_a_preflight_gate_keeps_the_resumable_state() {
        use std::os::unix::process::ExitStatusExt;
        const NAME: &str = "commands::tests::\
                            a_signal_to_a_start_parked_at_a_preflight_gate_keeps_the_resumable_state";
        let phase = PhaseId::new(5309);
        if devflow_core::test_support::in_child_test(NAME) {
            let err = start(
                &holder_child_root(),
                phase,
                AgentKind::Claude,
                Mode::Auto,
                false,
                false,
                false,
                None,
                false,
                true,
                FallbackSource::FlagOrConfig(None),
            )
            .expect_err("a stop at a safe point during the gate wait is an error")
            .to_string();
            assert!(err.contains("gate wait"), "{err}");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_start_repo(root, phase);
        let path_dir = crate::test_support::agent_free_dir_with_agent_stub("claude");
        let cache = crate::test_support::hermetic_cache_dir();
        let mut child = devflow_core::test_support::spawn_test_in_child(
            NAME,
            path_dir.path(),
            &[
                ("DEVFLOW_CACHE_DIR", cache.path().as_os_str()),
                (HOLDER_ROOT_ENV, root.as_os_str()),
            ],
        );
        let gate = Gates::gate_path(root, phase, Stage::Define);
        let deadline = std::time::Instant::now() + REACH_HOLD_WITHIN;
        while !gate.exists() {
            if std::time::Instant::now() > deadline || child.try_wait().unwrap().is_some() {
                let _ = child.kill();
                panic!(
                    "the start never parked at its gate: {:?}",
                    child.wait_with_output()
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // SAFETY: `kill` on the pid of a child this test has not reaped.
        assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
        let deadline = std::time::Instant::now() + REACH_HOLD_WITHIN;
        while child.try_wait().unwrap().is_none() {
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                panic!(
                    "the signalled start did not end: {:?}",
                    child.wait_with_output()
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.signal(), None, "{output:?}");
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);

        let stopped = holder_events_named(root, "advance_stopped");
        assert_eq!(stopped.len(), 1, "{stopped:?}");
        assert_eq!(stopped[0]["safe_point"], 4, "{}", stopped[0]);
        let state = workflow::load_state(root, phase).expect("the stop keeps the state");
        assert!(state.stopped);
        assert_eq!(state.supervisor, None);
        assert_eq!(state.pending_supervisor, None);
    }

    /// A signalled launcher's child half: hold the phase, and with `pending` the launch's
    /// liveness lock too, with no D-18 handler, so the signal ends this process.
    fn hold_as_a_launcher_child(phase: PhaseId, pending: bool) {
        let root = holder_child_root();
        let _holder = liveness::claim_phase_holder(&root, phase).unwrap();
        let _pending = pending.then(|| {
            liveness::Hold::take(&liveness::launch_liveness_path(&root, PENDING_LAUNCH).unwrap())
                .unwrap()
        });
        std::thread::sleep(std::time::Duration::from_secs(60));
        panic!("the launcher child was never signalled");
    }

    /// This test binary re-run on `name`, once it holds the phase and `liveness_path` reads
    /// live, whoever holds that.
    fn spawn_launcher_child(
        name: &str,
        root: &Path,
        phase: PhaseId,
        liveness_path: &Path,
    ) -> std::process::Child {
        let path_dir = tempfile::tempdir().unwrap();
        let mut child = devflow_core::test_support::spawn_test_in_child(
            name,
            path_dir.path(),
            &[(HOLDER_ROOT_ENV, root.as_os_str())],
        );
        let holder_path = liveness::phase_holder_path(root, phase);
        let deadline = std::time::Instant::now() + REACH_HOLD_WITHIN;
        while !matches!(liveness::live_holder(&holder_path), Ok(Some((pid, _))) if pid == child.id())
            || !matches!(liveness::probe(liveness_path), Ok(liveness::Held::Live))
        {
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                panic!(
                    "the launcher child never got ready: {:?}",
                    child.wait_with_output()
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        child
    }

    /// D-02, D-18: a signalled launcher that exits within `stop`'s wait leaves its
    /// reservation abandoned, so `stop` clears it and records the stop itself.
    #[test]
    fn stop_records_the_stop_when_a_signalled_launcher_exits_within_the_wait() {
        use std::os::unix::process::ExitStatusExt;
        const NAME: &str = "commands::tests::stop_records_the_stop_when_a_signalled_launcher_exits_within_the_wait";
        let phase = PhaseId::new(2912);
        if devflow_core::test_support::in_child_test(NAME) {
            hold_as_a_launcher_child(phase, true);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let state = state_with_pending(root, phase, false);
        let liveness_path = state
            .pending_supervisor
            .as_ref()
            .unwrap()
            .liveness_path
            .clone();
        let mut child = spawn_launcher_child(NAME, root, phase, &liveness_path);

        let result = stop_with_an_empty_cache(root, phase);
        let _ = child.kill();
        let status = child.wait().unwrap();

        result.unwrap_or_else(|err| panic!("the launcher exited, so this is a stop: {err}"));
        assert_eq!(
            status.signal(),
            Some(libc::SIGTERM),
            "the launcher was signalled"
        );
        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(after.pending_supervisor, None, "its reservation is cleared");
        assert!(!liveness_path.exists(), "and its liveness file removed");
        assert!(after.stopped, "the stop is recorded");
        assert!(
            after
                .stop_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("devflow stop")),
            "by `stop`: {:?}",
            after.stop_reason
        );
    }

    /// D-02: if the launch's lock outlives the signalled launcher, a supervisor it spawned
    /// holds it, so `stop` claims no stop and writes nothing.
    #[test]
    fn stop_claims_no_stop_when_a_launch_outlives_its_signalled_launcher() {
        use std::os::unix::process::ExitStatusExt;
        const NAME: &str =
            "commands::tests::stop_claims_no_stop_when_a_launch_outlives_its_signalled_launcher";
        let phase = PhaseId::new(2913);
        if devflow_core::test_support::in_child_test(NAME) {
            hold_as_a_launcher_child(phase, false);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let state = state_with_pending(root, phase, false);
        let _hold = hold_pending(&state);
        let liveness_path = state
            .pending_supervisor
            .as_ref()
            .unwrap()
            .liveness_path
            .clone();
        let mut child = spawn_launcher_child(NAME, root, phase, &liveness_path);
        let before = std::fs::read(workflow::state_path(root, phase)).unwrap();

        let result = stop_with_an_empty_cache(root, phase);
        let _ = child.kill();
        let status = child.wait().unwrap();

        let error = result.expect_err("a live launch is no stop").to_string();
        assert_eq!(
            status.signal(),
            Some(libc::SIGTERM),
            "the launcher was signalled"
        );
        assert!(error.contains("still holds its liveness lock"), "{error}");
        assert!(error.contains(PENDING_LAUNCH), "{error}");
        assert!(!error.contains("devflow resume"), "{error}");
        assert_eq!(
            std::fs::read(workflow::state_path(root, phase)).unwrap(),
            before,
            "nothing is written"
        );
    }

    /// D-18: over a missing state, a signalled holder ended its run on its own, which is
    /// not "already stopped". With nothing signalled, the line is today's.
    #[test]
    fn a_holder_that_ended_its_run_after_the_signal_is_not_reported_as_stopped() {
        let phase = PhaseId::new(2903);
        let signalled = missing_state_line(phase, true);
        assert!(signalled.contains("ended on its own"), "{signalled}");
        assert!(
            signalled.contains("nothing was marked stopped"),
            "{signalled}"
        );
        assert!(!signalled.contains("already stopped"), "{signalled}");
        let not_signalled = missing_state_line(phase, false);
        assert_eq!(
            not_signalled,
            format!("stop: no persisted state for phase {phase} — already stopped"),
            "control: with no signal sent the line is today's"
        );
    }

    /// D-19: no supervisor is live and the agent still runs, so `status` and `doctor`
    /// name its pid and the manual kill, not a `resume` the launch guard would refuse.
    /// Control: with the agent dead, they suggest `resume` again. Nothing under test
    /// signals the agent: it is alive until the test ends it.
    #[test]
    fn status_and_doctor_name_an_orphaned_agent_not_resume() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5188);
        let mut agent = StandInAgent::spawn();
        let pid = agent.pid();
        let kill_line = format!("kill -TERM -- -{pid}");
        let resume = format!("devflow resume --phase {phase}");

        let mut confirmed = State::new(phase, AgentKind::Claude, Mode::Supervise, root.into());
        confirmed.verification_baseline_captured = true;
        confirmed.supervisor = Some(SupervisorRecord::new(
            root.join("gone.sock"),
            "aaaa0000aaaa5188".to_string(),
            pid,
            agent::process_start_time(pid).expect("the agent's start time"),
            supervisor::boot_id().expect("a readable boot id"),
        ));
        assert_eq!(probe_phase(&confirmed), PhaseLiveness::GoneWhileActive);
        let live = live_phase_agent(root, &confirmed);
        assert_eq!(
            live,
            Some(LiveAgent {
                pid,
                confirmed: true
            }),
            "confirmed"
        );
        let hints = status_hints(&confirmed, &probe_phase(&confirmed), live.as_ref());
        assert_eq!(hints.len(), 1, "{hints:?}");
        assert!(hints[0].contains(&format!("pid {pid}")), "{hints:?}");
        assert!(hints[0].contains(&kill_line), "{hints:?}");
        assert!(hints[0].contains("999.170"), "{hints:?}");
        assert!(!hints[0].contains("devflow resume"), "{hints:?}");
        let finding = check_supervisor(&facts_for(root, &confirmed)).expect("a finding");
        assert_eq!(finding.repair.as_ref(), Some(&hints[0]), "doctor's repair");

        let unrecorded = State::new(phase, AgentKind::Claude, Mode::Supervise, root.into());
        let pid_file = agent_result::agent_pid_path(root, phase);
        std::fs::create_dir_all(pid_file.parent().unwrap()).unwrap();
        std::fs::write(&pid_file, pid.to_string()).unwrap();
        assert_eq!(probe_phase(&unrecorded), PhaseLiveness::NotLaunched);
        let live = live_phase_agent(root, &unrecorded);
        assert_eq!(
            live,
            Some(LiveAgent {
                pid,
                confirmed: false
            }),
            "unconfirmed"
        );
        let hints = status_hints(&unrecorded, &probe_phase(&unrecorded), live.as_ref());
        assert_eq!(hints.len(), 1, "{hints:?}");
        assert!(
            hints[0].contains(&format!("ps -o pid,pgid,lstart,args -p {pid}")),
            "{hints:?}"
        );
        assert!(hints[0].contains(&kill_line), "{hints:?}");
        assert!(!hints[0].contains("devflow resume"), "{hints:?}");
        let finding = check_supervisor(&facts_for(root, &unrecorded)).expect("a finding");
        assert_eq!(finding.severity, Severity::Problem);
        assert!(
            finding.detail.contains(&format!("pid {pid}")),
            "{}",
            finding.detail
        );
        assert_eq!(finding.repair.as_ref(), Some(&hints[0]), "doctor's repair");

        assert!(agent::agent_running(pid), "nothing under test signalled it");
        agent.kill_and_reap();
        assert_eq!(live_phase_agent(root, &confirmed), None, "control: dead");
        assert_eq!(live_phase_agent(root, &unrecorded), None, "control: dead");
        assert_eq!(
            status_hints(&confirmed, &probe_phase(&confirmed), None),
            vec![resume.clone()],
            "control"
        );
        let finding = check_supervisor(&facts_for(root, &confirmed)).expect("a finding");
        assert_eq!(finding.repair.as_ref(), Some(&resume), "control");
    }

    /// R-03: `status` and `doctor` read the write lock without waiting and name a held
    /// one, in wording that tells the operator how to judge a single reading. Control:
    /// once it is released, neither says anything.
    #[test]
    fn status_and_doctor_name_a_held_state_write_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(5189);
        let state = State::new(phase, AgentKind::Claude, Mode::Supervise, root.clone());
        workflow::save_state(&state).unwrap();
        let (holding, wait_holding) = std::sync::mpsc::channel();
        let (release, wait_release) = std::sync::mpsc::channel::<()>();
        let holder = {
            let root = root.clone();
            std::thread::spawn(move || {
                workflow::update_state(&root, phase, |_| {
                    holding.send(()).unwrap();
                    let _ = wait_release.recv_timeout(std::time::Duration::from_secs(30));
                    Ok::<(), workflow::WorkflowError>(())
                })
                .unwrap();
            })
        };
        wait_holding.recv().unwrap();
        let pid = std::process::id();

        let line = write_lock_line(&workflow::write_lock_status(&root, phase).unwrap())
            .expect("a held lock is named");
        for expected in [
            format!("pid {pid}"),
            "about 10 s in a gate answer".to_string(),
            "13 s while".to_string(),
            "16 s in a reservation".to_string(),
            "past 20 s".to_string(),
        ] {
            assert!(line.contains(&expected), "missing {expected:?}: {line}");
        }
        assert!(!line.contains("takes milliseconds;"), "{line}");
        let rendered = rendered_status(&state);
        assert!(rendered.contains(&line), "status prints it: {rendered}");
        let held = reconcile_phase(&facts_for(&root, &state));
        let finding = held
            .iter()
            .find(|finding| finding.detail.contains("state write lock"))
            .expect("doctor names a held lock");
        assert_eq!(finding.severity, Severity::Warn);
        assert!(
            finding.detail.contains(&format!("pid {pid}")),
            "{}",
            finding.detail
        );
        assert!(finding.detail.contains("past 20 s"), "{}", finding.detail);

        drop(release);
        holder.join().unwrap();
        assert_eq!(
            write_lock_line(&workflow::write_lock_status(&root, phase).unwrap()),
            None,
            "control: released"
        );
        assert!(
            !rendered_status(&state).contains("state write lock"),
            "control"
        );
        let free = reconcile_phase(&facts_for(&root, &state));
        assert!(
            !free
                .iter()
                .any(|finding| finding.detail.contains("state write lock")),
            "control: released"
        );
    }

    /// Criterion 2 at a gate nothing waits on: `stop`'s own line names no `resume` while
    /// a pending supervisor lives, and names it again once that supervisor is gone.
    #[test]
    fn stop_names_no_resume_at_a_waiterless_gate_while_a_supervisor_lives() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5185);
        let state = state_with_pending(root, phase, false);
        let hold = hold_pending(&state);
        let live = stop_no_waiter_repair(root, phase, Stage::Code);
        assert!(!live.contains("devflow resume"), "{live}");
        drop(hold);
        let gone = stop_no_waiter_repair(root, phase, Stage::Code);
        assert_eq!(gone, no_waiter_repair(phase, Stage::Code), "control");
        assert!(gone.contains("devflow resume"), "control: {gone}");
    }
}
