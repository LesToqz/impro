//! Realizing the widgets a painted block asked for: list markers, task boxes,
//! quote bars, dividers, images, code cards and tables.
use super::{model::Want, CodeInfo, ImageState, Inner, Role, Slot};
use gtk::prelude::*;
use std::rc::Rc;

/// Widgets of a code card's tool strip, reachable from tests and handlers.
pub struct Tools {
    pub copy: gtk::Button,
    pub language: gtk::EventBox,
    pub visible: std::cell::Cell<bool>,
}

/// The Copy button of a code card's tool strip.
pub fn tools_copy_button(tools: &gtk::Widget) -> Option<gtk::Button> {
    unsafe {
        tools
            .data::<Rc<Tools>>("impro-live-tools")
            .map(|t| t.as_ref().copy.clone())
    }
}
/// The language selector of a code card's tool strip.
pub fn tools_language_button(tools: &gtk::Widget) -> Option<gtk::EventBox> {
    unsafe {
        tools
            .data::<Rc<Tools>>("impro-live-tools")
            .map(|t| t.as_ref().language.clone())
    }
}
/// Whether the Copy button is currently revealed.
pub fn tools_copy_visible(tools: &gtk::Widget) -> bool {
    unsafe {
        tools
            .data::<Rc<Tools>>("impro-live-tools")
            .is_some_and(|t| t.as_ref().visible.get())
    }
}

impl Inner {
    /// Create the widgets of block `index` (painted, none realized yet).
    pub(super) fn realize(&self, index: usize) {
        let (id, wants, code, table) = {
            let blocks = self.blocks.borrow();
            let Some(deco) = blocks.items.get(index) else {
                return;
            };
            if !deco.widgets.is_empty() || !deco.painted {
                return;
            }
            (
                deco.id,
                deco.wants.clone(),
                deco.code.clone(),
                deco.table.clone(),
            )
        };
        let editable = self.view.is_editable();
        let mut slots: Vec<Slot> = Vec::new();
        let mut image_index = 0;
        for (rel, want) in wants {
            match want {
                Want::Task { checked, toggle, x } => {
                    let check = gtk::CheckButton::new();
                    check.style_context().add_class("live-md-task");
                    check.set_active(checked);
                    check.set_can_focus(false);
                    check.set_sensitive(editable);
                    check.set_tooltip_text(Some("Toggle task"));
                    name_widget(check.upcast_ref(), "Toggle task");
                    let weak = self.weak.clone();
                    check.connect_toggled(move |_| {
                        if let Some(s) = weak.upgrade() {
                            s.toggle_task_id(id, toggle);
                        }
                    });
                    slots.push(self.attach(check.upcast_ref(), rel, Role::Marker, x));
                }
                Want::Bullet { text, x } => {
                    let label = gtk::Label::new(Some(&text));
                    label.set_xalign(0.5);
                    label.set_size_request(16, -1);
                    label.style_context().add_class("live-md-marker");
                    slots.push(self.attach(label.upcast_ref(), rel, Role::Marker, x));
                }
                Want::Quote => {
                    let bar = gtk::Separator::new(gtk::Orientation::Vertical);
                    bar.style_context().add_class("live-md-quote");
                    slots.push(self.attach(bar.upcast_ref(), rel, Role::Quote, 0));
                }
                Want::Rule => {
                    let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
                    rule.style_context().add_class("live-md-rule");
                    slots.push(self.attach(rule.upcast_ref(), rel, Role::Rule, 24));
                }
                Want::Image { .. } => {
                    let k = image_index;
                    image_index += 1;
                    let state = {
                        let blocks = self.blocks.borrow();
                        blocks
                            .items
                            .get(index)
                            .and_then(|d| d.images.get(k))
                            .map(|i| match &i.state {
                                ImageState::Ready(l) => Some(l.pixbuf.clone()),
                                _ => None,
                            })
                    };
                    match state {
                        Some(Some(pixbuf)) => {
                            let image = gtk::Image::from_pixbuf(Some(&pixbuf));
                            slots.push(self.attach(
                                image.upcast_ref(),
                                rel,
                                Role::Image,
                                pixbuf.height(),
                            ));
                        }
                        Some(None) => self.request_image(id, k),
                        None => {}
                    }
                }
                Want::Code => {
                    if let Some(info) = &code {
                        self.code_widgets(&mut slots, id, info, editable);
                    }
                }
                Want::Table => {}
            }
        }
        let attached = {
            let mut blocks = self.blocks.borrow_mut();
            match blocks.items.get_mut(index).filter(|d| d.id == id) {
                Some(deco) => {
                    deco.widgets.extend(slots.iter().cloned());
                    true
                }
                None => false,
            }
        };
        if !attached {
            for slot in slots {
                self.release_widget(&slot.widget);
            }
            return;
        }
        if let Some(table) = table {
            self.tables().build(table);
            // A caret that landed in the block before its grid existed (the
            // grid is built when the block nears the viewport) enters it now.
            self.queue_caret();
            self.queue_fill();
        }
        if code.is_some() {
            self.request_highlight(id);
        }
    }

    /// Drop the widgets of block `index` (its tags stay).
    pub(super) fn release(&self, index: usize) {
        let id = match self.blocks.borrow().items.get(index) {
            Some(deco) if !deco.widgets.is_empty() => deco.id,
            _ => return,
        };
        let tables = self.tables.borrow().clone();
        if let Some(tables) = &tables {
            // Keep the table that holds the cell being edited.
            if tables
                .views()
                .iter()
                .any(|v| v.block().id == id && v.focused())
            {
                return;
            }
        }
        let slots = {
            let mut blocks = self.blocks.borrow_mut();
            match blocks.items.get_mut(index) {
                Some(deco) => std::mem::take(&mut deco.widgets),
                None => return,
            }
        };
        if let Some(tables) = &tables {
            if slots.iter().any(|s| s.role == Role::Table) {
                tables.remove(id);
            }
        }
        for slot in slots {
            self.release_widget(&slot.widget);
        }
    }

    fn code_widgets(&self, slots: &mut Vec<Slot>, id: u64, info: &CodeInfo, editable: bool) {
        let card = gtk::Frame::new(None);
        card.style_context().add_class("live-md-code-card");
        slots.push(self.attach(card.upcast_ref(), 0, Role::CodeCard, 0));
        for (side, role) in [("left", Role::CodeSideLeft), ("right", Role::CodeSideRight)] {
            let padding = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            padding.style_context().add_class("live-md-code-side");
            let _ = side;
            slots.push(self.attach(padding.upcast_ref(), 0, role, 0));
        }
        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        footer.style_context().add_class("live-md-code-footer");
        slots.push(self.attach(footer.upcast_ref(), 0, Role::CodeFooter, 0));
        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        tools.style_context().add_class("live-md-tools");
        let label = gtk::Label::new(Some(if info.language.is_empty() {
            "Plain text"
        } else {
            &info.language
        }));
        label.set_xalign(0.0);
        let language = gtk::EventBox::new();
        language.add(&label);
        language.set_visible_window(false);
        language.set_can_focus(true);
        language.set_sensitive(editable);
        language.set_tooltip_text(Some("Change code language"));
        name_widget(language.upcast_ref(), "Change code language");
        let weak = self.weak.clone();
        language.connect_button_press_event(move |button, event| {
            if event.button() == 1 {
                if let Some(s) = weak.upgrade() {
                    s.choose_language(button.upcast_ref(), id);
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        let weak = self.weak.clone();
        language.connect_key_press_event(move |button, event| {
            use gdk::keys::constants as k;
            let key = event.keyval();
            if key == k::Return || key == k::KP_Enter || key == k::space {
                if let Some(s) = weak.upgrade() {
                    s.choose_language(button.upcast_ref(), id);
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        let button = gtk::Button::with_label("Copy");
        button.set_can_focus(true);
        button.set_tooltip_text(Some("Copy code"));
        name_widget(button.upcast_ref(), "Copy code");
        let (first_len, end_rel) = (info.first_len, info.end_rel);
        let weak = self.weak.clone();
        button.connect_clicked(move |b| {
            if let Some(s) = weak.upgrade() {
                s.copy_code(b, id, first_len, end_rel);
            }
        });
        tools.pack_end(&button, false, false, 0);
        tools.pack_start(&language, false, false, 0);
        button.set_opacity(0.0);
        let state = Rc::new(Tools {
            copy: button.clone(),
            language: language.clone(),
            visible: std::cell::Cell::new(false),
        });
        unsafe { tools.set_data("impro-live-tools", state.clone()) };
        let (weak, strong) = (self.weak.clone(), Rc::downgrade(&state));
        button.connect_focus_in_event(move |_, _| {
            if let (Some(s), Some(t)) = (weak.upgrade(), strong.upgrade()) {
                s.reveal_copy(&t, true);
            }
            glib::Propagation::Proceed
        });
        let (weak, strong, host) = (self.weak.clone(), Rc::downgrade(&state), tools.downgrade());
        button.connect_focus_out_event(move |_, _| {
            if let (Some(s), Some(t), Some(host)) =
                (weak.upgrade(), strong.upgrade(), host.upgrade())
            {
                let hovered = s.hover_tools.borrow().as_ref() == Some(host.upcast_ref());
                s.reveal_copy(&t, hovered);
            }
            glib::Propagation::Proceed
        });
        let (weak, strong) = (self.weak.clone(), Rc::downgrade(&state));
        button.connect_enter_notify_event(move |_, _| {
            if let (Some(s), Some(t)) = (weak.upgrade(), strong.upgrade()) {
                s.reveal_copy(&t, true);
            }
            glib::Propagation::Proceed
        });
        tools.show_all();
        slots.push(self.attach(tools.upcast_ref(), 0, Role::Tools, 0));
    }

    /// Show or hide the Copy button (fades unless motion is reduced).
    pub(super) fn reveal_copy(&self, tools: &Tools, visible: bool) {
        let visible = visible || tools.copy.has_focus();
        if visible != tools.visible.get() {
            tools.visible.set(visible);
            impro_ui::motion::fade(&tools.copy, visible, 128, false, None);
        }
    }

    pub(super) fn toggle_task_id(&self, id: u64, relative: i32) {
        let index = self.blocks.borrow().find_id(id);
        if let Some(index) = index {
            self.toggle_task_at(index, relative);
        }
    }

    pub(super) fn toggle_task_at(&self, index: usize, relative: i32) {
        if !self.view.is_editable() {
            return;
        }
        let Some(start) = self.blocks.borrow().spans.get(index).map(|s| s.0) else {
            return;
        };
        let at = start + relative;
        let first = self.iter(at);
        let value = if first.char() == Some(' ') { "x" } else { " " };
        let mut last = first;
        last.forward_char();
        self.buffer.begin_user_action();
        let (mut a, mut b) = (first, last);
        self.buffer.delete(&mut a, &mut b);
        let mut at = self.iter(at);
        self.buffer.insert(&mut at, value);
        self.buffer.end_user_action();
    }
}

/// Accessible name of a widget (assistive technology label).
fn name_widget(widget: &gtk::Widget, name: &str) {
    if let Some(accessible) = widget.accessible() {
        gtk::prelude::AtkObjectExt::set_name(&accessible, name);
    }
}
