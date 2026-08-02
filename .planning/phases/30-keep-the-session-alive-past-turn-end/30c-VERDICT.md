---
unit: 30c
delivery: confirmed
claude_code_version: 2.1.220
result_events: 3
task_notification_origin_results: 2
children_completed: 2
task_set_drained: true
partial_delivery: false
unparseable_lines: 0
run_date: 2026-08-02
evidence:
  - 30c-evidence/raw_output.jsonl
  - 30c-evidence/stderr.log
  - 30c-evidence/run.log
harness: 30c-monitor-env-harness.py
---

# 30c — Does `task-notification` delivery survive DevFlow's production launch environment?

**Yes, on a single trial.** Both delegated children were independently delivered
into a live CLI session running inside a replica of `spawn_monitor`'s process
environment — detached in its own session, launched through `sh -c`, with the
`git.rs` env scrub applied, no TTY on any stream, and stderr routed to its own
file. Each completion produced its own top-level `result` event carrying
`origin.kind == "task-notification"`, and the background task set drained to an
empty array. Both arms of the both-children bar are satisfied, not just one.

This verdict was derived by re-reading the published capture, not from anything
the harness printed. The harness prints no verdict by design: 30a's README
records that the harness-printed verdict was wrong in v1 and unreliable in v2.

## Result events

Read from `30c-evidence/raw_output.jsonl`. Line numbers are 1-based into that
file; offsets come from the archived `run.log` timeline.

| # | Line | Offset from launch | `origin` verbatim | `num_turns` | `session_id` |
|---|------|--------------------|-------------------|-------------|--------------|
| 1 | 20 | t+9.17s | *absent* (no `origin` key) | 3 | `<session-01>` |
| 2 | 40 | t+31.71s | `{"kind": "task-notification"}` | 1 | `<session-01>` |
| 3 | 55 | t+44.23s | `{"kind": "task-notification"}` | 2 | `<session-01>` |

All three `system`/`init` events (lines 5, 34, 50) carry the same pseudonym
`<session-01>`, so the three turns ran in **one** session — the same fact the v3
baseline established, preserved through redaction by mapping equal inputs to
equal placeholders.

## Per-child correlation

Each notification-origin `result` is tied to a distinct child by the `task_id`
of the `task_notification` event immediately preceding it, and independently by
the result text naming that child.

| Child | `task_id` | Spawned (`background_tasks_changed`) | Notification | Signal file verified on disk | Result event triggered |
|-------|-----------|--------------------------------------|--------------|------------------------------|------------------------|
| A (10s sleep) | `ad352bd7c6e64148b` | line 8, t+6.27s | line 33, t+26.71s, `status: completed` | yes — `signalA_30c.txt`, contents exactly `CHILD-A-DONE` | #2 (line 40), text names "Subagent A" |
| B (22s sleep) | `a8065ce3edccc06bf` | line 12, t+7.27s | line 49, t+37.22s, `status: completed` | yes — `signalB_30c.txt`, contents exactly `CHILD-B-DONE` | #3 (line 55), text reports both complete |

`background_tasks_changed` progression: `[A]` (line 8) → `[A, B]` (line 12) →
`[B]` (line 31, A drained) → `[]` (line 47, both drained). The empty array at
line 47 is the second, independent proof that neither child was silently
dropped.

Two `task_notification` events with short ids and empty `output_file` (lines 25
and 41, ids `bjb9jjb8j` and `b6db7x15z`) are inner sub-agent notifications, not
the delegated children; they do not correlate to a `background_tasks_changed`
entry. The same pair-shape appears in the v3 baseline (lines 26 and 38). They
are noted so a reader recounting `task_notification` events does not read four
notifications as four children.

## Confirmation bar

The bar was strengthened during replanning because "at least one notification"
would let one delivered child and one lost child read as confirmed — exactly
the Phase 29 wave-2 shape this arc exists to fix. All three conditions hold
here:

| Condition | Required | Observed |
|-----------|----------|----------|
| Both children ran to completion, evidenced by signal files on disk | 2 | 2, contents exact |
| Notification-origin `result` events, each correlatable to a distinct child | 2 | 2 (lines 40, 55; ids `ad352…`, `a8065…`) |
| *or* one such result plus an observed empty `background_tasks_changed.tasks` | either arm | **both** arms satisfied (drain at line 47) |

`partial_delivery: false` — no child was delivered without the other.

## Side-by-side against the 30a interactive baseline

`30a-evidence/raw_output_v3.jsonl` is the same experiment (same prompt, same two
concurrent children at 10s/22s, same CLI version) run inside an interactive
Claude Code session. The two captures are structurally isomorphic.

| Fact | 30a v3 (interactive) | 30c (production replica) |
|------|----------------------|--------------------------|
| Capture lines | 54 | 55 |
| `result` events | 3 (lines 19, 37, 54) | 3 (lines 20, 40, 55) |
| First `result` `origin` | absent | absent |
| Later `result` origins | both `task-notification` | both `task-notification` |
| `task_notification` events | 4 (26, 31, 38, 46) | 4 (25, 33, 41, 49) |
| `background_tasks_changed` | 4 (8, 13 filling; 29 one drained; 44 empty) | 4 (8, 12 filling; 31 one drained; 47 empty) |
| `init` events sharing one `session_id` | 3 | 3 |
| Children completing | 2 | 2 |
| Exit delay after stdin close | 0.38s (harness stdout, **unarchived**) | **0.25s, archived** in `run.log` |
| Unparseable lines | 0 | 0 |

The archived exit timing also closes review finding M1, which flagged the v3
figure as measured but never written down.

### What differed between the two runs

| Characteristic | 30a v3 | 30c | Source in production |
|----------------|--------|-----|----------------------|
| Launch | direct `claude` exec from Python | `sh -c` script with production's trap / cd / background-and-wait / pid+exit files | `monitor.rs:134-174` |
| Process group | inherited from the harness | detached, own session leader (verified: harness sid ≠ child sid) | `start_new_session` equivalent of a detached monitor |
| stdout capture | Python pipe | file written by the `sh` redirection, tailed back | `monitor.rs:138` |
| stderr | **merged into stdout** (`stderr=STDOUT`) | separate file, `2>`; 0 bytes produced | `monitor.rs:138` |
| Env scrub | none applied | `git.rs` list applied (see caveat below) | `git.rs:87-94` |
| Workdir | a scratchpad directory | the repository root | production passes the project root |
| stdin | held open by the harness | held open by the harness | **deliberate deviation** — `monitor.rs:171` uses `Stdio::null()` |

Holding stdin open is constant across both runs: it is the enabling condition
for the measurement, not the variable under test. The environment is the
variable.

## Residual environment

The claim is deliberately not "this differs from production in exactly one
variable". What the harness can actually deliver is recorded here so a reader
can check it rather than take it.

**Applied scrub, and its honest caveat.** The harness parsed 18 variable names
out of `crates/devflow-core/src/git.rs` at runtime (15 from `REPO_LOCAL_GIT_VARS`,
3 from `ALSO_REDIRECTING_GIT_VARS`) and removed every one that was present.
**Zero were present**, so on this run the scrub was a no-op — `run.log` records
`removed_variables: (none were set)`. That matches production, where a normal
operator shell also carries none of them, but it means "scrubbed environment"
was not itself a difference from the interactive baseline on this trial. The
scrub *mechanism* is nonetheless proven: a separate runtime check planted
`GIT_DIR` and `GIT_WORK_TREE` in the parent environment and confirmed both were
absent in the child.

**Ancestry the scrub cannot remove.** `start_new_session` detaches a process; it
does not sanitise inherited environment or erase ancestry. The child inherited
150 environment variables, including these Claude-session markers, recorded by
name and never by value:

`AI_AGENT`, `CLAUDECODE`, `CLAUDE_CODE_BRIDGE_SESSION_ID`,
`CLAUDE_CODE_CHILD_SESSION`, `CLAUDE_CODE_ENTRYPOINT`, `CLAUDE_CODE_EXECPATH`,
`CLAUDE_CODE_SESSION_ID`, `CLAUDE_EFFORT`, `CLAUDE_PID`.

Eleven further variables carry credential-shaped names; their names are withheld
per T-30-07 and only the count is recorded. The parent process was `zsh`, itself
a descendant of an interactive `claude` process, so
`launched_from_agent_session` is `true`.

**Read against operator-launched DevFlow.** In production the operator runs
`devflow` from a plain shell, so none of those `CLAUDE_*` / `AI_AGENT` markers
would be set, and the parent would be the `devflow` binary rather than a shell
under an agent session. This run therefore does **not** establish delivery under
an environment with those markers absent.

What it does establish is narrower and still decisive: the markers were present
in the 30a baseline too, so they are held **constant** across the comparison
rather than varying with it. The five characteristics that did change —
`sh -c` launch, detachment, file-based capture, separated stderr, applied scrub
— did not break delivery. If a `CLAUDE_*` marker were the load-bearing reason
delivery works, this experiment would not detect it. That specific gap is named
in the limits below rather than argued away.

## Limits of this evidence

- **n=1.** One trial, one machine, one CLI version. The three 30a captures were
  each single trials too, so this is the same evidential standard the phase has
  used throughout — but it is a standard, not a proof of reliability. A
  delivery mechanism that works 3 times out of 4 would look identical to this
  run.
- **The Claude-session marker variables were not removed** (see above). The
  cheapest follow-up that would close this is a second run of the same harness
  with those names added to the scrub list; it was not run here because the plan
  specifies one run against the `git.rs` list, and inventing a second scrub list
  would have measured something production does not do.
- **The workdir was the repository root, not a worktree.** Production may run
  the monitor from a `git worktree` whose `.git` is a file. Nothing in the
  observed mechanism appears workdir-sensitive, but that is an inference, not a
  measurement.
- **Timing is not load.** Both children were trivial sleeps. This says nothing
  about delivery when children are long-running agents producing heavy output,
  which is the Phase 29 wave-2 shape in every respect except duration.
- **The mechanism remains undocumented and unpinned** (review finding M2). The
  `origin.kind == "task-notification"` contract is observed behavior of CLI
  `2.1.220`, not a published interface, and the harness hard-fails on any other
  version precisely so a future run cannot silently measure a different CLI.
- **stderr was empty.** Zero bytes were produced, so the separated-stderr change
  is proven wired but was not exercised by actual error output.

## Evidence integrity

Raw stdout and stderr were written by the `sh` script's own redirection into a
run-scoped `tempfile.mkdtemp` directory outside the repository
(`/tmp/devflow-30c-19cxeek9`, recorded in `run.log`). Nothing reached
`30c-evidence/` except through validate → structural redact → secret-scan →
atomic `os.replace`.

That ordering was not ceremonial. Scanning the **staged** raw capture matched
`home_path`, `os_username` and `session_identifier`; scanning each **published**
file matched nothing:

| Scanned artifact | Scanner result |
|------------------|----------------|
| staged raw stdout (never committed) | `home_path`, `os_username`, `session_identifier` |
| `30c-evidence/raw_output.jsonl` | no match |
| `30c-evidence/run.log` | no match |
| `30c-evidence/stderr.log` | no match |

The scanner covers home paths, OS username, session identifiers seen in the
staged capture, and four credential shapes (`openai_key_prefix`,
`github_token_prefix`, `bearer_token`, `credential_named_assignment`). It
reports which pattern matched, never the matched value.

`git status --porcelain crates/` is empty: `monitor.rs` and `git.rs` were read,
never written.

## What this unblocks

Per the ROADMAP's locked decision, `delivery: confirmed` means **Phase 31 may be
planned**. Its acceptance criterion remains the live Phase 29 wave-2 re-run,
which review constraint H4 states is not substitutable by integration tests —
and the limits above are exactly why: this experiment proves the mechanism
exists in the production environment, not that it is reliable under load.

Because the verdict is `confirmed`, no `## Rejected options` table is required
and the conditional stop on plans 30-03 / 30-05 does not trigger — both were
conditioned on a refutation.
