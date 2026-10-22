//! Tabbed editor for impro. Construct on GTK's main thread; register [`init`] before
//! applying the startup theme. [`EditorPanel`] is a native GtkBox.
//!
//! File reads and atomic saves run on workers. `open_file` is asynchronous;
//! `open_file_with(path, done)` reports completion with an optional stable tab.
//! `save_current(done)`, `save_all(done)`, `save_tab(tab, done)`, `close_current(done)`,
//! `close_tab(tab, done)` and `request_quit(done)` report success through callbacks.
//! These callbacks run on the main thread. No nested main-loop waits are needed.
//! Large files publish a first-screen prefix while bounded idle slices fill the buffer.
//! The view is temporarily read-only; open completion callbacks see the full document,
//! empty history and an unmodified buffer. Saves wait for loading to finish.
//!
//! Window integration:
//!
//! | Purpose | Public methods |
//! | --- | --- |
//! | Files | `new(Option<&Path>)`, `open_file`, `open_file_with`, `open_file_at(path,line,column,length)`, `save_current`, `save_all`, `save_tab`, `has_unsaved`, `request_quit` |
//! | Tab state | `tabs`, `current_tab`, `tab_for_path`, `current_path`, `buffer_text(path)`, `selected_text`, `focus_current`, `close_current`, `close_tab` |
//! | Path changes | `set_root`, `root`, `on_path_renamed(old,new)`, `on_path_deleted(path)` |
//! | Arbitrary pages | `add_view_tab(key,title,widget,tooltip,icon)`, `add_view_tab_with_shutdown`, `has_view_tab`, `view_tab_widget`, `set_view_tab_title`, `current_view_key`, `close_view_tab`, `view_tabs` |
//! | Find/keys | `focus_find`, `find(backwards,from_cursor)`, `handle_key`, `edit("enter"|"tab"|"unindent"|"comment")`, `zoom(step)` |
//! | Preferences | `apply_settings`, `apply_theme`, `set_syntax_style`, `set_chrome`, `set_font_size`, `set_default_font_size`, `set_font_family`, `set_line_numbers`, `set_highlight_line`, `set_indent`, `set_auto_pair`, `set_md_live_preview`, `set_md_readable_width`, `set_slash_commands`, `set_selection_toolbar` |
//! | Markdown | `set_markdown_mode(mode,Option<&Path>)`, `markdown_mode(Option<&Path>)`; controllers use the shared Markdown crate contract |
//! | Inspection | `notebook`, `stack`, `overlay`, `status`, `search_bar`, `search_entry`, `is_busy`, `last_error`; getters for size, indentation, Markdown, syntax and chrome preferences |
//! | Lifetime/dialogs | `shutdown` (also on destroy), `set_dialog_hook`; [`Prompt`] / [`Response`] support host dialog policies and tests |
//!
//! Typed GObject connectors are `connect_current_file_changed` (optional PathBuf),
//! `connect_file_saved` / `connect_tab_closed` (Path), `connect_view_tab_closed` (key),
//! `connect_dirty_changed` (bool), `connect_font_size_changed` (i32) and `connect_error`
//! (message). GObject names use hyphens. Connect `error` to the host's message dialog.
//! UTF-8 signal paths are absolute paths; other paths use local file URIs. Typed
//! connectors decode them losslessly. The search text provider excludes deleted tabs.
//!
//! [`EditorTab`] exposes GTK buffer/view/page/scroll/header and highlighter/chrome
//! handles; `path`, `ending`, `deleted`, `is_closed`, `cursor`, `text`, `undo(redo)`,
//! `history_len`, `max_load_slice`, `language`, `markdown_mode`, `markdown_location`,
//! `set_markdown_mode`, `set_font_size`, `set_tab_stops`, `update_title`.
//! Release tab snapshots before testing weak buffer finalization.
//!
//! [`highlight::tokenize`] is a blocking, pure thread-safe API returning character
//! offset ranges for the core theme kinds. Syntax data loads lazily in workers.
//! [`gutter::ViewChrome`] uses cached digit layouts and only visits visible lines.
//! It owns bounded bracket scans and caret tags, with no recurring idle timer.
mod file;
pub mod gutter;
pub mod highlight;
mod history;
mod panel;
mod tab;
pub use file::{
    atomic_write, detect_line_ending, leading_whitespace, read_file, toggle_comments, unindent,
    Document, MAX_FILE_SIZE,
};
use gtk::prelude::*;
pub use panel::{
    DialogHook, EditorPanel, Prompt, Response, StatusBar, ViewTab, DEFAULT_FONT_SIZE,
    MAX_FONT_SIZE, MIN_FONT_SIZE,
};
pub use tab::{EditorTab, TabHeader};
pub fn buffer_text(buffer: &gtk::TextBuffer) -> String {
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .unwrap_or_default()
        .to_string()
}
pub fn init() {
    impro_ui::register_css("editor.panel", editor_css);
    impro_ui::register_css("editor.gutter", gutter::css);
}
pub fn editor_css(ui: &impro_core::themes::UiColors) -> String {
    impro_ui::render_css(include_str!("editor.css"), ui)
}
pub fn css(ui: &impro_core::themes::UiColors) -> String {
    editor_css(ui) + &gutter::css(ui)
}
