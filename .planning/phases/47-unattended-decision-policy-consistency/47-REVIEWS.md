# Phase 47 — External Adversarial Review

**Run:** 2026-09-09
**Artifact reviewed:** `47-CONTEXT.md` (first draft, pre-revision — backed up in session scratchpad)
**Outcome:** 10 of 16 decisions revised. See the revised `47-CONTEXT.md` § Adversarial Review.

## Lane roster

| Lane | Model | Result | Depth |
|---|---|---|---|
| codex | `gpt-5.6-terra`, reasoning effort high | **completed** | 34 `file:line` citations, 148,642 tokens |
| agy (antigravity) | `gemini-3.8-flash-high`, effort high | **completed** | 30 `file:line` citations |
| opencode / hermes (DeepSeek v4 Pro) | — | **not run** | Two lanes were sufficient; roster kept to the operator's named pair for this review. |

Both lanes were probed before the run (`codex exec` → `PROBE_OK`; `agy --version` → 1.1.27;
`agy models` confirmed `gemini-3.8-flash-high` still offered). Neither lane dropped. Neither
returned a low-citation clean bill, so neither is being folded into a false consensus.

## Convergence and divergence

**Both lanes independently reached, and direct source inspection confirmed:**

1. **D-13's disjointness assumption is false** — the preflight-refusal path and the resume path
   can meet. (Both lanes: FALLS.)
2. **D-11/D-12's four-adapter live-run requirement is structurally impossible** — the resume route
   is gated on `AgentKind::Claude`.
3. **D-02's package-verification widening rests on a false premise** and is avoidable without
   arm-splitting.
4. **D-06's four/two adapter enumeration is correct** — the one claim in the draft that all three
   independent checks confirmed.

**codex found alone:** `GateAction::Advance` calls `launch_stage_inner` and skips the
just-adjudicated preflight check (`preflight.rs:1368-1376`) — the mechanism that makes the
refusal→resume path actually reachable. Also that `checkpoint_auto_decided` is emitted *before* the
spawn (`pipeline_launch.rs:1102-1108`), so D-10's evidence standard proved injection rather than
decision.

**agy found alone:** `cargo test --check` is not a valid invocation and `cargo-insta` is not
installed — D-16's specified command does not exist. Verified directly:
`cargo test --check` → `error: unexpected argument '--check' found`. Also that the primacy/position
framing in `<specifics>` is wrong for a multi-turn resumed session, where recency governs.

**Where they disagreed.** agy concluded the resume path is unreachable in `--mode auto` and that
`unattended-mode.md:29-31` should be left alone. codex found the `Advance` bypass that makes it
reachable after one human approval. **Checked against source directly: codex is right.** agy's own
report mentions the manual-approval case in passing but does not weigh it. The revised D-13 keeps
agy's correct observation that the *ordinary* auto path is closed, while fixing the doc line that
is actually false (`:85-88`, "a refusal is final").

## Verification stance

Every finding carried into the revised CONTEXT.md was re-checked against source in this
repository before being accepted — per the skill's rule that both lanes produce confident prose
regardless of what they read. Findings verified independently here: the `AgentKind::Claude` resume
gate, the single `exec_resume_command`, the preflight C2 adapter restriction, the two
non-equivalent checkpoint predicates, the `GateAction::Advance` bypass, the pre-spawn event
emission, the `cargo test --check` failure, the absent `cargo-insta`, and the adapter enumeration
(including that three `render_claude_style` references are inside `#[test]` blocks and are not
production paths).

**One claim was checked and rejected:** an apparent third and fourth production `render_claude_style`
call site at `agents/mod.rs:231,241` turned out to be inside a `#[test]` block. Had it not been
checked it would have been reported as a gap in D-07's coverage.

## Raw reports

Full lane output is in the session scratchpad (`review-agy.md`, `codex.clean`). codex writes its
review to **stderr**, not stdout — `review-codex.md` is 0 bytes and is not a failed run.

## Plan review rounds — 47-01 … 47-05

The CONTEXT review above preceded planning. The five plans then went through three more rounds. Every
round found real defects, and **two of the fix rounds introduced new defects of their own** — so the
fix text is the least-trustworthy text in these plans. Round-3 fixes were therefore tested from the
exact bytes written into the plan files, not from separate test copies.

| Round | Date | Lane | Model / mode | Outcome | Depth |
|---|---|---|---|---|---|
| 1 | 2026-09-10 | gsd-plan-checker (internal) | — | completed: 1 blocker, 6 warnings | — |
| 2 | 2026-09-10 | agy | gemini-3.8-flash-high, high | completed | 36 citations |
| 2 | 2026-09-10 | hermes | deepseek-v4-pro, `--provider deepseek` | completed, no wrong-model warning | 21 citations |
| 2 | 2026-09-10 | codex | gpt-5.6-terra | **DROPPED — usage limit** (recorded as a drop, not a pass) | — |
| 3 | 2026-09-11 | codex | gpt-5.6-terra, high, `-s workspace-write -C <scratch>` | completed | 26 citations |

**Round 1.** 47-02 Task 2's full-suite gate could not pass on correct work (the snapshot it drifts
was re-blessed only in Task 3); a grep for a sentence split across a `\` continuation could never
match; D-02's narrowness had no gate that could fail; a file-wide `Fix shape:` count was already
satisfied; several brittle extraction windows; a false cargo-machete causal claim; and a
`47-VALIDATION.md` filename collision. Fixed in `f497e9d`.

**Round 2.** Both completing lanes independently found two dead gates: the `sed` continuation-join
inside `bash -c '…'` never ran (bash collapsed its backslashes), and `git status --porcelain` cannot
see the gitignored `.snap.new` it was meant to catch. hermes alone found a `package-verification`
count already satisfied by an existing test's own strings. The D-02 token blacklist was evaded 15
ways. Fixed in `3896cf0` — which introduced round 3's D-02 defects and claimed a fix it did not make.
One agy finding (a `$pb`/`$p` typo) did not survive verification and was not acted on. **A lane in
this round wrote `*.snap.new` into the repository's `.gitignore` despite a read-only instruction;**
it was reverted, and round 3 enforced read-only access mechanically instead.

**Round 3.** codex ran sandboxed to a scratch directory. The protection was established on disk
before the run (a write into the repo failed, a write to scratch succeeded) and re-checked after it
(HEAD, status and mtimes unchanged, with a control proving the mtime check detects a new file). All
seven findings verified against source:

1. The D-02 Rust test specified in round 2 could not be passed by correct work: its pinned sentence
   contained two of the words its own token check forbade.
2. Even made passable, it missed a waiver in a sentence that never names package verification.
3. 47-03 Task 1 still diffed only one of its three protected tests.
4. 47-04's two-turn gate picked files in `ls` order, and passed identical captures when other
   snapshots sorted first.
5. Two "no residual change" checks in 47-01 used `git diff`, which cannot see staged changes.
6. 47-05's gates skipped three stated requirements: the event firing before the spawn, no percentage
   threshold, and DECN-03's claude-only limit.
7. The T-47-09 threat-model row cited a dropped gate as the mitigation for the phase's
   highest-severity risk.

Corrections to codex: it counted three forbidden words in the pinned sentence (there are two). And
my own quick check wrongly contradicted it on (6) — `DECN-03` appeared in that gate only as the end
marker of the DECN-02 extraction; codex was right.

**Operator decision, 2026-09-11:** pin the gate-rule section's exact text (recorded under CONTEXT
D-02). **Not covered mechanically:** an authorization added elsewhere in the policy, outside the
pinned section — caught only by snapshot-diff review.

**Found while fixing, not by any reviewer:** 47-03 told the executor to "interpolate" a `const`
into another `const`. `concat!` rejects a const name (`error: expected a literal`), so correct work
could not follow that instruction. The plan now specifies a `macro_rules!` literal, verified to
compile, render the pinned section, and keep the existing exclusion test passing.

**How the round-3 fixes were verified.** Each changed gate was written to a file, tested from those
bytes in scratch fixtures in both directions — every case set including one that must flip —
spliced into the plan unchanged, and byte-compared after writing:

| Plan | Gate | Correct work | Broken work | Control |
|---|---|---|---|---|
| 47-03 T1 | three protected tests | unrelated edit: all identical | mutated 2nd test: only it flagged | old gate blind to the same mutation |
| 47-03 T3 | pinned text in rendered snapshot | real insta 1.48.0 snapshot passes | widened, missing, duplicate all fail | — |
| 47-04 T2 | turn pair selected by name | correct pair with decoys passes | both-turn-1, both-turn-2, swapped, missing all fail | old gate passes the decoy attack |
| 47-01 T2/T3 | `git diff HEAD` | clean reads 0 | staged change reads 1 (old form: 0) | unstaged reads 1 either way |
| 47-05 T2/T3 | before-spawn, no `%`, DECN-03 | compliant passes | missing, `%`, uncited all fail | current REQUIREMENTS.md fails |

**Not established by any of this:** no `cargo` was run against the repository, so nothing here shows
the plans compile or that the specified Rust tests behave as written at runtime; that is the
executor's first real evidence.

## Gap-plan review — 47-06, 47-07

**Run:** 2026-09-11
**Artifacts reviewed:** `47-06-PLAN.md` (CR-01 wording fix) and `47-07-PLAN.md` (WR-01 fixture
isolation), at `e69fe3d`
**Inputs given to every lane:** both plans, `47-VERIFICATION.md`, `47-REVIEW.md`, and the operator
decisions (CR-01 wording-only; WR-01 in scope; the three `check.sh` truths already exercised).

### Lane roster

| Lane | Model / mode | Result | Depth |
|---|---|---|---|
| gsd-plan-checker (internal) | — | completed: 0 blockers, 0 warnings, 10 info | — |
| agy | `gemini-3.8-flash-high`, effort high, `--print-timeout 30m`, cwd in scratch | completed (7m36s) | 28 `file:line` citations |
| codex | `gpt-5.6-terra`, reasoning high, `-s workspace-write -C <scratch>` | completed (5m14s, 174,862 tokens). The first probe hit the **usage limit**; the lane waited for the stated reset and re-probed (`PROBE_OK`) before running | 40 `file:line` citations |
| DeepSeek v4 Pro | `pi --provider deepseek --tools read,grep,find,ls` (read-only tools) | **DROPPED — timeout.** Killed by the 60-minute cap (`exit=124`) with 0 bytes on stdout and stderr. Recorded as a drop, not a pass. Not re-run: two external lanes completed | — |

Every lane was probed before running (`agy --version` → 1.2.1 with `gemini-3.8-flash-high` listed;
`pi` → `PROBE_OK`; `codex` → usage limit, then `PROBE_OK` after the reset). Worktree HEAD, `git status`
and the main checkout's status were snapshotted before the run and compared after every lane:
unchanged each time, and no `scripts/.mutant-guard-test.sh` was left behind. The DeepSeek balance
moved $3.00 → $2.99 over the run; that is not attributable to the dropped lane alone, because the
operator's `hermes gateway` daemon (running) also bills DeepSeek.

### Findings — verification status

Every finding below was checked against source in this repository before being recorded. Findings 1
and 2 were confirmed by execution against synthetic fixtures, each case set including one that must
flip.

| # | Finding | Raised by | Verified | Severity (lane → assessed) |
|---|---|---|---|---|
| 1 | **47-06's `prompt.rs` region gate is blind to pure insertions after lines 55 and 56.** The awk uses the *old* hunk count; a zero count computes the end as start−1. | codex | **Executed** with the gate's exact sed/awk: `@@ -56,0` and `@@ -55,0` → `outside=0`; controls `@@ -57,1`, `@@ -41,0`, `@@ -60` → `outside=1`; in-range `@@ -45,2` → `0`. An edit *to* the pinned text at ≥57 is still caught. | HIGH → LOW |
| 2 | **47-06 Task 2's snapshot gate passes a wrong two-line edit.** It requires both new lines and absence of the old opening line, but never absence of the old closing line. | codex | **Executed** on copies of the real Claude snapshot: intended edit PASS; heading replaced by new line 2 with the old closing line kept → PASS (`old2_still_present=1`); edit missing new line 2 → FAIL (control). | MEDIUM → MEDIUM |
| 3 | **Intermediate red commit.** `feat(47-06)` leaves three snapshot tests failing until Task 2's `test(47-06)`. An interrupted executor, or a push in that window, reads as a broken tree; `git bisect` sees a regression. | agy, codex, checker | Task 1 commits `test(47-06)` then `feat(47-06)` (47-06:213, :235); re-bless is Task 2 (:272-307); the three tests are two core snapshot tests plus `the_gate_rule_holds_in_both_delivered_turns` (`pipeline_launch.rs:4016`); pre-push runs the full check (`scripts/hooks/pre-push:217-220`). | agy MEDIUM, codex HIGH, checker info → MEDIUM |
| 4 | **Gate-detector interaction.** Reasoning that restates a `gate: blocking-human` label can make a *failed* Claude stage read as a confirmed checkpoint and resume up to `MAX_CHECKPOINT_RESUMES` = 3. The plan's decision-record fixture deliberately avoids the label, so its test does not exercise this. | planner (T-47-14), agy, codex, checker | Matcher `agent_result.rs:693-713`; every top-level result event scanned `:1474-1480`; sole caller `pipeline_launch.rs:1594-1603` inside `Action::GateReview`; only `Failed`/`Unknown`/`IdleTimeout` map to GateReview (`outcome_policy.rs:59-71`, re-read here). A Success result never triggers it. Exposure pre-exists, from 47-03's final-message reasoning demand. | codex HIGH, agy LOW → MEDIUM (bounded) |
| 5 | **The "reasoning after the result line" control only holds past the 4000-character tail budget.** A short trailing sentence still parses as success, so the test cannot substantiate "must be last". | codex, checker | `parse_marker_lines` scans bottom-up within the budget (`agent_result.rs:2016-2044`); the fixture record is >5000 chars (47-06:191-193). | MEDIUM → MEDIUM |
| 6 | **The parser-boundary test covers plain text and Claude single-document/stream only**; Codex, OpenCode and Antigravity event shapes are not exercised with long reasoning. | agy, codex | 47-06:200-206. Already listed under "What this plan does not establish" (47-06:391-394). | codex MEDIUM, agy LOW → LOW (acknowledged) |
| 7 | **47-07's checklist count.** The plan changes "7 cases" to "8 cases"; counted as the existing text counts, 7a/7b/7c make 10. | codex | Today's 7 = the six named `check` calls 1–6 (`scripts/test-phase-worktree-guard.sh:60-94`) plus the separately printed 3b (`:78`). The plan keeps those (47-07:180) and adds 7a/7b/7c (:194-202); the summary becomes `passed=13` = 3 mode checks + 10. | HIGH → LOW |
| 8 | **`hostile_commit <dir>` never specifies `git -C "$dir"`.** A literal implementation runs in `$REPO`, whose fixture-local isolation disarms 7a/7b. | codex (suspected) | 47-07:189-191. Fails loudly: 7a/7b would report allow and the `passed=13` gate fails, so not a false green. | MEDIUM → LOW |
| 9 | **47-07's mutant is created in `scripts/`**; a hard kill can leave an untracked executable. | codex (suspected) | Gate at 47-07:214. It must sit beside the guard to resolve it; the trap removes it on normal exit. | LOW → LOW |
| 10 | **Environment-injected git config** (`GIT_CONFIG_PARAMETERS`, `GIT_CONFIG_COUNT`) outranks the fixture's repo-local isolation and is not unset. | agy (suspected) | `scripts/test-phase-worktree-guard.sh:13` unsets `GIT_DIR … GIT_CONFIG GIT_PREFIX` only. agy's `/etc/gitconfig` example is wrong: repo-local config outranks system config. | LOW → LOW |

**Checker advisories not duplicated above:** T-47-14 should record the Success short-circuit and the
3-resume cap; name the numstat gate, not the insta diff read, as what enforces D-15;
`requirements: [WR-01]` names a review finding rather than a REQUIREMENTS.md ID; `47-VALIDATION.md`
maps none of the four new tasks; `47-RESEARCH.md:702` Open Questions lack RESOLVED markers;
REQUIREMENTS.md marks DECN-03 complete while VERIFICATION lists it BLOCKED.

### Convergence

- **Found by more than one lane:** the red intermediate commit (3), the gate-detector interaction (4),
  the tail-budget control scope (5), and the parser coverage limit (6).
- **codex alone:** the region-gate insertion blind spot (1), the snapshot-gate weakness (2), the
  checklist count (7), `hostile_commit` (8) and the mutant location (9). Findings 1 and 2 are the two
  that change what a gate can prove, and both reproduced.
- **agy alone:** environment-injected git config (10).
- **Checked by the lanes, no finding:** the new wording removes the contradiction at every policy
  delivery site, and the four stage prompts with their own "must be exactly" line
  (`prompt.rs:271-279, 350-360, 478-487, 563-573`) ask for no other content (agy, codex, checker);
  47-07's isolation does not bypass the guard under test, with six direct `"$GUARD" --staged` calls
  retained (agy, codex, checker); D-02/D-04 pinned text lies outside every intended edit (agy, codex,
  checker).
- **agy reported executing 47-07's gates outside the repository** (unmodified script → `gate_rc=1`;
  with isolation → `gate_rc=0`, `passed=10`; mutant → `FAIL 7c`). Not reproduced here; recorded as
  the lane's claim.

### Not established

No `cargo` was run against the repository by any lane or by this review. Nothing here shows the
gap plans compile or that the specified Rust tests behave as written; the executor's first run is
the first real evidence. The DeepSeek lane contributed nothing, so this round is two external lanes,
not three.

## Final code review — phase 47 code, pre-UAT

**Run:** 2026-09-12
**Artifact reviewed:** the phase-47 code diff `034f5b6..c4d7231` (23 files, +1250/-77), with emphasis on
the fix round `7a7770a..c4d7231`, which no external lane had seen.
**Outcome:** 4 findings. All 4 were verified here and all 4 are fixed (`ac3ee42`, `c66cba2`, `2790678`,
`49bb324`).

### Lane roster

| Lane | Model | Result | Depth |
|---|---|---|---|
| agy (antigravity) | `gemini-3.8-flash-high`, effort high | **completed**, about 10 min | 14 `file:line` citations; 3 confirmed, 1 suspected |
| codex | `gpt-5.6-terra`, reasoning effort high, sandboxed to its scratch directory | **completed**, about 17 min | 13 `file:line` citations; 1 confirmed |

Neither lane wrote into the worktree. A fingerprint of HEAD, status, diff, untracked content, refs,
stash, worktrees and `.snap.new` files was identical before and after, and no worktree file was
modified during the run. The DeepSeek lane was not run; the operator named codex and agy.

### Findings — verification status

| # | Finding | Lanes | Verified here | Resolution |
|---|---|---|---|---|
| 1 | The worktree-guard harness exits 128 with no output in CI's container. `67cab06` nulled global git config before the harness's one query against the real checkout, which CI accepts only through a global `safe.directory`. | agy | In the pinned CI image, as root on a runner-owned copy: the committed harness exited 128 with no output; the pre-`67cab06` harness passed 13/0; the committed harness with matching ownership passed 13/0. | `c66cba2`. Global config is nulled only after that query, and a failed query is a visible FAIL. Under the same mismatch the fixed harness passes 13/0 while the committed one still exits 128. Both escapes `67cab06` closed stay closed: global excludes hiding `.planning/` gives 13/0 (pre-`67cab06` harness aborts), and an exported `GIT_OBJECT_DIRECTORY` gives 13/0 with 0 stray objects (pre-`67cab06`: 17). Parity guard `worktree_guard_harness_queries_the_checkout_before_nulling_global_config` failed before the fix. |
| 2 | The line-anchored detector missed `- **Gate:**`, `1. **Gate:**` and `> **Gate:**` lines. | agy (codex saw it and judged it acceptable) | Probe: those renderings return false after `bd7bd7c` and true before it; both controls behaved. | `ac3ee42`. Leading list, ordinal and blockquote markup is stripped, never words. `blocking_human_checkpoint_reported_sees_through_list_and_quote_markup` failed before the fix on the `- ` prefix. |
| 3 | A literal `\n` in agent-authored text, such as a code sample, read as a declaration and could spend a capped, audited auto-resume. | agy, codex | Probe: true both before and after `bd7bd7c`, so it predates the line anchoring. codex's proposed fix (stop splitting) failed 4 existing tests in an earlier mutation run. | `49bb324`. A raw-capture reader decodes the string values of JSON lines; the matcher scans physical lines only; the envelope fallback is unchanged. `literal_backslash_n_in_agent_text_is_not_a_line_break` failed before the fix on plain text. Mutation probe: disabling the stream branch fails the same 9 tests as HEAD, disabling the JSON decoding fails 5 including the new test, and restoring the old split fails only the new test. |
| 4 | `check.sh test` skipped the harness whenever `cargo test` failed. | agy (codex observed the behaviour and judged it correct) | A real failed run earlier the same day printed no harness output. | `2790678`. Both exit statuses are kept, the harness always runs, and cargo's status is returned first. `check_script_run_test.rs` failed before the fix on the cargo-failure case and asserts exit 101 and exit 1. |

### Convergence

- **Both lanes:** finding 3. codex rated it medium, tracing it to the resume path at
  `pipeline_launch.rs:1594`; agy rated it low.
- **agy alone:** findings 1, 2 and 4. codex missed 1 because it did not run the CI container (its own
  limits section says so), and recorded the behaviour behind 2 and 4 as acceptable.
- **Both lanes, independently of each other and of this session's probe:** disabling the Claude stream
  branch fails the same 9 stream-scoping tests (codex `190 passed; 9 failed`, with one test of its own
  added; agy `189 passed; 9 failed`).
- **agy's quoted evidence for finding 1 was partly wrong.** It quoted `fatal: detected dubious ownership`,
  which the real run never printed, because the harness discarded git's stderr. The finding held; the
  quoted output did not.

### Not established

- The fixed harness inside a full `scripts/check.sh test` on GitHub's runner. The container runs here
  exercised the harness alone, so the PR's first CI run is the first real observation.
- Whether anything relays checkpoints as list items, which is finding 2's practical reach. GSD's
  executor template does not.
- Live model compliance with prompt wording, which Phase 49 observes.

Raw lane output is in the session scratchpad (`review47/review-agy.md`, `review47/review-codex.md`);
codex's final review reached stdout this time as well as stderr.
