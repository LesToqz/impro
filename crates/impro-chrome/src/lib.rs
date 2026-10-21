//! Window chrome for impro. Call [`init`] before applying the first theme.
//!
//! [`TitleBar::new`] accepts the window's File/View menu bar. Native request
//! signals are `settings-requested`, `dock-toggle-requested`,
//! `bottom-dock-toggle-requested`, and `explorer-toggle-requested`.
//! Button accessors allow feature visibility control; `set_dock_active`,
//! `set_bottom_dock_active`, and `set_explorer_active` synchronize silently.
//! The header inherits normal GTK HeaderBar and Widget APIs.
//!
//! [`fuzzy`] exposes character-indexed matching, stable ranking, an incremental
//! [`FuzzyIndex`], escaped bold markup, path ranges and prompt geometry.
mod titlebar;
pub mod fuzzy;
pub use fuzzy::{fold, highlight_markup, name_start, prompt_geometry, rank, split_path_ranges, FuzzyIndex, MAX_RESULTS};
pub use titlebar::TitleBar;
pub fn init() {impro_ui::register_css("chrome",css);}
pub fn css(ui:&impro_core::themes::UiColors)->String {titlebar::css(ui)}
