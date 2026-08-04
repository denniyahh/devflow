# Phase 24: `release --check` Signing-Key Inline Classification - Research

**Researched:** 2026-07-27
**Domain:** Rust — git config parsing, child-process stdin piping, fail-soft CLI diagnostics
**Confidence:** HIGH

## Summary

This phase fixes one classification arm in `check_ssh_signing_viability`
(`crates/devflow-core/src/git.rs:742-791`). Today the function treats every
`user.signingkey` value as a filesystem path; git itself treats a value
prefixed `key::` or (deprecated, backward-compat) `ssh-` as an **inline
public key blob** and never stats it as a path. The fix is: classify by
prefix first (git's own precedence, quoted verbatim in CONTEXT.md D-01),
then route inline values to a new fingerprint-acquisition path that pipes
the blob to `ssh-keygen -lf -` over stdin (never argv, never a temp file),
reusing the existing `ssh-add -l` match logic unchanged.

Two facts this research had to get right are not textbook Rust — they were
independently verified against the live toolchain in this session, not
assumed:

1. **`ssh-keygen -lf -` accepts stdin** (`-f -`) and produces the identical
   whitespace-token-1 output shape `public_key_fingerprint` already parses.
   Confirmed live: `OpenSSH_10.4p1` on this host, exit 0, correct
   `SHA256:...` output for both the `key::` form and the raw `ssh-` form.
2. **The idiomatic `Stdio::piped()` + `spawn()` + `.stdin.take()` +
   `write_all()` + `wait_with_output()` pattern compiles and runs correctly
   under this workspace's exact toolchain** (`rustc 1.97.1`, edition 2024).
   Verified by compiling and running three throwaway variants against a
   real ed25519 keypair, including the empty-blob and garbage-input edge
   cases D-06's discretion note asks the planner to make an explicit call
   on. See Code Examples and Common Pitfalls below for what the borrow-only
   variant (no `.take()`) silently gets away with on this host and why that
   is not something to rely on.

**Primary recommendation:** Add one new private helper,
`inline_key_fingerprint(key_blob: &str) -> Option<String>`, mirroring
`public_key_fingerprint`'s signature/error-posture/parse but sourcing from
stdin instead of a path argument; branch `check_ssh_signing_viability` on a
new pure classifier (`classify_signingkey_source` or inlined `if`/`else if`
— Claude's Discretion per D-01) before today's `Path::new(&signingkey)`
line; leave everything downstream of fingerprint acquisition (the
`ssh-add -l` match, `classify_ssh_add_status`, all three
`SigningViability` arms) untouched, satisfying D-07.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| `user.signingkey` classification (path vs. inline) | Core library (`devflow-core::git`) | — | Pure, deterministic string classification; no I/O, must stay unit-testable without a live agent (D-10) |
| Inline key fingerprint acquisition | Core library (`devflow-core::git`) | OS subprocess (`ssh-keygen`) | Same tier as the existing path-based `public_key_fingerprint` — a sibling helper, not a new layer |
| Agent match / viability verdict | Core library (`devflow-core::git`) | OS subprocess (`ssh-add`) | Unchanged — shared between both branches per D-07 |
| CLI status mapping (`NotViable`→`fail`, `Unknown`→`warn`) | CLI (`devflow-cli::commands::check_signing`) | — | Unchanged; this phase's fix is upstream of this mapping, which is exactly why the false-`fail` bug propagates through it untouched today |
| Preflight aggregation / exit code | CLI (`devflow-cli::commands::release_check`) | — | Unchanged; consumes `Check.status` only |

This phase touches exactly one tier (core library, one function + one new
sibling helper). No CLI, no cross-tier change — consistent with the
Size-S / low-stakes-by-consequence framing in the phase description and
CONTEXT.md's "Specific Ideas" note about scope pressure.

## Standard Stack

No new dependencies. `std::process::{Command, Stdio}` and `std::io::Write`
are the entire mechanism — both already `std`, no crate to add. `git.rs`
currently imports `std::process::Command` only (`git.rs:5`); `Stdio` and
`std::io::Write` are the two new imports this phase needs.

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `std::process` | (workspace `rustc 1.97.1`, edition 2024) | Spawn `ssh-keygen` with piped stdin/stdout/stderr | Already the mechanism `public_key_fingerprint` and every other subprocess call in `git.rs` uses; no reason to add a process-management crate for one child |
| `std::io::Write` | stdlib | `.write_all()` on the child's stdin handle | Required trait import; not currently in scope in `git.rs` |

### Supporting
None. `tempfile` (dev-dependency, already used throughout `git.rs`'s test
module) covers any new test fixtures; no new supporting library.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `Stdio::piped()` + `.take()` + `write_all()` + `wait_with_output()` | Write the blob to a `tempfile::NamedTempFile` and pass its path as `-f <path>` | Rejected by D-09 explicitly: argv/filesystem exposure of the key blob is the exact risk the project's "no shell interpolation of agent-controlled data" line (PROJECT.md, Phase 12) exists to prevent. Stdin is strictly better and no harder to implement. |
| Single-threaded write-then-wait (this phase's blob sizes) | Spawn a writer thread before `wait_with_output()` (the general-purpose pattern for large payloads, per Rust's own `Stdio` docs) | Not needed here — see Common Pitfalls "Pipe buffer deadlock" below for why a public SSH key blob (tens to low hundreds of bytes) cannot trigger the deadlock the threaded pattern exists to avoid. Adding a thread would be unjustified complexity for this input size. |

**Installation:** none — no `Cargo.toml` change.

**Version verification:** N/A — no package versions to verify; this is a
`std`-only change on the workspace's pinned toolchain (`rust-toolchain.toml`
per CONVENTIONS.md; live `rustc --version` on this host: `1.97.1
(8bab26f4f 2026-07-14)`, edition 2024 confirmed via `Cargo.toml:10`).

## Package Legitimacy Audit

**Not applicable.** This phase installs zero external packages — no new
`Cargo.toml` dependency, no `cargo add`. The entire mechanism is `std` plus
the already-present `ssh-keygen`/`ssh-add` external binaries (unchanged
invocation surface, just a different flag/stdin source for one of them).

**Packages removed due to [SLOP] verdict:** none — none proposed.
**Packages flagged as suspicious [SUS]:** none — none proposed.

## Architecture Patterns

### System Architecture Diagram

```
git config user.signingkey <value>
            │
            ▼
   check_ssh_signing_viability(project_root)
            │
            ├─ git_config(project_root, "user.signingkey")  ── None ──▶ NotViable
            │         │ Some(signingkey)
            │         ▼
            │   classify by prefix on the TRIMMED value (D-01, no path stat)
            │         │
            │    ┌────┴─────────────────┬───────────────────┐
            │    │ starts "key::"       │ starts "ssh-"      │ else
            │    ▼ (strip prefix)       ▼ (raw blob)         ▼
            │  INLINE key blob      INLINE key blob        PATH (unchanged)
            │    │                      │                    │
            │    ▼                      ▼                    ▼
            │  inline_key_fingerprint  inline_key_fingerprint  Path::new(&signingkey)
            │  (ssh-keygen -lf -,      (same fn)              .exists() check
            │   blob via STDIN,                                    │
            │   never argv — D-09)                                 ▼
            │    │                                          public_key_fingerprint
            │    │  None → Unknown (D-06, fail-soft)        (ssh-keygen -lf <path>)
            │    │  Some(fp) ──────────────┐                       │
            │    └────────────────────────►│◄──────────────────────┘
            │                              ▼
            │                   ssh-add -l  (shared, D-07 — unchanged)
            │                              │
            │                classify_ssh_add_status(exit_code)
            │                   NoAgent / AgentEmpty / KeysListed / Unknown(code)
            │                              │
            │                    KeysListed: stdout.contains(&fingerprint)?
            │                              │
            │            ┌─────────────────┼──────────────────┐
            │            ▼                 ▼                  ▼
            │        Viable{fp}      NotViable{"not          Unknown{"ssh-keygen
            │      (fp in stdout)     the configured           not found/
            │                         key"}                    unreadable"}
            ▼
     check_signing (commands.rs:2036) — NotViable→"fail", Unknown→"warn", Viable→"ok"
            ▼
     release_check (commands.rs:1880) — any "fail" → non-zero exit
```

### Recommended Project Structure

No new files or directories. Everything lands inside the existing
`crates/devflow-core/src/git.rs`:

```
crates/devflow-core/src/git.rs
├── SigningStatus / classify_ssh_add_status        (unchanged, D-07)
├── SigningViability enum                          (unchanged)
├── git_config                                     (unchanged)
├── public_key_fingerprint                         (unchanged — path branch keeps using it)
├── inline_key_fingerprint                          ← NEW sibling helper (D-05/D-09)
├── check_ssh_signing_viability                     ← MODIFIED: new classification arm
├── check_gpg_signing_viability                    (unchanged — out of scope)
├── check_signing_viability                        (unchanged — dispatcher)
└── mod tests
    ├── classify_ssh_add_status_maps_...            (unchanged, existing)
    ├── check_signing_viability_degrades_...         (unchanged, existing)
    ├── <new> inline key never misclassified as path-not-found (D-10, D-11)
    └── <new> path-branch regression (D-12)

crates/devflow-cli/tests/release_check.rs
└── <optional, per D-11> extend the existing leak assertion to cover the
    inline blob, next to release_check_signing_output_leaks_no_key_material_or_path
```

### Pattern 1: Prefix classification before path existence (D-01, D-02)

**What:** Classify the trimmed `user.signingkey` value by string prefix,
in git's own documented precedence, *before* any filesystem check.

**When to use:** Any time a config value can be either "a path to X" or "X
itself" and the two are string-ambiguous (a working-directory file
literally named `ssh-key.pub` is exactly this ambiguity, per D-02).

**Example** (illustrative shape — exact enum/inlining is Claude's
Discretion per CONTEXT.md):
```rust
// Mirrors `man git-config`'s user.signingKey precedence (D-01), quoted in
// 24-CONTEXT.md: key:: form, then deprecated raw ssh- form, else a path.
let trimmed = signingkey.trim();
let inline_blob: Option<&str> = if let Some(rest) = trimmed.strip_prefix("key::") {
    Some(rest)
} else if trimmed.starts_with("ssh-") {
    Some(trimmed)
} else {
    None
};

let fingerprint = match inline_blob {
    Some(blob) => inline_key_fingerprint(blob),         // NEW branch
    None => {
        let key_path = Path::new(&signingkey);
        if !key_path.exists() {
            return SigningViability::NotViable {
                reason: "user.signingkey is set but the key file does not exist".into(),
            };
        }
        public_key_fingerprint(key_path)                 // existing branch, byte-for-byte
    }
};
```

### Pattern 2: Writing to a child process's stdin (the one new mechanism)

**What:** `ssh-keygen -lf -` reads the key from stdin instead of a file
path. Every other subprocess call in `git.rs` uses the one-shot
`Command::...output()` (no stdin needed); this is the first stdin-writing
call in the module.

**When to use:** Whenever a subprocess needs data that must never touch
argv (D-09) or a temp file, and the child reads a bounded amount from
stdin (see Common Pitfalls for the size caveat).

**Example — verified by compiling and running against a real
ed25519 keypair on this host** (`rustc 1.97.1`, edition 2024; exit 0,
correct `SHA256:...` fingerprint returned):
```rust
// Source: verified locally this session; pattern matches the canonical
// example in https://doc.rust-lang.org/std/process/struct.Stdio.html
// (Command::new("rev") with piped stdin/stdout, .stdin.take(), write_all,
// wait_with_output).
use std::io::Write;
use std::process::{Command, Stdio};

fn inline_key_fingerprint(key_blob: &str) -> Option<String> {
    let mut child = Command::new("ssh-keygen")
        .args(["-lf", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    // MUST take ownership and drop the handle to close the pipe before
    // wait_with_output — see Common Pitfalls "borrow vs. take()" below.
    let mut stdin = child.stdin.take()?;
    stdin.write_all(key_blob.as_bytes()).ok()?;
    drop(stdin);

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    // Same format as public_key_fingerprint: "<bits> SHA256:<hash> <comment> (<type>)"
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}
```
Verified outputs against this exact function during research:
- valid ed25519 public key blob → `Some("SHA256:HVcu...LVVU")`, exit 0
- empty string (`""`, the D-06-discretion "value is exactly `key::`" case)
  → `None` (ssh-keygen exits non-zero on empty stdin)
- garbage text (`"not a key\n"`) → `None`

### Anti-Patterns to Avoid
- **Path-first, inline-as-fallback:** "try the path, fall back to inline if
  the file is missing" is explicitly rejected by D-02 — it makes DevFlow
  report viable for a config git will reject at `git tag -s` time. Prefix
  wins unconditionally.
- **Re-deriving `ecdsa-`/`sk-` prefix detection:** D-03 explicitly
  supersedes `20-REVIEW.md` IN-01's proposed prefix list. Those algorithms
  reach the inline branch through `key::` only; do not add a second raw
  allowlist.
- **Writing the blob to argv or a temp file:** D-09 forbids both — argv is
  world-readable via `/proc`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Fingerprint parsing | A new/duplicate `SHA256:` extraction regex or parser | The exact `.split_whitespace().nth(1)` idiom `public_key_fingerprint` already uses | `ssh-keygen -l` output format is identical whether the input came from a path or stdin (verified this session); a second parser would be pure duplication with no behavioral upside |
| Agent-match / exit-code semantics | A separate "inline" `ssh-add` classification | `classify_ssh_add_status` unchanged, shared per D-07 | The agent doesn't know or care how the key was configured; only fingerprint *acquisition* differs |

**Key insight:** This phase's entire diff should be additive (one new
helper) plus one classification branch. Nothing downstream of "get me a
fingerprint" changes, and D-07 makes that explicit so the planner doesn't
accidentally fork logic that has no reason to fork.

## Common Pitfalls

### Pitfall 1: Borrowing `child.stdin` instead of `.take()`-ing it

**What goes wrong:** `child.stdin.as_mut()?.write_all(...)` compiles and,
on this host, even *worked* in a throwaway test against a real key (it did
not hang). But that is `ssh-keygen`'s own line-based read behavior, not a
documented contract — it is not something a fix should rely on. The
official/idiomatic pattern (confirmed via Rust's own `Stdio` docs and via
independent testing this session) is `let mut stdin = child.stdin.take()?;
... drop(stdin);` **before** `wait_with_output()`, so the pipe is
positively closed regardless of how the child reads its input.
**Why it happens:** `Option<ChildStdin>::as_mut()` returns a `&mut
ChildStdin` whose scope ending does not drop the underlying `Option`
field/fd — only `.take()` (or the field going out of scope entirely) does.
**How to avoid:** Always `.take()` the stdin handle, write, then explicitly
`drop()` (or let it go out of scope) before calling `wait_with_output()`.
**Warning signs:** A test that passes for one key but hangs (or is flaky
under CI timing) for a differently-shaped input is the tell — this is
exactly the kind of thing that would NOT be caught by CI-on-branch that
happens to use the same fixture repeatedly.

### Pitfall 2: Pipe buffer deadlock (why it does NOT apply here, but is worth naming)

**What goes wrong:** The classic `Stdio::piped()` deadlock: if you write
more to a child's stdin than the OS pipe buffer holds (commonly 64KB on
Linux) *before* reading its stdout, and the child is itself blocked
writing to a full stdout pipe waiting for you to read, both processes
block forever. Rust's own docs recommend a writer thread for this reason.
**Why it doesn't apply here:** An SSH public key blob (`ssh-ed25519 AAAA...
comment`) is on the order of 80-800 bytes — several orders of magnitude
under the pipe buffer, and `ssh-keygen -lf -`'s own stdout (`"<bits>
SHA256:<hash> <comment> (<type>)"`) is similarly tiny. Single write, then
`wait_with_output()`, is safe for this input class specifically.
**How to avoid re-introducing the risk:** Don't generalize this helper to
accept arbitrarily large stdin later without revisiting this assumption.
**Warning signs:** None expected in this phase — flagged for the record so
a future reviewer doesn't need to re-derive why the simple form is safe
here.

### Pitfall 3: Re-litigating D-01/D-05 as "safer to double-check"

**What goes wrong:** It is tempting to add a defensive path-existence
check *in addition to* the prefix check ("classify as inline, but also
verify the string doesn't happen to be a valid path, just in case"). D-02
explicitly rejects any ordering that consults path-existence for a
prefix-matched value — git never does, so DevFlow reporting differently
would itself be the bug.
**Why it happens:** Feels safer; isn't — it just reintroduces
divergence-from-git in a different shape.
**How to avoid:** Trust D-01/D-02 as locked; don't add extra conditionals
the decision record already argued against.
**Warning signs:** Any new code path that calls `.exists()` on a value
that already matched the `key::`/`ssh-` prefix.

### Pitfall 4: Treating `NotViable` as reachable from the inline branch on tool absence

**What goes wrong:** If `ssh-keygen` errors, is absent, or its output
doesn't parse, it's tempting to return `NotViable` ("couldn't verify the
key") since that reads as more actionable. D-06 requires `Unknown`
instead — `NotViable` is reserved for the one provable-bad case (agent
reachable, fingerprint doesn't match its listing). Getting this wrong
reintroduces exactly the false-hard-fail class this phase exists to
eliminate, just on a new code path.
**How to avoid:** `inline_key_fingerprint` returning `None` must route to
`SigningViability::Unknown`, mirroring the existing
`public_key_fingerprint` → `None` → `Unknown` arm at `git.rs:780-784`.
**Warning signs:** A new test asserting `NotViable` for a missing
`ssh-keygen` — that assertion itself would be wrong per D-06 and should be
corrected, not the code.

## Code Examples

### Existing pattern this phase must NOT change (`ssh-add -l` match, `git.rs:771-786`)

```rust
// Source: crates/devflow-core/src/git.rs:771-786 (verified live at HEAD)
SigningStatus::KeysListed => {
    let stdout = String::from_utf8_lossy(&output.stdout);
    match public_key_fingerprint(key_path) {   // <- becomes inline_key_fingerprint(blob)
                                                //    on the inline arm only
        Some(fingerprint) if stdout.contains(&fingerprint) => SigningViability::Viable {
            fingerprint: Some(fingerprint),
        },
        Some(_) => SigningViability::NotViable {
            reason: "ssh-agent has keys loaded, but not the configured signing key".into(),
        },
        None => SigningViability::Unknown {
            reason: "cannot verify signing viability — ssh-keygen not found or the key \
                     is unreadable"
                .into(),
        },
    }
}
```
This block is the same regardless of which branch produced `fingerprint` —
D-07's "only fingerprint acquisition forks" in code. The cleanest shape is
likely to compute `fingerprint: Option<String>` once (from either
`inline_key_fingerprint` or `public_key_fingerprint`) *before* this match,
then leave this block completely untouched, referencing a local `fingerprint`
variable instead of re-calling `public_key_fingerprint(key_path)` inline.

### Existing HOME-isolation test idiom to reuse for a D-10 test (`git.rs:1537-1567`)

```rust
// Source: crates/devflow-core/src/git.rs:1537-1567 (verified live at HEAD)
// Reuse HOME_ENV_MUTEX + save/restore HOME exactly as-is for any new test
// whose outcome depends on git config resolution — this project's own dev
// machine sets gpg.format=ssh / user.signingkey GLOBALLY, so an inline-key
// test must isolate $HOME/.gitconfig the same way this existing test does.
static HOME_ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn check_signing_viability_degrades_when_gpg_format_unset_and_no_signingkey() {
    let _lock = HOME_ENV_MUTEX.lock().unwrap();
    let repo = init_repo();
    let root = repo.path();
    let fake_home = tempfile::tempdir().unwrap();
    let original_home = std::env::var_os("HOME");
    unsafe { std::env::set_var("HOME", fake_home.path()) };
    let result = check_signing_viability(root);
    match original_home {
        Some(home) => unsafe { std::env::set_var("HOME", home) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    // ... assertion
}
```
A D-10 test sets `user.signingkey` to a `key::ssh-ed25519 AAAA…` value (and
separately to a raw `ssh-ed25519 AAAA…` value) inside a repo-local
`git config` call (no HOME isolation needed if `gpg.format=ssh` is also set
repo-locally — the existing tests only isolate HOME for the *unset* case).
The falsifying assertion per D-10 is negative: `result` must never be
`SigningViability::NotViable { reason }` where `reason` is the literal
string `"user.signingkey is set but the key file does not exist"`. It must
NOT assert a specific one of `Viable`/agent-`NotViable`/`Unknown` (agent
state varies by host/CI).

### Existing D-12 regression precedent — path branch must be unchanged

No existing unit test in `git.rs`'s `mod tests` currently exercises the
path branch of `check_ssh_signing_viability` directly with a real
existing key file (`check_signing_viability_degrades_when_gpg_format_unset...`
only covers the *unset* case). The integration test
`release_check_signing_output_leaks_no_key_material_or_path`
(`crates/devflow-cli/tests/release_check.rs:257-303`) does exercise a real
path-based key end-to-end (via `ssh-keygen -t ed25519 -f ...` fixture
generation) and is the closest existing coverage — D-12's new regression
test should assert on a value that is a real path **and does not start
with `ssh-`/`key::`** (the existing fixture's generated key path already
qualifies, since tempdir paths don't start with those prefixes) still
producing byte-for-byte the current `Viable`/agent-state behavior.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| `git config user.signingKey` accepted only a path to a public key file | Git 2.34+ added `gpg.format=ssh`; later git versions added `key::<blob>` inline form, with a deprecated backward-compat raw `ssh-`-prefixed form | Documented in current `man git-config` (verified on this host's installed git this session, quoted verbatim in CONTEXT.md D-01) | DevFlow's `check_ssh_signing_viability` predates or never accounted for the inline form — this phase closes that gap |

**Deprecated/outdated:**
- The raw `ssh-`-prefixed inline form is itself deprecated by git in favor
  of `key::` — DevFlow must keep *accepting* it (D-01 rule 2 is explicit
  about this), it's git's own operators being steered away from it, not
  DevFlow's job to reject it.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Git's `key::`/raw-`ssh-` precedence and the "private key must be available via ssh-agent" contract, as quoted in CONTEXT.md D-01/D-04, remain accurate for whatever git version runs in CI and on operator machines | Pattern 1, D-01/D-04 | If a git version changes this contract, the classification (and this whole phase) would need re-discussion, not a patch — CONTEXT.md's own "Specific Ideas" section already flags this explicitly, so it is not a new risk this research introduces |

No other claims in this research are tagged `[ASSUMED]`. The two
load-bearing technical facts (git's prefix precedence; `ssh-keygen -lf -`
stdin support and the `Stdio::piped()`/`.take()` mechanics) were both
independently re-verified in this research session against the live
toolchain on this host, not carried over from training data alone — hence
`[VERIFIED]` throughout rather than `[CITED]`/`[ASSUMED]`.

## Open Questions

1. **Shape of the refactor** (explicitly Claude's Discretion per
   CONTEXT.md): sibling function `inline_key_fingerprint`, a small source
   enum passed into a unified fingerprint function, or inlining at the
   call site.
   - What we know: D-05/D-09 constrain the *mechanism* (stdin, never
     argv); D-07 constrains what must stay shared (the `ssh-add -l` match
     and everything downstream).
   - What's unclear: whether the planner prefers a two-function shape
     (mirrors `public_key_fingerprint` most directly, smallest diff) or a
     single function taking an enum (`KeySource::Path(&Path) |
     KeySource::Inline(&str)`) that unifies error posture in one place.
   - Recommendation: the two-function sibling shape is the smallest diff
     and requires no signature change to `public_key_fingerprint` or any
     of its existing callers/tests — recommend that shape unless the
     planner has a reason to unify.

2. **Empty-remainder handling for `user.signingkey = "key::"` exactly**
   (Claude's Discretion, D-06 recommends `Unknown`).
   - What we know: verified this session — an empty string piped to
     `ssh-keygen -lf -` exits non-zero, so `inline_key_fingerprint("")`
     already naturally returns `None`, which (per D-06) routes to
     `Unknown` with no special-casing needed.
   - What's unclear: whether the planner wants an explicit early-return
     for the empty-remainder case (clearer intent, one extra branch) vs.
     letting it fall through to `ssh-keygen` failing naturally (fewer
     lines, same observable behavior, already verified to work).
   - Recommendation: let it fall through — it already produces the
     recommended `Unknown` outcome with zero extra code, and D-06 asks for
     an explicit *decision*, not necessarily explicit *code*; document the
     reasoning in the doc comment instead.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|--------------|-----------|---------|----------|
| `ssh-keygen` | inline fingerprint acquisition (new), existing path fingerprint | ✓ | OpenSSH_10.4p1, OpenSSL 3.6.3 (verified live on this host) | Already fail-soft to `Unknown` if absent (D-06); existing integration test `release_check_signing_degrades_when_ssh_add_absent` (`release_check.rs:325`) proves the tool-absent path via a `git_only_path()` fixture and should be extended, not duplicated, for the inline branch |
| `ssh-add` | agent match (shared, unchanged) | ✓ (implied by `ssh-keygen`'s presence in the same OpenSSH install) | OpenSSH_10.4p1 | Same fail-soft `Unknown` arm, unchanged |
| `rustc` / edition 2024 | `.take()`/`Stdio` mechanics | ✓ | 1.97.1 (8bab26f4f 2026-07-14) | N/A — pinned via `rust-toolchain.toml` per CONVENTIONS.md |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** none — both external tools this
phase touches already have proven fail-soft `Unknown` degradation paths in
the existing code and tests.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` (Rust built-in harness — no external framework, per `.planning/codebase/TESTING.md`) |
| Config file | none — no `pytest.ini`/`jest.config`-equivalent; behavior governed by `Cargo.toml` + `rust-toolchain.toml` |
| Quick run command | `cargo test -p devflow-core check_ssh_signing_viability` (or the specific new test names once written) |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map

This project does not track phase requirements by ID (no `REQUIREMENTS.md`
exists in `.planning/` — confirmed by search this session; the phase
description itself says "Requirements: TBD"). The acceptance surface is
instead the 12 locked decisions in `24-CONTEXT.md`. Mapping the
test-relevant ones:

| Decision | Behavior | Test Type | Automated Command | File Exists? |
|----------|----------|-----------|-------------------|-------------|
| D-01/D-02/D-03 | `key::`/raw-`ssh-`/path classification precedence, no `ecdsa-`/`sk-` allowlist | unit | `cargo test -p devflow-core -- git::tests` (new test in `mod tests`) | ❌ new test needed |
| D-04/D-05/D-09 | Inline key gets a real Viable/NotViable verdict via `ssh-keygen -lf -` on stdin | unit | same module | ❌ new test needed (`inline_key_fingerprint` unit coverage) |
| D-06 | `ssh-keygen` absence/failure/unparseable output → `Unknown`, never `NotViable` | unit | same module | ❌ new test needed; existing sibling `release_check_signing_degrades_when_ssh_add_absent` covers the *path*-branch tool-absence case only |
| D-08 | No reason string embeds the configured key value (whole or part) | integration | `cargo test -p devflow --test release_check` | ❌ extend `release_check_signing_output_leaks_no_key_material_or_path`, per D-11 |
| D-10 | Inline key never produces the `"key file does not exist"` `NotViable`, agent-independent | unit | `cargo test -p devflow-core -- git::tests` | ❌ new test needed |
| D-12 | Path branch regression — unchanged behavior for a real path not starting `ssh-`/`key::` | unit or integration | either module | ❌ new test needed (unit preferred — no live agent required to prove the classification arm itself is unchanged; can layer on top of the existing integration fixture for the full-stack proof) |

### Sampling Rate
- **Per task commit:** `cargo test -p devflow-core git` (fast, isolates the
  changed module) plus `cargo clippy -p devflow-core -- -D warnings`
- **Per wave merge:** `cargo test --workspace` (this is a single-wave,
  single-file-cluster phase per the Architectural Responsibility Map, so
  wave and phase gates likely coincide)
- **Phase gate:** `cargo test --workspace` + `cargo clippy --workspace
  --all-targets -- -D warnings` + `cargo fmt --check` (the exact three CI
  jobs per `.planning/codebase/TESTING.md`) green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] No new test *file* needed — all new unit tests land in
      `crates/devflow-core/src/git.rs`'s existing `mod tests` block (D-11);
      no framework install, no new fixture module.
- [ ] Optional: one extended assertion in
      `crates/devflow-cli/tests/release_check.rs` (D-11) — not a new file,
      an addition to the existing `release_check_signing_output_leaks_no_key_material_or_path`
      test or a small new `#[test]` fn immediately after it, reusing
      `init_repo`/`git`/`run_release` helpers already defined in that file.

*(No gaps beyond the tests themselves — the harness, fixtures, and helper
functions (`HOME_ENV_MUTEX`, `init_repo`, `run_release`, `git_only_path`)
all already exist and are directly reusable.)*

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|----------------|---------|-------------------|
| V2 Authentication | no | Not an authentication surface |
| V3 Session Management | no | N/A |
| V4 Access Control | no | N/A |
| V5 Input Validation | yes | Prefix-based classification of `user.signingkey` is itself the input-validation logic under change; must not widen acceptance beyond git's own documented contract (D-01/D-03) |
| V6 Cryptography | yes | No cryptographic material is generated, stored, or transmitted by this phase — only a *public* key blob is read and piped to `ssh-keygen -lf` for fingerprinting (never a private key, never persisted). Continuation of the existing T-20-04 discipline (D-08), not a new cryptographic surface. |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|----------------------|
| Sensitive value (key blob) leaking into a world-readable location | Information Disclosure | Never pass the blob as an argv element (readable via `/proc/<pid>/cmdline`); use stdin exclusively (D-09) — implemented via `Stdio::piped()` + `write_all()`, verified working this session |
| Sensitive value leaking into operator-visible output | Information Disclosure | No `SigningViability::NotViable`/`Unknown` reason string may embed the configured value, whole or partial (D-08) — carry forward the existing `public_key_fingerprint` pattern of returning only the derived `SHA256:` hash, never the input |
| False-negative security check giving a misleading "safe" signal | Tampering (of trust signal) | This is the inverse of this phase's actual bug (a false *positive* fail) but D-06's fail-soft-to-`Unknown` boundary is exactly where a false `Viable` could sneak in if `inline_key_fingerprint` ever returned a fingerprint without confirming `ssh-add -l` actually lists it — the existing shared match block (`git.rs:771-786`, Pattern 1 above) already enforces this correctly and this phase must not bypass it |

## Sources

### Primary (HIGH confidence — verified live this session)
- Live source read: `crates/devflow-core/src/git.rs:1-20, 660-834, 1500-1568` (current `check_ssh_signing_viability`, `public_key_fingerprint`, `SigningViability`, existing test patterns)
- Live source read: `crates/devflow-cli/src/commands.rs:1870-2061` (`release_check`, `check_signing`, `Check` struct at `commands.rs:1685-1690`)
- Live source read: `crates/devflow-cli/tests/release_check.rs:1-60, 257-359` (`run_release`, `init_repo`, the two named tests D-11 references)
- Live source read: `crates/devflow-core/src/test_support.rs` (hermetic git fixture helpers, `REPO_LOCAL_GIT_VARS`)
- Live shell verification: `ssh-keygen -lf -` accepting both a `key::`-stripped blob and a raw `ssh-`-prefixed blob on stdin, exit 0, correct `SHA256:...` output (`OpenSSH_10.4p1`, this host)
- Live compile+run verification: three `rustc --edition 2024` throwaway programs proving (a) the borrow-only `as_mut()` variant does not hang on this host but is not a documented guarantee, (b) the `.take()`+`drop()` idiom is correct and matches Rust's own documented `Stdio` example, (c) the empty-blob and garbage-input edge cases both correctly return `None`
- `.planning/codebase/TESTING.md`, `.planning/codebase/CONVENTIONS.md` — house test/style conventions
- `24-CONTEXT.md` — all 12 locked decisions, quoted `man git-config` text, quoted `man ssh-keygen` reference

### Secondary (MEDIUM confidence)
- WebSearch confirming `doc.rust-lang.org/std/process/struct.Stdio.html`'s canonical `Command::new("rev")` example uses the same `.stdin(Stdio::piped())` + `.stdin.take()` + `write_all()` + `wait_with_output()` shape independently arrived at and verified locally

### Tertiary (LOW confidence)
- none

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — std-only, no dependency to verify, toolchain version confirmed live
- Architecture: HIGH — every referenced line number and function signature read directly from live source this session, not from CONTEXT.md's citations alone
- Pitfalls: HIGH — the stdin-piping mechanics (the phase's one genuinely new technique) were compiled and executed against a real key on this exact host/toolchain, including the empty and garbage-input edge cases

**Research date:** 2026-07-27
**Valid until:** 30 days (stable `std` API, pinned toolchain, locked decisions) — re-verify only if the installed git or OpenSSH version changes materially on CI/operator machines (Assumption A1)
