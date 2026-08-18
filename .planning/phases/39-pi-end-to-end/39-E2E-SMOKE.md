# Phase 39 — Stage 2 end-to-end smoke (recorded evidence)

**Date:** 2026-08-18 (re-run after the 2026-08-17 smoke was found to be proxy-only)
**Profile:** throwaway `PI_CODING_AGENT_DIR=/tmp/p39-e2e-v2` (live profile untouched)
**Setup:** `models.json` **and** `settings.json` copied from the live profile (so `defaultProvider: litellm` is set — the first run copied only `models.json`, which is why it fell through to env-var providers); `pi install npm:@bacnh85/pi-subagent` into the throwaway profile.

## Result: PASS — discriminating evidence captured

Run: `PI_CODING_AGENT_DIR=/tmp/p39-e2e-v2 pi -p --no-approve "<prompt>"` (cwd `/tmp`) →
stdout `DEVFLOW_RESULT: {"status":"success"}`, exit 0.

The full session transcript is captured in-repo at
[`39-E2E-SESSION.jsonl`](./39-E2E-SESSION.jsonl). The discriminating chain, read
directly from that transcript:

| Event | Detail |
|---|---|
| `session` / `model_change` | parent provider **`litellm`**, model `deepseek-v4-pro` |
| parent `assistant` turn | exactly one `toolCall` with `name: "subagent"`, `arguments.agent: "worker"` |
| `toolResult` (subagent) | nested subagent session (`provider: openrouter`, `nvidia/nemotron-3-super-120b-a12b:free`) whose own `toolCall` `name: "bash"` ran `echo subagent-ran > /tmp/p39-v2-proof.txt && cat …`; bash result `subagent-ran` |
| parent final `assistant` | `DEVFLOW_RESULT: {"status":"success"}` — emitted only **after** the tool result returned |

`/tmp/p39-v2-proof.txt` contains `subagent-ran` (the bash side-effect), but it is
now corroborated by the transcript's `toolCall` nesting rather than standing
alone as proxy evidence.

## What this proves

1. `@bacnh85/pi-subagent` loads at user scope under `pi -p --no-approve`.
2. The **parent** runs on the configured `defaultProvider` (`litellm`) — the
   path `PiDriver::health` probes.
3. The parent delegates to the subagent (`toolCall: subagent`); the subagent's
   `bash` executes nested inside the subagent result, not as a parent tool call.
4. The parent emits `DEVFLOW_RESULT` *after* the subagent finishes, and exits 0
   — so `MonitorLaunch::Legacy` process-exit + the generic marker path observe
   completion, with **no drain gate, no `PipeOwning`, no DevFlow source change**.

## Honest limits

- The subagent resolved to `openrouter` (the extension's own role-model chain),
  not `litellm` — expected: the subagent's model is the extension's concern; the
  *parent's* provider is the `litellm` fix's target and is what the transcript
  shows on `litellm`.
- This is a recorded live run, not a re-runnable automated test — see the
  summary's coverage D4 (`human_judgment: true`).
