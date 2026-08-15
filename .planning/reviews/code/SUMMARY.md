# Peer Code Review — Phase 36

**Target:** the phase-36 diff (Pi adapter + release hardening) vs `develop`.
**Reviewers:** claude (opus, high) · codex (gpt-5.6-sol, high) · antigravity (Gemini 3.1 Pro).
**Verdict:** REQUEST CHANGES — the Pi adapter did not work; several real bugs fixed below.

## Consensus findings (fixed)

1. **CRITICAL — the `--` separator broke every Pi run.** `pi` has no `--` end-of-options
   convention; it rejects `--` as an unknown option. Removed; `exec_command` is now
   `pi -p --no-approve <prompt>`. (claude verified the two arms differ only in the `--`.)
2. **CRITICAL — `preflight` could never return `Ok`.** `pi auth check --json` requires a
   `--provider` selector (exit 2, empty stdout otherwise). Pinned `--provider google` (Pi's
   default) and parsed the JSON via `serde_json` (tolerating whitespace) instead of
   substring-matching.
3. **CRITICAL — `pi auth login` does not exist.** Remediation message corrected to
   `pi auth check` / `pi auth print-api-key`.
4. **CRITICAL — 3 integration tests red.** `release_check.rs` still asserted the deleted signing
   check; the four signing tests + their helpers were removed, and the stale `run_release`
   doc comment updated.
5. **HIGH — `cut-release.sh` passed the unexpanded `~` to git.** Now passes the tilde-expanded path.
6. **HIGH — `version_components_gt` conflated `2.5.0` with `2.5.0-rc.1`.** Replaced with
   `compare_versions` returning `Ordering` (with standard prerelease ordering).
7. **HIGH — changelog parser rejected `## [2.5.0]` and matched `## 1. Overview`.** Now strips
   brackets/`v` prefix and requires `X.Y` shape (digit, dot, digit).
8. **MEDIUM — `release --check` help still advertised tag-signing viability.** Updated `main.rs`
   doc + `OPERATIONS.md` + regenerated the help snapshot.
9. **Two unit tests could not fail for their claimed reason.** The `--` delimiter test was removed;
   the `classify_auth_check` tests now cover formatted JSON + the reachable success path.

## Not fixed (deferred / noted)

- **codex (HIGH, scope):** the changelog check compares two mutable files (Cargo.toml vs
  CHANGELOG) — "both stale at 2.5.0" passes. Comparing against an immutable baseline (latest
  release tag) is a follow-up; 999.96's recorded scope is "Cargo.toml vs CHANGELOG agree", and the
  version is derived from git tags at release time.
- **codex (MEDIUM):** `doctor` reports a credentialless Pi as healthy (its `pi` row checks only
  `--version`). The `preflight` (which runs on the start path) does the headless-capability check;
  wiring that into `doctor` is a follow-up.
- **antigravity (LOW):** the `cut-release.sh` readability guard assumes an SSH key *path* — a GPG
  key ID or inline `key::` blob would fail it. Consistent with the retained pre-push hook (which
  also does `ssh-keygen -lf` on the path), so SSH-path is the project's actual contract; noted for
  the GPG case if it ever arrives.

## Verification after fixes
- `cargo test -p devflow` — 21 test binaries, 0 failed.
- `cargo test -p devflow-core --lib` — 621 passed, 0 failed.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
