//! Live notes inside the real main window: tables, readable width, the slash
//! menu and the selection toolbar, driven through the window's own settings
//! and focus chain (ports of the original window/editor/table/popup suites).
mod common;
use common::md::*;
use common::*;
use gdk::keys::constants as k;
use gtk::prelude::*;
use impro_core::notes;
use impro_editor::EditorTab;
use impro_markdown::{
    livemd::LiveMarkdown,
    livetable::TableView,
    seltoolbar::{self, SelectionToolbar},
    slashmenu::{self, SlashMenu},
};
use std::rc::Rc;

const TABLE: &str = "| A | B |\n| --- | ---: |\n| 1 | 2 |\n| 3 | 4 |\n";

fn none() -> gdk::ModifierType {
    gdk::ModifierType::empty()
}
fn ctrl() -> gdk::ModifierType {
    gdk::ModifierType::CONTROL_MASK
}
fn note_path(name: &str) -> String {
    format!(".impro/Notes/{name}")
}

/// A note open in the full window, with its live renderer.
struct Fx {
    h: Harness,
    tab: Rc<EditorTab>,
    live: LiveMarkdown,
}
impl Fx {
    fn new() -> Self {
        Self::open("")
    }
    fn open(text: &str) -> Self {
        Self::with(Harness::new(), text)
    }
    fn with(h: Harness, text: &str) -> Self {
        let tab = h.open(&note_path("note.md"), text);
        let live = live_of(&tab);
        let fx = Self { h, tab, live };
        fx.ready(text.contains('|'));
        fx
    }
    fn reopen(&mut self, text: &str, name: &str) {
        self.tab = self.h.open(&note_path(name), text);
        self.live = live_of(&self.tab);
        self.ready(text.contains('|'));
    }
    /// Wait for the renderer to go idle and for the table grids to be mapped.
    fn ready(&self, tables: bool) {
        settle(&self.live);
        poll(|| {
            let views = self.views();
            !self.live.pending()
                && views.is_empty() != tables
                && views.iter().all(|v| v.widget().is_mapped())
        });
        poll(|| {
            let views = self.views();
            views.iter().all(|v| v.built_rows() == v.row_count())
        });
        settle(&self.live);
        // The stylesheet the renderer loads is applied (and the text view's
        // font refreshed) in the frame clock's update phase, not when the
        // main context goes idle: wait for a real frame before measuring.
        frame(&self.h.w.window);
        settle(&self.live);
        drain();
    }
    fn views(&self) -> Vec<TableView> {
        self.live.tables().map(|t| t.views()).unwrap_or_default()
    }
    fn view(&self) -> TableView {
        let mut views = self.views();
        assert_eq!(views.len(), 1);
        views.remove(0)
    }
    fn text(&self) -> String {
        text_of(&self.tab.buffer)
    }
    fn focus(&self, row: usize, column: usize) -> TableView {
        let view = self.view();
        view.focus_cell(row, column, None, None);
        poll(|| view.focused());
        view
    }
    fn cell_buffer(&self) -> gtk::TextBuffer {
        self.view()
            .editor()
            .expect("editable table")
            .buffer()
            .unwrap()
    }
    fn type_in_cell(&self, text: &str) {
        let buffer = self.cell_buffer();
        for c in text.chars() {
            buffer.insert_at_cursor(&c.to_string());
        }
        drain();
    }
    fn cell_key(&self, key: gdk::keys::Key, state: gdk::ModifierType) -> bool {
        let editor = self.view().editor().expect("editable table");
        let handled = emit_key(editor.text_view(), key, state);
        drain();
        handled
    }
    fn at(&self, view: &TableView) -> (usize, usize) {
        let focus = view.focus_state().expect("focused cell");
        (focus.row, focus.column)
    }
    fn selection(&self, view: &TableView) -> Vec<i32> {
        view.editor()
            .unwrap()
            .buffer()
            .unwrap()
            .selection_bounds()
            .map(|(a, b)| vec![a.offset(), b.offset()])
            .unwrap_or_default()
    }
    /// Pixels from the table's first row to the paragraph after it.
    fn table_to_end(&self) -> i32 {
        let top = self
            .tab
            .view
            .line_yrange(&self.tab.buffer.iter_at_line(0))
            .0;
        let text = self.text();
        let end = text[..text.find("end").unwrap()].chars().count() as i32;
        self.tab
            .view
            .line_yrange(&self.tab.buffer.iter_at_offset(end))
            .0
            - top
    }
}

#[test]
fn test_slash_table_focuses_the_first_header_cell_with_text_selected() {
    run(|| {
        let fx = Fx::new();
        fx.tab.view.grab_focus();
        let buffer = &fx.tab.buffer;
        let menu = slashmenu::for_view(&fx.tab.view).expect("slash menu");
        for c in "/table".chars() {
            buffer.begin_user_action();
            buffer.insert_at_cursor(&c.to_string());
            buffer.end_user_action();
        }
        poll(|| menu.opened() && menu.query() == "table" && menu.popover().is_visible());
        assert!(emit_key(&fx.tab.view, k::Return, none()));
        fx.ready(true);
        let view = fx.view();
        poll(|| view.focused());
        assert_eq!(fx.at(&view), (0, 0));
        assert_eq!(fx.selection(&view), [0, "Column 1".len() as i32]);
        fx.cell_buffer().delete_selection(true, true);
        fx.type_in_cell("Name");
        assert_eq!(fx.text(), notes::SLASH_TABLE.replace("Column 1", "Name"));
        fx.cell_key(k::z, ctrl());
        assert_eq!(fx.text(), notes::SLASH_TABLE);
    });
}

#[test]
fn test_toolbar_table_focuses_first_header_cell() {
    run(|| {
        let fx = Fx::open("para");
        fx.tab.view.grab_focus();
        fx.tab.buffer.place_cursor(&fx.tab.buffer.end_iter());
        impro_ui::notes::apply_to_buffer("table", &fx.tab.buffer).unwrap();
        fx.ready(true);
        let view = fx.view();
        poll(|| view.focused());
        assert_eq!(fx.at(&view), (0, 0));
        assert_eq!(text_of(&fx.cell_buffer()), "Column 1");
    });
}

#[test]
fn test_note_command_table_enters_the_new_grid_from_below() {
    run(|| {
        let fx = Fx::open("para");
        fx.tab.view.grab_focus();
        fx.tab.buffer.place_cursor(&fx.tab.buffer.end_iter());
        fx.h.command("Notes: Table");
        fx.ready(true);
        let view = fx.view();
        poll(|| view.focused());
        // The window places the caret after the inserted table, so the grid is
        // entered from below, at its last row.
        assert_eq!(fx.at(&view).0, view.row_count() - 1);
        assert!(fx
            .text()
            .starts_with("para\n\n| Column 1 | Column 2 | Column 3 |\n"));
    });
}

#[test]
fn test_table_edit_save_rebuild_and_undo_keep_the_181_pixel_extent() {
    run(|| {
        let mut fx = Fx::open(&format!("{TABLE}\nend\n"));
        assert_eq!(fx.table_to_end(), 181);
        fx.focus(2, 1);
        fx.cell_key(k::Tab, none());
        fx.cell_key(k::Return, none());
        fx.ready(true);
        let (edited, source) = (fx.table_to_end(), fx.text());
        assert_eq!(source.matches("|  |  |").count(), 2);
        // Save through the window's own accelerator and read the file back.
        fx.tab.view.grab_focus();
        assert!(key(&fx.h.w, "<Control>s"));
        let path = fx.tab.path();
        poll(|| !fx.tab.buffer.is_modified());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
        // Undo returns to the original grid and its 181 pixel extent.
        fx.tab.undo(false);
        fx.tab.undo(false);
        fx.ready(true);
        assert_eq!(fx.text(), format!("{TABLE}\nend\n"));
        assert_eq!(fx.table_to_end(), 181);
        // A fresh render of the saved source has exactly the edited geometry.
        fx.reopen(&source, "fresh.md");
        assert_eq!(fx.text(), source);
        assert_eq!(fx.table_to_end() - edited, 0);
        let mut gaps = Vec::new();
        fx.tab.buffer.tag_table().unwrap().foreach(|tag| {
            if tag.property::<bool>("pixels-below-lines-set")
                && tag.property::<i32>("pixels-below-lines") != 0
            {
                gaps.push(tag.clone());
            }
        });
        assert!(gaps.is_empty());
    });
}

#[test]
fn test_readable_width_positions_rendered_paragraphs_on_toggle_and_resize() {
    run(|| {
        let text = format!(
            "{}\n\n> quoted text\n\n```\n{}\n```",
            "body ".repeat(200),
            "code ".repeat(100)
        );
        let fx = Fx::open(&text);
        fx.tab.buffer.place_cursor(&fx.tab.buffer.end_iter());
        fx.h.w.editor.set_md_readable_width(true);
        fx.ready(false);
        let view = &fx.tab.view;
        let assert_positions = |readable: bool| {
            fx.ready(false);
            let margin = view.left_margin();
            let width = view.allocated_width();
            let expected = if readable {
                32.max((width - impro_markdown::livemd::COLUMN + 1) / 2)
            } else {
                32
            };
            assert_eq!(margin, expected, "width {width}");
            let rendered = fx.text();
            for (word, inset) in [("body", 0), ("quoted", 22), ("code", 16)] {
                let mut at =
                    fx.tab.buffer.iter_at_offset(
                        rendered[..rendered.find(word).unwrap()].chars().count() as i32,
                    );
                if word == "quoted" {
                    at.set_line_offset(0);
                }
                assert_eq!(view.iter_location(&at).x(), margin + inset, "{word}");
            }
            // Actual glyph positions, not just the view properties: paragraph
            // tags take precedence over view-level margins.
            let mut at = fx.tab.buffer.start_iter();
            while !at.is_end() {
                let rect = view.iter_location(&at);
                assert!(rect.x() + rect.width() <= width - margin, "{rect:?}");
                at.forward_char();
            }
        };
        assert_positions(true);
        let before = view.allocated_width();
        fx.h.w.window.resize(1600, 700);
        poll(|| view.allocated_width() != before);
        assert_positions(true);
        let wide = view.allocated_width();
        fx.h.w.window.resize(900, 700);
        poll(|| view.allocated_width() != wide);
        assert_positions(true);
        fx.h.w.editor.set_font_size(18);
        assert_positions(true);
        fx.h.w.editor.set_md_readable_width(false);
        fx.ready(false);
        assert_eq!(view.left_margin(), 32);
        assert_positions(false);
    });
}

/// A selection toolbar over a note holding a long document, in the full window.
struct Bar {
    fx: Fx,
}
impl Bar {
    fn new(values: impl FnOnce(&mut impro_core::settings::Values)) -> Self {
        let mut v = impro_core::settings::defaults().clone();
        values(&mut v);
        let text = format!(
            "A note\n\nAn editable paragraph with selected words.\n{}",
            "\nMore text.".repeat(100)
        );
        let fx = Fx::with(Harness::with(v), &text);
        fx.tab.view.grab_focus();
        Self { fx }
    }
    fn toolbar(&self) -> SelectionToolbar {
        seltoolbar::of(&self.fx.tab.view).expect("selection toolbar")
    }
    fn select(&self, start: i32, end: i32) {
        let buffer = &self.fx.tab.buffer;
        self.fx.tab.view.grab_focus();
        buffer.place_cursor(&buffer.start_iter());
        buffer.select_range(&buffer.iter_at_offset(end), &buffer.iter_at_offset(start));
    }
    fn visible(&self) -> bool {
        self.toolbar().popover().is_visible()
    }
    /// Wait for the debounce to fire; the popover then reflects the selection.
    fn settled(&self) {
        poll(|| !self.toolbar().timeout_pending());
        drain();
    }
}

#[test]
fn test_selection_toolbar_setting_loads_and_notes_toggle_applies_live() {
    run(|| {
        let bar = Bar::new(|v| {
            v.insert("notes_selection_toolbar".into(), false.into());
        });
        assert!(!bar.toolbar().eligible());
        bar.select(8, 16);
        bar.settled();
        assert!(!bar.visible());
        let window = &bar.fx.h.w;
        window.open_settings(Some("notes"));
        let dialog = window.settings_dialog().unwrap();
        let toggle = |dialog: &impro_settings_ui::SettingsDialog| {
            let control = dialog.control("notes_selection_toolbar").unwrap();
            control
                .downcast_ref::<gtk::Button>()
                .expect("toggle button")
                .clicked();
        };
        toggle(&dialog);
        assert_eq!(
            window.values().get("notes_selection_toolbar").cloned(),
            Some(true.into()),
            "toggle on"
        );
        dialog.hide();
        assert!(bar.toolbar().eligible());
        bar.select(8, 16);
        bar.settled();
        poll(|| bar.visible());
        window.open_settings(Some("notes"));
        toggle(&dialog);
        assert_eq!(
            window.values().get("notes_selection_toolbar").cloned(),
            Some(false.into()),
            "toggle off"
        );
        assert!(!bar.visible());
        dialog.hide();
        assert!(!bar.toolbar().eligible());
        bar.select(8, 16);
        bar.settled();
        assert!(!bar.visible());
    });
}

#[test]
fn test_selection_toolbar_first_line_uses_space_below_and_buttons_are_compact() {
    run(|| {
        let bar = Bar::new(|_| {});
        bar.select(0, 6);
        bar.settled();
        poll(|| bar.visible());
        let toolbar = bar.toolbar();
        assert_eq!(toolbar.popover().position(), gtk::PositionType::Bottom);
        let sizes = || -> std::collections::BTreeSet<(i32, i32)> {
            toolbar
                .buttons()
                .values()
                .map(|b| (b.allocated_width(), b.allocated_height()))
                .collect()
        };
        poll(|| !sizes().contains(&(1, 1)));
        assert_eq!(sizes(), [(28, 28)].into_iter().collect());
        // Painted pixels: the popover surface shows its buttons, not a blank fill.
        frame(&bar.fx.h.w.window);
        let surface = toolbar.popover().window().expect("popover surface");
        let pixbuf = surface
            .pixbuf(0, 0, surface.width(), surface.height())
            .expect("popover pixels");
        let colours: std::collections::BTreeSet<[u8; 3]> = pixbuf
            .read_pixel_bytes()
            .chunks(pixbuf.n_channels() as usize)
            .map(|p| [p[0], p[1], p[2]])
            .collect();
        assert!(colours.len() > 4, "{} colours", colours.len());
        assert_eq!(surface.width(), toolbar.popover().allocated_width());
    });
}

#[test]
fn test_selection_toolbar_window_shortcut_precedes_the_explorer_accelerator() {
    run(|| {
        let bar = Bar::new(|_| {});
        bar.select(8, 16);
        bar.settled();
        poll(|| bar.visible());
        let explorer = bar.fx.h.w.explorer.is_visible();
        assert!(emit_key(&bar.fx.h.w.window, k::b, ctrl()));
        drain();
        assert!(bar.fx.text().contains("**"));
        assert_eq!(bar.fx.h.w.explorer.is_visible(), explorer);
    });
}

fn slash_fixture(values: impl FnOnce(&mut impro_core::settings::Values)) -> (Fx, Rc<EditorTab>) {
    let mut v = impro_core::settings::defaults().clone();
    values(&mut v);
    let fx = Fx::with(Harness::with(v), "");
    let tab = fx.tab.clone();
    (fx, tab)
}
fn menu_of(tab: &EditorTab) -> Option<SlashMenu> {
    slashmenu::for_view(&tab.view)
}

#[test]
fn test_notes_slash_commands_dispatch_startup_and_live() {
    run(|| {
        let (fx, tab) = slash_fixture(|v| {
            v.insert("notes_slash_commands".into(), false.into());
        });
        let window = &fx.h.w;
        assert!(!window.editor.slash_commands());
        assert!(menu_of(&tab).is_none());
        window
            .set_setting("notes_slash_commands", true.into(), false)
            .unwrap();
        let menu = menu_of(&tab).expect("menu after enabling");
        // The enabled menu really pops up for `/` and filters as the user types.
        tab.view.grab_focus();
        for c in "/tab".chars() {
            tab.buffer.begin_user_action();
            tab.buffer.insert_at_cursor(&c.to_string());
            tab.buffer.end_user_action();
        }
        poll(|| menu.opened() && menu.query() == "tab" && menu.popover().is_visible());
        assert!(!menu.rows().is_empty());
        assert!(menu.matches().iter().any(|c| c.id == "table"));
        assert!(menu.popover().is_visible());
        assert!(emit_key(&tab.view, k::Escape, none()));
        assert!(!menu.popover().is_visible());
        // Disabling removes the menu and the popup does not come back.
        window
            .set_setting("notes_slash_commands", false.into(), false)
            .unwrap();
        assert!(menu_of(&tab).is_none());
        assert!(!menu.popover().is_visible());
        tab.buffer.set_text("");
        tab.buffer.begin_user_action();
        tab.buffer.insert_at_cursor("/");
        tab.buffer.end_user_action();
        drain();
        assert!(menu_of(&tab).is_none());
    });
}
