#!/usr/bin/env bash
# Print the workspace package version from the root Cargo.toml.
set -euo pipefail
root=${1:-.}
version=$(awk '/^\[workspace\.package\]/{in_pkg=1; next} /^\[/{in_pkg=0} in_pkg && /^version[[:space:]]*=/{gsub(/[" ]/, "", $3); print $3; exit}' "$root/Cargo.toml")
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
    echo "workspace-version: could not read a version from $root/Cargo.toml (got '$version')" >&2
    exit 1
fi
printf '%s\n' "$version"
