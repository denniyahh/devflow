---
phase: 50-addressable-monitor-liveness
plan: 17
subsystem: docs
tags: [docs, operations, supervisor, unix-socket, liveness, doc_check, macos, changelog]

requires:
  - phase: 50-12
    provides: PhaseLiveness wording (ALIVE/STALE/GONE, probe failed), status/doctor repairs, orphan sweep, doctor --json keys
  - phase: 50-14
    provides: stop arms (gate, socket, lock holder), --launch, recover --clean refusal
  - phase: 50-15
    provides: start/resume guard over a corrupt state, unlistable socket-dir refusal
  - phase: 50-16
    provides: State::monitor_pid removed; API break list for the release note
provides:
  - "OPERATIONS.md: rewritten `devflow stop` row, `__monitor` paragraph for both launch shapes, `Run liveness` section, macOS caveat, D-20 install rule"
  - "ARCHITECTURE.md `## Monitor daemon` rewritten for the socket supervisor and its in-process advance"
  - "docs/guides/unattended-mode.md: both auto-mode refusals give the capture shape as the reason"
  - "docs/diagrams/agent-lifecycle.md: supervisor model, no forked advance arrow"
  - "doc_check::operations_md_documents_run_liveness_and_the_stop_arms and doc_check::docs_describe_the_socket_supervisor_not_the_shell_monitor (permanent scans)"
  - "commit b629cd0 with the 3.0.0 changelog subject `docs(50-17): state socket supervision is unverified on macOS`"
affects: [50-18, 50-19, 50-20, release-3.0.0]

actuals:
  tokens: 10426   # chars/4 over `git diff a7d918a..4479785` (41706 chars, context lines included)
  tasks: 3
  commits: 3
plan_head_before: a7d918a9b2ce7d2f15f642f6d3bcc91a4dac572c
plan_head_after: 4479785f8a692396a407bbf4b380547f3ee7a32d

tech-stack:
  added: []
  patterns:
    - "Doc scans strip HTML comments first, so a fragment hidden in a comment never counts as documented"
    - "Section-scoped doc checks (markdown_section) instead of whole-file greps, with planted controls run through the same check function as the real doc"

key-files:
  created: []
  modified:
    - OPERATIONS.md
    - ARCHITECTURE.md
    - docs/guides/unattended-mode.md
    - docs/diagrams/agent-lifecycle.md
    - crates/devflow-core/src/doc_check.rs
    - .planning/phases/50-addressable-monitor-liveness/deferred-items.md

key-decisions:
  - "cleanup's live-run refusal is documented with the right verb (`devflow stop --phase N`), and OPERATIONS.md says the refusal's own text still names `devflow resume`, which does not end a live run"
  - "The `phase-NN-monitor-stop` row is removed from OPERATIONS.md's .devflow/ inventory: nothing writes or removes that file since the shell monitor was retired"
  - "mentions_shell_monitor lowercases as well as collapsing whitespace, so a capitalised `Shell monitor` also fails the guide check"

patterns-established:
  - "An operator doc that names a CLI wording which is out of step with the code says so in the doc, instead of documenting the wrong verb"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "OPERATIONS.md documents GONE / STALE / ALIVE, each repair verb, the stop arms in code order (gate, socket, lock holder), the backstop's leader-gone refusal with its ps inspection command, the socket location and both internal subcommands"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::operations_md_documents_run_liveness_and_the_stop_arms"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::source_devflow_env_vars_and_subcommands_are_documented"
        status: pass
    human_judgment: false
  - id: D2
    description: "OPERATIONS.md states the macOS caveat and D-20's install rule; no doc under docs/ or OPERATIONS.md claims macOS support; ARCHITECTURE.md's Monitor daemon section names the socket and the in-process advance and no forked advance or sh -c; the unattended-mode guide names the capture shape and no shell monitor"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::docs_describe_the_socket_supervisor_not_the_shell_monitor"
        status: pass
    human_judgment: false
  - id: D3
    description: "The agent lifecycle diagram shows the supervisor advancing in-process, with no forked advance arrow"
    requirement: SUPV-01
    verification:
      - kind: other
        ref: "test -f docs/diagrams/agent-lifecycle.md && ! rg -q -F -e monitor_pid -e 'monitor script' -e 'advance --phase' docs/diagrams/agent-lifecycle.md"
        status: pass
    human_judgment: false
  - id: D4
    description: "The prose in the four docs is accurate and readable for an operator who has not read the code (the scans pin fragments, not meaning)"
    requirement: SUPV-01
    verification: []
    human_judgment: true
    rationale: "The doc scans check that required phrases are present and retired ones absent; whether each sentence is correct and clear is a reading judgment. Every claim was checked against the code while writing (see Verification), but no test asserts prose meaning."

duration: 11min
completed: 2026-10-18
status: complete
---

# Phase 50 Plan 17: Operator Docs for the Socket Supervisor Summary

**OPERATIONS.md now explains run liveness the way the code answers it: ALIVE / STALE / GONE from the recorded socket, the repair verb for each, the `devflow stop` arms in code order, where the sockets live and their 107-byte bound, plus the macOS caveat and the 2.x install rule. ARCHITECTURE.md, the unattended-mode guide and the lifecycle diagram no longer describe a shell monitor or a forked `devflow advance`. Two permanent `doc_check` scans keep it that way.**

## Needs awareness

- **The 3.0.0 release note is only a commit subject so far.** The changelog phrase is the whole subject of `b629cd0`: `docs(50-17): state socket supervision is unverified on macOS` (60 characters, touches `OPERATIONS.md`, no `[ci skip]`). Both release paths draft the 3.0.0 entry from commit subjects, so this puts the phrase into the draft. It does not guarantee the released section keeps it. A squash merge of the phase PR replaces the subject with the PR title, and a hand-written release section can drop it. 50-20's release handoff in `50-LIVE-RUN.md` owns the merge-commit rule for the phase PR, a check of the 3.0.0 section before and after the release, and the repair.
- **The release notes must also name a break the semver gate cannot see.** `spawn_monitor` changed its return type from `Result<u32, _>` to `Result<Published, _>` (50-11, `a03b572`). Write it beside the gated breaks: `MonitorError` became `non_exhaustive`, `spawn_monitor`'s parameters changed, `run_pipe_owning_monitor` was removed, `State::monitor_pid` was removed, and the pre-phase baseline breaks. This plan writes no CHANGELOG text (see the plan's objective), so this list is for 50-20 to carry.
- **Two operator-facing CLI texts are still out of step with the docs.** The auto-mode preflight refusal (`preflight.rs`) says the guard lives in "the pipe-owning monitor, which this launch shape never starts". Since 50-11, a capture launch starts the same supervisor; it just never engages the guard. Separately, `cleanup`'s live-run refusal names `devflow resume` where `devflow stop` is right. OPERATIONS.md documents `stop` and says the refusal text is out of step. Both are logged in `deferred-items.md` with two smaller items: a dead `pub fn stop_marker_path`, and ARCHITECTURE.md's gate section, which still describes only the 60 s backoff.
- **macOS:** the docs claim nothing. They say "Socket supervision is unverified on macOS". 50-05..50-09 only compiled for macOS.

## Performance

- **Duration:** about 11 min
- **Started:** 2026-10-18T18:32:08Z
- **Completed:** 2026-10-18T18:43:27Z
- **Tasks:** 3
- **Files modified:** 5 (plus `deferred-items.md`)

## Accomplishments

- **`devflow stop` row.** The arms are listed in the order `commands::stop` runs them:
  1. An open gate with a live waiter gets a rejection.
  2. Otherwise the socket is probed. The socket is the one the state records, or the one sidecar-found socket when the state does not load. Each answer has its own arm:
     - ALIVE: `shutdown` (exit code 143, no advance).
     - STALE: the backstop. It signals only a group whose leader is present, has the recorded start time and still leads the group. A previous boot, an empty group or a recycled leader has its files removed and nothing signalled. A group whose leader is gone is refused, and its members are listed with `ps -o pid,pgid,sid,lstart,args -g <pgid>`. The residual limit is stated.
     - A failed probe: a refusal.
  3. GONE: the lock holder.

  The row also covers the unloadable-state message, the several-sockets refusal and `--launch <id>`. `state.monitor_pid` and the monitor script are gone from it.
- **`__monitor` paragraph.** Both launch shapes run the same Rust supervisor, which advances in-process:
  - capture: adapters other than Claude and Antigravity, the legacy opt-out, and a checkpoint resume;
  - stream: Claude and Antigravity `stream-json`.

  The literal `devflow monitor` stays in the paragraph, and the internal-command note above it keeps `devflow advance`, reworded (RV-06).
- **`## Run liveness` section.**
  - A table of the three states, with their meanings and repairs.
  - What `status`, `doctor` (text and `--json`), `cleanup`, `recover --clean`, `start` and `resume` each do with them.
  - "probe failed" and its `devflow status` repair.
  - The orphan socket sweep.
  - The socket location: `registry::cache_dir()/sock`, from `DEVFLOW_CACHE_DIR`, then `$XDG_CACHE_HOME/devflow`, then `~/.cache/devflow`.
  - A launch with no cache dir fails.
  - Modes 0700/0600, and the 107-byte bound, which is refused by name.
  - SIGTERM/SIGINT to a supervisor; a stop during a gate wait records `stopped` and does not abort.
  - The macOS sentence and the D-20 sentence.
- **Other OPERATIONS.md fixes:**
  - The `status` and `doctor` rows. The doctor row now lists all six `--json` keys.
  - The gate pickup latency: about 1 s in a supervisor, not about 60 s.
  - `DEVFLOW_CLAUDE_LEGACY_LAUNCH` now names the capture shape instead of the `sh` monitor.
  - The `.devflow/` inventory loses the `phase-NN-monitor-stop` row.
- **ARCHITECTURE.md `## Monitor daemon`.** Both shapes, then the supervisor's five steps: spawn into its own group; publish the socket and sidecar, recorded as `state.supervisor`; answer `ping`/`shutdown` and treat SIGTERM/SIGINT as `shutdown`; record the exit code; advance in-process, with the next stage's supervisor publishing before this one unlinks. `spawn_monitor()` returns on ping, and the section keeps "the single way an agent process is spawned (23d)". The section never spells the `advance` subcommand.
- **Unattended-mode guide.** Both headings and both refusals are kept. The reason is now that only the stream-shape supervisor engages the chain-flag guard, and `codex`, `opencode` and the legacy opt-out launch on the capture shape. I checked this against `resolve_launch_shape` and `stream_launch_enabled`. The guide also stopped calling auto mode "Claude-only": Antigravity is allowed.
- **Lifecycle diagram.** The supervisor owns the agent, publishes its socket and advances in-process. The next stage's supervisor publishes before the old socket is unlinked. The `Monitor->>CLI: advance --phase 16` arrow is gone.

## Task Commits

1. **Task 1 (tracer): the stop row and the liveness section** — `e42651a` `docs(50-17): document run liveness and the socket-based stop arms`
2. **Task 2: the macOS caveat, the install rule, ARCHITECTURE.md and the unattended-mode guide** — `b629cd0` `docs(50-17): state socket supervision is unverified on macOS` (**the 3.0.0 changelog subject; do not reword, squash or amend it**)
3. **Task 3: the lifecycle diagram** — `4479785` `docs(50-17): show the in-process advance in the agent lifecycle diagram`

**Plan metadata:** the docs commit that adds this SUMMARY.

**Tracer feedback gate:** auto mode was off (`auto_advance` and `_auto_chain_active` both false), `human_verify_mode` was at its default `end-of-phase`, and Task 1's `<verify>` is automated only, so row 3 applied. I re-ran the Task 1 gate after the doc edit (rc 0) before expanding to Task 2.

## TDD Gate Compliance

This is a `type: execute` plan with no `tdd="true"` task, so the RED/GREEN commit gate does not apply. Each scan was still written first and run against the unchanged docs, with these results.

- **Task 1 scan on the old OPERATIONS.md.** nextest exit 100. All planted controls passed first. The real-doc assertion then reported `GONE`, `STALE`, `ALIVE` and `ps -o pid,pgid,sid,lstart,args -g` missing.
- **Task 2 scan.**
  - **My first two runs failed on my own planted fixture.** It came out one line short of the 8-line minimum, because `lines()` does not count a trailing empty line. I fixed the fixture. Those runs were not evidence about the docs.
  - **The run after the fixture fix** failed on the real doc: `OPERATIONS.md does not state 'unverified on macOS'`.
  - **After the OPERATIONS.md sentences were added,** it failed on ARCHITECTURE.md with all three expected problems: no `in-process`, no `socket`, still names `devflow advance`.
  - **After the ARCHITECTURE.md rewrite,** it failed on `docs/guides/unattended-mode.md does not name the capture shape`.
  - **Every link in the chain failed for its intended reason** before the next doc was fixed.

## Verification (commands run, actual exit codes)

| Check | Result |
|---|---|
| Task 1 `<verify>`, run verbatim after the edit | rc 0. Both named tests `1 test run: 1 passed`. The absence check and `devflow stop --help` passed |
| Control for Task 1's absence check | the old `OPERATIONS.md` (`git show HEAD:`) matches the pattern set (rg rc 0), so the check discriminates |
| All `doc_check::` tests after Task 1 | 8 run, 8 passed. This includes `doc_referenced_identifiers_exist_in_source`, which checks every backticked identifier, flag and `.devflow/` path I wrote |
| Task 2 `<verify>`, verbatim | rc 0. Three named tests at `1 passed` each. The whole `devflow-core` lib: `934 tests run: 934 passed (1 leaky), 1 skipped` |
| Shell-monitor check, counted a second way | collapsing whitespace with `tr` and counting with `rg`: old guide 2 matches, new guide 0 |
| Task 3 `<verify>`, verbatim | rc 0. Named test `1 passed`. Control: the old diagram matches `advance --phase` |
| `docs/guides/configuration.md` unchanged | `git diff --stat` over it is empty |
| Plan-level verification (`git log --format=%s -n 200 -- OPERATIONS.md \| rg -q -x -F '<subject>'`) | rc 0. Control: the same check with `Linux` in place of `macOS` gives rc 1. `b629cd0` touches `OPERATIONS.md` |
| `cargo fmt --all --check`, `cargo clippy -p devflow-core --all-targets -- -D warnings` | rc 0 before each code commit |
| `scripts/check.sh all` (once, at `4479785`, real HOME) | `==> check.sh: all OK`, `check_all_exit=0`. Over 39 test binaries: 1633 passed, 0 failed, 3 ignored |

**What these checks do not establish:**
- **The scans pin phrases, not meaning.** A wrong sentence that holds the fragments passes. I checked each operator-facing claim against the code while writing:
  - `commands.rs`: `stop_in`, `stop_via_socket`, `shutdown_supervisor`, `run_backstop`, `cleanup`, `recovery_hints`, `check_supervisor`, `refuse_a_live_supervisor` and `unlistable_socket_dir_refusal`;
  - `supervisor.rs`: `SocketPath::new`, `ensure_socket_dir`, `publish_with` and the `SUN_PATH_MAX_*` constants;
  - `runtime.rs`: `install_signal_pipe`;
  - `registry::cache_dir`, `launch_context`, `resolve_launch_shape`, `stream_launch_enabled`, the preflight's `unattended_launch_shape_condition`, and `Gates::poll_response_cancellable`.

  That checking is a read, not a test.
- **`claims_macos_support` matches only two phrasings** (`supports macos` and `macos is supported`). "macOS works" or "runs on macOS" would pass it.
- **The ARCHITECTURE check is scoped to the `## Monitor daemon` section.** The gate section above it is not checked; see deferred items.
- **`scripts/check.sh all` ran with the real HOME,** not the sentinel-HOME variant some earlier plans used. Afterwards, `~/.cache/devflow/sock` did not exist (a read-only check).
- **The container parity check (`scripts/check-in-container.sh`) was not run.** This plan changes docs and a `#[cfg(test)]` module only.

## Files Created/Modified

- `OPERATIONS.md`: the stop row, the internal-command notes, the `## Run liveness` section, the `status`/`doctor` rows, the gate pickup sentence, the legacy-launch row and the inventory.
- `ARCHITECTURE.md`: `## Monitor daemon`.
- `docs/guides/unattended-mode.md`: condition 2's reason and known limitation 2's reason and cause.
- `docs/diagrams/agent-lifecycle.md`: the intro and the sequence diagram.
- `crates/devflow-core/src/doc_check.rs` (test-only module, no `pub` item):
  - helpers `strip_html_comments`, `missing_fragments`, `markdown_section`, `mentions_shell_monitor`, `claims_macos_support`, `monitor_section_problems`, `files_under` and `read_doc`;
  - the two tests.
- `.planning/phases/50-addressable-monitor-liveness/deferred-items.md`: one new open entry covering four out-of-scope items.

## Decisions Made

See `key-decisions`. The `cleanup` choice answers the orchestrator's carried-forward question. I described `stop` as the right verb, and said the CLI's own refusal text is out of step. I did not leave cleanup's refusal undocumented.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Doc accuracy] Stale OPERATIONS.md text outside the named rows**
- **Found during:** Task 1
- **Issue:** Four places described the retired mechanism:
  - the gate paragraph said pickup "can take up to ~60s" (a supervisor's gate wait reads the response every second, per 50-13);
  - the `DEVFLOW_CLAUDE_LEGACY_LAUNCH` row named "the `sh` monitor";
  - the inventory listed `phase-NN-monitor-stop`, which nothing writes or removes any more;
  - the doctor row listed only two of the six `--json` keys.
- **Fix:** I reworded each to the shipped behaviour, after checking it in `gates.rs`, `pipeline_launch.rs`, `agent_result.rs` (`rg stop_marker_path`) and `doctor_json_body`.
- **Committed in:** `e42651a`

**2. [Rule 1 - Doc accuracy] "Claude-only" in the unattended-mode guide**
- **Found during:** Task 2
- **Issue:** The guide said only the Claude launch path starts the guard and that `--mode auto` "remains Claude-only". `stream_launch_enabled` and the preflight both allow Antigravity on the stream shape.
- **Fix:** The guide now says Claude and Antigravity. Codex, OpenCode and the legacy opt-out are the capture-shape launches that are refused.
- **Committed in:** `b629cd0`

**3. [Scope note] The changelog subject went on the larger Task 2 commit**
- The plan requires that exact subject on Task 2's commit. That commit also carries the ARCHITECTURE.md, guide and `doc_check.rs` changes, as the plan specifies. The body lists them. The subject was not reworded.

---

**Total deviations:** 2 auto-fixed (both doc accuracy), plus 1 scope note.
**Impact on plan:** No code outside the `#[cfg(test)]` `doc_check` module changed, so there is no semver impact. Nothing from the deviations went beyond the four docs the plan names.

## Issues Encountered

- Pre-existing leaky tests (backlog 999.122) left a `sleep 30` behind twice:
  - pid 485716, `agent::tests::discover_stray_devflow_processes_finds_a_monitor_wrapper`, after the Task 2 gate;
  - pid 513923, after `check.sh all`.

  I confirmed each through `/proc/<pid>/environ` (`CARGO_MANIFEST_DIR` in this worktree), then killed it by its recorded PID and confirmed it was gone.

## Known Stubs

None.

## Threat Flags

None. The changes are docs and a test-only scan. T-50-44 (overclaiming macOS or a migration path) and T-50-66 (docs still describing the shell monitor or a forked advance) are mitigated by `doc_check::docs_describe_the_socket_supervisor_not_the_shell_monitor`.

## Cleanup

- **Processes:** the two leaked `sleep 30`s above, both killed and confirmed gone. No `__monitor` from this worktree is running.
- **Files:** the scratch directory `exec-50-17/` (start time, gate log, `check.sh` log) is removed after this SUMMARY is committed.
- **Left for the orchestrator:** `.planning/user/errors/state-stale.json`, if a hook writes it, is left uncommitted as instructed. STATE.md, ROADMAP.md and REQUIREMENTS.md were not touched.

## User Setup Required

None.

## Next Phase Readiness

- **50-20:** carry the release note. The `b629cd0` subject, the merge-commit rule, and the 3.0.0 section check before and after the release are in its handoff. The API-break list above includes the ungated `spawn_monitor` return-type change.
- **The deferred-items entry** (preflight refusal text, cleanup refusal verb, `stop_marker_path`, ARCHITECTURE gate section) needs an owner. None of it blocks the phase.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-18*

## Self-Check: PASSED

- FOUND: all five modified docs/source files and this SUMMARY.
- FOUND: `e42651a`, `b629cd0`, `4479785` (task commits) and `43a1934` (SUMMARY commit) are ancestors of HEAD.
- Ledger: `git rev-list --count a7d918a..4479785` = 3, matching `commits: 3`.
- Both new test functions exist in `doc_check.rs`; `OPERATIONS.md` contains `unverified on macOS`.
