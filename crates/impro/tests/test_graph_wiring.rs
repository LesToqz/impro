mod common;
use common::*;
use gtk::prelude::*;
use impro_graph::{view::testing, GraphType};

#[test]
fn test_graph_focus_shortcuts_and_hotkeys_without_webkit() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("a.md", "[[b]]\n");
        h.write("b.md", "[[a]]\n");
        assert!(h.w.graph_views().is_empty());
        assert!(key(&h.w, "<Control><Alt>g"));
        let view = h.w.graph_views()[0].clone();
        poll(|| view.summary().nodes >= 2);
        assert!(!h.w.terminal_focused());
        assert_eq!(view.graph_type(), GraphType::Global);
        assert!(key(&h.w, "<Control><Shift><Alt>g"));
        assert_eq!(view.graph_type(), GraphType::Local);
        h.w.editor.open_file(tab.path());
        h.w.open_graph();
        assert_eq!(view.graph_type(), GraphType::Global);
        assert_eq!(h.w.terminal_right.graph_count(), 1);
        assert_eq!(h.w.terminal_right.terminal_count(), 0);
        assert!(h
            .w
            .hotkeys()
            .contains(&("Graph view: Open graph view".into(), "Ctrl+Alt+G".into())));
    });
}
#[test]
fn test_right_dock_toggle_opens_a_graph_view_by_default() {
    run(|| {
        let h = Harness::new();
        h.w.titlebar.dock_button().clicked();
        assert_eq!(h.w.terminal_right.graph_count(), 1);
        assert_eq!(h.w.terminal_right.terminal_count(), 0);
        h.w.titlebar.dock_button().clicked();
        assert!(!h.w.terminal_right.is_visible());
        h.w.titlebar.dock_button().clicked();
        assert_eq!(h.w.terminal_right.pane_count(), 1);
    });
}
#[test]
fn test_graph_live_text_saved_closed_paths_and_tree_changes() {
    run(|| {
        let h = Harness::new();
        let tab = h.open("a.md", "[[b]]\n");
        h.write("b.md", "\n");
        let view = h.w.open_graph().unwrap();
        poll(|| testing::node_ids(&view).contains(&"a.md".to_string()));
        tab.buffer.set_text("[[missing]]\n");
        poll(|| {
            testing::node_ids(&view)
                .iter()
                .any(|id| id.contains("missing"))
        });
        h.w.editor.set_dialog_hook(Some(std::rc::Rc::new(|_, _| {
            impro_editor::Response::Discard
        })));
        assert!(result(|cb| h.w.editor.close_current(cb)));
        poll(|| {
            !testing::node_ids(&view)
                .iter()
                .any(|id| id.contains("missing"))
        });
        h.w.editor.open_file(tab.path());
        wait_path(&h.w, &tab.path());
        let tab = h.w.editor.current_tab().unwrap();
        tab.buffer.set_text("[[missing]]\n");
        assert!(result(|cb| h.w.editor.save_current(cb)));
        h.write("fresh.md", "[[b]]\n");
        h.w.explorer.refresh();
        poll(|| testing::node_ids(&view).contains(&"fresh.md".to_string()));
        h.w.explorer
            .rename(&tab.path(), "renamed.md", Box::new(|_| {}));
        poll(|| testing::node_ids(&view).contains(&"renamed.md".to_string()));
        h.w.explorer.delete(&tab.path(), true, Box::new(|_| {}));
        poll(|| !testing::node_ids(&view).contains(&"renamed.md".to_string()));
    });
}
#[test]
fn test_graph_settings_and_defaults_apply_to_open_and_new_panes() {
    run(|| {
        let h = Harness::new();
        h.w.set_setting("graph_default_type", "note".into(), false)
            .unwrap();
        let view = h.w.open_graph().unwrap();
        assert_eq!(view.graph_type(), GraphType::Note);
        h.w.set_setting("graph_default_type", "global".into(), false)
            .unwrap();
        assert_eq!(view.graph_type(), GraphType::Note);
        h.w.add_dock_pane(&h.w.terminal_bottom, "graph");
        let graphs = h.w.graph_views();
        assert_eq!(graphs[1].graph_type(), GraphType::Global);
        h.w.set_setting("graph", serde_json::json!({"showTags":false}), false)
            .unwrap();
        for graph in graphs {
            assert!(!graph.prefs().show_tags);
        }
    });
}
#[test]
fn test_feature_panes_close_processes_and_restore_in_both_docks() {
    run(|| {
        let h = Harness::new();
        h.w.open_graph();
        h.w.add_dock_pane(&h.w.terminal_bottom, "graph");
        h.w.set_setting("feature_graph", false.into(), false)
            .unwrap();
        assert!(h.w.graph_views().is_empty());
        assert_eq!(h.w.terminal_right.graph_count(), 0);
        h.w.set_setting("feature_graph", true.into(), false)
            .unwrap();
        assert_eq!(h.w.graph_views().len(), 2);
        assert_eq!(h.w.terminal_right.graph_count(), 1);
    });
}
#[test]
fn test_error_notice_once_and_cleanup() {
    run(|| {
        let h = Harness::new();
        let view = h.w.open_graph().unwrap();
        for error in ["first", "first", "second"] {
            h.w.view_error("Graph view", error);
        }
        assert_eq!(h.w.overlay.notices().len(), 1);
        h.w.terminal_right.close_kind("graph");
        assert!(h.w.graph_views().is_empty());
        let before = view.frame_stats().frames;
        drain();
        assert_eq!(view.frame_stats().frames, before);
    });
}

#[test]
fn test_graph_create_action_validates_and_opens_project_note() {
    run(|| {
        let h = Harness::new();
        h.w.create_graph_note("Created");
        let path = h.dir.path().join("Created.md");
        wait_path(&h.w, &path);
        assert!(path.is_file());
        h.w.create_graph_note("../escape");
        assert!(h
            .w
            .overlay
            .notices()
            .iter()
            .any(|n| n.text() == "Graph note names must be simple file names."));
    });
}
