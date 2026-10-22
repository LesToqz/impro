//! Reusable prompt box; callbacks run after the host restores focus.
use crate::fuzzy::{highlight_markup, Range, MAX_RESULTS};
use gtk::prelude::*;
use std::{
    any::Any,
    cell::{Cell, RefCell},
    rc::Rc,
};
pub const SHADOW_MARGINS: (i32, i32, i32, i32) = (24, 48, 72, 48);
pub const DEFAULT_INSTRUCTIONS: &[(&str, &str)] = &[
    ("↑↓", "to navigate"),
    ("↵", "to use"),
    ("esc", "to dismiss"),
];
#[derive(Clone, Default)]
pub struct PromptItem {
    pub label: String,
    pub detail: String,
    pub hotkey: Vec<String>,
    pub data: Option<Rc<dyn Any>>,
    pub label_ranges: Vec<Range>,
    pub detail_ranges: Vec<Range>,
    pub muted: bool,
}
impl PromptItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Default::default()
        }
    }
    pub fn with_data<T: Any>(mut self, data: T) -> Self {
        self.data = Some(Rc::new(data));
        self
    }
    pub fn data<T: Any>(&self) -> Option<&T> {
        self.data.as_ref()?.downcast_ref()
    }
    pub fn hotkeys(&self) -> Vec<String> {
        self.hotkey
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect()
    }
}
type Provider = Rc<dyn Fn(&str) -> Vec<PromptItem>>;
type Chosen = Rc<dyn Fn(&PromptItem, gdk::ModifierType)>;
type QueryChosen = Rc<dyn Fn(&str, gdk::ModifierType)>;
type Action = Rc<dyn Fn()>;
#[derive(Clone)]
pub struct ResultRow {
    pub widget: gtk::ListBoxRow,
    pub title: gtk::Label,
    pub detail: gtk::Label,
    pub chips: gtk::Box,
    cached: Rc<RefCell<Option<PromptItem>>>,
}
impl ResultRow {
    fn new() -> Self {
        let widget = gtk::ListBoxRow::new();
        widget.set_can_focus(false);
        widget.style_context().add_class("impro-prompt-item");
        let bx = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let title = gtk::Label::new(None);
        title.set_xalign(0.);
        title.set_ellipsize(pango::EllipsizeMode::End);
        title.style_context().add_class("impro-prompt-title");
        let detail = gtk::Label::new(None);
        detail.set_xalign(0.);
        detail.set_ellipsize(pango::EllipsizeMode::Start);
        detail.set_no_show_all(true);
        detail.style_context().add_class("impro-prompt-detail");
        text.pack_start(&title, false, false, 0);
        text.pack_start(&detail, false, false, 0);
        let chips = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        chips.set_valign(gtk::Align::Center);
        chips.set_no_show_all(true);
        bx.pack_start(&text, true, true, 0);
        bx.pack_end(&chips, false, false, 0);
        widget.add(&bx);
        bx.show_all();
        Self {
            widget,
            title,
            detail,
            chips,
            cached: Rc::new(RefCell::new(None)),
        }
    }
    fn update(&self, item: &PromptItem) {
        let same = self.cached.borrow().as_ref().is_some_and(|old| {
            old.label == item.label
                && old.detail == item.detail
                && old.hotkey == item.hotkey
                && old.label_ranges == item.label_ranges
                && old.detail_ranges == item.detail_ranges
                && old.muted == item.muted
        });
        if same {
            return;
        }
        let mut cached = item.clone();
        cached.data = None;
        *self.cached.borrow_mut() = Some(cached);
        self.title
            .set_markup(&highlight_markup(&item.label, &item.label_ranges));
        self.detail
            .set_markup(&highlight_markup(&item.detail, &item.detail_ranges));
        self.detail.set_visible(!item.detail.is_empty());
        let context = self.widget.style_context();
        if item.muted {
            context.add_class("impro-prompt-muted");
        } else {
            context.remove_class("impro-prompt-muted");
        }
        let keys = item.hotkeys();
        let mut chips = self.chips.children();
        while chips.len() < keys.len() {
            let chip = gtk::Label::new(None);
            chip.style_context().add_class("impro-prompt-hotkey");
            self.chips.pack_start(&chip, false, false, 0);
            chips.push(chip.upcast());
        }
        for (i, chip) in chips.iter().enumerate() {
            if let Some(key) = keys.get(i) {
                if let Some(label) = chip.downcast_ref::<gtk::Label>() {
                    label.set_text(key);
                }
                chip.show();
            } else {
                chip.hide();
            }
        }
        self.chips.set_visible(!keys.is_empty());
    }
}
struct Inner {
    widget: gtk::EventBox,
    frame: gtk::Box,
    entry: gtk::Entry,
    scroller: gtk::ScrolledWindow,
    list: gtk::ListBox,
    empty: gtk::Label,
    footer: gtk::Box,
    instructions: gtk::Box,
    provider: RefCell<Provider>,
    loader: RefCell<Option<Action>>,
    items: RefCell<Vec<PromptItem>>,
    rows: RefCell<Vec<ResultRow>>,
    selected: Cell<i32>,
    open: Cell<bool>,
    pending: RefCell<Option<glib::SourceId>>,
    host_closed: RefCell<Option<Action>>,
    busy: Cell<bool>,
    pending_choice: Cell<Option<gdk::ModifierType>>,
    closed: RefCell<Vec<Action>>,
    chosen: RefCell<Vec<Chosen>>,
    query_chosen: RefCell<Vec<QueryChosen>>,
    pointer: Cell<Option<(i32, i32)>>,
}
/// Cloneable GTK composition handle. Use `widget()` when adding to containers.
#[derive(Clone)]
pub struct Prompt(Rc<Inner>);
#[derive(Clone)]
pub(crate) struct WeakPrompt(std::rc::Weak<Inner>);
impl WeakPrompt {
    pub(crate) fn upgrade(&self) -> Option<Prompt> {
        self.0.upgrade().map(Prompt)
    }
}
impl std::ops::Deref for Prompt {
    type Target = gtk::EventBox;
    fn deref(&self) -> &Self::Target {
        &self.0.widget
    }
}
impl Prompt {
    pub fn new(provider: impl Fn(&str) -> Vec<PromptItem> + 'static, placeholder: &str) -> Self {
        let widget = gtk::EventBox::new();
        widget.set_visible_window(false);
        widget.style_context().add_class("impro-prompt-host");
        let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        frame.style_context().add_class("impro-prompt");
        let (top, right, bottom, left) = SHADOW_MARGINS;
        frame.set_margin_top(top);
        frame.set_margin_end(right);
        frame.set_margin_bottom(bottom);
        frame.set_margin_start(left);
        widget.add(&frame);
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some(placeholder));
        entry.set_hexpand(true);
        entry.style_context().add_class("impro-prompt-input");
        frame.pack_start(&entry, false, false, 0);
        let scroller = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_propagate_natural_height(true);
        scroller.style_context().add_class("impro-prompt-scroller");
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_activate_on_single_click(true);
        list.set_can_focus(false);
        list.style_context().add_class("impro-prompt-results");
        list.set_margin_top(12);
        list.set_margin_bottom(12);
        list.set_margin_start(12);
        list.set_margin_end(12);
        scroller.add(&list);
        frame.pack_start(&scroller, true, true, 0);
        let empty = gtk::Label::new(Some("No results found."));
        empty.set_no_show_all(true);
        empty.style_context().add_class("impro-prompt-empty");
        frame.pack_start(&empty, false, false, 0);
        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        footer.set_no_show_all(true);
        footer
            .style_context()
            .add_class("impro-prompt-instructions");
        let instructions = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        instructions.set_halign(gtk::Align::Center);
        instructions.set_hexpand(true);
        footer.add(&instructions);
        instructions.show();
        frame.pack_end(&footer, false, false, 0);
        let this = Self(Rc::new(Inner {
            widget,
            frame,
            entry,
            scroller,
            list,
            empty,
            footer,
            instructions,
            provider: RefCell::new(Rc::new(provider)),
            loader: RefCell::new(None),
            items: RefCell::new(vec![]),
            rows: RefCell::new(vec![]),
            selected: Cell::new(-1),
            open: Cell::new(false),
            pending: RefCell::new(None),
            host_closed: RefCell::new(None),
            busy: Cell::new(false),
            pending_choice: Cell::new(None),
            closed: RefCell::new(vec![]),
            chosen: RefCell::new(vec![]),
            query_chosen: RefCell::new(vec![]),
            pointer: Cell::new(None),
        }));
        this.set_instructions(DEFAULT_INSTRUCTIONS);
        this.0.frame.show_all();
        let weak = Rc::downgrade(&this.0);
        this.0.entry.connect_changed(move |e| {
            e.set_icon_from_icon_name(
                gtk::EntryIconPosition::Secondary,
                if e.text().is_empty() {
                    None
                } else {
                    Some("edit-clear-symbolic")
                },
            );
            if let Some(inner) = weak.upgrade() {
                let p = Prompt(inner);
                p.schedule();
            }
        });
        this.0.entry.connect_icon_press(|e, _, _| e.set_text(""));
        let weak = Rc::downgrade(&this.0);
        this.0.entry.connect_key_press_event(move |_, e| {
            weak.upgrade().map_or(glib::Propagation::Proceed, |inner| {
                if Prompt(inner).handle_key(e.keyval(), e.state()) {
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            })
        });
        let weak = Rc::downgrade(&this.0);
        this.0.list.connect_row_activated(move |_, row| {
            if let Some(inner) = weak.upgrade() {
                let p = Prompt(inner);
                p.select(row.index());
                p.choose(gtk::current_event_state().unwrap_or_else(gdk::ModifierType::empty));
            }
        });
        this.0.list.add_events(gdk::EventMask::POINTER_MOTION_MASK);
        let weak = Rc::downgrade(&this.0);
        this.0.list.connect_motion_notify_event(move |list, e| {
            if let Some(inner) = weak.upgrade() {
                let root = e.root();
                let pos = (root.0.round() as i32, root.1.round() as i32);
                if inner.pointer.replace(Some(pos)) != Some(pos) {
                    if let Some(row) = list.row_at_y(e.position().1 as i32) {
                        let p = Prompt(inner);
                        if row.index() < (p.0.items.borrow().len() as i32) {
                            p.0.selected.set(row.index());
                            list.select_row(Some(&row));
                        }
                    }
                }
            }
            glib::Propagation::Proceed
        });
        let weak = Rc::downgrade(&this.0);
        this.0.widget.connect_button_press_event(move |w, e| {
            if let Some(inner) = weak.upgrade() {
                if let Some((x, y)) = w.translate_coordinates(
                    &inner.frame,
                    e.position().0 as i32,
                    e.position().1 as i32,
                ) {
                    if x < 0
                        || y < 0
                        || x >= inner.frame.allocated_width()
                        || y >= inner.frame.allocated_height()
                    {
                        Prompt(inner).close();
                        return glib::Propagation::Stop;
                    }
                }
            }
            glib::Propagation::Proceed
        });
        let weak = Rc::downgrade(&this.0);
        this.0.widget.connect_destroy(move |_| {
            if let Some(inner) = weak.upgrade() {
                Prompt(inner).cancel_pending();
            }
        });
        this
    }
    pub(crate) fn downgrade(&self) -> WeakPrompt {
        WeakPrompt(Rc::downgrade(&self.0))
    }
    pub fn widget(&self) -> &gtk::EventBox {
        &self.0.widget
    }
    pub fn frame(&self) -> gtk::Box {
        self.0.frame.clone()
    }
    pub fn entry(&self) -> gtk::Entry {
        self.0.entry.clone()
    }
    pub fn scroller(&self) -> gtk::ScrolledWindow {
        self.0.scroller.clone()
    }
    pub fn list(&self) -> gtk::ListBox {
        self.0.list.clone()
    }
    pub fn empty(&self) -> gtk::Label {
        self.0.empty.clone()
    }
    pub fn footer(&self) -> gtk::Box {
        self.0.footer.clone()
    }
    pub fn rows(&self) -> Vec<ResultRow> {
        self.0.rows.borrow().clone()
    }
    pub fn query(&self) -> String {
        self.0.entry.text().into()
    }
    pub fn items(&self) -> Vec<PromptItem> {
        self.0.items.borrow().clone()
    }
    pub fn selected_index(&self) -> i32 {
        self.0.selected.get()
    }
    pub fn selected_item(&self) -> Option<PromptItem> {
        self.0
            .items
            .borrow()
            .get(self.selected_index() as usize)
            .cloned()
    }
    pub fn is_open(&self) -> bool {
        self.0.open.get()
    }
    pub fn set_provider(&self, provider: impl Fn(&str) -> Vec<PromptItem> + 'static) {
        *self.0.provider.borrow_mut() = Rc::new(provider);
        self.refresh();
    }
    pub fn set_loader(&self, loader: impl Fn() + 'static) {
        *self.0.loader.borrow_mut() = Some(Rc::new(loader));
    }
    pub fn set_placeholder(&self, text: &str) {
        self.0.entry.set_placeholder_text(Some(text));
    }
    pub fn set_empty_text(&self, text: &str) {
        self.0.empty.set_text(text);
    }
    pub fn set_instructions(&self, instructions: &[(&str, &str)]) {
        for c in self.0.instructions.children() {
            self.0.instructions.remove(&c);
        }
        for (key, text) in instructions {
            let l = gtk::Label::new(None);
            l.set_markup(&format!(
                "<b>{}</b> {}",
                highlight_markup(key, &[]),
                highlight_markup(text, &[])
            ));
            self.0.instructions.pack_start(&l, false, false, 0);
            l.show();
        }
        self.0.footer.set_visible(!instructions.is_empty());
    }
    pub fn set_query(&self, text: &str) {
        self.0.entry.set_text(text);
        self.0.entry.set_position(-1);
        self.refresh();
    }
    pub fn prepare(&self) {
        self.0.open.set(true);
        let loader = self.0.loader.borrow().clone();
        if let Some(load) = loader {
            load();
        }
        self.0.entry.set_text("");
        self.refresh();
    }
    pub fn focus_input(&self) {
        self.0.entry.grab_focus_without_selecting();
    }
    fn cancel_pending(&self) {
        let id = self.0.pending.borrow_mut().take();
        if let Some(id) = id {
            id.remove();
        }
    }
    fn schedule(&self) {
        if self.0.pending.borrow().is_some() {
            return;
        }
        let weak = Rc::downgrade(&self.0);
        let id = glib::idle_add_local_once(move || {
            if let Some(inner) = weak.upgrade() {
                inner.pending.borrow_mut().take();
                Prompt(inner).refresh();
            }
        });
        *self.0.pending.borrow_mut() = Some(id);
    }
    pub fn refresh(&self) {
        self.cancel_pending();
        let provider = self.0.provider.borrow().clone();
        let items = provider(&self.query());
        self.set_items(items);
    }
    /// Apply a worker result on the GTK thread. Rows and shortcut chips are reused.
    pub fn set_items(&self, mut items: Vec<PromptItem>) {
        items.truncate(MAX_RESULTS);
        let count = items.len();
        *self.0.items.borrow_mut() = items.clone();
        let mut rows = self.rows();
        while rows.len() < count {
            let row = ResultRow::new();
            self.0.rows.borrow_mut().push(row.clone());
            self.0.list.add(&row.widget);
            rows.push(row);
        }
        for (i, row) in rows.iter().enumerate() {
            if let Some(item) = items.get(i) {
                row.update(item);
                row.widget.show();
            } else {
                row.widget.hide();
            }
        }
        self.0.scroller.set_visible(count > 0);
        self.0.empty.set_visible(count == 0);
        self.0.selected.set(-1);
        self.select(if count > 0 { 0 } else { -1 });
        {
            let a = self.0.scroller.vadjustment();
            a.set_value(0.);
        }
    }
    pub fn select(&self, index: i32) {
        let len = self.0.items.borrow().len() as i32;
        if len == 0 {
            self.0.selected.set(-1);
            self.0.list.unselect_all();
            return;
        }
        let i = index.clamp(0, len - 1);
        self.0.selected.set(i);
        let row = self.0.rows.borrow()[i as usize].widget.clone();
        self.0.list.select_row(Some(&row));
        {
            let a = self.0.scroller.vadjustment();
            let rect = row.allocation();
            let top = (rect.y() + 12) as f64;
            let bottom = top + rect.height() as f64;
            if i == 0 {
                a.set_value(0.);
            } else if top < a.value() {
                a.set_value(top);
            } else if bottom > a.value() + a.page_size() {
                a.set_value(bottom - a.page_size());
            }
        }
    }
    pub fn r#move(&self, delta: i32, wrap: bool) {
        self.move_selection(delta, wrap);
    }
    pub fn move_selection(&self, delta: i32, wrap: bool) {
        let len = self.0.items.borrow().len() as i32;
        if len > 0 {
            let index = self.selected_index() + delta;
            self.select(if wrap { index.rem_euclid(len) } else { index });
        }
    }
    pub fn choose(&self, modifiers: gdk::ModifierType) {
        if self.0.pending.borrow().is_some() {
            self.refresh();
        }
        if self.0.busy.get() {
            self.0.pending_choice.set(Some(modifiers));
            return;
        }
        let item = self.selected_item();
        let query = self.query();
        if item.is_none() && query.trim().is_empty() {
            return;
        }
        let modifiers = modifiers
            & (gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::SHIFT_MASK
                | gdk::ModifierType::MOD1_MASK);
        self.close();
        if let Some(item) = item {
            let callbacks = self.0.chosen.borrow().clone();
            for callback in callbacks {
                callback(&item, modifiers);
            }
        } else {
            let callbacks = self.0.query_chosen.borrow().clone();
            for callback in callbacks {
                callback(&query, modifiers);
            }
        }
    }
    pub fn close(&self) {
        self.cancel_pending();
        if !self.0.open.replace(false) {
            return;
        }
        self.0.pending_choice.set(None);
        self.0.busy.set(false);
        let host = self.0.host_closed.borrow().clone();
        if let Some(f) = host {
            f();
        }
        let callbacks = self.0.closed.borrow().clone();
        for callback in callbacks {
            callback();
        }
    }
    pub(crate) fn set_host_closed(&self, callback: impl Fn() + 'static) {
        *self.0.host_closed.borrow_mut() = Some(Rc::new(callback));
    }
    pub(crate) fn refresh_after_index(&self) {
        let pending = self.0.pending_choice.take();
        self.refresh();
        if let Some(mods) = pending {
            self.choose(mods);
        }
    }
    pub(crate) fn set_search_pending(&self, pending: bool) {
        self.0.busy.set(pending);
        self.0.pending_choice.set(None);
    }
    pub fn search_pending(&self) -> bool {
        self.0.busy.get()
    }
    pub(crate) fn finish_search(&self, items: Vec<PromptItem>) {
        self.0.busy.set(false);
        self.set_items(items);
        if let Some(mods) = self.0.pending_choice.take() {
            self.choose(mods);
        }
    }
    pub fn connect_closed(&self, callback: impl Fn() + 'static) {
        self.0.closed.borrow_mut().push(Rc::new(callback));
    }
    pub fn connect_chosen(&self, callback: impl Fn(&PromptItem, gdk::ModifierType) + 'static) {
        self.0.chosen.borrow_mut().push(Rc::new(callback));
    }
    pub fn connect_query_chosen(&self, callback: impl Fn(&str, gdk::ModifierType) + 'static) {
        self.0.query_chosen.borrow_mut().push(Rc::new(callback));
    }
    #[allow(non_upper_case_globals)]
    pub fn handle_key(&self, key: gdk::keys::Key, state: gdk::ModifierType) -> bool {
        use gdk::keys::constants::{
            Down, End, Escape, Home, ISO_Enter, ISO_Left_Tab, KP_Down, KP_Enter, KP_Up, Page_Down,
            Page_Up, Return, Tab, Up,
        };
        if key == Escape {
            self.close();
            return true;
        }
        if [Return, KP_Enter, ISO_Enter].contains(&key) {
            self.choose(state);
            return true;
        }
        if [Tab, ISO_Left_Tab].contains(&key) {
            return true;
        }
        let state = state & gtk::accelerator_get_default_mod_mask();
        if !(state & !gdk::ModifierType::SHIFT_MASK).is_empty() {
            return false;
        }
        if self.0.pending.borrow().is_some() {
            self.refresh();
        }
        let page = {
            let a = self.0.scroller.vadjustment();
            let rows = self.rows();
            let height = rows
                .get(self.selected_index().max(0) as usize)
                .map_or(0, |r| r.widget.allocated_height());
            if height <= 1 || a.page_size() <= 0. {
                10
            } else {
                ((a.page_size() / height as f64) as i32 - 1).max(1)
            }
        };
        match key {
            Up | KP_Up => self.move_selection(-1, true),
            Down | KP_Down => self.move_selection(1, true),
            Page_Up => self.move_selection(-page, false),
            Page_Down => self.move_selection(page, false),
            Home => self.select(0),
            End => {
                let last = self.0.items.borrow().len() as i32 - 1;
                self.select(last);
            }
            _ => return false,
        }
        true
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(id) = self.pending.get_mut().take() {
            id.remove();
        }
    }
}
pub fn css(ui: &impro_core::themes::UiColors) -> String {
    let mut ui = ui.clone();
    let shadow = if ui["overlay"] == "#ffffff" {
        "0 1.8px 7.3px alpha(#000000, 0.071), 0 6.3px 24.7px alpha(#000000, 0.112), 0 16px 40px alpha(#000000, 0.2)"
    } else {
        "0 1.8px 7.3px alpha(#000000, 0.028), 0 6.3px 24.7px alpha(#000000, 0.042), 0 15px 30px alpha(#000000, 0.1)"
    };
    ui.insert("shadow".into(), shadow.into());
    impro_ui::render_css(include_str!("palette.css"), &ui)
}
