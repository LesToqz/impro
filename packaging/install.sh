#!/usr/bin/env bash
set -euo pipefail
applications="$HOME/.local/share/applications"
icons="$HOME/.local/share/icons/hicolor/scalable/apps"
launcher="$HOME/.local/bin/impro"
case "${1:-}" in
    --uninstall)
        rm -f "$applications/impro.desktop" "$icons/impro.svg" "$launcher"
        exit 0
        ;;
    '') ;;
    *) echo "Usage: $0 [--uninstall]" >&2; exit 2 ;;
esac
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
# A prebuilt executable avoids a rebuild for packaging and isolated checks.
binary=${IMPRO_INSTALL_BINARY:-}
if [[ -z "$binary" ]]; then
    cargo build --manifest-path "$repo/Cargo.toml" --release -p impro
    binary="$repo/target/release/impro"
fi
if [[ ! -f "$binary" || ! -x "$binary" ]]; then
    echo "impro: install binary must be an executable file: $binary" >&2
    exit 1
fi
mkdir -p "$applications" "$icons" "$(dirname -- "$launcher")"
install -m 755 "$binary" "$launcher"
install -m 644 "$repo/packaging/impro.desktop" "$applications/impro.desktop"
install -m 644 "$repo/packaging/impro.svg" "$icons/impro.svg"
