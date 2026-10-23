//! Window-level graph behavior: editor routing, live buffers, local focus and
//! settings written by one process and restored by another.
mod common;

use common::*;
use gtk::prelude::*;
use impro_graph::{view::testing as t, GraphType, GraphView};
use serde_json::json;
use std::{path::Path, process::Command, time::Instant};

fn ready(view: &GraphView) {
    poll(|| view.summary().nodes > 0);
    assert!(t::settle(view, 20_000));
    assert!(t::wait_frames(view, 2, 2_000));
}

fn click(view: &GraphView, id: &str, button: u32, modifiers: gdk::ModifierType) {
    let (x, y, _) = t::node_screen(view, id).expect("file marker");
    t::press(view, x, y, button, modifiers);
    assert!(t::wait_frames(view, 2, 2_000));
    t::release(view, x, y, button, modifiers);
}

#[test]
fn graph_actions_route_to_editor_tabs_and_keep_both_docks() {
    run(|| {
        let h = Harness::new();
        let a = h.open("a.md", "[[b]]\n");
        let b = h.write("b.md", "[[a]]\n");
        let right = h.w.open_graph().expect("right graph");
        assert!(h.w.add_dock_pane(&h.w.terminal_bottom, "graph"));
        h.w.show_dock(&h.w.terminal_bottom);
        let bottom = h.w.graph_views()[1].clone();
        ready(&right);
        ready(&bottom);
        click(&right, "b.md", 1, gdk::ModifierType::empty());
        wait_path(&h.w, &b);
        frame(&h.w.window);
        assert_eq!(h.w.editor.tabs().len(), 2);
        click(&bottom, "a.md", 1, gdk::ModifierType::CONTROL_MASK);
        wait_path(&h.w, &a.path());
        frame(&h.w.window);
        assert_eq!(
            h.w.editor.tabs().len(),
            2,
            "existing editor tabs are reused"
        );
        // The reference host routes every open mode to its shared editor.
        assert!(t::open_context_menu(&right, "b.md"));
        let menu = t::context_menu(&right).expect("file menu");
        let item = menu
            .children()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::MenuItem>().ok())
            .find(|item| item.label().as_deref() == Some("Open in right pane"))
            .expect("right action");
        item.activate();
        wait_path(&h.w, &b);
        frame(&h.w.window);
        assert_eq!(h.w.editor.tabs().len(), 2);
        for view in [&right, &bottom] {
            assert!(view.is_mapped());
            assert!(t::nodes_on_screen(view) > 0);
        }
        assert_eq!(h.w.terminal_right.graph_count(), 1);
        assert_eq!(h.w.terminal_bottom.graph_count(), 1);
    });
}

#[test]
fn editor_buffer_changes_wait_400_ms_and_local_graph_follows_tab_changes() {
    run(|| {
        let h = Harness::new();
        let a = h.open("a.md", "[[b]]\n");
        let b = h.open("b.md", "[[a]]\n");
        h.write("other.md", "unrelated\n");
        h.w.editor.open_file(a.path());
        wait_path(&h.w, &a.path());
        let view = h.w.open_graph().expect("graph");
        ready(&view);
        let start = Instant::now();
        a.buffer.set_text("[[Draft Target]]\n");
        frame(&h.w.window);
        t::pump(200);
        assert!(!t::node_ids(&view).contains(&"?Draft Target".into()));
        poll(|| t::node_ids(&view).contains(&"?Draft Target".into()));
        assert!(
            start.elapsed().as_millis() >= 380,
            "buffer debounce fired early"
        );
        assert!(
            start.elapsed().as_millis() < 2_000,
            "buffer forwarding has extra delay"
        );
        assert_eq!(std::fs::read_to_string(a.path()).unwrap(), "[[b]]\n");
        let local = h.w.open_local_graph().expect("local graph");
        assert_eq!(local.graph_type(), GraphType::Local);
        poll(|| t::node_ids(&local).contains(&"?Draft Target".into()));
        h.w.editor.open_file(b.path());
        wait_path(&h.w, &b.path());
        frame(&h.w.window);
        poll(|| {
            let ids = t::node_ids(&local);
            ids.contains(&"b.md".into()) && !ids.contains(&"?Draft Target".into())
        });
        h.w.editor.open_file(a.path());
        wait_path(&h.w, &a.path());
        frame(&h.w.window);
        poll(|| t::node_ids(&local).contains(&"?Draft Target".into()));
        assert!(!t::node_ids(&local).contains(&"other.md".into()));
    });
}

#[test]
fn graph_preferences_survive_a_fresh_process() {
    let dir = tempfile::tempdir().unwrap();
    for stage in ["save", "restore"] {
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "graph_settings_process", "--nocapture"])
            .env("IMPRO_GRAPH_PROCESS_STAGE", stage)
            .env("IMPRO_GRAPH_PROCESS_ROOT", dir.path())
            .env("XDG_CONFIG_HOME", dir.path().join("config"))
            .output()
            .expect("settings child process");
        assert!(
            result.status.success(),
            "{stage}: {}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn graph_settings_process() {
    let Ok(stage) = std::env::var("IMPRO_GRAPH_PROCESS_STAGE") else {
        return;
    };
    run(move || {
        let root = std::env::var("IMPRO_GRAPH_PROCESS_ROOT").unwrap();
        let root = Path::new(&root);
        let config = root.join("config/impro/settings.json");
        if stage == "save" {
            std::fs::write(root.join("a.md"), "[[b]] #topic\n").unwrap();
            std::fs::write(root.join("b.md"), "[[a]]\n").unwrap();
            let mut values = impro_core::settings::defaults().clone();
            values.insert("reduce_motion".into(), true.into());
            values.insert("graph".into(), json!({"scale":0.04,"close":true}));
            assert!(impro_core::settings::save(&values));
        }
        let app = gtk::Application::new(
            Some("dev.impro.GraphProcessTests"),
            gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(None::<&gio::Cancellable>).unwrap();
        let w = impro::MainWindow::new(&app, root);
        w.show();
        poll(|| w.layout_ready());
        let view = w.open_graph().expect("graph");
        ready(&view);
        let saved_zoom = 0.04 * (120.0f64 / 480.0).exp();
        let expected = if stage == "save" { 0.04 } else { saved_zoom };
        assert!((t::info(&view).zoom - expected).abs() < 1e-9);
        assert!(t::nodes_on_screen(&view) > 0);
        assert!(view.panel().chip_visible());
        assert!(!view.panel_open());
        if stage == "save" {
            let disk = || {
                serde_json::from_slice::<serde_json::Value>(&std::fs::read(&config).unwrap())
                    .unwrap()
            };
            let start = Instant::now();
            view.panel().switch("showTags").unwrap().toggle();
            frame(&w.window);
            t::pump(120);
            assert_ne!(
                disk()["graph"]["showTags"],
                json!(true),
                "disk write must debounce"
            );
            poll(|| disk()["graph"]["showTags"] == true);
            assert!(start.elapsed().as_millis() >= 230);
            assert!(start.elapsed().as_millis() < 2_000);
            // Adding the tag reheats physics for several seconds. Saving a
            // completed zoom must use the camera clock, independently of that.
            assert!(t::info(&view).simulating);
            let zoom_start = Instant::now();
            t::wheel(&view, 180.0, 120.0, -120.0, false);
            frame(&w.window);
            t::pump(600);
            assert_eq!(disk()["graph"]["scale"], json!(0.04));
            assert!(
                t::wait_for(1_300, || disk()["graph"]["scale"]
                    .as_f64()
                    .is_some_and(|scale| (scale - saved_zoom).abs() < 1e-9)),
                "finished camera motion must save before unrelated physics settles"
            );
            assert!(zoom_start.elapsed().as_millis() >= 700);
            assert!(zoom_start.elapsed().as_millis() < 2_000);
        } else {
            assert!(view.prefs().show_tags);
            poll(|| t::node_ids(&view).contains(&"#topic".into()));
        }
        view.shutdown();
        unsafe { w.window.destroy() };
        drain();
    });
}
