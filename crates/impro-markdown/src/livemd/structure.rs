//! Block structure: signal wiring, dirty-range tracking, incremental rebuild,
//! the sliced work loop and the enable/disable lifecycle.
use super::{
    model::{Scanner, Step},
    Aim, Blocks, Deco, Inner, Work, BAND, SLICE,
};
use gtk::prelude::*;
use std::time::Instant;

/// Priority of work that must finish before the next frame is drawn: above the
/// redraw (120) but below input events.
pub(super) const INTERACTIVE: i32 = 110;
const BACKGROUND: i32 = 200;
pub(super) const BACKGROUND_PRIORITY: i32 = BACKGROUND;
const CHUNK: i32 = 32 * 1024;

impl Inner {
    pub(super) fn connect(&self) {
        let weak = self.weak.clone();
        let buffer = &self.buffer;
        let w = weak.clone();
        let id = buffer.connect_insert_text(move |_, at, text| {
            if let Some(s) = w.upgrade() {
                s.on_insert(at.offset(), text.chars().count() as i32);
            }
        });
        self.track(buffer, id);
        let w = weak.clone();
        let id = buffer.connect_delete_range(move |_, a, b| {
            if let Some(s) = w.upgrade() {
                s.on_delete(a.offset(), b.offset());
            }
        });
        self.track(buffer, id);
        let w = weak.clone();
        let id = buffer.connect_changed(move |_| {
            if let Some(s) = w.upgrade() {
                s.on_changed();
            }
        });
        self.track(buffer, id);
        let w = weak.clone();
        let id = buffer.connect_mark_set(move |_, _, mark| {
            if let Some(s) = w.upgrade() {
                s.on_mark_set(mark);
            }
        });
        self.track(buffer, id);
        let view = &self.view;
        let w = weak.clone();
        let id = view.connect_size_allocate(move |_, _| {
            if let Some(s) = w.upgrade() {
                s.resize();
            }
        });
        self.track(view, id);
        let w = weak.clone();
        let id = view.connect_style_updated(move |_| {
            if let Some(s) = w.upgrade() {
                s.sync_accent();
            }
        });
        self.track(view, id);
        let w = weak.clone();
        let id = view.connect_button_press_event(move |_, event| {
            let Some(s) = w.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if s.press(event) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.track(view, id);
        let w = weak.clone();
        let id = view.connect_key_press_event(move |_, event| {
            let Some(s) = w.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if s.control_key(event) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.track(view, id);
        let w = weak.clone();
        let id = view.connect_copy_clipboard(move |v| {
            if let Some(s) = w.upgrade() {
                s.copy_selection(v);
            }
        });
        self.track(view, id);
        let w = weak.clone();
        let id = view.connect_cut_clipboard(move |v| {
            if let Some(s) = w.upgrade() {
                s.cut_selection(v);
            }
        });
        self.track(view, id);
        let w = weak.clone();
        let id = view.connect_motion_notify_event(move |_, event| {
            if let Some(s) = w.upgrade() {
                let (x, y) = event.position();
                s.motion(x, y);
            }
            glib::Propagation::Proceed
        });
        self.track(view, id);
        let w = weak.clone();
        let id = view.connect_leave_notify_event(move |_, event| {
            if let Some(s) = w.upgrade() {
                s.leave(event.detail() == gdk::NotifyType::Inferior);
            }
            glib::Propagation::Proceed
        });
        self.track(view, id);
        let w = weak.clone();
        let id = view.connect_realize(move |_| {
            if let Some(s) = w.upgrade() {
                s.on_realize();
            }
        });
        self.track(view, id);
        let w = weak.clone();
        let id = view.connect_unrealize(move |_| {
            if let Some(s) = w.upgrade() {
                s.on_unrealize();
            }
        });
        self.track(view, id);
        let w = weak.clone();
        let id = view.connect_destroy(move |_| {
            if let Some(s) = w.upgrade() {
                s.destroy();
            }
        });
        self.track(view, id);
        view.add_events(gdk::EventMask::POINTER_MOTION_MASK | gdk::EventMask::LEAVE_NOTIFY_MASK);
        if view.is_realized() {
            self.on_realize();
        }
    }

    pub(super) fn on_realize(&self) {
        self.on_unrealize();
        let Some(clock) = self.view.frame_clock() else {
            return;
        };
        let weak = self.weak.clone();
        let id = clock.connect_after_paint(move |_| {
            if let Some(s) = weak.upgrade() {
                s.position();
            }
        });
        *self.frame.borrow_mut() = Some((clock, id));
    }
    pub(super) fn on_unrealize(&self) {
        let frame = self.frame.borrow_mut().take();
        if let Some((clock, id)) = frame {
            clock.disconnect(id);
        }
    }

    // -- lifecycle --------------------------------------------------------

    pub(super) fn set_enabled(&self, enabled: bool) {
        if self.enabled.get() == enabled {
            return;
        }
        self.enabled.set(enabled);
        let context = self.view.style_context();
        if enabled {
            context.add_class("live-markdown");
            context.add_provider(&self.provider, gtk::STYLE_PROVIDER_PRIORITY_USER + 1);
            self.view.set_wrap_mode(gtk::WrapMode::WordChar);
            self.view_gaps.set((
                self.view.pixels_above_lines(),
                self.view.pixels_below_lines(),
            ));
            self.view.set_pixels_below_lines(0);
            self.view.set_top_margin(24);
            self.view.set_bottom_margin(super::model::BOTTOM_MARGIN);
            self.resize();
            self.restart_all();
        } else {
            self.cancel();
            self.clear();
            context.remove_class("live-markdown");
            context.remove_provider(&self.provider);
            self.view.set_wrap_mode(gtk::WrapMode::None);
            self.view.set_left_margin(0);
            self.view.set_right_margin(12);
            let (above, below) = self.view_gaps.get();
            self.view.set_pixels_above_lines(above);
            self.view.set_pixels_below_lines(below);
            self.view.set_top_margin(0);
            self.view.set_bottom_margin(0);
        }
    }

    pub(super) fn resize(&self) {
        if !self.enabled.get() {
            return;
        }
        let width = self.view.allocated_width();
        let margin = if self.readable_width.get() {
            32.max((width - super::COLUMN + 1) / 2)
        } else {
            32
        };
        let changed = self.view.left_margin() != margin;
        self.view.set_left_margin(margin);
        self.view.set_right_margin(margin);
        if self.margin.replace(margin) == margin {
            let widgets = self.blocks.borrow().items.iter().any(|b| !b.widgets.is_empty());
            if changed || widgets {
                self.queue_layout();
            }
            return;
        }
        let indents: Vec<(i32, gtk::TextTag)> = self
            .tags
            .borrow()
            .iter()
            .filter_map(|(name, tag)| {
                let inset = name.strip_prefix("indent-")?.split('-').next()?.parse().ok()?;
                Some((inset, tag.clone()))
            })
            .collect();
        for (inset, tag) in indents {
            tag.set_property("left-margin", margin + inset);
            tag.set_property("right-margin", margin + if inset == 16 { 16 } else { 0 });
        }
        let widgets = self.blocks.borrow().items.iter().any(|b| !b.widgets.is_empty());
        if changed || widgets {
            self.queue_layout();
        }
    }

    fn restart_all(&self) {
        self.cancel();
        self.clear();
        self.invalidate_all();
        self.on_changed();
    }

    /// Drop queued work and the dirty range.
    pub(super) fn cancel(&self) {
        let idle = self.idle.borrow_mut().take();
        if let Some((id, _)) = idle {
            id.remove();
        }
        let id = self.caret_idle.borrow_mut().take();
        if let Some(id) = id {
            id.remove();
        }
        let id = self.layout_idle.borrow_mut().take();
        if let Some(id) = id {
            id.remove();
        }
        let id = self.fill_idle.borrow_mut().take();
        if let Some(id) = id {
            id.remove();
        }
        self.work.replace(None);
        self.dirty.set(None);
    }

    /// Release every decoration: tags, widgets, tables, details.
    pub(super) fn clear(&self) {
        let popover = self.language_popover.borrow_mut().take();
        if let Some(popover) = popover {
            unsafe { popover.destroy() };
        }
        let tables = self.tables.borrow_mut().take();
        if let Some(tables) = tables {
            tables.destroy();
        }
        let blocks = std::mem::take(&mut *self.blocks.borrow_mut());
        let (start, end) = (self.buffer.start_iter(), self.buffer.end_iter());
        let tags: Vec<_> = self.tags.borrow().values().cloned().collect();
        for tag in &tags {
            self.buffer.remove_tag(tag, &start, &end);
        }
        let table = self.buffer.tag_table();
        for block in blocks.items {
            for tag in &block.tags {
                if let Some(table) = &table {
                    table.remove(tag);
                }
            }
            if let Some(marks) = &block.table {
                marks.delete_marks(&self.buffer);
            }
            for slot in &block.widgets {
                unsafe { slot.widget.destroy() };
            }
        }
        self.hot.set(0);
        self.unpainted.set(0);
        self.active.borrow_mut().clear();
        self.hover_tools.replace(None);
        self.aim.replace(Aim::default());
        let labels = std::mem::take(&mut *self.detail_labels.borrow_mut());
        for label in labels {
            unsafe { label.destroy() };
        }
        let placeholder = self.placeholder.borrow_mut().take();
        if let Some(placeholder) = placeholder {
            unsafe { placeholder.destroy() };
        }
        for id in std::mem::take(&mut *self.feedback.borrow_mut()) {
            id.remove();
        }
        self.jobs.cancel();
        self.bump();
    }

    pub(super) fn destroy(&self) {
        if self.destroyed.replace(true) {
            return;
        }
        self.set_enabled(false);
        self.cancel();
        self.on_unrealize();
        let handlers = std::mem::take(&mut *self.handlers.borrow_mut());
        for (object, id) in handlers {
            object.disconnect(id);
        }
        self.jobs.close();
        let tags: Vec<_> = self.tags.borrow_mut().drain().map(|(_, t)| t).collect();
        if let Some(table) = self.buffer.tag_table() {
            for tag in tags {
                table.remove(&tag);
            }
        }
        let toolbar = self.toolbar.borrow_mut().take();
        if let Some(toolbar) = toolbar {
            toolbar.destroy();
        }
        super::unregister(&self.view);
    }

    pub(super) fn bump(&self) {
        self.epoch.set(self.epoch.get() + 1);
        self.aim.borrow_mut().valid = false;
    }

    // -- dirty range --------------------------------------------------------

    fn invalidate_all(&self) {
        self.dirty.set(Some((0, self.char_count())));
    }
    fn merge_dirty(&self, start: i32, end: i32) {
        let (start, end) = match self.dirty.get() {
            Some((s, e)) => (start.min(s), end.max(e)),
            None => (start, end),
        };
        self.dirty.set(Some((start, end)));
    }

    pub(super) fn on_insert(&self, at: i32, n: i32) {
        if !self.enabled.get() {
            return;
        }
        let (left, right) = {
            let blocks = self.blocks.borrow();
            if blocks.len() == 0 {
                (None, None)
            } else {
                let index = blocks.index(at);
                (
                    Some(blocks.spans[index.saturating_sub(1)].0),
                    Some(blocks.len().min(index + 2) - 1),
                )
            }
        };
        self.blocks.borrow_mut().shift_insert(at, n);
        if let Some((s, e)) = self.dirty.get() {
            let e = if e >= at { e + n } else { e };
            let s = if s > at { s + n } else { s };
            self.dirty.set(Some((s, e)));
        }
        if self.silent.get() > 0 {
            return;
        }
        let end = right
            .map(|r| self.blocks.borrow().spans[r].1)
            .unwrap_or(at + n)
            .max(at + n);
        self.merge_dirty(left.unwrap_or(at).min(at), end);
    }

    pub(super) fn on_delete(&self, a: i32, b: i32) {
        if !self.enabled.get() {
            return;
        }
        let (left, right) = {
            let blocks = self.blocks.borrow();
            if blocks.len() == 0 {
                (None, None)
            } else {
                let index = blocks.index(a);
                let last = blocks.index(b);
                (
                    Some(blocks.spans[index.saturating_sub(1)].0),
                    Some(blocks.len().min(last + 2) - 1),
                )
            }
        };
        self.blocks.borrow_mut().shift_delete(a, b);
        if let Some((s, e)) = self.dirty.get() {
            let n = b - a;
            let map = |x: i32| {
                if x <= a {
                    x
                } else if x >= b {
                    x - n
                } else {
                    a
                }
            };
            self.dirty.set(Some((map(s), map(e))));
        }
        if self.silent.get() > 0 {
            return;
        }
        let end = right
            .map(|r| self.blocks.borrow().spans[r].1)
            .unwrap_or(a)
            .max(a);
        self.merge_dirty(left.unwrap_or(a).min(a), end);
    }

    pub(super) fn on_changed(&self) {
        if !self.enabled.get() || self.silent.get() > 0 {
            return;
        }
        if self.dirty.get().is_none() {
            self.invalidate_all();
        }
        self.work.replace(None);
        self.bump();
        let (start, end) = self.dirty.get().unwrap_or((0, 0));
        let priority = if end - start <= 2048 && self.blocks.borrow().len() > 0 {
            INTERACTIVE
        } else {
            BACKGROUND
        };
        self.schedule(priority);
    }

    // -- scheduling -----------------------------------------------------------

    pub(super) fn schedule(&self, priority: i32) {
        let old = {
            let mut idle = self.idle.borrow_mut();
            match idle.as_ref() {
                Some((_, p)) if *p <= priority => return,
                _ => idle.take(),
            }
        };
        if let Some((id, _)) = old {
            id.remove();
        }
        let weak = self.weak.clone();
        let id = glib::idle_add_local_full(glib::Priority::from(priority), move || match weak
            .upgrade()
        {
            Some(s) => s.step(),
            None => glib::ControlFlow::Break,
        });
        *self.idle.borrow_mut() = Some((id, priority));
    }

    /// One slice of decoration work: structure first (cheap), then paint blocks
    /// nearest the viewport until the slice budget is spent.
    fn step(&self) -> glib::ControlFlow {
        let started = Instant::now();
        let mut done = false;
        if !self.enabled.get() {
            self.idle.borrow_mut().take();
            return glib::ControlFlow::Break;
        }
        loop {
            if started.elapsed() >= SLICE {
                break;
            }
            if self.work.borrow().is_some() || self.dirty.get().is_some() {
                self.rebuild_unit();
                continue;
            }
            match self.next_unpainted() {
                Some(index) => self.paint(index),
                None => {
                    done = true;
                    break;
                }
            }
        }
        self.note_slice(started);
        if done {
            self.idle.borrow_mut().take();
            self.queue_caret();
            self.queue_layout();
            return glib::ControlFlow::Break;
        }
        let priority = self.idle.borrow().as_ref().map_or(BACKGROUND, |(_, p)| *p);
        if priority < BACKGROUND {
            self.idle.borrow_mut().take();
            self.schedule(BACKGROUND);
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    }

    // -- rebuild --------------------------------------------------------------

    fn start_work(&self) -> Option<Work> {
        let (start, stop) = self.dirty.get()?;
        let mut start = start.clamp(0, self.char_count());
        let mut iter = self.iter(start);
        if !iter.starts_line() {
            iter.set_line_offset(0);
            start = iter.offset();
        }
        let blocks = self.blocks.borrow();
        let mut index = if blocks.len() > 0 { blocks.index(start) } else { 0 };
        while index > 0 && blocks.spans[index - 1].0 == start {
            index -= 1;
        }
        if blocks.len() > 0 {
            let (s, e) = blocks.spans[index];
            if s < start && e <= start {
                index += 1;
            }
        }
        Some(Work {
            stop,
            index,
            next: start,
            chunk: String::new(),
            chunk_base: start,
            byte: 0,
            complete: false,
            size: CHUNK as usize,
        })
    }

    fn fetch(&self, work: &mut Work, from: i32) {
        let total = self.char_count();
        let from = from.clamp(0, total);
        let end = (from + work.size as i32).min(total);
        work.chunk = self
            .buffer
            .slice(&self.iter(from), &self.iter(end), true)
            .map(|s| s.to_string())
            .unwrap_or_default();
        work.chunk_base = from;
        work.next = from;
        work.byte = 0;
        work.complete = end >= total;
    }

    /// Scan and install one block (or finish). Returns whether work remains.
    fn rebuild_unit(&self) -> bool {
        let taken = self.work.borrow_mut().take();
        let mut work = match taken {
            Some(w) => w,
            None => match self.start_work() {
                Some(mut w) => {
                    let from = w.next;
                    self.fetch(&mut w, from);
                    w
                }
                None => return false,
            },
        };
        let mut guard = 0;
        loop {
            guard += 1;
            if guard > 64 {
                // Defensive: never spin on a scanner that cannot make progress.
                self.finish_rebuild();
                return false;
            }
            let step = {
                let mut scanner = Scanner::new(&work.chunk[work.byte..], work.next, work.complete);
                let step = scanner.next_step();
                (step, scanner.byte())
            };
            match step {
                (Step::Block(found), used) => {
                    let resynced = self.install(&mut work, found);
                    work.byte += used;
                    if resynced {
                        self.finish_rebuild();
                        return false;
                    }
                    work.next = self.blocks.borrow().spans[work.index - 1].1;
                    *self.work.borrow_mut() = Some(work);
                    return true;
                }
                (Step::More { from }, _) => {
                    work.size = (work.size * 2).min(8 << 20);
                    self.fetch(&mut work, from);
                }
                (Step::Done, _) => {
                    if work.complete {
                        let removed = {
                            let mut blocks = self.blocks.borrow_mut();
                            let mut removed = Vec::new();
                            while work.index < blocks.len() {
                                let span = blocks.spans[work.index];
                                removed.push((span, blocks.remove(work.index)));
                            }
                            removed
                        };
                        for (span, deco) in removed {
                            self.dispose(deco, span);
                        }
                        self.finish_rebuild();
                        return false;
                    }
                    let from = work.next;
                    self.fetch(&mut work, from);
                }
            }
        }
    }

    fn finish_rebuild(&self) {
        self.work.replace(None);
        let hint = self
            .dirty
            .take()
            .filter(|&(s, e)| e - s <= 4096)
            .map(|(s, _)| s);
        self.hint.set(hint);
        self.bump();
    }

    /// Replace the blocks `found` overlaps. Returns true when the old block at
    /// this position is identical and lies past the dirty range.
    fn install(&self, work: &mut Work, found: super::model::Scanned) -> bool {
        let removed = {
            let mut blocks = self.blocks.borrow_mut();
            if work.index < blocks.len() {
                let (s, e) = blocks.spans[work.index];
                let old = &blocks.items[work.index];
                if s == found.start
                    && e == found.end
                    && old.kind == found.kind
                    && old.text == found.text
                    && found.start >= work.stop
                {
                    return true;
                }
            }
            let mut removed = Vec::new();
            while work.index < blocks.len() && blocks.spans[work.index].0 < found.end {
                let span = blocks.spans[work.index];
                removed.push((span, blocks.remove(work.index)));
            }
            let deco = Box::new(Deco::new(self.fresh_id(), found.kind, found.text));
            blocks.insert(work.index, (found.start, found.end), deco);
            work.index += 1;
            removed
        };
        self.unpainted.set(self.unpainted.get() + 1);
        for (span, deco) in removed {
            self.dispose(deco, span);
        }
        false
    }

    /// Remove a block's tags and widgets (it is already out of the list).
    pub(super) fn dispose(&self, deco: Box<Deco>, span: (i32, i32)) {
        if !deco.painted {
            self.unpainted.set(self.unpainted.get().saturating_sub(1));
        }
        self.clear_paint_of(&deco, span);
        if let Some(marks) = &deco.table {
            marks.delete_marks(&self.buffer);
        }
    }

    // -- painting order --------------------------------------------------------

    /// Index of the nearest unpainted block, working outward from the focus.
    fn next_unpainted(&self) -> Option<usize> {
        if self.unpainted.get() == 0 {
            return None;
        }
        let valid = self.aim.borrow().valid;
        if !valid {
            let focus = self.focus_offset();
            let blocks = self.blocks.borrow();
            if blocks.len() == 0 {
                return None;
            }
            let focus = blocks.index(focus);
            *self.aim.borrow_mut() = Aim {
                focus,
                down: focus,
                up: focus as isize - 1,
                flip: false,
                valid: true,
            };
        }
        let blocks = self.blocks.borrow();
        let mut aim = self.aim.borrow_mut();
        for _ in 0..2 {
            let prefer_down = !aim.flip;
            aim.flip = !aim.flip;
            if prefer_down {
                while aim.down < blocks.len() {
                    let index = aim.down;
                    aim.down += 1;
                    if !blocks.items[index].painted {
                        return Some(index);
                    }
                }
            } else {
                while aim.up >= 0 {
                    let index = aim.up as usize;
                    aim.up -= 1;
                    if !blocks.items[index].painted {
                        return Some(index);
                    }
                }
            }
        }
        // Both sides are exhausted but the counter disagrees: rescan once.
        aim.valid = false;
        drop(aim);
        let stray = blocks.items.iter().position(|b| !b.painted);
        if stray.is_none() {
            self.unpainted.set(0);
        }
        stray
    }

    /// Offset painting should favour: a fresh edit, else the viewport centre,
    /// else the caret.
    fn focus_offset(&self) -> i32 {
        if let Some(hint) = self.hint.take() {
            return hint;
        }
        if self.view.is_mapped() {
            let visible = self.view.visible_rect();
            if visible.height() > 0 {
                let (iter, _) = self.view.line_at_y(visible.y() + visible.height() / 2);
                return iter.offset();
            }
        }
        self.caret()
    }

    /// Block index range `[first, last]` showing in the viewport (None when unmapped).
    pub(super) fn visible_blocks(&self) -> Option<(usize, usize)> {
        if !self.view.is_realized() {
            return None;
        }
        let visible = self.view.visible_rect();
        let (first, _) = self.view.line_at_y(visible.y());
        let (last, _) = self.view.line_at_y(visible.y() + visible.height());
        let blocks = self.blocks.borrow();
        (blocks.len() > 0).then(|| (blocks.index(first.offset()), blocks.index(last.offset())))
    }

    pub(super) fn band_bounds(&self, first: usize, last: usize) -> (usize, usize) {
        (first.saturating_sub(BAND), last + BAND)
    }
}

impl Blocks {
    #[allow(dead_code)]
    pub fn total_text(&self) -> String {
        self.items.iter().map(|b| b.text.as_str()).collect()
    }
}
