#!/usr/bin/env bash
# Refuse a plan whose `<automated>` verify command relies on bash-only syntax
# that expands to NOTHING under the executor's shell.
#
# GSD executor subagents run their Bash tool under **zsh**, not bash. zsh has no
# `PIPESTATUS` array (it spells it `$pipestatus`, and indexes from 1), so
# `${PIPESTATUS[0]}` expands to the EMPTY STRING there. A verify block written as
#
#     cargo test ... | tee /tmp/x.log; echo "cargo_exit=${PIPESTATUS[0]}"
#
# therefore prints `cargo_exit=` and every downstream assertion about that exit
# code passes vacuously — a green check over a command whose result was never
# read. Phase 46 hit this in all three of its plans; each executor rediscovered
# it independently and worked around it by re-running the blocks through
# `bash -c`.
#
# The fix at authoring time is to write the command as `bash -c '...'`, which is
# what this refuses to let you skip.
#
# ---------------------------------------------------------------------------
# THE ALLOW RULE IS CONTAINMENT, NOT ORDERING.
#
# A bashism is permitted only when it lies inside a single-quoted region that
# some `bash -c '` opened. Single quotes do not nest in POSIX shell, so that
# region is unambiguous: it runs from the quote immediately following `bash -c`
# to the very next single quote.
#
# Ordering — "a `bash -c` appears somewhere before the bashism" — is not enough.
# The four cases containment has to decide, and how it decides them:
#
#   1. ACCEPT  bash -c 'cargo test | tail -3; echo "x=${PIPESTATUS[0]}"'
#              The bashism is inside the region, so it genuinely runs under bash.
#              This is the legitimate form and rejecting it would make plan
#              authoring impossible.
#   2. REJECT  echo "${PIPESTATUS[0]}" # bash -c
#              A trailing comment opens no region at all. A substring filter on
#              `bash -c` accepted this, and it was committed through the live
#              hook to prove the point.
#   3. REJECT  bash -c ':'; echo "${PIPESTATUS[0]}"
#              A real `bash -c` precedes the bashism, but its region closed at
#              the second quote. The bashism runs in the OUTER shell. Ordering
#              accepts this; containment does not.
#   4. REJECT  bash -c "echo ${PIPESTATUS[0]}"
#              Double quotes open no single-quoted region, so the outer shell
#              expands the bashism before bash is ever started. Ordering accepts
#              this too.
#
# Where the rule cannot decide — nested quoting gymnastics, an unbalanced quote
# after `bash -c` — it fails CLOSED and says so. A conservative false positive
# costs one edit; a false negative is the whole finding.
#
# An opening tag with no closing tag is reported as a malformed block rather
# than ignored: leaving it unhandled would be a bypass by construction.
#
# The regression cases live in crates/devflow-cli/tests/plan_bashism_scanner.rs,
# which EXECUTES this script rather than reading it.
# ---------------------------------------------------------------------------
#
# Usage:
#   scripts/lint-plan-bashisms.sh --staged     scan staged *PLAN.md files
#   scripts/lint-plan-bashisms.sh <path>...    scan the named files
#   scripts/lint-plan-bashisms.sh              scan nothing; prints a 0 count
#
# Exit: 0 clean, 1 violation(s) found, 2 usage error or unreadable input.

set -euo pipefail

SELF="lint-plan-bashisms"

usage() {
    sed -n '/^# Usage:/,/^# Exit:/p' "$0" | sed 's/^# \{0,1\}//'
}

staged=0
case "${1-}" in
    -h | --help)
        usage
        exit 0
        ;;
    --staged)
        staged=1
        shift
        ;;
esac

files=()

if [ "$staged" -eq 1 ]; then
    if [ "$#" -gt 0 ]; then
        echo "$SELF: --staged takes no further arguments" >&2
        exit 2
    fi
    # `-z` with `read -r -d ''` is load-bearing: plain
    # `git diff --cached --name-only` QUOTES a path containing non-ASCII or a
    # tab, and a `[ -f "$path" ]` on the quoted form then fails and skips the
    # file SILENTLY — the same false-green class this scanner exists to close.
    #
    # `--diff-filter=ACM` is equally load-bearing in the other direction: a
    # DELETED plan has no working-tree content to scan, and listing it would
    # reintroduce the silent skip from the far end.
    while IFS= read -r -d '' path; do
        case "${path##*/}" in
            *PLAN.md) files+=("$path") ;;
        esac
    done < <(git diff --cached --name-only -z --diff-filter=ACM)
else
    while [ "$#" -gt 0 ]; do
        files+=("$1")
        shift
    done
fi

count=${#files[@]}

if [ "$count" -eq 0 ]; then
    # The one legitimate empty case. Print the count so the emptiness is VISIBLE
    # rather than inferred from a bare exit 0.
    echo "$SELF: scanned 0 file(s)"
    exit 0
fi

# A named path that is missing or unreadable is an ERROR, not a skip.
for path in "${files[@]}"; do
    if [ ! -f "$path" ] || [ ! -r "$path" ]; then
        echo "$SELF: cannot read '$path' (missing or unreadable)" >&2
        echo "  A named path that cannot be inspected is an error, not a skip:" >&2
        echo "  a check that reports success having inspected nothing is exactly" >&2
        echo "  the failure this scanner exists to prevent." >&2
        exit 2
    fi
done

hits=""
status=0

for path in "${files[@]}"; do
    out=""
    if ! out="$(awk -v fname="$path" '
function line_of(off,   pre, n) {
    pre = substr(buf, 1, off - 1)
    n = gsub(/\n/, "&", pre)
    return n + 1
}

# Build the set of single-quoted regions opened by a `bash -c` in this block.
function build_regions(body,   p, i, j, k, c) {
    nregions = 0
    undecidable = 0
    # The classic embedded-quote idiom cannot be decided by a flat region scan.
    # Fail closed rather than guess.
    if (index(body, SQ DQ SQ DQ) > 0) {
        undecidable = 1
        return
    }
    p = 1
    while (1) {
        i = index(substr(body, p), "bash -c")
        if (i == 0) return
        j = p + i - 1 + 7
        c = substr(body, j, 1)
        while (c == " " || c == "\t") {
            j++
            c = substr(body, j, 1)
        }
        if (c == SQ) {
            k = index(substr(body, j + 1), SQ)
            if (k == 0) {
                # Unbalanced quote after `bash -c`: the region has no end, so
                # containment cannot be decided.
                undecidable = 1
                return
            }
            k = j + k
            nregions++
            rstart[nregions] = j + 1
            rend[nregions] = k - 1
            p = k + 1
        } else {
            p = j
        }
    }
}

function contained(a, b,   n) {
    for (n = 1; n <= nregions; n++) {
        if (a >= rstart[n] && b <= rend[n]) return 1
    }
    return 0
}

function check_block(body, base,   rest, off, o, txt) {
    build_regions(body)
    off = 0
    rest = body
    while (match(rest, /\$\{[A-Za-z_][A-Za-z0-9_]*\[[^]]*\]\}|\$\{[A-Za-z_][A-Za-z0-9_]*(,,|\^\^)\}|declare -A/)) {
        o = off + RSTART
        txt = substr(rest, RSTART, RLENGTH)
        if (undecidable) {
            printf "%s:%d: %s  [quoting cannot be decided - restructure the command]\n", fname, line_of(base + o - 1), txt
            violations++
        } else if (!contained(o, o + RLENGTH - 1)) {
            printf "%s:%d: %s\n", fname, line_of(base + o - 1), txt
            violations++
        }
        off = off + RSTART + RLENGTH - 1
        rest = substr(rest, RSTART + RLENGTH)
    }
}

BEGIN {
    SQ = sprintf("%c", 39)
    DQ = sprintf("%c", 34)
    OPEN = "<" "automated" ">"
    CLOSE = "<" "/automated" ">"
    violations = 0
}

{ buf = buf $0 "\n" }

END {
    pos = 1
    while (1) {
        i = index(substr(buf, pos), OPEN)
        if (i == 0) break
        open_at = pos + i - 1
        bstart = open_at + length(OPEN)
        j = index(substr(buf, bstart), CLOSE)
        if (j == 0) {
            printf "%s:%d: unterminated %s block - no %s found after it\n", fname, line_of(open_at), OPEN, CLOSE
            violations++
            break
        }
        check_block(substr(buf, bstart, j - 1), bstart)
        pos = bstart + j - 1 + length(CLOSE)
    }
    exit (violations > 0 ? 1 : 0)
}
' "$path")"; then
        status=1
    fi
    if [ -n "$out" ]; then
        hits="${hits}${out}"$'\n'
    fi
done

echo "$SELF: scanned $count file(s)"

if [ "$status" -ne 0 ]; then
    echo "$SELF: refusing a plan whose <automated> verify command uses bash-only" >&2
    echo "  syntax that expands to NOTHING under the executor's zsh shell:" >&2
    printf '%s' "$hits" | cut -c1-200 | sed 's/^/    /' >&2
    echo "  These assertions would pass without ever reading the value they claim" >&2
    echo "  to check. Wrap the command so it runs under bash, e.g.:" >&2
    echo "      <automated>bash -c 'cargo test ... ; echo \"exit=\${PIPESTATUS[0]}\"'</automated>" >&2
    echo "  The bashism must sit INSIDE the single quotes that follow bash -c;" >&2
    echo "  a trailing '# bash -c' comment, a closed 'bash -c \":\";' prefix, or a" >&2
    echo "  double-quoted bash -c \"...\" all still expand in the outer shell." >&2
    echo "" >&2
    echo "  An 'unterminated ... block' line above means an opening tag has no" >&2
    echo "  closing tag after it. That is refused deliberately: a parser matching" >&2
    echo "  only complete blocks would see nothing to check, so an unclosed tag is" >&2
    echo "  a bypass by construction. Close the block. If you meant to name the tag" >&2
    echo "  in prose rather than open a block, write it indirectly (e.g. split the" >&2
    echo "  literal) so the document does not contain a half-open tag." >&2
    exit 1
fi

exit 0
