#!/usr/bin/env bash
# check.sh — type-check the workspace for Windows (x86_64-pc-windows-gnu) on a Linux host.
#
# Runs `cargo check --target x86_64-pc-windows-gnu "$@"` against a local MSYS2 UCRT64
# sysroot (see fetch-sysroot.sh) that only provides headers and pkg-config files, so
# the *-sys build scripts can probe GTK3 & friends. Nothing is linked, nothing runs
# on Windows: this proves "it compiles for the target", not that it works.
#
# Examples:
#   scripts/windows/check.sh -p impro-core
#   scripts/windows/check.sh --workspace --lib --bins
#
# Environment:
#   IMPRO_WIN_SYSROOT        sysroot cache base (see fetch-sysroot.sh).
#   IMPRO_SKIP_WIN_RESOURCES exported as 1: build scripts that need a real Windows
#                            toolchain (resource compiler, icon embedding, manifest
#                            linking) must skip that step when it is set. Harmless on
#                            a real Windows build, where it is simply not set.
set -euo pipefail

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" | sed '$d' | sed 's/^# \{0,1\}//' >&2
  echo "Usage: scripts/windows/check.sh [cargo check args...]" >&2
  exit 0
fi

target=x86_64-pc-windows-gnu
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if ! rustup target list --installed | grep -qx "$target"; then
  echo "check.sh: installing rust-std for $target" >&2
  rustup target add "$target" >&2
fi

sysroot="$("$here/fetch-sysroot.sh")"
pcdirs="$sysroot/ucrt64/lib/pkgconfig:$sysroot/ucrt64/share/pkgconfig"

export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_SYSROOT_DIR="$sysroot"
export PKG_CONFIG_PATH="$pcdirs"
# LIBDIR replaces the host search path, so a host-installed GTK can never satisfy a probe.
export PKG_CONFIG_LIBDIR="$pcdirs"
export IMPRO_SKIP_WIN_RESOURCES=1

exec cargo check --target "$target" "$@"
