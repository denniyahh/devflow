# Adversarial Review — Phase 36 PLAN (rerun, clean CWD)

**Targets:** `36-RESEARCH.md`, `36-01-PLAN.md`, `36-02-PLAN.md`
**Reviewers:** claude (opus, high) · codex (gpt-5.6-sol, high) · antigravity (Gemini 3.1 Pro)
**Date:** 2026-08-15 — rerun from the worktree CWD (correct SPEC/CONTEXT).

**Execution note:** claude and antigravity read the correct worktree SPEC/CONTEXT. codex's finding #1
still quotes the stale main-checkout SPEC ("terminal completion", "999.67") — codex anchors its
project to the main checkout via `~/.codex/config.toml`, not CWD. codex's findings #2-5 are code/plan
based and valid. The core findings below are confirmed by 2-3 independent reviewers.

## Consensus findings

### 3-of-3 consensus
1. **`tag()` contradiction** — `must_haves` + P-01 say `tag()` is "left untouched/unsigned"; P-03
   and Task 1 `<verify>` say `tag()` reads `devflow.releaseSigningKey` and errors when unset. No
   implementation can satisfy both; if P-03 wins, VersionBump (`hooks.rs:337`) hard-fails on every
   machine without the key (converting the opt-in hook into a mandatory global config).
2. **Credential predicate is a false-green** — `DEVFLOW_PI_PROVIDER` is a provider *name*, not a
   credential. Codex verified: `DEVFLOW_PI_PROVIDER=google` with no key still returns
   `credentials_not_configured` from `pi auth check`. The health check must shell out to `pi auth`
   (Pi's authoritative verb), not sniff env vars.

### 2-of-3 consensus
3. **999.104 targets dead code and deletes the live backstop** — `release_finish` has one caller
   (its own test); the real tag is hand-cut (`scripts/cut-release.sh:114` already applies the
   deterministic `-c user.signingkey=` override; `main.rs:291` says the release executor is
   "deferred, not-yet-built"). The plan hardens a function no production path reaches while deleting
   the probe + pre-push hook — the only things watching the real hand-cut tag. The "deterministic
   override makes wrong-key impossible" key_link is false.
4. **`doctor` is unowned** — `ensure_agent_binary` runs *before* `preflight`
   (`commands.rs:254`/`pipeline_launch.rs:1088`), so the "binary absent" arm is unreachable on the
   start path; and `doctor` (`commands.rs:2195`) only calls `.name()`, never `.preflight()`. SPEC
   AC #1 ("doctor reports Pi") is satisfied by neither plan because 36-01 forbids `commands.rs`.
5. **`--approve` is a security risk** — it trusts project-local `.pi/extensions`/`.agents/skills`
   that execute unsandboxed (Pi has no sandbox). Unconditional approval on a fresh worktree = repo
   contents become unsandboxed code execution.

### Single-reviewer (valid)
- **Missing `--` delimiter** (antigravity): the positional `<prompt>` can start with `-`/`- [ ]`,
  which pi's arg parser will read as flags. Needs a bare `--` before the prompt.
- **SSH signing broken** (antigravity): `git tag -s` defaults to GPG; signing an SSH key needs
  `-c gpg.format=ssh`. Deleting the SSH probe while hardcoding `git tag -s` breaks SSH signing.
- **Compiler-completeness is wrong** (claude): deleting the probe orphans private items → `dead_code`
  *warnings*, not errors; the gate is `check.sh`'s `clippy -D warnings`, which the plan doesn't run.
- **Hook deletion breaks files outside scope** (claude): `pre_push_signing_policy.rs` (139 lines)
  and `DEV-SETUP-CHECKLIST.md` (CLAUDE.md requires same-commit update).
- **env-var model/provider is volatile** (claude): the `legacy_claude_launch` precedent documents why
  env-scoped values vanish across detached stage launches.
- **`wave: 2` + `depends_on: []`** serializes two independent plans (claude).
- **stale line numbers** in read_first (claude).

## Invalidated
- codex finding #1 ("SPEC requires terminal completion / 999.67 present") — stale SPEC, CWD artifact.

## What the plan must change

1. **999.104:** point it at the real path — the hand-cut `cut-release.sh` — and **keep** the pre-push
   fingerprint hook (it is the only check that distinguishes the agent key from the maintainer key on
   a hand-pushed tag; it is not tautological in the way the plan claims). Rescope to: unset-key
   guard at `cut-release.sh` + delete the capability-only probe. Do not harden `release_finish`.
2. **Pi health check:** shell out to `pi auth check` (not env-guessing); own `commands.rs` for doctor.
3. **`--approve`:** reconsider — default `--no-approve`, or gate it explicitly.
4. **Fix** the `tag()` contradiction, add `--`, handle `gpg.format=ssh`, add `pre_push_signing_policy.rs`
   + `DEV-SETUP-CHECKLIST.md` to scope, run `clippy -D warnings`, fix wave/depends_on.
