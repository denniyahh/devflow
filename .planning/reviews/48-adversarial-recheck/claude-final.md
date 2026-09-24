# Phase 48 recovery patch: adversarial review (diff only)

**Verdict:** I found no confirmed security or correctness regression in the production code this patch changes. I found one confirmed false claim, in a test name. One test change may make that test hang, and I found three weaker suspicions. My main limit is that I only had the diff. `stop_via_lock`, `persist_stopped_state`, the `Gates` consume and cleanup order, and `answered_gate_contention_message` are not in it. Every conclusion below that depends on them is marked SUSPECTED. I did not run anything.

## Needs your awareness

### 1. SUSPECTED — the Supervise test fix can cause a hang
**File:** `pipeline_launch.rs`, `supervise_rescan_approval_records_but_requires_a_human_gate`

**What changed:** The old writer waited for `response_path` to disappear before writing the second response. The new writer only waits for `state.checkpoint_approval == expected_approval`, then immediately calls `write_gate_response(..., Code, false, "abort")`. The response file is no longer checked at all.

**The failure:** Suppose the approval arm runs in this order:
1. persist the approval,
2. clean up the re-scan response,
3. `write_gate` the human Code gate.

This is the same "save, then cleanup" order `park_auto_rescan_repair` uses in this very diff. The writer can land its abort between steps 1 and 2. Step 2 then deletes it, or step 3 does if `write_gate` clears stale responses. The second gate then gets no answer. `advance_for_stage` blocks on the poll that the nearby test comments call "three-day", and the only guard is whatever timeout the child-test harness has.

**Why I believe the order can differ from what the test assumes:** The old writer's wait-for-removal-then-assert pattern was apparently being replaced because "removed before persisted" happened. So neither order is guaranteed by what the test checks.

**How to reproduce:** Add a sleep between the approval's `save_state` and `Gates::cleanup` in the approval arm, then run the test. If the order is persist-then-cleanup, it should hang instead of failing.

**Fix:** Wait for both conditions: approval persisted *and* the old response gone, or better, the new Code gate file present.

### 2. SUSPECTED — a Ship gate with a recycled or unconfirmable holder may no longer be stoppable
**File:** `commands.rs` `stop_via_gate`, the `ship_without_holder` hunk

**What changed:** Ship with a `Recycled` holder now returns `(false, …)` and falls through to `stop_via_lock`. According to the patch's own tests, that fails with "refusing to signal … recycled", leaves the state untouched and writes no response. Ship with an `Unconfirmable` (legacy) lock is also newly refused. No test covers that case.

**Why it matters:** The printed repair is `no_waiter_repair(phase, Ship)`, which is `devflow ship --phase N`. If the recycled pid belongs to a long-lived process, `stop` can never succeed. Whether `devflow ship` or `resume` can proceed past a recycled lock is not visible in the diff. If they can't, the operator's only remaining option is deleting the lock by hand.

**Weak rationale:** The new doc comment says a response left for a recycled holder "would permanently poison the gate for the real recovery path". But a response left in the `NoHolder` case is the *same* abort, read by the *same* `devflow ship` recovery path, and the patch keeps that one on purpose. `Recycled` means the pid in the lock file is dead and has been reused, which is semantically no holder. The split is defensible as caution, but the "poison" justification doesn't separate the two cases.

## Confirmed (low severity)

### 3. CONFIRMED — false-claim test name
**File:** `stop_e2e.rs`, `stop_at_a_ship_gate_with_a_recycled_holder_reports_state_not_marked`

The hunk deletes `assert!(stderr.contains("not marked stopped"), …)` and replaces it with an assertion on "refusing to signal" plus "recycled". The name still claims the old behaviour, which no longer happens.

**To see it:** Read the renamed assertion block. Nothing in the test checks "not marked".

## Lower-confidence notes (SUSPECTED)

- **The "treating it as no waiter" message in `stop()` is now nearly unreachable.** `gate_holder = Recycled` now always comes with `gate_answered == false`, so `stop_via_lock(...)?` runs first. By the new tests, it errors on a recycled lock before the `println!`. The message only fires if the lock changes between the two reads. That is harmless, but it is a message that looks meaningful and almost never prints.
- **The doctor fixture now lives 4× longer if the test fails partway.** `sleep 30` became `sleep 120`, and there is no drop guard (`Child` does not kill on drop). An assertion panic before `child.kill()` now leaves a stray shaped like a monitor wrapper for 120 s. Any concurrent test that runs a stray census or reaps strays gets a 4× wider window to find it, count it, or kill it. That could produce the very false "doctor signalled it" failure the comment is trying to avoid. A kill-on-drop guard would fix both problems.
- **"Supervise for its remaining stages" is only checked for one launch.** The new gate and park text promises Supervise for the rest of the phase. The Auto test checks `mode == Supervise` only after the first supervised `resume`. Nothing in the diff shows that later stage transitions keep `Mode::Supervise`, so the claim is unverified beyond Code.

## Checked and clean (within what the diff shows)

- **Ship with no holder:** the reap path and the new test are consistent. The time-of-check gap between `holder_status` and `reap` already existed before this patch; the old code let every Ship status through.
- **Auto rejection handoff:** `park_auto_rescan_repair` changed only its strings, not its logic. In the Auto test, the new `third_gate_response_written` flag and the exact one-gate count are sound opposite-result checks. This holds as long as the response stays on disk until the cleanup that runs after `save_state`, which the doc comment asserts and I cannot see.
- **Same-identity holder proof:** the diff doesn't change the comparison code, so nothing new is introduced there.

**What would settle 1 and 2:** Reading the approval arm's persist/cleanup/`write_gate` order in the `Gates` source, and how `stop_via_lock` and `devflow ship` handle a `Recycled` lock.

One environment note: the GitHub MCP server timed out on connect, and the Linear and Vercel connectors need authorizing in claude.ai connector settings. None of them were needed for this review.
