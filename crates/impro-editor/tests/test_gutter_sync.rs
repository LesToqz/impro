mod common;
use common::*;
use gtk::prelude::*;
fn verify(t: &impro_editor::EditorTab) {
    pump(50);
    let view = &t.view;
    let rect = view.visible_rect();
    let (mut at, _) = view.line_at_y(rect.y());
    let mut expected = 0;
    let mut previous = -1;
    loop {
        let (y, h) = view.line_yrange(&at);
        if y >= rect.y() + rect.height() || h <= 0 || y <= previous {
            break;
        }
        previous = y;
        expected += 1;
        let line = at.line();
        at.forward_line();
        if at.line() == line {
            break;
        }
    }
    assert_eq!(t.chrome.painted_lines(), expected);
    let active = t
        .buffer
        .iter_at_mark(&t.buffer.get_insert().unwrap())
        .line();
    for row in t.chrome.painted_rows() {
        let at = t.buffer.iter_at_line(row.line);
        let (y, height) = view.line_yrange(&at);
        assert_eq!((row.y, row.height), (y - rect.y(), height));
        assert_eq!(row.active, row.line == active);
    }
    assert!(expected < 150);
}
fn at(t: &impro_editor::EditorTab, line: i32) {
    let mut it = t.buffer.iter_at_line(line);
    t.buffer.place_cursor(&it);
    t.view.scroll_to_iter(&mut it, 0.0, true, 0.0, 0.5);
    pump(250);
}
#[test]
fn test_first_open_and_background_tab_mapping() {
    run(|| {
        let h = Harness::new();
        let a = h.open("a.txt", &"row text\n".repeat(400));
        verify(&a);
        let b = h.open("b.txt", &"background text\n".repeat(500));
        verify(&b);
        h.panel.notebook().set_current_page(Some(0));
        verify(&a);
    });
}
#[test]
fn test_click_arrows_pages_home_end() {
    run(|| {
        use gdk::keys::constants as k;
        use glib::translate::*;
        let h = Harness::new();
        let t = h.open("a.txt", &"row text\n".repeat(400));
        at(&t, 80);
        let location = t.view.iter_location(&t.buffer.iter_at_line(76));
        let (x, y) =
            t.view
                .buffer_to_window_coords(gtk::TextWindowType::Text, location.x(), location.y());
        let mut event = button_event(1);
        unsafe {
            let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
            (*raw).button.window =
                gtk::prelude::TextViewExt::window(&t.view, gtk::TextWindowType::Text)
                    .unwrap()
                    .to_glib_full();
            (*raw).button.x = (x + 2) as f64;
            (*raw).button.y = (y + 3) as f64;
        }
        let device = h.window.display().default_seat().unwrap().pointer();
        event.set_device(device.as_ref());
        t.view.event(&event);
        pump(100);
        assert_eq!(
            t.buffer
                .iter_at_mark(&t.buffer.get_insert().unwrap())
                .line(),
            76
        );
        verify(&t);
        for (key, control) in [
            (k::Down, false),
            (k::Up, false),
            (k::Page_Down, false),
            (k::Page_Up, false),
            (k::End, true),
            (k::Home, true),
        ] {
            let before = t.cursor();
            assert!(gtk::bindings_activate(
                &t.view,
                *key,
                if control {
                    gdk::ModifierType::CONTROL_MASK
                } else {
                    gdk::ModifierType::empty()
                }
            ));
            pump(250);
            assert_ne!(t.cursor(), before);
            verify(&t);
        }
    });
}
#[test]
fn test_wheel_and_scrollbar_drag() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", &"row text\n".repeat(400));
        for value in [0.0, 200.0, 800.0, 2000.0, 50.0] {
            t.source_scroll.vadjustment().set_value(value);
            verify(&t);
        }
    });
}
#[test]
fn test_typing_enter_delete_paste_undo_redo() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", &"row text\n".repeat(400));
        at(&t, 37);
        h.panel.edit("enter");
        verify(&t);
        t.buffer.begin_user_action();
        t.buffer.insert_at_cursor("pasted\nrows\n");
        t.buffer.end_user_action();
        verify(&t);
        t.undo(false);
        verify(&t);
        t.undo(true);
        verify(&t);
    });
}
#[test]
fn test_zoom_resize_dock_and_theme() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", &"row text\n".repeat(400));
        at(&t, 37);
        for (size, w, height) in [
            (7, 900, 600),
            (18, 600, 500),
            (32, 800, 400),
            (11, 500, 300),
        ] {
            h.panel.set_font_size(size);
            h.window.resize(w, height);
            verify(&t);
        }
        for theme in impro_core::themes::themes() {
            h.panel.apply_theme(theme);
            verify(&t);
        }
    });
}
#[test]
fn test_hide_show_gutter_preserves_scroll_alignment_and_highlight_toggle() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", &"row text\n".repeat(400));
        at(&t, 37);
        let value = t.source_scroll.vadjustment().value();
        h.panel.set_line_numbers(false);
        h.panel.set_highlight_line(false);
        pump(30);
        assert_eq!(t.chrome.width(), 0);
        h.panel.set_line_numbers(true);
        h.panel.set_highlight_line(true);
        verify(&t);
        assert!((t.source_scroll.vadjustment().value() - value).abs() < 2.0);
    });
}
#[test]
fn test_twenty_thousand_lines_digit_growth_and_cost() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", &"row text\n".repeat(20000));
        assert_eq!(impro_editor::gutter::digits_for(t.buffer.line_count()), 5);
        for line in [0, 37, 2000, 10000, 19999] {
            at(&t, line);
            verify(&t);
        }
    });
}
#[test]
fn test_fallback_and_taller_line() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", &"row text\n".repeat(250));
        let tag = gtk::TextTag::new(Some("taller"));
        tag.set_size_points(32.0);
        t.buffer.tag_table().unwrap().add(&tag);
        t.buffer
            .apply_tag(&tag, &t.buffer.iter_at_line(3), &t.buffer.iter_at_line(4));
        at(&t, 3);
        verify(&t);
    });
}
#[test]
fn test_scale_two() {
    if std::env::var_os("IMPRO_EDITOR_SCALE_CHILD").is_some() {
        run(|| {
            let h = Harness::new();
            let t = h.open("a.txt", &"row text\n".repeat(250));
            assert_eq!(t.view.scale_factor(), 2);
            at(&t, 37);
            verify(&t);
        });
    } else {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "test_scale_two", "--nocapture"])
            .env("GDK_SCALE", "2")
            .env("IMPRO_EDITOR_SCALE_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
