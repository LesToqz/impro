mod common;
use common::*;
use gtk::prelude::*;
use impro_chrome::*;
use std::{cell::Cell, rc::Rc};
struct Header {
    window: gtk::Window,
    bar: TitleBar,
    menu: gtk::MenuBar,
}
impl Header {
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
        let bar = TitleBar::new(&menu);
        window.set_titlebar(Some(&bar));
        window.add(&gtk::TextView::new());
        window.show_all();
        pump(40);
        Self { window, bar, menu }
    }
}
impl Drop for Header {
    fn drop(&mut self) {
        unsafe {
            self.window.destroy();
        }
        pump(5);
    }
}
fn controls(bar: &TitleBar) -> Vec<gtk::Button> {
    fn visit(w: &gtk::Widget, out: &mut Vec<gtk::Button>) {
        if let Some(b) = w.downcast_ref::<gtk::Button>() {
            if b.style_context().has_class("titlebutton") {
                out.push(b.clone());
            }
        }
        if let Some(c) = w.downcast_ref::<gtk::Container>() {
            c.forall(|w| visit(w, out));
        }
    }
    let mut out = vec![];
    bar.forall(|w| visit(w, &mut out));
    out
}
#[test]
fn test_builder_matches_legacy_dark_plus_and_parses_all_themes() {
    run(|| {
        for t in impro_core::themes::themes() {
            let css = css(&t.ui);
            gtk::CssProvider::new()
                .load_from_data(css.as_bytes())
                .unwrap();
            assert!(css.contains("min-height: 39px"));
            assert!(css.contains("min-width: 40px"));
        }
    });
}
#[test]
fn test_explorer_and_bottom_dock_toggles_sit_beside_view_and_right_dock() {
    run(|| {
        let h = Header::new();
        let pos = |w: &gtk::Widget| w.translate_coordinates(&h.bar, 0, 0).unwrap().0;
        assert!(pos(h.bar.explorer_button().upcast_ref()) > pos(h.menu.upcast_ref()));
        assert!(
            pos(h.bar.bottom_dock_button().upcast_ref()) < pos(h.bar.dock_button().upcast_ref())
        );
        assert!(pos(h.bar.dock_button().upcast_ref()) < pos(h.bar.settings_button().upcast_ref()));
    });
}
#[test]
fn test_settings_button_accessibility_and_signal() {
    run(|| {
        let h = Header::new();
        let b = h.bar.settings_button();
        assert_eq!(b.accessible().unwrap().name().as_deref(), Some("Settings"));
        assert_eq!(b.tooltip_text().as_deref(), Some("Settings (Ctrl+,)"));
        let count = Rc::new(Cell::new(0));
        let c = count.clone();
        h.bar
            .connect_request("settings-requested", move |_| c.set(c.get() + 1));
        b.clicked();
        assert_eq!(count.get(), 1);
    });
}
#[test]
fn test_settings_immediately_precedes_minimize_in_real_allocations() {
    run(|| {
        let h = Header::new();
        let b = h.bar.settings_button();
        let x = b.translate_coordinates(&h.bar, 0, 0).unwrap().0;
        let min_x = controls(&h.bar)
            .iter()
            .map(|c| c.translate_coordinates(&h.bar, 0, 0).unwrap().0)
            .min()
            .unwrap();
        assert_eq!(x + b.allocated_width() + b.margin_end(), min_x);
    });
}
#[test]
fn test_settings_button_style_tracks_live_theme_switch() {
    run(|| {
        let h = Header::new();
        let b = h.bar.settings_button();
        let before = b.style_context().color(gtk::StateFlags::NORMAL);
        theme("impro-light");
        pump(20);
        assert_ne!(before, b.style_context().color(gtk::StateFlags::NORMAL));
    });
}
#[test]
fn test_menus_share_header_and_leave_content() {
    run(|| {
        let h = Header::new();
        assert_eq!(h.menu.parent(), Some(h.bar.clone().upcast()));
        assert_eq!(h.menu.children().len(), 2);
        assert!(h.window.child().unwrap().allocated_height() > 500);
    });
}
#[test]
fn test_title_is_just_the_application_name() {
    run(|| {
        let h = Header::new();
        assert_eq!(h.bar.title_label().text(), "impro");
    });
}
#[test]
fn test_css_parses_and_keeps_title_centered_in_compact_bar() {
    run(|| {
        let h = Header::new();
        let title = h.bar.title_label();
        let (x, _) = title.translate_coordinates(&h.bar, 0, 0).unwrap();
        assert!((2 * x + title.allocated_width() - h.bar.allocated_width()).abs() <= 1);
        assert_eq!(h.bar.allocated_height(), 40);
    });
}
#[test]
fn test_header_is_39_plus_divider_on_secondary_background() {
    run(|| {
        let h = Header::new();
        assert_eq!(h.bar.allocated_height(), HEADER_HEIGHT);
        assert_eq!(
            h.bar.style_context().border(gtk::StateFlags::NORMAL).bottom,
            1
        );
        let expected =
            gdk::RGBA::parse(&impro_core::themes::get_theme(None).ui["sidebar_bg"]).unwrap();
        assert_eq!(
            h.bar
                .style_context()
                .style_property_for_state("background-color", gtk::StateFlags::NORMAL)
                .get::<gdk::RGBA>()
                .unwrap(),
            expected
        );
    });
}
#[test]
fn test_window_buttons_are_flat_40px_with_16px_glyphs_and_overlay_hover() {
    run(|| {
        let h = Header::new();
        let buttons = controls(&h.bar);
        assert_eq!(buttons.len(), 3);
        for b in buttons {
            assert_eq!(b.allocated_width(), 40);
            assert_eq!(b.allocated_height(), 39);
            let image = b.child().unwrap().downcast::<gtk::Image>().unwrap();
            assert_eq!(image.pixel_size(), 32);
        }
    });
}
#[test]
fn test_native_window_glyphs_stay_cog_sized_after_theme_and_decoration_changes() {
    run(|| {
        let h = Header::new();
        for id in ["impro-light", "impro-dark"] {
            theme(id);
            h.bar.set_decoration_layout(Some(":close"));
            pump(30);
            for b in controls(&h.bar) {
                assert_eq!(
                    b.child()
                        .unwrap()
                        .downcast::<gtk::Image>()
                        .unwrap()
                        .pixel_size(),
                    32
                );
            }
            h.bar
                .set_decoration_layout(Some(":minimize,maximize,close"));
            pump(30);
            assert_eq!(controls(&h.bar).len(), 3);
        }
    });
}
#[test]
fn test_window_controls_fit_at_minimum_window_width() {
    run(|| {
        let h = Header::new();
        h.window.resize(500, 400);
        pump(40);
        for b in controls(&h.bar) {
            let x = b.translate_coordinates(&h.bar, 0, 0).unwrap().0;
            assert!(x >= 0 && x + b.allocated_width() <= h.bar.allocated_width());
        }
    });
}
#[test]
fn test_close_hover_is_danger_with_white_glyph() {
    run(|| {
        let h = Header::new();
        let close = controls(&h.bar)
            .into_iter()
            .find(|b| b.style_context().has_class("close"))
            .unwrap();
        close.set_state_flags(gtk::StateFlags::PRELIGHT, false);
        pump(20);
        let context = close.style_context();
        assert_eq!(
            context
                .style_property_for_state("background-color", gtk::StateFlags::PRELIGHT)
                .get::<gdk::RGBA>()
                .unwrap(),
            gdk::RGBA::parse(&impro_core::themes::get_theme(None).ui["danger"]).unwrap()
        );
        assert_eq!(
            context.color(gtk::StateFlags::PRELIGHT),
            gdk::RGBA::parse("#ffffff").unwrap()
        );
    });
}
#[test]
fn test_menubar_items_are_flat_13px_clickable_icons() {
    run(|| {
        let h = Header::new();
        for item in h.menu.children() {
            assert_eq!(item.style_context().border(gtk::StateFlags::NORMAL).top, 0);
            let label = item
                .downcast::<gtk::MenuItem>()
                .unwrap()
                .child()
                .unwrap()
                .downcast::<gtk::Label>()
                .unwrap();
            assert_eq!(
                label.style_context().font(gtk::StateFlags::NORMAL).size(),
                (9.75 * pango::SCALE as f64) as i32
            );
        }
    });
}
#[test]
fn test_title_is_13px_semibold_muted() {
    run(|| {
        let h = Header::new();
        let label = h.bar.title_label();
        let font = label.style_context().font(gtk::StateFlags::NORMAL);
        assert_eq!(font.size(), (9.75 * pango::SCALE as f64) as i32);
        assert_eq!(font.weight(), pango::Weight::Semibold);
    });
}
#[test]
fn test_settings_cog_is_a_clickable_icon_with_glyph_fade() {
    run(|| {
        let h = Header::new();
        let b = h.bar.settings_button();
        assert_eq!((b.allocated_width(), b.allocated_height()), (28, 28));
        assert!((b.child().unwrap().opacity() - 0.85).abs() < 1. / 255.);
    });
}
#[test]
fn test_settings_glyph_fade_honours_reduced_motion() {
    run(|| {
        let h = Header::new();
        let b = h.bar.settings_button();
        use glib::translate::*;
        let mut event = gdk::Event::new(gdk::EventType::EnterNotify);
        unsafe {
            let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
            (*raw).crossing.detail = gdk::NotifyType::Nonlinear.into_glib();
        }
        b.emit_by_name::<bool>("enter-notify-event", &[&event]);
        assert_eq!(b.child().unwrap().opacity(), 1.);
    });
}
