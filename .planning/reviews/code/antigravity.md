## Needs your decision

No architectural trade-offs require a decision before these fixes can be applied. The 5 findings below are concrete defects and brittle implementations in the new additions.

---

## Needs your awareness

### 1. `cut-release.sh` breaks GPG and inline SSH signing keys, and passes unexpanded tilde paths
- **Location:** [`scripts/cut-release.sh:L128-L138`](file:///scripts/cut-release.sh#L128-L138)
- **Problem:**
  1. Lines 128–132 run `[ ! -r "$release_key_expanded" ]`. This unconditionally assumes `devflow.releaseSigningKey` is a path to a file on disk. If the maintainer configures an OpenPGP key ID/fingerprint (e.g. `0x3AA5C34371567BD2`) or an inline SSH public key (`key::ssh-ed25519 AAAAC3...`), `[ ! -r ... ]` fails and aborts release cutting with `points at an unreadable file`.
  2. Line 128 computes `release_key_expanded` by expanding `~`, but line 137 passes the raw, unexpanded `"$release_key"` to `git -c user.signingkey="$release_key"`. Git's `-c` override does not perform shell expansion, risking signing failure when a path starting with `~/` is passed.
- **Fix:** If the key starts with `key::` or matches a hex key ID / does not contain path separators, bypass the filesystem existence check. When invoking `git -c user.signingkey=...`, pass `"$release_key_expanded"`.

---

### 2. `version_components_gt` conflates equality with "Cargo.toml ahead"
- **Location:** [`crates/devflow-cli/src/commands.rs:L2624-L2645`](file:///crates/devflow-cli/src/commands.rs#L2624-L2645)
- **Problem:**
  - The doc comment for [`version_components_gt`](file:///crates/devflow-cli/src/commands.rs#L2624-L2637) states: `Some(true) if a > b, Some(false) if a < b, None if either is unparseable`.
  - The implementation actually returns `Some(a > b)`. When `a == b` component-wise (for example, comparing `2.5.0` with `2.5.0-alpha` or `2.5.0-rc1`), `a > b` evaluates to `false`.
  - In [`check_changelog_version`](file:///crates/devflow-cli/src/commands.rs#L2611-L2622), `changelog_version == workspace_version` is false (string mismatch), so it falls into the `else` branch. `version_components_gt` returns `Some(false)`, which is mapped to `"Cargo.toml ahead of changelog (release notes missing)"`.
- **Fix:** Return an `Option<std::cmp::Ordering>` (or distinguish `Ordering::Greater`, `Ordering::Less`, and `Ordering::Equal`) so identical numeric components with prerelease tags do not produce a false "Cargo.toml ahead" diagnostic.

---

### 3. Changelog parser breaks on standard `[X.Y.Z]` headings and matches numbered list sections
- **Location:** [`crates/devflow-cli/src/commands.rs:L2595-L2604`](file:///crates/devflow-cli/src/commands.rs#L2595-L2604)
- **Problem:**
  - `changelog_version` extracts the version with:
    ```rust
    line.trim()
        .strip_prefix("## ")
        .and_then(|rest| rest.split_whitespace().next())
        .filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()))
    ```
  - If a changelog uses Keep a Changelog standard link formatting such as `## [2.5.0] - 2026-08-15`, the first character is `'['`. `is_ascii_digit()` rejects it, causing `check_changelog_version` to fail to find any version heading and return a `warn`.
  - Conversely, any top-level numbered section like `## 1. Overview` has `'1'` as its first character and will be parsed as version `"1."`.
- **Fix:** Strip leading brackets/`v` prefixes (e.g. `rest.trim_start_matches(|c| c == '[' || c == 'v')`) and strip trailing `]` or punctuation before matching against `workspace_version`.

---

### 4. `classify_auth_check` fails on formatted JSON with spaces
- **Location:** [`crates/devflow-core/src/agents/pi.rs:L66-L75`](file:///crates/devflow-core/src/agents/pi.rs#L66-L75)
- **Problem:**
  - `classify_auth_check` checks `stdout.contains("\"status\":\"ready\"")` using exact substring matching without spaces.
  - If `pi auth check --json` outputs formatted JSON (e.g., `{"status": "ready"}` or multi-line pretty-printed JSON), the exact match fails even on exit status 0.
  - The unit test at [`pi.rs:L109-L116`](file:///crates/devflow-core/src/agents/pi.rs#L109-L116) only tests artificially minified JSON (`r#"{"status":"ready",...}"#`), creating a false positive in test coverage.
- **Fix:** Parse the output via `serde_json::from_str::<serde_json::Value>` (or check for `"status"` and `"ready"` tokens tolerating whitespace) and report `stderr` when `preflight` fails.

---

### 5. `changelog_version_check` test lacks negative control for inverted version order
- **Location:** [`crates/devflow-cli/src/commands.rs:L6643-L6674`](file:///crates/devflow-cli/src/commands.rs#L6643-L6674)
- **Problem:**
  - The test [`changelog_version_check_flags_mismatch_and_passes_on_agreement`](file:///crates/devflow-cli/src/commands.rs#L6643-L6674) tests only the case where `workspace_version` is ahead of `changelog_version` (`2.5.0` vs `2.4.0`).
  - It does not test the reverse case (changelog ahead of Cargo.toml: `2.6.0` vs `2.5.0`), nor does it test equal components with prerelease tags.
- **Fix:** Add assertions for `changelog 2.6.0` vs `workspace 2.5.0` expecting `"changelog ahead of Cargo.toml"`, plus an assertion for bracketed version headings `## [2.5.0]`.

---

## Handled

- **Bulk deletion verification in [`crates/devflow-core/src/git.rs`](file:///crates/devflow-core/src/git.rs#L753-L761):** 
  - Verified that [`SigningViability`](file:///crates/devflow-core/src/git.rs), `check_signing_viability`, `SignProbeOutcome`, and all probe helper functions were cleanly excised.
  - The single external caller in [`crates/devflow-cli/src/commands.rs`](file:///crates/devflow-cli/src/commands.rs#L2404) was replaced with `check_changelog_version`.
  - Unused imports (`Stdio`, `Duration`, `Instant`) were removed; no dangling references or broken symbols remain in the crate.
- **`AgentKind::Pi` wiring:** [`crates/devflow-core/src/state.rs`](file:///crates/devflow-core/src/state.rs#L391-L426) and [`crates/devflow-core/src/agents/mod.rs`](file:///crates/devflow-core/src/agents/mod.rs#L65-L93) correctly wire `AgentKind::Pi`, `Display`, `FromStr`, and `adapter_for`.
