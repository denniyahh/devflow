### Executive Summary

All six targeted Phase 49 runbook repair items are verified as implemented and mechanically sound against the runbook and supporting scripts. No active collection deadlocks, silent state crashes, missing metadata, unpreserved recollections, or ambiguous unit sweeps remain in the reviewed scope.

---

### CONFIRMED REMAINING BLOCKERS

**None** in the targeted repair surface.

*(Note: Accepted plan-level deferrals—such as `KillMode=process` descendant leaks and deferred `cap_reached` provenance—remain deferred by design and are listed under Residual Limits).*

---

### FIXES VERIFIED

1. **Dead-Waiter / Stale-Gate Collection Deadlock**
   * **Verification**: [49-RUNBOOK.md:250-254](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L250-L254) explicitly documents that a leftover gate file does not constitute a parked run, and forbids deleting or answering stale gates to unblock collection.
   * **Mechanism**: [49-RUNBOOK.md:258-281](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L258-L281) prescribes read-only diagnosis (`devflow doctor --json`) to positively confirm waiter absence alongside monitor/agent process checks.
   * **Collection**: Once process and waiter absence is confirmed, [49-RUNBOOK.md:283-298](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L283-L298) and [collect-attempt.sh:300-304](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/collect-attempt.sh#L300-L304) permit collection using `--allow-state` and `--stop-note`. The stale gate files and `.devflow/state-01.json` are retained in the evidence record without mutating or resuming the stopped attempt.

2. **Null `monitor_pid` Mishandling**
   * **Verification**: [49-RUNBOOK.md:260-268](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L260-L268) parses `state-01.json` using strict `jq` type branching, segregating missing fields (`ABSENT`), JSON `null` values (`NULL`), and valid integer PIDs (`>= 1`).
   * **Behavior**: [49-RUNBOOK.md:265-274](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L265-L274) prevents invalid `ps -p null` calls, explicitly noting that `NULL` indicates pre-spawn state rather than corruption, while invalid types error immediately. Liveness must be corroborated via cgroup and agent-PID file inspection rather than relying on PID presence alone.

3. **SURV Concurrency Analysis Integration**
   * **Verification**: [49-RUNBOOK.md:156-159](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L156-L159) (normal exit) and [49-RUNBOOK.md:288-289](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L288-L289) (failure/death path) explicitly execute:
     ```bash
     systemctl --user stop "p49-a$N-surv"
     python3 "$T/surv-watch.py" analyze "$EV/surv"
     ```
   * **Output**: [surv-watch.py:21-29, 258-300](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/surv-watch.py#L21-L29) processes `watch.jsonl` into `analysis.txt`, checking for overlapping atomic write windows on `.state-01.json.*.tmp` without overclaiming absence of evidence as proof of safety.

4. **Metadata Additions (`created_at`, Tool Versions, `negctl` Unit)**
   * **Verification**:
     * `launch-attempt.sh:242-250`: Explicitly records `claude_version`, `gh_version`, `gsd_version`, `python3_version`, and `git_version` in `$ev/identity.txt`.
     * `launch-attempt.sh:256-257`: Injects epoch `created_at` timestamp directly into `$ev/attempt.json`.
     * `launch-attempt.sh:258`: Writes `negctl=p49-a$attempt-negctl` into `$ev/units.txt` alongside `start`, `surv`, and `authprobe`.
     * `launch-attempt.sh:78-81`: Selftest controls explicitly validate `created_at` integer type, all 5 version entries in identity, and the `negctl` unit mapping.
     * `EVIDENCE-LAYOUT.md:19`: Evidence specification updated to document all unit tracking keys.

5. **Retained-Unit and Artifact Cleanup**
   * **Verification**: [49-RUNBOOK.md:346-357](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L346-L357) specifies exact user unit targets (`p49-a$N-start`, `p49-a$N-surv`, `p49-a$N-negctl`) for `stop` and `reset-failed`.
   * **Scope Isolation**:
     * Gate sweep is strictly bounded via `--root "$EV/project"`, with an explicit note that sweep only cleans aged gates with live waiters and may legitimately be a no-op on dead runs.
     * Stale registration removal ([49-RUNBOOK.md:359-362](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L359-L362)) targets `~/.cache/devflow/roots/` by inspecting `project_root` and `phase`, removing solely the file matching `$EV/project` and phase 1.
     * Attempt removal ([49-RUNBOOK.md:373-376](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L373-L376)) forbids wildcards and requires prior identity and sanitized-evidence verification.

6. **Preserved Recollection on Transient Collector Failure**
   * **Verification**: [49-RUNBOOK.md:187-206](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L187-L206) establishes an archival retry flow for transient collection failures (e.g. missing GitHub `pr.json`):
     ```bash
     STAMP=$(date +%s)
     ...
     mv "$EV/collected" "$EV/collected.before-$STAMP" &&
     mv "$EV/verdict.json" "$EV/verdict.before-$STAMP.json" &&
     bash "$T/collect-attempt.sh" --attempt "$N" &&
     bash "$T/check-evidence.sh" --attempt-gate "$P49"
     ```
   * **Behavior**: Moving `$EV/collected` clears the `collected/DONE` short-circuit guard in [collect-attempt.sh:305-307](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/collect-attempt.sh#L305-L307) while archiving the original collection directory and verdict without data loss.

---

### RESIDUAL LIMITS

In accordance with RULE ZERO (b), the following boundaries and non-claims apply:

1. **Process Reaping (`KillMode=process`)**:
   Stopping systemd units does not guarantee destruction of orphan descendant processes spawned during execution. This limitation remains explicitly deferred per Plan 49-10 ([49-RUNBOOK.md:301-303](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L301-L303)).
2. **Survival Observation Scope (`SURV-01`)**:
   `surv-watch.py` analyzes observed state writes within a single run. An unflagged `analysis.txt` establishes only that no concurrency violation was recorded during that specific execution; it does not constitute a generalized mathematical proof of `SURV-01` across untested timings or platforms ([surv-watch.py:26-29](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/surv-watch.py#L26-L29), [49-RUNBOOK.md:380-382](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L380-L382)).
3. **PID Recyling Boundaries**:
   Checking `ps` against `monitor_pid` or `phase-01-agent-pid` cannot rule out PID reuse if sufficient delay elapses between process exit and inspection. Corroboration against start time (`lstart`), command line (`args`), and cgroup membership remains necessary.
4. **Gate Sweeping Semantics**:
   Running `gate sweep` after confirming process death will typically be a no-op because sweep requires an active waiter holding the lock. The runbook explicitly acknowledges this ([49-RUNBOOK.md:355-357](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md#L355-L357)). Stale files are cleared solely via subsequent attempt directory deletion.
5. **Collection Provenance vs Digest Verification**:
   The arm checker verifies structural presence and schema validity across ledger records, not cryptographic snapshot digest chains. Raw collected data integrity is pinned by collection-level manifests ([EVIDENCE-LAYOUT.md:21](file:///.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/EVIDENCE-LAYOUT.md#L21)).
