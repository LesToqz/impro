//! Painting a block: tags, vertical rhythm (`below`/`lead`/`pad`) and the list
//! of widgets the block wants. Widgets themselves are realized by `layout`.
use super::{
    model::{
        fence_closed, fence_language, plan_line, LinkSpan, Op, Want, BELOW_CODE_END,
        BELOW_CODE_LINE, BELOW_REPLACEMENT,
    },
    style::above_of,
    CodeInfo, Deco, ImageSlot, ImageState, Inner, Kind,
};
use crate::livetable;
use gtk::prelude::*;

/// What painting produced for one block, committed after all GTK calls.
#[derive(Default)]
pub(super) struct Out {
    pub applied: Vec<gtk::TextTag>,
    pub tags: Vec<gtk::TextTag>,
    pub lead_tag: Option<gtk::TextTag>,
    pub spacing: Option<&'static str>,
    pub below: i32,
    pub links: Vec<LinkSpan>,
    pub pills: Vec<(i32, i32)>,
    pub wants: Vec<(i32, Want)>,
    pub list_level: usize,
    pub number: usize,
    pub closed: bool,
    pub code: Option<CodeInfo>,
    pub table: Option<livetable::TableBlock>,
    pub images: Vec<ImageSlot>,
}
impl Out {
    fn mark(&mut self, tag: &gtk::TextTag) {
        if !self.applied.iter().any(|t| t == tag) {
            self.applied.push(tag.clone());
        }
    }
}

impl Inner {
    fn apply_named(&self, out: &mut Out, name: &str, a: i32, b: i32) {
        if b > a {
            if name == "checked" && std::env::var_os("DBG_SKIP").is_some() {
                return;
            }
            let t = std::time::Instant::now();
            let tag = self.tag(name);
            self.slow(&format!("tag {name}"), t);
            let t = std::time::Instant::now();
            self.apply_tag(out, &tag, a, b);
            self.slow(&format!("apply {name}"), t);
        }
    }
    pub(super) fn apply_tag(&self, out: &mut Out, tag: &gtk::TextTag, a: i32, b: i32) {
        if b > a {
            let t = std::time::Instant::now();
            let (ia, ib) = (self.iter(a), self.iter(b));
            self.slow("iters", t);
            let t = std::time::Instant::now();
            self.buffer.apply_tag(tag, &ia, &ib);
            self.slow(&format!("gtk-apply {a} {b} {}", self.char_count()), t);
            out.mark(tag);
        }
    }

    /// Per-block indent tag (`indent-<inset>-<hanging>`).
    fn indent(&self, out: &mut Out, kind: Kind, a: i32, b: i32, inset: i32, hanging: i32) {
        let name = format!("indent-{inset}-{hanging}");
        let fresh = !self.tags.borrow().contains_key(&name);
        let tag = self.tag(&name);
        if fresh {
            tag.set_property("left-margin", self.view.left_margin() + inset);
            tag.set_property(
                "right-margin",
                self.view.right_margin() + if kind == Kind::Codeblock { 16 } else { 0 },
            );
            tag.set_property("indent", -hanging);
        }
        self.apply_tag(out, &tag, a, b);
    }

    /// Paint block `index` (no-op when it is gone or already painted).
    pub(super) fn paint(&self, index: usize) {
        let snapshot = {
            let blocks = self.blocks.borrow();
            let Some(deco) = blocks.items.get(index) else {
                return;
            };
            if deco.painted {
                return;
            }
            let (a, b) = blocks.spans[index];
            let lead = if index > 0 {
                blocks.items[index - 1].below
            } else {
                0
            };
            (deco.id, deco.kind, deco.text.clone(), a, b, lead)
        };
        let (id, kind, text, a, b, lead) = snapshot;
        let started = std::time::Instant::now();
        self.painted_blocks.set(self.painted_blocks.get() + 1);
        let char_count = self.char_count();
        let cursor = self.caret();
        let editable = self.view.is_editable();
        let active = editable
            && ((a <= cursor && cursor < b)
                || (b == cursor && cursor == char_count && !text.ends_with('\n')));
        let mut out = Out::default();
        let t0 = std::time::Instant::now();
        match kind {
            Kind::Codeblock => self.paint_code(&mut out, &text, a, b),
            Kind::Table => {
                self.paint_table(&mut out, id, &text, a, b, lead);
            }
            Kind::Line => self.paint_line(&mut out, index, &text, a, b, lead, active, editable),
        }
        if active && kind == Kind::Line {
            // The paint already reflects this caret: no second pass in the caret idle.
            self.last_caret.set(Some(cursor));
        }
        self.slow("paint-body", t0);
        let t1 = std::time::Instant::now();
        self.commit(index, id, a, lead, active && kind != Kind::Table, out);
        self.slow("paint-commit", t1);
        if started.elapsed() > super::SLICE {
            self.note_slice(started);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_line(
        &self,
        out: &mut Out,
        index: usize,
        text: &str,
        a: i32,
        b: i32,
        lead: i32,
        active: bool,
        editable: bool,
    ) {
        let caret = editable.then(|| self.caret());
        let tp = std::time::Instant::now();
        let plan = {
            let blocks = self.blocks.borrow();
            let previous = || {
                (0..index).rev().map(|i| {
                    let item = &blocks.items[i];
                    (item.text.as_str(), item.list_level, item.number)
                })
            };
            plan_line(text, a, active, caret, previous)
        };
        self.slow("plan_line", tp);
        let tp = std::time::Instant::now();
        for op in &plan.ops {
            match op {
                Op::Tag(name, x, y) => self.apply_named(out, name, *x, *y),
                Op::Hidden(x, y) => self.apply_named(out, "hidden", *x, *y),
            }
        }
        self.slow("apply_ops", tp);
        if let Some((inset, hanging)) = plan.indent {
            self.indent(out, Kind::Line, a, b, inset, hanging);
        }
        out.spacing = plan.spacing;
        out.below = plan.below;
        out.links = plan.links;
        out.pills = plan.pills;
        out.list_level = plan.list_level;
        out.number = plan.number;
        out.wants = plan.wants.clone();
        if let Some(height) = plan.replacement {
            self.replacement(out, a, b, text, height, lead, false);
        }
        // Images that already loaded keep their pixels across repaints.
        let previous = {
            let blocks = self.blocks.borrow();
            blocks
                .items
                .get(index)
                .map(|d| {
                    d.images
                        .iter()
                        .filter_map(|i| match &i.state {
                            ImageState::Ready(loaded) => Some((i.target.clone(), loaded.clone())),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        for (rel, want) in &plan.wants {
            if let Want::Image { target, .. } = want {
                let state = previous
                    .iter()
                    .find(|(t, _)| t == target)
                    .map_or(ImageState::Pending, |(_, l)| ImageState::Ready(l.clone()));
                out.images.push(ImageSlot {
                    rel_a: *rel,
                    rel_b: 0,
                    target: target.clone(),
                    state,
                    tag: None,
                });
            }
        }
        // Reapply the reservation of already-decoded images.
        let ready: Vec<usize> = out
            .images
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.state, ImageState::Ready(_)))
            .map(|(i, _)| i)
            .collect();
        for slot in ready {
            self.reserve_image(out, slot, a, lead);
        }
    }

    /// The `hidden`/size/gap tags that stand in for a widget-covered block.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn replacement(
        &self,
        out: &mut Out,
        a: i32,
        b: i32,
        text: &str,
        height: i32,
        lead: i32,
        table: bool,
    ) {
        let hidden = self.tag("hidden");
        let end = b - i32::from(text.ends_with('\n'));
        if end > a + 1 {
            self.buffer
                .apply_tag(&hidden, &self.iter(a + 1), &self.iter(end));
            out.mark(&hidden);
        }
        let tag = gtk::TextTag::new(None);
        tag.set_property("size-points", 1.0f64);
        tag.set_property(
            "foreground",
            self.style
                .borrow()
                .chrome
                .get("gutter_bg")
                .cloned()
                .unwrap_or_default()
                .as_str(),
        );
        tag.set_property("pixels-above-lines", height + lead);
        tag.set_property("pixels-below-lines", 0);
        if let Some(t) = self.buffer.tag_table() {
            t.add(&tag);
        }
        out.tags.push(tag.clone());
        out.spacing = None;
        out.lead_tag = Some(tag.clone());
        out.below = BELOW_REPLACEMENT;
        if table {
            let pad = self.pad_tag(self.view_gaps.get().0 + self.view_gaps.get().1);
            self.apply_tag(out, &pad, a + 1, b);
        }
        self.buffer
            .apply_tag(&tag, &self.iter(a), &self.iter(a + 1));
    }

    fn paint_table(&self, out: &mut Out, id: u64, text: &str, a: i32, b: i32, lead: i32) {
        let estimate = livetable::Table::new(text).map_or(livetable::ROW_ESTIMATE, |t| {
            t.rows.len() as i32 * livetable::ROW_ESTIMATE
                + 2
                + if self.view.is_editable() {
                    livetable::REACH
                } else {
                    0
                }
        });
        self.replacement(out, a, b, text, estimate, lead, true);
        out.table = Some(livetable::TableBlock::new(id, &self.buffer, a, b, text));
        out.wants.push((0, Want::Table));
    }

    fn paint_code(&self, out: &mut Out, text: &str, a: i32, b: i32) {
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let first = lines.first().copied().unwrap_or("");
        let last = lines.last().copied().unwrap_or("");
        let language = fence_language(first);
        let closed = fence_closed(first, last, lines.len());
        out.closed = closed;
        let first_len = first.chars().count() as i32;
        let last_len = last.chars().count() as i32;
        let start = a + first_len;
        let end = if closed { b - last_len } else { b };
        self.apply_named(out, "codeblock", a, b);
        self.indent(out, Kind::Codeblock, a, b, 16, 0);
        let hidden = self.tag("hidden");
        self.apply_tag(
            out,
            &hidden,
            a,
            a.max(start - i32::from(first.ends_with('\n'))),
        );
        if closed {
            self.apply_tag(out, &hidden, end, b - i32::from(text.ends_with('\n')));
        }
        self.apply_named(out, "code-opening", a, start);
        let interior = if closed {
            lines.len() as i32 - 2
        } else {
            lines.len() as i32 - 1
        };
        out.spacing = Some("code-opening");
        if interior > 1 {
            let second = lines[1].chars().count() as i32;
            let pad = self.pad_tag(2 * BELOW_CODE_LINE);
            self.apply_tag(out, &pad, start + second, end);
        }
        if closed {
            self.apply_named(out, "code-closing", end, b);
            if interior > 0 {
                let pad = self.pad_tag(12 + BELOW_CODE_LINE);
                self.apply_tag(out, &pad, end, end + 1);
            }
        }
        out.below = if closed {
            BELOW_CODE_END
        } else if interior > 0 {
            BELOW_CODE_LINE
        } else {
            0
        };
        out.code = Some(CodeInfo {
            language,
            first_len,
            end_rel: end - a,
            highlighted: None,
        });
        out.wants.push((0, Want::Code));
    }

    /// Store the result of painting and settle the neighbours' rhythm.
    fn commit(&self, index: usize, id: u64, a: i32, lead: i32, active: bool, mut out: Out) {
        let mut pad_tag = None;
        {
            let mut blocks = self.blocks.borrow_mut();
            let current = blocks.items.get_mut(index).filter(|d| d.id == id);
            let Some(deco) = current else {
                drop(blocks);
                // The structure changed while painting: undo this paint.
                self.discard(out, a);
                return;
            };
            deco.lead = lead;
            deco.below = out.below;
            deco.spacing = out.spacing;
            deco.lead_tag = out.lead_tag.take();
            deco.tags.append(&mut out.tags);
            deco.applied = std::mem::take(&mut out.applied);
            deco.links = std::mem::take(&mut out.links);
            deco.pills = std::mem::take(&mut out.pills);
            deco.wants = std::mem::take(&mut out.wants);
            deco.list_level = out.list_level;
            deco.number = out.number;
            deco.closed = out.closed;
            deco.code = out.code.take();
            deco.table = out.table.take().or(deco.table.take());
            deco.images = std::mem::take(&mut out.images);
            deco.painted = true;
            deco.pad = None;
            if deco.lead_tag.is_none() && lead != 0 {
                if let Some(name) = deco.spacing {
                    pad_tag = Some(above_of(name) + lead);
                }
            }
            deco.hot = !deco.links.is_empty() || matches!(deco.kind, Kind::Codeblock);
            if deco.hot {
                self.hot.set(self.hot.get() + 1);
            }
        }
        if active {
            self.active.borrow_mut().insert(id);
        }
        self.unpainted.set(self.unpainted.get().saturating_sub(1));
        if let Some(above) = pad_tag {
            let tag = self.pad_tag(above);
            self.buffer
                .apply_tag(&tag, &self.iter(a), &self.iter(a + 1));
            let mut blocks = self.blocks.borrow_mut();
            if let Some(deco) = blocks.items.get_mut(index) {
                deco.pad = Some(tag.clone());
                if !deco.applied.contains(&tag) {
                    deco.applied.push(tag);
                }
            }
        }
        let next = {
            let blocks = self.blocks.borrow();
            (index + 1 < blocks.len()).then(|| blocks.items[index].below)
        };
        if let Some(below) = next {
            self.relead(index + 1, below);
        }
    }

    /// The block before `index` now leaves `lead` pixels below it.
    pub(super) fn relead(&self, index: usize, lead: i32) {
        let (old, lead_tag, painted, spacing, span) = {
            let mut blocks = self.blocks.borrow_mut();
            let span = blocks.spans[index];
            let deco = &mut blocks.items[index];
            if deco.lead == lead {
                return;
            }
            let old = deco.lead;
            deco.lead = lead;
            (old, deco.lead_tag.clone(), deco.painted, deco.spacing, span)
        };
        if let Some(tag) = lead_tag {
            let above: i32 = tag.property("pixels-above-lines");
            tag.set_property("pixels-above-lines", above - old + lead);
            return;
        }
        if !painted {
            return;
        }
        // Replace the pad tag on the first character.
        let old_pad = self.blocks.borrow_mut().items[index].pad.take();
        if let Some(pad) = old_pad {
            self.buffer
                .remove_tag(&pad, &self.iter(span.0), &self.iter(span.0 + 1));
        }
        if let (Some(name), true) = (spacing, lead != 0) {
            let tag = self.pad_tag(above_of(name) + lead);
            self.buffer
                .apply_tag(&tag, &self.iter(span.0), &self.iter(span.0 + 1));
            let mut blocks = self.blocks.borrow_mut();
            let deco = &mut blocks.items[index];
            deco.pad = Some(tag.clone());
            if !deco.applied.contains(&tag) {
                deco.applied.push(tag);
            }
        }
    }

    /// Remove what a paint applied when its block vanished mid-paint.
    fn discard(&self, out: Out, a: i32) {
        let _ = a;
        let end = self.buffer.end_iter();
        for tag in &out.applied {
            self.buffer.remove_tag(tag, &self.buffer.start_iter(), &end);
        }
        if let Some(table) = self.buffer.tag_table() {
            for tag in &out.tags {
                table.remove(tag);
            }
        }
        if let Some(marks) = &out.table {
            marks.delete_marks(&self.buffer);
        }
    }

    /// Strip a block's decoration but keep it in the list (it will be painted again).
    pub(super) fn unpaint(&self, index: usize) {
        let (deco_parts, span) = {
            let mut blocks = self.blocks.borrow_mut();
            let span = blocks.spans[index];
            let deco = &mut blocks.items[index];
            if !deco.painted {
                return;
            }
            let parts = (
                std::mem::take(&mut deco.applied),
                std::mem::take(&mut deco.tags),
                std::mem::take(&mut deco.widgets),
                deco.hot,
                deco.id,
                deco.table.take(),
            );
            deco.painted = false;
            deco.hot = false;
            deco.pad = None;
            deco.lead_tag = None;
            deco.links.clear();
            deco.pills.clear();
            deco.wants.clear();
            deco.spacing = None;
            deco.code = None;
            for image in &mut deco.images {
                image.tag = None;
            }
            (parts, span)
        };
        let (applied, tags, widgets, hot, id, table) = deco_parts;
        self.remove_paint(&applied, &tags, span);
        if hot {
            self.hot.set(self.hot.get() - 1);
        }
        for slot in widgets {
            self.release_widget(&slot.widget);
        }
        if let Some(tables) = self.tables.borrow().clone() {
            tables.remove(id);
        }
        if let Some(marks) = table {
            marks.delete_marks(&self.buffer);
        }
        self.active.borrow_mut().remove(&id);
        self.unpainted.set(self.unpainted.get() + 1);
        self.aim.borrow_mut().valid = false;
    }

    /// Paint a block again right now (caret entered or left it).
    pub(super) fn repaint(&self, index: usize) {
        self.unpaint(index);
        self.paint(index);
    }

    pub(super) fn clear_paint_of(&self, deco: &Deco, span: (i32, i32)) {
        self.remove_paint(&deco.applied, &deco.tags, span);
        if deco.hot {
            self.hot.set(self.hot.get() - 1);
        }
        for slot in &deco.widgets {
            self.release_widget(&slot.widget);
        }
        if deco.kind == Kind::Table {
            if let Some(tables) = self.tables.borrow().clone() {
                tables.remove(deco.id);
            }
        }
        self.active.borrow_mut().remove(&deco.id);
    }

    fn remove_paint(&self, applied: &[gtk::TextTag], tags: &[gtk::TextTag], span: (i32, i32)) {
        let total = self.char_count();
        let (a, b) = (span.0.clamp(0, total), span.1.clamp(0, total));
        if b > a {
            let (first, last) = (self.iter(a), self.iter(b));
            for tag in applied {
                self.buffer.remove_tag(tag, &first, &last);
            }
        }
        if let Some(table) = self.buffer.tag_table() {
            for tag in tags {
                table.remove(tag);
            }
        }
    }

    pub(super) fn release_widget(&self, widget: &gtk::Widget) {
        let hover = self.hover_tools.borrow().clone();
        if hover.as_ref() == Some(widget) {
            self.hover_tools.replace(None);
        }
        unsafe { widget.destroy() };
    }
}
