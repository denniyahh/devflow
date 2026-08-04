---
unit: 30c
trial_set: reliability-replication
arm: agent-session-markers-scrubbed
trials: 5
confirmed: 5
refuted: 0
partial_delivery_trials: 0
success_rate: "5/5"
cumulative_across_all_configurations: "7/7"
claude_code_version: 2.1.220
git_vars_removed_per_trial: 2
agent_session_markers_removed_per_trial: 11
unparseable_lines_total: 0
observation_window_s: 75
run_date: 2026-08-02
evidence: 30c-evidence-reliability/trial-1 .. trial-5
companions:
  - 30c-VERDICT.md
  - 30c-VERDICT-scrubbed.md
harness: 30c-monitor-env-harness.py --replicate 5
---

# 30c reliability replication — 5 of 5 confirmed

**No trial refuted. No trial partially delivered.** Five trials in the trial-2
scrubbed configuration, held fixed, each delivering both children. Combined with
the two earlier trials, delivery has now succeeded in **7 of 7 trials across 2
configurations**.

There is no headline failure to report first, because there was none. If there
had been, it would be at the top of this document with its evidence path.

## Per-trial results

Every column is derived by re-reading the published capture under
`30c-evidence-reliability/trial-N/`, not from anything the harness printed.

| Trial | Children delivered | Signal files on disk | Drain to `[]` | `result` events | Notification-origin results | Coalesced | Unparseable | Verdict |
|-------|--------------------|----------------------|---------------|-----------------|------------------------------|-----------|-------------|---------|
| 1 | 2 / 2 | 2 / 2 exact | yes, line 44 | 3 | 2 | no | 0 | **confirmed** |
| 2 | 2 / 2 | 2 / 2 exact | yes, line 48 | 3 | 2 | no | 0 | **confirmed** |
| 3 | 2 / 2 | 2 / 2 exact | yes, line 45 | 3 | 2 | no | 0 | **confirmed** |
| 4 | 2 / 2 | 2 / 2 exact | yes, line 51 | 3 | 2 | no | 0 | **confirmed** |
| 5 | 2 / 2 | 2 / 2 exact | yes, line 47 | 3 | 2 | no | 0 | **confirmed** |

"Children delivered" is judged per child — each spawned `task_id` from
`background_tasks_changed` must appear as a `task_notification` — **not** by
counting notification-origin `result` events. Trial 2 of the earlier pair proved
those two numbers can differ. "Signal files" is the independent on-disk check:
both files present with contents exactly `CHILD-A-DONE` / `CHILD-B-DONE`,
recorded per trial in each `run.log` as `signal_A_contents` / `signal_B_contents`.

## Cumulative picture

| Trial | Configuration | Markers scrubbed | Git vars scrubbed | Children delivered | `result` events | Notification-origin | Verdict |
|-------|---------------|------------------|-------------------|--------------------|-----------------|---------------------|---------|
| 1 | contaminated | 0 | 0 (none set) | 2 / 2 | 3 | 2 | confirmed |
| 2 | scrubbed | 11 | 0 (none set) | 2 / 2 | 2 | 1 | confirmed |
| rep 1 | scrubbed | 11 | **2** | 2 / 2 | 3 | 2 | confirmed |
| rep 2 | scrubbed | 11 | **2** | 2 / 2 | 3 | 2 | confirmed |
| rep 3 | scrubbed | 11 | **2** | 2 / 2 | 3 | 2 | confirmed |
| rep 4 | scrubbed | 11 | **2** | 2 / 2 | 3 | 2 | confirmed |
| rep 5 | scrubbed | 11 | **2** | 2 / 2 | 3 | 2 | confirmed |

**7 / 7.** Zero refutations, zero partial deliveries, zero unparseable lines
across the 280 published capture lines of the replication set (391 including
the two earlier trials).

## The git scrub now does real work

The earlier caveat — that the scrub removed nothing because no `GIT_*` variables
were set — is closed. `GIT_DIR` and `GIT_WORK_TREE` were planted in the parent
before the replication set, and every trial's `run.log` records
`removed_variables: GIT_DIR, GIT_WORK_TREE`. Both decoys point at paths that do
not exist, so a scrub failure would have surfaced as a loud git error inside the
child rather than a silent redirect.

This means the replication set exercised production's actual `hermetic_command`
behaviour during the measured run, not only in a separate mechanism check.

## Coalescing is real but rare — 1 in 7

Trial 2 of the earlier pair delivered both children through **one** resumed turn
(2 `result` events, 1 notification-origin). All five replication trials produced
the three-event shape (3 `result` events, 2 notification-origin).

So coalescing occurred in **1 of 7 trials**. It is not the common case — and it
is not a fluke to be discounted either, because when it happens the observable
signature is indistinguishable from "one child lost" unless the drain is checked.

**The design consequence for Phase 31 is unchanged and stands on the one
occurrence:** a monitor must not count notification-origin `result` events to
decide how many children returned. The count is 2 in six trials and 1 in the
seventh, while the delivered-children count was 2 in all seven.

## Timing — and why wall-clock duration is the wrong number

Wall-clock duration per trial was 75.34s, 75.62s, 75.66s, 75.34s, 75.44s.
**Do not read that as low variance.** The harness holds a deliberately FIXED
75-second observation window across every replication trial so the trials are
comparable, and that window dominates the measurement. The durations are nearly
identical because the window is, not because the experiment is.

The timings that carry information are internal:

| Measure | Range across all 7 trials | Relevance |
|---------|---------------------------|-----------|
| First `result` (turn 1 ends) | 9.04 – 9.80s | stable |
| Last `result` event | 42.41 – 46.88s | spread 4.47s |
| Child A signal written | 23.21 – 27.36s | spread 4.15s |
| Child B signal written | 32.62 – 36.07s | spread 3.45s |
| Task set drained to `[]` | 35.37 – 38.07s | spread 2.70s |
| **Drain → last `result`** | **4.54 – 11.51s** | **2.5x variance** |
| **Longest quiet gap between events** | **10.52 – 11.51s** | **idle-timeout floor** |

Two numbers matter for Phase 31's idle-timeout design (review constraint 5):

- **The longest quiet gap in a healthy run was 11.51s**, and every trial's
  longest gap sat in a tight 10.52–11.51s band. An idle timeout below roughly
  12 seconds would have cut a live, healthy run in all seven trials. A safe
  floor is meaningfully above that — the band is consistent, but seven trials
  do not establish a tail.
- **The lag from task-set drain to the final result varied 4.54s → 11.51s**,
  a 2.5x spread. This is the least stable interval measured, and it is exactly
  the window in which a monitor would be tempted to conclude "everything
  finished, nothing more is coming". Concluding that at the drain would have
  truncated the final orchestrator turn in all seven trials.

## Confirmation bar

Applied identically per trial, unchanged from the earlier two:

- Both children ran to completion, evidenced by signal files on disk; **and**
- both spawned `task_id`s appear as `task_notification` events; **and**
- either two notification-origin `result` events correlated to distinct
  children, **or** one such result together with an observed empty
  `background_tasks_changed.tasks`.

All five replication trials satisfy the first arm as well as the second, so
neither arm was load-bearing alone here — unlike trial 2 of the earlier pair,
where only the drain arm carried the verdict.

`partial_delivery` is `false` for every trial: no trial delivered one child
without the other.

## Limits of this evidence

- **n=5 in one configuration, on one machine, at one time of day, on one CLI
  version.** Five consecutive successes bound flakiness *loosely*, not tightly.
  The exact one-sided 95% binomial bound for 5/5 is `0.05^(1/5) = 0.549`, so the
  true success rate is only established as **above about 55%**. Pooling all
  seven trials gives `0.05^(1/7) = 0.652`, i.e. **above about 65%**. If Phase
  31's correctness depends on delivery being better than ~95% reliable, this set
  does not establish that — reaching a 95% floor at this confidence level needs
  roughly 59 consecutive successes.
- **All trials ran back to back within about seven minutes.** They share
  whatever transient conditions held in that window — service health, model
  routing, local load. Independent trials spread over days would test something
  this set does not.
- **Both children are trivial sleeps.** Nothing here speaks to delivery when
  children are long-running agents producing heavy output, which is the Phase 29
  wave-2 shape in every respect except duration and payload.
- **The observation window is fixed at 75s.** A delivery arriving after t+75
  would be invisible to this measurement and would read as a loss. No trial came
  close — the last result across all seven landed at 46.88s — but the window is
  an assumption, not a measurement.
- **Ancestry still is not environment.** The parent process remains `zsh`
  descended from an interactive `claude`. `launched_from_agent_session: False`
  means "no marker variables present", not "provably not descended from an
  agent".

## Evidence integrity

Same discipline, five more times. Each trial staged raw stdout and stderr in its
own `tempfile.mkdtemp` directory outside the repository, then published through
validate → structural redact → secret-scan → atomic replace.

The scan was run over all 15 published files with **7 real session UUIDs**
harvested from the still-present staged captures, so it was hunting live values
rather than checking a tautology:

| Scanned | Result |
|---------|--------|
| 15 published files under `30c-evidence-reliability/` | **0 matches** |
| staged raw captures (never committed), as a control | `home_path`, `os_username`, `session_identifier` |

The control line is the point: the scanner still matches on unsanitised input,
so the clean result on published files is a real negative rather than a broken
scanner.

Trials 1 and 2 remain byte-identical to their committed state, and
`git status --porcelain crates/` is empty.
