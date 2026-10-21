//! File explorer, Search in Files and Git decorations.
//!
//! This crate ports the explorer, search and Git-status modules of the original IDE. The
//! window task wires everything **only through the public surface listed here**.
//!
//! # Threading model
//!
//! All GTK objects live on the main thread. Every directory scan, Git status run, notes
//! listing, filesystem action and search runs on a small worker pool ([`pool`], at most
//! `min(cores - 1, 4)` threads, sleeping while idle); results come back as owned data through
//! `glib::MainContext` channels and are applied in steps of a few milliseconds. The only
//! main-thread filesystem calls are the `inotify` registrations of the `Gio::FileMonitor`
//! watchers (Git metadata, the active file's folder and the notes folders).
//!
//! # Public surface
//!
//! ## [`ExplorerPanel`] (a `GtkBox`, module [`explorer`])
//!
//! Construction and state:
//! * `ExplorerPanel::new(root)`; `root()`; `set_root(path)` (Open Folder).
//! * `refresh()` (F5, Refresh button; also re-runs the search, Git and notes),
//!   `refresh_git()` (debounced 150 ms; call after saves and on window focus-in),
//!   `refresh_notes()`.
//! * Settings: `set_font_size(pt)`, `set_show_hidden`, `set_confirm_delete`,
//!   `set_delete_mode(`[`DeleteMode`]`)` (`DeleteMode::parse("trash"|"delete")`),
//!   `set_git_colors`, `set_git_enabled`, `set_folders_first`, `set_compact`,
//!   `set_open_after_create`, `set_notes_enabled` (the Notes feature gate),
//!   `set_notes_sort("name"|"modified")`.
//! * Editor link: `set_current_file(Option<&Path>)` (reveals and selects the file, lists
//!   its search hits first, selects it in the Notes list), `set_text_provider(Option<Rc<dyn
//!   Fn(&Path) -> Option<String>>>)` (unsaved buffer text; called on the main thread).
//! * Search: `focus_search(Option<&str>)` (Ctrl+Shift+F; a query replaces the text and runs
//!   at once), `clear_search()`, `focus_tree()`, `search()` -> [`SearchPanel`].
//! * Notes: `focus_notes()`, `show_notes_section(bool)`, `set_notes_collapsed(bool)`,
//!   `new_note_dialog()`, `create_note(name, done)`.
//! * Models: `store()` (the file-tree `GtkTreeStore`: connect to `row-inserted` /
//!   `row-deleted` to learn about tree changes), `tree()`, `header()`, `header_button(..)`.
//! * Visibility uses inherited `gtk::WidgetExt` methods: `is_visible`, `set_visible`,
//!   `set_no_show_all`. The host applies the explorer feature gate and remembers visibility.
//! * Actions with asynchronous completion: `create(parent, name, folder, done)`,
//!   `rename(path, new_name, done)`, `delete(path, permanent, done)`; dialog based:
//!   `new_dialog`, `rename_dialog`, `delete_dialog`, `confirm_removal`, `name_dialog`.
//!   [`DialogHooks`] (`set_dialog_hooks`) lets a host or test answer the dialogs.
//!
//! Signals (`connect_*` helpers, GObject names in parentheses):
//! * `connect_file_activated(|panel, path|)` (`file-activated`) -> `editor.open_file`.
//! * `connect_path_renamed(|panel, old, new|)` (`path-renamed`) -> `editor.on_path_renamed`.
//! * `connect_path_deleted(|panel, path|)` (`path-deleted`) -> `editor.on_path_deleted`.
//! * `connect_search_result_activated(|panel, path, line, column, length|)`
//!   (`search-result-activated`, zero-based, in characters) -> `editor.open_file_at`.
//! * `connect_search_escaped(|panel|)` (`search-escaped`) -> focus the editor, else
//!   `focus_tree()`.
//!
//! ## [`SearchPanel`] (module [`searchpanel`])
//! `new(content, root)`, `query()`, `set_root`, `set_current_file`, `set_text_provider`,
//! `set_font_size`, `focus_search()`, `clear()`, `search_now(Option<&str>)` (starts at once;
//! results stream in, wait for `is_searching()` to turn false), `results()`, `entry()`,
//! `tree()`, `store()`, signals `result-activated` and `escaped`.
//! `case_button()` and `regex_button()` expose the toggles; `summary()` exposes the status
//! label. Use `connect_result_activated` and `connect_escaped` for typed signal handlers.
//! `is_searching()` includes pending model rows. Hosts normally react to activation
//! signals, while diagnostics can observe `store()` row signals.
//!
//! ## Search engine ([`search`], no GTK)
//! [`SKIP_DIRS`] (also `search::skip_dirs_sorted()`), `search::search`,
//! `search::search_stream`, `search::find_in_text`, `search::iter_project_files`,
//! `search::list_project_files(root, show_hidden, limit)` (the quick switcher listing) and
//! the result types (`SearchOptions`, `SearchResults`, `FileGroup`, `Match`, `SearchError`).
//! These helpers are blocking: the host must call them on a worker, using `pool::spawn`
//! and `pool::CancelToken` when appropriate.
//!
//! Tree inspection helpers are `is_busy`, `is_loaded`, `find_row`, `selected_path`,
//! `select`, `expanded_paths` and `target_folder`. State getters mirror the settings
//! setters (`show_hidden`, `confirm_delete`, `delete_mode`, `open_after_create`,
//! `font_size`, `folders_first`, `compact`, `git_colors`, `git_enabled`). Notes expose
//! `notes_section_visible`, `notes_collapsed`, `notes_height`, `selected_note`,
//! `select_note`, `reveal_note` and `active_file`. Remaining hidden methods and model
//! handles support diagnostics/tests; hosts should use the signals and commands above.
//!
//! ## Git status ([`gitstatus`], no GTK)
//! `collect_status`, `parse_porcelain`, `Snapshot`, `Status`, `Kind`.
//!
//! ## Styling
//! [`init`] registers the stylesheets `explorer.panel` and `explorer.search` with
//! `impro-ui` (call once before applying the startup theme); [`css`],
//! [`explorer_css`] and [`search_css_for`] render them for a palette.
//!
//! # Regular expressions
//! The original used Python's `re`; this port uses the `regex` crate (linear-time). The
//! UI label can keep saying "regular expressions". Differences users may notice: no
//! look-around or back-references (they report "Invalid regular expression: ..."), no
//! catastrophic backtracking, a lone `{` must be escaped, error messages come from the
//! `regex` crate, `\Z` is not
//! supported (use `\z`), and Unicode classes follow UTS#18. Literal search, case folding
//! (Unicode simple folding), line splitting (all Unicode line boundaries, as
//! `str.splitlines`) and character coordinates are identical.
pub mod explorer;
pub mod gitstatus;
pub mod pool;
pub mod search;
pub mod searchpanel;
#[doc(hidden)]
pub mod util;

pub use explorer::{ConfirmRequest, DeleteMode, DialogHooks, ExplorerPanel, NameAction};
use impro_core::themes::UiColors;
pub use search::SKIP_DIRS;
pub use searchpanel::{search_css, SearchPanel};

/// CSS of the search panel (`search.css`) for a theme's UI roles.
pub fn search_css_for(ui: &UiColors) -> String {
    impro_ui::render_css(include_str!("search.css"), ui)
}

/// CSS of the explorer, header and Notes section (`explorer.css`) for a theme's UI roles.
pub fn explorer_css(ui: &UiColors) -> String {
    explorer::css(ui)
}

/// Register the explorer and search stylesheets; call once before applying the theme.
pub fn init() {
    impro_ui::register_css("explorer.panel", explorer_css);
    impro_ui::register_css("explorer.search", search_css_for);
}

/// Both stylesheets, in the order the original assembled them.
pub fn css(ui: &UiColors) -> String {
    explorer_css(ui) + &search_css_for(ui)
}
