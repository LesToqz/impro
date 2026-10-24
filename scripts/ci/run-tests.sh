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
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo"

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

run_integration() {
    group "integration tests"
    run_suite 'kind(test)' --test '*' -- "$@"
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
