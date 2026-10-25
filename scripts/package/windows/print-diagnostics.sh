#!/usr/bin/env bash
# Print the bundle manifest and the DLL imports report to the job log.
#
#   print-diagnostics.sh <out-dir>
#
# Used with `if: failure()` so the evidence is readable from `gh run view --log`
# and not only from the uploaded artifacts.
set -uo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: ${0##*/} <out-dir>" >&2
    exit 2
fi
out=$1
for name in bundle-manifest.txt imports-report.txt; do
    echo "::group::$name"
    if [[ -f $out/$name ]]; then
        wc -l <"$out/$name" | sed "s/^/lines: /"
        cat -- "$out/$name"
    else
        echo "(missing: $out/$name)"
    fi
    echo "::endgroup::"
done
exit 0
