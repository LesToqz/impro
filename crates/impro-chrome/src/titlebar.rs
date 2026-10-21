//! Compact native header with request signals and silent toggle synchronization.
use glib::subclass::prelude::*;
use gtk::{prelude::*, subclass::prelude::*};
use std::{cell::OnceCell, sync::OnceLock};
pub const HEADER_HEIGHT: i32 = 40;
pub const WINDOW_BUTTON_WIDTH: i32 = 40;
pub const WINDOW_ICON_SIZE: i32 = 32;
pub const GLYPH_OPACITY: f64 = 0.85;
mod imp {
    use super::*;
    #[derive(Default)]
    pub struct TitleBar {
        pub controls: OnceCell<Controls>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for TitleBar {
        const NAME: &'static str = "ImproTitleBar";
        type Type = super::TitleBar;
        type ParentType = gtk::HeaderBar;
    }
    impl ObjectImpl for TitleBar {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                [
                    "settings-requested",
                    "dock-toggle-requested",
                    "bottom-dock-toggle-requested",
                    "explorer-toggle-requested",
                ]
                .iter()
                .map(|name| glib::subclass::Signal::builder(name).build())
                .collect()
            })
        }
    }
    impl WidgetImpl for TitleBar {
        fn size_allocate(&self, allocation: &gtk::Allocation) {
            fn size(w: &gtk::Widget) {
                if let Some(button) = w.downcast_ref::<gtk::Button>() {
                    if button.style_context().has_class("titlebutton") {
                        if let Some(image) =
                            button.child().and_then(|w| w.downcast::<gtk::Image>().ok())
                        {
                            if image.pixel_size() != WINDOW_ICON_SIZE {
                                image.set_pixel_size(WINDOW_ICON_SIZE);
                            }
                        }
                    }
                } else if let Some(c) = w.downcast_ref::<gtk::Container>() {
                    c.forall(size);
                }
            }
            self.obj().forall(size);
            self.parent_size_allocate(allocation);
        }
    }
    impl ContainerImpl for TitleBar {}
    impl HeaderBarImpl for TitleBar {}
}
pub struct Controls {
    settings: gtk::Button,
    dock: gtk::ToggleButton,
    bottom: gtk::ToggleButton,
    explorer: gtk::ToggleButton,
    handlers: [glib::SignalHandlerId; 3],
    title: gtk::Label,
}
glib::wrapper! {pub struct TitleBar(ObjectSubclass<imp::TitleBar>) @extends gtk::HeaderBar,gtk::Container,gtk::Widget, @implements gtk::Buildable;}
fn glyph(button: &impl IsA<gtk::Button>, names: &[&str], fallback: &str) {
    let button = button.as_ref();
    button.set_relief(gtk::ReliefStyle::None);
    button.set_valign(gtk::Align::Center);
    button.style_context().add_class("flat");
    button.style_context().add_class("impro-settings-button");
    if let Some(name) =
        gtk::IconTheme::default().and_then(|t| names.iter().find(|n| t.has_icon(n)).copied())
    {
        button.add(&gtk::Image::from_icon_name(Some(name), gtk::IconSize::Menu));
    } else {
        button.add(&gtk::Label::new(Some(fallback)));
    }
    if let Some(child) = button.child() {
        child.set_opacity(GLYPH_OPACITY);
        let weak = child.downgrade();
        impro_ui::motion::StateFader::new(
            button,
            move |p| {
                if let Some(c) = weak.upgrade() {
                    c.set_opacity(GLYPH_OPACITY + (1. - GLYPH_OPACITY) * p);
                }
            },
            impro_ui::motion::DURATIONS["clickable_icon_opacity"] as f64,
            0.,
        );
    }
}
impl TitleBar {
    /// Menus are supplied by the window so actions and accelerators remain centralized.
    pub fn new(menu_bar: &gtk::MenuBar) -> Self {
        let this: Self = glib::Object::new();
        this.style_context().add_class("impro-titlebar");
        this.set_spacing(0);
        this.set_show_close_button(true);
        this.set_decoration_layout(Some(":minimize,maximize,close"));
        menu_bar.set_valign(gtk::Align::Center);
        this.pack_start(menu_bar);
        let make = |names: &[&str], fallback, tooltip, accessible, signal: &'static str| {
            let b = gtk::ToggleButton::new();
            glyph(&b, names, fallback);
            b.set_tooltip_text(Some(tooltip));
            if let Some(a) = b.accessible() {
                a.set_name(accessible);
            }
            let weak = this.downgrade();
            let h = b.connect_clicked(move |_| {
                if let Some(t) = weak.upgrade() {
                    t.emit_by_name::<()>(signal, &[]);
                }
            });
            (b, h)
        };
        let (explorer, eh) = make(
            &["sidebar-show-symbolic", "panel-left-symbolic"],
            "▯",
            "Toggle file explorer (Ctrl+B)",
            "Toggle file explorer",
            "explorer-toggle-requested",
        );
        explorer.set_margin_start(2);
        this.pack_start(&explorer);
        let title = gtk::Label::new(Some("impro"));
        title.style_context().add_class("impro-title");
        this.set_custom_title(Some(&title));
        let settings = gtk::Button::new();
        glyph(
            &settings,
            &[
                "preferences-system-symbolic",
                "emblem-system-symbolic",
                "preferences-other-symbolic",
            ],
            "⚙",
        );
        settings.set_margin_end(6);
        settings.set_tooltip_text(Some("Settings (Ctrl+,)"));
        if let Some(a) = settings.accessible() {
            a.set_name("Settings");
        }
        let weak = this.downgrade();
        settings.connect_clicked(move |_| {
            if let Some(t) = weak.upgrade() {
                t.emit_by_name::<()>("settings-requested", &[]);
            }
        });
        this.pack_end(&settings);
        let (dock, dh) = make(
            &["sidebar-show-right-symbolic", "panel-right-symbolic"],
            "▯",
            "Toggle right terminal / graph dock (Ctrl+Alt+`)",
            "Toggle right dock",
            "dock-toggle-requested",
        );
        dock.set_margin_end(6);
        this.pack_end(&dock);
        let (bottom, bh) = make(
            &["panel-bottom-symbolic", "sidebar-show-bottom-symbolic"],
            "▭",
            "Toggle terminal below (Ctrl+`)",
            "Toggle terminal below",
            "bottom-dock-toggle-requested",
        );
        bottom.set_margin_end(2);
        this.pack_end(&bottom);
        let _ = this.imp().controls.set(Controls {
            settings,
            dock,
            bottom,
            explorer,
            handlers: [dh, bh, eh],
            title,
        });
        this
    }
    fn controls(&self) -> &Controls {
        self.imp().controls.get().expect("constructed title bar")
    }
    pub fn settings_button(&self) -> gtk::Button {
        self.controls().settings.clone()
    }
    pub fn dock_button(&self) -> gtk::ToggleButton {
        self.controls().dock.clone()
    }
    pub fn bottom_dock_button(&self) -> gtk::ToggleButton {
        self.controls().bottom.clone()
    }
    pub fn explorer_button(&self) -> gtk::ToggleButton {
        self.controls().explorer.clone()
    }
    pub fn title_label(&self) -> gtk::Label {
        self.controls().title.clone()
    }
    fn sync(&self, b: &gtk::ToggleButton, index: usize, active: bool) {
        let h = &self.controls().handlers[index];
        b.block_signal(h);
        b.set_active(active);
        b.unblock_signal(h);
    }
    pub fn set_dock_active(&self, active: bool) {
        self.sync(&self.controls().dock, 0, active);
    }
    pub fn set_bottom_dock_active(&self, active: bool) {
        self.sync(&self.controls().bottom, 1, active);
    }
    pub fn set_explorer_active(&self, active: bool) {
        self.sync(&self.controls().explorer, 2, active);
    }
    /// Connect one of the four native request signals.
    pub fn connect_request(
        &self,
        signal: &str,
        callback: impl Fn(&Self) + 'static,
    ) -> glib::SignalHandlerId {
        self.connect_local(signal, false, move |v| {
            if let Ok(this) = v[0].get::<Self>() {
                callback(&this);
            }
            None
        })
    }
}
pub fn css(ui: &impro_core::themes::UiColors) -> String {
    let mut ui = ui.clone();
    ui.insert("hover".into(), format!("alpha({}, 0.067)", ui["overlay"]));
    ui.insert("pressed".into(), format!("alpha({}, 0.12)", ui["overlay"]));
    impro_ui::render_css(include_str!("titlebar.css"), &ui)
}
