#!/usr/bin/env bash
# End-to-end smoke test of a built impro binary.
#
#   scripts/ci/smoke.sh <path-to-impro-binary>
#
# Checks the command line without a display, then launches the application
# under Xvfb, captures the first complete paint and verifies the screenshot.
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: ${0##*/} <impro-binary>" >&2
    exit 2
fi
bin=$(realpath -- "$1")
[[ -x $bin ]] || { echo "FAIL binary is not executable: $bin" >&2; exit 1; }

work=$(mktemp -d -t impro-smoke-XXXXXXXX)
trap 'rm -rf -- "$work"' EXIT
folder="$work/project"
mkdir -p "$folder" "$work/shots"
printf 'hello\n' > "$folder/hello.txt"
export HOME="$work/home"
mkdir -p "$HOME"

fail() {
    echo "FAIL $*" >&2
    exit 1
}

# --version needs no display.
version=$(env -u DISPLAY -u WAYLAND_DISPLAY "$bin" --version) || fail "--version exited non-zero"
[[ -n $version ]] || fail "--version printed nothing"
echo "PASS --version without a display: $version"

# --help starts with the usage line.
first=$(env -u DISPLAY -u WAYLAND_DISPLAY "$bin" --help | head -n 1)
expected='usage: impro [-h] [--version] [folder]'
[[ $first == "$expected" ]] || fail "--help first line was: $first"
echo "PASS --help usage line"

# An invalid folder is a usage error.
status=0
env -u DISPLAY -u WAYLAND_DISPLAY "$bin" "$work/does-not-exist" >/dev/null 2>&1 || status=$?
[[ $status -eq 2 ]] || fail "invalid folder exited $status, expected 2"
echo "PASS invalid folder exits 2"

# Launch under a virtual display and capture the first complete paint.
png="$work/shots/first-paint.png"
export GDK_BACKEND=x11 GIO_USE_VFS=local IMPRO_QA_SCREENSHOT="$png" IMPRO_QA_TIMING=1
unset WAYLAND_DISPLAY
log="$work/launch.log"
status=0
xvfb-run -a dbus-run-session -- "$bin" "$folder" >"$log" 2>&1 || status=$?
if [[ $status -ne 0 ]]; then
    cat "$log" >&2
    fail "launch exited $status"
fi
echo "PASS launch exits 0"

[[ -s $png ]] || { cat "$log" >&2; fail "no screenshot written to $png"; }
magic=$(head -c 8 "$png" | od -An -tx1 | tr -d ' \n')
[[ $magic == 89504e470d0a1a0a ]] || fail "screenshot is not a PNG (magic $magic)"
echo "PASS screenshot is a non-empty PNG ($(stat -c %s "$png") bytes)"

timing=$(grep -E 'first-paint-ms' "$log" | head -n 1 || true)
[[ -n $timing ]] || { cat "$log" >&2; fail "no first-paint-ms in output"; }
echo "PASS first paint reported: $timing"

# Keep the capture for CI artifact upload when requested.
if [[ -n ${IMPRO_SMOKE_OUTPUT_DIR:-} ]]; then
    mkdir -p "$IMPRO_SMOKE_OUTPUT_DIR"
    cp -- "$png" "$IMPRO_SMOKE_OUTPUT_DIR/first-paint.png"
fi
