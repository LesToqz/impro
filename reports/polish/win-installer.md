STATUS: DONE (nothing here could be compiled or run on Windows; see "Not verified")

Branch: `worktree-agent-a37050d2894154d2d`. Scope: installation and integration on Windows plus the CI evidence. The app
window itself is owned by other agents.

## What changed

| File | Change |
| --- | --- |
| `packaging/windows/impro.iss` | Shortcuts start in the user profile and carry `AppUserModelID` `dev.impro.impro`; tasks `contextmenu`, new `filecontextmenu` ("Open with impro", `Software\Classes\*\shell\impro`) and `addtopath` are checked by default, `desktopicon` stays off; App Paths key; `Applications\impro.exe` with `FriendlyAppName`, open command and 47 `SupportedTypes`, no default handler; `AppPublisher`/`AppSupportURL`/`AppUpdatesURL`/`AppCopyright`, `VersionInfoCompany`/`Description`/`ProductName`/`Copyright`, `SetupMutex`, `CloseApplicationsFilter=impro.exe`, `UsePreviousTasks=yes`, `ChangesAssociations=yes`, `MinVersion=10.0`; PATH now written to the right key for both install modes (see below); "Launch impro" starts in the profile folder with `runasoriginaluser`; WebView2 finish-page note next to the Git note; header comment documents everything including that `%APPDATA%\impro` is left alone. |
| `packaging/windows/test_iss.py` | 37 tests (was 5), 36 of them static checks of the script: directive, parameter and flag whitelists per section, `{constants}`, preprocessor symbols, balanced Pascal blocks, Check functions defined, each registration above, "never a default handler", every key removed on uninstall, PATH entries and removal code, WebView2 keys and URLs, notes cannot block, and the contract with `smoke.ps1` (tasks passed to `/TASKS`, registry keys it asserts). |
| `packaging/windows/smoke.ps1` | Two installer rounds (task defaults installed twice = upgrade in place, then all tasks with the app tests), the new assertions of Part 2, `%APPDATA%\impro` survival marker, deferred failure of round 1, desktop window capture at the end of `Test-App` (installer mode). |
| `packaging/windows/window-capture.ps1` (new) | `EnumWindows` P/Invoke, DWM frame bounds, `Graphics.CopyFromScreen`, `PrintWindow` fallback; compiled lazily, never throws. |
| `packaging/windows/print-screenshots.ps1`, `screenshot-log.ps1`, `test_screenshot_log.ps1` (new) | Print PNGs into the log between `-----BEGIN/END IMPRO-SCREENSHOT <name>-----`, base64 at 100 columns, downscale to 1100 px width above 250 KB; helper and round-trip test (the downscale case runs on Windows only). |
| `packaging/windows/lab-helpers.ps1`, `test_lab_helpers.ps1` | `Test-SamePath` (8.3 names, case, slashes), `Test-ByteSequence`, with tests. |
| `packaging/windows/startup-lab.ps1` | After its uninstall it waits for the directory to go (the lab now installs with the task defaults, and the uninstaller relaunches itself). |
| `packaging/windows/README.md` | Windows integration table, silent switches (`/TASKS` vs `/MERGETASKS`), smoke rounds, screenshots in the log. |
| `scripts/ci/extract_log_screenshots.py` + `test_extract_log_screenshots.py` (new) | Log to PNG files; tolerant of timestamp prefix, `gh run view --log` columns, CR, BOM, ANSI; verifies magic bytes (and chunk CRCs as a warning); safe names; exit codes 0/1/2/3. 22 tests on synthetic logs, `python3 -I` clean. |
| `.github/workflows/windows.yml` | `push` on `claude/**` (and `cancel-in-progress` for those refs); step "Print screenshots to the log" (`if: always()`, `continue-on-error: true`) in `smoke` before the upload; in `check`, after Clippy: the Python tests and `test_screenshot_log.ps1`. |

### Decisions worth knowing

- **PATH was wrong for all-users installs.** The old registry line wrote `HKA\Environment` (= `HKLM\Environment`, not the
  system environment) while the code read and cleaned `Session Manager\Environment`. Now two lines (`HKCU\Environment`
  with `Check: NeedsAddUserPath`, `HKLM\...\Session Manager\Environment` with `Check: NeedsAddSystemPath`), each guarded by
  `NeedsAddPath`, so an upgrade in place never duplicates the entry and the uninstaller's `RemoveFromPath` finds it.
- **Shortcut working directory** uses `{%USERPROFILE}` as asked. It is expanded when the installer runs, so in an all-users
  install done through another administrator account it would be that account's profile. A literal `%USERPROFILE%`
  (expanded by the shell at launch) would be correct there too but cannot be verified without Windows, so it was not used.
- **MinVersion** is `10.0`, not `10.0.17763`: the plain form is certain to compile; 1809 is documented, not enforced.
- **Publisher** is `LesToqz` (the licence holder and repository owner); one `#define` (`MyAppPublisher`) changes it.
- Round 1 of the smoke test is `Invoke-Deferred`: if my new checks are wrong on the runner, round 2 (the existing
  installed-app tests and the window capture) still runs and the job fails at the end with both messages.

## Verified (commands run here, Ubuntu, Python 3.13)

- `python3 -I -B -m unittest discover -s packaging/windows -p 'test_*.py'`: 51 tests OK, 4 skipped (the real-sysroot tests).
- `python3 -I -B scripts/ci/test_extract_log_screenshots.py` and `python3 -I -B -m unittest discover -s scripts/ci -p 'test_extract_log_screenshots.py'`: 22 tests OK.
- Mutation check of `test_iss.py` (throwaway harness in the scratchpad, 13 mutations): every one was caught - misspelled
  `AppUserModelID`, `VersionInfoDescription`, a flag, a Check function and a constant; a missing `end`; an extension key
  (default handler); a task made `unchecked`; a registry key without `uninsdeletekey`; a changed filter; `uninsdeletekey` on the
  Environment key; and the removed `Exit` in `RemoveFromPath`.
- A throwaway bracket/quote/here-string balance checker (not a parser) accepts every `.ps1` of the repository and rejects
  three deliberately broken samples (unterminated subexpression, `"$name:"` scope trap, unclosed brace).
- `windows.yml` loads with PyYAML; trigger, concurrency and step order inspected.

## Not verified (needs CI)

- **The installer was not compiled.** Inno Setup is not available here. Every directive, parameter, flag and constant was
  written from the Inno Setup 6 documentation and checked against whitelists in `test_iss.py`, which only proves
  self-consistency. Residual risk: `Root: HKLM` under `PrivilegesRequired=lowest` may produce a compiler warning (the entry
  is skipped at run time by its Check function); `AppUserModelID` and `{%USERPROFILE}` are standard but untested here.
- **No PowerShell was run or even parsed.** `pwsh` is not installed. An attempt to fetch PowerShell 7 into the scratchpad
  downloaded fine, but executing the downloaded binary was denied by the sandbox policy, so it was deleted and not retried.
  The parse check of the `check` job (`git ls-files '*.ps1'`) will be the first real syntax check of `smoke.ps1`,
  `window-capture.ps1`, `screenshot-log.ps1`, `print-screenshots.ps1`, `lab-helpers.ps1` and the two tests. Strict-mode
  traps were avoided by reading (unary comma for arrays returned from functions, no `$var:` in strings, `@(...)` around
  collected results, registry access through .NET because of the `*` key), not by running.
- Windows-only behaviour: `WScript.Shell` shortcut properties, 8.3 versus long path comparison (`RUNNER~1`), the
  AppUserModelID search in the `.lnk` bytes (a warning, not a failure, if absent), registry reads, the Inno upgrade over an
  existing install with `/VERYSILENT`, the uninstaller timing, `CopyFromScreen` on the runner's desktop (it may be blank
  on a runner without an interactive session; `PrintWindow` is then written as `desktop-window-printwindow.png`),
  System.Drawing downscaling, and the size of the printed log blocks.
- `actionlint` and `zizmor` (run by `security.yml`) were not run on the changed workflow.

## NEEDED (outside my files)

1. **The app rejects a file argument.** `crates/impro/src/cli.rs` accepts only a folder (`not a directory: ...`, exit 2). The
   new "Open with impro" context-menu entry and `Applications\impro.exe\shell\open\command` pass `"%1"` for files, so until
   the app opens a file argument (project = its parent folder, the file opened), those entries start impro and it exits
   with an error. The folder entries are unaffected. If the app change will not happen, remove task `filecontextmenu` and the
   Applications registration.
2. The "ambient launch" (working directory == user profile reopens the last folder) is app side; the shortcuts now rely on it.
3. `docs/windows.md` (task table says "all off by default", the silent example `/TASKS=desktopicon` now selects ONLY the desktop
   icon, use `/MERGETASKS=desktopicon`), `docs/ci.md` and the top-level README are for the orchestrator to consolidate.
4. After the first CI run: fetch the `smoke` job log and run `python3 -I scripts/ci/extract_log_screenshots.py -o shots job.log`.

## Addendum: window probe, capped screenshots, new usage line

STATUS: DONE (still nothing compiled or run on Windows; CI is the first run of the new PowerShell and C#)

### What was added

- **Window probe** (`packaging/windows/window-probe.ps1`, new). `Write-WindowProbe -Hwnd <hwnd> -ScreenshotPath <png>` prints
  one `[window-probe] key=value` line per fact: process id and path, title (`GetWindowTextW`), class name, `style` and
  `exstyle` in hex plus `ws_caption`, `ws_thickframe`, `ws_sysmenu`, `ws_minimizebox`, `ws_maximizebox`, `ws_visible` as
  `true`/`false`; `GetWindowRect`, `GetClientRect`, client origin and the DWM extended frame bounds (with HRESULT);
  DPI (`GetDpiForWindow`, fallback `GetDeviceCaps`), window and process DPI awareness; the `DwmGetWindowAttribute`
  read-backs 20, 33, 34, 35, 36 each with `<key>_hr` (colours as `#RRGGBB`, the 0xFFFFFFFF / 0xFFFFFFFE markers raw);
  the window's `System.AppUserModel.ID` (`SHGetPropertyStoreForWindow`, `{9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}` pid 5) and
  whether it equals `dev.impro.impro`; `WM_GETICON` small / big / small2 with `_nonzero` booleans and the class icons; the
  OS version, build and UBR. Every fact has its own `try`/`catch` (`key=error(<message>)`), the two P/Invoke classes
  compile independently (a COM mistake cannot take the Win32 facts down), nothing runs at dot-source time, and a final
  `done=true` line shows that the probe ran to the end. Full key list: README, "Window probe".
- **Pixel probes** from the `CopyFromScreen` capture of the window (frame included): `bitmap_size`,
  `caption_height_px` (+ `caption_height_source`: client origin minus visible frame top when the window has `WS_CAPTION`,
  else window minus client height, else 16 px at 96 DPI) and `pixel(<name>)=#RRGGBB` with `pixel_pos(<name>)` for
  `caption_center`, `below_caption_x60`, `below_caption_center`, `sidebar_body`, `editor_center`, `bottom_right`. Offsets
  are device pixels scaled by DPI/96 (half away from zero); positions outside the bitmap print `n/a(outside the bitmap)`.
- **Wiring.** The window only exists inside `Save-DesktopWindowScreenshot` (it stops the app in its `finally`), so the probe
  cannot run "after" that function returns. `Save-DesktopWindowScreenshot` therefore takes an optional `-AfterCapture`
  script block, run right after the capture while the app is still up; `smoke.ps1` passes
  `{ param($Window, $Png) Write-WindowProbe ... }` in installer mode only. A failing probe is only a warning.
- **`print-screenshots.ps1` is capped.** Only `desktop-window.png` and ONE `first-paint*.png` (first in name order) are
  considered; each is downscaled to at most 700 px width and skipped when its base64 would exceed 60 KB. Other files get
  `[screenshots] skipped <name> (<size>)`. The marker lines and the extractor are unchanged. Defaults: `-MaxWidth 700`,
  `-MaxBase64KB 60`.
- **Usage line.** `smoke.ps1` now has `-ExpectedUsage` with the default `usage: impro [-h] [--version] [path ...]`. The
  temporary stand-in executable still prints `[folder]`, so the two `smoke-standin` steps of `windows.yml` pass that text
  explicitly; the product `smoke` job uses the default.
- **CI.** `check` runs `test_window_probe.py` and `test_window_probe.ps1`. While there I fixed a masking problem: several
  PowerShell test scripts ran one after the other and only the last exit code counted, so an earlier failure was lost; each
  call is now followed by `if ($LASTEXITCODE) { exit $LASTEXITCODE }` (also for the two existing tests).

### Tests

- `probe-vectors.json`: hand-written expectations for the pure helpers (COLORREF to `#RRGGBB` including the raw markers and
  -1, signed and unsigned HRESULTs, style bits including `WS_BORDER` alone, DPI scaling of offsets with half-way cases,
  caption height rules, the six probe points at 96 / 144 DPI and for a bitmap that is too small, DWM value formatting, line
  format and single-lining, base64 length, which screenshots are selected).
- `test_window_probe.ps1` (new, runs in `check`) executes the PowerShell functions against the vectors and the fact runners
  (`error(...)` for a throwing fact). `test_screenshot_log.ps1` now covers the selection, the skipped lines and the size cap.
- `test_window_probe.py` (new, 28 tests) checks the same vectors against an independent Python implementation, pins the
  Win32 numbers of the probe (`WS_*`, DWM attribute ids 20/33/34/35/36, `GWL_STYLE`, `WM_GETICON`, `LOGPIXELSX`, per-monitor-v2
  pseudo handle, property key, IPropertyStore vtable order), requires every probe key to be documented in the README, and
  checks the wiring (probe after the capture and before the app is stopped, installer mode only, caps, usage line, CI).

### Verified here

- `python3 -I -B -m unittest discover -s packaging/windows -p 'test_*.py'`: 79 tests OK (4 skipped, real sysroot).
- `python3 -I -B scripts/ci/test_extract_log_screenshots.py`: 22 OK.
- Mutation check of the new Python tests (14 edits: a wrong `WS_CAPTION`, DWM attribute id, `LOGPIXELSX`, `WM_GETICON`, key
  id, COM method order, a renamed key, a statement at dot-source level, a lost `-AfterCapture`, the old usage line, both caps,
  a README key): all caught. The first run missed the README edit because of a bug in my test (`s[0]` of a string); fixed.
- Bracket/quote balance of every `.ps1` (throwaway checker, not a parser) and `windows.yml` loaded with PyYAML.

### Not verified

- No PowerShell or C# was executed or compiled. New risks, all contained: the two `Add-Type` blocks (a compile error
  yields `win32_helper=error(...)` / `shell_helper=error(...)` lines, not a failed run), the PROPVARIANT / IPropertyStore
  interop (sized for 64-bit), `SHGetPropertyStoreForWindow` reading a per-window ID (a process-level ID may show as
  `(empty)`), `DwmGetWindowAttribute` read-back support for attributes 20 and 33-36 on a given build (the `_hr` lines
  say), `ConvertFrom-Json` typing in `test_window_probe.ps1`, and strict-mode behaviour of the new functions (read, not run).
- Whether `CopyFromScreen` shows the window on the runner is unchanged and still unknown; the facts do not depend on it.

### NEEDED (outside my files)

1. `crates/impro-ui/examples/packaging_standin.rs` and the checks below still print or assert `[folder]`:
   `scripts/ci/smoke.sh` line 37 and `scripts/test-install.sh` lines 22 and 71 (also `crates/impro/src/cli.rs` on this branch,
   which the coordinator says is already `[path ...]` upstream). Update them with the app change; remove my `-ExpectedUsage`
   arguments from the two `smoke-standin` steps when the stand-in is updated or deleted.
