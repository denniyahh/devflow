# graphify-labs Review Triage — PRs #218 and #219

**Date:** 2026-09-24. **Triggered by:** operator question on whether the `graphify-labs[bot]` reviews were
being read. **They were not:** nothing in memory, STATE.md or phase 48 artifacts referenced them, and the
close-out procedure waited only on `statusCheckRollup`. Both PRs merged with every finding untriaged.

**Operator decision (2026-09-24):** do both. Triage the findings already posted on #218/#219 (this file),
and add a triage step to the close-out procedure going forward.

## What the bot posts

- A PR review per pushed head, plus a `Graphify` check run (conclusion `success` even when it says "Worth a
  look", so it never blocks a merge) and a `Graphify Formal Verification` check run (`neutral`, no output).
- **"Worth a look" findings**, 5 per review, each a one-line title plus `path:line`, and each marked by the
  bot itself "agreed by 2 of 2 members but NOT verified (no proof, no reproducing execution)". Titles only;
  no reasoning is published.
- **Inline "Health regression" comments**: deterministic coupling counts (callers and callees). On #218,
  15 notes posted twice (once per review) = 30 comments.
- A footer claiming "78 more finding(s) on lines outside this diff (see the check run)". **The check run
  does not contain them.** Its text is the same five findings. They are not retrievable from the API.

## Coupling notes: noise, with a stale baseline

Every review states its health baseline is commit `71a5192`, 138–204 commits behind the PR base.
Negative control: the review lists `handle_validate_outcome()` as a "new" hotspot, yet it already exists at
`71a5192` (`git show 71a5192:crates/devflow-cli/src/pipeline_outcomes.rs`, 1 match). The "new" labels
therefore measure index staleness, not this change. Most flagged functions are tests with 6–11 callees.
**Not triaged individually; treat as noise until the bot's index is current.**

## Findings

Checked against source at `e9c6de3`. The files these findings name are unchanged since the reviewed head
`ca3071c` (`git diff --name-only ca3071c e9c6de3 -- crates/` lists only Cargo.toml files and three test
files). `scripts/cut-pr-branch.sh` changed after the reviewed commit `0462294` (`ac9f70b`).

| # | PR | Finding (bot title) | Verdict | Evidence |
|---|----|---------------------|---------|----------|
| A1 | 218 | gate respond refuses valid open gates without a confirmed waiter | **By design** | Phase 48 D-05: at a gate with no live waiter, write nothing and name the repair. The refusal names `resume`/`recover --clean` (non-Ship) and `devflow ship` (Ship), `commands.rs` `gate_respond`. Residual limit already recorded in 48 D-02 (waiter trace was a text search). |
| A2 | 218 | gate sweep no longer reaps unattended gates | **By design** | `gate_sweep_may_reap` reaps only a `Live` holder (48 D-05 item 3: no answer file at a no-waiter gate). The residual hazard (an answer outliving its waiter) is accepted as `48-SECURITY.md` AR-48-04 and tracked in 999.130. |
| A3 | 218 | advance_with holds per-phase lock across multi-day gate blocking wait | **By design** | 48 D-02: every gate waiter holds the per-phase lock. D-05's waiter identification depends on exactly that. |
| A4 | 218 | Stop combines lock status and identity from separate racing observations | **Not a defect** | The two reads are separate (`stop_via_gate`), but `answered_gate_contention_message` claims a waiter only when both observations and the current holder all agree on pid **and** start time. Any disagreement falls to the fail-closed "cannot be confirmed … not marked stopped" error. The race costs an honest refusal, never a false claim. |
| A5 | 218 | resume refuses over live run then re-acquires lock (check-then-act) | **Premise false** | `resume` takes the lock (`pipeline_launch.rs:1418`) before the guard (`:1427`) and holds it through `load_state`. The guard never re-acquires. Same order at both reviewed heads (`2ebad61`, `ca3071c`). |
| B1 | 218 | Approval records a stale checkpoint snapshot after a human gate wait | **Backwards** | Approval is keyed by `(plan_file, element)` (`recorded_approval`, `pipeline_launch.rs:1205`). Recording the set scanned when the gate opened (the set the human was shown) is correct. A post-wait re-scan would approve declarations nobody saw, and a change made during the wait re-gates on the next scan. Relevant to Phase 49's seeded checkpoint and confirms the design. |
| B2 | 218 | recover --clean --phase now refuses a contended phase | **Tracked** | Refusing a *live* holder is 48 D-02's one-writer rule. The harmful case (a recycled pid blocking the lock) is **999.128**, already pinned by `recover::tests::clean_phase_returns_without_cleanup_for_a_recycled_pid_lock`. The now-stale "escape hatch" comment in `recover_cmd` was added to 999.128. |
| B3 | 218 | Recycled-pid gate contention returns Ok(()) leaving phase unstopped | **Not reproduced in source** | `answered_gate_contention_message` returns `Err` for `Recycled` ("nothing is waiting … not marked stopped") and for unconfirmable identity. `Ok(())` needs a fully agreeing `Live` holder, and `stop_via_lock` errs on a start-time mismatch. |
| B4 | 218 | Existing non-abort gate response is treated like no gate was handled | **By design** | `stop_with_existing_response` documents it: an approval or loop-back is not `stop`'s outcome, so `stop` falls through to the lock path and signals the holder. |
| B5 | 218 | Gate cleanup only on Advance path, not before park_auto_rescan_repair on LoopBack | **Premise false** | `park_auto_rescan_repair` calls `Gates::cleanup` itself, deliberately after persisting the Supervise handoff (its doc comment). The Supervise loop-back keeps the response as the human's repair request, also documented. |
| C1 | 219 | cargo test --exact detection matches module-qualified names (false positives) | **Not reproduced** | Ran `scripts/lint-plan-bashisms.sh` on five plans: `prompt::tests::x -- --exact`, `-- --exact prompt::tests::x` and a quoted `devflow_core::…` form all pass (rc 0). Positive control: two bare-name forms are refused (rc 1, "bare-name trap"). |
| C2 | 219 | Forbidden .gsd-* paths audited but not removed from mixed commits | **Real at `0462294`, fixed** | At the reviewed commit the `git rm` list omitted `.gsd-backups`/`.gsd-id`/`.gsd-worktrees` while the audit regex had them, so a mixed commit touching one would **fail the audit (fail-closed), not leak**. `ac9f70b` now removes by the same regex the audit uses. |
| C3 | 219 | Nested planning artifacts bypass PR-branch filtering | **Latent, by design** | The regex is anchored at the repo root ("Each entry matches that exact path or anything under it"). No nested forbidden-name path is tracked: 0 matches against 1170 top-level matches as a control. |
| C4 | 219 | Default PR branch inference drops the first hyphenated slug segment | **Real, filed 999.146** | Reproduced: `personal/add-cache-flag` → `feature/cache-flag`. Default name only; phase mode and an explicit argument are unaffected. Low severity. Also found: no test exercises `cut-pr-branch.sh` at all. |
| C5 | 219 | Fidelity diff compares wrong pair when included commits are a subset | **By design** | The per-path `PR_BRANCH` vs `ORIG_BRANCH` comparison is the documented warning ("a commit that was not replayed also touched it … review each"). It is a warning, never a failure. |

**Outcome:** 15 findings. 1 new real defect (C4 → 999.146, minor). 1 real but already fixed (C2, which
failed closed). 1 already tracked (B2 → 999.128). 12 by design, premise false or not reproduced. No code
change is warranted now.

**Hit-rate caveat.** 2 of 15 findings were real, and neither was severe. Finding titles are one line with no
reasoning, and the bot marks them unverified itself. That is a weak signal, not a reason to skip them: C4
was real and nothing else had caught it. Keep triaging, and budget roughly one verified check per finding.

## Going forward

Close-out procedure step added (operator's memory, `project-phase-closeout-pr-replay`): before
`gh pr merge`, fetch the bot's review for the current head SHA and triage each "Worth a look" finding
against source (fix, file, or dismiss with a reason) in the phase's close-out notes. Coupling notes are
skipped while the bot's baseline is stale.
