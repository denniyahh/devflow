# Phase 24: `release --check` Signing-Key Inline Classification - Pattern Map

**Mapped:** 2026-07-27
**Files analyzed:** 2 (1 production + tests colocated, 1 optional integration test extension)
**Analogs found:** 2 / 2 — both analogs are in the SAME files being modified (this is a small, self-contained phase; the "closest analog" to the new code is the sibling function immediately above/below it in the same module)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|--------------------|------|-----------|-----------------|---------------|
| `crates/devflow-core/src/git.rs` — new `inline_key_fingerprint` fn | utility (subprocess wrapper) | request-response (spawn, write stdin, read stdout) | `public_key_fingerprint` (same file, `git.rs:722-736`) | exact — same signature shape, same parse, same fail-soft posture; only the input channel differs (stdin vs. path arg) |
| `crates/devflow-core/src/git.rs` — modified `check_ssh_signing_viability` classification arm | service/pure-function (classifier) | transform | its own current body (`git.rs:742-791`), specifically the `:748-753` arm being replaced | exact — surgical edit in place, not a new function |
| `crates/devflow-core/src/git.rs` `mod tests` — new unit tests (D-10, D-12) | test | CRUD/state-setup + assert | `check_signing_viability_degrades_when_gpg_format_unset_and_no_signingkey` (`git.rs:1537-1567`) | exact — same `HOME_ENV_MUTEX` + repo fixture idiom |
| `crates/devflow-cli/tests/release_check.rs` — optional extended assertion (D-11) | test (integration) | request-response (spawn real binary, assert stdout) | `release_check_signing_output_leaks_no_key_material_or_path` (`release_check.rs:257-303`) | exact — same fixture helpers (`init_repo`, `commit`, `git`, `run_release`), extend in place or add adjacent `#[test]` |

## Pattern Assignments

### `crates/devflow-core/src/git.rs` — new `inline_key_fingerprint(key_blob: &str) -> Option<String>`

**Analog:** `public_key_fingerprint` (`git.rs:719-736`)

**Full analog to mirror:**
```rust
/// `ssh-keygen -lf <pub_key_path>`'s fingerprint (`SHA256:...`) — reads only
/// the PUBLIC key file, never a private key, and returns only the hash
/// token, never a filesystem path.
fn public_key_fingerprint(pub_key_path: &Path) -> Option<String> {
    let path_str = pub_key_path.to_str()?;
    let output = Command::new("ssh-keygen")
        .args(["-lf", path_str])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    // Format: "<bits> SHA256:<hash> <comment> (<type>)"
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}
```

**What to copy exactly:** the `Option<String>` return type, the `.ok()?` fail-soft chain, the `if !output.status.success() { return None; }` guard, and the identical `.split_whitespace().nth(1).map(str::to_string)` parse (D-05 — same output shape whether the input came via path or stdin, do not re-derive a parser).

**What differs (the one new mechanism, per RESEARCH.md Pattern 2, verified live on this host):**
```rust
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

    // MUST .take() then drop() before wait_with_output() — a borrow via
    // .as_mut() "works" on this host but is not a documented guarantee
    // (RESEARCH.md Pitfall 1).
    let mut stdin = child.stdin.take()?;
    stdin.write_all(key_blob.as_bytes()).ok()?;
    drop(stdin);

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}
```
New imports needed at file top (`git.rs` currently imports `std::process::Command` only, per RESEARCH.md — verify current import block before adding): `std::io::Write`, `std::process::Stdio`.

**Security constraint (D-09, non-negotiable):** the key blob goes to stdin only, never `.arg(key_blob)` — argv is world-readable via `/proc/<pid>/cmdline`.

---

### `crates/devflow-core/src/git.rs` — modified `check_ssh_signing_viability`

**Analog:** its own current body — this is an in-place edit, not new code written against a foreign pattern.

**Current full function** (`git.rs:738-791`, to be surgically modified):
```rust
/// `gpg.format == "ssh"` branch (Pattern 4): `user.signingkey` must be set
/// and the key file must exist, then `ssh-add -l`'s exit code determines
/// viability. On a match, only the PUBLIC key's fingerprint is reported —
/// never the configured key's filesystem path.
fn check_ssh_signing_viability(project_root: &Path) -> SigningViability {
    let Some(signingkey) = git_config(project_root, "user.signingkey") else {
        return SigningViability::NotViable {
            reason: "gpg.format=ssh but user.signingkey is not set".into(),
        };
    };
    let key_path = Path::new(&signingkey);
    if !key_path.exists() {
        return SigningViability::NotViable {
            reason: "user.signingkey is set but the key file does not exist".into(),
        };
    }

    let output = match Command::new("ssh-add").arg("-l").output() {
        Ok(out) => out,
        Err(_) => {
            return SigningViability::Unknown {
                reason: "cannot verify signing viability — ssh-add not found".into(),
            };
        }
    };
    let exit_code = output.status.code().unwrap_or(-1);
    match classify_ssh_add_status(exit_code) {
        SigningStatus::NoAgent => SigningViability::NotViable {
            reason: "no ssh-agent reachable (SSH_AUTH_SOCK unset or dead)".into(),
        },
        SigningStatus::AgentEmpty => SigningViability::NotViable {
            reason: "ssh-agent reachable but has no identities loaded".into(),
        },
        SigningStatus::KeysListed => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match public_key_fingerprint(key_path) {
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
        SigningStatus::Unknown(code) => SigningViability::Unknown {
            reason: format!("ssh-add -l exited with an unexpected code {code}"),
        },
    }
}
```

**Only the `:748-753` arm is replaced.** Everything from the `ssh-add` spawn (`:755`) through the end (`:791`) is unchanged EXCEPT `public_key_fingerprint(key_path)` at `:773` must become a reference to a `fingerprint: Option<String>` local computed once before the `ssh-add -l` spawn (per RESEARCH.md's recommended shape — "compute `fingerprint` once, leave the match block referencing a local variable instead of re-calling `public_key_fingerprint(key_path)` inline"), so D-07's shared match block is truly untouched in substance.

**Recommended replacement shape** (RESEARCH.md Pattern 1, illustrative — exact enum/inlining is Claude's Discretion per CONTEXT.md D-01 note):
```rust
// Mirrors `man git-config`'s user.signingKey precedence (D-01):
// key:: form, then deprecated raw ssh- form, else a path. Never stat a
// path for a prefix-matched value (D-02).
let trimmed = signingkey.trim();
let inline_blob: Option<&str> = if let Some(rest) = trimmed.strip_prefix("key::") {
    Some(rest)
} else if trimmed.starts_with("ssh-") {
    Some(trimmed)
} else {
    None
};

let fingerprint = match inline_blob {
    Some(blob) => inline_key_fingerprint(blob),
    None => {
        let key_path = Path::new(&signingkey);
        if !key_path.exists() {
            return SigningViability::NotViable {
                reason: "user.signingkey is set but the key file does not exist".into(),
            };
        }
        public_key_fingerprint(key_path)
    }
};
```
Then the existing `ssh-add -l` spawn + `classify_ssh_add_status` match block (`:755-790`) references `fingerprint` instead of re-deriving it inline at `:773`.

**Do NOT touch:** `classify_ssh_add_status`/`SigningStatus` (`:662-685`, D-07 — shared unchanged), `git_config` (`:704-717`, unchanged), `check_gpg_signing_viability` (out of scope, starts `:793`).

---

### `crates/devflow-core/src/git.rs` `mod tests` — new unit tests (D-10, D-12)

**Analog:** `check_signing_viability_degrades_when_gpg_format_unset_and_no_signingkey` (`git.rs:1537-1567`)

**Full idiom to reuse verbatim for HOME isolation:**
```rust
static HOME_ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn check_signing_viability_degrades_when_gpg_format_unset_and_no_signingkey() {
    let _lock = HOME_ENV_MUTEX.lock().unwrap();
    let repo = init_repo();
    let root = repo.path();
    let fake_home = tempfile::tempdir().unwrap();
    let original_home = std::env::var_os("HOME");
    // SAFETY: serialized under HOME_ENV_MUTEX; restored below before
    // the guard drops.
    unsafe { std::env::set_var("HOME", fake_home.path()) };

    let result = check_signing_viability(root);

    // SAFETY: still serialized under HOME_ENV_MUTEX.
    match original_home {
        Some(home) => unsafe { std::env::set_var("HOME", home) },
        None => unsafe { std::env::remove_var("HOME") },
    }

    match result {
        SigningViability::Unknown { reason } => {
            assert!(reason.contains("user.signingkey"), "unexpected reason: {reason}");
        }
        other => panic!("expected Unknown (fail-soft), got: {other:?}"),
    }
}
```

**Applies to the new D-10 test:** set `user.signingkey` to `key::ssh-ed25519 AAAA…` (and separately a raw `ssh-ed25519 AAAA…`) via repo-local `git config` (reuse the `git(root, &[...])` test helper already in this module), call `check_signing_viability` (or `check_ssh_signing_viability` directly if visible to `mod tests` — verify), and assert the result is **never**
`SigningViability::NotViable { reason }` where `reason == "user.signingkey is set but the key file does not exist"`. Do not assert a specific positive outcome (agent state varies).

Also reuse the existing sibling for exit-code classification as a template for style:
```rust
#[test]
fn classify_ssh_add_status_maps_all_three_documented_exit_codes() {
    assert_eq!(classify_ssh_add_status(2), SigningStatus::NoAgent);
    assert_eq!(classify_ssh_add_status(1), SigningStatus::AgentEmpty);
    assert_eq!(classify_ssh_add_status(0), SigningStatus::KeysListed);
    assert_eq!(classify_ssh_add_status(7), SigningStatus::Unknown(7));
}
```
(`git.rs:1521-1527`) — same flat `assert_eq!` style is appropriate for a D-12 path-branch regression test that doesn't need HOME isolation (repo-local `user.signingkey` set to a real generated key path is sufficient, no global-config leak risk if it's a real temp path not starting with `ssh-`/`key::`).

---

### `crates/devflow-cli/tests/release_check.rs` — optional extended assertion (D-11)

**Analog:** `release_check_signing_output_leaks_no_key_material_or_path` (`release_check.rs:248-303`)

**Full analog:**
```rust
#[test]
fn release_check_signing_output_leaks_no_key_material_or_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    commit(root, "base.txt");
    git(root, &["config", "gpg.format", "ssh"]);

    let key_path = root.join("release-signing-key");
    let keygen = Command::new("ssh-keygen")
        .args(["-t", "ed25519", "-f", key_path.to_str().unwrap(), "-N", "", "-q"])
        .output()
        .expect("spawn ssh-keygen");
    assert!(
        keygen.status.success(),
        "ssh-keygen fixture setup failed: {}",
        String::from_utf8_lossy(&keygen.stderr)
    );
    let pub_key_path = root.join("release-signing-key.pub");
    git(root, &["config", "user.signingkey", pub_key_path.to_str().unwrap()]);

    let output = run_release(root, &["--check"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(!stdout.contains("PRIVATE KEY"), "...");
    assert!(!stdout.contains(root.to_str().unwrap()), "...");
    assert!(!stdout.contains("panicked"), "...");
}
```

**Fixture helpers reused (defined earlier in the same file, `release_check.rs:1-60`):**
```rust
fn devflow_bin() -> &'static str { env!("CARGO_BIN_EXE_devflow") }

/// Runs `devflow release <args> <project>` with an ISOLATED `HOME` and no
/// inherited SSH_AUTH_SOCK/SSH_AGENT_PID.
fn run_release(project: &Path, args: &[&str]) -> Output {
    let isolated_home = tempfile::tempdir().unwrap();
    Command::new(devflow_bin())
        .arg("release")
        .args(args)
        .arg(project)
        .env("HOME", isolated_home.path())
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("SSH_AGENT_PID")
        .output()
        .expect("spawn devflow release")
}

fn git(root: &Path, args: &[&str]) {
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
}

fn init_repo(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "tag.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
}
```

**For D-11's extension:** instead of `git config user.signingkey <path>`, set
`git config user.signingkey "key::$(cat pub_key_path)"` (or a raw `ssh-ed25519 …`
value read from the generated `.pub` file) and add the assertion that the
blob itself never appears in `stdout` (`!stdout.contains(&pub_key_blob)`),
alongside the existing `PRIVATE KEY` / path / `panicked` assertions. Either
extend this test in place or add a new `#[test]` immediately after it,
reusing `init_repo`/`commit`/`git`/`run_release` unchanged.

**Related fail-soft fixture** (for reference if extending the tool-absent case to inline, `release_check.rs:305-320`):
```rust
/// A `PATH` containing ONLY a symlink to the real `git` binary — guarantees
/// `ssh-add`/`ssh-keygen` are genuinely absent regardless of the host.
fn git_only_path() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let which = Command::new("which").arg("git").output().expect("locate git via `which`");
    assert!(which.status.success(), "`which git` failed");
    let real_git = String::from_utf8_lossy(&which.stdout).trim().to_string();
    std::os::unix::fs::symlink(real_git, dir.path().join("git"))
        .expect("symlink git into the minimal PATH fixture");
    dir
}
```

---

## Shared Patterns

### Fail-soft, never panic (D-06)
**Source:** `inline_key_fingerprint` mirrors `public_key_fingerprint`'s `Option<String>` + `.ok()?` chain end-to-end. Every failure mode (tool absent, non-zero exit, unparseable output, empty blob) returns `None`, which the caller must route to `SigningViability::Unknown`, never `NotViable` — see the existing `None => SigningViability::Unknown { ... }` arm at `git.rs:780-784`, unchanged and now reached from either branch.

### Prefix-first classification, never fall back to path-existence for a matched value (D-01/D-02)
**Source:** new code only — no existing analog in this codebase (this is the one genuinely new classification idiom in the phase). Precedence: `key::` strip → inline; else `ssh-` prefix → inline (raw compat form); else → path (byte-for-byte existing `:748-753` behavior).

### No key material/path in output (T-20-04, D-08)
**Source:** `SigningViability` enum doc comment (`git.rs:687-691`) states the contract explicitly: "never private key material or a full filesystem path." `inline_key_fingerprint` must return only the derived `SHA256:` hash (same as `public_key_fingerprint`), never the input blob — apply to any new reason string.

### Stdin over argv for sensitive data (D-09)
**Source:** new mechanism, no prior analog in `git.rs` (every other subprocess call in this module uses one-shot `.output()` with no stdin). Verified live pattern is Rust's own canonical `Stdio::piped()` + `.stdin.take()` + `write_all()` + `drop()` + `wait_with_output()` shape (matches `doc.rust-lang.org/std/process/struct.Stdio.html`'s `Command::new("rev")` example).

## No Analog Found

None. All new/modified surfaces have a direct in-file or in-module analog (this is a Size-S, single-file-cluster phase per RESEARCH.md's Architectural Responsibility Map).

## Metadata

**Analog search scope:** `crates/devflow-core/src/git.rs` (full function + test module), `crates/devflow-cli/tests/release_check.rs` (fixture helpers + two named tests)
**Files scanned:** 2 (both already fully identified by CONTEXT.md's canonical_refs and RESEARCH.md's live source reads — no additional Glob/Grep search was needed given the phase's tight, pre-scoped file set)
**Pattern extraction date:** 2026-07-27
</content>
