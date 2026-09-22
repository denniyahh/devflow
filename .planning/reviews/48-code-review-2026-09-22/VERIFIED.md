# Phase 48 production-code adversarial review — orchestrator verification

Review base: `3f6669b` (git archive snapshot; source unchanged during the run: the only snapshot delta
was a new `graphify-out/cache/stat-index.json`). Lanes: codex (`gpt-5.6-terra`, high, 9.5 min,
exit 0) and agy (`gemini-3.8-flash-high`, high, 60m timeout, 12.7 min, exit 0). Both lanes completed;
neither dropped. Every finding below was re-traced against source by the orchestrator before being
classified.

| ID | Finding | Lanes | Verdict | Evidence |
|----|---------|-------|---------|----------|
| A | Implicit `recover --clean` (no `--phase`) deletes a live, gate-waiting phase's state | codex C-1, agy C-2 | CONFIRMED | `recover::clean` never checks or takes the per-phase lock; its liveness test is the agent pid only, while a gate waiter's agent has exited (`crates/devflow-core/src/recover.rs` `clean` / `is_stale_state`). `clean_phase` takes the lock; `clean` does not. codex ran a proof test in a disposable copy (1 passed, with controls). |
| B1 | A late `Gates::respond` can publish an answer after the gate was consumed and cleaned up, and the next same-stage gate consumes it | codex C-2 | CONFIRMED (source-permitted interleaving, not observed) | `respond` checks gate-exists and response-absent, then `publish_response_exclusive` hard-links with no re-check and no gate identity (`gates.rs` `respond`, `publish_response_exclusive`); `write_gate` rewrites only the request and `poll_response` reads the response path on its first pass. Likelihood low: the check-to-link window must straddle a consume + `cleanup`. |
| B2 | `resume` leaves an unconsumed answer in place; a later gate at the same stage consumes it without a new decision | codex C-2 | CONFIRMED (path) | `pipeline_launch::tests::resume_relaunches_without_consuming_a_pending_gate_answer` proves the answer survives `resume`; its control `resume_preflight_refusal_consumes_a_pending_gate_answer` proves a re-fired same-stage gate consumes it. Reachable when a waiter dies within the poll backoff (≤60 s) after an answer is written. 48-CONTEXT D-05 names this hazard as closed by (3)+(4); neither covers this path. |
| C | Supervise re-scan rejection leaves its response, and the fall-through stage-failure gate consumes it | agy C-1 | REFUTED as a defect — intended | The Supervise `LoopBack` arm deliberately skips `Gates::cleanup` ("the unchanged response is the human's request to repair", `pipeline_launch.rs` re-scan match), and `rejecting_the_rescan_gate_records_nothing_and_falls_through` pins it ("The fall-through gate below consumes this same response, which is correct: it is the same question"). Design note only: one answer resolves two gates in the event stream. |
| D | `recover --clean --phase N` prints "cleaned up workflow state" and exits 0 when the lock was contended and nothing was deleted | agy C-3 | CONFIRMED | `recover_cmd` prints the success line unconditionally after the warnings and returns `Ok(())` (`crates/devflow-cli/src/commands.rs` `recover_cmd`). The implicit sweep has the same unconditional line. |
| E | `gate approve|reject` writes a Ship response for a `Recycled` (or `Unconfirmable`) holder | agy C-4 | CONFIRMED | `gate_respond` skips the holder check whenever `stage == Ship` (`if stage != Stage::Ship && !holder.may_be_waiting()`), so the NoHolder-only Ship exception applied in `stop_via_gate` is not applied here. Contradicts `48-SECURITY.md` T-48-16-03 ("Recycled holders receive no response at every gate, including Ship"). Only the non-Ship recycled case is tested (`gate_respond_with_a_recycled_lock_pid_at_a_non_ship_gate_writes_nothing`). |

Checked-and-clean claims from both lanes (lock coordination, hard-link first-writer-wins for one
pathname, shared checkpoint parser, child-process harness non-vacuity, stage binding) were not
contradicted by anything found here; they are the lanes' bounded claims, not independent proofs.
