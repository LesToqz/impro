//! Minimal GTK3 VTE ABI, sharing the workspace's gtk-rs 0.18 types.
#![allow(non_camel_case_types)]
use libc::{c_char, c_int, c_uint, c_void};
#[repr(C)]
pub struct VteTerminal {
    _private: [u8; 0],
}
#[repr(C)]
pub struct VteTerminalClass {
    _private: [u8; 0],
}
#[repr(C)]
pub struct VtePty {
    _private: [u8; 0],
}
#[repr(C)]
pub struct VtePtyClass {
    _private: [u8; 0],
}
pub type SpawnCallback = Option<
    unsafe extern "C" fn(*mut VteTerminal, glib_sys::GPid, *mut glib_sys::GError, *mut c_void),
>;
extern "C" {
    pub fn vte_terminal_get_type() -> glib_sys::GType;
    pub fn vte_terminal_new() -> *mut gtk_sys::GtkWidget;
    pub fn vte_pty_get_type() -> glib_sys::GType;
    pub fn vte_pty_get_fd(pty: *mut VtePty) -> c_int;
    pub fn vte_terminal_spawn_async(
        terminal: *mut VteTerminal,
        flags: c_uint,
        cwd: *const c_char,
        argv: *mut *mut c_char,
        envv: *mut *mut c_char,
        spawn_flags: glib_sys::GSpawnFlags,
        child_setup: glib_sys::GSpawnChildSetupFunc,
        child_data: *mut c_void,
        child_destroy: glib_sys::GDestroyNotify,
        timeout: c_int,
        cancellable: *mut gio_sys::GCancellable,
        callback: SpawnCallback,
        data: *mut c_void,
    );
}

extern "C" {
    pub fn vte_terminal_set_audible_bell(t: *mut VteTerminal, value: glib_sys::gboolean);
    pub fn vte_terminal_get_audible_bell(t: *mut VteTerminal) -> glib_sys::gboolean;
    pub fn vte_terminal_set_scroll_on_output(t: *mut VteTerminal, value: glib_sys::gboolean);
    pub fn vte_terminal_get_scroll_on_output(t: *mut VteTerminal) -> glib_sys::gboolean;
    pub fn vte_terminal_set_scroll_on_keystroke(t: *mut VteTerminal, value: glib_sys::gboolean);
    pub fn vte_terminal_get_scroll_on_keystroke(t: *mut VteTerminal) -> glib_sys::gboolean;
    pub fn vte_terminal_set_mouse_autohide(t: *mut VteTerminal, value: glib_sys::gboolean);
    pub fn vte_terminal_get_mouse_autohide(t: *mut VteTerminal) -> glib_sys::gboolean;
    pub fn vte_terminal_set_allow_hyperlink(t: *mut VteTerminal, value: glib_sys::gboolean);
    pub fn vte_terminal_get_allow_hyperlink(t: *mut VteTerminal) -> glib_sys::gboolean;
    pub fn vte_terminal_set_input_enabled(t: *mut VteTerminal, value: glib_sys::gboolean);
    pub fn vte_terminal_get_input_enabled(t: *mut VteTerminal) -> glib_sys::gboolean;
    pub fn vte_terminal_set_bold_is_bright(t: *mut VteTerminal, value: glib_sys::gboolean);
    pub fn vte_terminal_get_bold_is_bright(t: *mut VteTerminal) -> glib_sys::gboolean;
    pub fn vte_terminal_select_all(t: *mut VteTerminal);
    pub fn vte_terminal_unselect_all(t: *mut VteTerminal);
    pub fn vte_terminal_paste_clipboard(t: *mut VteTerminal);
    pub fn vte_terminal_copy_primary(t: *mut VteTerminal);
    pub fn vte_terminal_paste_primary(t: *mut VteTerminal);
    pub fn vte_terminal_set_default_colors(t: *mut VteTerminal);
    pub fn vte_terminal_get_row_count(t: *mut VteTerminal) -> libc::c_long;
    pub fn vte_terminal_get_column_count(t: *mut VteTerminal) -> libc::c_long;
    pub fn vte_terminal_get_char_width(t: *mut VteTerminal) -> libc::c_long;
    pub fn vte_terminal_get_char_height(t: *mut VteTerminal) -> libc::c_long;
    pub fn vte_terminal_get_scrollback_lines(t: *mut VteTerminal) -> libc::c_long;
    pub fn vte_terminal_get_window_title(t: *mut VteTerminal) -> *const c_char;
    pub fn vte_terminal_get_current_directory_uri(t: *mut VteTerminal) -> *const c_char;
    pub fn vte_terminal_get_current_file_uri(t: *mut VteTerminal) -> *const c_char;
    pub fn vte_terminal_feed(t: *mut VteTerminal, data: *const c_char, len: libc::ssize_t);
    pub fn vte_terminal_feed_child(t: *mut VteTerminal, data: *const c_char, len: libc::ssize_t);
    pub fn vte_terminal_set_size(t: *mut VteTerminal, columns: libc::c_long, rows: libc::c_long);
    pub fn vte_terminal_set_scrollback_lines(t: *mut VteTerminal, lines: libc::c_long);
    pub fn vte_terminal_get_has_selection(t: *mut VteTerminal) -> glib_sys::gboolean;
    pub fn vte_terminal_get_pty(t: *mut VteTerminal) -> *mut VtePty;
    pub fn vte_terminal_reset(
        t: *mut VteTerminal,
        clear_tabstops: glib_sys::gboolean,
        clear_history: glib_sys::gboolean,
    );
    pub fn vte_terminal_copy_clipboard_format(t: *mut VteTerminal, format: c_uint);
    pub fn vte_terminal_set_font(t: *mut VteTerminal, font: *const pango_sys::PangoFontDescription);
    pub fn vte_terminal_get_font(t: *mut VteTerminal) -> *const pango_sys::PangoFontDescription;
    pub fn vte_terminal_set_font_scale(t: *mut VteTerminal, scale: f64);
    pub fn vte_terminal_get_font_scale(t: *mut VteTerminal) -> f64;
    pub fn vte_terminal_set_cursor_shape(t: *mut VteTerminal, shape: c_uint);
    pub fn vte_terminal_set_cursor_blink_mode(t: *mut VteTerminal, mode: c_uint);
    pub fn vte_terminal_set_colors(
        t: *mut VteTerminal,
        fg: *const gdk_sys::GdkRGBA,
        bg: *const gdk_sys::GdkRGBA,
        palette: *const gdk_sys::GdkRGBA,
        len: libc::size_t,
    );
    pub fn vte_terminal_hyperlink_check_event(
        t: *mut VteTerminal,
        event: *mut gdk_sys::GdkEvent,
    ) -> *mut c_char;
    pub fn vte_terminal_set_color_cursor(t: *mut VteTerminal, color: *const gdk_sys::GdkRGBA);
    pub fn vte_terminal_set_color_foreground(t: *mut VteTerminal, color: *const gdk_sys::GdkRGBA);
    pub fn vte_terminal_set_color_background(t: *mut VteTerminal, color: *const gdk_sys::GdkRGBA);
    pub fn vte_terminal_set_color_highlight(t: *mut VteTerminal, color: *const gdk_sys::GdkRGBA);
    pub fn vte_terminal_set_color_highlight_foreground(
        t: *mut VteTerminal,
        color: *const gdk_sys::GdkRGBA,
    );
    pub fn vte_terminal_set_color_cursor_foreground(
        t: *mut VteTerminal,
        color: *const gdk_sys::GdkRGBA,
    );
}
