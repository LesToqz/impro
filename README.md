# impro

A small Linux IDE built in Rust with GTK 3, gtk-rs, VTE 2.91 and syntect.
It combines a project explorer and search, tabbed editor and live Markdown notes,
terminal docks, a native project graph, read-only Git history, palettes and settings.

## Install and run

Build requirements: Rust **1.80 or newer**, a C toolchain, pkg-config and the
GTK3, VTE and WebKit2GTK development libraries. On Debian/Ubuntu:

```sh
sudo apt install build-essential pkg-config libgtk-3-dev libvte-2.91-dev libwebkit2gtk-4.1-dev
cargo build --release -p impro
cargo run --release -p impro -- [folder]
```

`[folder]` denotes an optional directory argument; omit it to open the current
directory. The executable is `target/release/impro`. For command-line information:

```sh
cargo run --release -p impro -- --version
cargo run --release -p impro -- --help
```

Help begins `usage: impro [-h] [--version] [folder]`. Bad arguments and invalid
folders exit with status 2; help and version need no display.

Runtime requires GTK3 and VTE 2.91 (native VTE version 0.72 or newer).
WebKit2GTK 4.1 supplies the Git pages: no WebKit process starts until a Git graph
or commit page opens. The binary links the WebKit library, so the library must
be installed even when those pages are unused; the lazy boundary is page/process
creation, not availability of the shared library. The native project graph needs
no WebKit process. Git must be available for repository features; shells run locally.

Install the desktop launcher, executable and icon for your user with:

```sh
bash packaging/install.sh
bash packaging/install.sh --uninstall
```

Installation builds release and copies three files under `~/.local`: `bin/impro`,
`share/applications/impro.desktop`, and `share/icons/hicolor/scalable/apps/impro.svg`.
Ensure `~/.local/bin` is on PATH. Uninstall removes those files, preserves settings
and documents, and can be repeated. Installation also supports a prebuilt binary:

```sh
IMPRO_INSTALL_BINARY="$PWD/target/release/impro" bash packaging/install.sh
```

`HOME` selects the user installation root, including for uninstall. No elevated
privileges, prefix option or staging-directory variable are needed for isolation.

## Layout and editing

impro uses compact surfaces,
one adjustable accent colour, rounded tabs, instant hover/selection feedback and motion only
where something moves (sliding toggles, notices, terminal panes).

One top bar contains File/View menus, the title **impro**, a settings cog and the
compact window buttons; the right-dock toggle sits beside the cog (Ctrl+Alt+`). The explorer sidebar starts 300 px wide.
The explorer on the left has a collapsible Notes section below the file tree, with a draggable divider.
Its Notes header button shows or hides the section; the tree stays visible above it.
The explorer supports searching (see Search in Files), creating
files/folders, renaming, deleting and refreshing. Renames retarget open files; deletions mark affected tabs deleted.
File → Open Folder changes the explorer root and the directory for new shells;
existing shells keep their directories. Deletion is permanent by default; Settings →
File explorer can move deleted items to the system trash instead.

Git colours in the explorer mark untracked and staged new files green, modified
and renamed files orange, and conflicts red. Folders inherit their strongest
descendant status, including while collapsed.
Decorations follow the theme and refresh after saves, file operations and window focus.

The editor has tabs, line numbers, current-line and matching-bracket highlights,
undo/redo, indentation, line comments and zoom via Ctrl+Plus/Minus or Ctrl+wheel;
Ctrl+0 resets to the editor text size chosen in Settings (11 pt by default).
Ctrl+F opens the in-file find bar (case-insensitive, Enter finds the next match).
Unsaved files prompt on close/quit.
Syntax highlighting detects filenames and shebangs: examples include Python,
JavaScript/TypeScript/JSX/TSX, HTML/XML, CSS, JSON, YAML, TOML, shell, C/C++, Rust,
Go, Java, SQL, Markdown, Dockerfile and Makefile through syntect.
Highlighting is skipped above 1.5 MB or for lines longer than 20,000 characters.
The editor accepts regular UTF-8 text files up to 5 MiB.

## Graph view

Graph view draws the project's notes and source files as an interactive network.
Open it from either dock's **+ → Graph view** chooser, **View → Graph view**,
**Ctrl+Alt+G**, or the command palette's **Graph view: Open graph view** command.
The menu/shortcut/palette reuse the graph in the right dock. Each dock supports
one graph alongside terminals; pane focus, fullscreen and dock toggling also work
with graph panes. Ctrl+P/Ctrl+O, Ctrl+Shift+P and Ctrl+, remain available while the
graph has focus.

The index understands wikilinks, aliases, heading/block link targets, embeds,
relative Markdown links, inline/frontmatter tags, referenced attachments,
unresolved targets and supported code imports. Heading/block targets connect to
the containing file. Click a file node to open it in the editor, a tag to search
files, or an unresolved node to create a note with a simple name in the project
root. Context actions reveal files and copy absolute paths; the open modes
currently open or focus a normal editor tab.

The controls panel has **Filters / Groups / Display / Forces** sections. Filters
remove non-matching nodes; **Hide missing targets** hides unresolved targets.
Tags and attachments start hidden, orphans and source files start visible.
Groups use the first matching non-empty query. Display adjusts labels, node/line
sizes and arrows; Forces adjusts attraction, repulsion and link distance.
Panel state, zoom, groups and graph preferences persist in the `graph` object
of the settings file. The graph follows the current theme, accent and Reduce motion
setting, and the active editor file is highlighted. Saved files, unsaved note/code
edits (after 400 ms), explorer operations and Open Folder update the graph live.

**View → Local graph**, **Ctrl+Shift+Alt+G**, or **Graph view: Open local graph**
shows the active file and its neighbors one link away. It follows editor tab
changes; the panel adjusts depth and incoming/outgoing links. Open Graph view
again to return to the global graph. The palette also provides
**Graph view: Rebuild graph** and **Graph view: Reset camera**. Reset camera
recentres at zoom 1 and restarts the layout.

The graph is a native GTK DrawingArea with software rasterised connections,
cached Cairo surfaces and Pango labels. Indexing and graph building run on workers;
the frame clock stops when settled and pauses while hidden. The view caps large
graphs at 3,000 nodes and reports truncation. Note-only mode is also available.
The app's Local graph command starts at one hop; the panel supports depths 1–6
and the engine's default preference is two hops.

Drag the background to pan, drag nodes to move them, and scroll to zoom around
the pointer. The toolbar offers fitting, reset and **Replay project history**:
files appear in timestamp order followed by their outgoing references.
See [Graph view](docs/graph-view.md) for indexing, queries, controls and persistence.
Group drag-reordering, touch pinch and hover document previews are not provided.

## Git graph

Git graph draws the repository's history with Horizontal / Vertical view icons to switch the timeline direction. Open it from
either dock's **+ → Git graph** chooser, **View → Git graph**, **Ctrl+Alt+H**,
or **Git graph: Open git graph** in the command palette. The menu, shortcut and
palette focus an existing Git graph, or open one below the editor (on the right
if the bottom dock is full). Each dock permits one Git graph
alongside Graph view and terminals. The shortcut also works from a terminal.

The default branch has a main line, with coloured branch lanes and labels,
merge callouts, a HEAD pointer, tag and remote chips, and a dashed tail showing
the number of uncommitted changes. Deleted branch names are inferred from merge
messages when possible. Hover a commit dot for its hash, message and time;
click it to open a read-only commit tab at the right end of the editor tabs.
Opening the same commit again selects its tab. Parent chips open more commit
tabs; file names open the working-tree file when it still exists. The active
commit tab is selected in every Git graph, and tabs can outlive their graph pane.

Drag to pan, scroll to zoom (horizontal scrolling pans), and use the toolbar's
**Fit**, **HEAD**, **Refresh**, **Tags** and **Remote** controls. Arrow keys move
between commits and lanes, **Home/End** go to the oldest commit/HEAD, **Enter**
opens the focused commit, and **Esc** dismisses its tooltip. The palette also
provides **Git graph: Refresh**, **Git graph: Jump to HEAD**, and
**Git graph: Close commit tabs**. Commit tabs have file collapse/expand, unified
and split diff controls, syntax colours, and expandable hidden context.

Repository monitors refresh history after Git changes; returning focus to the
window also refreshes it. Refresh preserves open commit tabs. Open Folder closes
the old repository's commit tabs and updates the graph. Both pages follow the
current theme, accent, fonts and Reduce motion setting; hidden graphs pause.

History defaults to 1,500 commits (configurable from 100 to 10,000). All Git operations are read-only and local:
there is no network access, fetch, checkout, staging or commit creation. A plain
folder or repository without commits shows a friendly empty state. The Git graph and commit pages use WebKit2GTK 4.1 and share a lazily created
web process. Closing the final page shuts down that process.

## Search in Files

The search box sits at the top of the explorer, directly under the EXPLORER header.
Press **Ctrl+Shift+F** (or View → Search in Files) to focus it: this shows the
explorer if it is hidden and leaves terminal fullscreen first. If the editor has
a non-empty, single-line selection of at most 200 characters, it is copied into the
box and searched at once; otherwise the previous query stays, selected.

Typing replaces the file tree with results after a short pause; the search runs in
the background, so the UI stays responsive. Clearing the box or pressing **Esc**
restores the tree and returns focus to the active editor tab (or to the tree when
no tab is open). Enter searches immediately and moves to the results; Down moves
from the box to the results.

- **Current file first.** Matches in the file of the active editor tab are listed
  first (marked "current file"), then the other files in path order, then a
  **File names** group with files whose name contains the query. Switching tabs
  re-sorts the list.
- **Unsaved edits are searched**, not just the saved text on disk.
- **Click or Enter** on a result opens (or focuses) the file, selects the match,
  scrolls it to roughly the middle and focuses the editor. A Markdown tab showing
  the preview is switched to its source first. Clicking a file header collapses it.
- **Aa** toggles match case (off: case-insensitive); **.\*** toggles Rust regular
  expressions (an invalid pattern shows its error; empty matches are ignored). File
  name matching is always a plain, case-insensitive substring. Look-around and
  back-references are unsupported; use `\z` for end-of-input.
- **Limits.** At most 200 matches per file and 1000 in total (the summary then reads
  "Showing the first N results"). Only UTF-8 text files up to 1 MiB are searched;
  binary files (a NUL byte in the first 8 KB), undecodable files and symlinks are
  skipped. Previews of very long lines are cut to 200 characters around the match.
- **Ignored folders:** `.git .hg .svn __pycache__ node_modules .venv venv .mypy_cache
  .pytest_cache .tox .idea .ceo`. Other hidden files are searched.
- Results follow File → Open Folder, and are re-run after refresh, rename, delete
  and create, so they never point at files that are gone. While the box or results
  have focus, the explorer's F2 / Delete / F5 / Ctrl+Shift+N do nothing to the tree.

## Quick Switcher and Command Palette

Both open as a box near the top of the window (no dimming; click outside or press
**Esc** to close, which returns focus to where it was). Type to filter with fuzzy
matching (matched letters are bold); **Up/Down** move, **Enter** chooses.

- **Quick Switcher** — **Ctrl+P** or **Ctrl+O** (File → Quick Switcher…). Lists the
  files of the open folder (the Search in Files ignored folders are skipped; hidden
  files follow *Show hidden files*; at most 20,000 files). With an empty query the
  recently active and open files come first. Enter opens the file in the editor.
- **Command Palette** — **Ctrl+Shift+P** (View → Command Palette…). Every File and
  View menu action plus Open settings, Show hotkeys, Next theme, Toggle Markdown
  preview and editor zoom in/out/reset, each with its shortcut shown as a chip.
  Recently run commands are listed first.

Ctrl+P, Ctrl+O and F1 are left to a focused terminal (readline, vim, mc and htop
use them); Ctrl+Shift+P works everywhere. Notices (small toasts at the top right)
appear only for things with no other feedback, such as a settings file that could
not be written; click one to dismiss it.

## Settings

The header-bar cog or **Ctrl+,** opens the Settings window (there is no File menu
entry); **F1** opens it on the Hotkeys page. It is non-modal: changes apply
immediately to open tabs, the explorer and both terminal docks, and are saved at
once. Search the left pane to filter settings across pages; Esc clears the search,
then closes the window.

Pages are Appearance, Editor, File explorer, Notes, Terminal, Graph view, Git graph,
Hotkeys and About. Each feature page begins with an Enable switch; switching it
off dims its remaining settings. Notes includes live Markdown preview, slash commands,
toolbar and sorting. Terminal includes a shell override and a per-dock pane limit
(default 6, range 1–8), counting terminals, graph and Git graph panes together.
Graph view includes display preferences; Git graph includes orientation, history
limits, branch visibility and timestamp formatting.

| Key | Default | Values / bounds |
| --- | --- | --- |
| `theme` | `"impro-dark"` | theme id (default: impro Dark) |
| `terminal_font_size` | `11` | 7–32 |
| `editor_font_size` | `11` | 7–32 |
| `explorer_font_size` | `10` | 8–24 |
| `feature_explorer` | `true` | boolean |
| `feature_terminal` | `true` | boolean |
| `feature_graph` | `true` | boolean |
| `feature_gitgraph` | `true` | boolean |
| `feature_notes` | `true` | boolean |
| `explorer_git_colors` | `true` | boolean |
| `explorer_folders_first` | `true` | boolean |
| `explorer_compact` | `false` | boolean |
| `terminal_max_panes` | `6` | 1–8 |
| `terminal_shell` | `""` | executable path; empty = default shell |
| `terminal_copy_on_select` | `false` | boolean |
| `terminal_bell` | `false` | boolean |
| `graph_default_type` | `"global"` | `global`, `local`, `note` |
| `gitgraph_orientation` | `"horizontal"` | `horizontal`, `vertical` |
| `gitgraph_max_commits` | `1500` | 100–10000 |
| `gitgraph_show_remotes` | `true` | boolean |
| `gitgraph_show_tags` | `true` | boolean |
| `gitgraph_time_format` | `"relative"` | `relative`, `absolute` |
| `notes_slash_commands` | `true` | boolean |
| `notes_toolbar` | `true` | boolean |
| `notes_selection_toolbar` | `true` | boolean |
| `notes_sort` | `"name"` | `name`, `modified` |
| `notes_open_after_create` | `true` | boolean |
| `accent` | `null` | hex colour or null |
| `reduce_motion` | `false` | boolean |
| `ui_font` | `""` | font family; empty = default |
| `editor_font` | `""` | font family; empty = default |
| `line_numbers` | `true` | boolean |
| `highlight_line` | `true` | boolean |
| `md_live_preview` | `true` | boolean |
| `md_readable_width` | `true` | boolean |
| `indent_tabs` | `false` | boolean |
| `indent_width` | `4` | 2–8 |
| `auto_pair` | `true` | boolean |
| `show_hidden` | `true` | boolean |
| `confirm_delete` | `true` | boolean |
| `delete_mode` | `"permanent"` | `trash`, `permanent` |
| `terminal_cursor` | `"block"` | `block`, `ibeam`, `underline` |
| `terminal_blink` | `true` | boolean |
| `terminal_scrollback` | `10000` | 1000–100000 |
| `terminal_autostart` | `true` | boolean |

Graph display, forces, filters, groups and zoom are stored in the `graph` object;
see [Graph view](docs/graph-view.md). The embedded defaults and validation rules
are in `crates/impro-core/src/{defaults,limits,choices}.json`.

Settings are stored as JSON in `~/.config/impro/settings.json`
(`$XDG_CONFIG_HOME/impro/settings.json` when set), written atomically with mode
0600; unknown keys are preserved. A missing, unreadable or invalid file falls back
to defaults; invalid values use their default and numbers are clamped. If the file
cannot be written, the change still applies for the session and an error notice
says so. Zooming with Ctrl+Plus/Minus/wheel is temporary: the saved size is what
Ctrl+0 returns to and what new sessions start with.

**Reduce motion** (or `IMPRO_REDUCE_MOTION=1`, or GTK's *enable animations* turned
off) makes toggles, notices and terminal pane changes instant.

## Markdown live preview

Files ending in `.md` or `.markdown` open in one editable live preview: type directly into the document, with Markdown markers revealed on the caret's line. Headings, lists, clickable tasks, quotes, local images, links, highlighted code with Copy buttons, and pipe tables render around your edits; tables reveal their source when entered. Save, undo, find, and line/column positions use the exact Markdown source. Click the status language label, use **Ctrl+Shift+M**, or choose **Markdown: Toggle source mode** in the command palette for plain source with line numbers. Readable width centres a 700 px column. Files larger than 1 MiB open in source mode.

## Terminal docks

The bottom terminal dock starts open with side-by-side panes; the right dock
starts hidden and stacks panes vertically. Both can be open together. Drag the
bottom divider to the very top or the right divider to the far left to collapse
the editor area. Hiding a dock preserves its shells; showing an empty dock
starts a new shell.

- Drag between panes to resize them left/right in the bottom dock or up/down in
  the right dock; double-click a divider to restore equal sizes.

Each terminal has its own header, labelled bash1…bash6 in order (or the selected
shell's name), with fullscreen and delete icons. The **+** in the dock's top-right
corner opens a chooser for a terminal, graph view or Git graph. Each dock allows
six panes by default, configurable from 1 to 8 in Settings → Terminal. All pane
kinds share this limit; adding a seventh shows **Cannot add (limit 6)**.
Adding/deleting terminals animates them;
closing the last hides the dock. Split inserts after the focused pane.
Right-click offers Copy, Paste, Select All, Clear, Split Terminal and Kill Terminal.

**Turning features off:** Settings lets you switch the file explorer, terminal,
graph view, Git graph and notes off independently. Changes apply immediately.
Disabling a pane feature closes its panes and stops its shells or web views;
re-enabling restores those panes. The chooser only offers enabled features.
Turning off the explorer also hides its notes and file search panels. Turning
off notes removes note insert toolbars while keeping open documents editable.

A header's fullscreen icon, or View → Toggle Terminal Fullscreen, fills the whole
window content area with that terminal, keeping the top bar visible. Toggle again
to restore the layout. The menu command uses the focused visible dock, otherwise
the bottom, otherwise the right; if both are hidden it opens the bottom dock.
Layout commands restore fullscreen first. Esc is intentionally not bound to
terminal fullscreen. Resizing is applied once a drag settles. Output already on
screen is not re-flowed, preventing duplicated prompts.

## Note commands

In live notes under `.impro/Notes`, type `/` at a line start or after a space to
open the command menu. Filter by typing, choose with arrows and Enter or Tab,
and dismiss with Esc. Commands include plain text, headings 1–3, bullet and
numbered lists, to-do lists, quotes, dividers, code and Mermaid blocks, tables,
links, today's ISO date, callouts, and images. A command is one undoable edit;
slash commands can be switched off in Notes settings.

## Themes

Pick a theme in Settings → Appearance for a live switch, or press **Ctrl+Alt+T**
(also a Command Palette entry) to cycle to the next one, wrapping after the last.
The accent colour setting recolours the accent of any theme.
Editor syntax, chrome, the explorer and both terminal docks change without restarting
shells; new tabs and terminals inherit the theme. The selection is saved in the
settings file described above. Fresh installs, and missing, invalid or unreadable
settings, use **impro Dark**; a theme you saved before is kept.

- **impro Dark** — blue-grey surfaces with a muted green accent; the default.
- **impro Light** — the light counterpart: green-tinted paper with darker ink.
- **Dark+** — charcoal with familiar blue and purple syntax accents.
- **Light+** — white editor with blue and purple accents.
- **Monokai** — warm dark grey with bright pink, green and yellow syntax.
- **Dracula** — purple-grey with pastel pink, purple and green.
- **Solarized Dark** — deep teal with restrained Solarized accents.
- **Solarized Light** — cream with Solarized accents.
- **Nord** — cool blue-grey with icy blue accents.
- **One Dark** — charcoal-blue with muted colourful syntax.
- **GitHub Dark** — near-black with blue accents.
- **impro Midnight** — deep navy with cyan accents.
- **impro Ember** — warm brown with amber accents.
- **impro Paper** — warm paper with teal accents.

## Shortcuts

These are app bindings; GTK text editing and shell bindings also apply.

| Shortcut | Command / focus |
| --- | --- |
| Ctrl+S | Save current file; terminal focus forwards Ctrl+S to the shell (XOFF) |
| Ctrl+Shift+S | Save all files |
| Ctrl+Q | Quit the app |
| Ctrl+P / Ctrl+O | Quick Switcher (not while a terminal has focus) |
| Ctrl+Shift+P | Command Palette (everywhere, terminals included) |
| Ctrl+, | Open Settings |
| F1 | Open Settings on the Hotkeys page (not while a terminal has focus) |
| Ctrl+B | Toggle explorer |
| Ctrl+Shift+F | Search in Files: focus the explorer search box, pre-filled from a one-line selection |
| Ctrl+` | Toggle bottom terminal dock and focus it when shown |
| Ctrl+Alt+` | Toggle right terminal dock and focus it when shown |
| Ctrl+Shift+5 | Split terminal in focused dock, otherwise bottom |
| Ctrl+Shift+F11 | Toggle whole-window terminal fullscreen |
| Ctrl+Alt+T | Next theme, wrapping after the last |
| Ctrl+Alt+G / Ctrl+Shift+Alt+G | Open global / local project graph |
| Ctrl+Alt+H | Open or focus Git graph |
| Ctrl+W | Close tab (editor) |
| Ctrl+Z / Ctrl+Shift+Z | Undo / redo (editor) |
| Ctrl+F | Find in the current file (editor); Enter finds next match |
| Ctrl+Shift+M | Toggle Markdown live preview / source (`.md`, `.markdown` tabs) |
| Esc | Close the Quick Switcher / Command Palette; in the explorer search box or results: clear the query, back to the editor |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous editor tab |
| Ctrl+PageDown / Ctrl+PageUp | Next / previous editor tab |
| Ctrl+/ | Toggle line comments (Python, shell, TOML, YAML, conf) |
| Tab / Shift+Tab | Indent / unindent (editor) |
| Enter | Newline with automatic indentation (editor) |
| Ctrl+Plus (or Ctrl+=) / Ctrl+Minus / Ctrl+0 | Increase / decrease / reset editor font to the size set in Settings |
| Ctrl+wheel | Zoom editor |
| Ctrl+Shift+T | Split terminal (terminal focus) |
| Ctrl+Shift+W | Kill focused terminal (terminal focus) |
| Ctrl+Alt+Left / Right | Previous / next terminal, wrapping within its dock |
| Ctrl+Alt+Up / Down | Previous / next terminal in the stacked right dock |
| Ctrl+Shift+C / V | Copy / paste (terminal focus; the editor uses Ctrl+Shift+M for Markdown source mode) |
| Ctrl+Shift+Plus (or Ctrl+Shift+=) / Minus / 0 | Increase / decrease / reset terminal font (hold Ctrl+Shift for each) |
| F2 / Delete / F5 | Rename / delete / refresh (explorer tree; ignored while the search box has focus) |
| Ctrl+Shift+N | New folder (explorer tree) |

## Performance

These measurements were supplied from release builds on a four-core VM under
Xvfb. Startup used ten fresh processes (fresh process startup, not a flushed OS
page cache); times are medians unless a range or percentile is given. They are
measurements of those fixtures and that machine, rather than universal guarantees.

| Measurement | Result |
| --- | --- |
| Main entry to completed first paint of the full window | **201 ms**, 174–225 ms across ten processes |
| Python original, same startup comparison | 620 ms; impro before the icon work was 396 ms |
| Empty GTK window floor | about 45 ms |
| Keyboard to completed draw: open file / switch tab / toggle dock | 16 / 16 / 15 ms |
| Idle over ten seconds | one CPU tick; no recurring application timers |
| New terminal pane first paint | 14 ms median; 28 ms worst in a six-pane burst |
| Settled graph draw, 3,000 nodes, zoomed in | 3.3 ms median / 6.5 ms p95 |
| Graph open to first frame, 600 files | 36–55 ms |
| Layout tick, 3,000 nodes | 1.7 ms |
| Explorer search row application | 2 ms per slice |
| Full-window pixel comparison | zero changed pixels across dark/light, 1x/2x and normal/hover/pressed/insensitive captures |

The design rule is to keep GTK's main thread responsive: cancellable background
I/O, search and lexing, small result batches, lazy syntect loading, graph workers
only when the graph opens, and no WebKit process until a Git page opens. Animation
callbacks detach when settled. Raw first-frame icons avoid synchronous SVG loader
startup. Settled graph measurements depend on surface caching and label detail;
they do not measure every moving scene. Full-window parity captures do not imply
that every document, graph or Git page has been compared. For reproduction, see
[docs/ARCHITECTURE.md QA helpers](docs/ARCHITECTURE.md#qa-helpers).

## Tests and limitations

Install Xvfb and D-Bus session support, then run:

```sh
bash scripts/test.sh
bash scripts/test.sh -p impro-core
bash scripts/test-install.sh
```

The test wrapper runs Cargo under Xvfb and a private D-Bus session. Without a
package selector it tests the workspace; `-p` selects just that crate. The install
test uses a temporary HOME and a release binary when present, or a small executable
fixture when absent; `IMPRO_INSTALL_BINARY` overrides the source executable.
It checks desktop metadata, repeated install/uninstall, copied contents, modes,
and help from outside the repository, without building. Run the GTK suites with
`G_DEBUG=fatal-criticals` for strict diagnostics, as shown in the architecture guide.

No GtkSourceView, persisted window geometry/layout/session, or Wayland-specific
testing. GTK3 CSS has no tree indent guides or concave active-tab corners;
menu/tooltip shadows depend on the compositor. Ctrl+P in the explorer opens the
Quick Switcher. Ctrl+Q quits even with terminal focus. Search uses Rust regex
syntax rather than Python regex syntax.

## Environment variables

| Variable | Purpose |
| --- | --- |
| `IMPRO_REDUCE_MOTION=1` | Finish interface animations immediately |
| `IMPRO_QA_SCREENSHOT=path.png` | Capture the first complete window paint and exit; parent directory must exist |
| `IMPRO_QA_TIMING=1` | Print startup phases and `first-paint-ms` with the screenshot helper |
| `IMPRO_QA_TRACE=1` | Trace startup phases to stderr |
| `IMPRO_EDITOR_TRACE=1` | Print editor/highlighting diagnostic timings |
| `IMPRO_INSTALL_BINARY=/absolute/path/impro` | Install an existing executable without rebuilding |

QA examples also accept capture and fixture variables documented in
[Architecture](docs/ARCHITECTURE.md#qa-helpers). Graph-related identity constants
are reserved for diagnostics; they are not renderer-selection options. Graph view
always uses its native renderer.

## Project layout

| Path | Responsibility |
| --- | --- |
| `crates/impro` | Binary and application/window integration |
| `crates/impro-core` | Settings, themes, filesystem and note logic; no GTK |
| `crates/impro-ui` | Shared widgets, CSS registry, motion, icons and note toolbar |
| `crates/impro-vte` | GTK3-compatible VTE wrapper and local sys binding |
| `crates/impro-explorer` | File tree, notes list, project search and Git status |
| `crates/impro-editor` | Tabs, source editing, gutters and syntect highlighting |
| `crates/impro-markdown` | Live Markdown, block/table editing, slash menu and selection toolbar |
| `crates/impro-terminal` | Bottom/right terminal panes, splits and resize handling |
| `crates/impro-chrome` | Titlebar, palettes, overlay and notices |
| `crates/impro-settings-ui` | Searchable settings dialog and hotkeys |
| `crates/impro-graph` | Native graph engine, layout, renderer and controls |
| `crates/impro-gitviews` | Git history/diff data and embedded WebKit pages |
| `docs` | Architecture, graph behaviour and design tokens |
| `packaging` | User installer, desktop entry and icon |
| `scripts` | Test wrappers and startup measurement helper |

See [Architecture](docs/ARCHITECTURE.md) for dependency direction and QA helpers,
and [Design tokens](docs/design-tokens.md) for shared geometry, colour and motion.
