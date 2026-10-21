//! Markdown components; implemented by the owning port task.

pub fn init() {}

pub fn css(_ui: &impro_core::themes::UiColors) -> String {
    String::new()
}

// STUB: replaced by task md-core/livemd
pub mod livemd;
pub mod seltoolbar;
pub mod slashmenu;
pub use livemd::{LiveMarkdown, MAX_BYTES};
pub use slashmenu::SlashMenu;
