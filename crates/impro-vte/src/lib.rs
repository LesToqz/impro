//! Safe GTK3 terminal bindings for the shared gtk-rs 0.18 stack.
use glib::translate::*;
use gtk::prelude::*;
pub use impro_vte_sys as ffi;

glib::wrapper! {
    pub struct Terminal(Object<ffi::VteTerminal, ffi::VteTerminalClass>)
        @extends gtk::Widget, @implements gtk::Scrollable, gtk::Buildable;
    match fn { type_ => || ffi::vte_terminal_get_type(), }
}

glib::wrapper! {
    pub struct Pty(Object<ffi::VtePty, ffi::VtePtyClass>);
    match fn { type_ => || ffi::vte_pty_get_type(), }
}

pub fn init() {}
impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}
impl Terminal {
    pub fn new() -> Self {
        assert!(
            gtk::is_initialized_main_thread(),
            "GTK must be initialized on this thread"
        );
        unsafe { gtk::Widget::from_glib_none(ffi::vte_terminal_new()).unsafe_cast() }
    }

    /// Spawn a child with no post-fork Rust callback; completion runs on GTK's main context.
    #[allow(clippy::too_many_arguments)] // Mirrors the terminal spawn contract.
    pub fn spawn_async<F: FnOnce(Result<glib::Pid, glib::Error>) + 'static>(
        &self,
        cwd: Option<&str>,
        argv: &[&str],
        envv: &[&str],
        flags: glib::SpawnFlags,
        timeout: i32,
        cancellable: Option<&gio::Cancellable>,
        callback: F,
    ) {
        self.spawn_async_path(
            cwd.map(std::path::Path::new),
            argv,
            envv,
            flags,
            timeout,
            cancellable,
            callback,
        );
    }

    /// Path-preserving variant of spawn_async, including non-UTF-8 Unix directories.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_async_path<F: FnOnce(Result<glib::Pid, glib::Error>) + 'static>(
        &self,
        cwd: Option<&std::path::Path>,
        argv: &[&str],
        envv: &[&str],
        flags: glib::SpawnFlags,
        timeout: i32,
        cancellable: Option<&gio::Cancellable>,
        callback: F,
    ) {
        use std::os::unix::ffi::OsStrExt;
        if argv.is_empty()
            || argv.iter().chain(envv.iter()).any(|s| s.contains('\0'))
            || cwd.is_some_and(|s| s.as_os_str().as_bytes().contains(&0))
        {
            callback(Err(glib::Error::new(
                gio::IOErrorEnum::InvalidArgument,
                "Spawn needs an executable and NUL-free arguments",
            )));
            return;
        }
        unsafe extern "C" fn done<F: FnOnce(Result<glib::Pid, glib::Error>) + 'static>(
            _terminal: *mut ffi::VteTerminal,
            pid: glib::ffi::GPid,
            error: *mut glib::ffi::GError,
            data: glib::ffi::gpointer,
        ) {
            let callback: Box<F> = Box::from_raw(data.cast());
            callback(if error.is_null() {
                Ok(from_glib(pid))
            } else {
                Err(from_glib_none(error))
            });
        }
        let cwd = cwd.to_glib_none();
        let argv = argv.to_glib_none();
        let environment = envv.to_glib_none();
        let environment_ptr = if envv.is_empty() {
            std::ptr::null_mut()
        } else {
            environment.0
        };
        unsafe {
            ffi::vte_terminal_spawn_async(
                self.to_glib_none().0,
                0,
                cwd.0,
                argv.0,
                environment_ptr,
                flags.into_glib(),
                None,
                std::ptr::null_mut(),
                None,
                timeout,
                cancellable.to_glib_none().0,
                Some(done::<F>),
                Box::into_raw(Box::new(callback)).cast(),
            );
        }
    }
}

/// Clipboard representation understood by VTE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Format {
    Text = 1,
    Html = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum CursorShape {
    Block = 0,
    Ibeam = 1,
    Underline = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum CursorBlinkMode {
    System = 0,
    On = 1,
    Off = 2,
}
bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default)]
    pub struct PtyFlags: u32 { const DEFAULT = 0; const NO_SESSION = 32; const NO_CTTY = 64; }
}
impl Pty {
    /// Borrowed file descriptor: valid while this PTY remains alive.
    pub fn fd(&self) -> std::os::fd::RawFd {
        unsafe { ffi::vte_pty_get_fd(self.to_glib_none().0) }
    }
}
impl Terminal {
    pub fn set_size(&self, columns: libc::c_long, rows: libc::c_long) {
        unsafe { ffi::vte_terminal_set_size(self.to_glib_none().0, columns, rows) }
    }
    pub fn set_scrollback_lines(&self, lines: libc::c_long) {
        unsafe { ffi::vte_terminal_set_scrollback_lines(self.to_glib_none().0, lines) }
    }
    pub fn has_selection(&self) -> bool {
        unsafe { from_glib(ffi::vte_terminal_get_has_selection(self.to_glib_none().0)) }
    }
    pub fn pty(&self) -> Option<Pty> {
        unsafe { from_glib_none(ffi::vte_terminal_get_pty(self.to_glib_none().0)) }
    }
    pub fn reset(&self, clear_tabstops: bool, clear_history: bool) {
        unsafe {
            ffi::vte_terminal_reset(
                self.to_glib_none().0,
                clear_tabstops.into_glib(),
                clear_history.into_glib(),
            )
        }
    }
    pub fn copy_clipboard_format(&self, format: Format) {
        unsafe { ffi::vte_terminal_copy_clipboard_format(self.to_glib_none().0, format as _) }
    }
    pub fn set_font(&self, font: &pango::FontDescription) {
        unsafe { ffi::vte_terminal_set_font(self.to_glib_none().0, font.to_glib_none().0) }
    }
    pub fn font(&self) -> Option<pango::FontDescription> {
        unsafe { from_glib_none(ffi::vte_terminal_get_font(self.to_glib_none().0)) }
    }
    pub fn set_font_scale(&self, scale: f64) {
        unsafe { ffi::vte_terminal_set_font_scale(self.to_glib_none().0, scale) }
    }
    pub fn font_scale(&self) -> f64 {
        unsafe { ffi::vte_terminal_get_font_scale(self.to_glib_none().0) }
    }
    pub fn set_cursor_shape(&self, shape: CursorShape) {
        unsafe { ffi::vte_terminal_set_cursor_shape(self.to_glib_none().0, shape as _) }
    }
    pub fn set_cursor_blink_mode(&self, mode: CursorBlinkMode) {
        unsafe { ffi::vte_terminal_set_cursor_blink_mode(self.to_glib_none().0, mode as _) }
    }
    pub fn set_colors(
        &self,
        foreground: Option<&gdk::RGBA>,
        background: Option<&gdk::RGBA>,
        palette: &[gdk::RGBA],
    ) {
        assert!(
            palette.is_empty() || palette.len() == 16 || palette.len() == 256,
            "VTE palette must have 0, 16 or 256 colors"
        );
        // RGBA is a boxed wrapper, not repr(C); borrow a contiguous FFI array.
        let colors: Vec<gdk::ffi::GdkRGBA> = palette
            .iter()
            .map(|c| unsafe { *c.to_glib_none().0 })
            .collect();
        unsafe {
            ffi::vte_terminal_set_colors(
                self.to_glib_none().0,
                foreground.to_glib_none().0,
                background.to_glib_none().0,
                colors.as_ptr(),
                colors.len(),
            )
        }
    }
    pub fn hyperlink_check_event(&self, event: &gdk::Event) -> Option<glib::GString> {
        unsafe {
            from_glib_full(ffi::vte_terminal_hyperlink_check_event(
                self.to_glib_none().0,
                event.to_glib_none().0,
            ))
        }
    }
    pub fn set_audible_bell(&self, value: bool) {
        unsafe { ffi::vte_terminal_set_audible_bell(self.to_glib_none().0, value.into_glib()) }
    }
    pub fn audible_bell(&self) -> bool {
        unsafe { from_glib(ffi::vte_terminal_get_audible_bell(self.to_glib_none().0)) }
    }
    pub fn set_scroll_on_output(&self, value: bool) {
        unsafe { ffi::vte_terminal_set_scroll_on_output(self.to_glib_none().0, value.into_glib()) }
    }
    pub fn scroll_on_output(&self) -> bool {
        unsafe {
            from_glib(ffi::vte_terminal_get_scroll_on_output(
                self.to_glib_none().0,
            ))
        }
    }
    pub fn set_scroll_on_keystroke(&self, value: bool) {
        unsafe {
            ffi::vte_terminal_set_scroll_on_keystroke(self.to_glib_none().0, value.into_glib())
        }
    }
    pub fn scroll_on_keystroke(&self) -> bool {
        unsafe {
            from_glib(ffi::vte_terminal_get_scroll_on_keystroke(
                self.to_glib_none().0,
            ))
        }
    }
    pub fn set_mouse_autohide(&self, value: bool) {
        unsafe { ffi::vte_terminal_set_mouse_autohide(self.to_glib_none().0, value.into_glib()) }
    }
    pub fn mouse_autohide(&self) -> bool {
        unsafe { from_glib(ffi::vte_terminal_get_mouse_autohide(self.to_glib_none().0)) }
    }
    pub fn set_allow_hyperlink(&self, value: bool) {
        unsafe { ffi::vte_terminal_set_allow_hyperlink(self.to_glib_none().0, value.into_glib()) }
    }
    pub fn allow_hyperlink(&self) -> bool {
        unsafe { from_glib(ffi::vte_terminal_get_allow_hyperlink(self.to_glib_none().0)) }
    }
    pub fn set_input_enabled(&self, value: bool) {
        unsafe { ffi::vte_terminal_set_input_enabled(self.to_glib_none().0, value.into_glib()) }
    }
    pub fn input_enabled(&self) -> bool {
        unsafe { from_glib(ffi::vte_terminal_get_input_enabled(self.to_glib_none().0)) }
    }
    pub fn set_bold_is_bright(&self, value: bool) {
        unsafe { ffi::vte_terminal_set_bold_is_bright(self.to_glib_none().0, value.into_glib()) }
    }
    pub fn bold_is_bright(&self) -> bool {
        unsafe { from_glib(ffi::vte_terminal_get_bold_is_bright(self.to_glib_none().0)) }
    }
    pub fn select_all(&self) {
        unsafe { ffi::vte_terminal_select_all(self.to_glib_none().0) }
    }
    pub fn unselect_all(&self) {
        unsafe { ffi::vte_terminal_unselect_all(self.to_glib_none().0) }
    }
    pub fn paste_clipboard(&self) {
        unsafe { ffi::vte_terminal_paste_clipboard(self.to_glib_none().0) }
    }
    pub fn copy_primary(&self) {
        unsafe { ffi::vte_terminal_copy_primary(self.to_glib_none().0) }
    }
    pub fn paste_primary(&self) {
        unsafe { ffi::vte_terminal_paste_primary(self.to_glib_none().0) }
    }
    pub fn set_default_colors(&self) {
        unsafe { ffi::vte_terminal_set_default_colors(self.to_glib_none().0) }
    }
    pub fn row_count(&self) -> libc::c_long {
        unsafe { ffi::vte_terminal_get_row_count(self.to_glib_none().0) }
    }
    pub fn column_count(&self) -> libc::c_long {
        unsafe { ffi::vte_terminal_get_column_count(self.to_glib_none().0) }
    }
    pub fn char_width(&self) -> libc::c_long {
        unsafe { ffi::vte_terminal_get_char_width(self.to_glib_none().0) }
    }
    pub fn char_height(&self) -> libc::c_long {
        unsafe { ffi::vte_terminal_get_char_height(self.to_glib_none().0) }
    }
    pub fn scrollback_lines(&self) -> libc::c_long {
        unsafe { ffi::vte_terminal_get_scrollback_lines(self.to_glib_none().0) }
    }
    pub fn window_title(&self) -> Option<glib::GString> {
        unsafe { from_glib_none(ffi::vte_terminal_get_window_title(self.to_glib_none().0)) }
    }
    pub fn current_directory_uri(&self) -> Option<glib::GString> {
        unsafe {
            from_glib_none(ffi::vte_terminal_get_current_directory_uri(
                self.to_glib_none().0,
            ))
        }
    }
    pub fn current_file_uri(&self) -> Option<glib::GString> {
        unsafe {
            from_glib_none(ffi::vte_terminal_get_current_file_uri(
                self.to_glib_none().0,
            ))
        }
    }
    pub fn feed(&self, data: &[u8]) {
        unsafe {
            ffi::vte_terminal_feed(self.to_glib_none().0, data.as_ptr().cast(), data.len() as _)
        }
    }
    pub fn feed_child(&self, data: &[u8]) {
        unsafe {
            ffi::vte_terminal_feed_child(
                self.to_glib_none().0,
                data.as_ptr().cast(),
                data.len() as _,
            )
        }
    }
    pub fn set_color_cursor(&self, color: Option<&gdk::RGBA>) {
        unsafe { ffi::vte_terminal_set_color_cursor(self.to_glib_none().0, color.to_glib_none().0) }
    }
    pub fn set_color_foreground(&self, color: Option<&gdk::RGBA>) {
        unsafe {
            ffi::vte_terminal_set_color_foreground(self.to_glib_none().0, color.to_glib_none().0)
        }
    }
    pub fn set_color_background(&self, color: Option<&gdk::RGBA>) {
        unsafe {
            ffi::vte_terminal_set_color_background(self.to_glib_none().0, color.to_glib_none().0)
        }
    }
    pub fn set_color_highlight(&self, color: Option<&gdk::RGBA>) {
        unsafe {
            ffi::vte_terminal_set_color_highlight(self.to_glib_none().0, color.to_glib_none().0)
        }
    }
    pub fn set_color_highlight_foreground(&self, color: Option<&gdk::RGBA>) {
        unsafe {
            ffi::vte_terminal_set_color_highlight_foreground(
                self.to_glib_none().0,
                color.to_glib_none().0,
            )
        }
    }
    pub fn set_color_cursor_foreground(&self, color: Option<&gdk::RGBA>) {
        unsafe {
            ffi::vte_terminal_set_color_cursor_foreground(
                self.to_glib_none().0,
                color.to_glib_none().0,
            )
        }
    }
    pub fn connect_selection_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        let this = self.downgrade();
        self.connect_local("selection-changed", false, move |_| {
            if let Some(this) = this.upgrade() {
                f(&this);
            }
            None
        })
    }
    pub fn connect_contents_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        let this = self.downgrade();
        self.connect_local("contents-changed", false, move |_| {
            if let Some(this) = this.upgrade() {
                f(&this);
            }
            None
        })
    }
    pub fn connect_window_title_changed<F: Fn(&Self) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        let this = self.downgrade();
        self.connect_local("window-title-changed", false, move |_| {
            if let Some(this) = this.upgrade() {
                f(&this);
            }
            None
        })
    }
    pub fn connect_current_directory_uri_changed<F: Fn(&Self) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_notify_local(Some("current-directory-uri"), move |this, _| f(this))
    }
    pub fn connect_child_exited<F: Fn(&Self, i32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        let this = self.downgrade();
        self.connect_local("child-exited", false, move |values| {
            if let (Some(this), Ok(status)) = (this.upgrade(), values[1].get::<i32>()) {
                f(&this, status);
            }
            None
        })
    }
    pub fn connect_commit<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        let this = self.downgrade();
        self.connect_local("commit", false, move |values| {
            if let (Some(this), Ok(text)) = (this.upgrade(), values[1].get::<String>()) {
                f(&this, &text);
            }
            None
        })
    }
}

impl Terminal {
    /// All visible terminal text plus scrollback, owned by the caller.
    pub fn text_format(&self, format: Format) -> Option<glib::GString> {
        unsafe {
            from_glib_full(ffi::vte_terminal_get_text_format(
                self.to_glib_none().0,
                format as _,
            ))
        }
    }
    pub fn text_selected(&self, format: Format) -> Option<glib::GString> {
        unsafe {
            from_glib_full(ffi::vte_terminal_get_text_selected(
                self.to_glib_none().0,
                format as _,
            ))
        }
    }
}

impl Terminal {
    /// Temporarily disable this during allocation to preserve original line wrapping.
    pub fn set_rewrap_on_resize(&self, enabled: bool) {
        self.set_property("rewrap-on-resize", enabled);
    }
    pub fn rewrap_on_resize(&self) -> bool {
        self.property("rewrap-on-resize")
    }
    pub fn cursor_position(&self) -> (libc::c_long, libc::c_long) {
        let (mut column, mut row) = (0, 0);
        unsafe {
            ffi::vte_terminal_get_cursor_position(self.to_glib_none().0, &mut column, &mut row);
        }
        (column, row)
    }
    /// Extract an inclusive rectangle of terminal cells without attribute arrays.
    pub fn text_range_format(
        &self,
        format: Format,
        start_row: libc::c_long,
        start_column: libc::c_long,
        end_row: libc::c_long,
        end_column: libc::c_long,
    ) -> Option<glib::GString> {
        unsafe {
            from_glib_full(ffi::vte_terminal_get_text_range_format(
                self.to_glib_none().0,
                format as _,
                start_row,
                start_column,
                end_row,
                end_column,
                std::ptr::null_mut(),
            ))
        }
    }
}
impl Pty {
    /// Kernel PTY size, in (columns, rows) order, matching Terminal's grid API.
    pub fn size(&self) -> Result<(i32, i32), glib::Error> {
        unsafe {
            let (mut rows, mut columns) = (0, 0);
            let mut error = std::ptr::null_mut();
            let ok =
                ffi::vte_pty_get_size(self.to_glib_none().0, &mut rows, &mut columns, &mut error);
            if ok != 0 {
                Ok((columns, rows))
            } else if !error.is_null() {
                Err(from_glib_full(error))
            } else {
                Err(glib::Error::new(
                    gio::IOErrorEnum::Failed,
                    "Unable to read PTY size",
                ))
            }
        }
    }
    /// Bytes waiting for VTE, without reading or consuming any output.
    pub fn unread_bytes(&self) -> usize {
        let mut bytes: libc::c_int = 0;
        let ok = unsafe { libc::ioctl(self.fd(), libc::FIONREAD, &mut bytes) };
        if ok == 0 {
            bytes.max(0) as usize
        } else {
            0
        }
    }
    /// Whether the program still uses canonical or echoing input (startup replay).
    pub fn canonical_or_echo(&self) -> bool {
        let mut state = std::mem::MaybeUninit::<libc::termios>::uninit();
        unsafe {
            libc::tcgetattr(self.fd(), state.as_mut_ptr()) == 0
                && state.assume_init().c_lflag & (libc::ECHO | libc::ICANON) != 0
        }
    }
}
impl Terminal {
    pub fn cursor_shape(&self) -> CursorShape {
        match unsafe { ffi::vte_terminal_get_cursor_shape(self.to_glib_none().0) } {
            1 => CursorShape::Ibeam,
            2 => CursorShape::Underline,
            _ => CursorShape::Block,
        }
    }
    pub fn cursor_blink_mode(&self) -> CursorBlinkMode {
        match unsafe { ffi::vte_terminal_get_cursor_blink_mode(self.to_glib_none().0) } {
            1 => CursorBlinkMode::On,
            2 => CursorBlinkMode::Off,
            _ => CursorBlinkMode::System,
        }
    }
}
