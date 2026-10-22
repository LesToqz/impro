//! Markdown components and pure source transformations.
//!
//! [`mdview`] exposes bounded block parsing, inline spans, structural matchers,
//! pipe-cell splitting and safe link resolution. [`blockedit`] supplies key
//! transforms using Unicode character offsets. [`livetable`] owns editable
//! table models, lazy GTK grids and the [`livetable::TableHost`] integration
//! contract for the live document renderer.

pub fn init() {
    livetable::init();
    impro_ui::register_css("markdown", css);
}

pub fn css(_ui: &impro_core::themes::UiColors) -> String {
    String::new()
}

// STUB: replaced by task md-core/livemd
pub mod livemd;
pub mod seltoolbar;
pub mod slashmenu;
pub use livemd::{LiveMarkdown, MAX_BYTES};
pub use slashmenu::SlashMenu;

pub mod blockedit;
pub mod livetable;
pub mod mdview;
