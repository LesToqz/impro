#!/usr/bin/env bash
# Zip a bundle tree as impro-<version>-x64-portable.zip with one top-level directory.
#
#   make-portable-zip.sh <bundle-dir> <version> <output-dir>
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: ${0##*/} <bundle-dir> <version> <output-dir>" >&2
    exit 2
fi
bundle=$1
version=$2
out=$3
name="impro-${version}-x64-portable"

[[ -f $bundle/impro.exe ]] || { echo "make-portable-zip: $bundle/impro.exe missing" >&2; exit 1; }
mkdir -p "$out"
out=$(cd "$out" && pwd)
stage=$(mktemp -d)
trap 'rm -rf -- "$stage"' EXIT
cp -a "$bundle" "$stage/$name"
rm -f -- "$out/$name.zip"
(cd "$stage" && zip -q -r -9 "$out/$name.zip" "$name")
echo "make-portable-zip: $out/$name.zip ($(wc -c <"$out/$name.zip") bytes)"
