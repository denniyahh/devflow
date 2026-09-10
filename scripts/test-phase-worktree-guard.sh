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

# Hermetic: the host checkout's git env must not leak into the fixture.
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_CONFIG GIT_PREFIX 2>/dev/null || true

GUARD="$(cd "$(dirname "$0")" && pwd)/lint-phase-worktree.sh"
[ -x "$GUARD" ] || { echo "missing guard: $GUARD" >&2; exit 1; }

pass=0; fail=0

# The guard is useless if git does not record it executable. `core.fileMode` is
# false in this repo, so an on-disk `chmod +x` is INVISIBLE to git and a new
# script commits as 100644 -- while still working locally, because the on-disk
# bit is real. A fresh clone then gets a non-executable guard, pre-commit takes
# its "missing or non-executable" branch, and EVERY commit is refused. That
# shipped once; this asserts against the index, not against the filesystem.
for f in lint-phase-worktree.sh phase-worktree.sh test-phase-worktree-guard.sh; do
    mode="$(git -C "$(dirname "$GUARD")/.." ls-files --stage -- "scripts/$f" 2>/dev/null | awk '{print $1}')"
    if [ "$mode" = "100755" ]; then
        printf '  ok   %-58s (mode %s)\n' "0. scripts/$f recorded executable" "$mode"; pass=$((pass+1))
    else
        printf '  FAIL %-58s (mode %s, want 100755)\n' "0. scripts/$f recorded executable" "${mode:-missing}"; fail=$((fail+1))
    fi
done

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

REPO="$TMP/repo"
mkdir -p "$REPO"; cd "$REPO"
git init -q -b workspace/tester .
git config user.email t@example.com; git config user.name Tester
mkdir -p .planning crates/devflow-core/src ".planning/phases/47-demo"
printf -- '---\ncurrent_phase: 47\n---\n' > .planning/STATE.md
echo "fn main() {}" > crates/devflow-core/src/lib.rs
echo "spec" > ".planning/phases/47-demo/47-CONTEXT.md"
git add -A >/dev/null; git commit -qm init

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

echo
echo "passed=$pass failed=$fail"
[ "$fail" -eq 0 ]
