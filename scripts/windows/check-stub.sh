#!/usr/bin/env bash
# check-stub.sh — type-check the workspace for Windows (x86_64-pc-windows-gnu) on a Linux host
# that cannot reach the MSYS2 mirrors (where fetch-sysroot.sh / check.sh cannot run).
#
# `cargo check` never links, so the GTK *-sys build scripts only need pkg-config to answer
# "is gtk+-3.0 >= x.y installed?". This script answers from the *host's* development files
# (libgtk-3-dev & friends, as used for the Linux build) by pointing PKG_CONFIG_LIBDIR at a
# copy of the host .pc files. Nothing is linked and nothing runs on Windows: this proves
# "it compiles for the target", exactly like check.sh, not that it works.
#
# Examples:
#   scripts/windows/check-stub.sh -p impro --all-targets
#   IMPRO_WIN_CARGO_CMD=clippy scripts/windows/check-stub.sh -p impro --all-targets -- -D warnings
#
# Environment:
#   IMPRO_WIN_CARGO_CMD      cargo subcommand to run instead of `check` (e.g. `clippy`).
#   IMPRO_WIN_STUB_DIR       where the copied .pc files live (default ~/.cache/impro-win-stub).
#   CARGO_TARGET_DIR         honoured as usual; set it to share build output between worktrees.
#
# Needs: the Linux GTK development packages (see README), gcc-mingw-w64-x86-64 (the `cc`
# crate probes for the target C compiler), rustup.
set -euo pipefail

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" | sed '$d' | sed 's/^# \{0,1\}//' >&2
  echo "Usage: scripts/windows/check-stub.sh [cargo check args...]" >&2
  exit 0
fi

target=x86_64-pc-windows-gnu
stub="${IMPRO_WIN_STUB_DIR:-$HOME/.cache/impro-win-stub}"

if ! rustup target list --installed | grep -qx "$target"; then
  echo "check-stub.sh: installing rust-std for $target" >&2
  rustup target add "$target" >&2
fi

if ! compgen -G "$stub/gtk+-3.0.pc" >/dev/null; then
  echo "check-stub.sh: copying host pkg-config files to $stub" >&2
  mkdir -p "$stub"
  for dir in /usr/lib/x86_64-linux-gnu/pkgconfig /usr/lib/pkgconfig /usr/share/pkgconfig; do
    [ -d "$dir" ] && cp -n "$dir"/*.pc "$stub"/ 2>/dev/null || true
  done
  if ! compgen -G "$stub/gtk+-3.0.pc" >/dev/null; then
    echo "check-stub.sh: gtk+-3.0.pc not found; install libgtk-3-dev first" >&2
    exit 1
  fi
fi

export PKG_CONFIG_ALLOW_CROSS=1
# LIBDIR replaces the default search path so the probes see only the copied files.
export PKG_CONFIG_LIBDIR="$stub"
export IMPRO_SKIP_WIN_RESOURCES=1

exec cargo "${IMPRO_WIN_CARGO_CMD:-check}" --target "$target" "$@"
