//! WebView2 child controller hosted by a native GTK3 DrawingArea on the GTK thread.
use crate::{protocol, windows_policy as policy};
use glib::{subclass::prelude::*, translate::*};
use gtk::{prelude::*, subclass::prelude::*};
use serde_json::Value;
use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::{Rc, Weak},
    time::Duration,
};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::{
    core::{Interface, PCWSTR, PWSTR},
    Win32::{
        Foundation::{HWND, RECT},
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
        UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT},
    },
};

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
    struct NativeSurface(ObjectSubclass<surface::NativeSurface>)
        @extends gtk::DrawingArea, gtk::Widget,
        @implements gtk::Buildable, gtk::atk::ImplementorIface;
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
    epoch: Cell<u64>,
    starting: Cell<bool>,
    closed: Cell<bool>,
    failed: Cell<bool>,
    navigation_allowed: Cell<bool>,
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
            epoch: Cell::new(0),
            starting: Cell::new(false),
            closed: Cell::new(false),
            failed: Cell::new(false),
            navigation_allowed: Cell::new(false),
            page_ready: Cell::new(false),
            syncing_focus: Cell::new(false),
            leaving_focus: Cell::new(false),
            timeout: RefCell::new(None),
            notice: RefCell::new(None),
        });
        *state.surface.imp().state.borrow_mut() = Rc::downgrade(&state);
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
        self.0.surface.grab_focus();
    }
    pub(crate) fn set_background_color(&self, color: &gdk::RGBA) {
        let color = COREWEBVIEW2_COLOR {
            A: 255,
            R: (color.red() * 255.).round() as u8,
            G: (color.green() * 255.).round() as u8,
            B: (color.blue() * 255.).round() as u8,
        };
        self.0.background.set(color);
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
            self.0.report(&format!("WebView2 ExecuteScript: {e}"));
        }
    }
    /// Fallback reason for runner diagnostics, or `None` while the page is available.
    pub fn fallback_reason(&self) -> Option<String> {
        self.0.notice.borrow().clone()
    }
    pub(crate) fn shutdown(&self) {
        self.0.shutdown();
    }
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
    }
    fn fallback(self: &Rc<Self>, text: &str) {
        if self.closed.get() || self.failed.replace(true) {
            return;
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.cancel_timeout();
        *self.notice.borrow_mut() = Some(text.into());
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
                label.set_text(&format!(
                    "Git pages require Microsoft Edge WebView2 (Evergreen runtime).\n{reason}\n{}",
                    policy::RUNTIME_DOWNLOAD
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
        if policy::forced_notice(std::env::var("IMPRO_GIT_PAGES").ok().as_deref()) {
            self.fallback("Disabled by IMPRO_GIT_PAGES=notice.");
            return;
        }
        let result = self.create_environment();
        if let Err(error) = result {
            self.fallback(&error);
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
        let _version = CoTaskMemPWSTR::from(version);
        detection.map_err(|e| format!("Evergreen runtime was not found: {e}"))?;
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
                s.fallback("WebView2 did not finish creating the page within 60 seconds.");
            }
        });
        *self.timeout.borrow_mut() = Some(timer);
        let weak = Rc::downgrade(self);
        let completion = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
            move |result, environment| {
                let Some(s) = weak.upgrade().filter(|s| s.current(epoch)) else {
                    return Ok(());
                };
                let result = result
                    .and_then(|_| environment.ok_or_else(|| windows::core::Error::from_win32()));
                match result {
                    Ok(environment) => {
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
                                        if let Err(error) = s.install(controller) {
                                            s.fallback(&format!("WebView2 setup: {error}"));
                                        }
                                    }
                                    Err(error) => s.fallback(&format!(
                                        "WebView2 controller creation: {error}"
                                    )),
                                }
                                Ok(())
                            }),
                        );
                        if let Err(error) =
                            unsafe { environment.CreateCoreWebView2Controller(hwnd, &completion) }
                        {
                            s.fallback(&format!("WebView2 controller request: {error}"));
                        }
                    }
                    Err(error) => s.fallback(&format!("WebView2 environment creation: {error}")),
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
        .map_err(|e| format!("WebView2 environment request: {e}"))
    }
    fn install(self: &Rc<Self>, controller: ICoreWebView2Controller) -> windows::core::Result<()> {
        let web = match unsafe { controller.CoreWebView2() } {
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
        macro_rules! event {
            ($object:expr, $add:ident, $remove:ident, $handler:ident, |$s:ident, $args:ident| $body:block) => {{
                let weak = Rc::downgrade(self);
                let handler = $handler::create(Box::new(move |_, args| {
                    let Some($s) = weak.upgrade().filter(|s| s.current(epoch)) else {
                        return Ok(());
                    };
                    if let Some($args) = args {
                        $body
                    }
                    Ok(())
                }));
                let object = $object.clone();
                let mut token = 0;
                unsafe {
                    object.$add(&handler, &mut token)?;
                }
                session.removals.borrow_mut().push(Box::new(move || {
                    let _ = unsafe { object.$remove(token) };
                }));
            }};
        }
        let prefs = policy::Settings::new(std::env::var("IMPRO_GIT_DEVTOOLS").ok().as_deref());
        unsafe {
            let settings = session.web.Settings()?;
            settings.SetAreDevToolsEnabled(prefs.devtools)?;
            settings.SetAreDefaultContextMenusEnabled(prefs.context_menus)?;
            settings.SetIsStatusBarEnabled(prefs.status_bar)?;
            settings.SetIsZoomControlEnabled(prefs.zoom)?;
            settings.SetIsBuiltInErrorPageEnabled(prefs.error_page)?;
            settings.SetIsWebMessageEnabled(true)?;
            settings.SetAreHostObjectsAllowed(false)?;
            settings
                .cast::<ICoreWebView2Settings3>()?
                .SetAreBrowserAcceleratorKeysEnabled(prefs.browser_accelerators)?;
            let settings4 = settings.cast::<ICoreWebView2Settings4>()?;
            settings4.SetIsPasswordAutosaveEnabled(prefs.passwords)?;
            settings4.SetIsGeneralAutofillEnabled(prefs.autofill)?;
            settings
                .cast::<ICoreWebView2Settings5>()?
                .SetIsPinchZoomEnabled(prefs.zoom)?;
            settings
                .cast::<ICoreWebView2Settings6>()?
                .SetIsSwipeNavigationEnabled(prefs.swipe)?;
            session.web.cast::<ICoreWebView2_8>()?.SetIsMuted(true)?;
            session
                .controller
                .cast::<ICoreWebView2Controller2>()?
                .SetDefaultBackgroundColor(self.background.get())?;
            // Bounds are physical pixels; Chromium follows monitor DPI for CSS pixels.
            session
                .controller
                .cast::<ICoreWebView2Controller3>()?
                .SetBoundsMode(COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS)?;
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
                        s.cancel_timeout();
                    }
                    let callback = s.message.borrow().clone();
                    if let Some(callback) = callback {
                        callback(value);
                    }
                }
            }
        );
        event!(
            session.web,
            add_NavigationStarting,
            remove_NavigationStarting,
            NavigationStartingEventHandler,
            |s, args| {
                let mut uri = PWSTR::null();
                unsafe {
                    args.Uri(&mut uri)?;
                }
                let uri = CoTaskMemPWSTR::from(uri);
                let allowed =
                    s.navigation_allowed.replace(false) && uri.to_string() == "about:blank";
                unsafe {
                    args.SetCancel(!allowed)?;
                }
            }
        );
        event!(
            session.web,
            add_NewWindowRequested,
            remove_NewWindowRequested,
            NewWindowRequestedEventHandler,
            |_s, args| {
                unsafe {
                    args.SetHandled(true)?;
                }
            }
        );
        event!(
            session.web,
            add_PermissionRequested,
            remove_PermissionRequested,
            PermissionRequestedEventHandler,
            |_s, args| {
                unsafe {
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
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
                unsafe {
                    args.IsSuccess(&mut success)?;
                }
                if !success.as_bool() && !s.page_ready.get() {
                    s.fallback("WebView2 could not load the embedded Git page.");
                }
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
                s.report(&format!(
                    "Git web process stopped: {kind:?}; recreating the page."
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
                }
            }
        );
        let downloads = session.web.cast::<ICoreWebView2_4>()?;
        event!(
            downloads,
            add_DownloadStarting,
            remove_DownloadStarting,
            DownloadStartingEventHandler,
            |_s, args| {
                unsafe {
                    args.SetCancel(true)?;
                    args.SetHandled(true)?;
                }
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
        let weak = Rc::downgrade(self);
        let completion = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(
            move |result, _| {
                if let Some(s) = weak.upgrade().filter(|s| s.current(epoch)) {
                    match result {
                        Ok(()) => {
                            let session = s.session();
                            let html = wide(&s.html.borrow());
                            s.navigation_allowed.set(true);
                            if let Some(session) = session {
                                if let Err(error) =
                                    unsafe { session.web.NavigateToString(PCWSTR(html.as_ptr())) }
                                {
                                    s.fallback(&format!("WebView2 document load: {error}"));
                                }
                            }
                        }
                        Err(error) => s.fallback(&format!("WebView2 bridge injection: {error}")),
                    }
                }
                Ok(())
            },
        ));
        let bridge = wide(policy::BRIDGE);
        unsafe {
            session
                .web
                .AddScriptToExecuteOnDocumentCreated(PCWSTR(bridge.as_ptr()), &completion)
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
                raw.time = gdk::CURRENT_TIME;
                raw.state = modifiers.into_glib();
                raw.keyval = gdk::keys::Key::from_name(name).into_glib();
                raw.hardware_keycode = key as u16;
            }
            window.event(&event);
        });
        Ok(())
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.shutdown();
    }
}
