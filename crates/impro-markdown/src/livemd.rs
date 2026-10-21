//! Live Markdown integration contract.
// STUB: replaced by task md-core/livemd
pub const MAX_BYTES: usize = 1024 * 1024;
#[derive(Default)]
pub struct LiveMarkdown;
impl LiveMarkdown {
    pub fn new<O, P>(_view: &gtk::TextView, _open_file: O, _path_getter: P) -> Self
    where
        O: Fn(&std::path::Path) + 'static,
        P: Fn() -> Option<std::path::PathBuf> + 'static,
    {
        Self
    }
    pub fn destroy(&self) {}
    pub fn set_enabled(&self, _enabled: bool) {}
    pub fn set_readable_width(&self, _enabled: bool) {}
    pub fn set_style<S, C>(&self, _syntax: &S, _chrome: &C, _font_size: i32) {}
    pub fn edit_key(&self, _event: &gdk::EventKey, _indent_tabs: bool, _indent_width: i32) -> bool {
        false
    }
}
