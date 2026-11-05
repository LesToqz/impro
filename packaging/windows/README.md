# Windows packaging

Target `x86_64-pc-windows-gnu`, GTK3 and Rust from MSYS2 UCRT64.

| File | Purpose |
| --- | --- |
| `bundle.py` | Builds the self-contained tree (exe, DLL import closure, GLib schemas, gdk-pixbuf loaders + cache, icons, licences, `bundle-manifest.txt`). Stdlib only. |
| `impro.iss` | Inno Setup 6 script. Per-user by default; registers the integration described in "Windows integration" below. |
| `smoke.ps1` | Installs/unzips the packages, asserts the integration, tests the installed app (and captures its real window), uninstalls and checks nothing is left. |
| `window-capture.ps1` | Best-effort capture of the real top-level window of the running app, native frame included (used by `smoke.ps1`). |
| `probe-log.ps1`, `print-probe-summary.ps1` | `Write-LogLine` (host plus `window-probe.txt`) and the last step of the `smoke` job, which prints that file framed by `===== WINDOW PROBE SUMMARY BEGIN/END =====`. |
| `window-probe.ps1`, `probe-vectors.json`, `test_window_probe.ps1`, `test_window_probe.py` | The window probe (`[window-probe] key=value` lines about the real window and a few pixels of its capture), the shared test vectors of its pure helpers and the tests. See "Window probe". |
| `print-screenshots.ps1`, `screenshot-log.ps1`, `test_screenshot_log.ps1` | Print `desktop-window.png` and one `first-paint*.png` (capped at 700 px width and 60 KB of base64) into the job log between marker lines, its helpers and their test. See "Screenshots in the job log". |
| `smoke-args.ps1`, `test_smoke_args.ps1` | Argument quoting for the Inno installer/uninstaller (a value with a space must be quoted by hand for `Start-Process`) and its plain-assert test, run in the `check` job. |
| `lab-helpers.ps1`, `test_lab_helpers.ps1` | Pure helpers (timing-line parser, Markdown table, decode-based blank-screenshot check, line limiter) shared by `smoke.ps1` and `startup-lab.ps1`, plus their test (run in the `check` job). |
| `startup-lab.ps1` | TEMPORARY startup experiment, see "Startup lab" below. |
| `test_bundle.py`, `test_iss.py` | Unit tests. `test_iss.py` is a static check of `impro.iss` (directive, parameter and flag names, `{constants}`, balanced Pascal blocks, Check functions, every registration listed below, and the contract with `smoke.ps1`); it also runs in the `check` job. The real-DLL tests need a full sysroot (`scripts/windows/fetch-sysroot.sh --full`) and `objdump`. |

Run the tests: `python3 -m unittest discover -s packaging/windows -p 'test_*.py' -v`

## Windows integration (installer)

`impro.iss` makes impro behave like other Windows software. Per-user is the default (`PrivilegesRequired=lowest`, no
administrator rights); the first wizard page can switch to "all users", in which case `HKA` means `HKLM`. The script can
only be compiled in CI (Inno Setup 6, `scripts/package/windows/build-installer.ps1`).

| Registration | When | Details |
| --- | --- | --- |
| Start Menu shortcut `impro` | always | Starts in the user profile folder (`WorkingDir: {%USERPROFILE}`): the app treats a start from there as an "ambient launch" and reopens the last folder, so the install directory must not be the working directory. Carries the `AppUserModelID` `dev.impro.impro` (equal to `impro_core::identity::APP_ID`, which the app sets on its process) so that pinning and taskbar grouping agree. |
| Desktop shortcut | task `desktopicon`, **off** by default | Same working directory and AppUserModelID. |
| "Open folder in impro" | task `contextmenu`, **on** | `Software\Classes\Directory\shell\impro` (folders, argument `%1`) and `Directory\Background\shell\impro` (folder background, `%V`). |
| "Open with impro" | task `filecontextmenu`, **on** | `Software\Classes\*\shell\impro` on every file, argument `%1`. On Windows 11 the entries are under "Show more options", like those of other classic applications. |
| `PATH` | task `addtopath`, **on** | Per-user installs edit `HKCU\Environment`, all-users installs the system environment. The directory is added only when it is not already an entry (an upgrade in place never duplicates it) and removed by the uninstaller (`CurUninstallStepChanged`, step `usPostUninstall`). |
| App Paths | always | `Software\Microsoft\Windows\CurrentVersion\App Paths\impro.exe` (default value = full path, `Path` = install directory): `Win+R` then `impro` works even without a `PATH` entry. |
| "Open with" list | always | `Software\Classes\Applications\impro.exe` with `FriendlyAppName`, `shell\open\command` and `SupportedTypes` for common text and source extensions. This only offers impro in the "Open with" dialog. impro is **never** registered as a default handler: no extension key, `UserChoice`, `Capabilities` or `RegisteredApplications` entry is written (`test_iss.py` enforces it). |
| Uninstall entry | always | Name, version, publisher, support and update links (Settings, Apps). |
| Finish page notes | wizard only | The Git for Windows note (no `git.exe` on `PATH`) and a WebView2 note when the Evergreen runtime is missing (`pv` value under `EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}` in `HKLM\SOFTWARE\WOW6432Node` or `HKCU`; absent or `0.0.0.0` counts as missing). The notes only extend the label; nothing can block the installation. |

Behaviour of the installer itself: one `AppId` for every version (a newer setup upgrades in place), `SetupMutex` (one
setup at a time), `CloseApplications=yes` with `CloseApplicationsFilter=impro.exe` (a running impro is closed through
the Restart Manager before files are replaced or removed), `UsePreviousTasks=yes` (an upgrade starts from the tasks
chosen last time), `MinVersion=10.0` (Windows 10 or newer; the documented 1809 floor is not enforced), version resources (`VersionInfo*`) and the Apps &
features metadata. The launch checkbox of the finish page starts impro in the profile folder, without elevation.

The uninstaller removes exactly what the installer wrote. It leaves the user's settings in `%APPDATA%\impro` untouched on
purpose, so that a reinstall finds them; `smoke.ps1` asserts that. A shortcut working directory is expanded at install
time, so in an all-users install performed through another administrator account it is that account's profile; the
per-user default is unaffected.

Silent installation (all standard Inno Setup switches):

```
impro-<version>-x64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CURRENTUSER /DIR="C:\Tools\impro" /LOG="setup.log"
```

Without `/TASKS` the defaults above apply (folder and file context menus and `PATH` on, desktop icon off).
`/TASKS=desktopicon,contextmenu,filecontextmenu,addtopath` selects exactly the listed tasks, `/MERGETASKS="desktopicon"`
adds to the defaults and `/MERGETASKS="!addtopath"` removes one of them. `/ALLUSERS` installs for all users (needs
administrator rights).

## Smoke test of the installer

`smoke.ps1 -Mode installer` runs two rounds (both silent, per user):

1. **Defaults** (no `/TASKS`, what a user gets by clicking Next): asserts the integration, runs the same setup again over
   the install (upgrade in place: still exactly one `PATH` entry, all registrations intact), uninstalls once and asserts that
   everything is gone. A failure here is collected and reported at the end, so round 2 still produces its evidence.
2. **All tasks** (`/TASKS=desktopicon,contextmenu,filecontextmenu,addtopath`): the same assertions plus the desktop shortcut,
   then the tests of the installed app (`--version`, `--help`, bad folder, two launches with the first-paint PNG, and the
   real window capture), then the uninstall: files, both shortcuts, the uninstall key, the context-menu, App Paths and
   Applications keys and the `PATH` entry must be gone, while a marker file in `%APPDATA%\impro` must still exist.

Asserted after an install: the Start Menu shortcut targets the installed `impro.exe` and its `WorkingDirectory` equals the
user profile (read through `WScript.Shell`); the `AppUserModelID` is found in the `.lnk` bytes (a warning if not, because
reading it back needs COM interop); App Paths; the Applications key (open command, `SupportedTypes`); the three
context-menu keys with their command lines; the user `PATH`; and the uninstall entry (name, version, publisher, help link,
install location). `test_iss.py` checks that every registry key `smoke.ps1` names is written by `impro.iss`.

## Window probe (facts about the real window)

Right after the desktop capture, while the window still exists, `smoke.ps1` (installer mode) calls `Write-WindowProbe`
(`window-probe.ps1`). It prints small structured facts, one per line, so that they can be read straight from the job log:

```
[window-probe] key=value
```

Best effort: each fact is read in its own `try`/`catch` and a failure is printed as `key=error(<message>)`; the probe never
fails the step. Search the log for `[window-probe]`.

The job log of the `smoke` job is long and can only be read from its end, so `smoke.ps1` also writes every
`[window-probe]` line and the desktop capture status lines (`[smoke] desktop capture ...`) to `<OutDir>\window-probe.txt`
(exactly the text written to the host, see `probe-log.ps1`; `print-screenshots.ps1` appends its `[screenshots]` lines to the
same file). The file target is a global variable (`Set-ProbeLogFile` / `Get-ProbeLogFile`): a script that sets it for a
while must restore the old value, because the test step of the `check` job runs several scripts in one PowerShell process
(a stale path made the next script print an extra warning line); an unwritable target is reported once and then switched off. The LAST step of the `smoke` job, `Window probe summary` (`print-probe-summary.ps1`, `if: always()`,
`continue-on-error: true`), prints the names and sizes of the files in the smoke output directory and then the file between

```
===== WINDOW PROBE SUMMARY BEGIN =====
...
===== WINDOW PROBE SUMMARY END =====
```

or a clear "window-probe.txt is missing" line between the same markers. The END marker is the last line of the job log.

| Keys | Meaning |
| --- | --- |
| `os_version`, `os_build`, `os_build_ubr`, `os_display_version`, `os_product` | Windows version and build number. |
| `hwnd`, `pid`, `process_path`, `title`, `class` | The window found by the capture (the largest visible unowned window of an `impro` process): handle, process id, `GetWindowText`, window class name. |
| `style`, `exstyle` | `GWL_STYLE` / `GWL_EXSTYLE` as `0x%08X`. |
| `ws_caption`, `ws_thickframe`, `ws_sysmenu`, `ws_minimizebox`, `ws_maximizebox`, `ws_visible` | `true`/`false` for the style bits (`ws_caption` needs both `WS_BORDER` and `WS_DLGFRAME`). No `ws_caption` means client-side decorations. |
| `window_rect`, `window_size`, `client_size`, `client_origin_screen` | `GetWindowRect` as `left,top,right,bottom`; sizes as `WxH` in device pixels; the client origin on the screen. |
| `dwm_frame_bounds`, `dwm_frame_size`, `dwm_frame_hr` | `DWMWA_EXTENDED_FRAME_BOUNDS` (the visible frame, the rectangle of the capture) with its HRESULT. |
| `nonclient_top_px` | Client origin minus visible frame top: caption plus top border. |
| `dpi_window`, `dpi_source`, `dpi_scale` | `GetDpiForWindow` (fallback `GetDeviceCaps(LOGPIXELSX)`), where it came from, DPI/96. |
| `dpi_awareness_window`, `dpi_per_monitor_v2_window`, `dpi_awareness_process`, `dpi_awareness_process_hr` | `unaware`, `system_aware` or `per_monitor_aware` with the raw value, from `GetAwarenessFromDpiAwarenessContext` and `GetProcessDpiAwareness`. |
| `dwm_immersive_dark_mode`, `dwm_window_corner_preference`, `dwm_border_color`, `dwm_caption_color`, `dwm_text_color` | `DwmGetWindowAttribute` read-backs (20, 33, 34, 35, 36), each followed by `<key>_hr` with the HRESULT. Colours are `#RRGGBB` (a COLORREF is `0x00BBGGRR`); `0xFFFFFFFF` is "default" and `0xFFFFFFFE` "none". Corner preference is `2(round)` style. `n/a` when the HRESULT is a failure (older Windows). |
| `app_user_model_id`, `app_user_model_id_hr`, `app_user_model_id_vartype`, `app_user_model_id_matches` | `System.AppUserModel.ID` from the window's property store (`SHGetPropertyStoreForWindow`) and whether it equals `dev.impro.impro`. The property store shows a per-window ID; a process-level ID (`SetCurrentProcessExplicitAppUserModelID`) may read back `(empty)`. |
| `wm_geticon_small`, `wm_geticon_big`, `wm_geticon_small2` (+ `_nonzero`), `class_icon_big`, `class_icon_small` | `WM_GETICON` answers (an `HICON` or 0) and the window class icons. |
| `caption_height_px`, `caption_height_source` | Title bar height in device pixels: `client_origin`, `rect_difference`, or `default` (16 px at 96 DPI, scaled) when the window has no `WS_CAPTION`. |
| `bitmap_size`, `pixel(<name>)`, `pixel_pos(<name>)` | Size of the capture and the colour (`#RRGGBB`) and position of a few pixels of it. Offsets are in device pixels at 96 DPI, scaled by DPI/96; `x` and `y` are bitmap coordinates. |
| `done` | Printed last; its absence means the probe stopped early (a `::warning::` says why). |

Pixel names: `caption_center` (x = width/2, y = caption/2), `below_caption_x60` and `below_caption_center` (6 px below
the caption, the docked bar), `sidebar_body` (x = 150, y = caption + 120), `editor_center` (centre of the area under the
caption) and `bottom_right` (12 px in from the corner). A position outside the bitmap prints `n/a(outside the bitmap)`.

The pure helpers (COLORREF to `#RRGGBB`, style bits, DPI scaling of offsets, caption height, probe points, line format) are
tested without Windows: `test_window_probe.ps1` (in the `check` job) runs them against `probe-vectors.json`, and
`test_window_probe.py` checks the same vectors against an independent Python implementation and pins the Win32 constants.

## Screenshots in the job log

The dev machine can read a job log but cannot download artifacts. The base64 is bulky (it buries the rest of the log), so
the step "Print screenshots to the log" of the `smoke` job is **opt-in**: it runs only for a `workflow_dispatch` run with the
boolean input `print_screenshots` set to true (default false), with `continue-on-error: true`. The code and the extractor
stay; the structured facts of the window probe (previous section) are the default evidence. When enabled, the step prints:

```
-----BEGIN IMPRO-SCREENSHOT <file name>-----
<base64, 100 columns>
-----END IMPRO-SCREENSHOT <file name>-----
```

`print-screenshots.ps1 -Directory DIR` keeps the log small: only `desktop-window.png` and ONE `first-paint*.png` (the first
in name order, normally the cold installer launch) are considered. Each is downscaled to at most 700 px width (`-MaxWidth`)
and skipped when its base64 text would still exceed 60 KB (`-MaxBase64KB`). Every other PNG gets one line,
`[screenshots] skipped <name> (<size>)`. The structured facts of the window probe (next section) are the primary evidence;
the base64 blocks are a bonus for whoever can decode them.

The files are `first-paint-<mode>-<cold|warm>.png` (the offscreen GTK capture, `IMPRO_QA_SCREENSHOT`, content only) and
`desktop-window.png`: `smoke.ps1` starts the installed app on a folder without the QA variables, waits for its main
window, brings it to the front and copies that rectangle off the screen with the native caption and frame
(`Graphics.CopyFromScreen`, DWM frame bounds, DPI aware). `PrintWindow` is the fallback, written as
`desktop-window-printwindow.png` when the screen copy stays blank. If no window appears the step only logs a warning.

To get the files back from a saved log (any OS, standard library only):

```
gh run view <run-id> --job <job-id> --log > job.log      # or save the raw log from the web page
python3 -I scripts/ci/extract_log_screenshots.py -o screenshots job.log
```

The extractor ignores the timestamp prefix, `\r`, a BOM, ANSI colours and the `job<TAB>step<TAB>` columns of `gh`, checks
the PNG magic bytes (and the chunk CRCs, as a warning) and never writes outside the output directory. Its tests:
`python3 -I scripts/ci/test_extract_log_screenshots.py`. The `windows.yml` workflow also runs on pushes to `claude/**`
branches, so the screenshots are available without opening a pull request.

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

## Startup lab

Question: the first launch of the installed app needs several seconds more than later ones. Which one-time cost is it?
`startup-lab.ps1` installs the setup exe and launches `impro.exe` in a fresh process per row, with
`IMPRO_QA_TIMING=1`. It runs FIRST in `smoke-standin` (so row 1 is the machine-cold start) and last, report-only, in `smoke`.
Results: job log, `$GITHUB_STEP_SUMMARY` table, and `windows-standin-smoke` / `windows-smoke` artifacts (`lab/`).

| # | Run | What it tells |
| --- | --- | --- |
| 1 | default (cold) | The cost to explain. |
| 2 | default (warm) | The floor once one-time work is done. |
| 3 | `PANGOCAIRO_BACKEND=win32` | Pango on GDI/DirectWrite instead of fontconfig: big drop => fontconfig font-cache build. |
| 4 | `DBUS_SESSION_BUS_ADDRESS=disabled:` | GIO no longer looks for a session bus: drop => D-Bus autolaunch lookup. |
| 5 | `GSETTINGS_BACKEND=memory` | No registry/dconf backend: drop => settings backend initialisation. |
| 6 | `FC_DEBUG=1` | First 40 output lines kept (`lab-*.first-lines.txt`): which fonts/cache dirs fontconfig scans. |
| 7 | cache listings | `cache-before-first-launch.txt` / `cache-after-first-launch.txt` (`%LOCALAPPDATA%`, `%LOCALAPPDATA%\fontconfig`, `%TEMP%`, bundle `var/cache`, `fc-cache -v` when shipped). |
| 8 | default, fresh profile dirs | What `smoke.ps1` does (new HOME/LOCALAPPDATA per run): cold again => a per-profile cache. |
| 9 | default (warm, repeated) | Drift check: should match row 2. |

Reading it: rows 3-5 and 8 are compared with row 2 (warm), not row 1. A variant is the cause when it is as slow as row 1
only after being applied to a cold start, so also compare row 1 with `app-gtk-startup-ms` (time to GTK init) versus
`first-paint-ms`: a cold-only gap inside `app-gtk-startup-ms` points at fontconfig/D-Bus/schemas, a gap after it at
rendering/themes. Remove the lab together with the stand-in.
