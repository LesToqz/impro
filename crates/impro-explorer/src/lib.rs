//! File explorer, search in files and Git decorations.
pub mod gitstatus;
pub mod pool;
pub mod search;
pub mod searchpanel;
#[doc(hidden)]
pub mod util;

use impro_core::themes::UiColors;
pub use searchpanel::{search_css, SearchPanel};

/// CSS of the search panel (`search.css`) for a theme's UI roles.
pub fn search_css_for(ui: &UiColors) -> String {
    impro_ui::render_css(include_str!("search.css"), ui)
}

pub fn init() {
    impro_ui::register_css("explorer.search", search_css_for);
}

pub fn css(ui: &UiColors) -> String {
    search_css_for(ui)
}
