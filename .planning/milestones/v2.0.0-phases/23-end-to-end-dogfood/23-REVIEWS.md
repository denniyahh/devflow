---
phase: 23
reviewers: [codex, opencode, hermes]
reviewed_at: 2026-07-26T00:16:53Z
plans_reviewed: [23-03-PLAN.md, 23-04-PLAN.md, 23-05-PLAN.md, 23-06-PLAN.md, 23-07-PLAN.md, 23-08-PLAN.md, 23-09-PLAN.md, 23-10-PLAN.md, 23-11-PLAN.md]
skipped_reviewers: [cursor, antigravity]   # operator instruction; both are in review.default_reviewers
verdict: changes_requested
---

# Cross-AI Plan Review — Phase 23 (replan: 23-03 … 23-11)

Three independent lanes, three different providers. `cursor` and `antigravity`
were skipped at the operator's instruction; `hermes` is not a GSD-known reviewer
slug and was run manually alongside the built-in dispatch.

All three lanes were given the same file-reference prompt and read the repository
themselves — findings below cite `file:line` in the live tree, not plan text.

| Lane | Provider / model | Risk | Verdict |
|---|---|---|---|
| Codex | OpenAI (default) | **MEDIUM** | request changes before executing 23-06 onward |
| OpenCode | GitHub Copilot | **MEDIUM** | sound, execute with CI-flakiness caveat |
| Hermes | deepseek-v4-pro | **LOW** | executable |

**Orchestrator note.** Every HIGH finding below was independently re-verified
against source by the orchestrator before this file was written; the verification
commands and their results are recorded in the Consensus section. Codex's three
HIGH findings all reproduced. Two of them are missed by the other two lanes.

---

## Codex Review

## Summary

The replan is mostly aimed at the measured failure, not the disproved socket-supervisor hypothesis: registry visibility, bounded gate lifetime, `devflow stop`, Ship evidence, `--yes-ship`, and a guarded one-way acceptance run all map to the probe/forensics record. The main problem is that one load-bearing 23-06 oracle predicate is still false-green-prone: `workflow_finished` is not uniquely a shipped event. I would request changes before executing 23-06 onward, mainly to fix that predicate, harden registry concurrency, reorder 23-10’s authorization, and replace verification shell idioms that can mask test failures.

## Strengths

- The plan set is correctly re-aimed at the measured defect. The forensics doc says the real issue is “bounded but operationally indistinguishable from unbounded” gate waits plus orphaned `devflow advance` processes, and explicitly recommends registry, gate lifetime, stop, and false-green fixes before acceptance (`.planning/phases/23-end-to-end-dogfood/23-ORPHAN-FORENSICS.md:176`). The current roadmap lists exactly those replacement plans, not a socket migration (`.planning/ROADMAP.md:878`).

- Claim A is source-backed. Ship approval is consumed in `handle_ship_outcome`, and only `GateAction::Advance` calls finalization (`crates/devflow-cli/src/pipeline_outcomes.rs:274`). The terminal hooks run inside `finish_workflow_with_gate_timeout` (`crates/devflow-cli/src/pipeline_gate.rs:182`), using `hooks_after_ship()` (`crates/devflow-core/src/hooks.rs:105`). So a pre-Ship-gate merge-evidence hook would run before the merge exists.

- Claim B is source-backed. `BranchCleanup` runs after `Merge` in the terminal hook batch (`crates/devflow-core/src/hooks.rs:105`), and branch cleanup may delete the feature branch (`crates/devflow-core/src/hooks.rs:121`). `is_merged_into_develop` intentionally returns false when the branch is absent (`crates/devflow-core/src/git.rs:89`). A post-batch ancestry check would therefore fail closed after successful cleanup; the merge post-condition belongs inside `merge_feature`.

- Claim D is source-backed. The monitor trap only kills `$apid`, the coding-agent process (`crates/devflow-core/src/monitor.rs:148`), while `devflow advance` holds the per-phase lock (`crates/devflow-cli/src/pipeline_launch.rs:286`). Targeting `.devflow/lock-{phase:02}` for `devflow stop` is the right mechanism.

- 23-10 covers both walls hit by the probe. It explicitly gates SECURITY.md/config alignment and false-green evidence cleanup before 23-11 (`.planning/phases/23-end-to-end-dogfood/23-10-PLAN.md:59`, `.planning/phases/23-end-to-end-dogfood/23-10-PLAN.md:244`, `.planning/phases/23-end-to-end-dogfood/23-10-PLAN.md:256`).

- The current plans avoid the known `cargo test --exact` vacuity trap. The CLI package is correctly `devflow`, not `devflow-cli` (`crates/devflow-cli/Cargo.toml:1`), and the targeted package commands I saw use `-p devflow` / `-p devflow-core` with nonzero pass-count greps rather than `--exact`.

## Concerns

- **HIGH: 23-06’s shipped predicate can false-pass stopped workflows.** Plan 23-06 defines `ShipEvidence::shipped` as “whether `workflow_finished` has been emitted” (`.planning/phases/23-end-to-end-dogfood/23-06-PLAN.md:152`). But `transition()` also emits `workflow_finished` for `--until` stops with payload `{ "reason": "stopped_at" }`, before normal transition hooks or Ship finalization (`crates/devflow-cli/src/pipeline_gate.rs:67`). The roadmap already records Phase 21 as “`workflow_finished` after one stage” (`.planning/ROADMAP.md:750`). This makes claim C only partially true: `workflow_finished` from `finish_workflow_with_gate_timeout` is guarded by terminal hooks (`crates/devflow-cli/src/pipeline_gate.rs:188`), but the event name alone is not a valid shipped oracle.

- **HIGH: 23-03’s registry concurrency accepts lost registrations while claiming none can be missing.** The plan’s concurrent criterion only requires the roots file contain “at least one” of two concurrent entries (`.planning/phases/23-end-to-end-dogfood/23-03-PLAN.md:178`), and the threat model says a concurrent registration “can lose an entry” (`.planning/phases/23-end-to-end-dogfood/23-03-PLAN.md:300`). That contradicts the success criterion that “a running phase cannot be missing from the registry” (`.planning/phases/23-end-to-end-dogfood/23-03-PLAN.md:317`). Since launch records one monitor PID per phase on the hot path (`crates/devflow-cli/src/pipeline_launch.rs:126`), a lost registry entry can make `gate list --all-roots`, sweep, and stop miss an active gated run.

- **HIGH: 23-10’s authorization checkpoint is ordered before the recovery proof it requires.** Task 1 asks the operator to confirm the Task 2 recovery rehearsal completed successfully (`.planning/phases/23-end-to-end-dogfood/23-10-PLAN.md:151`), but Task 2 starts later (`.planning/phases/23-end-to-end-dogfood/23-10-PLAN.md:154`). For a one-way unattended merge, the recovery ref and rehearsal should exist before final authorization is requested.

- **MEDIUM: Several verification commands can mask failed tests.** Multiple plans use patterns like `cargo test --workspace 2>&1 | rg -q 'test result: FAILED' && exit 1 || cargo clippy ...` (`.planning/phases/23-end-to-end-dogfood/23-04-PLAN.md:234`, `.planning/phases/23-end-to-end-dogfood/23-06-PLAN.md:245`, `.planning/phases/23-end-to-end-dogfood/23-09-PLAN.md:220`, `.planning/phases/23-end-to-end-dogfood/23-11-PLAN.md:211`). If `cargo test` fails before printing that exact string, `rg` returns nonzero and the `||` branch can continue, recreating a false-green verification path.

- **MEDIUM: 23-11 mixes “valid run record” with “acceptance passed.”** Task 1’s automated verification requires a `workflow_finished` event (`.planning/phases/23-end-to-end-dogfood/23-11-PLAN.md:148`), while the same plan says an incomplete run should still be documented accurately (`.planning/phases/23-end-to-end-dogfood/23-11-PLAN.md:115`, `.planning/phases/23-end-to-end-dogfood/23-11-PLAN.md:315`). Those should be separate outcomes: an honest failure record can be valid evidence, but it should not satisfy phase acceptance.

- **LOW: 23-04 and 23-05’s real-child tests are sound but expensive/flakiness-prone.** `Gates::poll_response` uses exponential backoff capped at 60 seconds (`crates/devflow-core/src/gates.rs:219`), and the plans intentionally spawn a real `devflow advance` child around that behavior (`.planning/phases/23-end-to-end-dogfood/23-04-PLAN.md:220`, `.planning/phases/23-end-to-end-dogfood/23-05-PLAN.md:128`). The lock/gate polling mitigations help, but these should be treated as slow integration tests with explicit timeouts.

## Suggestions

- Change 23-06’s strict predicate to either a new terminal-only event, such as `workflow_shipped`, emitted only in `finish_workflow_with_gate_timeout`, or parse `workflow_finished` and reject `reason: "stopped_at"`.

- Serialize registry writes or use one file per root/phase. Then change the concurrent test to require both registrations survive.

- Reorder 23-10 so recovery ref creation, rehearsal, binary rebuild checks, and content preconditions run before the final one-way authorization checkpoint.

- Replace shell verification chains with direct status checks: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`. If logs are needed, tee output while preserving command exit status.

- Split 23-11 reporting into two explicit statuses: “acceptance passed” requires shipped evidence; “run record valid” can pass for an incomplete run only if it records exact stop cause and cleanup state.

## Risk Assessment

**MEDIUM.** The replan is directionally right and most source-mechanism claims A, B, and D check out. But the 23-06 shipped oracle and masked verification commands are exactly the kind of false-green surface Phase 23 is supposed to eliminate, and 23-10’s checkpoint ordering is too risky for a one-way self-hosted merge until corrected.
---

## OpenCode Review


## 1. Summary

The nine-plan set is **sound and genuinely re-aimed** at the measured evidence from the probe and orphan forensics, not at the invalidated "monitor dies silently" hypothesis. The plans’ four load-bearing overrides of the RESEARCH.md are all independently **verified against source**. The phase’s acceptance criterion (one unattended run, self-hosted, through Ship) is reachable with the gates this plan set builds. The principle weakness is that genuine end-to-end process-liveness tests (23-04 Task 3, 23-05 Task 1) carry a 60-second backoff window that, while deterministic, will be slow in CI and is described as optional (“if the bounded wait proves flaky”). The plan set earns confidence through extensive source-line citation and explicit, recorded overrides of the research document; the primary residual risk is that CI-level flakiness in the real-child-process tests has not yet been measured.

## 2. Strengths

- **Claim A (merge-after-gate) verified.** `hooks_after_ship()` (`crates/devflow-core/src/hooks.rs:105-112`) returns `[Merge, VersionBump, ChangelogAppend, BranchCleanup]`. This batch runs only inside `finish_workflow_with_gate_timeout` (`crates/devflow-cli/src/pipeline_gate.rs:188`), which is reachable only from `handle_ship_outcome`’s `GateAction::Advance` arm (`crates/devflow-cli/src/pipeline_outcomes.rs:275-286`) — after gate approval. The RESEARCH.md’s suggested pre-gate placement would fire before any merge exists. This directly validates plan 23-06’s objective reasoning.

- **Claim B (post-batch ancestry fails) verified.** `hooks_after_ship` finishes with `BranchCleanup` (`hooks.rs:110`), which calls `delete_branch` (`hooks.rs:121-141`). The branch is gone after the batch. `GitFlow::is_merged_into_develop` returns `false` on an absent branch (`crates/devflow-core/src/git.rs:95-96`). Plan 23-06 correctly places the post-condition inside `merge_feature` (`hooks.rs:143-172`), before cleanup.

- **Claim C (gap is exposure, not correctness) verified.** `merge_feature` already refuses a missing branch (`hooks.rs:146-151`). `run_checkout_hooks` tracks `all_succeeded` (`crates/devflow-cli/src/pipeline_outcomes.rs:477-495`). `finish_workflow_with_gate_timeout` emits `workflow_finished` only when the entire batch succeeds (`pipeline_gate.rs:188-223`). DevFlow’s enforcement is sound; plan 23-06 exposes the authoritative event via `devflow evidence --require-shipped` rather than adding enforcement that already exists.

- **Claim D (SIGTERM-to-monitor orphans advance) verified.** The monitor script’s trap captures only `$apid` — the agent PID (`crates/devflow-core/src/monitor.rs:148-160`). By the time `advance` is the foreground child, `$apid` names a dead process. Plan 23-05 correctly targets `.devflow/lock-{phase:02}`'s PID, written by `lock::acquire` at `std::process::id()` (`crates/devflow-core/src/lock.rs:86-87`), and exposed via `lock::holder` (`lock.rs:145`). The plan also adds `agent::looks_like_devflow_process` (`crates/devflow-cli/src/commands.rs:173-178 in plan 23-05`) as a PID-identity guard, which is prudent.

- **Mechanism reuse is maximal and elegant.** The sweep and `devflow stop` both write rejection responses through the already-existing `Gates::respond` API (`crates/devflow-core/src/gates.rs:179-198`), which the still-polling `advance` process already consumes natively (`gates.rs:222-248` → `pipeline_gate.rs:284-318` → `abort` at `:322-335`). Zero new process-signalling machinery is needed. This is the strongest architectural call in the plan set.

- **The self-dogfood staleness wall is correctly preserved as an acceptance gate.** `staleness_outcome` hard-blocks only `(is_self_dogfood: true, Stale)` (`crates/devflow-cli/src/staleness.rs:276-284`). Plan 23-10 and 23-11 explicitly record whether this path was exercised, because it is structurally unreachable in a scratch repo. This is proper D-02 enforcement.

- **Both probe-observed Ship-block walls are checked before the acceptance run.** Plan 23-10 Task 3 checks for a security artifact in the target phase and for any false-green Ship-completion claim at Validate — the two explicit, different content gates that stopped the two recorded probe runs (`23-ORPHAN-FORENSICS.md:136-145`).

- **Pitfall 7 (auto-approving the wrong gate) is prevented by construction.** Plan 23-09 scopes the auto-response to `handle_ship_outcome`’s call only — `finish_workflow_with_gate_timeout`’s reopened retry gate passes `None` and cannot reach the auto-approving branch. Guarded by a named negative regression test (plan 23-09 Task 2).

- **Threat models are thorough.** Each plan carries a STRIDE table with concrete mitigations. Notable: T-23-41 scores EoP at HIGH and mitigates by denying `Gates::reap` a boolean parameter (Task 1 acceptance criteria assert the literal `approved: false`). T-23-42 scores a zero-threshold DoS at HIGH and mitigates by treating zero as unset.

## 3. Concerns

- **HIGH — 23-04 Task 3 and 23-05 Task 1 spawn real `devflow advance` child processes with a 60s poll-backoff window.** The `Gates::poll_response` loop (`crates/devflow-core/src/gates.rs:229-247`) backs off from 1s to a 60s cap. A response written by the sweep/stop after a poll check returns false takes up to 60s to be noticed. The plans handle this by setting `DEVFLOW_GATE_TIMEOUT_SECS` per-`Command` (good — avoids `set_var`) and using lock-file detection as synchronization points (good — no fixed sleeps). However, a 60-second worst case per test means the e2e tests in `gate_sweep_e2e.rs` and `stop_e2e.rs` could take ~2 minutes each. In CI under resource pressure, the child’s own polling could back off unpredictably. The plans acknowledge this (“If the bounded wait proves flaky in CI…”) but only offer the timeout override as remedy. The tests could flake due to CI clock jitter or CPU starvation, and the plan set has no explicit CI-timeout configuration recommendation.

- **MEDIUM — The acceptance criteria verification commands chain `rg -q` through multi-pipeline shell with non-obvious exit-semantics.** Example from 23-03 Task 3: `cargo test -p devflow gate 2>&1 | rg -q '^test result: ok\. [1-9][0-9]* passed' && cargo clippy --workspace ...`. If `cargo test -p devflow gate` matches zero tests (e.g., the filter name is misspelled or the test module hasn’t been created yet), the output says “0 passed”, `rg` fails (no `[1-9]` match), exiting non-zero — correct fail-closed. But the `rg -q` prefix before `^test result: ok\.` means a cargo build error would be silently absorbed without `rg` seeing the test result line at all. The commands are shell-golf that depend on the `[1-9]` guard to catch empty test runs; a future executor replacing the regex with a simpler `rg -q 'test result: ok'` would make it vacuously pass on zero tests. This specific hazard (`cargo test` matching nothing exits 0) is explicitly called out in the review brief and is worth a structural guard beyond regex.

- **MEDIUM — `handle_ship_outcome` currently calls `run_gate` (a one-arg wrapper), not `run_gate_with_timeout` (the three-arg body).** Plan 23-09 Task 1 says to “replace the `run_gate` call” in `handle_ship_outcome` with a direct `run_gate_with_timeout` call. The existing code at `crates/devflow-cli/src/pipeline_outcomes.rs:275-286` calls `run_gate`, which internally calls `run_gate_with_timeout` with the 7-day timeout (`pipeline_gate.rs:230-237`). The plan’s intent is to change this **one** call site to pass `Some(auto_response)` when `state.yes_ship` is set, while `run_gate`’s other callers (via `run_gate`) stay unchanged. This is correct, but the byte-level precision matters: if the executor inlines the `auto_response` injection into `run_gate_with_timeout`’s body (as a conditional on `state`) rather than scoping it to the call site, trap 1 becomes reachable. The plan specifies the call-site approach, but the task text says “change `run_gate_with_timeout`'s signature to take an additional `auto_response` parameter” — the executor must resist the urge to make `run_gate_with_timeout` also read `state.yes_ship` directly, which would couple two concerns and possibly affect the retry gate path.

- **MEDIUM — 23-10 Task 2’s “rehearse remote restore” may fail on branch-protected `develop`.** The task instructs to determine whether a force-push to `develop` is permitted or refused by branch protection, then establish “the exact command sequence.” Plan 23-10’s objective correctly identifies that D-07’s accepted risk is only acceptable if the undo works. But this rehearsal is a discovery step, not a guarantee — if the remote refuses the force-push and the only recourse is a revert-PR, that undo path is itself a multi-step operation that could take minutes to hours. The risk is correctly identified (T-23-102) but the mitigation depends on the rehearsal result.

- **LOW — `gate_max_unattended_age_secs` default is 6 hours (21600s), but the orphan forensics show 30-hour-old gates.** This is a deliberate on-demand-only decision (Open Question 3, resolved as on-demand). Six hours is reasonable for typical usage but is shorter than the oldest observed orphan (30 hours). The plan records this explicitly, but a naive reader might mistake “6 hours” for a reaper that auto-cleans everything — it only cleans what the operator tells it to. Not a defect; noted for completeness.

- **LOW — The false-green `cargo test --exact` hazard is not directly applicable to these plans (none use `--exact`), but multiple acceptance criteria depend on `cargo test -p devflow <filter>` where a misspelled filter name would exit 0 with zero tests matched.** The `[1-9]` regex guard catches this, but the plans document no convention requiring that guard in future criteria.

## 4. Suggestions

1. **Add a hard timeout to the real-child-process e2e tests.** A `std::time::Duration`-bounded wait on the child’s exit (e.g., 90s — 60s poll cap + 30s grace) with an explicit `panic!` on timeout, rather than relying on the child to eventually notice the response. This converts “potentially infinite CI hang” into “90s timeout with a clear message” regardless of clock skew or resource pressure.

2. **Make the “no vacuous pass” guard structural rather than regex-based.** Add a small assertion helper that, after `cargo test -p <package> <filter>`, parses the test result line and asserts `passed > 0`. The regex `[1-9]` is a guard but a dedicated function in the executor’s tooling would be harder to accidentally weaken.

3. **In 23-09 Task 1, add an explicit code comment on `handle_ship_outcome` stating why the auto-response is at this call site and not inside `run_gate_with_timeout`’s body.** The plan text says why, but the source should too — a future developer reading `handle_ship_outcome` won’t have the plan’s pitfall section in front of them, and a refactor that “simplifies” by folding `yes_ship` into `run_gate_with_timeout` directly would be catastrophic.

4. **Record the remote-restore rehearsal result in 23-10 Task 2 as a precondition-flag on 23-11.** If the remote rejects a force-push to `develop`, push the acceptance run decision back to the operator checkpoint with a concrete “the undo path is `<exact command sequence>` and takes `<estimated time>`” rather than proceeding with a recovery plan that hasn’t been exercised against the remote.

5. **Add `--exclude` to the full-workspace test run before the acceptance run to exclude the delete-verb-depended tests that are removed in 23-08.** The workspace test count delta between pre- and post-run should be predictable, and if `sequentagent` tests are removed in 23-07/23-08, the acceptance run’s post-conditions should not flag a declining test count as a regression. Plan 23-08 already requires recording the deliberate removal count, which is good — extend this to 23-10’s baseline.

## 5. Risk Assessment

**OVERALL: MEDIUM**

The architectural reasoning is sound, all four load-bearing source claims are verified, and the key mechanisms reuse existing APIs rather than introducing new primitives. The elevated risk comes from three factors: (1) the real-child-process e2e tests at the 60s poll cap are slow and their CI behavior is unmeasured; (2) the acceptance run is a one-way merge into `develop` with a recovery rehearsal that may reveal branch-protection restrictions at runtime rather than at plan time; (3) the `auto_response` parameter thread through `run_gate_with_timeout` — while correctly scoped to a single call site in the plan’s design — depends on the executor not “simplifying” it into the function body. If these three hold (the e2e CI tests pass without timeouts, the restore rehearsal succeeds against the actual remote rules, and the executor stays faithful to the call-site scoping), the plan set closes the phase’s stated acceptance criterion. The phase’s reward-to-risk ratio is favorable: nine additive plans (plus two subtractive cleanup plans) with zero new dependencies, no socket supervisor rewrite, and no process-signalling machinery that doesn’t already exist in the codebase today.

---

## Hermes Review (deepseek-v4-pro)

> Run manually via `hermes chat -Q -q` with a file-reference prompt (hermes is not in GSD's `KNOWN_REVIEWER_SLUGS`).

### 1. Summary

This nine-plan set re-aims Phase 23 against measured evidence, and it is the strongest plan set I have reviewed for this project. The re-aim is genuine — every plan traces its justification to the probe/forensics data rather than the invalidated "monitor dies" hypothesis. All four load-bearing claims (A–D) are verified against source: the plan author correctly identified why the research-recommended Ship-evidence placement is unworkable, why the post-condition must run before branch cleanup, why the false-green gap is one of exposure not correctness, and why the lock-file PID (not the monitor PID) is the correct target for `devflow stop`. The preconditions in plan 23-10 cover both walls the probe hit. One concern at MEDIUM severity about flakiness in the spawn-and-wait e2e fixtures; one at LOW about an overly strict acceptance regex. Overall risk: **LOW**.

### 2. Strengths

- **Scope re-aim is evidence-grounded and traceable.** Every plan's `<objective>` block explicitly states how the probe evidence (`23-PROBE-FINDINGS.md`, `23-ORPHAN-FORENSICS.md`) justifies the redefined scope. Plans that carry forward unchanged decisions (23-07/23-08 for the `sequentagent` deletion) document why the probe didn't touch them. The supervisor migration (D-08, D-10) is deferred with explicit reasoning at `23-03-PLAN.md:77-85` rather than silently dropped — the `~/.cache/devflow/` directory decision is reused for the registry while the socket mechanism itself is deferred.

- **Claim A verified — hooks_after_ship runs inside the gate loop, merge doesn't exist at approval time.** `handle_ship_outcome` (`crates/devflow-cli/src/pipeline_outcomes.rs:275-286`) calls `run_gate()` first; only on `GateAction::Advance` does it call `finish_workflow` → `finish_workflow_with_gate_timeout` which runs `run_checkout_hooks(project_root, state, &hooks::hooks_after_ship(), Stage::Ship)` (`crates/devflow-cli/src/pipeline_gate.rs:188`). The merge (`Hook::Merge`) is the first hook in `hooks_after_ship()` (`crates/devflow-core/src/hooks.rs:105-112`), so it executes AFTER gate approval. The research-recommended "before `handle_ship_outcome`" placement (`23-RESEARCH.md:462-473`) would indeed block 100% of legitimate Ships. Plan 23-06 correctly places enforcement inside `merge_feature` instead.

- **Claim B verified — BranchCleanup deletes the branch, is_merged_into_develop fails closed.** `hooks_after_ship()` returns `[Merge, VersionBump, ChangelogAppend, BranchCleanup]` (`crates/devflow-core/src/hooks.rs:105-112`). `GitFlow::is_merged_into_develop` returns `false` when the branch doesn't exist (`crates/devflow-core/src/git.rs:95-96`: `if !self.branch_exists(&branch) { return false; }`). A post-batch ancestry check would always fail because `BranchCleanup` runs last. Plan 23-06's placement inside `merge_feature` — before cleanup — is correct.

- **Claim C verified — `run_checkout_hooks` returning `all_succeeded` means merge happened.** The function runs hooks sequentially (`crates/devflow-cli/src/pipeline_outcomes.rs:491-511`), with terminal batches fail-fast on first error. `Merge` is the first hook in `hooks_after_ship()`, so `all_succeeded=true` guarantees a merge ran. The false-green gap IS one of exposure — the attestation document (`01-VERIFICATION.md`) can claim Ship completed without any structural check — not a correctness defect in the code path. Plan 23-06's Layer-0 probe (`devflow evidence --require-shipped`) is the right fix.

- **Claim D verified — SIGTERM to monitor_pid does not reach advance.** The generated monitor script (`crates/devflow-core/src/monitor.rs:148-160`) captures `$apid` (agent PID) in the trap, then `wait $apid`, then runs `devflow advance`. By the time `advance` runs, `$apid` refers to an exited process. The trap's `kill "$apid"` is a no-op. Meanwhile, `lock::acquire` writes `std::process::id()` — the `advance` process's own PID — into `.devflow/lock-{phase:02}` (`crates/devflow-core/src/lock.rs:84-88`). Plan 23-05 correctly targets the lock-file PID, not `state.monitor_pid`.

- **23-10 preconditions cover both probe walls.** The security-artifact wall (`/gsd-ship` preflight with `security_enforcement=true` and no `SECURITY.md`) is covered by Task 3's Precondition A check. The false-green attestation wall (`01-VERIFICATION.md` scoring an unrun Ship) is covered by Task 3's Precondition B check against the target phase's own acceptance criteria. Both checks independently verify `.planning/config.json` rather than taking `RESEARCH.md`'s word for the absence of a `security_enforcement` override — this is good paranoia.

- **`cargo test --exact` false-green is not present in these plans.** The plans use pass-count regex guards: `rg -q '^test result: ok\. [1-9][0-9]* passed'` (e.g., `23-03-PLAN.md:153`). The `[1-9]` requires at least one digit 1-9, so `0 passed` never matches. The `--test <name>` integration test targets (`--test stop_e2e`, `--test phase7_cli`, `--test devflow_dir_gitignore`) fail with a compile error if the test file doesn't exist, so they cannot vacuously pass.

- **Threat models are concrete and actionable.** Every plan includes STRIDE tables with specific mitigation strategies. T-23-52 (PID reuse in the lock file) is mitigated by `agent::looks_like_devflow_process` — fail-closed identity check. T-23-91 (auto-approving the retry gate) is mitigated by construction — the `auto_response` parameter is passed at exactly one call site. T-23-81 (narrowing the `.devflow` constructor guarantee) is mitigated by re-pointing rather than deleting coverage.

- **The re-aim explicitly records what was deferred and why.** `23-03-PLAN.md:76-85` records D-08 (supervisor), D-10 (in-process advance), and D-09 (socket directory) as deferred with rationale. `23-05-PLAN.md:69-74` records the reversibility cost of the new `devflow stop` verb. `23-09-PLAN.md:56-60` records the `--yes-ship` reversibility cost. This discipline prevents silent scope drift.

### 3. Concerns

- **MEDIUM — Spawn-and-wait e2e fixtures are inherently flaky on CI.** Plan 23-04 Task 3 and 23-05 Task 1 both spawn a real `devflow advance` child, wait for a lock file and gate file to appear, then assert behaviour. The plans use 60s poll backoff caps and set timeouts via `.env()` on the spawned `Command`. This is properly designed, but real child processes under CI (constrained CPU, I/O, PID namespace) can exceed any timeout. `23-05-PLAN.md:128-129` specifies `wait()` for reaping and timeout overrides on `Command`. The risk is that these tests become "sometimes flaky" rather than reliably passing. Mitigation exists (`.env()` timeouts) but one CI environment's 2-core VM under load can look very different from the developer's desktop. Consider a lower timeout (15-30s) for CI with a `DEVFLOW_TEST_` env override rather than a single hardcoded value.

- **MEDIUM — Plan 23-04's "no signal" guarantee is load-bearing but the acceptance criteria don't directly test it.** The `must_haves.truths` state "The sweep never sends a signal to any process — its only effect is writing a response file." This is structurally guaranteed because `Gates::reap` only calls `Gates::respond` (which only writes a file). But the acceptance criteria don't include an assertion that no `libc::kill`/`nix::sys::signal::kill` call path exists from the sweep CLI command. A source grep assertion (`rg -c 'kill'` in the sweep code path) would make this auditable. As written, the guarantee relies on code-review-level reasoning about the call chain rather than a mechanical check.

- **LOW — `cargo test -p devflow gate_list` substring matching could match unrelated tests.** Plan `23-03-PLAN.md:153` filters with `gate_list` as a test name substring. If another test in the `devflow` package happens to contain `gate_list` in its name, the pass-count regex would still match but the wrong tests could be running. This is low risk — `gate_list` is a command name, not a generic term — but worth noting alongside the review prompt's concern about false-green acceptance.

- **LOW — Plan 23-09 Task 2's negative test requires drilling into `finish_workflow_with_gate_timeout`'s internals.** The test ("drive `finish_workflow_with_gate_timeout` through a failing terminal hook and assert the reopened finalization gate has a request file and no response file") depends on constructing a state where a terminal hook fails. The plan doesn't specify _which_ terminal hook to fail or how to construct that failure deterministically. The executor will need to figure out a concrete mechanism — e.g., making `VersionBump` fail by pre-creating a conflicting tag. This is implementable but underspecified; the executor could waste time figuring out the test fixture rather than testing the invariant.

- **LOW — Plan 23-10 Task 2's "rehearse remote restore" requires branch-protection knowledge.** The task requires determining whether a force-push to `develop` is permitted by branch protection and establishing the real restore path. This is a human-ops question, not a code question. If branch protection is enforced and the operator doesn't have admin access during the acceptance window, the recovery point may be ceremonially useful but operationally useless. The plan acknowledges this by making it a checkpoint question, which is correct — but the review should note that if force-push is blocked and the only restore path is "open a revert PR," that path may take longer than the `devflow stop` timeout provides.

### 4. Suggestions

1. **Add a CI-specific timeout override for e2e spawn tests.** In 23-04 and 23-05, define `DEVFLOW_E2E_CHILD_TIMEOUT_SECS` (default 60s, but overridable to 15-30s in CI) and thread it into the spawned `Command`'s `.env()`. This lets CI run tighter timeouts without changing the source-visible default.

2. **Add a source grep assertion for the "no signal" guarantee in 23-04.** An acceptance criterion like `rg -c 'kill\|signal' crates/devflow-cli/src/commands.rs` restricted to the sweep function body would mechanically confirm no signal path exists. Currently the guarantee relies on reasoning about `Gates::reap` → `Gates::respond`, which is correct but not mechanically auditable in the acceptance criteria.

3. **Specify the concrete hook-failure mechanism for 23-09 Task 2's negative test.** Pre-creating a tag that conflicts with `VersionBump` is the most deterministic option. Stating it in the plan saves the executor from trial-and-error fixture construction.

4. **Consider a `GATE_SWEEP_MAX_AGE` config key defaulting to something shorter than 7 days.** The reaper's threshold should be independently configurable from the gate poll timeout. A gate can have a 7-day poll timeout but be reaped after 6 hours of abandonment. This decouples "how long to wait for a human" from "how long before the machine says this is abandoned." Currently `23-RESEARCH.md` raises this but no plan introduces the separate config.

5. **Verify the `BranchCleanup` hook's cleanup is idempotent before plan 23-06's merge-internal evidence check ships.** If `merge_feature`'s new evidence check fails after the merge but before `BranchCleanup`, what state is the checkout in? The merge has already been committed to `develop`. The plan should state whether `merge_feature` rolls back on evidence-check failure or leaves the merge in place (the latter being more realistic for a git merge that's already committed). This matters for the recovery path in 23-10.

### 5. Risk Assessment

**Overall: LOW**

The probe evidence forms a solid empirical foundation for the re-aim. All four load-bearing claims (A–D) are verified against source — the planner didn't over-claim. The false-green acceptance criteria concern is absent from these plans. The primary residual risks are (a) flakiness in spawn-and-wait e2e tests under CI resource constraints, and (b) the human-ops dependency in 23-10's remote restore rehearsal, both of which are acknowledged by the plans themselves. The re-aim correctly prioritizes the three things the probe justified (enumeration, reaping, stop) and defers the supervisor migration with an explicit, documented rationale rather than silently dropping it. The one-way door in 23-11 has appropriate preconditions in 23-10 covering both walls the probe hit. This plan set is executable.

---

## Consensus Summary

### Agreed Strengths

- **The re-aim is genuine.** All three lanes independently confirmed the plan set
  is aimed at the measured probe/forensics evidence, not the invalidated
  socket-supervisor hypothesis, and that D-08/D-10 are *stated* deferrals rather
  than silent drops (`23-03-PLAN.md:76-85`).
- **Claims A, B and D are source-backed.** All three lanes verified them
  independently, via different citation paths — Codex through
  `pipeline_outcomes.rs:274`, OpenCode through `:275-286`, Hermes through
  `lock.rs:84-88` vs OpenCode's `:86-87` + `lock::holder` at `:145`. Convergence
  from independent routes is the strongest signal in this review.
- **Mechanism reuse is the best architectural call in the set.** Sweep and
  `devflow stop` both write through the existing `Gates::respond`, which the
  already-polling `advance` consumes natively and tears itself down through its
  own `abort()` path — no new process-signalling machinery. Named as a strength
  by Codex, OpenCode and Hermes independently.
- **The known `cargo test --exact` vacuity trap is closed.** No plan uses
  `--exact`; the package is correctly `devflow`; pass-count guards use
  `[1-9][0-9]*` so `0 passed` cannot match.

### Agreed Concerns

- **The real-child-process e2e tests (23-04 Task 3, 23-05 Task 1) are the
  agreed top risk.** All three lanes flagged them — Codex LOW, Hermes MEDIUM,
  OpenCode HIGH. They spawn a real `devflow advance` and wait on
  `Gates::poll_response`'s 60s backoff cap (`crates/devflow-core/src/gates.rs:219`).
  Design is sound (lock-file synchronisation, per-`Command` env override, no
  fixed sleeps); the unmeasured variable is CI behaviour under CPU/IO pressure.
  OpenCode's fix is the most concrete: a hard `Duration`-bounded wait with an
  explicit `panic!` on timeout, converting a potential infinite CI hang into a
  90s failure with a clear message.
- **23-10's remote-restore rehearsal is a discovery step, not a guarantee**
  (Codex, OpenCode, Hermes). If branch protection refuses a force-push to
  `develop`, the only undo is a revert PR, whose latency may exceed what the
  recovery plan assumes.

### Divergent Views

- **Risk rating: Hermes LOW vs Codex/OpenCode MEDIUM.** The divergence is not
  about the evidence — it is about whether the 23-06 oracle defect and the
  masked verification chains count as blocking. Codex found both; Hermes found
  neither. On re-verification the orchestrator sides with Codex: see below.
- **Claim C is where the lanes actually disagree, and Codex is right.**
  Hermes and OpenCode both marked claim C "verified." They verified the narrow
  statement — that `workflow_finished` emitted *from finalization* is guarded by
  `run_checkout_hooks` returning `all_succeeded`. Codex checked the different and
  decisive question: whether the event name is emitted *only* there. It is not.

### Blocking findings (orchestrator-verified, must fix before executing 23-06+)

**1. [BLOCKER] 23-06's `shipped` predicate is itself a false green.**

`workflow_finished` is emitted in **two** places, not one:

- `crates/devflow-cli/src/pipeline_gate.rs:221` — real Ship finalization, guarded.
- `crates/devflow-cli/src/pipeline_gate.rs:79` — the `--until` clean-stop branch,
  with payload `{"reason": "stopped_at", "stage": …}`, which `return`s **before**
  transition hooks and Ship finalization ever run.

Plan 23-06 asserts the opposite three times — line 28 (*"the one record DevFlow
emits only after `hooks_after_ship` fully succeeds"*), line 97, and line 140
(*"the only site that emits `workflow_finished`"*) — and line 163 instructs the
executor to write that claim into source as *"the load-bearing comment of the
whole plan."*

Consequence: `devflow evidence --require-shipped` would report **shipped** for any
phase halted with `devflow start --until plan`. The oracle built to eliminate
false greens would ship one, in the exact predicate the phase depends on. ROADMAP
already records Phase 21 as `workflow_finished` after one stage.

*Fix (Codex):* emit a distinct terminal-only `workflow_shipped` event from
`finish_workflow_with_gate_timeout`, **or** keep `workflow_finished` and reject
payloads carrying `reason: "stopped_at"`.

**2. [BLOCKER] 23-10's authorization checkpoint precedes the proof it demands.**

Task 1 (`23-10-PLAN.md:106`) is the one-way-door authorization gate. Its
`<resume-signal>` requires the operator to state *"(d) confirmation that Task 2's
recovery-point rehearsal has completed successfully."* Task 2 begins at
`23-10-PLAN.md:155` — after it. The operator is asked to attest to a rehearsal
that cannot have happened. *Fix:* reorder so the recovery ref, rehearsal, binary
rebuild check and content preconditions all complete before final authorization.

**3. [HIGH] Verification chains can pass while the test suite is broken.**

Four plans use:

```
cargo test --workspace 2>&1 | rg -q 'test result: FAILED' && exit 1 || cargo clippy … && cargo fmt --check
```

`23-04-PLAN.md:234`, `23-06-PLAN.md:245`, `23-07-PLAN.md:279`, `23-11-PLAN.md:211`
— including the acceptance run itself.

If `cargo test` fails **without printing that exact string** (a compile error, a
panic/abort, a linker failure), `rg -q` exits non-zero, `&& exit 1` is skipped,
and the `||` branch runs clippy/fmt — so the criterion can exit 0 on a broken
build. Reproduced by the orchestrator: a `cargo test` emitting
`error[E0432]: unresolved import` produces no `test result: FAILED` line, `rg`
does not match, and control falls through to the `||` branch.

*Fix (Codex):* use direct status checks —
`cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`
— teeing output if logs are needed, preserving exit status.

**4. [HIGH] 23-03's registry contradicts itself on concurrency.**

The concurrent acceptance criterion requires only that the roots file contain
*"at least one"* of two concurrent entries (`23-03-PLAN.md:178`) and the threat
model concedes a concurrent registration *"can lose an entry"* (`:300`) — while
the success criteria state *"a running phase cannot be missing from the registry"*
(`:317`). A lost entry makes `gate list --all-roots`, sweep and stop blind to an
active gated run, which is the failure this plan exists to prevent.
*Fix (Codex):* serialize registry writes or use one file per root/phase, and
require **both** registrations to survive.

### Non-blocking, worth adopting

- **Decouple reap age from gate poll timeout** (Hermes). No plan introduces a
  `GATE_SWEEP_MAX_AGE` distinct from the 7-day `gate_timeout_secs`. "How long to
  wait for a human" and "how long before the machine calls this abandoned" are
  different questions; conflating them is arguably what produced 30-hour orphans.
- **Scope `--yes-ship` at the call site, and say so in source** (OpenCode). The
  plan changes `run_gate_with_timeout`'s signature; an executor could "simplify"
  by having that function read `state.yes_ship` directly, leaking auto-approval
  into the retry-gate path. Add a source comment explaining the scoping — the
  plan's pitfall section will not be in front of a future reader.
- **Make the "no signal" guarantee mechanically auditable** (Hermes). 23-04's
  truth that the sweep never signals a process currently rests on call-chain
  reasoning; a source-grep assertion over the sweep path would make it checkable.
- **Split 23-11's two outcomes** (Codex). "Acceptance passed" must require
  shipped evidence; "run record valid" may pass for an incomplete run that
  accurately records stop cause and cleanup state. They are currently conflated.
- **Specify 23-09 Task 2's hook-failure fixture** (Hermes) — pre-creating a
  conflicting tag to fail `VersionBump` is the deterministic option.

### Recommended next step

`/gsd-plan-phase 23 --reviews` — replan incorporating this file. Findings 1–4
are the required set; 1 and 2 are blocking for 23-06 and 23-10 respectively.
Plans 23-03 (modulo finding 4), 23-05, 23-07, 23-08 and 23-09 are unaffected by
the blockers and could execute as-is if the phase were paced by wave.
