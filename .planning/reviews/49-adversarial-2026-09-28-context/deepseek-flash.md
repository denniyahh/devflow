# Adversarial review — 49-CONTEXT.md (re-scoped 2026-09-28)

Snapshot root = `/home/denniyahh/.cache/devflow-review/phase49-context-066251f/snapshot`. All paths below are relative to it. Line numbers re-checked against this snapshot.

## CONFIRMED

**C1. `merge_result` carries no merge target — criterion 2's stated observation route is unsupported.**
`crates/devflow-core/src/hooks.rs:231-233` emits `"merge_result", serde_json::json!({"merged": true, "branch": branch})`. The other two emits (`:201-203`, `:213-215`) are the same shape. Nothing in the event names the target; `grep base_branch` over `pipeline_launch.rs`/`commands.rs`/`events.rs` finds no event field for it. The target is `ctx.git_flow.develop` = the configured `base_branch` (`hooks.rs:181-207`, `pipeline_outcomes.rs:1067`), only inferable from the persisted state file.
*Why it matters:* D-23 says "criterion 2's merge target is observed from the run's events (`merge_result`)". It is not. ROADMAP criterion 2's "first observation of the Ship merge target" needs a defined evidence source.
*Recommendation:* define the observation as `state-01.json`'s `base_branch == integration` **plus** a git assertion (`git merge-base --is-ancestor feature/phase-<N> integration` / integration tip is the `--no-ff` merge), and stop citing `merge_result` for the target.

**C2. D-21's MAJOR-gate line citation is wrong (claim is right, citation is not).**
`preflight.rs:724-760` is inside the D-09 explanatory comment block. `fn preflight_major_bump_check` starts at `preflight.rs:793` and closes at `:848`; its property "no `yes_ship` handling of its own" is true (no `yes_ship` token in the function), and it reaches a gate via `run_gate` → `run_gate_with_timeout(..., None)` (`pipeline_gate.rs:352-368`, doc at `:358-368`).
*Recommendation:* cite `preflight.rs:793-848`; the canonical-ref range `:724-814` merely grazes the function.

**C3. `--yes-ship` is not one-shot and is not consumed.**
`pipeline_outcomes.rs:838-848`: every `handle_ship_outcome` call does `let auto_response = state.yes_ship.then(|| GateResponse { approved: true, ... })`. `grep yes_ship` shows no site clears it. So a run that reaches Ship twice (Code loop-back then Ship again) auto-approves both times. The MAJOR gate and the reopened finalization-retry gate pass `None` and can never be auto-approved (`pipeline_gate.rs:360-368` doc + `pipeline_outcomes.rs:508-512`).
*Why it matters:* D-23's "auto-approves the Ship gate once" is the load-bearing justification for a throwaway repo; the accurate statement is "auto-approves every routine Ship gate in this run; preflight and finalization-retry gates still never auto-approve."

**C4. D-27's `stopped:` response has no GSD support, and GSD resume is plan-level, not task-level.**
`_external/gsd-core/workflows/execute-phase.md:1061-1076` (standard checkpoint flow) enumerates the accepted responses: `"approved"/"done" | issue description | decision selection`, then step 6 "Spawn continuation agent (NOT resume)". There is no `stopped:` arm anywhere in `_external/gsd-core/`. `execute-phase.md:1455` (the line the CONTEXT cites) says resume "resumes from first incomplete plan" — not at the first incomplete *task*. The CONTEXT's own superseded research states this explicitly: `49-RESEARCH.md:132` — "GSD has no per-task resume, so a re-presented live plan starts at Task 1."
*Why it matters:* D-27 asserts (a) the plan "fails its gate and writes no SUMMARY" and (b) a plain `/gsd-execute-phase 49` "re-presents the checkpoint". (a) is not a GSD behaviour — a non-approval response spawns a continuation agent. (b) holds only if the checkpoint plan has one task or every earlier task is idempotent; otherwise earlier tasks re-run, which the CONTEXT does not constrain.
*Recommendation:* require the live-checkpoint plan to be single-task (or give each pre-checkpoint task a tool-evidence skip), and have the plan's own `<action>` define the stop convention rather than citing a GSD mechanism that does not exist.

**C5. DECN-03's capture trigger cannot be "the gate watcher's snapshot at each gate open".**
The auto-decide branch writes no gate: `pipeline_launch.rs:1886-1887` — `} else if state.mode == Mode::Auto { return relaunch_checkpoint_session(&mut state, &session_id); }` — no `run_gate`/`Gates::write_gate`. `checkpoint_auto_decided` is emitted before the spawn (`pipeline_launch.rs:1247-1258`). The resumed capture lands in `.devflow/phase-NN-stdout` (`agent_result.rs:3056-3057`) and is archived to `.devflow/history/phase-NN/<stamp>-stdout` at the next launch (`agent_result.rs:3209-3227`).
*Why it matters:* Claude's Discretion says the gate watcher "at each gate open should capture everything DECN-03 needs". No gate file is ever written on the path that produces DECN-03's evidence, so that trigger never fires.
*Recommendation:* trigger the capture on the `checkpoint_auto_decided` event or a watch on `.devflow/phase-NN-stdout`; record that the resolution is read from the post-resume capture and/or the history archive (which is what makes it race-free after the run).

**C6. The scaffold does not build the topology D-06/D-20 assume, and the CONTEXT never says the required deltas.**
`scripts/scratch-dogfood-repo.sh:193-196` commits on `main`, then `git branch develop` / `checkout -q develop`. It creates no `integration` branch, no `devflow.toml`, no `gate="blocking-human"` task, and never pushes. DevFlow requires the base branch to exist **locally** when `base_branch` is explicitly configured: `commands.rs:405` calls `ensure_base_is_a_local_branch`, which does `show-ref --verify --quiet refs/heads/{base}` (`commands.rs:168-187`); and `commands.rs:456` calls `ensure_phase_reachable_on_base` (payload phase heading must be on it). Nothing in the run ever pushes `integration` — the after-Ship hooks only merge locally in the project root (`hooks.rs:180-207`, `pipeline_outcomes.rs:1003-1017`).
*Why it matters:* D-20 says "Build it by extending `scripts/scratch-dogfood-repo.sh`"; D-06 assumes `base_branch = integration`. A planner who extends only to add `devflow.toml` will fail at `start` with a missing local branch, or will fork a base that lacks the payload phase.
*Recommendation:* enumerate the exact scaffold deltas as plan tasks (create/commit on `integration`; write `devflow.toml`; seed the checkpoint task; add `external_verify`; record that `integration` is local-only).

**C7. "Validate does real work" is unevidenced, and `DocsUpdate` is an unmentioned hook.**
The scaffold's plan declares no `external_verify` frontmatter (`scripts/scratch-dogfood-repo.sh:133-180`), and Validate's outcome is derived from the agent's reported verdict (`classify_validate_outcome`, `pipeline_outcomes.rs:229`+, sealed by `(_, Success, Some(Pass)) => Passed`). `hooks_for_transition(Validate, Ship) = [DocsUpdate]` (`hooks.rs:100-104`), and `docs_update` runs `cargo doc --no-deps`, failing soft (`hooks.rs:237-253`).
*Why it matters:* D-20 asserts "Code and Validate do real work and can pass… research confirms what the Validate stage runs"; no cite is given. In a non-Rust project the only DevFlow-side command at that transition is a `cargo doc` that warns and skips. The CONTEXT's hook enumeration (D-21, D-23, question 4's list) omits `DocsUpdate` entirely.
*Recommendation:* make the extended payload plan declare a concrete `external_verify` command so Validate has a DevFlow-side signal, and note that `DocsUpdate` fails soft in a non-Rust target.

**C8. GSD's PR base is not `origin/HEAD`.**
`_external/gsd-core/workflows/ship.md:46` — `BASE_BRANCH=$(gsd_run query git.base-branch)`; `:373` — `--base "${BASE_BRANCH}"`. `origin/HEAD` appears only as a fallback in execute-phase's branching code, not in `ship.md`. The scaffold's GSD config is `{"workflow": {}}` (`scripts/scratch-dogfood-repo.sh:187-191`), which sets no git base-branch key.
*Why it matters:* D-06 asserts "GSD's ship and PR target stays the repository's `main` (`origin/HEAD`)". If `gsd_run query git.base-branch` resolves to `develop` (the scaffold creates it locally but never pushes it), `gh pr create --base develop` fails and the Ship stage dies.
*Recommendation:* verify the query's resolution for a fresh repo with `{"workflow": {}}` before relying on it; either set GSD's git base-branch explicitly in the scaffold's `.planning/config.json` or push whatever branch the query names.

**C9. D-28's `/gsd-quick` / 49.x fix route collides with the phase-worktree guard.**
`scripts/lint-phase-worktree.sh:98-114` hard-refuses staged `crates/*` (`crates/*) staged_source+=...`, then `echo "pre-commit: refusing to commit phase ${phase} source on '${current_branch}'."`, `exit 1`) whenever `feature/phase-${phase}` has a worktree and HEAD is not that branch; `scripts/hooks/pre-commit:143-155` invokes it. `phase` comes from `.planning/STATE.md`'s `current_phase:`.
*Why it matters:* while Phase 49 has a worktree and `current_phase: 49`, a DevFlow-source fix committed on the main checkout or on `feature/phase-49.x` is refused unless `DEVFLOW_ALLOW_OFF_WORKTREE=1`. D-28 does not mention this, and question 7's premise ("does anything block commits to crates/…") is answered **yes**.
*Recommendation:* decide the fix vehicle's branch up front (do source fixes inside the phase-49 worktree, or explicitly set the escape hatch, or update `current_phase`), and record it in D-28.

**C10. The payload checkpoint must be detected as `blocking_human`, not merely `human-action`.**
`verify.rs:153-155`: `HUMAN_ONLY_CHECKPOINT_MARKERS = [concat!("gate=", "\"", "blocking-human", "\""), concat!("type=", "\"", "checkpoint:human-action", "\"")]`. `phase_has_human_only_checkpoint` accepts either (`verify.rs:379-383`), so Define/Code preflight refuse for either. But the auto-decide resume route requires `phase_has_blocking_human_checkpoint`, i.e. `declaration.blocking_human` only (`verify.rs:131-134`, used at `pipeline_launch.rs:1777-1779`).
*Why it matters:* if the seeded payload checkpoint is written as `<task type="checkpoint:human-action">` with no `gate=` attribute, the two preflight gates still fire but the resume route never arms — the checkpoint becomes a **third** human gate, breaking D-22 and leaving DECN-03 unexercised.
*Recommendation:* pin the exact opening tag (`gate="blocking-human"` literally) and add a plan assertion on that marker string.

## SUSPECTED

**S1. D-11 ("only a run that ends without reaching Ship counts") has no evidence source.** (Speculation on intent.) Nothing in the CONTEXT says who decides or from what artifact. A run whose `/gsd-ship` succeeded (PR open, `merge_result` true) but whose after-Ship hook batch failed — `run_checkout_hooks` reopens the Ship gate on a terminal-hook failure (`pipeline_gate.rs:213-256`) — is ambiguous. Checked: the only durable signals are `merge_result`, `hook_run`, and the persisted state. Recommend defining "reached Ship" as "at least one `gate_resolved` for stage `ship` with `approved: true`" or similar, in the runbook.

**S2. Inserting/executing 49.x while 49 is mid-execution.** `gsd-phase`/`gsd-quick` are not in the copied `_external/gsd-core` subset, so I could not verify whether GSD permits a decimal phase insert while an incomplete plan exists in another phase. The worktree-guard collision (C9) is confirmed; the GSD-side behaviour is not.

**S3. The negative arm could refuse for a different reason and still pass.** I verified the intended refusal is the *only* reason on the `claude` path (major-bump and gh-auth apply only at Ship; claude is `HeadlessSafe`, so interactivity passes — `agents/mod.rs:122-124`, `preflight.rs:634-683`). But I could not execute the arm. The assertion must also assert the **absence** of other refusal clauses (e.g. it would break if the fixture lacked `develop`, since `ensure_phase_reachable_on_base` runs before preflight). Recommend the negative fixture's check asserts the exact refusal reason *and* that no second `[… does not hold …]` clause is present.

**S4. Race window for the resumed capture.** Verified the resume capture is archived at the *next* launch (`agent_result.rs:3209-3227`), so a post-run read of `.devflow/history/` is race-free. What I could not verify is whether the runbook/watch captures the *pre-resume* Code capture (which contains the checkpoint report) before it is overwritten — it is also archived, so this is recoverable, but the plan should name the archive path explicitly.

## CHECKED AND FINE

- **D-22's "exactly two human gates" holds on the intended path.** The unattended check applies only at Define/Code (`preflight.rs:971-972`); with the seeded plan present at both, the `blocking-human` branch returns `DoesNotHold` (`preflight.rs:1076-1080`) → a preflight gate at each (`run_preflight` → `run_gate`, `preflight.rs:1403-1407`). Plan/Validate/Ship preflights carry no human-only check; Validate gates in Auto only at `consecutive_failures >= 3` or `phase_validate_failures >= 10` (`mode.rs:243-260`); the Ship gate auto-approves under `--yes-ship` (C3); the MAJOR gate is Ship-only (`preflight.rs:759-762`) and passes `None`. The seeded checkpoint is auto-decided without a gate (`pipeline_launch.rs:1886-1887`) because the Code preflight approval records the set (`preflight.rs:1432-1434`). The `unattended-drill.sh` header's "Plan can never receive this bypass" (`scripts/unattended-drill.sh:26-29`) concerns GSD's chain flag, not a DevFlow gate, so it does not add a third gate. Caveat: any Define/Plan/Code **failure** adds a never-silent gate (`pipeline_outcomes.rs:743-780` doc; `pipeline_gate.rs:40-50`), so "exactly two" is a happy-path statement.
- **D-03 line cites are exact:** `:1076-1080` (DoesNotHold + message), `:971-972` (`matches!(stage, Stage::Define | Stage::Code)`), `:1432-1434` (`if stage == Stage::Code { record_checkpoint_set_for_code_evaluation(state)?; }`).
- **D-23 line cite `pipeline_outcomes.rs:839` is exact** (`let auto_response = state.yes_ship.then(|| GateResponse {`).
- **D-14 citation is right:** `scripts/scratch-dogfood-repo.sh:187-191` writes `.planning/config.json`; the refusal substring is verbatim in `preflight.rs:993-995` (`"no \`.planning/config.json\` under the launch root — nowhere for the chain flag \`" `).
- **D-15 citation `pipeline_launch.rs:1777` is exact** (`let checkpoint_confirmed = state.agent == AgentKind::Claude`).
- **D-06 citation `config.rs:351` is exact** (`if value == MAIN {`), and `ensure_base_ref_current` fails open for a local-only base (`preflight.rs:556-573`).
- **Gate timeout = 3 days is correct** (`config_parse.rs:16, 25, 42`); `DEVFLOW_GATE_NOTIFY_CMD` is a no-op unless set (`gates.rs:388-394`).
- **D-21's bump-classifier concern is achievable:** fresh repo, `highest_semver_tag` → `None`, `reachable_semver_baseline` → `None`, `range_start = ""`, whole-history `classify_range_bump` (`version.rs:652-688`); the scaffold's `chore:` commit → `Bump::None` → Patch floor → `v0.0.1`. MAJOR requires `!`/`BREAKING CHANGE:` (`version.rs:385-420`). Note: `version.rs` is **not** in the CONTEXT's canonical refs, so the planner is told to read about the gate but not the classifier.

## Still open (decision points I raised)

1. C1 — the exact evidence source for criterion 2's "Ship merge target" (state file + git, or a new event). Needs a decision.
2. C4 — the single-task constraint / stop convention for the live-checkpoint plan. Needs a decision.
3. C5 — the DECN-03 capture trigger (event-based, not gate-based). Needs a decision.
4. C6/C8 — the scaffold deltas (`integration`, `devflow.toml`, GSD git base-branch) and whether `integration` must be pushed. Needs a decision.
5. C7 — whether the payload plan declares `external_verify` (Validate's DevFlow-side signal). Needs a decision.
6. C9 — the fix vehicle's branch vs the worktree guard (D-28). Needs a decision.
7. C10 — the literal opening tag for the seeded checkpoint. Needs a decision.
8. S1 — the definition of "an attempt that reaches Ship" for D-11. Needs a decision.
