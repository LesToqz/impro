#!/usr/bin/env bash
# fetch-sysroot.sh — build a minimal MSYS2 UCRT64 "sysroot" on this machine so
# `cargo check --target x86_64-pc-windows-gnu` can probe GTK3 & friends via pkg-config.
#
# Nothing is linked and nothing is run: we only need headers and *.pc files. The
# script downloads the MSYS2 UCRT64 repository database, resolves the dependency
# closure of gtk3 (+ gdk-pixbuf2, cairo, pango, glib2, librsvg, adwaita-icon-theme,
# harfbuzz, atk), downloads every package, VERIFIES its SHA-256 against the database,
# and extracts it. Idempotent and resumable; offline once a sysroot is ready.
#
# Output: the sysroot path on stdout (last line); progress and sizes on stderr.
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/windows/fetch-sysroot.sh [--update] [--full] [--help]

  (default)  use the ready sysroot under the cache if there is one (no network);
             otherwise fetch the current MSYS2 database and build one.
  --update   re-download the database; if it changed, build a new sysroot
             directory (keyed by the database hash) and make it the current one.
  --full     also extract libraries, DLLs and data (default: include/ and
             pkgconfig dirs only, which is all `cargo check` needs).

Environment:
  IMPRO_WIN_SYSROOT   cache base directory (default: ~/.cache/impro-win-sysroot).
                      Sysroots live in <base>/<db-sha256-prefix>[-full]/ and
                      <base>/current points at the one in use.
  IMPRO_WIN_MSYS2_URL repository URL (default: https://repo.msys2.org/mingw/ucrt64).

The sysroot never goes inside the repository. Needs: curl, tar with zstd, python3.
USAGE
}

update=0
full=0
for arg in "$@"; do
  case "$arg" in
    --update) update=1 ;;
    --full) full=1 ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
base="${IMPRO_WIN_SYSROOT:-$HOME/.cache/impro-win-sysroot}"
url="${IMPRO_WIN_MSYS2_URL:-https://repo.msys2.org/mingw/ucrt64}"
suffix=""
[ "$full" = 1 ] && suffix="-full"
roots=(gtk3 gdk-pixbuf2 cairo pango glib2 librsvg adwaita-icon-theme harfbuzz atk)

log() { echo "fetch-sysroot: $*" >&2; }

report() {
  local dir="$1"
  log "sysroot: $dir"
  log "db sha256: $(cat "$dir/.db-sha256")"
  log "packages: $(wc -l <"$dir/.closure.tsv"), disk used: $(du -sh "$dir" | cut -f1)"
  echo "$dir"
}

# Offline fast path: a ready sysroot already exists.
if [ "$update" = 0 ] && [ -L "$base/current$suffix" ] && [ -f "$base/current$suffix/.ready" ]; then
  report "$(readlink -f "$base/current$suffix")"
  exit 0
fi

for tool in curl tar python3 sha256sum zstd; do
  command -v "$tool" >/dev/null || {
    log "missing required tool: $tool"
    exit 1
  }
done

mkdir -p "$base/dl"
db="$base/dl/ucrt64.db"
log "downloading repository database from $url"
curl -fsSL --retry 3 -o "$db.part" "$url/ucrt64.db"
mv "$db.part" "$db"
db_sha="$(sha256sum "$db" | cut -d' ' -f1)"
sysroot="$base/${db_sha:0:12}$suffix"

if [ -f "$sysroot/.ready" ]; then
  log "database unchanged, sysroot already complete"
else
  mkdir -p "$sysroot/.packages"
  work="$(mktemp -d "$base/dl/db.XXXXXX")"
  trap 'rm -rf "$work"' EXIT
  tar --zstd -xf "$db" -C "$work"
  python3 -I "$here/msys2_resolve.py" "$work" "${roots[@]}" >"$sysroot/.closure.tsv"
  echo "$db_sha" >"$sysroot/.db-sha256"

  patterns=('ucrt64/include' 'ucrt64/lib/pkgconfig' 'ucrt64/share/pkgconfig')
  [ "$full" = 1 ] && patterns=('ucrt64')

  while IFS=$'\t' read -r name file sha csize _; do
    [ -f "$sysroot/.packages/$name" ] && continue
    pkg="$base/dl/$file"
    if [ ! -f "$pkg" ]; then
      log "downloading $file ($((csize / 1024)) KiB)"
      curl -fsSL --retry 3 -C - -o "$pkg.part" "$url/$file"
      mv "$pkg.part" "$pkg"
    fi
    got="$(sha256sum "$pkg" | cut -d' ' -f1)"
    if [ "$got" != "$sha" ]; then
      rm -f "$pkg"
      log "SHA-256 mismatch for $file (database $sha, got $got)"
      exit 1
    fi
    # Packages may lack some of the directories: tolerate "not found in archive".
    tar --zstd -xf "$pkg" -C "$sysroot" --wildcards "${patterns[@]}" 2>/dev/null || true
    echo "$sha" >"$sysroot/.packages/$name"
    rm -f "$pkg"
  done <"$sysroot/.closure.tsv"

  touch "$sysroot/.ready"
fi

ln -sfn "$sysroot" "$base/current$suffix"
report "$sysroot"
