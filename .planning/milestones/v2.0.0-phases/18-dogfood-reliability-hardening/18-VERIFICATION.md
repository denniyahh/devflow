---
phase: 18-dogfood-reliability-hardening
verified: 2026-07-21T05:46:15Z
status: passed
score: 7/7 must-haves verified
behavior_unverified: 0
overrides_applied: 0
nyquist_compliant: true
---

# Phase 18: Dogfood Reliability Hardening Verification Report

**Phase Goal:** Make DevFlow's own supervision layer trustworthy and usable from a plain
terminal. Close the specific reliability gaps found by dogfooding that cost real hours or
produced a proven false-green.

**Verified:** 2026-07-21T05:46:15Z
**Status:** passed
**Re-verification:** No — initial verification

This project has no `REQUIREMENTS.md`; requirement text lives in `CONTEXT.md` and the
ROADMAP.md Phase 18 section (18a–18g), each mapped to exactly one of the seven plans
(18-01→18a, 18-02→18g, 18-03→18b, 18-04→18d, 18-05→18e, 18-06→18c, 18-07→18f, confirmed via
commit message prefixes). Absence of REQUIREMENTS.md is consistent with phases 12–17 and is
not treated as a gap.

## Goal Achievement

### Per-Requirement Verdict

| # | Requirement | Verdict | Source Evidence |
|---|---|---|---|
| 18a | `doctor` project-aware reconciliation, read-only by default | ✓ VERIFIED | `doctor(project_root, json)` (main.rs:3409) binds `project_root` (no longer `_project_root`), calls `collect_phase_facts` → `reconcile_phase` diffing `state.stage` vs. last `stage_launched` event, live `agent_pid`/`monitor_pid`, open gates, and feature-branch existence (main.rs:3627–3859). `doctor_is_read_only_on_a_mismatched_project` (main.rs:8180) runs `doctor(root, false)` **twice** and asserts byte-identical state-file length/mtime and unchanged `events.jsonl` line count. Test passes standalone. |
| 18b | `monitor_pid` persisted + serde-covered; "stuck" rendered distinctly | ✓ VERIFIED | `State.monitor_pid: Option<u32>` with `#[serde(default)]` (state.rs:66–72); round-trip test `monitor_pid_round_trips_through_serde` and absent-defaults-to-`None` test (state.rs:289–321) both pass. `liveness()` (main.rs:2806) is a pure 4-state predicate (`Healthy`/`BetweenStages`/`Stuck`/`Unknown`) matched on `monitor_pid` first so a pre-18b state can never misclassify as `Stuck`; `status()` prints `liveness: stuck — needs devflow resume` distinctly (main.rs:2884–2887) and `doctor`'s `check_dead_monitor` reuses the same `liveness()` call (main.rs:3722), so the two can't drift. `monitor_pid` is set at spawn in `launch_stage_inner` (main.rs:1301–1307) and persisted before `stage_launched` is emitted. |
| 18c | Staleness evaluated against `worktree_path` HEAD, not `project_root`; self-dogfood block, not warn | ✓ VERIFIED | `enforce_build_staleness` derives `execution_root = state.worktree_path.as_deref().unwrap_or(project_root)` (main.rs:1184) and evaluates ancestry/dirty-tree against it. `worktree_staleness_fixture` (main.rs:6857) builds a **real** `git worktree add` with two build-affecting commits made only inside the worktree; `embedded_commit_is_stale_uses_worktree_head` proves the same commit is simultaneously `Fresh` vs. `project_root` and `Stale` vs. the worktree HEAD; `enforce_build_staleness_blocks_self_dogfood_behind_worktree_head` proves the real entry point now returns `Err` (BLOCK) naming the worktree, not `project_root`; `staleness_without_worktree_is_unchanged` pins the non-worktree fallback. All three pass standalone and in the full suite. |
| 18d | `consecutive_failures` reset scoped to a pure predicate; ceiling reachable | ✓ VERIFIED | `mode::transition_resets_consecutive_failures(from, to) = !matches!((from, to), (Code, Validate))` (mode.rs:75–77) — false only for the one hop that made `MAX_CONSECUTIVE_FAILURES` unreachable. `transition()` (main.rs:2011–2023) gates the reset on this predicate while `infra_failures` still resets unconditionally, matching the documented asymmetry. `consecutive_failures_reaches_ceiling_across_cycles` (main.rs:5567) drives real `handle_validate_outcome`/`transition()` cycles and asserts `state.consecutive_failures == MAX_CONSECUTIVE_FAILURES` and `should_gate` becomes true — passes. `transition_resets_infra_failures` (main.rs:5485) is present, unmodified in the 18-04 diff (`git show 3036927` shows only additions elsewhere), and passes. |
| 18e | Layer 0 affirmative success at Validate consults Layer 1 verdict; three-way `ValidateOutcome`; binding decision honored | ✓ VERIFIED | `reconcile_layer0_verdict` (agent_result.rs:821–834) is invoked from `evaluate_agent_result_inner` for every Layer-0 affirmative success and only overrides `verdict` when `state.stage == Validate`, keeping `decided_by_layer: Some(0)`. `ValidateOutcome::{Passed, Failed, Ambiguous(String)}` (main.rs:1666) and `classify_validate_outcome` (main.rs:1689) implement exactly the binding decision: `Some(Verdict::Pass)` wins first; probe-pass + `Gaps`/`None` → `Ambiguous`, gated immediately in `handle_validate_outcome` via `forced = matches!(outcome, Ambiguous(_))` (main.rs:1723), never touching `consecutive_failures`. The shared regression test `external_verify_cycles_reach_ceiling_without_unbounded_loop` (main.rs, commit `1157d35`) proves both fixes hold together: Arm A shows an `Ambiguous` outcome gates on cycle one without touching the counter; Arm B shows a genuine `Failed` outcome still reaches `MAX_CONSECUTIVE_FAILURES`. Both arms pass. |
| 18f | `GateAction::Advance` skips `run_preflight` re-check; `LoopBack` re-checks; persisted, bounded `preflight_retries`; reset pinned by disk-reload test | ✓ VERIFIED | `launch_stage`/`launch_stage_inner` split (main.rs:1339–1370, 1242) lets `run_preflight`'s `Advance` arm call `launch_stage_inner` directly, bypassing `run_preflight` entirely (main.rs:853–861); `LoopBack` still calls the full `launch_stage` (main.rs:863–868). `State.preflight_retries: u32` (state.rs:44–55, `#[serde(default)]`) is checked against `mode::MAX_PREFLIGHT_RETRIES = 3` (mode.rs:49) **before** writing another gate (main.rs:825). `run_preflight_advance_skips_recheck_on_idempotently_failing_check`, `run_preflight_loopback_bounds_recursion`, and `preflight_retries_reset_on_pass` (the last explicitly reloads state from disk to prove the reset survives a monitor restart) all pass. |
| 18g | `parallel_creates_two_worktrees_and_spawns_two_monitors` assertions interleaved per-wait | ✓ VERIFIED | `phase7_cli.rs:184–198` now does `wait_for(&phase7_stdout); assert!(phase7_stdout.exists()); wait_for(&phase8_stdout); assert!(phase8_stdout.exists());` — each assertion runs inside its own capture's wait window, not after both waits. Re-ran the exact test 5× standalone (all passed, `1 passed` confirmed in output, not just exit code) — cargo-test-exact-false-green pitfall avoided per project convention. |

**Score:** 7/7 requirements verified.

### Second-Opinion Items (requested, not part of the pass/fail score)

**1. `unreachable!()` at `handle_validate_outcome`'s final match (main.rs:1753, not line 1613 —
line numbers appear to have shifted since the question was written; confirmed this is the
only `unreachable!()` in this function) — is the invariant sound?**

Yes, structurally sound, not just "probably fine":

```rust
let forced = matches!(outcome, ValidateOutcome::Ambiguous(_));
if forced || state.mode.should_gate(Stage::Validate, state.consecutive_failures) {
    ...
    return match run_gate(project_root, state, Stage::Validate, &context)? { ... };
}
match outcome {
    ValidateOutcome::Passed => transition(...),
    ValidateOutcome::Failed => loop_back_to_code(...),
    ValidateOutcome::Ambiguous(_) => unreachable!(...),
}
```

`forced` is computed directly from the same `outcome` binding via `matches!`, and `outcome` is
never reassigned between the two blocks. Whenever `outcome` is `Ambiguous(_)`, `forced` is
`true` by construction, so the `if` always takes its branch and `return`s — either via the
`GateAction` match or via the `?` propagating a `run_gate` error — before control ever reaches
the second `match`. There is no code path (including a `run_gate` error) that reaches the
second match with `outcome` still `Ambiguous`. The `unreachable!()` arm exists only to satisfy
Rust's match-exhaustiveness checker for a 3-variant enum; it is not a latent risk.

**2. WR-02 path/username leak class — any NEW output this phase added leak an absolute home
path or OS username into a persisted artifact?**

No new leak was introduced. All genuinely new output this phase added was audited:

- `doctor`'s `PhaseFinding.detail`/`repair` strings (all 6 `check_*` functions, main.rs:3660–3774) interpolate only phase numbers, stage names, and pids — no `Path` is ever formatted into them. This matches the function-level doc comment's explicit claim ("Never carries a filesystem path or username (T-18-01)").
- `status()`'s new `monitor_pid: {pid} (running: {bool})` / `liveness: ...` lines (main.rs:2872–2887) are pid/bool only.
- `render_reconciliation_text`/`render_reconciliation_json` (main.rs:3906–3958) never format a `Path`.

One genuine absolute-path exposure DOES exist and IS persisted to `events.jsonl`: the
`self-dogfood stale build blocked` message in `enforce_build_staleness` names
`execution_root.display()` (main.rs:1189–1213), and that message is written verbatim (via
`truncate_reason`) into the `self_dogfood_stale_blocked` event's `reason` field. On a typical
Linux path (`/home/<user>/...`) this does embed the OS username. However, this is **not new to
this phase** — `git show 10730ea^:crates/devflow-cli/src/main.rs` (the pre-18c version) shows
the identical message shape already naming `project_root.display()` and already being emitted
into `events.jsonl` the same way. 18c changed *which* path variable is named (`execution_root`
vs. `project_root`), not whether a path is named at all. This is a standing, pre-existing
exposure class, not a regression this phase introduced — flagged for the operator's awareness
per the question, but not scored against 18a–18g.

### Anti-Patterns / Findings

| Finding | Severity | Detail |
|---|---|---|
| Residual flake in new 18c fixture | ⚠️ Warning | `embedded_commit_is_stale_uses_worktree_head` (main.rs:6931) is a real `git worktree add`/`git commit` fixture, not guarded by `ENV_MUTEX` or `agent_free_git_only_path_dir` the way this codebase's other PATH-mutating tests are. Reproduced a `git commit` failure (not a "binary not found" error — `git` itself resolved) at roughly 1-in-8 to 1-in-10 when run concurrently with the phase's own PATH-mutating tests (`consecutive_failures_reaches_ceiling_across_cycles`, `run_preflight_*`) via a curated multi-name `cargo test -- name1 name2 ...` filter. **Not reproduced** across 8 consecutive full `cargo test -p devflow --bin devflow` runs (104/104 each time), nor in one full `cargo test --workspace` run (matches the operator's independently reported 424/424), nor across 3 standalone reruns of the single test. This is the same general class of flake this codebase has already named and partially fixed (WR-07 `build_provenance`, 19i `transition_resets_infra_failures`/`96411eb`) — a new instance was introduced by 18c's fixture without applying the same `ENV_MUTEX`/neutral-PATH hardening, somewhat in tension with the "reliability hardening" framing of this phase (18g is explicitly about the analogous phase7_cli flake, but doesn't cover this new one). Does not affect correctness of the 18c fix itself — the fix and its RED-then-GREEN proof are sound; this is test-harness fragility under thread contention. Recommend a follow-up (`ENV_MUTEX` guard or `#[serial]`-style isolation) but this is not blocking — production behavior is verified correct and the full suite is reliably green. |
| Stale `18-VALIDATION.md` frontmatter | ℹ️ Info | `18-VALIDATION.md` still carries `status: draft`, `nyquist_compliant: false`, `wave_0_complete: false`, and every per-requirement row is `⬜ pending`/`❌ W0`, despite all 7 plans having been executed and summarized. Every test named in its Per-Task Verification Map exists and passes (confirmed above). This is a documentation-sync gap in the phase's own paper trail, not a functional gap — consistent with the project memory note that GSD's VALIDATION.md tracking can go unowned post-execution. Does not affect this verification's `nyquist_compliant` determination, which is based on the actual test evidence, not the file's stale self-report. |
| No debt markers | — | `git diff fb251f3^..8f7cabd -- crates/` shows no `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` additions across the phase's changed lines. |

### Regression Posture (independently corroborated, not just trusted)

- `cargo test --workspace`: ran once independently — clean (295 + 104 + smaller suites, all passed, matching the operator's reported 424/424).
- `cargo clippy --workspace --all-targets -- -D warnings`: ran independently — clean, 0 warnings.
- `cargo fmt --check`: ran independently — clean.
- `git status --porcelain`: clean tree, consistent with the operator's report.
- Spot-ran 8 of the phase's own named regression tests individually (all pass; several re-run 3–10× to rule out the `cargo-test-exact-false-green` pitfall by asserting `1 passed`/`N passed` in output, not exit code alone).

### Requirements Coverage

All 7 phase requirement IDs (18a–18g) declared across the phase's 7 plans are SATISFIED — see
per-requirement table above. No orphaned requirements: `ROADMAP.md`'s Phase 18 section lists
exactly 18a–18g plus an "Already Resolved" and "Explicitly Out of Scope" list, both of which
are explicitly non-blocking for this phase and are not orphaned (they're either pre-closed or
explicitly deferred to the backlog, per `CONTEXT.md`).

### Human Verification Required

None. Every requirement was verifiable via source inspection plus an independently-executed,
passing behavioral test (not merely a SUMMARY claim).

### Gaps Summary

No blocking gaps found. All 7 phase requirements (18a–18g) are implemented in source, wired
into the real call paths (not orphaned), and each carries a real passing test that exercises
the specific behavior claimed (state-transition/invariant tests, not just presence checks) —
`doctor`'s read-only twice-run proof, the real `git worktree add` staleness fixture, the
cross-cycle `consecutive_failures` ceiling proof, the combined 18d+18e integration test, and
the preflight-retry disk-reload test all directly exercise runtime behavior rather than symbol
presence alone. Two second-opinion questions were independently investigated and answered
above (`unreachable!()` invariant is sound; the one path-leak pattern in
`enforce_build_staleness` predates this phase). One non-blocking WARNING is recorded: a new,
unguarded git-spawning test fixture added by 18c exhibits a low-probability (~10%,
concentrated-load-only) flake of the same class this codebase has previously named and
partially fixed elsewhere — recommended as follow-up hardening, not a phase-goal blocker.

---

_Verified: 2026-07-21T05:46:15Z_
_Verifier: Claude (gsd-verifier)_
