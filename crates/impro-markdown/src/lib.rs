//! Markdown components and pure source transformations.
//!
//! [`mdview`] exposes bounded block parsing, inline spans, structural matchers,
//! pipe-cell splitting and safe link resolution. [`blockedit`] supplies key
//! transforms using Unicode character offsets. [`livetable`] owns editable
//! table models, lazy GTK grids and the [`livetable::TableHost`] integration
//! contract for the live document renderer.
//!
//! A renderer implements `TableHost` for silent source writes, replacement tags,
//! layout, link activation and editor history. It creates `Tables` on demand,
//! supplies marked `TableBlock`s, and consumes one-cell `fill_visible_slice` or
//! `fill_slice` steps between main-loop yields. `caret` keeps table source hidden;
//! `remove`/`destroy` release grids and end pending typing actions. See the crate's
//! `tests/VERIFICATION.md` for integration checks requiring the real editor.
//!
//! Call [`init`] before applying a theme. Table styles are registered separately
//! as `markdown.tables`; the renderer can override its body font and colours with
//! [`livetable::styled_css`]. Pure parsers and models can run on worker threads;
//! GTK controllers and the CSS registry stay on the GTK thread.

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
