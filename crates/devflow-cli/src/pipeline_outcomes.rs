//! Pipeline seam B (D-06): deciding what happens after a stage produces a
//! result — the `handle_*_outcome` family, checkout-hook execution, and the
//! gate-context rendering helpers they share. Extracted mechanically
//! (19-08, D-09 pure move) out of `main.rs` — every function below is
//! byte-identical to its pre-move body modulo an added `pub(crate)` and
//! adjusted `use` paths.
//!
//! **This module sits in the middle of the pipeline's three-way module
//! cycle (19-RESEARCH.md Pattern 1):** it is called by
//! [`crate::pipeline_launch::advance`] once an agent result has been
//! classified, and it calls onward into [`crate::pipeline_gate`]'s
//! `transition`, `loop_back_to_code`, `finish_workflow`, and `abort` to
//! actually move the state machine.

use crate::CliError;
use crate::config_parse::checkout_lock_timeout;
use crate::parallel::retry_after_from_reason;
use crate::pipeline_gate::{abort, finish_workflow, loop_back_to_code, run_gate, transition};
use crate::pipeline_launch::launch_stage;
use devflow_core::config::GitFlowConfig;
use devflow_core::gates::{GateAction, Gates};
use devflow_core::hooks::{self, HookContext};
use devflow_core::mode;
use devflow_core::prompt::FixType;
use devflow_core::stage::Stage;
use devflow_core::state::State;
use devflow_core::{
    agent_result,
    agent_result::{AgentStatus, Verdict},
    events, lock, workflow,
};
use std::path::{Path, PathBuf};

/// Route a `GateInfra` outcome (ResourceKilled/AgentUnavailable) — bumps
/// `state.infra_failures` (saturating, never `consecutive_failures`),
/// persists, then either aborts at the ceiling or fires the never-silent
/// gate via [`handle_stage_failure`]. Deliberately never calls
/// `handle_validate_outcome`/`handle_ship_failure` on any stage (review
/// consensus #4) — those increment `consecutive_failures`, which would
/// conflate an infrastructure fault with an agent-caused failure (D-08).
pub(crate) fn handle_infra_outcome(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    state.infra_failures = state.infra_failures.saturating_add(1);
    workflow::save_state(state)?;
    gate_or_abort_infra(project_root, state, stage, reason)
}

/// The ceiling check + gate-or-abort half of the infra path, shared by
/// [`handle_infra_outcome`] and the `AutoResume` arm's infra-ceiling branch
/// (which bumps `infra_failures` itself before calling this, so the counter
/// is never bumped twice for the same outcome).
pub(crate) fn gate_or_abort_infra(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    if state.infra_failures >= mode::MAX_INFRA_FAILURES {
        return abort(
            project_root,
            state,
            &format!(
                "infrastructure failures reached the ceiling ({} of {}) — aborting rather than gating again",
                state.infra_failures,
                mode::MAX_INFRA_FAILURES
            ),
        );
    }
    handle_stage_failure(project_root, state, stage, reason)
}

/// Route a `RateLimited` outcome from the PRIMARY advance() monitor loop
/// (D-09): writes a single-agent cron-instructions resume record (`devflow
/// resume --phase N`) and returns without firing a blocking gate — unlike
/// `sequentagent`'s existing rate-limit handling, this path never called the
/// cron machinery before this plan (Pitfall 3). Shares the same
/// `infra_failures` ceiling as [`handle_infra_outcome`] (D-08's intentional
/// shared infra counter): once bumping would reach the ceiling, auto-resume
/// stops and the outcome instead routes through the infra gate/abort path.
/// Never touches `consecutive_failures`.
pub(crate) fn handle_rate_limited_outcome(
    project_root: &Path,
    state: &mut State,
    phase: u32,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    let retry_after = retry_after_from_reason(reason.as_deref());
    let projected_infra_failures = state.infra_failures.saturating_add(1);
    if projected_infra_failures >= mode::MAX_INFRA_FAILURES {
        return handle_infra_outcome(project_root, state, stage, reason);
    }
    state.infra_failures = projected_infra_failures;
    workflow::save_state(state)?;

    let instructions =
        devflow_core::ship::build_single_agent_cron_instructions(project_root, phase, &retry_after);
    devflow_core::ship::write_cron_instructions(project_root, &instructions)?;
    // CR-03: an unparseable retry hint (e.g. the `"usage limit"` fallback for
    // a 429 with no retry_after) leaves the schedule empty — and it must stay
    // empty, since an empty cron expression would degrade into an
    // every-minute resume. That means auto-resume cannot happen, so returning
    // here would exit the detached monitor with the phase stalled and no
    // operator signal at all (the println below is read by nobody). Route
    // through the same gate/notify path the infra ceiling uses so the phase is
    // never silently stalled (WR-11/D-15). `infra_failures` is already bumped
    // above, so `gate_or_abort_infra` — which never bumps — is the correct
    // entry point.
    if instructions.hermes_cron.schedule.is_empty() {
        return gate_or_abort_infra(
            project_root,
            state,
            stage,
            Some(format!(
                "rate limited with no parseable retry time ({retry_after}) — auto-resume cron not scheduled; resume manually"
            )),
        );
    }
    println!(
        "rate limited — wrote {}",
        devflow_core::ship::cron_instructions_path(project_root, phase)
            .strip_prefix(project_root)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| {
                devflow_core::ship::cron_instructions_path(project_root, phase)
                    .display()
                    .to_string()
            })
    );
    events::emit(
        project_root,
        phase,
        "rate_limit_resume_scheduled",
        serde_json::json!({
            "stage": stage.to_string(),
            "retry_after": retry_after,
            "infra_failures": state.infra_failures,
        }),
    );
    Ok(())
}

/// The three-way outcome of a Validate stage evaluation (18e, D-18e).
///
/// Distinct from a plain `bool`: an `external_verify`-declared Validate has
/// THREE distinguishable outcomes, not two — the probe and the agent's
/// self-reported verdict can independently agree, disagree, or leave one
/// signal missing. Collapsing disagreement or "no verdict at all" onto
/// `Failed` would route them through the counter-based auto-loop, a DELAYED
/// gate indistinguishable from an ordinary retry to the operator watching
/// it — the binding operator decision requires an IMMEDIATE one instead
/// (T-18-19).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ValidateOutcome {
    /// The two independent signals agree (or no `external_verify` is
    /// declared and the agent reported `verdict: pass`): advance to Ship.
    Passed,
    /// An ordinary Validate failure — the pre-existing fail-safe, unchanged:
    /// loop back to Code, or gate once `consecutive_failures` reaches the
    /// ceiling.
    Failed,
    /// The probe passed but the agent's verdict disagrees, or no verdict
    /// arrived at all. Gates for a human IMMEDIATELY, never touching
    /// `consecutive_failures`. The payload names which two signals
    /// disagreed, for the `[never-silent]` gate context.
    Ambiguous(String),
}

/// Classify a Validate-stage `AgentResult` into its three-way outcome
/// (D-18e, the binding operator decision reproduced in 18-05-PLAN.md).
///
/// Pure function over `&AgentResult` — no I/O — so the whole decision
/// matrix is directly unit-testable. `Some(Verdict::Pass)` is matched FIRST
/// and wins regardless of which layer decided the result: it is the "two
/// independent signals agreeing" arm and must not be shadowed by the
/// external-verify-specific arms below it.
pub(crate) fn classify_validate_outcome(result: &agent_result::AgentResult) -> ValidateOutcome {
    let external = result.decided_by_layer == Some(0) && result.status == AgentStatus::Success;
    match (external, result.verdict) {
        (_, Some(Verdict::Pass)) => ValidateOutcome::Passed,
        (true, Some(Verdict::Gaps)) => ValidateOutcome::Ambiguous(
            "external verification passed but the agent reported gaps".to_string(),
        ),
        (true, None) => ValidateOutcome::Ambiguous(
            "external verification passed but no agent verdict arrived".to_string(),
        ),
        _ => ValidateOutcome::Failed,
    }
}

/// The two ordinary Validate outcomes left once `ValidateOutcome::Ambiguous`
/// has been handled and returned on its own (WR-03, 18-fix). Deliberately a
/// distinct, two-variant type: matching on THIS below is exhaustive without
/// a third, panic-capable arm — the compiler enforces that
/// `handle_validate_outcome`'s tail can never see an ambiguous outcome,
/// instead of that invariant being proven by hand-tracing control flow (the
/// pre-fix shape's `unreachable!()`, which was sound but fragile: a future
/// edit to either the `forced` computation or the early-return `if` could
/// have silently reintroduced reachability).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValidateResult {
    Passed,
    Failed,
}

/// Decide what happens after a Validate stage, honoring the active mode's
/// gate policy, the consecutive-failure threshold, and (18e) the immediate
/// gate an ambiguous `external_verify` outcome forces regardless of either.
pub(crate) fn handle_validate_outcome(
    project_root: &Path,
    state: &mut State,
    outcome: ValidateOutcome,
) -> Result<(), CliError> {
    // 18e / T-18-19: an ambiguous outcome must gate IMMEDIATELY — it is
    // being adjudicated right now, not retried, so it must never fall
    // through to the counter-based `should_gate` check below and must never
    // touch `consecutive_failures`. Handled in its own arm, up front, and
    // converted to `ValidateResult` for the two variants that share the
    // rest of this function's logic (WR-03).
    let result = match outcome {
        ValidateOutcome::Ambiguous(detail) => {
            let context = format!(
                "[never-silent] validate ambiguous: {}",
                truncate_reason(&detail)
            );
            return match run_gate(project_root, state, Stage::Validate, &context)? {
                GateAction::Advance => transition(project_root, state, Stage::Ship),
                GateAction::LoopBack(_) => {
                    loop_back_to_code(project_root, state, FixType::GapsOnly)
                }
                GateAction::Abort(reason) => abort(project_root, state, &reason),
            };
        }
        ValidateOutcome::Passed => ValidateResult::Passed,
        ValidateOutcome::Failed => ValidateResult::Failed,
    };

    if result == ValidateResult::Failed {
        // Now that the counter genuinely accumulates (18d), an unbounded
        // loop could otherwise overflow it and wrap to 0, silently
        // restoring the unreachable-ceiling bug in a slower form.
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        workflow::save_state(state)?;
    }

    if state
        .mode
        .should_gate(Stage::Validate, state.consecutive_failures)
    {
        let context = match result {
            ValidateResult::Passed => "Validation passed — approve to ship?".to_string(),
            ValidateResult::Failed => format!(
                "Validation failed {} time(s) — human review needed.",
                state.consecutive_failures
            ),
        };
        return match run_gate(project_root, state, Stage::Validate, &context)? {
            GateAction::Advance => transition(project_root, state, Stage::Ship),
            GateAction::LoopBack(_) => loop_back_to_code(project_root, state, FixType::GapsOnly),
            GateAction::Abort(reason) => abort(project_root, state, &reason),
        };
    }

    match result {
        ValidateResult::Passed => transition(project_root, state, Stage::Ship),
        ValidateResult::Failed => loop_back_to_code(project_root, state, FixType::GapsOnly),
    }
}

/// Decide what happens after the Ship stage completes — always gated.
pub(crate) fn handle_ship_outcome(project_root: &Path, state: &mut State) -> Result<(), CliError> {
    match run_gate(
        project_root,
        state,
        Stage::Ship,
        "Ship complete — approve merge?",
    )? {
        GateAction::Advance => finish_workflow(project_root, state),
        GateAction::LoopBack(_) => loop_back_to_code(project_root, state, FixType::GapsOnly),
        GateAction::Abort(reason) => abort(project_root, state, &reason),
    }
}

/// Handle a non-Validate stage failure (Define/Plan/Code, or a Ship agent
/// crash routed in via [`handle_ship_failure`]). WR-11: this path must never
/// be silent — it unconditionally fires a gate + notify via [`run_gate`]
/// (independent of `Mode::should_gate`; `run_gate` marks it as an unexpected
/// gate and notifies accordingly), then lets the operator retry, loop back,
/// or abort. Deliberately kept separate from `handle_validate_outcome`: it
/// does not touch `consecutive_failures` and never auto-loops.
/// Cap a failure reason before it enters a gate context (and from there the
/// operator's notification). Reasons are agent- or parser-derived and can
/// embed arbitrary output — 13-06 dogfood finding: a multi-KB raw JSONL line
/// reached the desktop notification verbatim. Full detail stays available in
/// `.devflow/phase-NN-stdout`; the gate only needs a readable headline.
pub(crate) fn truncate_reason(reason: &str) -> String {
    render_gate_context(reason, 300)
}

/// Render agent-controlled gate text as one bounded, terminal-safe line.
pub(crate) fn render_gate_context(context: &str, max_chars: usize) -> String {
    const TRUNCATED: &str = "… [truncated; full output in .devflow/]";
    let sanitized: String = context
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    if sanitized.chars().count() <= max_chars {
        return sanitized;
    }

    let suffix_len = TRUNCATED.chars().count().min(max_chars);
    let head_len = max_chars.saturating_sub(suffix_len);
    let head: String = sanitized.chars().take(head_len).collect();
    let suffix: String = TRUNCATED.chars().take(suffix_len).collect();
    format!("{head}{suffix}")
}

pub(crate) fn handle_stage_failure(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    let context = format!(
        "[never-silent] stage {stage} failed: {} — human review needed (retry, loop-to-code, or abort)",
        truncate_reason(&reason.unwrap_or_else(|| "no details available".into()))
    );
    match run_gate(project_root, state, stage, &context)? {
        GateAction::Advance => {
            // CR-01: clean up the stale gate/response/ack before retrying so
            // the retry cannot silently consume the prior response.
            let _ = Gates::cleanup(project_root, state.phase, stage);
            state.gate_pending = false;
            launch_stage(state, None, Some(stage))
        }
        GateAction::LoopBack(_) => {
            // Retry the SAME failed stage — Code is not a valid recovery
            // target before planning exists for a Define/Plan failure
            // (Codex 13-01 MEDIUM). Only Ship's ReviewFailed path (handled
            // separately in `handle_ship_failure`) actually loops to Code.
            let _ = Gates::cleanup(project_root, state.phase, stage);
            launch_stage(state, None, Some(stage))
        }
        GateAction::Abort(reason) => abort(project_root, state, &reason),
    }
}

/// Handle the Ship stage's failure outcome, distinguishing an agent crash
/// (`AgentFailed`) from a review rejection (`ReviewFailed`). A `review:`-
/// prefixed reason (trimmed, case-folded) is the agent-reported convention
/// for "the change was reviewed and rejected" — that loops back to Code with
/// the `/gsd-audit-fix` prompt rather than firing a gate (consensus #7).
/// Anything else is treated as an agent crash and routed through the generic
/// never-silent gate path.
pub(crate) fn handle_ship_failure(
    project_root: &Path,
    state: &mut State,
    reason: Option<String>,
) -> Result<(), CliError> {
    if is_ship_review_failure(&reason) {
        return loop_back_to_code(project_root, state, FixType::AuditFix);
    }
    handle_stage_failure(project_root, state, Stage::Ship, reason)
}

/// Whether a Ship-stage failure `reason` is a review rejection (`review:`
/// prefix, trimmed + case-folded) rather than an agent crash. This string
/// convention is an inherent limitation of the agent-reported DEVFLOW_RESULT
/// contract (T-13-04) — verified live against a real agent in 13-06.
pub(crate) fn is_ship_review_failure(reason: &Option<String>) -> bool {
    reason
        .as_deref()
        .map(|r| r.trim().to_ascii_lowercase().starts_with("review:"))
        .unwrap_or(false)
}

/// Run a batch of hooks against the primary checkout, serialized across
/// phases by the coarse project lock (13-DEFERRED-CR-03 fix shape #3): the
/// hooks commit/tag/delete branches in the shared main checkout, and two
/// phases doing that concurrently race git's `index.lock`/`HEAD`. Held for
/// seconds — never across a gate wait. Hook failures stay fail-soft (warn
/// and continue), as before.
///
/// 14-CR-02: a lock timeout SKIPS the batch instead of running it
/// unserialized — mutating the shared checkout concurrently is the exact
/// race this lock exists to prevent, and the hooks are individually
/// fail-soft for ordinary transitions. The return value lets terminal
/// completion fail closed and preserve state when the batch was skipped or
/// a required hook failed.
/// Which tree a hook batch operates on.
///
/// The Validate→Ship transition batch (`DocsUpdate`) authors material *about
/// the branch being shipped*, so it must write into that phase's worktree —
/// otherwise its output is stranded on the base branch, uncommitted and
/// divorced from the commits it describes (found live: Phase 17's changelog
/// entry landed on `develop` while every one of its commits sat on
/// `feature/phase-17`).
///
/// The terminal batch (`Merge`, `VersionBump`, `ChangelogAppend`,
/// `BranchCleanup`) is the exact opposite: it merges the feature branch INTO
/// the base branch, tags the base branch, and deletes the feature branch.
/// Those are primary-checkout operations and retargeting them at the
/// worktree would be a correctness regression. `ChangelogAppend` moved here
/// in 17-12 (WR-04) — a release record naming a version only becomes true
/// once `VersionBump` has tagged it, so the changelog entry belongs on the
/// base branch alongside the tag, not in the worktree. Do not restore
/// 17-10's worktree targeting to this hook.
///
/// Falls back to `project_root` whenever no worktree is configured, so
/// `--no-worktree` runs are unaffected.
pub(crate) fn hook_context_root(
    project_root: &Path,
    state: &State,
    terminal_batch: bool,
) -> PathBuf {
    if terminal_batch {
        return project_root.to_path_buf();
    }
    state
        .worktree_path
        .as_ref()
        .filter(|path| path.exists())
        .map(|path| path.to_path_buf())
        .unwrap_or_else(|| project_root.to_path_buf())
}

pub(crate) fn run_checkout_hooks(
    project_root: &Path,
    state: &State,
    batch: &[hooks::Hook],
    stage: Stage,
) -> bool {
    if batch.is_empty() {
        return true;
    }
    let _checkout_lock = match lock::acquire_project_blocking(project_root, checkout_lock_timeout())
    {
        Ok(guard) => guard,
        Err(err) => {
            println!(
                "warning: could not acquire the checkout lock ({err}) — \
                 SKIPPING hooks {batch:?} rather than mutating the checkout \
                 unserialized. Re-run them once the holder finishes."
            );
            events::emit(
                project_root,
                state.phase,
                "checkout_lock_timeout",
                serde_json::json!({ "stage": stage.to_string(), "error": err.to_string() }),
            );
            for hook in batch {
                events::emit(
                    project_root,
                    state.phase,
                    "hook_run",
                    serde_json::json!({
                        "hook": format!("{hook:?}"),
                        "ok": false,
                        "skipped": "checkout lock timeout",
                    }),
                );
            }
            return false;
        }
    };
    let git_flow = GitFlowConfig::default();
    let mut all_succeeded = true;
    let terminal_batch = batch == hooks::hooks_after_ship().as_slice();
    let hook_root = hook_context_root(project_root, state, terminal_batch);
    // Hoisted out of the loop (GAP-7): these fields are loop-invariant, and
    // VersionBump needs to hand shipped_version forward to ChangelogAppend
    // within the same batch run, which a fresh per-iteration context would
    // discard.
    let mut ctx = HookContext {
        phase: state.phase,
        project_root: hook_root.clone(),
        stage,
        git_flow: git_flow.clone(),
        shipped_version: None,
    };
    for hook in batch {
        let outcome = hook.run(&mut ctx);
        if let Err(ref err) = outcome {
            println!("warning: hook {hook:?} failed: {err}");
            all_succeeded = false;
        }
        events::emit(
            project_root,
            state.phase,
            "hook_run",
            serde_json::json!({
                "hook": format!("{hook:?}"),
                "ok": outcome.is_ok(),
            }),
        );
        // Terminal finalization is ordered and fail-fast. In particular, a
        // failed version/tag operation must not delete the feature branch and
        // destroy the evidence needed for a safe retry.
        if terminal_batch && outcome.is_err() {
            break;
        }
    }
    all_succeeded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_gate::prepare_loop_back_to_code;
    use crate::pipeline_launch::advance;
    use crate::test_support::*;
    use devflow_core::git::GitFlow;
    use devflow_core::mode::Mode;
    use devflow_core::prompt;
    use devflow_core::state::AgentKind;

    /// 14-CR-02: when the checkout lock cannot be acquired, the hook batch
    /// must be SKIPPED — never run unserialized against the shared checkout
    /// — and the skip must be recorded in events.jsonl. `ChangelogAppend`
    /// would observably create `CHANGELOG.md` if the batch ran; it moved
    /// from the Validate→Ship batch into `hooks_after_ship()` in 17-12
    /// (WR-04), so this test now drives that batch instead — none of its
    /// hooks execute here regardless (the lock check short-circuits before
    /// the first hook runs), so no real merge/version state is needed.
    /// Env-mutating, so serialized under ENV_MUTEX; the "0" timeout only
    /// affects a concurrent test if it is actually contended, which none are
    /// (no other test holds the project lock).
    #[test]
    fn checkout_hooks_skip_instead_of_running_unserialized_on_lock_timeout() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A live holder (this process) keeps the lock contended; the stale-
        // holder reclaim cannot fire.
        let _held = lock::acquire_project(root).expect("hold checkout lock");
        unsafe {
            std::env::set_var("DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS", "0");
        }

        let state = State::new(33, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        run_checkout_hooks(root, &state, &hooks::hooks_after_ship(), Stage::Ship);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            std::env::remove_var("DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS");
        }

        assert!(
            !root.join("CHANGELOG.md").exists(),
            "hooks must not run while the checkout lock is held elsewhere"
        );
        let last = devflow_core::events::last_event_for_phase(root, 33)
            .expect("skip must be recorded in events.jsonl");
        assert_eq!(last["event"], "hook_run");
        assert_eq!(last["ok"], false);
        assert_eq!(last["skipped"], "checkout lock timeout");
    }

    #[test]
    fn terminal_hook_failure_stops_before_branch_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = 34;
        let branch = "feature/phase-34";
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        };
        git(&["branch", branch, "develop"]);
        // Force VersionBump to fail after Merge succeeds.
        std::fs::remove_file(root.join("Cargo.toml")).unwrap();
        std::fs::create_dir(root.join("Cargo.toml")).unwrap();

        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        let succeeded = run_checkout_hooks(root, &state, &hooks::hooks_after_ship(), Stage::Ship);

        assert!(!succeeded);
        assert!(
            GitFlow::new(root).branch_exists(branch),
            "a failed terminal batch must preserve the branch for retry"
        );
    }

    /// GAP-8 (17-VALIDATION.md): GAP-7 fixed `HookContext.shipped_version`
    /// forwarding `hooks_after_ship`'s `VersionBump` tag to `ChangelogAppend`
    /// within the same batch — but only the `devflow-core::hooks` unit tests
    /// exercised it directly by hand-rolling their own context and looping
    /// over `hooks_after_ship()`. `run_checkout_hooks` is the ONLY production
    /// caller of that batch, and it must construct the `HookContext` once,
    /// above the hook loop, for the forwarding to survive into production.
    /// This test drives `run_checkout_hooks` itself (not a hand-rolled loop)
    /// against a repo with no version file, and asserts the changelog
    /// heading names the actual tagged version rather than falling back to
    /// the "unreleased" literal.
    #[test]
    fn run_checkout_hooks_keeps_changelog_in_sync_with_tag_when_no_version_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo_no_version_file(root);

        let phase = 47;
        let branch = format!("feature/phase-{phase:02}");
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        };
        git(&["branch", &branch, "develop"]);
        std::fs::write(root.join(".gitignore"), ".devflow/\n").unwrap();
        git(&["checkout", &branch]);
        std::fs::write(root.join("feature.txt"), "phase work\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "phase work"]);
        git(&["checkout", "develop"]);

        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        let succeeded = run_checkout_hooks(root, &state, &hooks::hooks_after_ship(), Stage::Ship);
        assert!(
            succeeded,
            "after-ship batch must succeed against a clean repo"
        );

        let all_tags = std::process::Command::new("git")
            .arg("tag")
            .current_dir(root)
            .output()
            .unwrap();
        let all_tags = String::from_utf8_lossy(&all_tags.stdout);
        assert_eq!(all_tags.lines().count(), 1, "expected exactly one tag");
        let tag = all_tags.trim().to_string();
        let tag_version = tag
            .strip_prefix('v')
            .expect("tag should be prefixed with v")
            .to_string();

        let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
        let changelog_version = changelog
            .lines()
            .find(|l| l.starts_with("## "))
            .and_then(|l| l.trim_start_matches("## ").split(' ').next())
            .unwrap()
            .to_string();

        assert_ne!(
            changelog_version, "unreleased",
            "changelog heading must name the tagged version, not fall back to the literal"
        );
        assert_eq!(
            changelog_version, tag_version,
            "changelog heading must match the git tag ({tag}) produced by the same \
             run_checkout_hooks call, even with no version file"
        );
    }

    /// Reaching `MAX_CONSECUTIVE_FAILURES` on a failed Validate must force a
    /// gate (even in Auto mode, which otherwise auto-loops), and an `abort`
    /// gate response must end the workflow (state cleared) without spawning a
    /// new stage (11-VALIDATION.md 12f). The gate response is pre-seeded so the
    /// poll inside `run_gate` returns immediately.
    #[test]
    fn validate_failure_threshold_forces_gate_then_aborts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = 22;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = mode::MAX_CONSECUTIVE_FAILURES - 1;
        workflow::save_state(&state).unwrap();

        // Pre-write a rejected response whose note says "abort" so
        // `GateAction::from_response` resolves to `Abort` rather than a
        // loop-back-to-Code.
        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: requirements changed","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, ValidateOutcome::Failed).unwrap();

        assert_eq!(state.consecutive_failures, mode::MAX_CONSECUTIVE_FAILURES);
        // CR-01: the forced gate's request file (along with its response and
        // ack) must be cleaned up once it resolves to Abort — previously
        // only the terminal Ship-success path cleaned up gate files, leaving
        // this one on disk to be silently reused by a later gate.
        assert!(
            !Gates::gate_path(root, phase, Stage::Validate).exists(),
            "forced gate's files must be cleaned up once it resolves to Abort"
        );
        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
    }

    /// Seed a Validate-stage DEVFLOW_RESULT marker (with the given verdict
    /// JSON fragment, or `None` to omit the key entirely) and drive `advance()`
    /// on a scoped thread, busy-polling for the Validate gate file to appear
    /// so its `context` text — the only externally observable signal of the
    /// `passed` value `advance()` computed from the verdict — can be read
    /// before resolving the gate with an Abort response. Forcing a gate for
    /// every case (rather than letting a `passed=true` case fall through to a
    /// bare `transition`) is deliberate: `transition`/`loop_back_to_code` both
    /// call `launch_stage`, which spawns the real configured agent CLI and
    /// must never fire from a unit test (see `ship_review_failed_loops_to_code`).
    fn drive_validate_advance_and_read_gate_context(
        root: &Path,
        phase: u32,
        consecutive_failures: u32,
        verdict_json: Option<&str>,
    ) -> String {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = consecutive_failures;
        workflow::save_state(&state).unwrap();

        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        let marker = match verdict_json {
            Some(verdict) => {
                format!(r#"DEVFLOW_RESULT: {{"status":"success","verdict":"{verdict}"}}"#)
            }
            None => r#"DEVFLOW_RESULT: {"status":"success"}"#.to_string(),
        };
        std::fs::write(agent_result::stdout_path(root, phase), marker).unwrap();

        let gate_path = Gates::gate_path(root, phase, Stage::Validate);
        let response_path = Gates::response_path(root, phase, Stage::Validate);
        let mut context = String::new();

        std::thread::scope(|scope| {
            scope.spawn(|| {
                advance(root, Some(phase)).unwrap();
            });

            let mut seen = false;
            for _ in 0..150 {
                if gate_path.exists() {
                    seen = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            assert!(
                seen,
                "advance() must force a Validate gate, not advance silently"
            );

            context = std::fs::read_to_string(&gate_path).unwrap();

            std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
        });

        context
    }

    /// 13b verdict-vs-ran: a Validate agent that ran successfully but found
    /// gaps (`verdict: "gaps"`) must NOT advance to Ship — `advance()`'s
    /// Validate arm must compute `passed = false` and route through
    /// `handle_validate_outcome`'s failure path (gate/loop), never Ship.
    #[test]
    fn validate_gaps_does_not_advance_to_ship() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let context = drive_validate_advance_and_read_gate_context(
            root,
            60,
            mode::MAX_CONSECUTIVE_FAILURES - 1,
            Some("gaps"),
        );
        assert!(
            context.contains("Validation failed"),
            "a gaps verdict must be treated as a failed validation, not a pass: {context}"
        );
    }

    /// 13b verdict-vs-ran (consensus #1): because the Validate prompt now
    /// REQUIRES a verdict, its absence must be treated as a fail-safe
    /// (gate/loop), NOT a silent pass — this is the composition fix that
    /// closes the marker-less/verdict-less Validate → Ship false-advance.
    #[test]
    fn validate_missing_verdict_does_not_advance() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let context = drive_validate_advance_and_read_gate_context(
            root,
            61,
            mode::MAX_CONSECUTIVE_FAILURES - 1,
            None,
        );
        assert!(
            context.contains("Validation failed"),
            "a missing verdict must be treated as a failed validation, not a pass: {context}"
        );
    }

    /// A Validate result with an explicit `verdict: "pass"` must advance to
    /// Ship — `consecutive_failures` is pre-seeded at the gate threshold
    /// itself (rather than `threshold - 1`) because a `passed=true` result
    /// never increments the counter, so the gate must already be at the
    /// threshold to force it open without falling through to a real
    /// `transition`/`launch_stage` spawn.
    #[test]
    fn validate_pass_advances() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let context = drive_validate_advance_and_read_gate_context(
            root,
            62,
            mode::MAX_CONSECUTIVE_FAILURES,
            Some("pass"),
        );
        assert!(
            context.contains("Validation passed"),
            "an explicit pass verdict must advance to Ship: {context}"
        );
    }

    /// D-18e's "two independent signals agreeing" arm: a probe pass plus an
    /// explicit `verdict: pass` classify as `ValidateOutcome::Passed` and
    /// drive straight through to Ship — no forced gate (Auto mode,
    /// `consecutive_failures == 0`), no counter touched. PATH is
    /// neutralized under `ENV_MUTEX` (matching
    /// `consecutive_failures_reaches_ceiling_across_cycles`) so
    /// `transition`'s own `launch_stage` call cannot spawn a real agent CLI;
    /// its resulting `Err` (agent binary not found) is discarded, since
    /// `transition` mutates `state.stage` to `Ship` before that call and the
    /// mutation survives regardless of the launch outcome.
    #[test]
    fn external_verify_agreement_advances_to_ship() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 90;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let result = agent_result::AgentResult {
            status: AgentStatus::Success,
            exit_code: None,
            reason: None,
            commits: None,
            summary: None,
            verdict: Some(Verdict::Pass),
            decided_by_layer: Some(0),
        };
        let outcome = classify_validate_outcome(&result);
        assert_eq!(outcome, ValidateOutcome::Passed);

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = handle_validate_outcome(root, &mut state, outcome);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(state.stage, Stage::Ship);
        assert_eq!(
            state.consecutive_failures, 0,
            "an agreeing outcome must never touch the failure counter"
        );
    }

    /// D-18e's disagreement arm: the probe passes but the agent reports
    /// `verdict: gaps`. Must classify `Ambiguous` and gate IMMEDIATELY on
    /// the FIRST cycle — never touching `consecutive_failures` — which is
    /// what distinguishes this from `Failed`'s counter-based delayed gate
    /// and is the precise thing the binding operator decision (D-18e,
    /// T-18-19) requires. Resolved via an Abort response so no agent is
    /// ever launched.
    #[test]
    fn external_verify_disagreement_gates_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 91;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let result = agent_result::AgentResult {
            status: AgentStatus::Success,
            exit_code: None,
            reason: None,
            commits: None,
            summary: None,
            verdict: Some(Verdict::Gaps),
            decided_by_layer: Some(0),
        };
        let outcome = classify_validate_outcome(&result);
        assert!(matches!(outcome, ValidateOutcome::Ambiguous(_)));

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, outcome).unwrap();

        assert_eq!(
            state.consecutive_failures, 0,
            "an ambiguous outcome must gate on cycle one without touching the counter"
        );
        assert!(
            !Gates::gate_path(root, phase, Stage::Validate).exists(),
            "the immediate gate must resolve (and clean up) via the same abort path as any other gate"
        );
    }

    /// D-18e's ambiguous arm: the probe passes but NO agent verdict arrived
    /// at all. Same immediate-gate contract as the disagreement case above
    /// — `consecutive_failures` must stay 0.
    #[test]
    fn external_verify_no_verdict_gates_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 92;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let result = agent_result::AgentResult {
            status: AgentStatus::Success,
            exit_code: None,
            reason: None,
            commits: None,
            summary: None,
            verdict: None,
            decided_by_layer: Some(0),
        };
        let outcome = classify_validate_outcome(&result);
        assert!(matches!(outcome, ValidateOutcome::Ambiguous(_)));

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, outcome).unwrap();

        assert_eq!(
            state.consecutive_failures, 0,
            "an ambiguous outcome must gate on cycle one without touching the counter"
        );
    }

    /// D-08/consensus #4: a `ResourceKilled` outcome on a non-Validate stage
    /// bumps `infra_failures` and leaves `consecutive_failures` untouched —
    /// `handle_infra_outcome` (the `GateInfra` arm) never routes through
    /// `handle_validate_outcome`. A rejected/abort response is pre-seeded so
    /// the never-silent gate resolves immediately without a spawn thread.
    #[test]
    fn resource_killed_on_code_bumps_infra_failures_not_consecutive_failures() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 73;
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::exit_code_path(root, phase), "137").unwrap();

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.consecutive_failures = 1;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        advance(root, Some(phase)).unwrap();

        // abort() clears state entirely — assert against the terminal error
        // rather than a field, and confirm no Validate gate ever appeared.
        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
        assert!(!Gates::gate_path(root, phase, Stage::Validate).exists());
    }

    /// D-08/consensus #4 (Validate-stage case): a `ResourceKilled` outcome on
    /// the VALIDATE stage still bumps `infra_failures` and leaves
    /// `consecutive_failures` unchanged — proving `GateInfra`
    /// (`handle_infra_outcome`) bypasses `handle_validate_outcome` even on
    /// the one stage that normally owns `consecutive_failures`. The rejected
    /// gate response resolves the never-silent gate to `Abort` immediately
    /// (no spawn thread needed); `consecutive_failures` is asserted on the
    /// in-memory `state`, which `abort()` never mutates (it only clears the
    /// on-disk state file and gate artifacts).
    #[test]
    fn resource_killed_on_validate_bumps_infra_not_consecutive_failures() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 74;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = 2;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_infra_outcome(
            root,
            &mut state,
            Stage::Validate,
            Some("agent process was killed (exit code 137, likely OOM)".into()),
        )
        .unwrap();

        assert_eq!(state.infra_failures, 1);
        assert_eq!(
            state.consecutive_failures, 2,
            "consecutive_failures must be untouched by the infra path"
        );
    }

    /// D-08: reaching `MAX_INFRA_FAILURES` infra outcomes aborts rather than
    /// gating again.
    #[test]
    fn infra_ceiling_aborts_instead_of_gating() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 75;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.infra_failures = mode::MAX_INFRA_FAILURES - 1;
        workflow::save_state(&state).unwrap();

        handle_infra_outcome(root, &mut state, Stage::Code, Some("killed".into())).unwrap();

        assert_eq!(state.infra_failures, mode::MAX_INFRA_FAILURES);
        assert!(
            !Gates::gate_path(root, phase, Stage::Code).exists(),
            "at the ceiling, the run must abort rather than gate again"
        );
        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
    }

    /// 18d — the RED-then-GREEN core of the Code↔Validate safety-gate
    /// reachability fix. Drives `MAX_CONSECUTIVE_FAILURES` real
    /// fail/Code→Validate cycles via `handle_validate_outcome` (the +1) and
    /// `transition()` (previously an unconditional reset to 0). Before the
    /// fix, `consecutive_failures` oscillates 0/1 and never reaches the
    /// ceiling; after the fix it accumulates and forces the gate.
    ///
    /// `state.stage` is forced back to `Stage::Code` before every
    /// `transition()` call so each loop iteration exercises the exact
    /// `(Code, Validate)` hop under test, independent of which internal
    /// branch `handle_validate_outcome` took on that cycle (ordinary
    /// loop-back vs. the forced gate on the final cycle) — mirrors what
    /// `prepare_loop_back_to_code` does for real on every retry.
    ///
    /// A gate response is re-seeded at the top of every loop iteration (not
    /// just once before the loop) so it survives `prepare_loop_back_to_code`'s
    /// `Gates::cleanup(.., Stage::Validate)` — which fires on every ordinary
    /// loop-back cycle once `state.stage` is `Validate` and would otherwise
    /// delete a response written only once up front before the final,
    /// gate-triggering cycle ever gets to read it. With it re-seeded every
    /// iteration, the forced gate on the final cycle resolves immediately via
    /// `Gates::poll_response` finding an already-written file, instead of
    /// waiting out the (default 7-day) gate timeout. PATH is neutralized
    /// under `ENV_MUTEX` so neither `handle_validate_outcome`'s loop-back nor
    /// `transition()`'s own `launch_stage` call risk spawning a real agent
    /// CLI, following `transition_resets_infra_failures`' established
    /// approach.
    #[test]
    fn consecutive_failures_reaches_ceiling_across_cycles() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 81;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        for _ in 0..mode::MAX_CONSECUTIVE_FAILURES {
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            state.stage = Stage::Code;
            let _ = transition(root, &mut state, Stage::Validate);
        }

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(state.consecutive_failures, mode::MAX_CONSECUTIVE_FAILURES);
        assert!(
            state
                .mode
                .should_gate(Stage::Validate, state.consecutive_failures),
            "reaching the ceiling must force the Auto-mode Validate gate"
        );
        assert_eq!(
            state.infra_failures, 0,
            "infra_failures must still reset unconditionally on the same hop the consecutive reset now skips"
        );
    }

    /// Combined 18d+18e scenario (18-RESEARCH.md Pitfall 1) — the only test
    /// that proves both fixes hold TOGETHER, not each in isolation: 18e's
    /// Layer-0 discard is what makes an `external_verify` Validate fail for
    /// the wrong reason, and 18d's counter reset is what made that failure
    /// loop unbounded — fixing either alone leaves the other's failure mode
    /// partially masked. Arm A (18e dominates) proves an `Ambiguous` outcome
    /// gates on the FIRST cycle, never touching `consecutive_failures`. Arm
    /// B (18d dominates) proves a genuine, non-ambiguous failure still
    /// reaches `MAX_CONSECUTIVE_FAILURES` and forces the gate — the case
    /// that, before 18d, ran forever.
    #[test]
    fn external_verify_cycles_reach_ceiling_without_unbounded_loop() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Arm A: an Ambiguous outcome gates on cycle one, never touching
        // consecutive_failures. Arm B: a genuine failure still reaches
        // MAX_CONSECUTIVE_FAILURES and forces the gate.
        arm_a_ambiguous_outcome_gates_on_cycle_one(root, 93);
        arm_b_genuine_failures_reach_the_ceiling(root, 94);
    }

    /// Arm A (18e dominates): an ambiguous `external_verify` outcome gates
    /// immediately — no Code↔Validate loop ever starts, so 18d's counter is
    /// irrelevant here and must stay untouched. Asserting that prevents a
    /// future refactor from quietly routing ambiguity back through the
    /// counter-based auto-loop.
    fn arm_a_ambiguous_outcome_gates_on_cycle_one(root: &Path, phase: u32) {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let result = agent_result::AgentResult {
            status: AgentStatus::Success,
            exit_code: None,
            reason: None,
            commits: None,
            summary: None,
            verdict: Some(Verdict::Gaps),
            decided_by_layer: Some(0),
        };
        let outcome = classify_validate_outcome(&result);
        assert!(matches!(outcome, ValidateOutcome::Ambiguous(_)));

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, outcome).unwrap();

        assert_eq!(
            state.consecutive_failures, 0,
            "18e's ambiguous gate must fire on cycle one, never touching 18d's counter"
        );
    }

    /// Arm B (18d dominates): a genuine, non-ambiguous `ValidateOutcome::Failed`
    /// driven through repeated Code↔Validate cycles reaches
    /// `MAX_CONSECUTIVE_FAILURES` and forces the gate. PATH is neutralized
    /// under `ENV_MUTEX` (matching `consecutive_failures_reaches_ceiling_across_cycles`)
    /// so neither `handle_validate_outcome`'s loop-back nor `transition`'s
    /// own `launch_stage` risk spawning a real agent CLI.
    fn arm_b_genuine_failures_reach_the_ceiling(root: &Path, phase: u32) {
        let _guard = ENV_MUTEX.lock().unwrap();

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        for _ in 0..mode::MAX_CONSECUTIVE_FAILURES {
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            state.stage = Stage::Code;
            let _ = transition(root, &mut state, Stage::Validate);
        }

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(state.consecutive_failures, mode::MAX_CONSECUTIVE_FAILURES);
        assert!(
            state
                .mode
                .should_gate(Stage::Validate, state.consecutive_failures),
            "a genuine repeated failure must still reach the reachable ceiling (18d)"
        );
    }

    /// 18d precision edge: `consecutive_failures` must saturate at `u32::MAX`
    /// rather than wrap to 0 on overflow, so a long-running stuck loop can't
    /// silently restore the unreachable-ceiling bug in a slower, harder-to-
    /// diagnose form. At `u32::MAX`, `should_gate` is already true, so the
    /// failure resolves via the forced-gate path — pre-seed a response so
    /// `run_gate`'s poll doesn't wait out the timeout.
    #[test]
    fn consecutive_failures_increment_saturates() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 82;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = u32::MAX;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, ValidateOutcome::Failed).unwrap();

        assert_eq!(state.consecutive_failures, u32::MAX);
    }

    /// D-09: a primary-loop `RateLimited` outcome writes the single-agent
    /// cron-instructions record (`devflow resume --phase N`) and returns
    /// without firing a blocking gate.
    #[test]
    fn primary_loop_rate_limited_writes_single_agent_cron_instructions() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 76;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(
            agent_result::stdout_path(root, phase),
            r#"{"type":"result","subtype":"error_rate_limit","retry_after":"2026-06-18T15:45:30Z"}"#,
        )
        .unwrap();

        advance(root, Some(phase)).unwrap();

        let instructions = devflow_core::ship::load_cron_instructions(root, phase).unwrap();
        assert_eq!(instructions.resume.command, "devflow");
        assert_eq!(
            instructions.resume.args,
            ["resume", "--phase", &phase.to_string()]
        );
        assert!(
            instructions
                .hermes_cron
                .command
                .contains(&format!("devflow resume --phase {phase}"))
        );

        // No blocking gate — state persists, stage unchanged, not gate-pending.
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.stage, Stage::Code);
        assert!(!reloaded.gate_pending);
        assert_eq!(reloaded.infra_failures, 1);
        assert_eq!(reloaded.consecutive_failures, 0);
        assert!(!Gates::gate_path(root, phase, Stage::Code).exists());
    }

    /// D-08/D-09: the RateLimited path at `infra_failures ==
    /// MAX_INFRA_FAILURES - 1` bumps to the ceiling and stops auto-resuming —
    /// it routes to the infra gate/abort path instead of writing a resume
    /// record (bounded resume, no soft-loop).
    #[test]
    fn rate_limited_at_infra_ceiling_stops_resuming_and_aborts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 77;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.infra_failures = mode::MAX_INFRA_FAILURES - 1;
        workflow::save_state(&state).unwrap();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(
            agent_result::stdout_path(root, phase),
            r#"{"type":"result","subtype":"error_rate_limit","retry_after":"2026-06-18T15:45:30Z"}"#,
        )
        .unwrap();

        advance(root, Some(phase)).unwrap();

        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(
            matches!(err, workflow::WorkflowError::MissingState(_)),
            "the infra ceiling must abort, clearing state"
        );
        assert!(
            devflow_core::ship::load_cron_instructions(root, phase).is_err(),
            "must not schedule an auto-resume once the infra ceiling stops resumption"
        );
    }

    /// CR-03: a rate-limit reason whose retry hint is unparseable (e.g. the
    /// `"usage limit"` fallback `detect_claude_rate_limit` produces for a 429
    /// with no retry_after) yields an EMPTY cron schedule — auto-resume is
    /// impossible. That must not return `Ok(())` silently (the detached
    /// monitor would exit with the phase stalled and zero operator signal);
    /// it must fire the same never-silent gate + notify the infra path uses
    /// (WR-11/D-15), and must never invent a schedule.
    #[test]
    fn rate_limited_with_unparseable_retry_hint_gates_instead_of_stalling_silently() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 81;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        // Pre-seed an Abort response so `run_gate`'s poll resolves immediately.
        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_rate_limited_outcome(
            root,
            &mut state,
            phase,
            Stage::Code,
            Some("rate limited until usage limit".into()),
        )
        .unwrap();

        let events =
            std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
        assert!(
            events.contains("gate_fired"),
            "an unparseable retry hint must raise a gate, not stall the phase silently: {events}"
        );
        assert!(
            events.contains("notify_fired"),
            "the operator must be notified that a manual resume is needed: {events}"
        );
        assert!(
            !events.contains("rate_limit_resume_scheduled"),
            "nothing was scheduled — emitting a resume-scheduled event would be a false signal: {events}"
        );

        // The unparseable hint must never become a schedule (an empty cron
        // expression would otherwise degrade into an every-minute resume).
        let instructions = devflow_core::ship::load_cron_instructions(root, phase).unwrap();
        assert!(instructions.hermes_cron.schedule.is_empty());
    }

    /// The Validate→Ship content hook (`DocsUpdate`) authors material about
    /// the branch being shipped, so it must run in that phase's worktree;
    /// the terminal batch merges/tags/deletes against the primary checkout
    /// and must NOT be retargeted. `ChangelogAppend` moved into the terminal
    /// batch in 17-12 (WR-04) for exactly this reason — it now targets
    /// `project_root`, not the worktree.
    ///
    /// Found live: `ChangelogAppend` wrote Phase 17's release note into
    /// `develop`'s CHANGELOG.md while all of its commits sat on
    /// `feature/phase-17`, stranding the entry on the wrong branch.
    #[test]
    fn content_hooks_target_the_worktree_while_terminal_hooks_stay_on_project_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let worktree = root.join(".worktrees/phase-70");
        std::fs::create_dir_all(&worktree).unwrap();

        let mut state = State::new(70, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.worktree_path = Some(worktree.clone());

        assert_eq!(
            hook_context_root(root, &state, false),
            worktree,
            "content hooks must write into the phase's worktree"
        );
        assert_eq!(
            hook_context_root(root, &state, true),
            root.to_path_buf(),
            "terminal hooks merge/tag/delete against the primary checkout"
        );

        // --no-worktree runs, and a worktree recorded but already removed,
        // both fall back to the project root rather than writing nowhere.
        let mut no_worktree = state.clone();
        no_worktree.worktree_path = None;
        assert_eq!(hook_context_root(root, &no_worktree, false), root);

        let mut missing = state.clone();
        missing.worktree_path = Some(root.join(".worktrees/gone"));
        assert_eq!(hook_context_root(root, &missing, false), root);
    }

    /// 13-06 dogfood regression: a multi-KB parser-derived reason reached
    /// the operator's desktop notification verbatim. Gate contexts must cap
    /// the reason to a readable headline.
    #[test]
    fn truncate_reason_caps_long_reasons_and_keeps_short_ones() {
        assert_eq!(truncate_reason("short reason"), "short reason");
        let long = "x".repeat(5000);
        let capped = truncate_reason(&long);
        assert!(capped.chars().count() <= 300);
        assert!(capped.ends_with("[truncated; full output in .devflow/]"));
    }

    #[test]
    fn gate_context_rendering_neutralizes_all_controls_and_obeys_limit() {
        let rendered = render_gate_context("line 1\n\u{1b}[2J\tline 2\u{7}", 100);
        assert!(!rendered.chars().any(char::is_control));
        assert_eq!(rendered, "line 1  [2J line 2 ");

        let bounded = render_gate_context(&"x".repeat(500), 100);
        assert_eq!(bounded.chars().count(), 100);
        assert!(bounded.ends_with("[truncated; full output in .devflow/]"));
    }

    /// A Ship-stage AgentFailed result (no `review:` prefix) must write a
    /// gate file and block for a response — not silently return an `Err`
    /// with nothing surfaced (WR-11; the pre-Task-2 catch-all never wrote a
    /// gate at all for this case). Runs `handle_ship_failure` on a scoped
    /// thread and busy-polls for the gate file to appear while the call is
    /// still blocked in `run_gate`'s poll, then unblocks it with an Abort
    /// response so the thread can finish without spawning a real monitor
    /// (Abort resolves via `abort()`, which never calls `launch_stage`).
    #[test]
    fn ship_agent_failed_fires_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = 40;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        let gate_path = Gates::gate_path(root, phase, Stage::Ship);
        let response_path = Gates::response_path(root, phase, Stage::Ship);

        std::thread::scope(|scope| {
            scope.spawn(|| {
                handle_ship_failure(root, &mut state, Some("agent crashed".into())).unwrap();
            });

            let mut seen = false;
            for _ in 0..150 {
                if gate_path.exists() {
                    seen = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            assert!(
                seen,
                "handle_ship_failure must write a gate file, not silently return an Err"
            );

            // Unblock the poll with an Abort response so the spawned thread
            // finishes (abort() cleans up on its own; no monitor spawned).
            std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
        });
    }

    /// A Ship-stage result whose reason starts with `review:` must loop back
    /// to Code instead of firing a gate — it does not go through `run_gate`
    /// at all, so no gate file is ever written for this path.
    ///
    /// Exercises `is_ship_review_failure` (the exact dispatch predicate
    /// `handle_ship_failure` uses) plus `prepare_loop_back_to_code` (the
    /// state-mutating half of `loop_back_to_code`) directly, rather than the
    /// full `handle_ship_failure` → `loop_back_to_code` → `launch_stage`
    /// chain: `launch_stage` spawns the real configured agent CLI (e.g. real
    /// `claude -p ... --dangerously-skip-permissions` if it's on `$PATH`),
    /// which must never fire from a unit test.
    #[test]
    fn ship_review_failed_loops_to_code() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = 41;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        let reason = Some("review: please fix naming".to_string());
        assert!(is_ship_review_failure(&reason));

        prepare_loop_back_to_code(root, &mut state, FixType::AuditFix).unwrap();

        assert_eq!(state.stage, Stage::Code);
        assert!(!Gates::gate_path(root, phase, Stage::Ship).exists());
        // Not finished — finish_workflow would have cleared state entirely.
        assert!(workflow::load_state(root, phase).is_ok());
    }

    /// The ReviewFailed loop-back must select `FixType::AuditFix`
    /// (`/gsd-audit-fix`), not the Validate path's `FixType::GapsOnly`
    /// (consensus #7 / OpenCode HIGH #2).
    #[test]
    fn ship_review_failed_uses_audit_fix() {
        assert!(is_ship_review_failure(&Some(
            "review: needs changes".into()
        )));
        assert!(is_ship_review_failure(&Some("  Review: nitpick".into())));
        assert!(!is_ship_review_failure(&Some("agent crashed".into())));
        assert!(!is_ship_review_failure(&None));

        let prompt = prompt::fix_prompt(FixType::AuditFix, 11);
        assert!(prompt.contains("/gsd-audit-fix"));
        assert!(!prompt.contains("--gaps-only"));
    }

    /// A Code-stage failure must fire a gate AND run the configured notify
    /// hook — with `DEVFLOW_NON_SILENT_GATE=1` since Auto mode would not
    /// normally gate a Code failure (unexpected/never-silent gate). The
    /// notify sentinel is a side effect distinct from the gate file itself,
    /// so it survives even though `Gates::cleanup` removes the gate/
    /// response/ack once the gate resolves. This test sets
    /// `DEVFLOW_GATE_NOTIFY_CMD`, so it's serialized under `ENV_MUTEX`.
    #[test]
    fn non_validate_failure_fires_gate_and_hook() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let sentinel = root.join("notify-sentinel");

        // SAFETY: serialized under ENV_MUTEX — no other thread in this
        // process sets/removes DEVFLOW_GATE_NOTIFY_CMD concurrently. Note
        // this only prevents races between env-*mutating* tests: any other
        // concurrently-running test that calls `run_gate` (most of them do)
        // will also read whatever we set here and may itself fire our
        // sentinel command with its own `unexpected` value. So we assert
        // only that the hook fired at all (sentinel created), not its exact
        // content — the exact DEVFLOW_NON_SILENT_GATE propagation is already
        // covered contamination-free by gates.rs's
        // `notify_hook_sets_non_silent_flag` (calls the pure
        // `run_notify_command` directly, no global env involved).
        unsafe {
            std::env::set_var(
                "DEVFLOW_GATE_NOTIFY_CMD",
                format!("touch {}", sentinel.display()),
            );
        }

        let phase = 42;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        // A Code-stage failure in Auto mode is exactly the "unexpected" case
        // `run_gate` computes (`!should_gate(..)`) and passes to
        // `fire_gate_notify` — asserted here as a pure, race-free check.
        assert!(
            !state
                .mode
                .should_gate(Stage::Code, state.consecutive_failures)
        );

        // Pre-write an Abort response so the call resolves without spawning
        // a monitor (the notify hook already fired by the time `run_gate`
        // starts polling, so this doesn't affect what we're asserting).
        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        let result =
            handle_stage_failure(root, &mut state, Stage::Code, Some("build failed".into()));

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            std::env::remove_var("DEVFLOW_GATE_NOTIFY_CMD");
        }

        result.unwrap();
        assert!(
            sentinel.exists(),
            "handle_stage_failure must fire the configured notify hook, not silently skip it"
        );
    }

    /// CR-01 regression: after a stage failure's gate resolves via Advance
    /// and the retry (also a stage failure) fires a fresh gate, the SECOND
    /// gate's poll must not instantly resolve from the FIRST gate's
    /// already-consumed response/ack — `handle_stage_failure` must clean
    /// those up before the retry launches.
    #[test]
    fn stage_failure_retry_cleans_stale_response() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = 43;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        // Pre-write an Abort response so the first failure resolves
        // immediately without spawning a monitor.
        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_stage_failure(root, &mut state, Stage::Code, Some("first failure".into())).unwrap();

        // abort() must have cleaned up the gate/response/ack for Code.
        assert!(!Gates::gate_path(root, phase, Stage::Code).exists());
        assert!(!Gates::response_path(root, phase, Stage::Code).exists());
        assert!(!Gates::ack_path(root, phase, Stage::Code).exists());

        // Simulate the phase reaching the same gate again later (e.g. a
        // fresh retry after abort would normally clear state, but re-fire
        // here directly to prove the CR-01 stale-response reuse regression
        // is closed): write a fresh gate but no new response.
        Gates::write_gate(root, phase, Stage::Code, "re-fired gate").unwrap();
        let started = std::time::Instant::now();
        let got = Gates::poll_response(root, phase, Stage::Code, 1);
        assert!(
            got.is_none(),
            "poll_response must not instantly resolve from a stale response after cleanup"
        );
        assert!(started.elapsed() >= std::time::Duration::from_secs(1));
    }
}
