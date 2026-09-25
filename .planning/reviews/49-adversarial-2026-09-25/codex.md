Reviewed commit `0cf90204a6cc16d4112dd6f9ef15e8cb25af85c2`.

1. **CRITICAL — the required container gate is known not to work from the linked worktree.**

   **Required property:** Ship must pass the pinned container check before approval.

   **Evidence:** [49-12-PLAN.md:96-100](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-12-PLAN.md:96) runs `scripts/check-in-container.sh all` from `.worktrees/phase-49.1`. The repository’s own [ROADMAP.md:4161-4177](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/ROADMAP.md:4161) records that this fails deterministically from linked worktrees because the mounted `.git` file points outside the container.

   **Fix hint:** Run the gate from the main checkout against the intended worktree, or resolve 999.58 before making it a Ship prerequisite.

2. **HIGH — retries are blocked by the previous attempt’s state file.**

   **Required property:** A failed attempt can be archived and relaunched with `--force`.

   **Evidence:** [49-08-PLAN.md:123-126](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-08-PLAN.md:123) requires no `state-49.1.json` for every attempt. The archive action only copies `.devflow` artifacts ([49-08-PLAN.md:216-224](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-08-PLAN.md:216)); it does not clear the live state file.

   **Fix hint:** Make archival transactionally preserve then clear the prior state, or make the N≥2 precondition explicitly accept the archived state.

3. **HIGH — archive failure can permanently prevent the retry.**

   **Required property:** Partial archival must be recoverable.

   **Evidence:** `archive-attempt.sh` writes the archive ref first and refuses an existing ref ([49-08-PLAN.md:216-224](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-08-PLAN.md:216)). If copying or manifest creation fails afterward, the ref remains, while the next archive attempt is rejected as already archived.

   **Fix hint:** Stage the archive and manifest first, then create the ref atomically; allow idempotent continuation when the ref points to the same tip.

4. **HIGH — cleanup does not match the chosen systemd ownership mode.**

   **Required property:** Stopping or aborting the run must stop the monitor/agent descendants.

   **Evidence:** The positive unit uses `KillMode=process` ([49-08-PLAN.md:78](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-08-PLAN.md:78)), explicitly because it keeps the monitor chain alive after `start` exits ([49-08-PLAN.md:81-83](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/49-live-unattended-run-the-milestone-s-instrument/49-08-PLAN.md:81)). Cleanup later only stops the units ([49-13-PLAN.md:169-174](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-13-PLAN.md:169)).

   **Fix hint:** Use a dedicated keepalive owner plus an explicit control-group cleanup path, or prove descendant reaping separately.

5. **HIGH — the final base-head invariant is not checked before Ship approval.**

   **Required property:** `workspace/denniyahh` must still equal the recorded fork base immediately before the Ship-preflight approval.

   **Evidence:** Task 1 checks `HEAD == base_sha` before accommodations ([49-12-PLAN.md:88-100](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-12-PLAN.md:88)). Task 2 only asks for a clean status before approval ([49-12-PLAN.md:125-135](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/run-the-milestone-s-instrument/49-12-PLAN.md:125)), not the unchanged HEAD check required by D-13.

   **Fix hint:** Re-read and record the branch and HEAD immediately before the approval command.

6. **HIGH — the public-PR secret scan omits `ANTHROPIC_TOKEN`.**

   **Required property:** The PR body must be checked for all known secret-shaped environment values.

   **Evidence:** [49-12-PLAN.md:164-166](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-12-PLAN.md:164) scans `ANTHROPIC_API_KEY=` but not `ANTHROPIC_TOKEN=`. The research security register explicitly names both variables ([49-RESEARCH.md:844](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RESEARCH.md:844)).

   **Fix hint:** Scan both names, plus the existing secret-pattern set, before any public PR approval.

7. **HIGH — “worktree-only” accommodation changes repository-wide Git configuration.**

   **Required property:** No main-checkout or other-worktree behavior changes.

   **Evidence:** The must-have claims only the 49.1 worktree changes ([49-06-PLAN.md:20](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-06-PLAN.md:20)), but the action explicitly runs `git -C $ROOT config extensions.worktreeConfig true` ([49-06-PLAN.md:109-117](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-06-PLAN.md:109)). The plan itself identifies this as repository-level configuration ([49-06-PLAN.md:207-210](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-06-PLAN.md:207)).

   **Fix hint:** State and test the repo-wide mutation explicitly, including effects on every existing worktree, rather than calling it worktree-only.

8. **HIGH — `record-secure` can revert unrelated planning changes.**

   **Required property:** Reverting the security accommodation must remove only security-review changes.

   **Evidence:** It accepts any non-empty commit range where every changed path is under `.planning/` ([49-06-PLAN.md:147-151](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-06-PLAN.md:147)), then reverts the whole range ([49-06-PLAN.md:180-187](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-06-PLAN.md:180)). That permits unrelated STATE/ROADMAP/plan edits to be reverted on the base.

   **Fix hint:** Require an exact path set or record and revert only the SECURITY.md commit(s).

9. **HIGH — polling can miss a complete gate lifecycle.**

   **Required property:** Every opened gate is notified and snapshotted.

   **Evidence:** The watcher polls every five seconds ([49-03-PLAN.md:117-126](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-03-PLAN.md:117)); the only control runs two passes against a persistent gate ([49-03-PLAN.md:96-98](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-03-03-PLAN.md:96)). A gate created and answered between polls is invisible.

   **Fix hint:** Watch gate directory events or add a control that creates and answers a gate between polling passes, with the event ledger as the authoritative fallback.

10. **MEDIUM — the shared evidence ledger has concurrent unsynchronized writers.**

    **Required property:** `ledger.jsonl` remains valid and preserves every record.

    **Evidence:** `gate-watch.sh` appends `gate_open` records ([49-03-PLAN.md:120-125](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-03-PLAN.md:120)); `gate.sh` independently appends `operator_response` records ([49-08-PLAN.md:183-187](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-08-PLAN.md:183)). No lock or atomic append protocol is specified.

    **Fix hint:** Serialize ledger writes with a lock or write per-source ledgers and merge them deterministically.

11. **MEDIUM — SURV-01 can be reported as valid despite an invalid watcher window.**

    **Required property:** A C5 result must be `NOT_EXERCISED` when the watcher stopped uncleanly or saw an overflow.

    **Evidence:** The instrument defines `void_windows` and `watcher_stopped_cleanly` ([49-04-PLAN.md:20-24](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-04-PLAN.md:20)), but the final verification only checks `instrument_saw_nothing` and overlap count ([49-13-PLAN.md:169-182](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-13-PLAN.md:169)).

    **Fix hint:** Fail or downgrade C5 whenever `void_windows` is non-empty or `watcher_stopped_cleanly` is false.

12. **MEDIUM — evidence sanitization is manual and under-verified.**

    **Required property:** Every committed excerpt is reproducibly sanitized and every verdict is backed by an evidence file.

    **Evidence:** The plan says to copy “sanitised excerpts” and manually trim the resumed message ([49-15-PLAN.md:113-125](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-15-PLAN.md:113)), but its gate only checks section names, a home-path/secret regex, and file count ([49-15-PLAN.md:148-150](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-15-PLAN.md:148)). It does not verify UUID/session-ID redaction, regular-file provenance, or that each verdict cites evidence.

    **Fix hint:** Use a deterministic sanitizer, reject symlinks, scan UUIDs/usernames, and mechanically require one evidence path per verdict.

13. **MEDIUM — the binary build is not explicitly bound to the recorded fork SHA.**

    **Required property:** The pinned binary and both arms must be built from the same commit as the fork point.

    **Evidence:** The launch sequence builds first and records/rechecks the base later ([49-08-PLAN.md:133-142](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-08-PLAN.md:133)). The research says to build from the exact fork SHA ([49-08-PLAN.md:84-86](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-08-PLAN.md:84)), but no acceptance check compares `build_commit` to `base_sha`.

    **Fix hint:** Capture `base_sha` before the build and require equality with the binary’s embedded build commit.

14. **MEDIUM — immutable watcher copies are not the tools used for later evidence judgments.**

    **Required property:** The same tool bytes that ran the attempt must judge and answer it.

    **Evidence:** Watchers use byte copies under `$EV/tools` ([49-08-PLAN.md:87-90](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-08-PLAN.md:87)), but live plans hardcode the mutable phase-worktree `TOOLS` path ([49-10-PLAN.md:60-66](/var/home/denniyahh/Github/devflow/.worktrees/phase-49/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-10-PLAN.md:60)) for snapshots, gate commands, and evidence checks.

    **Fix hint:** Use `$EV/tools` for every attempt-scoped command and record tool checksums in `attempt.json`.

Static review cannot establish whether Claude follows either prompt instruction, whether the session-resume path actually fires, whether `/gsd-ship` reaches push/PR after the accommodations, whether the linked-worktree container failure reproduces in this exact runtime, whether the inotify watcher survives the live load, or whether the 999.55 flake rate changes after merge. Those require live execution and post-merge CI evidence.