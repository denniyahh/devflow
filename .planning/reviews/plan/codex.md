> codex exec -m gpt-5.6-sol (NOTE: finding #1 quotes STALE SPEC — codex anchors to main checkout via ~/.codex/config.toml, not CWD; findings #2-5 valid)

## Needs my awareness

1. **Critical — Phase 36 cannot meet its own acceptance criteria.**

   `36-01-PLAN.md:79-82`:

   > “It does not make `devflow start --agent pi` reach terminal completion … It does not parse Pi's JSON-mode output.”

   But `36-SPEC.md:71-72` requires:

   > “A `devflow` run driven by Pi reaches a terminal completion, with its completion parsed and its result classified.”

   The spec’s 999.67 `decided_by_layer` fix (`36-SPEC.md:42-45,73-74`) is also absent from both plans. Executing every planned task still fails the phase.

2. **High — the Pi credential predicate is observably wrong.**

   `36-01-PLAN.md:149-152` treats this as credential readiness:

   > “`DEVFLOW_PI_PROVIDER` set, or `GEMINI_API_KEY` set, or `pi` auth already stored.”

   `DEVFLOW_PI_PROVIDER` only selects a provider. Negative control with `DEVFLOW_PI_PROVIDER=google`, no Gemini key, and an empty Pi config returned:

   ```json
   {"status":"not_ready","provider":"google","reason":"credentials_not_configured"}
   ```

   Exit code: `1`. The planned negative control removes the provider variable too, so it cannot detect this false-positive branch.

3. **High — the claimed doctor integration cannot happen within the plan’s file scope.**

   `36-01-PLAN.md:151-153` claims:

   > “`devflow doctor`/preflight can report the two states separately.”

   But plan 01 modifies only `state.rs`, `agents/pi.rs`, and `agents/mod.rs`. Doctor’s agent list is hard-coded in [commands.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/commands.rs:2283) and contains only Claude, Codex, and OpenCode. It never calls adapter preflight. Pi will remain invisible to `devflow doctor`, violating `36-SPEC.md:39-40`.

4. **Critical — signing modifies an unused primitive and deletes the live backstop.**

   `36-02-PLAN.md:21` calls `release_finish`:

   > “The release tag path”

   The actual CLI explicitly says the release executor is “deferred, not-yet-built” in [main.rs](/var/home/denniyahh/Github/devflow/crates/devflow-cli/src/main.rs:291). Static search finds `release_finish` only at its definition and unit test—no production caller.

   The plan simultaneously deletes the pre-push fingerprint check. The existing hook states that fingerprint comparison is the only way to distinguish the maintainer and agent keys ([pre-push](/var/home/denniyahh/Github/devflow/scripts/hooks/pre-push:39)). The real release process remains manual. This hardens dead code while removing protection from the path actually used.

5. **High — plan 02 gives mutually impossible requirements for `tag()`.**

   `36-02-PLAN.md:61-68`:

   > “Only `release_finish` changes … `tag()` is left untouched.”

   `36-02-PLAN.md:80-82`:

   > “`release_finish` and `tag` read `devflow.releaseSigningKey`; if unset … return an error.”

   Task behavior again says leave `tag()` unsigned (`:109-113`), while verification requires both `release_finish` and `tag` to fail when the key is unset (`:128-130`). No implementation can satisfy both. The criteria also never create a signed tag and verify its fingerprint or exercise a wrong-key negative control.

## Handled

- Full review artifact: [SUMMARY.md](/var/home/denniyahh/Github/devflow/.planning/reviews/phase-36-adversarial/SUMMARY.md)
- `git diff --check` passed.
- External reviewers produced no findings: their invocations were blocked or terminated rather than transmitting repository documents without explicit authorization. No cross-model consensus is claimed.
- Evidence limits: the Pi probe covers Google on Pi 0.84.1; the caller search cannot rule out external consumers of the public Rust API.
diff --git a/.planning/reviews/phase-36-adversarial/SUMMARY.md b/.planning/reviews/phase-36-adversarial/SUMMARY.md
new file mode 100644
index 0000000000000000000000000000000000000000..09feca8e8919dd34aa31d648cd0800a0d4b0856d
--- /dev/null
+++ b/.planning/reviews/phase-36-adversarial/SUMMARY.md
