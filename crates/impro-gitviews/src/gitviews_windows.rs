//! WebView2 child controller hosted by a native GTK3 DrawingArea on the GTK thread.
use crate::{protocol, windows_diag as diag, windows_policy as policy};
use glib::{subclass::prelude::*, translate::*};
use gtk::{prelude::*, subclass::prelude::*};
use serde_json::Value;
use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::{Rc, Weak},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use webview2_com::{
    AcceleratorKeyPressedEventHandler, AddScriptToExecuteOnDocumentCreatedCompletedHandler,
    CoTaskMemPWSTR, CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, DownloadStartingEventHandler,
    ExecuteScriptCompletedHandler, FocusChangedEventHandler, Microsoft::Web::WebView2::Win32::*,
    MoveFocusRequestedEventHandler, NavigationCompletedEventHandler,
    NavigationStartingEventHandler, NewWindowRequestedEventHandler,
    PermissionRequestedEventHandler, ProcessFailedEventHandler, WebMessageReceivedEventHandler,
    WebResourceRequestedEventHandler,
};
use windows::{
    core::{Interface, PCWSTR, PWSTR},
    Win32::{
        Foundation::{HWND, RECT},
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
        UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT},
    },
};

static HOSTS: AtomicUsize = AtomicUsize::new(0);

type Message = Rc<dyn Fn(Value)>;
type Error = Rc<dyn Fn(&str)>;
type Reload = Rc<dyn Fn()>;
type Removal = Box<dyn FnOnce()>;

// GDK3 exports this function on Windows, but gdk-sys has no Win32 backend module.
#[link(name = "gdk-3")]
extern "C" {
    fn gdk_win32_window_get_handle(window: *mut gdk::ffi::GdkWindow) -> *mut std::ffi::c_void;
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
thread_local! {
    static APARTMENT: OnceCell<Result<Apartment, String>> = const { OnceCell::new() };
}
fn ensure_apartment() -> Result<(), String> {
    APARTMENT.with(|slot| {
        slot.get_or_init(|| {
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
                .ok()
                .map(|_| Apartment)
                .map_err(|e| format!("WebView2 requires the GTK STA thread: {e}"))
        })
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
    })
}

mod surface {
    use super::*;
    #[derive(Default)]
    pub struct NativeSurface {
        pub(super) state: RefCell<Weak<State>>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for NativeSurface {
        const NAME: &'static str = "ImproGitWebView2Surface";
        type Type = super::NativeSurface;
        type ParentType = gtk::DrawingArea;
    }
    impl ObjectImpl for NativeSurface {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_has_window(true);
            obj.set_can_focus(true);
            obj.set_hexpand(true);
            obj.set_vexpand(true);
        }
    }
    impl WidgetImpl for NativeSurface {
        fn realize(&self) {
            self.parent_realize();
            let state = self.state.borrow().upgrade();
            if let Some(state) = state {
                state.begin();
            }
        }
        fn unrealize(&self) {
            let state = self.state.borrow().upgrade();
            if let Some(state) = state {
                state.detach();
                if !state.closed.get() && !state.failed.get() {
                    let reload = state.reload.borrow().clone();
                    if let Some(reload) = reload {
                        reload();
                    }
                }
            }
            self.parent_unrealize();
        }
        fn size_allocate(&self, allocation: &gtk::Allocation) {
            self.parent_size_allocate(allocation);
            let state = self.state.borrow().upgrade();
            if let Some(state) = state {
                state.resize();
            }
        }
        fn map(&self) {
            self.parent_map();
            let state = self.state.borrow().upgrade();
            if let Some(state) = state {
                state.visible(true);
            }
        }
        fn unmap(&self) {
            let state = self.state.borrow().upgrade();
            if let Some(state) = state {
                state.visible(false);
            }
            self.parent_unmap();
        }
        fn destroy(&self) {
            let state = self.state.borrow().upgrade();
            if let Some(state) = state {
                state.shutdown();
            }
            self.parent_destroy();
        }
    }
    impl DrawingAreaImpl for NativeSurface {}
}
glib::wrapper! {
    pub struct NativeSurface(ObjectSubclass<surface::NativeSurface>)
        @extends gtk::DrawingArea, gtk::Widget,
        @implements gtk::Buildable;
}

struct Session {
    controller: ICoreWebView2Controller,
    web: ICoreWebView2,
    removals: RefCell<Vec<Removal>>,
    closed: Cell<bool>,
}
impl Session {
    fn close(&self) {
        if self.closed.replace(true) {
            return;
        }
        let removals = std::mem::take(&mut *self.removals.borrow_mut());
        for remove in removals {
            remove();
        }
        unsafe {
            let _ = self.controller.Close();
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}

struct State {
    container: glib::WeakRef<gtk::Box>,
    surface: NativeSurface,
    session: RefCell<Option<Rc<Session>>>,
    environment: RefCell<Option<ICoreWebView2Environment>>,
    html: RefCell<String>,
    background: Cell<COREWEBVIEW2_COLOR>,
    message: RefCell<Option<Message>>,
    error: RefCell<Option<Error>>,
    reload: RefCell<Option<Reload>>,
    focused: RefCell<Option<Reload>>,
    epoch: Cell<u64>,
    starting: Cell<bool>,
    closed: Cell<bool>,
    failed: Cell<bool>,
    navigations: RefCell<policy::Navigations>,
    diag: RefCell<diag::Diagnostics>,
    page_ready: Cell<bool>,
    syncing_focus: Cell<bool>,
    leaving_focus: Cell<bool>,
    timeout: RefCell<Option<glib::SourceId>>,
    notice: RefCell<Option<String>>,
}

/// Windows inspection handle. The public page hosts retain their GTK widget API.
#[derive(Clone)]
pub struct WebView(Rc<State>);
impl WebView {
    pub(crate) fn new(container: &gtk::Box) -> Self {
        let surface: NativeSurface = glib::Object::new();
        container.pack_start(&surface, true, true, 0);
        let state = Rc::new(State {
            container: container.downgrade(),
            surface,
            session: RefCell::new(None),
            environment: RefCell::new(None),
            html: RefCell::new(String::new()),
            background: Cell::new(COREWEBVIEW2_COLOR {
                A: 255,
                R: 24,
                G: 24,
                B: 24,
            }),
            message: RefCell::new(None),
            error: RefCell::new(None),
            reload: RefCell::new(None),
            focused: RefCell::new(None),
            epoch: Cell::new(0),
            starting: Cell::new(false),
            closed: Cell::new(false),
            failed: Cell::new(false),
            navigations: RefCell::new(policy::Navigations::default()),
            diag: RefCell::new(diag::Diagnostics::new(
                HOSTS.fetch_add(1, Ordering::Relaxed) + 1,
                diag::echo_enabled(
                    std::env::var("IMPRO_GIT_DEBUG").ok().as_deref(),
                    std::env::var("IMPRO_GIT_DIAG").ok().as_deref(),
                ),
            )),
            page_ready: Cell::new(false),
            syncing_focus: Cell::new(false),
            leaving_focus: Cell::new(false),
            timeout: RefCell::new(None),
            notice: RefCell::new(None),
        });
        *state.surface.imp().state.borrow_mut() = Rc::downgrade(&state);
        let weak = Rc::downgrade(&state);
        state.surface.connect_draw(move |_, context| {
            if let Some(s) = weak.upgrade() {
                let color = s.background.get();
                context.set_source_rgb(
                    f64::from(color.R) / 255.,
                    f64::from(color.G) / 255.,
                    f64::from(color.B) / 255.,
                );
                let _ = context.paint();
            }
            glib::Propagation::Proceed
        });
        let weak = Rc::downgrade(&state);
        state.surface.connect_focus_in_event(move |_, _| {
            if let Some(s) = weak.upgrade() {
                if !s.syncing_focus.get() && !s.leaving_focus.get() {
                    s.move_focus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
                }
            }
            glib::Propagation::Proceed
        });
        let weak = Rc::downgrade(&state);
        state.surface.connect_focus(move |widget, direction| {
            let Some(s) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if s.leaving_focus.get() || widget.has_focus() {
                return glib::Propagation::Proceed;
            }
            s.syncing_focus.set(true);
            widget.grab_focus();
            s.syncing_focus.set(false);
            s.move_focus(if direction == gtk::DirectionType::TabBackward {
                COREWEBVIEW2_MOVE_FOCUS_REASON_PREVIOUS
            } else {
                COREWEBVIEW2_MOVE_FOCUS_REASON_NEXT
            });
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(&state);
        state.surface.connect_scale_factor_notify(move |_| {
            if let Some(s) = weak.upgrade() {
                s.resize();
            }
        });
        Self(state)
    }
    pub(crate) fn connect_message(&self, callback: impl Fn(Value) + 'static) {
        *self.0.message.borrow_mut() = Some(Rc::new(callback));
    }
    pub(crate) fn connect_error(&self, callback: impl Fn(&str) + 'static) {
        *self.0.error.borrow_mut() = Some(Rc::new(callback));
    }
    pub(crate) fn connect_reload(&self, callback: impl Fn() + 'static) {
        *self.0.reload.borrow_mut() = Some(Rc::new(callback));
    }
    pub(crate) fn connect_focused(&self, callback: impl Fn() + 'static) {
        *self.0.focused.borrow_mut() = Some(Rc::new(callback));
    }
    pub(crate) fn load_html(&self, html: &str, _: Option<&str>) {
        *self.0.html.borrow_mut() = html.into();
        // Creation starts from realize, after the native parent HWND exists.
        if self.0.surface.is_realized() {
            self.0.begin();
        }
    }
    pub(crate) fn show(&self) {
        self.0.surface.show();
    }
    pub fn grab_focus(&self) {
        self.0.syncing_focus.set(true);
        self.0.surface.grab_focus();
        self.0.syncing_focus.set(false);
        self.0
            .move_focus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
    }
    pub(crate) fn set_background_color(&self, color: &gdk::RGBA) {
        let color = COREWEBVIEW2_COLOR {
            A: 255,
            R: (color.red() * 255.).round() as u8,
            G: (color.green() * 255.).round() as u8,
            B: (color.blue() * 255.).round() as u8,
        };
        self.0.background.set(color);
        self.0.surface.queue_draw();
        if let Some(session) = self.0.session() {
            unsafe {
                if let Ok(controller) = session.controller.cast::<ICoreWebView2Controller2>() {
                    let _ = controller.SetDefaultBackgroundColor(color);
                }
            }
        }
    }
    /// Asynchronously execute JavaScript and return WebView2's JSON-encoded result.
    /// No result is delivered after shutdown, unrealize, or controller replacement.
    pub fn evaluate_script(
        &self,
        script: &str,
        callback: impl FnOnce(Result<Value, String>) + 'static,
    ) {
        let Some(session) = self.0.session() else {
            return;
        };
        let epoch = self.0.epoch.get();
        let weak = Rc::downgrade(&self.0);
        let completion = ExecuteScriptCompletedHandler::create(Box::new(move |result, json| {
            if let Some(s) = weak.upgrade().filter(|s| s.current(epoch)) {
                let _keep_alive = s;
                callback(
                    result
                        .map_err(|e| e.to_string())
                        .map(|_| protocol::parse_message(&json).unwrap_or(Value::Null)),
                );
            }
            Ok(())
        }));
        let script = wide(script);
        if let Err(e) = unsafe {
            session
                .web
                .ExecuteScript(PCWSTR(script.as_ptr()), &completion)
        } {
            let _ = unsafe { completion.Invoke(e.code(), PCWSTR::null()) };
        }
    }
    /// Fallback reason for runner diagnostics, or `None` while the page is available.
    pub fn fallback_reason(&self) -> Option<String> {
        self.0.notice.borrow().clone()
    }
    /// Timeline of the page load (phase timings, every HRESULT, navigation decisions).
    /// Printed to stderr as it happens when `IMPRO_GIT_DEBUG=1` or `IMPRO_GIT_DIAG=1`.
    #[doc(hidden)]
    pub fn diagnostics(&self) -> String {
        self.0.diag.borrow().render()
    }
    pub(crate) fn shutdown(&self) {
        self.0.shutdown();
    }
}
/// `"<call> failed: <message>, HRESULT 0x........"`, the one error shape used in notices.
fn failure(call: &str, error: &windows::core::Error) -> String {
    format!(
        "{call} failed: {}, HRESULT {}",
        error.message().trim(),
        diag::hresult(error.code().0)
    )
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

impl State {
    fn session(&self) -> Option<Rc<Session>> {
        self.session.borrow().clone()
    }
    fn current(&self, epoch: u64) -> bool {
        !self.closed.get() && !self.failed.get() && self.epoch.get() == epoch
    }
    fn report(&self, text: &str) {
        let callback = self.error.borrow().clone();
        if !self.closed.get() {
            if let Some(callback) = callback {
                callback(text);
            }
        }
    }
    fn note(&self, text: &str) {
        self.diag.borrow_mut().note(text);
    }
    fn mark(&self, phase: diag::Phase, text: &str) {
        self.diag.borrow_mut().mark(phase, text);
    }
    /// Record a failed call in the timeline and return its notice text.
    fn failed(&self, call: &str, error: &windows::core::Error) -> String {
        let text = failure(call, error);
        self.diag.borrow_mut().fail(&text);
        text
    }
    /// Log a failing call on the way out of `?`.
    fn step<T>(&self, call: &str, result: windows::core::Result<T>) -> windows::core::Result<T> {
        result.inspect_err(|error| {
            self.failed(call, error);
        })
    }
    fn cancel_timeout(&self) {
        let timeout = self.timeout.borrow_mut().take();
        if let Some(timeout) = timeout {
            timeout.remove();
        }
    }
    fn detach(&self) {
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.cancel_timeout();
        self.starting.set(false);
        self.page_ready.set(false);
        *self.navigations.borrow_mut() = policy::Navigations::default();
        let session = self.session.borrow_mut().take();
        if let Some(session) = session {
            session.close();
        }
        self.environment.borrow_mut().take();
    }
    fn shutdown(&self) {
        if self.closed.replace(true) {
            return;
        }
        self.detach();
        self.message.borrow_mut().take();
        self.error.borrow_mut().take();
        self.reload.borrow_mut().take();
        self.focused.borrow_mut().take();
        unsafe {
            self.surface.destroy();
        }
    }
    /// Show the notice. `code` is the concrete cause (one short line, may be empty).
    fn fallback(self: &Rc<Self>, text: &str, code: &str) {
        if self.closed.get() || self.failed.replace(true) {
            return;
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.cancel_timeout();
        let notice = if code.is_empty() {
            text.to_owned()
        } else {
            format!("{text}\n[{code}]")
        };
        {
            let mut diag = self.diag.borrow_mut();
            diag.note(&format!("FALLBACK notice: {notice}"));
            let summary = diag.summary();
            diag.note(&format!("phases: {summary}"));
        }
        *self.notice.borrow_mut() = Some(notice);
        // Never destroy a native parent from inside a synchronous WebView2 callback.
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            let Some(s) = weak.upgrade().filter(|s| !s.closed.get()) else {
                return;
            };
            s.detach();
            s.surface.hide();
            if let Some(container) = s.container.upgrade() {
                let label = gtk::Label::new(None);
                let reason = s.notice.borrow().clone().unwrap_or_default();
                label.set_markup(&format!(
                    "Git pages require Microsoft Edge WebView2 (Evergreen runtime).\n{}\n<a href=\"{}\">Download Microsoft Edge WebView2</a>",
                    glib::markup_escape_text(&reason), policy::RUNTIME_DOWNLOAD
                ));
                label.set_halign(gtk::Align::Center);
                label.set_valign(gtk::Align::Center);
                label.set_hexpand(true);
                label.set_vexpand(true);
                label.set_line_wrap(true);
                label.set_selectable(true);
                label.style_context().add_class("dim-label");
                container.pack_start(&label, true, true, 0);
                label.show();
            }
            let reason = s
                .notice
                .borrow()
                .clone()
                .unwrap_or_else(|| "WebView2 failed".into());
            s.report(&format!("Git pages unavailable: {reason}"));
        });
    }
    fn begin(self: &Rc<Self>) {
        if self.closed.get()
            || self.failed.get()
            || self.starting.get()
            || self.session().is_some()
            || self.html.borrow().is_empty()
        {
            return;
        }
        *self.navigations.borrow_mut() = policy::Navigations::default();
        if policy::forced_notice(std::env::var("IMPRO_GIT_PAGES").ok().as_deref()) {
            self.fallback("Disabled by IMPRO_GIT_PAGES=notice.", "");
            return;
        }
        self.note("begin: creating the WebView2 environment");
        let result = self.create_environment();
        if let Err(error) = result {
            self.diag.borrow_mut().fail(&error);
            self.fallback(&error, "");
        }
    }
    fn create_environment(self: &Rc<Self>) -> Result<(), String> {
        ensure_apartment()?;
        let window = self
            .surface
            .window()
            .ok_or("GTK native window is unavailable")?;
        if !window.ensure_native() {
            return Err("GTK could not create a native child window".into());
        }
        let hwnd = HWND(unsafe { gdk_win32_window_get_handle(window.to_glib_none().0) });
        if hwnd.0.is_null() {
            return Err("GTK did not provide a Win32 window handle".into());
        }
        let local = std::env::var_os("LOCALAPPDATA");
        let folder = policy::user_data_folder(local.as_deref())?;
        // Environment creation creates its own directory asynchronously.
        let folder = wide(&folder.to_string_lossy());
        let mut version = PWSTR::null();
        let detection =
            unsafe { GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut version) };
        let version = CoTaskMemPWSTR::from(version);
        detection.map_err(|e| {
            failure(
                "Evergreen runtime detection (GetAvailableCoreWebView2BrowserVersionString)",
                &e,
            )
        })?;
        self.note(&format!("Evergreen runtime version {version}"));
        let options = CoreWebView2EnvironmentOptions::default();
        unsafe {
            options.set_additional_browser_arguments(policy::BROWSER_ARGUMENTS.into());
        }
        let options: ICoreWebView2EnvironmentOptions = options.into();
        self.starting.set(true);
        let epoch = self.epoch.get();
        let weak = Rc::downgrade(self);
        let timer = glib::timeout_add_local_once(Duration::from_secs(60), move || {
            if let Some(s) = weak.upgrade().filter(|s| s.current(epoch)) {
                s.timeout.borrow_mut().take();
                let phase = s.diag.borrow().last_phase().map_or("none", |p| p.label());
                s.note(&format!(
                    "TIMER: the 60 s creation/ready timer fired; last phase reached: {phase}"
                ));
                s.fallback(
                    "WebView2 did not finish creating the page within 60 seconds.",
                    &format!("60 s timer fired; last phase reached: {phase}"),
                );
            }
        });
        *self.timeout.borrow_mut() = Some(timer);
        let weak = Rc::downgrade(self);
        let completion = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
            move |result, environment| {
                let Some(s) = weak.upgrade().filter(|s| s.current(epoch)) else {
                    return Ok(());
                };
                let result =
                    result.and_then(|_| environment.ok_or_else(windows::core::Error::from_win32));
                match result {
                    Ok(environment) => {
                        s.mark(diag::Phase::Environment, "environment created");
                        *s.environment.borrow_mut() = Some(environment.clone());
                        let weak = Rc::downgrade(&s);
                        let completion = CreateCoreWebView2ControllerCompletedHandler::create(
                            Box::new(move |result, controller| {
                                let Some(s) = weak.upgrade().filter(|s| s.current(epoch)) else {
                                    if let Some(controller) = controller {
                                        let _ = unsafe { controller.Close() };
                                    }
                                    return Ok(());
                                };
                                match result.and_then(|_| {
                                    controller.ok_or_else(windows::core::Error::from_win32)
                                }) {
                                    Ok(controller) => {
                                        s.mark(diag::Phase::Controller, "controller created");
                                        if let Err(error) = s.install(controller) {
                                            let code = s.diag.borrow().failure().map_or_else(
                                                || failure("setup", &error),
                                                str::to_owned,
                                            );
                                            s.fallback("WebView2 setup failed.", &code);
                                        }
                                    }
                                    Err(error) => {
                                        let code = s.failed(
                                            "CreateCoreWebView2Controller completion",
                                            &error,
                                        );
                                        s.fallback("WebView2 controller creation failed.", &code);
                                    }
                                }
                                Ok(())
                            }),
                        );
                        if let Err(error) =
                            unsafe { environment.CreateCoreWebView2Controller(hwnd, &completion) }
                        {
                            let code = s.failed("CreateCoreWebView2Controller", &error);
                            s.fallback("WebView2 controller request failed.", &code);
                        }
                    }
                    Err(error) => {
                        let code = s.failed("CreateCoreWebView2Environment completion", &error);
                        s.fallback("WebView2 environment creation failed.", &code);
                    }
                }
                Ok(())
            },
        ));
        unsafe {
            CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                PCWSTR(folder.as_ptr()),
                &options,
                &completion,
            )
        }
        .map_err(|e| failure("CreateCoreWebView2EnvironmentWithOptions", &e))
    }
    fn install(self: &Rc<Self>, controller: ICoreWebView2Controller) -> windows::core::Result<()> {
        let web = match self.step("CoreWebView2Controller.CoreWebView2", unsafe {
            controller.CoreWebView2()
        }) {
            Ok(web) => web,
            Err(error) => {
                let _ = unsafe { controller.Close() };
                return Err(error);
            }
        };
        let session = Rc::new(Session {
            controller,
            web,
            removals: RefCell::new(Vec::new()),
            closed: Cell::new(false),
        });
        *self.session.borrow_mut() = Some(session.clone());
        self.starting.set(false);
        let epoch = self.epoch.get();
        // Each handler owns only a Weak<State>. Registrations are removed before Close.
        // A failing call inside a handler is logged, never returned to the runtime.
        macro_rules! event {
            ($object:expr, $add:ident, $remove:ident, $handler:ident, |$s:ident, $args:ident| $body:block) => {{
                let weak = Rc::downgrade(self);
                let handler = $handler::create(Box::new(move |_, args| {
                    let Some($s) = weak.upgrade().filter(|s| s.current(epoch)) else {
                        return Ok(());
                    };
                    if let Some($args) = args {
                        let outcome = (|| -> windows::core::Result<()> {
                            $body
                            Ok(())
                        })();
                        if let Err(error) = outcome {
                            $s.diag
                                .borrow_mut()
                                .fail(&failure(concat!(stringify!($handler), " body"), &error));
                        }
                    }
                    Ok(())
                }));
                let object = $object.clone();
                let mut token = 0;
                unsafe {
                    self.step(
                        stringify!($add),
                        object.$add(&handler, &mut token),
                    )?;
                }
                session.removals.borrow_mut().push(Box::new(move || {
                    let _ = unsafe { object.$remove(token) };
                }));
            }};
        }
        let prefs = policy::Settings::new(std::env::var("IMPRO_GIT_DEVTOOLS").ok().as_deref());
        unsafe {
            macro_rules! call {
                ($name:literal, $expr:expr) => {
                    self.step($name, $expr)?
                };
            }
            let settings = call!("Settings", session.web.Settings());
            call!(
                "SetAreDevToolsEnabled",
                settings.SetAreDevToolsEnabled(prefs.devtools)
            );
            call!(
                "SetAreDefaultContextMenusEnabled",
                settings.SetAreDefaultContextMenusEnabled(prefs.context_menus)
            );
            call!(
                "SetIsStatusBarEnabled",
                settings.SetIsStatusBarEnabled(prefs.status_bar)
            );
            call!(
                "SetIsZoomControlEnabled",
                settings.SetIsZoomControlEnabled(prefs.zoom)
            );
            call!(
                "SetIsBuiltInErrorPageEnabled",
                settings.SetIsBuiltInErrorPageEnabled(prefs.error_page)
            );
            call!(
                "SetIsWebMessageEnabled",
                settings.SetIsWebMessageEnabled(true)
            );
            call!(
                "SetAreHostObjectsAllowed",
                settings.SetAreHostObjectsAllowed(false)
            );
            call!(
                "SetAreDefaultScriptDialogsEnabled",
                settings.SetAreDefaultScriptDialogsEnabled(false)
            );
            call!(
                "SetAreBrowserAcceleratorKeysEnabled",
                call!("Settings3", settings.cast::<ICoreWebView2Settings3>())
                    .SetAreBrowserAcceleratorKeysEnabled(prefs.browser_accelerators)
            );
            let settings4 = call!("Settings4", settings.cast::<ICoreWebView2Settings4>());
            call!(
                "SetIsPasswordAutosaveEnabled",
                settings4.SetIsPasswordAutosaveEnabled(prefs.passwords)
            );
            call!(
                "SetIsGeneralAutofillEnabled",
                settings4.SetIsGeneralAutofillEnabled(prefs.autofill)
            );
            call!(
                "SetIsPinchZoomEnabled",
                call!("Settings5", settings.cast::<ICoreWebView2Settings5>())
                    .SetIsPinchZoomEnabled(prefs.zoom)
            );
            call!(
                "SetIsSwipeNavigationEnabled",
                call!("Settings6", settings.cast::<ICoreWebView2Settings6>())
                    .SetIsSwipeNavigationEnabled(prefs.swipe)
            );
            call!(
                "SetIsMuted",
                call!("WebView2_8", session.web.cast::<ICoreWebView2_8>()).SetIsMuted(true)
            );
            call!(
                "SetDefaultBackgroundColor",
                call!(
                    "Controller2",
                    session.controller.cast::<ICoreWebView2Controller2>()
                )
                .SetDefaultBackgroundColor(self.background.get())
            );
            // Bounds are physical pixels; Chromium follows monitor DPI for CSS pixels.
            let controller3 = call!(
                "Controller3",
                session.controller.cast::<ICoreWebView2Controller3>()
            );
            call!(
                "SetBoundsMode",
                controller3.SetBoundsMode(COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS)
            );
            call!(
                "SetShouldDetectMonitorScaleChanges",
                controller3.SetShouldDetectMonitorScaleChanges(true)
            );
        }
        event!(
            session.web,
            add_WebMessageReceived,
            remove_WebMessageReceived,
            WebMessageReceivedEventHandler,
            |s, args| {
                let mut raw = PWSTR::null();
                unsafe {
                    args.WebMessageAsJson(&mut raw)?;
                }
                let raw = CoTaskMemPWSTR::from(raw);
                if let Some(value) = protocol::parse_message(&raw.to_string()) {
                    if value["type"] == "ready" {
                        s.page_ready.set(true);
                        s.navigations.borrow_mut().disarm();
                        s.cancel_timeout();
                        s.mark(diag::Phase::Ready, "page sent its ready message");
                    } else if value["type"] == "error" {
                        let message = value["message"].as_str().unwrap_or("?");
                        s.note(&format!("page error: {}", diag::clip(message, 200)));
                    }
                    let callback = s.message.borrow().clone();
                    if let Some(callback) = callback {
                        callback(value);
                    }
                } else {
                    s.note("web message that is not JSON was dropped");
                }
            }
        );
        event!(
            session.web,
            add_NavigationStarting,
            remove_NavigationStarting,
            NavigationStartingEventHandler,
            |s, args| {
                s.navigation_starting(&args, false)?;
            }
        );
        event!(
            session.web,
            add_FrameNavigationStarting,
            remove_FrameNavigationStarting,
            NavigationStartingEventHandler,
            |s, args| {
                s.navigation_starting(&args, true)?;
            }
        );
        event!(
            session.web,
            add_NewWindowRequested,
            remove_NewWindowRequested,
            NewWindowRequestedEventHandler,
            |s, args| {
                unsafe {
                    args.SetHandled(true)?;
                }
                let mut uri = PWSTR::null();
                if unsafe { args.Uri(&mut uri) }.is_ok() {
                    let uri = CoTaskMemPWSTR::from(uri).to_string();
                    s.note(&format!("new window refused: {}", diag::describe_uri(&uri)));
                }
            }
        );
        event!(
            session.web,
            add_PermissionRequested,
            remove_PermissionRequested,
            PermissionRequestedEventHandler,
            |s, args| {
                unsafe {
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
                }
                let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                if unsafe { args.PermissionKind(&mut kind) }.is_ok() {
                    s.note(&format!("permission denied: kind {}", kind.0));
                }
            }
        );
        event!(
            session.web,
            add_NavigationCompleted,
            remove_NavigationCompleted,
            NavigationCompletedEventHandler,
            |s, args| {
                let mut success = windows::core::BOOL::default();
                let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                let mut id = 0u64;
                unsafe {
                    args.IsSuccess(&mut success)?;
                    args.WebErrorStatus(&mut status)?;
                    args.NavigationId(&mut id)?;
                }
                s.navigation_completed(id, success.as_bool(), status.0);
            }
        );
        event!(
            session.web,
            add_ProcessFailed,
            remove_ProcessFailed,
            ProcessFailedEventHandler,
            |s, args| {
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED;
                unsafe {
                    args.ProcessFailedKind(&mut kind)?;
                }
                s.note(&format!(
                    "ProcessFailed kind {} ({})",
                    kind.0,
                    diag::process_failed_name(kind.0)
                ));
                s.report(&format!(
                    "Git web process stopped: {} ({}); recreating the page.",
                    diag::process_failed_name(kind.0),
                    kind.0
                ));
                let weak = Rc::downgrade(&s);
                glib::idle_add_local_once(move || {
                    if let Some(s) = weak.upgrade().filter(|s| s.current(epoch)) {
                        s.detach();
                        let reload = s.reload.borrow().clone();
                        if let Some(reload) = reload {
                            reload();
                        }
                        s.begin();
                    }
                });
            }
        );
        // Embedded documents use inline resources only. Deny every external request,
        // including frames, DNS-producing image URLs, downloads, media, and scripts.
        unsafe {
            session.web.AddWebResourceRequestedFilter(
                windows::core::w!("*"),
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
            )?;
        }
        event!(
            session.web,
            add_WebResourceRequested,
            remove_WebResourceRequested,
            WebResourceRequestedEventHandler,
            |s, args| {
                let request = unsafe { args.Request()? };
                let mut uri = PWSTR::null();
                unsafe {
                    request.Uri(&mut uri)?;
                }
                let uri = CoTaskMemPWSTR::from(uri).to_string();
                // The embedded document itself arrives here as a data: request and
                // must pass untouched; only requests that could leave the process are refused.
                if policy::blocks_resource(&uri) {
                    let environment = s.environment.borrow().clone();
                    if let Some(environment) = environment {
                        let response = unsafe {
                            environment.CreateWebResourceResponse(
                                None,
                                403,
                                windows::core::w!("Blocked"),
                                windows::core::w!("Content-Type: text/plain"),
                            )?
                        };
                        unsafe {
                            args.SetResponse(&response)?;
                        }
                        s.note(&format!(
                            "resource blocked (403): {}",
                            diag::describe_uri(&uri)
                        ));
                    } else {
                        s.note(&format!(
                            "resource NOT blocked (no environment): {}",
                            diag::describe_uri(&uri)
                        ));
                    }
                } else {
                    s.note(&format!("resource passed: {}", diag::describe_uri(&uri)));
                }
            }
        );
        let downloads = session.web.cast::<ICoreWebView2_4>()?;
        event!(
            downloads,
            add_DownloadStarting,
            remove_DownloadStarting,
            DownloadStartingEventHandler,
            |s, args| {
                unsafe {
                    args.SetCancel(true)?;
                    args.SetHandled(true)?;
                }
                s.note("download cancelled");
            }
        );
        event!(
            session.controller,
            add_AcceleratorKeyPressed,
            remove_AcceleratorKeyPressed,
            AcceleratorKeyPressedEventHandler,
            |s, args| {
                s.accelerator(&args)?;
            }
        );
        // GotFocus/LostFocus carry an IUnknown argument, which may be null, so
        // register these separately rather than requiring an event-args object.
        let weak = Rc::downgrade(self);
        let focus = FocusChangedEventHandler::create(Box::new(move |_, _| {
            if let Some(s) = weak.upgrade().filter(|s| s.current(epoch)) {
                s.syncing_focus.set(true);
                s.surface.grab_focus();
                s.syncing_focus.set(false);
                let focused = s.focused.borrow().clone();
                if let Some(focused) = focused {
                    focused();
                }
            }
            Ok(())
        }));
        let mut token = 0;
        unsafe {
            session.controller.add_GotFocus(&focus, &mut token)?;
        }
        let controller = session.controller.clone();
        session.removals.borrow_mut().push(Box::new(move || {
            let _ = unsafe { controller.remove_GotFocus(token) };
        }));
        let weak = Rc::downgrade(self);
        let lost = FocusChangedEventHandler::create(Box::new(move |_, _| {
            if let Some(s) = weak.upgrade().filter(|s| s.current(epoch)) {
                s.syncing_focus.set(false);
            }
            Ok(())
        }));
        let mut token = 0;
        unsafe {
            session.controller.add_LostFocus(&lost, &mut token)?;
        }
        let controller = session.controller.clone();
        session.removals.borrow_mut().push(Box::new(move || {
            let _ = unsafe { controller.remove_LostFocus(token) };
        }));
        event!(
            session.controller,
            add_MoveFocusRequested,
            remove_MoveFocusRequested,
            MoveFocusRequestedEventHandler,
            |s, args| {
                let mut reason = COREWEBVIEW2_MOVE_FOCUS_REASON_NEXT;
                unsafe {
                    args.Reason(&mut reason)?;
                    args.SetHandled(true)?;
                }
                let weak = Rc::downgrade(&s);
                glib::idle_add_local_once(move || {
                    if let Some(s) = weak.upgrade().filter(|s| s.current(epoch)) {
                        s.leaving_focus.set(true);
                        if let Some(window) = s.surface.toplevel() {
                            window.child_focus(
                                if reason == COREWEBVIEW2_MOVE_FOCUS_REASON_PREVIOUS {
                                    gtk::DirectionType::TabBackward
                                } else {
                                    gtk::DirectionType::TabForward
                                },
                            );
                        }
                        s.leaving_focus.set(false);
                    }
                });
            }
        );
        self.resize();
        self.visible(self.surface.is_mapped());
        self.note("controller configured; registering the document-created shim");
        let weak = Rc::downgrade(self);
        let completion = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(
            move |result, _| {
                if let Some(s) = weak.upgrade().filter(|s| s.current(epoch)) {
                    match result {
                        Ok(()) => {
                            s.mark(diag::Phase::Shim, "document-created shim registered");
                            let session = s.session();
                            let (html, bytes) = {
                                let html = s.html.borrow();
                                (wide(&html), html.len())
                            };
                            // The runtime navigates to a data: URI for NavigateToString (the
                            // page origin is about:blank). Arm exactly that one navigation.
                            s.navigations.borrow_mut().arm();
                            s.note(&format!(
                                "NavigateToString: document is {bytes} bytes (runtime limit 2 MiB); navigation armed"
                            ));
                            if let Some(session) = session {
                                match unsafe { session.web.NavigateToString(PCWSTR(html.as_ptr())) }
                                {
                                    Ok(()) => s.note("NavigateToString returned S_OK"),
                                    Err(error) => {
                                        let code = s.failed("NavigateToString", &error);
                                        s.fallback("WebView2 document load failed.", &code);
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            let code =
                                s.failed("AddScriptToExecuteOnDocumentCreated completion", &error);
                            s.fallback("WebView2 bridge injection failed.", &code);
                        }
                    }
                }
                Ok(())
            },
        ));
        let bridge = wide(policy::BRIDGE);
        self.step("AddScriptToExecuteOnDocumentCreated", unsafe {
            session
                .web
                .AddScriptToExecuteOnDocumentCreated(PCWSTR(bridge.as_ptr()), &completion)
        })
    }
    /// Cancel everything except the one armed document navigation (fail closed: any
    /// unreadable argument cancels).
    fn navigation_starting(
        self: &Rc<Self>,
        args: &ICoreWebView2NavigationStartingEventArgs,
        frame: bool,
    ) -> windows::core::Result<()> {
        let kind = if frame {
            "FrameNavigationStarting"
        } else {
            "NavigationStarting"
        };
        let read = || -> windows::core::Result<(String, u64, bool, bool)> {
            let mut raw = PWSTR::null();
            unsafe { args.Uri(&mut raw)? };
            let uri = CoTaskMemPWSTR::from(raw).to_string();
            let mut id = 0u64;
            let mut user = windows::core::BOOL::default();
            let mut redirected = windows::core::BOOL::default();
            unsafe {
                args.NavigationId(&mut id)?;
                args.IsUserInitiated(&mut user)?;
                args.IsRedirected(&mut redirected)?;
            }
            Ok((uri, id, user.as_bool(), redirected.as_bool()))
        };
        let (cancel, line, document) = match read() {
            Err(error) => (
                true,
                format!(
                    "{kind}: arguments unreadable, cancelled; {}",
                    failure("read", &error)
                ),
                false,
            ),
            Ok((uri, id, user, redirected)) => {
                let verdict = if frame {
                    policy::NavigationVerdict::Cancel("frame navigation")
                } else {
                    self.navigations.borrow_mut().starting(id, &uri, redirected)
                };
                let (cancel, outcome) = match verdict {
                    policy::NavigationVerdict::Document => {
                        (false, "ALLOWED (the armed document)".into())
                    }
                    policy::NavigationVerdict::Blank => (false, "allowed (blank page)".into()),
                    policy::NavigationVerdict::Cancel(why) => (true, format!("CANCELLED: {why}")),
                };
                (
                    cancel,
                    format!(
                        "{kind} id={id} uri={} user_initiated={user} redirected={redirected} -> {outcome}",
                        diag::describe_uri(&uri)
                    ),
                    verdict == policy::NavigationVerdict::Document,
                )
            }
        };
        let applied = self.step(&format!("{kind}.SetCancel({cancel})"), unsafe {
            args.SetCancel(cancel)
        });
        if document {
            self.mark(diag::Phase::NavigationStarted, &line);
        } else {
            self.note(&line);
        }
        applied
    }
    fn navigation_completed(self: &Rc<Self>, id: u64, success: bool, status: i32) {
        let completion = self
            .navigations
            .borrow()
            .completed(id, success, status == 14);
        let name = diag::web_error_name(status);
        let line = format!(
            "NavigationCompleted id={id} IsSuccess={success} WebErrorStatus={status} ({name}) -> {completion:?}"
        );
        match completion {
            policy::Completion::DocumentLoaded => {
                self.mark(diag::Phase::NavigationCompleted, &line)
            }
            policy::Completion::DocumentFailed | policy::Completion::UnknownFailure => {
                self.note(&line);
                if !self.page_ready.get() {
                    let document = self.navigations.borrow().document();
                    self.fallback(
                        "WebView2 could not load the embedded Git page.",
                        &format!(
                            "NavigationCompleted IsSuccess=false WebErrorStatus={status} ({name}) navigation_id={id} document_id={}",
                            document.map_or("none".into(), |d| d.to_string())
                        ),
                    );
                }
            }
            policy::Completion::PolicyCancelled | policy::Completion::Unrelated => self.note(&line),
        }
    }
    fn resize(&self) {
        if let Some(session) = self.session() {
            let (width, height) = policy::physical_size(
                self.surface.allocated_width(),
                self.surface.allocated_height(),
                self.surface.scale_factor(),
            );
            unsafe {
                let _ = session.controller.SetBounds(RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                });
                let _ = session.controller.NotifyParentWindowPositionChanged();
            }
        }
    }
    fn visible(&self, visible: bool) {
        if let Some(session) = self.session() {
            let _ = unsafe { session.controller.SetIsVisible(visible) };
        }
    }
    fn move_focus(&self, reason: COREWEBVIEW2_MOVE_FOCUS_REASON) {
        if let Some(session) = self.session() {
            let _ = unsafe { session.controller.MoveFocus(reason) };
        }
    }
    fn accelerator(
        self: &Rc<Self>,
        args: &ICoreWebView2AcceleratorKeyPressedEventArgs,
    ) -> windows::core::Result<()> {
        let mut key = 0;
        let mut kind = COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN;
        unsafe {
            args.VirtualKey(&mut key)?;
            args.KeyEventKind(&mut kind)?;
        }
        let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
        let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
        let alt = unsafe { GetKeyState(VK_MENU.0 as i32) } < 0;
        let Some(name) = policy::shortcut(key, ctrl, shift, alt) else {
            return Ok(());
        };
        unsafe {
            args.SetHandled(true)?;
        }
        if ![
            COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN,
            COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
        ]
        .contains(&kind)
        {
            return Ok(());
        }
        // Posting through GLib avoids running GTK/window commands inside WebView2's
        // synchronous keyboard callback (where cross-process COM calls are forbidden).
        let weak = Rc::downgrade(self);
        let epoch = self.epoch.get();
        glib::idle_add_local_once(move || {
            let Some(s) = weak.upgrade().filter(|s| s.current(epoch)) else {
                return;
            };
            let Some(window) = s
                .surface
                .toplevel()
                .and_then(|w| w.downcast::<gtk::Window>().ok())
            else {
                return;
            };
            let Some(native) = window.window() else {
                return;
            };
            let mut event = gdk::Event::new(gdk::EventType::KeyPress);
            if let Some(seat) = window.display().default_seat() {
                event.set_device(seat.keyboard().as_ref());
            }
            let mut modifiers = gdk::ModifierType::empty();
            if ctrl {
                modifiers |= gdk::ModifierType::CONTROL_MASK;
            }
            if shift {
                modifiers |= gdk::ModifierType::SHIFT_MASK;
            }
            if alt {
                modifiers |= gdk::ModifierType::MOD1_MASK;
            }
            // gdk-rs exposes getters but no setters for GdkEventKey. The event owns
            // this referenced GdkWindow until gdk_event_free releases it.
            unsafe {
                let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
                let raw = &mut *(raw as *mut gdk::ffi::GdkEventKey);
                raw.window = native.to_glib_full();
                raw.send_event = 1;
                raw.time = gdk::ffi::GDK_CURRENT_TIME as u32;
                raw.state = modifiers.into_glib();
                raw.keyval = gdk::keys::Key::from_name(name).into_glib();
                raw.hardware_keycode = key as u16;
            }
            let handled = window.event(&event);
            if !handled && name == "Escape" {
                WebView(s).evaluate_script("(document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',code:'Escape',bubbles:true}));", |_| {});
            }
        });
        Ok(())
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.shutdown();
    }
}
