# Architecture

DevFlow is an agent-agnostic CLI that drives a fixed 5-stage pipeline
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

- `AgentAdapter` trait: `name()`, `exec_command(phase, prompt,
  extra_writable_roots) -> (program, args)`, `extra_env()` (defaults to
  none — Codex uses it to disable commit/tag signing inside its sandbox),
  `completion_signal_detected(output)`. Each adapter only knows how to wrap
  a prompt into its CLI's non-interactive launch flags.
- Supported `AgentKind`s (`state.rs`): `Claude`, `Codex`, `OpenCode`.
  Accepted names: `claude`, `codex`, `opencode` / `open-code`.
- `adapter_for(kind)` returns the boxed adapter for a kind
  (`ClaudeAgent`/`CodexAgent`/`OpenCodeAgent`).
- Prompts are built per-stage by `crate::prompt::stage_prompt(stage, phase)`
  (or `stage_prompt_for_project` when the CLI applies project config),
  not a single shared instruction template. Every prompt hands the agent its
  GSD slash command (`Stage::gsd_command()`) plus the `DEVFLOW_RESULT`
  completion contract (`DEVFLOW_RESULT: {"status": "success"}` /
  `{"status": "failed", "reason": "..."}`, required as the agent's exact
  final message). Three stages get dedicated prompts:
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
   declared as `external_verify` in operator-authored PLAN frontmatter run
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
sibling phases never clobber one another. A legacy single-slot
`.devflow/state.json` from a pre-14a binary is migrated to its per-phase
name on first read.

Two independent lock levels (`crates/devflow-core/src/lock.rs`), both
file-based (`O_EXCL` create, PID written into the file, stale-holder
reclaim if the recorded PID is dead):

- **Per-phase advance lock** (`.devflow/lock-{phase:02}`) — held by
  `advance()` across a gate's potentially multi-day blocking wait. Scoped
  per-phase (not per-project) so one phase blocked on a gate never starves
  `devflow parallel`'s sibling phases.
- **Project-wide checkout lock** (`.devflow/lock-project`,
  `lock::acquire_project_blocking`) — a coarse, seconds-scale lock that
  serializes mutations of the shared primary checkout (hook batches,
  `sequentagent`'s branch integration) across concurrently finishing
  phases. It must never be held across a gate wait; on timeout
  (`DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`, default 120s) the hook batch is
  **skipped** (loudly, via `events.jsonl`) rather than ever run
  unserialized.

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
`.devflow/gates/` and blocks until a human (or the Hermes cron poller) drops
a response file (`crates/devflow-core/src/gates.rs`, struct `Gates`). Three
files per gated stage, all written atomically (temp file + rename):

- `.devflow/gates/{phase:02}-{stage}.json` — the request (`GateFile`),
  written by `Gates::write_gate()`.
- `.devflow/gates/{phase:02}-{stage}.response.json` — the human's answer
  (`GateResponse { approved, note, responded_by }`), written by
  `Gates::respond()` (the `devflow gate approve|reject` CLI path) or by
  hand/Hermes.
- `.devflow/gates/{phase:02}-{stage}.ack.json` — DevFlow's receipt
  (`Gates::ack()`) once the response is consumed, so a poller knows to clean
  up.

`run_gate()` (`crates/devflow-cli/src/main.rs`) writes the gate, fires the
notify hook (`fire_gate_notify()` — runs `$DEVFLOW_GATE_NOTIFY_CMD` via
`sh -c` with gate metadata as environment variables, never interpolated
into the command string; a silent no-op when the variable is unset), then
blocks in `Gates::poll_response()` with exponential backoff (1s → 2s → …
capped at 60s) until a response appears or `DEVFLOW_GATE_TIMEOUT_SECS`
(default 604800 = 7 days) elapses. `GateAction::from_response()` turns an
approval into `Advance`, a rejection into `LoopBack(Code)`, or — when the
rejection note contains "abort" — `Abort(reason)`. `devflow gate list`
(`Gates::list_open()`) enumerates requests with no response file yet,
skipping unparsable entries rather than failing.

## Monitor daemon

`crates/devflow-core/src/monitor.rs` is the core automation primitive — no
cron, no scheduler, no agent cooperation. `spawn_monitor()` spawns a
**detached child process that owns the agent**:

1. Launches the agent (`program` + `args`), redirecting stdout/stderr to
   separate phase capture files (stderr is kept out of the stdout capture so
   it can never corrupt the `DEVFLOW_RESULT` parse) and recording the agent
   PID.
2. Waits for the agent to exit and records its exit code to
   `.devflow/phase-NN-exit`.
3. Runs `devflow advance <project> --phase N` to advance the stage machine.
   The phase is threaded in at spawn time, so `advance`'s identity never
   depends on a shared state singleton — under `devflow parallel`, each
   phase's monitor advances exactly its own stage.

Because the monitor outlives the `devflow start`/`devflow advance`
invocation that spawned it, agent stdout keeps flowing into the capture
file and the exit code is still reaped after the CLI process returns. A
second entry point, `spawn_monitor_no_advance()`, spawns the same
capture-owning child but skips step 3 — used by `sequentagent`, which
drives its own synchronous handoff loop instead.

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
- `sequentagent` — run two agents in sequence on one phase, each in its own
  worktree, rebasing the second onto the first's integrated branch.
- `reference` — create or refresh a static snapshot worktree at
  `.worktrees/reference/`.

Branch integration uses `GitFlow::ensure_branch()` (create at a start point
without checking out) and `fast_forward_branch()` (move a ref forward only
if it is a descendant — refuses non-fast-forward updates).

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
   Approval runs `finish_workflow()`: the `VersionBump` + `BranchCleanup`
   hooks, gate cleanup, and `workflow::clear_state()`. A rejection loops
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
- **`workflow.rs`** — state I/O (`save_state`, `load_state`, legacy
  migration) logs at `DEBUG`; unreadable/corrupt state files warn at `WARN`
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

DevFlow is agent-agnostic; agent-specific code lives only in `agents/*.rs` and
the targeted result parsing. Adding a backend is a checklist, not a fixed
"3 changes" — keep these in sync or tests/builds fail:

1. Add an adapter file under `crates/devflow-core/src/agents/` implementing the
   `AgentAdapter` trait.
2. Add a variant to the `AgentKind` enum in `state.rs`.
3. Update the `FromStr` parser, `Display`, and `AgentParseError` text in
   `state.rs` to accept/emit the new name.
4. Add a match arm in `agents::adapter_for()`.
5. Add the `pub mod` and `pub use` exports in `agents/mod.rs`.
6. Add/extend tests (adapter name, parser aliases, prompt-sharing).
7. Update docs (`README.md`, `DEPENDENCIES.md`, `CONTRIBUTING.md`, this file).
