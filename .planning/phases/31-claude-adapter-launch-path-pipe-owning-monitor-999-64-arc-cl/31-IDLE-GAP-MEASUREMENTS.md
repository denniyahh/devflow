# Phase 31 — Idle-Gap Measurement (D-02 revision)

**Measured:** 2026-08-03, during Phase 31 execution, between wave 2 and wave 3
**CLI:** `claude` 2.1.220 (the version this arc's delivery premise was witnessed on)
**Machine:** operator workstation, idle
**Outcome:** `IDLE_TIMEOUT_FLOOR_SECS` and `DEFAULT_IDLE_TIMEOUT_SECS` raised **30s → 120s**;
D-02's stated margin is superseded.

---

## Why this was run

D-02 locked the idle floor at 30s with the note: *"30s is ~4.2x margin — comfortable, not
marginal. Do not 'correct' this to a larger value on the assumption that 30s is tight; it is tight
only against a signal this phase is not using."*

The adversarial review of plan 31-05 (run before wave 3) flagged that the 4.2x figure came from
`30d-MEASUREMENTS.md`, whose trials ran **backgrounded 10s/22s sleeps** — a workload where the
agent never sits inside a long *foreground* tool call. 31-05's Code stage runs a post-merge
`cargo build` and test gate under a 300s timeout: one tool call that can be silent for minutes.

The reviewer was explicit that this was *a risk, not an observation* — no capture exercised it.
So it was measured.

## What was measured

Max gap between consecutive lines of `claude` `stream-json` output while the agent sits inside one
long silent foreground tool call. This is the quantity DevFlow's monitor actually bounds: the
reader thread forwards **every** line unfiltered into `recv_timeout(idle_timeout)`
(`monitor.rs`), so the timer measures *stream silence*, not child inactivity.

## Results

| workload | elapsed | lines | gaps > 5s |
|---|---|---|---|
| 90s busy loop, trial 1 | 99.4s | 17 | 26.23, **30.00**, 29.98 |
| 90s busy loop, trial 2 | 102.7s | 21 | 26.43, **30.00**, 30.00 |
| 90s busy loop, trial 3 | 103.8s | 19 | 26.37, **30.00**, 29.98 |
| `cargo test --workspace`, trial 1 | 87.9s | 23 | 26.43, **30.00**, 15.66 |
| `cargo test --workspace`, trial 2 | 93.8s | 26 | 26.42, **29.99**, 17.01 |
| negative control (no long call) | 2.4s | 8 | — (max 2.22) |

**Finding:** the CLI emits `tool_progress` keepalives on a **fixed 30.00s interval** during a long
tool call, variance ±0.02s across all five trials. The first gap after `task_started` is
consistently ~26.4s.

**Consequence:** against a 30s timeout the margin is approximately **zero**, and on the wrong side
— the timer starts when the previous line is *processed*, while the keepalive arrives 30s after it
was *sent*, plus pipe latency. The old floor would have killed healthy Code stages running any tool
call longer than ~30s. `cargo test --workspace` is such a call and sits inside DevFlow's own
post-merge gate, so this was the common path, not an edge case.

## Why 120s

4× the measured cadence. The hazard is not a marginally larger gap — it is a **dropped** keepalive,
which doubles the interval outright. 120s survives three consecutive missed keepalives. 90s (two
missed) is the lowest defensible value.

## Controls

Two, because the first attempt at this measurement was **void** for having only one.

1. **Negative control** — same harness, no long call: max gap 2.22s vs 30.00s. The harness
   discriminates.
2. **Workload control** — each trial asserts `elapsed >= workload duration` **and** no
   `tool_use_error`, else the trial is discarded rather than counted.

The workload control is not decorative. Attempt 1 used `sleep 75`, which this harness blocks
(`<tool_use_error>Blocked`); the long call never ran and its "5.23s max gap" measured nothing. It
reported a clean, plausible number. Only the elapsed-time check caught it. A first pass at Group B
was likewise discarded — an incremental `cargo build --tests` finished in ~19s, too short to
exercise the question — which is why Group B was re-run with the full test suite.

**Group B exists to test proxy validity.** A synthetic busy loop is a convenient instrument but
proves nothing about a real compile unless the two agree. They do — identical cadence — so the busy
loop is a valid proxy. That is now measured rather than assumed.

## What this does not establish

- **One machine, idle, one CLI version, two workload types.** It shows the 30.00s cadence is real
  and reproducible. It does **not** prove the interval is fixed across load, hardware, or CLI
  versions. That is exactly why the floor sits at 4× the observed maximum rather than just above it.
- **n=5 is a weak distribution.** It is strong enough to *refute* D-02's claimed 4.2x margin — a
  single counterexample does that — and far weaker as a basis for *setting* a value. 120s is chosen
  for headroom against an unmeasured failure mode (dropped keepalives), not because the data
  pinpoints it.
- **Nothing here validates the acceptance run.** This measures CLI output cadence only. Whether a
  multi-plan wave completes without orphaning delegated work remains 31-05's question.

## Provenance

Probe scripts and raw stamped `stream-json` captures were written to the session scratchpad. The
per-trial raw JSONL retains the arrival timestamp of every line, so any figure above can be
recomputed rather than taken on trust.

*Supersedes the margin claim in D-02 and in `30d-MEASUREMENTS.md` §idle-gap. The ≥30s floor's
original derivation is not wrong about its own workload — it does not transfer to this one.*
