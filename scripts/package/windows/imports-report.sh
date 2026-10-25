#!/usr/bin/env bash
# Write "<file> <imported DLL>" for impro.exe and every DLL of a bundle tree.
#
#   imports-report.sh <bundle-dir> <report-file>
#
# Run in an MSYS2 shell (needs objdump). The smoke test checks each imported
# DLL against the installed tree and System32 to name a missing DLL quickly.
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: ${0##*/} <bundle-dir> <report-file>" >&2
    exit 2
fi
dir=$1
report=$2
: >"$report"
shopt -s nullglob
for f in "$dir"/*.exe "$dir"/*.dll "$dir"/lib/gdk-pixbuf-2.0/2.10.0/loaders/*.dll; do
    name=${f#"$dir"/}
    objdump -p "$f" | sed -n 's/^[[:space:]]*DLL Name:[[:space:]]*//p' | while read -r dll; do
        printf '%s %s\n' "$name" "$dll" >>"$report"
    done
done
echo "imports-report: $(wc -l <"$report") import lines written to $report"
