#!/usr/bin/env bash
# Build the impro Debian package from a release binary.
#
#   scripts/package/build-deb.sh <path-to-impro-binary> [--version X.Y.Z] [--out DIR]
#
# Run it on Ubuntu/Debian: the Depends line is derived from the binary's
# direct shared-library dependencies via ldd and dpkg -S. Two runs with the
# same inputs and SOURCE_DATE_EPOCH produce byte-identical packages.
set -euo pipefail

usage() {
    cat <<'USAGE'
usage: build-deb.sh <path-to-impro-binary> [--version X.Y.Z] [--out DIR]

  --version X.Y.Z  package version (default: workspace version from Cargo.toml)
  --out DIR        output directory (default: dist/)

Environment:
  SOURCE_DATE_EPOCH  mtime for all files (default: HEAD commit time)
USAGE
}

die() {
    echo "build-deb: $*" >&2
    exit 1
}

# Debian renamed several libraries in the 64-bit time_t transition (Ubuntu
# 24.04 and Debian 13 use the t64 names, Debian 12 the old ones). Left side:
# package found by dpkg -S on the build machine; right side: older name.
declare -A T64_RENAMES=(
    [libglib2.0-0t64]=libglib2.0-0
    [libgtk-3-0t64]=libgtk-3-0
    [libatk1.0-0t64]=libatk1.0-0
    [libpng16-16t64]=libpng16-16
)

# Runtime packages the binary does not link but needs.
EXTRA_DEPENDS=(
    librsvg2-common
    adwaita-icon-theme
    gsettings-desktop-schemas
    shared-mime-info
)

bin="" version="" out=""
while [[ $# -gt 0 ]]; do
    case $1 in
        -h | --help) usage; exit 0 ;;
        --version) [[ $# -ge 2 ]] || die "--version needs a value"; version=$2; shift 2 ;;
        --out) [[ $# -ge 2 ]] || die "--out needs a value"; out=$2; shift 2 ;;
        -*) usage >&2; exit 2 ;;
        *) [[ -z $bin ]] || { usage >&2; exit 2; }; bin=$1; shift ;;
    esac
done
[[ -n $bin ]] || { usage >&2; exit 2; }
[[ -f $bin && -x $bin ]] || die "not an executable file: $bin"
bin=$(realpath -- "$bin")

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
out=${out:-$root/dist}
if [[ -z $version ]]; then
    version=$(sed -n '/^\[workspace\.package\]/,/^\[/{s/^version *= *"\(.*\)"/\1/p}' "$root/Cargo.toml" | head -n 1)
fi
[[ $version =~ ^[0-9]+(\.[0-9]+)*([~+][0-9A-Za-z.]+)?$ ]] || die "bad version: '$version'"
SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct)}
export SOURCE_DATE_EPOCH
arch=$(dpkg --print-architecture)

work=$(mktemp -d -t impro-deb-XXXXXXXX)
trap 'rm -rf -- "$work"' EXIT
stage="$work/stage"

# ---- Depends: direct NEEDED sonames -> files (ldd) -> packages (dpkg -S).
declare -A soname_path=()
while read -r name _ path _; do
    [[ $path == /* ]] && soname_path[$name]=$path
done < <(ldd "$bin")

declare -A seen=()
depends=()
mapping=()
while read -r soname; do
    path=${soname_path[$soname]:-}
    [[ -n $path ]] || die "cannot resolve $soname with ldd"
    real=$(readlink -f -- "$path")
    pkg=$(dpkg -S "$real" 2>/dev/null | head -n 1 | cut -d: -f1 || true)
    [[ -n $pkg ]] || pkg=$(dpkg -S "$path" 2>/dev/null | head -n 1 | cut -d: -f1 || true)
    [[ -n $pkg ]] || die "no package owns $real ($soname)"
    if [[ -n ${T64_RENAMES[$pkg]:-} ]]; then
        entry="$pkg | ${T64_RENAMES[$pkg]}"
    elif [[ $pkg == *t64 ]]; then
        echo "build-deb: warning: $pkg looks like a t64 rename, assuming ${pkg%t64}" >&2
        entry="$pkg | ${pkg%t64}"
    else
        entry=$pkg
    fi
    mapping+=("$soname -> $real -> $entry")
    if [[ -z ${seen[$entry]:-} ]]; then
        seen[$entry]=1
        depends+=("$entry")
    fi
done < <(readelf -d "$bin" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p')
for extra in "${EXTRA_DEPENDS[@]}"; do
    depends+=("$extra")
done
depends_line=$(printf '%s\n' "${depends[@]}" | LC_ALL=C sort -u | paste -sd, - | sed 's/,/, /g')
echo "== ldd -> dpkg -S mapping ==" >&2
printf '%s\n' "${mapping[@]}" >&2
echo "== Depends ==" >&2
echo "$depends_line" >&2

# Cross-check with dpkg-shlibdeps when available (informational).
if command -v dpkg-shlibdeps >/dev/null; then
    mkdir -p "$work/shlibdeps/debian"
    printf 'Source: impro\n\nPackage: impro\nArchitecture: any\n' > "$work/shlibdeps/debian/control"
    (cd "$work/shlibdeps" && dpkg-shlibdeps -O -e"$bin" 2>/dev/null | sed -n 's/^shlibs:Depends=//p') \
        > "$work/shlibdeps.out" || true
    echo "== dpkg-shlibdeps cross-check (informational) ==" >&2
    cat "$work/shlibdeps.out" >&2
fi

# ---- Staging tree.
inst() { install -D -m "$1" -- "$2" "$stage/$3"; }
inst 0755 "$bin" usr/bin/impro
inst 0644 "$root/packaging/impro.desktop" usr/share/applications/impro.desktop
while IFS= read -r -d '' png; do
    rel=${png#"$root/packaging/icons/"}
    inst 0644 "$png" "usr/share/icons/$rel"
done < <(find "$root/packaging/icons/hicolor" -name 'impro.png' -print0 | LC_ALL=C sort -z)
inst 0644 "$root/packaging/icons/hicolor/scalable/apps/impro.svg" usr/share/icons/hicolor/scalable/apps/impro.svg
inst 0644 "$root/packaging/debian/copyright" usr/share/doc/impro/copyright
inst 0644 "$root/THIRD_PARTY_NOTICES.md" usr/share/doc/impro/THIRD_PARTY_NOTICES.md
gzip -9nc "$root/README.md" > "$work/README.md.gz"
inst 0644 "$work/README.md.gz" usr/share/doc/impro/README.md.gz
gzip -9nc "$root/packaging/debian/impro.1" > "$work/impro.1.gz"
inst 0644 "$work/impro.1.gz" usr/share/man/man1/impro.1.gz

# The package has no Debian revision (native), so the changelog is changelog.gz.
{
    printf 'impro (%s) unstable; urgency=low\n\n' "$version"
    printf '  * Release %s.\n\n' "$version"
    printf ' -- LesToqz <214641162+LesToqz@users.noreply.github.com>  %s\n' "$(date -u -d "@$SOURCE_DATE_EPOCH" -R)"
} | gzip -9n > "$work/changelog.gz"
inst 0644 "$work/changelog.gz" usr/share/doc/impro/changelog.gz

# ---- Control file.
size=$(find "$stage" -type f -printf '%s\n' | awk '{s+=$1} END {print int((s+1023)/1024)}')
mkdir -p "$stage/DEBIAN"
control=$(<"$root/packaging/debian/control.in")
control=${control//@VERSION@/$version}
control=${control//@ARCH@/$arch}
control=${control//@INSTALLED_SIZE@/$size}
control=${control//@DEPENDS@/$depends_line}
printf '%s\n' "$control" > "$stage/DEBIAN/control"
chmod 0644 "$stage/DEBIAN/control"

# ---- Normalise modes and mtimes, then build.
find "$stage" -type d -exec chmod 0755 {} +
find "$stage" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
mkdir -p "$out"
deb="$out/impro_${version}_${arch}.deb"
dpkg-deb --root-owner-group -Zxz --build "$stage" "$deb" >&2
echo "$deb"
