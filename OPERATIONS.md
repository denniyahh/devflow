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
| `devflow start --phase N --agent claude\|codex\|opencode --mode auto\|supervise [--force] [--no-worktree] [--dry-run]` | Begin a phase; a detached monitor owns the agent and auto-advances |
| `devflow status` | All active phases: stage, gate state, agent liveness, elapsed, last action |
| `devflow logs [-f] [--phase N] [--stderr]` | Print/follow a phase's captured agent output |
| `devflow gate list` | Gates awaiting a response |
| `devflow gate approve <phase> [--stage S] [--note ...]` | Approve a gate — the workflow advances |
| `devflow gate reject <phase> --note ... [--stage S]` | Reject — loops back to Code; a note containing `abort` ends the phase |
| `devflow parallel --phases 7,8 [--agents claude,codex] [--mode M] [--force]` | Run phases concurrently, each in its own worktree + monitor |
| `devflow sequentagent --phase N --agents a,b [--force]` | Two agents sequentially on one phase with a rebase handoff |
| `devflow list` | Feature branches with divergence from develop |
| `devflow reference [--branch B] [--refresh]` | Static snapshot worktree at `.worktrees/reference/` |
| `devflow cleanup [--force]` | Remove phase worktrees + their feature branches |
| `devflow recover [--clean] [--phase N]` | Inspect state; `--clean` sweeps stale phases only; `--clean --phase N` clears one phase unconditionally |
| `devflow test` | cargo test + clippy + fmt --check |
| `devflow doctor [--json]` | Environment audit (agents installed, versions, RUST_LOG) |

(`devflow advance` is internal — invoked by monitors with `--phase N`.)

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
atomically; the blocked monitor polls it (exponential backoff, so pickup can
take up to ~60s), acks, and moves on. The CLI refuses to overwrite an
unconsumed response.

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

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `DEVFLOW_GATE_NOTIFY_CMD` | unset | Shell command fired when a gate is written |
| `DEVFLOW_GATE_TIMEOUT_SECS` | 604800 (7d) | How long a monitor waits at a gate before giving up |
| `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` | 120 | Wait on the shared-checkout lock; on timeout the hook batch is skipped (loudly), never run unserialized |
| `RUST_LOG` | `info` | Log verbosity (stderr) |
| `DEVFLOW_LOG_FORMAT` | plain | `json` for machine-readable log lines |

## `.devflow/` file inventory

| File | What it is |
|---|---|
| `state-NN.json` | A phase's persisted stage-machine state (per phase since 14a) |
| `lock-NN` | Per-phase advance lock (held across gate waits) |
| `lock-project` | Coarse checkout lock (held seconds, around shared-git mutations) |
| `phase-NN-stdout` / `phase-NN-stderr.log` | Agent capture files (what `logs` tails) |
| `phase-NN-exit` / `phase-NN-agent-pid` | Exit code + PID the monitor records |
| `gates/NN-<stage>.json` (+ `.response.json`, `.ack.json`) | Gate request / answer / receipt |
| `events.jsonl` | Append-only event log (schema v1, one JSON object per line, phase id on every line) — tail it from any tool |
| `cron-instructions-NN.json` | Rate-limit resume record for a paused sequentagent run |

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
`devflow recover --clean` (stale phases only) or
`devflow recover --clean --phase N` to reset one phase, then re-`start`.
