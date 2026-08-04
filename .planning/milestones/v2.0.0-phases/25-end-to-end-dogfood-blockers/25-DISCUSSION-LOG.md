# Phase 25: End-to-End Dogfood Blockers — Start, Progress, Finish, Recover - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-27
**Phase:** 25-end-to-end-dogfood-blockers
**Areas discussed:** 25c version rule, 25e predicate boundary, acceptance run target, 999.38 fold-in, 25b pin semantics

**Areas offered but not selected:** 25a base-ref strategy, 25d reaper design — their ROADMAP fix directions stand (CONTEXT.md D-17).

---

## Gray-area selection

| Option | Description | Selected |
|--------|-------------|----------|
| 25b pin semantics | One-liner vs persisted pin; the `is_none()` guard leaks on `resume` and preflight LoopBack | ✓ (re-opened at the end) |
| 25a base-ref strategy | Fetch / resolve against `origin/develop` / refuse loudly | |
| 25c version rule | What replaces tag-count minor and `git describe` patch | ✓ |
| 25d reaper design | PID discovery, TERM→KILL escalation, where it lives | |
| 25e boundary | Test-only vs tighten the predicate | ✓ |
| Acceptance run target | Which phase drives it, and does it live in Phase 25 | ✓ |
| 999.38 fold-in | The optional sixth unit | ✓ |

---

## 25c — Version rule

### Q1: What should the version at Ship be derived from?

| Option | Description | Selected |
|--------|-------------|----------|
| Fix the git derivation | Semver-filter `count_git_tags`, anchor patch to highest reachable version tag | |
| Read the file, bump patch | `read_version` + patch increment; minor/major become explicit operator actions | |
| Highest reachable tag +1 | Ignore both tag count and version file | |
| You decide | | |

**User's choice:** *"I want to fully automate the versioning starting from now. Help me determine the best versioning scheme for this type of project to the degree that deviates from the policy I originally defined."*

**Notes:** This reopened the June 2026 ban on commit-message-based versioning. Two things were measured before recommending: (1) the ban at `ROADMAP.md:36` is a bare bullet with no rationale, incident, or evidence attached; (2) commit hygiene is 118/120 conforming over the last 120 non-merge commits, with the 2 exceptions (`merge:`, `release:`) structurally excludable. Tag reachability was also checked — all of `v1.0.1`…`v2.0.0` are reachable from HEAD, and the only unreachable tag is the non-semver `archive-planning-docs-2026-07-24`, i.e. exactly the one inflating the count to 11. Recommended and adopted: reachable-max-semver baseline + conventional-commit bump classification, with `Cargo.toml` demoted from input to output.

### Q2: Should a breaking-change marker cut a major release unattended?

| Option | Description | Selected |
|--------|-------------|----------|
| Yes — fully automatic | `!` / `BREAKING CHANGE:` bumps major with no human in the loop | |
| Major needs a gate | feat→minor and fix→patch unattended; breaking opens a gate | ✓ |
| Cap at minor | Breaking markers bump minor; major stays a hand-cut | |

**Notes:** Chosen for the irreversibility asymmetry — a crates.io version can never be reused or unpublished, and `hooks_after_ship` has no rollback after Merge. Consistent with D-05's rule that a dangerous authorization is typed per-invocation, never a standing default.

### Q3: What version does a no-bump range get?

| Option | Description | Selected |
|--------|-------------|----------|
| Bump patch anyway | Every ship produces a distinct version | ✓ |
| No bump, no tag | Ship completes without a release | |
| Refuse loudly | Fail with a named reason | |

**Notes:** "Refuse loudly" was noted as halting an unattended run — the exact failure class the phase exists to remove.

### Q4: Unreachable highest semver tag (squashed sync broke ancestry)?

| Option | Description | Selected |
|--------|-------------|----------|
| Refuse with the fix | Error naming the tag and the repair command | ✓ |
| Use reachable max | Proceed silently from a lower baseline | |
| You decide | | |

**Notes:** The silent variant would compute a version below the real release history — the same false-evidence shape 999.51 warns about, arriving through tags instead of the base ref.

### Q5: Where is major-bump detection evaluated?

| Option | Description | Selected |
|--------|-------------|----------|
| Ship entry, pre-merge | Classify at Ship stage entry before `hooks_after_ship` | |
| Preflight check | Fold into `run_preflight` as another named check | ✓ |
| You decide | | |

**Notes:** Raised because a gate opening inside `VersionBump` opens *after* the merge to `develop` has already committed. Preflight reuses the D-13–D-16 gate + notify machinery and is bounded by `preflight_retries`.

### Q6: Unrecognised commit type?

| Option | Description | Selected |
|--------|-------------|----------|
| No bump, but report | Contributes nothing; count reported so drift is visible | |
| Add a commit-msg hook | Reject non-conforming subjects at commit time | |
| Treat as patch | Assume at least a fix | ✓ |

---

## 25e — Predicate boundary

**Reframed before the question was asked.** Verification showed `stop` already matches `lock::holder_identity` against `agent::process_start_time` (`commands.rs:1191-1200`), so 999.47's prescribed production fix has landed and `looks_like_devflow_process` has no production callers left — only test code. The area changed from "tighten a live guard" to "what to do with a dead `pub fn` whose tests flake."

### Q1 (asked twice — user requested a trade-off analysis first)

| Option | Description | Selected |
|--------|-------------|----------|
| Delete it, retarget tests | Remove the `pub fn`; rewrite both tests against the identity guard | |
| Deprecate, then retarget | `#[deprecated]`; same test retarget; no API break | ✓ |
| Keep and tighten | `argv[0]` + `/proc/exe` corroboration | |

**User's choice:** *"Help me understand the pros and cons of these approaches to help me decide"* → then "Deprecate, then retarget (Recommended)".

**Notes:** The analysis established three things. (1) The determinism win comes from the *retarget*, not the deletion — testing the identity guard means writing a mismatched starttime into a lock file, with no `spawn()` and therefore no exec race, whereas today's tests must race a real `execve`. (2) Deletion is a breaking API change (`devflow-core` has no `publish = false`; `lib.rs:54` is `pub mod agent`), so under the scheme locked in 25c it would spend `3.0.0` on a function with zero known callers. (3) "Keep and tighten" does not fix the flake at all — `argv[0]` sits inside the same parent-inherited cmdline the exec window exposes, and 999.47 records `/proc/<pid>/exe` as inherited in that window too.

---

## Acceptance run target

### Q1–Q3 (asked as a set; all three resolved by one policy answer)

| Option | Description | Selected |
|--------|-------------|----------|
| Final unit of Phase 25 | 25g runs the real unattended acceptance | |
| Its own phase 26 | Fixes ship independently; the run becomes Phase 26 | |
| You decide | | |

**User's choice:** *"I'm going to keep the unofficial from now on, not tied to phase completions until further notice."* — confirmed on read-back as: the acceptance run is unofficial and continuous, gates **no** phase's completion until further notice, and Phase 25 closes on unit-level acceptance.

**Notes:** Questions 2 (real ship vs throwaway branch vs `--until Validate`) and 3 (halt policy) were dissolved by this answer — with no in-phase acceptance run, there is no ship policy or halt policy to decide. Two consequences were confirmed on read-back: the ROADMAP's "Acceptance" paragraph must be rewritten (folded into 25f), and each of 25a–25e now needs its own unit-level verifiable acceptance since the end-to-end run no longer backstops them.

---

## 999.38 fold-in

| Option | Description | Selected |
|--------|-------------|----------|
| Fold in with 25b | 25b already edits `staleness.rs`'s call path; the flake lives in that module's tests | ✓ |
| Fold in with 25e | The ROADMAP's framing — group both flakes as one test-hygiene pass | |
| Leave it out | Keep in backlog as Medium | |

**Notes:** The 25b pairing was an argument the ROADMAP entry did not make. 25e's work moved to `agent.rs`/`commands.rs` once the predicate was found to be dead, so the entry's "group the flakes" framing would have paired unrelated files.

---

## 25b — Pin semantics (re-opened after the other areas)

Offered again at the wrap-up because a code-grounded finding contradicted the ROADMAP: `archived_stage == None` is passed by three callers (`commands.rs:236` fresh start, `pipeline_launch.rs:233` `resume`, `preflight.rs:435` preflight LoopBack), the latter two mid-run.

### Q1: How should the staleness verdict be pinned?

| Option | Description | Selected |
|--------|-------------|----------|
| Persist the pin in State | Record the passing verdict's build commit; skip when it matches | |
| The one-liner as written | `if archived_stage.is_none()` | |
| Hoist out of `launch_stage` | Move the call to the `start` path in `commands.rs` | ✓ |

**User's choice:** *"Which solution is the simplest and cleanest from a code perspective? I don't want to overengineer this since this is only to support dogfooding."* → then "Hoist — confirmed".

**Notes:** The hoist was verified before recommending: `state.worktree_path` is populated at `commands.rs:199`, before the `launch_stage` call at line 236, so the check still evaluates against the phase's worktree HEAD as the D-18 message promises. A hoist above line 199 would silently change that. The persisted pin would have cost a new `Option<String>` on `State`, a serde default, a write path, a comparison, event-log plumbing, and tests for migration and mismatch — for a guard that fires only in this repository.

### Q2: Pin mismatch (different binary mid-run)?

| Option | Description | Selected |
|--------|-------------|----------|
| Re-check, re-pin | Treat a different binary as an unproven driver | |
| Honor the pin | Standing bypass | |
| Refuse loudly | Treat a binary swap as operator error | |

**User's choice:** *"Reference my earlier comment."* — dissolved by the hoist: with no pin there is no mismatch case. The residual exposure (`resume` no longer re-checks) was recorded as an accepted trade rather than hidden, justified by the standing 2026-07-27 decision that only validated, pushed code ever drives a run.

### Q3: Also emit the verdict to the event log?

| Option | Description | Selected |
|--------|-------------|----------|
| Hoist — confirmed | No new state, no pin, no event | ✓ |
| Hoist, plus one event | Emit the build commit for 999.48's provenance ask | |
| Reconsider the pin | | |

---

## Claude's Discretion

- Sequencing within the phase, subject to the ROADMAP's spine and the binding 25b+25c ship-together constraint.
- Where the semver parsing and reachability predicate live, and whether semver ordering is hand-rolled or takes a dependency.
- The exact shape of 25d's discovery mechanism, within the constraints carried forward in CONTEXT.md D-17.

## Deferred Ideas

- A `commit-msg` hook enforcing Conventional Commits — new enforcement surface on every contributor's machine; beyond the six units.
- Reporting unrecognised-commit counts in ship output — the fail-soft variant of the above; not chosen.
- Deleting `looks_like_devflow_process` outright — rides the next major that something real earns.
- Deleting the `staleness.rs` module entirely — considered and rejected in the ROADMAP entry; recorded so it is not re-litigated.
- 999.52 (sync-discipline repair step) — named as a coupling for 25c; stays in the backlog.
- 999.45, 999.43, 999.46, 999.50 — open, out of scope, untouched.
- Reinstating the end-to-end run as a phase gate — deliberately reversible.
