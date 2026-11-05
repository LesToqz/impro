# impro on Windows

impro builds and runs on Windows with the same GTK 3 code base as on Linux. The
Windows build targets `x86_64-pc-windows-gnu` with GTK 3 from MSYS2 UCRT64. The
content of the window (themes, layout, start page, command-line paths) is shared with
Linux; the window frame, installer and system integration are Windows-specific, and
the Linux window frame is unchanged.

**Status:** everything on this page was written against CI results and Linux-side
checks; nobody has used the installed application on Windows hardware yet. The Windows
workflow builds, installs and launches the application on a GitHub runner
(Windows Server 2025, build 26100), and its smoke job reads the real window back (see
[CI](ci.md#windows-evidence-in-the-job-log)). That run confirmed a native captioned,
resizable frame (caption, system menu, minimise and maximise boxes), the caption
`<folder> - impro`, a caption colour equal to the docked bar's colour at the sampled
points, rounded corners, and a light theme chosen on first run because the runner uses
Windows' light mode. Items marked *not yet verified* below were not confirmed by it.

## Supported systems

Windows 10 version 1809 or newer, and Windows 11, 64-bit x86 only. Arm64 and 32-bit
Windows are not built.

## Packages

| File | Contents |
| --- | --- |
| `impro-<version>-x64-setup.exe` | Inno Setup installer. Per-user by default; no administrator rights needed. |
| `impro-<version>-x64-portable.zip` | The same tree without installer: unzip anywhere and run `impro.exe`. |
| `SHA256SUMS.txt` | SHA-256 of both files. |

Both contain `impro.exe`, the GTK/GLib/Pango/Cairo DLLs it imports, GLib schemas,
gdk-pixbuf loaders, the Adwaita and hicolor icons, `WebView2Loader.dll`, the licences
and `bundle-manifest.txt`. The layout and the reason it is flat are in
[docs/packaging.md](packaging.md). The packages are built by
`.github/workflows/windows.yml` and uploaded as the `impro-windows` artifact.

## Requirements

| Feature | Needs | Without it |
| --- | --- | --- |
| Git graph, commit pages, explorer Git status | Git for Windows (found on `PATH`) | Git features are unavailable; the editor, notes, graph view and terminal work. |
| Git graph and commit pages (rendering) | Microsoft Edge WebView2 Evergreen runtime (preinstalled on Windows 11 and current Windows 10) | The pages show a notice instead; everything else works. |
| Terminal docks | One of PowerShell 7, Windows PowerShell or `cmd.exe` | A shell is chosen in that order unless one is configured in the settings. |

## Install, uninstall, portable use

- **Install:** run the setup executable. It installs for the current user (the
  dialog on the first page can switch to all users) and adds a Start Menu entry.
  Silent install with the default tasks:
  `impro-<version>-x64-setup.exe /CURRENTUSER /VERYSILENT`. Add the desktop icon on
  top of the defaults with `/MERGETASKS=desktopicon`; `/TASKS=...` instead selects
  exactly the tasks it lists.
- **Tasks:**

  | Task | Default | Effect |
  | --- | --- | --- |
  | `desktopicon` | off | Desktop shortcut |
  | `contextmenu` | on | "Open folder in impro" on folders and folder backgrounds in Explorer |
  | `filecontextmenu` | on | "Open with impro" on every file in Explorer |
  | `addtopath` | on | Adds the install directory to the `PATH` (user, or system for an all-users install) |

- **Always registered:** the Start Menu shortcut, which starts in your user profile
  folder and carries the taskbar identity `dev.impro.impro`; an App Paths entry, so
  `Win+R`, then `impro` works without a `PATH` entry; and impro in the "Open with"
  list for common text and source files. impro never becomes the default program for
  any file type.
- **Uninstall:** Settings, Apps, or `unins000.exe` in the install directory. The
  uninstaller removes the files, shortcuts, registry entries above and the `PATH` entry.
  Settings and recent folders under `%APPDATA%\impro` are not touched.
- **Portable:** unzip and run `impro.exe [path ...]`. Settings are stored per user as described under Settings; the font cache is
  created by the GTK stack.

## Behaviour as a Windows application

- **Window frame.** The main window uses the native Windows title bar and frame, so
  the caption buttons, resize borders, snapping, the system menu (`Alt+Space`) and
  the caption icon are the system's. The bar with the File/View menus and the dock
  toggles is part of the window content, directly under the caption, with the same
  styling as on Linux. The caption reads `<folder> - impro`.
- **Colours.** The caption follows the theme. On Windows 10 (1809 and newer) it is a
  dark or light caption to match the theme's brightness. On Windows 11 (build 22000
  and newer) the caption, text and border colours are also set to the theme's bar
  colours so the caption and the bar form one surface, and the corners are rounded.
  Anything the running Windows does not support is skipped without an error.
- **Taskbar.** The process and the shortcuts share the AppUserModelID
  `dev.impro.impro`, so a pinned shortcut and the running window group together.
- **First run.** With no `settings.json` yet, impro starts in a light theme when
  Windows apps use the light mode and in `impro-dark` otherwise. A saved theme is
  never overridden. Windows' "animation effects" switch turns on Reduce motion unless
  you set it explicitly.
- **Starting.** Launched from the Start Menu or a desktop shortcut, impro reopens the
  last folder you used instead of its own install directory; see
  [Opening projects](../README.md#opening-projects). `impro.exe [path ...]` accepts
  folders and files, which is what the Explorer entries use.
- **Open Folder** is the system folder picker.

The default window is 1200 x 790 pixels of client area; on a screen whose usable area
(taskbar excluded) cannot hold it next to the native frame it shrinks to fit, but not
below 640 x 420. (The runner's 1024 x 768 screen showed the unfitted window running off
the bottom edge, which is what this fixes.)

Not yet verified: Windows 10 (the dark caption and its fallback attribute), Snap
Layouts and the system menu in use, taskbar grouping through the AppUserModelID (the
probe can only read the per-window property, which is empty by design), the caption icon
at each DPI scale, mixed-DPI monitors, the system folder picker, and how the installer
wizard looks.

The installer is not code-signed. Windows SmartScreen may warn on first run
("More info", then "Run anyway"). Compare the file against `SHA256SUMS.txt` first.

## First launch

The first start after installing builds a font cache and is slower than later ones.
On the CI runner (`windows-2025`) the measurements were roughly 4-9 s cold and about
1 s warm. These are runner measurements, not a promise for other machines. The
`first-paint-ms` value can be printed with `IMPRO_QA_TIMING=1`. The CI experiment
that investigates the one-time cost is described in
[packaging/windows/README.md](../packaging/windows/README.md#startup-lab); the
cause is not confirmed in this documentation.

## Settings

`%APPDATA%\impro\settings.json` (the roaming profile; the fallback is
`%USERPROFILE%\AppData\Roaming`). On Linux the file is under `XDG_CONFIG_HOME` or
`~/.config`. The format is identical. The last folder and the recent folders are in
`state.json` next to it.

## Where the Windows-specific code lives

| Part | Location |
| --- | --- |
| Native window frame (DWM caption colours, taskbar identity, first-run theme and reduce-motion from the system) | `crates/impro/src/win` (pure decisions in `policy.rs`, OS calls in `sys.rs` and `frame.rs`), docked bar in `crates/impro-chrome/src/titlebar.rs` |
| Launch resolution, recent folders, start page | `crates/impro/src/launch.rs`, `crates/impro/src/welcome.rs` |
| ConPTY terminal (pseudo console plus a terminal emulator and GTK widget) | `crates/impro-vte/src/windows`, `crates/impro-vte/src/emu` |
| WebView2 Git pages | `crates/impro-gitviews/src/gitviews_windows.rs` (navigation policy `windows_policy.rs`, diagnostics `windows_diag.rs`) |
| Executable resources (icon, version info, manifest) | `crates/impro/build.rs`, `crates/impro/windows/` |
| Platform directories, process spawning, executable lookup | `crates/impro-core/src/{platform,process,exec}.rs` |
| Bundling, installer, smoke tests | `packaging/windows`, `scripts/package/windows` |

See [Architecture](ARCHITECTURE.md#platform-backends) for the seams.

## Environment variables

| Variable | Effect |
| --- | --- |
| `IMPRO_GIT_PAGES=notice` | Do not create a WebView2 host; show the notice page instead. |
| `IMPRO_GIT_DEVTOOLS` | Controls WebView2 developer tools for the Git pages (see `crates/impro-gitviews/src/windows_policy.rs` for accepted values). |
| `IMPRO_GIT_DEBUG=1` | Print Git page diagnostics and one stderr line per failed git invocation. |
| `IMPRO_GIT_DIAG=1` | Echo the Git page diagnostics timeline to stderr (also used by the test harness). |
| `IMPRO_QA_TIMING=1` | Print startup phases and `first-paint-ms` (with the screenshot helper). |
| `IMPRO_QA_SCREENSHOT=path.png` | Capture the first complete paint to a PNG and exit. |

`impro.exe` is a GUI-subsystem program; it re-attaches to the parent console for
`--help`, `--version` and usage errors, so the output appears in `cmd` and PowerShell.

## Known limitations

- The installer and `impro.exe` are unsigned.
- Several shortcuts use Ctrl+Alt (Graph view Ctrl+Alt+G, Git graph Ctrl+Alt+H, the
  right dock Ctrl+Alt+`). On keyboard layouts where AltGr produces characters,
  Windows reports AltGr as Ctrl+Alt, so these can collide with typing. They have not
  been audited for this yet.
- No primary-selection clipboard: `copy_primary` and `paste_primary` in the terminal
  are no-ops on Windows. The normal clipboard works.
- Windows behaviour has been checked mainly through CI and cross type-checking from
  Linux. Not yet verified on Windows hardware: mixed-DPI monitor moves, non-ASCII,
  UNC and long paths, and symlink or junction saves.
- Several test targets are Linux-only (XTest divider dragging, FIFO and symlink
  tests, POSIX CPU-timer performance examples, WebKitGTK QA hosts). The Windows
  `test` job is report-only for this reason; see [CI](ci.md#windows).
- The GUI test harness cannot initialise GTK on the hosted runner (no interactive
  desktop), so most GUI test targets fail there.
- Dependency policy (`deny.toml`) is checked for the Linux target only.

## Troubleshooting

| Symptom | Likely cause and fix |
| --- | --- |
| SmartScreen warning | The installer is unsigned. Verify `SHA256SUMS.txt`, then "Run anyway". |
| Git pages show a notice | WebView2 runtime missing or failed to start. Install the Evergreen runtime, or check the reason with `IMPRO_GIT_DEBUG=1` run from a console. |
| Notice appears although WebView2 is installed | `IMPRO_GIT_PAGES=notice` is set in the environment. |
| Git graph empty or git features unavailable | Install Git for Windows and restart impro so it sees the new `PATH`. |
| Terminal opens the wrong shell | Set the shell in the settings, otherwise the order is PowerShell 7, Windows PowerShell, `cmd.exe`. |
| Slow first start | One-time font cache; later starts are faster (see First launch). |
| impro opens the wrong folder from a shortcut | A shortcut that starts in your user profile, the install directory or the Windows directory reopens the last folder you used. Start `impro.exe C:\path\to\folder` (or use "Open folder in impro") to choose a folder explicitly. |
| "Open with impro" does nothing | Needs the build that accepts file arguments (`impro [path ...]`); older builds reject a file path and exit with status 2. |
| `impro` not found in a console | The `addtopath` task was not selected; open a new console after enabling it, or use the full path. |
| Nothing printed by `impro --version` in a script | Output is attached to the parent console; when redirecting, check both streams. Not yet verified on all shells. |
