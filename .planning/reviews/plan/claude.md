> claude -p --model opus (worktree CWD, correct SPEC)

## Needs your decision

**F1 — 36-02 makes dead code deterministic and deletes the checks guarding the live path. This is the phase's central claim and it is false.**

`release_finish` has exactly one caller in the entire workspace: its own unit test at `crates/devflow-core/src/git.rs:1370`. Nothing on any pipeline path calls it. The only tag-creating call on a live path is `hooks.rs:337` → `git.tag(&tag)` (VersionBump), and 36-02's own must_have says *"The separate `tag()` … is deliberately left unsigned."*

So AC #3 — *"the release path signs tags with `devflow.releaseSigningKey` deterministically"* — is satisfied by a function no production code path reaches, while the same task deletes the two things that were actually watching the real tag: the `release --check` probe and `scripts/hooks/pre-push:40-70`. Your real releases are hand-cut (protected branches force a PR both hops; `release_finish` does a local `git checkout main; git merge --no-ff`, which cannot execute against your protection rules at all). The plan's key_link asserts *"the deterministic override makes the wrong-key case impossible from the release path"* — it does not, because the operator's hand still creates the tag, and after this plan nothing checks the fingerprint.

The SPEC's own D-02 flags this as **one-way** reversibility. Net effect as planned: enforcement surface removed, determinism added where it cannot fire.

**Decision: either (a) point 999.104 at the path that actually creates release tags, or (b) keep the pre-push fingerprint hook until a live path exists.** I recommend (b) plus re-scoping — the hook is tautological only in the sense that it compares config-to-config; what it *actually* catches is the agent's key vs. the maintainer's key on a tag you push by hand, and that case is real and undetectable any other way (the hook's own comment at `pre-push:41-48` says both keys share `user.email` and render identically everywhere but the fingerprint).

---

## Needs your awareness

**F2 — 36-02 contradicts itself on `tag()`, and one reading breaks every contributor's Ship.**

- must_have truth: *"The separate `tag()` (VersionBump's lightweight auto-bump tag) is deliberately left unsigned"*
- P-01: *"`tag()` is left untouched."*
- P-03: *"`release_finish` **and `tag`** read `devflow.releaseSigningKey`; if unset or the file is unreadable, return an error"*
- Task 1 `<verify>`: *"`release_finish`/`tag` fail with a clear error when `devflow.releaseSigningKey` is unset"*

If P-03/`<verify>` wins, `hooks.rs:337` starts hard-failing VersionBump on any machine without `devflow.releaseSigningKey`. The current hook is explicitly *"Opt-in by design: unset `devflow.releaseSigningKey` means no enforcement, so a contributor who never cuts a release needs no setup"* (`scripts/hooks/pre-push:50-52`). The plan silently converts an opt-in into a mandatory global config, and an executor has no way to resolve which of the four statements is authoritative.

**F3 — "The compiler is the completeness guarantee" is wrong; `cargo build` will stay green over a half-done deletion.**

P-02: *"a dangling reference fails `cargo build`."* True for the direction that doesn't matter. The direction that does: deleting `check_signing_viability` / `check_ssh_signing_viability` / `sign_probe_verdict` / `run_ssh_sign_probe` orphans these **private** items, which produce `dead_code` **warnings**, not errors:

`check_gpg_signing_viability` (`git.rs:1200`), `sign_probe_within` (`:999`), `public_key_fingerprint` (`:818`), `inline_signing_key_blob` (`:846`), `SSH_SIGN_PROBE_TIMEOUT`/`_POLL` (`:880`/`:883`), plus their tests at `:2238-2277`. `check_gpg_signing_viability` is not in P-02's deletion list at all.

Both tasks' `<verify>` blocks run `cargo build`/`cargo test`. The gate that would actually catch this is `scripts/check.sh`'s `cargo clippy --workspace --all-targets -- -D warnings`, which neither task runs. And the `<verify>` grep covers two symbol names — against your own recorded lesson in `CLAUDE.md:57-60`: *"A symbol search does not find tests that reference a deleted item through its strings."*

**F4 — Two files are mechanically guaranteed to break and appear in neither plan.**

- `crates/devflow-cli/tests/pre_push_signing_policy.rs` (139 lines) is an integration test of precisely the hook block Task 1 deletes. Not in `files_modified`, not mentioned anywhere in SPEC/CONTEXT/RESEARCH/plans. It will fail.
- `.planning/DEV-SETUP-CHECKLIST.md:41-43` documents `devflow.releaseSigningKey` *"enforced by the pre-push hook (§3) by comparing…"*. `CLAUDE.md:148-157` requires updating that checklist **in the same commit** when hooks change, and `scripts/hooks/post-commit` warns when it doesn't move. Neither plan mentions it.

**F5 — The Pi credential check measures a proxy, and its "negative control" validates the proxy, not the thing.**

36-01 Task 2 `<behavior>`: *"check a provider credential resolves — `DEVFLOW_PI_PROVIDER` set, or `GEMINI_API_KEY` set, or `pi` auth already stored."*

`DEVFLOW_PI_PROVIDER` is a provider **name**. `DEVFLOW_PI_PROVIDER=openai` with no OpenAI credential anywhere reports headless-capable — a false positive in the exact check whose stated purpose is separating "installed" from "can execute headless". *"or `pi` auth already stored"* names no file, no command, no format; an executor will drop it and ship env-var sniffing.

Meanwhile `pi --help` (verified, 0.84.1, on PATH here) documents a real probe the plan never mentions:

```
pi auth <command>     Print credentials or check provider readiness
```

Because the plan's check is env-var-only, its negative control ("binary present, no credential → NOT capable") proves the env vars are unset. It cannot distinguish that from "pi cannot reach a provider," which is the claim. Also, `preflight`'s signature is `fn preflight(&self, _state: &State)` (`agents/mod.rs:57`) — no env plumbing — so the test must mutate process-global env inside `devflow-core`'s single-process lib test binary. `devflow-core` has three *independent* per-file test mutexes (`config.rs:274`, `gates.rs:381`, `git.rs:2161`); a fourth in `pi.rs` guarding `GEMINI_API_KEY` is sound only by accident, which is verbatim the failure your Phase 19 `ENV_MUTEX` hoist decision exists to prevent (PROJECT.md Key Decisions).

**F6 — Half of must_have #3 is unreachable, and AC #1's `doctor` half is delivered by neither plan.**

`commands.rs:254` runs `ensure_agent_binary(agent_program(agent))` at `devflow start`, *before* any adapter preflight (`preflight.rs:85`), and hard-errors with `agent binary \`pi\` not found — is it installed? (run \`devflow doctor\`)`. The adapter's "binary absent" `Err` arm therefore cannot fire on the start path — it is an unfalsifiable branch that only a direct unit call can exercise.

And AC #1 (36-SPEC.md:70) says *"`devflow doctor` … reports Pi's presence and headless-capability."* `doctor` (`commands.rs:2195`) never calls `adapter_for(...).preflight(...)` — it only calls `.name()` (`:877`, `:2113`). 36-01's `files_modified` excludes `commands.rs`. The SPEC's *"(or the equivalent preflight)"* parenthetical is doing all the work, and no plan states that the doctor half is being dropped.

**F7 — The plan silently deviates from a SPEC marked `status: locked`.**

- SPEC §A: `("pi", vec!["-p", "--model", <model>, "--provider", <provider>, "--approve", <prompt>])` — unconditional.
- 36-01 must_have: `--model`/`--provider` *"only when set"*, plus `--no-session`, which appears in neither the SPEC nor RESEARCH's verified findings.

Not necessarily the wrong call — but it is an undeclared amendment to a locked document. Related, not currently triggered *(speculative on future prompts)*: the planned argv has no `--` separator, and `pi` parses `[options] [@files...] [messages...]` with extension-registered flags. I checked `prompt.rs` — today's stage prompts contain no `@` tokens and don't start with `-`, so this doesn't bite now. One token prevents it later.

**Also, cheap to fix:** `36-02` is `wave: 2` with `depends_on: []` and an objective stating *"neither depends on the Pi adapter (plan 01)"* — that serializes two independent plans for nothing, against your own wave==DAG-level convention. And the read_first line numbers have drifted: SPEC/CONTEXT cite `check_signing_viability` at `git.rs:1099` (actual: `1234`), CONTEXT D-02 cites `git.rs:1163` for "can this key sign" (that's `sign_probe_verdict`, not the probe), RESEARCH says `check_ssh_signing_viability (git.rs:1112-1150)` (actual: `1117`). An executor told to "delete lines ~1112-1234" from a stale map is how the wrong region goes.
