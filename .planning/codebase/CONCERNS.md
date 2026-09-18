---
last_mapped_commit: d581384145e82fd8f7091fee6387d70c6a62fff5
last_mapped_at: 2026-09-18
---
# Codebase Concerns

**Analysis Date:** 2026-09-18

Scope: `crates/`, `scripts/`, `.planning/ROADMAP.md` backlog, at `workspace/denniyahh` `d581384`
(Phase 48 paused at 10/17 plans). Excludes `.worktrees/`, `target/`, lockfiles. Every item below
was checked against current code; ROADMAP 999.x entries whose defect could not be confirmed at
HEAD (e.g. 999.66 `consecutive_failures`, 999.90 default git-flow in validate, 999.109/999.110
fixed in Phase 45) are omitted. Line numbers are at `d581384`.

## Tech Debt

**Very large source files:**

- Issue: Several modules each hold 2.5k-9k lines, much of it inline `#[cfg(test)]` modules.
- Files: `crates/devflow-core/src/agent_result.rs` (9134; tests start at :1314/:3498), `crates/devflow-cli/src/commands.rs` (7458; tests from :3983), `crates/devflow-cli/src/pipeline_outcomes.rs` (5546; tests from :1108), `crates/devflow-cli/src/pipeline_launch.rs` (5343), `crates/devflow-cli/src/preflight.rs` (4443), `crates/devflow-core/src/monitor.rs` (3548), `crates/devflow-cli/src/staleness.rs` (2679), `crates/devflow-core/src/version.rs` (2507)
- Impact: Parallel plans touching these files serialize into near-sequential waves; review and navigation cost is high.
- Fix approach: Move inline test modules to sibling `tests.rs` files first (mechanical), then split `agent_result.rs` by evaluation layer.

**Duplicated non-unique atomic-write helper:**

- Issue: Two copies of the same `with_extension("tmp")` write-then-rename helper.
- Files: `crates/devflow-core/src/workflow.rs:185-193`, `crates/devflow-core/src/gates.rs:356-364` (contrast `crates/devflow-core/src/registry.rs:217`, which already uses a unique temp name)
- Impact: See Known Bugs (999.118).
- Fix approach: One shared helper with unique temp names; Phase 48 SURV-01 targets this.

**Two checkpoint predicates for one concept (999.125, CHKPT-01, in progress):**

- Issue: Preflight and resume use separate scanners.
- Files: `crates/devflow-core/src/verify.rs:131` (`phase_has_blocking_human_checkpoint`), `crates/devflow-core/src/verify.rs:379` (`phase_has_human_only_checkpoint`)
- Impact: A marker can block preflight but not arm resume, or the reverse.
- Fix approach: A single parsed predicate (Phase 48 success criterion 5).

**Ambient git-flow re-resolution in Validate loop-back (999.120):**

- Issue: Validate failure counting re-resolves git-flow from the project instead of the persisted `State::base_branch`.
- Files: `crates/devflow-cli/src/pipeline_outcomes.rs:598-601`; persisted field at `crates/devflow-core/src/state.rs:355`
- Impact: If the config differs from what the run forked from, progress detection measures the wrong range.
- Fix approach: Thread the persisted base through, as the Phase 45 resolve-once pattern does elsewhere.

**Hardcoded `crates/` workspace-member prefix (999.117):**

- Issue: The scoped staleness check treats any `crates/**/*.rs` as build-affecting instead of reading declared workspace members.
- Files: `crates/devflow-cli/src/staleness.rs:24`, `crates/devflow-cli/src/staleness.rs:292-297`
- Impact: False "stale" results for non-member paths under `crates/`. Fails toward Stale, so it is noisy rather than unsafe.
- Fix approach: Parse `[workspace].members` from the root `Cargo.toml`.

## Known Bugs

**Fixed `.tmp` filename lets concurrent writers collide (999.118 / SURV-01):**

- Symptoms: Two processes saving the same phase state or gate share one temp path. One rename can publish the other's bytes, or fail with ENOENT.
- Files: `crates/devflow-core/src/workflow.rs:189`, `crates/devflow-core/src/gates.rs:360`
- Trigger: Concurrent `save_state` for one phase (e.g. monitor and a CLI verb).
- Workaround: None. Phase 48 plans 48-10..48-17 remain unexecuted.

**Commit gate counts all phase-branch commits, not this stage's:**

- Symptoms: A no-op Code retry passes the "made commits" check because Plan-stage commits already exist on the branch.
- Files: `crates/devflow-core/src/agent_result.rs:2411-2431` (`phase_commit_count` uses `develop..feature/phase-NN`)
- Trigger: A Code stage retried after Plan committed, with the agent producing no new work.
- Workaround: Validate's SUMMARY.md check happens to catch it downstream.

**OpenCode capability probe never strips ANSI (999.112):**

- Symptoms: `subagent_dispatch` can report false when colored output wraps the `(subagent)` marker.
- Files: `crates/devflow-core/src/agents/opencode.rs:298` (raw stdout passed to `parse_opencode_agent_list_for_subagent`, :329); `strip_ansi_escapes` exists at :183 but is not applied here
- Trigger: `opencode agent list` emitting SGR sequences.
- Workaround: Degrades to the single-agent path (safe direction).

## Security Considerations

**Plaintext local state under `.devflow/`:**

- Risk: State, gate answers, events and history are world-readable per umask. Registry dirs use 0700 (`crates/devflow-core/src/registry.rs`, test at :423), but `.devflow/` has no equivalent.
- Files: `crates/devflow-core/src/workflow.rs:185` (`ensure_devflow_dir`), `.devflow/events.jsonl`, `.devflow/gates/`
- Current mitigation: Single-user host. The operator accepts local plaintext.
- Recommendations: Low priority. Apply 0700 to `.devflow/` for parity with the registry.

**Gate answers written without verifying a live waiter (SURV-02, in progress):**

- Risk: `gate approve/reject/stop/sweep` can write an answer that no process consumes. At the Ship gate a stale answer could be picked up by a later run.
- Files: `crates/devflow-core/src/gates.rs`, `crates/devflow-core/src/lock.rs:178` (`holder_identity` exists as the building block)
- Current mitigation: Partial, pending the remaining Phase 48 plans.
- Recommendations: Complete Phase 48 success criterion 3.

## Performance Bottlenecks

**CI fan-out and cold container gate (999.123):**

- Problem: Each commit triggers multiple workflow runs. The pre-push container gate runs cold fmt/clippy/test and takes minutes.
- Files: `.github/workflows/ci.yml`, `.github/workflows/devcontainer.yml`, `scripts/check-in-container.sh`
- Cause: Overlapping triggers. The container gate has no warm cache.
- Improvement path: Deduplicate triggers per the 999.123 entry. Consider `cargo nextest` (999.122).

## Fragile Areas

**Process-global env mutation in tests (TEST-01, partially addressed):**

- Files: `clippy.toml:19-21` denies `set_var`/`remove_var` by default. Reasoned exceptions remain in `crates/devflow-cli/src/preflight.rs` (19 sites), `crates/devflow-core/src/monitor.rs:3412-3460`, `crates/devflow-cli/src/test_support.rs`, `crates/devflow-cli/src/pipeline_launch.rs`, `crates/devflow-cli/src/pipeline_gate.rs`, `crates/devflow-cli/src/pipeline_outcomes.rs`, `crates/devflow-cli/src/staleness.rs`, `crates/devflow-core/src/config.rs`, `crates/devflow-core/src/gates.rs:378`
- Why fragile: Remaining sites depend on mutex discipline. A new test that spawns `git` without the mutex races PATH mutation (999.114 `NotFound` flakes).
- Safe modification: Use `Command::env`/`env_remove`. Never add a new `expect(clippy::disallowed_methods)` without an ENV_MUTEX.
- Test coverage: The race is load-dependent. A green local run does not prove its absence.

**`phase7_cli` fixture timing (999.55 / 999.23):**

- Files: `crates/devflow-cli/tests/phase7_cli.rs` (1949 lines; fixed 5s `wait_for` budget)
- Why fragile: Real `git` plus a polling budget under CI contention.
- Safe modification: Widen budgets only alongside a deterministic signal. Do not re-run red CI to green.

**Unattended decision policy vs. auto-decide prompt (999.116):**

- Files: `crates/devflow-core/src/prompt.rs:97-125` (`CODE_STAGE_POLICY`), `crates/devflow-core/src/prompt.rs:613-626` (`checkpoint_auto_decide_prompt`)
- Why fragile: Two independently worded instructions govern one resumed session. The auto-decide prompt does not repeat the merit-based comparison or the package-verification exclusion.
- Safe modification: Derive both from shared literals, as `gate_resolution_rule!()` already does.

## Scaling Limits

**Per-phase state files and single global lock per phase:**

- Current capacity: One writer per phase via `crates/devflow-core/src/lock.rs:33`. State lives in `.devflow/state-NN.json`.
- Limit: Readers that load outside the lock race writers until SURV-01 lands.
- Scaling path: Phase 48 exclusion or loud refusal of second writers.

## Dependencies at Risk

**Upstream GSD / Claude Code behavior coupling:**

- Risk: DevFlow parses GSD artifacts and Claude/Codex/OpenCode output shapes that it does not control (999.101 upstream, 999.107 codex parser, 999.108 Pi dispatch).
- Impact: Silent misclassification after an upstream format change.
- Migration plan: Pin captured fixtures per adapter under `crates/devflow-core/src/agents/`. Add property tests (999.18).

## Missing Critical Features

**Security verdict not checked before Ship (999.111):**

- Problem: The gap surfaces only at Ship time. Not re-verified in code this pass (no dedicated pre-Ship security check was found by grep); treat as unverified-open.
- Blocks: Unattended runs reaching Ship cleanly.

**No live `--mode auto` end-to-end verification (999.119):**

- Problem: The configured-base fork, preflight and merge chain is proven only by unit and fixture tests.
- Blocks: Confidence in unattended mode. Phase 49 is planned to measure it.

## Test Coverage Gaps

**Concurrent writer path:**

- What's not tested: A second writer against `write_state_atomic`/`write_atomic`.
- Files: `crates/devflow-core/src/workflow.rs`, `crates/devflow-core/src/gates.rs`
- Risk: Silent state loss.
- Priority: High (Phase 48 success criterion 1).

**OpenCode marker-less run at CLI level (999.121):**

- What's not tested: The CLI regression for "a marker-less run never advances".
- Files: `crates/devflow-core/src/agents/opencode.rs`, `crates/devflow-cli/tests/`
- Risk: An OpenCode run that emits no completion marker could advance the pipeline anyway, with no CLI test to catch it.
- Priority: Medium.

**Rustdoc and mutation testing absent from green (999.91, 999.95, 999.17):**

- What's not tested: `cargo doc` warnings. Surviving mutants in state machines.
- Files: `scripts/check.sh`
- Risk: Broken intra-doc links. Tests that pass without constraining behavior.
- Priority: Low-Medium.

---

*Concerns audit: 2026-09-18*
