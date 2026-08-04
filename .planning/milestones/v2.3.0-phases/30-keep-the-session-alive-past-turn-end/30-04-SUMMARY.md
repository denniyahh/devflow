---
phase: 30-keep-the-session-alive-past-turn-end
plan: 04
subsystem: experiment-harness
tags: ["experiment", "harness", "python", "measurement", "exit-timing", "process-reaping"]

# Dependency graph
requires:
  - phase: 30-keep-the-session-alive-past-turn-end
    provides: "30c-monitor-env-harness.py — launch_in_monitor_env, publish_jsonl/publish_text, scan_for_secrets, PROMPT_TEMPLATE, StagedTail (imported, not reimplemented)"
  - phase: 30-keep-the-session-alive-past-turn-end
    provides: "30a-evidence/run_experiment_v3.py — the experiment core only (two-child prompt, never-block read loop)"
  - phase: 27-hermetic-git
    provides: "REPO_LOCAL_GIT_VARS / ALSO_REDIRECTING_GIT_VARS — the 18-name env-scrub list parsed at runtime"
provides:
  - "30d-exit-timing-harness.py — two-mode measurement harness over 30c's production-replica launcher"
  - "30d-evidence/mode-a/trial-1..5 — post-close exit latency at 2ms resolution"
  - "30d-evidence/mode-b/trial-1..2 — the previously-undefined close-with-pending-tasks case"
  - "30d-MEASUREMENTS.md — exit latency distribution, Mode B per-trial fields, constraint 8 revision"
  - "Measured refutation of the 0.38s exit figure: 169.5-279.7ms, median 242.0ms"
  - "Measured definition of close-with-pending-tasks: benign, n=2"
  - "Evidence that constraint 8's ~12s idle-timeout floor is too low"
affects: ["31-monitor-rewrite", "999.64", "999.46"]

actuals:
  tokens: 148292
  tasks: 2
  commits: 5

tech-stack:
  added: []
  patterns:
    - "Import the prior unit's launcher rather than re-deriving it; abort on import failure instead of falling back, so a 'production replica' label can never be attached to a differently-launched run"
    - "Verify a process-group reap by census, not by asserting the kill: union group membership with a descendant walk, then re-check for survivors"
    - "Give an observation window a FLOOR tied to the slowest thing being observed, enforced by an abort, so a negative finding cannot be a stopwatch artifact"
    - "Record orthogonal observations as independent fields; a summary token is secondary and says so in the body"
    - "Count events across a boundary rather than timestamp-comparing them when the boundary is triggered BY an event"
    - "State a measurement's poll resolution alongside its value — a prior figure quantised to its own poll interval is not a distribution"

key-files:
  created:
    - ".planning/phases/30-keep-the-session-alive-past-turn-end/30d-exit-timing-harness.py"
    - ".planning/phases/30-keep-the-session-alive-past-turn-end/30d-MEASUREMENTS.md"
    - ".planning/phases/30-keep-the-session-alive-past-turn-end/30d-evidence/"
  modified: []

key-decisions:
  - "Imported 30c's launcher by path with importlib; abort with exit 2 when 30c is unreadable, verified by chmod 000 — never a direct-launch fallback"
  - "Measured the CLI process itself via the pid the sh wrapper records, not only the wrapper, so the figure is comparable to v3's directly-launched baseline"
  - "2ms post-close poll for the first 10s, then coarse — a Mode B hang must not spin at 500Hz and perturb what it measures"
  - "Mode A closes on drain PLUS 15s of stream quiet, not on a result count (constraint 7 forbids counting results)"
  - "Ran with agent-session markers PRESENT (--scrub-agent-markers off) per 30-02's finding F-2: scrubbing them diverges from production, which carries ANTHROPIC_API_KEY"
  - "Planted 30c's GIT_DIR/GIT_WORK_TREE decoys so the git scrub did real work during every measured run"
  - "Discarded the first full trial set and re-ran all seven after two harness fixes, so every archived trial comes from exactly the committed harness"

patterns-established:
  - "Smoke-test a measurement harness against the live system BEFORE committing it — a harness committed on a parse check alone is unverified"
  - "Reproduce the prior unit's derived statistic from its published artifacts before claiming to revise it; 30c's quiet-gap band was recomputed exactly before 30d's number was compared to it"

requirements-completed: ["30d", "constraint-6"]

coverage:
  - id: D1
    description: "Post-close exit latency is archived as a multi-trial distribution measured through 30c's production-replica launcher"
    requirement: "30d"
    verification:
      - kind: experiment
        ref: "30d-evidence/mode-a/trial-1..5 — 169.5/215.5/242.0/276.0/279.7 ms, all exit code 0"
        status: pass
      - kind: script
        ref: "30d-exit-timing-harness.py recompute — min/median/max re-derived from published timings"
        status: pass
    human_judgment: false
  - id: D2
    description: "The close-with-pending-tasks case has an observed, recorded behavior across eleven independent fields"
    requirement: "30d"
    verification:
      - kind: experiment
        ref: "30d-evidence/mode-b/trial-1..2 — exit 42699.1/37059.5ms, 2 results after close, both children present, not truncated"
        status: pass
    human_judgment: true
  - id: D3
    description: "The observation window outlasts the slowest child's deadline plus a stated buffer, recorded per trial"
    requirement: "30d"
    verification:
      - kind: script
        ref: "90.0s window vs 52.0s floor; 63.1s and 62.3s measured past last dispatch; --window 51.9 aborts"
        status: pass
    human_judgment: false
  - id: D4
    description: "Every trial's process group is terminated and verified empty of descendants"
    requirement: "30d"
    verification:
      - kind: experiment
        ref: "7/7 trials record survivor_check_completed with zero survivors; interrupted trial reaped 2 out-of-group descendants"
        status: pass
      - kind: manual
        ref: "process listing after final trial — only the long-lived interactive session remains"
        status: pass
    human_judgment: false
  - id: D5
    description: "Archived evidence carries no home paths, usernames, session identifiers or credential-shaped tokens"
    requirement: "constraint-6"
    verification:
      - kind: scan
        ref: "scan_for_secrets over 29 in-scope files hunting 28 real session UUIDs -> 0 matches, staged captures matching as a live control"
        status: pass
    human_judgment: false
  - id: D6
    description: "crates/ is untouched"
    requirement: "30d"
    verification:
      - kind: manual
        ref: "git status --porcelain crates/ empty at every task boundary"
        status: pass
    human_judgment: false

duration: 45min
completed: 2026-08-02
status: complete
---

# Phase 30 Plan 04: 30d Exit Timing and Pending-Close Summary

**The 0.38s exit figure is refuted (169.5–279.7 ms, median 242.0 ms), closing stdin with pending tasks turns out to be benign rather than undefined, and constraint 8's ~12s idle-timeout floor is too low — 2 of 7 trials had a quiet gap above it.**

## The three results

### 1. Mode A — the 0.38s figure is NOT corroborated

| Trial | 1 | 2 | 3 | 4 | 5 |
|-------|---|---|---|---|---|
| CLI exit latency | 169.5 ms | 215.5 ms | 242.0 ms | 276.0 ms | 279.7 ms |

**min 169.5 · median 242.0 · max 279.7 ms · n=5 · spread 1.65x · all exit code 0**

0.38s sits **above the entire distribution** — 1.36x the maximum, 1.57x the
median. No trial came within 100 ms of it.

The qualitative claim survives ("the CLI exits promptly once stdin closes"); the
point estimate does not. Two mechanical reasons say the old number was measured
coarsely rather than that the CLI changed: v3's post-close loop had ≥100 ms
granularity (`run_experiment_v3.py:169-178`), and 30c's eight recorded values
are quantised to its own 0.25s poll — 0.25/0.5/0.51, three distinct values
across eight trials, which is a poll artifact and not a distribution. This
harness polls at 2 ms.

Recommended ROADMAP correction: replace `0.38s` with
`~0.24s (median of 5; 0.17–0.28s)`, citing `30d-evidence/mode-a/`.

### 2. Mode B — the undefined case is defined, and benign

**`mode_b_summary` is `exits_cleanly`** — and the body of `30d-MEASUREMENTS.md` says
explicitly that the per-trial fields are authoritative where they disagree with
it, because this token is badly incomplete on its own. It says nothing about the
**37–43 seconds of real work the CLI performed between the close and that clean
exit**, and a reader who stops at the token concludes the opposite of what
happened.

Both trials, independently: stdin closed with **two** outstanding `local_agent`
tasks, and then the CLI *kept going*. Both children completed and wrote their
signal files (14.8–26.0s after the close). The task set drained. **Two**
notification-origin `result` events arrived after the close. Nothing was
truncated. Exit code 0, unprompted.

| Field | Trial 1 | Trial 2 |
|---|---|---|
| `process_exited` / `exit_code` | true / 0 | true / 0 |
| `exit_latency_ms` | 42699.1 | 37059.5 |
| `results_after_close` | 2 | 2 |
| `final_result_truncated` | false | false |
| `drained_event_observed` | true (t+37.20) | true (t+38.38) |
| `child_a_signal_file` / `child_b_signal_file` | present / present | present / present |
| `stderr_nonempty` | false | false |
| `cleanup_action` | group reaped, survivor check completed, no survivors | same |
| `observation_window_s` | 90.002 | 90.036 |

### 3. Constraint 8 is revised upward — the number was wrong, the direction was right

| Longest quiet gap | 30c (7 trials) | 30d (7 trials) | Pooled max |
|---|---|---|---|
| **milestone events** (30c's definition) | 10.52 – 11.51 s | **7.70 – 13.73 s** | **13.73 s** |
| **every stream line** (what an idle timer sees) | not measurable | 6.02 – 7.09 s | 7.09 s |

**A 12-second idle timeout would have killed a live, healthy run in 2 of 7
trials** (13.638s and 13.728s). Before comparing, 30c's band was *recomputed*
from its published run logs (11.03, 10.77, 10.78, 10.52, 10.81, 11.28, 11.51) to
confirm the two units measure the same thing — 30c could only measure milestone
events, because its published logs carry no per-line timestamps.

Recommendation in `30d-MEASUREMENTS.md`: raise the floor to **≥30s**, which is
~2.2x the pooled observed maximum on the milestone definition and ~4x on the
every-line one.

Mode-A trial 1 shows the mechanism: it was a **coalesced** run (2 `result`
events, 1 notification-origin, both children delivered — F-1's signature), and
coalescing merges two turns into one, lengthening the quiet interval. Coalescing
occurred in 1 of 7 trials here, matching 30c's 1 in 7 exactly.

## What this means for Phase 31's close rule

Constraint 4's premise — that close-with-pending-tasks is undefined and must be
treated as unsafe — is retired. On this evidence the **drain gate is defensive,
not load-bearing**: a monitor that closed on its marker alone, children still
running, would still have received every completion.

That is reported plainly rather than suppressed for making a locked constraint
look less urgent. The recommendation is nonetheless to **keep the `AND` and
rewrite its justification**, for reasons that live in the data:

1. **n=2** establishes the benign path exists, not that it is the only one.
2. **The gate is nearly free** — the CLI outlived the close by ~40s anyway, so
   waiting for the drain costs the monitor essentially no wall-clock.
3. It removes a class of question about an undocumented, unpinned code path.

Two secondary consequences: a monitor must **not** read process exit as a
completion signal after an early close (it will block for tens of seconds
legitimately), and **999.64's failure mode is not reproduced by this mechanism**
— if Phase 31 loses child work, early stdin close is not the cause.

## Review findings — all four incorporated

| Finding | How it landed |
|---|---|
| **M1 — reuse 30c's launcher** | `launch_in_monitor_env` imported by path; all 7 trial logs record `launcher_source: 30c-monitor-env-harness.py`, the same 18-name parsed scrub list, and `removed_variables: GIT_DIR, GIT_WORK_TREE`. Verified the abort path by `chmod 000` on 30c: exit 2, no fallback. |
| **M2 — `mode_b_outcome` not a single token** | Eleven independent fields per trial; `mode_b_summary` present but explicitly secondary, with the body stating the fields win. |
| **M3 — bounded observation window** | Floor of 52.0s (22s deadline + 30s buffer), enforced by an abort (`--window 51.9` refuses). Observation ran 63.1s / 62.3s past the last dispatch. `absent` vs `absent_at_window_close` are separate classifications so a short window can never produce the strong claim. |
| **M4 — verified descendant reaping** | Running census unions group membership with a descendant walk; post-kill survivor re-check recorded per trial. **This caught a real leak** — see below. |

### The descendant walk was not redundant

In the deliberately interrupted trial, cleanup recorded:

```
killpg(1040458, SIGTERM); kill(1041393, SIGTERM) [out of group];
kill(1041395, SIGTERM) [out of group]; SIGKILL: skipped, group already empty
```

**Two descendants had left the process group.** `killpg` alone would have
orphaned them — exactly the leak M4 predicted and exactly the shape backlog
999.46 tracks. The pre-interrupt tree showed `sh`, `claude` and an
`npm exec @model...` grandchild; the post-interrupt listing showed the group
empty.

## Task Commits

1. **Task 1** — `4301f51` (feat): the harness (1,310 lines) — imported launcher with abort, two modes, windowed floor, verified reap
2. **Fix** — `3112a1b` (fix): count post-close results instead of timestamp-comparing them
3. **Fix** — `7e7877c` (fix): attribute `local_bash` notifications by task_id correlation
4. **Task 2** — `fb0bf2b` (feat): 7 archived trials + `30d-MEASUREMENTS.md`
5. **Close** — this commit: SUMMARY

## Deviations from Plan

### 1. [Rule 1 — Bug] `results_after_close` counted the result that triggered the close

Mode B's close is triggered **by** a `result` event, so that result's read-time
and the close instant are identical to within the 3-decimal rounding stored in
`timings.json`. The `> close_at` comparison counted the trigger itself: trial 1
reported **3** results after close when 2 arrived after it. Fixed by
snapshotting the count at the close instant and subtracting (`3112a1b`). Mode A
was unaffected — its close follows a 15s quiet settle, and all five trials
correctly reported 0.

Caught by reading the per-trial fields against the run log rather than trusting
the harness's own output. This is one of the eleven fields the review required
be independently observable; had it shipped, `30d-MEASUREMENTS.md` would have
claimed a post-close delivery that never happened.

### 2. [Rule 1 — Bug] `local_bash` notifications were silently recorded as none

Only `task_started` carries `task_type`; `task_notification` and `task_updated`
do not. Filtering those on a `task_type` field therefore matched nothing — the
exact under-recording that RESEARCH.md's assumption A2 needs evidence against.
Fixed by correlating on the task_id first seen in a `local_bash` `task_started`
(`7e7877c`), and by recording whether each notification carried `usage`, which
is Pitfall 6's stated discriminator.

Result: **A2 now has evidence.** All 7 trials show 4 `local_bash` events (2
`task_started` + 2 `task_notification`), zero `local_bash` entries ever in a
`background_tasks_changed` array, and `has_usage` false for every `local_bash`
notification and true for every `local_agent` one — the pattern
`[false, true, false, true]` in every trial without exception.

### 3. [Method] Discarded a complete trial set and re-ran all seven

Both fixes above landed **after** a full set of 5 Mode A + 2 Mode B trials had
already been archived. Rather than publish evidence produced by two different
harness versions with a per-field asterisk, the whole set was deleted and re-run
against the committed harness. Costs ~10 minutes of CLI budget; buys the literal
truth of "reproducible from the archived harness plus its recorded invocation",
which is this plan's entire point.

### 4. [Method] Two quiet-gap definitions recorded, not one

The plan asked for reconciliation against constraint 8. Constraint 8's number
turned out to be computed over a *milestone* event subset, because 30c's
published logs carry no per-line timestamps. Reporting a whole-stream gap
against it would have compared two different measures and manufactured a false
refinement. Both are now recorded per trial, and 30c's band was recomputed from
its own artifacts to prove the definitions line up.

### 5. [Scope] Agent-session markers deliberately left unscrubbed

`--scrub-agent-markers` defaults **off**, against 30c's reliability-set
configuration, following 30-02's finding F-2: scrubbing them diverges from
production, which carries `ANTHROPIC_API_KEY` from the operator's global config.
This also matches v3's marker condition, which is the baseline the 0.38s figure
came from — making the comparison valid rather than confounded. Recorded as a
limit either way.

## Issues Encountered

- **A concurrent session's blanket `git add` committed my superseded, in-progress
  evidence.** Commit `7e2a694` ("chore(planning): move phase 23's superseded
  plans out of the phase tree") swept up all 21 files of the discarded first
  trial set. My `fb0bf2b` corrected every one of them, and HEAD now matches the
  verified on-disk evidence exactly (re-derived from `git show HEAD:` after the
  fact, not assumed). The wart is provenance only: the superseded set exists in
  history under an unrelated message. 30-02 hit the same class of problem.
- **My first interrupt test did not interrupt anything.** Backgrounding the
  harness in a non-interactive shell makes POSIX set SIGINT to `SIG_IGN` for the
  job, so the trial ran to completion and the "group is empty afterward"
  observation proved only that the *normal* path reaps cleanly. The second
  attempt's `pkill -f` then matched the wrapper shell as well and killed the
  test harness itself. Only the third — foreground process, `pkill` anchored to
  `^python3` — actually tested the interrupt path. Had I stopped at the first,
  I would have reported an acceptance criterion as met on evidence that did not
  test it.
- **Three numbers in the first draft of `30d-MEASUREMENTS.md` were carried over
  from the superseded run** (two result-character counts and an event-count
  range). Caught by a scripted cross-check of every documented figure against
  the archived timings before commit, not by re-reading. This is precisely the
  failure mode — a remembered figure surviving into a document — that this plan
  exists to fix in the 0.38s case.
- **Scanner false positive worth knowing:** `scan_for_secrets` matches
  `credential_named_assignment` on the harness's own
  `except KeyboardInterrupt:` line, because the pattern is case-insensitive and
  `KeyboardInterrupt` contains `Key`. Documented in `30d-MEASUREMENTS.md` so
  nobody later "fixes" a Python keyword. Out of the acceptance scope
  (`30d-evidence/` + `30d-MEASUREMENTS.md`), which scanned 29 files for 0
  matches.

## Must-Haves Verified

| Truth | Status | Evidence |
|---|---|---|
| Exit latency measured across multiple trials and archived on disk | met | `30d-evidence/mode-a/trial-1..5`, recomputable via `recompute` |
| Every trial launched through 30-02's production-replica launcher | met | 7/7 run logs: `launcher_source`, 18-name parsed scrub list, `removed_variables: GIT_DIR, GIT_WORK_TREE`, distinct stdout/stderr paths |
| Close-with-pending-tasks has an observed, recorded behavior | met | 2 trials, eleven fields each, both `exits_cleanly` with 2 post-close results |
| Mode B observations independent, not one token | met | Eleven separate frontmatter fields; `mode_b_summary` explicitly secondary |
| Window outlasts the 22s deadline plus a stated buffer | met | 90.0s window / 52.0s floor; 63.1s and 62.3s past last dispatch; `--window 51.9` aborts |
| Every process group terminated and verified empty | met | 7/7 `survivor_check_completed`, 0 survivors; interrupted trial reaped 2 out-of-group descendants |
| Every measurement reproducible from harness + recorded invocation | met | Each `run.log` carries its own `invocation:` line verbatim |
| No home paths, usernames, session ids or credentials in evidence | met | 29 in-scope files, 0 matches, hunting 28 real session UUIDs; staged captures still match as control |

## Limits worth carrying forward

n=5 for Mode A and **n=2 for Mode B** — the exact one-sided 95% bound on 2/2 is
0.224, so the benign outcome is established only as above ~22%. One machine, one
CLI version (2.1.220), ten minutes. Load average sat at ~5 throughout (browser,
plus a sibling plan's `cargo test --workspace` which was waited out before the
first measured trial), and sub-second latency is the measurement most exposed to
that. Both children are trivial sleeps — a child still *streaming* when stdin
closes is untested. Mode B's close point is one specific early moment.

## Next Phase Readiness

Phase 31 inherits three concrete changes from this plan:

- **The 0.38s figure should be corrected in the ROADMAP** to ~0.24s (median of 5).
- **Constraint 8's floor should be raised** from ~12s to ≥30s; 12s would have
  killed 2 of 7 healthy runs here.
- **Constraint 4's justification should be rewritten** — keep the `AND`, but on
  "measured benign at n=2, and the gate costs nothing because the CLI outlives
  the close regardless", not on "undefined, treat as unsafe".

`30-05` and any remaining plans are unaffected by this one; it produces evidence,
not code with a consumer.

## Self-Check: PASSED

Verified on disk this session, not recalled:

- All 4 task commits present in `git log`; `git status --porcelain` clean.
- `git status --porcelain crates/` empty at every task boundary.
- Mode A aggregates re-derived from **committed** blobs via `git show HEAD:`:
  `[169.5, 215.5, 242.0, 276.0, 279.7]`, min/median/max `169.5 / 242.0 / 279.7`
  — matching the frontmatter.
- Every Mode B frontmatter field checked field-by-field against the committed
  `timings.json`; all 9 asserted values matched per trial, both windows ≥52s.
- 3 mismatches found and corrected before commit (789/718 result chars, 50–57
  event range, the "25 minutes" claim → measured 17:02:58–17:10:32).
- Arithmetic recomputed, not estimated: `380/279.7=1.36`, `380/242=1.57`,
  `279.7/169.5=1.65`, `0.05**0.5=0.224`.
- `run_experiment_v3.py` lines 169 and 178 read directly to confirm the
  granularity claim (`while time.time() < deadline2:` / `time.sleep(0.1)`).
- 30c's quiet-gap band recomputed from its 7 published run logs before
  comparison: 10.52–11.51, reproducing its verdict exactly.
- Secret scan re-run over the 29 in-scope files with 28 real session UUIDs
  harvested from staged captures: 0 matches, control still matching.
- Process listing after the final trial: the only `claude` process is the
  long-lived interactive session, pgid unrelated to any trial.

---
*Phase: 30-keep-the-session-alive-past-turn-end*
*Completed: 2026-08-02*
