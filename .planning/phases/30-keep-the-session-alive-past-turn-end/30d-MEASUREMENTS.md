---
unit: 30d
claude_code_version: 2.1.220
run_date: 2026-08-02
harness: 30d-exit-timing-harness.py
launcher: launch_in_monitor_env (imported from 30c-monitor-env-harness.py)
evidence: 30d-evidence/mode-a/trial-1..5, 30d-evidence/mode-b/trial-1..2

mode_a_iterations: 5
mode_a_exit_latency_ms_min: 169.5
mode_a_exit_latency_ms_median: 242.0
mode_a_exit_latency_ms_max: 279.7
mode_a_measured_process: "the claude CLI itself, via the pid the sh wrapper records"
mode_a_poll_resolution_ms: 2

mode_b_observation_window_s: 90.0
mode_b_observation_window_floor_s: 52.0
mode_b_summary: exits_cleanly

mode_b_trials:
  - trial: 1
    process_exited: true
    exit_code: 0
    exit_latency_ms: 42699.1
    results_after_close: 2
    final_result_truncated: false
    drained_event_observed: true
    child_a_signal_file: present
    child_b_signal_file: present
    stderr_nonempty: false
    cleanup_action: "process group 1144453 signalled (SIGTERM: skipped, group already empty); survivor check completed: true; no survivors"
    observation_window_s: 90.002
  - trial: 2
    process_exited: true
    exit_code: 0
    exit_latency_ms: 37059.5
    results_after_close: 2
    final_result_truncated: false
    drained_event_observed: true
    child_a_signal_file: present
    child_b_signal_file: present
    stderr_nonempty: false
    cleanup_action: "process group 1149247 signalled (SIGTERM: skipped, group already empty); survivor check completed: true; no survivors"
    observation_window_s: 90.036

longest_quiet_gap_milestone_s_min: 7.7
longest_quiet_gap_milestone_s_max: 13.728
longest_quiet_gap_all_events_s_min: 6.019
longest_quiet_gap_all_events_s_max: 7.087
---

# 30d measurements — exit timing, and what closing stdin early actually does

**Two headline results, and the second is the one that matters.**

1. **The 0.38s exit figure is not corroborated.** Five archived trials put the
   CLI's post-close exit latency at **169.5–279.7 ms, median 242.0 ms**. Every
   single measurement is *below* 0.38s; the cited figure sits 1.36x above the
   maximum and 1.57x above the median.
2. **Closing stdin with pending tasks does not discard child work.** In both
   Mode B trials the CLI kept running for **37–43 seconds after the close**,
   both children completed and wrote their signal files, the task set drained,
   **two** notification-origin `result` events arrived *after* the close, and
   the process then exited on its own with code 0. Nothing was truncated and
   nothing was lost.

The second result was explicitly undefined before this run. Review constraint 4
states that close-with-pending-tasks "is untested and must be treated as
undefined". It is now tested. See
[What this means for Phase 31's close rule](#what-this-means-for-phase-31s-close-rule)
— the honest reading is that constraint 4's drain gate is **defensive, not
load-bearing**, and that is reported here even though it makes a locked
constraint look less urgent.

A third result fell out of the run and contradicts a locked constraint in the
other direction: **binding constraint 8's ~12s idle-timeout floor is too low.**
Two of seven trials had a quiet gap above 12s. See
[Constraint 8 is revised upward](#constraint-8-is-revised-upward-not-merely-confirmed).

## How these trials were run

Every trial launched through **plan 30-02's `launch_in_monitor_env`**, imported
from `30c-monitor-env-harness.py` — `sh -c` with production's script shape,
environment scrubbed of the 18 names parsed out of `crates/devflow-core/src/git.rs`
at runtime, `start_new_session=True`, no TTY, stderr to its own file. This
harness contains no second launch of the `claude` binary and no second
redactor; with 30c's file made unreadable it aborts with exit 2 rather than
falling back to a direct launch (verified during Task 1).

`30a-evidence/run_experiment_v3.py`'s direct-exec, merged-stderr shape was
deliberately **not** extended. Only the experiment core is inherited from it —
the two-concurrent-children prompt (10s and 22s sleeps) and the never-block read
loop — and the prompt itself comes from 30c's verbatim copy.

Each trial's `run.log` records the provenance identically:

```
launcher_source: 30c-monitor-env-harness.py
launcher_function: launch_in_monitor_env
git_scrub_list_parsed: GIT_ALTERNATE_OBJECT_DIRECTORIES, GIT_CONFIG, ... (18 names)
removed_variables: GIT_DIR, GIT_WORK_TREE
stdout_and_stderr_are_distinct_paths: True
```

`removed_variables` is non-empty because 30c's `GIT_DIR`/`GIT_WORK_TREE` decoys
were planted before every trial, so production's git scrub did real work
*during* each measured run rather than removing nothing. Both decoys point at
paths that do not exist, so a scrub failure would have surfaced as a loud git
error inside the child.

All intervals use `time.monotonic()`. Wall-clock is subject to NTP adjustment
mid-measurement, and sub-second intervals are exactly where that shows up.

## Mode A — exit latency after a drained close

Stdin was closed only after the `background_tasks_changed` task set had drained
to empty **and** the stream had been quiet for 15 seconds. The 15s settle is
deliberate: 30c measured the drain-to-final-result lag at 4.54–11.51s, so a
shorter settle would have closed on a live run mid-turn and measured the exit
latency of a truncated session instead of a finished one.

| Trial | Exit latency (CLI) | Exit code | Wrapper exit latency | Closed at | Drained at | Last `result` | `result`s after close |
|-------|--------------------|-----------|----------------------|-----------|------------|---------------|-----------------------|
| 1 | **169.5 ms** | 0 | 198.7 ms | t+64.67 | t+39.63 | t+49.67 | 0 |
| 2 | **215.5 ms** | 0 | 263.8 ms | t+61.76 | t+37.90 | t+46.75 | 0 |
| 3 | **242.0 ms** | 0 | 267.8 ms | t+54.79 | t+36.99 | t+39.78 | 0 |
| 4 | **276.0 ms** | 0 | 315.0 ms | t+58.67 | t+35.91 | t+43.61 | 0 |
| 5 | **279.7 ms** | 0 | 307.1 ms | t+58.71 | t+37.48 | t+43.71 | 0 |

**min 169.5 ms · median 242.0 ms · max 279.7 ms · n=5 · spread 1.65x**

Every trial exited on its own; none required a kill. `results_after_close` is 0
in all five, confirming the close landed after the session had genuinely
finished rather than cutting a turn short.

Two latencies are reported per trial because two processes exit. **The CLI
figure is the comparable one** — v3 measured a directly-launched `claude` with
no wrapper. The `sh` wrapper that `wait`s on the CLI exits a further 25–49 ms
later; a production monitor watching the wrapper would see the larger number.

Reproduce the aggregates from the archived per-trial timings without rerunning
a single CLI trial:

```
python3 30d-exit-timing-harness.py recompute
```

That is the point of archiving per-trial files at all. The figure this document
replaces is unreproducible precisely because it was never archived.

### Is the 0.38s figure corroborated? No.

**It is not.** 0.38s lies above the entire archived distribution:

| | Value | Ratio to 0.38s |
|---|---|---|
| Archived minimum | 169.5 ms | 0.45x |
| Archived median | 242.0 ms | 0.64x |
| Archived maximum | 279.7 ms | 0.74x |
| ROADMAP's cited figure | 380 ms | 1.00x |

The figure is not *wrong in kind* — it is the same order of magnitude, and the
qualitative claim it supports ("the CLI exits promptly, in a fraction of a
second, once stdin closes") is confirmed. But as a point estimate it
**overstates the latency by roughly 60% against the median**, and no trial came
within 100 ms of it.

Two mechanical reasons make that unsurprising, both of which argue the single
sample was measured coarsely rather than that the CLI has slowed down:

- **v3's measurement had ≥100 ms granularity.** Its post-close loop ran
  `select(..., 0.5)` followed by `time.sleep(0.1)` per iteration
  (`30a-evidence/run_experiment_v3.py:169-178`), so "0.38s" means "detected
  somewhere in a ~100 ms-wide bucket ending at 0.38s".
- **30c's eight recorded values are quantised too, at its own 0.25s poll.** Its
  `exit_delay_after_stdin_close` fields read 0.25, 0.25, 0.25, 0.5, 0.5, 0.5,
  0.51, 0.51 — three distinct values across eight trials, which is a poll
  artifact, not a distribution. Those values are *consistent* with this
  measurement (all ≤0.51s) but carry no sub-second information, so they are not
  pooled with it here.

This harness polls at 2 ms for the first 10 seconds after the close, which is
~20x tighter than the 30c figure's grain and ~50x tighter than v3's.

**Recommended ROADMAP correction:** replace the cited `0.38s` with
`~0.24s (median of 5; 0.17–0.28s)` and cite `30d-evidence/mode-a/`.

## Mode B — closing stdin while tasks are still pending

Stdin was closed as early as the design allows: immediately after the first
`result` event arrived while `background_tasks_changed` still listed **two**
outstanding `local_agent` tasks. This mode had no expected outcome. Hang,
immediate exit, truncated final result and silently discarded child work were
all admissible findings.

### Observation window, and why it cannot have manufactured the result

The window is 90 seconds from launch, against a floor of 52 seconds
(the slowest child's 22s deadline plus a 30s delivery buffer). The floor is
enforced by an abort — `--window 51.9` refuses to run.

Measured from the **last child dispatch** rather than from launch, observation
continued for **63.1s** (trial 1) and **62.3s** (trial 2), against the 22s
deadline that child had to meet. That is 2.9x the deadline, and 41s past
`deadline + buffer`. Both children's signal files are therefore recorded as
`present` on direct on-disk observation, not inferred — and had they been
missing, the window was long enough for `absent` to have been a real absence
rather than an early stopwatch.

### Trial 1 — narrative

Stdin closed at **t+8.56**, with two outstanding `local_agent` tasks
(`background_tasks_changed` last reported a 2-entry array). Then:

- `process_exited: true`, `exit_code: 0`. The CLI did **not** die at the close
  and did **not** hang. It exited of its own accord.
- `exit_latency_ms: 42699.1` — **42.7 seconds** after the close syscall
  returned. Reported in the same field as Mode A's sub-second figures because
  it is the same measurement; the five-order-of-magnitude difference *is* the
  finding.
- Child A's signal file appeared at t+24.31 (15.8s after the close); child B's
  at t+33.16 (24.6s after the close). Both `present`, contents exactly
  `CHILD-A-DONE` / `CHILD-B-DONE`.
- `drained_event_observed: true` at t+37.20 — the task set drained to `[]`
  nearly 29 seconds after stdin was closed.
- `results_after_close: 2`. Both are notification-origin, both arrived at
  t+50.93. Counted, not timestamp-compared: the result that *triggers* the
  close shares its instant, and comparing timestamps counted the trigger
  itself (fixed in `7e7877c`).
- `final_result_truncated: false`. Basis: the capture ends on a complete
  `result` event with `is_error: false` and 789 characters of result text, with
  no trailing `assistant` events.
- `stderr_nonempty: false` — the CLI logged no complaint about the closed pipe.
- `cleanup_action`: process group 1144453 signalled; SIGTERM skipped because
  the group was already empty; **survivor check completed, no survivors**.
- `observation_window_s: 90.002`, of which 63.1s followed the last dispatch.

### Trial 2 — narrative

The same shape, independently. Stdin closed at **t+10.43** with two outstanding
`local_agent` tasks.

- `process_exited: true`, `exit_code: 0`.
- `exit_latency_ms: 37059.5` — **37.1 seconds** after the close.
- Child A signalled at t+25.22 (14.8s post-close), child B at t+36.41 (26.0s
  post-close). Both `present`, both with exact contents.
- `drained_event_observed: true` at t+38.38.
- `results_after_close: 2`, at t+38.53 and t+47.19, both notification-origin.
  Unlike trial 1 these arrived 8.7s apart rather than in the same instant.
- `final_result_truncated: false`. Basis: capture ends on a complete `result`,
  `is_error: false`, 718 characters, no trailing `assistant` events.
- `stderr_nonempty: false`.
- `cleanup_action`: process group 1149247 signalled; group already empty;
  survivor check completed, no survivors.
- `observation_window_s: 90.036`, of which 62.3s followed the last dispatch.

### On `mode_b_summary`

The frontmatter carries `mode_b_summary: exits_cleanly` for readability. **The
per-trial fields above are authoritative wherever they disagree with it**, and
they disagree with it in emphasis immediately: "exits cleanly" is true but
badly incomplete, because it says nothing about the ~40 seconds of useful work
the CLI performed *between* the close and that clean exit. A reader who takes
the token and stops will conclude the CLI shut down on close, which is the
opposite of what happened. This is exactly why the token is secondary.

The two trials did not disagree with each other, so no disagreement had to be
resolved. Had they, both would have been recorded as observed and the
divergence described rather than collapsed into `nondeterministic`.

## What this means for Phase 31's close rule

Review constraint 4 requires the monitor to close stdin only on marker **AND**
drained task set, on the stated grounds that closing with pending tasks is
undefined. That premise is now retired: the behaviour is defined, observed
twice, and **benign**.

**Constraint 4's drain gate is defensive, not load-bearing.** On this evidence a
monitor that closed stdin the moment it saw its marker — with children still
running — would still have received every child's completion, still seen the
drain, and still gotten a complete final result. Closing early cost nothing
observable.

That finding is reported plainly because suppressing it would be the failure
mode this phase exists to remove. It does **not**, however, argue for removing
the gate, for three reasons that live in the evidence rather than in caution:

1. **n=2.** Two trials establish that the benign path exists, not that it is the
   only path. The same statistical logic 30-02 applied to delivery applies here
   with far less data.
2. **The drain gate is nearly free.** The CLI stayed alive ~40s past the close
   *anyway*, so waiting for the drain before closing costs the monitor roughly
   nothing in wall-clock — it is already going to wait. A gate that buys
   insurance at no measured cost should not be traded away for n=2.
3. **It removes a whole class of question.** With the gate, "what if the CLI's
   behaviour on early close changes in a later version" is not a question the
   monitor has to have an answer for. The behaviour measured here is
   undocumented and unpinned (review finding M2), on one CLI version.

**Recommendation:** keep constraint 4's `AND`, and rewrite its *justification*.
It currently rests on "undefined, must be treated as unsafe", which is no longer
true and will read as stale to the next person. It should rest on "measured
benign at n=2 on an undocumented code path; the gate costs nothing because the
CLI outlives the close regardless."

Two secondary consequences for Phase 31:

- **Do not treat process exit as a completion signal after an early close.** The
  process outlived its close by 37–43s in these trials while doing real work. A
  monitor that closes stdin and then waits on the process will block for tens of
  seconds — correct behaviour, but it must not be mistaken for a hang, and any
  supervising timeout must exceed it by a wide margin.
- **999.64's original failure mode is not reproduced by this mechanism.** Closing
  stdin early did not silently discard child work in either trial. If Phase 31
  loses child work, this is not the cause.

## Constraint 8 is revised upward, not merely confirmed

Binding constraint 8 says the idle timeout "must not sit below ~12s; the drain
is not a stop signal", derived from 30c's observed 10.52–11.51s band of longest
quiet gaps.

**Two of these seven trials exceeded 12 seconds.**

| Measure | 30c (7 trials) | 30d (7 trials) | Pooled max |
|---------|----------------|----------------|------------|
| Longest quiet gap, **milestone events** | 10.52 – 11.51 s | **7.70 – 13.73 s** | **13.73 s** |
| Longest quiet gap, **every stream line** | not measurable | 6.02 – 7.09 s | 7.09 s |

The gaps above 12s were 13.638s (mode-a trial 1) and 13.728s (mode-b trial 1).
**A 12-second idle timeout would have killed a live, healthy run in 2 of 7
trials here.** Constraint 8's direction was right and its number is not.

Two definitional points, because the two rows measure different things and the
constraint depends on which is meant:

- **Milestone events** = `result` + `task_notification` +
  `background_tasks_changed`. This reproduces 30c's published band exactly for
  its seven trials (recomputed this session: 11.03, 10.77, 10.78, 10.52, 10.81,
  11.28, 11.51), which is why the two units' numbers are comparable at all.
  30c could measure nothing else — its published `run.log` carries no per-line
  timestamps.
- **Every stream line** = all 50–57 parsed events per trial, which is what a
  monitor's idle timer actually resets on. This harness timestamps each line as
  it is read, so it can measure what 30c could not. The maximum is 7.09s —
  roughly half the milestone figure.

Mode-a trial 1 is instructive about *why* the tail is longer than 30c saw: it
was a **coalesced** run (2 `result` events, 1 notification-origin, both children
delivered — finding F-1's signature), and coalescing merges two turns into one,
lengthening the quiet interval before the surviving turn lands. Coalescing
occurred in 1 of 7 trials here, matching 30c's 1 in 7 exactly; pooled, 2 in 14.

**Recommendation:** raise constraint 8's floor. If the monitor's idle timer
resets on milestone events only, no value below **30s** has margin — that is
~2.2x the pooled observed maximum, and 14 trials do not establish a tail. If it
resets on every stream line, 30s carries ~4x headroom against the 7.09s
observed maximum. Either way the recommended number is the same and the cheaper
implementation (reset on any line) is also the safer one.

## Pitfall 6's `local_bash` pair — assumption A2 now has evidence

RESEARCH.md records an extra `local_bash`-typed `task_started`/`task_notification`
pair per child that never appears in `background_tasks_changed`, with assumption
A2 being that it is informational. The drain gate here considers **only**
`local_agent` entries (an entry lacking `task_type` is conservatively counted as
still outstanding — 0 such entries were seen), while every `local_bash` event is
recorded rather than filtered out.

Across all 7 trials, without exception:

- 4 `local_bash` events per trial: 2 `task_started` and 2 `task_notification`.
- Zero `local_bash` entries ever appeared in a `background_tasks_changed` array.
- The `usage` discriminator holds: `has_usage` was `false` for every
  `local_bash` notification and `true` for every `local_agent` notification —
  the pattern `[false, true, false, true]` in all 7 trials.

A2 is consistent with 7 further trials and nothing in them contradicts it.
Attribution is by task_id correlation, not by reading a field: only
`task_started` carries `task_type`, so filtering later events on that field
silently records none of them (fixed in `7e7877c`).

## Cleanup — verified, not asserted

Every trial ran in its own process group, recorded its pgid, and terminated that
group in a `finally` block covering completion, timeout and exception. The reap
is then **verified**: a running census unions process-group membership with a
descendant walk, and after signalling, the census is re-checked for survivors.
All 7 trials record `survivor check completed: true` and `no survivors`.

The descendant arm is not redundant. In the deliberately interrupted trial run
during Task 1, the cleanup record reads:

```
killpg(1040458, SIGTERM); kill(1041393, SIGTERM) [out of group];
kill(1041395, SIGTERM) [out of group]; SIGKILL: skipped, group already empty
```

**Two descendants had left the process group.** `killpg` alone would have missed
them — precisely the orphaned-grandchild leak the review finding predicted, and
precisely the shape backlog 999.46 already tracks. The process listing taken
after that interrupt showed the group empty; the process listing taken after the
final trial shows no surviving `claude` process from any trial's group (the only
`claude` process on the machine is the long-lived interactive session that
launched this work, pgid unrelated to any trial).

## Evidence integrity

Raw stdout and stderr were staged in a per-trial `tempfile.mkdtemp` directory
outside the repository and published only through 30c's imported
validate → structural redact → secret-scan → atomic replace pipeline. No second
redactor was written.

| Scanned | Result |
|---------|--------|
| All 28 published files under `30d-evidence/`, plus this document | **0 matches** |
| Staged raw captures (never committed), as a live control | `home_path`, `os_username`, `session_identifier` |

The scan hunted **28 real session UUIDs** harvested from the still-present
staged captures, so a clean result on the published tree is a real negative
rather than a tautology. The control line is what makes it one: the scanner
still matches on unsanitised input.

**One scanner false positive to know about before you re-run this.** Scanning
`30d-exit-timing-harness.py` itself — outside the scope above, but an easy thing
to include — matches `credential_named_assignment` on the line
`except KeyboardInterrupt:`. The pattern is case-insensitive and
`KeyboardInterrupt` contains `Key`, followed by a colon and six-plus non-space
characters. It is a Python keyword, not a credential. Recorded here so the next
person does not spend time on it or, worse, "fix" it.

`git status --porcelain crates/` was empty at every task boundary. No file under
`crates/` was modified by this plan.

## Limits of this evidence

- **n=5 for Mode A and n=2 for Mode B.** Two trials establish that Mode B's
  benign path exists; they do not establish that it is the only path, and every
  conclusion drawn from Mode B above is conditioned on that. The exact
  one-sided 95% bound on a 2/2 observation is `0.05^(1/2) = 0.224` — the true
  rate of the benign outcome is established only as above about 22%.
- **One machine, one CLI version (2.1.220), one afternoon.** All seven archived
  trials ran back to back inside about ten minutes (first publish 17:02:58,
  last 17:10:32) and share that window's conditions.
  The behaviour measured is undocumented and unpinned (review finding M2); a
  CLI update may change it without notice.
- **The machine was not quiescent.** Load average sat at ~5 throughout, driven
  by an unrelated browser and, for part of the session, a concurrent sibling
  plan's `cargo test --workspace` (which was waited out before the first
  measured trial). Sub-second exit latency is the measurement most exposed to
  this; the 1.65x spread across five trials may be partly ambient load rather
  than CLI variance.
- **Both children are trivial sleeps.** Nothing here speaks to closing stdin on
  a session whose children are long-running agents producing heavy output — the
  Phase 29 wave-2 shape in every respect except duration and payload. A child
  that is still streaming when stdin closes is untested.
- **The trials ran with agent-session markers present.** `--scrub-agent-markers`
  was deliberately left off, following 30-02's finding F-2 that scrubbing them
  diverges from production, which carries `ANTHROPIC_API_KEY` from the
  operator's global config. Production launched from a plain shell would have
  those markers genuinely absent; these trials had them inherited. 30c confirmed
  delivery in both conditions, and exit delay showed no arm-dependence there,
  but this is an untested variable for exit timing specifically.
- **Mode B's close point is one specific early moment** — the first `result`
  with two tasks outstanding. Closing at other points (mid-tool-call, after one
  child drained but not the other) is untested.
- **The quiet-gap figures are read-times, not the CLI's write times.** This
  harness timestamps each line when it reads it from the staged file. Any
  buffering between the CLI's write and the file becoming readable is included
  in the interval.

## Reproducing this

```
python3 30d-exit-timing-harness.py mode-a --iterations 3 --start-index 1
python3 30d-exit-timing-harness.py mode-a --iterations 2 --start-index 4
python3 30d-exit-timing-harness.py mode-b --iterations 2
python3 30d-exit-timing-harness.py recompute
```

Each trial's `run.log` records its own invocation verbatim. The harness reports
observations and prints no verdict, for the reason 30a's README records: the
harness-printed verdict was wrong in v1 and unreliable in v2. The published
captures are the evidence of record.
