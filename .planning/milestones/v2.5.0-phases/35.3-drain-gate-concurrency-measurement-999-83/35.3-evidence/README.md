# Phase 35.3 — Drain Gate Concurrency Measurement: analysis record

Landing spot for the phase's measurement evidence and the honest answer to the question the
phase exists to ask: **does the drain gate observe real sub-agent concurrency?** See
`COUNTS.md` for the raw per-capture counts and `real/run.log` for the real-capture provenance.

## The answer, per path (criterion 2)

Measured on Claude CLI **2.1.228**, build `6314eff`.

| Path | What the CLI emits | Which gate arm observes it | Verdict |
|---|---|---|---|
| **Sub-agent dispatch** | `task_started` (`local_agent`) → `task_progress` → `task_updated`/`task_notification` (completed). **Zero** `background_tasks_changed`. | `open_tasks` (the `47ea511` per-task arm) — the legacy drain arm never fires for this path | ✅ observed |
| **Backgrounded shell** | `background_tasks_changed` ×2 (`local_bash` announce, then drained `[]`) **and** the per-task vocabulary. | BOTH arms — the legacy drain arm IS live for this path | ✅ observed |

**The honest answer is positive**: on CLI 2.1.228 the widened gate observes both
concurrent-work mechanisms. But the two paths resolve through **different arms** — the
sub-agent path is seen only by the new `open_tasks` arm (the legacy `background_tasks_changed`
arm is vacuous for it, exactly 34-evidence's refutation), while the backgrounded-shell path
fires the legacy arm. The gate as widened in `47ea511` is therefore the right shape: neither
arm alone covers both paths.

## What was measured and how strong it is (criterion 4)

| Dimension | Value |
|---|---|
| CLI version | 2.1.228 (Claude Code), pinned — no matrix |
| Synthetic sample | n=2 per path (4 captures) |
| Real sample | 1 `devflow start --no-worktree --until validate` probe, 2 sub-agent dispatches |
| Workload shapes | synthetic drill prompts (forced sub-agent / forced backgrounded shell); a real single-file probe phase |
| `devflow` build | `6314eff` |

**What this evidence can support:** on CLI 2.1.228, sub-agent dispatch emits the per-task
vocabulary (validated by BOTH synthetic n=2 and a real run) and the backgrounded shell emits
`background_tasks_changed` + the per-task vocabulary (synthetic n=2 only).

**What it cannot support:** the backgrounded-shell mapping is **not validated against a real
run** — the probe backgrounded nothing, so that half rests on the synthetic capture alone. A
different CLI version, or a workload that backgrounds differently, is outside this
measurement's reach (one CLI version, one workload family). n=2 is a floor, not a
statistical sample.

## Fixture-label correction (criterion 3) — recorded in source, not here

The synthetic fixture (`monitor.rs` `bg_tasks_line`) manufactures `background_tasks_changed`
with `task_type: "local_agent"`. This measurement found the current CLI emits
`background_tasks_changed` with `task_type: "local_bash"` (backgrounded shells) and uses
`local_agent` only in the per-task `task_started` vocabulary (sub-agents). The fixture's
provenance (Phase 30's `30a-evidence/raw_output_v3.jsonl`) is real — that capture *did* carry
`local_agent` inside `background_tasks_changed` — but it is now stale. The comment correction
(pinned in `monitor.rs` by this phase) records that CLI-version distinction.

## Validate-failure observation (criterion 5)

`phase_validate_failures` — **0**, read from the plan-02 real-capture probe's
`.devflow/state-01.json` at `stage: "validate"`. This is a real observation of a first-attempt
Validate pass with no Code loop-back — but of a **trivial single-file probe phase**, not of a
real multi-wave phase. Phase 35.3's own execution was inline (not a `devflow start` pipeline),
so it produces no Validate-failure counter of its own. The provisional
`MAX_PHASE_VALIDATE_FAILURES = 10` judgement is therefore still unmeasured by a real,
multi-wave run.

## Residual gaps (D-03)

**None.** The measurement did not find a path the widened gate fails to observe, so no
`999.x` backlog item is filed. (If the backgrounded-shell mapping had shown neither family, or
`open_tasks` had been wrong for a measured reason, that specific gap would be filed — it was
not.)
