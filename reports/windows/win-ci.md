# win-ci report

STATUS: DONE (everything that can be checked on Linux is checked; the Windows run itself is UNVERIFIED, see below)

## What changed
- `.github/workflows/windows.yml` — jobs `check` (required), `test` (report-only, `continue-on-error`), `package`, `smoke` (needs `package`), `windows-success` (aggregator over check/package/smoke).
- `.github/actions/setup-windows/action.yml` — composite: disk log, `msys2/setup-msys2` (UCRT64, `update`, `cache`), packages, toolchain/MSRV verification, `Swatinem/rust-cache`.
- `packaging/windows/`: `bundle.py`, `impro.iss`, `smoke.ps1`, `test_bundle.py`, `test_iss.py`, `README.md`.
- `scripts/package/windows/`: `workspace-version.sh`, `make-portable-zip.sh`, `write-checksums.sh`, `imports-report.sh`.
- No wizard images (skipped by design). No crates, Cargo files or other workflows touched.

## Decisions and findings
- Runner `windows-2025` exists (runner-images repo has `Windows2025-Readme.md`). That image already has **Inno Setup 6.7.1** and PowerShell 7.6; the workflow looks for `ISCC.exe` and only falls back to `choco install innosetup --version 6.7.1`; it fails explicitly if still missing. Chocolatey has 6.7.1 (checked).
- MSYS2 UCRT64 `mingw-w64-ucrt-x86_64-rust` is 1.99.1 (same as `RUST_VERSION` in ci.yml), host `x86_64-pc-windows-gnu`, so no rustup fallback. The setup action fails with a clear message if the host triple is wrong or rustc < 1.88.
- `setup-msys2` v2.33.0 inputs confirmed from its README: `msystem`, `update`, `install`, `cache` (default true). Packages are cached by the action.
- `CARGO_HOME` is set at workflow level to `D:\cargo-home` (MSYS2 `HOME` differs from the Windows profile, so rust-cache would otherwise miss the registry). Runner workspace is already on D:. Cache is saved only on `main`. Setting env through `GITHUB_ENV` in the composite was dropped because zizmor flags it.
- **Runtime layout (no env vars, no wrapper):** MSYS2's gdk-pixbuf ships no `loaders.cache` (pacman hook creates it) and its compiled-in module dir is `/ucrt64/lib/gdk-pixbuf-2.0/2.10.0/loaders`; GLib/GTK derive their prefix from the directory of their DLL. So the bundle keeps `impro.exe` and all DLLs in the root with `share/` and `lib/` beside them. `bundle.py` runs `gdk-pixbuf-query-loaders` on the bundled loaders and rewrites module paths to `lib\gdk-pixbuf-2.0\2.10.0\loaders\...` (relative). The SVG loader in this MSYS2 version is `pixbufloader_svg.dll` (no `lib` prefix); it is handled because all `*.dll` in the loader dir are used. **UNVERIFIED:** that gdk-pixbuf resolves relative cache paths against its own prefix on Windows; the smoke test fails if the app output contains "Unrecognized image file format", "Settings schema ... is not installed" or "Failed to load module", which is how a wrong layout would show. If it fails, the fix is to set `GDK_PIXBUF_MODULE_FILE` from the app (not a wrapper) or write absolute paths at install time.
- Bundle size from the real sysroot with a stand-in exe (libgtk-3-0.dll as exe): 46 DLLs, 985 files, 54.7 MB (limit 400 MB). A real impro build adds its own imports.
- Unresolved non-system imports are only logged by `bundle.py`; `smoke.ps1` then checks the `imports-report.txt` against the installed dir and System32 and fails with the missing names.

## Verification (Linux)
- `python3 -m unittest discover -s packaging/windows -p 'test_*.py' -v` → `Ran 14 tests ... OK`. (The literal form `python3 -m unittest packaging/windows/test_bundle.py` fails on this Python because `packaging` is not importable as a package path; use `discover` or `cd packaging/windows`.) Tests:
  - ClosureTests: transitive/cycles/case-insensitive, system DLLs never bundled even if present in the prefix, unresolved skipped, `is_system_dll`.
  - ManifestTests: manifest format (sha256, size, path) + size guard. LoadersCacheTests: absolute Windows/POSIX paths → relative.
  - RealSysrootTests (**ran**, full sysroot in `~/.cache/impro-win-sysroot/current-full`, host `objdump`): libgtk-3-0.dll closure, full bundle with a fake exe, size guard trips.
  - test_iss: required `[Setup]` keys, license/icon files exist, tasks default off, context-menu keys have `uninsdeletekey`, and every `[Files]` Source resolves in a bundle built from the sysroot (**ran**).
- `actionlint` (v1.7.12 in `.tools`, with shellcheck on PATH) → clean. `zizmor --min-severity medium --no-progress .github` → "No findings". `shellcheck scripts/package/windows/*.sh` → clean. All YAML parses.
- `pwsh` and `iscc` are not available locally: `smoke.ps1` and the inline pwsh in the workflow were NOT syntax-checked or run; `impro.iss` was only checked by reading and by `test_iss.py`. Inline `shell: msys2` blocks are not covered by actionlint's shellcheck.
- Pins (each `uses:` is a full SHA with a version comment; verified with `gh api repos/<r>/git/ref/tags/<tag>`, dereferencing annotated tags):
  - OK actions/checkout v7.0.1 -> 3d3c42e5aac5ba805825da76410c181273ba90b1
  - OK actions/download-artifact v8.0.2 -> 9000827ccba6bdab643e8b6fd33ac0654aef8333
  - OK actions/upload-artifact v7.0.2 -> cf430e030ddbb5b0abf93d22962f4752f3646cd9
  - OK msys2/setup-msys2 v2.33.0 -> ec48f7c5447b3140e2b088413ae3a55687bccb6e
  - OK Swatinem/rust-cache v2.9.2 -> 6323deb102c322ba6fcbdcafc7e3dddab59af2b6
- Gates: `cargo fmt --all --check` ok; `cargo clippy --workspace --all-targets -- -D warnings` ok; `git diff --check` ok; forbidden-word grep over tracked files empty for my files (existing `/home/dev/...` strings in two gitviews JSON fixtures predate this task and are not paths of a person); commit messages clean. No Rust changes, so no Linux test run was needed.

## Runbook (order on the runner) and what to look at first
1. **check** — checkout; setup-windows (disk log, MSYS2 install/update, "Verify toolchain" prints rustc/pkg-config gtk version); `cargo check --workspace --all-targets --locked`; clippy `-D warnings`. If it fails: (a) "Verify toolchain" step → MSYS2 rustc too old/wrong host; (b) pacman step → package name/mirror; (c) cargo → the Windows port of the crates is incomplete (other tasks), or `pkg-config` cannot find a `.pc` (look for the missing library name, add the `mingw-w64-ucrt-x86_64-*` package to `install:`).
2. **test** (report-only) — logic crates, then all crates with `--no-fail-fast`. Failures never fail the workflow.
3. **package** — version from `Cargo.toml`; `cargo build --release -p impro`; `bundle.py` (log lines `bundle: ...`, DLL count, total size, "note: import not found" lines); `imports-report.txt`; portable zip; ISCC; checksums; upload `impro-windows`. First look: "Bundle GTK runtime" for `objdump`/`glib-compile-schemas`/`gdk-pixbuf-query-loaders` errors (their stderr is captured by Python's CalledProcessError); "Installer" step for `ISCC` output (error line numbers refer to `impro.iss`); `target/release/impro.exe` missing means the build step wrote elsewhere.
4. **smoke** — download artifact; `smoke.ps1 -Mode installer` then `-Mode portable`. Failures print a "crash evidence" group: app stdout/stderr, Application event log IDs 1000/1001, WER files, and a per-import DLL lookup. Artifact `windows-smoke` holds `install.log`, `uninstall.log`, `*.stdout.txt/*.stderr.txt` and `first-paint-*.png`. First look: `install.log` for installer errors; "unresolved DLL imports" for a missing DLL; for `--version` printing nothing see below.
5. **windows-success** — fails if check/package/smoke is not `success`.

UNVERIFIED until a real run: everything on Windows. Highest-risk items:
- `--version`/`--help` output of the GUI-subsystem exe through redirected pipes (smoke.ps1 uses .NET `Process` with redirected stdout/stderr, not `&`, which would not wait for a GUI exe). If stdout is empty, the app's console re-attach ignores inherited handles.
- relative paths in `loaders.cache` (see above); `first-paint-ms` line may not reach the pipe (only a warning).
- Inno behaviour: `/CURRENTUSER` + `/TASKS=desktopicon,contextmenu,addtopath` silent install, PATH removal in `CurUninstallStepChanged`, finish-page Git note, uninstaller exit (smoke polls up to 60 s for the dir to disappear because `unins000.exe` relaunches itself).
- `shell: msys2 {0}` inside the composite action and `cygpath -u "$GITHUB_OUTPUT"` writes; `python` on the MSYS2 PATH taking `C:/...` prefix from `cygpath -m`.
- Disk: workspace and `CARGO_HOME` are on D:; free space is logged at the start of every job.

## Open issues / assumptions
- `cargo test --workspace` in the report-only step will run GUI tests on the Windows runner; some may fail until the port is complete.
- The `smoke` job asserts the installer's uninstall key under HKCU, so it relies on the per-user install (`/CURRENTUSER`).
- `WebView2Loader.dll`/terminal helper: pass with `bundle.py --extra`; the workflow does not pass any yet (a later task edits the "Bundle GTK runtime" step).
- The README `Release assets` names match what `write-checksums.sh` expects (setup exe + portable zip).
