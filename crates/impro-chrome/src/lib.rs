//! Window chrome for impro. Call [`init`] before applying the first theme;
//! [`css`] combines the title bar, palette and notice templates via `render_css`.
//! Every widget and callback below lives on GTK's main thread. No feature-crate
//! dependency is needed. Composition handles are cloneable and expose `widget()`
//! for GTK container APIs; [`TitleBar`] is a native GtkHeaderBar subclass.
//!
//! Window integration:
//! - [`TitleBar::new`] accepts the window-owned File/View menu bar. Native signals
//!   `settings-requested`, `dock-toggle-requested`, `bottom-dock-toggle-requested`
//!   and `explorer-toggle-requested` work with `connect_local` or `connect_request`.
//!   `settings_button`, `dock_button`, `bottom_dock_button`, `explorer_button` and
//!   `title_label` expose GTK widgets. `set_dock_active`, `set_bottom_dock_active`
//!   and `set_explorer_active` synchronize active state without emitting requests.
//!   All normal HeaderBar/Widget APIs remain available.
//! - [`OverlayHost::new`] takes an optional main child. `show_prompt(&Prompt)`,
//!   `hide_prompt()` and `prompt()` manage one prompt, outside-click closing and
//!   focus restoration. `cache_prompt(&Prompt)` pre-realises one hidden prompt
//!   after the host window is realised and retains it across opens.
//!   `connect_prompt_shown` and `connect_prompt_hidden` receive
//!   the prompt. `notify(text, timeout_ms, level)` returns `Result<Notice, String>`;
//!   `notify_default(text, level)` uses the shared hold duration. Use zero for sticky
//!   notices and info, success,
//!   warning or error for the level. `notices`, `dismiss` and `clear_notices` manage
//!   the bounded stack. `Notice` exposes `text`, `level`, `label`, `box_widget`,
//!   `offset`, `collapse`, `leaving`, `timer_active`, `stop_timer` and `widget`.
//! - [`Command::new`] takes a label, callback and shortcut strings. Public `id`
//!   defaults to the label; `label`, `callback` and `hotkey` remain accessible.
//!   [`CommandPalette::new`] takes a command provider, reloaded on every open.
//!   `recent`, `set_recent`, `recent_limit`, `set_recent_limit` and
//!   `connect_command_run` manage recency and execution notifications. Execution
//!   follows prompt closure and restored editor focus.
//! - [`QuickSwitcher::new`] takes a project root; `set_root` updates it and
//!   `display_path` computes the displayed relative path. `set_files(paths, recent)`
//!   replaces the caller-supplied file snapshot and recent order. Enumeration and
//!   filesystem watching belong to the caller. `index_ready` reports indexing;
//!   lists over 5,000 are indexed and ranked on cancellable workers. A chosen
//!   [`PromptItem`] contains the original `PathBuf` in `data::<PathBuf>()`.
//!   Both palettes dereference to [`Prompt`], so `host.show_prompt(&palette)` works.
//!
//! [`Prompt::new`] takes `provider(query) -> Vec<PromptItem>` and a placeholder.
//! `set_provider`, `set_loader`, `set_placeholder`, `set_empty_text` and
//! `set_instructions` customize it (an empty instruction slice hides the footer).
//! `prepare` resets the query/results on open; `focus_input`, `refresh`, `set_items`,
//! `set_query`, `select`, `move_selection` (also `r#move`), `choose`, `close` and
//! `handle_key` control it. `query`, `items`, `selected_index`, `selected_item`,
//! `is_open` and `search_pending` inspect state. `connect_closed`, `connect_chosen`
//! and `connect_query_chosen` provide typed callbacks; chosen callbacks receive
//! Control/Shift/Alt modifiers after closure. `entry`, `frame`, `scroller`, `list`,
//! `empty`, `footer`, `rows` and `widget` expose GTK parts. [`ResultRow`] exposes
//! `widget`, `title`, `detail` and `chips` for component inspection.
//!
//! [`PromptItem`] has public `label`, `detail`, `hotkey`, `data`, `label_ranges`,
//! `detail_ranges` and `muted` fields, plus `new`, `with_data`, `data::<T>` and
//! `hotkeys`. Data is an optional main-thread `Rc<dyn Any>`; match ranges count
//! characters, not UTF-8 bytes. Workers return only owned file/range data.
//!
//! [`fuzzy`] exports all scoring constants, `Range`, `Match`, [`fold`],
//! `r#match(query, text, path)`, [`rank`], [`FuzzyIndex`], [`highlight_markup`],
//! [`name_start`], [`split_path_ranges`] and [`prompt_geometry`]. `FuzzyIndex::new`
//! takes candidates, a text-key function and path mode; `search(query, limit)`
//! returns stable `(candidate, score, ranges)` triples; `None` means no cap.
//! `search_cancelled` additionally accepts an atomic cancellation flag. `len`,
//! `is_empty` and public `candidates` inspect the index. GTK prompt rows are
//! capped at [`MAX_RESULTS`]. Header metrics are exported at the crate root;
//! notice geometry/levels and its CSS builder are exported by [`overlay`].
pub mod overlay;
mod prompt;
mod titlebar;
pub use overlay::{Notice, OverlayHost};
mod palettes;
pub use palettes::{Command, CommandPalette, QuickSwitcher};
pub use prompt::{Prompt, PromptItem, ResultRow};
pub mod fuzzy;
pub use fuzzy::{
    fold, highlight_markup, name_start, prompt_geometry, rank, split_path_ranges, FuzzyIndex,
    MAX_RESULTS,
};
pub use titlebar::{TitleBar, GLYPH_OPACITY, HEADER_HEIGHT, WINDOW_BUTTON_WIDTH, WINDOW_ICON_SIZE};
pub fn init() {
    impro_ui::register_css("chrome", css);
}
pub fn css(ui: &impro_core::themes::UiColors) -> String {
    format!(
        "{}{}{}",
        titlebar::css(ui),
        prompt::css(ui),
        overlay::css(ui)
    )
}
