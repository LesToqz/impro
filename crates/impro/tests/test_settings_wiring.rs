mod common;
use common::*;
use gtk::prelude::*;
use serde_json::json;

#[test]
fn test_gear_accelerator_lazy_reusable_nonmodal_dialog() {
    run(|| {
        let h = Harness::new();
        assert!(h.w.settings_dialog().is_none());
        h.w.titlebar.settings_button().clicked();
        let dialog = h.w.settings_dialog().unwrap();
        assert!(!dialog.is_modal());
        assert_eq!(dialog.transient_for(), Some(h.w.window.clone().upcast()));
        dialog.hide();
        h.accelerate("<Control>comma");
        assert_eq!(*h.w.settings_dialog().unwrap(), *dialog);
        assert!(dialog.is_visible());
    });
}
#[test]
fn test_settings_shortcut_with_editor_and_both_terminal_docks_focused() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("shortcut.rs", "fn main() {}\n");
        let bottom = h.shell(false);
        let right = h.shell(true);
        for widget in [
            tab.view.clone().upcast::<gtk::Widget>(),
            bottom.terminal.as_ref().unwrap().clone().upcast(),
            right.terminal.as_ref().unwrap().clone().upcast(),
        ] {
            widget.grab_focus();
            assert!(key(&h.w, "<Control>comma"));
            assert!(h.w.settings_dialog().unwrap().is_visible());
            h.w.settings_dialog().unwrap().hide();
        }
    });
}
#[test]
fn test_live_sizes_future_content_reset_and_reopen() {
    run(|| {
        let h = Harness::new();
        h.open("first.rs", "");
        h.w.open_settings(None);
        let dialog = h.w.settings_dialog().unwrap();
        for (key, size) in [
            ("editor_font_size", 18),
            ("explorer_font_size", 16),
            ("terminal_font_size", 17),
        ] {
            let control = dialog
                .control(key)
                .unwrap()
                .downcast::<gtk::Scale>()
                .unwrap();
            control.set_value(size as f64);
        }
        assert_eq!(h.w.editor.font_size(), 18);
        assert_eq!(h.w.editor.default_font_size(), 18);
        assert_eq!(h.w.explorer.font_size(), 16.0);
        h.w.toggle_terminal_right();
        let tab = h.open("next.rs", "");
        frame(&h.w.window);
        assert_eq!(
            tab.view
                .style_context()
                .font(gtk::StateFlags::NORMAL)
                .size()
                / 1024,
            18
        );
        for panel in [&h.w.terminal_bottom, &h.w.terminal_right] {
            for pane in panel.panes() {
                assert_eq!(
                    pane.terminal.as_ref().unwrap().font().unwrap().size() / 1024,
                    17
                );
            }
        }
        impro_settings_ui::flush();
        let stored = impro_core::settings::get_values();
        assert_eq!(stored["editor_font_size"], 18);
        let other = impro::MainWindow::with_values(&h.app, h.dir.path(), stored);
        assert_eq!(other.editor.font_size(), 18);
        unsafe {
            other.window.destroy();
        }
        for key in impro_core::settings::FONT_KEYS {
            dialog.reset_button(key).unwrap().clicked();
        }
        assert_eq!(h.w.editor.font_size(), 11);
        assert_eq!(h.w.explorer.font_size(), 10.0);
    });
}
#[test]
fn test_invalid_saved_sizes_are_safe_before_first_frame() {
    run(|| {
        let mut values = impro_core::settings::defaults().clone();
        values.insert("editor_font_size".into(), json!(9999));
        values.insert("explorer_font_size".into(), json!("bad"));
        values.insert("terminal_font_size".into(), json!(-100));
        let h = Harness::with(values);
        assert_eq!(h.w.editor.font_size(), 32);
        assert_eq!(h.w.explorer.font_size(), 10.0);
        h.w.open_settings(None);
        assert_eq!(
            h.w.settings_dialog().unwrap().values()["terminal_font_size"],
            7
        );
    });
}
#[test]
fn test_terminal_base_font_preserves_zoom_and_commits_gate() {
    run(|| {
        let h = Harness::new();
        let pane = h.shell(false);
        let terminal = pane.terminal.as_ref().unwrap();
        terminal.set_font_scale(1.3);
        let gate = pane.gate.as_ref().unwrap();
        let before = gate.commit_count();
        h.w.set_setting("terminal_font_size", json!(18), false)
            .unwrap();
        poll(|| gate.commit_count() > before);
        assert_eq!(terminal.font_scale(), 1.3);
        assert_eq!(terminal.font().unwrap().size() / 1024, 18);
    });
}
#[test]
fn test_every_setting_changes_the_live_widgets_and_persists() {
    run(|| {
        let h = Harness::new();
        h.open("code.rs", "fn main() {}\n");
        let changes = json!({"line_numbers":false,"highlight_line":false,"md_live_preview":false,"md_readable_width":false,"indent_tabs":true,"indent_width":2,"auto_pair":false,"show_hidden":false,"confirm_delete":false,"delete_mode":"trash","terminal_cursor":"underline","terminal_blink":false,"terminal_scrollback":5000,"explorer_git_colors":false,"explorer_folders_first":false,"explorer_compact":true,"notes_sort":"modified","notes_open_after_create":false,"notes_toolbar":false,"notes_slash_commands":false,"notes_selection_toolbar":false,"terminal_max_panes":2,"terminal_copy_on_select":true,"terminal_bell":true,"graph_default_type":"note","gitgraph_orientation":"vertical","gitgraph_max_commits":100,"gitgraph_show_remotes":false,"gitgraph_show_tags":false,"gitgraph_time_format":"absolute"});
        for (key, value) in changes.as_object().unwrap() {
            h.w.set_setting(key, value.clone(), true).unwrap();
            assert_eq!(h.w.values()[key], *value);
        }
        assert!(!h.w.editor.line_numbers());
        assert!(!h.w.editor.highlight_line());
        assert!(!h.w.editor.auto_pair());
        assert!(h.w.editor.indent_tabs());
        assert_eq!(h.w.editor.indent_width(), 2);
        assert!(!h.w.explorer.show_hidden());
        assert!(!h.w.explorer.confirm_delete());
        assert!(h.w.explorer.compact());
        assert!(!h.w.explorer.folders_first());
        impro_settings_ui::flush();
        let stored = impro_core::settings::get_values();
        for (key, value) in changes.as_object().unwrap() {
            assert_eq!(stored[key], *value, "{key}");
        }
    });
}
#[test]
fn test_failed_save_still_changes_live_and_reused_dialog() {
    run(|| {
        let h = Harness::new();
        let parent = h.dir.path().join("blocked");
        std::fs::write(&parent, "file").unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &parent);
        h.w.set_setting("editor_font_size", json!(19), true)
            .unwrap();
        impro_settings_ui::flush();
        poll(|| {
            h.w.overlay
                .notices()
                .iter()
                .any(|n| n.text().starts_with("Could not save settings to "))
        });
        h.w.open_settings(None);
        assert_eq!(
            h.w.settings_dialog().unwrap().values()["editor_font_size"],
            19
        );
        assert_eq!(h.w.editor.font_size(), 19);
    });
}
#[test]
fn test_hotkeys_page_and_theme_cycle_sync() {
    run(|| {
        let h = Harness::new();
        h.open("hotkeys.rs", "").view.grab_focus();
        assert!(key(&h.w, "F1"));
        let dialog = h.w.settings_dialog().unwrap();
        assert_eq!(dialog.theme_id(), h.w.theme_id());
        h.accelerate("<Control><Alt>t");
        assert_eq!(dialog.theme_id(), h.w.theme_id());
        h.w.set_setting("reduce_motion", false.into(), false)
            .unwrap();
        h.w.set_setting("reduce_motion", true.into(), false)
            .unwrap();
        assert!(!impro_ui::motion::animations_enabled());
    });
}

#[test]
fn test_font_changes_preserve_real_shell_prompt_in_both_docks() {
    run(|| {
        let h = Harness::new();
        let panes = [h.shell(false), h.shell(true)];
        for pane in &panes {
            stable_prompt(pane);
        }
        for size in [16, 20, 11] {
            h.w.set_setting("terminal_font_size", size.into(), false)
                .unwrap();
            frame(&h.w.window);
            poll(|| {
                panes
                    .iter()
                    .all(|p| p.gate.as_ref().is_none_or(|g| !g.has_pending_sources()))
            });
            for pane in &panes {
                assert_eq!(
                    terminal_text(pane)
                        .lines()
                        .filter(|line| *line == "history-once")
                        .count(),
                    1
                );
                assert!(terminal_text(pane).contains("impro-check> "));
            }
        }
    });
}
