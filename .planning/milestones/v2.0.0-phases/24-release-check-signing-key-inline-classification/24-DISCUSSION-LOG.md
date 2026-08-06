# Phase 24: `release --check` Signing-Key Inline Classification - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-27
**Phase:** 24-release-check-signing-key-inline-classification
**Areas discussed:** Classification rule, Verdict for an inline key, Fingerprint acquisition, Blast radius on the path branch, Proof strategy

**Session note — how these were resolved.** This discuss stage ran
**unattended**, driven by DevFlow's Define stage (Phase 24 is the Phase 23
acceptance target: `devflow start --phase 24 --mode auto --yes-ship`). No
operator was available to answer `AskUserQuestion`, so each area was resolved
by the recommended option and logged here with the alternatives. Two choices
were settled by **executing a check rather than picking a default** — git's
own classification contract (`man git-config` § `user.signingKey`) and
`ssh-keygen -lf -` stdin support (run, exit 0) — and those are recorded as
verified facts in CONTEXT.md D-01/D-05, not as defaults. Anything genuinely
open was pushed to "Claude's Discretion" rather than silently decided.

---

## Classification rule — how do we tell an inline key from a path?

| Option | Description | Selected |
|--------|-------------|----------|
| Prefix allowlist per IN-01 | Treat `ssh-`, `ecdsa-`, `sk-` prefixes as inline | |
| Path-first fallback | Keep `key_path.exists()` as the primary test; only if the file is missing, ask "does it look like a key?" | |
| Mirror git's documented contract | `key::` prefix → inline (authoritative); else raw `ssh-` prefix → inline (git's deprecated compat form); else path | ✓ |

**Choice:** Mirror git's documented contract (D-01, D-02, D-03).
**Notes:** Settled by reading `man git-config` § `user.signingKey` on this
host rather than by preference. Git classifies **by prefix and never stats the
value**, which knocks out both alternatives:
- *Path-first fallback* is the intuitively safer ordering and was rejected
  precisely because it is not: for a file literally named `ssh-key.pub`, git
  sees an inline key while path-first would see a resolvable path and report
  viable for a config git will reject.
- *IN-01's prefix list* is wrong in the other direction — git's raw compat
  form is `ssh-`-only, so bare `ecdsa-…` / `sk-…` values are **paths** to git.
  Those algorithms reach the inline branch via `key::` without a second
  allowlist. The review's suggested fix is superseded, and CONTEXT.md D-03
  says so explicitly so the planner does not re-derive it.

---

## Verdict for a recognised inline key

| Option | Description | Selected |
|--------|-------------|----------|
| Corrected `NotViable` wording | Keep the failure, just stop saying "key file does not exist" | |
| Blanket `Unknown` | Non-blocking `warn`: "inline key configured — cannot verify from a path" | |
| Real verdict via `ssh-add -l` | Fingerprint the inline blob and run it through the same agent match the path branch uses | ✓ |

**Choice:** Real verdict via `ssh-add -l` (D-04), with `Unknown` retained as
the fail-soft floor (D-06).
**Notes:** *Corrected wording* was rejected outright — it preserves the actual
harm. Tracing `NotViable → Check{status:"fail"} → release_check` shows a
`"fail"` makes the command exit non-zero, so a correctly-configured operator
is **blocked from a release**, not merely misinformed. That reframing (Info
severity upstream → false blocking failure) is the single most consequential
output of this discussion.
*Blanket `Unknown`* is the cheap safe answer and would have been accepted if
viability were unknowable — but git's own text says *"The private key needs to
be available via ssh-agent"*, so for the inline form the agent match **is**
viability, not a proxy for it. The honest answer is reachable, so the check
should reach it. `Unknown` survives as the degradation target whenever
fingerprinting fails, never as the happy path.

---

## Fingerprint acquisition for the inline branch

| Option | Description | Selected |
|--------|-------------|----------|
| `ssh-keygen -lf -` via stdin | Write the key blob to the child's stdin | ✓ |
| Temp file + existing `public_key_fingerprint` | Materialise the blob, reuse the path helper untouched | |
| Parse the base64 and fingerprint in-process | Compute SHA256 over the wire format ourselves | |

**Choice:** stdin (D-05, D-09).
**Notes:** Verified before locking:
`printf 'ssh-ed25519 AAAA… test@example\n' | ssh-keygen -lf -` → exit 0,
stdout `256 SHA256:… test@example (ED25519)` — the same whitespace-token
layout `public_key_fingerprint` already parses at token index 1, so the parse
is shared rather than reinvented.
*Temp file* would have reused more code but writes operator key material to
disk for no reason and adds cleanup/permissions questions to an S-sized phase.
*In-process hashing* was rejected as reimplementing OpenSSH's wire format —
disproportionate, and a new correctness surface in a phase whose whole point
is correcting a classification.
Recorded separately (D-09) that the blob must go over **stdin, never argv** —
argv is world-readable via `/proc`, and this project already holds that line
(PROJECT.md, Phase 12). Noted so a later "simplification" to
`ssh-keygen -lf <blob>` is recognisably a regression rather than a tidy-up.

---

## Blast radius on the existing path branch

| Option | Description | Selected |
|--------|-------------|----------|
| Trust the existing coverage | ROADMAP states every path-based branch is already correct and tested | |
| Require a path-branch regression test | Re-establish today's behaviour under the new classification order | ✓ |

**Choice:** Require the regression test (D-12).
**Notes:** The ROADMAP's "already correct and tested" is true of the code as
it stands, but D-02 **changes the order in which classification happens**, so
that guarantee does not survive the change for free. Reading the actual test
inventory reinforced this: the `ssh` branch has no unit test of its own —
`git.rs`'s tests cover `classify_ssh_add_status` and the no-key degradation,
and the two integration tests in `release_check.rs` cover leak-prevention and
`ssh-add`-absent. Cheap insurance on the one arm operators actually depend on.

---

## Proof strategy — how do we test this without an ssh-agent?

| Option | Description | Selected |
|--------|-------------|----------|
| Assert the exact resulting verdict | Pin `Viable` / `NotViable` for a fixture key | |
| Assert the negative | The result must never be the "key file does not exist" `NotViable` | ✓ |
| Require a live agent in CI | Load a fixture key into a real `ssh-agent` for the test | |

**Choice:** Assert the negative (D-10), placed per the existing test split
(D-11).
**Notes:** *Exact verdict* is host-dependent — with an inline key the outcome
turns on whatever the runner's agent has loaded, which is exactly the kind of
environment coupling this suite already scrubs for (`release_check.rs` strips
`SSH_AUTH_SOCK` / `SSH_AGENT_PID` for this reason). *Live agent in CI* is
disproportionate for an S phase and adds a flake source. The negative
assertion is agent-independent and falsifies precisely the defect. Test
placement follows the existing split — unit tests in `git.rs`'s `mod tests`,
and the leak assertion extended in `release_check.rs` to cover the inline blob
— no new test files.

---

## Claude's Discretion

Recorded in CONTEXT.md for the planner/researcher rather than decided here:

- Shape of the fingerprint refactor (sibling function vs. source enum vs.
  inlined at the call site) — any shape satisfying D-05/D-09, smallest
  readable diff.
- Handling of a value that is exactly `key::` with an empty remainder —
  `Unknown` recommended per D-06, but the planner may argue otherwise; it must
  be an explicit call, not an omission.
- Whether the CLI reports the derived fingerprint for an inline `Viable` the
  same way it does for the path branch (consistency argues yes; no security
  reason not to).
- Refreshing `check_ssh_signing_viability`'s doc comment, whose "the key file
  must exist" wording becomes wrong once D-01 lands.

## Deferred Ideas

- **`gpg.ssh.defaultKeyCommand` fallback** — found on the same man page while
  verifying D-01. Git falls back to it (e.g. `ssh-add -L`) when
  `user.signingkey` is unset, so DevFlow's current
  `NotViable { "gpg.format=ssh but user.signingkey is not set" }` is a **false
  hard-fail of the same class** as this phase's bug. Deliberately not folded
  in: independent defect, its own surface (a configurable subprocess), and
  folding it would break the low-stakes-by-consequence property this phase was
  promoted for. Wants a backlog entry.
- **`gpg.format` values other than `ssh`/unset/`openpgp`** (e.g. `x509`) —
  silently routed to the GPG branch by the `_` arm. Noted only.
- **Steering operator docs toward the `key::` form** — git calls the raw
  `ssh-` form deprecated. DevFlow keeps accepting it; guidance is a docs
  change, not this phase.
