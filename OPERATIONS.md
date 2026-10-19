# DevFlow Operations Reference

The accurate, operator-facing reference for running DevFlow v2 day to day.
This page describes the CLI **as built** (verified against `main.rs` by the
`--help` snapshot test); the README/ARCHITECTURE rewrite lands in Phase 15b.

## The pipeline

Each phase runs a linear stage chain, driven end-to-end by detached monitors:

```
Define → Plan → Code → Validate → Ship
```

- **Define / Plan / Code** launch a headless coding agent with the stage's
  GSD command.
- **Validate** requires an explicit `verdict: pass` from the agent to
  advance; gaps loop back to Code (after `MAX_CONSECUTIVE_FAILURES` = 3
  failures a gate is forced).
- **Ship always gates** — a human approves before the terminal hooks
  (version bump, branch cleanup) run.
- Modes: `--mode auto` (only the Ship gate, plus never-silent failure
  gates) or `--mode supervise` (also gates every Validate).
- Every run gets an isolated git worktree under `.worktrees/phase-NN/` by
  default (`--no-worktree` opts out).

## Commands

| Command | Purpose |
|---|---|
| `devflow start --phase N --agent claude\|codex\|opencode --mode auto\|supervise [--force] [--no-worktree] [--dry-run] [--until define\|plan\|code\|validate]` | Begin a phase; a detached monitor owns the agent and auto-advances. `--until <stage>` runs through `<stage>` then halts cleanly (no orphaned monitor, no `doctor` false positive) instead of continuing to Ship; `--until ship` is rejected (a no-op — Ship is already terminal). Refuses before scaffolding anything if phase N is not reachable from `develop` (see Preflight refusals below) |
| `devflow resume --phase N` | Resume a rate-limited or infra-paused phase from its saved stage (loads `state-NN.json`, does not recreate the branch/worktree or reset to Define) — the command a primary-loop rate-limit auto-resume cron invokes. Also clears a `--until`-stopped phase's stop marker, so the phase advances past its old stop point instead of re-stopping immediately |
| `devflow status` | All active phases: stage, gate state, supervisor liveness (ALIVE / STALE / GONE, with the repair; see [Run liveness](#run-liveness)), elapsed, last action |
| `devflow logs [-f] [--phase N] [--stderr]` | Print/follow a phase's captured agent output |
| `devflow history [N]` | Show chronological events with retained capture and review evidence for a phase |
| `devflow gate list` | Gates awaiting a response |
| `devflow gate approve <phase> [--stage S] [--note ...]` | Approve a gate — the workflow advances |
| `devflow gate reject <phase> --note ... [--stage S]` | Reject — loops back to Code; a note containing `abort` ends the phase |
| `devflow gate sweep [--max-age-secs N] [--dry-run] [--root PATH]` | Answer or report aged, unattended gates across every registered root (or one `--root`) — bounds an abandoned run's lifetime without `kill(1)`. On-demand only; nothing schedules it. Never writes an approval |
| `devflow parallel --phases 7,8 [--agents claude,codex] [--mode M] [--force]` | Run phases concurrently, each in its own worktree + monitor |
| `devflow list` | Feature branches with divergence from develop |
| `devflow reference [--branch B] [--refresh]` | Static snapshot worktree at `.worktrees/reference/` |
| `devflow stop --phase N [PROJECT] [--root PATH] [--launch ID]` | End a running phase cleanly (23c). In this order: **(1)** an open gate with a confirmed live waiter is answered with a rejection — the waiter unwinds through its own abort path, no signal sent. **(2)** Otherwise the run's supervisor socket is probed: the one `.devflow/state-NN.json` records, or, when that file is missing or does not load, the one live socket found through its sidecar for this project and phase. **ALIVE** → a `shutdown` request: the supervisor ends the agent's process group (exit code 143) and runs no advance. **STALE** → the guarded backstop: it signals (SIGTERM, then SIGKILL) only an agent group whose leader is still present with the recorded start time and still leads the group; a socket from a previous boot, an empty group or a recycled leader pid has its socket and sidecar removed with nothing signalled; a group whose leader is gone is refused and its members listed, with `ps -o pid,pgid,sid,lstart,args -p "$(pgrep -d, -g <pgid>)"` to inspect them (residual limit: nothing proves those members are the agent's, so `stop` never signals them — end them yourself). A probe that cannot answer is refused, nothing signalled. **(3)** Only when no live socket remains (**GONE**, or nothing recorded) is the process holding `.devflow/lock-{phase:02}` signalled, after confirming it is alive and its pid and start time match the identity the lock recorded (a recycled pid, or a lock that records no start time, is refused). No pid read from the state file is ever signalled. Idempotent; records `stopped`/`stop_reason`, except after arm (1) while the gate's waiter still holds the phase lock: that waiter ends the phase through its abort path, which clears the state instead. Either way `cleanup`'s fail-closed refusal recognizes the phase as no longer live — `stop` then `cleanup --force` compose in that order. When the state file does not load, `stop` still ends the run, says `stopped` was not recorded and leaves the file untouched; several live sockets with no usable state are refused, with one `devflow stop --phase N --launch <id>` line each. `--launch <id>` tears down only the socket with that launch id — the form `devflow doctor` prints for an orphan, usable from any directory — and answers no gate, signals no lock holder and writes no state |
| `devflow cleanup [--force]` | Remove phase worktrees + their feature branches |
| `devflow recover [--clean] [--phase N]` | Inspect state; `--clean` sweeps stale phases only; `--clean --phase N` clears one phase unconditionally |
| `devflow test` | cargo test + clippy + fmt --check |
| `devflow doctor [--json]` | Environment audit (agents installed, versions, RUST_LOG) plus per-phase reconciliation, including each phase's supervisor liveness and a read-only sweep for orphan supervisor sockets (see [Run liveness](#run-liveness)); `--json` emits one object with the keys `environment`, `reconciliation`, `planning_doc_staleness`, `stray_processes`, `orphan_supervisors` and `orphan_supervisor_sweep_error` |
| `devflow release --check` | Read-only release-cut preflight: workspace self-pin, develop/main divergence (no `git fetch` — reads already-fetched refs), crates.io publish order, and changelog-version agreement (999.96). `--check` is required; a bare `devflow release` is rejected toward the deferred release-cut executor (merge/tag/sync/publish, DEN-50) |
| `devflow ship --phase N [--force]` | **Dead-monitor recovery.** `devflow gate approve` only *writes* the Ship gate's response file — a live monitor polling for it is what actually advances the workflow. If that monitor died before consuming the response (e.g. the machine restarted mid-pipeline), the approval sits unconsumed forever. `devflow ship` reads the already-written Ship response directly and drives the phase through the same terminal path (`finish_workflow`) the live monitor would have — requires `state.stage == Stage::Ship` and an existing request+response pair with no prior ack. `--force` is accepted for explicit operator intent but never skips the stage check, the per-phase lock, the gate-existence check, or the ack check — it can never be used to skip Validate or jump ahead of a healthy pipeline. If the Ship response routes to a rejection that loops back to Code, `devflow ship` launches a **new, detached monitor agent** to drive the retry — the command prints this explicitly so it is never a silently long-running process. |
| `devflow evidence --phase N [--json] [--require-shipped] [--root PATH]` | **Read-only structural oracle (23-06):** reports DevFlow's own append-only record of whether a phase actually shipped, instead of trusting an agent-authored attestation document. `shipped` is strictly true only after the terminal-only `workflow_shipped` event has been emitted (the last step inside `finish_workflow_with_gate_timeout`, once the entire post-Ship hook batch has succeeded) — a phase halted by `--until <stage>` always reports `shipped: false`, even though it emits the older, ambiguous `workflow_finished` event too (surfaced separately as `workflow_finished_seen`/`finished_reason` for corroboration, never consulted by `shipped`). `--require-shipped` exits non-zero unless `shipped` is true, so it is declarable as a `verify::external_verify_commands` Layer 0 probe (opt-in per phase; a phase's PLAN must declare it and the operator must approve it via `DEVFLOW_TRUST_EXTERNAL_VERIFY`) — a failed declared probe outranks every agent-controlled signal. |

(`devflow advance` is internal and takes `--phase N`. A supervisor does not
run it as a command; it advances in-process. `devflow status` names it as a
repair only for a gate-pending phase whose supervisor is GONE.)

(`devflow monitor` is internal and registered as the hidden `__monitor`
subcommand — it is the stage supervisor's own process body. Every stage
launch re-execs it detached, in its own process group, in one of two launch
shapes: **capture** (agent stdin `/dev/null`, stdout and stderr to the phase
capture files) for adapters other than Claude and Antigravity (such as `codex`
and `opencode`), the
`--legacy-claude-launch` opt-out and a checkpoint resume; or **stream** (the
supervisor owns the agent's stdin and stdout pipes and applies the idle
timeout) for a Claude or Antigravity `stream-json` launch. Both shapes run the
same Rust supervisor: it starts the agent in the agent's own process group,
records its pid and exit code, answers on a per-launch socket (see [Run
liveness](#run-liveness)), and when the agent exits runs the advance
in-process; the next stage's supervisor publishes its socket before this one
removes its own. Never run it by hand: it expects to own the agent and writes
the phase's capture, exit-code and agent-pid files as it goes.)

## Run liveness

Whether a phase's run is alive is answered from its supervisor socket, never
from a pid. Each launch records its socket's path in `.devflow/state-NN.json`
(the `supervisor` record); `devflow status`, `doctor`, `stop`, `cleanup`,
`recover --clean`, `start` and `resume` probe that recorded path (a connect
and one `ping`) and get one of three answers:

| State | Meaning | Repair |
|---|---|---|
| **ALIVE** | The socket answered the ping. The reply names the current owner (agent running, advancing, gate wait, …) and the launch shape. A moment between stages is ALIVE, because the supervisor advances in-process | None needed. To end the run: `devflow stop --phase N` |
| **STALE** | The socket file is still there and owned by you, but nothing accepts the connect: the supervisor died without cleaning up, and its agent group may still be running | `devflow stop --phase N` (the backstop ends a validated agent group and removes the socket and sidecar), then `devflow resume --phase N` |
| **GONE** | No file at the recorded path: the supervisor has exited. Normal when the phase is stopped; when it is not, nothing will advance the phase | `devflow resume --phase N` (when a gate is pending, answer it and run `devflow advance --phase N`) |

How each command shows them:

- **`devflow status`** prints `supervisor: <socket path> — <state>` for each
  phase, and under it the repair: `devflow stop --phase N` then
  `devflow resume --phase N` for STALE; `devflow resume --phase N` (plus
  `devflow advance --phase N` when a gate is pending) for GONE while the phase
  is not stopped. A phase with no supervisor recorded reads `not launched`.
- **`devflow doctor`** reports STALE, GONE-while-not-stopped and a failed probe
  as problems, naming the phase and launch id but no path. Its repairs:
  `devflow stop --phase N && devflow resume --phase N` for STALE,
  `devflow resume --phase N` for GONE, `devflow status` for a failed probe.
  ALIVE, a stopped GONE and an unlaunched phase are silent. `doctor --json`
  gives each phase a `supervisor` object (`liveness`, `owner`, `launch`,
  `launch_id`).
- **"probe failed".** A probe that cannot decide (the path is not a socket,
  belongs to another user, cannot be stat'ed or connected to, or the peer
  gives no valid ping reply) reads `UNKNOWN — the probe failed: …`. It is
  never guessed to be ALIVE or GONE: `status` names the socket to inspect,
  `doctor`'s repair is `devflow status`, `stop` refuses and signals nothing,
  and `cleanup` keeps the worktree unless `--force`.
- **`devflow cleanup`** refuses an ALIVE run even with `--force` — end it
  first with `devflow stop --phase N`, the line its refusal names, or wait
  for it. It keeps a STALE run's worktree, or one whose probe failed, unless
  `--force`.
- **`devflow recover --clean --phase N`** refuses while the phase's supervisor
  is ALIVE and deletes nothing.
- **`devflow start` / `devflow resume`** refuse while a run is live: an ALIVE
  socket, a STALE one unless the backstop's checks prove its agent group gone,
  or a probe that fails. When the state file is missing or does not load,
  they search the socket directory for this project and phase instead, and
  refuse when that directory exists but cannot be listed. The refusal names a
  `devflow stop` line for each live socket.

**Orphan sockets.** `devflow doctor` also lists, read-only, under
`supervisor sockets no state claims:`, each socket of this project whose state
is missing, does not load, records no supervisor or names another launch;
each socket of this project with a missing or unparsable sidecar; and,
machine-wide, each socket whose project root no longer exists. Each is
repaired by `devflow stop --phase N --launch <id>`. `doctor` changes nothing;
`doctor --json` reports them as `orphan_supervisors`, and a socket directory
it cannot list as `orphan_supervisor_sweep_error` rather than as no orphans.
A launch caught between publishing its socket and saving its record can be
listed this way for a moment.

**Where the sockets live.** Under `registry::cache_dir()/sock`: the cache
directory is `DEVFLOW_CACHE_DIR` when set, then `$XDG_CACHE_HOME/devflow`,
then `~/.cache/devflow`. A launch with none of `DEVFLOW_CACHE_DIR`,
`XDG_CACHE_HOME` and `HOME` set fails (`cannot resolve DevFlow's cache
directory for this launch`) instead of running without a socket. Each socket
is named `<project digest>-<phase>-<launch id>.sock`, with a `.json` sidecar of
the same stem recording the launch (project root, phase, agent process group,
start time, boot id). The `sock` directory is mode 0700 and refused if it is a
symlink or owned by another user; sockets and sidecars are 0600. A socket path
longer than Linux's 107-byte `sun_path` limit is refused by name (`socket path
is N bytes, longer than the 107-byte sun_path limit`) before anything is
bound; point `DEVFLOW_CACHE_DIR` at a shorter directory if the cache path is
deep.

**Signals.** `SIGTERM` or `SIGINT` sent to a supervisor process does what
`devflow stop`'s ALIVE arm does: a running agent's group is ended, no next
stage is launched, and `stopped` is recorded. A supervisor parked at a gate
clears the gate and records `stopped` rather than aborting the phase, so the
phase stays resumable.

**Platform.** Socket supervision is unverified on macOS: the 103-byte macOS
`sun_path` limit is enforced by a constant only, and no macOS host or CI has
run it. Off Linux the STALE backstop refuses before reading anything, so a
STALE socket there is never torn down by `devflow stop`.

**Upgrading from 2.x.** Install a 3.x binary only while no 2.x run is live;
3.x makes no promise about a 2.x state.json, which may read as GONE or fail to
load.

## Preflight refusals

Checks `devflow start` performs, in order, before it scaffolds anything (a
worktree, a feature branch, or `.devflow/` state):

| Check | Condition | Operator action |
|---|---|---|
| Agent binary present | The agent's executable (`claude`/`codex`/`opencode`) is not found on `PATH` | Install the agent, or run `devflow doctor` |
| Phase reachable from `develop` (23f) | Phase N's `### Phase N:` ROADMAP heading or its `.planning/phases/NN-*/` directory is missing from `develop` — skipped entirely if `develop` has no `.planning/ROADMAP.md` at all | Merge the branch carrying the phase's planning artifacts into `develop`, then re-run |
| Codex headless Define | `--agent codex` and phase N has no `-CONTEXT.md` on `develop` — Codex's `exec` mode cannot answer Define's interview headlessly | Run `/gsd-discuss-phase N` interactively first (any agent), or use `--agent claude` |

## Answering gates

When a gate fires you'll get the notify hook (below) and `devflow status`
shows `gate: pending`. Answer from any terminal:

```bash
devflow gate list
devflow gate approve 15 --note "lgtm"
devflow gate reject 15 --note "tests are thin, tighten coverage"   # loops to Code
devflow gate reject 15 --note "abort: wrong direction"             # ends the phase
```

`--stage` is only needed when one phase somehow has several open gates.
Under the hood this writes `.devflow/gates/NN-<stage>.response.json`
atomically; the supervisor parked at the gate checks for it about once a
second (a gate waited on outside a supervisor, such as `devflow ship`'s
foreground wait, backs off up to ~60s), acks, and moves on. The CLI refuses to
overwrite an unconsumed response.

## Notify hook (never miss a gate)

Set `DEVFLOW_GATE_NOTIFY_CMD` to any shell command; it runs on every gate
with metadata in env vars — never interpolated into the command:

```bash
# ntfy.sh example
export DEVFLOW_GATE_NOTIFY_CMD='curl -s -d "devflow gate: phase $DEVFLOW_GATE_PHASE $DEVFLOW_GATE_STAGE — $DEVFLOW_GATE_CONTEXT" ntfy.sh/my-topic'
```

Env provided to the hook: `DEVFLOW_GATE_PHASE`, `DEVFLOW_GATE_STAGE`,
`DEVFLOW_GATE_CONTEXT`, `DEVFLOW_NON_SILENT_GATE` (`1` when the gate exists
only because a stage failed unexpectedly).

The command is saved in the run's state file (`gate_notify_cmd` in
`.devflow/state-NN.json`), so gates fired by the detached monitor use it
without your shell's environment. `devflow start`, `resume` and `advance`
update the saved command from the variable: unset keeps it, a new value
replaces it, and an empty value (`DEVFLOW_GATE_NOTIFY_CMD= devflow resume …`)
turns notification off for the run. A scheduled rate-limit resume is itself a
`devflow resume`, so a variable set in the scheduler's environment applies to
it the same way.

`resume` refuses while a run is live (an agent running or a gate waiting), so
it changes the hook only for a stopped run. To change the hook of a live run,
edit `gate_notify_cmd` in the state file. An edit made while a gate is waiting
is kept when the gate is answered; an edit made while a stage is still running
can be overwritten when its gate opens (backlog 999.154). The coding agent
never receives the variable.

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `DEVFLOW_GATE_NOTIFY_CMD` | unset | Shell command fired when a gate is written |
| `DEVFLOW_GATE_TIMEOUT_SECS` | 259200 (3d) | How long a monitor stays alive holding the phase lock while parked at a gate. Not a deadline for answering: the gate request and phase state are files that never expire, and timing out is a clean resumable stop — past it you answer the gate and run `devflow resume` instead of the answer being picked up automatically |
| `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS` | 60 | How long `devflow ship --phase`'s foreground manual override waits for a reopened Ship gate (terminal-hook failure) before failing fast, instead of `DEVFLOW_GATE_TIMEOUT_SECS`' multi-day default |
| `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` | 120 | Wait on the shared-checkout lock; on timeout the hook batch is skipped (loudly), never run unserialized |
| `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` | 259200 (3d) | Age threshold `devflow gate sweep` uses to decide a gate is abandoned. Held **equal to** `DEVFLOW_GATE_TIMEOUT_SECS`, never shorter: `gate sweep` writes an `abort:` response, so a threshold below the poll timeout lets a sweep reap gates a live monitor is still polling — turning a clean resumable timeout into an `abort()` that clears state, machine-wide across every registered root. An unparsable value or explicit `0` falls back to the default rather than reaping every gate on the machine |
| `DEVFLOW_CACHE_DIR` | unset (falls back to `$XDG_CACHE_HOME/devflow`, then `$HOME/.cache/devflow`) | Test/override hook for the machine-global registry directory (`devflow gate list --all-roots`) |
| `DEVFLOW_CAPTURE_RETENTION` | 5 | Capture generations retained per phase; overrides `devflow.toml` |
| `DEVFLOW_REVIEW_ANGLES` | built-in five-angle list | Comma-separated Ship review angles; overrides `devflow.toml` |
| `DEVFLOW_EXTERNAL_VERIFY_ENABLED` | true | Enable PLAN-declared external post-condition probes; overrides `devflow.toml` |
| `DEVFLOW_BASE_BRANCH` | `develop` | The branch phase worktrees fork **from** and phase work merges back **into** (D-01, `45-CONTEXT.md`); overrides `devflow.toml`'s `base_branch` key. Set it when your `.planning/` artifacts live on a branch other than `develop` — a worktree forked from `develop` does not carry a `.planning/config.json` that exists only on a planning branch, and the unattended preflight then refuses every launch (999.110/AUTO-01). **An explicitly supplied value that is `main`, blank, or begins with `-` is a hard error, not a silent fallback to `develop`**, and the value must name an existing local branch — a remote-tracking name, a `refs/heads/` path, `HEAD` or a commit SHA is refused. A resolved base other than `develop` is announced at `devflow start`, naming the source |
| `DEVFLOW_YES_SHIP` | false | Standing Ship gate pre-authorization (D-12, `28-CONTEXT.md`); overrides `devflow.toml`'s `yes_ship` key, which itself is OR-combined with `--yes-ship` at `devflow start` |
| `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS` | 120 | The per-agent idle timeout for the pipe-owning Antigravity monitor (phase 41, D-08): how long the monitor waits with NO output on the child's `stream-json` stream before recording an `idle_timeout` verdict and terminating it. Same 120s floor and clamp semantics as `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS`. The default is a DECIDED starting point, not a silent inheritance of the Claude-measured floor — it is to be revisited after the first real Antigravity cadence measurement. Claude's variable is never read for Antigravity and vice versa |
| `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` | 120 | How long the pipe-owning Claude monitor waits with NO output on the child's stream before recording an `idle_timeout` verdict and terminating it. Clamped to a 120s floor: a lower value is raised and the clamp is printed loudly at `devflow start`. The floor is 4x the CLI's measured `tool_progress` keepalive interval (a fixed 30.00s, measured 2026-08-03 over five workload-controlled trials on CLI 2.1.220), so it survives three consecutive missed keepalives; the earlier 30s floor sat exactly ON the keepalive interval and would have killed healthy stages running any tool call longer than ~30s, including `cargo test --workspace`. Every stream line resets the window, so a healthy long-running stage is never bounded by wall clock |
| `DEVFLOW_CLAUDE_LEGACY_LAUNCH` | false | D-11's escape hatch (31-04): force the pre-31 single-document Claude launch — prompt positionally in argv, `--output-format json`, the capture launch shape — instead of the Phase 31 `stream-json` transport. OR-combined with `--legacy-claude-launch` at `devflow start` / `devflow resume` and persisted to `state-NN.json`, so a detached monitor honours it; a run authorized by this variable alone prints a notice naming it as the source. **Parsed as a bool: `=false` does NOT enable it, and an unparseable value warns and leaves the legacy path OFF.** Using it is announced on three channels (stdout, `phase-NN-monitor.log`, and a `claude_legacy_launch_forced` event in `events.jsonl`) because an escape hatch used routinely erodes what it protects. **What it gives up: the legacy path cannot deliver background-task notifications, so a multi-plan wave may orphan delegated work — that is 999.64, the defect Phase 31 exists to fix.** Once set for a run it is never cleared by `devflow resume`; edit `state-NN.json` to turn it off |
| `RUST_LOG` | `info` | Log verbosity (stderr) |
| `DEVFLOW_LOG_FORMAT` | plain | `json` for machine-readable log lines |
| `DEVFLOW_E2E_CHILD_TIMEOUT_SECS` | 90 | Test-only: bounds `gate_sweep_e2e.rs`'s patience with a spawned `devflow advance` child so CI cannot hang indefinitely; not read by any production code path |

## `.devflow/` file inventory

| File | What it is |
|---|---|
| `state-NN.json` | A phase's persisted stage-machine state (per phase since 14a) |
| `lock-NN` | Per-phase advance lock (held across gate waits) |
| `lock-project` | Coarse checkout lock (held seconds, around shared-git mutations) |
| `phase-NN-stdout` / `phase-NN-stderr.log` | Agent capture files (what `logs` tails) |
| `phase-NN-exit` / `phase-NN-agent-pid` | Exit code + PID the supervisor records |
| `gates/NN-<stage>.json` (+ `.response.json`, `.ack.json`) | Gate request / answer / receipt |
| `events.jsonl` | Append-only event log (schema v1, one JSON object per line, phase id on every line) — tail it from any tool |
| `cron-instructions-NN.json` | Rate-limit resume record naming `devflow resume --phase N` for a paused run |
| `history/phase-NN/` | Bounded archive of prior stage stdout/exit captures |

Everything under `.devflow/` and `.worktrees/` is runtime state
(git-ignored); `devflow recover --clean` is the sanctioned reset.

## A typical dogfood session

```bash
devflow start --phase 15 --agent claude --mode auto .
devflow status                       # any time, from any terminal
devflow logs -f --phase 15           # watch the agent work
# ...notify hook pings you at the Ship gate...
devflow gate list
devflow gate approve 15 --note "reviewed the PR-ready diff"
devflow status                       # idle — phase shipped, version tagged
```

When something wedges: `devflow recover` to inspect,
`devflow recover --clean` (stale phases, plus gate files whose phase has no
state; exits non-zero if anything could not be removed) or
`devflow recover --clean --phase N` to reset one phase, then re-`start`.
