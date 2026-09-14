# Phase 48: Survivable State Writes and Honest Gate Recovery - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-14
**Phase:** 48-survivable-state-writes-and-honest-gate-recovery
**Areas discussed:** Concurrent state writes, Is anyone waiting?, Checkpoint check + re-scan, PATH isolation

---

## Concurrent state writes

### What must the final state file guarantee when two writers race?

| Option | Description | Selected |
|--------|-------------|----------|
| No write lost | A save that finds the file changed since load never overwrites; re-read and re-apply, or fail loudly | |
| Operator actions survive | A named list of operator-owned fields is re-read and kept on every save | |
| No torn writes only | Unique temp names; last writer still wins; amends criterion 2 | |

**User's choice:** Free text — "What is the use case for two writers writing to the state file? Is this a valid use case?"
**Notes:** Claude verified that the design intends one writer per phase (the per-phase lock), but `start` and
`stop` write without it; neither overlap has been observed and neither is caused by the shared `.tmp` name.
Options were reframed as: enforce one writer / make concurrent writes safe / unique temp only. The user chose
**1 — enforce one writer** (D-01).

### How should `start` take the lock without stranding a fast-exiting agent's single `advance`?

| Option | Description | Selected |
|--------|-------------|----------|
| Hold + advance waits | `start` holds the lock like `resume`; `advance` waits a bounded time | ✓ (after discussion) |
| Hold like resume only | Smallest; a fast-exiting agent's `advance` is refused silently | |
| Release before spawn | Hold through preflight only; post-spawn pid write re-checks the lock | |

**User's choice:** Asked for pros and cons of options 1 and 3; then asked whether a foolproof design exists.
**Notes:** Claude presented a split-lock overhaul (short `flock` for writes, compiler-enforced update API,
token handoff, waiter registration, fsync) and the gaps no file-based design closes (non-cooperating writers,
alive-but-hung, Linux-only process identity, network filesystems). Options A (overhaul in Phase 48), B (split
into 48.1), C (don't adopt). The user said they did not want to over-engineer and asked for a recommendation.
Claude recommended option 1 with the gaps recorded and the overhaul parked for Phase 50 with revisit triggers.
**User:** "Agreed" (D-02, D-03).

### Gate response files — same fixed `.tmp` name, several writers by design

| Option | Description | Selected |
|--------|-------------|----------|
| First answer wins | Unique temp plus exclusive hard-link publish; second responder refused loudly | ✓ |
| Unique temp only | No torn responses; two answers still last-writer-wins | |
| Leave gates.rs alone | State files only; gate race to backlog | |

**User's choice:** First answer wins (D-04).

---

## Is anyone waiting?

### When `stop` or `gate reject` finds nobody waiting, what should it do?

| Option | Description | Selected |
|--------|-------------|----------|
| Say so, name resume | Message-only, criterion 3 as written | |
| stop finishes the abort | `stop` takes the lock and calls `abort()` itself | |

**User's choice:** Asked for help reasoning about the pros and cons; then asked Claude to explain the problem
and why it is needed.
**Notes:** Claude verified that `resume` relaunches the saved stage and never reads a pending answer, so it is
the working repair only for a preflight-refusal gate; `devflow ship --phase N` covers Ship gates;
stage-failure and Validate gates have no consumer; an unread answer persists and decides the next firing of
that gate; `devflow recover --clean --phase N` already ends a dead phase but leaves gate files behind. Claude
dropped the inline `stop` abort and recommended a smaller version: honest gate-aware messages, no answer
written at non-Ship gates, `recover`/`start` delete leftover gate files, `doctor` uses the same repair lines,
criterion 3 reworded. **User:** "Yes" (D-05).

---

## Checkpoint check + re-scan

### How should a run tell that a human-only checkpoint was added or changed after Code's preflight?

| Option | Description | Selected |
|--------|-------------|----------|
| Compare to what was checked | Record the checkpoint set at preflight pass or approval; new or changed parks at a human gate | ✓ |
| Re-run the preflight check | No record; re-gates checkpoints already approved | |
| Stop on any plan change | Hash plan files; stops runs for harmless plan edits | |

**User's choice:** Compare to what was checked (D-07).
**Notes:** Claude decided within remit, and stated during discussion: the shared parser rules (D-06) and who
may change the recorded set (first Code evaluation, then only on a human approval).

---

## PATH isolation

### How should tests that replace `PATH` be isolated?

| Option | Description | Selected |
|--------|-------------|----------|
| Child process per test | Generalize Phase 46's helper; no production change | ✓ |
| Pass PATH into production | Explicit search path threaded through spawns and lookups | |
| Guards and locks only | NeutralPath plus env_lock everywhere | |

**User's choice:** "Option 1 sounds the best but help me make sure whether it is or just commensurate with
option 2", then "Option 1" (D-08).
**Notes:** Claude verified that option 2 is technically feasible (std searches a `PATH` set on a `Command`)
but would touch 14 direct production spawns, the `hermetic_command` constructor and the in-process lookup, and
fails open for missed or new spawn sites; option 1 is safe by construction and already proven in Phase 46.
Child-process cost was not measured.

### How far beyond PATH, and should a lint stop env mutations creeping back?

| Option | Description | Selected |
|--------|-------------|----------|
| PATH only + lint | clippy `disallowed-methods` ban; ~40 non-PATH sites get explicit `#[expect]` | ✓ |
| PATH only, no lint | Verification counts zero PATH changes | |
| All env changes, then ban | Convert all ~40, ban outright, delete ENV_MUTEX | |

**User's choice:** PATH only + lint (D-09).

### 999.80 — tests protected from spawning a real agent only by an "abort" note

| Option | Description | Selected |
|--------|-------------|----------|
| Fold in: child + neutral PATH | Structural protection using D-08's mechanism | ✓ |
| Assert the abort only | Checked invariant, still content-dependent | |
| Keep 999.80 separate | Sweep leaves those tests alone | |

**User's choice:** Fold in (D-10).

---

## Requirement IDs (raised during scouting, answered before writing context)

| Option | Description | Selected |
|--------|-------------|----------|
| New IDs for Phase 48 | CHKPT-01, CHKPT-02, TEST-01; #184/#170 keep GATE-01/GATE-02 | ✓ |
| Rename the deferred items | Roadmap keeps GATE-01/02; #184/#170 renamed | |
| Backlog numbers only | Drop GATE labels; cite 999.x directly | |

**User's choice:** New IDs for Phase 48 (D-11).

---

## Claude's Discretion

- `advance` lock-wait bound value.
- Whether a cheap "caller holds the lock" check in `save_state` is feasible.
- Exact D-05 message wording.
- Normalization and storage of D-07's checkpoint set.
- Shape of the generalized child-process helper.
- Wave placement of D-08's conversion; plan split and waves.
- Within-remit decisions stated during discussion: #200 reproduction test shape, D-06 shared parser rules,
  D-07 set-change rule and tests, TEST-01 evidence standard.

## Deferred Ideas

- Split-lock state-handling overhaul — parked for Phase 50 with revisit triggers.
- `gate list` vs `status` disagreement after a Ship-gate answer with nothing waiting (cosmetic).
- Converting the ~40 non-PATH environment mutations in tests and deleting ENV_MUTEX.
- Lock reclaim by pid plus start time (recycled-pid gap).
- Ship finalization-retry gate recovery (unverified).
