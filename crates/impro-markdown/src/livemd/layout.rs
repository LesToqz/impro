//! Caret handling, widget realization and placement, details (pills and the
//! empty-line placeholder) and the document tail.
use super::{
    model::BOTTOM_MARGIN,
    structure::INTERACTIVE,
    Aim, Inner, Kind, Role, Slot, BAND,
};
use crate::{blockedit, mdview};
use gtk::prelude::*;
use std::collections::BTreeSet;

impl Inner {
    // -- caret --------------------------------------------------------------

    pub(super) fn on_mark_set(&self, mark: &gtk::TextMark) {
        if !self.enabled.get() {
            return;
        }
        if mark == &self.insert_mark {
            self.clamp_cursor();
            self.queue_caret();
        } else if mark == &self.bound_mark {
            self.queue_caret();
        }
    }

    /// Keep the caret out of hidden prefixes, fences and divider text.
    pub(super) fn clamp_cursor(&self) {
        if !self.enabled.get()
            || !self.view.is_editable()
            || self.editing.get()
            || self.clamping.get()
        {
            return;
        }
        let offset = self.caret();
        let mut first = self.iter(offset);
        first.set_line_offset(0);
        let mut last = first.clone();
        if !last.ends_line() {
            last.forward_to_line_end();
        }
        let line = self
            .buffer
            .text(&first, &last, true)
            .map(|s| s.to_string())
            .unwrap_or_default();
        let mut target = offset;
        let block = {
            let blocks = self.blocks.borrow();
            (blocks.len() > 0).then(|| {
                let index = blocks.index(offset);
                let (a, b) = blocks.spans[index];
                let deco = &blocks.items[index];
                (deco.kind, deco.closed, deco.text.clone(), a, b)
            })
        };
        let rebuilding = self.idle.borrow().is_some();
        match &block {
            Some((Kind::Codeblock, closed, text, a, b)) if !rebuilding => {
                let opening = a + text.split('\n').next().unwrap_or("").chars().count() as i32 + 1;
                let last_line = text.split_inclusive('\n').last().unwrap_or("");
                let closing = if *closed {
                    b - last_line.chars().count() as i32
                } else {
                    *b
                };
                if *a <= offset && offset < opening {
                    target = if closing > opening {
                        opening.min(closing - 1)
                    } else {
                        opening - 1
                    };
                } else if *closed
                    && closing <= offset
                    && offset <= *b
                    && (offset < *b || !text.ends_with('\n'))
                {
                    target = if closing > opening { closing - 1 } else { opening - 1 };
                }
            }
            Some((Kind::Codeblock | Kind::Table, ..)) => {}
            _ => {
                let start = first.offset();
                let end = last.offset();
                if let Some(prefix) = blockedit::prefix(&line) {
                    if offset < start + prefix.end as i32 {
                        target = start + prefix.end as i32;
                    }
                } else if mdview::_RULE.is_match(&line) && start < offset && offset < end {
                    let half = line.chars().count() as f64 / 2.0;
                    target = if f64::from(offset - start) < half { start } else { end };
                }
            }
        }
        if target != offset {
            self.clamping.set(true);
            let iter = self.iter(target);
            if self.offset_of(&self.bound_mark) != offset {
                self.buffer.move_mark(&self.insert_mark, &iter);
            } else {
                self.buffer.place_cursor(&iter);
            }
            self.clamping.set(false);
        }
    }

    pub(super) fn queue_caret(&self) {
        if self.caret_idle.borrow().is_some() || !self.enabled.get() {
            return;
        }
        let weak = self.weak.clone();
        let id = glib::idle_add_local(move || {
            if let Some(s) = weak.upgrade() {
                s.caret_idle.borrow_mut().take();
                s.caret_pass();
            }
            glib::ControlFlow::Break
        });
        *self.caret_idle.borrow_mut() = Some(id);
    }

    fn caret_pass(&self) {
        let started = std::time::Instant::now();
        self.caret_pass_inner();
        self.slow("caret", started);
    }

    fn caret_pass_inner(&self) {
        if self.idle.borrow().is_some() || self.blocks.borrow().len() == 0 || !self.enabled.get() {
            return;
        }
        self.clamp_cursor();
        let offset = self.caret();
        let char_count = self.char_count();
        let (index, id, kind, table_id) = {
            let blocks = self.blocks.borrow();
            let index = blocks.index(offset);
            let (a, b) = blocks.spans[index];
            let deco = &blocks.items[index];
            let hit = a <= offset
                && (offset < b || (offset == b && b == char_count && !deco.text.ends_with('\n')));
            (index, hit.then_some(deco.id), deco.kind, deco.id)
        };
        let mut active: BTreeSet<u64> = id.into_iter().collect();
        if !self.view.is_editable() {
            active.clear();
        }
        let tables = self.tables();
        let reveals_table = tables.caret((kind == Kind::Table).then_some(table_id), offset);
        if reveals_table {
            active.clear();
        }
        let previous = self.active.borrow().clone();
        let moved = self.last_caret.get() != Some(offset);
        if moved && kind == Kind::Line && active.iter().any(|a| previous.contains(a)) {
            self.repaint(index);
        }
        let changed: Vec<u64> = previous.symmetric_difference(&active).copied().collect();
        for current in changed {
            let found = {
                let blocks = self.blocks.borrow();
                blocks.find_id(current).map(|i| (i, blocks.items[i].kind))
            };
            if let Some((i, kind)) = found {
                if kind != Kind::Codeblock {
                    self.repaint(i);
                }
            }
        }
        *self.active.borrow_mut() = active;
        self.last_caret.set(Some(offset));
        self.queue_layout();
        self.view.queue_draw();
    }

    pub(super) fn tables(&self) -> crate::livetable::Tables {
        if let Some(tables) = self.tables.borrow().clone() {
            return tables;
        }
        let host: std::rc::Rc<dyn crate::livetable::TableHost> =
            std::rc::Rc::new(super::host::Host {
                inner: self.weak.clone(),
                view: self.view.clone(),
            });
        let tables = crate::livetable::Tables::new(host);
        *self.tables.borrow_mut() = Some(tables.clone());
        tables
    }

    // -- table cells ------------------------------------------------------------

    pub(super) fn queue_fill(&self) {
        if self.fill_idle.borrow().is_some() || !self.enabled.get() {
            return;
        }
        let weak = self.weak.clone();
        let id = glib::idle_add_local_full(glib::Priority::from(super::structure::BACKGROUND_PRIORITY), move || {
            let Some(s) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if s.fill_tables() {
                glib::ControlFlow::Continue
            } else {
                s.fill_idle.borrow_mut().take();
                s.queue_layout();
                glib::ControlFlow::Break
            }
        });
        *self.fill_idle.borrow_mut() = Some(id);
    }

    /// Build table cells (visible rows first) for one slice; true while cells remain.
    fn fill_tables(&self) -> bool {
        let Some(tables) = self.tables.borrow().clone() else {
            return false;
        };
        let started = std::time::Instant::now();
        let height = self.view.allocated_height();
        loop {
            if started.elapsed() >= super::SLICE {
                self.note_slice(started);
                return true;
            }
            let mut more = false;
            let views = tables.views();
            for view in &views {
                let top = view
                    .widget()
                    .translate_coordinates(&self.view, 0, 0)
                    .map_or(0, |(_, y)| y);
                let first = (-top).max(0) / crate::livetable::ROW_ESTIMATE;
                let last = (height - top).max(0) / crate::livetable::ROW_ESTIMATE + 1;
                more |= view.fill_visible_slice(first as usize..last as usize);
            }
            if !more {
                for view in &views {
                    more |= view.fill_slice();
                }
            }
            if !more {
                self.note_slice(started);
                return false;
            }
        }
    }

    // -- layout ---------------------------------------------------------------

    pub(super) fn column_width(&self) -> i32 {
        32.max(self.view.allocated_width() - self.view.left_margin() - self.view.right_margin() - 2)
    }

    pub(super) fn queue_layout(&self) {
        if self.layout_idle.borrow().is_some() {
            return;
        }
        let weak = self.weak.clone();
        let id = glib::idle_add_local_full(glib::Priority::LOW, move || {
            if let Some(s) = weak.upgrade() {
                s.layout_idle.borrow_mut().take();
                s.layout();
            }
            glib::ControlFlow::Break
        });
        *self.layout_idle.borrow_mut() = Some(id);
    }

    fn layout(&self) {
        let started = std::time::Instant::now();
        self.layout_inner();
        self.slow("layout", started);
    }

    fn layout_inner(&self) {
        if !self.enabled.get() {
            return;
        }
        self.size_replacements();
        self.rescale_images();
        self.sync_tail();
        self.position();
    }

    /// Size table hosts to the column and reserve their real height.
    fn size_replacements(&self) {
        let width = self.column_width();
        let tables: Vec<(usize, gtk::Widget)> = {
            let blocks = self.blocks.borrow();
            blocks
                .items
                .iter()
                .enumerate()
                .flat_map(|(i, b)| {
                    b.widgets
                        .iter()
                        .filter(|s| s.role == Role::Table)
                        .map(move |s| (i, s.widget.clone()))
                })
                .collect()
        };
        for (index, widget) in tables {
            if let Some(grid) = widget.downcast_ref::<gtk::Grid>() {
                grid.set_size_request(width, -1);
                for child in grid.children() {
                    if let Some(bin) = child.downcast_ref::<gtk::Bin>() {
                        if let Some(frame) = bin.child() {
                            frame.set_size_request(1.max(width - 2 - 25), -1);
                        }
                    }
                }
            }
            let height = widget.preferred_height_for_width(width).1;
            let (tag, lead) = {
                let blocks = self.blocks.borrow();
                match blocks.items.get(index) {
                    Some(b) => (b.lead_tag.clone(), b.lead),
                    None => (None, 0),
                }
            };
            if let Some(tag) = tag {
                let current: i32 = tag.property("pixels-above-lines");
                if current != height + lead {
                    tag.set_property("pixels-above-lines", height + lead);
                }
            }
        }
    }

    /// Space after the last block: the empty line a final newline leaves, else the bottom margin.
    fn sync_tail(&self) {
        let (above, below) = self.view_gaps.get();
        let trailing = self.buffer.end_iter().starts_line() && self.char_count() > 0;
        let last = self.blocks.borrow().items.last().map_or(0, |b| b.below);
        let want_above = above + if trailing { last } else { 0 };
        let want_bottom = BOTTOM_MARGIN + if trailing { below } else { last };
        if self.view.pixels_above_lines() != want_above {
            self.view.set_pixels_above_lines(want_above);
        }
        if self.view.bottom_margin() != want_bottom {
            self.view.set_bottom_margin(want_bottom);
        }
    }

    // -- widgets: realize near the viewport, place every frame -----------------

    /// Create or release block widgets for the current viewport band.
    fn manage_band(&self, first: usize, last: usize) {
        let (lo, hi) = self.band_bounds(first, last);
        let (far_lo, far_hi) = (first.saturating_sub(3 * BAND), last + 3 * BAND);
        let todo: Vec<(usize, bool)> = {
            let blocks = self.blocks.borrow();
            let top = blocks.len();
            let mut todo = Vec::new();
            for index in lo.min(top)..(hi + 1).min(top) {
                let deco = &blocks.items[index];
                if deco.painted && !deco.wants.is_empty() && deco.widgets.is_empty() {
                    todo.push((index, true));
                }
            }
            // Widgets far from the viewport are released, but only rescan the whole
            // list when the viewport moved far since the last scan.
            let moved = first.abs_diff(self.last_scan.get()) > BAND / 2;
            if moved || self.band.get().0 == usize::MAX {
                self.last_scan.set(first);
                self.band.set((0, 0));
                let released = |i: usize| i < far_lo || i > far_hi;
                for (index, deco) in blocks.items.iter().enumerate() {
                    if released(index) && !deco.widgets.is_empty() {
                        todo.push((index, false));
                    }
                }
            }
            todo
        };
        for (index, realize) in todo {
            if realize {
                self.realize(index);
            } else {
                self.release(index);
            }
        }
    }

    /// Per-frame placement of the realized widgets and details.
    pub(super) fn position(&self) {
        let started = std::time::Instant::now();
        self.position_inner();
        self.slow("position", started);
    }

    fn position_inner(&self) {
        if !self.enabled.get() || !self.view.is_realized() {
            return;
        }
        let Some((first, last)) = self.visible_blocks() else {
            self.position_details();
            return;
        };
        if self.unpainted.get() > 0 && !self.unpainted_in(first, last).is_empty() {
            *self.aim.borrow_mut() = Aim::default();
            self.hint.set(None);
            self.schedule(INTERACTIVE);
        }
        self.manage_band(first, last);
        let width = self.column_width();
        let visible = self.view.visible_rect();
        let (top, bottom) = (visible.y(), visible.y() + visible.height());
        let (lo, hi) = (first.saturating_sub(3 * BAND), last + 3 * BAND);
        let range: Vec<usize> = {
            let blocks = self.blocks.borrow();
            (lo..=hi.min(blocks.len().saturating_sub(1)))
                .filter(|&i| i < blocks.len() && !blocks.items[i].widgets.is_empty())
                .collect()
        };
        for index in range {
            self.place_block(index, width, top, bottom);
        }
        self.position_details();
    }

    fn unpainted_in(&self, first: usize, last: usize) -> Vec<usize> {
        let blocks = self.blocks.borrow();
        (first..=last.min(blocks.len().saturating_sub(1)))
            .filter(|&i| !blocks.items[i].painted)
            .take(1)
            .collect()
    }

    fn place_block(&self, index: usize, width: i32, top: i32, bottom: i32) {
        let (start, end, lead, below, slots, lead_tag) = {
            let blocks = self.blocks.borrow();
            let Some(deco) = blocks.items.get(index) else {
                return;
            };
            (
                blocks.spans[index].0,
                blocks.spans[index].1,
                deco.lead,
                deco.below,
                deco.widgets.clone(),
                deco.lead_tag.clone(),
            )
        };
        for slot in slots {
            let widget = &slot.widget;
            let iter = self.iter(start + slot.rel);
            let rect = self.view.iter_location(&iter);
            let (mut x, mut y) = (self.view.left_margin(), rect.y());
            let mut height = widget.preferred_height().1;
            match slot.role {
                Role::Tools => {
                    x += 1;
                    let (line_y, line_height) = self.view.line_yrange(&iter);
                    widget.set_size_request(width - 2, 28.max(line_height - lead - 1));
                    y = line_y + lead + 1;
                }
                Role::CodeCard | Role::CodeFooter | Role::CodeSideLeft | Role::CodeSideRight => {
                    let (line_y, line_height) = self.view.line_yrange(&iter);
                    let top_y = line_y + lead;
                    let tail = self.iter(start.max(end - 1));
                    let (mut bottom_y, mut bottom_height) = self.view.line_yrange(&tail);
                    if bottom_y == line_y {
                        bottom_y += lead;
                        bottom_height -= lead;
                    }
                    let tail_end = bottom_y + bottom_height + below;
                    y = top_y;
                    height = 36.max(tail_end - 8 - y);
                    if slot.role == Role::CodeFooter {
                        y = bottom_y;
                        x += 1;
                        height = 1.max(tail_end - y - 9);
                    }
                    let side = matches!(slot.role, Role::CodeSideLeft | Role::CodeSideRight);
                    if side {
                        x += if slot.role == Role::CodeSideLeft { 1 } else { width - 16 };
                        y = line_y + line_height;
                        height = 1.max(bottom_y - y);
                    }
                    widget.set_size_request(
                        if side {
                            15
                        } else {
                            width - if slot.role == Role::CodeFooter { 2 } else { 0 }
                        },
                        height,
                    );
                }
                Role::Marker => x += slot.data,
                Role::Table | Role::Image | Role::Rule => {
                    height = match slot.role {
                        Role::Table => lead_tag
                            .as_ref()
                            .map_or(0, |t| t.property::<i32>("pixels-above-lines") - lead),
                        Role::Image => widget
                            .downcast_ref::<gtk::Image>()
                            .and_then(|i| i.pixbuf())
                            .map_or(0, |p| p.height()),
                        _ => slot.data,
                    };
                    y -= height;
                    if slot.role == Role::Rule {
                        y += height / 2;
                    }
                    widget.set_size_request(
                        if slot.role != Role::Image { width } else { -1 },
                        if slot.role == Role::Rule { 1 } else { height },
                    );
                }
                Role::Quote => {
                    let (line_y, line_height) = self.view.line_yrange(&iter);
                    y = line_y + lead + 3;
                    widget.set_size_request(2, 18.max(line_height - lead + below - 6));
                    x += 4;
                }
            }
            let visible = y + height >= top && y <= bottom;
            widget.set_visible(visible);
            if visible {
                let (wx, wy) = self
                    .view
                    .buffer_to_window_coords(gtk::TextWindowType::Widget, x, y);
                let allocation = widget.allocation();
                if (allocation.x(), allocation.y()) != (wx, wy) {
                    self.view.move_child(widget, wx, wy);
                }
            }
        }
    }

    fn detail_label(&self, class: &str) -> gtk::Label {
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_no_show_all(true);
        label.style_context().add_class(class);
        label
            .style_context()
            .add_provider(&self.provider, gtk::STYLE_PROVIDER_PRIORITY_USER + 1);
        self.view
            .add_child_in_window(&label, gtk::TextWindowType::Widget, 0, 0);
        label
    }

    fn place_detail(&self, label: &gtk::Label, x: i32, y: i32) {
        let (x, y) = self
            .view
            .buffer_to_window_coords(gtk::TextWindowType::Widget, x, y);
        let allocation = label.allocation();
        if (allocation.x(), allocation.y()) != (x, y) {
            self.view.move_child(label, x, y);
        }
        label.show();
    }

    /// Reuse labels for visible inline pills; large notes never create one per span.
    fn position_details(&self) {
        let visible = self.view.visible_rect();
        let (first, _) = self.view.line_at_y(visible.y());
        let (last, _) = self.view.line_at_y(visible.y() + visible.height());
        let (begin, end) = {
            let blocks = self.blocks.borrow();
            (
                blocks.index(first.offset()),
                (blocks.index(last.offset()) + 1).min(blocks.len()),
            )
        };
        let selection = self.buffer.selection_bounds();
        let pills: Vec<(i32, i32)> = {
            let blocks = self.blocks.borrow();
            (begin..end)
                .flat_map(|i| {
                    let start = blocks.spans[i].0;
                    blocks.items[i].pills.iter().map(move |&(a, b)| (start + a, start + b))
                })
                .collect()
        };
        let mut used = 0;
        for (a, b) in pills {
            if let Some((s, e)) = &selection {
                if s.offset() < b && e.offset() > a {
                    continue;
                }
            }
            let (from, to) = (self.iter(a), self.iter(b));
            let (rect, tail) = (self.view.iter_location(&from), self.view.iter_location(&to));
            if rect.y() != tail.y()
                || rect.y() + rect.height() < visible.y()
                || rect.y() > visible.y() + visible.height()
            {
                continue;
            }
            let label = {
                let labels = self.detail_labels.borrow();
                if used == labels.len() {
                    drop(labels);
                    let label = self.detail_label("live-md-pill");
                    self.detail_labels.borrow_mut().push(label);
                    self.detail_labels.borrow()[used].clone()
                } else {
                    labels[used].clone()
                }
            };
            let text = self
                .buffer
                .text(&from, &to, true)
                .map(|s| s.to_string())
                .unwrap_or_default();
            if label.text() != text {
                label.set_text(&text);
            }
            self.place_detail(&label, rect.x() - 3, rect.y() - 1);
            used += 1;
        }
        let rest: Vec<gtk::Label> = self.detail_labels.borrow().iter().skip(used).cloned().collect();
        for label in rest {
            label.hide();
        }
        let cursor = self.iter(self.caret());
        let mut line_start = cursor.clone();
        line_start.set_line_offset(0);
        let after_block = {
            let blocks = self.blocks.borrow();
            blocks.len() > 0 && {
                let index = blocks.index(cursor.offset());
                let (_, b) = blocks.spans[index];
                let deco = &blocks.items[index];
                cursor.offset() == b
                    && deco.text.ends_with('\n')
                    && (deco.kind != Kind::Codeblock || deco.closed)
            }
        };
        let plain = {
            let blocks = self.blocks.borrow();
            blocks.len() == 0 || blocks.items[blocks.index(cursor.offset())].kind == Kind::Line
        };
        if self.view.is_editable()
            && !self.buffer.has_selection()
            && line_start.ends_line()
            && (plain || after_block)
        {
            let existing = self.placeholder.borrow().clone();
            let label = match existing {
                Some(label) => label,
                None => {
                    let label = self.detail_label("live-md-placeholder");
                    label.set_text("Type '/' for commands");
                    *self.placeholder.borrow_mut() = Some(label.clone());
                    label
                }
            };
            let rect = self.view.iter_location(&cursor);
            self.place_detail(&label, rect.x() + 4, rect.y());
        } else if let Some(label) = self.placeholder.borrow().clone() {
            label.hide();
        }
    }
}

impl Inner {
    /// A realized-widget slot for `widget` (hooked into the view, not yet shown).
    pub(super) fn attach(&self, widget: &gtk::Widget, rel: i32, role: Role, data: i32) -> Slot {
        widget.set_no_show_all(true);
        self.provide(widget);
        self.view
            .add_child_in_window(widget, gtk::TextWindowType::Widget, 0, 0);
        Slot {
            widget: widget.clone(),
            rel,
            role,
            data,
        }
    }
}
