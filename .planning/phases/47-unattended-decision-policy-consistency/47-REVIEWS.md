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
