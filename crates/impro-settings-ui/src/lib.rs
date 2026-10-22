//! Settings window of impro. Call [`init`] before applying the first theme; [`css`]
//! renders the dialog's template through `render_css` (the shared widget CSS is
//! registered by `impro-ui`). Everything below lives on GTK's main thread, except
//! the write-behind worker of [`writer`].
//!
//! ```ignore
//! let dialog = SettingsDialog::new(Some(&main_window), Some(theme.id), Some(&sizes));
//! dialog.connect_theme_selected(|id| window.set_theme(id));
//! dialog.connect_font_size_changed(|key, size| window.font_size_changed(key, size));
//! dialog.connect_setting_changed(|key, value| window.apply_setting(key, value));
//! dialog.set_values(&live_values);          // silent; never emits
//! dialog.set_hotkeys(&window.hotkeys());    // no-op when unchanged
//! dialog.present(Some("hotkeys"));          // F1; `None` keeps the current page
//! ```
//!
//! Public surface:
//! - [`SettingsDialog`] derefs to its `gtk::Window` (title `Settings`, default size
//!   900x700, minimum 600x400, transient for the parent, non-modal). Close, Escape
//!   and the window control hide it; it is reused. `new(parent, theme_id, sizes)` and
//!   `with_options(parent, DialogOptions)` (`ignored_folders`, `project_root`) build
//!   only the chrome: the navigation pane and the visited page. Other pages are built
//!   on first visit; after the first drawn frame one page per idle iteration is built and
//!   style-warmed (an idle chain that ends, never a timer; `set_prebuild(false)` turns it
//!   off) and the first search builds the rest. All pages live in one scrolled column of
//!   per-page sections (`section(id)`, `content_scroll()`): switching pages and starting or
//!   ending a search only toggle visibility. `present(page)` shows and focuses; page ids are
//!   [`PAGES`]. `response()` hides (the former dialog API).
//! - Signals as closures: `connect_theme_selected(Fn(&str))`,
//!   `connect_font_size_changed(Fn(&str, i32))` for the three font-size keys (the owner
//!   persists those and the theme), and `connect_setting_changed(Fn(&str, &Value))`
//!   for every other key of `impro_core::settings::new_keys()` plus `"graph"` (the full
//!   validated object). The dialog validates and queues the save *before* emitting.
//! - Silent programmatic setters: `set_theme_id(Option<&str>)`, `set_sizes(&BTreeMap)`,
//!   `set_values(&Values)` (validated; unknown keys ignored), `set_hotkeys(&[(label,
//!   accelerator)])` (alternatives separated by " / "), `set_project_root`. Getters:
//!   `theme_id()`, `sizes()`, `values()` (every key as shown, with `theme` and `graph`).
//! - `search(query) -> Option<usize>` filters across pages (hit count while searching),
//!   `focus_content()`, `visible_rows()`, `select_page(id)`, and lookups of built parts
//!   by settings key: `row(key)`, `control(key)`, `reset_button(key)`, `value_label(key)`
//!   (these build the owning page when needed). `build_all_pages()` builds every page.
//! - Persistence is write-behind ([`writer`]): [`save_async`], [`flush`] (call at shutdown),
//!   [`set_save_failed_handler`] (the path that could not be written; the session keeps
//!   the change). The graph blob is validated by [`normalize_graph`].
//! - Shortcut list: [`DEFAULT_HOTKEYS`], [`default_hotkeys`], [`merge_hotkeys`].
mod dialog;
mod graph_prefs;
mod hotkeys;
mod pages;
pub mod writer;

pub use dialog::{DialogOptions, SettingsDialog, NARROW_CONTENT, NAV_WIDTH, PAGES, SKIP_DIRS};
pub use graph_prefs::{
    graph_defaults, graph_limits, graph_step, graph_values_equal, normalize_graph,
};
pub use hotkeys::{default_hotkeys, merge_hotkeys, DEFAULT_HOTKEYS};
pub use writer::{flush, save_async, save_async_to, save_value_async, set_save_failed_handler};

pub fn init() {
    impro_ui::register_css("settings-ui", css);
}

pub fn css(ui: &impro_core::themes::UiColors) -> String {
    impro_ui::render_css(include_str!("settings.css"), ui)
}
