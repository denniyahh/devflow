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
