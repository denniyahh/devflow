# Phase 49: Live Unattended Run — The Milestone's Instrument - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-24
**Phase:** 49-live-unattended-run-the-milestone-s-instrument
**Areas discussed:** Run workload, Ship blast radius, Setup step status, Failure policy

**Premise corrections surfaced before discussion:** ROADMAP criterion 1 said "no committed
`devflow.toml`", but `050ef7f` added one that same day. The Ship stage reaches GitHub (`/gsd-ship` push
+ PR). A mid-discussion correction by Claude: `BranchCleanup` deletes only the **local** feature branch,
not the remote one as first stated.

---

## Run workload

| Option | Description | Selected |
|--------|-------------|----------|
| Synthetic tiny phase (Recommended) | Throwaway single-task phase; cheapest workload crossing every seam | |
| Real small backlog item | Drive a queued fix; useful work, more variance | ✓ |
| Phase 50 itself | Maximum realism and blast radius | |

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, seed one (Recommended) | Guarantees DECN-03's resume path fires | ✓ |
| No, observe naturally | Record "never exercised" if no gate appears | |

| Option | Description | Selected |
|--------|-------------|----------|
| Pre-author it (Recommended) | Fixed target; seeded checkpoint guaranteed present | ✓ |
| Let Define+Plan generate it | Organic, but planning variance enters the measurement | |

| Option | Description | Selected |
|--------|-------------|----------|
| 999.144 (Recommended) | Resume-guard ordering test; smallest, deterministic | (fallback) |
| 999.121 | OpenCode marker-less regression test | |
| 999.142 | Hints prescribing `resume` where it is refused | |

**User's choice (free text):** "How about the original CI flake bug? That's continued to plague us this
entire time. If not appropriate, I'm fine with 144."
**Resolved to:** 999.55 (descendant of 999.23). Second round:

| Option | Description | Selected |
|--------|-------------|----------|
| 999.55 (Recommended) | Configurable budget for six helpers; structural acceptance; no "flake eliminated" claim | ✓ |
| 999.55 + 999.123 | Also de-duplicate CI triggers; needs live-ruleset proof | |
| 999.144 | The operator's fallback | |

| Option | Description | Selected |
|--------|-------------|----------|
| Decimal phase 49.1 (Recommended) | Promote into ROADMAP under v3.0.0 | ✓ |
| Launch as 999.x directly | No 999.x has been driven by `devflow start` | |

| Option | Description | Selected |
|--------|-------------|----------|
| Gate the budget value (Recommended) | Real human decision as the seeded blocking-human gate | ✓ |
| Synthetic gate | Explicitly artificial checkpoint | |

---

## Ship blast radius

Finding presented: `gsd-tools query git.base-branch` → `main` (from `origin/HEAD`), while DevFlow's Merge
hook targets `workspace/denniyahh`.

| Option | Description | Selected |
|--------|-------------|----------|
| Real repo, human Ship gate (Recommended) | Unattended to the gate; operator sees targets before push | |
| Real repo, align base first | Pre-fix the mismatch, then `--yes-ship` | |
| Disposable clone, local bare origin | No GitHub effects; `gh pr create` likely fails | |

**User's choice (free text):** "I think the devflow.toml ship target was set incorrectly. Shipping should
happen to main. Let's review if there's anything else we may need to fix in it before moving on."

Review presented: DevFlow refuses `main` as `base_branch` by design (`config.rs:351`); `base_branch` is
both fork point and merge target (45-01 D-01); `main` tracks 0 `.planning/` files; PR-to-`main` comes from
GSD, not `devflow.toml`; nothing else in `devflow.toml` needed fixing.

| Option | Description | Selected |
|--------|-------------|----------|
| Keep toml; record the split (Recommended) | Both targets observed; human Ship gate | |
| Align GSD's PR base first | Set `.planning/config.json` git.base_branch to match | |
| Make DevFlow ship to main | New capability reversing 45-01 D-01 | |

**User's choice (free text):** "Let's be precise about what the purpose of devflow.toml is and its version
of base_branch. In terms of the purpose of my personal workspace branch, it should get resynced with a
phase worktree once development is completed. So that could be the purpose of the devflow.toml
base_branch. Given gsd has its own base_branch, we most likely do not need this repeated for no apparent
reason in devflow.toml. Given we can't determine a reason for both of these to be the same, we can likely
proceed with them being different."

Restatement confirmed, with the verified consequence shown (gsd-ship opens but never merges the PR;
2358 commits / 1263 files vs `origin/main`):

| Option | Description | Selected |
|--------|-------------|----------|
| Correct; human Ship gate (Recommended) | No `--yes-ship`; operator approves at the gate | ✓ |
| Correct; --yes-ship | Fully unattended incl. PR to main | |
| Correct; PR via cut-pr-branch | Ship target = local resync only | |

| Option | Description | Selected |
|--------|-------------|----------|
| Observe, then undo (Recommended) | Let VersionBump/tag/changelog run, record, then revert locally | ✓ |
| Keep the bump | Treat as real; reconcile at next release | |
| Prevent it before the run | Needs a code change; post-Ship chain unobserved | |

---

## Setup step status

| Option | Description | Selected |
|--------|-------------|----------|
| Satisfied by 050ef7f (Recommended) | Amend criterion 1; run still observes persisted `State::base_branch` | ✓ |
| Re-prove inside the phase | Re-derive and re-commit the config in 49 | |

---

## Failure policy

| Option | Description | Selected |
|--------|-------------|----------|
| Fix blockers in-phase, re-run (Recommended) | Gap-closure for chain-blocking defects; backlog the rest | ✓ |
| Backlog everything, record partial | Close PARTIAL; VERIFY-01 stays open | |
| Fix everything found | Unbounded scope | |

| Option | Description | Selected |
|--------|-------------|----------|
| 3 attempts (Recommended) | Initial + two re-runs; third failure escalates | ✓ |
| 2 attempts | Tighter bound | |
| No cap | Iterate until complete | |

| Option | Description | Selected |
|--------|-------------|----------|
| Loop-backs are data (Recommended) | Only a run ending before Ship counts; task-caused failures fix the 49.1 plan | ✓ |
| Any non-Ship end is an attempt | Simpler accounting | |

---

## Claude's Discretion

- Evidence-capture layout; SURV-01 detection method; negative-control scratch repo location; launch and
  supervision mechanics (must be owned outside any agent worktree or subagent turn).
- Rebuild each attempt and re-run both arms on the same binary (follows from criterion 3).

## Deferred Ideas

- 999.123 CI trigger de-duplication — stays in backlog.
- Making DevFlow merge into `main` — not pursued; would be its own phase.
- Measuring 999.55's effect on the CI flake rate — needs post-merge CI history.
