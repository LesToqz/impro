#!/usr/bin/env bash
# The runner owns isolation and the shared heavy-job lock. It never builds.
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
unset WAYLAND_DISPLAY DISPLAY
export GDK_BACKEND=x11 PYTHONDONTWRITEBYTECODE=1
exec flock <home>/projects/impro-wt/.heavy.lock \
    systemd-run --user --scope -q -p MemoryMax=3500M -p MemorySwapMax=0 \
    xvfb-run -a -s '-screen 0 1600x1000x24' dbus-run-session -- \
    /usr/bin/python3 -I -B "$repo/scripts/graph-compare/compare.py" "$@"
