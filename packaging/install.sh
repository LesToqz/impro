#!/usr/bin/env bash
set -euo pipefail
applications="$HOME/.local/share/applications"
hicolor="$HOME/.local/share/icons/hicolor"
sizes=(16 24 32 48 64 128 256 512)
launcher="$HOME/.local/bin/impro"
case "${1:-}" in
    --uninstall)
        rm -f "$applications/impro.desktop" "$hicolor/scalable/apps/impro.svg" "$launcher"
        for size in "${sizes[@]}"; do
            rm -f "$hicolor/${size}x${size}/apps/impro.png"
        done
        if command -v gtk-update-icon-cache >/dev/null && [[ -d "$hicolor" ]]; then
            gtk-update-icon-cache -q -t -f "$hicolor" || true
        fi
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
mkdir -p "$applications" "$hicolor/scalable/apps" "$(dirname -- "$launcher")"
install -m 755 "$binary" "$launcher"
install -m 644 "$repo/packaging/impro.desktop" "$applications/impro.desktop"
install -m 644 "$repo/packaging/icons/hicolor/scalable/apps/impro.svg" "$hicolor/scalable/apps/impro.svg"
for size in "${sizes[@]}"; do
    mkdir -p "$hicolor/${size}x${size}/apps"
    install -m 644 "$repo/packaging/icons/hicolor/${size}x${size}/apps/impro.png" "$hicolor/${size}x${size}/apps/impro.png"
done
if command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -q -t -f "$hicolor" || true
fi
