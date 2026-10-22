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
    fn prepare_controls(header: &super::TitleBar) {
        fn prepare(widget: &gtk::Widget) {
            if let Some(button) = widget.downcast_ref::<gtk::Button>() {
                if button.style_context().has_class("titlebutton") {
                    if let Some(image) =
                        button.child().and_then(|w| w.downcast::<gtk::Image>().ok())
                    {
                        if image.pixel_size() != WINDOW_ICON_SIZE {
                            image.set_pixel_size(WINDOW_ICON_SIZE);
                        }
                        impro_ui::icons::prepare_image(&image);
                    }
                }
            } else if let Some(container) = widget.downcast_ref::<gtk::Container>() {
                container.forall(prepare);
            }
        }
        header.forall(prepare);
    }
    impl WidgetImpl for TitleBar {
        fn preferred_width(&self) -> (i32, i32) {
            prepare_controls(&self.obj());
            self.parent_preferred_width()
        }
        fn preferred_height(&self) -> (i32, i32) {
            prepare_controls(&self.obj());
            self.parent_preferred_height()
        }
        fn size_allocate(&self, allocation: &gtk::Allocation) {
            prepare_controls(&self.obj());
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
        gtk::IconTheme::default().and_then(|t| choose_icon(names, |name| t.has_icon(name)))
    {
        let image = gtk::Image::from_icon_name(Some(name), gtk::IconSize::Menu);
        impro_ui::icons::prepare_image(&image);
        button.add(&image);
    } else {
        button.add(&gtk::Label::new(Some(fallback)));
    }
    if let Some(child) = button.child() {
        child.set_opacity(GLYPH_OPACITY);
        let value = std::rc::Rc::new(std::cell::Cell::new(0.));
        let key = impro_ui::motion::MotionKey::unique();
        let weak = child.downgrade();
        let current = value.clone();
        button.add_events(gdk::EventMask::ENTER_NOTIFY_MASK | gdk::EventMask::LEAVE_NOTIFY_MASK);
        button.connect_enter_notify_event(move |button, event| {
            if event.detail() != gdk::NotifyType::Inferior {
                fade_glyph(button, &weak, &current, key, 1.);
            }
            glib::Propagation::Proceed
        });
        let weak = child.downgrade();
        button.connect_leave_notify_event(move |button, event| {
            if event.detail() != gdk::NotifyType::Inferior {
                fade_glyph(button, &weak, &value, key, 0.);
            }
            glib::Propagation::Proceed
        });
    }
}
fn fade_glyph(
    button: &gtk::Button,
    child: &glib::WeakRef<gtk::Widget>,
    value: &std::rc::Rc<std::cell::Cell<f64>>,
    key: impro_ui::motion::MotionKey,
    target: f64,
) {
    let start = value.get();
    let value = value.clone();
    let child = child.clone();
    impro_ui::motion::animate(
        button,
        "clickable_icon_opacity",
        move |p| {
            let opacity = start + (target - start) * p;
            value.set(opacity);
            if let Some(c) = child.upgrade() {
                c.set_opacity(GLYPH_OPACITY + (1. - GLYPH_OPACITY) * opacity);
            }
        },
        impro_ui::motion::AnimateOptions {
            easing: impro_ui::motion::easing("ease_in_out").expect("built-in curve"),
            key,
            ..Default::default()
        },
    );
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

fn choose_icon<'a>(names: &[&'a str], available: impl Fn(&str) -> bool) -> Option<&'a str> {
    names.iter().find(|name| available(name)).copied()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_settings_icon_fallback_chain_and_glyph() {
        let names = [
            "preferences-system-symbolic",
            "emblem-system-symbolic",
            "preferences-other-symbolic",
        ];
        for wanted in names {
            assert_eq!(choose_icon(&names, |name| name == wanted), Some(wanted));
        }
        assert_eq!(choose_icon(&names, |_| false), None);
    }
    #[test]
    fn test_titlebar_toggle_fallback_glyphs() {
        for names in [
            ["sidebar-show-symbolic", "panel-left-symbolic"],
            ["sidebar-show-right-symbolic", "panel-right-symbolic"],
            ["panel-bottom-symbolic", "sidebar-show-bottom-symbolic"],
        ] {
            assert_eq!(choose_icon(&names, |name| name == names[1]), Some(names[1]));
            assert_eq!(choose_icon(&names, |_| false), None);
        }
    }
}
