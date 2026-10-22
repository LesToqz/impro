//! Real GTK note command interaction and live rendering (port of `test_slashmenu`).
mod support;
use gdk::keys::constants as k;
use gtk::prelude::*;
use impro_core::notes::{self, SLASH_COMMANDS};
use impro_editor::EditorTab;
use impro_markdown::{
    livemd,
    slashmenu::{self, SlashMenu},
};
use std::{path::PathBuf, rc::Rc};
use support::*;

struct Fx {
    h: Harness,
    tab: Rc<EditorTab>,
    path: PathBuf,
}
impl Fx {
    fn new() -> Self {
        impro_ui::motion::set_reduce_motion(true);
        let h = Harness::sized(780, 740);
        let tab = h.note("note.md", "");
        let path = tab.path();
        tab.view.grab_focus();
        pump(40);
        Self { h, tab, path }
    }
    fn menu(&self) -> SlashMenu {
        slashmenu::for_view(&self.tab.view).expect("slash menu")
    }
    fn has_menu(&self) -> bool {
        slashmenu::for_view(&self.tab.view).is_some()
    }
    fn text(&self) -> String {
        text_of(&self.tab.buffer)
    }
    fn reset(&self, text: &str) {
        self.menu().close();
        self.tab.buffer.set_text(text);
        self.tab.buffer.place_cursor(&self.tab.buffer.end_iter());
        pump(40);
    }
    fn type_text(&self, text: &str) {
        for c in text.chars() {
            self.tab.buffer.begin_user_action();
            self.tab.buffer.insert_at_cursor(&c.to_string());
            self.tab.buffer.end_user_action();
            pump(5);
        }
        pump(40);
    }
    fn key(&self, key: gdk::keys::Key) -> bool {
        emit_key(&self.tab.view, key, gdk::ModifierType::empty())
    }
}
impl Drop for Fx {
    fn drop(&mut self) {
        impro_ui::motion::set_reduce_motion(false);
    }
}

#[test]
fn test_trigger_rules() {
    run(|| {
        let fx = Fx::new();
        for (before, expected) in [
            ("", true),
            ("hello ", true),
            ("hello\n", true),
            ("a", false),
            ("`", false),
            ("`closed` ", true),
            ("```python\n", false),
            ("~~~\ncode\n", false),
            ("```\nx\n```\n", true),
            ("``code ` ", false),
        ] {
            fx.reset(before);
            fx.type_text("/");
            assert_eq!(fx.menu().opened(), expected, "{before:?}");
        }
        fx.reset("selected");
        let (a, b) = fx.tab.buffer.bounds();
        fx.tab.buffer.select_range(&a, &b);
        fx.tab.buffer.insert(&mut fx.tab.buffer.end_iter(), "/");
        pump(40);
        assert!(!fx.menu().opened());
        fx.reset("");
        fx.tab
            .view
            .emit_by_name::<()>("preedit-changed", &[&"compose"]);
        fx.type_text("/");
        assert!(!fx.menu().opened());
        fx.tab.view.emit_by_name::<()>("preedit-changed", &[&""]);
        fx.tab.buffer.insert_at_cursor("/");
        pump(40);
        assert!(!fx.menu().opened());
    });
}

fn row_text(row: &impl IsA<gtk::Widget>) -> String {
    row.clone()
        .upcast::<gtk::Widget>()
        .downcast::<gtk::Bin>()
        .ok()
        .and_then(|bin| bin.child())
        .and_then(|c| c.downcast::<gtk::Label>().ok())
        .map(|l| l.text().to_string())
        .unwrap_or_default()
}

#[test]
fn test_filter_ranking_and_group_headers() {
    run(|| {
        let fx = Fx::new();
        for (query, first) in [
            ("h1", "h1"),
            ("todo", "todo"),
            ("check", "todo"),
            ("ul", "bullet"),
            ("ol", "numbered"),
            ("hr", "divider"),
            ("code", "code"),
            ("tab", "table"),
            ("tbl", "table"),
        ] {
            fx.reset("");
            fx.type_text(&format!("/{query}"));
            assert_eq!(fx.menu().matches()[0].id, first, "{query}");
            assert_eq!(
                row_text(&fx.menu().list().children()[0]),
                "Filtered results",
                "{query}"
            );
        }
        fx.reset("");
        fx.type_text("/");
        let headings: Vec<String> = fx
            .menu()
            .list()
            .children()
            .iter()
            .filter_map(|r| r.clone().downcast::<gtk::ListBoxRow>().ok())
            .filter(|r| !r.is_selectable())
            .map(|r| row_text(&r))
            .collect();
        assert_eq!(headings, ["Basic blocks", "Insert"]);
        assert_eq!(fx.menu().matches().len(), 16);
        assert!(fx.menu().scroll().allocated_height() <= 274);
        assert!(fx.tab.view.has_focus());
    });
}

#[test]
fn test_navigation_wrap_swallowed_and_focus() {
    run(|| {
        let fx = Fx::new();
        fx.type_text("/");
        assert!(fx.key(k::Up));
        assert_eq!(fx.menu().index(), 15);
        assert!(fx.key(k::Down));
        assert_eq!(fx.menu().index(), 0);
        fx.key(k::End);
        assert_eq!(fx.menu().index(), 15);
        fx.key(k::Home);
        assert_eq!(fx.menu().index(), 0);
        assert_eq!(fx.text(), "/");
        assert!(fx.tab.view.has_focus());
        fx.key(k::Escape);
        assert!(!fx.menu().opened());
        assert_eq!(fx.text(), "/");
        fx.key(k::Return);
        assert_eq!(fx.text(), "/\n");
        fx.key(k::Tab);
        assert_eq!(fx.text(), "/\n    ");
    });
}

#[test]
fn test_closing_backspace_arrows_space_focus_click_scroll() {
    run(|| {
        let fx = Fx::new();
        fx.type_text("/h1");
        fx.key(k::Left);
        assert!(fx.menu().opened());
        fx.key(k::Left);
        assert!(fx.menu().opened());
        fx.key(k::Left);
        assert!(!fx.menu().opened());
        fx.reset("");
        fx.type_text("/h1");
        fx.key(k::Right);
        // At document end Right cannot move beyond the query.
        assert!(fx.menu().opened());
        fx.reset("tail");
        fx.tab.buffer.place_cursor(&fx.tab.buffer.start_iter());
        fx.type_text("/h1");
        fx.key(k::Right);
        assert!(!fx.menu().opened());
        fx.reset("");
        fx.type_text("/");
        fx.key(k::BackSpace);
        assert!(!fx.menu().opened());
        assert_eq!(fx.text(), "");
        fx.type_text("/zzzz");
        assert!(fx.menu().matches().is_empty());
        fx.key(k::Return);
        assert_eq!(fx.text(), "/zzzz");
        fx.type_text(" ");
        assert!(!fx.menu().opened());
        for (signal, kind) in [
            ("focus-out-event", gdk::EventType::FocusChange),
            ("button-press-event", gdk::EventType::ButtonPress),
            ("scroll-event", gdk::EventType::Scroll),
        ] {
            fx.reset("");
            fx.type_text("/");
            let _: bool = fx.tab.view.emit_by_name(signal, &[&bare_event(kind)]);
            assert!(!fx.menu().opened(), "{signal}");
        }
    });
}

#[test]
fn test_enter_tab_mouse_and_two_undo_actions() {
    run(|| {
        let fx = Fx::new();
        for key in [Some(k::Return), Some(k::Tab), None] {
            fx.reset("");
            fx.type_text("/h1");
            match key {
                Some(key) => {
                    fx.key(key);
                }
                None => {
                    let row = fx.menu().rows()[0].clone();
                    let _: bool = row.emit_by_name(
                        "enter-notify-event",
                        &[&bare_event(gdk::EventType::EnterNotify)],
                    );
                    let a = row.allocation();
                    let window = fx.menu().list().window().unwrap();
                    for kind in [gdk::EventType::ButtonPress, gdk::EventType::ButtonRelease] {
                        send_button(
                            kind,
                            &window,
                            f64::from(a.x() + 10),
                            f64::from(a.y() + 10),
                            1,
                        );
                    }
                    pump(40);
                }
            }
            assert_eq!(fx.text(), "# ");
            assert!(!fx.menu().opened());
            assert!(fx.tab.view.has_focus());
            fx.tab.undo(false);
            assert_eq!(fx.text(), "/h1");
            fx.tab.undo(false);
            assert_eq!(fx.text(), "");
        }
    });
}

#[test]
fn test_every_command_is_one_undo_and_keeps_text() {
    run(|| {
        let fx = Fx::new();
        for command in SLASH_COMMANDS.iter() {
            for before in ["", "## keep "] {
                fx.reset(before);
                let query = format!("/{}", command.id);
                fx.type_text(&query);
                let len = before.chars().count() as isize;
                let expected = command.edit(before, len, Some(len)).unwrap();
                fx.menu().execute(Some(command));
                assert_eq!(
                    fx.text(),
                    expected.apply(before),
                    "{} {before:?}",
                    command.id
                );
                let actual = fx
                    .tab
                    .buffer
                    .selection_bounds()
                    .map(|(a, b)| (a.offset() as usize, b.offset() as usize))
                    .unwrap_or((fx.tab.cursor() as usize, fx.tab.cursor() as usize));
                assert_eq!(
                    actual,
                    (expected.select_start, expected.select_end),
                    "{}",
                    command.id
                );
                fx.tab.undo(false);
                assert_eq!(fx.text(), format!("{before}{query}"), "{}", command.id);
                fx.tab.undo(false);
                assert_eq!(fx.text(), before, "{}", command.id);
            }
        }
    });
}

#[test]
fn test_live_blocks_render_and_checkbox_works() {
    run(|| {
        let fx = Fx::new();
        for (query, kind) in [("table", "table"), ("code", "codeblock"), ("todo", "line")] {
            fx.reset("");
            fx.type_text(&format!("/{query}"));
            fx.key(k::Return);
            fx.tab.buffer.insert_at_cursor("\n\nend");
            fx.tab.buffer.place_cursor(&fx.tab.buffer.end_iter());
            pump(160);
            let live = livemd::for_view(&fx.tab.view).unwrap();
            settle(&live);
            let blocks: Vec<_> = live
                .blocks()
                .into_iter()
                .filter(|b| b.kind == kind)
                .collect();
            assert!(!blocks.is_empty(), "{query}");
            let widgets: Vec<gtk::Widget> = blocks
                .iter()
                .flat_map(|b| b.widgets.iter().map(|w| w.0.clone()))
                .collect();
            assert!(!widgets.is_empty(), "{query}");
            if query == "todo" {
                let check = widgets
                    .iter()
                    .find_map(|w| w.clone().downcast::<gtk::CheckButton>().ok())
                    .expect("task box");
                check.set_active(true);
                assert!(fx.text().contains("[x]"));
            }
            if query == "code" {
                assert!(widgets.iter().any(|w| w
                    .clone()
                    .downcast::<gtk::Container>()
                    .is_ok_and(|c| c.children().iter().any(|child| child.is::<gtk::Button>()))));
            }
        }
    });
}

#[test]
fn test_fade_honours_motion_preference() {
    run(|| {
        let fx = Fx::new();
        fx.type_text("/");
        assert_eq!(fx.menu().popover().opacity(), 1.0);
        fx.reset("");
        impro_ui::motion::set_reduce_motion(false);
        fx.type_text("/");
        assert!(fx.menu().popover().opacity() < 1.0);
        pump(160);
        assert_eq!(fx.menu().popover().opacity(), 1.0);
    });
}

#[test]
fn test_caret_scroll_at_bottom_and_reduce_motion() {
    run(|| {
        let fx = Fx::new();
        fx.reset(&"line\n".repeat(80));
        fx.tab
            .view
            .scroll_mark_onscreen(&fx.tab.buffer.get_insert().unwrap());
        pump(40);
        fx.type_text("/");
        assert!(fx.menu().opened());
        assert_eq!(fx.menu().popover().opacity(), 1.0);
        fx.tab.source_scroll.vadjustment().set_value(0.0);
        pump(300);
        assert!(!fx.menu().opened());
    });
}

#[test]
fn test_selection_replacement_and_non_note_source_disabled_typing() {
    run(|| {
        let fx = Fx::new();
        fx.reset("selected");
        let (a, b) = fx.tab.buffer.bounds();
        fx.tab.buffer.select_range(&a, &b);
        fx.key(k::slash);
        fx.tab.buffer.delete_selection(true, true);
        fx.type_text("/");
        assert!(!fx.menu().opened());
        let other = fx.h.path("other.md");
        for mode in ["source", "disabled", "non-note"] {
            if fx.has_menu() {
                fx.reset("");
            }
            match mode {
                "source" => {
                    fx.h.panel.set_markdown_mode("source", None).unwrap();
                }
                "disabled" => fx.h.panel.set_slash_commands(false),
                _ => fx.h.panel.on_path_renamed(&fx.path, &other),
            }
            assert!(!fx.has_menu(), "{mode}");
            fx.tab.buffer.insert_at_cursor("/code");
            pump(40);
            assert!(!fx.has_menu(), "{mode}");
            fx.h.panel.set_slash_commands(true);
            if mode == "non-note" {
                fx.h.panel.on_path_renamed(&other, &fx.path);
            }
            fx.h.panel.set_markdown_mode("preview", None).unwrap();
            assert!(fx.has_menu(), "{mode}");
        }
    });
}

#[test]
fn test_theme_font_and_fallback() {
    run(|| {
        let fx = Fx::new();
        fx.menu().set_icons_enabled(false);
        fx.type_text("/code");
        let first = fx.menu().rows()[0]
            .child()
            .unwrap()
            .downcast::<gtk::Box>()
            .unwrap()
            .children()[0]
            .clone()
            .downcast::<gtk::Box>()
            .unwrap()
            .children()[0]
            .clone();
        let label = first.downcast::<gtk::Label>().expect("glyph label");
        assert_eq!(label.text(), "</>");
        impro_ui::apply_theme(
            impro_core::themes::get_theme(Some("impro-dark")),
            None,
            Some("DejaVu Serif"),
            Some("DejaVu Sans Mono"),
        )
        .unwrap();
        pump(40);
        let title = fx.menu().rows()[0]
            .child()
            .unwrap()
            .downcast::<gtk::Box>()
            .unwrap()
            .children()[1]
            .clone();
        #[allow(deprecated)]
        let font = title.style_context().font(gtk::StateFlags::NORMAL);
        assert_eq!(font.family().unwrap(), "DejaVu Serif");
        let _ = impro_ui::apply_style(None);
    });
}

#[test]
fn test_note_mode_disable_rename_and_destroy_lifecycle() {
    run(|| {
        let fx = Fx::new();
        fx.type_text("/");
        let old = fx.menu();
        fx.h.panel.set_slash_commands(false);
        assert!(!fx.has_menu());
        assert!(old.is_destroyed());
        assert_eq!(old.handler_count(), 0);
        fx.h.panel.set_slash_commands(true);
        assert!(fx.has_menu());
        fx.h.panel.set_markdown_mode("source", None).unwrap();
        assert!(!fx.has_menu());
        fx.h.panel.set_markdown_mode("preview", None).unwrap();
        assert!(fx.has_menu());
        let other = fx.h.path("other.md");
        fx.h.panel.on_path_renamed(&fx.path, &other);
        assert!(!fx.has_menu());
        fx.h.panel.on_path_renamed(&other, &fx.path);
        assert!(fx.has_menu());
        let old = fx.menu();
        fx.tab.buffer.set_modified(false);
        let done = Rc::new(std::cell::Cell::new(false));
        let flag = done.clone();
        fx.h.panel.close_tab(&fx.tab, move |_| flag.set(true));
        poll(|| done.get());
        assert!(old.is_destroyed());
    });
}

#[test]
fn test_screenshots() {
    run(|| {
        let fx = Fx::new();
        let folder = std::env::var("IMPRO_QA_SHOTS").ok().map(PathBuf::from);
        for theme_id in ["impro-dark", "impro-light"] {
            let theme = impro_core::themes::get_theme(Some(theme_id));
            impro_ui::apply_theme(theme, None, None, None).unwrap();
            fx.h.panel.set_syntax_style(&theme.syntax);
            fx.h.panel.set_chrome(&theme.chrome);
            for query in ["", "code"] {
                fx.reset("# Field notes\n\nA place for ideas and small experiments.\n\n- [x] Collect ideas\n- [ ] Try a new block\n\n");
                fx.type_text(&format!("/{query}"));
                pump(150);
                if let Some(folder) = &folder {
                    let window = fx.h.window.window().unwrap();
                    let pixbuf = window.pixbuf(0, 0, 780, 740).unwrap();
                    let name = format!(
                        "{theme_id}-{}.png",
                        if query.is_empty() { "all" } else { query }
                    );
                    pixbuf.savev(folder.join(name), "png", &[]).unwrap();
                }
                assert!(fx.menu().opened());
            }
        }
        let _ = impro_ui::apply_style(None);
        let _ = notes::SLASH_COMMANDS.len();
    });
}
