# Phase 47 Retrospective — and the Integration Session That Followed

**Written:** 2026-09-13, at the operator's request, so it can be reviewed in a later session with a clean context.
**Status:** OPEN. Decisions D1 and D2 have not been made. D3 is due before Phase 49 planning.
**Scope:**
- Phase 47 (Unattended Decision Policy Consistency), from context capture on 2026-09-09 to completion on 2026-09-13.
- The integration session on 2026-09-13: merging to `develop` via PR #213, the `deny.toml` dependency-checks PR #214, and housekeeping.
- The operator's question of whether the personal `workspace/denniyahh` branch helps or hurts productivity.

**How to use this document:** read §1 and §2 first. Sections 3–8 hold the evidence behind them, §10 says what this document does not establish, and §11 has commands to re-check any number here. The operator's complaint was: "This phase in particular felt extremely unproductive because of numerous and continual errors that were only found in adversarial reviews."

---

## 1. Summary

1. **The time went into verifying the verification, not into writing the product.**
   - Product code for plans 01–05 took about 4 hours (13:14–17:07 on 09-11). The gap plans 06–07 took about 12 minutes.
   - Review, fix and re-verify loops took roughly 2.5 days of wall clock before and after that.
   - Of the 75 phase commits, 47 are `docs`, 14 `test`, 7 `fix`, 5 `feat`, 1 `refactor` and 1 `ci`.
2. **The largest defect class was the plans' own check commands, not DevFlow's code.**
   - 23 defects were in `<automated>` verification commands, plus 5 other plan-text defects, found across four plan-review rounds, execution and the gap-plan review (§4.1).
   - Nothing ran a check command before a reviewer read it, so reviewers acted as the test runner, one round at a time.
   - Two of the three plan fix rounds introduced new defects.
3. **The one expensive code-level miss was a conflict between two contracts.** CR-01: the new policy demanded reasoning in the final message, while the completion protocol requires that message to be exactly one result line. No planning artifact listed the contracts the change touched, so code review found it after execution. The gap-closure loop it caused took about 17 hours of wall clock (§4.2).
4. **Written rules did not stop recurrence; an enforced rule did.**
   - Bash-only syntax in check commands broke all three Phase 46 plans. Since the pre-commit lint (`3d7f723`, 2026-09-06), all 20 Phase 47 check blocks comply, and no Phase 47 record reports that class.
   - The `--exact` bare-name trap and the `rg -c` empty-output trap were documented beforehand and recurred anyway (§5).
5. **The personal workspace branch is a net cost in its current form.** The cost comes from the personal *code and config* kept on it, not from the planning docs: 27 non-planning files and +1,436 lines diverge from `develop`. Every conflict and rework step in the integration session traced back to maintaining two lines (§6).
6. **The integration session's own errors were almost all in one-off shell probes**, run in a zsh shell while written for bash (§4.6). Controls caught all of them before harm, but each cost a round trip.

---

## 2. Open decisions (take them in this order)

### D1 — Workspace branch structure (framed as: decide before Phase 48 work)

| Option | What changes | Cost / unknowns |
|---|---|---|
| **A. Minimal-divergence workspace** (recommended) | `workspace/denniyahh` becomes `develop` + `.planning/` + the graphify snapshot only. Each of the 27 divergent non-planning files (§6.1) is upstreamed to `develop`, moved out of the repo tree, or dropped. A guard fails whenever a shared file diverges again. | One-time: upstream PRs and a per-file keep/upstream/drop call. `.gitignore` must still differ at least for `.planning/`, because `develop` ignores it wholesale. **Not established:** whether GSD's commit helper can work with force-tracked ignored paths, which would let `.gitignore` match `develop` exactly. |
| B. Put `.planning/` on `develop` | All split mechanics disappear: no cut script, no exclusions, no sync conflicts from planning. | The repository is **public**, so this publishes 1,054 planning files: operator decisions, review transcripts, and 674 lines containing the home path across 97 files. It reverses the 2026-08-22 split (`2a2ce97`, `fad1ef5`). |
| C. Planning in a separate private repo | Code gets a single line. | **Not established:** GSD commits planning files through the project repository (`gsd-tools query commit`), so a nested or separate repo needs a spike first. |
| Keep as is | — | Everything in §6.2 recurs at every integration. |

### D2 — Which process improvements to adopt, and when

Details and evidence are in §7. Recommended now: **P1** and **P2**. Trial **P3** and **P4** during Phase 48. Do **P5** after P1 lands.

| # | Improvement | Targets |
|---|---|---|
| P1 | Check commands as files, dry-run before review, plus a trap linter | §4.1, the largest class |
| P2 | Tested probe helpers, and run probes under bash | §4.6, and the zsh traps in §5 |
| P3 | Contract inventory at plan time | §4.2, the CR-01 class |
| P4 | One automated fix round per review, then the operator decides | §4.1 (fix rounds that introduced defects) |
| P5 | Prune standing context once rules are enforced | §5 (about 52 KB loaded every session) |

(In the chat these were items 2, 3, 4, 5 and 7. Item 1 is D1, and item 6 moved into D3.)

### D3 — Backlog promotions (due before Phase 49 planning; also recorded in STATE.md)

- **999.125** (preflight and resume disagree on where a `blocking-human` gate exists) and **999.126** (a checkpoint added after Code preflight is never re-scanned). Both concern whether an unattended run can reach the resume route past a human gate, which is the path Phase 49's live run observes.
- **999.38**, the test-suite PATH race (GitHub #181). It is a recurring source of red runs and review findings (§4.3).

---

## 3. Timeline (local time, UTC-4)

Source: `git log` on `workspace/denniyahh` (command in §11).

| When | Commit | Event |
|---|---|---|
| 09-09 21:43 | `6761f73` | Phase context captured |
| 09-09 22:05 | `1bc5d8c` | CONTEXT revised after external adversarial review: 10 of 16 decisions changed |
| 09-10 09:53 | `eea8628` | Operator decisions and phase research recorded |
| 09-10 12:46 | `59fb3ec` | Plans 47-01…05 created |
| 09-10 18:41 | `f497e9d` | Plan round 1 (gsd-plan-checker): 1 blocker, 6 warnings fixed |
| 09-10 20:49 | `3896cf0` | Plan round 2 (agy + hermes; codex dropped at its usage limit): three dead check commands fixed |
| 09-11 09:51 | `ada299d` | Plan round 3 (codex, sandboxed): 7 defects fixed; two of them came from round 2's fixes |
| 09-11 13:14 | `4dbba5c` | Execution starts |
| 09-11 13:42 | `871c5f3` | Executor fixes 4 `--exact` filters that correct work could never pass |
| 09-11 17:07 | `1d53443` | Plans 01–05 done |
| 09-11 17:15 | `e6ab767` | Code review: CR-01 (critical), WR-01 (warning) |
| 09-11 19:55 | `b3b47ea` | Verification: `gaps_found`, 19/23 |
| 09-11 21:38 | `e69fe3d` | Gap plans 47-06 (CR-01) and 47-07 (WR-01) |
| 09-11 22:45 | `ead8443` | External review of the gap plans (agy + codex; DeepSeek lane timed out): 10 findings |
| 09-12 10:00 | `bf49a77` | Gap-plan findings addressed |
| 09-12 10:02–10:11 | `b33406c`…`b331719` | Gap plans executed |
| 09-12 10:28 | `95ee5a7` | Re-verified |
| 09-12 12:08–13:27 | `a2141e2`, `dd00c30`, `aba3cee`, `b3529a8` | Pre-UAT review: agy's 5 findings checked, then a GSD code re-review with 7 findings (2 warnings, 5 info) |
| 09-12 17:30–17:39 | `67cab06` `bbf696d` `c3de608` `bd7bd7c` `ae9970a` `9a2c7fe` `5385022` | Review fixes, including corrections to overclaims in the summary and verification |
| 09-12 18:18 | `c4d7231` | False commit-claim BLOCKER from `verify-work` waived, filed as gsd-core#4670 |
| 09-12 19:15–19:26 | `ac3ee42` `c66cba2` `2790678` `49bb324` `8d60ff8` | Final external code review (codex + agy): 4 findings, 4 fixes |
| 09-12 20:09–20:34 | `0afbff5` `96faa54` `dd873a6` | UAT 24/24 passed, Nyquist validation, security verification |
| 09-13 02:39 | `7400042` | Worktree-guard bypass (names with quotes or non-ASCII characters) found while measuring T-47-16; fixed |
| 09-13 03:03–03:10 | `3fa9206`, `4bb9504` | Re-verified at 39/39 with one operator-accepted override; phase complete |
| 09-13 ~12:20–18:30 | `57c018c` … `a6524bf` | Integration session: see §6.2 and §9 |

**Where the wall clock went.** These figures include overnight gaps and idle time, so they are not effort hours.

| Stretch | Wall clock |
|---|---|
| Context capture to first code commit | ~39.5 h |
| Plans 01–05 execution | ~3 h 53 min |
| First code review to phase complete | ~34 h |
| — of which the CR-01 gap loop (code review to re-verify) | ~17 h |
| Integration session | ~6 h |

---

## 4. Error classes, with evidence

### 4.1 Plan check commands and plan text (the largest class)

The classification is mine, taken from `47-REVIEWS.md`, the commit bodies and the plan summaries. "Check defect" means an `<automated>` command that could never fail, could never pass on correct work, or could not see what it claimed to check.

| Where found | Check defects | Other plan defects | Examples |
|---|---|---|---|
| Round 1 — gsd-plan-checker (`f497e9d`) | 5 | 2 | **Unpassable:** a full-suite gate on correct work, because the snapshot it drifts was re-blessed only in a later task. **Dead:** a grep for a sentence split across a `\` continuation. **Missing:** no failing gate for D-02's narrowness. **Already satisfied:** a `Fix shape:` count. **Brittle:** extraction windows. Other defects: a false cargo-machete claim, and a `47-VALIDATION.md` filename collision. |
| Round 2 — agy + hermes (`3896cf0`) | 4 | 0 | **Collapsed quoting:** the `sed` continuation-join inside `bash -c '…'` never ran, because bash collapsed its backslashes; it read 0 before and after the fix. **Blind:** `git status --porcelain` cannot see the gitignored `.snap.new` it was meant to catch. **Already satisfied:** a `package-verification` count, by an existing test's own strings. **Evadable:** the D-02 token blacklist, 15 ways. |
| Round 3 — codex (`ada299d`) | 6 | 2 | **Unpassable:** a D-02 test whose pinned sentence contained two of its own forbidden words. **Missed cases:** a waiver in a sentence that never names package verification. **Too narrow:** 1 of 3 protected tests diffed; file selection in `ls` order. **Blind to staged changes:** `git diff` in "no residual change" checks. **Incomplete:** checks skipping three stated requirements. Other defects: a threat row citing a dropped check, and an instruction to `concat!` a `const`, which cannot compile. |
| Execution (`871c5f3`, `e3f4fe2`, 47-05) | 6 | 0 | Four `cargo test --exact` filters used bare leaf names (measured: `0 passed; 764 filtered out` and `0 passed; 363 filtered out`). A vacuous `git check-ignore` negative control. `rg -c` printing nothing on zero matches, which broke integer checks. |
| Gap-plan review (`ead8443`) | 2 | 1 | **Blind to insertions:** a region gate, after lines 55/56 (reproduced). **Too weak:** a snapshot gate passing a wrong two-line edit (reproduced). Other defect: an intermediate red commit between `feat(47-06)` and its re-bless. |
| **Total** | **23** | **5** | |

**What made this class expensive:**
- A check command is a shell program embedded in markdown. The planner wrote them unexecuted; round 1's fix was "verified" against a separately escaped heredoc copy, not the bytes written into the plan (`3896cf0` says so). No review lane ran `cargo` against the repository.
- **Two of the three fix rounds introduced new defects.** `47-REVIEWS.md` itself calls the fix text "the least-trustworthy text in these plans". Only round 3 tested fixes from the exact bytes spliced into the plan files.
- **Review-lane unreliability added verification work:**
  - codex hit its usage limit (round 2) and the DeepSeek lane timed out (gap-plan review);
  - a lane wrote into `.gitignore` despite a read-only instruction;
  - agy quoted `fatal: detected dubious ownership` for a run that never printed it;
  - every finding had to be re-verified in session.

### 4.2 Contract conflicts nobody listed at plan time

- **The conflict.** CR-01 (`e6ab767`): the delivered policy and resume instruction required substantive reasoning in the final message, but `COMPLETION_PROTOCOL` requires that final message to be exactly one `DEVFLOW_RESULT` line.
- **The cost.** Verification went to 19/23 (`b3b47ea`), which led to gap plan 47-06, a gap-plan review, execution and re-verification: about 17 hours of wall clock.
- **The follow-on detector issues** found later, still in this contract area: a human gate only matched on a `Gate`-labelled line (`bd7bd7c`); list and blockquote markup missed (`ac3ee42`); a literal `\n` read as a line break (`49bb324`).
- **Why it wasn't caught at plan time.** No planning artifact inventoried the existing contracts the new text interacts with: the completion protocol, the result parser, the gate detector. Plan-level reviews looked at the plans' own tasks.

### 4.3 Test isolation and environment leakage (the 999.37 / 999.38 family)

- **WR-01:** the worktree-guard test fixture inherited and ran the operator's global git hooks and signing.
- **The fix broke CI.** `67cab06` nulled global git config before the harness's one query against the real checkout. In CI's root container, git accepts that checkout only through a global `safe.directory`, so the harness exited 128 with no output (found by the final review, fixed in `c66cba2`).
- **Git environment escape:** an exported `GIT_OBJECT_DIRECTORY` could write into the real repository (closed in `67cab06`).
- **PATH-race flakes:** two deferred items. One is devflow-core's `PathGuard`, still open under 999.38. The other is devflow-cli's empty-PATH test, mitigated by #212.
- **2026-09-13 recurrence:** a host `scripts/check.sh all` had the `devflow` bin report 296 passed and 68 failed (`preflight` 38, `staleness` 23, `pipeline_outcomes` 4, `test_support` 3). Every failure was a spawn `NotFound` or a git/sleep-unresolvable assertion, and the immediate re-run passed 364/0. The tree contained #208's Phase 46 `env_lock()` fix. Recorded in ROADMAP 999.38 and on #181.

### 4.4 Records claiming more than was done

- A round-2 fix commit claimed a fix it had not made (`ada299d` notes this).
- `9a2c7fe` corrected overclaims in the 47-06 summary and in the verification report.
- Four must-have texts went stale against HEAD as later fixes changed literals (VERIFICATION "What changed").

### 4.5 GSD tool defects, each corrected by hand

- `begin-phase` mis-writes (`c4f4ca1`).
- `state.planned-phase` left STATE frontmatter wrong (`e69fe3d`).
- Two code-review workflow defects worked around (`b3529a8`).
- A false `verify-work` commit-claim BLOCKER (gsd-core#4670, `c4d7231`).
- The verification digest covering shared planning docs, which staled the very phase `phase.complete` was completing (gsd-core#4623).

### 4.6 The integration session's own errors (Claude, 2026-09-13)

The Bash tool runs zsh: `$0` is `/usr/bin/zsh`, `$SHELL` is `/usr/bin/zsh`, and bash was not running.

| # | Error | Caught by | Cost |
|---|---|---|---|
| 1 | The PR-check wait loop matched `Build + test in devcontainer` as an unescaped regex. `+` is a quantifier, so the count stuck at 3 of 4 and the loop could never exit. It had been "validated" with an escaped copy. | Loop never finished; the OS killed it under memory pressure | ~30 min; checks read directly instead |
| 2 | zsh's `$P:scripts` colon modifier caused "bad substitution" in a reference check | Error output | One re-run under bash |
| 3 | `cargo deny check --disable-fetch` is not a valid flag, so both control runs exited 2 with a usage error | Asserting on output text, not exit code | One re-run |
| 4 | `echo ===X` triggered zsh `=` expansion | Output check | One re-run |
| 5 | A count came back blank because ANSI colour codes split the matched text | Noticed a blank count | Minor |
| 6 | Nested heredoc quoting broke the #181 comment command (nothing posted) | Parse error | One retry with a duplicate guard |
| 7 | A combined `git log -- a b c` attributed `e3f4fe2` to the wrong file | Per-file query | Minor |
| 8 | `awk` picked the wrong `test result` line (a filtered bin run) | Cross-check | Minor |
| 9 | A draft checklist note overclaimed ("fails every crate") | Self-review before commit | Minor |

**Also friction, but not errors:** the auto-mode classifier denied a combined `git restore` + `git rm` (redone with Edit plus a separate `git rm`), and host memory pressure (swap full) killed a background job.

---

## 5. Why the errors surfaced only in adversarial review — root causes

- **R1. Check commands are code that is never run before review.**
  - Plans embed shell programs in markdown. The plan-checker reads them and external lanes read them, but the first execution is the executor's.
  - Fix rounds edited them without re-running them until round 3.
- **R2. Written rules don't prevent recurrence; mechanical checks do.**

  | Trap | Recorded in | Recurred? |
  |---|---|---|
  | `cargo test -p devflow --lib` verifies nothing | CLAUDE.md; Phase 35 shipped it into 21 check blocks | Documented after Phase 35 |
  | `cargo test --exact <name>` with a bare name matches nothing | CLAUDE.md habit #1 | **Yes:** 4 Phase 47 checks that could never pass |
  | `rg -c` prints nothing on zero matches | CLAUDE.md, after Phases 43 and 44 shipped a dead `rg -c \| rg '^0$'` | **Yes:** 47-05's integer checks |
  | `${PIPESTATUS[0]}` is empty under zsh | CLAUDE.md, after all three Phase 46 plans hit it, **plus pre-commit lint `3d7f723`** | **No:** 20/20 Phase 47 blocks comply, and no Phase 47 record reports it |
  | The zsh `$VAR:path` colon modifier | Memory `project-zsh-colon-modifier-trap` | **Yes:** §4.6 #2 |
  | Control fails for the wrong reason | Memory `feedback-control-must-fail-for-the-right-reason` | Caught in time (§4.6 #3) |

  **Standing context loaded every Claude Code session**, measured 2026-09-13: about 52 KB, roughly 13k tokens at ~4 bytes per token (an estimate):
  - global rules at `~/.config/agents/AGENTS.md`: 20,347 bytes;
  - project `CLAUDE.md`: 18,672 bytes, including 11 "verification habits" bullets;
  - `MEMORY.md`: 13,313 bytes, 43 entries.

  One memory file, `project-gsd-execute-devflow-quirks.md`, is another 20,363 bytes. Each new incident adds prose; the prose gets longer and less salient.
- **R3. Enforcement moved the trap without removing it.** The lint forces every check into `bash -c '…'`. Inline `bash -c` quoting is exactly what collapsed round 2's `sed` backslashes (`3896cf0`). Inline shell in markdown is the underlying hazard.
- **R4. Integration questions were deferred to the end:**
  - the contract inventory (CR-01);
  - CI container semantics: the harness running as root on a runner-owned checkout;
  - which files are shared with `develop`: the harness was wired into shared files, then unwound on 2026-09-13.
- **R5. Each stage re-verifies the previous stage's records by hand.** Every external finding needed in-session verification, and every GSD tool defect needed a hand correction and a write-up (§4.5).

---

## 6. Workspace branch assessment

### 6.1 Facts (measured 2026-09-13)

- **Visibility:** `denniyahh/devflow` is **PUBLIC**.
- **Why the split exists.** `2a2ce97` (2026-08-22, "isolate personal dev environment artifacts to workspace branch", no body) and `fad1ef5` (2026-08-23): `.planning/` stays gitignored on `develop`/`main` as shared history, and is force-tracked on the workspace branch "as a personal planning record". No productivity rationale is recorded.
- **`.planning/` on workspace:** 1,054 tracked files. 674 lines in 97 files contain the home path; 0 contain `claude.ai/code/session_` URLs.
- **Non-planning divergence from `develop`** (excluding `.planning/` and `graphify-out/`): 27 files, +1,436/−66 lines.
  - **Agent and tool config:** `.agents/rules/graphify.md`, `.agents/workflows/graphify.md`, `.claude/settings.json`, `.claude/settings.local.json`, `.codex/hooks.json`, `.cursor/rules/graphify.mdc`, `.opencode/opencode.json`, `.opencode/plugins/graphify.js`, `AGENTS.md`, `CLAUDE.md`, `skills-lock.json`.
  - **Personal workflow scripts and tests:** `scripts/cut-pr-branch.sh`, `scripts/sync-workspace.sh`, `scripts/phase-worktree.sh`, `scripts/lint-phase-worktree.sh`, `scripts/test-phase-worktree-guard.sh`, `crates/devflow-cli/tests/worktree_guard_harness.rs`.
  - **Shared files with personal edits (the conflict surface):** `.gitignore`, `Cargo.toml`, `crates/devflow-cli/Cargo.toml`, `crates/devflow-core/Cargo.toml` (the workspace-only lints block from `22d9ff3`), `CONTRIBUTING.md`, `scripts/check-in-container.sh`, `scripts/hooks/commit-msg`, `scripts/hooks/post-commit`, `scripts/hooks/pre-commit`, `.devcontainer/devcontainer-lock.json`.
- **Cadence since 2026-08-14:** 7 syncs of `develop` into workspace; 40 PR merges into `develop`.

### 6.2 What the split cost in the 2026-09-13 integration

Every conflict and rework step below traced back to maintaining two lines:

1. **Step 2b rework.** The worktree-guard harness had been wired into shared files, which `develop` would receive without the harness scripts. It had to move into a workspace-only test: `check.sh` restored to `4dbba5c`, `ci_parity_guards.rs` restored to `c66cba2^`, `check_script_run_test.rs` deleted (`8c41c46`). One consequence: `develop`'s `check.sh` now permanently lacks the harness call.
2. **Develop cut.**
   - 21 code commits replayed onto `develop`, with 7 phase commits plus the step-2b commit excluded.
   - One hand-resolved commit (`4dbba5c`: `.gitignore`, `Cargo.toml`, `crates/devflow-core/Cargo.toml`).
   - Then tree, lockfile and reference verification.
3. **Sync conflicts** in the same 3 files when merging `develop` back into workspace (`f5681da`).
4. **The `deny.toml` / dependency-checks detour.** `deny.toml` had lived only on workspace since `22d9ff3`, so the parked CI job needed a separate PR (#214). A later fix-branch merge hit a duplicate dev-dependency line, because #212's changes reached workspace by two routes.
5. **Four container-gate pushes** instead of one or two.
6. **Standing rules the split forces:**
   - base phase worktrees on workspace, not `develop` (CLAUDE.md said `develop` until 2026-09-04, which was wrong);
   - sync before branching;
   - graphify snapshots only on workspace;
   - the cut script's forbidden-path list.

   Related memories: codex planned against a stale `.planning/` in the main checkout (`b102898`), and workspace commits went unpushed for days.

**Correction to the chat version.** The chat said the planning-only commits "skipped with zero conflicts". That is true by construction: they are never replayed, and `develop` has no `.planning/`. It shows planning divergence doesn't cause *merge conflicts*, not that it's cheap. Its costs are the standing rules and the cut/replay process.

**Not attributable to the split:** sequential-only executor dispatch. The harness forks executor worktrees from `origin/HEAD`, which never contains a phase branch's commits, whatever `.planning/` placement is chosen.

### 6.3 Assessment

- **The costs concentrate in personal code and config sitting in or next to shared files.** Those are what conflict, get referenced by shared files, and need exclusions and rework.
- **The benefit is real:** a clean public history, with no planning transcripts or home paths published.
- **Hence D1 option A:** keep only content that cannot conflict with code on the workspace branch, and give everything else one home.

---

## 7. Improvement proposals (for D2)

### P1 — Check commands as files, dry-run before review, plus a trap linter (recommend now)

- **What:**
  - Each `<automated>` check becomes a `.sh` file next to the plan, run with `bash`. No inline `bash -c` strings, which removes R3.
  - A dry-run step at the end of plan-phase, before any review, runs every check on the base commit:
    - it must pass `bash -n` and `shellcheck`;
    - red-first checks must FAIL, with the intended failure text recorded, not just a non-zero exit;
    - invariant checks must PASS.
  - A lint refuses known trap signatures:
    - `cargo test … --exact` without a module-qualified name;
    - `rg -c … | rg '^0$'`;
    - `git diff` without `--cached`/`HEAD` in residual-change checks;
    - `git status --porcelain` used for ignored paths;
    - `cargo test -p devflow --lib`.
- **Evidence it targets:** §4.1 and R1–R3. Mapped against Phase 47 history:
  - the linter matches about 8 of the 23 check defects: the 4 `--exact` filters, the collapsed `sed`, the porcelain-ignored check, the staged `git diff`, and `rg -c`;
  - the dry-run, requiring intended failure text, catches about 4 always-pass checks (the continuation grep, two already-satisfied counts, the dead `sed` join).
  - Combined: roughly 11–12 of 23. **This is a retrospective fit and has not been tested.**
- **Does not catch:** checks that correct work cannot pass. Those need a correct fixture or mutation, which is a larger step.
- **Cost:** medium. Needs a red-first/invariant marker per check and an extractor. The existing `scripts/lint-plan-bashisms.sh` and its pre-commit wiring are the pattern to extend.

### P2 — Tested probe helpers, and run probes under bash (recommend now)

- **What:**
  - Write once, test in both directions, then reuse, instead of improvising shell each session:
    - wait for a PR's required checks, with literal names, a pinned HEAD and a control;
    - trial merge and replay;
    - count with an independent second count.
  - Make ad-hoc probes run under bash.
- **Evidence:** §4.6. The PR wait loop was written twice this session and was broken the first time. Most of those errors were zsh semantics or one-off quoting.
- **Unknown:** how to make the Bash tool use bash.
  - The Claude Code docs document `CLAUDE_CODE_SHELL_PREFIX`, a wrapper around Bash-tool commands.
  - No setting that selects the Bash tool's shell was found; that was one context7 query, not an exhaustive search.
  - Whether launching Claude Code with `SHELL=/usr/bin/bash` switches it is untested. One command answers it.
- **Cost:** small.

### P3 — Contract inventory at plan time (trial in Phase 48)

- **What:** plan-phase research lists every existing contract or invariant the change touches, each with `file:line`, and the plan-checker confirms each one has a test. For example: the completion protocol, the result parser, the gate detector, CI's container user and ownership, and which files are shared with `develop`.
- **Evidence:** §4.2 and R4. It plausibly catches CR-01 before execution, and the 2026-09-13 shared-file rework.
- **Limit:** judgment, not mechanics, so its value depends on the planner's thoroughness.
- **Cost:** a template/checklist change.

### P4 — One automated fix round per review, then the operator decides (trial in Phase 48)

- **What:** keep both adversarial reviews (memory `feedback-adversarial-review-after-definition-and-plans`), with P1's dry-run running before them. After one fix round, the operator decides scope instead of starting another automated round.
- **Evidence:** two of three plan fix rounds introduced new defects (§4.1). Memory `project-irreversible-ops-need-review-not-tests` already recommends stopping after one automated fix round.
- **Trade-off:** every review pass in Phase 47 found real defects. The aim is to move detection earlier (P1, P3), not to skip reviews.

### P5 — Prune standing context once rules are enforced (after P1)

- **What:** when a rule becomes a lint or hook, replace its prose in CLAUDE.md or memory with a one-line pointer. Merge or retire phase-specific memory entries.
- **Evidence:** R2: about 52 KB per session, and growing.
- **Cost:** small. Overlaps the pending todo "Review and normalize agent instruction files across tools".

---

## 8. Proposal-to-evidence map (quick reference)

| Finding | P1 | P2 | P3 | P4 | P5 | D1-A | D3 |
|---|---|---|---|---|---|---|---|
| §4.1 check defects | ● | | | ● | | | |
| §4.2 contract conflicts | | | ● | | | | ● (999.125/126 are gate-contract defects) |
| §4.3 test isolation | | | | | | | ● (999.38) |
| §4.6 probe errors | | ● | | | | | |
| §5 R2 prose growth | ● | | | | ● | | |
| §6.2 integration tax | | | ● (shared-file inventory) | | | ● | |

---

## 9. Already done on 2026-09-13 (no action needed)

- **Phase 47 on `develop`:** PR #213 (merge `c412170`), after the four required checks passed on its head `5301b88`.
- **Phase 47 into workspace:** merged (`8795853`), `develop` synced (`f5681da`), graphify snapshot `ae5b3b3`, pushed.
- **Dependency checks:** `deny.toml` and the advisory Dependency checks job are on `develop` via PR #214 (merge `add41a6`). Both job runs printed `advisories ok, bans ok, licenses ok, sources ok` and found no unused dependencies. Without `deny.toml`, `cargo deny check` on `develop`'s tree exits 4 with `licenses FAILED` (178 rejections).
- **Workspace brought current:** `develop` synced again (`335992d`), fix branch merged (`2780d0b`).
- **Docs:** `STATE.md` refreshed, ROADMAP 999.38 recurrence paragraph, checklist `deny.toml` note (`ad7678c`); snapshot `c492827`; pushed.
- **Checklist corrections (`a6524bf`):** the signing-key roles and the `gh` token scopes.
- **GitHub #181:** PATH-race recurrence comment posted once.
- **Removed:**
  - the obsolete untracked `.planning/user/errors/state-stale.json` (written by `scripts/hooks/post-commit`, read by nothing);
  - worktrees `phase-47`, `phase-47-pr`, `chore-ignore-graphify-output`, `chore-dependency-checks` and `fix-test-support-and-dependency-checks`, with their branches.
- **Phase 48 worktree:** `.worktrees/phase-48` on `feature/phase-48`, fast-forwarded to the workspace tip.
- **Memory:** `feedback-agent-merges-to-develop-ok`. Agents may merge to `develop`; merges are recorded as `denniyahh` through the `gh` keyring token. Both rulesets have had 0 required approvals since creation on 2026-07-23; nothing was relaxed. Branch commits are signed with the agent key `devflow_signing_ed25519`, not the maintainer's `github_ed25519`.
- **Todos captured in `.planning/todos/pending/`** (all `minor`):
  - Review and normalize agent instruction files across tools.
  - True up development setup docs against live config.
  - Evaluate skills, plugins, MCP and ACP servers to add.

---

## 10. What this retrospective does not establish

- **The defect counts in §4.1 are my classification** of review records. Another reader could split them differently; the per-round breakdown lets anyone recount.
- **Wall-clock durations include overnight gaps and idle time.** No effort-hour data exists.
- **The P1 catch estimate (~11–12 of 23) maps history to rules.** No linter or dry-run was built or run against the Phase 47 plans.
- **The "enforced rule held" evidence is one phase** (20 check blocks). It is weak: a real signal, not a rate.
- **The 999.38 PATH race has no failure rate:** one failing run out of two locally. Which test replaces `PATH` is unknown.
- **D1 option A's `.gitignore` feasibility and option C's GSD compatibility are unverified.**
- **Whether the Bash tool can be switched to bash is untested** (P2).
- **The ~13k-token figure** is a bytes/4 estimate, not a tokenizer count.

---

## 11. Re-check commands

Run from `/var/home/denniyahh/Github/devflow`. Wrap in `bash -c '…'`, because the Bash tool runs zsh.

```bash
# Phase 47 timeline and commit-type mix
git log --reverse --date=format:'%m-%d %H:%M' --format='%ad %h %s' 6cfa869^..8c41c46
git log --format=%s 034f5b6..8c41c46 | sed -E 's/^([a-z]+)(\(.*\))?!?:.*/\1/' | sort | uniq -c

# Review records
sed -n '70,145p' .planning/phases/47-unattended-decision-policy-consistency/47-REVIEWS.md   # plan rounds
sed -n '146,270p' .planning/phases/47-unattended-decision-policy-consistency/47-REVIEWS.md  # gap-plan + final code review
git show -s --format=%B 3896cf0 ada299d 871c5f3 c4d7231                                     # root causes in commit bodies

# Enforced lint held: all check blocks bash -c wrapped
for f in .planning/phases/47-*/47-0*-PLAN.md; do echo "$f $(rg -c '<automated>' "$f") $(rg -c '<automated>bash -c ' "$f")"; done

# Workspace divergence and split rationale
git diff --stat origin/develop workspace/denniyahh -- . ':(exclude).planning' ':(exclude)graphify-out'
git log -1 --format='%h %ad %s%n%b' --date=short fad1ef5
gh repo view denniyahh/devflow --json visibility

# Rulesets: approvals have been 0 in every version
gh api repos/denniyahh/devflow/rulesets/19616771/history
gh api repos/denniyahh/devflow/rulesets/19616766/history

# Standing context size
wc -c ~/.config/agents/AGENTS.md CLAUDE.md ~/.claude/projects/-var-home-denniyahh-Github-devflow/memory/MEMORY.md

# Which shell the Bash tool runs
echo "$0 $SHELL ${ZSH_VERSION:-no-zsh} ${BASH_VERSION:-no-bash}"
```

---

## 12. Suggested agenda for the review session

1. Read §1 and §2 (5 minutes).
2. **Decide D1.** If option A: walk the 27 files in §6.1 and mark each keep, upstream, or drop.
3. **Decide D2:** which of P1–P5, in what order, and whether any must land before `/gsd-discuss-phase 48`.
4. Note D3's deadline (before Phase 49 planning); decide now if convenient.
5. Place the three pending todos relative to D1/D2. The agent-files and setup-docs todos depend on D1.
6. Then implement the adopted items, or start `/gsd-discuss-phase 48` from `.worktrees/phase-48`.
