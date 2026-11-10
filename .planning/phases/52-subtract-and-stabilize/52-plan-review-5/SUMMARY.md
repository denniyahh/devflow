# Phase 52 plan review, round 5: D-46 and the round-4 fixes (2026-11-10)

Snapshot: `feature/phase-52` at fe9ebd0 (the reviewed snapshot is identical to HEAD). Lanes: agy (gemini-3.8-flash-high, high) and pi (deepseek-flash) returned. **codex DROPPED** on a usage limit (`codex-DROPPED.txt`); the operator agreed to add it only on a material disagreement, and there was none. The prompt is `prompt.txt`.

A host verifier checked every finding against the plan text, in wave order 52-08 … 52-22 → 26 → 27 → 28 → 29 → 33 → 30 → 31 → 32, and against source at fe9ebd0. It ran two scratch experiments, since deleted: the real `devflow_core::gsd_config::set_auto_chain_active` round trip (`true` then `false`, as `AutoChainGuard` does) in throwaway repos over four config shapes, and a candidate rule for the 52-26 source scan over the non-test text at HEAD with a planted control. The host re-checked V1's evidence (GSD `plan-phase/steps/auto-advance-check.md` writes `workflow._auto_chain_active false` whenever it runs without `--auto`/`--chain`; this repo's committed config already has the key). Severities are the host's own.

No lane found a path that finishes a run without `can_finish`. D-46's amendment makes the earlier refutation of the false finish from the `no_gap_closure_plan` gate (agy C1) stronger, not weaker.

The most important result came from the host, not from a lane (V1): in a typical fresh GSD project, GSD's own `/gsd-plan-phase` dirties the tracked `.planning/config.json` and never commits it, so D-46 blocks a correct run's finish until the operator commits the file once. This repo is not affected.

Totals: 4 confirmed, 12 refuted, 4 plausible. Confirmed by class: (a) 1, (b) 1, (c) 1, (d) 1, (e) 0. agy and pi do not disagree on anything material.

## Confirmed

Root-cause classes: (a) a gap in the D-45/D-46 design; (b) cut machinery still referenced; (c) two plans, or plan and source, disagree; (d) a verify gate or test mechanic; (e) a pre-existing problem, unrelated to D-45.

| # | Sev | Source | Finding (evidence) | Class | Smallest fix |
|---|-----|--------|--------------------|-------|--------------|
| V1 | Medium | host (from the agy C3 / pi C5 check) | **D-46 blocks a correct run's finish in a fresh GSD project.** A fresh project's committed config has no `workflow._auto_chain_active` key (`gsd-tools config-new-project '{}'`, gsd-core 1.20.1, writes none). DevFlow's Plan stage runs `/gsd-plan-phase N` with no `--auto` (`prompt.rs` ~L751), so GSD's `plan-phase/steps/auto-advance-check.md` (L13-17) runs `config-set workflow._auto_chain_active false`. GSD commits that file only from new-project (`new-project.md` L636). Reproduced: after GSD's config-set, `git status --porcelain --untracked-files=all` printed ` M .planning/config.json` (diff adds `"_auto_chain_active": false`). Control: the same config-set on this repo's HEAD config, which already has the key, left the tree clean. From then on every Validate outcome in that run reads a dirty tree and records no PASS; auto gates and approve runs Validate again until the operator commits. The gate names the path (never silent), and one commit clears it for good. Supervise is affected too, because GSD writes the file. | a | **Operator decision** — see 52-REVIEWS.md. Options: (1) accept and document the one commit; (2) have the start/resume repair (`force_clear_auto_chain`) write and commit the key when absent; (3) leave `.planning/config.json` out of the dirty read. |
| V2 | Medium | agy C2, pi C4 | **The 52-26 stage-write scan, written naively, fails on correct code.** 52-26:230-235 asks for "the assignment `.stage = ` on a `State` … exactly twice" and excludes only `spec.stage` and `let stage`; it says nothing about comments. Non-test text at HEAD (split at `"\n#[cfg(test)]\nmod tests {"`) holds the two real writes (`pipeline_gate.rs` L108, L199), `spec.stage = invocation.stage` (`pipeline_launch.rs` L1926) and three comment hits (`pipeline_gate.rs` L70, `commands.rs` L1989, `ship_evidence.rs` L74, the last removed by 52-13). A raw count is 3 or 4 at 52-26's wave: it fails loudly (no false green) but invites a receiver allowlist that would miss `persisted.stage =`. 52-26 also borrows its non-test split from 52-29, a later plan. | d | State the rule in 52-26: split at the existing marker; drop each line from the first `//`; count `\b([A-Za-z_]\w*)\.stage\s*=(?![=>])` whose receiver is not `spec`. Tested at HEAD: exactly 2 hits (L108, L199). Planted control: `persisted.stage = Stage::Plan;` counts; `// state.stage = to`, `spec.stage = a`, `a.stage == b` and `!= state.stage =>` do not. Limits: a struct literal, `mem::replace` or `&mut s.stage` write is not seen. Do not make `State::stage` private (published `devflow-core` field, `state.rs` L38; semver). |
| V3 | Low | host (from the agy C3 / pi C5 check) | **52-29's chain-flag prose misstates both cases.** (i) 52-29:178 makes the guard fix conditional ("if `an_auto_code_stage_leaves_the_gsd_config_as_it_found_it` fails"). It does fail today: the real function on a key-less config, and on one with the key `false` in 4-space indent, leaves ` M .planning/config.json`; this repo's 2-space config is byte-identical. So the fix is required, and the gated test enforces it. (ii) 52-29:441-443 (Limits) says a leaked `true` in HEAD leaves every gate naming `.planning/config.json` until the operator commits. Source contradicts this: `force_clear_auto_chain` commits the repair when the flag is the only change (`gsd_config.rs` L320-339); the file stays dirty only when it also carries the operator's own uncommitted edit. | c | (i) "The no-key row fails at fe9ebd0 (`set_auto_chain_active` adds the key): make the guard restore the bytes …". (ii) "is repaired and committed at start or resume; only when the file also carries the operator's own uncommitted edit is the commit refused, and every gate then names it". |
| V4 | Low | pi C6 | **A history row in 52-29's ledger still tells the executor to build cut machinery.** The Round 4 row at 52-29:508 names "a row for its `stale_after_reverify` gate over `stale` and over `missing`". The current body (:36, :229) lists ten kinds and no `stale_after_reverify`. Round 6 supersedes only Round 5's R3/R4, and the note at :558 covers only Round 1's P1/P2 and Round 6's task numbers. | b | One line at the ledger head: "Rows before Round 6 are history; where one conflicts with the body, the body and D-45 govern." |

Negative controls (the patterns searched to confirm each row's refuting text is absent):

| Row | Pattern | Searched in | Result |
|-----|---------|-------------|--------|
| V1 | `plan-phase.*_auto_chain\|auto-advance-check\|config-set` | 52-*-PLAN.md, 52-CONTEXT.md | none |
| V2 | `comment` near `scan\|strip\|stage` | 52-26-PLAN.md | none |
| V3 | `commit_refused\|committed_tree_repaired\|repair commit` | 52-29-PLAN.md | none |
| V4 | `history rows\|Rows before\|left as written` | 52-29-PLAN.md | only :558, scoped to Round 1 P1/P2 and Round 6 |

## Refuted

- **agy C1 (High): a finish from the `no_gap_closure_plan` gate.** Same claim as round 4's refuted C-01, and D-46 answers it more firmly: a PASS is recorded only over a clean tree, and `read_evidence` re-reads the dirty tree at approve. GSD's status can become `passed` at an unchanged HEAD only through a tree change, which is now either dirty (no PASS) or a commit (a new HEAD). agy also cites "`pipeline_gate.rs` line 104 (`finish_run`)", which does not exist at HEAD; 52-08 creates it.
- **agy C3 (High): `AutoChainGuard` blocks every run.** As a plan defect this is refuted: 52-29:143's gated test has the no-key fixture and requires byte-identity. The round trip is real for key-less or non-serde-formatted files (reproduced), not for this repo's config (reproduced clean). Residue: V3; GSD's own write: V1.
- **agy C4 (High): 52-22 lacks D-46.** D-46 lands in 52-29, which owns the dirty read and changes what 52-22's site writes (:170-174); its blanket clause (:175-177) makes fixtures with untracked files commit or ignore them and list them; its tests carry the dirty rows. GATE-03's VALIDATION row (L73) already carries D-46.
- **agy C5 (Medium): `relaunch_note` survives a Validate retry.** Stated design (52-26:34: "A retry of the same stage … keep it"); the note is cleared at every stage exit.
- **agy C6 (Medium): HEAD-then-dirty read order.** The sequence yields a PASS on H0 while HEAD is H1, so approve runs Validate again: the conservative result. No ordering produces a false PASS.
- **agy C7 (Low): no streak count between 52-22 and 52-28.** An intra-phase window, never released; supervise is operator-bounded.
- **agy S1 (Low): timing race in the U8 e2e.** 52-32:147 already names the pure-function row as the equal-deadline pin, and a late gate still relaunches at the reset.
- **pi C1 (Critical): 52-08's gate fails on an unnamed test.** 52-08:177 requires existing Ship-path tests that asserted Validate→Ship to be rewritten to the finish or deleted, and listed in the SUMMARY (:180). `external_verify_agreement_advances_to_ship` (`pipeline_outcomes.rs` L1819, assert L1850) is in that class.
- **pi C5 (Medium): the repair leaves the file dirty.** Only when the file also carries the operator's own uncommitted edit, in which case the tree is dirty anyway and D-46 is right to refuse; a leak alone is committed (`gsd_config.rs` L320-339). Residue: V3(ii).
- **pi C7 (Medium): the `--until` halt clears the note.** The halt fires after the stage completed, mirroring the existing `code_fix = None` (L86); "a stop and resume" means `devflow stop`. `resume` re-running the halted stage (`pipeline_launch.rs` L3669) is pre-existing behaviour.
- **pi S1: the rule-6 control is a second implementation.** 52-29:366 passes the routing function to the property as a parameter; the real `next_step` is the subject, the mutant only the control.
- **pi S3: no counter on "Validate again".** Each cycle ends at a gate; T-52-62 (:417) accepts it, and abort ends the run.

## Plausible

- **pi C2: continuation after a refused answer.** 52-29:38/:271 says "reopens the gate" but not that the supervisor waits again; today's wait is single-shot (`pipeline_gate.rs` L631-700). The rewritten tests (:280-282, :297, :304) only pass if it waits again, so a fall-through would fail loudly. One line: "a refusal re-enters `gate_wait_call` on the reopened gate".
- **pi C3: a double budget reset.** `reset_phase_failures_at_ceiling` is keyed on the caller's `ceiling_gate` (L802-811); if 52-29 keeps 52-28's call beside its own, "one event" tests fail loudly. One word: "moves 52-28's reset into the dispatch".
- **pi S2: read count after 52-33.** The control at 52-29:142 answers through the gate file, not `devflow gate approve`; unsettled until the code exists, and a wrong count fails loudly.
- **host: projects that ignore `.planning/`.** In a `commit_docs: false` project with `.planning/` ignored, VERIFICATION.md is outside both HEAD and the porcelain read, so GSD's status can change with HEAD and tree unchanged. One 52-29 Limits line.

## Simplification proposals

- Keep the 52-26 scan with V2's rule; pi's private `State::stage` + `set_stage` is a semver change in published `devflow-core`.
- Merge `relaunch_note` and `code_fix_note` (pi); raised in round 4 and declined at 52-29:578.
- Narrow the `workflow_finished` emission scan to "one `stopped_at` site plus one `finished` site" (pi).
- The 52-33 approve print could print the rule and skip `read_evidence` (pi; 52-33:310 has the option).
- Clear `validate_pass_head` when the run leaves Validate (agy): unnecessary under D-46; 52-22's test pins "survives a transition".

## What this check does not establish

- V1 was shown with gsd-core 1.20.1's `config-new-project '{}'` and one `config-set`, not a live `/gsd-plan-phase` stage. Whether projects from other GSD versions, or from new-project in `--auto` mode (which commits the key, `auto-mode-config.md` L167-173), lack the key is unverified.
- The round trip used the real `set_auto_chain_active` with the workspace's `preserve_order` feature over four shapes (this repo's config, a fresh GSD config, a key-less file, a 4-space file); other JSON shapes were not tested.
- The V2 rule was tested at HEAD, not at 52-26's wave; plans 52-08..25 can add or remove hits.
- Plans were read in targeted windows; 52-30 was not read, and 52-31/32/33 only where cited. No cargo test or proptest ran.
- Each negative-control search only shows the refuting text is absent under the patterns used.
