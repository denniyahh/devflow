//! DevFlow state machine.
//!
//! Drives the development workflow through a single linear chain of five stages:
//! Define → Plan → Code → Validate → Ship. See [`crate::stage::Stage`].

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::mode::Mode;
use crate::phase_id::PhaseId;
use crate::stage::Stage;

/// Full workflow state persisted to `.devflow/state.json`.
///
/// # Construction
///
/// Marked `#[non_exhaustive]`: downstream crates must build this through
/// [`State::new`] and then assign the fields they care about, rather than by
/// struct literal. Deserialization is unaffected — the `Deserialize` derive
/// and every `#[serde(default)]` field keep working exactly as before, so
/// state files written by older binaries still load.
///
/// This exists because `State` accumulates a field roughly every phase that
/// adds a run-scoped concept (`worktree_path`, `monitor_pid`, `stop_until`,
/// `yes_ship`, and — in phase 28 — `session_id` and `checkpoint_resumes`).
/// Without `non_exhaustive`, each of those additions is a semver-breaking
/// change for any consumer that used a struct literal, which would force a
/// major bump for what is really an internal bookkeeping change. Paying that
/// cost once here makes every future field additive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct State {
    /// Current workflow stage.
    pub stage: Stage,
    /// Phase number being worked on.
    pub phase: PhaseId,
    /// Which coding agent was launched.
    pub agent: AgentKind,
    /// How the pipeline is driven (auto vs. supervise).
    pub mode: Mode,
    /// Whether a gate has been written and is awaiting a human response.
    #[serde(default)]
    pub gate_pending: bool,
    /// Consecutive Validate failures — drives the Auto-mode forced gate after
    /// [`crate::mode::MAX_CONSECUTIVE_FAILURES`] failures. Persisted across
    /// `devflow advance` invocations so the counter survives monitor restarts.
    #[serde(default)]
    pub consecutive_failures: u32,
    /// Consecutive infrastructure-class faults (`ResourceKilled`,
    /// `AgentUnavailable`) — distinct from [`Self::consecutive_failures`]
    /// (D-08, 17-01). Gates at [`crate::mode::MAX_INFRA_FAILURES`]. Any
    /// increment (wired in Plan 04) must use `saturating_add` so a
    /// long-running stuck loop cannot overflow `u32`. A serde-absent value
    /// (older persisted state) defaults to 0. Reset to 0 on every successful
    /// stage transition, alongside `consecutive_failures` (CR-01, 17-06 gap
    /// closure), so the ceiling bounds a stuck loop, not a phase's lifetime.
    #[serde(default)]
    pub infra_failures: u32,
    /// How many times a preflight gate has been resolved and retried for
    /// this phase (18f). Bounded by [`crate::mode::MAX_PREFLIGHT_RETRIES`].
    /// Persisted rather than recursion-scoped because the documented wedge
    /// spanned separate `devflow` invocations after a monitor death — an
    /// in-process recursion-depth counter would reset to zero on every new
    /// process and fail to bound the exact incident it exists to prevent.
    /// Reset to 0 whenever preflight passes and whenever a human explicitly
    /// approves (`GateAction::Advance`), both inside `run_preflight`. Unlike
    /// [`Self::consecutive_failures`] and [`Self::infra_failures`], this
    /// counter is NOT touched by `transition()`.
    #[serde(default)]
    pub preflight_retries: u32,
    /// The commit count observed on the phase's feature branch at the most
    /// recent Validate failure (999.66, D-03) — the forward-progress
    /// baseline [`crate::mode::consecutive_failures_made_progress`] compares
    /// against to decide whether a new failure begins a fresh streak or
    /// continues the existing one.
    ///
    /// `None` means no prior failure has been recorded — either the first
    /// failure of a phase, or the first failure observed after resuming
    /// state written by a binary predating this field — and is deliberately
    /// distinct from `Some(0)`, which means a failure WAS recorded and the
    /// branch genuinely carried zero commits at that moment; a later failure
    /// that again counts zero commits must accumulate against that `Some(0)`
    /// baseline rather than being treated as a fresh streak.
    ///
    /// A serde-absent value (state written by a binary predating this field)
    /// deserializes to `None`, which is exactly the "no prior record"
    /// meaning above — the same backward-compat pattern as every other
    /// `#[serde(default)]` field added since 17-01.
    ///
    /// Unlike [`Self::consecutive_failures`] and [`Self::infra_failures`],
    /// this field is NOT touched by `transition()` — it is a baseline
    /// observation rather than a counter, matching how
    /// [`Self::preflight_retries`] and [`Self::checkpoint_resumes`] are
    /// handled. It is replaced wholesale at each failure rather than
    /// incremented, so it needs no `saturating_add` treatment, unlike every
    /// other numeric field on this struct.
    #[serde(default)]
    pub last_validate_failure_commit_count: Option<u32>,
    /// Every Validate failure recorded for this PHASE, accumulated without
    /// regard to forward progress (999.78/WR-01, D-07) — the backstop bound
    /// [`crate::mode::MAX_PHASE_VALIDATE_FAILURES`] compares against, and the
    /// leading number in the Supervise gate message (WR-04).
    ///
    /// A serde-absent value (state written by a binary predating this field)
    /// deserializes to 0, which is exactly its "no failures recorded for this
    /// phase" meaning — the same backward-compat pattern as every other
    /// `#[serde(default)]` field added since 17-01. Unlike
    /// [`Self::last_validate_failure_commit_count`], zero is not ambiguous
    /// here: an upgraded binary and a genuine first failure both start the
    /// budget at its full width, and that widening is what IN-02's distinct
    /// loop-back reason exists to announce.
    ///
    /// Why it exists next to [`Self::consecutive_failures`] rather than
    /// replacing it: `consecutive_failures` is reset whenever
    /// [`crate::mode::consecutive_failures_made_progress`] reports that new
    /// commits landed, and the Code stage's fix command is a GSD command
    /// which routinely commits `.planning/` artifacts even when no source
    /// changed. A loop that commits something trivial every cycle therefore
    /// resets the streak every cycle and never reaches
    /// [`crate::mode::MAX_CONSECUTIVE_FAILURES`]. This total cannot be reset
    /// by a commit count.
    ///
    /// **Lifetime — deliberately unlike every other counter on this struct.**
    /// It is NOT touched by the stage transition (`transition_resets_*` has no
    /// say over it), matching how [`Self::preflight_retries`] and
    /// [`Self::checkpoint_resumes`] are handled, because it is a per-phase
    /// total rather than a per-streak counter. It is also carried across a
    /// forced restart: `commands::start()` reads any persisted state for the
    /// same phase and copies this one field into the fresh `State`, because a
    /// bound a `devflow start --force` resets does not bound the unattended
    /// case D-07 exists for. Exactly two events reset it to zero:
    ///
    /// 1. **Phase completion** — `finish_workflow_with_gate_timeout` calls
    ///    `workflow::clear_state`, deleting `.devflow/state-{NN}.json`, so the
    ///    next start for that phase finds nothing to carry.
    /// 2. **Operator approval at the ceiling gate** — the Validate gate
    ///    handling zeroes it when a human advances or loops back AND
    ///    [`crate::mode::phase_failure_ceiling_reached`] is true. Keyed on that
    ///    predicate and never on "a gate fired": Supervise gates on every
    ///    Validate, so a gate-keyed reset would clear the total at every
    ///    failure and it would never accumulate in the one mode where an
    ///    operator watches every occurrence.
    ///
    /// Any increment must use `saturating_add`, like [`Self::infra_failures`]
    /// and [`Self::checkpoint_resumes`], so an exhausted budget can never wrap
    /// back to zero and silently restore itself.
    #[serde(default)]
    pub phase_validate_failures: u32,
    /// The content fingerprint of this phase's `{N}-VERIFICATION.md` as it
    /// stood at the START of this run (999.79), read via
    /// [`crate::agent_result::phase_verification_fingerprint`] once the
    /// evidence root for the run is known.
    ///
    /// `None` means no artifact was observed at the start of this run — the
    /// ordinary case for a phase being executed for the first time. It is
    /// deliberately distinct from `Some(h)`: an artifact that EXISTS now where
    /// the baseline recorded none was authored during this run, whereas an
    /// artifact whose fingerprint still equals the baseline was inherited from
    /// a previous run and its verdict must not be reused.
    ///
    /// **State written by a binary predating this field also deserializes to
    /// `None`, and that is NOT the same reading** (WR-05, 35-REVIEW). This doc
    /// comment used to claim it was. For a phase started under an older binary
    /// and continued by this one, the previous run's committed
    /// `{N}-VERIFICATION.md` is already on disk while the baseline reads
    /// `None` — so the `(Some, None)` row would classify an inherited artifact
    /// as authored-this-run and dispatch `--gaps-only` against zero matching
    /// plans, gating unresolvably. That is verbatim the DOGFOOD-01-class stall
    /// 999.79 exists to close, reproduced for every in-flight phase across the
    /// upgrade.
    ///
    /// [`Self::verification_baseline_captured`] is the discriminator: only a
    /// run that actually performed the observation sets it, so a `None` from an
    /// old state file is distinguishable from a `None` that means "looked, and
    /// there was nothing there".
    ///
    /// Why this exists at all: nothing deletes or dates `{N}-VERIFICATION.md`,
    /// so a `devflow start --force` re-run checks out a branch still carrying
    /// the previous run's committed copy. Without this baseline the first
    /// Validate failure of that re-run reads the inherited artifact as a
    /// verdict and dispatches a `--gaps-only` pass against zero matching plans,
    /// which gates unresolvably — the same unattended-stall class as
    /// DOGFOOD-01, reached from a different direction.
    ///
    /// **Lifetime.** Like [`Self::last_validate_failure_commit_count`], and
    /// unlike [`Self::consecutive_failures`] and [`Self::infra_failures`], this
    /// field is NOT touched by `transition()` — it is a run-scoped observation
    /// rather than a counter, so it is replaced wholesale rather than
    /// incremented and needs no `saturating_add` treatment. It is also NOT
    /// carried across a forced restart the way
    /// [`Self::phase_validate_failures`] is: a new run must re-observe the
    /// artifact, because the whole point is to compare against what THIS run
    /// started with.
    #[serde(default)]
    pub last_verification_fingerprint: Option<u64>,
    /// Whether [`Self::last_verification_fingerprint`] was actually observed by
    /// this run, as opposed to merely absent (WR-05, 35-REVIEW).
    ///
    /// `Option<u64>` cannot carry this on its own: `None` means both "the run
    /// looked and found no artifact" and "this state file predates the field,
    /// so nobody ever looked", and those two demand OPPOSITE dispatches. The
    /// first is the ordinary first-verification case and `--gaps-only` is
    /// right; the second may be sitting on an inherited artifact, where
    /// `--gaps-only` matches zero plans and stalls.
    ///
    /// `false` is therefore the correct serde default in both directions: a
    /// state file written before this field existed genuinely did not capture a
    /// baseline, and the conservative reading of an artifact whose provenance
    /// is unknown is "inherited" — a full execute is wasteful, an unresolvable
    /// gate is not recoverable.
    ///
    /// Set exactly once per run, at the same site that captures the baseline,
    /// after `state.worktree_path` holds its final value.
    #[serde(default)]
    pub verification_baseline_captured: bool,
    /// The mtime of the same artifact [`Self::last_verification_fingerprint`]
    /// hashes, in nanoseconds since the Unix epoch, as of the same observation.
    ///
    /// WR-06 (35-REVIEW): a content fingerprint cannot see an IDEMPOTENT
    /// rewrite. A Validate agent that re-authors byte-identical content on a
    /// later failing cycle produces the same hash as an artifact nobody
    /// touched, so a hash-only rule reads its own agent's work as inherited and
    /// dispatches a full execute — re-running every plan in the phase on every
    /// subsequent cycle instead of the gaps-only pass Phase 33 built. That is
    /// the "too strict" direction the freshness rule's own comment claims to
    /// guard against and did not.
    ///
    /// Moves in lockstep with the fingerprint: written at the same capture
    /// site, replaced at the same update site, and never read on its own — the
    /// pair is the observation, and either one differing means the artifact was
    /// written during this run.
    #[serde(default)]
    pub last_verification_mtime_nanos: Option<u64>,
    /// When the phase started (Unix seconds).
    pub started_at: String,
    /// Path to the project root.
    pub project_root: PathBuf,
    /// Working directory for the agent when running in a git worktree.
    ///
    /// `None` means the agent runs in `project_root`. State and capture files
    /// always live under the main `project_root`; only the agent's cwd changes.
    #[serde(default)]
    pub worktree_path: Option<PathBuf>,
    /// PID of the detached monitor process that owns the agent for the
    /// current stage, recorded by `launch_stage` at spawn time. `None` means
    /// no monitor has been spawned for this state yet, OR the state was
    /// written by a binary predating this field — in both cases the
    /// liveness probe reports Unknown, never Stuck.
    #[serde(default)]
    pub monitor_pid: Option<u32>,
    /// The Claude session id captured from the most recent captured stdout
    /// envelope for this phase's current stage (D-04, 28-02), read via
    /// [`crate::agent_result::session_id_from_capture`]. `None` means EITHER
    /// "no session has been captured for this state yet" OR "the state was
    /// written by a binary predating this field" — both cases behave
    /// identically (no relaunch target to address). Recorded so a checkpoint
    /// auto-decide relaunch (plan 28-03) can `--resume` the exact session
    /// that hit the checkpoint rather than spawning a fresh one, which would
    /// lose the original session's conversation context and permission mode.
    #[serde(default)]
    pub session_id: Option<String>,
    /// How many times the current stage's agent has been relaunched via a
    /// checkpoint auto-decide resume (D-04, 28-03). Bounds a stuck
    /// checkpoint loop against `mode::MAX_CHECKPOINT_RESUMES` (added in plan
    /// 28-03) the same way [`Self::infra_failures`] bounds an infra-fault
    /// loop against `mode::MAX_INFRA_FAILURES`. Reset to 0 by every ordinary fresh stage
    /// launch, so the ceiling bounds one stage's resume budget, not a
    /// phase's lifetime (the same distinction `MAX_INFRA_FAILURES`' doc
    /// comment draws for `infra_failures`). Any increment must use
    /// `saturating_add` so a stuck loop cannot overflow `u32`. A
    /// serde-absent value (state written by a binary predating this field)
    /// defaults to 0.
    #[serde(default)]
    pub checkpoint_resumes: u32,
    /// The stage `devflow start --until <stage>` requests as the last stage
    /// to run before halting (20c). `None` means no stop point was
    /// requested (the pipeline runs to Ship), OR the state was written by a
    /// binary predating this field — both cases behave identically (no
    /// interception in `transition()`).
    #[serde(default)]
    pub stop_until: Option<Stage>,
    /// Set by `transition()` when `stop_until` names the stage just
    /// completed — a terminal-but-not-failed halt short of Ship (20c).
    /// `false` for a normal in-flight or completed-to-Ship phase, and for
    /// any state written by a binary predating this field.
    #[serde(default)]
    pub stopped: bool,
    /// Human-readable reason recorded alongside `stopped` (20c). `None`
    /// when `stopped` is `false`, or when the state predates this field.
    #[serde(default)]
    pub stop_reason: Option<String>,
    /// Pre-authorization for the Ship gate (D-04/D-05/D-06, 23-09),
    /// set only from the `--yes-ship` CLI flag typed on `devflow start`.
    ///
    /// Persisted rather than passed through the call stack: the Ship gate
    /// fires inside a detached monitor's `advance` process, minutes to
    /// hours after the launching `devflow start` process has already
    /// exited, so a CLI-scoped value would be gone by the time it matters —
    /// only a value written to `state.json` at start time survives to be
    /// read back by that later, separate process. `false` for any state
    /// written by a binary predating this field.
    #[serde(default)]
    pub yes_ship: bool,
    /// What this run's delivery canary established (D-13/D-15, 31-03),
    /// recorded by the first stage launch that routes through the Claude
    /// `stream-json` transport. `None` means EITHER "no canary has run for
    /// this run yet" OR "the state was written by a binary predating this
    /// field" — both cases behave identically: the canary runs.
    ///
    /// Persisted rather than held in memory for the same reason
    /// [`Self::yes_ship`] is: each stage launch happens in a SEPARATE
    /// `devflow` process (the monitor's own `advance` tail), so an
    /// in-process flag would reset to "not yet run" at every stage
    /// transition and re-spend a real throwaway agent invocation each time —
    /// which is exactly the symptom 31-RESEARCH Pitfall 5 names for a canary
    /// that landed in the per-stage `preflight` hook.
    ///
    /// A recorded `Absent`/`Unverified` keeps refusing on every later launch
    /// in the run; it is not consumed by the first refusal.
    #[serde(default)]
    pub canary: Option<crate::canary::CanaryOutcome>,
    /// D-11's opt-out: force the pre-31 single-document Claude launch
    /// (positional prompt, `--output-format json`, the `sh` monitor) for this
    /// run, off by default.
    ///
    /// `false` means EITHER "the operator did not ask for the legacy path" OR
    /// "the state was written by a binary predating this field" — both cases
    /// behave identically: the D-09/D-10 rollout decides the transport, which
    /// is the pre-existing behaviour.
    ///
    /// Persisted rather than passed through the call stack for the reason
    /// [`Self::yes_ship`] gives: each stage launch happens in a SEPARATE
    /// `devflow` process (the detached monitor's own `advance` tail), so a
    /// CLI-scoped value would be gone by the time the second stage launches
    /// and the run would silently revert to the stream transport mid-flight.
    ///
    /// Only ever OR-ed, never cleared, once set — see
    /// `pipeline_launch::apply_legacy_launch_opt_out`. Clearing it on a plain
    /// `devflow resume` would be the same silent-drop class as `stop_until`'s
    /// old unconditional clear (999.60). To turn it back off, edit
    /// `.devflow/state-NN.json` or start a new run.
    #[serde(default)]
    pub legacy_claude_launch: bool,
}

/// Supported coding agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    /// Anthropic Claude Code CLI.
    Claude,
    /// OpenAI Codex CLI.
    Codex,
    /// OpenCode CLI.
    OpenCode,
}

impl fmt::Display for AgentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::OpenCode => "opencode",
        };
        f.write_str(name)
    }
}

impl FromStr for AgentKind {
    type Err = AgentParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "claude" => Ok(AgentKind::Claude),
            "codex" => Ok(AgentKind::Codex),
            "opencode" | "open-code" => Ok(AgentKind::OpenCode),
            other => Err(AgentParseError(other.to_string())),
        }
    }
}

/// Error returned when parsing an unsupported agent name.
#[derive(Debug, Clone, thiserror::Error)]
#[error("unsupported agent `{0}`; expected claude, codex, or opencode")]
pub struct AgentParseError(String);

impl State {
    /// Create a new state for starting a phase at the [`Stage::Define`] stage.
    pub fn new(phase: PhaseId, agent: AgentKind, mode: Mode, project_root: PathBuf) -> Self {
        State {
            stage: Stage::Define,
            phase,
            agent,
            mode,
            gate_pending: false,
            consecutive_failures: 0,
            infra_failures: 0,
            preflight_retries: 0,
            last_validate_failure_commit_count: None,
            phase_validate_failures: 0,
            last_verification_fingerprint: None,
            verification_baseline_captured: false,
            last_verification_mtime_nanos: None,
            started_at: timestamp_now(),
            project_root,
            worktree_path: None,
            monitor_pid: None,
            session_id: None,
            checkpoint_resumes: 0,
            stop_until: None,
            stopped: false,
            stop_reason: None,
            yes_ship: false,
            canary: None,
            legacy_claude_launch: false,
        }
    }
}

fn timestamp_now() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("{}", duration.as_secs()),
        Err(_) => String::from("0"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn agent_name_and_display() {
        use crate::agents::adapter_for;
        assert_eq!(adapter_for(AgentKind::Claude).name(), "Claude Code");
        assert_eq!(adapter_for(AgentKind::Codex).name(), "OpenAI Codex");
        assert_eq!(adapter_for(AgentKind::OpenCode).name(), "OpenCode");

        assert_eq!(AgentKind::Claude.to_string(), "claude");
        assert_eq!(AgentKind::Codex.to_string(), "codex");
        assert_eq!(AgentKind::OpenCode.to_string(), "opencode");
    }

    #[test]
    fn agent_from_str_accepts_canonical_and_aliases() {
        assert_eq!("claude".parse::<AgentKind>().unwrap(), AgentKind::Claude);
        assert_eq!("CLAUDE".parse::<AgentKind>().unwrap(), AgentKind::Claude);
        assert_eq!("codex".parse::<AgentKind>().unwrap(), AgentKind::Codex);
        assert_eq!(
            "opencode".parse::<AgentKind>().unwrap(),
            AgentKind::OpenCode
        );
        assert_eq!(
            "open-code".parse::<AgentKind>().unwrap(),
            AgentKind::OpenCode
        );
    }

    #[test]
    fn agent_from_str_rejects_unknown() {
        let err = "aider".parse::<AgentKind>().unwrap_err();
        assert!(err.to_string().contains("aider"));
    }

    #[test]
    fn new_state_starts_at_define() {
        let state = State::new(
            PhaseId::new(2),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        assert_eq!(state.stage, Stage::Define);
        assert_eq!(state.phase, PhaseId::new(2));
        assert_eq!(state.agent, AgentKind::Claude);
        assert_eq!(state.mode, Mode::Auto);
        assert!(!state.gate_pending);
        assert_eq!(state.consecutive_failures, 0);
        assert_eq!(state.infra_failures, 0);
        assert_eq!(state.preflight_retries, 0);
        assert_eq!(state.phase_validate_failures, 0);
        assert!(!state.started_at.is_empty());
        assert_eq!(state.monitor_pid, None);
        assert_eq!(state.stop_until, None);
        assert!(!state.stopped);
        assert_eq!(state.stop_reason, None);
        assert!(!state.yes_ship);
    }

    #[test]
    fn state_serde_round_trips() {
        let state = State::new(
            PhaseId::new(9),
            AgentKind::Codex,
            Mode::Supervise,
            PathBuf::from("/repo"),
        );
        let json = serde_json::to_string(&state).unwrap();
        let back: State = serde_json::from_str(&json).unwrap();
        assert_eq!(back.phase, PhaseId::new(9));
        assert_eq!(back.agent, AgentKind::Codex);
        assert_eq!(back.stage, Stage::Define);
        assert_eq!(back.mode, Mode::Supervise);
    }

    #[test]
    fn consecutive_failures_persists_across_advance_calls() {
        let mut state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        state.consecutive_failures = 3;
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("consecutive_failures"),
            "consecutive_failures must appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.consecutive_failures, 3,
            "consecutive_failures must round-trip through serde"
        );
    }

    /// D-08 (17-01): a distinct infra-failure counter round-trips through
    /// serde and its own key appears in the persisted JSON.
    #[test]
    fn infra_failures_round_trips_through_serde() {
        let mut state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        state.infra_failures = 4;
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("infra_failures"),
            "infra_failures must appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.infra_failures, 4,
            "infra_failures must round-trip through serde"
        );
    }

    /// A serde-absent `infra_failures` (older persisted state.json without
    /// the field) must default to 0, not fail to deserialize.
    #[test]
    fn infra_failures_absent_from_json_defaults_to_zero() {
        let json = r#"{
            "stage": "code",
            "phase": 1,
            "agent": "claude",
            "mode": "auto",
            "started_at": "0",
            "project_root": "/repo"
        }"#;
        let loaded: State = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.infra_failures, 0);
    }

    /// `last_validate_failure_commit_count` round-trips through serde as an
    /// exact `Option<u32>` (999.66, D-03) — its own key appears in the
    /// persisted JSON before the value round-trip is asserted, so a field
    /// accidentally attributed `skip_serializing_if` (which would still pass
    /// a naive in-memory round-trip while never persisting anything) is
    /// caught.
    #[test]
    fn last_validate_failure_commit_count_round_trips_through_serde() {
        let mut state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        state.last_validate_failure_commit_count = Some(3);
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("last_validate_failure_commit_count"),
            "last_validate_failure_commit_count must appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.last_validate_failure_commit_count,
            Some(3),
            "last_validate_failure_commit_count must round-trip through serde"
        );
    }

    /// A serde-absent `last_validate_failure_commit_count` (state written by
    /// a binary predating this field) must deserialize to `None` — the
    /// "no prior failure recorded" meaning — not to `Some(0)`, which would
    /// misrepresent a never-observed baseline as an observed zero.
    #[test]
    fn last_validate_failure_commit_count_absent_from_json_defaults_to_none() {
        let json = r#"{
            "stage": "code",
            "phase": 1,
            "agent": "claude",
            "mode": "auto",
            "started_at": "0",
            "project_root": "/repo"
        }"#;
        let loaded: State = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.last_validate_failure_commit_count, None);
    }

    /// 999.78/D-07: `phase_validate_failures` round-trips through serde. The
    /// key-presence assertion comes BEFORE the value round-trip deliberately —
    /// a field that never actually persists still passes a naive in-memory
    /// round trip, and a bound that lives only in memory does not bound a
    /// phase whose whole failure mode spans separate `devflow` processes.
    #[test]
    fn phase_validate_failures_round_trips_through_serde() {
        let mut state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        state.phase_validate_failures = 7;
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("phase_validate_failures"),
            "phase_validate_failures must appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.phase_validate_failures, 7,
            "phase_validate_failures must round-trip through serde"
        );
    }

    /// A serde-absent `phase_validate_failures` (state written by a binary
    /// predating this field) deserializes to 0 — "no failures recorded for
    /// this phase" — rather than failing the load outright, which would make
    /// an upgrade mid-phase unrecoverable.
    #[test]
    fn phase_validate_failures_absent_from_json_defaults_to_zero() {
        let json = r#"{
            "stage": "code",
            "phase": 1,
            "agent": "claude",
            "mode": "auto",
            "started_at": "0",
            "project_root": "/repo"
        }"#;
        let loaded: State = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.phase_validate_failures, 0);
    }

    /// 999.79 (35-05): `last_verification_fingerprint` round-trips through
    /// serde. The key-presence assertion comes BEFORE the value round-trip for
    /// the same reason the two fields above give — this baseline is written by
    /// `devflow start` and compared by a later `devflow advance`, which is a
    /// different process, so a field that never reaches disk would leave every
    /// comparison reading `None` and defeat the whole rule.
    #[test]
    fn last_verification_fingerprint_round_trips_through_serde() {
        let mut state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        state.last_verification_fingerprint = Some(0x0123_4567_89ab_cdef);
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("last_verification_fingerprint"),
            "last_verification_fingerprint must appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.last_verification_fingerprint,
            Some(0x0123_4567_89ab_cdef),
            "last_verification_fingerprint must round-trip through serde"
        );
    }

    /// A serde-absent `last_verification_fingerprint` (state written by a
    /// binary predating this field) deserializes to `None` — "no artifact was
    /// observed at the start of this run" — rather than failing the load, which
    /// would make an upgrade mid-phase unrecoverable.
    #[test]
    fn last_verification_fingerprint_absent_from_json_defaults_to_none() {
        let json = r#"{
            "stage": "code",
            "phase": 1,
            "agent": "claude",
            "mode": "auto",
            "started_at": "0",
            "project_root": "/repo"
        }"#;
        let loaded: State = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.last_verification_fingerprint, None);
        // WR-05 (35-REVIEW): the SAME absent JSON must also report that nobody
        // captured a baseline. `None` alone cannot carry that — it means both
        // "looked, found nothing" and "never looked" — and the two demand
        // opposite dispatches downstream.
        assert!(
            !loaded.verification_baseline_captured,
            "state predating the baseline field never captured one, and must not claim to"
        );
    }

    /// The other half of the pair above: a state file written by THIS binary
    /// carries the flag, so the two cases really are distinguishable after a
    /// round trip. Without this, `verification_baseline_captured` could be
    /// hardcoded `false` and the absent-JSON assertion above would still pass.
    #[test]
    fn verification_baseline_captured_round_trips_through_serde() {
        let mut state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        state.verification_baseline_captured = true;
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("verification_baseline_captured"),
            "verification_baseline_captured must appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert!(
            loaded.verification_baseline_captured,
            "a captured baseline must survive the save/load the real pipeline performs"
        );
    }

    /// D-18f: `preflight_retries` round-trips through serde (its own key
    /// appears in the persisted JSON) — the wedge this counter bounds spans
    /// separate `devflow` invocations, so it must survive a save/load
    /// cycle, not just live in memory — and a serde-absent value (state
    /// written by a pre-18f binary) deserializes to 0, not a hard error.
    #[test]
    fn preflight_retries_round_trips_through_serde() {
        let mut state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        state.preflight_retries = 2;
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("preflight_retries"),
            "preflight_retries must appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.preflight_retries, 2,
            "preflight_retries must round-trip through serde"
        );

        let absent_json = r#"{
            "stage": "code",
            "phase": 1,
            "agent": "claude",
            "mode": "auto",
            "started_at": "0",
            "project_root": "/repo"
        }"#;
        let loaded_absent: State = serde_json::from_str(absent_json).unwrap();
        assert_eq!(loaded_absent.preflight_retries, 0);
    }

    /// `monitor_pid` round-trips through serde as an exact `u32` (18b).
    #[test]
    fn monitor_pid_round_trips_through_serde() {
        let mut state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        state.monitor_pid = Some(4242);
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("monitor_pid"),
            "monitor_pid must appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.monitor_pid,
            Some(4242),
            "monitor_pid must round-trip through serde"
        );
    }

    /// A serde-absent `monitor_pid` (state written by a pre-18b binary) must
    /// deserialize to `None`, not `Some(0)` — a `Some(0)` default would let a
    /// pre-18b state file render as a monitor at pid 0.
    #[test]
    fn monitor_pid_absent_from_json_defaults_to_none() {
        let json = r#"{
            "stage": "code",
            "phase": 1,
            "agent": "claude",
            "mode": "auto",
            "started_at": "0",
            "project_root": "/repo"
        }"#;
        let loaded: State = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.monitor_pid, None);
    }

    /// `session_id` round-trips through serde as an exact `Option<String>`
    /// (D-04, 28-02) — mirrors the `monitor_pid` pair above.
    #[test]
    fn session_id_round_trips_through_serde() {
        let mut state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        state.session_id = Some("cf29bfec-69e8-45df-a4f3-3da08ab6f66e".to_string());
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("session_id"),
            "session_id must appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.session_id.as_deref(),
            Some("cf29bfec-69e8-45df-a4f3-3da08ab6f66e"),
            "session_id must round-trip through serde"
        );
    }

    /// A serde-absent `session_id` (state written by a pre-28-02 binary) must
    /// deserialize to `None`, not fail to deserialize.
    #[test]
    fn session_id_absent_from_json_defaults_to_none() {
        let json = r#"{
            "stage": "code",
            "phase": 1,
            "agent": "claude",
            "mode": "auto",
            "started_at": "0",
            "project_root": "/repo"
        }"#;
        let loaded: State = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.session_id, None);
    }

    /// `checkpoint_resumes` round-trips through serde as an exact `u32`
    /// (D-04, 28-02).
    #[test]
    fn checkpoint_resumes_round_trips_through_serde() {
        let mut state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        state.checkpoint_resumes = 2;
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("checkpoint_resumes"),
            "checkpoint_resumes must appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.checkpoint_resumes, 2,
            "checkpoint_resumes must round-trip through serde"
        );
    }

    /// A serde-absent `checkpoint_resumes` (state written by a pre-28-02
    /// binary) must deserialize to `0`, not fail to deserialize.
    #[test]
    fn checkpoint_resumes_absent_from_json_defaults_to_zero() {
        let json = r#"{
            "stage": "code",
            "phase": 1,
            "agent": "claude",
            "mode": "auto",
            "started_at": "0",
            "project_root": "/repo"
        }"#;
        let loaded: State = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.checkpoint_resumes, 0);
    }

    /// 23-09 Task 1: `yes_ship` round-trips through serde as an exact `bool`
    /// — its own key appears in the persisted JSON, and a fresh deserialize
    /// recovers the value set, mirroring the `monitor_pid` pair above.
    #[test]
    fn yes_ship_round_trips_through_serde() {
        let mut state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        state.yes_ship = true;
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("yes_ship"),
            "yes_ship must appear in persisted JSON"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert!(loaded.yes_ship, "yes_ship must round-trip through serde");
    }

    /// A serde-absent `yes_ship` (state written by a pre-23-09 binary) must
    /// deserialize to `false`, not fail to deserialize — the same
    /// backward-compat pattern as every other `#[serde(default)]` field
    /// added since 17-01.
    #[test]
    fn yes_ship_absent_from_json_defaults_to_false() {
        let json = r#"{
            "stage": "code",
            "phase": 1,
            "agent": "claude",
            "mode": "auto",
            "started_at": "0",
            "project_root": "/repo"
        }"#;
        let loaded: State = serde_json::from_str(json).unwrap();
        assert!(!loaded.yes_ship);
    }

    /// 20c: `stop_until`/`stopped`/`stop_reason` all round-trip through
    /// serde — each field's own key appears in the persisted JSON, and a
    /// fresh deserialize recovers the exact values set.
    #[test]
    fn stop_fields_round_trip_through_serde() {
        let mut state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            PathBuf::from("/repo"),
        );
        state.stop_until = Some(Stage::Plan);
        state.stopped = true;
        state.stop_reason = Some("stopped after plan completed (--until plan)".to_string());
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("stop_until") && json.contains("stopped") && json.contains("stop_reason"),
            "all three stop fields must appear in persisted JSON: {json}"
        );
        let loaded: State = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.stop_until,
            Some(Stage::Plan),
            "stop_until must round-trip through serde"
        );
        assert!(loaded.stopped, "stopped must round-trip through serde");
        assert_eq!(
            loaded.stop_reason.as_deref(),
            Some("stopped after plan completed (--until plan)"),
            "stop_reason must round-trip through serde"
        );
    }

    /// A serde-absent `stop_until`/`stopped`/`stop_reason` (state written by
    /// a pre-20c binary) must default to `None`/`false`/`None`, not fail to
    /// deserialize — the same backward-compat pattern as every other
    /// `#[serde(default)]` field added since 17-01.
    #[test]
    fn stop_fields_absent_from_json_default() {
        let json = r#"{
            "stage": "code",
            "phase": 1,
            "agent": "claude",
            "mode": "auto",
            "started_at": "0",
            "project_root": "/repo"
        }"#;
        let loaded: State = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.stop_until, None);
        assert!(!loaded.stopped);
        assert_eq!(loaded.stop_reason, None);
    }
}
