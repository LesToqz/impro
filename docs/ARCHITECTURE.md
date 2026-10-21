# impro architecture and worker integration contracts

The workspace targets Rust 2021 and one GTK3 gtk-rs **0.18** stack. Shared
versions live in the root `Cargo.toml`; `webkit2gtk` is 2.0. Local `impro-vte`
wraps libvte-2.91 without introducing a second GLib/GTK ABI. It includes a nested
`impro-vte-sys` package; do not replace it with a third-party VTE binding.
Cargo jobs are limited to three. Run only one Cargo operation at a time.

## Crates and dependency direction

| Crate | Owns | Depends on |
| --- | --- | --- |
| impro-core | identity, 14 themes, settings, fsops, pure note edits | std, serde/JSON, regex, libc, tempfile, Unicode helpers; no GTK/GIO |
| impro-ui | shared CSS/provider, tokens, motion, widgets, note toolbar | core, GTK stack |
| impro-vte | safe Terminal/Pty and the local FFI shim | GTK stack, sys/pkg-config |
| impro-explorer | explorer, search, gitstatus | core, ui, GTK |
| impro-terminal | terminal docks, panes, resizing, terminal bar | core, ui, vte, GTK |
| impro-editor | editor, gutter, highlighting | core, ui, markdown, GTK, syntect |
| impro-markdown | Markdown editing/preview, tables, live rendering, slash/selection menus | core, ui, GTK |
| impro-chrome | titlebar, palette, overlays | core, ui, GTK |
| impro-settings-ui | settings dialog | core, ui, GTK |
| impro-gitviews | git data and WebKit git pages (created by owning task) | core, ui, GTK/WebKit as needed |
| impro-graph | native graph engine/view (created by owning task) | core; ui/GTK for the view |
| impro | argv, application, window wiring, stylesheet assembly | feature crates, core, ui |

Direction is `impro → feature crates → impro-ui → impro-core`; VTE is a sibling
of ui. Feature crates never depend on `impro`. Avoid dependencies between
features except editor → markdown. The binary connects their callbacks.
`members = ["crates/*"]` picks up the graph and git-view workers' crates.
Do not create those crates from a foundation stub. Each worker adds its own
crate-specific dependencies; do not alter shared GTK versions.

## Public data APIs

```rust
use impro_core::{settings, themes};
let theme = themes::get_theme(Some("impro-dark")); // &'static Theme; unknown => default
let foreground = &theme.ui["foreground"];
let points = settings::get_font_size("editor_font_size")?;
let values = settings::get_values(); // 45 validated keys, owned JSON map
let reduce = values["reduce_motion"].as_bool().unwrap_or(false);
```

`Theme` has public `id`, `name`, `dark`, `ui`, `syntax`, `terminal`, `chrome`.
`UiColors` is `BTreeMap<String, String>` using the same role keys everywhere.
`SyntaxStyle` contains foreground/bold/italic/underline; `KINDS` lists 26 roles.
`TerminalColors.ansi` is `[String; 16]`. Catalogue values are resolved in
`themes.json`, preserving contrast adjustment and rounding exactly.
Use `themes::{contrast,luminance,legible,mix_color}` for derived colours.

`settings::{normalize,get_value,set_value,set_values,reset_value}` return a
`Result` for unknown keys. A save result of `false` means IO failed; retain your
in-memory preferences. `save` merges raw keys, preserving extensions.
`Store::new(path)` offers the same operations with an explicit path for tests.
Unbounded JSON integers clamp without float overflow, and font-family validation
rejects Unicode control/format/separator characters.
Writes use a 0600 temporary file, fsync, atomic replacement and parent fsync.
`settings_path()` resolves XDG/HOME at call time and never imports older settings.
`get_graph/set_graph` and `get_gitgraph/set_gitgraph` preserve **opaque JSON**;
normalization belongs to the owning graph task. They are outside `defaults()`.

`fsops` returns `Result<PathBuf, FsOpError>` with original error wording.
`rename_no_replace` uses Linux renameat2; deletion protects filesystem/project
roots and unlinks symlinks. `trash` implements freedesktop home trash on the same
filesystem, including `.trashinfo`. Unsupported mounts return
`FsOpError::TrashUnavailable`; never silently delete. Call all filesystem/store
operations on a worker thread once the event loop is running.

`notes::{notes_dir,note_filename,note_path,create_note,list_notes,is_note,is_note_path}`
operate on `.impro/Notes`. `list_notes(root, "name"|"modified")` is fallible for
an unknown sort mode. Creation is lazy/exclusive and retries suffix collisions.
`apply_command(id, text, start, Some(end))`, `apply_slash_command` and
`apply_selection_command` return `Edit`. Inputs are signed character offsets and
are clamped/reordered. `Edit` offsets count Unicode characters, **not bytes**,
so they match `GtkTextBuffer`; `Edit::apply` returns a new document.
`NOTE_COMMANDS`, `SLASH_COMMANDS`, `INLINE_MARKERS`, `TURN_COMMANDS` and
`selection_formats` are shared with all Markdown UIs. Date insertion uses the
local calendar date. `impro_ui::notes::{apply_edit,apply_to_buffer}` group edits
into one GTK user action and select the result.

## CSS porting and registration

Every feature exposes `pub fn init()` and `pub fn css(ui: &UiColors) -> String`.
Call each `init()` once **before** applying the startup theme. Register by a
stable unique name; duplicate registration of the same function is harmless:

```rust
use impro_core::themes::UiColors;
pub fn css(ui: &UiColors) -> String {
    impro_ui::render_css(include_str!("panel.css"), ui)
}
pub fn init() {
    impro_ui::register_css("explorer.panel", css);
}
```

Copy the original f-string body into `panel.css`, rename CSS classes to `impro-`,
and leave `{ui['bg']}`, `{ui["foreground"]}`, or use shorter `{bg}` placeholders.
Both literal CSS braces and doubled f-string braces work. Locals such as
`{hover}` must be inserted into a cloned UiColors map by your builder first.
Register multiple builders for multiple original modules. Registration uses
name order; keep independent selectors, or combine dependent modules in one
builder to preserve cascade order. The ui crate registers its own widgets and
notes CSS; do not add them twice. It never imports feature crates.

`style::{TOKENS,build_css,build_overrides,with_accent}` are pure.
`apply_style(Some(theme))` installs the base theme;
`apply_theme(theme, accent, ui_font, editor_font)` adds user overrides.
`current_theme()` returns `Option<Rc<Theme>>` of the effective palette.
CSS strings are cached by full theme/font contents, invalidated on registration.
Strict staging catches parser errors before changing the current provider or
GTK dark preference. One registered provider updates existing widgets.
Editors and terminals still apply syntax/chrome/font/ANSI values via their APIs.

## Motion

```rust
use gtk::prelude::*;
use impro_ui::motion::{self, AnimateOptions, MotionKey};
let weak = widget.downgrade();
let tween = motion::animate(&widget, "state", move |p| {
    if let Some(widget) = weak.upgrade() { widget.set_opacity(p); }
}, AnimateOptions { key: MotionKey::Opacity, ..Default::default() });
// tween.cancel(); // leaves the current value; never calls on_done
```

Durations accept token strings, i32/u32 or finite f64 milliseconds.
`DURATIONS` and `EASING_POINTS` include compatibility aliases.
`easing(name)` / `cubic_bezier(...)` produce a `Curve`; `.evaluate()` validates
input and `.sample()` is allocation-free for known finite progress.
`AnimateOptions` has easing, key, delay_ms, optional boxed FnOnce on_done.
Keys are per widget; use `MotionKey::unique()` for independent custom effects.
Replacement captures the current value; the old completion never runs.
`Tween::finished()` includes cancellation. Dropping a handle keeps it running.
Use weak GTK references in callbacks to avoid extending widget lifetimes.

`fade(widget, show, duration, hide_when_done, on_done)`,
`slide_in(widget, offset, duration, on_done)`,
`animate_size_request(widget, Axis::Width|Height, target, duration, restore, on_done)`
manage reversal baselines. `StateFader::new(widget, update, duration_ms, initial)`
handles hover crossings; `.set()`, `.cancel()`, `.disconnect()` are reentrant.
New tweens finish synchronously when the application preference,
`IMPRO_REDUCE_MOTION=1`, or GTK enable-animations disables motion.

There is **one** shared frame-clock callback while work is active; unmapped
initial widgets use one shared temporary fallback until mapped. Unmap/unrealize/
destroy cancels the corresponding tween. Idle has no tick/timer. Normal tween
steps reuse the active vector and allocate nothing; new animations allocate at
creation. `active_count()` and `clock_attached()` support lifecycle tests.

## Widgets

`Toggle`, `IconButton`, `HotkeyChip`, `SearchField`, `SettingRow`, `SettingCard`,
`NavRow`, `NavList` are cloneable composition handles. They dereference to their
GTK widget for methods/signals; pass `.widget()` into generic GTK container
methods (`container.add(toggle.widget())`). Use `.widget().upcast_ref()` where
an API takes `&gtk::Widget`. This avoids unsafe wrapper casts.

`Toggle::new(active, label)` exposes `thumb` and `thumb_offset()`; active/toggled
and keyboard semantics are GTK's. IconButton takes icon name + tooltip.
`SearchField::default()` has the settings placeholder; use `connect_changed`
for immediate filtering. `SettingRow::new(name, description, control, RowOptions)`
supports key, aliases, reset callback and extras. It exposes labels/controls/
reset_button, `matches`, `set_narrow`, `activate_control`. Add rows with
`SettingCard::add_row`, then `set_query` / `visible_rows`.

`NavList::add_page` returns a NavRow; use `select_page`, `page_id`, `rows()` and
GTK row-selected. Nav rows expose labels/count and `set_count(Option<usize>)`.
The list implements j/k/G/gg in addition to GTK arrow/Home/End navigation.
`SwatchPicker` is a native GtkBox subclass with `buttons()`, `value()`,
`set_value(value, emit)` and `connect_value_changed`; clicking a chosen swatch
keeps it chosen. `CenteredColumn::default()` is a custom Bin with 700 max width,
32 side, 48 top, 64 bottom. `WrapBox::new(spacing,line_spacing)` is a custom
Container preserving each child's natural width. Both accept normal `.add()`.
`NoteToolbar` is a native GtkBox subclass: `new`, `buttons`, `connect_command`.
All accessible names/roles and CSS are installed by constructors/init.

## VTE and the toolchain spike

`impro_vte::Terminal` is a real GtkWidget implementing Scrollable; normal GTK
extension traits work. `Terminal::spawn_async(cwd, argv, envv, flags, timeout,
cancellable, callback)` completes with `Result<glib::Pid, glib::Error>` on the
main context. Empty envv inherits the environment; an explicit envv merges as
VTE specifies. No Rust callback runs between fork/exec. `PtyFlags::DEFAULT` is
the spawn default. `pty().fd()` is borrowed and only valid while the PTY lives.
The native version must be >=0.72 (text-format API). Public FFI types are under
`impro_vte::ffi`; the terminal worker owns subsequent binding extensions.

Feed/feed_child accept byte slices. Bindings cover clipboard formats, primary
selection, selection, reset, terminal size/grid/font scale/font, scrollback,
colours, cursor shape/blink, bell, scroll behaviour, autohide, hyperlinks,
window title/current directory/file URI, PTY fd, and the original signals
(child-exited, commit, selection/contents/title/current-directory changes).
`current-directory-uri` is a property notification, exposed as a convenient
`connect_current_directory_uri_changed`. Native GTK key/focus/tick APIs are
inherited. See the real shell/PTY test in `impro-vte/tests/terminal.rs`.

`cargo run -p impro --example toolchain_spike` retains the D1 one-process GTK +
VTE + WebKit proof. The ordinary binary creates neither a terminal nor WebView
until later feature wiring requests them.

## Stub ownership contracts

Explorer, terminal, editor, chrome and settings-ui currently export only
`init()` and `css(&UiColors) -> String`. Replace those bodies in the owning task;
the feature worker defines the panel types and the final window task wires them.
The terminal worker adds an impro-vte dependency; the editor worker adds
impro-markdown plus workspace syntect. The markdown stub exposes these modules:

| Module | Compiling contract |
| --- | --- |
| livemd | `MAX_BYTES = 1 MiB`, `LiveMarkdown::new(&TextView, Fn(&Path), Fn() -> Option<PathBuf>)`, destroy, set_enabled, set_readable_width, generic set_style(&syntax, &chrome, i32), edit_key(&EventKey, bool, i32) -> bool |
| slashmenu | `SlashMenu::new(&TextView)`, destroy, handle_key(&TextView, &EventKey) -> bool |
| seltoolbar | generic `sync(&tab)`, `set_enabled(&editor, bool)` |

These no-op items carry `STUB: replaced by task md-core/livemd`. Root reexports
LiveMarkdown, MAX_BYTES, SlashMenu. The Markdown workers replace them while
preserving callers, and add mdview/blockedit/livetable modules. Callback paths
are Rust Path/PathBuf, not UTF-8-only strings. No-op methods return false.

## Threading, borrows and safety

Only GTK's main thread creates/updates GTK or uses ui's thread-local registry.
Core Theme/settings JSON and owned results can cross worker threads. No async
runtime: use std::thread + glib MainContext channels/invoke. Cancel background
search/highlight/graph/git work and deliver small batches (<4 ms main-thread
work). Avoid starting WebKit, shell processes or syntax loading at startup.

Never retain a RefCell borrow across GTK calls, signal emissions, callback
invocation, or builder invocation. Copy/clone/take state out first; setters,
selection changes, CSS reloads and widget removal can synchronously re-enter.
Use Cell for scalar flags. The motion scheduler clones each Rc handle from its vector before an
update, so callbacks can replace or cancel any registered tween. Native custom-container forall/allocation code snapshots
children before GTK calls. User/IO errors return Result, and the binary installs
a logging panic hook. Do not unwrap user documents, paths or JSON values.

## Running and QA

```sh
cargo build --workspace
scripts/test.sh
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo build --release -p impro
IMPRO_QA_SCREENSHOT=.ceo/shot.png xvfb-run -a target/release/impro /tmp
```

The CLI accepts an optional folder, --help and --version; invalid folders and
unknown arguments exit 2. Application ID is `dev.impro.impro`, title `impro`.
The placeholder ApplicationWindow is 1200x800; the window task replaces it.
`app::install_screenshot_helper` attaches before show_all and captures the native
window after its first draw is flushed, then quits. PNG parent directories must
exist. `IMPRO_QA_TIMING=1` additionally prints `first-paint-ms=...` (main entry to
paint, excluding PNG encoding). Capture failures log and exit nonzero.

`xvfb-run -a dbus-run-session -- python3 scripts/startup-bench.py --runs 10 --output .ceo/startup.json`
records comparable process-launch→map measurements plus the internal paint
milestone. Supply `--reference-root /absolute/reference/project` to benchmark a
read-only Python reference package (module derived from directory name).
Runs reuse one display/bus, use /tmp as project, preserve user settings, and
disable Python bytecode writes. Map timing is not a substitute for complete
workspace parity: this binary is still a placeholder. No full-IDE performance
claim can be made until the final window task is integrated.

UI integration tests serialize closures onto a dedicated GTK thread within each
test executable. Pure core tests need no display. Noop feature stubs have no
tests; their workers add behavioral tests. Build artifacts and QA PNGs are ignored.
`packaging/install.sh` builds release and installs the binary, desktop launcher
and SVG under ~/.local; --uninstall removes those three user-owned files.
