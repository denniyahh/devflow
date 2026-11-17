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
use crate::phase_worktree::{ensure_phase_worktree, leftover_refusal, unmerged_commits};
use crate::pipeline_gate::print_dry_run;
use crate::pipeline_launch::single_active_phase;
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
    GateError, GateFile, GateId, GateResponse, Gates, OpenGate, QuarantinedAnswer, WaiterStatus,
    is_gate_id,
};
use devflow_core::git::{GitFlow, git_command};
use devflow_core::history;
use devflow_core::liveness;
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::registry;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::workflow;
use devflow_core::worktree;
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
/// refused at the first stage for an artifact that existed.
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
/// `State::new` zeroes all of them, and `start()` calls it unconditionally.
/// A plain `State` field is therefore per-RUN, while D-07
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
/// **The three events that DO reset the total**, none of which is "a new
/// process started":
///
/// 1. **State removal** — `workflow::clear_state` deletes
///    `.devflow/state-{NN}.json` (an abort does, and so does `cleanup` or
///    `discard` with the run's worktree), so the next start for this phase finds nothing
///    to carry. No code here.
/// 2. **Operator approval at the ceiling gate** — `handle_validate_outcome`
///    zeroes the total when a human advances or loops back AND
///    `mode::phase_failure_ceiling_reached` is true.
/// 3. **A finished run** — a finish keeps its state, but a `start` over a
///    state recorded `finished` begins at zero, because that run passed
///    Validate (operator decision, 2026-11-11). A stopped run that did not
///    finish still carries its total.
pub(crate) fn fresh_state_carrying_phase_failures(
    project_root: &Path,
    phase: PhaseId,
    agent: AgentKind,
    mode: Mode,
) -> State {
    let mut state = State::new(phase, agent, mode, project_root.to_path_buf());
    let loaded = workflow::load_state(project_root, phase);
    let (carried, warning) = carried_phase_failures(phase, loaded);
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
        // A finished run passed Validate, so its failures say nothing about
        // the next run (operator decision, 2026-11-11).
        Ok(persisted) if persisted.finished => (0, None),
        Ok(persisted) => (persisted.phase_validate_failures, None),
        // Genuine zero: a phase's first start, or one whose state was removed.
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
    for stage in [Stage::Plan, Stage::Code, Stage::Validate] {
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
}

/// What `devflow start --until validate` prints (52-27, pi S3): Validate is
/// the last stage, so the run ends at its finish or at a gate as it would
/// without `--until`, and a gaps verdict runs the gap-closure loop.
const UNTIL_VALIDATE_NOTICE: &str = "note: --until validate halts nothing: Validate is the \
last stage, so the run ends at its finish or at a gate, as it would without --until";

#[allow(clippy::too_many_arguments)]
pub(crate) fn start(
    project_root: &Path,
    phase: PhaseId,
    agent: AgentKind,
    mode: Mode,
    worktree: bool,
    dry_run: bool,
    until: Option<Stage>,
    fallback: FallbackSource,
) -> Result<(), CliError> {
    // 999.229: said first, before any other output, a refusal or the dry-run
    // return. The detached supervisor that reads the value has no foreground.
    if let Some(warning) = config::external_verify_env_warning_now() {
        eprintln!("{warning}");
    }

    // 999.78/A-11: NOT a bare `State::new`. See
    // `fresh_state_carrying_phase_failures` for why one field survives a
    // forced restart and every other counter does not.
    let mut state = fresh_state_carrying_phase_failures(project_root, phase, agent, mode);
    state.stop_until = until;
    // Persist the operator's gate notify hook for the processes that outlive
    // this one (monitor chain, rate-limit resume), on the same "before
    // `create_run_state`" idiom. Captured here rather than in `State::new`
    // so constructing a state never reads the environment (49-REVIEW.md
    // WR-01); `resume` and `advance` apply the same rule (CR-03).
    state.gate_notify_cmd = devflow_core::gates::apply_notify_env(state.gate_notify_cmd.take());

    // R-3: `devflow.toml`'s `fallback_agent` key, combined with the flag: the
    // flag wins when given, the key applies when it is absent. Resolved
    // unconditionally, also when the flag is given, so a broken key refuses
    // `start` whatever the flag says: the flag wins the value, it does not
    // excuse a broken config file (stricter than `base_branch`, whose env
    // override skips the file).
    let (fallback_agent, from_config) = match fallback {
        FallbackSource::FlagOrConfig(flag) => {
            let config_fallback =
                config::fallback_agent(project_root).map_err(CliError::Message)?;
            (
                flag.or(config_fallback),
                flag.is_none() && config_fallback.is_some(),
            )
        }
    };

    // D-01, D-02, D-03: the fallback is checked against the state as combined above
    // (agent and mode) and set before `dry_run`, the holder and the
    // guards, so a refused fallback changes nothing and a dry run shows it.
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
        // supplied the fallback.
        if from_config {
            println!(
                "note: fallback agent {fallback} set by devflow.toml (fallback_agent) — pass \
                 --fallback-agent to choose another"
            );
        }
    }

    if dry_run {
        return start_dry_run(project_root, &state);
    }

    // D-03: no launch without an answering user manager, and no fallback launch.
    refuse_without_a_user_manager(phase, LaunchVerb::Start)?;

    // D-07: hold the phase across the guards, the state write and the launch, until
    // systemd-run returns. The phase holder is what keeps a second launcher out.
    let phase_holder = claim_foreground_holder(project_root, phase)?;
    // D-07a, D-12: the guards' facts are read under the holder, before the write lock; a
    // unit still stopping is waited out there before liveness is read.
    let facts = gather_guard_facts(project_root, phase, LaunchVerb::Start)?;

    // D-01, D-08: the guards run before any gate cleanup, git mutation or run state,
    // so a refusal changes nothing.
    let reserved = reserve_launch(project_root, phase, LaunchVerb::Start, facts, Some(&state))?;
    let mut created = reserved.created;
    #[cfg(test)]
    crate::pipeline_launch::launch_test_hold("holder_claimed");
    let result = start_reserved(project_root, state, worktree, &mut created);
    crate::pipeline_launch::abandon_reservation(project_root, phase, created);
    let mode = result?;
    match crate::service::launch_runner_unit(project_root, phase) {
        Ok(unit) => {
            drop(phase_holder);
            print_runner_launched(
                phase,
                &format!("started phase {phase} in {mode} mode"),
                &unit,
            );
            Ok(())
        }
        Err(why) => {
            // WR-11's rule: a run that never launched leaves no state behind, so
            // `devflow status` reports no phantom run. Its worktree and branch stay, and
            // the next `start` refuses them (52.1 D-09.1), so the line names `discard`.
            if let Err(clear_err) = workflow::clear_state(project_root, phase) {
                eprintln!("warning: could not clear state after the failed launch: {clear_err}");
            }
            Err(CliError::Message(failed_launch_message(
                phase, worktree, &why,
            )))
        }
    }
}

/// What `start` prints when its unit launch failed: the reason, what the start created
/// and kept, and the verb that removes it before the next start.
fn failed_launch_message(phase: PhaseId, worktree: bool, why: &str) -> String {
    let kept = if worktree {
        "its worktree and branch were created and are kept"
    } else {
        "its branch was created and is kept"
    };
    format!(
        "start: phase {phase} was not launched: {why}; {kept}: run \
         `devflow discard --phase {phase}` before starting it again"
    )
}

/// `start --dry-run` (52.1 D-26, D-27): the real start's guard sequence, read-only and
/// in its order, then the agent-binary check and the base's resolution, then the line
/// naming what it skipped and the pipeline preview. The manager probe comes first, as
/// in [`start`] (D-03); the facts are read without the phase holder, the write lock or
/// a reservation, and the state is read without the lock, so nothing is written. Never
/// [`crate::preflight::ensure_base_ref_current`], which fetches and may fast-forward.
fn start_dry_run(project_root: &Path, state: &State) -> Result<(), CliError> {
    let phase = state.phase;
    refuse_without_a_user_manager(phase, LaunchVerb::Start)?;
    let facts = gather_guard_facts(project_root, phase, LaunchVerb::Start)?;
    let state_file = workflow::state_path(project_root, phase);
    let existing = match workflow::load_state(project_root, phase) {
        Ok(loaded) => workflow::Existing::Loaded(loaded),
        Err(workflow::WorkflowError::MissingState(_)) => workflow::Existing::Missing,
        Err(error) => workflow::Existing::Unloadable(error),
    };
    let facts = GuardFacts {
        state: StateFact::of(&existing, &state_file),
        ..facts
    };
    launch_guards(phase, &facts)?;
    // 52.1 D-27: the two local checks, at the real start's position after its guards.
    ensure_agent_binary(agent_program(state.agent))?;
    let base = crate::preflight::resolve_start_base(project_root)?.value;
    println!("{}", dry_run_not_checked_line(phase, &base));
    print_dry_run(state);
    Ok(())
}

/// The start checks a dry run skips (52.1 D-27): each reads the remote or the base's
/// tree, and the first may fast-forward the base, so only a real start runs them.
fn dry_run_not_checked_line(phase: PhaseId, base: &str) -> String {
    format!(
        "not checked by the dry run: base currency (`{base}` against its remote), phase \
         reachability (phase {phase} on `{base}`), and the CONTEXT.md/PLAN.md checks on \
         `{base}`; a real start runs them after these checks"
    )
}

/// D-03 (review codex-2, agy-6): refuse `verb` when the systemd user manager does not
/// answer, or answers with a version older than [`crate::service::MIN_SYSTEMD_VERSION`],
/// with no fallback launch. The refusal carries systemctl's stderr and names what is
/// missing; only an Absent manager gets the lingering hint.
pub(crate) fn refuse_without_a_user_manager(
    phase: PhaseId,
    verb: LaunchVerb,
) -> Result<(), CliError> {
    let unavailable = match crate::service::manager_available() {
        Ok(version) if version < crate::service::MIN_SYSTEMD_VERSION => {
            return Err(CliError::Message(format!(
                "{} refused for phase {phase}: systemd user manager {version} is older than \
                 {}, which DevFlow's unit launch needs (systemd-run --expand-environment=no); \
                 nothing was written",
                verb.label(),
                crate::service::MIN_SYSTEMD_VERSION
            )));
        }
        Ok(_) => return Ok(()),
        Err(unavailable) => unavailable,
    };
    let hint = if unavailable.kind == crate::service::ManagerUnavailableKind::Absent {
        "; unattended runs need one: loginctl enable-linger, or see \
         docs/guides/unattended-mode.md"
    } else {
        ""
    };
    Err(CliError::Message(format!(
        "{} refused for phase {phase}: {}{hint}; nothing was written",
        verb.label(),
        unavailable.reason()
    )))
}

/// D-07a, D-12 (review agy-1): read [`runner_liveness`] under the caller's phase holder
/// and, when it reads Stopping, wait for the unit with
/// [`crate::service::UNIT_STOP_WAIT`] and read it again. Only a held runner lock refuses
/// a launch, and that refusal is the launch guard's; this refuses a unit still not
/// inactive after the bound, naming it and its state, and a liveness read that failed,
/// printing its text (systemctl's stderr included).
pub(crate) fn wait_out_a_stopping_unit(
    project_root: &Path,
    phase: PhaseId,
    verb: LaunchVerb,
) -> Result<RunnerLiveness, CliError> {
    let refusal = |why: String| {
        CliError::Message(format!(
            "phase {phase}: {why} — refusing to {}; nothing was written",
            verb.label()
        ))
    };
    let liveness = runner_liveness(project_root, phase).map_err(refusal)?;
    let RunnerLiveness::Stopping { unit, .. } = liveness else {
        return Ok(liveness);
    };
    crate::service::wait_unit_inactive(&unit, crate::service::UNIT_STOP_WAIT).map_err(refusal)?;
    match runner_liveness(project_root, phase).map_err(refusal)? {
        RunnerLiveness::Stopping { unit, state } => Err(refusal(format!(
            "unit {unit} reads {state} again after it read inactive"
        ))),
        again => Ok(again),
    }
}

/// What `start` and `resume` print once systemd-run started the runner's unit (D-01),
/// plus the logout warning when lingering is off (D-04c).
pub(crate) fn print_runner_launched(phase: PhaseId, headline: &str, unit: &str) {
    println!("{headline} as user service {unit}");
    println!("  status:     devflow status");
    println!("  watch live: devflow logs --phase {phase} -f");
    match crate::service::lingering_enabled() {
        Ok(true) => {}
        Ok(false) => eprintln!(
            "warning: lingering is off for this user, so the run ends at logout; run \
             `loginctl enable-linger` to keep it running"
        ),
        Err(why) => eprintln!(
            "warning: could not tell whether lingering is on ({why}); without it the run \
             ends at logout"
        ),
    }
}

/// [`start`] after its guards: everything that can fail or return without launching,
/// so `start` gives the phase back whichever way this ends ([`abandon_reservation`]).
/// `created` goes `false` once the state file holds this run, not only what the
/// guard write left.
///
/// [`abandon_reservation`]: crate::pipeline_launch::abandon_reservation
fn start_reserved(
    project_root: &Path,
    mut state: State,
    worktree: bool,
    created: &mut bool,
) -> Result<Mode, CliError> {
    let (phase, agent, mode) = (state.phase, state.agent, state.mode);
    set_aside_leftover_gates(project_root, phase, &mut std::io::stderr());

    // 14-CR-05: fail on a missing agent binary BEFORE any branch/worktree is
    // scaffolded (launch_stage re-checks for the advance-time launch paths).
    ensure_agent_binary(agent_program(agent))?;

    // 45-01 / D-01 / AUTO-01: resolve the project's integration trunk once,
    // here, and use the SAME value for every guard and both fork paths below.
    // Resolved before anything is spawned or mutated. A `main`, blank or
    // flag-shaped base is refused rather than silently falling back to
    // `develop`, and a configured base must be a local branch: a commit-ish
    // that is not one would be forwarded raw to `git worktree add` (46-02, D-07).
    let resolved_base = crate::preflight::resolve_start_base(project_root)?;
    let base = resolved_base.value.as_str();
    // CR-02 (45-REVIEW.md): persist the resolved trunk before
    // `create_run_state`, because `advance`, `resume` and the checkout hooks
    // that merge this phase branch all run in SEPARATE processes whose
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
    // hardcoded `agent == Codex`. A `RequiresExistingArtifact` first stage (Plan)
    // must have CONTEXT.md on the base branch, and also warns without a PLAN.md. Fail in one second with instructions instead of
    // after a burned agent run and a dead-end gate. Checked on the RESOLVED
    // base (the branch worktrees fork from), so the result does not depend on
    // what the primary checkout happens to have checked out — and, since
    // 45-01, does not silently probe a branch the run will not use.
    let driver = agents::driver_for(agent);
    if start_requires_phase_context(driver.as_ref())
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

    // CR-45-03: the `--no-worktree` fork below uses the base resolved once at the top
    // and persisted onto `State`, never a second read of `devflow.toml` / the
    // environment, which falls soft to `develop` where `base_branch` fails hard.
    // 52.1 D-24: no check of how far the primary checkout is behind the base runs
    // here: `feature_start` cuts the phase branch from the base whatever is checked out.
    let resolved_git_flow = devflow_core::config::GitFlowConfig {
        develop: resolved_base.value.clone(),
        ..devflow_core::config::GitFlowConfig::default()
    };

    if worktree {
        let wt = ensure_phase_worktree(project_root, phase, base)?;
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
        let git = GitFlow::with_config(project_root, resolved_git_flow);
        // 52.1 D-09.1, D-10: a leftover branch is refused before `feature_start`, which
        // would switch the primary checkout to the base before failing on it.
        let branch = format!("{FEATURE_PREFIX}phase-{padded}", padded = phase.padded());
        if git.branch_exists(&branch) {
            return Err(leftover_refusal(phase, &format!("branch `{branch}`"), base));
        }
        let branch = git.feature_start(phase)?;
        println!("created feature branch: {branch}");
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
    // "combine before `create_run_state`" idiom for the base branch and
    // D-11's opt-out.
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

    // Mark the run prepared, after the `if worktree { ... }` fork, where
    // `state.worktree_path` holds its final value. `run_never_prepared` reads
    // this flag to tell this run from a state only the guard write left.
    // Persisted by `create_run_state` below rather than by a second write.
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

    // WR-11 (13-REVIEW.md), revised: the state is on disk before the runner unit
    // exists, because the runner's first step loads it. If the launch then fails,
    // `start` clears it, so `devflow status` reports no phantom run.
    create_run_state(project_root, phase, &state)?;
    *created = false;
    events::emit(
        project_root,
        phase,
        "workflow_started",
        workflow_started_payload(&state),
    );
    // 52-22: which GSD answers this run's verification reads, once per run.
    crate::pipeline_outcomes::record_gsd_runtime(project_root, phase);
    // 52-27 (pi S3): `--until validate` halts nothing in v4, so say so once.
    if state.stop_until == Some(Stage::Validate) {
        println!("{UNTIL_VALIDATE_NOTICE}");
        events::emit(
            project_root,
            phase,
            "until_noop",
            serde_json::json!({ "until": Stage::Validate.to_string() }),
        );
    }
    Ok(mode)
}

/// Whether `start` must find the phase's CONTEXT.md on the base before launching `driver`:
/// the driver declares the first stage a run launches as needing an existing artifact.
fn start_requires_phase_context(driver: &dyn agents::AgentDriver) -> bool {
    driver.interactivity_mode(Stage::Plan) == agents::InteractivityMode::RequiresExistingArtifact
}

/// Whether `state` is only what a `start`'s guard write left: that `start` died before
/// preparing the run, so no worktree was forked, no base resolved and none of its guards
/// passed. Every prepared run sets `verification_baseline_captured` in [`start_reserved`], and
/// nothing resets it. `worktree_path` cannot say this, because `--no-worktree` leaves it
/// `None` for a prepared run. The stage cannot say it either, since a fresh state and a
/// prepared run both sit at the first stage, and the event log cannot, because it is
/// project-wide and append-only: an earlier run of the same phase left its events there.
pub(crate) fn run_never_prepared(state: &State) -> bool {
    !state.verification_baseline_captured
}

/// The `start` that prepares what [`run_never_prepared`] says is missing. A worktree already
/// at the phase's path makes `start` refuse (52.1 D-09.1), so that case names
/// `devflow discard` first, with what it removes after a `#` so the line still pastes into a
/// shell.
fn restart_verb(state: &State) -> String {
    let verb = format!(
        "devflow start --phase {} --agent {} --mode {}",
        state.phase, state.agent, state.mode
    );
    let worktree_path = worktree::phase_path(&state.project_root, state.phase);
    if worktree_path.exists() {
        format!(
            "devflow discard --phase {} && {verb}  # discard removes {}, its branch and this \
             state, and refuses a branch with commits not merged into the base unless you name it",
            state.phase,
            worktree_path.display()
        )
    } else {
        verb
    }
}

/// `resume`'s refusal of a run [`run_never_prepared`] names.
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
/// guard write left. This is the one documented write of a whole state: it replaces
/// what is on disk. The phase holder `start` holds keeps every other launcher out, so
/// nothing else wrote the phase's state in between (D-07).
fn create_run_state(project_root: &Path, phase: PhaseId, state: &State) -> Result<(), CliError> {
    workflow::update_state_with(project_root, phase, |_existing| {
        Ok::<_, CliError>((Some(state.clone()), ()))
    })
}

/// What a launch's guard write left, at the top of `start` and `resume`.
pub(crate) struct Reserved {
    /// Whether the guard write created the state file, so that giving the phase back
    /// removes it.
    pub(crate) created: bool,
    /// The state as the guard write left it.
    pub(crate) state: State,
}

/// A runner holding the phase's runner lock (D-07), as the launch guards read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LiveRunner {
    /// The runner's recorded pid, `None` when its record does not read.
    pub(crate) pid: Option<u32>,
}

/// The phase's run state as the launch guards read it under the write lock (52.1 D-20).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum StateFact {
    /// No state file.
    #[default]
    Missing,
    /// Only what a killed `start`'s guard write left ([`run_never_prepared`]): no run.
    ReservationOnly,
    /// A prepared run that has not finished.
    Incomplete {
        /// The stage it is at.
        stage: Stage,
    },
    /// A run that finished (its Validate passed).
    Finished,
    /// A state file that does not load.
    Unloadable {
        /// The state file.
        path: PathBuf,
        /// Why it does not load.
        error: String,
    },
}

impl StateFact {
    /// What `existing`, read from `state_file`, is to the guards. Finished is read
    /// before reservation-only, and both before incomplete.
    fn of(existing: &workflow::Existing, state_file: &Path) -> Self {
        match existing {
            workflow::Existing::Missing => StateFact::Missing,
            workflow::Existing::Loaded(state) if state.finished => StateFact::Finished,
            workflow::Existing::Loaded(state) if run_never_prepared(state) => {
                StateFact::ReservationOnly
            }
            workflow::Existing::Loaded(state) => StateFact::Incomplete { stage: state.stage },
            workflow::Existing::Unloadable(error) => StateFact::Unloadable {
                path: state_file.to_path_buf(),
                error: error.to_string(),
            },
        }
    }
}

/// Everything [`launch_guards`] decides on (D-12, D-26), read before the state write
/// lock by [`gather_guard_facts`] except `state`, which [`reserve_launch`]'s closure
/// fills from the state it holds. It carries no unit state: a unit still stopping is
/// waited out before the facts are read (D-07a, review agy-1), so only a held runner
/// lock is a live run.
#[derive(Debug, Clone, Default)]
pub(crate) struct GuardFacts {
    /// The runner holding the phase's runner lock, `None` when the lock is free.
    pub(crate) live_runner: Option<LiveRunner>,
    /// The phase's run state.
    pub(crate) state: StateFact,
    /// The phase's gate files ([`phase_gate_files`]): orphans once no run is left to
    /// answer them (D-09a).
    pub(crate) orphan_gate_files: Vec<PathBuf>,
    /// The phase worktree's path, when it exists.
    pub(crate) leftover_worktree: Option<PathBuf>,
    /// The phase branch, when it exists.
    pub(crate) leftover_branch: Option<String>,
    /// The base the leftover refusal names; resolved only when a leftover was found.
    pub(crate) base: String,
}

impl GuardFacts {
    /// The facts one liveness read gives. [`RunnerLiveness::Stopping`] reads as no live
    /// runner, because the caller waited it out first ([`wait_out_a_stopping_unit`]),
    /// which never returns it.
    pub(crate) fn from_liveness(liveness: &RunnerLiveness) -> Self {
        let live_runner = match liveness {
            RunnerLiveness::Live { pid } => Some(LiveRunner { pid: *pid }),
            RunnerLiveness::Stopping { .. } | RunnerLiveness::NotLive => None,
        };
        GuardFacts {
            live_runner,
            ..GuardFacts::default()
        }
    }
}

/// Read what `verb`'s guards decide on, under the caller's phase holder and before the
/// state write lock, so no process is spawned under that lock (D-08): first
/// [`wait_out_a_stopping_unit`], whose liveness read gives the live row, then the
/// phase's gate files, then whether its worktree and branch exist (`git`).
///
/// # Errors
/// A liveness read that failed, a unit still not inactive after the bound, an
/// unreadable gates directory, or, with a leftover found, a base that does not resolve.
pub(crate) fn gather_guard_facts(
    project_root: &Path,
    phase: PhaseId,
    verb: LaunchVerb,
) -> Result<GuardFacts, CliError> {
    let liveness = wait_out_a_stopping_unit(project_root, phase, verb)?;
    let orphan_gate_files = phase_gate_files(project_root, phase)?;
    let worktree_path = worktree::phase_path(project_root, phase);
    let leftover_worktree = worktree_path.exists().then_some(worktree_path);
    let branch = format!("{FEATURE_PREFIX}phase-{}", phase.padded());
    let leftover_branch = GitFlow::for_project(project_root)
        .branch_exists(&branch)
        .then_some(branch);
    let base = if leftover_worktree.is_some() || leftover_branch.is_some() {
        config::base_branch(project_root)
            .map_err(CliError::Message)?
            .value
    } else {
        String::new()
    };
    Ok(GuardFacts {
        orphan_gate_files,
        leftover_worktree,
        leftover_branch,
        base,
        ..GuardFacts::from_liveness(&liveness)
    })
}

/// `start`'s one guard sequence (D-12, D-26), pure over `facts`: live, finished,
/// incomplete, unloadable, orphan gate files, leftover worktree or branch. The first
/// refusal wins, and each names the verb that applies (52.1 D-09.1). A missing or
/// reservation-only state is no run (52.1 D-20), so only the last two rows see it.
pub(crate) fn launch_guards(phase: PhaseId, facts: &GuardFacts) -> Result<(), CliError> {
    refuse_a_live_runner(phase, LaunchVerb::Start, facts)?;
    refuse_an_existing_run(phase, &facts.state)?;
    if !facts.orphan_gate_files.is_empty() {
        let files: Vec<String> = facts
            .orphan_gate_files
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        return Err(CliError::Message(format!(
            "phase {phase}: gate files are left with no run to answer them ({}) — refusing to \
             start; nothing was written. Remove them with `devflow discard --phase {phase}`, \
             then run this start again",
            files.join(", ")
        )));
    }
    if let Some(path) = &facts.leftover_worktree {
        let what = format!("the phase worktree {}", path.display());
        return Err(leftover_refusal(phase, &what, &facts.base));
    }
    if let Some(branch) = &facts.leftover_branch {
        return Err(leftover_refusal(
            phase,
            &format!("branch `{branch}`"),
            &facts.base,
        ));
    }
    Ok(())
}

/// `start`'s state rows: a finished, incomplete or unloadable state refuses, each
/// naming the verb that acts on it.
fn refuse_an_existing_run(phase: PhaseId, state: &StateFact) -> Result<(), CliError> {
    match state {
        StateFact::Incomplete { stage } => Err(CliError::Message(format!(
            "phase {phase} has an incomplete run at its {stage} stage, and `start` runs only a \
             new phase — refusing to start; nothing was written. Continue it with \
             `devflow resume --phase {phase}`, or remove it with `devflow discard --phase {phase}`"
        ))),
        StateFact::Unloadable { path, error } => Err(CliError::Message(format!(
            "phase {phase}: its state file {} does not load ({error}), and `start` replaces \
             nothing — refusing to start; nothing was written. Remove it with \
             `devflow discard --phase {phase}`, then run this start again",
            path.display()
        ))),
        StateFact::Finished => Err(CliError::Message(format!(
            "phase {phase} finished (its Validate passed), and `start` runs only a new phase \
             — refusing to start; nothing was written. To end it, close the phase out, then remove \
             its run with `devflow discard --phase {phase}`"
        ))),
        _ => Ok(()),
    }
}

/// `resume`'s guards (52.1 D-09.1), pure over `facts`: [`launch_guards`]' live row,
/// then the states `resume` does not continue, each naming the one verb that applies:
/// a finished run (the close-out or `discard`), no state (`start`), and a state that
/// does not load (`discard`). A reservation-only state is refused after the guard
/// write, by [`refuse_unprepared_run`].
pub(crate) fn resume_guards(phase: PhaseId, facts: &GuardFacts) -> Result<(), CliError> {
    refuse_a_live_runner(phase, LaunchVerb::Resume, facts)?;
    let why = match &facts.state {
        StateFact::Finished => format!(
            "phase {phase} finished (its Validate passed), so there is nothing to resume; \
             nothing was written. To end it, close the phase out, then remove its run with \
             `devflow discard --phase {phase}`"
        ),
        StateFact::Missing => format!(
            "phase {phase} has no active DevFlow state, so there is nothing to resume; nothing \
             was written. Run `devflow start --phase {phase}` to start a run"
        ),
        StateFact::Unloadable { path, error } => format!(
            "phase {phase}: its state file {} does not load ({error}), so there is nothing to \
             resume; nothing was written. Remove it with `devflow discard --phase {phase}`",
            path.display()
        ),
        StateFact::ReservationOnly | StateFact::Incomplete { .. } => return Ok(()),
    };
    Err(CliError::Message(format!("resume refused: {why}")))
}

/// The guard sequence's first row (D-07, D-12): a held runner lock refuses `verb`,
/// naming `devflow stop`. A lock-free unit never reaches here as live.
fn refuse_a_live_runner(
    phase: PhaseId,
    verb: LaunchVerb,
    facts: &GuardFacts,
) -> Result<(), CliError> {
    let Some(runner) = facts.live_runner else {
        return Ok(());
    };
    // 52.1 D-26 clarification: after the stop, the verb that acts on what it leaves.
    let then = match facts.state {
        StateFact::Incomplete { .. } => format!("devflow resume --phase {phase}"),
        StateFact::Finished | StateFact::Unloadable { .. } => {
            format!("devflow discard --phase {phase}")
        }
        StateFact::Missing | StateFact::ReservationOnly => verb.rerun(phase),
    };
    Err(CliError::Message(format!(
        "phase {phase}: its runner holds the runner lock{} — refusing to {}; nothing was \
         written. Run `devflow stop --phase {phase}` or wait for it to finish, then run \
         `{then}`.",
        runner_pid_note(runner.pid),
        verb.label(),
    )))
}

/// Run a launch's guards in one write-lock closure (D-01): [`launch_guards`] for
/// `start`, [`resume_guards`] for `resume`, then, for `start`,
/// write `fresh` when no state loads. `facts` were read before the lock
/// ([`gather_guard_facts`]); the closure fills in the state it holds. The phase
/// holder the caller holds across the guards, the state write and the unit launch keeps
/// a second launcher out (D-07). `resume` passes `None` and gets its missing-state or
/// load error once the guard has passed.
///
/// A refused launch changes no state. No spawn, hook or gate wait happens under the
/// write lock (D-08).
pub(crate) fn reserve_launch(
    project_root: &Path,
    phase: PhaseId,
    verb: LaunchVerb,
    facts: GuardFacts,
    fresh: Option<&State>,
) -> Result<Reserved, CliError> {
    let reserved = workflow::update_state_with(project_root, phase, |existing| {
        let state_file = workflow::state_path(project_root, phase);
        let facts = GuardFacts {
            state: StateFact::of(&existing, &state_file),
            ..facts
        };
        match verb {
            LaunchVerb::Start => launch_guards(phase, &facts)?,
            LaunchVerb::Resume => resume_guards(phase, &facts)?,
        }
        #[cfg(test)]
        crate::pipeline_launch::launch_test_hold("guarded");
        let (next, created) = match (existing, fresh) {
            (workflow::Existing::Loaded(state), _) => (state, false),
            (workflow::Existing::Missing, Some(fresh)) => (fresh.clone(), true),
            (workflow::Existing::Missing, None) => {
                return Err(workflow::WorkflowError::MissingState(state_file).into());
            }
            // 52.1 D-20: `start`'s guards refused it above; nothing replaces it.
            (workflow::Existing::Unloadable(error), _) => return Err(error.into()),
        };
        let reserved = Reserved {
            state: next.clone(),
            created,
        };
        Ok::<_, CliError>((created.then_some(next), reserved))
    })?;
    #[cfg(test)]
    crate::pipeline_launch::launch_test_hold("reserved");
    Ok(reserved)
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
// cleanup / status / recover
// ---------------------------------------------------------------------------

/// Parse the phase number encoded in a `.worktrees/phase-NN[-agent]` path.
/// Used only as a fallback join key when no persisted `State.worktree_path`
/// matches the worktree entry (review: Codex MEDIUM — worktree->phase join).
/// Returns `None` for paths that don't follow this naming, which excludes
/// them from the liveness guard: such a worktree has no owning phase/agent to
/// be alive.
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
/// the persisted `worktree_path` (set by `start`) and falling back
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

/// What [`cleanup`] did with one phase, or with one worktree no phase owns.
#[derive(Debug, PartialEq, Eq)]
enum CleanupOutcome {
    /// Something was removed.
    Removed,
    /// Kept, with a line saying why and naming the verb that removes it.
    Kept,
    /// Kept because a live run could not be ruled out or a removal failed: a line on
    /// stderr says why, and cleanup exits non-zero at the end.
    Unsettled,
    /// Nothing exists for the phase.
    Untouched,
}

/// Remove what is provably safe to remove, phase by phase (52.1 D-09.5, amended):
/// a finished run's worktree and state, a dead start's reservation, and a worktree with
/// no or unloadable state, each only when its branch is merged into the base, its
/// worktree is clean and no run is live. An incomplete run is kept whatever its branch,
/// naming `devflow resume` and `devflow discard`. Everything kept prints why, and the
/// verb that removes it. No path deletes a branch with commits not merged into the base
/// (52.1 D-10).
///
/// The pass is driven by `worktree::list`, so a worktree with no state is still seen,
/// then by `workflow::state_file_phases` for each phase no worktree resolved to. Each
/// phase is handled under its foreground holder, after [`cleanup_liveness`] (D-07,
/// D-07a). A phase whose run may be live, or whose liveness cannot be read, is kept
/// and named on stderr, the pass goes on, and cleanup exits non-zero at the end.
pub(crate) fn cleanup(project_root: &Path) -> Result<(), CliError> {
    // Project-resolved (45-01): the merged-branch sweep counts "merged" relative to the
    // trunk and treats it as protected, so it must be the configured one.
    let flow = devflow_core::config::git_flow_for_project(project_root);
    let git = GitFlow::with_config(project_root, flow);
    let worktrees_dir = worktree::worktrees_dir(project_root);
    let states = workflow::list_states(project_root);

    let mut outcomes = Vec::new();
    let mut seen = Vec::new();
    for wt in worktree::list(project_root)?
        .iter()
        .filter(|wt| wt.path.starts_with(&worktrees_dir))
    {
        let phase = state_for_worktree(&states, &worktrees_dir, wt)
            .map(|s| s.phase)
            .or_else(|| phase_from_worktree_path(&worktrees_dir, &wt.path));
        outcomes.push(match phase {
            Some(phase) => {
                seen.push(phase);
                cleanup_phase(project_root, phase, Some(wt))
            }
            None => cleanup_unowned_worktree(project_root, &git, wt),
        });
    }
    for phase in workflow::state_file_phases(project_root) {
        if !seen.contains(&phase) {
            seen.push(phase);
            outcomes.push(cleanup_phase(project_root, phase, None));
        }
    }
    // D-07: a held runner lock is a live run whether or not a state names it, so its
    // phase is kept and makes the exit non-zero rather than being passed over.
    for phase in runner_lock_phases_not_free(project_root) {
        if !seen.contains(&phase) {
            outcomes.push(cleanup_phase(project_root, phase, None));
        }
    }

    worktree::prune(project_root)?;
    if !outcomes.contains(&CleanupOutcome::Removed) {
        println!("no worktrees to clean up");
    }
    sweep_merged_branches(project_root, &git)?;
    let unsettled = outcomes
        .iter()
        .filter(|outcome| **outcome == CleanupOutcome::Unsettled)
        .count();
    if unsettled > 0 {
        return Err(CliError::Message(format!(
            "cleanup kept {unsettled} item(s) whose run may still be live or that could not be \
             removed; see the lines above"
        )));
    }
    Ok(())
}

/// The phases whose `.devflow/.runner-NN` file does not probe free: held, or a probe
/// that failed. A free lock file is left by every run that ended, so it names no phase.
fn runner_lock_phases_not_free(project_root: &Path) -> Vec<PhaseId> {
    let Ok(entries) = std::fs::read_dir(project_root.join(".devflow")) else {
        return Vec::new();
    };
    let mut phases: Vec<PhaseId> = entries
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()?
                .strip_prefix(".runner-")?
                .parse::<PhaseId>()
                .ok()
        })
        .filter(|&phase| {
            !matches!(
                liveness::probe(&liveness::runner_lock_path(project_root, phase)),
                Ok(liveness::Held::Abandoned | liveness::Held::Missing)
            )
        })
        .collect();
    phases.sort();
    phases
}

/// Delete the branches merged into the trunk. A merged branch that a kept worktree (or
/// any other checkout) still has checked out is skipped, not left to git to refuse.
fn sweep_merged_branches(project_root: &Path, git: &GitFlow) -> Result<(), CliError> {
    let checked_out: Vec<String> = worktree::list(project_root)?
        .into_iter()
        .filter_map(|wt| wt.branch)
        .collect();
    match git.cleanup_merged_keeping(&checked_out) {
        Ok(merged) => {
            for branch in merged {
                println!("deleted merged branch {branch}");
            }
        }
        Err(err) => println!("warning: could not prune merged branches: {err}"),
    }
    Ok(())
}

/// A `.worktrees/` entry no phase owns: removed as before 52.1 D-09.5, through
/// `git worktree remove` (which refuses a dirty worktree) and `git branch -d` (which
/// refuses an unmerged branch).
fn cleanup_unowned_worktree(
    project_root: &Path,
    git: &GitFlow,
    wt: &worktree::WorktreeInfo,
) -> CleanupOutcome {
    if let Err(err) = remove_worktree_with_retry(project_root, &wt.path, false) {
        println!(
            "warning: could not remove worktree {} after retrying — manually delete this directory: {err}",
            wt.path.display()
        );
        return CleanupOutcome::Kept;
    }
    print!("removed worktree {}", wt.path.display());
    match &wt.branch {
        Some(branch) if branch.starts_with(FEATURE_PREFIX) => {
            match git.delete_branch(branch, false) {
                Ok(()) => println!(" + deleted branch {branch}"),
                Err(err) => println!(" (branch {branch} kept: {err})"),
            }
        }
        _ => println!(),
    }
    CleanupOutcome::Removed
}

/// One phase of [`cleanup`], under its foreground holder: its worktree `wt` when one
/// resolved to it, else its state file alone.
fn cleanup_phase(
    project_root: &Path,
    phase: PhaseId,
    wt: Option<&worktree::WorktreeInfo>,
) -> CleanupOutcome {
    let state_file = workflow::state_path(project_root, phase);
    let what = match wt {
        Some(wt) => format!("worktree {}", wt.path.display()),
        None if state_file.exists() => format!("run state {}", state_file.display()),
        None => format!(
            "runner lock {}",
            liveness::runner_lock_path(project_root, phase).display()
        ),
    };
    let unsettled = |why: String| {
        eprintln!("keeping phase {phase} ({what}): {why}");
        CleanupOutcome::Unsettled
    };
    let _holder = match claim_foreground_holder(project_root, phase) {
        Ok(holder) => holder,
        Err(err) => return unsettled(err.to_string()),
    };
    let existing = match workflow::update_state_with_within(
        project_root,
        phase,
        workflow::WRITE_LOCK_REPAIR_WAIT,
        |existing| Ok::<_, CliError>((None, existing)),
    ) {
        Ok(existing) => existing,
        Err(err) => return unsettled(format!("its run state could not be read ({err})")),
    };
    if let Err(why) = cleanup_liveness(project_root, phase) {
        return unsettled(why);
    }
    if wt.is_none() && matches!(existing, workflow::Existing::Missing) {
        return CleanupOutcome::Untouched;
    }
    match wt {
        Some(wt) => cleanup_phase_worktree(project_root, phase, wt, &existing),
        None => cleanup_state_only(project_root, phase, &existing),
    }
}

/// D-07, D-07a: `Ok` when no run of `phase` is live, as [`runner_liveness`] reads it. A
/// unit that is still stopping is waited on for [`crate::service::UNIT_STOP_WAIT`], as
/// `discard` waits, and then read again. Otherwise the reason to keep the phase: a held
/// runner lock, a unit that does not stop, or a read that failed (systemctl's stderr
/// included).
fn cleanup_liveness(project_root: &Path, phase: PhaseId) -> Result<(), String> {
    let read = || {
        runner_liveness(project_root, phase)
            .map_err(|why| format!("{why}, so a live runner cannot be ruled out"))
    };
    let answer = match read()? {
        RunnerLiveness::Stopping { unit, state } => {
            eprintln!(
                "cleanup: phase {phase}'s unit {unit} reads {state}; waiting up to {}s for it to \
                 read inactive",
                crate::service::UNIT_STOP_WAIT.as_secs()
            );
            crate::service::wait_unit_inactive(&unit, crate::service::UNIT_STOP_WAIT).map_err(
                |why| {
                    format!(
                        "its unit {unit} did not read inactive ({why}); run `devflow stop --phase \
                         {phase}` or wait for it to finish"
                    )
                },
            )?;
            read()?
        }
        answer => answer,
    };
    match answer {
        RunnerLiveness::NotLive => Ok(()),
        RunnerLiveness::Live { pid } => Err(format!(
            "its runner holds the runner lock{}; run `devflow stop --phase {phase}` or wait for \
             it to finish",
            runner_pid_note(pid)
        )),
        RunnerLiveness::Stopping { unit, state } => Err(format!(
            "its unit {unit} reads {state} again after it read inactive; run `devflow stop \
             --phase {phase}` or wait for it to finish"
        )),
    }
}

/// D-09.5 for a phase worktree whose run is not live: an incomplete run is kept whatever
/// its branch; anything else is removed, with its state, only when its branch has no
/// commit outside the base and its worktree is clean (untracked files count).
fn cleanup_phase_worktree(
    project_root: &Path,
    phase: PhaseId,
    wt: &worktree::WorktreeInfo,
    existing: &workflow::Existing,
) -> CleanupOutcome {
    if let workflow::Existing::Loaded(state) = existing
        && incomplete_run(state)
    {
        println!(
            "keeping worktree {} for phase {phase}: {}",
            wt.path.display(),
            incomplete_run_keep(state)
        );
        return CleanupOutcome::Kept;
    }
    let held = cleanup_worktree_holds(project_root, wt, existing);
    if !held.is_empty() {
        println!(
            "keeping worktree {} for phase {phase} ({}): {}; run `devflow discard --phase \
             {phase}` to remove the worktree, the run's state and the branch (discard refuses \
             a branch with commits not merged into the base unless you name it)",
            wt.path.display(),
            run_description(existing),
            held.join(", and ")
        );
        return CleanupOutcome::Kept;
    }
    if let Err(err) = remove_worktree_with_retry(project_root, &wt.path, false) {
        eprintln!(
            "warning: could not remove worktree {} after retrying: {err}",
            wt.path.display()
        );
        return CleanupOutcome::Unsettled;
    }
    print!("removed worktree {}", wt.path.display());
    let (_, git) = discard_base(project_root, existing);
    match &wt.branch {
        // Every commit of it is in the base: just measured, under the holder.
        Some(branch) if branch.starts_with(FEATURE_PREFIX) => match git.delete_branch(branch, true)
        {
            Ok(()) => println!(" + deleted branch {branch}"),
            Err(err) => println!(" (branch {branch} kept: {err})"),
        },
        _ => println!(),
    }
    match existing {
        workflow::Existing::Loaded(state) => {
            remove_run_state_with_worktree(project_root, state, &wt.path);
        }
        workflow::Existing::Unloadable(_) => clear_run_state(project_root, phase),
        workflow::Existing::Missing => {}
    }
    CleanupOutcome::Removed
}

/// Why the worktree `wt` of a run that is not live is not yet safe to remove: its branch
/// has commits outside the run's base (or its merge state, or its HEAD, does not read),
/// or it holds uncommitted changes. Empty when it is safe.
fn cleanup_worktree_holds(
    project_root: &Path,
    wt: &worktree::WorktreeInfo,
    existing: &workflow::Existing,
) -> Vec<String> {
    let (base, git) = discard_base(project_root, existing);
    let mut held = Vec::new();
    match wt.branch.as_deref() {
        None => held.push(
            "it has a detached HEAD, so cleanup cannot tell what removing it would lose"
                .to_string(),
        ),
        Some(branch) => match git.commits_not_in_develop(branch) {
            Ok(0) => {}
            Ok(count) => held.push(format!(
                "its branch {branch} has {}",
                unmerged_commits(count, &base)
            )),
            Err(err) => held.push(format!(
                "its branch {branch} could not be checked for commits not merged into `{base}` \
                 ({err})"
            )),
        },
    }
    match uncommitted_changes(&wt.path) {
        Ok(0) => {}
        Ok(1) => held.push("it holds 1 uncommitted change".to_string()),
        Ok(count) => held.push(format!("it holds {count} uncommitted changes")),
        Err(why) => held.push(format!(
            "its uncommitted changes could not be counted ({why})"
        )),
    }
    held
}

/// D-09.5 for a phase with a state file and no worktree, whose run is not live: a dead
/// start's reservation ([`run_never_prepared`]) has only its state removed; every other
/// state is kept, naming the verb that removes it.
fn cleanup_state_only(
    project_root: &Path,
    phase: PhaseId,
    existing: &workflow::Existing,
) -> CleanupOutcome {
    let path = workflow::state_path(project_root, phase);
    let keep = match existing {
        workflow::Existing::Missing => return CleanupOutcome::Untouched,
        workflow::Existing::Loaded(state) if dead_reservation(state) => {
            return match workflow::clear_state(project_root, phase) {
                Ok(true) => {
                    println!(
                        "removed run state {} for phase {phase}: the start that reserved it never \
                         prepared its run, and it has no worktree",
                        path.display()
                    );
                    CleanupOutcome::Removed
                }
                Ok(false) => CleanupOutcome::Untouched,
                Err(err) => {
                    eprintln!(
                        "warning: could not remove the run state {}: {err}",
                        path.display()
                    );
                    CleanupOutcome::Unsettled
                }
            };
        }
        workflow::Existing::Loaded(state) if incomplete_run(state) => incomplete_run_keep(state),
        workflow::Existing::Loaded(_) | workflow::Existing::Unloadable(_) => format!(
            "{}, with no worktree; run `devflow discard --phase {phase}` to remove the run's \
             state and the branch (discard refuses a branch with commits not merged into the \
             base unless you name it)",
            run_description(existing)
        ),
    };
    println!(
        "keeping run state {} for phase {phase}: {keep}",
        path.display()
    );
    CleanupOutcome::Kept
}

/// Whether `state` is a run that has not finished: one that stopped, or one a start
/// prepared. Its state is the only copy of what `resume` continues from, so cleanup
/// keeps it whatever its branch (52.1 D-09.5 amendment).
fn incomplete_run(state: &State) -> bool {
    !state.finished && (state.stopped || !run_never_prepared(state))
}

/// Whether `state` is a dead start's reservation: a state no start prepared, that
/// neither stopped nor finished.
fn dead_reservation(state: &State) -> bool {
    !state.finished && !state.stopped && run_never_prepared(state)
}

/// The keep reason for an incomplete run (52.1 D-09.5 amendment): its state is the only
/// copy of what `resume` continues from, so it names `resume` and `discard`.
fn incomplete_run_keep(state: &State) -> String {
    let phase = state.phase;
    let kind = if state.stopped {
        let reason = state.stop_reason.as_deref().unwrap_or("no reason recorded");
        format!("the run stopped (stop reason: {reason})")
    } else {
        "the run did not complete and no runner is live".to_string()
    };
    format!(
        "{kind}; run `devflow resume --phase {phase}` to continue it, or `devflow discard \
         --phase {phase}` to remove the worktree, the run's state and the branch"
    )
}

/// What cleanup found for a phase's run, for its keep lines.
fn run_description(existing: &workflow::Existing) -> String {
    match existing {
        workflow::Existing::Loaded(state) if state.finished => {
            let reason = state.stop_reason.as_deref().unwrap_or("no reason recorded");
            format!("the run finished, stop reason: {reason}")
        }
        workflow::Existing::Loaded(_) => {
            "the start that reserved it never prepared its run".to_string()
        }
        workflow::Existing::Unloadable(err) => format!("its run state does not load: {err}"),
        workflow::Existing::Missing => "no run state".to_string(),
    }
}

/// Remove the run's state file once its worktree is gone (OPS-02, 999.213), so nothing
/// of the run is left behind. Only a state that records this worktree, or none, is
/// removed: a state joined by name whose recorded worktree is another path still owns
/// that one.
fn remove_run_state_with_worktree(project_root: &Path, state: &State, removed: &Path) {
    if state
        .worktree_path
        .as_deref()
        .is_some_and(|recorded| recorded != removed)
    {
        return;
    }
    clear_run_state(project_root, state.phase);
}

/// Remove `phase`'s state file, printing what happened.
fn clear_run_state(project_root: &Path, phase: PhaseId) {
    let path = workflow::state_path(project_root, phase);
    match workflow::clear_state(project_root, phase) {
        Ok(true) => println!("removed run state {}", path.display()),
        Ok(false) => {}
        Err(err) => println!(
            "warning: could not remove the run state {} for phase {phase}: {err}",
            path.display()
        ),
    }
}

/// `devflow discard --phase N`: remove the phase's worktree, its run state, its
/// feature branch, and the debris a killed run can leave (its gate files and hold
/// temps, D-06), after listing them.
///
/// The worktree and the state are the derived `.worktrees/phase-NN` and
/// `.devflow/state-NN.json`, never a path read from the state, and the branch is the
/// derived `feature/phase-NN`. The phase's foreground holder is held from the first
/// check to the last removal, so a `start` cannot begin in between. The lock files
/// `.holder-NN` and `.runner-NN` are never removed.
///
/// Every refusal comes before the first removal: a run that is live or may be
/// ([`phase_run_liveness`]), whatever [`discard_items`] cannot account for, a gate
/// whose waiter is live ([`discard_debris`]), a phase with nothing to discard, a
/// branch another checkout holds, and an unmerged branch that `delete_unmerged_branch`
/// does not name.
pub(crate) fn discard(
    project_root: &Path,
    phase: PhaseId,
    delete_unmerged_branch: Option<&str>,
) -> Result<(), CliError> {
    let _holder = claim_foreground_holder(project_root, phase)?;
    let existing = workflow::update_state_with_within(
        project_root,
        phase,
        workflow::WRITE_LOCK_REPAIR_WAIT,
        |existing| Ok::<_, CliError>((None, existing)),
    )?;
    let (base, git) = discard_base(project_root, &existing);
    let liveness = phase_run_liveness(project_root, phase);
    let liveness = discard_waits_out_a_stopping_unit(project_root, phase, liveness)?;
    refuse_discard_of_a_live_run(phase, liveness)?;
    let items = discard_items(project_root, phase, &existing, &git, base)?;
    let debris = discard_debris(project_root, phase)?;
    if items.worktree.is_none()
        && items.state.is_none()
        && items.unmerged.is_none()
        && debris.is_empty()
    {
        return Err(CliError::Message(format!(
            "discard refused: phase {phase} has no run state, worktree or branch, and no gate \
             file or hold temp; nothing to discard"
        )));
    }
    print_discard_listing(phase, &items, &debris);
    refuse_discard_of_a_checked_out_branch(phase, &items)?;
    refuse_discard_of_branch(phase, &items, delete_unmerged_branch)?;
    let removed = remove_discarded(project_root, phase, &git, &items);
    let debris_removed = remove_discarded_debris(project_root, phase, &debris);
    removed.and(debris_removed)
}

/// The suffix of a gate's waiter registration, `NN-{stage}.{gate_id}.waiter`: the
/// name `devflow_core::gates` gives it.
const GATE_WAITER_SUFFIX: &str = ".waiter";

/// The suffix of an answer set aside rather than consumed or deleted. A close keeps
/// every one (D-05), so discard neither lists nor removes one.
const GATE_QUARANTINED_SUFFIX: &str = ".quarantined.json";

/// What a killed run can leave for a phase beside its worktree, state and branch
/// (D-06), each removed by [`remove_discarded_debris`].
#[derive(Default)]
struct DiscardDebris {
    /// The stages with gate files, each closed through [`Gates::close`].
    gate_stages: Vec<Stage>,
    /// The phase's files in the gates directory: requests, answers, claims, unheld
    /// waiter registrations and write temps. Set-aside answers are not among them.
    gate_files: Vec<PathBuf>,
    /// The phase's hold temps in `.devflow/` and in the gates directory.
    hold_temps: Vec<PathBuf>,
}

impl DiscardDebris {
    fn is_empty(&self) -> bool {
        self.gate_files.is_empty() && self.hold_temps.is_empty()
    }
}

/// Read the phase's gate files and hold temps, removing nothing. Refuses a gate whose
/// waiter registration is held, or cannot be probed: something may still wait on it
/// (T-52.2-22). The lock files `.holder-NN` and `.runner-NN` are never listed.
fn discard_debris(project_root: &Path, phase: PhaseId) -> Result<DiscardDebris, CliError> {
    let gates = Gates::dir(project_root);
    let gate_prefix = format!("{}-", phase.padded());
    let mut debris = DiscardDebris::default();
    for name in file_names(&gates)? {
        let path = gates.join(&name);
        if let Some(final_name) = hold_temp_final_name(&name) {
            if final_name.starts_with(&gate_prefix) {
                debris.hold_temps.push(path);
            }
            continue;
        }
        let Some(stage) = phase_gate_file_stage(&name, &gate_prefix) else {
            continue;
        };
        if name.ends_with(GATE_WAITER_SUFFIX) {
            refuse_discard_of_a_live_waiter(phase, stage, &path)?;
        }
        if !debris.gate_stages.contains(&stage) {
            debris.gate_stages.push(stage);
        }
        debris.gate_files.push(path);
    }
    let devflow = workflow::devflow_dir(project_root);
    let locks = [
        liveness::phase_holder_path(project_root, phase),
        liveness::runner_lock_path(project_root, phase),
    ];
    for name in file_names(&devflow)? {
        let Some(final_name) = hold_temp_final_name(&name) else {
            continue;
        };
        if locks
            .iter()
            .any(|lock| lock.file_name().is_some_and(|lock| lock == final_name))
        {
            debris.hold_temps.push(devflow.join(&name));
        }
    }
    debris.gate_files.sort();
    debris.hold_temps.sort();
    Ok(debris)
}

/// The stage of `name` in the gates directory when it is one of a phase's gate files
/// (`gate_prefix` is `NN-`): a request, answer, claim, waiter registration or write
/// temp. `None` for any other name, a set-aside answer and a hold temp.
fn phase_gate_file_stage(name: &str, gate_prefix: &str) -> Option<Stage> {
    if hold_temp_final_name(name).is_some() || name.ends_with(GATE_QUARANTINED_SUFFIX) {
        return None;
    }
    let rest = name
        .strip_prefix('.')
        .unwrap_or(name)
        .strip_prefix(gate_prefix)?;
    rest.split('.').next().and_then(|s| s.parse::<Stage>().ok())
}

/// The phase's gate files, sorted, as [`phase_gate_file_stage`] names them; removes
/// nothing.
fn phase_gate_files(project_root: &Path, phase: PhaseId) -> Result<Vec<PathBuf>, CliError> {
    let gates = Gates::dir(project_root);
    let gate_prefix = format!("{}-", phase.padded());
    let mut files: Vec<PathBuf> = file_names(&gates)?
        .into_iter()
        .filter(|name| phase_gate_file_stage(name, &gate_prefix).is_some())
        .map(|name| gates.join(name))
        .collect();
    files.sort();
    Ok(files)
}

/// The final name a hold temp `.{final_name}.{pid}.{seq}.hold-tmp` is taken for;
/// `None` for any other name.
fn hold_temp_final_name(name: &str) -> Option<&str> {
    let mut fields = name
        .strip_suffix(liveness::HOLD_TEMP_SUFFIX)?
        .rsplitn(3, '.');
    fields.next()?;
    fields.next()?;
    fields.next()?.strip_prefix('.')
}

/// Refuse discard when the waiter registration at `path` is held, or cannot be probed.
fn refuse_discard_of_a_live_waiter(
    phase: PhaseId,
    stage: Stage,
    path: &Path,
) -> Result<(), CliError> {
    let why = match liveness::probe(path) {
        Ok(liveness::Held::Live) => "a live waiter".to_string(),
        Ok(liveness::Held::Abandoned | liveness::Held::Missing) => return Ok(()),
        Err(error) => format!("a waiter registration that could not be probed ({error})"),
    };
    Err(CliError::Message(format!(
        "refusing to discard phase {phase} — its {stage} gate has {why} ({}), so something \
         may still wait on it; run `devflow status` to see the run and end it, then run \
         `devflow discard --phase {phase}` again; nothing was removed",
        path.display()
    )))
}

/// Remove what [`discard_debris`] found: each stage's gate files through
/// [`Gates::close`], then the hold temps whose creator died through
/// [`liveness::remove_orphan_hold_temps`]. A `removed` line for each file gone, a
/// `warning:` line for each failure, and an error when any failed. A hold temp whose
/// creator may still run is kept and named; that is not a failure.
fn remove_discarded_debris(
    project_root: &Path,
    phase: PhaseId,
    debris: &DiscardDebris,
) -> Result<(), CliError> {
    let mut failed = false;
    for stage in &debris.gate_stages {
        match Gates::close(project_root, phase, *stage) {
            Ok(report) => {
                for moved in report.quarantined {
                    println!(
                        "set aside an unclaimed answer of phase {phase} {stage} as {}; \
                         set-aside answers are kept",
                        moved.display()
                    );
                }
            }
            Err(err) => {
                failed = true;
                println!(
                    "warning: could not remove the {stage} gate files of phase {phase}: {err}"
                );
            }
        }
    }
    for path in &debris.gate_files {
        if path.exists() {
            failed = true;
            println!("warning: the gate file {} is still there", path.display());
        } else {
            println!("removed gate file {}", path.display());
        }
    }
    if !debris.hold_temps.is_empty() {
        for dir in [
            workflow::devflow_dir(project_root),
            Gates::dir(project_root),
        ] {
            match liveness::remove_orphan_hold_temps(&dir) {
                Ok(removed) => {
                    for path in removed {
                        println!("removed hold temp {}", path.display());
                    }
                }
                Err(err) => {
                    failed = true;
                    println!(
                        "warning: could not sweep the hold temps in {}: {err}",
                        dir.display()
                    );
                }
            }
        }
    }
    for path in debris.hold_temps.iter().filter(|path| path.exists()) {
        println!(
            "kept hold temp {}: its creator may still run or it is held",
            path.display()
        );
    }
    if failed {
        return Err(CliError::Message(format!(
            "discard could not remove every gate file of phase {phase} — see the warnings above"
        )));
    }
    Ok(())
}

/// D-07a: a unit that is not yet inactive while the runner lock is free is waited on,
/// under discard's phase holder, for [`crate::service::UNIT_STOP_WAIT`]; then the
/// liveness is read again and that answer is acted on. A wait that fails refuses
/// naming the unit and its last state, and so does a unit that reads stopping again.
fn discard_waits_out_a_stopping_unit(
    project_root: &Path,
    phase: PhaseId,
    liveness: PhaseRunLiveness,
) -> Result<PhaseRunLiveness, CliError> {
    let PhaseRunLiveness::RunnerStopping { unit, .. } = liveness else {
        return Ok(liveness);
    };
    let refusal = |why: String| {
        CliError::Message(format!(
            "refusing to discard phase {phase} — {why}; run `devflow stop --phase {phase}` or \
             wait for it to finish; nothing was removed"
        ))
    };
    crate::service::wait_unit_inactive(&unit, crate::service::UNIT_STOP_WAIT).map_err(refusal)?;
    match phase_run_liveness(project_root, phase) {
        PhaseRunLiveness::RunnerStopping { unit, state } => Err(refusal(format!(
            "unit {unit} reads {state} again after it read inactive"
        ))),
        again => Ok(again),
    }
}

/// The base discard counts "merged" against (D-10), and a [`GitFlow`] whose trunk is
/// that base: the base the run recorded when its state loads and records one, else
/// the project's.
fn discard_base(project_root: &Path, existing: &workflow::Existing) -> (String, GitFlow) {
    let flow = config::git_flow_for_project(project_root);
    let recorded = match existing {
        workflow::Existing::Loaded(state) => state.base_branch.clone(),
        workflow::Existing::Missing | workflow::Existing::Unloadable(_) => None,
    };
    let base = recorded.unwrap_or_else(|| flow.develop.clone());
    let git = GitFlow::with_config(
        project_root,
        config::GitFlowConfig {
            develop: base.clone(),
            ..flow
        },
    );
    (base, git)
}

/// Whether a phase's run is live, may be live, or is not, for `discard`, which removes
/// a run's worktree and state: [`runner_liveness`]'s answer (D-07, D-12a). It refuses
/// on every arm but [`Self::NotLive`] and waits on [`Self::RunnerStopping`].
enum PhaseRunLiveness {
    /// Nothing says a run is live.
    NotLive,
    /// D-07, 999.237: the phase's runner lock is held, whatever the state names.
    Runner {
        /// The runner's recorded pid, `None` when its record does not read.
        pid: Option<u32>,
    },
    /// D-07a: the runner lock is free and the phase's unit is not yet inactive. Not a
    /// refusal: `discard` waits on it.
    RunnerStopping {
        /// The phase's unit.
        unit: String,
        /// Its `ActiveState`.
        state: String,
    },
    /// The runner lock or the unit could not be read, so a live runner cannot be
    /// ruled out: [`runner_liveness`]'s text.
    RunnerUnreadable(String),
}

/// The live-run test of `discard`: [`runner_liveness`], read whatever the state names
/// (D-07, 999.237). A held runner lock is live and a read error refuses; a unit that is
/// not yet inactive is [`PhaseRunLiveness::RunnerStopping`].
fn phase_run_liveness(project_root: &Path, phase: PhaseId) -> PhaseRunLiveness {
    match runner_liveness(project_root, phase) {
        Err(why) => PhaseRunLiveness::RunnerUnreadable(why),
        Ok(RunnerLiveness::Live { pid }) => PhaseRunLiveness::Runner { pid },
        Ok(RunnerLiveness::Stopping { unit, state }) => {
            PhaseRunLiveness::RunnerStopping { unit, state }
        }
        Ok(RunnerLiveness::NotLive) => PhaseRunLiveness::NotLive,
    }
}

/// D-07: what the phase's runner says about a run. One runner lock answers for every
/// launch, so it is read whatever the state names (999.237).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RunnerLiveness {
    /// The runner lock is held: a runner is alive.
    Live {
        /// The runner's recorded pid, `None` when its record does not read.
        pid: Option<u32>,
    },
    /// The lock is free and the phase's unit is neither `inactive` nor `failed`: a
    /// runner that has let go of its lock and is still exiting (D-07a).
    Stopping {
        /// The phase's unit.
        unit: String,
        /// Its `ActiveState`.
        state: String,
    },
    /// The lock is free and the unit is gone.
    NotLive,
}

/// D-07: read [`liveness::runner_lock_path`] first, then, with the lock free, whether
/// the user manager answers ([`crate::service::manager_available`]), then the phase's
/// unit. An Absent manager prints the runner-lock-only note and reads no unit;
/// Unreachable and Failed are errors. Only [`RunnerLiveness::Live`] is a live-run refusal (D-12 as
/// corrected): a stopping unit is waited on, never refused before the wait.
///
/// # Errors
/// The lock could not be probed, or the unit's state could not be read: text naming
/// what failed, systemctl's stderr included. Never read as free or inactive.
pub(crate) fn runner_liveness(
    project_root: &Path,
    phase: PhaseId,
) -> Result<RunnerLiveness, String> {
    let lock = liveness::runner_lock_path(project_root, phase);
    match liveness::probe(&lock) {
        Err(error) => {
            return Err(format!(
                "its runner lock {} could not be probed ({error})",
                lock.display()
            ));
        }
        Ok(liveness::Held::Live) => {
            let pid = liveness::live_holder(&lock)
                .ok()
                .flatten()
                .map(|(pid, _)| pid);
            return Ok(RunnerLiveness::Live { pid });
        }
        Ok(liveness::Held::Abandoned | liveness::Held::Missing) => {}
    }
    // D-03, agy-2: with no manager there is no unit to read; only positive evidence
    // of that (Absent) lets the lock answer alone, and the verb says so.
    match crate::service::manager_available() {
        Ok(_) => {}
        Err(unavailable) if unavailable.kind == crate::service::ManagerUnavailableKind::Absent => {
            eprintln!("{}", unavailable.describe(phase));
            return Ok(RunnerLiveness::NotLive);
        }
        Err(unavailable) => return Err(unavailable.describe(phase)),
    }
    let unit = crate::service::unit_name(project_root, phase)
        .map_err(|error| format!("its unit name could not be derived ({error})"))?;
    let state = crate::service::unit_active_state(&unit, crate::service::SYSTEMCTL_CALL_BOUND)
        .map_err(|why| format!("the systemd user manager could not read unit {unit}: {why}"))?;
    Ok(match state.as_str() {
        "inactive" | "failed" => RunnerLiveness::NotLive,
        _ => RunnerLiveness::Stopping { unit, state },
    })
}

/// ` (pid N)` for a runner whose record reads, else nothing.
fn runner_pid_note(pid: Option<u32>) -> String {
    pid.map(|pid| format!(" (pid {pid})")).unwrap_or_default()
}

/// D-09: a phase's run as `status` and `logs -f` name it, one word and the verb that
/// applies, from [`runner_liveness`] first and only then the state (RESEARCH "Guard
/// Sequence and Status Classification").
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunStatus {
    /// The runner lock is held, whatever the state says.
    Live {
        /// The runner's recorded pid, `None` when its record does not read.
        pid: Option<u32>,
        /// The state's stage, `None` when the state is missing or does not load.
        stage: Option<Stage>,
        /// Whether the state records a gate the runner is waiting on.
        gate_pending: bool,
    },
    /// The lock is free and the unit is not yet inactive (D-07a).
    Stopping {
        /// The phase's unit.
        unit: String,
        /// Its `ActiveState`.
        state: String,
    },
    /// The run finished.
    Finished,
    /// The run was stopped; `resume` is the verb that continues it.
    Stopped {
        /// The resume line for this run.
        resume: String,
    },
    /// The runner is gone and the run neither finished nor stopped.
    Crashed {
        /// The stage the state names.
        stage: Stage,
        /// Whether the state records a pending gate nothing waits on now.
        gate_pending: bool,
        /// The resume line for this run.
        resume: String,
    },
    /// D-20: the state loads and [`run_never_prepared`] holds, read once the run is not
    /// live: a reservation whose `start` never prepared the run.
    Idle {
        /// The `start` that prepares it.
        start: String,
    },
    /// D-09a: no state, and gate files remain.
    Debris,
    /// The state file does not load.
    Unloadable(String),
    /// [`runner_liveness`] could not answer.
    Error(String),
}

impl RunStatus {
    /// The one word `status` names the run with.
    pub(crate) fn word(&self) -> &'static str {
        match self {
            RunStatus::Live { .. } => "live",
            RunStatus::Stopping { .. } => "live (stopping)",
            RunStatus::Finished => "finished",
            RunStatus::Stopped { .. } => "stopped",
            RunStatus::Crashed { .. } => "crashed",
            RunStatus::Idle { .. } => "idle",
            RunStatus::Debris => "debris",
            RunStatus::Unloadable(_) => "unloadable",
            RunStatus::Error(_) => "error",
        }
    }

    /// The word, what it rests on, and the verb that applies, as one line.
    pub(crate) fn line(&self, phase: PhaseId) -> String {
        let word = self.word();
        let discard = format!("`devflow discard --phase {phase}`");
        let stop = format!("`devflow stop --phase {phase}`");
        match self {
            RunStatus::Live {
                pid,
                stage,
                gate_pending: true,
            } => format!(
                "{word}{runner}, waiting at its {stage} gate — answer it with `devflow gate \
                 approve {phase}` or `devflow gate reject {phase} --note <reason>`; end it with \
                 {stop}",
                runner = runner_note(*pid),
                stage = stage.map_or_else(|| "pending".to_string(), |stage| stage.to_string()),
            ),
            RunStatus::Live { pid, stage, .. } => format!(
                "{word}{runner}{at} — watch it with `devflow logs --phase {phase} -f`; end it \
                 with {stop}",
                runner = runner_note(*pid),
                at = stage
                    .map(|stage| format!(" in {stage}"))
                    .unwrap_or_default(),
            ),
            RunStatus::Stopping { unit, state } => format!(
                "{word} — its runner let go of its lock and unit {unit} reads {state}; wait for \
                 it to stop, or run {stop}"
            ),
            RunStatus::Finished => {
                format!("{word} — close the phase out, then remove its run with {discard}")
            }
            RunStatus::Stopped { resume } => {
                format!("{word} — continue it with `{resume}`, or remove it with {discard}")
            }
            RunStatus::Crashed {
                stage,
                gate_pending,
                resume,
            } => format!(
                "{word} — its runner is gone at {stage}{gate} and nothing advances it; continue \
                 it with `{resume}`, or remove it with {discard}",
                gate = if *gate_pending {
                    " with a gate pending"
                } else {
                    ""
                },
            ),
            RunStatus::Idle { start } => {
                format!("{word} — its `devflow start` never prepared the run; run `{start}`")
            }
            RunStatus::Debris => format!(
                "{word} — gate files with no state and nothing waiting on them; remove them with \
                 {discard}"
            ),
            RunStatus::Unloadable(why) => {
                format!("{word} — its state file does not load ({why}); remove it with {discard}")
            }
            RunStatus::Error(why) => {
                format!("{word} — whether a run is live could not be read: {why}")
            }
        }
    }
}

/// ` (runner pid N)` for a runner whose record reads, else ` (runner)`.
fn runner_note(pid: Option<u32>) -> String {
    pid.map_or_else(
        || " (runner)".to_string(),
        |pid| format!(" (runner pid {pid})"),
    )
}

/// The verb that continues a stopped or crashed run: `resume --agent <other>` while
/// its recorded delivery canary refuses the agent, as `resume` judges it
/// ([`recorded_canary_refusal`](crate::pipeline_launch::recorded_canary_refusal)), since
/// only a handoff clears that canary.
fn resume_verb(state: &State) -> String {
    let phase = state.phase;
    if crate::pipeline_launch::recorded_canary_refusal(state).is_some() {
        format!("devflow resume --phase {phase} --agent <other>")
    } else {
        format!("devflow resume --phase {phase}")
    }
}

/// D-09, D-20: the phase's [`RunStatus`] from [`runner_liveness`], then `existing`.
/// `None` when nothing says there is or was a run: no state, no gate files, the lock
/// free and the unit inactive.
pub(crate) fn run_status(
    project_root: &Path,
    phase: PhaseId,
    existing: &workflow::Existing,
) -> Option<RunStatus> {
    let loaded = match existing {
        workflow::Existing::Loaded(state) => Some(state),
        _ => None,
    };
    match runner_liveness(project_root, phase) {
        Err(why) => return Some(RunStatus::Error(why)),
        Ok(RunnerLiveness::Live { pid }) => {
            return Some(RunStatus::Live {
                pid,
                stage: loaded.map(|state| state.stage),
                gate_pending: loaded.is_some_and(|state| state.gate_pending),
            });
        }
        Ok(RunnerLiveness::Stopping { unit, state }) => {
            return Some(RunStatus::Stopping { unit, state });
        }
        Ok(RunnerLiveness::NotLive) => {}
    }
    Some(match existing {
        workflow::Existing::Missing => {
            if !Gates::phases_on_disk(project_root).contains(&phase) {
                return None;
            }
            RunStatus::Debris
        }
        workflow::Existing::Unloadable(error) => RunStatus::Unloadable(error.to_string()),
        workflow::Existing::Loaded(state) if run_never_prepared(state) => RunStatus::Idle {
            start: restart_verb(state),
        },
        workflow::Existing::Loaded(state) if state.finished => RunStatus::Finished,
        workflow::Existing::Loaded(state) if state.stopped => RunStatus::Stopped {
            resume: resume_verb(state),
        },
        workflow::Existing::Loaded(state) => RunStatus::Crashed {
            stage: state.stage,
            gate_pending: state.gate_pending,
            resume: resume_verb(state),
        },
    })
}

/// The phase's state as it is on disk now, read without the write lock: `status` and
/// `logs` only read it.
fn existing_state(project_root: &Path, phase: PhaseId) -> workflow::Existing {
    if !workflow::state_path(project_root, phase).exists() {
        return workflow::Existing::Missing;
    }
    match workflow::load_state(project_root, phase) {
        Ok(state) => workflow::Existing::Loaded(state),
        Err(error) => workflow::Existing::Unloadable(error),
    }
}

/// D-09.2: discard refuses a run that is live or may be, on every arm of
/// [`PhaseRunLiveness`] but `NotLive`, naming `devflow stop`. Nothing is removed.
fn refuse_discard_of_a_live_run(
    phase: PhaseId,
    liveness: PhaseRunLiveness,
) -> Result<(), CliError> {
    let text = match liveness {
        PhaseRunLiveness::NotLive | PhaseRunLiveness::RunnerStopping { .. } => return Ok(()),
        PhaseRunLiveness::Runner { pid } => format!(
            "refusing to discard phase {phase} — its runner holds the runner lock{}; run \
             `devflow stop --phase {phase}` or wait for it to finish; nothing was removed",
            runner_pid_note(pid)
        ),
        PhaseRunLiveness::RunnerUnreadable(why) => format!(
            "refusing to discard phase {phase} — {why}, so a live runner cannot be ruled out; \
             nothing was removed"
        ),
    };
    Err(CliError::Message(text))
}

/// What `devflow discard` found for a phase: the three things it removes, each present
/// only when it exists, so a partial phase is discarded item by item.
struct DiscardItems {
    /// The worktree git lists at `.worktrees/phase-NN`, with its count of uncommitted
    /// changes.
    worktree: Option<(PathBuf, usize)>,
    /// `.devflow/state-NN.json`, when the file exists, whether or not it loads.
    state: Option<PathBuf>,
    /// The derived `feature/phase-NN`.
    branch: String,
    /// The branch's count of commits not merged into `base`. `None` when the branch
    /// does not exist.
    unmerged: Option<usize>,
    /// The branch `unmerged` is counted against: the base the run recorded, else the
    /// project's.
    base: String,
    /// A checkout other than the phase worktree that has `branch` checked out, and
    /// whether it is the primary checkout. git deletes no branch a checkout holds.
    checked_out_elsewhere: Option<(PathBuf, bool)>,
}

/// Read what discard would remove for `phase`, removing nothing. `git`'s trunk is
/// `base`. Refuses whatever it cannot account for: see [`discard_worktree`], and a
/// branch whose merge state does not read, which is never assumed merged.
fn discard_items(
    project_root: &Path,
    phase: PhaseId,
    existing: &workflow::Existing,
    git: &GitFlow,
    base: String,
) -> Result<DiscardItems, CliError> {
    let branch = format!("{FEATURE_PREFIX}phase-{padded}", padded = phase.padded());
    let worktrees = worktree::list(project_root).map_err(|err| {
        CliError::Message(format!(
            "discard refused: this repository's worktrees could not be listed ({err}); \
             nothing was removed"
        ))
    })?;
    let phase_path = worktree::phase_path(project_root, phase);
    let worktree = discard_worktree(&worktrees, phase_path.clone(), phase, &branch)?;
    // git lists the primary checkout first.
    let checked_out_elsewhere = worktrees
        .iter()
        .enumerate()
        .find(|(_, wt)| {
            wt.branch.as_deref() == Some(branch.as_str()) && !same_path(&wt.path, &phase_path)
        })
        .map(|(index, wt)| (wt.path.clone(), index == 0));
    let state = (!matches!(existing, workflow::Existing::Missing))
        .then(|| workflow::state_path(project_root, phase));
    let unmerged = if git.branch_exists(&branch) {
        Some(git.commits_not_in_develop(&branch).map_err(|err| {
            CliError::Message(format!(
                "discard refused: phase {phase}'s branch {branch} could not be checked for \
                 commits not merged into `{base}` ({err}); nothing was removed. Make `{base}` \
                 resolve, then run `devflow discard --phase {phase}` again"
            ))
        })?)
    } else {
        None
    };
    Ok(DiscardItems {
        worktree,
        state,
        branch,
        unmerged,
        base,
        checked_out_elsewhere,
    })
}

/// The phase worktree discard may remove, with its count of uncommitted changes: the
/// one of `worktrees` (git's list) at `path`, the derived `.worktrees/phase-NN`, with
/// `branch` checked out. `None` when nothing is at that path.
///
/// Refuses what it cannot account for: something at that path that git does not list
/// as a worktree; a worktree on another branch or a detached HEAD, which may hold
/// commits no branch keeps and the listing would not show; and a worktree whose status
/// does not read.
fn discard_worktree(
    worktrees: &[worktree::WorktreeInfo],
    path: PathBuf,
    phase: PhaseId,
    branch: &str,
) -> Result<Option<(PathBuf, usize)>, CliError> {
    let again = format!("then run `devflow discard --phase {phase}` again");
    let Some(listed) = worktrees.iter().find(|wt| same_path(&wt.path, &path)) else {
        if path.symlink_metadata().is_ok() {
            return Err(CliError::Message(format!(
                "discard refused: {} exists but git does not list it as a worktree, so discard \
                 cannot tell what is in it; nothing was removed. Inspect it and remove it \
                 yourself, {again}",
                path.display()
            )));
        }
        return Ok(None);
    };
    if listed.branch.as_deref() != Some(branch) {
        let found = listed.branch.as_deref().map_or_else(
            || "a detached HEAD".to_string(),
            |other| format!("branch {other}"),
        );
        return Err(CliError::Message(format!(
            "discard refused: the worktree {} has {found} checked out, not {branch}, so discard \
             cannot tell what removing it would lose; nothing was removed. Inspect it and remove \
             it yourself, {again}",
            path.display()
        )));
    }
    let uncommitted = uncommitted_changes(&path).map_err(|why| {
        CliError::Message(format!(
            "discard refused: the uncommitted changes in the worktree {} could not be counted \
             ({why}); nothing was removed. If its directory is gone, run `git worktree prune`, \
             {again}",
            path.display()
        ))
    })?;
    Ok(Some((path, uncommitted)))
}

/// Whether two paths name the same place: their canonical forms are compared, and a
/// path that does not resolve is compared as written.
fn same_path(left: &Path, right: &Path) -> bool {
    let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical(left) == canonical(right)
}

/// Discard's listing of what it will remove (D-09.2), printed before any removal.
fn print_discard_listing(phase: PhaseId, items: &DiscardItems, debris: &DiscardDebris) {
    println!("discard: phase {phase} — to be removed:");
    if let Some((path, uncommitted)) = &items.worktree {
        let changes = match uncommitted {
            1 => "1 uncommitted change".to_string(),
            count => format!("{count} uncommitted changes"),
        };
        println!(
            "  worktree {}, with everything in it: {changes} (ignored files are removed too \
             and are not counted)",
            path.display()
        );
    }
    if let Some(path) = &items.state {
        println!("  run state {}", path.display());
    }
    if let Some(unmerged) = items.unmerged {
        let merged = match unmerged {
            0 => format!("merged into `{}`", items.base),
            count => unmerged_commits(count, &items.base),
        };
        println!("  branch {} ({merged})", items.branch);
    }
    for path in &debris.gate_files {
        println!("  gate file {}", path.display());
    }
    for path in &debris.hold_temps {
        println!(
            "  hold temp {} (kept if its creator still runs)",
            path.display()
        );
    }
}

/// D-09.2: a phase branch that a checkout other than the phase worktree holds is not
/// discard's to delete, and git would refuse it after the worktree and the state were
/// gone. For the primary checkout (`start --no-worktree`) the refusal names the branch
/// to switch to; discard never switches a branch itself.
fn refuse_discard_of_a_checked_out_branch(
    phase: PhaseId,
    items: &DiscardItems,
) -> Result<(), CliError> {
    let Some((path, primary)) = &items.checked_out_elsewhere else {
        return Ok(());
    };
    let (branch, base, path) = (&items.branch, &items.base, path.display());
    let again = format!("then run `devflow discard --phase {phase}` again");
    Err(CliError::Message(if *primary {
        format!(
            "discard refused: phase {phase}'s branch {branch} is checked out in the primary \
             checkout {path}; nothing was removed. Run `git switch {base}` there, {again}"
        )
    } else {
        format!(
            "discard refused: phase {phase}'s branch {branch} is checked out in the worktree \
             {path}, and git deletes no branch a worktree has checked out; nothing was removed. \
             Switch that worktree to another branch or remove it, {again}"
        )
    }))
}

/// D-10 / D-23: no unmerged branch is deleted unless `--delete-unmerged-branch` names
/// it, and the name must be the phase's own branch whatever its merge state.
fn refuse_discard_of_branch(
    phase: PhaseId,
    items: &DiscardItems,
    named: Option<&str>,
) -> Result<(), CliError> {
    let branch = &items.branch;
    if let Some(named) = named
        && named != branch
    {
        return Err(CliError::Message(format!(
            "discard refused: --delete-unmerged-branch names {named}, and phase {phase}'s branch \
             is {branch}; nothing was removed. Name {branch}, or leave the flag out"
        )));
    }
    match items.unmerged {
        Some(count) if count > 0 && named.is_none() => Err(CliError::Message(format!(
            "discard refused: phase {phase}'s branch {branch} has {}; nothing was removed. To \
             delete it with the rest, run `devflow discard --phase {phase} \
             --delete-unmerged-branch {branch}`",
            unmerged_commits(count, &items.base)
        ))),
        _ => Ok(()),
    }
}

/// Remove what [`discard_items`] found, in the order worktree, state, branch: a
/// `removed` line for each, a `warning:` line for each that failed, and an error when
/// any did. An item that was not found is not attempted.
fn remove_discarded(
    project_root: &Path,
    phase: PhaseId,
    git: &GitFlow,
    items: &DiscardItems,
) -> Result<(), CliError> {
    let mut failed = false;
    if let Some((path, _)) = &items.worktree {
        match worktree::remove(project_root, path, true) {
            Ok(()) => println!("removed worktree {}", path.display()),
            Err(err) => {
                failed = true;
                println!(
                    "warning: could not remove the worktree {}: {err}",
                    path.display()
                );
            }
        }
    }
    if let Some(path) = &items.state {
        match workflow::clear_state(project_root, phase) {
            Ok(true) => println!("removed run state {}", path.display()),
            // Gone since it was listed: nothing to remove, and not a failure.
            Ok(false) => {}
            Err(err) => {
                failed = true;
                println!(
                    "warning: could not remove the run state {}: {err}",
                    path.display()
                );
            }
        }
    }
    if items.unmerged.is_some() {
        match git.delete_branch(&items.branch, true) {
            Ok(()) => println!("removed branch {}", items.branch),
            Err(err) => {
                failed = true;
                println!(
                    "warning: could not remove the branch {}: {err}",
                    items.branch
                );
            }
        }
    }
    if failed {
        return Err(CliError::Message(format!(
            "discard could not remove everything for phase {phase} — see the warnings above"
        )));
    }
    Ok(())
}

/// How many uncommitted changes the worktree at `path` holds: the lines of
/// `git status --porcelain --untracked-files=all`. Ignored files are not counted.
fn uncommitted_changes(path: &Path) -> Result<usize, String> {
    let output = git_command(path)
        .args(["status", "--porcelain", "--untracked-files=all"])
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).lines().count())
}

/// The age of a unix-seconds timestamp, in seconds; `None` when it does not parse
/// or lies in the future.
fn state_age_secs(started_at: &str) -> Option<u64> {
    let started: u64 = started_at.parse().ok()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now.checked_sub(started)
}

/// Format a unix-seconds timestamp's age as a human-readable string ("5m ago"), or
/// "unknown" when it does not parse. `status` uses it for elapsed time and event
/// recency, and the gate lines for a gate's age.
fn format_age(started_at: &str) -> String {
    match state_age_secs(started_at) {
        Some(s) if s < 60 => format!("{s}s ago"),
        Some(s) if s < 3600 => format!("{}m ago", s / 60),
        Some(s) if s < 86400 => format!("{}h ago", s / 3600),
        Some(s) => format!("{}d ago", s / 86400),
        None => "unknown".into(),
    }
}

/// `status`'s in-stage progress line: real elapsed time since the phase's
/// most recent `stage_launched` event. `None` (no such event yet) renders
/// the stage name with no age, rather than mislabeling phase age as stage
/// age — never pass `state.started_at`'s age here (3/3 review MEDIUM).
fn render_stage_progress_line(stage: Stage, stage_launched_ts: Option<u64>) -> String {
    match stage_launched_ts {
        Some(ts) => format!("  in stage {stage}: {}", format_age(&ts.to_string())),
        None => format!("  in stage {stage}"),
    }
}

/// How to judge one reading of the state write lock, which `status` and `doctor` take
/// once and do not time. The bounds are the holds D-08 allows (R2-12).
const WRITE_LOCK_HOLD_NOTE: &str = "a state write usually takes milliseconds, but a stalled gate \
     lock or a socket dying mid-ping can stretch it to about 10 s in a gate answer and 16 s in a \
     reservation; a hold past 20 s is the fault";

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

/// `status`'s write-lock line for `phase`: [`write_lock_line`], or the read error.
fn status_write_lock_line(project_root: &Path, phase: PhaseId) -> Option<String> {
    match workflow::write_lock_status(project_root, phase) {
        Ok(status) => write_lock_line(&status),
        Err(error) => Some(format!("  state write lock: could not be read ({error})")),
    }
}

/// The run's worktree when `state` records one that no longer exists on disk.
fn missing_worktree(state: &State) -> Option<&Path> {
    state.worktree_path.as_deref().filter(|path| !path.exists())
}

/// What a phase's run is, as `status` names it (OPS-02, D-35). A run is active
/// only while it has not stopped and its recorded worktree still exists: a gone
/// worktree leaves nothing for an agent to work in, however live the supervisor
/// record or agent pid looks.
fn run_kind(state: &State) -> &'static str {
    if state.finished {
        "finished"
    } else if state.stopped {
        "stopped"
    } else if missing_worktree(state).is_some() {
        "worktree missing"
    } else {
        "active"
    }
}

/// `status`'s header: the phases whose runs are active (or `none`), then every
/// other phase with what it is, so the header never calls a finished, stopped or
/// worktree-less run active.
fn active_phases_line(states: &[State]) -> String {
    let (active, others): (Vec<&State>, Vec<&State>) =
        states.iter().partition(|s| run_kind(s) == "active");
    let active = if active.is_empty() {
        "none".to_string()
    } else {
        active
            .iter()
            .map(|s| s.phase.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut line = format!("active phases: {active}");
    if !others.is_empty() {
        let others = others
            .iter()
            .map(|s| format!("{} ({})", s.phase, run_kind(s)))
            .collect::<Vec<_>>()
            .join(", ");
        line.push_str(&format!("\nother phases: {others}"));
    }
    line
}

/// The fallback agent's state (OPS-04): exactly one of `armed` (configured and
/// not used), `spent` (`fallback_consumed`) or `none` (no fallback configured).
fn fallback_line(state: &State) -> String {
    match state.fallback_agent {
        Some(agent) if state.fallback_consumed => {
            format!("  fallback: spent ({agent} already took over this run)")
        }
        Some(agent) => format!("  fallback: armed ({agent} takes over if Code is rate-limited)"),
        None => "  fallback: none".to_string(),
    }
}

/// `status`'s lines about what a phase's run did and what is left (OPS-02,
/// OPS-04): its worktree (marked when it is gone), what the run is, a stopped
/// run's recorded stop reason verbatim, and the fallback agent's state.
fn status_run_lines(state: &State) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(wt) = &state.worktree_path {
        let gone = if missing_worktree(state).is_some() {
            " (missing)"
        } else {
            ""
        };
        lines.push(format!("  worktree: {}{gone}", wt.display()));
    }
    lines.push(match (run_kind(state), missing_worktree(state)) {
        ("worktree missing", Some(path)) => format!(
            "  run: not active — its worktree {} no longer exists",
            path.display()
        ),
        (kind, _) => format!("  run: {kind}"),
    });
    if state.stopped {
        lines.push(format!(
            "  stop reason: {}",
            state.stop_reason.as_deref().unwrap_or("none recorded")
        ));
    }
    lines.push(fallback_line(state));
    lines
}

/// D-09 (review codex-8): every phase `status` lists, found by file name only, never
/// by file contents: `.runner-NN` locks in `.devflow/`, gate request files
/// `NN-{stage}.json` in [`Gates::dir`] and state files through
/// [`workflow::state_file_phases`], whether or not they load. A missing directory is an
/// empty set; any other read error is returned, never read as empty.
fn status_phases(project_root: &Path) -> Result<BTreeSet<PhaseId>, CliError> {
    let mut phases = BTreeSet::new();
    for name in file_names(&workflow::devflow_dir(project_root))? {
        if let Some(phase) = name
            .strip_prefix(".runner-")
            .and_then(|phase| phase.parse::<PhaseId>().ok())
        {
            phases.insert(phase);
        }
    }
    phases.extend(workflow::state_file_phases(project_root));
    for name in file_names(&Gates::dir(project_root))? {
        if let Some(phase) = gate_request_phase(&name) {
            phases.insert(phase);
        }
    }
    Ok(phases)
}

/// The phase a gate request file `NN-{stage}.json` names; `None` for any other name,
/// such as a protocol artifact with more after the stage ([`Gates::list_open`]'s rule).
fn gate_request_phase(name: &str) -> Option<PhaseId> {
    let (phase, stage) = name.strip_suffix(".json")?.split_once('-')?;
    if stage.contains('.') || stage.parse::<Stage>().is_err() {
        return None;
    }
    phase.parse().ok()
}

/// The names of the entries in `dir`: empty when it does not exist, an error naming
/// it for any other read failure.
fn file_names(dir: &Path) -> Result<Vec<String>, CliError> {
    let unreadable = |error: std::io::Error| {
        CliError::Message(format!("could not read {}: {error}", dir.display()))
    };
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(unreadable(error)),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(unreadable)?;
        if let Some(name) = entry.file_name().to_str() {
            names.push(name.to_string());
        }
    }
    Ok(names)
}

pub(crate) fn status(project_root: &Path) -> Result<(), CliError> {
    // D-09: every phase found from its files, each with its liveness row. A phase found
    // only through a free runner lock is history and has no row.
    let rows: Vec<(PhaseId, workflow::Existing, RunStatus)> = status_phases(project_root)?
        .into_iter()
        .filter_map(|phase| {
            let existing = existing_state(project_root, phase);
            run_status(project_root, phase, &existing).map(|row| (phase, existing, row))
        })
        .collect();
    let mut current_worktree: Option<PathBuf> = None;
    if rows.is_empty() {
        println!("stage: idle");
        println!("project_root: {}", project_root.display());
    } else {
        let states: Vec<State> = rows
            .iter()
            .filter_map(|(_, existing, _)| match existing {
                workflow::Existing::Loaded(state) => Some(state.clone()),
                _ => None,
            })
            .collect();
        // 14-CR-10: one pass over events.jsonl for every phase's last event,
        // instead of a full-file scan per phase.
        let mut last_events = events::last_events_by_phase(project_root);
        println!("project_root: {}", project_root.display());
        println!("{}", active_phases_line(&states));
        for (phase, _, row) in &rows {
            println!("\nphase {phase}: {}", row.line(*phase));
            if let Some(state) = states.iter().find(|state| state.phase == *phase) {
                for line in status_state_lines(project_root, state, last_events.remove(phase)) {
                    println!("{line}");
                }
                current_worktree = current_worktree.or_else(|| state.worktree_path.clone());
            }
        }
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let open = Gates::list_open(project_root);
    if let Some(banner) = render_pending_gate_banner(project_root, &open, now) {
        println!("\n{banner}");
    }
    if let Some(block) = render_set_aside_answers(&Gates::quarantined(project_root)) {
        println!("\n{block}");
    }
    print_open_branches(project_root);
    print_worktrees(project_root, current_worktree.as_deref());
    Ok(())
}

/// `status`'s lines about a loaded state under its row: what the run is recorded as,
/// the write lock, the stage's progress and the last action.
fn status_state_lines(
    project_root: &Path,
    state: &State,
    summary: Option<events::PhaseEventSummary>,
) -> Vec<String> {
    let gate = if state.gate_pending {
        "pending"
    } else {
        "none"
    };
    let mut lines = vec![
        format!(
            "  stage: {} | mode: {} | gate: {gate}",
            state.stage, state.mode
        ),
        format!("  agent: {}", agents::driver_for(state.agent).name()),
    ];
    if state.consecutive_failures > 0 {
        lines.push(format!(
            "  validate failures: {}",
            state.consecutive_failures
        ));
    }
    lines.push(format!(
        "  started: {} ({})",
        state.started_at,
        format_age(&state.started_at)
    ));
    lines.extend(status_run_lines(state));
    lines.extend(status_write_lock_line(project_root, state.phase));
    lines.push(render_stage_progress_line(
        state.stage,
        summary.as_ref().and_then(|s| s.stage_launched_ts),
    ));
    if let Some(summary) = summary {
        let ago = summary
            .event
            .get("ts")
            .and_then(|t| t.as_u64())
            .map(|t| format!(" ({})", format_age(&t.to_string())))
            .unwrap_or_default();
        lines.push(format!(
            "  last action: {}{ago}",
            events::describe(&summary.event)
        ));
    }
    lines
}

/// The characters of a gate's context the pending-gate banner, `devflow gate
/// list` and the notify hook show; `gate show` prints the context in full.
pub(crate) use devflow_core::gates::GATE_CONTEXT_DISPLAY_CHARS;

/// Build the persistent status-side signal for gates awaiting an operator.
/// Context is agent-controlled, so it must use the same bounded rendering as
/// gate notifications and failure events.
///
/// D-09a (review pi-9): a gate is offered approve and reject only while its waiter
/// registration reads [`WaiterStatus::Live`] ([`Gates::waiter_status`]). A gate nothing
/// waits on is debris naming `devflow discard`; a probe error offers no answer.
fn render_pending_gate_banner(project_root: &Path, open: &[OpenGate], now: u64) -> Option<String> {
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
        let context = render_gate_context(&gate.context, GATE_CONTEXT_DISPLAY_CHARS);
        let stage = gate.stage.to_string();
        banner.push_str(&format!(
            "{marker}: phase {} {stage} ({})\n  {context}\n{}",
            gate.phase,
            format_age(&gate.timestamp),
            banner_answer_lines(project_root, gate),
        ));
    }
    banner.push_str("======================================================");
    Some(banner)
}

/// The banner's lines under one gate: the answers while its waiter is live, else why
/// none is offered.
fn banner_answer_lines(project_root: &Path, gate: &OpenGate) -> String {
    let (phase, stage) = (gate.phase, gate.stage);
    let waiter = GateId::parse(&gate.gate_id)
        .map_err(|error| error.to_string())
        .and_then(|id| {
            Gates::waiter_status(project_root, phase, stage, &id).map_err(|e| e.to_string())
        });
    match waiter {
        Ok(WaiterStatus::Live) => format!(
            "  approve: devflow gate approve {phase} --stage {stage}\n  reject:  devflow gate \
             reject {phase} --stage {stage} --note <reason>\n"
        ),
        Ok(WaiterStatus::Absent) => format!(
            "  debris: nothing waits on this gate, so an answer would reach no run; remove it \
             with `devflow discard --phase {phase}`\n"
        ),
        Err(error) => format!(
            "  error: whether anything waits on this gate could not be read ({error}); no \
             answer is offered\n"
        ),
    }
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
         devflow gate reject <phase> [--gate-id <id>] --note ... (fix and relaunch) | \
         devflow gate abort <phase> [--gate-id <id>] [--note ...] (ends the run)"
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
        let context = render_gate_context(&gate.context, GATE_CONTEXT_DISPLAY_CHARS);
        table.push_str(&format!(
            "\n{:<6} {:<9} {:<9} {:<16} {context}",
            gate.phase,
            gate.stage.to_string(),
            format_age(&gate.timestamp),
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
    let context = render_gate_context(&gate.context, GATE_CONTEXT_DISPLAY_CHARS);
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

/// The three answers an operator can give a gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GateAnswerKind {
    /// The stage's work is accepted and the run advances.
    Approve,
    /// The stage is fixed and relaunched; the run stays alive.
    Reject,
    /// The run ends.
    Abort,
}

impl GateAnswerKind {
    /// The word `gate_respond`'s pickup line opens with.
    fn past_tense(self) -> &'static str {
        match self {
            GateAnswerKind::Approve => "approved",
            GateAnswerKind::Reject => "rejected",
            GateAnswerKind::Abort => "aborted",
        }
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
    kind: GateAnswerKind,
    note: Option<String>,
    observed_gate_id: Option<String>,
) -> Result<(), CliError> {
    let report = gate_respond_report(project_root, phase, stage, kind, note, observed_gate_id)?;
    print!("{report}");
    Ok(())
}

/// [`gate_respond`]'s work, returning what it prints. An answer the supervisor's
/// own answer screen would refuse is refused here, before anything is written
/// (D-10, one refusal predicate); a written answer's report names the route
/// it runs, from the routers (D-45 rule 2).
pub(crate) fn gate_respond_report(
    project_root: &Path,
    phase: PhaseId,
    stage: Option<Stage>,
    kind: GateAnswerKind,
    note: Option<String>,
    observed_gate_id: Option<String>,
) -> Result<String, CliError> {
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
        approved: kind == GateAnswerKind::Approve,
        abort: kind == GateAnswerKind::Abort,
        note,
        responded_by: Some(responded_by),
    };
    // R10, D-10: the gate file as it is on disk names the answer's family, and
    // the supervisor's own screen decides whether the answer is refused.
    let gate =
        crate::pipeline_gate::read_gate_file(project_root, phase, stage).ok_or_else(|| {
            CliError::Message(format!(
                "phase {phase} {stage}'s gate file could not be read, so DevFlow cannot tell what \
             this answer would run; no response was written"
            ))
        })?;
    if let Err(refusal) = crate::pipeline_outcomes::screen_answer(gate.gate_kind, &response) {
        return Err(CliError::Message(format!(
            "the {} on phase {phase} {stage}'s gate is refused: {}; no response was written. \
             The answers that apply: {}",
            refusal.answer, refusal.why, refusal.applies
        )));
    }
    let path = Gates::answer(project_root, phase, stage, &observed, &response)
        .map_err(gate_answer_error)?;
    events::emit(
        project_root,
        phase,
        "gate_response_written",
        serde_json::json!({
            "stage": stage.to_string(),
            "gate_id": observed.as_str(),
            "approved": response.approved,
            "abort": response.abort,
            "via": "cli",
        }),
    );
    let pickup = if waiter_is_live(project_root, phase, stage, &observed) {
        response_pickup_message(WaiterStatus::Live).to_owned()
    } else {
        format!(
            "response was written, but no registered waiter will act. {}",
            no_waiter_repair(phase)
        )
    };
    let mut report = format!(
        "{} gate for phase {phase} {stage} — response was written; {pickup} (response at {})\n",
        kind.past_tense(),
        path.display()
    );
    for line in answer_route_lines(
        project_root,
        (phase, stage),
        &gate,
        kind,
        response.note.as_deref(),
    ) {
        report.push_str(&line);
        report.push('\n');
    }
    Ok(report)
}

/// The route a written answer runs, as `gate approve`, `gate reject` and
/// `gate abort` print it (D-10, D-45 rule 2, D-39): from
/// [`crate::pipeline_outcomes::approve_route`] and
/// [`crate::pipeline_outcomes::reject_route`] over the gate file read back from
/// disk and the run's recorded canary refusal, never the gate's `context` or
/// its recorded `gsd_status`. A verification approve reads GSD's status and
/// HEAD now, through the same `read_evidence` the supervisor uses, and says the
/// supervisor reads them again when it picks the answer up. A route that needs
/// the run's state is not guessed when the state cannot be loaded: the print
/// names the failure instead.
fn answer_route_lines(
    project_root: &Path,
    at: (PhaseId, Stage),
    gate: &GateFile,
    kind: GateAnswerKind,
    note: Option<&str>,
) -> Vec<String> {
    use crate::pipeline_outcomes::{
        AnswerFamily, ApproveRoute, RejectRoute, Step, Trigger, answer_family, approve_route,
        evidence_root, gsd_status_name, next_step, read_evidence, reject_route, status_less_phrase,
        step_phrase, unread_evidence,
    };
    let (phase, stage) = at;
    let route_line = |name: &str, phrase: &str| format!("route: {name} — {phrase}");
    let family = answer_family(gate.gate_kind);
    let state = workflow::load_state(project_root, phase);
    // The family's route reads the run's state: the canary for a stage
    // failure, the recorded PASS head and evidence root for a verification
    // approve.
    let reads_state = matches!(
        (family, kind),
        (AnswerFamily::StageFailure, _) | (AnswerFamily::Verification, GateAnswerKind::Approve)
    );
    let state = match state {
        Ok(state) => Some(state),
        Err(error) if reads_state && kind != GateAnswerKind::Abort => {
            return vec![format!(
                "route: not known — the run's state could not be loaded ({error}), so DevFlow \
                 cannot tell which route this answer runs; the supervisor decides when it picks \
                 this answer up"
            )];
        }
        Err(_) => None,
    };
    let canary_refusal = state
        .as_ref()
        .and_then(crate::pipeline_launch::recorded_canary_refusal);
    let canary_refusal = canary_refusal.as_deref();
    // A relaunch runs on the run's current agent, so the print names it (D-15,
    // D-37: the rate-limit gate relaunches on the same agent).
    let on_agent = match &state {
        Some(state) => format!(", on {} (the run's current agent)", state.agent),
        None => ", on the run's current agent (its state could not be loaded)".to_string(),
    };
    let status_less = |step: &crate::pipeline_outcomes::StatusLessStep| {
        use crate::pipeline_outcomes::StatusLessStep;
        let agent = match step {
            StatusLessStep::RetryStage
            | StatusLessStep::RelaunchNow
            | StatusLessStep::RelaunchWithNote(_) => on_agent.as_str(),
            StatusLessStep::RelaunchRefused { .. }
            | StatusLessStep::PreflightOverride
            | StatusLessStep::RecordCheckpointSet => "",
        };
        route_line(
            step.name(),
            &format!("{}{agent}", status_less_phrase(step, phase, stage)),
        )
    };
    let refused = |why: &str| format!("route: refused — {why}");
    match kind {
        GateAnswerKind::Abort => {
            let step: Step = next_step(Trigger::Abort);
            vec![route_line(step.name(), step_phrase(&step))]
        }
        GateAnswerKind::Reject => {
            let note = note.unwrap_or_default();
            let route = match reject_route(gate, canary_refusal, note) {
                RejectRoute::Verification(step) => route_line(step.name(), step_phrase(&step)),
                RejectRoute::StatusLess(step) => status_less(&step),
                RejectRoute::Refused(refusal) => refused(refusal.why),
            };
            vec![route, format!("note: {note}")]
        }
        GateAnswerKind::Approve => {
            let route = match &state {
                Some(state) => approve_route(gate, canary_refusal, || {
                    read_evidence(
                        &evidence_root(project_root, state),
                        phase,
                        state.validate_pass_head.as_deref(),
                    )
                }),
                // No state, and the family reads none: its approve reads no
                // evidence either.
                None => approve_route(gate, None, unread_evidence),
            };
            match route {
                ApproveRoute::Verification { step, evidence } => vec![
                    route_line(step.name(), step_phrase(&step)),
                    format!(
                        "read now: GSD verification {}; Validate pass on this commit over a \
                         clean tree: {}",
                        evidence
                            .status()
                            .map_or("could not be read", gsd_status_name),
                        if evidence.agent_pass_on_head {
                            "yes"
                        } else {
                            "no"
                        }
                    ),
                    "the supervisor reads GSD's verification status again when it picks this \
                     answer up, and routes on that read"
                        .to_string(),
                ],
                ApproveRoute::StatusLess(step) => vec![status_less(&step)],
                ApproveRoute::Refused(refusal) => vec![refused(refusal.why)],
            }
        }
    }
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
            no_waiter_repair(phase)
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

/// The liveness half of [`gate_respond`] (D-04). With no registered waiter there is
/// nothing to act on an answer, so none is written.
fn refuse_a_gate_with_no_waiter(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
    observed: &GateId,
) -> Result<(), CliError> {
    if waiter_is_live(project_root, phase, stage, observed) {
        return Ok(());
    }
    Err(CliError::Message(format!(
        "no registered waiter for phase {phase} {stage}; no response was written. {}",
        no_waiter_repair(phase)
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

fn no_waiter_repair(phase: PhaseId) -> String {
    format!(
        "run `devflow resume --phase {phase}` to continue the run, or `devflow discard --phase \
         {phase}` to remove it (its state, gate files, worktree and branch)"
    )
}

/// Conservative post-publication wording. The second status observation only
/// narrows the message race; it cannot establish a waiter's lifetime.
fn response_pickup_message(status: WaiterStatus) -> &'static str {
    match status {
        WaiterStatus::Live => {
            "a registered waiter may pick up this response; this is not a lifetime guarantee"
        }
        WaiterStatus::Absent => {
            "no confirmed waiter remains; the response may be stale, so use the named recovery"
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
/// a human may have answered the same gate between
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
) -> Result<(), CliError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    gate_sweep_at(max_age_secs, dry_run, root, now)
}

/// [`gate_sweep`] at `now`, in Unix epoch seconds, so a test can place the
/// sweep before or after a rate-limit gate's not-before time without waiting
/// on the clock.
fn gate_sweep_at(
    max_age_secs: Option<u64>,
    dry_run: bool,
    root: Option<PathBuf>,
    now: u64,
) -> Result<(), CliError> {
    let threshold = max_age_secs.unwrap_or_else(config_parse::gate_max_unattended_age_secs);

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
            match sweep_one_gate(project_root, &gate, age, threshold, dry_run, now)? {
                SweepStep::Reaped => reaped += 1,
                SweepStep::Skipped => skipped += 1,
                SweepStep::LeftAlone => left_alone += 1,
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

/// Whether the sweep may reap an aged gate at `stage`: only one with a live
/// waiter, and never a rate-limit gate before its `not_before` time (D-27.2,
/// T-52-46), `now` being Unix epoch seconds.
fn gate_sweep_may_reap(
    _stage: Stage,
    waiter_live: bool,
    not_before: Option<i64>,
    now: u64,
) -> bool {
    waiter_live && !rate_limit_gate_still_waits(not_before, now)
}

/// True while a rate-limit gate's `not_before` time is still ahead of `now`.
fn rate_limit_gate_still_waits(not_before: Option<i64>, now: u64) -> bool {
    not_before.is_some_and(|not_before| i64::try_from(now).unwrap_or(i64::MAX) < not_before)
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
    now: u64,
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
    if waiter_live && rate_limit_gate_still_waits(gate.not_before, now) {
        println!(
            "left phase {phase} {stage} alone at {at} — a rate-limit gate waiting for its reset \
             (not before {}); not reaped",
            gate.not_before.unwrap_or_default()
        );
        return Ok(SweepStep::LeftAlone);
    }
    if !gate_sweep_may_reap(stage, waiter_live, gate.not_before, now) {
        println!(
            "left phase {phase} {stage} alone at {at} — no confirmed waiter; {}",
            no_waiter_repair(phase)
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

/// D-05: end the phase's run by stopping its user unit, then report how it ended from
/// the state. `stop` takes no holder and writes nothing but the unit's stop.
///
/// With the runner lock free it reads the phase holder first, without taking it: a held
/// holder means `start`, `resume`, `discard` or `cleanup` holds the phase (D-07), so
/// `stop` stops nothing, names the holder's pid and exits non-zero (review pi-10, pi-5).
/// With no live run it is a no-op that exits 0. Otherwise it runs
/// `systemctl --user stop` on the unit, which returns once the unit is down
/// (`TimeoutStopSec`, then SIGKILL), and prints [`stop_outcome`]'s line.
pub(crate) fn stop(project_root: &Path, phase: PhaseId) -> Result<(), CliError> {
    println!("{}", stop_report(project_root, phase)?);
    Ok(())
}

/// [`stop`]'s work (D-05): the line it prints when it exits 0, or its refusal or failure.
pub(crate) fn stop_report(project_root: &Path, phase: PhaseId) -> Result<String, CliError> {
    let runner = runner_liveness(project_root, phase).map_err(|why| {
        CliError::Message(format!("stop: phase {phase}: {why}; nothing was stopped"))
    })?;
    if !matches!(runner, RunnerLiveness::Live { .. }) {
        let holder = liveness::phase_holder_path(project_root, phase);
        match liveness::live_holder(&holder) {
            Err(err) => {
                return Err(CliError::Message(format!(
                    "stop: phase {phase}: its holder file {} could not be read ({err}); \
                     nothing was stopped",
                    holder.display()
                )));
            }
            Ok(Some((pid, _))) => {
                return Err(CliError::Message(format!(
                    "another DevFlow verb holds phase {phase} (pid {pid}) and is in progress; \
                     run devflow stop --phase {phase} again once it returns"
                )));
            }
            Ok(None) => {}
        }
        if runner == RunnerLiveness::NotLive {
            return Ok(format!(
                "stop: phase {phase} has no live run; nothing to stop"
            ));
        }
    }
    let unit = crate::service::unit_name(project_root, phase).map_err(|err| {
        CliError::Message(format!(
            "stop: phase {phase}'s unit name could not be derived ({err}); nothing was stopped"
        ))
    })?;
    crate::service::stop_unit(&unit)
        .map_err(|why| CliError::Message(format!("stop: unit {unit} was not stopped: {why}")))?;
    let (line, clean) = stop_outcome(phase, workflow::load_state(project_root, phase));
    if clean {
        Ok(line)
    } else {
        Err(CliError::Message(line))
    }
}

/// What `stop` prints once the unit is down, and whether it exits 0 (D-05), from the
/// state's load result. Finished is read before stopped, because a finished run records
/// `stopped` too. A state that is absent, or that only a reservation wrote
/// ([`run_never_prepared`]), was stopped during its launch (review agy-10). A state that
/// does not load names only `devflow discard` (D-12). Any other state was not marked
/// stopped by its runner: it crashed.
pub(crate) fn stop_outcome(
    phase: PhaseId,
    loaded: Result<State, workflow::WorkflowError>,
) -> (String, bool) {
    match loaded {
        Ok(state) if state.finished => (
            format!("stop: phase {phase} had already finished; nothing more to stop"),
            true,
        ),
        Ok(state) if state.stopped => (
            format!(
                "stop: phase {phase} stopped ({}); run devflow resume --phase {phase} to \
                 continue it",
                state.stop_reason.as_deref().unwrap_or("no reason recorded")
            ),
            true,
        ),
        Ok(state) if run_never_prepared(&state) => (stopped_during_launch_line(phase), true),
        Err(workflow::WorkflowError::MissingState(_)) => (stopped_during_launch_line(phase), true),
        Err(err) => (
            format!(
                "stop: the unit is down, but phase {phase}'s state does not load ({err}); \
                 run devflow discard --phase {phase} to remove it"
            ),
            false,
        ),
        Ok(_) => (
            format!(
                "stop: phase {phase} crashed: its runner ended without recording a stop; run \
                 devflow resume --phase {phase} to continue it, or devflow discard --phase \
                 {phase} to remove it"
            ),
            false,
        ),
    }
}

/// [`stop_outcome`]'s line for a run the stop ended before its runner recorded anything.
fn stopped_during_launch_line(phase: PhaseId) -> String {
    format!(
        "stop: phase {phase} stopped during launch; nothing ran, devflow start --phase {phase} \
         starts it again"
    )
}

/// Hold `phase` for this foreground verb's lifetime (D-07): the one path `start`,
/// `resume`, `discard` and `cleanup` claim through, so no two of them act on the phase at
/// once. It installs no stop handler (D-12): `devflow stop` never signals a holder, it
/// refuses while one holds the phase. The runner never takes it.
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
    let holder = liveness::claim_phase_holder(project_root, phase).map_err(|err| match err {
        liveness::HolderError::Held { holder } => {
            let pid = holder.map_or_else(|| "unknown".to_string(), |(pid, _)| pid.to_string());
            CliError::Message(format!(
                "another DevFlow verb holds phase {phase} (pid {pid}) and is in progress; run \
                 this again once it returns"
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
/// discoverability counterpart to `gate_list`'s `GATE_CONTEXT_DISPLAY_CHARS` truncation
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
/// `gate_list`'s `render_gate_context(.., GATE_CONTEXT_DISPLAY_CHARS)`). Factored out of `gate_show`
/// so the untruncated-context guarantee is unit-testable without capturing
/// process stdout.
fn render_gate_show(gate: &OpenGate) -> String {
    format!(
        "phase {} {} ({})\ngate id: {}\n{}",
        gate.phase,
        gate.stage,
        format_age(&gate.timestamp),
        printable_gate_id(gate),
        render_gate_context(&gate.context, usize::MAX),
    )
}

/// How long `logs -f` waits, once the agent's exit file is there, for the runner to
/// leave the followed stage before it prints the run's state line anyway.
const POST_EXIT_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

/// How often `logs -f` polls the capture and, at its end, the run.
const FOLLOW_POLL: std::time::Duration = std::time::Duration::from_millis(500);

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
    let offset = print_capture_from(&path, 0)?;
    if !follow {
        return Ok(());
    }
    follow_capture(project_root, phase, &path, offset)
}

/// The stage the phase's state names, `None` when it does not load.
fn state_stage(project_root: &Path, phase: PhaseId) -> Option<Stage> {
    workflow::load_state(project_root, phase)
        .ok()
        .map(|state| state.stage)
}

/// `logs -f`'s loop. It ends, printing what comes next, when the agent's exit code has
/// landed and a further poll is quiescent (D-10: then the run's state line once the
/// runner has left the stage, [`post_exit_line`]); when the state is gone; or when a
/// quiescent poll finds the runner NotLive, which is how a hard-killed runner that
/// wrote no exit file looks (review codex-9). A liveness read error ends it with that
/// error. An operator can always Ctrl-C sooner.
fn follow_capture(
    project_root: &Path,
    phase: PhaseId,
    path: &Path,
    mut offset: u64,
) -> Result<(), CliError> {
    let exit_path = agent_result::exit_code_path(project_root, phase);
    let state_path = workflow::state_path(project_root, phase);
    let mut followed_stage = state_stage(project_root, phase);
    loop {
        std::thread::sleep(FOLLOW_POLL);
        let state_exists = state_path.exists();
        // 14-CR-03: a stage transition archives and recreates the capture
        // file (launch_stage → archive_phase_files), so a shrunken file
        // means the next stage started — reset to the top instead of
        // seeking past EOF forever and silently skipping its output.
        let base = rollover_offset(path, offset);
        if base != offset {
            eprintln!("== capture restarted (next stage) — following from the top ==");
            followed_stage = state_stage(project_root, phase);
        }
        let new_offset = print_capture_from(path, base)?;
        // Quiescent only if no rollover happened AND no new bytes appeared.
        let quiescent = base == offset && new_offset == offset;
        if exit_path.exists() && quiescent {
            if let Ok(code) = std::fs::read_to_string(&exit_path) {
                eprintln!("== agent exited with code {} ==", code.trim());
            }
            if let Some(stage) = followed_stage {
                let line = post_exit_line(project_root, phase, stage, POST_EXIT_WAIT)?;
                eprintln!("== phase {phase}: {line} ==");
            }
            return Ok(());
        }
        // A runner whose state vanished writes no exit file (D-03) and has no successor,
        // so none will come. A state that was gone before the first poll is the same
        // case: every launch saves its state before it spawns (D-01), so there is no run
        // still to start here.
        if !state_exists && quiescent {
            eprintln!("== phase {phase}'s state is gone and no exit code was recorded ==");
            return Ok(());
        }
        if quiescent {
            let liveness = runner_liveness(project_root, phase).map_err(|why| {
                CliError::Message(format!(
                    "phase {phase}: whether its run is live could not be read: {why}"
                ))
            })?;
            if liveness == RunnerLiveness::NotLive {
                eprintln!("== phase {phase}: {} ==", run_line(project_root, phase)?);
                return Ok(());
            }
        }
        offset = new_offset;
    }
}

/// The state line for a phase with nothing left on disk: no state, no gate file, the
/// runner lock free and the unit inactive.
const NO_RUN_LINE: &str = "no run — its state is gone and nothing runs it";

/// The phase's [`RunStatus`] line now, or [`NO_RUN_LINE`].
///
/// # Errors
/// [`runner_liveness`] could not answer.
fn run_line(project_root: &Path, phase: PhaseId) -> Result<String, CliError> {
    match run_status(project_root, phase, &existing_state(project_root, phase)) {
        Some(RunStatus::Error(why)) => Err(CliError::Message(format!(
            "phase {phase}: whether its run is live could not be read: {why}"
        ))),
        Some(status) => Ok(status.line(phase)),
        None => Ok(NO_RUN_LINE.to_string()),
    }
}

/// D-10 (review r2 codex-7): the run's state line once the agent's exit file is there.
/// The exit file is written before the runner advances, so the state read that moment
/// can still name `followed_stage` as running. This polls [`run_status`] every
/// [`FOLLOW_POLL`] until the run is no longer live at `followed_stage` with no gate
/// pending (the state names another stage, a gate is pending, the run finished or
/// stopped, the state is gone, or the runner reads NotLive) and returns that line. At
/// `bound` it returns the line as it stands, naming the live stage, so it never polls
/// forever.
///
/// # Errors
/// [`runner_liveness`] could not answer.
pub(crate) fn post_exit_line(
    project_root: &Path,
    phase: PhaseId,
    followed_stage: Stage,
    bound: std::time::Duration,
) -> Result<String, CliError> {
    let deadline = std::time::Instant::now() + bound;
    loop {
        let status = run_status(project_root, phase, &existing_state(project_root, phase));
        let waiting = match &status {
            Some(RunStatus::Error(why)) => {
                return Err(CliError::Message(format!(
                    "phase {phase}: whether its run is live could not be read: {why}"
                )));
            }
            Some(RunStatus::Live {
                stage,
                gate_pending,
                ..
            }) => !gate_pending && *stage == Some(followed_stage),
            Some(RunStatus::Stopping { .. }) => true,
            _ => false,
        };
        if !waiting || std::time::Instant::now() >= deadline {
            return Ok(status.map_or_else(|| NO_RUN_LINE.to_string(), |status| status.line(phase)));
        }
        std::thread::sleep(FOLLOW_POLL);
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
        rust_log_check(std::env::var("RUST_LOG").ok().as_deref()),
    ]
}

/// The doctor row for `RUST_LOG`, from its value: `None` when it is unset (or
/// not Unicode). Split out of [`doctor_checks`] so a test can pin each case
/// without mutating the process environment.
fn rust_log_check(value: Option<&str>) -> Check {
    // RUST_LOG environment check: validate the value is a parsable log directive.
    let (rust_log_status, rust_log_version, rust_log_hint) = match value {
        // Unset and empty both turn tracing off (main.rs's default filter):
        // information, not a failure.
        None | Some("") => (
            "info",
            Some("not set or empty — tracing off".to_string()),
            Some("Set RUST_LOG=info to see tracing lines".to_string()),
        ),
        Some(val) => {
            let all_valid = val.split(',').all(|directive| {
                let directive = directive.trim();
                if let Some((_target, level)) = directive.split_once('=') {
                    matches!(level.trim(), "error" | "warn" | "info" | "debug" | "trace")
                } else {
                    matches!(directive, "error" | "warn" | "info" | "debug" | "trace")
                }
            });
            if all_valid {
                ("ok", Some(val.to_string()), None)
            } else {
                (
                    "warn",
                    Some(val.to_string()),
                    Some("RUST_LOG value may be invalid — expected error, warn, info, debug, or trace".to_string()),
                )
            }
        }
    };
    Check {
        name: "RUST_LOG".into(),
        status: rust_log_status.into(),
        version: rust_log_version,
        install_hint: rust_log_hint,
    }
}

/// One environment check as `doctor` prints it (without the newline): name,
/// version or detail, status icon, and the hint when the status carries one.
fn check_line(c: &Check) -> String {
    let icon = match c.status.as_str() {
        "ok" => "✓",
        "missing" => "✗",
        "warn" => "⚠",
        "info" => "ℹ",
        _ => "?",
    };
    let version_str = c.version.as_deref().unwrap_or("-");
    let mut line = format!("  {:<20} {:<20} {}", c.name, version_str, icon);
    #[allow(clippy::collapsible_if)]
    if c.status == "missing" || c.status == "warn" || c.status == "info" {
        if let Some(hint) = &c.install_hint {
            line.push_str(&format!(" — {}", hint));
        }
    }
    line
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
        let body = doctor_json_body(&checks, &facts, &doc_findings, &stray_findings, &set_aside);
        println!(
            "{}",
            serde_json::to_string_pretty(&body).expect("doctor --json body must serialize")
        );
    } else {
        for c in &checks {
            println!("{}", check_line(c));
        }
        print!("{}", render_reconciliation_text(&facts));
        print!("{}", render_set_aside_findings_text(&set_aside));
        print!("{}", render_planning_doc_text(&doc_findings));
        print!("{}", render_stray_process_text(&stray_findings));
    }

    Ok(())
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
    /// The phase's runner liveness, from [`runner_liveness`] (D-07, D-12a): the runner
    /// lock, then the unit. `Err` holds why it could not be read.
    pub(crate) runner: Result<RunnerLiveness, String>,
    /// The most recent event's `event` field value, for display context.
    pub(crate) last_event: Option<String>,
    /// The `stage` field of the most recent `stage_launched` event; `None`
    /// when the last event recorded for this phase is not a launch.
    pub(crate) last_launched_stage: Option<Stage>,
    pub(crate) open_gate_stages: Vec<Stage>,
    pub(crate) feature_branch_exists: bool,
    /// Whether this phase was intentionally halted (`State.stopped`): a stopped phase
    /// with no runner is expected, not a crash, so [`check_runner`] reports nothing.
    pub(crate) stopped: bool,
    /// Whether the run finished (`State.finished`, D-35): no runner is expected.
    pub(crate) finished: bool,
    /// The `start` to run instead of `resume` when [`run_never_prepared`] holds.
    pub(crate) unprepared_restart: Option<String>,
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
        detail: format!(
            "phase {}: gate_pending is true at stage {} but no gate file is open",
            facts.phase, facts.stage
        ),
        repair: Some(repair_instead_of_resume(
            facts,
            format!("devflow resume --phase {}", facts.phase),
        )),
    })
}

/// A repair that would send the operator to `devflow resume`, unless the launch guard
/// would refuse it: while the runner may be live (its lock is held, its unit is still
/// stopping, or the read failed) `resume` refuses, so the repair names what to do
/// instead. A live runner may be healthy, so `stop` is offered, not prescribed.
fn repair_instead_of_resume(facts: &PhaseFacts, resume: String) -> String {
    let phase = facts.phase;
    match &facts.runner {
        Ok(RunnerLiveness::Live { .. }) => {
            format!("wait for its live runner, or `devflow stop --phase {phase}`")
        }
        Ok(RunnerLiveness::Stopping { .. }) => {
            format!("wait for its unit to stop, or `devflow stop --phase {phase}`")
        }
        Err(_) => "devflow status".to_string(),
        Ok(RunnerLiveness::NotLive) => facts.unprepared_restart.clone().unwrap_or(resume),
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
        repair: Some(repair_instead_of_resume(
            facts,
            no_waiter_repair(facts.phase),
        )),
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

/// The runner's liveness needs a repair (D-07, D-12a): it could not be read, so a live
/// runner cannot be ruled out, or no runner runs a phase that is neither stopped nor
/// finished, so nothing will advance it. A live or stopping runner is never a finding.
/// The read error's text may name the operator's lock path, so the finding does not
/// carry it (T-18-01): `devflow status` prints it.
fn check_runner(facts: &PhaseFacts) -> Option<PhaseFinding> {
    let phase = facts.phase;
    let (detail, repair) = match &facts.runner {
        Err(_) => (
            format!(
                "phase {phase}: its runner's liveness could not be read, so a live runner \
                 cannot be ruled out"
            ),
            "devflow status".to_string(),
        ),
        Ok(RunnerLiveness::NotLive) if !facts.stopped && !facts.finished => (
            format!(
                "phase {phase}: no runner runs this phase at stage {}, but it is not stopped",
                facts.stage
            ),
            repair_instead_of_resume(facts, format!("devflow resume --phase {phase}")),
        ),
        Ok(_) => return None,
    };
    Some(PhaseFinding {
        phase,
        severity: Severity::Problem,
        detail,
        repair: Some(repair),
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
        let give_up = "`devflow discard` gives up after 10 s naming it";
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

/// The phase's feature branch does not exist even though its run is past
/// preparation. A `Warn` — a not-yet-pushed or manually deleted branch is
/// recoverable without state surgery. A run that was never prepared
/// ([`PhaseFacts::unprepared_restart`] is set) never forked a branch.
fn check_missing_branch(facts: &PhaseFacts) -> Option<PhaseFinding> {
    if facts.feature_branch_exists || facts.unprepared_restart.is_some() {
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
        check_runner(facts),
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
/// primitive already used elsewhere (`status`) —
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
    let (waiter, waiter_gate) = phase_gate_waiter(project_root, phase, open_gates);
    let runner = runner_liveness(project_root, phase);
    let (write_lock, write_lock_error) = match workflow::write_lock_status(project_root, phase) {
        Ok(status) => (status, None),
        Err(error) => (workflow::WriteLockStatus::Free, Some(error.to_string())),
    };
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
        runner,
        last_event: last_event_name,
        last_launched_stage,
        open_gate_stages,
        feature_branch_exists,
        stopped,
        finished: state.finished,
        unprepared_restart,
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
                    "runner": runner_json(phase_facts),
                    "gate_waiter": gate_waiter_json(phase_facts),
                })
            })
            .collect(),
    )
}

/// A phase's runner facts for `doctor --json` (D-07, D-12a): its liveness label
/// (`live`, `stopping`, `not_live` or `error`) and the runner's pid when its record
/// reads. Never a path (T-18-01).
fn runner_json(facts: &PhaseFacts) -> serde_json::Value {
    let (liveness, pid) = match &facts.runner {
        Ok(RunnerLiveness::Live { pid }) => ("live", *pid),
        Ok(RunnerLiveness::Stopping { .. }) => ("stopping", None),
        Ok(RunnerLiveness::NotLive) => ("not_live", None),
        Err(_) => ("error", None),
    };
    serde_json::json!({
        "liveness": liveness,
        "pid": pid,
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
/// D-05 adds `"set_aside_gate_answers"`, an empty array when there are none.
fn doctor_json_body(
    checks: &[Check],
    facts: &[PhaseFacts],
    doc_findings: &[PlanningDocFinding],
    stray_findings: &[StrayProcessFinding],
    set_aside: &[PhaseFinding],
) -> serde_json::Value {
    serde_json::json!({
        "environment": checks_json_value(checks),
        "reconciliation": render_reconciliation_json(facts),
        "planning_doc_staleness": render_planning_doc_findings_json(doc_findings),
        "stray_processes": render_stray_process_findings_json(stray_findings),
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

/// Human-readable label for a [`agent::StrayLayer`] in `doctor`'s finding.
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
/// root registered, or a file taken, after it is not covered, so `doctor` can
/// report a process as a stray that became reachable meanwhile. Nothing
/// signals on this set: `doctor`'s repair has the operator confirm the pid
/// with `ps` before ending it by hand.
pub(crate) fn registry_reachable_pids(roots: &[PathBuf]) -> HashSet<u32> {
    let mut reachable = HashSet::new();
    let mut scanned = HashSet::new();
    for root in roots {
        if !scanned.insert(root.clone()) {
            // One root with N phases yields N registry entries, and one pass over its
            // `.devflow/` already covers every phase.
            continue;
        }
        reachable.extend(held_phase_holders(root));
        reachable.extend(
            Gates::live_waiter_holders(root)
                .into_iter()
                .map(|(pid, _start_time)| pid),
        );
    }
    reachable
}

/// The pid recorded by each held `.devflow/.holder-*` under `root`. A file whose record
/// cannot be read is reported and left out.
fn held_phase_holders(root: &Path) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir(root.join(".devflow")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with(".holder-")
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

/// The composition `doctor`'s stray finding routes through: the
/// registry-independent `/proc` census
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
                repair: Some(format!(
                    "confirm pid {pid} is the stray with `ps -o args= -p {pid}`, then end it \
                     with `kill {pid}`",
                    pid = stray.pid
                )),
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
/// starts, so this finding covers 2.x leftovers only (50-12).
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
    use devflow_core::gates::AnswerRead;
    use devflow_core::gates::WaiterRegistration;
    use devflow_core::test_support::{gate_answer_files, with_answer_before_publish};

    #[test]
    fn state_age_secs_parses_epoch() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let started = (now - 120).to_string();
        let age = state_age_secs(&started).expect("age");
        // Allow a small window for clock drift during the test.
        assert!((118..=125).contains(&age), "unexpected age: {age}");
    }

    #[test]
    fn format_age_buckets_by_magnitude() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ago = |secs: u64| format_age(&(now - secs).to_string());
        assert!(ago(30).ends_with("s ago"));
        assert!(ago(120).ends_with("m ago"));
        assert!(ago(7200).ends_with("h ago"));
        assert!(ago(2 * 86400).ends_with("d ago"));
        assert_eq!(format_age("garbage"), "unknown");
    }

    /// The names of the removed `recover` verb that D-09a keeps out of production text.
    const REMOVED_RECOVER_NAMES: [&str; 3] = ["devflow recover", "recover --clean", "`recover`"];

    /// The lines of `source` that name the removed `recover` verb, as `line: text`:
    /// `source` is cut at its first `mod tests {` line, and a line whose trimmed start
    /// is `//` is a comment and not counted.
    fn lines_naming_the_removed_recover_verb(source: &str) -> Vec<String> {
        source
            .lines()
            .enumerate()
            .take_while(|(_, line)| !line.contains("mod tests {"))
            .filter(|(_, line)| !line.trim_start().starts_with("//"))
            .filter(|(_, line)| REMOVED_RECOVER_NAMES.iter().any(|name| line.contains(name)))
            .map(|(index, line)| format!("{}: {}", index + 1, line.trim()))
            .collect()
    }

    /// Every `.rs` file under `dir`, recursively, sorted.
    fn rust_sources(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut pending = vec![dir.to_path_buf()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir)
                .unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
            {
                let path = entry.expect("a source entry").path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    files.push(path);
                }
            }
        }
        files.sort();
        files
    }

    /// D-09a: no production string sends the operator to the removed `recover` verb.
    /// Scans every source of both crates but devflow-core's `doc_check.rs` (a
    /// `#[cfg(test)]` module, whose fixtures quote removed verbs). Controls: a planted
    /// string line counts 1; the same line as a comment, or after a `mod tests {` line,
    /// counts 0.
    #[test]
    fn no_production_string_names_the_removed_recover_verb() {
        let planted = "    \"run `devflow recover --clean --phase 7` first\"";
        assert_eq!(
            lines_naming_the_removed_recover_verb(planted).len(),
            1,
            "planted control: a string line naming the verb counts"
        );
        let commented = format!("// {}", planted.trim_start());
        assert_eq!(
            lines_naming_the_removed_recover_verb(&commented).len(),
            0,
            "planted control: a comment does not count"
        );
        let in_tests = format!("fn f() {{}}\nmod tests {{\n{planted}\n}}\n");
        assert_eq!(
            lines_naming_the_removed_recover_verb(&in_tests).len(),
            0,
            "planted control: a line inside the test module does not count"
        );

        let cli = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../devflow-core/src");
        let mut files = rust_sources(&cli);
        files.extend(
            rust_sources(&core)
                .into_iter()
                .filter(|path| path.file_name().is_none_or(|name| name != "doc_check.rs")),
        );
        assert!(
            files.len() > 40 && files.iter().any(|path| path.ends_with("src/commands.rs")),
            "the scan read too few sources: {files:?}"
        );
        let found: Vec<String> = files
            .iter()
            .flat_map(|path| {
                let source = std::fs::read_to_string(path)
                    .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
                lines_naming_the_removed_recover_verb(&source)
                    .into_iter()
                    .map(move |line| format!("{}:{line}", path.display()))
            })
            .collect();
        assert_eq!(
            found,
            Vec::<String>::new(),
            "production lines name the removed `recover` verb"
        );
    }

    /// How many production lines of `source` read the agent pid file: `source` is cut
    /// at its first `mod tests {` line, and a line whose trimmed start is `//` is not
    /// counted. Neither is the declaration (`fn agent_pid_path(`) nor archive's
    /// stale-file cleanup (`remove_file(agent_pid_path(`), which deletes and reads nothing.
    fn agent_pid_readers(source: &str) -> usize {
        source
            .lines()
            .take_while(|line| !line.contains("mod tests {"))
            .filter(|line| !line.trim_start().starts_with("//"))
            .filter(|line| line.contains("agent_pid_path("))
            .filter(|line| {
                !line.contains("fn agent_pid_path(")
                    && !line.contains("remove_file(agent_pid_path(")
            })
            .count()
    }

    /// D-12a: no production code of either crate reads the agent pid file; liveness is
    /// the runner lock ([`runner_liveness`]). Controls: a planted source with one reader
    /// counts 1, and one holding only the declaration and the cleanup counts 0.
    #[test]
    fn no_production_reader_of_the_agent_pid_file_remains() {
        let planted = "fn f() {\n    let path = agent_result::agent_pid_path(root, phase);\n}\n\
                       // agent_pid_path( in a comment\nmod tests {\n    agent_pid_path(x);\n}\n";
        assert_eq!(
            agent_pid_readers(planted),
            1,
            "planted control: one production reader counts 1"
        );
        let reader = "    let pid = std::fs::read_to_string(agent_pid_path(root, phase));\n";
        assert_eq!(
            agent_pid_readers(reader),
            1,
            "planted control: the read_to_string reader counts 1"
        );
        let declaration_and_cleanup = "pub fn agent_pid_path(project_root: &Path, phase: PhaseId) -> PathBuf {\n\
                                       }\n    let _ = std::fs::remove_file(agent_pid_path(project_root, phase));\n";
        assert_eq!(
            agent_pid_readers(declaration_and_cleanup),
            0,
            "planted control: the declaration and the stale-file cleanup are not readers"
        );
        let files = both_crates_rust_sources();
        let found: Vec<String> = files
            .iter()
            .filter_map(|path| {
                let source = std::fs::read_to_string(path)
                    .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
                let count = agent_pid_readers(&source);
                (count > 0).then(|| format!("{}: {count}", path.display()))
            })
            .collect();
        assert_eq!(
            found,
            Vec::<String>::new(),
            "production code still reads the agent pid file"
        );
    }

    /// Every `.rs` file under the CLI's `src/` and, recursively, devflow-core's `src/`,
    /// but devflow-core's `doc_check.rs`: lib.rs declares it under `#[cfg(test)]`, so it
    /// is never in a build, and its fixtures quote removed names.
    fn both_crates_rust_sources() -> Vec<PathBuf> {
        let cli = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../devflow-core/src");
        let mut files = rust_sources(&cli);
        files.extend(
            rust_sources(&core)
                .into_iter()
                .filter(|path| path.file_name().is_none_or(|name| name != "doc_check.rs")),
        );
        assert!(
            files.len() > 40
                && files.iter().any(|path| path.ends_with("src/commands.rs"))
                && files
                    .iter()
                    .any(|path| path.ends_with("src/agents/claude.rs")),
            "the scan read too few sources: {files:?}"
        );
        files
    }

    /// Each line `keep` keeps from each of `files`, with the file's path. `keep` gets the
    /// whole source and decides which part counts (a test-module cut, a comment filter).
    fn kept_lines_with_paths(
        files: &[PathBuf],
        keep: impl Fn(&str) -> Vec<String>,
    ) -> Vec<(PathBuf, String)> {
        files
            .iter()
            .flat_map(|path| {
                let source = std::fs::read_to_string(path)
                    .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
                keep(&source)
                    .into_iter()
                    .map(|line| (path.clone(), line))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The names of the State supervisor records 52.2-25 removed.
    const REMOVED_STATE_RECORD_NAMES: [&str; 3] = [
        "pending_supervisor",
        "PendingSupervisor",
        "SupervisorRecord",
    ];

    /// The lines of `source` before its first `mod tests {` line, comments kept, that
    /// name a removed State record.
    fn lines_naming_a_removed_state_record(source: &str) -> Vec<String> {
        source
            .lines()
            .take_while(|line| !line.contains("mod tests {"))
            .filter(|line| {
                REMOVED_STATE_RECORD_NAMES
                    .iter()
                    .any(|name| line.contains(name))
            })
            .map(str::to_string)
            .collect()
    }

    /// Whether `source`'s `pub struct State {` block, through the first line that is
    /// exactly `}`, declares a field named `supervisor`.
    fn state_struct_has_a_supervisor_field(source: &str) -> bool {
        source
            .lines()
            .skip_while(|line| !line.starts_with("pub struct State {"))
            .take_while(|line| *line != "}")
            .any(|line| {
                let field = line.trim_start();
                field
                    .strip_prefix("pub ")
                    .unwrap_or(field)
                    .starts_with("supervisor:")
            })
    }

    /// D-01, D-07: no source of either crate names a removed State supervisor record,
    /// comments included, outside its test module, and `State` has no `supervisor`
    /// field. Controls: a planted line counts 1, the same line after a `mod tests {`
    /// line counts 0, and a planted `State` block with the field is found while one
    /// without it is not.
    #[test]
    fn no_source_names_a_removed_state_supervisor_item() {
        let planted = "    let pending = state.pending_supervisor.take();";
        assert_eq!(
            lines_naming_a_removed_state_record(&format!("fn f() {{\n{planted}\n}}\n")).len(),
            1,
            "planted control: a production line counts 1"
        );
        assert_eq!(
            lines_naming_a_removed_state_record(&format!(
                "fn f() {{}}\nmod tests {{\n{planted}\n}}\n"
            ))
            .len(),
            0,
            "planted control: a line inside the test module does not count"
        );
        assert!(
            state_struct_has_a_supervisor_field(
                "pub struct State {\n    pub supervisor: Option<Record>,\n}\n"
            ),
            "planted control: a State field named supervisor is found"
        );
        assert!(
            !state_struct_has_a_supervisor_field(
                "pub struct State {\n    pub phase: PhaseId,\n}\n"
            ),
            "planted control: a State without it is not"
        );

        let found: Vec<String> = kept_lines_with_paths(
            &both_crates_rust_sources(),
            lines_naming_a_removed_state_record,
        )
        .into_iter()
        .map(|(path, line)| format!("{}: {line}", path.display()))
        .collect();
        assert_eq!(
            found,
            Vec::<String>::new(),
            "sources name a removed State supervisor record"
        );
        let state_rs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../devflow-core/src/state.rs");
        let state_source = std::fs::read_to_string(&state_rs).unwrap();
        assert!(
            state_source.contains("pub struct State {"),
            "state.rs no longer declares `pub struct State {{`, so the field check reads nothing"
        );
        assert!(
            !state_struct_has_a_supervisor_field(&state_source),
            "State still declares a `supervisor` field"
        );
    }

    /// The supervisor module's paths, split so this file holds neither as contiguous
    /// source text: the module scan reads this file too.
    const SUPERVISOR_MODULE_NEEDLES: [&str; 2] = [
        concat!("crate::", "supervisor"),
        concat!("devflow_core::", "supervisor"),
    ];

    /// The code lines of `source` (whole file, test modules included, lines whose
    /// trimmed start is `//` dropped) that name the supervisor module.
    fn code_lines_naming_the_supervisor_module(source: &str) -> Vec<String> {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .filter(|line| {
                SUPERVISOR_MODULE_NEEDLES
                    .iter()
                    .any(|needle| line.contains(needle))
            })
            .map(str::to_string)
            .collect()
    }

    /// Whether `path` is the supervisor module's own source: devflow-core's
    /// `supervisor.rs`, or any file under a `supervisor/` directory.
    fn in_the_supervisor_module(path: &Path) -> bool {
        path.ends_with("src/supervisor.rs")
            || path.parent().is_some_and(|dir| {
                dir.components()
                    .any(|part| part.as_os_str() == "supervisor")
            })
    }

    /// Plan check revision 3, warning 4: no code line of either crate, tests included,
    /// names the supervisor module outside its own files, so 52.2-24 can delete it
    /// without touching another file. Controls: a planted `use` line counts 1, a planted
    /// doc line naming it counts 0, and a planted code line in a file under a
    /// `supervisor/` path is skipped.
    #[test]
    fn no_code_outside_the_supervisor_module_names_it() {
        let [crate_needle, _] = SUPERVISOR_MODULE_NEEDLES;
        let planted_use = format!("use {crate_needle}::SocketPath;");
        assert_eq!(
            code_lines_naming_the_supervisor_module(&format!("{planted_use}\nfn f() {{}}\n")),
            vec![planted_use.clone()],
            "planted control: a code line counts 1"
        );
        assert_eq!(
            code_lines_naming_the_supervisor_module(&format!(
                "/// [`{crate_needle}::probe`]\nfn f() {{}}\n"
            ))
            .len(),
            0,
            "planted control: a doc line does not count"
        );
        let inside = Path::new("/repo/crates/devflow-core/src/supervisor/backstop.rs");
        let outside = Path::new("/repo/crates/devflow-core/src/stop.rs");
        let planted_files = [inside.to_path_buf(), outside.to_path_buf()];
        let kept: Vec<&PathBuf> = planted_files
            .iter()
            .filter(|path| !in_the_supervisor_module(path))
            .collect();
        assert_eq!(
            kept,
            vec![&outside.to_path_buf()],
            "planted control: a file under supervisor/ is skipped, another is read"
        );
        assert!(
            in_the_supervisor_module(Path::new("/repo/crates/devflow-core/src/supervisor.rs")),
            "planted control: the module root file is skipped"
        );

        let files: Vec<PathBuf> = both_crates_rust_sources()
            .into_iter()
            .filter(|path| !in_the_supervisor_module(path))
            .collect();
        assert!(
            files.iter().any(|path| path.ends_with("src/commands.rs")),
            "the scan must read this file, its own needles included"
        );
        let found: Vec<String> =
            kept_lines_with_paths(&files, code_lines_naming_the_supervisor_module)
                .into_iter()
                .map(|(path, line)| format!("{}: {line}", path.display()))
                .collect();
        assert_eq!(
            found,
            Vec::<String>::new(),
            "code outside the supervisor module names it"
        );
    }

    /// Whether `path` exists within 30 s, polled every 20 ms.
    fn wait_for_file(path: &Path) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            if path.exists() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        path.exists()
    }

    /// The variable a holder child of
    /// `doctor_phase_facts_read_the_runner_lock` reads: the project root whose runner
    /// lock it holds.
    const HOLDER_ROOT_ENV: &str = "RUNNER_LOCK_HOLDER_ROOT";

    /// D-12a: doctor's phase facts read the runner lock. A child process holds
    /// `.devflow/.runner-NN` through `claim_runner_lock`, and `build_phase_facts` reads
    /// the run live with that child's pid; control: once the child is reaped and the
    /// lock is free, the same call reads it not live. Runs in a process of its own
    /// (Pitfall 5: no other test's fork holds a copy of a lock descriptor here).
    #[test]
    fn doctor_phase_facts_read_the_runner_lock() {
        const NAME: &str = "commands::tests::doctor_phase_facts_read_the_runner_lock";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_runner_liveness_child(NAME, &[]);
            return;
        }
        let phase = PhaseId::new(4404);
        if let Some(root) = std::env::var_os(HOLDER_ROOT_ENV) {
            let root = PathBuf::from(root);
            let _hold = liveness::claim_runner_lock(&root, phase).expect("hold the runner lock");
            std::fs::write(root.join("held"), "").unwrap();
            assert!(
                wait_for_file(&root.join("release")),
                "the holder was never released"
            );
            return;
        }
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let state = State::new(phase, AgentKind::Claude, Mode::Supervise, root.into());
        let facts = |state: &State| {
            build_phase_facts(
                root,
                state.clone(),
                &mut std::collections::HashMap::new(),
                &[],
            )
        };
        let path_dir = tempfile::tempdir().unwrap();
        let holder = devflow_core::test_support::spawn_test_in_child(
            NAME,
            path_dir.path(),
            &[(HOLDER_ROOT_ENV, root.as_os_str())],
        );
        let held = wait_for_file(&root.join("held"));
        let live = held.then(|| facts(&state).runner);
        std::fs::write(root.join("release"), "").unwrap();
        let holder_pid = holder.id();
        let output = holder.wait_with_output().expect("reap the holder");
        crate::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
        assert!(held, "the holder never took the runner lock");
        assert_eq!(
            live,
            Some(Ok(RunnerLiveness::Live {
                pid: Some(holder_pid)
            })),
            "a held runner lock did not read live"
        );
        assert_eq!(
            facts(&state).runner,
            Ok(RunnerLiveness::NotLive),
            "control: a free runner lock read live"
        );
    }

    /// D-07 / D-12 as corrected (review agy-1): with the runner lock free, a unit that
    /// still reads `deactivating` is not a live-run refusal for the launch guard or for
    /// discard's liveness read; discard waits on it instead. Control: with the lock
    /// held both refuse, naming `devflow stop`.
    #[test]
    fn a_lock_free_stopping_unit_is_not_a_live_run_refusal() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let phase = PhaseId::new(4401);
        crate::service::double::set_unit_state(Ok("deactivating"));

        let launch_facts = |root: &Path| {
            GuardFacts::from_liveness(&runner_liveness(root, phase).expect("liveness reads"))
        };
        let launch = launch_guards(phase, &launch_facts(root));
        assert!(
            launch.is_ok(),
            "the launch guard returned a refusal for a lock-free deactivating unit: {launch:?}"
        );
        let liveness = phase_run_liveness(root, phase);
        assert!(
            matches!(liveness, PhaseRunLiveness::RunnerStopping { ref state, .. } if state == "deactivating"),
            "a lock-free deactivating unit reads RunnerStopping"
        );
        let discard = refuse_discard_of_a_live_run(phase, liveness);
        assert!(
            discard.is_ok(),
            "discard's liveness read returned a refusal for a lock-free deactivating unit: \
             {discard:?}"
        );

        let _hold = liveness::claim_runner_lock(root, phase).expect("hold the runner lock");
        let stop = format!("devflow stop --phase {phase}");
        let launch = launch_guards(phase, &launch_facts(root))
            .expect_err("control: the launch guard refuses under a held runner lock");
        assert!(launch.to_string().contains(&stop), "control: {launch}");
        let discard = refuse_discard_of_a_live_run(phase, phase_run_liveness(root, phase))
            .expect_err("control: discard refuses under a held runner lock");
        assert!(discard.to_string().contains(&stop), "control: {discard}");
    }

    /// D-12, D-26: each of `start`'s guard rows alone refuses with its own text and
    /// verb; facts that carry several rows get the earliest one; facts that carry none
    /// pass. The live row fires only for a held runner lock: with no live runner (a
    /// lock-free stopping unit reads so) and every other row empty, the guards pass.
    #[test]
    fn launch_guards_refuse_in_order() {
        let phase = PhaseId::new(4402);
        let discard = format!("devflow discard --phase {phase}");
        let stop = format!("devflow stop --phase {phase}");
        let resume = format!("devflow resume --phase {phase}");
        let live = Some(LiveRunner { pid: Some(4242) });
        let incomplete = StateFact::Incomplete { stage: Stage::Code };
        let unloadable = StateFact::Unloadable {
            path: PathBuf::from("/repo/.devflow/state-4402.json"),
            error: "not json".to_string(),
        };
        let gates = vec![PathBuf::from("/repo/.devflow/gates/4402-code.json")];
        let worktree = Some(PathBuf::from("/repo/.worktrees/phase-4402"));
        let branch = Some("feature/phase-4402".to_string());
        let base = "develop".to_string();
        let refusal = |facts: &GuardFacts| match launch_guards(phase, facts) {
            Ok(()) => panic!("no refusal for {facts:?}"),
            Err(err) => err.to_string(),
        };
        const LIVE: &str = "holds the runner lock";
        const FINISHED: &str = "finished (its Validate passed)";
        const INCOMPLETE: &str = "has an incomplete run at its code stage";
        const UNLOADABLE: &str = "does not load (not json)";
        const GATES: &str = "gate files are left with no run to answer them";
        const WORKTREE: &str = "the phase worktree /repo/.worktrees/phase-4402";
        const BRANCH: &str = "branch `feature/phase-4402`";

        let alone = [
            (
                "live",
                GuardFacts {
                    live_runner: live,
                    ..GuardFacts::default()
                },
                LIVE,
                &stop,
            ),
            (
                "finished",
                GuardFacts {
                    state: StateFact::Finished,
                    ..GuardFacts::default()
                },
                FINISHED,
                &discard,
            ),
            (
                "incomplete",
                GuardFacts {
                    state: incomplete.clone(),
                    ..GuardFacts::default()
                },
                INCOMPLETE,
                &resume,
            ),
            (
                "unloadable",
                GuardFacts {
                    state: unloadable.clone(),
                    ..GuardFacts::default()
                },
                UNLOADABLE,
                &discard,
            ),
            (
                "orphan gate files",
                GuardFacts {
                    orphan_gate_files: gates.clone(),
                    ..GuardFacts::default()
                },
                GATES,
                &discard,
            ),
            (
                "leftover worktree",
                GuardFacts {
                    leftover_worktree: worktree.clone(),
                    base: base.clone(),
                    ..GuardFacts::default()
                },
                WORKTREE,
                &discard,
            ),
            (
                "leftover branch",
                GuardFacts {
                    leftover_branch: branch.clone(),
                    base: base.clone(),
                    ..GuardFacts::default()
                },
                BRANCH,
                &discard,
            ),
        ];
        for (row, facts, marker, verb) in &alone {
            let text = refusal(facts);
            assert!(
                text.contains(marker),
                "row {row}: refusal lacks {marker:?}: {text}"
            );
            assert!(
                text.contains(verb.as_str()),
                "row {row}: refusal does not name {verb}: {text}"
            );
            assert!(
                !text.contains("--force"),
                "row {row}: refusal offers --force: {text}"
            );
        }

        // Several rows: the earliest wins. Each case drops the row before it.
        let every_row = GuardFacts {
            live_runner: live,
            state: incomplete.clone(),
            orphan_gate_files: gates.clone(),
            leftover_worktree: worktree.clone(),
            leftover_branch: branch.clone(),
            base: base.clone(),
        };
        let live_text = refusal(&every_row);
        assert!(live_text.contains(LIVE), "live is first: {live_text}");
        assert!(
            live_text.contains(&resume),
            "52.1 D-26: the live refusal over an incomplete run names resume after stop: {live_text}"
        );
        let after_live = GuardFacts {
            live_runner: None,
            ..every_row.clone()
        };
        let text = refusal(&after_live);
        assert!(
            text.contains(INCOMPLETE),
            "incomplete precedes gates: {text}"
        );
        assert!(!text.contains(GATES), "only the earliest row: {text}");
        let finished = GuardFacts {
            state: StateFact::Finished,
            ..after_live.clone()
        };
        assert!(
            refusal(&finished).contains(FINISHED),
            "finished precedes gates"
        );
        let unloadable_and_more = GuardFacts {
            state: unloadable,
            ..after_live.clone()
        };
        assert!(
            refusal(&unloadable_and_more).contains(UNLOADABLE),
            "unloadable precedes gates"
        );
        let after_state = GuardFacts {
            state: StateFact::ReservationOnly,
            ..after_live.clone()
        };
        let text = refusal(&after_state);
        assert!(
            text.contains(GATES),
            "a reservation-only state is no run: {text}"
        );
        assert!(!text.contains(WORKTREE), "only the earliest row: {text}");
        let after_gates = GuardFacts {
            orphan_gate_files: Vec::new(),
            ..after_state
        };
        let text = refusal(&after_gates);
        assert!(
            text.contains(WORKTREE),
            "the worktree precedes the branch: {text}"
        );
        assert!(!text.contains(BRANCH), "only the earliest row: {text}");

        // No row: the guards pass.
        assert!(
            launch_guards(phase, &GuardFacts::default()).is_ok(),
            "an empty set passes"
        );
        let reservation_only = GuardFacts {
            state: StateFact::ReservationOnly,
            ..GuardFacts::default()
        };
        assert!(
            launch_guards(phase, &reservation_only).is_ok(),
            "52.1 D-20: reservation-only is no run"
        );
        let stopping = GuardFacts::from_liveness(&RunnerLiveness::Stopping {
            unit: "devflow-x-4402.service".to_string(),
            state: "deactivating".to_string(),
        });
        assert!(
            launch_guards(phase, &stopping).is_ok(),
            "the live row fired for a lock-free stopping unit"
        );
    }

    /// Run the in-crate test `name` alone in a child process, with an empty `PATH` and
    /// `extra_env`, and return its output once it has passed.
    fn run_runner_liveness_child(name: &str, extra_env: &[(&str, &std::ffi::OsStr)]) -> String {
        let path_dir = tempfile::tempdir().unwrap();
        let output =
            crate::test_support::run_hermetic_test_in_child(name, path_dir.path(), extra_env);
        crate::test_support::assert_child_ran_exactly_one_passing_test(&output, name);
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    /// D-09, D-09a, D-20: every row of RESEARCH's status table, one word and one verb
    /// each, in a process of its own (the runner lock is held and probed here). No row
    /// names the removed `recover`. Control: a phase with no state, no gate file and a
    /// free lock has no row.
    #[test]
    fn status_words_follow_the_liveness_table() {
        const NAME: &str = "commands::tests::status_words_follow_the_liveness_table";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_runner_liveness_child(NAME, &[]);
            return;
        }
        use crate::service::double;
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let phase = PhaseId::new(4403);
        let (stop, logs, discard) = (
            format!("`devflow stop --phase {phase}`"),
            format!("`devflow logs --phase {phase} -f`"),
            format!("`devflow discard --phase {phase}`"),
        );
        let resume = format!("`devflow resume --phase {phase}`");
        let mut prepared = State::new(phase, AgentKind::Claude, Mode::Supervise, root.into());
        prepared.verification_baseline_captured = true;
        prepared.stage = Stage::Code;
        let loaded = |edit: &dyn Fn(&mut State)| {
            let mut state = prepared.clone();
            edit(&mut state);
            workflow::Existing::Loaded(state)
        };
        let row = |label: &str, existing: &workflow::Existing, word: &str, verbs: &[&str]| {
            let status = run_status(root, phase, existing)
                .unwrap_or_else(|| panic!("row `{label}`: no row"));
            let line = status.line(phase);
            assert_eq!(status.word(), word, "row `{label}`: {line}");
            assert!(line.starts_with(word), "row `{label}`: {line}");
            for verb in verbs {
                assert!(line.contains(verb), "row `{label}` lacks {verb}: {line}");
            }
            assert!(
                !line.contains("recover"),
                "row `{label}` names recover: {line}"
            );
        };

        let hold = liveness::claim_runner_lock(root, phase).expect("hold the runner lock");
        row("held", &loaded(&|_| {}), "live", &[&stop, &logs]);
        let gate = format!("`devflow gate approve {phase}`");
        let at_gate = loaded(&|state| state.gate_pending = true);
        row("held at a gate", &at_gate, "live", &[&gate, &stop]);
        row(
            "held, no state",
            &workflow::Existing::Missing,
            "live",
            &[&stop],
        );
        drop(hold);

        double::set_unit_state(Ok("deactivating"));
        row("stopping", &loaded(&|_| {}), "live (stopping)", &[&stop]);
        double::set_unit_state(Ok("inactive"));
        let finished = loaded(&|state| {
            state.stopped = true;
            state.finished = true;
        });
        row("finished", &finished, "finished", &[&discard]);
        let stopped = loaded(&|state| state.stopped = true);
        row("stopped", &stopped, "stopped", &[&resume, &discard]);
        row("crashed", &loaded(&|_| {}), "crashed", &[&resume, &discard]);
        let crashed_at_gate = loaded(&|state| state.gate_pending = true);
        row(
            "crashed at a gate",
            &crashed_at_gate,
            "crashed",
            &[&resume, &discard],
        );
        let reserved = loaded(&|state| state.verification_baseline_captured = false);
        row(
            "reservation-only",
            &reserved,
            "idle",
            &["devflow start --phase"],
        );

        assert_eq!(
            run_status(root, phase, &workflow::Existing::Missing),
            None,
            "control: nothing on disk, no row"
        );
        Gates::write_gate(
            root,
            phase,
            Stage::Plan,
            devflow_core::gates::GateKind::StageFailure,
            "left behind",
        )
        .unwrap();
        row(
            "orphan gates",
            &workflow::Existing::Missing,
            "debris",
            &[&discard],
        );

        let state_file = workflow::state_path(root, phase);
        std::fs::write(&state_file, "{ not json").unwrap();
        let unloadable = existing_state(root, phase);
        assert!(matches!(unloadable, workflow::Existing::Unloadable(_)));
        row("unloadable", &unloadable, "unloadable", &[&discard]);

        double::set_unit_state(Err("the bus went away (test double)"));
        row(
            "probe error",
            &loaded(&|_| {}),
            "error",
            &["the bus went away (test double)"],
        );
    }

    /// D-09a: the banner offers approve and reject for a gate whose waiter registration is
    /// held, and for the same gate with the waiter gone prints the debris line naming
    /// `devflow discard` and no answer (control). In a process of its own.
    #[test]
    fn the_gate_banner_offers_answers_only_for_a_live_waiter() {
        const NAME: &str = "commands::tests::the_gate_banner_offers_answers_only_for_a_live_waiter";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_runner_liveness_child(NAME, &[]);
            return;
        }
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let phase = PhaseId::new(4404);
        Gates::write_gate(
            root,
            phase,
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            "stage code failed",
        )
        .unwrap();
        let open = Gates::list_open(root);
        assert_eq!(open.len(), 1, "{open:?}");
        let gate_id = GateId::parse(&open[0].gate_id).unwrap();
        let approve = format!("devflow gate approve {phase} --stage code");
        let discard = format!("`devflow discard --phase {phase}`");

        let waiter = Gates::register_waiter(root, phase, Stage::Code, &gate_id).unwrap();
        let banner = render_pending_gate_banner(root, &open, 10_000).unwrap();
        assert!(
            banner.contains(&approve),
            "no approve with a live waiter: {banner}"
        );
        assert!(banner.contains("devflow gate reject"), "{banner}");
        assert!(!banner.contains(&discard), "{banner}");
        drop(waiter);

        let banner = render_pending_gate_banner(root, &open, 10_000).unwrap();
        assert!(
            !banner.contains(&approve) && !banner.contains("devflow gate reject"),
            "banner offered approve without a waiter: {banner}"
        );
        assert!(banner.contains("debris"), "{banner}");
        assert!(banner.contains(&discard), "{banner}");
        assert!(
            banner.contains("stage code failed"),
            "the context stays: {banner}"
        );
    }

    /// D-10 (review r2 codex-7), in a process of its own: a runner held between its
    /// agent's exit and its advance. The exit file alone does not end
    /// [`post_exit_line`]'s wait; the advance that commits a pending gate does, with the
    /// gate line. Controls: the runner gone with the state still running Plan returns the
    /// crashed line, and with nothing changing the wait returns the live line at its
    /// bound, so it never hangs.
    #[test]
    fn logs_follow_waits_past_the_exit_file_until_the_runner_advances() {
        const NAME: &str =
            "commands::tests::logs_follow_waits_past_the_exit_file_until_the_runner_advances";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_runner_liveness_child(NAME, &[]);
            return;
        }
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path().to_path_buf();
        let phase = PhaseId::new(4405);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Supervise, root.clone());
        state.verification_baseline_captured = true;
        state.stage = Stage::Plan;
        workflow::save_state(&state).unwrap();
        let exit = agent_result::exit_code_path(&root, phase);
        std::fs::write(&exit, "0").unwrap();
        let wait_on = |unit_state: &'static str, bound: std::time::Duration| {
            let (sent, received) = std::sync::mpsc::channel();
            let root = root.clone();
            std::thread::spawn(move || {
                crate::service::double::set_unit_state(Ok(unit_state));
                let _ = sent.send(post_exit_line(&root, phase, Stage::Plan, bound));
            });
            received
        };
        let within = std::time::Duration::from_secs(2);

        let hold = liveness::claim_runner_lock(&root, phase).expect("hold the runner lock");
        let waiting = wait_on("active", std::time::Duration::from_secs(10));
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(
            waiting.try_recv().is_err(),
            "follow ended on the exit file before the advance"
        );
        let mut advanced = state.clone();
        advanced.gate_pending = true;
        workflow::save_state(&advanced).unwrap();
        let line = waiting
            .recv_timeout(within)
            .expect("no gate line after the advance")
            .unwrap();
        assert!(
            line.contains(&format!("`devflow gate approve {phase}`")),
            "no gate line after the advance: {line}"
        );

        workflow::save_state(&state).unwrap();
        let waiting = wait_on("inactive", std::time::Duration::from_secs(10));
        std::thread::sleep(std::time::Duration::from_millis(700));
        drop(hold);
        let line = waiting
            .recv_timeout(within)
            .expect("no crashed line after the runner went")
            .unwrap();
        assert!(
            line.starts_with("crashed")
                && line.contains(&format!("`devflow resume --phase {phase}`"))
                && line.contains(&format!("`devflow discard --phase {phase}`")),
            "no crashed line after the runner went: {line}"
        );

        let _hold = liveness::claim_runner_lock(&root, phase).expect("hold the lock again");
        let waiting = wait_on("active", std::time::Duration::from_secs(1));
        let line = waiting
            .recv_timeout(within)
            .expect("post_exit_line did not return at its bound")
            .unwrap();
        assert!(
            line.starts_with("live") && line.contains("in plan"),
            "post_exit_line did not return the live line at its bound: {line}"
        );
    }

    /// A start whose unit launch failed names what it created and kept, and the verb that
    /// removes it, since the next `start` refuses a leftover worktree or branch. Control:
    /// a start without a worktree names only the branch.
    #[test]
    fn a_failed_launch_names_what_it_kept_and_discard() {
        let phase = PhaseId::new(4405);
        let with_worktree = failed_launch_message(phase, true, "systemd-run failed");
        assert!(
            with_worktree.contains("systemd-run failed")
                && with_worktree.contains("worktree and branch")
                && with_worktree.contains("devflow discard --phase 4405"),
            "the failed-launch line does not name what was kept and discard: {with_worktree}"
        );
        let branch_only = failed_launch_message(phase, false, "systemd-run failed");
        assert!(
            branch_only.contains("its branch") && !branch_only.contains("worktree"),
            "control: a start without a worktree names only its branch: {branch_only}"
        );
    }

    /// A launch refuses, before anything is written, under a user manager older than
    /// systemd 254, whose `systemd-run` lacks `--expand-environment=no`. Control: 254
    /// itself launches.
    #[test]
    fn a_user_manager_older_than_254_refuses_a_launch() {
        use crate::service::double;
        let phase = PhaseId::new(4404);
        double::set_manager_version(253);
        let refused = refuse_without_a_user_manager(phase, LaunchVerb::Start);
        assert!(
            matches!(&refused, Err(CliError::Message(why))
                if why.contains("253 is older than 254") && why.contains("nothing was written")),
            "a systemd 253 user manager did not refuse the launch: {refused:?}"
        );
        double::set_manager_version(crate::service::MIN_SYSTEMD_VERSION);
        let launched = refuse_without_a_user_manager(phase, LaunchVerb::Resume);
        assert!(
            launched.is_ok(),
            "control: systemd 254 launches: {launched:?}"
        );
        double::set_manager(None);
    }

    /// D-07: runner_liveness's answers, one row each, in a process of its own (the
    /// runner lock is held and probed here). A held lock is Live whatever the unit
    /// reads; with the lock free a deactivating unit is Stopping and an inactive one
    /// NotLive; an Absent manager is NotLive and Unreachable or Failed an error, none
    /// of the three reading the unit (agy-2, codex-2/pi-8); an unprobeable lock is an
    /// error (control).
    #[test]
    fn runner_liveness_reads_live_stopping_and_not_live() {
        const NAME: &str = "commands::tests::runner_liveness_reads_live_stopping_and_not_live";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_runner_liveness_child(NAME, &[]);
            return;
        }
        use crate::service::{ManagerUnavailableKind, double};
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let phase = PhaseId::new(4402);
        let row = |label: &str,
                   manager: Option<ManagerUnavailableKind>,
                   unit: &str,
                   expected: Result<RunnerLiveness, ()>,
                   unit_read: bool| {
            double::set_manager(manager);
            double::set_unit_state(Ok(unit));
            let reads = double::unit_reads();
            let answer = runner_liveness(root, phase);
            assert_eq!(
                answer.as_ref().map_err(|_| ()),
                expected.as_ref().map_err(|_| ()),
                "row `{label}`: runner_liveness answered {answer:?}"
            );
            assert_eq!(
                double::unit_reads() - reads,
                usize::from(unit_read),
                "row `{label}`: unit reads"
            );
        };

        let hold = liveness::claim_runner_lock(root, phase).expect("hold the runner lock");
        row(
            "lock held",
            None,
            "deactivating",
            Ok(RunnerLiveness::Live {
                pid: Some(std::process::id()),
            }),
            false,
        );
        drop(hold);
        let unit = crate::service::unit_name(root, phase).unwrap();
        row(
            "lock free, deactivating",
            None,
            "deactivating",
            Ok(RunnerLiveness::Stopping {
                unit,
                state: "deactivating".to_string(),
            }),
            true,
        );
        row(
            "lock free, inactive",
            None,
            "inactive",
            Ok(RunnerLiveness::NotLive),
            true,
        );
        row(
            "manager absent",
            Some(ManagerUnavailableKind::Absent),
            "deactivating",
            Ok(RunnerLiveness::NotLive),
            false,
        );
        row(
            "manager unreachable",
            Some(ManagerUnavailableKind::Unreachable),
            "deactivating",
            Err(()),
            false,
        );
        row(
            "manager failed",
            Some(ManagerUnavailableKind::Failed),
            "deactivating",
            Err(()),
            false,
        );

        let unprobeable = tempfile::tempdir().unwrap();
        std::fs::write(unprobeable.path().join(".devflow"), "not a dir\n").unwrap();
        double::set_manager(None);
        let answer = runner_liveness(unprobeable.path(), phase);
        assert!(
            answer
                .as_ref()
                .is_err_and(|why| why.contains("could not be probed")),
            "row `unprobeable lock`: runner_liveness answered {answer:?}"
        );
    }

    /// D-03, agy-2: with no user manager (positive evidence, Absent) the runner lock
    /// alone answers: NotLive, no unit read, and a note on stderr saying so. Runs in a
    /// child, whose stderr libtest does not capture, so the parent can read the note.
    /// The held-lock row of `runner_liveness_reads_live_stopping_and_not_live` is its
    /// control.
    #[test]
    fn an_absent_user_manager_reads_the_lock_alone_and_says_why() {
        const NAME: &str =
            "commands::tests::an_absent_user_manager_reads_the_lock_alone_and_says_why";
        if !devflow_core::test_support::in_child_test(NAME) {
            let stderr = run_runner_liveness_child(
                NAME,
                &[("RUST_TEST_NOCAPTURE", std::ffi::OsStr::new("1"))],
            );
            assert!(
                stderr.contains("no systemd user manager runs")
                    && stderr.contains("runner lock only"),
                "no runner-lock-only note on the child's stderr:\n{stderr}"
            );
            return;
        }
        use crate::service::{ManagerUnavailableKind, double};
        let repo = tempfile::tempdir().unwrap();
        double::set_manager(Some(ManagerUnavailableKind::Absent));
        double::set_unit_state(Ok("deactivating"));
        let reads = double::unit_reads();
        let answer = runner_liveness(repo.path(), PhaseId::new(4403));
        assert_eq!(
            answer,
            Ok(RunnerLiveness::NotLive),
            "an absent manager did not read NotLive"
        );
        assert_eq!(
            double::unit_reads(),
            reads,
            "the unit was read with no manager"
        );
    }

    /// D-03 (review agy-6; moved from run_liveness_e2e in round 3): an Absent user
    /// manager refuses `start` before the phase holder, naming lingering and printing
    /// systemctl's stderr. Control: an Unreachable one names `XDG_RUNTIME_DIR` and gives
    /// no lingering hint. Neither launches.
    #[test]
    fn start_refuses_an_absent_user_manager_naming_lingering() {
        use crate::service::{ManagerUnavailableKind, double};
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let phase = PhaseId::new(4406);
        let start_once = || {
            start(
                root,
                phase,
                AgentKind::Codex,
                Mode::Supervise,
                false,
                false,
                None,
                FallbackSource::FlagOrConfig(None),
            )
            .map_err(|err| err.to_string())
        };

        double::set_manager(Some(ManagerUnavailableKind::Absent));
        let absent = start_once().expect_err("start launched with no user manager");
        assert!(
            absent.contains("no systemd user manager runs"),
            "no user manager named: {absent}"
        );
        assert!(
            absent.contains("loginctl enable-linger") && absent.contains("No medium found"),
            "lingering hint missing for an absent manager: {absent}"
        );
        assert!(
            !liveness::phase_holder_path(root, phase).exists(),
            "start took the phase holder before the manager check"
        );

        double::set_manager(Some(ManagerUnavailableKind::Unreachable));
        let unreachable = start_once().expect_err("start launched with an unreachable manager");
        assert!(
            unreachable.contains("XDG_RUNTIME_DIR") && !unreachable.contains("enable-linger"),
            "lingering hint on an unreachable bus: {unreachable}"
        );
        assert!(
            double::launched_units().is_empty(),
            "start launched a runner: {:?}",
            double::launched_units()
        );
    }

    #[test]
    fn gate_response_message_does_not_claim_pickup_after_the_waiter_goes() {
        let live = response_pickup_message(WaiterStatus::Live);
        assert!(live.contains("may pick up"), "live message: {live}");
        assert!(!live.contains("lock"), "live message: {live}");
        let gone = response_pickup_message(WaiterStatus::Absent);
        assert!(
            gone.contains("no confirmed waiter"),
            "no-holder message: {gone}"
        );
        assert!(
            !gone.contains("will pick this up"),
            "no-holder message: {gone}"
        );
    }

    /// CUT-01 (Pitfall 2): "never prepared" is this run's own flag, not `stage == Define`. A
    /// fresh state is never prepared; a Plan run that captured its baseline is prepared, and
    /// so is not reported as never launched (control).
    #[test]
    fn run_never_prepared_is_true_for_a_fresh_state_only() {
        let fresh = State::new(
            PhaseId::new(5310),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        assert!(
            run_never_prepared(&fresh),
            "a fresh state was never prepared"
        );

        let mut prepared = fresh.clone();
        prepared.stage = Stage::Plan;
        prepared.verification_baseline_captured = true;
        assert!(
            !run_never_prepared(&prepared),
            "control: a prepared Plan run with no supervisor is not never-launched"
        );
    }

    /// The event log is project-wide and append-only, so an earlier run's `workflow_started`
    /// and `stage_launched` for the same phase must not make a fresh state read as launched.
    #[test]
    fn run_never_prepared_ignores_an_earlier_runs_events() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5311);
        devflow_core::events::emit(
            root,
            phase,
            "workflow_started",
            serde_json::json!({ "agent": "claude", "mode": "auto" }),
        );
        devflow_core::events::emit(
            root,
            phase,
            "stage_launched",
            serde_json::json!({ "stage": "plan" }),
        );
        assert!(
            devflow_core::events::has_event_for_phase(root, phase, "workflow_started")
                && devflow_core::events::has_event_for_phase(root, phase, "stage_launched"),
            "control: the earlier run's events are on disk"
        );

        let fresh = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        assert!(
            run_never_prepared(&fresh),
            "an earlier run's events must not make a fresh state read as prepared"
        );
    }

    /// Doctor's missing-branch check skips a run that was never prepared, and only that run:
    /// the same facts with `unprepared_restart: None` report the missing branch.
    #[test]
    fn check_missing_branch_skips_a_never_launched_run_only() {
        let facts = |unprepared_restart: Option<String>| PhaseFacts {
            unprepared_restart,
            stage: Stage::Plan,
            feature_branch_exists: false,
            ..doctor_reconciliation::agreeing_facts(PhaseId::new(5312))
        };
        let unprepared = facts(Some("devflow start --phase 5312".to_string()));
        assert!(
            check_missing_branch(&unprepared).is_none(),
            "a never-prepared run has no branch to miss"
        );

        let prepared = facts(None);
        let finding = check_missing_branch(&prepared).expect("a prepared run's branch is missing");
        assert_eq!(finding.severity, Severity::Warn);
        assert!(
            finding.detail.contains("feature/phase-5312"),
            "{}",
            finding.detail
        );
    }

    /// A driver whose only artifact-requiring stage is `stage`.
    struct ArtifactAtStage(Stage);

    impl agents::AgentDriver for ArtifactAtStage {
        fn name(&self) -> &'static str {
            "test-artifact-at-stage"
        }
        fn build_command(
            &self,
            _phase: PhaseId,
            _prompt: &str,
            _roots: &[PathBuf],
        ) -> (&'static str, Vec<String>) {
            ("true", Vec::new())
        }
        fn render_prompt(&self, intent: &devflow_core::prompt::StageIntent) -> String {
            devflow_core::prompt::render_claude_style(intent)
        }
        fn interactivity_mode(&self, stage: Stage) -> agents::InteractivityMode {
            if stage == self.0 {
                agents::InteractivityMode::RequiresExistingArtifact
            } else {
                agents::InteractivityMode::HeadlessSafe
            }
        }
    }

    /// `start`'s CONTEXT.md probe asks the driver about the first stage a run launches, Plan.
    /// A driver that needs an artifact only at a later stage is not asked about (control).
    #[test]
    fn start_context_probe_asks_the_driver_about_plan() {
        assert!(start_requires_phase_context(&ArtifactAtStage(Stage::Plan)));
        assert!(
            !start_requires_phase_context(&ArtifactAtStage(Stage::Validate)),
            "control: a requirement of a later stage is not probed at start"
        );
        assert!(!start_requires_phase_context(&ArtifactAtStage(Stage::Code)));
    }

    #[test]
    fn gate_sweep_leaves_a_no_waiter_gate_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4802);
        Gates::write_gate(
            root,
            phase,
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            "paused",
        )
        .unwrap();
        gate_sweep(Some(0), false, Some(root.to_path_buf())).unwrap();
        assert!(gate_answer_files(root, phase, Stage::Code).is_empty());
    }

    /// Unix epoch seconds now, for a sweep step whose gate has no not-before
    /// time.
    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// D-27.2, T-52-46: an aged rate-limit gate whose not-before time is still
    /// ahead is not reaped, even with a live waiter; the same gate at its
    /// not-before time is. Control in the same sweep: an ordinary aged gate
    /// with a live waiter is reaped. The time is injected, never waited on.
    #[test]
    fn gate_sweep_skips_a_rate_limit_gate_before_its_reset() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let stage = Stage::Code;
        let (limited, ordinary) = (PhaseId::new(5321), PhaseId::new(5322));
        let reset = i64::try_from(now_secs()).unwrap() + 3600;
        let not_before = reset + devflow_core::reset_time::RATE_LIMIT_RELAUNCH_MARGIN_SECS;
        let (limited_gate, limited_registration) = Gates::open_registered(
            root,
            limited,
            stage,
            devflow_core::gates::GateLabel::rate_limit(Some(reset)),
            "rate limited",
        )
        .unwrap();
        let (ordinary_gate, ordinary_registration) = Gates::open_registered(
            root,
            ordinary,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "failed",
        )
        .unwrap();
        let before = u64::try_from(not_before - 1).unwrap();

        gate_sweep_at(Some(0), false, Some(root.to_path_buf()), before).unwrap();

        assert_eq!(
            Gates::read_answer(root, limited, stage, &limited_gate.gate_id),
            AnswerRead::None,
            "a rate-limit gate is not reaped one second before its not-before time"
        );
        assert!(
            matches!(
                Gates::read_answer(root, ordinary, stage, &ordinary_gate.gate_id),
                AnswerRead::Bound(answer) if answer.response.abort
            ),
            "control: the ordinary aged gate is reaped"
        );

        let at = u64::try_from(not_before).unwrap();
        gate_sweep_at(Some(0), false, Some(root.to_path_buf()), at).unwrap();

        assert!(
            matches!(
                Gates::read_answer(root, limited, stage, &limited_gate.gate_id),
                AnswerRead::Bound(answer) if answer.response.abort
            ),
            "at its not-before time the rate-limit gate is reaped"
        );
        limited_registration.finish();
        ordinary_registration.finish();
    }

    #[test]
    fn gate_sweep_preview_uses_the_no_waiter_matrix() {
        for stage in [Stage::Code, Stage::Validate] {
            assert!(
                !gate_sweep_may_reap(stage, false, None, 0),
                "{stage} with no waiter"
            );
            assert!(
                gate_sweep_may_reap(stage, true, None, 0),
                "{stage} with a waiter"
            );
        }
    }

    #[test]
    fn gate_respond_with_no_registered_waiter_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4804);
        Gates::write_gate(
            root,
            phase,
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            "paused",
        )
        .unwrap();
        let err = gate_respond(
            root,
            phase,
            Some(Stage::Code),
            GateAnswerKind::Reject,
            Some("abort".into()),
            None,
        )
        .expect_err("an unregistered gate must not receive a response");
        assert!(
            err.to_string().contains("devflow resume --phase 4804"),
            "repair must name the concrete phase: {err}"
        );
        assert!(gate_answer_files(root, phase, Stage::Code).is_empty());
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
        Gates::open_gate(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g1",
        )
        .unwrap();
        let g1_entry = listed_gate(root, phase);
        Gates::close(root, phase, stage).unwrap();
        let (g2, registration) = Gates::open_registered(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g2",
        )
        .unwrap();
        let g2 = g2.gate_id;

        assert_eq!(
            sweep_one_gate(root, &g1_entry, 1, 0, false, now_secs()).unwrap(),
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
            sweep_one_gate(root, &g2_entry, 1, 0, false, now_secs()).unwrap(),
            SweepStep::Reaped
        );
        let AnswerRead::Bound(answer) = Gates::read_answer(root, phase, stage, &g2) else {
            panic!("the sweep answers the gate it observed");
        };
        assert!(!answer.response.approved, "a reap never approves (T-23-41)");
        registration.finish();
    }

    /// A gate file naming an empty id can be reported, and nothing else: no answer
    /// can bind to it. The sweep skips it before reading liveness.
    #[test]
    fn a_gate_with_no_gate_id_is_reported_and_never_answered() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(4912), Stage::Code);
        state_pending_at(root, phase, stage);
        Gates::open_gate(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap();
        let mut entry = listed_gate(root, phase);
        entry.gate_id = String::new();
        let file = GateFile {
            phase,
            stage,
            context: entry.context.clone(),
            timestamp: entry.timestamp.clone(),
            gate_id: String::new(),
            gate_kind: None,
            gsd_status: None,
            not_before: None,
        };
        std::fs::write(
            Gates::gate_path(root, phase, stage),
            serde_json::to_string_pretty(&file).unwrap(),
        )
        .unwrap();

        assert_eq!(
            sweep_one_gate(root, &entry, 1, 0, false, now_secs()).unwrap(),
            SweepStep::Skipped
        );
        assert!(gate_answer_files(root, phase, stage).is_empty());
    }

    /// Open G1, close it, and open G2 at the same stage with a held registration:
    /// the shape in which a writer that observed G1 could answer G2.
    fn g1_replaced_by_registered_g2(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
    ) -> (GateId, GateId, WaiterRegistration) {
        let g1 = Gates::open_gate(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g1",
        )
        .unwrap()
        .gate_id;
        Gates::close(root, phase, stage).unwrap();
        let (g2, registration) = Gates::open_registered(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g2",
        )
        .unwrap();
        (g1, g2.gate_id, registration)
    }

    #[test]
    fn gate_respond_publishes_only_for_the_observed_gate_id() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(4901), Stage::Code);
        let (g1, g2, registration) = g1_replaced_by_registered_g2(root, phase, stage);

        let err = gate_respond(
            root,
            phase,
            Some(stage),
            GateAnswerKind::Approve,
            None,
            Some(g1.to_string()),
        )
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

        gate_respond(
            root,
            phase,
            Some(stage),
            GateAnswerKind::Approve,
            None,
            None,
        )
        .unwrap();

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
        let (opened, registration) = Gates::open_registered(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap();

        let err = gate_respond(
            root,
            phase,
            Some(stage),
            GateAnswerKind::Approve,
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

    /// `gate list` shows a gate's context as far as the pending-gate banner
    /// does, so an idle-timeout cause clause past the 100th character is not cut
    /// from the list; past the shared bound both truncate identically.
    #[test]
    fn gate_list_shows_the_context_as_far_as_the_banner_does() {
        let cause =
            "idle timeout (terminal_reason: max_turns; origin: tool_use; errors: quota exceeded)";
        let context = format!(
            "[never-silent] stage code failed: {}{cause}: no output for 900s",
            "x".repeat(80)
        );
        assert!(context.chars().count() > 100 && context.chars().count() <= 261);
        let dir = tempfile::tempdir().unwrap();
        Gates::write_gate(
            dir.path(),
            PhaseId::new(5108),
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            &context,
        )
        .unwrap();
        let open = Gates::list_open(dir.path());

        let list = render_gate_list(&open);
        let row = render_all_roots_gate_row(Path::new("/tmp/some-root"), &open[0], 10_000);
        let banner = render_pending_gate_banner(dir.path(), &open, 10_000).unwrap();
        for (rendering, text) in [
            ("list", &list),
            ("all-roots row", &row),
            ("banner", &banner),
        ] {
            assert!(
                text.contains(&context),
                "{rendering} cut the cause clause: {text}"
            );
        }

        let long = format!("{context} {}", "y".repeat(400));
        Gates::write_gate(
            dir.path(),
            PhaseId::new(5108),
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            &long,
        )
        .unwrap();
        let open = Gates::list_open(dir.path());
        let shown = render_gate_context(&long, GATE_CONTEXT_DISPLAY_CHARS);
        assert!(shown.contains(&context), "{shown}");
        assert!(render_gate_list(&open).contains(&shown));
        assert!(
            render_pending_gate_banner(dir.path(), &open, 10_000)
                .unwrap()
                .contains(&shown)
        );
    }

    /// When the recorded delivery canary refuses the run's agent, the context the
    /// stage-failure gate writer builds leads with the routers' relaunch-refused
    /// sentence, and its way on (`devflow resume --phase N --agent <other>`) stays
    /// visible in the pending-gate banner, `gate list` and the all-roots row even when
    /// the reason is long enough to be capped. Control: a run whose recorded canary
    /// admits its agent names no `--agent <other>`.
    #[test]
    fn the_failure_gate_way_on_survives_the_banner_and_gate_list_for_a_long_reason() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new(
            PhaseId::new(9999),
            AgentKind::Antigravity,
            Mode::Auto,
            dir.path().to_path_buf(),
        );
        state.canary = Some(devflow_core::canary::CanaryOutcome::Absent);
        let reason = format!(
            "the handoff to antigravity could not relaunch the code stage: {}",
            "canary refusal detail ".repeat(20)
        );
        assert!(reason.chars().count() > 400);
        let way_on = "`devflow resume --phase 9999 --agent <other>`";

        let context = crate::pipeline_outcomes::failure_gate_context(
            &state,
            Stage::Code,
            Some(reason.clone()),
        );
        let refusal = crate::pipeline_launch::recorded_canary_refusal(&state);
        let sentence = crate::pipeline_outcomes::way_on_sentence(
            devflow_core::gates::GateKind::StageFailure,
            PhaseId::new(9999),
            Stage::Code,
            refusal.as_deref(),
        );
        assert!(
            context.starts_with(&format!("[never-silent] {sentence}")),
            "the writer leads with the routers' sentence: {context}"
        );
        assert!(sentence.contains(way_on), "{sentence}");
        Gates::write_gate(
            dir.path(),
            PhaseId::new(9999),
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            &context,
        )
        .unwrap();
        let open = Gates::list_open(dir.path());
        let list = render_gate_list(&open);
        let row = render_all_roots_gate_row(Path::new("/tmp/some-root"), &open[0], 10_000);
        let banner = render_pending_gate_banner(dir.path(), &open, 10_000).unwrap();
        for (rendering, text) in [
            ("list", &list),
            ("all-roots row", &row),
            ("banner", &banner),
        ] {
            assert!(text.contains(way_on), "{rendering} cut the way on: {text}");
        }

        state.canary = Some(devflow_core::canary::CanaryOutcome::Confirmed);
        let admitted =
            crate::pipeline_outcomes::failure_gate_context(&state, Stage::Code, Some(reason));
        assert!(
            !admitted.contains("--agent <other>"),
            "control: an admitting canary names no way on: {admitted}"
        );
    }

    /// The idle-timeout cause clause, at the longest length `devflow-core` budgets for it
    /// (`MAX_IDLE_TIMEOUT_CLAUSE_CHARS`), closes inside what the banner and `gate list`
    /// show (`GATE_CONTEXT_DISPLAY_CHARS`) after the reason cap and behind the longest
    /// failure prefix (the Validate stage's way-on sentence and failure words), however
    /// long the text after it. The bound is derived from the constants, so changing
    /// either one fails this test until the budget is re-derived. Control: a clause 24
    /// characters past the budget is cut.
    #[test]
    fn the_budgeted_idle_timeout_clause_closes_inside_the_gate_display_bound() {
        let clause_close = |clause_chars: usize| {
            let clause = "c".repeat(clause_chars);
            let reason = format!(
                "idle timeout ({clause}): no output for 900s {}",
                "z".repeat(400)
            );
            let dir = tempfile::tempdir().unwrap();
            let state = State::new(
                PhaseId::new(9999),
                AgentKind::Claude,
                Mode::Auto,
                dir.path().to_path_buf(),
            );
            let context = crate::pipeline_outcomes::failure_gate_context(
                &state,
                Stage::Validate,
                Some(reason),
            );
            assert!(
                context.starts_with("[never-silent] Answers — approve: retries the validate stage")
                    && context.contains(" stage validate failed: idle timeout ("),
                "{context}"
            );
            Gates::write_gate(
                dir.path(),
                PhaseId::new(9999),
                Stage::Validate,
                devflow_core::gates::GateKind::StageFailure,
                &context,
            )
            .unwrap();
            let open = Gates::list_open(dir.path());
            let closed = format!("{clause}): ");
            [
                render_gate_list(&open),
                render_all_roots_gate_row(Path::new("/tmp/some-root"), &open[0], 10_000),
                render_pending_gate_banner(dir.path(), &open, 10_000).unwrap(),
            ]
            .map(|text| text.contains(&closed))
        };

        let budget = devflow_core::agent_result::MAX_IDLE_TIMEOUT_CLAUSE_CHARS;
        assert_eq!(
            clause_close(budget),
            [true; 3],
            "list, all-roots row, banner"
        );
        assert_eq!(
            clause_close(budget + 24),
            [false; 3],
            "control: a clause past the budget is cut"
        );
    }

    #[test]
    fn gate_list_and_show_print_the_gate_id() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let opened = Gates::open_gate(
            root,
            PhaseId::new(4903),
            Stage::Validate,
            devflow_core::gates::GateKind::StageFailure,
            "ctx",
        )
        .unwrap();
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
        let (_, registration) = Gates::open_registered(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap();
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
    /// still stored.
    #[test]
    fn gate_respond_answers_a_registered_waiter_and_refuses_when_none_is_registered() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let (registered, stage) = (PhaseId::new(5001), Stage::Code);
        let (opened, registration) = Gates::open_registered(
            root,
            registered,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap();
        gate_respond(
            root,
            registered,
            Some(stage),
            GateAnswerKind::Approve,
            None,
            None,
        )
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
        Gates::open_gate(
            root,
            beside_a_lock,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap();
        plant_live_lock_record(root, beside_a_lock);
        let refused = gate_respond(
            root,
            beside_a_lock,
            Some(stage),
            GateAnswerKind::Approve,
            None,
            None,
        )
        .expect_err("a live lock record is no waiter")
        .to_string();
        assert!(refused.contains("no registered waiter"), "{refused}");
        assert!(gate_answer_files(root, beside_a_lock, stage).is_empty());

        let released = PhaseId::new(5002);
        let (_, registration) = Gates::open_registered(
            root,
            released,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap();
        registration.finish();
        let refused = gate_respond(
            root,
            released,
            Some(stage),
            GateAnswerKind::Approve,
            None,
            None,
        )
        .expect_err("a finished registration is no waiter")
        .to_string();
        assert!(refused.contains("no registered waiter"), "{refused}");
        assert!(refused.contains("no response was written"), "{refused}");
        assert!(gate_answer_files(root, released, stage).is_empty());
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

    /// `GateLockTimeout`: a gate writer gives up within the bound and writes nothing,
    /// and is never told there is no waiter. Control: with the lock free, the same
    /// answer lands.
    #[test]
    fn a_gate_writer_gives_up_on_a_held_gate_lock_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let stage = Stage::Code;

        let phase = PhaseId::new(5005);
        let (opened, registration) = Gates::open_registered(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap();
        let gate_lock = hold_gate_lock(root, phase);
        let started = std::time::Instant::now();
        let error = gate_respond(
            root,
            phase,
            Some(stage),
            GateAnswerKind::Approve,
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

        drop(gate_lock);
        gate_respond(
            root,
            phase,
            Some(stage),
            GateAnswerKind::Approve,
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
        let (live_gate, registration) = Gates::open_registered(
            root,
            live,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap();
        let (crashed_gate, crashed_registration) = Gates::open_registered(
            root,
            crashed,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap();
        drop(crashed_registration);
        let crashed_file = Gates::waiter_path(root, crashed, stage, &crashed_gate.gate_id);
        wait_until_unheld(&crashed_file);
        assert!(crashed_file.exists(), "a crash leaves its file behind");

        gate_sweep(Some(0), false, Some(root.to_path_buf())).unwrap();

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

    /// D-04: `doctor`'s fact is the registration. A lock record plays no part.
    #[test]
    fn doctor_reads_the_waiter_from_the_registration() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(5014), Stage::Code);
        state_pending_at(root, phase, stage);
        plant_live_lock_record(root, phase);
        let (_, registration) = Gates::open_registered(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap();

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
        state_pending_at(root, phase, Stage::Plan);
        let (opened, registration) = Gates::open_registered(
            root,
            phase,
            Stage::Plan,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap();
        let gate_id = opened.gate_id.as_str();

        for waiter in doctor_json_gate_waiters(root) {
            assert_eq!(
                waiter,
                serde_json::json!({"stage": "plan", "gate_id": gate_id, "status": "live"})
            );
        }

        registration.finish();
        for waiter in doctor_json_gate_waiters(root) {
            assert_eq!(
                waiter,
                serde_json::json!({"stage": "plan", "gate_id": gate_id, "status": "absent"})
            );
        }

        let no_gate = tempfile::tempdir().unwrap();
        state_pending_at(no_gate.path(), phase, Stage::Plan);
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
        let unwaited = Gates::open_gate(
            root,
            phase,
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            "no waiter",
        )
        .unwrap();
        let (waited, registration) = Gates::open_registered(
            root,
            phase,
            Stage::Validate,
            devflow_core::gates::GateKind::StageFailure,
            "waiter",
        )
        .unwrap();

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

    /// T-50.1-70: a held waiter or phase-holder file protects its holder from the stray
    /// reaper, and an unheld one protects nothing. An older build's held launch reservation
    /// (`supervisor-*.live`) protects nothing: no current build writes one.
    #[test]
    fn registry_reachable_pids_includes_held_waiter_and_holder_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let (phase, stage) = (PhaseId::new(5015), Stage::Code);
        let pid = std::process::id();
        let roots = std::slice::from_ref(&root);
        assert!(
            !registry_reachable_pids(roots).contains(&pid),
            "nothing held"
        );

        let live_path = root.join(".devflow/supervisor-aaaa0000aaaa5015.live");
        std::fs::create_dir_all(live_path.parent().unwrap()).unwrap();
        let reservation = liveness::Hold::take(&live_path).unwrap();
        assert!(
            !registry_reachable_pids(roots).contains(&pid),
            "an older build's held launch reservation is not a holder any build reads"
        );
        drop(reservation);
        wait_until_unheld(&live_path);

        let gate = Gates::open_gate(
            &root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap()
        .gate_id;
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

    /// `gate sweep` stops at a gate lock it cannot take, rather than skipping that gate
    /// and reporting the rest. Control: with the lock free, the same sweep reaps it.
    #[test]
    fn gate_sweep_ends_on_a_held_gate_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(5008), Stage::Code);
        let (opened, registration) = Gates::open_registered(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap();
        let gate_lock = hold_gate_lock(root, phase);

        let error = gate_sweep(Some(0), false, Some(root.to_path_buf()))
            .expect_err("a held gate lock ends the sweep");

        assert!(
            matches!(error, CliError::Gate(GateError::GateLockTimeout { .. })),
            "{error}"
        );
        assert!(gate_answer_files(root, phase, stage).is_empty());

        drop(gate_lock);
        gate_sweep(Some(0), false, Some(root.to_path_buf())).unwrap();
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
            let (opened, registration) = Gates::open_registered(
                root,
                phase,
                Stage::Code,
                devflow_core::gates::GateKind::StageFailure,
                "g",
            )
            .unwrap();
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
            gate_respond(
                root,
                responding,
                Some(Stage::Code),
                GateAnswerKind::Approve,
                None,
                None,
            )
        });
        check(PhaseId::new(5010), &|root| {
            gate_sweep(Some(0), false, Some(root.to_path_buf()))
        });
    }

    /// D-04: only the registration counts. A live lock record alone reads not live.
    #[test]
    fn a_waiter_is_live_only_by_its_registration() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(4904), Stage::Code);
        let gate = Gates::open_gate(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap()
        .gate_id;
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
        let (opened, registration) = Gates::open_registered(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "g1",
        )
        .unwrap();
        let g1 = opened.gate_id;

        let hook_root = root.to_path_buf();
        let err = with_answer_before_publish(
            move || {
                Gates::close(&hook_root, phase, stage).unwrap();
                Gates::open_gate(
                    &hook_root,
                    phase,
                    stage,
                    devflow_core::gates::GateKind::StageFailure,
                    "g2",
                )
                .unwrap();
            },
            || {
                gate_respond(
                    root,
                    phase,
                    Some(stage),
                    GateAnswerKind::Approve,
                    None,
                    Some(g1.to_string()),
                )
            },
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
            doctor_json_body(&[], &[], &[], &[], findings)["set_aside_gate_answers"]
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

        let opened = Gates::open_gate(
            root,
            phase,
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            "ctx",
        )
        .unwrap();
        let response = GateResponse {
            approved: true,
            abort: false,
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
    /// per-phase Validate-failure total survives a new `devflow start` of
    /// the same phase, because a bound a restart resets does not
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
        assert_eq!(fresh.stage, Stage::Plan);
        assert_eq!(fresh.stop_until, None);
    }

    /// A-11 reset event ONE: state removal. `workflow::clear_state` (an abort
    /// calls it) deletes the phase's state file — so the
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

        // What an abort does to the state file.
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

    /// Operator decision (2026-11-11, Phase 52): a `start` over a FINISHED
    /// run begins with a zero Validate-failure total. A finished run passed
    /// Validate, so its old failures say nothing about the new run. Since
    /// 52-08 the finish keeps the state file, so without this the total
    /// carried. The control is a stopped run that did NOT finish: its total is
    /// still carried, so a stop-and-restart cannot reset the bound.
    #[test]
    fn a_start_over_a_finished_run_resets_the_failure_total_and_a_stopped_one_carries_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(74);

        let mut persisted = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        persisted.phase_validate_failures = 5;
        persisted.stopped = true;
        persisted.finished = false;
        workflow::save_state(&persisted).unwrap();
        let (stopped_total, stopped_warning) =
            carried_phase_failures(phase, workflow::load_state(root, phase));
        assert_eq!(
            stopped_total, 5,
            "control: a stopped, unfinished run's total must carry into the next start"
        );
        assert_eq!(stopped_warning, None);

        persisted.finished = true;
        workflow::save_state(&persisted).unwrap();
        let (finished_total, finished_warning) =
            carried_phase_failures(phase, workflow::load_state(root, phase));
        assert_eq!(
            finished_total, 0,
            "a finished run passed Validate, so the next start begins at zero"
        );
        assert_eq!(finished_warning, None);
    }

    #[test]
    fn gate_approve_arg_parsing_takes_the_stage_option() {
        let flagged =
            Cli::try_parse_from(["devflow", "gate", "approve", "15", "--stage", "validate"])
                .unwrap();
        let Command::Gate {
            action: GateCmd::Approve { stage, project, .. },
        } = flagged.command
        else {
            panic!("expected flagged gate approve command");
        };
        assert_eq!(stage, Some(Stage::Validate));
        assert_eq!(project, PathBuf::from("."));

        let bare = Cli::try_parse_from(["devflow", "gate", "approve", "15"]).unwrap();
        let Command::Gate {
            action: GateCmd::Approve { stage, .. },
        } = bare.command
        else {
            panic!("expected bare gate approve command");
        };
        assert_eq!(stage, None);
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
            Cli::try_parse_from(["devflow", "gate", "show", "15", "--stage", "validate"]).unwrap();
        let Command::Gate {
            action: GateCmd::Show { phase, stage, .. },
        } = flagged.command
        else {
            panic!("expected gate show command with stage");
        };
        assert_eq!(phase, PhaseId::new(15));
        assert_eq!(stage, Some(Stage::Validate));
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
        Gates::write_gate(
            dir.path(),
            PhaseId::new(15),
            Stage::Validate,
            devflow_core::gates::GateKind::StageFailure,
            &context,
        )
        .unwrap();
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
        Gates::write_gate(
            dir.path(),
            PhaseId::new(15),
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            "ctx1",
        )
        .unwrap();
        Gates::write_gate(
            dir.path(),
            PhaseId::new(15),
            Stage::Validate,
            devflow_core::gates::GateKind::StageFailure,
            "ctx2",
        )
        .unwrap();

        let err = gate_show(dir.path(), PhaseId::new(15), None).unwrap_err();

        assert!(err.to_string().contains("--stage"));
    }

    #[test]
    fn gate_show_auto_resolves_single_open_gate() {
        let dir = tempfile::tempdir().unwrap();
        Gates::write_gate(
            dir.path(),
            PhaseId::new(15),
            Stage::Validate,
            devflow_core::gates::GateKind::StageFailure,
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

    /// 18b: a corrupt pid (0, or above `i32::MAX`) must never read as alive
    /// — `liveness` relies entirely on `agent::agent_running`'s existing
    /// hardening (no second probe is written), so it can only ever produce
    /// `Stuck` or `Unknown` for a corrupt pid, never a false `Healthy`.
    #[test]
    fn liveness_treats_zero_and_overflow_pids_as_dead() {
        assert!(!agent::agent_running(0));
        assert!(!agent::agent_running(u32::MAX));
    }

    /// 18b (idempotency edge): running `devflow status` twice must produce
    /// byte-identical `.devflow/` state — its liveness reads are purely reads.
    #[test]
    fn status_reading_monitor_liveness_writes_no_state_and_no_event() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(66);
        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
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
    /// A saved Plan-stage state with both kinds of answer an earlier run can leave:
    /// a keyed answer for its gate, which an abort, and an unkeyed approval from before
    /// gate ids. Returns the state and the two answers' bytes.
    fn plant_leftover_plan_answers(root: &Path, phase: PhaseId) -> (State, Vec<Vec<u8>>) {
        let mut state = State::new(
            phase,
            devflow_core::state::AgentKind::Claude,
            devflow_core::mode::Mode::Auto,
            root.to_path_buf(),
        );
        state.gate_notify_cmd = Some("true".to_string());
        workflow::save_state(&state).unwrap();

        let old = Gates::open_gate(
            root,
            phase,
            Stage::Plan,
            devflow_core::gates::GateKind::StageFailure,
            "the earlier run's gate",
        )
        .unwrap();
        let keyed = Gates::answer(
            root,
            phase,
            Stage::Plan,
            &old.gate_id,
            &GateResponse {
                approved: false,
                abort: true,
                note: Some("abort: the earlier run".into()),
                responded_by: Some("test".into()),
            },
        )
        .unwrap();
        let unkeyed = Gates::dir(root).join(format!("{}-plan.response.json", phase.padded()));
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
        let (mut state, mut planted) = plant_leftover_plan_answers(root, phase);
        planted.sort();

        let err = crate::pipeline_gate::run_gate_with_timeout(
            root,
            &mut state,
            Stage::Plan,
            devflow_core::gates::GateKind::StageFailure,
            "the new run's gate",
            1,
        )
        .expect_err("a leftover must not decide the new run's gate");
        assert!(
            err.to_string().contains("timed out awaiting a response"),
            "{err}"
        );
        assert_eq!(quarantined_bytes(root), planted, "both leftovers set aside");
        assert_eq!(
            gate_answer_files(root, phase, Stage::Plan),
            Vec::<std::path::PathBuf>::new()
        );

        devflow_core::test_support::preanswer_gate(
            root,
            phase,
            Stage::Plan,
            &GateResponse {
                approved: true,
                abort: false,
                note: None,
                responded_by: Some("test".into()),
            },
        );
        let action = crate::pipeline_gate::run_gate_with_timeout(
            root,
            &mut state,
            Stage::Plan,
            devflow_core::gates::GateKind::StageFailure,
            "control: an answer that binds",
            1,
        )
        .expect("control: an answer that binds decides the gate");
        assert!(
            matches!(action, crate::pipeline_outcomes::Answer::Approve(_)),
            "{action:?}"
        );
    }

    /// D-05 / T-50.1-48: `start`'s leftover step deletes no answer, and names each one
    /// it set aside.
    #[test]
    fn start_sets_leftover_answers_aside_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4911);
        let (_state, mut planted) = plant_leftover_plan_answers(root, phase);
        planted.sort();

        let mut warnings = Vec::new();
        set_aside_leftover_gates(root, phase, &mut warnings);
        let warnings = String::from_utf8(warnings).unwrap();

        assert_eq!(quarantined_bytes(root), planted, "no leftover was deleted");
        for answer in Gates::quarantined(root) {
            let line = format!(
                "warning: set aside an unused answer for phase {phase} plan: {}",
                answer.path.display()
            );
            assert!(warnings.contains(&line), "{line} not in: {warnings}");
        }
        assert!(!Gates::gate_path(root, phase, Stage::Plan).exists());
        assert_eq!(
            gate_answer_files(root, phase, Stage::Plan),
            Vec::<std::path::PathBuf>::new()
        );
    }

    #[test]
    fn gate_respond_auto_resolves_single_open_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (_, registration) = Gates::open_registered(
            root,
            PhaseId::new(15),
            Stage::Validate,
            devflow_core::gates::GateKind::StageFailure,
            "approve the finish?",
        )
        .unwrap();

        gate_respond(
            root,
            PhaseId::new(15),
            None,
            GateAnswerKind::Approve,
            Some("lgtm".into()),
            None,
        )
        .unwrap();

        let gate = crate::test_support::live_gate_id(root, PhaseId::new(15), Stage::Validate);
        let AnswerRead::Bound(answer) =
            Gates::read_answer(root, PhaseId::new(15), Stage::Validate, &gate)
        else {
            panic!("the live gate holds the answer gate_respond wrote");
        };
        assert!(answer.response.approved);
        assert_eq!(answer.response.note.as_deref(), Some("lgtm"));
        let event = devflow_core::events::last_event_for_phase(root, PhaseId::new(15)).unwrap();
        assert_eq!(event["event"], "gate_response_written");
        assert_eq!(event["stage"], "validate");
        registration.finish();
    }

    #[test]
    fn gate_respond_reports_an_abort() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(16);
        let (_, registration) = Gates::open_registered(
            root,
            phase,
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            "relaunch Code?",
        )
        .unwrap();

        gate_respond(
            root,
            phase,
            Some(Stage::Code),
            GateAnswerKind::Abort,
            None,
            None,
        )
        .unwrap();

        let event = devflow_core::events::last_event_for_phase(root, phase).unwrap();
        assert_eq!(event["event"], "gate_response_written");
        assert_eq!(event["abort"], true, "{event}");
        assert_eq!(event["approved"], false, "{event}");
        assert_eq!(GateAnswerKind::Abort.past_tense(), "aborted");
        // Control: the other two answers keep their own words.
        assert_eq!(GateAnswerKind::Reject.past_tense(), "rejected");
        assert_eq!(GateAnswerKind::Approve.past_tense(), "approved");
        registration.finish();
    }

    /// A registered gate of `kind` at `stage` with `context`; `None` writes a
    /// file with no kind (a 3.x or hand-written file), whose waiter is live.
    fn registered_gate_of(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
        kind: Option<devflow_core::gates::GateKind>,
        context: &str,
    ) -> WaiterRegistration {
        let label = kind.unwrap_or(devflow_core::gates::GateKind::StageFailure);
        let (_, registration) = Gates::open_registered(root, phase, stage, label, context).unwrap();
        if kind.is_none() {
            let path = Gates::gate_path(root, phase, stage);
            let mut gate: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let object = gate.as_object_mut().unwrap();
            object.remove("gate_kind");
            object.remove("gsd_status");
            std::fs::write(&path, serde_json::to_string(&gate).unwrap()).unwrap();
        }
        registration
    }

    /// The stage a gate of `kind` opens at in these tests.
    fn print_stage(kind: Option<devflow_core::gates::GateKind>) -> Stage {
        match crate::pipeline_outcomes::answer_family(kind) {
            crate::pipeline_outcomes::AnswerFamily::Verification => Stage::Validate,
            _ => Stage::Code,
        }
    }

    /// A Code-stage state for `phase` under Claude, its recorded canary set to
    /// `canary`, saved under `root`.
    fn print_state(
        root: &Path,
        phase: PhaseId,
        canary: Option<devflow_core::canary::CanaryOutcome>,
    ) -> State {
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state.canary = canary;
        workflow::save_state(&state).unwrap();
        state
    }

    /// D-10, D-39: `gate reject` prints the route its answer runs, for a gate of
    /// every kind, from the routers over the gate file on disk: `/gsd-quick`
    /// with the note on every verification kind, the stage's relaunch with the
    /// note on a stage failure. A reject the supervisor's screen refuses
    /// (preflight, checkpoint declaration, a kindless file, a whitespace-only
    /// note) exits non-zero, names the answers that apply and writes nothing.
    /// Decoys: a `clean_pass` gate whose context reads like a stage failure
    /// prints `/gsd-quick`, and a `stage_failure` gate whose context names
    /// `gaps_found` prints the relaunch. Canary row: a stage failure on a run
    /// whose recorded delivery canary refuses its agent prints the stop and the
    /// `--agent <other>` resume; its control, an admitting canary, prints the
    /// relaunch.
    #[test]
    fn gate_reject_prints_the_route_it_will_run() {
        use crate::pipeline_outcomes::{AnswerFamily, answer_family};
        use devflow_core::canary::CanaryOutcome;
        use devflow_core::gates::GateKind;
        const NOTE: &str = "rename `--quiet` to `--silent`; keep the alias";
        let reject = |root: &Path, phase: PhaseId, stage: Stage, note: &str| {
            gate_respond_report(
                root,
                phase,
                Some(stage),
                GateAnswerKind::Reject,
                Some(note.to_string()),
                None,
            )
        };
        for (index, kind) in crate::test_support::every_gate_kind()
            .into_iter()
            .enumerate()
        {
            let row = format!("{kind:?}");
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(6100 + u32::try_from(index).unwrap());
            let stage = print_stage(kind);
            print_state(root, phase, None);
            let registration = registered_gate_of(root, phase, stage, kind, "a gate");
            let family = answer_family(kind);
            if matches!(
                family,
                AnswerFamily::Verification | AnswerFamily::StageFailure | AnswerFamily::RateLimit
            ) {
                let refused = reject(root, phase, stage, " \t ").expect_err(&row);
                assert!(
                    refused.to_string().contains("refused")
                        && refused.to_string().contains("devflow gate reject --note"),
                    "{row}: a whitespace-only note is refused, naming the answers: {refused}"
                );
                assert!(
                    gate_answer_files(root, phase, stage).is_empty(),
                    "{row}: a refused reject writes nothing"
                );
            }
            match family {
                AnswerFamily::Verification => {
                    let printed = reject(root, phase, stage, NOTE).expect(&row);
                    assert!(
                        printed.contains("route: quick")
                            && printed.contains("`/gsd-quick` with the note")
                            && printed.contains(NOTE),
                        "{row}: {printed}"
                    );
                }
                AnswerFamily::StageFailure => {
                    let printed = reject(root, phase, stage, NOTE).expect(&row);
                    assert!(
                        printed.contains("route: relaunch_with_note")
                            && printed.contains("relaunches the code stage with the note")
                            && printed.contains(NOTE),
                        "{row}: {printed}"
                    );
                    assert!(!printed.contains("/gsd-quick"), "{row}: {printed}");
                }
                // The rate-limit reject's own assertion, with the agent, is
                // `gate_reject_on_a_rate_limit_gate_prints_the_relaunch_with_the_note`.
                AnswerFamily::RateLimit => {
                    let printed = reject(root, phase, stage, NOTE).expect(&row);
                    assert!(
                        printed.contains("route: relaunch_with_note") && printed.contains(NOTE),
                        "{row}: {printed}"
                    );
                    assert!(!printed.contains("/gsd-quick"), "{row}: {printed}");
                }
                AnswerFamily::Preflight
                | AnswerFamily::CheckpointDeclaration
                | AnswerFamily::Unclassified => {
                    let refused = reject(root, phase, stage, NOTE).expect_err(&row);
                    assert!(
                        refused.to_string().contains("refused")
                            && refused.to_string().contains("devflow gate abort"),
                        "{row}: the refusal names the answers that apply: {refused}"
                    );
                    assert!(
                        gate_answer_files(root, phase, stage).is_empty(),
                        "{row}: a refused reject writes nothing"
                    );
                }
            }
            registration.finish();
        }

        // Decoys: the context says the other family's words.
        let decoys = [
            (
                GateKind::CleanPass,
                "[never-silent] stage code failed: no output for 900s",
                "route: quick",
            ),
            (
                GateKind::StageFailure,
                "GSD verification: gaps_found. Approving closes the gaps.",
                "route: relaunch_with_note",
            ),
        ];
        for (index, (kind, context, route)) in decoys.into_iter().enumerate() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(6150 + u32::try_from(index).unwrap());
            let stage = print_stage(Some(kind));
            print_state(root, phase, None);
            let registration = registered_gate_of(root, phase, stage, Some(kind), context);
            let printed = reject(root, phase, stage, NOTE).unwrap();
            assert!(printed.contains(route), "decoy {kind:?}: {printed}");
            registration.finish();
        }

        // Canary row and its control.
        for (index, (canary, route, way_on)) in [
            (CanaryOutcome::Absent, "route: relaunch_refused", true),
            (CanaryOutcome::Confirmed, "route: relaunch_with_note", false),
        ]
        .into_iter()
        .enumerate()
        {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(6160 + u32::try_from(index).unwrap());
            print_state(root, phase, Some(canary.clone()));
            let registration = registered_gate_of(
                root,
                phase,
                Stage::Code,
                Some(GateKind::StageFailure),
                "a gate",
            );
            let printed = reject(root, phase, Stage::Code, NOTE).unwrap();
            assert!(printed.contains(route), "{canary:?}: {printed}");
            assert_eq!(
                printed.contains(&format!("`devflow resume --phase {phase} --agent <other>`")),
                way_on,
                "{canary:?}: {printed}"
            );
            registration.finish();
        }
    }

    /// D-45 rule 2, D-39: `gate approve` prints the route its answer runs. A
    /// verification approve reads GSD and HEAD now through `read_evidence`, prints
    /// the route `approve_route` gives (one row per table row: finish, Validate
    /// again, gap loop, re-verify, reopen), and says the supervisor reads again
    /// when it picks the answer up. A status-less approve prints its own route,
    /// and the GSD stub logs no read. A kindless file is refused and nothing is
    /// written. Decoy: a gate whose context and label say `passed` while GSD
    /// now reads `gaps_found` prints the gap loop. Canary row: a stage failure
    /// under a refusing canary prints the stop and the `--agent <other>`
    /// resume, with no GSD read; control: an admitting canary prints the retry.
    /// A state that cannot be loaded is named, not guessed.
    #[test]
    fn gate_approve_prints_the_route_it_will_run() {
        use crate::pipeline_outcomes::{AnswerFamily, answer_family};
        use crate::test_support::{gsd_says, validate_evidence};
        use devflow_core::canary::CanaryOutcome;
        use devflow_core::gates::GateKind;
        use devflow_core::gsd_reader::GsdStatus;
        const PICKUP: &str = "the supervisor reads GSD's verification status again when it \
                              picks this answer up";
        let approve = |root: &Path, phase: PhaseId, stage: Stage| {
            gate_respond_report(
                root,
                phase,
                Some(stage),
                GateAnswerKind::Approve,
                None,
                None,
            )
        };

        // One row per table row of the verification approve.
        let table: [(GsdStatus, bool, &str); 6] = [
            (GsdStatus::Passed, true, "route: finish"),
            (GsdStatus::Passed, false, "route: validate_again"),
            (GsdStatus::GapsFound, true, "route: gap_loop"),
            (GsdStatus::Stale, false, "route: reverify"),
            (GsdStatus::Missing, false, "route: reverify"),
            (GsdStatus::HumanNeeded, true, "route: reopen"),
        ];
        for (index, (status, pass_on_head, route)) in table.into_iter().enumerate() {
            let row = format!("{status:?}, pass on HEAD {pass_on_head}");
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(6200 + u32::try_from(index).unwrap());
            let gsd = validate_evidence(root, phase, &[gsd_says(status)]);
            let mut state = print_state(root, phase, None);
            state.stage = Stage::Validate;
            if pass_on_head {
                state.validate_pass_head = Some(crate::test_support::repo_with_head(root));
            }
            workflow::save_state(&state).unwrap();
            let registration = registered_gate_of(
                root,
                phase,
                Stage::Validate,
                Some(GateKind::CleanPass),
                "a gate",
            );
            let printed = approve(root, phase, Stage::Validate).expect(&row);
            assert!(printed.contains(route), "{row}: {printed}");
            assert!(printed.contains(PICKUP), "{row}: {printed}");
            assert_eq!(gsd.calls().len(), 1, "{row}: one status read now");
            registration.finish();
        }

        for (index, kind) in crate::test_support::every_gate_kind()
            .into_iter()
            .enumerate()
        {
            let row = format!("{kind:?}");
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(6220 + u32::try_from(index).unwrap());
            let stage = print_stage(kind);
            let gsd = validate_evidence(root, phase, &[gsd_says(GsdStatus::Stale)]);
            let mut state = print_state(root, phase, None);
            state.stage = stage;
            workflow::save_state(&state).unwrap();
            let registration = registered_gate_of(root, phase, stage, kind, "a gate");
            let expected = match answer_family(kind) {
                AnswerFamily::Verification => Some("route: reverify"),
                AnswerFamily::StageFailure => Some("route: retry_stage"),
                AnswerFamily::RateLimit => Some("route: relaunch_now"),
                AnswerFamily::Preflight => Some("route: preflight_override"),
                AnswerFamily::CheckpointDeclaration => Some("route: record_checkpoint_set"),
                AnswerFamily::Unclassified => None,
            };
            match expected {
                Some(route) => {
                    let printed = approve(root, phase, stage).expect(&row);
                    assert!(printed.contains(route), "{row}: {printed}");
                    let verification = answer_family(kind) == AnswerFamily::Verification;
                    assert_eq!(printed.contains(PICKUP), verification, "{row}: {printed}");
                    assert_eq!(
                        gsd.calls().len(),
                        usize::from(verification),
                        "{row}: only a verification approve reads GSD"
                    );
                }
                None => {
                    let refused = approve(root, phase, stage).expect_err(&row);
                    assert!(
                        refused.to_string().contains("refused")
                            && refused.to_string().contains("devflow gate abort"),
                        "{row}: {refused}"
                    );
                    assert!(gate_answer_files(root, phase, stage).is_empty(), "{row}");
                    assert!(gsd.calls().is_empty(), "{row}");
                }
            }
            registration.finish();
        }

        // Decoy: the gate's context and label say `passed`; GSD reads
        // `gaps_found` now.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(6240);
        let _gsd = validate_evidence(root, phase, &[gsd_says(GsdStatus::GapsFound)]);
        let mut state = print_state(root, phase, None);
        state.stage = Stage::Validate;
        state.validate_pass_head = Some(crate::test_support::repo_with_head(root));
        workflow::save_state(&state).unwrap();
        let (_, registration) = Gates::open_registered(
            root,
            phase,
            Stage::Validate,
            devflow_core::gates::GateLabel {
                kind: GateKind::CleanPass,
                gsd_status: Some(GsdStatus::Passed),
                not_before: None,
            },
            "Validation passed. GSD verification: passed.",
        )
        .unwrap();
        let printed = approve(root, phase, Stage::Validate).unwrap();
        assert!(printed.contains("route: gap_loop"), "decoy: {printed}");
        registration.finish();

        // Canary row and its control: no GSD read either way.
        for (index, (canary, route, way_on)) in [
            (CanaryOutcome::Absent, "route: relaunch_refused", true),
            (CanaryOutcome::Confirmed, "route: retry_stage", false),
        ]
        .into_iter()
        .enumerate()
        {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(6250 + u32::try_from(index).unwrap());
            let gsd = validate_evidence(root, phase, &[gsd_says(GsdStatus::Passed)]);
            print_state(root, phase, Some(canary.clone()));
            let registration = registered_gate_of(
                root,
                phase,
                Stage::Code,
                Some(GateKind::StageFailure),
                "a gate",
            );
            let printed = approve(root, phase, Stage::Code).unwrap();
            assert!(printed.contains(route), "{canary:?}: {printed}");
            assert_eq!(
                printed.contains(&format!("`devflow resume --phase {phase} --agent <other>`")),
                way_on,
                "{canary:?}: {printed}"
            );
            assert!(gsd.calls().is_empty(), "{canary:?}: no GSD read");
            registration.finish();
        }

        // A state that cannot be loaded is named; no route is guessed.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(6260);
        let registration = registered_gate_of(
            root,
            phase,
            Stage::Code,
            Some(GateKind::StageFailure),
            "a gate",
        );
        let printed = approve(root, phase, Stage::Code).unwrap();
        assert!(
            printed.contains("route: not known") && printed.contains("state could not be loaded"),
            "{printed}"
        );
        assert!(!printed.contains("route: retry_stage"), "{printed}");
        registration.finish();
    }

    /// D-15, D-27.4, D-37: `gate reject` with a note on a rate-limit gate
    /// opened at Plan on a Codex run is written, and prints the relaunch of the
    /// plan stage on codex with the note; a whitespace-only note exits non-zero
    /// and writes nothing. `gate approve` on such a gate prints the relaunch
    /// now, on codex, and `gate abort` is written. The print comes from the
    /// routers over the gate's kind: control in the same test, a `clean_pass`
    /// gate at Plan with the same context prints `/gsd-quick` on reject.
    #[test]
    fn gate_reject_on_a_rate_limit_gate_prints_the_relaunch_with_the_note() {
        use devflow_core::gates::GateKind;
        const NOTE: &str = "the quota is back; keep the diff small";
        const CONTEXT: &str = "[never-silent] stage plan failed: rate limited until soon";
        let stage = Stage::Plan;
        let run_at = |root: &Path, phase: PhaseId, kind: GateKind| {
            let mut state =
                State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
            state.stage = stage;
            workflow::save_state(&state).unwrap();
            registered_gate_of(root, phase, stage, Some(kind), CONTEXT)
        };
        let answer = |root: &Path, phase: PhaseId, kind: GateAnswerKind, note: Option<&str>| {
            gate_respond_report(
                root,
                phase,
                Some(stage),
                kind,
                note.map(str::to_string),
                None,
            )
        };

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(6270);
        let registration = run_at(root, phase, GateKind::RateLimit);
        let refused = answer(root, phase, GateAnswerKind::Reject, Some(" \t ")).unwrap_err();
        assert!(
            refused.to_string().contains("refused")
                && refused.to_string().contains("devflow gate reject --note"),
            "{refused}"
        );
        assert!(
            gate_answer_files(root, phase, stage).is_empty(),
            "a refused reject writes nothing"
        );
        let printed = answer(root, phase, GateAnswerKind::Reject, Some(NOTE)).unwrap();
        assert!(
            printed.contains("route: relaunch_with_note")
                && printed.contains("relaunches the plan stage with the note")
                && printed.contains("codex")
                && printed.contains(NOTE),
            "{printed}"
        );
        assert!(!printed.contains("/gsd-quick"), "{printed}");
        assert_eq!(gate_answer_files(root, phase, stage).len(), 1, "written");
        registration.finish();

        let phase = PhaseId::new(6271);
        let registration = run_at(root, phase, GateKind::RateLimit);
        let printed = answer(root, phase, GateAnswerKind::Approve, None).unwrap();
        assert!(
            printed.contains("route: relaunch_now")
                && printed.contains("relaunches the plan stage now on the same agent")
                && printed.contains("codex"),
            "{printed}"
        );
        registration.finish();

        let phase = PhaseId::new(6272);
        let registration = run_at(root, phase, GateKind::RateLimit);
        let printed = answer(root, phase, GateAnswerKind::Abort, None).unwrap();
        assert!(printed.contains("route: end"), "{printed}");
        assert_eq!(gate_answer_files(root, phase, stage).len(), 1, "written");
        registration.finish();

        // Control: the same stage and context on a `clean_pass` gate.
        let phase = PhaseId::new(6273);
        let registration = run_at(root, phase, GateKind::CleanPass);
        let printed = answer(root, phase, GateAnswerKind::Reject, Some(NOTE)).unwrap();
        assert!(
            printed.contains("route: quick") && printed.contains("/gsd-quick"),
            "control: {printed}"
        );
        registration.finish();
    }

    #[test]
    fn gate_respond_requires_stage_when_ambiguous_and_errors_when_none_open() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let err = gate_respond(
            root,
            PhaseId::new(15),
            None,
            GateAnswerKind::Approve,
            None,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("no open gate"), "{err}");

        Gates::write_gate(
            root,
            PhaseId::new(15),
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            "a",
        )
        .unwrap();
        let (_, registration) = Gates::open_registered(
            root,
            PhaseId::new(15),
            Stage::Validate,
            devflow_core::gates::GateKind::StageFailure,
            "b",
        )
        .unwrap();
        let err = gate_respond(
            root,
            PhaseId::new(15),
            None,
            GateAnswerKind::Reject,
            Some("nope".into()),
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("--stage"), "{err}");

        // An explicit stage disambiguates. A gate needs a confirmed waiter
        // before a response can be published, so the Validate gate has one.
        gate_respond(
            root,
            PhaseId::new(15),
            Some(Stage::Validate),
            GateAnswerKind::Reject,
            Some("gaps".into()),
            None,
        )
        .unwrap();
        assert!(
            !gate_answer_files(root, PhaseId::new(15), Stage::Validate).is_empty(),
            "explicit-stage rejection must land"
        );
        assert!(gate_answer_files(root, PhaseId::new(15), Stage::Code).is_empty());
        registration.finish();
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
        let (phase, stage) = (PhaseId::new(30), Stage::Validate);
        let (_, registration) = Gates::open_registered(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "ctx",
        )
        .unwrap();
        backdate_gate(root, phase, stage, aged_past_threshold());

        gate_sweep(None, true, Some(root.to_path_buf())).unwrap();

        assert!(
            gate_answer_files(root, phase, stage).is_empty(),
            "dry-run must never write a response file"
        );
        let open = Gates::list_open(root);
        assert_eq!(open.len(), 1, "dry-run must leave the gate open");

        gate_sweep(None, false, Some(root.to_path_buf())).unwrap();
        assert_eq!(
            gate_answer_files(root, phase, stage).len(),
            1,
            "the control: without --dry-run the same gate and waiter are reaped"
        );
        registration.finish();
    }

    /// Task 2 behavior: a gate that already has a response is a benign race
    /// (a human may have answered it between `list_open`
    /// and `reap`) — the sweep must return `Ok` and leave the existing
    /// response byte-for-byte untouched, never overwrite it.
    #[test]
    fn gate_sweep_skips_already_responded_gate_without_clobbering() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(31), Stage::Validate);
        let (opened, registration) = Gates::open_registered(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "ctx",
        )
        .unwrap();
        backdate_gate(root, phase, stage, aged_past_threshold());
        let listed_before_the_answer = listed_gate(root, phase);
        let response = GateResponse {
            approved: true,
            abort: false,
            note: None,
            responded_by: Some("human".into()),
        };
        let path = Gates::answer(root, phase, stage, &opened.gate_id, &response).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();

        let result = gate_sweep(None, false, Some(root.to_path_buf()));
        let raced =
            sweep_one_gate(root, &listed_before_the_answer, 1, 0, false, now_secs()).unwrap();

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
        let (_, registration) = Gates::open_registered(
            root,
            phase,
            Stage::Validate,
            devflow_core::gates::GateKind::StageFailure,
            "ctx",
        )
        .unwrap();
        backdate_gate(root, phase, Stage::Validate, aged_past_threshold());

        gate_sweep(None, false, Some(root.to_path_buf())).unwrap();

        let event = devflow_core::events::last_event_for_phase(root, phase).unwrap();
        assert_eq!(event["event"], "gate_reaped");
        assert_eq!(event["stage"], "validate");
        registration.finish();
    }

    /// `gate_sweep` never looks at the process table: a live stray-shaped
    /// fixture is neither discovered nor touched.
    #[test]
    fn gate_sweep_ignores_a_live_stray() {
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg("trap cleanup TERM INT; sleep 30")
            .spawn()
            .expect("spawn monitor-wrapper-shaped fixture");
        let pid = child.id();
        let dir = tempfile::tempdir().unwrap();

        gate_sweep(None, false, Some(dir.path().to_path_buf())).unwrap();

        assert!(
            agent::agent_running(pid),
            "gate_sweep must never signal anything discoverable only via the process table"
        );

        let _ = child.kill();
        let _ = child.wait();
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
    /// never pass Plan, so `start --agent codex` pre-flights on the
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
        Gates::write_gate(
            dir.path(),
            PhaseId::new(16),
            Stage::Validate,
            devflow_core::gates::GateKind::StageFailure,
            &context,
        )
        .unwrap();
        let open = Gates::list_open(dir.path());
        let gate_id = GateId::parse(&open[0].gate_id).unwrap();
        let _waiter =
            Gates::register_waiter(dir.path(), PhaseId::new(16), Stage::Validate, &gate_id)
                .unwrap();

        let banner = render_pending_gate_banner(dir.path(), &open, u64::MAX).unwrap();

        assert!(banner.contains("PENDING GATE"));
        assert!(banner.contains("phase 16"));
        assert!(banner.contains("validate"));
        assert!(banner.contains("devflow gate approve 16 --stage validate"));
        assert!(banner.contains("devflow gate reject 16 --stage validate"));
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
            stage: Stage::Validate,
            context: "ctx".to_string(),
            timestamp: "not-a-number".to_string(),
            gate_id: String::new(),
            not_before: None,
        };

        let row = render_all_roots_gate_row(Path::new("/tmp/some-root"), &gate, 10_000);

        assert!(row.contains("42"), "row must still name the phase: {row}");
        assert!(row.contains('?'), "row must render the unknown age: {row}");
    }

    /// The line `status` prints for `state`'s run, with the runner lock free and the
    /// unit double's default `inactive`.
    fn not_live_row(state: &State) -> String {
        run_status(
            &state.project_root,
            state.phase,
            &workflow::Existing::Loaded(state.clone()),
        )
        .expect("a loaded state has a row")
        .line(state.phase)
    }

    /// 21a / 50-12, D-09: a prepared run whose runner is gone while it is not stopped
    /// reads crashed and names `resume`.
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

        let row = not_live_row(&state);

        assert!(row.starts_with("crashed"), "{row}");
        assert!(row.contains("`devflow resume --phase 7`"), "{row}");
    }

    /// Whether a hint sends the operator through a hand-run `advance`, whose gate open
    /// sets an earlier answer aside (D-06).
    fn hint_routes_an_answer_through_advance(hints: &[String]) -> bool {
        hints.iter().any(|hint| hint.contains("devflow advance"))
    }

    /// V-33, D-06: a gate-pending phase whose runner is gone is repaired by `resume`, which
    /// asks again, at Validate as at any other stage. No row, gate state or stage names
    /// `advance`.
    #[test]
    fn recovery_hints_for_a_gone_gate_pending_phase_name_resume_not_advance() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new(
            PhaseId::new(7),
            AgentKind::Claude,
            Mode::Auto,
            dir.path().to_path_buf(),
        );
        state.verification_baseline_captured = true;
        state.gate_pending = true;

        for stage in [Stage::Code, Stage::Validate] {
            state.stage = stage;
            let row = not_live_row(&state);
            assert!(row.contains("`devflow resume --phase 7`"), "{stage}: {row}");
        }

        for (stopped, finished) in [(false, false), (true, false), (true, true)] {
            for gate_pending in [false, true] {
                for stage in [Stage::Code, Stage::Validate] {
                    state.stopped = stopped;
                    state.finished = finished;
                    state.gate_pending = gate_pending;
                    state.stage = stage;
                    let row = not_live_row(&state);
                    assert!(
                        !hint_routes_an_answer_through_advance(std::slice::from_ref(&row)),
                        "stopped={stopped} finished={finished} gate_pending={gate_pending} \
                         {stage}: {row}"
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

    /// Once `stop` has ended a run nothing else continues it, so `status` names `resume`.
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

        let row = not_live_row(&state);

        assert!(row.starts_with("stopped"), "{row}");
        assert!(row.contains("`devflow resume --phase 7`"), "{row}");
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
        let row = not_live_row(&state);
        assert!(
            row.contains("`devflow resume --phase 7`"),
            "control: a recorded Confirmed keeps the plain resume: {row}"
        );

        for refusal in [
            devflow_core::canary::CanaryOutcome::Absent,
            devflow_core::canary::CanaryOutcome::Unverified("no binary".to_string()),
        ] {
            state.canary = Some(refusal.clone());
            for stopped in [true, false] {
                state.stopped = stopped;
                let row = not_live_row(&state);
                assert!(
                    row.contains("`devflow resume --phase 7 --agent <other>`"),
                    "{refusal:?} stopped={stopped}: {row}"
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

    /// Unit tests for the pure `doctor` reconciliation core (18a). Each test
    /// builds a `PhaseFacts` directly — no repository, no I/O — proving
    /// `reconcile_phase` is a predicate over facts alone.
    #[cfg(test)]
    mod doctor_reconciliation {
        use super::*;

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

        /// D-07, D-12a (T-18-01): `doctor --json` phase facts carry the runner's
        /// liveness label and its pid, no reservation key, and no string in the
        /// reconciliation section holds a path.
        #[test]
        fn doctor_json_reports_the_runner_liveness_without_paths() {
            let gate_id = GateId::new().to_string();
            let facts = vec![
                PhaseFacts {
                    gate_pending: true,
                    ..agreeing_facts(PhaseId::new(27))
                },
                PhaseFacts {
                    runner: Err("its runner lock /home/someone/x could not be probed".to_string()),
                    ..agreeing_facts(PhaseId::new(28))
                },
                PhaseFacts {
                    waiter: WaiterStatus::Live,
                    waiter_gate: Some((Stage::Plan, gate_id.clone())),
                    open_gate_stages: vec![Stage::Plan],
                    ..agreeing_facts(PhaseId::new(29))
                },
            ];
            let body = render_reconciliation_json(&facts);
            let entries = body.as_array().expect("an array");
            let live = &entries[0]["runner"];
            assert_eq!(live["liveness"], "live", "{body}");
            assert_eq!(live["pid"], 4242, "{body}");
            assert!(live.get("pending").is_none(), "no reservation key: {body}");
            let failed = &entries
                .iter()
                .find(|entry| entry["phase"] == 28)
                .expect("phase 28's finding")["runner"];
            assert_eq!(failed["liveness"], "error", "{body}");
            assert!(failed["pid"].is_null(), "{body}");
            let mut strings = Vec::new();
            strings_in(&body, &mut strings);
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
                waiter_gate: Some((Stage::Plan, PATH_AS_GATE_ID.to_string())),
                open_gate_stages: vec![Stage::Plan],
                ..agreeing_facts(PhaseId::new(31))
            }];
            let body = render_reconciliation_json(&facts);
            let entries = body.as_array().expect("an array");
            assert!(!entries.is_empty(), "{body}");
            for entry in entries {
                assert_eq!(
                    entry["gate_waiter"],
                    serde_json::json!({"stage": "plan", "gate_id": null, "status": "absent"}),
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
                runner: Ok(RunnerLiveness::NotLive),
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

        /// A fully-agreeing baseline: `reconcile_phase` over this returns
        /// zero findings. Each test overrides only the field(s) needed to
        /// trigger the one check it's proving.
        pub(super) fn agreeing_facts(phase: PhaseId) -> PhaseFacts {
            PhaseFacts {
                phase,
                stage: Stage::Code,
                gate_pending: false,
                waiter: WaiterStatus::Absent,
                waiter_gate: None,
                runner: Ok(RunnerLiveness::Live { pid: Some(4242) }),
                last_event: Some("stage_launched".into()),
                last_launched_stage: Some(Stage::Code),
                open_gate_stages: Vec::new(),
                unprepared_restart: None,
                feature_branch_exists: true,
                stopped: false,
                finished: false,
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
                runner: Ok(RunnerLiveness::NotLive),
                stopped: true,
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
                runner: Ok(RunnerLiveness::NotLive),
                stopped: true,
                ..agreeing_facts(PhaseId::new(3))
            };
            let findings = reconcile_phase(&facts);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Problem);
            assert!(findings[0].detail.contains("gate open for stage validate"));
            assert_eq!(
                findings[0].repair.as_deref(),
                Some(no_waiter_repair(PhaseId::new(3)).as_str())
            );
        }

        #[test]
        fn reconcile_phase_flags_open_gate_with_no_waiter_with_the_cli_repair() {
            let phase = PhaseId::new(13);
            let facts = PhaseFacts {
                open_gate_stages: vec![Stage::Validate],
                waiter: WaiterStatus::Absent,
                runner: Ok(RunnerLiveness::NotLive),
                stopped: true,
                ..agreeing_facts(phase)
            };

            let findings = reconcile_phase(&facts);

            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Problem);
            assert_eq!(
                findings[0].repair.as_deref(),
                Some(no_waiter_repair(phase).as_str())
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
                finding.repair.as_deref() != Some(no_waiter_repair(phase).as_str())
            }));
        }

        /// Criterion 2 for the doctor repairs that name `resume`: none does while the
        /// phase's runner may be live (its lock is held, its unit is still stopping, or
        /// the read failed), since `resume` refuses then. Control: with no runner, each
        /// one does.
        #[test]
        fn no_repair_names_resume_while_a_supervisor_or_agent_lives() {
            let phase = PhaseId::new(29);
            let gate_pending = |facts: PhaseFacts| PhaseFacts {
                gate_pending: true,
                ..facts
            };
            let open_gate = |facts: PhaseFacts| PhaseFacts {
                gate_pending: true,
                open_gate_stages: vec![Stage::Code],
                ..facts
            };
            let shapes: [&dyn Fn(PhaseFacts) -> PhaseFacts; 2] = [&gate_pending, &open_gate];
            for (index, shape) in shapes.iter().enumerate() {
                for (case, runner) in [
                    ("live", Ok(RunnerLiveness::Live { pid: Some(4242) })),
                    (
                        "stopping",
                        Ok(RunnerLiveness::Stopping {
                            unit: "devflow-x-29.service".to_string(),
                            state: "deactivating".to_string(),
                        }),
                    ),
                    (
                        "unreadable",
                        Err("its runner lock could not be probed".to_string()),
                    ),
                ] {
                    let findings = reconcile_phase(&shape(PhaseFacts {
                        runner,
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
                    runner: Ok(RunnerLiveness::NotLive),
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

        /// D-07, D-12a: a runner liveness that could not be read is a `Problem` whose
        /// repair is `devflow status`, and a phase no runner runs that is neither
        /// stopped nor finished is a `Problem` whose repair resumes it, or starts it
        /// when its `start` never prepared it. Controls: a live or stopping runner, and
        /// a stopped or finished phase with no runner, are not findings.
        #[test]
        fn check_runner_reports_an_unreadable_runner_and_a_run_nothing_advances() {
            let unreadable = check_runner(&PhaseFacts {
                runner: Err("its runner lock could not be probed (x)".to_string()),
                ..agreeing_facts(PhaseId::new(21))
            })
            .expect("an unreadable runner is a finding");
            assert_eq!(unreadable.severity, Severity::Problem);
            assert_eq!(unreadable.repair.as_deref(), Some("devflow status"));
            assert!(
                unreadable.detail.contains("could not be read")
                    && unreadable.detail.contains("cannot be ruled out")
                    && !unreadable.detail.contains("(x)"),
                "names the failure without its text: {}",
                unreadable.detail
            );

            let crashed = check_runner(&PhaseFacts {
                runner: Ok(RunnerLiveness::NotLive),
                ..agreeing_facts(PhaseId::new(22))
            })
            .expect("a run no runner advances is a finding");
            assert_eq!(crashed.severity, Severity::Problem);
            assert_eq!(crashed.repair.as_deref(), Some("devflow resume --phase 22"));
            assert!(
                crashed
                    .detail
                    .contains("no runner runs this phase at stage code"),
                "{}",
                crashed.detail
            );
            let unprepared = check_runner(&PhaseFacts {
                runner: Ok(RunnerLiveness::NotLive),
                unprepared_restart: Some("devflow start --phase 22".to_string()),
                ..agreeing_facts(PhaseId::new(22))
            })
            .expect("an unprepared run no runner advances is a finding");
            assert_eq!(
                unprepared.repair.as_deref(),
                Some("devflow start --phase 22")
            );
            assert!(
                reconcile_phase(&PhaseFacts {
                    runner: Ok(RunnerLiveness::NotLive),
                    ..agreeing_facts(PhaseId::new(22))
                })
                .iter()
                .any(|finding| finding.detail == crashed.detail),
                "reconcile_phase runs check_runner"
            );

            let live = agreeing_facts(PhaseId::new(24));
            assert!(check_runner(&live).is_none(), "control: a live runner");
            assert!(reconcile_phase(&live).is_empty(), "control: a live runner");
            let stopping = PhaseFacts {
                runner: Ok(RunnerLiveness::Stopping {
                    unit: "devflow-x-25.service".to_string(),
                    state: "deactivating".to_string(),
                }),
                ..agreeing_facts(PhaseId::new(25))
            };
            assert!(
                check_runner(&stopping).is_none(),
                "control: a stopping unit"
            );
            for (case, stopped, finished) in [("stopped", true, false), ("finished", false, true)] {
                let ended = PhaseFacts {
                    runner: Ok(RunnerLiveness::NotLive),
                    stopped,
                    finished,
                    ..agreeing_facts(PhaseId::new(26))
                };
                assert!(check_runner(&ended).is_none(), "control: a {case} run");
            }
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
                feature_branch_exists: false,
                ..agreeing_facts(PhaseId::new(6))
            };
            let findings = reconcile_phase(&facts);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Warn);
            assert!(findings[0].detail.contains("feature/phase-06"));
            assert!(findings[0].repair.is_none());
        }

        /// Several checks trigger simultaneously; the returned findings must
        /// come back in the fixed order `reconcile_phase` evaluates checks
        /// in, not in whatever order the facts happen to be populated.
        #[test]
        fn reconcile_phase_ordering_is_input_order_independent() {
            let facts = PhaseFacts {
                gate_pending: true,
                runner: Ok(RunnerLiveness::NotLive),
                last_launched_stage: Some(Stage::Validate),
                open_gate_stages: Vec::new(),
                unprepared_restart: None,
                feature_branch_exists: false,
                ..agreeing_facts(PhaseId::new(7))
            };
            let findings = reconcile_phase(&facts);
            let severities: Vec<Severity> = findings.iter().map(|f| f.severity).collect();
            assert_eq!(
                severities,
                vec![
                    Severity::Problem, // check_gate_pending_without_gate
                    Severity::Problem, // check_runner
                    Severity::Warn,    // check_stage_event_drift
                    Severity::Warn,    // check_missing_branch
                ]
            );
            assert!(findings[0].detail.contains("gate_pending is true"));
            assert!(findings[1].detail.contains("no runner runs this phase"));
            assert!(
                findings[2]
                    .detail
                    .contains("last stage_launched event named validate")
            );
            assert!(findings[3].detail.contains("feature/phase-07"));
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
            state.verification_baseline_captured = true;
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

            let body = doctor_json_body(&checks, &facts, &[], &[], &[]);
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
                5,
                "doctor --json must have exactly five top-level keys: {reparsed}"
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
            let repair = finding.repair.as_deref().unwrap();
            assert!(
                repair.contains("pid 424242"),
                "the repair must name the pid: {repair:?}"
            );
            assert!(
                repair.contains("`ps -o args= -p 424242`"),
                "the repair must have the operator confirm the process before ending it: \
                 {repair:?}"
            );
            assert!(
                repair.contains("`kill 424242`"),
                "the repair must name the manual kill: {repair:?}"
            );
            assert!(
                // Split so the removed-surface scan does not read this control as a leftover.
                !repair.contains(concat!("--reap", "-strays")),
                "the repair must not name the removed flag: {repair:?}"
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
            assert!(
                text.contains("repair: confirm pid 555555"),
                "the repair line must name the pid: {text}"
            );
            assert!(text.contains("`ps -o args= -p 555555`"), "{text}");
            assert!(text.contains("`kill 555555`"), "{text}");
            // Split so the removed-surface scan does not read this control as a leftover.
            assert!(!text.contains(concat!("--reap", "-strays")), "{text}");
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
            let body = doctor_json_body(&checks, &facts, &[], &stray_findings, &[]);
            let obj = body.as_object().unwrap();
            assert_eq!(obj.len(), 5, "must be exactly five top-level keys: {body}");
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
                .contains("`ps -o args= -p 909090`"),
            "the repair must have the operator confirm the process before ending it: {:?}",
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
    fn reachable_pids_are_excluded_from_the_stray_findings() {
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

        // End both with a VERIFIED signal, never a bare one, plus a final
        // `wait()` on each to reclaim the zombie regardless.
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

        // End it with a verified signal, plus a final `wait()` to reclaim
        // the zombie regardless.
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
            let body = doctor_json_body(&checks, &facts, &doc_findings, &[], &[]);
            let obj = body.as_object().unwrap();
            assert_eq!(
                obj.len(),
                5,
                "must be exactly {{environment, reconciliation, planning_doc_staleness, \
                 stray_processes, set_aside_gate_answers}}: {body}"
            );
            assert!(obj.contains_key("environment"));
            assert!(obj.contains_key("reconciliation"));
            let staleness = obj["planning_doc_staleness"].as_array().unwrap();
            assert_eq!(staleness.len(), 1);
            assert!(obj.contains_key("stray_processes"));
        }
    }

    // ------------------------------------------------------------------
    // Phase 41 Task 7 (D-04/F7): the doctor antigravity entry lives behind an
    // assertable seam and reports presence, never a hard failure. The
    // PATH-based presence reporting tests live in tests/doctor_antigravity.rs
    // (integration — spawning the real binary from a unit-test harness
    // re-enters the suite).
    // ------------------------------------------------------------------

    #[test]
    fn doctor_reports_an_unset_rust_log_as_information() {
        let row = rust_log_check(None);
        assert_eq!(
            row.status, "info",
            "an unset RUST_LOG is information, not a failure"
        );
        let detail = row.version.as_deref().unwrap_or("");
        assert!(
            detail.contains("tracing off"),
            "the row must say tracing is off: {detail:?}"
        );
        let hint = row.install_hint.as_deref().unwrap_or("");
        assert!(
            hint.contains("RUST_LOG=info"),
            "the row must say how to turn tracing on: {hint:?}"
        );
        let line = check_line(&row);
        assert!(
            !line.contains('✗'),
            "no red cross for an unset RUST_LOG: {line}"
        );
        assert!(
            line.contains(hint),
            "the printed row carries the hint: {line}"
        );
    }

    #[test]
    fn doctor_reports_an_empty_rust_log_as_information() {
        // An empty RUST_LOG turns tracing off exactly as an unset one does.
        let row = rust_log_check(Some(""));
        assert_eq!(row.status, "info");
        assert!(row.version.as_deref().unwrap_or("").contains("tracing off"));
    }

    #[test]
    fn doctor_still_warns_on_an_invalid_rust_log() {
        // Control: only the unset and empty cases became information.
        assert_eq!(rust_log_check(Some("loud")).status, "warn");
        assert_eq!(rust_log_check(Some("devflow=debug")).status, "ok");
    }

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

    /// How long a test leaves a writer blocked on the write lock before it lets the
    /// holder go. Long enough that a writer not taking the lock has finished.
    const BLOCKED_WRITER_GRACE: std::time::Duration = std::time::Duration::from_millis(200);
    const HOLDER_ENTERS_WITHIN: std::time::Duration = std::time::Duration::from_secs(30);

    // The launch reservation (50.1-16, D-01). Every test here runs its launch in a
    // child: `launch_cache_dir` needs a hermetic DEVFLOW_CACHE_DIR, and the test holds
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
        // `start` refuses a Codex run without the phase's CONTEXT.md on the base.
        let padded = phase.padded();
        let context = format!(".planning/phases/{padded}-fixture/{padded}-CONTEXT.md");
        std::fs::create_dir_all(root.join(&context).parent().unwrap()).unwrap();
        std::fs::write(root.join(&context), "# Context\n").unwrap();
        for args in [
            &["add", ".planning/ROADMAP.md", context.as_str()][..],
            &["commit", "-q", "-m", "roadmap"][..],
        ] {
            let status = devflow_core::test_support::git_command(root)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?} failed");
        }
    }

    /// `start` as these tests run it: Supervise, no worktree, the capture shape (Codex).
    fn start_for_test(root: &Path, phase: PhaseId) -> Result<(), String> {
        start(
            root,
            phase,
            AgentKind::Codex,
            Mode::Supervise,
            false,
            false,
            None,
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

    /// `start` as the fallback tests run it: no worktree, `agent` as the primary and
    /// `mode` and `fallback` as given.
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
            None,
            FallbackSource::FlagOrConfig(fallback),
        )
    }

    /// The lock a holder child of `every_verb_follows_the_lock_and_state_table` takes:
    /// `runner` (the runner lock) or `holder` (the phase holder).
    const VERB_TABLE_HOLD_ENV: &str = "VERB_TABLE_HOLD";
    /// The project root that child holds a lock of.
    const VERB_TABLE_ROOT_ENV: &str = "VERB_TABLE_ROOT";
    /// The dir that child writes `held` into and waits for `release` in.
    const VERB_TABLE_MARKS_ENV: &str = "VERB_TABLE_MARKS";

    /// A verb of D-07's table.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TableVerb {
        Start,
        Resume,
        Discard,
        Stop,
        Cleanup,
        Status,
    }

    /// Which lock a cell holds, each in a child process of its own.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TableLock {
        /// The runner lock is held: a live run.
        RunnerHeld,
        /// Neither lock is held.
        Free,
        /// The phase holder is held and the runner lock is free: a verb in progress.
        HolderHeld,
    }

    /// The state a cell plants.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TableState {
        None,
        ReservationOnly,
        Running,
        Stopped,
        Finished,
        Unloadable,
    }

    /// What a cell expects. Every arm but `Launch` and `Removes` also expects the state
    /// file's bytes unchanged; every arm but `Launch` expects no launch, and every arm but
    /// `Stop` no `stop_unit` call.
    #[derive(Debug)]
    enum Expect {
        /// The verb returns an error carrying every `needles` text and no `absent` one.
        Refuse {
            needles: Vec<String>,
            absent: Vec<String>,
        },
        /// The verb returns Ok and launched one runner unit.
        Launch,
        /// The verb returns Ok and the state file is gone.
        Removes,
        /// The verb returns Ok and touched nothing.
        Keeps,
        /// `status` returns Ok and the phase's row names this word (`None`: no row).
        Word(Option<&'static str>),
        /// `stop` exits 0 (`ok`) or not with a line carrying every `needles` text and
        /// no `absent` one, after `stop_calls` calls of `stop_unit`.
        Stop {
            ok: bool,
            needles: Vec<String>,
            absent: Vec<String>,
            stop_calls: usize,
        },
    }

    /// What a cell did.
    #[derive(Debug)]
    struct Observed {
        /// The verb's Ok text (stop's line, status's word) or its error.
        result: Result<String, String>,
        launched: usize,
        stop_calls: usize,
        state_before: Option<String>,
        state_after: Option<String>,
        /// Whether the runner lock reads held once the verb returned.
        runner_live_after: bool,
    }

    /// A child process holding one of a cell's locks. Dropping it releases and reaps the
    /// child, unless the service double was given it to stop.
    struct TableHolder {
        child: Option<std::process::Child>,
        pid: u32,
        marks: tempfile::TempDir,
    }

    impl Drop for TableHolder {
        fn drop(&mut self) {
            let _ = std::fs::write(self.marks.path().join("release"), "");
            if let Some(mut child) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    /// Start a child of `name` that holds `kind`'s lock of `root`'s `phase` and wait
    /// until it holds it.
    fn hold_in_a_child(name: &str, root: &Path, kind: &str, path_dir: &Path) -> TableHolder {
        let marks = tempfile::tempdir().unwrap();
        let child = devflow_core::test_support::spawn_test_in_child(
            name,
            path_dir,
            &[
                (VERB_TABLE_HOLD_ENV, std::ffi::OsStr::new(kind)),
                (VERB_TABLE_ROOT_ENV, root.as_os_str()),
                (VERB_TABLE_MARKS_ENV, marks.path().as_os_str()),
            ],
        );
        let pid = child.id();
        let holder = TableHolder {
            child: Some(child),
            pid,
            marks,
        };
        assert!(
            wait_for_file(&holder.marks.path().join("held")),
            "the {kind} holder child never took its lock"
        );
        holder
    }

    /// The child side of [`hold_in_a_child`]: `true` when this process is one.
    fn hold_if_a_table_holder(phase: PhaseId) -> bool {
        let Ok(kind) = std::env::var(VERB_TABLE_HOLD_ENV) else {
            return false;
        };
        let root = PathBuf::from(std::env::var_os(VERB_TABLE_ROOT_ENV).unwrap());
        let marks = PathBuf::from(std::env::var_os(VERB_TABLE_MARKS_ENV).unwrap());
        let _hold = if kind == "runner" {
            liveness::claim_runner_lock(&root, phase).expect("hold the runner lock")
        } else {
            liveness::claim_phase_holder(&root, phase).expect("hold the phase holder")
        };
        std::fs::write(marks.join("held"), "").unwrap();
        assert!(
            wait_for_file(&marks.join("release")),
            "the holder was never released"
        );
        true
    }

    /// Plant `state` for `phase` under `root`, as a Codex Supervise run.
    fn plant_table_state(root: &Path, phase: PhaseId, state: TableState) {
        let mut planted = State::new(phase, AgentKind::Codex, Mode::Supervise, root.into());
        match state {
            TableState::None => return,
            TableState::Unloadable => {
                std::fs::create_dir_all(root.join(".devflow")).unwrap();
                std::fs::write(workflow::state_path(root, phase), "{ not json").unwrap();
                return;
            }
            TableState::ReservationOnly => {}
            TableState::Running => {
                planted.verification_baseline_captured = true;
                planted.stage = Stage::Code;
            }
            TableState::Stopped => {
                planted.verification_baseline_captured = true;
                planted.stage = Stage::Code;
                planted.stopped = true;
                planted.stop_reason = Some("operator stop".to_string());
            }
            TableState::Finished => {
                planted.verification_baseline_captured = true;
                planted.stage = Stage::Validate;
                planted.stopped = true;
                planted.finished = true;
            }
        }
        workflow::save_state(&planted).unwrap();
    }

    /// The outcome D-07's table gives `verb` over `lock` and `state`; `holder_pid` is
    /// the pid of the child holding the phase holder in a `HolderHeld` cell.
    fn table_expectation(
        verb: TableVerb,
        lock: TableLock,
        state: TableState,
        phase: PhaseId,
        holder_pid: u32,
    ) -> Expect {
        use TableState as S;
        let stop = format!("devflow stop --phase {phase}");
        let resume = format!("devflow resume --phase {phase}");
        let discard = format!("devflow discard --phase {phase}");
        let start = format!("devflow start --phase {phase}");
        let refuse = |needles: &[&str]| Expect::Refuse {
            needles: needles.iter().map(|n| (*n).to_string()).collect(),
            absent: Vec::new(),
        };
        let stopped =
            |ok: bool, needles: &[&str], absent: &[&str], stop_calls: usize| Expect::Stop {
                ok,
                needles: needles.iter().map(|n| (*n).to_string()).collect(),
                absent: absent.iter().map(|n| (*n).to_string()).collect(),
                stop_calls,
            };
        match (verb, lock) {
            (TableVerb::Stop, TableLock::HolderHeld) => stopped(
                false,
                &[&format!(
                    "another DevFlow verb holds phase {phase} (pid {holder_pid}) and is in \
                     progress"
                )],
                &[],
                0,
            ),
            (_, TableLock::HolderHeld) => unreachable!("only stop has holder-held cells"),
            (TableVerb::Start, TableLock::RunnerHeld) => refuse(&[&stop, "refusing to start"]),
            (TableVerb::Resume, TableLock::RunnerHeld) => refuse(&[&stop, "refusing to resume"]),
            (TableVerb::Discard, TableLock::RunnerHeld) => refuse(&[&stop]),
            (TableVerb::Cleanup, TableLock::RunnerHeld) => refuse(&["may still be live"]),
            (TableVerb::Status, TableLock::RunnerHeld) => Expect::Word(Some("live")),
            (TableVerb::Stop, TableLock::RunnerHeld) => match state {
                S::None | S::ReservationOnly => {
                    stopped(true, &["stopped during launch"], &["crashed"], 1)
                }
                S::Stopped => stopped(true, &["stopped (operator stop)"], &[], 1),
                S::Finished => stopped(true, &["had already finished"], &[], 1),
                S::Running => stopped(false, &["crashed", &resume, &discard], &[], 1),
                S::Unloadable => stopped(false, &["does not load", &discard], &[&resume], 1),
            },
            (TableVerb::Stop, TableLock::Free) => stopped(true, &["has no live run"], &[], 0),
            (TableVerb::Start, TableLock::Free) => match state {
                S::None | S::ReservationOnly => Expect::Launch,
                S::Running | S::Stopped => refuse(&["incomplete run", &resume, &discard]),
                S::Finished => refuse(&["finished", &discard]),
                S::Unloadable => refuse(&["does not load", &discard]),
            },
            (TableVerb::Resume, TableLock::Free) => match state {
                S::None => refuse(&["nothing to resume", &start]),
                S::ReservationOnly => refuse(&["never finished preparing", &start]),
                S::Running | S::Stopped => Expect::Launch,
                S::Finished => refuse(&["nothing to resume", &discard]),
                S::Unloadable => refuse(&["does not load", &discard]),
            },
            (TableVerb::Discard, TableLock::Free) => match state {
                S::None => refuse(&["nothing to discard"]),
                _ => Expect::Removes,
            },
            (TableVerb::Cleanup, TableLock::Free) => match state {
                S::ReservationOnly => Expect::Removes,
                _ => Expect::Keeps,
            },
            (TableVerb::Status, TableLock::Free) => Expect::Word(match state {
                S::None => None,
                S::ReservationOnly => Some("idle"),
                S::Running => Some("crashed"),
                S::Stopped => Some("stopped"),
                S::Finished => Some("finished"),
                S::Unloadable => Some("unloadable"),
            }),
        }
    }

    /// Run `verb` over `root`'s `phase` as the CLI does, through its library function.
    fn run_table_verb(verb: TableVerb, root: &Path, phase: PhaseId) -> Result<String, String> {
        let text = |result: Result<(), CliError>| {
            result
                .map(|()| String::new())
                .map_err(|err| err.to_string())
        };
        match verb {
            TableVerb::Start => start_for_test(root, phase).map(|()| String::new()),
            TableVerb::Resume => text(crate::pipeline_launch::resume(root, phase, None)),
            TableVerb::Discard => text(discard(root, phase, None)),
            TableVerb::Stop => stop_report(root, phase).map_err(|err| err.to_string()),
            TableVerb::Cleanup => text(cleanup(root)),
            TableVerb::Status => {
                text(status(root))?;
                Ok(run_status(root, phase, &existing_state(root, phase))
                    .map_or("no row", |status| status.word())
                    .to_string())
            }
        }
    }

    /// Build one cell's fixture in a scratch root, run its verb and say what it did.
    fn run_table_cell(
        name: &str,
        verb: TableVerb,
        lock: TableLock,
        state: TableState,
        phase: PhaseId,
        path_dir: &Path,
    ) -> (Observed, u32) {
        use crate::service::double;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_start_repo(root, phase);
        plant_table_state(root, phase, state);
        let state_file = workflow::state_path(root, phase);
        let mut holder = match lock {
            TableLock::Free => None,
            TableLock::RunnerHeld => Some(hold_in_a_child(name, root, "runner", path_dir)),
            TableLock::HolderHeld => Some(hold_in_a_child(name, root, "holder", path_dir)),
        };
        let holder_pid = holder.as_ref().map_or(0, |holder| holder.pid);
        if let (TableVerb::Stop, Some(holder)) = (verb, holder.as_mut())
            && lock == TableLock::RunnerHeld
        {
            let unit = crate::service::unit_name(root, phase).unwrap();
            double::register_stop_target(&unit, holder.child.take().unwrap());
        }
        let state_before = std::fs::read_to_string(&state_file).ok();
        let (launches, stops) = (double::launched_units().len(), double::stop_calls());
        let result = run_table_verb(verb, root, phase);
        let observed = Observed {
            result,
            launched: double::launched_units().len() - launches,
            stop_calls: double::stop_calls() - stops,
            state_before,
            state_after: std::fs::read_to_string(&state_file).ok(),
            runner_live_after: liveness::probe(&liveness::runner_lock_path(root, phase))
                .is_ok_and(|held| held == liveness::Held::Live),
        };
        double::end_runners(None, crate::service::TIMEOUT_STOP_SEC);
        drop(holder);
        (observed, holder_pid)
    }

    /// Why `observed` does not meet `expect`, or `None` when it does.
    fn table_mismatch(expect: &Expect, observed: &Observed, lock: TableLock) -> Option<String> {
        let unchanged = observed.state_before == observed.state_after;
        let carries = |text: &str, needles: &[String], absent: &[String]| {
            needles.iter().all(|needle| text.contains(needle.as_str()))
                && !absent.iter().any(|gone| text.contains(gone.as_str()))
        };
        let stop_calls = match expect {
            Expect::Stop { stop_calls, .. } => *stop_calls,
            _ => 0,
        };
        let launched = usize::from(matches!(expect, Expect::Launch));
        let met = observed.launched == launched
            && observed.stop_calls == stop_calls
            && match expect {
                Expect::Refuse { needles, absent } => {
                    unchanged
                        && observed
                            .result
                            .as_ref()
                            .is_err_and(|err| carries(err, needles, absent))
                }
                Expect::Launch => observed.result.is_ok(),
                Expect::Removes => {
                    observed.result.is_ok()
                        && observed.state_before.is_some()
                        && observed.state_after.is_none()
                }
                Expect::Keeps => unchanged && observed.result.is_ok(),
                Expect::Word(word) => {
                    unchanged && observed.result.as_deref() == Ok(word.unwrap_or("no row"))
                }
                Expect::Stop {
                    ok,
                    needles,
                    absent,
                    ..
                } => {
                    let line = match &observed.result {
                        Ok(line) | Err(line) => line,
                    };
                    unchanged
                        && observed.result.is_ok() == *ok
                        && carries(line, needles, absent)
                        && !(lock == TableLock::RunnerHeld && observed.runner_live_after)
                }
            };
        (!met).then(|| format!("expected {expect:?}, got {observed:?}"))
    }

    /// D-07 / 999.237: every verb x lock (held, free) x state (none, reservation-only,
    /// running, stopped, finished, unloadable), 72 cells, plus 6 `stop` cells with the
    /// phase holder held and the runner lock free (review pi-10), walked in one test. Each
    /// lock is held by a child process (Pitfall 5), in a process of its own; no cell
    /// reaches a real user manager (D-07b). Held/none and held/unloadable refuse for every
    /// mutating verb. Controls: each `start`/`resume` act cell records exactly one launch
    /// and each refusal none; the lock-free `stop` cells answer "no live run" with exit 0,
    /// against the holder-held cells' non-zero refusal; the running and unloadable cells of
    /// a stop under a held lock say crashed and unloadable, against the absent and
    /// reservation-only cells' `stopped during launch`.
    #[test]
    fn every_verb_follows_the_lock_and_state_table() {
        const NAME: &str = "commands::tests::every_verb_follows_the_lock_and_state_table";
        let phase = PhaseId::new(5171);
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::go_waiting_agent_dir("codex");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        if hold_if_a_table_holder(phase) {
            return;
        }
        let _runners = crate::service::double::LaunchedRunners;
        let path_dir = PathBuf::from(std::env::var_os("PATH").expect("the child's PATH"));
        let verbs = [
            TableVerb::Start,
            TableVerb::Resume,
            TableVerb::Discard,
            TableVerb::Stop,
            TableVerb::Cleanup,
            TableVerb::Status,
        ];
        let states = [
            TableState::None,
            TableState::ReservationOnly,
            TableState::Running,
            TableState::Stopped,
            TableState::Finished,
            TableState::Unloadable,
        ];
        let mut cells = Vec::new();
        for verb in verbs {
            for lock in [TableLock::RunnerHeld, TableLock::Free] {
                for state in states {
                    cells.push((verb, lock, state));
                }
            }
        }
        for state in states {
            cells.push((TableVerb::Stop, TableLock::HolderHeld, state));
        }
        assert_eq!(cells.len(), 78, "the table's cells");
        let mut failures = Vec::new();
        for (verb, lock, state) in cells {
            let (observed, holder_pid) = run_table_cell(NAME, verb, lock, state, phase, &path_dir);
            let expect = table_expectation(verb, lock, state, phase, holder_pid);
            if let Some(why) = table_mismatch(&expect, &observed, lock) {
                failures.push(format!("{verb:?} x {lock:?} x {state:?}: {why}"));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of 78 cells broke the table:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    /// D-01: `start --fallback-agent` persists the fallback, unused, in the run state the
    /// first supervisor reads, and says which agent it stands behind.
    #[test]
    fn start_persists_the_fallback_agent_in_the_run_state() {
        const NAME: &str = "commands::tests::start_persists_the_fallback_agent_in_the_run_state";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::go_waiting_agent_dir("codex");
            crate::test_support::add_agent_stub(path_dir.path(), "claude");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            let printed = String::from_utf8_lossy(&output.stdout);
            assert!(
                printed.contains(
                    "fallback agent: claude takes over the Code stage once if codex is rate-limited"
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
            AgentKind::Codex,
            Mode::Supervise,
            Some(AgentKind::Claude),
        );
        let _runners = crate::service::double::LaunchedRunners;
        result.expect("the start launches");
        let state = workflow::load_state(root, phase).unwrap();

        assert_eq!(state.agent, AgentKind::Codex);
        assert_eq!(state.fallback_agent, Some(AgentKind::Claude));
        assert!(!state.fallback_consumed, "a new fallback is unused");
        assert_eq!(
            crate::service::double::launched_units().len(),
            1,
            "the run launched"
        );
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

    /// A `start` that fails after its guards gives the phase back: a state its guard
    /// write created is gone, an earlier run's state is as it was, and no liveness file
    /// is left. The hold shows the guard write was really made, or a start that never
    /// wrote would pass every check after it.
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
        assert_eq!(observed.phase, phase, "the state loaded at the hold");
        let err = result.expect_err("no agent binary resolves");
        assert!(err.contains("not found"), "{err}");
        assert_eq!(
            std::fs::read(&state_file).ok(),
            before,
            "the state file is back as it was before the start"
        );
        assert_eq!(liveness_files(root), Vec::<String>::new());
    }

    /// A runner whose state vanished ends with no exit file and no successor, so a
    /// follower must end too, saying why. Control: while the state exists and its runner
    /// holds the lock it keeps following (with the lock free it would end at once, D-10).
    #[test]
    fn logs_follow_ends_when_the_followed_state_is_gone_and_no_exit_file_will_come() {
        const WAIT: std::time::Duration = std::time::Duration::from_secs(10);
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(5306);
        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.clone());
        workflow::save_state(&state).unwrap();
        let _runner = liveness::claim_runner_lock(&root, phase).expect("hold the runner lock");

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

    /// The state a `start` leaves when it is killed during preparation: what its real
    /// guard write left.
    fn state_of_a_start_killed_during_preparation(root: &Path, phase: PhaseId) -> State {
        state_of_a_start_killed_in_mode(root, phase, Mode::Auto)
    }

    /// [`state_of_a_start_killed_during_preparation`] for a start in `mode`.
    fn state_of_a_start_killed_in_mode(root: &Path, phase: PhaseId, mode: Mode) -> State {
        let fresh = State::new(phase, AgentKind::Claude, mode, root.to_path_buf());
        reserve_launch(
            root,
            phase,
            LaunchVerb::Start,
            GuardFacts::default(),
            Some(&fresh),
        )
        .unwrap();
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
        assert!(
            run_never_prepared(&killed),
            "the start's guard write left it"
        );

        let err = crate::pipeline_launch::resume(root, phase, None)
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
        assert_eq!(after.stage, Stage::Plan);
        assert!(!after.stopped, "the refusal marks nothing");

        let prepared = PhaseId::new(5302);
        let mut state = State::new(prepared, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.verification_baseline_captured = true;
        workflow::save_state(&state).unwrap();
        let err = crate::pipeline_launch::resume(root, prepared, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("not found"), "{err}");
    }

    /// A real `start` of what a killed one's refusal names, over `root`: a worktree start
    /// in Supervise mode with a stub `codex`, which returns once it has launched.
    fn start_as_refusal_names(root: &Path, phase: PhaseId) -> Result<(), String> {
        start(
            root,
            phase,
            AgentKind::Codex,
            Mode::Supervise,
            true,
            false,
            None,
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
            let path_dir = crate::test_support::go_waiting_agent_dir("codex");
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

        let result = start_as_refusal_names(root, phase);
        result.expect("the start the refusal names launches");
    }

    /// With a worktree already there, `start` refuses naming `devflow discard` (52.1
    /// D-09.1), so the refusal and the hints name discard and then `start`, and that pair
    /// launches.
    #[test]
    fn a_killed_start_with_a_worktree_names_discard_then_a_start_that_launches() {
        const NAME: &str = "commands::tests::a_killed_start_with_a_worktree_names_discard_then_a_start_that_launches";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::go_waiting_agent_dir("codex");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (root, phase) = (dir.path(), PhaseId::new(5305));
        init_start_repo(root, phase);
        let killed = state_of_a_start_killed_in_mode(root, phase, Mode::Supervise);
        let worktree_path = ensure_phase_worktree(root, phase, DEVELOP).unwrap();

        let err = refuse_unprepared_run(&killed).unwrap_err().to_string();
        let verb = format!(
            "devflow discard --phase {phase} && devflow start --phase {phase} --agent claude \
             --mode supervise"
        );
        assert!(err.contains(&format!("run `{verb}")), "{err}");
        assert!(err.contains(&*worktree_path.to_string_lossy()), "{err}");
        assert!(!err.contains("--force"), "{err}");
        assert!(!err.contains("no worktree"), "{err}");
        let row = not_live_row(&killed);
        assert!(row.starts_with("idle"), "{row}");
        assert!(row.contains(&format!("run `{verb}")), "{row}");

        let refused = start_as_refusal_names(root, phase).unwrap_err();
        assert!(
            refused.contains(&format!("devflow discard --phase {phase}")),
            "{refused}"
        );

        discard(root, phase, None).expect("the discard the refusal names runs");
        let result = start_as_refusal_names(root, phase);
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
        let killed = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        workflow::save_state(&killed).unwrap();
        let mut stopped = killed.clone();
        stopped.stopped = true;

        for (case, state) in [("killed", &killed), ("stopped after", &stopped)] {
            let row = not_live_row(state);
            assert!(row.starts_with("idle"), "{case}: {row}");
            assert!(
                row.contains(&format!("run `{start_verb}`")),
                "{case}: {row}"
            );
            assert!(!row.contains("devflow resume"), "{case}: {row}");
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
        let row = not_live_row(&prepared);
        assert!(row.contains(&format!("`{resume_verb}`")), "control: {row}");
        let findings = reconcile_phase(&facts_for(root, &prepared));
        assert!(
            findings
                .iter()
                .any(|finding| finding.repair.as_deref() == Some(resume_verb.as_str())),
            "control: a prepared run names resume"
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
        let guarded = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        workflow::save_state(&guarded).unwrap();
        let fresh = State::new(
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
            let fresh = &fresh;
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
                create_run_state(root, phase, fresh).unwrap();
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
            guarded.mode,
            Mode::Auto,
            "control: what was on disk differed"
        );
    }

    /// The row and the write-lock line `status` prints for `state`'s phase.
    fn rendered_status(state: &State) -> String {
        let root = &state.project_root;
        let mut lines = vec![not_live_row(state)];
        lines.extend(status_write_lock_line(root, state.phase));
        lines.join("\n")
    }

    /// OPS-04: `status` names the fallback agent's state as exactly one of `armed`,
    /// `spent` or `none`, prints a stopped run's recorded stop reason verbatim, and
    /// says a finished run (D-35) finished.
    #[test]
    fn status_prints_the_stop_reason_and_fallback_state() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let state_for = |phase: u32| {
            State::new(
                PhaseId::new(phase),
                AgentKind::Claude,
                Mode::Supervise,
                root.to_path_buf(),
            )
        };
        let fallback_word = |lines: &[String]| -> Vec<String> {
            lines
                .iter()
                .filter_map(|line| line.trim().strip_prefix("fallback: "))
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_string())
                .collect()
        };

        let mut armed = state_for(5251);
        armed.fallback_agent = Some(AgentKind::Codex);
        let mut spent = state_for(5252);
        spent.fallback_agent = Some(AgentKind::Codex);
        spent.fallback_consumed = true;
        let none = state_for(5253);
        for (state, word) in [(&armed, "armed"), (&spent, "spent"), (&none, "none")] {
            let lines = status_run_lines(state);
            assert_eq!(
                fallback_word(&lines),
                vec![word.to_string()],
                "exactly one fallback word: {lines:?}"
            );
        }

        let reason = "stopped after plan completed (--until plan)";
        let mut stopped = state_for(5254);
        stopped.stopped = true;
        stopped.stop_reason = Some(reason.to_string());
        let lines = status_run_lines(&stopped).join("\n");
        assert!(lines.contains(reason), "the stop reason verbatim: {lines}");
        assert!(lines.contains("run: stopped"), "{lines}");
        assert!(
            !lines.contains("finished"),
            "a halt is not a finish: {lines}"
        );

        let mut finished = state_for(5255);
        finished.stopped = true;
        finished.finished = true;
        finished.stop_reason = Some(crate::pipeline_gate::FINISHED_STOP_REASON.to_string());
        let lines = status_run_lines(&finished).join("\n");
        assert!(lines.contains("run: finished"), "{lines}");
        assert!(
            lines.contains(crate::pipeline_gate::FINISHED_STOP_REASON),
            "{lines}"
        );
        assert!(!lines.contains("run: active"), "{lines}");

        let in_flight = status_run_lines(&none).join("\n");
        assert!(
            !in_flight.contains("stop reason") && !in_flight.contains("finished"),
            "control: a run that has not stopped prints no stop reason: {in_flight}"
        );
    }

    /// OPS-02: a run whose recorded worktree no longer exists is never reported
    /// active, however live its agent pid looks, and the output
    /// names the missing worktree. Control: the same state with the directory present
    /// is reported active.
    #[test]
    fn status_does_not_report_a_run_with_a_missing_worktree_as_active() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5256);
        let worktree = root.join(".worktrees").join("phase-5256");
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.worktree_path = Some(worktree.clone());
        workflow::save_state(&state).unwrap();
        let pid_file = agent_result::agent_pid_path(root, phase);
        std::fs::create_dir_all(pid_file.parent().unwrap()).unwrap();
        std::fs::write(&pid_file, std::process::id().to_string()).unwrap();
        let states = vec![state.clone()];
        let shown = worktree.display().to_string();

        let lines = status_run_lines(&state).join("\n");
        assert!(!lines.contains("run: active"), "{lines}");
        assert!(lines.contains("run: not active"), "{lines}");
        assert!(
            lines.contains(&shown) && lines.contains("no longer exists"),
            "names the missing worktree: {lines}"
        );
        let header = active_phases_line(&states);
        let active = header.lines().next().unwrap_or_default();
        assert!(
            active.starts_with("active phases:") && !active.contains(&phase.to_string()),
            "the header does not call it active: {header}"
        );

        std::fs::create_dir_all(&worktree).unwrap();
        let lines = status_run_lines(&state).join("\n");
        assert!(lines.contains("run: active"), "control: {lines}");
        assert!(!lines.contains("no longer exists"), "control: {lines}");
        assert_eq!(
            active_phases_line(&states),
            format!("active phases: {phase}"),
            "control"
        );
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

    // The phase holder (50.1-29, D-17, D-18). A holder that `stop` signals runs as its own
    // process: `stop` would otherwise be signalling the test runner.

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

    /// D-17 (V-31): `start` refuses while another process holds the phase, before its
    /// guards. The control is the no-state fixture through `reserve_launch` alone: the
    /// guard write the absence check would see from a `start` that claimed late.
    #[test]
    fn start_refuses_while_another_process_holds_the_phase_before_reserving() {
        const NAME: &str =
            "commands::tests::start_refuses_while_another_process_holds_the_phase_before_reserving";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = crate::test_support::agent_free_dir_with_agent_stub("codex");
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
                error.contains(&format!("another DevFlow verb holds phase {phase}"))
                    && error.contains("in progress"),
                "{error}"
            );
            assert!(!error.contains("devflow resume"), "{error}");
            assert!(!error.contains("devflow stop"), "{error}");
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

        let reserved = reserve_launch(
            empty,
            phase,
            LaunchVerb::Start,
            GuardFacts::default(),
            Some(&dead),
        )
        .unwrap_or_else(|err| panic!("the control's guards pass: {err}"));
        assert!(
            workflow::state_path(empty, phase).exists(),
            "control: the guard write creates a state while the holder is held"
        );
        crate::pipeline_launch::abandon_reservation(empty, phase, reserved.created);
        assert!(
            !workflow::state_path(empty, phase).exists(),
            "control: giving the phase back removes it"
        );
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
        wait_for_released_write_lock(&root, phase);
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

    /// Take the phase's state write lock on another thread and hold it until the
    /// returned sender is dropped. Returns once the lock is held.
    fn hold_state_write_lock(
        root: &Path,
        phase: PhaseId,
    ) -> (std::thread::JoinHandle<()>, std::sync::mpsc::Sender<()>) {
        let (holding, wait_holding) = std::sync::mpsc::channel();
        let (release, wait_release) = std::sync::mpsc::channel::<()>();
        let root = root.to_path_buf();
        let holder = std::thread::spawn(move || {
            workflow::update_state(&root, phase, |_| {
                holding.send(()).unwrap();
                let _ = wait_release.recv_timeout(std::time::Duration::from_secs(30));
                Ok::<(), workflow::WorkflowError>(())
            })
            .unwrap();
        });
        wait_holding.recv().unwrap();
        (holder, release)
    }

    /// How long a released state write lock may keep reading held before the test
    /// calls it a leak.
    const RELEASED_WRITE_LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

    /// Wait for the released state write lock to read free, telling `on_held` about
    /// each reading that still says held.
    ///
    /// A fork in another test thread holds a duplicate of every descriptor until its
    /// exec, so a just-released lock can read held for a moment (999.221). A real leak
    /// outlasts the bound. The poll is the shared read `status` and `doctor` make, not
    /// `write_lock_is_free`: that probe takes an exclusive lock, and a fork during the
    /// probe would leave a child holding it, so the next shared read could still say
    /// held.
    fn wait_for_released_write_lock_with(root: &Path, phase: PhaseId, mut on_held: impl FnMut()) {
        devflow_core::test_support::wait_until(
            RELEASED_WRITE_LOCK_WAIT,
            "the state write lock to read free after its release",
            || {
                let status = workflow::write_lock_status(root, phase).unwrap();
                if status != workflow::WriteLockStatus::Free {
                    on_held();
                }
                status == workflow::WriteLockStatus::Free
            },
        );
    }

    /// [`wait_for_released_write_lock_with`], with nothing to tell.
    fn wait_for_released_write_lock(root: &Path, phase: PhaseId) {
        wait_for_released_write_lock_with(root, phase, || {});
    }

    /// 999.221 on demand: a child forked while the state write lock is held, and parked
    /// before any exec, keeps a duplicate of the lock's descriptor after the holder lets
    /// go, so a read right after the release says held. The released-lock wait outlasts
    /// it.
    ///
    /// No clock decides what the first reading sees: the parked child is released only
    /// by the wait's own on-held observer, or by its guard's drop once an assertion has
    /// failed.
    #[test]
    fn the_released_write_lock_control_outlasts_a_fork_held_before_exec() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(5190);
        let state = State::new(phase, AgentKind::Claude, Mode::Supervise, root.clone());
        workflow::save_state(&state).unwrap();
        let (holder, release) = hold_state_write_lock(&root, phase);
        let mut held_fork = Some(devflow_core::test_support::fork_held_before_exec());

        drop(release);
        holder.join().unwrap();
        let mut times_held = 0_u32;
        wait_for_released_write_lock_with(&root, phase, || {
            times_held += 1;
            if let Some(fork) = held_fork.take() {
                fork.release();
            }
        });

        assert!(
            times_held >= 1,
            "the held fork was never seen holding the lock: this run did not reproduce 999.221"
        );
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

    /// Control: a lock that stays held ends the released-lock wait at its bound with
    /// the wait's own message. It does not hang and does not pass.
    ///
    /// Runs in its own process: the parked fork holds every descriptor of the process
    /// it was forked from for the whole bound, and in a shared test process those
    /// would be the sibling tests' descriptors too.
    #[test]
    fn the_released_write_lock_wait_times_out_while_a_fork_keeps_the_lock() {
        const NAME: &str =
            "commands::tests::the_released_write_lock_wait_times_out_while_a_fork_keeps_the_lock";
        let dir = tempfile::tempdir().unwrap();
        if !devflow_core::test_support::in_child_test(NAME) {
            // The child half execs nothing, so its PATH is this empty directory.
            let output = devflow_core::test_support::run_test_in_child(NAME, dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(5191);
        let state = State::new(phase, AgentKind::Claude, Mode::Supervise, root.clone());
        workflow::save_state(&state).unwrap();
        let (holder, release) = hold_state_write_lock(&root, phase);
        let held_fork = devflow_core::test_support::fork_held_before_exec();
        drop(release);
        holder.join().unwrap();
        let started = std::time::Instant::now();

        let waited = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            wait_for_released_write_lock(&root, phase);
        }));

        let waited_for = started.elapsed();
        held_fork.release();
        let payload = waited.expect_err("the wait must not pass while a fork keeps the lock");
        let message = payload
            .downcast_ref::<String>()
            .expect("the wait panics with a formatted message");
        assert!(
            message.contains(
                "timed out after 2s waiting for the state write lock to read free after its release"
            ),
            "the wait fails with its own timeout text: {message}"
        );
        assert!(
            waited_for >= RELEASED_WRITE_LOCK_WAIT,
            "the wait gave up after {waited_for:?}, before its {RELEASED_WRITE_LOCK_WAIT:?} bound"
        );
    }
}
