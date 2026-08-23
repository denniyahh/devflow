# Operator observability findings — Phase 17 dogfood run (2026-07-18/19)

Captured from a live end-to-end dogfood of `devflow` driving Phase 17 on itself.
Source: operator observations during the run, each verified against the code
before being written down. Intended as scope input for a future phase.

Overlaps `ROADMAP.md` Phase 18's **18d** (project-aware `devflow doctor`
reconciliation). 18d proposes a *diagnostic command* that diffs state vs. events
vs. live PIDs. Findings 1 and 2 below argue the same reconciliation is needed
as an *always-on* property of `devflow status`, not only on demand — an operator
who does not already suspect a problem will never think to run `doctor`.

---

## Finding 1 — Monitor liveness is unobservable ("who watches the watcher")

**Verified in code.** `monitor_pid` is emitted into `events.jsonl` at spawn time
(`monitor::spawn_monitor`) but is **never persisted to `State`** (confirmed:
no `monitor_pid` field in `crates/devflow-core/src/state.rs`) and is **never
liveness-checked anywhere**. `devflow status` probes only `agent_pid`
(`main.rs:2917`).

The liveness probe itself is correct and well-hardened — `agent::agent_running`
uses `kill(pid, 0)` and explicitly rejects pid `0` and values above `i32::MAX`
that would wrap to `kill(-1, 0)`. The defect is not the probe; it is that the
process which actually owns the pipeline is never probed.

**Consequence.** A dead monitor is indistinguishable from a healthy
between-stages moment. Both render as:

```
agent_pid: <pid> (running: false)
agent is not running — the monitor may have already advanced
```

The word "may" is the tool conceding it cannot tell. There is no CLI path to
separate "evaluating, will advance in seconds" from "monitor died, this phase
is stuck permanently."

**Observed impact in this run: two separate monitor deaths, both silent.**

1. After the Code stage succeeded (verification 12/12), its monitor died with
   an empty stderr, no OOM trace, and no `advance_evaluated` event. The stage
   had *succeeded* and simply never advanced.
2. After a gate approval relaunched the Code stage, the same class of stall
   recurred.

Both were found only by running `ps` by hand, and both were recovered with
`devflow advance --phase 17`. Roughly four hours of wall-clock were lost across
the two incidents.

**Proposed direction.** Persist `monitor_pid` in `State` alongside the agent
pid, probe it in `status`, and make the three states explicit:

| monitor | agent | meaning |
|---------|-------|---------|
| alive | alive | running normally |
| alive | dead | evaluating / advancing (normal, brief) |
| **dead** | dead | **stuck — recover with `devflow resume --phase N`** |

The third row is currently unrepresentable and is the one that costs real time.
A stale-monitor check belongs in `recover` / `doctor` (18d) as well, but the
default `status` output is where an operator actually looks.

---

## Finding 2 — A phase tracks exactly one process

**Partially a duplicate of Finding 1, stated precisely.** `devflow status`
*does* already iterate every active phase, so a `devflow parallel` run shows one
block per phase — multi-*phase* display is not missing. The real gap is that a
phase has a single `phase-N-agent-pid` file, so:

- the monitor has nowhere to be recorded (Finding 1), and
- `sequentagent`'s second agent has nowhere to live.

Framing this as "two tracked processes per phase" (monitor + agent) is more
accurate than "display multiple agents."

**Related evidence — orphaned processes are invisible.** During this run a
leftover agent process from a *test fixture* was found still alive under
`/tmp/.tmpn0yheZ` (phase-12), owned by nothing and visible to no devflow
command. Whatever tracks processes should be able to surface strays.

---

## Finding 3 — The CLI assumes a reader who will parse JSONL

Goal stated by the operator: devflow should run just as well from a plain
terminal as it does with an LLM driving it. The supervision layer currently
falls short in specific, reproducible ways. Each item below is a point where
this run fell back to raw tooling that a human operator would not reach for.

- **Gate reasons are truncated.** Gate context ends in
  `"… [truncated; full output in .devflow/]"`. Reading the full blocking reason
  required grepping raw JSON out of `events.jsonl`. There is no
  `devflow gate show <phase>`.
- **Rate-limit reset time is never surfaced.** The agent's 429 carried
  `"You've hit your session limit · resets 12pm (America/New_York)"`, but that
  string existed only inside the raw JSON capture. The CLI reported the phase as
  rate limited without ever saying *when to come back*.
- **No intra-stage progress.** Learning that plans 17-03/04/05 had landed
  required reading the worktree's git log. `status` reports the stage but
  nothing about work completed inside it.
- **Recovery verbs are undiscoverable.** `advance` is documented as internal yet
  was the only thing that unstuck a dead monitor. `resume` (added by 17-04) is
  the correct verb and appears in no output a stuck operator would encounter.

---

## Cross-reference: the flake this run surfaced

`17-VALIDATION.md` GAP-2 documents
`concurrent_ship_advances_finish_both_phases_independently` as a latent race
where a reopened gate **polls forever with no timeout**. This run hit it live: a
`cargo test --workspace` invocation hung for 83 minutes with a thread parked in
`hrtimer_nanosleep`. It did not reproduce on retry (62/62 in 10.2s serial, 3.0s
parallel), which matches the auditor's warning that a green result for that test
is "a lucky pass, not evidence." Unbounded gate polling with no timeout is the
same operator-visibility theme: a hang that looks exactly like slow progress.
