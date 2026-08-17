# Phase 39 — Stage 2 end-to-end smoke (recorded evidence)

**Date:** 2026-08-17
**Profile:** throwaway `PI_CODING_AGENT_DIR=/tmp/p39-e2e-profile` (live profile untouched)
**Setup:** `models.json` copied from the live profile (provider `litellm`); `pi install npm:@bacnh85/pi-subagent@0.15.1` into the throwaway profile.

## Result: PASS

Run 1 (completion): `PI_CODING_AGENT_DIR=/tmp/p39-e2e-profile pi -p --no-approve "<subagent prompt>"` →
stdout `DEVFLOW_RESULT: {"status":"success"}`, exit 0.

Run 2 (dispatch proof): same, with the subagent task `echo subagent-ran > /tmp/p39-subagent-proof.txt`
→ stdout `DEVFLOW_RESULT: {"status":"success"}`, exit 0, and `/tmp/p39-subagent-proof.txt` contains
`subagent-ran`.

## What this proves

1. `@bacnh85/pi-subagent` loads at user scope under `pi -p --no-approve`.
2. The model invokes the `subagent` tool; the subagent executes (bash wrote a file).
3. The parent emits `DEVFLOW_RESULT` *after* the subagent finishes, and exits 0.
4. `MonitorLaunch::Legacy` process-exit + the generic `DEVFLOW_RESULT` marker path observes
   completion — **no drain gate, no `PipeOwning`, no DevFlow source change**.

## Capability-detection sanity check

`PI_CODING_AGENT_DIR=/tmp/p39-e2e-profile pi list --no-approve` → `npm:@bacnh85/pi-subagent@0.15.1`
→ the `subagent` name-match predicate returns `true` (matches the unit test's stubbed shape).
