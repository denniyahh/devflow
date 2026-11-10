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
  pass; gaps loop back to Code (after `MAX_CONSECUTIVE_FAILURES` = 3
  failures a gate is forced). Validate is the last stage that runs: a pass
  ends the run as finished and leaves the worktree and branch in place
  ([How a run ends](#how-a-run-ends)). No transition reaches Ship.
- Modes: `--mode auto` (finishes on a passing Validate without a gate;
  only never-silent failure gates and the failure caps stop it) or
  `--mode supervise` (also gates every Validate; approving it finishes the
  run).
- Every run gets an isolated git worktree under `.worktrees/phase-NN/` by
  default (`--no-worktree` opts out).

## Commands

| Command | Purpose |
|---|---|
| `devflow start --phase N --agent claude\|codex\|opencode --mode auto\|supervise [--force] [--no-worktree] [--dry-run] [--until define\|plan\|code\|validate] [--fallback-agent <agent>]` | Begin a phase; a detached monitor owns the agent and auto-advances. `--until <stage>` runs through `<stage>` then halts cleanly (no orphaned monitor, no `doctor` false positive) instead of continuing to Ship; `--until ship` is rejected (a no-op — Ship is already terminal). Refuses before scaffolding anything if phase N is not reachable from `develop` (see Preflight refusals below). `--fallback-agent <agent>` names one agent that takes over if the Code stage is rate-limited: one hop per run, at Code only (a rate limit at any other stage takes the cron arm), relaunching Code in the same worktree on the same branch. It is saved in `state-NN.json`, and refused when it equals `--agent`, when its binary is missing or its driver unhealthy, and in `--mode auto` unless it is `claude` or `antigravity`. That launch check is best effort: the full preflight runs again at the handoff, a refusal there takes the cron arm, and a relaunch that fails after the handoff stops at a Code gate (see [Rate-limit failover](#rate-limit-failover)). Without the flag, `start` uses `devflow.toml`'s `fallback_agent` key and prints a note naming it; the flag wins the value, and no environment variable selects a fallback |
| `devflow resume --phase N [--agent <agent>] [--fallback-agent <agent>]` | Resume a rate-limited or infra-paused phase from its saved stage (loads `state-NN.json`, does not recreate the branch/worktree or reset to Define) — the command a primary-loop rate-limit auto-resume cron invokes. Also clears a `--until`-stopped phase's stop marker, so the phase advances past its old stop point instead of re-stopping immediately. It reruns the stage, so a pending gate is asked again: it sets any unused answer aside and reports it, and reports an answer that was claimed with no record that it was applied (see [Answering gates](#answering-gates)). `--agent <agent>` hands the run to another agent (an `agent_handoff` event with reason `resume --agent`); naming the run's saved, unused fallback is refused unless `--fallback-agent` names another. `--fallback-agent <agent>` sets the fallback or re-arms a spent one, checked as on `start`; like any resume it is accepted only on a run that is not live. Without it the saved fallback is kept as it is, spent or not: `resume` does not re-read `devflow.toml` |
| `devflow status` | All active phases: stage, gate state, supervisor liveness (ALIVE / STALE / GONE, with the repair; see [Run liveness](#run-liveness)), elapsed, last action. Also shows a held state write lock with its holder, names a live agent that has no supervisor with the manual kill, and lists set-aside gate answers |
| `devflow logs [-f] [--phase N] [--stderr]` | Print/follow a phase's captured agent output |
| `devflow history [N]` | Show chronological events with retained capture and review evidence for a phase |
| `devflow gate list` | Gates awaiting a response |
| `devflow gate approve <phase> [--stage S] [--note ...] [--gate-id ID]` | Approve a gate — the workflow advances |
| `devflow gate reject <phase> --note ... [--stage S] [--gate-id ID]` | Reject — loops back to Code; a note containing `abort` ends the phase |
| `devflow gate sweep [--max-age-secs N] [--dry-run] [--root PATH]` | Answer or report aged, unattended gates across every registered root (or one `--root`) — bounds an abandoned run's lifetime without `kill(1)`. On-demand only; nothing schedules it. Never writes an approval |
| `devflow stop --phase N [PROJECT] [--root PATH] [--launch ID]` | End a running phase cleanly (23c). In this order: **(1)** an open gate with a registered live waiter is answered with an abort rejection — the waiter unwinds through its own abort path, no signal sent. A Ship gate with no waiter stores that rejection for `devflow ship`. A gate whose answer a consumer already claimed is left alone. If `.devflow/.gates-NN.lock` stays held past 5 s, `stop` says so, answers nothing and goes on to the next arms, so a wedged `gate approve` cannot leave a run unstoppable. **(2)** Otherwise the run's supervisor socket is probed: the one `.devflow/state-NN.json` records, or, when that file records none (a `start` still waiting for its supervisor to publish), is missing or does not load, the one live socket found through its sidecar for this project and phase. **ALIVE** → a `shutdown` request: the supervisor ends the agent's process group (exit code 143) and runs no advance. A stop honoured at an advance safe point is named as that, since nothing was signalled, and the exit file then records 143 too. A supervisor that accepted the stop but whose final answer did not arrive in time is reported as accepted, not confirmed: it honours the stop at its next safe point, and `devflow status` shows when it has. **STALE** → the guarded backstop: it signals (SIGTERM, then SIGKILL) only an agent group whose leader is still present with the recorded start time and still leads the group; a socket from a previous boot, an empty group or a recycled leader pid has its socket and sidecar removed with nothing signalled; a group whose leader is gone is refused and its members listed, with `ps -o pid,pgid,sid,lstart,args -p "$(pgrep -d, -g <pgid>)"` to inspect them (residual limit: nothing proves those members are the agent's, so `stop` never signals them — end them yourself). A probe that cannot answer is refused, nothing signalled. A launch the state names as pending comes next, and only its socket reaches it: `stop` waits up to 15 s for an unpublished one and never signals it. If it never publishes, a foreground launcher still holding the phase is reached through arm (3), and with none `stop` reports the launch and the pid its liveness lock records, records nothing and names the manual route: end that process yourself, then run `stop` again to clear the reservation. A reservation nothing holds is abandoned: `stop` clears it and says so. **(3)** Only when no socket answers (**GONE**, or nothing recorded) is the phase's foreground holder signalled — a `start`/`resume` before its spawn, `devflow ship`, or a hand-run `devflow advance`, which holds `.devflow/.holder-NN` for its lifetime — after confirming its pid and start time match the identity that file recorded (a recycled pid, or a record with no start time, is refused). No pid read from the state file is ever signalled. The holder catches the signal and ends at its next safe point (1–6; a foreground gate wait at 4), recording `stopped by devflow stop at a foreground safe point` and logging `advance_stopped` with `source: foreground_holder`. A holder still running 3 s later is reported as not marked, exit non-zero, and it records the stop itself. **Finalization.** The boundary is before `Merge`, or after the pass ends: `devflow ship` reads a stop only at safe point 6, before a finalization pass. Once the pass starts it runs `Merge`, `VersionBump`, `ChangelogAppend` and `BranchCleanup` to the end; a failed pass's retry gate then honours the stop at safe point 4, and a successful pass ships. So a stop that arrives after the pass started lets the run finish shipping. What you see: `stop` prints its not-marked message (the pid was signalled, phase state was not marked stopped, and a `devflow ship` that has started its finalization pass runs it to the end and, if the pass succeeds, ships instead of stopping) and exits non-zero; or, when the ship finishes within the 3 s wait, `stop` says the signalled holder exited and left no phase state, that the run ended on its own, and that nothing was marked stopped. Afterwards the record is the shipped run's (`workflow_shipped`, `workflow_finished`, no state file) with no `advance_stopped`: nothing records that the stop arrived. There is no safe point between hooks and no resume path that skips a hook already run. Limit: a terminal Ctrl-C sends SIGINT to the whole foreground process group, so a hook's own `git` child receives it directly; this covers `devflow stop` and signals sent to the holder pid only. **An agent with no supervisor.** With no socket, pending launch or holder left but the phase's agent still alive, `stop` names the agent's pid and the manual kill (`kill -TERM -- -<pid>`, after `ps -o pid,pgid,lstart,args -p <pid>` when the pid is unconfirmed), writes nothing and exits non-zero. `devflow stop` cannot end that agent (backlog 999.170). The pid is unconfirmed when it comes only from `.devflow/phase-NN-agent-pid`: a pid reused by an unrelated process makes `stop` exit non-zero until that process ends, and the way out is to remove the pid file (end that process only if it is yours to end). **What it records.** Idempotent; records `stopped`/`stop_reason`. After arm (1) the waiter's abort path then clears the state. Either way `cleanup`'s fail-closed refusal recognizes the phase as no longer live — `stop` then `cleanup --force` compose in that order. `stop` does not mark a run `stopped` when, by the time it writes, the state names a launch none of its arms ended, answered, cleared or found not running: one a signalled holder published, a pending launch that promoted while `stop` ran, or one started meanwhile. Run `stop` again: a launch that turns out dead is found not running by that second `stop`, which then marks the run. `stop` waits at most 10 s for the state write lock, then names its holder and writes nothing. When the state file does not load, `stop` still ends the run, says `stopped` was not recorded and leaves the file untouched; several live sockets with no usable state are refused, with one `devflow stop --phase N --launch <id>` line each. `--launch <id>` tears down only the socket with that launch id — the form `devflow doctor` prints for an orphan, usable from any directory — and answers no gate, signals no holder and writes no state. `stop` adds no exit codes |
| `devflow cleanup [--force]` | Remove phase worktrees + their feature branches |
| `devflow recover [--clean] [--phase N]` | Inspect state; `--clean` sweeps stale phases only; `--clean --phase N` clears one phase. Either form keeps a phase whose recorded or pending supervisor, gate waiter or foreground holder may be live, deciding on a fresh read under the state write lock, so a run that starts while it works is kept. It waits at most 10 s for that lock, then keeps the phase and names the holder. The `--clean` sweep also removes abandoned `supervisor-*.live` and `*.waiter` markers and orphan `*.hold-tmp` temps, and lists set-aside gate answers as kept; both forms remove stale `lock-NN` leftovers. Neither ever removes `.holder-NN`, `.state-NN.lock`, `.gates-NN.lock` or a set-aside answer |
| `devflow doctor [--json]` | Environment audit (agents installed, versions, RUST_LOG) plus per-phase reconciliation, including each phase's supervisor liveness and a read-only sweep for orphan supervisor sockets (see [Run liveness](#run-liveness)); `--json` emits one object with the keys `environment`, `reconciliation`, `planning_doc_staleness`, `stray_processes`, `orphan_supervisors` and `orphan_supervisor_sweep_error` |
| `devflow ship --phase N [--force]` | **Dead-monitor recovery.** `devflow gate approve` only *writes* the Ship gate's answer — a live supervisor waiting on it is what actually advances the workflow. If that supervisor died before claiming the answer (e.g. the machine restarted mid-pipeline), the approval sits unconsumed forever. `devflow ship` claims the stored answer exclusively and drives the phase through the same terminal path (`finish_workflow`) the live supervisor would have — requires `state.stage == Stage::Ship` and an open Ship gate holding an answer bound to it, not yet claimed. It holds `.devflow/.holder-NN` for its lifetime, and because it is dead-monitor recovery it refuses while a recorded or pending supervisor of the phase is live, logging `ship_override_refused` and naming `devflow stop --phase N`. `--force` is accepted for explicit operator intent but never skips the stage check, the foreground holder, that refusal, the gate-existence check, or the answer claim — it can never be used to skip Validate or jump ahead of a healthy pipeline. If the Ship answer routes to a rejection that loops back to Code, `devflow ship` launches a **new, detached monitor agent** to drive the retry — the command prints this explicitly so it is never a silently long-running process. For what `devflow stop` does to a running `devflow ship`, see its row above |
| `devflow evidence --phase N [--json] [--require-shipped] [--root PATH]` | **Read-only structural oracle (23-06):** reports DevFlow's own append-only record of whether a phase actually shipped, instead of trusting an agent-authored attestation document. `shipped` is strictly true only after the terminal-only `workflow_shipped` event has been emitted (the last step inside `finish_workflow_with_gate_timeout`, once the entire post-Ship hook batch has succeeded) — a phase halted by `--until <stage>` always reports `shipped: false`, even though it emits the older, ambiguous `workflow_finished` event too (surfaced separately as `workflow_finished_seen`/`finished_reason` for corroboration, never consulted by `shipped`). `--require-shipped` exits non-zero unless `shipped` is true, so it is declarable as a `verify::external_verify_commands` Layer 0 probe (opt-in per phase; a phase's PLAN must declare it and the operator must approve it via `DEVFLOW_TRUST_EXTERNAL_VERIFY`) — a failed declared probe outranks every agent-controlled signal. |

(`devflow advance` is internal and takes `--phase N`. A supervisor does not
run it as a command; it advances in-process. `devflow status` does not suggest
it, because answering a gate and then running it sets the answer aside, and a
hand-run advance refuses while a supervisor of the phase is live.)

(`devflow monitor` is internal and registered as the hidden `__monitor`
subcommand — it is the stage supervisor's own process body. Every stage
launch re-execs it detached, in its own process group, in one of two launch
shapes: **capture** (agent stdin `/dev/null`, stdout and stderr to the phase
capture files) for adapters other than Claude and Antigravity (such as `codex`
and `opencode`) and a checkpoint resume; or **stream** (the
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
| **GONE** | No file at the recorded path: the supervisor has exited. Normal when the phase is stopped; when it is not, nothing will advance the phase | `devflow resume --phase N`, which reruns the stage and asks again. When the Ship gate is pending, answer it and run `devflow ship --phase N`; any other pending gate is asked again by `resume`. When the agent is still running with no supervisor, `resume` refuses: `status` and `stop` name its pid and the manual kill (`kill -TERM -- -<pid>`, after `ps -o pid,pgid,lstart,args -p <pid>` when the pid is unconfirmed). `devflow stop` cannot end it (backlog 999.170), so end it by hand first |

How each command shows them:

- **`devflow status`** prints `supervisor: <socket path> — <state>` for each
  phase, and under it the repair: `devflow stop --phase N` then
  `devflow resume --phase N` for STALE; for GONE while the phase is not
  stopped, `devflow resume --phase N`, plus `devflow ship --phase N` when the
  Ship gate is pending: when the Ship gate is pending, answer it and run
  `devflow ship --phase N`; any other pending gate is asked again by `resume`.
  When the supervisor is gone but the phase's agent still runs, it names the
  agent's pid and the manual kill (`kill -TERM -- -<pid>`, after
  `ps -o pid,pgid,lstart,args -p <pid>` when the pid is unconfirmed) instead of
  `resume`, which refuses while that agent lives; `devflow stop` cannot end it
  (backlog 999.170), so end it by hand first. The pid is unconfirmed when it
  comes only from `.devflow/phase-NN-agent-pid`, which an unrelated process can
  have reused; if `ps` shows that, remove the pid file; end that process only if it is yours to end. A
  phase with no supervisor recorded reads `not launched`.
- **`devflow doctor`** reports STALE, GONE-while-not-stopped and a failed probe
  as problems, naming the phase and launch id but no path. Its repairs:
  `devflow stop --phase N && devflow resume --phase N` for STALE,
  `devflow resume --phase N` for GONE (`devflow ship --phase N` when a Ship answer is stored,
  and in addition when the phase sits at a Ship gate with no stored answer),
  `devflow status` for a failed probe.
  ALIVE, a stopped GONE and an unlaunched phase are silent. `doctor --json`
  gives each phase a `supervisor` object (`liveness`, `owner`, `launch`,
  `launch_id`). It also warns about a held state write lock, naming its holder,
  names a live agent that has no supervisor with the manual kill, and lists
  set-aside gate answers.
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
- **`devflow recover --clean --phase N`** refuses, deleting nothing, while
  the phase's supervisor may still run, by the same rule as `start`: an ALIVE
  socket, a STALE one unless the backstop's checks prove its agent group
  gone, or a probe that fails, searching the socket directory when the state
  file is missing or does not load (and refusing when that directory exists
  but cannot be listed). It does not read the agent pid file. The refusal
  names a `devflow stop` line for each such socket. It then decides again on a
  fresh read under the state write lock, and that decision keeps any STALE
  socket, even one this guard lets through, printing `kept phase N` with
  `run \`devflow stop --phase N\` first` and exiting non-zero. So for a STALE
  socket, `stop` comes first: it removes the socket. When the state file is
  missing or does not load, it also keeps the phase while a launch liveness
  file (`supervisor-<id>.live`) is held that no other phase's state records,
  naming its holder pid: that may be the phase's supervisor before it publishes.
- **`devflow start` / `devflow resume`** refuse while a run is live: an ALIVE
  socket, a STALE one unless the backstop's checks prove its agent group gone,
  or a probe that fails. When the state file is missing or does not load,
  they search the socket directory for this project and phase instead, and
  refuse when that directory exists but cannot be listed. The refusal names a
  `devflow stop` line for each live socket. They also refuse while a live pid
  sits in `.devflow/phase-NN-agent-pid`, until that process is ended, and while
  another foreground process holds `.devflow/.holder-NN`. A `start` over a
  state file that is missing or does not load also refuses while a launch
  liveness file is held that no other phase's state records, naming its holder
  pid; a launch between its reservation and its record can show this for a
  moment, so retry.

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

**Superseded supervisors.** A supervisor the state no longer records, or whose
promotion never came within its 15 s wait, writes no state or exit file and cannot
record its own stop. When the state records another launch it also leaves
`workflow._auto_chain_active` to that launch; otherwise it clears the flag as usual.
`devflow logs --follow` ends on an exit file, or once the phase has no state file
(including one gone before it started); while the state stays, it keeps following
until Ctrl-C. It ends its agent's group if the agent still runs and logs
`supervisor_superseded`, whose `agent_group` field says `ended` or
`already_exited`. A promoted supervisor keeps reading the state once a second
until its agent exits, so editing the state to name another launch or none, or
deleting it for more than 3 s, ends a running agent. A state that does not load, or names the
launch only as pending, ends nothing: the supervisor logs a warning at most once
a minute, and its agent keeps running.

**`git clean -fdx` under `--no-worktree`.** It removes `.devflow`, with the state,
capture and history in it. If the file stays gone for 3 s the supervisor ends the
agent's group and logs `supervisor_superseded` with `recorded: null`. If the agent
exits first, the exit file has nowhere to go and it logs `supervisor_failed`. Both land
in a `.devflow/events.jsonl` recreated at that moment, which is then all that remains
of the run's log. A clean after that event removes it too, with nothing left to say why.

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
phase stays resumable. A foreground holder handles them as `devflow stop`'s
third arm describes.

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
`--gate-id <ID>` binds the answer to the gate `gate list` or `gate show`
displayed, and a gate opened since is refused; without it the command answers
the gate open now.

Every gate opened gets a fresh `gate_id`, named in its request file
`.devflow/gates/NN-<stage>.json`. An answer belongs to that one gate. `gate
approve` and `gate reject` write it to
`.devflow/gates/NN-{stage}.{gate_id}.response.json`. Before they publish, they
move an answer in that gate's own slot that does not bind to it aside, so a
refused answer never blocks a valid one, and they refuse to overwrite an
unconsumed answer that does bind. At a stage other than Ship with no registered
waiter they write nothing and name the repair; a Ship answer with no waiter is
stored for `devflow ship`.

To answer by hand, copy `gate_id` from the gate file and publish the same path
with the same body, renaming it into place so no reader sees it half written:

```json
{"gate_id": "<copied from the gate file>", "approved": true, "note": "lgtm"}
```

`approved` is `true` or `false`, and `note` is optional. A rejection loops back
to Code, or ends the phase when its note contains `abort`. `devflow gate list`
stops listing a gate once an answer bound to it is in its slot. A
process waiting at the gate reads its answer slot with a backoff (1s, 2s, … capped
at 60s, so a long wait costs few reads, and a stop is still seen within about a
second), claims the answer with `.devflow/gates/NN-{stage}.{gate_id}.ack.json`,
and moves on. The first process to publish that claim consumes the answer.

**Answers that do not bind.** An answer with a missing or other id, an
unparsable one, a legacy unkeyed `NN-<stage>.response.json`, or one left unused
when its gate closes is never consumed or deleted. It is set aside as
`NN-{stage}.{gate_id|noid}.{secs}.{pid}.{seq}.quarantined.json` and logged as
`gate_answer_quarantined` with its reason. `status`, `doctor` and
`recover --clean` list it. A set-aside file persists until removed by hand:
nothing in DevFlow deletes it, so remove it once read.

**A late answer.** `DEVFLOW_GATE_TIMEOUT_SECS` ends the wait, not the gate, and
a run that timed out has ended at that gate. `devflow resume --phase N` reruns
the stage and asks again. It reports any unused answer it set aside, and any
answer that "was claimed, with no record that it was applied" (logged
`resume_found_unapplied_answer`; the event's name means that record is missing,
and the closed gate's claim is logged `gate_claimed_answer_removed`). The
answer's consumer may have ended before acting on it, or it may have acted with
the gate left open, so the line says only what is known, and the rerun asks
again either way. At Ship the line adds that a merge, version bump or tag may
already have run, so check them before answering again. A stored Ship approval
is applied without a rerun by `devflow ship --phase N`; once `resume` has set
that answer aside, `devflow ship` cannot apply it.

## Rate-limit failover

A rate-limited stage gets one of two answers.

- **Failover.** The run has an unused fallback agent (`--fallback-agent` on
  `devflow start` or `devflow resume`, or `devflow.toml`'s `fallback_agent` key
  at `devflow start`) and the rate limit hit the Code stage. The supervisor
  hands the run to the fallback and relaunches Code in the same worktree, on the
  same branch, with the tree as it is: nothing is reset, stashed or cleaned. The
  hop happens once per run, because it marks the fallback used in
  `state-NN.json`; `devflow resume --phase N --fallback-agent <agent>` re-arms it
  on a run that is not live. A hop does not count toward the infra ceiling.
- **Cron resume.** Otherwise DevFlow writes `cron-instructions-NN.json` naming
  `devflow resume --phase N`: no fallback configured, the fallback already used,
  a rate limit at any stage but Code, or a handoff refused before it happened.
  The full preflight runs again at the handoff, because the check at launch is
  best effort, so a fallback can still be refused there (its binary gone, say).

Only top-level events decide a rate limit: a subagent's rate-limit event or
`result` in the capture does not. A stage that went silent with no quota
denial in its capture ends as an idle timeout, which takes neither answer. Its
failure names the last top-level `result` event's `terminal_reason`, `origin`
and `errors` when the capture carries them, as in
`idle timeout (last result: terminal_reason aborted_tools; origin task-notification; …)`.
When the capture ends in a torn final line, the clause starts with
`torn final line after the last parsed event`, and the verdict stays an idle
timeout. For a Claude capture the torn line may have been a quota denial; the
classifier reads only Claude's denials, so for other agents it says nothing
about one either way. A torn final line outranks a quota denial parsed before
it, because the torn line may be a later event that replaced the denial: the
stage is never classified as rate-limited, so no failover and no cron resume
follow. At an idle timeout it is recorded as one, flagged torn, and stops at a
gate, when the torn line reached the capture before the monitor's idle check.
Known residual (backlog 999.207): the capture keeps whole lines only, so a
partial line the agent was writing when it was killed reaches the capture only
at end of output, after that check. If the capture held a complete quota denial
at the check, the monitor logs the stage as rate-limited (resumable), writes no
idle-timeout record and kills the agent; the verdict then reads the torn line
and the gate shows an indeterminate failure (`stream capture ends in an
unparseable line`), not an idle timeout. The outcome is still a gate, with no
failover and no cron resume.

The failover events in `events.jsonl`:

| Event | When | Keys |
|---|---|---|
| `agent_handoff` | The run changed agent | `stage`, `from_agent`, `to_agent`, `reason` (`rate_limited` for a failover, `resume --agent` for `devflow resume --agent`), `tree_dirty` (the tree handed over had uncommitted changes), `commit_count` (commits on the phase branch). Each of the last two is `null` when it could not be measured |
| `agent_handoff_refused` | The fallback was refused before the handoff; the cron arm follows | `stage`, `from_agent`, `to_agent`, `reason` |
| `agent_handoff_relaunch_failed` | The handoff happened, but Code could not be relaunched under the fallback | `stage`, `from_agent`, `to_agent`, `reason` (the launch error) |
| `gate_relaunch_refused` | A retry or loop-back answer to a stage's failure gate was not relaunched, because the delivery canary recorded for the run refuses its agent; the run is marked stopped | `stage`, `agent`, `reason` (the recorded refusal) |

`agent_handoff_relaunch_failed` is followed by the never-silent Code gate
(`gate_fired`), or by an abort when the run is at the infra ceiling, never by
cron instructions. When the fallback's delivery canary refused, a scheduled
resume would only fail again: the refusal stays recorded for the run and
refuses every later launch. And after a shutdown that could not be confirmed, a
successor may still be running. The gate names the cause as
`the handoff to <agent> could not relaunch the code stage: <error>`. When the
recorded canary refuses the agent, the gate context (and so the notify hook)
also says that a retry or loop-back will not relaunch and names
`devflow resume --phase N --agent <other>`.

The way on when the fallback's canary refused:

- Answering the gate with a retry (`devflow gate approve`, or a rejection
  without `abort`) does not relaunch Code, because the recorded refusal would
  refuse it again. The supervisor marks the run stopped, with a stop reason
  naming the refusal and `devflow resume --phase N --agent <other>`, writes a
  `gate_relaunch_refused` event, and exits. The same applies to any stage's
  failure gate whose run has a refusing canary recorded for its agent.
- `devflow stop --phase N` answers the gate with an abort, which ends the run
  and removes its state.
- To continue on another agent, answer the gate with a retry, wait until
  `devflow status` shows the run not live (its hint then reads
  `devflow resume --phase N --agent <other>`), then run
  `devflow resume --phase N --agent <agent>` naming an agent other than the
  run's current one. The handoff clears the recorded canary, so that agent
  runs its own. A plain `devflow resume --phase N` is refused by the same
  recorded canary before it changes anything, so the stop reason that names
  the way on is kept.

## How a run ends

A run records how it ended with one `workflow_finished` event in
`.devflow/events.jsonl`, followed by one `notify_fired` event carrying the
same `reason`. The notify command runs once, between the two (see
[Notify hook](#notify-hook-never-miss-a-gate)).

| `workflow_finished` payload | When | State afterwards |
|---|---|---|
| `{"reason":"finished","shipped":false}` | Validate passed: on the pass in auto mode, on the approved Validate gate in supervise mode | kept, with `stopped: true`, `finished: true` and a stop reason naming the finish |
| `{"reason":"stopped_at","stage":"<stage>"}` | `--until plan` or `--until code` halted the run after that stage | kept, with `stopped: true`, `finished: false` |

Validate is the last stage, so `--until validate` ends a run whose Validate
passes as `finished`, exactly like a run without `--until`; it never reports
`stopped_at` for Validate.

Finishing never merges, pushes, tags, deletes or resets anything and runs no
hook: the worktree and the `feature/phase-NN` branch are left as Validate left
them. The phase is removed from the machine-global gate registry. `cleanup`
without `--force` keeps a finished run's worktree.

## Notify hook (never miss a gate)

Set `DEVFLOW_GATE_NOTIFY_CMD` to any shell command; it runs on every gate
with metadata in env vars — never interpolated into the command. It also
runs once when a run ends ([How a run ends](#how-a-run-ends)): for a
finish `DEVFLOW_GATE_STAGE` is `validate`, for an `--until` halt it is the
stage the run stopped after, and `DEVFLOW_GATE_CONTEXT` names the end.

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
is kept when the gate is answered. An edit made while a stage is still running
is kept too: the gate reloads the state file when it opens and fires the edited
command. The coding agent never receives the variable.

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `DEVFLOW_GATE_NOTIFY_CMD` | unset | Shell command fired when a gate is written, and once when a run ends |
| `DEVFLOW_GATE_TIMEOUT_SECS` | 259200 (3d) | How long a monitor waits at a gate, holding its gate registration rather than a lock. Not a deadline for answering: the gate request and phase state are files that never expire, and timing out is a clean resumable stop. Past it the run has ended at that gate: `devflow resume` reruns the stage and asks again, setting any unused answer aside (a stored Ship approval is applied without a rerun by `devflow ship --phase N`) |
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
| `RUST_LOG` | `info` | Log verbosity (stderr) |
| `DEVFLOW_LOG_FORMAT` | plain | `json` for machine-readable log lines |
| `DEVFLOW_E2E_CHILD_TIMEOUT_SECS` | 90 | Test-only: bounds `gate_sweep_e2e.rs`'s patience with a spawned `devflow advance` child so CI cannot hang indefinitely; not read by any production code path |

## `.devflow/` file inventory

| File | What it is |
|---|---|
| `state-NN.json` | A phase's persisted stage-machine state (per phase since 14a) |
| `.state-NN.lock` | The phase's short state write lock: held for one state write, usually milliseconds and never across a gate wait, and never removed |
| `.gates-NN.lock` | The per-phase gate lock: held for a few file operations, never removed |
| `.holder-NN` | D-17 foreground holder: held for its lifetime by a `start`/`resume` before its spawn, `devflow ship` or a hand-run `advance`, recording its pid and start time. `stop`'s third arm signals it; never removed by `recover --clean` |
| `supervisor-<launch_id>.live` | Launch reservation: its lock is taken before the pending record is saved and held by the spawned supervisor for life. An abandoned one is removed by `recover --clean` |
| `*.hold-tmp` | A hold being taken, here or under `gates/`. An orphan is removed by `recover --clean` |
| `lock-NN` | A leftover from an older binary. Nothing takes it now and it blocks nothing; `recover --clean` removes it, keeping it with a notice while the pid it names is alive |
| `lock-project` | Coarse checkout lock (held seconds, around shared-git mutations) |
| `phase-NN-stdout` / `phase-NN-stderr.log` | Agent capture files (what `logs` tails) |
| `phase-NN-exit` / `phase-NN-agent-pid` | Exit code + PID the supervisor records |
| `gates/NN-<stage>.json` | Gate request, naming its `gate_id` |
| `gates/NN-{stage}.{gate_id}.response.json` | The answer to that one gate |
| `gates/NN-{stage}.{gate_id}.ack.json` | The claim: the first process to publish it consumes the answer |
| `gates/NN-{stage}.{gate_id}.waiter` | The waiter's registration: its held lock says something waits on that gate. An abandoned one is removed by `recover --clean` |
| `gates/NN-{stage}.{gate_id\|noid}.{secs}.{pid}.{seq}.quarantined.json` | An answer set aside. It persists until removed by hand |
| `events.jsonl` | Append-only event log (schema v1, one JSON object per line, phase id on every line) — tail it from any tool |
| `cron-instructions-NN.json` | Rate-limit resume record naming `devflow resume --phase N` for a paused run. Written when no unused fallback applies: none configured, already used, the rate limit was not at Code, or the handoff was refused before it happened. Not written when the fallback's relaunch fails after the handoff: that run stops at a Code gate, or aborts at the infra ceiling. The cron arm itself writes no file when its infra count reaches the ceiling (the run aborts). When the retry time does not parse, it still writes the file, with an empty `hermes_cron.schedule` that schedules nothing, and the run stops at a gate: the file is a record, not a scheduled resume, so resume by hand |
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
`devflow recover --clean --phase N` to reset one phase, then re-`start`. Both
keep a phase that may still be live, and name `devflow stop --phase N` first.
