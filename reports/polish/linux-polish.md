# linux-polish report

STATUS: DONE (all brief items covered; items that live in files owned by other agents are listed under "NEEDED").

Branch `worktree-agent-a88001bab9d8e5b9a`. Screenshots live in
`/tmp/claude-0/-home-user-impro/2fa35344-8d20-5ba7-956a-15cb3dffe012/scratchpad/shots/` (prefix `linux-polish-`, shortened
to `shots/` below). The reusable driver is `$S/ux.sh` (Xvfb :88 + openbox + dbus session + app, xdotool, `import`).

## Part 1: Linux desktop integration

| Item | Result |
| --- | --- |
| `packaging/impro.desktop` | `Exec=impro %F`, `TryExec=impro`, `GenericName`, `Comment`, `Keywords`, `MimeType` for 20 text and source types (never `inode/directory`, no URL scheme, no default-handler claim), `StartupWMClass=impro`. `desktop-file-validate` passes (one pre-existing hint: TextEditor without Utility; adding Utility would put impro in Accessories, so it is kept). |
| `StartupWMClass` | The old value `dev.impro.impro` **never matched**: under Xvfb the window sets `WM_CLASS = "impro", "Impro"` (GTK derives it from the program name; `_GTK_APPLICATION_ID` is `dev.impro.impro`). Fixed to `impro`, and `scripts/test-install.sh` now starts the installed binary under Xvfb and compares `xprop WM_CLASS` with the desktop file. |
| "Open a new window" action | Not added. On Linux the app is a unique GApplication (see NEEDED 1), so a second `impro` only presents the existing window; an action would be a no-op. |
| `packaging/dev.impro.impro.metainfo.xml` | New. `appstreamcli validate --no-net --pedantic` passes. Release entry is `type="development"` 0.1.0 (no tagged release exists); the install test fails when it differs from the workspace version. `project_license` is `MIT AND CC-BY-SA-4.0 AND LGPL-3.0-or-later`, because the binary embeds Yaru and libpanel icons (THIRD_PARTY_NOTICES.md); revert to `MIT` if you prefer the code-only licence. |
| `packaging/install.sh` | Installs/removes the metainfo, runs `update-desktop-database` (so `mimeinfo.cache` feeds "Open With") and `gtk-update-icon-cache`, removes a header-only `mimeinfo.cache` on uninstall, warns when `~/.local/bin` is not on PATH. Install and uninstall are symmetrical (test: no files remain). |
| `.deb` | `build-deb.sh` ships the metainfo under `/usr/share/metainfo` and `DEBIAN/md5sums`. Built from `/home/user/impro/target/release/impro`: two builds are byte-identical; `lintian` reports no error or warning (only info: `.comment` section, no fortify, spelling in strings of the binary); `apt-get install` then `remove` in this container refreshed `mimeinfo.cache` through the dpkg triggers, `appstreamcli get dev.impro.impro` found the component, and nothing was left behind. |
| `packaging/debian/impro.1` | Synopsis `[path ...]` and the new launch semantics (folders, files, ambient launch reopens the last folder; read from `launch.rs` on the orchestrator branch). |
| `.github/workflows/package-linux.yml` | The installed-files step also validates the installed metainfo and checks the MIME cache. |
| `scripts/test-install.sh` | Asserts the new files, desktop keys, metainfo identity and release version, `appstreamcli validate`, `mimeinfo.cache`, the live WM_CLASS. Passes with the release binary and with the fixture. `shellcheck` clean. |

Launcher check: with the debug binary installed into a throwaway HOME, `gio launch impro.desktop "<dir with a space>"`
opened the window with that folder as explorer root and terminal cwd (`shots/70-gio-launch-dir.png`).
On this branch `impro file` and `impro a b` exit 2 ("not a directory" / "unrecognized arguments"); the path-list CLI
exists only on the orchestrator branch and was **not run by me** (I read its `cli.rs` and `launch.rs`).

## Part 2: hands-on UX audit (Linux, debug build, Xvfb + openbox)

Exercised: tree expand/collapse, context menu, rename, new file (also an error), delete confirmation, README.md and
main.rs, tabs, Command Palette, Quick Switcher, search in files, every Settings page (9), terminal dock (add menu,
context menu), graph view, git graph (WebKit works under Xvfb), tooltips, `GDK_SCALE=2` (window, sidebar, Settings), one
light-theme pass (`impro-paper`) of dialogs and menus.

### Ranked findings

Fixed (each its own commit; before/after pairs named):

1. **Settings > About stated that no third-party icons are bundled**, but the binary embeds Yaru (CC BY-SA 4.0) and
   libpanel (LGPL-3.0-or-later) icons. Fixed; a unit test ties the text to THIRD_PARTY_NOTICES.md.
   `shots/28-settings-about.png` -> `shots/after-settings-about.png`.
2. **"+" add-pane menu showed GTK's missing-image square beside "Git graph"** (`view-list-tree-symbolic` is in no
   theme) and two people for "Graph view". Now the same icons as the Settings navigation; new test resolves every icon
   in the active theme (fails on the old name). `shots/plus-menu-before-after.png`.
3. **Mixed UK/US spelling** next to each other ("Color theme" vs "Colours for the whole app", "Behavior" vs
   "organise", "Colour rules"). US spelling in Settings and the graph panel (test pin in `view_panel.rs` updated
   deliberately). `shots/20-settings-first.png`, `shots/31-graph-view.png` -> `shots/after-graph-panel.png`.
4. **Folder icon touched its name** in the explorer (obvious at 2x): 4 px cell spacing.
   `shots/40-hidpi-sidebar.png`, `shots/tree-before-after.png`.
5. **Explorer dialogs**: a permanent blank band above the buttons (empty error label), "Already exists:
   /very/long/absolute/path" wrapped mid-path in plain text, buttons said OK, and the delete warning about
   "folders ... recursively" appeared for single files. Now: error line only when needed, danger colour, `"name"
   already exists in this folder.`, Create / Rename / Delete / Move to Trash / Delete Permanently (destructive style,
   Cancel is the default), folder text only for folders. `shots/56-newfile-dup.png`, `shots/06-delete-dialog.png` ->
   `shots/after-newfile-dup.png`, `shots/after-delete-file.png`; light theme `shots/light-delete.png`.
6. **Terminal context menu was monospace** (a menu inherits the font of the widget it is attached to).
   `shots/terminal-ctx-before-after.png`.
7. **Settings opened in the screen corner** on window managers that do not place transients (openbox, i3, xfwm); now
   centered on the main window. `shots/20-settings-first.png` -> `shots/after-settings-centered.png`.

Not fixed (not in files I own, or a design/behaviour decision):

8. (High) Second launch is swallowed on Linux, see NEEDED 1.
9. (Med) Command Palette and Quick Switcher have an empty input with no placeholder (`shots/15-palette.png`,
   `17-quickswitch.png`), see NEEDED 2.
10. (Med) Opening a binary file shows the toast "cannot open binary or non-UTF-8 file": lower case, no file name
   (`shots/54-binary-open.png`), see NEEDED 3.
11. (Med) New File creates the file but does not open it; the empty editor stays (`shots/55-newfile-error.png`).
   Wiring lives in `crates/impro` (path-created handling); behaviour decision.
12. (Low-Med) The explorer header "Notes" is a toggle for the Notes section (tooltip "Show project notes") but is styled
   flat and reads as the panel title (`shots/01-start.png`). Needs an affordance (border or chevron) or a rename.
13. (Low) Keyboard focus ring can be left as a stray teal line at a row edge after cancelling a menu/dialog with Escape
   and then clicking; it vanished on the next arrow key (`shots/07-readme.png`, `08-line-check.png`,
   `09-line-after-click.png`). Seen once, not reproduced exactly, cause not found (no DnD code in the explorer).
14. (Low) Search result group rows read "ideas.md 1 notes" (name, hit count, folder run together;
   `shots/30b-search-zoom.png`). The markup is built in `searchpanel.rs`; left alone because it mirrors the reference
   layout used by the parity scripts. A `(1)` count would disambiguate.
15. (Low) Settings > Terminal: the "Shell" row is taller than the others with uneven padding below the wrapped
   description (`shots/24-settings-terminal.png`); lives in `impro-ui` `SettingRow` height-for-width.
16. (Low) Right-dock graph view is narrow by default: the options panel covers nodes (`shots/31-graph-view.png`); the
   git graph in a split pane is cramped (`shots/32-git-graph.png`) but fine maximized (`33-git-graph-full.png`).
17. (Info) Dialogs and Settings use window-manager decorations, the main window its own title bar; expected on
   non-GNOME WMs, only the Mutter look is unchecked here. The floating status pill overlaps the last line of a Markdown
   preview (`shots/13-readme-open.png`).
18. (Info, works) HiDPI at `GDK_SCALE=2`: icons and text are crisp in window, sidebar, Settings (`shots/40`, `41`,
   `61b`); tooltips are themed (`shots/37-tips.png`); context menus, palette, quick switcher, search, tabs, git graph
   are consistent.

## NEEDED by orchestrator (files owned by others)

1. `crates/impro/src/app.rs` (the `flags` choice, ~line 19; same on the orchestrator branch): on Linux the
   application uses `FLAGS_NONE` with no `HANDLES_OPEN` / `HANDLES_COMMAND_LINE`, so `impro <path>` (and the new
   `Exec=impro %F`) while impro is already running only presents the existing window and the paths are dropped.
   Proposal: add `HANDLES_COMMAND_LINE` and open the paths in a new window from the primary instance, or use
   `NON_UNIQUE` on Linux as on Windows. Until then the desktop entry works for the first launch only.
2. `crates/impro/src/commands.rs`: call `set_placeholder("Type a command...")` for the Command Palette and
   `set_placeholder("Go to file...")` for the Quick Switcher (the API exists, `crates/impro-chrome/src/prompt.rs:414`).
3. `crates/impro-editor/src/file.rs:50,53`: replace "cannot open binary or non-UTF-8 file" by a sentence naming the file,
   e.g. `Cannot open "binary.png": it is not a UTF-8 text file.`
4. `docs/packaging.md` (file table, line ~24): add `/usr/share/metainfo/dev.impro.impro.metainfo.xml` and
   `DEBIAN/md5sums`; `README.md` (install section, lines ~59-73): the user install now also writes
   `~/.local/share/metainfo/dev.impro.impro.metainfo.xml` and refreshes `mimeinfo.cache`.
5. Optional, `crates/impro-core/src/fsops.rs:57-63`: the I/O errors embed the absolute path; the explorer now shortens the
   two common ones itself, but a structured error would be cleaner.

## Verification (what was run, results)

All builds and test runs below were repeated after `touch`ing every crate source and checking that the `Compiling ...
(path)` lines named this worktree (shared `target-linux-dev`), as requested.

- `cargo fmt --all --check`: clean. `cargo clippy -p impro-explorer -p impro-terminal -p impro-settings-ui -p impro-graph
  --all-targets -- -D warnings`: clean.
- Windows: `scripts/windows/check-stub.sh -p impro-explorer -p impro-terminal -p impro-settings-ui -p impro-graph
  --all-targets`: Finished, no errors (type check only).
- `scripts/test.sh`: impro-settings-ui, impro-explorer, impro-terminal all binaries; impro-graph `view_panel`,
  `view_interaction`, `index_behaviour`. All pass except the failures below. **Identical failures occur with the
  pre-change sources of the same crates** (checked out from the base commit, path-verified), so they come from this
  sandbox, not from the changes:
  - `impro-settings-ui` `test_nav_width_clamp_rows_and_column`, `impro-explorer`
    `test_tree_geometry_rounded_selection_hover_and_live_themes` and
    `test_search::test_result_rows_share_geometry_and_rounded_overlay_in_both_themes`: row height is 23 px here, tests
    expect 24-26 (font metrics of this container).
  - `impro-explorer` `test_permission_error_and_symlink_cycle`: runs as root, permission denial does not occur.
  - `impro-terminal` `test_resizegate` and `test_terminal_startup`: every test passes, then the process aborts
    (SIGTRAP, `GLib-GObject-CRITICAL invalid (NULL) pointer instance`) under `G_DEBUG=fatal-criticals`.
- New tests: About note vs notices (settings-ui), add-menu icons resolve (terminal, red/green checked), terminal menu not
  attached to the terminal, Settings window position, explorer text helpers (4 unit tests).
- Packaging: `bash scripts/test-install.sh` with `IMPRO_INSTALL_BINARY=/home/user/impro/target/release/impro` and with the
  fixture: all PASS; `shellcheck` on the three scripts; `desktop-file-validate`; `appstreamcli validate --no-net --pedantic`;
  `lintian` on the built `.deb`; `apt-get install`/`remove` of that `.deb`.

Not verified: Wayland (`app_id` of the window; GTK3 normally uses the program name, so `impro.desktop` should match, but I
did not run a compositor), GNOME Shell / KDE / other distros, the `.deb` on Debian 12 (T64 alternatives untested), a real
file manager's "Open With", the orchestrator branch's path-list CLI at runtime, light theme beyond the images listed.
Screenshots `01`-`61` and `70` were taken with the shared debug binary before the cross-worktree warning; the `after-*`
images were retaken from a binary whose `Compiling` lines all named this worktree. Shots `41` and `50`-`54` show a welcome
screen that is not in this branch (another worktree's `impro` crate was linked then), which does not affect the crates I
changed.
