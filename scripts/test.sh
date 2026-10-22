#!/usr/bin/env bash
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
# local avoids starting a GVfs FUSE daemon; accessibility remains enabled.
export GDK_BACKEND=x11 GIO_USE_VFS=local
# Package selectors must not also select the entire workspace.
selection=(--workspace)
for argument in "$@"; do
    case "$argument" in
        -p|--package|--package=*|-p?*) selection=(); break ;;
    esac
done
exec xvfb-run -a dbus-run-session -- cargo test --manifest-path "$repo/Cargo.toml" "${selection[@]}" "$@"
