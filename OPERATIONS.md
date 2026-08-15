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
| `devflow status` | All active phases: stage, gate state, agent liveness, elapsed, last action |
| `devflow logs [-f] [--phase N] [--stderr]` | Print/follow a phase's captured agent output |
| `devflow history [N]` | Show chronological events with retained capture and review evidence for a phase |
| `devflow gate list` | Gates awaiting a response |
| `devflow gate approve <phase> [--stage S] [--note ...]` | Approve a gate — the workflow advances |
| `devflow gate reject <phase> --note ... [--stage S]` | Reject — loops back to Code; a note containing `abort` ends the phase |
| `devflow gate sweep [--max-age-secs N] [--dry-run] [--root PATH]` | Answer or report aged, unattended gates across every registered root (or one `--root`) — bounds an abandoned run's lifetime without `kill(1)`. On-demand only; nothing schedules it. Never writes an approval |
| `devflow parallel --phases 7,8 [--agents claude,codex] [--mode M] [--force]` | Run phases concurrently, each in its own worktree + monitor |
| `devflow list` | Feature branches with divergence from develop |
| `devflow reference [--branch B] [--refresh]` | Static snapshot worktree at `.worktrees/reference/` |
| `devflow stop --phase N [--root PATH]` | End a running phase cleanly (23c). Answers its open gate with a rejection if one is open — the target unwinds through its own abort path, no signal sent — otherwise signals the process recorded in `.devflow/lock-{phase:02}` (never `state.monitor_pid`, which the generated monitor script's trap only ever captures for the agent, not the trailing `advance`) after confirming it is alive and its `/proc/<pid>/cmdline` identifies it as belonging to DevFlow. Idempotent; marks `state.stopped`/`state.stop_reason` so `cleanup`'s existing fail-closed refusal (unweakened by this command) recognizes the phase as no longer live — `stop` then `cleanup --force` compose in that order |
| `devflow cleanup [--force]` | Remove phase worktrees + their feature branches |
| `devflow recover [--clean] [--phase N]` | Inspect state; `--clean` sweeps stale phases only; `--clean --phase N` clears one phase unconditionally |
| `devflow test` | cargo test + clippy + fmt --check |
| `devflow doctor [--json]` | Environment audit (agents installed, versions, RUST_LOG) plus per-phase reconciliation; `--json` emits one object `{"environment": [...], "reconciliation": [...]}` |
| `devflow release --check` | Read-only release-cut preflight: workspace self-pin, develop/main divergence (no `git fetch` — reads already-fetched refs), crates.io publish order, and `gpg.format`-aware tag-signing viability. `--check` is required; a bare `devflow release` is rejected toward the deferred release-cut executor (merge/tag/sync/publish, DEN-50) |
| `devflow ship --phase N [--force]` | **Dead-monitor recovery.** `devflow gate approve` only *writes* the Ship gate's response file — a live monitor polling for it is what actually advances the workflow. If that monitor died before consuming the response (e.g. the machine restarted mid-pipeline), the approval sits unconsumed forever. `devflow ship` reads the already-written Ship response directly and drives the phase through the same terminal path (`finish_workflow`) the live monitor would have — requires `state.stage == Stage::Ship` and an existing request+response pair with no prior ack. `--force` is accepted for explicit operator intent but never skips the stage check, the per-phase lock, the gate-existence check, or the ack check — it can never be used to skip Validate or jump ahead of a healthy pipeline. If the Ship response routes to a rejection that loops back to Code, `devflow ship` launches a **new, detached monitor agent** to drive the retry — the command prints this explicitly so it is never a silently long-running process. |
| `devflow evidence --phase N [--json] [--require-shipped] [--root PATH]` | **Read-only structural oracle (23-06):** reports DevFlow's own append-only record of whether a phase actually shipped, instead of trusting an agent-authored attestation document. `shipped` is strictly true only after the terminal-only `workflow_shipped` event has been emitted (the last step inside `finish_workflow_with_gate_timeout`, once the entire post-Ship hook batch has succeeded) — a phase halted by `--until <stage>` always reports `shipped: false`, even though it emits the older, ambiguous `workflow_finished` event too (surfaced separately as `workflow_finished_seen`/`finished_reason` for corroboration, never consulted by `shipped`). `--require-shipped` exits non-zero unless `shipped` is true, so it is declarable as a `verify::external_verify_commands` Layer 0 probe (opt-in per phase; a phase's PLAN must declare it and the operator must approve it via `DEVFLOW_TRUST_EXTERNAL_VERIFY`) — a failed declared probe outranks every agent-controlled signal. |

(`devflow advance` is internal — invoked by monitors with `--phase N`.)

(`devflow monitor` is internal and registered as the hidden `__monitor`
subcommand — it is the pipe-owning monitor's own process body, re-exec'd
detached by `spawn_monitor` for a Claude `stream-json` launch. It supervises
the agent's stdin/stdout, then calls the same `advance` the shell monitor's
tail invokes. Never run it by hand: it expects to own a child's pipes and
writes the phase's capture, exit-code and agent-pid files as it goes.)

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
| `DEVFLOW_GATE_TIMEOUT_SECS` | 259200 (3d) | How long a monitor stays alive holding the phase lock while parked at a gate. Not a deadline for answering: the gate request and phase state are files that never expire, and timing out is a clean resumable stop — past it you answer the gate and run `devflow resume` instead of the answer being picked up automatically |
| `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS` | 60 | How long `devflow ship --phase`'s foreground manual override waits for a reopened Ship gate (terminal-hook failure) before failing fast, instead of `DEVFLOW_GATE_TIMEOUT_SECS`' multi-day default |
| `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` | 120 | Wait on the shared-checkout lock; on timeout the hook batch is skipped (loudly), never run unserialized |
| `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` | 259200 (3d) | Age threshold `devflow gate sweep` uses to decide a gate is abandoned. Held **equal to** `DEVFLOW_GATE_TIMEOUT_SECS`, never shorter: `gate sweep` writes an `abort:` response, so a threshold below the poll timeout lets a sweep reap gates a live monitor is still polling — turning a clean resumable timeout into an `abort()` that clears state, machine-wide across every registered root. An unparsable value or explicit `0` falls back to the default rather than reaping every gate on the machine |
| `DEVFLOW_CACHE_DIR` | unset (falls back to `$XDG_CACHE_HOME/devflow`, then `$HOME/.cache/devflow`) | Test/override hook for the machine-global registry directory (`devflow gate list --all-roots`) |
| `DEVFLOW_CAPTURE_RETENTION` | 5 | Capture generations retained per phase; overrides `devflow.toml` |
| `DEVFLOW_REVIEW_ANGLES` | built-in five-angle list | Comma-separated Ship review angles; overrides `devflow.toml` |
| `DEVFLOW_EXTERNAL_VERIFY_ENABLED` | true | Enable PLAN-declared external post-condition probes; overrides `devflow.toml` |
| `DEVFLOW_YES_SHIP` | false | Standing Ship gate pre-authorization (D-12, `28-CONTEXT.md`); overrides `devflow.toml`'s `yes_ship` key, which itself is OR-combined with `--yes-ship` at `devflow start` |
| `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` | 120 | How long the pipe-owning Claude monitor waits with NO output on the child's stream before recording an `idle_timeout` verdict and terminating it. Clamped to a 120s floor: a lower value is raised and the clamp is printed loudly at `devflow start`. The floor is 4x the CLI's measured `tool_progress` keepalive interval (a fixed 30.00s, measured 2026-08-03 over five workload-controlled trials on CLI 2.1.220), so it survives three consecutive missed keepalives; the earlier 30s floor sat exactly ON the keepalive interval and would have killed healthy stages running any tool call longer than ~30s, including `cargo test --workspace`. Every stream line resets the window, so a healthy long-running stage is never bounded by wall clock |
| `DEVFLOW_CLAUDE_LEGACY_LAUNCH` | false | D-11's escape hatch (31-04): force the pre-31 single-document Claude launch — prompt positionally in argv, `--output-format json`, the `sh` monitor — instead of the Phase 31 `stream-json` transport. OR-combined with `--legacy-claude-launch` at `devflow start` / `devflow resume` and persisted to `state-NN.json`, so a detached monitor honours it; a run authorized by this variable alone prints a notice naming it as the source. **Parsed as a bool: `=false` does NOT enable it, and an unparseable value warns and leaves the legacy path OFF.** Using it is announced on three channels (stdout, `phase-NN-monitor.log`, and a `claude_legacy_launch_forced` event in `events.jsonl`) because an escape hatch used routinely erodes what it protects. **What it gives up: the legacy path cannot deliver background-task notifications, so a multi-plan wave may orphan delegated work — that is 999.64, the defect Phase 31 exists to fix.** Once set for a run it is never cleared by `devflow resume`; edit `state-NN.json` to turn it off |
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
| `phase-NN-exit` / `phase-NN-agent-pid` | Exit code + PID the monitor records |
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
`devflow recover --clean` (stale phases only) or
`devflow recover --clean --phase N` to reset one phase, then re-`start`.
