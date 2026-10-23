# impro architecture

impro is a Rust 2021 workspace on one GTK3 gtk-rs **0.18** stack. Shared
versions live in the root `Cargo.toml`; `webkit2gtk` is 2.0. Local `impro-vte`
wraps libvte-2.91 without introducing a second GLib/GTK ABI; its nested
`impro-vte-sys` package supplies the FFI. The workspace requires Rust 1.88 or
newer, matching the minimum required by the locked `plist`, `time` and `time-core`
releases. Cargo jobs are limited to three; run one Cargo operation at a time.

## Crates and dependency direction

| Crate | Responsibility | Internal dependencies |
| --- | --- | --- |
| impro-core | Identity, 14 themes, settings, filesystem operations, pure note edits | None; no GTK/GIO |
| impro-ui | Shared CSS/provider, tokens, motion, widgets, icons, note toolbar | core |
| impro-vte | Safe Terminal/Pty and local FFI | Local sys binding |
| impro-explorer | File tree, notes, search, Git status | core, ui |
| impro-terminal | Docks, panes, terminal bar and resize handling | core, ui, vte |
| impro-editor | Tabs, gutter, source editing and syntect highlighting | core, ui, markdown |
| impro-markdown | Markdown parsing, block/table edits, live rendering, slash/selection menus | core, ui |
| impro-chrome | Titlebar, palettes, overlays and notices | core, ui |
| impro-settings-ui | Searchable settings dialog and hotkeys | core, ui |
| impro-gitviews | Git data, Git graph and commit WebKit hosts/assets | core, ui |
| impro-graph | Independent native graph engine and GTK view | core, ui |
| impro | Binary, application, window and feature integration | All feature crates, core, ui |

Direction is `impro → feature crates → impro-ui → impro-core`; VTE is a sibling
of ui. Features never depend on the application crate. Editor → Markdown is
the explicit feature dependency. Graph engine modules are GTK-free even though
the crate also contains its native GTK view. No async runtime is used.

## Application modules

| Module in `crates/impro/src` | Responsibility |
| --- | --- |
| `main.rs`, `cli.rs` | Argument parsing, logging panic hook and process exit |
| `lib.rs` | Library exports used by the binary, integration tests and QA examples |
| `app.rs` | GTK application lifecycle, initial settings snapshot, first-paint capture |
| `window.rs` | `MainWindow` composition, panel signals, project lifecycle and shutdown |
| `commands.rs` | File/View menus, accelerators, palette, quick switcher and cache warmup |
| `docks.rs` | Bottom/right dock choice, capacity, fullscreen and feature toggles |
| `views.rs` | Native graph hosting, Git graphs, commit tabs and note actions |
| `preferences.rs` | Settings dispatch, live theme/font changes and write-behind integration |
| `style.rs` | Idempotent registration of every feature's CSS |
| `startup_icons.rs` | Preparation of remaining native icon render points |

The window owns a horizontal sidebar split, a bottom vertical dock split, a
right horizontal dock split, editor, titlebar and overlay. Bottom terminals are
side by side; right terminals are stacked. All pane kinds share the configured
capacity. Graph/Git handles and commit tabs have explicit shutdown hooks.
Callbacks use weak window owners; closing the window cancels pending work.
Project epochs reject responses from a previous Open Folder operation.

## Startup sequence

1. Parse arguments before GTK; `--help` and `--version` exit without a display.
   Initialize bundled icon support and read the settings snapshot before UI event
   dispatch. The application id is `dev.impro.impro`, with window title `impro`.
2. Register feature CSS and apply the initial palette/fonts. Construct the
   complete widget skeleton: titlebar, explorer, editor, dock containers and
   overlay. Wire menus/signals and apply in-memory preferences. Initial directory
   scanning is asynchronous; it does not wait for the complete tree.
3. Create the default bottom terminal only when autostart is enabled. Custom
   shell paths are validated on a worker before their first pane is spawned.
   VTE spawning is asynchronous. No graph index or WebKit view is created here.
4. Prepare the first-frame icons, show the window and let GTK allocate/draw the
   complete layout. `layout_ready()` gates the first-paint screenshot milestone.
5. After that draw, warm the command palette at LOW idle priority. Realisation
   and measurement proceed in slices, one native widget operation per dispatch;
   `OverlayHost::cache_prompt` retains the hidden prompt for fast reopening.
   The idle source removes itself when finished or the window closes.

Expensive features are lazy: quick-switcher enumeration starts on demand,
syntect loads syntax data in a worker on first use, graph workers start on graph
open, and Git pages share a WebKit process created on first use. The skeleton
and required preference wiring are synchronous; background results and optional
work arrive through channels or bounded idle slices.

### First-frame icons

`crates/impro-ui/assets/icons` contains exact raw RGBA pixels at 1x and 2x for
startup symbolic icons. `impro_ui::icons` supplies these directly for supported
palette/state/scale combinations. Native GTK rendering remains the fallback for
other themes, colours and translucent states. This avoids the first-show SVG
loader's process and file-loading costs without changing the captured pixels.
The application registers a small CSS rule preventing GTK from dimming the
already coloured raw icons twice. `startup_icons.rs` prepares tree cells and
window decoration render points while preserving semantic icon names.

## CSS registry and theme flow

`impro::style::init()` calls every feature's `init()` once, before applying the
first theme. Each registers a named CSS builder in `impro-ui`; the shared layer
never imports features. Builders expose `css(&UiColors) -> String`, render their
own templates and are assembled in registry name order with shared base CSS.
Stable names make repeat registration harmless and avoid dependency cycles.

`impro_ui::render_css` resolves palette expressions in templates.
`style::{TOKENS,build_css,build_overrides,with_accent}` build shared geometry and
overrides. `apply_theme(theme, accent, ui_font, editor_font)` validates CSS before
replacing the shared provider and GTK dark preference. Strings are cached by
full palette/font contents and invalidated when builders register. Existing
widgets update through that provider. Syntax tags, terminal ANSI colours and
Git/native graph styles also receive explicit theme/font updates from the host.
`current_theme()` supplies the effective palette after accent overrides.
See [design-tokens.md](design-tokens.md) for geometry and motion constants.

## Data, settings and Markdown

`impro-core` contains pure themes, validation, note transformations and filesystem
helpers. Themes have UI, syntax, terminal and chrome maps; syntax uses 26 token
roles. Editor highlighting maps syntect scopes to these roles off-thread.
Filesystem operations return structured errors, protect project/filesystem roots,
and do not silently replace a failed trash operation with permanent deletion.

Settings resolve to `$XDG_CONFIG_HOME/impro/settings.json`, or
`~/.config/impro/settings.json`. Defaults, ranges and choices are embedded in
`crates/impro-core/src/{defaults,limits,choices}.json`. Invalid input falls back
to defaults or clamps to bounds. Saves preserve unknown keys and atomically
replace a 0600 temporary file after flush/fsync; the parent directory is synced
best-effort. Changes apply immediately in memory; write failures produce notices.
`graph` and `gitgraph` blobs have feature-owned normalization; the settings dialog
also exposes validated `gitgraph_*` keys. Graph controls synchronize across views.

Notes live in `.impro/Notes`, created lazily. Editor and Markdown offsets count
Unicode characters, matching GtkTextBuffer rather than UTF-8 bytes. Live Markdown
renders around the active source line; source remains the authoritative document
for saving, find, copy, undo and cursor reporting. Source mode is available through
Ctrl+Shift+M. Table grids are built lazily through `TableHost`, retain stable block
marks, forward history to the editor and fill visible cells between loop yields.
Slash commands and the selection toolbar share pure note-edit commands from core.

## Threading, borrows and safety

Only GTK's main thread creates or updates GTK objects or uses the thread-local
CSS registry. Owned paths, text, theme/settings data and search/index results can
cross workers. Use `std::thread` and GLib main-context channels/invoke, cooperative
cancellation and epochs. Directory/file I/O, Git commands, search, graph indexing
and syntax lexing belong on workers. Apply results in small slices: the design
budget is at most 4 ms of application work per batch. A slice does not bound GTK's
subsequent allocation/paint time.

The native graph currently advances allocation-free layout steps in bounded
frame-clock callbacks on GTK's thread; indexing/building and preference writes
are off-thread. It reuses flat arrays and a Barnes–Hut tree, and caps catch-up
steps. Keep these steps bounded; do not describe them as a background layout
worker. See [graph-view.md](graph-view.md) for the renderer and idle lifecycle.

Never retain a `RefCell` borrow across GTK calls, signal emissions, callback
invocation or builder invocation. Copy, clone or take the state out first:
selection changes, CSS reloads and widget removal can synchronously re-enter.
Use `Cell` for scalar flags, weak owners for callbacks and `try_borrow*` where
re-entry is expected. User documents, paths and JSON must return errors rather
than panic. The binary installs a logging panic hook.

Idle has no recurring application timers. Debounce sources exist only for pending
changes; animation uses a shared frame-clock callback only while active. Unmapping
or destroying a widget cancels its animation. A hidden graph pauses; a settled
graph detaches its tick callback. VTE or WebKit may perform their own housekeeping.
Reduce motion, `IMPRO_REDUCE_MOTION=1`, or disabled GTK animations finish new tweens
immediately. Notice lifetimes use one-shot timers while notices are visible.

## Build and test

```sh
cargo build --workspace
G_DEBUG=fatal-criticals bash scripts/test.sh
G_DEBUG=fatal-criticals bash scripts/test.sh -p impro-core
cargo fmt --check
cargo clippy --workspace -- -D warnings
node crates/impro-gitviews/tests/js/index.js
bash scripts/test-install.sh
```

`scripts/test.sh` exports `GDK_BACKEND=x11 GIO_USE_VFS=local` and runs
`xvfb-run -a dbus-run-session -- cargo test`. It defaults to `--workspace`;
`-p`/`--package` selects individual crates. Pure core tests also work without a
display. GTK suites serialize UI closures onto their dedicated GTK thread;
WebKit integration runs in separate process hosts. Feature `VERIFICATION.md`
files document test groups and composition contracts; the final window's host
cases are in `crates/impro/tests` and `window_qa`.

## QA helpers

Use release binaries for timing. Create screenshot parent directories first;
PNG encoding is excluded from `first-paint-ms`. The screenshot helper captures
only after the complete-layout draw has flushed and exits nonzero on failure.
`IMPRO_QA_TIMING=1` prints phase timings; `IMPRO_QA_TRACE=1` traces to stderr.

```sh
cargo build --release -p impro
mkdir -p .ceo/qa
IMPRO_QA_SCREENSHOT=.ceo/qa/window.png IMPRO_QA_TIMING=1 GDK_BACKEND=x11 xvfb-run -a dbus-run-session -- target/release/impro /tmp
cargo build --release -p impro --example window_qa --example startup_floor
G_DEBUG=fatal-criticals GDK_BACKEND=x11 xvfb-run -a dbus-run-session -- target/release/examples/window_qa performance
```

`window_qa` supports `git` (default, repository integration), `performance`,
`capture` and `controls`. `IMPRO_QA_CONTROLS_DIR` chooses control-state capture
output; `IMPRO_QA_QUIET_FOCUS` controls capture focus handling. Inspect the example's
arguments before automating captures. `startup_floor` supports `empty`, `style`,
`chrome`, `glyphs`, `native` and `decorations`; supply `IMPRO_QA_SCREENSHOT`.

`crates/impro/examples/startup_bench.py` interleaves ten or more fresh release
processes for before/after workspace, empty-window and chrome comparisons.
`--before` is a saved baseline directory containing `impro` and `startup_floor`;
`--after` defaults to `target/release` with `examples/startup_floor`. For example,
after preparing that baseline under `.ceo/qa/before`:

```sh
GDK_BACKEND=x11 xvfb-run -a dbus-run-session -- python3 crates/impro/examples/startup_bench.py --before .ceo/qa/before --after target/release --root /tmp --runs 10 --output .ceo/qa/startup.json
```

It records a separate initial session-service warmup and excludes PNG encoding.
Fresh processes do not mean a cold filesystem cache. Preserve identical project,
settings, fonts, display scale and window dimensions for comparisons.
`scripts/startup-bench.py` is a separate process-launch-to-X11-map harness with an
optional read-only Python reference root; mapping and completed paint are different
milestones. Run it under Xvfb/D-Bus, with `--runs 10 --output .ceo/qa/map.json`.

| Feature | QA examples (Cargo package / example) |
| --- | --- |
| Graph | `impro-graph` / `graph_demo`, `graph_bench` |
| Chrome | `impro-chrome` / `chrome_demo` |
| Markdown tables | `impro-markdown` / `table_demo` |
| Editor and Markdown host | `impro-editor` / `editor_demo`, `editor_perf` |
| Explorer | `impro-explorer` / `explorer_demo`, `perf` |
| Terminal | `impro-terminal` / `dock_demo` |
| Settings | `impro-settings-ui` / `settings_demo` |
| Git pages | `impro-gitviews` / `git_demo`, `git_perf`, `webkit_host_qa` |

For example, `cargo run -p impro-graph --example graph_demo -- --vault /tmp` opens a native
graph demo on a display. `table_demo` accepts a theme id and optional PNG path;
`editor_demo` accepts `IMPRO_DEMO_THEME`, `IMPRO_DEMO_FIXTURES` and
`IMPRO_QA_SCREENSHOT`. Graph benchmark image output uses `IMPRO_GRAPH_BENCH_IMAGES`;
chrome's cache benchmark uses `IMPRO_CHROME_BENCH_CACHE=1` with its ignored prompt
benchmark test. These are QA controls, not user renderer switches.

Measured startup/input/terminal/graph results and the scope of the pixel comparison
are in the README's Performance section. They are recorded results from the
four-core VM, not timings inferred from unit-test execution.
