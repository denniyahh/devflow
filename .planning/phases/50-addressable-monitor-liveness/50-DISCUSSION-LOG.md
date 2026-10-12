# Phase 50: Addressable Monitor Liveness - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-10-10
**Phase:** 50-addressable-monitor-liveness
**Areas discussed:** Supervisor scope, Backlog folding, 2.x state break, Proof bar

---

## Supervisor scope

**Teardown scope.** First ask: the operator asked for the pros and cons of "liveness + shutdown" and the full spike
design before choosing. Claude presented them, including the arguments against the full design: the risky backstop,
thin evidence on how often STALE happens, and untested agent survival.

| Option | Description | Selected |
|--------|-------------|----------|
| Shutdown + backstop | Socket shutdown for ALIVE; guarded pgid backstop for STALE; no sweep | |
| Full spike design | Shutdown + backstop + `~/.cache/devflow/*.sock` sweep | ✓ |
| Liveness + shutdown | No backstop; STALE reported, repair manual | |

**Signal handling**

| Option | Description | Selected |
|--------|-------------|----------|
| Same as shutdown | Trap TERM/INT → kill group, 143, suppress advance, remove socket | ✓ |
| Forward only | Forward to agent group; natural-exit path then runs advance | |
| No handler | Leave STALE for backstop/sweep | |

**Sweep authority**

| Option | Description | Selected |
|--------|-------------|----------|
| Report; act on request | doctor lists; explicit verb tears down | ✓ |
| Auto-reap STALE | Any sweep, including doctor, kills and removes | |
| Report only | Never acts | |

---

## Backlog folding

Claude stated that 999.134 and 999.138 are closed by the scope decisions. The operator did not object.

| Option | Description | Selected |
|--------|-------------|----------|
| 999.141 test HOME isolation | Prerequisite for sweep tests | ✓ |
| 999.128 lock reclaim start-time | Same identity primitive as backstop | ✓ |
| 999.139 corrupt state hides monitor | `start` refuses via sweep | ✓ |
| 999.130 split-lock overhaul | Trigger fired; phase-sized | |

**999.130 disposition**

| Option | Description | Selected |
|--------|-------------|----------|
| Re-park, note trigger | Record review, revisit after 51 | |
| Promote to new phase | Insert between 50 and 51 | ✓ |
| Keep parked, no note | Untouched | |

**999.143 remainder**

| Option | Description | Selected |
|--------|-------------|----------|
| Fold agent-pid identity | Start time beside agent pid in resume guard | |
| Leave parked | Note monitor half is gone | ✓ |
| Fold all of it | Identity plus work-preserving repair | |

---

## 2.x state break

| Option | Description | Selected |
|--------|-------------|----------|
| Explicit schema_version | `schema_version: 3`; absent = pre-3.0 | ✓ |
| Field presence | `monitor_pid` without supervisor block = 2.x | |
| Both | Version authoritative, presence as cross-check | |

| Option | Description | Selected |
|--------|-------------|----------|
| Read-only verbs report | status/doctor say incompatible; writers refuse | ✓ |
| Every verb refuses | Including status/doctor | |
| Auto-migrate | Rewrite in place on load | |

| Option | Description | Selected |
|--------|-------------|----------|
| Finish on 2.x, or discard | No new verb | |
| Explicit upgrade verb | Refuses while 2.x pids live; stamps v3, keeps worktree+branch | ✓ |
| Discard only | `recover --clean` | |

---

## Proof bar

| Option | Description | Selected |
|--------|-------------|----------|
| Minimal local live run | Scratch repo, no Ship, both variants, faults | |
| Full Phase-49 rig | Disposable GitHub repo, run to Ship | ✓ |
| Tests only | No live agent run | |

| Option | Description | Selected |
|--------|-------------|----------|
| Ship run + fault arm | Full run plus a separate short fault arm | ✓ |
| Ship run only | Faults by tests | |
| Faults inside the Ship run | One run, a failed recovery costs the attempt | |

| Option | Description | Selected |
|--------|-------------|----------|
| Carry unchanged | 49 D-10/D-11/D-18/D-19; fault arm not counted | ✓ |
| Carry, lower cap | Cap 2 | |
| Different rules | Free text | |

---

## Claude's Discretion

These were decided within Claude's remit and never put to the operator:
- socket keying;
- liveness presentation and `doctor --json` shape;
- wire protocol;
- coexistence with the pid and exit files;
- group enumeration on Linux;
- macOS (enforced by a constant, not claimed as supported);
- the upgrade verb's name;
- plan and wave layout.

## Deferred Ideas

- 999.130 is promoted to its own phase between 50 and 51.
- 999.143's agent half stays in the backlog.
- macOS verification has no host and no CI.

---

## Post-review: 2.x way out (2026-10-11, review finding O-1)

The operator first answered "I need help to determine what the best solution is". Claude then explained:
- what a live 2.x run does once 3.x is installed (it ends itself at its next stage boundary);
- where it really gets stuck (a run parked at a gate, and a recycled pid);
- that a cmdline check is safe for ruling a pid out but unsound for deciding to kill.

| Option | Description | Selected |
|--------|-------------|----------|
| Adopt recommendation | `stop` signal-only on 2.x; self-termination at the boundary; upgrade waits using cmdline exclusion; no override | ✓ |
| Recommendation + override | Same, plus `--assume-dead` | |
| Finish on 2.x | Keep D-14/D-15; clear a gate-parked run with a 2.x binary | |

Recorded as D-19 (amends D-14, D-15).

---

## Migration retired (2026-10-12)

The operator said they do not care about 2.x → 3.x migrations, since nobody else uses DevFlow, and asked to retire
all migration work unless it would interfere with Phase 50 or later development. Claude checked first:
- no DevFlow process was running;
- the only 2.x state files on the machine were inert: `trading_bot/.devflow/state-08.json` from 2026-07-19, with no
  recorded pids, and DevFlow's own gitignored `.devflow/state-stale.json`, which is not a phase state;
- the Phase 50 proof rig starts from a fresh throwaway repo, so it never meets a 2.x state.

Recorded as D-20. It retires D-13, D-14, D-15 and D-19 and ROADMAP criterion 4. The 2.x state break, the upgrade
verb and the O-1 tables above are kept as audit trail only.
