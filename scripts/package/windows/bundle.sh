#!/usr/bin/env bash
# Bundle the GTK runtime around an executable and write the diagnostics next to it.
#
#   bundle.sh <exe> <exe-name> <bundle-dir> <out-dir> [extra-file...]
#
# Run in an MSYS2 UCRT64 shell from the repository root. Produces <bundle-dir>
# (bundle.py), <out-dir>/imports-report.txt and <out-dir>/bundle-manifest.txt.
set -euo pipefail

if [[ $# -lt 4 ]]; then
    echo "usage: ${0##*/} <exe> <exe-name> <bundle-dir> <out-dir> [extra-file...]" >&2
    exit 2
fi
exe=$1 name=$2 bundle=$3 out=$4
shift 4
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$here/../../.." && pwd)

ls -l -- "$exe"
extra=()
if (($# > 0)); then
    extra=(--extra "$@")
fi
python "$repo/packaging/windows/bundle.py" \
    --prefix "$(cygpath -m "$MINGW_PREFIX")" \
    --exe "$exe" --exe-name "$name" \
    --out "$bundle" "${extra[@]}"
mkdir -p -- "$out"
"$here/imports-report.sh" "$bundle" "$out/imports-report.txt"
cp -- "$bundle/bundle-manifest.txt" "$out/bundle-manifest.txt"
