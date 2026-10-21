//! Ratio layout with six-pixel handles, pairwise resizing and equalization.
use gtk::{prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};

pub const HANDLE_SIZE: i32 = 6;

/// Integer apportionment preserving the total, even below combined minima.
pub fn sizes_from_ratios(ratios: &[f64], extent: i32, minimum: i32) -> Vec<i32> {
    let n = ratios.len();
    if n == 0 { return Vec::new(); }
    let extent = extent.max(0);
    if extent <= n as i32 * minimum.max(0) {
        return (0..n).map(|i| extent / n as i32 + i32::from((i as i32) < extent % n as i32)).collect();
    }
    let weights: Vec<_> = ratios.iter().map(|r| if r.is_finite() { r.max(0.0) } else { 0.0 }).collect();
    let mut result = vec![0.0; n];
    let mut pending = vec![true; n];
    let mut remaining = extent as f64;
    loop {
        let count = pending.iter().filter(|v| **v).count();
        if count == 0 { break; }
        let total: f64 = weights.iter().zip(&pending).filter(|(_, p)| **p).map(|(w, _)| w).sum();
        let mut small = false;
        for i in 0..n {
            if pending[i] {
                result[i] = if total > 0.0 { remaining * weights[i] / total } else { remaining / count as f64 };
            }
        }
        for i in 0..n {
            if pending[i] && result[i] < minimum as f64 {
                result[i] = minimum as f64;
                remaining -= minimum as f64;
                pending[i] = false;
                small = true;
            }
        }
        if !small { break; }
    }
    let mut pixels: Vec<_> = result.iter().map(|r| r.floor() as i32).collect();
    let mut order: Vec<_> = (0..n).collect();
    order.sort_by(|a, b| (result[*b] - pixels[*b] as f64).total_cmp(&(result[*a] - pixels[*a] as f64)));
    let extra = (extent - pixels.iter().sum::<i32>()).max(0) as usize;
    for i in order.into_iter().take(extra) { pixels[i] += 1; }
    pixels
}

/// Update a pair in place. This path performs no heap allocation during motion.
pub fn move_boundary(sizes: &mut [i32], index: usize, delta: f64, minimum: i32) {
    if index + 1 >= sizes.len() || !delta.is_finite() { return; }
    let total = sizes[index] + sizes[index + 1];
    let floor = minimum.max(0).min(total / 2);
    sizes[index] = (sizes[index] as f64 + delta).round_ties_even().clamp(floor as f64, (total - floor) as f64) as i32;
    sizes[index + 1] = total - sizes[index];
}

#[derive(Default, Clone, Debug)]
pub struct SplitLayout { pub ratios: Vec<f64> }
impl SplitLayout {
    pub fn equalize(&mut self) { self.ratios.fill(1.0); }
    pub fn insert(&mut self, index: usize) { self.ratios.insert(index.min(self.ratios.len()), 1.0); self.equalize(); }
    pub fn remove(&mut self, index: usize) {
        if index >= self.ratios.len() { return; }
        let removed = self.ratios.remove(index);
        let left = index.checked_sub(1);
        let right = (index < self.ratios.len()).then_some(index);
        let total = left.map(|i| self.ratios[i]).unwrap_or(0.0) + right.map(|i| self.ratios[i]).unwrap_or(0.0);
        let count = usize::from(left.is_some()) + usize::from(right.is_some());
        for i in [left, right].into_iter().flatten() {
            self.ratios[i] += removed * if total > 0.0 { self.ratios[i] / total } else { 1.0 / count as f64 };
        }
    }
}

mod imp {
    use super::*;
    #[derive(Default)]
    pub struct PaneSplit {
        pub panes: RefCell<Vec<gtk::Widget>>,
        pub handles: RefCell<Vec<gtk::EventBox>>,
        pub layout: RefCell<SplitLayout>,
        pub vertical: Cell<bool>,
        pub minimum: Cell<i32>,
        pub generation: Cell<u64>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for PaneSplit {
        const NAME: &'static str = "ImproPaneSplit";
        type Type = super::PaneSplit;
        type ParentType = gtk::Container;
    }
    impl ObjectImpl for PaneSplit {
        fn constructed(&self) { self.parent_constructed(); self.obj().set_has_window(false); }
        fn dispose(&self) {
            let panes = std::mem::take(&mut *self.panes.borrow_mut());
            let handles = std::mem::take(&mut *self.handles.borrow_mut());
            for child in panes { child.unparent(); }
            for child in handles { child.unparent(); }
        }
    }
    impl PaneSplit {
        fn preferred(&self, vertical: bool) -> (i32, i32) {
            let panes = self.panes.borrow().clone();
            let mut low = 0; let mut nat = 0; let mut count = 0;
            for pane in panes.into_iter().filter(|p| p.is_visible()) {
                let (a, b) = if vertical { pane.preferred_height() } else { pane.preferred_width() };
                if vertical == self.vertical.get() { nat += b; } else { low = low.max(a); nat = nat.max(b); }
                count += 1;
            }
            if vertical == self.vertical.get() { (0, nat + HANDLE_SIZE * (count - 1).max(0)) } else { (low, nat) }
        }
    }
    impl WidgetImpl for PaneSplit {
        fn preferred_width(&self) -> (i32, i32) { self.preferred(false) }
        fn preferred_height(&self) -> (i32, i32) { self.preferred(true) }
        fn size_allocate(&self, a: &gtk::Allocation) {
            let obj = self.obj(); obj.set_allocation(a);
            obj.sync_handles();
            let panes = self.panes.borrow().clone();
            let handles = self.handles.borrow().clone();
            let visible = panes.iter().filter(|p| p.is_visible()).count();
            if visible == 0 { return; }
            let sizes = obj.pane_sizes();
            let mut offset = 0;
            for (i, pane) in panes.iter().enumerate().filter(|(_, p)| p.is_visible()) {
                let size = if visible == panes.len() { sizes[i] } else if self.vertical.get() { a.height() } else { a.width() };
                let rect = if self.vertical.get() { gtk::Allocation::new(a.x(), a.y() + offset, a.width().max(1), size.max(1)) }
                    else { gtk::Allocation::new(a.x() + offset, a.y(), size.max(1), a.height().max(1)) };
                pane.size_allocate(&rect); offset += size;
                if let Some(handle) = handles.get(i).filter(|h| h.is_visible()) {
                    let rect = if self.vertical.get() { gtk::Allocation::new(a.x(), a.y() + offset, a.width().max(1), HANDLE_SIZE) }
                        else { gtk::Allocation::new(a.x() + offset, a.y(), HANDLE_SIZE, a.height().max(1)) };
                    handle.size_allocate(&rect); offset += HANDLE_SIZE;
                }
            }
        }
    }
    impl ContainerImpl for PaneSplit {
        fn add(&self, child: &gtk::Widget) { let len = self.panes.borrow().len(); self.obj().insert(child, len); }
        fn remove(&self, child: &gtk::Widget) {
            let index = self.panes.borrow().iter().position(|p| p == child);
            if let Some(i) = index {
                self.panes.borrow_mut().remove(i); self.layout.borrow_mut().remove(i);
                child.unparent(); self.obj().rebuild_handles(); self.obj().queue_resize();
            } else {
                let i = self.handles.borrow().iter().position(|p| p.upcast_ref::<gtk::Widget>() == child);
                if let Some(i) = i { self.handles.borrow_mut().remove(i); child.unparent(); }
            }
        }
        fn child_type(&self) -> glib::Type { gtk::Widget::static_type() }
        fn forall(&self, internals: bool, cb: &gtk::subclass::container::Callback) {
            let panes = self.panes.borrow().clone();
            let handles = if internals { self.handles.borrow().clone() } else { Vec::new() };
            for p in panes { cb.call(&p); }
            for h in handles { cb.call(h.upcast_ref()); }
        }
    }
}
glib::wrapper! { pub struct PaneSplit(ObjectSubclass<imp::PaneSplit>) @extends gtk::Container, gtk::Widget, @implements gtk::Buildable; }
impl PaneSplit {
    pub fn new(orientation: gtk::Orientation, minimum: i32) -> Self {
        let obj: Self = glib::Object::new(); obj.imp().vertical.set(orientation == gtk::Orientation::Vertical); obj.imp().minimum.set(minimum); obj
    }
    pub fn orientation(&self) -> gtk::Orientation { if self.imp().vertical.get() { gtk::Orientation::Vertical } else { gtk::Orientation::Horizontal } }
    pub fn ratios(&self) -> Vec<f64> { self.imp().layout.borrow().ratios.clone() }
    pub fn handles(&self) -> Vec<gtk::EventBox> { self.imp().handles.borrow().clone() }
    pub fn pane_sizes(&self) -> Vec<i32> {
        let a = self.allocation(); let extent = if self.imp().vertical.get() { a.height() } else { a.width() };
        sizes_from_ratios(&self.imp().layout.borrow().ratios, extent - HANDLE_SIZE * self.imp().handles.borrow().len() as i32, self.imp().minimum.get())
    }
    pub fn insert(&self, child: &impl IsA<gtk::Widget>, index: usize) {
        if child.parent().is_some() { return; }
        let index = index.min(self.imp().panes.borrow().len());
        self.imp().panes.borrow_mut().insert(index, child.clone().upcast());
        self.imp().layout.borrow_mut().insert(index); child.set_parent(self); self.rebuild_handles(); self.queue_resize();
    }
    pub fn equalize(&self) { self.imp().layout.borrow_mut().equalize(); self.queue_resize(); }
    pub fn sync_handles(&self) {
        let panes = self.imp().panes.borrow().clone(); let handles = self.handles();
        for (i, h) in handles.iter().enumerate() { h.set_visible(panes.get(i).is_some_and(|p| p.is_visible()) && panes.get(i+1).is_some_and(|p| p.is_visible())); }
    }
    /// Resize an adjacent pair from the current allocation (keyboard/API path).
    pub fn move_handle(&self, index: usize, delta: f64) { let mut sizes = self.pane_sizes(); move_boundary(&mut sizes, index, delta, self.imp().minimum.get()); self.set_sizes(&sizes); }
    fn set_sizes(&self, sizes: &[i32]) {
        { let mut layout = self.imp().layout.borrow_mut(); for (r, s) in layout.ratios.iter_mut().zip(sizes) { *r = *s as f64; } }
        self.queue_resize();
    }
    fn rebuild_handles(&self) {
        let generation = self.imp().generation.get().wrapping_add(1); self.imp().generation.set(generation);
        let old = std::mem::take(&mut *self.imp().handles.borrow_mut());
        for h in old { unsafe { h.destroy(); } }
        let count = self.imp().panes.borrow().len().saturating_sub(1);
        for index in 0..count {
            let h = gtk::EventBox::new(); h.set_can_focus(true); h.set_no_show_all(true);
            h.style_context().add_class("impro-pane-handle"); h.set_tooltip_text(Some("Drag to resize panes; double-click to equalize"));
            let line = gtk::Separator::new(if self.imp().vertical.get() { gtk::Orientation::Horizontal } else { gtk::Orientation::Vertical });
            line.style_context().add_class("impro-terminal-sep");
            if self.imp().vertical.get() { line.set_valign(gtk::Align::Center); } else { line.set_halign(gtk::Align::Center); }
            h.add(&line); line.show();
            h.add_events(gdk::EventMask::BUTTON_PRESS_MASK | gdk::EventMask::BUTTON_RELEASE_MASK | gdk::EventMask::POINTER_MOTION_MASK | gdk::EventMask::ENTER_NOTIFY_MASK | gdk::EventMask::LEAVE_NOTIFY_MASK);
            let vertical = self.imp().vertical.get();
            h.connect_realize(move |h| { if let Some(w) = h.window() { w.set_cursor(gdk::Cursor::from_name(&h.display(), if vertical { "row-resize" } else { "col-resize" }).as_ref()); } });
            h.connect_enter_notify_event(|h, _| { h.set_state_flags(gtk::StateFlags::PRELIGHT, false); glib::Propagation::Proceed });
            h.connect_leave_notify_event(|h, _| { h.unset_state_flags(gtk::StateFlags::PRELIGHT); glib::Propagation::Proceed });
            // Capture once per press, then update a reusable fixed-capacity buffer.
            let drag = std::rc::Rc::new(RefCell::new(None::<(f64, Vec<i32>)>));
            let weak = self.downgrade(); let d = drag.clone();
            h.connect_button_press_event(move |h, e| {
                if e.button() != 1 { return glib::Propagation::Proceed; }
                if let Some(owner) = weak.upgrade() {
                    h.grab_focus();
                    if e.event_type() == gdk::EventType::DoubleButtonPress { d.borrow_mut().take(); owner.equalize(); }
                    else { let (x,y) = e.root(); *d.borrow_mut() = Some((if vertical {y} else {x}, owner.pane_sizes())); h.set_state_flags(gtk::StateFlags::ACTIVE, false); h.grab_add(); }
                }
                glib::Propagation::Stop
            });
            let weak = self.downgrade(); let d = drag.clone();
            h.connect_motion_notify_event(move |_, e| {
                let Some(owner) = weak.upgrade().filter(|o| o.imp().generation.get() == generation) else { return glib::Propagation::Proceed; };
                let state = d.borrow(); let Some((origin, sizes)) = state.as_ref() else { return glib::Propagation::Proceed; };
                // No cloning or temporary vectors on the pointer-motion path.
                { let mut layout = owner.imp().layout.borrow_mut(); let (x,y) = e.root(); let delta = if vertical {y} else {x} - origin;
                  let total = sizes[index] + sizes[index+1]; let floor = owner.imp().minimum.get().min(total/2);
                  let first = (sizes[index] as f64 + delta).round_ties_even().clamp(floor as f64, (total-floor) as f64);
                  for (r,s) in layout.ratios.iter_mut().zip(sizes) { *r = *s as f64; }
                  layout.ratios[index] = first; layout.ratios[index+1] = total as f64-first; }
                drop(state); owner.queue_resize(); glib::Propagation::Stop
            });
            let d = drag.clone();
            h.connect_button_release_event(move |h,e| { if e.button()!=1 { return glib::Propagation::Proceed; } d.borrow_mut().take(); if h.has_grab() { h.grab_remove(); } h.unset_state_flags(gtk::StateFlags::ACTIVE); glib::Propagation::Stop });
            for destroy in [false,true] {
                let d = drag.clone();
                let cancel = move |h: &gtk::EventBox| { d.borrow_mut().take(); if h.has_grab() { h.grab_remove(); } h.unset_state_flags(gtk::StateFlags::ACTIVE); };
                if destroy { h.connect_destroy(cancel); } else { h.connect_unmap(cancel); }
            }
            let weak = self.downgrade();
            h.connect_key_press_event(move |_,e| {
                let name = e.keyval().name().map(|v|v.to_string()).unwrap_or_default();
                let delta = match (vertical,name.as_str()) { (false,"Left") | (true,"Up") => -16.0, (false,"Right") | (true,"Down") => 16.0, _=>return glib::Propagation::Proceed };
                if let Some(owner)=weak.upgrade() { owner.move_handle(index,delta); } glib::Propagation::Stop
            });
            self.imp().handles.borrow_mut().push(h.clone()); h.set_parent(self);
        }
        self.sync_handles();
    }
}
