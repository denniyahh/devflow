---
unit: 30c
trial: 2
arm: agent-session-markers-scrubbed
delivery: confirmed
agrees_with_trial_1: true
claude_code_version: 2.1.220
result_events: 2
task_notification_origin_results: 1
children_completed: 2
task_set_drained: true
partial_delivery: false
unparseable_lines: 0
agent_session_markers_removed: 11
residual_agent_session_markers: 0
run_date: 2026-08-02
evidence:
  - 30c-evidence-scrubbed/raw_output.jsonl
  - 30c-evidence-scrubbed/stderr.log
  - 30c-evidence-scrubbed/run.log
companion: 30c-VERDICT.md
harness: 30c-monitor-env-harness.py --scrub-agent-markers
---

# 30c trial 2 — delivery with the agent-session markers removed

**The two trials agree: `confirmed` both times. The nested-session hypothesis
is retired.** Trial 1 ran with nine `CLAUDE_*`/`AI_AGENT` markers inherited
from the surrounding Claude Code session, leaving open the possibility that it
had proven delivery works *inside an agent session* rather than in production's
plain-shell environment. Trial 2 removed all eleven `CLAUDE*`/`ANTHROPIC*`/
`AI_AGENT*` variables — `run.log` records `claude_session_markers: (none)` and
`launched_from_agent_session: False` — and both delegated children were still
delivered.

This verdict is a **companion to `30c-VERDICT.md`, not a replacement.** Trial 1's
evidence under `30c-evidence/` is untouched and remains the comparison arm that
makes this trial interpretable.

## The one thing that differs, and it is not the verdict

Trial 2 produced **one** notification-origin `result` event where trial 1
produced two — and both children were delivered in both cases. The CLI
**coalesced** the two completions into a single resumed turn rather than
resuming twice.

The timing explains it. In trial 1 the first resume finished (t+31.71) before
child B's notification arrived (t+37.22), forcing a second resume. In trial 2
both notifications landed 4.75s apart (t+30.62 and t+35.37) while the
orchestrator was still working, so one turn absorbed both.

The resumed turn says so itself, unprompted:

> "Both subagents completed and both notifications were delivered to this
> orchestrator turn. […] Notification order matched completion order: A (~24s)
> then B (~28s), each as a separate task-notification after my turn ended on
> `ORCHESTRATOR-TURN-1-COMPLETE`."

**This is a finding Phase 31 must not ignore: the count of
`task-notification`-origin `result` events is NOT a count of delivered
children.** A monitor that resumes work by counting result events would
undercount a wave whose completions arrive close together. Delivery is
per-notification; turns are batched.

It also vindicates the strengthened confirmation bar's second arm. Trial 2's
shape — one notification-origin result — is exactly the shape the review
warned could mask "one delivered, one lost". The drain to an empty task set is
what distinguishes the two: with a child genuinely lost,
`background_tasks_changed.tasks` would not have reached `[]`. Both arms were
needed, and here the second one carried the verdict.

## Result events

Line numbers are 1-based into `30c-evidence-scrubbed/raw_output.jsonl`; offsets
come from that trial's `run.log`.

| # | Line | Offset | `origin` verbatim | `num_turns` | Text |
|---|------|--------|-------------------|-------------|------|
| 1 | 20 | t+9.05s | *absent* (no `origin` key) | 3 | `ORCHESTRATOR-TURN-1-COMPLETE` |
| 2 | 56 | t+46.88s | `{"kind": "task-notification"}` | 3 | acknowledges **both** children, verified on disk |

Two `system`/`init` events (lines 5 and 36), both `<session-01>` — one session,
one resume, consistent with the coalescing above (trial 1 had three inits for
two resumes).

## Per-child correlation

| Child | `task_id` | Spawned | Notification | Signal file verified | Result event |
|-------|-----------|---------|--------------|----------------------|--------------|
| A (`Signal A after 10s`) | `a6c5ea07c91bf7de8` | line 8, t+6.29s | line 35, t+30.62s, `status: completed` | yes — `signalA_30c.txt`, `CHILD-A-DONE` | #2 (line 56) |
| B (`Signal B after 22s`) | `afc6448ef381a60d3` | line 12, t+7.54s | line 44, t+35.37s, `status: completed` | yes — `signalB_30c.txt`, `CHILD-B-DONE` | #2 (line 56) |

`background_tasks_changed`: `[A]` (line 8) → `[A, B]` (line 12) → `[B]` (line 33,
A drained) → `[]` (line 42, both drained).

Delivery of both children is established three independent ways: both
`task_notification` events are present with distinct `task_id`s matching the two
spawned tasks; the task set drained to empty; and the resumed turn's own text
names both and reports verifying both files on disk.

Lines 26 and 37 (`bix3mwo0f`, `bngnh2859`, empty `output_file`) are inner
sub-agent notifications, not the delegated children — the same pair-shape seen
in trial 1 and in the 30a baseline.

## Confirmation bar — identical to trial 1

| Condition | Required | Observed |
|-----------|----------|----------|
| Both children ran to completion, signal files on disk | 2 | 2, contents exact |
| Two notification-origin results, each correlated to a distinct child | arm 1 | **not met** — 1 result (coalesced) |
| *or* one such result **plus** an empty `background_tasks_changed.tasks` | arm 2 | **met** — result at line 56 + drain at line 42 |

`children_completed` (2) equals the number dispatched (2), and arm 2 is
satisfied, so `delivery: confirmed`.

`partial_delivery: false`. The plan defines it as "at least one but not all
children were delivered". Both were delivered; only the *turns* were merged. A
one-of-two outcome would show a missing `task_notification`, a task set that
never drained, and a result text naming one child — none of which is present.

## Side by side

| Fact | Trial 1 — `30c-evidence/` | Trial 2 — `30c-evidence-scrubbed/` |
|------|---------------------------|------------------------------------|
| Agent-session markers in child env | 9 present (+2 credential-named) | **0** |
| `launched_from_agent_session` | `True` | **`False`** |
| Total env vars in child | 150 | 139 |
| Markers removed | 0 | 11 |
| Git-scrub vars removed | 0 (none were set) | 0 (none were set) |
| Capture lines | 55 | 56 |
| `result` events | 3 | 2 |
| Notification-origin results | 2 | 1 |
| `init` events / resumes | 3 | 2 |
| `task_notification` events | 4 (2 children + 2 inner) | 4 (2 children + 2 inner) |
| Task set drained to `[]` | yes (line 47) | yes (line 42) |
| Children completed | 2 | 2 |
| Exit delay after stdin close | 0.25s | 0.50s |
| `sh` exit code | 0 | 0 |
| Unparseable lines | 0 | 0 |
| **Verdict** | **`confirmed`** | **`confirmed`** |

**They do not diverge.** Delivery survives with the markers present and with
them absent, so the trial-1 `confirmed` was not an artifact of the agent-session
environment.

## Authentication note — requirement not to break auth

`ANTHROPIC_API_KEY` and `ANTHROPIC_TOKEN` were removed along with the other nine.
That intersects the instruction not to scrub credentials the CLI needs, so it
was decided on evidence rather than assumption: the `init` events of **both**
trial 1 and the 30a v3 baseline report `apiKeySource: "none"`, meaning the CLI
authenticates from stored credentials and was already ignoring those variables.

A cheap probe confirmed it before the measurement was spent: launching the CLI
through the same replica with all eleven removed returned `PROBE-OK`, exit code
0, and `apiKeySource: "none"`. The full trial then ran normally. No
authentication failure occurred, so there is nothing to report under that
heading — and nothing was quietly restored.

> **CORRECTION (2026-08-02, from the operator's plain-shell trial,
> `30c-evidence-operator/`).** The inference above is over-read by one step and
> is corrected here rather than restated.
>
> `apiKeySource: "none"` establishes that the CLI does not **authenticate** from
> `ANTHROPIC_API_KEY`. It does **not** establish that the variable is inert. The
> operator's trial kept it — their mise config sets it globally, so production
> genuinely carries it — and its stderr shows a warning the scrubbed trials
> never produced:
>
> `⚠ claude.ai connectors are disabled because ANTHROPIC_API_KEY or another auth
> source is set and takes precedence over your claude.ai login`
>
> So trials 2 through 7 **over-scrubbed**: they removed a variable production
> carries, and ran with connectors loaded where production runs with them
> disabled. Delivery succeeded in both conditions — 6 scrubbed trials and the
> operator's unscrubbed one — so the verdict is unaffected. The defective part
> was the reasoning, not the result: a verified check (`apiKeySource`) was
> extended to a claim it does not support (that the CLI "was already ignoring"
> the variable).

## Residual environment

After the scrub the child carried **139** variables and **zero**
`CLAUDE*`/`ANTHROPIC*`/`AI_AGENT*` names. Nine credential-named variables
unrelated to Claude remain (count only, names withheld per T-30-07).

What still cannot be replicated is ancestry: the parent process is `zsh`,
descended from an interactive `claude` process. Process ancestry and any
non-environment channel it might carry are untouched by an environment scrub.
`launched_from_agent_session` reads `False` because the marker variables are
gone, which is the honest reading of that field — it means "no marker variables
present", not "provably not descended from an agent".

## Limits of this evidence

All of `30c-VERDICT.md`'s limits carry over, plus:

- **Still n=1 per arm.** Two trials in two environments, not two trials in one.
  Neither arm has a repeat.
- **The two trials are not otherwise identical.** They ran minutes apart with
  independent model scheduling; the coalescing difference is a timing outcome,
  not something the environment change caused. Nothing here isolates *why* the
  turn structure differed, only that the verdict did not depend on it.
- **The read window closed at t+60 by deadline, not by the completion bar.**
  The harness's stop condition expects three `result` events (trial 1's shape),
  and trial 2 legitimately produced two, so it ran to the deadline. Nothing was
  truncated: the task set was already empty at t+35.37, the final result arrived
  at t+46.88, and the process exited cleanly 0.5s after stdin close. The
  deadline exit is a harness-expectation artifact, not a lost event.
- **Ancestry is not environment.** See above.

## Evidence integrity

Same discipline as trial 1, not a shortcut around it. Raw output staged in
`/tmp/devflow-30c-1m8ww51z` (outside the repository, recorded in `run.log`),
then validate → structural redact → secret-scan → atomic replace.

| Scanned artifact | Scanner result |
|------------------|----------------|
| staged trial-2 raw stdout (never committed) | `home_path`, `os_username`, `session_identifier` |
| `30c-evidence-scrubbed/raw_output.jsonl` | no match |
| `30c-evidence-scrubbed/run.log` | no match |
| `30c-evidence-scrubbed/stderr.log` | no match |

Trial 1's four committed files are byte-identical to their committed state
(`git status --porcelain` over `30c-evidence/` and `30c-VERDICT.md` is empty),
and `git status --porcelain crates/` is empty.
