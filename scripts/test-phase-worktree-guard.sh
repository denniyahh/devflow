#!/usr/bin/env bash
# End-to-end exercise of scripts/lint-phase-worktree.sh in a scratch repository.
#
# This asserts BOTH directions. A guard observed only passing has not been shown
# to discriminate -- this repo has shipped two guards that could never fail
# (`rg -c | rg '^0$'`, and `${PIPESTATUS[0]}` under zsh), and both were caught
# only after the fact. Cases 2 and 5 are the negative controls: they must PASS,
# and if they ever start refusing, the guard has become a constant-fail rather
# than a discriminator.
set -euo pipefail

# Hermetic: the host checkout's Git environment may not leak into the fixture.
# Git owns the complete local-environment inventory; listing it here avoids
# silently missing a new carrier such as object storage.
mapfile -t git_local_env_vars < <(git rev-parse --local-env-vars)
unset "${git_local_env_vars[@]}"

GUARD="$(cd "$(dirname "$0")" && pwd)/lint-phase-worktree.sh"
[ -x "$GUARD" ] || { echo "missing guard: $GUARD" >&2; exit 1; }

pass=0; fail=0

# The guard is useless if git does not record it executable. `core.fileMode` is
# false in this repo, so an on-disk `chmod +x` is INVISIBLE to git and a new
# script commits as 100644 -- while still working locally, because the on-disk
# bit is real. A fresh clone then gets a non-executable guard, pre-commit takes
# its "missing or non-executable" branch, and EVERY commit is refused. That
# shipped once; this asserts against the index, not against the filesystem.
#
# This is the harness's only query against the real checkout, so it runs with
# the inherited git config: CI runs as root on a runner-owned checkout that git
# accepts only through a global safe.directory entry. A failed query is reported
# as a FAIL with git's own error above it, never as a silent exit.
for f in lint-phase-worktree.sh phase-worktree.sh test-phase-worktree-guard.sh; do
    if ! listing="$(git -C "$(dirname "$GUARD")/.." ls-files --stage -- "scripts/$f")"; then
        printf '  FAIL %-58s (git ls-files failed; its error is above)\n' "0. scripts/$f recorded executable"; fail=$((fail+1))
        continue
    fi
    mode="$(printf '%s\n' "$listing" | awk '{print $1}')"
    if [ "$mode" = "100755" ]; then
        printf '  ok   %-58s (mode %s)\n' "0. scripts/$f recorded executable" "$mode"; pass=$((pass+1))
    else
        printf '  FAIL %-58s (mode %s, want 100755)\n' "0. scripts/$f recorded executable" "${mode:-missing}"; fail=$((fail+1))
    fi
done

# Hermetic, continued: from here on only scratch fixtures are touched, and they
# may not inherit global or system config either (hooks, signing, excludes).
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
check() { # check <name> <expected: refuse|allow> <actual_exit>
    local name="$1" expected="$2" rc="$3"
    local actual; if [ "$rc" -eq 0 ]; then actual=allow; else actual=refuse; fi
    if [ "$actual" = "$expected" ]; then
        printf '  ok   %-58s (%s, exit %d)\n' "$name" "$actual" "$rc"; pass=$((pass+1))
    else
        printf '  FAIL %-58s expected %s, got %s (exit %d)\n' "$name" "$expected" "$actual" "$rc"; fail=$((fail+1))
    fi
}

init_fixture_repo() { # init_fixture_repo <dir>
    local dir="$1"
    mkdir -p "$dir" "$TMP/no-hooks"
    git -C "$dir" init -q -b workspace/tester .
    # Inherited hooks and inherited SSH commit signing both come from global git
    # config. Signing with no ssh-agent was observed to abort with exit 128; these
    # settings are local to the fixture repository, so the guard's environment is untouched.
    git -C "$dir" config core.hooksPath "$TMP/no-hooks"
    git -C "$dir" config commit.gpgsign false
    git -C "$dir" config user.email t@example.com; git -C "$dir" config user.name Tester
    mkdir -p "$dir/.planning" "$dir/crates/devflow-core/src" "$dir/.planning/phases/47-demo"
    printf -- '---\ncurrent_phase: 47\n---\n' > "$dir/.planning/STATE.md"
    echo "fn main() {}" > "$dir/crates/devflow-core/src/lib.rs"
    echo "spec" > "$dir/.planning/phases/47-demo/47-CONTEXT.md"
    git -C "$dir" add -A >/dev/null; git -C "$dir" commit -qm init
}

REPO="$TMP/repo"
init_fixture_repo "$REPO"
cd "$REPO"

echo "== before any worktree exists =="
echo "// edit" >> crates/devflow-core/src/lib.rs; git add crates >/dev/null
set +e; "$GUARD" --staged >/dev/null 2>&1; rc=$?; set -e
check "5. no phase worktree -> must not block (control)" allow "$rc"
git reset -q

git worktree add -q -b feature/phase-47 "$TMP/wt47" >/dev/null 2>&1

echo "== with the phase-47 worktree present =="
echo "// edit" >> crates/devflow-core/src/lib.rs; git add crates >/dev/null
set +e; "$GUARD" --staged >/dev/null 2>&1; rc=$?; set -e
check "1. source staged off-worktree -> must refuse" refuse "$rc"

set +e; DEVFLOW_ALLOW_OFF_WORKTREE=1 "$GUARD" --staged >/dev/null 2>&1; rc=$?; set -e
check "4. same, with DEVFLOW_ALLOW_OFF_WORKTREE=1 -> allow" allow "$rc"
git reset -q

git add ".planning/phases/47-demo/47-CONTEXT.md" >/dev/null 2>&1 || true
echo "more" >> ".planning/phases/47-demo/47-CONTEXT.md"; git add .planning >/dev/null
set +e; warn="$("$GUARD" --staged 2>&1)"; rc=$?; set -e
check "3. phase docs staged off-worktree -> warn, not block" allow "$rc"
case "$warn" in *WARNING*) printf '  ok   %-58s\n' "3b. warning text emitted"; pass=$((pass+1));;
                *) printf '  FAIL %-58s (no WARNING in output)\n' "3b. warning text emitted"; fail=$((fail+1));; esac
git reset -q

echo "== from inside the phase-47 worktree (negative control) =="
cd "$TMP/wt47"
echo "// edit" >> crates/devflow-core/src/lib.rs; git add crates >/dev/null
set +e; "$GUARD" --staged >/dev/null 2>&1; rc=$?; set -e
check "2. source staged INSIDE the worktree -> must allow (control)" allow "$rc"
git reset -q
cd "$REPO"

echo "== degenerate inputs =="
mv .planning/STATE.md .planning/STATE.md.bak
echo "// edit" >> crates/devflow-core/src/lib.rs; git add crates >/dev/null
set +e; "$GUARD" --staged >/dev/null 2>&1; rc=$?; set -e
check "6. no STATE.md -> no-op (control)" allow "$rc"
mv .planning/STATE.md.bak .planning/STATE.md; git reset -q

echo "== staged names git quotes, and a subdirectory under diff.relative =="
# `git diff --cached --name-only` quotes a name holding a non-ASCII byte (under
# the default core.quotePath) or a double quote, backslash or control character
# (under any config). A guard matching `^crates/` against that output never sees
# the quoted line. Each control proves its condition is really present in this
# fixture, so its case cannot pass for want of the thing it tests. Each case
# stages only its own file: the lib.rs edits left unstaged above would otherwise
# make the guard refuse for the wrong reason.
nonascii="crates/devflow-core/src/café.rs"
echo "// edit" > "$nonascii"; git add -- "$nonascii" >/dev/null
case "$(git diff --cached --name-only)" in '"'*) rc=1 ;; *) rc=0 ;; esac
check "8a. control: git quotes a non-ASCII staged name" refuse "$rc"
set +e; "$GUARD" --staged >/dev/null 2>&1; rc=$?; set -e
check "8b. non-ASCII source name staged off-worktree -> refuse" refuse "$rc"
git reset -q; rm -f -- "$nonascii"

dquote='crates/devflow-core/src/a"b.rs'
echo "// edit" > "$dquote"; git add -- "$dquote" >/dev/null
case "$(git diff --cached --name-only)" in '"'*) rc=1 ;; *) rc=0 ;; esac
check "8c. control: git quotes a staged name holding a double quote" refuse "$rc"
set +e; "$GUARD" --staged >/dev/null 2>&1; rc=$?; set -e
check "8d. double-quote source name staged off-worktree -> refuse" refuse "$rc"
git reset -q; rm -f -- "$dquote"

# Hooks run from the repository root, but the guard can be started by hand from
# anywhere, and diff.relative limits name-only output to the current directory.
# Injected per command: this harness nulls global config.
git add -- crates/devflow-core/src/lib.rs >/dev/null
relative="$(cd .planning && GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=diff.relative GIT_CONFIG_VALUE_0=true git diff --cached --name-only)"
if [ -z "$relative" ]; then rc=1; else rc=0; fi
check "8e. control: diff.relative hides root staging from a subdirectory" refuse "$rc"
set +e; (cd .planning && GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=diff.relative GIT_CONFIG_VALUE_0=true "$GUARD" --staged) >/dev/null 2>&1; rc=$?; set -e
check "8f. run from a subdirectory under diff.relative -> refuse" refuse "$rc"
git reset -q

echo "== fixture isolation from inherited git config =="
mkdir -p "$TMP/hostile/hooks" "$TMP/empty-hooks"
for hook in pre-commit post-checkout; do
    printf '#!/bin/sh\ntouch "%s"\nexit 1\n' "$TMP/hostile/fired" > "$TMP/hostile/hooks/$hook"
    chmod +x "$TMP/hostile/hooks/$hook"
done
printf '[core]\n\thooksPath = %s/hostile/hooks\n[commit]\n\tgpgsign = true\n[gpg]\n\tformat = ssh\n[user]\n\tsigningkey = %s/hostile/no-such-key.pub\n\tname = Hostile\n\temail = h@example.com\n' "$TMP" "$TMP" > "$TMP/hostile/gitconfig"

hostile_commit() { # hostile_commit <dir> <git -c args...>
    local dir="$1"
    shift
    (
        set -e
        export GIT_CONFIG_GLOBAL="$TMP/hostile/gitconfig" GIT_CONFIG_NOSYSTEM=1
        mkdir -p "$dir"
        git -C "$dir" init -q .
        echo hostile > "$dir/f"
        git -C "$dir" add f
        git -C "$dir" "$@" commit -qm hostile
    )
}

rm -f "$TMP/hostile/fired"
set +e; hostile_commit "$TMP/h7a" -c commit.gpgsign=false >/dev/null 2>&1; rc=$?; set -e
[ -f "$TMP/hostile/fired" ] || rc=0
check "7a. inherited hook armed: unisolated commit -> refuse (control)" refuse "$rc"

rm -f "$TMP/hostile/fired"
set +e; hostile_commit "$TMP/h7b" -c core.hooksPath="$TMP/empty-hooks" >/dev/null 2>&1; rc=$?; set -e
[ -f "$TMP/hostile/fired" ] && rc=0
check "7b. inherited signing armed: unisolated commit -> refuse (control)" refuse "$rc"

rm -f "$TMP/hostile/fired"
set +e; (set -e; export GIT_CONFIG_GLOBAL="$TMP/hostile/gitconfig" GIT_CONFIG_NOSYSTEM=1; init_fixture_repo "$TMP/iso") >/dev/null 2>&1; rc=$?; set -e
[ -f "$TMP/hostile/fired" ] && rc=1
check "7c. fixture bootstrap under hostile config -> allow" allow "$rc"

echo
echo "passed=$pass failed=$fail"
[ "$fail" -eq 0 ]
