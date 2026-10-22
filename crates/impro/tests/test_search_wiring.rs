mod common;
use common::*;
use gtk::prelude::*;

#[test]
fn test_unsaved_edits_are_searched() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("a.py", "original\n");
        tab.buffer.set_text("zebra unsaved\nneedle edited\n");
        h.w.explorer.focus_search(Some("zebra"));
        let search = h.w.explorer.search();
        poll(|| !search.is_searching() && search.results().total_matches > 0);
        assert_eq!(search.results().groups[0].path, tab.path());
        assert_eq!(std::fs::read_to_string(tab.path()).unwrap(), "original\n");
    });
}
#[test]
fn test_activating_result_opens_tab_and_selects_exact_match() {
    run(|| {
        let h = Harness::new();
        let path = h.write("a.py", "alpha needle one\nline two\n  needle two\n");
        h.w.explorer.emit_by_name::<()>(
            "search-result-activated",
            &[&path.to_str().unwrap(), &2i32, &2i32, &6i32],
        );
        wait_path(&h.w, &path);
        poll(|| !h.w.editor.is_busy());
        let tab = h.w.editor.current_tab().unwrap();
        let (a, b) = tab.buffer.selection_bounds().unwrap();
        assert_eq!(tab.buffer.text(&a, &b, true).unwrap(), "needle");
        assert_eq!(a.line(), 2);
        assert_eq!(a.line_offset(), 2);
        assert_eq!(h.w.window.focused_widget(), Some(tab.view.clone().upcast()));
    });
}
#[test]
fn test_markdown_preview_switches_to_source() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("notes.md", "# Title\n\nsome needle here\n");
        assert_eq!(tab.markdown_mode(), Some("preview"));
        h.w.explorer.emit_by_name::<()>(
            "search-result-activated",
            &[&tab.path().to_str().unwrap(), &2i32, &5i32, &6i32],
        );
        poll(|| tab.markdown_mode() == Some("source"));
        let (a, b) = tab.buffer.selection_bounds().unwrap();
        assert_eq!(tab.buffer.text(&a, &b, true).unwrap(), "needle");
    });
}
#[test]
fn test_open_file_at_clamps() {
    run(|| {
        let h = Harness::new();
        let path = h.write("a.py", "alpha needle one\nline two\n");
        h.w.editor.open_file_at(&path, 999, 999, 999);
        wait_path(&h.w, &path);
        poll(|| !h.w.editor.is_busy());
        let tab = h.w.editor.current_tab().unwrap();
        assert!(!tab.buffer.has_selection());
        h.w.editor.open_file_at(&path, 0, 6, 999);
        let (a, b) = tab.buffer.selection_bounds().unwrap();
        assert_eq!(tab.buffer.text(&a, &b, true).unwrap(), "needle one");
        h.w.editor.open_file_at(&path, -4, -4, -1);
        assert_eq!(tab.cursor(), 0);
    });
}
#[test]
fn test_ctrl_shift_f_restores_terminal_fullscreen_and_view_menu_item() {
    run(|| {
        let h = Harness::new();
        h.w.toggle_terminal_fullscreen();
        assert!(h.w.is_terminal_fullscreen());
        assert!(key(&h.w, "<Control><Shift>f"));
        assert!(!h.w.is_terminal_fullscreen());
        assert!(h.w.explorer.is_visible());
        assert_eq!(
            h.w.window.focused_widget(),
            Some(h.w.explorer.search().entry().upcast())
        );
    });
}
#[test]
fn test_ctrl_shift_f_focuses_entry_shows_explorer_and_prefills_selection() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("a.py", "alpha needle one\n");
        tab.buffer.select_range(
            &tab.buffer.iter_at_offset(6),
            &tab.buffer.iter_at_offset(12),
        );
        h.w.toggle_explorer();
        h.w.search_in_files();
        assert!(h.w.explorer.is_visible());
        assert_eq!(h.w.explorer.search().query(), "needle");
        tab.buffer.place_cursor(&tab.buffer.start_iter());
        h.w.explorer.search().entry().set_text("keep");
        h.w.search_in_files();
        assert_eq!(h.w.explorer.search().query(), "keep");
    });
}
#[test]
fn test_ctrl_f_is_still_in_file_find() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("a.py", "needle\n");
        tab.view.grab_focus();
        let e = event(&h.w.window, "<Control>f");
        let e: &gdk::Event = &e;
        h.w.window.emit_by_name::<bool>("key-press-event", &[e]);
        assert!(h.w.editor.search_bar().is_search_mode());
        assert_eq!(
            h.w.window.focused_widget(),
            Some(h.w.editor.search_entry().upcast())
        );
    });
}
#[test]
fn test_result_is_scrolled_into_view_near_the_middle() {
    run(|| {
        let h = Harness::new();
        let text = (0..400).map(|i| format!("line {i}\n")).collect::<String>();
        let path = h.write("big.py", &text);
        h.w.explorer.emit_by_name::<()>(
            "search-result-activated",
            &[&path.to_str().unwrap(), &300i32, &0i32, &4i32],
        );
        wait_path(&h.w, &path);
        let tab = h.w.editor.current_tab().unwrap();
        poll(|| {
            let rect = tab.view.visible_rect();
            let line = tab.buffer.iter_at_line(300);
            let (y, _) = tab.view.line_yrange(&line);
            let ratio = (y - rect.y()) as f64 / rect.height() as f64;
            (0.3..0.7).contains(&ratio)
        });
    });
}
#[test]
fn test_open_folder_changes_search_root() {
    run(|| {
        let h = Harness::new();
        h.write("old.py", "needle\n");
        h.w.explorer.focus_search(Some("needle"));
        let other = tempfile::tempdir().unwrap();
        let path = other.path().join("only.py");
        std::fs::write(&path, "needle only\n").unwrap();
        h.w.set_root(other.path());
        poll(|| h.w.root() == other.path());
        let search = h.w.explorer.search();
        poll(|| {
            !search.is_searching()
                && search.results().groups.len() == 1
                && search.results().groups[0].path == path
        });
    });
}
#[test]
fn test_ctrl_zero_resets_to_configured_editor_size_and_settings_update_it() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("a.py", "");
        tab.view.grab_focus();
        h.w.set_setting("editor_font_size", 15.into(), false)
            .unwrap();
        h.w.editor.zoom(2);
        h.w.editor.handle_key(&event(&h.w.window, "<Control>0"));
        assert_eq!(h.w.editor.font_size(), 15);
        h.w.set_setting("editor_font_size", 13.into(), true)
            .unwrap();
        h.w.editor.zoom(-1);
        h.w.editor.handle_key(&event(&h.w.window, "<Control>0"));
        assert_eq!(h.w.editor.font_size(), 13);
    });
}

#[test]
fn test_search_box_is_first_under_header() {
    run(|| {
        let h = Harness::new();
        let children = h.w.explorer.children();
        assert_eq!(children.len(), 2);
        assert!(children[0]
            .style_context()
            .has_class("impro-explorer-header"));
        let split = children[1].clone().downcast::<gtk::Paned>().unwrap();
        assert_eq!(split.child1(), Some(h.w.explorer.search().upcast()));
    });
}
#[test]
fn test_typing_shows_results_and_escape_restores_tree() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("a.py", "needle\n");
        h.w.search_in_files();
        let search = h.w.explorer.search();
        search.entry().set_text("needle");
        poll(|| !search.is_searching() && search.results().total_matches == 1);
        let e = event(&h.w.window, "Escape");
        let e: &gdk::Event = &e;
        h.w.window.emit_by_name::<bool>("key-press-event", &[e]);
        assert!(search.query().is_empty());
        assert_eq!(h.w.window.focused_widget(), Some(tab.view.clone().upcast()));
    });
}
#[test]
fn test_escape_without_tabs_focuses_tree() {
    run(|| {
        let h = Harness::new();
        h.write("a.py", "needle\n");
        h.w.explorer.focus_search(Some("needle"));
        let e = event(&h.w.window, "Escape");
        let e: &gdk::Event = &e;
        h.w.window.emit_by_name::<bool>("key-press-event", &[e]);
        assert_eq!(
            h.w.window.focused_widget(),
            Some(h.w.explorer.tree().upcast())
        );
    });
}
#[test]
fn test_active_tab_group_first_and_switching_resorts() {
    run(|| {
        let h = Harness::new();
        let a = h.open("a.py", "needle\n");
        let b = h.open("b.py", "needle\n");
        h.w.explorer.focus_search(Some("needle"));
        let search = h.w.explorer.search();
        poll(|| !search.is_searching() && search.results().groups.len() == 2);
        assert_eq!(search.results().groups[0].path, b.path());
        h.w.editor.open_file(a.path());
        poll(|| search.results().groups[0].path == a.path());
    });
}
#[test]
fn test_current_file_signal() {
    run(|| {
        let h = Harness::new();
        let a = h.open("a.py", "");
        let b = h.open("b.py", "");
        h.w.editor.open_file(a.path());
        assert_eq!(h.w.explorer.active_file(), Some(a.path()));
        assert!(result(|cb| h.w.editor.close_current(cb)));
        assert_eq!(h.w.explorer.active_file(), Some(b.path()));
        assert!(result(|cb| h.w.editor.close_current(cb)));
        assert_eq!(h.w.explorer.active_file(), None);
    });
}
#[test]
fn test_rename_and_delete_refresh_results() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("a.py", "needle\n");
        let b = h.write("b.py", "needle\n");
        h.w.explorer.focus_search(Some("needle"));
        let search = h.w.explorer.search();
        poll(|| !search.is_searching() && search.results().groups.len() == 2);
        h.w.explorer.rename(&b, "c.py", Box::new(|_| {}));
        poll(|| {
            !search.is_searching()
                && search
                    .results()
                    .groups
                    .iter()
                    .any(|g| g.path.ends_with("c.py"))
        });
        h.w.explorer.delete(&tab.path(), true, Box::new(|_| {}));
        poll(|| !search.is_searching() && search.results().groups.len() == 1);
        assert!(tab.deleted());
    });
}
#[test]
fn test_explorer_font_size_reaches_result_rows() {
    run(|| {
        let h = Harness::new();
        h.w.set_setting("explorer_font_size", 14.into(), false)
            .unwrap();
        let search = h.w.explorer.search();
        for column in search.tree().columns() {
            for cell in column.cells() {
                if cell.find_property("size-points").is_some() {
                    assert_eq!(cell.property::<f64>("size-points"), 14.0);
                }
            }
        }
    });
}

#[test]
fn test_explorer_shortcuts_do_not_act_from_search_entry_or_results() {
    run(|| {
        let h = Harness::new();
        let path = h.write("keep.py", "needle\n");
        h.w.explorer.refresh();
        poll(|| !h.w.explorer.is_busy());
        h.w.explorer.set_current_file(Some(&path));
        h.w.search_in_files();
        let search = h.w.explorer.search();
        search.entry().set_text("needle");
        poll(|| !search.is_searching() && search.results().total_matches == 1);
        let windows = gtk::Window::list_toplevels().len();
        for target in [
            search.entry().upcast::<gtk::Widget>(),
            search.tree().upcast(),
        ] {
            target.grab_focus();
            search.entry().set_position(-1);
            for accelerator in ["Delete", "F2", "F5", "<Control><Shift>n"] {
                let key = event(&h.w.window, accelerator);
                assert!(!h.w.explorer.on_key(&key));
                let event: &gdk::Event = &key;
                h.w.window.emit_by_name::<bool>("key-press-event", &[event]);
            }
            assert_eq!(gtk::Window::list_toplevels().len(), windows);
            assert_eq!(search.query(), "needle");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "needle\n");
        }
    });
}
