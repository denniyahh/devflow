# Code review — phase 36

**Headline: the Pi adapter does not work. Neither of its two process invocations survives `pi`'s argument parser.** I verified this against the real `pi` binary on this host (`/home/linuxbrew/.linuxbrew/bin/pi`), with negative controls, not by reading the docstrings. Separately, the branch is red: three integration tests still assert the deleted signing check.

The git.rs deletion itself is clean — that part I have no complaint about (details at the end).

---

## Needs my decision — none. Everything below is a defect to fix.

## Findings

### 1. `crates/devflow-core/src/agents/pi.rs:38` — the `--` separator makes *every* Pi run fail

`pi` has no `--` end-of-options convention. It parses `--` as an option name and rejects it.

```
$ pi -p --no-approve -- "do the thing"
Error: Unknown option: --

$ pi -p --no-approve "do the thing"          # negative control: only the `--` removed
"the thing" is ambiguous and this /tmp directory is ...   ← agent actually ran
```

The two arms differ only in the `--`, and they disagree, so the measurement is on the `--` and nothing else. Every `AgentKind::Pi` launch dies at arg parsing before the model is reached.

The `--` was added to protect a prompt beginning with `- [ ]`. It does not:

```
$ pi -p --no-approve -- "- [ ] task"      → Error: Unknown option: - [ ] task
$ pi -p --no-approve    "- [ ] task"      → Error: Unknown option: - [ ] task   (identical)
```

So the leading-dash hazard the docstring describes is **real and still unsolved** — dropping the `--` fixes the common case but leaves markdown-list prompts broken. Pi's help shows no separator flag; `pi [options] [@files...] [messages...]` suggests the prompt is positional with no escape hatch. This needs a workable answer (leading whitespace, or a non-dash preamble line), not just the `--` removed.

### 2. `crates/devflow-core/src/agents/pi.rs:57` — `preflight` can never return `Ok`

`pi auth check` requires a provider or model selector. Without one it never evaluates credentials at all:

```
$ pi auth check --json                        → exit 2, stdout EMPTY (error on stderr)
$ pi auth check --json --provider google      → exit 1, {"status":"not_ready",...}
```

The adapter passes no selector, so `classify_auth_check("", false)` returns `Err` unconditionally — on a fully credentialed host as much as an empty one. `preflight.rs:1270` calls this on the real start path, so Pi is blocked before it launches, and the operator is told *"no provider credential resolves"* when the actual fault is a malformed invocation. Add `--provider` (and source it from somewhere — see #4).

### 3. `crates/devflow-cli/tests/release_check.rs:327, 474, 536` — three tests still assert the deleted check; the branch is red

The signing probe was deleted from `git.rs` and `check_signing` from `commands.rs`, but this integration test file was never touched. Verified by running:

```
test result: FAILED. 7 passed; 3 failed
  release_check_inline_signingkey_is_not_reported_missing_and_leaks_no_key_material
  release_check_inline_signingkey_warns_without_probing
  release_check_signing_degrades_when_ssh_keygen_absent
```

They assert on `"an inline user.signingkey is not probed"`, a string no longer produced by any code. This is exactly the failure mode CLAUDE.md already records — *"a symbol search does not find tests that reference a deleted item through its strings"* — recurring for the third time.

### 4. `crates/devflow-core/src/agents/pi.rs:73` — the remediation names a command that does not exist

```
$ pi auth login
Error: Unknown auth command "login". Use "pi auth print-api-key", "pi auth print-bearer-token", or "pi auth check".
```

Related: the docstring at `pi.rs:51` reasons at length about `DEVFLOW_PI_PROVIDER`, which appears nowhere in the codebase except that comment. The provider name is the one thing #2 actually needs, and the comment dismisses it.

### 5. Two of the new unit tests cannot fail for the reason they claim

- `pi.rs:89` `exec_command_delimits_a_prompt_that_starts_with_a_dash` — asserts the argv *shape*, never that `pi` accepts it. It passes green over a command that is 100% rejected. The name asserts a delimiting behaviour that does not exist.
- `pi.rs:109` `classify_auth_check_accepts_ready` — feeds a hand-authored `{"status":"ready",...}` body that the production call site (#2) can never produce. The classifier's only success path is unreachable from the code that calls it.
- `agents/mod.rs:250` `default_preflight_is_ok_for_built_in_adapters` covers Claude/Codex/OpenCode and silently omits Pi. It is the one existing test that would have caught #2, and Pi was excluded from it without comment.

---

## Detail

**The git.rs deletion is clean.** Verified rather than assumed: `cargo build --workspace --tests` succeeds; `rg` over all non-`.planning` files finds zero surviving references to `check_signing_viability`, `SigningViability`, `SignProbeOutcome`, `ProbeWorkspace`, `probe_workspace_name`, `inline_signing_key_blob`, `SSH_SIGN_NAMESPACE`, or `classify_ssh_add_status` outside the two replacement comments and CHANGELOG prose. The `libc` dependency is not orphaned (still used by `monitor.rs` and `agent.rs`). The only fallout is #3.

**`cut-release.sh` tilde handling — I checked, it's fine.** The script validates readability against `${release_key/#\~/$HOME}` but passes the *unexpanded* value to `git -c user.signingkey=`. I tested whether git expands `~` there itself: it does — a tag signed with `~/key.pub` and the same key by absolute path both exit 0. No bug.

**Low, `commands.rs:2612` `version_components_gt`:** the parser splits on every non-digit and `take(3)`, so `2.5.0-rc.1` and `2.5.0` both parse to `[2,5,0]`. A prerelease mismatch fails the string equality check, then reports `Some(false)` → *"Cargo.toml ahead of changelog (release notes missing)"*, which is the wrong direction. Only the advisory `direction` string is affected, not the pass/fail verdict.

**What my checks do not establish:** I ran `release_check`, the two new unit tests, and the `agents::` unit tests — not the full workspace suite, so there may be further fallout I did not see. The `pi` observations are from one binary version on one host; I did not pin the version or check whether a newer `pi` adds `--` support. And #1/#2 are compile-clean and unit-test-green, which is precisely why nothing in this branch's own verification caught them — a green suite here licenses nothing about whether Pi runs.
