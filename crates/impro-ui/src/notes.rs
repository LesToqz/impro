//! GTK adapters for pure note edits and the compact insertion toolbar.
use gtk::{prelude::*, subclass::prelude::*};
use impro_core::{
    notes::{self, Edit},
    themes::UiColors,
};
use std::{cell::RefCell, collections::BTreeMap, sync::LazyLock};
pub fn css(ui: &UiColors) -> String {
    let mut ui = ui.clone();
    ui.insert("hover".into(), format!("alpha({}, 0.09)", ui["overlay"]));
    crate::style::render_css(include_str!("notes.css"), &ui)
}
pub fn build_css(ui: &UiColors) -> String {
    css(ui)
}
pub fn apply_edit(buffer: &gtk::TextBuffer, edit: &Edit) {
    buffer.begin_user_action();
    let mut first = buffer.iter_at_offset(edit.start as i32);
    let mut last = buffer.iter_at_offset(edit.end as i32);
    buffer.delete(&mut first, &mut last);
    let mut at = buffer.iter_at_offset(edit.start as i32);
    buffer.insert(&mut at, &edit.text);
    buffer.select_range(
        &buffer.iter_at_offset(edit.select_end as i32),
        &buffer.iter_at_offset(edit.select_start as i32),
    );
    buffer.end_user_action();
}
pub fn apply_to_buffer(id: &str, buffer: &gtk::TextBuffer) -> Result<Edit, notes::NoteNameError> {
    let (start, end) = buffer
        .selection_bounds()
        .map(|(a, b)| (a.offset(), b.offset()))
        .unwrap_or_else(|| {
            let pos = buffer.cursor_position();
            (pos, pos)
        });
    let doc = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .unwrap_or_default();
    let edit = notes::apply_command(id, &doc, start as isize, Some(end as isize))?;
    apply_edit(buffer, &edit);
    Ok(edit)
}
mod imp {
    use super::*;
    #[derive(Default)]
    pub struct NoteToolbar {
        pub buttons: RefCell<BTreeMap<String, gtk::Button>>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for NoteToolbar {
        const NAME: &'static str = "ImproNoteToolbar";
        type Type = super::NoteToolbar;
        type ParentType = gtk::Box;
    }
    impl ObjectImpl for NoteToolbar {
        fn signals() -> &'static [glib::subclass::Signal] {
            static S: LazyLock<Vec<glib::subclass::Signal>> = LazyLock::new(|| {
                vec![glib::subclass::Signal::builder("command")
                    .param_types([String::static_type()])
                    .build()]
            });
            &S
        }
    }
    impl WidgetImpl for NoteToolbar {}
    impl ContainerImpl for NoteToolbar {}
    impl BoxImpl for NoteToolbar {}
}
glib::wrapper! {pub struct NoteToolbar(ObjectSubclass<imp::NoteToolbar>) @extends gtk::Box,gtk::Container,gtk::Widget,@implements gtk::Orientable,gtk::Buildable;}
impl Default for NoteToolbar {
    fn default() -> Self {
        Self::new()
    }
}
impl NoteToolbar {
    pub fn new() -> Self {
        let this: Self = glib::Object::builder()
            .property("spacing", 2)
            .property("valign", gtk::Align::Start)
            .build();
        this.style_context().add_class("impro-note-toolbar");
        let icon_theme = gtk::IconTheme::default();
        for c in notes::NOTE_COMMANDS {
            if ["bullet", "table", "link"].contains(&c.id) {
                let separator = gtk::Separator::new(gtk::Orientation::Vertical);
                separator.set_valign(gtk::Align::Center);
                separator.set_size_request(1, 16);
                this.pack_start(&separator, false, false, 4);
            }
            let button = gtk::Button::new();
            button.set_size_request(28, 28);
            button.set_relief(gtk::ReliefStyle::None);
            button.set_can_focus(false);
            if let Some(icon) = c
                .icons
                .iter()
                .find(|name| icon_theme.as_ref().is_some_and(|t| t.has_icon(name)))
            {
                button.add(&gtk::Image::from_icon_name(Some(icon), gtk::IconSize::Menu));
            } else {
                let label = gtk::Label::new(Some(c.glyph));
                label.style_context().add_class("impro-note-glyph");
                button.add(&label);
            }
            let child = button.child().expect("toolbar child").downgrade();
            crate::motion::StateFader::new(
                &button,
                move |p| {
                    if let Some(child) = child.upgrade() {
                        child.set_opacity(0.85 + 0.15 * p);
                    }
                },
                128.0,
                0.0,
            );
            button.set_tooltip_text(Some(c.label));
            if let Some(a) = button.accessible() {
                a.set_name(c.label);
            }
            let weak = this.downgrade();
            button.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.emit_by_name::<()>("command", &[&c.id]);
                }
            });
            this.imp()
                .buttons
                .borrow_mut()
                .insert(c.id.into(), button.clone());
            this.pack_start(&button, false, false, 0);
        }
        this
    }
    pub fn buttons(&self) -> BTreeMap<String, gtk::Button> {
        self.imp().buttons.borrow().clone()
    }
    pub fn connect_command(&self, f: impl Fn(&Self, &str) + 'static) -> glib::SignalHandlerId {
        self.connect_local("command", false, move |values| {
            if let (Ok(this), Ok(id)) = (values[0].get::<Self>(), values[1].get::<String>()) {
                f(&this, &id);
            }
            None
        })
    }
}
