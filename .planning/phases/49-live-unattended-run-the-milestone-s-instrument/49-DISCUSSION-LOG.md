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

## Post-review re-decisions (adversarial review of CONTEXT at `0531ef5`)

Three lanes (codex, DeepSeek `deepseek-flash` via pi, agy) completed with 35/59/43 citations; all three
independently found the same four defects, each checked in source by the orchestrator. Raw output:
`~/.cache/devflow-review/phase49-context-0531ef5/`.

**Seeded checkpoint:** auto-mode preflight refuses any plan declaring `blocking-human`
(`preflight.rs:1076-1080`), and auto-decide only fires for a set a human approved at Code preflight.

| Option | Description | Selected |
|--------|-------------|----------|
| Keep it; approve at preflight (Recommended) | Operator approves Define and Code preflight gates; then unattended | ✓ |
| Drop it; fully unattended | DECN-03 recorded "never exercised" | |
| Pre-seed gate responses | Pre-written approval; sidesteps a deliberate gate | |

**Ship:** the Ship gate fires after `/gsd-ship` pushes and opens the PR (`pipeline_launch.rs:1746`).
Claude had told the operator the gate prevents the push; that was asserted without checking and was
wrong. Repo is public, but `workspace/denniyahh` is already on `origin`.

| Option | Description | Selected |
|--------|-------------|----------|
| Let it open the PR, close after (Recommended) | PR to main is the observed ship target; closed unmerged | ✓ |
| Halt before Ship, then resume | `--until validate`, inspect, `devflow resume` | |

Claude (remit) additions from the review: D-13 clean launch checkout + restore, D-14 negative-control
fixture removes config.json and asserts on text, D-15 `--agent claude`, D-16 fork-point measurement,
D-12 retry preserves the prior attempt before `--force`; D-05 corrected (heading alone satisfies the
reachability guard).

## Claude's Discretion

- Evidence-capture layout; SURV-01 detection method; negative-control scratch repo location; launch and
  supervision mechanics (must be owned outside any agent worktree or subagent turn).
- Rebuild each attempt and re-run both arms on the same binary (follows from criterion 3).

## Deferred Ideas

- 999.123 CI trigger de-duplication — stays in backlog.
- Making DevFlow merge into `main` — not pursued; would be its own phase.
- Measuring 999.55's effect on the CI flake rate — needs post-merge CI history.

---

# Update — 2026-09-28 (re-scope to a disposable project)

**Date:** 2026-09-28
**Phase:** 49-live-unattended-run-the-milestone-s-instrument
**Areas discussed:** Run payload, Gates & DECN-03, Throwaway GitHub repo, Blocking defects — all delegated

**Why the update.** After four retry designs and four external review rounds against the 18-plan set, the
operator decided (outside this discussion, then approved as a ROADMAP amendment in `968b713`): run against a
disposable GSD project, and hold each live attempt as one human-action checkpoint judged afterwards. The
operator set the principle to operate within GSD's current confines (D-17…D-19). Before the discussion, the
operator also asked why two gates rather than one: the Define gate follows from D-03 with D-04, because
DevFlow's Define preflight gates on a pre-authored plan but defers "no plans yet" to Code
(`preflight.rs:1068-1072`).

## Area selection

| Option | Description | Selected |
|--------|-------------|----------|
| Run payload | Synthetic single-task phase (Phase 23a) or small realistic task; pre-authored (D-04)? | |
| Gates & DECN-03 | Two gates (D-03 kept), one (agent-written plan), or zero (DECN-03 not exercised); Ship gate or `--yes-ship`? | |
| Throwaway GitHub repo | Who creates it, per attempt or reused, when deleted; keep D-06's base split inside it? | |
| Blocking defects | Fix vehicle (`/gsd-quick`, inserted decimal phase, escalate); keep the cap of 3 (D-11)? | |

**User's choice:** "You decide on all" (free text) — all four areas delegated to Claude.
**Notes:** Claude decided within the operator's standing decisions and labelled each result "Claude
(delegated, 2026-09-28)" in CONTEXT.md. D-03, D-04, D-10 and D-11 were kept. **One operator decision was
changed under the delegation:** D-07's human Ship gate became `--yes-ship` (D-23), because its purpose was
protecting this repository's local merge, bump and tag, which a throwaway repository does not need.

## Claude's Discretion (decisions made under the delegation)

- Payload: synthetic single-task phase via an extended `scripts/scratch-dogfood-repo.sh`; seeded
  `blocking-human` checkpoint kept; pre-authored; must not classify as MAJOR (the MAJOR gate cannot be
  auto-approved) — D-20, D-03, D-04, D-21.
- Gates: exactly Define and Code preflight; `--yes-ship` for Ship — D-22, D-23.
- Repository: a fresh private GitHub repository per attempt, operator-created and operator-deleted after
  verification; D-06's base split kept inside it; negative arm stays local; committed evidence sanitized —
  D-24…D-26.
- Failure handling: a stopped attempt fails its checkpoint with no SUMMARY and GSD's normal resume re-presents
  it; fix via `/gsd-quick` or an inserted decimal phase, confirmed by the operator at the stop — D-27, D-28.

## Deferred Ideas

- 999.55 is no longer the payload; it stays in the backlog as its own future phase.
- GSD re-run / resume / loop support: the operator is taking a proposal to the GSD maintainers outside DevFlow.
