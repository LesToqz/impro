//! File explorer, search in files and Git decorations.
pub mod explorer;
pub mod gitstatus;
pub mod pool;
pub mod search;
pub mod searchpanel;
#[doc(hidden)]
pub mod util;

use impro_core::themes::UiColors;
pub use explorer::{ConfirmRequest, DeleteMode, DialogHooks, ExplorerPanel, NameAction};
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
