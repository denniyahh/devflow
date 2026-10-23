## CONFIRMED

*All citations checked by reading the source at this snapshot. I did not build or run anything, and the snapshot has no `.git`, so no claim below rests on test outcomes.*

### 1. D-05's "refused … and left on disk" makes the gate it refuses permanently unanswerable — and D-07 guarantees that case fires

There is exactly one response path per phase+stage: `gates.rs:131-136` (`{NN}-{stage}.response.json`), with no id in the name. Three code facts then compose:

- `Gates::respond` refuses while that path exists: `gates.rs:220-221` → `GateError::AlreadyResponded`. So once a stale answer sits there, `devflow gate approve` **cannot** write the answer for the gate that is actually waiting.
- `Gates::list_open` skips any gate whose response file exists: `gates.rs:170-172`. So the live gate disappears from `gate list`, from `stop_via_gate`'s `Gates::list_open(...).find(...)` (`commands.rs:2681-2685`), and from `gate_respond`'s `--stage`-omitted resolution (`commands.rs:1529-1531` → `resolve_single_open_gate_stage`, `commands.rs:1509-1512`: *"no open gate for phase N"*). The tool reports no gate while one is pending.
- `poll_response` re-reads that same path on every backoff step (`gates.rs:290-300`), up to 3 days. An id mismatch therefore re-logs the refusal every 1–60 s for the whole wait.

D-05 requires the mismatched answer be "never silently consumed and never silently deleted". D-07 *creates* the mismatch by construction: the Supervise rejection of the re-scan gate is answered at `stage` (`pipeline_launch.rs:2708-2719`), the `GateAction::LoopBack(_)` arm for Supervise sets `reason` and falls through **with no `Gates::cleanup`** (`pipeline_launch.rs:2726-2742`; contrast the `Advance` arm at `:2704` which does clean up), and `run_gate_with_timeout`'s `Answered` branch cleans nothing up either (`pipeline_gate.rs:466-493`). The follow-on gate then fires at the *same phase+stage* via `handle_stage_failure` → `run_gate` (`pipeline_outcomes.rs:903-909`) and finds the just-refused rejection still on disk.

Net: the operator's only escape is a manual `rm` of a file that D-05 forbids the code to delete, and the wedge is created by the decision whose purpose was to make gate answers safer. Nothing in the CONTEXT, ROADMAP criteria 3–6, or D-05's four enumerated tests names this outcome. Either the response path becomes per-gate-id (not stated anywhere) or "left on disk" is not implementable as written.

### 2. D-01 cannot close A7/C4, because a pending record has no handle to probe

D-01 claims "a record that the start/resume guard probes" closes A7/C4 "by construction". The state field is a single `Option<SupervisorRecord>` whose own doc says the socket path is the only starting point: *"The socket path is the only handle a liveness question starts from: it is passed to `probe` as recorded, never re-derived"* (`state.rs:288-293`). Then:

- `supervisor::probe` on a path with no file returns `Liveness::Gone` (`supervisor.rs:341-343`, via `own_socket_at` `:363-366`), and `recorded_live_socket` maps `Gone` → `None` (`commands.rs:3377-3378`), so the guard proceeds and the launcher launches.
- Before the supervisor publishes there is no socket. So in the window D-01 introduces — pending record saved, spawn in progress or spawn died — the guard reads **GONE** and permits a second launch over a supervisor that is live. That is A7/C4 relocated, not closed.
- The record carries no supervisor pid to fall back on (`state.rs:453-464`: `socket_path`, `launch_id`, `agent_pgid`, `agent_start_time`, `boot_id`). `Published.supervisor_pid` exists but is not persisted (`monitor.rs:390-396`). So fail-closed liveness is not merely unimplemented, it is unrepresentable in the record shape D-01 chose.

This also falsifies criterion 1 as written: after D-01 lands, "a supervisor that is live but whose record was never saved" **cannot be constructed** (the record now precedes the spawn), so the A7 fixture can only exist by disabling the pending save — the criterion cannot fail for a wrong implementation, only for a missing one.

### 3. D-01's stated constraint contradicts the shape it leaves to Claude's discretion

*"The constraint: at every instant the state names every supervisor that might be live for this phase."* At the instant N+1's record is saved, S(N) is still live and `publish_record` overwrites the one slot (`pipeline_launch.rs:1745-1747`). The crash table already records the consequence: C11's expected outcome is *"state names the live N+1; N's leftover socket reads STALE with another launch id"* (`50-CRASH-TABLE.md`, row C11). The offered discretion — *"a `SupervisorRecord` state field, or a separate `pending` field"* — only satisfies the constraint in its second form; the first form cannot, by construction. The discretion is presented as free when the constraint has already decided it.

### 4. D-08 silently voids four Phase 48/50 mitigations and creates a new stale-answer path nobody tests

Every response writer keys off `lock::holder_status` because a waiting process holds the phase lock (48 D-02's "verified side effect relied on by D-05"). Under D-08 no gate wait holds a lock, so all of these degrade to a constant `NoHolder`:

- `gate_respond`'s refusal and its Ship exception: `ship_without_holder = stage == Ship && NoHolder` (`commands.rs:1534-1541`). Post-D-08 this reads "**at Ship, always write, waiter or not**".
- `gate_sweep_may_reap` requires `Live` (`commands.rs:1857-1859`) — post-D-08 the aged-gate sweep **never reaps**, silently.
- `stop_via_gate`'s waiter gate and Ship exception (`commands.rs:2698-2706`).
- `answered_gate_contention_message`, which is entirely a comparison of observed/current lock identities (`commands.rs:2953-2985`) — meaningless once no lock is held while waiting. T-48-16-02's stated mitigation ("the observed and current lock records have the same live `(pid, start-time)` identity", `48-SECURITY.md:90`) ceases to hold.

The new stale-answer path this opens: operator runs `devflow gate approve` at Ship after the run died (nothing waiting, `NoHolder` → writes anyway, `commands.rs:1538-1546`); the gate file and its id survive; `devflow resume` later re-opens "a gate that is still pending under the same id" (D-06) and consumes that answer. D-05's four enumerated tests cover the killed-waiter, late-`respond`, check-to-write and `start`-cleanup paths — **not** this one, which D-08 itself creates. D-04 acknowledges the writers "must identify the live waiter some other way" but leaves which way to Claude's call, so the very predicate these five call sites share is undecided at the level of a phase decision.

### 5. D-05 and D-06 contradict each other on the first stale-answer path

D-05 lists *"a waiter killed after an answer is written, then `resume`"* as a path that gets a **failing-then-passing** test. D-06 deliberately preserves exactly that outcome: resume re-opens the still-pending gate under the **same** id, so the pre-crash answer decides it. That is AR-48-04 path (a) verbatim, whose pinned evidence is `pipeline_launch::tests::resume_relaunches_without_consuming_a_pending_gate_answer` (`pipeline_launch.rs:3687-3689`, present, and derived from `48-SECURITY.md:131`). A test for this path cannot fail before the fix, because the fix's stated promise is the pre-fix behaviour. Either that bullet belongs under "intended, unchanged" or D-06 must give the id a reason to change across a resume — and criterion 4 ("Each stale-answer path … has a failing-then-passing test") is then unsatisfiable as written for path (a).

### 6. D-03's premise understates existing code; the decision as worded makes a live supervisor's stop unrecordable

The Scout claim is technically true of `advance_with` (`pipeline_launch.rs:2508-2532` has no launch-id check) but omits that the same file already implements launch-id binding twice, including a deliberate double check:

- `record_supervisor_stop`: `names_this_launch` compares `record.launch_id == launch_id` before the lock and again under it (`pipeline_launch.rs:1428-1466`), with a doc comment explaining why a superseded supervisor "write[s] nothing".
- The launch id already travels to the supervisor by argv and *is* the socket's last file-name segment (`state.rs:456-457`; `monitor.rs:499-503`). So D-03's open question — *"Whether it travels by env, argv or sidecar is Claude's call"* — is not open.

More consequentially, D-03's scope ("covers its advance and **every other state write**") combined with `record_supervisor_stop`'s existing refusal means a supervisor that is *live* but superseded in the state cannot record its own stop. `stop` then writes whatever it writes by its own path, while the agent group keeps running and `record_supervisor_stop` logs *"the state does not name launch {id}; its stop is not this supervisor's to record"*. Crash-table rows B1/B2 ("socket stop → agent group killed … `stopped`") are stated for the owning supervisor only; the superseded-but-live case has no row and D-03 does not name it.

---

## SUSPECTED

### 7. Two launchers can both pass the guard once the long lock stops being ownership (speculation grounded in code)

`start` does check-then-act while holding `lock::acquire` — lock at `commands.rs:369`, guard at `:373`, leftover-gate cleanup at `:385`. D-08 removes ownership from that lock and leaves "whether a coarse per-phase *launch* exclusion survives … is Claude's call". If it does not survive, both launchers see GONE, both write a pending record, and `update_state`'s re-read-and-apply overwrites the first id with the second (last writer wins unless the closure refuses — no decision says it must). Launcher 1's `publish_record` then saves its own record over launcher 2's (`pipeline_launch.rs:1745-1747`), both supervisors run, and per finding 6 the loser cannot record its own stop. `<specifics>` already demands that every new window D-01 opens be named with a crash-table row and an outcome; this window is left to discretion instead.

### 8. The `flock` primitive D-08 reuses is inherited across `fork`, so the "short" write lock can outlive its scope

`lock.rs:216-224` documents this in-tree: *"a child forked while the guard was live holds a duplicate of the coordination fd, and with it the flock, until it execs."* A per-write lock taken around `update_state` is therefore briefly held by any concurrently forked child, and the design's claim that "no … agent run … holds that lock" holds only because Rust opens fds close-on-exec. Worth stating in the plan rather than inheriting silently.

### 9. Two citations and one acceptance criterion do not carry the weight placed on them

- D-07's *Verified* citation `preflight.rs:1098` lands in the doc comment block (`preflight.rs:1088-1112`), not on the code that returns `Ok(())` for `Mode::Supervise`. The conclusion is correct; the file:line is not where the behaviour lives.
- Criterion 6's second half — a single Phase 49-rig run to Ship — is a **n=1** observation of a binary whose supervisor the same document says has never run at HEAD ("No live run has exercised the Phase 50 supervisor at HEAD"). A failure cannot be attributed to 50.1, and a pass licenses nothing about the durability or ownership properties the phase exists for. Criterion 3's "no gate wait holds a lock" likewise has no stated test and no negative control (plant a lock acquisition inside a gate wait; nothing in D-12's proof bar would notice).
- D-09's source-scan test cannot see the state *deletions* it must not be confused with (`clear_state`, `workflow.rs:345-359`) and would miss `migrate_legacy_state`'s `std::fs::rename` as a write (`workflow.rs:181`); D-11 removes that one writer but not the scan's blind spot.

### 10. Rig/runbook updates are missing from the plan-task list D-08/D-05 require

`<domain>` names only README, OPERATIONS and ARCHITECTURE. But D-12's proof bar runs the Phase 49/50 rig, which drives gates through the CLI whose `NoHolder` refusal changes under D-08: `50-RUNBOOK.md:152` and `:160` (`gate approve 1 --stage define|code`), `.planning/phases/49-.../tools/negctl.sh:249` (`gate reject`). If D-04 does not re-point `may_be_waiting` at a registration, those scripted approvals refuse and D-12's live run cannot reach Ship. `OPERATIONS.md:229` also documents the long lock as user-visible ("How long a monitor stays alive holding the phase lock while parked at a gate") — that is one of the three docs listed, so this one is covered; the runbook is not.
