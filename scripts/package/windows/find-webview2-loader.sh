#!/usr/bin/env bash
# Locate the WebView2Loader.dll that webview2-com-sys copies into its build output.
#
#   find-webview2-loader.sh <cargo-build-dir>      e.g. target/release/build
#
# Prints the Windows-style path (cygpath -m when available) on stdout; everything
# else goes to stderr. Fails unless exactly one distinct file is found. A hash that
# differs from the one verified for webview2-com-sys 0.38.2 is only a warning.
set -euo pipefail

EXPECTED_SHA256=8427b1fc58ec707813e5c0a51eb5d69397bb333250a7b891be4d3b123f1e0f1c

if [[ $# -ne 1 ]]; then
    echo "usage: ${0##*/} <cargo-build-dir>" >&2
    exit 2
fi
root=$1
[[ -d $root ]] || { echo "::error::find-webview2-loader: $root is not a directory" >&2; exit 1; }

mapfile -t found < <(find "$root" -path '*webview2-com-sys-*/out/x64/WebView2Loader.dll' -type f | sort)
if ((${#found[@]} == 0)); then
    echo "::error::WebView2Loader.dll not found below $root (expected webview2-com-sys-*/out/x64/WebView2Loader.dll); was impro built with the WebView2 host?" >&2
    exit 1
fi
declare -A distinct=()
for f in "${found[@]}"; do
    distinct[$(sha256sum -- "$f" | cut -d' ' -f1)]=$f
done
if ((${#distinct[@]} != 1)); then
    echo "::error::found ${#distinct[@]} different WebView2Loader.dll files below $root:" >&2
    for h in "${!distinct[@]}"; do echo "  $h  ${distinct[$h]}" >&2; done
    exit 1
fi
for hash in "${!distinct[@]}"; do
    loader=${distinct[$hash]}
done
echo "find-webview2-loader: $loader sha256=$hash" >&2
if [[ $hash != "$EXPECTED_SHA256" ]]; then
    echo "::warning::WebView2Loader.dll sha256 $hash differs from the verified $EXPECTED_SHA256 (webview2-com-sys 0.38.2); check the crate version" >&2
fi
if command -v cygpath >/dev/null 2>&1; then
    cygpath -m -- "$(realpath -- "$loader")"
else
    realpath -- "$loader"
fi
