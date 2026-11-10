# Architecture

DevFlow is an opinionated CLI that drives a fixed 5-stage pipeline
(Define → Plan → Code → Validate → Ship) end-to-end: it launches a coding
agent per stage, evaluates whether the agent succeeded, fires human gates at
Validate/Ship, and runs side-effecting hooks (branch/docs/changelog/version)
at the mapped transitions. This document describes the implementation as it
actually exists; treat the named source files as the source of truth if this
drifts.

## Crates

DevFlow is a two-crate Cargo workspace (`Cargo.toml`):

| Crate | Kind | Responsibility |
|---|---|---|
| `devflow-core` | Library (`crates/devflow-core`) | Stage machine, per-phase state, hooks, locking, events log, gate protocol, versioning, agent adapters, monitor daemon, worktrees, recovery. Returns structured data; no terminal formatting. |
| `devflow-cli` | Binary (`crates/devflow-cli`) | Clap command parser and the user-facing `devflow` binary. Owns all printing and orchestration (`advance`, `transition`, `run_gate`); delegates primitives to `devflow-core`. |

The split keeps workflow logic testable and independent of both the CLI surface
and any individual coding agent.

## The Stage machine

The pipeline is a single linear chain of five stages
(`crates/devflow-core/src/stage.rs`, `enum Stage`):

```
Define → Plan → Code → Validate → Ship
```

- `Stage::next()` defines the linear transitions; `Ship` is terminal
  (`next()` returns `None`).
- `Stage::is_agent_stage()` is true for `Define`, `Plan`, `Code` — these
  launch a headless coding agent driven by a GSD slash command
  (`Stage::gsd_command()`): `/gsd-discuss-phase {N}`, `/gsd-plan-phase {N}`,
  `/gsd-execute-phase {N}`.
- `Stage::is_gate()` is true for `Validate` and `Ship` — these may pause the
  workflow at a human gate (see [Gate protocol](#gate-protocol)) depending on
  the active `Mode`. Validate also launches an agent
  (`/gsd-validate-phase {N}`) before its gate decision; Ship's agent step
  (`/gsd-ship {N}`) is described in [Git and ship model](#git-and-ship-model).
- `State` (`crates/devflow-core/src/state.rs`) holds `stage`, `phase`,
  `agent`, `mode`, `gate_pending`, `consecutive_failures`, `project_root`,
  and an optional `worktree_path`. It is persisted per-phase — see
  [Per-phase state and locking](#per-phase-state-and-locking).
- `crates/devflow-cli/src/main.rs`'s `advance()` function drives the
  machine: after an agent exits (or a gate resolves), it evaluates the
  result (see [Completion evaluation](#completion-evaluation)) and calls
  `transition()` to move to the next stage, `handle_validate_outcome()` /
  `handle_ship_outcome()` for the two gate stages, or
  `handle_stage_failure()` for a never-silent failure gate (WR-11 — any
  non-Validate stage failure fires a gate rather than halting silently).

## Hooks

Creating/cleaning up branches, updating docs, appending the changelog, and
bumping the version are **not** stages — they are side-effecting *hooks*
that fire at specific stage transitions
(`crates/devflow-core/src/hooks.rs`, `enum Hook`):

| Hook | Effect |
|---|---|
| `BranchCreate` | Create `feature/phase-NN` from `develop` (`GitFlow::feature_start`) |
| `BranchCleanup` | Delete the merged feature branch after Ship (non-force — never discards unmerged work) |
| `DocsUpdate` | Run `cargo doc --no-deps` and commit any doc changes |
| `ChangelogAppend` | Prepend a `CHANGELOG.md` entry for the computed version |
| `VersionBump` | Compute the next version, write it to the version file, and tag `v{version}` |

`hooks_for_transition(from, to)` maps a stage move to the hooks that should
run; today the only non-empty mapping is `Validate → Ship`, which runs
`DocsUpdate` + `ChangelogAppend` (docs/changelog are finalized before the
Ship agent runs). `hooks_after_ship()` — `VersionBump` + `BranchCleanup` —
fires once a human approves the Ship gate (`finish_workflow()`), not as a
`hooks_for_transition` entry. Both hook batches run under the project-wide
checkout lock (see [locking](#per-phase-state-and-locking)) and are
individually fail-soft: a hook failure warns and the batch continues rather
than aborting the workflow.

`BranchCreate` is defined as a `Hook` variant but `hooks_for_transition`
never returns it — branch/worktree creation for a phase actually happens
directly in `start()` (`crates/devflow-cli/src/main.rs`) via
`GitFlow::feature_start`/`feature_start_force` (branch-in-place mode) or
`worktree::add` (the default, isolated-worktree mode), not through the hook
dispatch table.

## Agent model

Agent backends are isolated behind a trait
(`crates/devflow-core/src/agents/mod.rs`):

- `AgentDriver` trait: `name()`, `render_prompt(intent)`, `build_command(phase, prompt,
  extra_writable_roots) -> (program, args)`, `parse_completion(output)`, `health(state)`,
  `environment()`, `capabilities()`, `interactivity_mode(stage)`, `test_contract()`. Each
  driver owns its prompt rendering, command building, completion parsing, and health.
- Supported `AgentKind`s (`state.rs`): `Claude`, `Codex`, `OpenCode`, `Pi`.
  Accepted names: `claude`, `codex`, `opencode` / `open-code`, `pi`.
- `driver_for(kind)` returns the boxed driver for a kind
  (`ClaudeDriver`/`CodexDriver`/`OpenCodeDriver`/`PiDriver`).
- **Pi subagent dispatch is user-installed and capability-detected.** A user
  installs `@bacnh85/pi-subagent` at **user scope** (`pi install
  npm:@bacnh85/pi-subagent`); DevFlow never ships or manages it. `PiDriver::
  capabilities()` probes `pi list --no-approve` for the vetted
  `@bacnh85/pi-subagent` package (not a bare `*subagent*` name-match) and sets
  `DriverCapabilities::subagent_dispatch`. The capability is **reported only**
  (surfaced by `devflow doctor`) — no stage-routing or launch logic consumes it
  yet. The signal is name-based (Pi has no `pi tools` command), not a
  tool-registry proof.
- Prompts are built per-stage by `crate::prompt::stage_prompt(stage, phase)`
  (or `stage_prompt_for_project` when the CLI applies project config),
  not a single shared instruction template. Every prompt hands the agent its
  GSD slash command (`Stage::gsd_command()`) plus a `DEVFLOW_RESULT`
  completion contract (`DEVFLOW_RESULT: {"status": "success"}` /
  `{"status": "failed", "reason": "..."}`). Prompts using
  `COMPLETION_PROTOCOL` require that result as the last line of the agent's
  final message, with any decision reasoning above it. Three stages get
  dedicated prompts:
  - **Define / Plan** — idempotent: if the stage's deliverable
    (`CONTEXT.md` / `PLAN.md`) already exists, the agent reports success
    without re-running the GSD command (headless Codex cannot answer GSD's
    interactive overwrite/append/cancel prompt).
  - **Validate** — REQUIRES a `verdict: "pass"` or `verdict: "gaps"` field
    distinct from `status`, so `advance()` can tell "the validation task
    ran" apart from "validation passed."
  - **Ship** — runs `/gsd-code-review {N}` first; if `REVIEW.md` contains
    any Critical-severity finding, the agent must NOT run `/gsd-ship {N}`
    and instead reports a `review:`-prefixed failure (looped back to Code
    with the audit-fix prompt, not gated as a crash). Only a clean review
    proceeds to `/gsd-ship {N}`.

## Completion evaluation

When an agent exits, `evaluate_agent_result()`
(`crates/devflow-core/src/agent_result.rs`) decides success/failure with
four layers, tried in order:

0. **Layer 0 — external post-condition (authoritative failure).** Commands
   declared as `external_verify` in PLAN frontmatter run only after exact
   command-array approval through `DEVFLOW_TRUST_EXTERNAL_VERIFY`; they run
   before agent-controlled signals. A failure returns `Failed`; success or
   no declaration defers to the ordinary cascade.
1. **Layer 1 — `DEVFLOW_RESULT` marker (authoritative for ordinary plans).** Parses the
   agent's native per-adapter envelope (Claude's JSON result object, or
   Codex's `--json` JSONL event stream) or scans captured stdout for the
   last `DEVFLOW_RESULT: {...}` line. An envelope `is_error: true` overrides
   a stale marker; a marker-less Codex `turn.completed` defers to Layer 2
   rather than assuming success.
2. **Layer 2 — exit code + commit gate (reliable fallback).** Reads the exit
   code from `.devflow/phase-NN-exit` and counts commits on
   `feature/phase-NN` (`develop..feature/phase-NN`). `exit != 0` is always
   `Failed`, for every stage. On `exit == 0`, the "zero commits → failed"
   gate applies **only** to `Stage::Plan`/`Stage::Code` (checked explicitly,
   not via `is_agent_stage()`, since that also includes `Define`, which
   legitimately produces zero commits) — Define/Validate/Ship with
   `exit == 0` succeed regardless of commit count; Validate's real pass
   signal is its `verdict`, not a commit count.
3. **Layer 3 — process gone + commits exist (last resort).** If neither the
   marker nor the exit-code file is available, commit presence is used as a
   heuristic (`AgentStatus::Unknown`) with a warning.

## Per-phase state and locking

State is per-phase (`crates/devflow-core/src/workflow.rs`): each active
phase persists to `.devflow/state-{phase:02}.json`, so `devflow parallel`
sibling phases never clobber one another. A single-slot
`.devflow/state.json` left by a pre-14a binary is not read, renamed or
removed.

Coordination is split by what each step needs, and no lock is held across a
gate wait (D-08):

- **State write lock** (`.devflow/.state-{phase:02}.lock`, `workflow.rs`) — a
  `flock` taken for one read-modify-write of the state file, so it is short:
  usually milliseconds. A stalled gate lock or a socket dying mid-ping can stretch
  it to about 10 s in a gate answer, 13 s while `recover` decides a phase and 16 s
  in a reservation that loads a state. The 16 s is counted from the code's timeouts,
  not measured: two socket probes at 8 s, or a 6 s probe that finds a dead group, the
  2 s wait for a supervisor exiting, then an 8 s probe. A missing or unloadable state
  instead probes each socket left for the phase, up to 8 s each. The file is never replaced or removed, because a `flock`
  belongs to an inode. Repair verbs (`devflow stop`, `devflow recover --clean`)
  wait at most `WRITE_LOCK_REPAIR_WAIT` (10 s), then name the holder and write
  nothing, so a hold that long costs them a refusal, not a hang.
- **Launch reservation** (`state.pending_supervisor` and
  `.devflow/supervisor-<launch_id>.live`, `liveness.rs`) — a launcher takes the
  `flock` before it saves the pending record, and the spawned supervisor inherits
  and holds it for life, so a pending supervisor is probeable before its socket
  exists. A reservation whose launcher died holds nothing and reads abandoned.
- **Gate registration** (`.devflow/gates/{phase:02}-{stage}.{gate_id}.waiter`,
  `gates.rs`) — a `flock` a waiter holds for the length of its wait, which is what
  "something waits on this gate" means. `.devflow/.gates-{phase:02}.lock` guards a
  few file operations at a time (at most `GATE_LOCK_WAIT`, 5 s) and is never
  removed.
- **Foreground holder** (`.devflow/.holder-{phase:02}`, `liveness.rs`) — held for
  its lifetime by a `start`/`resume` before its spawn, `devflow ship` or a hand-run
  `devflow advance`, recording its pid and start time. It is the only pid
  `devflow stop` signals, and only when the holder's start time still matches.
- **Project-wide checkout lock** (`.devflow/lock-project`,
  `lock::acquire_project_blocking`, `crates/devflow-core/src/lock.rs`) — a coarse,
  seconds-scale, file-based lock (`O_EXCL` create, PID written into the file,
  stale-holder reclaim if the recorded PID is dead) that serializes mutations of
  the shared primary checkout (hook batches) across concurrently finishing phases.
  It must never be held across a gate wait; on timeout
  (`DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`, default 120s) the hook batch is
  **skipped** (loudly, via `events.jsonl`) rather than ever run unserialized.

A `.devflow/lock-{phase:02}` left by an older binary means nothing now and blocks
nothing; `devflow recover --clean` removes it once the pid it names has exited.

State and gate files are written through the durable helpers in
`crates/devflow-core/src/durable.rs` (D-09), and a source scan
(`durable::tests::state_and_gate_modules_mutate_files_only_through_the_durable_helpers`)
holds `workflow.rs` and `gates.rs` to them. The scan is a text scan, and power
loss itself is not tested: the fsyncs are covered in code, not proven against a
real power cut.

That scope covers state and gate files only. `supervisor-*.live`, `*.waiter`,
`.holder-NN`, `.state-NN.lock`, `.gates-NN.lock`, `*.hold-tmp` and
`.devflow/.gitignore` are each an advisory marker deliberately outside D-09: no
state lives in them, a held lock rather than the file's presence decides liveness,
and no holder survives a power loss, so a marker a crash resurrects or loses reads
abandoned. This is chosen, not overlooked.

## Events log

`.devflow/events.jsonl` (`crates/devflow-core/src/events.rs`) is an
append-only, one-JSON-object-per-line log, schema v1:

```json
{"v":1,"ts":1752600000,"phase":14,"event":"transition","from":"code","to":"validate"}
```

Every line carries `v`, `ts` (unix seconds), `phase`, and `event`; the rest
of the fields are event-specific. Emission is fail-soft (an unwritable log
warns and returns — never aborts the workflow it records), and appends are a
single `write_all` on an `O_APPEND` handle so concurrent phase monitors'
lines interleave without tearing. The CLI emits events at every meaningful
step — `workflow_started`, `advance_evaluated`, `transition`, `gate_fired`,
`notify_fired`, `loop_back`, `hook_run`, `checkout_lock_timeout`,
`workflow_finished`, `advance_failed` — so any frontend (Hermes plugin, a
future TUI/web UI) can observe a running loop by tailing one file instead of
integrating with DevFlow internals. `devflow logs`/`status` read it via
`last_events_by_phase()` (one read+parse pass over the whole log).

## Gate protocol

A gate is a pause point where DevFlow writes a request to
`.devflow/gates/` and waits for a human (or the Hermes cron poller) to drop an
answer (`crates/devflow-core/src/gates.rs`, struct `Gates`). Phase and stage are
a gate's location; its `gate_id`, fresh for every gate opened, is its identity.
Files per gate (the waiter's registration is an advisory marker, the rest are
written durably):

- `.devflow/gates/{phase:02}-{stage}.json` — the request (`GateFile`, which must
  name the `gate_id`), written by `Gates::open_gate()` and its path-returning form
  `Gates::write_gate()`.
- `.devflow/gates/{phase:02}-{stage}.{gate_id}.response.json` — the answer
  (`GateAnswer { gate_id, approved, note, responded_by }`), written by
  `Gates::answer()` (the `devflow gate approve|reject` CLI path) or by hand/Hermes.
- `.devflow/gates/{phase:02}-{stage}.{gate_id}.ack.json` — the consumption claim
  (`GateAck`, `Gates::claim()`): the first process to publish it consumes the
  answer.
- `.devflow/gates/{phase:02}-{stage}.{gate_id}.waiter` — the waiter's registration
  (`Gates::register_waiter()`).
- `.devflow/gates/{phase:02}-{stage}.{gate_id|noid}.{secs}.{pid}.{seq}.quarantined.json`
  — an answer set aside rather than consumed or deleted (`Gates::quarantined()`):
  a missing or other id, an unparsable body, a legacy unkeyed
  `{phase:02}-{stage}.response.json`, or one left unused when its gate closed. It
  stays until removed by hand, and `devflow status`, `devflow doctor` and
  `devflow recover --clean` list it.

`devflow gate approve|reject` answer the gate open now, or with `--gate-id <ID>`
the one `gate list` or `gate show` displayed, and first move aside an answer in
that gate's own slot that does not bind to it, so a refused answer never blocks a
valid one. A hand-written answer copies `gate_id` from the gate file into the keyed
path and body.

`run_gate()` (`crates/devflow-cli/src/pipeline_gate.rs`) writes the gate,
fires the notify hook (`fire_gate_notify()` — runs `$DEVFLOW_GATE_NOTIFY_CMD`
via `sh -c` with gate metadata as environment variables, never interpolated
into the command string; a silent no-op when the variable is unset), then
waits in `wait_for_gate_response()` (`crates/devflow-cli/src/pipeline_launch.rs`),
which holds no write lock and waits through `Gates::await_answer()`. That reads
the gate's answer slot with a backoff (1s → 2s → … capped at 60s) and checks for a
stop at least once a second, one of two ways:

- **Inside a supervisor's advance** (its stop token is engaged, 50-13): the
  run's owner reads `gate_wait` in a `ping` reply for the length of the wait,
  and the stop token is what is checked. A stop — a `shutdown` request on the
  supervisor's socket, or SIGTERM/SIGINT to the supervisor — ends the wait within
  about a second: the gate is closed, `stopped` is recorded in the state, which is
  kept for `devflow resume` (not an abort), and the advance unwinds without acting
  on the gate. A stop accepted in the same second as an answer wins.
- **Anywhere else** (no stop token: the hidden `devflow advance` run from a
  shell, or a gate a foreground command fires): the foreground holder's stop flag
  is checked instead, so `devflow stop`'s signal ends the wait the same way (safe
  point 4).

A file in the slot that does not bind to the gate is set aside and the wait goes
on with the time left. Either wait ends when a bound answer appears or
`DEVFLOW_GATE_TIMEOUT_SECS`
(default 259200 = 3 days, held equal to
`DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` so `devflow gate sweep` cannot reap a
gate this wait is still watching) elapses, and past that the run has ended at the
gate. `devflow resume` then reruns the stage and asks again, reporting any unused
answer it set aside; a stored Ship approval is applied without a rerun by
`devflow ship`. `GateAction::from_response()` turns an
approval into `Advance`, a rejection into `LoopBack(Code)`, or — when the
rejection note contains "abort" — `Abort(reason)`. `devflow gate list`
(`Gates::list_open()`) enumerates requests with no answer bound to them yet,
skipping unparsable entries rather than failing.

## Monitor daemon

`crates/devflow-core/src/monitor.rs` is the core automation primitive — no
cron, no scheduler, no agent cooperation. `spawn_monitor()` re-execs the CLI
as its hidden `__monitor` subcommand, **a detached supervisor process in its
own process group that owns the agent**, for both launch shapes:

- **capture** (`MonitorLaunch::Legacy`): the agent's stdin is `/dev/null` and
  its stdout and stderr go straight to the phase capture files (stderr is kept
  out of the stdout capture so it can never corrupt the `DEVFLOW_RESULT`
  parse). Non-Claude adapters other than Antigravity, the legacy Claude launch
  opt-out and a checkpoint resume use it.
- **stream** (`MonitorLaunch::PipeOwning`): the supervisor owns the agent's
  stdin and stdout pipes, delivers the prompt as the first turn and applies the
  idle timeout. Claude and Antigravity `stream-json` launches use it.

The supervisor (`run_supervised_stage()`):

1. Spawns the agent (`program` + `args`) in the agent's own process group and
   records its PID.
2. Publishes a per-launch Unix socket under `registry::cache_dir()/sock/`,
   with a JSON sidecar beside it naming the launch (project root, phase,
   agent process group, start time, boot id). The socket path is recorded in
   the phase's `state.json` as `state.supervisor`; liveness (ALIVE / STALE /
   GONE) is answered by probing that path, never from a pid.
3. Answers `ping` and `shutdown` on the socket, and treats SIGTERM and SIGINT
   sent to it as a `shutdown`: the agent's group is ended and no advance runs.
4. Waits for the agent to exit and records its exit code to
   `.devflow/phase-NN-exit`. A stream supervisor's reader keeps draining the
   agent's stdout until the pipe closes, and a process the agent started can
   hold that stdout open after the agent exits. The supervisor then waits up
   to `READER_DRAIN_WAIT` (3 s) for its pipe threads, then takes the capture
   file from the reader, within one more `READER_DRAIN_WAIT` when a capture
   write is blocked, so at most 6 s in all (plus a poll interval per wait). A
   write still blocked after that keeps the capture file. It detaches the
   unfinished threads, records a `stream_wait_bounded` event, and signals
   nothing. The detached reader keeps draining the pipe, discarding what it
   reads, until the supervisor exits.
5. On the agent's natural exit, runs the advance in-process. The phase is
   threaded in at spawn time, so the advance's identity never depends on a
   shared state singleton — under `devflow parallel`, each phase's supervisor
   advances exactly its own stage. When the advance launches the next stage,
   that stage's supervisor publishes its socket before this one unlinks its
   own, so the hand-off leaves no gap with neither socket published.

`spawn_monitor()` returns once the new supervisor answers a ping with this
launch's id. Because the supervisor outlives the CLI process that launched
it, agent stdout keeps flowing into the capture file and the exit code is
still reaped after that process returns. `spawn_monitor()` is the single way
an agent process is spawned (23d).

## Worktree model

`crates/devflow-core/src/worktree.rs` wraps plain `git worktree` commands.
Each agent gets an isolated working directory under
`<project_root>/.worktrees/`, sharing the main repo's object database; state
and capture files always live under the main `project_root` — only the
agent's working directory moves. **Worktree isolation is the default**:
`devflow start` creates a worktree unless `--no-worktree` is passed (the
`--worktree` flag itself is a deprecated no-op kept for one release). This
backs:

- `start` (default) — run a single phase in its own worktree;
  `--no-worktree` runs directly in the primary checkout instead.
- `parallel` — run multiple phases concurrently, each in its own worktree.

Branch integration uses `GitFlow::ensure_branch()` (create at a start point
without checking out).

## Git and ship model

`crates/devflow-core/src/git.rs` (`GitFlow`) implements branch primitives:
`feature_start(phase)`/`feature_start_force(phase)` (create
`feature/phase-NN` from `develop`), `feature_finish(phase)` (merge into
`develop` with `--no-ff` and delete), `push`, `has_remote`,
`cleanup_merged`. `release_start`/`release_finish` also exist on `GitFlow`
but are not called from any production CLI path today (only exercised in
`git.rs`'s own tests) — cutting a `release/{version}` branch and opening a
PR is not part of the current Ship flow.

The real Ship model is **gate-driven, not command-driven**: there is no
standalone ship subcommand and no separate commands to finalize or undo a
recorded ship — those do not exist in `enum Command`. Ship works like any
other stage plus a mandatory terminal gate:

1. `Validate → Ship` transition fires the `DocsUpdate` + `ChangelogAppend`
   hooks, then launches the Ship agent with its dedicated prompt (code
   review gate, then `/gsd-ship {N}` — see [Agent
   model](#agent-model)). PR creation, if any, happens inside that GSD
   command, not in DevFlow's own git primitives.
2. When the Ship agent exits successfully, `handle_ship_outcome()` **always**
   fires a Ship gate (`run_gate(..., Stage::Ship, "Ship complete — approve
   merge?")`) — Ship is never auto-advanced past, regardless of `Mode`.
3. A human answers via `devflow gate approve <phase> --stage ship [--note
   ...]` or `devflow gate reject <phase> --note "..." [--stage ship]`
   (`GateCmd::Approve`/`GateCmd::Reject`, `crates/devflow-cli/src/main.rs`).
   Approval runs `finish_workflow()`: the `Merge`, `VersionBump`,
   `ChangelogAppend` and `BranchCleanup` hooks, gate cleanup, and
   `workflow::clear_state()`. A rejection loops
   back to Code (or aborts, if the note contains "abort").
4. A Ship-stage **agent crash** (vs. a review rejection) is routed through
   the same never-silent gate path as any other stage failure
   (`handle_stage_failure`) — WR-11: no non-Validate stage failure is ever
   silent.

## Configuration

DevFlow has no initialization step. Workflow options remain CLI flags to
`devflow start` (`crates/devflow-cli/src/main.rs`, `Command::Start`). An
optional minimal `devflow.toml` contains only reliability knobs:
`capture_retention`, `review_angles`, and `external_verify_enabled`.

| Flag | Purpose |
|---|---|
| `--phase N` | Phase number to work on |
| `--agent claude\|codex\|opencode` | Agent to launch |
| `--mode auto\|supervise` | `auto` gates only at Ship (plus never-silent failure gates); `supervise` also gates every Validate |
| `--force` | Overwrite the feature branch if it already exists |
| `--no-worktree` | Run in the primary checkout instead of an isolated worktree |
| `--dry-run` | Print the pipeline that would run without launching anything |

Runtime state lives under `.devflow/` (git-ignored), keyed per-phase —
`state-{phase:02}.json` (see [Per-phase state and
locking](#per-phase-state-and-locking)). Tunable *runtime behavior* (not
workflow options) is read from environment variables —
`DEVFLOW_GATE_NOTIFY_CMD`, `DEVFLOW_GATE_TIMEOUT_SECS`,
`DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`, `DEVFLOW_CAPTURE_RETENTION`,
`DEVFLOW_REVIEW_ANGLES`, `DEVFLOW_EXTERNAL_VERIFY_ENABLED`, `RUST_LOG`,
`DEVFLOW_LOG_FORMAT` — see
[OPERATIONS.md](OPERATIONS.md) for the full operator-facing reference.

## Logging

DevFlow uses the [`tracing`](https://docs.rs/tracing) ecosystem for structured
diagnostic logging. All log output goes to **stderr**; **stdout is reserved**
for agent/system output and structured results.

### Configuration

| Environment variable | Purpose | Default |
|---|---|---|
| `RUST_LOG` | Controls log verbosity via `tracing-subscriber::EnvFilter`. Accepts bare levels (`error`, `warn`, `info`, `debug`, `trace`) or targeted directives (`devflow_core=debug,devflow=info`). | `info` |
| `DEVFLOW_LOG_FORMAT` | When set to `json`, enables JSON-structured log output (one JSON object per line on stderr). Any other value (or unset) produces human-readable plain-text logs. | plain text |

### Instrumentation

Key modules log via `tracing`'s `info!`/`warn!`/`debug!` macros:

- **`main.rs`** — `advance`/`transition`/`run_gate` print human-readable
  progress to stdout and emit structured events to `events.jsonl` at every
  step (see [Events log](#events-log)).
- **`workflow.rs`** — state I/O (`update_state`, `load_state`) logs at `DEBUG`;
  unreadable/corrupt state files warn at `WARN`
  rather than aborting a listing.
- **`git.rs`** — branch operations (`feature_start`, `feature_finish`,
  `tag`, `push`) log at `INFO`; checkout/rebase/commit operations log at
  `DEBUG`; force operations log at `WARN`.
- **`hooks.rs`** — each hook run logs its outcome at `INFO` (success) or
  `WARN` (fail-soft failure, e.g. an unmerged branch left in place by
  `BranchCleanup`).
- **`gates.rs`** — gate writes/polls log at `INFO`/`DEBUG`; a failed or
  unspawnable notify command warns at `WARN` (fail-soft — never aborts
  `run_gate`).
- **`lock.rs`** — reclaiming a stale (dead-holder) lock warns at `WARN`.
- **`monitor.rs`** — agent spawn logs at `INFO`; PID/exit polling logs at
  `DEBUG`.

### Log levels

| Level | Usage |
|---|---|
| `ERROR` | Unrecoverable conditions that abort an operation. |
| `WARN` | Recoverable anomalies (force operations, stale locks, fail-soft hook/notify failures). |
| `INFO` | Stage transitions, agent lifecycle, branch/gate milestones. |
| `DEBUG` | I/O details, merge/checkout operations, poll iterations. |
| `TRACE` | Fine-grained execution tracing (rarely needed outside debugging). |

### JSON output

When `DEVFLOW_LOG_FORMAT=json`, each log event is a single JSON line
containing `timestamp`, `level`, `target`, `fields`, and optional `span`
information — for machine consumption (e.g. Hermes watching agent output).
This is a separate stream from `events.jsonl` (append-only workflow events);
JSON tracing output is DevFlow's own diagnostic logging.

```bash
# JSON log smoke test
DEVFLOW_LOG_FORMAT=json RUST_LOG=info cargo run -- status 2>devflow.json
head -1 devflow.json | python3 -m json.tool  # Should parse as valid JSON
```

`devflow doctor` includes a `RUST_LOG` environment check that validates the
variable is set to a parseable value and warns when it is missing or invalid.

## Extension points — adding an agent

DevFlow supports four agents today (Claude Code, Codex, OpenCode, Pi) through
the modular `AgentDriver` contract; agent-specific code lives only in
`agents/*.rs` and the targeted result parsing. Adding a backend today is a
checklist — keep these in sync or tests/builds fail:

1. Add a driver file under `crates/devflow-core/src/agents/` implementing the
   `AgentDriver` trait.
2. Add a variant to the `AgentKind` enum in `state.rs`.
3. Update the `FromStr` parser, `Display`, and `AgentParseError` text in
   `state.rs` to accept/emit the new name.
4. Add a match arm in `agents::driver_for()`.
5. Add the `pub mod` and `pub use` exports in `agents/mod.rs`.
6. Add/extend tests (driver name, parser aliases, and a conformance-suite run).
7. Update docs (`README.md`, `DEPENDENCIES.md`, `CONTRIBUTING.md`, this file).
