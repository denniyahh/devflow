//! Pipeline seam A (D-06): launching a stage's agent and driving the
//! `advance` decision after a monitored agent exits. Extracted mechanically
//! (19-08, D-09 pure move) out of `main.rs` — every function below is
//! byte-identical to its pre-move body modulo an added `pub(crate)` and
//! adjusted `use` paths.
//!
//! **This module participates in the pipeline's three-way module cycle
//! (19-RESEARCH.md Pattern 1), and that is intentional:** [`launch_stage`]
//! calls [`crate::preflight::run_preflight`] on the way in, while
//! `run_preflight`'s `Advance` arm calls [`launch_stage_inner`] back
//! directly (18-07, D-18f) — the bidirectional preflight/launch pair is
//! unchanged by this split, just repointed to a named module. Closing the
//! OTHER side of the cycle, `pipeline_gate::transition` (and
//! `loop_back_to_code`) call [`launch_stage`] at their last line — that
//! call is what actually drives the state machine forward after a stage
//! transition, and is the edge that closes `launch → outcomes → gate →
//! launch` back to this module. Rust permits cyclic module references
//! (only the crate dependency graph must be acyclic), so this compiles
//! cleanly. All three pipeline modules import from each other directly —
//! see `pipeline_gate`'s module doc comment for the explicit caveat that
//! this cycle is NOT a wave-parallelism promise for future pipeline work.

use crate::CliError;
use crate::pipeline_gate::transition;
use crate::pipeline_outcomes::{
    ValidateOutcome, classify_validate_outcome, handle_infra_outcome, handle_rate_limited_outcome,
    handle_ship_failure, handle_ship_outcome, handle_stage_failure, handle_validate_outcome,
    truncate_reason,
};
use crate::preflight::{ensure_agent_binary, run_preflight, worktree_writable_roots};
use crate::staleness::enforce_build_staleness;
use devflow_core::config::{GitFlowConfig, capture_retention};
use devflow_core::outcome_policy::{self, Action};
use devflow_core::prompt;
use devflow_core::stage::Stage;
use devflow_core::state::State;
use devflow_core::{agent_result, agents, events, lock, monitor, workflow};
use std::path::Path;

/// The post-preflight body of [`launch_stage`]: self-dogfood build-staleness
/// enforcement, capture archival/rollover, and spawning the monitor.
/// Extracted (18f, D-18f) so `run_preflight`'s `Advance` arm can call it
/// directly and skip the just-adjudicated preflight check, while every
/// other caller keeps going through [`launch_stage`]'s full path (readiness
/// resolution, `ensure_agent_binary`, then `run_preflight`).
///
/// Recomputes `prompt`/`adapter`/`roots`/`program`/`args` from `state` and
/// `prompt_override` — deliberately NOT threaded through as parameters.
/// They are pure functions of `state` and the prompt override; recomputing
/// them here (rather than widening `run_preflight`'s signature to carry
/// them from `launch_stage`'s earlier resolution) keeps this function
/// callable entirely on its own, which is exactly what `run_preflight`'s
/// `Advance` arm needs. This does not duplicate `worktree_writable_roots`'s
/// logic — both call sites call the same shared helper.
pub(crate) fn launch_stage_inner(
    state: &mut State,
    prompt_override: Option<String>,
    archived_stage: Option<Stage>,
) -> Result<(), CliError> {
    // WR-04 (18-fix): clear the prior stage's monitor pid up front, before
    // any fallible step below (`ensure_agent_binary`, `enforce_build_staleness`)
    // can return early via `?`. Without this, a failed relaunch left
    // `state.stage` already advanced (by `transition()`, before this
    // function was ever called) alongside a stale `monitor_pid` still
    // naming the PREVIOUS stage's (now-dead) monitor — `liveness()` then
    // misreports `Stuck → devflow resume`, even when the real remedy is
    // unrelated (e.g. rebuild after a staleness block). The real pid is
    // set again below once `monitor::spawn_monitor` actually succeeds.
    state.monitor_pid = None;
    workflow::save_state(state)?;

    let prompt = prompt_override.unwrap_or_else(|| {
        prompt::stage_prompt_for_project(state.stage, state.phase, &state.project_root)
    });
    let adapter = agents::adapter_for(state.agent);
    // In worktree mode the agent's cwd is the linked worktree, but git
    // metadata for commits lives under the main repo's `.git/` — sandboxed
    // agents need it (and the worktree admin dir, which Codex read-only-
    // mounts otherwise) writable (13-06 dogfood finding).
    let roots = state
        .worktree_path
        .as_deref()
        .map(|wt| worktree_writable_roots(&state.project_root, wt))
        .unwrap_or_default();
    let (program, args) = adapter.exec_command(state.phase, &prompt, &roots);
    ensure_agent_binary(program)?;

    let project_root = state.project_root.clone();

    // 17d (Task 2, D-17-D-19): self-dogfood build-staleness gate — also
    // before spawn_monitor, so a stale DevFlow-on-itself run never even
    // reaches the agent.
    enforce_build_staleness(
        &project_root,
        state,
        env!("DEVFLOW_BUILD_COMMIT"),
        env!("DEVFLOW_BUILD_DIRTY") == "true",
    )?;

    if let Some(stamp) = agent_result::archive_phase_files(
        &state.project_root,
        state
            .worktree_path
            .as_deref()
            .unwrap_or(&state.project_root),
        state.phase,
        capture_retention(&state.project_root),
    )
    .map_err(|err| {
        CliError::Message(format!(
            "could not archive phase {} capture before rollover: {err}",
            state.phase
        ))
    })? {
        events::emit(
            &state.project_root,
            state.phase,
            "capture_archived",
            serde_json::json!({
                "stage": archived_stage.unwrap_or(state.stage).to_string(),
                "to_stage": state.stage.to_string(),
                "stamp": stamp,
            }),
        );
    }
    let pid = monitor::spawn_monitor(state, program, &args, &adapter.extra_env())
        .map_err(|err| CliError::Message(format!("could not spawn monitor: {err}")))?;
    // `transition()` calls `workflow::save_state` BEFORE `launch_stage`, so a
    // pid recorded only in memory here is lost unless it is written again
    // (18b).
    state.monitor_pid = Some(pid);
    workflow::save_state(state)?;
    events::emit(
        &state.project_root,
        state.phase,
        "stage_launched",
        serde_json::json!({
            "stage": state.stage.to_string(),
            "agent": state.agent.to_string(),
            "monitor_pid": pid,
        }),
    );
    println!(
        "stage {} → launched {} (monitor pid {pid})",
        state.stage,
        adapter.name()
    );
    Ok(())
}

/// Spawn the background monitor that owns the agent for `state.stage`. The
/// monitor calls `devflow advance` when the agent exits. An optional
/// `prompt_override` is used for Code loop-backs (fix prompts).
///
/// Resolves the prompt/adapter/roots/program, validates the agent binary,
/// then runs the readiness gate ([`run_preflight`]) before delegating to
/// [`launch_stage_inner`] for the actual spawn. Every EXISTING caller of
/// this function keeps getting the full path including preflight — the
/// ONLY caller of `launch_stage_inner` directly is `run_preflight`'s own
/// `Advance` arm (18f, D-18f), which is skipping a check it just
/// adjudicated for this one relaunch, not granting a standing bypass
/// (T-18-28: the skip must never leak beyond the single stage a human
/// approved).
pub(crate) fn launch_stage(
    state: &mut State,
    prompt_override: Option<String>,
    archived_stage: Option<Stage>,
) -> Result<(), CliError> {
    let adapter = agents::adapter_for(state.agent);
    let prompt = prompt_override.clone().unwrap_or_else(|| {
        prompt::stage_prompt_for_project(state.stage, state.phase, &state.project_root)
    });
    let roots = state
        .worktree_path
        .as_deref()
        .map(|wt| worktree_writable_roots(&state.project_root, wt))
        .unwrap_or_default();
    let (program, _args) = adapter.exec_command(state.phase, &prompt, &roots);
    ensure_agent_binary(program)?;

    // 17c (Task 1, D-13-D-16): a scoped readiness gate runs before any agent
    // time is spent — a failing check surfaces as a named preflight gate +
    // notify (never a hard exit, D-15), not here.
    //
    // CR-01 (17-08 gap closure): `run_preflight` returns `Ok(false)` when a
    // failing check was ALREADY resolved via a full retried launch (or an
    // abort) — this frame must not run any more launch steps in that case,
    // or the agent gets spawned a second time for the same stage.
    let project_root = state.project_root.clone();
    if !run_preflight(&project_root, state, adapter.as_ref())? {
        return Ok(());
    }

    launch_stage_inner(state, prompt_override, archived_stage)
}

/// Resume a rate-limited or infra-paused phase from its saved stage (review
/// consensus #5). Loads the persisted `.devflow/state-{NN}.json` and
/// relaunches its saved stage via [`launch_stage`] — unlike `start`, this
/// does NOT call `State::new`, `feature_start`, or `ensure_phase_worktree`:
/// the branch/worktree already exist and agent/mode are read from the saved
/// state, so neither needs to be passed as a flag and the workflow is never
/// reset to Define.
///
/// 20c (review: Codex MEDIUM — resume semantics): a phase halted by
/// `devflow start --until <stage>` persists `stopped`/`stop_reason`/
/// `stop_until`. Without clearing them here, `state.stop_until ==
/// Some(from)` would immediately re-stop the phase the next time
/// `transition()` ran, and the phase would remain marked `stopped` forever
/// even though the operator explicitly asked to resume past it. Cleared and
/// persisted BEFORE `launch_stage`, so a reload mid-relaunch already sees
/// the phase as no longer stopped.
pub(crate) fn resume(project_root: &Path, phase: u32) -> Result<(), CliError> {
    let _lock = match lock::acquire(project_root, phase) {
        Ok(guard) => guard,
        Err(lock::LockError::Contended { pid, path: _ }) => {
            return Err(CliError::Message(format!(
                "another devflow process (pid {pid}) is already running"
            )));
        }
        Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
    };
    let mut state = workflow::load_state(project_root, phase)?;
    state.stopped = false;
    state.stop_reason = None;
    state.stop_until = None;
    workflow::save_state(&state)?;
    launch_stage(&mut state, None, None)
}

/// The single active phase: `Ok(Some)` when exactly one is active, `Ok(None)`
/// when none, and an error naming the candidates when several are — shared by
/// `advance`'s legacy fallback and `logs`'s default-phase resolution so the
/// ambiguity rule and message live in one place.
pub(crate) fn single_active_phase(project_root: &Path) -> Result<Option<u32>, CliError> {
    let states = workflow::list_states(project_root);
    match states.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(one.phase)),
        many => Err(CliError::Message(format!(
            "multiple active phases ({}) — pass --phase to pick one",
            many.iter()
                .map(|s| s.phase.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Resolve which phase a bare `devflow advance` (no `--phase`) refers to:
/// only unambiguous when exactly one phase is active. Exists for monitors
/// spawned by a pre-14a binary that doesn't pass `--phase`.
pub(crate) fn resolve_sole_active_phase(project_root: &Path) -> Result<u32, CliError> {
    single_active_phase(project_root)?
        .ok_or_else(|| CliError::Message("no active DevFlow state — nothing to advance".into()))
}

/// Advance the stage machine after a monitored agent for `state.stage` exits.
/// Invoked by the monitor process; not normally run by a human.
pub(crate) fn advance(project_root: &Path, phase: Option<u32>) -> Result<(), CliError> {
    // 13-DEFERRED-CR-03 fix shape #2: the phase is threaded in by the monitor
    // (recorded at spawn time), so advance's identity never depends on a
    // shared state singleton — under `devflow parallel`, each monitor
    // advances exactly its own phase. The Option fallback only serves
    // monitors spawned by an older binary.
    let phase = match phase {
        Some(phase) => phase,
        None => match resolve_sole_active_phase(project_root) {
            Ok(phase) => phase,
            Err(err) => {
                // 14-CR-06: a legacy monitor's bare `advance` failing here
                // would otherwise be invisible (its output goes to
                // /dev/null) and its phase silently stalls — record the
                // failure in events.jsonl. Phase 0 is the "could not
                // attribute a phase" sentinel; no real phase is 0.
                events::emit(
                    project_root,
                    0,
                    "advance_failed",
                    serde_json::json!({ "reason": err.to_string() }),
                );
                return Err(err);
            }
        },
    };
    // CR-03 (13-REVIEW.md): the lock is scoped per-phase, not per-project.
    // advance() holds it across a gate's multi-day blocking wait, and every
    // successful run ends at a mandatory Ship gate — a project-wide lock
    // would starve `devflow parallel`'s sibling phases with no retry.
    let _lock = match lock::acquire(project_root, phase) {
        Ok(guard) => guard,
        Err(lock::LockError::Contended { pid, path: _ }) => {
            return Err(CliError::Message(format!(
                "another devflow process (pid {pid}) is already running"
            )));
        }
        Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
    };
    // Load under the lock: with per-phase state files keyed by the same
    // phase as the lock, there is no cross-phase TOCTOU left by
    // construction — a concurrent advance of another phase touches a
    // different file and a duplicate advance of THIS phase is excluded by
    // the lock itself.
    let mut state = workflow::load_state(project_root, phase)?;

    let git_flow = GitFlowConfig::default();
    let result = agent_result::evaluate_agent_result(project_root, &state, &git_flow)
        .map_err(|err| CliError::Message(format!("could not evaluate agent result: {err}")))?;
    let stage = state.stage;
    println!("stage {stage} finished with status {:?}", result.status);
    if let Some(reason) = &result.reason {
        println!("  detail: {reason}");
    }
    events::emit(
        project_root,
        phase,
        "advance_evaluated",
        serde_json::json!({
            "stage": stage.to_string(),
            "status": result.status.as_wire_str(),
            "verdict": result.verdict.map(|v| format!("{v:?}").to_ascii_lowercase()),
            "decided_by_layer": result.decided_by_layer,
            "reason": result.reason.as_deref().map(truncate_reason),
        }),
    );

    // D-01/D-06: dispatch on the exhaustive outcome_policy::decide_action
    // table (no wildcard arm upstream) so a new/unhandled AgentStatus variant
    // is a compile error here rather than a silent advance. Replaces the old
    // `matches!(Failed | RateLimited)` boolean, which let Unknown fall
    // through into the success arm below.
    match outcome_policy::decide_action(stage, result.status) {
        Action::Advance => match stage {
            Stage::Define => transition(project_root, &mut state, Stage::Plan),
            Stage::Plan => transition(project_root, &mut state, Stage::Code),
            Stage::Code => transition(project_root, &mut state, Stage::Validate),
            Stage::Validate => {
                // 13b verdict-vs-ran + 18e: the Validate prompt now REQUIRES
                // a verdict, so ONLY an explicit `verdict: pass` advances to
                // Ship. A missing verdict is a fail-safe (gate/loop), NOT a
                // silent pass — closes the composition bug where a
                // marker-less/verdict-less Validate could otherwise reach
                // Ship. `classify_validate_outcome` additionally resolves
                // the `external_verify` three-way matrix (D-18e): agreement
                // advances, disagreement/no-verdict gates immediately.
                handle_validate_outcome(
                    project_root,
                    &mut state,
                    classify_validate_outcome(&result),
                )
            }
            Stage::Ship => handle_ship_outcome(project_root, &mut state),
        },
        Action::GateReview => match stage {
            // Validate failures drive the Code↔Validate loop (or a gate).
            Stage::Validate => {
                handle_validate_outcome(project_root, &mut state, ValidateOutcome::Failed)
            }
            // Ship distinguishes an agent crash (AgentFailed) from a review
            // rejection (ReviewFailed, `review:`-prefixed reason).
            Stage::Ship => handle_ship_failure(project_root, &mut state, result.reason),
            // Every other non-Validate failure (incl. Unknown, D-06) is
            // never silent (WR-11): it always fires a gate + notify instead
            // of returning a bare error or silently advancing.
            _ => handle_stage_failure(project_root, &mut state, stage, result.reason),
        },
        // ResourceKilled/AgentUnavailable: a dedicated infra path, identical
        // for every stage (including Validate/Ship) — MUST NOT route through
        // handle_validate_outcome/handle_ship_failure, which would bump
        // consecutive_failures (review consensus #4, D-08).
        Action::GateInfra => handle_infra_outcome(project_root, &mut state, stage, result.reason),
        // RateLimited: auto-resume via the primary loop's single-agent cron
        // path (D-09), bounded by the shared infra-failure ceiling (D-08).
        Action::AutoResume => {
            handle_rate_limited_outcome(project_root, &mut state, phase, stage, result.reason)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use devflow_core::gates::Gates;
    use devflow_core::mode::Mode;
    use devflow_core::state::AgentKind;

    /// 18b: after `launch_stage` spawns a monitor, the persisted state file
    /// for that phase carries the monitor's pid — `transition()` saves state
    /// BEFORE calling `launch_stage`, so the pid must be saved again inside
    /// `launch_stage` or it is lost.
    #[test]
    fn launch_stage_persists_monitor_pid_for_reload() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 65;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        workflow::save_state(&state).unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = launch_stage(&mut state, None, None);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
        result.unwrap();

        assert!(
            state.monitor_pid.is_some(),
            "launch_stage must record the monitor pid on the in-memory state"
        );
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.monitor_pid, state.monitor_pid,
            "the monitor pid recorded by launch_stage must be persisted to disk, \
             since transition() saves state before launch_stage runs"
        );
    }
    /// 20c (review: Codex MEDIUM — resume semantics): a phase halted by
    /// `--until <stage>` has `stopped`/`stop_reason`/`stop_until` persisted.
    /// `resume` must clear all three BEFORE relaunching — otherwise
    /// `transition()`'s `stop_until == Some(from)` check would immediately
    /// re-stop the phase the next time it advances, and the phase would
    /// stay marked `stopped` forever despite the operator's explicit
    /// resume. Asserts on the persisted state (not just `resume`'s exit
    /// code), since `transition()` saves state before `launch_stage` runs.
    #[test]
    fn resume_clears_stop_marker_and_advances_past_stop_point() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 66;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        state.stop_until = Some(Stage::Plan);
        state.stopped = true;
        state.stop_reason = Some("stopped after plan completed (--until plan)".to_string());
        workflow::save_state(&state).unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = resume(root, phase);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert!(
            !reloaded.stopped,
            "resume must clear stopped so the phase is no longer marked halted"
        );
        assert_eq!(
            reloaded.stop_reason, None,
            "resume must clear stop_reason alongside stopped"
        );
        assert_eq!(
            reloaded.stop_until, None,
            "resume must clear stop_until so the phase does not immediately re-stop \
             the next time it advances past Plan"
        );
    }

    /// D-01/D-06 regression: a Code-stage `Unknown` outcome (Layer 3's
    /// "process gone but commits exist" case) must route through
    /// `handle_stage_failure`'s never-silent gate, never
    /// `transition(.., Stage::Validate)`. Drives a real `advance()` on a
    /// scoped thread, polling for the Code gate file (not a Validate one) to
    /// prove the dispatch never took the success/Advance arm.
    #[test]
    fn code_unknown_does_not_transition_to_validate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = 72;
        let branch = format!("feature/phase-{phase:02}");
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .status()
                    .unwrap()
                    .success(),
                "git {args:?} failed"
            );
        };
        git(&["checkout", "-q", "-b", &branch, "develop"]);
        std::fs::write(root.join("work.txt"), "wip\n").unwrap();
        git(&["add", "work.txt"]);
        git(&["commit", "-q", "-m", "wip commit"]);
        git(&["checkout", "-q", "develop"]);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let code_gate = Gates::gate_path(root, phase, Stage::Code);
        let validate_gate = Gates::gate_path(root, phase, Stage::Validate);
        let response_path = Gates::response_path(root, phase, Stage::Code);

        std::thread::scope(|scope| {
            scope.spawn(|| {
                advance(root, Some(phase)).unwrap();
            });

            let mut seen = false;
            for _ in 0..150 {
                if code_gate.exists() {
                    seen = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            assert!(
                seen,
                "an Unknown Code outcome must fire a never-silent gate, not advance silently"
            );
            assert!(
                !validate_gate.exists(),
                "an Unknown Code outcome must never transition to Validate"
            );

            std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
        });
    }

    /// WR-04 (18-fix): an early failure in `launch_stage_inner` — before
    /// `monitor::spawn_monitor` ever runs — must not leave a stale
    /// `monitor_pid` behind. Pre-fix, `state.monitor_pid` still named the
    /// PREVIOUS stage's (now-dead) monitor after `ensure_agent_binary`
    /// returned early via `?`, and `liveness()`/`doctor` then misreported
    /// `Stuck → devflow resume` — the wrong remedy for what's actually an
    /// agent-binary/staleness failure. PATH is neutralized to a `git`-only
    /// directory under `ENV_MUTEX`, mirroring `transition_resets_infra_failures`,
    /// so `ensure_agent_binary("claude")` reliably fails without touching a
    /// real agent CLI and without racing other PATH-mutating tests.
    #[test]
    fn launch_stage_inner_clears_monitor_pid_on_early_failure() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 93;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        // A stale pid from a prior stage's now-dead monitor — this is what
        // must be cleared, not carried forward into the new stage.
        state.monitor_pid = Some(999_999);
        workflow::save_state(&state).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let result = launch_stage_inner(&mut state, None, None);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert!(
            result.is_err(),
            "ensure_agent_binary must fail against the neutralized, agent-free PATH"
        );
        assert_eq!(
            state.monitor_pid, None,
            "an early launch failure must clear the stale monitor_pid in-memory, not carry it \
             forward from the previous stage"
        );
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.monitor_pid, None,
            "the monitor_pid clear must be persisted to state.json, not just in-memory"
        );
    }
    /// D-10: `advance_evaluated` emits `status` via `AgentStatus::as_wire_str()`
    /// (never the Debug-lowercase formatter that collapses `ResourceKilled`
    /// into `resourcekilled`) and carries the `decided_by_layer` evidence
    /// field.
    #[test]
    fn advance_evaluated_emits_wire_status_and_decided_by_layer_for_resource_killed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 78;
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::exit_code_path(root, phase), "137").unwrap();

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        advance(root, Some(phase)).unwrap();

        // advance_evaluated isn't the last event once the infra gate/abort
        // path runs, so read the raw log and find it by name rather than
        // using `last_event_for_phase`.
        let contents = std::fs::read_to_string(events::events_path(root)).unwrap();
        let event = contents
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|e| e["event"] == "advance_evaluated")
            .expect("advance_evaluated event recorded");
        assert_eq!(event["status"], "resource_killed");
        assert_ne!(event["status"], "resourcekilled");
        assert_eq!(event["decided_by_layer"], 2);
    }
}
