//! Block transforms driven by real key, mouse and clipboard events in the live
//! editor (the `BlockKeysTest` half of the original `test_blockedit` suite).
mod support;
use gdk::keys::constants as k;
use gtk::prelude::*;
use impro_editor::EditorTab;
use impro_markdown::{
    blockedit,
    livemd::{self, LiveMarkdown},
};
use std::rc::Rc;
use support::*;

struct Fx {
    h: Harness,
    tab: Rc<EditorTab>,
    live: LiveMarkdown,
}
impl Fx {
    fn new() -> Self {
        impro_ui::motion::set_reduce_motion(true);
        let h = Harness::rooted(850, 720);
        let tab = h.note("blocks.md", "");
        tab.view.grab_focus();
        let live = live_of(&tab);
        let fx = Self { h, tab, live };
        fx.ready();
        fx
    }
    fn ready(&self) {
        settle(&self.live);
        pump(10);
        settle(&self.live);
    }
    fn text(&self) -> String {
        text_of(&self.tab.buffer)
    }
    fn reset(&self, text: &str, cursor: Option<i32>) {
        self.tab.buffer.set_text(text);
        let at = cursor.unwrap_or(text.chars().count() as i32);
        self.tab
            .buffer
            .place_cursor(&self.tab.buffer.iter_at_offset(at));
        self.ready();
    }
    fn key(&self, key: gdk::keys::Key, state: gdk::ModifierType) -> bool {
        let handled = emit_key(&self.tab.view, key, state);
        self.ready();
        handled
    }
    fn press(&self, key: gdk::keys::Key) -> bool {
        self.key(key, gdk::ModifierType::empty())
    }
    fn type_text(&self, text: &str) {
        for c in text.chars() {
            self.key(gdk::keys::Key::from_unicode(c), gdk::ModifierType::empty());
        }
    }
    fn hidden(&self, offset: i32) -> bool {
        self.tab
            .buffer
            .iter_at_offset(offset)
            .has_tag(&self.live.tag("hidden").unwrap())
    }
    fn widgets(&self, role: &str) -> Vec<gtk::Widget> {
        self.live.widgets(role)
    }
    fn index_of(&self, needle: &str) -> i32 {
        let text = self.text();
        text[..text.find(needle).unwrap()].chars().count() as i32
    }
}
impl Drop for Fx {
    fn drop(&mut self) {
        impro_ui::motion::set_reduce_motion(false);
    }
}
fn ctrl() -> gdk::ModifierType {
    gdk::ModifierType::CONTROL_MASK
}
fn shift() -> gdk::ModifierType {
    gdk::ModifierType::SHIFT_MASK
}
fn selected(fx: &Fx) -> String {
    let (a, b) = fx.tab.buffer.selection_bounds().expect("selection");
    fx.tab.buffer.text(&a, &b, true).unwrap().to_string()
}

#[test]
fn test_typing_live_shortcuts_and_save() {
    run(|| {
        let fx = Fx::new();
        fx.type_text("# Title");
        assert_eq!(fx.text(), "# Title");
        assert!(fx.hidden(0));
        fx.press(k::Return);
        fx.type_text("- one");
        fx.press(k::Return);
        fx.type_text("two");
        fx.press(k::Return);
        fx.press(k::Return);
        fx.type_text("text");
        assert_eq!(fx.text(), "# Title\n- one\n- two\ntext");
        let saved = Rc::new(std::cell::Cell::new(None));
        let flag = saved.clone();
        fx.h.panel.save_current(move |ok| flag.set(Some(ok)));
        poll(|| saved.get().is_some());
        assert_eq!(saved.get(), Some(true));
        assert_eq!(std::fs::read_to_string(fx.tab.path()).unwrap(), fx.text());
        for shortcut in ["## ", "### ", "* ", "1. ", "> ", "- [ ] "] {
            fx.reset("", None);
            fx.type_text(&format!("{shortcut}item"));
            assert!(fx.hidden(0), "{shortcut}");
        }
        fx.reset("", None);
        fx.type_text("[] ");
        assert_eq!(fx.text(), "- [ ] ");
    });
}

#[test]
fn test_active_prefixes_and_inline_span_visibility() {
    run(|| {
        let fx = Fx::new();
        for (source, position) in [
            ("# Heading", 4),
            ("- item", 4),
            ("  - item", 6),
            ("2. item", 5),
            ("- [x] item", 8),
            ("> quote", 4),
        ] {
            fx.reset(source, Some(position));
            assert!(fx.hidden(0), "{source}");
            fx.tab.buffer.place_cursor(&fx.tab.buffer.start_iter());
            assert!(fx.tab.cursor() > 0, "{source}");
            assert!(!fx.tab.buffer.has_selection(), "{source}");
            if source.contains("[x]") {
                assert!(fx
                    .tab
                    .buffer
                    .iter_at_offset(7)
                    .has_tag(&fx.live.tag("checked").unwrap()));
            }
        }
        let source = "- **one** and *two*";
        fx.reset(source, Some(fx.index_of_in(source, "one") + 1));
        assert!(!fx.hidden(fx.index_of("**")));
        assert!(fx.hidden(fx.index_of("*two")));
        assert!(fx.hidden(0));
        fx.press(k::End);
        assert!(fx.hidden(fx.index_of("**")));
        fx.reset("- [x] `finished`", Some(10));
        let tags = fx.tab.buffer.iter_at_offset(10).tags();
        let foreground: Vec<_> = tags
            .iter()
            .filter(|t| t.property::<bool>("foreground-set"))
            .collect();
        assert_eq!(
            foreground.last().unwrap(),
            &&fx.live.tag("checked").unwrap()
        );
    });
}
impl Fx {
    fn index_of_in(&self, source: &str, needle: &str) -> i32 {
        source[..source.find(needle).unwrap()].chars().count() as i32
    }
}

#[test]
fn test_home_left_selection_copy_click_and_undo_clamp() {
    run(|| {
        let fx = Fx::new();
        fx.reset("previous\n- content\n- next", Some(15));
        fx.key(k::Home, shift());
        assert_eq!(fx.tab.cursor(), 11);
        assert_eq!(selected(&fx), "cont");
        fx.press(k::Home);
        fx.press(k::Left);
        assert_eq!(fx.tab.cursor(), 8);
        fx.tab
            .buffer
            .place_cursor(&fx.tab.buffer.iter_at_offset(15));
        let rect = fx.tab.view.iter_location(&fx.tab.buffer.iter_at_offset(11));
        let (x, y) = fx.tab.view.buffer_to_window_coords(
            gtk::TextWindowType::Text,
            rect.x() - 18,
            rect.y() + 3,
        );
        let window = TextViewExt::window(&fx.tab.view, gtk::TextWindowType::Text).unwrap();
        let press = button_event_at(
            gdk::EventType::ButtonPress,
            &window,
            f64::from(x),
            f64::from(y),
            1,
        );
        let _: bool = fx.tab.view.emit_by_name("button-press-event", &[&press]);
        fx.ready();
        assert_eq!(fx.tab.cursor(), 11);
        let (a, b) = fx.tab.buffer.bounds();
        fx.tab.buffer.select_range(&b, &a);
        fx.key(k::c, ctrl());
        let clipboard = gtk::Clipboard::default(&gdk::Display::default().unwrap()).unwrap();
        assert_eq!(clipboard.wait_for_text().unwrap(), fx.text());
        fx.tab
            .buffer
            .place_cursor(&fx.tab.buffer.iter_at_offset(11));
        fx.press(k::BackSpace);
        assert_eq!(fx.text(), "previous\ncontent\n- next");
        fx.key(k::z, ctrl());
        assert_eq!(fx.text(), "previous\n- content\n- next");
        assert_eq!(fx.tab.cursor(), 11);
        fx.key(k::z, ctrl() | shift());
        assert_eq!(fx.text(), "previous\ncontent\n- next");
    });
}

#[test]
fn test_list_keys_children_renumber_and_one_step_undo() {
    run(|| {
        let fx = Fx::new();
        let _ = fx.h.panel.set_indent(false, 2);
        fx.reset("1. one\n2. two\n3. three", Some(6));
        let (before, history) = (fx.text(), fx.tab.history_len());
        fx.press(k::Return);
        assert_eq!(fx.text(), "1. one\n2. \n3. two\n4. three");
        assert_eq!(fx.tab.history_len(), history + 1);
        fx.key(k::z, ctrl());
        assert_eq!(fx.text(), before);
        fx.reset("- parent\n  - child\n- sibling", Some(5));
        fx.press(k::Tab);
        assert_eq!(fx.text(), "  - parent\n    - child\n- sibling");
        fx.press(k::ISO_Left_Tab);
        assert_eq!(fx.text(), "- parent\n  - child\n- sibling");
        fx.reset("- one\n- two\n", None);
        let (a, b) = fx.tab.buffer.bounds();
        fx.tab.buffer.select_range(&a, &b);
        assert!(fx.tab.buffer.has_selection());
        fx.press(k::Tab);
        assert_eq!(fx.text(), "  - one\n  - two\n");
        fx.press(k::ISO_Left_Tab);
        assert_eq!(fx.text(), "- one\n- two\n");
        fx.reset("  - ", Some(4));
        fx.press(k::Return);
        assert_eq!(fx.text(), "- ");
        fx.press(k::Return);
        assert_eq!(fx.text(), "");
        let _ = fx.h.panel.set_indent(false, 4);
        fx.reset("- root\n  - parent\n    - ", Some(24));
        fx.press(k::Return);
        assert_eq!(fx.text(), "- root\n  - parent\n  - ");
        fx.reset("- [x] done", Some(9));
        fx.key(k::Return, ctrl());
        assert_eq!(fx.text(), "- [ ] done");
        let boxes = fx.widgets("marker");
        boxes[0]
            .clone()
            .downcast::<gtk::CheckButton>()
            .unwrap()
            .clicked();
        fx.ready();
        assert_eq!(fx.text(), "- [x] done");
    });
}

#[test]
fn test_split_demote_quote_exit_and_numbering_after_interruption() {
    run(|| {
        let fx = Fx::new();
        fx.reset("- first second", Some(7));
        fx.press(k::Return);
        assert_eq!(fx.text(), "- first\n-  second");
        fx.reset("## Title", Some(3));
        fx.press(k::BackSpace);
        assert_eq!(fx.text(), "Title");
        fx.reset("> quote", None);
        fx.press(k::Return);
        assert_eq!(fx.text(), "> quote\n> ");
        assert!(fx.hidden(8));
        fx.press(k::Return);
        assert_eq!(fx.text(), "> quote\n");
        fx.reset("> quote", Some(2));
        fx.press(k::BackSpace);
        assert_eq!(fx.text(), "quote");
        fx.reset("1. first\n1. second\n\n1. restart", None);
        let labels: Vec<String> = fx
            .widgets("marker")
            .iter()
            .filter_map(|w| w.clone().downcast::<gtk::Label>().ok())
            .map(|l| l.text().to_string())
            .collect();
        assert_eq!(labels, ["1.", "2.", "1."]);
        assert_eq!(fx.text(), "1. first\n1. second\n\n1. restart");
        fx.reset("1. one\n2. \n3. three", Some(10));
        fx.press(k::Return);
        assert_eq!(fx.text(), "1. one\n\n1. three");
    });
}

#[test]
fn test_code_keys_language_picker_copy_and_exit() {
    run(|| {
        let fx = Fx::new();
        fx.reset("before\n\n```python\n    value\n```\n\nafter", Some(27));
        fx.press(k::Return);
        assert!(fx.text().contains("    value\n    \n```"));
        fx.press(k::Tab);
        fx.press(k::ISO_Left_Tab);
        assert!(fx.hidden(fx.index_of("```")));
        let tools = fx.widgets("tools")[0].clone();
        livemd::tools_copy_button(&tools).unwrap().clicked();
        let clipboard = gtk::Clipboard::default(&gdk::Display::default().unwrap()).unwrap();
        assert_eq!(clipboard.wait_for_text().unwrap(), "    value\n    \n");
        let click = button_event_at(
            gdk::EventType::ButtonPress,
            &TextViewExt::window(&fx.tab.view, gtk::TextWindowType::Widget).unwrap(),
            0.0,
            0.0,
            1,
        );
        let _: bool = livemd::tools_language_button(&tools)
            .unwrap()
            .emit_by_name("button-press-event", &[&click]);
        pump(40);
        let popover = fx.live.language_popover().expect("language popover");
        assert!(!popover.property::<bool>("transitions-enabled"));
        let container = popover.child().unwrap().downcast::<gtk::Box>().unwrap();
        let children = container.children();
        let search = children[0].clone().downcast::<gtk::SearchEntry>().unwrap();
        let scroll = children[1]
            .clone()
            .downcast::<gtk::ScrolledWindow>()
            .unwrap();
        search.set_text("json");
        pump(200);
        let viewport = scroll.child().unwrap().downcast::<gtk::Bin>().unwrap();
        let choices = viewport
            .child()
            .unwrap()
            .downcast::<gtk::ListBox>()
            .unwrap();
        let row = choices
            .children()
            .into_iter()
            .filter_map(|c| c.downcast::<gtk::ListBoxRow>().ok())
            .find(|r| unsafe {
                r.data::<String>("language")
                    .is_some_and(|l| l.as_ref() == "json")
            })
            .expect("json row");
        let history = fx.tab.history_len();
        choices.emit_by_name::<()>("row-activated", &[&row]);
        fx.ready();
        assert!(fx.text().contains("```json\n"));
        assert_eq!(fx.tab.history_len(), history + 1);
        fx.key(k::Return, ctrl());
        let text = fx.text();
        let second = text[text[10..].find("```").unwrap() + 10..].len();
        let _ = second;
        let fence = text[..10].len() + text[10..].find("```").unwrap();
        assert!(fx.tab.cursor() >= text[..fence].chars().count() as i32 + 3);
        fx.reset("```\nx\n```", Some(5));
        fx.press(k::Down);
        assert_eq!(fx.text(), "```\nx\n```\n");
        assert_eq!(fx.tab.cursor(), fx.text().chars().count() as i32);
        fx.reset("```\n\n```", Some(4));
        fx.press(k::BackSpace);
        assert_eq!(fx.text(), "");
        fx.reset("```\n```", Some(1));
        fx.type_text("x");
        assert_eq!(fx.text(), "```\nx\n```");
        fx.reset("```\n\n```", Some(4));
        fx.press(k::Return);
        fx.press(k::Return);
        assert_eq!(fx.tab.cursor(), fx.text().chars().count() as i32);
        assert!(fx.text().ends_with("```\n"));
        fx.reset("before\n```\n# comment\n```\nafter", Some(15));
        fx.press(k::Home);
        assert_eq!(fx.tab.cursor(), 11);
        fx.press(k::Up);
        assert_eq!(fx.tab.cursor(), 6);
        fx.tab
            .buffer
            .place_cursor(&fx.tab.buffer.iter_at_offset(14));
        fx.press(k::Down);
        assert_eq!(fx.tab.cursor(), fx.index_of("after"));
        fx.reset("", None);
        fx.type_text("```python");
        fx.press(k::Return);
        assert_eq!(fx.text(), "```python\n\n```");
        fx.type_text("x");
        fx.press(k::Escape);
        assert_eq!(fx.text(), "```python\nx\n```\n");
    });
}

#[test]
fn test_fence_caret_positions_stay_in_code_content() {
    run(|| {
        let fx = Fx::new();
        let source = "```python\nvalue\n```";
        fx.reset(source, None);
        for offset in [0, 3, 9, 16, 18, source.chars().count() as i32] {
            fx.tab
                .buffer
                .place_cursor(&fx.tab.buffer.iter_at_offset(offset));
            fx.ready();
            assert!(!fx.hidden(fx.tab.cursor()), "{offset}");
            assert!(!fx.tab.buffer.has_selection(), "{offset}");
            assert!(fx.tab.cursor() >= fx.index_of("value"));
            assert!(fx.tab.cursor() <= fx.index_of("\n```"));
        }
        fx.tab
            .buffer
            .place_cursor(&fx.tab.buffer.iter_at_offset(fx.index_of("value")));
        fx.press(k::BackSpace);
        assert_eq!(fx.text(), source);
        fx.tab
            .buffer
            .place_cursor(&fx.tab.buffer.iter_at_offset(fx.index_of("\n```")));
        fx.press(k::Delete);
        assert_eq!(fx.text(), source);
    });
}

#[test]
fn test_divider_and_multiline_paste() {
    run(|| {
        let fx = Fx::new();
        fx.type_text("---");
        fx.press(k::Return);
        assert_eq!(fx.widgets("rule").len(), 1);
        fx.press(k::BackSpace);
        assert_eq!(fx.text(), "");
        let clipboard = gtk::Clipboard::default(&gdk::Display::default().unwrap()).unwrap();
        let markdown = "- first\n  - child\n- [ ] task\n\n1. number\n2. next";
        clipboard.set_text(markdown);
        fx.key(k::v, ctrl());
        fx.ready();
        assert_eq!(fx.text(), markdown);
        for block in fx.live.blocks() {
            if blockedit::LIST.is_match(&block.text) {
                assert!(fx.hidden(block.start), "{}", block.text);
            }
        }
        fx.key(k::z, ctrl());
        assert_eq!(fx.text(), "");
        fx.key(k::z, ctrl() | shift());
        assert_eq!(fx.text(), markdown);
    });
}
