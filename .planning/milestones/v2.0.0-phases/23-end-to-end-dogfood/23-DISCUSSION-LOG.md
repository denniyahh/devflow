# Phase 23: End-to-End Dogfood - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-25
**Phase:** 23-end-to-end-dogfood
**Areas discussed:** Probe target, Unattended semantics, Supervisor migration, sequentagent removal, Acceptance run location, `--yes-ship` constraints

---

## Probe target

*What work should 23a's dogfood probe actually drive? (It needs a real phase — devflow can't meaningfully drive a fake one.)*

| Option | Description | Selected |
|--------|-------------|----------|
| A small separate backlog item *(Claude's recommendation)* | Promote something tiny like 999.27/DEN-52 into a throwaway phase and let devflow drive it. Real work, low blast radius, failure doesn't block Phase 23 itself. | |
| Phase 23 drives itself | Maximally honest dogfood, but self-referential: the supervisor rewrite would run under the monitor it is replacing, and a mid-run failure wedges the phase doing the fixing. | |
| A scratch repo, not this one | Safest after the 999.37 corruption incident, but exercises none of the self-dogfood paths (staleness guard, build provenance) that broke every prior run. | ✓ |

**User's choice:** A scratch repo, not this one
**Notes:** Departs from Claude's recommendation. Claude had flagged the coverage gap in the option description before selection. After the choice, Claude verified the gap concretely against `staleness.rs:276-284` — `staleness_outcome(false, Stale)` returns `Warn`, not `Block`, so the self-dogfood hard block is structurally *unreachable* in a scratch repo. This finding directly prompted the follow-up question on acceptance-run location, which closed the gap.

---

## Unattended semantics

*Given Ship always gates in both modes, what does the acceptance criterion's "unattended" actually mean?*

Surfaced during scouting: `crates/devflow-core/src/mode.rs:82-94` documents *"Ship always gates (both modes)"* — `Mode::Auto` suppresses Validate gates only. The acceptance criterion as originally written was unreachable by construction.

| Option | Description | Selected |
|--------|-------------|----------|
| Auto mode; operator answers only the Ship gate *(Claude's recommendation)* | Walk away for Define→Code→Validate; return once to approve Ship. Keeps the human checkpoint before anything irreversible. No new code. | |
| Add a pre-authorize / `--yes-ship` escape | Truly hands-off, but deliberately removes the last human checkpoint before merge — new surface, arguably its own phase. | ✓ |
| Stop at Validate; Ship stays manual | Uses existing 20c `--until validate`. Smallest target, but leaves the Ship path — never exercised by any devflow-launched phase — unproven. | |

**User's choice:** Add a pre-authorize / `--yes-ship` escape
**Notes:** Departs from Claude's recommendation. Prioritises a genuine hands-off proof over minimising new surface. Prompted the follow-up on what constraints the flag should carry.

---

## Supervisor migration

*How should the supervisor replace the `sh -c` monitor?*

| Option | Description | Selected |
|--------|-------------|----------|
| Big-bang replace, no flag *(Claude's recommendation)* | Delete the `sh -c` path outright. DEN-58 warns the migration is the real cost; a parallel path doubles it. | ✓ |
| Feature-flagged parallel path | Safer rollback, but doubles the surface DEN-58 flags as expensive and risks the flag becoming permanent. | |
| Big-bang, plus an explicit upgrade guard | Replace outright, plus a check refusing to operate on a phase whose `state.json` predates the supervisor field. | |

**User's choice:** Big-bang replace, no flag
**Notes:** The third option's upgrade-guard concern was not discarded — it was carried into CONTEXT.md's Claude's Discretion section, since DevFlow's self-dogfooding makes a mid-run upgrade plausible and the planner should make that call explicitly rather than by omission.

---

## sequentagent removal

*How should `sequentagent` be removed? It's a top-level CLI verb (`main.rs:159`), so deleting it is user-facing.*

| Option | Description | Selected |
|--------|-------------|----------|
| Hard delete — and let it earn v2.0 *(Claude's recommendation)* | The v2.0.0 milestone is explicitly open "until a genuinely breaking change earns the 2.0 slot"; removing a documented CLI command is exactly that. | ✓ |
| Hard delete, ship as a minor bump | Faster, but sets a precedent of removing published commands without a major bump. | |
| Deprecate first, remove next phase | Kindest to external users, but leaves ~110 references and the untested `wait_for_agent_exit` path in place through 23b, forfeiting the shrink that motivated the drop. | |

**User's choice:** Hard delete — and let it earn v2.0
**Notes:** Confirms the planner should assume a major version bump. The capability intent is preserved in DEN-67 rather than discarded (CONTEXT.md D-13).

---

## Acceptance run location *(follow-up, prompted by the Probe target choice)*

*The probe runs in a scratch repo. Where does the phase's final acceptance run happen — the one that proves "devflow works end to end"?*

| Option | Description | Selected |
|--------|-------------|----------|
| Scratch probe, then self-hosted acceptance *(Claude's recommendation)* | Probe in scratch (isolates the supervisor); acceptance on this repo, so the staleness gate, build provenance and worktree paths — unreachable in scratch — are actually exercised. | ✓ |
| Scratch only | Safest and fastest, but Phase 23 would ship having never proven the self-dogfood path that broke every prior run. | |
| Self-hosted only, skip the scratch probe | Fewest runs, maximum fidelity, but reintroduces the blast radius just avoided. | |

**User's choice:** Scratch probe, then self-hosted acceptance
**Notes:** Closes the coverage gap created by the scratch-repo probe decision while keeping the risky run until after the supervisor is proven.

---

## `--yes-ship` constraints *(follow-up, prompted by the Unattended semantics choice)*

*What constraints should the new `--yes-ship` pre-authorization carry? It removes the last human checkpoint before merge, version bump and changelog.* (multi-select)

| Option | Description | Selected |
|--------|-------------|----------|
| Per-run flag only — never config-persistable | Must be typed each invocation; not settable in `devflow.toml`. Keeps a standing auto-merge from becoming the silent default. | ✓ |
| Refuse on the self-dogfood workspace | Hard-refuse when the target is DevFlow's own repo, reusing `is_self_dogfood_workspace()`. Prevents an unattended auto-merge into this project's `develop`. | |
| Still fire the gate and record the decision | Gate fires and is auto-answered rather than bypassed, so `events.jsonl` and the gate ledger show an explicit pre-authorized approval. | ✓ |
| Require `--until ship` to be explicit | Pre-authorization can never extend past the stage the operator intended. | |

**User's choice:** Per-run flag only + Still fire the gate and record the decision
**Notes:** The self-dogfood refusal guard was **considered and declined.** Combined with the self-hosted acceptance run, this means the acceptance run will unattended-merge a real phase into `develop`, bump the version and append the changelog on this repository — two days after 999.37 corrupted it. Claude surfaced this consequence explicitly before writing CONTEXT.md; the operator did not revise. Recorded in CONTEXT.md as **D-07, an accepted risk rated one-way**, with suggested mitigations (drive a low-stakes phase; take a recovery point first) for the planner to encode rather than re-decide. The declined guard is preserved in Deferred Ideas as the ready-made mitigation.

---

## Claude's Discretion

The operator did not constrain these; recorded in CONTEXT.md for the planner and researcher:

- **Supervisor signal handling** — DEN-58 notes the spike installs no SIGTERM handler, so a SIGTERM'd monitor leaves a stale socket. Degrades correctly via sweep + pgid backstop, but production should trap SIGTERM/SIGINT. Must be an explicit recorded call, not an omission.
- **Scratch-repo scaffolding for 23a** — the minimum `.planning/` + GSD structure a probe target needs to be a valid devflow target.
- **In-flight-phase behaviour across the big-bang upgrade** — whether a phase whose `state.json` predates the `supervisor` field is refused with guidance or handled otherwise.
- **`hooks_after_ship` `WorktreeRemove` and capture-file sweeping** — both untested-on-success paths this phase will be first to exercise (per DEN-59's operator note).

## Deferred Ideas

- `--yes-ship` refusal on the self-dogfood workspace — declined this phase (D-07); ready-made mitigation if the accepted risk proves uncomfortable.
- 999.31 / DEN-56 — Modular Agent Driver (Codex blocker, not a Claude one).
- 999.25 / DEN-50 — release-cut executor (the crates.io half of Ship).
- 999.42 / DEN-67 — agent failover on token exhaustion; blocked on DEN-58 *and* DEN-56.
- macOS verification — DEN-58's largest unknown; `chore/macos-ci` holds the deferred CI work.
- 999.38 / DEN-65 — test-suite `PATH` race; fixing it would let `ENV_MUTEX` shrink or disappear.
- 999.39 / DEN-66 — production git calls inherit a redirecting environment.
- The displaced "Test Suite & CI Hardening" theme — 999.15, 999.17, 999.18, 999.19, 999.20, 999.22 — untouched in the backlog.
