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
# ---------------------------------------------------------------------------
# COMMAND RULES. Each command in a block is split into words (quotes removed,
# stopping at a separator or at the end of its bash -c region) and refused when
# it has a shape that has shipped a false green or a gate that fails correct
# work on this repository:
#
#   - `cargo test`: a name filter that matches nothing still exits 0. Gates run
#     one named test as `cargo nextest run -p PKG --lib --no-tests=fail
#     -E 'test(=module::path::name)'`, or a suite with `--test NAME`.
#   - `cargo nextest run` without `--no-tests=fail`, with a positional or `--`
#     filter, with more than one `-E`/`--skip`, or with a filter that is not
#     exactly `test(=...)`: `-E 'test(=real) | test(=wrong)'` runs one test and
#     exits 0. One test per command, chained with `&&`.
#   - `rg -c` / `--count` / `--count-matches`: zero matches print nothing, not
#     0, and exit 1. Count source structure in a Rust source-scan test instead.
#   - `-p devflow-cli`: no such package; the CLI package is `devflow`.
#
# A command whose quote does not close before it ends is refused as
# unparsable, for the same fail-closed reason as above.
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
staged_inputs=()

# shellcheck disable=SC2329 # Invoked by the EXIT trap immediately below.
cleanup_staged_inputs() {
    local input
    for input in "${staged_inputs[@]}"; do
        rm -f -- "$input"
    done
}

trap cleanup_staged_inputs EXIT

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
    # `--diff-filter=ACMR` selects added, copied, modified, and renamed plan
    # destinations. DELETED plans remain deliberately excluded: they have no
    # index blob to scan, and listing them would turn an intentional zero-file
    # scan into a read failure.
    while IFS= read -r -d '' path; do
        case "${path##*/}" in
            *PLAN.md) files+=("$path") ;;
        esac
    done < <(git diff --cached --name-only -z --diff-filter=ACMR)
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

# Named paths read from the filesystem. Staged paths must never use this branch:
# the index is what Git will commit, and it can deliberately differ from the
# working tree after partial staging or a subsequent local edit.
if [ "$staged" -eq 0 ]; then
    for path in "${files[@]}"; do
        if [ ! -f "$path" ] || [ ! -r "$path" ]; then
            echo "$SELF: cannot read '$path' (missing or unreadable)" >&2
            echo "  A named path that cannot be inspected is an error, not a skip:" >&2
            echo "  a check that reports success having inspected nothing is exactly" >&2
            echo "  the failure this scanner exists to prevent." >&2
            exit 2
        fi
    done
fi

hits=""
status=0

for path in "${files[@]}"; do
    input=$path
    if [ "$staged" -eq 1 ]; then
        if ! input="$(mktemp)"; then
            echo "$SELF: cannot create a temporary input for staged '$path'" >&2
            exit 2
        fi
        staged_inputs+=("$input")
        # `:$path` is Git's index-path syntax. Pass it as one quoted argument
        # and keep the original path in `fname` below for author diagnostics.
        # Do not use command substitution here: it strips terminal newlines.
        if ! git show ":$path" >"$input"; then
            echo "$SELF: cannot read staged index blob '$path'" >&2
            exit 2
        fi
    fi
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

    scan_commands(body, base)
}

# ---- Command rules (no apostrophes anywhere below: this is inside the
# single-quoted awk program). Each rule names a gate shape that passes on
# broken work or fails on correct work.

function report(base, o, msg) {
    printf "%s:%d: %s\n", fname, line_of(base + o - 1), msg
    violations++
}

# End of the bash -c region holding offset o, or 0 when o is in none.
function region_end(o,   n) {
    for (n = 1; n <= nregions; n++) {
        if (o >= rstart[n] && o <= rend[n]) return rend[n]
    }
    return 0
}

function is_word_start(body, o,   c) {
    if (o == 1) return 1
    c = substr(body, o - 1, 1)
    return index(" \t\n(|;&!`" SQ DQ, c) > 0
}

# Split the command starting at offset o into tok[1..ntok], quotes removed. It
# stops at an unquoted separator or redirect, or at the end of the bash -c
# region holding o. Returns -1 when a quote does not close inside that span.
function tokenize(body, o,   limit, c, cur, have, inq) {
    limit = region_end(o)
    if (limit == 0) limit = length(body)
    delete tok
    ntok = 0
    cur = ""
    have = 0
    inq = ""
    for (; o <= limit; o++) {
        c = substr(body, o, 1)
        if (inq != "") {
            # Inside double quotes a backslash escapes a quote, a backslash, a
            # dollar sign or a backtick, as the shell reads it.
            if (inq == DQ && c == "\\" && index(DQ "\\$`", substr(body, o + 1, 1)) > 0) {
                o++
                cur = cur substr(body, o, 1)
            } else if (c == inq) {
                inq = ""
            } else {
                cur = cur c
            }
            continue
        }
        if (c == "\\" && o < limit && substr(body, o + 1, 1) != "\n") {
            o++
            cur = cur substr(body, o, 1)
            have = 1
            continue
        }
        if (c == SQ || c == DQ) {
            inq = c
            have = 1
            continue
        }
        if (c == "\\" && substr(body, o + 1, 1) == "\n") {
            o++
            c = " "
        }
        if (c == " " || c == "\t") {
            if (have) tok[++ntok] = cur
            cur = ""
            have = 0
            continue
        }
        if (index("\n;&|<>)`", c) > 0) {
            # The 2 of 2>&1 is a file descriptor, not an argument.
            if ((c == ">" || c == "<") && cur ~ /^[0-9]+$/) have = 0
            break
        }
        cur = cur c
        have = 1
    }
    if (inq != "") return -1
    if (have) tok[++ntok] = cur
    return ntok
}

function scan_commands(body, base,   rest, off, o) {
    off = 0
    rest = body
    while (match(rest, /(rg|cargo)[ \t]/)) {
        o = off + RSTART
        if (is_word_start(body, o)) {
            if (substr(body, o, 2) == "rg") check_rg(body, base, o)
            else check_cargo(body, base, o)
        }
        off = o
        rest = substr(body, o + 1)
    }
}

# rg -c and --count print nothing (not 0) on zero matches and exit 1, so a
# count compared in shell misreads the zero case.
function check_rg(body, base, o,   n, i, j, t, ch, hit) {
    n = tokenize(body, o)
    if (n < 0) {
        report(base, o, "cannot parse this rg command (a quote does not close before the command ends)")
        return
    }
    for (i = 2; i <= n && hit == ""; i++) {
        t = tok[i]
        if (t == "--") break
        if (t ~ /^--count(-matches)?(=|$)/) {
            hit = t
        } else if (t ~ /^--/) {
            if (t !~ /=/ && (substr(t, 3) in RG_LONGVAL)) i++
        } else if (t ~ /^-./) {
            for (j = 2; j <= length(t); j++) {
                ch = substr(t, j, 1)
                if (ch == "c") {
                    hit = t
                    break
                }
                if (index(RG_SHORTVAL, ch) > 0) {
                    if (j == length(t)) i++
                    break
                }
            }
        }
    }
    if (hit != "") {
        report(base, o, "rg count flag " hit " in a verify gate (rg prints nothing, not 0, on zero matches; count source structure in a Rust source-scan test, or use rg -q)")
    }
}

function check_cargo(body, base, o,   n, k, i) {
    n = tokenize(body, o)
    if (n < 0) {
        report(base, o, "cannot parse this cargo command (a quote does not close before the command ends)")
        return
    }
    for (i = 2; i <= n; i++) {
        if (tok[i] == "--") break
        if (((tok[i] == "-p" || tok[i] == "--package") && tok[i + 1] == "devflow-cli") || tok[i] == "--package=devflow-cli") {
            report(base, o, "-p devflow-cli names no package (the CLI package is devflow)")
        }
    }
    k = 2
    if (tok[k] ~ /^\+/) k++
    if (tok[k] == "test") {
        report(base, o, "cargo test in a verify gate (a name filter that matches nothing still exits 0; run one named test as cargo nextest run -p PKG --lib --no-tests=fail -E " SQ "test(=module::path::name)" SQ ", a suite as cargo nextest run -p PKG --test NAME --no-tests=fail)")
    } else if (tok[k] == "nextest" && tok[k + 1] == "run") {
        check_nextest(base, o, k + 2, n)
    }
}

# One nextest command proves one named test: --no-tests=fail makes a name that
# matches nothing exit 4, and a filter combining two names passes when only one
# of them exists.
function check_nextest(base, o, i, n,   t, nofail, nfilt, npos, val) {
    for (; i <= n; i++) {
        t = tok[i]
        if (t == "--") {
            npos += n - i
            break
        }
        if (t == "--no-tests=fail") {
            nofail = 1
        } else if (t == "--no-tests") {
            if (tok[i + 1] == "fail") nofail = 1
            i++
        } else if (t == "-E" || t == "--filterset" || t == "--skip") {
            nfilt++
            val = tok[++i]
        } else if (t ~ /^(--filterset|--skip)=/) {
            nfilt++
            val = substr(t, index(t, "=") + 1)
        } else if (t ~ /^-E./) {
            nfilt++
            val = substr(t, 3)
        } else if (t ~ /^--/) {
            if (t !~ /=/ && (substr(t, 3) in NX_LONGVAL)) i++
        } else if (t ~ /^-./) {
            if (length(t) == 2 && index(NX_SHORTVAL, substr(t, 2, 1)) > 0) i++
        } else {
            npos++
        }
    }
    if (!nofail) {
        report(base, o, "cargo nextest run without --no-tests=fail (a filter that matches nothing then passes)")
    }
    if (npos > 0) {
        report(base, o, "cargo nextest run with a positional test filter (a substring match; use -E " SQ "test(=module::path::name)" SQ ")")
    }
    if (nfilt > 1) {
        report(base, o, "cargo nextest run with " nfilt " test filters (a wrong name is skipped silently; run one test per command and chain them with &&)")
    } else if (nfilt == 1 && val !~ /^test\(=[A-Za-z0-9_:]+\)$/) {
        report(base, o, "cargo nextest run filter is not exactly test(=module::path::name): " val " (one test per command, chained with &&)")
    }
}

BEGIN {
    SQ = sprintf("%c", 39)
    DQ = sprintf("%c", 34)
    OPEN = "<" "automated" ">"
    CLOSE = "<" "/automated" ">"
    violations = 0
    # Flags that take a separate value, so the next token is not a filter.
    RG_SHORTVAL = "efgtTmABCrMjEd"
    split("regexp file glob iglob type type-not type-add type-clear max-count after-context before-context context replace max-columns threads encoding max-depth sort sortr color colors pre pre-glob path-separator context-separator field-match-separator field-context-separator dfa-size-limit regex-size-limit max-filesize ignore-file engine hostname-bin hyperlink-format", rgv, " ")
    for (n in rgv) RG_LONGVAL[rgv[n]] = 1
    NX_SHORTVAL = "pFZRjP"
    split("color package exclude bin example test bench features build-jobs cargo-profile target target-dir manifest-path cargo-message-format config rerun run-ignored partition platform-filter test-threads retries flaky-result max-fail debugger tracer stress-count stress-duration failure-output success-output status-level final-status-level show-progress max-progress-running message-format message-format-version archive-file archive-format extract-to cargo-metadata workspace-remap binaries-metadata target-dir-remap build-dir-remap config-file user-config-file tool-config-file profile", nxv, " ")
    for (n in nxv) NX_LONGVAL[nxv[n]] = 1
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
' "$input")"; then
        status=1
    fi
    if [ -n "$out" ]; then
        hits="${hits}${out}"$'\n'
    fi
done

echo "$SELF: scanned $count file(s)"

if [ "$status" -ne 0 ]; then
    echo "$SELF: refusing a plan whose <automated> verify command has a shape" >&2
    echo "  that passes on broken work or fails on correct work:" >&2
    printf '%s' "$hits" | cut -c1-200 | sed 's/^/    /' >&2
    echo "  A cargo, nextest or rg line above names its own fix. A bash-only" >&2
    echo "  expansion (\${...} or declare -A) expands to NOTHING under the" >&2
    echo "  executor's zsh shell, so an assertion on it would pass without ever" >&2
    echo "  reading the value it claims to check. Wrap the command so it runs" >&2
    echo "  under bash, e.g.:" >&2
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
