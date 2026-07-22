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

use crate::pipeline_launch::launch_stage;
use crate::pipeline_launch::launch_stage_inner;
use crate::{CliError, abort, phase_artifact_on_develop, run_gate, truncate_reason};
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
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(root)
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
}
