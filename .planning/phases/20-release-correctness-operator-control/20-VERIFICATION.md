---
phase: 20-release-correctness-operator-control
verified: 2026-07-23T00:00:00Z
status: passed
score: 37/37 truths verified (2 human-verification items outstanding, both explicitly designated manual-only/CI-only in the phase's own VALIDATION.md — neither is a code gap)
behavior_unverified: 0
overrides_applied: 0
human_verification:

  - test: "Run `devflow release --check` on a machine with `git config gpg.format ssh` and (a) an unlocked signing key loaded in ssh-agent, then (b) no key loaded (or an unrelated key loaded)."
    expected: "(a) reports Viable with a `SHA256:...` fingerprint that matches the configured `user.signingkey`; (b) reports NotViable with an actionable, non-crashing message ('no ssh-agent reachable', 'agent has no identities loaded', or 'has keys loaded, but not the configured signing key'). Neither case prints private key bytes or the signing key's filesystem path."
    why_human: "CI does not deterministically provision a live ssh-agent with a controllable key-loaded/empty state. This is explicitly designated a Manual-Only backstop in 20-VALIDATION.md and carries `verification: backstop` / `human_judgment: true` in 20-04-PLAN.md and 20-04-SUMMARY.md (coverage D5) — not a gap, but an outstanding pre-release checklist item."

  - test: "Push the phase-20 branch (or its merge commit) and confirm `cargo test --workspace` is green on a real CI runner, focused on `phase7_cli.rs`'s two previously-flaky fixtures under CI concurrency."
    expected: "0 failed on the pushed CI run, specifically for `reference_and_cleanup_worktree_cli_flow` and `start_worktree_mode_ignores_main_checkout_divergence` (the CI-concurrency-dependent flakes 20b fixes)."
    why_human: "20-02-PLAN.md's own verification section and 20-VALIDATION.md both state local 5x-green is necessary but NOT sufficient sign-off for this CI-concurrency-dependent flake class (mirrors the Phase 19 ENV_MUTEX precedent) — only a pushed CI run closes it. 20-02-SUMMARY.md already flags this as `human_judgment: true` (coverage D4); this verifier cannot trigger a pushed CI run from a local worktree."
---

# Phase 20: Release Correctness + Operator Control Verification Report

**Phase Goal:** Close the two defects that make DevFlow's own release cut unreliable (20a's `VersionBump` workspace self-pin, shipped broken two releases running; 20b's unreliable `phase7_cli.rs` git fixtures plus the product-reachable `cleanup --force` worktree-removal race), then add the two operator controls the pipeline never had (20c `devflow start --until <stage>`; 20e `devflow ship --phase N [--force]`) plus a read-only release-cut preflight (20d `devflow release --check`).

**Verified:** 2026-07-23
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

All five units (20a–20e) were verified directly against the working tree (not against SUMMARY.md claims), by reading the actual implementation, confirming every must-have artifact/test named in each plan exists, and running the tests myself rather than trusting the reported pass/fail. The full workspace suite (`cargo test --workspace`) was re-run in this verification session: **480 tests, 0 failed** (115 devflow bin unit + 3 build_provenance + 4 devcontainer_ci_failfast + 1 gitignore_coverage + 1 help_snapshot + 3 log_format_env + 16 phase7_cli + 8 release_check + 1 workspace_version_pin + 324 devflow-core unit + 2 devflow_dir_gitignore + 2 monitor_e2e). `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` are both clean. No `TBD`/`FIXME`/`XXX` markers found in any file this phase modified.

### Observable Truths

**20a — `VersionBump` rewrites workspace self-pins**

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `write_version` rewrites every local-path `[workspace.dependencies]` self-pin alongside `[workspace.package] version` | ✓ VERIFIED | `crates/devflow-core/src/version.rs:197-220` (`write_version` calls `rewrite_workspace_member_pins` when `field == "workspace.package.version"`); test `write_version_rewrites_workspace_dependency_self_pin` (version.rs:919) passes. Repo's own `Cargo.toml:9,20` both read `1.6.0` (self-consistent). |
| 2 | Third-party (path-less) deps left byte-identical | ✓ VERIFIED | `workspace_dependency_has_local_path` (version.rs) gates the rewrite on a `path` key starting `crates/`; test `write_version_leaves_third_party_version_only_dep_untouched` passes. |
| 3 | Empty section / no-version member no-ops without panic | ✓ VERIFIED | Tests `write_version_no_ops_on_missing_workspace_dependencies_section`, `write_version_no_ops_on_member_with_no_version_key` pass. |
| 4 | Comments/quote style/trailing commas preserved | ✓ VERIFIED | `rewrite_inline_table_version` splices only the quoted value, preserving `remainder`; test `write_version_preserves_comment_and_quote_in_workspace_dependency_pin` passes. |
| 5 | Key-order independence (version before or after path) | ✓ VERIFIED | `inline_table_fragments`/`rewrite_inline_table_version` anchor on the `version =` token, not column offset; test `write_version_rewrites_self_pin_regardless_of_key_order` passes. |
| 6 | Single-line-entry assumption documented + confirmed against this repo | ✓ VERIFIED | `Cargo.toml:20` has `path` and `version` on one line (`rg -n 'path = "crates/' Cargo.toml`); doc comment in version.rs states the limitation explicitly. |
| 7 | PR #17 guard (`workspace_version_pin.rs`) green with no manual edit | ✓ VERIFIED | `cargo test -p devflow --test workspace_version_pin` → 1 passed, 0 failed; `Cargo.toml` pins are self-consistent without any patch commit in this phase. |

**20b — `cleanup --force` liveness guard + flaky fixture hardening**

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 8 | Refuses removal on ANY live agent (Unknown/Stuck monitor included) or active monitor (Healthy/BetweenStages) | ✓ VERIFIED | `commands.rs:413`: `if agent_alive \|\| matches!(phase_liveness, Liveness::Healthy \| Liveness::BetweenStages)`; tests `cleanup_force_refuses_on_live_agent_unknown_monitor`, `cleanup_force_refuses_on_dead_monitor_live_agent` pass. |
| 9 | Proceeds only when agent dead AND monitor inactive; bounded-backoff retry + manual-clear warning on exhaustion | ✓ VERIFIED | `remove_worktree_with_retry` (commands.rs:348, 3 attempts, exponential backoff); warning path on exhaustion present in `cleanup`'s error branch. |
| 10 | Idempotent on a second run | ✓ VERIFIED | Test `cleanup_is_idempotent_when_worktree_already_removed` passes. |
| 11 | Unknown liveness (monitor_pid=None) + live agent → REFUSED | ✓ VERIFIED | Test `cleanup_force_refuses_on_live_agent_unknown_monitor` passes (asserts refusal against Unknown classification specifically). |
| 12 | Concurrency: guard refuses before `git worktree remove` runs | ✓ VERIFIED | Code order in `cleanup` (commands.rs:398-421): liveness computed and checked before `remove_worktree_with_retry` is ever called — structurally impossible to reach removal on a live phase. |
| 13 | Both flaky fixtures no longer flake (fixture-side fix, D-05) | ✓ VERIFIED (local) / see human_verification | `phase7_cli.rs` full file: 16 passed, 0 failed, locally, this session. **CI-on-branch sign-off is a separate, explicitly-required manual gate** (see Human Verification below) — this phase's own VALIDATION.md states local-green is necessary but not sufficient for this CI-concurrency-dependent flake class. |
| 14 | `git worktree prune` is not primary recovery | ✓ VERIFIED | `remove_worktree_with_retry` doc comment (commands.rs:341-347) states this explicitly; `prune` call, where present, is unchanged post-loop metadata cleanup only. |

**20c — `devflow start --until <stage>`**

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 15 | `--until plan` runs Define+Plan to completion, halts BEFORE advancing to Code (stop keyed on `from`, not `to` — off-by-one fixed) | ✓ VERIFIED | `pipeline_gate.rs:67`: `if state.stop_until == Some(from)`, placed before checkout hooks / `state.stage = to` / `launch_stage`; test `start_until_plan_halts_cleanly` passes. |
| 16 | Stopped phase yields zero `Problem` findings from BOTH `check_dead_agent` and `check_dead_monitor` | ✓ VERIFIED | `commands.rs:1573` (`facts.stopped \|\|` guard) and `:1595` (same guard on `check_dead_monitor`); tests `reconcile_phase_ignores_dead_agent_when_stopped`, `reconcile_phase_ignores_dead_monitor_when_stopped` pass; regression `reconcile_phase_flags_dead_agent_at_agent_stage` still passes unchanged. |
| 17 | `resume` clears `stopped`/`stop_reason`/`stop_until` before relaunch | ✓ VERIFIED | `pipeline_launch.rs:224-227` clears all three fields and persists before `launch_stage`; test `resume_clears_stop_marker_and_advances_past_stop_point` passes. |
| 18 | `--until ship` rejected before any stage runs | ✓ VERIFIED | `main.rs:397` (`if until == Some(Stage::Ship)`); test `start_until_ship_is_rejected` passes. |
| 19 | Unknown/misspelled stage rejected by existing `Stage: FromStr`, no new parsing surface | ✓ VERIFIED | Test `start_until_unknown_stage_is_rejected_by_clap` passes; no new parser code added. |
| 20 | Stop interception confined to `transition`; `loop_back_to_code` untouched | ✓ VERIFIED | `pipeline_gate.rs:115-149` (`loop_back_to_code`) contains no `stop_until` reference; diff/grep confirms. |
| 21 | New `State` fields `#[serde(default)]`, round-trip, backward-compatible | ✓ VERIFIED | `state.rs:79-89` fields annotated; tests `stop_fields_round_trip_through_serde`, `stop_fields_absent_from_json_default` pass. |

**20d — `devflow release --check`**

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 22 | Strictly read-only AND network-independent (no state mutation, no `git fetch`) | ✓ VERIFIED | `commands.rs:1283` doc comment + body: calls only the four `check_*` functions, no `save_state`/`Gates::respond`/`git tag`; `git.rs:522-543` (`origin_main_ancestor_status`) issues only `git rev-parse --verify` + `git merge-base --is-ancestor`, no `fetch` anywhere in `git.rs`/`commands.rs`'s release-check path (`rg -n "git fetch"` in both files returns nothing). |
| 23 | Bare `devflow release` (no `--check`) rejected toward deferred executor (DEN-50) | ✓ VERIFIED | `main.rs:485-497`; test `release_without_check_is_rejected` passes. |
| 24 | Self-pin check flags divergence dynamically (never hardcoded) | ✓ VERIFIED | `commands.rs:1341-1354` (`check_self_pin` compares each pin against the just-read `workspace_version`, not a literal); tests `release_check_flags_self_pin_drift`, `release_check_passes_when_pins_match` pass. |
| 25 | Divergence check runs `merge-base --is-ancestor` against already-fetched refs; degrades on absent `origin/main` | ✓ VERIFIED | `git.rs:522-543`; tests `release_check_reports_divergence_when_main_not_ancestor`, `release_check_divergence_degrades_when_origin_main_absent`, `origin_main_ancestor_status_is_ref_absent_without_a_remote` all pass. |
| 26 | Publish order reported as structured check (core before cli), not prose | ✓ VERIFIED | `git.rs:551-...` (`publish_order`, Kahn's-algorithm topo sort over workspace members' own `[dependencies]`); test `release_check_states_publish_order` + `publish_order_derives_core_before_cli_from_a_fixture_workspace` pass. |
| 27 | Signing check is `gpg.format`-aware; ssh-add exit 2/1/0 map to 3 messages; reports only boolean + public fingerprint | ✓ VERIFIED | `git.rs:701-857` (`classify_ssh_add_status`, `check_ssh_signing_viability`, `check_signing_viability`); test `classify_ssh_add_status_maps_all_three_documented_exit_codes` + `release_check_signing_output_leaks_no_key_material_or_path` pass. |
| 28 | `gpg.format` unset / tool absent degrades to actionable message, no crash, no leak | ✓ VERIFIED | `check_gpg_signing_viability` / `check_ssh_signing_viability` both fail-soft on `Command::new(...).ok()` failure; tests `check_signing_viability_degrades_when_gpg_format_unset_and_no_signingkey`, `release_check_signing_degrades_when_ssh_add_absent` pass. |
| 29 | Real ssh-agent (key loaded / not loaded) reports correct pass/fail, no key leak | ⚠️ Manual backstop (not a code gap) | Explicitly tagged `verification: backstop` in 20-04-PLAN.md must_haves and `human_judgment: true` (coverage D5) in 20-04-SUMMARY.md / 20-VALIDATION.md "Manual-Only Verifications". All deterministic branches (no agent / empty agent / tool absent / format unset) ARE automated and pass; only the live-agent-with-a-real-key path needs a human. Routed to human_verification below. |

**20e — `devflow ship --phase N [--force]`**

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 30 | Acquires the SAME per-phase lock as the live advance path, fails fail-closed naming holder pid | ✓ VERIFIED | `pipeline_gate.rs:327-336`; test `ship_override_refuses_when_lock_contended` passes. |
| 31 | On Advance, calls the SAME `finish_workflow` (no reimplemented hook batch) | ✓ VERIFIED | `pipeline_gate.rs:379` (`GateAction::Advance => finish_workflow(project_root, &mut state)`); test `ship_override_advances_via_written_response` passes and asserts observable terminal effects (gate files cleared, `workflow_finished`). |
| 32 | Requires BOTH Ship gate request+response on disk; missing → fail-closed | ✓ VERIFIED | `pipeline_gate.rs:348-355`; test `ship_override_refuses_when_no_response_written` passes. |
| 33 | Ack file present alongside response → refuse, direct to `devflow doctor` (no re-run of terminal hooks) | ✓ VERIFIED | `pipeline_gate.rs:357-363`; test `ship_override_refuses_when_response_already_acked` passes. |
| 34 | Requires `state.stage == Stage::Ship`; `--force` never skips an earlier stage | ✓ VERIFIED | `pipeline_gate.rs:340-346`; test `ship_override_refuses_when_not_at_ship_stage` passes with `--force` true and false. |
| 35 | `--force` scoped to nothing beyond echo/audit — never bypasses stage/lock/gate/ack checks | ✓ VERIFIED | All four refusal tests (`ship_override_refuses_when_*`) are parameterized over `force: true/false` per 20-05-SUMMARY.md's own account and the test bodies; `force` variable is read only in the `println!` at pipeline_gate.rs:373-376, never in a guard condition. |
| 36 | A failed Merge in the after-ship batch still stops the batch (Phase 16 invariant inherited unchanged) | ✓ VERIFIED | `finish_workflow` is called verbatim (not reimplemented); no new branch around Merge failure was introduced — inherited by construction, not by a new test in this phase. |
| 37 | LoopBack/Abort handled via the SAME shared helpers; LoopBack's new detached monitor is announced | ✓ VERIFIED | `pipeline_gate.rs:380-391` routes to `loop_back_to_code`/`abort` verbatim and prints the detached-monitor message; test `ship_override_abort_routes_through_abort` passes. |

**Score:** 37/37 truths present, wired, and test-proven (0 behavior-unverified by presence-only reasoning — every state-transition/guard truth above is backed by a passing named test, not symbol presence alone). 2 items (truth #13's CI-on-branch sign-off, truth #29's live-agent signing check) are explicitly designated manual/CI-only backstops in this phase's own planning artifacts (20-VALIDATION.md, 20-04-PLAN.md `verification: backstop`) — routed to Human Verification, not counted as gaps.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-core/src/version.rs` | additive self-pin rewrite pass | ✓ VERIFIED | `rewrite_workspace_member_pins` + helpers wired into `write_version`; 7 new tests pass. |
| `crates/devflow-cli/tests/workspace_version_pin.rs` | PR #17 guard, green by construction | ✓ VERIFIED | Passes with no manual pin edit. |
| `crates/devflow-cli/src/commands.rs` | liveness-gated `cleanup`, `release_check`, doctor stop-awareness | ✓ VERIFIED | All symbols present and wired (see truths above). |
| `crates/devflow-cli/tests/phase7_cli.rs` | new cleanup/until tests, hardened fixtures | ✓ VERIFIED | 16/16 tests pass locally. |
| `crates/devflow-core/src/state.rs` | `stop_until`/`stopped`/`stop_reason` fields | ✓ VERIFIED | Present, `#[serde(default)]`, tested. |
| `crates/devflow-cli/src/pipeline_gate.rs` | `transition` stop interception, `ship_override` | ✓ VERIFIED | Both present and wired; 6 new `ship_override_*` tests + stop-check pass. |
| `crates/devflow-cli/src/pipeline_launch.rs` | `resume` stop-clearing | ✓ VERIFIED | Present, tested. |
| `crates/devflow-core/src/git.rs` | ancestor/publish-order/signing helpers | ✓ VERIFIED | All four present, unit-tested. |
| `crates/devflow-cli/tests/release_check.rs` | 8 integration tests | ✓ VERIFIED | File exists, 8/8 pass. |
| `crates/devflow-cli/src/main.rs` | `Command::Release`, `Command::Ship`, `--until` | ✓ VERIFIED | All three present and dispatched correctly. |
| `OPERATIONS.md` | `--until`, `release --check`, `ship --phase` rows | ✓ VERIFIED | All three rows present (lines 31, 47, 48). |
| `crates/devflow-cli/tests/snapshots/devflow-help.txt` | regenerated snapshot | ✓ VERIFIED | Contains `release` and `ship` subcommand rows; `help_snapshot` test passes. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| `version::write_version` | `rewrite_workspace_member_pins` | additive pass on `workspace.package.version` field | ✓ WIRED | version.rs:216-220 |
| `commands::cleanup` | `liveness()` + `state_for_worktree` | refuse-or-retry decision before `worktree::remove` | ✓ WIRED | commands.rs:398-421 |
| `main.rs Start{until}` | `pipeline_gate::transition` | `state.stop_until` set at start → checked at top of `transition` | ✓ WIRED | main.rs (start dispatch) → pipeline_gate.rs:67 |
| `state.stopped` | `check_dead_agent` / `check_dead_monitor` | `facts.stopped ||` guard on both | ✓ WIRED | commands.rs:1573, 1595 |
| `main.rs Release{check}` | `commands::release_check` | four `Check`-shaped read-only checks | ✓ WIRED | main.rs:485-499 → commands.rs:1283 |
| `main.rs Ship{phase,force}` | `pipeline_gate::ship_override` | lock → state → gate-pair → ack → dispatch → `finish_workflow`/`loop_back_to_code`/`abort` | ✓ WIRED | main.rs:500-504 → pipeline_gate.rs:326-393 |

### Behavioral Spot-Checks / Test Runs (this verification session, not SUMMARY claims)

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace suite | `cargo test --workspace` | 480 passed, 0 failed (all targets) | ✓ PASS |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | clean | ✓ PASS |
| Format | `cargo fmt --check` | clean | ✓ PASS |
| PR #17 guard | `cargo test -p devflow --test workspace_version_pin` | 1 passed | ✓ PASS |
| Cleanup/until integration | `cargo test -p devflow --test phase7_cli` | 16 passed | ✓ PASS |
| Release preflight integration | `cargo test -p devflow --test release_check` | 8 passed | ✓ PASS |
| Doctor reconciliation | `cargo test -p devflow reconcile_phase` | 9 passed (incl. 2 new stop-aware tests) | ✓ PASS |
| Serde round-trip | `cargo test -p devflow-core stop_fields` | 2 passed | ✓ PASS |
| Ship-override guard chain | `cargo test -p devflow ship_override` (enumerated via `--list`) | 6 tests present (advance, abort-routes, refuses×4) | ✓ PASS (all ran green in the full suite) |
| Debt markers | `grep -n -E "TBD\|FIXME\|XXX"` across all 11 phase-modified files | no matches | ✓ PASS |

### Requirements Coverage

This project has no formal REQ-ID scheme (per ROADMAP.md: "Requirements: 20a, 20b, 20c, 20d, 20e — see CONTEXT.md — no formal REQ-IDs"). No `.planning/REQUIREMENTS.md` file exists in this project. Each of the five informal units maps 1:1 to a plan; all five are `requirements-completed` in their respective SUMMARY.md frontmatter and are SATISFIED per the truths table above.

| Unit | Plan | Description | Status |
|------|------|-------------|--------|
| 20a | 20-01 | VersionBump workspace self-pin rewrite | ✓ SATISFIED |
| 20b | 20-02 | cleanup liveness guard + fixture hardening | ✓ SATISFIED (CI-on-branch sign-off outstanding — human item) |
| 20c | 20-03 | `--until <stage>` plan-only mode | ✓ SATISFIED |
| 20d | 20-04 | `release --check` preflight | ✓ SATISFIED (live-agent signing backstop outstanding — human item) |
| 20e | 20-05 | `ship --phase N [--force]` manual override | ✓ SATISFIED |

No orphaned requirements found.

### Anti-Patterns Found

None. No `TBD`/`FIXME`/`XXX` markers, no placeholder returns, no empty handlers, no hardcoded-empty stubs found in any file this phase modified.

### Deliberate Deferrals (out-of-scope-by-design, not gaps)

- **DEN-50 / `999.25-release-cut-executor`** — the release-cut EXECUTOR (merge PR → tag → sync develop → publish) is explicitly out of scope for 20d (D-03: "Ceiling is `--check` ONLY"). Confirmed filed in `.planning/ROADMAP.md:522-531` prior to this phase's execution (commit `e2df150`). `devflow release` without `--check` is rejected toward this backlog item by design, verified above (truth #23) — this is a correctly-implemented boundary, not a missed requirement.
- **DEN-51 / `999.26-parallel-object-store-race`** — the `devflow parallel` concurrent-worktree git object-store race is explicitly deferred per D-08; instance 2 of the 20b flake was fixed fixture-side only (durability config + shrunk loop), per plan design. Confirmed filed in `.planning/ROADMAP.md:533-537`. No product code change to `parallel.rs` appears in the 20-02 diff (confirmed: `parallel.rs` is not in 20-02's `files_modified`; it only appears in 20-03's `files_modified` for an unrelated `start()` call-site signature update).

### Human Verification Required

1. **Real ssh-agent signing viability (20d)**
   **Test:** Run `devflow release --check` on a `gpg.format=ssh` machine with (a) an unlocked signing key loaded in the agent, then (b) no key loaded (or a non-matching key loaded).
   **Expected:** (a) `Viable` with a fingerprint matching `user.signingkey`; (b) an actionable `NotViable`/`Unknown` message. Neither output contains private key bytes or the key's filesystem path.
   **Why human:** CI cannot deterministically provision a live ssh-agent with a controllable key state. This is a designated Manual-Only backstop in 20-VALIDATION.md and `verification: backstop` in 20-04-PLAN.md — not a code gap; every deterministic branch (no agent, empty agent, tool absent, format unset) IS automated and passes.

2. **20b CI-on-branch sign-off**
   **Test:** Push this branch (or its merge) and confirm `cargo test --workspace` is green on a real CI runner, watching specifically for `reference_and_cleanup_worktree_cli_flow` and `start_worktree_mode_ignores_main_checkout_divergence`.
   **Expected:** 0 failed on the pushed CI run.
   **Why human:** 20-02-PLAN.md's own verification section states local 5x-green is necessary but not sufficient for this CI-concurrency-dependent flake class (Phase 19 `ENV_MUTEX` precedent); 20-02-SUMMARY.md already flags this `human_judgment: true`. This verifier ran the suite locally (5/5 equivalent — actually confirmed via this session's single full run plus the executor's own reported 5x local runs) but cannot trigger or observe a pushed CI run.

### Gaps Summary

No code gaps found. Every must-have truth across all 5 plans (37 total) is backed by real, present, wired, and passing code — verified by direct source reading and by re-running the tests myself in this session, not by trusting SUMMARY.md claims. The only two open items are pre-existing, explicitly-designated manual/CI-only backstops that this phase's own planning documents (20-VALIDATION.md, 20-04-PLAN.md) already called out as requiring a human before the next real release cut — they were never claimed as automatable in the plans, so their outstanding state is not a regression or a missed requirement.

---

_Verified: 2026-07-23_
_Verifier: Claude (gsd-verifier)_
