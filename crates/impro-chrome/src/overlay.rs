//! Transparent prompt catcher and top-right, bounded notice stack.
use crate::{prompt::SHADOW_MARGINS, prompt_geometry, Prompt};
use glib::translate::*;
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
pub const MAX_NOTICES: usize = 5;
pub const NOTICE_PADDING: i32 = 10;
pub const NOTICE_GAP: i32 = 14;
pub const NOTICE_MAX_WIDTH: i32 = 300;
pub const NOTICE_SLIDE: f64 = 350.;
pub const NOTICE_SHADOW: (i32, i32, i32, i32) = (0, 8, 12, 8);
pub const NOTICE_LEVELS: [&str; 4] = ["info", "success", "warning", "error"];
struct NoticeInner {
    widget: gtk::EventBox,
    box_: gtk::Box,
    label: gtk::Label,
    level: String,
    offset: Cell<f64>,
    collapse: Cell<f64>,
    leaving: Cell<bool>,
    timer: RefCell<Option<glib::SourceId>>,
    timeout: Cell<u32>,
}
#[derive(Clone)]
pub struct Notice(Rc<NoticeInner>);
impl std::ops::Deref for Notice {
    type Target = gtk::EventBox;
    fn deref(&self) -> &Self::Target {
        &self.0.widget
    }
}
impl Notice {
    pub fn new(text: &str, level: &str) -> Self {
        let widget = gtk::EventBox::new();
        widget.set_visible_window(false);
        let bx = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        bx.style_context().add_class("impro-notice");
        bx.style_context()
            .add_class(&format!("impro-notice-{level}"));
        let (top, right, bottom, left) = NOTICE_SHADOW;
        bx.set_margin_top(top);
        bx.set_margin_end(right);
        bx.set_margin_bottom(bottom);
        bx.set_margin_start(left);
        let label = gtk::Label::new(Some(text));
        label.set_xalign(0.);
        label.set_line_wrap(true);
        label.set_line_wrap_mode(pango::WrapMode::WordChar);
        bx.add(&label);
        widget.add(&bx);
        widget.add_events(gdk::EventMask::ENTER_NOTIFY_MASK | gdk::EventMask::LEAVE_NOTIFY_MASK);
        Self(Rc::new(NoticeInner {
            widget,
            box_: bx,
            label,
            level: level.into(),
            offset: Cell::new(0.),
            collapse: Cell::new(0.),
            leaving: Cell::new(false),
            timer: RefCell::new(None),
            timeout: Cell::new(0),
        }))
    }
    pub fn widget(&self) -> &gtk::EventBox {
        &self.0.widget
    }
    pub fn box_widget(&self) -> gtk::Box {
        self.0.box_.clone()
    }
    pub fn label(&self) -> gtk::Label {
        self.0.label.clone()
    }
    pub fn text(&self) -> String {
        self.0.label.text().into()
    }
    pub fn level(&self) -> &str {
        &self.0.level
    }
    pub fn offset(&self) -> f64 {
        self.0.offset.get()
    }
    pub fn collapse(&self) -> f64 {
        self.0.collapse.get()
    }
    pub fn leaving(&self) -> bool {
        self.0.leaving.get()
    }
    pub fn timer_active(&self) -> bool {
        self.0.timer.borrow().is_some()
    }
    pub fn stop_timer(&self) {
        let id = self.0.timer.borrow_mut().take();
        if let Some(id) = id {
            id.remove();
        }
    }
    fn size(&self) -> (i32, i32) {
        let bx = &self.0.box_;
        let (min, natural) = bx.preferred_width();
        let w = min.max(natural.min(NOTICE_MAX_WIDTH + 16)) - 16;
        let h = bx.preferred_height_for_width(w + 16).1 - 12;
        (w, h)
    }
}
impl Drop for NoticeInner {
    fn drop(&mut self) {
        if let Some(id) = self.timer.get_mut().take() {
            id.remove();
        }
    }
}
type PromptCallback = Rc<dyn Fn(&Prompt)>;
struct Inner {
    widget: gtk::Overlay,
    catcher: gtk::EventBox,
    prompt: RefCell<Option<Prompt>>,
    previous: RefCell<Option<glib::WeakRef<gtk::Widget>>>,
    notices: RefCell<Vec<Notice>>,
    shown: RefCell<Vec<PromptCallback>>,
    hidden: RefCell<Vec<PromptCallback>>,
}
#[derive(Clone)]
pub struct OverlayHost(Rc<Inner>);
impl std::ops::Deref for OverlayHost {
    type Target = gtk::Overlay;
    fn deref(&self) -> &Self::Target {
        &self.0.widget
    }
}
impl OverlayHost {
    pub fn new(child: Option<&gtk::Widget>) -> Self {
        let widget = gtk::Overlay::new();
        widget.style_context().add_class("impro-overlay-host");
        if let Some(child) = child {
            widget.add(child);
        }
        let catcher = gtk::EventBox::new();
        catcher.set_visible_window(false);
        catcher.style_context().add_class("impro-overlay-catcher");
        let this = Self(Rc::new(Inner {
            widget,
            catcher,
            prompt: RefCell::new(None),
            previous: RefCell::new(None),
            notices: RefCell::new(vec![]),
            shown: RefCell::new(vec![]),
            hidden: RefCell::new(vec![]),
        }));
        let weak = Rc::downgrade(&this.0);
        this.0.catcher.connect_button_press_event(move |_, _| {
            if let Some(inner) = weak.upgrade() {
                OverlayHost(inner).hide_prompt();
            }
            glib::Propagation::Stop
        });
        this.0
            .catcher
            .connect_scroll_event(|_, _| glib::Propagation::Stop);
        let weak = Rc::downgrade(&this.0);
        connect_position(&this.0.widget, move |child, rect| {
            weak.upgrade()
                .is_some_and(|inner| OverlayHost(inner).position(child, rect))
        });
        let weak = Rc::downgrade(&this.0);
        this.0.widget.connect_destroy(move |_| {
            if let Some(inner) = weak.upgrade() {
                let host = OverlayHost(inner);
                host.hide_prompt();
                host.clear_notices();
            }
        });
        this
    }
    pub fn widget(&self) -> &gtk::Overlay {
        &self.0.widget
    }
    pub fn prompt(&self) -> Option<Prompt> {
        self.0.prompt.borrow().clone()
    }
    pub fn show_prompt(&self, prompt: &Prompt) {
        if self.prompt().is_some_and(|p| p.widget() == prompt.widget()) {
            prompt.focus_input();
            return;
        }
        if self.prompt().is_some() {
            self.dismiss_prompt(false);
        } else {
            let focus = self
                .toplevel()
                .and_then(|w| w.downcast::<gtk::Window>().ok())
                .and_then(|w| GtkWindowExt::focused_widget(&w));
            *self.0.previous.borrow_mut() = focus.map(|w| w.downgrade());
        }
        *self.0.prompt.borrow_mut() = Some(prompt.clone());
        if self.0.catcher.parent().is_none() {
            self.add_overlay(&self.0.catcher);
        }
        self.0.catcher.show();
        self.add_overlay(prompt.widget());
        for n in self.all_notices() {
            self.reorder_overlay(n.widget(), -1);
        }
        let weak = Rc::downgrade(&self.0);
        let p = prompt.downgrade();
        prompt.set_host_closed(move || {
            if let (Some(inner), Some(p)) = (weak.upgrade(), p.upgrade()) {
                let host = OverlayHost(inner);
                if host
                    .prompt()
                    .is_some_and(|current| current.widget() == p.widget())
                {
                    host.dismiss_prompt(true);
                }
            }
        });
        prompt.prepare();
        prompt.show();
        prompt.focus_input();
        let callbacks = self.0.shown.borrow().clone();
        for f in callbacks {
            f(prompt);
        }
    }
    pub fn hide_prompt(&self) {
        if let Some(p) = self.prompt() {
            p.close();
        }
    }
    fn dismiss_prompt(&self, restore: bool) {
        let p = self.0.prompt.borrow_mut().take();
        let Some(p) = p else {
            return;
        };
        p.close();
        if p.parent().as_ref() == Some(self.widget().upcast_ref()) {
            self.remove(p.widget());
        }
        if restore {
            if self.0.catcher.parent().is_some() {
                self.remove(&self.0.catcher);
            }
            let previous = self
                .0
                .previous
                .borrow_mut()
                .take()
                .and_then(|w| w.upgrade());
            if let Some(w) = previous {
                if w.toplevel() == self.toplevel() && w.is_mapped() && w.is_sensitive() {
                    if let Some(e) = w.downcast_ref::<gtk::Entry>() {
                        e.grab_focus_without_selecting();
                    } else {
                        w.grab_focus();
                    }
                }
            }
        }
        let callbacks = self.0.hidden.borrow().clone();
        for f in callbacks {
            f(&p);
        }
    }
    pub fn connect_prompt_shown(&self, callback: impl Fn(&Prompt) + 'static) {
        self.0.shown.borrow_mut().push(Rc::new(callback));
    }
    pub fn connect_prompt_hidden(&self, callback: impl Fn(&Prompt) + 'static) {
        self.0.hidden.borrow_mut().push(Rc::new(callback));
    }
    fn all_notices(&self) -> Vec<Notice> {
        self.0.notices.borrow().clone()
    }
    pub fn notices(&self) -> Vec<Notice> {
        self.all_notices()
            .into_iter()
            .filter(|n| !n.leaving())
            .collect()
    }
    /// Show a notice with the shared default hold duration.
    pub fn notify_default(&self, text: &str, level: &str) -> Result<Notice, String> {
        self.notify(
            text,
            impro_ui::motion::DURATIONS["notice_autohide"] as i32,
            level,
        )
    }
    /// Show a notice; zero timeout stays until dismissed. Invalid inputs are errors.
    pub fn notify(&self, text: &str, timeout: i32, level: &str) -> Result<Notice, String> {
        if !NOTICE_LEVELS.contains(&level) {
            return Err(format!("unknown notice level {}", quoted(level)));
        }
        if timeout < 0 {
            return Err("timeout must be nonnegative".into());
        }
        let notice = Notice::new(text, level);
        notice.0.timeout.set(timeout as u32);
        let weak = Rc::downgrade(&self.0);
        let n = Rc::downgrade(&notice.0);
        notice.connect_button_press_event(move |_, _| {
            if let (Some(inner), Some(n)) = (weak.upgrade(), n.upgrade()) {
                OverlayHost(inner).dismiss(&Notice(n));
            }
            glib::Propagation::Stop
        });
        let n = Rc::downgrade(&notice.0);
        notice.connect_enter_notify_event(move |_, e| {
            if e.detail() != gdk::NotifyType::Inferior {
                if let Some(n) = n.upgrade() {
                    Notice(n).stop_timer();
                }
            }
            glib::Propagation::Proceed
        });
        let weak = Rc::downgrade(&self.0);
        let n = Rc::downgrade(&notice.0);
        notice.connect_leave_notify_event(move |_, e| {
            if e.detail() != gdk::NotifyType::Inferior {
                if let (Some(inner), Some(n)) = (weak.upgrade(), n.upgrade()) {
                    let timeout = n.timeout.get();
                    if timeout > 0 {
                        OverlayHost(inner).start_timer(
                            &Notice(n),
                            timeout.min(impro_ui::motion::DURATIONS["notice_hover_grace"]),
                        );
                    }
                }
            }
            glib::Propagation::Proceed
        });
        self.0.notices.borrow_mut().push(notice.clone());
        self.add_overlay(notice.widget());
        notice.show_all();
        let visible = self.notices();
        for old in visible
            .iter()
            .take(visible.len().saturating_sub(MAX_NOTICES))
        {
            self.dismiss(old);
        }
        self.start_timer(&notice, timeout as u32);
        notice.0.offset.set(NOTICE_SLIDE);
        let weak = Rc::downgrade(&self.0);
        let n = Rc::downgrade(&notice.0);
        impro_ui::motion::animate(
            notice.widget(),
            "notice_enter",
            move |p| {
                if let (Some(inner), Some(n)) = (weak.upgrade(), n.upgrade()) {
                    n.offset.set(NOTICE_SLIDE * (1. - p));
                    inner.widget.queue_allocate();
                }
            },
            impro_ui::motion::AnimateOptions {
                easing: impro_ui::motion::easing("ease_in_out").expect("built-in curve"),
                key: impro_ui::motion::MotionKey::unique(),
                ..Default::default()
            },
        );
        Ok(notice)
    }
    pub fn dismiss(&self, notice: &Notice) {
        if notice.leaving()
            || !self
                .all_notices()
                .iter()
                .any(|n| Rc::ptr_eq(&n.0, &notice.0))
        {
            return;
        }
        notice.0.leaving.set(true);
        notice.stop_timer();
        let weak = Rc::downgrade(&self.0);
        let done = weak.clone();
        let n = Rc::downgrade(&notice.0);
        let done_n = n.clone();
        impro_ui::motion::animate(
            notice.widget(),
            "notice_exit",
            move |p| {
                if let (Some(inner), Some(n)) = (weak.upgrade(), n.upgrade()) {
                    n.collapse.set(p);
                    n.widget.set_opacity((1. - p).max(0.));
                    inner.widget.queue_allocate();
                }
            },
            impro_ui::motion::AnimateOptions {
                easing: impro_ui::motion::easing("ease_in_out").expect("built-in curve"),
                key: impro_ui::motion::MotionKey::unique(),
                on_done: Some(Box::new(move || {
                    if let (Some(inner), Some(n)) = (done.upgrade(), done_n.upgrade()) {
                        OverlayHost(inner).remove_notice(&Notice(n));
                    }
                })),
                ..Default::default()
            },
        );
    }
    fn remove_notice(&self, notice: &Notice) {
        notice.stop_timer();
        self.0
            .notices
            .borrow_mut()
            .retain(|n| !Rc::ptr_eq(&n.0, &notice.0));
        if notice.parent().is_some() {
            self.remove(notice.widget());
        }
        self.queue_allocate();
    }
    pub fn clear_notices(&self) {
        for n in self.all_notices() {
            self.remove_notice(&n);
        }
    }
    fn start_timer(&self, notice: &Notice, delay: u32) {
        notice.stop_timer();
        if delay == 0 || notice.leaving() {
            return;
        }
        let weak = Rc::downgrade(&self.0);
        let n = Rc::downgrade(&notice.0);
        let id = glib::timeout_add_local_once(Duration::from_millis(delay as u64), move || {
            if let (Some(inner), Some(n)) = (weak.upgrade(), n.upgrade()) {
                n.timer.borrow_mut().take();
                OverlayHost(inner).dismiss(&Notice(n));
            }
        });
        *notice.0.timer.borrow_mut() = Some(id);
    }
    fn position(&self, child: &gtk::Widget, rect: &mut gdk::Rectangle) -> bool {
        let (width, height) = (self.allocated_width(), self.allocated_height());
        if child == self.0.catcher.upcast_ref::<gtk::Widget>() {
            *rect = gdk::Rectangle::new(0, 0, width, height);
            return true;
        }
        if self
            .prompt()
            .is_some_and(|p| p.widget().upcast_ref::<gtk::Widget>() == child)
        {
            let (x, y, w, max_h) = prompt_geometry(width, height);
            let (top, right, bottom, left) = SHADOW_MARGINS;
            let (min, natural) = child.preferred_height_for_width(w + left + right);
            let h = (min - top - bottom).max((natural - top - bottom).min(max_h));
            *rect = gdk::Rectangle::new(x - left, y - top, w + left + right, h + top + bottom);
            return true;
        }
        let mut y = NOTICE_PADDING as f64;
        let mut index = 0;
        loop {
            let n = self.0.notices.borrow().get(index).cloned();
            let Some(n) = n else {
                break;
            };
            index += 1;
            let (w, h) = n.size();
            if n.widget().upcast_ref::<gtk::Widget>() == child {
                *rect = gdk::Rectangle::new(
                    (width - NOTICE_PADDING - w - 8) + (n.offset().round() as i32),
                    y.round() as i32,
                    w + 16,
                    h + 12,
                );
                return true;
            }
            y += (h + NOTICE_GAP) as f64 * (1. - n.collapse());
        }
        false
    }
}
// gtk-rs does not expose this signal because its rectangle argument is mutable.
// GTK owns both pointers for the duration of the callback; no pointer escapes.
fn connect_position(
    overlay: &gtk::Overlay,
    callback: impl Fn(&gtk::Widget, &mut gdk::Rectangle) -> bool + 'static,
) {
    unsafe extern "C" fn trampoline<F: Fn(&gtk::Widget, &mut gdk::Rectangle) -> bool>(
        _: *mut gtk::ffi::GtkOverlay,
        child: *mut gtk::ffi::GtkWidget,
        rect: *mut gdk::ffi::GdkRectangle,
        data: glib::ffi::gpointer,
    ) -> glib::ffi::gboolean {
        let f = &*(data as *const F);
        let child: Borrowed<gtk::Widget> = from_glib_borrow(child);
        let mut value: gdk::Rectangle = from_glib_none(rect);
        let result = f(&child, &mut value);
        *rect = *value.to_glib_none().0;
        result.into_glib()
    }
    unsafe fn connect<F: Fn(&gtk::Widget, &mut gdk::Rectangle) -> bool + 'static>(
        overlay: &gtk::Overlay,
        f: F,
    ) {
        glib::signal::connect_raw(
            overlay.as_ptr() as *mut _,
            c"get-child-position".as_ptr(),
            Some(std::mem::transmute::<*const (), unsafe extern "C" fn()>(
                trampoline::<F> as *const (),
            )),
            Box::into_raw(Box::new(f)),
        );
    }
    unsafe {
        connect(overlay, callback);
    }
}
pub fn css(ui: &impro_core::themes::UiColors) -> String {
    let mut ui = ui.clone();
    ui.insert(
        "shadow".into(),
        if ui["overlay"] == "#ffffff" {
            "0 1px 2px alpha(#000000, 0.121), 0 3.4px 6.7px alpha(#000000, 0.179)"
        } else {
            "0 1px 2px alpha(#000000, 0.028), 0 3.4px 6.7px alpha(#000000, 0.042)"
        }
        .into(),
    );
    impro_ui::render_css(include_str!("overlay.css"), &ui)
}

fn quoted(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::new();
    out.push(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}
