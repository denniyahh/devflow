Review written to `/home/denniyahh/.claude/plans/you-are-reviewing-only-whimsical-cray.md`. Nothing was compiled or run — every claim is read off the diff text.

## Needs your decision

**~1/3 of this is unresolvable without the checkout.** Four questions decide whether my three SUSPECTED findings are real or void: does `events::emit` fan out to a notifier; can `HolderStatus::Live` coexist with a `None` start time; does `resume`'s 3rd param take a mode override; does `Gates::cleanup` tolerate `NotFound`.

Options: (a) rerun me against the repo, (b) you answer the four, (c) ship as-is with labels intact. **Recommend (a)** — finding #1, the most serious, hinges entirely on the first question.

## Needs your awareness

**CONFIRMED — the Auto rejection path is a silent halt in the unattended mode.** The `return park_auto_rescan_repair(...)` skips `handle_stage_failure`, which the diff's own context describes as "never silent (WR-11): it always fires a gate + notify." Supervise rejections still get that; Auto no longer does. Park's entire output is one event emit and a `println!` — and `advance` runs as the monitor's child, so that stdout lands in a capture file, not a terminal.

**CONFIRMED — one rejection permanently downgrades the phase to Supervise, undisclosed.** `state.mode = Mode::Supervise` is persisted and nothing in the diff restores Auto. Checkpoint auto-decide stays disarmed for the phase's remaining life; Validate/Ship become human-blocking. Neither the operator message ("parked for supervised repair... run `devflow resume`") nor the new Auto gate-context string ("approve, park for supervised repair, or abort") says so. That's your false-operator-claim.

**CONFIRMED — the Auto test's "negative control" cannot fire.** Its writer checks `stopped` before checking the response file, and park persists `stopped = true` *before* cleanup — so the abort branch is unreachable whenever park runs at all, however wrong park is. Nothing asserts inertness either. Net effect: the test asserts the new path works but never demonstrates the Auto loop-back refusal that justifies the fix. The only evidence for that anywhere is prose in an unrelated test's comment.

**CONFIRMED — new flaky assertion in the Supervise test.** The added `checkpoint_approval` assertion wakes on `response_path` disappearing, but the producer does `Gates::cleanup` (removes it) *then* `save_state` (writes the asserted value). Wrong signal. Ironically the Auto test avoids this by keying on `stopped` — the two tests disagree about which signal is safe and only one is right.

**CONFIRMED — park commits state before cleanup.** A `Gates::cleanup` failure returns via `?` before the `checkpoint_repair_parked` event, leaving the phase parked with gate+response+ack live and no audit record — the next supervised Code gate then consumes the stale rejection, which is the CR-01 hazard the `Advance` arm explicitly guards against. Also: that arm swallows cleanup errors, park propagates them; same call, opposite handling.

**SUSPECTED — `stop`'s tightened arm 1 can error on its own happy path.** It now requires the holder to still hold the lock at a fresh read, but the correct outcome of `stop` is the holder releasing it. If `abort()` completes between the failed `acquire` and the two reads, the match falls to `_` and prints "phase state was not marked stopped" plus a repair command that restarts what the operator just stopped. Microsecond window; mechanism is plain in the hunk.

Two smaller ones in the file: `rescan_gate_context`'s new mode branch has zero tests (and it's where finding #2's disclosure gap lives), and park skips the registry deregistration / `monitor_pid` clear that `abort()` does.

## Detail

The `github` MCP server timed out connecting (30s), and `claude.ai Linear` / `claude.ai Vercel` need OAuth you can't grant in a non-interactive session — so even a repo-fetch fallback wasn't available this run. Worth a retry before option (a).
