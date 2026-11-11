# DevFlow Operations Reference

The accurate, operator-facing reference for running DevFlow v2 day to day.
This page describes the CLI **as built** (verified against `main.rs` by the
`--help` snapshot test); the README/ARCHITECTURE rewrite lands in Phase 15b.

## The pipeline

Each phase runs a linear stage chain, driven end-to-end by detached monitors:

```
Plan → Code → Validate
```

- **Plan / Code** launch a headless coding agent with the stage's
  GSD command.
- **Validate** passes only when the agent reports an explicit
  `verdict: pass` **and** GSD's own reader, `gsd query verification status`,
  reports `passed` for the phase ([How Validate decides](#how-validate-decides)).
  Any other answer routes by GSD's status: gaps loop back to Code (after
  `MAX_CONSECUTIVE_FAILURES` = 3 failures a gate is forced), and a reader
  that cannot answer stops at a gate. Validate is the last stage that runs: a
  pass ends the run as finished and leaves the worktree and branch in place
  ([How a run ends](#how-a-run-ends)).
- Modes: `--mode auto` (finishes on a passing Validate without a gate;
  only never-silent failure gates and the failure caps stop it) or
  `--mode supervise` (also gates every Validate; approving it finishes the
  run).
- Every run gets an isolated git worktree under `.worktrees/phase-NN/` by
  default (`--no-worktree` opts out).

## Commands

| Command | Purpose |
|---|---|
| `devflow start --phase N --agent claude\|codex\|opencode --mode auto\|supervise [--force] [--no-worktree] [--dry-run] [--until plan\|code\|validate] [--fallback-agent <agent>]` | Begin a phase; a detached monitor owns the agent and auto-advances. Over an existing state it says first that it replaces it, naming which: a finished run, a stopped run (with its stop reason), or a state file that does not load (`--dry-run` shows this too). `--until <stage>` runs through `<stage>` then halts cleanly (no orphaned monitor, no `doctor` false positive) instead of continuing; `--until validate` halts nothing, prints a note saying so and records an `until_noop` event: the run ends as one without `--until` does ([How a run ends](#how-a-run-ends)). `--until define` and `--until ship` are rejected by clap as invalid values: there is no Define or Ship stage. Refuses before scaffolding anything if phase N is not reachable from `develop` (see Preflight refusals below). `--fallback-agent <agent>` names one agent that takes over if the Code stage is rate-limited: one hop per run, at Code only (a rate limit at any other stage waits at the stage's gate), relaunching Code in the same worktree on the same branch. It is saved in `state-NN.json`, and refused when it equals `--agent`, when its binary is missing or its driver unhealthy, and in `--mode auto` unless it is `claude` or `antigravity`. That launch check is best effort: the full preflight runs again at the handoff, a refusal there leaves the run waiting at the Code gate, and a relaunch that fails after the handoff stops at a Code gate (see [Rate-limit failover](#rate-limit-failover)). Without the flag, `start` uses `devflow.toml`'s `fallback_agent` key and prints a note naming it; the flag wins the value, and no environment variable selects a fallback |
| `devflow resume --phase N [--agent <agent>] [--fallback-agent <agent>]` | Resume a rate-limited or infra-paused phase from its saved stage (loads `state-NN.json`, does not recreate the branch/worktree or reset to Plan) — the command a rate-limit gate names for after the reset. Also clears a `--until`-stopped phase's stop marker, so the phase advances past its old stop point instead of re-stopping immediately. A run that finished (its Validate passed) is refused, naming `devflow start`, and nothing is launched. It reruns the stage, so a pending gate is asked again: it sets any unused answer aside and reports it, and reports an answer that was claimed with no record that it was applied (see [Answering gates](#answering-gates)). `--agent <agent>` hands the run to another agent (an `agent_handoff` event with reason `resume --agent`); naming the run's saved, unused fallback is refused unless `--fallback-agent` names another. `--fallback-agent <agent>` sets the fallback or re-arms a spent one, checked as on `start`; like any resume it is accepted only on a run that is not live. Without it the saved fallback is kept as it is, spent or not: `resume` does not re-read `devflow.toml` |
| `devflow status` | Every phase with a state: the header lists the active runs and names each other phase's kind (finished, stopped, worktree missing). Per phase: `run: active`, `run: stopped`, `run: finished`, or `run: not active` when its recorded worktree no longer exists (the worktree line is marked `(missing)`); a stopped or finished run's `stop reason:` verbatim; `fallback: armed` (configured, not used), `spent` (already handed off) or `none`; stage, gate state, supervisor liveness (ALIVE / STALE / GONE, with the repair; see [Run liveness](#run-liveness)), elapsed, last action. Also shows a held state write lock with its holder, names a live agent that has no supervisor with the manual kill, and lists set-aside gate answers |
| `devflow logs [-f] [--phase N] [--stderr]` | Print/follow a phase's captured agent output |
| `devflow history [N]` | Show chronological events with retained capture and review evidence for a phase |
| `devflow gate list` | Gates awaiting a response |
| `devflow gate approve <phase> [--stage S] [--note ...] [--gate-id ID]` | Approve a gate — the workflow advances |
| `devflow gate reject <phase> --note ... [--stage S] [--gate-id ID]` | Reject — the stage is fixed and relaunched; the run stays alive, whatever the note says |
| `devflow gate abort <phase> [--note ...] [--stage S] [--gate-id ID]` | Abort — the only answer that ends the run |
| `devflow gate sweep [--max-age-secs N] [--dry-run] [--root PATH]` | Answer or report aged, unattended gates across every registered root (or one `--root`) — bounds an abandoned run's lifetime without `kill(1)`. On-demand only; nothing schedules it. Never writes an approval |
| `devflow stop --phase N [PROJECT] [--root PATH] [--launch ID]` | End a running phase cleanly (23c). In this order: **(1)** an open gate with a registered live waiter is answered with an abort — the waiter unwinds through its own abort path, no signal sent. A gate whose answer a consumer already claimed is left alone. If `.devflow/.gates-NN.lock` stays held past 5 s, `stop` says so, answers nothing and goes on to the next arms, so a wedged `gate approve` cannot leave a run unstoppable. **(2)** Otherwise the run's supervisor socket is probed: the one `.devflow/state-NN.json` records, or, when that file records none (a `start` still waiting for its supervisor to publish), is missing or does not load, the one live socket found through its sidecar for this project and phase. **ALIVE** → a `shutdown` request: the supervisor ends the agent's process group (exit code 143) and runs no advance. A stop honoured at an advance safe point is named as that, since nothing was signalled, and the exit file then records 143 too. A supervisor that accepted the stop but whose final answer did not arrive in time is reported as accepted, not confirmed: it honours the stop at its next safe point, and `devflow status` shows when it has. **STALE** → the guarded backstop: it signals (SIGTERM, then SIGKILL) only an agent group whose leader is still present with the recorded start time and still leads the group; a socket from a previous boot, an empty group or a recycled leader pid has its socket and sidecar removed with nothing signalled; a group whose leader is gone is refused and its members listed, with `ps -o pid,pgid,sid,lstart,args -p "$(pgrep -d, -g <pgid>)"` to inspect them (residual limit: nothing proves those members are the agent's, so `stop` never signals them — end them yourself). A probe that cannot answer is refused, nothing signalled. A launch the state names as pending comes next, and only its socket reaches it: `stop` waits up to 15 s for an unpublished one and never signals it. If it never publishes, a foreground launcher still holding the phase is reached through arm (3), and with none `stop` reports the launch and the pid its liveness lock records, records nothing and names the manual route: end that process yourself, then run `stop` again to clear the reservation. A reservation nothing holds is abandoned: `stop` clears it and says so. **(3)** Only when no socket answers (**GONE**, or nothing recorded) is the phase's foreground holder signalled — a `start`/`resume` before its spawn or a hand-run `devflow advance`, which holds `.devflow/.holder-NN` for its lifetime — after confirming its pid and start time match the identity that file recorded (a recycled pid, or a record with no start time, is refused). No pid read from the state file is ever signalled. The holder catches the signal and ends at its next safe point (1–6; a foreground gate wait at 4), recording `stopped by devflow stop at a foreground safe point` and logging `advance_stopped` with `source: foreground_holder`. A holder still running 3 s later is reported as not marked, exit non-zero, and it records the stop itself. **An agent with no supervisor.** With no socket, pending launch or holder left but the phase's agent still alive, `stop` names the agent's pid and the manual kill (`kill -TERM -- -<pid>`, after `ps -o pid,pgid,lstart,args -p <pid>` when the pid is unconfirmed), writes nothing and exits non-zero. `devflow stop` cannot end that agent (backlog 999.170). The pid is unconfirmed when it comes only from `.devflow/phase-NN-agent-pid`: a pid reused by an unrelated process makes `stop` exit non-zero until that process ends, and the way out is to remove the pid file (end that process only if it is yours to end). **What it records.** Idempotent; records `stopped`/`stop_reason`. After arm (1) the waiter's abort path then clears the state. Either way `cleanup`'s fail-closed refusal recognizes the phase as no longer live — `stop` then `cleanup --force` compose in that order. `stop` does not mark a run `stopped` when, by the time it writes, the state names a launch none of its arms ended, answered, cleared or found not running: one a signalled holder published, a pending launch that promoted while `stop` ran, or one started meanwhile. Run `stop` again: a launch that turns out dead is found not running by that second `stop`, which then marks the run. `stop` waits at most 10 s for the state write lock, then names its holder and writes nothing. When the state file does not load, `stop` still ends the run, says `stopped` was not recorded and leaves the file untouched; several live sockets with no usable state are refused, with one `devflow stop --phase N --launch <id>` line each. `--launch <id>` tears down only the socket with that launch id — the form `devflow doctor` prints for an orphan, usable from any directory — and answers no gate, signals no holder and writes no state. `stop` adds no exit codes |
| `devflow cleanup [--force]` | Remove phase worktrees + their feature branches, then delete branches merged into the base. **Keeps a worktree, and says why:** a stopped or finished run (the line quotes the run's recorded stop reason; a stopped run is offered `devflow resume --phase N` or `--force`, a finished run only `--force`, since `resume` refuses it), a STALE supervisor or a failed probe (`--force` removes it). A live run (a running agent, or an ALIVE or pending supervisor) is refused with a non-zero exit even with `--force`, naming that a run is live and `devflow stop --phase N`. `--force` also removes the run's `.devflow/state-NN.json` once its worktree is gone, so nothing of the run is left; another phase's state is untouched. A merged branch that a kept worktree (or any checkout) still has checked out is never deleted |
| `devflow recover [--clean] [--phase N]` | Inspect state; `--clean` sweeps stale phases only; `--clean --phase N` clears one phase. Either form keeps a phase whose recorded or pending supervisor, gate waiter or foreground holder may be live, deciding on a fresh read under the state write lock, so a run that starts while it works is kept. It waits at most 10 s for that lock, then keeps the phase and names the holder. The `--clean` sweep also removes abandoned `supervisor-*.live` and `*.waiter` markers and orphan `*.hold-tmp` temps, and lists set-aside gate answers as kept. Neither ever removes `.holder-NN`, `.state-NN.lock`, `.gates-NN.lock` or a set-aside answer |
| `devflow doctor [--json]` | Environment audit (agents installed, versions, RUST_LOG) plus per-phase reconciliation, including each phase's supervisor liveness and a read-only sweep for orphan supervisor sockets (see [Run liveness](#run-liveness)); `--json` emits one object with the keys `environment`, `reconciliation`, `planning_doc_staleness`, `stray_processes`, `orphan_supervisors` and `orphan_supervisor_sweep_error` |

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
| **GONE** | No file at the recorded path: the supervisor has exited. Normal when the phase is stopped; when it is not, nothing will advance the phase | `devflow resume --phase N`, which reruns the stage and asks again; a pending gate is asked again by `resume`. When the agent is still running with no supervisor, `resume` refuses: `status` and `stop` name its pid and the manual kill (`kill -TERM -- -<pid>`, after `ps -o pid,pgid,lstart,args -p <pid>` when the pid is unconfirmed). `devflow stop` cannot end it (backlog 999.170), so end it by hand first |

How each command shows them:

- **`devflow status`** prints `supervisor: <socket path> — <state>` for each
  phase, and under it the repair: `devflow stop --phase N` then
  `devflow resume --phase N` for STALE; for GONE while the phase is not
  stopped, `devflow resume --phase N`; a pending gate is asked again by `resume`.
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
  `devflow resume --phase N` for GONE,
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
| Codex headless Plan | `--agent codex` and phase N has no `-CONTEXT.md` on `develop` — Codex's `exec` mode cannot run the discuss-phase interview headlessly, so its Plan stage needs CONTEXT.md to exist | Run `/gsd-discuss-phase N` interactively first (any agent), or use `--agent claude` |

## Answering gates

When a gate fires you'll get the notify hook (below) and `devflow status`
shows `gate: pending`. Answer from any terminal:

```bash
devflow gate list
devflow gate approve 15 --note "lgtm"
devflow gate reject 15 --note "tests are thin, tighten coverage"   # fix and relaunch
devflow gate abort 15 --note "wrong direction"                     # ends the run
```

There are three answers. `gate approve` advances. `gate reject` always means
fix and relaunch: the run stays alive. `gate abort` is the only answer that
ends the run. A note's wording never changes which one it is, so a rejection
that reads "do not abort, fix the test" relaunches the stage. An answer that
both approves and aborts is refused when it is written.

### What each answer runs

Every gate's context opens with one sentence that names what each answer runs
on that gate, or that it is not offered, and that abort ends the run. The cause
follows it, so the notify hook, the pending-gate banner and `gate list` show
the way on first. `gate approve`, `gate reject` and `gate abort` print the same
route (a `route:` line) once the answer is written. The sentence and the print
come from the routers the supervisor dispatches through, read over the gate
file's kind and the run's recorded delivery canary, never from the gate's
text. An answer the supervisor would refuse is refused by the command before
anything is written: it exits non-zero and names the answers that apply.

**Verification gates** are the gates opened over a GSD status or a failed GSD
read: `clean_pass`, `gaps_found`, `reverify_needed`, `human_needed`,
`disagreement_over_passed`, `disagreement_over_human_needed`,
`ambiguous_verdict`, `failure_cap`, `reader_failure` and
`no_gap_closure_plan`. Approve reads GSD's verification status again when the
supervisor picks the answer up, and routes on that fresh read, never on the
status the gate was opened over:

| Fresh GSD status | Validate pass on the current commit, over a clean tree | Auto mode, after Validate | Approve |
|---|---|---|---|
| `passed` | yes | finish | finish |
| `passed` | no | gate | run Validate again |
| `gaps_found` | any | the gap-closure loop | the gap-closure loop |
| `stale` or `missing` | any | the re-verify Code pass | the re-verify Code pass |
| `human_needed` | any | gate | read again; the gate reopens with the fresh facts |
| no answer (a reader failure) | any | gate | read again; the gate reopens with the fresh facts |

In supervise mode every Validate ends at a gate. An approve follows the
Approve column whether the run is in auto or supervise mode, a `failure_cap`
approve included: that one restarts the count first. `gate approve` prints the
route the table gives over a read it makes itself, and says that the
supervisor reads again at pickup; a status that changes between the two reads
can route differently from the print.

A finish needs a fresh `passed` and a Validate pass recorded on the current
commit. A commit made during a gate wait sends approve back to Validate, and so
does an uncommitted change: a pass counts only on a clean tree, `.planning/`
included, and the gate names the dirty paths. A Validate that ran over a dirty
tree records no pass, so cleaning the tree afterwards does not revive one: the
finish needs another Validate over the clean tree. Approving an
`ambiguous_verdict` gate no longer advances the run: an ambiguous verdict
records no Validate pass, so approve over `passed` runs Validate again, and a
Validate that stays ambiguous can finish only through a later clean pass.

GSD's own `/gsd-plan-phase`, run without `--auto` as DevFlow's Plan stage runs
it, writes `workflow._auto_chain_active` into `.planning/config.json` and does
not commit it. So in a project whose committed config lacks that key, the
first Validate gate names `.planning/config.json` as a dirty path; commit the
file once, and later Validates read a clean tree (GSD's later writes of the
same value leave it unchanged). A `devflow resume` runs DevFlow's existing
chain-flag repair before it relaunches, and when the flag is the file's only
difference from the branch's last commit the repair commits the file itself,
with a commit message and notice that say a killed run leaked the flag, which
is not so here (backlog 999.219, not fixed in Phase 52). `devflow start
--no-worktree` on a checkout that still holds that uncommitted write (from an
earlier run or a hand-run `/gsd-plan-phase`) commits it the same way when git
carries the write across start's branch checkout and the flag is the file's
only difference from develop; otherwise the checkout refuses and start fails
first, or the repair declines with a warning and the file stays dirty. That run
launches in the project checkout, and start makes no dirty-tree check before
the repair. `devflow start` runs the repair before its own Plan writes the key,
so a worktree run that is not resumed, or a `--no-worktree` run whose checkout
held no such write at start, needs the one manual commit. DevFlow makes no
other write for it and exempts no path.

Reject with a note runs `/gsd-quick` with the note, then Validate, on every
verification gate. A reject whose note is empty or only whitespace is refused.
Abort ends the run.

Accepted limits, fixed upstream: a `gaps_found` report gone stale runs the
gap-closure loop, whose execute-phase re-verifies (open-gsd/gsd-core#6078); and
a quick fix to a file the report does not cover can leave GSD's `passed`
unchanged (open-gsd/gsd-core#6096), so the finish still needs a fresh Validate
pass on that commit.

**Status-less gates** read no GSD status, and none of them can finish a run:

| Gate kind | Approve | Reject with a note | Abort |
|---|---|---|---|
| `stage_failure` | retries the stage | relaunches the stage with the note | ends the run |
| `rate_limit` | relaunches the stage now on the same agent | relaunches the stage with the note | ends the run |
| `stage_failure` on a run whose recorded delivery canary refuses its agent | stops the run; the way on is `devflow resume --phase N --agent <other>` | the same, and the note is kept for that resume | ends the run |
| `preflight` | relaunches the stage with the failed check overridden | not offered: refused | ends the run |
| `checkpoint_declaration` | records the checkpoints as seen and relaunches Code | not offered: refused | ends the run |
| a gate file with no `gate_kind` (a 3.x or hand-written file) | refused | refused | ends the run |

`--stage` is only needed when one phase somehow has several open gates.
`--gate-id <ID>` binds the answer to the gate `gate list` or `gate show`
displayed, and a gate opened since is refused; without it the command answers
the gate open now.

Every gate opened gets a fresh `gate_id`, named in its request file
`.devflow/gates/NN-<stage>.json`. An answer belongs to that one gate. `gate
approve`, `gate reject` and `gate abort` write it to
`.devflow/gates/NN-{stage}.{gate_id}.response.json`. Before they publish, they
move an answer in that gate's own slot that does not bind to it aside, so a
refused answer never blocks a valid one, and they refuse to overwrite an
unconsumed answer that does bind. With no registered waiter they write nothing
and name the repair.

To answer by hand, copy `gate_id` from the gate file and publish the same path
with the same body, renaming it into place so no reader sees it half written:

```json
{"gate_id": "<copied from the gate file>", "approved": true, "note": "lgtm"}
```

`approved` is `true` or `false`, and `note` is optional. Add `"abort": true`
(with `"approved": false`) to end the run; without the field the answer is not
an abort. Otherwise a rejection loops back to Code. A body with both
`"approved": true` and `"abort": true` is read as an abort, never as an
approval. `devflow gate list`
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
again either way.

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
- **Rate-limit gate.** Otherwise the run waits at the stage's never-silent
  gate: no fallback configured, the fallback already used, a rate limit at any
  stage but Code, or a handoff refused before it happened. The full preflight
  runs again at the handoff, because the check at launch is best effort, so a
  fallback can still be refused there (its binary gone, say). The gate shows in
  `devflow gate list` and `devflow gate show`, and fires the notify command.
  It is its own kind of gate (`gate_kind: rate_limit`). Its context leads with
  the routers' way-on sentence, then `devflow resume --phase N` (with
  `--agent <agent>` when the run is not on `claude`), then the reset time when
  the denial carried one (`the limit resets at <time>`, otherwise `the denial
  carried no reset time`), then the cause. `devflow gate approve N` relaunches
  the stage now, on the same agent (`relaunch_now`); `devflow gate reject N
  --note <text>` relaunches it with the note in the stage's prompt
  (`relaunch_with_note`; an empty note is refused); `devflow gate abort N` ends
  the run. The run's recorded delivery canary does not change these answers.
  When the denial named a reset time, the gate file records `not_before` (the
  reset plus 60 seconds), and `devflow gate sweep` leaves the gate alone until
  then, however old it is. Waiting for quota does not count toward the infra
  ceiling, so it cannot abort the run.

  **The one automatic relaunch.** When the denial named a reset time, the
  waiting supervisor relaunches the stage by itself once `not_before` has
  passed with the gate still unanswered: it answers the gate as
  `devflow-rate-limit-timer` (the answer's `responded_by`), and the stage
  relaunches on the same agent, as an approve would. The wake is the reset plus
  60 seconds whatever the gate timeout. The gate keeps waiting past
  `DEVFLOW_GATE_TIMEOUT_SECS` when the reset is later, and the relaunch still
  happens at the wake. This happens at most once per run: the supervisor
  records `rate_limit_auto_relaunch_used: true` in `state-NN.json` before it
  writes the answer. If an operator answered first, the supervisor takes that
  answer and the flag goes back to false. A gate's answer is claimed once, so
  the timer and an operator answering the same gate launch the stage once.

  - A second denial after the automatic relaunch opens the same kind of gate.
    The gate records its `not_before`, but no timer is armed, and it waits for
    an operator.
  - A denial with no reset time is an ordinary blocking gate. Nothing is
    guessed and nothing relaunches by itself.
  - An operator's answer (`gate approve` relaunches now, `gate reject --note`
    relaunches with the note in the stage's prompt, `gate abort` ends the run)
    does not use up the run's one automatic relaunch. A later denial with a
    reset time still gets it.
  - A resume, a stop and resume, or a supervisor takeover after the automatic
    relaunch was used does not arm it again.
  - A crash after the supervisor recorded the flag leaves no automatic
    relaunch: nothing re-arms the timer or writes its answer again. Run the
    `devflow resume --phase N` command the gate names. `devflow resume` is
    refused while the waiting supervisor is alive, so it is the way on once
    that supervisor has ended. It closes the phase's gates and relaunches the
    stage itself.

  The timer's write is recorded as a `rate_limit_timer_fired` event (below).

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
stage is never classified as rate-limited, so no failover and no rate-limit
gate follow. At an idle timeout it is recorded as one, flagged torn, and stops at a
gate, when the torn line reached the capture before the monitor's idle check.
Known residual (backlog 999.207): the capture keeps whole lines only, so a
partial line the agent was writing when it was killed reaches the capture only
at end of output, after that check. If the capture held a complete quota denial
at the check, the monitor logs the stage as rate-limited (resumable), writes no
idle-timeout record and kills the agent; the verdict then reads the torn line
and the gate shows an indeterminate failure (`stream capture ends in an
unparseable line`), not an idle timeout. The outcome is still a gate, with no
failover and no rate-limit gate.

The failover events in `events.jsonl`:

| Event | When | Keys |
|---|---|---|
| `agent_handoff` | The run changed agent | `stage`, `from_agent`, `to_agent`, `reason` (`rate_limited` for a failover, `resume --agent` for `devflow resume --agent`), `tree_dirty` (the tree handed over had uncommitted changes), `commit_count` (commits on the phase branch). Each of the last two is `null` when it could not be measured |
| `agent_handoff_refused` | The fallback was refused before the handoff; the rate-limit gate follows | `stage`, `from_agent`, `to_agent`, `reason` |
| `rate_limit_timer_fired` | The rate-limit gate's reset plus margin passed with the gate unanswered, and the supervisor's timer tried its one answer | `stage`, `gate_id`, `outcome` (`answered`; `operator_answered_first`, when an operator's answer was taken instead and the automatic relaunch stays unused; `failed`, when the write failed, the automatic relaunch is spent and the gate waits for an operator) |
| `agent_handoff_relaunch_failed` | The handoff happened, but Code could not be relaunched under the fallback | `stage`, `from_agent`, `to_agent`, `reason` (the launch error) |
| `gate_relaunch_refused` | A retry or loop-back answer to a stage's failure gate was not relaunched, because the delivery canary recorded for the run refuses its agent; the run is marked stopped | `stage`, `agent`, `reason` (the recorded refusal) |

`agent_handoff_relaunch_failed` is followed by the never-silent Code gate
(`gate_fired`), or by an abort when the run is at the infra ceiling. When the
fallback's delivery canary refused, a resume would only fail again: the
refusal stays recorded for the run and refuses every later launch. And after a shutdown that could not be confirmed, a
successor may still be running. The gate names the cause as
`the handoff to <agent> could not relaunch the code stage: <error>`. When the
recorded canary refuses the agent, the gate context (and so the notify hook)
leads with the routers' sentence: approve and reject both stop the run, and the
way on is `devflow resume --phase N --agent <other>`. The cause follows it.

The way on when the fallback's canary refused:

- Answering the gate with a retry (`devflow gate approve` or `devflow gate
  reject`) does not relaunch Code, because the recorded refusal would
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

## How Validate decides

A Validate verdict has two halves: the validate agent's own `verdict`, and
the status GSD's reader reports for the phase. DevFlow never parses
`VERIFICATION.md` itself. After every Validate it runs
`gsd query verification status <phase dir>` through GSD's `gsd_run`
launcher, found under the run's `HOME` (`.claude/gsd-core/bin/`, then
`.codex/gsd-core/bin/`, then `.pi/agent/gsd-core/bin/`). The read runs in the
phase's worktree, where the Validate agent wrote its report (the main checkout
when the run uses `--no-worktree`), and is killed after 60 seconds.

| GSD status | After an agent pass | After an agent fail |
|---|---|---|
| `passed` | auto: the run finishes; supervise: the Validate gate, whose approval finishes the run | a gate: the agent and GSD disagree; approving runs Validate again |
| `gaps_found` | auto: a gap-closure pass: Plan runs `/gsd-plan-phase N --gaps`, then Code runs `/gsd-execute-phase N --gaps-only`, then Validate runs again; supervise: a `gaps_found` gate, whose approval runs that gap-closure pass and never finishes the run, and whose abort (`devflow gate abort`) ends the run | the same |
| `stale` or `missing` | auto: back to Code for the re-verify pass (below); supervise: a `reverify_needed` gate, and nothing launches until it is answered | the same |
| `human_needed` | a gate in both modes; approving reads GSD again and reopens the gate while it still says `human_needed` | a gate: the agent and GSD disagree |
| no answer | a `[never-silent]` gate naming the failure: no launcher, a non-zero exit, a timeout, output that is not JSON, a status outside GSD's set, no phase directory, and the like. It is never a pass and never a silent retry | the same |

A gap-closure pass keeps going until a Validate reads `passed`. A `devflow
stop`, a resume or a crash in the middle of it keeps it: a resumed Plan still
runs `--gaps` and a resumed Code still runs `--gaps-only`. After its Plan
pass DevFlow checks that the pass wrote a plan with `gap_closure: true` (read
through GSD's `frontmatter get`) whose file is newer than the phase's
`VERIFICATION.md`. An earlier round's gap plan does not count. Without one the
run stops at a Plan gate of kind `no_gap_closure_plan`; if the check cannot
read the plans it stops at a `[never-silent]` `reader_failure` gate. Approving
either gate reruns the gaps Plan pass, and `devflow gate abort` ends the run.

The re-verify pass. A `stale` or `missing` status says the phase's report is
out of date or was never written, not that the code has gaps. DevFlow
relaunches Code with `/gsd-execute-phase N`, telling the agent to resume at the
phase gates and re-run the verifier, and the prompt quotes GSD's status, route
and next action as GSD gave them. DevFlow does not read GSD's status when that
pass ends: Validate runs after it as after any Code pass, and that Validate's
own read decides. A report that is still `stale` takes the re-verify pass
again, and the run finishes only over `passed`. A `devflow stop` keeps the
pass: a stopped re-verify pass resumes as the re-verify pass, and one taken in
the middle of a gap-closure pass ends that pass, so it resumes as the re-verify
pass and not as `--gaps-only`.

The re-verify prompt cannot make an agent run the verifier (gsd-core#6096). What
bounds an agent that skips it is the count: every Validate whose status is
`stale`, `missing` or `gaps_found` counts one Validate failure, whatever the
agent said and in either mode, and a Validate counts at most once even when the
agent also failed. Three such rounds whose commits change nothing outside
`.planning/` reach the consecutive-failure gate instead of looping.

DevFlow does not check a `gaps_found` report's own digest. A `gaps_found`
report whose covered files changed after it was written still runs the
gap-closure pass, whose execute-phase re-verifies at its end. The upstream fix
is open-gsd/gsd-core#6078. Re-running `/gsd-execute-phase N` by hand refreshes
the report.

An agent pass over any status but `passed` counts as a Validate failure, so the
consecutive-failure and per-phase caps still bound the loop; at a cap the run
stops at a gate whatever the status says. A gaps Plan pass that wrote no
gap-closure plan counts one failure the same way. When that brings the phase
to its per-phase ceiling, its `no_gap_closure_plan` gate says so, and an answer
that continues the run restarts the per-phase count (a
`phase_failure_budget_reset` event); `devflow gate abort` does not. An ambiguous verdict (the external
verification probe succeeded but the agent reported gaps or no verdict)
records no Validate pass: over `passed`, `human_needed` or a failed read it
stops at a gate, and in auto mode over `gaps_found`, `stale` or `missing` it
takes the table's route (see [What each answer runs](#what-each-answer-runs)). A gate context quotes GSD's status as GSD printed it
and says `passed` only when GSD reported `passed`, apart from the way-on
sentence that leads every verification gate, whose finish rule names `passed`.

What a run records:

- Each Validate read is one `gsd_verification` event: the agent's verdict and
  GSD's status, or the failure's short label. GSD's message can name absolute
  paths, so it goes only to the gate context and stderr, never to an event.
- Every gate file records which gate it is in `gate_kind` (for example
  `clean_pass`, `gaps_found`, `reverify_needed`, `reader_failure`,
  `failure_cap`), and the `gate_fired` event carries the same value. Gates
  outside Validate's routes record one too: `stage_failure` for a failed stage
  at any stage, `rate_limit` for a rate limit no fallback takes, `preflight`
  for a failed preflight check and `checkpoint_declaration` for an unapproved
  human-only checkpoint.
  A gate file written by a 3.x binary has no `gate_kind` and still loads.
- An agent pass is recorded in the state file's `validate_pass_head`, the
  commit the Validate ran on; any other verdict clears it. A commit that cannot
  be read stops at the `[never-silent]` gate.
- `devflow start` records the GSD version that answers the run's reads in one
  `gsd_runtime` event.

## How a run ends

A run records how it ended with one `workflow_finished` event in
`.devflow/events.jsonl`, followed by one `notify_fired` event carrying the
same `reason`. The notify command runs once, between the two (see
[Notify hook](#notify-hook-never-miss-a-gate)).

| `workflow_finished` payload | When | State afterwards |
|---|---|---|
| `{"reason":"finished","shipped":false}` | Validate passed: on the pass in auto mode, on the approved Validate gate in supervise mode | kept, with `stopped: true`, `finished: true` and a stop reason naming the finish |
| `{"reason":"stopped_at","stage":"<stage>"}` | `--until plan` or `--until code` halted the run after that stage | kept, with `stopped: true`, `finished: false` |

Validate is the last stage, so `--until validate` halts nothing: a run whose
Validate passes ends as `finished`, exactly like a run without `--until`, and
a `gaps_found` verdict runs the gap-closure loop (see
[How Validate decides](#how-validate-decides)) as it would without `--until`.
It never reports `stopped_at` for Validate. `devflow start --until validate`
says so in one printed note and records one `until_noop` event with
`{"until":"validate"}`; `--until plan` and `--until code` do neither.

A `devflow stop` taken while the run advances toward its finish (for example
just after the Validate gate was approved) loses to a finish that is
permitted: when GSD's fresh status is `passed` and the Validate pass is on the
current commit over a clean tree, the run is recorded finished, one
`workflow_finished` is recorded, and `stop` prints that the run already
finished and that nothing was stopped (the supervisor answers
`already_finished`, never `handed_off`). A stop that lands where no finish is
permitted (the approve runs Validate again, the gap loop or the re-verify
pass) is honoured at the next advance safe point as before, and the run ends
stopped and unfinished.

Finishing never merges, pushes, tags, deletes or resets anything and runs no
hook: the worktree and the `feature/phase-NN` branch are left as Validate left
them. The phase is removed from the machine-global gate registry. `cleanup`
without `--force` keeps a finished run's worktree and names its stop reason, `cleanup --force` removes the worktree and the run's state, and `devflow resume`
refuses a finished run and launches nothing: start a new run with
`devflow start`.

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

`DEVFLOW_GATE_CONTEXT` holds the first 450 characters of the gate's context,
the same start the pending-gate banner and `devflow gate list` show; a longer
context ends in `… [truncated; full output in .devflow/]`. Every context leads
with the way-on sentence, so the start names each answer's route. The gate
file stores a failure's reason whole (up to 4096 characters), and `devflow
gate show <phase>` prints all of it.

The command is saved in the run's state file (`gate_notify_cmd` in
`.devflow/state-NN.json`), so gates fired by the detached monitor use it
without your shell's environment. `devflow start`, `resume` and `advance`
update the saved command from the variable: unset keeps it, a new value
replaces it, and an empty value (`DEVFLOW_GATE_NOTIFY_CMD= devflow resume …`)
turns notification off for the run.

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
| `DEVFLOW_GATE_TIMEOUT_SECS` | 259200 (3d) | How long a monitor waits at a gate, holding its gate registration rather than a lock. Not a deadline for answering: the gate request and phase state are files that never expire, and timing out is a clean resumable stop. Past it the run has ended at that gate: `devflow resume` reruns the stage and asks again, setting any unused answer aside |
| `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` | 259200 (3d) | Age threshold `devflow gate sweep` uses to decide a gate is abandoned. Held **equal to** `DEVFLOW_GATE_TIMEOUT_SECS`, never shorter: `gate sweep` writes an abort response, so a threshold below the poll timeout lets a sweep reap gates a live monitor is still polling — turning a clean resumable timeout into an `abort()` that clears state, machine-wide across every registered root. An unparsable value or explicit `0` falls back to the default rather than reaping every gate on the machine |
| `DEVFLOW_CACHE_DIR` | unset (falls back to `$XDG_CACHE_HOME/devflow`, then `$HOME/.cache/devflow`) | Test/override hook for the machine-global registry directory (`devflow gate list --all-roots`) |
| `DEVFLOW_CAPTURE_RETENTION` | 5 | Capture generations retained per phase; overrides `devflow.toml` |
| `DEVFLOW_EXTERNAL_VERIFY_ENABLED` | true | Enable PLAN-declared external post-condition probes; overrides `devflow.toml` |
| `DEVFLOW_BASE_BRANCH` | `develop` | The branch phase worktrees fork **from** and phase work merges back **into** (D-01, `45-CONTEXT.md`); overrides `devflow.toml`'s `base_branch` key. Set it when your `.planning/` artifacts live on a branch other than `develop` — a worktree forked from `develop` does not carry a `.planning/config.json` that exists only on a planning branch, and the unattended preflight then refuses every launch (999.110/AUTO-01). **An explicitly supplied value that is `main`, blank, or begins with `-` is a hard error, not a silent fallback to `develop`**, and the value must name an existing local branch — a remote-tracking name, a `refs/heads/` path, `HEAD` or a commit SHA is refused. A resolved base other than `develop` is announced at `devflow start`, naming the source |
| `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS` | 120 | The per-agent idle timeout for the pipe-owning Antigravity monitor (phase 41, D-08): how long the monitor waits with NO output on the child's `stream-json` stream before recording an `idle_timeout` verdict and terminating it. Same 120s floor and clamp semantics as `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS`. The default is a DECIDED starting point, not a silent inheritance of the Claude-measured floor — it is to be revisited after the first real Antigravity cadence measurement. Claude's variable is never read for Antigravity and vice versa |
| `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` | 120 | How long the pipe-owning Claude monitor waits with NO output on the child's stream before recording an `idle_timeout` verdict and terminating it. Clamped to a 120s floor: a lower value is raised and the clamp is printed loudly at `devflow start`. The floor is 4x the CLI's measured `tool_progress` keepalive interval (a fixed 30.00s, measured 2026-08-03 over five workload-controlled trials on CLI 2.1.220), so it survives three consecutive missed keepalives; the earlier 30s floor sat exactly ON the keepalive interval and would have killed healthy stages running any tool call longer than ~30s, including `cargo test --workspace`. Every stream line resets the window, so a healthy long-running stage is never bounded by wall clock |
| `RUST_LOG` | `off` | Tracing verbosity (stderr). Unset or empty prints no tracing lines; set a level such as `info` or `debug` to see them |
| `DEVFLOW_LOG_FORMAT` | plain | `json` for machine-readable log lines |
| `DEVFLOW_E2E_CHILD_TIMEOUT_SECS` | 90 | Test-only: bounds `gate_sweep_e2e.rs`'s patience with a spawned `devflow advance` child so CI cannot hang indefinitely; not read by any production code path |

## `.devflow/` file inventory

| File | What it is |
|---|---|
| `state-NN.json` | A phase's persisted stage-machine state (per phase since 14a) |
| `.state-NN.lock` | The phase's short state write lock: held for one state write, usually milliseconds and never across a gate wait, and never removed |
| `.gates-NN.lock` | The per-phase gate lock: held for a few file operations, never removed |
| `.holder-NN` | D-17 foreground holder: held for its lifetime by a `start`/`resume` before its spawn or a hand-run `advance`, recording its pid and start time. `stop`'s third arm signals it; never removed by `recover --clean` |
| `supervisor-<launch_id>.live` | Launch reservation: its lock is taken before the pending record is saved and held by the spawned supervisor for life. An abandoned one is removed by `recover --clean` |
| `*.hold-tmp` | A hold being taken, here or under `gates/`. An orphan is removed by `recover --clean` |
| `phase-NN-stdout` / `phase-NN-stderr.log` | Agent capture files (what `logs` tails) |
| `phase-NN-exit` / `phase-NN-agent-pid` | Exit code + PID the supervisor records |
| `gates/NN-<stage>.json` | Gate request, naming its `gate_id` |
| `gates/NN-{stage}.{gate_id}.response.json` | The answer to that one gate |
| `gates/NN-{stage}.{gate_id}.ack.json` | The claim: the first process to publish it consumes the answer |
| `gates/NN-{stage}.{gate_id}.waiter` | The waiter's registration: its held lock says something waits on that gate. An abandoned one is removed by `recover --clean` |
| `gates/NN-{stage}.{gate_id\|noid}.{secs}.{pid}.{seq}.quarantined.json` | An answer set aside. It persists until removed by hand |
| `events.jsonl` | Append-only event log (schema v1, one JSON object per line, phase id on every line) — tail it from any tool |
| `history/phase-NN/` | Bounded archive of prior stage stdout/exit captures |

Everything under `.devflow/` and `.worktrees/` is runtime state
(git-ignored); `devflow recover --clean` is the sanctioned reset.

## A typical dogfood session

```bash
devflow start --phase 15 --agent claude --mode supervise .
devflow status                       # any time, from any terminal
devflow logs -f --phase 15           # watch the agent work
# ...notify hook pings you at the Validate gate...
devflow gate list
devflow gate approve 15 --note "reviewed the validation"
devflow status                       # finished — worktree and branch kept
```

When something wedges: `devflow recover` to inspect,
`devflow recover --clean` (stale phases, plus gate files whose phase has no
state; exits non-zero if anything could not be removed) or
`devflow recover --clean --phase N` to reset one phase, then re-`start`. Both
keep a phase that may still be live, and name `devflow stop --phase N` first.
