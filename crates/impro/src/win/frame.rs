//! Colours the native Windows caption from the active theme through DWM window attributes.
//!
//! Dark or light caption glyphs work from Windows 10 (build 18985, and 1809 through the
//! pre-20H1 attribute number); caption, text and border colours and the rounded-corner request
//! need Windows 11 (build 22000). Anything the running Windows does not know is skipped silently.
use super::{policy, sys};
use gtk::prelude::*;
use std::{cell::Cell, rc::Rc, sync::OnceLock};

// The pure policy keeps its own copies of the dwmapi.h numbers so Linux can test them; a
// mismatch with windows-sys fails the Windows build here.
const _: () = {
    use windows_sys::Win32::Graphics::Dwm as dwm;
    assert!(policy::DWMWA_USE_IMMERSIVE_DARK_MODE == dwm::DWMWA_USE_IMMERSIVE_DARK_MODE);
    assert!(policy::DWMWA_WINDOW_CORNER_PREFERENCE == dwm::DWMWA_WINDOW_CORNER_PREFERENCE);
    assert!(policy::DWMWA_BORDER_COLOR == dwm::DWMWA_BORDER_COLOR);
    assert!(policy::DWMWA_CAPTION_COLOR == dwm::DWMWA_CAPTION_COLOR);
    assert!(policy::DWMWA_TEXT_COLOR == dwm::DWMWA_TEXT_COLOR);
    assert!(policy::DWMWCP_ROUND as i32 == dwm::DWMWCP_ROUND);
};

/// Apply when the window is realized (before it is first shown, so no flash of the default
/// colours) and once more after the first map; both are idempotent.
pub fn install(window: &gtk::ApplicationWindow) {
    window.connect_realize(apply);
    let mapped = Rc::new(Cell::new(false));
    window.connect_map(move |window| {
        if !mapped.replace(true) {
            apply(window);
        }
    });
}

/// Read the effective theme (accent included) and push it to the window's DWM attributes.
pub fn apply(window: &impl IsA<gtk::Widget>) {
    let Some(hwnd) = sys::hwnd(window) else {
        return;
    };
    let ui = impro_ui::current_theme()
        .map(|theme| theme.ui.clone())
        .unwrap_or_else(|| impro_core::themes::get_theme(None).ui.clone());
    let style = policy::caption_style(&ui);
    let dark = u32::from(style.dark);
    // Attribute 20 from Windows 10 20H1; the same switch was number 19 on 1809 to 1909.
    if !sys::set_window_attribute(hwnd, policy::DWMWA_USE_IMMERSIVE_DARK_MODE, dark) {
        let _ = sys::set_window_attribute(
            hwnd,
            policy::DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1,
            dark,
        );
    }
    if policy::supports_caption_colors(windows_build()) {
        for (attribute, value) in policy::caption_attributes(&style) {
            let _ = sys::set_window_attribute(hwnd, attribute, value);
        }
    }
}

fn windows_build() -> Option<u32> {
    static BUILD: OnceLock<Option<u32>> = OnceLock::new();
    *BUILD.get_or_init(sys::build_number)
}
