# Backlog migration to GitHub Issues — 2026-11-22

Operator decision 2026-11-22: GitHub Issues (`denniyahh/devflow`) is the only issue store. The `.planning/ROADMAP.md`
`## Backlog` section (243 `999.x` entries) and its 41 `.planning/phases/999.*` directories were removed; the last
commit holding them is 203061d on `workspace/denniyahh`. Code comments and planning docs that cite a `999.x` id
resolve through this table, or by searching GitHub issues for the `999.N:` title prefix.

Triage: entries whose heading already said RESOLVED, DELIVERED, PROMOTED, FOLDED, OBSOLETE, REMOVED, CLOSED,
DUPLICATE, DONE or HOTFIX were taken as closed. The 132 BACKLOG/DEFERRED entries were checked against the code at
203061d by seven read-only Sonnet subagents (verdict fixed / obsolete / relevant; unsure = relevant), with a
spot check of seven verdicts by the orchestrator. A supervisor item was dropped only when every part concerned code
removed in 4.0.0-alpha.1. Relevant entries became issues labelled `from-backlog`; the 17 that already had a GitHub
mirror kept it. Fixed or obsolete GitHub issues were closed with a comment (completed / not planned), not deleted.

GitHub-only issues closed: #148 (not planned), #153 (not planned), #158 (not planned), #161 (completed), #164 (completed), #166 (completed), #168 (completed), #170 (completed), #174 (completed), #179 (completed), #180 (completed), #181 (completed), #185 (not planned), #187 (not planned), #197 (completed), #204 (completed), #206 (completed), #209 (completed).
Left open without a backlog entry to check: #182, #183, #184 (their 999.x entries were already gone).

| Entry | Title | Verdict | GitHub | Reason |
|---|---|---|---|---|
| 999.1 | Hermes Support (DELIVERED — Phase 42 / HRMS-01..03) | delivered | — | heading status DELIVERED |
| 999.2 | A Phase Tracks Exactly One Process (DELIVERED — Phase 21 / 21c) | delivered | — | heading status DELIVERED |
| 999.3 | CLI Operator Discoverability (DELIVERED — Phase 21 / 21a) | delivered | — | heading status DELIVERED |
| 999.4 | Version-Tag Contention on Concurrent Ship (REMOVED — 2026-07-29) | removed | — | heading status REMOVED |
| 999.5 | ChangelogAppend Placeholder Content (PROMOTED — Phase 26) | promoted | — | heading status PROMOTED |
| 999.9 | Dependency Update Review (RESOLVED 2026-11-05 — actions/checkout@v7 on all 7 uses; rust-toolchain.toml pinned to 1.97.1; no floating dtolnay | resolved | #197 | heading status RESOLVED |
| 999.12 | Layer 0 Unapproved-Probe Veto Coverage (BACKLOG) | relevant | #196 | Only the "not approved" message arm exists at agent_result.rs:2927 and I found no test naming it; the configuration-guide inheritance note was also not found. |
| 999.14 | Doctor Reconciliation for Planning-Doc Staleness (DELIVERED — Phase 21 / 21b) | delivered | — | heading status DELIVERED |
| 999.15 | Hermetic Tests for Shell Entry Points (BACKLOG) | relevant | #195 | No behavioral tests for install.sh, sync-main-to-develop.sh or deploy; scripts/deploy-docs.sh is the current name of deploy.sh. GitHub #195 already tracks it. |
| 999.17 | Mutation Testing (`cargo-mutants`) (BACKLOG) | relevant | #194 | No cargo-mutants gate exists. |
| 999.18 | Property and Fuzz Testing for Protocol Parsers (BACKLOG) | relevant | #193 | Only one proptest block exists (pipeline_outcomes.rs) and there is no fuzz target; GitHub #193 already tracks it. |
| 999.19 | Fast and Slow Validation Lanes (BACKLOG) | relevant | #192 | CI has no slow/scheduled lane for build_provenance, mutation or fuzz; only scheduled-deps.yml is scheduled. GitHub #192 already tracks it. |
| 999.20 | Differential Coverage Enforcement (BACKLOG) | relevant | #191 | No differential coverage enforcement in scripts or CI. |
| 999.21 | Mechanical Lint Enforcement for AI Test Anti-Patterns (BACKLOG) | relevant | #190 | No mechanical anti-pattern lint exists and the referenced .claude/skills directory is absent. |
| 999.22 | Refactor Equivalence Guard in CI (BACKLOG) | relevant | #189 | No equivalence guard exists in CI. |
| 999.25 | Release-Cut Executor (`devflow release` that executes) (CLOSED — WON'T DO, superseded by Phase 30) | closed | — | heading status CLOSED |
| 999.26 | `devflow parallel` Git Object-Store Race (OBSOLETE with Phase 52 CUT-04 — `devflow parallel` removed) | obsolete | — | heading status OBSOLETE |
| 999.27 | `release --check` Signing-Key Inline Classification (PROMOTED — Phase 24) | promoted | — | heading status PROMOTED |
| 999.28 | Explicit `--base` Branch Override for `devflow start` (BACKLOG) | relevant | #186 | devflow start has no --base option and no explicit base-branch override flag exists; ship/parallel dependencies are gone but stacking on an unmerged predecessor is still unsupported. |
| 999.29 | Dogfood Staleness Guard False-Positives on Docs-Only Commits (DELIVERED — Phase 21 / 21d) | delivered | — | heading status DELIVERED |
| 999.30 | Phase 21 Code-Review Cleanup — gate_show/doctor DRY + TOCTOU (DELIVERED — Phase 22 / 22-01, 22-02) | delivered | — | heading status DELIVERED |
| 999.31 | Modular Agent Driver Architecture (DELIVERED — Phase 37 / AGDR-01..04) | delivered | — | heading status DELIVERED |
| 999.37 | Test Suite Escaped Its Sandbox and Corrupted the Real Repo (RESOLVED — this release) | resolved | — | heading status RESOLVED |
| 999.38 | Test-Suite PATH Race Between ENV_MUTEX Mutators and Concurrent Git Callers (PROMOTED — Phase 48 test-suite isolation, 2026-09-13) | promoted | #181 | heading status PROMOTED |
| 999.39 | Production Git Calls Inherit a Redirecting Environment (DONE — Phase 27, 2026-07-30) | done | — | heading status DONE |
| 999.43 | `devflow cleanup` Deletes Recovery Refs by Ancestry (BACKLOG) | relevant | #177 | Cleanup still deletes merged branches by ancestry with no exclusion for recovery refs. |
| 999.44 | State-Orphaned Processes Are Unreachable by `stop` and `gate sweep` (PROMOTED — Phase 25) | promoted | — | heading status PROMOTED |
| 999.45 | Registry Emits Duplicate Entries for a Single Root (BACKLOG) | fixed | #179 | Registry entries are one file per (root, phase), so a root cannot be listed twice. |
| 999.46 | E2E Suites Leak Scratch Roots; Process Leak Does Not Reproduce on a Clean Run (BACKLOG) | relevant | #178 | E2E suites still spawn child processes and the cleanup claim is unverified; the entry itself says the leak does not reproduce, and the `devflow advance` it names no longer appears in the suites. |
| 999.47 | `looks_like_devflow_process` False-Positives Under CI Load (PROMOTED — Phase 25) | promoted | — | heading status PROMOTED |
| 999.48 | Driving Binary Is Re-Checked for Staleness Mid-Run, Halting Self-Modifying Phases (PROMOTED — Phase 25) | promoted | — | heading status PROMOTED |
| 999.49 | `compute_version` Derives a Nonsensical Version from Tag Count and Describe Distance (PROMOTED — Phase 25) | promoted | — | heading status PROMOTED |
| 999.50 | `release --check` Tag-Signing Gate False-Negatives When the Key Is Not in `ssh-agent` (REMOVED — 2026-07-29) | removed | — | heading status REMOVED |
| 999.51 | `devflow start` Resolves Its Base From a Possibly-Stale Local `develop`, and Never Fetches (PROMOTED — Phase 25) | promoted | — | heading status PROMOTED |
| 999.52 | DevFlow Imposes a Branch Model Whose Required Repair Step It Does Not Ship (PROMOTED — Phase 26) | promoted | — | heading status PROMOTED |
| 999.53 | CI Cannot Reproduce the Load Shape That Catches Exec-Visibility Races (RESOLVED 2026-11-04 — the `Sequential 2-CPU check` CI job) | resolved | — | heading status RESOLVED |
| 999.54 | `release --check` Tag-Signing Viability Reads the Wrong Config Key (REMOVED — 2026-07-29) | removed | — | heading status REMOVED |
| 999.55 | `phase7_cli::wait_for` Fixed 5s Budget Times Out Under Load (BACKLOG) | relevant | #301 | wait_for still polls a fixed 200 x 25ms (5s) budget with no scaling. |
| 999.56 | macOS CI Coverage Without Breaking Required Checks (BACKLOG) | relevant | #316 | CI has no macOS runner. |
| 999.57 | An Operator's Checkpoint Answer Can Never Reach the Agent (PROMOTED — Phase 28, parts A+C; part B deferred) | promoted | — | heading status PROMOTED |
| 999.58 | The Pre-Push Container Gate Cannot Run From a Linked Worktree (RESOLVED 2026-11-04 — `check-in-container.sh` mounts the worktree and common  | resolved | — | heading status RESOLVED |
| 999.59 | A Missing CONTEXT.md Is Ambiguous, So Define Attempts an Interview It Cannot Conduct (PROMOTED — Phase 28, unit 28c) | promoted | — | heading status PROMOTED |
| 999.60 | `resume` Discards an `--until` Cap That Never Fired, and Cannot Re-Specify One (PROMOTED — Phase 28, unit 28d) | promoted | — | heading status PROMOTED |
| 999.61 | Four Indirect Git-Reaching Spawn Edges a Literal Grep Cannot See (BACKLOG) | relevant | #252 | Two of the three remaining indirect git-reaching spawn sites still inherit the git env; the hooks.rs site is gone. |
| 999.63 | The Phase-Reachability Guard Demands Define's Own Output as Define's Precondition (DELIVERED — patch, 2026-07-31) | delivered | — | heading status DELIVERED |
| 999.64 | One-Shot Launch Kills the Session at Turn End, Orphaning Any Delegated Work (PROMOTED — Phase 30) | promoted | — | heading status PROMOTED |
| 999.65 | The Validate→Code Loop-Back Issues an Impossible Command on a Mid-Arc Phase (RESOLVED — shipped; verified 2026-09-18) | resolved | — | heading status RESOLVED |
| 999.66 | `consecutive_failures` Accumulates on Healthy Multi-Wave Progress, Not Just Repeated Failure (RESOLVED — shipped; verified 2026-09-18) | resolved | — | heading status RESOLVED |
| 999.67 | `parse_devflow_result` Lets an Agent Plant Its Own Layer-0 Provenance (RESOLVED — shipped; verified 2026-08-15) | resolved | — | heading status RESOLVED |
| 999.68 | Planning-Artifact Architecture — Requirements, a Constitution, and the Linear Boundary (BACKLOG) | relevant | #172 | Planning-artifact gap (dormant requirements machinery, no constitution) is a GSD/process question that was not resolved; the Linear half is moot but already mirrored as GitHub #172. |
| 999.69 | Re-Publish the Three Committed `30a-evidence` Captures Through the Redaction Pipeline (BACKLOG) | relevant | #171 | The three 30a-evidence captures are still committed and still contain home-path strings. |
| 999.70 | Checkpoint Detection Cannot Tell a Gate DECLARATION From a Gate MENTION (BACKLOG) | fixed | #170 | Gate detection no longer scans stream text; it uses the structured checkpoint field on a failed result, so a documentary mention cannot trip it. |
| 999.71 | Measure Whether the Capture Writer Actually Leaves Torn Terminal Lines (BACKLOG — Phase 51 takes only the rate-limit classifier's torn-line  | relevant | #169 | The measurement of torn capture lines is still open; torn handling exists in the readers but nothing measures production frequency. |
| 999.72 | ROADMAP.md Layout Hides Every Phase From `gsd-tools`' Milestone-Scoped Parsers (RESOLVED — 2026-08-04, Phase 32) | resolved | — | heading status RESOLVED |
| 999.73 | Widen `STREAM_JSON_STAGES` Beyond `Stage::Code` (PROMOTED — Phase 34) | promoted | — | heading status PROMOTED |
| 999.74 | `classify_validate_outcome` Trusts the Agent's Verdict Over Its Own Status (PROMOTED — Phase 34) | promoted | — | heading status PROMOTED |
| 999.75 | `CloseRule` Treats an Unparseable `tasks` List on the FIRST Announcement as Permission to Close (RESOLVED 2026-08-04) | resolved | — | heading status RESOLVED |
| 999.76 | Layer 0 External Verification Reads Its Declaration From the Main Checkout, So It Is Inert in Worktree Mode (PROMOTED — Phase 34) | promoted | — | heading status PROMOTED |
| 999.77 | A Single Transient `git` Failure Grants a Free `consecutive_failures` Reset, and the Doc Comment Promises the Opposite (PROMOTED — Phase 35) | promoted | — | heading status PROMOTED |
| 999.78 | The Code↔Validate Loop Has No Progress-Independent Bound, and the Gate Message Understates How Long It Has Run (PROMOTED — Phase 35) | promoted | — | heading status PROMOTED |
| 999.79 | `{N}-VERIFICATION.md` Never Goes Stale, So a `--force` Re-Run Inherits the Previous Run's Verdict and Gates Unresolvably (PROMOTED — Phase 3 | promoted | — | heading status PROMOTED |
| 999.80 | Three Test Sites Are Protected From Spawning a Real Agent Only by a Content-Dependent Gate Response, Not Structurally (RESOLVED 2026-11-04 — | resolved | #168 | heading status RESOLVED |
| 999.81 | Phase 33 Advisory Cleanup — the Loop-Back Prompt Calls a Normal Continuation a Defect, Plus Three Hygiene Items (BACKLOG) | relevant | #167 | IN-05 persists: GapsOnly and FullExecute still share the "Validation reported issues" preamble. |
| 999.82 | Re-File 31/D-14 — Per-Child Declared Tokens, Deferred on Size for the Second Time (BACKLOG) | relevant | #165 | Only top-level total_cost_usd is read; per-child token attribution was never built. |
| 999.83 | The Drain Gate Never Saw 8 Concurrent Sub-Agents — Its Fixture's Shape Is Not What Production Emits (RESOLVED 2026-11-05 — monitor.rs docume | resolved | #166 | heading status RESOLVED |
| 999.84 | Nothing Guards the Root Argument at the `GateReview` Checkpoint Call Site, So 999.76's Fix Can Regress Silently (PROMOTED — Phase 35) | promoted | — | heading status PROMOTED |
| 999.85 | Two Protected Comments Now Justify Themselves by a Mechanism Phase 34 Deleted (RESOLVED 2026-11-04 — Phase 40 MAINT-01, f07ab99) | resolved | — | heading status RESOLVED |
| 999.86 | Replace `release --check`'s Tag-Signing Predictor With a Direct Probe (PROMOTED — Phase 35, revises D-10 for `release --check` only) | promoted | — | heading status PROMOTED |
| 999.87 | `evaluate_layer2` Reads an Unrunnable `git` as "No Work Done", Misclassifying a Successful Agent as `Failed` (PROMOTED — Phase 35) | promoted | — | heading status PROMOTED |
| 999.88 | The `setsid` Guard on the Tag-Signing Probe Has No Regression Test (RESOLVED 2026-08-07) | resolved | — | heading status RESOLVED |
| 999.89 | The Verification-Freshness Rule Infers Provenance From Bytes, and Is Only Safe Because Two Branch-Naming Conventions Happen to Agree (BACKLO | fixed | #164 | The content-change predicate verification_authored_this_run no longer exists. |
| 999.90 | `handle_validate_outcome` Counts Commits Against `GitFlowConfig::default()`, Not the Project's Configured Git-Flow (RESOLVED — shipped; veri | resolved | — | heading status RESOLVED |
| 999.91 | Add a Doc Gate to `check.sh` That Denies Only the Rustdoc Lints Which Catch Real Errors (BACKLOG) | relevant | #162 | No cargo doc gate in check.sh or CI. |
| 999.92 | The 999.47 Regression Test Loses Its Own Fixture Shape Before It Asserts — Flaky, and Weak When Green (RESOLVED 2026-11-04 — trap-held shell | resolved | #161 | heading status RESOLVED |
| 999.93 | Unattended Preflight Policy & Validation Completeness Audit (BACKLOG) | relevant | #298 | The completeness audit of preflight.rs has not been done; the item is still listed. |
| 999.94 | An Unattended `decision` Checkpoint Takes the First Option Without Reading It (RESOLVED 2026-11-05 — prompt.rs tells Code/Validate to ignore | resolved | — | heading status RESOLVED |
| 999.95 | `cargo doc` Is Not in This Repo's Definition of Green, and Carries a 33-Warning Baseline (DUPLICATE of 999.91) | duplicate | — | heading status DUPLICATE |
| 999.96 | `release --check` Cannot Catch a Forgotten Version Bump (RESOLVED 2026-11-04 — `check_changelog_version` in `release --check`) | resolved | — | heading status RESOLVED |
| 999.97 | DevFlow Cannot Launch Any Phase GSD Numbers With a Decimal (HOTFIX — 2026-08-07) | hotfix | — | heading status HOTFIX |
| 999.98 | The Idle-Timeout Verdict Has No Lifetime, So One Attempt's Death Certificate Condemns Every Later Run of That Phase (PROMOTED — Phase 35.1) | promoted | — | heading status PROMOTED |
| 999.99 | The Idle Timeout Kills a Stage That Backgrounds Work, Because the Drain Gate Reads an Event the Plan Stage Never Emits (PROMOTED — Phase 35. | promoted | — | heading status PROMOTED |
| 999.100 | A Quota Denial Is Recorded as an Idle Timeout, Turning a Resumable Pause Into a Terminal Verdict (PROMOTED — Phase 35.1) | promoted | — | heading status PROMOTED |
| 999.101 | The Task-Notification Turn Errors Under Parallel-Executor Load, Aborting Its Tools (BACKLOG — upstream Claude Code, not DevFlow) | relevant | #264 | Upstream Claude Code task-notification turn error; nothing DevFlow-side removes it. |
| 999.102 | The Idle-Timeout Verdict Discards Diagnostics That Are Already Sitting in the Capture (PROMOTED — Phase 51, operator 2026-11-05) | promoted | — | heading status PROMOTED |
| 999.103 | The Gate Notify Hook Has No Home, No Default, and No Way to Tell You It Is Broken (BACKLOG) | relevant | #262 | The notify hook is still driven by an env var/state field with no checked default or config-file home I could confirm. |
| 999.104 | Fix `release --check` Signing-Key Probe Target & Fingerprint Validation (RESOLVED 2026-11-04 — the signing-key probe was removed, not retarg | resolved | — | heading status RESOLVED |
| 999.105 | Make Adversarial Cross-Model Review of CONTEXT, PLAN and the Executed Diff a Default Phase Gate (BACKLOG) | relevant | #289 | No default cross-model review gate exists in the pipeline; review is still operator-run per CLAUDE.md. |
| 999.106 | Remove `AgentAdapter` and the Legacy DriverShim (DEFERRED from Phase 37) | fixed | — | AgentAdapter, DriverShim and the legacy adapter structs no longer exist. |
| 999.107 | Two Pre-Existing Codex-Parser Defects Surfaced by the Phase 37 Code Review (RESOLVED 2026-11-04 — both Codex-parser defects fixed) | resolved | — | heading status RESOLVED |
| 999.108 | GSD Subagent Dispatch Is Unavailable From the Pi Runtime (CLOSED 2026-11-05 — upstream GSD limitation, not DevFlow code) | closed | — | heading status CLOSED |
| 999.109 | Self-Dogfood Staleness Check's Build-Relevance Heuristic Matches Any `.rs`/`Cargo.toml` Repo-Wide, Not Just Workspace Members (RESOLVED — sh | resolved | — | heading status RESOLVED |
| 999.110 | `devflow init` + `devflow doctor` — Make the `.planning/`-Tracking Base Branch an Explicit, Checked Setup Step (BACKLOG) | relevant | #261 | No `devflow init` verb exists and doctor does not check the planning base branch. |
| 999.111 | `devflow` Never Detects or Runs the Security Verdict Before Ship — Discovers the Gap Only at Ship Time (OBSOLETE with Phase 52 CUT-02 — Ship | obsolete | — | heading status OBSOLETE |
| 999.112 | `OpenCodeDriver::capabilities()` Never ANSI-Strips `opencode agent list` Output Before Matching (BACKLOG) | relevant | #290 | opencode_subagent_dispatch_available_with still parses agent list stdout without strip_ansi_escapes. |
| 999.113 | `consume_cron_instructions_preserves_unreadable_legacy_record` Passes Silently Under Root, Defeating Its Own Fixture (RESOLVED 2026-08-27) | resolved | — | heading status RESOLVED |
| 999.114 | Test Binary Blanks Process-Global `PATH`, Failing Whichever Concurrent Test Happens to Spawn a Program (RESOLVED 2026-11-04 — Phase 46-07 de | resolved | — | heading status RESOLVED |
| 999.115 | The Unattended Decision Policy Never Reaches the Claude Loop-Back Code Prompt (RESOLVED 2026-11-04 — Phase 47) | resolved | — | heading status RESOLVED |
| 999.116 | `CODE_STAGE_POLICY` Contradicts `checkpoint_auto_decide_prompt` in the Same Resumed Session (RESOLVED 2026-11-04 — Phase 47-03) | resolved | — | heading status RESOLVED |
| 999.117 | Scoped Staleness Accepts Any `crates/**` Path Rather Than the Declared Workspace Members (BACKLOG) | relevant | #254 | affects_compiled_binary scoped branch still accepts any crates/ path; entry says deliberately not fixed, promote only if premise changes. |
| 999.118 | `write_state_atomic` Uses a Fixed `.tmp` Filename, So Two Processes Saving the Same Phase Race (RESOLVED 2026-11-04 — unique temp names in ` | resolved | — | heading status RESOLVED |
| 999.119 | Live `devflow start --mode auto` End-to-End Verification of the Configured-Base Fork/Preflight/Merge Chain (RESOLVED 2026-10-09 — Phase 49 V | resolved | — | heading status RESOLVED |
| 999.120 | One `git_flow_for_project` Ambient Re-Resolution Survives in the Validate Loop-Back Path (BACKLOG) | relevant | #304 | The Validate path still calls git_flow_for_project(project_root) instead of the persisted state.base_branch. |
| 999.121 | OpenCode Has No CLI-Level `marker-less run never advances` Regression Test (BACKLOG) | relevant | #266 | Only codex gained a CLI-level marker-less test; claude and opencode still have none. |
| 999.122 | Adopt `cargo nextest` for Per-Test Process Isolation (BACKLOG) | relevant | #314 | nextest is used only for the service_e2e check; the full suite still runs shared-process tests with 68 ENV_MUTEX references and no .config/nextest.toml. |
| 999.123 | Every Commit Triggers FOUR CI Runs, and the Contention Manufactures Flakes (BACKLOG) | relevant | #263 | ci.yml and devcontainer.yml still declare unfiltered `push:` plus pull_request. |
| 999.124 | GSD `phase complete` Leaves the Planning Docs Half-Updated on Every Phase Close (BACKLOG) | relevant | #275 | GSD phase complete behaviour is upstream tooling that could not be re-run here (gsd-tools not on PATH); DevFlow now runs phase.complete itself but the doc-layout misses are GSD-side. |
| 999.125 | Preflight and Resume Disagree on Where a `blocking-human` Gate Exists (PROMOTED — Phase 48, 2026-09-13) | promoted | — | heading status PROMOTED |
| 999.126 | A Checkpoint Added After Code Preflight Is Never Re-Scanned (PROMOTED — Phase 48, 2026-09-13) | promoted | — | heading status PROMOTED |
| 999.127 | A `human-action` Gate Can Be Auto-Decided Because Another Checkpoint Is `blocking-human` (BACKLOG) | fixed | — | The checkpoint auto-decision path is gone; unattended launch now refuses a phase with any human-only checkpoint. |
| 999.128 | Lock Reclaim Ignores the Holder's Start Time (RESOLVED 2026-11-04 — Phase 50.1, `lock.rs` reclaims a recycled holder) | resolved | — | heading status RESOLVED |
| 999.129 | Ship Finalization-Retry Gate Recovery Is Unverified (OBSOLETE with Phase 52 CUT-02 — Ship finalization removed) | obsolete | — | heading status OBSOLETE |
| 999.130 | Split-Lock State-Handling Overhaul (PROMOTED — Phase 50.1) | promoted | — | heading status PROMOTED |
| 999.131 | Convert the Remaining Test Environment Mutations (BACKLOG) | relevant | #286 | ENV_MUTEX and process-global env set_var/remove_var in tests remain. |
| 999.132 | Gate List and Status Disagree After an Unwaited Ship Answer (OBSOLETE with Phase 52 CUT-02 — Ship gate removed) | obsolete | — | heading status OBSOLETE |
| 999.133 | Commands Report Success After a Failed or No-Op Action (BACKLOG) | fixed | — | abort and cleanup now report failures and recover is removed. |
| 999.134 | PipeOwning Monitor Orphans Its Agent on TERM (RESOLVED 2026-11-04 — Phase 50, the supervisor handles TERM and INT) | resolved | — | heading status RESOLVED |
| 999.135 | Probe Descendant Survives the Group Kill Under Full-Gate Load (RESOLVED 2026-11-04 — 0268976 fixed the zero-poll signature; the original 2 s | resolved | — | heading status RESOLVED |
| 999.136 | Recover Sweep Deletes a Live Run Between Agent Exit and Advance (RESOLVED 2026-11-04 — Phase 50.1, `recover` rechecks liveness under the sta | resolved | — | heading status RESOLVED |
| 999.137 | Recover Sweep Does Not Reach Every Artifact It Resets (BACKLOG) | relevant | #291 | R-2 went with cron; R-7 (state temp orphan) and R-8 (phases_on_disk accepts any NN-* name) remain in the sweep code. |
| 999.138 | `devflow stop` Cannot End a Run That Holds No Lock (RESOLVED 2026-11-04 — Phase 50, `stop` goes through the socket) | resolved | — | heading status RESOLVED |
| 999.139 | A State File That Will Not Load Hides a Live Monitor From `start`'s Guard (RESOLVED 2026-11-04 — Phase 50.1, ef856e6) | resolved | — | heading status RESOLVED |
| 999.140 | `resume` Launches With No Live-Run Check (PROMOTED — Phase 48 gap closure) | promoted | — | heading status PROMOTED |
| 999.141 | E2E Tests Write Real `~/.cache/devflow/roots` and Leak Monitors (RESOLVED 2026-11-04 — test home isolation guard) | resolved | — | heading status RESOLVED |
| 999.142 | Hints Prescribe `devflow resume` Where the Live-Run Guard Refuses It (RESOLVED 2026-11-04 — Phase 50.1 criterion 2) | resolved | — | heading status RESOLVED |
| 999.143 | A Recycled Recorded Pid Leaves `resume` No Work-Preserving Way Out (BACKLOG) | obsolete | — | The recorded-pid liveness and the recover --clean/start refusal path it describes were replaced by runner-lock liveness and recover was removed. |
| 999.144 | No Test Pins `resume`'s Live-Run Guard Ahead of Its Early Writes (BACKLOG) | relevant | #293 | The guard is now refuse_a_live_runner in commands.rs; no test was found that pins its order ahead of resume writes, so unverified and kept. |
| 999.145 | `spawn_with_timeout_kills_a_silent_same_group_descendant_after_parent_exit` Flakes in the Container Gate (RESOLVED 2026-11-04 — 0268976, sam | resolved | — | heading status RESOLVED |
| 999.146 | `cut-pr-branch.sh`'s Default PR Branch Name Drops the First Hyphenated Word (BACKLOG) | relevant | #307 | The first-hyphen strip is still present. |
| 999.147 | `sanitize-evidence.py`'s Email Redaction Swallows a JSON Escape Letter and Leaves Invalid JSON (BACKLOG) | obsolete | — | sanitize-evidence.py is a one-off Phase 49 evidence tool that lives only in the archived v3.0.0 milestone dir and is not used by any current script. |
| 999.148 | Collected Evidence Carries DevFlow's `.gitignore` (`*`), So a Plain `git add` Commits None of the DevFlow Copy (BACKLOG) | obsolete | — | collect-attempt.sh was a Phase 49 live-run evidence collector, now only in the archived v3.0.0 milestone dir. |
| 999.149 | `check-evidence.sh` Labels a Repeated Stage Gate `unexpected=false`, So an Extra Code Gate Prints No Deviation (BACKLOG) | obsolete | — | check-evidence.sh exists only as a one-off evidence tool of the archived v3.0.0 Phase 49 run. |
| 999.150 | Archived Stage Captures Do Not Keep the Prompt Each Turn Received (BACKLOG) | relevant | #320 | history.rs has no prompt archival that I could find; I did not read the capture-writing code, so treating as still open. |
| 999.151 | `probe_rejects_a_setpgid_pipe_holder_before_parent_exit` Fails Intermittently in the Full Suite (BACKLOG) | relevant | #265 | The test still exists with the same reaped-assertion and no clean-run history was checked. |
| 999.152 | Phase 49 Live-Run Tooling Robustness Findings, to Fix Before the Tools Are Reused (BACKLOG) | obsolete | — | The Phase 49 live-run tools directory no longer exists; phase 49 was archived with the v3.0.0 milestone. |
| 999.153 | The Test Suite Registers Temp-Dir Roots in the Developer's Real `~/.cache/devflow` (DUPLICATE of 999.141) | duplicate | — | heading status DUPLICATE |
| 999.154 | Gate Notify Command — Remaining Gaps After the Phase 49 State-File Design (PROMOTED — Phase 51, operator 2026-11-05) | promoted | — | heading status PROMOTED |
| 999.155 | A Halted Checkpoint Can Advance Past Code When the Last Turn Has No Result Marker (BACKLOG) | relevant | #292 | decide_action still keys on status only; a Success with no result marker after a halted checkpoint is not caught (D-13 covers only Failed results). |
| 999.156 | A SIGKILL Between the Agent's Spawn and Its Sidecar Write Orphans the Agent (BACKLOG) | obsolete | — | The sidecar-write window belonged to the removed detached supervisor; nothing now spawns an agent with a sidecar. [supervisor] |
| 999.157 | A Natural Agent Exit Leaves the Agent's Background Processes Running (DUPLICATE of 999.164 item 5) | duplicate | — | heading status DUPLICATE |
| 999.158 | Process Identity and Group Enumeration Do Not Work Off Linux (BACKLOG) | relevant | #282 | The STALE backstop it cites is gone, but process_start_time and other /proc reads that survive still return None off Linux. [supervisor] |
| 999.159 | Reboot Detection Does Not Work Off Linux (BACKLOG) | relevant | #284 | The /proc boot_id read survives in liveness and has no non-Linux source. |
| 999.160 | Socket Supervision Has Never Run on macOS (BACKLOG) | obsolete | — | Socket supervision is removed, so there is nothing left to verify on macOS; the general macOS-unverified note is separate. [supervisor] |
| 999.161 | Move Plan-Check Shell Tools to a Rust Tool Crate (BACKLOG) | relevant | #256 | lint-plan-bashisms.sh is still a bespoke awk script and the move to a Rust tool crate was never done, though most of the named Phase 50 tools are gone. |
| 999.162 | A Gate Response Written During Cleanup Can Outlive Its Gate (RESOLVED 2026-11-04 — Phase 50.1 gate-id keying) | resolved | — | heading status RESOLVED |
| 999.163 | GSD's Codex Adapter Ignores `workflow.subagent_timeout` and Gives the Stall Watcher No Output File (BACKLOG) | relevant | #283 | It is an upstream gsd-core Codex adapter gap with no DevFlow-side change, still unverified as filed upstream. |
| 999.164 | Supervisor Residuals From the Phase 50 External Review (BACKLOG) | relevant | #317 | Items 1-4 concern the removed supervisor socket, doctor orphan list and control thread, but item 5 (only the agent leader is reaped on normal completion, leaving group members) concerns surviving stop/monitor code. [supervisor] |
| 999.165 | A State File's `gate_notify_cmd` Runs as a Shell Command (BACKLOG) | relevant | #321 | gate_notify_cmd is still persisted in state and run via sh -c. |
| 999.166 | Supervisor Residuals From the Phase 50 Code Review (FOLDED INTO Phase 50.1) | folded | — | heading status FOLDED |
| 999.167 | `cut-pr-branch.sh`'s Phase Scope Filter Drops `(NN-review)` Commits (BACKLOG) | fixed | — | cut-pr-branch.sh SCOPE_RE now accepts -review and -fix suffixes. |
| 999.168 | Adopt wsx for Repo Operations (BACKLOG) | relevant | #318 | Adopting wsx for repo operations is a pending cross-repo integration, not delivered. |
| 999.169 | The Plan Bashism Linter Cuts Its Report Off Under a Long Checkout Path (BACKLOG) | relevant | #295 | lint-plan-bashisms.sh still cuts each full line (path included) to 200 chars. |
| 999.170 | `stop` Cannot End an Orphaned Agent Whose Supervisor Was Killed (BACKLOG) | relevant | #319 | The supervisor is removed, but stop being unable to end a live agent group after its owner (now the runner) dies is a stop/liveness concern in surviving code; not reproduced. [supervisor] |
| 999.171 | `agent_running` Can Report a Just-Reaped Process as Running (RESOLVED 2026-11-04 — 0268976) | resolved | — | heading status RESOLVED |
| 999.172 | A `stopped` Flag Written by Hand During a Gate Wait Does Not Stop the Run (BACKLOG) | relevant | #269 | The supervisor part is gone, but stop and gates survive and whether a hand-written `stopped` flag halts a gate-parked run is unchecked. [supervisor] |
| 999.173 | A Ship Retry After a Restart Can Still Bump the Version Twice (OBSOLETE with Phase 52 CUT-02 — after-ship hooks removed) | obsolete | — | heading status OBSOLETE |
| 999.174 | `devflow stop` at an Open Gate Deletes State, While a Signal Keeps It Resumable (BACKLOG) | relevant | #312 | Gate abort semantics in stop (abort ends the run) still exist; not re-tested whether stop still deletes state at an open gate. |
| 999.175 | An Approved Auto-Decided Checkpoint Can Still Park an Unattended Run at Ship (FOLDED INTO Phase 53 — ASK-05; its Ship refusal goes with CUT- | folded | — | heading status FOLDED |
| 999.176 | Validate's Mutation Audit Has No Stopping Point on a Run With No Operator to Answer Its Gap Gate (BACKLOG) | relevant | #278 | No bound on the Validate gap loop for unattended runs was found; only the infra-failure ceiling exists. |
| 999.177 | `gate approve` Can Report a Failure for an Approval That Was Applied (BACKLOG) | relevant | #270 | gate approve can still report "published after that gate closed" and tests still tolerate it. |
| 999.178 | A `yes_ship` Revoke Written During the Ship Advance Is Ignored (OBSOLETE with Phase 52 CUT-02 — `yes_ship` removed) | obsolete | — | heading status OBSOLETE |
| 999.179 | A Launch Whose Successor Already Ended Still Reports "launched" (BACKLOG) | obsolete | — | The launch path it describes (publish_record, SuccessorAlreadyEnded, successor supervisor) was removed with the detached supervisor. [supervisor] |
| 999.180 | An Approved Validate Gate Is Not Closed Before Ship (FOLDED INTO Phase 52 — CUT-02, Ship removed) | folded | — | heading status FOLDED |
| 999.181 | A `devflow stop` Signal to `devflow parallel` Also Stops Phases It Launches Later (OBSOLETE with Phase 52 CUT-04 — `devflow parallel` remove | obsolete | — | heading status OBSOLETE |
| 999.182 | `stop`'s "Not Marked" Message Can Name the Wrong Pid (BACKLOG) | obsolete | — | holder_still_running, the supervisor-era stop message path it names, no longer exists. [supervisor] |
| 999.183 | `start --dry-run` Says Define Runs `/gsd-discuss-phase`, Which It Never Does (FOLDED INTO Phase 52 — CUT-01, Define removed) | folded | — | heading status FOLDED |
| 999.184 | An Ordinary Plan Success Over Uncommitted Plan Files Still Advances to Code (BACKLOG) | relevant | #305 | Plan success still transitions to Code with no check that PLAN.md files are tracked and unmodified. |
| 999.185 | A Superseded Supervisor Can Still Write the Rate-Limit Cron File (FOLDED INTO Phase 52 — CUT-03, cron file removed) | folded | — | heading status FOLDED |
| 999.186 | Nothing Acts on the Rate-Limit Cron Instructions, So a Paused Run Never Resumes (FOLDED INTO Phase 52 — CUT-03, cron file removed) | folded | — | heading status FOLDED |
| 999.187 | An In-Supervisor Launch Failure Outside Failover Ends the Run With No Gate and No Cause (BACKLOG) | relevant | #315 | The supervisor is gone, but the launch-failure-outside-failover path (handle_ambiguous_outcome -> launch_stage) still exists and I did not verify the runner now surfaces a gate or event for it. [supervisor] |
| 999.188 | The Held-State-Write-Lock Status Test Fails Intermittently Under the Full Suite (BACKLOG) | relevant | #310 | The test still exists and its flake was never root-caused. |
| 999.189 | A Gate Rejection's Note Never Reaches the Fix Pass, and Any Note Containing "abort" Aborts the Run (RESOLVED 2026-11-20 — fixed before Phase | resolved | — | heading status RESOLVED |
| 999.190 | `devflow status` Never Shows Why a Stopped Run Stopped (BACKLOG) | fixed | — | devflow status now prints the recorded stop_reason for a stopped run. |
| 999.191 | `stop_ends_a_launch_its_running_launcher_promotes_during_the_wait` Fails Under Full-Suite Load (BACKLOG) | obsolete | — | The test file pending_supervisor_e2e.rs and the pending-supervisor/supervisor socket it exercised were removed. [supervisor] |
| 999.192 | A Verification Report Written During Code Is Read as Inherited, So a Validate Reject Re-Runs the Whole Phase (BACKLOG) | obsolete | — | The stamp/fingerprint mechanism the entry blames was removed in the Phase 52 gate redesign, so the described inherited-report misread cannot occur as written. |
| 999.193 | The Validate Gate Says "Validation Passed — Approve to Ship?" While the Phase's Verification Reports `gaps_found` (RESOLVED 2026-11-20 — fix | resolved | — | heading status RESOLVED |
| 999.194 | After the Delivery Canary Refuses an Agent, Nothing Can Re-Run It for the Same Agent (BACKLOG) | relevant | #322 | No same-agent canary recheck route exists; only hand_off_agent clears the canary. |
| 999.195 | `capture_shows_rate_limit_denial` Has No Production Caller and Gives the Two-Valued Answer D-16 Replaced (BACKLOG) | relevant | #274 | capture_shows_rate_limit_denial is still pub with only test callers. |
| 999.196 | The Single-Document Rate-Limit Classifier Matches Nested Fields, Against OPERATIONS.md's Top-Level-Only Promise (BACKLOG) | relevant | #272 | The single-document classifier still uses recursive json_scan; whether nested 429 is still matched was not run. |
| 999.197 | `resume --agent X --fallback-agent Y` Commits State Twice (BACKLOG) | relevant | #299 | resume --agent X --fallback-agent Y still commits the handoff and the fallback re-arm in two commits. |
| 999.198 | A Missing Fallback Binary Is Reported Without the `--fallback-agent` Prefix (BACKLOG) | relevant | #260 | validate_fallback_agent still calls ensure_agent_binary unmapped, so a missing fallback binary is reported without the --fallback-agent prefix. |
| 999.199 | doc_check's Removed-Verb Guard Is Bypassed by an Escaped String Literal (BACKLOG) | relevant | #288 | string_literal_value still strips quotes without decoding Rust escapes. |
| 999.200 | A Hanging Fallback Health Check Can Park the Handoff Indefinitely (BACKLOG — suspected) | relevant | #287 | pi health runs `pi auth check` via a plain Command with no deadline. |
| 999.201 | A Stream `result` With `subtype: error_rate_limit` Is Classified Failed, Not RateLimited (BACKLOG — suspected) | fixed | — | The stream path now classifies a top-level result with subtype error_rate_limit as RateLimited. |
| 999.202 | `devflow status` Shows Nothing About an Armed or Spent Fallback Agent (RESOLVED 2026-11-20 — fixed before Phase 53.1; closed by its discuss, | resolved | — | heading status RESOLVED |
| 999.203 | The SUPV-01 Evidence Checker Accepts Stale Line Citations (BACKLOG) | obsolete | — | The SUPV-01 evidence file it checks no longer exists (phase 51 archived) and the evidence concerns the removed supervisor failover trace. [supervisor] |
| 999.204 | Phase 51 Failover Tests Miss a Non-Claude Primary, a Real Gate-Context Render, and a Failed Relaunch One Short of the Infra Ceiling (BACKLOG | relevant | #323 | MAX_INFRA_FAILURES - 1 appears in several tests but I could not confirm a failed fallback relaunch hop test, a non-Claude primary hop test, or a production gate-context render assertion. |
| 999.205 | No Validate Loop-Back Plans Gap Closure, So a `gaps_found` Verification Cannot Be Closed by the Run (BACKLOG) | fixed | — | Validate loop-back now has a PlanGaps stage intent that runs /gsd-plan-phase N --gaps. |
| 999.206 | A Failure Gate Keeps Only 300 Characters of the Reason, and `gate show` Cannot Show the Rest (RESOLVED 2026-11-21 — Phase 53.1, 53.1-06) | resolved | — | heading status RESOLVED |
| 999.207 | A Quota Denial Followed by a Line the Agent Was Killed Mid-Way Through Still Splits the Monitor and the Verdict (BACKLOG) | relevant | #255 | Idle-timeout classification of a quota denial before a killed-mid-line partial still happens in the live monitor pipe code; the cron-resume second case no longer exists but the first case does. |
| 999.208 | A Code Re-Run After an Out-of-Band Fix Cannot Re-Verify a `gaps_found` Report (BACKLOG) | relevant | #253 | Defect is in gsd-core readVerificationStatus ordering, outside this repo; could not check the installed gsd-core source. |
| 999.209 | A Torn Line After a Quota Denial Is Lost When an Escaped Process Holds the Agent's Stdout, So the Stage Fails Over (BACKLOG) | relevant | #302 | The 3s READER_DRAIN_WAIT/join_within/close_shared_capture path that drops a torn line still exists in monitor.rs (pipe-owning stage code, not removed). |
| 999.210 | A Supervise Run Has No Way to End at Validate for a By-Hand Close-Out Except "Abort" (FOLDED INTO Phase 52 — CUT-02, Ship removed) | folded | — | heading status FOLDED |
| 999.211 | README and OPERATIONS.md Describe Rate-Limit Detection and the One-Hop Rule Loosely (BACKLOG) | relevant | #259 | README rate-limit sentence and OPERATIONS.md once-per-run wording are still loose. |
| 999.212 | No Real Claude Quota Denial Has Ever Been Captured, and the Denial Fixtures Lag the Live Event Shape (BACKLOG) | relevant | #280 | No real captured denial exists and no fixture carries unifiedWindows. |
| 999.213 | `devflow cleanup` Misreports a Stopped Run as "Halted via --until", Contradicts Itself, and `--force` Leaves the State Behind (RESOLVED 2026 | resolved | — | heading status RESOLVED |
| 999.214 | Define Is a Paid No-Op: Every Run Launches a Full Agent Session That Does Nothing (FOLDED INTO Phase 52 — CUT-01, Define removed) | folded | — | heading status FOLDED |
| 999.215 | After a Failover the Fallback Keeps the Run, but Every Message Says It Takes Over Only the Code Stage; Its Prompt Says Nothing About the Han | relevant | #309 | README still says the fallback takes over Code once while state.agent stays the fallback afterwards; prompt/handoff wording unchecked. |
| 999.216 | `devflow --version` Does Not Show the Build's Commit (BACKLOG) | fixed | — | devflow --version now prints the version with the embedded commit and tree state. |
| 999.217 | Operator-Facing Output Is Noisy: Raw Tracing Lines, a Red ✗ for an Unset Log Level, and Warnings That Do Not Apply (BACKLOG) | fixed | — | Tracing is off unless RUST_LOG is set and doctor reports an unset RUST_LOG as info; the provenance warning text is gone. |
| 999.218 | The Plan-Bashism Scanner Drops the Finding Text When the File Path Is Long (DUPLICATE of 999.169) | duplicate | — | heading status DUPLICATE |
| 999.219 | `devflow resume` (and `start --no-worktree`) Commits GSD's First Chain-Flag Write as a "Killed Run" Leak Repair (RESOLVED 2026-11-20 — fixed | resolved | — | heading status RESOLVED |
| 999.220 | `cut-release.sh` Cannot See a `BREAKING CHANGE` Footer (BACKLOG) | relevant | #268 | cut-release.sh still reads only subject lines so a footer BREAKING CHANGE is missed. |
| 999.221 | `status_and_doctor_name_a_held_state_write_lock` Flakes on Its "Released" Control (BACKLOG) | fixed | — | The test now waits for the released lock via a bounded helper that tolerates a fork holding the fd. |
| 999.222 | The GSD Test Stub Can Fail to Exec With "Text file busy" (BACKLOG) | fixed | — | The stub write no longer holds a write descriptor at exec and the reader retries ETXTBSY in test builds. |
| 999.223 | `stop_ends_a_launch_its_running_launcher_promotes_during_the_wait` Flakes on Its Liveness Check (BACKLOG) | relevant | #308 | The named test and pending_supervisor_e2e.rs are gone, but the suspected fd-inheritance race touches liveness/stop code that survives. [supervisor] |
| 999.224 | Replace the Overloaded `--force` Flags With Named Actions (BACKLOG) | fixed | — | The overloaded --force flags on start and cleanup no longer exist in the CLI definition. |
| 999.225 | `parse_reset_time` Accepts a Day Past the Month's End (BACKLOG) | relevant | #313 | parse_rfc3339ish still only checks day <= 31, so Feb 31 parses. |
| 999.226 | devflow-core Public Items With No Production Caller After the v4 Cuts (BACKLOG) | fixed | — | The listed unused pub items are gone. |
| 999.227 | Stale v3 Wording in Comments, Test Names and One Retention Count (BACKLOG) | relevant | #311 | Five-stage wording remains and the obsolete guard test still exists. |
| 999.228 | `devflow doctor` Still Checks for the `gh` CLI (BACKLOG) | relevant | #300 | doctor still checks for the gh CLI. |
| 999.229 | An Invalid `DEVFLOW_EXTERNAL_VERIFY_ENABLED` Is Ignored With No Operator-Visible Line (BACKLOG) | fixed | — | An invalid DEVFLOW_EXTERNAL_VERIFY_ENABLED now prints a foreground warning at start. |
| 999.230 | `cut-pr-branch.sh` Phase Mode Drops Phase Work With a Non-Phase Scope, and Halts on the Conflict (BACKLOG) | fixed | — | cut-pr-branch.sh now has a repeatable --include SHA and lists skipped commits. |
| 999.231 | `pre-push` Runs the Workspace Divergence Check Against the Checkout, Not the Pushed Ref (BACKLOG) | fixed | — | pre-push now checks the manifest and divergence against each pushed local_sha, not the checkout. |
| 999.232 | `health_fails_closed_on_a_hung_probe` Flakes on Its Descendant-Dead Check Under Load (BACKLOG) | fixed | — | The test now waits (wait_until) for the descendant to be gone before asserting. |
| 999.233 | `stop` Refuses an Unprobeable Pending-Launch or `--launch` Socket With No Way On (BACKLOG) | obsolete | — | The pending-launch and --launch socket-probe refusals it names (stop_pending_launch, stop_one_launch, probe_refusal) no longer exist. [supervisor] |
| 999.234 | Remaining `fs::write`-Then-Exec Test Stubs Can Hit "Text file busy" (BACKLOG) | relevant | #276 | The fs::write-then-exec stubs remain with no ETXTBSY retry. |
| 999.235 | A Refused Recording Commit Is Logged as `auto_chain_flag_repaired` (BACKLOG) | relevant | #303 | The event is still emitted whenever commit_refused is set, even with nothing repaired. |
| 999.236 | `write_lock_status_reads_free_held_and_the_recorded_holder` Polls an Exclusive Probe, Then Reads Once (BACKLOG) | relevant | #277 | The test still polls wait_for_free_write_lock then does one shared write_lock_status read. |
| 999.237 | `discard` and `start` Do Not Look for a Held Launch Lock a Loadable State Does Not Name (BACKLOG) | fixed | — | start, resume and discard now refuse on the runner liveness of the phase itself, not a supervisor the state names. [supervisor] |
| 999.238 | A GSD 1.21.0 Project That Does Not Ignore `.planning/state.json` Can Never Finish a Run (BACKLOG) | relevant | #306 | Only the scratch fixture now ignores state.json; DevFlow itself still has no handling for an untracked .planning/state.json looping the Validate gate. |
| 999.239 | The Runner's Stop Reason Still Names the Removed Supervisor (BACKLOG) | relevant | #324 | The stop reason and event source still say "supervisor" although no supervisor exists. [supervisor] |
| 999.240 | The Post-Commit Stale-State Marker Leaves the Phase Worktree Dirty, So an Installed Build Reads `dirty` (BACKLOG) | relevant | #294 | build.rs still sets dirty from the whole git status --porcelain including .planning/. |
| 999.241 | `start` Should Warn or Refuse Up Front When GSD's Derived Files Are Not Ignored (BACKLOG) | relevant | #285 | start does not check git check-ignore for GSD derived files. |
| 999.242 | `service_e2e` Fails Intermittently: a Stopped Unit Reads `failed` (BACKLOG) | relevant | #279 | The intermittent unit-reads-failed assertion still exists and no fix was found. |
| 999.243 | A Stale, Empty Worktree `index.lock` Blocks `git add` During Phase Work (BACKLOG) | relevant | #271 | The stale index.lock cause was never found; no fix is in scripts or hooks. |
| 999.244 | A Timed-Out `systemd-run` Makes `start` Clear the State of a Runner That May Be Live (RESOLVED 2026-11-18 — 517854b, 917af52) | resolved | — | heading status RESOLVED |
| 999.245 | A Timed-Out Launch Whose Unit Reads Not Live Clears a Run That May Have Finished a Stage (BACKLOG) | relevant | #258 | The NotLive arm of a timed-out launch still clears state unconditionally. |
| 999.246 | `stop` Reloads the State Without the Phase Holder, So a Concurrent `resume` Can Make It Report "crashed" (BACKLOG) | relevant | #297 | stop_report still calls stop_unit then reloads state without holding the phase holder across both. |
| 999.247 | The Fake Service Manager Never Reports `active`, and Its Launch Can Take a Stale Lock as Ready (BACKLOG) | relevant | #296 | The fake systemctl still never yields active; no test drives the active arm. |
| 999.248 | MCP Registrations for opencode, pi, hermes and antigravity (BACKLOG) | relevant | #257 | Per-launch devflow_ask registration exists only for claude and codex; the other four agents are refused with a named message. |
| 999.249 | A Stage That Ends Needing an Operator Decision Gets a "Stage Failed" Gate Whose Answer Is "Reject" (BACKLOG) | fixed | — | Stage-failure gates now say an agent asking for operator decisions is answered with gate reject. |
| 999.250 | A Rate-Limit Gate Offers "Approve: Relaunches Now" When the Limit Resets Days Later (BACKLOG) | fixed | — | A far-reset rate-limit gate now shows a UTC time and distance and leads with devflow stop then resume --agent. |
| 999.251 | `devflow resume` After an `--until` Stop Re-Runs the Stage That Already Finished (RESOLVED 2026-11-21 — Phase 53.1, 53.1-03; residual 999.26 | resolved | — | heading status RESOLVED |
| 999.252 | GSD's End-of-Phase TDD Review Blocks With No Working Override (BACKLOG) | relevant | #267 | GSD execute-phase TDD review lacks a working override, a GSD gap outside DevFlow. |
| 999.253 | The Post-Commit Hook Rewrites a Tracked File, So a Phase Tree Is Never Clean for Validate (BACKLOG) | fixed | — | state-stale.json is now gitignored, so the post-commit hook no longer dirties tracked files. |
| 999.254 | A Validate Gate Whose Sign-Offs Can Only Run After the Run Ends Has No Way to Finish (BACKLOG) | fixed | — | The human_needed gate now names a route for sign-offs that can only run after the run ends (gate reject with a note), added in Phase 53.1. |
| 999.255 | The `--live-payload` Fixture Has No CONTEXT.md, So a Codex Proof Run Cannot Start (BACKLOG) | fixed | — | --live-payload now commits a 01-CONTEXT.md and the selftest asserts it is present. |
| 999.256 | A Failed Abort Save in devflow_ask Still Closes the Question Gate, So the Run Can Miss the Abort (BACKLOG) | fixed | — | record_abort_request now returns its Result and consume checks it, so a failed state write no longer closes the gate; the message says the abort stays on the open question gate. |
| 999.257 | `a_question_fires_the_notify_hook_and_records_notify_fired` Can Read the Marker Before the Hook Writes It (BACKLOG) | relevant | #273 | The test still waits on marker.exists() then reads the file. |
| 999.258 | Minor Findings From the PR #245 Review (BACKLOG) | fixed | — | All three findings are addressed: the gate-id parse warns, cancel only records an in-flight id, and the doc comment lists pi as admitted. |
| 999.259 | A Finished DevFlow Run Leaves the Phase "In Progress" in GSD, and a Post-Validate Fix Makes Its Verification Stale (BACKLOG) | fixed | — | DevFlow finish now runs GSD phase.complete and commits what it wrote. |
| 999.260 | `devflow resume` After an `--until plan` Halt Reached Through Plan→Validate Starts Code, Not Validate (BACKLOG) | relevant | #281 | The --until halt still records no target, so resume advances to Stage::next. |
| 999.261 | Small Gate-Text and Doc Defects from the PR 251 Re-Review (RESOLVED 2026-11-22 — Phase 53.1, PR #251: 266af2a, 92169fe) | resolved | — | heading status RESOLVED |
