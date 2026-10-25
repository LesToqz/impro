#!/usr/bin/env python3
"""Assemble the self-contained Windows tree of impro (stdlib only).

    bundle.py --prefix /ucrt64 --exe target/release/impro.exe --out dist/impro \
              [--extra WebView2Loader.dll ...] [--repo-root .]

The output tree is flat for the executable and its DLLs (GLib/GTK derive their
prefix from the directory of their own DLL, so `share/` and `lib/` next to
`impro.exe` are found without environment variables or a wrapper):

    impro.exe, *.dll
    share/glib-2.0/schemas/        (compiled)
    lib/gdk-pixbuf-2.0/2.10.0/loaders/*.dll + loaders.cache
    share/icons/Adwaita, share/icons/hicolor
    licenses/, LICENSE, THIRD_PARTY_NOTICES.md
    bundle-manifest.txt            (file, size, sha256)

The import closure is a pure function over an `imports_of(name_or_path)`
callable so it can be unit-tested without a Windows toolchain.
"""

from __future__ import annotations

import argparse
import functools
import hashlib
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Callable, Iterable

MAX_BUNDLE_BYTES = 400 * 1024 * 1024

# Windows system DLLs are never bundled, even if a prefix happens to hold a
# same-named file.
SYSTEM_DLLS = frozenset(
    n.lower()
    for n in (
        "kernel32.dll kernelbase.dll ntdll.dll user32.dll gdi32.dll advapi32.dll shell32.dll "
        "ole32.dll oleaut32.dll comdlg32.dll comctl32.dll shlwapi.dll ws2_32.dll wsock32.dll "
        "winmm.dll imm32.dll msvcrt.dll ucrtbase.dll bcrypt.dll crypt32.dll dnsapi.dll "
        "iphlpapi.dll mpr.dll netapi32.dll userenv.dll version.dll winspool.drv setupapi.dll "
        "dwmapi.dll uxtheme.dll usp10.dll opengl32.dll glu32.dll d3d11.dll dxgi.dll d2d1.dll "
        "dwrite.dll secur32.dll rpcrt4.dll psapi.dll shcore.dll normaliz.dll wldap32.dll "
        "dbghelp.dll msimg32.dll hid.dll dinput8.dll cfgmgr32.dll powrprof.dll "
        "gdiplus.dll msvcp_win.dll bcryptprimitives.dll wintrust.dll ncrypt.dll vcruntime140.dll"
    ).split()
)
SYSTEM_PREFIXES = ("api-ms-win-", "ext-ms-win-")


def is_system_dll(name: str) -> bool:
    low = name.lower()
    return low in SYSTEM_DLLS or low.startswith(SYSTEM_PREFIXES)


def import_closure(
    roots: Iterable[str],
    imports_of: Callable[[str], Iterable[str]],
    resolve: Callable[[str], str | None],
) -> dict[str, str]:
    """Transitive closure of DLL imports that resolve inside the prefix.

    `roots` are names/paths handed to `imports_of` first. `resolve(name)` returns
    the on-disk location of a DLL inside the prefix or None. Names are compared
    case-insensitively; the result maps lower-case name -> resolved location and
    omits system DLLs and anything `resolve` does not find (reported by caller).
    """
    found: dict[str, str] = {}
    seen: set[str] = set()
    queue = list(roots)
    while queue:
        item = queue.pop()
        for dep in imports_of(item):
            key = dep.lower()
            if key in seen or is_system_dll(dep):
                continue
            seen.add(key)
            loc = resolve(dep)
            if loc is None:
                continue
            found[key] = loc
            queue.append(loc)
    return found


def objdump_imports(objdump: str) -> Callable[[str], list[str]]:
    @functools.lru_cache(maxsize=None)
    def imports(path: str) -> list[str]:
        out = subprocess.run(
            [objdump, "-p", path], check=True, capture_output=True, text=True, errors="replace"
        ).stdout
        return re.findall(r"^\s*DLL Name:\s*(\S+)", out, re.MULTILINE)

    return imports


def make_resolver(search_dirs: Iterable[Path]) -> Callable[[str], str | None]:
    index: dict[str, str] = {}
    for d in search_dirs:
        if d.is_dir():
            for p in sorted(d.iterdir()):
                index.setdefault(p.name.lower(), str(p))
    return lambda name: index.get(name.lower())


def sha256_of(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def manifest_lines(root: Path) -> list[str]:
    """`<sha256>  <size>  <relative/path>` for every file, sorted, '/' separators."""
    lines = []
    for p in sorted(root.rglob("*")):
        if p.is_file() and p.name != "bundle-manifest.txt":
            rel = p.relative_to(root).as_posix()
            lines.append(f"{sha256_of(p)}  {p.stat().st_size}  {rel}")
    return lines


def tree_size(root: Path) -> int:
    return sum(p.stat().st_size for p in root.rglob("*") if p.is_file())


def check_size(total: int, limit: int = MAX_BUNDLE_BYTES) -> None:
    if total > limit:
        raise SystemExit(f"bundle is {total / 1e6:.1f} MB, over the {limit / 1e6:.0f} MB limit")


def log(msg: str) -> None:
    print(f"bundle: {msg}", flush=True)


def copy_tree(src: Path, dst: Path, skip: Callable[[Path], bool] = lambda p: False) -> int:
    n = 0
    if not src.is_dir():
        return 0
    for p in sorted(src.rglob("*")):
        if p.is_file() and not skip(p):
            target = dst / p.relative_to(src)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(p, target)
            n += 1
    return n


def rewrite_loaders_cache(text: str, prefix: Path) -> str:
    """Make every module path in a gdk-pixbuf loaders.cache relative to the bundle.

    gdk-pixbuf-query-loaders writes absolute paths below the build prefix
    (`C:\\\\msys64\\\\ucrt64\\\\lib\\\\gdk-pixbuf-2.0\\\\...` or the POSIX spelling). On
    Windows gdk-pixbuf resolves a relative module path against its own
    installation directory, so strip everything before `lib/gdk-pixbuf-2.0`.
    """
    out = []
    for line in text.splitlines():
        if line.startswith('"') and "gdk-pixbuf-2.0" in line:
            body = line.strip().strip('"')
            norm = body.replace("\\\\", "/").replace("\\", "/")
            idx = norm.find("lib/gdk-pixbuf-2.0")
            if idx >= 0:
                line = '"' + norm[idx:].replace("/", "\\\\") + '"'
        out.append(line)
    return "\n".join(out) + "\n"


def run_tool(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    log("run: " + " ".join(cmd))
    return subprocess.run(cmd, check=True, capture_output=True, text=True, **kw)


def build_bundle(args: argparse.Namespace) -> Path:
    prefix = Path(args.prefix)
    out = Path(args.out)
    repo = Path(args.repo_root)
    exe = Path(args.exe)
    if not exe.is_file():
        raise SystemExit(f"executable not found: {exe}")
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)

    shutil.copy2(exe, out / "impro.exe")
    extras = [Path(e) for e in args.extra]
    for e in extras:
        if not e.is_file():
            raise SystemExit(f"--extra file not found: {e}")

    # 1. DLL closure.
    imports = objdump_imports(args.objdump)
    resolve = make_resolver([prefix / "bin"])
    loader_dir = prefix / "lib" / "gdk-pixbuf-2.0" / "2.10.0" / "loaders"
    loader_dlls = sorted(loader_dir.glob("*.dll")) if loader_dir.is_dir() else []
    roots = [str(exe), *map(str, extras), *map(str, loader_dlls)]
    closure = import_closure(roots, imports, resolve)
    for dll in sorted(closure.values()):
        shutil.copy2(dll, out / Path(dll).name)
    for e in extras:
        shutil.copy2(e, out / e.name)
    log(f"{len(closure)} DLLs bundled from the import closure")

    unresolved = set()
    for item in [str(exe), *map(str, extras), *closure.values()]:
        for dep in imports(item):
            if not is_system_dll(dep) and resolve(dep) is None and not (out / dep).exists():
                unresolved.add(dep)
    for dep in sorted(unresolved):
        log(f"note: import not found in prefix and not a known system DLL: {dep}")

    # 2. GLib schemas (compiled in the bundle).
    schemas_src = prefix / "share" / "glib-2.0" / "schemas"
    schemas = out / "share" / "glib-2.0" / "schemas"
    copied = copy_tree(schemas_src, schemas, lambda p: p.suffix != ".xml" and p.suffix != ".override")
    if copied:
        if not args.no_tools:
            run_tool([args.glib_compile_schemas, str(schemas)])
        else:
            log("skipping glib-compile-schemas (--no-tools)")
    else:
        log("warning: no GLib schemas found in the prefix")

    # 3. gdk-pixbuf loaders + cache with bundle-relative paths.
    loaders_out = out / "lib" / "gdk-pixbuf-2.0" / "2.10.0" / "loaders"
    for dll in loader_dlls:
        loaders_out.mkdir(parents=True, exist_ok=True)
        shutil.copy2(dll, loaders_out / dll.name)
    if loader_dlls and not args.no_tools:
        res = run_tool([args.query_loaders, *map(str, sorted(loaders_out.glob("*.dll")))])
        cache = rewrite_loaders_cache(res.stdout, prefix)
        (loaders_out.parent / "loaders.cache").write_text(cache, encoding="utf-8")
    elif loader_dlls:
        log("skipping gdk-pixbuf-query-loaders (--no-tools)")

    # 4. Icons: Adwaita (minus the legacy raster sizes we do not need) and hicolor.
    copy_tree(prefix / "share" / "icons" / "Adwaita", out / "share" / "icons" / "Adwaita")
    for theme in ("hicolor",):
        copy_tree(prefix / "share" / "icons" / theme, out / "share" / "icons" / theme)
    copy_tree(repo / "packaging" / "icons" / "hicolor", out / "share" / "icons" / "hicolor")

    # 5. Licences.
    lic = out / "licenses"
    lic.mkdir(exist_ok=True)
    pkg_lic = prefix / "share" / "licenses"
    if pkg_lic.is_dir():
        copy_tree(pkg_lic, lic)
    for name in ("LICENSE", "THIRD_PARTY_NOTICES.md"):
        src = repo / name
        if not src.is_file():
            raise SystemExit(f"missing {src}")
        shutil.copy2(src, out / name)

    # 6. Manifest and size guard.
    total = tree_size(out)
    lines = manifest_lines(out)
    (out / "bundle-manifest.txt").write_text(
        "# sha256  size  path\n" + "\n".join(lines) + "\n", encoding="utf-8"
    )
    log(f"{len(lines)} files, {total / 1e6:.1f} MB total")
    check_size(total, args.max_bytes)
    return out


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--prefix", required=True, help="MSYS2 UCRT64 prefix (contains bin/, lib/, share/)")
    ap.add_argument("--exe", required=True, help="path of impro.exe")
    ap.add_argument("--out", required=True, help="output directory (recreated)")
    ap.add_argument("--extra", nargs="*", default=[], help="extra DLLs/files copied next to impro.exe")
    ap.add_argument("--repo-root", default=str(Path(__file__).resolve().parents[2]))
    ap.add_argument("--objdump", default="objdump")
    ap.add_argument("--glib-compile-schemas", default="glib-compile-schemas")
    ap.add_argument("--query-loaders", default="gdk-pixbuf-query-loaders")
    ap.add_argument("--no-tools", action="store_true", help="do not run the compile/query helper programs (tests)")
    ap.add_argument("--max-bytes", type=int, default=MAX_BUNDLE_BYTES)
    return ap.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    build_bundle(parse_args(argv))
    return 0


if __name__ == "__main__":
    sys.exit(main())
