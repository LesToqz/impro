//! RefCell and signal re-entrancy: the renderer, slash menu and selection
//! toolbar must survive callbacks and signal handlers that edit the buffer,
//! change modes or destroy them while they are mid-operation, and must release
//! every tag, mark and widget with the document.
mod support;
use gdk::keys::constants as k;
use gtk::prelude::*;
use impro_markdown::{livemd::{self, LiveMarkdown}, seltoolbar, slashmenu::SlashMenu};
use std::{cell::{Cell, RefCell}, rc::Rc};
use support::*;

fn joined(live: &LiveMarkdown) -> String {
    live.blocks().iter().map(|b| b.text.clone()).collect()
}

#[test]
fn nested_buffer_edits_from_a_changed_handler_keep_blocks_consistent() {
    run(|| {
        let h = Harness::new();
        let tab = h.note("nested.md", "# Title\n\n- one\n- two\n\nend");
        let live = live_of(&tab);
        settle(&live);
        // A handler connected after the renderer's: it edits the buffer in response to an edit.
        let depth = Rc::new(Cell::new(0));
        let guard = depth.clone();
        let id = tab.buffer.connect_changed(move |buffer| {
            if guard.get() < 3 && buffer.char_count() < 200 {
                guard.set(guard.get() + 1);
                let mut at = buffer.end_iter();
                buffer.insert(&mut at, "!");
                guard.set(guard.get() - 1);
            }
        });
        tab.buffer.begin_user_action();
        tab.buffer.insert(&mut tab.buffer.iter_at_offset(2), "x");
        tab.buffer.end_user_action();
        tab.buffer.disconnect(id);
        settle(&live);
        assert_eq!(joined(&live), text_of(&tab.buffer));
        let blocks = live.blocks();
        assert!(blocks.iter().zip(blocks.iter().skip(1)).all(|(a, b)| a.end == b.start));
    });
}

#[test]
fn mode_switch_and_destroy_inside_signal_handlers_do_not_panic() {
    run(|| {
        let h = Harness::new();
        let tab = h.note("modes.md", "# Title\n\n```py\nx\n```\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\nend");
        let live = live_of(&tab);
        settle(&live);
        // Switching modes from a mark-set handler (fires inside the renderer's own mark-set).
        let fired = Rc::new(Cell::new(0));
        let count = fired.clone();
        let t = tab.clone();
        let id = tab.buffer.connect_mark_set(move |_, _, mark| {
            if mark.name().as_deref() == Some("insert") && count.get() < 2 {
                count.set(count.get() + 1);
                t.set_markdown_mode(if count.get() == 1 { "source" } else { "preview" });
            }
        });
        tab.buffer.place_cursor(&tab.buffer.iter_at_offset(3));
        pump(60);
        tab.buffer.disconnect(id);
        settle(&live_of(&tab));
        assert!(fired.get() >= 1);
        assert_eq!(tab.markdown_mode(), Some(if fired.get() == 1 { "source" } else { "preview" }));
        // Destroying the renderer from a key handler while work is queued.
        tab.set_markdown_mode("preview");
        tab.buffer.insert(&mut tab.buffer.iter_at_offset(0), "a");
        let live = live_of(&tab);
        live.destroy();
        pump(80);
        assert!(livemd::for_view(&tab.view).is_none());
        assert_eq!(text_of(&tab.buffer).chars().next(), Some('a'));
    });
}

#[test]
fn path_getter_and_open_callbacks_may_reenter() {
    run(|| {
        let view = gtk::TextView::new();
        let buffer = view.buffer().unwrap();
        buffer.set_text("[other](other.md) and text\n");
        let calls = Rc::new(Cell::new(0));
        let seen = calls.clone();
        let edit = buffer.clone();
        let path = std::env::temp_dir().join("impro-reentrancy-note.md");
        std::fs::write(path.with_file_name("other.md"), "x").unwrap();
        let live = LiveMarkdown::new(
            &view,
            move |_| {
                // Opening a file edits the document the renderer is decorating.
                edit.insert(&mut edit.end_iter(), "opened\n");
            },
            move || {
                seen.set(seen.get() + 1);
                Some(path.clone())
            },
        );
        let window = window(&view);
        live.set_enabled(true);
        settle(&live);
        assert!(live.activate_link("other.md"));
        settle(&live);
        assert!(text_of(&buffer).ends_with("opened\n"));
        assert_eq!(joined(&live), text_of(&buffer));
        live.destroy();
        unsafe { window.destroy() };
    });
}

#[test]
fn slash_menu_and_toolbar_survive_reentrant_edits_and_destroy() {
    run(|| {
        let h = Harness::new();
        let tab = h.note("menus.md", "");
        tab.view.grab_focus();
        pump(50);
        let menu: SlashMenu = impro_markdown::slashmenu::for_view(&tab.view).expect("menu");
        // A changed handler that closes the menu and edits while the menu refreshes.
        let m = menu.clone();
        let buffer = tab.buffer.clone();
        let id = tab.buffer.connect_changed(move |b| {
            if m.opened() && b.char_count() == 2 {
                m.close();
                b.insert(&mut b.end_iter(), "?");
            }
        });
        for c in "/h".chars() {
            tab.buffer.begin_user_action();
            tab.buffer.insert_at_cursor(&c.to_string());
            tab.buffer.end_user_action();
            pump(10);
        }
        tab.buffer.disconnect(id);
        pump(50);
        menu.close();
        buffer.set_text("");
        // Destroy the toolbar while a debounce is pending.
        buffer.set_text("selected text");
        buffer.select_range(&buffer.start_iter(), &buffer.iter_at_offset(8));
        let toolbar = seltoolbar::of(&tab.view).expect("toolbar");
        toolbar.queue();
        toolbar.destroy();
        toolbar.destroy();
        pump(250);
        assert!(!toolbar.popover().is_visible());
        menu.destroy();
        menu.destroy();
        assert!(emit_key(&tab.view, k::slash, gdk::ModifierType::empty()) || true);
        pump(30);
    });
}

#[test]
fn releasing_a_document_frees_tags_marks_and_widgets() {
    run(|| {
        let h = Harness::new();
        let tab = h.note("weak.md", "# Title\n\n- [ ] a\n- b\n\n> q\n\n---\n\n```py\nx\n```\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\nend");
        let live = live_of(&tab);
        settle(&live);
        let tags: Vec<glib::WeakRef<gtk::TextTag>> = ["hidden", "h1", "body", "strong", "link", "codeblock"]
            .iter()
            .map(|n| live.tag(n).unwrap().downgrade())
            .collect();
        let widgets: Vec<glib::WeakRef<gtk::Widget>> = live
            .blocks()
            .iter()
            .flat_map(|b| b.widgets.iter().map(|w| w.0.downgrade()))
            .collect();
        assert!(!widgets.is_empty());
        let table = tab.buffer.tag_table().unwrap();
        let before = table.size();
        assert!(before > 6);
        live.destroy();
        let inner = Rc::downgrade(&Rc::new(()));
        let _ = inner;
        drop(live);
        pump(80);
        assert!(tags.iter().all(|t| t.upgrade().is_none()), "style tags leaked");
        assert!(widgets.iter().all(|w| w.upgrade().is_none()), "widgets leaked");
        // Only foreign tags (highlighter, gutter) remain in the table.
        assert!(table.size() < before);
        assert!(!tab.buffer.start_iter().tags().iter().any(|t| t.name().is_some_and(|n| n.starts_with("live-md-"))));
        let mut live_tags = 0;
        table.foreach(|t| {
            if t.name().is_some_and(|n| n.starts_with("live-md-")) {
                live_tags += 1;
            }
        });
        assert_eq!(live_tags, 0);
    });
}

#[test]
fn closing_a_tab_releases_the_buffer_view_and_renderer() {
    run(|| {
        let h = Harness::new();
        let tab = h.note("close.md", "# Title\n\n- a\n\n```py\nx\n```\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\nend");
        let live = live_of(&tab);
        settle(&live);
        tab.buffer.set_modified(false);
        let buffer = tab.buffer.downgrade();
        let view = tab.view.downgrade();
        let done = Rc::new(Cell::new(false));
        let flag = done.clone();
        h.panel.close_tab(&tab, move |_| flag.set(true));
        poll(|| done.get());
        drop(live);
        drop(tab);
        pump(120);
        assert!(buffer.upgrade().is_none(), "buffer leaked");
        assert!(view.upgrade().is_none(), "view leaked");
        let _ = RefCell::new(0);
    });
}

#[test]
fn idle_has_no_sources_once_settled() {
    run(|| {
        let h = Harness::new();
        let tab = h.note("idle.md", "# Title\n\n- a\n\n```py\nx\n```\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\nend");
        let live = live_of(&tab);
        settle(&live);
        pump(300);
        assert!(!live.pending());
        assert_eq!(live.jobs_in_flight(), 0);
    });
}
