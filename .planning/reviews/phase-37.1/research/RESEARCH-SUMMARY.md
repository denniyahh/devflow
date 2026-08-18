# Phase 37.1 — Research summary (actual investigation, not doc review)

**Reviewers:** codex (gpt-5.6-terra, high) · antigravity (gemini-3.7-flash-high, --print-timeout 15m)
**Date:** 2026-08-17
**Root:** `.worktrees/phase-37.1` (feature/phase-37.1 @ cef4059). Both unpacked 20+ npm tarballs and read the installed Pi 0.84.1 binary/package source; both ran `pi`/`npm` commands (full logs in `codex.md` / `antigravity.md`).

## Bottom lines (they disagree, and it is a real disagreement worth reading, not a tie to average)

- **codex — NO**: "every discovered background dispatcher can outlive the Legacy parent while the only verified foreground candidates require trust-boundary changes before use."
- **antigravity — YES**: "`@bacnh85/pi-subagent` and `@dreki-gg/pi-subagent` are fully functional, maintained extensions that execute subagents synchronously via `await` … without any core Rust driver changes."

## What BOTH agree on (convergent, evidence-backed — this is the reliable core)

1. **Pi core has no native subagent tool** — an extension is strictly required. (Both verified against the binary.)
2. **Synchronous foreground dispatch works under the EXISTING launch shape.** An extension whose `execute()` *awaits* its children before returning keeps the parent `pi -p --no-approve` process alive until subagents finish, which then emits `DEVFLOW_RESULT` and exits — `MonitorLaunch::Legacy` process-exit supervision is sufficient. **No new drain gate, no `PipeOwning`, no monitor change, no Rust change needed.**
3. **Background/detached dispatch is incompatible** — the parent finishes before children; DevFlow has no Pi drain predicate, and `PipeOwning` cannot be reused (Pi consumes stdin until EOF; `PipeOwning` holds it open).
4. **The original `37.1-RESEARCH.md` enumeration was incomplete.** It catalogued only 6 npm packages and dismissed all as SUS on metadata heuristics. The real registry contains **20+ installable extensions**, several actively maintained (`pi-subagents` has 50+ releases; `@dreki-gg`, `@bacnh85`, `@parke.dev` are versioned and maintained), plus the bundled reference example. The "comparison doc" was never actually done.

## Where they diverge (the one thing that needs a final code check)

The disagreement is **only about the trust boundary**, not the mechanism:

- **antigravity** reads the top candidates as already fail-closed: `@bacnh85` rejects project agents headlessly (`extensions/index.ts:614` returns "Project agents require explicit user approval…" when `!ctx.hasUI`); `@dreki-gg` is "prompt-native" (no repo `.md` loading); bundled example defaults `scope: "user"`.
- **codex** reads the same surface as still open: `@mystilleef` explicitly adds `--approve` to its child (`src/child/process.ts:401`) and defaults `agentScope: "both"` with headless confirmation bypassed; the bundled example skips project-agent consent because the confirmation is gated on `ctx.hasUI`; and no candidate is installed in the real profile (so none is "verified secure").

They directly conflict on `@mystilleef` (antigravity: works out-of-the-box; codex: child forces `--approve`, scope "both"). That is a factual dispute about specific source lines, resolvable only by reading the 2–3 finalists' child-argv + `agentScope`/headless-confirmation code once more.

## Synthesized answer to "is there ANY that works, yes or no?"

**YES — with changes, and the changes are small and concrete.** The strong-form "NOT VIABLE" verdict from the spike documents is **refuted**: viable synchronous foreground extensions exist and the dispatch mechanism works under DevFlow's current `Legacy`/`-p`/`--no-approve` launch shape with no monitor work. The remaining requirement is a **trust-boundary patch** — every subagent child must be launched `--no-approve` and must refuse project-scoped agents in headless mode — plus installing/vendoring the extension at user scope. The only open question is whether specific finalist packages *already* satisfy that patch (antigravity says yes, codex says no); that needs one final primary-source read of the 2–3 finalists' child-argv and `agentScope` handling, which neither reviewer could settle because no package is installed in the real profile and an authenticated child run was unavailable.

Net: the original verdict was wrong in its *strong* form (overgeneralized from "can't reuse Claude's `PipeOwning`" to "not viable at all", and it missed the maintained extension population). The correct, defensible verdict is **"viable with a synchronous-foreground + trust-boundary-patch constraint"** — not a closed-and-verified YES, but also not NOT VIABLE.

---

## Contention-resolved (primary-source read, 2026-08-17)

Method: unpacked the published tarballs from `npm` (already on disk at `/tmp/pi-subagent-registry/<name>-<version>/package/` from the reviewer runs) and read the actual source. Every claim below is backed by file:line in that source.

### Per-finalist findings (file:line)

**Bundled example — `examples/extensions/subagent/index.ts` (Pi 0.84.1)**
- Child argv (`:294`): `["--mode","json","-p","--no-session"]` — **neither `--no-approve` nor `--approve`** → child runs with Pi's default approval (trusts project-local `.pi/extensions`/settings in its cwd).
- `agentScope` (`:473`): `params.agentScope ?? "user"` — default `user`, but **model-supplied** (the model can pass `"project"`/`"both"`).
- Headless confirmation (`:505`): `if ((agentScope === "project" || agentScope === "both") && confirmProjectAgents && ctx.hasUI)` — the `ctx.hasUI` gate means the confirmation is **skipped entirely in headless mode**, so project agents load without consent. **codex was correct.**
- Synchronous: yes (`:235` `await Promise.all(workers)`; single path `await new Promise` on child close).

**`@bacnh85/pi-subagent@0.15.1`**
- Architecture: **in-process SDK sessions** (`createAgentSession`), not child processes — `extensions/index.ts:11` "Compared to process-spawning, this saves ~4-11K tokens". No child `pi` argv exists, so the `--approve`/`--no-approve` child question is moot (children inherit the parent's `--no-approve`).
- `agentScope` (`extensions/index.ts:480`): `params.agentScope ?? "user"` — default `user`; description `:466-467` "Default agent scope is user".
- Headless confirmation (`extensions/index.ts:586-615`): `confirmProjectAgents = !trusted.allowUnconfirmedProjectAgents` (from trusted config, **not** tool params); when project agents are requested and there is no prior session trust and no UI, the `else` branch is **"Fail closed in headless sessions"** → returns `"Project agents require explicit user approval."`. **antigravity was correct — this one genuinely fails closed headlessly.** (Also `security.ts` keeps a `DENIED_CHILD_TOOLS` set incl. `subagent`, a recursion guard.)
- **Caveat:** in-process = shared parent context, so it is *not* isolated-context subagent dispatch like Claude's Task.

**`@dreki-gg/pi-subagent@0.19.0`**
- Child argv (`extensions/subagent/spawn-utils.ts:233-245`, `buildPiAgentArgs`): `["--mode","json","-p","--no-session","--no-prompt-templates"]` (+ `--model`/`--thinking`/`--tools`/`--exclude-tools`) — **no `--no-approve`**. Child re-trusts project-local extensions/settings in its cwd.
- `agentScope`/`.md` agents: **none** — prompt-native (the caller's prompt is streamed over stdin, `sendPromptToStdin`; no agent definition files). antigravity was right on "prompt-native", **wrong** that it is fully `--no-approve`-safe — the child is not launched headless-safe.
- Synchronous: yes (`spawnPiAgent` → `runLineDelimitedProcess` resolves on child close).

**`@mystilleef/pi-subagent@0.12.2`**
- Child argv (`src/child/process.ts:395-403`): `["--mode","json","-p","--no-session","--approve","--no-themes","--no-prompt-templates"]` — **explicitly adds `--approve`**.
- `agentScope` (`src/orchestration/subagent-orchestrator.ts:58-63`): `default: "both"` — "user + project-local agents".
- Headless confirmation (`src/orchestration/subagent-orchestrator.ts:371-373`): `needsProjectAgentConfirmation(ctx, agent) { return ctx.hasUI && agent.source === "project"; }` — **no confirmation in headless mode**. **codex was correct on all three; antigravity's "works out of the box" is wrong for this package.**

**`@smoose/pi-subagent@0.1.0`** (adjacent; codex's "strongest foreground candidate")
- Child argv (`src/runner.ts:135-136`, `buildPiArgs`): `["--mode","json","-p","--no-session","--no-extensions"]` — `--no-extensions` but **no `--no-approve`**. No `.md` agent loading (prompt-native). → needs the same `--no-approve` patch.

### Definitive verdict table

| Finalist | child `--no-approve`? | refuses project agents headless? | works under Legacy/`-p`? | Verdict |
|---|---|---|---|---|
| Bundled example | ✗ (neither) | ✗ (`ctx.hasUI` gate skips it) | synchronous ✓ | **works with patch** (add `--no-approve`; reject `agentScope !== "user"` headless) |
| `@bacnh85/pi-subagent` | n/a (in-process) | ✓ fails closed | synchronous ✓ | **works as-is** (in-process; shared context caveat) |
| `@dreki-gg/pi-subagent` | ✗ (neither) | n/a (prompt-native, no `.md`) | synchronous ✓ | **works with patch** (add `--no-approve` to `buildPiAgentArgs`) |
| `@mystilleef/pi-subagent` | ✗ (**`--approve`**) | ✗ (headless bypass) | synchronous ✓ | **does NOT work as-is** (actively unsafe) |
| `@smoose/pi-subagent` | ✗ (neither) | n/a (prompt-native) | synchronous ✓ | **works with patch** (add `--no-approve`) |

### Who was right

- **codex** was right on: bundled example (no `--no-approve`, `ctx.hasUI` skip), `@mystilleef` (`--approve` + `"both"` + bypass), `@smoose` (missing `--no-approve`), and the general "foreground candidates need a trust-boundary patch".
- **antigravity** was right on: `@bacnh85` (fails closed headless — codex over-grouped it into "no verified secure integration"), and the "prompt-native" characterization of `@dreki-gg`.
- **antigravity was wrong** on `@mystilleef` ("works out of the box" — it actively passes `--approve` and defaults `"both"`) and overstated `@dreki-gg` as fully `--no-approve`-safe.

### Settled bottom line

There **is** at least one package that works as-is under DevFlow's launch shape — `@bacnh85/pi-subagent` — with the caveat that it is in-process (shared context), not isolated-context process dispatch. For process-spawning (isolated-context) subagents, the answer is "works with a one-line `--no-approve` child-argv patch" for the bundled example / `@dreki-gg` / `@smoose`; `@mystilleef` is the one that must NOT be used as shipped. The strong-form `NOT VIABLE` verdict remains refuted.
