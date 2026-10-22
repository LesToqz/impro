mod common;
use common::*;
use gtk::prelude::*;
use impro_chrome::*;
use std::{cell::Cell, rc::Rc};
#[test]
fn test_programmatic_sync_does_not_emit_request_and_icon_fallbacks() {
    run(|| {
        let bar = TitleBar::new(&gtk::MenuBar::new());
        let count = Rc::new(Cell::new(0));
        for signal in [
            "dock-toggle-requested",
            "bottom-dock-toggle-requested",
            "explorer-toggle-requested",
        ] {
            let c = count.clone();
            bar.connect_request(signal, move |_| c.set(c.get() + 1));
        }
        for active in [true, false] {
            bar.set_dock_active(active);
            bar.set_bottom_dock_active(active);
            bar.set_explorer_active(active);
            assert_eq!(bar.dock_button().is_active(), active);
            assert_eq!(bar.bottom_dock_button().is_active(), active);
            assert_eq!(bar.explorer_button().is_active(), active);
        }
        assert_eq!(count.get(), 0);
        bar.dock_button().clicked();
        bar.bottom_dock_button().clicked();
        bar.explorer_button().clicked();
        assert_eq!(count.get(), 3);
    });
}
#[test]
fn test_button_accessibility_geometry_fade_and_keyboard_activation() {
    run(|| {
        theme("impro-dark");
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_default_size(900, 600);
        let bar = TitleBar::new(&gtk::MenuBar::new());
        window.set_titlebar(Some(&bar));
        window.show_all();
        pump(40);
        let b = bar.dock_button();
        assert!(b.can_focus());
        assert_eq!(
            b.accessible().unwrap().name().as_deref(),
            Some("Toggle right dock")
        );
        assert_eq!(
            b.tooltip_text().as_deref(),
            Some("Toggle right terminal / graph dock (Ctrl+Alt+`)")
        );
        assert_eq!((b.allocated_width(), b.allocated_height()), (28, 28));
        assert!((b.child().unwrap().opacity() - 0.85).abs() < 1. / 255.);
        b.grab_focus();
        assert!(b.activate());
        poll(|| b.is_active());
        unsafe {
            window.destroy();
        }
    });
}
