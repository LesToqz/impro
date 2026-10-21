//! Reusable compact settings controls; composite handles expose widget() for GTK containers.
use crate::{motion, style};
use gtk::prelude::*;
use impro_core::{settings, themes::UiColors};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    ops::Deref,
    rc::Rc,
};
pub const TOGGLE_SIZE: (i32, i32) = (40, 22);
pub const THUMB_SIZE: i32 = 18;
pub const THUMB_INSET: i32 = 2;
pub const THUMB_TRAVEL: i32 = 18;
pub const TOGGLE_DURATION: i32 = 150;
pub const SWATCH_SIZE: i32 = 22;
fn class(widget: &impl IsA<gtk::Widget>, name: &str) {
    widget.style_context().add_class(name);
}
fn accessible(widget: &impl IsA<gtk::Widget>, name: &str) {
    if let Some(a) = widget.accessible() {
        a.set_name(name);
    }
}
pub fn css(ui: &UiColors) -> String {
    let mut ui = ui.clone();
    ui.insert("hover".into(), format!("alpha({}, 0.067)", ui["overlay"]));
    ui.insert(
        "swatches".into(),
        settings::accent_presets()
            .iter()
            .map(|c| {
                format!(
                    "button.impro-swatch.impro-swatch-{} {{ background: {c}; }}\n",
                    c.trim_start_matches('#')
                )
            })
            .collect(),
    );
    style::render_css(include_str!("widgets.css"), &ui)
}
pub fn build_css(ui: &UiColors) -> String {
    css(ui)
}
macro_rules! handle {
    ($name:ident,$gtk:ty) => {
        impl Deref for $name {
            type Target = $gtk;
            fn deref(&self) -> &Self::Target {
                &self.widget
            }
        }
        impl $name {
            pub fn widget(&self) -> &$gtk {
                &self.widget
            }
        }
    };
}
#[derive(Clone)]
pub struct Toggle {
    widget: gtk::ToggleButton,
    pub thumb: gtk::Box,
}
handle!(Toggle, gtk::ToggleButton);
impl Default for Toggle {
    fn default() -> Self {
        Self::new(false, None)
    }
}
impl Toggle {
    pub fn new(active: bool, label: Option<&str>) -> Self {
        let widget = gtk::ToggleButton::new();
        class(&widget, "impro-toggle");
        widget.set_size_request(40, 22);
        widget.set_halign(gtk::Align::End);
        widget.set_valign(gtk::Align::Center);
        widget.set_focus_on_click(false);
        let thumb = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        class(&thumb, "impro-toggle-thumb");
        thumb.set_size_request(18, 18);
        thumb.set_halign(gtk::Align::Start);
        thumb.set_valign(gtk::Align::Center);
        widget.add(&thumb);
        if let Some(a) = widget.accessible() {
            a.set_role(atk::Role::for_name("switch"));
            if let Some(label) = label {
                a.set_name(label);
            }
        }
        widget.set_active(active);
        thumb.set_margin_start(if active { 20 } else { 2 });
        let weak = thumb.downgrade();
        let key = motion::MotionKey::unique();
        widget.connect_toggled(move |button| {
            if let Some(thumb) = weak.upgrade() {
                let start = thumb.margin_start();
                let target = if button.is_active() { 20 } else { 2 };
                let duration = if button.is_mapped() {
                    TOGGLE_DURATION
                } else {
                    0
                };
                let weak = thumb.downgrade();
                motion::animate(
                    &thumb,
                    duration,
                    move |p| {
                        if let Some(t) = weak.upgrade() {
                            t.set_margin_start(
                                (start as f64 + (target - start) as f64 * p).round_ties_even()
                                    as i32,
                            );
                        }
                    },
                    motion::AnimateOptions {
                        key,
                        easing: motion::easing("ease_in_out").expect("built-in curve"),
                        ..Default::default()
                    },
                );
            }
        });
        Self { widget, thumb }
    }
    pub fn thumb_offset(&self) -> i32 {
        self.thumb.margin_start()
    }
}
#[derive(Clone)]
pub struct IconButton {
    widget: gtk::Button,
}
handle!(IconButton, gtk::Button);
impl IconButton {
    pub fn new(icon_name: &str, tooltip: &str) -> Self {
        let widget = gtk::Button::new();
        class(&widget, "impro-icon-button");
        widget.set_relief(gtk::ReliefStyle::None);
        widget.set_focus_on_click(false);
        widget.set_valign(gtk::Align::Center);
        let image = gtk::Image::from_icon_name(Some(icon_name), gtk::IconSize::Menu);
        image.set_pixel_size(16);
        widget.add(&image);
        widget.set_tooltip_text(Some(tooltip));
        accessible(&widget, tooltip);
        Self { widget }
    }
}
#[derive(Clone)]
pub struct HotkeyChip {
    widget: gtk::Label,
}
handle!(HotkeyChip, gtk::Label);
impl HotkeyChip {
    pub fn new(text: &str) -> Self {
        let widget = gtk::Label::new(Some(text));
        class(&widget, "impro-hotkey-chip");
        widget.set_valign(gtk::Align::Center);
        Self { widget }
    }
}
#[derive(Clone)]
pub struct SearchField {
    widget: gtk::SearchEntry,
}
handle!(SearchField, gtk::SearchEntry);
impl Default for SearchField {
    fn default() -> Self {
        Self::new("Search settings…")
    }
}
impl SearchField {
    pub fn new(placeholder: &str) -> Self {
        let widget = gtk::SearchEntry::new();
        class(&widget, "impro-search-field");
        widget.set_placeholder_text(Some(placeholder));
        accessible(&widget, placeholder.trim_end_matches('…'));
        Self { widget }
    }
}
fn label(text: &str, css: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    class(&label, css);
    label
}
#[derive(Default)]
pub struct RowOptions<'a> {
    pub key: Option<&'a str>,
    pub aliases: &'a [&'a str],
    pub reset: Option<Box<dyn Fn()>>,
    pub extra: Vec<gtk::Widget>,
}
#[derive(Clone)]
pub struct SettingRow {
    widget: gtk::ListBoxRow,
    pub key: Option<String>,
    pub name: String,
    pub description: String,
    pub aliases: Vec<String>,
    pub control: Option<gtk::Widget>,
    pub name_label: gtk::Label,
    pub desc_label: Option<gtk::Label>,
    pub controls: gtk::Box,
    pub reset_button: Option<IconButton>,
    layout: gtk::Box,
    haystack: String,
}
handle!(SettingRow, gtk::ListBoxRow);
impl SettingRow {
    pub fn new(
        name: &str,
        description: &str,
        control: Option<&gtk::Widget>,
        options: RowOptions<'_>,
    ) -> Self {
        let widget = gtk::ListBoxRow::new();
        class(&widget, "impro-setting-row");
        let layout = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        let info = gtk::Box::new(gtk::Orientation::Vertical, 0);
        info.set_valign(gtk::Align::Center);
        let name_label = label(name, "impro-setting-name");
        name_label.set_line_wrap(true);
        name_label.set_line_wrap_mode(pango::WrapMode::WordChar);
        info.pack_start(&name_label, false, false, 0);
        let desc_label = if description.is_empty() {
            None
        } else {
            let label = label(description, "impro-setting-desc");
            label.set_line_wrap(true);
            label.set_line_wrap_mode(pango::WrapMode::WordChar);
            label.set_margin_top(4);
            info.pack_start(&label, false, false, 0);
            Some(label)
        };
        layout.pack_start(&info, true, true, 0);
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        controls.set_valign(gtk::Align::Center);
        controls.set_halign(gtk::Align::End);
        let reset_button = options.reset.map(|reset| {
            let button = IconButton::new("edit-undo-symbolic", "Restore default");
            button.connect_clicked(move |_| reset());
            controls.pack_start(button.widget(), false, false, 0);
            button
        });
        for extra in options.extra {
            controls.pack_start(&extra, false, false, 0);
        }
        if let Some(control) = control {
            control.set_valign(gtk::Align::Center);
            controls.pack_start(control, false, false, 0);
            if let Some(a) = control.accessible() {
                if a.name().is_none_or(|s| s.is_empty()) {
                    a.set_name(name);
                }
            }
        }
        layout.pack_end(&controls, false, false, 0);
        widget.add(&layout);
        let aliases = options
            .aliases
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let haystack =
            impro_core::text::casefold(&format!("{name} {description} {}", aliases.join(" ")));
        Self {
            widget,
            key: options.key.map(str::to_owned),
            name: name.into(),
            description: description.into(),
            aliases,
            control: control.cloned(),
            name_label,
            desc_label,
            controls,
            reset_button,
            layout,
            haystack,
        }
    }
    pub fn matches(&self, query: &str) -> bool {
        impro_core::text::casefold(query)
            .split_whitespace()
            .all(|w| self.haystack.contains(w))
    }
    pub fn narrow(&self) -> bool {
        self.layout.orientation() == gtk::Orientation::Vertical
    }
    pub fn set_narrow(&self, narrow: bool) {
        let narrow = narrow
            && self
                .control
                .as_ref()
                .is_some_and(|c| !c.style_context().has_class("impro-toggle"));
        if narrow == self.narrow() {
            return;
        }
        self.layout.set_orientation(if narrow {
            gtk::Orientation::Vertical
        } else {
            gtk::Orientation::Horizontal
        });
        self.layout.set_spacing(if narrow { 12 } else { 16 });
        self.controls.set_halign(if narrow {
            gtk::Align::Start
        } else {
            gtk::Align::End
        });
    }
    pub fn activate_control(&self) -> bool {
        let Some(c) = self.control.as_ref().filter(|c| c.is_sensitive()) else {
            return false;
        };
        if !c.is::<gtk::FontButton>() {
            if let Some(button) = c.downcast_ref::<gtk::Button>() {
                button.emit_clicked();
                return true;
            }
        }
        if let Some(combo) = c.downcast_ref::<gtk::ComboBox>() {
            combo.grab_focus();
            combo.popup();
            return true;
        }
        if c.child_focus(gtk::DirectionType::TabForward) {
            true
        } else {
            c.grab_focus();
            c.has_focus()
        }
    }
}
#[derive(Clone)]
pub struct SettingCard {
    widget: gtk::ListBox,
    rows: Rc<RefCell<Vec<SettingRow>>>,
    query: Rc<RefCell<String>>,
}
handle!(SettingCard, gtk::ListBox);
impl Default for SettingCard {
    fn default() -> Self {
        Self::new()
    }
}
impl SettingCard {
    pub fn new() -> Self {
        let widget = gtk::ListBox::new();
        class(&widget, "impro-card");
        widget.set_selection_mode(gtk::SelectionMode::None);
        widget.set_activate_on_single_click(false);
        let rows = Rc::new(RefCell::new(Vec::<SettingRow>::new()));
        let query = Rc::new(RefCell::new(String::new()));
        let (r, q) = (rows.clone(), query.clone());
        widget.set_filter_func(Some(Box::new(move |row| {
            let haystack = r
                .borrow()
                .iter()
                .find(|r| r.widget() == row)
                .map(|r| r.haystack.clone());
            let query = q.borrow().clone();
            query
                .split_whitespace()
                .all(|word| haystack.as_ref().is_none_or(|h| h.contains(word)))
        })));
        widget.set_header_func(Some(Box::new(|row, before| {
            if before.is_none() {
                row.set_header(gtk::Widget::NONE);
            } else if row.header().is_none() {
                let divider = gtk::Separator::new(gtk::Orientation::Horizontal);
                class(&divider, "impro-card-divider");
                divider.set_margin_start(20);
                divider.set_margin_end(20);
                divider.show();
                row.set_header(Some(&divider));
            }
        })));
        let r = rows.clone();
        widget.connect_row_activated(move |_, row| {
            let item = r.borrow().iter().find(|r| r.widget() == row).cloned();
            if let Some(item) = item {
                item.activate_control();
            }
        });
        Self {
            widget,
            rows,
            query,
        }
    }
    pub fn add_row(&self, row: &SettingRow) {
        self.rows.borrow_mut().push(row.clone());
        self.widget.add(row.widget());
    }
    pub fn rows(&self) -> Vec<SettingRow> {
        self.rows.borrow().clone()
    }
    pub fn visible_rows(&self) -> Vec<SettingRow> {
        let query = self.query.borrow().clone();
        self.rows()
            .into_iter()
            .filter(|r| r.is_visible() && r.matches(&query))
            .collect()
    }
    pub fn set_query(&self, query: &str) {
        *self.query.borrow_mut() = impro_core::text::casefold(query.trim());
        self.widget.invalidate_filter();
        self.widget.invalidate_headers();
    }
}
#[derive(Clone)]
pub struct NavRow {
    widget: gtk::ListBoxRow,
    pub page_id: String,
    pub label: gtk::Label,
    pub count_label: gtk::Label,
}
handle!(NavRow, gtk::ListBoxRow);
impl NavRow {
    pub fn new(page_id: &str, text: &str, icon_name: &str) -> Self {
        let widget = gtk::ListBoxRow::new();
        let layout = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let image = gtk::Image::from_icon_name(Some(icon_name), gtk::IconSize::Menu);
        image.set_pixel_size(16);
        layout.pack_start(&image, false, false, 0);
        let label = label(text, "");
        label.set_ellipsize(pango::EllipsizeMode::End);
        layout.pack_start(&label, true, true, 0);
        let count_label = gtk::Label::new(None);
        count_label.set_xalign(1.0);
        class(&count_label, "impro-nav-count");
        count_label.set_no_show_all(true);
        layout.pack_end(&count_label, false, false, 0);
        widget.add(&layout);
        accessible(&widget, text);
        Self {
            widget,
            page_id: page_id.into(),
            label,
            count_label,
        }
    }
    pub fn set_count(&self, count: Option<usize>) {
        self.count_label
            .set_text(&count.map(|c| c.to_string()).unwrap_or_default());
        self.count_label.set_visible(count.is_some());
    }
}
#[derive(Clone)]
pub struct NavList {
    widget: gtk::ListBox,
    rows: Rc<RefCell<BTreeMap<String, NavRow>>>,
    pending_g: Rc<Cell<bool>>,
}
handle!(NavList, gtk::ListBox);
impl Default for NavList {
    fn default() -> Self {
        Self::new()
    }
}
impl NavList {
    pub fn new() -> Self {
        let widget = gtk::ListBox::new();
        class(&widget, "impro-nav");
        widget.set_selection_mode(gtk::SelectionMode::Browse);
        let this = Self {
            widget,
            rows: Rc::default(),
            pending_g: Rc::default(),
        };
        let rows = this.rows.clone();
        let pending = this.pending_g.clone();
        this.widget.connect_key_press_event(move |widget, event| {
            let this = Self {
                widget: widget.clone(),
                rows: rows.clone(),
                pending_g: pending.clone(),
            };
            if this.handle_key(event.keyval(), event.state()) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        this
    }
    pub fn add_page(&self, id: &str, label: &str, icon_name: &str) -> NavRow {
        let row = NavRow::new(id, label, icon_name);
        self.rows.borrow_mut().insert(id.into(), row.clone());
        self.widget.add(row.widget());
        row
    }
    pub fn rows(&self) -> BTreeMap<String, NavRow> {
        self.rows.borrow().clone()
    }
    pub fn page_id(&self) -> Option<String> {
        let selected = self.widget.selected_row()?;
        self.rows
            .borrow()
            .values()
            .find(|r| r.widget == selected)
            .map(|r| r.page_id.clone())
    }
    pub fn select_page(&self, id: &str) -> bool {
        let row = self.rows.borrow().get(id).cloned();
        if let Some(row) = row {
            self.widget.select_row(Some(row.widget()));
            if self.widget.focus_child().is_some() {
                row.grab_focus();
            }
            true
        } else {
            false
        }
    }
    fn move_by(&self, step: i32, absolute: Option<i32>) {
        let rows: Vec<_> = self
            .widget
            .children()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::ListBoxRow>().ok())
            .filter(|r| r.is_visible() && r.is_child_visible())
            .collect();
        if rows.is_empty() {
            return;
        }
        let selected = self.widget.selected_row();
        let index = absolute
            .map(|i| i.rem_euclid(rows.len() as i32))
            .unwrap_or_else(|| {
                rows.iter()
                    .position(|r| Some(r) == selected.as_ref())
                    .map(|i| (i as i32 + step).clamp(0, rows.len() as i32 - 1))
                    .unwrap_or(0)
            });
        self.widget.select_row(Some(&rows[index as usize]));
        rows[index as usize].grab_focus();
    }
    pub fn handle_key(&self, key: gdk::keys::Key, state: gdk::ModifierType) -> bool {
        if state.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::MOD1_MASK) {
            return false;
        }
        let pending = self.pending_g.replace(false);
        use gdk::keys::constants as k;
        match key {
            k::j => self.move_by(1, None),
            k::k => self.move_by(-1, None),
            k::G => self.move_by(0, Some(-1)),
            k::g => {
                if pending {
                    self.move_by(0, Some(0));
                } else {
                    self.pending_g.set(true);
                }
            }
            _ => return false,
        }
        true
    }
}
mod swatch_imp {
    use super::*;
    use glib::subclass::prelude::*;
    use gtk::subclass::prelude::*;
    use std::sync::LazyLock;
    #[derive(Default)]
    pub struct SwatchPicker {
        pub buttons: RefCell<BTreeMap<String, gtk::ToggleButton>>,
        pub value: RefCell<Option<String>>,
        pub updating: Cell<bool>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for SwatchPicker {
        const NAME: &'static str = "ImproSwatchPicker";
        type Type = super::SwatchPicker;
        type ParentType = gtk::Box;
    }
    impl ObjectImpl for SwatchPicker {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: LazyLock<Vec<glib::subclass::Signal>> = LazyLock::new(|| {
                vec![glib::subclass::Signal::builder("value-changed")
                    .param_types([String::static_type()])
                    .build()]
            });
            &SIGNALS
        }
    }
    impl WidgetImpl for SwatchPicker {}
    impl ContainerImpl for SwatchPicker {}
    impl BoxImpl for SwatchPicker {}
}
glib::wrapper! {pub struct SwatchPicker(ObjectSubclass<swatch_imp::SwatchPicker>) @extends gtk::Box,gtk::Container,gtk::Widget,@implements gtk::Orientable,gtk::Buildable;}
impl Default for SwatchPicker {
    fn default() -> Self {
        Self::new(settings::accent_presets().iter().map(String::as_str))
    }
}
impl SwatchPicker {
    pub fn new<'a>(colors: impl IntoIterator<Item = &'a str>) -> Self {
        use glib::subclass::prelude::*;
        let this: Self = glib::Object::builder()
            .property("spacing", 6)
            .property("valign", gtk::Align::Center)
            .build();
        for c in colors {
            let color = c.to_string();
            let button = gtk::ToggleButton::new();
            class(&button, "impro-swatch");
            class(
                &button,
                &format!(
                    "impro-swatch-{}",
                    c.trim_start_matches('#').to_ascii_lowercase()
                ),
            );
            button.set_size_request(22, 22);
            button.set_valign(gtk::Align::Center);
            button.set_focus_on_click(false);
            button.set_tooltip_text(Some(c));
            accessible(&button, &format!("Accent {c}"));
            let weak = this.downgrade();
            let captured = color.clone();
            button.connect_toggled(move |b| {
                if let Some(this) = weak.upgrade() {
                    if !this.imp().updating.get()
                        && (b.is_active() || this.value().as_deref() == Some(&captured))
                    {
                        this.set_value(Some(&captured), b.is_active());
                    }
                }
            });
            this.imp()
                .buttons
                .borrow_mut()
                .insert(color, button.clone());
            this.pack_start(&button, false, false, 0);
        }
        this
    }
    pub fn buttons(&self) -> BTreeMap<String, gtk::ToggleButton> {
        use glib::subclass::prelude::*;
        self.imp().buttons.borrow().clone()
    }
    pub fn value(&self) -> Option<String> {
        use glib::subclass::prelude::*;
        self.imp().value.borrow().clone()
    }
    pub fn set_value(&self, value: Option<&str>, emit: bool) {
        use glib::subclass::prelude::*;
        let buttons = self.buttons();
        let value = value
            .filter(|v| buttons.contains_key(*v))
            .map(str::to_owned);
        let previous = self.imp().updating.replace(true);
        for (color, button) in buttons {
            button.set_active(Some(&color) == value.as_ref());
        }
        self.imp().updating.set(previous);
        let changed = self.value() != value;
        *self.imp().value.borrow_mut() = value.clone();
        if emit && changed {
            self.emit_by_name::<()>("value-changed", &[&value]);
        }
    }
    pub fn connect_value_changed(
        &self,
        f: impl Fn(&Self, Option<&str>) + 'static,
    ) -> glib::SignalHandlerId {
        self.connect_local("value-changed", false, move |values| {
            if let (Ok(this), Ok(value)) =
                (values[0].get::<Self>(), values[1].get::<Option<String>>())
            {
                f(&this, value.as_deref());
            }
            None
        })
    }
}
mod layout;
pub use layout::{CenteredColumn, WrapBox};
