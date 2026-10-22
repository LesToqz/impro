//! Live decoration preserves source, editing history and a responsive GTK loop.
//! Port of the original `test_livemd` suite; test names mirror it.
mod support;
use gdk::keys::constants as k;
use gtk::prelude::*;
use impro_editor::EditorTab;
use impro_markdown::livemd::{self, LiveMarkdown};
use std::{cell::RefCell, path::PathBuf, rc::Rc};
use support::*;

struct Fx {
    h: Harness,
    tab: Rc<EditorTab>,
    live: LiveMarkdown,
    path: PathBuf,
}
impl Fx {
    fn new(text: &str) -> Self {
        let h = Harness::new();
        let path = h.path("note.md");
        std::fs::write(&path, text).unwrap();
        let tab = h.open_path(&path);
        let live = live_of(&tab);
        let fx = Self { h, tab, live, path };
        fx.at(text.chars().count() as i32);
        fx.ready();
        fx
    }
    fn open(&mut self, text: &str, name: &str) {
        let path = self.h.path(name);
        std::fs::write(&path, text).unwrap();
        self.tab = self.h.open_path(&path);
        self.live = live_of(&self.tab);
        self.path = path;
        self.at(text.chars().count() as i32);
        self.ready();
    }
    fn ready(&self) {
        settle(&self.live);
        pump(5);
        settle(&self.live);
    }
    fn at(&self, offset: i32) {
        self.tab
            .buffer
            .place_cursor(&self.tab.buffer.iter_at_offset(offset));
    }
    fn text(&self) -> String {
        text_of(&self.tab.buffer)
    }
    fn hidden(&self, offset: i32) -> bool {
        self.tab
            .buffer
            .iter_at_offset(offset)
            .has_tag(&self.live.tag("hidden").unwrap())
    }
    fn key(&self, key: gdk::keys::Key) -> bool {
        panel_key(&self.h.panel, key, gdk::ModifierType::empty())
    }
    fn widgets(&self, role: &str) -> Vec<gtk::Widget> {
        self.live.widgets(role)
    }
    fn index_of(&self, needle: &str) -> i32 {
        let text = self.text();
        text.find(needle)
            .map(|b| text[..b].chars().count() as i32)
            .unwrap_or_else(|| panic!("{needle:?} not in document"))
    }
    /// Pixels between the text rows that start at each marker, in source order.
    fn gaps(&self, marks: &[&str]) -> Vec<i32> {
        let rows: Vec<(i32, i32)> = marks
            .iter()
            .map(|m| {
                let rect = self
                    .tab
                    .view
                    .iter_location(&self.tab.buffer.iter_at_offset(self.index_of(m)));
                (rect.y(), rect.y() + rect.height())
            })
            .collect();
        (0..rows.len() - 1)
            .map(|i| rows[i + 1].0 - rows[i].1)
            .collect()
    }
}
fn char_len(text: &str) -> i32 {
    text.chars().count() as i32
}

#[test]
fn test_markers_follow_caret_without_source_changes() {
    run(|| {
        let source = "# Heading\n\n**bold** *italic* ***both*** ~~gone~~ `code` [web](https://example.org)\n\n[[note|alias]] https://example.org\n\nend";
        let fx = Fx::new(source);
        let at = |needle: &str| fx.index_of(needle);
        let markers = [
            0,
            at("**"),
            at("*italic"),
            at("***both"),
            at("~~"),
            at("`"),
            at("[web]"),
            at("[[note"),
        ];
        for marker in markers {
            fx.at(char_len(source));
            fx.ready();
            assert!(fx.hidden(marker), "{marker}");
            fx.at(marker + 1);
            fx.ready();
            assert_eq!(fx.hidden(marker), marker == 0, "{marker}");
            assert_eq!(fx.text(), source);
            assert_eq!(fx.tab.buffer.char_count(), char_len(source));
            let (a, b) = fx.tab.buffer.bounds();
            assert_eq!(fx.tab.buffer.slice(&a, &b, true).unwrap(), source);
            assert!(!fx.tab.buffer.is_modified());
        }
        for (name, word) in [
            ("strong", "bold"),
            ("emphasis", "italic"),
            ("strike", "gone"),
            ("code", "code"),
            ("link", "alias"),
        ] {
            let iter = fx.tab.buffer.iter_at_offset(at(word));
            assert!(iter.has_tag(&fx.live.tag(name).unwrap()), "{name}");
        }
    });
}

#[test]
fn test_placeholder_tracks_empty_caret_line_and_mode_without_source_edits() {
    run(|| {
        let source = "# Title\n\ntext\n\n```\n\n```\n";
        let mut fx = Fx::new(source);
        fx.at(fx.index_of("\n\n") + 1);
        fx.ready();
        fx.live.position();
        let placeholder = fx.live.placeholder().expect("placeholder");
        assert_eq!(placeholder.text(), "Type '/' for commands");
        assert!(placeholder.is_visible());
        let caret = fx
            .tab
            .buffer
            .iter_at_mark(&fx.tab.buffer.get_insert().unwrap());
        assert!(!caret.has_tag(&fx.live.tag("blank").unwrap()));
        fx.at(fx.index_of("text") + 1);
        fx.ready();
        fx.live.position();
        assert!(!placeholder.is_visible());
        fx.at(fx.index_of("```") + 4);
        fx.ready();
        fx.live.position();
        assert!(!placeholder.is_visible());
        assert_eq!(fx.text(), source);
        assert!(!fx.tab.buffer.is_modified());
        fx.tab.set_markdown_mode("source");
        assert!(fx.live.placeholder().is_none());
        assert!(fx.live.detail_labels().is_empty());
        fx.open("", "empty.md");
        fx.live.position();
        assert!(fx.live.placeholder().expect("placeholder").is_visible());
        assert_eq!(fx.text(), "");
    });
}

#[test]
fn test_code_copy_reveals_on_card_hover_and_hides_when_pointer_leaves() {
    run(|| {
        let fx = Fx::new("```python\nprint(1)\n```\n\nend");
        impro_ui::motion::set_reduce_motion(true);
        let tools = fx.widgets("tools")[0].clone();
        let button = livemd::tools_copy_button(&tools).expect("copy button");
        assert_eq!(button.opacity(), 0.0);
        let rect = fx.tab.view.iter_location(&fx.tab.buffer.iter_at_offset(11));
        let (x, y) =
            fx.tab
                .view
                .buffer_to_window_coords(gtk::TextWindowType::Widget, rect.x(), rect.y());
        fx.live.pointer_moved(f64::from(x), f64::from(y));
        assert_eq!(button.opacity(), 1.0);
        fx.live.pointer_left(false);
        assert_eq!(button.opacity(), 0.0);
        assert_eq!(fx.widgets("code-card").len(), 1);
        impro_ui::motion::set_reduce_motion(false);
    });
}

struct Lcg(u64);
impl Lcg {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }
}

#[test]
fn test_scripted_edits_always_equal_expected_source() {
    run(|| {
        let mut expected: Vec<char> =
            "# Title\n\n- [ ] todo\n\n```python\nprint(1)\n```\n\n| A | B |\n| --- | --- |\n| x | y |\n\nend"
                .chars()
                .collect();
        let fx = Fx::new(&expected.iter().collect::<String>());
        let mut random = Lcg(917);
        let pieces = ["**x**", "\n", "# ", "|", "ø", "", "`"];
        for index in 0..45 {
            let a = random.below(expected.len() + 1);
            let b = (a + random.below(5)).min(expected.len());
            let replacement = pieces[random.below(pieces.len())];
            let buffer = &fx.tab.buffer;
            buffer.begin_user_action();
            let (mut first, mut last) = (
                buffer.iter_at_offset(a as i32),
                buffer.iter_at_offset(b as i32),
            );
            buffer.delete(&mut first, &mut last);
            let mut at = buffer.iter_at_offset(a as i32);
            buffer.insert(&mut at, replacement);
            buffer.end_user_action();
            expected.splice(a..b, replacement.chars());
            fx.at(random.below(expected.len() + 1) as i32);
            fx.ready();
            let want: String = expected.iter().collect();
            assert_eq!(fx.text(), want, "{index}");
            assert_eq!(buffer.char_count() as usize, expected.len(), "{index}");
            let joined: String = fx.live.blocks().iter().map(|b| b.text.clone()).collect();
            assert_eq!(joined, want, "{index}");
            let blocks = fx.live.blocks();
            assert!(
                blocks
                    .iter()
                    .zip(blocks.iter().skip(1))
                    .all(|(x, y)| x.end == y.start),
                "{index}"
            );
        }
        let saved = Rc::new(RefCell::new(None));
        let flag = saved.clone();
        fx.h.panel
            .save_current(move |ok| *flag.borrow_mut() = Some(ok));
        poll(|| saved.borrow().is_some());
        assert_eq!(*saved.borrow(), Some(true));
        assert_eq!(
            std::fs::read_to_string(&fx.path).unwrap(),
            expected.iter().collect::<String>()
        );
    });
}

#[test]
fn test_checkbox_is_one_undoable_edit() {
    run(|| {
        let source = "- [ ] **todo**\n- [x] done\n\nend";
        let fx = Fx::new(source);
        let checks = fx.widgets("marker");
        let boxes: Vec<gtk::CheckButton> = checks
            .iter()
            .filter_map(|w| w.clone().downcast::<gtk::CheckButton>().ok())
            .collect();
        assert_eq!(
            boxes.iter().map(|c| c.is_active()).collect::<Vec<_>>(),
            [false, true]
        );
        assert!(boxes.iter().all(|c| c.is_sensitive()));
        let history = fx.tab.history_len();
        boxes[0].clicked();
        fx.ready();
        assert_eq!(fx.text(), source.replacen("[ ]", "[x]", 1));
        assert_eq!(fx.tab.history_len(), history + 1);
        assert!(fx
            .tab
            .buffer
            .iter_at_offset(fx.index_of("todo"))
            .has_tag(&fx.live.tag("checked").unwrap()));
        fx.tab.undo(false);
        fx.ready();
        assert_eq!(fx.text(), source);
        fx.tab.undo(true);
        fx.ready();
        assert_eq!(fx.text(), source.replacen("[ ]", "[x]", 1));
    });
}

#[test]
fn test_list_continuation_termination_indent_and_backspace() {
    run(|| {
        let mut fx = Fx::new("");
        for (source, continuation) in [
            ("- item", "\n- "),
            ("3. item", "\n4. "),
            ("- [x] item", "\n- [ ] "),
            ("> quote", "\n> "),
        ] {
            let name = format!("{}{:x}.md", source.len(), source.as_bytes()[0]);
            fx.open(source, &name);
            assert!(fx.key(k::Return), "{source}");
            fx.ready();
            assert_eq!(fx.text(), format!("{source}{continuation}"));
            fx.key(k::Return);
            assert_eq!(fx.text(), format!("{source}\n"));
            fx.tab.undo(false);
            assert_eq!(fx.text(), format!("{source}{continuation}"));
        }
        fx.open("- item", "indent.md");
        let _ = fx.h.panel.set_indent(false, 2);
        fx.key(k::Tab);
        assert_eq!(fx.text(), "  - item");
        assert_eq!(fx.tab.cursor(), 8);
        fx.key(k::ISO_Left_Tab);
        assert_eq!(fx.text(), "- item");
        fx.at(2);
        fx.ready();
        assert!(fx.key(k::BackSpace));
        assert_eq!(fx.text(), "item");
        fx.tab.undo(false);
        assert_eq!(fx.text(), "- item");
        fx.open("- one\n- two\n", "selected.md");
        let (a, b) = fx.tab.buffer.bounds();
        fx.tab.buffer.select_range(&a, &b);
        fx.key(k::Tab);
        assert_eq!(fx.text(), "  - one\n  - two\n");
        fx.key(k::ISO_Left_Tab);
        assert_eq!(fx.text(), "- one\n- two\n");
    });
}

#[test]
fn test_fenced_copy_exact_code_highlighting_and_feedback() {
    run(|| {
        let source = "```python\ndef f():\n\treturn \"<&>\"\n```\n\n~~~sh\nprintf hi\n~~~\n\n```\n```\n\nend";
        let fx = Fx::new(source);
        let boxes = fx.widgets("tools");
        assert_eq!(boxes.len(), 3);
        let clipboard = gtk::Clipboard::default(&gdk::Display::default().unwrap()).unwrap();
        for (tools, expected) in
            boxes
                .iter()
                .zip(["def f():\n\treturn \"<&>\"\n", "printf hi\n", ""])
        {
            let button = livemd::tools_copy_button(tools).unwrap();
            button.clicked();
            assert_eq!(
                clipboard
                    .wait_for_text()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                expected
            );
            assert_eq!(button.label().unwrap(), "Copied");
        }
        poll(|| {
            boxes
                .iter()
                .all(|t| livemd::tools_copy_button(t).unwrap().label().unwrap() == "Copy")
        });
        poll(|| fx.live.jobs_in_flight() == 0);
        pump(20);
        let at = fx.tab.buffer.iter_at_offset(fx.index_of("return"));
        assert!(at.has_tag(&fx.live.tag("syntax-control").unwrap()));
        assert!(fx.hidden(0));
        fx.at(fx.index_of("return"));
        fx.ready();
        assert!(fx.hidden(0));
        assert_eq!(fx.text(), source);
    });
}

#[test]
fn test_tables_grid_alignment_wrapping_escaped_pipes_and_entry() {
    run(|| {
        let source = "| A | B | C |\n| :--- | :---: | ---: |\n| **bold** | left \\| right | `code` |\n| next | cell | 2 |\n\nend";
        let fx = Fx::new(source);
        let grids = fx.widgets("table");
        assert_eq!(grids.len(), 1);
        assert!(grids[0].is::<gtk::Grid>());
        let tables = fx.live.tables().expect("tables");
        let view = tables.views().remove(0);
        poll(|| view.built_rows() == view.row_count());
        for (column, alignment) in [0.0f32, 0.5, 1.0].into_iter().enumerate() {
            let cell = view.cell(0, column).unwrap();
            assert_eq!(cell.label.xalign(), alignment);
            assert!(cell.label.is_line_wrap());
            assert!(cell.widget.style_context().has_class("live-table-header"));
        }
        assert_eq!(view.cell(1, 1).unwrap().label.text(), "left | right");
        assert!(view
            .cell(1, 0)
            .unwrap()
            .label
            .label()
            .contains("<b>bold</b>"));
        // Entering a cell keeps the grid and never reveals the source.
        let cell = view.cell(1, 1).unwrap();
        let event = {
            use glib::translate::*;
            let mut event = gdk::Event::new(gdk::EventType::ButtonPress);
            unsafe {
                let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
                (*raw).button.button = 1;
            }
            event
        };
        let _: bool = cell.widget.emit_by_name("button-press-event", &[&event]);
        fx.ready();
        assert_eq!(fx.widgets("table"), grids);
        assert!(fx.hidden(1));
        assert_eq!(fx.text(), source);
        fx.at(fx.index_of("A"));
        fx.ready();
        assert_eq!(fx.widgets("table"), grids);
        assert_eq!(fx.text(), source);
    });
}

#[test]
fn test_table_insert_renders_after_caret_leaves() {
    run(|| {
        let fx = Fx::new("before\n\n");
        impro_ui::notes::apply_to_buffer("table", &fx.tab.buffer).unwrap();
        assert_eq!(fx.text(), format!("before\n\n{}", impro_core::notes::TABLE));
        fx.at(fx.tab.buffer.char_count());
        fx.ready();
        assert_eq!(fx.widgets("table").len(), 1);
        assert_eq!(fx.tab.markdown_mode(), Some("preview"));
        fx.tab.undo(false);
        fx.ready();
        assert!(fx.widgets("table").is_empty());
        assert_eq!(fx.text(), "before\n\n");
    });
}

#[test]
fn test_links_plain_click_ctrl_click_hover_and_wiki() {
    run(|| {
        let fx = Fx::new("[web](https://example.org) [[other|alias]]\n\nend");
        let other = fx.h.path("other.md");
        std::fs::write(&other, "other").unwrap();
        let opened = Rc::new(RefCell::new(Vec::<String>::new()));
        let log = opened.clone();
        fx.live.set_uri_opener(Some(Rc::new(move |uri| {
            log.borrow_mut().push(uri.to_string())
        })));
        assert!(fx.live.activate_link("https://example.org"));
        assert_eq!(*opened.borrow(), ["https://example.org"]);
        assert!(fx.live.activate_link("[[other]]"));
        poll(|| fx.h.panel.tab_for_path(&other).is_some());
        // A plain click on a rendered link follows it; a revealed one needs Ctrl.
        assert!(fx
            .live
            .press_link(Some(("https://example.org".into(), false)), false));
        assert!(!fx
            .live
            .press_link(Some(("https://example.org".into(), true)), false));
        assert!(fx
            .live
            .press_link(Some(("https://example.org".into(), true)), true));
        assert_eq!(opened.borrow().len(), 3);
        // Hovering a link switches the pointer cursor.
        fx.live.pointer_moved(60.0, 40.0);
        let window = TextViewExt::window(&fx.tab.view, gtk::TextWindowType::Text).unwrap();
        let rect = fx.tab.view.iter_location(&fx.tab.buffer.iter_at_offset(3));
        let (x, y) = fx.tab.view.buffer_to_window_coords(
            gtk::TextWindowType::Widget,
            rect.x() + 2,
            rect.y() + 3,
        );
        let link = fx.live.link_at(f64::from(x), f64::from(y));
        assert_eq!(link, Some(("https://example.org".into(), false)));
        fx.live.pointer_moved(f64::from(x), f64::from(y));
        assert!(window.cursor().is_some());
    });
}

#[test]
fn test_local_images_placeholder_and_no_network() {
    run(|| {
        let h = Harness::new();
        let pixbuf =
            gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, false, 8, 30, 20).unwrap();
        pixbuf.fill(0x3366_99ff);
        pixbuf.savev(h.path("image.png"), "png", &[]).unwrap();
        let source = "![local](image.png)\n\n![missing](none.png)\n\n![remote](https://example.org/x.png)\n\nend";
        let tab = h.open("note.md", source);
        let live = live_of(&tab);
        tab.buffer
            .place_cursor(&tab.buffer.iter_at_offset(char_len(source)));
        settle(&live);
        poll(|| live.jobs_in_flight() == 0);
        settle(&live);
        assert_eq!(live.widgets("image").len(), 1);
        let at = source.find("missing").unwrap() as i32;
        assert!(tab
            .buffer
            .iter_at_offset(at)
            .has_tag(&live.tag("image").unwrap()));
        assert_eq!(text_of(&tab.buffer), source);
    });
}

#[test]
fn test_incremental_5000_lines_and_typing_burst() {
    run(|| {
        let source = format!(
            "# Heading\n\n{}\nend",
            "paragraph **bold** and `code`\n".repeat(4997)
        );
        let fx = Fx::new(&source);
        assert_eq!(fx.live.block_count(), 5001);
        fx.live.position();
        assert!(fx.live.detail_labels().len() < 100);
        assert!(
            fx.live.max_slice_time() < std::time::Duration::from_millis(16),
            "{:?}",
            fx.live.max_slice_time()
        );
        let before = fx.live.painted_blocks();
        fx.at(fx.index_of("paragraph") + 4);
        fx.ready();
        let mut expected: Vec<char> = source.chars().collect();
        let mut at = fx.tab.cursor() as usize;
        let ticks = Rc::new(std::cell::Cell::new(0));
        let beat = ticks.clone();
        let heartbeat = glib::timeout_add_local(std::time::Duration::from_millis(1), move || {
            beat.set(beat.get() + 1);
            glib::ControlFlow::Continue
        });
        #[allow(clippy::explicit_counter_loop)]
        for c in "typing burst".chars() {
            let mut cursor = fx
                .tab
                .buffer
                .iter_at_mark(&fx.tab.buffer.get_insert().unwrap());
            fx.tab.buffer.insert(&mut cursor, &c.to_string());
            expected.insert(at, c);
            at += 1;
        }
        fx.ready();
        heartbeat.remove();
        assert_eq!(fx.text(), expected.iter().collect::<String>());
        assert!(
            fx.live.painted_blocks() - before < 20,
            "{}",
            fx.live.painted_blocks() - before
        );
        assert!(ticks.get() > 0);
        assert!(fx.live.max_slice_time() < std::time::Duration::from_millis(16));
    });
}

#[test]
fn test_theme_font_accent_and_sample_screenshots() {
    run(|| {
        let source = "# A living document\n\n## Working notes\n\n**Bold ideas**, *gentle emphasis*, and `inline code`.\n\n- A first item\n  - A nested item\n- [ ] A task to finish\n- [x] A completed task\n\n> A quote worth remembering.\n\n```python\ndef greet(name):\n    return f\"Hello {name}\"\n```\n\n| Task | Owner | State |\n| :--- | :---: | ---: |\n| Review | Team | Ready |\n| Write | Me | Next |\n\n[Project link](https://example.org) and [[note|a related note]]\n\n";
        let fx = Fx::new(source);
        let mut last = None;
        for id in ["impro-dark", "impro-light"] {
            let theme = impro_core::themes::get_theme(Some(id));
            impro_ui::apply_theme(theme, Some("#c15d84"), None, None).unwrap();
            fx.h.panel.set_syntax_style(&theme.syntax);
            fx.h.panel.set_chrome(&theme.chrome);
            fx.h.panel.set_font_size(12);
            fx.ready();
            poll(|| fx.tab.view.allocated_width() > 900);
            pump(200);
            let expected = gdk::RGBA::parse(&theme.syntax["operator"].foreground).unwrap();
            let h1: gdk::RGBA = fx.live.tag("h1").unwrap().property("foreground-rgba");
            assert_eq!(h1, expected);
            assert_eq!(fx.live.size(), 12);
            assert_eq!(fx.live.chrome(), fx.h.panel.chrome());
            assert!(fx.live.provider_css().contains("12pt"));
            assert_eq!(fx.text(), source);
            assert!(!fx.tab.buffer.is_modified());
            last = Some(theme);
        }
        let theme = last.unwrap();
        let before: gdk::RGBA = fx.live.tag("link").unwrap().property("foreground-rgba");
        impro_ui::apply_theme(theme, Some("#36995f"), None, None).unwrap();
        poll(|| {
            fx.live
                .tag("link")
                .unwrap()
                .property::<gdk::RGBA>("foreground-rgba")
                != before
        });
        let accent = impro_ui::current_theme().unwrap().ui["accent_text"].clone();
        let link: gdk::RGBA = fx.live.tag("link").unwrap().property("foreground-rgba");
        assert_eq!(link, gdk::RGBA::parse(&accent).unwrap());
        fx.h.panel.set_md_readable_width(false);
        fx.ready();
        assert_eq!(fx.tab.view.left_margin(), 32);
        fx.h.panel.set_md_readable_width(true);
        fx.ready();
        assert!(fx.live.column_width() <= 700);
        let _ = impro_ui::apply_style(None);
    });
}

// -- vertical rhythm: GTK aborts on pixels-below-lines gaps next to invisible text ----------

#[test]
fn test_blocks_keep_their_rhythm_without_pixels_below_lines() {
    run(|| {
        let mut fx = Fx::new(
            "plain\nnext\n# head\ntail\n> quote\n> more\n- item\n- item2\n1. one\n2. two\n",
        );
        assert_eq!(
            fx.gaps(&[
                "plain", "next", "head", "tail", "quote", "more", "item\n", "item2", "one", "two"
            ]),
            [8, 24, 12, 16, 20, 12, 8, 8, 8]
        );
        fx.open("## head\n### sub\n", "headings.md");
        assert_eq!(fx.gaps(&["head", "sub"]), [22]);
    });
}

fn below_tags(buffer: &gtk::TextBuffer) -> Vec<gtk::TextTag> {
    let mut found = Vec::new();
    buffer.tag_table().unwrap().foreach(|tag| {
        if tag.property::<bool>("pixels-below-lines-set")
            && tag.property::<i32>("pixels-below-lines") != 0
        {
            found.push(tag.clone());
        }
    });
    found
}

#[test]
fn test_no_tag_or_view_leaves_a_gap_below_a_line() {
    run(|| {
        let h = Harness::new();
        let pixbuf =
            gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, false, 8, 30, 20).unwrap();
        pixbuf.fill(0x3366_99ff);
        pixbuf.savev(h.path("pic.png"), "png", &[]).unwrap();
        let source = "# one\n# two\n\n- a\n  - b\n- [ ] c\n\n> q\n> > r\n\n---\n\n![pic](pic.png)\n\n```py\nx\ny\n```\n```\nz\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n[l](https://example.org) `c` **b**\n";
        let tab = h.open("note.md", source);
        let live = live_of(&tab);
        settle(&live);
        poll(|| live.jobs_in_flight() == 0);
        settle(&live);
        let buffer = &tab.buffer;
        assert!(below_tags(buffer).is_empty());
        assert_eq!(tab.view.pixels_below_lines(), 0);
        let mut offset = 0;
        while offset < char_len(source) {
            buffer.place_cursor(&buffer.iter_at_offset(offset));
            settle(&live);
            assert!(below_tags(buffer).is_empty(), "{offset}");
            offset += 7;
        }
        live.set_enabled(false);
        assert_eq!(tab.view.pixels_below_lines(), 1);
        assert_eq!(tab.view.pixels_above_lines(), 1);
    });
}

#[test]
fn test_neighbours_follow_edits_to_the_block_above() {
    run(|| {
        let fx = Fx::new("alpha\nbeta\n");
        assert_eq!(fx.gaps(&["alpha", "beta"]), [8]);
        fx.tab.buffer.insert(&mut fx.tab.buffer.start_iter(), "# ");
        fx.ready();
        assert_eq!(fx.gaps(&["alpha", "beta"]), [12]);
        let (mut a, mut b) = (fx.tab.buffer.start_iter(), fx.tab.buffer.iter_at_offset(2));
        fx.tab.buffer.delete(&mut a, &mut b);
        fx.ready();
        assert_eq!(fx.gaps(&["alpha", "beta"]), [8]);
        fx.tab
            .buffer
            .insert(&mut fx.tab.buffer.iter_at_offset(6), "> quote\n");
        fx.ready();
        assert_eq!(fx.gaps(&["alpha", "quote", "beta"]), [16, 12]);
        let (mut a, mut b) = (
            fx.tab.buffer.iter_at_offset(6),
            fx.tab.buffer.iter_at_offset(14),
        );
        fx.tab.buffer.delete(&mut a, &mut b);
        fx.ready();
        assert_eq!(fx.text(), "alpha\nbeta\n");
        assert_eq!(fx.gaps(&["alpha", "beta"]), [8]);
        assert_eq!(
            fx.live.blocks().iter().map(|b| b.lead).collect::<Vec<_>>(),
            [0, 6]
        );
    });
}

#[test]
fn test_a_blank_line_hands_the_next_block_its_own_gap_and_follows_the_caret() {
    run(|| {
        let fx = Fx::new("a\n\nb\n");
        let resting = fx.gaps(&["a", "b"]);
        fx.at(2);
        fx.ready();
        assert!(fx.gaps(&["a", "b"])[0] > resting[0]);
        assert_eq!(fx.live.blocks()[2].lead, 6);
        fx.at(char_len("a\n\nb\n"));
        fx.ready();
        assert_eq!(fx.gaps(&["a", "b"]), resting);
        assert_eq!(fx.live.blocks()[2].lead, 0);
    });
}

#[test]
fn test_pointer_without_links_or_code_tools_is_never_hit_tested() {
    run(|| {
        let fx = Fx::new("# one\n# two\n- a\n- b\n");
        let before = fx.live.hit_tests();
        for y in (0..120).step_by(3) {
            fx.live.pointer_moved(40.0, f64::from(y));
        }
        assert_eq!(fx.live.hit_tests(), before);
    });
}

#[test]
fn test_code_card_paints_one_uniform_surface_down_to_its_footer() {
    run(|| {
        let fx = Fx::new("x\n\n```python\na = 1\nb = 2\nc = 3\n```\n\nend\n");
        fx.at(0);
        fx.ready();
        let card = fx.widgets("code-card")[0].clone();
        poll(|| card.allocation().height() > 40);
        pump(60);
        let window = &fx.h.window;
        let pixels = window
            .window()
            .unwrap()
            .pixbuf(0, 0, window.allocated_width(), window.allocated_height())
            .unwrap();
        let a = card.allocation();
        let (x, y) = card
            .translate_coordinates(window, a.width() - 60, 0)
            .unwrap();
        let data = pixels.read_pixel_bytes();
        let bytes = data.as_ref();
        let (stride, channels) = (pixels.rowstride(), pixels.n_channels());
        let column: Vec<(u8, u8, u8)> = (y + 8..y + a.height() - 10)
            .map(|row| {
                let at = (row * stride + x * channels) as usize;
                (bytes[at], bytes[at + 1], bytes[at + 2])
            })
            .collect();
        assert!(column.len() > 40);
        assert!(
            column.iter().all(|p| *p == column[0]),
            "a differently shaded band sits inside the code card"
        );
    });
}
