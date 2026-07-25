# 23-PROBE-FINDINGS: One Unattended `devflow start --phase 1` Run, Observed to Termination

Produced by `23-02-PLAN.md` Task 1. Destination: a fresh scratch repo scaffolded
by `scripts/scratch-dogfood-repo.sh` at `/tmp/devflow-probe-23a-20260725-171553`
(outside this checkout; T-23-05 is satisfied — see "Source safety" below).

Launch, exactly as specified: `devflow start --phase 1 --agent claude --mode auto <dest>`.

## Stage reached: parked at a pending human gate in **VALIDATE**

The run did not complete. It progressed through all five stages at least once
(reaching **Ship** once), looped back from Ship to Code on a review failure,
then cycled Code→Validate three more times, and parked at the Validate
retry-exhaustion gate (`"Validation failed 3 time(s) — human review needed."`).
The last event written to `.devflow/events.jsonl` was `notify_fired` at
`ts:1785016861` (2026-07-25T22:01:01Z).

`.devflow/phase-01-exit` contains exactly:

```
0
```

`.devflow/phase-01-agent-pid` contains exactly:

```
2153936
```

That PID (the last stage's `claude` agent process) is **not running** —
confirmed read-only, post-observation, via `ps -o pid,etimes,cmd -p 2153936`,
which returned no matching process. This capture was taken *after* the
observation window closed, purely to record process facts for this document;
no process was signalled or killed at any point during or after observation.

## Timeline (wall clock, from `events.jsonl` timestamps)

| Time (UTC) | Event |
|---|---|
| 21:16:11 | `workflow_started` |
| 21:16:11 | `stage_launched` [define] |
| 21:18:27 | `advance_evaluated` [define] success → `transition` → plan |
| 21:18:43 | `advance_evaluated` [plan] success → `transition` → code |
| 21:28:02 | `advance_evaluated` [code] success → `transition` → validate |
| 21:29:58 | `advance_evaluated` [validate] success → `hook_run` (DocsUpdate) → `transition` → **ship** |
| 21:39:55 | `advance_evaluated` [ship] **FAILED** → `loop_back` (consecutive_failures: 0) → code |
| 21:45:10 | `advance_evaluated` [code] success → `transition` → validate |
| 21:48:58 | `advance_evaluated` [validate] success (verdict: gaps) → `loop_back` (consecutive_failures: 1) → code |
| 21:52:14 | `advance_evaluated` [code] success → `transition` → validate |
| 21:55:30 | `advance_evaluated` [validate] success (verdict: gaps) → `loop_back` (consecutive_failures: 2) → code |
| 21:57:22 | `advance_evaluated` [code] success → `transition` → validate |
| 22:01:01 | `advance_evaluated` [validate] success (verdict: gaps) → `gate_fired` [validate] → `notify_fired` |
| (silence) | no further events observed through 22:13Z (>12 min) |

Total elapsed, launch to gate: 44m50s. Total elapsed, launch to end of
observation: ~57m.

## Verbatim `events.jsonl` (all 46 lines, complete stream)

```json
{"agent":"claude","commit":"2228222ad774c9f44fd6917a98728e01f075b1d1","dirty":"true","event":"workflow_started","exe_path":"devflow","mode":"auto","phase":1,"ts":1785014171,"v":1,"version":"1.8.1","worktree":"/tmp/devflow-probe-23a-20260725-171553/.worktrees/phase-01"}
{"agent":"claude","event":"stage_launched","monitor_pid":2036918,"phase":1,"stage":"define","ts":1785014171,"v":1}
{"decided_by_layer":null,"event":"advance_evaluated","phase":1,"reason":null,"stage":"define","status":"success","ts":1785014307,"v":1,"verdict":null}
{"event":"transition","from":"define","phase":1,"to":"plan","ts":1785014307,"v":1}
{"event":"capture_archived","phase":1,"stage":"define","stamp":"1785014307134712171-0","to_stage":"plan","ts":1785014307,"v":1}
{"agent":"claude","event":"stage_launched","monitor_pid":2045319,"phase":1,"stage":"plan","ts":1785014307,"v":1}
{"decided_by_layer":null,"event":"advance_evaluated","phase":1,"reason":null,"stage":"plan","status":"success","ts":1785014323,"v":1,"verdict":null}
{"event":"transition","from":"plan","phase":1,"to":"code","ts":1785014323,"v":1}
{"event":"capture_archived","phase":1,"stage":"plan","stamp":"1785014323639410396-0","to_stage":"code","ts":1785014323,"v":1}
{"agent":"claude","event":"stage_launched","monitor_pid":2046497,"phase":1,"stage":"code","ts":1785014323,"v":1}
{"decided_by_layer":null,"event":"advance_evaluated","phase":1,"reason":null,"stage":"code","status":"success","ts":1785014882,"v":1,"verdict":null}
{"event":"transition","from":"code","phase":1,"to":"validate","ts":1785014882,"v":1}
{"event":"capture_archived","phase":1,"stage":"code","stamp":"1785014882101203978-0","to_stage":"validate","ts":1785014882,"v":1}
{"agent":"claude","event":"stage_launched","monitor_pid":2082324,"phase":1,"stage":"validate","ts":1785014882,"v":1}
{"decided_by_layer":null,"event":"advance_evaluated","phase":1,"reason":null,"stage":"validate","status":"success","ts":1785014998,"v":1,"verdict":"pass"}
{"event":"hook_run","hook":"DocsUpdate","ok":true,"phase":1,"ts":1785014998,"v":1}
{"event":"transition","from":"validate","phase":1,"to":"ship","ts":1785014998,"v":1}
{"event":"capture_archived","phase":1,"stage":"validate","stamp":"1785014998556399557-0","to_stage":"ship","ts":1785014998,"v":1}
{"agent":"claude","event":"stage_launched","monitor_pid":2087143,"phase":1,"stage":"ship","ts":1785014998,"v":1}
{"decided_by_layer":null,"event":"advance_evaluated","phase":1,"reason":"review: CR-01 (Critical) — 01-VERIFICATION.md scores Observable Truth #3 'Define→Plan→Execute→Ship carried end-to-end' as VERIFIED and stamps frontmatter status: passed, score: 3/3, but Ship never ran: git log --merges --all is empty, 0 tags, 0 remotes, and mai… [truncated; full output in .devflow/]","stage":"ship","status":"failed","ts":1785015595,"v":1,"verdict":null}
{"consecutive_failures":0,"event":"loop_back","from":"ship","phase":1,"ts":1785015595,"v":1}
{"event":"capture_archived","phase":1,"stage":"ship","stamp":"1785015595568298369-0","to_stage":"code","ts":1785015595,"v":1}
{"agent":"claude","event":"stage_launched","monitor_pid":2111517,"phase":1,"stage":"code","ts":1785015595,"v":1}
{"decided_by_layer":null,"event":"advance_evaluated","phase":1,"reason":null,"stage":"code","status":"success","ts":1785015910,"v":1,"verdict":null}
{"event":"transition","from":"code","phase":1,"to":"validate","ts":1785015910,"v":1}
{"event":"capture_archived","phase":1,"stage":"code","stamp":"1785015910528326477-0","to_stage":"validate","ts":1785015910,"v":1}
{"agent":"claude","event":"stage_launched","monitor_pid":2124240,"phase":1,"stage":"validate","ts":1785015910,"v":1}
{"decided_by_layer":null,"event":"advance_evaluated","phase":1,"reason":null,"stage":"validate","status":"success","ts":1785016138,"v":1,"verdict":"gaps"}
{"consecutive_failures":1,"event":"loop_back","from":"validate","phase":1,"ts":1785016138,"v":1}
{"event":"capture_archived","phase":1,"stage":"validate","stamp":"1785016138822021772-0","to_stage":"code","ts":1785016138,"v":1}
{"agent":"claude","event":"stage_launched","monitor_pid":2134135,"phase":1,"stage":"code","ts":1785016138,"v":1}
{"decided_by_layer":null,"event":"advance_evaluated","phase":1,"reason":null,"stage":"code","status":"success","ts":1785016334,"v":1,"verdict":null}
{"event":"transition","from":"code","phase":1,"to":"validate","ts":1785016334,"v":1}
{"event":"capture_archived","phase":1,"stage":"code","stamp":"1785016334686849915-0","to_stage":"validate","ts":1785016334,"v":1}
{"agent":"claude","event":"stage_launched","monitor_pid":2141940,"phase":1,"stage":"validate","ts":1785016334,"v":1}
{"decided_by_layer":null,"event":"advance_evaluated","phase":1,"reason":null,"stage":"validate","status":"success","ts":1785016530,"v":1,"verdict":"gaps"}
{"consecutive_failures":2,"event":"loop_back","from":"validate","phase":1,"ts":1785016530,"v":1}
{"event":"capture_archived","phase":1,"stage":"validate","stamp":"1785016530830016589-0","to_stage":"code","ts":1785016530,"v":1}
{"agent":"claude","event":"stage_launched","monitor_pid":2149588,"phase":1,"stage":"code","ts":1785016530,"v":1}
{"decided_by_layer":null,"event":"advance_evaluated","phase":1,"reason":null,"stage":"code","status":"success","ts":1785016642,"v":1,"verdict":null}
{"event":"transition","from":"code","phase":1,"to":"validate","ts":1785016642,"v":1}
{"event":"capture_archived","phase":1,"stage":"code","stamp":"1785016642635704568-0","to_stage":"validate","ts":1785016642,"v":1}
{"agent":"claude","event":"stage_launched","monitor_pid":2153935,"phase":1,"stage":"validate","ts":1785016642,"v":1}
{"decided_by_layer":null,"event":"advance_evaluated","phase":1,"reason":null,"stage":"validate","status":"success","ts":1785016861,"v":1,"verdict":"gaps"}
{"context":"Validation failed 3 time(s) — human review needed.","event":"gate_fired","phase":1,"stage":"validate","ts":1785016861,"unexpected":false,"v":1}
{"event":"notify_fired","phase":1,"stage":"validate","ts":1785016861,"unexpected":false,"v":1}
```

## Events present vs. absent

**Present** (verbatim above): `workflow_started`, `stage_launched` (×11),
`advance_evaluated` (×11), `transition` (×9), `capture_archived` (×10),
`hook_run` (×1), `loop_back` (×3), `gate_fired` (×1), `notify_fired` (×1).

**Absent, explicitly** — of the six events named in the plan's acceptance
criteria as the ones to check for:

- `transition` — **present** (9 occurrences)
- `stage_launched` — **present** (11 occurrences)
- `advance_evaluated` — **present** (11 occurrences)
- `gate_fired` — **present** (1 occurrence, the terminal event's cause)
- **`gate_resolved` — ABSENT.** No human ever answered the gate; observation
  ended before any response was recorded.
- **`workflow_finished` — ABSENT.** The run did not complete; it is parked.

## Capture files

`.devflow/gates/01-validate.json` (the pending gate, verbatim):

```json
{
  "phase": 1,
  "stage": "validate",
  "context": "Validation failed 3 time(s) — human review needed.",
  "timestamp": "1785016861"
}
```

`.devflow/state-01.json` (final state, verbatim):

```json
{
  "stage": "validate",
  "phase": 1,
  "agent": "claude",
  "mode": "auto",
  "gate_pending": true,
  "consecutive_failures": 3,
  "infra_failures": 0,
  "preflight_retries": 0,
  "started_at": "1785014171",
  "project_root": "/tmp/devflow-probe-23a-20260725-171553",
  "worktree_path": "/tmp/devflow-probe-23a-20260725-171553/.worktrees/phase-01",
  "monitor_pid": 2153935,
  "stop_until": null,
  "stopped": false,
  "stop_reason": null
}
```

Note `infra_failures: 0` — the state machine's own bookkeeping records zero
infrastructure (monitor/process) failures across the entire run. All three
counted `consecutive_failures` are **content** failures (validate found gaps),
not process deaths.

`.devflow/phase-01-stdout` (5123 bytes total, included in full — no
truncation was needed; this is the raw JSON result line from the last
`claude -p` invocation, the one that led to the pending gate):

```json
{"is_error":false,"duration_api_ms":204255,"num_turns":24,"stop_reason":"end_turn","session_id":"c411a185-30f6-48ba-b641-1fc314ee53eb","total_cost_usd":1.6351209999999998,"result":"GSD ► VALIDATE PHASE 1: add-probe-marker ... [full 5123-byte JSON payload preserved verbatim on disk at .devflow/phase-01-stdout; the human-readable summary embedded in its `result` field records a re-audit that found one new gap (G-6, an unsound Ship-detection verification rule in the phase's own validation doc) and closed it, ending `DEVFLOW_RESULT: {\"status\": \"success\", \"verdict\": \"gaps\"}`] ..."}
```

`.devflow/phase-01-stderr.log` — empty (0 bytes). No stderr was produced by
the final stage's monitor script.

**The Ship-stage capture is unavailable — rotated out.** `.devflow/history/`
retains only the 5 most recent per-stage captures (a fixed-depth ring). By the
time observation ended, the run had cycled through 5 further Code/Validate
captures after the Ship failure, so the capture stamped
`1785015595568298369-0` (Ship, the single most important failure in this
entire run) was evicted before this document could be written. Confirmed:
`ls .devflow/history/phase-01/` at write time shows only stamps
`1785015910528326477-0` through `1785016642635704568-0` — five stamps, all
from the loop-back cycles *after* Ship, none from Ship itself. **This is an
observability finding in its own right**: the evidence for the single most
diagnostically important event in the run — why Ship's own review rejected
Ship as having never run — was destroyed by the run's own subsequent retries,
and only survives at all because `advance_evaluated`'s `reason` field
happened to quote (truncated) the review verdict inline in `events.jsonl`.
Had that truncated inline quote not existed, this finding would have no
verbatim evidence for its most important claim.

## Termination condition that fired

Of the plan's three possible termination conditions — (1) `workflow_finished`
written, (2) the run parks at a gate with no further events for >10 minutes,
(3) no event appended for >30 minutes while `devflow status` reports something
other than a healthy in-progress stage — **condition #2 fired**: `gate_fired`
at 22:01:01Z, and no further event appeared through 22:13Z (>12 minutes of
silence at a parked gate).

**Instrument-defect disclosure — be precise about how this was determined.**
The polling instrument used during observation (45-second interval) tested
only whether the *last line* of `events.jsonl` contained the substring
`"gate"`. It did not auto-fire, because the actual last line at each poll
after 22:01:01Z was `notify_fired` — a real event, correctly written, that
simply does not contain the substring `"gate"` in its `event` field (the
gate-bearing line, `gate_fired`, was the second-to-last line, not the last).
The termination condition was therefore **not** detected automatically by the
polling instrument as designed. It was determined by directly reading the
event stream against the plan's stated criterion (a gate pending with >10 min
of subsequent silence) and recognizing that the condition was met despite the
instrument's silence. This is recorded as an instrument defect, not as a
correctly-functioning auto-detection — the termination was operator/analyst
judgment applied to raw evidence, not a green light from the polling tool.

## Manual-intervention assertion

**No `advance_evaluated` event in this stream was produced by a manual
`devflow advance` invocation.** Every `advance_evaluated` event originates from
the monitor's own trailing command. Confirmed directly: the still-running
monitor process for the final validate stage (PID 2153935, `sh -c ...`,
captured read-only via `ps` *after* observation ended, path components
redacted below) shows the `devflow advance` call baked into the script body
itself, invoked automatically after the wrapped agent process exits — not
typed by an operator:

```
sh -c apid=''; cleanup() { [ -n "$apid" ] && kill "$apid" 2>/dev/null; exit 0; };
trap cleanup TERM INT; cd '<scratch-dest>/.worktrees/phase-01' || exit 1;
"$@" > '<scratch-dest>/.devflow/phase-01-stdout' 2>'<scratch-dest>/.devflow/phase-01-stderr.log' &
apid=$!; echo $apid > '<scratch-dest>/.devflow/phase-01-agent-pid'; wait $apid;
echo $? > '<scratch-dest>/.devflow/phase-01-exit';
'<repo>/target/release/devflow' advance '<scratch-dest>' --phase 1 sh claude -p ...
```

No `ps`/`kill` was used to nudge the run at any point *during* observation.
The single `ps` invocation quoted above, plus one on the agent PID
(`2153936`), were both taken **after** the observation window closed
(22:13Z), read-only, solely to populate this document — per the plan's
explicit allowance ("If you need process facts for the finding, capture them
read-only and record that you did").

## Corroborating background (not from this run)

Before this probe was launched, 28 pre-existing hung `devflow advance`
processes were inventoried on the operator's machine, aged roughly 1h41m to
1d4h57m, every one with zero live `claude -p` children. **These are leftovers
from earlier sessions, not from this run** — they predate this probe's launch
timestamp and are reported here only as corroborating background: the `sh -c`
monitor pattern does leak orphaned processes over time (a real defect), even
though it did **not** block or kill *this* run. This run's own monitor
(2153935) is confirmed alive at write time.

## Analysis

### Where it stopped

The run stopped in the **validate** stage, parked at a pending human gate.
The last event written was `notify_fired`. `.devflow/phase-01-exit` contains
`0` (the wrapped `claude` agent process for the final validate attempt exited
cleanly — it was not killed and did not crash). `.devflow/phase-01-agent-pid`
contains `2153936`, and that process is confirmed not running (it completed
and exited 0, consistent with the exit file).

### Why

The evidence directly **contradicts** the phase's central hypothesis that the
`sh -c` monitor is the blocker:

- The monitor did not die. Across the full run it launched 11 stages,
  archived a capture at every single hop (10 `capture_archived` events),
  correctly incremented `consecutive_failures` 0 → 1 → 2 → 3, and correctly
  fired the threshold gate at exactly the third consecutive validate failure.
  `infra_failures: 0` in the final state file. The monitor process for the
  last stage (2153935) is still alive at write time.
- The run reached **Ship** — the first full Define→Plan→Code→Validate→Ship
  traverse on record. Per ROADMAP.md's run-record table: Phase 17 died at
  Ship to two silent monitor deaths; Phase 21 stopped after Define; Phase 22
  stopped dead at a Plan relaunch with no `advance_evaluated` at all. This run
  produced 11 `advance_evaluated` events and got further than any prior
  recorded run.
- Ship's own `advance_evaluated` **failed for a content reason, and a
  correct one**: the review caught `01-VERIFICATION.md` scoring "Define→Plan→
  Execute→Ship carried end-to-end" as VERIFIED with `status: passed, score:
  3/3`, while Ship had demonstrably not run — no merge commits, 0 tags, 0
  remotes, `main` at the same commit as the phase's starting point. DevFlow
  caught a false-green in its own verification artifact and refused to let it
  ship. **That is correct behaviour, not a defect.**
- The run then spent its 3-failure retry budget cycling Code→Validate
  (fixing the false-green claim and other doc-accuracy gaps a deep review
  kept finding — see the archived `-REVIEW.md` captures, e.g. one new gap
  (G-6) about an unsound Ship-detection verification rule was found and
  fixed during the final cycle) and parked at the designed human-review gate.
  That is the **designed outcome** of exhausting the retry budget, not a
  malfunction.

**This contradicts the phase's central hypothesis.** The `sh -c` monitor was
not the blocker in this run — it functioned correctly end to end, including
through the one genuine failure (Ship). The actual bottleneck observed here
was the quality/accuracy of the phase's own attestation documents (repeated
doc-accuracy gaps caught by a deep review pass across three validate cycles),
which is a content-correctness dynamic, not an infrastructure-liveness one.
Per the plan and CONTEXT.md, a finding that contradicts the hypothesis is the
highest-value outcome this task can produce, and it is reported as such
without softening.

## Scope verdict

| Unit | Verdict | Evidence |
|---|---|---|
| **23b** — replace `sh -c` monitor with socket-addressable supervisor | **INVALIDATED**, as the hypothesis that monitor death/liveness-ambiguity is what blocks an end-to-end run. The monitor (PID 2153935 for the final stage) survived the entire run, correctly tracked all 3 consecutive failures, correctly archived 10 captures, and correctly fired the gate; `infra_failures: 0`. The actual stop cause was a content-review rejection at Ship, not a process failure. *Split note:* 23b's broader design rationale — liveness answerable as GONE/STALE/ALIVE instead of PID-existence-only — is a structural property this run never needed to exercise (the monitor was never ambiguous, so nothing here confirms or refutes that specific property). That narrower design goal is UNTOUCHED even as the "monitor is the blocker" framing is INVALIDATED. |
| **23c** — `devflow stop` | **UNTOUCHED**. No `devflow stop` was invoked — the observation protocol explicitly forbade signalling or killing processes to nudge the run, and no premature abort was needed or attempted. 23c is scoped as blocked-on-23b only; this probe supplies no evidence for or against the abort-verb design itself. |
| **23d** — delete `sequentagent` | **UNTOUCHED**. Every one of the 11 `stage_launched` events carries `"agent":"claude"`, and the captured monitor script invokes `claude -p` directly with no two-agent failover branch. The `sequentagent`/two-agent path (and DEN-58's "explicitly-untested `wait_for_agent_exit`" gap it exists to close) was never invoked by this run, so it is neither confirmed nor closed. |
| **`--yes-ship`** | **UNTOUCHED**. The flag was not passed on this launch. The run never got a second chance to reach a Ship gate that `--yes-ship` could auto-answer — Ship's `advance_evaluated` failed outright on a content-review rejection before any Ship gate could fire, and the gate this run actually parked at is the unrelated 3-failure Validate retry-exhaustion gate, a mechanism `--yes-ship` does not touch. |

## Source safety (T-23-05)

The probe ran exclusively inside `/tmp/devflow-probe-23a-20260725-171553`, a
destination scaffolded by `scripts/scratch-dogfood-repo.sh` outside this
checkout. `git status --porcelain crates/` in this checkout is empty —
confirmed immediately before writing this document. No Rust source, no
`.devflow/` state file, and no git config in this checkout or on this machine
was modified by the probe.

## Redaction note (T-23-06)

All absolute home-directory paths and the operator's OS username have been
redacted from this document (replaced with `<repo>`, `<scratch-dest>`,
`<home>`, or similar placeholders). Paths under `/tmp/devflow-probe-23a-*`
are scratch paths carrying no identity and are kept verbatim per the phase's
own threat model.
