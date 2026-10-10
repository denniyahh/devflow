# Phase 49 runbook review — 2026-09-30

Two independent reports completed against 0c51f71. Antigravity and Hermes/DeepSeek returned exit 0;
pi/DeepSeek dropped before review because its provider key was unavailable. Hermes state.db confirms
model deepseek-flash. Both reports have source citations; neither is counted as a live-run proof.

## Confirmed and repaired

- Dead waiter plus stale gate blocked collection: commands.rs gate_respond refuses non-Ship answers
  without a possible live holder. Runbook now preserves the stale gate and permits allow-state only
  after positive waiter/monitor/agent death confirmation, including read-only doctor evidence.
- Pre-monitor null PID was valid state, not malformed: state.rs monitor_pid is optional and defaults
  to None. Exact runbook jq now distinguishes NULL/ABSENT/PID and rejects false/zero/string controls.
- SURV analysis was omitted: surv-watch.py analyze is the analysis.txt publisher. Both collection
  paths now invoke it after stopping the watcher.
- Producer contract fields omitted: EVIDENCE-LAYOUT promises created_at, CLI/runtime versions and
  negctl unit identity. A failing prepare test now covers all of them; the producer supplies them.
- Cleanup omitted the negative unit, and a retained exited start unit was not released: the runbook
  records/reclaims all owned named units after verifying no run processes remain. It explicitly
  distinguishes unit active(exited) from live processes and stops short of claiming descendant reaping.
- A shipped run with failed collection lookup had no documented recovery: preserve collection/verdict
  under a fresh non-overwriting suffix, repeat only collection from the same ended project and rejudge.

## Disputed, deferred and limits

- Gate sweep is age/live-waiter based and often a no-op after death. It is retained as a project-scoped
  diagnostic/aged response, with its limit stated; it is not claimed to remove stale files or registry.
- KillMode=process descendant survival is the explicit 49-10 plan review deferral. No process-model
  change was made; no allow-state or deletion is permitted over live or unidentified processes.
- The three-day default is confirmed by devflow-cli/config_parse.rs, not the stale seven-day comments
  in preflight/pipeline tests. No production constant or unrelated comment was changed.
- The watcher temp pattern matches the actual workflow.rs unique_temp_path_with formatter. Its
  synthetic event control does not prove coverage of all real scheduling or future naming changes.
- The suggested alternate-scaffold mismatch is not present on the documented path: prepare explicitly
  uses this phase's scaffold even for a fix checkout; later commands default to that same phase tree.
- Snapshot digests are recorded, but the arm checker verifies criterion fields rather than every raw
  snapshot manifest. The collection gate verifies its manifest; sanitized manifests describe prior
  bytes under 49-12. These are consistency checks, not authenticity against malicious evidence editing.
  No new cryptographic/provenance guarantee or retroactive checksum scheme was invented.
- A failure after auth while launching a run unit can leave partial unit/attempt state. The procedure
  requires diagnosis and confirmed-dead recording rather than an automatic launch retry; a launch
  failure alone is not absence proof. This remains a bounded operational limitation, not a green run.

## Recheck

The repairs are committed in the following fix revision and receive focused independent recheck.
Do not treat this initial review or local structural gates alone as live-run readiness.

## Focused recheck result

Both independent lanes completed at 6c935ed: Antigravity and Hermes/DeepSeek report no remaining
blockers in the six targeted repairs, with source citations and explicit static-review limits.
The exact runbook PID parser was separately exercised on NULL, ABSENT, valid PID, boolean, zero
and string controls; its invalid directions actually failed. Both task gates pass, all 25 Bash
blocks pass syntax checks without execution, and the metadata producer passes a real temporary
prepare with all required fields. No live positive attempt or external repo was created.

The DeepSeek claim that metadata's failing direction was unseen is a review-input limitation: the
actual RED run on db15a1f printed FAIL prepare and SELFTEST passed=16 failed=1 before the fix.
The run-unit start wrapper is a parent, not the detached monitor; core monitor.rs contains the
__monitor and Legacy advance paths, and the runbook refuses dead collection if any unit process
remains. Antigravity's statement that the evidence layout itself was updated is not repeated: it
already declared negctl, and the repair changes the producer to satisfy that existing contract.

Readiness strength: repaired procedure and tool logic reviewed on this snapshot, not a guarantee
of unattended execution, process cleanup, evidence authenticity or reliability. KillMode and raw
snapshot-manifest limits remain explicit. The operator-owned live attempt is still required.
