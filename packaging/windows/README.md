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

Extra DLLs (for example `WebView2Loader.dll`) go next to the exe with `bundle.py --extra <file>...`.

Release assets: `impro-<version>-x64-setup.exe`, `impro-<version>-x64-portable.zip`, `SHA256SUMS.txt`.
CI entry point: `.github/workflows/windows.yml`; shared setup: `.github/actions/setup-windows`;
helper scripts: `scripts/package/windows/`.
