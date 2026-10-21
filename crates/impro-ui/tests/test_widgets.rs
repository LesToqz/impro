mod common;
use common::*;
use gtk::prelude::*;
use impro_ui::{motion, style, widgets::*};
use std::{cell::RefCell, rc::Rc};
fn setup() {
    impro_ui::init();
    style::apply_style(None).unwrap();
}
#[test]
fn test_geometry_role_and_animated_thumb() {
    run(|| {
        setup();
        motion::set_reduce_motion(false);
        let toggle = Toggle::new(false, Some("Line numbers"));
        let layout = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        layout.pack_start(toggle.widget(), false, false, 0);
        let window = window(&layout);
        assert_eq!(
            (toggle.allocated_width(), toggle.allocated_height()),
            (40, 22)
        );
        assert_eq!(
            (
                toggle.thumb.allocated_width(),
                toggle.thumb.allocated_height()
            ),
            (18, 18)
        );
        assert_eq!(
            toggle.accessible().unwrap().role(),
            atk::Role::for_name("switch")
        );
        toggle.set_active(true);
        pump(60);
        assert!(toggle.thumb_offset() > 2 && toggle.thumb_offset() < 20);
        pump(140);
        assert_eq!(toggle.thumb_offset(), 20);
        unsafe {
            window.destroy();
        }
    });
}
#[test]
fn test_reduce_motion_and_unmapped_toggles_jump() {
    run(|| {
        setup();
        let toggle = Toggle::new(true, None);
        assert_eq!(toggle.thumb_offset(), 20);
        toggle.set_active(false);
        assert_eq!(toggle.thumb_offset(), 2);
        motion::set_reduce_motion(true);
        toggle.set_active(true);
        assert_eq!(toggle.thumb_offset(), 20);
        motion::set_reduce_motion(false);
    });
}
#[test]
fn test_row_search_reset_and_activation() {
    run(|| {
        setup();
        let resets = Rc::new(RefCell::new(0));
        let r = resets.clone();
        let toggle = Toggle::default();
        let row = SettingRow::new(
            "Accent color",
            "Colour of toggles.",
            Some(toggle.widget().upcast_ref()),
            RowOptions {
                key: Some("accent"),
                aliases: &["highlight brand"],
                reset: Some(Box::new(move || *r.borrow_mut() += 1)),
                ..Default::default()
            },
        );
        assert!(row.matches("ACCENT"));
        assert!(row.matches("brand col"));
        assert!(!row.matches("terminal"));
        row.reset_button.as_ref().unwrap().emit_clicked();
        assert_eq!(*resets.borrow(), 1);
        assert!(row.activate_control());
        assert!(toggle.is_active());
        assert_eq!(
            toggle.accessible().unwrap().name().as_deref(),
            Some("Accent color")
        );
    });
}
#[test]
fn test_card_dividers_filter_and_radius() {
    run(|| {
        setup();
        let card = SettingCard::new();
        let rows: [SettingRow; 3] = ["Alpha", "Beta", "Gamma"]
            .map(|name| SettingRow::new(name, "Description", None, Default::default()));
        for r in &rows {
            card.add_row(r);
        }
        let window = window(card.widget());
        assert!(rows[0].header().is_none());
        assert_eq!(rows[1].header().unwrap().allocated_height(), 1);
        card.set_query("beta");
        pump(5);
        assert_eq!(card.visible_rows().len(), 1);
        assert!(rows[1].header().is_none());
        card.set_query("");
        assert_eq!(card.visible_rows().len(), 3);
        unsafe {
            window.destroy();
        }
    });
}
#[test]
fn test_hotkey_chips_wrap() {
    run(|| {
        setup();
        let wrap = WrapBox::default();
        let chips: Vec<_> = (0..12)
            .map(|n| HotkeyChip::new(&format!("Ctrl+Shift+{n}")))
            .collect();
        for c in &chips {
            wrap.add(c.widget());
        }
        let window = window(&wrap);
        let lines: std::collections::BTreeSet<_> =
            chips.iter().map(|c| c.allocation().y()).collect();
        assert!(lines.len() > 1);
        for c in &chips {
            assert_eq!(c.allocated_width(), c.preferred_width().1);
        }
        wrap.remove(chips[0].widget());
        assert_eq!(wrap.children().len(), 11);
        unsafe {
            window.destroy();
        }
    });
}
#[test]
fn test_nav_rows_and_vim_keys() {
    run(|| {
        setup();
        let nav = NavList::new();
        for page in ["one", "two", "three"] {
            nav.add_page(page, page, "folder-symbolic");
        }
        let window = window(nav.widget());
        nav.select_page("one");
        use gdk::keys::constants as k;
        for (key, want) in [
            (k::j, "two"),
            (k::G, "three"),
            (k::k, "two"),
            (k::g, "two"),
            (k::g, "one"),
        ] {
            assert!(nav.handle_key(key, gdk::ModifierType::empty()));
            assert_eq!(nav.page_id().as_deref(), Some(want));
        }
        nav.rows()["three"].set_count(Some(4));
        assert_eq!(nav.rows()["three"].count_label.text(), "4");
        unsafe {
            window.destroy();
        }
    });
}
#[test]
fn test_centered_column() {
    run(|| {
        setup();
        let column = CenteredColumn::default();
        let child = gtk::Label::new(Some("x"));
        column.add(&child);
        let window = window(&column);
        window.resize(1000, 300);
        pump(70);
        assert_eq!(child.allocated_width(), 700);
        assert_eq!(
            child.allocation().x() - column.allocation().x(),
            (column.allocated_width() - 700) / 2
        );
        assert_eq!(child.allocation().y() - column.allocation().y(), 48);
        window.resize(600, 300);
        pump(70);
        assert_eq!(child.allocated_width(), 536);
        assert_eq!(child.allocation().x() - column.allocation().x(), 32);
        unsafe {
            window.destroy();
        }
    });
}
#[test]
fn test_swatches_and_search_field() {
    run(|| {
        setup();
        let picker = SwatchPicker::new(["#111111", "#222222"]);
        let events = Rc::new(RefCell::new(Vec::new()));
        let e = events.clone();
        picker.connect_value_changed(move |_, value| e.borrow_mut().push(value.map(str::to_owned)));
        picker.buttons()["#222222"].emit_clicked();
        picker.buttons()["#222222"].emit_clicked();
        assert_eq!(picker.value().as_deref(), Some("#222222"));
        assert_eq!(events.borrow().len(), 1);
        picker.set_value(None, false);
        assert!(!picker.buttons().values().any(|b| b.is_active()));
        let field = SearchField::default();
        let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
        layout.pack_start(field.widget(), false, false, 0);
        let window = window(&layout);
        assert_eq!(field.allocated_height(), 34);
        unsafe {
            window.destroy();
        }
    });
}
#[test]
fn test_css_parses_for_every_theme() {
    run(|| {
        for theme in impro_core::themes::themes() {
            let css = css(&theme.ui);
            style::create_provider(&css).unwrap();
            assert!(!css.contains("transition:"));
        }
    });
}
