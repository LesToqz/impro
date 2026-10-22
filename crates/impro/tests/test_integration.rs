mod common;
use common::*;
use gtk::prelude::*;
use std::{cell::Cell, rc::Rc};

#[test]
fn test_explorer_activation_edit_and_save() {
    run(|| {
        let h = Harness::new();
        let path = h.write("edit.py", "original\n");
        h.w.explorer
            .emit_by_name::<()>("file-activated", &[&path.to_str().unwrap()]);
        wait_path(&h.w, &path);
        let tab = h.w.editor.current_tab().unwrap();
        tab.buffer.set_text("edited\n");
        assert!(result(|cb| h.w.editor.save_current(cb)));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "edited\n");
        assert!(!h.w.editor.has_unsaved());
    });
}
#[test]
fn test_explorer_mutations_retarget_and_mark_deleted() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("folder/created.py", "text\n");
        let old = tab.path();
        let new = old.with_file_name("renamed.py");
        h.w.explorer.rename(&old, "renamed.py", Box::new(|_| {}));
        poll(|| tab.path() == new);
        tab.buffer.set_text("after rename\n");
        assert!(result(|cb| h.w.editor.save_current(cb)));
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "after rename\n");
        h.w.explorer.delete(&new, true, Box::new(|_| {}));
        poll(|| tab.deleted());
        assert!(tab.buffer.is_modified());
    });
}
#[test]
fn test_hide_show_both_docks_keeps_live_shells() {
    run(|| {
        let h = Harness::new();
        for right in [false, true] {
            let pane = h.shell(right);
            let pid = pane.pid();
            let panel = if right {
                &h.w.terminal_right
            } else {
                &h.w.terminal_bottom
            };
            h.w.hide_dock(panel);
            h.w.show_dock(panel);
            assert_eq!(panel.panes()[0].id, pane.id);
            assert_eq!(panel.panes()[0].pid(), pid);
            pane.terminal
                .as_ref()
                .unwrap()
                .feed_child(b"printf '%s%s\\n' 'integ_' 'ok'\n");
            poll(|| {
                pane.terminal
                    .as_ref()
                    .unwrap()
                    .text_format(impro_vte::Format::Text)
                    .is_some_and(|text| text.lines().any(|s| s == "integ_ok"))
            });
        }
    });
}
#[test]
fn test_split_targets_focused_dock_and_defaults_to_bottom() {
    run(|| {
        let h = Harness::new();
        let right = h.shell(true);
        right.terminal.as_ref().unwrap().grab_focus();
        h.w.split_terminal();
        assert_eq!(h.w.terminal_right.pane_count(), 2);
        let tab = h.open("a.py", "");
        tab.view.grab_focus();
        h.w.split_terminal();
        assert_eq!(h.w.terminal_bottom.pane_count(), 2);
    });
}
#[test]
fn test_destroy_cleans_both_docks() {
    run(|| {
        let h = Harness::new();
        let bottom = h.shell(false);
        let right = h.shell(true);
        let pids = [bottom.pid().unwrap(), right.pid().unwrap()];
        unsafe {
            h.w.window.destroy();
        }
        poll(|| {
            pids.iter()
                .all(|pid| !std::path::Path::new(&format!("/proc/{pid}")).exists())
        });
    });
}
#[test]
fn test_unsaved_quit_decisions() {
    run(|| {
        for response in [
            impro_editor::Response::Cancel,
            impro_editor::Response::Save,
            impro_editor::Response::Discard,
        ] {
            let h = Harness::new();
            let tab = h.open("quit.py", "original\n");
            tab.buffer.set_text("unsaved\n");
            h.w.editor
                .set_dialog_hook(Some(Rc::new(move |_, _| response)));
            let destroyed = Rc::new(Cell::new(false));
            let seen = destroyed.clone();
            h.w.window.connect_destroy(move |_| seen.set(true));
            h.w.request_quit();
            if response == impro_editor::Response::Cancel {
                assert!(!destroyed.get());
                assert!(h.w.editor.has_unsaved());
            } else {
                poll(|| destroyed.get());
                assert_eq!(
                    std::fs::read_to_string(tab.path()).unwrap(),
                    if response == impro_editor::Response::Save {
                        "unsaved\n"
                    } else {
                        "original\n"
                    }
                );
            }
        }
    });
}
#[test]
fn test_save_from_explorer_focus_writes_once_and_clears_dirty_marker() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("saved.py", "original\n");
        tab.buffer.set_text("saved from explorer\n");
        h.w.explorer.focus_tree();
        let count = Rc::new(Cell::new(0));
        let seen = count.clone();
        h.w.editor
            .connect_file_saved(move |_, _| seen.set(seen.get() + 1));
        assert!(key(&h.w, "<Control>s"));
        poll(|| !h.w.editor.has_unsaved());
        assert_eq!(count.get(), 1);
        assert_eq!(
            std::fs::read_to_string(tab.path()).unwrap(),
            "saved from explorer\n"
        );
    });
}
#[test]
fn test_terminal_save_key_is_forwarded_but_file_menu_still_saves() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("save.py", "original\n");
        let pane = h.shell(false);
        tab.buffer.set_text("changed\n");
        let terminal = pane.terminal.as_ref().unwrap();
        terminal.grab_focus();
        assert!(key(&h.w, "<Control>s"));
        assert!(h.w.editor.has_unsaved());
        assert_eq!(std::fs::read_to_string(tab.path()).unwrap(), "original\n");
        h.command("File: Save");
        poll(|| !h.w.editor.has_unsaved());
        assert_eq!(std::fs::read_to_string(tab.path()).unwrap(), "changed\n");
        terminal.feed_child(b"\x11");
    });
}
#[test]
fn test_terminal_focus_keeps_readline_keys_but_ctrl_shift_p_opens_palette() {
    run(|| {
        let h = Harness::new();
        for right in [false, true] {
            let pane = h.shell(right);
            pane.terminal.as_ref().unwrap().grab_focus();
            for accelerator in ["<Control>p", "<Control>o", "F1"] {
                key(&h.w, accelerator);
                assert!(h.w.overlay.prompt().is_none());
                assert!(h.w.settings_dialog().is_none());
            }
            assert!(key(&h.w, "<Control><Shift>p"));
            assert!(h.w.overlay.prompt().is_some());
            h.w.overlay.hide_prompt();
        }
    });
}
#[test]
fn test_live_theme_recolours_real_widgets_and_preserves_shells() {
    run(|| {
        let h = Harness::new();
        let pane = h.shell(false);
        let pid = pane.pid();
        let tab = h.open("theme.rs", "fn main() {}\n");
        for theme in ["impro-dark", "impro-light"] {
            h.w.set_theme(theme, false);
            frame(&h.w.window);
            assert_eq!(pane.pid(), pid);
            assert_eq!(
                h.w.editor.chrome(),
                impro_core::themes::get_theme(Some(theme)).chrome
            );
            assert!(!tab.buffer.is_modified());
        }
    });
}
#[test]
fn test_window_wires_editor_save_and_focus_refresh() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("saved.py", "before\n");
        tab.buffer.set_text("after\n");
        assert!(result(|cb| h.w.editor.save_current(cb)));
        assert!(!tab.buffer.is_modified());
        h.w.window.emit_by_name::<bool>(
            "focus-in-event",
            &[&gdk::Event::new(gdk::EventType::FocusChange)],
        );
        assert_eq!(h.w.explorer.active_file(), Some(tab.path()));
    });
}
