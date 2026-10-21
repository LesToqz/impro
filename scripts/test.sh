#!/usr/bin/env bash
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
# local avoids starting a GVfs FUSE daemon; accessibility remains enabled.
export GDK_BACKEND=x11 GIO_USE_VFS=local
exec xvfb-run -a dbus-run-session -- cargo test --manifest-path "$repo/Cargo.toml" --workspace "$@"
