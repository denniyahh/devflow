## CONFIRMED

These are source and document findings. No runtime interleavings were executed.

1. **The “Verified” guard claim is incomplete, allowing A7/C4 tests to pass without fixing ownership.**

   `50.1-CONTEXT.md:65–67` says the guard “probes only `state.supervisor`.” That describes `possibly_live_sockets`, not the complete guard. `crates/devflow-cli/src/commands.rs:3291–3296` also reads the agent PID file and refuses when `agent::agent_running` succeeds. The supervisor writes that PID before publishing its socket (`crates/devflow-core/src/monitor.rs:1493–1508`).

   Consequently, ROADMAP criterion 1’s fixture can refuse on unchanged code simply because its agent remains alive. To discriminate ownership protection, keep the unrecorded supervisor alive **without a live agent PID providing the refusal**; separately retain a live-agent control. The broad “second launch is not refused” descriptions in crash-table A7/C4 also need this qualification.

2. **D-09 specifies the wrong publication operation for responses and omits durable deletion.**

   `50.1-CONTEXT.md:54–55` preserves exclusive hard-link publication, but `:148–149` routes **responses too** through a helper described as “fsync … before the rename.” The existing implementation explicitly rejects rename fallback because it could overwrite the first response (`crates/devflow-core/src/gates.rs:536–538`, `:556–562`). An unconditional rename helper contradicts first-answer-wins.

   Deletion is another missing operation: state removal uses an unsynced `remove_file` (`crates/devflow-core/src/workflow.rs:364–371`), as does gate/response/ack cleanup (`crates/devflow-core/src/gates.rs:378–389`). Durable replacement does not make those removals durable; a crash can resurrect deleted records.

   ROADMAP criterion 3’s source scan establishes helper usage, not correct exclusivity or durability. Specify separate replace, exclusive-publish, and remove contracts. Controls should include competing responses, a bypassing removal, and injected synchronization failures. **The stated helper contract does not establish full D1 closure.**

3. **D-08’s promise to preserve unlocked hand edits is impossible as stated.**

   `50.1-CONTEXT.md:142–143`: “a re-read-on-write keeps them, without locking,” including a `yes_ship` revocation.

   Counterexample: `update_state` acquires its advisory lock and reads `yes_ship=true`; the operator writes `false` without taking that lock; `update_state` renames its modified copy containing `true`. Re-reading protects edits completed **before** the read, not edits racing the transaction.

   The current implementation already separates reload and save (`crates/devflow-cli/src/pipeline_gate.rs:471–474`); wrapping that sequence in an advisory lock cannot constrain an editor ignoring it. Phase 48 explicitly accepted outside-lock writers as a gap (`48-CONTEXT.md:84–94`). D-08 claims that gap disappears without supplying a mechanism. Require a coordinated edit command or narrow the guarantee.

## SUSPECTED

These are source-grounded failure schedules the document leaves unresolved, not reproduced failures of a future implementation.

4. **A pending record does not distinguish “spawn failed” from “supervisor exists but has not published.” A7/C4 remain conditional.**

   D-01 claims closure “by construction” (`50.1-CONTEXT.md:60–71`), while D-02 treats GONE as safe to resume (`:74–76`). Currently GONE excludes a record from the guard (`crates/devflow-cli/src/commands.rs:3377–3379`), and the child starts its agent before publishing (`crates/devflow-core/src/monitor.rs:1493–1508`).

   Schedule: launcher A saves pending, spawns S, then dies before S publishes; launcher B probes GONE and reserves another launch; S continues. Conversely, refusing every pending/GONE record wedges a launcher that died before spawning. `stop` cannot contact an unpublished socket.

   D-08 makes launch exclusion optional (`:144–145`). Require atomic check-and-reserve, fencing before agent spawn, and an explicit abandoned-reservation outcome. At stage handoff, also resolve the permitted single-field shape (`:200`): replacing N’s token with pending N+1 immediately supersedes N under D-03, although today N performs the subsequent record save and failure repair (`pipeline_launch.rs:1745–1755`).

5. **Removing the long lock exposes a second gate consumer: `devflow ship` can race the supervisor into finalization.**

   D-04 inventories answer **writers** (`50.1-CONTEXT.md:88–91`), but `ship_override` is an independent response **consumer**. Its current exclusion is the per-phase lock (`crates/devflow-cli/src/pipeline_gate.rs:556–559`, `:584–593`); it checks ack absence, reads the answer, and enters finalization (`:614–628`, `:635–651`).

   With no lock across the supervisor’s wait, both consumers can observe the same approval before the supervisor writes its ack (`:466–475`). A correct gate ID does not prevent both from running merge/version/tag/push hooks.

   Require an atomic consumption claim shared by both consumers. The same ownership fence must cover a superseded waiter’s ack and cleanup: checking only its eventual state write leaves destructive gate-file operations outside the fence (`:494–507`). Criterion 5 needs a paused old-waiter/new-owner test, not merely a rejected stale state save.

6. **D-06 does not define how resume returns to the original question. Same stage is insufficient.**

   `50.1-CONTEXT.md:108–110` promises reopening the pending gate under the same ID. Current `resume` instead saves state and calls `launch_stage` (`crates/devflow-cli/src/pipeline_launch.rs:2334–2335`). Persisted gate tracking is a boolean (`crates/devflow-core/src/state.rs:45–47`); `GateFile` contains stage and human-readable context, not a typed continuation (`crates/devflow-core/src/gates.rs:28–36`).

   Code-stage gates include preflight refusal, checkpoint re-scan, and stage failure. Ship has routine approval and finalization retry. Reusing an existing ID merely because a same-stage gate file remains can bless a different question; always generating a fresh ID breaks the late-answer promise.

   Persist the gate’s purpose and decision inputs, and define its resume continuation. Otherwise the killed-waiter→resume path in AR-48-04 remains unresolved. This is additional execution/recovery work beyond adding identifier fields, and changes Phase 48’s documented relaunch behavior (`48-CONTEXT.md:117–132`).

7. **Automatically reading the current gate ID at publication can relabel a stale CLI decision as a fresh one.**

   D-05 says the CLI “fill[s] it in” (`50.1-CONTEXT.md:92–94`) without specifying when identity is captured. Existing sweep decisions are made from a previously listed gate, but publication passes only phase and stage (`crates/devflow-cli/src/commands.rs:1667–1678`, `:1699–1705`). `Gates::reap` likewise accepts no observed gate identity (`crates/devflow-core/src/gates.rs:247–259`).

   Schedule: sweep identifies aged gate G1; G1 closes and G2 opens at the same stage; publication reads G2’s ID and attaches it to the rejection decided for G1. ID validation then succeeds incorrectly.

   Pass the **observed** ID through every writer and revalidate it at publication. Registration must also identify the actual waiter and gate: the supervisor-socket alternative in D-04 cannot alone cover foreground preflight and manual-Ship waits. This leaves the three CLI check-to-write paths conditional.

8. **Preserving mismatched answers can permanently occupy the exclusive response slot.**

   D-05 requires wrong-ID responses to remain on disk (`50.1-CONTEXT.md:95–96`), while exclusivity remains mandatory. Today response paths are keyed only by phase/stage (`crates/devflow-core/src/gates.rs:130–135`); any existing response causes `AlreadyResponded` (`:219–222`) and hides the gate from `list_open` (`:170–172`).

   If those paths survive the overhaul, a late G1 writer publishing after cleanup can occupy G2’s response slot. G2 refuses that answer but cannot receive a valid one. Failed fresh-start cleanup produces the same obstruction.

   IDs can prevent the wrong decision here; they do **not** restore progress. Require ID-specific response storage or explicit preservation/quarantine and recovery semantics. Criterion 4 needs to demonstrate both rejection of the stale answer **and subsequent acceptance of a valid answer** for these two backlog paths.

No files were modified; no scratch files or background processes were created.
