# Phase 24: `release --check` Signing-Key Inline Classification - Context

**Gathered:** 2026-07-27
**Status:** Ready for planning

<domain>
## Phase Boundary

Make `check_ssh_signing_viability` (`crates/devflow-core/src/git.rs:742-791`)
classify an **inline (literal) `user.signingkey` value** the way git itself
does, instead of assuming every value is a filesystem path.

**The defect.** The function does `Path::new(&signingkey)` then
`key_path.exists()`, and on a miss returns
`NotViable { reason: "user.signingkey is set but the key file does not exist" }`.
`check_signing` (`crates/devflow-cli/src/commands.rs:2036-2061`) maps
`NotViable` → status `"fail"`, and `release_check`
(`commands.rs:1880-1916`) turns any `"fail"` into
`Err(CliError::Message("release preflight failed"))`.

So this is **not** a cosmetic wording nit as the backlog entry frames it: an
operator whose signing setup is legitimately viable gets `devflow release
--check` exiting non-zero, with a reason that is factually wrong. That
escalation from "misleading diagnostic" to "false hard-fail on a correct
config" is the finding that actually motivates the phase.

**Scope: the `gpg.format=ssh` branch only.** One classification arm plus the
fingerprint path it needs, and the tests that prove it. Everything else in
`release --check` is untouched.

**Explicitly out of scope:** the `check_gpg_signing_viability` branch (it
passes the value straight to `gpg --list-secret-keys`, which is correct); the
other three preflight checks; `gpg.ssh.defaultKeyCommand` fallback (see
Deferred).

</domain>

<decisions>
## Implementation Decisions

### Classification rule — mirror git, do not invent a heuristic

- **D-01:** Classification follows **git's own documented contract**, verified
  against `man git-config` § `user.signingKey` on this machine (git docs,
  verbatim): *"Alternatively it can contain a public key prefixed with `key::`
  directly (e.g.: `key::ssh-rsa XXXXXX identifier`). … For backward
  compatibility, a raw key which begins with `ssh-`, such as `ssh-rsa XXXXXX
  identifier`, is treated as `key::ssh-rsa XXXXXX identifier`, but this form is
  deprecated; use the `key::` form instead."*

  Precedence, applied to the trimmed value:
  1. starts with `key::` → **inline**; strip the prefix, the remainder is the
     key blob.
  2. else starts with `ssh-` → **inline** (git's deprecated compat form).
  3. else → **path**; current behaviour, byte-for-byte unchanged.

  This is a preflight whose only job is to predict what git will do at
  `git tag -s` time. Any rule that diverges from git's is wrong by
  construction, even when it looks more forgiving.
  — **Reversibility:** reversible — one arm in one private function.

- **D-02:** **Prefix beats path-existence.** Git classifies by prefix and never
  stats the value, so a file that happens to be named `ssh-key.pub` in the
  working directory is an *inline key* to git and must be one to DevFlow too.
  Do **not** implement this as "try the path first, fall back to inline if the
  file is missing" — that reads as the safer ordering and is the one this
  discussion rejected, because it makes DevFlow report viable for a config git
  will reject.
  — **Reversibility:** reversible.

- **D-03 (corrects the source finding):** `20-REVIEW.md` IN-01 proposes
  detecting `ssh-` **or `ecdsa-`/`sk-`** prefixes. **Do not implement the
  `ecdsa-`/`sk-` half.** Git's raw compat form is `ssh-`-prefixed only; a bare
  `ecdsa-sha2-nistp256 AAAA…` or `sk-ssh-ed25519@openssh.com AAAA…` value is
  treated by git as a *path*. Those algorithms reach the inline branch through
  `key::` (rule 1), which covers them without a second allowlist. Widening the
  raw allowlist would make DevFlow disagree with git in the opposite
  direction. The planner should treat IN-01's prefix list as superseded by
  D-01 and not re-derive it.
  — **Reversibility:** reversible.

### Verdict for a recognised inline key

- **D-04:** An inline key gets a **real `Viable` / `NotViable` verdict via the
  same `ssh-add -l` fingerprint match the path branch already performs** — not
  a blanket `Unknown`. Justified by git's own text: *"The private key needs to
  be available via ssh-agent."* For the inline form the agent match is not an
  approximation of viability, it **is** viability, so the honest answer is
  reachable and the check should reach it.
  — **Reversibility:** reversible.

- **D-05:** Fingerprint acquisition for the inline branch is
  **`ssh-keygen -lf -` with the key blob written to stdin.** Verified on this
  machine before locking the decision:
  `printf 'ssh-ed25519 AAAA… test@example\n' | ssh-keygen -lf -` → exit 0,
  stdout `256 SHA256:… test@example (ED25519)`. Same whitespace-token layout
  the existing `public_key_fingerprint` already parses with
  `.split_whitespace().nth(1)`, so the parse is shared, not re-invented.
  — **Reversibility:** reversible.

- **D-06:** **Fail-soft is preserved and takes precedence over accuracy.** If
  `ssh-keygen` is absent, exits non-zero, or its output does not parse, the
  inline branch returns `Unknown` (→ CLI `"warn"`, non-blocking) — never
  `NotViable`. The whole point of the phase is that a false `"fail"` blocks a
  legitimate release; a new code path must not reintroduce one. `NotViable` is
  reserved for the case DevFlow can actually prove: fingerprint extracted, and
  `ssh-add -l`'s listing does not contain it.
  — **Reversibility:** reversible.

- **D-07:** The `NoAgent` / `AgentEmpty` / `Unknown(code)` arms of
  `classify_ssh_add_status` stay **shared between the path and inline
  branches**. They are about the agent, not about how the key was configured.
  Only fingerprint *acquisition* forks.
  — **Reversibility:** reversible.

### Security discipline (inherited, non-negotiable)

- **D-08:** No reason string may embed the configured `user.signingkey` value,
  in whole or in part — the same T-20-04 / ASVS V6 / WR-02 rule that already
  keeps filesystem paths out of the output, now extended to the inline blob.
  Reporting the derived `SHA256:` fingerprint on `Viable` remains permitted;
  that is the existing, reviewed contract and does not change here.
  — **Reversibility:** reversible.

- **D-09:** The key blob goes to `ssh-keygen` over **stdin**, never as an
  argv element. Argv is world-readable via `/proc`, and this project already
  holds the "no shell interpolation of agent-controlled data" line
  (PROJECT.md, Phase 12). D-05's mechanism satisfies this by construction —
  record it so a later refactor to `ssh-keygen -lf <blob>` is recognisably a
  regression.
  — **Reversibility:** reversible.

### Proof

- **D-10:** The inline classification must be **provable without an
  ssh-agent**. The falsifying assertion is negative and agent-independent:
  with `user.signingkey` set to a `key::ssh-ed25519 AAAA…` value (and to a raw
  `ssh-ed25519 AAAA…` value), the result must **never** be the
  `"key file does not exist"` `NotViable`. Whether the run then lands on
  `Viable`, agent-`NotViable`, or `Unknown` depends on the host's agent, so
  tests must not assert a specific one of those.
  — **Reversibility:** reversible.

- **D-11:** Test placement follows the existing split, no new files:
  unit tests in `crates/devflow-core/src/git.rs`'s `mod tests` (alongside
  `check_signing_viability_degrades_when_gpg_format_unset_and_no_signingkey`),
  and, if an operator-visible assertion is warranted, one integration test in
  `crates/devflow-cli/tests/release_check.rs` next to
  `release_check_signing_output_leaks_no_key_material_or_path` — extended to
  assert the *inline* blob never appears in stdout (D-08).
  — **Reversibility:** reversible.

- **D-12:** A path-branch regression test is **required, not optional**: a
  value that is a real path and does *not* start with `ssh-`/`key::` must
  still produce today's exact behaviour. D-02 changes the classification
  order, so "the path branch is already correct and tested" must be
  re-established rather than assumed.
  — **Reversibility:** reversible.

### Claude's Discretion

Not constrained here; the planner and researcher decide:

- **Shape of the refactor.** Whether `public_key_fingerprint` grows a sibling
  (`inline_key_fingerprint(&str)`), takes a small source enum, or the inline
  path is inlined at the call site. Any shape that satisfies D-05/D-09 is
  acceptable; keep it to the smallest diff that stays readable.
- **Empty-remainder handling** for a value of exactly `key::` (prefix present,
  blob empty). Recommend `Unknown` per D-06 rather than a parse error, but the
  planner may argue otherwise — just make it an explicit call, not an
  omission.
- **Whether the `Viable` fingerprint for an inline key is reported** in the
  CLI detail line the same way the path branch reports it. Consistency argues
  yes; there is no security reason not to.
- **Doc-comment refresh** on `check_ssh_signing_viability`, whose current
  wording ("the key file must exist") becomes wrong once D-01 lands.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### The finding this phase fixes
- `.planning/phases/20-release-correctness-operator-control/20-REVIEW.md`
  § "Info" → **`IN-01`** (lines 315-332) — the original review text. **Note the
  ID:** the ROADMAP entry cites it as `INF-01`, which does not exist in the
  file; the section is `IN-01`. Read D-03 before adopting its suggested fix —
  its `ecdsa-`/`sk-` prefix list is superseded.
- `.planning/ROADMAP.md` § "Phase 24" (line 1190) — goal, size (S), the
  promotion note, and the low-stakes-by-consequence rationale.
- `.planning/ROADMAP.md` § "Phase 999.27" (line 547) — the backlog entry this
  was promoted from; Linear **DEN-52**.

### Code this phase changes or depends on
- `crates/devflow-core/src/git.rs:742-791` — `check_ssh_signing_viability`;
  the misclassifying arm is `:748-753`.
- `crates/devflow-core/src/git.rs:722-736` — `public_key_fingerprint`; the
  `ssh-keygen -lf` invocation and the `.split_whitespace().nth(1)` parse the
  inline branch reuses (D-05).
- `crates/devflow-core/src/git.rs:687-702` — the `SigningViability` enum and
  its "never private key material or a full filesystem path" contract (D-08).
- `crates/devflow-core/src/git.rs:670-685` — `classify_ssh_add_status` /
  `SigningStatus`; shared across both branches per D-07.
- `crates/devflow-core/src/git.rs:706-717` — `git_config`, the fail-soft
  `git config --get` helper.
- `crates/devflow-cli/src/commands.rs:2036-2061` — `check_signing`; the
  `NotViable → "fail"` / `Unknown → "warn"` mapping that makes D-06 matter.
- `crates/devflow-cli/src/commands.rs:1880-1916` — `release_check`; any
  `"fail"` becomes a non-zero exit.

### Tests that constrain the change
- `crates/devflow-core/src/git.rs` `mod tests` (from `:845`) — in particular
  `classify_ssh_add_status` coverage (`:1518-1526`) and
  `check_signing_viability_degrades_when_gpg_format_unset_and_no_signingkey`
  (`:1538`), which shows the `HOME_ENV_MUTEX` + `unsafe set_var` restore
  pattern used for HOME-sensitive git config tests.
- `crates/devflow-cli/tests/release_check.rs:248-340` —
  `release_check_signing_output_leaks_no_key_material_or_path` (the T-20-04
  leak assertion) and `release_check_signing_degrades_when_ssh_add_absent`
  (the fail-soft assertion). Also read its header comment (`:16-17`) on why
  the fixture scrubs `SSH_AUTH_SOCK` / `SSH_AGENT_PID`.
- `.planning/phases/20-release-correctness-operator-control/20-UAT.md` —
  records that the Phase 20 live UAT used a **path-based** key, so the inline
  edge was never exercised by hand. Nothing there to re-run; it explains the
  coverage hole.

### External contract (authoritative — outranks the review text)
- `man git-config` § `user.signingKey` — the `key::` form, the deprecated raw
  `ssh-` form, and "The private key needs to be available via ssh-agent".
  This is the specification D-01 and D-04 are derived from. Quoted inline in
  D-01 so the planner need not have the man page to hand.
- `man ssh-keygen` § `-l` / `-f` — stdin (`-f -`) support relied on by D-05.

### Conventions
- `.planning/codebase/CONVENTIONS.md`, `.planning/codebase/TESTING.md` —
  house style and test placement.
- `CONTRIBUTING.md` — `core.hooksPath` / pre-push hermeticity; the suite
  scrubs `GIT_*` per 999.37.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`public_key_fingerprint`** (`git.rs:722`) — already does `ssh-keygen -lf`
  + whitespace-token-1 parse + fail-soft `None` on any error. The inline
  branch differs only in feeding stdin instead of a path argument; the parse
  and the error posture are reusable as-is.
- **`classify_ssh_add_status` / `SigningStatus`** (`git.rs:670-685`) — exit
  code 2/1/0/other → NoAgent/AgentEmpty/KeysListed/Unknown. Unit-tested and
  branch-agnostic; reuse unchanged (D-07).
- **`git_config`** (`git.rs:706`) — fail-soft `git config --get` scoped to
  `project_root`, `None` on unset/empty/failure. No change needed.
- **`devflow_core::test_support::{git_command, hermetic_command}`** — hermetic
  git fixture construction with the full `REPO_LOCAL_GIT_VARS` scrub, behind
  the off-by-default `test-support` feature. Any new test that shells out to
  git uses this, never a bare `Command::new("git")`.
- **`HOME_ENV_MUTEX` + save/restore `HOME`** — the established pattern for
  tests whose outcome depends on git's global config resolution
  (`git.rs:1538`). Reuse verbatim if a new test needs it.

### Established Patterns
- **Fail-soft, never panic** — every arm of the signing check degrades to
  `Unknown` with an actionable reason rather than erroring. D-06 is this
  pattern, not a new rule.
- **`NotViable` is expensive** — it is the only verdict that makes
  `release --check` exit non-zero. Reserve it for proven-bad, per D-06.
- **No `.unwrap()` / `.expect()` outside tests** — new error paths propagate
  or degrade.
- **Output redaction** — no filesystem paths, no key material, no usernames in
  any operator-visible string (T-20-04 / ASVS V6 / WR-02).
- **Per-`Command` `env_remove`, never process-global `set_var`** — established
  by 999.37; `std::env::set_var` is `unsafe` in Rust 2024 and unsound in a
  threaded test binary. The one existing exception is the HOME test above,
  which is explicitly mutex-serialised and restores in both arms.
- **No new dependencies** for something this small — `std` + already-present
  crates only.

### Integration Points
- `check_signing_viability` (`git.rs:829`) → `check_ssh_signing_viability` —
  the only caller of the changed function; its `gpg.format` branch is
  untouched.
- `check_signing` (`commands.rs:2036`) → `Check { status }` → `release_check`
  aggregation → process exit code. This is the chain that turns a
  classification bug into a blocked release; nothing in it needs to change,
  which is the point.
- Writing to a child's stdin requires `Stdio::piped()` + `.spawn()` +
  `wait_with_output()` rather than the `.output()` one-shot used everywhere
  else in this module — the one genuinely new mechanism in the phase.

</code_context>

<specifics>
## Specific Ideas

- **Severity is understated upstream, deliberately not re-scoped.** The
  backlog entry and ROADMAP call this a "slightly misleading diagnostic"
  (Info severity). Tracing `NotViable → "fail" → non-zero exit` shows it is a
  false blocking failure on a correct configuration. The *fix* stays exactly
  as scoped — single classification branch plus tests — but the planner should
  write acceptance criteria against "a viable inline config is not reported as
  a failure", not against "the wording is nicer".
- **Two verified facts were established during this discussion rather than
  assumed**, and are quoted in D-01/D-05 so no downstream agent needs to
  re-derive them: git's `key::`-vs-raw-`ssh-` precedence (from `man
  git-config` on this host), and that `ssh-keygen -lf -` accepts a key on
  stdin (executed, exit 0, expected output shape). If either is contradicted
  by a newer git/OpenSSH, that invalidates D-01/D-05 and the phase should be
  re-discussed, not patched around.
- **This phase is also the Phase 23 acceptance target** (promoted 2026-07-26
  as the low-stakes payload for a `devflow start --phase 24 --mode auto
  --yes-ship` run). That does not change what gets built, but it does mean
  scope pressure resolves toward *staying small*: a phase that grows here
  degrades its usefulness as a controlled acceptance run.

</specifics>

<deferred>
## Deferred Ideas

- **`gpg.ssh.defaultKeyCommand` fallback.** Git's docs: *"If not set Git will
  call `gpg.ssh.defaultKeyCommand` (e.g.: `ssh-add -L`) and try to use the
  first key available."* DevFlow's current
  `NotViable { "gpg.format=ssh but user.signingkey is not set" }`
  (`git.rs:743-747`) is therefore also wrong for anyone relying on that
  fallback — another false hard-fail, same class as this phase's bug, found
  while reading the same man page. **Not folded in:** it is a second,
  independent classification defect with its own surface (a configurable
  subprocess), and folding it would break the low-stakes-by-consequence
  property this phase was promoted for. Worth a backlog entry.
- **`gpg.format` values other than `ssh` / unset / `openpgp`** — e.g. `x509`.
  Currently silently routed to the GPG branch by the `_` arm at `git.rs:832`.
  Untouched; note only.
- **Deprecating the raw `ssh-` form in DevFlow's own guidance** — git calls it
  deprecated. DevFlow should keep *accepting* it (D-01 rule 2), but operator
  docs could steer toward `key::`. Documentation, not this phase.

</deferred>

---

*Phase: 24-release-check-signing-key-inline-classification*
*Context gathered: 2026-07-27*
