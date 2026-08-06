---
phase: 23-end-to-end-dogfood
plan: 10
subsystem: infra
tags: [git, github-rulesets, gh-cli, release-safety, checkpoint, dogfood-acceptance]

# Dependency graph
requires:
  - phase: 23-end-to-end-dogfood
    provides: "23-03..23-09's shipped command surface (gate list --all-roots, gate sweep, stop, evidence --require-shipped, --yes-ship) and 23-06's merge_feature no-rollback doc comment — everything Task 2's behavioral proof and recovery-path documentation cite"
provides:
  - "23-ACCEPTANCE-SETUP.md — rebuild proof, seven behavioral checks with verbatim output/exit codes, a pushed-and-read-back recovery ref with a rehearsed local restore, the real remote restore path (GitHub ruleset, not classic branch protection) with its measured ~2min undo latency, the worst-case failed-run state, and the operator's verbatim Task 4 authorization"
  - "The acceptance target named and authorized: backlog 999.27 -> phase 24 (numbering not yet assigned — orchestrator-owned prerequisite for plan 23-11)"
  - "Both content preconditions (security artifact, no self-attested Ship claim) explicitly ESCALATED and ACCEPTED UNMITIGATED by the operator, not silently cleared"
affects: ["23-11 (the acceptance run itself — consumes the recovery ref, the pre-run test baseline, and the authorization record)"]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "GitHub rulesets (the modern per-branch policy mechanism) can silently coexist with and override classic branch-protection API answers — this plan discovered the two disagreed on required-review count for `develop` and cross-checked against real PR history (five solo self-merges) to determine which one is actually authoritative, rather than trusting either API response alone."
    - "A checkpoint's own escalation must be corrected in place, not appended — when an operator accepts a precondition unmitigated, the original 'escalated' disposition line is edited to read 'escalated — accepted unmitigated', never overwritten to 'clear' or 'remedied', so a later reader sees exactly what was traded and cannot mistake acceptance for resolution."

key-files:
  created:
    - .planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-SETUP.md
  modified: []

key-decisions:
  - "Task 1 (operator, reversible): acceptance target is backlog 999.27, expected to promote to phase 24."
  - "Task 4 (operator, one-way): PROCEED. Both content preconditions (security-artifact production, no self-attested Ship claim) accepted unmitigated rather than remedied — the operator's stated reasoning is that devflow's Define/Plan stages are designed to author an unplanned target's plan set themselves, so pre-resolving either precondition would remove the thing the acceptance run exists to test."
  - "The 999.27 -> phase 24 promotion is explicitly NOT this plan's or plan 23-11's job: neither declares ROADMAP.md in files_modified, so it is recorded as an orchestrator-owned inter-wave prerequisite."
  - "The real `develop` restore mechanism is a GitHub ruleset ('develop-merge-or-squash'), not the classic branch-protection API this plan first queried — determined by a genuine cross-check against observed history (self-merged PRs with no second reviewer) rather than trusting the first API response, which reported a contradictory required_approving_review_count."

requirements-completed: [23b, 23c, 23d, 23e, yes-ship]

coverage:
  - id: D1
    description: "The operator explicitly authorized the unattended merge, naming both the target phase and what it changes"
    requirement: "23b"
    verification:
      - kind: other
        ref: "source assertion: rg -q 'Task 4 — Authorization' and rg -q 'PROCEED' both match .planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-SETUP.md"
        status: pass
    human_judgment: false
  - id: D2
    description: "The one-way authorization was asked only after the rehearsal ran, the remote's restore answer was known, and both preconditions carried a recorded disposition — Task 1 (reversible) preceded the evidence, not the authorization"
    requirement: "23b"
    verification:
      - kind: other
        ref: "git log --format='%h %ad' -- 23-ACCEPTANCE-SETUP.md: Task 2 commit 0cab011 (2026-07-26T07:19:48-04:00) precedes Task 4 commit 718a260 (2026-07-26T08:10:53-04:00); Task 4's own <precondition> element named all four evidence sections and was verified present before the operator was asked"
        status: pass
    human_judgment: false
  - id: D3
    description: "A recovery point exists, was demonstrated to restore (not merely asserted), including against the remote's real branch-protection/ruleset rules, with a recorded real undo latency rather than an assumed one"
    requirement: "23c"
    verification:
      - kind: other
        ref: "source assertion: recovery/pre-23-11-acceptance-e0f87c2 @ e0f87c2 pushed + read back via git ls-remote; local restore rehearsed in a throwaway clone (byte-identical diff); remote restore path determined via gh api rulesets (not assumed) with measured ~2min CI-bounded latency"
        status: pass
    human_judgment: false
  - id: D4
    description: "The binary driving the acceptance run is provably built from this phase's own work — every new command answers, not just a matching version string"
    requirement: "23d"
    verification:
      - kind: other
        ref: "source assertion: sha256sum of target/release/devflow changed across the rebuild and the PATH symlink resolves to the identical post-rebuild hash; all 7 behavioral checks (gate list --all-roots, gate sweep --dry-run, stop --phase 99, evidence --json, evidence --require-shipped, start --help --yes-ship, removed sequentagent verb) answered correctly with recorded exit codes"
        status: pass
    human_judgment: false
  - id: D5
    description: "The target phase's plan set is known to produce a security artifact before Ship, so the run cannot hit the same preflight wall a recorded probe already hit"
    requirement: "23e"
    verification: []
    human_judgment: true
    rationale: "Escalated at Task 3 (999.27 has zero plans to check) and explicitly accepted unmitigated by the operator at Task 4, not resolved. security_enforcement is confirmed true in .planning/config.json, so the run may legitimately be turned away at Ship preflight — this is an accepted risk, not a proven-clear precondition, and cannot be marked pass."
  - id: D6
    description: "The target phase's own acceptance criteria contain no claim that its Ship stage completed, since such a claim is unverifiable at Validate and is the exact false-green that stopped the other recorded probe run"
    requirement: "23e"
    verification: []
    human_judgment: true
    rationale: "Escalated at Task 3 for the same reason as D5 (no plan set exists yet to check) and explicitly accepted unmitigated at Task 4. The named remedy (declare --require-shipped as external_verify) was offered and explicitly declined by the operator, not applied."
  - id: D7
    description: "The chosen target phase (999.27 -> phase 24) is low-stakes by consequence, not merely by diff size"
    requirement: "yes-ship"
    verification: []
    human_judgment: true
    rationale: "Backstop verification per the plan's own must_haves frontmatter (verification: backstop) — a judgment call made by the operator at Task 1, restated with concrete file/behavior detail (one classification branch + one test in a release-preflight advisory check, no merge/version/ship control flow touched) rather than assessed by diff size alone."

duration: ~65min across two operator checkpoints
completed: 2026-07-26
status: complete
---

# Phase 23 Plan 10: Acceptance Setup — Recovery, Rebuild Proof, and the One-Way Authorization Summary

**Rebuilt-and-behaviorally-proven binary, a pushed-and-rehearsed recovery point with a real (not assumed) remote restore path measured at ~2 minutes, and the operator's one-way authorization to drive backlog item 999.27 (as phase 24) through devflow's unattended Define→Ship loop with both content preconditions explicitly accepted unmitigated rather than resolved.**

## Performance

- **Duration:** ~65 min of executor work, split across two operator checkpoints (Task 1 selection, Task 4 authorization) whose wall-clock gaps are not executor time.
- **Started:** 2026-07-26T07:08:19Z (rebuild)
- **Completed:** 2026-07-26T08:10:53-04:00 (Task 4 commit)
- **Tasks:** 4/4 (2 checkpoint decisions, 1 tracer, 1 auto)
- **Files modified:** 1 (`23-ACCEPTANCE-SETUP.md`, created then extended across 3 commits)

## Accomplishments

- **Task 1 (checkpoint, resolved):** Operator selected backlog **999.27** (inline signing-key misclassification in `check_ssh_signing_viability`, one classification branch + one test, release-preflight advisory only) as the acceptance target, expecting `VersionBump` to produce **2.0.0** (driven by plan 23-07's already-landed breaking change, independent of 999.27's own content).
- **Task 2 (tracer, committed `0cab011`):** Rebuilt `cargo build --release --workspace`; confirmed the PATH binary (a symlink directly into this repo's `target/release/devflow`) hash-matches the fresh build, not just `--version`. All seven required behavioral checks answered against this repository with verbatim output and exit codes. Full gate chain green (592 passed / 0 failed, clippy + fmt clean), paired with plan 23-08's documented 9-test deliberate removal. Created recovery ref `recovery/pre-23-11-acceptance-e0f87c2` at `develop`'s tip (`e0f87c2`), pushed and read back from `origin`, rehearsed a full restore in a throwaway clone (byte-identical tree). Determined the **real** remote restore mechanism via `gh api` — discovered `develop` is governed by an active GitHub ruleset (not the classic branch-protection settings queried first, which gave a contradictory answer), with force-push refused categorically for everyone and no admin override; the real undo is a revert PR requiring 0 approving reviews and ~2 minutes of required-CI wait, measured from this repo's own recent CI runtimes. Documented the worst-case failed-run state directly from `hooks::merge_feature`'s no-rollback doc comment.
- **Task 3 (auto, committed `30cb347`):** Checked both content preconditions against the actual target and found neither could be resolved in advance — 999.27 has zero plans, so there is no plan-set content to inspect. Escalated both rather than marking either "clear," with `.planning/config.json`'s `security_enforcement: true` confirmed by direct read (not taken on `23-RESEARCH.md`'s word) and the actual enforcement mechanism traced to `~/.claude/gsd-core/workflows/ship.md` (not DevFlow's own Rust source, which has zero matches for the check).
- **Task 4 (checkpoint, resolved, committed `718a260`):** Operator authorized **PROCEED** against phase 24, re-confirmed the expected `2.0.0` version, explicitly read and accepted the ~2-minute revert-PR undo latency and the worst-case failed-run state, and — deliberately, not by oversight — **accepted both content preconditions unmitigated** rather than directing either named remedy. Both precondition disposition lines were corrected in place from `escalated` to `escalated — accepted unmitigated by operator at Task 4`, never rewritten to `clear` or `remedied`.

## Task Commits

Each task was committed atomically:

1. **Task 1: Name the acceptance target phase (D-02)** — checkpoint decision, no commit (recorded in Task 2's artifact and restated verbatim in Task 4's authorization section)
2. **Task 2: Rebuild, 7 behavioral checks, recovery point, rehearsed restore** — `0cab011` (docs)
3. **Task 3: Close the two content preconditions** — `30cb347` (docs)
4. **Task 4: Authorize the one-way unattended merge (D-07)** — `718a260` (docs)

**Plan metadata:** this commit (docs: complete plan) — made after this SUMMARY.

## Files Created/Modified

- `.planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-SETUP.md` — the full acceptance-setup record: rebuild proof, seven behavioral checks, pre-run test baseline pair, recovery ref + rehearsal + real remote restore path, worst-case failed-run state, both content preconditions' findings and final dispositions, and the operator's verbatim Task 4 authorization

## Decisions Made

- **The active `develop` restore mechanism is a GitHub ruleset, not classic branch protection — determined by cross-checking against real history, not trusted from the first API response.** `gh api .../branches/develop/protection` reported `required_approving_review_count: 1`, which is inconsistent with this repository's own observed PR history (five consecutive `develop` merges self-approved by the sole collaborator, no second reviewer). Querying `gh api .../rulesets` found the actually-active mechanism (`develop-merge-or-squash`, `required_approving_review_count: 0`), which matches the observed behavior. Recorded both findings and the reconciliation, rather than presenting only the first (misleading) API answer.
- **999.27's numeric phase (24) is an inference, stated as such, not asserted as fact.** `devflow start --phase` takes a `u32`; ROADMAP backlog IDs (`999.x`) have no number until `/gsd-review-backlog` promotion. The operator confirmed 24 explicitly at Task 4, but the promotion itself remains unperformed — see Next Phase Readiness.
- **Both content preconditions were escalated, not resolved, at Task 3 — and the operator's Task 4 choice was to accept them unmitigated rather than direct either named remedy.** The operator's own reasoning (recorded verbatim in the artifact) is that devflow's Define/Plan stages are designed to author an unplanned target's plan set themselves, so pre-resolving either precondition ahead of the run would remove the very thing the acceptance run is meant to test.
- **Task 2 and Task 3 were committed as two separate atomic commits despite sharing one output file**, matching the plan's task boundaries rather than combining them into a single commit, so each task's own `<verify>` criteria can be checked against exactly the state its own commit produced.

## Deviations from Plan

None — plan executed exactly as written, including its explicit checkpoint ordering (Task 1 selection → Task 2/3 evidence → Task 4 authorization). No Rule 1/2/3 auto-fixes were needed: this plan produces one document, no source code, and every acceptance criterion in Tasks 2 and 3 was met on the first pass.

## Issues Encountered

- **The classic branch-protection API and the active GitHub ruleset disagreed on `develop`'s required-review count** (1 vs. 0). Resolved by cross-checking against this repository's own PR history rather than trusting either API response alone — recorded as a "Decisions Made" item above, not silently reconciled to whichever answer looked more conservative.
- **999.27 has zero plans**, which the two content-precondition checks (Task 3) are written against a target's plan set. This was surfaced explicitly as a wrinkle (both preconditions escalated, with concrete remedies named) rather than worked around by inventing a plan-set assessment that doesn't exist yet.

## User Setup Required

None — no external service configuration required. The `gh` CLI calls used in this plan (`gh api`, `gh run list`) were read-only queries against already-authenticated `gh` credentials; no new setup was performed.

## Known Stubs

None. This plan produces only a recorded evidence document; no source code, no UI, no data flow.

## Threat Flags

None beyond what the plan's own `<threat_model>` register already covers (T-23-101 through T-23-107, T-23-SC), all `mitigate`d and verified in the artifact itself:
- T-23-101 (unattended merge) — mitigated by the blocking Task 4 checkpoint, now exercised for real.
- T-23-102 (unrestorable/slow recovery point) — mitigated; the real remote restore path was determined (not assumed) and its latency measured (~2min), not estimated from the review's worst-case framing alone.
- T-23-103 (stale binary) — mitigated; seven behavioral checks, hash-verified rebuild.
- T-23-104 (an artifact reading "clear" while a precondition is open) — mitigated; both escalations are recorded explicitly as `escalated — accepted unmitigated`, never softened to `clear`.
- T-23-105 (PII in verbatim command output) — checked; no OS username or home-directory basename appears in the artifact (confirmed by direct comparison against `whoami` output during authoring). The GitHub handle `denniyahh` appears in `gh api`/`gh run list` output, but this is pre-existing public project metadata (already committed in `Cargo.toml`/`README.md`), not a new local-filesystem leak.
- T-23-106 (unexpected version bump) — mitigated; expected `2.0.0` stated and re-confirmed at both checkpoints.
- T-23-SC (package installs) — not applicable; no installs in this plan.

## Next Phase Readiness

- **Orchestrator-owned prerequisite before plan 23-11 can run: promote backlog 999.27 to concrete phase number 24 in `ROADMAP.md`.** Neither this plan nor plan 23-11 declares `ROADMAP.md` in its `files_modified`, so this promotion is explicitly not this executor's job — it must happen as an inter-wave step the orchestrator performs between plans 23-10 and 23-11. `devflow start --phase` requires a numeric `u32`; 999.27 is not one until promoted.
- **Plan 23-11 can reuse Task 2's evidence as-is** — the operator confirmed the inferred phase number (24) explicitly, so the seven behavioral checks, the recovery ref, and the pre-run test baseline (592 passed, paired with 23-08's 9-test deliberate removal) do not need to be re-gathered.
- **Both content preconditions remain genuinely open, not resolved — plan 23-11 should expect either or both to bite.** Specifically: the run may reach Ship preflight and be turned away for lack of a `*-SECURITY.md` (accepted risk, not fixed); and no code-enforced `external_verify` probe was declared for the target's own Ship-completion claim (the named remedy was offered and explicitly declined). Plan 23-11 must document the actual outcome honestly regardless of which way it resolves — a run that is turned away at preflight is a valid, informative acceptance-run result under this plan's accepted trade, not a plan-10 failure.
- **Recovery ref `recovery/pre-23-11-acceptance-e0f87c2` @ `e0f87c2` remains on `origin`**, available to plan 23-11 (and to the operator, out of band) if the real restore path is ever needed. No blockers from this plan's own execution — `git status --porcelain` was empty after every task commit, and `develop` was never checked out, reset, or force-moved.

---
*Phase: 23-end-to-end-dogfood*
*Completed: 2026-07-26*

## Self-Check: PASSED

- FOUND: `.planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-SETUP.md`
- FOUND: `.planning/phases/23-end-to-end-dogfood/23-10-SUMMARY.md`
- FOUND: commit `0cab011` (Task 2, docs)
- FOUND: commit `30cb347` (Task 3, docs)
- FOUND: commit `718a260` (Task 4, docs)
- FOUND: recovery ref `recovery/pre-23-11-acceptance-e0f87c2` on `origin` (`git ls-remote` confirms `e0f87c2c2230257f7aa8092a836225626941d09a`)
- Confirmed `develop` was never checked out, reset, or force-moved during this session (branch stayed on `feature/phase-23` throughout; local `develop`'s tip was only read via `git rev-parse`/`git fetch`, never mutated).

No missing items.
