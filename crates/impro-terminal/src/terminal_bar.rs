//! Compact pane chrome, chooser and informational limit notice.
use glib::translate::*;
use gtk::prelude::*;
use impro_core::themes::{self, UiColors};
use impro_ui::motion::{self, AnimateOptions, MotionKey};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

pub const GLYPH_OPACITY: f64 = 0.85;
pub const SHOW_MS: u64 = 3840;
pub const ENTER_MS: u32 = 224;
pub const FADE_MS: u32 = 128;

pub fn css(ui: &UiColors) -> String {
    let mut values = ui.clone();
    values.insert(
        "header_hover".into(),
        format!("alpha({}, 0.067)", ui["overlay"]),
    );
    values.insert(
        "notice_fg".into(),
        themes::legible(&ui["warning"], "#1a1a1a", 4.5),
    );
    values.insert("menu_radius".into(), "11".into());
    values.insert("item_radius".into(), "5".into());
    values.insert("compact".into(), "6".into());
    impro_ui::render_css(include_str!("terminal_bar.css"), &values)
}

/// Cancelable linear opacity animation; unmapping settles at its target.
#[derive(Clone)]
pub struct Fade(Rc<FadeInner>);
struct FadeInner {
    widget: glib::WeakRef<gtk::Widget>,
    tween: RefCell<Option<motion::Tween>>,
    unmap: RefCell<Option<glib::SignalHandlerId>>,
    done: RefCell<Option<Box<dyn FnOnce()>>>,
    finished: Cell<bool>,
    target: f64,
}
impl FadeInner {
    fn settle(&self, complete: bool) {
        if self.finished.replace(true) {
            return;
        }
        let handler = self.unmap.borrow_mut().take();
        if let Some(w) = self.widget.upgrade() {
            if let Some(h) = handler {
                crate::disconnect(&w, h);
            }
            if complete {
                w.set_opacity(self.target);
            }
        }
        let tween = self.tween.borrow_mut().take();
        if let Some(t) = tween {
            t.cancel();
        }
        let done = self.done.borrow_mut().take();
        if complete {
            if let Some(done) = done {
                done();
            }
        }
    }
}
impl Fade {
    pub fn cancel(&self) {
        self.0.settle(false);
    }
    pub fn finished(&self) -> bool {
        self.0.finished.get()
    }
}
pub fn fade(
    widget: &impl IsA<gtk::Widget>,
    start: f64,
    end: f64,
    ms: u32,
    done: Option<Box<dyn FnOnce()>>,
) -> Fade {
    let w = widget.as_ref();
    w.set_opacity(start);
    let inner = Rc::new(FadeInner {
        widget: w.downgrade(),
        tween: RefCell::new(None),
        unmap: RefCell::new(None),
        done: RefCell::new(done),
        finished: Cell::new(false),
        target: end,
    });
    if ms == 0 || !w.is_mapped() {
        inner.settle(true);
        return Fade(inner);
    }
    let state = inner.clone();
    *inner.unmap.borrow_mut() = Some(w.connect_unmap(move |_| state.settle(true)));
    let weak = w.downgrade();
    let state = inner.clone();
    let t = motion::animate(
        w,
        ms,
        move |p| {
            if let Some(w) = weak.upgrade() {
                w.set_opacity(start + (end - start) * p);
            }
        },
        AnimateOptions {
            key: MotionKey::Opacity,
            easing: motion::easing("linear").expect("built-in curve"),
            on_done: Some(Box::new(move || state.settle(true))),
            ..Default::default()
        },
    );
    if !inner.finished.get() {
        *inner.tween.borrow_mut() = Some(t);
    }
    Fade(inner)
}

#[derive(Clone)]
pub struct IconButton {
    pub button: gtk::Button,
    hover: Rc<Cell<f64>>,
}
impl IconButton {
    pub fn new(icon: &str, glyph: &str, tooltip: &str) -> Self {
        let button = gtk::Button::new();
        button.set_relief(gtk::ReliefStyle::None);
        button.set_focus_on_click(false);
        button.set_tooltip_text(Some(tooltip));
        button.style_context().add_class("impro-terminal-icon");
        let obj = Self {
            button,
            hover: Rc::new(Cell::new(0.0)),
        };
        obj.set_icon(icon, glyph);
        obj.button
            .add_events(gdk::EventMask::ENTER_NOTIFY_MASK | gdk::EventMask::LEAVE_NOTIFY_MASK);
        let weak = obj.button.downgrade();
        let hover = obj.hover.clone();
        obj.button.connect_enter_notify_event(move |_, e| {
            if e.detail() != gdk::NotifyType::Inferior {
                if let Some(b) = weak.upgrade() {
                    Self::animate_hover(&b, &hover, 1.0);
                }
            }
            glib::Propagation::Proceed
        });
        let weak = obj.button.downgrade();
        let hover = obj.hover.clone();
        obj.button.connect_leave_notify_event(move |_, e| {
            if e.detail() != gdk::NotifyType::Inferior {
                if let Some(b) = weak.upgrade() {
                    Self::animate_hover(&b, &hover, 0.0);
                }
            }
            glib::Propagation::Proceed
        });
        obj
    }
    fn animate_hover(button: &gtk::Button, hover: &Rc<Cell<f64>>, target: f64) {
        let start = hover.get();
        let hover = hover.clone();
        let weak = button.downgrade();
        motion::animate(
            button,
            "state",
            move |p| {
                let value = start + (target - start) * p;
                hover.set(value);
                if let Some(child) = weak.upgrade().and_then(|b| b.child()) {
                    child.set_opacity(GLYPH_OPACITY + (1.0 - GLYPH_OPACITY) * value);
                }
            },
            AnimateOptions::default(),
        );
    }
    pub fn set_hover(&self, value: f64) {
        Self::animate_hover(&self.button, &self.hover, value.clamp(0.0, 1.0));
    }
    pub fn set_icon(&self, icon: &str, glyph: &str) {
        if let Some(old) = self.button.child() {
            self.button.remove(&old);
        }
        let child: gtk::Widget = if gtk::IconTheme::default().is_some_and(|t| t.has_icon(icon)) {
            gtk::Image::from_icon_name(Some(icon), gtk::IconSize::Menu).upcast()
        } else {
            gtk::Label::new(Some(glyph)).upcast()
        };
        child.set_opacity(GLYPH_OPACITY + (1.0 - GLYPH_OPACITY) * self.hover.get());
        child.show();
        self.button.add(&child);
    }
}

#[derive(Clone)]
pub struct PaneHeader {
    pub widget: gtk::Box,
    pub label: gtk::Label,
    pub label_button: gtk::Button,
    pub fullscreen_button: IconButton,
    pub delete_button: IconButton,
    shell_name: Rc<RefCell<String>>,
    number: Rc<Cell<usize>>,
    kind: String,
}
impl PaneHeader {
    pub fn new(shell_name: &str, kind: &str) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        widget.style_context().add_class("impro-terminal-header");
        widget.set_hexpand(true);
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_ellipsize(pango::EllipsizeMode::End);
        let label_button = gtk::Button::new();
        label_button.set_relief(gtk::ReliefStyle::None);
        label_button.set_focus_on_click(false);
        label_button
            .style_context()
            .add_class("impro-terminal-header-label");
        label_button.add(&label);
        let fullscreen_button = IconButton::new("view-fullscreen-symbolic", "□", "Fullscreen");
        let delete_button = IconButton::new(
            "edit-delete-symbolic",
            "✕",
            match kind {
                "graph" => "Close graph view",
                "gitgraph" => "Close git graph",
                _ => "Kill terminal",
            },
        );
        delete_button
            .button
            .style_context()
            .add_class("impro-terminal-delete");
        widget.pack_start(&label_button, false, true, 0);
        widget.pack_end(&delete_button.button, false, false, 0);
        widget.pack_end(&fullscreen_button.button, false, false, 0);
        let obj = Self {
            widget,
            label,
            label_button,
            fullscreen_button,
            delete_button,
            shell_name: Rc::new(RefCell::new(shell_name.into())),
            number: Rc::new(Cell::new(1)),
            kind: kind.into(),
        };
        obj.set_number(1);
        obj
    }
    pub fn text(&self) -> String {
        self.label.text().into()
    }
    pub fn set_shell_name(&self, name: &str) {
        *self.shell_name.borrow_mut() = name.into();
        self.set_number(self.number.get());
    }
    pub fn set_number(&self, position: usize) {
        self.number.set(position);
        self.label.set_text(&match self.kind.as_str() {
            "graph" => "Graph view".into(),
            "gitgraph" => "Git graph".into(),
            _ => format!("{}{position}", self.shell_name.borrow()),
        });
    }
    pub fn set_active(&self, value: bool) {
        set_class(&self.widget, "active", value);
    }
    pub fn set_focused(&self, value: bool) {
        set_class(&self.widget, "focused", value);
    }
    pub fn set_corner(&self, value: bool) {
        set_class(&self.widget, "corner", value);
    }
    pub fn set_maximized(&self, value: bool) {
        self.fullscreen_button.set_icon(
            if value {
                "view-restore-symbolic"
            } else {
                "view-fullscreen-symbolic"
            },
            if value { "❐" } else { "□" },
        );
        self.fullscreen_button
            .button
            .set_tooltip_text(Some(if value { "Restore" } else { "Fullscreen" }));
    }
    pub fn connect_focus_requested(&self, f: impl Fn() + 'static) -> glib::SignalHandlerId {
        self.label_button.connect_clicked(move |_| f())
    }
    pub fn connect_fullscreen_requested(&self, f: impl Fn() + 'static) -> glib::SignalHandlerId {
        self.fullscreen_button.button.connect_clicked(move |_| f())
    }
    pub fn connect_delete_requested(&self, f: impl Fn() + 'static) -> glib::SignalHandlerId {
        self.delete_button.button.connect_clicked(move |_| f())
    }
}
pub(crate) fn set_class(w: &impl IsA<gtk::Widget>, name: &str, on: bool) {
    let c = w.style_context();
    if on {
        c.add_class(name);
    } else {
        c.remove_class(name);
    }
}

type Choose = Rc<dyn Fn(&str)>;
struct AddInner {
    icon: IconButton,
    popover: gtk::Popover,
    choices: gtk::ListBox,
    kinds: RefCell<Vec<String>>,
    limit: Cell<usize>,
    callback: RefCell<Option<Choose>>,
}
#[derive(Clone)]
pub struct AddButton(Rc<AddInner>);
impl AddButton {
    pub fn new(limit: usize) -> Self {
        let icon = IconButton::new("list-add-symbolic", "+", "New pane");
        icon.button.style_context().add_class("impro-terminal-add");
        icon.button.set_halign(gtk::Align::End);
        icon.button.set_valign(gtk::Align::Start);
        let popover = gtk::Popover::new(Some(&icon.button));
        popover.set_position(gtk::PositionType::Bottom);
        popover.style_context().add_class("impro-pane-chooser");
        let choices = gtk::ListBox::new();
        choices.set_selection_mode(gtk::SelectionMode::Single);
        choices.set_activate_on_single_click(true);
        popover.add(&choices);
        let obj = Self(Rc::new(AddInner {
            icon,
            popover,
            choices,
            kinds: RefCell::new(Vec::new()),
            limit: Cell::new(limit),
            callback: RefCell::new(None),
        }));
        let weak = Rc::downgrade(&obj.0);
        obj.0.icon.button.connect_clicked(move |_| {
            if let Some(o) = weak.upgrade() {
                Self(o).open();
            }
        });
        let weak = Rc::downgrade(&obj.0);
        obj.0.choices.connect_row_activated(move |_, r| {
            if let Some(o) = weak.upgrade() {
                let k = o.kinds.borrow().get(r.index() as usize).cloned();
                if let Some(k) = k {
                    Self(o).choose(&k);
                }
            }
        });
        let weak = Rc::downgrade(&obj.0);
        obj.0.popover.connect_key_press_event(move |_, e| {
            weak.upgrade()
                .map(|o| Self(o).handle_key(e.keyval()))
                .unwrap_or(glib::Propagation::Proceed)
        });
        let popover = obj.0.popover.downgrade();
        obj.0.icon.button.connect_destroy(move |_| {
            if let Some(p) = popover.upgrade() {
                unsafe {
                    p.destroy();
                }
            }
        });
        obj.set_kinds(&["terminal", "graph", "gitgraph"]);
        obj
    }
    pub fn widget(&self) -> &gtk::Button {
        &self.0.icon.button
    }
    pub fn popover(&self) -> &gtk::Popover {
        &self.0.popover
    }
    pub fn choices(&self) -> &gtk::ListBox {
        &self.0.choices
    }
    pub fn connect_choose(&self, f: impl Fn(&str) + 'static) {
        *self.0.callback.borrow_mut() = Some(Rc::new(f));
    }
    pub fn set_limit(&self, n: usize) {
        self.0.limit.set(n);
    }
    pub fn set_kinds(&self, kinds: &[&str]) {
        self.0.popover.popdown();
        for c in self.0.choices.children() {
            unsafe {
                c.destroy();
            }
        }
        let available = [
            ("terminal", "Terminal", "utilities-terminal-symbolic"),
            ("graph", "Graph view", "network-workgroup-symbolic"),
            ("gitgraph", "Git graph", "view-list-tree-symbolic"),
        ];
        let mut enabled = Vec::new();
        for (kind, label, icon) in available {
            if !kinds.contains(&kind) {
                continue;
            }
            enabled.push(kind.to_string());
            let row = gtk::ListBoxRow::new();
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            b.pack_start(
                &gtk::Image::from_icon_name(Some(icon), gtk::IconSize::Menu),
                false,
                false,
                0,
            );
            let l = gtk::Label::new(Some(label));
            l.set_xalign(0.0);
            b.pack_start(&l, true, true, 0);
            row.add(&b);
            self.0.choices.add(&row);
        }
        let sensitive = !enabled.is_empty();
        *self.0.kinds.borrow_mut() = enabled;
        self.widget().set_sensitive(sensitive);
    }
    pub fn open(&self) {
        if self.0.kinds.borrow().is_empty() {
            return;
        }
        unsafe {
            gtk::ffi::gtk_popover_set_transitions_enabled(
                self.0.popover.to_glib_none().0,
                motion::animations_enabled().into_glib(),
            );
        }
        self.0.popover.show_all();
        if let Some(row) = self.0.choices.row_at_index(0) {
            self.0.choices.select_row(Some(&row));
            self.0.popover.popup();
            row.grab_focus();
        }
    }
    pub fn choose(&self, kind: &str) {
        if !self.0.kinds.borrow().iter().any(|k| k == kind) {
            return;
        }
        let callback = self.0.callback.borrow().clone();
        self.0.popover.popdown();
        if let Some(f) = callback {
            f(kind);
        }
    }
    pub fn handle_key(&self, key: gdk::keys::Key) -> glib::Propagation {
        use gdk::keys::constants::{space, Down, End, Escape, Home, KP_Enter, Return, Up};
        let count = self.0.kinds.borrow().len() as i32;
        if count == 0 {
            return glib::Propagation::Proceed;
        }
        let index = self
            .0
            .choices
            .selected_row()
            .map(|r| r.index())
            .unwrap_or(0);
        if key == Escape {
            self.0.popover.popdown();
            self.widget().grab_focus();
        } else if key == Return || key == KP_Enter || key == space {
            let k = self.0.kinds.borrow().get(index as usize).cloned();
            if let Some(k) = k {
                self.choose(&k);
            }
        } else if key == Up || key == Down || key == Home || key == End {
            let i = if key == Home {
                0
            } else if key == End {
                count - 1
            } else {
                (index + if key == Down { 1 } else { -1 }).clamp(0, count - 1)
            };
            if let Some(row) = self.0.choices.row_at_index(i) {
                self.0.choices.select_row(Some(&row));
                row.grab_focus();
            }
        } else {
            return glib::Propagation::Proceed;
        }
        glib::Propagation::Stop
    }
    pub fn set_full(&self, full: bool) {
        set_class(self.widget(), "limit", full);
        self.widget().set_tooltip_text(Some(&if full {
            format!("Cannot add (limit {})", self.0.limit.get())
        } else {
            "New pane".into()
        }));
    }
}
struct ToastInner {
    label: gtk::Label,
    leaving: Cell<bool>,
    timer: RefCell<Option<glib::SourceId>>,
    destroyed: Cell<bool>,
}
#[derive(Clone)]
pub struct LimitToast(Rc<ToastInner>);
impl LimitToast {
    pub fn new(limit: usize) -> Self {
        let label = gtk::Label::new(Some(&format!("Cannot add (limit {limit})")));
        label.style_context().add_class("impro-terminal-toast");
        label.set_halign(gtk::Align::End);
        label.set_valign(gtk::Align::Start);
        label.set_no_show_all(true);
        let obj = Self(Rc::new(ToastInner {
            label,
            leaving: Cell::new(false),
            timer: RefCell::new(None),
            destroyed: Cell::new(false),
        }));
        let weak = Rc::downgrade(&obj.0);
        obj.0.label.connect_destroy(move |_| {
            if let Some(o) = weak.upgrade() {
                o.destroyed.set(true);
                let s = o.timer.borrow_mut().take();
                if let Some(s) = s {
                    s.remove();
                }
            }
        });
        let weak = Rc::downgrade(&obj.0);
        obj.0.label.connect_unmap(move |l| {
            if let Some(o) = weak.upgrade() {
                let s = o.timer.borrow_mut().take();
                if let Some(s) = s {
                    s.remove();
                }
                o.leaving.set(false);
                motion::fade(l, false, 0, true, None);
                l.set_opacity(1.0);
            }
        });
        obj
    }
    pub fn widget(&self) -> &gtk::Label {
        &self.0.label
    }
    pub fn has_timer(&self) -> bool {
        self.0.timer.borrow().is_some()
    }
    pub fn popup(&self) {
        if self.0.destroyed.get() {
            return;
        }
        let old = self.0.timer.borrow_mut().take();
        if let Some(s) = old {
            s.remove();
        }
        if self.0.leaving.replace(false) || !self.0.label.is_visible() {
            motion::slide_in(&self.0.label, 8, ENTER_MS, None);
        }
        let weak = Rc::downgrade(&self.0);
        let id = glib::timeout_add_local_once(Duration::from_millis(SHOW_MS), move || {
            if let Some(o) = weak.upgrade() {
                o.timer.borrow_mut().take();
                o.leaving.set(true);
                let weak = Rc::downgrade(&o);
                motion::fade(
                    &o.label,
                    false,
                    FADE_MS,
                    true,
                    Some(Box::new(move || {
                        if let Some(o) = weak.upgrade() {
                            o.leaving.set(false);
                            o.label.set_opacity(1.0);
                        }
                    })),
                );
            }
        });
        *self.0.timer.borrow_mut() = Some(id);
    }
}
