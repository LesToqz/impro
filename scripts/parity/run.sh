#!/usr/bin/env bash
# Capture only: build the QA example explicitly before running.
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
screen='1600x1000x24'
previous=''
for argument in "$@"; do
    if [[ "$previous" == '--scale' && "$argument" == '2' ]] || [[ "$argument" == '--scale=2' ]]; then
        screen='3200x2000x24'
    fi
    previous="$argument"
done
exec xvfb-run -a -s "-screen 0 $screen" dbus-run-session -- \
    env GDK_BACKEND=x11 GIO_USE_VFS=local PYTHONDONTWRITEBYTECODE=1 \
    /usr/bin/python3 "$repo/scripts/parity/matrix.py" "$@"
