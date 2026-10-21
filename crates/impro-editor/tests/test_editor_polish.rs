mod common;
use common::*;
use gdk::keys::constants as k;
use gtk::prelude::*;
use impro_editor::{gutter, Response};
use std::time::Instant;
fn tagged(t: &impro_editor::EditorTab, tag: &gtk::TextTag) -> Vec<i32> {
    (0..t.buffer.char_count())
        .filter(|i| t.buffer.iter_at_offset(*i).has_tag(tag))
        .collect()
}
#[test]
fn test_gutter_width_grows_with_digits() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", &"x\n".repeat(998));
        let width = t.chrome.width();
        t.buffer.insert_at_cursor("x\n");
        assert!(t.chrome.width() > width);
        assert_eq!(gutter::digits_for(999), 3);
        assert_eq!(gutter::digits_for(1000), 4);
    });
}
#[test]
fn test_width_follows_typing() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", &"x\n".repeat(999));
        let width = t.chrome.width();
        t.buffer.insert_at_cursor(&"x\n".repeat(9000));
        assert!(t.chrome.width() > width);
        t.undo(false);
        assert_eq!(t.chrome.width(), width);
    });
}
#[test]
fn test_paints_only_visible_lines() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", &"a row\n".repeat(5000));
        pump(60);
        assert!(t.chrome.painted_lines() > 0);
        assert!(t.chrome.painted_lines() < 100);
        let mut at = t.buffer.iter_at_line(3000);
        t.view.scroll_to_iter(&mut at, 0.0, true, 0.0, 0.5);
        pump(60);
        assert!(t.view.visible_rect().y() > 1000);
        assert!(t.chrome.painted_lines() < 100);
    });
}
#[test]
fn test_last_empty_line_is_numbered() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", "a\nb\n");
        pump(50);
        assert_eq!(t.chrome.painted_lines(), 3);
    });
}
#[test]
fn test_active_line_colour_differs() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", "a\nb\nc\n");
        t.buffer.place_cursor(&t.buffer.iter_at_line(1));
        let colors = t.chrome.colors();
        assert_ne!(colors["gutter_fg"], colors["gutter_fg_active"]);
        assert!(t.buffer.iter_at_line(1).has_tag(&t.chrome.line_tag()));
    });
}
#[test]
fn test_current_line_follows_caret_edits_and_undo() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", "one\ntwo\nthree\n");
        t.buffer.place_cursor(&t.buffer.iter_at_line(1));
        assert_eq!(tagged(&t, &t.chrome.line_tag()), vec![4, 5, 6, 7]);
        t.buffer.place_cursor(&t.buffer.iter_at_line(2));
        assert_eq!(tagged(&t, &t.chrome.line_tag())[0], 8);
        t.buffer.insert_at_cursor("x\n");
        assert_eq!(tagged(&t, &t.chrome.line_tag())[0], 10);
        t.undo(false);
        assert!(!t.buffer.is_modified());
        t.buffer
            .select_range(&t.buffer.iter_at_line(0), &t.buffer.iter_at_line(2));
        assert!(tagged(&t, &t.chrome.line_tag()).is_empty());
    });
}
#[test]
fn test_brackets() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", "f(a[1], {2})\n");
        for cursor in [1, 12] {
            t.buffer.place_cursor(&t.buffer.iter_at_offset(cursor));
            assert_eq!(tagged(&t, &t.chrome.bracket_tag()), vec![1, 11]);
        }
        t.buffer.place_cursor(&t.buffer.iter_at_offset(7));
        assert!(tagged(&t, &t.chrome.bracket_tag()).is_empty());
        let t = h.open("long.txt", &format!("({})\n", "x".repeat(25000)));
        t.buffer.place_cursor(&t.buffer.start_iter());
        assert!(tagged(&t, &t.chrome.bracket_tag()).is_empty());
        assert_eq!(gutter::find_match("<a>", 0), None);
        assert_eq!(gutter::find_match("😀(a[1])", 1), Some(6));
    });
}
#[test]
fn test_set_chrome() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", "x\n");
        let colors = [
            ("gutter_bg".into(), "#101010".into()),
            ("bogus".into(), "#fff".into()),
            ("bracket_bg".into(), "not a colour".into()),
        ]
        .into_iter()
        .collect();
        h.panel.set_chrome(&colors);
        assert_eq!(h.panel.chrome()["gutter_bg"], "#101010");
        assert_eq!(t.chrome.colors()["gutter_bg"], "#101010");
        assert!(!h.panel.chrome().contains_key("bogus"));
        assert_eq!(h.panel.chrome()["bracket_bg"], "#3a3d41");
        h.panel.set_chrome(
            &[("current_line_bg".into(), "#303030".into())]
                .into_iter()
                .collect(),
        );
        let new = h.open("b.txt", "y");
        assert_eq!(new.chrome.colors()["current_line_bg"], "#303030");
        assert!(gutter::valid_color("red"));
        assert!(!gutter::valid_color("wrong"));
    });
}
#[test]
fn test_zoom() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", "x");
        let width = t.chrome.width();
        key(&h.panel, k::plus, true, false);
        assert_eq!(h.panel.font_size(), 12);
        assert!(t.chrome.width() > width);
        h.panel.set_font_size(1000);
        assert_eq!(h.panel.font_size(), 32);
        h.panel.set_font_size(-100);
        assert_eq!(h.panel.font_size(), 7);
        key(&h.panel, k::_0, true, false);
        assert_eq!(t.chrome.width(), width);
    });
}
#[test]
fn test_big_file_draw_is_cheap() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", &"row text\n".repeat(65000));
        pump(50);
        let start = Instant::now();
        let mut at = t.buffer.iter_at_line(60000);
        t.view.scroll_to_iter(&mut at, 0.0, true, 0.0, 0.5);
        pump(50);
        t.chrome.update_labels();
        assert!(start.elapsed().as_millis() < 1000);
        assert!(!t.buffer.is_modified());
        assert!(t.chrome.painted_lines() < 100);
    });
}
#[test]
fn test_modified_stays_false_and_find_works() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "def f():\n    return 1\n");
        h.settled();
        h.panel.zoom(2);
        h.panel.set_chrome(
            &[("gutter_fg".into(), "#ffffff".into())]
                .into_iter()
                .collect(),
        );
        assert!(!t.buffer.is_modified());
        h.panel.search_entry().set_text("return");
        h.panel.find(false, false);
        assert_eq!(selected(&t), "return");
    });
}
#[test]
fn test_header_tab_sizes_and_rounded_corner_pixels() {
    run(|| {
        let h = Harness::new();
        h.window.resize(900, 600);
        let first = h.open("a.txt", "a");
        h.open("b.txt", "b");
        h.panel.notebook().set_current_page(Some(0));
        for id in ["impro-dark", "impro-light"] {
            let theme = impro_core::themes::get_theme(Some(id));
            impro_ui::apply_style(Some(theme)).unwrap();
            h.panel.apply_theme(theme);
            pump(80);
            let (hx, hy) = first
                .header
                .widget
                .translate_coordinates(&h.window, 0, 0)
                .unwrap();
            assert_eq!(first.header.widget.allocated_height() + 2, 34);
            assert_eq!(first.header.widget.allocated_width() + 8, 200);
            assert_eq!(
                first
                    .page
                    .translate_coordinates(&h.panel.notebook(), 0, 0)
                    .unwrap()
                    .1,
                40
            );
            let image = pixels(&h.window);
            assert_eq!(pixel(&image, hx - 4, hy - 1), rgb(&theme.ui["sidebar_bg"]));
            assert_eq!(pixel(&image, hx + 80, hy - 1), rgb(&theme.ui["border"]));
            assert_eq!(pixel(&image, hx + 80, hy + 1), rgb(&theme.ui["bg"]));
        }
    });
}
#[test]
fn test_close_and_dividers_hover_instant_and_dirty_stays_visible() {
    run(|| {
        let h = Harness::new();
        let first = h.open("a.txt", "a");
        let second = h.open("b.txt", "b");
        let third = h.open("c.txt", "c");
        pump(50);
        let opacity = |t: &impro_editor::EditorTab| {
            t.header
                .close
                .style_context()
                .style_property_for_state("opacity", t.header.close.state_flags())
                .get::<f64>()
                .unwrap()
        };
        assert_eq!(opacity(&first), 0.0);
        assert_eq!(opacity(&third), 1.0);
        first.header.widget.emit_by_name::<bool>(
            "enter-notify-event",
            &[&gdk::Event::new(gdk::EventType::EnterNotify)],
        );
        assert_eq!(opacity(&first), 1.0);
        first.header.widget.emit_by_name::<bool>(
            "leave-notify-event",
            &[&gdk::Event::new(gdk::EventType::LeaveNotify)],
        );
        assert_eq!(opacity(&first), 0.0);
        replace(&first, "changed");
        assert!(first.header.label.text().starts_with("● "));
        assert_eq!(first.header.divider.opacity(), 1.0);
        assert_eq!(second.header.divider.opacity(), 0.0);
        assert_eq!(third.header.divider.opacity(), 0.0);
        assert_eq!(first.header.divider.allocated_width(), 1);
        assert_eq!(first.header.divider.allocated_height(), 13);
    });
}
#[test]
fn test_tab_widths_shrink_ellipsis_and_reorder_middle_close() {
    run(|| {
        let h = Harness::new();
        let mut tabs = Vec::new();
        for i in 0..8 {
            tabs.push(h.open(&format!("long-file-{i}.txt"), "x"));
        }
        pump(50);
        for t in &tabs {
            assert_eq!(t.header.label.ellipsize(), pango::EllipsizeMode::End);
            assert_eq!(t.header.widget.width_request(), 112);
        }
        h.panel.notebook().reorder_child(&tabs[7].page, Some(0));
        h.answer(Response::Discard);
        assert!(h.close(&tabs[0]));
        assert_eq!(h.panel.notebook().n_pages(), 7);
    });
}
#[test]
fn test_status_bottom_right_reserves_last_line_with_find_and_zoom() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", &"text\n".repeat(100));
        for size in [7, 11, 32] {
            for find in [false, true] {
                h.panel.set_font_size(size);
                h.panel.search_bar().set_search_mode(find);
                pump(80);
                let status = h.panel.status().widget;
                assert_eq!(h.panel.stack().margin_bottom(), status.allocated_height());
                assert_eq!(status.halign(), gtk::Align::End);
                assert_eq!(status.valign(), gtk::Align::End);
                assert!(!t.buffer.is_modified());
            }
        }
    });
}
#[test]
fn test_status_item_hover_has_real_overlay_pixels() {
    run(|| {
        let h = Harness::new();
        h.open("a.txt", "x");
        let status = h.panel.status();
        let item = status.items[0].parent().unwrap();
        let (x, y) = item.translate_coordinates(&h.window, 1, 5).unwrap();
        let before = pixel(&pixels(&h.window), x, y);
        let event = gdk::Event::new(gdk::EventType::EnterNotify);
        status.targets[0].emit_by_name::<bool>("enter-notify-event", &[&event]);
        pump(50);
        assert_ne!(before, pixel(&pixels(&h.window), x, y));
        let event = gdk::Event::new(gdk::EventType::LeaveNotify);
        status.targets[0].emit_by_name::<bool>("leave-notify-event", &[&event]);
        pump(50);
        assert_eq!(before, pixel(&pixels(&h.window), x, y));
    });
}
#[test]
fn test_gutter_defaults_roles_gap_and_toggles_survive_show_all() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", "a\nb\nc");
        h.panel.set_line_numbers(false);
        h.panel.set_highlight_line(false);
        h.window.show_all();
        pump(30);
        assert_eq!(t.chrome.width(), 0);
        assert!(tagged(&t, &t.chrome.line_tag()).is_empty());
        h.panel.set_line_numbers(true);
        h.panel.set_highlight_line(true);
        assert!(t.chrome.width() > 0);
    });
}
#[test]
fn test_indent_changes_insertion_unindent_and_tab_stops() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", "");
        h.panel.edit("tab");
        assert_eq!(t.text(), "    ");
        h.panel.set_indent(false, 2).unwrap();
        let width = t.view.tabs().unwrap().tab(0).1;
        h.panel.edit("tab");
        assert_eq!(t.text(), "      ");
        h.panel.edit("unindent");
        assert_eq!(t.text(), "    ");
        h.panel.set_indent(true, 8).unwrap();
        assert!(t.view.tabs().unwrap().tab(0).1 > width);
        h.panel.edit("tab");
        assert_eq!(t.text(), "    \t");
        assert!(h.panel.set_indent(false, 0).is_err());
    });
}
#[test]
fn test_auto_pair_default_toggle_skip_selection_and_undo() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", "");
        assert!(!h.panel.auto_pair());
        h.panel.set_auto_pair(true);
        key(&h.panel, k::parenleft, false, false);
        assert_eq!(t.text(), "()");
        assert_eq!(t.cursor(), 1);
        key(&h.panel, k::parenright, false, false);
        assert_eq!(t.text(), "()");
        assert_eq!(t.cursor(), 2);
        t.buffer
            .select_range(&t.buffer.start_iter(), &t.buffer.end_iter());
        key(&h.panel, k::bracketleft, false, false);
        assert_eq!(t.text(), "[()]");
        assert_eq!(selected(&t), "()");
        t.undo(false);
        assert_eq!(t.text(), "()");
    });
}
#[test]
fn test_markdown_defaults_readable_width_and_new_tab_preview() {
    run(|| {
        let h = Harness::new();
        h.open("a.md", "# Title");
        assert!(h.panel.md_readable_width());
        assert_eq!(h.panel.markdown_mode(None), Some("preview"));
        h.panel.set_md_live_preview(false);
        h.panel.set_md_readable_width(false);
        h.open("b.md", "# B");
        assert_eq!(h.panel.markdown_mode(None), Some("source"));
    });
}
#[test]
fn test_close_reflow_motion_and_reduced_motion() {
    run(|| {
        let h = Harness::new();
        let a = h.open("a.txt", "a");
        h.open("b.txt", "b");
        assert!(h.close(&a));
        pump(350);
        assert!(h
            .panel
            .tabs()
            .iter()
            .all(|t| t.header.widget.margin_start() == 0));
    });
}
#[test]
fn test_component_css_parses_every_theme() {
    run(|| {
        for theme in impro_core::themes::themes() {
            let provider = gtk::CssProvider::new();
            provider
                .load_from_data(impro_editor::css(&theme.ui).as_bytes())
                .unwrap();
        }
    });
}
