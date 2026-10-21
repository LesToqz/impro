//! Terminal docks and pane chrome for impro.
pub mod panesplit;
pub mod resizegate;
pub mod terminal_bar;
pub fn init() {
    impro_ui::register_css("terminal.bar", css);
}
pub fn css(ui: &impro_core::themes::UiColors) -> String {
    terminal_bar::css(ui)
}

mod panel;
mod startup;
pub use panel::{
    pick_shell, safe_cwd, Pane, TerminalPanel, ANIMATION_MS, DEFAULT_MAX_PANES, HEADER_HEIGHT,
    SCROLLBACK_RANGE,
};

/// GTK may have disposed a signal owner before its containing panel's destroy.
fn disconnect(object: &impl glib::prelude::IsA<glib::Object>, handler: glib::SignalHandlerId) {
    use glib::{prelude::*, translate::*};
    unsafe {
        if glib::gobject_ffi::g_signal_handler_is_connected(
            object.as_ref().to_glib_none().0,
            handler.as_raw(),
        ) != 0
        {
            object.disconnect(handler);
        }
    }
}
