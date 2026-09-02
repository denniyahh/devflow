//! Read-only structural oracle: "did this phase actually ship?"
//!
//! Closes the false-green attestation class described in `23-06-PLAN.md`'s
//! objective — an agent-authored attestation document (`VERIFICATION.md`,
//! `SUMMARY.md`, …) previously had to be trusted or caught by a
//! non-deterministic review prompt, because `devflow-core` never exposed its
//! own append-only record of whether a phase actually reached a finalized
//! Ship. [`collect`] exposes that record directly, and [`ShipEvidence::shipped`]
//! is safe to declare as a `verify::external_verify_commands` probe (Layer 0,
//! `agent_result.rs:704-711`): a failed declared probe outranks every
//! agent-controlled signal.
//!
//! **This module is opt-in per phase, deliberately, and this module does not
//! itself decide whether Layer 0 is active** — it only reports facts. A
//! declared command must be approved via
//! [`crate::verify::TRUST_EXTERNAL_VERIFY_ENV`] before it ever runs, on top
//! of whatever project-level configuration gates declared-probe execution in
//! the first place. This module must not be, and is not, the thing that
//! flips that switch on: a default-on, unconditional `--require-shipped`
//! probe would fail at every pre-Ship stage of every phase and block all
//! work (T-23-64). Declaring this probe is a per-phase choice a PLAN author
//! makes when a phase's own attestation claims a completed Ship.

use crate::git::GitFlow;
use crate::phase_id::PhaseId;
use crate::stage::Stage;
use crate::{events, workflow};
use serde::Serialize;
use std::path::Path;

/// The name of the event marking a phase as ended after one stage, still
/// carrying `workflow_finished` — kept as a named constant so its meaning
/// doesn't have to be re-derived at every call site that needs to explain
/// the ambiguity.
const STOPPED_AT_REASON: &str = "stopped_at";

/// The literal event name emitted at exactly one site —
/// `pipeline_gate::finish_workflow_with_gate_timeout`, after the entire
/// `hooks_after_ship` batch has succeeded — after which this module's
/// `shipped` predicate is true.
const WORKFLOW_SHIPPED_EVENT: &str = "workflow_shipped";

/// The older, ambiguous event name. Emitted at TWO sites (see
/// [`ShipEvidence::shipped`]'s doc comment): real Ship finalization, and
/// `transition`'s `--until` clean-stop branch. Deliberately not the
/// predicate.
const WORKFLOW_FINISHED_EVENT: &str = "workflow_finished";

/// DevFlow's own structural record of whether a phase has shipped.
///
/// Every field degrades to its safest value rather than erroring —
/// [`collect`] returns a value, never a `Result` — because an oracle that
/// can fail is an oracle a reviewer will learn to skip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShipEvidence {
    /// The phase this evidence was collected for.
    pub phase: PhaseId,
    /// The strict shipped predicate: whether the terminal-only
    /// `workflow_shipped` event has been emitted for this phase.
    ///
    /// **This is the load-bearing field of the whole module — read this
    /// comment before touching it.** An earlier revision of the plan that
    /// produced this module defined the shipped predicate as "whether
    /// `workflow_finished` has been emitted", asserted three separate times
    /// that it was the only site emitting that event, and was proven wrong
    /// by a cross-AI review before landing: `workflow_finished` is emitted at
    /// TWO sites. The first is real Ship finalization
    /// (`pipeline_gate::finish_workflow_with_gate_timeout`), guarded by the
    /// entire `hooks_after_ship` batch succeeding. The second is
    /// `transition`'s `devflow start --until <stage>` clean-stop branch
    /// (`crates/devflow-cli/src/pipeline_gate.rs`, the
    /// `state.stop_until == Some(from)` arm near the top of `transition`),
    /// which emits `workflow_finished` with `{"reason": "stopped_at", …}`
    /// and `return`s BEFORE any checkout hook, before `state.stage = to`,
    /// before the `"transition"` event, and before `launch_stage` — nothing
    /// resembling a Ship has run. Had `workflow_finished` stayed the
    /// predicate, a phase halted after one stage would read as shipped: a
    /// false green inside the very oracle built to eliminate false greens.
    ///
    /// The fix is structural, not a payload convention: a distinct,
    /// terminal-only `workflow_shipped` event, emitted at exactly one site,
    /// strictly after the `hooks_after_ship` batch's success loop breaks and
    /// strictly before the (unchanged) `workflow_finished` emission there.
    /// `shipped` reads ONLY this event — it must never fall back to
    /// filtering `workflow_finished` on `reason != "stopped_at"`, because
    /// that is a payload-discipline convention a future third emitter could
    /// silently violate (the real finalization payload is literally `Null`
    /// today, so "absence of a `reason` key" is exactly the fingerprint a
    /// careless new emitter would also have).
    ///
    /// Git ancestry is also deliberately not the predicate: `merged_into_develop`
    /// is shape-sensitive (a squash merge does not preserve the ancestry
    /// `is_merged_into_develop` checks), and it goes false for every
    /// successfully shipped phase once `BranchCleanup` — the hook that runs
    /// immediately after `Merge` in the very same `hooks_after_ship` batch —
    /// deletes the feature branch the ancestry check depends on. Git facts are
    /// reported below as corroboration only and never gate `shipped`.
    ///
    /// Phases that finalized before this event existed have no
    /// `workflow_shipped` line in their event log, so this reports `false`
    /// for them. That fail-closed direction is deliberate: an oracle that
    /// under-claims is safe, one that over-claims is the defect class this
    /// module exists to remove.
    pub shipped: bool,
    /// Corroboration only: whether the older `workflow_finished` event has
    /// ever been emitted for this phase. Never consulted by `shipped`.
    pub workflow_finished_seen: bool,
    /// The `reason` field from the last `workflow_finished` event, if any
    /// event exists and it carried one. A value of `"stopped_at"` is what
    /// distinguishes a `--until` halt from a real finalization — surfaced
    /// here so the ambiguity is legible in the oracle's own output instead
    /// of hidden inside this module's implementation.
    pub finished_reason: Option<String>,
    /// The phase's current stage, read from its persisted state file, or
    /// `None` when no state file exists (state is cleared once a phase
    /// finalizes — see `finish_workflow_with_gate_timeout`'s
    /// `workflow::clear_state` call).
    pub stage: Option<Stage>,
    /// Whether a state file exists at all for this phase.
    pub state_present: bool,
    /// Whether the phase's `feature/phase-NN` branch currently exists.
    pub feature_branch_exists: bool,
    /// Whether that branch (if it exists) is an ancestor of `develop`.
    /// Corroboration only — see `shipped`'s doc comment for why this is not
    /// the predicate.
    pub merged_into_develop: bool,
    /// Whether the repository has at least one configured remote.
    pub has_remote: bool,
}

/// Collect DevFlow's own structural record of whether `phase` has shipped.
///
/// Nothing in this module writes, commits, checks out, or emits — it is
/// strictly read-only. Every field degrades to its safest value rather than
/// failing: a root with no `.devflow` directory at all still returns a valid
/// `ShipEvidence` with `shipped: false` and `state_present: false`, never a
/// panic.
pub fn collect(project_root: &Path, phase: PhaseId) -> ShipEvidence {
    // The strict predicate, and nothing else — see `ShipEvidence::shipped`'s
    // doc comment for why this must never consult `workflow_finished_seen`,
    // `finished_reason`, or any git field.
    let shipped = events::has_event_for_phase(project_root, phase, WORKFLOW_SHIPPED_EVENT);

    let last_finished =
        events::last_event_of_kind_for_phase(project_root, phase, WORKFLOW_FINISHED_EVENT);
    let workflow_finished_seen = last_finished.is_some();
    let finished_reason = last_finished
        .as_ref()
        .and_then(|event| event.get("reason"))
        .and_then(|reason| reason.as_str())
        .map(str::to_owned);

    let (stage, state_present) = match workflow::load_state(project_root, phase) {
        Ok(state) => (Some(state.stage), true),
        Err(_) => (None, false),
    };

    // Project-resolved (45-01): `is_merged_into_develop` below asks whether
    // the phase branch is an ancestor of the TRUNK, and against a mismatched
    // trunk that answer is confidently wrong.
    let git = GitFlow::for_project(project_root);
    let branch = format!(
        "{}phase-{}",
        crate::config::git_flow_for_project(project_root).feature_prefix,
        phase.padded()
    );
    let feature_branch_exists = git.branch_exists(&branch);
    let merged_into_develop = git.is_merged_into_develop(phase);
    let has_remote = git.has_remote();

    ShipEvidence {
        phase,
        shipped,
        workflow_finished_seen,
        finished_reason,
        stage,
        state_present,
        feature_branch_exists,
        merged_into_develop,
        has_remote,
    }
}

/// Whether `finished_reason` names the `--until` clean-stop branch, so
/// callers (the CLI's `--require-shipped` failure message) can say "it
/// finished but it did not ship" instead of a generic "not shipped" — the
/// confusing case a reader hits first, per the plan's Task 1 acceptance
/// criteria.
pub fn is_stopped_at(evidence: &ShipEvidence) -> bool {
    evidence.finished_reason.as_deref() == Some(STOPPED_AT_REASON)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AgentKind, State};

    fn init_repo(root: &Path) {
        let git = |args: &[&str]| {
            let ok = crate::test_support::git_command(root)
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?} failed");
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("README.md"), "init\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "init"]);
        git(&["branch", "-M", "main"]);
        git(&["checkout", "-q", "-b", "develop"]);
    }

    /// The blocker's regression guard: a phase halted by `transition`'s
    /// `--until` clean-stop branch emits `workflow_finished` with a
    /// `"stopped_at"` reason and NOTHING else — `shipped` must read false.
    #[test]
    fn stopped_at_phase_reports_not_shipped_but_corroborates_finished() {
        let dir = tempfile::tempdir().unwrap();
        events::emit(
            dir.path(),
            PhaseId::new(42),
            "workflow_finished",
            serde_json::json!({"reason": "stopped_at", "stage": "plan"}),
        );

        let evidence = collect(dir.path(), PhaseId::new(42));
        assert!(
            !evidence.shipped,
            "a phase that only stopped must not read as shipped"
        );
        assert!(evidence.workflow_finished_seen);
        assert_eq!(evidence.finished_reason.as_deref(), Some("stopped_at"));
        assert!(is_stopped_at(&evidence));
    }

    #[test]
    fn shipped_event_is_true_only_for_the_phase_it_names() {
        let dir = tempfile::tempdir().unwrap();
        events::emit(
            dir.path(),
            PhaseId::new(7),
            "workflow_shipped",
            serde_json::json!({"stage": "ship"}),
        );

        assert!(collect(dir.path(), PhaseId::new(7)).shipped);
        assert!(!collect(dir.path(), PhaseId::new(8)).shipped);
    }

    #[test]
    fn shipped_predicate_consults_no_git_field() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        crate::test_support::git_command(dir.path())
            .args(["branch", "feature/phase-05", "develop"])
            .status()
            .unwrap();
        // The branch exists and IS merged into develop (it was branched
        // from develop's tip), and a remote is configured — every git field
        // true — but no shipped event was ever emitted.
        crate::test_support::git_command(dir.path())
            .args([
                "remote",
                "add",
                "origin",
                "https://example.invalid/repo.git",
            ])
            .status()
            .unwrap();

        let evidence = collect(dir.path(), PhaseId::new(5));
        assert!(evidence.feature_branch_exists);
        assert!(evidence.merged_into_develop);
        assert!(evidence.has_remote);
        assert!(
            !evidence.shipped,
            "shipped must not be inferred from any git field"
        );
    }

    #[test]
    fn torn_final_line_does_not_hide_an_earlier_shipped_event() {
        let dir = tempfile::tempdir().unwrap();
        events::emit(
            dir.path(),
            PhaseId::new(9),
            "workflow_shipped",
            serde_json::json!({"stage": "ship"}),
        );
        let path = events::events_path(dir.path());
        let mut contents = std::fs::read_to_string(&path).unwrap();
        contents.push_str("{truncated\n");
        std::fs::write(&path, contents).unwrap();

        assert!(collect(dir.path(), PhaseId::new(9)).shipped);
    }

    #[test]
    fn missing_devflow_dir_degrades_safely_without_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let evidence = collect(dir.path(), PhaseId::new(1));
        assert!(!evidence.shipped);
        assert!(!evidence.state_present);
        assert!(evidence.stage.is_none());
        assert!(!evidence.workflow_finished_seen);
    }

    #[test]
    fn collect_reports_stage_and_state_present_from_live_state() {
        let dir = tempfile::tempdir().unwrap();
        let state = State::new(
            PhaseId::new(3),
            AgentKind::Claude,
            crate::mode::Mode::Auto,
            dir.path().to_path_buf(),
        );
        workflow::save_state(&state).unwrap();

        let evidence = collect(dir.path(), PhaseId::new(3));
        assert!(evidence.state_present);
        assert_eq!(evidence.stage, Some(Stage::Define));
        assert!(!evidence.shipped);
    }
}
