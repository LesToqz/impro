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
