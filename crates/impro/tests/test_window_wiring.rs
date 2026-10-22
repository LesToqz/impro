mod common;
use common::*;
use gtk::prelude::*;

#[test]
fn test_command_list_has_every_menu_action_with_its_accelerator() {
    run(|| {
        let h = Harness::new();
        let commands = h.w.commands();
        for top in h.w.menu_bar.children() {
            let top = top.downcast::<gtk::MenuItem>().unwrap();
            let menu = top.submenu().unwrap().downcast::<gtk::Menu>().unwrap();
            for entry in menu.children() {
                let label = entry.downcast::<gtk::MenuItem>().unwrap().label().unwrap();
                let expected = match label.as_str() {
                    "Graph view" => "Graph view: Open graph view".into(),
                    "Local graph" => "Graph view: Open local graph".into(),
                    "Git graph" => "Git graph: Open git graph".into(),
                    _ => format!("{}: {}", top.label().unwrap(), label.trim_end_matches('…')),
                };
                assert!(commands.iter().any(|c| c.label == expected), "{expected}");
            }
        }
        assert_eq!(
            commands
                .iter()
                .find(|c| c.label == "File: Save")
                .unwrap()
                .hotkey,
            ["Ctrl+S"]
        );
        assert_eq!(
            commands
                .iter()
                .find(|c| c.label == "File: Quick Switcher")
                .unwrap()
                .hotkey,
            ["Ctrl+P", "Ctrl+O"]
        );
        assert!(h.w.hotkeys().contains(&("Undo".into(), "Ctrl+Z".into())));
    });
}
#[test]
fn test_editor_focus_hotkeys_open_prompts_and_escape_restores_focus() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("main.rs", "fn main() {}\n");
        for accelerator in ["<Control>p", "<Control>o", "<Control><Shift>p"] {
            tab.view.grab_focus();
            assert!(key(&h.w, accelerator));
            assert!(h.w.overlay.prompt().is_some());
            assert!(key(&h.w, "Escape"));
            assert!(h.w.overlay.prompt().is_none());
            assert_eq!(h.w.window.focused_widget(), Some(tab.view.clone().upcast()));
        }
    });
}
#[test]
fn test_switcher_lists_project_files_recent_first_and_opens_choice() {
    run(|| {
        let h = Harness::new();
        let a = h.open("a.rs", "fn a() {}\n");
        h.open("b.rs", "fn b() {}\n");
        h.write("nested/c.rs", "");
        h.write("node_modules/skipped.rs", "");
        h.w.open_quick_switcher();
        let switcher = h.w.quick_switcher().unwrap();
        poll(|| switcher.items().len() >= 3);
        assert_eq!(
            switcher.items()[0].data::<std::path::PathBuf>(),
            Some(&h.dir.path().join("b.rs"))
        );
        switcher.set_query("a.rs");
        poll(|| !switcher.search_pending());
        switcher.choose(gdk::ModifierType::empty());
        wait_path(&h.w, &a.path());
        assert!(h.w.overlay.prompt().is_none());
    });
}
#[test]
fn test_palette_runs_chosen_command_and_lists_it_first_next_time() {
    run(|| {
        let h = Harness::new();
        h.w.open_command_palette();
        let palette = h.w.command_palette().unwrap();
        palette.set_query("toggle explorer sidebar");
        palette.choose(gdk::ModifierType::empty());
        assert!(!h.w.explorer.is_visible());
        assert!(h.w.overlay.prompt().is_none());
        h.w.open_command_palette();
        assert_eq!(palette.items()[0].label, "View: Toggle Explorer Sidebar");
    });
}
#[test]
fn test_palette_commands_zoom_and_markdown_toggle() {
    run(|| {
        let h = Harness::new();
        h.open("notes.md", "# Notes\n");
        let mode = h.w.editor.markdown_mode(None);
        h.command("Markdown: Toggle source mode");
        assert_ne!(h.w.editor.markdown_mode(None), mode);
        let size = h.w.editor.font_size();
        h.command("Editor: Zoom in");
        assert_eq!(h.w.editor.font_size(), size + 1);
        h.command("Editor: Reset zoom");
        assert_eq!(h.w.editor.font_size(), h.w.editor.default_font_size());
    });
}
#[test]
fn test_titlebar_toggles_explorer_and_bottom_dock_and_stay_synced() {
    run(|| {
        let h = Harness::new();
        h.w.titlebar.explorer_button().clicked();
        assert!(!h.w.explorer.is_visible());
        assert!(!h.w.titlebar.explorer_button().is_active());
        h.w.toggle_explorer();
        assert!(h.w.titlebar.explorer_button().is_active());
        h.w.titlebar.bottom_dock_button().clicked();
        assert!(!h.w.terminal_bottom.is_visible());
        h.w.toggle_terminal_bottom();
        assert!(h.w.titlebar.bottom_dock_button().is_active());
    });
}
#[test]
fn test_note_toolbar_only_on_notes_and_buttons_insert_markdown() {
    run(|| {
        let h = Harness::new();
        let plain = h.open("main.rs", "");
        assert!(h.w.note_toolbar(&plain).is_none());
        let note = h.open(".impro/Notes/Example.md", "# Example\n\n");
        let toolbar = h.w.note_toolbar(&note).expect("note toolbar");
        frame(&h.w.window);
        assert_eq!(note.source_scroll.margin_top(), toolbar.allocated_height());
        note.buffer.place_cursor(&note.buffer.end_iter());
        toolbar.buttons()["checklist"].clicked();
        assert!(note.text().ends_with("- [ ] "));
        note.undo(false);
        assert_eq!(note.text(), "# Example\n\n");
        h.w.set_setting("notes_toolbar", false.into(), false)
            .unwrap();
        assert!(h.w.note_toolbar(&note).is_none());
        assert_eq!(note.source_scroll.margin_top(), 0);
    });
}
#[test]
fn test_every_theme_parses_with_composed_css_and_overrides() {
    run(|| {
        let h = Harness::new();
        for theme in impro_core::themes::themes() {
            h.w.set_theme(&theme.id, false);
            h.w.set_setting("accent", "#e8762d".into(), false).unwrap();
            h.w.set_setting("ui_font", "DejaVu Sans".into(), false)
                .unwrap();
            h.w.set_setting("editor_font", "DejaVu Sans Mono".into(), false)
                .unwrap();
            assert_eq!(impro_ui::current_theme().unwrap().id, theme.id);
            assert_eq!(impro_ui::current_theme().unwrap().ui["accent"], "#e8762d");
        }
    });
}
#[test]
fn test_shortcut_palette_hide_last_closed_and_fullscreen_sync() {
    run(|| {
        let h = Harness::new();
        h.accelerate("<Control><Alt>grave");
        assert!(h.w.titlebar.dock_button().is_active());
        h.command("View: Terminal on Right");
        assert!(!h.w.titlebar.dock_button().is_active());
        h.w.toggle_terminal_right();
        h.w.terminal_bottom.toggle_maximize();
        assert!(!h.w.titlebar.dock_button().is_active());
        h.w.terminal_bottom.restore();
        assert!(h.w.titlebar.dock_button().is_active());
        h.w.terminal_right.close_current();
        poll(|| !h.w.terminal_right.is_visible());
        assert!(!h.w.titlebar.dock_button().is_active());
    });
}
#[test]
fn test_explorer_notes_toolbar_and_all_dock_toggles_follow_features() {
    run(|| {
        let h = Harness::new();
        let tab = h.open(".impro/Notes/a.md", "# A\n");
        h.w.set_setting("feature_notes", false.into(), false)
            .unwrap();
        assert!(h.w.note_toolbar(&tab).is_none());
        assert!(!h.w.commands().iter().any(|c| c.label.starts_with("Notes:")));
        h.w.set_setting("feature_explorer", false.into(), false)
            .unwrap();
        h.w.window.show_all();
        assert!(!h.w.explorer.is_visible());
        h.w.search_in_files();
        assert!(!h.w.explorer.is_visible());
        for kind in ["terminal", "graph", "gitgraph"] {
            h.w.set_setting(&format!("feature_{kind}"), false.into(), false)
                .unwrap();
        }
        assert!(!h.w.titlebar.dock_button().is_visible());
        assert!(!h.w.titlebar.bottom_dock_button().is_visible());
        h.w.set_setting("feature_terminal", true.into(), false)
            .unwrap();
        assert!(h.w.titlebar.dock_button().is_visible());
    });
}

#[test]
fn test_theme_fallback_and_nonpersisting_switch() {
    run(|| {
        let h = Harness::new();
        h.w.set_theme("impro-dark", true);
        impro_settings_ui::flush();
        h.w.set_theme("impro-light", false);
        assert_eq!(impro_core::settings::get_theme_id(), "impro-dark");
        h.w.set_theme("unknown", false);
        assert_eq!(h.w.theme_id(), impro_core::themes::get_theme(None).id);
    });
}
#[test]
fn test_next_theme_accelerator_cycles_and_wraps() {
    run(|| {
        let h = Harness::new();
        let themes = impro_core::themes::themes();
        h.w.set_theme(&themes[0].id, false);
        for theme in themes.iter().skip(1).chain(themes.first()) {
            h.accelerate("<Control><Alt>t");
            assert_eq!(h.w.theme_id(), theme.id);
        }
    });
}
#[test]
fn test_open_folder_updates_markdown_root_and_live_notes() {
    run(|| {
        let h = Harness::new();
        let old = h.open("guide.md", "# Guide\n");
        let other = tempfile::tempdir().unwrap();
        let path = other.path().join(".impro/Notes/fresh.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# Fresh\n").unwrap();
        h.w.set_root(other.path());
        poll(|| h.w.root() == other.path() && !h.w.editor.is_busy());
        assert_eq!(old.markdown_location(), "outside");
        assert_eq!(old.markdown_mode(), Some("source"));
        h.w.editor.open_file(&path);
        wait_path(&h.w, &path);
        let tab = h.w.editor.current_tab().unwrap();
        assert_eq!(tab.markdown_location(), "note");
        assert!(tab.view.is_editable());
        assert!(h.w.note_toolbar(&tab).is_some());
    });
}
