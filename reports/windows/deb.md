STATUS: DONE

## What changed
- `scripts/package/build-deb.sh`: builds `impro_<version>_<arch>.deb` from a binary (`<binary> [--version X.Y.Z] [--out DIR]`, default out `dist/`, default version from the workspace manifest, `SOURCE_DATE_EPOCH` default = HEAD commit time).
- `packaging/debian/control.in` (placeholders `@VERSION@ @ARCH@ @INSTALLED_SIZE@ @DEPENDS@`), `packaging/debian/copyright` (DEP-5: MIT project, CC-BY-SA-4.0 Yaru icons, LGPL-3+ libpanel icon), `packaging/debian/impro.1` (man page, avoids the lintian `no-manual-page` warning).
- `.github/workflows/package-linux.yml`: build, package, inspect, lintian (`--fail-on error`, report kept), install, installed checks, smoke test of `/usr/bin/impro`, remove and assert no file remains, upload artifact `impro-deb` (deb, `impro_SHA256SUMS`, lintian report; 30 days).
- `.gitignore`: `dist/`.

## Package contents
`/usr/bin/impro` (0755), desktop file, hicolor PNGs 16..512 + scalable SVG, `/usr/share/doc/impro/{copyright,changelog.gz,README.md.gz,THIRD_PARTY_NOTICES.md}`, `/usr/share/man/man1/impro.1.gz`. All root:root, 0644 data, 0755 dirs, xz-compressed, no maintainer scripts.

Deviation: the package version is the bare workspace version (a Debian native package), so the changelog is `changelog.gz` (not `changelog.Debian.gz`, which lintian expects only for versions with a revision).

## Depends
Derived from the binary's direct `NEEDED` entries (not the transitive `ldd` closure; those come through the owning packages), resolved with `ldd` and mapped with `dpkg -S` (built on Ubuntu 26.04 here, 24.04 in CI):

| soname | package (dpkg -S) | Depends entry |
| --- | --- | --- |
| libgtk-3, libgdk-3 | libgtk-3-0t64 | `libgtk-3-0t64 \| libgtk-3-0` |
| libglib-2.0, libgobject-2.0, libgio-2.0 | libglib2.0-0t64 | `libglib2.0-0t64 \| libglib2.0-0` |
| libatk-1.0 | libatk1.0-0t64 | `libatk1.0-0t64 \| libatk1.0-0` |
| libpango-1.0 / libpangocairo-1.0 | libpango-1.0-0 / libpangocairo-1.0-0 | same |
| libcairo / libcairo-gobject | libcairo2 / libcairo-gobject2 | same |
| libgdk_pixbuf-2.0 | libgdk-pixbuf-2.0-0 | same |
| libwebkit2gtk-4.1, libjavascriptcoregtk-4.1 | libwebkit2gtk-4.1-0, libjavascriptcoregtk-4.1-0 | same |
| libvte-2.91 | libvte-2.91-0 | same |
| libgcc_s, libc, libm | libgcc-s1, libc6 | same |

Plus `librsvg2-common, adwaita-icon-theme, gsettings-desktop-schemas, shared-mime-info`. Recommends: `git`. No version constraints. The t64 renames are in an explicit table in the script (with a generic `*t64` fallback that warns).

Resulting line (local build):
`adwaita-icon-theme, gsettings-desktop-schemas, libatk1.0-0t64 | libatk1.0-0, libc6, libcairo-gobject2, libcairo2, libgcc-s1, libgdk-pixbuf-2.0-0, libglib2.0-0t64 | libglib2.0-0, libgtk-3-0t64 | libgtk-3-0, libjavascriptcoregtk-4.1-0, libpango-1.0-0, libpangocairo-1.0-0, librsvg2-common, libvte-2.91-0, libwebkit2gtk-4.1-0, shared-mime-info`

`dpkg-shlibdeps` cross-check (printed by the script): the same package set, except it lacks the four extras (not linked) and adds version constraints (e.g. `libglib2.0-0t64 (>= 2.64.0)`, `libc6 (>= 2.39)`, which would exclude Debian 12) and uses t64 names only. Ours is deliberately looser.

## Verification
- Release binary: the main checkout's `target/release/impro` (built 24 Oct). `scripts/package/build-deb.sh <that> ` -> `dist/impro_0.1.0_amd64.deb` (2719056 bytes).
- Byte-identical: two builds both `90ea787ff06ad5b072ee23e8de0484c65e9ebec6d6fbd63166cbbfff85ff1338`; with one byte appended to the binary: `0df08634025b12cb4c9bdccafed8b177d376099add3cfd315a389c93db115c73`.
- `dpkg-deb --info/--contents`: root/root, modes as listed above, files only under `/usr`. `dpkg-deb -x` + `desktop-file-validate`: passes (one hint about adding the `Utility` category; the desktop file is not owned by this task). All 8 PNGs have their declared pixel size (read from the IHDR). Copyright starts with the DEP-5 `Format:` header; man page renders.
- `bash -n` and `shellcheck` (v0.11.0) clean on the script. `actionlint` (shellcheck on PATH): clean. `zizmor --offline --min-severity medium .github`: no findings (the one low `self-repository` note is the same as for ci.yml).
- Action SHAs verified with `gh api repos/<owner>/<repo>/git/ref/tags/<tag>`: checkout v7.0.1 = 3d3c42e..., upload-artifact v7.0.2 = cf430e0... (both as in ci.yml).
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`: both clean (clippy finished with no warnings). Forbidden-word, local-path and `git diff --check` gates clean.

## Open issues / assumptions
- `lintian` is not installed locally and not run; CI runs it (`--fail-on error`). The `--fail-on` option needs lintian >= 2.115 (24.04 ships 2.117). First CI run may surface warnings/errors to fix in `packaging/debian/*` (e.g. `binary-or-shlib-defines-rpath`, `hardening-*`, the native-package changelog name).
- Tested distros: built on Ubuntu 26.04 locally, CI targets Ubuntu 24.04 only. Debian 12 and 13 are covered only by the alternative names; not tested.
- The workflow was not run on GitHub; install, smoke and removal steps are untested end to end (never ran `dpkg -i` locally, by instruction).
- `Installed-Size` is the sum of file sizes (KiB, rounded up).
