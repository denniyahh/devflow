#!/usr/bin/env bash
# Staged named-break gate for `cargo semver-checks` on devflow-core.
#
# devflow-core is published, and CLAUDE.md requires `cargo semver-checks check-release -p devflow-core`
# before committing a `pub` break. 50-CONTEXT.md R-9 requires every break to be named. semver-checks
# 0.51.0 already exits 100 at the Phase 50 base (Phase 49's breaks), so "exit 0" is no gate. This script
# compares the real run, item by item, with the breaks named up to a plan's stage.
#
# Usage:
#   semver-gate.sh check --through <stage> [--log <file>]       run semver-checks, save its stdout, compare
#   semver-gate.sh compare --through <stage> [--fail-count <n>] <file>   compare a saved stdout
#   semver-gate.sh --selftest
# Stages, in order: baseline 50-08 50-10 50-11 50-16. `--through S` includes every row introduced at S
# or earlier.
# Exit: 0 `SEMVER verdict=match`; 1 `SEMVER verdict=mismatch ...`; 2 usage; 3 `SEMVER verdict=error ...`
# (no failure lines, output this parser does not recognise, or semver-checks itself failing).
#
# The table below is the R-9 list. A later plan whose real run disagrees with it makes its change
# additive or stops and reports a blocker; no later plan edits it (50-16 checks that every commit
# touching this file is a 50-01 commit).
set -euo pipefail

# introduced-at  required|allowed  lint-id  item-identifier (whole-word match in the item text)
NAMED_BREAKS='
baseline required constructible_struct_adds_field AgentResult.checkpoint
baseline required enum_marked_non_exhaustive RecoverError
baseline required enum_marked_non_exhaustive WorkflowError
50-08 required enum_marked_non_exhaustive MonitorError
50-08 allowed enum_variant_added MonitorError
50-10 required function_parameter_count_changed spawn_monitor
50-11 required function_missing run_pipe_owning_monitor
50-16 required struct_pub_field_missing monitor_pid
'
STAGES=(baseline 50-08 50-10 50-11 50-16)

T="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SELF="$T/$(basename "${BASH_SOURCE[0]}")"
TMP=""

cleanup() { if [ -n "$TMP" ]; then rm -rf -- "$TMP"; fi; }
trap cleanup EXIT

usage() {
    echo "usage: semver-gate.sh check --through <stage> [--log <file>]" >&2
    echo "       semver-gate.sh compare --through <stage> [--fail-count <n>] <file>" >&2
    echo "       semver-gate.sh --selftest" >&2
    echo "stages: ${STAGES[*]}" >&2
    exit 2
}

stage_index() {
    local i
    for i in "${!STAGES[@]}"; do
        if [ "${STAGES[$i]}" = "$1" ]; then echo "$i"; return 0; fi
    done
    return 1
}

verdict_error() { echo "SEMVER verdict=error reason=$1"; exit 3; }

# parse_log <file>: fills ITEM_LINT/ITEM_TEXT. A block starts at `--- failure <lint>[: ]`; its items are
# the two-space-indented lines after `Failed in:` up to a blank line (or the end); an item's text drops
# everything from ` in /` or `, previously in ` onward.
ITEM_LINT=()
ITEM_TEXT=()
BLOCKS=0
parse_log() {
    local line lint="" in_items=0 block_items=0 text
    while IFS= read -r line || [ -n "$line" ]; do
        if [[ $line == '--- failure '* ]]; then
            if [ "$BLOCKS" -gt 0 ] && [ "$block_items" -eq 0 ]; then
                verdict_error "failure block $lint lists no items"
            fi
            [[ $line =~ ^---\ failure\ ([a-z0-9_]+)[:\ ] ]] || verdict_error "unparseable failure header: $line"
            lint=${BASH_REMATCH[1]}
            BLOCKS=$((BLOCKS + 1)); block_items=0; in_items=0
        elif [ -n "$lint" ] && [ "$line" = 'Failed in:' ]; then
            in_items=1
        elif [ -z "$line" ]; then
            in_items=0
        elif [ "$in_items" -eq 1 ]; then
            [[ $line == '  '[!\ ]* ]] || verdict_error "unparseable item line in block $lint: $line"
            text=${line#  }
            text=${text%% in /*}
            text=${text%%, previously in *}
            ITEM_LINT+=("$lint"); ITEM_TEXT+=("$text"); block_items=$((block_items + 1))
        fi
    done <"$1"
    if [ "$BLOCKS" -gt 0 ] && [ "$block_items" -eq 0 ]; then verdict_error "failure block $lint lists no items"; fi
    [ "$BLOCKS" -gt 0 ] || verdict_error "semver-checks produced no failure lines"
}

# compare_log <stage> <file> [<fail count reported by semver-checks>]
compare_log() {
    local through=$1 log=$2 fail_count=${3:-} limit stage kind lint ident i j matched
    local -a row_kind=() row_lint=() row_ident=() row_hit=() unexpected=() missing=()
    limit="$(stage_index "$through")"
    while read -r stage kind lint ident; do
        [ -n "$stage" ] || continue
        [ "$(stage_index "$stage")" -le "$limit" ] || continue
        row_kind+=("$kind"); row_lint+=("$lint"); row_ident+=("$ident"); row_hit+=(0)
    done <<<"$NAMED_BREAKS"

    parse_log "$log"
    if [ -n "$fail_count" ] && [ "$fail_count" -ne "$BLOCKS" ]; then
        verdict_error "parsed $BLOCKS failure blocks but semver-checks reported $fail_count failed checks"
    fi

    for i in "${!ITEM_TEXT[@]}"; do
        echo "SEMVER lint=${ITEM_LINT[$i]} item=${ITEM_TEXT[$i]}"
        matched=0
        for j in "${!row_lint[@]}"; do
            if [ "${row_lint[$j]}" = "${ITEM_LINT[$i]}" ] \
                && printf '%s\n' "${ITEM_TEXT[$i]}" | rg -q -w -F -e "${row_ident[$j]}"; then
                row_hit[j]=1; matched=1
            fi
        done
        [ "$matched" -eq 1 ] || unexpected+=("${ITEM_LINT[$i]}:${ITEM_TEXT[$i]}")
    done
    for j in "${!row_lint[@]}"; do
        if [ "${row_kind[$j]}" = required ] && [ "${row_hit[$j]}" -eq 0 ]; then
            missing+=("${row_lint[$j]}:${row_ident[$j]}")
        fi
    done

    if [ "${#unexpected[@]}" -eq 0 ] && [ "${#missing[@]}" -eq 0 ]; then
        echo "SEMVER verdict=match"
        exit 0
    fi
    local IFS=';'
    echo "SEMVER verdict=mismatch unexpected=${unexpected[*]:--} missing=${missing[*]:--}"
    exit 1
}

run_check() {
    local through=$1 log=$2 root rc=0 fail_count
    root="$(git -C "$T" rev-parse --show-toplevel)"
    TMP="$(mktemp -d)"
    (cd "$root" && cargo semver-checks check-release -p devflow-core --color never) \
        >"$TMP/out" 2>"$TMP/err" || rc=$?
    if [ -n "$log" ]; then cp -- "$TMP/out" "$log"; fi
    case "$rc" in
        0|100) ;;
        *) tail -n 20 "$TMP/err" >&2; verdict_error "cargo semver-checks exited $rc" ;;
    esac
    # Second, independent count: semver-checks' own "N fail" total must equal the blocks parsed.
    fail_count="$(rg -o -r '$1' '([0-9]+) fail,' "$TMP/err" | tail -n 1 || true)"
    [ -n "$fail_count" ] || verdict_error "no 'N fail' count in semver-checks stderr"
    compare_log "$through" "$TMP/out" "$fail_count"
}

# ---- selftest: synthetic logs in the real 0.51.0 layout -------------------------------------------

W=/work/devflow/crates/devflow-core/src
P=/home/u/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/devflow-core-2.12.0/src

# block <lint> <summary> <description> <ref> <item>...
block() {
    local lint=$1 summary=$2 description=$3 ref=$4
    shift 4
    cat <<BLOCK

--- failure $lint: $summary ---

Description:
$description
        ref: $ref
       impl: https://github.com/obi1kenobi/cargo-semver-checks/tree/v0.51.0/src/lints/$lint.ron

Failed in:
BLOCK
    printf '  %s\n' "$@"
}

b_field() {
    block constructible_struct_adds_field "struct exhaustively constructible through public API adds field" \
        "A pub struct that could be exhaustively constructed with a literal using only public API has a new pub field, breaking existing exhaustive literals." \
        https://doc.rust-lang.org/reference/expressions/struct-expr.html \
        "field AgentResult.checkpoint in $W/agent_result.rs:60"
}
b_nonexh() {
    block enum_marked_non_exhaustive "enum marked #[non_exhaustive]" \
        "A public enum has been marked #[non_exhaustive]. Pattern-matching on it outside of its crate must now include a wildcard pattern like \`_\`, or it will fail to compile." \
        https://doc.rust-lang.org/cargo/reference/semver.html#attr-adding-non-exhaustive "$@"
}
b_variant() {
    block enum_variant_added "enum variant added on exhaustive enum" \
        "A publicly-visible enum without #[non_exhaustive] has a new variant." \
        https://doc.rust-lang.org/cargo/reference/semver.html#enum-variant-new \
        "variant MonitorError:SocketPath in $W/monitor.rs:41"
}
b_params() {
    block function_parameter_count_changed "pub fn parameter count changed" \
        "A publicly-visible function now takes a different number of parameters." \
        https://doc.rust-lang.org/cargo/reference/semver.html#fn-change-arity \
        "devflow_core::monitor::spawn_monitor now takes 3 parameters instead of 2, in $W/monitor.rs:308"
}
b_fnmissing() {
    block function_missing "pub fn removed or renamed" \
        "A publicly-visible function cannot be imported by its prior path. A \`pub use\` may have been removed, or the function itself may have been renamed or removed entirely." \
        https://doc.rust-lang.org/cargo/reference/semver.html#item-remove "$@"
}
b_fieldmissing() {
    block struct_pub_field_missing "pub struct's pub field removed or renamed" \
        "A publicly-visible struct has at least one public field that is no longer available under its prior name." \
        https://doc.rust-lang.org/cargo/reference/semver.html#item-remove \
        "field monitor_pid of struct State, previously in file $P/state.rs:288"
}
b_traitmethod() {
    block trait_method_missing "pub trait method removed or renamed" \
        "A trait method is no longer callable, and may have been renamed or removed entirely." \
        https://doc.rust-lang.org/cargo/reference/semver.html#major-any-item-removal \
        "method shutdown of trait Probe, previously in file $P/supervisor.rs:12"
}

log_baseline() { b_field; b_nonexh "enum RecoverError in $W/recover.rs:19" "enum WorkflowError in $W/workflow.rs:20"; }
log_50_11() {
    b_field
    b_nonexh "enum MonitorError in $W/monitor.rs:28" "enum RecoverError in $W/recover.rs:19" \
        "enum WorkflowError in $W/workflow.rs:20"
    b_variant
    b_params
    b_fnmissing "function devflow_core::monitor::run_pipe_owning_monitor, previously in file $P/monitor.rs:961" "$@"
}
log_50_16() { log_50_11; b_fieldmissing; }

selftest() {
    local passed=0 failed=0 d
    TMP="$(mktemp -d)"
    d=$TMP

    # expect <case> <exit code> <exact final SEMVER line> -- <semver-gate.sh args...>
    expect() {
        local name=$1 want_rc=$2 want_line=$3 rc=0 got
        shift 4
        bash "$SELF" "$@" >"$d/out" 2>"$d/err" || rc=$?
        got="$(rg '^SEMVER verdict=' "$d/out" | tail -n 1 || true)"
        if [ "$rc" -eq "$want_rc" ] && [ "$got" = "$want_line" ]; then
            printf '  ok   %s\n' "$name"; passed=$((passed + 1))
        else
            printf '  FAIL %s — exit %s (want %s), got [%s], want [%s]\n' "$name" "$rc" "$want_rc" "$got" "$want_line"
            failed=$((failed + 1))
        fi
    }

    log_baseline >"$d/baseline.log"
    { log_baseline; b_traitmethod; } >"$d/unlisted.log"
    {
        log_baseline
        block enum_variant_missing "pub enum variant removed or renamed" \
            "A publicly-visible enum has at least one variant that is no longer available under its prior name." \
            https://doc.rust-lang.org/cargo/reference/semver.html#item-remove \
            "variant WorkflowError::Paused, previously in file $P/workflow.rs:31"
    } >"$d/cross-lint.log"
    log_baseline | rg -v -F 'enum WorkflowError in' >"$d/missing.log"
    : >"$d/empty.log"
    log_50_11 "function devflow_core::other_fn, previously in file $P/lib.rs:7" >"$d/extra-item.log"
    log_50_16 >"$d/s50-16.log"
    log_baseline | sed 's/enum WorkflowError in/enum WorkflowErrorKind in/' >"$d/word.log"
    { log_baseline; printf '\n--- failure Bad-Lint: not a lint id ---\n\nFailed in:\n  enum X in %s/x.rs:1\n' "$W"; } >"$d/badheader.log"
    { log_baseline; printf '\n--- failure function_missing: pub fn removed or renamed ---\n\nDescription:\nx\n\nFailed in:\n'; } >"$d/noitems.log"

    expect "baseline log matches --through baseline" 0 \
        "SEMVER verdict=match" -- compare --through baseline "$d/baseline.log"
    expect "an unlisted lint is unexpected" 1 \
        "SEMVER verdict=mismatch unexpected=trait_method_missing:method shutdown of trait Probe missing=-" \
        -- compare --through baseline "$d/unlisted.log"
    expect "an unlisted lint naming a listed identifier is unexpected" 1 \
        "SEMVER verdict=mismatch unexpected=enum_variant_missing:variant WorkflowError::Paused missing=-" \
        -- compare --through baseline "$d/cross-lint.log"
    expect "a missing required item is named missing" 1 \
        "SEMVER verdict=mismatch unexpected=- missing=enum_marked_non_exhaustive:WorkflowError" \
        -- compare --through baseline "$d/missing.log"
    expect "an empty run is an error" 3 \
        "SEMVER verdict=error reason=semver-checks produced no failure lines" \
        -- compare --through baseline "$d/empty.log"
    expect "an unexpected item under a listed lint is unexpected" 1 \
        "SEMVER verdict=mismatch unexpected=function_missing:function devflow_core::other_fn missing=-" \
        -- compare --through 50-11 "$d/extra-item.log"
    expect "a stage whose break has not happened yet is missing" 1 \
        "SEMVER verdict=mismatch unexpected=- missing=enum_marked_non_exhaustive:MonitorError" \
        -- compare --through 50-08 "$d/baseline.log"
    expect "the full 50-16 log matches --through 50-16 (allowed row accepted)" 0 \
        "SEMVER verdict=match" -- compare --through 50-16 "$d/s50-16.log"
    expect "a break ahead of its stage is unexpected" 1 \
        "SEMVER verdict=mismatch unexpected=struct_pub_field_missing:field monitor_pid of struct State missing=-" \
        -- compare --through 50-11 "$d/s50-16.log"
    expect "identifiers match as whole words only" 1 \
        "SEMVER verdict=mismatch unexpected=enum_marked_non_exhaustive:enum WorkflowErrorKind missing=enum_marked_non_exhaustive:WorkflowError" \
        -- compare --through baseline "$d/word.log"
    expect "an unparseable failure header is an error" 3 \
        "SEMVER verdict=error reason=unparseable failure header: --- failure Bad-Lint: not a lint id ---" \
        -- compare --through baseline "$d/badheader.log"
    expect "a failure block with no items is an error" 3 \
        "SEMVER verdict=error reason=failure block function_missing lists no items" \
        -- compare --through baseline "$d/noitems.log"
    expect "a fail count that disagrees with the parsed blocks is an error" 3 \
        "SEMVER verdict=error reason=parsed 2 failure blocks but semver-checks reported 3 failed checks" \
        -- compare --through baseline --fail-count 3 "$d/baseline.log"
    expect "a fail count that agrees with the parsed blocks matches" 0 \
        "SEMVER verdict=match" -- compare --through baseline --fail-count 2 "$d/baseline.log"
    expect "an unknown stage is a usage error" 2 "" -- compare --through 50-99 "$d/baseline.log"

    echo "SELFTEST passed=$passed failed=$failed"
    [ "$failed" -eq 0 ]
}

# ---- argument parsing ----------------------------------------------------------------------------

main() {
    [ "$#" -ge 1 ] || usage
    local mode=$1 through="" log="" fail_count="" file=""
    shift
    case "$mode" in
        --selftest) [ "$#" -eq 0 ] || usage; selftest; return ;;
        check|compare) ;;
        *) usage ;;
    esac
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --through) [ "$#" -ge 2 ] || usage; through=$2; shift 2 ;;
            --log) [ "$mode" = check ] && [ "$#" -ge 2 ] || usage; log=$2; shift 2 ;;
            --fail-count) [ "$mode" = compare ] && [ "$#" -ge 2 ] || usage; fail_count=$2; shift 2 ;;
            -*) usage ;;
            *) [ "$mode" = compare ] && [ -z "$file" ] || usage; file=$1; shift ;;
        esac
    done
    if [ -z "$through" ] || ! stage_index "$through" >/dev/null; then usage; fi
    case "$fail_count" in ''|*[!0-9]*) [ -z "$fail_count" ] || usage ;; esac
    if [ "$mode" = check ]; then
        run_check "$through" "$log"
    else
        [ -n "$file" ] && [ -f "$file" ] || usage
        compare_log "$through" "$file" "$fail_count"
    fi
}

main "$@"
