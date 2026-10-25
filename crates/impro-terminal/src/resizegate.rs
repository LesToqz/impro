//! Clip a complete terminal page while its program answers the last grid change.
use glib::translate::*;
use gtk::{prelude::*, subclass::prelude::*};
use impro_vte::Terminal;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

pub const PACE_MS: u64 = 50;
struct Answer {
    terminal: Terminal,
    handler: RefCell<Option<glib::SignalHandlerId>>,
    watch: RefCell<Option<glib::SourceId>>,
    written: Cell<bool>,
}
mod imp {
    use super::*;
    pub struct ResizeGate {
        pub target: Cell<Option<(i32, i32)>>,
        pub committed: Cell<Option<(i32, i32)>>,
        pub child_minimum: Cell<bool>,
        pub reflow: Cell<bool>,
        pub pace_ms: Cell<u64>,
        pub destroyed: Cell<bool>,
        pub extended: Cell<bool>,
        pub requested: Cell<bool>,
        pub timer: RefCell<Option<glib::SourceId>>,
        pub release: RefCell<Option<glib::SourceId>>,
        pub(super) answers: RefCell<Vec<Rc<Answer>>>,
        pub terminals: RefCell<Vec<Terminal>>,
        pub provider: RefCell<Option<gtk::CssProvider>>,
        pub commits: Cell<u64>,
    }
    impl Default for ResizeGate {
        fn default() -> Self {
            Self {
                target: Cell::new(None),
                committed: Cell::new(None),
                child_minimum: Cell::new(false),
                reflow: Cell::new(false),
                pace_ms: Cell::new(PACE_MS),
                destroyed: Cell::new(false),
                extended: Cell::new(false),
                requested: Cell::new(false),
                timer: RefCell::new(None),
                release: RefCell::new(None),
                answers: RefCell::new(Vec::new()),
                terminals: RefCell::new(Vec::new()),
                provider: RefCell::new(None),
                commits: Cell::new(0),
            }
        }
    }
    #[glib::object_subclass]
    impl ObjectSubclass for ResizeGate {
        const NAME: &'static str = "ImproResizeGate";
        type Type = super::ResizeGate;
        type ParentType = gtk::Bin;
    }
    impl ObjectImpl for ResizeGate {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_has_window(true);
            obj.style_context().add_class("background");
            obj.style_context().add_class("impro-resize-gate");
            obj.connect_destroy(|obj| {
                obj.imp().destroyed.set(true);
                obj.cancel_timer();
                obj.stop_waiting();
                let terminals = std::mem::take(&mut *obj.imp().terminals.borrow_mut());
                drop(terminals);
            });
            obj.connect_add(|obj, _| obj.child_changed());
            obj.connect_remove(|obj, _| obj.child_changed());
        }
        fn dispose(&self) {
            self.obj().cancel_timer();
            self.obj().stop_waiting();
            let terminals = std::mem::take(&mut *self.terminals.borrow_mut());
            drop(terminals);
        }
    }
    impl ResizeGate {
        fn preferred(&self, vertical: bool, other: Option<i32>) -> (i32, i32) {
            let Some(child) = self.obj().child().filter(|c| c.is_visible()) else {
                return (0, 0);
            };
            let (low, nat) = match (vertical, other) {
                (true, Some(w)) => child.preferred_height_for_width(w.max(1)),
                (false, Some(h)) => child.preferred_width_for_height(h.max(1)),
                (true, None) => child.preferred_height(),
                (false, None) => child.preferred_width(),
            };
            (if self.child_minimum.get() { low } else { 0 }, nat)
        }
    }
    impl WidgetImpl for ResizeGate {
        fn draw(&self, cr: &gtk::cairo::Context) -> glib::Propagation {
            let obj = self.obj();
            gtk::render_background(
                &obj.style_context(),
                cr,
                0.0,
                0.0,
                obj.allocated_width() as f64,
                obj.allocated_height() as f64,
            );
            self.parent_draw(cr)
        }

        fn preferred_width(&self) -> (i32, i32) {
            self.preferred(false, None)
        }
        fn preferred_height(&self) -> (i32, i32) {
            self.preferred(true, None)
        }
        fn preferred_width_for_height(&self, h: i32) -> (i32, i32) {
            self.preferred(false, Some(h))
        }
        fn preferred_height_for_width(&self, w: i32) -> (i32, i32) {
            self.preferred(true, Some(w))
        }
        fn realize(&self) {
            let obj = self.obj();
            obj.set_realized(true);
            let a = obj.allocation();
            let window = gdk::Window::new(
                obj.parent_window().as_ref(),
                &gdk::WindowAttr {
                    window_type: gdk::WindowType::Child,
                    x: Some(a.x()),
                    y: Some(a.y()),
                    width: a.width().max(1),
                    height: a.height().max(1),
                    visual: obj.visual(),
                    event_mask: obj.events() | gdk::EventMask::EXPOSURE_MASK,
                    ..Default::default()
                },
            );
            // Primary window ownership is released by GtkWidget's unrealize.
            unsafe {
                gdk::ffi::gdk_window_set_user_data(window.to_glib_none().0, obj.as_ptr().cast());
            }
            obj.set_window(window);
            obj.commit();
        }
        fn unrealize(&self) {
            self.obj().cancel_timer();
            self.obj().stop_waiting();
            self.parent_unrealize();
        }
        fn size_allocate(&self, a: &gtk::Allocation) {
            let obj = self.obj();
            let safe = gtk::Allocation::new(a.x(), a.y(), a.width().max(1), a.height().max(1));
            obj.set_allocation(&safe);
            if let Some(w) = obj.window().filter(|_| obj.is_realized()) {
                w.move_resize(safe.x(), safe.y(), safe.width(), safe.height());
            }
            let target = (safe.width(), safe.height());
            self.target.set(Some(target));
            if self.committed.get().is_none()
                || (self.committed.get() != Some(target) && self.answers.borrow().is_empty())
            {
                obj.commit();
            } else {
                obj.allocate_child();
            }
        }
    }
    impl ContainerImpl for ResizeGate {}
    impl BinImpl for ResizeGate {}
}
glib::wrapper! { pub struct ResizeGate(ObjectSubclass<imp::ResizeGate>) @extends gtk::Bin,gtk::Container,gtk::Widget,@implements gtk::Buildable; }
impl Default for ResizeGate {
    fn default() -> Self {
        glib::Object::new()
    }
}
impl ResizeGate {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_child_minimum(&self, enabled: bool) {
        self.imp().child_minimum.set(enabled);
        self.queue_resize();
    }
    pub fn set_reflow_vte(&self, enabled: bool) {
        self.imp().reflow.set(enabled);
    }
    pub fn set_pace_ms(&self, ms: u64) {
        self.imp().pace_ms.set(ms.max(1));
    }
    pub fn target(&self) -> Option<(i32, i32)> {
        self.imp().target.get()
    }
    pub fn committed(&self) -> Option<(i32, i32)> {
        self.imp().committed.get()
    }
    pub fn waiting_count(&self) -> usize {
        self.imp().answers.borrow().len()
    }
    pub fn has_pending_sources(&self) -> bool {
        self.imp().timer.borrow().is_some()
            || self.imp().release.borrow().is_some()
            || self.waiting_count() > 0
    }
    pub fn commit_count(&self) -> u64 {
        self.imp().commits.get()
    }
    fn child_changed(&self) {
        self.cancel_timer();
        self.stop_waiting();
        self.imp().committed.set(None);
        fn collect(w: &gtk::Widget, out: &mut Vec<Terminal>) {
            if let Some(t) = w.downcast_ref::<Terminal>() {
                out.push(t.clone());
            }
            if let Some(c) = w.downcast_ref::<gtk::Container>() {
                for child in c.children() {
                    collect(&child, out);
                }
            }
        }
        let mut terminals = Vec::new();
        if let Some(c) = self.child() {
            collect(&c, &mut terminals);
        }
        *self.imp().terminals.borrow_mut() = terminals;
        self.queue_resize();
    }
    fn allocate_child(&self) {
        let Some(child) = self.child().filter(|c| c.is_visible()) else {
            return;
        };
        let Some((width, height)) = self.committed() else {
            return;
        };
        let width = width
            .max(child.preferred_width().0)
            .max(child.margin_start() + child.margin_end() + 1);
        let height = height
            .max(child.preferred_height().0)
            .max(child.margin_top() + child.margin_bottom() + 1);
        let terminals = self.imp().terminals.borrow().clone();
        let grids: Vec<_> = terminals
            .iter()
            .map(|t| (t.column_count(), t.row_count(), t.rewrap_on_resize()))
            .collect();
        if !self.imp().reflow.get() {
            for t in &terminals {
                t.set_rewrap_on_resize(false);
            }
        }
        child.size_allocate(&gtk::Allocation::new(0, 0, width, height));
        if !self.imp().reflow.get() {
            for (t, g) in terminals.iter().zip(&grids) {
                t.set_rewrap_on_resize(g.2);
            }
        }
        let changed: Vec<_> = terminals
            .into_iter()
            .zip(grids)
            .filter(|(t, g)| {
                (t.column_count(), t.row_count()) != (g.0, g.1)
                    && t.is_mapped()
                    && t.pty().is_some()
            })
            .map(|(t, _)| t)
            .collect();
        if !changed.is_empty() && !self.imp().destroyed.get() {
            self.wait_for(changed);
        }
    }
    /// Pace the next allocation against output from these terminal programs.
    pub fn wait_for_programs(&self, terminals: &[Terminal]) {
        self.wait_for(terminals.to_vec());
    }
    fn wait_for(&self, terminals: Vec<Terminal>) {
        self.stop_waiting();
        for terminal in terminals {
            let answer = Rc::new(Answer {
                terminal: terminal.clone(),
                handler: RefCell::new(None),
                watch: RefCell::new(None),
                written: Cell::new(false),
            });
            let weak = self.downgrade();
            *answer.handler.borrow_mut() = Some(terminal.connect_contents_changed(move |t| {
                if let Some(g) = weak.upgrade() {
                    g.answered(t);
                }
            }));
            #[cfg(unix)]
            if let Some(pty) = terminal.pty() {
                let weak = Rc::downgrade(&answer);
                let source = glib::source::unix_fd_add_local(
                    pty.fd(),
                    glib::IOCondition::IN,
                    move |_, _| {
                        if let Some(a) = weak.upgrade() {
                            a.watch.borrow_mut().take();
                            a.written.set(true);
                        }
                        glib::ControlFlow::Break
                    },
                );
                if let Some(s) = glib::MainContext::default().find_source_by_id(&source) {
                    unsafe {
                        glib::ffi::g_source_set_priority(
                            s.to_glib_none().0,
                            glib::Priority::HIGH.into_glib(),
                        );
                    }
                }
                *answer.watch.borrow_mut() = Some(source);
            }
            self.imp().answers.borrow_mut().push(answer);
        }
        self.imp().extended.set(false);
        self.schedule_release();
    }
    fn schedule_release(&self) {
        let weak = self.downgrade();
        let source = glib::timeout_add_local_once(
            Duration::from_millis(self.imp().pace_ms.get()),
            move || {
                if let Some(g) = weak.upgrade() {
                    g.imp().release.borrow_mut().take();
                    let written = g.imp().answers.borrow().iter().any(|a| a.written.get());
                    if !g.imp().extended.get() && written {
                        g.imp().extended.set(true);
                        g.schedule_release();
                    } else {
                        g.stop_waiting();
                        g.settled();
                    }
                }
            },
        );
        *self.imp().release.borrow_mut() = Some(source);
    }
    fn answered(&self, t: &Terminal) {
        let answer = self
            .imp()
            .answers
            .borrow()
            .iter()
            .find(|a| a.terminal == *t)
            .cloned();
        let Some(answer) = answer else { return };
        if answer.watch.borrow().is_some() || t.pty().is_some_and(|p| p.unread_bytes() != 0) {
            return;
        }
        self.imp().answers.borrow_mut().retain(|a| a.terminal != *t);
        let handler = answer.handler.borrow_mut().take();
        if let Some(h) = handler {
            crate::disconnect(t, h);
        }
        if self.waiting_count() == 0 {
            self.stop_waiting();
            if self.imp().timer.borrow().is_none() {
                let weak = self.downgrade();
                let id = glib::idle_add_local_full(glib::Priority::HIGH_IDLE, move || {
                    if let Some(g) = weak.upgrade() {
                        g.imp().timer.borrow_mut().take();
                        g.settled();
                    }
                    glib::ControlFlow::Break
                });
                *self.imp().timer.borrow_mut() = Some(id);
            }
        }
    }
    fn stop_waiting(&self) {
        let answers = std::mem::take(&mut *self.imp().answers.borrow_mut());
        for a in answers {
            let handler = a.handler.borrow_mut().take();
            if let Some(h) = handler {
                crate::disconnect(&a.terminal, h);
            }
            let watch = a.watch.borrow_mut().take();
            if let Some(s) = watch {
                s.remove();
            }
        }
        let source = self.imp().release.borrow_mut().take();
        if let Some(s) = source {
            s.remove();
        }
    }
    fn cancel_timer(&self) {
        let source = self.imp().timer.borrow_mut().take();
        if let Some(s) = source {
            s.remove();
        }
    }
    fn settled(&self) {
        if self.waiting_count() == 0
            && self.target().is_some()
            && (self.target() != self.committed() || self.imp().requested.get())
        {
            self.commit();
        }
    }
    /// Bypass waiting for a discrete layout action; never inject or erase text.
    pub fn commit(&self) {
        self.cancel_timer();
        self.imp().requested.set(false);
        if self.imp().destroyed.get() {
            return;
        }
        let target = self.target().unwrap_or_else(|| {
            let a = self.allocation();
            (a.width().max(1), a.height().max(1))
        });
        self.imp().target.set(Some(target));
        self.imp().committed.set(Some(target));
        self.imp().commits.set(self.commit_count() + 1);
        self.allocate_child();
        self.queue_draw();
    }
    /// Discrete layout action that must still honor an in-flight program answer.
    pub fn commit_when_answered(&self) {
        if self.waiting_count() > 0 {
            self.imp().requested.set(true);
        } else {
            self.commit();
        }
    }
    pub fn set_background(&self, value: &str) -> Result<(), glib::Error> {
        let color = gdk::RGBA::parse(value).map_err(|_| {
            glib::Error::new(
                gio::IOErrorEnum::InvalidArgument,
                "Invalid background colour",
            )
        })?;
        let provider = gtk::CssProvider::new();
        provider.load_from_data(
            format!(".impro-resize-gate {{ background-color: {}; }}", color).as_bytes(),
        )?;
        let old = self.imp().provider.borrow_mut().replace(provider.clone());
        let context = self.style_context();
        if let Some(old) = old {
            context.remove_provider(&old);
        }
        context.add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        self.queue_draw();
        Ok(())
    }
}
