STATUS: DONE (Linux behaviour verified; everything under `cfg(windows)` is compile-checked only, never run)

# win-frame: native Windows frame, impro content

## Problem and decision

The Windows build used a GTK client-side header bar (`window.set_titlebar`), so the window had no real
caption: no Snap Layouts, DWM shadow or rounded corners, system menu or native minimise/maximise
animations. The decision (not revisited) is to keep the **native Windows frame** and move the impro
bar into the window content. No window procedure subclassing, no `WM_NCCALCSIZE`.

## What changed

| Area | Files | Change |
| --- | --- | --- |
| Docked bar | `crates/impro-chrome/src/titlebar.rs` | `TitleBar::new_docked(menu_bar)` and `TitleBar::is_docked()`. Same widget, same `impro-titlebar` class and CSS (plus an extra `impro-titlebar-docked` class that no rule uses yet). Docked: no close/minimise/maximise buttons, no decoration layout, no centred title (the label exists but stays unparented, so `title_label()` keeps working). `TitleBar::new` is unchanged. |
| Placement | `crates/impro/src/window.rs` | Docked: the content is `Box(titlebar, overlay host)`; the bar sits **above** the overlay host so the palette, notices and switcher keep the geometry they have under the CSD bar. Not docked: `set_titlebar` and `window.add(overlay)` exactly as before. |
| Default size | `crates/impro/src/window.rs` | Docked only: `set_default_size(1200, 750 + HEADER_HEIGHT)`. Under the CSD bar GTK sizes the window by its content, so the workspace is 1200x750 under the 40 px bar; a native caption is outside the client area, so without the extra 40 px the docked bar would eat into the workspace. One line, easy to drop if a shorter window is preferred on small screens. |
| Title | `crates/impro/src/window.rs` | Three `set_title` sites use `win::policy::window_title(root)`: `<folder> - impro` on Windows, `impro` everywhere else. `apply_root` (every Open Folder / `set_root`) updates it. |
| Windows module | `crates/impro/src/win/{mod,policy,sys,frame}.rs`, `lib.rs`, `main.rs` | See below. |
| First-run defaults | `crates/impro/src/preferences.rs` | `startup_values` (pure, tested) folds system theme and animation settings into choices the user never saved. |
| Theme refresh | `crates/impro/src/preferences.rs` | `refresh_style` calls `win::refresh_frame` (no-op off Windows), so theme and accent changes recolour the caption. |
| Features | `crates/impro/Cargo.toml` | `windows-sys` features: `Win32_Foundation`, `Win32_Graphics_Dwm`, `Win32_System_Console`, `Win32_System_Registry`, `Win32_UI_Shell`, `Win32_UI_WindowsAndMessaging`. No new crate, `Cargo.lock` untouched. |
| Constants | `crates/impro-core/src/identity.rs` | `APP_USER_MODEL_ID` (= `APP_ID`, `dev.impro.impro`) and `DOCKED_TITLEBAR_ENV`. |
| Tests | `crates/impro-chrome/tests/test_titlebar_docked.rs` (new), unit tests in `win/policy.rs` and `preferences.rs` | See "Verified". |

### `win` module layout

`lib.rs` has `pub mod win;` **unconditionally**, not `#[cfg(windows)] pub mod win;` as the brief
suggested. Reason: the brief also requires the first-run theme decision, the title format and the DWM
colour mapping to have unit tests that run on Linux, and tests inside a `cfg(windows)` module cannot.
So the module is split:

- `win/policy.rs`: every decision as a pure function, compiled on all hosts, unit-tested on Linux.
- `win/sys.rs`, `win/frame.rs`: `#[cfg(windows)]` only. All OS calls.
- `win/mod.rs`: `init_process()` (Windows only) and four shims (`install_frame`, `refresh_frame`,
  `system_apps_use_light_theme`, `system_client_area_animations`) that are no-ops or `None` off
  Windows, so `window.rs` and `preferences.rs` carry no `cfg` clutter.

### Windows behaviour (compile-checked only)

- **AppUserModelID**: `win::init_process()` is the first statement of `main` (`cfg(windows)`) and calls
  `SetCurrentProcessExplicitAppUserModelID("dev.impro.impro")`. The installer shortcut must carry the
  same string (`identity::APP_USER_MODEL_ID`). A failure only loses taskbar grouping.
- **HWND**: `gdk_win32_window_get_handle(GdkWindow*)` declared with `extern "C"` in `sys.rs` (libgdk-3
  is already linked via gdk-sys, which has no win32 bindings). One small safe wrapper, `sys::hwnd`,
  returns `None` before realize or for a null handle.
- **DWM** (`frame.rs`, through `DwmSetWindowAttribute`, every call's HRESULT ignored, nothing panics):
  - `DWMWA_USE_IMMERSIVE_DARK_MODE` (20), falling back to 19 for Windows 10 1809 to 1909. Chosen by
    whichever of white and black glyphs contrasts better with the caption colour; a unit test proves
    this equals the catalogue's `dark` flag for all 14 themes.
  - Only when the Windows build is >= 22000 (read from `HKLM\...\CurrentBuildNumber`, independent of the
    manifest): `DWMWA_WINDOW_CORNER_PREFERENCE` = round, `DWMWA_BORDER_COLOR`, `DWMWA_CAPTION_COLOR`,
    `DWMWA_TEXT_COLOR`.
  - Colour keys: `sidebar_bg` (caption), `titlebar_fg` (text), `border` (border). **Not** `titlebar_bg`:
    titlebar.css paints the bar with `sidebar_bg`, and `titlebar_bg` differs from it in 9 of the 14
    themes (nothing in the code uses it), so using it would leave a visible seam between caption and bar.
    The effective theme (accent applied) comes from `impro_ui::current_theme()`.
  - Applied on `realize` (before the window is shown, so no flash), once more on the first `map`, and
    on every theme/accent change through `refresh_style`.
  - The dwmapi numbers live twice (pure copies in `policy.rs`, windows-sys in `frame.rs`); a `const`
    assertion in `frame.rs` fails the Windows build if they ever diverge.
- **Dark/light on first run**: `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme`.
  `0`, a missing value or an unreadable registry keep the default `impro-dark`; `1` selects
  `impro-light` (the light sibling of the default; if it were absent, the first light theme). Applies
  only when settings hold no non-null `theme`. Nothing is written back, so a user who never chooses
  keeps following Windows at each start; choosing a theme stores it as before. Schema and
  `defaults.json` are untouched.
- **Reduced motion**: when `SPI_GETCLIENTAREAANIMATION` reports off and `settings.json` has no
  `reduce_motion` key, the startup value of `reduce_motion` is `true` (so the Settings page shows it
  ticked and every existing consumer, including the graph views that read the flag directly, honours
  it). An explicit stored `true` or `false` is never overridden. Done entirely inside owned files.
  GTK's own `gtk-enable-animations` (which `motion::animations_enabled` already consults) may or may not
  track this OS setting on the Win32 backend; the explicit handling does not depend on that.

### Window and taskbar icon (verified by reading, no change made)

`app_icon.rs` calls `gtk::Window::set_default_icon_list` with the seven PNG sizes from an idle queued by
the first draw (a startup-cost decision, covered by `test_app_icon`). On GTK 3 that call also refreshes
already-realized windows that use the default icon, and the Win32 backend turns the list into
`WM_SETICON` (big and small). Before that idle runs, the window class icon is the executable's first
icon, i.e. `1 ICON` from `impro.rc.in` (the .ico). So the caption and taskbar show the impro icon
at both stages and I found no functional gap. I did not change `app_icon.rs` (not an owned file) and
did not add an earlier icon install, which would only move decode cost back into startup.
UNVERIFIED on Windows: which of the two icons Windows shows for a small caption at 125 to 200 % scale.

### Manifest (`crates/impro/windows/impro.manifest`): unchanged on purpose

No `dpiAware` / `dpiAwareness` / `PerMonitorV2` entry. GTK 3.24's Win32 backend sets the process DPI
awareness itself (`SetProcessDpiAwareness(PROCESS_SYSTEM_DPI_AWARE)` with a `SetProcessDPIAware`
fallback, and it copes with awareness that was already configured; see `reports/windows/win-port.md`)
and scales its own drawing by the resulting factor. A manifest entry would add nothing for the system
mode, and a per-monitor-v2 declaration would make Windows stop bitmap-scaling for GTK while GTK 3.24 does
not handle per-monitor DPI changes, giving a mismatched scale on a monitor change. The native caption
and resize borders are drawn by Windows for the awareness GTK selected. `rcgen.rs` and `impro.rc.in` are
untouched (`test_rc_template` still guards them). Reasoning is from the GTK 3.24 source behaviour
documented in the earlier port report, not from a run on Windows: multi-monitor fractional scaling
remains UNVERIFIED.

### Preview switch

`IMPRO_DOCKED_TITLEBAR=1` (constant `identity::DOCKED_TITLEBAR_ENV`) makes a **debug** build on any
platform use the docked layout (`policy::docked_titlebar_for(windows, debug_build, env)`; release
builds ignore it). Under Xvfb there is no window manager, so the screenshot shows only the client area
that Windows would put below its caption.

## Verified (Linux host)

All commands run from the worktree root; `S` is the session scratchpad. Private target directories
(copies of the shared one, see "Shared target directory hazard") were used for every result below.

| Check | Command | Result |
| --- | --- | --- |
| Format | `cargo fmt --all --check` | clean |
| Linux lint | `cargo clippy -p impro -p impro-chrome --all-targets -- -D warnings` | clean |
| Windows type-check and lint (compile only) | `IMPRO_WIN_CARGO_CMD=clippy scripts/windows/check-stub.sh -p impro --all-targets -- -D warnings` | clean; `sys.rs`, `frame.rs` and `init_process` really are compiled (an earlier run flagged a `redundant_closure` in `frame.rs`, now fixed) |
| Chrome tests | `bash scripts/test.sh -p impro-chrome --test test_titlebar --test test_titlebar_dock --test test_titlebar_docked` | 17 + 2 + 7 passed. The 7 are new: no window buttons or centred title, same 39 px + 1 px divider and `sidebar_bg`, control order and cog geometry, fit at 500 px, requests and silent sync with keyboard activation, live theme switch, and the default bar is not docked |
| Pure policy and first-run unit tests | `bash scripts/test.sh -p impro --lib` | 22 passed: 5 in `preferences` (saved theme wins, null is not a choice, light and dark first run, an explicit reduce-motion choice wins), 13 in `win::policy` (path and title formats including `\\?\C:\...` and drive roots, theme choice and fallbacks, COLORREF byte order, caption colours equal `sidebar_bg`/`titlebar_fg`/`border` for all 14 themes, glyph darkness equals the catalogue's `dark` flag, the Windows 11 gate at build 22000, build-number parsing, the debug-switch rule) |
| Whole `impro` suite, default (CSD) layout | `bash scripts/test.sh -p impro --no-fail-fast` | every binary passes except `test_markdown_window::test_table_edit_save_rebuild_and_undo_keep_the_181_pixel_extent` (173 vs 181 px). **Pre-existing**: it fails identically on the untouched base commit `c4337b1` (built from a clean export in a separate directory), so it is a container font-metric issue, not this change |
| Window wiring with the docked layout | same, with `IMPRO_DOCKED_TITLEBAR=1`, for `test_window_layout`, `test_window_wiring`, `test_settings_wiring`, `test_search_wiring`, `test_graph_wiring`, `test_integration`, `test_markdown_blocks`, `test_startup` | all pass (accelerators, dock and explorer toggles, settings cog, palette, search), except `test_window_layout::test_default_layout_and_title`, which hard-codes the 1200x750 default size and sees 1200x790 by design, and the pre-existing markdown test above. A throwaway test (deleted, not committed: `crates/impro/tests` is not mine) confirmed the real `MainWindow` is docked under the switch: bar parent `GtkBox`, `window.titlebar()` none, bar at y=0 and 40 px high, sidebar at y=40, title `impro` |
| Linux look unchanged | `IMPRO_QA_SCREENSHOT=... GDK_BACKEND=x11 xvfb-run -a -s "-screen 0 1400x900x24" dbus-run-session -- target/debug/impro $S/proj`, then `compare -metric AE` against `$S/shots/linux-baseline.png` | 0 differing pixels (1210x800 both) |
| Docked look | same with `IMPRO_DOCKED_TITLEBAR=1` (`$S/shots/win-frame-docked.png`, 1200x790) | seen: File and View menu and the explorer toggle at the left; bottom-dock, right-dock and cog at the right; no window buttons, no title; same colours and divider; the workspace below matches the Linux one; nothing overlaps |

A pixel-identical Linux screenshot and the unchanged `TitleBar::new` path are the evidence that Linux did not move.

## Shared target directory hazard (important for the other agents)

The brief shares one `CARGO_TARGET_DIR` between all worktrees. That is not safe: Cargo derives artifact
names from workspace-relative paths, and its freshness check compares source mtimes recorded in the
dep-info file (package-root-relative), so a worktree whose files are older than the artifact another
worktree built last silently links that worktree's crate instead of its own. I hit this: after a
baseline build of the base commit in the shared directory, my next `cargo test` linked the base `impro`
library (symptom: `cannot find win in impro`), and my "passing" runs after that point were worthless. I
deleted the `impro-<hash>` fingerprint directories I had polluted in the shared directory (the next build
of any worktree rebuilds `impro`), then redid every result above in private copies of the target
directories and confirmed from the `Compiling impro ... (path)` lines that my sources were built.
Recommend per-worktree target directories, or touching the sources of the crates you changed before
trusting a shared-directory result.

## UNVERIFIED on real Windows

- Everything under `cfg(windows)`: HWND lookup, DWM calls, registry reads, SPI call, AppUserModelID.
- That caption, text and border colours really blend with the docked bar on Windows 11, and the
  dark/light caption glyphs on Windows 10; behaviour on 10 builds below 18985 (attribute 19 fallback).
- Snap Layouts, shadow, rounded corners, system menu (expected from the native frame; not observed).
- `<folder> - impro` in the caption and taskbar; the icon in caption and taskbar at each DPI scale.
- That `realize` is late enough for `gdk_win32_window_get_handle` to return a handle (it should: the
  GDK Win32 backend creates the HWND with the GdkWindow), and that DWM keeps attributes set while the
  window is still hidden (the second apply on `map` covers the case where it does not).
- The 40 px taller docked default window (790 px client area) on a screen shorter than about 830 px.
- Dragging: the docked bar is client area, so only the native caption moves the window.
- Whether the Windows first-run light theme, reduce-motion default and `gtk-enable-animations` interplay
  behave as described.

## Needed from others / notes for the orchestrator

- The installer shortcut must set AppUserModelID `dev.impro.impro` for pinning to match (installer files
  are not mine).
- `crates/impro/examples/window_qa.rs` asserts three native window glyphs on the titlebar; it is for the
  CSD layout and is unaffected unless it is run with the preview switch.
- Docs: nothing edited; the preview switch, the first-run theme/reduce-motion behaviour and the manifest
  reasoning above are what docs may want.
