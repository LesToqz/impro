mod support;
use gtk::prelude::*;
use impro_markdown::livetable::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Instant,
};
const TABLE: &str = "| A | B |\n| --- | ---: |\n| 1 | 2 |\n| 3 | 4 |\n";
struct Host {
    view: gtk::TextView,
    slot: gtk::Box,
    silent: Cell<i32>,
    hidden: gtk::TextTag,
    actions: Cell<u32>,
    ends: Cell<u32>,
    retags: Cell<u32>,
    history: RefCell<Vec<bool>>,
}
impl TableHost for Host {
    fn view(&self) -> gtk::TextView {
        self.view.clone()
    }
    fn silent(&self, enter: bool) {
        self.silent
            .set(self.silent.get() + if enter { 1 } else { -1 });
    }
    fn replacement(&self, block: &TableBlock, widget: &gtk::Widget, _height: i32) {
        self.slot.pack_start(widget, true, true, 0);
        widget.show_all();
        self.retag(block);
    }
    fn retag(&self, block: &TableBlock) {
        self.retags.set(self.retags.get() + 1);
        let b = self.view.buffer().unwrap();
        let a = b.iter_at_mark(&block.start);
        let z = b.iter_at_mark(&block.end);
        b.apply_tag(&self.hidden, &a, &z);
    }
    fn queue_layout(&self) {}
    fn history(&self, redo: bool) -> bool {
        self.history.borrow_mut().push(redo);
        true
    }
}
struct Fixture {
    host: Rc<Host>,
    tables: Tables,
    grid: TableView,
    window: gtk::Window,
}
impl Fixture {
    fn new(source: &str, start: usize, end: usize, editable: bool) -> Self {
        Self::new_mode(source, start, end, editable, true)
    }
    fn new_mode(source: &str, start: usize, end: usize, editable: bool, fill: bool) -> Self {
        impro_markdown::init();
        impro_ui::init();
        impro_ui::style::apply_style(None).unwrap();
        impro_ui::motion::set_reduce_motion(true);
        let view = gtk::TextView::new();
        view.set_editable(editable);
        let b = view.buffer().unwrap();
        b.set_text(source);
        let tag = gtk::TextTag::new(None);
        tag.set_property("invisible", true);
        b.tag_table().unwrap().add(&tag);
        let host = Rc::new(Host {
            view,
            slot: gtk::Box::new(gtk::Orientation::Vertical, 0),
            silent: Cell::new(0),
            hidden: tag,
            actions: Cell::new(0),
            ends: Cell::new(0),
            retags: Cell::new(0),
            history: RefCell::new(Vec::new()),
        });
        let weak = Rc::downgrade(&host);
        b.connect_begin_user_action(move |_| {
            if let Some(h) = weak.upgrade() {
                h.actions.set(h.actions.get() + 1);
            }
        });
        let weak = Rc::downgrade(&host);
        b.connect_end_user_action(move |_| {
            if let Some(h) = weak.upgrade() {
                h.ends.set(h.ends.get() + 1);
            }
        });
        let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        outer.pack_start(&host.slot, false, false, 0);
        outer.pack_start(&host.view, true, true, 0);
        outer.set_margin_start(30);
        outer.set_margin_top(30);
        let block = TableBlock::new(
            1,
            &b,
            start as i32,
            end as i32,
            &source
                .chars()
                .skip(start)
                .take(end - start)
                .collect::<String>(),
        );
        let tables = Tables::new(host.clone());
        let grid = tables.build(block).unwrap();
        let window = support::window(&outer);
        window.set_default_size(850, 720);
        window.present();
        if fill {
            while grid.fill_slice() {}
        }
        support::pump(20);
        Self {
            host,
            tables,
            grid,
            window,
        }
    }
    fn table() -> Self {
        Self::new(TABLE, 0, TABLE.len(), true)
    }
    fn text(&self) -> String {
        let b = self.host.view.buffer().unwrap();
        b.text(&b.start_iter(), &b.end_iter(), true).unwrap().into()
    }
    fn focus(&self, r: usize, c: usize) {
        self.grid.focus_cell(r, c, None, None);
        support::pump(5);
    }
    fn type_text(&self, s: &str) {
        self.grid
            .editor()
            .unwrap()
            .buffer()
            .unwrap()
            .insert_at_cursor(s);
    }
    fn key(&self, key: gdk::keys::Key) {
        assert!(self.grid.key(key, gdk::ModifierType::empty()));
    }
    fn current(&self) -> (usize, usize) {
        let p = self.grid.focus_state().unwrap();
        (p.row, p.column)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.tables.destroy();
        unsafe {
            self.window.destroy();
        }
        support::pump(5);
    }
}
#[test]
fn test_grid_stays_rendered_with_the_caret_inside_and_source_hidden() {
    support::run(|| {
        let f = Fixture::table();
        for offset in [0, 5, TABLE.find("| 3").unwrap(), TABLE.len() - 1] {
            f.host.view.grab_focus();
            f.tables.caret(None, -1);
            assert!(f.tables.caret(Some(1), offset as i32));
            support::pump(5);
            assert!(f.grid.focused());
            assert!(f
                .host
                .view
                .buffer()
                .unwrap()
                .iter_at_offset(1)
                .has_tag(&f.host.hidden));
        }
        for (r, row) in [["A", "B"], ["1", "2"], ["3", "4"]].iter().enumerate() {
            for (c, text) in row.iter().enumerate() {
                assert_eq!(f.grid.cell(r, c).unwrap().label.text(), *text);
            }
        }
        assert_eq!(f.grid.cell(0, 1).unwrap().label.xalign(), 1.);
        assert!(f
            .grid
            .cell(0, 0)
            .unwrap()
            .widget
            .style_context()
            .has_class("live-table-header"));
        assert_eq!(f.text(), TABLE);
    });
}
#[test]
fn test_typing_rewrites_only_its_row_and_keeps_grid_and_focus() {
    support::run(|| {
        let f = Fixture::table();
        f.focus(1, 1);
        let editor = f.grid.editor().unwrap().text_view().clone();
        f.type_text("0 | x");
        assert_eq!(f.text(), TABLE.replace("| 1 | 2 |", "| 1 | 20 \\| x |"));
        assert_eq!(f.grid.editor().unwrap().text_view(), &editor);
        assert!(f.grid.focused());
        assert_eq!(f.current(), (1, 1));
        assert_eq!(f.grid.block().text(), f.text());
        assert_eq!(f.host.silent.get(), 0);
        assert!(f
            .host
            .view
            .buffer()
            .unwrap()
            .iter_at_offset(f.text().find("20").unwrap() as i32)
            .has_tag(&f.host.hidden));
        assert_eq!(
            f.host.view.buffer().unwrap().cursor_position() as usize,
            f.text().find("x |").unwrap() + 1
        );
    });
}
#[test]
fn test_typing_burst_is_one_undo_step_and_undo_redo_refocus_the_cell() {
    support::run(|| {
        use gdk::keys::constants as k;
        let f = Fixture::table();
        f.focus(2, 0);
        for c in ["a", "b", "c"] {
            f.type_text(c);
        }
        assert_eq!(f.host.actions.get(), 1);
        assert_eq!(f.host.ends.get(), 0);
        assert!(f.grid.key(k::z, gdk::ModifierType::CONTROL_MASK));
        assert_eq!(f.host.ends.get(), 1);
        assert!(f.grid.key(k::y, gdk::ModifierType::CONTROL_MASK));
        assert!(f.grid.key(
            k::Z,
            gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK
        ));
        assert_eq!(*f.host.history.borrow(), [false, true, true]);
        let b = f.host.view.buffer().unwrap();
        b.set_text(TABLE);
        let block = TableBlock::new(2, &b, 0, TABLE.len() as i32, TABLE);
        let restored = f.tables.build(block).unwrap();
        support::pump(10);
        assert_eq!(
            restored.focus_state().map(|s| (s.row, s.column)),
            Some((2, 0))
        );
    });
}
#[test]
fn test_external_buffer_edit_refreshes_the_grid() {
    support::run(|| {
        let f = Fixture::table();
        let b = f.host.view.buffer().unwrap();
        let start = TABLE.find('3').unwrap() as i32;
        b.delete(
            &mut b.iter_at_offset(start),
            &mut b.iter_at_offset(start + 1),
        );
        b.insert(&mut b.iter_at_offset(start), "three");
        let text = f.text();
        let v = f
            .tables
            .build(TableBlock::new(1, &b, 0, text.len() as i32, &text))
            .unwrap();
        while v.fill_slice() {}
        assert_eq!(v.cell(2, 0).unwrap().label.text(), "three");
    });
}
#[test]
fn test_keyboard_navigation_between_cells_rows_and_out_of_the_table() {
    support::run(|| {
        use gdk::keys::constants as k;
        let text = format!("before\n\n{TABLE}\nafter\n");
        let f = Fixture::new(&text, 8, 8 + TABLE.len(), true);
        f.focus(0, 0);
        for (key, want) in [
            (k::Tab, (0, 1)),
            (k::Tab, (1, 0)),
            (k::ISO_Left_Tab, (0, 1)),
            (k::Return, (1, 1)),
            (k::Down, (2, 1)),
            (k::Up, (1, 1)),
            (k::Right, (2, 0)),
            (k::Left, (1, 1)),
        ] {
            f.key(key);
            assert_eq!(f.current(), want);
        }
        let b = f.grid.editor().unwrap().buffer().unwrap();
        b.place_cursor(&b.start_iter());
        f.key(k::Left);
        assert_eq!(f.current(), (1, 0));
        assert!(f.grid.key(k::Return, gdk::ModifierType::SHIFT_MASK));
        f.type_text("x");
        assert!(f.text().contains("| 1<br>x | 2 |"));
        assert_eq!(
            b.text(&b.start_iter(), &b.end_iter(), true).unwrap(),
            "1\nx"
        );
        f.focus(0, 0);
        f.key(k::Up);
        assert!(f.host.view.is_focus());
        assert_eq!(f.host.view.buffer().unwrap().cursor_position(), 7);
        f.focus(2, 1);
        f.key(k::Down);
        assert!(f.host.view.is_focus());
        assert_eq!(
            f.host.view.buffer().unwrap().cursor_position() as usize,
            f.text().find("\nafter").unwrap()
        );
        f.focus(1, 1);
        f.key(k::Escape);
        assert!(f.host.view.is_focus());
    });
}
#[test]
fn test_tab_and_enter_at_the_end_add_rows_and_backspace_is_harmless() {
    support::run(|| {
        use gdk::keys::constants as k;
        let text = format!("{TABLE}\nend\n");
        let f = Fixture::new(&text, 0, TABLE.len(), true);
        f.focus(2, 1);
        f.key(k::Tab);
        assert_eq!(f.current(), (3, 0));
        assert_eq!(f.text(), format!("{TABLE}|  |  |\n\nend\n"));
        f.key(k::BackSpace);
        f.key(k::BackSpace);
        assert_eq!(f.text(), format!("{TABLE}|  |  |\n\nend\n"));
        f.key(k::Right);
        f.key(k::Return);
        assert_eq!(f.current(), (4, 1));
        assert_eq!(f.text().matches("|  |  |").count(), 2);
    });
}
#[test]
fn test_structure_menu_actions_are_single_undo_steps_without_rebuilds() {
    support::run(|| {
        let f = Fixture::table();
        f.focus(1, 0);
        let editor = f.grid.editor().unwrap().text_view().clone();
        for (axis, action, index, expected) in [
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
        ] {
            let before = f.host.actions.get();
            f.grid.act(axis, action, index);
            assert_eq!(f.text(), expected);
            assert_eq!(f.host.actions.get(), before + 1);
            assert_eq!(f.host.actions.get(), f.host.ends.get());
            assert_eq!(f.grid.editor().unwrap().text_view(), &editor);
            let model = f.grid.model();
            for (r, row) in model.rows.iter().enumerate() {
                for (c, source) in row.cells.iter().enumerate() {
                    assert_eq!(f.grid.cell(r, c).unwrap().source(), *source);
                }
            }
            assert!(f.grid.focused());
        }
    });
}
#[test]
fn test_menus_disable_impossible_actions_and_keep_a_valid_table() {
    support::run(|| {
        let text = "| A | B |\n| --- | --- |\n| only | row |\n";
        let f = Fixture::new(text, 0, text.len(), true);
        let sensitivity = |axis, index, name: &str| {
            let menu = f.grid.open_menu(axis, index);
            support::pump(5);
            let list = menu.child().unwrap().downcast::<gtk::Box>().unwrap();
            let button = list
                .children()
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::Button>().ok())
                .find(|b| {
                    b.accessible()
                        .unwrap()
                        .property::<Option<String>>("accessible-name")
                        .as_deref()
                        == Some(name)
                })
                .unwrap();
            let enabled = button.is_sensitive();
            menu.popdown();
            support::pump(5);
            enabled
        };
        assert!(!sensitivity("row", 1, "Delete row"));
        assert!(!sensitivity("row", 1, "Move down"));
        assert!(sensitivity("row", 1, "Move up"));
        assert!(!sensitivity("row", 0, "Delete row"));
        assert!(!sensitivity("column", 0, "Delete column"));
        assert!(!sensitivity("column", 0, "Move left"));
        assert!(sensitivity("column", 0, "Move right"));
        f.grid.act("column", "after", 1);
        assert!(sensitivity("column", 2, "Delete column"));
        f.grid.act("column", "delete", 2);
        assert_eq!(f.text(), text);
    });
}
#[test]
fn test_hover_handles_appear_and_fade_out_with_reduced_motion() {
    support::run(|| {
        let f = Fixture::table();
        assert!(f.grid.handles().iter().all(|w| !w.is_visible()));
        f.grid.hovered(Some((1, 1)));
        assert!(f
            .grid
            .handles()
            .iter()
            .all(|w| w.is_visible() && w.opacity() == 1.));
        f.grid.clear_hover();
        assert!(f.grid.handles().iter().all(|w| !w.is_visible()));
        f.focus(2, 0);
        assert!(f.grid.handles()[..2].iter().all(|w| w.is_visible()));
        assert!(f.grid.handles()[2..].iter().all(|w| !w.is_visible()));
    });
}
#[test]
fn test_add_bars_append_a_row_and_a_column() {
    support::run(|| {
        let f = Fixture::table();
        f.grid.handles()[3]
            .clone()
            .downcast::<gtk::Button>()
            .unwrap()
            .clicked();
        assert_eq!(f.current(), (3, 0));
        f.grid.handles()[2]
            .clone()
            .downcast::<gtk::Button>()
            .unwrap()
            .clicked();
        assert_eq!(f.current(), (0, 2));
        assert_eq!(
            f.text(),
            "| A | B |  |\n| --- | ---: | --- |\n| 1 | 2 |  |\n| 3 | 4 |  |\n|  |  |  |\n"
        );
    });
}
#[test]
fn test_click_on_rendered_cell_puts_the_caret_in_its_source() {
    support::run(|| {
        let text = "| **bold** word | b |\n| --- | --- |\n| x | y |\n";
        let f = Fixture::new(text, 0, text.len(), true);
        let cell = f.grid.cell(0, 0).unwrap();
        let layout = cell.label.layout().unwrap();
        let (ox, oy) = cell.label.layout_offsets();
        let rect = layout.index_to_pos("bold w".len() as i32);
        let caret = cell.index_at(
            (ox + rect.x() / 1024 + 1) as f64,
            (oy + rect.y() / 1024 + 2) as f64,
        );
        f.grid.focus_cell(0, 0, Some(caret), None);
        assert_eq!(
            f.grid.editor().unwrap().buffer().unwrap().cursor_position(),
            "**bold** w".len() as i32
        );
    });
}
#[test]
fn test_down_from_the_last_block_creates_a_paragraph_after_the_table() {
    support::run(|| {
        let text = format!("top\n\n{TABLE}");
        let f = Fixture::new(&text, 5, text.len(), true);
        f.focus(2, 0);
        f.key(gdk::keys::constants::Down);
        assert_eq!(f.text(), format!("{text}\n"));
        let b = f.host.view.buffer().unwrap();
        assert_eq!(b.cursor_position(), b.char_count());
        assert!(f.host.view.is_focus());
    });
}
#[test]
fn test_main_caret_entering_from_above_or_below_lands_in_a_cell() {
    support::run(|| {
        let source = format!("top\n\n{TABLE}\nend\n");
        let f = Fixture::new(&source, 5, 5 + TABLE.len(), true);
        for (outside, row) in [(1, 0), (source.find("end").unwrap() as i32, 2)] {
            f.host.view.grab_focus();
            f.tables.caret(None, outside);
            f.tables.caret(Some(1), 5);
            assert_eq!(f.current(), (row, 0));
        }
    });
}
#[test]
fn test_read_only_preview_keeps_plain_cells() {
    support::run(|| {
        let f = Fixture::new(TABLE, 0, TABLE.len(), false);
        assert!(f.grid.editor().is_none());
        assert!(f.grid.handles().is_empty());
        assert_eq!(f.grid.cell(1, 0).unwrap().label.text(), "1");
        f.focus(0, 0);
        assert!(f.grid.focus_state().is_none());
    });
}
#[test]
fn test_two_hundred_rows_build_in_bounded_slices_and_type_fast() {
    support::run(|| {
        let head="| A | B | C | D | E | F | G | H |\n| --- | --- | --- | --- | --- | --- | --- | --- |\n";
        let rows = (0..200)
            .map(|i| format!("| row {i} | **bold** {i} | `code` | a | b | c | d | e |\n"))
            .collect::<String>();
        let source = format!("{head}{rows}");
        let f = Fixture::new_mode(&source, 0, source.len(), true, false);
        assert_eq!(f.grid.built_rows(), 1);
        while f.grid.fill_visible_slice(140..160) {}
        assert_eq!(f.grid.built_rows(), 21);
        while f.grid.fill_slice() {}
        assert_eq!(f.grid.row_count(), 201);
        f.focus(150, 1);
        let start = Instant::now();
        f.type_text("quick");
        let ms = start.elapsed().as_secs_f64() * 1000.;
        println!(
            "table edit_ms={ms:.3} max_build_slice_ms={:.3}",
            f.grid.max_slice_time().as_secs_f64() * 1000.
        );
        assert!(ms < 8.);
        assert!(f.grid.max_slice_time().as_millis() < 4);
        assert!(f.text().contains("| row 149 | **bold** 149quick |"));
        let weak = f.grid.widget().downgrade();
        let grid = f.grid.clone();
        f.tables.remove(1);
        assert!(grid.widget().in_destruction() || !grid.widget().is_visible());
        assert_eq!(f.tables.views().len(), 0);
        assert!(weak.upgrade().is_none());
    });
}
#[test]
fn test_ctrl_a_selects_only_the_cell_and_copy_is_plain_text() {
    support::run(|| {
        let f = Fixture::table();
        f.focus(1, 0);
        let editor = f.grid.editor().unwrap();
        assert!(!f
            .grid
            .key(gdk::keys::constants::a, gdk::ModifierType::CONTROL_MASK));
        editor.emit_by_name::<()>("select-all", &[&true]);
        let b = editor.buffer().unwrap();
        let (a, z) = b.selection_bounds().unwrap();
        assert_eq!((a.offset(), z.offset()), (0, 1));
        assert!(!f.host.view.buffer().unwrap().has_selection());
        editor.emit_by_name::<()>("copy-clipboard", &[]);
        assert_eq!(
            gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD)
                .wait_for_text()
                .unwrap(),
            "1"
        );
    });
}
#[test]
fn test_malformed_tables_render_and_edit_without_errors() {
    support::run(|| {
        let text = "| a | b | c |\n| --- | --- | --- |\n| short |\n| 1 | `2|3` | 4 | 5 |\n";
        let f = Fixture::new(text, 0, text.len(), true);
        assert_eq!(f.grid.column_count(), 3);
        for (c, source) in ["short", "", ""].iter().enumerate() {
            assert_eq!(f.grid.cell(1, c).unwrap().source(), *source);
        }
        for (c, source) in ["1", "`2", "3`"].iter().enumerate() {
            assert_eq!(f.grid.cell(2, c).unwrap().source(), *source);
        }
        f.focus(1, 2);
        f.type_text("z");
        assert!(f.text().contains("| short |  | z |\n"));
        assert!(f.text().contains("| 1 | `2|3` | 4 | 5 |"));
        let wide = format!(
            "| {} |\n|{}\n",
            (0..40)
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(" | "),
            " --- |".repeat(40)
        );
        let f = Fixture::new(&wide, 0, wide.len(), true);
        assert_eq!(f.grid.column_count(), mdview_cells_count(&wide));
    });
}
fn mdview_cells_count(text: &str) -> usize {
    impro_markdown::mdview::_cells(text.lines().nth(1).unwrap()).len()
}
#[test]
fn test_in_place_row_edits_keep_the_spacing_of_a_fresh_render() {
    support::run(|| {
        let f = Fixture::table();
        f.focus(2, 1);
        f.key(gdk::keys::constants::Tab);
        f.key(gdk::keys::constants::Return);
        support::pump(50);
        let edited = f.grid.widget().preferred_height_for_width(700).1;
        let source = f.text();
        let fresh = Fixture::new(&source, 0, source.len(), true);
        assert_eq!(
            fresh.grid.widget().preferred_height_for_width(700).1,
            edited
        );
        let b = f.host.view.buffer().unwrap();
        b.tag_table().unwrap().foreach(|tag| {
            if tag.property::<bool>("pixels-below-lines-set") {
                assert_eq!(tag.property::<i32>("pixels-below-lines"), 0);
            }
        });
    });
}
#[test]
fn test_no_idle_sources_after_burst_and_removal() {
    support::run(|| {
        let f = Fixture::table();
        f.focus(1, 0);
        f.type_text("x");
        assert!(f.tables.bursting());
        f.tables.end_burst();
        assert!(!f.tables.bursting());
        let cell = f.grid.cell(1, 0).unwrap().widget;
        f.tables.remove(1);
        support::pump(20);
        assert_eq!(
            cell.ref_count(),
            1,
            "cell signal handlers must not retain their widget"
        );
        assert!(f.tables.views().is_empty());
        assert!(!impro_ui::motion::clock_attached());
    });
}
