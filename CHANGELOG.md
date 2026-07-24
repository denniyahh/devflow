# Changelog

## 1.8.0 — 2026-07-23

Operator legibility and observability: make DevFlow's operator surface legible
and its self-reported state trustworthy. Every unit is single-writer,
operator-facing, and reversible or detection-only. Phase 21.

### Fixed
- DevFlow's dogfood build-staleness guard no longer hard-blocks a self-run when the only commits ahead of the running binary's embedded commit changed nothing the compiler sees. `embedded_commit_is_stale`'s strict-ancestor arm now filters `git diff --name-only <embedded> HEAD` through the same `affects_compiled_binary` predicate the dirty-tree arm already used — a docs-only (`.planning/`) range reads `Fresh`, any build-input change reads `Stale`, and a git error fails toward `Stale`. The block message no longer claims "is not an ancestor of HEAD" for the common case where the embedded commit *is* an ancestor, just behind

### Added
- `devflow gate show <phase> [--stage]` prints a gate's full context untruncated (the `gate list` view caps at 100 chars), routed through the same control-character sanitizer so it stays terminal-safe
- `devflow status` now surfaces the rate-limit reset time in its cron hints, an in-stage progress line sourced from the latest `stage_launched` event (not the phase's own `started_at`), and `resume`/`advance` recovery-verb hints when a phase is stuck
- `devflow doctor` gains a detection-only planning-doc staleness check that reconciles `ROADMAP.md`/`STATE.md` version and outcome claims against the repo's git tags and flags drift, in both human and `--json` output — it never rewrites prose
- `sequentagent`'s second agent now writes a tracked, path-free slot record so `devflow status` observes it while it runs, with RAII cleanup on every exit path and no routing through the phase state machine

## 1.7.0 — 2026-07-23

Release correctness and operator control: close the two defects that made
DevFlow's own release cut unreliable, then add the operator controls the
pipeline never had. Phase 20.

### Fixed
- `VersionBump` now rewrites every workspace-member `[workspace.dependencies]` self-pin, not just `[workspace.package] version` — previously left the self-pin on the prior release, causing `cargo publish` to reject the upload as a duplicate (shipped broken two releases running)
- `devflow cleanup --force` is now fail-closed on worktree removal: it refuses whenever the recorded agent process is alive or the monitor is active — including `Unknown` (no recorded monitor) and `Stuck` (dead monitor) liveness states — with bounded-backoff retry for genuinely-dead phases and a descriptive warning if retries exhaust. Closes a real race behind two CI flakes in `phase7_cli.rs`
- `cleanup` no longer deletes the worktree of a phase intentionally parked via `devflow start --until <stage>` — it now recognizes the stop marker the same way `doctor` does, and requires `--force` to discard a parked phase

### Added
- `devflow start --until <stage>` halts the pipeline cleanly at a named stage instead of stranding state or orphaning a worktree — `--until ship` is rejected as a semantic no-op. `doctor` and `resume` are both aware of the stop marker
- `devflow release --check`: a read-only, network-independent release-cut preflight — workspace self-pin invariant, `develop`/`main` divergence (no `git fetch`), crates.io publish order, and `gpg.format`-aware signing viability (reports only a public-key fingerprint, never key material or a filesystem path)
- `devflow ship --phase N [--force]`: drives a phase through Ship when the monitor that would have consumed its already-written gate response is dead — reuses the existing fail-closed `finish_workflow` path verbatim, guarded by a per-phase lock and ack-file check so it cannot race a live monitor or double-run the terminal hook batch

### Changed
- `find_version_in_contents`'s TOML value parser now anchors on the opening quote and scans forward for the matching close, so a trailing inline comment (e.g. `version = "1.7.0"  # pinned`) no longer corrupts the parsed value — brings the read path back in line with the comment-preserving write path
- `member_depends_on` now recognizes long-form `[dependencies.NAME]` TOML sections in addition to inline tables, so `release --check`'s publish-order topo-sort no longer silently misses that dependency edge

## 1.6.0 — 2026-07-22

Release integrity and `main.rs` decomposition: close the two defects whose blast
radius reaches outside this repository, then decompose the 8,487-line CLI entry
point as a pure-move refactor. Phase 19.

### Fixed
- DevFlow's runtime artifacts can no longer end up in **your** commits. Every `.devflow/` directory now self-protects with a `.gitignore` containing `*` at creation time, so a routine `git add . && git commit` in a project DevFlow is running against no longer sweeps agent stdout, gate context, and workflow state into that project's history. This held regardless of whether the project's own root `.gitignore` mentioned `.devflow` — it usually didn't
- The `workflow_started` event no longer records the absolute path of the DevFlow binary, which leaked the operator's home directory and OS username into `events.jsonl`
- A release tag can no longer land on an empty commit: `commit_path` no longer forces `--allow-empty`, and is now idempotent when the file it is asked to commit is unchanged

### Changed
- `crates/devflow-cli/src/main.rs` went from 8,487 lines to 478, split into nine flat sibling modules (`staleness`, `preflight`, `pipeline_launch`, `pipeline_outcomes`, `pipeline_gate`, `parallel`, `commands`, `config_parse`, `test_support`). `main.rs` now holds only the Clap types, `CliError`, dispatch, `main`, `run`, and `project_root`. This is a pure move with no behavioral change — verified by symbol reconciliation (231 functions before and after, none lost or added), a normalized body diff showing zero logic-line changes, and a test name-set identical to a committed pre-split baseline (438/438)
- The CLI's test environment lock is now a single shared mutex rather than three independent ones that were sound only by accident. Distributing `PATH`-mutating tests across five modules would otherwise have broken the serialization they depend on
- CI uses `actions/checkout@v7`, retiring the Node 20 deprecation warning

### Added
- An AI change acceptance contract (`.claude/skills/ai-change-acceptance/`, plus a `CONTRIBUTING.md` section) stating what evidence a change must carry before it is accepted, and which test shapes are rejected as false signal

## 1.5.0 — 2026-07-21

Dogfood reliability hardening: make DevFlow's own supervision layer trustworthy
and legible from a plain terminal, and close the state-machine correctness gaps
that let a broken run look healthy. Phase 18.

### Added
- `devflow doctor` is now project-aware: it reconciles the persisted state against the event log, live process IDs, open gates, and branch ancestry, and reports a repair plan — read-only by default, mutating nothing
- Monitor liveness is observable: `monitor_pid` is persisted and probed, so `status` and `doctor` render a distinct "stuck — needs `devflow resume`" state instead of a dead monitor looking identical to a healthy between-stages pause
- Worktree-aware build-staleness: a self-dogfood build behind the worktree branch it is meant to be testing is now detected and blocked

### Changed
- `devflow doctor --json` emits a single JSON document — `{ "environment": [...], "reconciliation": [...] }` — instead of two concatenated top-level arrays, so ordinary JSON parsers can read the full `--json` output
- Build-staleness for a worktree-based phase is evaluated against the worktree branch HEAD rather than the project root, and a stale self-dogfood binary is now blocked rather than warned — the false-evidence class where a two-hours-behind binary re-ran an old hook batch
- The self-dogfood staleness-block event no longer records an absolute filesystem path in `events.jsonl`; the full path stays in the terminal message only

### Fixed
- The Code↔Validate failure loop can now reach its `MAX_CONSECUTIVE_FAILURES` ceiling: the counter was being reset on every stage transition, making the bound unreachable and the loop effectively unbounded under `--mode auto`
- Validate is passable again when an external post-condition is declared: the Layer 0 affirmative-success path now consults the agent's verdict instead of discarding it, advancing automatically only when the probe and the verdict agree, and gating for a human when they disagree or no verdict arrived
- Approving a preflight gate no longer re-runs the identical deterministic check and wedges on a multi-day poll: approval is an explicit override that skips the already-adjudicated check, with a bounded retry backstop; a loop-back still re-checks
- A failed stage relaunch no longer leaves a stale `monitor_pid` that `status`/`doctor` would misreport as "stuck"
- Stabilized a flaky parallel-worktree capture test that could race the monitor's capture archival

## 1.4.0 — 2026-07-20

Pipeline reliability: a completion cascade that cannot silently advance, build
provenance the binary can prove, and pre-launch readiness gates. Phase 17.

### Added
- Typed agent outcomes `ResourceKilled` (exit 137) and `AgentUnavailable` (exit 127), classified in Layer 2 alongside a `decided_by_layer` field recording which layer reached the verdict
- `outcome_policy::decide_action` — a pure, exhaustively-matched outcome-to-action function, so the never-advance guarantee is enforced by the compiler rather than by convention
- `devflow resume --phase N` — relaunches a phase from its saved stage after a rate limit or infrastructure pause, without resetting the workflow to Define or recreating the branch
- Build provenance: the binary embeds the commit it was built from and whether that tree was dirty, degrading gracefully when git metadata is unavailable
- Self-dogfood staleness gate — refuses to drive DevFlow's own workspace from a stale build, the Phase 16 false-evidence incident class
- Preflight readiness checks before every stage launch (plan interactivity, credential validity), reported as a named gate rather than a hard exit
- Separate infrastructure-failure counter, so transient rate limits and OOM kills no longer consume the functional-failure budget that gates a genuinely broken phase

### Changed
- `advance()` dispatches on an exhaustive match over typed outcomes instead of a two-value boolean; an `Unknown` outcome can no longer advance a stage
- Layer 3 splits the former blanket `Unknown`: a vanished process with zero commits and no declaration is now a `Failed` outcome that notifies a human, while commits-exist remains `Unknown` and stays gated
- Layer 0 runs for every stage and treats an approved, all-passing external post-condition as affirmative success, without relaxing the approval-mismatch security property
- Rate-limited outcomes route to the auto-resume machinery instead of being counted as functional failures
- `ChangelogAppend` moved after `VersionBump` and now commits its own write, so a changelog heading can never outlive the tag it claims
- Stage-advance events carry a structured evidence record in place of `reason: null`

### Fixed
- `write_version` no longer drops a trailing comma when rewriting `package.json`, which produced invalid committed JSON
- The release changelog and the git tag can no longer disagree: the shipped version is threaded through the hook context rather than recomputed after tagging
- Build-staleness checks ignore files that cannot affect a compiled binary, so a modified changelog or planning document no longer reports the binary as stale
- A build made from a commit ahead of the checkout is classified as ahead rather than stale
- The concurrent-ship test can no longer wedge the suite indefinitely on an unbounded gate poll

## 1.3.69 — 2026-07-18

- Released phase via DevFlow.

## 1.3.16 — 2026-07-17

- Released phase via DevFlow.

## 1.3.0 — 2026-07-17

- Phase 15: OSS readiness — README/ARCHITECTURE/guides rewritten against v2
  reality, CONTRIBUTING + pinned devcontainer with CI parity, dual
  MIT/Apache-2.0 licensing backed by both texts, devflow-core and devflow
  1.2.0 published to crates.io, per-phase SECURITY.md threat verification.

All notable changes to DevFlow.

## [Unreleased]

### Added
- Root `ARCHITECTURE.md` documenting crates, state machine, agent model, three-layer completion evaluation, monitor daemon, worktree model, git/ship model, configuration schema, and the add-an-agent checklist
- Stale binary detection in `devflow doctor`
- Never-silent gates: every stage failure (not just Validate) now writes a gate and fires a pluggable notify hook via `DEVFLOW_GATE_NOTIFY_CMD`, so unattended runs can never halt silently (WR-11). Gate poll timeout is configurable via `DEVFLOW_GATE_TIMEOUT_SECS` (default 7 days)
- Ship stage now runs `/gsd-code-review` before `/gsd-ship` and refuses to ship on any Critical-severity finding, reporting a distinct `review:`-prefixed failure that loops back to Code instead of gating
- Native per-adapter completion parsing: Claude's `is_error`/`num_turns` envelope fields and a Codex `--json` JSONL event-stream parser (previously only a generic marker/exit-code check)
- `verdict` field (`pass`/`gaps`) on the Validate stage's `DEVFLOW_RESULT` contract — `advance()` only proceeds to Ship on `verdict: pass`, closing the gap between "the agent ran validation" and "validation passed"
- `devflow start` runs in an isolated git worktree by default; `--no-worktree` opts out (previously opt-in via `--worktree`)
- Define/Plan stage prompts are now idempotent: if the stage's deliverable (CONTEXT.md/PLAN.md) already exists, the agent reports success without re-running the GSD command or requesting input — fixes headless Codex runs hanging on GSD's "already exists" decision
- `devflow start --agent codex` pre-flights: errors immediately if the phase has no CONTEXT.md on `develop` (headless Codex cannot run an interactive discussion), with a warning if PLAN.md is also missing
- Codex sandbox (`--sandbox workspace-write`) now gets explicit writable-root grants for the linked worktree's git metadata (both the common `.git` and the worktree's admin dir under `.git/worktrees/<name>`), and commit/tag signing is disabled scoped to Codex's own process tree via `GIT_CONFIG_*` env — the sandbox has no route to the operator's ssh/gpg agent

### Changed
- Corrected docs: removed the phantom `git_flow.enabled` field from examples, fixed the completion-evaluation description (Layer 2 = exit code + commit count, Layer 3 = commit heuristic), and replaced the "3 changes" agent claim with the real checklist
- Removed local-only setup assumptions: untracked `distrobox.ini`, narrowed the `.planning/` gitignore to keep only the prompt-required convention files tracked, documented the GPG-off test setup
- Layer 2's commit-count gate is now scoped to Code-like stages only — Define and Validate legitimately produce zero commits and are no longer mis-flagged as failures
- `devflow`'s lock file (`.devflow/lock`) now reclaims itself when the recorded holder process is dead, instead of wedging every later `devflow advance` for the project

### Removed
- OMX (oh-my-codex) agent support — fully removed (adapter, enum/parser/display, module exports, Hermes skill references, and the stale `.omx/` runtime directory). It had been disabled since 1.0.0; `omx`/`oh-my-codex` are no longer accepted agent names.
- Dead v1 `ship.rs` bookkeeping: the `LastShip` record and the PR-body/goal-extraction/test-summary machinery left over from the removed `devflow confirm`/`devflow rejectpr` commands (zero non-test call sites; PR creation and merging happen entirely inside the external `/gsd-ship` slash command, not in DevFlow's Rust code)

### Fixed
- `devflow ship` now cuts the release branch from the current `HEAD` instead of `develop`, so commits unique to the branch being shipped are no longer dropped from the release
- `git tag` (DevFlow's automatic version-bump tags) no longer blocks on `$EDITOR` when the operator's global `tag.gpgsign` is set to `true` — these are internal SemVer bookkeeping tags, not signed release artifacts, so signing is scoped off per-invocation rather than depending on global config
- Codex's self-reported `DEVFLOW_RESULT` (delivered inside an `agent_message` JSONL item, not as a raw stdout line) is now read correctly — previously a self-reported failure with exit code 0 could be misclassified as success
- The rate-limit detection heuristic no longer scans JSONL event lines as plain text, which could false-match ordinary document content echoed into an agent's output and stuff a multi-KB line into a gate's notification; failure reasons surfaced in gate contexts are now capped at 300 characters

### Fixed
- `devflow ship` now cuts the release branch from the current `HEAD` instead of `develop`, so commits unique to the branch being shipped are no longer dropped from the release

## [1.0.1] — 2026-06-18

### Added
- `devflow doctor` command — environment audit with version detection, JSON output mode
- `scripts/install.sh` — single-command bootstrap for Linux/macOS
- `DEPENDENCIES.md` — full dependency matrix with install instructions
- Standard OSS files: LICENSE (MIT OR Apache-2.0), CONTRIBUTING.md, CODE_OF_CONDUCT.md, SECURITY.md

### Changed
- README completely rewritten for v1.0.0 — accurate command listing, state machine diagram, quick start
- Removed tmux references from docs
- `.github/ISSUE_TEMPLATE/` and `.github/PULL_REQUEST_TEMPLATE.md` added

## [1.0.0] — 2026-06-18

### Added
- Worktree isolation: `--worktree` flag on `start`, dedicated `parallel` and `sequentagent` commands
- Multi-agent support: `parallel` (concurrent phases) and `sequentagent` (sequential handoff)
- Reference worktrees: `reference` command for static snapshots
- PR integration: `ship` creates GitHub PR, `confirm`/`rejectpr` manage lifecycle
- Rate-limit detection: auto-detect agent 429s, write cron instructions for retry
- Monitor daemon: background agent completion detection with auto-advance
- Agent trait system: pluggable agent adapters (Claude, Codex, OpenCode)
- `cleanup` command: remove worktrees and feature branches
- Shared prompt generation: `phase_prompt()` in agent module
- `recover` command: inspect and clean stale workflow state

### Changed
- Removed tmux dependency — agents run directly via CLI
- Deprecated omx/oh-my-codex agent support (disabled; fully removed in a later release)
- CLI reorganized: new command groups for multi-agent and shipping workflows

### Fixed
- Monitor capture thread lifecycle tied to agent process
- Shell-safe quoting in state machine commands
- JSON envelope parsing for agent results

## [0.5.1] — 2026-06-17

### Added
- Ship readiness: version bump, release branch, PR creation via `gh`
- `config` command shows effective configuration
- `init` command bootstraps `.devflow.yaml`

### Changed
- State machine expanded with SHIPPING and CLEANING steps
- Config schema updated with `git_flow` section

## [0.1.0] — 2026-06-16

### Added
- Initial release
- State machine: IDLE → BRANCHING → EXECUTING → VERIFYING → DOCSING
- Git flow branch management
- Version bumper (Cargo.toml, pyproject.toml, package.json)
- `.devflow.yaml` configuration
- Basic CLI: `start`, `check`, `status`, `ship`
