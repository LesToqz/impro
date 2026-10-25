#!/usr/bin/env bash
# Write SHA256SUMS.txt for the release files in a directory.
#
#   write-checksums.sh <dir> <version>
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: ${0##*/} <dir> <version>" >&2
    exit 2
fi
dir=$1
version=$2
files=("impro-${version}-x64-setup.exe" "impro-${version}-x64-portable.zip")
cd "$dir"
for f in "${files[@]}"; do
    [[ -f $f ]] || { echo "write-checksums: $f missing in $dir" >&2; exit 1; }
done
sha256sum "${files[@]}" >SHA256SUMS.txt
sha256sum --check --strict SHA256SUMS.txt
