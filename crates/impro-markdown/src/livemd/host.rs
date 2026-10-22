//! `TableHost` for the live renderer: silent writes, replacement geometry and
//! history forwarding for in-document tables.
use super::{Inner, Role};
use crate::livetable::{TableBlock, TableHost};
use gtk::prelude::*;
use std::rc::Weak;

pub(super) struct Host {
    pub inner: Weak<Inner>,
    pub view: gtk::TextView,
}

impl TableHost for Host {
    fn view(&self) -> gtk::TextView {
        self.view.clone()
    }
    fn silent(&self, enter: bool) {
        if let Some(s) = self.inner.upgrade() {
            let count = s.silent.get();
            s.silent.set(if enter { count + 1 } else { count.saturating_sub(1) });
        }
    }
    fn replacement(&self, block: &TableBlock, widget: &gtk::Widget, height: i32) {
        let Some(s) = self.inner.upgrade() else {
            return;
        };
        let (index, tag, lead) = {
            let blocks = s.blocks.borrow();
            let Some(index) = blocks.find_id(block.id) else {
                return;
            };
            let deco = &blocks.items[index];
            (index, deco.lead_tag.clone(), deco.lead)
        };
        if let Some(tag) = tag {
            let wanted = height + lead;
            if tag.property::<i32>("pixels-above-lines") != wanted {
                tag.set_property("pixels-above-lines", wanted);
            }
        }
        let slot = s.attach(widget, 0, Role::Table, 0);
        if let Some(deco) = s.blocks.borrow_mut().items.get_mut(index) {
            deco.widgets.push(slot);
        }
        s.queue_layout();
    }
    fn retag(&self, block: &TableBlock) {
        let Some(s) = self.inner.upgrade() else {
            return;
        };
        let text = block.text();
        let (tag, start, hidden_tag) = {
            let mut blocks = s.blocks.borrow_mut();
            let Some(index) = blocks.find_id(block.id) else {
                return;
            };
            let start = blocks.spans[index].0;
            let deco = &mut blocks.items[index];
            deco.text = text.clone();
            (deco.lead_tag.clone(), start, s.tag("hidden"))
        };
        let Some(tag) = tag else {
            return;
        };
        let end = start + text.chars().count() as i32;
        let hidden_end = end - i32::from(text.ends_with('\n'));
        let (first, rest) = (s.iter(start), s.iter(start + 1));
        s.buffer.remove_tag(&tag, &first, &s.iter(end));
        s.buffer.apply_tag(&tag, &first, &rest);
        s.buffer.remove_tag(&hidden_tag, &first, &rest);
        if hidden_end > start + 1 {
            s.buffer.apply_tag(&hidden_tag, &rest, &s.iter(hidden_end));
        }
        let (above, below) = s.view_gaps.get();
        let pad = s.pad_tag(above + below);
        s.buffer.apply_tag(&pad, &rest, &s.iter(end));
    }
    fn queue_layout(&self) {
        if let Some(s) = self.inner.upgrade() {
            s.queue_layout();
        }
    }
    fn provide(&self, widget: &gtk::Widget) {
        if let Some(s) = self.inner.upgrade() {
            s.provide(widget);
        }
    }
    fn activate_link(&self, target: &str) {
        if let Some(s) = self.inner.upgrade() {
            s.activate_link(target);
        }
    }
    fn history(&self, redo: bool) -> bool {
        use gdk::keys::constants as k;
        let Some(window) = TextViewExt::window(&self.view, gtk::TextWindowType::Widget) else {
            return false;
        };
        let mut state = gdk::ModifierType::CONTROL_MASK;
        if redo {
            state |= gdk::ModifierType::SHIFT_MASK;
        }
        let event = gdk::Event::new(gdk::EventType::KeyPress);
        unsafe {
            use glib::translate::ToGlibPtr;
            let base: *mut gdk::ffi::GdkEvent =
                ToGlibPtr::<*mut gdk::ffi::GdkEvent>::to_glib_none(&event).0;
            let raw = base as *mut gdk::ffi::GdkEventKey;
            (*raw).window = ToGlibPtr::<*mut gdk::ffi::GdkWindow>::to_glib_full(&window);
            (*raw).send_event = 1;
            (*raw).keyval = *k::z;
            (*raw).state = state.bits();
        }
        self.view.emit_by_name::<bool>("key-press-event", &[&event])
    }
}
