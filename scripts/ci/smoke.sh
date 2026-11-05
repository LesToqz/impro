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
export XDG_CONFIG_HOME="$HOME/.config"
mkdir -p "$HOME"
state="$XDG_CONFIG_HOME/impro/state.json"

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
expected='usage: impro [-h] [--version] [path ...]'
[[ $first == "$expected" ]] || fail "--help first line was: $first"
echo "PASS --help usage line"

# The help text names the path argument (a folder, or files to open).
help=$(env -u DISPLAY -u WAYLAND_DISPLAY "$bin" --help)
grep -q '^  path ' <<<"$help" || fail "--help does not describe the path argument"
echo "PASS --help describes path"

# An invalid folder is a usage error.
status=0
env -u DISPLAY -u WAYLAND_DISPLAY "$bin" "$work/does-not-exist" >/dev/null 2>&1 || status=$?
[[ $status -eq 2 ]] || fail "invalid folder exited $status, expected 2"
echo "PASS invalid folder exits 2"

# A missing file is a usage error too, even next to a valid folder.
status=0
env -u DISPLAY -u WAYLAND_DISPLAY "$bin" "$folder" "$work/missing.txt" >/dev/null 2>&1 || status=$?
[[ $status -eq 2 ]] || fail "missing file exited $status, expected 2"
echo "PASS missing file exits 2"

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

# The opened folder is remembered in state.json (written off the UI thread, flushed at exit).
[[ -s $state ]] || fail "no state.json written to $state"
canon=$(realpath -- "$folder")
recorded() {  # recorded <last-folder> <first-recent>
    python3 - "$state" "$1" "$2" <<'PY'
import json, sys
state = json.load(open(sys.argv[1]))
assert state["last_folder"] == sys.argv[2], state
assert state["recent"][0] == sys.argv[3], state
PY
}
recorded "$canon" "$canon" || fail "state.json does not record the opened folder"
echo "PASS opened folder recorded in state.json"

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

# Command-line shapes. Each runs from a chosen working directory under the same display.
launch() {  # launch <cwd> <shot-name> [impro arguments...]
    local cwd=$1 shot="$work/shots/$2.png"
    shift 2
    (cd -- "$cwd" && IMPRO_QA_SCREENSHOT="$shot" xvfb-run -a dbus-run-session -- "$bin" "$@") \
        >"$work/launch-shape.log" 2>&1 || { cat "$work/launch-shape.log" >&2; fail "launch '$*' from $cwd failed"; }
    [[ -s $shot ]] || fail "launch '$*' from $cwd wrote no screenshot"
}

# A folder with a file beside it, and a lone file ("Open with" from a file manager).
launch "$work" folder-and-file "$folder" "$folder/hello.txt"
echo "PASS folder plus file arguments launch"
launch "$work" file-only "$folder/hello.txt"
recorded "$canon" "$canon" || fail "a file-only launch should open and record the file's folder"
echo "PASS file-only launch opens the file's folder"

# A terminal in some directory keeps it.
other="$work/other"
mkdir -p "$other"
canon_other=$(realpath -- "$other")
launch "$other" terminal-cwd
recorded "$canon_other" "$canon_other" || fail "a launch from a terminal directory should open and record it"
echo "PASS launch from a terminal directory keeps it"

# A desktop launcher starts in $HOME with no path: the last folder reopens, $HOME does not.
python3 - "$state" "$canon" "$canon_other" <<'PY'
import json, sys
path, folder, other = sys.argv[1:]
json.dump({"version": 1, "last_folder": folder, "recent": [other, folder]}, open(path, "w"))
PY
launch "$HOME" launcher-cwd
recorded "$canon" "$canon" || fail "a launch from \$HOME should reopen the last folder"
echo "PASS launch from \$HOME reopens the last folder"
