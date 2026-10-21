mod common;
use common::*;
use gdk::keys::constants as k;
use gtk::prelude::*;
use impro_editor::*;
use std::{cell::RefCell, fs, os::unix::fs::PermissionsExt, rc::Rc};
#[test]
fn test_reject_binary_and_invalid_utf8() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("file");
    for content in [b"a\0b".as_slice(), b"\xff"] {
        fs::write(&p, content).unwrap();
        assert!(read_file(&p)
            .unwrap_err()
            .to_string()
            .contains("binary or non-UTF-8"));
    }
}
#[test]
fn test_size_limit() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("file");
    fs::File::create(&p)
        .unwrap()
        .set_len(MAX_FILE_SIZE as u64 + 1)
        .unwrap();
    assert!(read_file(&p).unwrap_err().to_string().contains("5 MB"));
}
#[test]
fn test_crlf_round_trip_and_mode() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("file");
    fs::write(&p, b"first\r\nsecond\r\n").unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o751)).unwrap();
    let doc = read_file(&p).unwrap();
    assert_eq!(doc.text, "first\nsecond\n");
    assert_eq!(doc.ending, "CRLF");
    atomic_write(&p, &doc.text, doc.ending).unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"first\r\nsecond\r\n");
    assert_eq!(fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o751);
}
#[test]
fn test_reject_nonregular_without_blocking() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("fifo");
    let name = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(read_file(p)
        .unwrap_err()
        .to_string()
        .contains("non-regular"));
}
#[test]
fn test_no_added_newline() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("file");
    atomic_write(&p, "no newline", "LF").unwrap();
    assert_eq!(fs::read(p).unwrap(), b"no newline");
}
#[test]
fn test_atomic_failure_preserves_original_and_removes_temp() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("folder");
    fs::create_dir(&p).unwrap();
    assert!(atomic_write(&p, "replacement", "LF").is_err());
    assert!(p.is_dir());
    assert_eq!(fs::read_dir(d.path()).unwrap().count(), 1);
}
#[test]
fn test_indent_helpers() {
    assert_eq!(leading_whitespace(" \t code"), " \t ");
    assert_eq!(unindent("  x", 4), "x");
    assert_eq!(unindent("\tx", 4), "x");
    let s = "  a\n\n    b";
    assert_eq!(toggle_comments(&toggle_comments(s)), s);
}
#[test]
fn test_empty_open_switch_duplicate() {
    run(|| {
        let h = Harness::new();
        assert_eq!(
            h.panel.stack().visible_child_name().as_deref(),
            Some("empty")
        );
        let first = h.open("file.py", "first\nsecond");
        h.open("other.py", "other");
        h.panel.open_file(first.path());
        assert!(Rc::ptr_eq(&h.panel.current_tab().unwrap(), &first));
        assert_eq!(h.panel.tabs().len(), 2);
        assert!(h.panel.notebook().is_scrollable());
        assert_eq!(
            first.header.widget.tooltip_text().as_deref(),
            first.path().to_str()
        );
    });
}
#[test]
fn test_invalid_missing_directory_binary_size() {
    run(|| {
        let h = Harness::new();
        for p in [h.dir.path().join("missing"), h.dir.path().to_path_buf()] {
            h.panel.open_file(p);
            poll(|| !h.panel.is_busy());
            assert!(h.panel.last_error().is_some());
        }
        for b in [b"\0".as_slice(), b"\xff"] {
            let p = h.dir.path().join("bad");
            fs::write(&p, b).unwrap();
            h.panel.open_file(p);
            poll(|| !h.panel.is_busy());
        }
        let p = h.dir.path().join("big");
        fs::File::create(&p)
            .unwrap()
            .set_len(MAX_FILE_SIZE as u64 + 1)
            .unwrap();
        h.panel.open_file(p);
        poll(|| !h.panel.is_busy());
        assert!(h.panel.tabs().is_empty());
    });
}
#[test]
fn test_status_follows_tab_switch() {
    run(|| {
        let h = Harness::new();
        let t = h.open("file.py", "first\nsecond");
        t.buffer.place_cursor(&t.buffer.end_iter());
        h.open("other.py", "one\r\ntwo");
        h.panel.notebook().set_current_page(Some(0));
        assert_eq!(
            h.panel.status().text(),
            "Ln 2, Col 7  |  UTF-8  |  LF  |  Python"
        );
        h.panel.notebook().set_current_page(Some(1));
        assert!(h.panel.status().text().contains("CRLF"));
    });
}
#[test]
fn test_grouped_undo_and_redo_branch() {
    run(|| {
        let h = Harness::new();
        let t = h.open("file.py", "first\nsecond");
        t.buffer.begin_user_action();
        t.buffer.insert_at_cursor("one");
        t.buffer.insert_at_cursor("two");
        t.buffer.end_user_action();
        assert_eq!(t.text(), "first\nsecondonetwo");
        t.undo(false);
        assert_eq!(t.text(), "first\nsecond");
        assert!(!h.panel.has_unsaved());
        replace(&t, "new branch");
        t.undo(true);
        assert_eq!(t.text(), "new branch");
    });
}
#[test]
fn test_dirty_save_and_undo_to_saved_state() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "first");
        replace(&t, "modified");
        assert!(h.panel.has_unsaved());
        assert!(t.header.label.text().starts_with("● "));
        assert!(h.save());
        assert_eq!(fs::read_to_string(t.path()).unwrap(), "modified");
        assert!(!h.panel.has_unsaved());
        t.undo(false);
        assert!(h.panel.has_unsaved());
        t.undo(true);
        assert!(!h.panel.has_unsaved());
    });
}
#[test]
fn test_save_all_and_nothing_to_save() {
    run(|| {
        let h = Harness::new();
        assert!(h.save());
        assert!(h.save_all());
        let a = h.open("a.py", "a");
        let b = h.open("b.py", "b");
        replace(&a, "one");
        replace(&b, "two");
        assert!(h.save_all());
        assert_eq!(fs::read_to_string(a.path()).unwrap(), "one");
        assert_eq!(fs::read_to_string(b.path()).unwrap(), "two");
        assert!(!h.panel.has_unsaved());
    });
}
#[test]
fn test_save_all_continues_after_failure() {
    run(|| {
        let h = Harness::new();
        let a = h.open("nested/a.py", "a");
        let b = h.open("b.py", "b");
        replace(&a, "one");
        replace(&b, "two");
        fs::remove_file(a.path()).unwrap();
        fs::remove_dir(a.path().parent().unwrap()).unwrap();
        h.answer(Response::Overwrite);
        assert!(!h.save_all());
        assert_eq!(fs::read_to_string(b.path()).unwrap(), "two");
        assert!(a.buffer.is_modified());
    });
}
#[test]
fn test_crlf_status_and_save() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "one\r\ntwo");
        assert!(h.panel.status().text().contains("CRLF"));
        replace(&t, "one\ntwo\nthree");
        assert!(h.save());
        assert_eq!(fs::read(t.path()).unwrap(), b"one\r\ntwo\r\nthree");
        t.buffer.place_cursor(&t.buffer.iter_at_offset(5));
        assert!(h.panel.status().text().contains("Ln 2, Col 2"));
    });
}
#[test]
fn test_rename_file_and_folder_boundary() {
    run(|| {
        let h = Harness::new();
        let t = h.open("file.py", "x");
        let new = h.dir.path().join("renamed.py");
        fs::rename(t.path(), &new).unwrap();
        h.panel.on_path_renamed(t.path(), &new);
        assert_eq!(t.path(), new);
        let child = h.open("folder/nested.py", "n");
        let unrelated = h.open("folder-other.py", "s");
        let old = h.dir.path().join("folder");
        let new = h.dir.path().join("moved");
        fs::rename(&old, &new).unwrap();
        h.panel.on_path_renamed(&old, &new);
        assert_eq!(child.path(), new.join("nested.py"));
        assert_eq!(unrelated.path(), h.dir.path().join("folder-other.py"));
    });
}
#[test]
fn test_delete_file_recreate_and_folder() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "first");
        fs::remove_file(t.path()).unwrap();
        h.panel.on_path_deleted(t.path());
        assert!(t.deleted());
        assert!(t.header.label.text().contains("(deleted)"));
        assert!(h.save());
        assert_eq!(fs::read_to_string(t.path()).unwrap(), "first");
        assert!(!t.deleted());
        let c = h.open("dir/nested.py", "nested");
        h.panel.on_path_deleted(h.dir.path().join("dir"));
        assert!(c.deleted());
        assert!(!t.deleted());
    });
}
#[test]
fn test_close_cancel_discard_save() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "a");
        replace(&t, "edited");
        assert!(!h.close(&t));
        h.answer(Response::Save);
        assert!(h.close(&t));
        assert_eq!(fs::read_to_string(t.path()).unwrap(), "edited");
        assert_eq!(
            h.panel.stack().visible_child_name().as_deref(),
            Some("empty")
        );
        let t = h.open("b.py", "b");
        replace(&t, "discarded");
        h.answer(Response::Discard);
        assert!(h.close(&t));
        assert_eq!(fs::read_to_string(t.path()).unwrap(), "b");
    });
}
#[test]
fn test_external_modification_prompt() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "original");
        let time = fs::metadata(t.path()).unwrap().modified().unwrap();
        replace(&t, "mine");
        fs::write(t.path(), "theirs").unwrap();
        set_path_time(&t.path(), time + std::time::Duration::from_secs(2));
        assert!(!h.save());
        assert_eq!(fs::read_to_string(t.path()).unwrap(), "theirs");
        h.answer(Response::Overwrite);
        assert!(h.save());
        assert_eq!(fs::read_to_string(t.path()).unwrap(), "mine");
    });
}
#[test]
fn test_edit_keys_undo_redo() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "  hello");
        key(&h.panel, k::Return, false, false);
        assert_eq!(t.text(), "  hello\n  ");
        key(&h.panel, k::Tab, false, false);
        assert_eq!(t.text(), "  hello\n      ");
        key(&h.panel, k::ISO_Left_Tab, false, true);
        assert_eq!(t.text(), "  hello\n  ");
        key(&h.panel, k::z, true, false);
        assert_eq!(t.text(), "  hello\n      ");
        key(&h.panel, k::Z, true, true);
        assert_eq!(t.text(), "  hello\n  ");
    });
}
#[test]
fn test_comment_selection_and_unknown_extension() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "first\nsecond");
        t.buffer
            .select_range(&t.buffer.start_iter(), &t.buffer.end_iter());
        key(&h.panel, k::slash, true, false);
        assert_eq!(t.text(), "# first\n# second");
        t.buffer
            .select_range(&t.buffer.start_iter(), &t.buffer.end_iter());
        key(&h.panel, k::slash, true, false);
        assert_eq!(t.text(), "first\nsecond");
        let txt = h.open("a.txt", "plain");
        key(&h.panel, k::slash, true, false);
        assert_eq!(txt.text(), "plain");
    });
}
#[test]
fn test_tab_switch_close_find_and_search() {
    run(|| {
        let h = Harness::new();
        let a = h.open("a.py", "first\nsecond");
        h.open("b.py", "b");
        key(&h.panel, k::Tab, true, false);
        assert!(Rc::ptr_eq(&h.panel.current_tab().unwrap(), &a));
        key(&h.panel, k::Tab, true, true);
        key(&h.panel, k::Page_Up, true, false);
        key(&h.panel, k::f, true, false);
        assert!(h.panel.search_bar().is_search_mode());
        h.panel.search_entry().set_text("FIRST");
        h.panel.find(false, false);
        assert_eq!(selected(&a), "first");
        h.panel.find(true, false);
        assert!(a.buffer.has_selection());
        h.panel.search_entry().set_text("absent");
        h.panel.find(false, false);
        assert!(h.panel.search_entry().style_context().has_class("error"));
        key(&h.panel, k::w, true, false);
        assert_eq!(h.panel.tabs().len(), 1);
    });
}
#[test]
fn test_open_file_at_selects_clamps_and_reports() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "first\nsecond");
        h.panel.open_file_at(t.path(), 1, 2, 3);
        assert_eq!(selected(&t), "con");
        assert!(h.panel.status().text().starts_with("Ln 2, Col 3"));
        h.panel.open_file_at(t.path(), 0, 3, 50);
        assert_eq!(selected(&t), "st");
        h.panel.open_file_at(t.path(), 99, 99, 99);
        assert!(!t.buffer.has_selection());
        assert_eq!(h.panel.tabs().len(), 1);
    });
}
#[test]
fn test_current_file_signal_buffer_text_and_selected_text() {
    run(|| {
        let h = Harness::new();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s = seen.clone();
        h.panel
            .connect_current_file_changed(move |_, p| s.borrow_mut().push(p));
        let a = h.open("a.py", "first");
        let b = h.open("b.py", "b");
        h.panel.notebook().set_current_page(Some(0));
        assert!(h.close(&a));
        assert!(h.close(&b));
        assert_eq!(
            *seen.borrow(),
            vec![
                Some(a.path()),
                Some(b.path()),
                Some(a.path()),
                Some(b.path()),
                None
            ]
        );
        let t = h.open("c.py", "text");
        replace(&t, "edited text");
        assert_eq!(
            h.panel.buffer_text(t.path()).as_deref(),
            Some("edited text")
        );
        t.buffer
            .select_range(&t.buffer.start_iter(), &t.buffer.iter_at_offset(6));
        assert_eq!(h.panel.selected_text(), "edited");
        h.panel.on_path_deleted(t.path());
        assert!(h.panel.buffer_text(t.path()).is_none());
    });
}
#[test]
fn test_ctrl_zero_uses_default_size_and_ctrl_shift_f_is_not_in_file_find() {
    run(|| {
        let h = Harness::new();
        h.open("a.py", "a");
        h.panel.set_default_font_size(15);
        key(&h.panel, k::_0, true, false);
        assert_eq!(h.panel.font_size(), 15);
        key(&h.panel, k::plus, true, false);
        key(&h.panel, k::_0, true, false);
        assert_eq!(h.panel.font_size(), 15);
        h.panel.set_default_font_size(1000);
        assert_eq!(h.panel.default_font_size(), 32);
        assert!(!key(&h.panel, k::F, true, true));
        assert!(!h.panel.search_bar().is_search_mode());
    });
}
#[test]
fn test_only_markdown_defaults_live_without_toggle() {
    run(|| {
        let h = Harness::new();
        let plain = h.open("a.py", "x");
        assert!(plain.markdown_mode().is_none());
        for name in ["notes.md", "README.MD", "doc.MarkDown"] {
            let t = h.open(name, "# Title\n\n**body**");
            assert_eq!(t.markdown_mode(), Some("preview"));
            assert!(t.view.is_editable());
            assert_eq!(t.chrome.width(), 0);
            assert_eq!(t.markdown_location(), "standalone");
        }
        assert!(h.panel.set_markdown_mode("invalid", None).is_err());
    });
}
#[test]
fn test_default_source_and_per_tab_modes() {
    run(|| {
        let h = Harness::new();
        h.panel.set_md_live_preview(false);
        let a = h.open("a.md", "# A");
        let b = h.open("b.md", "# B");
        assert_eq!(b.markdown_mode(), Some("source"));
        h.panel
            .set_markdown_mode("preview", Some(&a.path()))
            .unwrap();
        assert_eq!(a.markdown_mode(), Some("preview"));
        assert_eq!(b.markdown_mode(), Some("source"));
        assert_eq!(h.panel.current_path(), Some(b.path()));
        h.panel.set_md_live_preview(true);
        assert_eq!(b.markdown_mode(), Some("source"));
    });
}
#[test]
fn test_mode_keeps_caret_history_and_source() {
    run(|| {
        let h = Harness::new();
        let text = "# Title\n\n**body**";
        let t = h.open("a.md", text);
        t.buffer.place_cursor(&t.buffer.iter_at_offset(5));
        let count = t.history_len();
        h.panel.set_markdown_mode("source", None).unwrap();
        assert_eq!(t.cursor(), 5);
        assert_eq!(t.text(), text);
        assert_eq!(t.history_len(), count);
        assert!(!t.buffer.is_modified());
        h.panel.set_markdown_mode("preview", None).unwrap();
        assert_eq!(t.cursor(), 5);
        assert_eq!(t.text(), text);
    });
}
#[test]
fn test_find_selection_and_open_at_preserve_search_contract() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.md", "# Title\n\n**body**");
        h.panel.search_entry().set_text("body");
        h.panel.find(false, false);
        assert_eq!(h.panel.selected_text(), "body");
        h.panel.open_file_at(t.path(), 2, 2, 4);
        assert_eq!(t.markdown_mode(), Some("source"));
        assert_eq!(selected(&t), "body");
    });
}
#[test]
fn test_shortcut_status_label_and_zoom() {
    run(|| {
        let h = Harness::new();
        h.open("a.md", "# Title");
        assert!(h.panel.status().text().contains("Markdown (Live)"));
        key(&h.panel, k::M, true, true);
        assert_eq!(h.panel.markdown_mode(None), Some("source"));
        key(&h.panel, k::plus, true, false);
        assert_eq!(h.panel.font_size(), 12);
        assert!(h.panel.status().text().contains("Markdown (Source)"));
        assert!(h.panel.status().targets[3]
            .emit_by_name::<bool>("button-press-event", &[&button_event(1)]));
        assert_eq!(h.panel.markdown_mode(None), Some("preview"));
    });
}
#[test]
fn test_scroll_preserved_in_same_view() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.md", &"paragraph\n\n".repeat(400));
        pump(50);
        let adj = t.source_scroll.vadjustment();
        adj.set_value(300.0);
        let before = adj.value();
        t.set_markdown_mode("source");
        pump(30);
        assert!((adj.value() - before).abs() < 1.0);
    });
}
#[test]
fn test_rename_attaches_and_detaches_controller() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", "# Title");
        assert_eq!(t.markdown_mode(), None);
        h.panel.on_path_renamed(t.path(), h.dir.path().join("a.md"));
        assert_eq!(t.markdown_mode(), Some("preview"));
        h.panel
            .on_path_renamed(t.path(), h.dir.path().join("a.log"));
        assert_eq!(t.markdown_mode(), None);
        assert_eq!(t.language(), "Plain Text");
        assert!(t.view.is_editable());
        assert!(t.chrome.width() > 0);
    });
}
#[test]
fn test_large_markdown_opens_source() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.md", &"text\n".repeat(220000));
        assert_eq!(t.markdown_mode(), Some("source"));
        assert!(t.language().contains("disabled above 1 MiB"));
        assert!(t.view.is_editable());
    });
}
#[test]
fn test_close_destroy_cancel_pending_work() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.md", "# Title");
        t.set_markdown_mode("source");
        t.buffer.insert_at_cursor("changed");
        assert!(t.highlighter.pending());
        h.answer(Response::Discard);
        assert!(h.close(&t));
        assert!(t.is_closed());
        assert!(!t.highlighter.pending());
        pump(150);
    });
}
#[test]
fn test_project_note_and_outside_modes() {
    run(|| {
        let h = Harness::new();
        h.panel.set_root(Some(h.dir.path()));
        let project = h.open("README.md", "# Project");
        assert_eq!(project.markdown_location(), "project");
        assert_eq!(project.markdown_mode(), Some("preview"));
        assert!(!project.view.is_editable());
        project.set_markdown_mode("source");
        assert!(project.view.is_editable());
        let note = h.open(".impro/Notes/Note.md", "# Note");
        assert_eq!(note.markdown_location(), "note");
        assert_eq!(note.markdown_mode(), Some("preview"));
        assert!(note.view.is_editable());
        h.panel.set_root(Some(&h.dir.path().join("other")));
        assert_eq!(note.markdown_location(), "outside");
        assert_eq!(note.markdown_mode(), Some("source"));
    });
}
#[test]
fn test_settings_apply_to_existing_and_future_tabs() {
    run(|| {
        let h = Harness::new();
        let a = h.open("a.txt", "a");
        let mut values = impro_core::settings::defaults().clone();
        values.insert("editor_font_size".into(), serde_json::json!(15));
        values.insert("editor_font".into(), serde_json::json!("DejaVu Sans Mono"));
        values.insert("indent_width".into(), serde_json::json!(2));
        values.insert("notes_slash_commands".into(), serde_json::json!(false));
        h.panel.apply_settings(&values);
        assert_eq!(h.panel.font_size(), 15);
        assert_eq!(h.panel.default_font_size(), 15);
        assert_eq!(h.panel.indent_width(), 2);
        assert!(!h.panel.slash_commands());
        let b = h.open("b.txt", "b");
        assert_eq!(
            a.view.tabs().unwrap().tab(0).1,
            b.view.tabs().unwrap().tab(0).1
        );
        assert_eq!(h.panel.font_family().as_deref(), Some("DejaVu Sans Mono"));
    });
}
#[test]
fn test_symlink_atomic_save_preserves_link() {
    use std::os::unix::fs::symlink;
    let d = tempfile::tempdir().unwrap();
    let target = d.path().join("target");
    fs::write(&target, "before").unwrap();
    let link = d.path().join("link");
    symlink("target", &link).unwrap();
    atomic_write(&link, "after", "LF").unwrap();
    assert!(link.is_symlink());
    assert_eq!(fs::read_to_string(target).unwrap(), "after");
}
#[test]
fn test_non_utf8_path_signals_round_trip() {
    run(|| {
        use std::os::unix::ffi::OsStringExt;
        let h = Harness::new();
        let path = h
            .dir
            .path()
            .join(std::ffi::OsString::from_vec(b"name-\xff.py".to_vec()));
        fs::write(&path, "x = 1\n").unwrap();
        let seen = Rc::new(RefCell::new(None));
        let value = seen.clone();
        h.panel.connect_current_file_changed(move |_, path| {
            value.replace(path);
        });
        h.panel.open_file(&path);
        poll(|| !h.panel.is_busy());
        assert_eq!(*seen.borrow(), Some(path.clone()));
        let t = h.panel.current_tab().unwrap();
        t.buffer.insert_at_cursor("# edit");
        assert!(h.save());
        assert!(h.close(&t));
    });
}
#[test]
fn test_symlink_markdown_uses_resolved_project_boundary() {
    run(|| {
        use std::os::unix::fs::symlink;
        let h = Harness::new();
        let outside = tempfile::tempdir().unwrap();
        let path = outside.path().join("outside.md");
        fs::write(&path, "# Outside").unwrap();
        let link = h.dir.path().join("linked.md");
        symlink(&path, &link).unwrap();
        h.panel.set_root(Some(h.dir.path()));
        h.panel.open_file(&link);
        poll(|| !h.panel.is_busy());
        let t = h.panel.tab_for_path(&link).unwrap();
        assert_eq!(t.markdown_location(), "outside");
        assert_eq!(t.markdown_mode(), Some("source"));
        assert!(t.view.is_editable());
        let root_link = outside.path().join("root");
        symlink(h.dir.path(), &root_link).unwrap();
        h.panel.set_root(Some(&root_link));
        poll(|| !h.panel.is_busy());
        assert_eq!(h.panel.root(), Some(h.dir.path().to_path_buf()));
    });
}
#[test]
fn test_quit_cancel_discard_and_save_all() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", "before");
        replace(&t, "after");
        assert!(!result(|cb| h.panel.request_quit(cb)));
        h.answer(Response::Discard);
        assert!(result(|cb| h.panel.request_quit(cb)));
        assert!(h.panel.has_unsaved());
        h.answer(Response::Save);
        assert!(result(|cb| h.panel.request_quit(cb)));
        assert!(!h.panel.has_unsaved());
        assert_eq!(fs::read_to_string(t.path()).unwrap(), "after");
    });
}
