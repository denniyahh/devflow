> invocation: claude -p --model opus --settings {"effortLevel":"high"} --output-format json --dangerously-skip-permissions "$(cat /tmp/p39-review-prompt.txt)" (cd .worktrees/phase-39)
> review root: /var/home/denniyahh/Github/devflow/.worktrees/phase-39 (feature/phase-39 @ e4f0bb6)
> model resolved: claude-opus-5; verified cargo test agents::pi (12 passed) + clippy -D warnings clean

Findings below. Every claim I make was checked against the live `pi` 0.84.1 install and the phase's own artifacts in this turn; where I inferred rather than observed, I say so.

---

## BLOCKER

**1. `PiDriver::health` reads the wrong file — `models.json` is not Pi's provider configuration**

`crates/devflow-core/src/agents/pi.rs:127-148`
```rust
fn configured_pi_providers() -> Vec<String> { ... json.get("providers") ... }
```

Pi's actually-configured provider is `settings.json` → `defaultProvider` (`dist/core/settings-manager.js:373 getDefaultProvider()`; your live `~/.pi/agent/settings.json` has `"defaultProvider": "litellm"`). `models.json` is a **custom-model catalog** — built-in providers (deepseek, openrouter, anthropic, google…) are credentialled from env vars and **never appear there**. This breaks in both directions:

**(a) Fail-closed false refusal — a regression.** `pi.rs:74-79` hard-refuses before ever running `pi`. Verified on this machine:
```
$ pi auth check --json --provider deepseek --no-refresh
{"status":"ready","provider":"deepseek","authType":"api_key"}   # exit 0
```
`deepseek` is ready from `DEEPSEEK_API_KEY` alone and appears in no `models.json`. On such a profile `configured_pi_providers()` returns `[]` → `preflight.rs:1279` refuses `devflow start --agent pi` outright. The old hardcoded `--provider google` would have passed the equivalent google-env-key install. The phase's own test `health_refuses_when_no_provider_configured` (`pi.rs:384`) encodes the regression as intended behaviour.

**What breaks:** any Pi install without a custom `models.json` — i.e. the default install — cannot start under DevFlow, with the misleading message `"no provider configured in Pi's models.json"`.

**(b) Fail-open — health can green-light a provider the run never uses.** `build_command` (`pi.rs:52-62`) passes no `--provider`, so the run uses `defaultProvider`; `health` (`pi.rs:81-100`) returns `Ok` if **any** catalog provider is ready. The phase's own smoke profile proves the mismatch:

| | value |
|---|---|
| `/tmp/p39-e2e-profile/models.json` providers | `["litellm"]` ← what health would probe |
| `/tmp/p39-e2e-profile/settings.json` | no `defaultProvider` |
| recorded session `"provider"` | `deepseek`, `openrouter` ← what actually ran |

**What breaks:** preflight passes, the launch then fails on a credentialless default provider — the exact class `health` exists to prevent.

---

## HIGH

**2. `pi_resolves_to_legacy_launch` is a tautology w.r.t. the property it names**

`crates/devflow-cli/src/pipeline_launch.rs:3269-3293`
```rust
/// Phase 39 Stage 1 regression: Pi always resolves to `MonitorLaunch::Legacy`.
/// Pi is never Claude, so `stream_launch` is false ...
let (program, args, launch) = resolve_launch_shape(..., false);
```
`stream_launch` is the *sole* gate for `PipeOwning` (`:200`), and the test hardcodes it to `false`. It asserts nothing about `claude_stream_launch_enabled` (`:707-714`), the function that would have to change for Pi to land in `PipeOwning`. Change that predicate to return `true` for Pi and this test still passes — while the deadlock its own comment warns about ships.

The repo already has the right idiom 30 lines up, at `:3235-3241`: *"Precondition: … so the assertion below is a real discrimination and not a stage that was going to be legacy anyway."* One added `assert!(!claude_stream_launch_enabled(AgentKind::Pi, Stage::Code, false))` fixes it.

**What breaks:** the named regression is unguarded; the test measures the literal `false` it passed in.

---

**3. Name-match reports explicitly-excluded packages as "available"**

`crates/devflow-core/src/agents/pi.rs:160-171`
```rust
output.status.success() && String::from_utf8_lossy(&output.stdout).to_lowercase().contains("subagent")
```
`39-CONTEXT.md` excludes `@mystilleef` outright (*"passes `--approve`, default scope `both`"*) and defers `@dreki-gg`/`@smoose` pending an unshipped `--no-approve` child-argv patch. Every one of those package names contains `subagent` → detection returns `true` → `devflow doctor` prints `pi subagent dispatch  available ✓`. The predicate cannot distinguish the one vetted package from the ones the phase ruled unsafe.

Secondary: the match runs over **all** of stdout — `pi list` also prints the install path under each package (`dist/core/package-commands.js:215-218`) and a `User packages:` header — so it is broader than the "package name" both doc comments claim.

**What breaks:** doctor tells the operator the vetted dispatch capability is present when the installed extension is one the phase deliberately rejected.

*Verified correct, for contrast:* `--no-approve` genuinely is the right scope. Negative control run inside a **trusted** path (`trust.json` covers `/var/home/denniyahh/Github`, which is why my first probe in `/tmp` was a false clean): bare `pi list` → `Project packages: npm:@evil/fake-subagent@1.0.0`; `pi list --no-approve` → `No packages installed.`; `pi list --approve` → project package back. Fixtures removed.

---

## MEDIUM

**4. Nothing branches on `subagent_dispatch`; ARCHITECTURE.md describes routing that does not exist**

The only non-test consumer is `crates/devflow-cli/src/commands.rs:2330`. `ARCHITECTURE.md:104-106` reads as a live decision — *"Present → the dispatch arm …; absent/undetectable → the baseline single-agent arm (fail-closed, never refused)"* — as does `docs/guides/pi-subagent-dispatch.md:27`. `docs/guides/pi-subagent-dispatch.md:38-39` then concedes *"The launch argv is identical in both arms"*. There are no arms: it is a doctor line. **What breaks:** a reader (or the next phase) plans against routing that is not implemented; the honest statement is "reported only, no consumer yet."

**5. The multi-provider semantic change is entirely untested**

`pi.rs:80-101` introduces iterate-all + accept-any + keep-last-error. No test uses more than one provider, and the stub's `printf ... > args.txt` (`pi.rs:243`) overwrites, so only the final invocation is observable anyway. Order, any-ready-wins, and `last_err` are all unverified. **What breaks:** the fail-open behaviour in finding 1(b) has no test that would catch a change to it.

**6. `39-E2E-SMOKE.md` cites evidence that cannot carry its claim 2**

`.planning/phases/39-pi-end-to-end/39-E2E-SMOKE.md:14-16, 21` rests dispatch proof on `/tmp/p39-subagent-proof.txt`. The parent has its own `bash` tool and the path is fixed — the file is identical whether the model called `subagent` or ran bash itself. The proof file does not discriminate.

The discriminating evidence *does* exist, and I checked it: `/tmp/p39-e2e-profile/sessions/…/2026-08-17T19-10-31…jsonl` shows the parent making exactly one `toolCall` (`name: "subagent"`), with both `bash` calls nested inside `toolResult.details.results[].messages`, and `DEVFLOW_RESULT` emitted only after the tool result returned. The conclusion holds — but that transcript lives in `/tmp` and is not captured in the repo, so the recorded evidence is the weak one. Also: the doc's stated setup (*"models.json copied … (provider litellm)"*) does not match what ran (`deepseek`/`openrouter`), so the smoke never exercised the litellm path it describes.

---

## LOW

**7.** `pi.rs:128-133` — `PI_CODING_AGENT_DIR` is taken literally; Pi expands `~` and `~/…` (`dist/config.js` `getAgentDir`). A tilde value yields an unreadable path → `[]` → the finding-1(a) refusal.

**8.** `commands.rs:2326-2350` — the probe spawns `pi list` on every `devflow doctor`, measured at **1.28s**, regardless of the project's agent. When `pi` is absent entirely it prints `pi subagent dispatch  not installed ⚠ — optional — pi install npm:@bacnh85/pi-subagent` directly beneath the separate `pi ✗ missing` line. No test covers the new doctor check.

**9.** `39-PLAN.md:57` (`devflow start --agent pi --dry-run` shows the full pipeline) has no recorded output anywhere in the phase artifacts. `39-PLAN.md:58-59` — *"one live run proves … **or** is recorded as blocked on credentials"* — passes either way; it is a non-falsifiable acceptance as written.

---

Build state I verified this turn: `cargo test -p devflow-core --lib agents::pi` → **12 passed, 0 failed, 626 filtered out**; `cargo clippy --workspace --all-targets -- -D warnings` → clean. That establishes the code compiles and the *stubbed* paths behave as written — it says nothing about findings 1, 2 and 3, all of which are green under exactly these gates.

**VERDICT: FIX-FIRST** — `health` probes `models.json` instead of `settings.json.defaultProvider`, which both hard-refuses a default Pi install at preflight and can pass on a provider the run will never use; the Legacy regression test hardcodes the input that decides its own assertion.
