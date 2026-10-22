mod common;
use common::*;
use gdk::keys::constants as key;
use gtk::prelude::*;
use impro_chrome::*;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};
#[test]
fn test_css_parses_for_every_theme() {
    run(|| {
        for theme in impro_core::themes::themes() {
            gtk::CssProvider::new()
                .load_from_data(css(&theme.ui).as_bytes())
                .unwrap();
        }
    });
}
#[test]
fn test_geometry_width_top_and_max_height() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(100);
        f.show(&p);
        let frame = p.frame();
        assert_eq!(frame.allocated_width(), 700);
        assert_eq!(
            frame.translate_coordinates(f.host.widget(), 0, 0).unwrap(),
            (100, 90)
        );
        assert_eq!(frame.allocated_height(), 420);
    });
}
#[test]
fn test_short_list_uses_natural_height() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(2);
        f.show(&p);
        assert!(p.frame().allocated_height() < 420);
    });
}
#[test]
fn test_narrow_host_clamps_width_to_eighty_percent() {
    run(|| {
        let f = Fixture::new();
        f.window.resize(600, 400);
        pump(50);
        let p = prompt(100);
        f.show(&p);
        assert_eq!(p.frame().allocated_width(), 480);
        assert_eq!(
            p.frame()
                .translate_coordinates(f.host.widget(), 0, 0)
                .unwrap(),
            (60, 60)
        );
    });
}
#[test]
fn test_row_and_input_heights() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(5);
        f.show(&p);
        assert_eq!(p.entry().allocated_height(), 50);
        assert!([31, 32].contains(&p.rows()[0].widget.allocated_height()));
    });
}
#[test]
fn test_result_cap() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(200);
        f.show(&p);
        assert_eq!(p.items().len(), 100);
        assert_eq!(p.rows().len(), 100);
    });
}
#[test]
fn test_keyboard_navigation_wraps_and_pages_clamp() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(30);
        f.show(&p);
        assert_eq!(p.selected_index(), 0);
        p.handle_key(key::Up, gdk::ModifierType::empty());
        assert_eq!(p.selected_index(), 29);
        p.handle_key(key::Down, gdk::ModifierType::empty());
        assert_eq!(p.selected_index(), 0);
        p.handle_key(key::End, gdk::ModifierType::empty());
        assert_eq!(p.selected_index(), 29);
        p.handle_key(key::Page_Down, gdk::ModifierType::empty());
        assert_eq!(p.selected_index(), 29);
        p.handle_key(key::Home, gdk::ModifierType::empty());
        assert_eq!(p.selected_index(), 0);
        p.handle_key(key::Page_Up, gdk::ModifierType::empty());
        assert_eq!(p.selected_index(), 0);
        assert!(p.handle_key(key::Tab, gdk::ModifierType::empty()));
        assert!(!p.handle_key(key::Down, gdk::ModifierType::CONTROL_MASK));
    });
}
#[test]
fn test_selection_is_instant_overlay_highlight() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(5);
        f.show(&p);
        p.select(3);
        assert!(p.rows()[3].widget.is_selected());
        assert!(!p.rows()[0].widget.is_selected());
        assert_eq!(p.list().selected_rows().len(), 1);
    });
}
#[test]
fn test_enter_and_modifier_enter_choose_then_restore_focus() {
    run(|| {
        let f = Fixture::new();
        for mods in [
            gdk::ModifierType::empty(),
            gdk::ModifierType::CONTROL_MASK,
            gdk::ModifierType::SHIFT_MASK,
            gdk::ModifierType::MOD1_MASK,
        ] {
            let p = prompt(5);
            f.show(&p);
            let events = Rc::new(RefCell::new(vec![]));
            let out = events.clone();
            p.connect_closed(move || out.borrow_mut().push("closed"));
            let out = events.clone();
            let text = f.text.clone();
            p.connect_chosen(move |item, state| {
                assert_eq!(item.data::<usize>(), Some(&0));
                assert_eq!(state, mods);
                assert!(text.has_focus());
                out.borrow_mut().push("chosen");
            });
            p.handle_key(key::Return, mods);
            assert_eq!(*events.borrow(), ["closed", "chosen"]);
        }
    });
}
#[test]
fn test_escape_closes_once_and_restores_focus() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(5);
        f.show(&p);
        let count = Rc::new(Cell::new(0));
        let c = count.clone();
        p.connect_closed(move || c.set(c.get() + 1));
        p.handle_key(key::Escape, gdk::ModifierType::empty());
        p.close();
        assert_eq!(count.get(), 1);
        assert!(f.host.prompt().is_none());
        assert!(f.text.has_focus());
    });
}
#[test]
fn test_focus_restore_keeps_entry_selection() {
    run(|| {
        let f = Fixture::new();
        f.host.remove(&f.text);
        let entry = gtk::Entry::new();
        f.host.add(&entry);
        entry.show();
        entry.set_text("abcde");
        entry.grab_focus();
        entry.select_region(1, 3);
        let p = prompt(5);
        f.show(&p);
        p.close();
        assert!(entry.has_focus());
        assert_eq!(entry.selection_bounds(), Some((1, 3)));
    });
}
#[test]
fn test_mouse_click_chooses_row() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(5);
        f.show(&p);
        let chosen = Rc::new(Cell::new(99));
        let c = chosen.clone();
        p.connect_chosen(move |item, _| c.set(*item.data::<usize>().unwrap()));
        let row = p.rows()[3].widget.clone();
        p.list().emit_by_name::<()>("row-activated", &[&row]);
        assert_eq!(chosen.get(), 3);
        assert!(f.text.has_focus());
    });
}
#[test]
fn test_pointer_motion_moves_the_single_highlight() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(5);
        f.show(&p);
        use glib::translate::*;
        let row = p.rows()[2].widget.allocation();
        let mut event = gdk::Event::new(gdk::EventType::MotionNotify);
        unsafe {
            let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
            (*raw).motion.x = 30.;
            (*raw).motion.y = (row.y() + 10) as f64;
            (*raw).motion.x_root = 300.;
            (*raw).motion.y_root = 250.;
        }
        p.list()
            .emit_by_name::<bool>("motion-notify-event", &[&event]);
        assert_eq!(p.selected_index(), 2);
        p.handle_key(key::Down, gdk::ModifierType::empty());
        assert_eq!(p.selected_index(), 3);
    });
}
#[test]
fn test_outside_click_closes() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(5);
        f.show(&p);
        click(p.widget(), 1., 1.);
        assert!(f.host.prompt().is_none());
        assert!(f.text.has_focus());
        f.show(&p);
        let pos = p.frame().allocation();
        click(p.widget(), (pos.x() + 10) as f64, (pos.y() + 10) as f64);
        assert!(f.host.prompt().is_some());
    });
}
#[test]
fn test_markup_is_escaped_and_matches_are_bold() {
    run(|| {
        let f = Fixture::new();
        let p = Prompt::new(
            |q| {
                vec![PromptItem {
                    label: "a<b&c".into(),
                    detail: "x>y".into(),
                    label_ranges: fuzzy::r#match(q, "a<b&c", false).unwrap().1,
                    ..Default::default()
                }]
            },
            "",
        );
        f.show(&p);
        p.set_query("b&");
        assert_eq!(p.rows()[0].title.text(), "a<b&c");
        assert_eq!(p.rows()[0].title.label(), "a&lt;<b>b&amp;</b>c");
        assert_eq!(p.rows()[0].detail.text(), "x>y");
    });
}
#[test]
fn test_typing_filters_and_enter_flushes_pending_query() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(20);
        f.show(&p);
        p.entry().set_text("item 1");
        pump(20);
        assert_eq!(
            p.items()
                .iter()
                .map(|i| *i.data::<usize>().unwrap())
                .collect::<Vec<_>>(),
            [1, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19]
        );
        let out = Rc::new(Cell::new(0));
        let c = out.clone();
        p.connect_chosen(move |i, _| c.set(*i.data::<usize>().unwrap()));
        p.entry().set_text("item 13");
        p.handle_key(key::Return, gdk::ModifierType::empty());
        assert_eq!(out.get(), 13);
    });
}
#[test]
fn test_empty_state_and_query_chosen() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(5);
        p.set_empty_text("Nothing here.");
        f.show(&p);
        p.set_query("zzz");
        assert!(p.items().is_empty());
        assert!(p.empty().is_visible());
        assert!(!p.scroller().is_visible());
        assert_eq!(p.empty().text(), "Nothing here.");
        let out = Rc::new(RefCell::new(String::new()));
        let c = out.clone();
        p.connect_query_chosen(move |q, mods| {
            assert_eq!(mods, gdk::ModifierType::SHIFT_MASK);
            *c.borrow_mut() = q.into();
        });
        p.handle_key(key::Down, gdk::ModifierType::empty());
        p.handle_key(key::Return, gdk::ModifierType::SHIFT_MASK);
        assert_eq!(*out.borrow(), "zzz");
    });
}
#[test]
fn test_footer_instructions() {
    run(|| {
        let p = prompt(5);
        let texts = p.footer().children()[0]
            .clone()
            .downcast::<gtk::Box>()
            .unwrap()
            .children()
            .into_iter()
            .map(|w| w.downcast::<gtk::Label>().unwrap().text().to_string())
            .collect::<Vec<_>>();
        assert_eq!(texts, ["↑↓ to navigate", "↵ to use", "esc to dismiss"]);
        p.set_instructions(&[]);
        assert!(!p.footer().get_visible());
        p.set_instructions(&[("<x>", "a & b")]);
        assert!(p.footer().get_visible());
    });
}
#[test]
fn test_one_prompt_at_a_time_and_reopen_resets() {
    run(|| {
        let f = Fixture::new();
        let first = prompt(5);
        f.show(&first);
        first.set_query("item 3");
        let second = prompt(5);
        f.show(&second);
        assert!(!first.is_open());
        assert!(first.parent().is_none());
        second.close();
        assert!(f.text.has_focus());
        f.show(&first);
        assert_eq!(first.query(), "");
        assert_eq!(first.items().len(), 5);
        assert_eq!(first.selected_index(), 0);
    });
}
#[test]
fn test_set_provider_and_refresh() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(5);
        f.show(&p);
        p.set_provider(|_| {
            vec![PromptItem {
                label: "other".into(),
                hotkey: vec!["Ctrl+K".into(), "F2".into()],
                ..Default::default()
            }]
        });
        assert_eq!(p.items()[0].label, "other");
        assert_eq!(p.rows()[0].chips.children().len(), 2);
        assert!(!p.rows()[1].widget.is_visible());
    });
}
#[test]
fn test_quick_switcher_lists_recent_first_then_alphabetical() {
    run(|| {
        let f = Fixture::new();
        let s = QuickSwitcher::new("/r");
        s.set_files(
            [
                "/r/src/b.py",
                "/r/a.md",
                "/r/Zeta.md",
                "/r/src/plan/Plan.md",
                "/elsewhere/x.md",
            ]
            .map(PathBuf::from)
            .to_vec(),
            vec!["/r/Zeta.md".into()],
        );
        f.show(&s);
        assert_eq!(
            s.items()
                .iter()
                .map(|i| (i.label.as_str(), i.detail.as_str()))
                .collect::<Vec<_>>(),
            [
                ("Zeta.md", ""),
                ("x.md", "/elsewhere"),
                ("a.md", ""),
                ("b.py", "src"),
                ("Plan.md", "src/plan")
            ]
        );
        s.set_query("pla");
        assert_eq!(s.items()[0].label, "Plan.md");
        assert_eq!(s.items()[0].label_ranges, [(0, 3)]);
        assert_eq!(s.rows()[0].title.label(), "<b>Pla</b>n.md");
        assert_eq!(
            s.items()[0].data::<PathBuf>(),
            Some(&PathBuf::from("/r/src/plan/Plan.md"))
        );
    });
}
#[test]
fn test_quick_switcher_without_recent_is_alphabetical_and_reloads() {
    run(|| {
        let f = Fixture::new();
        let s = QuickSwitcher::new("");
        s.set_files(
            ["b.md", "A.md", "c/a.md"].map(PathBuf::from).to_vec(),
            vec![],
        );
        f.show(&s);
        assert_eq!(
            s.items()
                .iter()
                .map(|i| i.data::<PathBuf>().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["A.md", "b.md", "c/a.md"]
        );
        s.close();
        s.set_files(
            ["b.md", "A.md", "c/a.md", "0.md"]
                .map(PathBuf::from)
                .to_vec(),
            vec![],
        );
        f.show(&s);
        assert_eq!(s.items()[0].label, "0.md");
    });
}
#[test]
fn test_quick_switcher_twenty_thousand_files() {
    run(|| {
        let f = Fixture::new();
        let s = QuickSwitcher::new("");
        s.set_files(
            (0..20000)
                .map(|i| format!("src/plan/main_helper{i}.md").into())
                .collect(),
            vec![],
        );
        let start = std::time::Instant::now();
        f.host.show_prompt(&s);
        println!(
            "open prompt {:.3} ms",
            start.elapsed().as_secs_f64() * 1000.
        );
        poll(|| s.index_ready() && s.items().len() == 100);
        for q in ["p", "pl", "pla", "plan", "plan m", "e"] {
            s.set_query(q);
            poll(|| s.items().len() == 100);
        }
        assert_eq!(s.rows().len(), 100);
    });
}
#[test]
fn test_command_palette_runs_callback_after_close_and_tracks_recent() {
    run(|| {
        let f = Fixture::new();
        let calls = Rc::new(RefCell::new(vec![]));
        let out = calls.clone();
        let text = f.text.clone();
        let p = CommandPalette::new(move || {
            let out = out.clone();
            let text = text.clone();
            vec![
                Command::new(
                    "File: Save",
                    move || {
                        assert!(text.has_focus());
                        out.borrow_mut().push("save");
                    },
                    vec!["Ctrl+S".into()],
                ),
                Command::new("View: Toggle explorer", || {}, vec!["Ctrl+B".into()]),
                Command::new("Editor: Toggle comment", || {}, vec!["Ctrl+/".into()]),
            ]
        });
        f.show(&p);
        assert_eq!(p.items()[0].label, "File: Save");
        p.set_query("tog com");
        assert_eq!(p.items()[0].label, "Editor: Toggle comment");
        p.choose(gdk::ModifierType::empty());
        f.show(&p);
        assert_eq!(p.items()[0].label, "Editor: Toggle comment");
        p.set_query("save");
        p.choose(gdk::ModifierType::empty());
        assert_eq!(*calls.borrow(), ["save"]);
        assert_eq!(p.recent(), ["File: Save", "Editor: Toggle comment"]);
    });
}
#[test]
#[ignore]
fn benchmark_prompt() {
    run(|| {
        let f = Fixture::new();
        let s = QuickSwitcher::new("");
        let start = std::time::Instant::now();
        s.set_files(
            (0..20000)
                .map(|i| format!("src/plan/main_helper{i}.md").into())
                .collect(),
            vec![],
        );
        let dispatch = start.elapsed();
        poll(|| s.index_ready() && s.items().len() == 100);
        pump(30);
        let painted = Rc::new(Cell::new(None::<std::time::Duration>));
        let flag = painted.clone();
        let start = std::time::Instant::now();
        let handler = s.widget().connect_local("draw", true, move |_| {
            flag.set(flag.get().or_else(|| Some(start.elapsed())));
            Some(false.to_value())
        });
        let mapped = Rc::new(Cell::new(None::<std::time::Duration>));
        let flag = mapped.clone();
        let map_handler = s.widget().connect_map(move |_| {
            flag.set(flag.get().or_else(|| Some(start.elapsed())));
        });
        f.host.show_prompt(&s);
        println!(
            "open ready 20k prompt: {:.3} ms; index dispatch {:.3} ms",
            start.elapsed().as_secs_f64() * 1000.,
            dispatch.as_secs_f64() * 1000.
        );
        poll(|| painted.get().is_some());
        println!(
            "open to first draw {:.3} ms",
            painted.get().unwrap().as_secs_f64() * 1000.
        );
        println!(
            "open to map {:.3} ms; map to first draw {:.3} ms",
            mapped.get().unwrap().as_secs_f64() * 1000.,
            painted
                .get()
                .unwrap()
                .saturating_sub(mapped.get().unwrap())
                .as_secs_f64()
                * 1000.
        );
        s.widget().disconnect(handler);
        s.widget().disconnect(map_handler);
        for reopen in 0..3 {
            f.host.hide_prompt();
            pump(40);
            let painted = Rc::new(Cell::new(None::<std::time::Duration>));
            let flag = painted.clone();
            let start = std::time::Instant::now();
            let handler = s.widget().connect_local("draw", true, move |_| {
                flag.set(flag.get().or_else(|| Some(start.elapsed())));
                Some(false.to_value())
            });
            f.host.show_prompt(&s);
            poll(|| painted.get().is_some());
            println!(
                "reopen {reopen} to first draw {:.3} ms",
                painted.get().unwrap().as_secs_f64() * 1000.
            );
            s.widget().disconnect(handler);
        }
        pump(50);
        let start = std::time::Instant::now();
        s.set_query("main");
        let dispatch = start.elapsed();
        poll(|| !s.search_pending());
        println!(
            "async query dispatch {:.3} ms; delivery {:.3} ms",
            dispatch.as_secs_f64() * 1000.,
            start.elapsed().as_secs_f64() * 1000.
        );
        let items = s.items();
        let update = std::time::Instant::now();
        s.set_items(items.clone());
        println!(
            "update 100 mapped rows {:.3} ms",
            update.elapsed().as_secs_f64() * 1000.
        );
        let start = std::time::Instant::now();
        let p = Prompt::new(|_| vec![], "");
        p.set_items(items);
        println!(
            "create 100 rows: {:.3} ms",
            start.elapsed().as_secs_f64() * 1000.
        );
    });
}
#[test]
fn test_async_enter_waits_for_current_query_and_reuses_rows() {
    run(|| {
        let f = Fixture::new();
        let s = QuickSwitcher::new("");
        s.set_files(
            (0..20000)
                .map(|i| format!("src/plan/main_helper{i}.md").into())
                .collect(),
            vec![],
        );
        poll(|| s.index_ready());
        f.show(&s);
        let first = s.rows()[0].widget.clone();
        let chosen = Rc::new(RefCell::new(None::<PathBuf>));
        let c = chosen.clone();
        s.connect_chosen(move |item, _| *c.borrow_mut() = item.data::<PathBuf>().cloned());
        s.set_query("main_helper19999");
        assert!(s.search_pending());
        s.choose(gdk::ModifierType::CONTROL_MASK);
        assert!(s.is_open());
        poll(|| chosen.borrow().is_some());
        assert_eq!(
            *chosen.borrow(),
            Some("src/plan/main_helper19999.md".into())
        );
        assert!(!s.is_open());
        assert_eq!(s.rows()[0].widget, first);
    });
}
#[test]
fn test_async_cancel_does_not_deliver_stale_rows() {
    run(|| {
        let f = Fixture::new();
        let s = QuickSwitcher::new("");
        s.set_files(
            (0..20000)
                .map(|i| format!("src/plan/main_helper{i}.md").into())
                .collect(),
            vec![],
        );
        poll(|| s.index_ready());
        f.show(&s);
        s.set_query("main");
        s.set_query("main_helper19999");
        poll(|| !s.search_pending());
        assert_eq!(s.items().len(), 1);
        assert_eq!(s.items()[0].label, "main_helper19999.md");
        s.set_query("main");
        s.close();
        pump(50);
        assert!(!s.is_open());
    });
}
#[test]
fn test_root_change_reindexes_existing_snapshot_and_normalizes_paths() {
    run(|| {
        let f = Fixture::new();
        let s = QuickSwitcher::new("/r");
        s.set_files(vec!["/r/src/../plan/a.md".into()], vec![]);
        f.show(&s);
        assert_eq!(s.items()[0].detail, "plan");
        s.set_root("/r/plan");
        assert_eq!(s.items()[0].detail, "");
        assert_eq!(s.display_path(std::path::Path::new("/r/plan")), ".");
    });
}
#[test]
fn test_reentrant_provider_and_selection_callbacks() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(5);
        f.show(&p);
        let weak = p.widget().downgrade();
        let observed = Rc::new(Cell::new(0));
        let out = observed.clone();
        p.list().connect_row_selected(move |_, _| {
            if let Some(widget) = weak.upgrade() {
                widget.set_tooltip_text(Some("selected"));
                out.set(out.get() + 1);
            }
        });
        p.select(3);
        p.set_query("item");
        assert!(observed.get() > 0);
        let calls = Rc::new(Cell::new(0));
        let c = calls.clone();
        let host = f.host.clone();
        p.connect_closed(move || {
            c.set(c.get() + 1);
            host.hide_prompt();
        });
        p.close();
        assert_eq!(calls.get(), 1);
    });
}
#[test]
fn test_lazy_rows_keep_scroll_geometry_and_render_on_navigation() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(100);
        f.show(&p);
        assert!(p
            .rows()
            .iter()
            .all(|row| row.widget.allocated_height() == 32));
        assert!(p.rows()[50].widget.child().is_none());
        p.select(99);
        pump(30);
        assert_eq!(p.rows()[99].title.text(), "item 99");
        assert!(p.rows()[99].widget.child().is_some());
        assert!(p
            .rows()
            .iter()
            .all(|row| row.widget.allocated_height() == 32));
        let s = QuickSwitcher::new("");
        s.set_files(
            (0..100)
                .map(|i| format!("src/plan/main{i}.md").into())
                .collect(),
            vec![],
        );
        f.show(&s);
        let height = s.rows()[0].widget.allocated_height();
        assert!(s
            .rows()
            .iter()
            .all(|row| row.widget.allocated_height() == height));
        s.select(99);
        pump(30);
        assert!(s
            .rows()
            .iter()
            .all(|row| row.widget.allocated_height() == height));
    });
}
