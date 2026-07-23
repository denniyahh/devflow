//! Pipeline seam C (D-06): stage transitions, gate firing and resolution,
//! loop-backs, workflow completion, and abort. Extracted mechanically
//! (19-08, D-09 pure move) out of `main.rs` — every function below is
//! byte-identical to its pre-move body modulo an added `pub(crate)` and
//! adjusted `use` paths.
//!
//! **This module closes the pipeline's three-way module cycle
//! (19-RESEARCH.md Pattern 1):** [`transition`] and [`loop_back_to_code`]
//! both call [`crate::pipeline_launch::launch_stage`] at their final step —
//! that call is what closes the cycle `pipeline_launch (advance) →
//! pipeline_outcomes (handle_*_outcome) → pipeline_gate (transition/
//! run_gate/finish_workflow) → pipeline_launch (launch_stage)` back to
//! where it started. This cycle is the state machine's real control flow
//! (Code → Validate → Ship, with loop-backs), and Rust permits cyclic
//! module references — only the crate dependency graph must be acyclic —
//! so this compiles cleanly. **A future change to pipeline logic is likely
//! to touch two or three of these files together:** the split buys
//! `pub(crate)` boundaries, reviewability, and wave independence for the
//! *other* clusters, not pipeline-internal parallelism (19-RESEARCH.md
//! Pitfall 1).

use crate::CliError;
use crate::config_parse::{foreground_gate_timeout_secs, gate_timeout_secs};
use crate::pipeline_launch::launch_stage;
use crate::pipeline_outcomes::{run_checkout_hooks, truncate_reason};
use devflow_core::gates::{self, GateAction, GateResponse, Gates};
use devflow_core::hooks;
use devflow_core::mode;
use devflow_core::prompt::{self, FixType};
use devflow_core::stage::Stage;
use devflow_core::state::State;
use devflow_core::{events, lock, workflow};
use std::path::Path;
use tracing::info;

/// Fire the hooks for `from → to`, persist the new stage, and launch its agent.
///
/// `infra_failures` resets unconditionally on every successful transition
/// (CR-01, 17-06 gap closure). Without this, an infra-fault ceiling meant to
/// bound a *stuck loop* (D-08, [`mode::MAX_INFRA_FAILURES`]) instead
/// accumulates across a phase's entire lifetime — several well-spaced,
/// cleanly-resolved infra faults would falsely reach the ceiling and
/// hard-abort a long-running but otherwise healthy phase.
///
/// `consecutive_failures` clears on every transition EXCEPT Code→Validate
/// (18d, [`mode::transition_resets_consecutive_failures`]): that hop is
/// crossed on every single Code↔Validate retry cycle, so unconditionally
/// clearing it there made [`mode::MAX_CONSECUTIVE_FAILURES`] unreachable for
/// the exact loop it bounds. The two counters deliberately no longer share a
/// single reset condition.
pub(crate) fn transition(
    project_root: &Path,
    state: &mut State,
    to: Stage,
) -> Result<(), CliError> {
    let from = state.stage;

    // 20c: `devflow start --until <stage>` halts cleanly once the requested
    // stage has completed — checked here, at the TOP, before anything else
    // runs. `stop_until == Some(from)` means the JUST-COMPLETED stage (the
    // one whose outcome triggered this call) is the requested stop point;
    // checking `to` instead would halt BEFORE the target stage ever ran
    // (review: Codex HIGH off-by-one). This bypasses the from→to checkout
    // hooks, `state.stage = to`, the normal `"transition"` event, and
    // `launch_stage` — no new monitor is spawned, and `loop_back_to_code`
    // (a retry, not an advance) is untouched by this check.
    if state.stop_until == Some(from) {
        state.stopped = true;
        state.stop_reason = Some(format!("stopped after {from} completed (--until {from})"));
        // T-20-03b / doctor gap (D-09): clear monitor_pid/gate_pending so
        // neither check_dead_agent nor check_dead_monitor misreports this
        // intentional stop as a crashed agent or dead monitor.
        state.monitor_pid = None;
        state.gate_pending = false;
        workflow::save_state(state)?;
        events::emit(
            project_root,
            state.phase,
            "workflow_finished",
            serde_json::json!({
                "reason": "stopped_at",
                "stage": from.to_string(),
            }),
        );
        return Ok(());
    }

    let _ = run_checkout_hooks(
        project_root,
        state,
        &hooks::hooks_for_transition(from, to),
        to,
    );
    state.stage = to;
    if mode::transition_resets_consecutive_failures(from, to) {
        state.consecutive_failures = 0;
    }
    state.infra_failures = 0;
    state.gate_pending = false;
    workflow::save_state(state)?;
    events::emit(
        project_root,
        state.phase,
        "transition",
        serde_json::json!({
            "from": from.to_string(),
            "to": to.to_string(),
        }),
    );
    launch_stage(state, None, Some(from))
}

/// Loop the pipeline back to Code with the given fix prompt (`GapsOnly` for a
/// Validate rejection, `AuditFix` for a Ship `review:` rejection).
pub(crate) fn loop_back_to_code(
    project_root: &Path,
    state: &mut State,
    fix: FixType,
) -> Result<(), CliError> {
    let from = state.stage;
    let prompt = prepare_loop_back_to_code(project_root, state, fix)?;
    launch_stage(state, Some(prompt), Some(from))
}

/// The state-mutating half of `loop_back_to_code`, split out so it's
/// unit-testable without spawning a real agent process (`launch_stage`
/// invokes the actual configured agent CLI). Cleans up the stale gate for
/// the stage the gate fired on (CR-01), moves `state` to Code, persists it,
/// and returns the fix prompt the caller should launch with.
pub(crate) fn prepare_loop_back_to_code(
    project_root: &Path,
    state: &mut State,
    fix: FixType,
) -> Result<String, CliError> {
    // Capture the stage the gate actually fired on before it's mutated below,
    // so cleanup targets the right stage's gate files (see CR-01: a stale
    // response/ack left on disk after a loop-back is silently reused by a
    // later gate for the same phase+stage).
    let gate_stage = state.stage;
    let _ = Gates::cleanup(project_root, state.phase, gate_stage);
    state.stage = Stage::Code;
    state.gate_pending = false;
    workflow::save_state(state)?;
    events::emit(
        project_root,
        state.phase,
        "loop_back",
        serde_json::json!({
            "from": gate_stage.to_string(),
            "consecutive_failures": state.consecutive_failures,
        }),
    );
    println!(
        "looping back to Code (validate failures: {})",
        state.consecutive_failures
    );
    Ok(prompt::fix_prompt(fix, state.phase))
}

/// Run the terminal hooks (version bump + branch cleanup) and clear state.
///
/// Uses [`gate_timeout_secs`]'s multi-day production default for the
/// retry-gate wait below — safe for every caller EXCEPT the foreground
/// `ship_override` path (WR-02), which calls
/// [`finish_workflow_with_gate_timeout`] directly with a bounded timeout
/// instead.
pub(crate) fn finish_workflow(project_root: &Path, state: &mut State) -> Result<(), CliError> {
    finish_workflow_with_gate_timeout(project_root, state, gate_timeout_secs())
}

/// `finish_workflow`'s body, parameterized on how long the retry-gate poll
/// (below) waits for a response before failing fast (WR-02, phase 20
/// review). Every caller reached through a detached monitor process should
/// keep using `finish_workflow`'s multi-day default — invisible to an
/// operator's terminal by construction. `ship_override` is the one caller
/// invoked directly from the foreground CLI, so it passes
/// [`crate::config_parse::foreground_gate_timeout_secs`] here instead,
/// bounding how long a terminal-hook failure can block the operator's shell
/// without weakening the fail-closed terminal-Ship invariant: an unanswered
/// gate still fails the operation entirely (via `run_gate_with_timeout`'s
/// existing timeout error), just after seconds instead of days.
pub(crate) fn finish_workflow_with_gate_timeout(
    project_root: &Path,
    state: &mut State,
    gate_timeout_secs: u64,
) -> Result<(), CliError> {
    loop {
        if run_checkout_hooks(project_root, state, &hooks::hooks_after_ship(), Stage::Ship) {
            break;
        }
        // The original Ship approval has already been consumed. Reopen an
        // actionable gate and keep this monitor waiting so a terminal-hook
        // failure cannot turn into an invisible stalled Ship state.
        let _ = Gates::cleanup(project_root, state.phase, Stage::Ship);
        let context = format!(
            "[finalization failed] phase {} terminal hooks did not complete. Resolve the git/version error, then approve to retry; reject to loop back or abort.",
            state.phase
        );
        match run_gate_with_timeout(
            project_root,
            state,
            Stage::Ship,
            &context,
            gate_timeout_secs,
        )? {
            GateAction::Advance => {
                let _ = Gates::cleanup(project_root, state.phase, Stage::Ship);
            }
            GateAction::LoopBack(_) => {
                return loop_back_to_code(project_root, state, FixType::AuditFix);
            }
            GateAction::Abort(reason) => return abort(project_root, state, &reason),
        }
    }
    let _ = Gates::cleanup(project_root, state.phase, Stage::Validate);
    let _ = Gates::cleanup(project_root, state.phase, Stage::Ship);
    workflow::clear_state(project_root, state.phase)?;
    events::emit(
        project_root,
        state.phase,
        "workflow_finished",
        serde_json::Value::Null,
    );
    println!("phase {} shipped — workflow complete", state.phase);
    Ok(())
}

/// Write a gate file and block (in the detached monitor) until a response or
/// the long poll timeout. Acks the response so the Hermes poller can clean up.
pub(crate) fn run_gate(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    context: &str,
) -> Result<GateAction, CliError> {
    run_gate_with_timeout(project_root, state, stage, context, gate_timeout_secs())
}

/// `run_gate`'s body, parameterized on the poll timeout (WR-02, phase 20
/// review) so [`finish_workflow_with_gate_timeout`]'s foreground retry-gate
/// wait can pass a bounded timeout instead of [`gate_timeout_secs`]'s
/// multi-day production default.
pub(crate) fn run_gate_with_timeout(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    context: &str,
    timeout_secs: u64,
) -> Result<GateAction, CliError> {
    state.gate_pending = true;
    workflow::save_state(state)?;
    Gates::write_gate(project_root, state.phase, stage, context)?;
    println!(
        "gate written: .devflow/gates/{:02}-{stage}.json — awaiting response",
        state.phase
    );
    // A gate is "unexpected" when the active mode would not normally fire
    // one for this stage (e.g. a Define/Plan/Code failure in Auto mode) —
    // WR-11's never-silent path gates unconditionally, independent of mode.
    let unexpected = !state.mode.should_gate(stage, state.consecutive_failures);
    if unexpected {
        info!(
            "never-silent gate: {stage} failed in {:?} mode — surfacing an unattended gate this mode would not normally fire",
            state.mode
        );
    }
    events::emit(
        project_root,
        state.phase,
        "gate_fired",
        serde_json::json!({
            "stage": stage.to_string(),
            "unexpected": unexpected,
            "context": context,
        }),
    );
    gates::fire_gate_notify(state.phase, stage, context, unexpected);
    events::emit(
        project_root,
        state.phase,
        "notify_fired",
        serde_json::json!({ "stage": stage.to_string(), "unexpected": unexpected }),
    );
    match Gates::poll_response(project_root, state.phase, stage, timeout_secs) {
        Some(response) => {
            state.gate_pending = false;
            workflow::save_state(state)?;
            Gates::ack(project_root, state.phase, stage)?;
            let action = GateAction::from_response(&response);
            events::emit(
                project_root,
                state.phase,
                "gate_resolved",
                serde_json::json!({
                    "stage": stage.to_string(),
                    "approved": response.approved,
                    "action": match &action {
                        GateAction::Advance => "advance",
                        GateAction::LoopBack(_) => "loop_back",
                        GateAction::Abort(_) => "abort",
                    },
                    "responded_by": response.responded_by,
                }),
            );
            Ok(action)
        }
        None => {
            events::emit(
                project_root,
                state.phase,
                "gate_timeout",
                serde_json::json!({ "stage": stage.to_string() }),
            );
            Err(CliError::Message(format!(
                "gate for stage {stage} timed out awaiting a response"
            )))
        }
    }
}

/// Abort the workflow with a reason, clearing state.
pub(crate) fn abort(project_root: &Path, state: &State, reason: &str) -> Result<(), CliError> {
    println!("workflow aborted for phase {}: {reason}", state.phase);
    // See CR-01: without this, a stale response/ack for this phase+stage
    // survives on disk and is silently reused if the gate fires again later.
    let _ = Gates::cleanup(project_root, state.phase, state.stage);
    let _ = workflow::clear_state(project_root, state.phase);
    events::emit(
        project_root,
        state.phase,
        "workflow_aborted",
        serde_json::json!({ "reason": truncate_reason(reason) }),
    );
    Ok(())
}

/// Manual ship override (20e, D-01): a second, out-of-process consumer of
/// the SAME on-disk Ship gate response `run_gate`'s live blocking poll
/// consumes. `devflow gate approve` only WRITES a response file
/// (`Gates::respond`) — a live monitor polling `Gates::poll_response` is
/// what actually advances the workflow. If that monitor died before
/// consuming the response, the approval sits unconsumed forever; this
/// function reads the already-written response directly and, on
/// `GateAction::Advance`, drives the SAME `finish_workflow` the live poll
/// loop would have called (D-01) — not a reimplementation of the after-ship
/// hook batch.
///
/// Guard order (D-02, review: Codex HIGH + MEDIUM, Hermes ack-race):
/// 1. Acquire the per-phase lock ([`lock::acquire`]) BEFORE touching state,
///    so this can never race a still-live monitor's `poll_response` — the
///    exact idiom [`crate::pipeline_launch::resume`] uses.
/// 2. `state.stage` must be EXACTLY `Stage::Ship`; any earlier stage is
///    refused, naming the stage to resolve first.
/// 3. Both the Ship gate REQUEST (`Gates::gate_path`) and RESPONSE
///    (`Gates::response_path`) must exist on disk.
/// 4. The ACK (`Gates::ack_path`) must be ABSENT — its presence means a
///    (now-dead) monitor already consumed this response and may have died
///    mid-`finish_workflow`; re-running the terminal hooks in that state
///    would risk a double-run, so this refuses and directs the operator to
///    `devflow doctor` instead.
///
/// `force` is accepted and echoed in the CLI output for explicit operator
/// auditability (Hermes LOW: make `--force` semantics explicit), but is
/// deliberately NOT consulted by any guard above — D-02 scopes `--force` to
/// "skip Ship-gate re-verification," never to bypassing the stage, lock,
/// gate-existence, or ack checks themselves. This design has no separate
/// Ship-gate re-verification step beyond those four guards, so `--force`
/// currently changes no observable behavior; it exists so the flag is never
/// silently ignored and cannot later be wired to widen scope without an
/// explicit, reviewed change to this function.
pub(crate) fn ship_override(project_root: &Path, phase: u32, force: bool) -> Result<(), CliError> {
    let _lock = match lock::acquire(project_root, phase) {
        Ok(guard) => guard,
        Err(lock::LockError::Contended { pid, .. }) => {
            return Err(CliError::Message(format!(
                "phase {phase}: another devflow process (pid {pid}) holds the per-phase lock — \
                 refusing to race its poll of the Ship gate response"
            )));
        }
        Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
    };

    let mut state = workflow::load_state(project_root, phase)?;

    if state.stage != Stage::Ship {
        return Err(CliError::Message(format!(
            "phase {phase} is at stage {} — `devflow ship` requires state.stage == Stage::Ship; \
             resolve stage {} first (--force does not skip stages)",
            state.stage, state.stage
        )));
    }

    if !Gates::gate_path(project_root, phase, Stage::Ship).exists()
        || !Gates::response_path(project_root, phase, Stage::Ship).exists()
    {
        return Err(CliError::Message(format!(
            "phase {phase}: no Ship gate response written yet — nothing to ship (a dead monitor \
             never wrote or received one; wait for `devflow gate approve` or resolve the pipeline first)"
        )));
    }

    if Gates::ack_path(project_root, phase, Stage::Ship).exists() {
        return Err(CliError::Message(format!(
            "phase {phase}: the Ship gate response was already consumed (an ack file is present) \
             — the phase may be mid-finalization from a monitor that died partway through; run \
             `devflow doctor` to inspect it rather than re-running terminal hooks"
        )));
    }

    let response_path = Gates::response_path(project_root, phase, Stage::Ship);
    let contents = std::fs::read_to_string(&response_path).map_err(|err| {
        CliError::Message(format!("could not read the Ship gate response: {err}"))
    })?;
    let response: GateResponse = serde_json::from_str(&contents).map_err(|err| {
        CliError::Message(format!("could not parse the Ship gate response: {err}"))
    })?;

    println!(
        "phase {phase}: manual ship override (--force={force}) — driving the already-written \
         Ship response through the same terminal path the live monitor would have used"
    );

    match GateAction::from_response(&response) {
        GateAction::Advance => {
            // WR-02 (phase 20 review): finish_workflow's retry-gate wait
            // (on a terminal-hook failure) normally uses gate_timeout_secs'
            // multi-day production default, invisible to an operator
            // because every OTHER caller runs inside a detached monitor.
            // ship_override runs in the FOREGROUND CLI — bound the wait so
            // a hook failure fails fast with an actionable message instead
            // of blocking this shell for days.
            let timeout = foreground_gate_timeout_secs();
            println!(
                "phase {phase}: if terminal-hook finalization fails, this foreground command \
                 will wait up to {timeout}s for the reopened Ship gate before failing (vs. the \
                 multi-day background default) — set DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS to \
                 change this"
            );
            finish_workflow_with_gate_timeout(project_root, &mut state, timeout)
        }
        GateAction::LoopBack(_) => {
            // Antigravity LOW: `loop_back_to_code` → `launch_stage` forks a
            // NEW detached monitor daemon — say so explicitly, so `devflow
            // ship` is not a silently long-running process the operator
            // can't account for.
            println!(
                "phase {phase}: Ship response loops back to Code — launching a new, detached \
                 monitor agent to drive the retry"
            );
            loop_back_to_code(project_root, &mut state, FixType::AuditFix)
        }
        GateAction::Abort(reason) => abort(project_root, &state, &reason),
    }
}

/// Print the full pipeline that a `start` would run, without launching anything.
pub(crate) fn print_dry_run(state: &State) {
    println!(
        "dry run — phase {} | agent {} | mode {}",
        state.phase, state.agent, state.mode
    );
    println!("\nstage pipeline:");
    let mut stage = Some(Stage::Define);
    while let Some(s) = stage {
        let command = s.gsd_command().replace("{N}", &state.phase.to_string());
        let gate = if state.mode.should_gate(s, 0) {
            " [GATE]".to_string()
        } else if state.mode.should_gate(s, mode::MAX_CONSECUTIVE_FAILURES) {
            format!(" [GATE after {} failures]", mode::MAX_CONSECUTIVE_FAILURES)
        } else {
            String::new()
        };
        // WR-01 (phase 20 review): annotate the stage matching `--until` so
        // the dry-run preview reflects that a real invocation would halt
        // here instead of always showing the full Define→Ship pipeline.
        let stop_marker = if state.stop_until == Some(s) {
            " [STOPS HERE — --until]"
        } else {
            ""
        };
        println!("  {s:<9} {command}{gate}{stop_marker}");
        if let Some(next) = s.next() {
            let transition_hooks = hooks::hooks_for_transition(s, next);
            if !transition_hooks.is_empty() {
                println!("            ↳ hooks: {transition_hooks:?}");
            }
        }
        stage = s.next();
    }
    if let Some(until) = state.stop_until {
        println!("\nnote: --until {until} — this run will halt after {until} completes");
    }
    println!("\nafter ship: {:?}", hooks::hooks_after_ship());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_launch::advance;
    use crate::pipeline_outcomes::{
        ValidateOutcome, handle_infra_outcome, handle_validate_outcome,
    };
    use crate::test_support::*;
    use devflow_core::agent_result;
    use devflow_core::gates::GateResponse;
    use devflow_core::mode::Mode;
    use devflow_core::state::AgentKind;

    /// `advance()` over a Ship-stage success with an approved Ship gate must run
    /// the terminal `finish_workflow` path (after-ship hooks + gate cleanup +
    /// state cleared) — the only non-spawning branch of `advance`'s orchestration
    /// (11-VALIDATION.md 12f). The gate response is pre-seeded on disk so
    /// `run_gate`'s poll returns immediately instead of blocking.
    #[test]
    fn advance_ship_success_runs_finish_workflow() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 21;
        let branch = format!("feature/phase-{phase:02}");
        let branch_created = std::process::Command::new("git")
            .args(["branch", &branch, "develop"])
            .current_dir(root)
            .status()
            .unwrap()
            .success();
        assert!(branch_created);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        // Seed a DEVFLOW_RESULT success marker so `evaluate_agent_result` resolves
        // at Layer 1 without needing the exit-code/commit-count fallback.
        std::fs::write(
            agent_result::stdout_path(root, phase),
            "DEVFLOW_RESULT: {\"status\":\"success\"}\n",
        )
        .unwrap();

        // Pre-write an approved Ship gate response so `run_gate` returns
        // `GateAction::Advance` immediately instead of polling.
        let response_path = Gates::response_path(root, phase, Stage::Ship);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":true,"note":null,"responded_by":"test"}"#,
        )
        .unwrap();

        advance(root, Some(phase)).unwrap();

        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
        assert!(!Gates::gate_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::response_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::ack_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::gate_path(root, phase, Stage::Validate).exists());
    }

    #[test]
    fn terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        };
        git(&["checkout", "-q", "-b", "feature/phase-22"]);
        std::fs::write(root.join("conflict.txt"), "feature\n").unwrap();
        git(&["add", "conflict.txt"]);
        git(&["commit", "-q", "-m", "feature change"]);
        git(&["checkout", "-q", "develop"]);
        std::fs::write(root.join("conflict.txt"), "develop\n").unwrap();
        git(&["add", "conflict.txt"]);
        git(&["commit", "-q", "-m", "develop change"]);

        let mut state = State::new(22, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        let root_owned = root.to_path_buf();
        let handle = std::thread::spawn(move || {
            let mut state = workflow::load_state(&root_owned, 22).unwrap();
            finish_workflow(&root_owned, &mut state)
        });
        let gate_path = Gates::gate_path(root, 22, Stage::Ship);
        for _ in 0..100 {
            if gate_path.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(
            gate_path.exists(),
            "finalization failure must reopen Ship gate"
        );
        assert!(workflow::load_state(root, 22).unwrap().gate_pending);
        Gates::respond(
            root,
            22,
            Stage::Ship,
            &GateResponse {
                approved: false,
                note: Some("abort after merge conflict".into()),
                responded_by: Some("test".into()),
            },
        )
        .unwrap();
        handle.join().unwrap().unwrap();

        assert_ne!(
            events::last_event_for_phase(root, 22)
                .and_then(|event| event["event"].as_str().map(str::to_owned))
                .as_deref(),
            Some("workflow_finished")
        );
        let tags = std::process::Command::new("git")
            .arg("tag")
            .current_dir(root)
            .output()
            .unwrap();
        assert!(tags.stdout.is_empty());
    }

    /// 13-DEFERRED-CR-03 acceptance: two phases advancing their Ship stages
    /// CONCURRENTLY must each finish their own stage machine — per-phase
    /// state files prevent cross-phase clobbering, and the coarse checkout
    /// lock serializes both `finish_workflow`s' git operations on the shared
    /// primary checkout. Gate responses are pre-seeded so neither advance
    /// blocks polling on its *first* Ship gate.
    ///
    /// 17-09 gap closure (GAP-2): both phases compute their next version from
    /// the same starting git state, and on some runs genuinely race to
    /// create the same version tag — confirmed directly during this plan's
    /// RED phase via temporary debug instrumentation, which caught both
    /// threads inside `version_bump` with the identical computed version
    /// (`2.0.1`) within ~1.8ms of each other, and the loser's `git tag`
    /// failing with git's own "reference already exists". That failure
    /// reopens the loser's Ship gate for human review (`finish_workflow`'s
    /// retry loop) — but only ONE gate response was ever pre-written per
    /// phase (consumed by its first gate open), so the reopened gate has
    /// nothing to consume. Unbounded, `Gates::poll_response` then polls the
    /// 7-day production default (`DEVFLOW_GATE_TIMEOUT_SECS`) with no
    /// response ever arriving — that is the wedge this plan closes.
    ///
    /// The binding constraint is "never hangs," not "always both succeed."
    /// This test does not try to make the race loser also succeed (that
    /// would require re-answering a gate reactively and still not rule out
    /// a second, equally rare collision) — instead it bounds the reopened
    /// gate's poll to a few seconds via `DEVFLOW_GATE_TIMEOUT_SECS`
    /// (overridden ONLY for this test's poll, under the established
    /// `ENV_MUTEX` guard — the 7-day production default is never touched)
    /// and asserts the loser's documented behavior: a bounded timeout error,
    /// state left intact (not cleared), and an actionable Ship gate still on
    /// disk awaiting a human. The common case (no collision) still asserts
    /// both phases finish independently, exactly as before.
    #[test]
    fn concurrent_ship_advances_finish_both_phases_independently() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let original_gate_timeout = std::env::var_os("DEVFLOW_GATE_TIMEOUT_SECS");
        // SAFETY: serialized under ENV_MUTEX. Bounds a reopened Ship gate's
        // poll to a few seconds instead of the 7-day production default.
        // Every OTHER test that reaches `run_gate` pre-writes its response
        // before calling in, so `poll_response` finds it on the very first
        // read regardless of this value — only a *reopened*, unanswered
        // gate (this test's race-loser path) ever actually waits it out.
        unsafe {
            std::env::set_var("DEVFLOW_GATE_TIMEOUT_SECS", "2");
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phases = [31u32, 32u32];
        for &phase in &phases {
            let branch = format!("feature/phase-{phase:02}");
            let branch_created = std::process::Command::new("git")
                .args(["branch", &branch, "develop"])
                .current_dir(root)
                .status()
                .unwrap()
                .success();
            assert!(branch_created);
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Ship;
            workflow::save_state(&state).unwrap();
            std::fs::write(
                agent_result::stdout_path(root, phase),
                "DEVFLOW_RESULT: {\"status\":\"success\"}\n",
            )
            .unwrap();
            let response_path = Gates::response_path(root, phase, Stage::Ship);
            std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
            std::fs::write(
                &response_path,
                r#"{"approved":true,"note":null,"responded_by":"test"}"#,
            )
            .unwrap();
        }

        let results: Vec<(u32, Result<(), CliError>)> = std::thread::scope(|scope| {
            let handles: Vec<_> = phases
                .iter()
                .map(|&phase| (phase, scope.spawn(move || advance(root, Some(phase)))))
                .collect();
            handles
                .into_iter()
                .map(|(phase, handle)| (phase, handle.join().expect("advance thread")))
                .collect()
        });

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_gate_timeout {
                Some(value) => std::env::set_var("DEVFLOW_GATE_TIMEOUT_SECS", value),
                None => std::env::remove_var("DEVFLOW_GATE_TIMEOUT_SECS"),
            }
        }

        let succeeded = results.iter().filter(|(_, r)| r.is_ok()).count();
        assert!(
            succeeded == 1 || succeeded == 2,
            "at least one phase must finish independently of the other; got {succeeded}/2 successes"
        );

        for (phase, result) in &results {
            match result {
                Ok(()) => {
                    assert!(
                        matches!(
                            workflow::load_state(root, *phase),
                            Err(workflow::WorkflowError::MissingState(_))
                        ),
                        "phase {phase} must be finished (state cleared)"
                    );
                    assert!(!Gates::gate_path(root, *phase, Stage::Ship).exists());
                    let last = devflow_core::events::last_event_for_phase(root, *phase)
                        .expect("events recorded for phase");
                    assert_eq!(
                        last["event"], "workflow_finished",
                        "phase {phase}'s own event stream must end in workflow_finished"
                    );
                }
                Err(err) => {
                    // The documented loser behavior (GAP-2): a version-tag
                    // race lost by VersionBump reopens the Ship gate for a
                    // human; with no second response pre-written, the
                    // bounded poll above times out rather than hanging.
                    assert!(
                        err.to_string().contains("timed out"),
                        "phase {phase}'s only non-success outcome must be a bounded gate \
                         timeout, not some other failure: {err}"
                    );
                    let state = workflow::load_state(root, *phase)
                        .expect("a timed-out gate leaves state intact, not cleared");
                    assert!(
                        state.gate_pending,
                        "phase {phase} must leave an actionable, still-open gate for a human"
                    );
                    assert!(
                        Gates::gate_path(root, *phase, Stage::Ship).exists(),
                        "phase {phase}'s reopened Ship gate file must remain on disk"
                    );
                }
            }
        }
    }

    /// Regression test for CR-01: `abort()` must clean up the gate's
    /// response/ack files for the stage the gate actually fired on. Without
    /// that cleanup, a later gate for the same phase+stage would find the
    /// old, already-consumed response still on disk and `poll_response`
    /// would resolve from it instantly instead of waiting for a fresh human
    /// decision.
    #[test]
    fn abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = 23;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = mode::MAX_CONSECUTIVE_FAILURES - 1;
        workflow::save_state(&state).unwrap();

        // Pre-write a rejected response whose note says "abort" so
        // `GateAction::from_response` resolves to `Abort`.
        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: requirements changed","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, ValidateOutcome::Failed).unwrap();

        // The gate, response, and ack files for the stage the gate fired on
        // (Validate) must all be gone after the Abort path runs.
        assert!(!Gates::gate_path(root, phase, Stage::Validate).exists());
        assert!(
            !Gates::response_path(root, phase, Stage::Validate).exists(),
            "stale response file must not survive an aborted gate"
        );
        assert!(!Gates::ack_path(root, phase, Stage::Validate).exists());

        // Simulate the phase reaching the same gate again later (e.g. after
        // a restart) — write a fresh request but no new response. If cleanup
        // had not happened, `poll_response` would instantly return the old,
        // already-consumed response instead of blocking for a fresh human
        // decision.
        Gates::write_gate(root, phase, Stage::Validate, "re-fired gate").unwrap();
        let started = std::time::Instant::now();
        let got = Gates::poll_response(root, phase, Stage::Validate, 1);
        assert!(
            got.is_none(),
            "poll_response must not instantly resolve from a stale response after cleanup"
        );
        assert!(started.elapsed() >= std::time::Duration::from_secs(1));
    }

    /// CR-01 regression (17-06 gap closure): `transition()` resets
    /// `infra_failures` to 0 alongside `consecutive_failures` — both in the
    /// in-memory `State` and the persisted `state.json` — and a subsequent
    /// infra fault after a clean transition starts counting from 1, not the
    /// pre-transition count. PATH is neutralized under `ENV_MUTEX` (pointed
    /// at a directory containing ONLY a `git` symlink, so
    /// `agent_binary_available`'s PATH scan has zero possible matches) before
    /// calling `transition()`, because this host genuinely has
    /// `claude`/`codex`/`opencode` on PATH — without neutralizing it,
    /// `transition()`'s downstream `launch_stage` would try to actually spawn
    /// a real agent CLI subprocess, which this test must never do. The
    /// resulting `Err` from `ensure_agent_binary` is expected and ignored:
    /// the counter reset happens earlier in `transition()` and is unaffected
    /// by that downstream failure.
    ///
    /// 19i: PATH must NOT be pointed at an empty directory. `set_var`
    /// mutates the whole process's environment, and Rust's default test
    /// runner executes tests in parallel threads within that one process —
    /// an empty PATH here previously made every OTHER concurrently running,
    /// unguarded git-spawning test fail with `Os { NotFound }` (confirmed
    /// live: both duplicate CI runs for the same commit hit this race).
    /// `agent_free_git_only_path_dir` keeps `git` resolvable for every other
    /// thread while still hiding agent CLIs from this one.
    #[test]
    fn transition_resets_infra_failures() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 80;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.infra_failures = mode::MAX_INFRA_FAILURES - 1;
        workflow::save_state(&state).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = transition(root, &mut state, Stage::Validate);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(
            state.infra_failures, 0,
            "transition() must reset infra_failures in-memory, not just consecutive_failures"
        );
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.infra_failures, 0,
            "transition() must persist the infra_failures reset to state.json"
        );

        // A fresh infra fault after the clean transition starts counting
        // from 1, not resuming the pre-transition MAX_INFRA_FAILURES - 1
        // count toward a false premature abort.
        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_infra_outcome(root, &mut state, Stage::Validate, Some("killed".into())).unwrap();

        assert_eq!(state.infra_failures, 1);
    }

    /// 18d idempotency edge: a repeated Code→Validate transition leaves
    /// `consecutive_failures` unchanged rather than zeroing it. `state.stage`
    /// is reset to `Code` before each call so both calls exercise the exact
    /// hop under test.
    #[test]
    fn repeated_code_to_validate_transition_is_idempotent_on_the_counter() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 83;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.consecutive_failures = 2;
        workflow::save_state(&state).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = transition(root, &mut state, Stage::Validate);
        state.stage = Stage::Code;
        let _ = transition(root, &mut state, Stage::Validate);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(state.consecutive_failures, 2);
    }

    /// 20e Task 1: the tracer end-to-end case — a phase parked at
    /// `Stage::Ship` with a REQUEST + RESPONSE (approved) written via
    /// `Gates::respond`, no ack, and no live process polling. `ship_override`
    /// must reach the SAME terminal state `finish_workflow` produces: state
    /// cleared, gate/response/ack files gone, `workflow_finished` emitted.
    #[test]
    fn ship_override_advances_via_written_response() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 90;
        let branch = format!("feature/phase-{phase:02}");
        let branch_created = std::process::Command::new("git")
            .args(["branch", &branch, "develop"])
            .current_dir(root)
            .status()
            .unwrap()
            .success();
        assert!(branch_created);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        Gates::write_gate(root, phase, Stage::Ship, "Ship complete — approve merge?").unwrap();
        Gates::respond(
            root,
            phase,
            Stage::Ship,
            &GateResponse {
                approved: true,
                note: None,
                responded_by: Some("test".into()),
            },
        )
        .unwrap();

        ship_override(root, phase, false).unwrap();

        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
        assert!(!Gates::gate_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::response_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::ack_path(root, phase, Stage::Ship).exists());
        let last = devflow_core::events::last_event_for_phase(root, phase)
            .expect("events recorded for phase");
        assert_eq!(last["event"], "workflow_finished");
    }

    /// WR-02 (phase 20 review): `ship_override` runs in the FOREGROUND CLI,
    /// unlike every other `finish_workflow` caller (which runs inside a
    /// detached monitor). A terminal-hook failure (merge conflict) reopens
    /// the Ship gate; with no response ever written for the REOPENED gate,
    /// this must fail fast within `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS`
    /// (bounded here to a couple seconds) rather than block the caller's
    /// shell for `DEVFLOW_GATE_TIMEOUT_SECS`' multi-day production default —
    /// which is left untouched, proving the two timeouts are genuinely
    /// independent knobs.
    #[test]
    fn ship_override_bounds_foreground_wait_on_terminal_hook_failure() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let original_foreground_timeout = std::env::var_os("DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS");
        // SAFETY: serialized under ENV_MUTEX. Bounds ONLY the foreground
        // knob — DEVFLOW_GATE_TIMEOUT_SECS (the background default) is
        // never touched by this test, so a regression that made
        // `ship_override` fall back to the multi-day default would hang
        // this test instead of silently passing.
        unsafe {
            std::env::set_var("DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS", "2");
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        };
        let phase = 96;
        let branch = format!("feature/phase-{phase:02}");
        git(&["checkout", "-q", "-b", &branch]);
        std::fs::write(root.join("conflict.txt"), "feature\n").unwrap();
        git(&["add", "conflict.txt"]);
        git(&["commit", "-q", "-m", "feature change"]);
        git(&["checkout", "-q", "develop"]);
        std::fs::write(root.join("conflict.txt"), "develop\n").unwrap();
        git(&["add", "conflict.txt"]);
        git(&["commit", "-q", "-m", "develop change"]);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        Gates::write_gate(root, phase, Stage::Ship, "Ship complete — approve merge?").unwrap();
        Gates::respond(
            root,
            phase,
            Stage::Ship,
            &GateResponse {
                approved: true,
                note: None,
                responded_by: Some("test".into()),
            },
        )
        .unwrap();

        let started = std::time::Instant::now();
        let result = ship_override(root, phase, false);
        let elapsed = started.elapsed();

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_foreground_timeout {
                Some(value) => {
                    std::env::set_var("DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS", value);
                }
                None => std::env::remove_var("DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS"),
            }
        }

        assert!(
            result.is_err(),
            "an unresolved reopened Ship gate must fail closed, not silently advance"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(30),
            "the foreground wait must be bounded by DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS, \
             not gate_timeout_secs' multi-day default — took {elapsed:?}"
        );
        assert!(
            Gates::gate_path(root, phase, Stage::Ship).exists(),
            "the merge failure must reopen an actionable Ship gate for a human, not silently \
             drop the phase"
        );
        assert!(
            workflow::load_state(root, phase).is_ok(),
            "state must NOT be cleared — finish_workflow_with_gate_timeout must fail before \
             reaching workflow::clear_state"
        );
    }

    /// 20e Task 3: an `Abort`-routing Ship response (a rejection whose note
    /// says "abort") must reach the SAME shared `abort` helper the live
    /// `handle_ship_outcome` path uses — no special-cased branch inside
    /// `ship_override`. Asserts the shared path's own effects: state
    /// cleared, gate files cleaned up, `workflow_aborted` emitted.
    #[test]
    fn ship_override_abort_routes_through_abort() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 95;

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        Gates::write_gate(root, phase, Stage::Ship, "Ship complete — approve merge?").unwrap();
        Gates::respond(
            root,
            phase,
            Stage::Ship,
            &GateResponse {
                approved: false,
                note: Some("abort: found a blocking regression during manual review".into()),
                responded_by: Some("test".into()),
            },
        )
        .unwrap();

        ship_override(root, phase, false).unwrap();

        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
        assert!(!Gates::gate_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::response_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::ack_path(root, phase, Stage::Ship).exists());
        let last = devflow_core::events::last_event_for_phase(root, phase)
            .expect("events recorded for phase");
        assert_eq!(last["event"], "workflow_aborted");
    }

    /// 20e Task 2 (D-02, EoP regression): `--force` must never let
    /// `ship_override` reach `finish_workflow` from a non-Ship stage. Checked
    /// for every earlier `Stage` with `--force` both true and false.
    #[test]
    fn ship_override_refuses_when_not_at_ship_stage() {
        for stage in [Stage::Define, Stage::Plan, Stage::Code, Stage::Validate] {
            for force in [true, false] {
                let dir = tempfile::tempdir().unwrap();
                let root = dir.path();
                let phase = 91;

                let mut state =
                    State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
                state.stage = stage;
                workflow::save_state(&state).unwrap();

                let err = ship_override(root, phase, force).unwrap_err();
                let msg = err.to_string();
                assert!(
                    msg.contains(&stage.to_string()),
                    "error for stage {stage} (force={force}) must name the stage: {msg}"
                );

                // finish_workflow was never reached: state is untouched, not cleared.
                let reloaded = workflow::load_state(root, phase)
                    .expect("state must survive a stage-mismatch refusal, not be cleared");
                assert_eq!(reloaded.stage, stage);
            }
        }
    }

    /// 20e Task 2 (edge-probe 20e/empty): `state.stage == Stage::Ship` but no
    /// Ship gate request/response exists on disk — a dead monitor never
    /// wrote or received one. Must fail-closed with a clear error, both with
    /// `--force` true and false.
    #[test]
    fn ship_override_refuses_when_no_response_written() {
        for force in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = 92;

            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Ship;
            workflow::save_state(&state).unwrap();

            // Neither request nor response written yet.
            let err = ship_override(root, phase, force).unwrap_err();
            assert!(
                err.to_string().contains("no Ship gate response"),
                "force={force}: {err}"
            );

            // A request written but no response yet is the same fail-closed case.
            Gates::write_gate(root, phase, Stage::Ship, "ctx").unwrap();
            let err = ship_override(root, phase, force).unwrap_err();
            assert!(
                err.to_string().contains("no Ship gate response"),
                "force={force}: {err}"
            );
            let _ = Gates::cleanup(root, phase, Stage::Ship);
        }
    }

    /// 20e Task 2 (review: Codex HIGH — lock race): a contended per-phase
    /// lock refuses fail-closed, naming the holder pid, with `--force` both
    /// true and false — `ship_override` must never race a live holder's
    /// `poll_response`.
    #[test]
    fn ship_override_refuses_when_lock_contended() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 93;

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        let _held = lock::acquire(root, phase).expect("hold the per-phase lock");
        let this_pid = std::process::id().to_string();

        for force in [true, false] {
            let err = ship_override(root, phase, force).unwrap_err();
            let msg = err.to_string();
            assert!(msg.contains(&this_pid), "force={force}: {msg}");
        }
    }

    /// 20e Task 2 (review: Hermes ack-race): a Ship response that already
    /// has an ack file alongside it was already consumed by a (now-dead)
    /// monitor — `ship_override` refuses and directs to `devflow doctor`
    /// rather than re-running terminal hooks, with `--force` both true and
    /// false. `finish_workflow` must never be reached: state stays intact.
    #[test]
    fn ship_override_refuses_when_response_already_acked() {
        for force in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = 94;

            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Ship;
            workflow::save_state(&state).unwrap();

            Gates::write_gate(root, phase, Stage::Ship, "ctx").unwrap();
            Gates::respond(
                root,
                phase,
                Stage::Ship,
                &GateResponse {
                    approved: true,
                    note: None,
                    responded_by: Some("test".into()),
                },
            )
            .unwrap();
            Gates::ack(root, phase, Stage::Ship).unwrap();

            let err = ship_override(root, phase, force).unwrap_err();
            assert!(
                err.to_string().contains("devflow doctor"),
                "force={force}: {err}"
            );

            let reloaded = workflow::load_state(root, phase)
                .expect("an already-acked refusal must never clear state");
            assert_eq!(reloaded.stage, Stage::Ship);
        }
    }

    /// 18d concurrency edge: two concurrently-active phases' `consecutive_failures`
    /// counters are independent — a Code→Validate hop on one phase must not
    /// reset a sibling phase's counter.
    #[test]
    fn consecutive_failures_are_independent_across_phases() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let mut state_a = State::new(84, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state_a.stage = Stage::Code;
        state_a.consecutive_failures = 1;
        workflow::save_state(&state_a).unwrap();

        let mut state_b = State::new(85, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state_b.stage = Stage::Code;
        state_b.consecutive_failures = 2;
        workflow::save_state(&state_b).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = transition(root, &mut state_a, Stage::Validate);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        let reloaded_a = workflow::load_state(root, 84).unwrap();
        let reloaded_b = workflow::load_state(root, 85).unwrap();

        assert_eq!(
            reloaded_a.consecutive_failures, 1,
            "the Code->Validate hop must not reset consecutive_failures"
        );
        assert_eq!(
            reloaded_b.consecutive_failures, 2,
            "an untouched sibling phase's counter must be unaffected"
        );
    }
}
