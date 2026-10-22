//! Selection formatting for editable live notes.
//!
//! A small popover over the selection offers bold, italic, strikethrough, inline
//! code, link, "turn into" and copy. It exists only for note views whose live
//! renderer is on, follows the selection after a short debounce and stays out of
//! the way of drags, scrolling, focus loss and the slash menu.
//!
//! The editor keeps calling [`sync`] when a tab's mode or path may have changed
//! and [`set_enabled`] for the notes-toolbar preference. Both work from the
//! renderers registered in [`crate::livemd`], so the toolbar needs no handle on
//! the editor's tab type.
use crate::{livemd, slashmenu};
use gtk::prelude::*;
use impro_core::notes;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::PathBuf,
    rc::{Rc, Weak},
    time::Duration,
};

const ENABLED_KEY: &str = "impro-selection-toolbar-enabled";
const CONTROLS: [(&str, &str, &str, &str); 7] = [
    ("bold", "Bold (Ctrl+B)", "format-text-bold-symbolic", "B"),
    ("italic", "Italic (Ctrl+I)", "format-text-italic-symbolic", "I"),
    ("strike", "Strikethrough (~~)", "format-text-strikethrough-symbolic", "S"),
    ("inline_code", "Inline code (`)", "", "‹›"),
    ("link", "Link ([text](url))", "insert-link-symbolic", "↗"),
    ("turn", "Turn into (Text, H1, H2, H3, lists, quote)", "", "H⌄"),
    ("copy", "Copy (Ctrl+C)", "edit-copy-symbolic", "⧉"),
];

/// Follow a tab's mode, path and lifetime: create or remove its toolbar.
pub fn sync<T>(_tab: &T) {
    for view in livemd::registered_views() {
        refresh_for(&view);
    }
}

/// Apply the combined notes feature and toolbar settings to every view under
/// `panel`, existing and future ones.
pub fn set_enabled<T: IsA<gtk::Widget>>(panel: &T, enabled: bool) {
    let widget: &gtk::Widget = panel.upcast_ref();
    unsafe { widget.set_data(ENABLED_KEY, enabled) };
    for view in livemd::registered_views() {
        if !view.is_ancestor(widget) {
            continue;
        }
        if let Some(toolbar) = of(&view) {
            if enabled {
                toolbar.0.dismissed.replace(None);
                toolbar.0.queue();
            } else {
                toolbar.dismiss();
            }
        }
    }
}

/// The selection toolbar of `view`, if its renderer wants one.
pub fn of(view: &gtk::TextView) -> Option<SelectionToolbar> {
    livemd::for_view(view)?.toolbar()
}

/// Create or release the toolbar of `view` after its renderer changed state.
pub(crate) fn refresh_for(view: &gtk::TextView) {
    let Some(live) = livemd::for_view(view) else {
        return;
    };
    let path = live.path();
    let wanted = live.enabled() && notes::is_note_path(path.as_deref());
    let current = live.toolbar();
    match (wanted, current) {
        (true, None) => {
            let toolbar = SelectionToolbar::new(&live);
            live.set_toolbar(Some(toolbar));
        }
        (false, Some(toolbar)) => {
            toolbar.destroy();
            live.set_toolbar(None);
        }
        (true, Some(toolbar)) => toolbar.hook_slash(),
        _ => {}
    }
}

/// Panel-level gate stored on the nearest ancestor that was given a setting.
fn gate(view: &gtk::TextView) -> bool {
    let mut widget: Option<gtk::Widget> = view.parent();
    while let Some(w) = widget {
        let value = unsafe { w.data::<bool>(ENABLED_KEY).map(|p| *p.as_ref()) };
        if let Some(value) = value {
            return value;
        }
        widget = w.parent();
    }
    true
}

struct Inner {
    live: Weak<livemd::LiveInner>,
    view: gtk::TextView,
    buffer: gtk::TextBuffer,
    popover: gtk::Popover,
    menu: gtk::Popover,
    buttons: BTreeMap<&'static str, gtk::Button>,
    menu_buttons: Vec<(&'static str, gtk::Button)>,
    handlers: RefCell<Vec<(glib::Object, glib::SignalHandlerId)>>,
    top_handlers: RefCell<Vec<(glib::Object, glib::SignalHandlerId)>>,
    slash_hook: RefCell<Option<(glib::WeakRef<gtk::Popover>, glib::SignalHandlerId)>>,
    timeout: RefCell<Option<glib::SourceId>>,
    dragging: Cell<bool>,
    editing: Cell<bool>,
    destroyed: Cell<bool>,
    dismissed: RefCell<Option<(i32, i32)>>,
    weak: Weak<Inner>,
}

/// Selection toolbar of one live note view.
#[derive(Clone)]
pub struct SelectionToolbar(Rc<Inner>);

impl SelectionToolbar {
    pub(crate) fn new(live: &livemd::LiveMarkdown) -> Self {
        let view = live.view();
        let buffer = view.buffer().expect("text view buffer");
        let popover = gtk::Popover::new(Some(&view));
        popover.set_modal(false);
        popover.set_can_focus(false);
        popover.set_position(gtk::PositionType::Top);
        popover.style_context().add_class("impro-selection-toolbar");
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        popover.add(&row);
        let theme = gtk::IconTheme::default();
        let mut buttons = BTreeMap::new();
        let mut order = Vec::new();
        for (command, tooltip, icon, glyph) in CONTROLS {
            if command == "turn" || command == "copy" {
                let separator = gtk::Separator::new(gtk::Orientation::Vertical);
                separator.set_valign(gtk::Align::Center);
                separator.set_size_request(1, 16);
                row.pack_start(&separator, false, false, 4);
            }
            let button = gtk::Button::new();
            button.set_can_focus(false);
            button.set_relief(gtk::ReliefStyle::None);
            button.set_size_request(28, 28);
            let child: gtk::Widget = if !icon.is_empty() && theme.as_ref().is_some_and(|t| t.has_icon(icon)) {
                let image = gtk::Image::from_icon_name(Some(icon), gtk::IconSize::Menu);
                image.set_pixel_size(16);
                image.upcast()
            } else {
                gtk::Label::new(Some(glyph)).upcast()
            };
            button.add(&child);
            button.set_tooltip_text(Some(tooltip));
            if let Some(accessible) = button.accessible() {
                gtk::prelude::AtkObjectExt::set_name(
                    &accessible,
                    tooltip.split(" (").next().unwrap_or(tooltip),
                );
            }
            let weak_child = child.downgrade();
            impro_ui::motion::StateFader::new(
                &button,
                move |amount| {
                    if let Some(child) = weak_child.upgrade() {
                        child.set_opacity(0.85 + 0.15 * amount);
                    }
                },
                128.0,
                0.0,
            );
            buttons.insert(command, button.clone());
            order.push(button.clone());
            row.pack_start(&button, false, false, 0);
        }
        let menu = gtk::Popover::new(buttons.get("turn"));
        menu.set_modal(false);
        menu.set_can_focus(false);
        menu.style_context().add_class("impro-selection-menu");
        let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        menu.add(&menu_box);
        let mut menu_buttons = Vec::new();
        for command in notes::SLASH_COMMANDS.iter() {
            if !notes::TURN_COMMANDS.contains(&command.id) {
                continue;
            }
            let button = gtk::Button::with_label(&format!("{}   {}", command.glyph, command.label));
            button.set_can_focus(false);
            if let Some(label) = button.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
                label.set_xalign(0.0);
            }
            menu_box.add(&button);
            menu_buttons.push((command.id, button));
        }
        let inner = Rc::new_cyclic(|weak| Inner {
            live: live.downgrade(),
            view: view.clone(),
            buffer: buffer.clone(),
            popover,
            menu,
            buttons,
            menu_buttons,
            handlers: RefCell::new(Vec::new()),
            top_handlers: RefCell::new(Vec::new()),
            slash_hook: RefCell::new(None),
            timeout: RefCell::new(None),
            dragging: Cell::new(false),
            editing: Cell::new(false),
            destroyed: Cell::new(false),
            dismissed: RefCell::new(None),
            weak: weak.clone(),
        });
        for (command, button) in &inner.buttons {
            let weak = inner.weak.clone();
            let command = *command;
            button.connect_clicked(move |_| {
                if let Some(s) = weak.upgrade() {
                    s.apply(command);
                }
            });
        }
        for (command, button) in &inner.menu_buttons {
            let weak = inner.weak.clone();
            let command = *command;
            button.connect_clicked(move |_| {
                if let Some(s) = weak.upgrade() {
                    s.apply(command);
                }
            });
        }
        inner.connect();
        inner.hook_slash();
        Self(inner)
    }
    pub fn popover(&self) -> gtk::Popover {
        self.0.popover.clone()
    }
    pub fn menu(&self) -> gtk::Popover {
        self.0.menu.clone()
    }
    /// `bold`, `italic`, `strike`, `inline_code`, `link`, `turn`, `copy`.
    pub fn buttons(&self) -> BTreeMap<&'static str, gtk::Button> {
        self.0.buttons.clone()
    }
    /// Entries of the "Turn into" menu in command order.
    pub fn menu_buttons(&self) -> Vec<(&'static str, gtk::Button)> {
        self.0.menu_buttons.clone()
    }
    /// Run a toolbar command (button `clicked` does the same).
    pub fn apply(&self, command: &str) {
        self.0.apply(command);
    }
    pub fn queue(&self) {
        self.0.queue();
    }
    pub fn hide(&self) {
        self.0.hide();
    }
    pub fn dismiss(&self) {
        self.0.dismiss();
    }
    pub fn is_destroyed(&self) -> bool {
        self.0.destroyed.get()
    }
    /// A debounce timeout is pending.
    pub fn timeout_pending(&self) -> bool {
        self.0.timeout.borrow().is_some()
    }
    pub fn eligible(&self) -> bool {
        self.0.eligible()
    }
    pub fn destroy(&self) {
        self.0.destroy();
    }
    fn hook_slash(&self) {
        self.0.hook_slash();
    }
    /// Offer a key to the toolbar (Escape, Ctrl+B, Ctrl+I).
    pub fn handle_key(&self, event: &gdk::EventKey) -> bool {
        self.0.handle_key(event)
    }
}

impl Inner {
    fn track<O: IsA<glib::Object>>(&self, object: &O, id: glib::SignalHandlerId) {
        self.handlers.borrow_mut().push((object.clone().upcast(), id));
    }

    fn connect(&self) {
        let (buffer, view) = (&self.buffer, &self.view);
        let w = self.weak.clone();
        let id = buffer.connect_mark_set(move |_, _, mark| {
            if let Some(s) = w.upgrade() {
                if matches!(mark.name().as_deref(), Some("insert" | "selection_bound")) {
                    s.queue();
                }
            }
        });
        self.track(buffer, id);
        let w = self.weak.clone();
        let id = buffer.connect_changed(move |_| {
            if let Some(s) = w.upgrade() {
                if !s.editing.get() {
                    s.dismiss();
                }
            }
        });
        self.track(buffer, id);
        let w = self.weak.clone();
        let id = view.connect_button_press_event(move |_, event| {
            if let Some(s) = w.upgrade() {
                if event.button() == 1 {
                    s.dragging.set(true);
                    s.hide();
                }
            }
            glib::Propagation::Proceed
        });
        self.track(view, id);
        let w = self.weak.clone();
        let id = view.connect_button_release_event(move |_, event| {
            if let Some(s) = w.upgrade() {
                if event.button() == 1 {
                    s.dragging.set(false);
                    s.dismissed.replace(None);
                    s.queue();
                }
            }
            glib::Propagation::Proceed
        });
        self.track(view, id);
        let w = self.weak.clone();
        let id = view.connect_key_press_event(move |_, event| match w.upgrade() {
            Some(s) if s.handle_key(event) => glib::Propagation::Stop,
            _ => glib::Propagation::Proceed,
        });
        self.track(view, id);
        let w = self.weak.clone();
        let id = view.connect_focus_out_event(move |_, _| {
            if let Some(s) = w.upgrade() {
                s.dismiss();
            }
            glib::Propagation::Proceed
        });
        self.track(view, id);
        let w = self.weak.clone();
        let id = view.connect_focus_in_event(move |_, _| {
            if let Some(s) = w.upgrade() {
                s.dismissed.replace(None);
                s.queue();
            }
            glib::Propagation::Proceed
        });
        self.track(view, id);
        let w = self.weak.clone();
        let id = view.connect_notify_local(Some("editable"), move |_, _| {
            if let Some(s) = w.upgrade() {
                s.queue();
            }
        });
        self.track(view, id);
        let w = self.weak.clone();
        let id = view.connect_unmap(move |_| {
            if let Some(s) = w.upgrade() {
                s.dismiss();
            }
        });
        self.track(view, id);
        let w = self.weak.clone();
        let id = view.connect_destroy(move |_| {
            if let Some(s) = w.upgrade() {
                s.destroy();
            }
        });
        self.track(view, id);
        let w = self.weak.clone();
        let id = view.connect_hierarchy_changed(move |_, _| {
            if let Some(s) = w.upgrade() {
                s.hierarchy();
            }
        });
        self.track(view, id);
        if let Some(parent) = view.parent().and_then(|p| p.downcast::<gtk::ScrolledWindow>().ok()) {
            for adjustment in [parent.vadjustment(), parent.hadjustment()] {
                let w = self.weak.clone();
                let id = adjustment.connect_value_changed(move |_| {
                    if let Some(s) = w.upgrade() {
                        s.scrolled();
                    }
                });
                self.track(&adjustment, id);
            }
        }
        self.hierarchy();
    }

    fn hierarchy(&self) {
        for (object, id) in self.top_handlers.borrow_mut().drain(..) {
            object.disconnect(id);
        }
        let Some(top) = self.view.toplevel().and_then(|t| t.downcast::<gtk::Window>().ok()) else {
            return;
        };
        let w = self.weak.clone();
        let id = top.connect_key_press_event(move |top, event| {
            // Window accelerators run before TextView bindings. Keep Ctrl+B local
            // to editable notes, leaving the global explorer shortcut elsewhere.
            if let Some(s) = w.upgrade() {
                let focus = top.focused_widget();
                if focus.as_ref() == Some(s.view.upcast_ref()) && s.handle_key(event) {
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        self.top_handlers.borrow_mut().push((top.clone().upcast(), id));
        let w = self.weak.clone();
        let id = top.connect_focus_out_event(move |_, _| {
            if let Some(s) = w.upgrade() {
                s.dismiss();
            }
            glib::Propagation::Proceed
        });
        self.top_handlers.borrow_mut().push((top.upcast(), id));
    }

    fn hook_slash(&self) {
        let slash = slashmenu::for_view(&self.view).map(|m| m.popover());
        let hooked = self.slash_hook.borrow().as_ref().and_then(|(w, _)| w.upgrade());
        if hooked == slash {
            return;
        }
        if let Some((weak, id)) = self.slash_hook.borrow_mut().take() {
            if let Some(popover) = weak.upgrade() {
                popover.disconnect(id);
            }
        }
        if let Some(popover) = slash {
            let w = self.weak.clone();
            let id = popover.connect_show(move |_| {
                if let Some(s) = w.upgrade() {
                    s.dismiss();
                }
            });
            *self.slash_hook.borrow_mut() = Some((popover.downgrade(), id));
        }
    }

    fn live(&self) -> Option<livemd::LiveMarkdown> {
        livemd::LiveMarkdown::from_inner(self.live.upgrade()?)
    }

    fn path(&self) -> Option<PathBuf> {
        self.live().and_then(|l| l.path())
    }

    fn selection(&self) -> Option<(i32, i32)> {
        self.buffer
            .selection_bounds()
            .map(|(a, b)| (a.offset(), b.offset()))
    }

    fn eligible(&self) -> bool {
        let Some(live) = self.live() else {
            return false;
        };
        let slash_open = slashmenu::for_view(&self.view).is_some_and(|m| m.opened());
        !self.destroyed.get()
            && self.view.is_editable()
            && self.view.is_mapped()
            && self.view.has_focus()
            && live.enabled()
            && notes::is_note_path(self.path().as_deref())
            && gate(&self.view)
            && !slash_open
    }

    fn queue(&self) {
        if self.destroyed.get() || self.editing.get() {
            return;
        }
        if let Some(id) = self.timeout.borrow_mut().take() {
            id.remove();
        }
        if self.selection() != *self.dismissed.borrow() {
            self.dismissed.replace(None);
        }
        if self.selection().is_none() || !self.eligible() {
            self.hide();
        } else {
            let weak = self.weak.clone();
            let id = glib::timeout_add_local_once(Duration::from_millis(150), move || {
                if let Some(s) = weak.upgrade() {
                    s.timeout.borrow_mut().take();
                    s.refresh();
                }
            });
            *self.timeout.borrow_mut() = Some(id);
        }
    }

    /// Window rectangle the toolbar points at, or `None` when the selection is offscreen.
    fn anchor(&self) -> Option<gdk::Rectangle> {
        let (start, mut end) = self.buffer.selection_bounds()?;
        // The end iterator is exclusive, and may be at the next line's start.
        end.backward_char();
        let first = self.view.iter_location(&start);
        let last = self.view.iter_location(&end);
        let visible = self.view.visible_rect();
        if last.y() + last.height() <= visible.y() || first.y() >= visible.y() + visible.height() {
            return None;
        }
        let mut rect;
        if first.y() == last.y() {
            if last.x() + last.width() <= visible.x() || first.x() >= visible.x() + visible.width() {
                return None;
            }
            let x = first.x().max(visible.x());
            let right = (last.x() + last.width().max(1)).min(visible.x() + visible.width());
            rect = gdk::Rectangle::new(x, first.y(), 1.max(right - x), first.height());
        } else {
            // A selection can span the whole viewport with both ends offscreen.
            rect = first;
            if rect.y() < visible.y() {
                if let Some(iter) = self.view.iter_at_location(visible.x(), visible.y()) {
                    rect = self.view.iter_location(&iter);
                }
            }
            let x = visible.x().max(rect.x().min(visible.x() + visible.width() - 1));
            rect = gdk::Rectangle::new(x, rect.y(), rect.width().max(1), rect.height());
        }
        let (x, y) = self
            .view
            .buffer_to_window_coords(gtk::TextWindowType::Widget, rect.x(), rect.y());
        Some(gdk::Rectangle::new(x, y, rect.width(), rect.height()))
    }

    fn scrolled(&self) {
        if !self.popover.is_visible() {
            return;
        }
        if self.anchor().is_none() {
            self.dismiss();
        } else {
            if let Some(id) = self.timeout.borrow_mut().take() {
                id.remove();
            }
            self.refresh();
        }
    }

    fn refresh(&self) {
        let selection = self.selection();
        let rect = if selection.is_some() && self.eligible() {
            self.anchor()
        } else {
            None
        };
        let (Some(rect), Some(selection)) = (rect, selection) else {
            self.hide();
            return;
        };
        if self.dragging.get() || Some(selection) == *self.dismissed.borrow() {
            self.hide();
            return;
        }
        let document = self
            .buffer
            .text(&self.buffer.start_iter(), &self.buffer.end_iter(), true)
            .map(|s| s.to_string())
            .unwrap_or_default();
        let active = notes::selection_formats(&document, selection.0 as usize, selection.1 as usize);
        for (command, button) in &self.buttons {
            let context = button.style_context();
            if active.contains_key(command) {
                context.add_class("active");
            } else {
                context.remove_class("active");
            }
            if let Some(accessible) = button.accessible() {
                gtk::prelude::AtkObjectExt::set_description(
                    &accessible,
                    if active.contains_key(command) { "Active" } else { "" },
                );
            }
        }
        let height = self.popover.preferred_height().1;
        self.popover.set_position(if rect.y() >= 48.max(height) {
            gtk::PositionType::Top
        } else {
            gtk::PositionType::Bottom
        });
        self.popover.set_pointing_to(&rect);
        let was_visible = self.popover.is_visible();
        if !was_visible {
            self.popover.set_opacity(0.0);
        }
        self.popover.show_all();
        if !was_visible {
            impro_ui::motion::slide_in(&self.popover, 8, 128, None);
        }
    }

    fn apply(&self, command: &str) {
        let Some(selection) = self.selection().filter(|_| self.eligible()) else {
            self.hide();
            return;
        };
        if command == "turn" {
            self.menu.set_opacity(0.0);
            self.menu.show_all();
            impro_ui::motion::slide_in(&self.menu, 8, 128, None);
            return;
        }
        self.menu.hide();
        if command == "copy" {
            if let Some(display) = gdk::Display::default() {
                if let Some(clipboard) = gtk::Clipboard::default(&display) {
                    self.buffer.copy_clipboard(&clipboard);
                }
            }
            return;
        }
        let document = self
            .buffer
            .text(&self.buffer.start_iter(), &self.buffer.end_iter(), true)
            .map(|s| s.to_string())
            .unwrap_or_default();
        let Ok(edit) =
            notes::apply_selection_command(command, &document, selection.0 as isize, selection.1 as isize)
        else {
            return;
        };
        self.editing.set(true);
        impro_ui::notes::apply_edit(&self.buffer, &edit);
        self.editing.set(false);
        self.dismissed.replace(None);
        self.queue();
    }

    fn handle_key(&self, event: &gdk::EventKey) -> bool {
        use gdk::keys::constants as k;
        let key = event.keyval();
        if key == k::Escape && self.popover.is_visible() {
            self.dismiss();
            return true;
        }
        let modifiers = event.state() & gtk::accelerator_get_default_mod_mask();
        if modifiers == gdk::ModifierType::CONTROL_MASK && self.eligible() {
            let command = if key == k::b || key == k::B {
                Some("bold")
            } else if key == k::i || key == k::I {
                Some("italic")
            } else {
                None
            };
            if let Some(command) = command {
                if self.selection().is_some() {
                    self.apply(command);
                    return true;
                }
            }
        }
        false
    }

    fn hide(&self) {
        if let Some(id) = self.timeout.borrow_mut().take() {
            id.remove();
        }
        self.menu.hide();
        self.popover.hide();
    }

    fn dismiss(&self) {
        self.dismissed.replace(self.selection());
        self.hide();
    }

    fn destroy(&self) {
        if self.destroyed.replace(true) {
            return;
        }
        self.hide();
        for (object, id) in self.handlers.borrow_mut().drain(..) {
            object.disconnect(id);
        }
        for (object, id) in self.top_handlers.borrow_mut().drain(..) {
            object.disconnect(id);
        }
        if let Some((weak, id)) = self.slash_hook.borrow_mut().take() {
            if let Some(popover) = weak.upgrade() {
                popover.disconnect(id);
            }
        }
        unsafe {
            self.menu.destroy();
            self.popover.destroy();
        }
    }
}
