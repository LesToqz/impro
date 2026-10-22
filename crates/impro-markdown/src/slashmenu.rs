//! Caret-anchored note commands, driven entirely from the text view.
//!
//! Typing `/` at a line start or after a space opens a popover list under the
//! caret; the query is the text typed after the slash. Arrow keys, Enter, Tab
//! and Escape drive it without taking focus from the view. Choosing a command
//! is one undoable edit through [`impro_core::notes::apply_slash_command`]; the
//! typed `/query` is its own undo step.
use gtk::prelude::*;
use impro_core::notes::{self, SlashCommand};
use regex::Regex;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::LazyLock,
};

static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\w+").expect("word pattern"));
static FENCE_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:>\s*)*(`{3,}|~{3,})(.*)$").expect("fence pattern"));
static FENCE_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:>\s*)*(`{3,}|~{3,})").expect("fence start pattern"));
static TICKS: LazyLock<fancy_regex::Regex> =
    LazyLock::new(|| fancy_regex::Regex::new(r"(?<!\\)(`+)").expect("tick pattern"));

/// Prefer exact aliases, prefixes, word starts, then compact subsequences.
/// `None` means the command does not match at all; lower scores rank first.
pub fn match_score(command: &SlashCommand, query: &str) -> Option<usize> {
    let query = query.to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    let needle: Vec<char> = query.chars().collect();
    let mut scores = Vec::new();
    for value in std::iter::once(command.label).chain(command.keywords.iter().copied()) {
        let value = value.to_lowercase();
        let length = value.chars().count();
        if value == query {
            scores.push(0);
        } else if value.starts_with(&query) {
            scores.push(10 + length - needle.len());
        } else if WORD.find_iter(&value).any(|w| w.as_str().starts_with(&query)) {
            scores.push(100 + length);
        } else {
            let hay: Vec<char> = value.chars().collect();
            let (mut at, mut positions) = (0, Vec::new());
            let mut matched = true;
            for &c in &needle {
                match hay[at.min(hay.len())..].iter().position(|&h| h == c) {
                    Some(i) => {
                        positions.push(at + i);
                        at += i + 1;
                    }
                    None => {
                        matched = false;
                        break;
                    }
                }
            }
            if matched {
                scores.push(200 + positions[positions.len() - 1] - positions[0] + length);
            }
        }
    }
    scores.into_iter().min()
}

fn char_slice(text: &str, from: usize, to: usize) -> &str {
    let byte = |n: usize| text.char_indices().nth(n).map_or(text.len(), |(i, _)| i);
    let (a, b) = (byte(from), byte(to));
    &text[a..b.max(a)]
}

/// Check fenced and inline code, including an unfinished inline delimiter.
/// `offset` counts characters.
pub fn in_code(document: &str, offset: usize) -> bool {
    let before = char_slice(document, 0, offset);
    let mut fence: Option<String> = None;
    let mut lines: Vec<&str> = before.split('\n').collect();
    lines.pop();
    for line in lines {
        let Some(found) = FENCE_LINE.captures(line) else {
            continue;
        };
        let (token, rest) = (found[1].to_string(), found[2].to_string());
        match &fence {
            None => fence = Some(token),
            Some(open) => {
                if token.starts_with(open.chars().next().unwrap_or('`'))
                    && token.chars().count() >= open.chars().count()
                    && rest.trim().is_empty()
                {
                    fence = None;
                }
            }
        }
    }
    if fence.is_some() {
        return true;
    }
    let line = before.rsplit('\n').next().unwrap_or("");
    if FENCE_START.is_match(line) {
        return true;
    }
    let mut delimiter: Option<String> = None;
    for found in TICKS.find_iter(line).flatten() {
        let token = found.as_str().to_string();
        match &delimiter {
            None => delimiter = Some(token),
            Some(open) if *open == token => delimiter = None,
            _ => {}
        }
    }
    delimiter.is_some()
}

struct Inner {
    view: gtk::TextView,
    buffer: gtk::TextBuffer,
    popover: gtk::Popover,
    scroll: gtk::ScrolledWindow,
    list: gtk::ListBox,
    marks: RefCell<Option<(gtk::TextMark, gtk::TextMark)>>,
    query: RefCell<String>,
    matches: RefCell<Vec<&'static SlashCommand>>,
    rows: RefCell<Vec<gtk::ListBoxRow>>,
    index: Cell<usize>,
    composing: Cell<bool>,
    grouped: Cell<bool>,
    destroyed: Cell<bool>,
    ime_commit: Cell<bool>,
    blocked_insert: Cell<bool>,
    idle: RefCell<Option<glib::SourceId>>,
    ime_idle: RefCell<Option<glib::SourceId>>,
    scroll_timeout: RefCell<Option<glib::SourceId>>,
    handlers: RefCell<Vec<(glib::Object, glib::SignalHandlerId)>>,
    top_handler: RefCell<Option<(glib::Object, glib::SignalHandlerId)>>,
    icons: Cell<bool>,
    weak: Weak<Inner>,
}

/// Slash-command popover of one live note view.
#[derive(Clone)]
pub struct SlashMenu(Rc<Inner>);

thread_local! {
    static REGISTRY: RefCell<Vec<(glib::WeakRef<gtk::TextView>, Weak<Inner>)>> =
        const { RefCell::new(Vec::new()) };
}

/// The slash menu attached to `view`, if any.
pub fn for_view(view: &gtk::TextView) -> Option<SlashMenu> {
    REGISTRY.with(|r| {
        r.borrow()
            .iter()
            .find(|(v, _)| v.upgrade().as_ref() == Some(view))
            .and_then(|(_, w)| w.upgrade())
            .map(SlashMenu)
    })
}

impl SlashMenu {
    pub fn new(view: &gtk::TextView) -> Self {
        let buffer = view.buffer().expect("text view buffer");
        let popover = gtk::Popover::new(Some(view));
        popover.set_modal(false);
        popover.set_can_focus(false);
        popover.set_position(gtk::PositionType::Bottom);
        popover.style_context().add_class("impro-slash-menu");
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        popover.add(&container);
        let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_propagate_natural_height(true);
        scroll.set_max_content_height(7 * 34 + 22);
        scroll.set_size_request(282, -1);
        scroll.set_can_focus(false);
        if let Some(bar) = scroll.vscrollbar() {
            bar.set_can_focus(false);
        }
        if let Some(bar) = scroll.hscrollbar() {
            bar.set_can_focus(false);
        }
        container.add(&scroll);
        let list = gtk::ListBox::new();
        list.set_can_focus(false);
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_activate_on_single_click(true);
        scroll.add(&list);
        let inner = Rc::new_cyclic(|weak| Inner {
            view: view.clone(),
            buffer: buffer.clone(),
            popover,
            scroll,
            list,
            marks: RefCell::new(None),
            query: RefCell::new(String::new()),
            matches: RefCell::new(Vec::new()),
            rows: RefCell::new(Vec::new()),
            index: Cell::new(0),
            composing: Cell::new(false),
            grouped: Cell::new(false),
            destroyed: Cell::new(false),
            ime_commit: Cell::new(false),
            blocked_insert: Cell::new(false),
            idle: RefCell::new(None),
            ime_idle: RefCell::new(None),
            scroll_timeout: RefCell::new(None),
            handlers: RefCell::new(Vec::new()),
            top_handler: RefCell::new(None),
            icons: Cell::new(true),
            weak: weak.clone(),
        });
        inner.build(&container);
        REGISTRY.with(|r| {
            let mut r = r.borrow_mut();
            r.retain(|(v, w)| v.upgrade().is_some() && w.upgrade().is_some());
            r.push((view.downgrade(), Rc::downgrade(&inner)));
        });
        Self(inner)
    }
    pub fn destroy(&self) {
        self.0.destroy();
    }
    /// Offer the keyboard handler of the owning editor (see the panel's key routing).
    pub fn handle_key(&self, view: &gtk::TextView, event: &gdk::EventKey) -> bool {
        let _ = view;
        self.0.handle_key(event)
    }
    pub fn opened(&self) -> bool {
        self.0.opened()
    }
    pub fn close(&self) {
        self.0.close();
    }
    /// Run `command` (or the highlighted one) for the typed query.
    pub fn execute(&self, command: Option<&SlashCommand>) {
        self.0.execute(command);
    }
    pub fn query(&self) -> String {
        self.0.query.borrow().clone()
    }
    pub fn matches(&self) -> Vec<&'static SlashCommand> {
        self.0.matches.borrow().clone()
    }
    pub fn index(&self) -> usize {
        self.0.index.get()
    }
    pub fn rows(&self) -> Vec<gtk::ListBoxRow> {
        self.0.rows.borrow().clone()
    }
    pub fn list(&self) -> gtk::ListBox {
        self.0.list.clone()
    }
    pub fn scroll(&self) -> gtk::ScrolledWindow {
        self.0.scroll.clone()
    }
    pub fn popover(&self) -> gtk::Popover {
        self.0.popover.clone()
    }
    pub fn is_destroyed(&self) -> bool {
        self.0.destroyed.get()
    }
    /// Signal connections still held (zero after `destroy`).
    pub fn handler_count(&self) -> usize {
        self.0.handlers.borrow().len() + usize::from(self.0.top_handler.borrow().is_some())
    }
    /// Show command glyphs instead of theme icons (test hook).
    pub fn set_icons_enabled(&self, enabled: bool) {
        self.0.icons.set(enabled);
    }
}

impl Inner {
    fn connect_to<O: IsA<glib::Object>>(&self, object: &O, id: glib::SignalHandlerId) {
        self.handlers.borrow_mut().push((object.clone().upcast(), id));
    }

    fn build(&self, container: &gtk::Box) {
        let weak = self.weak.clone();
        self.list.connect_row_activated(move |_, row| {
            if let Some(s) = weak.upgrade() {
                s.activate_row(row);
            }
        });
        let footer = gtk::EventBox::new();
        footer.style_context().add_class("slash-footer");
        footer.add(&self.labels("↑↓ Move  ↵ Insert", "esc Close", ""));
        let weak = self.weak.clone();
        footer.connect_button_press_event(move |_, _| {
            if let Some(s) = weak.upgrade() {
                s.close();
            }
            glib::Propagation::Stop
        });
        container.pack_end(&footer, false, false, 0);

        let buffer = &self.buffer;
        let w = self.weak.clone();
        let id = buffer.connect_insert_text(move |b, at, text| {
            if let Some(s) = w.upgrade() {
                s.on_insert(b, at.offset(), text);
            }
        });
        self.connect_to(buffer, id);
        let w = self.weak.clone();
        let id = buffer.connect_changed(move |_| {
            if let Some(s) = w.upgrade() {
                s.queue();
            }
        });
        self.connect_to(buffer, id);
        let w = self.weak.clone();
        let id = buffer.connect_mark_set(move |_, _, mark| {
            if let Some(s) = w.upgrade() {
                if matches!(mark.name().as_deref(), Some("insert" | "selection_bound")) {
                    s.queue();
                }
            }
        });
        self.connect_to(buffer, id);
        let view = &self.view;
        let w = self.weak.clone();
        let id = view.connect_preedit_changed(move |_, text| {
            if let Some(s) = w.upgrade() {
                s.preedit(text);
            }
        });
        self.connect_to(view, id);
        let w = self.weak.clone();
        let id = view.connect_key_press_event(move |_, event| {
            match w.upgrade() {
                Some(s) if s.handle_key(event) => glib::Propagation::Stop,
                _ => glib::Propagation::Proceed,
            }
        });
        self.connect_to(view, id);
        let w = self.weak.clone();
        let id = view.connect_focus_out_event(move |_, _| {
            if let Some(s) = w.upgrade() {
                s.close();
            }
            glib::Propagation::Proceed
        });
        self.connect_to(view, id);
        let w = self.weak.clone();
        let id = view.connect_button_press_event(move |_, _| {
            if let Some(s) = w.upgrade() {
                s.close();
            }
            glib::Propagation::Proceed
        });
        self.connect_to(view, id);
        let w = self.weak.clone();
        let id = view.connect_scroll_event(move |_, _| {
            if let Some(s) = w.upgrade() {
                s.close();
            }
            glib::Propagation::Proceed
        });
        self.connect_to(view, id);
        let w = self.weak.clone();
        let id = view.connect_destroy(move |_| {
            if let Some(s) = w.upgrade() {
                s.destroy();
            }
        });
        self.connect_to(view, id);
        let w = self.weak.clone();
        let id = view.connect_hierarchy_changed(move |_, _| {
            if let Some(s) = w.upgrade() {
                s.hierarchy_changed();
            }
        });
        self.connect_to(view, id);
        self.hierarchy_changed();
        if let Some(parent) = view.parent().and_then(|p| p.downcast::<gtk::ScrolledWindow>().ok()) {
            for adjustment in [parent.vadjustment(), parent.hadjustment()] {
                let w = self.weak.clone();
                let id = adjustment.connect_value_changed(move |_| {
                    if let Some(s) = w.upgrade() {
                        s.scrolled();
                    }
                });
                self.connect_to(&adjustment, id);
            }
        }
    }

    fn hierarchy_changed(&self) {
        if let Some((object, id)) = self.top_handler.borrow_mut().take() {
            object.disconnect(id);
        }
        if let Some(top) = self.view.toplevel().and_then(|t| t.downcast::<gtk::Window>().ok()) {
            let w = self.weak.clone();
            let id = top.connect_button_press_event(move |_, event| {
                let Some(s) = w.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let mut raw: gdk::Event = (**event).clone();
                let widget = gtk::event_widget(&mut raw);
                if let Some(widget) = widget {
                    let popover: &gtk::Widget = s.popover.upcast_ref();
                    if &widget == popover || widget.is_ancestor(popover) {
                        return glib::Propagation::Proceed;
                    }
                }
                s.close();
                glib::Propagation::Proceed
            });
            *self.top_handler.borrow_mut() = Some((top.upcast(), id));
        }
    }

    fn opened(&self) -> bool {
        self.marks.borrow().is_some()
    }

    fn text(&self) -> String {
        self.buffer
            .text(&self.buffer.start_iter(), &self.buffer.end_iter(), true)
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    fn insert_offset(&self) -> i32 {
        self.buffer
            .get_insert()
            .map_or(0, |m| self.buffer.iter_at_mark(&m).offset())
    }

    fn preedit(&self, text: &str) {
        if let Some(id) = self.ime_idle.borrow_mut().take() {
            id.remove();
        }
        self.composing.set(!text.is_empty());
        if self.composing.get() {
            self.ime_commit.set(true);
            self.close();
        } else {
            // GTK clears preedit before inserting the committed text.
            let weak = self.weak.clone();
            let id = glib::idle_add_local(move || {
                if let Some(s) = weak.upgrade() {
                    s.ime_idle.borrow_mut().take();
                    s.ime_commit.set(false);
                }
                glib::ControlFlow::Break
            });
            *self.ime_idle.borrow_mut() = Some(id);
        }
    }

    fn scrolled(&self) {
        if !self.opened() {
            return;
        }
        // GTK can animate a caret scroll through intermediate offscreen positions.
        if let Some(id) = self.scroll_timeout.borrow_mut().take() {
            id.remove();
        }
        let weak = self.weak.clone();
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
            if let Some(s) = weak.upgrade() {
                s.scroll_timeout.borrow_mut().take();
                s.check_visible();
            }
        });
        *self.scroll_timeout.borrow_mut() = Some(id);
        self.queue();
    }

    fn check_visible(&self) {
        if !self.opened() {
            return;
        }
        let Some(mark) = self.buffer.get_insert() else {
            return;
        };
        let rect = self.view.iter_location(&self.buffer.iter_at_mark(&mark));
        let visible = self.view.visible_rect();
        if rect.y() + rect.height() < visible.y()
            || rect.y() > visible.y() + visible.height()
            || rect.x() < visible.x()
            || rect.x() > visible.x() + visible.width()
        {
            self.close();
        }
    }

    fn on_insert(&self, buffer: &gtk::TextBuffer, offset: i32, text: &str) {
        let blocked = self.blocked_insert.replace(false);
        if blocked
            || self.opened()
            || self.composing.get()
            || self.ime_commit.get()
            || buffer.has_selection()
            || text != "/"
        {
            return;
        }
        let document = self.text();
        let before = char_slice(&document, (offset - 1).max(0) as usize, offset.max(0) as usize);
        if (offset > 0 && !before.chars().next().is_some_and(char::is_whitespace))
            || in_code(&document, offset.max(0) as usize)
        {
            return;
        }
        let at = buffer.iter_at_offset(offset);
        let start = buffer.create_mark(None, &at, true);
        let end = buffer.create_mark(None, &at, false);
        if let (Some(start), Some(end)) = (start, end) {
            *self.marks.borrow_mut() = Some((start, end));
        } else {
            return;
        }
        // Keep the typed query together; executing the command starts a new action.
        buffer.begin_user_action();
        self.grouped.set(true);
        self.queue();
    }

    fn queue(&self) {
        if !self.opened() || self.idle.borrow().is_some() {
            return;
        }
        let weak = self.weak.clone();
        let id = glib::idle_add_local(move || {
            if let Some(s) = weak.upgrade() {
                s.idle.borrow_mut().take();
                s.refresh();
            }
            glib::ControlFlow::Break
        });
        *self.idle.borrow_mut() = Some(id);
    }

    fn refresh(&self) {
        let Some((start_mark, end_mark)) = self.marks.borrow().clone() else {
            return;
        };
        let start = self.buffer.iter_at_mark(&start_mark).offset() as usize;
        let end = self.buffer.iter_at_mark(&end_mark).offset() as usize;
        let cursor = self.insert_offset() as usize;
        let document = self.text();
        let between = char_slice(&document, start, cursor);
        if self.buffer.has_selection()
            || !(start < cursor && cursor <= end)
            || char_slice(&document, start, start + 1) != "/"
            || between.contains('\n')
        {
            self.close();
            return;
        }
        let query = char_slice(&document, start + 1, cursor).to_string();
        if query.ends_with(' ') && self.matches.borrow().is_empty() && !self.query.borrow().is_empty() {
            self.close();
            return;
        }
        let changed = query != *self.query.borrow() || !self.popover.is_visible();
        *self.query.borrow_mut() = query.clone();
        let mut ranked: Vec<(usize, usize, &'static SlashCommand)> = notes::SLASH_COMMANDS
            .iter()
            .enumerate()
            .filter_map(|(i, c)| match_score(c, &query).map(|s| (s, i, c)))
            .collect();
        ranked.sort_by_key(|item| (item.0, item.1));
        *self.matches.borrow_mut() = ranked.into_iter().map(|item| item.2).collect();
        if changed {
            self.index.set(0);
            self.build_rows();
        }
        if let Some(mark) = self.buffer.get_insert() {
            let rect = self.view.iter_location(&self.buffer.iter_at_mark(&mark));
            let (x, y) = self
                .view
                .buffer_to_window_coords(gtk::TextWindowType::Widget, rect.x(), rect.y());
            let target = gdk::Rectangle::new(x, y, rect.width().max(1), rect.height());
            self.popover.set_pointing_to(&target);
        }
        let was_visible = self.popover.is_visible();
        self.popover.show_all();
        if !was_visible {
            self.popover.set_opacity(0.0);
            impro_ui::motion::slide_in(&self.popover, 8, 128, None);
        }
    }

    fn labels(&self, text: &str, hint: &str, secondary: &str) -> gtk::Box {
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let title = gtk::Label::new(Some(text));
        title.set_xalign(0.0);
        title.style_context().add_class("slash-title");
        container.pack_start(&title, false, false, 0);
        if !secondary.is_empty() {
            container.pack_start(&self.muted(secondary), false, false, 0);
        }
        container.pack_end(&self.muted(hint), false, false, 0);
        container
    }

    fn muted(&self, text: &str) -> gtk::Label {
        let label = gtk::Label::new(Some(text));
        label.set_xalign(0.0);
        label.style_context().add_class("slash-muted");
        label
    }

    fn heading(&self, text: &str) {
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        row.set_activatable(false);
        row.set_can_focus(false);
        row.style_context().add_class("slash-group");
        let label = self.muted(text);
        label.style_context().add_class("slash-heading");
        row.add(&label);
        self.list.add(&row);
    }

    fn build_rows(&self) {
        for child in self.list.children() {
            unsafe { child.destroy() };
        }
        self.rows.borrow_mut().clear();
        let query = self.query.borrow().clone();
        let matches = self.matches.borrow().clone();
        let mut group: Option<&str> = None;
        if !query.is_empty() {
            self.heading("Filtered results");
        }
        for (index, command) in matches.iter().enumerate() {
            if query.is_empty() && group != Some(command.group) {
                group = Some(command.group);
                self.heading(command.group);
            }
            let row = gtk::ListBoxRow::new();
            row.set_can_focus(false);
            let container = self.labels(command.label, command.hint, command.secondary);
            let theme = if self.icons.get() { gtk::IconTheme::default() } else { None };
            let icon = command
                .icons
                .iter()
                .find(|name| theme.as_ref().is_some_and(|t| t.has_icon(name)));
            let widget: gtk::Widget = match icon {
                Some(name) => gtk::Image::from_icon_name(Some(name), gtk::IconSize::Menu).upcast(),
                None => gtk::Label::new(Some(command.glyph)).upcast(),
            };
            let tile = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            tile.style_context().add_class("slash-icon");
            widget.set_halign(gtk::Align::Center);
            widget.set_valign(gtk::Align::Center);
            tile.pack_start(&widget, true, true, 0);
            container.pack_start(&tile, false, false, 0);
            container.reorder_child(&tile, 0);
            row.add(&container);
            row.add_events(gdk::EventMask::ENTER_NOTIFY_MASK);
            let weak = self.weak.clone();
            row.connect_enter_notify_event(move |_, _| {
                if let Some(s) = weak.upgrade() {
                    s.select(index as isize);
                }
                glib::Propagation::Proceed
            });
            self.rows.borrow_mut().push(row.clone());
            self.list.add(&row);
        }
        if matches.is_empty() {
            self.heading("No results");
        }
        self.select(0);
    }

    fn select(&self, index: isize) {
        let rows = self.rows.borrow().clone();
        if rows.is_empty() {
            return;
        }
        let at = index.rem_euclid(rows.len() as isize) as usize;
        self.index.set(at);
        let row = &rows[at];
        self.list.select_row(Some(row));
        let allocation = row.allocation();
        self.scroll
            .vadjustment()
            .clamp_page(f64::from(allocation.y()), f64::from(allocation.y() + allocation.height()));
    }

    fn activate_row(&self, row: &gtk::ListBoxRow) {
        let position = self.rows.borrow().iter().position(|r| r == row);
        let command = position.and_then(|i| self.matches.borrow().get(i).copied());
        if let Some(command) = command {
            self.execute(Some(command));
        }
    }

    fn execute(&self, command: Option<&SlashCommand>) {
        let Some((start_mark, _)) = self.marks.borrow().clone() else {
            return;
        };
        let matches = self.matches.borrow().clone();
        if matches.is_empty() {
            return;
        }
        let Some(command) = command
            .copied()
            .or_else(|| matches.get(self.index.get()).map(|c| **c))
        else {
            return;
        };
        let first = self.buffer.iter_at_mark(&start_mark).offset() as usize;
        let cursor = self.insert_offset() as usize;
        let document = self.text();
        let cleaned = format!(
            "{}{}",
            char_slice(&document, 0, first),
            char_slice(&document, cursor, usize::MAX)
        );
        let edit = command.edit(&cleaned, first as isize, Some(first as isize));
        self.close();
        let Ok(edit) = edit else {
            return;
        };
        let buffer = &self.buffer;
        buffer.begin_user_action();
        let (mut a, mut b) = (buffer.iter_at_offset(first as i32), buffer.iter_at_offset(cursor as i32));
        buffer.delete(&mut a, &mut b);
        let (mut a, mut b) = (
            buffer.iter_at_offset(edit.start as i32),
            buffer.iter_at_offset(edit.end as i32),
        );
        buffer.delete(&mut a, &mut b);
        let mut at = buffer.iter_at_offset(edit.start as i32);
        buffer.insert(&mut at, &edit.text);
        buffer.select_range(
            &buffer.iter_at_offset(edit.select_end as i32),
            &buffer.iter_at_offset(edit.select_start as i32),
        );
        buffer.end_user_action();
        if let Some(mark) = buffer.get_insert() {
            self.view.scroll_mark_onscreen(&mark);
        }
    }

    fn handle_key(&self, event: &gdk::EventKey) -> bool {
        use gdk::keys::constants as k;
        let key = event.keyval();
        if !self.opened() || self.composing.get() {
            if key == k::slash && self.buffer.has_selection() {
                self.blocked_insert.set(true);
            }
            return false;
        }
        if let Some(id) = self.idle.borrow_mut().take() {
            id.remove();
            self.refresh();
        }
        if !self.opened() {
            return false;
        }
        if event.state().intersects(
            gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::MOD1_MASK | gdk::ModifierType::SUPER_MASK,
        ) {
            self.close();
            return false;
        }
        let count = self.rows.borrow().len() as isize;
        let index = self.index.get() as isize;
        if key == k::Up {
            self.select(index - 1);
        } else if key == k::Down {
            self.select(index + 1);
        } else if key == k::Home {
            self.select(0);
        } else if key == k::End {
            self.select(count - 1);
        } else if key == k::Return || key == k::KP_Enter || key == k::Tab || key == k::ISO_Left_Tab {
            self.execute(None);
        } else if key == k::Escape {
            self.close();
        } else if key == k::Left || key == k::Right {
            if let Some(mark) = self.buffer.get_insert() {
                let mut cursor = self.buffer.iter_at_mark(&mark);
                if key == k::Left {
                    cursor.backward_char();
                } else {
                    cursor.forward_char();
                }
                self.buffer.place_cursor(&cursor);
            }
            self.refresh_now();
        } else if key == k::BackSpace {
            if let Some(mark) = self.buffer.get_insert() {
                let mut cursor = self.buffer.iter_at_mark(&mark);
                let mut before = cursor.clone();
                before.backward_char();
                self.buffer.begin_user_action();
                self.buffer.delete(&mut before, &mut cursor);
                self.buffer.end_user_action();
            }
            self.refresh_now();
        } else {
            return false;
        }
        true
    }

    fn refresh_now(&self) {
        if let Some(id) = self.idle.borrow_mut().take() {
            id.remove();
        }
        self.refresh();
    }

    fn close(&self) {
        if let Some(id) = self.scroll_timeout.borrow_mut().take() {
            id.remove();
        }
        if let Some(id) = self.idle.borrow_mut().take() {
            id.remove();
        }
        if let Some((start, end)) = self.marks.borrow_mut().take() {
            self.buffer.delete_mark(&start);
            self.buffer.delete_mark(&end);
        }
        if self.grouped.replace(false) {
            self.buffer.end_user_action();
        }
        self.popover.hide();
    }

    fn destroy(&self) {
        if self.destroyed.replace(true) {
            return;
        }
        if let Some(id) = self.ime_idle.borrow_mut().take() {
            id.remove();
        }
        self.close();
        for (object, id) in std::mem::take(&mut *self.handlers.borrow_mut()) {
            object.disconnect(id);
        }
        if let Some((object, id)) = self.top_handler.borrow_mut().take() {
            object.disconnect(id);
        }
        unsafe { self.popover.destroy() };
        REGISTRY.with(|r| {
            r.borrow_mut().retain(|(_, w)| w.upgrade().is_some_and(|i| !std::ptr::eq(&*i, self)));
        });
    }
}
