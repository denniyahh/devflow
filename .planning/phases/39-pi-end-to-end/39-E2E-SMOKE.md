# Phase 39 — Stage 2 end-to-end smoke (recorded evidence)

**Date:** 2026-08-17
**Profile:** throwaway `PI_CODING_AGENT_DIR=/tmp/p39-e2e-profile` (live profile untouched)
**Setup:** `models.json` copied from the live profile (provider `litellm`); `pi install npm:@bacnh85/pi-subagent@0.15.1` into the throwaway profile.

## Result: proxy-only evidence; provider claim corrected post-review

**Provider correction (phase-39 code review, finding 4):** the throwaway profile's `settings.json`
carried no `defaultProvider`, so Pi did **not** run on `litellm` — the recorded session `provider`
was `deepseek`/`openrouter` (env-var providers). The `litellm` path this phase's provider fix
targets was therefore **not exercised** by this smoke.

Run 1 (completion): `PI_CODING_AGENT_DIR=/tmp/p39-e2e-profile pi -p --no-approve "<subagent prompt>"`
→ stdout `DEVFLOW_RESULT: {"status":"success"}`, exit 0.

Run 2 (dispatch): same, with the subagent task `echo subagent-ran > /tmp/p39-subagent-proof.txt` →
stdout `DEVFLOW_RESULT: {"status":"success"}`, exit 0, and `/tmp/p39-subagent-proof.txt` contains
`subagent-ran`.

## What this establishes — and what it does not

Established:

1. `@bacnh85/pi-subagent` loads at user scope under `pi -p --no-approve`. (stdout + exit 0)
2. The parent emits `DEVFLOW_RESULT` and exits 0.

**Not established by the in-repo evidence:** that the *subagent tool* (rather than Pi's own `bash`
tool) wrote `/tmp/p39-subagent-proof.txt`. A bash side-effect file is a proxy — the parent's own
`bash` tool could produce the identical file without ever invoking `subagent`. The discriminating
evidence (the session transcript showing exactly one `toolCall` named `subagent`, with the bash
calls nested inside the subagent's result) was observed at review time (2026-08-17) in
`/tmp/p39-e2e-profile/sessions/`, but it was **not captured into the repo** and the throwaway
profile has since been removed.

**Verdict:** the acceptance criterion in `39-PLAN.md` ("captured transcript shows `toolCall:
subagent` nesting") is **not met** — a re-run that captures the transcript is required.

## Capability-detection sanity check

`PI_CODING_AGENT_DIR=/tmp/p39-e2e-profile pi list --no-approve` → `npm:@bacnh85/pi-subagent@0.15.1`
→ the vetted `@bacnh85/pi-subagent` name-match predicate returns `true` (matches the unit test's
stubbed shape).
