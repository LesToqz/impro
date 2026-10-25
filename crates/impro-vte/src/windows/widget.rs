use super::conpty::{self, Output, Session};
use crate::{
    emu::{self, term::TermMode, Screen},
    CursorBlinkMode, CursorShape, Format,
};
use alacritty_terminal::{event::Event, term::cell::Flags};
use glib::prelude::*;
use glib::subclass::prelude::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use std::os::windows::io::AsRawHandle;
use std::{
    cell::RefCell,
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};

/// Grid dimension type, equal to `libc::c_long` on 64-bit Unix.
pub type CLong = i64;

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::sync::OnceLock;

    pub struct Terminal {
        pub screen: RefCell<Screen>,
        pub session: RefCell<Option<Arc<Session>>>,
        pub pending_cancel: RefCell<Option<gio::Cancellable>>,
        pub im: gtk::IMMulticontext,
        pub cell_width: Cell<i64>,
        pub cell_height: Cell<i64>,
        pub colors: RefCell<Colors>,
        pub dragging: Cell<bool>,
        pub hover: RefCell<Option<String>>,
        pub hover_points: RefCell<Vec<emu::index::Point>>,
        pub resize_source: RefCell<Option<glib::SourceId>>,
        pub blink_source: RefCell<Option<glib::SourceId>>,
        pub cursor_visible: Cell<bool>,
        pub focused: Cell<bool>,
        pub spawning: Cell<bool>,
        pub exited: Cell<bool>,
        pub closed: Cell<bool>,
        pub max_slice: Cell<Duration>,
        pub layouts: RefCell<
            std::collections::HashMap<u16, std::collections::HashMap<String, pango::Layout>>,
        >,
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
                screen: RefCell::new(Screen::new(80, 24, 10_000)),
                session: RefCell::new(None),
                pending_cancel: RefCell::new(None),
                im: gtk::IMMulticontext::new(),
                cell_width: Cell::new(8),
                cell_height: Cell::new(16),
                colors: RefCell::new(Colors::default()),
                dragging: Cell::new(false),
                hover: RefCell::new(None),
                hover_points: RefCell::new(Vec::new()),
                resize_source: RefCell::new(None),
                blink_source: RefCell::new(None),
                cursor_visible: Cell::new(true),
                focused: Cell::new(false),
                spawning: Cell::new(false),
                exited: Cell::new(false),
                closed: Cell::new(false),
                max_slice: Cell::new(Duration::ZERO),
                layouts: RefCell::new(Default::default()),
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
        const NAME: &'static str = "ImproConptyTerminal";
        type Type = super::Terminal;
        type ParentType = gtk::DrawingArea;
    }
    impl ObjectImpl for Terminal {
        fn constructed(&self) {
            self.parent_constructed();
            let widget = self.obj();
            widget.setup();
        }
        fn dispose(&self) {
            let pending = self.pending_cancel.borrow_mut().take();
            if let Some(pending) = pending {
                pending.cancel();
            }
            let session = self.session.borrow_mut().take();
            if let Some(session) = session {
                session.stop();
            }
            for source in [&self.resize_source, &self.blink_source] {
                let id = source.borrow_mut().take();
                if let Some(id) = id {
                    id.remove();
                }
            }
            self.im.set_client_window(None::<&gdk::Window>);
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
                _ => self.obj().current_directory_uri().to_value(),
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
    impl WidgetImpl for Terminal {
        fn draw(&self, cr: &gtk::cairo::Context) -> glib::Propagation {
            self.obj().paint(cr);
            glib::Propagation::Stop
        }
        fn size_allocate(&self, allocation: &gtk::Allocation) {
            self.parent_size_allocate(allocation);
            let obj = self.obj();
            obj.set_size(
                (allocation.width() as i64 / self.cell_width.get()).max(2),
                (allocation.height() as i64 / self.cell_height.get()).max(1),
            );
        }
    }
    impl DrawingAreaImpl for Terminal {}
}

glib::wrapper! {
    pub struct Terminal(ObjectSubclass<imp::Terminal>)
        @extends gtk::DrawingArea, gtk::Widget,
        @implements gtk::Buildable;
}

/// A shared view of the live pseudo console. No Unix descriptor exists.
#[derive(Clone, Debug)]
pub struct Pty(Arc<Session>);
impl Pty {
    pub fn fd(&self) -> i32 {
        -1
    }
    pub fn size(&self) -> Result<(i32, i32), glib::Error> {
        Ok((
            self.0.columns.load(Ordering::Acquire),
            self.0.rows.load(Ordering::Acquire),
        ))
    }
    /// Output read from the pipe and awaiting emulator consumption.
    pub fn unread_bytes(&self) -> usize {
        self.0.unread.load(Ordering::Acquire)
    }
    /// ConPTY has no termios equivalent. Conservatively retain startup replay for
    /// the bounded one-second grace period; never guess from prompt characters.
    pub fn canonical_or_echo(&self) -> bool {
        true
    }
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

    /// Create a ConPTY child on a worker; complete on the GTK main context.
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
        cwd: Option<&std::path::Path>,
        argv: &[&str],
        envv: &[&str],
        flags: glib::SpawnFlags,
        timeout: i32,
        cancellable: Option<&gio::Cancellable>,
        callback: F,
    ) {
        if self.imp().closed.get() {
            callback(Err(glib::Error::new(
                gio::IOErrorEnum::Closed,
                "Terminal widget was destroyed",
            )));
            return;
        }
        if argv.is_empty()
            || argv.iter().chain(envv).any(|s| s.contains('\0'))
            || cwd.is_some_and(|p| {
                use std::os::windows::ffi::OsStrExt;
                p.as_os_str().encode_wide().any(|c| c == 0)
            })
        {
            callback(Err(glib::Error::new(
                gio::IOErrorEnum::InvalidArgument,
                "Spawn needs an executable and NUL-free arguments",
            )));
            return;
        }
        if self.imp().spawning.get() || self.pty().is_some() {
            callback(Err(glib::Error::new(
                gio::IOErrorEnum::Pending,
                "Terminal already has a child",
            )));
            return;
        }
        self.imp().spawning.set(true);
        self.imp().exited.set(false);
        let cancellable = cancellable.cloned().unwrap_or_default();
        *self.imp().pending_cancel.borrow_mut() = Some(cancellable.clone());
        let request = conpty::Request {
            cwd: cwd.map(std::path::Path::to_owned),
            argv: argv.iter().map(|s| (*s).to_owned()).collect(),
            env: envv.iter().map(|s| (*s).to_owned()).collect(),
            flags,
            timeout,
            cancellable: Some(cancellable.clone()),
            columns: self.column_count() as i32,
            rows: self.row_count() as i32,
        };
        #[allow(deprecated)]
        let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT);
        let cancel_tx = tx.clone();
        let handler = cancellable.connect_cancelled(move |_| {
            let _ = cancel_tx.send(Output::Aborted(gio::IOErrorEnum::Cancelled));
        });
        let timer = std::rc::Rc::new(RefCell::new(None));
        if timeout >= 0 {
            let timeout_tx = tx.clone();
            let cancel = cancellable.clone();
            let pending_timer = timer.clone();
            let id =
                glib::timeout_add_local_once(Duration::from_millis(timeout as u64), move || {
                    pending_timer.borrow_mut().take();
                    let _ = timeout_tx.send(Output::Aborted(gio::IOErrorEnum::TimedOut));
                    cancel.cancel();
                });
            *timer.borrow_mut() = Some(id);
        }
        let cancel_watch = cancellable.clone();
        let mut watch = SpawnWatch {
            timer,
            disconnect: Some(Box::new(move || {
                if let Some(handler) = handler {
                    cancel_watch.disconnect_cancelled(handler);
                }
            })),
        };
        let weak = self.downgrade();
        let mut callback = Some(callback);
        rx.attach(None, move |event| {
            let Some(t) = weak.upgrade() else {
                if let Output::Ready(Ok(session)) = event {
                    session.stop();
                }
                return glib::ControlFlow::Break;
            };
            match event {
                Output::Aborted(code) => {
                    watch.clear();
                    if let Some(callback) = callback.take() {
                        callback(Err(glib::Error::new(
                            code,
                            if code == gio::IOErrorEnum::TimedOut {
                                "Terminal spawn timed out"
                            } else {
                                "Terminal spawn cancelled"
                            },
                        )));
                    }
                }
                Output::Ready(result) => {
                    watch.clear();
                    t.imp().pending_cancel.borrow_mut().take();
                    t.imp().spawning.set(false);
                    if callback.is_none() {
                        if let Ok(session) = result {
                            session.stop();
                        }
                        return glib::ControlFlow::Break;
                    }
                    let result = result
                        .map(|session| {
                            if t.imp().closed.get() {
                                session.stop();
                                return Err(glib::Error::new(
                                    gio::IOErrorEnum::Cancelled,
                                    "Terminal widget was destroyed",
                                ));
                            }
                            let pid = glib::Pid(session.process.as_raw_handle());
                            *t.imp().session.borrow_mut() = Some(session);
                            t.schedule_resize();
                            Ok(pid)
                        })
                        .map_err(|message| {
                            t.feed(format!("{message}\r\n").as_bytes());
                            glib::Error::new(gio::IOErrorEnum::Failed, &message)
                        })
                        .and_then(|r| r);
                    if let Some(callback) = callback.take() {
                        callback(result);
                    }
                }
                Output::Data(packet, ack) => {
                    let weak = t.downgrade();
                    // One packet is outstanding. Feed at most 256 bytes at once,
                    // yielding after a 2 ms budget, even within a single packet.
                    let mut packet = Some(packet);
                    let mut ack = Some(ack);
                    let mut consumed = 0;
                    glib::idle_add_local(move || {
                        let Some(t) = weak.upgrade().filter(|t| !t.imp().closed.get()) else {
                            packet.take();
                            if let Some(ack) = ack.take() {
                                let _ = ack.send(());
                            }
                            return glib::ControlFlow::Break;
                        };
                        let start = Instant::now();
                        let length = packet.as_ref().expect("pending output").bytes.len();
                        while consumed < length {
                            let end = (consumed + 256).min(length);
                            t.feed_data(
                                &packet.as_ref().expect("pending output").bytes[consumed..end],
                            );
                            consumed = end;
                            if start.elapsed() >= Duration::from_millis(2) || t.imp().closed.get() {
                                break;
                            }
                        }
                        let finished = consumed == length || t.imp().closed.get();
                        if finished {
                            packet.take();
                            if !t.imp().closed.get() {
                                t.emit_by_name::<()>("contents-changed", &[]);
                            }
                            if let Some(ack) = ack.take() {
                                let _ = ack.send(());
                            }
                        }
                        t.imp()
                            .max_slice
                            .set(t.imp().max_slice.get().max(start.elapsed()));
                        if finished {
                            glib::ControlFlow::Break
                        } else {
                            glib::ControlFlow::Continue
                        }
                    });
                }
                Output::Exited(status) => {
                    let session = t.imp().session.borrow_mut().take();
                    drop(session);
                    if !t.imp().exited.replace(true) && !t.imp().closed.get() {
                        t.emit_by_name::<()>("child-exited", &[&status]);
                    }
                    return glib::ControlFlow::Break;
                }
            }
            glib::ControlFlow::Continue
        });
        std::thread::spawn(move || {
            if let Err(error) = conpty::spawn(request, tx.clone()) {
                let _ = tx.send(Output::Ready(Err(error.to_string())));
            }
        });
    }

    /// Vertical viewport adjustment, including the current scrollback extent.
    pub fn vadjustment(&self) -> Option<gtk::Adjustment> {
        Some(self.imp().vadjustment.clone())
    }
    pub fn hadjustment(&self) -> Option<gtk::Adjustment> {
        Some(self.imp().hadjustment.clone())
    }

    pub fn set_size(&self, columns: CLong, rows: CLong) {
        let columns = columns.clamp(2, 32767);
        let rows = rows.clamp(1, 32767);
        if (columns, rows) == (self.column_count(), self.row_count()) {
            return;
        }
        self.imp().columns.set(columns);
        self.imp().rows.set(rows);
        let rewrap = self.rewrap_on_resize();
        self.imp()
            .screen
            .borrow_mut()
            .resize(columns as usize, rows as usize, rewrap);
        self.update_adjustment();
        self.schedule_resize();
        self.queue_draw();
    }
    pub fn set_scrollback_lines(&self, lines: CLong) {
        self.imp().scrollback.set(lines);
        self.imp().screen.borrow_mut().set_scrollback(if lines < 0 {
            usize::MAX / 2
        } else {
            lines as usize
        });
        self.update_adjustment();
        self.queue_draw();
    }
    pub fn has_selection(&self) -> bool {
        self.imp()
            .screen
            .borrow()
            .term
            .selection
            .as_ref()
            .is_some_and(|s| !s.is_empty())
    }
    pub fn pty(&self) -> Option<Pty> {
        self.imp().session.borrow().clone().map(Pty)
    }
    pub fn reset(&self, clear_tabstops: bool, clear_history: bool) {
        let selected = self.has_selection();
        self.imp()
            .screen
            .borrow_mut()
            .reset(clear_tabstops, clear_history);
        self.update_adjustment();
        self.restart_blink();
        self.queue_draw();
        if selected {
            self.emit_by_name::<()>("selection-changed", &[]);
        }
        self.emit_by_name::<()>("contents-changed", &[]);
    }
    pub fn copy_clipboard_format(&self, format: Format) {
        let text = self.imp().screen.borrow().selection();
        if let Some(text) = text {
            let clipboard = gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD);
            if format == Format::Text {
                clipboard.set_text(&text);
            } else {
                let html = self.styled_html(true).unwrap_or_else(|| emu::html(&text));
                clipboard.set_with_data(
                    &[
                        gtk::TargetEntry::new("UTF8_STRING", gtk::TargetFlags::empty(), 0),
                        gtk::TargetEntry::new("text/html", gtk::TargetFlags::empty(), 1),
                    ],
                    move |_, data, info| {
                        if info == 0 {
                            data.set_text(&text);
                        } else {
                            data.set(&gdk::Atom::intern("text/html"), 8, html.as_bytes());
                        }
                    },
                );
            }
        }
    }
    pub fn set_font(&self, font: &pango::FontDescription) {
        *self.imp().font.borrow_mut() = Some(font.clone());
        self.font_changed();
    }
    pub fn font(&self) -> Option<pango::FontDescription> {
        self.imp().font.borrow().clone()
    }
    pub fn set_font_scale(&self, scale: f64) {
        self.imp().font_scale.set(scale.clamp(0.1, 10.0));
        self.font_changed();
    }
    pub fn font_scale(&self) -> f64 {
        self.imp().font_scale.get()
    }
    pub fn set_cursor_shape(&self, shape: CursorShape) {
        self.imp().cursor_shape.set(shape);
        self.update_cursor_style();
        self.queue_draw();
    }
    pub fn set_cursor_blink_mode(&self, mode: CursorBlinkMode) {
        self.imp().cursor_blink_mode.set(mode);
        self.update_cursor_style();
        self.restart_blink();
    }
    /// Set default foreground/background and the 16- or 256-colour palette.
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
        {
            let mut colors = self.imp().colors.borrow_mut();
            if let Some(c) = foreground {
                colors.foreground = *c;
            }
            if let Some(c) = background {
                colors.background = *c;
            }
            for (i, c) in palette.iter().enumerate() {
                colors.palette[i] = *c;
            }
        }
        self.queue_draw();
    }
    pub fn hyperlink_check_event(&self, event: &gdk::Event) -> Option<glib::GString> {
        if !self.allow_hyperlink() {
            return None;
        }
        let (x, y) = event.coords()?;
        let (col, row) = self.point(x, y);
        self.imp()
            .screen
            .borrow()
            .hyperlink(col, row)
            .map(Into::into)
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
        if !value {
            self.imp().hover.borrow_mut().take();
            self.imp().hover_points.borrow_mut().clear();
        }
        self.queue_draw();
    }
    pub fn allow_hyperlink(&self) -> bool {
        self.imp().allow_hyperlink.get()
    }
    pub fn set_input_enabled(&self, value: bool) {
        self.imp().input_enabled.set(value);
        self.restart_blink();
    }
    pub fn input_enabled(&self) -> bool {
        self.imp().input_enabled.get()
    }
    pub fn set_bold_is_bright(&self, value: bool) {
        self.imp().bold_is_bright.set(value);
        self.queue_draw();
    }
    pub fn bold_is_bright(&self) -> bool {
        self.imp().bold_is_bright.get()
    }
    pub fn select_all(&self) {
        self.imp().screen.borrow_mut().select_all();
        self.selection_changed();
    }
    pub fn unselect_all(&self) {
        self.imp().screen.borrow_mut().term.selection = None;
        self.selection_changed();
    }
    pub fn paste_clipboard(&self) {
        let weak = self.downgrade();
        gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).request_text(move |_, text| {
            if let (Some(t), Some(text)) = (weak.upgrade(), text) {
                let bytes = t.imp().screen.borrow().paste(text);
                t.typed(&bytes);
            }
        });
    }
    pub fn copy_primary(&self) {}
    pub fn paste_primary(&self) {}
    pub fn set_default_colors(&self) {
        *self.imp().colors.borrow_mut() = Colors::default();
        self.queue_draw();
    }
    pub fn row_count(&self) -> CLong {
        self.imp().rows.get()
    }
    pub fn column_count(&self) -> CLong {
        self.imp().columns.get()
    }
    /// Cell dimensions in pixels, derived from the rendered font metrics.
    pub fn char_width(&self) -> CLong {
        self.imp().cell_width.get()
    }
    pub fn char_height(&self) -> CLong {
        self.imp().cell_height.get()
    }
    pub fn scrollback_lines(&self) -> CLong {
        self.imp().scrollback.get()
    }
    pub fn window_title(&self) -> Option<glib::GString> {
        self.imp().screen.borrow().title.clone().map(Into::into)
    }
    pub fn current_directory_uri(&self) -> Option<glib::GString> {
        self.imp().screen.borrow().cwd().map(Into::into)
    }
    pub fn current_file_uri(&self) -> Option<glib::GString> {
        None
    }
    pub fn feed(&self, data: &[u8]) {
        self.feed_data(data);
        self.emit_by_name::<()>("contents-changed", &[]);
    }
    /// Parse display bytes. Output dispatch emits `contents-changed` once per
    /// consumed packet itself instead of once per parser slice.
    fn feed_data(&self, data: &[u8]) {
        let (events, cwd_changed, selection_changed, cursor_changed, damage) = {
            let mut screen = self.imp().screen.borrow_mut();
            let old = screen.cwd().map(str::to_owned);
            let selected = screen
                .term
                .selection
                .as_ref()
                .and_then(|s| s.to_range(&screen.term))
                .map(|s| (s.start, s.end, s.is_block));
            let old_cursor = (
                screen.mode().contains(TermMode::SHOW_CURSOR),
                screen.term.cursor_style(),
            );
            let events = screen.feed(data);
            if self.scroll_on_output() {
                screen.bottom();
            }
            let changed = selected
                != screen
                    .term
                    .selection
                    .as_ref()
                    .and_then(|s| s.to_range(&screen.term))
                    .map(|s| (s.start, s.end, s.is_block));
            let damage = match screen.term.damage() {
                emu::term::TermDamage::Full => None,
                emu::term::TermDamage::Partial(lines) => Some(lines.collect::<Vec<_>>()),
            };
            screen.term.reset_damage();
            (
                events,
                old.as_deref() != screen.cwd(),
                changed,
                old_cursor
                    != (
                        screen.mode().contains(TermMode::SHOW_CURSOR),
                        screen.term.cursor_style(),
                    ),
                damage,
            )
        };
        let mut title_changed = false;
        let mut bell = false;
        for event in events {
            match event {
                Event::Title(_) | Event::ResetTitle => {
                    title_changed = true;
                }
                Event::Bell if self.audible_bell() => bell = true,
                Event::CursorBlinkingChange => self.restart_blink(),
                Event::PtyWrite(text) => self.feed_child(text.as_bytes()),
                Event::TextAreaSizeRequest(format) => self.feed_child(
                    format(alacritty_terminal::event::WindowSize {
                        num_cols: self.column_count() as u16,
                        num_lines: self.row_count() as u16,
                        cell_width: self.char_width() as u16,
                        cell_height: self.char_height() as u16,
                    })
                    .as_bytes(),
                ),
                Event::ColorRequest(index, format) => {
                    let c = self.imp().colors.borrow().indexed(index);
                    self.feed_child(
                        format(emu::Rgb {
                            r: (c.red() * 255.) as u8,
                            g: (c.green() * 255.) as u8,
                            b: (c.blue() * 255.) as u8,
                        })
                        .as_bytes(),
                    );
                }
                _ => (),
            }
        }
        if cursor_changed {
            self.restart_blink();
        }
        self.imp().hover.borrow_mut().take();
        self.imp().hover_points.borrow_mut().clear();
        if title_changed {
            self.emit_by_name::<()>("window-title-changed", &[]);
        }
        if bell {
            self.display().beep();
        }
        if cwd_changed {
            self.notify("current-directory-uri");
        }
        self.update_adjustment();
        if let Some(lines) = damage {
            let cw = self.char_width() as i32;
            let ch = self.char_height() as i32;
            for line in lines {
                self.queue_draw_area(
                    line.left.saturating_sub(1) as i32 * cw,
                    line.line as i32 * ch,
                    (line.right - line.left + 3) as i32 * cw,
                    ch,
                );
            }
        } else {
            self.queue_draw();
        }
        if selection_changed {
            self.emit_by_name::<()>("selection-changed", &[]);
        }
    }
    pub fn feed_child(&self, data: &[u8]) {
        let session = self.imp().session.borrow().clone();
        if let Some(session) = session {
            session.write(data);
        }
    }
    pub fn set_color_cursor(&self, color: Option<&gdk::RGBA>) {
        self.imp().colors.borrow_mut().cursor = color.copied().unwrap_or(Colors::default().cursor);
        self.queue_draw();
    }
    pub fn set_color_foreground(&self, color: Option<&gdk::RGBA>) {
        self.imp().colors.borrow_mut().foreground =
            color.copied().unwrap_or(Colors::default().foreground);
        self.queue_draw();
    }
    pub fn set_color_background(&self, color: Option<&gdk::RGBA>) {
        self.imp().colors.borrow_mut().background =
            color.copied().unwrap_or(Colors::default().background);
        self.queue_draw();
    }
    pub fn set_color_highlight(&self, color: Option<&gdk::RGBA>) {
        self.imp().colors.borrow_mut().highlight =
            color.copied().unwrap_or(Colors::default().highlight);
        self.queue_draw();
    }
    pub fn set_color_highlight_foreground(&self, color: Option<&gdk::RGBA>) {
        self.imp().colors.borrow_mut().highlight_foreground = color
            .copied()
            .unwrap_or(Colors::default().highlight_foreground);
        self.queue_draw();
    }
    pub fn set_color_cursor_foreground(&self, color: Option<&gdk::RGBA>) {
        self.imp().colors.borrow_mut().cursor_foreground = color
            .copied()
            .unwrap_or(Colors::default().cursor_foreground);
        self.queue_draw();
    }

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

    /// Current viewport text; select-all and explicit ranges also expose scrollback.
    pub fn text_format(&self, format: Format) -> Option<glib::GString> {
        if format == Format::Html {
            return self.styled_html(false).map(Into::into);
        }
        Some(self.imp().screen.borrow().viewport_text().into())
    }
    pub fn text_selected(&self, format: Format) -> Option<glib::GString> {
        if format == Format::Html {
            return self.styled_html(true).map(Into::into);
        }
        self.imp().screen.borrow().selection().map(Into::into)
    }
    pub fn set_rewrap_on_resize(&self, enabled: bool) {
        self.set_property("rewrap-on-resize", enabled);
    }
    pub fn rewrap_on_resize(&self) -> bool {
        self.property("rewrap-on-resize")
    }
    pub fn cursor_position(&self) -> (CLong, CLong) {
        self.imp().screen.borrow().cursor()
    }
    pub fn text_range_format(
        &self,
        format: Format,
        start_row: CLong,
        start_column: CLong,
        end_row: CLong,
        end_column: CLong,
    ) -> Option<glib::GString> {
        if format == Format::Html {
            let colors = self.imp().colors.borrow().clone();
            let html = self.imp().screen.borrow().html_range(
                (start_row, start_column, end_row, end_column),
                |c, bright| {
                    let c = colors.resolve(c, bright);
                    (
                        (c.red() * 255.) as u8,
                        (c.green() * 255.) as u8,
                        (c.blue() * 255.) as u8,
                    )
                },
                self.bold_is_bright(),
            );
            return Some(html.into());
        }
        Some(
            self.imp()
                .screen
                .borrow()
                .range(start_row, start_column, end_row, end_column)
                .into(),
        )
    }
    pub fn cursor_shape(&self) -> CursorShape {
        self.imp().cursor_shape.get()
    }
    pub fn cursor_blink_mode(&self) -> CursorBlinkMode {
        self.imp().cursor_blink_mode.get()
    }
}

struct SpawnWatch {
    timer: std::rc::Rc<RefCell<Option<glib::SourceId>>>,
    disconnect: Option<Box<dyn FnOnce()>>,
}
impl SpawnWatch {
    fn clear(&mut self) {
        let timer = self.timer.borrow_mut().take();
        if let Some(timer) = timer {
            timer.remove();
        }
        if let Some(disconnect) = self.disconnect.take() {
            disconnect();
        }
    }
}
impl Drop for SpawnWatch {
    fn drop(&mut self) {
        self.clear();
    }
}

#[derive(Clone)]
pub struct Colors {
    foreground: gdk::RGBA,
    background: gdk::RGBA,
    cursor: gdk::RGBA,
    cursor_foreground: gdk::RGBA,
    highlight: gdk::RGBA,
    highlight_foreground: gdk::RGBA,
    palette: Vec<gdk::RGBA>,
}
impl Default for Colors {
    fn default() -> Self {
        let base = [
            "#181818", "#cd3131", "#0dbc79", "#e5e510", "#2472c8", "#bc3fbc", "#11a8cd", "#e5e5e5",
            "#666666", "#f14c4c", "#23d18b", "#f5f543", "#3b8eea", "#d670d6", "#29b8db", "#ffffff",
        ];
        let mut palette = base
            .iter()
            .map(|s| gdk::RGBA::parse(s).unwrap())
            .collect::<Vec<_>>();
        let levels = [0., 95., 135., 175., 215., 255.];
        for i in 16..232 {
            let n = i - 16;
            palette.push(gdk::RGBA::new(
                levels[n / 36] / 255.,
                levels[n / 6 % 6] / 255.,
                levels[n % 6] / 255.,
                1.,
            ));
        }
        for i in 0..24 {
            let c = (8. + 10. * i as f64) / 255.;
            palette.push(gdk::RGBA::new(c, c, c, 1.));
        }
        Self {
            foreground: palette[7],
            background: palette[0],
            cursor: palette[7],
            cursor_foreground: palette[0],
            highlight: gdk::RGBA::parse("#264f78").unwrap(),
            highlight_foreground: palette[15],
            palette,
        }
    }
}
impl Colors {
    fn indexed(&self, index: usize) -> gdk::RGBA {
        match index {
            0..=255 => self.palette[index],
            257 => self.background,
            258 => self.cursor,
            _ => self.foreground,
        }
    }
    fn resolve(&self, color: emu::Color, bright: bool) -> gdk::RGBA {
        match color {
            emu::Color::Spec(rgb) => gdk::RGBA::new(
                rgb.r as f64 / 255.,
                rgb.g as f64 / 255.,
                rgb.b as f64 / 255.,
                1.,
            ),
            emu::Color::Indexed(i) => {
                self.palette[i as usize + if bright && i < 8 { 8 } else { 0 }]
            }
            emu::Color::Named(n) => {
                let index = n as usize;
                if index < 16 {
                    self.palette[index + if bright && index < 8 { 8 } else { 0 }]
                } else if (259..267).contains(&index) {
                    let c = self.palette[index - 259];
                    gdk::RGBA::new(c.red() * 0.66, c.green() * 0.66, c.blue() * 0.66, c.alpha())
                } else {
                    self.indexed(index)
                }
            }
        }
    }
}
fn color(cr: &gtk::cairo::Context, c: &gdk::RGBA) {
    cr.set_source_rgba(c.red(), c.green(), c.blue(), c.alpha());
}
impl Terminal {
    fn styled_html(&self, selected: bool) -> Option<String> {
        let colors = self.imp().colors.borrow().clone();
        self.imp().screen.borrow().html(
            selected,
            |c, bright| {
                let c = colors.resolve(c, bright);
                (
                    (c.red() * 255.) as u8,
                    (c.green() * 255.) as u8,
                    (c.blue() * 255.) as u8,
                )
            },
            self.bold_is_bright(),
        )
    }
    fn update_cursor_style(&self) {
        use alacritty_terminal::vte::ansi::{CursorShape as Shape, CursorStyle};
        let shape = match self.cursor_shape() {
            CursorShape::Block => Shape::Block,
            CursorShape::Ibeam => Shape::Beam,
            CursorShape::Underline => Shape::Underline,
        };
        self.imp()
            .screen
            .borrow_mut()
            .set_cursor_style(CursorStyle {
                shape,
                blinking: self.cursor_blink_mode() != CursorBlinkMode::Off,
            });
    }
    fn setup(&self) {
        self.set_can_focus(true);
        self.connect_destroy(|t| {
            t.imp().closed.set(true);
            let pending = t.imp().pending_cancel.borrow_mut().take();
            if let Some(pending) = pending {
                pending.cancel();
            }
            let session = t.imp().session.borrow_mut().take();
            if let Some(session) = session {
                session.stop();
            }
            for source in [&t.imp().resize_source, &t.imp().blink_source] {
                let id = source.borrow_mut().take();
                if let Some(id) = id {
                    id.remove();
                }
            }
        });
        self.add_events(
            gdk::EventMask::KEY_PRESS_MASK
                | gdk::EventMask::KEY_RELEASE_MASK
                | gdk::EventMask::BUTTON_PRESS_MASK
                | gdk::EventMask::BUTTON_RELEASE_MASK
                | gdk::EventMask::POINTER_MOTION_MASK
                | gdk::EventMask::SCROLL_MASK
                | gdk::EventMask::SMOOTH_SCROLL_MASK
                | gdk::EventMask::FOCUS_CHANGE_MASK,
        );
        let weak = self.downgrade();
        self.imp().im.connect_commit(move |_, text| {
            if let Some(t) = weak.upgrade() {
                t.typed(text.as_bytes());
            }
        });
        let weak = self.downgrade();
        self.imp().im.connect_preedit_changed(move |_| {
            if let Some(t) = weak.upgrade() {
                t.queue_draw();
            }
        });
        // After normal handlers: the panel owns copy/paste and pane shortcuts.
        let weak = self.downgrade();
        self.connect_local("key-press-event", true, move |values| {
            let result = weak
                .upgrade()
                .zip(values[1].get::<gdk::Event>().ok())
                .and_then(|(t, e)| e.downcast_ref::<gdk::EventKey>().map(|e| t.key(e)))
                .unwrap_or(false);
            Some(result.to_value())
        });
        let weak = self.downgrade();
        self.connect_key_release_event(move |_, event| {
            if let Some(t) = weak.upgrade() {
                if t.imp().im.filter_keypress(event) {
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        self.connect_realize(|t| {
            t.imp().im.set_client_window(t.window().as_ref());
            t.font_changed();
        });
        self.connect_unrealize(|t| t.imp().im.set_client_window(None::<&gdk::Window>));
        self.connect_focus_in_event(|t, _| {
            t.imp().focused.set(true);
            t.imp().im.focus_in();
            t.focus_report(true);
            t.restart_blink();
            glib::Propagation::Proceed
        });
        self.connect_focus_out_event(|t, _| {
            t.imp().focused.set(false);
            t.imp().im.focus_out();
            t.focus_report(false);
            t.restart_blink();
            glib::Propagation::Proceed
        });
        self.connect_unmap(|t| {
            let id = t.imp().blink_source.borrow_mut().take();
            if let Some(id) = id {
                id.remove();
            }
        });
        self.connect_map(|t| t.restart_blink());
        self.connect_button_press_event(|t, e| {
            let (x, y) = e.position();
            let (col, row) = t.point(x, y);
            t.grab_focus();
            if t.report_mouse(
                e.button().saturating_sub(1) as u8,
                col,
                row,
                e.state(),
                false,
                false,
            ) {
                return glib::Propagation::Stop;
            }
            if e.button() == 1 {
                t.imp().dragging.set(true);
                t.imp().screen.borrow_mut().select_start(col, row);
                t.selection_changed();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.connect_button_release_event(|t, e| {
            let (x, y) = e.position();
            let (col, row) = t.point(x, y);
            if t.report_mouse(
                e.button().saturating_sub(1) as u8,
                col,
                row,
                e.state(),
                true,
                false,
            ) {
                return glib::Propagation::Stop;
            }
            if e.button() == 1 {
                t.imp().dragging.set(false);
            }
            glib::Propagation::Proceed
        });
        self.connect_motion_notify_event(|t, e| {
            let (x, y) = e.position();
            let (col, row) = t.point(x, y);
            if let Some(window) = t.window() {
                window.set_cursor(None::<&gdk::Cursor>);
            }
            let button = if e.state().contains(gdk::ModifierType::BUTTON1_MASK) {
                0
            } else if e.state().contains(gdk::ModifierType::BUTTON2_MASK) {
                1
            } else if e.state().contains(gdk::ModifierType::BUTTON3_MASK) {
                2
            } else {
                3
            };
            if t.report_mouse(button, col, row, e.state(), false, true) {
                return glib::Propagation::Stop;
            }
            if t.imp().dragging.get() {
                t.imp().screen.borrow_mut().select_update(col, row);
                t.selection_changed();
            }
            let link = if t.allow_hyperlink() {
                t.imp().screen.borrow().hyperlink(col, row)
            } else {
                None
            };
            let changed = *t.imp().hover.borrow() != link;
            *t.imp().hover.borrow_mut() = link;
            if changed {
                let points = t
                    .imp()
                    .hover
                    .borrow()
                    .as_ref()
                    .map(|link| t.imp().screen.borrow().hyperlink_points(link))
                    .unwrap_or_default();
                *t.imp().hover_points.borrow_mut() = points;
                t.queue_draw();
            }
            glib::Propagation::Proceed
        });
        self.connect_scroll_event(|t, e| {
            let delta = match e.direction() {
                gdk::ScrollDirection::Up => 3,
                gdk::ScrollDirection::Down => -3,
                _ => {
                    if e.delta().1 > 0. {
                        -3
                    } else {
                        3
                    }
                }
            };
            let (x, y) = e.position();
            let (col, row) = t.point(x, y);
            if !t.report_mouse(
                if delta > 0 { 64 } else { 65 },
                col,
                row,
                e.state(),
                false,
                false,
            ) {
                t.imp().screen.borrow_mut().scroll(delta);
                t.update_adjustment();
                t.queue_draw();
            }
            glib::Propagation::Stop
        });
        let weak = self.downgrade();
        self.imp().vadjustment.connect_value_changed(move |a| {
            if let Some(t) = weak.upgrade() {
                let changed = if let Ok(mut screen) = t.imp().screen.try_borrow_mut() {
                    let desired = (screen.history() as f64 - a.value()).round().max(0.) as i32;
                    let delta = desired - screen.offset() as i32;
                    if delta != 0 {
                        screen.scroll(delta);
                    }
                    delta != 0
                } else {
                    false
                };
                if changed {
                    t.queue_draw();
                }
            }
        });
        self.font_changed();
    }
    fn point(&self, x: f64, y: f64) -> (usize, usize) {
        (
            (x / self.char_width() as f64).max(0.) as usize,
            (y / self.char_height() as f64).max(0.) as usize,
        )
    }
    fn report_mouse(
        &self,
        button: u8,
        col: usize,
        row: usize,
        state: gdk::ModifierType,
        release: bool,
        motion: bool,
    ) -> bool {
        if state.contains(gdk::ModifierType::SHIFT_MASK) || !self.input_enabled() {
            return false;
        }
        let (mode, x10) = {
            let screen = self.imp().screen.borrow();
            (screen.mode(), screen.x10_mouse())
        };
        if x10 && (release || motion || button >= 64) {
            return true;
        }
        if !mode.intersects(TermMode::MOUSE_MODE) {
            return false;
        }
        if motion
            && !mode.contains(TermMode::MOUSE_MOTION)
            && !(mode.contains(TermMode::MOUSE_DRAG) && button < 3)
        {
            return true;
        }
        let modifiers = if state.contains(gdk::ModifierType::SHIFT_MASK) {
            4
        } else {
            0
        } | if state.contains(gdk::ModifierType::MOD1_MASK) {
            8
        } else {
            0
        } | if state.contains(gdk::ModifierType::CONTROL_MASK) {
            16
        } else {
            0
        };
        self.feed_child(&emu::mouse(
            mode,
            button | if motion { 32 } else { 0 },
            col.min(self.column_count() as usize - 1),
            row.min(self.row_count() as usize - 1),
            modifiers,
            release,
        ));
        true
    }
    fn focus_report(&self, focused: bool) {
        let reporting = self
            .imp()
            .screen
            .borrow()
            .mode()
            .contains(TermMode::FOCUS_IN_OUT);
        if reporting {
            self.feed_child(if focused { b"\x1b[I" } else { b"\x1b[O" });
        }
        self.queue_draw();
    }
    fn key(&self, event: &gdk::EventKey) -> bool {
        if !self.input_enabled() {
            return true;
        }
        use emu::keys::{encode, Key, Modifiers};
        use gdk::keys::constants as k;
        let state = event.state();
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        let alt = state.contains(gdk::ModifierType::MOD1_MASK);
        // GDK marks layout-consumed Ctrl/Alt for AltGr. Never encode them as
        // terminal modifiers; the IM context gets first chance at composed text.
        let consumed = gdk::Keymap::for_display(&self.display())
            .and_then(|map| {
                map.translate_keyboard_state(
                    event.hardware_keycode() as u32,
                    state,
                    event.group() as i32,
                )
            })
            .map(|v| v.3)
            .unwrap_or_else(gdk::ModifierType::empty);
        let right_alt = unsafe {
            windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(
                windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_RMENU as i32,
            )
        } < 0;
        let altgr = (ctrl && alt && right_alt)
            || state.contains(gdk::ModifierType::MOD5_MASK)
            || (ctrl
                && alt
                && consumed
                    .intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::MOD1_MASK));
        if (!ctrl && !alt || altgr) && self.imp().im.filter_keypress(event) {
            return true;
        }
        let mode = self.imp().screen.borrow().mode();
        let application_keypad = if mode.contains(TermMode::APP_KEYPAD) {
            match event.keyval() {
                k::KP_Insert => Some('0'),
                k::KP_End => Some('1'),
                k::KP_Down => Some('2'),
                k::KP_Page_Down => Some('3'),
                k::KP_Left => Some('4'),
                k::KP_Begin => Some('5'),
                k::KP_Right => Some('6'),
                k::KP_Home => Some('7'),
                k::KP_Up => Some('8'),
                k::KP_Page_Up => Some('9'),
                k::KP_Delete => Some('.'),
                _ => None,
            }
        } else {
            None
        };
        let key = if let Some(c) = application_keypad {
            Key::Keypad(c)
        } else {
            match event.keyval() {
                k::Return | k::KP_Enter => {
                    if event.keyval() == k::KP_Enter {
                        Key::Keypad('\r')
                    } else {
                        Key::Enter
                    }
                }
                k::BackSpace => Key::Backspace,
                k::Tab | k::ISO_Left_Tab => Key::Tab,
                k::Escape => Key::Escape,
                k::Up | k::KP_Up => Key::Up,
                k::Down | k::KP_Down => Key::Down,
                k::Left | k::KP_Left => Key::Left,
                k::Right | k::KP_Right => Key::Right,
                k::Home | k::KP_Home => Key::Home,
                k::End | k::KP_End => Key::End,
                k::Page_Up | k::KP_Page_Up => Key::PageUp,
                k::Page_Down | k::KP_Page_Down => Key::PageDown,
                k::Insert | k::KP_Insert => Key::Insert,
                k::Delete | k::KP_Delete => Key::Delete,
                key if key >= k::F1 && key <= k::F12 => Key::Function((*key - *k::F1 + 1) as u8),
                key if key >= k::KP_0 && key <= k::KP_9 => {
                    Key::Keypad((b'0' + (*key - *k::KP_0) as u8) as char)
                }
                k::KP_Add => Key::Keypad('+'),
                k::KP_Subtract => Key::Keypad('-'),
                k::KP_Multiply => Key::Keypad('*'),
                k::KP_Divide => Key::Keypad('/'),
                k::KP_Decimal => Key::Keypad('.'),
                k::KP_Equal => Key::Keypad('='),
                key => {
                    if let Some(c) = key.to_unicode() {
                        Key::Text(c)
                    } else {
                        return false;
                    }
                }
            }
        };
        let bytes = encode(
            key,
            Modifiers {
                shift: state.contains(gdk::ModifierType::SHIFT_MASK),
                control: ctrl && !altgr,
                alt: alt && !altgr,
            },
            mode.contains(TermMode::APP_CURSOR),
            mode.contains(TermMode::APP_KEYPAD),
        );
        self.typed(&bytes);
        true
    }
    fn typed(&self, bytes: &[u8]) {
        if !self.input_enabled() || bytes.is_empty() {
            return;
        }
        if self.scroll_on_keystroke() {
            self.imp().screen.borrow_mut().bottom();
            self.update_adjustment();
        }
        if self.mouse_autohide() {
            if let Some(w) = self.window() {
                w.set_cursor(gdk::Cursor::from_name(&self.display(), "none").as_ref());
            }
        }
        if self.has_selection() {
            self.unselect_all();
        }
        self.feed_child(bytes);
        let text = String::from_utf8_lossy(bytes);
        self.emit_by_name::<()>("commit", &[&text.as_ref(), &(bytes.len() as u32)]);
        self.imp().cursor_visible.set(true);
        self.queue_draw();
    }
    fn selection_changed(&self) {
        self.queue_draw();
        self.emit_by_name::<()>("selection-changed", &[]);
    }
    fn update_adjustment(&self) {
        let (history, rows, offset) = {
            let screen = self.imp().screen.borrow();
            (
                screen.history() as f64,
                screen.rows() as f64,
                screen.offset() as f64,
            )
        };
        self.imp().vadjustment.configure(
            history - offset,
            0.,
            history + rows,
            1.,
            rows.max(1.) - 1.,
            rows,
        );
    }
    fn schedule_resize(&self) {
        if self.imp().resize_source.borrow().is_some() {
            return;
        }
        let weak = self.downgrade();
        let id = glib::timeout_add_local_once(Duration::from_millis(16), move || {
            if let Some(t) = weak.upgrade() {
                t.imp().resize_source.borrow_mut().take();
                let session = t.imp().session.borrow().clone();
                if let Some(s) = session {
                    s.resize(t.column_count() as i32, t.row_count() as i32);
                }
            }
        });
        *self.imp().resize_source.borrow_mut() = Some(id);
    }
    fn scaled_font(&self) -> pango::FontDescription {
        let mut font = self
            .font()
            .unwrap_or_else(|| pango::FontDescription::from_string("Monospace 11"));
        font.set_size((font.size() as f64 * self.font_scale()) as i32);
        font
    }
    fn font_changed(&self) {
        let font = self.scaled_font();
        let metrics = self.pango_context().metrics(Some(&font), None);
        self.imp().cell_width.set(
            ((metrics.approximate_digit_width() as f64 / pango::SCALE as f64).ceil() as i64).max(1),
        );
        self.imp().cell_height.set(
            (((metrics.ascent() + metrics.descent()) as f64 / pango::SCALE as f64).ceil() as i64)
                .max(1),
        );
        self.imp().layouts.borrow_mut().clear();
        let (width, height) = (
            self.allocated_width() as i64,
            self.allocated_height() as i64,
        );
        if width >= self.char_width() * 2 && height >= self.char_height() {
            self.set_size(width / self.char_width(), height / self.char_height());
        }
        self.queue_resize();
        self.queue_draw();
    }
    fn restart_blink(&self) {
        let old = self.imp().blink_source.borrow_mut().take();
        if let Some(old) = old {
            old.remove();
        }
        self.imp().cursor_visible.set(true);
        let enabled = match self.cursor_blink_mode() {
            CursorBlinkMode::Off => false,
            CursorBlinkMode::On => true,
            CursorBlinkMode::System => {
                gtk::Settings::default().is_some_and(|s| s.is_gtk_cursor_blink())
            }
        };
        let visible = self
            .imp()
            .screen
            .borrow()
            .mode()
            .contains(TermMode::SHOW_CURSOR);
        if self.imp().focused.get()
            && self.is_mapped()
            && enabled
            && visible
            && self.input_enabled()
        {
            let weak = self.downgrade();
            let id = glib::timeout_add_local(Duration::from_millis(600), move || {
                if let Some(t) = weak.upgrade() {
                    t.imp().cursor_visible.set(!t.imp().cursor_visible.get());
                    t.queue_draw();
                    glib::ControlFlow::Continue
                } else {
                    glib::ControlFlow::Break
                }
            });
            *self.imp().blink_source.borrow_mut() = Some(id);
        }
        self.queue_draw();
    }
    /// Longest output-consumption callback, for the Windows runner's responsiveness test.
    pub fn maximum_output_slice(&self) -> Duration {
        self.imp().max_slice.get()
    }
    fn glyph_layout(&self, text: &str, flags: Flags) -> pango::Layout {
        let style = flags & (Flags::BOLD | Flags::ITALIC);
        let cached = self
            .imp()
            .layouts
            .borrow()
            .get(&style.bits())
            .and_then(|map| map.get(text))
            .cloned();
        if let Some(layout) = cached {
            return layout;
        }
        let mut font = self.scaled_font();
        if style.contains(Flags::BOLD) {
            font.set_weight(pango::Weight::Bold);
        }
        if style.contains(Flags::ITALIC) {
            font.set_style(pango::Style::Italic);
        }
        let layout = self.create_pango_layout(Some(text));
        layout.set_font_description(Some(&font));
        {
            let mut cache = self.imp().layouts.borrow_mut();
            if cache.values().map(|m| m.len()).sum::<usize>() >= 4096 {
                cache.clear();
            }
            cache
                .entry(style.bits())
                .or_default()
                .insert(text.to_owned(), layout.clone());
        }
        layout
    }
    fn paint(&self, cr: &gtk::cairo::Context) {
        let start = Instant::now();
        // Snapshot before entering Pango/GTK: those calls can re-enter signal handlers.
        let (cells, offset, selection, cursor, mode, overrides) = {
            let screen = self.imp().screen.borrow();
            let content = screen.term.renderable_content();
            let overrides = (0..269)
                .map(|i| screen.term.colors()[i])
                .collect::<Vec<_>>();
            (
                content
                    .display_iter
                    .map(|c| (c.point, c.cell.clone()))
                    .collect::<Vec<_>>(),
                content.display_offset,
                content.selection,
                content.cursor,
                *screen.term.mode(),
                overrides,
            )
        };
        let colors = self.imp().colors.borrow().clone();
        color(cr, &colors.background);
        let _ = cr.paint();
        let cw = self.char_width() as f64;
        let ch = self.char_height() as f64;
        let hover_points = self.imp().hover_points.borrow().clone();
        let cursor_shape = match cursor.shape {
            alacritty_terminal::vte::ansi::CursorShape::Beam => CursorShape::Ibeam,
            alacritty_terminal::vte::ansi::CursorShape::Underline => CursorShape::Underline,
            _ => CursorShape::Block,
        };
        let (x1, y1, x2, y2) = cr.clip_extents().unwrap_or((
            0.,
            0.,
            self.allocated_width() as f64,
            self.allocated_height() as f64,
        ));
        let mut glyph = String::with_capacity(16);
        for (point, cell) in cells {
            let x = point.column.0 as f64 * cw;
            let y = (point.line.0 + offset as i32) as f64 * ch;
            if x + cw < x1 || x > x2 || y + ch < y1 || y > y2 {
                continue;
            }
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let width = if cell.flags.contains(Flags::WIDE_CHAR) {
                cw * 2.
            } else {
                cw
            };
            let resolve = |c, bright| {
                let index = match c {
                    emu::Color::Named(n) => Some(n as usize),
                    emu::Color::Indexed(i) => Some(i as usize),
                    _ => None,
                };
                index
                    .and_then(|i| overrides.get(i).copied().flatten())
                    .map(|rgb| {
                        gdk::RGBA::new(
                            rgb.r as f64 / 255.,
                            rgb.g as f64 / 255.,
                            rgb.b as f64 / 255.,
                            1.,
                        )
                    })
                    .unwrap_or_else(|| colors.resolve(c, bright))
            };
            let mut fg = resolve(
                cell.fg,
                self.bold_is_bright() && cell.flags.contains(Flags::BOLD),
            );
            let mut bg = resolve(cell.bg, false);
            if cell.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            let selected = selection.is_some_and(|s| s.contains(point));
            if selected {
                fg = colors.highlight_foreground;
                bg = colors.highlight;
            }
            let cursor_here = point == cursor.point
                && mode.contains(TermMode::SHOW_CURSOR)
                && self.imp().cursor_visible.get()
                && self.has_focus()
                && offset == 0;
            if cursor_here && cursor_shape == CursorShape::Block {
                fg = colors.cursor_foreground;
                bg = colors.cursor;
            }
            if bg != colors.background {
                color(cr, &bg);
                cr.rectangle(x, y, width, ch);
                let _ = cr.fill();
            }
            if !cell.flags.contains(Flags::HIDDEN) && cell.c != ' ' {
                glyph.clear();
                glyph.push(cell.c);
                if let Some(zero) = cell.zerowidth() {
                    glyph.extend(zero);
                }
                if cell.flags.contains(Flags::DIM) {
                    fg = gdk::RGBA::new(fg.red(), fg.green(), fg.blue(), fg.alpha() * 0.66);
                }
                color(cr, &fg);
                cr.move_to(x, y);
                let layout = self.glyph_layout(&glyph, cell.flags);
                pangocairo::functions::show_layout(cr, &layout);
            }
            let linked = hover_points.binary_search(&point).is_ok();
            color(cr, &fg);
            cr.set_line_width(1.);
            if cell.flags.intersects(Flags::ALL_UNDERLINES) || linked {
                cr.move_to(x, y + ch - 2.);
                cr.line_to(x + width, y + ch - 2.);
                let _ = cr.stroke();
            }
            if cell.flags.contains(Flags::STRIKEOUT) {
                cr.move_to(x, y + ch * 0.55);
                cr.line_to(x + width, y + ch * 0.55);
                let _ = cr.stroke();
            }
            if cursor_here && cursor_shape != CursorShape::Block {
                color(cr, &colors.cursor);
                if cursor_shape == CursorShape::Ibeam {
                    cr.rectangle(x, y, 2., ch);
                } else {
                    cr.rectangle(x, y + ch - 2., width, 2.);
                }
                let _ = cr.fill();
            }
        }
        let (preedit, _, _) = self.imp().im.preedit_string();
        let x = cursor.point.column.0 as f64 * cw;
        let y = cursor.point.line.0 as f64 * ch;
        self.imp().im.set_cursor_location(&gdk::Rectangle::new(
            x as i32, y as i32, cw as i32, ch as i32,
        ));
        if !preedit.is_empty() {
            color(cr, &colors.foreground);
            cr.move_to(x, y);
            pangocairo::functions::show_layout(cr, &self.glyph_layout(&preedit, Flags::UNDERLINE));
        }
        self.imp()
            .max_slice
            .set(self.imp().max_slice.get().max(start.elapsed()));
    }
}
