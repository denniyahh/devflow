# Phase 50.1 CONTEXT — external adversarial review (2026-10-23)

**Target:** `50.1-CONTEXT.md` and ROADMAP Phase 50.1 criteria at `1516170`, reviewed in a `git archive` snapshot.
**Lanes:** agy (`gemini-3.8-flash-high`, high), DeepSeek (`pi`, `deepseek-flash`), codex (`gpt-6.1-sol`, high). All three
returned in full (19.9 KB, 12.5 KB, 8.7 KB), none dropped, and none modified the snapshot (codex left only a cargo
`target/`). The prompt is `prompt.txt`.

The orchestrator (Claude) checked every finding marked *Verified* below in source before folding it in. The others
were judged on their reasoning.

## Folded into CONTEXT / ROADMAP

| # | Finding | Lanes | Where it landed |
|---|---------|-------|-----------------|
| 1 | Before the socket exists, a pending record probes GONE, so A7/C4 move rather than close (Verified: `supervisor.rs:341-343`, `commands.rs:3377-3378`, no pid in record) | all | D-01: an inherited per-launch liveness `flock` gives live / abandoned |
| 2 | Two launchers both pass the guard once the long lock goes | pi, agy | D-01: atomic check-and-reserve under the write lock (mandatory) |
| 3 | One supervisor slot cannot name both S(N) and pending S(N+1) | pi | D-01: separate pending field |
| 4 | A failed spawn wedges the run: `stop` never clears the record | agy | D-02: `stop` clears an abandoned reservation; criterion 1 control |
| 5 | The agent pid file already refuses A7/C4 fixtures on unchanged code (Verified: `commands.rs:3291-3294`, `monitor.rs:1493-1503`) | codex | D-01 fixture note; criterion 1 |
| 6 | Keeping the late-answer promise via resume *is* AR-48-04 path (a); resume reruns the stage (Verified: `pipeline_launch.rs:2334-2335`) | all | **D-06 revised by the operator**: the rerun asks again, the unused answer is quarantined, Ship's late path stays through `devflow ship` |
| 7 | A refused answer left on disk wedges the single response slot (Verified: `gates.rs:131-136`, `:170-172`, `:219-222`) | pi, codex | D-05: response storage keyed by id, refused answers quarantined; criterion 4 shows that a valid answer is then accepted |
| 8 | Five `holder_status` "waiter live" checks go stale without the long lock (Verified: `commands.rs:1535-1546`, `:1857-1859`) | pi, agy | D-04: the waiter holds a registration `flock` |
| 9 | Foreground waits (preflight, `devflow ship` retry) have no socket | agy | D-04: they register too |
| 10 | A writer re-reading the id at publish can relabel a G1 decision onto G2 | codex | D-04: pass the observed id and re-check it |
| 11 | `ship_override` is a second consumer, separated today only by the long lock | codex | D-04: an exclusive ack publish is the one consumption claim |
| 12 | A superseded supervisor still consumes, acks and cleans up gates, and its agent keeps running | agy, codex | D-03: the fence covers gate side effects and next spawn; it ends its own agent group; criterion 5 |
| 13 | The launch id already travels by argv; `record_supervisor_stop` already binds by launch id (Verified: `pipeline_launch.rs:1430-1458`) | pi | D-03 corrected; a crash row is needed for superseded-but-live |
| 14 | "Re-read keeps hand edits without locking" overclaims: a racing edit can be lost | codex | D-08 narrowed; the residual stays an accepted gap (48 D-03) |
| 15 | One rename helper breaks first-wins responses; deletions and the sidecar link are not durable | codex, agy | D-09: three operations (replace, exclusive publish, remove), sidecar dir fsync, the scan covers all four syscalls with a planted control |
| 16 | `flock` is inherited across fork | pi | D-08: the write lock is close-on-exec (the D-01 liveness lock is inherited on purpose) |
| 17 | Criteria 1, 2, 3, 5 lack negative controls | agy, pi | ROADMAP criteria 1–5 tightened |
| 18 | The rig scripts drive gates through writers whose check changes | pi | D-04: plan task to update `50-RUNBOOK.md` and `negctl.sh` |
| 19 | "600 s" read as the hold time | agy | D-08 clarified: 600 s to acquire, held up to 3 days |
| 20 | The `preflight.rs:1098` citation is a doc comment | pi | D-07 citation corrected |

## Not adopted

| Finding | Lane | Reason |
|---------|------|--------|
| D-13 (fold 999.166) and D-12 (live run) are scope leaks | agy | Operator decisions in this discussion, made knowingly |
| D-10 (allow `devflow-core` breaks) is unconstrained | agy | Claude's remit, conditioned on cutting 3.0.0 after 50.1 and listing every break from `cargo semver-checks` |
| D-11 (retire `migrate_legacy_state`) is unrelated cleanup | agy | It is an unlocked state writer, and Phase 50 D-20 removes the reason to keep it |
| Criterion 6's "accepted with a reason" is an escape hatch | agy | Kept as 999.166's own done-when; D-13 now requires any acceptance to go to the operator |
| The n=1 live run licenses nothing about the phase's properties | pi | Agreed and recorded in D-12; the run stays as a smoke check by operator decision |

## Limits of this review
- Three reviewers, one pass, reading a document plus code at one commit. Nothing was built or run except codex's
  `cargo` metadata.
- The new mechanisms D-01/D-04 introduce (inherited liveness `flock`, registration `flock`, quarantine) have not been
  reviewed. The plan-set review after `/gsd-plan-phase` is where that happens.
