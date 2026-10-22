#!/usr/bin/env bash
# Capture only: build the QA example explicitly before running.
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
exec xvfb-run -a -s '-screen 0 1600x1000x24' dbus-run-session -- \
    env GDK_BACKEND=x11 GIO_USE_VFS=local PYTHONDONTWRITEBYTECODE=1 \
    /usr/bin/python3 "$repo/scripts/parity/matrix.py" "$@"
