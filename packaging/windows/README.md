# Windows packaging

Target `x86_64-pc-windows-gnu`, GTK3 and Rust from MSYS2 UCRT64.

| File | Purpose |
| --- | --- |
| `bundle.py` | Builds the self-contained tree (exe, DLL import closure, GLib schemas, gdk-pixbuf loaders + cache, icons, licences, `bundle-manifest.txt`). Stdlib only. |
| `impro.iss` | Inno Setup 6 script. Per-user by default, tasks for desktop icon, Explorer context menu and PATH (all off by default). |
| `smoke.ps1` | Installs/unzips the packages, tests the installed app, uninstalls and checks nothing is left. |
| `test_bundle.py`, `test_iss.py` | Unit tests. The real-DLL tests need a full sysroot (`scripts/windows/fetch-sysroot.sh --full`) and `objdump`. |

Run the tests: `python3 -m unittest discover -s packaging/windows -p 'test_*.py' -v`

Bundle layout (flat for the exe and DLLs, because GLib/GTK derive their prefix from the directory of
their own DLL; no environment variables and no wrapper are needed):

```
impro.exe  *.dll
share/glib-2.0/schemas/gschemas.compiled
lib/gdk-pixbuf-2.0/2.10.0/loaders/*.dll  loaders.cache   (paths relative to the bundle)
share/icons/{Adwaita,hicolor}
licenses/  LICENSE  THIRD_PARTY_NOTICES.md  bundle-manifest.txt
```

Extra DLLs go next to the exe with `bundle.py --extra <file>...` (absolute paths in either spelling, `cygpath -m`
style `C:/dir with spaces/x.dll` included). `--exe-name NAME` names the executable inside the bundle (default: basename of
`--exe`). The product job passes `WebView2Loader.dll`, located by `scripts/package/windows/find-webview2-loader.sh`
(exactly one distinct file below `target/release/build`, sha256 logged, a changed hash only warns); `smoke.ps1
-RequireFiles WebView2Loader.dll` asserts it is installed next to `impro.exe`.

Release assets: `impro-<version>-x64-setup.exe`, `impro-<version>-x64-portable.zip`, `SHA256SUMS.txt`.
CI entry point: `.github/workflows/windows.yml`; shared setup: `.github/actions/setup-windows`;
helper scripts: `scripts/package/windows/`.

## Packaging stand-in (TEMPORARY)

`crates/impro-ui/examples/packaging_standin.rs` is a small GTK program that obeys the `impro.exe` contract of
`smoke.ps1` (`--version`, `--help`, exit 2 for a bad folder, `IMPRO_QA_SCREENSHOT`, `IMPRO_QA_TIMING`, console re-attach) and
probes every bundled runtime piece, printing `STANDIN-CHECK <name> ok|skipped|FAIL` lines. The workflow jobs
`package-standin` and `smoke-standin` (both `continue-on-error`, not part of `windows-success`) bundle it as `impro.exe`
and run the same installer, zip, checksum and smoke steps; artifacts are `windows-standin` and `windows-standin-smoke`.
On Linux the exe-relative checks are skipped unless `IMPRO_STANDIN_BUNDLE_CHECK=1`.

Remove it when the real binary packages: see "Continuation 1" in `reports/windows/win-ci.md`.

Shared scripts (`scripts/package/windows/`): `bundle.sh` (bundle + imports report + manifest), `build-installer.ps1`
(ISCC with diagnostics), `find-webview2-loader.sh`, `print-diagnostics.sh`, plus the existing zip/checksum helpers.
