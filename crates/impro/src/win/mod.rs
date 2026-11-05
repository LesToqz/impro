//! Native Windows frame integration.
//!
//! The window keeps its real Windows caption (Snap Layouts, shadow, rounded corners, system
//! menu); the menu/toggle bar is docked inside the window content with the Linux styling.
//! `policy` holds every decision as a pure function and is compiled on all hosts so Linux CI can
//! test it. Everything that calls the operating system is `cfg(windows)`; the shims below are
//! no-ops (or `None`) elsewhere so call sites stay free of `cfg` clutter.
pub mod policy;

#[cfg(windows)]
mod frame;
#[cfg(windows)]
mod sys;

/// Process-wide setup; call first thing in `main`, before GTK creates any window.
///
/// Sets the AppUserModelID so the taskbar groups and pins every impro window under the
/// identity the installer's shortcut carries. A failure only costs that grouping.
#[cfg(windows)]
pub fn init_process() {
    let _ = sys::set_app_user_model_id(impro_core::identity::APP_USER_MODEL_ID);
}

/// Hook a new main window so its native caption follows the theme: applied when the window is
/// realized and again when it is first shown. No-op off Windows.
pub fn install_frame(window: &gtk::ApplicationWindow) {
    #[cfg(windows)]
    frame::install(window);
    #[cfg(not(windows))]
    let _ = window;
}

/// Re-apply the caption colours after a theme or accent change. No-op off Windows.
pub fn refresh_frame(window: &gtk::ApplicationWindow) {
    #[cfg(windows)]
    frame::apply(window);
    #[cfg(not(windows))]
    let _ = window;
}

/// Usable size (taskbar excluded) of the primary monitor in GTK's logical pixels, `None` when the
/// display cannot say. Feeds `policy::fit_default_size`.
pub fn primary_work_area() -> Option<(i32, i32)> {
    use gdk::prelude::MonitorExt;
    let area = gdk::Display::default()?.primary_monitor()?.workarea();
    Some((area.width(), area.height()))
}

/// `AppsUseLightTheme` of the current user: `Some(true)` when apps should be light.
pub fn system_apps_use_light_theme() -> Option<bool> {
    #[cfg(windows)]
    return sys::apps_use_light_theme();
    #[cfg(not(windows))]
    None
}

/// Whether Windows client-area animations are on (`SPI_GETCLIENTAREAANIMATION`).
pub fn system_client_area_animations() -> Option<bool> {
    #[cfg(windows)]
    return sys::client_area_animations();
    #[cfg(not(windows))]
    None
}
