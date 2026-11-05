# launch-welcome

STATUS: DONE

Launch and first-run experience: path arguments, ambient-launch resolution, remembered folders,
a welcome page instead of the one-line empty state, a native Open Folder dialog, File > Open Recent.

## Behaviour (for the docs)

Command line: `impro [-h] [--version] [path ...]`.

- A directory argument is the project root; with several, the first one wins and the rest are ignored.
- File arguments open in the editor after the window exists, in argument order, the first one selected.
  With no directory argument the root is the first file's parent.
- `file://` URIs are accepted as paths (a desktop entry using `%U`).
- `--` ends option parsing (`impro -- -odd-name.txt`). Unknown options, and any path that does not exist, print
  the usage line plus `impro: error: ...` and exit with status 2. Help and version still need no display.
  Missing paths read `no such file or directory: <path as typed>`.
- An explicit directory argument is always honoured, even when it is the home or install directory.

Ambient launch (no path argument and a working directory nobody chose):

| Platform | Working directory counts as ambient when | Opens |
| --- | --- | --- |
| Windows | inside the executable's directory, inside `%SystemRoot%` (else `%windir%`), or equal to the user profile | last used folder if it still exists, else `%USERPROFILE%\Documents` if present, else the profile |
| Linux / others | equals `$HOME` | last used folder if it still exists, else `$HOME` |

Any other working directory (a terminal in a project) is used as is. Comparisons are by path component:
both separators, case-insensitive and verbatim-prefix tolerant on Windows; trailing slashes and symlinked
`$HOME` are normalised (both sides are canonicalised before comparing). Folders chosen only by the
fallback (Documents or home) are not written to the recent list.

State: `state.json` next to `settings.json` (`<config root>/impro/state.json`, i.e. `%APPDATA%\impro` on Windows,
`$XDG_CONFIG_HOME` or `~/.config` on Linux):

```json
{ "version": 1, "last_folder": "/work/app", "recent": ["/work/app", "/work/notes"] }
```

Up to 10 recent folders, newest first. Written through a temporary file in the same directory plus rename (mode 0600
on Unix) on a short-lived background thread; read once at launch. Missing, unreadable, corrupt or hand-edited
files (wrong types, relative paths, duplicates) never panic: they read as "no state". It is recorded when a window
opens its folder and after every successful Open Folder / Open Recent. The settings schema and `defaults.json`
are untouched. Recent folders that no longer exist are hidden for the session by a background check, never
deleted from the file (a removable drive may return).

Linux single instance: a plain `impro` still focuses the running window (unique GApplication). A launch that names
paths uses `NON_UNIQUE`, so `impro /other/folder` or "open with" gets its own window instead of silently dropping
the argument. Windows was already non-unique.

Welcome page (editor empty state, `EditorPanel` stack page `empty`): the impro mark (the packaging mark drawn
as vector paths, no decoding and no extra asset), title and tagline, a Start list (Open Folder, Quick Switcher,
Command Palette, New Note, Settings) with one key cap per key of the real shortcut read from the command table
(only commands that have one show chips), and Recent folders (up to 5, current folder excluded, hidden when
empty, click or Enter opens). It follows theme and accent changes live (CSS through `impro_ui::register_css`;
hover and focus tint with the accent; secondary text is the foreground at 74% so every bundled theme keeps at
least 4.5:1 on the page and on the key caps), has no timers or animation (`transition: none`), rows are buttons
(Tab, arrows, Enter, Space, accessible names), two columns when the pane is wide, one when narrow, scrolls and
scrolls the focused row into view, and switches to a compact layout in a short pane (small window with the
terminal open).

Open Folder uses `gtk::FileChooserNative` (system folder picker on Windows, portal or GTK otherwise), title
"Open Folder", accept label "Open", starts in the current root, kept alive until it answers.
File > Open Recent (after Open Folder) lists the recent folders other than the current one and is rebuilt each
time the File menu opens; it is disabled when there is nothing to reopen.

## Files

New: `crates/impro/src/launch.rs`, `crates/impro/src/welcome.rs`, `crates/impro/tests/test_launch.rs`,
`crates/impro/tests/test_welcome.rs`, this report.
Changed: `crates/impro/src/{cli,app,commands,window,lib}.rs`, `crates/impro-editor/src/panel.rs`
(`EditorPanel::set_empty_widget`, default label kept), `scripts/ci/smoke.sh`, `scripts/test-install.sh`
(fixture usage text).
`lib.rs`: exactly `pub mod launch;` and `pub mod welcome;`. `window.rs`: `open_folder` and one line in `apply_root`.

## How verified

See the final section; everything on Linux unless noted.

## Not verified / assumptions

- Nothing ran on Windows. The `cfg(windows)` / Windows-rule paths are covered by unit tests with injected
  Windows-style paths (they run on any host) and compile- and clippy-checked for `x86_64-pc-windows-gnu`
  through the host-stub script. The native Windows folder picker, the Explorer launch environment (shortcut working
  directory, `%SystemRoot%`) and `%APPDATA%` persistence are untested.
- `Documents` is `%USERPROFILE%\Documents`; a redirected (OneDrive) Documents folder is not resolved (that needs the
  Shell known-folder API and an extra windows-sys feature).
- Dead network drives: the last-folder existence check and the recent-folder check run, respectively, once on the main
  thread for ambient launches only and on a background thread.

## NEEDED from other owners

See the hand-off list in the final reply: usage text in `packaging/windows/smoke.ps1`,
`crates/impro-ui/examples/packaging_standin.rs`, `README.md`, `docs/windows.md`, `packaging/debian/impro.1`;
`Exec=impro %F` and MimeType in `packaging/impro.desktop` (then `scripts/test-install.sh` expects `Exec='impro'`);
Windows installer "Open with" registration; `impro_ui::widgets::WrapBox` style-invalidation limitation.
