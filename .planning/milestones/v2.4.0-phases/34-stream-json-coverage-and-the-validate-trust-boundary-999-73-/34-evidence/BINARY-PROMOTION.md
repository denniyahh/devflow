# How a locally rebuilt `devflow` becomes the binary the shell runs

Resolves `34-RESEARCH.md` § "Open Questions" item 1 — the one item RESEARCH declined to answer and
handed to planning as an investigation. Written so a later capture run does not re-derive it.

**Paths below are placeholder-scrubbed by construction** (plan 34-05 task 1, threat T-34-05-01a):
`<home>` stands for this machine's home directory and `<user>` for the OS username. The raw,
unscrubbed transcript is at `/tmp/34-05-t1.txt` (deliberately not committed); the placeholders here
correspond one-to-one to what that transcript shows. On this immutable-Fedora layout `<home>` has
two spellings that both resolve to the same directory — the `$HOME` form and the `/var/...` form
that `readlink -f` prints — and `<home>` stands for either.

Paths containing neither component (e.g. `/home/linuxbrew/.linuxbrew/bin/devflow`) are recorded
**verbatim**; the scrub is targeted at those two components, not at path-shaped text generally.

## Answer in one sentence

**Promotion is automatic, not manual:** the `devflow` the shell resolves is a symlink into this
repo's own `target/release/`, so `cargo build --release -p devflow` *is* the promotion step and
there is no copy or install command to run afterwards.

## Reproduction

```sh
command -v devflow                       # which one PATH selects
ls -la "$(command -v devflow)"           # is it a symlink, and to what
readlink -f "$(command -v devflow)"      # the ultimate target
type -a devflow                          # every one on PATH, in PATH order
sha256sum target/release/devflow "$(readlink -f "$(command -v devflow)")"
```

## Every `devflow` on this machine

`type -a devflow` returns four lines, but they are **two distinct binaries** — `<home>/.local/bin`
appears three times because it occurs at three PATH positions (twice literally, once as
`<home>/.local/share/../bin`, which is the same directory reached by a different spelling).

| # | Path | PATH position | Kind | Ultimate target | SHA-256 (first 8) | `--version` |
|---|---|---|---|---|---|---|
| 1 | `/home/linuxbrew/.linuxbrew/bin/devflow` | **1 — wins** | symlink | `<home>/Github/devflow/target/release/devflow` | `02262977` | `devflow 2.3.0` |
| 2 | `<home>/.local/bin/devflow` | 10, 12, 18 | static copy | itself | `1c2da2cd` | `devflow 1.8.0` |

`<home>/.cargo/bin/devflow` — **absent**. There is no `justfile`, `Justfile` or `Makefile` at the
repo root, so no build-file promotion target exists either.

## The shadowing hazard, stated explicitly

**Two `devflow` binaries exist on this machine and they are five minor versions apart.** The
1.8.0 static copy at `<home>/.local/bin/devflow` is a year-stale artifact of `scripts/install.sh`'s
build-from-source fallback (`install.sh:80-81`, `cp target/release/devflow "$HOME/.local/bin/devflow"`
— also documented at `README.md:208`), which copies rather than links and therefore does **not**
track subsequent rebuilds.

PATH order is what saves this machine: linuxbrew's `bin` sits at position 1 and the stale copy at
position 10, so the symlink wins. **That is the only thing keeping the stale copy out of the
capture run.** Any PATH reordering — a shell-profile change, a different login shell, a `sudo`
or container invocation with a sanitised PATH — silently promotes a 1.8.0 binary that predates
`STREAM_JSON_STAGES` entirely.

`34-RESEARCH.md` Pitfall 5 records that `enforce_build_staleness` will **not** catch this.

**One thing does discriminate this particular pair:** the version strings differ (2.3.0 vs 1.8.0),
so `devflow --version` would catch *this* shadowing. It does **not** generalise — see the next
section.

## `devflow --version` is NOT a freshness check

The workspace version is `2.3.0` and every build from this tree reports `2.3.0` regardless of when
it was built or what `STREAM_JSON_STAGES` contained when it was compiled. A binary built before
this phase and one built after a five-stage widening are indistinguishable by `--version`.

**The discriminator is the SHA-256.** The digest of `target/release/devflow` immediately before
plan 34-05's widening, built at commit `b96dfca`, was:

```
022629778d715f935550f50adfc6d53a80f37f0ddf65ff415874ba9956227932
```

A capture produced by a binary still hashing to `02262977…` does not contain the widening and is
not evidence for criterion 1.

## The procedure the capture run uses

1. Edit `STREAM_JSON_STAGES` in the working tree.
2. `cargo build --release -p devflow` — the package is `devflow`, **not** `devflow-cli`; the latter
   is the crate directory name and `-p devflow-cli` fails.
3. Confirm the build **consumed the edit**: `target/release/devflow`'s mtime is strictly newer than
   `crates/devflow-cli/src/pipeline_launch.rs`'s, and the build output says `Compiling devflow`
   rather than only `Fresh`.
4. Confirm the **shell runs that build**: `sha256sum` on `target/release/devflow` and on
   `readlink -f "$(command -v devflow)"` must be **equal**, and both must have moved off
   `02262977…`.
5. Only then capture.

Steps 3 and 4 are separate checks answering separate questions — "did the compiler see my edit" and
"does `PATH` select what the compiler produced" — and a run needs both. Neither substitutes for the
other: a fresh build the shell does not select is useless, and a selected binary the compiler never
rebuilt is worse, because it looks fine.

## What is NOT established here

- This records the resolution **on this machine, at this moment**. PATH composition is per-shell
  environment state; a run launched from a different shell, a `systemd` unit, or a container may
  resolve differently, and nothing here detects that.
- Nothing here verifies the *contents* of the widened constant in the compiled binary. A
  `strings`-based probe for it was tested and rejected as non-discriminating — see plan 34-05
  task 2 and the SUMMARY. The mtime + digest chain establishes byte-identity with a build made from
  this tree after the edit; it does not read the constant back out of the binary.
