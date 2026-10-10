Warning: Unknown toolsets: moa, rl
P49 REPAIRS RECHECK — static read of the bounded snapshot only. No command executed: no selftest was run, no unit/auth/GitHub/DevFlow action taken. Everything below is source inspection, not execution evidence. Tool accounting: 12 attempted reads, 10 returned content; 2 returned nothing (one terminal command blocked by the security scanner, one rg search failed on an environment locale fault).

CONFIRMED REMAINING BLOCKERS
None. I found no repair that is still absent and no path that still deadlocks end to end on the runbook's own instructions.

FIXES VERIFIED (by reading, not by running)

1. Dead-waiter / stale-gate collection deadlock — repaired.
49-RUNBOOK.md:248-302 now separates "gate file on disk" from "parked run": a dead waiter can leave a stale Define/Code gate that `gate reject` refuses (251-253); the operator must not delete or answer it; "Only after positively confirming all run processes are gone, no waiter remains and no shipped event exists, a remaining gate file is stale evidence" (283-284); "Never pass --allow-state while a waiter, monitor or agent is alive or unconfirmable" (296); "Preserve stale state and gate files for the collector" (297). The escape hatch exists in code: collect-attempt.sh:273 parses `--allow-state`, 299-304 refuse only while `state-01.json` exists and die otherwise, and the collector never inspects gate files, so a stale gate no longer blocks collection. The prior deadlock is closed and 49-RUNBOOK.md:181 routes a state-remaining normal case explicitly into the death procedure.

2. NULL monitor_pid — repaired, and consistent with the binary.
Runbook 260-268 discriminates tri-state with `jq -er`: missing key → ABSENT, `null` → NULL, integer ≥1 → PID, anything else → `error()`; 273 states NULL is valid pre-spawn and never proves death by itself; 265-268 only runs `ps` for a real PID. Source agrees: commands.rs:1009-1021 `liveness(None) → Liveness::Unknown` (explicitly not `Stuck`), commands.rs:3697-3700 the field is `Option<u32>` (so the integer type check is the right one), 1111-1118 renders PID vs `none`.

3. SURV analysis step — present in both paths.
49-RUNBOOK.md:158 (shipped) and :289 (dead) run `surv-watch.py analyze "$EV/surv"`; :161 forbids inferring interleaving from absence. surv-watch.py:384-396 implements analyze with atomic publish.

4. created_at / tool versions / negctl unit metadata — recorded and asserted.
launch-attempt.sh:241 writes build_commit/build_dirty/bin_sha256; :243-250 appends claude_version, gh_version, gsd_version, python3_version, git_version; :254-257 gives attempt.json a `created_at`; :258 writes units.txt including `negctl=p49-aN-negctl`. Selftest :76-81 asserts created_at is an int > 0, all five version keys present and non-empty with fixture-exact values, and `units['negctl']=='p49-a1-negctl'`; a miss fails the SELFTEST (173).

5. Retained-unit cleanup — no longer ambiguous.
Runbook 161-167 releases the exited start unit only after confirming `workflow_shipped` and no monitor/agent; 269-270 collect unit status/MainPID/ControlGroup as evidence; 300-302 explain that RemainAfterExit can read "active (exited)" and that unit activity proves neither life nor death; 346-354 restrict stop/reset to this attempt's units, point at units.txt for their names, and state gate sweep may legitimately be a no-op that removes neither stale gate files nor registry entries; 359-362 give the exact registry-file removal rule honoring DEVFLOW_CACHE_DIR/XDG_CACHE_HOME and forbid wildcard sweeps. This matches the "unit activity is not proof" requirement.

6. Preserved recollection path after a shipped run's transient collector failure — repaired.
Runbook 187-206: diagnose the checker reason, then a STAMP guard refuses if `$EV/collected.before-$STAMP` or `$EV/verdict.before-$STAMP.json` already exists (193-196); both the directory and the verdict file are moved aside before re-collecting (197-199); the attempt gate is re-run (200); "Preserve both versions and record why collection was repeated" (204) and "Do not rewrite collected files to satisfy a manifest or overwrite old evidence" (205-206). The mv is genuinely required because collect-attempt.sh:305-308 returns "already collected" on DONE and :309 dies on a partial `collected/`, so no silent overwrite is possible and the guard prevents clobbering a prior archive.

RESIDUAL LIMITS (no action demanded of you; listed so nothing reads stronger than it is)

a. SURV analysis is instructed, not enforced, and is outside the manifested evidence. Its output is `$EV/surv/analysis.txt` (launcher:323-325), outside the project and outside `collected/`. Collect-attempt.sh (all 732 lines read) copies only `.devflow`, sessions, git.txt, pr.json, evidence.json; write_manifest (242-251) covers only that tree; check-evidence.sh (all 456 lines read) never mentions surv or analysis.txt. So skipping `analyze` fails nothing, and a shipped verdict does not require or hash the SURV observation. Whether a later sanitize step archives `$EV/surv` is unverified — I did not read EVIDENCE-LAYOUT.md, sanitize-evidence.py or 49-11/49-12 (my EVIDENCE-LAYOUT search call failed).

b. The death checks are procedural, not mechanized. `--allow-state` is accepted on the operator's word; neither the collector nor any checker re-derives waiter/monitor/agent absence. The runbook counteracts this with doctor + ps + cgroup inspection, but the guard is human judgement.

c. Monitor identification spellings may not cover this launcher's own wrapper. Runbook 276-277 accepts two shapes: a `__monitor` process naming project/phase, or an `sh -c` wrapper naming the project's phase-01 files and ending with `advance --phase 1`. launch-attempt.sh:328-330 creates the run unit as `sh -c 'exec "$0" start --phase 1 --agent claude --mode auto --yes-ship …'`, which ends with the binary and project/ev paths, not `advance --phase 1`. I did not read the code that spawns the monitor, so I cannot say whether a surviving monitor matches either spelling — an unverified identification gap, not a demonstrated defect, and the runbook's fallback (280-281: if absence cannot be established, stop and diagnose) covers it.

d. The metadata control's red direction is untested. launch-attempt.sh:76-81 is a positive fixture check with distinctive version strings; no case omits a version key or created_at and asserts the check goes red. The assertion is wired, but its firing on missing metadata is not demonstrated in the selftest I read.

e. "Gate sweep may be a no-op" is unverified against source — my gate-sweep grep was the blocked call. The advice is conservative either way, so the risk is a wrong rationale, not a wrong action.

Accepted limits I did not re-litigate: KillMode=process descendant strays (49-10 deferral), cap_reached/VERIFY-01, manifest vs sanitized-manifest scope, and the three-day default confirmed at config_parse.rs:19-20/:42 (THREE_DAYS = 3*24*60*60), consistent with 49-RUNBOOK.md:115-116; the source comment at config_parse.rs:109-110 still narrates the old seven days historically.

STILL OPEN: nothing requires your decision. Item (c) is the only one I would spend a further read on — the monitor-spawn path in commands.rs — if you want that gap closed rather than carried.
