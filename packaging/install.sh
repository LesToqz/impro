#!/usr/bin/env bash
set -euo pipefail
applications="$HOME/.local/share/applications"
metainfo="$HOME/.local/share/metainfo"
hicolor="$HOME/.local/share/icons/hicolor"
sizes=(16 24 32 48 64 128 256 512)
launcher="$HOME/.local/bin/impro"
metainfo_name="dev.impro.impro.metainfo.xml"
# Refresh the desktop MIME cache (mimeinfo.cache, which feeds "Open With") and
# the icon cache. Both tools are optional; a failure never aborts the script.
refresh_caches() {
    if command -v update-desktop-database >/dev/null && [[ -d "$applications" ]]; then
        update-desktop-database -q "$applications" || true
        # With no MIME entries left the tool still writes a header-only cache.
        if [[ -f "$applications/mimeinfo.cache" ]] && ! grep -q '=' "$applications/mimeinfo.cache"; then
            rm -f "$applications/mimeinfo.cache"
        fi
    fi
    if command -v gtk-update-icon-cache >/dev/null && [[ -d "$hicolor" ]]; then
        gtk-update-icon-cache -q -t -f "$hicolor" || true
    fi
}
case "${1:-}" in
    --uninstall)
        rm -f "$applications/impro.desktop" "$metainfo/$metainfo_name" "$hicolor/scalable/apps/impro.svg" "$launcher"
        for size in "${sizes[@]}"; do
            rm -f "$hicolor/${size}x${size}/apps/impro.png"
        done
        refresh_caches
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
mkdir -p "$applications" "$metainfo" "$hicolor/scalable/apps" "$(dirname -- "$launcher")"
install -m 755 "$binary" "$launcher"
install -m 644 "$repo/packaging/impro.desktop" "$applications/impro.desktop"
install -m 644 "$repo/packaging/$metainfo_name" "$metainfo/$metainfo_name"
install -m 644 "$repo/packaging/icons/hicolor/scalable/apps/impro.svg" "$hicolor/scalable/apps/impro.svg"
for size in "${sizes[@]}"; do
    mkdir -p "$hicolor/${size}x${size}/apps"
    install -m 644 "$repo/packaging/icons/hicolor/${size}x${size}/apps/impro.png" "$hicolor/${size}x${size}/apps/impro.png"
done
refresh_caches
# The launcher entry runs "impro" from PATH, so a menu or file-manager launch
# only works when the session's PATH contains the install directory.
case ":$PATH:" in
    *":$(dirname -- "$launcher"):"*) ;;
    *) echo "impro: note: $(dirname -- "$launcher") is not on PATH; add it so the menu entry and 'impro' find the program." >&2 ;;
esac
