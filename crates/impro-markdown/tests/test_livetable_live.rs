//! Live tables inside the real editor and renderer (the cases of the original
//! `test_livetable` that need `LiveMarkdown` as the `TableHost`).
mod support;
use gdk::keys::constants as k;
use gtk::prelude::*;
use impro_core::notes;
use impro_editor::EditorTab;
use impro_markdown::{livemd::LiveMarkdown, livetable::TableView};
use std::rc::Rc;
use support::*;

const TABLE: &str = "| A | B |\n| --- | ---: |\n| 1 | 2 |\n| 3 | 4 |\n";

struct Fx {
    h: Harness,
    tab: Rc<EditorTab>,
    live: LiveMarkdown,
}
impl Fx {
    fn new() -> Self {
        impro_ui::motion::set_reduce_motion(true);
        let h = Harness::new();
        let tab = h.note("note.md", "");
        let live = live_of(&tab);
        Self { h, tab, live }
    }
    fn open(text: &str) -> Self {
        impro_ui::motion::set_reduce_motion(true);
        let h = Harness::new();
        let tab = h.note("note.md", text);
        let live = live_of(&tab);
        let fx = Self { h, tab, live };
        fx.ready(text.contains('|'));
        fx
    }
    fn reopen(&mut self, text: &str, name: &str) {
        self.tab = self.h.note(name, text);
        self.live = live_of(&self.tab);
        self.ready(text.contains('|'));
    }
    fn ready(&self, tables: bool) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            pump(2);
            let views = self.views();
            if !self.live.pending()
                && views.is_empty() != tables
                && views.iter().all(|v| v.widget().is_mapped())
            {
                break;
            }
            assert!(std::time::Instant::now() < until, "tables did not settle");
        }
        pump(30);
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
    fn buffer(&self) -> gtk::TextBuffer {
        self.tab.buffer.clone()
    }
    fn hidden(&self, offset: i32) -> bool {
        self.buffer()
            .iter_at_offset(offset)
            .has_tag(&self.live.tag("hidden").unwrap())
    }
    fn focus(&self, row: usize, column: usize) -> TableView {
        let view = self.view();
        view.focus_cell(row, column, None, None);
        poll(|| view.focused());
        view
    }
    fn cell_text(&self) -> String {
        let editor = self.view().editor().expect("editable table");
        let buffer = editor.buffer().unwrap();
        text_of(&buffer)
    }
    fn type_text(&self, text: &str) {
        let editor = self.view().editor().expect("editable table");
        let buffer = editor.buffer().unwrap();
        for c in text.chars() {
            buffer.insert_at_cursor(&c.to_string());
        }
        pump(20);
    }
    fn key(&self, key: gdk::keys::Key, state: gdk::ModifierType) -> bool {
        let editor = self.view().editor().expect("editable table");
        let handled = emit_key(editor.text_view(), key, state);
        pump(30);
        handled
    }
    fn at(&self, view: &TableView) -> (usize, usize) {
        let focus = view.focus_state().expect("focused cell");
        (focus.row, focus.column)
    }
    fn selection(&self, view: &TableView) -> Vec<i32> {
        let buffer = view.editor().unwrap().buffer().unwrap();
        buffer
            .selection_bounds()
            .map(|(a, b)| vec![a.offset(), b.offset()])
            .unwrap_or_default()
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
fn none() -> gdk::ModifierType {
    gdk::ModifierType::empty()
}

#[test]
fn test_slash_table_focuses_the_first_header_cell_with_text_selected() {
    run(|| {
        let fx = Fx::new();
        fx.tab.view.grab_focus();
        for c in "/table".chars() {
            fx.buffer().begin_user_action();
            fx.buffer().insert_at_cursor(&c.to_string());
            fx.buffer().end_user_action();
            pump(10);
        }
        pump(50);
        emit_key(&fx.tab.view, k::Return, none());
        fx.ready(true);
        let view = fx.view();
        poll(|| view.focused());
        assert_eq!(fx.at(&view), (0, 0));
        assert_eq!(fx.selection(&view), [0, "Column 1".len() as i32]);
        view.editor()
            .unwrap()
            .buffer()
            .unwrap()
            .delete_selection(true, true);
        fx.type_text("Name");
        assert_eq!(fx.text(), notes::SLASH_TABLE.replace("Column 1", "Name"));
        fx.key(k::z, ctrl());
        assert_eq!(fx.text(), notes::SLASH_TABLE);
    });
}

#[test]
fn test_toolbar_table_focuses_first_header_cell() {
    run(|| {
        let fx = Fx::open("para");
        fx.tab.view.grab_focus();
        fx.buffer().place_cursor(&fx.buffer().end_iter());
        impro_ui::notes::apply_to_buffer("table", &fx.buffer()).unwrap();
        fx.ready(true);
        let view = fx.view();
        poll(|| view.focused());
        assert_eq!(fx.at(&view), (0, 0));
        assert_eq!(fx.cell_text(), "Column 1");
    });
}

#[test]
fn test_grid_stays_rendered_with_the_caret_inside_and_source_hidden() {
    run(|| {
        let source = format!("intro\n\n{TABLE}\nend\n");
        let fx = Fx::open(&source);
        let view = fx.view();
        let start = source.find('|').unwrap() as i32;
        let char_at = |needle: &str| source[..source.find(needle).unwrap()].chars().count() as i32;
        for offset in [start, start + 5, char_at("| 3"), char_at("\nend") - 1] {
            fx.buffer()
                .place_cursor(&fx.buffer().iter_at_offset(offset));
            fx.ready(true);
            assert_eq!(fx.view().block().id, view.block().id);
            assert!(fx.hidden(start + 1));
            assert!(fx.hidden(char_at("---")));
        }
        poll(|| view.built_rows() == view.row_count());
        let rows: Vec<Vec<String>> = (0..3)
            .map(|r| {
                (0..2)
                    .map(|c| view.cell(r, c).unwrap().label.text().to_string())
                    .collect()
            })
            .collect();
        assert_eq!(rows, [["A", "B"], ["1", "2"], ["3", "4"]]);
        assert_eq!(view.cell(0, 1).unwrap().label.xalign(), 1.0);
        assert!(view
            .cell(0, 0)
            .unwrap()
            .widget
            .style_context()
            .has_class("live-table-header"));
        assert_eq!(fx.text(), source);
    });
}

#[test]
fn test_typing_rewrites_only_its_row_and_keeps_grid_and_focus() {
    run(|| {
        let source = format!("intro\n\n{TABLE}\nend\n");
        let fx = Fx::open(&source);
        let view = fx.focus(1, 1);
        let editor = view.editor().unwrap();
        fx.type_text("0 | x");
        assert_eq!(fx.text(), source.replace("| 1 | 2 |", "| 1 | 20 \\| x |"));
        assert_eq!(fx.view().block().id, view.block().id);
        assert_eq!(fx.view().editor().unwrap().text_view(), editor.text_view());
        assert!(view.focused());
        assert_eq!(fx.at(&view), (1, 1));
        let joined: String = fx.live.blocks().iter().map(|b| b.text.clone()).collect();
        assert_eq!(joined, fx.text());
        let block = fx
            .live
            .blocks()
            .into_iter()
            .find(|b| b.kind == "table")
            .unwrap();
        assert_eq!(block.text, view.text());
        let text = fx.text();
        let two = text[..text.find("20").unwrap()].chars().count() as i32;
        assert!(fx.hidden(two + 1));
        // The main caret mirrors the cell caret (status bar, undo cursor).
        let x = text.find('x').unwrap();
        let expected = text[..x + text[x..].find(" |").unwrap()].chars().count() as i32;
        assert_eq!(fx.tab.cursor(), expected);
        let saved = Rc::new(std::cell::Cell::new(None));
        let flag = saved.clone();
        fx.h.panel.save_current(move |ok| flag.set(Some(ok)));
        poll(|| saved.get().is_some());
        assert_eq!(saved.get(), Some(true));
        assert_eq!(std::fs::read_to_string(fx.tab.path()).unwrap(), fx.text());
    });
}

#[test]
fn test_typing_burst_is_one_undo_step_and_undo_redo_refocus_the_cell() {
    run(|| {
        let source = format!("{TABLE}\nend\n");
        let fx = Fx::open(&source);
        let view = fx.focus(2, 0);
        fx.type_text("abc");
        fx.key(k::z, ctrl());
        fx.ready(true);
        assert_eq!(fx.text(), source);
        let restored = fx.view();
        poll(|| restored.focused());
        assert_eq!(fx.at(&restored), (2, 0));
        let editor = restored.editor().unwrap();
        emit_key(editor.text_view(), k::y, ctrl());
        fx.ready(true);
        assert_eq!(fx.text(), source.replace("| 3 |", "| 3abc |"));
        poll(|| fx.view().focused());
        let editor = fx.view().editor().unwrap();
        emit_key(
            editor.text_view(),
            k::z,
            ctrl() | gdk::ModifierType::SHIFT_MASK,
        );
        fx.ready(true);
        assert_eq!(fx.text(), source.replace("| 3 |", "| 3abc |"));
        assert_ne!(view.block().id, fx.view().block().id);
    });
}

#[test]
fn test_external_buffer_edit_refreshes_the_grid() {
    run(|| {
        let source = format!("x\n\n{TABLE}");
        let fx = Fx::open(&source);
        let start = source[..source.find('3').unwrap()].chars().count() as i32;
        let buffer = fx.buffer();
        buffer.begin_user_action();
        let (mut a, mut b) = (
            buffer.iter_at_offset(start),
            buffer.iter_at_offset(start + 1),
        );
        buffer.delete(&mut a, &mut b);
        buffer.insert(&mut buffer.iter_at_offset(start), "three");
        buffer.end_user_action();
        fx.ready(true);
        let view = fx.view();
        poll(|| view.built_rows() == view.row_count());
        assert_eq!(view.cell(2, 0).unwrap().label.text(), "three");
    });
}

#[test]
fn test_keyboard_navigation_between_cells_rows_and_out_of_the_table() {
    run(|| {
        let source = format!("before\n\n{TABLE}\nafter\n");
        let fx = Fx::open(&source);
        let view = fx.focus(0, 0);
        let go = |key: gdk::keys::Key, state| {
            fx.key(key, state);
        };
        go(k::Tab, none());
        assert_eq!(fx.at(&view), (0, 1));
        go(k::Tab, none());
        assert_eq!(fx.at(&view), (1, 0));
        go(k::ISO_Left_Tab, gdk::ModifierType::SHIFT_MASK);
        assert_eq!(fx.at(&view), (0, 1));
        go(k::Return, none());
        assert_eq!(fx.at(&view), (1, 1));
        go(k::Down, none());
        assert_eq!(fx.at(&view), (2, 1));
        go(k::Up, none());
        assert_eq!(fx.at(&view), (1, 1));
        go(k::Right, none());
        assert_eq!(fx.at(&view), (2, 0));
        go(k::Left, none());
        assert_eq!(fx.at(&view), (1, 1));
        go(k::Home, none());
        go(k::Left, none());
        assert_eq!(fx.at(&view), (1, 0));
        go(k::Return, gdk::ModifierType::SHIFT_MASK);
        fx.type_text("x");
        assert!(fx.text().contains("| 1<br>x | 2 |"));
        assert_eq!(fx.cell_text(), "1\nx");
        fx.focus(0, 0);
        go(k::Up, none());
        assert!(fx.tab.view.is_focus());
        assert_eq!(fx.tab.cursor(), "before\n".len() as i32);
        fx.focus(2, 1);
        go(k::Down, none());
        assert!(fx.tab.view.is_focus());
        let text = fx.text();
        assert_eq!(
            fx.tab.cursor(),
            text[..text.find("\nafter").unwrap()].chars().count() as i32
        );
        fx.focus(1, 1);
        go(k::Escape, none());
        assert!(fx.tab.view.is_focus());
    });
}

#[test]
fn test_tab_and_enter_at_the_end_add_rows_and_backspace_is_harmless() {
    run(|| {
        let fx = Fx::open(&format!("{TABLE}\nend\n"));
        let view = fx.focus(2, 1);
        fx.key(k::Tab, none());
        assert_eq!(fx.at(&view), (3, 0));
        assert_eq!(fx.text(), format!("{TABLE}|  |  |\n\nend\n"));
        fx.key(k::BackSpace, none());
        fx.key(k::BackSpace, none());
        assert_eq!(fx.text(), format!("{TABLE}|  |  |\n\nend\n"));
        fx.key(k::Right, none());
        fx.key(k::Return, none());
        assert_eq!(fx.at(&view), (4, 1));
        assert_eq!(fx.text().matches("|  |  |").count(), 2);
        fx.tab.undo(false);
        fx.ready(true);
        assert_eq!(fx.text().matches("|  |  |").count(), 1);
    });
}

fn table_to_end(fx: &Fx) -> i32 {
    let top = fx.tab.view.line_yrange(&fx.buffer().iter_at_line(0)).0;
    let text = fx.text();
    let end = text[..text.find("end").unwrap()].chars().count() as i32;
    fx.tab.view.line_yrange(&fx.buffer().iter_at_offset(end)).0 - top
}

#[test]
fn test_in_place_row_edits_keep_the_spacing_of_a_fresh_render() {
    run(|| {
        let mut fx = Fx::open(&format!("{TABLE}\nend\n"));
        let view = fx.view();
        poll(|| view.built_rows() == view.row_count());
        pump(100);
        assert_eq!(table_to_end(&fx), 181);
        fx.focus(2, 1);
        fx.key(k::Tab, none());
        fx.key(k::Return, none());
        pump(200);
        let (edited, source) = (table_to_end(&fx), fx.text());
        fx.reopen(&source, "fresh.md");
        let view = fx.view();
        poll(|| view.built_rows() == view.row_count());
        pump(200);
        assert_eq!(table_to_end(&fx) - edited, 0);
        let mut found = Vec::new();
        fx.buffer().tag_table().unwrap().foreach(|tag| {
            if tag.property::<bool>("pixels-below-lines-set")
                && tag.property::<i32>("pixels-below-lines") != 0
            {
                found.push(tag.clone());
            }
        });
        assert!(found.is_empty());
    });
}

#[test]
fn test_down_from_the_last_block_creates_a_paragraph_after_the_table() {
    run(|| {
        let fx = Fx::open(&format!("top\n\n{TABLE}"));
        fx.focus(2, 0);
        fx.key(k::Down, none());
        assert_eq!(fx.text(), format!("top\n\n{TABLE}\n"));
        assert_eq!(fx.tab.cursor(), fx.buffer().char_count());
        assert!(fx.tab.view.is_focus());
    });
}

#[test]
fn test_main_caret_entering_from_above_or_below_lands_in_a_cell() {
    run(|| {
        let source = format!("top\n\n{TABLE}\nend\n");
        let fx = Fx::open(&source);
        fx.tab.view.grab_focus();
        let at = |needle: &str| source[..source.find(needle).unwrap()].chars().count() as i32;
        fx.buffer().place_cursor(&fx.buffer().iter_at_offset(1));
        fx.ready(true);
        fx.buffer()
            .place_cursor(&fx.buffer().iter_at_offset(at("|")));
        let view = fx.view();
        poll(|| view.focused());
        assert_eq!(fx.at(&view), (0, 0));
        fx.tab.view.grab_focus();
        fx.buffer()
            .place_cursor(&fx.buffer().iter_at_offset(at("end")));
        fx.ready(true);
        fx.buffer()
            .place_cursor(&fx.buffer().iter_at_offset(at("|")));
        let view = fx.view();
        poll(|| view.focused());
        assert_eq!(fx.at(&view), (2, 0));
    });
}

#[test]
fn test_structure_menu_actions_are_single_undo_steps_without_rebuilds() {
    run(|| {
        let source = format!("x\n\n{TABLE}\nend\n");
        let fx = Fx::open(&source);
        let view = fx.focus(1, 0);
        let steps: [(&str, &str, usize, &str); 9] = [
            (
                "row",
                "after",
                1,
                "| A | B |\n| --- | ---: |\n| 1 | 2 |\n|  |  |\n| 3 | 4 |\n",
            ),
            (
                "row",
                "down",
                1,
                "| A | B |\n| --- | ---: |\n|  |  |\n| 1 | 2 |\n| 3 | 4 |\n",
            ),
            ("row", "delete", 1, TABLE),
            (
                "column",
                "before",
                0,
                "|  | A | B |\n| --- | --- | ---: |\n|  | 1 | 2 |\n|  | 3 | 4 |\n",
            ),
            (
                "column",
                "align-center",
                0,
                "|  | A | B |\n| :---: | --- | ---: |\n|  | 1 | 2 |\n|  | 3 | 4 |\n",
            ),
            (
                "column",
                "down",
                0,
                "| A |  | B |\n| --- | :---: | ---: |\n| 1 |  | 2 |\n| 3 |  | 4 |\n",
            ),
            ("column", "delete", 1, TABLE),
            (
                "column",
                "clear",
                1,
                "| A | B |\n| --- | ---: |\n| 1 |  |\n| 3 |  |\n",
            ),
            (
                "row",
                "clear",
                0,
                "|  |  |\n| --- | ---: |\n| 1 |  |\n| 3 |  |\n",
            ),
        ];
        let mut history = Vec::new();
        for (axis, action, index, expected) in steps {
            history.push(fx.text());
            view.act(axis, action, index);
            pump(30);
            assert_eq!(
                fx.text(),
                format!("x\n\n{expected}\nend\n"),
                "{axis} {action}"
            );
            assert_eq!(fx.view().block().id, view.block().id);
            assert!(view.focused());
            let joined: String = fx.live.blocks().iter().map(|b| b.text.clone()).collect();
            assert_eq!(joined, fx.text());
        }
        for previous in history.iter().rev() {
            fx.tab.undo(false);
            assert_eq!(&fx.text(), previous);
        }
        fx.ready(true);
        assert_eq!(fx.text(), source);
    });
}

#[test]
fn test_add_bars_append_a_row_and_a_column() {
    run(|| {
        let fx = Fx::open(TABLE);
        let view = fx.view();
        view.append("row");
        pump(30);
        assert_eq!(fx.at(&view), (3, 0));
        view.append("column");
        pump(30);
        assert_eq!(fx.at(&view), (0, 2));
        assert_eq!(
            fx.text(),
            "| A | B |  |\n| --- | ---: | --- |\n| 1 | 2 |  |\n| 3 | 4 |  |\n|  |  |  |\n"
        );
    });
}

#[test]
fn test_caret_placed_in_an_unbuilt_table_enters_its_grid_when_it_is_built() {
    run(|| {
        let filler = "A filler paragraph.\n\n".repeat(80);
        let source = format!("{filler}{TABLE}\nend\n");
        impro_ui::motion::set_reduce_motion(true);
        let h = Harness::new();
        let tab = h.note("note.md", &source);
        let live = live_of(&tab);
        let fx = Fx { h, tab, live };
        settle(&fx.live);
        fx.tab.view.grab_focus();
        let at = source[..source.find("| 1").unwrap()].chars().count() as i32 + 3;
        fx.buffer().place_cursor(&fx.buffer().iter_at_offset(at));
        settle(&fx.live);
        // Scroll the table into view: its grid is built and takes the caret.
        let adjustment = fx.tab.view.vadjustment().unwrap();
        let target = fx.tab.view.line_yrange(&fx.buffer().iter_at_offset(at)).0 - 100;
        poll(|| {
            adjustment.set_value(f64::from(target));
            fx.views().iter().any(|v| v.focused())
        });
        assert_eq!(fx.at(&fx.view()), (1, 0));
    });
}

#[test]
fn test_malformed_tables_render_and_edit_without_errors() {
    run(|| {
        let wide = (0..40)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" | ");
        let source = format!(
            "| a | `x|y` | c |\n| --- | --- | --- |\n\n| a | b | c |\n| --- | --- | --- |\n| short |\n| 1 | `2|3` | 4 | 5 |\n\n| no rule |\n| row |\n\n| only | header |\n| :---: | --- |\n\n| {wide} |\n|{} \n\nend\n",
            " --- |".repeat(40)
        );
        let fx = Fx::open(&source);
        poll(|| fx.views().len() == 3);
        let mut views = fx.views();
        views.sort_by_key(|v| v.block().offset(&fx.buffer()));
        assert_eq!(views[0].model().count, 3);
        for v in &views {
            poll(|| v.built_rows() == v.row_count());
        }
        let cells = |view: &TableView, row: usize| -> Vec<String> {
            (0..view.column_count())
                .map(|c| view.cell(row, c).unwrap().source())
                .collect()
        };
        assert_eq!(cells(&views[0], 1), ["short", "", ""]);
        assert_eq!(cells(&views[0], 2), ["1", "`2", "3`"]);
        views[0].focus_cell(1, 2, None, None);
        poll(|| views[0].focused());
        views[0]
            .editor()
            .unwrap()
            .buffer()
            .unwrap()
            .insert_at_cursor("z");
        pump(30);
        assert!(fx.text().contains("| short |  | z |\n"));
        assert!(fx.text().contains("| 1 | `2|3` | 4 | 5 |"));
        views[1].focus_cell(0, 0, None, None);
        poll(|| views[1].focused());
        views[1]
            .editor()
            .unwrap()
            .buffer()
            .unwrap()
            .insert_at_cursor("!");
        pump(30);
        assert!(fx.text().contains("| only! | header |\n| :---: | --- |\n"));
        assert_eq!(fx.text().matches("| no rule |\n| row |").count(), 1);
    });
}

#[test]
fn test_two_hundred_rows_build_in_bounded_slices_and_type_fast() {
    run(|| {
        let rows: String = (0..200).map(|i| format!("| r{i} | v{i} |\n")).collect();
        let source = format!("| A | B |\n| --- | --- |\n{rows}\nend\n");
        let fx = Fx::open(&source);
        let view = fx.view();
        poll(|| view.built_rows() == view.row_count());
        assert!(
            view.max_slice_time() < std::time::Duration::from_millis(16),
            "{:?}",
            view.max_slice_time()
        );
        fx.focus(100, 0);
        let started = std::time::Instant::now();
        for _ in 0..20 {
            view.editor()
                .unwrap()
                .buffer()
                .unwrap()
                .insert_at_cursor("x");
        }
        pump(20);
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        let text = fx.text();
        let line = text
            .lines()
            .find(|l| l.contains(&"x".repeat(20)))
            .expect("edited row");
        assert!(line.contains("r99") && line.contains("v99"), "{line}");
    });
}
