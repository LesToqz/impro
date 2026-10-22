//! In-document pipe tables. The document buffer remains the source of truth.
//!
//! [`Tables`] is created lazily by a host. [`TableHost`] brackets silent writes
//! and supplies replacement tags/layout/history integration. All GTK methods
//! run on the GTK thread; the model and source helpers are independent of GTK.
use crate::{
    blockedit::{byte_at, Edit},
    mdview,
};
use gtk::{prelude::*, subclass::prelude::*};
use impro_core::themes::UiColors;
use regex::Regex;
use std::{
    cell::{Cell as Flag, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
    sync::LazyLock,
    time::{Duration, Instant},
};

pub const BURST_MS: u64 = 900;
pub const REACH: i32 = 18;
pub const GUTTER: i32 = 18;
pub const ROW_ESTIMATE: i32 = 38;
/// HTML line breaks accepted in cell source.
pub static BREAK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>").expect("break pattern"));
/// Cell source as edited: pipe escapes are plain; HTML line breaks become LF.
pub fn to_display(source: &str) -> String {
    BREAK
        .replace_all(&source.replace("\\|", "|"), "\n")
        .into_owned()
}
/// Edited cell text as source; every pipe stays inside its cell.
pub fn to_source(text: &str) -> String {
    let mut result = String::new();
    let mut slashes = 0;
    for c in text.chars() {
        match c {
            '\r' => continue,
            '\n' => result.push_str("<br>"),
            '|' => {
                if slashes % 2 == 0 {
                    result.push('\\');
                }
                result.push('|');
            }
            _ => result.push(c),
        }
        slashes = if c == '\\' { slashes + 1 } else { 0 };
    }
    result
}
/// Canonical pipe row, without its line ending.
pub fn serialize(cells: &[String]) -> String {
    format!("| {} |", cells.join(" | "))
}
/// Alignment encoded by a delimiter cell; no colons means unspecified.
pub fn alignment(rule: &str) -> Option<&'static str> {
    let r = rule.trim();
    if r.starts_with(':') && r.ends_with(':') {
        Some("center")
    } else if r.ends_with(':') {
        Some("right")
    } else if r.starts_with(':') {
        Some("left")
    } else {
        None
    }
}
/// Character ranges of stripped cell source. Unlike the parser this helper
/// retains all columns so malformed excess cells can be preserved on editing.
pub fn cell_spans(line: &str) -> Vec<(usize, usize)> {
    let first = line.len() - line.trim_start().len();
    let end = line.trim_end().len();
    let mut bounds = Vec::new();
    let mut last = first;
    if first > end {
        return vec![(line[..first].chars().count(), line[..first].chars().count())];
    }
    for at in mdview::separators(line).filter(|&i| i >= first && i < end) {
        bounds.push((last, at));
        last = at + 1;
    }
    bounds.push((last, end));
    if line.trim_start().starts_with('|') {
        bounds.remove(0);
    }
    if bounds.len() > 1
        && bounds
            .last()
            .is_some_and(|&(a, b)| line[a..b].trim().is_empty())
    {
        bounds.pop();
    }
    bounds
        .into_iter()
        .map(|(a, b)| {
            let s = &line[a..b];
            let (start, end) = if s.trim().is_empty() {
                let at = a + byte_at(s, 1);
                (at, at)
            } else {
                let start = a + s.len() - s.trim_start().len();
                (start, start + s.trim().len())
            };
            (line[..start].chars().count(), line[..end].chars().count())
        })
        .collect()
}
/// Pango markup for rendered inline Markdown, with inert image labels.
pub fn markup(source: &str) -> String {
    BREAK
        .split(source)
        .map(|piece| {
            mdview::parse_inline(piece)
                .into_iter()
                .map(|span| {
                    let mut value = glib::markup_escape_text(&span.text).to_string();
                    for name in span.styles {
                        let tag = match name.as_str() {
                            "strong" => "b",
                            "emphasis" => "i",
                            "strike" => "s",
                            "code" => "tt",
                            _ => continue,
                        };
                        value = format!("<{tag}>{value}</{tag}>");
                    }
                    if let Some(link) = span.link {
                        value = format!(
                            "<a href=\"{}\">{value}</a>",
                            glib::markup_escape_text(&link)
                        );
                    }
                    value
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
/// Map a rendered caret to raw text by skipping Markdown markers.
pub fn align_index(rendered: &str, raw: &str, index: usize) -> usize {
    let chars: Vec<char> = raw.chars().collect();
    let mut position = 0;
    for c in rendered.chars().take(index) {
        let Some(found) = chars[position..].iter().position(|&r| r == c) else {
            return chars.len();
        };
        position += found + 1;
    }
    if let Some(c) = rendered.chars().nth(index) {
        if let Some(found) = chars[position..].iter().position(|&r| r == c) {
            return position + found;
        }
    }
    position
}
/// One source row, retained verbatim until a cell changes.
#[derive(Clone, Debug)]
pub struct Row {
    pub cr: bool,
    pub line: Option<String>,
    pub cells: Vec<String>,
    pub extra: Vec<String>,
}
impl Row {
    pub fn new(line: Option<&str>, count: usize, cells: Option<Vec<String>>) -> Self {
        let cr = line.is_some_and(|v| v.ends_with('\r'));
        let mut cells = cells.unwrap_or_else(|| mdview::_cells(line.unwrap_or_default()));
        let extra = if cells.len() > count {
            cells.split_off(count)
        } else {
            Vec::new()
        };
        cells.resize(count, String::new());
        Self {
            cr,
            line: line.map(String::from),
            cells,
            extra,
        }
    }
    pub fn render(&self) -> String {
        self.line.clone().unwrap_or_else(|| {
            let mut all = self.cells.clone();
            all.extend(self.extra.clone());
            format!("{}{}", serialize(&all), if self.cr { "\r" } else { "" })
        })
    }
}
/// Header, delimiter row and body. Unchanged lines, CRLF and ragged extras survive.
#[derive(Clone, Debug)]
pub struct Table {
    pub trailing: bool,
    pub rule_line: Option<String>,
    pub rules: Vec<String>,
    pub count: usize,
    pub rows: Vec<Row>,
}
impl Table {
    /// Parse a table block. Returns None if its header/delimiter lines are absent.
    pub fn new(text: &str) -> Option<Self> {
        let trailing = text.ends_with('\n');
        let lines: Vec<_> = text
            .strip_suffix('\n')
            .unwrap_or(text)
            .split('\n')
            .collect();
        if lines.len() < 2 {
            return None;
        }
        let mut rules = mdview::_cells(lines[1]);
        let count = rules.len().max(1);
        rules.resize(count, "---".into());
        let rows = std::iter::once(lines[0])
            .chain(lines[2..].iter().copied())
            .map(|line| Row::new(Some(line), count, None))
            .collect();
        Some(Self {
            trailing,
            rule_line: Some(lines[1].into()),
            rules,
            count,
            rows,
        })
    }
    pub fn cr(&self) -> bool {
        self.rows.first().is_some_and(|r| r.cr)
    }
    pub fn align(&self, column: usize) -> Option<&'static str> {
        self.rules.get(column).and_then(|s| alignment(s))
    }
    pub fn text(&self) -> String {
        let rule = self.rule_line.clone().unwrap_or_else(|| {
            format!(
                "{}{}",
                serialize(&self.rules),
                if self.cr() { "\r" } else { "" }
            )
        });
        let mut lines = vec![self.rows[0].render(), rule];
        lines.extend(self.rows[1..].iter().map(Row::render));
        format!(
            "{}{}",
            lines.join("\n"),
            if self.trailing { "\n" } else { "" }
        )
    }
    pub fn line_index(&self, row: usize) -> usize {
        if row == 0 {
            0
        } else {
            row + 1
        }
    }
    pub fn set(&mut self, row: usize, column: usize, source: &str) {
        if let Some(row) = self.rows.get_mut(row) {
            if let Some(cell) = row.cells.get_mut(column) {
                if cell != source {
                    *cell = source.into();
                    row.line = None;
                }
            }
        }
    }
    pub fn insert_row(&mut self, index: usize) {
        let mut row = Row::new(None, self.count, Some(vec![String::new(); self.count]));
        row.cr = self.cr();
        self.rows.insert(index.min(self.rows.len()), row);
    }
    pub fn delete_row(&mut self, index: usize) {
        if self.rows.len() > 1 && index < self.rows.len() {
            self.rows.remove(index);
        }
    }
    pub fn move_row(&mut self, index: usize, target: usize) {
        if index < self.rows.len() && target < self.rows.len() {
            self.rows.swap(index, target);
        }
    }
    pub fn clear_row(&mut self, index: usize) {
        for column in 0..self.count {
            self.set(index, column, "");
        }
    }
    fn columns_changed(&mut self) {
        self.rule_line = None;
        for row in &mut self.rows {
            row.line = None;
        }
    }
    pub fn insert_column(&mut self, index: usize) {
        let i = index.min(self.count);
        self.count += 1;
        self.rules.insert(i, "---".into());
        for row in &mut self.rows {
            row.cells.insert(i, String::new());
        }
        self.columns_changed();
    }
    pub fn delete_column(&mut self, index: usize) {
        if self.count <= 1 || index >= self.count {
            return;
        }
        self.count -= 1;
        self.rules.remove(index);
        for row in &mut self.rows {
            row.cells.remove(index);
        }
        self.columns_changed();
    }
    pub fn move_column(&mut self, index: usize, target: usize) {
        if index >= self.count || target >= self.count {
            return;
        }
        self.rules.swap(index, target);
        for row in &mut self.rows {
            row.cells.swap(index, target);
        }
        self.columns_changed();
    }
    pub fn clear_column(&mut self, index: usize) {
        for row in 1..self.rows.len() {
            self.set(row, index, "");
        }
    }
    pub fn set_align(&mut self, index: usize, kind: &str) {
        let Some(current) = self.rules.get(index) else {
            return;
        };
        if alignment(current) == Some(kind) || kind == "left" && alignment(current).is_none() {
            return;
        }
        let value = match kind {
            "center" => ":---:",
            "right" => "---:",
            "left" if current.starts_with(':') => ":---",
            _ => "---",
        };
        self.rules[index] = value.into();
        self.rule_line = None;
    }
}

/// Small integration contract for the live document renderer. Implementations
/// must not retain mutable borrows while invoking GTK or host callbacks.
///
/// `silent(true/false)` blocks/unblocks the renderer's insert/delete/changed
/// handlers, including caret handling. `retag` restores replacement/hidden/gap
/// tags after a write; `replacement` reserves space and hides the source before
/// any cells are built. The editor's history still observes silent writes.
pub trait TableHost {
    fn view(&self) -> gtk::TextView;
    fn silent(&self, enter: bool);
    fn replacement(&self, block: &TableBlock, widget: &gtk::Widget, height: i32);
    fn retag(&self, block: &TableBlock);
    fn queue_layout(&self);
    fn provide(&self, _widget: &gtk::Widget) {}
    fn activate_link(&self, _target: &str) {}
    /// Forward the editor's undo/redo shortcut; return whether it was handled.
    fn history(&self, _redo: bool) -> bool {
        false
    }
}
/// Source marks shared by the host and table. Start is left-gravity, end is
/// right-gravity. Text is updated before `retag`, including in-place writes.
#[derive(Clone)]
pub struct TableBlock {
    pub id: u64,
    pub start: gtk::TextMark,
    pub end: gtk::TextMark,
    text: Rc<RefCell<String>>,
}
impl TableBlock {
    pub fn new(id: u64, buffer: &gtk::TextBuffer, start: i32, end: i32, text: &str) -> Self {
        Self {
            id,
            start: buffer
                .create_mark(None, &buffer.iter_at_offset(start), true)
                .expect("unnamed start mark"),
            end: buffer
                .create_mark(None, &buffer.iter_at_offset(end), false)
                .expect("unnamed end mark"),
            text: Rc::new(RefCell::new(text.into())),
        }
    }
    pub fn text(&self) -> String {
        self.text.borrow().clone()
    }
    pub fn offset(&self, buffer: &gtk::TextBuffer) -> i32 {
        buffer.iter_at_mark(&self.start).offset()
    }
    /// Release source marks when the renderer discards this block.
    pub fn delete_marks(&self, buffer: &gtk::TextBuffer) {
        buffer.delete_mark(&self.start);
        buffer.delete_mark(&self.end);
    }
}
struct Silence(Rc<dyn TableHost>);
impl Silence {
    fn new(host: Rc<dyn TableHost>) -> Self {
        host.silent(true);
        Self(host)
    }
}
impl Drop for Silence {
    fn drop(&mut self) {
        self.0.silent(false);
    }
}
mod editor_imp {
    use super::*;
    #[derive(Default)]
    pub struct WidthBin;
    #[glib::object_subclass]
    impl ObjectSubclass for WidthBin {
        const NAME: &'static str = "ImproTableEditorBin";
        type Type = super::WidthBin;
        type ParentType = gtk::Bin;
    }
    impl ObjectImpl for WidthBin {}
    impl WidgetImpl for WidthBin {
        fn preferred_width(&self) -> (i32, i32) {
            (1, 1)
        }
    }
    impl ContainerImpl for WidthBin {}
    impl BinImpl for WidthBin {}
}
glib::wrapper! {pub struct WidthBin(ObjectSubclass<editor_imp::WidthBin>) @extends gtk::Bin,gtk::Container,gtk::Widget,@implements gtk::Buildable;}
/// Wrapped text view whose containing bin accepts the grid's assigned width.
#[derive(Clone)]
pub struct CellEditor {
    bin: WidthBin,
    view: gtk::TextView,
}
impl CellEditor {
    fn new() -> Self {
        let bin: WidthBin = glib::Object::new();
        let view = gtk::TextView::new();
        bin.add(&view);
        Self { bin, view }
    }
    pub fn widget(&self) -> &gtk::Widget {
        self.bin.upcast_ref()
    }
    pub fn text_view(&self) -> &gtk::TextView {
        &self.view
    }
}
impl std::ops::Deref for CellEditor {
    type Target = gtk::TextView;
    fn deref(&self) -> &Self::Target {
        &self.view
    }
}
/// Rendered cell, reusing one text view per table for the active source editor.
#[derive(Clone)]
pub struct Cell {
    pub widget: gtk::EventBox,
    pub inner: gtk::Box,
    pub label: gtk::Label,
    pub row: Rc<Flag<usize>>,
    pub column: Rc<Flag<usize>>,
    source: Rc<RefCell<String>>,
}
impl Cell {
    pub fn source(&self) -> String {
        self.source.borrow().clone()
    }
    fn set_source(&self, source: &str) {
        if *self.source.borrow() == source {
            return;
        }
        *self.source.borrow_mut() = source.into();
        self.label.set_markup(&markup(source));
    }
    pub fn index_at(&self, x: f64, y: f64) -> usize {
        let raw = to_display(&self.source());
        if !self.label.is_visible() {
            return raw.chars().count();
        }
        let Some(layout) = self.label.layout() else {
            return raw.chars().count();
        };
        let (ox, oy) = self.label.layout_offsets();
        let (_, index, trailing) = layout.xy_to_index(
            ((x - ox as f64) * pango::SCALE as f64) as i32,
            ((y - oy as f64) * pango::SCALE as f64) as i32,
        );
        let s = layout.text();
        let byte = (index.max(0) as usize).min(s.len());
        let mut at = byte;
        while !s.is_char_boundary(at) {
            at -= 1;
        }
        align_index(
            &self.label.text(),
            &raw,
            s[..at].chars().count() + trailing.max(0) as usize,
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Focus {
    pub row: usize,
    pub column: usize,
    pub caret: usize,
}
/// Controller shared by tables belonging to one live document. Drop/destroy
/// cancels a typing timeout and releases all owned grids; idle has no sources.
#[derive(Clone)]
pub struct Tables(Rc<TablesInner>);
struct TablesInner {
    host: Rc<dyn TableHost>,
    views: RefCell<BTreeMap<u64, TableView>>,
    outside: Flag<i32>,
    burst: RefCell<Option<glib::SourceId>>,
    bursting: Flag<bool>,
    pending: RefCell<Option<(i32, Focus, Instant)>>,
}
impl Tables {
    pub fn new(host: Rc<dyn TableHost>) -> Self {
        Self(Rc::new(TablesInner {
            host,
            views: RefCell::new(BTreeMap::new()),
            outside: Flag::new(-1),
            burst: RefCell::new(None),
            bursting: Flag::new(false),
            pending: RefCell::new(None),
        }))
    }
    pub fn views(&self) -> Vec<TableView> {
        self.0.views.borrow().values().cloned().collect()
    }
    /// Reserve/hide the block immediately. Call `fill_slice` between main-loop
    /// iterations, or `ensure_rows` for a newly exposed viewport.
    pub fn build(&self, block: TableBlock) -> Option<TableView> {
        let model = Table::new(&block.text())?;
        self.remove(block.id);
        let view = TableView::new(self, block, model);
        self.0
            .views
            .borrow_mut()
            .insert(view.0.block.id, view.clone());
        let host = gtk::Grid::new();
        host.style_context().add_class("live-table-host");
        host.attach(view.widget(), 0, 0, 1, 1);
        host.show_all();
        self.0.host.provide(host.upcast_ref());
        self.0
            .host
            .replacement(&view.0.block, host.upcast_ref(), view.estimate());
        let weak = Rc::downgrade(&self.0);
        let id = view.0.block.id;
        host.connect_destroy(move |_| {
            if let Some(inner) = weak.upgrade() {
                Tables(inner).remove(id);
            }
        });
        view.ensure_rows(0..1);
        self.restore(&view);
        Some(view)
    }
    /// Destroy a disappeared block, ending its typing burst before release.
    pub fn remove(&self, id: u64) {
        let view = self.0.views.borrow_mut().remove(&id);
        if let Some(view) = view {
            self.remember(&view);
            self.end_burst();
            view.destroy();
        }
    }
    pub fn destroy(&self) {
        self.end_burst();
        let views = std::mem::take(&mut *self.0.views.borrow_mut());
        for view in views.into_values() {
            view.destroy();
        }
    }
    pub fn remember(&self, view: &TableView) {
        if let Some(state) = view.focus_state() {
            let buffer = self.0.host.view().buffer().expect("view buffer");
            let line = buffer.iter_at_mark(&view.0.block.start).line();
            *self.0.pending.borrow_mut() = Some((line, state, Instant::now()));
        }
    }
    fn restore(&self, view: &TableView) {
        let pending = self.0.pending.borrow().as_ref().copied();
        if let Some((line, state, time)) = pending {
            if time.elapsed() > Duration::from_secs(3) {
                self.0.pending.borrow_mut().take();
                return;
            }
            let buffer = self.0.host.view().buffer().expect("view buffer");
            let first = buffer.iter_at_mark(&view.0.block.start).line();
            let last = buffer.iter_at_mark(&view.0.block.end).line();
            if first <= line && line <= last {
                self.0.pending.borrow_mut().take();
                view.focus_cell(state.row, state.column, Some(state.caret), None);
            }
        }
    }
    /// Main caret hook. Returns true for a table (the renderer must keep its
    /// source hidden), false for an ordinary block. Selections stay native unless
    /// they fit within one cell.
    pub fn caret(&self, block: Option<u64>, offset: i32) -> bool {
        let Some(id) = block else {
            self.0.outside.set(offset);
            return false;
        };
        let view = self.0.views.borrow().get(&id).cloned();
        let Some(view) = view else { return true };
        let main = self.0.host.view();
        if !view.editable() || view.focused() || !main.is_focus() {
            return true;
        }
        let buffer = main.buffer().expect("view buffer");
        let start = view.0.block.offset(&buffer);
        let end = buffer.iter_at_mark(&view.0.block.end).offset();
        if let Some((a, b)) = buffer.selection_bounds() {
            let first = view.locate(a.offset());
            let last = view.locate(b.offset());
            if start <= a.offset()
                && b.offset() <= end
                && first.row == last.row
                && first.column == last.column
            {
                view.focus_cell(
                    first.row,
                    first.column,
                    None,
                    Some((first.caret, last.caret)),
                );
            }
            return true;
        }
        if offset <= start + 1 {
            let row = if self.0.outside.get() >= end {
                view.row_count() - 1
            } else {
                0
            };
            view.focus_cell(row, 0, Some(0), None);
        } else if offset >= end - 1 {
            view.focus_cell(view.row_count() - 1, view.column_count() - 1, None, None);
        } else {
            let p = view.locate(offset);
            view.focus_cell(p.row, p.column, Some(p.caret), None);
        }
        true
    }
    fn typing(&self) {
        if let Some(source) = self.0.burst.borrow_mut().take() {
            source.remove();
        }
        if !self.0.bursting.replace(true) {
            self.0
                .host
                .view()
                .buffer()
                .expect("view buffer")
                .begin_user_action();
        }
        let weak = Rc::downgrade(&self.0);
        let source = glib::timeout_add_local_once(Duration::from_millis(BURST_MS), move || {
            if let Some(inner) = weak.upgrade() {
                inner.burst.borrow_mut().take();
                Tables(inner).end_burst();
            }
        });
        *self.0.burst.borrow_mut() = Some(source);
    }
    pub fn end_burst(&self) {
        let source = self.0.burst.borrow_mut().take();
        if let Some(source) = source {
            source.remove();
        }
        if self.0.bursting.replace(false) {
            self.0
                .host
                .view()
                .buffer()
                .expect("view buffer")
                .end_user_action();
        }
    }
    pub fn bursting(&self) -> bool {
        self.0.bursting.get()
    }
}
impl Drop for TablesInner {
    fn drop(&mut self) {
        if let Some(source) = self.burst.get_mut().take() {
            source.remove();
        }
        if self.bursting.get() {
            self.host
                .view()
                .buffer()
                .expect("view buffer")
                .end_user_action();
        }
        for view in self.views.get_mut().values() {
            view.destroy();
        }
    }
}

/// Cloneable grid handle. Unbuilt rows reserve their estimated height; the host
/// drives bounded filling, prioritizing rows in the viewport. No polling timer.
#[derive(Clone)]
pub struct TableView(Rc<ViewInner>);
struct ViewInner {
    tables: Weak<TablesInner>,
    block: TableBlock,
    widget: gtk::Overlay,
    pub frame: gtk::Grid,
    model: RefCell<Table>,
    cells: RefCell<Vec<Option<Vec<Cell>>>>,
    editor: Option<CellEditor>,
    current: Flag<Option<(usize, usize)>>,
    loading: Flag<bool>,
    moving: Flag<bool>,
    dead: Flag<bool>,
    want: RefCell<Option<Focus>>,
    row_strip: gtk::EventBox,
    column_strip: gtk::EventBox,
    add_row: gtk::Button,
    add_column: gtk::Button,
    hover: Flag<Option<(usize, usize)>>,
    grip: Flag<Option<(usize, usize)>>,
    menu: RefCell<Option<gtk::Popover>>,
    max_slice: Flag<Duration>,
}
impl TableView {
    fn tables(&self) -> Option<Tables> {
        self.0.tables.upgrade().map(Tables)
    }
    fn new(tables: &Tables, block: TableBlock, model: Table) -> Self {
        let widget = gtk::Overlay::new();
        widget.set_hexpand(true);
        widget.style_context().add_class("live-table-view");
        let frame = gtk::Grid::builder()
            .column_homogeneous(true)
            .row_spacing(1)
            .column_spacing(1)
            .build();
        frame.style_context().add_class("live-table");
        widget.add(&frame);
        let editable = tables.0.host.view().is_editable();
        let editor = editable.then(|| {
            let e = CellEditor::new();
            e.set_wrap_mode(gtk::WrapMode::WordChar);
            e.set_accepts_tab(false);
            e.style_context().add_class("live-table-editor");
            tables.0.host.provide(e.upcast_ref());
            e
        });
        let row_strip = make_strip(true);
        let column_strip = make_strip(false);
        let add_row = make_bar("Add row");
        let add_column = make_bar("Add column");
        let len = model.rows.len();
        let this = Self(Rc::new(ViewInner {
            tables: Rc::downgrade(&tables.0),
            block,
            widget,
            frame,
            model: RefCell::new(model),
            cells: RefCell::new(vec![None; len]),
            editor,
            current: Flag::new(None),
            loading: Flag::new(false),
            moving: Flag::new(false),
            dead: Flag::new(false),
            want: RefCell::new(None),
            row_strip,
            column_strip,
            add_row,
            add_column,
            hover: Flag::new(None),
            grip: Flag::new(None),
            menu: RefCell::new(None),
            max_slice: Flag::new(Duration::ZERO),
        }));
        // Empty spacers preserve document geometry before offscreen rows exist.
        for row in 0..len {
            let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
            spacer.set_size_request(1, ROW_ESTIMATE - 1);
            this.0
                .frame
                .attach(&spacer, 0, row as i32, this.column_count() as i32, 1);
        }
        if editable {
            this.connect_editing();
        }
        let weak = Rc::downgrade(&this.0);
        this.0.widget.connect_map(move |_| {
            if let Some(v) = weak.upgrade() {
                let view = TableView(v);
                let want = view.0.want.borrow_mut().take();
                if let Some(p) = want {
                    view.focus_cell(p.row, p.column, Some(p.caret), None);
                }
            }
        });
        this
    }
    pub fn widget(&self) -> &gtk::Overlay {
        &self.0.widget
    }
    pub fn frame(&self) -> &gtk::Grid {
        &self.0.frame
    }
    pub fn editor(&self) -> Option<CellEditor> {
        self.0.editor.clone()
    }
    pub fn block(&self) -> TableBlock {
        self.0.block.clone()
    }
    pub fn model(&self) -> Table {
        self.0.model.borrow().clone()
    }
    pub fn text(&self) -> String {
        self.0.block.text()
    }
    pub fn editable(&self) -> bool {
        self.0.editor.is_some()
    }
    pub fn row_count(&self) -> usize {
        self.0.model.borrow().rows.len()
    }
    pub fn column_count(&self) -> usize {
        self.0.model.borrow().count
    }
    pub fn estimate(&self) -> i32 {
        self.row_count() as i32 * ROW_ESTIMATE + 2 + if self.editable() { REACH } else { 0 }
    }
    pub fn cell(&self, row: usize, column: usize) -> Option<Cell> {
        self.0
            .cells
            .borrow()
            .get(row)?
            .as_ref()?
            .get(column)
            .cloned()
    }
    pub fn built_rows(&self) -> usize {
        self.0.cells.borrow().iter().filter(|r| r.is_some()).count()
    }
    pub fn max_slice_time(&self) -> Duration {
        self.0.max_slice.get()
    }
    /// Build a slice of missing rows, stopping after one row or 3 ms. Returns
    /// whether more rows remain. The host schedules subsequent slices as needed.
    pub fn fill_slice(&self) -> bool {
        let start = Instant::now();
        let index = self.0.cells.borrow().iter().position(Option::is_none);
        if let Some(row) = index {
            self.ensure_rows(row..row + 1);
        }
        self.0
            .max_slice
            .set(self.0.max_slice.get().max(start.elapsed()));
        self.built_rows() < self.row_count()
    }
    /// Materialize visible rows first. Call with a small viewport range; each row
    /// is the atomic unit, so even an eight-column table yields between rows.
    pub fn ensure_rows(&self, range: std::ops::Range<usize>) {
        for row in range.take_while(|&r| r < self.row_count()) {
            if self.0.cells.borrow()[row].is_some() {
                continue;
            }
            let children = self.0.frame.children();
            for child in children {
                if self.0.frame.child_property::<i32>(&child, "top-attach") == row as i32 {
                    self.0.frame.remove(&child);
                }
            }
            let cells = (0..self.column_count())
                .map(|column| self.make_cell(row, column))
                .collect::<Vec<_>>();
            self.0.cells.borrow_mut()[row] = Some(cells);
        }
    }
    fn make_cell(&self, row: usize, column: usize) -> Cell {
        let widget = gtk::EventBox::new();
        widget.style_context().add_class("live-table-cell");
        let inner = gtk::Box::new(gtk::Orientation::Vertical, 0);
        inner.style_context().add_class("live-table-inner");
        let label = gtk::Label::new(None);
        label.set_xalign(0.);
        label.set_yalign(0.);
        label.set_line_wrap(true);
        label.set_line_wrap_mode(pango::WrapMode::WordChar);
        label.set_width_chars(1);
        label.set_max_width_chars(1);
        label.style_context().add_class("live-table-label");
        inner.pack_start(&label, true, true, 0);
        widget.add(&inner);
        let cell = Cell {
            widget,
            inner,
            label,
            row: Rc::new(Flag::new(row)),
            column: Rc::new(Flag::new(column)),
            source: Rc::new(RefCell::new(String::new())),
        };
        let source = self.0.model.borrow().rows[row].cells[column].clone();
        cell.label.set_markup(&markup(&source));
        *cell.source.borrow_mut() = source;
        self.style_cell(&cell);
        let weak = Rc::downgrade(&self.0);
        cell.label.connect_activate_link(move |_, uri| {
            if let Some(v) = weak.upgrade() {
                if let Some(t) = TableView(v).tables() {
                    t.0.host.activate_link(uri);
                }
            }
            glib::Propagation::Stop
        });
        if self.editable() {
            cell.widget.add_events(
                gdk::EventMask::BUTTON_PRESS_MASK
                    | gdk::EventMask::ENTER_NOTIFY_MASK
                    | gdk::EventMask::LEAVE_NOTIFY_MASK,
            );
            let weak = Rc::downgrade(&self.0);
            let copy = cell.clone();
            cell.widget.connect_button_press_event(move |_, e| {
                if e.button() != 1 || e.event_type() != gdk::EventType::ButtonPress {
                    return glib::Propagation::Proceed;
                }
                if let Some(v) = weak.upgrade() {
                    let (x, y) = e.position();
                    TableView(v).focus_cell(
                        copy.row.get(),
                        copy.column.get(),
                        Some(copy.index_at(x, y)),
                        None,
                    );
                }
                glib::Propagation::Stop
            });
            let weak = Rc::downgrade(&self.0);
            let (r, c) = (cell.row.clone(), cell.column.clone());
            cell.widget.connect_enter_notify_event(move |_, _| {
                if let Some(v) = weak.upgrade() {
                    TableView(v).hovered(Some((r.get(), c.get())));
                }
                glib::Propagation::Proceed
            });
            let weak = Rc::downgrade(&self.0);
            cell.widget.connect_leave_notify_event(move |_, e| {
                if e.detail() != gdk::NotifyType::Inferior {
                    if let Some(v) = weak.upgrade() {
                        TableView(v).unhovered();
                    }
                }
                glib::Propagation::Proceed
            });
            cell.widget.connect_realize(|w| {
                if let Some(window) = w.window() {
                    window.set_cursor(gdk::Cursor::from_name(&window.display(), "text").as_ref());
                }
            });
        }
        self.0
            .frame
            .attach(&cell.widget, column as i32, row as i32, 1, 1);
        cell.widget.show_all();
        if let Some(t) = self.tables() {
            t.0.host.provide(cell.widget.upcast_ref());
        }
        cell
    }
    fn style_cell(&self, cell: &Cell) {
        let row = cell.row.get();
        let col = cell.column.get();
        let last = self.row_count() - 1;
        let count = self.column_count();
        let context = cell.widget.style_context();
        for (name, on) in [
            ("live-table-header", row == 0),
            ("tl", row == 0 && col == 0),
            ("tr", row == 0 && col + 1 == count),
            ("bl", row == last && col == 0),
            ("br", row == last && col + 1 == count),
        ] {
            if on {
                context.add_class(name)
            } else {
                context.remove_class(name)
            }
        }
        let align = self.0.model.borrow().align(col);
        cell.label.set_xalign(match align {
            Some("center") => 0.5,
            Some("right") => 1.,
            _ => 0.,
        });
        cell.label.set_justify(justification(align));
    }
    pub fn focused(&self) -> bool {
        self.0.current.get().is_some() && self.0.editor.as_ref().is_some_and(|e| e.is_focus())
    }
    pub fn focus_state(&self) -> Option<Focus> {
        if !self.focused() {
            return None;
        }
        let (row, column) = self.0.current.get()?;
        let e = self.0.editor.as_ref()?;
        Some(Focus {
            row,
            column,
            caret: e.buffer()?.cursor_position() as usize,
        })
    }
    /// Reveal the cell's source, then place/select the caret in edited text.
    pub fn focus_cell(
        &self,
        row: usize,
        column: usize,
        caret: Option<usize>,
        select: Option<(usize, usize)>,
    ) {
        if self.0.dead.get() {
            return;
        }
        let Some(editor) = &self.0.editor else { return };
        let row = row.min(self.row_count() - 1);
        let column = column.min(self.column_count() - 1);
        self.ensure_rows(row..row + 1);
        if !self.0.widget.is_mapped() {
            *self.0.want.borrow_mut() = Some(Focus {
                row,
                column,
                caret: caret.unwrap_or(usize::MAX),
            });
            return;
        }
        let Some(cell) = self.cell(row, column) else {
            return;
        };
        let buffer = editor.buffer().expect("cell buffer");
        self.0.moving.set(true);
        if self.0.current.get() != Some((row, column)) {
            if let Some(t) = self.tables() {
                t.end_burst();
            }
            self.release();
            if let Some(parent) = editor
                .bin
                .parent()
                .and_then(|w| w.downcast::<gtk::Container>().ok())
            {
                parent.remove(editor.widget());
            }
            self.0.current.set(Some((row, column)));
            cell.inner.pack_start(editor.widget(), true, true, 0);
            self.0.loading.set(true);
            buffer.set_text(&to_display(&cell.source()));
            self.0.loading.set(false);
            editor.set_justification(justification(self.0.model.borrow().align(column)));
            cell.label.hide();
            cell.widget.style_context().add_class("focused");
            editor.bin.show_all();
        }
        editor.grab_focus();
        let size = buffer.char_count() as usize;
        if let Some((a, b)) = select {
            buffer.select_range(
                &buffer.iter_at_offset(b.min(size) as i32),
                &buffer.iter_at_offset(a.min(size) as i32),
            );
        } else {
            buffer.place_cursor(&buffer.iter_at_offset(caret.unwrap_or(size).min(size) as i32));
        }
        self.0.moving.set(false);
        self.0.grip.set(Some((row, column)));
        self.sync_caret();
        self.update_handles();
        self.reveal(&cell);
    }
    fn release(&self) {
        let current = self.0.current.take();
        if let Some((r, c)) = current {
            if let Some(cell) = self.cell(r, c) {
                cell.label.set_markup(&markup(&cell.source()));
                cell.label.show();
                cell.widget.style_context().remove_class("focused");
            }
        }
        if let Some(editor) = &self.0.editor {
            editor.bin.hide();
        }
    }
    pub fn locate(&self, offset: i32) -> Focus {
        let start = self
            .tables()
            .and_then(|t| t.0.host.view().buffer())
            .map_or(0, |b| self.0.block.offset(&b));
        let relative = offset.saturating_sub(start).max(0) as usize;
        let text = self.text();
        let lines: Vec<_> = text.split('\n').collect();
        let mut position = 0;
        let mut index = 0;
        for (i, line) in lines.iter().enumerate() {
            index = i;
            if relative <= position + line.chars().count() {
                break;
            }
            position += line.chars().count() + 1;
        }
        let row = if index <= 1 { 0 } else { index - 1 }.min(self.row_count() - 1);
        let line = lines[self.0.model.borrow().line_index(row)];
        let spans = cell_spans(line);
        let within = if index == 1 {
            0
        } else {
            relative.saturating_sub(position)
        };
        let (column, (a, b)) = spans
            .iter()
            .take(self.column_count())
            .copied()
            .enumerate()
            .find(|(c, (_, b))| within <= *b || c + 1 == spans.len().min(self.column_count()))
            .unwrap_or((0, (0, 0)));
        let source = &line[byte_at(line, a)..byte_at(line, b)];
        Focus {
            row,
            column,
            caret: to_display(&source[..byte_at(source, within.saturating_sub(a))])
                .chars()
                .count(),
        }
    }
    fn sync_caret(&self) {
        if self.0.loading.get() {
            return;
        }
        let Some((row, column)) = self.0.current.get() else {
            return;
        };
        let Some(t) = self.tables() else { return };
        let editor = self.0.editor.as_ref().expect("editable table");
        let cell_buffer = editor.buffer().expect("cell buffer");
        let caret = cell_buffer.cursor_position();
        let value = cell_buffer
            .text(
                &cell_buffer.start_iter(),
                &cell_buffer.iter_at_offset(caret),
                true,
            )
            .unwrap_or_default();
        let model = self.0.model.borrow();
        let index = model.line_index(row);
        drop(model);
        let text = self.text();
        let lines: Vec<_> = text.split('\n').collect();
        let mut position = lines[..index]
            .iter()
            .map(|v| v.chars().count() + 1)
            .sum::<usize>();
        if let Some(&(a, b)) = cell_spans(lines[index]).get(column) {
            position += (a + to_source(&value).chars().count()).min(b);
        }
        let buffer = t.0.host.view().buffer().expect("view buffer");
        let offset = self.0.block.offset(&buffer) + position.min(text.chars().count()) as i32;
        let _silent = Silence::new(t.0.host.clone());
        buffer.place_cursor(&buffer.iter_at_offset(offset));
    }
    /// Replace source through a minimal Unicode hunk, retaining its final LF.
    pub fn write(&self, new: &str) {
        let old = self.text();
        if old == new {
            return;
        }
        let Some(t) = self.tables() else { return };
        let edit = minimal_edit(&old, new);
        let buffer = t.0.host.view().buffer().expect("view buffer");
        let start = self.0.block.offset(&buffer);
        {
            let _silent = Silence::new(t.0.host.clone());
            if edit.end > edit.start {
                buffer.delete(
                    &mut buffer.iter_at_offset(start + edit.start as i32),
                    &mut buffer.iter_at_offset(start + edit.end as i32),
                );
            }
            if !edit.text.is_empty() {
                buffer.insert(
                    &mut buffer.iter_at_offset(start + edit.start as i32),
                    &edit.text,
                );
            }
            buffer.move_mark(
                &self.0.block.end,
                &buffer.iter_at_offset(start + new.chars().count() as i32),
            );
        }
        *self.0.block.text.borrow_mut() = new.into();
        t.0.host.retag(&self.0.block);
        t.0.host.queue_layout();
    }
    fn edited(&self) {
        if self.0.loading.get() || self.0.dead.get() {
            return;
        }
        let Some((row, column)) = self.0.current.get() else {
            return;
        };
        let Some(t) = self.tables() else { return };
        let buffer = self
            .0
            .editor
            .as_ref()
            .expect("cell editor")
            .buffer()
            .expect("cell buffer");
        let value = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .unwrap_or_default();
        let source = to_source(&value);
        if let Some(cell) = self.cell(row, column) {
            *cell.source.borrow_mut() = source.clone();
        }
        self.0.model.borrow_mut().set(row, column, &source);
        t.typing();
        let text = self.0.model.borrow().text();
        self.write(&text);
        self.sync_caret();
    }
    pub fn append(&self, axis: &str) {
        let index = if axis == "row" {
            self.row_count() - 1
        } else {
            self.column_count() - 1
        };
        self.act(axis, "after", index);
    }
    /// Apply a row/column menu action in one GTK user action. Existing grid and
    /// editor objects survive; only inserted/removed cells change ownership.
    pub fn act(&self, axis: &str, action: &str, index: usize) {
        if !self.editable() {
            return;
        }
        let rows = axis == "row";
        let length = if rows {
            self.row_count()
        } else {
            self.column_count()
        };
        if index >= length {
            return;
        }
        if action == "delete"
            && if rows {
                index == 0 || length <= 2
            } else {
                length <= 2
            }
        {
            return;
        }
        if action == "up" && index == 0 || action == "down" && index + 1 >= length {
            return;
        }
        let target = match action {
            "after" | "down" => index + 1,
            "up" => index - 1,
            "delete" => index.min(length - 2),
            _ => index,
        };
        let state = self.0.current.get().unwrap_or((0, 0));
        let Some(t) = self.tables() else { return };
        t.end_burst();
        self.0.moving.set(true);
        self.release();
        if let Some(editor) = &self.0.editor {
            if let Some(parent) = editor
                .bin
                .parent()
                .and_then(|w| w.downcast::<gtk::Container>().ok())
            {
                parent.remove(editor.widget());
            }
        }
        {
            let mut model = self.0.model.borrow_mut();
            match (action, rows) {
                ("before" | "after", true) => model.insert_row(target),
                ("before" | "after", false) => model.insert_column(target),
                ("delete", true) => model.delete_row(index),
                ("delete", false) => model.delete_column(index),
                ("up" | "down", true) => model.move_row(index, target),
                ("up" | "down", false) => model.move_column(index, target),
                ("clear", true) => model.clear_row(index),
                ("clear", false) => model.clear_column(index),
                (_, false) if action.starts_with("align-") => model.set_align(index, &action[6..]),
                _ => {
                    self.0.moving.set(false);
                    return;
                }
            }
        }
        self.resize_cells(rows, action, target, index);
        self.restyle();
        let buffer = t.0.host.view().buffer().expect("view buffer");
        buffer.begin_user_action();
        let new = self.0.model.borrow().text();
        self.write(&new);
        buffer.end_user_action();
        self.0.moving.set(false);
        let (row, column) = if rows {
            (target, state.1)
        } else {
            (state.0, target)
        };
        self.focus_cell(
            row,
            column,
            if ["before", "after", "clear"].contains(&action) {
                Some(0)
            } else {
                None
            },
            None,
        );
        self.0.widget.queue_resize();
    }
    fn resize_cells(&self, rows: bool, action: &str, target: usize, index: usize) {
        if rows {
            if ["before", "after"].contains(&action) {
                self.0.frame.insert_row(target as i32);
                self.0.cells.borrow_mut().insert(target, None);
                self.ensure_rows(target..target + 1);
            } else if action == "delete" {
                let old = self.0.cells.borrow_mut().remove(index);
                if let Some(cells) = old {
                    for cell in cells {
                        self.0.frame.remove(&cell.widget);
                    }
                }
                self.0.frame.remove_row(index as i32);
            }
        } else if ["before", "after", "delete"].contains(&action) {
            let built: Vec<_> = self
                .0
                .cells
                .borrow()
                .iter()
                .enumerate()
                .filter_map(|(i, r)| r.is_some().then_some(i))
                .collect();
            if action == "delete" {
                let removed: Vec<_> = self
                    .0
                    .cells
                    .borrow_mut()
                    .iter_mut()
                    .filter_map(|row| row.as_mut().map(|r| r.remove(index)))
                    .collect();
                for cell in removed {
                    self.0.frame.remove(&cell.widget);
                }
                self.0.frame.remove_column(index as i32);
            } else {
                self.0.frame.insert_column(target as i32);
                for row in built {
                    let cell = self.make_cell(row, target);
                    self.0.cells.borrow_mut()[row]
                        .as_mut()
                        .expect("built row")
                        .insert(target, cell);
                }
            }
        }
    }
    fn restyle(&self) {
        let cells = self.0.cells.borrow().clone();
        let model = self.model();
        for (row, cells) in cells.into_iter().enumerate() {
            if let Some(cells) = cells {
                for (column, cell) in cells.into_iter().enumerate() {
                    cell.row.set(row);
                    cell.column.set(column);
                    cell.set_source(&model.rows[row].cells[column]);
                    self.style_cell(&cell);
                }
            }
        }
    }
    pub fn leave(&self, after: bool) {
        let Some(t) = self.tables() else { return };
        t.end_burst();
        let main = t.0.host.view();
        let buffer = main.buffer().expect("view buffer");
        let start = self.0.block.offset(&buffer);
        let text = self.text();
        let end = start + text.chars().count() as i32;
        buffer.begin_user_action();
        let target = if after {
            if end >= buffer.char_count() {
                buffer.insert(
                    &mut buffer.end_iter(),
                    if text.ends_with('\n') { "\n" } else { "\n\n" },
                );
                buffer.char_count()
            } else {
                end
            }
        } else if start == 0 {
            buffer.insert(&mut buffer.start_iter(), "\n");
            0
        } else {
            start - 1
        };
        buffer.place_cursor(&buffer.iter_at_offset(target));
        buffer.end_user_action();
        t.0.outside.set(target);
        self.release();
        main.grab_focus();
        main.scroll_mark_onscreen(&buffer.get_insert().expect("insert mark"));
        self.update_handles();
    }
    /// Dispatch cell navigation; plain editing and clipboard keys remain native.
    pub fn key(&self, key: gdk::keys::Key, state: gdk::ModifierType) -> bool {
        use gdk::keys::constants as k;
        let Some((row, col)) = self.0.current.get() else {
            return false;
        };
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        if ctrl && [k::z, k::Z, k::y, k::Y].contains(&key) {
            if let Some(t) = self.tables() {
                t.end_burst();
                t.remember(self);
                return t.0.host.history(shift || [k::y, k::Y].contains(&key));
            }
            return false;
        }
        if state.intersects(
            gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::MOD1_MASK
                | gdk::ModifierType::SUPER_MASK,
        ) {
            return false;
        }
        let editor = self.0.editor.as_ref().expect("editable");
        let b = editor.buffer().expect("cell buffer");
        let caret = b.iter_at_offset(b.cursor_position());
        let selection = b.has_selection();
        let rows = self.row_count();
        let count = self.column_count();
        if key == k::ISO_Left_Tab || key == k::Tab && shift {
            if col > 0 {
                self.focus_cell(row, col - 1, None, None);
            } else if row > 0 {
                self.focus_cell(row - 1, count - 1, None, None);
            }
            return true;
        }
        if [k::Tab, k::KP_Tab].contains(&key) {
            if col + 1 < count {
                self.focus_cell(row, col + 1, None, None);
            } else if row + 1 < rows {
                self.focus_cell(row + 1, 0, None, None);
            } else {
                self.append("row");
            }
            return true;
        }
        if [k::Return, k::KP_Enter].contains(&key) {
            if shift {
                b.delete_selection(true, true);
                b.insert_at_cursor("\n");
            } else if row + 1 < rows {
                self.focus_cell(row + 1, col, None, None);
            } else {
                self.act("row", "after", rows - 1);
                self.focus_cell(rows, col, Some(0), None);
            }
            return true;
        }
        let up = [k::Up, k::KP_Up].contains(&key);
        let down = [k::Down, k::KP_Down].contains(&key);
        if !shift && (up || down) {
            let edge = if up { b.start_iter() } else { b.end_iter() };
            if editor.iter_location(&caret).y() == editor.iter_location(&edge).y() {
                if up && row > 0 {
                    self.focus_cell(row - 1, col, Some(caret.line_offset() as usize), None);
                } else if down && row + 1 < rows {
                    self.focus_cell(row + 1, col, Some(caret.line_offset() as usize), None);
                } else {
                    self.leave(down);
                }
                return true;
            }
        }
        if !shift && !selection && [k::Left, k::KP_Left].contains(&key) && caret.is_start() {
            if col > 0 {
                self.focus_cell(row, col - 1, None, None);
            } else if row > 0 {
                self.focus_cell(row - 1, count - 1, None, None);
            } else {
                self.leave(false);
            }
            return true;
        }
        if !shift && !selection && [k::Right, k::KP_Right].contains(&key) && caret.is_end() {
            if col + 1 < count {
                self.focus_cell(row, col + 1, Some(0), None);
            } else if row + 1 < rows {
                self.focus_cell(row + 1, 0, Some(0), None);
            } else {
                self.leave(true);
            }
            return true;
        }
        if key == k::Escape {
            self.leave(true);
            return true;
        }
        key == k::BackSpace && !selection && caret.is_start()
    }
    fn reveal(&self, cell: &Cell) {
        let Some(t) = self.tables() else { return };
        let view = t.0.host.view();
        if !cell.widget.is_realized() {
            return;
        }
        let a = cell.widget.allocation();
        let (_, y) = view.window_to_buffer_coords(gtk::TextWindowType::Widget, a.x(), a.y());
        let visible = view.visible_rect();
        if let Some(adjustment) = view.vadjustment() {
            let value = adjustment.value();
            if y < visible.y() + 8 {
                adjustment.set_value((value - (visible.y() + 8 - y) as f64).max(0.));
            } else if y + a.height() > visible.y() + visible.height() - 8 {
                adjustment.set_value(
                    value + (y + a.height() - visible.y() - visible.height() + 8) as f64,
                );
            }
        }
    }
    pub fn destroy(&self) {
        if self.0.dead.replace(true) {
            return;
        }
        if let Some(t) = self.tables() {
            t.end_burst();
        }
        let menu = self.0.menu.borrow_mut().take();
        if let Some(menu) = menu {
            unsafe {
                menu.destroy();
            }
        }
        self.0.current.set(None);
        unsafe {
            self.0.widget.destroy();
        }
    }
    fn connect_editing(&self) {
        let e = self.0.editor.as_ref().expect("editable");
        self.0.frame.set_margin_end(REACH);
        self.0.frame.set_margin_bottom(REACH);
        let weak = Rc::downgrade(&self.0);
        e.connect_key_press_event(move |_, event| {
            if weak
                .upgrade()
                .is_some_and(|v| TableView(v).key(event.keyval(), event.state()))
            {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        let weak = Rc::downgrade(&self.0);
        e.buffer().expect("cell buffer").connect_changed(move |_| {
            if let Some(v) = weak.upgrade() {
                TableView(v).edited();
            }
        });
        let weak = Rc::downgrade(&self.0);
        e.buffer()
            .expect("cell buffer")
            .connect_mark_set(move |_, _, mark| {
                if mark.name().as_deref() == Some("insert") {
                    if let Some(v) = weak.upgrade() {
                        let view = TableView(v);
                        if !view.0.moving.get() {
                            view.sync_caret();
                        }
                    }
                }
            });
        let weak = Rc::downgrade(&self.0);
        e.connect_focus_out_event(move |_, _| {
            let weak = weak.clone();
            glib::idle_add_local_once(move || {
                if let Some(v) = weak.upgrade() {
                    let view = TableView(v);
                    if !view.0.dead.get()
                        && !view.focused()
                        && !view.0.moving.get()
                        && view.0.menu.borrow().is_none()
                    {
                        if let Some(t) = view.tables() {
                            t.end_burst();
                        }
                        view.release();
                        view.update_handles();
                    }
                }
            });
            glib::Propagation::Proceed
        });
        for handle in self.handles() {
            handle.set_no_show_all(true);
            handle.set_opacity(0.);
            self.0.widget.add_overlay(&handle);
            let weak = Rc::downgrade(&self.0);
            handle.connect_enter_notify_event(move |_, _| {
                if let Some(v) = weak.upgrade() {
                    TableView(v).hovered(None);
                }
                glib::Propagation::Proceed
            });
            let weak = Rc::downgrade(&self.0);
            handle.connect_leave_notify_event(move |_, event| {
                if event.detail() != gdk::NotifyType::Inferior {
                    if let Some(v) = weak.upgrade() {
                        TableView(v).unhovered();
                    }
                }
                glib::Propagation::Proceed
            });
        }
        let weak = Rc::downgrade(&self.0);
        self.0.add_row.connect_clicked(move |_| {
            if let Some(v) = weak.upgrade() {
                TableView(v).append("row");
            }
        });
        let weak = Rc::downgrade(&self.0);
        self.0.add_column.connect_clicked(move |_| {
            if let Some(v) = weak.upgrade() {
                TableView(v).append("column");
            }
        });
        for (axis, strip) in [("row", &self.0.row_strip), ("column", &self.0.column_strip)] {
            let weak = Rc::downgrade(&self.0);
            strip.connect_button_press_event(move |_, e| {
                if [1, 3].contains(&e.button()) {
                    if let Some(v) = weak.upgrade() {
                        let view = TableView(v);
                        let grip = view.0.grip.get().unwrap_or((0, 0));
                        view.open_menu(axis, if axis == "row" { grip.0 } else { grip.1 });
                    }
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
        }
        let weak = Rc::downgrade(&self.0);
        self.0.widget.connect_get_child_position(move |_, child| {
            let v = weak.upgrade()?;
            let a = v.frame.allocation();
            let own = v.widget.allocation();
            let x = a.x() - own.x();
            let y = a.y() - own.y();
            let (x, y, w, h) = if child == v.add_column.upcast_ref::<gtk::Widget>() {
                (x + a.width() + 5, y, REACH - 8, a.height())
            } else if child == v.add_row.upcast_ref::<gtk::Widget>() {
                (x, y + a.height() + 5, a.width(), REACH - 8)
            } else if child == v.row_strip.upcast_ref::<gtk::Widget>() {
                (x - GUTTER, y, GUTTER - 1, a.height())
            } else if child == v.column_strip.upcast_ref::<gtk::Widget>() {
                (x, y - GUTTER, a.width(), GUTTER - 2)
            } else {
                return None;
            };
            Some(gdk::Rectangle::new(x, y, w.max(1), h.max(1)))
        });
    }
    /// Editable handles in row, column, add-column, add-row order.
    pub fn handles(&self) -> Vec<gtk::Widget> {
        if !self.editable() {
            return Vec::new();
        }
        vec![
            self.0.row_strip.clone().upcast(),
            self.0.column_strip.clone().upcast(),
            self.0.add_column.clone().upcast(),
            self.0.add_row.clone().upcast(),
        ]
    }
    pub fn hovered(&self, cell: Option<(usize, usize)>) {
        let cell = cell.or(self.0.grip.get()).unwrap_or((0, 0));
        self.0.hover.set(Some(cell));
        self.0.grip.set(Some(cell));
        self.update_handles();
    }
    pub fn unhovered(&self) {
        self.0.hover.set(None);
        self.update_handles();
    }
    fn update_handles(&self) {
        if !self.editable() {
            return;
        }
        let hovered = self.0.hover.get().is_some();
        let shown = hovered || self.focused() || self.0.menu.borrow().is_some();
        for (index, widget) in self.handles().iter().enumerate() {
            let show = if index < 2 { shown } else { hovered };
            if widget.is_visible() != show {
                impro_ui::motion::fade(widget, show, 140, true, None);
            }
        }
        let (row, column) = self.0.grip.get().unwrap_or((0, 0));
        for (strip, is_row) in [(&self.0.row_strip, true), (&self.0.column_strip, false)] {
            if let Some(cell) = self.cell(
                if is_row { row } else { 0 },
                if is_row { 0 } else { column },
            ) {
                let a = cell.widget.allocation();
                if let Some(fixed) = strip.child().and_then(|w| w.downcast::<gtk::Fixed>().ok()) {
                    if let Some(pill) = fixed.children().first() {
                        let origin = self.0.frame.allocation();
                        fixed.move_(
                            pill,
                            if is_row {
                                2
                            } else {
                                a.x() - origin.x() + a.width() / 2 - 10
                            },
                            if is_row {
                                a.y() - origin.y() + a.height() / 2 - 10
                            } else {
                                2
                            },
                        );
                    }
                }
            }
        }
    }
    /// Menu items use the same strings and validity constraints as table keys.
    pub fn open_menu(&self, axis: &str, index: usize) -> gtk::Popover {
        let old = self.0.menu.borrow_mut().take();
        if let Some(old) = old {
            old.popdown();
        }
        let strip = if axis == "row" {
            &self.0.row_strip
        } else {
            &self.0.column_strip
        };
        let popover = gtk::Popover::new(Some(strip));
        popover.set_position(gtk::PositionType::Bottom);
        popover.set_property(
            "transitions-enabled",
            impro_ui::motion::animations_enabled(),
        );
        popover.style_context().add_class("impro-selection-menu");
        popover.style_context().add_class("live-table-menu");
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let count = if axis == "row" {
            self.row_count()
        } else {
            self.column_count()
        };
        let mut items = if axis == "row" {
            vec![
                ("Insert row above", "before", true, false),
                ("Insert row below", "after", true, false),
                ("Move up", "up", index > 0, false),
                ("Move down", "down", index + 1 < count, false),
            ]
        } else {
            vec![
                ("Insert column left", "before", true, false),
                ("Insert column right", "after", true, false),
                ("Move left", "up", index > 0, false),
                ("Move right", "down", index + 1 < count, false),
            ]
        };
        let align = self.0.model.borrow().align(index).unwrap_or("left");
        if axis != "row" {
            items.extend([
                ("Align left", "align-left", true, align == "left"),
                ("Align center", "align-center", true, align == "center"),
                ("Align right", "align-right", true, align == "right"),
            ]);
        }
        items.push(("Clear contents", "clear", true, false));
        items.push((
            if axis == "row" {
                "Delete row"
            } else {
                "Delete column"
            },
            "delete",
            if axis == "row" {
                index > 0 && count > 2
            } else {
                count > 2
            },
            false,
        ));
        for (i, (label, action, enabled, checked)) in items.into_iter().enumerate() {
            if [2, 4].contains(&i) || (axis != "row" && i == 7) {
                let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
                separator.set_margin_top(3);
                separator.set_margin_bottom(3);
                list.pack_start(&separator, false, false, 0);
            }
            let button = gtk::Button::new();
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let text = gtk::Label::new(Some(label));
            text.set_xalign(0.);
            row.pack_start(&text, true, true, 0);
            let mark = gtk::Label::new(Some(if checked { "✓" } else { "" }));
            mark.style_context().add_class("live-table-check");
            row.pack_end(&mark, false, false, 0);
            button.add(&row);
            button.set_relief(gtk::ReliefStyle::None);
            button.set_sensitive(enabled);
            if let Some(accessible) = button.accessible() {
                accessible.set_property("accessible-name", label);
            }
            let weak = Rc::downgrade(&self.0);
            let axis = axis.to_string();
            let menu = popover.downgrade();
            button.connect_clicked(move |_| {
                if let Some(menu) = menu.upgrade() {
                    menu.popdown();
                }
                if let Some(v) = weak.upgrade() {
                    TableView(v).act(&axis, action, index);
                }
            });
            list.pack_start(&button, false, false, 0);
        }
        list.show_all();
        popover.add(&list);
        if let Some(t) = self.tables() {
            t.0.host.provide(popover.upcast_ref());
        }
        let weak = Rc::downgrade(&self.0);
        popover.connect_closed(move |p| {
            if let Some(v) = weak.upgrade() {
                let current = v.menu.borrow_mut().take();
                drop(current);
                TableView(v).update_handles();
            }
            let p = p.clone();
            glib::idle_add_local_once(move || unsafe {
                p.destroy();
            });
        });
        *self.0.menu.borrow_mut() = Some(popover.clone());
        self.update_handles();
        popover.popup();
        popover
    }
}
fn justification(align: Option<&str>) -> gtk::Justification {
    match align {
        Some("center") => gtk::Justification::Center,
        Some("right") => gtk::Justification::Right,
        _ => gtk::Justification::Left,
    }
}
fn make_bar(tooltip: &str) -> gtk::Button {
    let b = gtk::Button::with_label("+");
    b.set_can_focus(false);
    b.set_relief(gtk::ReliefStyle::None);
    b.set_tooltip_text(Some(tooltip));
    if let Some(a) = b.accessible() {
        a.set_property("accessible-name", tooltip);
    }
    b.style_context().add_class("live-table-add");
    b
}
fn make_strip(rows: bool) -> gtk::EventBox {
    let strip = gtk::EventBox::new();
    strip.set_visible_window(false);
    strip.set_tooltip_text(Some(if rows {
        "Row options"
    } else {
        "Column options"
    }));
    strip.add_events(
        gdk::EventMask::BUTTON_PRESS_MASK
            | gdk::EventMask::ENTER_NOTIFY_MASK
            | gdk::EventMask::LEAVE_NOTIFY_MASK,
    );
    let fixed = gtk::Fixed::new();
    let pill = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    pill.style_context().add_class("live-table-grip");
    let dots = gtk::Grid::builder()
        .row_spacing(2)
        .column_spacing(2)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build();
    for i in 0..6 {
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.style_context().add_class("live-table-dot");
        dots.attach(
            &dot,
            if rows { i % 2 } else { i / 2 },
            if rows { i / 2 } else { i % 2 },
            1,
            1,
        );
    }
    pill.pack_start(&dots, true, true, 0);
    pill.set_size_request(if rows { 13 } else { 20 }, if rows { 20 } else { 13 });
    fixed.put(&pill, 0, 0);
    strip.add(&fixed);
    fixed.show_all();
    strip
}
/// Minimal character hunk, with the table's trailing LF kept outside the edit.
/// Extending a lone slash insertion prevents the host slash menu opening.
pub fn minimal_edit(old: &str, new: &str) -> Edit {
    let a: Vec<_> = old.chars().collect();
    let b: Vec<_> = new.chars().collect();
    let keep = usize::from(old.ends_with('\n') && new.ends_with('\n'));
    let mut first = a
        .iter()
        .zip(&b)
        .take_while(|(a, b)| a == b)
        .count()
        .min(a.len() - keep)
        .min(b.len() - keep);
    let tail = a[first..]
        .iter()
        .rev()
        .zip(b[first..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    if &b[first..b.len() - tail] == ['/'] && first > 0 {
        first -= 1;
    }
    Edit {
        start: first,
        end: a.len() - tail,
        text: b[first..b.len() - tail].iter().collect(),
        cursor: b.len() - tail,
    }
}
/// Table CSS registered with the shared theme registry.
pub fn css(ui: &UiColors) -> String {
    let mut ui = ui.clone();
    ui.insert("table_bg".into(), ui["editor_bg"].clone());
    ui.insert("table_size".into(), "11".into());
    ui.insert("table_add_size".into(), "10".into());
    ui.insert("table_font".into(), "Sans".into());
    impro_ui::render_css(include_str!("livetable.css"), &ui)
}
/// Host style override matching its body font, editor colour and point size.
pub fn styled_css(ui: &UiColors, bg: &str, size: i32, font: &str) -> String {
    let mut ui = ui.clone();
    ui.insert("table_bg".into(), bg.into());
    ui.insert("table_size".into(), size.to_string());
    ui.insert("table_add_size".into(), 8.max(size - 1).to_string());
    ui.insert("table_font".into(), font.replace(['"', '\\'], ""));
    impro_ui::render_css(include_str!("livetable.css"), &ui)
}
pub fn init() {
    impro_ui::register_css("markdown.tables", css);
}
