# Phase 7 — Git Worktrees + PR Integration — Implementation Plan

**Status:** Ready for execution | **Target:** v1.0.0 | **Baseline:** v0.5.1, 115 tests, clippy clean

This plan is executable by a coding agent without further clarification. Scope is
locked (see CONTEXT.md). Do **not** re-litigate the design decisions — implement them.

---

## 0. Ground Truth (read before starting)

Workspace layout (Rust, edition 2024, `[workspace]` with two crates):

- `crates/devflow-core/` — library: state machine, git, agents, monitor, version, config
- `crates/devflow-cli/src/main.rs` — clap CLI, one `fn` per subcommand

Key facts the plan relies on (verified against current source):

- **Agent cwd is controlled by the launcher, not the prompt.** `agent::launch_agent`
  (`agent.rs:30`) sets `.current_dir(root)`. `monitor::spawn_monitor` (`monitor.rs:42`)
  runs `sh -c <script>` with **no** `current_dir` — the agent inherits the CLI's cwd.
  This must change for worktree support (see 7a).
- **State + capture files live under `<project_root>/.devflow/`.** `agent_result::stdout_path`,
  `exit_code_path`, `agent_pid_path` all derive from `state.project_root`. Worktrees share
  the same git object DB, so commit-counting (`evaluate_layer2`/`layer3`) works from the main
  root regardless of which worktree the agent ran in. **Keep state/capture in the main repo;
  only the agent's cwd changes.**
- `git.rs::GitFlow` already has `feature_start`, `release_start`, `cleanup_merged`,
  `list_feature_branches`, `commit_all`, `divergence_from_develop` and private `git`/
  `git_output`/`git_raw` runners — reuse these patterns.
- There is **no** `devflow cleanup` command today. The `Command` enum
  (`main.rs:21-107`) has: Start, Check, Status, List, Ship, Init, Config, Recover,
  Verify, Lint, Docs. 7a/7b add new variants.
- `gh` CLI is installed (`/home/linuxbrew/.linuxbrew/bin/gh`). `git worktree` is available.
- `State` (`state.rs:75`) is serialized to `.devflow/state.json`. New fields need
  `#[serde(default)]` so old state files still load.

### Shared verification commands (run after every sub-phase)

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                       # full workspace suite
cargo test -p devflow-core       # library tests only (faster inner loop)
```

Commit each sub-task atomically with Conventional Commit messages
(`feat(worktree): ...`, `test(monitor): ...`, etc.).

---

## 7a — Git Worktree Support

Largest sub-phase. Build the worktree primitive first, then layer commands on top.

### Task 7a.1 — New `worktree` module (the primitive)

- [ ] Create `crates/devflow-core/src/worktree.rs` and register it in `lib.rs`
      (`pub mod worktree;`).
- [ ] Define `WorktreeError` (`thiserror`, mirror `GitError`: `Io`, `Command(String)`).
- [ ] Define `WorktreeInfo { path: PathBuf, branch: Option<String>, head: String }`.
- [ ] `pub fn worktrees_dir(project_root: &Path) -> PathBuf` → `<root>/.worktrees`.
- [ ] `pub fn phase_path(root, phase: u32) -> PathBuf` → `.worktrees/phase-NN` (`{:02}`).
- [ ] `pub fn phase_agent_path(root, phase, agent: &str) -> PathBuf`
      → `.worktrees/phase-NN-<agent>`.
- [ ] `pub fn reference_path(root) -> PathBuf` → `.worktrees/reference`.
- [ ] `pub fn add(root, path, branch, start_point, create_branch: bool)` — runs
      `git worktree add -b <branch> <path> <start_point>` when `create_branch`, else
      `git worktree add <path> <branch>`. If `path` already exists, return a clear error
      (callers decide whether to refresh).
- [ ] `pub fn remove(root, path, force: bool)` — `git worktree remove [--force] <path>`.
- [ ] `pub fn list(root) -> Result<Vec<WorktreeInfo>, WorktreeError>` — parse
      `git worktree list --porcelain` (records separated by blank lines; lines
      `worktree <path>`, `HEAD <sha>`, `branch refs/heads/<name>`, or `detached`).
- [ ] `pub fn prune(root)` — `git worktree prune`.
- [ ] Add `.worktrees/` to the repo's `.gitignore` (create or append).
- [ ] **Tests** (inline `#[cfg(test)]`, reuse the `init_repo()` temp-git helper pattern from
      `git.rs:274`): `add` creates a dir on a new branch; `list` returns the main worktree
      plus added ones with correct branch names; `remove` deletes it; `list --porcelain`
      parser handles detached HEAD and the trailing blank-line record.
- [ ] **Verify:** `cargo test -p devflow-core worktree`

### Task 7a.2 — Thread worktree path through state, launcher, and monitor

- [ ] Add `#[serde(default)] pub worktree_path: Option<PathBuf>` to `State`
      (`state.rs:75`). Initialize `None` in `State::new`. Confirm `state_serde_round_trips`
      still passes and old state files (without the field) still deserialize.
- [ ] Change `agent::launch_agent` (`agent.rs:30`) so its `project_root` param is the
      **agent working directory** (rename to `workdir` for clarity; it is already used only
      as `.current_dir`). It must remain the cwd where the agent process runs.
- [ ] In `monitor::spawn_monitor` (`monitor.rs:86`), prefix the shell script with
      `cd <workdir> && ...` where `workdir = state.worktree_path.as_deref()
      .unwrap_or(&state.project_root)` (shell-escaped). The `devflow check <project_root>`
      calls at the script's tail must still pass the **main** `state.project_root`
      (state/capture live there), not the worktree.
- [ ] In `main.rs::start`, compute `workdir` once: worktree path when worktree mode is on,
      else `project_root`. Pass it to `launch_agent` (blocking mode). Capture
      (`capture_agent_output`) keeps receiving the main `project_root`.
- [ ] **Verify:** `cargo test -p devflow-core` (state + monitor + agent tests green).

### Task 7a.3 — `devflow start --phase N --worktree`

- [ ] Add `#[arg(long)] worktree: bool` to the `Start` variant (`main.rs:24`).
- [ ] In `start()`, when `worktree` is set: **skip** the in-place `git.feature_start`
      branch creation (it would mutate the main checkout). Instead create the worktree via
      `worktree::add(root, worktree::phase_path(root, phase), "feature/phase-NN",
      &config.git_flow.develop, /*create_branch=*/true)`. Respect `--force` by removing an
      existing worktree first (`worktree::remove(..., force=true)`), then re-adding.
- [ ] Set `state.worktree_path = Some(<worktree path>)` before `save_state`.
- [ ] Print the worktree path so the user/Hermes can see where work happens.
- [ ] Worktree mode works in both blocking and `--monitor` paths (agent cwd = worktree;
      state/capture in main `.devflow/`).
- [ ] **Verify (manual smoke, agent stubbed):** in a throwaway git repo with develop,
      `devflow start --phase 7 --worktree` creates `.worktrees/phase-07/` checked out on
      `feature/phase-07`; `git worktree list` shows it.

### Task 7a.4 — `devflow parallel --phases N,M [--agents a,b]`

- [ ] Add `Parallel { phases: String, #[arg(long)] agents: Option<String>, project }`
      variant. Parse `phases` as comma-separated `u32`, `agents` as comma-separated
      `Agent` (positional match to phases; default each to `claude` if fewer agents
      than phases; error if more agents than phases).
- [ ] Implement `fn parallel(...)` that, for each `(phase, agent)` pair, performs the same
      work as `start(... worktree=true, monitor=true ...)`. Monitor mode is required so the
      two runs are concurrent and non-blocking. Each phase gets its own
      `.worktrees/phase-NN/` and its own `.devflow/phase-NN-*` capture files (already
      phase-scoped), so there is no cross-phase collision.
- [ ] Factor the per-phase launch in 7a.3 into a reusable helper
      (e.g. `start_worktree_monitored(root, &config, phase, agent, force)`) and call it
      from both `start` and `parallel`.
- [ ] Print a one-line summary per spawned phase (worktree path + monitor pid).
- [ ] **Verify:** `devflow parallel --phases 7,8 --agents claude,codex` creates
      `.worktrees/phase-07/` and `.worktrees/phase-08/` on separate branches and spawns two
      monitors without error (this is the CONTEXT.md acceptance check for 7a).

### Task 7a.5 — `devflow sequentagent --phase N --agents a,b`

Separate worktrees, sequential with rebase. **No** file-ownership parser, **no** diff
checker — the worktree boundary is the isolation (CONTEXT Q2).

- [ ] Add `Sequentagent { phase: u32, #[arg(long)] agents: String, project }` variant.
      Parse `agents` into exactly two `Agent`s (error otherwise for now).
- [ ] Ensure base branch `feature/phase-NN` exists (create from `develop` if missing,
      reusing `GitFlow::feature_start`-style logic but **without** leaving the main checkout
      on it — create the ref, then return main to `develop`). Record its tip commit.
- [ ] Create worktree A: `worktree::add(root, phase_agent_path(root, phase, "<agentA>"),
      "feature/phase-NN-<agentA>", "feature/phase-NN", create_branch=true)`. Same for B
      off the same `feature/phase-NN` tip.
- [ ] **Sequencing (documented in code comments):**
  1. Run agent A to completion in worktree A (blocking, or monitor + wait). On success,
     integrate A's work into the shared base branch `feature/phase-NN`
     (fast-forward/merge `feature/phase-NN-<agentA>` into it). If a remote exists, push.
  2. Rebase worktree B's branch onto the updated `feature/phase-NN`
     (`git -C <worktreeB> rebase feature/phase-NN`). Surface rebase conflicts as an
     explicit error telling the user to resolve in worktree B — do **not** auto-abort
     silently.
  3. Run agent B to completion in worktree B. Integrate B into `feature/phase-NN`. Push if
     remote exists.
- [ ] Add the git helpers needed in `git.rs` (e.g. `branch_tip(branch)`,
      `merge_ff(branch)`, `rebase_onto(branch)` running with `-C <worktree>` or
      `current_dir`). Keep them small and tested.
- [ ] If agent A fails (Layer-1/2 status `Failed`), stop before touching B and report.
- [ ] **Verify:** in a temp repo, `sequentagent --phase 7 --agents claude,codex` creates
      both `.worktrees/phase-07-claude/` and `phase-07-codex/` from the same base commit;
      after a stubbed A run + integration, B's branch rebases cleanly onto the updated base.
      Cover the happy path and a deliberate-conflict path in tests for the new git helpers.

### Task 7a.6 — `devflow reference [--branch name] [--refresh]`

- [ ] Add `Reference { #[arg(long)] branch: Option<String>, #[arg(long)] refresh: bool,
      project }` variant.
- [ ] Default branch = `config.git_flow.develop`. Target path = `worktree::reference_path`.
- [ ] No `--refresh` and path absent → create static worktree at `.worktrees/reference/`
      on the chosen branch (checkout existing branch, `create_branch=false`).
- [ ] No `--refresh` and path present → print "reference exists at <path> (use --refresh)".
- [ ] `--refresh` → run `git -C .worktrees/reference fetch`-free update: simplest correct
      behavior is `git -C <ref> checkout <branch> && git -C <ref> reset --hard <branch>`
      (or remove + re-add). Manual only — **no** hooks, **no** auto-sync (CONTEXT Q3).
- [ ] **Verify (manual):** `devflow reference` then `devflow reference --refresh` updates the
      snapshot; a separate ongoing phase worktree is untouched.

### Task 7a.7 — `devflow cleanup` (new command)

- [ ] Add `Cleanup { project, #[arg(long)] force: bool }` variant.
- [ ] Implement `fn cleanup(...)`: for every worktree under `.worktrees/` (via
      `worktree::list`, excluding the main worktree and `reference/` unless `--force`),
      run `worktree::remove`, then delete the associated `feature/phase-*` branch with the
      existing `GitFlow` branch-delete path. Finish with `worktree::prune` and
      `GitFlow::cleanup_merged()`.
- [ ] Print each removed worktree + deleted branch. Never delete `main`/`develop`.
- [ ] **Verify:** after a `start --worktree` run, `devflow cleanup` removes the worktree dir
      and the feature branch; `git worktree list` shows only the main worktree.

### Task 7a.8 — `devflow status` lists active worktrees

- [ ] In `fn status` (`main.rs:434`), after the existing state/branch output, call
      `worktree::list` and print active worktrees with their branch and inferred phase/agent
      (parse from the dir name `phase-NN[-agent]`). Skip if none. Tolerate errors (no git
      repo) the same way `print_open_branches` does.
- [ ] If `state.worktree_path` is set, label it as the current run's worktree.
- [ ] **Verify (manual):** with one active phase worktree, `devflow status` lists it.

### Task 7a.9 — CliError wiring

- [ ] Add `#[error(transparent)] Worktree(#[from] devflow_core::worktree::WorktreeError)` to
      `CliError` (`main.rs:109`) and route the new subcommands through `run()`'s match.

---

## 7b — PR Integration (Model B, confirmation-gated)

No polling, no webhooks. User drives confirmation. Three commands: extend `ship`, add
`confirm`, add `rejectpr`.

### Task 7b.1 — `LastShip` record + module

- [ ] Create `crates/devflow-core/src/ship.rs` (register in `lib.rs`).
- [ ] Define `LastShip { phase: u32, version_from: String, version_to: String,
      release_branch: String, pr_number: Option<u64>, pr_url: Option<String>,
      version_file: PathBuf, rejected: bool, reject_reason: Option<String>,
      created_at: String }` (serde).
- [ ] `pub fn last_ship_path(root) -> PathBuf` → `.devflow/last-ship.json`.
- [ ] `pub fn save(root, &LastShip)`, `pub fn load(root) -> Result<LastShip, _>`,
      `pub fn delete(root)`. Define `ShipError` (`thiserror`: `Io`, `Json`, `Missing`).
- [ ] **Tests:** save → load round-trips; `load` on missing file returns `Missing`;
      `delete` is idempotent.

### Task 7b.2 — PR body generation

- [ ] `pub fn build_pr_body(root, phase, &VersionConfig, &GitFlowConfig) -> String` in
      `ship.rs`. Compose from three sources (each fail-soft to a placeholder line):
  1. **Phase summary** — read `.planning/phases/{phase:02}-*/CONTEXT.md`, extract the
     `## Goal` section (text until the next `##`). Use a glob/dir scan since the suffix
     after `NN-` varies.
  2. **Changed files** — `git diff --stat <develop>...HEAD` run in `root`.
  3. **Test count** — run the configured `verify_command` (or `cargo test`) and parse the
     `test result: ok. N passed` line; fall back to "tests: unknown" if it can't be parsed.
- [ ] Render as Markdown with `## Summary`, `## Changes`, `## Tests` headings.
- [ ] **Tests:** with a fixture CONTEXT.md, the body contains the Goal text; with no
      CONTEXT.md it still produces a valid body with a placeholder.

### Task 7b.3 — Extend `devflow ship`

Current `ship` (`main.rs:507`) bumps version + creates release branch. Add the PR flow.

- [ ] After `release_start` succeeds: commit the version bump on the release branch
      (`GitFlow::commit_all("chore: bump version to <next>")`).
- [ ] Push the release branch to origin: `git push -u origin <release_branch>`. If no
      remote/push fails, print a warning and continue (PR step will then be skipped).
- [ ] Create the PR: shell out to `gh pr create --base <develop> --head <release_branch>
      --title "Release <next> (phase N)" --body-file <tmp>` where the body is from 7b.2.
      Parse the PR URL/number from `gh` stdout.
- [ ] Write `.devflow/last-ship.json` (`ship::save`) capturing version_from/to, release
      branch, pr_number/url, version_file path, `rejected=false`.
- [ ] Print PR URL. If `gh` is missing or fails, print a clear warning, still write
      last-ship.json with `pr_number=None` so `confirm`/`rejectpr` work for a manual PR.
- [ ] Add `Ship` flag `#[arg(long)] no_pr: bool` to allow the old (bump-only) behavior.
- [ ] **Verify (manual, real repo):** `devflow ship` on devflow itself bumps the version,
      pushes a release branch, opens a PR against `develop` with a descriptive body, and
      writes `.devflow/last-ship.json`.

### Task 7b.4 — `devflow confirm`

- [ ] Add `Confirm { project }` variant + `fn confirm`.
- [ ] Load `last-ship.json` (error clearly if absent: "nothing to confirm").
- [ ] If `pr_number` is set, query merge status: `gh pr view <n> --json state,mergedAt`.
      If not merged, **warn** but allow the user to proceed (do not hard-fail — Model B
      keeps the user in control).
- [ ] Finalize docs: append a CHANGELOG.md entry for `version_to` (create the file with a
      standard header if missing) and mark the phase complete in `.planning/ROADMAP.md`
      (flip the phase's checklist/▮ markers or annotate "COMPLETED v<version_to>", matching
      the existing Phase 6 line format `(Priority: HIGH — COMPLETED v0.5.1)`).
- [ ] Clear workflow state for the phase if present (`workflow::clear_state`), then
      `ship::delete` the last-ship.json.
- [ ] Print a completion summary.
- [ ] **Tests:** the doc-finalization helpers (CHANGELOG append, ROADMAP phase-marking) are
      pure-ish string transforms — unit-test them against fixture content. Keep the `gh`
      call behind a thin wrapper so logic is testable without network.

### Task 7b.5 — `devflow rejectpr [--reason ...] [--redo]`

- [ ] Add `Rejectpr { project, #[arg(long)] reason: Option<String>, #[arg(long)] redo: bool }`.
- [ ] Load `last-ship.json`. Without `--redo`: set `rejected=true`, store `reason`, save.
      Print "rejection recorded".
- [ ] With `--redo`: 
  1. Close the PR if `pr_number` set: `gh pr close <n>`.
  2. Delete the release branch locally and on origin (`git branch -D` + `git push origin
     --delete`, each fail-soft).
  3. Revert the version bump: `version::write_version(root, &config.version, &version_from)`.
  4. Re-open the phase: restore/clear workflow state so the phase can be re-run (set step
     back to `Executing` for that phase, or clear state — document the chosen behavior).
  5. `ship::delete` the last-ship.json.
- [ ] Print what was undone.
- [ ] **Verify (manual):** after a `ship`, `devflow rejectpr --redo` closes the PR, deletes
      the release branch, and restores the prior version in the version file.

### Task 7b.6 — ShipError wiring

- [ ] Add `#[error(transparent)] Ship(#[from] devflow_core::ship::ShipError)` to `CliError`
      and route `confirm`/`rejectpr` through `run()`.

---

## 7c — Test Hardening (close Phase 6 review gaps)

### Task 7c.1 — Strengthen the `spawn_monitor` test

- [ ] Replace `spawn_monitor_returns_pid_for_valid_input` (`monitor.rs:192`) so it asserts
      **observable output**, not just `pid > 0`: use an agent command that writes a known
      marker (e.g. `sh -c 'echo READY'`), wait briefly via the existing
      `wait_for_agent_pid` helper, and assert the agent-pid file appears with a valid numeric
      PID and (after the script runs) the stdout capture file contains the marker.
- [ ] Keep the test hermetic (tempdir, no real agent binary; `sh`/`true`/`echo` only).
- [ ] **Verify:** `cargo test -p devflow-core monitor`

### Task 7c.2 — Layer 2 failure-path tests

- [ ] In `agent_result.rs` tests, add:
  - `evaluate_layer2_exit_zero_no_commits_is_failed`: feature branch exists with **0**
    commits ahead of develop, exit file = `0` → `AgentStatus::Failed`, reason mentions
    "no commits".
  - `evaluate_layer2_nonzero_exit_is_failed`: exit file = `1` → `AgentStatus::Failed`,
    reason mentions "exited with code 1".
- [ ] Reuse/extend the `init_repo_with_feature_commit` helper; for the zero-commits case,
      create the feature branch at develop's tip with no extra commit.
- [ ] **Verify:** `cargo test -p devflow-core agent_result`

### Task 7c.3 — Lowercase-no-space marker variant

- [ ] Add `parse_lowercase_no_space_marker`: input `"devflow_result:{\"status\":\"success\"}\n"`
      (lowercase **and** no space) → parses to `Success`. (Existing tests cover lowercase
      *with* space and uppercase *no* space separately; this covers the combination, which
      is the documented gap.)
- [ ] **Verify:** `cargo test -p devflow-core agent_result`

### Task 7c.4 — End-to-end monitor integration test

- [ ] Add `crates/devflow-core/tests/monitor_e2e.rs` (integration test). Set up a temp git
      repo (develop + a feature branch with one commit, gpgsign off, `core.hooksPath`
      `/dev/null` — copy the helper pattern). Use a **fake agent**: a `sh -c` script that
      prints `DEVFLOW_RESULT: {"status":"success"}` and exits 0.
- [ ] Drive `spawn_monitor` with that fake agent. Poll until the exit file appears, then
      assert: agent-pid file written, stdout capture contains the marker, exit file == `0`.
- [ ] Because the real `devflow` binary isn't built inside the lib's integration test,
      assert the **capture/result** side (Layers 1–2 evaluate to `Success`) rather than the
      `devflow check` self-invocation. Document this boundary in a comment.
- [ ] **Verify:** `cargo test -p devflow-core --test monitor_e2e`

---

## 7d — Prompt Rationalization

Goal: one prompt source. Both agents get identical instruction text; only CLI flags differ.

### Task 7d.1 — Extract `phase_prompt()` into `agents/mod.rs`

- [ ] Move the body of `claude.rs::rich_prompt` (`claude.rs:35`) to
      `agents/mod.rs` as `pub fn phase_prompt(phase: u32) -> String` (verbatim text — do not
      water it down to Codex's old shorter version).
- [ ] `ClaudeAgent::exec_command` calls `super::phase_prompt(phase)`; delete the now-unused
      `rich_prompt` from `claude.rs`.

### Task 7d.2 — Codex uses the shared prompt; delete `simple_prompt`

- [ ] `CodexAgent::exec_command` (`codex.rs:14`) calls `super::phase_prompt(phase)` for its
      prompt arg.
- [ ] Delete `simple_prompt` from `codex.rs`.

### Task 7d.3 — Update affected tests

- [ ] `state.rs::exec_command_codex_uses_exec_and_json` (`state.rs:316`) currently only
      asserts `exec`/`--sandbox`/`--json`/`phase 7`. Extend it to also assert the shared
      prompt content now reaches Codex: `ROADMAP.md`, `CONTEXT.md`, `cargo test`,
      `DEVFLOW_RESULT`.
- [ ] Add an `agents::tests` assertion that
      `adapter_for(Claude).exec_command(N)` and `adapter_for(Codex).exec_command(N)` contain
      **identical prompt text** (extract the prompt arg from each and compare for equality).
- [ ] **Verify:** `cargo test -p devflow-core` and confirm both prompts match.\n\n### Task 7d.4 — Fix pervasive `DEVLOW`→`DEVFLOW` typo (49 instances, 13 files)\n\n- [ ] Global find-and-replace `DEVFLOW_RESULT`→`DEVFLOW_RESULT`, `devflow_result`→`devflow_result`,\n      `parse_devflow_result`→`parse_devflow_result` across:\n      - Source: `agent_result.rs`, `claude.rs`, `codex.rs`, `opencode.rs`, `monitor.rs`,\n        `state.rs`, `main.rs`\n      - Docs: 6 `.planning/` files + `skills/` (2 files)\n- [ ] The marker string in agent prompts (`claude.rs:71`, `codex.rs:42`) must match the parser —\n      both change to `DEVFLOW_RESULT:`\n- [ ] The `--output-format json` envelope unwrapper must match the new case\n- [ ] **Verify:** `cargo test` (all 115+), `cargo clippy`, grep for `DEVLOW` returns zero\n      results in source files.\n\n---\n\n## 7e — Rate-Limit Detection + Cron Instructions\n\n### Task 7e.1 — `AgentStatus::RateLimited` + detection\n\n- [ ] Add `RateLimited` variant to `AgentStatus` enum (`agent_result.rs:26`). Serde already\n      `rename_all = \"lowercase\"` — `\"ratelimited\"` serialization is automatic.\n- [ ] Add `pub fn detect_rate_limit(stdout: &str) -> Option<String>` in `agent_result.rs`:\n      - Claude: parse `--output-format json` envelope, check for `\"subtype\":\"error_rate_limit\"`,\n        extract `retry_after` field if present\n      - Codex: regex `Try again at (.+?)\\b`, capture the time string\n      - Return `Some(retry_time)` or `None`\n- [ ] In `evaluate_layer1`, after failing to find a DEVFLOW_RESULT marker, call\n      `detect_rate_limit(stdout)`. If detected, return `AgentResult { status: RateLimited,\n      reason: Some(\"rate limited until <time>\"), ... }`.\n- [ ] **Tests:** Claude JSON rate-limit fixture → `RateLimited`; Codex text rate-limit →\n      `RateLimited`; normal stdout → no false positive.\n- [ ] **Verify:** `cargo test -p devflow-core agent_result`\n\n### Task 7e.2 — Sequentagent rate-limit handoff\n\n- [ ] In sequentagent (`main.rs`), after agent A completes: if `agent_result.status ==\n      RateLimited`, do NOT treat as failure. Instead:\n      1. Log \"Agent A rate-limited; handing off to agent B\"\n      2. Commit agent A's partial work (already committed during execution — verify)\n      3. Proceed to agent B (rebase + continue) the same as a normal sequential handoff\n      4. Write `.devflow/cron-instructions.json` so Hermes can schedule a resume later if needed\n- [ ] If agent A was rate-limited mid-work with zero commits, don't hand off — pause and\n      write cron instructions (no work to rebase onto).\n- [ ] **Verify:** In a temp repo with a fake rate-limited agent A, sequentagent proceeds to\n      agent B; `cron-instructions.json` is written.\n\n### Task 7e.3 — Cron-instructions manifest\n\n- [ ] Define `CronInstructions { project: String, phase: u32, status: String,\n      retry_after: String, resume: ResumeCommand, hermes_cron: HermesCronJob }` in\n      `ship.rs` (alongside `LastShip` — same persistence pattern).\n- [ ] `ResumeCommand { command: String, args: Vec<String> }`\n- [ ] `HermesCronJob { schedule: String, name: String, command: String, once: bool }`\n- [ ] `pub fn write_cron_instructions(root: &Path, instructions: &CronInstructions)` writes\n      `.devflow/cron-instructions.json`.\n- [ ] `pub fn delete_cron_instructions(root: &Path)` removes it on resume/confirm.\n- [ ] Generate `hermes_cron.schedule` from the `retry_after` timestamp (round up to the\n      nearest minute, format as cron expression `M H D M W`).\n- [ ] Generate `hermes_cron.command` as: `cd <project_root> && devflow sequentagent\n      --phase <N> --agents <next_agents>`.\n- [ ] `devflow status` detects `.devflow/cron-instructions.json` and prints:\n      \"Cron instruction pending: hermes cron create --from-devflow <project>\"\n- [ ] **Tests:** round-trip save/load; schedule formatting; `devflow status` includes cron hint.\n- [ ] **Verify:** After a rate-limited sequentagent run, `.devflow/cron-instructions.json`\n      exists; `devflow status` prints the `hermes cron create` command verbatim.\n\n---\n\n## 8. Final Integration Pass

- [ ] Update `default_config_yaml()` only if a new config knob was actually added
      (none required by this plan — leave untouched unless a task above demands it).
- [ ] Update `AGENTS.md` / `CLAUDE.md` command list if it enumerates subcommands, adding
      `parallel`, `sequentagent`, `reference`, `cleanup`, `confirm`, `rejectpr`.
- [ ] `cargo fmt` and ensure `cargo clippy --all-targets -- -D warnings` is clean.
- [ ] Bump workspace version per the ship flow when the phase is confirmed (not during
      implementation).

---

## Success Criteria (phase is done when all hold)

1. `devflow start --phase N --worktree` creates `.worktrees/phase-NN/` on `feature/phase-NN`
   and the agent runs with that worktree as its cwd; state/capture stay in the main repo.
2. `devflow parallel --phases 7,8 --agents claude,codex` spawns two concurrent monitored
   runs in separate worktrees with no file/branch/capture collision.
3. `devflow sequentagent --phase N --agents claude,codex` creates two worktrees from the same
   base commit; agent A integrates → agent B rebases onto the updated base → integrates;
   conflicts surface as explicit rebase errors.
4. `devflow reference` / `--refresh` create and manually update a static `.worktrees/reference/`
   snapshot with no auto-sync.
5. `devflow cleanup` removes worktrees and their feature branches; `devflow status` lists
   active worktrees.
6. `devflow ship` bumps version, pushes a release branch, opens a `gh` PR against `develop`
   with a body containing the phase Goal, `git diff --stat`, and a test count; writes
   `.devflow/last-ship.json`.
7. `devflow confirm` finalizes CHANGELOG.md + ROADMAP.md, marks the phase complete, and
   deletes `last-ship.json` (warns, does not block, if the PR is unmerged).
8. `devflow rejectpr --redo` closes the PR, deletes the release branch, reverts the version
   bump, and re-opens the phase.
9. Both agents receive **identical** prompt text via `agents::phase_prompt`; `simple_prompt`
   and `rich_prompt` no longer exist.
10. Test count ≥ 115 + all new tests pass; `cargo clippy --all-targets -- -D warnings` clean;
    `cargo fmt --check` clean. The four Phase-6 review gaps (weak spawn_monitor test, Layer 2
    failure paths, lowercase-no-space marker, E2E monitor test) are closed.

## Verification Matrix

| Sub-phase | Automated check | Manual check |
|---|---|---|
| 7a | `cargo test -p devflow-core worktree` | `parallel --phases 7,8` creates 2 worktrees |
| 7b | `cargo test -p devflow-core ship` | `ship` opens a real PR on devflow |
| 7c | `cargo test -p devflow-core` (+ `--test monitor_e2e`) | — |
| 7d | `cargo test -p devflow-core` (prompt-equality test) | — |
| all | `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` | — |
