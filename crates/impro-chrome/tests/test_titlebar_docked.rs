//! The docked variant: the same bar packed above the window content, with no window buttons and
//! no centred title, for windows that keep their native caption.
mod common;
use common::*;
use gtk::prelude::*;
use impro_chrome::*;
use std::{cell::Cell, rc::Rc};

struct Docked {
    window: gtk::Window,
    bar: TitleBar,
    menu: gtk::MenuBar,
    content: gtk::TextView,
}
impl Docked {
    fn new() -> Self {
        theme("impro-dark");
        impro_ui::motion::set_reduce_motion(true);
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_default_size(900, 600);
        let menu = gtk::MenuBar::new();
        for name in ["File", "View"] {
            let item = gtk::MenuItem::with_label(name);
            let sub = gtk::Menu::new();
            sub.add(&gtk::MenuItem::with_label("Action"));
            item.set_submenu(Some(&sub));
            menu.append(&item);
        }
        let bar = TitleBar::new_docked(&menu);
        let content = gtk::TextView::new();
        let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        frame.pack_start(&bar, false, false, 0);
        frame.pack_start(&content, true, true, 0);
        window.add(&frame);
        window.show_all();
        pump(40);
        Self {
            window,
            bar,
            menu,
            content,
        }
    }
    fn x(&self, widget: &impl IsA<gtk::Widget>) -> i32 {
        widget
            .as_ref()
            .translate_coordinates(&self.bar, 0, 0)
            .unwrap()
            .0
    }
}
impl Drop for Docked {
    fn drop(&mut self) {
        unsafe {
            self.window.destroy();
        }
        pump(5);
    }
}
fn window_buttons(bar: &TitleBar) -> usize {
    fn count(widget: &gtk::Widget) -> usize {
        let own = widget
            .downcast_ref::<gtk::Button>()
            .is_some_and(|b| b.style_context().has_class("titlebutton"));
        own as usize
            + widget
                .downcast_ref::<gtk::Container>()
                .map_or(0, |c| c.children().iter().map(count).sum())
    }
    count(bar.upcast_ref())
}

#[test]
fn test_docked_bar_has_no_window_buttons_and_no_centred_title() {
    run(|| {
        let d = Docked::new();
        assert!(d.bar.is_docked());
        assert!(!d.bar.shows_close_button());
        assert_eq!(window_buttons(&d.bar), 0);
        assert!(d.bar.custom_title().is_none());
        assert!(d.bar.title_label().parent().is_none());
        assert!(d.window.titlebar().is_none());
        assert!(d.bar.style_context().has_class("impro-titlebar"));
    });
}

#[test]
fn test_default_bar_is_not_docked_and_keeps_its_window_buttons() {
    run(|| {
        theme("impro-dark");
        let bar = TitleBar::new(&gtk::MenuBar::new());
        assert!(!bar.is_docked());
        assert!(bar.shows_close_button());
        assert_eq!(bar.title_label().text(), "impro");
        assert!(bar.custom_title().is_some());
        assert!(!bar.style_context().has_class("impro-titlebar-docked"));
    });
}

#[test]
fn test_docked_bar_keeps_the_header_styling_and_height() {
    run(|| {
        let d = Docked::new();
        assert_eq!(d.bar.allocated_height(), HEADER_HEIGHT);
        assert_eq!(d.bar.allocated_width(), d.window.allocated_width());
        assert_eq!(
            d.bar.style_context().border(gtk::StateFlags::NORMAL).bottom,
            1
        );
        let expected =
            gdk::RGBA::parse(&impro_core::themes::get_theme(None).ui["sidebar_bg"]).unwrap();
        assert_eq!(
            d.bar
                .style_context()
                .style_property_for_state("background-color", gtk::StateFlags::NORMAL)
                .get::<gdk::RGBA>()
                .unwrap(),
            expected
        );
        // The content starts directly under the bar.
        let (_, y) = d.content.translate_coordinates(&d.bar, 0, 0).unwrap();
        assert_eq!(y, HEADER_HEIGHT);
    });
}

#[test]
fn test_docked_controls_keep_their_order_and_the_cog_geometry() {
    run(|| {
        let d = Docked::new();
        assert_eq!(d.menu.parent(), Some(d.bar.clone().upcast()));
        assert_eq!(d.menu.children().len(), 2);
        let right = |b: &gtk::Widget| d.x(b) + b.allocated_width();
        assert!(d.x(&d.bar.explorer_button()) > d.x(&d.menu));
        assert!(right(d.menu.upcast_ref()) <= d.x(&d.bar.explorer_button()));
        assert!(d.x(&d.bar.bottom_dock_button()) < d.x(&d.bar.dock_button()));
        assert!(d.x(&d.bar.dock_button()) < d.x(&d.bar.settings_button()));
        for b in [
            d.bar.settings_button().upcast::<gtk::Widget>(),
            d.bar.dock_button().upcast(),
            d.bar.bottom_dock_button().upcast(),
            d.bar.explorer_button().upcast(),
        ] {
            // 28px squares; the bottom-dock glyph may round its height up by one pixel.
            assert_eq!(b.allocated_width(), 28);
            assert!((28..=29).contains(&b.allocated_height()));
        }
        // The cog is the right-most control and stays inside the bar.
        let cog = d.bar.settings_button();
        assert_eq!(
            d.x(&cog) + cog.allocated_width() + cog.margin_end(),
            d.bar.allocated_width()
        );
    });
}

#[test]
fn test_docked_controls_fit_at_minimum_window_width() {
    run(|| {
        let d = Docked::new();
        d.window.resize(500, 400);
        pump(40);
        let width = d.bar.allocated_width();
        assert!(width <= 500);
        for b in [
            d.bar.settings_button().upcast::<gtk::Widget>(),
            d.bar.dock_button().upcast(),
            d.bar.bottom_dock_button().upcast(),
            d.bar.explorer_button().upcast(),
        ] {
            assert!(d.x(&b) >= 0 && d.x(&b) + b.allocated_width() <= width);
        }
        assert!(d.x(&d.bar.bottom_dock_button()) >= d.x(&d.menu) + d.menu.allocated_width());
    });
}

#[test]
fn test_docked_toggles_and_cog_emit_requests_and_sync_silently() {
    run(|| {
        let d = Docked::new();
        let count = Rc::new(Cell::new(0));
        for signal in [
            "settings-requested",
            "dock-toggle-requested",
            "bottom-dock-toggle-requested",
            "explorer-toggle-requested",
        ] {
            let c = count.clone();
            d.bar.connect_request(signal, move |_| c.set(c.get() + 1));
        }
        d.bar.set_dock_active(true);
        d.bar.set_bottom_dock_active(true);
        d.bar.set_explorer_active(true);
        assert_eq!(count.get(), 0);
        d.bar.settings_button().clicked();
        d.bar.dock_button().clicked();
        d.bar.bottom_dock_button().clicked();
        d.bar.explorer_button().clicked();
        assert_eq!(count.get(), 4);
        // Keyboard activation works on a bar that is not the window's titlebar.
        let toggle = d.bar.dock_button();
        let before = toggle.is_active();
        toggle.grab_focus();
        assert!(toggle.activate());
        poll(|| toggle.is_active() != before);
    });
}

#[test]
fn test_docked_menu_bar_opens_its_menu_and_follows_the_theme() {
    run(|| {
        let d = Docked::new();
        let file = d.menu.children()[0].clone();
        assert!(file.is_sensitive() && file.is_visible());
        let menu_item = file.downcast::<gtk::MenuItem>().unwrap();
        assert!(menu_item.submenu().is_some());
        let button = d.bar.settings_button();
        let before = button.style_context().color(gtk::StateFlags::NORMAL);
        theme("impro-light");
        pump(20);
        assert_ne!(
            before,
            button.style_context().color(gtk::StateFlags::NORMAL)
        );
        let expected =
            gdk::RGBA::parse(&impro_core::themes::get_theme(Some("impro-light")).ui["sidebar_bg"])
                .unwrap();
        assert_eq!(
            d.bar
                .style_context()
                .style_property_for_state("background-color", gtk::StateFlags::NORMAL)
                .get::<gdk::RGBA>()
                .unwrap(),
            expected
        );
    });
}
