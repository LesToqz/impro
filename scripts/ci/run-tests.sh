#!/usr/bin/env bash
# Single entry point for the functional test suite, used by CI and by humans.
#
#   scripts/ci/run-tests.sh unit|integration|doc|js|all [extra runner arguments]
#
#   unit         library and binary unit-test targets
#   integration  the tests/ directory targets
#   doc          documentation tests (cargo test --doc)
#   js           Node tests for the Git pages
#   all          unit, integration, doc, js in that order
#
# IMPRO_CI_RUNNER selects the runner for unit/integration: cargo (default) or
# nextest. GTK tests need a display and a session bus, so both run under the
# same xvfb-run + dbus-run-session wrapper as scripts/test.sh. cargo runs the
# tests of one binary on one GTK thread; nextest starts one process per test
# and needs the serial `ci` profile in .config/nextest.toml (see there).
#
#   scripts/ci/run-tests.sh --self-test-critical-filter
#
# checks the CRITICAL-message filter used for the tolerated targets (below).
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo"

# Integration targets that run with GLib criticals non-fatal (G_DEBUG unset)
# instead of G_DEBUG=fatal-criticals. With libvte 0.76 (Ubuntu 24.04 runners)
# dropping the last reference to a terminal while its window is unrealized
# makes libvte emit two GLib-GObject criticals during teardown (see
# known_criticals below); libvte 0.84 does not. Both targets pass when
# criticals are not fatal. Any other CRITICAL in their output fails the run.
# The opt-in nextest runner does not apply this tolerance.
tolerated_pkg=impro-terminal
tolerated_targets=(test_resizegate test_terminal_startup)

runner=${IMPRO_CI_RUNNER:-cargo}
# local avoids starting a GVfs FUSE daemon; fatal criticals surface GTK misuse.
export GDK_BACKEND=x11 GIO_USE_VFS=local G_DEBUG=fatal-criticals

usage() {
    echo "usage: ${0##*/} unit|integration|doc|js|all [extra arguments]" >&2
    exit 2
}

[[ $# -ge 1 ]] || usage
mode=$1
shift

group() {
    echo "=== $1 ==="
}

gui() {
    xvfb-run -a dbus-run-session -- "$@"
}

# run_suite <nextest filterset> <cargo target flags...> -- [extra arguments]
run_suite() {
    local filter=$1
    shift
    local flags=()
    while [[ $# -gt 0 && $1 != -- ]]; do
        flags+=("$1")
        shift
    done
    [[ ${1:-} == -- ]] && shift
    case "$runner" in
        nextest)
            gui cargo nextest run --workspace --locked --profile "${NEXTEST_PROFILE:-ci}" \
                -E "$filter" "$@"
            ;;
        cargo)
            gui cargo test --workspace --no-fail-fast --locked "${flags[@]}" "$@"
            ;;
        *)
            echo "run-tests: unknown IMPRO_CI_RUNNER '$runner' (cargo|nextest)" >&2
            exit 2
            ;;
    esac
}

run_unit() {
    group "unit tests"
    run_suite 'kind(lib) | kind(bin)' --lib --bins -- "$@"
}

# Reads output on stdin and prints every line that mentions CRITICAL but is not
# one of the two known libvte teardown messages (matched on message text, not
# on process id or time). Exits non-zero if any line was printed.
unknown_criticals() {
    local line bad=0
    while IFS= read -r line; do
        [[ $line == *CRITICAL* ]] || continue
        case "$line" in
            *"GLib-GObject-CRITICAL **: "*"invalid (NULL) pointer instance") ;;
            *"GLib-GObject-CRITICAL **: "*"g_signal_handlers_disconnect_matched: assertion 'G_TYPE_CHECK_INSTANCE (instance)' failed") ;;
            *)
                printf '%s\n' "$line"
                bad=1
                ;;
        esac
    done
    return "$bad"
}

self_test_critical_filter() {
    local known unrelated out failed=0
    known=$'(a.out:11): GLib-GObject-CRITICAL **: 10:01:02.003: invalid (NULL) pointer instance\n'
    known+=$'(b.out:222): GLib-GObject-CRITICAL **: 23:59:59.999: g_signal_handlers_disconnect_matched: assertion \'G_TYPE_CHECK_INSTANCE (instance)\' failed\n'
    unrelated="(x:1): Gtk-CRITICAL **: 00:00:00.000: gtk_widget_show: assertion 'GTK_IS_WIDGET (widget)' failed"
    if unknown_criticals <<<"$known" >/dev/null; then
        echo "self-test (i) known messages: accepted (expected)"
    else
        echo "self-test (i) known messages: REJECTED (unexpected)"
        failed=1
    fi
    if out=$(unknown_criticals <<<"$known$unrelated"); then
        echo "self-test (ii) with unrelated critical: ACCEPTED (unexpected)"
        failed=1
    else
        echo "self-test (ii) with unrelated critical: rejected (expected), offending line: $out"
    fi
    if unknown_criticals </dev/null; then
        echo "self-test (iii) empty output: accepted (expected)"
    else
        echo "self-test (iii) empty output: REJECTED (unexpected)"
        failed=1
    fi
    return "$failed"
}

# Prints "<package> <target>" for every integration test target.
test_targets() {
    cargo metadata --no-deps --format-version 1 --locked | python3 -c '
import json, sys
for pkg in json.load(sys.stdin)["packages"]:
    for t in pkg["targets"]:
        if "test" in t["kind"]:
            print(pkg["name"], t["name"])
'
}

run_integration() {
    group "integration tests"
    # Some tests spawn the QA example binaries; selecting only test targets
    # (--test '*', or nextest) does not build examples.
    cargo build --examples --workspace --locked
    if [[ $runner != cargo ]]; then
        run_suite 'kind(test)' --test '*' -- "$@"
        return
    fi

    local rc=0 name all rest=() pkg_targets=()
    all=$(test_targets)
    mapfile -t pkg_targets < <(awk -v p="$tolerated_pkg" '$1 == p { print $2 }' <<<"$all")
    for name in "${pkg_targets[@]}"; do
        [[ " ${tolerated_targets[*]} " == *" $name "* ]] || rest+=(--test "$name")
    done
    echo "integration targets: $(wc -l <<<"$all") in total," \
        "${#tolerated_targets[@]} tolerated, $(($(wc -l <<<"$all") - ${#tolerated_targets[@]})) strict"

    # (a) strict: fatal criticals, every target except the tolerated ones.
    group "integration tests (fatal criticals)"
    gui cargo test --workspace --exclude "$tolerated_pkg" --no-fail-fast --locked \
        --test '*' -- "$@" || rc=1
    if [[ ${#rest[@]} -gt 0 ]]; then
        gui cargo test -p "$tolerated_pkg" --no-fail-fast --locked "${rest[@]}" -- "$@" || rc=1
    fi

    # (b) tolerated: criticals non-fatal, only the known ones allowed.
    group "integration tests (tolerated criticals: ${tolerated_targets[*]})"
    local flags=() out bad
    for name in "${tolerated_targets[@]}"; do
        flags+=(--test "$name")
    done
    out=$(mktemp)
    env -u G_DEBUG xvfb-run -a dbus-run-session -- \
        cargo test -p "$tolerated_pkg" --no-fail-fast --locked "${flags[@]}" -- "$@" \
        2>&1 | tee "$out" || rc=1
    if ! bad=$(unknown_criticals <"$out"); then
        echo "run-tests: unexpected CRITICAL messages in tolerated targets:" >&2
        printf '%s\n' "$bad" >&2
        rc=1
    fi
    rm -f "$out"
    return "$rc"
}

run_doc() {
    group "doc tests"
    gui cargo test --doc --workspace --locked "$@"
}

run_js() {
    group "js tests"
    node --test crates/impro-gitviews/tests/js/*.test.mjs "$@"
}

case "$mode" in
    --self-test-critical-filter) self_test_critical_filter ;;
    unit) run_unit "$@" ;;
    integration) run_integration "$@" ;;
    doc) run_doc "$@" ;;
    js) run_js "$@" ;;
    all)
        run_unit "$@"
        run_integration "$@"
        run_doc "$@"
        run_js "$@"
        ;;
    *) usage ;;
esac
