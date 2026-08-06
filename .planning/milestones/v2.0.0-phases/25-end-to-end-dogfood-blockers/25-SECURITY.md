---
phase: 25
slug: end-to-end-dogfood-blockers
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
block_on: high
threats_total: 129
threats_closed: 129
register_authored_at_plan_time: true
created: 2026-07-28
---

# Phase 25 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

State B run (no prior SECURITY.md). The register was authored at plan time — 18 of 19 plans carry
a `<threat_model>` block — so the audit verified that declared mitigations exist rather than
scanning for new threats. The one exception is plan 25-19, which had no threat model; a
retroactive STRIDE register was built for its diff and verified alongside the rest. See
*Register Provenance* below.

---

## Trust Boundaries

Aggregated across the phase's 18 plan-level threat models. The distinct boundaries this phase's
code actually crosses:

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| `devflow start` → the repository's ref database | An unattended automated process writes `refs/heads/<base>`, a ref humans and other tools depend on | Git ref updates (destructive if unconditional) |
| Concurrent writer → the same ref database | Another `devflow`, an operator, or a hook may move the base ref between this code's read and its write | Git ref state (lost-update window) |
| Linked worktree → the same ref database | This repository routinely runs several worktrees; each pins a ref this code can move | Worktree HEAD / index / working tree |
| `origin` → local remote-tracking refs | `base_ref_currency` fetches before comparing, so comparison inputs cross a network boundary | Remote ref state |
| `/proc` census → signal delivery | A structural argv/euid scan feeds a TERM→SIGKILL escalation path | OS process identifiers, signals |
| Registry / lock / state files → reachability filter | Machine-wide read of every registered root's `monitor_pid` and lock holders | Process ownership facts |
| Conventional-commit history → computed release version | `compute_version` gates whether a release ships and at what version | Version integrity (supply-chain adjacent) |
| Refusal / finding messages → operator terminal | Messages must not embed filesystem paths (on Linux a path embeds the operator's username) | Potential PII in diagnostics |
| Test fixtures → real spawned processes | Test helpers spawn and signal real detached children | OS process identifiers, signals |

---

## Threat Register

129 rows: 124 declared across 18 plan-level `<threat_model>` blocks, plus 5 in the retroactive
25-19 register. **All 129 verified CLOSED.**

Full per-row detail lives in each plan's own `<threat_model>` block (`25-NN-PLAN.md`), which
remains the authoritative source. Summarised here by severity, with every critical row and the
retroactive register enumerated in full.

### Distribution

| Severity | Count | Blocking under `block_on: high` | Open |
|----------|-------|-------------------------------|------|
| critical | 6 | yes | 0 |
| high | 65 + 3 (25-19) | yes | 0 |
| medium | 39 + 2 (25-19) | no | 0 |
| low | 14 | no | 0 |
| **total** | **129** | 74 blocking-relevant | **0** |

Dispositions: 107 `mitigate`, 17 `accept`, plus the retroactive 5 (all `mitigate`).

### Critical threats (6/6 closed)

| Threat ID | Category | Component | Disposition | Mitigation Evidence | Status |
|-----------|----------|-----------|-------------|---------------------|--------|
| T-25-50 | Elevation of Privilege | `preflight_major_bump_check` composed before `hooks_after_ship` | mitigate | `preflight.rs:841-857` runs it unconditionally and aggregates; called from `run_preflight` (`:895-902`) strictly before any Merge/Ship hook path | closed |
| T-25-51 | Elevation of Privilege | `yes_ship` must not auto-approve the major-bump gate | mitigate | `pipeline_gate.rs:281-296` takes `auto_response` as an explicit param and doc-forbids deriving it from `state`; its only caller `run_gate` (`:261-275`) always passes `None` | closed |
| T-25-08-01 | Elevation of Privilege | Aggregation replaces the `?`-chain so `Advance` cannot discharge an unevaluated check | mitigate | `preflight.rs:841-857` — all three checks run unconditionally, errors joined not short-circuited | closed |
| T-25-08-02 | Tampering | Major-bump check scoped to the phase's worktree, not just `project_root` | mitigate | `preflight.rs:707` — `execution_root = state.worktree_path.as_deref().unwrap_or(project_root)` | closed |
| T-25-09-01 | Tampering | `release_range_start` anchors correctly under an intervening trunk commit | mitigate | `version.rs:301-357` full ancestry-path walk; tripwire tests `two_squash_sync_cycles_anchor_to_the_second_merge_only`, `trunk_commit_between_tag_and_sync_merge_still_anchors_at_the_sync_merge`, `feature_merge_after_sync_merge_does_not_move_the_anchor` | closed |
| T-25-15-01 | Denial of Service | `gate sweep --reap-strays` must not SIGKILL a live registered monitor | mitigate | `commands.rs:3050-3122` — `registry_reachable_pids` + `retain_unreachable_strays` + `unreachable_stray_candidates`; the only other production caller of `discover_stray_devflow_processes()` is a read-only post-pass report at `:1247` | closed |

### High-severity threats (68/68 closed)

Verified individually at the cited line for every plan whose files fall inside the phase's real
security surface. Representative rows:

| Threat ID | Component | Mitigation Evidence | Status |
|-----------|-----------|---------------------|--------|
| T-25-01 | `classify_range_bump` breaking detection | `version.rs:429-452` uses `git_conventional::Commit::parse(...).breaking()`, not a regex | closed |
| T-25-02 | `highest_semver_tag` / `reachable_semver_baseline` | `version.rs:194-230` — `filter_map(Version::parse(..).ok())`, never panics on a malformed tag | closed |
| T-25-04 | `compute_version` refuses rather than under-reporting | `version.rs:474-492` returns `Err(UnreachableBaseline)`, never a smaller `Ok` | closed |
| T-25-10 | `/proc` census uid filter | `agent.rs:413,427-432` skips any `/proc/<pid>` not owned by `geteuid()` before reading anything | closed |
| T-25-11 | Narrow argv matchers only | `agent.rs:478-490` `classify_stray_layer` — two structural matchers, no heuristics | closed |
| T-25-12 | TOCTOU re-confirmation before signalling | `commands.rs:1343` re-checks `is_same_process` immediately before the signal | closed |
| T-25-14 | Non-positive pid guard | `agent.rs:118-128` rejects `pid<=0` / out-of-`pid_t` before any `kill` | closed |
| T-25-40 | `ensure_base_ref_current` ordered before the reachability guard | `commands.rs:155` then `:165` | closed |
| T-25-41 | Fetch failure fails soft, never propagates | `preflight.rs:332-343` — `.unwrap_or(false)`, falls through to a warning | closed |
| T-25-42 | Refusal message is path-free (no username leak) | `preflight.rs:395-403` `stale_base_message` — refs and commands only | closed |
| T-25-14-01 | Ref write is a compare-and-swap | `preflight.rs:425-468` `fast_forward_base_ref` passes `expected_old` to `git update-ref` | closed |
| T-25-14-02 | Checked-out predicate is repository-wide and fail-closed | `preflight.rs:425-468` `base_is_checked_out_anywhere`, `_ => true` on unreadable output | closed |
| T-25-15-02..07 | `doctor` read-only; safety set unioned not narrowed; one shared composition | `commands.rs:1072` (`prune_missing` only in `gate_sweep`), `:3090-3111` (`stray_safety_roots` unions), `:3173-3183` (`doctor` → filtered candidates only); twice-run read-only fixture test intact at `:5003-5024` | closed |
| T-25-16-01..07, T-25-17-01/02, T-25-18-01 | Test teardown signals only the pid the test spawned; escalation verified; double-panic abort avoided | `test_support.rs:364-386` (`after_launch` captures `monitor_pid` by value), `:389-414` (`Drop` uses `terminate_and_verify`, `std::thread::panicking()` interlock, direct stderr write not `eprintln!`) | closed |
| T-25-12-01..03 | Age floor refuses inside the exec-visibility window; fail-closed on unreadable `/proc/uptime` | `agent.rs:228-287`; `commands.rs:1350-1364` `TooYoung` refuses before any signal | closed |
| T-25-13-04 | Push-gate bypass not used | `git config --get core.hooksPath` → `scripts/hooks`, live-checked | closed |
| T-25-13-05, T-25-10-04 | Executor cannot self-sign a human gate | `25-13-SUMMARY.md:39,79,220-259` records a verbatim, dated, content-engaged human response | closed |

### Retroactive register — plan 25-19 (5/5 closed)

`25-19-PLAN.md` carried no `<threat_model>`. Built from its diff and verified.

| Threat ID | Category | Severity | Disposition | Mitigation Evidence | Status |
|-----------|----------|----------|-------------|---------------------|--------|
| T-25-19-01 | Tampering | high | mitigate | A stale or attacker-influenced state file cannot redirect the signal: `workflow::state_path` (`workflow.rs:126-128`) derives the path solely from `root`, which is a fresh `tempfile::tempdir()` created at test entry (`pipeline_launch.rs:460-461`) and first written by the same test at `:470`. No external writer can reach it between creation and the `resume()` readback. | closed |
| T-25-19-02 | Repudiation | medium | mitigate | Guard cannot silently no-op: `pipeline_launch.rs:520-525` asserts `reloaded.monitor_pid.is_some()` | closed |
| T-25-19-03 | Denial of Service | high | mitigate | `Drop` cannot abort the test binary mid-unwind: shared `Drop` at `test_support.rs:389-414` with the `std::thread::panicking()` interlock (fixed in `c2f5080`), reused unmodified | closed |
| T-25-19-04 | Tampering | high | mitigate | Signal cannot reach pid 0 or a wrapped pid: shared `terminate_and_verify` guard (`agent.rs:118-128`), reused unmodified | closed |
| T-25-19-05 | Tampering | medium | mitigate | No scope creep into production: `git show 02cb9ba --stat` — 21 lines added, entirely inside `#[cfg(test)] mod tests` | closed |

### Medium / low severity (53 rows, non-blocking under `block_on: high`)

All verified against their plans' stated rationale; none open. Representative accepted-risk rows
are enumerated in the Accepted Risks Log below.

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above `block_on` count toward `threats_open`*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

17 rows carry disposition `accept`. Each was declared at plan time with a rationale and confirmed
against the implementation during this audit. Representative entries:

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| R-25-01 | T-25-02-13 | SIGKILL after a bounded TERM wait is the intended escalation; an uncatchable signal is the point, and 999.44 measured 15/15 wrappers surviving TERM alone | Plan 25-02 (`<threat_model>`) | 2026-07-27 |
| R-25-02 | T-25-09-04 | `git merge-base --is-ancestor` spawn failure resolves `.unwrap_or(false)` — fail-closed toward refusing rather than mis-anchoring a release range | Plan 25-09 | 2026-07-28 |
| R-25-03 | T-25-11-02, T-25-14-04, T-25-15-08 | TOCTOU residuals: the reachability set and the checked-out scan are point-in-time reads. Bounded, documented in source, and backstopped by `is_same_process` re-confirmation, the `STRAY_MIN_AGE` floor, and the compare-and-swap respectively | Plans 25-11 / 25-14 / 25-15 | 2026-07-28 |
| R-25-04 | T-25-14 (scan-to-swap window) | A worktree checking out `<base>` between the repository-wide scan and the compare-and-swap is not covered by the scan. The CAS still prevents a lost update; the only alternative that closes it (`git branch -f`) reopens the higher-severity unconditional-write defect | Plan 25-14, recorded in `must_haves.truths` as a backstop | 2026-07-28 |
| R-25-05 | T-25-17-04/05 | No new dependency introduced; `eprintln!` residual superseded by the `c2f5080` fix | Plan 25-17 | 2026-07-28 |

Remaining `accept` rows are recorded in their originating plans' `<threat_model>` blocks and were
confirmed unchanged during this audit.

*Accepted risks do not resurface in future audit runs.*

---

## Register Provenance

`register_authored_at_plan_time: true`. 18 of 19 plans carry a parseable `<threat_model>` block:
25-01 through 25-18 (25-10 included, though that plan was superseded by 25-13 and never executed).

**Gap, disclosed rather than smoothed:** `25-19-PLAN.md` has no threat model. That plan was
authored by the orchestrating agent during gap-closure round 5 rather than by `gsd-planner`, and
the block was omitted. Because the short-circuit rule sets `register_authored_at_plan_time: true`
when *at least one* plan has a block, this omission would not have been caught by the rule itself —
it was found by inspection and covered by the retroactive register above. A future orchestrator
authoring a plan directly must carry the `<threat_model>` block forward.

Five SUMMARY files carry a `## Threat Flags` section (25-05, 25-06, 25-07, 25-08, 25-16); each
states no threats beyond those already registered in its plan. No unregistered flags were found.

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-07-28 | 129 | 129 | 0 | gsd-security-auditor (ASVS L1 presence verification, with L2/L3-depth tracing on the ref-write, `/proc`-census, signal-safety, version-derivation and test-teardown surfaces) |

Audit method: each plan's own `<threat_model>` was read (not merely the aggregate index), the
cited implementation file was read at the cited line, and for the 6 critical and 65 high rows the
auditor traced call order, fail-open/fail-closed polarity, and — where a regression test existed —
confirmed the test asserts the property rather than merely compiling. The auditor independently
re-ran `cargo test --workspace --no-fail-fast` (696 passed / 0 failed), `cargo fmt --all -- --check`
(clean) and `cargo clippy --workspace --all-targets -- -D warnings` (clean) rather than trusting
self-reported numbers in the SUMMARY files. No implementation file was modified.

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-07-28
