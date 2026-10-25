# Packaging

impro ships as a Debian package for Linux and as an installer plus a portable zip
for Windows. Both are built in CI and can be built locally. Windows usage is in
[docs/windows.md](windows.md); the CI jobs are in [docs/ci.md](ci.md).

## Linux `.deb`

Built by `scripts/package/build-deb.sh` from a release binary:

```sh
cargo build --release --locked -p impro --bin impro
scripts/package/build-deb.sh target/release/impro [--version X.Y.Z] [--out DIR]
```

Output: `impro_<version>_<arch>.deb` in `dist/` by default. The version defaults to the
workspace version (a Debian native package, so the changelog is `changelog.gz`).

### Contents

| Path | Notes |
| --- | --- |
| `/usr/bin/impro` | 0755 |
| `/usr/share/applications/impro.desktop` | from `packaging/impro.desktop` |
| `/usr/share/icons/hicolor/<N>x<N>/apps/impro.png`, `scalable/apps/impro.svg` | 16 to 512 |
| `/usr/share/doc/impro/` | `copyright`, `changelog.gz`, `README.md.gz`, `THIRD_PARTY_NOTICES.md` |
| `/usr/share/man/man1/impro.1.gz` | from `packaging/debian/impro.1` |

All files are `root:root`, data 0644, directories 0755, xz-compressed, no maintainer
scripts. Control data comes from `packaging/debian/control.in`; the licence file is
`packaging/debian/copyright` (DEP-5).

### Depends and Recommends

`Depends` is derived from the binary's direct `NEEDED` entries (not the transitive
`ldd` closure): each soname is resolved with `ldd`, mapped with `dpkg -S`, and the
`t64` renames use an explicit table with `new | old` alternatives (for example
`libgtk-3-0t64 | libgtk-3-0`). GTK, GLib, ATK, Pango, Cairo, gdk-pixbuf, WebKitGTK
4.1, JavaScriptCore, VTE 2.91 and libgcc/libc are covered that way. Four packages are
added because they are needed at run time but not linked: `librsvg2-common`,
`adwaita-icon-theme`, `gsettings-desktop-schemas`, `shared-mime-info`.
`Recommends: git`. No version constraints are written, deliberately looser than
`dpkg-shlibdeps` (whose constraints would exclude Debian 12).

### Reproducibility

The archive is deterministic: `SOURCE_DATE_EPOCH` (default: HEAD commit time) is the mtime of all files. Two builds of the same binary gave the same SHA-256
when checked locally.

### CI

`.github/workflows/package-linux.yml` (job `deb`, Ubuntu 24.04) builds the release
binary and the package, inspects it, runs `lintian --fail-on error` (the report is
kept), installs it, checks the installed files, smoke-tests `/usr/bin/impro`, removes
the package and asserts that no file remains, then uploads `impro-deb`.
The first run of this workflow on GitHub had not been checked when this page was
written; lintian findings may still need fixes in `packaging/debian/*`. Only Ubuntu
24.04 is tested; Debian 12 and 13 are covered by the alternative package names only.

## Windows

### Layout of the bundle (`packaging/windows/bundle.py`)

`bundle.py` (standard library only) builds one self-contained tree from the release
`impro.exe` and the MSYS2 UCRT64 prefix: the DLL import closure (resolved with
`objdump`; Windows system DLLs are never bundled), compiled GLib schemas, gdk-pixbuf
loaders with a `loaders.cache` using bundle-relative paths, icons, licences and
`bundle-manifest.txt` (sha256, size, path; a size guard fails an unexpectedly large
bundle).

```
impro.exe  *.dll
share/glib-2.0/schemas/gschemas.compiled
lib/gdk-pixbuf-2.0/2.10.0/loaders/*.dll  loaders.cache
share/icons/{Adwaita,hicolor}
licenses/  LICENSE  THIRD_PARTY_NOTICES.md  bundle-manifest.txt
```

The exe and DLLs are flat because GLib and GTK derive their prefix from the directory
of their own DLL, so no environment variables or wrapper are needed. Extra files go
next to the exe with `--extra` (the product job passes `WebView2Loader.dll`, found by
`scripts/package/windows/find-webview2-loader.sh`).

### Installer, zip, checksums

| Step | Tool |
| --- | --- |
| Bundle | `scripts/package/windows/bundle.sh` (bundle, `imports-report.txt`, manifest) |
| Installer | `packaging/windows/impro.iss`, compiled by `scripts/package/windows/build-installer.ps1` (Inno Setup 6) |
| Portable zip | `scripts/package/windows/make-portable-zip.sh` |
| Checksums | `scripts/package/windows/write-checksums.sh` (`SHA256SUMS.txt`) |

`impro.iss` installs per user (`PrivilegesRequired=lowest`, overridable in the dialog)
and defines the tasks `desktopicon`, `contextmenu` and `addtopath`, all off by default.
Release assets: `impro-<version>-x64-setup.exe`, `impro-<version>-x64-portable.zip`,
`SHA256SUMS.txt`.

### Smoke test

`packaging/windows/smoke.ps1` installs the setup executable silently, checks the
installed tree and registry keys, launches `impro.exe` twice (cold and warm) with
`IMPRO_QA_SCREENSHOT` and `IMPRO_QA_TIMING`, fails on a blank screenshot (decode-based
pixel check), uninstalls and checks nothing is left; then the same for the portable
zip. Helper scripts and their tests are listed in `packaging/windows/README.md`.

### Temporary stand-in jobs

`package-standin` and `smoke-standin` in `windows.yml` package a small GTK example
(`crates/impro-ui/examples/packaging_standin.rs`) as `impro.exe`, to separate packaging
problems from application problems. They are `continue-on-error`, not part of
`windows-success`, and upload `windows-standin` and `windows-standin-smoke` (7 days).
Remove them once the real binary packages reliably (list copied from
`reports/windows/win-ci.md`):

- delete `crates/impro-ui/examples/packaging_standin.rs`;
- in `windows.yml`, delete the jobs `package-standin` and `smoke-standin`;
- delete the README section "Packaging stand-in" in `packaging/windows/README.md`;
- delete the stand-in paragraphs of the report.

`--exe-name`, the shared scripts and `-RequireFiles` stay; the product jobs use them.
The startup lab (`startup-lab.ps1`) is also temporary and goes with the stand-in.

### Building a package locally

On Windows, in an MSYS2 UCRT64 shell with the toolchain and GTK packages installed
(see `.github/actions/setup-windows/action.yml` for the package list) and Inno Setup 6:

```sh
cargo build --release --locked -p impro
scripts/package/windows/bundle.sh target/release/impro.exe impro.exe dist/impro dist/out
scripts/package/windows/make-portable-zip.sh dist/impro <version> dist/out
```

then `scripts/package/windows/build-installer.ps1 -Version <version> -BundleDir dist\impro -OutDir dist\out`
from PowerShell, and `write-checksums.sh`. The Windows steps have only been run on
GitHub runners; the exact local invocation is *not yet verified* outside CI.
Unit tests that run on Linux: `python3 -m unittest discover -s packaging/windows -p 'test_*.py' -v`.
