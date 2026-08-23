---
status: backlog
source: Phase 19 plan 19-05 dogfood checkpoint (2026-07-22), retained as a non-blocking finding in 19-VERIFICATION.md
---

# Backlog: AI Change Acceptance Contract — Review Wiring

## Goal

Make the `.claude/skills/ai-change-acceptance/` contract actually govern AI
change review, rather than merely existing in the repository.

Phase 19's 19g landed the contract (skill + two rule files + `CONTRIBUTING.md`
section) and plan 19-05 dogfooded it. The dogfood found a real gap:

- **The contract's *wording* works.** Every non-compliant scratch diff was
  flagged and every compliant control was left alone, when the reviewer was
  explicitly pointed at the rule files.
- **The contract's *wiring* does not.** A freshly spawned, context-isolated
  reviewer with routine dispatch wording independently reached the same
  verdicts on both anti-pattern shapes (`assert_eq!(x, x)`-style constant
  assertions; a test reproducing the production algorithm inline) — but it
  never cited the project contract as its authority, and it classified those
  findings as `warning`/`info` rather than at the contract's
  acceptance-blocking severity.

So today the contract only binds when whoever dispatches the review already
knows to load it. A routine review does not escalate via the contract.

## Why this matters

19g exists because a green test suite is not evidence that behavior is
protected. If the contract only applies when someone remembers to invoke it,
it does not close that hole for the case it was written for — an unattended or
routine AI-authored change.

## Possible shapes (not yet decided)

- Have the review dispatch explicitly load `.claude/skills/ai-change-acceptance`
  so the reviewer inherits the contract's severity model.
- Strengthen the skill's own trigger/description wording so it surfaces on
  relevance rather than only on explicit dispatch.
- Encode the four rejection patterns as a mechanical lint over changed test
  files, so detection does not depend on reviewer judgment at all.

## Notes

Part of the wiring surface lives in the GSD code-review workflow **outside this
repository**, so an in-repo fix alone may not fully close this. Scope that
boundary before committing to an approach.

Do not "fix" this by simply asserting in `CONTRIBUTING.md` that reviewers must
read the contract — that is the same unenforced-by-construction shape the
contract itself rejects.

Promote with `/gsd-review-backlog` when ready.
