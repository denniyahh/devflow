---
phase: 21
reviewers: [codex, opencode, hermes]
hermes_model: deepseek-v4-pro
dropped_reviewers: [cursor, antigravity]
reviewed_at: "2026-07-23"
plans_reviewed: [21-01-PLAN.md, 21-02-PLAN.md, 21-03-PLAN.md, 21-04-PLAN.md]
---

# Cross-AI Plan Review — Phase 21 (Operator Legibility & Observability)

> Two independent source-grounded reviews completed (Codex / OpenAI, OpenCode / Copilot). Two lanes dropped on environmental limits, **not** plan problems: **cursor** hit its usage limit; **antigravity** (agy) was auto-denied file-read permission in headless mode even with `--dangerously-skip-permissions` (the known agy-vs-agycli quirk). Both dropped lanes are recorded below as no-review, not counted in consensus.

---

## Codex Review

**Overall**
I verified the plan claims against the local repo source. The phase structure is mostly sound: sequencing is justified, no new dependencies are needed, and the plans generally reuse existing helpers instead of inventing new subsystems. Main issues are 21-02’s misleading “in-stage” progress design and 21-03’s too-narrow document-staleness detector.

**21-01-PLAN.md**
Summary: Strong plan. The source supports the intended narrow change.

Strengths:
- Correctly targets only the strict-ancestor arm: `embedded_commit_is_stale` currently maps `Some(head) != embedded_commit` to unconditional `Stale` at [staleness.rs:54](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/staleness.rs:54)-[56](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/staleness.rs:56).
- Reuses the right predicate: `affects_compiled_binary` already captures `.rs`, `Cargo.toml`, `Cargo.lock`, `build.rs`, and `rust-toolchain.toml` at [staleness.rs:146](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/staleness.rs:146)-[157](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/staleness.rs:157).
- The two fixture-fix claims are real: `wr01_...` uses `b.txt` at [staleness.rs:778](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/staleness.rs:778)-[782](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/staleness.rs:782), and `init_repo_with_diverged_commit` uses `trunk2.txt` at [staleness.rs:692](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/staleness.rs:692)-[695](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/staleness.rs:695).
- Block-message wording fix is warranted: the current message says “embedded commit is not an ancestor” even though the edited arm is the ancestor-but-behind case, [staleness.rs:299](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/staleness.rs:299)-[302](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/staleness.rs:302).

Concerns:
- LOW: The plan’s “byte-identical” acceptance criterion is brittle if comments or local formatting shift. The real invariant should be behavioral: preserve the `Ok(Some(1))` reverse probe at [staleness.rs:63](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/staleness.rs:63)-[73](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/staleness.rs:73) and indeterminate fallback at [staleness.rs:74](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/staleness.rs:74).

Suggestions:
- Keep the helper fail-closed to `Stale` on `git diff` failure.
- Add one assertion that `git diff --name-only` output containing a nested Rust file, e.g. `crates/devflow-cli/src/main.rs`, routes through `affects_compiled_binary`.

Risk Assessment: LOW.

**21-02-PLAN.md**
Summary: Good operator-UX coverage, but the “in-stage progress” mechanism is inaccurate as written.

Strengths:
- `gate show` is well scoped. Full gate context already exists in `OpenGate.context`, populated from `GateFile.context` at [gates.rs:163](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/gates.rs:163)-[168](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/gates.rs:168); only `gate_list` truncates it via `render_gate_context(..., 100)` at [commands.rs:654](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:654)-[663](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:663).
- Stage auto-resolution can mirror existing `gate_respond` behavior at [commands.rs:686](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:686)-[710](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:710).
- Rate-limit reset surfacing is correctly presentation-only: `CronInstructions.retry_after` exists at [ship.rs:12](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/ship.rs:12)-[20](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/ship.rs:20), while `cron_instruction_hints` currently ignores it at [commands.rs:893](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:893)-[903](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:903).
- `status` currently only suggests `resume` for stuck phases, [commands.rs:592](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:592)-[598](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:598), so recovery-hint extraction is useful.

Concerns:
- MEDIUM: “elapsed-in-stage” is not supported by the current data model. `State` only has `started_at`, documented as “When the phase started,” [state.rs:56](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/state.rs:56)-[57](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/state.rs:57), initialized once in `State::new` at [state.rs:135](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/state.rs:135)-[146](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/state.rs:146), and not reset in `transition`, [pipeline_gate.rs:94](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_gate.rs:94)-[100](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_gate.rs:100). Printing it as stage elapsed time would make status less trustworthy.
- MEDIUM: Suggesting `advance` when `gate_pending` may be misleading unless there is already a consumed response/ack state. Gate resolution happens through `Gates::poll_response`, clears `gate_pending`, and derives `GateAction` at [pipeline_gate.rs:243](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_gate.rs:243)-[289](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/pipeline_gate.rs:289); unresolved gates are answered with `devflow gate approve/reject`, not `advance`.

Suggestions:
- Rename the new line to “phase age” if it uses `started_at`, or derive real stage age from the latest `stage_launched` event instead.
- Make recovery hints state-specific: `resume` for dead monitor/stuck; `gate approve/reject` for pending gates; `advance` only where the source path proves it is the right verb.

Risk Assessment: MEDIUM.

**21-03-PLAN.md**
Summary: The doctor integration shape is good, but the planned detector may miss the very narrative-drift class it is meant to catch.

Strengths:
- Correct integration point: `doctor` gathers checks and facts once at [commands.rs:1121](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:1121)-[1256](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:1256), then emits either text or JSON at [commands.rs:1258](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:1258)-[1290](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:1290).
- Third JSON key is the right shape. `doctor_json_body` is currently a single object with `environment` and `reconciliation`, [commands.rs:1858](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:1858)-[1870](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:1870).
- New sibling finding type is justified because `collect_phase_facts` only reads active state files, [commands.rs:1695](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:1695)-[1708](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:1708), while shipped phases are historical document rows.
- Noise risk is real: ROADMAP has ranges/em-dashes and duplicate/legacy versions at [ROADMAP.md:23](/var/home/denniyahh/Github/devflow/.planning/ROADMAP.md:23)-[31](/var/home/denniyahh/Github/devflow/.planning/ROADMAP.md:31); actual tags start at `v1.0.1`, not `v1.0.0`.

Concerns:
- HIGH: The planned parser only checks ROADMAP `## Shipped` and STATE `## Completed`, so it can miss cross-document narrative drift. Current ROADMAP milestone rows say Phase 19 is complete at `1.6.0`, [ROADMAP.md:15](/var/home/denniyahh/Github/devflow/.planning/ROADMAP.md:15)-[17](/var/home/denniyahh/Github/devflow/.planning/ROADMAP.md:17), but ROADMAP `## Shipped` skips Phase 19, [ROADMAP.md:23](/var/home/denniyahh/Github/devflow/.planning/ROADMAP.md:23)-[24](/var/home/denniyahh/Github/devflow/.planning/ROADMAP.md:24). STATE includes Phase 19 completed at `v1.6.0`, [STATE.md:466](/var/home/denniyahh/Github/devflow/.planning/STATE.md:466)-[468](/var/home/denniyahh/Github/devflow/.planning/STATE.md:468). A tag-existence check over only present rows would report “consistent” while this omission remains.
- MEDIUM: The v1.5.0 cutoff needs numeric semver comparison, not string comparison. A future `v1.10.0` claim would compare incorrectly if implemented lexicographically.
- LOW: Missing `.planning/ROADMAP.md` or `.planning/STATE.md` yielding “no rows, consistent” would hide a document-surface failure. The source currently reads state/gates defensively, but doctor should distinguish “missing docs” from “docs consistent.”

Suggestions:
- Extend scope to compare phase/version claims across ROADMAP milestone table, ROADMAP shipped table, and STATE completed table for post-v1.5.0 phases.
- Parse semver into `(major, minor, patch)` tuples for cutoff and ordering.
- Emit a Warn for missing planning docs, not an empty success.

Risk Assessment: MEDIUM-HIGH.

**21-04-PLAN.md**
Summary: Good narrow design and correctly avoids `State`, but lifecycle and pid-race details need tightening.

Strengths:
- The no-`save_state` constraint is source-backed: `parallel.rs` documents synthetic, never-persisted state at [parallel.rs:4](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:4)-[7](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:7), and `rg` shows no live `save_state` call in that file.
- The two-agent overwrite problem is real: `run_agent_blocking` is called for agent A and B at [parallel.rs:336](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:336)-[338](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:338) and [parallel.rs:374](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:374)-[376](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:376), while pid/stdout/exit paths are keyed only by phase at [agent_result.rs:876](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:876)-[895](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:895).
- Status-only surfacing is coherent with current `status` implementation, which enumerates persisted states first and then ancillary sections, [commands.rs:528](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:528)-[621](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:621).

Concerns:
- MEDIUM: Slot record lifecycle is underspecified on early returns. `sequentagent` returns early on agent A zero-commit rate limit at [parallel.rs:346](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:346)-[355](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:355), errors on agent A failure at [parallel.rs:338](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:338)-[345](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:345), and errors on agent B failure/rate-limit at [parallel.rs:376](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:376)-[390](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:390). A single clear call after successful integration at [parallel.rs:392](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:392)-[400](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:400) will leave stale records by design. That may be useful, but the plan should say which terminal states preserve vs clear.
- MEDIUM: The status line relies on the existing agent pid file, but the monitor writes that file asynchronously inside its shell script, [monitor.rs:148](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/monitor.rs:148)-[154](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/monitor.rs:154). If the slot is written immediately after `spawn_monitor_no_advance`, [parallel.rs:199](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:199)-[201](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/parallel.rs:201), status can briefly show “not running” for a just-launched agent. There is already a `wait_for_agent_pid` helper at [monitor.rs:181](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/monitor.rs:181)-[190](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/monitor.rs:190) that the plan should consider.
- LOW: Adding write helpers in `agent_result.rs` should use `workflow::ensure_devflow_dir`; `agent_result`’s private `devflow_dir` only constructs a path, [agent_result.rs:871](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:871)-[895](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/agent_result.rs:895), while `.gitignore` hygiene lives in `workflow::ensure_devflow_dir`, [workflow.rs:103](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/workflow.rs:103)-[121](/var/home/denniyahh/Github/devflow/crates/devflow-core/src/workflow.rs:121).

Suggestions:
- Make lifecycle explicit: clear only on successful full completion, preserve failed/paused records as forensic status, or install a guard that clears on every terminal path.
- Use `monitor::wait_for_agent_pid` before claiming “running”, or render a third state like `starting` when slot exists but pid file has not appeared yet.
- Add tests for early-return cases, not just happy-path write/read/clear.

Risk Assessment: MEDIUM.
---

## OpenCode Review

I'll perform a thorough review by first reading the key source files the plans claim to extend, then cross-referencing every substantive claim.
Let me continue reading the remaining portions of the key files.
Now I have all the evidence needed. Here is the full cross-AI plan review:

---

# Phase 21 Cross-AI Plan Review

## Verification Method

Every finding below was traced against the actual source files at HEAD (`7163347`). No claim is accepted on the plan's word alone. Evidence format: `file:line`.

---

## 21-01-PLAN.md: Dogfood Staleness Guard Content-Awareness (21d)

### Summary
A tightly-scoped, two-task plan that narrows `embedded_commit_is_stale`'s strict-ancestor arm from "any forward HEAD movement = Stale" to "only build-affecting forward movement = Stale." Tasks: (1) the narrowing + new docs-only Fresh test, (2) fixture repairs + mixed-range test + block-message reword.

### Strengths

1. **Exact line-level precision.** The plan correctly identifies the single line to change at `staleness.rs:56` (`Some(_) => Staleness::Stale`). This is the `Ok(Some(0))` arm's non-HEAD-match case. The exit-1 reverse-probe arm (`lines 63-72`) and `Indeterminate` fallbacks are correctly identified as out of scope.

2. **Test fixture conflicts correctly diagnosed.** Both fixture conflicts the plan flags are real:
   - `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` (`staleness.rs:780`): the second commit writes `b.txt`, confirmed at `line 780`. Under content-aware semantics, this non-build file would class as `Fresh`, silently flipping the test.
   - `init_repo_with_diverged_commit` (`staleness.rs:693`): the final `trunk` commit writes `trunk2.txt`, confirmed at `line 693`. This means `embedded_commit_is_stale_maps_ancestry_exit_codes`'s `base -> Stale` assertion (`line 721`) also flips.

3. **`affects_compiled_binary` reuse mandate is well-scoped.** The predicate at `staleness.rs:146-157` matches `.rs`, `Cargo.toml`, `Cargo.lock`, `build.rs`, `rust-toolchain.toml` — exactly the right set. The plan's prohibition on forking this is correct.

4. **Worktree fixture correctly left alone.** The plan notes that `worktree_staleness_fixture` (`staleness.rs:464`) already commits `src/lib.rs` (build-affecting), so tests `embedded_commit_is_stale_uses_worktree_head` (`line 547`) and `enforce_build_staleness_blocks_self_dogfood_behind_worktree_head` (`line 578`) remain green — verified correct at `lines 513, 517`.

5. **Block message reword is in scope.** The current message at `staleness.rs:298-301` reads "embedded commit is not an ancestor of ... current HEAD." After the fix, an ancestor-but-behind case returns `Fresh` (docs-only) or `Stale` (source present) — but neither is "not an ancestor." The message reword is necessary.

### Concerns

**HIGH** — None.

**MEDIUM** — None.

**LOW**

1. **Git-error fail-toward-Stale semantics need explicit test.** The plan says `ancestry_range_affects_build` "fails toward true (Stale) on any git error." The plan's `<behavior>` block mentions this but neither task includes a test for it. At `staleness.rs:45-76`, the existing tests (`embedded_commit_is_stale_maps_ancestry_exit_codes` at `line 708`) test the bogus-SHA path, but a git-error-inducing scenario (e.g., a missing git binary or a trashed repo) in the ancestry arm is not tested. The `unwrap_or(true)` default is the right semantics, but it should be explicitly proven.

2. **Task 1 verify command has an awk exit-code trap.** `cargo test --workspace staleness::tests::docs_only_range_is_fresh -- --exact --nocapture 2>&1 | rg "test result: ok. 1 passed"` — the `rg` pipe absorbs `cargo test`'s exit code. On this repo, `cargo test --workspace` exits 0 even when no tests match (a bare name filter), so the `rg` assertion that 1 test passed is the right safeguard. However, the verify for Task 2 uses `cargo test --workspace staleness:: 2>&1 | rg "test result: ok"` with no exact pass count — a partial match (e.g., 1 passed, 0 failed) satisfies `rg` but may not represent the full module. Recommend the full-module verify assert on `"test result: ok. N passed; 0 failed"`.

### Suggestions

1. Add a git-failure-smoke test in the `docs_only_range_is_fresh` fixture style: init a repo, write a non-build commit, delete `.git` to force git failure, assert `Stale` (not `Fresh`). This is a 20-line test that closes the one stated but untested behavior.

2. In the Task 2 verify, pipe `rg "0 failed"` alongside `rg "test result: ok"` for the full-module run.

### Risk Assessment: **LOW**

The plan touches exactly one line in a well-tested function, reuses an existing, proven predicate, and the two fixture conflicts were correctly identified against live source. The worst-case failure mode (a git error returning Fresh) is mitigated by the `unwrap_or(true)` default.

---

## 21-02-PLAN.md: Operator Discoverability (21a)

### Summary
Three additive-UI tasks: (1) `devflow gate show <phase>` prints untruncated gate context, (2) `cron_instruction_hints` surfaces `retry_after`, (3) in-stage progress + recovery-verb hints in `status`. All are read-only, no pipeline behavior change.

### Strengths

1. **`gate show` resolution correctly mirrors `gate_respond`.** The plan reuses `gate_respond`'s auto-resolve pattern at `commands.rs:686-710`. The source confirms: filter by phase, `[]` = error, `[one]` = that stage, `many` = ask for `--stage`. This is the right approach.

2. **`CronInstructions.retry_after` correctly identified as the data source.** Verified at `ship.rs:20` (`pub retry_after: String`) and `commands.rs:893-904` (`cron_instruction_hints` currently ignores `retry_after`). The plan correctly notes the field is already populated. No new detection logic is needed — this is a pure presentation change.

3. **Recovery-hints helper design is solid.** The plan creates a pure `fn recovery_hints(state, liveness) -> Vec<String>` that returns `resume` for `Stuck` and adds `advance` when gate-pending. This mirrors the existing `render_pending_gate_banner` pure-String-then-`println!` idiom at `commands.rs:627-651`.

4. **Flag surface decision is explicit.** The plan chooses positional `phase` + `--stage` matching `gate approve`/`gate reject` (`main.rs:283-303`), with the rationale stated rather than assumed. This resolves RESEARCH Open Q3 cleanly.

5. **`main.rs` churn is minimal.** Adding a single `GateCmd::Show` variant to the enum at `main.rs:274-328` and one dispatch arm to the match at `lines 417-443` — the plan scopes this correctly.

### Concerns

**HIGH** — None.

**MEDIUM** — None.

**LOW**

1. **`gate show` must NOT transitively use `render_gate_context(.., 100)` via any code path.** The plan says to call `render_gate_context(&gate.context, usize::MAX)` directly in the new function. But also says to reuse `gate_respond`'s resolution block. `gate_respond` (`commands.rs:679`) does not call `render_gate_context` at all — it's `gate_list` that does (`line 662`). So the risk of accidentally calling the 100-char version is negligible, but the executor should grep-verify.

2. **`gate show` verify at Task 1 AC says `rg -n "render_gate_context(&gate.context, 100)" crates/devflow-cli/src/commands.rs` still only appears in `gate_list`.** This is correct, but the AC should also assert the new call site uses `usize::MAX`.

3. **Elapsed-in-stage progress uses `state.started_at`.** The plan proposes `println!("  in stage {}: {}", state.stage, recover::format_age(&state.started_at))`. But `started_at` is the workflow start time, not the current stage's entry time. A phase that has been running for 30 minutes with a 2-minute-old stage transition would display "in stage code: 30m ago." The RESEARCH.md notes this tradeoff explicitly ("no fixed stage-count denominator exists to render N of M reliably"), and the plan's phrasing "elapsed time in the current stage" is a slight overstatement — it's elapsed time since the workflow started. This is a display-nuance concern, not a correctness one.

### Suggestions

1. For the cron hint, the plan says to use `render_gate_context(&instructions.retry_after, 100)` for sanitization. Since `CronInstructions.retry_after` comes from `retry_after_from_reason` (`parallel.rs:404-408`), which strips a known prefix from rate-limit agent output, the value can contain arbitrary agent text. The sanitization is correct. Consider a test asserting that a `retry_after` containing escape sequences is rendered without them.

### Risk Assessment: **LOW**

Three additive, read-only presentation changes. No pipeline logic modified. Every surface is unit-testable via the pure-helper pattern the codebase already follows. The only material risk is the `started_at` vs stage-entry-time imprecision for progress display, which is cosmetic.

---

## 21-03-PLAN.md: Doctor Planning-Doc Staleness Reconciliation (21b)

### Summary
Two tasks: (1) detection core — parse version rows from `ROADMAP.md`/`STATE.md`, do `v`-prefix-normalized git-tag lookups, produce `PlanningDocFinding` items with a v1.5.0 severity cutoff; (2) wire into `doctor()` text + `doctor_json_body()` as a third top-level key `"planning_doc_staleness"`.

### Strengths

1. **Pitfall #2 (legacy-row noise) is correctly identified and mitigated.** The plan's scope — `^v?\d+\.\d+\.\d+$` regex on version cells, pre-v1.5.0 downgrade to Warn — directly addresses the alert-fatigue failure mode. This is the single most important design decision in 21b, and the plan makes it explicit rather than hand-waving it.

2. **Sibling finding type is the correct design.** The plan creates `PlanningDocFinding` (not `PhaseFinding`) for the new third key in `doctor_json_body`. This is correct because shipped phases have no active `PhaseFacts` — verified by the fact that `collect_phase_facts` (`commands.rs:1695-1708`) derives exclusively from `workflow::list_states()`.

3. **`doctor_json_body` extension is correctly scoped.** Verified at `commands.rs:1866-1871`: the function composes `{"environment": ..., "reconciliation": ...}`. Adding a third key `"planning_doc_staleness"` is the right approach — no fork, no second top-level array, preserving the WR-01 fix.

4. **No-write-path guarantee is enforced by acceptance criteria.** The plan mandates `rg` asserting no `fs::write`/`File::create`/`OpenOptions` in the new functions.

5. **Argv-array git shelling is mandated for `tag_exists_and_reachable`.** Mirroring `run_git_stdout`'s idiom at `staleness.rs:82-91` — correct.

### Concerns

**MEDIUM**

1. **`tag_exists_and_reachable` reachability check relative to `"main"` is fragile for Phase 21.** The plan says `tag_exists_and_reachable(project_root, tag, "main")`. On this repo, `main` may not exist locally (it's an origin ref). The `doctor` command at `commands.rs:1121` is designed to work without a network (`release_check`'s `check_divergence` explicitly avoids `git fetch` at `lines 1406-1430`). If `origin/main` is not fetched, `git merge-base --is-ancestor <tag> main` will fail. The plan should either (a) use `origin/main` (and accept the network dependency), (b) use a local branch like `develop`, or (c) make reachability best-effort with graceful degradation. This is not addressed.

2. **The plan does not specify behavior when `ROADMAP.md`/`STATE.md` files don't exist.** In `doctor` (`commands.rs:1258`), `collect_phase_facts` can return empty if there are no active states — but the plan's Task 2 says to "read `.planning/ROADMAP.md` and `.planning/STATE.md` (best-effort; a missing file yields no rows, never an error)." This is the correct approach, but it should be explicitly tested (a missing-`ROADMAP.md` fixture asserting `planning_doc_staleness` key is `[]` / produces the "consistent with git tags" text line).

**LOW**

3. **The `v`-prefix normalization logic is stated but edge cases exist.** The plan says "a bare cell `1.7.0` is looked up as `v1.7.0`." But what about cells that already carry a `v` prefix (e.g., if someone writes `v1.7.0` in the table)? The regex `^v?\d+\.\d+\.\d+$` matches both — good. But the normalization should be: if matches as bare (no leading `v`), prepend `v`; if already prefixed, use as-is. The plan's wording is ambiguous on whether a second `v` could be prepended. Low risk since the regex won't double-match.

4. **`parse_planning_doc_versions` must handle the dual-table structure of `.planning/ROADMAP.md`.** The command says to scan "## Shipped" and "## Completed" sections, but the exact row shapes differ between the two tables. The plan doesn't specify how to identify table boundaries. The `is_self_dogfood_workspace` precedent at `staleness.rs:198-236` demonstrates the project's preferred approach (hand-scanning anchored on known markers). The plan should mention that tables end at a blank line or the next `## ` heading.

### Suggestions

1. Make `tag_exists_and_reachable` use `develop` instead of `main` (all tags that exist on this repo are reachable from `develop`), or accept a branch parameter with `develop` as the default.

2. Add a missing-file test for the "consistent with git tags" path.

3. Explicitly state that `parse_planning_doc_versions` scans for `## Shipped`/`## Completed` headings, reads rows until a blank line or next heading, and splits on `|`.

### Risk Assessment: **MEDIUM**

The `main`-vs-local ref question is the primary concern. If `origin/main` isn't fetched, every `doctor` run could produce spurious "tag unreachable" findings. The detection-only, no-write-path design is otherwise solid.

---

## 21-04-PLAN.md: sequentagent Second-Process Tracking (21c)

### Summary
Two tasks: (1) add a `SequentagentSlot` record type + write/read/clear helpers in `agent_result.rs`, wire into `run_agent_blocking` via a new `slot: &str` parameter; (2) surface live slots in `status` via `render_sequentagent_status`.

### Strengths

1. **Re-scope finding is stated explicitly rather than assumed.** The plan acknowledges that both agent A and B are currently invisible (because sequentagent never persists a `State`), then makes the deliberate choice to narrow per D-06 — while noting that the slot record labels whichever agent is currently running anyway.

2. **Avoiding `State`/`save_state` is correctly enforced.** Verified at `parallel.rs:4-7`: the doc comment explicitly states "`run_agent_blocking` operates on synthetic, never-persisted state — `sequentagent` does not participate in the stage machine, so there is no `save_state` chokepoint here." The plan's AC (`rg -n "save_state" crates/devflow-cli/src/parallel.rs | rg -v '//'`) correctly asserts that no new `save_state` call is introduced.

3. **Record file naming follows existing conventions.** `agent_pid_path` at `agent_result.rs:893-895` uses `devflow_dir(project_root).join(format!("phase-{:02}-agent-pid", phase))`. The plan's `sequentagent_slot_path` → `phase-{:02}-sequentagent` matches this pattern.

4. **`run_agent_blocking` signature change is minimal.** Current signature at `parallel.rs:155-160` takes `(project_root, phase, agent, workdir)`. Adding `slot: &str` is backward-compatible for the two call sites at lines 338 and 376, both within `sequentagent`.

5. **Write point is correctly placed.** The plan says to write the slot record "immediately after `spawn_monitor_no_advance` returns `monitor_pid`." Verified at `parallel.rs:199-200` — the monitor spawn line is right there. This means the record is on disk before the agent starts running.

6. **Clear point is correctly placed.** The plan says to call `clear_sequentagent_slot` "next to the final `delete_cron_instructions`." Verified at `parallel.rs:396-397` — the delete call exists at the end of `sequentagent`, so the clear goes right alongside it.

7. **WR-02 discipline is enforced.** The plan mandates path-free record content (slot letter + agent kind only) and tests that the raw file contains no temp dir path or home path.

### Concerns

**MEDIUM**

1. **`doctor` surfacing deliberately excluded — but the rationale is only partially correct.** The plan says "adding a text-only line without a matching `--json` key would reintroduce the human/json inconsistency 18-WR01 fixed." This is correct — `doctor_json_body` at `commands.rs:1866-1871` composes `{"environment", "reconciliation"}`, and adding a text-only section would be inconsistent. However, the plan could add a fourth JSON key (e.g., `"sequentagent_status"`) for parity. The exclusion is a deliberate scope decision, but it means `doctor` (the diagnostic command) knows less about a running sequentagent than `status` does, which is a mild legibility asymmetry.

2. **`render_sequentagent_status` uses a `read_dir` scan to enumerate records.** The plan says to mirror `default_logs_phase`'s `read_dir` scan. Looking at `commands.rs:861-884`, `default_logs_phase` scans for `phase-*-stdout` files. The plan's description to "enumerate `.devflow/phase-*-sequentagent` records" implies a similar directory scan. For a single-at-a-time sequentagent phase, this is fine — but the plan should note that the number of files being scanned is O(1) in practice.

3. **The `slot` parameter typed as `&str` not an enum.** The plan uses `slot: &str` taking `"A"` / `"B"` literals rather than a `Slot` enum (`enum Slot { A, B }`). This is expedient but introduces a potential for typos at the two call sites. Low risk given there are exactly two call sites, both in `sequentagent`.

**LOW**

4. **`archive_phase_files` interaction.** The plan notes that `archive_phase_files` (`agent_result.rs:937`) is called at the top of each `run_agent_blocking` invocation (`parallel.rs:161`). The slot record is a sibling to the archived files but is NOT archived — it's overwritten at each agent launch. This is correct because the slot record describes "who is running NOW," not "who ran historically." The plan correctly keeps the record out of the archive sweep.

5. **Clearing on sequentagent completion only handles the success path.** Looking at `sequentagent` (`parallel.rs:278-402`): if agent A fails (line 340-361, `return Err(...)`), the function returns without reaching the clear call at line 396. The same for agent B failure (line 387). The plan should clear on ALL exit paths (including error returns), or the `status` display could show a stale "agent B running" for a phase that already errored out.

### Suggestions

1. Move `clear_sequentagent_slot` to a cleanup block that runs on both success and error paths. A `let _cleanup = ... Drop` guard or a `finally`-style block before the early returns would address this.

2. Consider whether `doctor --json` should also carry a `"sequentagent_status"` key for symmetry. If not, document the decision explicitly in a comment next to `render_sequentagent_status`.

3. Type `slot` as an enum rather than `&str` — two variants, no room for typo.

### Risk Assessment: **MEDIUM**

The stale-record-on-error-return concern (5) is the primary risk. A sequentagent run that fails agent B would leave a slot record on disk saying "agent B (codex) running," and a subsequent `devflow status` would report it as live until something else overwrites or deletes the file. The plan addresses the happy path but not the error-exit paths.

---

## Cross-Cutting Observations

### Wave Sequencing

The wave ordering (Wave 1: 21d → Wave 2: 21a → Wave 3: 21b → Wave 4: 21c) is correctly justified:

- **21d leads** because it unblocks this phase's own dogfood stages (D-07). Verified correct — the `enforce_build_staleness` guard at `staleness.rs:287` runs on every `launch_stage` call, and Phase 21's own Plan/Code/Validate stages will trigger it.
- **21a/21b/21c serialize** because all three edit `commands.rs`, and the same-wave zero-file-overlap rule forbids parallelizing them. Verified — 21-02 adds `gate_show`, 21-03 extends `doctor`/`doctor_json_body`, and 21-04 adds `render_sequentagent_status` to `status`. All touch `commands.rs`.

### Scope Adherence

All four plans respect the phase boundary constraints:
- No release-executor work (999.25 → own phase)
- No `--base` work (999.28 → Phase 22)
- All units are reversible/detection-only
- No new crate dependencies

### Overall Risk Assessment: **LOW**

The four plans collectively cover the phase's committed units (21a-21d) with well-scoped, source-verified changes. The concerns raised are all addressable at plan-execution time:

| Plan | Risk | Primary Concern |
|------|------|-----------------|
| 21-01 | LOW | Git-error path untested (suggestion, not blocker) |
| 21-02 | LOW | `started_at` vs stage-entry-time for progress (cosmetic) |
| 21-03 | MEDIUM | `main` vs local ref for reachability check |
| 21-04 | MEDIUM | Slot record not cleared on error-exit paths |

The plans achieve the phase goal: every unit is operator-facing, reversible or detection-only, and testable without irreversible side effects. The 21d→21a→21b→21c sequencing is load-bearing and correctly reasoned from the source code.

---

## Cursor Review

**DROPPED — not a review.** `cursor-agent` exited with `ActionRequiredError: You've hit your usage limit`. No plan feedback produced; excluded from consensus.

---

## Antigravity Review

**DROPPED — not a review.** `agy` headless mode auto-denied the `command` permission required to read repo files ("jetski: no output produced"); retry with `--dangerously-skip-permissions` still produced no output. No plan feedback produced; excluded from consensus.

---

## Hermes Review (deepseek-v4-pro)

> Third independent lane, run via the Hermes review adapter (`hermes chat -Q -q`). Backed by **deepseek-v4-pro** — a distinct model family from Codex (OpenAI) and OpenCode (Copilot), so a genuinely independent third voice, not a same-family echo.

deepseek.
Now I have all the source-verified evidence. Here's the review:

---

# Cross-AI Plan Review: Phase 21 — Operator Legibility & Observability

**Reviewer:** Hermes (deepseek-v4-pro)
**Date:** 2026-07-23
**Source verification:** Every claim below traced against actual source at `/var/home/denniyahh/Github/devflow` HEAD.

---

## Overall Assessment

**Verdict: APPROVE with one MEDIUM concern on 21-02 and a LOW-MEDIUM concern on 21-04.**

The four plans form a coherent, well-scoped phase. Wave ordering (21d→21a→21b→21c) is sound and load-bearing: 21d's dogfood guard must land first or this phase's own Plan/Code/Validate stages will hard-block after every `.planning/` commit. The zero-file-overlap constraint on `commands.rs` (21-02, 21-03, 21-04) is correctly enforced via wave serialization. Every plan stays within D-02's reversible/detection-only boundary. The RESEARCH.md's three critical pitfalls are all correctly addressed in the respective plans.

**Two issues need attention before execution:**
1. 21-02's in-stage progress line will mislead — `State.started_at` is phase-level, not stage-level.
2. 21-04's slot record is not cleared on five error-exit paths in `sequentagent`.

---

## 21-01-PLAN.md — Content-aware staleness guard (21d)

### Summary
Narrows `embedded_commit_is_stale`'s strict-ancestor arm to be content-aware: runs `git diff --name-only <embedded> HEAD`, filters through the existing `affects_compiled_binary`, returns `Fresh` for docs-only ranges, `Stale` only when a build input changed. Two tasks: (1) core narrowing + `docs_only_range_is_fresh` test, (2) repair two now-broken fixtures + `mixed_range_docs_and_source_is_stale` test + block-message reword.

### Strengths
- **Precise surgical scope.** The plan targets the EXACT single line to change: `staleness.rs:56` (`Some(_) => Staleness::Stale`) inside the `Ok(Some(0))` arm. I verified the surrounding code — the `Ok(Some(1))` reverse-probe arm (lines 63-72) and `Indeterminate` fallbacks are in separate match branches and remain byte-identical.
- **Both regression fixtures correctly identified.** `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` at line 780 writes `b.txt` (non-build file → flips to `Fresh`). `init_repo_with_diverged_commit` at line 693 writes `trunk2.txt` (non-build file → the `base → Stale` assertion at line 721 flips). Both are named and their exact repair (retarget to `.rs` file) is specified.
- **`affects_compiled_binary` reuse** is explicit and correct. I verified the predicate at `staleness.rs:146` matches `.rs`, `Cargo.toml`, `Cargo.lock`, `build.rs`, `rust-toolchain.toml` — `b.txt` and `trunk2.txt` are correctly excluded.
- **Fail-toward-Stale semantics** on git error: the `ancestry_range_affects_build` helper returns `true` on any git failure, so a transient error never yields a false `Fresh`. Correct safety posture.

### Concerns
- **None.** The plan is the most precise and well-verified of the four. RESEARCH Pitfall #1's warning about the WR-01 fixture flip is addressed down to the exact line numbers.

### Suggestions
- The `enforce_build_staleness` block-message reword is described but no specific text is proposed. Given the plan says "describe instead that a build-relevant file changed since the build (or the commit is not an ancestor at all)", the implementer should ensure both branches of the message (ancestor-with-build-change vs non-ancestor) are distinct. The current message asserts "is not an ancestor" unconditionally — the new message needs a conditional or two branches.

### Risk: LOW
The change is a one-line logic gate + a helper function + fixture edits. Every affected line is named. Fallback safety (fail toward `Stale`) preserves the existing hard-block guarantee.

---

## 21-02-PLAN.md — Operator discoverability (21a)

### Summary
Four additive UX gaps: `gate show <phase>`, rate-limit reset time in `status`, in-stage progress line, recovery-verb hints from stuck state. Three tasks over `main.rs` and `commands.rs`.

### Strengths
- **Flag surface decision is correct.** Positional `phase` + `--stage` mirrors `GateCmd::Approve`'s existing convention (line 285: `phase: u32`). The plan simplifies by omitting `Approve`'s legacy `stage`/`legacy_project` positional ambiguity. Clean.
- **`gate_respond` auto-resolve pattern reuse** (lines 686-710) is verified as the correct precedent — it already handles `[]`/`[one]`/`many` gate resolution.
- **`render_gate_context` sanitization** (line 305 in `pipeline_outcomes.rs`) handles control characters — using `usize::MAX` gives untruncated-but-safe output. Correct.
- **Zero new rate-limit detection.** `CronInstructions.retry_after` (ship.rs:20) is already populated. `cron_instruction_hints` at commands.rs:893 ignores it — the plan correctly identifies this as a presentation-only fix.
- **`recovery_hints` extraction** into a pure, testable function: `gate_pending` → `advance` hint; `Stuck` → always `resume` hint; `Healthy` → empty. Reasonable design.

### Concerns

- **MEDIUM: In-stage progress line is misleading.** The plan says "elapsed time in the current stage" and proposes reusing `format_age(&state.started_at)`. But `State.started_at` (line 57, documented as "When the phase started (Unix seconds)") is **phase-level**, not stage-level. There is no `stage_started_at` field. The displayed time will be "elapsed since phase start", not "elapsed in current stage" — potentially hours off for a long-running phase. Adding a per-stage timestamp would require a new `#[serde(default)]` field in `State` plus a write in `transition()`. The plan does not acknowledge this gap.
  - **Recommendation:** Either (a) add a `stage_started_at: Option<String>` field to `State` (set in `transition()`, default `None` for backward compat) and use that, or (b) label the line honestly: "elapsed: {age}" rather than "in stage {stage}: {age}". Option (b) is simpler and still improves legibility.

### Suggestions
- `recovery_hints` only checks `state.gate_pending` for the `advance` hint. A stuck phase at Preflight/Plan has no gate pending — `advance` is still the correct verb but won't be shown. This is defensible (the gate-pending case is the main footgun), but flagging it for operator awareness.

### Risk: MEDIUM
The stage-progress issue could erode trust in `status` output if operators notice the counter doesn't reset on stage transitions. Fixable by relabeling the line or adding a minimal state field.

---

## 21-03-PLAN.md — Doctor planning-doc staleness (21b)

### Summary
Adds a detection-only `doctor` check comparing `ROADMAP.md`/`STATE.md` version claims against git tag existence + reachability from `main`. Two tasks: (1) detection core (parse, tag lookup, finding production), (2) wiring into `doctor()` text + `doctor_json_body()` third key.

### Strengths
- **`doctor_json_body` extension** is correct: third top-level key `planning_doc_staleness` in the existing single-object JSON (line 1866: `{\"environment\", \"reconciliation\"}`). No second array. WR-01 discipline preserved.
- **Legacy noise mitigation** (RESEARCH Pitfall #2) is fully addressed: regex `^v?\d+\.\d+\.\d+$` filters out ranges/em-dashes; pre-v1.5.0 mismatches downgraded to `Warn`; unparseable rows skipped. This is the exact correct scope.
- **`main` branch exists locally** — verified via `git branch -a`. Tag reachability check (`merge-base --is-ancestor <tag> main`) is valid.
- **Tag normalization** (`1.7.0` → `v1.7.0`): verified that real git tags carry `v` prefix (e.g. `v1.7.0`) while table cells are bare. The plan's "add `v` prefix if missing" is correct and conditional (no double-prefix).
- **New sibling `PlanningDocFinding` type** (not `PhaseFinding`) is the right call per RESEARCH Open Q1: shipped phases have no active `PhaseFacts`. Separate key under `doctor_json_body` avoids forcing document-level facts through the per-phase reconciliation pipeline.

### Concerns
- **LOW: `git merge-base --is-ancestor <tag> main`** — if `main` is ever force-pushed, a previously-reachable tag could become orphaned. The check would correctly flag this (unreachable → finding), which is what it SHOULD do for a detection-only tool. Not a bug, just noting the behavior is correct for the stated goal.
- **LOW: The plan says the cutoff is "fixed `v1.5.0`"** which is the first consistently-tagged release. This is reasonable but will need updating if the project's release discipline changes. Document the assumption in a code comment.

### Suggestions
- Consider testing the case where `main` is NOT checked out locally (e.g., detached HEAD). `git merge-base --is-ancestor v1.7.0 main` works regardless of current checkout as long as the `main` ref exists.

### Risk: LOW
Well-scoped, noise-mitigated, detection-only. The parse design handles all known edge cases in the real `ROADMAP.md`/`STATE.md` tables.

---

## 21-04-PLAN.md — sequentagent second-process tracking (21c)

### Summary
Gives `sequentagent`'s agents a slot record (A/B + AgentKind) in a per-phase file, surfaced in `status`. Two tasks: (1) data model + write/clear wiring in `agent_result.rs` and `parallel.rs`, (2) status surfacing in `commands.rs`.

### Strengths
- **Correct avoidance of State/save_state.** Phase 19 D-14 (synthetic, never-persisted state) is explicitly honored. The slot record is a standalone sibling file at `.devflow/phase-NN-sequentagent`, not routed through `workflow::save_state`.
- **Path-naming convention** follows `agent_pid_path` (agent_result.rs:893): `devflow_dir(project_root).join(format!("phase-{:02}-sequentagent", phase))`. Consistent.
- **WR-02 compliance:** record stores only slot letter + agent kind — no paths/usernames. Verified the plan explicitly prohibits this.
- **Narrow reading documented.** The plan states the tradeoff: status-only (not `doctor`), agent A is incidentally covered by the slot labeling, and the decision is flagged at plan time per D-06.
- **Stale-record safety:** `render_sequentagent_status` cross-references liveness via `agent::agent_running(pid)`. A stuck slot record with a dead pid renders "not running", never a false live agent. Correct.

### Concerns

- **LOW-MEDIUM: Slot record not cleared on error-exit paths.** I traced `sequentagent` (parallel.rs:278-401) and found five early-return paths where the slot record is written but never cleared:

  1. **Agent A Failed** (line 340-345): `return Err(...)` — slot A written, not cleared.
  2. **Agent A RateLimited, zero commits** (line 350-355): `return Ok(())` — slot A written, not cleared.
  3. **Rebase B onto base fails** (line 365-371): `?` propagates — slot A written, slot B never written, slot A not cleared.
  4. **Agent B Failed/RateLimited** (line 376-391): `return Err(...)` — slot B written, not cleared.
  5. **Agent B integrate fails** (line 392): `?` propagates — slot B written, not cleared.

  The plan only places `clear_sequentagent_slot` at the success path (next to `delete_cron_instructions` at line 396). While the liveness check in `status` makes stale records *harmless* (dead pid → "not running"), they accumulate in `.devflow/` until the phase is re-run. This is a cleanliness/operator-confusion concern, not a correctness bug.

  - **Recommendation:** At minimum, add a comment in the plan acknowledging stale records on error paths are acceptable (liveness-verified). Better: add `clear_sequentagent_slot` calls to the error-return paths, or move the clear into a defer/drop-style pattern.

### Suggestions
- The plan says "surface in `status` (the live-observability command); `doctor` integration is deliberately NOT included." The rationale (WR-01 human/json consistency) is sound. Consider adding a code comment at the `doctor` call site explaining the deliberate omission so a future contributor doesn't "fix" it.

### Risk: LOW-MEDIUM
The error-path slot leakage is a real gap, but mitigated by liveness verification in status rendering. The core design (path-free record, no State involvement, slot-overwrite semantics) is solid.

---

## Cross-Cutting Observations

### Wave ordering: SOUND
21d first (unblocks this phase's own stages), then 21a/21b/21c serialized on `commands.rs` zero-file-overlap. RESEARCH D-07 sequencing is correctly encoded.

### Scope fence: CLEAN
No plan touches 999.25 (release executor), 999.28 (`--base`), or 21e (changelog content). All four plans stay within D-02's reversible/detection-only boundary.

### Test coverage: ADEQUATE
Every plan includes TDD tasks with specific test names, acceptance criteria, and verification commands. The RESEARCH Pitfall #1 warning (WR-01 fixture must be edited, not left as-is) is explicitly addressed in 21-01 Task 2. RESEARCH Pitfall #2 (legacy row noise) is explicitly addressed in 21-03 Task 1.

### Dependency verification: NO NEW DEPS
All four plans use only existing workspace dependencies. Verified against `crates/devflow-core/Cargo.toml` and `crates/devflow-cli/Cargo.toml`.

---

## Summary Table

| Plan | Unit | Risk | Key Concern |
|------|------|------|-------------|
| 21-01 | 21d — staleness content-awareness | **LOW** | None — precise, well-verified |
| 21-02 | 21a — operator discoverability | **MEDIUM** | `State.started_at` is phase-level, not stage-level; progress line will mislead |
| 21-03 | 21b — doctor planning-doc staleness | **LOW** | Tag reachability ref against `main` is correct; noise mitigation verified |
| 21-04 | 21c — sequentagent slot tracking | **LOW-MEDIUM** | Slot record leaked on 5 error-exit paths; harmless but accumulates |

**Bottom line:** Approve for execution. Fix the 21-02 progress-line labeling (or add `stage_started_at` to `State`). Document the 21-04 error-path slot leakage as acceptable (liveness-verified) or add cleanup.

---

## Consensus Summary (3 independent reviewers)

Three source-grounded reviews from three distinct model families — **Codex (OpenAI), OpenCode (Copilot), Hermes (deepseek-v4-pro)**. All three traced claims to `file:line`; all three return **0 blockers** and approve for execution. The third lane did more than add a vote: combined with source re-verification, it **resolved the phase's highest-rated risk** and clarified a severity.

### Unanimous strengths (3/3)
- **21-01 is precise/correct** — the single-line change at `staleness.rs:56`, `affects_compiled_binary` reuse, both regression fixtures (`b.txt`, `trunk2.txt`) correctly flagged, fail-toward-Stale on git error. All three rate it LOW, no concerns.
- **Wave ordering 21d→21a→21b→21c** is load-bearing and correctly reasoned (21d unblocks the phase's own stages; 21a/b/c serialize on `commands.rs`).
- **Scope fence clean** — no 999.25/999.28/21e; all reversible/detection-only; no new dependencies.
- **21-03 core shape right** — third `doctor --json` key (WR-01), `PlanningDocFinding` sibling type, pre-v1.5.0 noise scoping.
- **21-04 correctly avoids `State`/`save_state`** (D-14), path-free record (WR-02), `agent_result.rs` naming convention.

### Unanimous concerns (3/3 — the actual fix list)
1. **[21-02, MEDIUM] In-stage progress uses `State.started_at`, which is phase-level, not stage-level.** Flagged by all three. It would render phase age labeled as stage progress. **Fix:** relabel to "phase age", or add a `stage_started_at` to `State` / derive stage age from the latest `stage_launched` event.
2. **[21-04, LOW–MEDIUM] Slot record not cleared on `sequentagent`'s error-exit paths.** All three flagged it; Hermes enumerated **five** (agent-A fail; agent-A rate-limit/zero-commit; rebase-B fail; agent-B fail/rate-limit; integrate-B fail). **Severity clarified by Hermes + source:** the plan's `status` rendering already probes `agent::agent_running(pid)` (`21-04-PLAN.md:120,150`), so a stale record with a dead pid renders "not running" — **not** a false "live agent" (Codex/OpenCode's original framing). The real issue is **clutter accumulation in `.devflow/`**, not false liveness. **Fix:** clear on all terminal paths (Drop/cleanup guard), or document the leak as acceptable.

### Divergence on 21-03 — RESOLVED by the third review + source check
The two-reviewer pass rated 21-03 the phase's highest risk. The third lane, plus source verification, flipped that:
- **OpenCode (MEDIUM): "reachability against `main` may be origin-only → every `doctor` run spuriously flags tags unreachable." → REFUTED.** `main` **is** a local branch here (`git branch --list main`), and `git merge-base --is-ancestor v1.7.0 main` succeeds offline. Hermes independently verified it. Dismissed — this was the scariest concern (a spurious-flood on every run) and it does not apply.
- **Codex (MEDIUM): "v1.5.0 cutoff needs numeric semver, not string compare (`v1.10.0` breaks lexicographically)." → STANDS.** The plan states "≥ v1.5.0" without specifying numeric-tuple comparison (`21-03-PLAN.md:88`). One-line fix.
- **Codex (HIGH): "detector reads only `## Shipped` + `## Completed`, missing the ROADMAP milestone table." → STANDS as a coverage enhancement, not a live bug.** Confirmed the plan scopes only those two tables (`21-03-PLAN.md:24,38,65`); it wouldn't catch drift appearing only in the milestone table. No current drift there, so it's a breadth improvement, not urgent.
- **Hermes (LOW):** rates 21-03 well-scoped and noise-mitigated.
- **Net:** 21-03 is **lower risk than the two-reviewer consensus implied.** The spurious-flood concern is refuted; what remains is a numeric-semver nicety and an optional cross-table breadth improvement.

### Verdict
**No blockers; approve for execution.** The genuine must-fix set is two cheap items — 21-02's progress label and 21-04's error-path slot clear (or a documented-acceptable note). 21-03's remaining items (numeric semver; optional cross-table scope) are nice-to-haves. All map onto a light `/gsd-plan-phase 21 --reviews` pass; none touch the phase's reversible/detection-only boundary.
