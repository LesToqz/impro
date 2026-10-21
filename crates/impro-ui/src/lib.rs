//! Shared GTK infrastructure. Call init before applying the application's theme.
pub mod motion;
pub mod notes;
pub mod style;
pub mod widgets;
pub use impro_core::themes::{Theme, UiColors};
pub use style::{apply_style, apply_theme, current_theme, register_css, render_css};
pub fn init() {
    register_css("shared.widgets", widgets::css);
    register_css("shared.notes", notes::css);
}
pub fn css(ui: &UiColors) -> String {
    format!("{}{}", widgets::css(ui), notes::css(ui))
}
