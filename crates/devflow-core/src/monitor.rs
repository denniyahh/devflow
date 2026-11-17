//! Stage monitor: how the foreground runner owns a stage's coding agent.
//!
//! [`run_stage`] launches the agent in its own process group, captures its
//! stdout, stderr and exit code into `.devflow/`, drives the stream shape's
//! pipes (first turn, close rule, idle timeout), and returns once the agent has
//! exited, under the runner's [`StopToken`](crate::stop::StopToken).

use crate::agent_result::{
    CaptureDenial, IdleTimeoutCause, IdleTimeoutCommit, IdleTimeoutRecord, IdleTimeoutRecordOnDisk,
};
use crate::git::hermetic_command;
use crate::phase_id::PhaseId;
use crate::stage::Stage;
use crate::state::AgentKind;
use crate::stop::AgentOutcome;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError, TryLockError, mpsc};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Errors produced by monitor operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MonitorError {
    /// Spawning the monitor process failed.
    #[error("failed to spawn monitor: {0}")]
    Io(#[from] std::io::Error),
    /// Project path is not valid UTF-8.
    #[error("project path is not valid UTF-8")]
    NonUtf8Path,
    /// Could not determine the current executable path.
    #[error("could not determine devflow binary path")]
    NoBinaryPath,
    /// A child spawned with piped stdio did not expose one of its pipes.
    #[error("supervised child exposed no {0} pipe")]
    NoChildPipe(&'static str),
    /// The agent did not lead its own process group, so a group signal
    /// could reach a process that is not the agent's (R-3). The agent was
    /// killed and reaped.
    #[error("agent {pid} is not its own process-group leader (pgid {pgid})")]
    PgidMismatch {
        /// The agent's pid.
        pid: u32,
        /// The process group the agent was found in (`-1` when unreadable).
        pgid: i32,
    },
    /// A supervision shape that this build cannot run yet.
    #[error("supervision shape not ready: {0}")]
    NotReady(String),
}

/// Idle-timeout default in seconds (D-02): the measured constraint-8 floor.
///
/// Plan 31-02 supplies the configurable-and-clamped reader that can only raise
/// this.
///
/// Raised 30s -> 120s on 2026-08-03 by direct measurement; see
/// [`IDLE_TIMEOUT_FLOOR_SECS`] for the trials and the reasoning. The previous
/// value's "~4.2x margin" was computed against a workload that never entered a
/// long foreground tool call, and did not transfer to one.
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 120;

/// The floor an idle timeout can never be configured below (D-02/D-04, 31-02).
///
/// **Raised 30s -> 120s on 2026-08-03, and the reasoning that set 30s was
/// wrong — read this before touching it again.**
///
/// The original ≥30s floor cited "~4.2x margin" against an every-line signal
/// whose observed max was 7.09s. Both numbers were real; the inference was not.
/// Phase 30d measured *backgrounded* 10s/22s sleeps, where the agent is never
/// sitting inside a long foreground tool call. Under one, the CLI emits
/// `tool_progress` keepalives on a **fixed 30.00s interval**, so a healthy,
/// hard-working child produces a 30.00s gap between stream lines — dead level
/// with a 30s timeout, and on the wrong side of it, since the timer starts when
/// the previous line is *processed* while the keepalive arrives 30s after it
/// was *sent*, plus pipe latency.
///
/// Measured 2026-08-03, CLI 2.1.220, five workload-controlled trials across two
/// unrelated workload types (each verified to have actually run — elapsed >=
/// the workload duration, no `tool_use_error`), plus a negative control:
///
/// | workload                  | gaps > 5s              |
/// |---------------------------|------------------------|
/// | 90s busy loop x3          | ~26.4, **30.00**, ~30.0 |
/// | `cargo test --workspace` x2 | ~26.4, **30.00**, ~16  |
/// | control (no long call)    | max 2.2                |
///
/// Variance across all five: ±0.02s. `cargo test --workspace` is not a contrived
/// case — it sits inside DevFlow's own post-merge gate, so the old floor would
/// have killed healthy Code stages on the common path.
///
/// 120s is 4x the measured cadence: it survives **three** consecutive missed
/// keepalives. That headroom is the point — the hazard is not a slightly larger
/// gap but a *dropped* keepalive, which doubles the interval outright. 90s
/// (two missed) is the lowest defensible value; do not go below it.
///
/// Do NOT lower it, and note that no configuration can. Phase 30d measured a
/// 12-second bound killing a LIVE, HEALTHY run in 2 of 7 trials.
///
/// **What the five trials do not establish:** one machine, idle, one CLI
/// version, two workload types. They show the 30.00s cadence is real and
/// reproducible; they do not prove the interval is fixed across load, hardware,
/// or CLI versions. That is precisely why this floor sits well above the
/// observed maximum rather than near it.
///
/// Because the default IS the floor, the value can only ever be raised.
pub const IDLE_TIMEOUT_FLOOR_SECS: u64 = 120;

/// How many consecutive idle windows the monitor will wait through while a
/// background task is known to be outstanding, before treating the silence as
/// a hang after all.
///
/// At the 120s default this is a 20-minute ceiling that applies ONLY when the
/// stream has told us work is in flight; a stage with no open task is still
/// judged on the first window. The bound exists because an open task is not
/// proof of progress — a wedged subagent never reports a terminal status, and
/// an unbounded wait would turn a false kill into an immortal run.
pub const MAX_IDLE_EXTENSIONS_WITH_TASKS_OPEN: u32 = 10;

/// The environment variable that raises the idle timeout above its floor.
pub const IDLE_TIMEOUT_ENV: &str = "DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS";

/// How [`parse_idle_timeout_secs`] arrived at the timeout now in force.
///
/// A distinct enum rather than the plain `clamped: bool` the plan sketched:
/// there are FOUR distinguishable resolutions, not two, and the loud operator
/// notice needs to name the value that was configured — which a bool cannot
/// carry. `ValidateOutcome` in `pipeline_outcomes.rs` makes the same argument
/// for the same reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdleTimeoutResolution {
    /// Nothing was configured; the default — which is the floor — is in force.
    Default,
    /// A configured value at or above the floor is in force verbatim.
    Configured,
    /// A configured value BELOW the floor was raised to it (D-04).
    Clamped {
        /// What the operator asked for, for the notice to name.
        configured: u64,
    },
    /// A value was set but could not be parsed; the default is in force.
    ///
    /// Loud for the same reason the clamp is. An operator who meant `600` and
    /// typed `60O` silently gets the 120s default, and a legitimately slow stage then dies
    /// on a timeout nobody chose. `parse_gate_max_unattended_age` substitutes
    /// silently in this case and is the anti-pattern here, not the precedent.
    Unparseable {
        /// The raw value, echoed back so the typo is visible.
        raw: String,
    },
}

/// A resolved idle timeout together with how it was arrived at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdleTimeoutSetting {
    /// The window that must elapse with NO line on the child's stdout.
    pub timeout: Duration,
    /// How that value was reached — observable to the caller as a VALUE, not
    /// only as a log line, so a test can assert on it directly.
    pub resolution: IdleTimeoutResolution,
}

impl IdleTimeoutSetting {
    /// Whether the floor clamp engaged.
    pub fn clamped(&self) -> bool {
        matches!(self.resolution, IdleTimeoutResolution::Clamped { .. })
    }

    /// The loud, operator-facing notice this resolution owes, if any.
    ///
    /// `None` for the two unremarkable cases. `Some` exactly when a value the
    /// operator supplied is NOT the value in force — the case that must never
    /// pass silently.
    pub fn notice(&self) -> Option<String> {
        self.notice_for(IDLE_TIMEOUT_ENV)
    }

    /// [`Self::notice`] named for the SPECIFIC environment variable that
    /// produced this resolution.
    ///
    /// The per-agent idle policy (D-08, round 3) resolves different variables
    /// per agent; the notice must name the knob the operator actually set, or
    /// a clamped/typo'd `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS` would be
    /// reported as if `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` were the culprit.
    pub fn notice_for(&self, env: &str) -> Option<String> {
        match &self.resolution {
            IdleTimeoutResolution::Default | IdleTimeoutResolution::Configured => None,
            IdleTimeoutResolution::Clamped { configured } => Some(format!(
                "{env}={configured} is below the {IDLE_TIMEOUT_FLOOR_SECS}s floor \
                 and was CLAMPED; {}s is in force. A shorter window kills healthy runs: a 12s \
                 bound terminated a live, healthy run in 2 of 7 measured trials.",
                self.timeout.as_secs()
            )),
            IdleTimeoutResolution::Unparseable { raw } => Some(format!(
                "{env}={raw:?} could not be parsed as a whole number of seconds; \
                 the {}s default is in force. If you meant to RAISE the timeout, this did not \
                 do it.",
                self.timeout.as_secs()
            )),
        }
    }
}

/// Resolve a raw idle-timeout override into the value actually in force.
///
/// Pure — no environment access — so it is unit-testable directly rather than
/// by mutating process-global env. That shape is copied from
/// `devflow-cli`'s four `parse_*` timeout readers; their BEHAVIOUR is
/// deliberately not copied, because none of them clamps against a floor and
/// none logs when a fallback engages. There is no clamp-and-log precedent
/// anywhere in this workspace; this is the first (D-04).
pub fn parse_idle_timeout_secs(raw: Option<String>) -> IdleTimeoutSetting {
    let floor = Duration::from_secs(IDLE_TIMEOUT_FLOOR_SECS);

    // An unset variable and an EMPTY one are the same intent: nothing chosen.
    // Only a non-empty value that fails to parse is a typo worth shouting at.
    let Some(trimmed) = raw.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return IdleTimeoutSetting {
            timeout: floor,
            resolution: IdleTimeoutResolution::Default,
        };
    };

    let Ok(configured) = trimmed.parse::<u64>() else {
        return IdleTimeoutSetting {
            timeout: floor,
            resolution: IdleTimeoutResolution::Unparseable {
                raw: trimmed.to_string(),
            },
        };
    };

    if configured < IDLE_TIMEOUT_FLOOR_SECS {
        IdleTimeoutSetting {
            timeout: floor,
            resolution: IdleTimeoutResolution::Clamped { configured },
        }
    } else {
        IdleTimeoutSetting {
            timeout: Duration::from_secs(configured),
            resolution: IdleTimeoutResolution::Configured,
        }
    }
}

/// The AGENT-SPECIFIC idle-timeout resolution (round-3 D-08, B3).
///
/// The 120s floor was measured against Claude's stream cadence; applying it
/// to an unmeasured agent would be a behaviour prediction. The decision is
/// therefore per-agent and explicit, never a silent inheritance:
///
/// - Claude reads `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` — byte-identical to the
///   pre-phase-41 behaviour this replaces.
/// - Antigravity reads `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS` with the same
///   120s floor as a DECIDED starting point (documented in the variable's
///   OPERATIONS.md row), to be revisited after the first real cadence
///   measurement.
///
/// Both literals are spelled out here so `doc_check` keeps every variable
/// visible to the operator-doc parity gate (same rule as the single-variable
/// wrapper above).
pub fn idle_timeout_setting_for(agent: AgentKind) -> IdleTimeoutSetting {
    let raw = match agent {
        AgentKind::Antigravity => std::env::var("DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS").ok(),
        _ => std::env::var("DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS").ok(),
    };
    parse_idle_timeout_secs(raw)
}

/// Remove `DEVFLOW_GATE_NOTIFY_CMD` from a child's environment (49-REVIEW.md
/// WR-07): the command may carry a credential such as a webhook token, and it
/// never reaches an agent's environment. `NOTIFY_SOCKET` goes too (D-04a).
///
/// Called from `spawn_stage_agent` and `run_stream_canary`.
///
/// `spawn_stage_agent` (the runner's agent, either shape) and
/// `run_stream_canary` (the delivery canary) strip it from the agent they
/// spawn directly. The runner itself keeps the variable for the gates it
/// fires; a gate's entry write reloads the run's state file, so an on-disk edit
/// of `gate_notify_cmd` made after the advance loaded its copy is the command
/// the gate fires (`gate_entry_fires_a_notify_cmd_edited_after_the_advance_loaded`),
/// and an edit made while the gate waits survives its resolution
/// (`gate_resolution_keeps_a_notify_cmd_edited_during_the_wait`).
pub(crate) fn strip_notify_env(child_cmd: &mut std::process::Command) {
    child_cmd.env_remove(crate::gates::GATE_NOTIFY_CMD_ENV);
    // A runner unit's notify socket is systemd's (D-04a): no child is meant to report
    // readiness, and `NotifyAccess=main` ignores one that tries.
    child_cmd.env_remove(NOTIFY_SOCKET_ENV);
}

/// The variable systemd names a `Type=notify` unit's notify socket in.
const NOTIFY_SOCKET_ENV: &str = "NOTIFY_SOCKET";

/// Constraint 4's close rule as a pure, line-fed state machine: stdin may be
/// released only once a `DEVFLOW_RESULT` marker has appeared inside a
/// TOP-LEVEL `result` event **and** the background-task list has drained.
///
/// An `AND` of two arms, neither sufficient alone:
///
/// - **Marker arm.** Satisfied only by
///   [`crate::agent_result::event_is_top_level_result_marker`] — a composition
///   of the existing `is_top_level` predicate and the existing marker parser,
///   never a looser text search. The CLI echoes the operator's prompt back
///   into the same stdout, and DevFlow's own stage prompts discuss
///   `DEVFLOW_RESULT` markers at length, so marker text alone is not evidence
///   (T-31-01; the same echo produced the checkpoint false positive 30-05
///   fixed).
/// - **Drain arm.** Satisfied when no `background_tasks_changed` event has
///   ever announced anything (vacuous — the common single-plan case) or when
///   the most recent one carried an empty list.
///
/// **The drain alone is never a stop signal.** 30c/30d measured the
/// drain-to-final-`result` lag at 4.54–11.51s across 14 trials; closing at the
/// drain would have truncated the final orchestrator turn in all seven 30d
/// trials.
///
/// **Never count `result` events.** Constraint 7: the CLI coalesces
/// completions, so a wave whose children finish together produces one `result`
/// for several of them — a shape superficially indistinguishable from "one
/// child delivered, one lost". The drained list is the only thing separating
/// those two. Per 30-04 the drain arm is *defensive rather than load-bearing*
/// (n=2 Mode B trials delivered everything without it); that is the recorded
/// reason to keep it cheaply, not a reason to drop it.
///
/// **A line that does not parse as JSON is ignored by this rule** — it can
/// neither satisfy nor block either arm — but it is still teed verbatim to the
/// capture file by the reader thread. A torn line therefore cannot silently
/// decide anything, and cannot be silently lost either.
/// The three states a `background_tasks_changed` announcement can leave the
/// close rule in. Kept as a named enum rather than `Option<usize>` (999.75 /
/// DEN-96, fixed 2026-08-04): a plain `Option` cannot distinguish "no
/// announcement has ever arrived" from "an announcement arrived but its
/// `tasks` field was not a readable array", and both used to collapse onto
/// `None`. `should_close()` treats `None` as permission to close — so an
/// unparseable *first* announcement closed stdin exactly when a background
/// task was actually pending, which is the 999.64 orphan shape reachable
/// through the guard built to prevent it.
#[derive(Default, PartialEq, Eq, Debug, Clone, Copy)]
enum BackgroundTaskState {
    /// No `background_tasks_changed` event has been observed at all. Vacuously
    /// drained: a stage that never backgrounds anything must still be able to
    /// close on its marker, or every non-backgrounding stage would hang for
    /// the full idle timeout.
    #[default]
    NeverAnnounced,
    /// The last announcement carried a readable `tasks` array of this length.
    /// `Pending(0)` is a real drain; `Pending(n>0)` blocks closing.
    Pending(usize),
    /// An announcement arrived — `type: "system"`,
    /// `subtype: "background_tasks_changed"` — but its `tasks` field was not a
    /// readable JSON array. Distinct from `NeverAnnounced` specifically so it
    /// does NOT satisfy `should_close()`: the CLI said tasks might exist and
    /// this rule could not read the count, so the safe assumption is that
    /// something is still pending, not that nothing ever was.
    Unreadable,
}

/// The task statuses that end a background task's life.
///
/// Deliberately an allow-list of TERMINAL states rather than a deny-list of
/// active ones. An unrecognised status leaves the task OPEN, which blocks
/// closing and extends the idle window — the same conservative direction
/// [`BackgroundTaskState::Unreadable`] already takes. Being wrong here delays
/// a stage; being wrong the other way orphans its work (999.64).
const TERMINAL_TASK_STATUSES: &[&str] = &[
    "completed",
    "killed",
    "failed",
    "stopped",
    "cancelled",
    "canceled",
    "error",
];

pub struct CloseRule {
    marker_seen: bool,
    background_tasks: BackgroundTaskState,
    /// Task ids announced via the per-task event vocabulary and not yet seen
    /// to reach a terminal status.
    ///
    /// **Why this exists alongside [`Self::background_tasks`].** The drain arm
    /// above reads `background_tasks_changed`, and production does not emit
    /// that event *for sub-agent dispatch*. Measured on a real Phase 35.1 Plan
    /// capture (2026-08-08) — a sub-agent (researcher) dispatch — `task_started`
    /// ×1, `task_progress` ×61, `task_notification` ×1, `task_updated` ×1, and
    /// `background_tasks_changed` ×0. Every occurrence of `background_tasks_changed`
    /// in this repository's *source* is a test fixture synthesising it, which is
    /// why the blindness never surfaced in the suite. This is 999.83's "the
    /// fixture's shape doesn't match what production actually emits", with the
    /// capture to prove it.
    ///
    /// Phase 35.3's measurement (HARDEN-06, 2026-08-12, CLI 2.1.228) later refined
    /// that: the current CLI DOES emit `background_tasks_changed` — but only for
    /// backgrounded shells, with `task_type: "local_bash"` — while sub-agent
    /// dispatch emits only the per-task vocabulary. So the drain arm is not dead;
    /// it is scoped to the backgrounded-shell path, and `open_tasks` is what
    /// covers the sub-agent path.
    ///
    /// The two signals are ANDed, never substituted: whichever one says work
    /// is pending wins.
    open_tasks: std::collections::HashSet<String>,
    /// Which event shape carries the `DEVFLOW_RESULT` marker this rule closes
    /// on — the AGENT-AWARE half of the close rule (round-3 B1).
    ///
    /// Claude emits `type: "result"` with a STRING `result` field;
    /// Antigravity emits `event: "result"` with `result.response`. The
    /// predicate is selected at construction via [`CloseRule::for_agent`];
    /// [`Default`] keeps the Claude predicate so every pre-existing
    /// construction site is unchanged.
    marker_predicate: fn(&serde_json::Value) -> bool,
}

impl Default for CloseRule {
    fn default() -> Self {
        Self::for_agent(AgentKind::Claude)
    }
}

impl CloseRule {
    /// The close rule for a specific agent.
    ///
    /// Selects the marker predicate by agent: Claude keeps
    /// [`crate::agent_result::event_is_top_level_result_marker`]; Antigravity
    /// uses [`crate::agent_result::event_is_top_level_antigravity_result_marker`]
    /// (round-3 B1). Without the agent-aware predicate, an Antigravity stream's
    /// `event: "result"` object never sets `marker_seen`, stdin is never
    /// released, and every real stage idle-times-out before its capture is read.
    ///
    /// **Vacuously-satisfied drain arms (Antigravity, B1).** The
    /// `background_tasks` / `open_tasks` arms read `type: "system"` subtypes
    /// (`background_tasks_changed`, `task_started`, `task_notification`, ...)
    /// that the Antigravity CLI never emits — its event-key schema has no
    /// `type` field at all. An Antigravity rule therefore stays at
    /// `NeverAnnounced` / empty forever and [`CloseRule::should_close`] reduces
    /// to the marker predicate. This is STATED, not silently inherited: it is a
    /// documented property of the Antigravity transport, asserted by the
    /// close-rule tests (close_rule_antigravity_*).
    pub fn for_agent(agent: AgentKind) -> Self {
        let marker_predicate = match agent {
            AgentKind::Antigravity => {
                crate::agent_result::event_is_top_level_antigravity_result_marker
            }
            _ => crate::agent_result::event_is_top_level_result_marker,
        };
        Self {
            marker_seen: false,
            background_tasks: BackgroundTaskState::NeverAnnounced,
            open_tasks: std::collections::HashSet::new(),
            marker_predicate,
        }
    }

    /// Fold one raw stdout line into the rule.
    pub fn observe(&mut self, line: &str) {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            return;
        };
        if (self.marker_predicate)(&event) {
            self.marker_seen = true;
        }
        if event.get("type").and_then(serde_json::Value::as_str) == Some("system")
            && event.get("subtype").and_then(serde_json::Value::as_str)
                == Some("background_tasks_changed")
        {
            self.background_tasks = match event.get("tasks").and_then(serde_json::Value::as_array) {
                Some(tasks) => BackgroundTaskState::Pending(tasks.len()),
                // The announcement exists but its `tasks` field could not be
                // read as an array. Distinct from "never announced" — see the
                // enum doc comment. This is the fix: previously this arm did
                // nothing, leaving `pending_background_tasks` at its prior
                // value, which on the FIRST announcement was `None` —
                // indistinguishable from vacuous drain, and so treated as
                // permission to close exactly when it should not have been.
                None => BackgroundTaskState::Unreadable,
            };
        }

        self.observe_task_event(&event);
    }

    /// Fold the per-task event vocabulary the CLI actually emits into
    /// [`Self::open_tasks`].
    ///
    /// `task_started` and `task_progress` both open a task — progress is
    /// treated as opening, not merely as a heartbeat, so a capture joined
    /// mid-flight (a resumed monitor, a rotated capture) still learns that
    /// work is outstanding instead of concluding the stage is quiet.
    fn observe_task_event(&mut self, event: &serde_json::Value) {
        if event.get("type").and_then(serde_json::Value::as_str) != Some("system") {
            return;
        }
        let Some(subtype) = event.get("subtype").and_then(serde_json::Value::as_str) else {
            return;
        };
        let Some(task_id) = event.get("task_id").and_then(serde_json::Value::as_str) else {
            return;
        };

        match subtype {
            "task_started" | "task_progress" => {
                self.open_tasks.insert(task_id.to_string());
            }
            "task_notification" | "task_updated" => {
                // `task_notification` carries `status` at the top level;
                // `task_updated` carries it inside `patch`. Read both rather
                // than assuming one shape.
                let status = event
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| {
                        event
                            .get("patch")
                            .and_then(|patch| patch.get("status"))
                            .and_then(serde_json::Value::as_str)
                    });
                match status {
                    Some(status) if TERMINAL_TASK_STATUSES.contains(&status) => {
                        self.open_tasks.remove(task_id);
                    }
                    // A status we do not recognise, or none at all, leaves the
                    // task open. See TERMINAL_TASK_STATUSES.
                    _ => {
                        self.open_tasks.insert(task_id.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    /// Whether any background task is known to be outstanding.
    ///
    /// The idle-timeout arm consults this: a parent that is correctly quiet
    /// while a subagent works is not idle, and killing it is the defect this
    /// predicate exists to prevent.
    #[must_use]
    pub fn has_open_background_tasks(&self) -> bool {
        !self.open_tasks.is_empty()
            || matches!(
                self.background_tasks,
                BackgroundTaskState::Pending(1..) | BackgroundTaskState::Unreadable
            )
    }

    /// Whether both arms hold and the child's stdin may be released.
    pub fn should_close(&self) -> bool {
        self.marker_seen
            && matches!(
                self.background_tasks,
                BackgroundTaskState::NeverAnnounced | BackgroundTaskState::Pending(0)
            )
            && self.open_tasks.is_empty()
    }
}

/// The single place the stdin wire shape is constructed: one line of JSON
/// carrying the initial user turn for a `--input-format stream-json` child.
///
/// Shape (`{"type":"user","message":{"role":"user","content":<prompt>}}`) is
/// reproduced from the three archived Phase 30 harnesses, which all wrote
/// exactly this and got a working turn back.
///
/// Built with `serde_json` rather than `format!` so the prompt is ESCAPED, not
/// interpolated. A stage prompt is arbitrary text containing quotes, newlines
/// and backslashes; interpolating it would produce a torn JSON line the CLI
/// rejects, and a prompt could then alter the surrounding document's structure.
pub fn user_turn_line(prompt: &str) -> String {
    serde_json::json!({
        "type": "user",
        "message": { "role": "user", "content": prompt },
    })
    .to_string()
}

/// The AGENT-AWARE first-turn wire shape for a `--input-format stream-json`
/// child (round-3 D-02, antigravity notice (b)).
///
/// Antigravity's CLI rejects Claude's `{"type":"user",...}` turn — the round-2
/// live capture records `stream input message is missing the "event" field`
/// for exactly that shape — so the Antigravity turn is
/// `{"event":"user","message":{...}}` (the `event`-key schema, matching every
/// other event the CLI emits). Every other agent keeps today's
/// [`user_turn_line`] shape; the Claude path is byte-identical.
pub fn user_turn_line_for(agent: AgentKind, prompt: &str) -> String {
    match agent {
        AgentKind::Antigravity => serde_json::json!({
            "event": "user",
            "message": { "role": "user", "content": prompt },
        })
        .to_string(),
        _ => user_turn_line(prompt),
    }
}

/// The capture file the stream shape's reader tees into, shared with
/// [`StreamThreads`] so the bound in [`StreamThreads::join_within`] can take
/// it from a reader still blocked on a held stdout. `None` once taken: the
/// reader then discards what it reads and keeps reading to EOF.
type SharedCapture = Arc<Mutex<Option<std::fs::File>>>;

/// How long [`run_stage`] waits for a stream stage's threads once
/// the agent has exited (WR-03). The agent's own output is already in the
/// pipe by then and drains in milliseconds; a thread still running at the
/// bound is blocked on a pipe end that a process the agent started still
/// holds, which no wait would end. Waiting on it without a bound kept a
/// completed or stopped stage from returning forever.
const READER_DRAIN_WAIT: Duration = crate::agent::TERMINATE_VERIFY_WAIT;

/// The stream shape's writer, reader and exit-watch threads, and the capture
/// they share, waited for once the agent has been reaped.
struct StreamThreads {
    writer: std::thread::JoinHandle<()>,
    reader: std::thread::JoinHandle<()>,
    exit_watch: std::thread::JoinHandle<()>,
    capture: SharedCapture,
}

/// What [`StreamThreads::join_within`] left running at its bound.
#[derive(Debug)]
struct UnjoinedStreamThreads {
    /// The detached threads: `writer`, `reader` or `exit_watch`.
    unfinished: Vec<&'static str>,
    /// How long the threads were waited for.
    waited: Duration,
    /// Whether the capture file was taken from the reader (and closed), so
    /// nothing more reaches it.
    capture_closed: bool,
}

impl StreamThreads {
    /// Wait at most `wait` for all three threads and join each that
    /// finished. `None` when all three were joined. Otherwise take the
    /// capture from the reader (`try_lock`, retried for at most one more
    /// `wait`, so a capture write blocked past both waits leaves it in
    /// place), detach every unfinished thread by dropping its handle, and
    /// report what was left. Nothing is signalled: a detached reader keeps
    /// the pipe's read end open and drained until this process exits.
    fn join_within(self, wait: Duration) -> Option<UnjoinedStreamThreads> {
        let started = std::time::Instant::now();
        let all_finished = |threads: &Self| {
            threads.writer.is_finished()
                && threads.reader.is_finished()
                && threads.exit_watch.is_finished()
        };
        while !all_finished(&self) && started.elapsed() < wait {
            std::thread::sleep(crate::agent::TERMINATE_VERIFY_POLL);
        }
        let waited = started.elapsed();
        let Self {
            writer,
            reader,
            exit_watch,
            capture,
        } = self;
        let mut unfinished = Vec::new();
        for (name, handle) in [
            ("writer", writer),
            ("reader", reader),
            ("exit_watch", exit_watch),
        ] {
            if handle.is_finished() {
                let _ = handle.join();
            } else {
                // Dropping the handle detaches the thread.
                unfinished.push(name);
            }
        }
        if unfinished.is_empty() {
            return None;
        }
        Some(UnjoinedStreamThreads {
            unfinished,
            waited,
            capture_closed: close_shared_capture(&capture, wait),
        })
    }
}

/// Take the capture from the reader and close it: `try_lock`, retried every
/// [`crate::agent::TERMINATE_VERIFY_POLL`] for at most `wait`. A poisoned
/// lock is recovered, as `runtime.rs` does. `false` when the lock stayed
/// held for the whole wait (a capture write blocked), leaving it in place.
fn close_shared_capture(capture: &SharedCapture, wait: Duration) -> bool {
    let started = std::time::Instant::now();
    loop {
        match capture.try_lock() {
            Ok(mut slot) => {
                *slot = None;
                return true;
            }
            Err(TryLockError::Poisoned(poisoned)) => {
                *poisoned.into_inner() = None;
                return true;
            }
            Err(TryLockError::WouldBlock) => {}
        }
        if started.elapsed() >= wait {
            return false;
        }
        std::thread::sleep(crate::agent::TERMINATE_VERIFY_POLL);
    }
}

/// Wait at most [`READER_DRAIN_WAIT`] for a stream stage's threads, and when
/// one is still running at the bound, log it and record a
/// `stream_wait_bounded` event (`stage`, `waited_ms`, `unfinished`,
/// `capture_closed`): the event is the run's on-disk record of the bound
/// (WR-03).
fn finish_stream_threads(
    threads: StreamThreads,
    project_root: &Path,
    phase: PhaseId,
    stage: Option<Stage>,
) {
    let Some(report) = threads.join_within(READER_DRAIN_WAIT) else {
        return;
    };
    warn!(
        "stream threads {:?} still running {:?} after the agent exited: a process the agent \
         started still holds its stdout or stdin; detaching them (capture closed: {})",
        report.unfinished, report.waited, report.capture_closed
    );
    crate::events::emit(
        project_root,
        phase,
        "stream_wait_bounded",
        serde_json::json!({
            "stage": stage.map(|stage| stage.to_string()),
            "waited_ms": u64::try_from(report.waited.as_millis()).unwrap_or(u64::MAX),
            "unfinished": report.unfinished,
            "capture_closed": report.capture_closed,
        }),
    );
}

/// One thing [`drive_stream_io`]'s loop hears about.
enum StreamEvent {
    /// A line the reader read from the agent's stdout (and teed).
    Line(String),
    /// The reader reached EOF (or a read error, treated as EOF).
    Eof,
    /// The agent process exited, seen without reaping it. Its stdout may
    /// still be open in a process it started.
    AgentExited,
}

/// Append `line` to the shared capture and flush, exactly as the reader
/// always teed; discard it once the capture has been taken. The lock is
/// released before the line is forwarded.
fn tee_to_capture(capture: &SharedCapture, line: &str) {
    let mut slot = capture.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(file) = slot.as_mut() {
        if let Err(err) = writeln!(file, "{line}") {
            warn!("could not append to the capture file: {err}");
        }
        let _ = file.flush();
    }
}

/// Drive a `stream-json` child's two pipes until its stream is over.
///
/// Agent-aware transport (round-3): the first turn is written via
/// [`user_turn_line_for`] and the close rule via [`CloseRule::for_agent`], so
/// the write/read/close triple matches whichever CLI is being supervised.
///
/// Threading model (constraint 4 / T-31-04). Three participants:
/// - a **writer thread** owning the child's stdin: it writes the initial user
///   turn, then BLOCKS on a channel rather than returning. It drops stdin only
///   when told to, because constraint 4's `AND` can never be honoured if stdin
///   is already gone — a task-notification turn arriving after the child's
///   first turn would have nowhere to be delivered.
/// - a **reader thread** owning the child's stdout: it tees each line verbatim
///   to `capture` (shared, so the caller's bound can take it) and forwards it
///   to this thread, then sends `Eof` at EOF. Once this loop has ended it
///   keeps reading and teeing, so the agent's last buffered lines still reach
///   the capture and the pipe stays drained.
/// - an **exit-watch thread**: it blocks until the agent process exits,
///   without reaping it (`waitid(WNOWAIT)`), then sends `AgentExited` (WR-03).
///   A process the agent started may hold its stdout open after the agent
///   exits, so EOF alone cannot end the stream.
/// - this thread, which applies the close rule and the idle timeout.
///
/// The write and the read MUST be on independent threads. Writing the prompt
/// synchronously before reading stdout is the textbook two-pipe deadlock: it
/// passes every short-prompt smoke test and hangs on exactly the
/// context-heavy production stages that matter (the Linux pipe buffer is
/// commonly 64KiB and a DevFlow stage prompt can exceed that in one write).
///
/// Returns once stdout reaches EOF, the agent process has exited, the idle
/// timeout has fired (after `on_idle_timeout` ran), or the stream stayed
/// quiet after the close rule released stdin. Stdin is released before it
/// returns, so the caller may then wait for the child. Used by
/// [`run_stage`]'s stream shape.
fn drive_stream_io(
    child: &mut std::process::Child,
    capture: std::fs::File,
    agent: AgentKind,
    prompt: &str,
    idle_timeout: Duration,
    on_idle_timeout: impl FnOnce(),
) -> Result<StreamThreads, MonitorError> {
    let mut child_stdin = child
        .stdin
        .take()
        .ok_or(MonitorError::NoChildPipe("stdin"))?;
    let child_stdout = child
        .stdout
        .take()
        .ok_or(MonitorError::NoChildPipe("stdout"))?;

    let (close_tx, close_rx) = mpsc::channel::<()>();
    let turn = user_turn_line_for(agent, prompt);
    let writer = std::thread::spawn(move || {
        let wrote = child_stdin
            .write_all(turn.as_bytes())
            .and_then(|()| child_stdin.write_all(b"\n"))
            .and_then(|()| child_stdin.flush());
        if let Err(err) = wrote {
            warn!("could not write the initial user turn to the child's stdin: {err}");
            return;
        }
        // Deliberately NOT dropping stdin here — see this function's doc.
        // Either signal (an explicit close, or the driver dropping its
        // sender) means the same thing: stop holding the pipe open.
        let _ = close_rx.recv();
        drop(child_stdin);
    });

    let capture: SharedCapture = Arc::new(Mutex::new(Some(capture)));
    let (event_tx, event_rx) = mpsc::channel::<StreamEvent>();
    let reader_tx = event_tx.clone();
    let reader_capture = Arc::clone(&capture);
    let reader = std::thread::spawn(move || {
        // `read_until` + `from_utf8_lossy`, NOT `BufRead::lines()` (peer review
        // 2026-08-03, CRITICAL). `lines()` yields `Err(InvalidData)` on a single
        // non-UTF-8 byte, and the previous code treated any read error as EOF —
        // so one bad byte silently truncated the capture and dropped every later
        // line INCLUDING the terminal `DEVFLOW_RESULT` marker. That is precisely
        // the boundary-truncation class constraint 9 exists for, manufactured by
        // the monitor itself rather than by a dying writer.
        //
        // Decoding is now lossy and NON-fatal: undecodable bytes become U+FFFD
        // and the line still reaches the capture and the close rule. A genuine
        // I/O error still ends the loop, because that one really is EOF.
        let mut reader_buf = BufReader::new(child_stdout);
        let mut raw = Vec::new();
        let mut forwarding = true;
        loop {
            raw.clear();
            match reader_buf.read_until(b'\n', &mut raw) {
                Ok(0) => break, // real EOF
                Ok(_) => {}
                Err(err) => {
                    warn!("stdout read error, treating as EOF: {err}");
                    break;
                }
            }
            while raw.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
                raw.pop();
            }
            let line = String::from_utf8_lossy(&raw).into_owned();
            // Tee VERBATIM before any interpretation: the whole Layer 1
            // cascade reads this file, and a line the close rule ignores
            // (unparseable noise, interleaved prose) must still reach it.
            tee_to_capture(&reader_capture, &line);
            // A failed send means the loop below has ended. Keep reading:
            // the agent's last buffered lines still reach the capture, and
            // the pipe's read end stays open and drained, so a leftover
            // writer is never killed by SIGPIPE while this process lives.
            if forwarding && reader_tx.send(StreamEvent::Line(line)).is_err() {
                forwarding = false;
            }
        }
        let _ = reader_tx.send(StreamEvent::Eof);
    });

    // WR-03: end the stream at the agent's exit, not only at stdout EOF. A
    // process the agent started can hold its stdout open after it exits.
    // `waitid(WNOWAIT)` never reaps, so the caller's reap is untouched; an
    // error (ECHILD once the caller reaped) counts as exited.
    let watch_tx = event_tx;
    let agent_pid = child.id();
    let exit_watch = std::thread::spawn(move || {
        if let Err(err) = crate::stop::wait_for_exit_without_reaping(agent_pid) {
            debug!("exit watch on agent {agent_pid} ended with {err}; treating it as exited");
        }
        let _ = watch_tx.send(StreamEvent::AgentExited);
    });

    // Constraint 4's close rule lives in `CloseRule` so it can be unit-tested
    // by feeding it lines, with no child process per case.
    let mut rule = CloseRule::for_agent(agent);
    let mut close_signalled = false;
    let mut idle_extensions: u32 = 0;

    loop {
        match event_rx.recv_timeout(idle_timeout) {
            Ok(StreamEvent::Line(line)) => {
                if close_signalled {
                    continue;
                }
                rule.observe(&line);
                if rule.should_close() {
                    let _ = close_tx.send(());
                    close_signalled = true;
                }
            }
            Ok(StreamEvent::Eof) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(StreamEvent::AgentExited) => {
                info!(
                    "the agent exited; its stdout may still be open in a process it started, \
                     so the stream ends here instead of at EOF"
                );
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // AFTER a deliberate close, silence is EXPECTED, not a hang
                // (peer review 2026-08-03, CRITICAL). The close rule fires only
                // once the agent has emitted its terminal marker AND background
                // tasks have drained — at which point it has said everything it
                // intends to say and is merely winding down. Firing the idle
                // timeout here wrote an authoritative `IdleTimeout` verdict OVER
                // a completed, successful stage; and because `evaluate_layer1`
                // reads that side channel FIRST, by design, so that nothing can
                // shadow a real timeout, the bogus verdict outranked the real
                // success and could not be recovered from. The mechanism that
                // protects a true timeout is what made a false one fatal.
                //
                // Break instead: the reap path below already bounds a child that
                // will not exit, via `terminate_and_verify`.
                if close_signalled {
                    info!(
                        "no output for {idle_timeout:?} after the close rule released stdin; \
                         the stage already reported — proceeding to reap, NOT recording a timeout"
                    );
                    break;
                }
                // No outer wall-clock bound exists anywhere in this loop, and
                // none may be added (D-03). `recv_timeout` measures the gap
                // since the LAST LINE, so a healthy 47-minute stage that keeps
                // emitting is never touched — every line the reader thread
                // forwards resets the window naturally, which is D-01's
                // every-line signal rather than a milestone-only one. There is
                // no single wall-clock value that is safe for both a hang and
                // a legitimately long stage, which is why constraint 5
                // rejected one.
                //
                // That reasoning assumed healthy ⇒ keeps emitting. A stage
                // that BACKGROUNDS work breaks the assumption: the parent is
                // correctly silent while a subagent runs, and the subagent's
                // `task_progress` heartbeat is bursty — a real Phase 35.1 Plan
                // capture showed organic gaps of 25s, 28s, 42s and 44.5s while
                // the researcher was demonstrably alive, then one gap that
                // reached the 120s floor and got the whole run killed. The
                // drain gate should have covered this and could not: it reads
                // `background_tasks_changed`, which production never emits.
                //
                // So consult the task state before firing. While work is known
                // to be outstanding, silence is expected and this arm extends
                // instead of killing. The extension is BOUNDED — a subagent
                // that wedges leaves its task open forever, so an unbounded
                // wait would trade a false kill for an immortal run, which is
                // the failure this project already knows by name.
                if rule.has_open_background_tasks()
                    && idle_extensions < MAX_IDLE_EXTENSIONS_WITH_TASKS_OPEN
                {
                    idle_extensions += 1;
                    info!(
                        "no output for {idle_timeout:?}, but background work is still \
                         outstanding — extending ({idle_extensions}/\
                         {MAX_IDLE_EXTENSIONS_WITH_TASKS_OPEN}) instead of recording a timeout"
                    );
                    continue;
                }
                if rule.has_open_background_tasks() {
                    warn!(
                        "background work still outstanding after \
                         {MAX_IDLE_EXTENSIONS_WITH_TASKS_OPEN} idle extensions \
                         ({idle_timeout:?} each) — treating as a hang, not as progress"
                    );
                }
                on_idle_timeout();
                break;
            }
        }
    }

    // Guarantee stdin is released before waiting. A child still holding an
    // open stdin may never exit, and `child.wait()` would then block forever.
    drop(close_tx);

    Ok(StreamThreads {
        writer,
        reader,
        exit_watch,
        capture,
    })
}

/// How [`run_stage`] wires the agent's stdio: the two supervision shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StageIo {
    /// The capture shape: stdin is `/dev/null`, stdout and stderr go straight to the phase's
    /// capture and stderr files.
    Capture,
    /// The stream shape: the runner owns both pipes, delivers `prompt` as the first turn on stdin, tees stdout
    /// to the capture file, applies the close rule and fires the idle
    /// timeout after `idle_timeout` of silence.
    Stream {
        /// The stage prompt, delivered on the agent's stdin.
        prompt: String,
        /// How long the stream may be silent before the idle timeout fires.
        idle_timeout: Duration,
    },
}

impl StageIo {
    /// The launch label `stage_launched` and the log carry: `capture` or
    /// `stream`.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Stream { .. } => "stream",
        }
    }
}

/// Everything [`run_stage`] needs to supervise one stage's agent.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StageSpec {
    /// The project root the capture, stderr and exit files live under.
    pub project_root: PathBuf,
    /// The phase the stage runs for.
    pub phase: PhaseId,
    /// The stage the agent runs, when one is known.
    pub stage: Option<Stage>,
    /// The agent's working directory (the phase worktree, or the project
    /// root when worktree mode is off).
    pub workdir: PathBuf,
    /// The agent program.
    pub program: String,
    /// The agent's argv after the program, passed literally.
    pub args: Vec<String>,
    /// The adapter environment, applied after `hermetic_command`'s scrub so
    /// deliberate configuration survives it (process env, never argv).
    pub envs: Vec<(String, String)>,
    /// Which coding agent this is.
    pub agent: AgentKind,
    /// The supervision shape.
    pub io: StageIo,
}

impl StageSpec {
    /// A spec with no stage, no extra argv and no adapter env, whose agent
    /// runs in `project_root`. Set the public fields for the rest.
    pub fn new(
        project_root: PathBuf,
        phase: PhaseId,
        agent: AgentKind,
        program: String,
        io: StageIo,
    ) -> Self {
        Self {
            workdir: project_root.clone(),
            project_root,
            phase,
            stage: None,
            program,
            args: Vec::new(),
            envs: Vec::new(),
            agent,
            io,
        }
    }
}

/// How long [`confirm_own_group`] waits for a just-spawned agent's
/// `setpgid` to become visible before it declares a mismatch.
const PGID_SETTLE_WAIT: Duration = Duration::from_millis(100);

/// Run one stage's agent in this process, under `token`, and return once it has
/// exited: the foreground runner's stage (D-02). The order is the contract:
/// 1. Spawn the agent through `hermetic_command` (the adapter env re-applied after
///    its scrub, the gate notify command stripped) with `process_group(0)`, then
///    require that it leads its own process group (R-3). A mismatch kills and reaps
///    it and returns [`MonitorError::PgidMismatch`].
/// 2. [`StopToken::begin_agent`](crate::stop::StopToken::begin_agent) re-arms the
///    token for the agent (the run enters
///    [`OwnerPhase::AgentRunning`](crate::stop::OwnerPhase::AgentRunning)), and a
///    stop already requested ends the new group at once; the run leaves that phase
///    as the exit decides.
/// 3. The stream shape only: drive the agent's pipes through `drive_stream_io`
///    (first turn, capture tee, close rule, idle timeout), which also ends when the
///    agent process exits, even while a process it started still holds its stdout
///    (WR-03). An idle-timeout group kill is a natural outcome, `Completed(128 + n)`
///    with the idle-timeout verdict file, never a stop.
/// 4. Wait for the agent under `token`, then write the exit file: the agent's code
///    for [`AgentOutcome::Completed`], `143` for [`AgentOutcome::Stopped`]. For the
///    stream shape the stream threads are then waited for at most
///    `READER_DRAIN_WAIT`; at the bound the capture is closed, the unfinished
///    threads are detached (nothing is signalled), and the bound is recorded as a
///    `stream_wait_bounded` event.
///
/// The caller installs the signal handling and owns `token` across stages.
///
/// # Errors
/// The agent could not be spawned, did not lead its own group, or its files could not
/// be written.
pub fn run_stage(
    spec: StageSpec,
    token: &crate::stop::StopToken,
) -> Result<AgentOutcome, MonitorError> {
    let stdout_file = crate::agent_result::stdout_path(&spec.project_root, spec.phase);
    let exit_file = crate::agent_result::exit_code_path(&spec.project_root, spec.phase);
    if let Some(parent) = stdout_file.parent() {
        crate::workflow::ensure_devflow_dir(parent)?;
    }

    let (mut child, stream_capture) = spawn_stage_agent(&spec)?;
    let pid = child.id();

    if let Err(pgid) = confirm_own_group(pid) {
        abandon_agent(&mut child);
        return Err(MonitorError::PgidMismatch { pid, pgid });
    }
    // One token spans the whole run: re-arm it for this stage's agent. A stop that
    // arrived while the stage was launched ends the new group at once.
    if token.begin_agent(pid) {
        crate::stop::end_group_while_running(token, pid);
    }

    let threads = match (&spec.io, stream_capture) {
        (
            StageIo::Stream {
                prompt,
                idle_timeout,
            },
            Some(capture),
        ) => {
            let idle = *idle_timeout;
            let driven = drive_stream_io(&mut child, capture, spec.agent, prompt, idle, || {
                fire_idle_timeout(&spec.project_root, spec.phase, &spec.workdir, pid, idle);
            });
            match driven {
                Ok(threads) => Some(threads),
                Err(err) => {
                    token.abandon(|| abandon_agent(&mut child));
                    return Err(err);
                }
            }
        }
        _ => None,
    };

    let outcome = match crate::stop::wait_owned_agent(token, pid, &mut child) {
        Ok(outcome) => outcome,
        Err(err) => {
            token.abandon(|| abandon_agent(&mut child));
            return Err(err.into());
        }
    };
    let code = match outcome {
        AgentOutcome::Completed(code) => code,
        AgentOutcome::Stopped => crate::stop::STOPPED_EXIT_CODE,
    };
    std::fs::write(&exit_file, format!("{code}\n"))?;
    if let Some(threads) = threads {
        // Test-only (50-26): park here, after the exit file and before the
        // stream threads are waited for. Compiled out of release builds.
        #[cfg(any(test, feature = "test-support"))]
        stream_wait_test_hold(token);
        finish_stream_threads(threads, &spec.project_root, spec.phase, spec.stage);
    }
    info!("{} agent {pid} ended: {outcome:?}", spec.io.label());
    Ok(outcome)
}

/// How long [`stream_wait_test_hold`] waits for its release file: above the
/// bounded waits of the CLI test that uses it, combined.
#[cfg(any(test, feature = "test-support"))]
const STREAM_WAIT_HOLD_WAIT: Duration = Duration::from_secs(60);

/// Test-only hold (50-26). When `DEVFLOW_TEST_STREAM_WAIT_HOLD` names a
/// path: create `<path>.held`, then wait, polling every 20 ms and bounded by
/// [`STREAM_WAIT_HOLD_WAIT`], until `<path>` exists or a stop is requested.
/// [`run_stage`] calls it for the stream shape after the exit file is
/// written and before the stream threads are waited for, so a completed or
/// stopped stage parks there while the agent's stdout is still held, and a
/// stop releases it. Returns at once when the variable
/// is absent. Compiled only under `cfg(test)` or the `test-support` feature,
/// together with its one call; a release build compiles neither (pinned by
/// `monitor::tests::the_stream_wait_hold_is_test_only_and_read_literally`).
#[cfg(any(test, feature = "test-support"))]
fn stream_wait_test_hold(token: &crate::stop::StopToken) {
    let Some(release) = std::env::var_os("DEVFLOW_TEST_STREAM_WAIT_HOLD") else {
        return;
    };
    let release = PathBuf::from(release);
    let mut held = release.clone().into_os_string();
    held.push(".held");
    let _ = std::fs::write(&held, "");
    let deadline = std::time::Instant::now() + STREAM_WAIT_HOLD_WAIT;
    while !release.exists() && !token.stop_requested() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Spawn `spec`'s agent in its own process group (`process_group(0)`, R-3),
/// through `hermetic_command` with the adapter env re-applied after its
/// scrub and the gate notify command stripped. Returns the child and, for
/// the stream shape, the capture file the reader thread tees into.
///
/// The capture and stderr files truncate at open, as the Legacy script's
/// `>` and `2>` do, and stay separate so stderr never corrupts the parsed
/// stdout capture. The capture shape hands the capture file to the agent as
/// its stdout, with stdin `/dev/null`; the stream shape pipes both.
fn spawn_stage_agent(
    spec: &StageSpec,
) -> Result<(std::process::Child, Option<std::fs::File>), MonitorError> {
    let capture = std::fs::File::create(crate::agent_result::stdout_path(
        &spec.project_root,
        spec.phase,
    ))?;
    let stderr = std::fs::File::create(crate::agent_result::stderr_path(
        &spec.project_root,
        spec.phase,
    ))?;
    let mut agent_cmd = hermetic_command(&spec.program, &spec.workdir);
    strip_notify_env(&mut agent_cmd);
    agent_cmd
        .args(&spec.args)
        .envs(spec.envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stderr(Stdio::from(stderr))
        .process_group(0);
    let stream_capture = match spec.io {
        StageIo::Capture => {
            agent_cmd.stdin(Stdio::null()).stdout(Stdio::from(capture));
            None
        }
        StageIo::Stream { .. } => {
            agent_cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
            Some(capture)
        }
    };
    Ok((agent_cmd.spawn()?, stream_capture))
}

/// Require that `pid` leads its own process group: `getpgid(pid) == pid`.
/// A just-spawned child's `setpgid` has normally landed by the time `spawn`
/// returns; if it has not, re-read `/proc/<pid>/stat` field 5 (falling back
/// to `getpgid` where `/proc` is unreadable) for at most
/// [`PGID_SETTLE_WAIT`]. Returns the pgid last seen on a mismatch (`-1` when
/// it could not be read).
fn confirm_own_group(pid: u32) -> Result<(), i32> {
    let Ok(signed) = libc::pid_t::try_from(pid) else {
        return Err(-1);
    };
    // SAFETY: getpgid has no memory preconditions; a bad pid returns -1.
    let read_pgid = || proc_stat_pgrp(pid).unwrap_or_else(|| unsafe { libc::getpgid(signed) });
    // SAFETY: as above.
    let mut pgid = unsafe { libc::getpgid(signed) };
    let deadline = std::time::Instant::now() + PGID_SETTLE_WAIT;
    while pgid != signed && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        pgid = read_pgid();
    }
    if pgid == signed { Ok(()) } else { Err(pgid) }
}

/// Field 5 (`pgrp`) of `/proc/<pid>/stat`, or `None` where it cannot be read.
fn proc_stat_pgrp(pid: u32) -> Option<i32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // `comm` (field 2) may hold spaces or parentheses: split after the last
    // `)`. Field 3 is then index 0, so field 5 is index 2.
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(2)?.parse().ok()
}

/// SIGKILL an agent this function's caller spawned (its group, when it leads
/// one) and reap it, on a path that gives up before the stage waits for it.
/// The pid comes from the in-memory `Child`, never from disk.
fn abandon_agent(child: &mut std::process::Child) {
    // SAFETY: getpgrp has no preconditions and cannot fail.
    let own_group = unsafe { libc::getpgrp() };
    if let Ok(signed) = libc::pid_t::try_from(child.id())
        && signed > 1
        && signed != own_group
    {
        // SAFETY: `signed > 1` and not our own group. A child that does not
        // lead a group makes this ESRCH: no group can be numbered by a pid
        // that is still alive as a non-leader.
        unsafe { libc::kill(-signed, libc::SIGKILL) };
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The idle-timeout firing sequence, in the ONE order it may run (D-05).
///
/// 1. Enumerate the commits the agent made.
/// 2. Write the authoritative verdict to its side-channel file, and fsync it.
/// 3. **Only then** terminate the child.
/// 4. Log a loud warning naming the commits.
///
/// Step 3 must not precede step 2, and reversing them is not a stylistic
/// choice. Between "the child is dead" and "an authoritative result exists"
/// there is a window in which the verdict cascade sees a dead process, no
/// Layer-1 answer, and some commits on the branch — and Layer 2 scores exactly
/// that as `Success`. That is 999.64 reborn inside its own fix. A bare kill
/// with no record is the other half of the same failure: exit code 137 reads
/// as `ResourceKilled`, blaming an OOM that never happened.
///
/// **Nothing here rolls back, resets, or reverts a commit** (D-07, T-31-09).
/// The commit log is READ and never written. A timeout can be a false
/// positive, and destroying real work on a false positive is unrecoverable —
/// this repo treats irreversible operations as needing review, not tests.
///
/// Scoped to the stream shape alone: the capture shape keeps today's behaviour, and
/// Codex/OpenCode/Pi keep theirs. The 120-second floor was measured against
/// Claude's stream cadence (a fixed 30.00s `tool_progress` keepalive). The
/// per-agent resolution is explicit, not inherited (round-3 D-08):
/// [`idle_timeout_setting_for`] reads `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` for
/// Claude unchanged and `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS` for
/// Antigravity with the same floor as a DECIDED starting point — revisited
/// after the first real cadence measurement, never a silent application of an
/// unmeasured policy (the thing constraint 1 forbids).
///
/// Every step is best-effort and none can abort the sequence. A failure to
/// enumerate, write, or log must still leave the child terminated and the
/// stage machine advancing to a never-silent gate; the operator loses detail,
/// never the verdict.
fn fire_idle_timeout(
    project_root: &Path,
    phase: PhaseId,
    workdir: &Path,
    child_pid: u32,
    idle: Duration,
) {
    let idle_secs = idle.as_secs();
    warn!("idle timeout: no output from the supervised child for {idle_secs}s");

    // 0. Ask WHY the stream went quiet before recording a verdict about the
    //    silence. A quota denial silences the agent — it has nothing left to
    //    say — and the capture already carries the answer in a form
    //    `detect_claude_stream_rate_limit` classifies as `RateLimited`, which
    //    `outcome_policy` routes to auto-resume. Writing an idle-timeout record
    //    here would bury that: `parse_idle_timeout_side_channel` is
    //    `evaluate_layer1`'s first statement and returns unconditionally
    //    (T-31-06), so the record outranks the better classification sitting in
    //    the same capture, and a resumable pause is reported as "TERMINAL and
    //    not retried automatically".
    //
    //    Observed 2026-08-08 on a real Code stage that hit a `seven_day`
    //    `out_of_credits` denial: the operator was told the stream had been
    //    silent for 120s. Running out of quota is the likeliest way a long
    //    unattended run stops.
    //
    //    The child is still terminated below — it is wedged either way. Only
    //    the VERDICT changes, and it changes by omission: with no record
    //    written, the cascade reaches the rate-limit classifier and returns the
    //    right answer. Never silent — this is logged loudly beside the kill.
    //
    //    D-16 (backlog 999.71): the answer has three values, not two. A capture
    //    that ends in a torn JSON line after its last parsed event is neither
    //    a proven denial (T-30-12: only a parsed top-level `rejected` event is)
    //    nor a clean silence, so the record is written and flagged below. A
    //    torn tail outranks a denial parsed before it (the torn line may have
    //    replaced it), by the same rule the verdict uses
    //    (`agent_result::classify_capture_denial`), so such a capture is never
    //    suppressed here as a rate limit.
    let denial = crate::agent_result::capture_rate_limit_denial(project_root, phase);
    if denial == CaptureDenial::Denied {
        warn!(
            "idle timeout after {idle_secs}s, but the capture carries an explicit quota \
             denial — NOT recording an idle-timeout verdict, so the rate-limit \
             classifier decides and the run stays resumable"
        );
        terminate_child_group(child_pid);
        return;
    }

    // 1. Enumerate. A failure degrades to an empty list plus a note; it never
    //    aborts, because a missing commit list must not cost the verdict.
    let (commits, enumeration_note) = enumerate_phase_commits(project_root, workdir, phase);

    // 1b. Read WHY the agent stopped, once, while the capture is still the
    //     child's last word (D-15, backlog 999.102): `terminal_reason`,
    //     `errors` and `origin.kind` of the last top-level `result` event. No
    //     such event yields no cause, and the reason reads as it always did.
    let cause = crate::agent_result::idle_timeout_cause(project_root, phase);

    // 1c. A torn final line is recorded, never suppressed: the write below is
    //     still what stops Layer 2 scoring partial commits as success. The
    //     flag puts the torn segment at the front of the reason's clause.
    let torn = denial == CaptureDenial::TornAfterLastEvent;
    if torn {
        warn!(
            "idle timeout after {idle_secs}s: the capture ends in a torn JSON line after its \
             last parsed event, so if the agent is Claude a quota denial cannot be ruled out \
             (only Claude's stream is classified for one); recording the timeout with \
             capture_torn_final_line"
        );
    }

    // 2. Write, flush, fsync. This completing is the ONLY thing that stops
    //    Layer 2 from later scoring partial commits as Success.
    let write_error = write_idle_timeout_record(
        project_root,
        phase,
        idle_secs,
        child_pid,
        &commits,
        cause,
        torn,
    )
    .err()
    .map(|err| err.to_string());
    if let Some(err) = &write_error {
        warn!("idle timeout: could not persist the verdict: {err}");
    }

    // 3. Only now is it safe to kill.
    let terminated = terminate_child_group(child_pid);

    // 4. Loud, durable, and readable after the fact.
    let named: Vec<String> = commits
        .iter()
        .map(|commit| {
            let short: String = commit.sha.chars().take(7).collect();
            format!("{short} {}", commit.subject)
        })
        .collect();
    let mut entry = format!(
        "[idle-timeout] no output for {idle_secs}s; terminated agent pid {child_pid} \
         (verified dead: {terminated}). {} commit(s) on the phase branch, NONE rolled back{}{}",
        named.len(),
        if named.is_empty() {
            String::new()
        } else {
            format!(": {}", named.join("; "))
        },
        enumeration_note
            .map(|note| format!(" [commit enumeration degraded: {note}]"))
            .unwrap_or_default(),
    );
    if let Some(err) = write_error {
        entry.push_str(&format!(" [verdict file could not be written: {err}]"));
    }
    warn!("{entry}");
}

/// Enumerate the commits on this phase's feature branch, as
/// `(commits, degradation note)`.
///
/// Same range construction `evaluate_layer2`'s commit COUNT uses
/// (`{base}..{feature_prefix}phase-NN`) — the same question asked with
/// `git log` instead of `rev-list --count`, so the two can never disagree
/// about which commits are the agent's.
///
/// **`project_root` and `workdir` are deliberately distinct (45-01, review
/// round 2).** The git-flow config resolves from `project_root` — the
/// canonical checkout — while the `git log` still runs in `workdir`.
/// Resolving from `workdir` would read a phase worktree, which need not carry
/// a `devflow.toml` that exists only in the main checkout, and would silently
/// fall back to the default trunk. Against a mismatched trunk this range
/// either over- or under-reports the phase's commits, which is exactly the
/// false-evidence shape the never-silent commit gate depends on not producing.
///
/// Never returns an error. Every failure path yields an empty list and a note
/// naming what went wrong: the operator losing the commit NAMES is bad, the
/// operator losing the VERDICT is the failure this whole plan exists to
/// prevent.
fn enumerate_phase_commits(
    project_root: &Path,
    workdir: &Path,
    phase: PhaseId,
) -> (Vec<IdleTimeoutCommit>, Option<String>) {
    // CR-45-06: `git_flow_for_project` re-resolves from AMBIENT
    // configuration, and `DEVFLOW_BASE_BRANCH` lives in the environment of
    // whichever shell ran `devflow start`. A monitor re-launched by
    // `devflow resume` from a clean shell therefore ranged from `develop`
    // against a run based on something else — over- or under-reporting the
    // phase's commits in the record that IS the operator's timeout evidence.
    // Round 2's fix made this resolve from `project_root` rather than
    // `workdir`, which is a different bug and left this one open.
    //
    // The state file is the same source of truth `git_flow_for_run` reads;
    // consulting it here rather than threading a base through the
    // supervisor's `StageSpec` keeps the fix local to the site that needs
    // it. A missing or unreadable state degrades to the
    // ambient resolution — the pre-existing behaviour, never an abort,
    // because losing the verdict is worse than losing the commit names.
    let persisted_base = crate::workflow::load_state(project_root, phase)
        .ok()
        .and_then(|state| state.base_branch);
    let git_flow = crate::config::git_flow_for_run(project_root, persisted_base.as_deref());
    let branch = format!("{}phase-{}", git_flow.feature_prefix, phase.padded());
    let range = format!("{}..{branch}", git_flow.develop);

    let output = match crate::git::git_command(workdir)
        .args(["log", "--format=%H %s", &range])
        .output()
    {
        Ok(output) => output,
        Err(err) => return (Vec::new(), Some(format!("git log could not run: {err}"))),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return (
            Vec::new(),
            Some(format!("git log {range} failed: {stderr}")),
        );
    }

    let commits = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            // `%H %s` — a sha, one space, then the subject, which may itself
            // contain spaces. `split_once` is therefore correct and `split`
            // is not. A subject-less commit still yields an empty subject
            // rather than being dropped.
            let (sha, subject) = line.split_once(' ').unwrap_or((line, ""));
            Some(IdleTimeoutCommit {
                sha: sha.to_string(),
                subject: subject.to_string(),
            })
        })
        .collect();

    (commits, None)
}

/// Write the idle-timeout verdict and get it onto the platter before returning.
///
/// `sync_all` is not decoration: D-05's guarantee is that the result exists
/// before anything can race it, and a buffered write that is still in the page
/// cache when the process is signalled has not achieved that.
///
/// Serializes [`IdleTimeoutRecordOnDisk`], the same type the reader parses, so
/// the cause keys (D-15) cannot drift between the two directions.
fn write_idle_timeout_record(
    project_root: &Path,
    phase: PhaseId,
    idle_secs: u64,
    child_pid: u32,
    commits: &[IdleTimeoutCommit],
    cause: IdleTimeoutCause,
    capture_torn_final_line: bool,
) -> std::io::Result<()> {
    let record = IdleTimeoutRecord {
        status: crate::agent_result::AgentStatus::IdleTimeout
            .as_wire_str()
            .to_string(),
        idle_secs,
        agent_pid: child_pid,
        written_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        commits: commits.to_vec(),
    };
    let mut on_disk = IdleTimeoutRecordOnDisk::new(record, cause);
    on_disk.capture_torn_final_line = capture_torn_final_line;
    let json = serde_json::to_string(&on_disk)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;

    let path = crate::agent_result::idle_timeout_path(project_root, phase);
    if let Some(parent) = path.parent() {
        crate::workflow::ensure_devflow_dir(parent)?;
    }
    let mut file = std::fs::File::create(&path)?;
    file.write_all(json.as_bytes())?;
    file.flush()?;
    file.sync_all()
}

/// Terminate the supervised child's whole process group, returning the
/// VERIFIED fact of whether the leader is dead.
///
/// Acts on `child_pid`, which came from the in-memory `Child` handle — never
/// on the on-disk pid file (T-31-07). That distinction is what makes the
/// negative-pid signal below safe at all: while this monitor still holds the
/// unwaited `Child`, the kernel cannot recycle that pid, so it cannot come to
/// mean some unrelated process between spawn and now. A pid re-read from disk
/// carries no such guarantee.
///
/// Three steps, and the middle one is borrowed whole rather than reimplemented:
///
/// 1. `SIGTERM` to the GROUP. `.process_group(0)` at spawn made the child its
///    own group leader, so its pid IS its pgid and `-pid` reaches its whole
///    subtree — the tool subprocesses a coding agent leaves behind, which a
///    leader-only signal would orphan. It cannot reach this monitor: the
///    monitor stayed in its own inherited group, which is precisely what
///    `.process_group(0)` bought (T-31-05).
/// 2. [`crate::agent::terminate_and_verify`] for the leader — reused, not
///    rewritten. It owns the `SIGTERM` → poll → `SIGKILL` → re-poll
///    escalation and returns a verified liveness fact instead of an
///    assumption. 999.44 measured 15 of 15 orphaned wrappers surviving
///    `SIGTERM`, so the escalation is not optional.
/// 3. `SIGKILL` to the group, sweeping any survivor the leader's own
///    escalation did not cover. Unconditional by design: at this point the run
///    is over, everything in the group is the agent's subtree, and a `kill` to
///    an empty group is a no-op `ESRCH`.
///
/// The `signed > 1` guard is load-bearing twice over. `kill(-1, sig)` signals
/// every process the caller may signal, and `kill(0, sig)` signals the
/// caller's own group — the two catastrophic cases `agent::terminate` already
/// documents, reachable here through the negation rather than through a
/// hostile pid file.
fn terminate_child_group(child_pid: u32) -> bool {
    let Ok(signed) = libc::pid_t::try_from(child_pid) else {
        warn!("idle timeout: child pid {child_pid} does not fit pid_t; not signalling");
        return false;
    };
    if signed <= 1 {
        warn!("idle timeout: refusing to signal group for pid {signed}");
        return false;
    }

    // SAFETY: `signed > 1`, so `-signed < -1` and the two catastrophic
    // targets (`0` = our own group, `-1` = everything) are both excluded.
    unsafe {
        libc::kill(-signed, libc::SIGTERM);
    }

    let dead = crate::agent::terminate_and_verify(
        child_pid,
        crate::agent::TERMINATE_VERIFY_WAIT,
        crate::agent::TERMINATE_VERIFY_POLL,
    );

    // SAFETY: same guard as above.
    unsafe {
        libc::kill(-signed, libc::SIGKILL);
    }

    dead
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::Stage;
    use crate::state::AgentKind;

    /// WR-07 (49-REVIEW.md): the chain's command never carries the variable.
    #[test]
    fn strip_notify_env_removes_the_variable_from_the_child() {
        let mut cmd = std::process::Command::new("true");
        cmd.env(crate::gates::GATE_NOTIFY_CMD_ENV, "touch x");
        strip_notify_env(&mut cmd);
        let entry = cmd
            .get_envs()
            .find(|(key, _)| *key == crate::gates::GATE_NOTIFY_CMD_ENV);
        assert_eq!(
            entry,
            Some((
                std::ffi::OsStr::new(crate::gates::GATE_NOTIFY_CMD_ENV),
                None
            ))
        );
    }

    // ---- 45-01 Task 3: idle-timeout commit evidence resolves its trunk
    //      from the project, not from the phase worktree it runs `git log` in.

    fn commit_range_git(root: &Path, args: &[&str]) {
        let out = crate::test_support::git_command(root)
            .args(args)
            .output()
            .expect("spawn git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A repo with `develop`, a `workspace/example` branch one commit ahead
    /// of it, and `feature/phase-04` one commit ahead of THAT. The middle
    /// commit is the discriminator: it is reachable from the configured base,
    /// so a correctly-scoped range excludes it and a default-trunk range
    /// includes it.
    fn idle_commit_fixture(root: &Path) {
        commit_range_git(root, &["init", "-q", "-b", "develop"]);
        commit_range_git(root, &["config", "user.email", "t@e.st"]);
        commit_range_git(root, &["config", "user.name", "t"]);
        commit_range_git(root, &["config", "commit.gpgsign", "false"]);
        commit_range_git(root, &["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("README.md"), "x").unwrap();
        commit_range_git(root, &["add", "-A"]);
        commit_range_git(root, &["commit", "-q", "-m", "init"]);

        commit_range_git(root, &["checkout", "-q", "-b", "workspace/example"]);
        std::fs::write(root.join("base-only.txt"), "x").unwrap();
        commit_range_git(root, &["add", "-A"]);
        commit_range_git(root, &["commit", "-q", "-m", "base-only commit"]);

        commit_range_git(root, &["checkout", "-q", "-b", "feature/phase-04"]);
        std::fs::write(root.join("agent-work.txt"), "x").unwrap();
        commit_range_git(root, &["add", "-A"]);
        commit_range_git(root, &["commit", "-q", "-m", "agent-work commit"]);
    }

    /// codex (external review, 2026-09-02, CR-45-06): this enumeration read
    /// AMBIENT configuration, and `DEVFLOW_BASE_BRANCH` lives in the
    /// environment of whichever shell ran `devflow start`. A monitor
    /// re-launched by `devflow resume` from a clean shell therefore ranged
    /// from `develop` against a run based elsewhere, and this list IS the
    /// operator's idle-timeout evidence — the false-evidence shape the
    /// never-silent commit gate depends on not producing. Round 2's fix
    /// (resolve from `project_root`, not `workdir`) is a different bug and
    /// left this one open.
    #[test]
    fn enumerate_phase_commits_prefers_the_runs_persisted_base() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        idle_commit_fixture(root);
        // No `devflow.toml` and no environment variable: ambient resolution
        // yields `develop`, which is precisely the resumed-shell case.
        assert!(
            std::env::var("DEVFLOW_BASE_BRANCH")
                .map(|v| v.is_empty())
                .unwrap_or(true),
            "DEVFLOW_BASE_BRANCH is exported — it outranks the ambient default"
        );

        let subjects = |root: &Path| -> Vec<String> {
            let (commits, note) = enumerate_phase_commits(root, root, PhaseId::new(4));
            assert!(note.is_none(), "enumeration must not degrade: {note:?}");
            commits.into_iter().map(|c| c.subject).collect()
        };

        // NEGATIVE CONTROL, asserted FIRST: with no state on disk the
        // enumeration falls back to ambient resolution and over-reports —
        // `base-only commit` belongs to the base, not to the agent. This is
        // the bug, and it pins that the assertion below is measuring the
        // persisted base rather than passing for any reason at all.
        let ambient = subjects(root);
        assert!(
            ambient.iter().any(|s| s == "base-only commit"),
            "without persisted state the range runs from develop and includes base commits: \
             {ambient:?}"
        );

        // The run recorded its base at `start`.
        let mut state = crate::state::State::new(
            PhaseId::new(4),
            crate::state::AgentKind::Claude,
            crate::mode::Mode::Auto,
            root.to_path_buf(),
        );
        state.base_branch = Some("workspace/example".to_string());
        crate::workflow::save_state(&state).expect("save state");

        let persisted = subjects(root);
        assert!(
            persisted.iter().any(|s| s == "agent-work commit"),
            "a commit reachable only from the feature branch must be listed: {persisted:?}"
        );
        assert!(
            !persisted.iter().any(|s| s == "base-only commit"),
            "a commit already on the run's own base must NOT be reported as agent work: \
             {persisted:?}"
        );
    }

    #[test]
    fn enumerate_phase_commits_ranges_from_the_configured_base() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        idle_commit_fixture(root);
        std::fs::write(
            root.join("devflow.toml"),
            "base_branch = \"workspace/example\"\n",
        )
        .unwrap();
        assert!(
            std::env::var("DEVFLOW_BASE_BRANCH")
                .map(|v| v.is_empty())
                .unwrap_or(true),
            "DEVFLOW_BASE_BRANCH is exported — it outranks devflow.toml"
        );

        let (commits, note) = enumerate_phase_commits(root, root, PhaseId::new(4));
        assert!(note.is_none(), "enumeration must not degrade: {note:?}");
        let subjects: Vec<&str> = commits.iter().map(|c| c.subject.as_str()).collect();

        assert!(
            subjects.contains(&"agent-work commit"),
            "a commit reachable only from the feature branch must be listed: {subjects:?}"
        );
        // NEGATIVE CONTROL: a commit present on the configured base must NOT
        // be listed. Without this half the test passes against a range built
        // from the default trunk, which lists both.
        assert!(
            !subjects.contains(&"base-only commit"),
            "a commit already on the configured base must not be listed: {subjects:?}"
        );
    }

    /// Review round 2's monitor-root finding: the config must resolve from
    /// the canonical project root, not from the phase worktree the `git log`
    /// runs in. A phase worktree need not carry a `devflow.toml` that exists
    /// only in the main checkout, and the fallback is silent.
    #[test]
    fn enumerate_phase_commits_resolves_config_from_the_project_root() {
        // The project root: carries the config AND the git history.
        let project = tempfile::tempdir().unwrap();
        let project_root = project.path();
        idle_commit_fixture(project_root);
        std::fs::write(
            project_root.join("devflow.toml"),
            "base_branch = \"workspace/example\"\n",
        )
        .unwrap();
        assert!(
            std::env::var("DEVFLOW_BASE_BRANCH")
                .map(|v| v.is_empty())
                .unwrap_or(true),
            "DEVFLOW_BASE_BRANCH is exported — it outranks devflow.toml"
        );

        // The workdir: a linked worktree of the same repository that carries
        // NO `devflow.toml` — the shape of a phase worktree whose base branch
        // does not track the operator's local config.
        let wt = tempfile::tempdir().unwrap();
        let workdir = wt.path().join("phase-04");
        commit_range_git(
            project_root,
            &[
                "worktree",
                "add",
                "--detach",
                workdir.to_str().unwrap(),
                "feature/phase-04",
            ],
        );
        assert!(
            !workdir.join("devflow.toml").exists(),
            "the worktree must not carry the config, or the test proves nothing"
        );

        let (commits, note) = enumerate_phase_commits(project_root, &workdir, PhaseId::new(4));
        assert!(note.is_none(), "enumeration must not degrade: {note:?}");
        let configured: Vec<&str> = commits.iter().map(|c| c.subject.as_str()).collect();
        assert!(
            !configured.contains(&"base-only commit"),
            "config resolved from the project root must scope the range: {configured:?}"
        );

        // NEGATIVE CONTROL: swap the two arguments so the config lookup hits
        // the worktree instead. Without the swap the test cannot distinguish
        // "resolved from the right root" from "both roots gave the same
        // answer", which is the whole of the concern.
        let (swapped, _) = enumerate_phase_commits(&workdir, project_root, PhaseId::new(4));
        let swapped: Vec<&str> = swapped.iter().map(|c| c.subject.as_str()).collect();
        assert_ne!(
            configured, swapped,
            "resolving config from the worktree must give a DIFFERENT range"
        );
        assert!(
            swapped.contains(&"base-only commit"),
            "the worktree has no config, so the swapped range falls back to the \
             default trunk and over-reports: {swapped:?}"
        );
    }

    // ---- close-rule fixtures ------------------------------------------
    //
    // Key names, nesting and event types are taken from the real archived
    // capture at
    // `.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/raw_output_v3.jsonl`
    // (lines 5, 8, 19, 44 and 54), not invented: `tasks` is an array of
    // objects with `task_id`/`task_type`/`description`, the drained event is
    // the same event with `tasks":[]`, and a coalesced completion carries
    // `origin.kind == "task-notification"` on an ordinary `result`. Volumes
    // and identifiers are generalized; shapes are not.
    //
    // CLI-version pin (999.83 / HARDEN-06, measured 2026-08-12 on Claude
    // 2.1.228): the Phase 30 capture above predates the current CLI. Phase 35.3's
    // drill measured that the current CLI emits `background_tasks_changed` ONLY
    // for backgrounded shells — with `task_type: "local_bash"`, not `local_agent` —
    // while sub-agent dispatch emits the per-task vocabulary (`task_started` with
    // `task_type: "local_agent"`, then `task_updated`/`task_notification`) and zero
    // `background_tasks_changed`. `bg_tasks_line` below therefore reproduces a
    // combination the current CLI never produces: `local_agent` inside
    // `background_tasks_changed` was a Phase 30 behavior. It is kept as the legacy
    // drain shape; the current per-task vocabulary is covered by the `REAL_TASK_*`
    // constants and `observe_task_event`.

    const INIT_LINE: &str = r#"{"type":"system","subtype":"init","cwd":"/tmp/work","session_id":"s-1","tools":["Task","Bash"],"uuid":"u-init"}"#;

    /// A `system`/`background_tasks_changed` event announcing `count` tasks.
    /// `count == 0` is the DRAINED shape (v3 line 44). The `local_agent`
    /// task_type is the Phase 30 shape; the current CLI uses `local_bash` here
    /// (see the CLI-version pin in the block comment above).
    fn bg_tasks_line(count: usize) -> String {
        let tasks: Vec<String> = (0..count)
            .map(|i| {
                format!(
                    r#"{{"task_id":"t{i}","task_type":"local_agent","description":"child {i}"}}"#
                )
            })
            .collect();
        format!(
            r#"{{"type":"system","subtype":"background_tasks_changed","tasks":[{}],"uuid":"u-bg{count}","session_id":"s-1"}}"#,
            tasks.join(",")
        )
    }

    /// A top-level `result` event. `marker` is the `result` field's text —
    /// the agent's own final message, where a `DEVFLOW_RESULT:` line lives.
    fn result_line(marker: &str) -> String {
        format!(
            r#"{{"type":"result","subtype":"success","is_error":false,"num_turns":3,"stop_reason":"end_turn","session_id":"s-1","uuid":"u-res","result":"{marker}"}}"#
        )
    }

    /// The v3 line-54 shape: ONE `result` closing out work that several
    /// children contributed to, tagged with the task-notification origin.
    fn coalesced_result_line(marker: &str) -> String {
        format!(
            r#"{{"type":"result","subtype":"success","is_error":false,"num_turns":2,"stop_reason":"end_turn","origin":{{"kind":"task-notification"}},"session_id":"s-1","uuid":"u-res-coalesced","result":"{marker}"}}"#
        )
    }

    /// Same envelope, forwarded from a subagent rather than authored by the
    /// orchestrator session.
    fn subagent_result_line(marker: &str) -> String {
        result_line(marker).replacen('{', r#"{"parent_tool_use_id":"toolu_child","#, 1)
    }

    /// A success marker as it appears INSIDE a `result` string field — the
    /// quotes are escaped because the field is itself JSON.
    const MARKER: &str = r#"All done.\nDEVFLOW_RESULT: {\"status\":\"success\",\"commits\":3}"#;
    const NO_MARKER: &str = "Acknowledged; nothing to report.";

    fn observe_all(lines: &[String]) -> CloseRule {
        let mut rule = CloseRule::default();
        for line in lines {
            rule.observe(line);
        }
        rule
    }

    /// The per-task events the CLI ACTUALLY emits, copied verbatim (minus
    /// truncated payloads) from a real Phase 35.1 Plan capture on
    /// 2026-08-08. Not synthesised.
    ///
    /// That capture contained `task_started` ×1, `task_progress` ×61,
    /// `task_notification` ×1, `task_updated` ×1 — and `background_tasks_changed`
    /// ×0. The drain arm reads only the last of those, which is why it was
    /// blind in production while every fixture-fed test passed (999.83).
    const REAL_TASK_STARTED: &str = r#"{"type":"system","subtype":"task_started","task_id":"a5c0bae42941134b0","tool_use_id":"toolu_017H7RUmWejPcfm5Dc1whhdi","description":"Research Phase 35.1","subagent_type":"gsd-phase-researcher","task_type":"local_agent"}"#;
    const REAL_TASK_PROGRESS: &str = r#"{"type":"system","subtype":"task_progress","task_id":"a5c0bae42941134b0","tool_use_id":"toolu_017H7RUmWejPcfm5Dc1whhdi","description":"Reading CONTEXT.md","subagent_type":"gsd-phase-researcher"}"#;
    const REAL_TASK_UPDATED_TERMINAL: &str = r#"{"type":"system","subtype":"task_updated","task_id":"a5c0bae42941134b0","patch":{"status":"completed","end_time":1786159637614}}"#;

    /// A marker as it appears in a top-level `result` event.
    const RESULT_WITH_MARKER: &str = r#"{"type":"result","subtype":"success","is_error":false,"result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}"#;

    /// The regression this whole change exists for: a backgrounding stage must
    /// be recognised as busy from the events production really sends.
    #[test]
    fn open_tasks_are_learned_from_the_events_production_actually_emits() {
        let busy = observe_all(&[
            INIT_LINE.to_string(),
            REAL_TASK_STARTED.to_string(),
            REAL_TASK_PROGRESS.to_string(),
            RESULT_WITH_MARKER.to_string(),
        ]);
        assert!(
            busy.has_open_background_tasks(),
            "a started task must read as outstanding — this is what stops the \
             idle-timeout arm from killing a healthy backgrounding stage"
        );
        assert!(
            !busy.should_close(),
            "stdin must stay open while a subagent runs, even once the marker \
             has landed (999.64 orphan shape)"
        );

        let drained = observe_all(&[
            INIT_LINE.to_string(),
            REAL_TASK_STARTED.to_string(),
            REAL_TASK_PROGRESS.to_string(),
            RESULT_WITH_MARKER.to_string(),
            REAL_TASK_UPDATED_TERMINAL.to_string(),
        ]);
        assert!(
            !drained.has_open_background_tasks(),
            "a terminal status must close the task"
        );
        assert!(
            drained.should_close(),
            "marker seen and every task drained — the stage may close"
        );
    }

    /// Negative control on the conservative direction: an unknown status must
    /// not be mistaken for completion.
    #[test]
    fn an_unrecognised_task_status_leaves_the_task_open() {
        let rule = observe_all(&[
            REAL_TASK_STARTED.to_string(),
            RESULT_WITH_MARKER.to_string(),
            r#"{"type":"system","subtype":"task_updated","task_id":"a5c0bae42941134b0","patch":{"status":"reticulating_splines"}}"#.to_string(),
        ]);
        assert!(
            rule.has_open_background_tasks(),
            "an unrecognised status must leave the task open — being wrong this \
             way delays a stage, being wrong the other way orphans its work"
        );
        assert!(!rule.should_close());
    }

    /// Negative control on the other side: the fix must not make ordinary,
    /// non-backgrounding stages hang. Every stage that never dispatches a
    /// subagent has to close exactly as before.
    #[test]
    fn a_stage_that_backgrounds_nothing_is_unaffected() {
        let rule = observe_all(&[INIT_LINE.to_string(), RESULT_WITH_MARKER.to_string()]);
        assert!(
            !rule.has_open_background_tasks(),
            "no task events means no outstanding work"
        );
        assert!(
            rule.should_close(),
            "a non-backgrounding stage must still close on its marker alone"
        );
    }

    /// Constraint 4 is an `AND`, and neither arm is sufficient alone. Both
    /// halves are asserted here because a rule that accidentally became an
    /// `OR` still passes any test that only ever feeds it both.
    #[test]
    fn close_rule_requires_both_marker_and_drained_background_tasks() {
        // Arm A: the drain lands, but no marker ever appears in a top-level
        // result. Closing here truncates the run before its verdict exists.
        // The torn line carrying marker TEXT is the negative control: a line
        // that does not parse as JSON must not be able to satisfy the marker
        // arm through the back door.
        let drained_but_unmarked = observe_all(&[
            INIT_LINE.to_string(),
            bg_tasks_line(1),
            bg_tasks_line(0),
            r#"{"type":"result","result":"DEVFLOW_RESULT: {\"status\":\"succ"#.to_string(),
            "progress: still working".to_string(),
            result_line(NO_MARKER),
        ]);
        assert!(
            !drained_but_unmarked.should_close(),
            "the drain alone must never close stdin: 30c/30d measured the \
             drain-to-final-result lag at 4.54-11.51s across 14 trials, and \
             closing at the drain would have truncated the final orchestrator \
             turn in all seven 30d trials"
        );

        // Arm B: the marker lands while a child is still pending.
        let marked_but_pending =
            observe_all(&[INIT_LINE.to_string(), bg_tasks_line(1), result_line(MARKER)]);
        assert!(
            !marked_but_pending.should_close(),
            "a marker while a background task is still announced must not \
             close stdin — the pending child's task-notification turn would \
             have nowhere to be delivered"
        );
    }

    /// 999.75 / DEN-96, fixed 2026-08-04. The FIRST `background_tasks_changed`
    /// announcement carries an unparseable `tasks` field (`null`, not an
    /// array). Before the fix, an unreadable announcement left the field at
    /// its prior value — which on the first announcement was the same `None`
    /// used for "nothing was ever announced", so `should_close()` treated it
    /// as a vacuous drain and closed stdin with a task genuinely pending. This
    /// is the 999.64 orphan shape, reachable through the guard built to
    /// prevent it.
    #[test]
    fn unreadable_first_announcement_does_not_satisfy_the_drain_arm() {
        let unreadable_first = observe_all(&[
            INIT_LINE.to_string(),
            r#"{"type":"system","subtype":"background_tasks_changed","tasks":null}"#.to_string(),
            result_line(MARKER),
        ]);
        assert!(
            !unreadable_first.should_close(),
            "an unreadable FIRST announcement must not be indistinguishable \
             from never-announced — closing here would release stdin while \
             the CLI has said a task exists whose count could not be read"
        );

        // Negative control: the identical sequence, but with NO announcement
        // at all, must still close — this is the ordinary non-backgrounding
        // stage, and the fix must not regress it into hanging for the idle
        // timeout on every run that never backgrounds anything.
        let never_announced = observe_all(&[INIT_LINE.to_string(), result_line(MARKER)]);
        assert!(
            never_announced.should_close(),
            "a stage that never announces background tasks at all must still \
             close on its marker alone — conflating NeverAnnounced with \
             Unreadable would hang every ordinary stage for the full idle \
             timeout"
        );

        // A LATER unreadable announcement, after a real pending count was
        // already known, must also block — the fix must not accidentally
        // treat Unreadable as forgiving once real state exists.
        let unreadable_after_pending = observe_all(&[
            INIT_LINE.to_string(),
            bg_tasks_line(1),
            r#"{"type":"system","subtype":"background_tasks_changed","tasks":"not-an-array"}"#
                .to_string(),
            result_line(MARKER),
        ]);
        assert!(
            !unreadable_after_pending.should_close(),
            "an unreadable announcement following a real pending count must \
             still block closing, not silently forget the pending task"
        );
    }

    /// The common case: a single-plan stage that never dispatches anything.
    /// The drain arm is satisfied VACUOUSLY, because nothing was ever
    /// announced — an implementation that waited for a literal empty-list
    /// event would hang every such stage until the idle timeout.
    ///
    /// The interleaved noise lines also pin the other half of the rule's
    /// tolerance: a torn JSON line and a prose line are ignored for the rule
    /// (they can neither satisfy nor block it) while still being teed to the
    /// capture by the reader thread.
    #[test]
    fn close_rule_is_vacuously_drained_when_no_background_tasks_event_appears() {
        let rule = observe_all(&[
            INIT_LINE.to_string(),
            "starting up".to_string(),
            r#"{"type":"assist"#.to_string(),
            result_line(MARKER),
        ]);
        assert!(
            rule.should_close(),
            "a stage that never announced a background task is drained by \
             definition; only the marker arm has anything to satisfy"
        );
    }

    /// Constraint 7. The CLI COALESCES completions: two children can finish
    /// into one `result` event, and two announced tasks can drain to an empty
    /// list in a single `background_tasks_changed`. Counting `result` events
    /// therefore silently undercounts any wave whose completions cluster —
    /// and that shape is superficially indistinguishable from "one child
    /// delivered, one lost". The drained list is the only thing separating
    /// them, so the rule asserts on the list state and never on a count.
    ///
    /// Per 30-04 the drain arm is DEFENSIVE rather than load-bearing: n=2
    /// Mode B trials delivered everything without it. That is the documented
    /// reason to keep it cheaply — "defensive" is not "removable".
    #[test]
    fn coalesced_completions_do_not_undercount_children() {
        let rule = observe_all(&[
            INIT_LINE.to_string(),
            bg_tasks_line(2),
            // BOTH children drain in ONE event...
            bg_tasks_line(0),
            // ...and complete into ONE result.
            coalesced_result_line(MARKER),
        ]);
        assert!(
            rule.should_close(),
            "two announced children, one drain event and one coalesced result \
             must still close — a rule that matched result events against \
             child count would stall here forever"
        );

        // Negative control: the SAME single coalesced result with the drain
        // withheld must NOT close. Without this, the assertion above is also
        // satisfied by a rule that simply closes on any result event, and the
        // test would be measuring nothing.
        let undrained = observe_all(&[
            INIT_LINE.to_string(),
            bg_tasks_line(2),
            coalesced_result_line(MARKER),
        ]);
        assert!(
            !undrained.should_close(),
            "control: it is the drained list that decides, not the arrival of \
             a result event"
        );
    }

    /// T-31-01. The CLI echoes the operator's prompt back into the same
    /// stdout, and DevFlow's own stage prompts discuss `DEVFLOW_RESULT`
    /// markers at length — so marker TEXT is not evidence of a verdict. Only
    /// a marker inside an event that is both `type: "result"` and top-level
    /// counts, reusing the one provenance predicate rather than inventing a
    /// second notion of trustworthiness.
    #[test]
    fn marker_inside_a_non_top_level_result_does_not_satisfy_the_close_rule() {
        let subagent = observe_all(&[INIT_LINE.to_string(), subagent_result_line(MARKER)]);
        assert!(
            !subagent.should_close(),
            "a subagent-origin result carrying a marker must not close the \
             stream — same provenance hole constraint 9 item 2 closed for the \
             stage verdict"
        );

        // Control: the identical envelope WITHOUT the planted parent id is
        // top-level and legitimately closes. Without this the assertion above
        // would also pass against a rule that never closes at all.
        let top_level = observe_all(&[INIT_LINE.to_string(), result_line(MARKER)]);
        assert!(
            top_level.should_close(),
            "control: the same event without a parent id is authoritative"
        );
    }

    /// The Phase 31 tracer: ONE Claude-shaped stage driven end to end through
    /// the pipe-owning supervisor.
    ///
    /// The stub behaves like the real CLI on the two axes under test and no
    /// others: it takes its initial turn from stdin, and it keeps stdin open
    /// as a channel it can still be spoken to on. It is a `sh` script because
    /// the wire behaviour is the subject, not the binary.
    ///
    /// **The early-close negative control is the point of the probe files.**
    /// A stub that merely blocks on stdin EOF before exiting cannot fail:
    /// whether the monitor closes stdin immediately after the write or only
    /// after the close rule is satisfied, the stub still eventually sees EOF
    /// and still exits 0. So the stub instead SAMPLES stdin liveness at a
    /// moment when a correct monitor provably has not closed it — after the
    /// drain, before any marker — and records `EARLY` if it is already gone.
    /// Two files that must disagree: `eof` must exist at the end, `early`
    /// must never exist.
    ///
    /// **The prompt sentinel is a negative control on JSON escaping.** The
    /// sentinel sits on the SECOND line of a multi-line prompt containing a
    /// double quote. `user_turn_line` escapes it, so the whole prompt arrives
    /// as one physical line and the stub's single `read` sees the sentinel. A
    /// `format!`-interpolated implementation would emit a torn two-line
    /// document, the stub's `read` would return only the first line, and the
    /// sentinel check would fail — which is exactly what should happen.
    #[test]
    fn pipe_owning_monitor_delivers_prompt_via_stdin_and_captures_stream() {
        const SENTINEL: &str = "TRACER-PROMPT-SENTINEL";

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let eof_file = root.join("stdin-eof");
        let early_file = root.join("stdin-closed-early");

        // A quote on line one, the sentinel on line two — see the doc above.
        let prompt = format!("first line with a \" quote\n{SENTINEL}");

        let script = format!(
            r#"
set -u
IFS= read -r turn || {{ echo "NO_INITIAL_TURN_ON_STDIN" >&2; exit 91; }}
case "$turn" in
  *{SENTINEL}*) ;;
  *) echo "INITIAL_TURN_MISSING_PROMPT: $turn" >&2; exit 92 ;;
esac

# Probe: block on stdin until EOF, then record it. stdout is redirected so
# this subshell does not hold the capture pipe open after the main shell exits.
#
# `exec 3<&0` then `cat <&3` is load-bearing, not a flourish: POSIX assigns
# /dev/null to a BACKGROUNDED list's stdin before any explicit redirection
# when job control is off. A bare `( cat > /dev/null ) &` therefore reads EOF
# instantly and reports an early close that never happened. The explicit
# `<&3` is applied after that default and overrides it.
exec 3<&0
( cat <&3 > /dev/null; printf 'EOF\n' > '{eof}' ) > /dev/null 2>&1 &

printf '%s\n' '{{"type":"system","subtype":"init","session_id":"tracer-1"}}'
printf '%s\n' '{{"type":"system","subtype":"background_tasks_changed","tasks":[{{"task_id":"t1","task_type":"local_agent"}}]}}'
printf '%s\n' '{{"type":"system","subtype":"background_tasks_changed","tasks":[]}}'

# The drain has landed but no marker has. A correct monitor is still holding
# stdin open here; sample it and record the violation if it is not.
sleep 0.5
if [ -f '{eof}' ]; then printf 'EARLY\n' > '{early}'; fi

printf '%s\n' '{{"type":"result","subtype":"success","is_error":false,"session_id":"tracer-1","result":"DEVFLOW_RESULT: {{\"status\":\"success\",\"commits\":2}}"}}'

# Bounded wait for EOF: a monitor that never closes stdin must fail the
# assertions below, not hang the suite.
i=0
while [ $i -lt 100 ] && [ ! -f '{eof}' ]; do
  sleep 0.1
  i=$((i+1))
done
exit 0
"#,
            eof = eof_file.display(),
            early = early_file.display(),
        );

        let code = supervise_stream_child(
            root,
            phase,
            root,
            &prompt,
            Duration::from_secs(20),
            "sh",
            &["-c".to_string(), script],
            &[],
            AgentKind::Claude,
        )
        .expect("pipe-owning monitor should supervise the stub to completion");

        let stderr = std::fs::read_to_string(crate::agent_result::stderr_path(root, phase))
            .unwrap_or_default();
        assert_eq!(
            code, 0,
            "stub exited {code}; 91 = no initial turn arrived on stdin, \
             92 = the turn arrived but did not carry the prompt (a JSON \
             escaping regression tears it across lines). stderr: {stderr:?}"
        );

        assert!(
            !early_file.exists(),
            "the monitor closed the child's stdin BEFORE the close rule was \
             satisfied — the drain had landed but no DEVFLOW_RESULT marker had. \
             Constraint 4's AND cannot be honoured once stdin is gone: a \
             task-notification turn would have nowhere to be delivered."
        );
        assert!(
            eof_file.exists(),
            "the monitor never closed the child's stdin at all; the close rule \
             should have fired once the marker arrived with the task list drained"
        );

        let capture =
            std::fs::read_to_string(crate::agent_result::stdout_path(root, phase)).unwrap();
        for expected in [
            r#""subtype":"init""#,
            r#""task_id":"t1""#,
            r#""tasks":[]"#,
            r#""type":"result""#,
        ] {
            assert!(
                capture.contains(expected),
                "capture is missing {expected}; got:\n{capture}"
            );
        }
        assert!(
            crate::agent_result::capture_is_claude_stream(&capture),
            "the capture must classify as a Claude stream-json document — \
             this is what makes 30b's stream parser reachable at all:\n{capture}"
        );

        let result = crate::agent_result::evaluate_layer1(root, phase)
            .expect("Layer 1 must decide this capture");
        assert_eq!(
            result.status,
            crate::agent_result::AgentStatus::Success,
            "Layer 1 verdict from the stream capture: {result:?}"
        );

        let exit = std::fs::read_to_string(crate::agent_result::exit_code_path(root, phase))
            .expect("the monitor must record the child's exit code");
        assert_eq!(exit.trim(), "0", "exit file contents: {exit:?}");
    }

    /// Build a stub that goes silent for longer than the idle window, with or
    /// without first announcing a background task.
    ///
    /// The two arms differ in exactly one line. That is the point: it is the
    /// announcement, and nothing else, that decides whether the silence is
    /// read as work or as a hang.
    fn silent_stub(announce_task: bool) -> String {
        let announce = if announce_task {
            format!("printf '%s\\n' '{REAL_TASK_STARTED}'")
        } else {
            String::from(": # no background task announced")
        };
        format!(
            r#"
set -u
IFS= read -r _turn || exit 91
printf '%s\n' '{INIT_LINE}'
{announce}
sleep 3
printf '%s\n' '{REAL_TASK_UPDATED_TERMINAL}'
printf '%s\n' '{RESULT_WITH_MARKER}'
exit 0
"#
        )
    }

    /// The defect this change fixes, end to end: a parent that is correctly
    /// quiet while a subagent works must not be killed.
    ///
    /// Measured shape it reproduces — a real Phase 35.1 Plan run went silent
    /// for exactly the 120s window while `gsd-phase-researcher` was live, and
    /// DevFlow killed it. The CLI then reported the task as `killed` and the
    /// tool use as "user rejected", which reads like an external failure and
    /// is in fact our own signal coming back at us.
    #[test]
    fn idle_timeout_does_not_fire_while_a_background_task_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(51);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let code = supervise_stream_child(
            root,
            phase,
            root,
            "prompt",
            Duration::from_secs(1),
            "sh",
            &["-c".to_string(), silent_stub(true)],
            &[],
            AgentKind::Claude,
        )
        .expect("the monitor must supervise a backgrounding stub to completion");

        assert_eq!(code, 0, "stub should exit cleanly, not be killed");
        assert!(
            !crate::agent_result::idle_timeout_path(root, phase).exists(),
            "an idle-timeout verdict was recorded for a stage whose subagent was \
             demonstrably alive — this is the false kill the extension exists to \
             prevent, and because evaluate_layer1 reads that side channel FIRST \
             the bogus verdict would outrank the stage's real success"
        );
    }

    /// A quota denial must not be recorded as a hang.
    ///
    /// End-to-end counterpart of `capture_shows_rate_limit_denial`'s unit
    /// tests: the stub announces an explicit `rejected` denial and then goes
    /// silent, exactly as a real agent does when it runs out of credits. No
    /// idle-timeout verdict may be written, because that record would outrank
    /// the rate-limit classifier and turn a resumable pause into "TERMINAL and
    /// not retried automatically".
    ///
    /// Note the stub has NO open background task — this is the arm the
    /// drain-gate extension does not cover, and the arm a real quota denial
    /// lands in.
    #[test]
    fn a_quota_denial_is_not_recorded_as_an_idle_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(53);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let denial = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1786222800,"rateLimitType":"seven_day","overageStatus":"rejected","overageDisabledReason":"out_of_credits","isUsingOverage":false},"uuid":"u-rl","session_id":"s-rl"}"#;
        let script = format!(
            r#"
set -u
IFS= read -r _turn || exit 91
printf '%s\n' '{INIT_LINE}'
printf '%s\n' '{denial}'
sleep 3
exit 0
"#
        );

        let _ = supervise_stream_child(
            root,
            phase,
            root,
            "prompt",
            Duration::from_secs(1),
            "sh",
            &["-c".to_string(), script],
            &[],
            AgentKind::Claude,
        );

        assert!(
            !crate::agent_result::idle_timeout_path(root, phase).exists(),
            "a quota denial was recorded as an idle timeout — the operator is told the \
             stream went silent when the truth is 'out of credits', and the run is \
             marked terminal instead of resumable"
        );
    }

    /// D-15 (backlog 999.102): the idle-timeout record carries the cause the
    /// capture already held — `terminal_reason`, `errors` and `origin.kind` of
    /// the last top-level `result` event — beside the five existing keys.
    ///
    /// The stub's `result` event is the measured 999.102 shape and carries no
    /// `DEVFLOW_RESULT` marker, so stdin stays open and the stub's silence is
    /// what fires the idle timeout.
    ///
    /// **What this does NOT establish:** that a real Claude CLI still emits
    /// these keys under these names. It proves the monitor reads them from a
    /// stub's event and writes them before the kill.
    #[test]
    fn the_idle_timeout_record_carries_the_last_result_cause() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(57);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let result = r#"{"type":"result","subtype":"success","is_error":false,"terminal_reason":"aborted_tools","errors":["[ede_diagnostic] result_type=user last_content_type=n/a stop_reason=tool_use"],"origin":{"kind":"task-notification"},"session_id":"s-1","uuid":"u-res-cause","result":"no marker"}"#;
        let script = format!(
            r#"
set -u
IFS= read -r _turn || exit 91
printf '%s\n' '{INIT_LINE}'
printf '%s\n' '{result}'
sleep 3
exit 0
"#
        );

        let _ = supervise_stream_child(
            root,
            phase,
            root,
            "prompt",
            Duration::from_secs(1),
            "sh",
            &["-c".to_string(), script],
            &[],
            AgentKind::Claude,
        );

        let raw = std::fs::read_to_string(crate::agent_result::idle_timeout_path(root, phase))
            .expect("the silent stub must have produced an idle-timeout record");
        let record: serde_json::Value = serde_json::from_str(&raw).expect("record must parse");
        let object = record.as_object().expect("the record is a JSON object");
        let key = |name: &str| {
            object
                .get(name)
                .unwrap_or_else(|| panic!("record has no {name:?} key: {raw}"))
        };
        for existing in ["status", "idle_secs", "agent_pid", "written_at", "commits"] {
            key(existing);
        }
        assert_eq!(key("terminal_reason").as_str(), Some("aborted_tools"));
        let first_error = key("errors")
            .as_array()
            .and_then(|errors| errors.first())
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("errors is not a non-empty string array: {raw}"));
        assert!(
            first_error.starts_with("[ede_diagnostic]"),
            "errors[0] = {first_error:?}"
        );
        assert_eq!(key("origin_kind").as_str(), Some("task-notification"));
    }

    /// D-16 (backlog 999.71, classifier half): a capture that ends in a torn
    /// JSON line at the idle timeout is not a clean silence. The record is
    /// still written (it is what stops Layer 2 scoring partial commits as
    /// success), flagged `capture_torn_final_line`, and the monitor log names
    /// the torn line.
    ///
    /// The stub prints a complete but malformed line, which is the only way a
    /// torn line can reach the capture before the kill: the reader tees whole
    /// lines, and a killed child's partial line arrives at EOF, after this
    /// check. The read side re-checks for it only when this check wrote a
    /// record: after a complete denial this check suppresses the record, so
    /// that order ends as an indeterminate `Failed` gate while the runner's log
    /// says rate-limited (known residual, backlog 999.207; not tested here).
    #[test]
    fn a_torn_final_line_at_the_idle_timeout_is_flagged_on_the_record() {
        let run_silent_stub = |root: &Path, phase: PhaseId, last_line: Option<&str>| {
            std::fs::create_dir_all(root.join(".devflow")).unwrap();
            let last = last_line
                .map(|line| format!("printf '%s\\n' '{line}'"))
                .unwrap_or_default();
            let script = format!(
                r#"
set -u
IFS= read -r _turn || exit 91
printf '%s\n' '{INIT_LINE}'
{last}
sleep 3
exit 0
"#
            );
            let _ = supervise_stream_child(
                root,
                phase,
                root,
                "prompt",
                Duration::from_secs(1),
                "sh",
                &["-c".to_string(), script],
                &[],
                AgentKind::Claude,
            );
            let raw = std::fs::read_to_string(crate::agent_result::idle_timeout_path(root, phase))
                .expect("a silent stub must produce an idle-timeout record");
            let record: serde_json::Value =
                serde_json::from_str(&raw).expect("the record must parse");
            assert!(
                !crate::agent_result::monitor_log_path(root, phase).exists(),
                "no monitor log is written (D-09): the runner logs the timeout"
            );
            record
        };

        let torn_dir = tempfile::tempdir().unwrap();
        let record = run_silent_stub(
            torn_dir.path(),
            PhaseId::new(58),
            Some(r#"{"type":"rate_limit_ev"#),
        );
        assert_eq!(
            record.get("capture_torn_final_line"),
            Some(&serde_json::Value::Bool(true)),
            "a torn final line at the timeout must be flagged on the record: {record}"
        );

        // CONTROL (separate fixture dir): an intact, init-only capture writes
        // the record without the flag.
        let intact_dir = tempfile::tempdir().unwrap();
        let record = run_silent_stub(intact_dir.path(), PhaseId::new(59), None);
        assert!(
            record.get("capture_torn_final_line").is_none(),
            "an intact capture must not carry the torn flag: {record}"
        );
    }

    /// D-16: a quota denial followed by a torn final line at the idle timeout
    /// is recorded, flagged torn, and not suppressed as a rate limit, so the
    /// monitor and the verdict agree: the advance reads an idle timeout, never
    /// `RateLimited`, and no failover or cron resume follows.
    #[test]
    fn a_denial_followed_by_a_torn_line_at_the_idle_timeout_is_recorded_not_suppressed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(54);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let denial = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1786222800,"rateLimitType":"seven_day","overageStatus":"rejected","overageDisabledReason":"out_of_credits","isUsingOverage":false},"uuid":"u-rl","session_id":"s-rl"}"#;
        let torn = r#"{"type":"res"#;
        let script = format!(
            r#"
set -u
IFS= read -r _turn || exit 91
printf '%s\n' '{INIT_LINE}'
printf '%s\n' '{denial}'
printf '%s\n' '{torn}'
sleep 3
exit 0
"#
        );

        let _ = supervise_stream_child(
            root,
            phase,
            root,
            "prompt",
            Duration::from_secs(1),
            "sh",
            &["-c".to_string(), script],
            &[],
            AgentKind::Claude,
        );

        let raw = std::fs::read_to_string(crate::agent_result::idle_timeout_path(root, phase))
            .expect(
                "a denial followed by a torn line was suppressed as a rate limit: the monitor \
                 wrote no idle-timeout record",
            );
        let record: serde_json::Value = serde_json::from_str(&raw).expect("the record parses");
        assert_eq!(
            record.get("capture_torn_final_line"),
            Some(&serde_json::Value::Bool(true)),
            "the record must carry the torn flag: {record}"
        );
        let verdict =
            crate::agent_result::evaluate_layer1(root, phase).expect("the record is a verdict");
        assert_eq!(
            verdict.status,
            crate::agent_result::AgentStatus::IdleTimeout,
            "{verdict:?}"
        );
    }

    /// Negative control for the test above. Same stub, same silence, one line
    /// removed — the guard must still fire when nothing is outstanding, or it
    /// has simply been disabled rather than made accurate.
    #[test]
    fn idle_timeout_still_fires_when_no_background_task_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(52);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let _ = supervise_stream_child(
            root,
            phase,
            root,
            "prompt",
            Duration::from_secs(1),
            "sh",
            &["-c".to_string(), silent_stub(false)],
            &[],
            AgentKind::Claude,
        );

        assert!(
            crate::agent_result::idle_timeout_path(root, phase).exists(),
            "silence with no outstanding work must still be recorded as a hang; \
             if this never fires, the idle guard has been removed, not fixed"
        );
    }

    /// Peer review 2026-08-03, CRITICAL: `BufRead::lines()` yields
    /// `Err(InvalidData)` on one non-UTF-8 byte, and the reader treated any read
    /// error as EOF — silently truncating the capture and dropping every later
    /// line, INCLUDING the terminal marker. The supervisor manufactured exactly
    /// the boundary-truncation failure constraint 9 exists to defend against.
    ///
    /// **What this does NOT establish:** that the real `claude` CLI ever emits
    /// non-UTF-8 on this stream. It emits JSON, which should be valid UTF-8. This
    /// pins the supervisor's robustness, not a demonstrated CLI behaviour.
    #[test]
    fn non_utf8_byte_does_not_truncate_the_capture() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(11);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        // A raw 0xFF is invalid UTF-8 in any position. It sits BETWEEN two good
        // lines, so a reader that dies on it loses the marker that follows.
        let script = r#"
set -u
IFS= read -r _turn || exit 91
printf '%s\n' '{"type":"system","subtype":"init","session_id":"utf8-1"}'
printf 'raw-\377-bytes\n'
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"session_id":"utf8-1","result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}'
exit 0
"#;

        let code = supervise_stream_child(
            root,
            phase,
            root,
            "prompt",
            Duration::from_secs(20),
            "sh",
            &["-c".to_string(), script.to_string()],
            &[],
            AgentKind::Claude,
        )
        .expect("the monitor must survive a non-UTF-8 byte on the child's stdout");
        assert_eq!(code, 0, "stub should exit cleanly");

        let capture =
            std::fs::read_to_string(crate::agent_result::stdout_path(root, phase)).unwrap();
        assert!(
            capture.contains(r#""type":"result""#),
            "the terminal result event was lost: a non-UTF-8 byte earlier in the \
             stream truncated the capture. This is the regression:\n{capture}"
        );
        assert!(
            capture.contains("raw-"),
            "the undecodable line itself must still be teed (lossily), since the \
             capture is the verbatim record:\n{capture}"
        );
        let result = crate::agent_result::evaluate_layer1(root, phase)
            .expect("Layer 1 must still decide a capture that contained a bad byte");
        assert_eq!(
            result.status,
            crate::agent_result::AgentStatus::Success,
            "verdict after lossy decode: {result:?}"
        );
    }

    /// Peer review 2026-08-03, CRITICAL: after the close rule released stdin the
    /// supervisor kept timing out on silence and fired `fire_idle_timeout`,
    /// writing an authoritative `IdleTimeout` verdict OVER a stage that had
    /// already reported success. `evaluate_layer1` reads that side channel first
    /// — by design, so nothing can shadow a real timeout — so the bogus verdict
    /// won and was unrecoverable.
    ///
    /// The timeout here (600ms) is injected short deliberately; the child sleeps
    /// well past it AFTER the marker. **What this does NOT establish:** that the
    /// 120s production floor is right — that rests on the keepalive measurement
    /// in `31-IDLE-GAP-MEASUREMENTS.md`, not on this test.
    #[test]
    fn no_idle_timeout_is_recorded_when_the_child_is_merely_slow_to_exit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(12);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let script = r#"
set -u
IFS= read -r _turn || exit 91
printf '%s\n' '{"type":"system","subtype":"init","session_id":"slow-1"}'
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"session_id":"slow-1","result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}'
# Everything has been said; the close rule fires here. Now wind down slowly,
# well past the injected idle window, emitting nothing.
sleep 3
exit 0
"#;

        let code = supervise_stream_child(
            root,
            phase,
            root,
            "prompt",
            Duration::from_millis(600),
            "sh",
            &["-c".to_string(), script.to_string()],
            &[],
            AgentKind::Claude,
        )
        .expect("a slow-exiting child that already reported is not a failure");

        assert!(
            !crate::agent_result::idle_timeout_path(root, phase).exists(),
            "an idle-timeout verdict was written for a stage that had ALREADY \
             emitted its terminal marker and drained its tasks — silence after a \
             deliberate close is expected, not a hang"
        );
        assert_eq!(code, 0, "the child exited cleanly, if slowly");

        let result = crate::agent_result::evaluate_layer1(root, phase)
            .expect("Layer 1 must decide this capture");
        assert_eq!(
            result.status,
            crate::agent_result::AgentStatus::Success,
            "a completed stage must not be reported as a timeout: {result:?}"
        );
    }

    /// Peer review 2026-08-03 (found independently by BOTH reviewers and by the
    /// 31-04 plan review as W1): `status.code()` is `None` for a signal-killed
    /// child, and `unwrap_or(-1)` discarded the signal. `-1` matches neither the
    /// 137 nor the 127 arm, so a kernel OOM kill arrived as a generic `Failed`
    /// and routed to `GateReview` — asking a human to code-review a stage the
    /// kernel killed — instead of `GateInfra`.
    ///
    /// This asserts on what the monitor ACTUALLY writes for a real SIGKILL. The
    /// pre-existing arbitration test hardcoded `"137\n"` into its fixture, so it
    /// passed green against this defect the entire time — which is why this test
    /// spawns a child and kills it rather than writing the file itself.
    #[test]
    fn a_signal_killed_child_records_128_plus_signal_not_minus_one() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(13);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        // SIGKILL itself: no exit code exists, only a termination signal.
        let script = r#"
set -u
IFS= read -r _turn || exit 91
printf '%s\n' '{"type":"system","subtype":"init","session_id":"sig-1"}'
kill -9 $$
"#;

        let code = supervise_stream_child(
            root,
            phase,
            root,
            "prompt",
            Duration::from_secs(20),
            "sh",
            &["-c".to_string(), script.to_string()],
            &[],
            AgentKind::Claude,
        )
        .expect("the monitor must reap a signal-killed child");

        assert_eq!(
            code, 137,
            "SIGKILL(9) must be recorded as 128+9=137, the value \
             `evaluate_layer2` and `reconcile_stream_success_against_exit_code` \
             map to ResourceKilled/GateInfra. -1 means the signal was discarded."
        );
        let exit = std::fs::read_to_string(crate::agent_result::exit_code_path(root, phase))
            .expect("the monitor must record the exit code");
        assert_eq!(exit.trim(), "137", "exit file contents: {exit:?}");
    }

    /// A one-line identity/state summary of a pid, for failure diagnostics.
    /// `Name`/`State`/`PPid` come from `/proc/<pid>/status`; the cmdline
    /// distinguishes a shell that exec'd its command from one that forked it.
    /// Test-only; never used in a decision.
    fn proc_snapshot(pid: u32) -> String {
        let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
            return format!("GONE (no /proc/{pid})");
        };
        let field = |key: &str| {
            status
                .lines()
                .find(|l| l.starts_with(key))
                .map(|l| l.split_whitespace().skip(1).collect::<Vec<_>>().join(" "))
                .unwrap_or_else(|| "?".into())
        };
        let cmdline = std::fs::read(format!("/proc/{pid}/cmdline"))
            .map(|raw| {
                let joined = raw
                    .split(|&b| b == 0)
                    .filter(|a| !a.is_empty())
                    .map(|a| String::from_utf8_lossy(a).into_owned())
                    .collect::<Vec<_>>()
                    .join(" ");
                if joined.is_empty() {
                    "<empty>".to_string()
                } else {
                    joined
                }
            })
            .unwrap_or_else(|e| format!("<unreadable: {e}>"));
        format!(
            "ALIVE Name={} State={} PPid={} cmdline=[{cmdline}]",
            field("Name:"),
            field("State:"),
            field("PPid:")
        )
    }

    /// The stream shape through [`run_stage`] in this test process, under a
    /// fresh run token and with no signal pipe (which needs a child test
    /// process; the 50-08 tests below run one). Returns the code the exit
    /// file records: the agent's own for a completion, 143 for a stop.
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors the stream shape's independently varied inputs"
    )]
    fn supervise_stream_child(
        project_root: &Path,
        phase: PhaseId,
        workdir: &Path,
        prompt: &str,
        idle_timeout: Duration,
        program: &str,
        args: &[String],
        envs: &[(String, String)],
        agent: AgentKind,
    ) -> Result<i32, MonitorError> {
        let mut spec = StageSpec::new(
            project_root.to_path_buf(),
            phase,
            agent,
            program.to_string(),
            StageIo::Stream {
                prompt: prompt.to_string(),
                idle_timeout,
            },
        );
        spec.workdir = workdir.to_path_buf();
        spec.args = args.to_vec();
        spec.envs = envs.to_vec();
        let token = crate::stop::StopToken::for_run();
        Ok(match run_stage(spec, &token)? {
            AgentOutcome::Completed(code) => code,
            AgentOutcome::Stopped => crate::stop::STOPPED_EXIT_CODE,
        })
    }

    // ---- the three capture-shape launch tests, each running `run_stage` in
    //      a child test process (`launch_stage_with`, below).

    /// A capture-shape fake agent `codex` running `body`, then waiting for
    /// `go` so the parent sees it running before it exits.
    fn released_agent(fixture: &StageFixture, body: &str, go: &Path) -> PathBuf {
        fixture.agent(
            "codex",
            &format!(
                "{body}\nwhile [ ! -e '{go}' ]; do sleep 0.05; done\nexit 0",
                go = go.display()
            ),
        )
    }

    /// The named signal mask of `pid`'s `/proc/<pid>/status` (`SigCgt`,
    /// `SigBlk`, ...). Panics when the field cannot be read: an absent field
    /// must not read as "clear".
    #[cfg(target_os = "linux")]
    fn signal_mask(pid: u32, field: &str) -> u64 {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status"))
            .expect("/proc/<pid>/status must be readable for a live child");
        let hex = status
            .lines()
            .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
            .unwrap_or_else(|| panic!("/proc/{pid}/status has no {field} line"))
            .trim();
        u64::from_str_radix(hex, 16).expect("signal masks are hex")
    }

    const WORKTREE_CAPTURE_TEST: &str =
        "monitor::tests::run_stage_runs_agent_in_worktree_but_captures_in_project_root";

    #[test]
    fn run_stage_runs_agent_in_worktree_but_captures_in_project_root() {
        if crate::test_support::in_child_test(WORKTREE_CAPTURE_TEST) {
            supervise_stage_in_child();
            return;
        }
        let fixture = StageFixture::new();
        let go = fixture.path("go");
        // Stub agent: print its cwd so the test proves the stage started it
        // in the worktree.
        let program = released_agent(&fixture, "pwd; echo WORKTREE_READY", &go);
        let mut run = launch_stage(
            WORKTREE_CAPTURE_TEST,
            &fixture,
            Shape::Capture,
            AgentKind::Codex,
            &program,
            "",
        );
        std::fs::write(&go, "").expect("release the agent");
        assert_eq!(run.finish(WORKTREE_CAPTURE_TEST), "Completed(0) advancing");

        let captured = fixture.stdout();
        assert!(
            captured.contains("WORKTREE_READY")
                && captured.contains(&fixture.workdir.display().to_string()),
            "agent did not run in worktree cwd; captured stdout: {captured:?}"
        );
        assert!(
            crate::agent_result::stdout_path(&fixture.root, STAGE_PHASE).exists(),
            "stdout capture missing in main .devflow"
        );
        assert!(
            !crate::agent_result::stdout_path(&fixture.workdir, STAGE_PHASE).exists(),
            "stdout capture should not be written under the worktree"
        );
        drop(run.group);
    }

    /// Build the fixture repositories through the scrubbing constructor, as
    /// the other git-touching test modules do.
    ///
    /// A bare `Command::new("git")` here would itself inherit an ambient
    /// hostile `GIT_DIR` — so under this phase's own acceptance command
    /// (`GIT_DIR=<throwaway>/.git cargo test -p devflow-core ...`) the
    /// fixture setup would target the throwaway repository instead of
    /// `root`, and the test below would fail for a reason that has nothing
    /// to do with the behavior it is guarding.
    fn git(root: &Path, args: &[&str]) {
        let ok = crate::test_support::git_command(root)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    }

    fn init_repo(root: &Path) {
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "user.name", "Test"]);
    }

    const HOSTILE_GIT_DIR_TEST: &str =
        "monitor::tests::run_stage_agent_git_calls_resolve_workdir_not_a_hostile_git_dir";

    /// 27-REVIEW WR-03: whatever environment the runner carries rides
    /// down to the agent and every git command it runs. This proves the
    /// scrub with a real spawned agent process: the stage's process (a child
    /// test process) runs with `GIT_DIR` pointed at an unrelated foreign
    /// repository, the agent shells out to `git rev-parse --absolute-git-dir`,
    /// and the resolved path must be the agent's own workdir.
    ///
    /// `GIT_DIR` is never set on this test process itself (Rust 2024
    /// `unsafe`, unsound under threaded tests — Phase 25 D-14), only on the
    /// child test process. Control: the child's own environment is read from
    /// `/proc` and must carry the hostile value, so a pass cannot come from a
    /// `GIT_DIR` that never reached it.
    #[test]
    fn run_stage_agent_git_calls_resolve_workdir_not_a_hostile_git_dir() {
        if crate::test_support::in_child_test(HOSTILE_GIT_DIR_TEST) {
            supervise_stage_in_child();
            return;
        }
        let fixture = StageFixture::new();
        init_repo(&fixture.workdir);
        let foreign = tempfile::tempdir().unwrap();
        init_repo(foreign.path());
        let foreign_git = foreign.path().join(".git");
        let go = fixture.path("go");
        let program = released_agent(&fixture, "git rev-parse --absolute-git-dir", &go);
        let mut run = launch_stage_with(
            HOSTILE_GIT_DIR_TEST,
            &fixture,
            StageLaunchArgs {
                shape: Shape::Capture,
                agent: AgentKind::Codex,
                program: &program,
                adapter_env: "",
                args: &[],
                extra_env: &[("GIT_DIR", foreign_git.as_os_str())],
            },
        );
        let environ = std::fs::read(format!("/proc/{}/environ", run.child.id()))
            .expect("read the child test process's environment");
        let hostile = format!("GIT_DIR={}", foreign_git.display());
        assert!(
            environ
                .split(|byte| *byte == 0)
                .any(|entry| entry == hostile.as_bytes()),
            "control: the child test process must run with the hostile {hostile}"
        );
        std::fs::write(&go, "").expect("release the agent");
        assert_eq!(run.finish(HOSTILE_GIT_DIR_TEST), "Completed(0) advancing");

        let captured = fixture.stdout();
        let resolved = std::fs::canonicalize(captured.trim())
            .expect("agent's reported git-dir must exist on disk");
        let expected = std::fs::canonicalize(fixture.workdir.join(".git"))
            .expect("caller repo .git must exist");
        assert_eq!(
            resolved, expected,
            "agent's git call resolved to a hostile GIT_DIR's repository instead of the \
             caller's own workdir: got {resolved:?}, want {expected:?}"
        );
        drop(run.group);
    }

    const LITERAL_ARGV_TEST: &str = "monitor::tests::run_stage_treats_agent_args_as_literal_argv";

    #[test]
    fn run_stage_treats_agent_args_as_literal_argv() {
        if crate::test_support::in_child_test(LITERAL_ARGV_TEST) {
            supervise_stage_in_child();
            return;
        }
        let fixture = StageFixture::new();
        let go = fixture.path("go");
        let payload = "value; touch INJECTED";
        let program = released_agent(&fixture, "printf '%s\\n' \"$1\"; echo ARGV_SAFE", &go);
        let args = [payload.to_string()];
        let mut run = launch_stage_with(
            LITERAL_ARGV_TEST,
            &fixture,
            StageLaunchArgs {
                shape: Shape::Capture,
                agent: AgentKind::Codex,
                program: &program,
                adapter_env: "",
                args: &args,
                extra_env: &[],
            },
        );
        std::fs::write(&go, "").expect("release the agent");
        assert_eq!(run.finish(LITERAL_ARGV_TEST), "Completed(0) advancing");

        let captured = fixture.stdout();
        assert!(
            captured.contains(payload),
            "literal argv missing: {captured:?}"
        );
        assert!(captured.contains("ARGV_SAFE"));
        assert!(!fixture.workdir.join("INJECTED").exists());
        assert!(!fixture.root.join("INJECTED").exists());
        drop(run.group);
    }

    // ---- idle timeout (31-02, D-01..D-08) --------------------------------

    /// D-04: a value below the floor is raised to it, and the fact is
    /// observable to the CALLER as a value — not only as a log line a test
    /// would have to capture stdout to see.
    #[test]
    fn idle_timeout_secs_clamps_below_floor_and_logs() {
        let setting = parse_idle_timeout_secs(Some("5".to_string()));

        assert_eq!(setting.timeout, Duration::from_secs(120));
        assert!(setting.clamped(), "the clamp must be observable as a value");
        assert_eq!(
            setting.resolution,
            IdleTimeoutResolution::Clamped { configured: 5 }
        );

        // The notice must NAME the configured value, the floor, and the value
        // actually in force — a clamp that says only "clamped" leaves the
        // operator guessing which of the three numbers won.
        let notice = setting.notice().expect("a clamp owes a loud notice");
        for fragment in ["5", "120", IDLE_TIMEOUT_ENV] {
            assert!(
                notice.contains(fragment),
                "notice must name {fragment:?}; got: {notice}"
            );
        }
    }

    /// The floor raises, it never lowers: a value above it survives verbatim
    /// and reports no clamp.
    #[test]
    fn idle_timeout_secs_accepts_values_above_floor() {
        let setting = parse_idle_timeout_secs(Some("300".to_string()));

        assert_eq!(setting.timeout, Duration::from_secs(300));
        assert!(!setting.clamped());
        assert_eq!(setting.resolution, IdleTimeoutResolution::Configured);
        assert_eq!(
            setting.notice(),
            None,
            "an honoured value is unremarkable and must not shout"
        );

        // Boundary: exactly the floor is CONFIGURED, not CLAMPED. An
        // off-by-one here would report a clamp that never happened and train
        // operators to ignore the notice.
        let exact = parse_idle_timeout_secs(Some("120".to_string()));
        assert_eq!(exact.resolution, IdleTimeoutResolution::Configured);
        assert!(!exact.clamped());
    }

    /// Absent, empty, and unparseable all resolve to the floor. The three are
    /// NOT equivalent in loudness: nothing configured is silent, a typo is not.
    #[test]
    fn idle_timeout_secs_defaults_to_the_floor() {
        let floor = Duration::from_secs(IDLE_TIMEOUT_FLOOR_SECS);

        for raw in [None, Some(String::new()), Some("   ".to_string())] {
            let setting = parse_idle_timeout_secs(raw.clone());
            assert_eq!(setting.timeout, floor, "raw {raw:?} must yield the floor");
            assert_eq!(setting.resolution, IdleTimeoutResolution::Default);
            assert_eq!(setting.notice(), None, "nothing chosen is not an error");
        }

        for raw in ["banana", "60O", "-5", "30.5"] {
            let setting = parse_idle_timeout_secs(Some(raw.to_string()));
            assert_eq!(setting.timeout, floor, "raw {raw:?} must yield the floor");
            assert_eq!(
                setting.resolution,
                IdleTimeoutResolution::Unparseable {
                    raw: raw.to_string()
                }
            );
            assert!(
                setting.notice().is_some(),
                "a typo that silently halves an intended timeout must be loud: {raw:?}"
            );
        }
    }

    /// D-01/D-03: every line resets the window, and there is no outer
    /// wall-clock bound. A child that keeps talking for FOUR times the idle
    /// timeout is never terminated.
    ///
    /// The timeout is injected short (400ms) rather than using the 120s
    /// production default — this measures the RESET MECHANISM, and does so at
    /// a scale the suite can afford. **What it does not establish:** that 120s
    /// is the right production value. That rests on the 2026-08-03 keepalive
    /// measurement recorded on [`IDLE_TIMEOUT_FLOOR_SECS`], not on this test.
    #[test]
    fn idle_timer_resets_on_every_stream_line() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(6);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        // 12 lines x 100ms = 1.2s of talking against a 400ms window. Any
        // implementation that resets on milestones only, or that imposes an
        // outer bound, kills this child before it finishes.
        let script = r#"
set -u
IFS= read -r turn || exit 91
i=0
while [ $i -lt 12 ]; do
  printf '%s\n' '{"type":"system","subtype":"heartbeat","n":'"$i"'}'
  sleep 0.1
  i=$((i+1))
done
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"session_id":"idle-1","result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}'
exit 0
"#;

        let started = std::time::Instant::now();
        let code = supervise_stream_child(
            root,
            phase,
            root,
            "prompt",
            Duration::from_millis(400),
            "sh",
            &["-c".to_string(), script.to_string()],
            &[],
            AgentKind::Claude,
        )
        .expect("a chatty child must be supervised to completion");
        let elapsed = started.elapsed();

        assert_eq!(code, 0, "the chatty child must exit cleanly, not be killed");
        assert!(
            !crate::agent_result::idle_timeout_path(root, phase).exists(),
            "no timeout may fire while the child is still emitting lines"
        );
        assert!(
            elapsed > Duration::from_millis(400),
            "the run must outlast the idle window, else it proves nothing \
             about resetting: {elapsed:?}"
        );

        let capture =
            std::fs::read_to_string(crate::agent_result::stdout_path(root, phase)).unwrap();
        assert_eq!(
            capture.matches("heartbeat").count(),
            12,
            "all twelve resets must have been observed: {capture:?}"
        );
    }

    /// D-05, and the assertion the whole ordering exists for.
    ///
    /// The observation is made LIVE, by a watcher thread sampling the child's
    /// liveness at the first instant the verdict file exists — not by
    /// inspecting order after the fact, which cannot distinguish
    /// write-then-kill from kill-then-write.
    ///
    /// Its own negative control is structural: if the implementation wrote the
    /// verdict AFTER terminating, the watcher would sample a dead child and
    /// this test fails with `Some(false)`. The stub ignores `SIGTERM` so the
    /// window in which "file exists AND child alive" is observable is the full
    /// `TERMINATE_VERIFY_WAIT`, rather than a microsecond race.
    ///
    /// **What the duration of this test measures:** almost entirely
    /// `agent::TERMINATE_VERIFY_WAIT` (3s), because the stub refuses `SIGTERM`
    /// and must be escalated to `SIGKILL`. The 250ms idle window is a rounding
    /// error against it.
    #[test]
    fn idle_timeout_writes_side_channel_before_terminating_child() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(7);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        // One line, then silence. `trap '' TERM` widens the observation
        // window to the full SIGTERM->SIGKILL escalation. The agent records
        // its own pid: no agent pid file is written (D-09).
        let pid_file = root.join("agent-own-pid");
        let script = format!(
            r#"
set -u
IFS= read -r turn || exit 91
echo $$ > '{pid}'
trap '' TERM
printf '%s\n' '{{"type":"system","subtype":"init","session_id":"idle-2"}}'
sleep 120
"#,
            pid = pid_file.display()
        );

        let verdict = crate::agent_result::idle_timeout_path(&root, phase);
        let watcher = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            let mut pid: Option<u32> = None;
            while std::time::Instant::now() < deadline {
                if pid.is_none() {
                    pid = std::fs::read_to_string(&pid_file)
                        .ok()
                        .and_then(|s| s.trim().parse::<u32>().ok());
                }
                if verdict.exists() {
                    // Sample liveness at the FIRST moment the verdict exists.
                    return pid.map(crate::agent::agent_running);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            None
        });

        let code = supervise_stream_child(
            &root,
            phase,
            &root,
            "prompt",
            Duration::from_millis(250),
            "sh",
            &["-c".to_string(), script],
            &[],
            AgentKind::Claude,
        )
        .expect("a silent child must still produce a supervised outcome");

        let observed = watcher.join().expect("watcher thread panicked");
        assert_eq!(
            observed,
            Some(true),
            "the verdict must be on disk while the child is STILL ALIVE. \
             Some(false) = written after termination (the D-05 violation); \
             None = the verdict never appeared at all"
        );

        // The verdict must also be readable and correct, not merely present.
        let raw = std::fs::read_to_string(crate::agent_result::idle_timeout_path(&root, phase))
            .expect("verdict file must be readable");
        let record: IdleTimeoutRecord = serde_json::from_str(&raw).expect("verdict must parse");
        assert_eq!(record.status, "idle_timeout");
        assert_eq!(record.idle_secs, 0, "250ms truncates to 0 whole seconds");
        assert!(record.agent_pid > 1);

        // And the whole cascade must agree: Layer 1 reports the timeout.
        let result = crate::agent_result::evaluate_layer1(&root, phase)
            .expect("Layer 1 must decide a timed-out run");
        assert_eq!(
            result.status,
            crate::agent_result::AgentStatus::IdleTimeout,
            "the monitor's verdict must survive all the way to the oracle"
        );

        // The child was killed, so it has no ordinary exit code — the point is
        // that the stage machine still reaches a gate rather than hanging.
        assert!(
            crate::agent_result::exit_code_path(&root, phase).exists(),
            "the exit file must still be written so advance() is reachable"
        );
        let _ = code;

        // The runner logs the timeout through its own log; the monitor log
        // and the agent pid file are no longer written (D-09).
        assert!(
            !crate::agent_result::monitor_log_path(&root, phase).exists(),
            "no monitor log is written"
        );
        assert!(
            !crate::agent_result::agent_pid_path(&root, phase).exists(),
            "no agent pid file is written"
        );
    }

    /// Minimal git repo: `develop` plus a `feature/phase-NN` branch carrying
    /// `commits` extra commits.
    fn init_repo_with_feature_commits(root: &Path, phase: PhaseId, commits: usize) {
        let git = |args: &[&str]| {
            let output = crate::git::git_command(root).args(args).output().unwrap();
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init"]);
        git(&["config", "user.email", "devflow@example.com"]);
        git(&["config", "user.name", "DevFlow Tests"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);
        git(&["checkout", "-b", "develop"]);
        std::fs::write(root.join("README.md"), "base\n").unwrap();
        git(&["add", "README.md"]);
        git(&["commit", "-m", "base"]);

        let branch = format!("feature/phase-{padded}", padded = phase.padded());
        git(&["checkout", "-b", &branch]);
        for i in 0..commits {
            let name = format!("work-{i}.txt");
            std::fs::write(root.join(&name), "work\n").unwrap();
            git(&["add", &name]);
            git(&["commit", "-m", &format!("feat: agent work {i}")]);
        }
    }

    fn commit_count(root: &Path, phase: PhaseId) -> u32 {
        let range = format!("develop..feature/phase-{padded}", padded = phase.padded());
        let output = crate::git::git_command(root)
            .args(["rev-list", "--count", &range])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .unwrap()
    }

    /// D-07/T-31-09: a timeout READS the commit log and never writes to it. A
    /// timeout can be a false positive, and destroying real work on a false
    /// positive is unrecoverable.
    ///
    /// The "commits were enumerated" half is this test's negative control, and
    /// it is not optional: if enumeration silently returned nothing, "no
    /// commits were rolled back" would be trivially, vacuously true.
    #[test]
    fn idle_timeout_does_not_roll_back_commits() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(8);
        init_repo_with_feature_commits(root, phase, 2);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let before = commit_count(root, phase);
        assert_eq!(before, 2, "fixture precondition");

        // No TERM trap here: the child dies promptly, keeping this test fast.
        let script = r#"
set -u
IFS= read -r turn || exit 91
printf '%s\n' '{"type":"system","subtype":"init","session_id":"idle-3"}'
sleep 120
"#;

        supervise_stream_child(
            root,
            phase,
            root,
            "prompt",
            Duration::from_millis(250),
            "sh",
            &["-c".to_string(), script.to_string()],
            &[],
            AgentKind::Claude,
        )
        .expect("a silent child must still produce a supervised outcome");

        assert_eq!(
            commit_count(root, phase),
            before,
            "an idle timeout must never roll back, reset, or revert a commit"
        );

        // NEGATIVE CONTROL: enumeration must actually have found them, else
        // the assertion above is vacuous.
        let raw = std::fs::read_to_string(crate::agent_result::idle_timeout_path(root, phase))
            .expect("verdict file must exist");
        let record: IdleTimeoutRecord = serde_json::from_str(&raw).expect("verdict must parse");
        assert_eq!(
            record.commits.len(),
            2,
            "the verdict must NAME the commits, not merely leave them alone"
        );
        for commit in &record.commits {
            assert_eq!(commit.sha.len(), 40, "full sha expected: {commit:?}");
            assert!(
                commit.subject.starts_with("feat: agent work"),
                "subject must survive enumeration: {commit:?}"
            );
        }

        // And the operator-facing reason names them.
        let result = crate::agent_result::evaluate_layer1(root, phase).unwrap();
        assert_eq!(result.commits, Some(2));
        let reason = result.reason.unwrap();
        assert!(
            reason.contains("NONE of them were rolled back"),
            "reason: {reason}"
        );
    }

    // ------------------------------------------------------------------
    // Agent-aware transport (phase 41, Task 2): user_turn_line_for,
    // agent-aware CloseRule, per-agent idle timeout. Live Antigravity shapes
    // per the round-2 review evidence (antigravity-cli 1.1.16).
    // ------------------------------------------------------------------

    const ANTG_INIT_LINE: &str = r#"{"event":"init","model":"gemini-3.7-flash-high","inputFormat":"stream-json","outputFormat":"stream-json","printTimeout":"60m"}"#;
    const ANTG_STEP_LINE: &str = r#"{"event":"step_update","index":0,"text_delta":"..."}"#;
    const ANTG_RESULT_MARKER_LINE: &str = r#"{"event":"result","result":{"status":"SUCCESS","response":"DEVFLOW_RESULT: {\"status\":\"success\"}\n"}}"#;

    #[test]
    fn user_turn_line_for_antigravity_uses_event_key() {
        let prompt = "stage prompt with \"quotes\" and\nnewlines";
        let antg = user_turn_line_for(AgentKind::Antigravity, prompt);
        let v: serde_json::Value =
            serde_json::from_str(&antg).expect("antigravity turn must be valid JSON");
        assert_eq!(
            v.get("event").and_then(serde_json::Value::as_str),
            Some("user"),
            "antigravity first turn is the event-key shape (D-02)"
        );
        assert!(
            v.get("type").is_none(),
            "no Claude type key may leak into the antigravity turn"
        );
        assert_eq!(
            v.pointer("/message/content")
                .and_then(serde_json::Value::as_str),
            Some(prompt),
            "the prompt survives escaping"
        );

        // Claude stays byte-identical to the long-standing shape.
        let claude = user_turn_line_for(AgentKind::Claude, prompt);
        assert_eq!(
            claude,
            user_turn_line(prompt),
            "Claude must be byte-identical"
        );
        let v: serde_json::Value = serde_json::from_str(&claude).unwrap();
        assert_eq!(
            v.get("type").and_then(serde_json::Value::as_str),
            Some("user")
        );
        assert!(v.get("event").is_none());
    }

    #[test]
    fn close_rule_antigravity_closes_on_event_key_marker() {
        let mut rule = CloseRule::for_agent(AgentKind::Antigravity);
        rule.observe(ANTG_INIT_LINE);
        assert!(!rule.should_close(), "init alone must not close");
        rule.observe(ANTG_STEP_LINE);
        assert!(!rule.should_close(), "progress alone must not close");
        rule.observe(ANTG_RESULT_MARKER_LINE);
        assert!(
            rule.should_close(),
            "event:result with a marker in result.response must close the antigravity stream (B1)"
        );

        // Claude-shaped lines never satisfy the antigravity rule.
        let mut rule = CloseRule::for_agent(AgentKind::Antigravity);
        rule.observe(r#"{"type":"system","subtype":"init","session_id":"s1"}"#);
        rule.observe(
            r#"{"type":"result","subtype":"success","result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}"#,
        );
        assert!(
            !rule.should_close(),
            "a Claude stream must not satisfy the antigravity close rule"
        );

        // The Claude rule's behaviour is unchanged for Claude input (B1: the
        // Claude predicate is byte-for-byte what it always was).
        let mut claude_rule = CloseRule::for_agent(AgentKind::Claude);
        claude_rule.observe(r#"{"type":"system","subtype":"init","session_id":"s1"}"#);
        claude_rule.observe(
            r#"{"type":"result","subtype":"success","result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}"#,
        );
        assert!(
            claude_rule.should_close(),
            "Claude rule closes on Claude input"
        );
    }

    /// B1: for Antigravity the `type:"system"` background-task/open-task drain
    /// arms are VACUOUSLY satisfied — the CLI never emits a `type` key at all
    /// — stated here, not silently inherited. The rule reduces to the marker
    /// predicate, and no Antigravity-shaped input can ever open a task.
    #[test]
    fn close_rule_antigravity_drain_arms_vacuously_satisfied() {
        let mut rule = CloseRule::for_agent(AgentKind::Antigravity);
        rule.observe(ANTG_INIT_LINE);
        rule.observe(ANTG_STEP_LINE);
        assert_eq!(
            rule.background_tasks,
            BackgroundTaskState::NeverAnnounced,
            "Antigravity emits no type:system background_tasks_changed events (stated, B1)"
        );
        assert!(
            rule.open_tasks.is_empty(),
            "Antigravity emits no type:system per-task events (stated, B1)"
        );
        assert!(
            !rule.has_open_background_tasks(),
            "no open tasks means the drain arms never block closing"
        );
    }

    #[test]
    // D-09 (48-09): mutates `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS`, `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` process-wide.
    // Deferred deliberately, not overlooked: THIS process reads the value, so
    // per-`Command` scoping would not reach the reader. ENV_MUTEX bounds the
    // race and the value is restored on every exit path, unwinding included.
    #[expect(clippy::disallowed_methods, reason = "test-only; ENV_MUTEX-guarded")]
    fn idle_timeout_setting_for_is_agent_specific() {
        use std::sync::Mutex;
        static ENV_MUTEX: Mutex<()> = Mutex::new(());
        let _lock = ENV_MUTEX.lock().unwrap();

        // Baseline: nothing set -> both agents get the decided floor.
        unsafe {
            std::env::remove_var("DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS");
            std::env::remove_var("DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS");
        }
        let claude_default = idle_timeout_setting_for(AgentKind::Claude);
        let antg_default = idle_timeout_setting_for(AgentKind::Antigravity);
        assert_eq!(
            claude_default.timeout,
            Duration::from_secs(IDLE_TIMEOUT_FLOOR_SECS)
        );
        assert_eq!(
            antg_default.timeout,
            Duration::from_secs(IDLE_TIMEOUT_FLOOR_SECS),
            "the antigravity default is the DECIDED 120s floor (D-08), not inherited \
             silently — the decision is explicit and documented"
        );

        // Claude's variable moves Claude, NOT Antigravity.
        unsafe {
            std::env::set_var("DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS", "300");
            std::env::remove_var("DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS");
        }
        assert_eq!(
            idle_timeout_setting_for(AgentKind::Claude).timeout,
            Duration::from_secs(300)
        );
        assert_eq!(
            idle_timeout_setting_for(AgentKind::Antigravity).timeout,
            Duration::from_secs(IDLE_TIMEOUT_FLOOR_SECS),
            "Claude's variable must not move Antigravity"
        );

        // Antigravity's variable moves Antigravity, NOT Claude.
        unsafe {
            std::env::set_var("DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS", "240");
            std::env::remove_var("DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS");
        }
        assert_eq!(
            idle_timeout_setting_for(AgentKind::Antigravity).timeout,
            Duration::from_secs(240)
        );
        assert_eq!(
            idle_timeout_setting_for(AgentKind::Claude).timeout,
            Duration::from_secs(IDLE_TIMEOUT_FLOOR_SECS),
            "Antigravity's variable must not move Claude"
        );

        unsafe {
            std::env::remove_var("DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS");
            std::env::remove_var("DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS");
        }
    }

    /// codex-3: the REAL PipeOwning writer path, not just the pure helper —
    /// spawn a child whose stdin is read and validated. A stub `sh` script
    /// records its first stdin line, emits the Antigravity stream (init +
    /// result with marker in `response`), then drains stdin to EOF; the
    /// monitor writes via `user_turn_line_for(Antigravity, ...)` and closes
    /// via the agent-aware rule. The recorded line must be the event-key
    /// shape — an implementation that left the writer on Claude's `type`-form
    /// FAILS here, not just in the unit helper.
    #[test]
    fn pipe_owning_writer_delivers_antigravity_event_key_turn() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let turn_file = root.join("stdin-turn.txt");
        let eof_file = root.join("stdin-eof");
        let script = format!(
            r#"
set -u
IFS= read -r turn || {{ echo "NO_INITIAL_TURN_ON_STDIN" >&2; exit 91; }}
printf '%s\n' "$turn" > '{turn}'
printf '%s\n' '{{"event":"init","model":"stub","inputFormat":"stream-json","outputFormat":"stream-json"}}'
printf '%s\n' '{{"event":"result","result":{{"status":"SUCCESS","response":"DEVFLOW_RESULT: {{\"status\":\"success\"}}"}}}}'
exec 3<&0
( cat <&3 > /dev/null; printf 'EOF\n' > '{eof}' ) > /dev/null 2>&1 &
exit 0
"#,
            turn = turn_file.display(),
            eof = eof_file.display(),
        );

        let code = supervise_stream_child(
            root,
            phase,
            root,
            "the-prompt",
            Duration::from_secs(30),
            "sh",
            &["-c".to_string(), script],
            &[],
            AgentKind::Antigravity,
        )
        .expect("pipe-owning monitor should supervise the antigravity stub");

        assert_eq!(code, 0, "stub exited {code}");
        // The close rule must have released stdin (B1): the stub's background
        // drain saw EOF. Without the agent-aware close rule this fails. The
        // drain runs in a backgrounded subshell, so poll with a bounded wait
        // (same discipline as the Phase-31 tracer test) rather than asserting
        // synchronously after the child exits.
        let mut saw_eof = false;
        for _ in 0..100 {
            if eof_file.exists() {
                saw_eof = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(
            saw_eof,
            "stdin was never released — the agent-aware close rule (B1) did not fire"
        );

        let turn = std::fs::read_to_string(&turn_file).unwrap();
        let v: serde_json::Value = serde_json::from_str(&turn).expect("recorded turn must be JSON");
        assert_eq!(
            v.get("event").and_then(serde_json::Value::as_str),
            Some("user"),
            "the REAL writer must deliver the event-key turn (codex-3): {turn}"
        );
        assert!(v.get("type").is_none(), "no type key: {turn}");
        assert_eq!(
            v.pointer("/message/content")
                .and_then(serde_json::Value::as_str),
            Some("the-prompt")
        );

        // The capture round-trips through the antigravity parser.
        let capture =
            std::fs::read_to_string(crate::agent_result::stdout_path(root, phase)).unwrap();
        let parsed = crate::agent_result::parse_antigravity_event_result(&capture).unwrap();
        assert_eq!(parsed.status, crate::agent_result::AgentStatus::Success);
    }

    // ---- 50-08: both supervision shapes under the runner's stop token ----
    //
    // Each stage runs in a child test process (`spawn_test_in_child`),
    // because the runner's signal pipe is process-wide signal state. The
    // parent finds the agent among the child's children in `/proc`, signals
    // the child by the pid of the `Child` it owns, and reads the files the
    // child wrote.

    /// Where the child records `<outcome> <owner phase>`.
    const STAGE_OUTCOME_ENV: &str = "DEVFLOW_TEST_STAGE_OUTCOME";
    /// `capture` or `stream`.
    const STAGE_SHAPE_ENV: &str = "DEVFLOW_TEST_STAGE_SHAPE";
    /// The fake agent's absolute path.
    const STAGE_PROGRAM_ENV: &str = "DEVFLOW_TEST_STAGE_PROGRAM";
    /// The project root the phase files live under.
    const STAGE_ROOT_ENV: &str = "DEVFLOW_TEST_STAGE_ROOT";
    /// The agent's working directory.
    const STAGE_WORKDIR_ENV: &str = "DEVFLOW_TEST_STAGE_WORKDIR";
    /// The agent kind's label.
    const STAGE_AGENT_ENV: &str = "DEVFLOW_TEST_STAGE_AGENT";
    /// One adapter env pair as `NAME=VALUE`, or empty for none.
    const STAGE_ADAPTER_ENV: &str = "DEVFLOW_TEST_STAGE_ADAPTER_ENV";
    /// The agent's argv after its program, as a JSON array of strings.
    const STAGE_ARGS_ENV: &str = "DEVFLOW_TEST_STAGE_ARGS";

    /// The phase every supervised-stage test runs.
    const STAGE_PHASE: PhaseId = PhaseId::new(50);
    /// The stream shape's idle timeout in these tests: far above every wait
    /// below, so it never fires unless a test asks it to.
    const STAGE_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
    /// How long the parent waits for a child supervisor to answer or exit.
    const STAGE_CHILD_WAIT: Duration = Duration::from_secs(30);
    /// The first turn the stream shape delivers.
    const STAGE_PROMPT: &str = "SUPERVISED-STAGE-PROMPT";

    /// One supervision shape, as the tests name it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Shape {
        Capture,
        Stream,
    }

    impl Shape {
        const BOTH: [Shape; 2] = [Shape::Capture, Shape::Stream];

        fn label(self) -> &'static str {
            self.io().label()
        }

        fn io(self) -> StageIo {
            match self {
                Shape::Capture => StageIo::Capture,
                Shape::Stream => StageIo::Stream {
                    prompt: STAGE_PROMPT.to_string(),
                    idle_timeout: STAGE_IDLE_TIMEOUT,
                },
            }
        }

        fn parse(label: &str) -> Self {
            match label {
                "capture" => Shape::Capture,
                "stream" => Shape::Stream,
                other => panic!("unknown shape {other}"),
            }
        }

        /// The non-Claude adapter each shape runs in these tests (R-11):
        /// Codex under capture, Antigravity under stream.
        fn adapter(self) -> AgentKind {
            match self {
                Shape::Capture => AgentKind::Codex,
                Shape::Stream => AgentKind::Antigravity,
            }
        }

        /// The agent script's first lines: the stream shape reads its first
        /// turn from stdin before anything else, as a real stream agent does,
        /// and exits 91 or 92 when the turn is missing or lacks the prompt.
        fn preamble(self) -> &'static str {
            match self {
                Shape::Capture => "",
                Shape::Stream => {
                    "IFS= read -r turn || { echo NO_FIRST_TURN >&2; exit 91; }\n\
                     case \"$turn\" in *SUPERVISED-STAGE-PROMPT*) ;; *) echo BAD_TURN >&2; exit 92 ;; esac\n"
                }
            }
        }

        /// The result line the shape's adapter prints on stdout.
        fn result_line(self) -> &'static str {
            match self {
                Shape::Capture => r#"DEVFLOW_RESULT: {"status":"success"}"#,
                Shape::Stream => ANTG_RESULT_MARKER_LINE,
            }
        }
    }

    /// The stderr line every waiting agent prints.
    const STAGE_STDERR_LINE: &str = "STAGE-STDERR-LINE";

    /// A fake agent for `shape`, named after its adapter: it reads its first
    /// turn (stream), prints the shape's result line on stdout and
    /// [`STAGE_STDERR_LINE`] on stderr, then waits for `go` and exits `code`.
    fn waiting_agent(fixture: &StageFixture, shape: Shape, go: &Path, code: i32) -> PathBuf {
        fixture.agent(
            &shape.adapter().to_string(),
            &format!(
                "{preamble}printf '%s\\n' '{result}'\n\
                 echo '{STAGE_STDERR_LINE}' >&2\n\
                 while [ ! -e '{go}' ]; do sleep 0.05; done\n\
                 exit {code}",
                preamble = shape.preamble(),
                result = shape.result_line(),
                go = go.display(),
            ),
        )
    }

    /// The child half of every supervised-stage test: build the spec from
    /// the parent's env, install the runner's signal pipe and dispatcher, run
    /// the stage under a run token, and record `<outcome> <owner>`.
    fn supervise_stage_in_child() {
        let var =
            |name: &str| std::env::var_os(name).unwrap_or_else(|| panic!("the parent sets {name}"));
        let shape = Shape::parse(var(STAGE_SHAPE_ENV).to_str().expect("utf-8 shape"));
        let agent: AgentKind = var(STAGE_AGENT_ENV)
            .to_str()
            .expect("utf-8 agent")
            .parse()
            .expect("a known agent label");
        let program = var(STAGE_PROGRAM_ENV)
            .into_string()
            .expect("utf-8 program path");
        let mut spec = StageSpec::new(
            PathBuf::from(var(STAGE_ROOT_ENV)),
            STAGE_PHASE,
            agent,
            program,
            shape.io(),
        );
        spec.workdir = PathBuf::from(var(STAGE_WORKDIR_ENV));
        spec.stage = Some(Stage::Code);
        let adapter_env = var(STAGE_ADAPTER_ENV)
            .into_string()
            .expect("utf-8 adapter env");
        if let Some((name, value)) = adapter_env.split_once('=') {
            spec.envs.push((name.to_string(), value.to_string()));
        }
        if let Some(args) = std::env::var_os(STAGE_ARGS_ENV) {
            spec.args = serde_json::from_str(args.to_str().expect("utf-8 agent args"))
                .expect("the parent passes the agent args as a JSON array");
        }
        crate::stop::install_signal_pipe().expect("install the signal pipe");
        let token = crate::stop::StopToken::for_run();
        let signals =
            crate::stop::spawn_signal_dispatcher(token.clone()).expect("start the dispatcher");
        let outcome = run_stage(spec, &token).expect("run the stage");
        drop(signals);
        let record = format!("{outcome:?} {}", token.phase().as_str());
        std::fs::write(var(STAGE_OUTCOME_ENV), record).expect("record the outcome");
    }

    /// A short tempdir under `/tmp` holding the cache, the project root and
    /// the agent's separate working directory (`<root>/wt`).
    struct StageFixture {
        cache: tempfile::TempDir,
        root: PathBuf,
        workdir: PathBuf,
    }

    impl StageFixture {
        fn new() -> Self {
            let cache = tempfile::Builder::new()
                .prefix("dfs")
                .tempdir_in("/tmp")
                .expect("create a short cache dir under /tmp");
            let root = cache.path().join("proj");
            let workdir = root.join("wt");
            std::fs::create_dir_all(&workdir).expect("create the project and its worktree");
            Self {
                cache,
                root,
                workdir,
            }
        }

        fn path(&self, name: &str) -> PathBuf {
            self.cache.path().join(name)
        }

        /// Write an executable `#!/bin/sh` fake agent named `name`.
        fn agent(&self, name: &str, body: &str) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let path = self.path(name);
            std::fs::write(
                &path,
                format!("#!/bin/sh\nPATH=/usr/bin:/bin\nexport PATH\n{body}\n"),
            )
            .expect("write the fake agent");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make the fake agent executable");
            path
        }

        fn read(&self, path: PathBuf) -> String {
            std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
        }

        fn stdout(&self) -> String {
            self.read(crate::agent_result::stdout_path(&self.root, STAGE_PHASE))
        }

        fn stderr(&self) -> String {
            self.read(crate::agent_result::stderr_path(&self.root, STAGE_PHASE))
        }

        fn exit_code(&self) -> String {
            self.read(crate::agent_result::exit_code_path(&self.root, STAGE_PHASE))
                .trim()
                .to_string()
        }

        /// Whether a stage wrote the agent pid file, which no stage writes
        /// (D-09).
        fn pid_file_exists(&self) -> bool {
            crate::agent_result::agent_pid_path(&self.root, STAGE_PHASE).exists()
        }
    }

    /// Polls `done` every 20 ms until it holds or `within` elapses.
    fn stage_wait_until(within: Duration, mut done: impl FnMut() -> bool) -> bool {
        let deadline = std::time::Instant::now() + within;
        loop {
            if done() {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Kills and reaps the child test process on every exit path. Only the
    /// pid of the `Child` this test spawned is ever signalled.
    struct StageChild(Option<std::process::Child>);

    impl StageChild {
        fn id(&self) -> u32 {
            self.0.as_ref().expect("the child is still owned").id()
        }

        /// Send `signal` to the child, by the pid of the `Child` this test
        /// owns.
        fn signal(&self, signal: libc::c_int) {
            let pid = libc::pid_t::try_from(self.id()).expect("pid fits pid_t");
            assert!(pid > 1, "refusing to signal pid {pid}");
            // SAFETY: `pid > 1` names one process this test spawned and still
            // owns unreaped.
            let rc = unsafe { libc::kill(pid, signal) };
            assert_eq!(rc, 0, "signal {signal} to the child supervisor {pid}");
        }

        /// SIGKILL the child (by the pid of the `Child` this test owns) and
        /// reap it.
        fn kill_now(&mut self) {
            let mut child = self.0.take().expect("the child is still owned");
            child.kill().expect("SIGKILL the child supervisor");
            child.wait().expect("reap the child supervisor");
        }

        fn try_exited(&mut self) -> bool {
            self.0
                .as_mut()
                .expect("the child is still owned")
                .try_wait()
                .expect("poll the child")
                .is_some()
        }

        /// Wait for the child to exit on its own and collect its output.
        fn wait_exit(&mut self) -> std::process::Output {
            let exited = stage_wait_until(STAGE_CHILD_WAIT, || self.try_exited());
            let child = self.0.take().expect("the child is still owned");
            if !exited {
                let mut child = StageChild(Some(child));
                let _ = child.0.as_mut().expect("owned").kill();
                let output = child.0.take().expect("owned").wait_with_output();
                panic!("the child supervisor did not exit within {STAGE_CHILD_WAIT:?}: {output:?}");
            }
            child
                .wait_with_output()
                .expect("collect the child's output")
        }
    }

    impl Drop for StageChild {
        fn drop(&mut self) {
            if let Some(mut child) = self.0.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    /// SIGKILLs whatever is left of the agent's process group on drop.
    struct StageGroup(u32);

    impl Drop for StageGroup {
        fn drop(&mut self) {
            let Ok(signed) = libc::pid_t::try_from(self.0) else {
                return;
            };
            // SAFETY: getpgrp has no preconditions.
            if signed > 1 && signed != unsafe { libc::getpgrp() } {
                // SAFETY: `signed > 1` and not our own group.
                unsafe { libc::kill(-signed, libc::SIGKILL) };
            }
        }
    }

    /// A child test process the parent launched, once its agent runs.
    struct StageLaunch {
        child: StageChild,
        group: StageGroup,
        outcome: PathBuf,
        agent_pid: u32,
    }

    impl StageLaunch {
        fn agent_pid(&self) -> u32 {
            self.agent_pid
        }

        /// Wait for the child to exit, require that it ran exactly its one
        /// test, and return the outcome it recorded.
        fn finish(&mut self, test_name: &str) -> String {
            let output = self.child.wait_exit();
            crate::test_support::assert_child_ran_exactly_one_passing_test(&output, test_name);
            std::fs::read_to_string(&self.outcome).expect("the child recorded its outcome")
        }
    }

    /// What [`launch_stage_with`] launches beyond the fixture: the shape,
    /// the agent, its program and argv, one adapter env pair, and extra
    /// environment for the child test process itself.
    struct StageLaunchArgs<'a> {
        shape: Shape,
        agent: AgentKind,
        program: &'a Path,
        adapter_env: &'a str,
        args: &'a [String],
        extra_env: &'a [(&'a str, &'a std::ffi::OsStr)],
    }

    /// [`launch_stage_with`] with no agent argv and no extra environment.
    fn launch_stage(
        test_name: &str,
        fixture: &StageFixture,
        shape: Shape,
        agent: AgentKind,
        program: &Path,
        adapter_env: &str,
    ) -> StageLaunch {
        launch_stage_with(
            test_name,
            fixture,
            StageLaunchArgs {
                shape,
                agent,
                program,
                adapter_env,
                args: &[],
                extra_env: &[],
            },
        )
    }

    /// Re-run `test_name` as a child test process running `launch.program` in
    /// `launch.shape`, and wait until its agent runs: a direct child of the
    /// child test process that leads its own process group.
    fn launch_stage_with(
        test_name: &str,
        fixture: &StageFixture,
        launch: StageLaunchArgs<'_>,
    ) -> StageLaunch {
        let StageLaunchArgs {
            shape,
            agent,
            program,
            adapter_env,
            args,
            extra_env,
        } = launch;
        let outcome = fixture.path("outcome");
        let agent_label = agent.to_string();
        let args_json = serde_json::to_string(args).expect("encode the agent args");
        let mut env: Vec<(&str, &std::ffi::OsStr)> = vec![
            (STAGE_OUTCOME_ENV, outcome.as_os_str()),
            (STAGE_SHAPE_ENV, std::ffi::OsStr::new(shape.label())),
            (STAGE_PROGRAM_ENV, program.as_os_str()),
            (STAGE_ROOT_ENV, fixture.root.as_os_str()),
            (STAGE_WORKDIR_ENV, fixture.workdir.as_os_str()),
            (STAGE_AGENT_ENV, std::ffi::OsStr::new(&agent_label)),
            (STAGE_ADAPTER_ENV, std::ffi::OsStr::new(adapter_env)),
            (STAGE_ARGS_ENV, std::ffi::OsStr::new(&args_json)),
        ];
        env.extend_from_slice(extra_env);
        let mut child = StageChild(Some(crate::test_support::spawn_test_in_child(
            test_name,
            fixture.cache.path(),
            &env,
        )));
        let deadline = std::time::Instant::now() + STAGE_CHILD_WAIT;
        let agent_pid = loop {
            if let Some(agent) = group_leading_child_of(child.id()) {
                break agent;
            }
            let exited = child.try_exited();
            if exited || std::time::Instant::now() >= deadline {
                let output = child.0.take().expect("owned").wait_with_output();
                panic!("the child test process never ran its agent (exited: {exited}): {output:?}");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        StageLaunch {
            child,
            group: StageGroup(agent_pid),
            outcome,
            agent_pid,
        }
    }

    /// The direct child of `parent` that leads its own process group (the
    /// stage's agent), from one read of `/proc`; `None` while there is none.
    fn group_leading_child_of(parent: u32) -> Option<u32> {
        let entries = std::fs::read_dir("/proc").expect("list /proc");
        entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
            .find(|pid| {
                std::fs::read_to_string(format!("/proc/{pid}/stat"))
                    .ok()
                    .and_then(|line| parse_proc_stat_line(&line))
                    .is_some_and(|stat| {
                        stat.ppid == parent && i64::from(stat.pgid) == i64::from(*pid)
                    })
            })
    }

    /// `Ok` when `agent_pid` leads its own process group and that group is
    /// not `supervisor_pid`'s; the reason otherwise. Uses the production
    /// [`confirm_own_group`] check for the first half.
    fn check_own_group(agent_pid: u32, supervisor_pid: u32) -> Result<(), String> {
        confirm_own_group(agent_pid)
            .map_err(|pgid| format!("agent {agent_pid} is in group {pgid}, not its own"))?;
        let supervisor = libc::pid_t::try_from(supervisor_pid).expect("pid fits pid_t");
        // SAFETY: getpgid has no memory preconditions.
        let supervisor_group = unsafe { libc::getpgid(supervisor) };
        if i64::from(supervisor_group) == i64::from(agent_pid) {
            return Err(format!(
                "agent {agent_pid} shares its group with supervisor {supervisor_pid}"
            ));
        }
        Ok(())
    }

    const OWN_GROUP_TEST: &str = "monitor::tests::both_arms_put_the_agent_in_its_own_group";

    #[test]
    fn both_arms_put_the_agent_in_its_own_group() {
        if crate::test_support::in_child_test(OWN_GROUP_TEST) {
            supervise_stage_in_child();
            return;
        }
        for shape in Shape::BOTH {
            let fixture = StageFixture::new();
            let go = fixture.path("go");
            let program = waiting_agent(&fixture, shape, &go, 0);
            let mut run = launch_stage(
                OWN_GROUP_TEST,
                &fixture,
                shape,
                shape.adapter(),
                &program,
                "",
            );
            let agent = run.agent_pid();
            check_own_group(agent, run.child.id())
                .unwrap_or_else(|reason| panic!("{shape:?}: {reason}"));
            let agent_signed = libc::pid_t::try_from(agent).expect("pid fits pid_t");
            let child_signed = libc::pid_t::try_from(run.child.id()).expect("pid fits pid_t");
            // SAFETY: getpgid has no memory preconditions.
            let (agent_group, child_group) =
                unsafe { (libc::getpgid(agent_signed), libc::getpgid(child_signed)) };
            assert_eq!(agent_group, agent_signed, "{shape:?}: pgid == pid");
            assert_ne!(
                agent_group, child_group,
                "{shape:?}: the agent's group is not the supervisor's"
            );
            std::fs::write(&go, "").expect("let the agent exit");
            assert_eq!(run.finish(OWN_GROUP_TEST), "Completed(0) advancing");
            drop(run.group);
        }

        // Control: the same check refuses a process spawned without
        // `process_group(0)`, which stays in this test process's group.
        let mut stray = StageChild(Some(
            std::process::Command::new("/bin/sleep")
                .arg("30")
                .stdin(std::process::Stdio::null())
                .spawn()
                .expect("spawn a process in this test's group"),
        ));
        let verdict = check_own_group(stray.id(), std::process::id());
        stray.kill_now();
        assert!(
            verdict.is_err(),
            "control: a process left in the launcher's group must fail the check"
        );
    }

    /// Whether any live member of process group `pgid` still exists. A zombie
    /// is not one: an orphaned member is PID 1's to reap, and a CI job
    /// container's PID 1 never reaps (`test_support::live_group_members`).
    fn stage_group_exists(pgid: u32) -> bool {
        !crate::test_support::live_group_members(pgid).is_empty()
    }

    /// A fake agent for `shape` that leaves a second group member running
    /// (`sleep 60`, its stdio detached so it holds no capture pipe), then
    /// waits for a file that never appears. Only a signal ends it.
    fn group_agent(fixture: &StageFixture, shape: Shape) -> PathBuf {
        fixture.agent(
            &shape.adapter().to_string(),
            &format!(
                "{preamble}sleep 60 </dev/null >/dev/null 2>&1 &\n\
                 printf '%s\\n' '{result}'\n\
                 while [ ! -e '{never}' ]; do sleep 0.05; done\n\
                 exit 0",
                preamble = shape.preamble(),
                result = shape.result_line(),
                never = fixture.path("never").display(),
            ),
        )
    }

    const STOP_SIGNAL_TEST: &str = "monitor::tests::sigterm_to_the_supervisor_kills_the_agent_group_records_143_and_does_not_advance";

    /// V-16. Do NOT widen the 5 s windows below to make this pass: a missed
    /// TERM or INT is a defect, not slowness.
    #[test]
    fn sigterm_to_the_supervisor_kills_the_agent_group_records_143_and_does_not_advance() {
        if crate::test_support::in_child_test(STOP_SIGNAL_TEST) {
            supervise_stage_in_child();
            return;
        }
        for shape in Shape::BOTH {
            for (name, signal) in [("SIGTERM", libc::SIGTERM), ("SIGINT", libc::SIGINT)] {
                let fixture = StageFixture::new();
                let program = group_agent(&fixture, shape);
                let mut run = launch_stage(
                    STOP_SIGNAL_TEST,
                    &fixture,
                    shape,
                    shape.adapter(),
                    &program,
                    "",
                );
                let pgid = run.agent_pid();
                assert!(
                    stage_group_exists(pgid),
                    "{shape:?} {name}: control: the agent group runs before the signal"
                );

                // The child test process's own pid, from the `Child` we own.
                run.child.signal(signal);
                let outcome = run.finish(STOP_SIGNAL_TEST);
                assert_eq!(
                    outcome, "Stopped finished",
                    "{shape:?} {name}: a signal to the runner is a stop, never a completion"
                );
                assert!(
                    stage_wait_until(Duration::from_secs(5), || !stage_group_exists(pgid)),
                    "{shape:?} {name}: every member of the agent group must be gone: {}",
                    proc_snapshot(run.agent_pid())
                );
                assert_eq!(fixture.exit_code(), "143", "{shape:?} {name}");
                drop(run.group);
            }

            // Control: SIGTERM straight to the agent, not the runner. The agent
            // dies of it (the test observes a real death), and the stage
            // reports that as a natural completion that advances.
            let fixture = StageFixture::new();
            let program = group_agent(&fixture, shape);
            let mut run = launch_stage(
                STOP_SIGNAL_TEST,
                &fixture,
                shape,
                shape.adapter(),
                &program,
                "",
            );
            let agent = libc::pid_t::try_from(run.agent_pid()).expect("pid fits pid_t");
            assert!(agent > 1, "refusing to signal pid {agent}");
            // SAFETY: `agent > 1` is the live agent the child test process
            // still holds unreaped (it is waiting on it).
            assert_eq!(unsafe { libc::kill(agent, libc::SIGTERM) }, 0);
            assert_eq!(
                run.finish(STOP_SIGNAL_TEST),
                "Completed(143) advancing",
                "{shape:?} control: a TERM to the agent alone is not a runner stop"
            );
            assert_eq!(fixture.exit_code(), "143", "{shape:?} control");
            drop(run.group);
        }
    }

    /// One checker per supervisor responsibility of the spike's R-A..R-M
    /// table (V-08), each returning the violation it found. Linux-only:
    /// they read `/proc`.
    #[cfg(target_os = "linux")]
    mod responsibilities {
        use std::path::Path;

        /// R-A: the agent's cwd is the worktree, not the project root.
        pub(super) fn r_a_cwd(agent_pid: u32, workdir: &Path) -> Result<(), String> {
            let cwd = std::fs::read_link(format!("/proc/{agent_pid}/cwd"))
                .map_err(|err| format!("agent cwd unreadable: {err}"))?;
            let want = workdir.canonicalize().map_err(|err| err.to_string())?;
            (cwd == want)
                .then_some(())
                .ok_or_else(|| format!("agent cwd {} is not {}", cwd.display(), want.display()))
        }

        /// R-B: stdout and stderr land in separate files, and stderr never
        /// reaches the stdout capture.
        pub(super) fn r_b_streams(
            stdout: &str,
            stderr: &str,
            result_line: &str,
            stderr_line: &str,
        ) -> Result<(), String> {
            if !stdout.lines().any(|line| line == result_line) {
                return Err(format!("stdout lacks the result line: {stdout:?}"));
            }
            if stdout.contains(stderr_line) {
                return Err(format!("stderr text reached stdout: {stdout:?}"));
            }
            if !stderr.contains(stderr_line) {
                return Err(format!("stderr lacks its line: {stderr:?}"));
            }
            Ok(())
        }

        /// R-C: no agent pid file is written (D-09), and the agent pid runs
        /// the adapter program, not the runner.
        pub(super) fn r_c_pid(
            pid_file_written: bool,
            agent_pid: u32,
            program: &Path,
        ) -> Result<(), String> {
            if pid_file_written {
                return Err("an agent pid file was written".to_string());
            }
            let cmdline = std::fs::read(format!("/proc/{agent_pid}/cmdline"))
                .map_err(|err| format!("agent cmdline unreadable: {err}"))?;
            let cmdline = String::from_utf8_lossy(&cmdline).replace('\0', " ");
            cmdline
                .contains(&*program.to_string_lossy())
                .then_some(())
                .ok_or_else(|| format!("agent {agent_pid} runs {cmdline:?}, not the adapter"))
        }

        /// R-D: the exit file holds the exact code (`128 + n` for signal n).
        pub(super) fn r_d_exit(exit_file: &str, code: i32) -> Result<(), String> {
            (exit_file == code.to_string())
                .then_some(())
                .ok_or_else(|| format!("exit file {exit_file:?}, want {code}"))
        }

        /// R-E: natural completion is `Completed(code)` and the run is left
        /// advancing.
        pub(super) fn r_e_completes(outcome: &str, code: i32) -> Result<(), String> {
            (outcome == format!("Completed({code}) advancing"))
                .then_some(())
                .ok_or_else(|| format!("outcome {outcome:?} is not a completion with {code}"))
        }

        /// R-F: the adapter variable reached a grandchild of the agent.
        pub(super) fn r_f_env(recorded: Option<&str>, value: &str) -> Result<(), String> {
            (recorded == Some(value))
                .then_some(())
                .ok_or_else(|| format!("grandchild saw {recorded:?}, want {value:?}"))
        }

        /// R-K: a stopped run still records an exit code, 143.
        pub(super) fn r_k_stop_exit(exit_file: &str) -> Result<(), String> {
            r_d_exit(exit_file, 143)
        }

        /// R-M: a stop is not a completion: the run ended `Stopped` and
        /// finished, so nothing advances.
        pub(super) fn r_m_stop_is_not_completion(outcome: &str) -> Result<(), String> {
            (outcome == "Stopped finished")
                .then_some(())
                .ok_or_else(|| format!("a stopped run reported {outcome:?}"))
        }

        /// The agent was handed no blocked signal: its leader (a shell, which
        /// blocks SIGCHLD itself while it waits) has no TERM or INT bit in
        /// `SigBlk`, and a child the shell runs, which inherits the mask the
        /// agent was handed, has an empty `SigBlk`.
        pub(super) fn sigblk_clear(agent_pid: u32, grandchild_pid: u32) -> Result<(), String> {
            let stop_bits = (1_u64 << (libc::SIGTERM - 1)) | (1_u64 << (libc::SIGINT - 1));
            let leader = super::signal_mask(agent_pid, "SigBlk");
            let inherited = super::signal_mask(grandchild_pid, "SigBlk");
            (leader & stop_bits == 0 && inherited == 0)
                .then_some(())
                .ok_or_else(|| format!("SigBlk leader {leader:016x}, inherited {inherited:016x}"))
        }
    }

    #[cfg(target_os = "linux")]
    const RESPONSIBILITIES_TEST: &str =
        "monitor::tests::both_arms_meet_every_supervisor_responsibility_with_a_non_claude_adapter";

    /// The adapter variable R-F threads: one `hermetic_command` scrubs, so
    /// it only reaches the agent when `envs` is applied after the scrub.
    #[cfg(target_os = "linux")]
    const R_F_VARIABLE: &str = "GIT_CONFIG_COUNT";
    #[cfg(target_os = "linux")]
    const R_F_VALUE: &str = "1";

    /// V-08 (R-11): the spike's responsibilities under both shapes, with a
    /// fake `codex` under capture and a fake `antigravity` under stream.
    /// R-J and R-L (the socket sweep and the socket modes) went with the
    /// socket; R-G (detached from the launching CLI) is the runner unit's.
    #[cfg(target_os = "linux")]
    #[test]
    fn both_arms_meet_every_supervisor_responsibility_with_a_non_claude_adapter() {
        use responsibilities as r;
        if crate::test_support::in_child_test(RESPONSIBILITIES_TEST) {
            supervise_stage_in_child();
            return;
        }
        let mut rows: Vec<(Shape, &str, Result<(), String>)> = Vec::new();
        let adapter_env = format!("{R_F_VARIABLE}={R_F_VALUE}");
        for shape in Shape::BOTH {
            // Run 1: a natural exit 7, observed alive first.
            let fixture = StageFixture::new();
            let (go, env_out, gc_pid) = (
                fixture.path("go"),
                fixture.path("grandchild-env"),
                fixture.path("grandchild-pid"),
            );
            let program = fixture.agent(
                &shape.adapter().to_string(),
                &format!(
                    "{preamble}sh -c 'printf %s \"${R_F_VARIABLE}\" > \"$1\"; exec sleep 60' sh '{env_out}' </dev/null >/dev/null 2>&1 &\n\
                     echo $! > '{gc_pid}'\n\
                     printf '%s\\n' '{result}'\n\
                     echo '{STAGE_STDERR_LINE}' >&2\n\
                     while [ ! -e '{go}' ]; do sleep 0.05; done\n\
                     kill \"$(cat '{gc_pid}')\"\n\
                     exit 7",
                    preamble = shape.preamble(),
                    result = shape.result_line(),
                    env_out = env_out.display(),
                    gc_pid = gc_pid.display(),
                    go = go.display(),
                ),
            );
            let mut run = launch_stage(
                RESPONSIBILITIES_TEST,
                &fixture,
                shape,
                shape.adapter(),
                &program,
                &adapter_env,
            );
            let agent = run.agent_pid();
            assert!(
                stage_wait_until(Duration::from_secs(5), || env_out.exists()
                    && gc_pid.exists()),
                "{shape:?}: the agent's grandchild started"
            );
            let grandchild: u32 = fixture
                .read(gc_pid.clone())
                .trim()
                .parse()
                .expect("grandchild pid");
            rows.push((shape, "R-A", r::r_a_cwd(agent, &fixture.workdir)));
            rows.push((
                shape,
                "R-C",
                r::r_c_pid(fixture.pid_file_exists(), agent, &program),
            ));
            rows.push((
                shape,
                "R-F",
                r::r_f_env(std::fs::read_to_string(&env_out).ok().as_deref(), R_F_VALUE),
            ));
            rows.push((shape, "SigBlk", r::sigblk_clear(agent, grandchild)));
            std::fs::write(&go, "").expect("let the agent exit");
            let outcome = run.finish(RESPONSIBILITIES_TEST);
            rows.push((
                shape,
                "R-B",
                r::r_b_streams(
                    &fixture.stdout(),
                    &fixture.stderr(),
                    shape.result_line(),
                    STAGE_STDERR_LINE,
                ),
            ));
            rows.push((shape, "R-D exit", r::r_d_exit(&fixture.exit_code(), 7)));
            rows.push((shape, "R-E", r::r_e_completes(&outcome, 7)));
            drop(run.group);

            // Run 2: the agent kills itself with SIGUSR1 (128 + 10). Under
            // capture it first merges stderr into stdout: the R-B control.
            let fixture = StageFixture::new();
            let go = fixture.path("go");
            let merge = if shape == Shape::Capture {
                "exec 2>&1\n"
            } else {
                ""
            };
            let program = fixture.agent(
                &shape.adapter().to_string(),
                &format!(
                    "{preamble}{merge}printf '%s\\n' '{result}'\n\
                     echo '{STAGE_STDERR_LINE}' >&2\n\
                     while [ ! -e '{go}' ]; do sleep 0.05; done\n\
                     kill -USR1 $$\n\
                     sleep 5\n\
                     exit 0",
                    preamble = shape.preamble(),
                    result = shape.result_line(),
                    go = go.display(),
                ),
            );
            let mut run = launch_stage(
                RESPONSIBILITIES_TEST,
                &fixture,
                shape,
                shape.adapter(),
                &program,
                "",
            );
            std::fs::write(&go, "").expect("let the agent signal itself");
            let outcome = run.finish(RESPONSIBILITIES_TEST);
            let by_signal = 128 + libc::SIGUSR1;
            rows.push((
                shape,
                "R-D signal",
                r::r_d_exit(&fixture.exit_code(), by_signal),
            ));
            rows.push((shape, "R-E signal", r::r_e_completes(&outcome, by_signal)));
            if shape == Shape::Capture {
                assert!(
                    r::r_b_streams(
                        &fixture.stdout(),
                        &fixture.stderr(),
                        shape.result_line(),
                        STAGE_STDERR_LINE
                    )
                    .is_err(),
                    "control: the R-B checker must reject a capture whose agent merged stderr \
                     into stdout: {:?}",
                    fixture.stdout()
                );
            }
            drop(run.group);

            // Run 3: a stop (SIGTERM to the runner) still records 143 and
            // is not a completion.
            let fixture = StageFixture::new();
            let program = group_agent(&fixture, shape);
            let mut run = launch_stage(
                RESPONSIBILITIES_TEST,
                &fixture,
                shape,
                shape.adapter(),
                &program,
                "",
            );
            run.child.signal(libc::SIGTERM);
            let outcome = run.finish(RESPONSIBILITIES_TEST);
            rows.push((shape, "R-K", r::r_k_stop_exit(&fixture.exit_code())));
            rows.push((shape, "R-M", r::r_m_stop_is_not_completion(&outcome)));
            drop(run.group);
        }
        let failures: Vec<String> = rows
            .iter()
            .filter_map(|(shape, row, result)| {
                result
                    .as_ref()
                    .err()
                    .map(|reason| format!("{shape:?} {row}: {reason}"))
            })
            .collect();
        assert_eq!(rows.len(), 22, "every row ran: {rows:?}");
        assert!(
            failures.is_empty(),
            "responsibilities not met: {failures:#?}"
        );
    }

    // ---- process-group and /proc helpers ---------------------------------

    /// The `/proc/<pid>/stat` fields [`group_leading_child_of`] picks the
    /// agent by, from one read.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct ChildStat {
        /// Field 4, the parent pid.
        ppid: u32,
        /// Field 5, the process group id.
        pgid: i32,
        /// Field 22, the start time in clock ticks since boot.
        start_time: u64,
    }

    /// Parse one `/proc/<pid>/stat` line into [`ChildStat`]. `comm` (field 2)
    /// may hold spaces and parentheses, so the line is split after its last
    /// `)`: index 0 is then field 3, so the parent pid (field 4) is index 1,
    /// the process group (field 5) index 2 and the start time (field 22)
    /// index 19. A missing or unparsable field is `None`, never a guess.
    fn parse_proc_stat_line(line: &str) -> Option<ChildStat> {
        let rest = &line[line.rfind(')')? + 1..];
        let fields: Vec<&str> = rest.split_whitespace().collect();
        Some(ChildStat {
            ppid: fields.get(1)?.parse().ok()?,
            pgid: fields.get(2)?.parse().ok()?,
            start_time: fields.get(19)?.parse().ok()?,
        })
    }

    #[test]
    fn proc_stat_parse_reads_ppid_pgid_and_start_time_after_the_last_paren() {
        // comm `(a) b (c)` holds spaces and parentheses; every field the
        // parse could confuse carries a distinct value (session 555, tty 0,
        // itrealvalue 0, vsize 1000).
        let head = "1234 ((a) b (c)) S 4242 777 555 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0";
        let line = format!("{head} 98765 1000 200 18446744073709551615");
        assert_eq!(
            parse_proc_stat_line(&line),
            Some(ChildStat {
                ppid: 4242,
                pgid: 777,
                start_time: 98765,
            })
        );
        assert_eq!(
            parse_proc_stat_line(head),
            None,
            "a line cut off before field 22"
        );

        let own = std::fs::read_to_string("/proc/self/stat").expect("read /proc/self/stat");
        let parsed = parse_proc_stat_line(&own).expect("parse /proc/self/stat");
        assert_eq!(parsed.ppid, std::os::unix::process::parent_id());
        assert_eq!(
            Some(parsed.start_time),
            crate::agent::process_start_time(std::process::id())
        );
        // SAFETY: getpgrp has no preconditions.
        assert_eq!(parsed.pgid, unsafe { libc::getpgrp() });
    }

    // ---- 50-26: WR-03 ---------------------------------------------------
    //
    // A stream supervisor whose agent exited while a process it started
    // still holds the agent's stdout must not wait on that stdout. Every
    // 50-26 test, guard and helper lives in this block, so the
    // `fix(50-26):` hunks stay disjoint from later edits (agy-5b).

    const STREAM_HOLD_NO_LITERAL_READ: &str =
        "no literal std::env::var_os read of DEVFLOW_TEST_STREAM_WAIT_HOLD";
    const STREAM_HOLD_FN_NOT_ONCE: &str = "fn stream_wait_test_hold( is not defined exactly once";
    const STREAM_HOLD_NOT_GATED: &str = "fn stream_wait_test_hold( is not directly under #[cfg(any(test, feature = \"test-support\"))]";
    const STREAM_HOLD_CALL_NOT_GATED: &str = "a call to stream_wait_test_hold( is not directly under #[cfg(any(test, feature = \"test-support\"))]";

    /// The stream-wait hold's layout over `source`'s non-test code (cut at
    /// its first `mod tests {` line, whole-line `//` comments dropped): the
    /// variable is read literally (the form the doc check sees), the hold is
    /// defined once directly under the test-support gate, and every call is
    /// directly under that gate too.
    fn stream_wait_hold_violations(source: &str) -> Vec<String> {
        const GATE: &str = r#"#[cfg(any(test, feature = "test-support"))]"#;
        const DEFINITION: &str = "fn stream_wait_test_hold(";
        let lines: Vec<&str> = source
            .lines()
            .take_while(|line| *line != "mod tests {")
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect();
        let gated = |at: usize| {
            at.checked_sub(1)
                .is_some_and(|above| lines[above].trim() == GATE)
        };
        let mut violations = Vec::new();
        if !lines
            .iter()
            .any(|line| line.contains(r#"std::env::var_os("DEVFLOW_TEST_STREAM_WAIT_HOLD")"#))
        {
            violations.push(STREAM_HOLD_NO_LITERAL_READ.to_string());
        }
        let defined: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.trim_start().starts_with(DEFINITION))
            .map(|(at, _)| at)
            .collect();
        match defined.as_slice() {
            [at] => {
                if !gated(*at) {
                    violations.push(STREAM_HOLD_NOT_GATED.to_string());
                }
            }
            _ => violations.push(STREAM_HOLD_FN_NOT_ONCE.to_string()),
        }
        let ungated_call = lines.iter().enumerate().any(|(at, line)| {
            line.contains("stream_wait_test_hold(")
                && !line.trim_start().starts_with(DEFINITION)
                && !gated(at)
        });
        if ungated_call {
            violations.push(STREAM_HOLD_CALL_NOT_GATED.to_string());
        }
        violations
    }

    /// G (50-26, permanent source scan, T-50-101): a release build never
    /// compiles the stream-wait hold or its call, and the hold's variable is
    /// read literally. The real file yields no violation; each planted
    /// source yields the violation it plants.
    #[test]
    fn the_stream_wait_hold_is_test_only_and_read_literally() {
        let source = include_str!("monitor.rs");
        assert!(
            source.lines().any(|line| line == "mod tests {"),
            "monitor.rs must keep its `mod tests {{` line, or the scan would read its own controls"
        );
        let real = stream_wait_hold_violations(source);
        assert!(real.is_empty(), "monitor.rs: {real:?}");

        let gate = "#[cfg(any(test, feature = \"test-support\"))]\n";
        let read = "    let _path = std::env::var_os(\"DEVFLOW_TEST_STREAM_WAIT_HOLD\");\n";
        let hold = |attribute: &str, body: &str| {
            format!("{attribute}fn stream_wait_test_hold(token: &StopToken) {{\n{body}}}\n")
        };
        let caller = |attribute: &str| {
            format!(
                "fn run_stage() {{\n    // Test-only.\n    {attribute}    stream_wait_test_hold(&token);\n}}\n"
            )
        };
        let gated_call = format!("    {gate}");
        let good = format!("{}{}", hold(gate, read), caller(&gated_call));
        let only = |violation: &str| vec![violation.to_string()];

        assert_eq!(
            stream_wait_hold_violations(&good),
            Vec::<String>::new(),
            "a correct layout (the comment line above the gate is dropped)"
        );
        assert_eq!(
            stream_wait_hold_violations(&format!("{}{}", hold("", read), caller(&gated_call))),
            only(STREAM_HOLD_NOT_GATED),
            "an ungated definition"
        );
        assert_eq!(
            stream_wait_hold_violations(&format!(
                "{}{}",
                hold("#[cfg(test)]\n", read),
                caller(&gated_call)
            )),
            only(STREAM_HOLD_NOT_GATED),
            "a definition gated by cfg(test) only"
        );
        assert_eq!(
            stream_wait_hold_violations(&format!("{}{}", hold(gate, read), caller(""))),
            only(STREAM_HOLD_CALL_NOT_GATED),
            "an ungated call"
        );
        assert_eq!(
            stream_wait_hold_violations(&format!(
                "const HOLD: &str = \"DEVFLOW_TEST_STREAM_WAIT_HOLD\";\n{}{}",
                hold(gate, "    let _path = std::env::var_os(HOLD);\n"),
                caller(&gated_call)
            )),
            only(STREAM_HOLD_NO_LITERAL_READ),
            "a read through a constant"
        );
        assert_eq!(
            stream_wait_hold_violations(&format!(
                "{}{}{}",
                hold(gate, read),
                hold(gate, ""),
                caller(&gated_call)
            )),
            only(STREAM_HOLD_FN_NOT_ONCE),
            "two definitions"
        );
        assert_eq!(
            stream_wait_hold_violations(&format!("mod tests {{\n{good}")),
            vec![
                STREAM_HOLD_NO_LITERAL_READ.to_string(),
                STREAM_HOLD_FN_NOT_ONCE.to_string()
            ],
            "the read and the hold only after the test-module cut"
        );
    }

    /// The process a fake agent left holding its stdout, as the agent itself
    /// recorded it while the holder was still its unreaped child
    /// (DeepSeek-3): `<pid> <start>`, where `<start>` is `/proc/<pid>/stat`
    /// field 22 (set at fork, unchanged by `exec`). Dropped, it SIGKILLs the
    /// holder by that recorded pid only, and only while it is still that
    /// process; never by a name pattern.
    struct HeldStdout {
        pid: u32,
        start: u64,
    }

    impl HeldStdout {
        /// Wait up to 10 s for the agent's holder record at `record`, and
        /// require that it names a live process.
        fn take(record: &Path) -> Self {
            if !stage_wait_until(Duration::from_secs(10), || record.exists()) {
                panic!(
                    "FIXTURE: the agent wrote no holder record at {} within 10 s",
                    record.display()
                );
            }
            let text = std::fs::read_to_string(record)
                .unwrap_or_else(|err| panic!("FIXTURE: read {}: {err}", record.display()));
            let fields: Vec<&str> = text.split_whitespace().collect();
            let parsed = match fields.as_slice() {
                [pid, start] => pid.parse::<u32>().ok().zip(start.parse::<u64>().ok()),
                _ => None,
            };
            let Some((pid, start)) = parsed else {
                panic!("FIXTURE: the holder record is not `<pid> <start>`: {text:?}");
            };
            if pid <= 1 || crate::agent::process_start_time(pid) != Some(start) {
                panic!("FIXTURE: the holder record {text:?} names no live process");
            }
            Self { pid, start }
        }

        /// Whether the recorded holder is still that process, against the
        /// start time recorded before the supervisor could return.
        /// A zombie holder has exited: it is not alive, though its `/proc`
        /// entry keeps the recorded start time until something reaps it.
        fn is_alive(&self) -> bool {
            crate::agent::is_same_process(self.pid, self.start)
                && crate::test_support::process_is_live(self.pid)
        }
    }

    impl Drop for HeldStdout {
        fn drop(&mut self) {
            let Ok(pid) = libc::pid_t::try_from(self.pid) else {
                return;
            };
            if pid > 1 && self.is_alive() {
                // SAFETY: `pid > 1` is the holder this test's agent recorded,
                // confirmed by its start time to still be that process.
                unsafe { libc::kill(pid, libc::SIGKILL) };
            }
        }
    }

    /// The shell lines a fake agent runs right after starting its holder as
    /// a background job: record `<pid> <start>` while the holder is still
    /// its unreaped child, through a temp file and `mv`.
    fn record_holder(record: &Path) -> String {
        format!(
            "holder=$!\n\
             echo \"$holder $(cut -d' ' -f22 /proc/$holder/stat)\" > '{record}.tmp'\n\
             mv '{record}.tmp' '{record}'\n",
            record = record.display()
        )
    }

    /// Every `stream_wait_bounded` event for `phase` in `root`'s event log,
    /// each as its JSON object. Fields are indexed on the map, so a missing
    /// one panics instead of reading as a default. No log is zero events.
    fn stream_wait_events(
        root: &Path,
        phase: PhaseId,
    ) -> Vec<serde_json::Map<String, serde_json::Value>> {
        let path = crate::events::events_path(root);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(err) => panic!("read {}: {err}", path.display()),
        };
        let phase = serde_json::json!(phase);
        text.lines()
            .map(
                |line| match serde_json::from_str::<serde_json::Value>(line) {
                    Ok(serde_json::Value::Object(map)) => map,
                    other => panic!("an event line is not a JSON object: {line:?} ({other:?})"),
                },
            )
            .filter(|map| map["event"] == "stream_wait_bounded" && map["phase"] == phase)
            .collect()
    }

    /// Require exactly one `stream_wait_bounded` event for `phase`, naming
    /// only the reader as unfinished, with the capture closed.
    fn assert_one_reader_bound(root: &Path, phase: PhaseId, context: &str) {
        let events = stream_wait_events(root, phase);
        assert!(
            events.len() == 1,
            "WR-03: {context}: expected exactly one stream_wait_bounded event, found {events:?}"
        );
        let event = &events[0];
        assert_eq!(
            event["unfinished"],
            serde_json::json!(["reader"]),
            "{context}: {event:?}"
        );
        assert_eq!(
            event["capture_closed"],
            serde_json::json!(true),
            "{context}: {event:?}"
        );
    }

    /// A fake Claude stream agent: it reads its first turn, prints
    /// [`RESULT_WITH_MARKER`], starts `holder` as a background job (it
    /// inherits the agent's stdout unless `holder` redirects it, and stays
    /// in the agent's group), records it at `record`, waits for `go` and
    /// exits 0.
    fn holder_agent(fixture: &StageFixture, holder: &str, record: &Path, go: &Path) -> PathBuf {
        fixture.agent(
            "claude",
            &format!(
                "{preamble}printf '%s\\n' '{RESULT_WITH_MARKER}'\n\
                 {holder} &\n\
                 {record_lines}\
                 while [ ! -e '{go}' ]; do sleep 0.05; done\n\
                 exit 0",
                preamble = Shape::Stream.preamble(),
                record_lines = record_holder(record),
                go = go.display(),
            ),
        )
    }

    /// A and B: a supervised Claude stream stage (production 120 s idle
    /// timeout) whose agent leaves `holder` running, then exits 0. The
    /// supervisor must exit within 20 s of the agent's exit and record a
    /// normal completion; `stdout_held` says whether exactly one
    /// `stream_wait_bounded` event or none is expected.
    fn completed_run_with_a_leftover(test_name: &str, holder: &str, stdout_held: bool) {
        let fixture = StageFixture::new();
        let go = fixture.path("go");
        let record = fixture.path("holder");
        let program = holder_agent(&fixture, holder, &record, &go);
        let mut run = launch_stage_with(
            test_name,
            &fixture,
            StageLaunchArgs {
                shape: Shape::Stream,
                agent: AgentKind::Claude,
                program: &program,
                adapter_env: "",
                args: &[],
                extra_env: &[],
            },
        );
        let held = HeldStdout::take(&record);
        std::fs::write(&go, "").expect("let the agent exit");
        if !stage_wait_until(Duration::from_secs(20), || run.child.try_exited()) {
            panic!(
                "WR-03: the stage still runs 20 s after its agent was released to exit; \
                 holder {} alive: {}",
                held.pid,
                held.is_alive()
            );
        }
        let outcome = run.finish(test_name);
        assert_eq!(
            outcome,
            "Completed(0) advancing",
            "stderr: {:?}",
            fixture.stderr()
        );
        let stdout = fixture.stdout();
        assert!(
            stdout.lines().any(|line| line == RESULT_WITH_MARKER),
            "the capture holds the result line: {stdout:?}"
        );
        assert_eq!(fixture.exit_code(), "0");
        assert!(
            !crate::agent_result::idle_timeout_path(&fixture.root, STAGE_PHASE).exists(),
            "no idle-timeout verdict for a completed run"
        );
        assert!(
            held.is_alive(),
            "the holder still runs: the supervisor's wait ended, not the holder"
        );
        if stdout_held {
            assert_one_reader_bound(&fixture.root, STAGE_PHASE, test_name);
        } else {
            let events = stream_wait_events(&fixture.root, STAGE_PHASE);
            assert!(
                events.is_empty(),
                "a released stdout is not a bounded wait: {events:?}"
            );
        }
        drop(run.group);
    }

    const HELD_STDOUT_TEST: &str = "monitor::tests::a_completed_stream_run_returns_while_a_leftover_process_holds_the_agents_stdout";

    /// A (fail-first): a completed stream run whose agent left `sleep 300`
    /// holding its stdout returns at the agent's exit, under the production
    /// 120 s idle timeout. Before the fix the supervisor waited 120 s of
    /// silence in `drive_stream_io` and then forever in the thread join.
    #[test]
    fn a_completed_stream_run_returns_while_a_leftover_process_holds_the_agents_stdout() {
        if crate::test_support::in_child_test(HELD_STDOUT_TEST) {
            supervise_stage_in_child();
            return;
        }
        completed_run_with_a_leftover(HELD_STDOUT_TEST, "sleep 300", true);
    }

    const RELEASED_STDOUT_TEST: &str = "monitor::tests::a_completed_stream_run_returns_once_its_leftover_process_released_the_agents_stdout";

    /// B (control, the verifier's `/dev/null` control): the same run with a
    /// holder on `/dev/null` returns normally on the unfixed and the fixed
    /// code, and records no bounded wait.
    #[test]
    fn a_completed_stream_run_returns_once_its_leftover_process_released_the_agents_stdout() {
        if crate::test_support::in_child_test(RELEASED_STDOUT_TEST) {
            supervise_stage_in_child();
            return;
        }
        completed_run_with_a_leftover(
            RELEASED_STDOUT_TEST,
            "sleep 300 </dev/null >/dev/null 2>&1",
            false,
        );
    }

    /// Run [`supervise_stream_child`] for a Claude `sh -c <script>` (120 s
    /// idle timeout) on its own thread, so a supervisor that never returns
    /// fails the test instead of hanging it.
    fn supervise_stream_on_thread(
        root: &Path,
        script: String,
    ) -> mpsc::Receiver<Result<i32, String>> {
        let (sender, receiver) = mpsc::channel();
        let root = root.to_path_buf();
        std::thread::spawn(move || {
            let result = supervise_stream_child(
                &root,
                STAGE_PHASE,
                &root,
                STAGE_PROMPT,
                Duration::from_secs(120),
                "sh",
                &["-c".to_string(), script],
                &[],
                AgentKind::Claude,
            );
            let _ = sender.send(result.map_err(|err| err.to_string()));
        });
        receiver
    }

    /// The line C's holder writes 20 s after the agent exited.
    const LATE_HOLDER_LINE: &str = "LATE-HOLDER-LINE";

    /// C (fail-first): a line a leftover writes after the bound never
    /// reaches the capture, although the detached reader is still alive in
    /// this process when it arrives: only the taken capture handle keeps it
    /// out.
    #[test]
    fn no_line_reaches_the_capture_after_the_stdout_wait_expires() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        let record = root.join("holder");
        let script = format!(
            "PATH=/usr/bin:/bin\n\
             IFS= read -r _turn || exit 91\n\
             printf '%s\\n' '{RESULT_WITH_MARKER}'\n\
             ( sleep 20; echo {LATE_HOLDER_LINE} ) &\n\
             {record_lines}exit 0\n",
            record_lines = record_holder(&record)
        );
        let started = std::time::Instant::now();
        let result = supervise_stream_on_thread(root, script);
        let held = HeldStdout::take(&record);
        let returned = result.recv_timeout(Duration::from_secs(30));
        let elapsed = started.elapsed();
        if !(matches!(returned, Ok(Ok(0))) && elapsed <= Duration::from_secs(10)) {
            panic!(
                "WR-03: the stream supervisor did not return 0 within 10 s while a leftover \
                 held its agent's stdout: {returned:?} after {elapsed:?}"
            );
        }
        assert!(
            held.is_alive(),
            "the holder still runs when the supervisor returns (the control for the cause)"
        );
        if !stage_wait_until(Duration::from_secs(30), || !held.is_alive()) {
            panic!(
                "FIXTURE: the holder {} did not exit after its 20 s sleep",
                held.pid
            );
        }
        std::thread::sleep(Duration::from_millis(300));
        let capture =
            std::fs::read_to_string(crate::agent_result::stdout_path(root, STAGE_PHASE)).unwrap();
        assert!(
            capture.lines().any(|line| line == RESULT_WITH_MARKER),
            "the capture holds the result line: {capture:?}"
        );
        assert!(
            !capture.contains(LATE_HOLDER_LINE),
            "WR-03: a line the leftover wrote after the bound reached the capture: {capture:?}"
        );
        assert!(
            !crate::agent_result::idle_timeout_path(root, STAGE_PHASE).exists(),
            "no idle-timeout verdict for a completed run"
        );
        let events = stream_wait_events(root, STAGE_PHASE);
        assert!(
            events.len() == 1,
            "WR-03: expected exactly one stream_wait_bounded event, found {events:?}"
        );
    }

    /// The line F's leftover writes every 0.2 s.
    const NOISE_LINE: &str = "NOISE-LINE";

    /// F (fail-first): a leftover that keeps writing to the agent's stdout
    /// after the agent exited no longer keeps the supervisor from returning
    /// (before the fix every line restarted the idle wait), and it is not
    /// killed by SIGPIPE afterwards: the read end stays open and drained,
    /// and nothing more reaches the capture.
    #[test]
    fn a_stream_run_returns_while_a_leftover_process_keeps_writing_to_the_agents_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        let record = root.join("holder");
        let script = format!(
            "PATH=/usr/bin:/bin\n\
             IFS= read -r _turn || exit 91\n\
             printf '%s\\n' '{RESULT_WITH_MARKER}'\n\
             ( i=0; while [ $i -lt 1500 ]; do echo {NOISE_LINE}; sleep 0.2; i=$((i+1)); done ) &\n\
             {record_lines}exit 0\n",
            record_lines = record_holder(&record)
        );
        let started = std::time::Instant::now();
        let result = supervise_stream_on_thread(root, script);
        let held = HeldStdout::take(&record);
        let returned =
            result.recv_timeout(Duration::from_secs(20).saturating_sub(started.elapsed()));
        let elapsed = started.elapsed();
        if !(matches!(returned, Ok(Ok(0))) && elapsed <= Duration::from_secs(10)) {
            // SIGKILL the writer by its recorded pid, which unblocks the
            // reader, before failing.
            drop(held);
            panic!(
                "WR-03: the stream supervisor did not return 0 within 10 s while a leftover \
                 kept writing to its agent's stdout: {returned:?} after {elapsed:?}"
            );
        }
        assert!(
            held.is_alive(),
            "the writer still runs when the supervisor returns"
        );
        let capture_path = crate::agent_result::stdout_path(root, STAGE_PHASE);
        let size = || {
            std::fs::metadata(&capture_path)
                .map(|meta| meta.len())
                .expect("stat the capture")
        };
        let before = size();
        std::thread::sleep(Duration::from_millis(1500));
        assert!(
            held.is_alive(),
            "WR-03: the writer died within 1.5 s of the bound (SIGPIPE: the read end closed)"
        );
        assert!(
            size() == before,
            "WR-03: the capture grew after the bound ({before} bytes before)"
        );
        let capture = std::fs::read_to_string(&capture_path).unwrap();
        assert!(
            capture.lines().any(|line| line == RESULT_WITH_MARKER),
            "the capture holds the result line: {capture:?}"
        );
        assert_one_reader_bound(root, STAGE_PHASE, "F");
    }

    const STOPPED_HELD_STDOUT_TEST: &str = "monitor::tests::a_stopped_stream_run_returns_while_a_process_outside_the_agent_group_holds_its_stdout";

    /// D: a stop taken while the agent runs, with the agent's stdout also
    /// held by a process outside the agent's group (util-linux `setsid` as
    /// a background job: a non-interactive `sh` job is not a group leader,
    /// so `setsid` does not fork and `$!` is the sleeper), finishes within
    /// the stop's own wait under the production 120 s idle timeout. The
    /// stop's group signals cannot reach the holder, so before the fix the
    /// supervisor waited 120 s in `drive_stream_io` and then forever in the
    /// thread join.
    #[test]
    fn a_stopped_stream_run_returns_while_a_process_outside_the_agent_group_holds_its_stdout() {
        if crate::test_support::in_child_test(STOPPED_HELD_STDOUT_TEST) {
            supervise_stage_in_child();
            return;
        }
        let fixture = StageFixture::new();
        let record = fixture.path("holder");
        let program = fixture.agent(
            "claude",
            &format!(
                "{preamble}setsid sleep 300 &\n\
                 {record_lines}\
                 while :; do sleep 0.05; done",
                preamble = Shape::Stream.preamble(),
                record_lines = record_holder(&record),
            ),
        );
        let mut run = launch_stage_with(
            STOPPED_HELD_STDOUT_TEST,
            &fixture,
            StageLaunchArgs {
                shape: Shape::Stream,
                agent: AgentKind::Claude,
                program: &program,
                adapter_env: "",
                args: &[],
                extra_env: &[],
            },
        );
        let held = HeldStdout::take(&record);
        let agent_pgid = libc::pid_t::try_from(run.agent_pid()).expect("pgid fits pid_t");
        let holder = libc::pid_t::try_from(held.pid).expect("pid fits pid_t");
        let outside = stage_wait_until(Duration::from_secs(2), || {
            // SAFETY: getpgid has no memory preconditions.
            let pgid = unsafe { libc::getpgid(holder) };
            pgid > 0 && pgid != agent_pgid
        });
        if !outside {
            panic!(
                "FIXTURE: the holder {holder} did not leave the agent's group {agent_pgid} \
                 within 2 s"
            );
        }
        run.child.signal(libc::SIGTERM);
        if !stage_wait_until(Duration::from_secs(20), || run.child.try_exited()) {
            panic!(
                "WR-03: a stop did not finish within 20 s while a process outside the agent's \
                 group held its stdout"
            );
        }
        assert_eq!(run.finish(STOPPED_HELD_STDOUT_TEST), "Stopped finished");
        assert_eq!(fixture.exit_code(), "143");
        assert!(
            !crate::agent_result::idle_timeout_path(&fixture.root, STAGE_PHASE).exists(),
            "no idle-timeout verdict for a stopped run"
        );
        assert!(
            held.is_alive(),
            "the holder outside the agent's group still runs: the stop's group signals cannot \
             reach it, so the bounded wait is what let the stop finish"
        );
        assert_one_reader_bound(&fixture.root, STAGE_PHASE, STOPPED_HELD_STDOUT_TEST);
        drop(run.group);
    }
}
