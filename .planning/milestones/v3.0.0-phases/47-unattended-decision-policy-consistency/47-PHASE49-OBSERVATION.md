# Phase 47 Observation for Phase 49: DECN-03 Behavioural Evidence

## Question

Phase 49's live Claude run must determine how an agent behaves when the first
turn carries `CODE_STAGE_POLICY` and the resumed turn carries
`checkpoint_auto_decide_prompt`. Source inspection establishes that the text is
consistent; it does not establish which instruction the model follows.

## Evidence standard

- **Followed:** `checkpoint_auto_decided` fires, the resumed session capture
  shows the concrete resolution, the final message carries the comparison
  reasoning the policy requires, and the run progresses past that gate.
- **Not followed:** the session actually ran, but the agent reported the gate
  and halted instead of resolving it.
- **Void:** the spawn failed and only the event is present. Neither followed
  nor not followed applies.

The event alone is insufficient: it is emitted before the spawn deliberately,
so a spawn failure still records the decision attempt. Treating that event as
proof of a decision would label a failed spawn as followed.

## Scope limit

DECN-03 closes for claude alone. `pipeline_launch.rs:1569` gates the resume
route on `AgentKind::Claude`; `exec_resume_command` exists only on
`ClaudeDriver`; and auto-mode preflight admits only claude and antigravity.
Three of the four claude-style adapters cannot reach the co-resident prompt,
and two cannot start an unattended run. Model-specificity therefore remains a
limit on this closure, not a test requirement for the other adapters.

## Position effect

The resume delivers `checkpoint_auto_decide_prompt` as the latest turn of a
multi-turn session, while `CODE_STAGE_POLICY` sits in turn 1. The governing
effect is recency, not intra-document primacy. Phase 49 should record which
turn carried which rule alongside the outcome.
