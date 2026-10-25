//! Windows stub backend: the public API of the libvte binding with nothing behind it.
//!
//! `Terminal` is a `gtk::Box` that shows a notice; no child process ever starts and no
//! output is ever produced. Settings round-trip (set, then get) so host logic that reads
//! them back behaves; everything else is a harmless no-op. `spawn_async*` reports failure
//! through its callback so callers take their normal "could not start" path.
//!
//! Signature differences from the Unix wrapper (see `unix.rs`):
//! * grid sizes and counts use `i64` where Unix uses `libc::c_long` (`i32` on Windows),
//!   so callers compile unchanged on both platforms;
//! * `Pty::fd()` returns `i32` (always `-1`) because `RawFd` does not exist;
//! * `Terminal` has no `gtk::Scrollable` interface, so `vadjustment()` and `hadjustment()`
//!   are inherent methods returning `Option<gtk::Adjustment>` (call syntax is unchanged);
//! * there is no `ffi` re-export.
//!
//! Kept as an opt-in compatibility fixture; Windows uses the ConPTY backend.
use crate::{CursorBlinkMode, CursorShape, Format};
use glib::prelude::*;
use glib::subclass::prelude::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

/// Grid dimension type, equal to `libc::c_long` on 64-bit Unix.
pub type CLong = i64;

/// Text of the notice shown in place of the terminal.
pub const NOTICE: &str = "Terminal is not available in this Windows build yet.";

pub fn init() {}

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::sync::OnceLock;

    pub struct Terminal {
        pub columns: Cell<CLong>,
        pub rows: Cell<CLong>,
        pub scrollback: Cell<CLong>,
        pub font: RefCell<Option<pango::FontDescription>>,
        pub font_scale: Cell<f64>,
        pub cursor_shape: Cell<CursorShape>,
        pub cursor_blink_mode: Cell<CursorBlinkMode>,
        pub rewrap: Cell<bool>,
        pub audible_bell: Cell<bool>,
        pub scroll_on_output: Cell<bool>,
        pub scroll_on_keystroke: Cell<bool>,
        pub mouse_autohide: Cell<bool>,
        pub allow_hyperlink: Cell<bool>,
        pub input_enabled: Cell<bool>,
        pub bold_is_bright: Cell<bool>,
        pub vadjustment: gtk::Adjustment,
        pub hadjustment: gtk::Adjustment,
    }
    impl Default for Terminal {
        fn default() -> Self {
            Self {
                columns: Cell::new(80),
                rows: Cell::new(24),
                scrollback: Cell::new(10_000),
                font: RefCell::new(None),
                font_scale: Cell::new(1.0),
                cursor_shape: Cell::new(CursorShape::Block),
                cursor_blink_mode: Cell::new(CursorBlinkMode::System),
                rewrap: Cell::new(true),
                audible_bell: Cell::new(true),
                scroll_on_output: Cell::new(true),
                scroll_on_keystroke: Cell::new(false),
                mouse_autohide: Cell::new(false),
                allow_hyperlink: Cell::new(false),
                input_enabled: Cell::new(true),
                bold_is_bright: Cell::new(false),
                vadjustment: gtk::Adjustment::new(0.0, 0.0, 1.0, 1.0, 1.0, 1.0),
                hadjustment: gtk::Adjustment::new(0.0, 0.0, 1.0, 1.0, 1.0, 1.0),
            }
        }
    }
    #[glib::object_subclass]
    impl ObjectSubclass for Terminal {
        const NAME: &'static str = "ImproStubTerminal";
        type Type = super::Terminal;
        type ParentType = gtk::Box;
    }
    impl ObjectImpl for Terminal {
        fn constructed(&self) {
            self.parent_constructed();
            let widget = self.obj();
            widget.set_orientation(gtk::Orientation::Vertical);
            let label = gtk::Label::new(Some(NOTICE));
            label.set_halign(gtk::Align::Center);
            label.set_valign(gtk::Align::Center);
            label.set_hexpand(true);
            label.set_vexpand(true);
            label.set_line_wrap(true);
            label.style_context().add_class("dim-label");
            widget.add(&label);
            label.show();
        }
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: OnceLock<Vec<glib::ParamSpec>> = OnceLock::new();
            PROPERTIES.get_or_init(|| {
                vec![
                    glib::ParamSpecBoolean::builder("rewrap-on-resize")
                        .default_value(true)
                        .build(),
                    glib::ParamSpecString::builder("current-directory-uri")
                        .read_only()
                        .build(),
                ]
            })
        }
        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            if pspec.name() == "rewrap-on-resize" {
                self.rewrap.set(value.get().expect("boolean value"));
            }
        }
        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "rewrap-on-resize" => self.rewrap.get().to_value(),
                _ => Option::<String>::None.to_value(),
            }
        }
        fn signals() -> &'static [glib::subclass::Signal] {
            use glib::subclass::Signal;
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("selection-changed").build(),
                    Signal::builder("contents-changed").build(),
                    Signal::builder("window-title-changed").build(),
                    Signal::builder("child-exited")
                        .param_types([i32::static_type()])
                        .build(),
                    Signal::builder("commit")
                        .param_types([String::static_type(), u32::static_type()])
                        .build(),
                ]
            })
        }
    }
    impl WidgetImpl for Terminal {}
    impl ContainerImpl for Terminal {}
    impl BoxImpl for Terminal {}
}

glib::wrapper! {
    pub struct Terminal(ObjectSubclass<imp::Terminal>)
        @extends gtk::Box, gtk::Container, gtk::Widget,
        @implements gtk::Buildable, gtk::Orientable;
}

/// Placeholder for the PTY of a running terminal. No instance is ever created: `pty()`
/// is always `None`.
#[derive(Clone, Debug)]
pub struct Pty(());

impl Pty {
    /// Always `-1`: there is no descriptor on this backend.
    pub fn fd(&self) -> i32 {
        -1
    }
    pub fn size(&self) -> Result<(i32, i32), glib::Error> {
        Err(unavailable())
    }
    pub fn unread_bytes(&self) -> usize {
        0
    }
    pub fn canonical_or_echo(&self) -> bool {
        false
    }
}

fn unavailable() -> glib::Error {
    glib::Error::new(gio::IOErrorEnum::NotSupported, NOTICE)
}

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
        glib::Object::new()
    }

    /// Always fails: the callback receives `Err` (`IOErrorEnum::NotSupported`) from an idle
    /// handler, like the asynchronous completion of the Unix backend.
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

    #[allow(clippy::too_many_arguments)]
    pub fn spawn_async_path<F: FnOnce(Result<glib::Pid, glib::Error>) + 'static>(
        &self,
        _cwd: Option<&std::path::Path>,
        _argv: &[&str],
        _envv: &[&str],
        _flags: glib::SpawnFlags,
        _timeout: i32,
        _cancellable: Option<&gio::Cancellable>,
        callback: F,
    ) {
        glib::idle_add_local_once(move || callback(Err(unavailable())));
    }

    /// Vertical adjustment a scrollbar can bind to; never changes.
    pub fn vadjustment(&self) -> Option<gtk::Adjustment> {
        Some(self.imp().vadjustment.clone())
    }
    pub fn hadjustment(&self) -> Option<gtk::Adjustment> {
        Some(self.imp().hadjustment.clone())
    }

    pub fn set_size(&self, columns: CLong, rows: CLong) {
        self.imp().columns.set(columns);
        self.imp().rows.set(rows);
    }
    pub fn set_scrollback_lines(&self, lines: CLong) {
        self.imp().scrollback.set(lines);
    }
    pub fn has_selection(&self) -> bool {
        false
    }
    pub fn pty(&self) -> Option<Pty> {
        None
    }
    pub fn reset(&self, _clear_tabstops: bool, _clear_history: bool) {}
    pub fn copy_clipboard_format(&self, _format: Format) {}
    pub fn set_font(&self, font: &pango::FontDescription) {
        *self.imp().font.borrow_mut() = Some(font.clone());
    }
    pub fn font(&self) -> Option<pango::FontDescription> {
        self.imp().font.borrow().clone()
    }
    pub fn set_font_scale(&self, scale: f64) {
        self.imp().font_scale.set(scale);
    }
    pub fn font_scale(&self) -> f64 {
        self.imp().font_scale.get()
    }
    pub fn set_cursor_shape(&self, shape: CursorShape) {
        self.imp().cursor_shape.set(shape);
    }
    pub fn set_cursor_blink_mode(&self, mode: CursorBlinkMode) {
        self.imp().cursor_blink_mode.set(mode);
    }
    /// Validates the palette length like the Unix backend, then ignores the colours.
    pub fn set_colors(
        &self,
        _foreground: Option<&gdk::RGBA>,
        _background: Option<&gdk::RGBA>,
        palette: &[gdk::RGBA],
    ) {
        assert!(
            palette.is_empty() || palette.len() == 16 || palette.len() == 256,
            "VTE palette must have 0, 16 or 256 colors"
        );
    }
    pub fn hyperlink_check_event(&self, _event: &gdk::Event) -> Option<glib::GString> {
        None
    }
    pub fn set_audible_bell(&self, value: bool) {
        self.imp().audible_bell.set(value);
    }
    pub fn audible_bell(&self) -> bool {
        self.imp().audible_bell.get()
    }
    pub fn set_scroll_on_output(&self, value: bool) {
        self.imp().scroll_on_output.set(value);
    }
    pub fn scroll_on_output(&self) -> bool {
        self.imp().scroll_on_output.get()
    }
    pub fn set_scroll_on_keystroke(&self, value: bool) {
        self.imp().scroll_on_keystroke.set(value);
    }
    pub fn scroll_on_keystroke(&self) -> bool {
        self.imp().scroll_on_keystroke.get()
    }
    pub fn set_mouse_autohide(&self, value: bool) {
        self.imp().mouse_autohide.set(value);
    }
    pub fn mouse_autohide(&self) -> bool {
        self.imp().mouse_autohide.get()
    }
    pub fn set_allow_hyperlink(&self, value: bool) {
        self.imp().allow_hyperlink.set(value);
    }
    pub fn allow_hyperlink(&self) -> bool {
        self.imp().allow_hyperlink.get()
    }
    pub fn set_input_enabled(&self, value: bool) {
        self.imp().input_enabled.set(value);
    }
    pub fn input_enabled(&self) -> bool {
        self.imp().input_enabled.get()
    }
    pub fn set_bold_is_bright(&self, value: bool) {
        self.imp().bold_is_bright.set(value);
    }
    pub fn bold_is_bright(&self) -> bool {
        self.imp().bold_is_bright.get()
    }
    pub fn select_all(&self) {}
    pub fn unselect_all(&self) {}
    pub fn paste_clipboard(&self) {}
    pub fn copy_primary(&self) {}
    pub fn paste_primary(&self) {}
    pub fn set_default_colors(&self) {}
    pub fn row_count(&self) -> CLong {
        self.imp().rows.get()
    }
    pub fn column_count(&self) -> CLong {
        self.imp().columns.get()
    }
    /// Nominal cell size in pixels (never zero, so callers can divide by it).
    pub fn char_width(&self) -> CLong {
        8
    }
    pub fn char_height(&self) -> CLong {
        16
    }
    pub fn scrollback_lines(&self) -> CLong {
        self.imp().scrollback.get()
    }
    pub fn window_title(&self) -> Option<glib::GString> {
        None
    }
    pub fn current_directory_uri(&self) -> Option<glib::GString> {
        None
    }
    pub fn current_file_uri(&self) -> Option<glib::GString> {
        None
    }
    pub fn feed(&self, _data: &[u8]) {}
    pub fn feed_child(&self, _data: &[u8]) {}
    pub fn set_color_cursor(&self, _color: Option<&gdk::RGBA>) {}
    pub fn set_color_foreground(&self, _color: Option<&gdk::RGBA>) {}
    pub fn set_color_background(&self, _color: Option<&gdk::RGBA>) {}
    pub fn set_color_highlight(&self, _color: Option<&gdk::RGBA>) {}
    pub fn set_color_highlight_foreground(&self, _color: Option<&gdk::RGBA>) {}
    pub fn set_color_cursor_foreground(&self, _color: Option<&gdk::RGBA>) {}

    pub fn connect_selection_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_unit_signal("selection-changed", f)
    }
    pub fn connect_contents_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_unit_signal("contents-changed", f)
    }
    pub fn connect_window_title_changed<F: Fn(&Self) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_unit_signal("window-title-changed", f)
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
    fn connect_unit_signal<F: Fn(&Self) + 'static>(
        &self,
        name: &str,
        f: F,
    ) -> glib::SignalHandlerId {
        let this = self.downgrade();
        self.connect_local(name, false, move |_| {
            if let Some(this) = this.upgrade() {
                f(&this);
            }
            None
        })
    }

    /// All visible terminal text plus scrollback; always empty here.
    pub fn text_format(&self, _format: Format) -> Option<glib::GString> {
        None
    }
    pub fn text_selected(&self, _format: Format) -> Option<glib::GString> {
        None
    }
    pub fn set_rewrap_on_resize(&self, enabled: bool) {
        self.set_property("rewrap-on-resize", enabled);
    }
    pub fn rewrap_on_resize(&self) -> bool {
        self.property("rewrap-on-resize")
    }
    pub fn cursor_position(&self) -> (CLong, CLong) {
        (0, 0)
    }
    pub fn text_range_format(
        &self,
        _format: Format,
        _start_row: CLong,
        _start_column: CLong,
        _end_row: CLong,
        _end_column: CLong,
    ) -> Option<glib::GString> {
        None
    }
    pub fn cursor_shape(&self) -> CursorShape {
        self.imp().cursor_shape.get()
    }
    pub fn cursor_blink_mode(&self) -> CursorBlinkMode {
        self.imp().cursor_blink_mode.get()
    }
}
