//! Slash-menu integration contract.
// STUB: replaced by task md-core/livemd
#[derive(Default)]
pub struct SlashMenu;
impl SlashMenu {
    pub fn new(_view: &gtk::TextView) -> Self {
        Self
    }
    pub fn destroy(&self) {}
    pub fn handle_key(&self, _view: &gtk::TextView, _event: &gdk::EventKey) -> bool {
        false
    }
}
